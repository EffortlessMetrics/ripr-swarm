//! Codex adapter/delegation consumer on the selected-work basis (#1706,
//! delivery slice PR B, RIPR-SPEC-0277, consuming #1637 and #1649).
//!
//! `cargo xtask work adapter check [--corpus <dir>] [--json]` validates
//! committed `WorkAdapterV1` delegation rows against the same
//! `PortfolioBasisV1` the RIPR-SPEC-0244 selection check compiles from
//! immutable captured inputs (default `fixtures/work_adapter_identity`).
//! Every delegation binds the repository, the portfolio-snapshot identity,
//! the basis head and the full selected-work identity it delegates; a
//! delegation for another repository, portfolio snapshot, issue/action,
//! basis or exact head fails stale/wrong-input with the exact
//! recompile/reconcile route recorded on the violation. Human Markdown and
//! JSON projections derive from one check DTO and can never strengthen each
//! other.
//!
//! Selection law: this module selects no work, claims nothing, authorizes
//! no writer and mutates nothing. A subagent delegation carries exactly one
//! closed project role and one closed flow stage; the subagent consumes the
//! selected-work context packet plus relevant overflow on demand and never
//! re-reads the whole portfolio — portfolio compilation stays in the root
//! thread. Write-capable delegations (`builder`) require a prepared claim
//! or isolated worktree on the bound selection, and no subagent delegation
//! may claim write authority — claims stay in the root thread. It reuses
//! the RIPR-SPEC-0244 identity law engine instead of forking one: each
//! delegation embeds the `SelectedWorkIdentityV1` it delegates, and every
//! law that engine already enforces for a packet is enforced for the
//! embedded selection before the adapter-level laws run. Legacy
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

pub(crate) const WORK_ADAPTER_IDENTITY_CORPUS_SCHEMA_VERSION: &str =
    "work_adapter_identity_corpus.v1";
pub(crate) const WORK_ADAPTER_CHECK_VIEW_SCHEMA_VERSION: &str = "work_adapter_check_view.v1";
pub(crate) const WORK_ADAPTER_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_adapter_identity_provenance.v1";

pub(crate) const DEFAULT_WORK_ADAPTER_CORPUS_DIR: &str = "fixtures/work_adapter_identity";

pub(crate) const WORK_ADAPTER_CHECK_CLAIM_BOUNDARY: &str = "Read-only Codex adapter check: the \
 check validates committed delegation rows against one compiled portfolio basis and the \
 selected-work identity each delegation binds; it selects no work, claims nothing, authorizes \
 no writer, spawns no subagent, grants no readiness, merge or closeout authority, mutates no \
 GitHub state, branch, worktree, claim, spec, campaign or source, and it never fabricates \
 identity from missing evidence.";

// ---------------------------------------------------------------------------
// Adapter DTOs (#1637/#1649): one delegation row binds one closed role and
// one closed flow stage over one selected-work identity.
// ---------------------------------------------------------------------------

/// Closed project-role vocabulary for the #1637/#1649 Codex adapter
/// consumer: the six base execution roles plus the five issue-lifecycle
/// roles from #1649. Frozen wire names; adding a role is a schema-version
/// break, not a rename.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkAdapterRoleV1 {
    Scout,
    Adversary,
    Builder,
    Verifier,
    Reviewer,
    CleanupAuditor,
    IssueTriager,
    SpecAuthor,
    SpecAdversary,
    WorkPlanner,
    BurnDownReviewer,
}

impl WorkAdapterRoleV1 {
    /// Every role in canonical declaration order, so projections keep a
    /// stable row per role even when a corpus uses only some.
    pub(crate) fn all() -> [Self; 11] {
        [
            Self::Scout,
            Self::Adversary,
            Self::Builder,
            Self::Verifier,
            Self::Reviewer,
            Self::CleanupAuditor,
            Self::IssueTriager,
            Self::SpecAuthor,
            Self::SpecAdversary,
            Self::WorkPlanner,
            Self::BurnDownReviewer,
        ]
    }

