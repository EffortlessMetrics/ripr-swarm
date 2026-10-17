//! Issue intake consumer on the selected-work basis (#1706, delivery slice
//! PR B, RIPR-SPEC-0275, consuming #1644).
//!
//! `cargo xtask work intake check [--corpus <dir>] [--json]` validates
//! committed `WorkIntakeV1` rows against the same `PortfolioBasisV1` the
//! RIPR-SPEC-0244 selection check compiles from immutable captured inputs
//! (default `fixtures/work_intake_identity`). Every intake binds the
//! repository, the portfolio-snapshot identity, the basis head, the
//! current main and one issue the captured portfolio records; an intake
//! for another repository, portfolio snapshot, basis, current main or
//! unknown issue fails stale/wrong-input with the exact
//! recompile/reconcile route recorded on the violation. Human Markdown and
//! JSON projections derive from one check DTO and can never strengthen
//! each other.
//!
//! Selection law: this module selects no work, claims nothing, authorizes
//! nothing and mutates nothing. Intake consumes a portfolio snapshot,
//! relevant campaigns/plans, open overlapping PRs and claims, the current
//! main and accepted contracts; an issue outside every campaign may still
//! qualify — placement remains a later root/planning decision, so the
//! schema has no required campaign placement and no placement grants
//! authority. Legacy `legacy_active_goal_ref` /
//! `legacy_current_work_item_ref` fields resolve read-only to recorded
//! campaigns and the intake issue, emit migration posture, and grant no
//! write, readiness, merge or closeout authority.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::work_portfolio::{
    WorkCapturedDirV1, WorkPortfolioSnapshotV1, compile_work_portfolio, load_work_captured_dir,
    portable_identity, work_portfolio_json, workspace_path,
};
use crate::work_selection_identity::{
    PortfolioBasisV1, WorkSelectionExpectV1, WorkSelectionExpectedViolationV1, WorkSelectionLawV1,
    WorkSelectionLawViolationV1, WorkSelectionRouteV1, build_portfolio_basis, corpus_root,
    issue_identity, resolve_scenario_captured, selection_wire_name,
};

pub(crate) const WORK_INTAKE_IDENTITY_CORPUS_SCHEMA_VERSION: &str =
    "work_intake_identity_corpus.v1";
pub(crate) const WORK_INTAKE_CHECK_VIEW_SCHEMA_VERSION: &str = "work_intake_check_view.v1";
pub(crate) const WORK_INTAKE_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_intake_identity_provenance.v1";

pub(crate) const DEFAULT_WORK_INTAKE_CORPUS_DIR: &str = "fixtures/work_intake_identity";

pub(crate) const WORK_INTAKE_CHECK_CLAIM_BOUNDARY: &str = "Read-only issue-intake check: the \
 check validates committed intake rows against one compiled portfolio basis; it selects no \
 work, claims nothing, authorizes no writer, places no campaign, grants no readiness, merge \
 or closeout authority, mutates no GitHub state, branch, worktree, claim, spec, campaign or \
 source, and it never fabricates identity from missing evidence.";

// ---------------------------------------------------------------------------
// Intake DTOs (#1644): intake consumes the portfolio snapshot, relevant
// campaigns/plans, open overlapping PRs and claims, the current main and
// accepted contracts. An out-of-campaign issue may still qualify.
// ---------------------------------------------------------------------------

/// Closed qualification vocabulary for the #1644 intake consumer. A row is
/// a recorded triage observation, never an authority grant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkQualificationV1 {
    Qualified,
    NeedsTriage,
}

