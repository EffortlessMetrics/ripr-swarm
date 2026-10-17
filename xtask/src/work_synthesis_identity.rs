//! Root synthesis consumer on the selected-work basis (#1706, delivery
//! slice PR B, RIPR-SPEC-0273, consuming #1636).
//!
//! `cargo xtask work synthesis check [--corpus <dir>] [--json]` validates
//! committed `WorkSynthesisV1` rows against the same `PortfolioBasisV1` the
//! RIPR-SPEC-0244 selection check compiles from immutable captured inputs
//! (default `fixtures/work_synthesis_identity`). Every synthesis binds the
//! repository, the portfolio-snapshot identity, the basis head and the full
//! selected-work identity it reports on; a synthesis for another
//! repository, portfolio snapshot, issue/action, basis or exact head fails
//! stale/wrong-input with the exact recompile/reconcile route recorded on
//! the violation. Human Markdown and JSON projections derive from one check
//! DTO and can never strengthen each other.
//!
//! Selection law: this module selects no work, claims nothing, authorizes
//! nothing and mutates nothing. A synthesis reports the selected
//! transition, its acceptance coverage, its contradictions and the next
//! action; the schema has no field that could create another selected
//! issue or switch campaign — `deny_unknown_fields` rejects such additions
//! — and a `reported_campaign_ref` that is not one of the bound selection's
//! campaign references fails the campaign-ref law. It reuses the
//! RIPR-SPEC-0244 identity law engine instead of forking one: each
//! synthesis embeds the `SelectedWorkIdentityV1` it reports on, and every
//! law that engine already enforces for a packet is enforced for the
//! embedded selection before the synthesis-level laws run. Legacy
//! `legacy_active_goal_ref` / `legacy_current_work_item_ref` fields resolve
//! read-only to historical campaign references, emit migration posture, and
//! grant no write, readiness, merge or closeout authority.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::work_portfolio::{
    WorkCandidateKindV1, WorkCapturedDirV1, WorkPortfolioSnapshotV1, compile_work_portfolio,
    load_work_captured_dir, portable_identity, work_portfolio_json, workspace_path,
};
use crate::work_selection_identity::{
    CaseEvaluation, PortfolioBasisV1, RelevantCampaignRelationV1, SelectedWorkIdentityV1,
    WorkSelectionExpectV1, WorkSelectionExpectedViolationV1, WorkSelectionLawV1,
    WorkSelectionLawViolationV1, WorkSelectionRouteV1, build_portfolio_basis, check_packet,
    corpus_root, resolve_scenario_captured, selection_wire_name,
};

pub(crate) const WORK_SYNTHESIS_IDENTITY_CORPUS_SCHEMA_VERSION: &str =
    "work_synthesis_identity_corpus.v1";
pub(crate) const WORK_SYNTHESIS_CHECK_VIEW_SCHEMA_VERSION: &str = "work_synthesis_check_view.v1";
pub(crate) const WORK_SYNTHESIS_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_synthesis_identity_provenance.v1";

pub(crate) const DEFAULT_WORK_SYNTHESIS_CORPUS_DIR: &str = "fixtures/work_synthesis_identity";

pub(crate) const WORK_SYNTHESIS_CHECK_CLAIM_BOUNDARY: &str = "Read-only root-synthesis check: \
 the check validates committed synthesis rows against one compiled portfolio basis and the \
 selected-work identity each synthesis reports on; it selects no work, claims nothing, \
 authorizes no writer, creates no selection, switches no campaign, grants no readiness, merge \
 or closeout authority, mutates no GitHub state, branch, worktree, claim, spec, campaign or \
 source, and it never fabricates identity from missing evidence.";

// ---------------------------------------------------------------------------
// Synthesis DTOs (#1636): a synthesis reports one selected transition, its
// acceptance coverage, its contradictions and the next action. It can
// create no selection and switch no campaign.
// ---------------------------------------------------------------------------

/// Closed acceptance-row status vocabulary for the #1636 synthesis
/// consumer. A row is a recorded observation about one requirement; it is
/// never an authority grant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkAcceptanceStatusV1 {
    Covered,
    Uncovered,
    Contradicted,
}

