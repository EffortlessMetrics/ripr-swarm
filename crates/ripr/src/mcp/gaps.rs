//! Canonical gap-item projection for the MCP adapter.
//!
//! A [`GapItem`] is the MCP-facing view of one producer-owned
//! [`crate::domain::Finding`]. The projection is built once when a completed
//! snapshot is committed; MCP never re-runs classification, ranking, or
//! evidence production. List responses carry only the small summary document;
//! the complete bounded evidence document is served lazily by
//! `ripr_get_gap` / `ripr://gap/{canonical_id}`.

use super::repair_admission::{SeamAdmission, SeamVerdict};
use crate::domain::Finding;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) const GAP_LIST_SCHEMA_VERSION: &str = "ripr-mcp-gap-list-v4";
pub(crate) const GAP_SCHEMA_VERSION: &str = "ripr-mcp-gap-v1";
pub(crate) const LIMITATION_SCHEMA_VERSION: &str = "ripr-mcp-gap-limitation-v1";
/// Typed kind of a limitation document that names no limiting producer fact.
pub(crate) const LIMITATION_NONE_ESTABLISHED: &str = "no_limitation_established";

/// `ripr_get_gap` / `ripr://gap/{canonical_id}` never authorizes an
/// edit by itself: the readiness block reports the producer repair-readiness
/// facts projected at snapshot commit time, and `ripr_prepare_repair` (#3090)
/// is the only route that binds a repair transaction — and only when every
/// readiness gate is established.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepairFixSite {
    pub(crate) test_name: String,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) oracle: Option<String>,
    pub(crate) oracle_kind: &'static str,
    /// The governed inline `#[cfg(test)]` module the repair is confined to,
    /// when the fix site is the seam's own production file (#5516). `None`
    /// for a test-surface fix site.
    pub(crate) inline_test_module: Option<String>,
}

/// Producer-fact repair readiness for one canonical item, computed once when
/// the snapshot is committed. The evaluation is a fail-closed conjunction of
/// producer facts already on the finding; MCP never upgrades a missing fact
/// and never runs its own classification:
///
/// 1. candidate actionability — the shared `#3281` predicate
///    [`Finding::is_candidate_actionable`];
/// 2. an established discriminator — the canonical gap names a non-empty
///    normalized discriminator, and any producer-named missing discriminator
///    sits on a weakly exposed finding, where the named value is the repair
///    target (#8717). A producer that names its missing discriminators on
///    any other class refuses as `missing_discriminator`; a finding whose
///    canonical gap was withheld behind the finding's own typed static
///    limitation refuses as
///    `static_limitation`; only a producer that names no missing
///    discriminator and populates no canonical gap at all for the language
///    refuses as `discriminator_not_populated_for_language`, so one document
///    cannot contradict its own `discriminator_availability` block (#5268);
/// 3. an established fix site — a strong, high-confidence directly-related
///    test, on a path the shared edit-cage test-surface predicate accepts.
///
/// The first failing gate in this fixed order is the typed reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepairReadiness {
    pub(crate) ready: bool,
    pub(crate) fix_site: Option<RepairFixSite>,
    /// First failing gate (`not_candidate_actionable`, `missing_discriminator`,
    /// `discriminator_not_populated_for_language`, `static_limitation`,
    /// `fix_site_not_established`, `fix_site_not_test_surface`); `None` only
    /// when every gate is established.
    pub(crate) ineligibility: Option<&'static str>,
    /// The shared repair validator's own explanation when the seam-level
    /// admission refused an otherwise finding-ready item (#8717).
    pub(crate) seam_detail: Option<String>,
}

impl RepairReadiness {
    /// Conjoin the shared seam-level repair validator's verdict (#8717).
    /// The validator that backs `ripr repair` is the one authority for why a
    /// seam is refused, so its refusal outranks every finding-level gate
    /// (#10718): the same item must carry the same typed cause here as in
    /// `ripr repair`, never a producer-side `fix_site_not_established` for an
    /// already-gripped, route-not-ready or seamless item. A finding-ready
    /// item becomes not ready, keeping no fix site, when that validator does
    /// not admit its seam. The one exception is a saved-worktree snapshot's
    /// withheld verdict (#7357): it is a snapshot-level fact, not a seam
    /// decision, so a finding-level gate stays the first failing gate there.
    pub(crate) fn with_seam_verdict(mut self, verdict: Option<&SeamVerdict>) -> Self {
        if let Some(SeamVerdict::Refused { code, detail }) = verdict
            && (self.ready || *code != crate::mcp::repair_admission::SEAM_WORKTREE_SOURCE)
        {
            self.ready = false;
            self.fix_site = None;
            self.ineligibility = Some(*code);
            self.seam_detail = Some(detail.clone());
        }
        // A fix site on the seam's own production file is not a test surface,
        // but the shared validator admitted that seam with its one governed
        // inline test module as the edit target (#5210). Promote only when
        // that module's file is exactly the fix site the finding named, so
        // the confinement stated here is the one the cage enforces; any
        // other combination stays refused.
        if let Some(SeamVerdict::AdmittedInline { file, module }) = verdict
            && self.ineligibility == Some("fix_site_not_test_surface")
            && let Some(site) = self.fix_site.as_mut()
            && site.file == *file
        {
            site.inline_test_module = Some(module.clone());
            self.ready = true;
            self.ineligibility = None;
        }
        self
    }

    /// The human sentence for this readiness: the shared validator's own
    /// explanation when it refused, else the finding-level gate sentence.
    pub(crate) fn detail(&self) -> String {
        match &self.seam_detail {
            Some(detail) => detail.clone(),
            None => self.reason().to_string(),
        }
    }

    pub(crate) fn from_finding(finding: &Finding, root: &Path) -> Self {
        if !finding.is_candidate_actionable() {
            return Self {
                ready: false,
                fix_site: None,
                ineligibility: Some("not_candidate_actionable"),
                seam_detail: None,
            };
        }
        let producer_named_missing_discriminator =
            !finding.activation.missing_discriminators.is_empty()
                || finding.missing.iter().any(|entry| {
                    entry.starts_with(crate::domain::MISSING_DISCRIMINATOR_VALUE_PREFIX)
                });
        // A named missing discriminator on a weakly exposed finding is the
        // repair target itself (#8717): the existing test reaches the changed
        // behavior without pinning the named value, and `ripr repair` extends
        // that test. It stays a refusal for every other class, where the
        // missing value does not describe a weak-test repair. Whether the
        // item is actually startable remains the shared seam validator's
        // decision, conjoined after these finding-level gates.
        let named_missing_is_repair_target = producer_named_missing_discriminator
            && finding.class == crate::domain::ExposureClass::WeaklyExposed;
        if producer_named_missing_discriminator && !named_missing_is_repair_target {
            return Self {
                ready: false,
                fix_site: None,
                ineligibility: Some("missing_discriminator"),
                seam_detail: None,
            };
        }
        let discriminator_unpopulated = finding
            .canonical_gap
            .as_ref()
            .is_none_or(|gap| gap.normalized_discriminator.trim().is_empty());
        if discriminator_unpopulated {
            // The refusal must name the actual producer condition: a finding
            // the producer withheld behind its own typed static limitation
            // (Python omits the canonical gap exactly then) refuses as
            // `static_limitation`; only a language producer that does not
            // populate canonical gaps at all refuses as
            // `discriminator_not_populated_for_language` (#5268).
            let ineligibility = if finding.static_limit_kind.is_some() {
                "static_limitation"
            } else {
                "discriminator_not_populated_for_language"
            };
            return Self {
                ready: false,
                fix_site: None,
                ineligibility: Some(ineligibility),
                seam_detail: None,
            };
        }
        // The gate contract is "a strong, high-confidence directly-related
        // test on a shared edit-cage test-surface path", so the path
        // predicate joins the search: a qualifying test-surface candidate
        // later in the list wins over a non-surface candidate earlier.
        let candidates = finding
            .related_tests
            .iter()
            .filter(|test| {
                test.oracle_strength == crate::domain::OracleStrength::Strong
                    && test.relation_confidence == Some(crate::domain::RelationConfidence::High)
            })
            .map(|test| RepairFixSite {
                test_name: test.name.clone(),
                file: crate::output::path::repository_relative_path_text(root, &test.file),
                line: test.line,
                oracle: test.oracle.clone(),
                oracle_kind: test.oracle_kind.as_str(),
                inline_test_module: None,
            })
            .collect::<Vec<_>>();
        let Some(first) = candidates.first().cloned() else {
            return Self {
                ready: false,
                fix_site: None,
                ineligibility: Some("fix_site_not_established"),
                seam_detail: None,
            };
        };
        let Some(fix_site) = candidates
            .into_iter()
            .find(|site| crate::analysis::is_test_surface_path(&site.file))
        else {
            return Self {
                ready: false,
                fix_site: Some(first),
                ineligibility: Some("fix_site_not_test_surface"),
                seam_detail: None,
            };
        };
        Self {
            ready: true,
            fix_site: Some(fix_site),
            ineligibility: None,
            seam_detail: None,
        }
    }

    pub(crate) fn reason(&self) -> &'static str {
        match self.ineligibility {
            Some("not_candidate_actionable") => {
                "the producer did not establish the changed source as candidate-current, so no repair target is actionable (#3281)"
            }
            Some("missing_discriminator") => {
                "the producer did not establish a discriminator for the changed behavior (no normalized discriminator, or a producer-named missing discriminator on a finding that is not weakly exposed)"
            }
            Some("discriminator_not_populated_for_language") => {
                "the producer named no missing discriminator but has not populated a normalized discriminator for this language's findings yet, so the discriminator gate cannot be established from available producer facts; repair stays unavailable until the language producer populates canonical gaps (#5268)"
            }
            Some("static_limitation") => {
                "the producer withheld this finding's canonical gap behind a typed static limitation on the finding itself; the limitation, not the language, explains why no normalized discriminator is established"
            }
            Some("fix_site_not_established") => {
                "no strong, high-confidence directly-related test establishes an exact fix site"
            }
            Some("fix_site_not_test_surface") => {
                "the strongest established fix site is not a test-surface path, and the shared repair gate did not admit it as the seam's one governed inline `#[cfg(test)]` module; only test-surface paths or that admitted module can be the authored edit target"
            }
            Some(
                crate::mcp::repair_admission::SEAM_ALREADY_GRIPPED
                | crate::mcp::repair_admission::SEAM_GATE_REFUSED
                | crate::mcp::repair_admission::SEAM_AMBIGUOUS
                | crate::mcp::repair_admission::SEAM_NOT_FOUND
                | crate::mcp::repair_admission::SEAM_INVENTORY_TRUNCATED
                | crate::mcp::repair_admission::SEAM_WORKTREE_SOURCE,
            ) => {
                "the shared seam-level repair validator behind `ripr repair` does not admit this item"
            }
            Some(_other) => "the producer did not establish every repair-readiness fact",
            None if self
                .fix_site
                .as_ref()
                .is_some_and(|site| site.inline_test_module.is_some()) =>
            {
                "every evaluated producer fact is established: candidate-current source, an established discriminator, and a strong directly-related test fix site inside the seam's own governed inline `#[cfg(test)]` module, which the shared repair gate admitted"
            }
            None => {
                "every evaluated producer fact is established: candidate-current source, an established discriminator, and a strong directly-related test fix site on a test surface"
            }
        }
    }
}