    /// Roles whose delegations may write source under a prepared claim and
    /// an isolated worktree/edit cage. Every other role is read-only.
    pub(crate) fn write_capable(self) -> bool {
        matches!(self, Self::Builder)
    }
}

/// Closed adapter flow-stage vocabulary. The root thread runs
/// `reconstruct` through `reconcile`; a subagent delegation normally binds
/// `delegate` or one verification/inspection stage, and never `claim_write`
/// — claims stay in the root thread.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkAdapterFlowStageV1 {
    Reconstruct,
    Select,
    Compile,
    Delegate,
    ClaimWrite,
    VerifyReview,
    Reconcile,
}

impl WorkAdapterFlowStageV1 {
    /// Every stage in canonical declaration order, so projections keep a
    /// stable row per stage even when a corpus uses only some.
    pub(crate) fn all() -> [Self; 7] {
        [
            Self::Reconstruct,
            Self::Select,
            Self::Compile,
            Self::Delegate,
            Self::VerifyReview,
            Self::ClaimWrite,
            Self::Reconcile,
        ]
    }
}

/// One Codex adapter delegation row (#1637/#1649). It binds the repository,
/// the compiled portfolio-snapshot identity, the basis head and the full
/// selected-work identity it delegates. It must not contain a
/// repository-wide current/default campaign, a mutable agent assignment, CI
/// wait state, or a progress percentage; the embedded selection schema
/// already rejects those, `deny_unknown_fields` rejects additions here, and
/// no field on a delegation grants write, readiness, merge or closeout
/// authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkAdapterV1 {
    pub adapter_id: String,
    pub repository: String,
    /// The compiled portfolio snapshot identity the delegation binds; a
    /// delegation naming another snapshot is stale and routes
    /// recompilation.
    pub portfolio_snapshot: String,
    /// The basis (default-branch head) the delegation was produced
    /// against.
    pub basis_sha: String,
    /// The full selected-work identity this delegation hands to the
    /// subagent.
    pub selection: SelectedWorkIdentityV1,
    /// The closed project role being delegated.
    pub role: WorkAdapterRoleV1,
    /// The closed flow stage the delegation participates in.
    pub flow_stage: WorkAdapterFlowStageV1,
    /// The packet/prompt artifact references the subagent receives. The
    /// subagent consumes the selected-work context packet plus relevant
    /// overflow on demand; it never re-reads the whole portfolio, so this
    /// flag must stay `false` — portfolio compilation stays in the root
    /// thread.
    pub subagent_reads_portfolio: bool,
    /// The strict result schema the delegation must return (for example
    /// `SubtaskResult` or the typed issue/spec/plan/burn-down result).
    pub result_schema: String,
    /// Retained artifact references the delegation derives from. A
    /// delegation derived from nothing is a claim, not a delegation.
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
pub(crate) struct WorkAdapterCaseV1 {
    pub adapter: WorkAdapterV1,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkAdapterScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkAdapterCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkAdapterIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkAdapterScenarioV1>,
}

