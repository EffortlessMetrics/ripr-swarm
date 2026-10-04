//! Issue lifecycle contract/plan decision-boundary pilot (#4931, RIPR-SPEC-0232).
//!
//! The second real-corpus consumer of the frozen RIPR-SPEC-0218 attempt
//! contract (#4929), extending the sibling intake machinery (RIPR-SPEC-0223):
//! exactly two REAL current issue snapshots, captured immutably and read-only,
//! traverse the decision boundary between contract-required work and direct
//! one-PR work. The contract-required row retains the full contract-case
//! evidence (source-of-truth identity, spec-required rationale, draft spec
//! identity, author role/config/result identity, independent adversary result
//! with inspected scope, missing failure/limited findings or bounded
//! `none_found`, preserved open decisions, the root disposition in the closed
//! RIPR-SPEC-0218 vocabulary and the accepted/amended/rejected/provisional
//! contract state). Both rows retain the planning evidence (one-PR versus
//! campaign decision, work-item identities with dependency edges, acceptance
//! rows covered or explicitly omitted, edit cages and semantic conflict
//! resources, proof commands with denominators, stop conditions, non-goals,
//! portfolio placement without a tracked selection/current-work mutation, and
//! root corrections/re-splits). The author/adversary/root roles are distinct
//! fixture identities with distinct results: the author result never carries
//! an acceptance state and the root disposition is the only acceptance
//! authority, expressed in the RIPR-SPEC-0218 disposition set only.
//!
//! Selection law: this module selects no work. It reads only committed corpus
//! fields; no `active.toml`, label, issue age or title is consulted, and the
//! packet projection is a pure function of the corpus row. Mechanics controls
//! (the ten required decision-boundary controls) live in a separate synthetic
//! control corpus and never enter the two real rows or any real denominator.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::issue_lifecycle_attempt::{
    IssueLifecycleAttemptV1, IssueLifecycleDispositionV1, IssueLifecycleRowAssessmentV1,
    assess_issue_lifecycle_attempt,
};
use crate::issue_lifecycle_intake::{
    IssueLifecycleIntakeRetrievalStepV1, IssueLifecycleIntakeSnapshotV1,
};

pub(crate) const ISSUE_LIFECYCLE_CONTRACT_PLAN_CORPUS_SCHEMA_VERSION: &str =
    "issue_lifecycle_contract_plan_corpus.v1";
pub(crate) const ISSUE_LIFECYCLE_CONTRACT_PLAN_CONTROL_CORPUS_SCHEMA_VERSION: &str =
    "issue_lifecycle_contract_plan_control_corpus.v1";
pub(crate) const ISSUE_LIFECYCLE_CONTRACT_PLAN_PROVENANCE_SCHEMA_VERSION: &str =
    "issue_lifecycle_contract_plan_provenance.v1";
pub(crate) const DEFAULT_CONTRACT_PLAN_CORPUS_DIR: &str = "fixtures/issue_lifecycle_contract_plan";

/// The closed two-category vocabulary #4931 requires, one row per category.
pub(crate) const REQUIRED_ISSUE_LIFECYCLE_CONTRACT_PLAN_CATEGORIES: [&str; 2] =
    ["contract_required", "narrow_accepted_contract_bug"];

/// The ten mechanics controls the committed control corpus must carry: each
/// names one decision-boundary law the two real rows can never exercise
/// alone, so an emptied or gutted control corpus fails closed instead of
/// passing vacuously.
pub(crate) const REQUIRED_ISSUE_LIFECYCLE_CONTRACT_PLAN_CONTROLS: [&str; 10] = [
    "adversary_catches_missing_failure_state_or_bounded_none_found",
    "author_cannot_accept_own_contract",
    "unresolved_decision_blocks_implementation",
    "narrow_bug_gets_no_unnecessary_spec",
    "campaign_rejected_when_one_vertical_slice_suffices",
    "one_pr_rejected_when_acceptance_cannot_be_covered_coherently",
    "specs_contain_behavior_not_execution_queues",
    "plans_contain_work_order_not_new_behavior_authority",
    "no_tracked_selection_or_current_work_file_changed",
    "cold_start_root_reconstructs_decisions_from_artifacts",
];

pub(crate) const ISSUE_LIFECYCLE_CONTRACT_PLAN_CLAIM_BOUNDARY: &str = "Read-only contract/plan pilot receipt: two exact real issue snapshots \
 traverse the contract/plan decision boundary with distinct author, adversary \
 and root fixture identities; it claims no decision correctness beyond these \
 two rows, no implementation, no plan execution and no parent acceptance.";

/// The closed contract-state vocabulary a root disposition may carry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IssueLifecycleContractStateV1 {
    Accepted,
    Amended,
    Rejected,
    Provisional,
}

/// One role result: identity, config and produced result artifact. The author
/// result must never carry an acceptance state; only the root disposition
/// records acceptance authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractRoleResultV1 {
    pub role: String,
    pub config_identity: String,
    pub result_identity: String,
    pub carries_acceptance: bool,
}

/// The independent adversary result: what it inspected and what it found. A
/// bounded `none_found` (empty findings) is honest; findings and `none_found`
/// are mutually exclusive.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractAdversaryResultV1 {
    pub result_identity: String,
    pub inspected_scope: String,
    pub findings: Vec<String>,
    pub none_found: bool,
}

/// The full contract-case evidence for one contract-required row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractCaseEvidenceV1 {
    pub source_of_truth: String,
    pub spec_required_rationale: String,
    pub draft_spec_identity: String,
    pub author: IssueLifecycleContractRoleResultV1,
    pub adversary: IssueLifecycleContractAdversaryResultV1,
    pub open_decisions: Vec<String>,
    pub root_disposition: IssueLifecycleDispositionV1,
    pub contract_state: IssueLifecycleContractStateV1,
}

/// The bounded plan shape decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IssueLifecyclePlanShapeV1 {
    OnePr,
    Campaign,
}

/// One planned work item with its dependency edges.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecyclePlanWorkItemV1 {
    pub id: String,
    pub depends_on: Vec<String>,
}

/// One proof command with the exact denominator it must run against.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleProofCommandV1 {
    pub command: String,
    pub denominator: String,
}

/// The planning evidence retained for one row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecyclePlanEvidenceV1 {
    pub shape_decision: IssueLifecyclePlanShapeV1,
    pub shape_rationale: String,
    pub work_items: Vec<IssueLifecyclePlanWorkItemV1>,
    pub acceptance_covered: Vec<String>,
    pub acceptance_omitted: Vec<String>,
    pub edit_cages: Vec<String>,
    pub conflict_resources: Vec<String>,
    pub proof_commands: Vec<IssueLifecycleProofCommandV1>,
    pub stop_conditions: Vec<String>,
    pub non_goals: Vec<String>,
    pub portfolio_placement: String,
    pub root_corrections: Vec<String>,
    pub re_splits: Vec<String>,
}

/// One contract/plan row: the #4931 contract-case and planning evidence plus
/// the embedded RIPR-SPEC-0218 attempt the scorecard counts.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractPlanRowV1 {
    pub id: String,
    pub category: String,
    pub snapshot: IssueLifecycleIntakeSnapshotV1,
    pub current_main: String,
    pub contract: Option<IssueLifecycleContractCaseEvidenceV1>,
    pub planning: IssueLifecyclePlanEvidenceV1,
    pub retrieval_steps: Vec<IssueLifecycleIntakeRetrievalStepV1>,
    pub limitations: Vec<String>,
    pub non_claims: Vec<String>,
    pub attempt: IssueLifecycleAttemptV1,
}

/// The committed read-only contract/plan corpus: exactly two real rows.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractPlanCorpusV1 {
    pub schema_version: String,
    pub captured_at: String,
    pub base_main: String,
    pub rows: Vec<IssueLifecycleContractPlanRowV1>,
}

/// One synthetic mechanics control row (outside the two real rows).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractPlanControlRowV1 {
    pub id: String,
    pub control: String,
    pub scenario: String,
    pub row: IssueLifecycleContractPlanRowV1,
}

