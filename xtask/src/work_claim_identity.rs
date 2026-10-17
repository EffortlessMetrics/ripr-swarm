//! Durable claim binding on the selected-work portfolio basis (#1706,
//! delivery slice PR B, RIPR-SPEC-0270, consuming #1635/#1647).
//!
//! `cargo xtask work claim check [--corpus <dir>] [--json]` validates
//! committed `WorkClaimV1` rows against the same `PortfolioBasisV1` the
//! RIPR-SPEC-0244 selection check compiles from immutable captured inputs
//! (default `fixtures/work_claim_identity`). Every claim binds one selected
//! issue/work item/action and the exact portfolio basis; claiming one item
//! never asserts repository-wide priority, and a stale portfolio basis
//! requires reconciliation — never automatic release or takeover. Human
//! Markdown and JSON projections derive from one check DTO and can never
//! strengthen each other.
//!
//! Selection law: this module selects no work, authorizes no writer beyond
//! the recorded claim, and mutates nothing. It reuses the RIPR-SPEC-0244
//! identity law engine instead of forking one: each claim embeds the
//! `SelectedWorkIdentityV1` it binds, and every law that engine already
//! enforces for a packet is enforced for the embedded selection before the
//! claim-level laws run. Legacy `legacy_active_goal_ref` /
//! `legacy_current_work_item_ref` fields on a claim are accepted only as
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
    corpus_root, issue_identity, resolve_scenario_captured, selection_identity,
    selection_wire_name,
};

pub(crate) const WORK_CLAIM_IDENTITY_CORPUS_SCHEMA_VERSION: &str = "work_claim_identity_corpus.v1";
pub(crate) const WORK_CLAIM_CHECK_VIEW_SCHEMA_VERSION: &str = "work_claim_check_view.v1";
pub(crate) const WORK_CLAIM_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_claim_identity_provenance.v1";

pub(crate) const DEFAULT_WORK_CLAIM_CORPUS_DIR: &str = "fixtures/work_claim_identity";

pub(crate) const WORK_CLAIM_CHECK_CLAIM_BOUNDARY: &str = "Read-only durable-claim identity \
 receipt: the check validates committed claim rows against one compiled portfolio basis and the \
 selected-work identity each claim binds; it selects no work, grants no readiness, merge or \
 closeout authority, performs no release or takeover on stale or conflicting claims, mutates no \
 GitHub state, branch, worktree, claim, spec, campaign or source, and it never fabricates \
 identity from missing evidence.";

// ---------------------------------------------------------------------------
// Claim DTOs (#1635 local claims / #1647 durable claims): one claim binds
// one selected item and the exact portfolio basis, never repository-wide
// priority.
// ---------------------------------------------------------------------------

/// Closed claim-kind vocabulary. Only a writer claim exists on this basis;
/// widening it is a schema-version break, not a rename.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkClaimKindV1 {
    Writer,
}

/// One durable claim row. It binds exactly one selected issue/work
/// item/action and the exact portfolio basis. It must not contain a
/// repository-wide current/default campaign, a priority, a mutable agent
/// assignment, CI wait state, or a progress percentage; the embedded
/// selection schema already rejects those, `deny_unknown_fields` rejects
/// additions here, and no field on a claim grants readiness, merge or
/// closeout authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkClaimV1 {
    pub claim_id: String,
    pub repository: String,
    /// The compiled portfolio snapshot identity the claim binds; a claim
    /// naming another snapshot is stale and routes recompilation.
    pub portfolio_snapshot: String,
    /// The basis (default-branch head) the claim was recorded against.
    pub basis_sha: String,
    /// The full selected-work identity this claim binds.
    pub selection: SelectedWorkIdentityV1,
    /// The writer worktree the claim occupies; it must be one of the
    /// worktrees recorded in the selection overlaps.
    pub worktree: String,
    pub claim_kind: WorkClaimKindV1,
    /// Legacy compatibility references (read-only; see module docs).
    pub legacy_active_goal_ref: Option<String>,
    pub legacy_current_work_item_ref: Option<String>,
}

/// The selection key a claim binds: the canonical selection identity for
/// the embedded action and issue. Claims on the same key compete; claims on
/// different keys never interact.
fn claim_selection_key(claim: &WorkClaimV1) -> Option<String> {
    claim
        .selection
        .issue
        .as_ref()
        .map(|issue| selection_identity(claim.selection.lifecycle_action, issue.number))
}

// ---------------------------------------------------------------------------
// Corpus DTOs. Cases evaluate in order: the claims that became active in
// earlier cases are the committed claim set for later cases.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkClaimCaseV1 {
    pub claim: WorkClaimV1,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkClaimScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkClaimCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkClaimIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkClaimScenarioV1>,
}

