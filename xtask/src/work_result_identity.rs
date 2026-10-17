//! Task/result/receipt schema consumer on the selected-work basis (#1706,
//! delivery slice PR B, RIPR-SPEC-0269, consuming #1633).
//!
//! `cargo xtask work result check [--corpus <dir>] [--json]` validates
//! committed `WorkTaskResultV1` rows against the same `PortfolioBasisV1` the
//! RIPR-SPEC-0244 selection check compiles from immutable captured inputs
//! (default `fixtures/work_result_identity`). Every result must bind the
//! repository, the portfolio-snapshot identity, the basis head and the full
//! selected-work identity it extends; a result for another repository,
//! portfolio snapshot, issue/action, basis, worktree or exact head fails
//! stale/wrong-input with the exact recompile/reconcile route recorded on
//! the violation. Human Markdown and JSON projections derive from one check
//! DTO and can never strengthen each other.
//!
//! Selection law: this module selects no work, claims nothing, authorizes
//! nothing and mutates nothing. It reuses the RIPR-SPEC-0244 identity law
//! engine instead of forking one: each result embeds the
//! `SelectedWorkIdentityV1` it extends, and every law that engine already
//! enforces for a packet is enforced for the embedded selection before the
//! result-level laws run. Campaign references stay context, never
//! permission. Legacy `legacy_active_goal_ref` /
//! `legacy_current_work_item_ref` fields on a result are accepted only as
//! explicit schema compatibility: they resolve read-only to historical
//! campaign references, emit migration posture, and grant no write,
//! readiness, merge or closeout authority.

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

pub(crate) const WORK_RESULT_IDENTITY_CORPUS_SCHEMA_VERSION: &str =
    "work_result_identity_corpus.v1";
pub(crate) const WORK_RESULT_CHECK_VIEW_SCHEMA_VERSION: &str = "work_result_check_view.v1";
pub(crate) const WORK_RESULT_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_result_identity_provenance.v1";

pub(crate) const DEFAULT_WORK_RESULT_CORPUS_DIR: &str = "fixtures/work_result_identity";

pub(crate) const WORK_RESULT_CHECK_CLAIM_BOUNDARY: &str = "Read-only task-result identity \
 receipt: the check validates committed result rows against one compiled portfolio basis and the \
 selected-work identity each result binds; it selects no work, claims nothing, authorizes no \
 writer, grants no readiness, merge or closeout authority, mutates no GitHub state, branch, \
 worktree, claim, spec, campaign or source, and it never fabricates identity from missing \
 evidence.";

// ---------------------------------------------------------------------------
// Result DTOs (#1633): every packet/result binds repository, portfolio basis
// and selected-work identity.
// ---------------------------------------------------------------------------

/// Closed result-kind vocabulary for the #1633 task/result/receipt consumer.
/// Frozen wire names: a result kind is a recorded observation, never an
/// authority grant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkResultKindV1 {
    ContextCompiled,
    TaskDispatched,
    BuildCompleted,
    VerificationRecorded,
    ReviewRecorded,
}

impl WorkResultKindV1 {
    /// Every result kind in canonical declaration order, so projections keep
    /// a stable row per kind even when a corpus uses only some.
    pub(crate) fn all() -> [Self; 5] {
        [
            Self::ContextCompiled,
            Self::TaskDispatched,
            Self::BuildCompleted,
            Self::VerificationRecorded,
            Self::ReviewRecorded,
        ]
    }
}

