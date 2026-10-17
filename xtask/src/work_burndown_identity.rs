//! Burn-down consumer on the selected-work basis (#1706, delivery slice
//! PR B, RIPR-SPEC-0274, consuming #1648).
//!
//! `cargo xtask work burndown check [--corpus <dir>] [--json]` validates
//! committed `WorkBurnDownV1` rows against the same `PortfolioBasisV1` the
//! RIPR-SPEC-0244 selection check compiles from immutable captured inputs
//! (default `fixtures/work_burndown_identity`). Every burn-down binds the
//! repository, the portfolio-snapshot identity, the basis head and the
//! full selected-work identity it tracks; a burn-down for another
//! repository, portfolio snapshot, issue/action, basis or exact head fails
//! stale/wrong-input with the exact recompile/reconcile route recorded on
//! the violation. Human Markdown and JSON projections derive from one check
//! DTO and can never strengthen each other.
//!
//! Selection law: this module selects no work, claims nothing, authorizes
//! nothing and mutates nothing. Issue completion derives from issue/spec
//! acceptance, implementation slices, PRs, exact-head review/proof and
//! current main evidence; campaign/work-item records are plan context only
//! and can never override a current evidence row, so a `Complete`
//! burn-down with an open or stale evidence row fails visibly. It reuses
//! the RIPR-SPEC-0244 identity law engine instead of forking one: each
//! burn-down embeds the `SelectedWorkIdentityV1` it tracks, and every law
//! that engine already enforces for a packet is enforced for the embedded
//! selection before the burn-down-level laws run. Legacy
//! `legacy_active_goal_ref` / `legacy_current_work_item_ref` fields resolve
//! read-only to historical campaign references, emit migration posture, and
//! grant no write, readiness, merge or closeout authority.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::work_portfolio::{
    WorkCapturedDirV1, WorkPortfolioSnapshotV1, compile_work_portfolio, load_work_captured_dir,
    portable_identity, work_portfolio_json, workspace_path,
};
use crate::work_selection_identity::{
    CaseEvaluation, PortfolioBasisV1, RelevantCampaignRelationV1, SelectedWorkIdentityV1,
    WorkSelectionExpectV1, WorkSelectionExpectedViolationV1, WorkSelectionLawV1,
    WorkSelectionLawViolationV1, WorkSelectionRouteV1, build_portfolio_basis, check_packet,
    corpus_root, resolve_scenario_captured, selection_wire_name,
};

pub(crate) const WORK_BURNDOWN_IDENTITY_CORPUS_SCHEMA_VERSION: &str =
    "work_burndown_identity_corpus.v1";
pub(crate) const WORK_BURNDOWN_CHECK_VIEW_SCHEMA_VERSION: &str = "work_burndown_check_view.v1";
pub(crate) const WORK_BURNDOWN_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_burndown_identity_provenance.v1";

pub(crate) const DEFAULT_WORK_BURNDOWN_CORPUS_DIR: &str = "fixtures/work_burndown_identity";

pub(crate) const WORK_BURNDOWN_CHECK_CLAIM_BOUNDARY: &str = "Read-only burn-down check: the \
 check validates committed burn-down rows against one compiled portfolio basis and the \
 selected-work identity each burn-down tracks; it selects no work, claims nothing, authorizes \
 no writer, closes nothing, grants no readiness, merge or closeout authority, mutates no \
 GitHub state, branch, worktree, claim, spec, campaign or source, and it never fabricates \
 identity from missing evidence.";

// ---------------------------------------------------------------------------
// Burn-down DTOs (#1648): completion derives from issue/spec acceptance,
// implementation slices, PRs, exact-head review/proof and current main;
// campaign/work-item records are context only.
// ---------------------------------------------------------------------------

/// Closed evidence-row status vocabulary for the #1648 burn-down consumer.
/// A row is a recorded observation about one evidence source; it is never
/// an authority grant, and campaign/work-item records cannot override it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkEvidenceStatusV1 {
    AcceptanceCovered,
    AcceptanceOpen,
    ProofCurrent,
    ProofStale,
    Merged,
}

