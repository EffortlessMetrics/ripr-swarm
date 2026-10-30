//! Normalized release-ledger DTO and its closed vocabularies. JSON, Markdown
//! and the manifest are all projections of this one shape.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(super) const SCHEMA_VERSION: &str = "0.1";
pub(super) const KIND: &str = "release_ledger";
pub(super) const ADJUDICATION_BASIS: &str = "rule_derived";
pub(super) const STAGE_DRY_RUN: &str = "dry_run";
pub(super) const STAGE_FINAL: &str = "final";
pub(super) const BLOCKING: &str = "blocking_unresolved";

pub(super) const DISPOSITIONS: &[&str] = &[
    "included_accepted_product",
    "included_accepted_release_or_control",
    "included_test_or_evidence",
    "included_structural_no_semantic_delta",
    "included_superseded_by_later_fix",
    "included_historical_reachable_only",
    "included_with_release_nonclaim",
    "not_in_cut_accepted_defer",
    "not_in_cut_superseded",
    "not_in_cut_unrelated",
    BLOCKING,
];

/// Open-PR dispositions, as in `policy/release-selection.json` (#2766). An
/// open PR is by construction outside the cut, so these are the packet's own
/// values rather than the commit vocabulary.
pub(super) const PR_DISPOSITIONS: &[&str] = &[
    "release_required_product",
    "release_required_control",
    "release_required_test_infrastructure",
    "release_optional_pending_decision",
    "accepted_defer_with_non_claim",
    "candidate_exclusion_with_manifest",
    "blocked_on_named_authority",
    "superseded",
    "unrelated_to_selected_candidate",
];

/// Open-PR dispositions that carry no unfinished work.
pub(super) const TERMINAL_PR_DISPOSITIONS: &[&str] = &[
    "accepted_defer_with_non_claim",
    "candidate_exclusion_with_manifest",
    "superseded",
    "unrelated_to_selected_candidate",
];

pub(super) const PRESENT_EFFECTIVE: &str = "present_effective";
pub(super) const PRESENT_BUT_SUPERSEDED: &str = "present_but_superseded";
pub(super) const HISTORY_NO_BLOB_EFFECT: &str = "history_reachable_no_final_blob_effect";
pub(super) const NOT_REACHABLE: &str = "not_reachable_from_candidate";

pub(super) const TREE_EFFECTS: &[&str] = &[
    PRESENT_EFFECTIVE,
    PRESENT_BUT_SUPERSEDED,
    HISTORY_NO_BLOB_EFFECT,
    NOT_REACHABLE,
];

pub(super) const SOURCE_SYNC_EFFECTS: &[&str] = &[
    "no_source_sync_paths",
    "touches_source_sync_paths",
    "not_established",
];

/// #2766 claim resolutions, as in `policy/release-selection.json`.
pub(super) const CLAIM_RESOLUTIONS: &[&str] = &[
    "pending",
    "landed_and_accepted",
    "accepted_defer_with_non_claim",
    "candidate_exclusion_with_manifest",
    "failed_release_hold",
    "final_candidate_execution_only",
    "historical_evidence_only",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ledger {
    pub(super) schema_version: String,
    pub(super) kind: String,
    pub(super) rule_version: String,
    pub(super) adjudication_basis: String,
    pub(super) stage: String,
    pub(super) source: Source,
    pub(super) range: Range,
    pub(super) merged_prs: Vec<MergedPr>,
    pub(super) selected_claims: SelectedClaims,
    pub(super) open_prs: Vec<OpenPr>,
    pub(super) digests: Digests,
    #[serde(default)]
    pub(super) audit: Option<AuditRef>,
    pub(super) records: Vec<Record>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub(super) repository: String,
    pub(super) base_sha: String,
    pub(super) cut_sha: String,
    pub(super) cut_tree_sha: String,
    /// Tip of protected `main` observed at capture. A cut is exact only when
    /// it equals this tip.
    pub(super) main_sha: String,
}

/// Retained separately so "first-parent commit" and "reachable commit" are
/// never conflated.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Range {
    /// Ordered oldest to newest, `base..cut`.
    pub(super) first_parent: Vec<String>,
    /// Every commit reachable from the cut and not from the base, in
    /// first-parent order followed by the remaining commits sorted by SHA.
    pub(super) reachable: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MergedPr {
    pub(super) number: u64,
    pub(super) merge_sha: String,
    pub(super) state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SelectedClaims {
    /// `sha256:` of the raw bytes of `policy/release-selection.json`.
    pub(super) packet_digest: String,
    pub(super) rows: Vec<ClaimRow>,
    pub(super) totals: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClaimRow {
    pub(super) claim_ref: String,
    pub(super) resolution: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OpenPr {
    pub(super) number: u64,
    pub(super) head_sha: String,
    pub(super) disposition: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Digests {
    pub(super) first_parent: String,
    pub(super) reachable: String,
    pub(super) records: String,
    pub(super) tree: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuditRef {
    pub(super) path: String,
    pub(super) sha256: String,
}

/// Evidence behind `final_tree_effect`, so the effect can be checked without
/// re-reading Git.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectBasis {
    /// Touched paths that exist at the cut.
    pub(super) alive_paths: u64,
    /// Touched paths absent at the cut.
    pub(super) absent_paths: u64,
    /// Alive paths whose final blob this commit last wrote.
    pub(super) last_toucher_paths: u64,
    /// Surviving lines blamed to this commit in alive multi-touch paths.
    pub(super) blamed_lines: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub(super) commit_sha: String,
    pub(super) parent_shas: Vec<String>,
    pub(super) tree_sha: String,
    /// 1-based; `None` for reachable commits that are not first-parent.
    pub(super) first_parent_position: Option<u64>,
    pub(super) subject: String,
    pub(super) pr_refs: Vec<u64>,
    pub(super) issue_refs: Vec<u64>,
    pub(super) theme: String,
    pub(super) claim_ref: Option<String>,
    pub(super) release_disposition: String,
    pub(super) final_tree_effect: String,
    pub(super) effect_basis: EffectBasis,
    pub(super) acceptance_owner: String,
    pub(super) review_proof_refs: Vec<String>,
    pub(super) source_sync_authority_effect: String,
    pub(super) limitations: Vec<String>,
    #[serde(default)]
    pub(super) override_ref: Option<String>,
}