/// The committed mechanics control corpus; every embedded attempt is
/// synthetic and never enters a real denominator.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractPlanControlCorpusV1 {
    pub schema_version: String,
    pub rows: Vec<IssueLifecycleContractPlanControlRowV1>,
}

/// One provenance entry per real row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractPlanProvenanceRowV1 {
    pub issue: u64,
    pub category: String,
    pub justification: String,
}

/// Capture provenance: when, against which main, and why each row matches its
/// category.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractPlanProvenanceV1 {
    pub schema_version: String,
    pub repository: String,
    pub captured_at: String,
    pub base_main: String,
    pub capture_method: String,
    pub rows: Vec<IssueLifecycleContractPlanProvenanceRowV1>,
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

/// Load and shape-check the committed contract/plan corpus: exactly two rows,
/// the closed category set covered once each, unique issue numbers, real (not
/// synthetic) attempts.
pub(crate) fn load_issue_lifecycle_contract_plan_corpus(
    body: &str,
) -> Result<IssueLifecycleContractPlanCorpusV1, String> {
    let corpus: IssueLifecycleContractPlanCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse issue lifecycle contract plan corpus: {error}"))?;
    if corpus.schema_version != ISSUE_LIFECYCLE_CONTRACT_PLAN_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported issue lifecycle contract plan corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.captured_at.trim().is_empty() || corpus.base_main.trim().is_empty() {
        return Err(
            "issue lifecycle contract plan corpus must record captured_at and base_main"
                .to_string(),
        );
    }
    if corpus.rows.len() != REQUIRED_ISSUE_LIFECYCLE_CONTRACT_PLAN_CATEGORIES.len() {
        return Err(format!(
            "issue lifecycle contract plan corpus must carry exactly {} rows, got {}",
            REQUIRED_ISSUE_LIFECYCLE_CONTRACT_PLAN_CATEGORIES.len(),
            corpus.rows.len()
        ));
    }
    let mut categories = BTreeSet::new();
    let mut issues = BTreeSet::new();
    for row in &corpus.rows {
        if !categories.insert(row.category.clone()) {
            return Err(format!(
                "issue lifecycle contract plan corpus duplicates category `{}`",
                row.category
            ));
        }
        if !issues.insert(row.snapshot.issue_number) {
            return Err(format!(
                "issue lifecycle contract plan corpus duplicates issue `{}`",
                row.snapshot.issue_ref
            ));
        }
        if row.attempt.synthetic {
            return Err(format!(
                "issue lifecycle contract plan row `{}` must be a real row, not synthetic",
                row.id
            ));
        }
    }
    for required in REQUIRED_ISSUE_LIFECYCLE_CONTRACT_PLAN_CATEGORIES {
        if !categories.contains(required) {
            return Err(format!(
                "issue lifecycle contract plan corpus is missing category `{required}`"
            ));
        }
    }
    Ok(corpus)
}

/// Load and shape-check the mechanics control corpus: unique ids, the
/// `control_` id prefix, synthetic attempts only, and the full set of
/// required mechanics controls present — an empty or gutted control corpus
/// must fail closed rather than pass vacuously.
pub(crate) fn load_issue_lifecycle_contract_plan_control_corpus(
    body: &str,
) -> Result<IssueLifecycleContractPlanControlCorpusV1, String> {
    let corpus: IssueLifecycleContractPlanControlCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse issue lifecycle contract plan control corpus: {error}"))?;
    if corpus.schema_version != ISSUE_LIFECYCLE_CONTRACT_PLAN_CONTROL_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported issue lifecycle contract plan control corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.rows.is_empty() {
        return Err("issue lifecycle contract plan control corpus must not be empty".to_string());
    }
    let present: BTreeSet<&str> = corpus
        .rows
        .iter()
        .map(|control| control.control.as_str())
        .collect();
    for required in REQUIRED_ISSUE_LIFECYCLE_CONTRACT_PLAN_CONTROLS {
        if !present.contains(required) {
            return Err(format!(
                "issue lifecycle contract plan control corpus is missing required control `{required}`"
            ));
        }
    }
    let mut ids = BTreeSet::new();
    for control in &corpus.rows {
        if !control.id.starts_with("control_") {
            return Err(format!(
                "issue lifecycle contract plan control row `{}` must use the `control_` id prefix",
                control.id
            ));
        }
        if !ids.insert(control.id.clone()) {
            return Err(format!(
                "issue lifecycle contract plan control corpus duplicates id `{}`",
                control.id
            ));
        }
        if control.control.trim().is_empty() || control.scenario.trim().is_empty() {
            return Err(format!(
                "issue lifecycle contract plan control row `{}` must name its control and scenario",
                control.id
            ));
        }
        if !control.row.attempt.synthetic {
            return Err(format!(
                "issue lifecycle contract plan control row `{}` must be synthetic",
                control.id
            ));
        }
    }
    Ok(corpus)
}

/// Load and parse the committed provenance file.
pub(crate) fn load_issue_lifecycle_contract_plan_provenance(
    body: &str,
) -> Result<IssueLifecycleContractPlanProvenanceV1, String> {
    let provenance: IssueLifecycleContractPlanProvenanceV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse issue lifecycle contract plan provenance: {error}"))?;
    if provenance.schema_version != ISSUE_LIFECYCLE_CONTRACT_PLAN_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported issue lifecycle contract plan provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.base_main.trim().is_empty() || provenance.captured_at.trim().is_empty() {
        return Err(
            "issue lifecycle contract plan provenance must record captured_at and base_main"
                .to_string(),
        );
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "issue lifecycle contract plan provenance must record repository and capture_method"
                .to_string(),
        );
    }
    Ok(provenance)
}

fn identity_matches(identity: &str, scheme: &str, computed_hex: &str) -> bool {
    identity == format!("{scheme}:sha256:{computed_hex}")
}

/// Fail-closed snapshot binding: recompute the snapshot and comment digests
/// from the committed bytes and reject any drift. A row whose snapshot file
/// is missing or altered can never be counted. Timeline surfaces are
/// optional; retrieval-step byte claims for the three capture commands are
/// bound to the committed file sizes.
pub(crate) fn verify_contract_plan_row_snapshot(
    row: &IssueLifecycleContractPlanRowV1,
    root: &Path,
) -> Result<(), String> {
    let snapshot_path = row
        .snapshot
        .snapshot_path
        .as_ref()
        .ok_or_else(|| format!("contract plan row `{}` has no snapshot path", row.id))?;
    let comments_path = row
        .snapshot
        .comments_path
        .as_ref()
        .ok_or_else(|| format!("contract plan row `{}` has no comments path", row.id))?;
    if row.snapshot.captured_at.trim().is_empty() {
        return Err(format!(
            "contract plan row `{}` snapshot records no captured_at",
            row.id
        ));
    }
    let snapshot_body = fs::read(root.join(snapshot_path)).map_err(|error| {
        format!(
            "contract plan row `{}` snapshot {} is unreadable: {error}",
            row.id, snapshot_path
        )
    })?;
    let computed = crate::blind_journey::sha256_hex(&snapshot_body);
    if !identity_matches(
        &row.snapshot.issue_snapshot_id,
        "gh-issue-snapshot",
        &computed,
    ) {
        return Err(format!(
            "contract plan row `{}` snapshot digest drifted: recorded `{}`, recomputed `gh-issue-snapshot:sha256:{computed}`",
            row.id, row.snapshot.issue_snapshot_id
        ));
    }
    let comments_body = fs::read(root.join(comments_path)).map_err(|error| {
        format!(
            "contract plan row `{}` comments {} are unreadable: {error}",
            row.id, comments_path
        )
    })?;
    let computed = crate::blind_journey::sha256_hex(&comments_body);
    if !identity_matches(
        &row.snapshot.comments_snapshot_id,
        "gh-issue-comments",
        &computed,
    ) {
        return Err(format!(
            "contract plan row `{}` comments digest drifted: recorded `{}`, recomputed `gh-issue-comments:sha256:{computed}`",
            row.id, row.snapshot.comments_snapshot_id
        ));
    }
    let timeline_body = match &row.snapshot.timeline_path {
        Some(timeline_path) => Some(fs::read(root.join(timeline_path)).map_err(|error| {
            format!(
                "contract plan row `{}` timeline {} is unreadable: {error}",
                row.id, timeline_path
            )
        })?),
        None => None,
    };
    if let Some(timeline_body) = &timeline_body {
        let computed = crate::blind_journey::sha256_hex(timeline_body);
        let recorded = row
            .snapshot
            .timeline_snapshot_id
            .as_deref()
            .ok_or_else(|| format!("contract plan row `{}` timeline records no digest", row.id))?;
        if !identity_matches(recorded, "gh-issue-timeline", &computed) {
            return Err(format!(
                "contract plan row `{}` timeline digest drifted: recorded `{recorded}`, recomputed `gh-issue-timeline:sha256:{computed}`",
                row.id
            ));
        }
    }
    verify_retrieval_step_bytes(row, "issue", snapshot_body.len())?;
    verify_retrieval_step_bytes(row, "comments", comments_body.len())?;
    if let Some(timeline_body) = &timeline_body {
        verify_retrieval_step_bytes(row, "timeline", timeline_body.len())?;
    }
    Ok(())
}