/// One issue intake row (#1644). It binds the repository, the compiled
/// portfolio-snapshot identity, the basis head, the current main and one
/// issue the captured portfolio records. It must not contain a
/// repository-wide current/default campaign, a mutable agent assignment,
/// CI wait state, or a progress percentage; `deny_unknown_fields` rejects
/// such additions, and no field on an intake grants write, readiness,
/// merge or closeout authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkIntakeV1 {
    pub intake_id: String,
    pub repository: String,
    /// The compiled portfolio snapshot identity the intake binds; an
    /// intake naming another snapshot is stale and routes recompilation.
    pub portfolio_snapshot: String,
    /// The basis (default-branch head) the intake was produced against.
    pub basis_sha: String,
    /// The current main head the intake derives against; it must match the
    /// captured default-branch head.
    pub current_main_sha: String,
    /// The issue number being qualified; it must be recorded in the
    /// captured issue source.
    pub issue_number: u64,
    /// The recorded qualification. An out-of-campaign issue may still be
    /// `Qualified`; placement remains a later root/planning decision.
    pub qualification: WorkQualificationV1,
    /// The campaign placement under consideration, when any; it must be a
    /// recorded campaign. `None` means standalone: valid triage input,
    /// never a blocker.
    pub campaign_placement: Option<String>,
    /// Relevant campaign/plan references; each must be a recorded
    /// campaign.
    pub relevant_campaigns: Vec<String>,
    /// Open overlapping PRs and claims that must stay visible before any
    /// mutation.
    pub open_overlaps: Vec<String>,
    /// Accepted contracts the intake was qualified against; an empty list
    /// qualifies nothing.
    pub accepted_contracts: Vec<String>,
    /// Whether the captured issue prefers a single agent.
    pub single_agent: bool,
    /// Retained artifact references the intake derives from. An intake
    /// derived from nothing is a claim, not an intake.
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
pub(crate) struct WorkIntakeCaseV1 {
    pub intake: WorkIntakeV1,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkIntakeScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkIntakeCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkIntakeIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkIntakeScenarioV1>,
}

pub(crate) fn load_work_intake_corpus(body: &str) -> Result<WorkIntakeIdentityCorpusV1, String> {
    let corpus: WorkIntakeIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work intake identity corpus: {error}"))?;
    if corpus.schema_version != WORK_INTAKE_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work intake identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work intake identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    if corpus.scenarios.is_empty() {
        return Err("work intake identity corpus must carry at least one scenario".to_string());
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work intake identity corpus has an empty or duplicate scenario id `{}`",
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
                "scenario `{}` must carry at least one intake case",
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
            if case.intake.intake_id.trim().is_empty() || case.intake.basis_sha.trim().is_empty() {
                return Err(format!(
                    "scenario `{}` case {index} intake must record intake_id and basis_sha",
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
pub(crate) struct WorkIntakeCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub intake_id: String,
    pub issue_key: String,
    pub qualification: WorkQualificationV1,
    pub overlap_count: u64,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub intake_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkIntakeScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub cases: Vec<WorkIntakeCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkIntakeCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkIntakeCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkIntakeCheckCountsV1,
    pub results: Vec<WorkIntakeScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Intake law engine. Pure and read-only.
// ---------------------------------------------------------------------------

fn intake_violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// True when the captured issue source records the issue number.
fn captured_issue_known(captured: &WorkCapturedDirV1, number: u64) -> bool {
    captured
        .issues
        .as_ref()
        .is_some_and(|issues| issues.issues.iter().any(|issue| issue.number == number))
}

/// Run every identity law for one intake row against the basis, the
/// compiled snapshot and the captured sources. The intake-level laws bind
/// the repository, the portfolio-snapshot identity, the basis head, the
/// current main, the recorded issue, the accepted contracts, the campaign
/// references and the derived artifacts. Legacy references resolve
/// read-only and emit migration posture. Each wrong identity is reported
/// once with its exact recompile/reconcile route.
fn check_intake(
    intake: &WorkIntakeV1,
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> (Vec<WorkSelectionLawViolationV1>, Vec<String>) {
    let mut violations = Vec::new();
    let mut migration_posture = Vec::new();

    // Repository identity: the intake must name the basis repository.
    if intake.repository != basis.repository.repository {
        violations.push(intake_violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "intake names repository `{}` but the portfolio basis is `{}`; an intake for \
                 another repository is wrong input — recompile the basis for the intended \
                 repository with `cargo xtask work portfolio`",
                intake.repository, basis.repository.repository
            ),
        ));
    }

    // Portfolio-snapshot identity: the intake must bind this compiled
    // snapshot; an intake for another snapshot is stale.
    if intake.portfolio_snapshot != basis.snapshot_identity {
        violations.push(intake_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "intake binds portfolio snapshot `{}` but the compiled basis is `{}`; an \
                 intake for another portfolio snapshot is stale — recompile the basis and \
                 rebind the intake",
                intake.portfolio_snapshot, basis.snapshot_identity
            ),
        ));
    }

    // Basis identity: the recorded basis head must be the captured
    // default-branch head.
    if intake.basis_sha != basis.repository.default_branch_sha {
        violations.push(intake_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and rebind the intake",
                intake.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Current-main identity: the intake must derive against the captured
    // default-branch head.
    if intake.current_main_sha != basis.repository.default_branch_sha {
        violations.push(intake_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "intake derives against current main `{}` but the captured default-branch head \
                 is `{}`; recompile the basis before qualifying the issue",
                intake.current_main_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Subject identity: the issue must be recorded in the captured issue
    // source; an intake for an unknown issue fabricates triage input.
    if !captured_issue_known(captured, intake.issue_number) {
        violations.push(intake_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            format!(
                "intake qualifies issue `{}` which the captured portfolio does not record; \
                 recompile the basis from the intended repository",
                issue_identity(intake.issue_number)
            ),
        ));
    }

    // Accepted contracts: an intake qualified against nothing qualifies
    // nothing.
    if intake.accepted_contracts.is_empty() {
        violations.push(intake_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "intake records no accepted contracts; bind the accepted requirement/spec/contract \
             references the qualification derives from"
                .to_string(),
        ));
    }

    // Derived-artifact identity: an intake derived from nothing is a
    // claim, not an intake.
    if intake.derived_from.is_empty() {
        violations.push(intake_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "intake records no derived_from artifact reference; bind the retained portfolio \
             snapshot, campaign or issue sources the intake derives from"
                .to_string(),
        ));
    }

    // Campaign identity: the placement under consideration and every
    // relevant campaign reference must be recorded campaigns. An
    // out-of-campaign issue (no placement, no relevant campaigns) still
    // qualifies; placement is a later root/planning decision.
    let mut campaign_refs: Vec<&str> = intake
        .relevant_campaigns
        .iter()
        .map(String::as_str)
        .collect();
    if let Some(placement) = &intake.campaign_placement {
        campaign_refs.push(placement);
    }
    for campaign_ref in campaign_refs {
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == campaign_ref);
        if !known {
            violations.push(intake_violation(
                WorkSelectionLawV1::CampaignRefIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "campaign reference `{campaign_ref}` is not a recorded campaign; intake \
                     campaign references must stay source-linked"
                ),
            ));
        }
    }

    // Legacy compatibility references: resolve read-only with migration
    // posture; grant no authority. A legacy active-goal ref must name a
    // recorded campaign, and a legacy current-work-item ref must match the
    // intake issue — a legacy packet attempting to authorize intake
    // authority fails.
    if let Some(legacy_ref) = &intake.legacy_active_goal_ref {
        migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical campaign \
             reference; it grants no write, readiness, merge or closeout authority"
        ));
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !known {
            violations.push(intake_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign; \
                     legacy references cannot authorize intake"
                ),
            ));
        }
    }
    if let Some(legacy_ref) = &intake.legacy_current_work_item_ref {
        migration_posture.push(format!(
            "legacy_current_work_item_ref `{legacy_ref}` accepted as a read-only historical \
             work-item reference; it grants no write, readiness, merge or closeout authority"
        ));
        if *legacy_ref != issue_identity(intake.issue_number) {
            violations.push(intake_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_current_work_item_ref `{legacy_ref}` does not match the intake \
                     issue `{}`",
                    issue_identity(intake.issue_number)
                ),
            ));
        }
    }

    (violations, migration_posture)
}