pub(crate) fn load_work_claim_corpus(body: &str) -> Result<WorkClaimIdentityCorpusV1, String> {
    let corpus: WorkClaimIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work claim identity corpus: {error}"))?;
    if corpus.schema_version != WORK_CLAIM_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work claim identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work claim identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    if corpus.scenarios.is_empty() {
        return Err("work claim identity corpus must carry at least one scenario".to_string());
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work claim identity corpus has an empty or duplicate scenario id `{}`",
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
                "scenario `{}` must carry at least one claim case",
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
            if case.claim.claim_id.trim().is_empty() || case.claim.basis_sha.trim().is_empty() {
                return Err(format!(
                    "scenario `{}` case {index} claim must record claim_id and basis_sha",
                    scenario.id
                ));
            }
            if case.claim.worktree.trim().is_empty() {
                return Err(format!(
                    "scenario `{}` case {index} claim must record the writer worktree it occupies",
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
pub(crate) struct WorkClaimCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub claim_state: String,
    pub claim_id: String,
    pub selection_key: Option<String>,
    pub worktree: String,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub claim_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkClaimScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub cases: Vec<WorkClaimCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkClaimCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkClaimCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkClaimCheckCountsV1,
    pub results: Vec<WorkClaimScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Claim law engine. Pure and read-only; reuses the RIPR-SPEC-0244 engine.
// ---------------------------------------------------------------------------

/// One active claim in the committed claim set, for interaction laws.
struct ActiveClaim {
    claim_id: String,
    selection_key: Option<String>,
    worktree: String,
}

fn claim_violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// Run every identity law for one claim row against the basis, the compiled
/// snapshot, the captured sources and the already-active claims. The
/// embedded selection must satisfy every RIPR-SPEC-0244 law first; the
/// claim-level laws then bind the repository, the portfolio-snapshot
/// identity, the basis head and the occupied worktree, and the interaction
/// laws reject duplicate claim ids, one worktree serving two selections and
/// one selection claimed by two worktrees — each wrong identity reported
/// once with its exact route. A conflict never releases or takes over the
/// occupying claim: reconciliation is the only route.
fn check_claim(
    claim: &WorkClaimV1,
    active: &[ActiveClaim],
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> CaseEvaluation {
    // #1635/#1647: the selected-work identity is required in every claim.
    let mut evaluation = check_packet(&claim.selection, basis, snapshot, captured);

    // Repository identity.
    if claim.repository != basis.repository.repository {
        evaluation.violations.push(claim_violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "claim names repository `{}` but the portfolio basis is `{}`; a claim for \
                 another repository is wrong input — recompile the basis for the intended \
                 repository with `cargo xtask work portfolio`",
                claim.repository, basis.repository.repository
            ),
        ));
    }

    // Portfolio-snapshot identity: a claim naming another snapshot is stale.
    if claim.portfolio_snapshot != basis.snapshot_identity {
        evaluation.violations.push(claim_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "claim binds portfolio snapshot `{}` but the compiled basis is `{}`; a claim \
                 for another portfolio snapshot is stale — recompile the basis and rebind the \
                 claim",
                claim.portfolio_snapshot, basis.snapshot_identity
            ),
        ));
    }

    // Basis identity: the recorded basis head must be the captured
    // default-branch head. A stale basis requires reconciliation, never
    // automatic release or takeover.
    if claim.basis_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(claim_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and reconcile the claim — \
                 it is not released or taken over",
                claim.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Worktree identity: the occupied worktree must be recorded in the
    // selection overlaps, so existing PR/claim/worktree overlap stays
    // visible before any mutation.
    if !claim.selection.overlaps.worktrees.contains(&claim.worktree) {
        evaluation.violations.push(claim_violation(
            WorkSelectionLawV1::WorktreeIdentity,
            WorkSelectionRouteV1::ReconcileResources,
            format!(
                "claim occupies worktree `{}` that the selection does not record in its \
                 overlaps; reconcile the writer resources before this claim may count",
                claim.worktree
            ),
        ));
    }

    // Duplicate claim id in the committed claim set.
    if active.iter().any(|row| row.claim_id == claim.claim_id) {
        evaluation.violations.push(claim_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            format!(
                "claim id `{}` is already active in this claim set; claim ids are durable \
                 identities, not reusable labels",
                claim.claim_id
            ),
        ));
    }

    // Interaction: one worktree cannot serve two selections at once. The
    // occupying claim stays active; the new claim reconciles.
    for row in active {
        if row.worktree == claim.worktree && row.selection_key != claim_selection_key(claim) {
            evaluation.violations.push(claim_violation(
                WorkSelectionLawV1::WorktreeIdentity,
                WorkSelectionRouteV1::ReconcileResources,
                format!(
                    "worktree `{}` is already claimed by `{}` for another selection; reconcile \
                     the writer resources — the occupying claim is not released or taken over",
                    claim.worktree, row.claim_id
                ),
            ));
        }
        // Interaction: one selection cannot be claimed by two worktrees;
        // claiming an item never asserts repository-wide priority, and two
        // writers on one selection is a reconciliation, not a race won.
        if row.selection_key.is_some()
            && row.selection_key == claim_selection_key(claim)
            && row.worktree != claim.worktree
        {
            evaluation.violations.push(claim_violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "selection `{}` is already claimed by `{}` on another worktree; reconcile \
                     the selection instead of double-claiming it",
                    claim_selection_key(claim).unwrap_or_default(),
                    row.claim_id
                ),
            ));
        }
    }

    // Legacy compatibility references at the claim level: resolve read-only
    // with migration posture, exactly as the packet law does; grant no
    // authority.
    if let Some(legacy_ref) = &claim.legacy_active_goal_ref {
        evaluation.migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical campaign \
             reference; it grants no write, readiness, merge or closeout authority"
        ));
        let resolved = claim.selection.campaign_refs.iter().any(|campaign_ref| {
            campaign_ref.id == *legacy_ref
                && campaign_ref.relation == RelevantCampaignRelationV1::Historical
        });
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !resolved || !known {
            evaluation.violations.push(claim_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign \
                     carried as a historical reference by the bound selection"
                ),
            ));
        }
    }
    if let Some(legacy_ref) = &claim.legacy_current_work_item_ref {
        evaluation.migration_posture.push(format!(
            "legacy_current_work_item_ref `{legacy_ref}` accepted as a read-only historical \
             work-item reference; it grants no write, readiness, merge or closeout authority"
        ));
        let matches_subject = claim
            .selection
            .issue
            .as_ref()
            .is_some_and(|issue| *legacy_ref == issue_identity(issue.number));
        if !matches_subject {
            evaluation.violations.push(claim_violation(
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

/// Claim state recorded on the projection: active claims occupy their
/// worktree; a stale basis keeps the claim recorded but demands
/// reconciliation; every other violation demands reconciliation too, never
/// silent release.
fn claim_state(violations: &[WorkSelectionLawViolationV1]) -> &'static str {
    if violations.is_empty() {
        "active"
    } else if violations
        .iter()
        .any(|violation| violation.law == WorkSelectionLawV1::BasisIdentity)
    {
        "stale_requires_reconcile"
    } else {
        "reconcile_required"
    }
}

// ---------------------------------------------------------------------------
// Corpus runner and projections.
// ---------------------------------------------------------------------------

fn run_claim_scenario(
    scenario: &WorkClaimScenarioV1,
    corpus: &WorkClaimIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkClaimScenarioResultV1, String> {
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
    let mut active: Vec<ActiveClaim> = Vec::new();
    for (index, case) in scenario.cases.iter().enumerate() {
        let evaluation = check_claim(&case.claim, &active, &basis, &snapshot, &captured);
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
        // Only violations-free claims join the committed claim set; a
        // conflicting or stale claim is recorded but never occupies
        // resources — reconcile, not takeover.
        if evaluation.violations.is_empty() {
            active.push(ActiveClaim {
                claim_id: case.claim.claim_id.clone(),
                selection_key: claim_selection_key(&case.claim),
                worktree: case.claim.worktree.clone(),
            });
        }
        let claim_identity = if evaluation.violations.is_empty() {
            Some(portable_identity("work claim", &case.claim)?)
        } else {
            None
        };
        let selection_key = claim_selection_key(&case.claim);
        results.push(WorkClaimCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            claim_state: claim_state(&evaluation.violations).to_string(),
            claim_id: case.claim.claim_id.clone(),
            selection_key,
            worktree: case.claim.worktree.clone(),
            violations: evaluation.violations,
            migration_posture: evaluation.migration_posture,
            claim_identity,
        });
    }
    Ok(WorkClaimScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        cases: results,
    })
}

pub(crate) fn run_work_claim_check(
    corpus_dir: &str,
    corpus: &WorkClaimIdentityCorpusV1,
) -> Result<WorkClaimCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_claim_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkClaimCheckViewV1 {
        schema_version: WORK_CLAIM_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkClaimCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work claim check".to_string(),
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_CLAIM_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work claim check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_claim_check_json(view: &WorkClaimCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

pub(crate) fn work_claim_check_markdown(view: &WorkClaimCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work durable-claim check\n\n");
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
            out.push_str(&format!(
                "## {} case {} — `{}` on `{}` ({})\n\n",
                scenario.scenario_id,
                case.case_index,
                case.claim_id,
                case.worktree,
                case.claim_state
            ));
            for row in &case.violations {
                out.push_str(&format!(
                    "- `{}` → `{}`: {}\n",
                    selection_wire_name(&row.law),
                    selection_wire_name(&row.route),
                    row.detail
                ));
            }
            if case.violations.is_empty() {
                out.push_str("- no violations; claim is active\n");
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

/// `cargo xtask work claim check [--corpus <dir>] [--json]` (#1706 PR B,
/// RIPR-SPEC-0270, consuming #1635/#1647): validate the committed
/// durable-claim corpus against the compiled portfolio bases and render
/// JSON or the derived Markdown. Read-only: no GitHub, branch, worktree,
/// claim, spec, campaign or source state is touched.
pub(crate) fn work_claim_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work claim check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_CLAIM_CORPUS_DIR.to_string();
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
    let corpus = load_work_claim_corpus(&body)?;
    let view = run_work_claim_check(&corpus_dir, &corpus)?;
    let json_body = work_claim_check_json(&view)?;
    crate::write_report("work-claim-check.json", &json_body)?;
    crate::write_report("work-claim-check.md", &work_claim_check_markdown(&view))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_claim_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work claim check: {} of {} cases failed; see work-claim-check.md",
            view.counts.failed, view.counts.cases
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

/// Fail closed on provenance paths that could escape the corpus root, with
/// the same cross-platform rules as the RIPR-SPEC-0244 provenance loader.
fn reject_unsafe_claim_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work claim identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work claim identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The committed claim-corpus scenario ids, pinned in declaration order.
pub(crate) const WORK_CLAIM_SCENARIO_IDS: [&str; 5] = [
    "claim-01-writer-claims-for-distinct-selections-pass",
    "claim-02-standalone-selection-claim-passes",
    "claim-03-stale-basis-reconciles-without-takeover",
    "claim-04-double-claim-of-one-selection-fails",
    "claim-05-legacy-claim-read-only-migration-posture",
];

/// Validate the committed durable-claim corpus fail-closed: provenance
/// digests in both directions, the five required scenario ids, and every
/// scenario's pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_claim_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_CLAIM_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_claim_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance: crate::work_selection_identity::WorkSelectionProvenanceV1 =
        match serde_json::from_str(&provenance_body)
            .map_err(|error| format!("parse work claim identity provenance: {error}"))
            .and_then(|parsed| {
                let parsed: crate::work_selection_identity::WorkSelectionProvenanceV1 = parsed;
                if parsed.schema_version != WORK_CLAIM_IDENTITY_PROVENANCE_SCHEMA_VERSION {
                    return Err(format!(
                        "unsupported work claim identity provenance schema `{}`",
                        parsed.schema_version
                    ));
                }
                if parsed.repository.trim().is_empty() || parsed.capture_method.trim().is_empty() {
                    return Err(
                        "work claim identity provenance must record repository and capture_method"
                            .to_string(),
                    );
                }
                if parsed.captured_at.trim().is_empty() {
                    return Err(
                        "work claim identity provenance must record captured_at".to_string()
                    );
                }
                Ok(parsed)
            }) {
            Ok(provenance) => provenance,
            Err(error) => {
                violations.push(format!("fixtures/work_claim_identity: {error}"));
                return;
            }
        };
    let mut provenance_paths = BTreeSet::new();
    for file in &provenance.files {
        if let Err(error) = reject_unsafe_claim_path(&file.path) {
            violations.push(format!("fixtures/work_claim_identity: {error}"));
        }
        if !provenance_paths.insert(file.path.clone()) {
            violations.push(format!(
                "fixtures/work_claim_identity: provenance duplicates path `{}`",
                file.path
            ));
        }
    }
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/work_claim_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_claim_identity: {} digest drifted: recorded `{}`, recomputed \
                 `work-claim:sha256:{}`",
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
                        "fixtures/work_claim_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_claim_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_claim_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_claim_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_claim_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_CLAIM_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_claim_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_CLAIM_SCENARIO_IDS
        ));
    }
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_claim_identity: corpus.json references `{forbidden}`; resume \
                 authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_claim_check(DEFAULT_WORK_CLAIM_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_claim_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_claim_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
