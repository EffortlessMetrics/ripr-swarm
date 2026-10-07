//! Task-first repair selection services (#6305).
//!
//! The `ripr repair` / `ripr continue` façade decides here; the CLI adapter
//! only parses arguments and renders the outcome. Item discovery reuses the
//! classified-seam inventory and the repair-packet eligibility flip, the
//! start/stop decision goes through the shared #6304 selector, and attempt
//! selection reuses the attempt inventory. Nothing here mints lifecycle,
//! currentness, cage, or receipt facts: the attempt authority keeps those.

use std::path::Path;

use crate::agent::artifact::current_git_head;
use crate::agent::command_specs::repair_start_command_spec;
use crate::agent::loop_commands::{bound_root, shell_arg};
use crate::analysis::canonical_gap::canonical_gap_identity;
use crate::analysis::repair_route::{RepairPacketIneligibility, repair_packet_eligibility};
use crate::app::agent_status::{AgentAttemptStatusReport, build_agent_attempt_status};
use crate::app::repair_attempt::{
    RepairAttemptId, RepairAttemptInventoryEntry, RepairAttemptState,
    inventory_repair_attempts_from, load_repair_attempt_manifest_from,
};
use crate::app::repair_card::command_effect_label;
use crate::config::load_for_root;
use crate::domain::{
    CanonicalNextActionV1, CommandRole, CommandSpec, NextActionClass, NextActionCurrentness,
    NextActionDiffSource, NextActionInput, NextActionProducer, current_command_platform,
    select_canonical_next_action,
};

/// What `ripr repair` without an item resolved to: the single seam to
/// start, the canonical action naming what to do instead, or the honest
/// no-eligible-seam outcome. The last case is producer-owned prose rather
/// than a canonical action: with no suitable subject the selector's
/// anti-invention law leaves nothing to bind.
pub(crate) enum RepairStartDecision {
    Start {
        seam_id: String,
    },
    Action(Box<CanonicalNextActionV1>),
    NoEligible {
        total_seams: usize,
        reasons: Vec<String>,
    },
}

/// Producer-owned facts for one repair-start decision. [`decide_repair_start`]
/// takes these directly so the decision law is unit-testable without a
/// checkout; [`resolve_repair_start`] gathers them from the live authorities.
pub(crate) struct RepairStartFacts<'a> {
    /// The caller-bound root display; the selector copies it verbatim.
    pub(crate) root_display: &'a str,
    /// Eligible seam IDs in inventory order.
    pub(crate) eligible_ids: &'a [String],
    /// Visible seams overall, eligible or not.
    pub(crate) total_seams: usize,
    /// Distinct ineligibility reasons across the visible seams, bounded by
    /// the gatherer. Empty when every visible seam is eligible.
    pub(crate) ineligible_reasons: &'a [String],
    /// The current HEAD, when it could be read.
    pub(crate) head: Option<&'a str>,
    /// The before-phase command for the single candidate, if any. Ignored
    /// unless exactly one seam is eligible.
    pub(crate) start_spec: Option<&'a CommandSpec>,
}

