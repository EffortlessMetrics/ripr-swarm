//! Execution-wave planning on the selected-work basis (#1706, delivery
//! slice PR B, RIPR-SPEC-0271, consuming #1634).
//!
//! `cargo xtask work wave check [--corpus <dir>] [--json]` validates
//! committed `WorkWavePlanV1` rows against the same `PortfolioBasisV1` the
//! RIPR-SPEC-0244 selection check compiles from immutable captured inputs
//! (default `fixtures/work_wave_identity`). A plan consumes the complete
//! selected candidate set and capacity/conflict facts and partitions it
//! into waves over explicit resources. There is no generic global
//! `active-goal` resource: the retired spelling fails closed, and every
//! resource is one of the explicit forms `campaign-record:<id>`,
//! `portfolio-schema`, `spec-ledger`, `output-contract:<id>`, `issue:<n>`,
//! `pr:<n>` or `worktree:<path>`. Independent waves across campaigns are
//! permitted — and admitted — when their exclusive resources and the
//! declared writer capacity are disjoint. Human Markdown and JSON
//! projections derive from one check DTO and can never strengthen each
//! other.
//!
//! Selection law: this module selects no work, claims nothing, authorizes
//! nothing and mutates nothing. It reuses the RIPR-SPEC-0244 identity law
//! engine instead of forking one: every selection a plan embeds must
//! satisfy every law that engine enforces before the wave-level laws run.
//! Legacy `legacy_active_goal_ref` on a plan is accepted only as explicit
//! schema compatibility: it resolves read-only to a historical campaign
//! reference, emits migration posture, and grants no write, readiness,
//! merge or closeout authority.

use std::collections::{BTreeMap, BTreeSet};
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

pub(crate) const WORK_WAVE_IDENTITY_CORPUS_SCHEMA_VERSION: &str = "work_wave_identity_corpus.v1";
pub(crate) const WORK_WAVE_CHECK_VIEW_SCHEMA_VERSION: &str = "work_wave_check_view.v1";
pub(crate) const WORK_WAVE_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_wave_identity_provenance.v1";

pub(crate) const DEFAULT_WORK_WAVE_CORPUS_DIR: &str = "fixtures/work_wave_identity";

pub(crate) const WORK_WAVE_CHECK_CLAIM_BOUNDARY: &str = "Read-only wave-planning identity \
 receipt: the check validates committed wave plans against one compiled portfolio basis and the \
 selected-work identities they consume; it selects no work, claims nothing, authorizes no writer, \
 grants no readiness, merge or closeout authority, admits no wave silently, mutates no GitHub \
 state, branch, worktree, claim, spec, campaign or source, and it never fabricates identity from \
 missing evidence.";

// ---------------------------------------------------------------------------
// Wave plan DTOs (#1634): one plan consumes the complete selected candidate
// set and capacity/conflict facts and partitions it into explicit-resource
// waves. No generic global active-goal resource exists.
// ---------------------------------------------------------------------------

/// Declared writer capacity for one plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWaveCapacityV1 {
    /// Maximum selections one wave may run concurrently.
    pub concurrent_writers: u64,
}

/// One execution wave: a bounded set of selected candidates over explicit
/// resources.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWaveV1 {
    pub wave_id: String,
    /// Selection ids this wave runs; each must be one of the plan's
    /// selected candidates and must appear in exactly one wave.
    pub selections: Vec<String>,
    /// The explicit resources this wave occupies. `portfolio-schema` and
    /// `spec-ledger` are read-only shared references; every other resource
    /// is exclusive to one wave at a time.
    pub resources: Vec<String>,
}