pub(crate) struct GapItem {
    pub(crate) canonical_id: String,
    pub(crate) finding_id: String,
    pub(crate) file: String,
    /// Exposure class wire token (`exposed`, `weakly_exposed`, ...) captured
    /// at projection time, so list filters re-read stored facts (#7464).
    class: String,
    /// Language wire string (`rust`, `python`, ...) or `None` when the
    /// producer populated no language; stored for the same filter route.
    language: Option<String>,
    /// Small deterministic document used by `ripr_list_gaps`; its serialized
    /// byte length is the item's budget payload bytes. It projects stored
    /// class/language facts and the source line alongside the evidence core.
    /// The summary also carries the commit-time repair-readiness projection
    /// (`repair_packet_ready` plus the typed first-failing-gate
    /// `repair_ineligibility`, null when ready), so agents can sort
    /// actionable items without one `ripr_get_gap` call per candidate
    /// (#7464).
    pub(crate) list_summary: Value,
    list_summary_bytes: usize,
    /// Complete bounded evidence document (without the per-request snapshot
    /// binding and links, which are injected at serve time). The repair
    /// projection reads `changed_behavior` and the discriminator from it.
    pub(crate) evidence_core: Value,
    pub(crate) evidence_bytes: usize,
    /// Producer candidate-actionability captured at projection time; the
    /// shared budget maps it to eligibility so MCP never invents its own.
    candidate_actionable: bool,
    /// Deterministic digest of `evidence_core`, bound into the snapshot
    /// identity so an evidence change yields a new snapshot id instead of
    /// serving altered bytes under the old identity.
    pub(crate) evidence_sha256: String,
    /// Producer-fact repair readiness projected at commit time; the evidence
    /// core embeds it so the snapshot identity binds readiness changes too.
    pub(crate) repair_readiness: RepairReadiness,
    /// The producer's own typed static limitation on the finding, captured at
    /// projection time so the limitation read re-uses the one vocabulary
    /// (`describe`/`next_step`) instead of re-deriving it.
    static_limit: Option<crate::domain::StaticLimitKind>,
    /// The producer limit's reason as the CLI and LSP surfaces word it for this
    /// finding's probe family (`describe_for_family`), so the MCP reason cannot
    /// drift from them (#11821).
    static_limit_reason: Option<String>,
}

impl GapItem {
    /// `root` is the analyzed workspace root: item locations render through
    /// the shared finding-location owner (#5996), so an MCP item names the
    /// finding's file with the same workspace-relative string the check,
    /// context and LSP surfaces emit. Related-test and fix-site files keep
    /// the repository-relative renderer (#5254 item 6), which additionally
    /// tolerates verbatim/UNC/case spelling drift.
    #[cfg(test)]
    pub(crate) fn from_finding(finding: &Finding, root: &Path) -> Result<Self, String> {
        Self::from_finding_admitted(finding, root, &SeamAdmission::unevaluated())
    }

    /// [`Self::from_finding`] with the shared seam-level repair validator's
    /// verdict conjoined into the readiness projection (#8717), so the
    /// list summary, the evidence document, the snapshot identity and
    /// `ripr_prepare_repair` all carry the one admission decision.
    pub(crate) fn from_finding_admitted(
        finding: &Finding,
        root: &Path,
        admission: &SeamAdmission,
    ) -> Result<Self, String> {
        let canonical_id = finding
            .canonical_gap
            .as_ref()
            .map(|gap| gap.id.clone())
            .unwrap_or_else(|| finding.id.clone());
        let file = crate::analysis::finding_location_text(root, &finding.probe.location.file);
        let class = finding.class.as_str().to_string();
        let language = finding
            .language
            .as_ref()
            .map(|language| language.as_str().to_string());
        let line = finding.probe.location.line;

        let repair_readiness = RepairReadiness::from_finding(finding, root)
            .with_seam_verdict(admission.verdict(&finding.id).as_ref());
        let list_summary = json!({
            "canonical_id": canonical_id,
            "class": class,
            "language": language,
            "file": file,
            "line": line,
            "repair_packet_ready": repair_readiness.ready,
            "repair_ineligibility": repair_readiness.ineligibility,
        });
        let list_summary_bytes = serialized_bytes(&list_summary)?;

        let evidence_core = gap_evidence_core(finding, &canonical_id, root, &repair_readiness)?;
        let (evidence_bytes, evidence_sha256) = serialized_evidence_identity(&evidence_core)?;
        Ok(Self {
            canonical_id,
            finding_id: finding.id.clone(),
            file,
            class,
            language,
            list_summary,
            list_summary_bytes,
            evidence_core,
            evidence_bytes,
            candidate_actionable: finding.is_candidate_actionable(),
            evidence_sha256,
            repair_readiness,
            static_limit: finding.static_limit_kind,
            static_limit_reason: finding
                .static_limit_kind
                .map(|kind| kind.describe_for_family(&finding.probe.family).into_owned()),
        })
    }

    pub(crate) fn class(&self) -> &str {
        &self.class
    }

    /// The earliest producer fact that prevents a safe repair route for this
    /// item, read from facts committed with the snapshot: no analysis reruns
    /// and no limitation is invented here. Order is upstream first:
    ///
    /// 1. the producer's own typed static limitation on the finding
    ///    (`producer_static_limit`), which outranks every readiness gate
    ///    because it owns why no discriminator or fix site is establishable;
    /// 2. the first failing repair-readiness gate, attributed to the shared
    ///    seam validator (`seam_admission`) or to the finding-level gates
    ///    (`repair_readiness`);
    /// 3. otherwise `no_limitation_established`, an explicit typed answer
    ///    (never `null`) that claims no runtime or sufficiency result.
    pub(crate) fn limitation_document(&self, snapshot_id: &str) -> Value {
        let links = json!({
            "snapshot": format!("ripr://snapshot/{snapshot_id}"),
            "gap": format!("ripr://gap/{}", self.canonical_id),
            "limitation": format!("ripr://gap/{}/limitation", self.canonical_id),
        });
        let restart = json!({
            "snapshot_id": snapshot_id,
            "canonical_id": self.canonical_id,
            "route": "ripr_refresh",
        });
        let guidance = finding_guidance(self);
        let advisory_next_step = guidance
            .get("recommended_next_step")
            .and_then(Value::as_str);
        let refresh_recovery =
            "call ripr_refresh and re-read this limitation against the new snapshot";
        let limitation = if let Some(kind) = self.static_limit {
            let kind_token = serde_json::to_value(kind)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "static_limitation".to_string());
            json!({
                "kind": kind_token,
                "stage": "producer_static_limit",
                "reason": self
                    .static_limit_reason
                    .clone()
                    .unwrap_or_else(|| kind.describe().to_string()),
                "recovery": kind.next_step(),
                "retryable": false,
                "prohibited_claim": "any exposure verdict or repair route stronger than the producer's typed static limitation: no `exposed` grip, no repair-ready item, and no runtime outcome is claimed",
                "restart": restart,
            })
        } else if let Some(code) = self.repair_readiness.ineligibility {
            let seam_owned = self.repair_readiness.seam_detail.is_some();
            // Only a saved-worktree snapshot is retryable without a source
            // change: re-refreshing on committed input re-evaluates it.
            let retryable = code == crate::mcp::repair_admission::SEAM_WORKTREE_SOURCE;
            let recovery = if retryable {
                "call ripr_refresh with the default committed source (omit `source: \"worktree\"`), then re-read this limitation".to_string()
            } else if let Some(step) = advisory_next_step {
                format!("{step}; then {refresh_recovery}")
            } else {
                format!(
                    "read the item's evidence with ripr_get_gap, make the test change it implies, then {refresh_recovery}"
                )
            };
            json!({
                "kind": code,
                "stage": if seam_owned { "seam_admission" } else { "repair_readiness" },
                "reason": self.repair_readiness.detail(),
                "recovery": recovery,
                "retryable": retryable,
                "prohibited_claim": "repair readiness: this item is not repair-ready, so no repair transaction or authored edit is established from it",
                "restart": restart,
            })
        } else {
            json!({
                "kind": LIMITATION_NONE_ESTABLISHED,
                "stage": "none",
                "reason": "no producer limitation is committed for this item: its static limitation is unset and every repair-readiness gate is established",
                "recovery": "none required; read ripr_get_repair_card for the repair route",
                "retryable": false,
                "prohibited_claim": "runtime or sufficiency outcome: an absent static limitation is not a mutation result, a passing test, or proof that a repair is correct",
                "restart": restart,
            })
        };
        json!({
            "schema_version": LIMITATION_SCHEMA_VERSION,
            "snapshot_id": snapshot_id,
            "canonical_id": self.canonical_id,
            "class": self.class,
            "limitation": limitation,
            "links": links,
            "claim_boundary": "The earliest committed producer limitation for one canonical item. Static analysis only: this document does not edit source, execute tests or mutation, or authorize a repair transaction.",
            "limitations": [
                "reads facts committed with the completed snapshot; it never reruns analysis",
                "a missing or limited fact is reported as a typed limitation and never becomes a stronger route",
            ],
        })
    }

    pub(crate) fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Commit-time repair readiness: `true` only when every producer gate
    /// was established. Matches the `readiness.repair_packet_ready` fact the
    /// same item's `ripr_get_gap` document reports (#7464).
    pub(crate) fn repair_ready(&self) -> bool {
        self.repair_readiness.ready
    }

    pub(crate) fn list_summary_bytes(&self) -> usize {
        self.list_summary_bytes
    }

    /// The complete item document for one committed snapshot. The caller
    /// binds the snapshot identity and the resource links; a missing repair
    /// link stays an explicit `null`, never prose invention.
    pub(crate) fn document(&self, snapshot_id: &str) -> Value {
        json!({
            "schema_version": GAP_SCHEMA_VERSION,
            "snapshot_id": snapshot_id,
            "item": serde_json::Value::Object({
                let mut object = self
                    .evidence_core
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                object.insert(
                    "links".to_string(),
                    json!({
                        "snapshot": format!("ripr://snapshot/{snapshot_id}"),
                        "gap": format!("ripr://gap/{}", self.canonical_id),
                        "repair_attempt": Value::Null,
                        "repair_attempt_note": "ripr://repair-attempt/{attempt_id} binds once ripr_prepare_repair creates the session transaction for this item (#3090)",
                    }),
                );
                object
            }),
            "claim_boundary": "Static exposure evidence for one canonical item. This document does not edit source, execute tests or mutation, or authorize a repair transaction.",
            "limitations": [
                "evidence is static analysis only; no runtime, mutation, or test-execution result is claimed",
                "missing or limited fields remain typed limitations and never become repair-ready",
            ],
        })
    }
}

/// Most `observed_values` entries one gap document carries.
pub(crate) const MAX_OBSERVED_VALUES: usize = 12;
/// A value fact counts as near a related test when its line falls within this
/// many lines after the test's start line.
const RELATED_TEST_VALUE_WINDOW: usize = 40;