/// Bind recorded retrieval-step byte claims to the committed snapshot bytes:
/// a step that names one of the three capture commands must carry a measured
/// byte count equal to the committed file it produced. A fabricated or stale
/// count fails closed.
fn verify_retrieval_step_bytes(
    row: &IssueLifecycleContractPlanRowV1,
    surface: &str,
    committed_len: usize,
) -> Result<(), String> {
    let suffix = match surface {
        "issue" => format!("/issues/{}", row.snapshot.issue_number),
        "comments" => format!("/issues/{}/comments", row.snapshot.issue_number),
        "timeline" => format!("/issues/{}/timeline", row.snapshot.issue_number),
        other => return Err(format!("unknown retrieval surface `{other}`")),
    };
    for step in &row.retrieval_steps {
        if !step.command.ends_with(&suffix) {
            continue;
        }
        if let crate::issue_lifecycle_intake::IssueLifecycleIntakeBytesV1::Measured(bytes) =
            step.bytes
            && bytes as usize != committed_len
        {
            return Err(format!(
                "contract plan row `{}` retrieval step `{}` claims {bytes} bytes, committed bytes measure {committed_len}",
                row.id, step.command
            ));
        }
    }
    Ok(())
}

/// One deterministic packet projection: everything a cold-start root needs
/// to reconstruct the contract/plan decision from the committed corpus alone,
/// with no chat, no live GitHub read and no selection signal. The projection
/// retains the draft identity, the distinct author/adversary result
/// identities, the inspected scope, the shape rationale, the edit cages, the
/// conflict resources, the non-goals and the row limitations alongside the
/// decisions themselves; snapshot-only signals (title, labels, age) never
/// enter it.
#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleContractPlanProjectionV1 {
    pub schema_version: String,
    pub id: String,
    pub issue_ref: String,
    pub category: String,
    pub current_main: String,
    pub contract_state: Option<IssueLifecycleContractStateV1>,
    pub root_disposition: Option<IssueLifecycleDispositionV1>,
    pub draft_spec_identity: Option<String>,
    pub author_result_identity: Option<String>,
    pub adversary_result_identity: Option<String>,
    pub adversary_inspected_scope: Option<String>,
    pub open_decisions: Vec<String>,
    pub adversary_findings: Vec<String>,
    pub adversary_none_found: Option<bool>,
    pub shape_decision: IssueLifecyclePlanShapeV1,
    pub shape_rationale: String,
    pub work_item_edges: Vec<String>,
    pub acceptance_covered: Vec<String>,
    pub acceptance_omitted: Vec<String>,
    pub proof_commands: Vec<IssueLifecycleProofCommandV1>,
    pub stop_conditions: Vec<String>,
    pub edit_cages: Vec<String>,
    pub conflict_resources: Vec<String>,
    pub non_goals: Vec<String>,
    pub portfolio_placement: String,
    pub root_corrections: Vec<String>,
    pub re_splits: Vec<String>,
    pub limitations: Vec<String>,
}

pub(crate) fn build_contract_plan_projection(
    row: &IssueLifecycleContractPlanRowV1,
) -> IssueLifecycleContractPlanProjectionV1 {
    let mut work_item_edges: Vec<String> = row
        .planning
        .work_items
        .iter()
        .flat_map(|item| {
            item.depends_on
                .iter()
                .map(move |dependency| format!("{}->{}", item.id, dependency))
        })
        .collect();
    work_item_edges.sort();
    IssueLifecycleContractPlanProjectionV1 {
        schema_version: "issue_lifecycle_contract_plan_packet.v1".to_string(),
        id: row.id.clone(),
        issue_ref: row.snapshot.issue_ref.clone(),
        category: row.category.clone(),
        current_main: row.current_main.clone(),
        contract_state: row
            .contract
            .as_ref()
            .map(|contract| contract.contract_state),
        root_disposition: row
            .contract
            .as_ref()
            .map(|contract| contract.root_disposition),
        draft_spec_identity: row
            .contract
            .as_ref()
            .map(|contract| contract.draft_spec_identity.clone()),
        author_result_identity: row
            .contract
            .as_ref()
            .map(|contract| contract.author.result_identity.clone()),
        adversary_result_identity: row
            .contract
            .as_ref()
            .map(|contract| contract.adversary.result_identity.clone()),
        adversary_inspected_scope: row
            .contract
            .as_ref()
            .map(|contract| contract.adversary.inspected_scope.clone()),
        open_decisions: row
            .contract
            .as_ref()
            .map_or_else(Vec::new, |contract| contract.open_decisions.clone()),
        adversary_findings: row
            .contract
            .as_ref()
            .map_or_else(Vec::new, |contract| contract.adversary.findings.clone()),
        adversary_none_found: row
            .contract
            .as_ref()
            .map(|contract| contract.adversary.none_found),
        shape_decision: row.planning.shape_decision,
        shape_rationale: row.planning.shape_rationale.clone(),
        work_item_edges,
        acceptance_covered: row.planning.acceptance_covered.clone(),
        acceptance_omitted: row.planning.acceptance_omitted.clone(),
        proof_commands: row.planning.proof_commands.clone(),
        stop_conditions: row.planning.stop_conditions.clone(),
        edit_cages: row.planning.edit_cages.clone(),
        conflict_resources: row.planning.conflict_resources.clone(),
        non_goals: row.planning.non_goals.clone(),
        portfolio_placement: row.planning.portfolio_placement.clone(),
        root_corrections: row.planning.root_corrections.clone(),
        re_splits: row.planning.re_splits.clone(),
        limitations: row.limitations.clone(),
    }
}

/// Every planning string surface the behavior-authority and selection-file
/// laws scan; plans carry work order, never new behavior authority, and never
/// reference a tracked selection or current-work file. Proof commands scan
/// too: a proof command or denominator minting behavior authority or naming
/// a selection file fails closed like any other planning surface.
fn planning_strings(planning: &IssueLifecyclePlanEvidenceV1) -> Vec<String> {
    let mut strings = vec![
        planning.shape_rationale.clone(),
        planning.portfolio_placement.clone(),
    ];
    strings.extend(planning.acceptance_covered.iter().cloned());
    strings.extend(planning.acceptance_omitted.iter().cloned());
    strings.extend(planning.edit_cages.iter().cloned());
    strings.extend(planning.conflict_resources.iter().cloned());
    strings.extend(planning.stop_conditions.iter().cloned());
    strings.extend(planning.non_goals.iter().cloned());
    strings.extend(planning.root_corrections.iter().cloned());
    strings.extend(planning.re_splits.iter().cloned());
    strings.extend(planning.work_items.iter().map(|item| item.id.clone()));
    for proof in &planning.proof_commands {
        strings.push(proof.command.clone());
        strings.push(proof.denominator.clone());
    }
    strings
}

