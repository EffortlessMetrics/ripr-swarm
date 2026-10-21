//! Seam-level repair admission for MCP repair readiness (#8717).
//!
//! The repair gate the CLI applies to an explicit repair item
//! (`ripr repair <finding-id>`, `ripr agent repair --seam-id <item>`) is a
//! seam-level decision: [`resolve_repair_subject_in`] names the one seam an
//! item resolves to and applies the shared repair-admission gate
//! (headline-eligible class, resolved cross-language visibility, a ready
//! repair route, brief policy). Finding-level producer facts alone cannot
//! make that decision: the producer may classify a finding as weak while the
//! seam's own tests already grip it, and an item can be "ready" over
//! finding facts while the CLI refuses the very same item.
//!
//! This module runs the shared resolver once per committed finding while the
//! refresh attempt holds its diff-scoped seam inventory, and hands the
//! verdicts to the snapshot projection. The projection treats the verdict as
//! a conjunct of repair readiness: a finding is repair-ready only when its
//! finding-level facts hold *and* the shared validator admits its seam. The
//! MCP adapter keeps no parallel validator and never reads a refusal as
//! readiness: an item the inventory does not resolve, resolves ambiguously,
//! or refuses is not ready, and says why with the CLI's own reason.

use crate::analysis::ClassifiedSeam;
use crate::app::agent_brief::AgentBriefPolicy;
use crate::app::task_first::{RefusalKind, RepairSubjectOutcome, resolve_repair_subject_in};
use crate::domain::Finding;
use std::collections::HashMap;

/// Typed reason: the shared gate names the seam and its existing tests
/// already grip it (`exposed`), so no static gap remains to repair.
pub(crate) const SEAM_ALREADY_GRIPPED: &str = "seam_already_gripped";
/// Typed reason: the shared gate names the seam and refuses it for another
/// reason (route not ready, cross-language limits, policy omission, ...).
pub(crate) const SEAM_GATE_REFUSED: &str = "seam_gate_refused";
/// Typed reason: several seams name the finding; the resolver never picks.
pub(crate) const SEAM_AMBIGUOUS: &str = "seam_ambiguous";
/// Typed reason: no classified seam names the finding.
pub(crate) const SEAM_NOT_FOUND: &str = "seam_not_found";

/// The shared validator's verdict for one committed finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SeamVerdict {
    /// Exactly one seam, admitted by the shared repair gate.
    Admitted,
    /// The finding is not startable; `code` is the typed reason and
    /// `detail` the shared validator's own explanation.
    Refused { code: &'static str, detail: String },
}

/// Per-finding verdicts from the shared validator over one refresh's seam
/// inventory.
#[derive(Clone, Debug, Default)]
pub(crate) struct SeamAdmission {
    evaluated: bool,
    by_finding: HashMap<String, SeamVerdict>,
}

impl SeamAdmission {
    /// No seam evaluation ran: verdict lookups yield `None` and the
    /// projection keeps finding-level readiness only. Production refresh
    /// never uses this; it exists for projections built directly over
    /// fabricated findings in unit tests.
    #[cfg(test)]
    pub(crate) fn unevaluated() -> Self {
        Self::default()
    }

    /// Run the shared repair-subject resolver for every finding over the
    /// refresh's classified seam inventory.
    pub(crate) fn evaluate(
        findings: &[Finding],
        candidate_seams: &[ClassifiedSeam],
        policy: AgentBriefPolicy<'_>,
    ) -> Self {
        let by_finding = findings
            .iter()
            .map(|finding| {
                (
                    finding.id.clone(),
                    verdict_for(resolve_repair_subject_in(
                        candidate_seams,
                        policy,
                        &finding.id,
                    )),
                )
            })
            .collect();
        Self {
            evaluated: true,
            by_finding,
        }
    }

    /// The verdict for one finding id. Once evaluated, a finding the
    /// evaluation did not cover is refused rather than admitted.
    pub(crate) fn verdict(&self, finding_id: &str) -> Option<SeamVerdict> {
        if !self.evaluated {
            return None;
        }
        Some(
            self.by_finding
                .get(finding_id)
                .cloned()
                .unwrap_or_else(|| SeamVerdict::Refused {
                    code: SEAM_NOT_FOUND,
                    detail: "the shared repair validator was not run for this finding".to_string(),
                }),
        )
    }
}

fn verdict_for(outcome: RepairSubjectOutcome) -> SeamVerdict {
    match outcome {
        RepairSubjectOutcome::Seam(_) => SeamVerdict::Admitted,
        RepairSubjectOutcome::Ineligible {
            seam_id,
            reason,
            kind,
        } => SeamVerdict::Refused {
            code: match kind {
                RefusalKind::AlreadyGripped => SEAM_ALREADY_GRIPPED,
                _ => SEAM_GATE_REFUSED,
            },
            detail: format!(
                "the shared repair gate (`ripr repair`) names seam `{seam_id}` for this item and refuses it: {reason}"
            ),
        },
        RepairSubjectOutcome::Ambiguous(seam_ids) => SeamVerdict::Refused {
            code: SEAM_AMBIGUOUS,
            detail: format!(
                "the shared repair gate (`ripr repair`) resolves this item to {} seams and never picks one",
                seam_ids.len()
            ),
        },
        RepairSubjectOutcome::Unknown => SeamVerdict::Refused {
            code: SEAM_NOT_FOUND,
            detail: "the shared repair gate (`ripr repair`) resolves this item to no classified seam in the snapshot's seam inventory".to_string(),
        },
    }
}