/// The activation block with `observed_values` bounded. The producer lists
/// every literal it saw across the test surface, which for a heavily tested
/// file is ~10 KB of values from unrelated tests (agent-experience MCP audit
/// finding 3). Entries on or just after a related test's line come first,
/// the rest follow in producer order, the list is capped, and the omission is
/// disclosed with a total and an omitted count; no entry is rewritten and
/// `missing_discriminators` is untouched.
fn bounded_activation(
    activation: &crate::domain::ActivationEvidence,
    related_tests: &[crate::domain::RelatedTest],
) -> Result<Value, String> {
    let mut value = serde_json::to_value(activation)
        .map_err(|error| format!("serialize activation evidence: {error}"))?;
    let total = activation.observed_values.len();
    let near_related = |line: usize| {
        related_tests
            .iter()
            .any(|test| line >= test.line && line - test.line <= RELATED_TEST_VALUE_WINDOW)
    };
    let mut ranked: Vec<&crate::domain::ValueFact> = activation
        .observed_values
        .iter()
        .filter(|fact| near_related(fact.line))
        .collect();
    ranked.extend(
        activation
            .observed_values
            .iter()
            .filter(|fact| !near_related(fact.line)),
    );
    ranked.truncate(MAX_OBSERVED_VALUES);
    let kept = ranked.len();
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "observed_values".to_string(),
            serde_json::to_value(&ranked)
                .map_err(|error| format!("serialize observed values: {error}"))?,
        );
        object.insert("observed_values_total".to_string(), json!(total));
        object.insert(
            "observed_values_omitted".to_string(),
            json!(total.saturating_sub(kept)),
        );
    }
    Ok(value)
}

/// Producer-owned evidence fields, projected once per finding. Fields the
/// producer does not populate stay typed `null`/`absent` states in the
/// serialized finding values rather than being back-filled here.
fn gap_evidence_core(
    finding: &Finding,
    canonical_id: &str,
    root: &Path,
    readiness: &RepairReadiness,
) -> Result<Value, String> {
    let activation = bounded_activation(&finding.activation, &finding.related_tests)?;
    let related_tests = finding
        .related_tests
        .iter()
        .map(|test| {
            json!({
                "name": test.name,
                "file": crate::output::path::repository_relative_path_text(root, &test.file),
                "line": test.line,
                "oracle": test.oracle,
                "oracle_kind": test.oracle_kind.as_str(),
                "oracle_strength": test.oracle_strength.as_str(),
                "relation_reason": test.relation_reason.map(|reason| reason.as_str()),
                "relation_confidence": test.relation_confidence.map(|confidence| confidence.as_str()),
                "miss": test.miss.map(|miss| miss.as_str()),
                "why": crate::output::related_test_miss::related_test_miss_reason(
                    test,
                    &finding.probe.family,
                ),
            })
        })
        .collect::<Vec<_>>();
    let source_currentness = serde_json::to_value(finding.source_currentness)
        .map_err(|error| format!("serialize source currentness: {error}"))?;
    let static_limitation = finding
        .static_limit_kind
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| format!("serialize static limitation: {error}"))?;
    let canonical_gap = finding
        .canonical_gap
        .as_ref()
        .map(|gap| {
            json!({
                "id": gap.id,
                "language": gap.language,
                "owner": gap.owner,
                "behavior_kind": gap.behavior_kind,
                "probe_kind": gap.probe_kind,
                "normalized_discriminator": gap.normalized_discriminator,
            })
        })
        .unwrap_or(Value::Null);

    Ok(json!({
        "canonical_id": canonical_id,
        "finding_id": finding.id,
        "class": finding.class.as_str(),
        "language": finding.language.as_ref().map(|language| language.as_str()),
        "location": {
            "file": crate::analysis::finding_location_text(root, &finding.probe.location.file),
            "line": finding.probe.location.line,
            "column": finding.probe.location.column,
        },
        "changed_behavior": {
            "expression": finding.probe.expression,
            "before": finding.probe.before,
            "after": finding.probe.after,
            "delta_kind": finding.probe.delta.as_str(),
            "probe_family": finding.probe.family.as_str(),
            "expected_sinks": finding.probe.expected_sinks,
        },
        "causal_attribution": canonical_gap,
        "discriminator_availability": activation,
        "related_tests_total": finding.related_tests_total(),
        "related_tests": related_tests,
        "evidence": finding.evidence,
        "missing": finding.missing,
        "static_limitation": static_limitation,
        "source_currentness": source_currentness,
        "recommended_next_step": crate::output::next_step::unresolved_edge_guidance(finding)
            .or_else(|| finding.recommended_next_step.clone()),
        "readiness": {
            "repair_packet_ready": readiness.ready,
            "reason": readiness.detail(),
            "prepared_by": "#3090",
            "evaluation_basis": "producer facts on the committed finding: candidate actionability (#3281), the canonical gap's normalized discriminator (a producer-named missing discriminator is accepted only on a weakly exposed finding, where it is the repair target), and the strongest directly-related related-test grip; conjoined with the shared seam-level repair validator behind `ripr repair`, which must admit the item's seam (#8717)",
        },
        "repair_boundary": {
            "allowed_edit_surface": "none_declared",
            "must_not_change": "none_declared",
            "prepared_by": "#3090",
        },
    }))
}

/// #11213: the finding's own `missing` and `recommended_next_step`, read from
/// the same committed `evidence_core` that `ripr_get_gap` serves, so a
/// not-ready repair card or `ripr_prepare_repair` refusal tells an agent what
/// to change without a second call. Additive and advisory: it carries no
/// readiness, never replaces the typed refusal, and invents nothing the gap
/// does not already hold.
pub(super) fn finding_guidance(item: &GapItem) -> Value {
    json!({
        "missing": item.evidence_core.get("missing").cloned().unwrap_or(Value::Null),
        "recommended_next_step": item
            .evidence_core
            .get("recommended_next_step")
            .cloned()
            .unwrap_or(Value::Null),
        "gap_route": {
            "tool": "ripr_get_gap",
            "arguments": { "canonical_id": item.canonical_id },
        },
    })
}

/// Budget items for the shared [`crate::lsp::diagnostic_budget`] authority.
/// Eligibility is the producer's candidate-actionability predicate captured
/// at projection time; MCP does not invent its own filter or ranking. The
/// selection key uses the same unset (128) ranks the LSP delivery bridge
/// uses, so the budget's evidence-owned order (selection key, then canonical
/// id, then document) is the only ordering applied.
pub(crate) fn budget_items(
    items: &[GapItem],
) -> Vec<crate::lsp::diagnostic_budget::DiagnosticBudgetItem> {
    use crate::lsp::diagnostic_budget::{
        DiagnosticBudgetEligibility, DiagnosticBudgetItem, DiagnosticSelectionKey,
    };
    items
        .iter()
        .map(|item| DiagnosticBudgetItem {
            canonical_id: item.canonical_id.clone(),
            document: item.file.clone(),
            payload_bytes: item.list_summary_bytes(),
            inline_detail_bytes: item
                .evidence_bytes
                .saturating_sub(item.list_summary_bytes()),
            eligibility: if item.candidate_actionable {
                DiagnosticBudgetEligibility::Actionable
            } else {
                DiagnosticBudgetEligibility::ProfileFiltered
            },
            selection_key: DiagnosticSelectionKey {
                repair_route_rank: 128,
                causal_rank: 128,
                evidence_rank: 128,
            },
        })
        .collect()
}

/// Tolerate exactly one trailing slash on a captured resource id (#9895).
/// Clients — especially LLMs hand-assembling `ripr://` links — not
/// uncommonly produce `ripr://gap/{id}/`; every id-bearing resource route
/// normalizes that single slash instead of answering `item_not_found` for
/// an id that merely looks typo'd. Two trailing slashes still miss: the
/// second one is not a normalization artifact, it is a different (invalid)
/// id.
pub(crate) fn trim_one_trailing_slash(id: &str) -> &str {
    id.strip_suffix('/').unwrap_or(id)
}

/// Extract the canonical item id from a `ripr://gap/{id}` resource URI.
///
/// The producer gap id embeds a workspace-relative path (Python, and since
/// #5268 Rust), so the suffix is read whole: the `ripr://gap/` prefix alone
/// routes the URI, and only an empty suffix fails. One trailing slash is
/// normalized away (see [`trim_one_trailing_slash`]). Returns `None` for
/// any other URI, including the exact status resource.
pub(crate) fn gap_resource_id(uri: &str) -> Option<&str> {
    uri.strip_prefix("ripr://gap/")
        .map(trim_one_trailing_slash)
        // A residual trailing slash means the URI carried more than one —
        // that id does not exist, so the route misses rather than
        // normalizing a second slash away. Gap ids embed workspace-relative
        // paths, so only the trailing position is rejected.
        .filter(|id| !id.is_empty() && !id.ends_with('/'))
}

/// Suffix of the `ripr://gap/{canonical_id}/limitation` resource route.
const LIMITATION_RESOURCE_SUFFIX: &str = "/limitation";

/// Extract the canonical item id from a `ripr://gap/{id}/limitation` URI.
/// Canonical ids embed workspace-relative paths, so the id is the whole text
/// between the `ripr://gap/` prefix and the `/limitation` suffix. The caller
/// must still prefer the plain gap route when the full suffix is itself an
/// existing item id.
pub(crate) fn limitation_resource_id(uri: &str) -> Option<&str> {
    uri.strip_prefix("ripr://gap/")
        .map(trim_one_trailing_slash)
        .and_then(|rest| rest.strip_suffix(LIMITATION_RESOURCE_SUFFIX))
        .filter(|id| !id.is_empty() && !id.ends_with('/'))
}

/// Extract the snapshot id from a `ripr://snapshot/{id}` resource URI.
pub(crate) fn snapshot_resource_id(uri: &str) -> Option<&str> {
    uri.strip_prefix("ripr://snapshot/")
        .map(trim_one_trailing_slash)
        .filter(|id| !id.is_empty() && !id.contains('/'))
}

/// Percent-decode a captured resource-id segment (#8736). Canonical ids
/// embed workspace-relative file paths raw — spaces, `#`, and non-ASCII
/// characters are literal — while URI-emitting clients must percent-encode
/// those characters to form a valid URI. Each `%XX` with two hex digits
/// (either case) decodes to its byte; a stray `%` or a short tail stays
/// literal, so a capture that is already raw decodes to itself. Returns
/// `None` when nothing decoded or the decode is not UTF-8: the resource
/// route then keeps the raw capture and its ordinary miss mapping.
pub(crate) fn percent_decode_resource_id(segment: &str) -> Option<String> {
    if !segment.contains('%') {
        return None;
    }
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut changed = false;
    while index < bytes.len() {
        let rest = &bytes[index..];
        if rest[0] == b'%'
            && rest.len() >= 3
            && let (Some(high), Some(low)) = (hex_value(rest[1]), hex_value(rest[2]))
        {
            decoded.push(high * 16 + low);
            index += 3;
            changed = true;
        } else {
            decoded.push(rest[0]);
            index += 1;
        }
    }
    if !changed {
        return None;
    }
    String::from_utf8(decoded)
        .ok()
        .filter(|decoded| decoded != segment)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn serialized_bytes(value: &Value) -> Result<usize, String> {
    struct LengthWriter(usize);

    impl std::io::Write for LengthWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0 += buffer.len();
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = LengthWriter(0);
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| format!("serialize gap item: {error}"))?;
    Ok(writer.0)
}

/// Count and hash the same encoded evidence bytes without retaining them or
/// serializing twice. Snapshot identity binds the digest, so an evidence change
/// still produces a new identity even when its encoded length is unchanged.
fn serialized_evidence_identity(value: &impl serde::Serialize) -> Result<(usize, String), String> {
    struct IdentityWriter {
        bytes: usize,
        digest: Sha256,
    }

    impl std::io::Write for IdentityWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.bytes += buffer.len();
            self.digest.update(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = IdentityWriter {
        bytes: 0,
        digest: Sha256::new(),
    };
    // Preserve the first serialization failure's context from the previous
    // length pass. The same Value and infallible writer supplied both passes.
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| format!("serialize gap item: {error}"))?;
    Ok((writer.bytes, format!("{:x}", writer.digest.finalize())))
}