impl WorkEvidenceStatusV1 {
    /// A row is unresolved when it still has to burn down before the
    /// tracked issue can be complete.
    fn is_open(self) -> bool {
        matches!(
            self,
            WorkEvidenceStatusV1::AcceptanceOpen | WorkEvidenceStatusV1::ProofStale
        )
    }
}

/// Closed completion vocabulary. `Complete` is only honest when no
/// evidence row is open or stale.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkCompletionV1 {
    Complete,
    Partial,
    Open,
}

impl WorkCompletionV1 {
    /// Every completion in canonical declaration order, so projections
    /// keep a stable row per completion even when a corpus uses only some.
    pub(crate) fn all() -> [Self; 3] {
        [Self::Complete, Self::Partial, Self::Open]
    }
}

/// One burn-down evidence row: one issue/spec/PR/exact-head/current-main
/// observation and its recorded status.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownEvidenceRowV1 {
    pub source: String,
    pub status: WorkEvidenceStatusV1,
}

/// One burn-down row (#1648). It binds the repository, the compiled
/// portfolio-snapshot identity, the basis head and the full selected-work
/// identity it tracks. It must not contain a repository-wide
/// current/default campaign, a mutable agent assignment, CI wait state, or
/// a progress percentage; the embedded selection schema already rejects
/// those, `deny_unknown_fields` rejects additions here, and no field on a
/// burn-down grants write, readiness, merge or closeout authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownV1 {
    pub burndown_id: String,
    pub repository: String,
    /// The compiled portfolio snapshot identity the burn-down binds; a
    /// burn-down naming another snapshot is stale and routes recompilation.
    pub portfolio_snapshot: String,
    /// The basis (default-branch head) the burn-down was produced against.
    pub basis_sha: String,
    /// The full selected-work identity this burn-down tracks.
    pub selection: SelectedWorkIdentityV1,
    /// The current main head the burn-down derives against; it must match
    /// the captured default-branch head, so a burn-down anchored to an old
    /// main fails visibly.
    pub current_main_sha: String,
    /// Evidence rows over issue/spec acceptance, implementation slices,
    /// PRs and exact-head review/proof. A burn-down with no rows derives
    /// from nothing.
    pub issue_evidence: Vec<WorkBurnDownEvidenceRowV1>,
    /// The derived completion. `Complete` is only honest when no evidence
    /// row is open or stale; campaign/work-item records are context only
    /// and cannot override these rows.
    pub completion: WorkCompletionV1,
    /// Campaign/work-item record references consulted as plan context.
    /// They grant no authority and cannot override the evidence rows.
    pub campaign_records: Vec<String>,
    /// Retained artifact references the burn-down derives from. A
    /// burn-down derived from nothing is a claim, not a burn-down.
    pub derived_from: Vec<String>,
    /// Legacy compatibility references (read-only; see module docs).
    pub legacy_active_goal_ref: Option<String>,
    pub legacy_current_work_item_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// Corpus DTOs.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownCaseV1 {
    pub burndown: WorkBurnDownV1,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkBurnDownCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkBurnDownScenarioV1>,
}

pub(crate) fn load_work_burndown_corpus(
    body: &str,
) -> Result<WorkBurnDownIdentityCorpusV1, String> {
    let corpus: WorkBurnDownIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work burndown identity corpus: {error}"))?;
    if corpus.schema_version != WORK_BURNDOWN_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work burndown identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work burndown identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    if corpus.scenarios.is_empty() {
        return Err("work burndown identity corpus must carry at least one scenario".to_string());
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work burndown identity corpus has an empty or duplicate scenario id `{}`",
                scenario.id
            ));
        }
        if scenario.captured.trim().is_empty() {
            return Err(format!(
                "scenario `{}` must record its captured portfolio directory",
                scenario.id
            ));
        }
        if scenario.cases.is_empty() {
            return Err(format!(
                "scenario `{}` must carry at least one burn-down case",
                scenario.id
            ));
        }
        for (index, case) in scenario.cases.iter().enumerate() {
            if case.expect == WorkSelectionExpectV1::Fail && case.expect_violations.is_empty() {
                return Err(format!(
                    "scenario `{}` case {index} expects failure but pins no exact law/route pair",
                    scenario.id
                ));
            }
            if case.burndown.burndown_id.trim().is_empty()
                || case.burndown.basis_sha.trim().is_empty()
            {
                return Err(format!(
                    "scenario `{}` case {index} burn-down must record burndown_id and basis_sha",
                    scenario.id
                ));
            }
        }
    }
    Ok(corpus)
}