/// One wave plan. It binds the repository, the compiled portfolio-snapshot
/// identity, the basis head and the complete selected candidate set. It
/// must not contain a repository-wide current/default campaign or a
/// generic active-goal resource; `deny_unknown_fields` rejects additions
/// and the resource vocabulary law rejects the retired singleton spelling.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWavePlanV1 {
    pub plan_id: String,
    pub repository: String,
    /// The compiled portfolio snapshot identity the plan binds.
    pub portfolio_snapshot: String,
    /// The basis (default-branch head) the plan was compiled against.
    pub basis_sha: String,
    /// The complete selected candidate set the plan consumes.
    pub selections: Vec<SelectedWorkIdentityV1>,
    pub capacity: WorkWaveCapacityV1,
    pub waves: Vec<WorkWaveV1>,
    /// Legacy compatibility reference (read-only; see module docs).
    pub legacy_active_goal_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// Resource vocabulary. The generic active-goal resource is retired; these
// are the only explicit resource forms a wave may name.
// ---------------------------------------------------------------------------

/// Resource kinds and their exclusivity. `PortfolioSchema` and `SpecLedger`
/// are read-only shared references; everything else is exclusive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WaveResourceKind {
    CampaignRecord,
    PortfolioSchema,
    SpecLedger,
    OutputContract,
    Issue,
    Pr,
    Worktree,
    RetiredActiveGoal,
    Unknown,
}

fn classify_resource(resource: &str) -> WaveResourceKind {
    if matches!(resource, "active-goal" | "active_goal" | "active.goal") {
        return WaveResourceKind::RetiredActiveGoal;
    }
    if resource == "portfolio-schema" {
        return WaveResourceKind::PortfolioSchema;
    }
    if resource == "spec-ledger" {
        return WaveResourceKind::SpecLedger;
    }
    if let Some(id) = resource.strip_prefix("campaign-record:") {
        return if id.trim().is_empty() {
            WaveResourceKind::Unknown
        } else {
            WaveResourceKind::CampaignRecord
        };
    }
    if let Some(id) = resource.strip_prefix("output-contract:") {
        return if id.trim().is_empty() {
            WaveResourceKind::Unknown
        } else {
            WaveResourceKind::OutputContract
        };
    }
    if let Some(id) = resource.strip_prefix("issue:") {
        return if id.parse::<u64>().is_err() {
            WaveResourceKind::Unknown
        } else {
            WaveResourceKind::Issue
        };
    }
    if let Some(id) = resource.strip_prefix("pr:") {
        return if id.parse::<u64>().is_err() {
            WaveResourceKind::Unknown
        } else {
            WaveResourceKind::Pr
        };
    }
    if let Some(path) = resource.strip_prefix("worktree:") {
        return if path.trim().is_empty() {
            WaveResourceKind::Unknown
        } else {
            WaveResourceKind::Worktree
        };
    }
    WaveResourceKind::Unknown
}

/// Exclusive resources participate in wave-conflict laws; read-only shared
/// references do not serialize unrelated work.
fn is_exclusive(kind: WaveResourceKind) -> bool {
    matches!(
        kind,
        WaveResourceKind::CampaignRecord
            | WaveResourceKind::OutputContract
            | WaveResourceKind::Issue
            | WaveResourceKind::Pr
            | WaveResourceKind::Worktree
    )
}

// ---------------------------------------------------------------------------
// Corpus DTOs.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWaveCaseV1 {
    pub plan: WorkWavePlanV1,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWaveScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkWaveCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWaveIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkWaveScenarioV1>,
}