/// Decide whether `ripr repair` without an item may start. Exactly one
/// eligible seam with a fresh HEAD offers the start command; several yield
/// a bounded selection through the shared selector, which owns the class
/// in both cases; zero yields the honest producer-owned outcome below.
pub(crate) fn decide_repair_start(
    facts: &RepairStartFacts<'_>,
) -> Result<RepairStartDecision, String> {
    if facts.eligible_ids.is_empty() {
        return Ok(RepairStartDecision::NoEligible {
            total_seams: facts.total_seams,
            reasons: facts.ineligible_reasons.to_vec(),
        });
    }
    let single = facts.eligible_ids.len() == 1;
    let (item_id, candidates, offered) = if single {
        (facts.eligible_ids[0].clone(), Vec::new(), facts.start_spec)
    } else {
        (String::new(), facts.eligible_ids.to_vec(), None)
    };
    let detail_route = format!("ripr pilot --root {}", shell_arg(facts.root_display));
    let restart_route = format!("ripr repair --root {}", shell_arg(facts.root_display));
    let transition_from = if single {
        "one_eligible_seam"
    } else {
        "several_eligible_seams"
    };
    let head = facts.head.map(str::to_string);
    let action = select_canonical_next_action(&NextActionInput {
        producer: NextActionProducer::RepairStart,
        root: facts.root_display.to_string(),
        // The inventory walks the live tree, so the decision's evidence
        // source is the working tree at the observed HEAD.
        diff_source: NextActionDiffSource::WorkingTree { head: head.clone() },
        item_id,
        check_item: None,
        card_item: None,
        item_candidates: candidates,
        attempts: Vec::new(),
        // The evidence was just gathered at this HEAD: expected and observed
        // bind together, like the card's snapshot. An unreadable HEAD stays
        // unbound and the executable tail refuses honestly.
        currentness: NextActionCurrentness {
            head_expected: head.clone(),
            head_observed: head,
            config_expected: None,
            config_observed: None,
        },
        offered_command: offered,
        // The eligibility flip admitted the single candidate; the before
        // driver adjudicates finally at run time.
        route_admitted: true,
        route_refusal: None,
        missing_input: None,
        platform: current_command_platform(),
        limitation: None,
        limitation_route: None,
        detail_route,
        transition_from: transition_from.to_string(),
        transition_to: offered.map(|_| command_effect_label(CommandRole::RepairStart).to_string()),
        restart_route,
        check_case: None,
        doctor_recovery: None,
        pilot_delegation: None,
        alternatives: Vec::new(),
        limitations: Vec::new(),
    })?;
    if action.action_class() == NextActionClass::RunCommand {
        let seam_id = action
            .subject()
            .item
            .clone()
            .ok_or_else(|| "repair-start decision bound no subject".to_string())?;
        return Ok(RepairStartDecision::Start { seam_id });
    }
    Ok(RepairStartDecision::Action(Box::new(action)))
}

fn ineligibility_label(reason: RepairPacketIneligibility) -> &'static str {
    match reason {
        RepairPacketIneligibility::ClassNotHeadlineEligible => "not a headline repair target",
        RepairPacketIneligibility::CrossLanguageOracleVisibilityUnresolved => {
            "cross-language oracle visibility unresolved"
        }
        RepairPacketIneligibility::CrossLanguageTestTargetUnresolved => {
            "cross-language test target unresolved"
        }
        RepairPacketIneligibility::RouteNotReady => "repair route not ready",
    }
}

/// Gather the live authorities and decide one repair start. A truncated seam
/// inventory fails closed: selecting from a partial list could hide
/// candidates and silently pick first.
pub(crate) fn resolve_repair_start(root: &Path) -> Result<RepairStartDecision, String> {
    let config = load_for_root(root)?;
    let (classified, limit_info) =
        crate::analysis::inventory_classified_seams_at_with_config(root, &config)?;
    if let Some(info) = &limit_info
        && info.analyzed < info.total
    {
        return Err(format!(
            "repair start refused: the seam inventory is truncated ({} of {} seams); rerun with a wider seam budget",
            info.analyzed, info.total
        ));
    }
    let mut eligible_ids = Vec::new();
    let mut reasons: Vec<String> = Vec::new();
    for entry in &classified {
        let eligibility = repair_packet_eligibility(entry);
        if eligibility.eligible() {
            eligible_ids.push(entry.seam.id().as_str().to_string());
        } else if let Some(reason) = eligibility.ineligibility {
            let label = ineligibility_label(reason).to_string();
            if reasons.len() < 3 && !reasons.contains(&label) {
                reasons.push(label);
            }
        }
    }
    let bound = bound_root(&root.to_string_lossy());
    let head = current_git_head(root).ok();
    let spec = if eligible_ids.len() == 1 {
        Some(repair_start_command_spec(&bound, &eligible_ids[0]))
    } else {
        None
    };
    decide_repair_start(&RepairStartFacts {
        root_display: &bound,
        eligible_ids: &eligible_ids,
        total_seams: classified.len(),
        ineligible_reasons: &reasons,
        head: head.as_deref(),
        start_spec: spec.as_ref(),
    })
}