// ---------------------------------------------------------------------------
// Check DTOs (one DTO drives JSON and Markdown projections).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub burndown_id: String,
    pub completion: WorkCompletionV1,
    pub selection_key: Option<String>,
    pub open_evidence: Vec<String>,
    pub campaign_record_count: u64,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub burndown_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub cases: Vec<WorkBurnDownCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkBurnDownCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkBurnDownCheckCountsV1,
    pub results: Vec<WorkBurnDownScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Burn-down law engine. Pure and read-only; reuses the RIPR-SPEC-0244
// engine.
// ---------------------------------------------------------------------------

fn burndown_violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// Run every identity law for one burn-down row against the basis, the
/// compiled snapshot and the captured sources. The embedded selection must
/// satisfy every RIPR-SPEC-0244 law first; the burn-down-level laws then
/// bind the repository, the portfolio-snapshot identity, the basis head,
/// the current main head, the evidence rows, the derived completion, the
/// campaign-record context and the derived artifacts. Legacy references
/// resolve read-only and emit migration posture. Each wrong identity is
/// reported once with its exact recompile/reconcile route.
fn check_burndown(
    burndown: &WorkBurnDownV1,
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> CaseEvaluation {
    // #1648: the selected-work identity is required in every burn-down.
    // Reuse the shared law engine so a burn-down cannot smuggle in a
    // selection the selection check would reject.
    let mut evaluation = check_packet(&burndown.selection, basis, snapshot, captured);

    // Repository identity: the burn-down must name the basis repository.
    if burndown.repository != basis.repository.repository {
        evaluation.violations.push(burndown_violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "burn-down names repository `{}` but the portfolio basis is `{}`; a burn-down \
                 for another repository is wrong input — recompile the basis for the intended \
                 repository with `cargo xtask work portfolio`",
                burndown.repository, basis.repository.repository
            ),
        ));
    }

    // Portfolio-snapshot identity: the burn-down must bind this compiled
    // snapshot; a burn-down for another snapshot is stale.
    if burndown.portfolio_snapshot != basis.snapshot_identity {
        evaluation.violations.push(burndown_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "burn-down binds portfolio snapshot `{}` but the compiled basis is `{}`; a \
                 burn-down for another portfolio snapshot is stale — recompile the basis and \
                 rebind the burn-down",
                burndown.portfolio_snapshot, basis.snapshot_identity
            ),
        ));
    }

    // Basis identity: the recorded basis head must be the captured
    // default-branch head.
    if burndown.basis_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(burndown_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and rebind the burn-down",
                burndown.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Current-main identity: the burn-down must derive against the
    // captured default-branch head, so a burn-down anchored to an old main
    // fails visibly.
    if burndown.current_main_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(burndown_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "burn-down derives against current main `{}` but the captured default-branch \
                 head is `{}`; recompile the basis before deriving completion",
                burndown.current_main_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Evidence identity: a burn-down with no evidence rows derives from
    // nothing.
    if burndown.issue_evidence.is_empty() {
        evaluation.violations.push(burndown_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "burn-down records no issue/spec/PR/exact-head evidence rows; bind the observed \
             evidence completion derives from"
                .to_string(),
        ));
    }

    // Completion honesty: `Complete` is only honest when no evidence row
    // is open or stale. Campaign/work-item records are plan context only
    // and can never override a current evidence row.
    if burndown.completion == WorkCompletionV1::Complete {
        for row in &burndown.issue_evidence {
            if row.status.is_open() {
                evaluation.violations.push(burndown_violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "burn-down reports `complete` but evidence row `{}` is {}; campaign and \
                         work-item records are context only and cannot override current \
                         evidence — reconcile the completion",
                        row.source,
                        selection_wire_name(&row.status)
                    ),
                ));
            }
        }
    }

    // Derived-artifact identity: a burn-down derived from nothing is a
    // claim, not a burn-down.
    if burndown.derived_from.is_empty() {
        evaluation.violations.push(burndown_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "burn-down records no derived_from artifact reference; bind the retained reports, \
             receipts or run rows the burn-down derives from"
                .to_string(),
        ));
    }

    // Campaign-record identity: every consulted campaign/work-item record
    // must be a recorded campaign or one of the bound selection's campaign
    // references; unknown records fail closed rather than silently
    // fabricating plan context.
    for record in &burndown.campaign_records {
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *record)
            || burndown
                .selection
                .campaign_refs
                .iter()
                .any(|campaign_ref| campaign_ref.id == *record);
        if !known {
            evaluation.violations.push(burndown_violation(
                WorkSelectionLawV1::CampaignRefIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "campaign record `{record}` is not a recorded campaign and not a campaign \
                     reference of the bound selection; burn-down context must stay source-linked"
                ),
            ));
        }
    }

    // Legacy compatibility references at the burn-down level: resolve
    // read-only with migration posture, exactly as the selection law does;
    // grant no authority.
    if let Some(legacy_ref) = &burndown.legacy_active_goal_ref {
        evaluation.migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical campaign \
             reference; it grants no write, readiness, merge or closeout authority"
        ));
        let resolved = burndown.selection.campaign_refs.iter().any(|campaign_ref| {
            campaign_ref.id == *legacy_ref
                && campaign_ref.relation == RelevantCampaignRelationV1::Historical
        });
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !resolved || !known {
            evaluation.violations.push(burndown_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign \
                     carried as a historical reference by the bound selection"
                ),
            ));
        }
    }
    if let Some(legacy_ref) = &burndown.legacy_current_work_item_ref {
        evaluation.migration_posture.push(format!(
            "legacy_current_work_item_ref `{legacy_ref}` accepted as a read-only historical \
             work-item reference; it grants no write, readiness, merge or closeout authority"
        ));
        let matches_subject = burndown.selection.issue.as_ref().is_some_and(|issue| {
            *legacy_ref == crate::work_selection_identity::issue_identity(issue.number)
        });
        if !matches_subject {
            evaluation.violations.push(burndown_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_current_work_item_ref `{legacy_ref}` does not match the bound \
                     selection's issue identity"
                ),
            ));
        }
    }

    evaluation
}

