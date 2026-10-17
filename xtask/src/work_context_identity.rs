//! Context/work-item packet consumer on the selected-work basis (#1706,
//! delivery slice PR B, RIPR-SPEC-0272, consuming #1632).
//!
//! `cargo xtask work context check [--corpus <dir>] [--json]` validates
//! committed `WorkContextPacketV1` rows against the same `PortfolioBasisV1`
//! the RIPR-SPEC-0244 selection check compiles from immutable captured
//! inputs (default `fixtures/work_context_identity`). Every packet binds the
//! repository, the portfolio-snapshot identity, the basis head and the full
//! selected-work identity it compiles context for; a packet for another
//! repository, portfolio snapshot, issue/action, basis or exact head fails
//! stale/wrong-input with the exact recompile/reconcile route recorded on
//! the violation. Human Markdown and JSON projections derive from one check
//! DTO and can never strengthen each other.
//!
//! Selection law: this module selects no work, claims nothing, authorizes
//! nothing and mutates nothing. It reuses the RIPR-SPEC-0244 identity law
//! engine instead of forking one: each packet embeds the
//! `SelectedWorkIdentityV1` it compiles context for, and every law that
//! engine already enforces for a packet is enforced for the embedded
//! selection before the packet-level laws run. Campaign references stay
//! context, never permission: a packet may select an issue with no campaign
//! placement at all, and every recorded context/overflow reference must stay
//! relevant to the bound selection — raw portfolio, issue or log dumps never
//! travel inside a packet. Legacy `legacy_active_goal_ref` /
//! `legacy_current_work_item_ref` fields on a packet are accepted only as
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
    corpus_root, issue_identity, resolve_scenario_captured, selection_wire_name,
};

pub(crate) const WORK_CONTEXT_IDENTITY_CORPUS_SCHEMA_VERSION: &str =
    "work_context_identity_corpus.v1";
pub(crate) const WORK_CONTEXT_CHECK_VIEW_SCHEMA_VERSION: &str = "work_context_check_view.v1";
pub(crate) const WORK_CONTEXT_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_context_identity_provenance.v1";

pub(crate) const DEFAULT_WORK_CONTEXT_CORPUS_DIR: &str = "fixtures/work_context_identity";

pub(crate) const WORK_CONTEXT_CHECK_CLAIM_BOUNDARY: &str = "Read-only context-packet check: \
 the check validates committed packet rows against one compiled portfolio basis and the \
 selected-work identity each packet binds; it selects no work, claims nothing, authorizes no \
 writer, grants no readiness, merge or closeout authority, mutates no GitHub state, branch, \
 worktree, claim, spec, campaign or source, and it never fabricates identity from missing \
 evidence.";

// ---------------------------------------------------------------------------
// Packet DTOs (#1632): context packets bind the basis and the selection, and
// every reference they carry stays relevant to that selection.
// ---------------------------------------------------------------------------

/// Closed delegation-scope vocabulary for the #1632 context packet. A
/// packet is always bounded: `SelectedWorkOnly` carries no overflow at all,
/// and `SelectedWorkPlusRelevantOverflow` may carry only references already
/// vetted as relevant. There is deliberately no repository-wide scope.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkContextScopeV1 {
    SelectedWorkOnly,
    SelectedWorkPlusRelevantOverflow,
}

impl WorkContextScopeV1 {
    /// Every scope in canonical declaration order, so projections keep a
    /// stable row per scope even when a corpus uses only some.
    pub(crate) fn all() -> [Self; 2] {
        [
            Self::SelectedWorkOnly,
            Self::SelectedWorkPlusRelevantOverflow,
        ]
    }
}