/// What an explicit `ripr repair` item resolved to: the seam to start,
/// or the deliberate message naming why no seam was selected.
pub(crate) enum RepairSubject {
    Seam(String),
    Decision(String),
}

/// Resolve an explicit `ripr repair` item to its seam: an exact seam ID, or
/// a canonical gap ID naming exactly one seam. Anything else is a
/// deliberate no-selection message, never an implicit pick.
pub(crate) fn resolve_repair_subject(root: &Path, item: &str) -> Result<RepairSubject, String> {
    let config = load_for_root(root)?;
    let (classified, limit_info) =
        crate::analysis::inventory_classified_seams_at_with_config(root, &config)?;
    if let Some(info) = &limit_info
        && info.analyzed < info.total
    {
        return Err(format!(
            "repair item refused: the seam inventory is truncated ({} of {} seams); rerun with a wider seam budget",
            info.analyzed, info.total
        ));
    }
    if classified
        .iter()
        .any(|entry| entry.seam.id().as_str() == item)
    {
        return Ok(RepairSubject::Seam(item.to_string()));
    }
    let matches: Vec<&str> = classified
        .iter()
        .filter(|entry| canonical_gap_identity(entry).is_some_and(|identity| identity.id == item))
        .map(|entry| entry.seam.id().as_str())
        .collect();
    match matches.len() {
        1 => Ok(RepairSubject::Seam(matches[0].to_string())),
        0 => Ok(RepairSubject::Decision(unknown_repair_item(root, item))),
        _ => Ok(RepairSubject::Decision(ambiguous_gap_item(
            root, item, &matches,
        ))),
    }
}

fn unknown_repair_item(root: &Path, item: &str) -> String {
    let list = format!(
        "`ripr pilot --root {}` to list current seam IDs",
        shell_arg(&bound_root(&root.to_string_lossy()))
    );
    if item.starts_with("probe:") {
        return format!(
            "repair item `{item}` is a `ripr check` finding ID, not a repair subject; run {list}"
        );
    }
    format!("repair item `{item}` names no seam in the selected root; run {list}")
}

fn ambiguous_gap_item(root: &Path, item: &str, matches: &[&str]) -> String {
    const MAX_LISTED: usize = 8;
    let mut listed: Vec<&str> = matches.iter().take(MAX_LISTED).copied().collect();
    listed.sort_unstable();
    let mut message = format!(
        "repair item `{item}` names {} seams; select one explicitly: {}",
        matches.len(),
        listed.join(", ")
    );
    if matches.len() > MAX_LISTED {
        message.push_str(&format!(" (and {} more)", matches.len() - MAX_LISTED));
    }
    message.push_str(&format!(
        ". Retry: `ripr repair <seam-id> --root {}`",
        shell_arg(&bound_root(&root.to_string_lossy()))
    ));
    message
}

/// What `ripr continue` resolved to: run the after path, report an ended
/// attempt, or ask for a current attempt explicitly.
pub(crate) enum ContinueSelection {
    /// Exactly one current attempt is eligible: run the after path for it.
    /// Awaiting attempts proceed; an explicitly named prepared attempt
    /// proceeds into the shared after selection, which names its honest
    /// restart instead of running.
    Proceed { attempt_id: RepairAttemptId },
    /// The selected attempt already ended: report it, do not run.
    AlreadyComplete {
        report: Box<AgentAttemptStatusReport>,
    },
    /// No current nonterminal attempt exists under the selected root.
    NoneAvailable { prepared: usize, terminal: usize },
    /// Several current attempts exist: select one explicitly.
    Ambiguous { candidates: Vec<String> },
}