// ---------------------------------------------------------------------------
// Corpus runner and projections.
// ---------------------------------------------------------------------------

fn run_burndown_scenario(
    scenario: &WorkBurnDownScenarioV1,
    corpus: &WorkBurnDownIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkBurnDownScenarioResultV1, String> {
    let captured_root = resolve_scenario_captured(corpus_root, &scenario.captured)
        .map_err(|error| format!("scenario `{}`: {error}", scenario.id))?;
    let captured = load_work_captured_dir(&captured_root)
        .map_err(|error| format!("scenario `{}`: {error}", scenario.id))?;
    let snapshot = compile_work_portfolio(&captured, None)
        .map_err(|error| format!("scenario `{}`: {error}", scenario.id))?;
    let basis = build_portfolio_basis(
        &snapshot,
        &corpus.selection_policy,
        &corpus.selection_policy_version,
    );
    let mut results = Vec::new();
    for (index, case) in scenario.cases.iter().enumerate() {
        let evaluation = check_burndown(&case.burndown, &basis, &snapshot, &captured);
        let passed = match case.expect {
            WorkSelectionExpectV1::Pass => evaluation.violations.is_empty(),
            WorkSelectionExpectV1::Fail => {
                // Exact match on the law/route pair multiset: a negative case
                // pins every occurrence, so an added spurious violation
                // cannot stay green.
                let mut actual_pairs: Vec<(WorkSelectionLawV1, WorkSelectionRouteV1)> = evaluation
                    .violations
                    .iter()
                    .map(|row| (row.law, row.route))
                    .collect();
                let mut expected_pairs: Vec<(WorkSelectionLawV1, WorkSelectionRouteV1)> = case
                    .expect_violations
                    .iter()
                    .map(|expected| (expected.law, expected.route))
                    .collect();
                actual_pairs.sort();
                expected_pairs.sort();
                !actual_pairs.is_empty() && actual_pairs == expected_pairs
            }
        };
        let burndown_identity = if evaluation.violations.is_empty() {
            Some(portable_identity("work burndown", &case.burndown)?)
        } else {
            None
        };
        let selection_key = case.burndown.selection.issue.as_ref().map(|issue| {
            crate::work_selection_identity::selection_identity(
                case.burndown.selection.lifecycle_action,
                issue.number,
            )
        });
        let open_evidence = case
            .burndown
            .issue_evidence
            .iter()
            .filter(|row| row.status.is_open())
            .map(|row| row.source.clone())
            .collect();
        results.push(WorkBurnDownCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            burndown_id: case.burndown.burndown_id.clone(),
            completion: case.burndown.completion,
            selection_key,
            open_evidence,
            campaign_record_count: case.burndown.campaign_records.len() as u64,
            violations: evaluation.violations,
            migration_posture: evaluation.migration_posture,
            burndown_identity,
        });
    }
    Ok(WorkBurnDownScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        cases: results,
    })
}