/// One context/work-item packet (#1632). It binds the repository, the
/// compiled portfolio-snapshot identity, the basis head and the full
/// selected-work identity it compiles context for. It must not contain a
/// repository-wide current/default campaign, a mutable agent assignment, CI
/// wait state, or a progress percentage; the embedded selection schema
/// already rejects those, `deny_unknown_fields` rejects additions here, and
/// no field on a packet grants write, readiness, merge or closeout
/// authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkContextPacketV1 {
    pub packet_id: String,
    pub repository: String,
    /// The compiled portfolio snapshot identity the packet binds; a packet
    /// naming another snapshot is stale and routes recompilation.
    pub portfolio_snapshot: String,
    /// The basis (default-branch head) the packet was compiled against.
    pub basis_sha: String,
    /// The full selected-work identity this packet compiles context for.
    pub selection: SelectedWorkIdentityV1,
    /// The bounded delegation scope; a selected-work-only packet must not
    /// carry overflow references.
    pub scope: WorkContextScopeV1,
    /// Relevant campaign/spec/context references. Every entry must stay
    /// relevant to the bound selection: a campaign reference's id or source,
    /// an accepted requirement, a spec ref, an implementation slice, or an
    /// overlap issue/PR identity. Raw portfolio, issue or log dumps never
    /// travel inside a packet.
    pub context_refs: Vec<String>,
    /// Relevant overflow references for the widened scope; each entry must
    /// satisfy the same relevance rule as `context_refs`.
    pub overflow_refs: Vec<String>,
    /// Retained artifact references the packet was compiled from. A packet
    /// compiled from nothing is a claim, not a packet.
    pub compiled_from: Vec<String>,
    /// Legacy compatibility references (read-only; see module docs).
    pub legacy_active_goal_ref: Option<String>,
    pub legacy_current_work_item_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// Corpus DTOs.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkContextCaseV1 {
    pub packet: WorkContextPacketV1,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkContextScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkContextCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkContextIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkContextScenarioV1>,
}