/// Resolve the attempt `ripr continue` acts on. An explicit ID loads
/// directly, like the advanced route; without one, exactly one awaiting
/// attempt must be visible — several require selection and a corrupt row
/// refuses rather than letting a partial listing misselect.
pub(crate) fn select_continue_attempt(
    root: &Path,
    attempt_id: Option<&str>,
) -> Result<ContinueSelection, String> {
    if let Some(id) = attempt_id {
        let parsed = RepairAttemptId::parse(id.to_string())
            .map_err(|error| format!("continue --attempt: {error}"))?;
        let manifest = load_repair_attempt_manifest_from(root, None, &parsed)?;
        if matches!(
            manifest.state,
            RepairAttemptState::AwaitingEdit | RepairAttemptState::Prepared
        ) {
            return Ok(ContinueSelection::Proceed { attempt_id: parsed });
        }
        let report = build_agent_attempt_status(root, root, None, &parsed)?;
        return Ok(ContinueSelection::AlreadyComplete {
            report: Box::new(report),
        });
    }
    let mut eligible = Vec::new();
    let mut prepared = 0usize;
    let mut terminal = 0usize;
    for entry in inventory_repair_attempts_from(root, None)? {
        match entry {
            RepairAttemptInventoryEntry::Valid(manifest) => match manifest.state {
                RepairAttemptState::AwaitingEdit => {
                    eligible.push(manifest.repair_attempt_id.as_str().to_string());
                }
                RepairAttemptState::Prepared => prepared += 1,
                RepairAttemptState::ReadyToFinish
                | RepairAttemptState::Stale
                | RepairAttemptState::Incomparable
                | RepairAttemptState::Failed => terminal += 1,
            },
            RepairAttemptInventoryEntry::Invalid { directory, error } => {
                return Err(format!(
                    "continue selection refused: attempt row `{directory}` is invalid: {error}"
                ));
            }
        }
    }
    if eligible.len() > 1 {
        return Ok(ContinueSelection::Ambiguous {
            candidates: eligible,
        });
    }
    match eligible.into_iter().next() {
        Some(id) => Ok(ContinueSelection::Proceed {
            attempt_id: RepairAttemptId::parse(id)
                .map_err(|error| format!("continue selection: {error}"))?,
        }),
        None => Ok(ContinueSelection::NoneAvailable { prepared, terminal }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(
        eligible_ids: &'a [String],
        total_seams: usize,
        reasons: &'a [String],
        start_spec: Option<&'a CommandSpec>,
    ) -> RepairStartFacts<'a> {
        RepairStartFacts {
            root_display: ".",
            eligible_ids,
            total_seams,
            ineligible_reasons: reasons,
            head: Some("head1"),
            start_spec,
        }
    }

    #[test]
    fn decide_zero_visible_seams_yields_setup_outcome() -> Result<(), String> {
        match decide_repair_start(&facts(&[], 0, &[], None))? {
            RepairStartDecision::NoEligible {
                total_seams,
                reasons,
            } => {
                if total_seams != 0 || !reasons.is_empty() {
                    return Err("setup outcome must carry zero seams".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } | RepairStartDecision::Action(_) => {
                Err("zero visible seams must not start or select".to_string())
            }
        }
    }

    #[test]
    fn decide_zero_eligible_seams_yields_limitation_outcome() -> Result<(), String> {
        let reasons = vec!["repair route not ready".to_string()];
        match decide_repair_start(&facts(&[], 2, &reasons, None))? {
            RepairStartDecision::NoEligible {
                total_seams,
                reasons,
            } => {
                if total_seams != 2 || reasons != ["repair route not ready".to_string()] {
                    return Err("limitation outcome must carry the evidence".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } | RepairStartDecision::Action(_) => {
                Err("zero eligible seams must not start or select".to_string())
            }
        }
    }

    /// A single candidate without an offered command is not executable:
    /// the decision degrades to inspect, never to a start.
    #[test]
    fn decide_single_candidate_without_spec_does_not_start() -> Result<(), String> {
        let eligible = vec!["seam:demo".to_string()];
        match decide_repair_start(&facts(&eligible, 1, &[], None))? {
            RepairStartDecision::Action(action) => {
                if action.is_executable() {
                    return Err("an unoffered candidate must not execute".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } => {
                Err("an unoffered candidate must not start".to_string())
            }
            RepairStartDecision::NoEligible { .. } => {
                Err("one eligible seam is not a zero outcome".to_string())
            }
        }
    }
}