pub(crate) fn run_work_burndown_check(
    corpus_dir: &str,
    corpus: &WorkBurnDownIdentityCorpusV1,
) -> Result<WorkBurnDownCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_burndown_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkBurnDownCheckViewV1 {
        schema_version: WORK_BURNDOWN_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkBurnDownCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work burndown check".to_string(),
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_BURNDOWN_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work burndown check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_burndown_check_json(view: &WorkBurnDownCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

pub(crate) fn work_burndown_check_markdown(view: &WorkBurnDownCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work burn-down check\n\n");
    out.push_str(&format!("- corpus: `{}`\n", view.corpus));
    out.push_str(&format!("- repository: `{}`\n", view.repository));
    out.push_str(&format!(
        "- selection policy: `{}` (`{}`)\n",
        view.selection_policy, view.selection_policy_version
    ));
    out.push_str(&format!(
        "- scenarios: {}, cases: {}, passed: {}, failed: {}\n",
        view.counts.scenarios, view.counts.cases, view.counts.passed, view.counts.failed
    ));
    out.push_str(&format!(
        "- portable identity: `{}`\n\n",
        view.portable_identity
    ));
    let mut completion_counts: std::collections::BTreeMap<WorkCompletionV1, u64> =
        std::collections::BTreeMap::new();
    for case in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
    {
        *completion_counts.entry(case.completion).or_default() += 1;
    }
    out.push_str("Completions:\n\n");
    for completion in WorkCompletionV1::all() {
        let count = completion_counts.get(&completion).copied().unwrap_or(0);
        out.push_str(&format!(
            "- `{}`: {count}\n",
            selection_wire_name(&completion)
        ));
    }
    out.push('\n');
    out.push_str("| scenario | captured | basis | cases |\n");
    out.push_str("| --- | --- | --- | --- |\n");
    for scenario in &view.results {
        let cases_passed = scenario
            .cases
            .iter()
            .filter(|case| case.status == "passed")
            .count();
        out.push_str(&format!(
            "| {} | {} | {} | {}/{} |\n",
            scenario.scenario_id,
            scenario.captured,
            scenario.basis_sha,
            cases_passed,
            scenario.cases.len()
        ));
    }
    out.push('\n');
    for scenario in &view.results {
        for case in &scenario.cases {
            if !case.open_evidence.is_empty() {
                out.push_str(&format!(
                    "- {} case {} open evidence: {}\n",
                    scenario.scenario_id,
                    case.case_index,
                    case.open_evidence.join(", ")
                ));
            }
            if case.violations.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "## {} case {}\n\n",
                scenario.scenario_id, case.case_index
            ));
            for row in &case.violations {
                out.push_str(&format!(
                    "- `{}` → `{}`: {}\n",
                    selection_wire_name(&row.law),
                    selection_wire_name(&row.route),
                    row.detail
                ));
            }
            out.push('\n');
        }
    }
    for posture_line in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
        .flat_map(|case| case.migration_posture.iter())
    {
        out.push_str(&format!("- migration: {posture_line}\n"));
    }
    out.push('\n');
    out.push_str(&format!("Claim boundary: {}\n", view.claim_boundary));
    out
}