/// Per-row decision-boundary law over the contract-case and planning
/// evidence plus the embedded RIPR-SPEC-0218 attempt. Returns every failure;
/// an empty vector means the row is internally consistent, the roles are
/// separated and the root disposition is genuinely independent.
pub(crate) fn assess_contract_plan_row(row: &IssueLifecycleContractPlanRowV1) -> Vec<String> {
    let mut failures = Vec::new();
    match row.category.as_str() {
        "contract_required" => {
            let contract = match &row.contract {
                Some(contract) => contract,
                None => {
                    failures.push(format!(
                        "row `{}` is contract-required but records no contract evidence",
                        row.id
                    ));
                    return failures;
                }
            };
            if !row.attempt.contract_decision.spec_required {
                failures.push(format!(
                    "row `{}` is contract-required but the attempt records no spec-required decision",
                    row.id
                ));
            }
            if contract.root_disposition != row.attempt.disposition {
                failures.push(format!(
                    "row `{}` contract root disposition {:?} disagrees with the attempt disposition {:?}",
                    row.id, contract.root_disposition, row.attempt.disposition
                ));
            }
            if contract.author.carries_acceptance {
                failures.push(format!(
                    "row `{}` author result `{}` carries an acceptance state; the author cannot accept its own contract",
                    row.id, contract.author.result_identity
                ));
            }
            if contract.author.result_identity == contract.adversary.result_identity {
                failures.push(format!(
                    "row `{}` author and adversary results must be distinct identities",
                    row.id
                ));
            }
            if contract.author.role.trim().is_empty()
                || contract.author.config_identity.trim().is_empty()
                || contract.author.result_identity.trim().is_empty()
            {
                failures.push(format!(
                    "row `{}` author role/config/result identity must be complete",
                    row.id
                ));
            }
            if contract.adversary.inspected_scope.trim().is_empty() {
                failures.push(format!(
                    "row `{}` adversary records no inspected scope",
                    row.id
                ));
            }
            let findings_reported = !contract.adversary.findings.is_empty();
            if !(contract.adversary.none_found ^ findings_reported) {
                failures.push(format!(
                    "row `{}` adversary findings and bounded none_found must be mutually exclusive",
                    row.id
                ));
            }
            if !contract.draft_spec_identity.starts_with("RIPR-SPEC-draft-") {
                failures.push(format!(
                    "row `{}` draft identity `{}` is not a behavior draft; specs contain behavior, not execution queues",
                    row.id, contract.draft_spec_identity
                ));
            }
            if row.attempt.disposition == IssueLifecycleDispositionV1::QualifiedOnePr {
                failures.push(format!(
                    "row `{}` is contract-required and can never qualify as direct one-PR work",
                    row.id
                ));
            }
            if !contract.open_decisions.is_empty() {
                if row.planning.stop_conditions.is_empty() {
                    failures.push(format!(
                        "row `{}` preserves open decisions but records no stop condition; unresolved decisions stop the plan rather than being guessed",
                        row.id
                    ));
                }
                if matches!(
                    row.attempt.disposition,
                    IssueLifecycleDispositionV1::Completed
                        | IssueLifecycleDispositionV1::QualifiedOnePr
                ) {
                    failures.push(format!(
                        "row `{}` binds open decisions but claims an implementation-ready disposition {:?}",
                        row.id, row.attempt.disposition
                    ));
                }
                if row.attempt.contract_artifacts.acceptance.is_some() {
                    failures.push(format!(
                        "row `{}` records contract acceptance evidence while open decisions remain; acceptance cannot outrun the root disposition",
                        row.id
                    ));
                }
            }
        }
        "narrow_accepted_contract_bug" => {
            if row.contract.is_some() {
                failures.push(format!(
                    "row `{}` is a narrow bug but records contract evidence; no unnecessary spec",
                    row.id
                ));
            }
            if row.attempt.contract_decision.spec_required {
                failures.push(format!(
                    "row `{}` narrow bug must not require a spec",
                    row.id
                ));
            }
            let artifacts = &row.attempt.contract_artifacts;
            if artifacts.proposal.is_some()
                || artifacts.spec.is_some()
                || artifacts.adr.is_some()
                || artifacts.challenge.is_some()
                || !artifacts.amendments.is_empty()
                || artifacts.acceptance.is_some()
            {
                failures.push(format!(
                    "row `{}` narrow bug must carry no contract artifacts",
                    row.id
                ));
            }
            if row.attempt.disposition != IssueLifecycleDispositionV1::QualifiedOnePr {
                failures.push(format!(
                    "row `{}` narrow bug must qualify as one-PR work, got {:?}",
                    row.id, row.attempt.disposition
                ));
            }
            if row.planning.shape_decision != IssueLifecyclePlanShapeV1::OnePr {
                failures.push(format!(
                    "row `{}` narrow bug must plan as one PR, got {:?}",
                    row.id, row.planning.shape_decision
                ));
            }
        }
        other => failures.push(format!("row `{}` has unknown category `{other}`", row.id)),
    }
    // Planning law: work order, not behavior authority. No planning surface
    // may mint a draft spec identity; draft specs live on the contract
    // evidence only.
    for string in planning_strings(&row.planning) {
        if string.contains("RIPR-SPEC-draft") {
            failures.push(format!(
                "row `{}` planning surface mints behavior authority (`{string}`); plans contain work order, not new behavior authority",
                row.id
            ));
        }
        if string.contains("active.toml") || string.contains("selected_work") {
            failures.push(format!(
                "row `{}` planning surface references a tracked selection or current-work file (`{string}`)",
                row.id
            ));
        }
    }
    // Dependency-edge law: every depends_on target must name another work
    // item in the same plan; a dangling edge is a guessed dependency.
    let work_item_ids: BTreeSet<&str> = row
        .planning
        .work_items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    for item in &row.planning.work_items {
        for dependency in &item.depends_on {
            if !work_item_ids.contains(dependency.as_str()) {
                failures.push(format!(
                    "row `{}` work item `{}` depends on unknown work item `{dependency}`",
                    row.id, item.id
                ));
            }
        }
    }
    // Drift law: the outer planning evidence and the embedded RIPR-SPEC-0218
    // attempt plan must describe the same work; a plan that drifted from the
    // attempt it claims to plan fails closed.
    let mut planned_ids: Vec<&str> = row
        .planning
        .work_items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    let mut attempt_ids: Vec<&str> = row
        .attempt
        .plan
        .work_items
        .iter()
        .map(String::as_str)
        .collect();
    planned_ids.sort_unstable();
    attempt_ids.sort_unstable();
    if planned_ids != attempt_ids {
        failures.push(format!(
            "row `{}` planning work items drifted from the embedded attempt plan work items",
            row.id
        ));
    }
    let mut planned_edges: Vec<String> = row
        .planning
        .work_items
        .iter()
        .flat_map(|item| {
            item.depends_on
                .iter()
                .map(move |dependency| format!("{}->{dependency}", item.id))
        })
        .collect();
    let mut attempt_edges = row.attempt.plan.dependencies.clone();
    planned_edges.sort();
    attempt_edges.sort();
    if planned_edges != attempt_edges {
        failures.push(format!(
            "row `{}` planning dependency edges drifted from the embedded attempt plan dependencies",
            row.id
        ));
    }
    let mut planned_coverage = row.planning.acceptance_covered.clone();
    let mut attempt_coverage = row.attempt.plan.acceptance_coverage.clone();
    planned_coverage.sort();
    attempt_coverage.sort();
    if planned_coverage != attempt_coverage {
        failures.push(format!(
            "row `{}` planning acceptance coverage drifted from the embedded attempt plan acceptance coverage",
            row.id
        ));
    }
    // Acceptance law: explicit omissions must carry a reason; an omitted row
    // is disclosed, never hidden.
    for omitted in &row.planning.acceptance_omitted {
        if omitted.trim().is_empty() || !omitted.contains(':') {
            failures.push(format!(
                "row `{}` acceptance omission `{omitted}` must name the omitted row and its reason after a colon",
                row.id
            ));
        }
    }
    // Proof law: every proof command must name its denominator.
    for proof in &row.planning.proof_commands {
        if proof.command.trim().is_empty() || proof.denominator.trim().is_empty() {
            failures.push(format!(
                "row `{}` proof commands must name the command and its denominator",
                row.id
            ));
        }
    }
    failures
}