// ---------------------------------------------------------------------------
// Corpus runner and projections.
// ---------------------------------------------------------------------------

fn run_intake_scenario(
    scenario: &WorkIntakeScenarioV1,
    corpus: &WorkIntakeIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkIntakeScenarioResultV1, String> {
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
        let (violations, migration_posture) =
            check_intake(&case.intake, &basis, &snapshot, &captured);
        let passed = match case.expect {
            WorkSelectionExpectV1::Pass => violations.is_empty(),
            WorkSelectionExpectV1::Fail => {
                // Exact match on the law/route pair multiset: a negative case
                // pins every occurrence, so an added spurious violation
                // cannot stay green.
                let mut actual_pairs: Vec<(WorkSelectionLawV1, WorkSelectionRouteV1)> =
                    violations.iter().map(|row| (row.law, row.route)).collect();
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
        let intake_identity = if violations.is_empty() {
            Some(portable_identity("work intake", &case.intake)?)
        } else {
            None
        };
        results.push(WorkIntakeCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            intake_id: case.intake.intake_id.clone(),
            issue_key: issue_identity(case.intake.issue_number),
            qualification: case.intake.qualification,
            overlap_count: case.intake.open_overlaps.len() as u64,
            violations,
            migration_posture,
            intake_identity,
        });
    }
    Ok(WorkIntakeScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        cases: results,
    })
}