/// One task result/receipt row (#1633). It binds the repository, the compiled
/// portfolio-snapshot identity, the basis head, and the full selected-work
/// identity it extends. It must not contain a repository-wide
/// current/default campaign, a mutable agent assignment, CI wait state, or a
/// progress percentage; the embedded selection schema already rejects those,
/// `deny_unknown_fields` rejects additions here, and no field on a result
/// grants write, readiness, merge or closeout authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkTaskResultV1 {
    pub result_id: String,
    pub repository: String,
    /// The compiled portfolio snapshot identity the result binds; a result
    /// naming another snapshot is stale and routes recompilation.
    pub portfolio_snapshot: String,
    /// The basis (default-branch head) the result was produced against.
    pub basis_sha: String,
    /// The full selected-work identity this result extends.
    pub selection: SelectedWorkIdentityV1,
    pub result_kind: WorkResultKindV1,
    /// The writer worktree the result was produced on, when any; it must be
    /// one of the worktrees recorded in the selection overlaps.
    pub worktree: Option<String>,
    /// The exact head observed when the result was produced; it must match
    /// the selection's expected head.
    pub reported_head: String,
    /// Retained evidence references (reports, receipts, run rows). A result
    /// with no evidence reference is a claim, not a result.
    pub evidence_refs: Vec<String>,
    /// Legacy compatibility references (read-only; see module docs).
    pub legacy_active_goal_ref: Option<String>,
    pub legacy_current_work_item_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// Corpus DTOs.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkResultCaseV1 {
    pub result: WorkTaskResultV1,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkResultScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkResultCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkResultIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkResultScenarioV1>,
}