pub(crate) fn load_work_adapter_corpus(body: &str) -> Result<WorkAdapterIdentityCorpusV1, String> {
    let corpus: WorkAdapterIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work adapter identity corpus: {error}"))?;
    if corpus.schema_version != WORK_ADAPTER_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work adapter identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work adapter identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    if corpus.scenarios.is_empty() {
        return Err("work adapter identity corpus must carry at least one scenario".to_string());
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work adapter identity corpus has an empty or duplicate scenario id `{}`",
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
                "scenario `{}` must carry at least one adapter case",
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
            if case.adapter.adapter_id.trim().is_empty()
                || case.adapter.basis_sha.trim().is_empty()
                || case.adapter.result_schema.trim().is_empty()
            {
                return Err(format!(
                    "scenario `{}` case {index} adapter must record adapter_id, basis_sha and result_schema",
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
pub(crate) struct WorkAdapterCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub adapter_id: String,
    pub role: WorkAdapterRoleV1,
    pub flow_stage: WorkAdapterFlowStageV1,
    pub selection_key: Option<String>,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub adapter_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkAdapterScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub cases: Vec<WorkAdapterCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkAdapterCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkAdapterCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkAdapterCheckCountsV1,
    pub results: Vec<WorkAdapterScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Adapter law engine. Pure and read-only; reuses the RIPR-SPEC-0244
// engine.
// ---------------------------------------------------------------------------

fn adapter_violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// Run every identity law for one delegation row against the basis, the
/// compiled snapshot and the captured sources. The embedded selection must
/// satisfy every RIPR-SPEC-0244 law first; the adapter-level laws then
/// bind the repository, the portfolio-snapshot identity, the basis head,
/// the subagent portfolio boundary, the write-capable roles, the claim
/// boundary and the derived artifacts. Legacy references resolve read-only
/// and emit migration posture. Each wrong identity is reported once with
/// its exact recompile/reconcile route.
fn check_adapter(
    adapter: &WorkAdapterV1,
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> CaseEvaluation {
    // #1637/#1649: the selected-work identity is required in every
    // delegation. Reuse the shared law engine so a delegation cannot
    // smuggle in a selection the selection check would reject.
    let mut evaluation = check_packet(&adapter.selection, basis, snapshot, captured);

    // Repository identity: the delegation must name the basis repository.
    if adapter.repository != basis.repository.repository {
        evaluation.violations.push(adapter_violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "delegation names repository `{}` but the portfolio basis is `{}`; a delegation \
                 for another repository is wrong input — recompile the basis for the intended \
                 repository with `cargo xtask work portfolio`",
                adapter.repository, basis.repository.repository
            ),
        ));
    }

    // Portfolio-snapshot identity: the delegation must bind this compiled
    // snapshot; a delegation for another snapshot is stale.
    if adapter.portfolio_snapshot != basis.snapshot_identity {
        evaluation.violations.push(adapter_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "delegation binds portfolio snapshot `{}` but the compiled basis is `{}`; a \
                 delegation for another portfolio snapshot is stale — recompile the basis and \
                 rebind the delegation",
                adapter.portfolio_snapshot, basis.snapshot_identity
            ),
        ));
    }

    // Basis identity: the recorded basis head must be the captured
    // default-branch head.
    if adapter.basis_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(adapter_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and rebind the \
                 delegation",
                adapter.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Subagent portfolio boundary (#1637): a subagent consumes the
    // selected-work context packet plus relevant overflow on demand and
    // never re-reads the whole portfolio; portfolio compilation stays in
    // the root thread.
    if adapter.subagent_reads_portfolio {
        evaluation.violations.push(adapter_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "delegation asserts the subagent re-reads the portfolio; subagents consume the \
             selected-work context packet plus relevant overflow on demand — portfolio \
             compilation stays in the root thread"
                .to_string(),
        ));
    }

    // Write-capable roles (#1637 builder contract): a builder delegation is
    // permitted only with a prepared claim or an isolated worktree on the
    // bound selection.
    if adapter.role.write_capable()
        && adapter.selection.overlaps.claims.is_empty()
        && adapter.selection.overlaps.worktrees.is_empty()
    {
        evaluation.violations.push(adapter_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            format!(
                "role `{}` is write-capable and requires a prepared claim or isolated worktree \
                 on the bound selection; acquire the durable claim before delegating",
                selection_wire_name(&adapter.role)
            ),
        ));
    }

    // Claim boundary (#1635): no subagent delegation may claim write
    // authority; claims stay in the root thread.
    if adapter.flow_stage == WorkAdapterFlowStageV1::ClaimWrite {
        evaluation.violations.push(adapter_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            format!(
                "delegation binds flow stage `claim_write`; subagents never claim — acquire, \
                 bind and reconcile claims in the root thread (role `{}`)",
                selection_wire_name(&adapter.role)
            ),
        ));
    }

    // Derived-artifact identity (#1633): a delegation derived from nothing
    // is a claim, not a delegation.
    if adapter.derived_from.is_empty() {
        evaluation.violations.push(adapter_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "delegation records no derived_from artifact reference; bind the retained packets, \
             prompts, receipts or run rows the delegation derives from"
                .to_string(),
        ));
    }

    // Legacy compatibility references at the delegation level: resolve
    // read-only with migration posture, exactly as the selection law does;
    // grant no authority.
    if let Some(legacy_ref) = &adapter.legacy_active_goal_ref {
        evaluation.migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical campaign \
             reference; it grants no write, readiness, merge or closeout authority"
        ));
        let resolved = adapter.selection.campaign_refs.iter().any(|campaign_ref| {
            campaign_ref.id == *legacy_ref
                && campaign_ref.relation == RelevantCampaignRelationV1::Historical
        });
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !resolved || !known {
            evaluation.violations.push(adapter_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign \
                     carried as a historical reference by the bound selection"
                ),
            ));
        }
    }
    if let Some(legacy_ref) = &adapter.legacy_current_work_item_ref {
        evaluation.migration_posture.push(format!(
            "legacy_current_work_item_ref `{legacy_ref}` accepted as a read-only historical \
             work-item reference; it grants no write, readiness, merge or closeout authority"
        ));
        let matches_subject = adapter.selection.issue.as_ref().is_some_and(|issue| {
            *legacy_ref == crate::work_selection_identity::issue_identity(issue.number)
        });
        if !matches_subject {
            evaluation.violations.push(adapter_violation(
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

fn run_adapter_scenario(
    scenario: &WorkAdapterScenarioV1,
    corpus: &WorkAdapterIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkAdapterScenarioResultV1, String> {
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
        let evaluation = check_adapter(&case.adapter, &basis, &snapshot, &captured);
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
        let adapter_identity = if evaluation.violations.is_empty() {
            Some(portable_identity("work adapter", &case.adapter)?)
        } else {
            None
        };
        let selection_key = case.adapter.selection.issue.as_ref().map(|issue| {
            crate::work_selection_identity::selection_identity(
                case.adapter.selection.lifecycle_action,
                issue.number,
            )
        });
        results.push(WorkAdapterCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            adapter_id: case.adapter.adapter_id.clone(),
            role: case.adapter.role,
            flow_stage: case.adapter.flow_stage,
            selection_key,
            violations: evaluation.violations,
            migration_posture: evaluation.migration_posture,
            adapter_identity,
        });
    }
    Ok(WorkAdapterScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        cases: results,
    })
}

pub(crate) fn run_work_adapter_check(
    corpus_dir: &str,
    corpus: &WorkAdapterIdentityCorpusV1,
) -> Result<WorkAdapterCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_adapter_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkAdapterCheckViewV1 {
        schema_version: WORK_ADAPTER_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkAdapterCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work adapter check".to_string(),
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_ADAPTER_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work adapter check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_adapter_check_json(view: &WorkAdapterCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

pub(crate) fn work_adapter_check_markdown(view: &WorkAdapterCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work Codex adapter check\n\n");
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
    let mut role_counts: std::collections::BTreeMap<WorkAdapterRoleV1, u64> =
        std::collections::BTreeMap::new();
    for case in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
    {
        *role_counts.entry(case.role).or_default() += 1;
    }
    out.push_str("Roles:\n\n");
    for role in WorkAdapterRoleV1::all() {
        let count = role_counts.get(&role).copied().unwrap_or(0);
        out.push_str(&format!("- `{}`: {count}\n", selection_wire_name(&role)));
    }
    out.push('\n');
    let mut stage_counts: std::collections::BTreeMap<WorkAdapterFlowStageV1, u64> =
        std::collections::BTreeMap::new();
    for case in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
    {
        *stage_counts.entry(case.flow_stage).or_default() += 1;
    }
    out.push_str("Flow stages:\n\n");
    for stage in WorkAdapterFlowStageV1::all() {
        let count = stage_counts.get(&stage).copied().unwrap_or(0);
        out.push_str(&format!("- `{}`: {count}\n", selection_wire_name(&stage)));
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

/// `cargo xtask work adapter check [--corpus <dir>] [--json]` (#1706 PR B,
/// RIPR-SPEC-0277, consuming #1637/#1649): validate the committed adapter
/// corpus against the compiled portfolio bases and render JSON or the
/// derived Markdown. Read-only: no GitHub, branch, worktree, claim, spec,
/// campaign or source state is touched.
pub(crate) fn work_adapter_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work adapter check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_ADAPTER_CORPUS_DIR.to_string();
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
    let corpus = load_work_adapter_corpus(&body)?;
    let view = run_work_adapter_check(&corpus_dir, &corpus)?;
    let json_body = work_adapter_check_json(&view)?;
    crate::write_report("work-adapter-check.json", &json_body)?;
    crate::write_report("work-adapter-check.md", &work_adapter_check_markdown(&view))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_adapter_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work adapter check: {} of {} cases failed; see work-adapter-check.md",
            view.counts.failed, view.counts.cases
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

/// Load the committed adapter-corpus provenance fail-closed.
pub(crate) fn load_work_adapter_provenance(
    body: &str,
) -> Result<crate::work_selection_identity::WorkSelectionProvenanceV1, String> {
    let provenance: crate::work_selection_identity::WorkSelectionProvenanceV1 =
        serde_json::from_str(body)
            .map_err(|error| format!("parse work adapter identity provenance: {error}"))?;
    if provenance.schema_version != WORK_ADAPTER_IDENTITY_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work adapter identity provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "work adapter identity provenance must record repository and capture_method"
                .to_string(),
        );
    }
    if provenance.captured_at.trim().is_empty() {
        return Err("work adapter identity provenance must record captured_at".to_string());
    }
    let mut paths = BTreeSet::new();
    for file in &provenance.files {
        reject_unsafe_adapter_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(format!(
                "work adapter identity provenance duplicates path `{}`",
                file.path
            ));
        }
    }
    Ok(provenance)
}

/// Fail closed on provenance paths that could escape the corpus root, with
/// the same cross-platform rules as the RIPR-SPEC-0244 provenance loader.
fn reject_unsafe_adapter_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work adapter identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work adapter identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The committed adapter-corpus scenario ids, pinned in declaration order.
pub(crate) const WORK_ADAPTER_SCENARIO_IDS: [&str; 5] = [
    "adapter-01-scout-read-only-delegation-passes",
    "adapter-02-builder-with-prepared-claim-passes",
    "adapter-03-planner-compile-delegation-passes",
    "adapter-04-resume-from-artifacts-review-passes",
    "adapter-05-portfolio-reread-and-wrong-inputs-fail",
];

/// Validate the committed adapter corpus fail-closed: provenance digests in
/// both directions, the five required scenario ids, and every scenario's
/// pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_adapter_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_ADAPTER_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_adapter_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_work_adapter_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/work_adapter_identity: {error}"));
            return;
        }
    };
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/work_adapter_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_adapter_identity: {} digest drifted: recorded `{}`, recomputed \
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
                        "fixtures/work_adapter_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_adapter_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_adapter_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_adapter_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_adapter_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_ADAPTER_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_adapter_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_ADAPTER_SCENARIO_IDS
        ));
    }
    // The committed corpus must never carry chat or singleton goal-file
    // authority bytes; resume authority comes from committed artifacts only.
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_adapter_identity: corpus.json references `{forbidden}`; resume \
                 authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_adapter_check(DEFAULT_WORK_ADAPTER_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_adapter_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_adapter_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