pub(crate) fn run_work_intake_check(
    corpus_dir: &str,
    corpus: &WorkIntakeIdentityCorpusV1,
) -> Result<WorkIntakeCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_intake_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkIntakeCheckViewV1 {
        schema_version: WORK_INTAKE_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkIntakeCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work intake check".to_string(),
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_INTAKE_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work intake check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_intake_check_json(view: &WorkIntakeCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

pub(crate) fn work_intake_check_markdown(view: &WorkIntakeCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work issue-intake check\n\n");
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
    let mut qualified = 0_u64;
    let mut overlaps_total = 0_u64;
    for case in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
    {
        if case.qualification == WorkQualificationV1::Qualified {
            qualified += 1;
        }
        overlaps_total += case.overlap_count;
    }
    out.push_str(&format!("- qualified intakes: {qualified}\n"));
    out.push_str(&format!("- open overlaps recorded: {overlaps_total}\n\n"));
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

/// `cargo xtask work intake check [--corpus <dir>] [--json]` (#1706 PR B,
/// RIPR-SPEC-0275, consuming #1644): validate the committed issue-intake
/// corpus against the compiled portfolio bases and render JSON or the
/// derived Markdown. Read-only: no GitHub, branch, worktree, claim, spec,
/// campaign or source state is touched.
pub(crate) fn work_intake_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work intake check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_INTAKE_CORPUS_DIR.to_string();
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
    let corpus = load_work_intake_corpus(&body)?;
    let view = run_work_intake_check(&corpus_dir, &corpus)?;
    let json_body = work_intake_check_json(&view)?;
    crate::write_report("work-intake-check.json", &json_body)?;
    crate::write_report("work-intake-check.md", &work_intake_check_markdown(&view))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_intake_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work intake check: {} of {} cases failed; see work-intake-check.md",
            view.counts.failed, view.counts.cases
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

/// Load the committed intake-corpus provenance fail-closed.
pub(crate) fn load_work_intake_provenance(
    body: &str,
) -> Result<crate::work_selection_identity::WorkSelectionProvenanceV1, String> {
    let provenance: crate::work_selection_identity::WorkSelectionProvenanceV1 =
        serde_json::from_str(body)
            .map_err(|error| format!("parse work intake identity provenance: {error}"))?;
    if provenance.schema_version != WORK_INTAKE_IDENTITY_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work intake identity provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "work intake identity provenance must record repository and capture_method".to_string(),
        );
    }
    if provenance.captured_at.trim().is_empty() {
        return Err("work intake identity provenance must record captured_at".to_string());
    }
    let mut paths = BTreeSet::new();
    for file in &provenance.files {
        reject_unsafe_intake_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(format!(
                "work intake identity provenance duplicates path `{}`",
                file.path
            ));
        }
    }
    Ok(provenance)
}

/// Fail closed on provenance paths that could escape the corpus root, with
/// the same cross-platform rules as the RIPR-SPEC-0244 provenance loader.
fn reject_unsafe_intake_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work intake identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work intake identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The committed intake-corpus scenario ids, pinned in declaration order.
pub(crate) const WORK_INTAKE_SCENARIO_IDS: [&str; 5] = [
    "intake-01-in-campaign-qualified-passes",
    "intake-02-out-of-campaign-still-qualifies",
    "intake-03-pr-repair-intake-overlap-visible",
    "intake-04-narrow-single-agent-intake-passes",
    "intake-05-legacy-and-wrong-inputs-fail",
];

/// Validate the committed issue-intake corpus fail-closed: provenance
/// digests in both directions, the five required scenario ids, and every
/// scenario's pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_intake_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_INTAKE_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_intake_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_work_intake_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/work_intake_identity: {error}"));
            return;
        }
    };
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/work_intake_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_intake_identity: {} digest drifted: recorded `{}`, recomputed \
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
                        "fixtures/work_intake_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_intake_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_intake_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_intake_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_intake_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_INTAKE_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_intake_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_INTAKE_SCENARIO_IDS
        ));
    }
    // The committed corpus must never carry chat or singleton goal-file
    // authority bytes.
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_intake_identity: corpus.json references `{forbidden}`; resume \
                 authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_intake_check(DEFAULT_WORK_INTAKE_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_intake_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_intake_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