/// One acceptance-coverage row: one accepted requirement or spec ref of the
/// bound selection and the observed status.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkAcceptanceRowV1 {
    pub requirement: String,
    pub status: WorkAcceptanceStatusV1,
}

/// One root synthesis row (#1636). It binds the repository, the compiled
/// portfolio-snapshot identity, the basis head and the full selected-work
/// identity it reports on. It must not contain a repository-wide
/// current/default campaign, a mutable agent assignment, CI wait state, or
/// a progress percentage; the embedded selection schema already rejects
/// those, `deny_unknown_fields` rejects additions here, and no field on a
/// synthesis grants write, readiness, merge or closeout authority. The
/// schema deliberately has no field that could create another selected
/// issue: a synthesis reports, it never selects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSynthesisV1 {
    pub synthesis_id: String,
    pub repository: String,
    /// The compiled portfolio snapshot identity the synthesis binds; a
    /// synthesis naming another snapshot is stale and routes recompilation.
    pub portfolio_snapshot: String,
    /// The basis (default-branch head) the synthesis was produced against.
    pub basis_sha: String,
    /// The full selected-work identity this synthesis reports on.
    pub selection: SelectedWorkIdentityV1,
    /// The selected transition reported; it must match the bound
    /// selection's lifecycle action — a synthesis cannot silently report a
    /// different action than the one selected.
    pub reported_transition: WorkCandidateKindV1,
    /// Acceptance coverage over the bound selection's accepted requirements
    /// and spec refs. A synthesis with no rows reports nothing.
    pub acceptance_coverage: Vec<WorkAcceptanceRowV1>,
    /// Contradictions observed while producing the synthesis. Every
    /// `Uncovered` or `Contradicted` acceptance row must be named here, so
    /// a synthesis can never hide a gap.
    pub contradictions: Vec<String>,
    /// The bounded next action. A synthesis never encodes a new selection:
    /// this names the next bounded step for the already-selected work.
    pub next_action: String,
    /// Retained artifact references the synthesis derives from. A
    /// synthesis derived from nothing is a claim, not a synthesis.
    pub derived_from: Vec<String>,
    /// The campaign reference the synthesis reports under, when any; it
    /// must be one of the bound selection's campaign references, so a
    /// synthesis cannot silently switch campaign.
    pub reported_campaign_ref: Option<String>,
    /// Legacy compatibility references (read-only; see module docs).
    pub legacy_active_goal_ref: Option<String>,
    pub legacy_current_work_item_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// Corpus DTOs.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSynthesisCaseV1 {
    pub synthesis: WorkSynthesisV1,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSynthesisScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkSynthesisCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSynthesisIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkSynthesisScenarioV1>,
}