/// `cargo xtask work burndown check [--corpus <dir>] [--json]` (#1706 PR B,
/// RIPR-SPEC-0274, consuming #1648): validate the committed burn-down
/// corpus against the compiled portfolio bases and render JSON or the
/// derived Markdown. Read-only: no GitHub, branch, worktree, claim, spec,
/// campaign or source state is touched.
pub(crate) fn work_burndown_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work burndown check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_BURNDOWN_CORPUS_DIR.to_string();
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--corpus" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for --corpus\n{USAGE}"))?;
                corpus_dir = value.clone();
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    let root = corpus_root(&corpus_dir);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read {}: {error}", root.join("corpus.json").display()))?;
    let corpus = load_work_burndown_corpus(&body)?;
    let view = run_work_burndown_check(&corpus_dir, &corpus)?;
    let json_body = work_burndown_check_json(&view)?;
    crate::write_report("work-burndown-check.json", &json_body)?;
    crate::write_report(
        "work-burndown-check.md",
        &work_burndown_check_markdown(&view),
    )?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_burndown_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work burndown check: {} of {} cases failed; see work-burndown-check.md",
            view.counts.failed, view.counts.cases
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

/// Load the committed burn-down-corpus provenance fail-closed.
pub(crate) fn load_work_burndown_provenance(
    body: &str,
) -> Result<crate::work_selection_identity::WorkSelectionProvenanceV1, String> {
    let provenance: crate::work_selection_identity::WorkSelectionProvenanceV1 =
        serde_json::from_str(body)
            .map_err(|error| format!("parse work burndown identity provenance: {error}"))?;
    if provenance.schema_version != WORK_BURNDOWN_IDENTITY_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work burndown identity provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "work burndown identity provenance must record repository and capture_method"
                .to_string(),
        );
    }
    if provenance.captured_at.trim().is_empty() {
        return Err("work burndown identity provenance must record captured_at".to_string());
    }
    let mut paths = BTreeSet::new();
    for file in &provenance.files {
        reject_unsafe_burndown_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(format!(
                "work burndown identity provenance duplicates path `{}`",
                file.path
            ));
        }
    }
    Ok(provenance)
}

/// Fail closed on provenance paths that could escape the corpus root, with
/// the same cross-platform rules as the RIPR-SPEC-0244 provenance loader.
fn reject_unsafe_burndown_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work burndown identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work burndown identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The committed burn-down-corpus scenario ids, pinned in declaration order.
pub(crate) const WORK_BURNDOWN_SCENARIO_IDS: [&str; 5] = [
    "burndown-01-complete-evidence-burn-down-passes",
    "burndown-02-standalone-burn-down-passes",
    "burndown-03-partial-after-one-campaign-item-merges",
    "burndown-04-campaign-override-and-wrong-inputs-fail",
    "burndown-05-legacy-read-only-migration-posture",
];

/// Validate the committed burn-down corpus fail-closed: provenance digests
/// in both directions, the five required scenario ids, and every scenario's
/// pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_burndown_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_BURNDOWN_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_burndown_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_work_burndown_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/work_burndown_identity: {error}"));
            return;
        }
    };
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/work_burndown_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_burndown_identity: {} digest drifted: recorded `{}`, recomputed \
                 `work-result:sha256:{}`",
                path.display(),
                file.sha256,
                crate::blind_journey::sha256_hex(&bytes)
            ));
        }
    }
    // Reverse direction: every corpus byte must be listed in the provenance.
    let listed: BTreeSet<&str> = provenance
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    match crate::collect_files(&root) {
        Ok(on_disk) => {
            for path in on_disk {
                if path == provenance_path
                    || path.extension().and_then(|ext| ext.to_str()) != Some("json")
                {
                    continue;
                }
                let Ok(relative) = path.strip_prefix(&root) else {
                    continue;
                };
                let relative = relative.to_string_lossy().replace('\\', "/");
                if !listed.contains(relative.as_str()) {
                    violations.push(format!(
                        "fixtures/work_burndown_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_burndown_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_burndown_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_burndown_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_burndown_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_BURNDOWN_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_burndown_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_BURNDOWN_SCENARIO_IDS
        ));
    }
    // The committed corpus must never carry chat or singleton goal-file
    // authority bytes.
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_burndown_identity: corpus.json references `{forbidden}`; resume \
                 authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_burndown_check(DEFAULT_WORK_BURNDOWN_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_burndown_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_burndown_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