/// Assess every row of the committed corpus plus every synthetic control row
/// by the same decision-boundary law.
pub(crate) fn assess_contract_plan_corpus(
    corpus: &IssueLifecycleContractPlanCorpusV1,
) -> Vec<String> {
    let mut failures = Vec::new();
    for row in &corpus.rows {
        failures.extend(assess_contract_plan_row(row));
    }
    failures
}

pub(crate) fn assess_contract_plan_control_corpus(
    corpus: &IssueLifecycleContractPlanControlCorpusV1,
) -> Vec<String> {
    let mut failures = Vec::new();
    for control in &corpus.rows {
        failures.extend(assess_contract_plan_row(&control.row));
    }
    failures
}

fn parse_corpus_dir_arg(args: &[String]) -> Result<String, String> {
    const USAGE: &str =
        "usage: cargo xtask issue-lifecycle-contract-plan-scorecard [--corpus <dir>]";
    let mut corpus = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--corpus" | "--captured" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for {}\n{USAGE}", args[index]))?;
                if corpus.replace(value.clone()).is_some() {
                    return Err(format!("duplicate corpus path\n{USAGE}"));
                }
                index += 2;
            }
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    Ok(corpus.unwrap_or_else(|| DEFAULT_CONTRACT_PLAN_CORPUS_DIR.to_string()))
}

/// Fail-closed provenance shape check: the provenance must name exactly the
/// corpus rows — no missing entry, no duplicate issue, and no extra row the
/// corpus does not contain — and must record the same capture instant and
/// base main as the corpus it describes.
pub(crate) fn check_provenance_against_corpus(
    corpus: &IssueLifecycleContractPlanCorpusV1,
    provenance: &IssueLifecycleContractPlanProvenanceV1,
) -> Result<(), String> {
    if provenance.base_main != corpus.base_main {
        return Err(format!(
            "contract plan provenance base main `{}` disagrees with the corpus base main `{}`",
            provenance.base_main, corpus.base_main
        ));
    }
    if provenance.captured_at != corpus.captured_at {
        return Err(format!(
            "contract plan provenance captured at `{}` disagrees with the corpus captured at `{}`",
            provenance.captured_at, corpus.captured_at
        ));
    }
    if provenance.rows.len() != corpus.rows.len() {
        return Err(format!(
            "contract plan provenance must name {} rows, got {}",
            corpus.rows.len(),
            provenance.rows.len()
        ));
    }
    let mut provenance_issues = BTreeSet::new();
    for entry in &provenance.rows {
        if !provenance_issues.insert(entry.issue) {
            return Err(format!(
                "contract plan provenance duplicates issue `{}`",
                entry.issue
            ));
        }
    }
    for row in &corpus.rows {
        if !provenance_issues.contains(&row.snapshot.issue_number) {
            return Err(format!(
                "contract plan row `{}` has no provenance entry for issue `{}`",
                row.id, row.snapshot.issue_ref
            ));
        }
        let entry = provenance
            .rows
            .iter()
            .find(|entry| entry.issue == row.snapshot.issue_number)
            .map(|entry| entry.category.clone())
            .unwrap_or_default();
        if entry != row.category {
            return Err(format!(
                "contract plan row `{}` category `{}` disagrees with its provenance category `{entry}`",
                row.id, row.category
            ));
        }
    }
    Ok(())
}

/// Read one committed contract/plan corpus directory end to end: corpus,
/// controls, provenance and snapshot bindings, all fail-closed. Shared by the
/// scorecard command and the fixture-contract gate.
pub(crate) fn load_contract_plan_corpus_dir(
    dir: &Path,
) -> Result<
    (
        IssueLifecycleContractPlanCorpusV1,
        IssueLifecycleContractPlanControlCorpusV1,
        IssueLifecycleContractPlanProvenanceV1,
    ),
    String,
> {
    let corpus_body = fs::read_to_string(dir.join("corpus.json"))
        .map_err(|error| format!("read issue lifecycle contract plan corpus: {error}"))?;
    let corpus = load_issue_lifecycle_contract_plan_corpus(&corpus_body)?;
    let controls_body = fs::read_to_string(dir.join("controls.json"))
        .map_err(|error| format!("read issue lifecycle contract plan controls: {error}"))?;
    let controls = load_issue_lifecycle_contract_plan_control_corpus(&controls_body)?;
    let provenance_body = fs::read_to_string(dir.join("provenance.json"))
        .map_err(|error| format!("read issue lifecycle contract plan provenance: {error}"))?;
    let provenance = load_issue_lifecycle_contract_plan_provenance(&provenance_body)?;
    check_provenance_against_corpus(&corpus, &provenance)?;
    for row in &corpus.rows {
        verify_contract_plan_row_snapshot(row, dir)?;
    }
    Ok((corpus, controls, provenance))
}

/// Fail-closed RIPR-SPEC-0218 assessment of the embedded real attempt rows:
/// a rejected row fails the run, and a counted row whose deterministically
/// downgraded assessed disposition disagrees with the row's independent root
/// disposition fails too — the command never emits a scorecard stronger than
/// the retained evidence. For contract-required rows the root disposition
/// lives on the contract evidence; narrow rows bind no separate root.
pub(crate) fn assess_real_rows_against_counting_law(
    corpus: &IssueLifecycleContractPlanCorpusV1,
) -> (Vec<String>, Vec<IssueLifecycleRowAssessmentV1>) {
    let attempts: Vec<IssueLifecycleAttemptV1> =
        corpus.rows.iter().map(|row| row.attempt.clone()).collect();
    let assessed: Vec<IssueLifecycleRowAssessmentV1> = attempts
        .iter()
        .map(assess_issue_lifecycle_attempt)
        .collect();
    let mut failures = Vec::new();
    for (row, assessment) in corpus.rows.iter().zip(assessed.iter()) {
        if !assessment.counted {
            failures.push(format!(
                "contract plan row `{}` attempt was rejected by the RIPR-SPEC-0218 counting law: {:?}",
                row.id, assessment.reasons
            ));
        }
        let root = row
            .contract
            .as_ref()
            .map(|contract| contract.root_disposition)
            .unwrap_or(row.attempt.disposition);
        if assessment.disposition != Some(root) {
            failures.push(format!(
                "contract plan row `{}` root disposition {:?} disagrees with the assessed disposition {:?}",
                row.id, root, assessment.disposition
            ));
        }
    }
    (failures, assessed)
}