pub(crate) fn load_work_wave_corpus(body: &str) -> Result<WorkWaveIdentityCorpusV1, String> {
    let corpus: WorkWaveIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work wave identity corpus: {error}"))?;
    if corpus.schema_version != WORK_WAVE_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work wave identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work wave identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    if corpus.scenarios.is_empty() {
        return Err("work wave identity corpus must carry at least one scenario".to_string());
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work wave identity corpus has an empty or duplicate scenario id `{}`",
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
                "scenario `{}` must carry at least one wave plan case",
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
            if case.plan.plan_id.trim().is_empty() || case.plan.basis_sha.trim().is_empty() {
                return Err(format!(
                    "scenario `{}` case {index} plan must record plan_id and basis_sha",
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
pub(crate) struct WorkWaveCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub plan_state: String,
    pub plan_id: String,
    pub waves: u64,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub plan_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWaveScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub cases: Vec<WorkWaveCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWaveCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkWaveCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkWaveCheckCountsV1,
    pub results: Vec<WorkWaveScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Wave law engine. Pure and read-only; reuses the RIPR-SPEC-0244 engine.
// ---------------------------------------------------------------------------

fn wave_violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// Run every identity law for one wave plan against the basis, the compiled
/// snapshot and the captured sources. Every embedded selection must satisfy
/// every RIPR-SPEC-0244 law first; the plan-level laws then bind the
/// repository, snapshot identity and basis head; the wave-level laws bind
/// the resource vocabulary (the retired generic `active-goal` resource
/// fails closed), the wave membership against the complete selected
/// candidate set (every candidate in exactly one wave), per-wave capacity,
/// captured worktree spellings and disjointness of exclusive resources
/// across concurrent waves. Each wrong identity is reported once with its
/// exact route.
fn check_wave_plan(
    plan: &WorkWavePlanV1,
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> CaseEvaluation {
    // #1634: the complete selected candidate set is consumed from the
    // shared identity contract; every embedded selection must pass the
    // shared law engine before any wave-level law runs.
    let mut evaluation = CaseEvaluation {
        violations: Vec::new(),
        migration_posture: Vec::new(),
    };
    let mut selection_ids: BTreeSet<String> = BTreeSet::new();
    for selection in &plan.selections {
        let mut embedded = check_packet(selection, basis, snapshot, captured);
        evaluation.violations.append(&mut embedded.violations);
        evaluation
            .migration_posture
            .append(&mut embedded.migration_posture);
        if !selection_ids.insert(selection.selection_id.clone()) {
            evaluation.violations.push(wave_violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "the plan's selected candidate set lists selection id `{}` twice; the \
                     candidate set is a set, not a queue",
                    selection.selection_id
                ),
            ));
        }
    }

    // Repository identity.
    if plan.repository != basis.repository.repository {
        evaluation.violations.push(wave_violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "plan names repository `{}` but the portfolio basis is `{}`; a plan for \
                 another repository is wrong input — recompile the basis for the intended \
                 repository with `cargo xtask work portfolio`",
                plan.repository, basis.repository.repository
            ),
        ));
    }

    // Portfolio-snapshot identity: a plan naming another snapshot is stale.
    if plan.portfolio_snapshot != basis.snapshot_identity {
        evaluation.violations.push(wave_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "plan binds portfolio snapshot `{}` but the compiled basis is `{}`; a plan \
                 for another portfolio snapshot is stale — recompile the basis and rebind the \
                 plan",
                plan.portfolio_snapshot, basis.snapshot_identity
            ),
        ));
    }

    // Basis identity.
    if plan.basis_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(wave_violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and rebind the plan",
                plan.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Wave membership: every wave selection must be a selected candidate.
    for wave in &plan.waves {
        for referenced in &wave.selections {
            if !selection_ids.contains(referenced) {
                evaluation.violations.push(wave_violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "wave `{}` names selection `{}` that is not in the plan's selected \
                         candidate set; waves partition the declared candidates, never work \
                         the plan did not select",
                        wave.wave_id, referenced
                    ),
                ));
            }
        }
    }

    // Coverage: every selected candidate appears in exactly one wave — the
    // plan consumes the complete selected candidate set.
    let mut assignment: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for wave in &plan.waves {
        for referenced in &wave.selections {
            assignment
                .entry(referenced.clone())
                .or_default()
                .push(wave.wave_id.as_str());
        }
    }
    for selection_id in &selection_ids {
        match assignment.get(selection_id) {
            None => {
                evaluation.violations.push(wave_violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "selected candidate `{selection_id}` is not assigned to any wave; \
                         assign it or record why the plan does not run it"
                    ),
                ));
            }
            Some(waves) if waves.len() > 1 => {
                evaluation.violations.push(wave_violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "selected candidate `{selection_id}` is assigned to waves {waves:?}; \
                         a candidate runs in exactly one wave"
                    ),
                ));
            }
            Some(_) => {}
        }
    }

    // Capacity: one wave cannot exceed the declared writer capacity.
    for wave in &plan.waves {
        if wave.selections.len() as u64 > plan.capacity.concurrent_writers {
            evaluation.violations.push(wave_violation(
                WorkSelectionLawV1::WorktreeIdentity,
                WorkSelectionRouteV1::ReconcileResources,
                format!(
                    "wave `{}` schedules {} selections against a writer capacity of {}; \
                     split the wave or raise the declared capacity",
                    wave.wave_id,
                    wave.selections.len(),
                    plan.capacity.concurrent_writers
                ),
            ));
        }
    }

    // Resource vocabulary: the retired generic active-goal resource fails
    // closed, and every resource is one of the explicit forms.
    for wave in &plan.waves {
        for resource in &wave.resources {
            match classify_resource(resource) {
                WaveResourceKind::RetiredActiveGoal => {
                    evaluation.violations.push(wave_violation(
                        WorkSelectionLawV1::CampaignRefIdentity,
                        WorkSelectionRouteV1::ReconcileResources,
                        format!(
                            "wave `{}` names the retired generic `active-goal` resource; \
                             planning no longer serializes work through one global goal — name \
                             explicit resources such as `campaign-record:<id>`, \
                             `portfolio-schema`, `spec-ledger`, `output-contract:<id>`, \
                             `issue:<n>`, `pr:<n>` or `worktree:<path>`",
                            wave.wave_id
                        ),
                    ));
                }
                WaveResourceKind::Unknown => {
                    evaluation.violations.push(wave_violation(
                        WorkSelectionLawV1::CampaignRefIdentity,
                        WorkSelectionRouteV1::ReconcileResources,
                        format!(
                            "wave `{}` names resource `{resource}` that is not an explicit \
                             resource form (`campaign-record:<id>`, `portfolio-schema`, \
                             `spec-ledger`, `output-contract:<id>`, `issue:<n>`, `pr:<n>` or \
                             `worktree:<path>`)",
                            wave.wave_id
                        ),
                    ));
                }
                WaveResourceKind::CampaignRecord => {
                    let id = resource
                        .strip_prefix("campaign-record:")
                        .unwrap_or_default();
                    if !snapshot.campaigns.iter().any(|campaign| campaign.id == id) {
                        evaluation.violations.push(wave_violation(
                            WorkSelectionLawV1::CampaignRefIdentity,
                            WorkSelectionRouteV1::ReconcileSelection,
                            format!(
                                "resource `{resource}` names a campaign the portfolio does not \
                                 record; campaign resources stay source-linked"
                            ),
                        ));
                    }
                }
                WaveResourceKind::Worktree => {
                    let path = resource.strip_prefix("worktree:").unwrap_or_default();
                    if !captured_worktrees(captured).contains(path) {
                        evaluation.violations.push(wave_violation(
                            WorkSelectionLawV1::WorktreeIdentity,
                            WorkSelectionRouteV1::ReconcileResources,
                            format!(
                                "resource `{resource}` names a worktree the captured local \
                                 state does not record; reconcile the writer resources"
                            ),
                        ));
                    }
                }
                WaveResourceKind::PortfolioSchema
                | WaveResourceKind::SpecLedger
                | WaveResourceKind::OutputContract
                | WaveResourceKind::Issue
                | WaveResourceKind::Pr => {}
            }
        }
    }

    // Disjointness: exclusive resources cannot appear in two waves at
    // once; read-only `portfolio-schema`/`spec-ledger` references are
    // shareable and never serialize unrelated work. Independent waves
    // across campaigns are admitted when exclusive resources and capacity
    // are disjoint.
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    for wave in &plan.waves {
        for resource in &wave.resources {
            if !is_exclusive(classify_resource(resource)) {
                continue;
            }
            if let Some(other_wave) = seen.insert(resource.as_str(), wave.wave_id.as_str())
                && other_wave != wave.wave_id
            {
                evaluation.violations.push(wave_violation(
                    WorkSelectionLawV1::OverlapVisibility,
                    WorkSelectionRouteV1::ReconcileResources,
                    format!(
                        "exclusive resource `{resource}` is scheduled in both wave \
                         `{other_wave}` and wave `{}`; reconcile the waves or serialize them \
                         — disjoint cross-campaign waves stay independent",
                        wave.wave_id
                    ),
                ));
            }
        }
    }

    // Legacy compatibility reference: resolve read-only with migration
    // posture; grant no authority.
    if let Some(legacy_ref) = &plan.legacy_active_goal_ref {
        evaluation.migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical campaign \
             reference; it grants no write, readiness, merge or closeout authority"
        ));
        let resolved = plan.selections.iter().any(|selection| {
            selection.campaign_refs.iter().any(|campaign_ref| {
                campaign_ref.id == *legacy_ref
                    && campaign_ref.relation == RelevantCampaignRelationV1::Historical
            })
        });
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !resolved || !known {
            evaluation.violations.push(wave_violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign \
                     carried as a historical reference by one of the plan's selections"
                ),
            ));
        }
    }

    evaluation
}