pub(crate) fn load_work_result_corpus(body: &str) -> Result<WorkResultIdentityCorpusV1, String> {
    let corpus: WorkResultIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work result identity corpus: {error}"))?;
    if corpus.schema_version != WORK_RESULT_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work result identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work result identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    if corpus.scenarios.is_empty() {
        return Err("work result identity corpus must carry at least one scenario".to_string());
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work result identity corpus has an empty or duplicate scenario id `{}`",
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
                "scenario `{}` must carry at least one result case",
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
            if case.result.result_id.trim().is_empty() || case.result.basis_sha.trim().is_empty() {
                return Err(format!(
                    "scenario `{}` case {index} result must record result_id and basis_sha",
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
pub(crate) struct WorkResultCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub result_kind: WorkResultKindV1,
    pub result_id: String,
    pub selection_key: Option<String>,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub result_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkResultScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub cases: Vec<WorkResultCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkResultCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkResultCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkResultCheckCountsV1,
    pub results: Vec<WorkResultScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Result law engine. Pure and read-only; reuses the RIPR-SPEC-0244 engine.
// ---------------------------------------------------------------------------

fn result_violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// Run every identity law for one result row against the basis, the compiled
/// snapshot and the captured sources. The embedded selection must satisfy
/// every RIPR-SPEC-0244 law first; the result-level laws then bind the
/// repository, the portfolio-snapshot identity, the basis head, the reported
/// head, the writer worktree and the recorded evidence. Legacy references
/// resolve read-only and emit migration posture. Each wrong identity is
/// reported once with its exact recompile/reconcile route.
fn check_result(
    result: &WorkTaskResultV1,
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> CaseEvaluation {
    // #1633: the selected-work identity is required in every result. Reuse
    // the shared law engine so a result cannot smuggle in a packet the
    // selection check would reject.
    let mut evaluation = check_packet(&result.selection, basis, snapshot, captured);

    // Repository identity: the result must name the basis repository.
    if result.repository != basis.repository.repository {
        evaluation.violations.push(result_violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "result names repository `{}` but the portfolio basis is `{}`; a result for \
                 another repository is wrong input — recompile the basis for the intended \
                 repository with `cargo xtask work portfolio`",
                result.repository, basis.repository.repository
            ),
        ));
    }

    // Portfolio-snapshot identity: the result must bind this compiled
    // snapshot; a result for another snapshot is stale.
    if result.portfolio_snapshot != basis.snapshot_identity {
        evaluation.violations.push(result_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "result binds portfolio snapshot `{}` but the compiled basis is `{}`; a result \
                 for another portfolio snapshot is stale — recompile the basis and rebind the \
                 result",
                result.portfolio_snapshot, basis.snapshot_identity
            ),
        ));
    }

    // Basis identity: the recorded basis head must be the captured
    // default-branch head.
    if result.basis_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(result_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and rebind the result",
                result.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Head identity: the reported head must match the selection's expected
    // head (the basis head when the selection binds none).
    let expected_head = result.selection.expected_head.as_ref().map_or_else(
        || basis.repository.default_branch_sha.as_str(),
        String::as_str,
    );
    if result.reported_head != *expected_head {
        evaluation.violations.push(result_violation(
            WorkSelectionLawV1::HeadIdentity,
            WorkSelectionRouteV1::ReconcileHead,
            format!(
                "reported head `{}` does not match the selection's expected head `{expected_head}`; \
                 reconcile the exact head before this result may be consumed",
                result.reported_head
            ),
        ));
    }

    // Worktree identity: a worktree result must be recorded in the selection
    // overlaps, so existing PR/claim/worktree overlap stays visible before
    // any mutation.
    if let Some(worktree) = &result.worktree
        && !result.selection.overlaps.worktrees.contains(worktree)
    {
        evaluation.violations.push(result_violation(
            WorkSelectionLawV1::WorktreeIdentity,
            WorkSelectionRouteV1::ReconcileResources,
            format!(
                "result claims worktree `{worktree}` that the selection does not record in its \
                 overlaps; reconcile the writer resources before consuming this result"
            ),
        ));
    }

    // Evidence identity: a result with no evidence reference is a claim, not
    // a result.
    if result.evidence_refs.is_empty() {
        evaluation.violations.push(result_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "result records no evidence reference; bind the retained reports, receipts or run \
             rows the result derives from"
                .to_string(),
        ));
    }

    // Legacy compatibility references at the result level: resolve read-only
    // with migration posture, exactly as the packet law does; grant no
    // authority.
    if let Some(legacy_ref) = &result.legacy_active_goal_ref {
        evaluation.migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical campaign \
             reference; it grants no write, readiness, merge or closeout authority"
        ));
        let resolved = result.selection.campaign_refs.iter().any(|campaign_ref| {
            campaign_ref.id == *legacy_ref
                && campaign_ref.relation == RelevantCampaignRelationV1::Historical
        });
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !resolved || !known {
            evaluation.violations.push(result_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign \
                     carried as a historical reference by the bound selection"
                ),
            ));
        }
    }
    if let Some(legacy_ref) = &result.legacy_current_work_item_ref {
        evaluation.migration_posture.push(format!(
            "legacy_current_work_item_ref `{legacy_ref}` accepted as a read-only historical \
             work-item reference; it grants no write, readiness, merge or closeout authority"
        ));
        let matches_subject = result.selection.issue.as_ref().is_some_and(|issue| {
            *legacy_ref == crate::work_selection_identity::issue_identity(issue.number)
        });
        if !matches_subject {
            evaluation.violations.push(result_violation(
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

fn run_result_scenario(
    scenario: &WorkResultScenarioV1,
    corpus: &WorkResultIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkResultScenarioResultV1, String> {
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
        let evaluation = check_result(&case.result, &basis, &snapshot, &captured);
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
        let result_identity = if evaluation.violations.is_empty() {
            Some(portable_identity("work task result", &case.result)?)
        } else {
            None
        };
        let selection_key = case.result.selection.issue.as_ref().map(|issue| {
            crate::work_selection_identity::selection_identity(
                case.result.selection.lifecycle_action,
                issue.number,
            )
        });
        results.push(WorkResultCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            result_kind: case.result.result_kind,
            result_id: case.result.result_id.clone(),
            selection_key,
            violations: evaluation.violations,
            migration_posture: evaluation.migration_posture,
            result_identity,
        });
    }
    Ok(WorkResultScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        cases: results,
    })
}

pub(crate) fn run_work_result_check(
    corpus_dir: &str,
    corpus: &WorkResultIdentityCorpusV1,
) -> Result<WorkResultCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_result_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkResultCheckViewV1 {
        schema_version: WORK_RESULT_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkResultCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work result check".to_string(),
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_RESULT_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work result check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_result_check_json(view: &WorkResultCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

pub(crate) fn work_result_check_markdown(view: &WorkResultCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work task-result check\n\n");
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
    let mut kind_counts: std::collections::BTreeMap<WorkResultKindV1, u64> =
        std::collections::BTreeMap::new();
    for case in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
    {
        *kind_counts.entry(case.result_kind).or_default() += 1;
    }
    out.push_str("Result kinds:\n\n");
    for kind in WorkResultKindV1::all() {
        let count = kind_counts.get(&kind).copied().unwrap_or(0);
        out.push_str(&format!("- `{}`: {count}\n", selection_wire_name(&kind)));
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

/// `cargo xtask work result check [--corpus <dir>] [--json]` (#1706 PR B,
/// RIPR-SPEC-0269, consuming #1633): validate the committed task-result
/// corpus against the compiled portfolio bases and render JSON or the
/// derived Markdown. Read-only: no GitHub, branch, worktree, claim, spec,
/// campaign or source state is touched.
pub(crate) fn work_result_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work result check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_RESULT_CORPUS_DIR.to_string();
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
    let corpus = load_work_result_corpus(&body)?;
    let view = run_work_result_check(&corpus_dir, &corpus)?;
    let json_body = work_result_check_json(&view)?;
    crate::write_report("work-result-check.json", &json_body)?;
    crate::write_report("work-result-check.md", &work_result_check_markdown(&view))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_result_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work result check: {} of {} cases failed; see work-result-check.md",
            view.counts.failed, view.counts.cases
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

/// Validate the committed task-result corpus fail-closed: provenance digests
/// in both directions, the five required scenario ids, and every scenario's
/// pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn load_work_result_provenance(
    body: &str,
) -> Result<crate::work_selection_identity::WorkSelectionProvenanceV1, String> {
    let provenance: crate::work_selection_identity::WorkSelectionProvenanceV1 =
        serde_json::from_str(body)
            .map_err(|error| format!("parse work result identity provenance: {error}"))?;
    if provenance.schema_version != WORK_RESULT_IDENTITY_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work result identity provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "work result identity provenance must record repository and capture_method".to_string(),
        );
    }
    if provenance.captured_at.trim().is_empty() {
        return Err("work result identity provenance must record captured_at".to_string());
    }
    let mut paths = BTreeSet::new();
    for file in &provenance.files {
        reject_unsafe_result_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(format!(
                "work result identity provenance duplicates path `{}`",
                file.path
            ));
        }
    }
    Ok(provenance)
}

/// Fail closed on provenance paths that could escape the corpus root, with
/// the same cross-platform rules as the RIPR-SPEC-0244 provenance loader.
fn reject_unsafe_result_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work result identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work result identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The committed result-corpus scenario ids, pinned in declaration order.
pub(crate) const WORK_RESULT_SCENARIO_IDS: [&str; 5] = [
    "result-01-one-campaign-selection-results-pass",
    "result-02-standalone-selection-results-pass",
    "result-03-repair-selection-worktree-result-passes",
    "result-04-legacy-read-only-migration-posture",
    "result-05-wrong-identities-fail-stale-or-wrong-input",
];

/// Validate the committed task-result corpus fail-closed: provenance digests
/// in both directions, the five required scenario ids, and every scenario's
/// pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_result_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_RESULT_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_result_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_work_result_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/work_result_identity: {error}"));
            return;
        }
    };
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/work_result_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_result_identity: {} digest drifted: recorded `{}`, recomputed \
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
                        "fixtures/work_result_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_result_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_result_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_result_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_result_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_RESULT_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_result_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_RESULT_SCENARIO_IDS
        ));
    }
    // The committed corpus must never carry chat or singleton goal-file
    // authority bytes.
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_result_identity: corpus.json references `{forbidden}`; resume \
                 authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_result_check(DEFAULT_WORK_RESULT_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_result_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_result_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