/// `cargo xtask issue-lifecycle-contract-plan-scorecard [--corpus <dir>]`
/// (#4931, RIPR-SPEC-0232): validate the committed read-only contract/plan
/// corpus fail-closed (shape, provenance, snapshot digest bindings,
/// decision-boundary law), then project the embedded real attempt rows
/// through the unchanged RIPR-SPEC-0218 validator and scorecard builder,
/// writing the standard issue-lifecycle scorecard reports — results flow
/// through #4929 without a parallel contract/plan report. The deterministic
/// per-row contract/plan projections are printed to stdout.
pub(crate) fn issue_lifecycle_contract_plan_scorecard(args: &[String]) -> Result<(), String> {
    let corpus_dir = parse_corpus_dir_arg(args)?;
    let root = workspace_path(&corpus_dir);
    let (corpus, controls, _provenance) = load_contract_plan_corpus_dir(&root)?;
    let mut failures = assess_contract_plan_corpus(&corpus);
    failures.extend(assess_contract_plan_control_corpus(&controls));
    let (law_failures, assessed) = assess_real_rows_against_counting_law(&corpus);
    failures.extend(law_failures);
    let attempts: Vec<IssueLifecycleAttemptV1> =
        corpus.rows.iter().map(|row| row.attempt.clone()).collect();
    for control in &controls.rows {
        let assessment = assess_issue_lifecycle_attempt(&control.row.attempt);
        if !assessment.counted {
            failures.push(format!(
                "contract plan control `{}` was rejected by the RIPR-SPEC-0218 counting law: {:?}",
                control.id, assessment.reasons
            ));
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "issue lifecycle contract plan corpus failed assessment: {failures:?}"
        ));
    }
    let corpus_identity = crate::reports::issue_lifecycle_corpus_identity(&attempts)?;
    let scorecard =
        crate::reports::build_issue_lifecycle_scorecard(&assessed, corpus_identity, &corpus_dir);
    let json_body = crate::reports::issue_lifecycle_scorecard_json(&scorecard)?;
    crate::write_report("issue-lifecycle-scorecard.json", &json_body)?;
    crate::write_report(
        "issue-lifecycle-scorecard.md",
        &crate::reports::issue_lifecycle_scorecard_markdown(&scorecard),
    )?;
    println!("{json_body}");
    println!("{}", ISSUE_LIFECYCLE_CONTRACT_PLAN_CLAIM_BOUNDARY);
    let mut projections: Vec<IssueLifecycleContractPlanProjectionV1> = corpus
        .rows
        .iter()
        .map(build_contract_plan_projection)
        .collect();
    projections.sort_by(|left, right| left.id.cmp(&right.id));
    let packet_body = serde_json::to_string_pretty(&projections)
        .map_err(|error| format!("serialize contract plan projections: {error}"))?;
    println!("{packet_body}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue_lifecycle_intake::IssueLifecycleIntakeBytesV1;

    fn contract_plan_root() -> PathBuf {
        workspace_path(DEFAULT_CONTRACT_PLAN_CORPUS_DIR)
    }

    fn load_committed() -> Result<
        (
            IssueLifecycleContractPlanCorpusV1,
            IssueLifecycleContractPlanControlCorpusV1,
            IssueLifecycleContractPlanProvenanceV1,
        ),
        String,
    > {
        load_contract_plan_corpus_dir(&contract_plan_root())
    }

    fn row_by_category<'a>(
        corpus: &'a IssueLifecycleContractPlanCorpusV1,
        category: &str,
    ) -> Result<&'a IssueLifecycleContractPlanRowV1, String> {
        corpus
            .rows
            .iter()
            .find(|row| row.category == category)
            .ok_or_else(|| format!("committed corpus is missing category `{category}`"))
    }

    fn control_by_id<'a>(
        controls: &'a IssueLifecycleContractPlanControlCorpusV1,
        id: &str,
    ) -> Result<&'a IssueLifecycleContractPlanControlRowV1, String> {
        controls
            .rows
            .iter()
            .find(|control| control.id == id)
            .ok_or_else(|| format!("committed control corpus is missing control `{id}`"))
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_committed_corpus_loads_with_two_categories()
    -> Result<(), String> {
        let (corpus, controls, provenance) = load_committed()?;
        if corpus.rows.len() != 2 {
            return Err(format!("expected two real rows, got {}", corpus.rows.len()));
        }
        if provenance.rows.len() != 2 {
            return Err(format!(
                "expected two provenance rows, got {}",
                provenance.rows.len()
            ));
        }
        for control in &controls.rows {
            if !control.row.attempt.synthetic {
                return Err(format!("control row `{}` must be synthetic", control.id));
            }
        }
        for row in &corpus.rows {
            if row.attempt.synthetic {
                return Err(format!("real row `{}` must not be synthetic", row.id));
            }
        }
        if !assess_contract_plan_corpus(&corpus).is_empty() {
            return Err("the committed real corpus must assess clean".to_string());
        }
        if !assess_contract_plan_control_corpus(&controls).is_empty() {
            return Err("the committed control corpus must assess clean".to_string());
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_contract_case_runs_author_adversary_and_root()
    -> Result<(), String> {
        let (corpus, _controls, _provenance) = load_committed()?;
        let row = row_by_category(&corpus, "contract_required")?;
        if row.snapshot.issue_number != 6225 {
            return Err(format!(
                "the contract case must bind issue 6225, got {}",
                row.snapshot.issue_number
            ));
        }
        let contract = row
            .contract
            .as_ref()
            .ok_or_else(|| "the contract case must carry contract evidence".to_string())?;
        if contract.author.carries_acceptance {
            return Err("the author result must not carry an acceptance state".to_string());
        }
        if contract.author.result_identity == contract.adversary.result_identity {
            return Err("author and adversary results must be distinct identities".to_string());
        }
        if contract.adversary.inspected_scope.trim().is_empty() {
            return Err("the adversary must record its inspected scope".to_string());
        }
        if contract.adversary.findings.is_empty() {
            return Err(
                "the adversary must return concrete missing failure/limited findings for this case"
                    .to_string(),
            );
        }
        if contract.adversary.none_found {
            return Err("a findings-carrying adversary must not claim none_found".to_string());
        }
        if contract.open_decisions.is_empty() {
            return Err("the contract case must preserve its open decisions".to_string());
        }
        if contract.root_disposition != IssueLifecycleDispositionV1::QualifiedSpecRequired {
            return Err(format!(
                "the root disposition must stay qualified_spec_required, got {:?}",
                contract.root_disposition
            ));
        }
        if contract.contract_state != IssueLifecycleContractStateV1::Amended {
            return Err(format!(
                "the root amended the draft per the adversary findings, got {:?}",
                contract.contract_state
            ));
        }
        if row.attempt.contract_artifacts.acceptance.is_some() {
            return Err("no acceptance artifact may exist while open decisions remain".to_string());
        }
        if row.planning.acceptance_omitted.is_empty() {
            return Err("the contract plan must disclose its omitted acceptance rows".to_string());
        }
        let projection = build_contract_plan_projection(row);
        if projection.draft_spec_identity.as_deref() != Some(contract.draft_spec_identity.as_str())
        {
            return Err("the projection must retain the draft spec identity".to_string());
        }
        if projection.author_result_identity.as_deref() != Some("draft-result-6225-author-v1") {
            return Err("the projection must retain the author result identity".to_string());
        }
        if projection.adversary_result_identity.as_deref()
            != Some("challenge-result-6225-adversary-v1")
        {
            return Err("the projection must retain the adversary result identity".to_string());
        }
        if projection.adversary_inspected_scope.as_deref()
            != Some(contract.adversary.inspected_scope.as_str())
        {
            return Err("the projection must retain the adversary inspected scope".to_string());
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_narrow_case_routes_directly_to_one_pr()
    -> Result<(), String> {
        let (corpus, _controls, _provenance) = load_committed()?;
        let row = row_by_category(&corpus, "narrow_accepted_contract_bug")?;
        if row.snapshot.issue_number != 6180 {
            return Err(format!(
                "the narrow case must bind issue 6180, got {}",
                row.snapshot.issue_number
            ));
        }
        if row.contract.is_some() {
            return Err("the narrow bug must carry no contract evidence".to_string());
        }
        if row.attempt.contract_decision.spec_required {
            return Err("the narrow bug must not require a spec".to_string());
        }
        let artifacts = &row.attempt.contract_artifacts;
        if artifacts.proposal.is_some()
            || artifacts.spec.is_some()
            || artifacts.adr.is_some()
            || artifacts.challenge.is_some()
            || !artifacts.amendments.is_empty()
            || artifacts.acceptance.is_some()
        {
            return Err("the narrow bug must carry no contract artifacts".to_string());
        }
        if row.attempt.disposition != IssueLifecycleDispositionV1::QualifiedOnePr {
            return Err(format!(
                "the narrow bug must qualify as one-PR work, got {:?}",
                row.attempt.disposition
            ));
        }
        if row.planning.shape_decision != IssueLifecyclePlanShapeV1::OnePr {
            return Err("the narrow bug must plan as one PR".to_string());
        }
        if !row.planning.acceptance_omitted.is_empty() {
            return Err("the narrow bug must omit no acceptance rows".to_string());
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_adversary_findings_and_bounded_none_found_exclusive()
    -> Result<(), String> {
        let (_corpus, controls, _provenance) = load_committed()?;
        let caught = control_by_id(&controls, "control_adversary_catches_missing_failure_state")?;
        if caught.row.contract.is_none() {
            return Err("the catch control must carry contract evidence".to_string());
        }
        let contract = caught
            .row
            .contract
            .as_ref()
            .ok_or_else(|| "the catch control must carry contract evidence".to_string())?;
        if contract.adversary.findings.is_empty() || contract.adversary.none_found {
            return Err(
                "the catch control must carry concrete findings without none_found".to_string(),
            );
        }
        let none = control_by_id(&controls, "control_adversary_bounded_none_found")?;
        let none_contract = none
            .row
            .contract
            .as_ref()
            .ok_or_else(|| "the none-found control must carry contract evidence".to_string())?;
        if !none_contract.adversary.findings.is_empty() || !none_contract.adversary.none_found {
            return Err(
                "the none-found control must return bounded none_found with an empty finding list"
                    .to_string(),
            );
        }
        if none_contract.contract_state != IssueLifecycleContractStateV1::Accepted {
            return Err("a clean challenge supports an accepted root state".to_string());
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_author_cannot_accept_own_contract() -> Result<(), String>
    {
        let (corpus, controls, _provenance) = load_committed()?;
        for row in corpus
            .rows
            .iter()
            .chain(controls.rows.iter().map(|control| &control.row))
        {
            if let Some(contract) = &row.contract {
                if contract.author.carries_acceptance {
                    return Err(format!(
                        "row `{}` author result carries an acceptance state",
                        row.id
                    ));
                }
                if contract.author.role != "contract_author" {
                    return Err(format!(
                        "row `{}` author role must be contract_author, got {}",
                        row.id, contract.author.role
                    ));
                }
            }
        }
        let mut row = row_by_category(&corpus, "contract_required")?.clone();
        let contract = row
            .contract
            .as_mut()
            .ok_or_else(|| "the contract case must carry contract evidence".to_string())?;
        contract.author.carries_acceptance = true;
        let failures = assess_contract_plan_row(&row);
        if !failures
            .iter()
            .any(|failure| failure.contains("cannot accept its own contract"))
        {
            return Err(format!(
                "an author-carrying-acceptance row must fail the law, got {failures:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_unresolved_decision_blocks_implementation()
    -> Result<(), String> {
        let (corpus, _controls, _provenance) = load_committed()?;
        let row = row_by_category(&corpus, "contract_required")?;
        let contract = row
            .contract
            .as_ref()
            .ok_or_else(|| "the contract case must carry contract evidence".to_string())?;
        if contract.open_decisions.is_empty() {
            return Err("the committed contract row must preserve open decisions".to_string());
        }
        if matches!(
            row.attempt.disposition,
            IssueLifecycleDispositionV1::Completed | IssueLifecycleDispositionV1::QualifiedOnePr
        ) {
            return Err("open decisions must block implementation-ready dispositions".to_string());
        }
        if row.planning.stop_conditions.is_empty() {
            return Err("open decisions must bind stop conditions".to_string());
        }
        // A spec-required row qualified as one-PR work fails even with the
        // open decisions preserved: the disposition cap binds by category,
        // not by the open-decision gate alone.
        let mut qualified = row.clone();
        qualified.attempt.disposition = IssueLifecycleDispositionV1::QualifiedOnePr;
        let failures = assess_contract_plan_row(&qualified);
        if !failures
            .iter()
            .any(|failure| failure.contains("can never qualify as direct one-PR work"))
        {
            return Err(format!(
                "a spec-required row qualified as one-PR work must fail the law, got {failures:?}"
            ));
        }
        // Each open-decision guard must fire on its own mutation.
        let mut no_stop = row.clone();
        no_stop.planning.stop_conditions.clear();
        let failures = assess_contract_plan_row(&no_stop);
        if !failures
            .iter()
            .any(|failure| failure.contains("records no stop condition"))
        {
            return Err(format!(
                "a row preserving open decisions without stop conditions must fail the law, got {failures:?}"
            ));
        }
        let mut completed = row.clone();
        completed.attempt.disposition = IssueLifecycleDispositionV1::Completed;
        let failures = assess_contract_plan_row(&completed);
        if !failures
            .iter()
            .any(|failure| failure.contains("claims an implementation-ready disposition"))
        {
            return Err(format!(
                "a row preserving open decisions with a completed disposition must fail the law, got {failures:?}"
            ));
        }
        let mut acceptance = row.clone();
        acceptance.attempt.contract_artifacts.acceptance = Some("acceptance-x".to_string());
        let failures = assess_contract_plan_row(&acceptance);
        if !failures
            .iter()
            .any(|failure| failure.contains("acceptance cannot outrun the root disposition"))
        {
            return Err(format!(
                "a row preserving open decisions with acceptance evidence must fail the law, got {failures:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_campaign_and_one_pr_shapes_rejected_when_incoherent()
    -> Result<(), String> {
        let (_corpus, controls, _provenance) = load_committed()?;
        let campaign_rejected =
            control_by_id(&controls, "control_campaign_rejected_when_slice_suffices")?;
        if campaign_rejected.row.planning.shape_decision != IssueLifecyclePlanShapeV1::OnePr {
            return Err(
                "the rejected-campaign control must land on one_pr with its rationale".to_string(),
            );
        }
        if !campaign_rejected
            .row
            .planning
            .shape_rationale
            .contains("rejected")
        {
            return Err(
                "the rejected-campaign control must record the rejection rationale".to_string(),
            );
        }
        let one_pr_rejected = control_by_id(&controls, "control_one_pr_rejected_when_incoherent")?;
        if one_pr_rejected.row.planning.shape_decision != IssueLifecyclePlanShapeV1::Campaign {
            return Err(
                "the rejected-one-PR control must land on campaign with its rationale".to_string(),
            );
        }
        if !one_pr_rejected
            .row
            .planning
            .shape_rationale
            .contains("rejected")
        {
            return Err(
                "the rejected-one-PR control must record the rejection rationale".to_string(),
            );
        }
        // The real contract row considered and rejected a campaign on the
        // record: its shape rationale must name the rejection.
        let (corpus, _controls, _provenance) = load_committed()?;
        let contract_row = row_by_category(&corpus, "contract_required")?;
        if contract_row.planning.shape_decision != IssueLifecyclePlanShapeV1::OnePr {
            return Err("the real contract row must land on one_pr".to_string());
        }
        if !contract_row.planning.shape_rationale.contains("rejected") {
            return Err("the real contract row must record the campaign rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_specs_behavior_and_plans_work_order_laws()
    -> Result<(), String> {
        let (corpus, _controls, _provenance) = load_committed()?;
        let row = row_by_category(&corpus, "contract_required")?;
        let contract = row
            .contract
            .as_ref()
            .ok_or_else(|| "the contract case must carry contract evidence".to_string())?;
        if !contract.draft_spec_identity.starts_with("RIPR-SPEC-draft-") {
            return Err("the draft spec identity must be a behavior draft".to_string());
        }
        // Spec law: a queue-shaped draft identity fails closed.
        let mut mutated = row.clone();
        let draft_identity = {
            let contract = mutated
                .contract
                .as_mut()
                .ok_or_else(|| "the contract case must carry contract evidence".to_string())?;
            contract.draft_spec_identity = "work-order-queue-1".to_string();
            contract.draft_spec_identity.clone()
        };
        let failures = assess_contract_plan_row(&mutated);
        if !failures
            .iter()
            .any(|failure| failure.contains("specs contain behavior, not execution queues"))
        {
            return Err(format!(
                "a queue-shaped draft identity must fail the spec law, got {failures:?}"
            ));
        }
        // Plan law: a plan minting behavior authority fails closed. The
        // rationale must name the real behavior draft identity so the
        // planning-surface scan sees the minted authority.
        let mut mutated_plan = row.clone();
        mutated_plan.planning.shape_rationale = format!(
            "implements {} as the behavior authority",
            contract.draft_spec_identity
        );
        let failures = assess_contract_plan_row(&mutated_plan);
        if !failures
            .iter()
            .any(|failure| failure.contains("plans contain work order, not new behavior authority"))
        {
            return Err(format!(
                "a plan minting a draft spec identity must fail the plan law, got {failures:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_no_tracked_selection_file_changed() -> Result<(), String>
    {
        let (corpus, controls, _provenance) = load_committed()?;
        for row in corpus
            .rows
            .iter()
            .chain(controls.rows.iter().map(|control| &control.row))
        {
            for string in planning_strings(&row.planning) {
                if string.contains("active.toml") || string.contains("selected_work") {
                    return Err(format!(
                        "row `{}` planning surface references a tracked selection file: {string}",
                        row.id
                    ));
                }
            }
        }
        let mut mutated = row_by_category(&corpus, "narrow_accepted_contract_bug")?.clone();
        mutated.planning.portfolio_placement = "writes active.toml to select this work".to_string();
        let failures = assess_contract_plan_row(&mutated);
        if !failures
            .iter()
            .any(|failure| failure.contains("tracked selection or current-work file"))
        {
            return Err(format!(
                "a selection-file mutation must fail the law, got {failures:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_cold_start_root_reconstructs_decisions_without_chat()
    -> Result<(), String> {
        let (corpus, _controls, _provenance) = load_committed()?;
        let first: Vec<String> = corpus
            .rows
            .iter()
            .map(|row| {
                serde_json::to_string(&build_contract_plan_projection(row))
                    .map_err(|error| format!("serialize contract plan projection: {error}"))
            })
            .collect::<Result<_, _>>()?;
        let reloaded = load_contract_plan_corpus_dir(&contract_plan_root())?;
        let second: Vec<String> = reloaded
            .0
            .rows
            .iter()
            .map(|row| {
                serde_json::to_string(&build_contract_plan_projection(row))
                    .map_err(|error| format!("serialize contract plan projection: {error}"))
            })
            .collect::<Result<_, _>>()?;
        if first != second {
            return Err(
                "a cold-start reload from the committed corpus alone must reconstruct byte-identical contract/plan decisions"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_fabricated_retrieval_bytes_fail_closed()
    -> Result<(), String> {
        let (corpus, _controls, _provenance) = load_committed()?;
        let mut row = corpus.rows[0].clone();
        row.retrieval_steps[0].bytes = IssueLifecycleIntakeBytesV1::Measured(1);
        let error = verify_contract_plan_row_snapshot(&row, &contract_plan_root())
            .err()
            .ok_or_else(|| "a fabricated retrieval byte count must fail closed".to_string())?;
        if !error.contains("claims 1 bytes") {
            return Err(format!("unexpected verification error: {error}"));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_root_disposition_downgrade_rejected()
    -> Result<(), String> {
        let (mut corpus, _controls, _provenance) = load_committed()?;
        let lifecycle_id = {
            let row = corpus
                .rows
                .iter_mut()
                .find(|row| row.category == "contract_required")
                .ok_or_else(|| "missing contract-required row".to_string())?;
            row.attempt.disposition = IssueLifecycleDispositionV1::RootDecisionRequired;
            row.attempt.row_digest =
                crate::issue_lifecycle_attempt::issue_lifecycle_row_digest(&row.attempt)?;
            row.attempt.lifecycle_id.clone()
        };
        let (failures, assessed) = assess_real_rows_against_counting_law(&corpus);
        let assessment = assessed
            .iter()
            .find(|assessment| assessment.lifecycle_id == lifecycle_id)
            .ok_or_else(|| "downgraded row was not assessed".to_string())?;
        if !assessment.counted {
            return Err(
                "the downgrade keeps the row counted, so only the root-disposition disagreement may reject it"
                    .to_string(),
            );
        }
        if !failures
            .iter()
            .any(|failure| failure.contains("disagrees with the assessed disposition"))
        {
            return Err(format!(
                "a counted but downgraded row must be rejected, got failures {failures:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_extra_provenance_row_rejected() -> Result<(), String> {
        let (corpus, _controls, mut provenance) = load_committed()?;
        check_provenance_against_corpus(&corpus, &provenance)?;
        provenance
            .rows
            .push(IssueLifecycleContractPlanProvenanceRowV1 {
                issue: 9_999_999,
                category: "narrow_accepted_contract_bug".to_string(),
                justification: "synthetic extra provenance entry".to_string(),
            });
        let error = check_provenance_against_corpus(&corpus, &provenance)
            .err()
            .ok_or_else(|| "an extra provenance row must fail closed".to_string())?;
        if !error.contains("must name 2 rows, got 3") {
            return Err(format!("unexpected provenance error: {error}"));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_empty_control_corpus_rejected() -> Result<(), String> {
        let body = format!(
            "{{\"schema_version\":\"{ISSUE_LIFECYCLE_CONTRACT_PLAN_CONTROL_CORPUS_SCHEMA_VERSION}\",\"rows\":[]}}"
        );
        let error = load_issue_lifecycle_contract_plan_control_corpus(&body)
            .err()
            .ok_or_else(|| "an empty control corpus must fail closed".to_string())?;
        if !error.contains("must not be empty") {
            return Err(format!("unexpected control corpus error: {error}"));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_missing_required_control_rejected() -> Result<(), String>
    {
        let path = contract_plan_root().join("controls.json");
        let body = fs::read_to_string(&path)
            .map_err(|error| format!("read committed controls: {error}"))?;
        let mut value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|error| format!("parse committed controls: {error}"))?;
        let rows = value
            .get_mut("rows")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| "committed controls carry no rows array".to_string())?;
        rows.retain(|row| {
            row.get("control").and_then(serde_json::Value::as_str)
                != Some("cold_start_root_reconstructs_decisions_from_artifacts")
        });
        let trimmed = serde_json::to_string(&value)
            .map_err(|error| format!("serialize trimmed controls: {error}"))?;
        let error = load_issue_lifecycle_contract_plan_control_corpus(&trimmed)
            .err()
            .ok_or_else(|| {
                "a control corpus missing a required control must fail closed".to_string()
            })?;
        if !error.contains("missing required control") {
            return Err(format!("unexpected control corpus error: {error}"));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_author_and_adversary_results_must_be_distinct()
    -> Result<(), String> {
        let (corpus, _controls, _provenance) = load_committed()?;
        let mut row = row_by_category(&corpus, "contract_required")?.clone();
        let contract = row
            .contract
            .as_mut()
            .ok_or_else(|| "the contract case must carry contract evidence".to_string())?;
        contract.adversary.result_identity = contract.author.result_identity.clone();
        let failures = assess_contract_plan_row(&row);
        if !failures
            .iter()
            .any(|failure| failure.contains("must be distinct identities"))
        {
            return Err(format!(
                "a shared author/adversary result identity must fail the law, got {failures:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_planning_drift_from_embedded_attempt_rejected()
    -> Result<(), String> {
        let (corpus, _controls, _provenance) = load_committed()?;
        let contract_row = row_by_category(&corpus, "contract_required")?;
        // Work-item identity drift.
        let mut drifted = contract_row.clone();
        drifted.planning.work_items[0].id = "wi-drifted".to_string();
        let failures = assess_contract_plan_row(&drifted);
        if !failures
            .iter()
            .any(|failure| failure.contains("planning work items drifted"))
        {
            return Err(format!(
                "a planning work-item drift must fail the law, got {failures:?}"
            ));
        }
        // Dependency-edge drift.
        let mut edge_drift = contract_row.clone();
        edge_drift.planning.work_items[1].depends_on.clear();
        let failures = assess_contract_plan_row(&edge_drift);
        if !failures
            .iter()
            .any(|failure| failure.contains("planning dependency edges drifted"))
        {
            return Err(format!(
                "a planning dependency-edge drift must fail the law, got {failures:?}"
            ));
        }
        // Acceptance-coverage drift.
        let mut coverage_drift = contract_row.clone();
        coverage_drift
            .planning
            .acceptance_covered
            .push("synthetic row absent from the attempt plan".to_string());
        let failures = assess_contract_plan_row(&coverage_drift);
        if !failures
            .iter()
            .any(|failure| failure.contains("planning acceptance coverage drifted"))
        {
            return Err(format!(
                "a planning acceptance-coverage drift must fail the law, got {failures:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn issue_lifecycle_contract_plan_pilot_provenance_captured_at_mismatch_rejected()
    -> Result<(), String> {
        let (corpus, _controls, mut provenance) = load_committed()?;
        provenance.captured_at = "2026-01-01T00:00:00Z".to_string();
        let error = check_provenance_against_corpus(&corpus, &provenance)
            .err()
            .ok_or_else(|| "a captured_at mismatch must fail closed".to_string())?;
        if !error.contains("captured at") {
            return Err(format!("unexpected provenance error: {error}"));
        }
        Ok(())
    }
}