/// Captured local worktrees relativized exactly as the compiler renders
/// them (local_state.root when captured, else the manifest root).
fn captured_worktrees(captured: &WorkCapturedDirV1) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let root = captured.local_state.as_ref().map_or_else(
        || captured.manifest.root.as_str(),
        |state| state.root.as_str(),
    );
    if let Some(body) = &captured.local_state {
        for worktree in &body.worktrees {
            out.insert(crate::work_portfolio::portable_path(
                &worktree.path,
                &body.root,
            ));
        }
    }
    if let Some(body) = &captured.claims {
        for claim in &body.claims {
            if let Some(worktree) = &claim.worktree {
                out.insert(crate::work_portfolio::portable_path(worktree, root));
            }
        }
    }
    if let Some(body) = &captured.pull_requests {
        for pr in &body.pull_requests {
            if let Some(worktree) = &pr.worktree_path {
                out.insert(crate::work_portfolio::portable_path(worktree, root));
            }
        }
    }
    out
}

/// Plan state recorded on the projection: admitted plans partition the
/// selected candidate set over disjoint explicit resources; every
/// violation demands reconciliation, never silent admission.
fn plan_state(violations: &[WorkSelectionLawViolationV1]) -> &'static str {
    if violations.is_empty() {
        "admitted"
    } else {
        "reconcile_required"
    }
}