pub(crate) fn load_work_context_corpus(body: &str) -> Result<WorkContextIdentityCorpusV1, String> {
    let corpus: WorkContextIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work context identity corpus: {error}"))?;
    if corpus.schema_version != WORK_CONTEXT_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work context identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work context identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    if corpus.scenarios.is_empty() {
        return Err("work context identity corpus must carry at least one scenario".to_string());
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work context identity corpus has an empty or duplicate scenario id `{}`",
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
                "scenario `{}` must carry at least one packet case",
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
            if case.packet.packet_id.trim().is_empty() || case.packet.basis_sha.trim().is_empty() {
                return Err(format!(
                    "scenario `{}` case {index} packet must record packet_id and basis_sha",
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
pub(crate) struct WorkContextCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub scope: WorkContextScopeV1,
    pub packet_id: String,
    pub selection_key: Option<String>,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub packet_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkContextScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub cases: Vec<WorkContextCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkContextCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkContextCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkContextCheckCountsV1,
    pub results: Vec<WorkContextScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Packet law engine. Pure and read-only; reuses the RIPR-SPEC-0244 engine.
// ---------------------------------------------------------------------------

fn packet_violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// One recorded reference stays relevant to the bound selection when it
/// names a campaign reference's id or source, an accepted requirement, a
/// spec ref, an implementation slice, or an overlap issue/PR/claim/worktree
/// identity. Everything else is a raw dump sneaking into the packet.
fn context_ref_relevant(reference: &str, packet: &WorkContextPacketV1) -> bool {
    let selection = &packet.selection;
    selection
        .campaign_refs
        .iter()
        .any(|campaign_ref| campaign_ref.id == reference || campaign_ref.source == reference)
        || selection
            .accepted_requirements
            .iter()
            .any(|req| req == reference)
        || selection
            .spec_refs
            .iter()
            .any(|spec_ref| spec_ref == reference)
        || selection
            .implementation_slices
            .iter()
            .any(|slice| slice == reference)
        || selection
            .overlaps
            .issues
            .iter()
            .any(|number| issue_identity(*number) == reference)
        || selection
            .overlaps
            .pull_requests
            .iter()
            .any(|number| format!("pr:{number}") == reference)
        || selection
            .overlaps
            .claims
            .iter()
            .any(|claim| claim == reference)
        || selection
            .overlaps
            .worktrees
            .iter()
            .any(|worktree| worktree == reference)
}

/// Run every identity law for one packet row against the basis, the compiled
/// snapshot and the captured sources. The embedded selection must satisfy
/// every RIPR-SPEC-0244 law first; the packet-level laws then bind the
/// repository, the portfolio-snapshot identity, the basis head, the compiled
/// artifacts, the bounded scope and the relevance of every carried context
/// or overflow reference. Legacy references resolve read-only and emit
/// migration posture. Each wrong identity is reported once with its exact
/// recompile/reconcile route.
fn check_context_packet(
    packet: &WorkContextPacketV1,
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> CaseEvaluation {
    // #1632: the selected-work identity is required in every packet. Reuse
    // the shared law engine so a packet cannot smuggle in a selection the
    // selection check would reject.
    let mut evaluation = check_packet(&packet.selection, basis, snapshot, captured);

    // Repository identity: the packet must name the basis repository.
    if packet.repository != basis.repository.repository {
        evaluation.violations.push(packet_violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "packet names repository `{}` but the portfolio basis is `{}`; a packet for \
                 another repository is wrong input — recompile the basis for the intended \
                 repository with `cargo xtask work portfolio`",
                packet.repository, basis.repository.repository
            ),
        ));
    }

    // Portfolio-snapshot identity: the packet must bind this compiled
    // snapshot; a packet for another snapshot is stale.
    if packet.portfolio_snapshot != basis.snapshot_identity {
        evaluation.violations.push(packet_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "packet binds portfolio snapshot `{}` but the compiled basis is `{}`; a packet \
                 for another portfolio snapshot is stale — recompile the basis and rebind the \
                 packet",
                packet.portfolio_snapshot, basis.snapshot_identity
            ),
        ));
    }

    // Basis identity: the recorded basis head must be the captured
    // default-branch head.
    if packet.basis_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(packet_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and rebind the packet",
                packet.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Compiled-artifact identity: a packet compiled from nothing is a claim,
    // not a packet.
    if packet.compiled_from.is_empty() {
        evaluation.violations.push(packet_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "packet records no compiled_from artifact reference; bind the retained packets, \
             receipts or run rows the context was compiled from"
                .to_string(),
        ));
    }

    // Bounded scope: a selected-work-only packet must not carry overflow.
    if packet.scope == WorkContextScopeV1::SelectedWorkOnly && !packet.overflow_refs.is_empty() {
        evaluation.violations.push(packet_violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            format!(
                "a selected-work-only packet carries {} overflow reference(s); bound the packet \
                 or widen the scope explicitly — raw portfolio, issue or log dumps never travel \
                 inside a packet",
                packet.overflow_refs.len()
            ),
        ));
    }

    // Reference relevance: every carried context or overflow reference must
    // stay relevant to the bound selection; campaign references are context,
    // never permission.
    for reference in packet
        .context_refs
        .iter()
        .chain(packet.overflow_refs.iter())
    {
        if !context_ref_relevant(reference, packet) {
            evaluation.violations.push(packet_violation(
                WorkSelectionLawV1::CampaignRefIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "reference `{reference}` is not relevant to the bound selection; packets \
                     carry only campaign/spec/context references that stay source-linked to \
                     the selected work"
                ),
            ));
        }
    }

    // Legacy compatibility references at the packet level: resolve read-only
    // with migration posture, exactly as the selection law does; grant no
    // authority.
    if let Some(legacy_ref) = &packet.legacy_active_goal_ref {
        evaluation.migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical campaign \
             reference; it grants no write, readiness, merge or closeout authority"
        ));
        let resolved = packet.selection.campaign_refs.iter().any(|campaign_ref| {
            campaign_ref.id == *legacy_ref
                && campaign_ref.relation == RelevantCampaignRelationV1::Historical
        });
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !resolved || !known {
            evaluation.violations.push(packet_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign \
                     carried as a historical reference by the bound selection"
                ),
            ));
        }
    }
    if let Some(legacy_ref) = &packet.legacy_current_work_item_ref {
        evaluation.migration_posture.push(format!(
            "legacy_current_work_item_ref `{legacy_ref}` accepted as a read-only historical \
             work-item reference; it grants no write, readiness, merge or closeout authority"
        ));
        let matches_subject = packet.selection.issue.as_ref().is_some_and(|issue| {
            *legacy_ref == crate::work_selection_identity::issue_identity(issue.number)
        });
        if !matches_subject {
            evaluation.violations.push(packet_violation(
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

fn run_context_scenario(
    scenario: &WorkContextScenarioV1,
    corpus: &WorkContextIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkContextScenarioResultV1, String> {
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
        let evaluation = check_context_packet(&case.packet, &basis, &snapshot, &captured);
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
        let packet_identity = if evaluation.violations.is_empty() {
            Some(portable_identity("work context packet", &case.packet)?)
        } else {
            None
        };
        let selection_key = case.packet.selection.issue.as_ref().map(|issue| {
            crate::work_selection_identity::selection_identity(
                case.packet.selection.lifecycle_action,
                issue.number,
            )
        });
        results.push(WorkContextCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            scope: case.packet.scope,
            packet_id: case.packet.packet_id.clone(),
            selection_key,
            violations: evaluation.violations,
            migration_posture: evaluation.migration_posture,
            packet_identity,
        });
    }
    Ok(WorkContextScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        cases: results,
    })
}

pub(crate) fn run_work_context_check(
    corpus_dir: &str,
    corpus: &WorkContextIdentityCorpusV1,
) -> Result<WorkContextCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_context_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkContextCheckViewV1 {
        schema_version: WORK_CONTEXT_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkContextCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work context check".to_string(),
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_CONTEXT_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work context check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_context_check_json(view: &WorkContextCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

pub(crate) fn work_context_check_markdown(view: &WorkContextCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work context-packet check\n\n");
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
    let mut scope_counts: std::collections::BTreeMap<WorkContextScopeV1, u64> =
        std::collections::BTreeMap::new();
    for case in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
    {
        *scope_counts.entry(case.scope).or_default() += 1;
    }
    out.push_str("Delegation scopes:\n\n");
    for scope in WorkContextScopeV1::all() {
        let count = scope_counts.get(&scope).copied().unwrap_or(0);
        out.push_str(&format!("- `{}`: {count}\n", selection_wire_name(&scope)));
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

/// `cargo xtask work context check [--corpus <dir>] [--json]` (#1706 PR B,
/// RIPR-SPEC-0272, consuming #1632): validate the committed context-packet
/// corpus against the compiled portfolio bases and render JSON or the
/// derived Markdown. Read-only: no GitHub, branch, worktree, claim, spec,
/// campaign or source state is touched.
pub(crate) fn work_context_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work context check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_CONTEXT_CORPUS_DIR.to_string();
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
    let corpus = load_work_context_corpus(&body)?;
    let view = run_work_context_check(&corpus_dir, &corpus)?;
    let json_body = work_context_check_json(&view)?;
    crate::write_report("work-context-check.json", &json_body)?;
    crate::write_report("work-context-check.md", &work_context_check_markdown(&view))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_context_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work context check: {} of {} cases failed; see work-context-check.md",
            view.counts.failed, view.counts.cases
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

/// Load the committed context-corpus provenance fail-closed.
pub(crate) fn load_work_context_provenance(
    body: &str,
) -> Result<crate::work_selection_identity::WorkSelectionProvenanceV1, String> {
    let provenance: crate::work_selection_identity::WorkSelectionProvenanceV1 =
        serde_json::from_str(body)
            .map_err(|error| format!("parse work context identity provenance: {error}"))?;
    if provenance.schema_version != WORK_CONTEXT_IDENTITY_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work context identity provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "work context identity provenance must record repository and capture_method"
                .to_string(),
        );
    }
    if provenance.captured_at.trim().is_empty() {
        return Err("work context identity provenance must record captured_at".to_string());
    }
    let mut paths = BTreeSet::new();
    for file in &provenance.files {
        reject_unsafe_context_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(format!(
                "work context identity provenance duplicates path `{}`",
                file.path
            ));
        }
    }
    Ok(provenance)
}

/// Fail closed on provenance paths that could escape the corpus root, with
/// the same cross-platform rules as the RIPR-SPEC-0244 provenance loader.
fn reject_unsafe_context_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work context identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work context identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The committed context-corpus scenario ids, pinned in declaration order.
pub(crate) const WORK_CONTEXT_SCENARIO_IDS: [&str; 6] = [
    "context-01-one-campaign-packet-passes",
    "context-02-several-related-campaigns-packet-passes",
    "context-03-standalone-no-campaign-packet-passes",
    "context-04-pr-repair-packet-passes",
    "context-05-narrow-single-agent-packet-passes",
    "context-06-legacy-and-wrong-context-refs-fail",
];

/// Validate the committed context-packet corpus fail-closed: provenance
/// digests in both directions, the six required scenario ids, and every
/// scenario's pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_context_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_CONTEXT_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_context_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_work_context_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/work_context_identity: {error}"));
            return;
        }
    };
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/work_context_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_context_identity: {} digest drifted: recorded `{}`, recomputed \
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
                        "fixtures/work_context_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_context_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_context_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_context_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_context_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_CONTEXT_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_context_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_CONTEXT_SCENARIO_IDS
        ));
    }
    // The committed corpus must never carry chat or singleton goal-file
    // authority bytes.
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_context_identity: corpus.json references `{forbidden}`; resume \
                 authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_context_check(DEFAULT_WORK_CONTEXT_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_context_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_context_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