pub(crate) fn load_work_synthesis_corpus(
    body: &str,
) -> Result<WorkSynthesisIdentityCorpusV1, String> {
    let corpus: WorkSynthesisIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work synthesis identity corpus: {error}"))?;
    if corpus.schema_version != WORK_SYNTHESIS_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work synthesis identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work synthesis identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    if corpus.scenarios.is_empty() {
        return Err("work synthesis identity corpus must carry at least one scenario".to_string());
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work synthesis identity corpus has an empty or duplicate scenario id `{}`",
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
                "scenario `{}` must carry at least one synthesis case",
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
            if case.synthesis.synthesis_id.trim().is_empty()
                || case.synthesis.basis_sha.trim().is_empty()
            {
                return Err(format!(
                    "scenario `{}` case {index} synthesis must record synthesis_id and basis_sha",
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
pub(crate) struct WorkSynthesisCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub synthesis_id: String,
    pub selection_key: Option<String>,
    pub uncovered_requirements: Vec<String>,
    pub contradiction_count: u64,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub synthesis_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSynthesisScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub cases: Vec<WorkSynthesisCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSynthesisCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSynthesisCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkSynthesisCheckCountsV1,
    pub results: Vec<WorkSynthesisScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Synthesis law engine. Pure and read-only; reuses the RIPR-SPEC-0244
// engine.
// ---------------------------------------------------------------------------

fn synthesis_violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// Run every identity law for one synthesis row against the basis, the
/// compiled snapshot and the captured sources. The embedded selection must
/// satisfy every RIPR-SPEC-0244 law first; the synthesis-level laws then
/// bind the repository, the portfolio-snapshot identity, the basis head,
/// the reported transition, the acceptance coverage, the contradiction
/// honesty, the bounded next action, the derived artifacts and the reported
/// campaign reference. Legacy references resolve read-only and emit
/// migration posture. Each wrong identity is reported once with its exact
/// recompile/reconcile route.
fn check_synthesis(
    synthesis: &WorkSynthesisV1,
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> CaseEvaluation {
    // #1636: the selected-work identity is required in every synthesis.
    // Reuse the shared law engine so a synthesis cannot smuggle in a
    // selection the selection check would reject.
    let mut evaluation = check_packet(&synthesis.selection, basis, snapshot, captured);

    // Repository identity: the synthesis must name the basis repository.
    if synthesis.repository != basis.repository.repository {
        evaluation.violations.push(synthesis_violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "synthesis names repository `{}` but the portfolio basis is `{}`; a synthesis \
                 for another repository is wrong input — recompile the basis for the intended \
                 repository with `cargo xtask work portfolio`",
                synthesis.repository, basis.repository.repository
            ),
        ));
    }

    // Portfolio-snapshot identity: the synthesis must bind this compiled
    // snapshot; a synthesis for another snapshot is stale.
    if synthesis.portfolio_snapshot != basis.snapshot_identity {
        evaluation.violations.push(synthesis_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "synthesis binds portfolio snapshot `{}` but the compiled basis is `{}`; a \
                 synthesis for another portfolio snapshot is stale — recompile the basis and \
                 rebind the synthesis",
                synthesis.portfolio_snapshot, basis.snapshot_identity
            ),
        ));
    }

    // Basis identity: the recorded basis head must be the captured
    // default-branch head.
    if synthesis.basis_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(synthesis_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and rebind the synthesis",
                synthesis.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Transition identity: the reported transition must match the bound
    // selection's lifecycle action; a synthesis cannot silently report a
    // different action than the one selected.
    if synthesis.reported_transition != synthesis.selection.lifecycle_action {
        evaluation.violations.push(synthesis_violation(
            WorkSelectionLawV1::ActionIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            format!(
                "synthesis reports transition `{}` but the bound selection's action is `{}`; \
                 reconcile the synthesis to the selected transition",
                selection_wire_name(&synthesis.reported_transition),
                selection_wire_name(&synthesis.selection.lifecycle_action)
            ),
        ));
    }

    // Acceptance coverage: a synthesis with no rows reports nothing.
    if synthesis.acceptance_coverage.is_empty() {
        evaluation.violations.push(synthesis_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "synthesis records no acceptance-coverage rows; report the observed coverage of \
             the bound selection's accepted requirements and spec refs"
                .to_string(),
        ));
    }

    // Contradiction honesty: every Uncovered or Contradicted acceptance row
    // must be named in the contradictions list, so a synthesis can never
    // hide a gap behind a green row.
    for row in &synthesis.acceptance_coverage {
        if matches!(
            row.status,
            WorkAcceptanceStatusV1::Uncovered | WorkAcceptanceStatusV1::Contradicted
        ) && !synthesis
            .contradictions
            .iter()
            .any(|contradiction| contradiction.contains(&row.requirement))
        {
            evaluation.violations.push(synthesis_violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "acceptance row `{}` is {} but no contradiction entry names it; a synthesis \
                     must surface every gap it records",
                    row.requirement,
                    selection_wire_name(&row.status)
                ),
            ));
        }
    }

    // Next action: a synthesis must name a bounded next action for the
    // already-selected work; it never encodes a new selection.
    if synthesis.next_action.trim().is_empty() {
        evaluation.violations.push(synthesis_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "synthesis records an empty next action; name the bounded next step for the \
             already-selected work"
                .to_string(),
        ));
    }

    // Derived-artifact identity: a synthesis derived from nothing is a
    // claim, not a synthesis.
    if synthesis.derived_from.is_empty() {
        evaluation.violations.push(synthesis_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "synthesis records no derived_from artifact reference; bind the retained packets, \
             receipts or run rows the synthesis derives from"
                .to_string(),
        ));
    }

    // Campaign identity: when the synthesis reports under a campaign
    // reference, it must be one of the bound selection's campaign
    // references; a synthesis cannot silently switch campaign.
    if let Some(reported) = &synthesis.reported_campaign_ref
        && !synthesis
            .selection
            .campaign_refs
            .iter()
            .any(|campaign_ref| campaign_ref.id == *reported)
    {
        evaluation.violations.push(synthesis_violation(
            WorkSelectionLawV1::CampaignRefIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            format!(
                "synthesis reports under campaign `{reported}` which the bound selection does \
                 not carry; a synthesis cannot silently switch campaign"
            ),
        ));
    }

    // Legacy compatibility references at the synthesis level: resolve
    // read-only with migration posture, exactly as the selection law does;
    // grant no authority.
    if let Some(legacy_ref) = &synthesis.legacy_active_goal_ref {
        evaluation.migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical campaign \
             reference; it grants no write, readiness, merge or closeout authority"
        ));
        let resolved = synthesis
            .selection
            .campaign_refs
            .iter()
            .any(|campaign_ref| {
                campaign_ref.id == *legacy_ref
                    && campaign_ref.relation == RelevantCampaignRelationV1::Historical
            });
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !resolved || !known {
            evaluation.violations.push(synthesis_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign \
                     carried as a historical reference by the bound selection"
                ),
            ));
        }
    }
    if let Some(legacy_ref) = &synthesis.legacy_current_work_item_ref {
        evaluation.migration_posture.push(format!(
            "legacy_current_work_item_ref `{legacy_ref}` accepted as a read-only historical \
             work-item reference; it grants no write, readiness, merge or closeout authority"
        ));
        let matches_subject = synthesis.selection.issue.as_ref().is_some_and(|issue| {
            *legacy_ref == crate::work_selection_identity::issue_identity(issue.number)
        });
        if !matches_subject {
            evaluation.violations.push(synthesis_violation(
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

fn run_synthesis_scenario(
    scenario: &WorkSynthesisScenarioV1,
    corpus: &WorkSynthesisIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkSynthesisScenarioResultV1, String> {
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
        let evaluation = check_synthesis(&case.synthesis, &basis, &snapshot, &captured);
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
        let synthesis_identity = if evaluation.violations.is_empty() {
            Some(portable_identity("work synthesis", &case.synthesis)?)
        } else {
            None
        };
        let selection_key = case.synthesis.selection.issue.as_ref().map(|issue| {
            crate::work_selection_identity::selection_identity(
                case.synthesis.selection.lifecycle_action,
                issue.number,
            )
        });
        let uncovered_requirements = case
            .synthesis
            .acceptance_coverage
            .iter()
            .filter(|row| row.status != WorkAcceptanceStatusV1::Covered)
            .map(|row| row.requirement.clone())
            .collect();
        results.push(WorkSynthesisCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            synthesis_id: case.synthesis.synthesis_id.clone(),
            selection_key,
            uncovered_requirements,
            contradiction_count: case.synthesis.contradictions.len() as u64,
            violations: evaluation.violations,
            migration_posture: evaluation.migration_posture,
            synthesis_identity,
        });
    }
    Ok(WorkSynthesisScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        cases: results,
    })
}

pub(crate) fn run_work_synthesis_check(
    corpus_dir: &str,
    corpus: &WorkSynthesisIdentityCorpusV1,
) -> Result<WorkSynthesisCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_synthesis_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkSynthesisCheckViewV1 {
        schema_version: WORK_SYNTHESIS_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkSynthesisCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work synthesis check".to_string(),
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_SYNTHESIS_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work synthesis check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_synthesis_check_json(view: &WorkSynthesisCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

pub(crate) fn work_synthesis_check_markdown(view: &WorkSynthesisCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work root-synthesis check\n\n");
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
    let mut uncovered_total = 0_u64;
    let mut contradiction_total = 0_u64;
    for case in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
    {
        uncovered_total += case.uncovered_requirements.len() as u64;
        contradiction_total += case.contradiction_count;
    }
    out.push_str(&format!("- uncovered acceptance rows: {uncovered_total}\n"));
    out.push_str(&format!(
        "- recorded contradictions: {contradiction_total}\n\n"
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
            if !case.uncovered_requirements.is_empty() {
                out.push_str(&format!(
                    "- {} case {} uncovered: {}\n",
                    scenario.scenario_id,
                    case.case_index,
                    case.uncovered_requirements.join(", ")
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

/// `cargo xtask work synthesis check [--corpus <dir>] [--json]` (#1706 PR B,
/// RIPR-SPEC-0273, consuming #1636): validate the committed root-synthesis
/// corpus against the compiled portfolio bases and render JSON or the
/// derived Markdown. Read-only: no GitHub, branch, worktree, claim, spec,
/// campaign or source state is touched.
pub(crate) fn work_synthesis_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work synthesis check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_SYNTHESIS_CORPUS_DIR.to_string();
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
    let corpus = load_work_synthesis_corpus(&body)?;
    let view = run_work_synthesis_check(&corpus_dir, &corpus)?;
    let json_body = work_synthesis_check_json(&view)?;
    crate::write_report("work-synthesis-check.json", &json_body)?;
    crate::write_report(
        "work-synthesis-check.md",
        &work_synthesis_check_markdown(&view),
    )?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_synthesis_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work synthesis check: {} of {} cases failed; see work-synthesis-check.md",
            view.counts.failed, view.counts.cases
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

/// Load the committed synthesis-corpus provenance fail-closed.
pub(crate) fn load_work_synthesis_provenance(
    body: &str,
) -> Result<crate::work_selection_identity::WorkSelectionProvenanceV1, String> {
    let provenance: crate::work_selection_identity::WorkSelectionProvenanceV1 =
        serde_json::from_str(body)
            .map_err(|error| format!("parse work synthesis identity provenance: {error}"))?;
    if provenance.schema_version != WORK_SYNTHESIS_IDENTITY_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work synthesis identity provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "work synthesis identity provenance must record repository and capture_method"
                .to_string(),
        );
    }
    if provenance.captured_at.trim().is_empty() {
        return Err("work synthesis identity provenance must record captured_at".to_string());
    }
    let mut paths = BTreeSet::new();
    for file in &provenance.files {
        reject_unsafe_synthesis_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(format!(
                "work synthesis identity provenance duplicates path `{}`",
                file.path
            ));
        }
    }
    Ok(provenance)
}

/// Fail closed on provenance paths that could escape the corpus root, with
/// the same cross-platform rules as the RIPR-SPEC-0244 provenance loader.
fn reject_unsafe_synthesis_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work synthesis identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work synthesis identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The committed synthesis-corpus scenario ids, pinned in declaration order.
pub(crate) const WORK_SYNTHESIS_SCENARIO_IDS: [&str; 5] = [
    "synthesis-01-covered-transition-synthesis-passes",
    "synthesis-02-standalone-synthesis-passes",
    "synthesis-03-partial-coverage-contradiction-surfaced",
    "synthesis-04-hidden-gap-silent-switch-and-wrong-inputs-fail",
    "synthesis-05-legacy-read-only-migration-posture",
];

/// Validate the committed root-synthesis corpus fail-closed: provenance
/// digests in both directions, the five required scenario ids, and every
/// scenario's pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_synthesis_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_SYNTHESIS_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_synthesis_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_work_synthesis_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/work_synthesis_identity: {error}"));
            return;
        }
    };
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/work_synthesis_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_synthesis_identity: {} digest drifted: recorded `{}`, recomputed \
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
                        "fixtures/work_synthesis_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_synthesis_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_synthesis_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_synthesis_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_synthesis_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_SYNTHESIS_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_synthesis_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_SYNTHESIS_SCENARIO_IDS
        ));
    }
    // The committed corpus must never carry chat or singleton goal-file
    // authority bytes.
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_synthesis_identity: corpus.json references `{forbidden}`; resume \
                 authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_synthesis_check(DEFAULT_WORK_SYNTHESIS_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_synthesis_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_synthesis_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