// ---------------------------------------------------------------------------
// Corpus runner and projections.
// ---------------------------------------------------------------------------

fn run_wave_scenario(
    scenario: &WorkWaveScenarioV1,
    corpus: &WorkWaveIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkWaveScenarioResultV1, String> {
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
        let evaluation = check_wave_plan(&case.plan, &basis, &snapshot, &captured);
        let passed = match case.expect {
            WorkSelectionExpectV1::Pass => evaluation.violations.is_empty(),
            WorkSelectionExpectV1::Fail => {
                // Exact match on the law/route pair multiset: a negative
                // case pins every occurrence, so an added spurious
                // violation cannot stay green.
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
        let plan_identity = if evaluation.violations.is_empty() {
            Some(portable_identity("work wave plan", &case.plan)?)
        } else {
            None
        };
        results.push(WorkWaveCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            plan_state: plan_state(&evaluation.violations).to_string(),
            plan_id: case.plan.plan_id.clone(),
            waves: case.plan.waves.len() as u64,
            violations: evaluation.violations,
            migration_posture: evaluation.migration_posture,
            plan_identity,
        });
    }
    Ok(WorkWaveScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        cases: results,
    })
}

pub(crate) fn run_work_wave_check(
    corpus_dir: &str,
    corpus: &WorkWaveIdentityCorpusV1,
) -> Result<WorkWaveCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_wave_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkWaveCheckViewV1 {
        schema_version: WORK_WAVE_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkWaveCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work wave check".to_string(),
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_WAVE_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work wave check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_wave_check_json(view: &WorkWaveCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

pub(crate) fn work_wave_check_markdown(view: &WorkWaveCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work wave-planning check\n\n");
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
                "## {} case {} — `{}` ({})\n\n",
                scenario.scenario_id, case.case_index, case.plan_id, case.plan_state
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
                out.push_str("- no violations; plan is admitted\n");
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

/// `cargo xtask work wave check [--corpus <dir>] [--json]` (#1706 PR B,
/// RIPR-SPEC-0271, consuming #1634): validate the committed wave-planning
/// corpus against the compiled portfolio bases and render JSON or the
/// derived Markdown. Read-only: no GitHub, branch, worktree, claim, spec,
/// campaign or source state is touched.
pub(crate) fn work_wave_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work wave check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_WAVE_CORPUS_DIR.to_string();
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
    let corpus = load_work_wave_corpus(&body)?;
    let view = run_work_wave_check(&corpus_dir, &corpus)?;
    let json_body = work_wave_check_json(&view)?;
    crate::write_report("work-wave-check.json", &json_body)?;
    crate::write_report("work-wave-check.md", &work_wave_check_markdown(&view))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_wave_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work wave check: {} of {} cases failed; see work-wave-check.md",
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
fn reject_unsafe_wave_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work wave identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work wave identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The committed wave-corpus scenario ids, pinned in declaration order.
pub(crate) const WORK_WAVE_SCENARIO_IDS: [&str; 6] = [
    "wave-01-disjoint-cross-campaign-waves-pass",
    "wave-02-single-agent-wave-passes-despite-active-campaigns",
    "wave-03-overlapping-exclusive-resources-fail",
    "wave-04-retired-active-goal-resource-fails-closed",
    "wave-05-stale-basis-and-unassigned-candidate-fail",
    "wave-06-capacity-exceeded-fails",
];

/// Validate the committed wave-planning corpus fail-closed: provenance
/// digests in both directions, the six required scenario ids, and every
/// scenario's pinned laws. Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_wave_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_WAVE_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_wave_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance: crate::work_selection_identity::WorkSelectionProvenanceV1 =
        match serde_json::from_str(&provenance_body)
            .map_err(|error| format!("parse work wave identity provenance: {error}"))
            .and_then(
                |parsed: crate::work_selection_identity::WorkSelectionProvenanceV1| {
                    if parsed.schema_version != WORK_WAVE_IDENTITY_PROVENANCE_SCHEMA_VERSION {
                        return Err(format!(
                            "unsupported work wave identity provenance schema `{}`",
                            parsed.schema_version
                        ));
                    }
                    if parsed.repository.trim().is_empty()
                        || parsed.capture_method.trim().is_empty()
                    {
                        return Err(
                    "work wave identity provenance must record repository and capture_method"
                        .to_string(),
                );
                    }
                    if parsed.captured_at.trim().is_empty() {
                        return Err(
                            "work wave identity provenance must record captured_at".to_string()
                        );
                    }
                    Ok(parsed)
                },
            ) {
            Ok(provenance) => provenance,
            Err(error) => {
                violations.push(format!("fixtures/work_wave_identity: {error}"));
                return;
            }
        };
    let mut provenance_paths = BTreeSet::new();
    for file in &provenance.files {
        if let Err(error) = reject_unsafe_wave_path(&file.path) {
            violations.push(format!("fixtures/work_wave_identity: {error}"));
        }
        if !provenance_paths.insert(file.path.clone()) {
            violations.push(format!(
                "fixtures/work_wave_identity: provenance duplicates path `{}`",
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
                    "fixtures/work_wave_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_wave_identity: {} digest drifted: recorded `{}`, recomputed \
                 `work-wave:sha256:{}`",
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
                        "fixtures/work_wave_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_wave_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_wave_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_wave_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_wave_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_WAVE_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_wave_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_WAVE_SCENARIO_IDS
        ));
    }
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_wave_identity: corpus.json references `{forbidden}`; resume \
                 authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_wave_check(DEFAULT_WORK_WAVE_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_wave_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_wave_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