/// One fully-established candidate finding shared by the `gaps` and `repair`
/// test suites: candidate-current source, a normalized discriminator, and one
/// strong, high-confidence directly-related test on a test surface.
#[cfg(test)]
pub(crate) fn test_finding() -> Result<Finding, String> {
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, Probe, ProbeFamily, ProbeId, RelatedTest,
        RiprEvidence, SourceLocation, StageEvidence, StageState,
    };
    let stage = || StageEvidence::new(StageState::Unknown, Confidence::Unknown, "test");
    Ok(Finding {
        analysis_scope: crate::domain::AnalysisScope::Diff,
        confidence_basis: Vec::new(),
        id: "finding:test:1".to_string(),
        canonical_gap: Some(crate::domain::FindingCanonicalGap {
            id: "gap:test:1".to_string(),
            language: "rust".to_string(),
            file: "src/lib.rs".to_string(),
            owner: "checkout".to_string(),
            behavior_kind: "predicate".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "total == 10000".to_string(),
        }),
        probe: Probe {
            id: ProbeId("probe:test:1".to_string()),
            location: SourceLocation::new("src/lib.rs", 12, 5),
            owner: Some(crate::domain::SymbolId("symbol:checkout".to_string())),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Value,
            before: Some("total > 10000".to_string()),
            after: Some("total >= 10000".to_string()),
            expression: "total >= 10000".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        },
        class: crate::domain::ExposureClass::WeaklyExposed,
        ripr: RiprEvidence {
            reach: stage(),
            infect: stage(),
            propagate: stage(),
            reveal: crate::domain::RevealEvidence {
                observe: stage(),
                discriminate: stage(),
            },
        },
        confidence: 0.5,
        evidence: vec!["the changed comparison reaches the checkout return".to_string()],
        missing: vec!["No strong discriminator was detected".to_string()],
        flow_sinks: Vec::new(),
        activation: ActivationEvidence::default(),
        stop_reasons: Vec::new(),
        related_tests_matched_total: Some(1),
        related_tests: vec![RelatedTest {
            name: "checkout_totals".to_string(),
            file: std::path::PathBuf::from("tests/checkout.rs"),
            line: 40,
            oracle: Some("assert_eq!(total, 10001)".to_string()),
            oracle_kind: crate::domain::OracleKind::ExactValue,
            oracle_strength: crate::domain::OracleStrength::Strong,
            relation_reason: Some(crate::domain::RelationReason::DirectOwnerCall),
            relation_confidence: Some(crate::domain::RelationConfidence::High),
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }],
        recommended_next_step: Some("add a boundary assertion for 10000".to_string()),
        language: Some(crate::domain::LanguageId::Rust),
        language_status: None,
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: Some("total".to_string()),
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn finding() -> Result<Finding, String> {
        test_finding()
    }

    fn value_fact(line: usize) -> crate::domain::ValueFact {
        crate::domain::ValueFact {
            line,
            text: format!("assert_eq!(f({line}), {line})"),
            value: line.to_string(),
            context: crate::domain::ValueContext::AssertionArgument,
        }
    }

    /// Audit finding 3: `observed_values` is the producer's dump of literals
    /// across the test surface. The projection ranks entries at or just after
    /// a related test first, caps the list, and discloses what it dropped;
    /// `missing_discriminators` is untouched.
    #[test]
    fn observed_values_are_ranked_toward_related_tests_capped_and_disclosed() -> Result<(), String>
    {
        let mut finding = finding()?;
        // The fixture's one related test starts at line 40. Unrelated values
        // come first in producer order; the two related ones come last.
        let mut lines: Vec<usize> = (200..240).collect();
        lines.extend([41, 44]);
        finding.activation.observed_values = lines.iter().copied().map(value_fact).collect();
        let activation = bounded_activation(&finding.activation, &finding.related_tests)?;
        let kept = activation["observed_values"]
            .as_array()
            .ok_or("observed_values lost")?;
        if kept.len() != MAX_OBSERVED_VALUES
            || kept[0]["line"] != 41
            || kept[1]["line"] != 44
            || activation["observed_values_total"] != 42
            || activation["observed_values_omitted"] != 42 - MAX_OBSERVED_VALUES
        {
            return Err(format!(
                "observed values not bounded and ranked: {activation}"
            ));
        }
        // Negative: a short list is unchanged and discloses zero omitted.
        finding.activation.observed_values = vec![value_fact(7)];
        let short = bounded_activation(&finding.activation, &finding.related_tests)?;
        if short["observed_values"].as_array().map(Vec::len) != Some(1)
            || short["observed_values_omitted"] != 0
            || short["missing_discriminators"] != serde_json::json!([])
        {
            return Err(format!("short list must pass through whole: {short}"));
        }
        Ok(())
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn limited_journey_mcp_preserves_shared_guidance_and_refusal() -> Result<(), String> {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or("missing repository root")?;
        let fixture = repository.join("fixtures/rust_transitive_reach_positive");
        let root = fixture.join("input");
        let output = crate::app::check_workspace(crate::app::CheckInput {
            root: root.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode: crate::app::Mode::Fast,
            ..Default::default()
        })?;
        assert_eq!(output.findings.len(), 1, "nonempty real producer subject");
        let finding = &output.findings[0];
        assert_eq!(finding.class, crate::domain::ExposureClass::NoStaticPath);
        assert!(matches!(
            finding.static_limit_kind,
            Some(crate::domain::StaticLimitKind::RustIntegrationPublicApiPathUnresolved)
        ));
        let item = GapItem::from_finding(finding, &root)?;
        let expected = crate::output::next_step::reconcile_next_step(finding);
        assert!(expected.contains("Tests cannot close this static-analysis limitation"));
        assert!(expected.contains("entry `outer` -> owner `inner`"));
        assert_ne!(
            finding.recommended_next_step.as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(item.evidence_core["recommended_next_step"], expected);
        assert_eq!(item.evidence_core["class"], "no_static_path");
        assert_eq!(
            item.evidence_core["source_currentness"],
            serde_json::to_value(finding.source_currentness).map_err(|error| error.to_string())?
        );
        assert_eq!(item.evidence_core["evidence"], json!(finding.evidence));
        assert_eq!(
            item.evidence_core["readiness"]["repair_packet_ready"],
            false
        );

        let mut absent = finding.clone();
        absent.recommended_next_step = None;
        let absent = GapItem::from_finding(&absent, &root)?;
        assert_eq!(absent.evidence_core["recommended_next_step"], Value::Null);

        let mut no_edge = finding.clone();
        no_edge.evidence.retain(|line| {
            !line.starts_with(crate::domain::LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX)
        });
        let projected = GapItem::from_finding(&no_edge, &root)?;
        assert_eq!(
            projected.evidence_core["recommended_next_step"],
            json!(no_edge.recommended_next_step)
        );

        let mut unrelated = finding.clone();
        unrelated.static_limit_kind =
            Some(crate::domain::StaticLimitKind::RustValuePropagationUnresolved);
        let projected = GapItem::from_finding(&unrelated, &root)?;
        assert_eq!(
            projected.evidence_core["recommended_next_step"],
            json!(unrelated.recommended_next_step)
        );
        Ok(())
    }

    #[test]
    fn related_test_totals_bind_equal_windows_to_distinct_evidence() -> Result<(), String> {
        let mut source = finding()?;
        let row = source.related_tests[0].clone();
        source.related_tests = (0..8)
            .map(|index| {
                let mut row = row.clone();
                row.name = format!("checkout_{index}");
                row.line += index;
                row
            })
            .collect();
        source.related_tests_matched_total = Some(12);
        let twelve = GapItem::from_finding(&source, Path::new("."))?;
        source.related_tests_matched_total = Some(13);
        let thirteen = GapItem::from_finding(&source, Path::new("."))?;
        assert_eq!(twelve.evidence_core["related_tests_total"], 12);
        assert_eq!(thirteen.evidence_core["related_tests_total"], 13);
        assert_eq!(
            twelve.evidence_core["related_tests"],
            thirteen.evidence_core["related_tests"]
        );
        assert_eq!(
            twelve.evidence_core["related_tests"]
                .as_array()
                .map(Vec::len),
            Some(8)
        );
        let mut only_total_changed = twelve.evidence_core.clone();
        only_total_changed["related_tests_total"] = json!(13);
        assert_eq!(only_total_changed, thirteen.evidence_core);
        assert_eq!(twelve.evidence_bytes, thirteen.evidence_bytes);
        assert_ne!(twelve.evidence_sha256, thirteen.evidence_sha256);
        assert_eq!(twelve.repair_readiness, thirteen.repair_readiness);
        assert_eq!(twelve.list_summary, thirteen.list_summary);

        for (total, retained) in [(1, 1), (0, 0)] {
            source.related_tests.truncate(retained);
            source.related_tests_matched_total = Some(total);
            let item = GapItem::from_finding(&source, Path::new("."))?;
            assert_eq!(item.evidence_core["related_tests_total"], json!(total));
            assert_eq!(
                item.evidence_core["related_tests"].as_array().map(Vec::len),
                Some(retained)
            );
        }
        Ok(())
    }

    #[test]
    fn legacy_serialized_findings_project_retained_totals_without_digest_drift()
    -> Result<(), String> {
        let mut source = finding()?;
        let row = source.related_tests[0].clone();
        source.related_tests = (0..3)
            .map(|index| {
                let mut row = row.clone();
                row.name = format!("checkout_{index}");
                row.line += index;
                row
            })
            .collect();
        source.related_tests_matched_total = Some(3);
        let explicit = GapItem::from_finding(&source, Path::new("."))?;
        let serialized = serde_json::to_value(&source).map_err(|error| error.to_string())?;
        for metadata in [None, Some(Value::Null), Some(json!(0))] {
            let mut legacy = serialized.clone();
            let object = legacy
                .as_object_mut()
                .ok_or("Finding must serialize as an object")?;
            object.remove("related_tests_matched_total");
            if let Some(metadata) = metadata {
                object.insert("related_tests_matched_total".to_string(), metadata);
            }
            let decoded: Finding =
                serde_json::from_value(legacy).map_err(|error| error.to_string())?;
            assert_eq!(decoded.related_tests.len(), 3);
            let projected = GapItem::from_finding(&decoded, Path::new("."))?;
            assert_eq!(projected.evidence_core["related_tests_total"], 3);
            assert_eq!(projected.evidence_core, explicit.evidence_core);
            assert_eq!(projected.evidence_bytes, explicit.evidence_bytes);
            assert_eq!(projected.evidence_sha256, explicit.evidence_sha256);
            assert_eq!(projected.repair_readiness, explicit.repair_readiness);
        }
        Ok(())
    }

    /// #5996: one finding, one location string on every surface. The MCP
    /// item file must join byte-for-byte with the check JSON `probe.file`,
    /// the context packet `probe.file` and the human finding header for the
    /// same finding — including when the analyzed root is a verbatim Windows
    /// path (the MCP server's root spelling), which previously leaked the
    /// raw `//?/` producer join no other surface produced and no editor
    /// resolves.
    ///
    /// #8743 scope note: the packet here is rendered with the default
    /// configuration, and this test claims parity only for fields that are
    /// configuration-independent — the location strings above. `severity`
    /// is config-resolved on the packet since 1.1, so the production MCP
    /// route (which renders with the session's repo config) may legitimately
    /// carry a different `severity` value under a user override; full-packet
    /// parity is only claimed when the effective configurations match.
    #[test]
    fn one_finding_renders_one_location_string_on_every_surface_under_default_config()
    -> Result<(), String> {
        // check-local-context forbids drive-letter path literals; assemble
        // the verbatim root from parts (the `//?/` text lives in the owner's
        // own unit tests).
        let drive = "F:";
        let backslash = '\\';
        let root = PathBuf::from(format!(
            "{backslash}{backslash}?{backslash}{drive}{backslash}repo"
        ));
        let mut finding = finding()?;
        finding.probe.location =
            crate::domain::SourceLocation::new(root.join("src").join("main.rs"), 4, 1);
        let expected = "./src/main.rs";

        let item = GapItem::from_finding(&finding, &root)?;
        if item.file != expected {
            return Err(format!(
                "MCP item file must be workspace-relative: {}",
                item.file
            ));
        }
        let location_file = item
            .evidence_core
            .get("location")
            .and_then(|location| location.get("file"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "gap evidence must carry a location file: {}",
                    item.evidence_core
                )
            })?;
        if location_file != expected {
            return Err(format!(
                "MCP gap location file must be workspace-relative: {location_file}"
            ));
        }

        let output = crate::app::CheckOutput {
            value_origins: Default::default(),
            schema_version: crate::app::CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
            harness_projections: Vec::new(),
            tool: "ripr".to_string(),
            mode: crate::app::Mode::Draft,
            root: root.clone(),
            base: None,
            analysis_outcome: None,
            summary: crate::domain::Summary::default(),
            findings: vec![finding.clone()],
            preview_language_advisories: Vec::new(),
            language_runs: Vec::new(),
            no_scope_provided: false,
            unanalyzed_working_tree: false,
            untracked_working_tree_source_paths: Vec::new(),
            ignored_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            partial_scope: None,
            analyzed_revisions: None,
        };
        let check_json: Value = serde_json::from_str(&crate::output::json::render(&output))
            .map_err(|error| format!("parse check JSON: {error}"))?;
        if check_json
            .pointer("/findings/0/probe/file")
            .and_then(Value::as_str)
            != Some(expected)
        {
            return Err(format!("check JSON probe.file must match: {check_json}"));
        }

        let packet: Value = serde_json::from_str(&crate::output::json::render_context_packet(
            &finding,
            5,
            &crate::config::RiprConfig::default(),
            &root,
        ))
        .map_err(|error| format!("parse context packet: {error}"))?;
        if packet.pointer("/probe/file").and_then(Value::as_str) != Some(expected) {
            return Err(format!("context packet probe.file must match: {packet}"));
        }

        let header = crate::output::human::render_finding_with_config(
            &finding,
            &crate::config::RiprConfig::default(),
            &root,
            crate::output::human::JsonPointerWording::CheckFlag,
        );
        if !header
            .lines()
            .next()
            .is_some_and(|line| line.contains(expected))
        {
            return Err(format!(
                "human header must carry the same location: {header}"
            ));
        }
        Ok(())
    }

    #[test]
    fn evidence_identity_serializes_once_and_matches_encoded_bytes() -> Result<(), String> {
        struct Observed<'a> {
            value: &'a Value,
            calls: std::cell::Cell<usize>,
        }

        impl serde::Serialize for Observed<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.calls.set(self.calls.get() + 1);
                serde::Serialize::serialize(self.value, serializer)
            }
        }

        let item = GapItem::from_finding(&finding()?, Path::new("."))?;
        let values = [
            Value::Null,
            json!({}),
            json!([]),
            json!({
                "escaped": "quote: \" backslash: \\ newline: \n",
                "unicode": "é e\u{301} 😀",
                "nested": [true, false, 42, -17, 0.125, null, {"empty": ""}],
                "large": "x".repeat(65_536),
            }),
            item.evidence_core,
        ];
        for value in values {
            let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
            let observed = Observed {
                value: &value,
                calls: std::cell::Cell::new(0),
            };
            let (length, digest) = serialized_evidence_identity(&observed)?;
            assert_eq!(observed.calls.get(), 1, "evidence must serialize only once");
            assert_eq!(length, bytes.len());
            assert_eq!(digest, format!("{:x}", Sha256::digest(&bytes)));
        }
        Ok(())
    }

    #[test]
    fn evidence_identity_preserves_the_first_serialization_error_context() {
        struct Refused;

        impl serde::Serialize for Refused {
            fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom(
                    "injected evidence encoding failure",
                ))
            }
        }

        assert_eq!(
            serialized_evidence_identity(&Refused),
            Err("serialize gap item: injected evidence encoding failure".to_string())
        );
    }

    #[test]
    fn gap_projection_preserves_evidence_length_and_digest() -> Result<(), String> {
        let mut finding = finding()?;
        finding.recommended_next_step = Some("assert \"é😀\"\nwith a \\ escape".to_string());
        let item = GapItem::from_finding(&finding, Path::new("."))?;
        let bytes = serde_json::to_vec(&item.evidence_core).map_err(|error| error.to_string())?;
        assert_eq!(item.evidence_bytes, bytes.len());
        assert_eq!(item.evidence_sha256, format!("{:x}", Sha256::digest(bytes)));
        Ok(())
    }

    #[test]
    fn canonical_id_prefers_the_producer_gap_identity() -> Result<(), String> {
        let item = GapItem::from_finding(&finding()?, Path::new("."))?;
        if item.canonical_id != "gap:test:1" || item.finding_id != "finding:test:1" {
            return Err(format!(
                "canonical identity must prefer the producer gap id: {item:?}",
                item = (item.canonical_id.clone(), item.finding_id.clone())
            ));
        }
        if item.list_summary.pointer("/class").and_then(Value::as_str) != Some("weakly_exposed") {
            return Err(format!(
                "unexpected class wire token: {}",
                item.list_summary
            ));
        }
        if item.file != "src/lib.rs"
            || item.list_summary.pointer("/line").and_then(Value::as_u64) != Some(12)
        {
            return Err(format!(
                "location projection drifted: {}:{}",
                item.file, item.list_summary
            ));
        }
        Ok(())
    }

    #[test]
    fn fallback_identity_uses_the_finding_id() -> Result<(), String> {
        let mut finding = finding()?;
        finding.canonical_gap = None;
        let item = GapItem::from_finding(&finding, Path::new("."))?;
        if item.canonical_id != "finding:test:1" {
            return Err(format!(
                "without a canonical gap the finding id must be the item identity: {}",
                item.canonical_id
            ));
        }
        Ok(())
    }

    #[test]
    fn eligibility_follows_the_producer_actionability_predicate() -> Result<(), String> {
        use crate::lsp::diagnostic_budget::DiagnosticBudgetEligibility;
        let actionable = budget_items(&[GapItem::from_finding(&finding()?, Path::new("."))?]);
        let Some(actionable_item) = actionable.first() else {
            return Err("budget items must not be empty".to_string());
        };
        if actionable_item.eligibility != DiagnosticBudgetEligibility::Actionable {
            return Err("candidate-current findings must stay actionable".to_string());
        }
        let mut base_deleted = finding()?;
        base_deleted.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        let filtered = budget_items(&[GapItem::from_finding(&base_deleted, Path::new("."))?]);
        let Some(filtered_item) = filtered.first() else {
            return Err("budget items must not be empty".to_string());
        };
        if filtered_item.eligibility != DiagnosticBudgetEligibility::ProfileFiltered {
            return Err(
                "non-candidate-current findings must map to the producer's filtered state"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn evidence_document_reports_producer_repair_readiness() -> Result<(), String> {
        let item = GapItem::from_finding(&finding()?, Path::new("."))?;
        let document = item.document("snapshot:sha256:abc");
        let readiness = document
            .pointer("/item/readiness/repair_packet_ready")
            .ok_or_else(|| "gap document lost its readiness block".to_string())?;
        if readiness != &serde_json::json!(true) {
            return Err(format!(
                "the fully established fixture finding must project repair readiness: {document}"
            ));
        }
        let reason = document
            .pointer("/item/readiness/reason")
            .and_then(Value::as_str)
            .ok_or_else(|| "readiness needs its evaluation reason".to_string())?;
        if reason.is_empty() {
            return Err("readiness reason must not be empty".to_string());
        }
        if document
            .pointer("/item/readiness/prepared_by")
            .and_then(Value::as_str)
            != Some("#3090")
        {
            return Err("readiness lost the repair-slice owner".to_string());
        }
        if document.pointer("/item/repair_boundary/allowed_edit_surface")
            != Some(&serde_json::json!("none_declared"))
        {
            return Err("gap evidence must not invent an allowed edit surface".to_string());
        }
        if document.pointer("/item/links/repair_attempt") != Some(&Value::Null) {
            return Err("the repair-attempt link must stay an explicit null".to_string());
        }
        let snapshot_link = document
            .pointer("/item/links/snapshot")
            .and_then(Value::as_str)
            .ok_or_else(|| "gap document lost its snapshot link".to_string())?;
        if snapshot_link != "ripr://snapshot/snapshot:sha256:abc" {
            return Err(format!("snapshot link drifted: {snapshot_link}"));
        }
        Ok(())
    }

    #[test]
    fn readiness_stays_fail_closed_on_missing_producer_facts() -> Result<(), String> {
        let mut no_discriminator = finding()?;
        no_discriminator.canonical_gap = None;
        let item = GapItem::from_finding(&no_discriminator, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility
                != Some("discriminator_not_populated_for_language")
        {
            return Err(format!(
                "a finding without a canonical gap must refuse repair as an unpopulated language producer fact: {:?}",
                item.repair_readiness.ineligibility
            ));
        }

        let mut named_missing = finding()?;
        named_missing.activation.missing_discriminators.push(
            crate::domain::MissingDiscriminatorFact {
                value: "total == 10000".to_string(),
                reason: "boundary not asserted".to_string(),
                flow_sink: None,
            },
        );
        // #8717: on a weakly exposed finding the named value is the repair
        // target, so the discriminator gate passes and the finding is ready.
        let item = GapItem::from_finding(&named_missing, Path::new("."))?;
        if !item.repair_readiness.ready || item.repair_readiness.ineligibility.is_some() {
            return Err(format!(
                "a weak finding with a named missing discriminator is the repair target: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        // Discriminating negative: the same named value on any non-weak class
        // keeps the typed refusal.
        for class in [
            crate::domain::ExposureClass::ReachableUnrevealed,
            crate::domain::ExposureClass::NoStaticPath,
            crate::domain::ExposureClass::InfectionUnknown,
            crate::domain::ExposureClass::PropagationUnknown,
            crate::domain::ExposureClass::StaticUnknown,
            crate::domain::ExposureClass::Exposed,
        ] {
            let mut other = named_missing.clone();
            other.class = class.clone();
            let item = GapItem::from_finding(&other, Path::new("."))?;
            if item.repair_readiness.ready
                || item.repair_readiness.ineligibility != Some("missing_discriminator")
            {
                return Err(format!(
                    "a named missing discriminator on {class:?} must stay the typed refusal: {:?}",
                    item.repair_readiness.ineligibility
                ));
            }
        }
        // A weak finding whose canonical gap is absent stays refused even
        // with a named missing discriminator.
        let mut ungapped = named_missing.clone();
        ungapped.canonical_gap = None;
        let item = GapItem::from_finding(&ungapped, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility
                != Some("discriminator_not_populated_for_language")
        {
            return Err(format!(
                "a named missing discriminator cannot substitute for a populated gap: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        // The fix-site gate still applies to the weak repair target.
        let mut no_site = named_missing.clone();
        no_site.related_tests.clear();
        let item = GapItem::from_finding(&no_site, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("fix_site_not_established")
        {
            return Err(format!(
                "a weak finding without a fix site must stay refused: {:?}",
                item.repair_readiness.ineligibility
            ));
        }

        let mut base_deleted = finding()?;
        base_deleted.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        let item = GapItem::from_finding(&base_deleted, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("not_candidate_actionable")
        {
            return Err(format!(
                "base-side evidence must not be repair-ready: {:?}",
                item.repair_readiness.ineligibility
            ));
        }

        let mut weak_grip = finding()?;
        for test in &mut weak_grip.related_tests {
            test.oracle_strength = crate::domain::OracleStrength::Weak;
        }
        let item = GapItem::from_finding(&weak_grip, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("fix_site_not_established")
        {
            return Err(format!(
                "a finding without a strong directly-related test must not be repair-ready: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        Ok(())
    }

    fn not_test_surface_readiness(file: &str) -> RepairReadiness {
        let mut readiness = ready_readiness();
        readiness.ready = false;
        readiness.ineligibility = Some("fix_site_not_test_surface");
        if let Some(site) = readiness.fix_site.as_mut() {
            site.file = file.to_string();
        }
        readiness
    }

    /// #5516: only the exact inline module the shared validator admitted can
    /// lift `fix_site_not_test_surface`; a different file, a plain admission
    /// or no verdict keeps the refusal, and a refusal still wins.
    #[test]
    fn inline_admission_lifts_only_the_matching_non_surface_fix_site() -> Result<(), String> {
        let inline = SeamVerdict::AdmittedInline {
            file: "src/lib.rs".to_string(),
            module: "tests".to_string(),
        };
        let lifted = not_test_surface_readiness("src/lib.rs").with_seam_verdict(Some(&inline));
        let site = lifted
            .fix_site
            .as_ref()
            .ok_or("lifted readiness lost its fix site")?;
        if !lifted.ready
            || lifted.ineligibility.is_some()
            || site.inline_test_module.as_deref() != Some("tests")
            || !lifted.reason().contains("governed inline")
            || lifted.reason().contains("on a test surface")
        {
            return Err(format!(
                "matching inline admission did not lift: {lifted:?}"
            ));
        }
        let other_file = not_test_surface_readiness("src/other.rs");
        for (name, readiness, verdict) in [
            ("different file", other_file.clone(), Some(&inline)),
            (
                "plain admission",
                other_file.clone(),
                Some(&SeamVerdict::Admitted),
            ),
            ("no verdict", other_file.clone(), None),
        ] {
            let kept = readiness.clone().with_seam_verdict(verdict);
            if kept != readiness {
                return Err(format!("{name} must keep the refusal: {kept:?}"));
            }
        }
        // A test-surface site is never rewritten by an inline verdict.
        let surface = ready_readiness().with_seam_verdict(Some(&inline));
        if surface != ready_readiness() {
            return Err(format!("test-surface readiness changed: {surface:?}"));
        }
        Ok(())
    }

    fn ready_readiness() -> RepairReadiness {
        RepairReadiness {
            ready: true,
            fix_site: Some(RepairFixSite {
                test_name: "a".to_string(),
                file: "tests/t.rs".to_string(),
                line: 2,
                oracle: Some("assert_eq!(f(), 1);".to_string()),
                oracle_kind: "exact_value",
                inline_test_module: None,
            }),
            ineligibility: None,
            seam_detail: None,
        }
    }

    /// #8717: the shared seam validator's refusal is a conjunct of
    /// readiness. A finding-ready item it refuses is not ready, drops its fix
    /// site, and keeps the validator's own typed code and explanation; an
    /// admitted or unevaluated verdict changes nothing, and a finding-level
    /// refusal stays the first failing gate.
    #[test]
    fn seam_verdict_is_a_conjunct_of_repair_readiness() -> Result<(), String> {
        let refused = SeamVerdict::Refused {
            code: crate::mcp::repair_admission::SEAM_ALREADY_GRIPPED,
            detail: "the shared repair gate refuses: already grip".to_string(),
        };
        let blocked = ready_readiness().with_seam_verdict(Some(&refused));
        if blocked.ready
            || blocked.fix_site.is_some()
            || blocked.ineligibility != Some("seam_already_gripped")
            || !blocked.detail().contains("already grip")
        {
            return Err(format!("refused verdict must block readiness: {blocked:?}"));
        }
        for verdict in [Some(&SeamVerdict::Admitted), None] {
            let kept = ready_readiness().with_seam_verdict(verdict);
            if !kept.ready || kept.ineligibility.is_some() || kept.fix_site.is_none() {
                return Err(format!("admitted/unevaluated must keep ready: {kept:?}"));
            }
        }
        let finding_level = RepairReadiness {
            ready: false,
            fix_site: None,
            ineligibility: Some("missing_discriminator"),
            seam_detail: None,
        }
        .with_seam_verdict(Some(&refused));
        // #10718: the shared validator is the one authority for the cause, so
        // its refusal outranks the producer-side gate; the same item must not
        // read `missing_discriminator` here and "already gripped" in
        // `ripr repair`.
        if finding_level.ineligibility != Some("seam_already_gripped")
            || finding_level.seam_detail.is_none()
        {
            return Err(format!(
                "the shared validator's refusal must outrank the finding gate: {finding_level:?}"
            ));
        }
        // The saved-worktree withheld verdict is a snapshot-level fact, not a
        // seam decision, so a finding-level gate stays first there (#7357).
        let withheld = SeamVerdict::Refused {
            code: crate::mcp::repair_admission::SEAM_WORKTREE_SOURCE,
            detail: "withheld".to_string(),
        };
        let worktree = RepairReadiness {
            ready: false,
            fix_site: None,
            ineligibility: Some("missing_discriminator"),
            seam_detail: None,
        }
        .with_seam_verdict(Some(&withheld));
        if worktree.ineligibility != Some("missing_discriminator") || worktree.seam_detail.is_some()
        {
            return Err(format!(
                "a worktree withhold must not outrank a finding gate: {worktree:?}"
            ));
        }
        Ok(())
    }

    /// The evaluated-admission default is fail-closed: a finding the shared
    /// validator did not cover is refused, never admitted.
    #[test]
    fn evaluated_admission_refuses_an_uncovered_finding() -> Result<(), String> {
        let evaluated = SeamAdmission::evaluate(
            &[],
            &[],
            crate::app::agent_brief::AgentBriefPolicy::from_config(
                &crate::config::RiprConfig::default(),
            ),
            &crate::mcp::repair_admission::InventoryCompleteness::Complete,
            Path::new("."),
        );
        match evaluated.verdict("probe:uncovered") {
            Some(SeamVerdict::Refused { code, .. })
                if code == crate::mcp::repair_admission::SEAM_NOT_FOUND =>
            {
                Ok(())
            }
            other => Err(format!("uncovered finding must be refused: {other:?}")),
        }
    }

    /// #9108: the CLI errors before resolving any item when the repo-wide
    /// inventory is truncated, so MCP must refuse every finding with the
    /// typed truncation reason. Without the fix the empty inventory below
    /// yields `seam_not_found` for the truncated case too.
    #[test]
    fn truncated_inventory_refuses_before_any_resolution() -> Result<(), String> {
        use crate::mcp::repair_admission::{InventoryCompleteness, SEAM_INVENTORY_TRUNCATED};
        let config = crate::config::RiprConfig::default();
        let policy = crate::app::agent_brief::AgentBriefPolicy::from_config(&config);
        let findings = [finding()?];
        let id = findings[0].id.as_str();
        let complete = SeamAdmission::evaluate(
            &findings,
            &[],
            policy,
            &InventoryCompleteness::Complete,
            Path::new("."),
        );
        match complete.verdict(id) {
            Some(SeamVerdict::Refused { code, .. }) if code != SEAM_INVENTORY_TRUNCATED => {}
            other => return Err(format!("complete inventory must not truncate: {other:?}")),
        }
        for completeness in [
            InventoryCompleteness::Truncated {
                analyzed: 3,
                total: 7,
            },
            InventoryCompleteness::Unverified("inventory failed".to_string()),
        ] {
            let evaluated =
                SeamAdmission::evaluate(&findings, &[], policy, &completeness, Path::new("."));
            match evaluated.verdict(id) {
                Some(SeamVerdict::Refused { code, detail }) if code == SEAM_INVENTORY_TRUNCATED => {
                    if matches!(completeness, InventoryCompleteness::Truncated { .. })
                        && !(detail.contains("3 of 7")
                            && detail.contains("RIPR_REPO_EXPOSURE_SEAM_LIMIT"))
                    {
                        return Err(format!("detail must name the counts and remedy: {detail}"));
                    }
                }
                other => return Err(format!("{completeness:?} must refuse: {other:?}")),
            }
        }
        // The refusal blocks readiness with its typed reason and a specific
        // explanation, never the generic fallback.
        let refused = SeamVerdict::Refused {
            code: SEAM_INVENTORY_TRUNCATED,
            detail: "truncated".to_string(),
        };
        let blocked = ready_readiness().with_seam_verdict(Some(&refused));
        if blocked.ready
            || blocked.ineligibility != Some(SEAM_INVENTORY_TRUNCATED)
            || blocked.reason().contains("did not establish every")
        {
            return Err(format!("truncation must block readiness: {blocked:?}"));
        }
        // Precedence: the shared validator's refusal, truncation or an
        // ordinary seam refusal alike, outranks a finding-level gate (#10718).
        let unready = RepairReadiness {
            ready: false,
            fix_site: None,
            ineligibility: Some("discriminator_not_populated_for_language"),
            seam_detail: None,
        };
        let truncated = unready.clone().with_seam_verdict(Some(&refused));
        if truncated.ineligibility != Some(SEAM_INVENTORY_TRUNCATED)
            || truncated.seam_detail.is_none()
        {
            return Err(format!("truncation must outrank the gate: {truncated:?}"));
        }
        let ordinary = SeamVerdict::Refused {
            code: "seam_gate_refused",
            detail: "gate".to_string(),
        };
        let outranked = unready.clone().with_seam_verdict(Some(&ordinary));
        if outranked.ineligibility != Some("seam_gate_refused") || outranked.seam_detail.is_none() {
            return Err(format!(
                "an ordinary seam refusal must outrank a finding gate: {outranked:?}"
            ));
        }
        Ok(())
    }

    /// #9108: the completeness fact is read exactly like the CLI reads it:
    /// an error or `analyzed < total` refuses; an exact fit or no limit
    /// does not.
    #[test]
    fn completeness_mirrors_the_cli_truncation_check() {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        use crate::mcp::repair_admission::InventoryCompleteness as C;
        let info = |analyzed, total| {
            Ok(Some(SeamLimitInfo {
                analyzed,
                total,
                source: SeamLimitSource::Default,
            }))
        };
        assert_eq!(
            C::from_inventory_limit(info(3, 7)),
            C::Truncated {
                analyzed: 3,
                total: 7
            }
        );
        assert_eq!(C::from_inventory_limit(info(7, 7)), C::Complete);
        assert_eq!(C::from_inventory_limit(Ok(None)), C::Complete);
        // Parity with the shared predicate the CLI gates read: for every
        // analyzed/total pair the MCP verdict is truncated iff the CLI's is.
        for analyzed in 0..4usize {
            for total in 0..4usize {
                let limit = SeamLimitInfo {
                    analyzed,
                    total,
                    source: SeamLimitSource::Default,
                };
                assert_eq!(
                    matches!(
                        C::from_inventory_limit(Ok(Some(limit.clone()))),
                        C::Truncated { .. }
                    ),
                    analyzed < total,
                    "analyzed {analyzed} total {total}"
                );
                assert_eq!(limit.is_truncated(), analyzed < total);
            }
        }
        assert_eq!(
            C::from_inventory_limit(Err("boom".to_string())),
            C::Unverified("boom".to_string())
        );
    }

    /// #5210: the non-surface refusal names the inline-test boundary, so
    /// an MCP caller learns the permanent scope from the refusal itself.
    #[test]
    fn non_surface_reason_names_the_inline_test_boundary() -> Result<(), String> {
        let readiness = RepairReadiness {
            ready: false,
            fix_site: None,
            ineligibility: Some("fix_site_not_test_surface"),
            seam_detail: None,
        };
        let reason = readiness.reason();
        for needle in [
            "only test-surface paths or that admitted module can be the authored edit target",
            "inline",
            "#[cfg(test)]",
            "did not admit",
        ] {
            if !reason.contains(needle) {
                return Err(format!(
                    "non-surface reason must name the inline boundary ({needle}): {reason}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn unpopulated_language_discriminator_reason_agrees_with_its_own_availability_block()
    -> Result<(), String> {
        // #5268: when the producer named no missing discriminator but never
        // populated a normalized discriminator either (every Rust finding
        // today), one served document must not contradict itself: the
        // readiness refusal names the unpopulated producer condition instead
        // of claiming the producer failed to establish a discriminator.
        let mut unpopulated = finding()?;
        unpopulated.canonical_gap = None;
        unpopulated.missing = Vec::new();
        unpopulated.class = crate::domain::ExposureClass::Exposed;
        let item = GapItem::from_finding(&unpopulated, Path::new("."))?;
        let document = item.document("snapshot:sha256:abc");
        let reason = document
            .pointer("/item/readiness/reason")
            .and_then(Value::as_str)
            .ok_or_else(|| "readiness lost its reason".to_string())?;
        if document
            .pointer("/item/readiness/repair_packet_ready")
            .and_then(Value::as_bool)
            != Some(false)
        {
            return Err(format!(
                "an unpopulated discriminator must stay fail-closed: {document}"
            ));
        }
        let availability = document
            .pointer("/item/discriminator_availability/missing_discriminators")
            .and_then(Value::as_array)
            .ok_or_else(|| "availability block lost its missing discriminators".to_string())?;
        if !availability.is_empty() {
            return Err(format!(
                "the mismatch control needs an availability block with nothing missing: {availability:?}"
            ));
        }
        if !reason.contains("normalized discriminator") || !reason.contains("not populated") {
            return Err(format!(
                "the readiness reason must name the unpopulated producer condition, not a contradiction: {reason}"
            ));
        }
        if reason.contains("did not establish a discriminator") {
            return Err(format!(
                "the readiness reason must not claim the producer failed to establish a discriminator while the same document shows nothing missing: {reason}"
            ));
        }
        Ok(())
    }

    #[test]
    fn static_limited_python_finding_refuses_as_static_limitation_not_language_gap()
    -> Result<(), String> {
        // A Python producer withholds the canonical gap exactly when the
        // finding carries its own typed static limitation
        // (`analysis/language/python/classify.rs` gates the gap on
        // `static_limit.is_none()`), so such a finding must refuse as
        // `static_limitation` — the limitation, not language-wide
        // non-population, explains the missing fact.
        let mut limited = finding()?;
        limited.canonical_gap = None;
        limited.missing = Vec::new();
        limited.language = Some(crate::domain::LanguageId::Python);
        limited.static_limit_kind = Some(crate::domain::StaticLimitKind::UnsupportedSyntax);
        let item = GapItem::from_finding(&limited, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("static_limitation")
        {
            return Err(format!(
                "a static-limited finding must refuse as static_limitation, not as a language-wide gap: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        let document = item.document("snapshot:sha256:abc");
        let reason = document
            .pointer("/item/readiness/reason")
            .and_then(Value::as_str)
            .ok_or_else(|| "readiness lost its reason".to_string())?;
        if !reason.contains("static limitation") || reason.contains("not populated") {
            return Err(format!(
                "the static-limited refusal must name the finding's limitation, not the language: {reason}"
            ));
        }
        Ok(())
    }

    fn limitation_field<'a>(document: &'a Value, name: &str) -> Result<&'a str, String> {
        document
            .pointer(&format!("/limitation/{name}"))
            .and_then(Value::as_str)
            .ok_or_else(|| format!("limitation document lacks {name}: {document}"))
    }

    /// #1898: the producer's own typed static limitation is the earliest
    /// fact and outranks every readiness gate, reusing the shared
    /// `describe`/`next_step` vocabulary instead of a renderer-local string.
    #[test]
    fn limitation_names_the_producer_static_limit_ahead_of_readiness_gates() -> Result<(), String> {
        let mut limited = finding()?;
        limited.canonical_gap = None;
        limited.language = Some(crate::domain::LanguageId::Python);
        limited.static_limit_kind = Some(crate::domain::StaticLimitKind::UnsupportedSyntax);
        let item = GapItem::from_finding(&limited, Path::new("."))?;
        let document = item.limitation_document("snapshot:sha256:abc");
        assert_eq!(limitation_field(&document, "kind")?, "unsupported_syntax");
        assert_eq!(
            limitation_field(&document, "stage")?,
            "producer_static_limit"
        );
        assert_eq!(
            limitation_field(&document, "reason")?,
            crate::domain::StaticLimitKind::UnsupportedSyntax.describe()
        );
        assert_eq!(
            limitation_field(&document, "recovery")?,
            crate::domain::StaticLimitKind::UnsupportedSyntax.next_step()
        );
        assert_eq!(
            document.pointer("/limitation/retryable"),
            Some(&Value::Bool(false))
        );
        assert!(limitation_field(&document, "prohibited_claim")?.contains("stronger"));
        assert_eq!(
            document.pointer("/limitation/restart/snapshot_id"),
            Some(&Value::from("snapshot:sha256:abc"))
        );
        // Discriminating negative: the same finding without the static limit
        // does not report a producer static limit, so the kind is driven by
        // the finding's own typed fact and not by the readiness refusal.
        let mut unlimited = limited.clone();
        unlimited.static_limit_kind = None;
        let other = GapItem::from_finding(&unlimited, Path::new("."))?;
        let other = other.limitation_document("snapshot:sha256:abc");
        assert_ne!(limitation_field(&other, "stage")?, "producer_static_limit");
        Ok(())
    }

    /// #1898: with no producer static limit the first failing readiness gate
    /// is the limitation; the shared seam validator's refusal outranks the
    /// finding gates and is attributed to `seam_admission`; a saved-worktree
    /// withhold is the one retryable-without-edit limitation.
    #[test]
    fn limitation_reports_the_first_failing_readiness_gate_with_its_owner() -> Result<(), String> {
        let mut no_gap = finding()?;
        no_gap.canonical_gap = None;
        let item = GapItem::from_finding(&no_gap, Path::new("."))?;
        let document = item.limitation_document("snapshot:sha256:abc");
        assert_eq!(
            limitation_field(&document, "kind")?,
            "discriminator_not_populated_for_language"
        );
        assert_eq!(limitation_field(&document, "stage")?, "repair_readiness");

        let mut ready = GapItem::from_finding(&finding()?, Path::new("."))?;
        ready.repair_readiness = ready_readiness().with_seam_verdict(Some(&SeamVerdict::Refused {
            code: crate::mcp::repair_admission::SEAM_ALREADY_GRIPPED,
            detail: "the seam is already gripped".to_string(),
        }));
        let document = ready.limitation_document("snapshot:sha256:abc");
        assert_eq!(limitation_field(&document, "kind")?, "seam_already_gripped");
        assert_eq!(limitation_field(&document, "stage")?, "seam_admission");
        assert_eq!(
            limitation_field(&document, "reason")?,
            "the seam is already gripped"
        );
        assert_eq!(
            document.pointer("/limitation/retryable"),
            Some(&Value::Bool(false))
        );

        let mut worktree = GapItem::from_finding(&finding()?, Path::new("."))?;
        worktree.repair_readiness =
            ready_readiness().with_seam_verdict(Some(&SeamVerdict::Refused {
                code: crate::mcp::repair_admission::SEAM_WORKTREE_SOURCE,
                detail: "worktree snapshot".to_string(),
            }));
        let document = worktree.limitation_document("snapshot:sha256:abc");
        assert_eq!(
            document.pointer("/limitation/retryable"),
            Some(&Value::Bool(true))
        );
        assert!(limitation_field(&document, "recovery")?.contains("committed source"));
        Ok(())
    }

    /// #1898: an item with no limiting fact answers with an explicit typed
    /// `no_limitation_established`, never `null`, and claims no runtime result.
    #[test]
    fn limitation_for_a_ready_item_is_explicit_and_claims_no_runtime_result() -> Result<(), String>
    {
        let mut item = GapItem::from_finding(&finding()?, Path::new("."))?;
        item.repair_readiness = ready_readiness();
        item.static_limit = None;
        let document = item.limitation_document("snapshot:sha256:abc");
        assert_eq!(
            limitation_field(&document, "kind")?,
            LIMITATION_NONE_ESTABLISHED
        );
        assert_eq!(limitation_field(&document, "stage")?, "none");
        assert!(limitation_field(&document, "prohibited_claim")?.contains("runtime"));
        assert_eq!(
            document.get("schema_version"),
            Some(&Value::from(LIMITATION_SCHEMA_VERSION))
        );
        // Deterministic: repeated reads of unchanged facts are identical.
        assert_eq!(document, item.limitation_document("snapshot:sha256:abc"));
        Ok(())
    }

    #[test]
    fn limitation_resource_route_reads_the_whole_nested_id() {
        assert_eq!(
            limitation_resource_id("ripr://gap/gap:a/b.rs:1/limitation"),
            Some("gap:a/b.rs:1")
        );
        assert_eq!(
            limitation_resource_id("ripr://gap/gap:x/limitation/"),
            Some("gap:x")
        );
        for other in [
            "ripr://gap/gap:x",
            "ripr://gap//limitation",
            "ripr://gap/limitation",
            "ripr://snapshot/x/limitation",
            "ripr://gap/x/limitation//",
        ] {
            assert_eq!(limitation_resource_id(other), None, "{other}");
        }
    }

    /// #5268 residual: once the Rust producer populates canonical gaps, a
    /// producer-shaped Rust finding (the pricing boundary seam) must obey the
    /// same gate ordering as every other language — a producer-named missing
    /// discriminator keeps the typed refusal ahead of the populated gap, and
    /// clearing it (the boundary got pinned by a test) lets the same finding
    /// through the discriminator gate.
    #[test]
    fn populated_rust_gap_opens_the_discriminator_gate_for_weak_named_or_nothing_missing()
    -> Result<(), String> {
        let mut boundary = finding()?;
        boundary.language = Some(crate::domain::LanguageId::Rust);
        boundary.canonical_gap = Some(crate::domain::FindingCanonicalGap {
            id: "gap:rust:src/lib.rs:discount:predicate_boundary:predicate:total==100".to_string(),
            language: "rust".to_string(),
            file: "src/lib.rs".to_string(),
            owner: "discount".to_string(),
            behavior_kind: "predicate_boundary".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "total==100".to_string(),
        });
        boundary
            .activation
            .missing_discriminators
            .push(crate::domain::MissingDiscriminatorFact {
                value: "total == 100".to_string(),
                reason: "no test call pins the equality boundary".to_string(),
                flow_sink: None,
            });
        // Weak + named missing discriminator + populated gap: the repair
        // target, ready at the finding level (#8717).
        let item = GapItem::from_finding(&boundary, Path::new("."))?;
        if !item.repair_readiness.ready {
            return Err(format!(
                "a weak Rust finding naming its missing discriminator with the gap populated is the repair target: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        // The same named value on a non-weak class keeps the typed refusal
        // ahead of the populated gap.
        let mut unrevealed = boundary.clone();
        unrevealed.class = crate::domain::ExposureClass::ReachableUnrevealed;
        let item = GapItem::from_finding(&unrevealed, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("missing_discriminator")
        {
            return Err(format!(
                "a named missing discriminator on a non-weak class must keep the typed refusal even with the gap populated: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        // The boundary got pinned: the producer names nothing missing, the
        // finding is exposed, and the populated gap opens the gate — the
        // finding then fails or passes on the remaining (fix-site) gates
        // exactly like the fixture's strong test surface allows.
        boundary.activation.missing_discriminators.clear();
        boundary.missing.clear();
        boundary.class = crate::domain::ExposureClass::Exposed;
        let item = GapItem::from_finding(&boundary, Path::new("."))?;
        if !item.repair_readiness.ready {
            return Err(format!(
                "an exposed Rust finding with a populated gap and nothing missing must pass the discriminator gate: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        Ok(())
    }

    #[test]
    fn complete_and_incomplete_evidence_stay_distinct() -> Result<(), String> {
        let mut with_gap = finding()?;
        with_gap.missing = Vec::new();
        let complete = GapItem::from_finding(&with_gap, Path::new("."))?;
        let incomplete = GapItem::from_finding(&finding()?, Path::new("."))?;
        if complete.evidence_bytes == incomplete.evidence_bytes {
            return Err(
                "missing-evidence and complete-evidence items must not project identically"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn resource_uri_parsing_is_strict() {
        assert_eq!(gap_resource_id("ripr://gap/gap:test:1"), Some("gap:test:1"));
        assert_eq!(
            snapshot_resource_id("ripr://snapshot/snapshot:sha256:x"),
            Some("snapshot:sha256:x")
        );
        for other in [
            "ripr://workspace/status",
            "ripr://gap/",
            "ripr://snapshot/",
            "https://example.com/gap/x",
        ] {
            assert_eq!(gap_resource_id(other), None, "{other}");
        }
        assert_eq!(snapshot_resource_id("ripr://gap/x"), None);
    }

    /// #9895: every id-bearing resource route tolerates exactly one
    /// trailing slash — `ripr://gap/{id}/` addresses the same item as
    /// `ripr://gap/{id}` — while a second slash, or a slash that would
    /// leave the id empty, still misses.
    #[test]
    fn resource_id_routes_tolerate_one_trailing_slash() {
        for (uri, expected) in [
            ("ripr://gap/gap:test:1/", Some("gap:test:1")),
            (
                "ripr://gap/gap:rust:src/lib.rs:nested/",
                Some("gap:rust:src/lib.rs:nested"),
            ),
            // A second slash is a different (invalid) id, not a
            // normalization artifact.
            ("ripr://gap/gap:test:1//", None),
            // The bare prefix with only a slash still routes nowhere.
            ("ripr://gap//", None),
        ] {
            assert_eq!(gap_resource_id(uri), expected, "gap route: {uri}");
        }
        for (uri, expected) in [
            (
                "ripr://snapshot/snapshot:sha256:x/",
                Some("snapshot:sha256:x"),
            ),
            ("ripr://snapshot/snapshot:sha256:x//", None),
            ("ripr://snapshot//", None),
            // The single-segment grammar stays strict inside the id.
            ("ripr://snapshot/a/b/", None),
        ] {
            assert_eq!(snapshot_resource_id(uri), expected, "snapshot route: {uri}");
        }
    }

    /// #5268 review: the producer gap id embeds a workspace-relative path,
    /// so the advertised `ripr://gap/{id}` resource must parse the suffix
    /// whole — a nested-file Rust (or Python) finding's resource link has to
    /// resolve to the same id its tool calls address. The snapshot grammar
    /// stays single-segment.
    #[test]
    fn gap_resource_route_reads_a_nested_producer_gap_id_whole() {
        let nested = "gap:rust:src/lib.rs:discount:predicate_boundary:predicate:amount==fee";
        assert_eq!(
            gap_resource_id(&format!("ripr://gap/{nested}")),
            Some(nested)
        );
        assert_eq!(
            gap_resource_id(&format!("ripr://gap/{nested}/")),
            Some(nested),
            "one trailing slash must not truncate a nested id"
        );
        assert_eq!(
            crate::mcp::repair_card::repair_card_resource_id(&format!(
                "ripr://repair-card/{nested}"
            )),
            Some(nested)
        );
        assert_eq!(
            crate::mcp::repair_card::repair_card_resource_id(&format!(
                "ripr://repair-card/{nested}/"
            )),
            Some(nested),
            "one trailing slash must not truncate a nested repair-card id"
        );
        // A snapshot id never carries a path, so its grammar stays strict.
        assert_eq!(snapshot_resource_id("ripr://snapshot/a/b"), None);
    }

    /// #8736: canonical ids embed raw workspace-relative paths (spaces,
    /// `#`, and non-ASCII are literal), so URI-emitting clients must
    /// percent-encode those characters. The decoder inverts valid `%XX`
    /// escapes (either hex case), keeps stray `%` and short tails literal,
    /// and reports no change when nothing decoded — the resource route
    /// retries the decoded segment only when it differs from the capture.
    #[test]
    fn percent_decoding_resource_id_segments() {
        assert_eq!(
            percent_decode_resource_id("gap%3Arust%3Asrc%2Flib.rs"),
            Some("gap:rust:src/lib.rs".to_string())
        );
        assert_eq!(
            percent_decode_resource_id("gap:rust:my%20file%23name.rs"),
            Some("gap:rust:my file#name.rs".to_string())
        );
        assert_eq!(
            percent_decode_resource_id("%E2%98%83"),
            Some("☃".to_string())
        );
        assert_eq!(
            percent_decode_resource_id("gap%3arust"),
            Some("gap:rust".to_string())
        );
        // A raw stable-path id (`%25` is a literal percent) must decode to
        // the raw spelling, and the server falls back to the raw capture
        // when the decoded form misses.
        assert_eq!(
            percent_decode_resource_id("pricing_%25FF.rs"),
            Some("pricing_%FF.rs".to_string())
        );
        // Nothing to decode: no `%`, no valid escape, or no change.
        assert_eq!(percent_decode_resource_id("gap:rust:src/lib.rs"), None);
        assert_eq!(percent_decode_resource_id("gap:rust:100%.rs"), None);
        assert_eq!(percent_decode_resource_id("gap:rust:trailing%"), None);
        // Invalid UTF-8 after decoding refuses the retry instead of
        // manufacturing an unmatchable id.
        assert_eq!(percent_decode_resource_id("%FF%FE"), None);
    }

    /// #5510: the MCP gap evidence carries the packet-backed Perl rows with
    /// the shared sentence on the direct row only.
    #[cfg(feature = "lang-perl")]
    #[test]
    fn perl_packet_backed_rows_agree_in_gap_evidence() -> Result<(), String> {
        use crate::output::related_test_miss::related_test_miss_reason;
        let finding = crate::analysis::perl_direct_and_advisory_finding()?;
        let [direct, advisory] = finding.related_tests.as_slice() else {
            return Err(format!("expected two rows: {:?}", finding.related_tests));
        };
        let why = related_test_miss_reason(direct, &finding.probe.family)
            .ok_or("the direct row should have a reason")?;
        let item = GapItem::from_finding(&finding, Path::new("/test-root"))?;
        let rows = item.evidence_core["related_tests"]
            .as_array()
            .ok_or("expected related_tests in gap evidence")?;
        let names = rows
            .iter()
            .map(|row| row["name"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(names, [direct.name.as_str(), advisory.name.as_str()]);
        assert_eq!(rows[0]["miss"], "observation_unconfirmed");
        assert_eq!(rows[0]["why"], why.as_str());
        assert!(rows[1]["miss"].is_null() && rows[1]["why"].is_null());
        Ok(())
    }

    #[test]
    fn gap_projection_renders_every_served_file_root_relative() -> Result<(), String> {
        // #5254 item 6: the observed wire leak was a canonicalized finding
        // file served verbatim. Every served file renders relative to the
        // analyzed root: the item, the list summary, and the evidence
        // location through the shared finding-location owner (#5996, with
        // its `./` prefix), related tests and the fix site through the
        // repository-relative renderer. Host-native absolute roots (no
        // filesystem touch) so the test pins the threading on every host;
        // the verbatim/case spelling matrix lives portably with the
        // renderer in `output::path`.
        let root = std::env::temp_dir().join("ripr-gap-path-root");
        let mut finding = finding()?;
        finding.probe.location.file = root.join("src/lib.rs");
        finding.related_tests[0].file = root.join("tests/checkout.rs");

        let item = GapItem::from_finding(&finding, &root)?;
        assert_eq!(item.file, "./src/lib.rs");
        assert_eq!(
            item.list_summary.pointer("/file").and_then(Value::as_str),
            Some("./src/lib.rs")
        );
        assert_eq!(
            item.evidence_core
                .pointer("/location/file")
                .and_then(Value::as_str),
            Some("./src/lib.rs")
        );
        assert_eq!(
            item.evidence_core
                .pointer("/related_tests/0/file")
                .and_then(Value::as_str),
            Some("tests/checkout.rs")
        );
        let fix_site = item
            .repair_readiness
            .fix_site
            .as_ref()
            .ok_or("the strong related test should establish a fix site")?;
        assert_eq!(fix_site.file, "tests/checkout.rs");
        // Belt and braces: the root's unique marker appears nowhere in the
        // served documents.
        for document in [&item.list_summary, &item.evidence_core] {
            let text = serde_json::to_string(document).map_err(|error| error.to_string())?;
            assert!(
                !text.contains("ripr-gap-path-root"),
                "served documents must not leak host paths: {text}"
            );
        }
        Ok(())
    }

    #[test]
    fn gap_projection_keeps_an_out_of_root_file_stable() -> Result<(), String> {
        // A file the root does not contain cannot be expressed relatively;
        // it keeps its full stable spelling rather than an invented location.
        let root = std::env::temp_dir().join("ripr-gap-path-root");
        let elsewhere = std::env::temp_dir().join("ripr-gap-path-elsewhere");
        let mut finding = finding()?;
        finding.probe.location.file = elsewhere.join("lib.rs");

        let item = GapItem::from_finding(&finding, &root)?;
        assert!(
            item.file.contains("ripr-gap-path-elsewhere") && item.file.ends_with("lib.rs"),
            "an out-of-root file must keep its full stable spelling, got: {}",
            item.file
        );
        Ok(())
    }
}
