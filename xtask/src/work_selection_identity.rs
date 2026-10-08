//! Shared selected-work identity schemas and legacy compatibility (#1706,
//! delivery slice PR A, RIPR-SPEC-0244).
//!
//! `cargo xtask work selection check [--corpus <dir>] [--json]` validates
//! committed `SelectedWorkIdentityV1` packets against a `PortfolioBasisV1`
//! compiled from immutable captured inputs (default
//! `fixtures/work_selection_identity`, scenario captured dirs under
//! `fixtures/`). Human Markdown and JSON projections derive from one check
//! DTO and can never strengthen each other.
//!
//! Selection law: this module selects no work, claims nothing, authorizes
//! nothing and mutates nothing. Campaign references are context, never
//! permission; a selection outside every campaign is representable. Legacy
//! `legacy_active_goal_ref` / `legacy_current_work_item_ref` fields are
//! accepted only as explicit schema compatibility: they resolve to
//! historical/relevant campaign references, emit migration posture, and
//! grant no write, readiness, merge or closeout authority.
//!
//! Identity law: every wrong repository, issue, action, basis, worktree or
//! head fails visibly with the exact recompile/reconcile route recorded on
//! the violation (RIPR-SPEC-0244 acceptance: stale/wrong identity states
//! return exact recompile/reconcile routes).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::work_portfolio::{
    WorkCandidateKindV1, WorkCapturedDirV1, WorkCapturedIssueV1, WorkConfidenceV1,
    WorkConflictEdgeKindV1, WorkIssueV1, WorkPortfolioSnapshotV1, WorkRepositoryIdentityV1,
    WorkSourceFreshnessV1, WorkSourceObservationV1, classify_captured_issue,
    compile_work_portfolio, load_work_captured_dir, portable_identity, portable_path,
    work_portfolio_json, workspace_path,
};

pub(crate) const WORK_SELECTION_IDENTITY_CORPUS_SCHEMA_VERSION: &str =
    "work_selection_identity_corpus.v1";
pub(crate) const WORK_SELECTION_IDENTITY_PROVENANCE_SCHEMA_VERSION: &str =
    "work_selection_identity_provenance.v1";
pub(crate) const WORK_SELECTION_CHECK_VIEW_SCHEMA_VERSION: &str = "work_selection_check_view.v1";

pub(crate) const DEFAULT_WORK_SELECTION_CORPUS_DIR: &str = "fixtures/work_selection_identity";

pub(crate) const WORK_SELECTION_CHECK_CLAIM_BOUNDARY: &str = "Read-only selected-work identity \
 receipt: the check validates committed selection packets against one compiled portfolio basis; \
 it selects no work, claims nothing, authorizes no writer, grants no readiness, merge or closeout \
 authority, mutates no GitHub state, branch, worktree, claim, spec, campaign or source, and it \
 never fabricates identity from missing evidence.";

// ---------------------------------------------------------------------------
// Shared identity DTOs (consumed by the PR B/C migrations, defined here once).
// ---------------------------------------------------------------------------

/// Closed plan-disposition vocabulary for the #1646 planning consumer that
/// PR C migrates. Frozen: PR C consumes these wire names; changing them is a
/// schema-version break, not a rename.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkPlanDispositionV1 {
    SingleScopedPr,
    MultiPrCampaign,
    AppendToNamedCampaign,
    StandaloneIssueWork,
    FocusedTrackerOnly,
    AlreadyPlanned,
    BlockedByContractOrDecision,
    RootPortfolioDecisionRequired,
}

impl WorkPlanDispositionV1 {
    /// Every disposition in canonical declaration order, so projections keep
    /// a stable row per disposition even when a corpus uses only some.
    pub(crate) fn all() -> [Self; 8] {
        [
            Self::SingleScopedPr,
            Self::MultiPrCampaign,
            Self::AppendToNamedCampaign,
            Self::StandaloneIssueWork,
            Self::FocusedTrackerOnly,
            Self::AlreadyPlanned,
            Self::BlockedByContractOrDecision,
            Self::RootPortfolioDecisionRequired,
        ]
    }
}

/// Normalized issue identity: the number plus its stable string form
/// (`issue:<number>`). Equality is whole-value; the string form is the
/// canonical wire identity and must be derived, not free-typed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectedIssueIdentityV1 {
    pub number: u64,
    pub identity: String,
}

/// Normalized work-item identity: the durable work-item id plus its stable
/// string form (`work-item:<id>`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectedWorkItemIdentityV1 {
    pub id: String,
    pub identity: String,
}

/// Relationship of one campaign reference to the selected work. `Member`
/// means the campaign records name the issue; `Relevant` and `Historical`
/// are context only. No relation grants execution authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RelevantCampaignRelationV1 {
    Member,
    Relevant,
    Historical,
}

/// One relevant campaign reference, zero or more per selection. Campaigns
/// stay discoverable and source-linked without granting authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RelevantCampaignRefV1 {
    pub id: String,
    pub relation: RelevantCampaignRelationV1,
    pub source: String,
}

/// The live overlap set: issues, PRs, claims, worktrees and semantic
/// resources that touch the selected work and must stay visible before any
/// mutation (RIPR-SPEC-0244 acceptance: existing PR/claim/worktree overlap
/// is visible before mutation).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LiveOverlapSetV1 {
    pub issues: Vec<u64>,
    pub pull_requests: Vec<u64>,
    pub claims: Vec<String>,
    pub worktrees: Vec<String>,
    pub resources: Vec<String>,
}

/// Role/context/budget profile. Fixed at selection time; there is
/// deliberately no mutable agent-assignment field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkRoleBudgetProfileV1 {
    pub role: String,
    pub context_profile: String,
    pub budget: String,
    pub single_agent: bool,
}

/// The shared selected-work identity. It binds exactly one root-selected
/// transition and its point-in-time basis. It must not contain a
/// repository-wide current/default campaign, a mutable agent assignment, CI
/// wait state, or a progress percentage; the schema has no such fields and
/// `deny_unknown_fields` rejects them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectedWorkIdentityV1 {
    pub selection_id: String,
    pub repository: String,
    pub candidate_id: String,
    pub issue: Option<SelectedIssueIdentityV1>,
    pub work_item: Option<SelectedWorkItemIdentityV1>,
    pub pull_request: Option<u64>,
    /// The selected lifecycle action. Reuses the closed #1704 candidate-kind
    /// vocabulary instead of forking a parallel action taxonomy.
    pub lifecycle_action: WorkCandidateKindV1,
    pub accepted_requirements: Vec<String>,
    pub spec_refs: Vec<String>,
    pub implementation_slices: Vec<String>,
    pub campaign_refs: Vec<RelevantCampaignRefV1>,
    /// The portfolio-basis default-branch head the selection was made
    /// against.
    pub basis_sha: String,
    /// The exact head the selection expects; when present it must match the
    /// basis head or the check routes a `reconcile_head` violation.
    pub expected_head: Option<String>,
    pub overlaps: LiveOverlapSetV1,
    pub role_context_budget: WorkRoleBudgetProfileV1,
    pub claim_boundary: String,
    pub stop_conditions: Vec<String>,
    /// Legacy compatibility references (read-only; see module docs).
    pub legacy_active_goal_ref: Option<String>,
    pub legacy_current_work_item_ref: Option<String>,
}

/// The point-in-time portfolio basis a selection binds against. Reuses the
/// #1704 compiler DTOs for repository identity, source observations and
/// completeness instead of forking duplicate types.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PortfolioBasisV1 {
    pub repository: WorkRepositoryIdentityV1,
    /// The compiled snapshot's portable identity digest.
    pub snapshot_identity: String,
    /// `Complete` only when every captured source is current.
    pub snapshot_completeness: WorkConfidenceV1,
    pub captured_sources: Vec<WorkSourceObservationV1>,
    /// Source names whose observation is missing or unavailable.
    pub unavailable_sources: Vec<String>,
    pub selection_policy: String,
    pub selection_policy_version: String,
}

/// Legacy compatibility packet. Accepted only through explicit schema
/// compatibility; resolves to historical/relevant campaign references and
/// emits migration posture. `attempted_authorities` must be empty: legacy
/// references grant no write, readiness, merge or closeout authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyWorkPacketV1 {
    pub legacy_active_goal_ref: Option<String>,
    pub legacy_current_work_item_ref: Option<String>,
    pub attempted_authorities: Vec<String>,
}

// ---------------------------------------------------------------------------
// Normalized identity string forms.
// ---------------------------------------------------------------------------

/// Stable identity string for one issue.
pub(crate) fn issue_identity(number: u64) -> String {
    format!("issue:{number}")
}

/// Stable identity string for one durable work item.
pub(crate) fn work_item_identity(id: &str) -> String {
    format!("work-item:{id}")
}

/// Stable identity string for one portfolio candidate.
pub(crate) fn candidate_identity(number: u64) -> String {
    format!("candidate:issue:{number}")
}

/// Stable identity string for one selection.
pub(crate) fn selection_identity(action: WorkCandidateKindV1, number: u64) -> String {
    format!("selection:{}:issue:{number}", action.wire_name())
}

/// Canonical spec-ref wire shape: `RIPR-SPEC-` plus exactly four digits, with
/// nothing or a `-<slug>` suffix. Resolution against the spec corpus is a
/// consumer concern; this pins only the identity shape the captured bytes can
/// honestly enforce.
fn spec_ref_has_canonical_shape(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("RIPR-SPEC-") else {
        return false;
    };
    let digits: usize = rest.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits != 4 {
        return false;
    }
    let mut remainder = rest.chars().skip(digits);
    match remainder.next() {
        None => true,
        Some('-') => {
            let slug: Vec<char> = remainder.collect();
            !slug.is_empty()
                && slug
                    .iter()
                    .all(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        }
        Some(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Identity law vocabulary: closed laws and exact reconcile routes.
// ---------------------------------------------------------------------------

/// Closed identity-law vocabulary. Every rejection names exactly one law.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkSelectionLawV1 {
    RepositoryIdentity,
    SubjectIdentity,
    ActionIdentity,
    BasisIdentity,
    HeadIdentity,
    WorktreeIdentity,
    CampaignRefIdentity,
    LegacyCompatibility,
    OverlapVisibility,
}

/// The exact recovery route a violation demands: recompile the basis,
/// reconcile the selection/resources/head, or reject legacy authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkSelectionRouteV1 {
    RecompileBasis,
    ReconcileSelection,
    ReconcileResources,
    ReconcileHead,
    RejectLegacyAuthority,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionLawViolationV1 {
    pub law: WorkSelectionLawV1,
    pub route: WorkSelectionRouteV1,
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Corpus DTOs.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkSelectionExpectV1 {
    Pass,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionExpectedViolationV1 {
    pub law: WorkSelectionLawV1,
    pub route: WorkSelectionRouteV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionCaseV1 {
    pub packet: Option<SelectedWorkIdentityV1>,
    pub legacy_packet: Option<LegacyWorkPacketV1>,
    pub expect: WorkSelectionExpectV1,
    pub expect_violations: Vec<WorkSelectionExpectedViolationV1>,
    /// The frozen plan disposition the PR C planning consumer will later
    /// emit for this case; pinned here at schema/fixture level.
    pub plan_disposition: WorkPlanDispositionV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionScenarioV1 {
    pub id: String,
    pub title: String,
    /// Captured portfolio directory, relative to `fixtures/`.
    pub captured: String,
    pub cases: Vec<WorkSelectionCaseV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionIdentityCorpusV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub scenarios: Vec<WorkSelectionScenarioV1>,
}

pub(crate) fn load_work_selection_corpus(
    body: &str,
) -> Result<WorkSelectionIdentityCorpusV1, String> {
    let corpus: WorkSelectionIdentityCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work selection identity corpus: {error}"))?;
    if corpus.schema_version != WORK_SELECTION_IDENTITY_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work selection identity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.repository.trim().is_empty()
        || corpus.selection_policy.trim().is_empty()
        || corpus.selection_policy_version.trim().is_empty()
    {
        return Err(
            "work selection identity corpus must record repository, selection_policy and selection_policy_version"
                .to_string(),
        );
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if scenario.id.trim().is_empty() || !ids.insert(scenario.id.clone()) {
            return Err(format!(
                "work selection identity corpus has an empty or duplicate scenario id `{}`",
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
                "scenario `{}` must carry at least one selection case",
                scenario.id
            ));
        }
        for (index, case) in scenario.cases.iter().enumerate() {
            if case.packet.is_none() && case.legacy_packet.is_none() {
                return Err(format!(
                    "scenario `{}` case {index} carries neither a packet nor a legacy packet",
                    scenario.id
                ));
            }
            if case.expect == WorkSelectionExpectV1::Fail && case.expect_violations.is_empty() {
                return Err(format!(
                    "scenario `{}` case {index} expects failure but pins no exact law/route pair",
                    scenario.id
                ));
            }
            if let Some(packet) = &case.packet
                && (packet.selection_id.trim().is_empty() || packet.basis_sha.trim().is_empty())
            {
                return Err(format!(
                    "scenario `{}` case {index} packet must record selection_id and basis_sha",
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
pub(crate) struct WorkSelectionCaseResultV1 {
    pub case_index: u64,
    pub status: String,
    pub expected: String,
    pub plan_disposition: WorkPlanDispositionV1,
    pub selection_key: Option<String>,
    pub violations: Vec<WorkSelectionLawViolationV1>,
    pub migration_posture: Vec<String>,
    pub selection_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionScenarioResultV1 {
    pub scenario_id: String,
    pub title: String,
    pub captured: String,
    pub portfolio_identity: String,
    pub basis_sha: String,
    pub snapshot_completeness: WorkConfidenceV1,
    /// `covered`, `acceptance_uncovered`, `in_flight`, `not_started` or
    /// `unknown`; a plan-context note only, never completion authority.
    pub acceptance_state: String,
    pub cases: Vec<WorkSelectionCaseResultV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionCheckCountsV1 {
    pub scenarios: u64,
    pub cases: u64,
    pub passed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionCheckViewV1 {
    pub schema_version: String,
    pub repository: String,
    pub selection_policy: String,
    pub selection_policy_version: String,
    pub corpus: String,
    pub counts: WorkSelectionCheckCountsV1,
    pub results: Vec<WorkSelectionScenarioResultV1>,
    pub retrieval_commands: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

// ---------------------------------------------------------------------------
// Basis construction and the identity law engine. Pure and read-only.
// ---------------------------------------------------------------------------

fn build_portfolio_basis(
    snapshot: &WorkPortfolioSnapshotV1,
    selection_policy: &str,
    selection_policy_version: &str,
) -> PortfolioBasisV1 {
    let complete = snapshot
        .source_observations
        .iter()
        .all(|observation| observation.freshness == WorkSourceFreshnessV1::Current);
    let unavailable_sources = snapshot
        .source_observations
        .iter()
        .filter(|observation| observation.freshness == WorkSourceFreshnessV1::Unknown)
        .map(|observation| observation.source.clone())
        .collect();
    PortfolioBasisV1 {
        repository: snapshot.repository.clone(),
        snapshot_identity: snapshot.portable_identity.clone(),
        snapshot_completeness: if complete {
            WorkConfidenceV1::Complete
        } else {
            WorkConfidenceV1::Partial
        },
        captured_sources: snapshot.source_observations.clone(),
        unavailable_sources,
        selection_policy: selection_policy.to_string(),
        selection_policy_version: selection_policy_version.to_string(),
    }
}

fn snapshot_issue(snapshot: &WorkPortfolioSnapshotV1, number: u64) -> Option<&WorkIssueV1> {
    snapshot.issues.iter().find(|issue| issue.number == number)
}

fn snapshot_candidate(
    snapshot: &WorkPortfolioSnapshotV1,
    number: u64,
) -> Option<&crate::work_portfolio::WorkCandidateV1> {
    snapshot
        .candidates
        .iter()
        .find(|candidate| candidate.issue == number)
}

fn captured_issue(captured: &WorkCapturedDirV1, number: u64) -> Option<&WorkCapturedIssueV1> {
    captured
        .issues
        .as_ref()
        .and_then(|body| body.issues.iter().find(|issue| issue.number == number))
}

/// Sibling issues in the same duplicate family as `number`, from the
/// compiled conflict edges. Edge subjects carry the `candidate:issue:<n>`
/// identity form, so membership parses from the edge itself; an issue in no
/// duplicate family yields an empty set, never invented relatives.
fn duplicate_family_issues(snapshot: &WorkPortfolioSnapshotV1, number: u64) -> Vec<u64> {
    let self_identity = candidate_identity(number);
    let mut siblings = BTreeSet::new();
    for edge in &snapshot.conflict_edges {
        if edge.kind != WorkConflictEdgeKindV1::DuplicateFamily
            || !edge
                .subjects
                .iter()
                .any(|subject| subject == &self_identity)
        {
            continue;
        }
        for subject in &edge.subjects {
            if let Some(rest) = subject.strip_prefix("candidate:issue:")
                && let Ok(sibling) = rest.parse::<u64>()
                && sibling != number
            {
                siblings.insert(sibling);
            }
        }
    }
    siblings.into_iter().collect()
}

/// Compute the live overlap set for one issue from the compiled snapshot.
fn live_overlaps(snapshot: &WorkPortfolioSnapshotV1, number: u64) -> LiveOverlapSetV1 {
    let mut overlaps = LiveOverlapSetV1 {
        issues: duplicate_family_issues(snapshot, number),
        pull_requests: Vec::new(),
        claims: Vec::new(),
        worktrees: Vec::new(),
        resources: Vec::new(),
    };
    for pr in &snapshot.pull_requests {
        if pr.state == "open" && pr.linked_issues.contains(&number) {
            overlaps.pull_requests.push(pr.number);
            if let Some(worktree) = &pr.worktree {
                overlaps.worktrees.push(worktree.clone());
            }
        }
    }
    for claim in &snapshot.claims {
        if claim.issue == Some(number) && claim.state == "active" {
            overlaps.claims.push(claim.id.clone());
            if let Some(worktree) = &claim.worktree {
                overlaps.worktrees.push(worktree.clone());
            }
        }
    }
    if let Some(candidate) = snapshot_candidate(snapshot, number) {
        overlaps.resources = candidate.conflict_resources.clone();
    }
    overlaps.worktrees.sort();
    overlaps.worktrees.dedup();
    overlaps
}

/// Compute the live overlap set for a standalone issue straight from the
/// captured sources, because the RIPR-SPEC-0234 snapshot only surfaces
/// campaign-member issues. Worktree spellings relativize exactly as the
/// compiler renders them. Duplicate-family siblings still come from the
/// compiled conflict edges: edges are built from the captured issue set, so
/// they cover standalone members the candidate list does not surface.
fn live_overlaps_captured(
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
    number: u64,
) -> LiveOverlapSetV1 {
    let mut overlaps = LiveOverlapSetV1 {
        issues: duplicate_family_issues(snapshot, number),
        pull_requests: Vec::new(),
        claims: Vec::new(),
        worktrees: Vec::new(),
        resources: Vec::new(),
    };
    // Worktree spellings must match the compiler's effective root
    // (local_state.root when captured, else manifest.root).
    let root = captured.local_state.as_ref().map_or_else(
        || captured.manifest.root.as_str(),
        |state| state.root.as_str(),
    );
    if let Some(body) = &captured.pull_requests {
        for pr in &body.pull_requests {
            if pr.state == "open" && pr.linked_issues.contains(&number) {
                overlaps.pull_requests.push(pr.number);
                if let Some(worktree) = &pr.worktree_path {
                    overlaps.worktrees.push(portable_path(worktree, root));
                }
            }
        }
    }
    if let Some(body) = &captured.claims {
        for claim in &body.claims {
            if claim.issue == Some(number) && claim.state == "active" {
                overlaps.claims.push(claim.id.clone());
                if let Some(worktree) = &claim.worktree {
                    overlaps.worktrees.push(portable_path(worktree, root));
                }
            }
        }
    }
    if let Some(issue) = captured_issue(captured, number) {
        overlaps.resources = issue.conflict_resources.clone();
    }
    overlaps.worktrees.sort();
    overlaps.worktrees.dedup();
    overlaps
}

struct CaseEvaluation {
    violations: Vec<WorkSelectionLawViolationV1>,
    migration_posture: Vec<String>,
}

fn violation(
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
    detail: String,
) -> WorkSelectionLawViolationV1 {
    WorkSelectionLawViolationV1 { law, route, detail }
}

/// Run every identity law for one packet against the basis, the compiled
/// snapshot and the captured sources (the snapshot only surfaces
/// campaign-member issues; standalone subjects resolve from captured
/// sources). Laws run in a fixed order; each wrong identity is reported once
/// with its exact recompile/reconcile route.
fn check_packet(
    packet: &SelectedWorkIdentityV1,
    basis: &PortfolioBasisV1,
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
) -> CaseEvaluation {
    let mut evaluation = CaseEvaluation {
        violations: Vec::new(),
        migration_posture: Vec::new(),
    };

    // Repository identity.
    if packet.repository != basis.repository.repository {
        evaluation.violations.push(violation(
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "selection packet names repository `{}` but the portfolio basis is `{}`; \
                 recompile the basis for the intended repository with `cargo xtask work portfolio`",
                packet.repository, basis.repository.repository
            ),
        ));
    }

    // Subject identity: at least one subject, canonical string forms, and
    // the subject must exist in the compiled portfolio or the captured
    // issue source (standalone work no campaign records stays
    // representable, RIPR-SPEC-0244 acceptance 4).
    let mut subject_known = false;
    let mut standalone_issue = false;
    if packet.issue.is_none() && packet.work_item.is_none() && packet.pull_request.is_none() {
        evaluation.violations.push(violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "selection packet binds no issue, work item or pull request subject".to_string(),
        ));
    }
    if let Some(issue) = &packet.issue {
        let canonical = issue_identity(issue.number);
        if issue.identity != canonical {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "issue identity `{}` is not the canonical form `{canonical}`",
                    issue.identity
                ),
            ));
        }
        if snapshot_issue(snapshot, issue.number).is_some() {
            subject_known = true;
        } else if captured_issue(captured, issue.number).is_some() {
            subject_known = true;
            standalone_issue = true;
        } else {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "issue `#{}` exists neither in the compiled portfolio nor in the captured \
                     issue source; recompile the portfolio and reselect the intended issue",
                    issue.number
                ),
            ));
        }
    }
    if let Some(work_item) = &packet.work_item {
        let canonical = work_item_identity(&work_item.id);
        if work_item.id.trim().is_empty() || work_item.identity != canonical {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "work-item identity `{}` is not the canonical form `{canonical}`",
                    work_item.identity
                ),
            ));
        }
        // A work-item subject is only known when it binds alongside an issue
        // or pull-request subject the captured sources can resolve; a bare
        // work-item id has no captured source of record, so it cannot make
        // the subject known (unknown stays unknown, never invented).
    }
    if packet.issue.is_none() && packet.work_item.is_some() && packet.pull_request.is_none() {
        evaluation.violations.push(violation(
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
            "a bare work-item subject cannot be resolved against the captured sources; \
             reselect with an issue or pull-request subject the portfolio can recompile"
                .to_string(),
        ));
    }
    if let Some(pr) = packet.pull_request {
        // The pull-request subject resolves from the captured pull-request
        // source first (the compiled snapshot drops PRs whose only linked
        // issues are standalone), then the snapshot; when both the PR row and
        // an issue subject are available, the PR must link that issue, so a
        // PR opened for different work cannot be claimed for this selection.
        let captured_row = captured
            .pull_requests
            .as_ref()
            .and_then(|body| body.pull_requests.iter().find(|row| row.number == pr));
        let known =
            captured_row.is_some() || snapshot.pull_requests.iter().any(|row| row.number == pr);
        if !known {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "pull request `#{pr}` exists in neither the captured pull-request source \
                     nor the compiled portfolio; recompile the portfolio and reselect"
                ),
            ));
        } else if let (Some(row), Some(issue)) = (captured_row, &packet.issue)
            && !row.linked_issues.contains(&issue.number)
        {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "pull request `#{pr}` is not linked to issue `#{}` in the captured source; \
                     it records different work and cannot carry this selection",
                    issue.number
                ),
            ));
        }
    }

    // Selection identity: the recorded selection id must be the canonical
    // derived form for the packet's own action and issue binding; a
    // selection id naming another issue or action contradicts the packet.
    if let Some(issue) = &packet.issue {
        let derived = selection_identity(packet.lifecycle_action, issue.number);
        if packet.selection_id != derived {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::SubjectIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "selection id `{}` is not the canonical derived form `{derived}` for \
                     action `{}` on issue `#{}`",
                    packet.selection_id,
                    packet.lifecycle_action.wire_name(),
                    issue.number
                ),
            ));
        }
    }

    // Scope-reference identity: accepted requirements and implementation
    // slices must resolve in the captured cargo-allow graph when that source
    // is present; a spec ref pins the canonical `RIPR-SPEC-NNNN` wire shape
    // and, once requirements are bound, must belong to one of them. When the
    // graph source is absent the resolution evidence is unavailable: only
    // the wire shape is pinned, never invented membership.
    if let Some(graph) = &captured.cargo_allow {
        let mut accepted_specs: BTreeSet<&str> = BTreeSet::new();
        for requirement_id in &packet.accepted_requirements {
            match graph
                .requirements
                .iter()
                .find(|row| &row.id == requirement_id)
            {
                Some(row) => {
                    accepted_specs.extend(row.spec_refs.iter().map(String::as_str));
                }
                None => {
                    evaluation.violations.push(violation(
                        WorkSelectionLawV1::SubjectIdentity,
                        WorkSelectionRouteV1::ReconcileSelection,
                        format!(
                            "accepted requirement `{requirement_id}` is not recorded in the \
                             captured cargo-allow graph; scope references must stay source-linked"
                        ),
                    ));
                }
            }
        }
        for spec_ref in &packet.spec_refs {
            if !spec_ref_has_canonical_shape(spec_ref) {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "spec ref `{spec_ref}` is not the canonical `RIPR-SPEC-NNNN` wire form"
                    ),
                ));
            } else if !packet.accepted_requirements.is_empty()
                && !accepted_specs.contains(spec_ref.as_str())
            {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "spec ref `{spec_ref}` is not a spec of the accepted requirements \
                         {reqs:?}; reconcile the pinned scope",
                        reqs = packet.accepted_requirements
                    ),
                ));
            }
        }
        for slice_id in &packet.implementation_slices {
            match graph
                .requirements
                .iter()
                .flat_map(|row| row.slices.iter())
                .find(|slice| &slice.id == slice_id)
            {
                Some(slice) => {
                    if let Some(issue) = &packet.issue
                        && !slice.issue_refs.contains(&issue.number)
                    {
                        evaluation.violations.push(violation(
                            WorkSelectionLawV1::SubjectIdentity,
                            WorkSelectionRouteV1::ReconcileSelection,
                            format!(
                                "implementation slice `{slice_id}` records issues {:?} and does \
                                 not list issue `#{}`; it cannot carry this selection",
                                slice.issue_refs, issue.number
                            ),
                        ));
                    }
                }
                None => {
                    evaluation.violations.push(violation(
                        WorkSelectionLawV1::SubjectIdentity,
                        WorkSelectionRouteV1::ReconcileSelection,
                        format!(
                            "implementation slice `{slice_id}` is not recorded in the captured \
                             cargo-allow graph; scope references must stay source-linked"
                        ),
                    ));
                }
            }
        }
    } else {
        for spec_ref in &packet.spec_refs {
            if !spec_ref_has_canonical_shape(spec_ref) {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "spec ref `{spec_ref}` is not the canonical `RIPR-SPEC-NNNN` wire form"
                    ),
                ));
            }
        }
    }

    // Action identity: the selected lifecycle action must be exactly the
    // portfolio candidate's classified kind for the subject issue, or — for
    // a standalone issue no campaign records — the kind the compiler's own
    // classification law derives from the captured sources.
    if let Some(issue) = &packet.issue
        && subject_known
    {
        if let Some(candidate) = snapshot_candidate(snapshot, issue.number) {
            if packet.candidate_id != candidate_identity(issue.number)
                || packet.candidate_id != candidate.candidate_id
            {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::ActionIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "candidate id `{}` does not match the portfolio candidate `{}`",
                        packet.candidate_id, candidate.candidate_id
                    ),
                ));
            }
            if packet.lifecycle_action != candidate.kind {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::ActionIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "lifecycle action `{}` mismatches the portfolio candidate kind `{}` \
                             for issue `#{}`; recompile the portfolio and reconcile the selection",
                        packet.lifecycle_action.wire_name(),
                        candidate.kind.wire_name(),
                        issue.number
                    ),
                ));
            }
            if candidate.single_agent_preferred && !packet.role_context_budget.single_agent {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "issue `#{}` is recorded single-agent preferred; the packet budget \
                             must keep the role profile single-agent",
                        issue.number
                    ),
                ));
            }
        } else if standalone_issue && let Some(issue_row) = captured_issue(captured, issue.number) {
            let kind = classify_captured_issue(captured, issue_row);
            if packet.candidate_id != candidate_identity(issue.number)
                || packet.lifecycle_action != kind
            {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::ActionIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "lifecycle action `{}` mismatches the captured classification \
                                 `{}` for standalone issue `#{}`; recompile the portfolio and \
                                 reconcile the selection",
                        packet.lifecycle_action.wire_name(),
                        kind.wire_name(),
                        issue.number
                    ),
                ));
            }
            if issue_row.single_agent_preferred && !packet.role_context_budget.single_agent {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::SubjectIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "issue `#{}` is recorded single-agent preferred; the packet \
                                 budget must keep the role profile single-agent",
                        issue.number
                    ),
                ));
            }
        }
    }

    // Basis identity: the recorded basis head must be the captured
    // default-branch head. A stale basis routes recompilation, never silent
    // release or takeover.
    if packet.basis_sha != basis.repository.default_branch_sha {
        evaluation.violations.push(violation(
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
            format!(
                "recorded basis `{}` no longer matches the captured default-branch head `{}`; \
                 recompile the basis with `cargo xtask work portfolio` and rebind the selection",
                packet.basis_sha, basis.repository.default_branch_sha
            ),
        ));
    }

    // Head identity.
    if let Some(expected_head) = &packet.expected_head
        && expected_head != &basis.repository.default_branch_sha
    {
        evaluation.violations.push(violation(
            WorkSelectionLawV1::HeadIdentity,
            WorkSelectionRouteV1::ReconcileHead,
            format!(
                "expected head `{expected_head}` does not match the basis head `{}`; \
                     reconcile the exact head before any review or merge",
                basis.repository.default_branch_sha
            ),
        ));
    }

    // Worktree identity: every claimed worktree must exist in the captured
    // local state — the captured worktree list itself, plus worktrees
    // recorded on claims and PRs in the captured sources.
    if !packet.overlaps.worktrees.is_empty() {
        let mut known_worktrees: BTreeSet<String> = BTreeSet::new();
        // Same effective root as the compiler: claim/PR worktree spellings
        // must relativize exactly as the compiled snapshot renders them.
        let root = captured.local_state.as_ref().map_or_else(
            || captured.manifest.root.as_str(),
            |state| state.root.as_str(),
        );
        if let Some(body) = &captured.local_state {
            for worktree in &body.worktrees {
                known_worktrees.insert(portable_path(&worktree.path, &body.root));
            }
        }
        if let Some(body) = &captured.claims {
            for claim in &body.claims {
                if let Some(worktree) = &claim.worktree {
                    known_worktrees.insert(portable_path(worktree, root));
                }
            }
        }
        if let Some(body) = &captured.pull_requests {
            for pr in &body.pull_requests {
                if let Some(worktree) = &pr.worktree_path {
                    known_worktrees.insert(portable_path(worktree, root));
                }
            }
        }
        for declared in &packet.overlaps.worktrees {
            if !known_worktrees.contains(declared) {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::WorktreeIdentity,
                    WorkSelectionRouteV1::ReconcileResources,
                    format!(
                        "claimed worktree `{declared}` is not a captured local worktree; \
                         reconcile the writer resources before claiming it"
                    ),
                ));
            }
        }
    }

    // Campaign reference identity: known campaigns, no duplicates, member
    // relation must be recorded by the campaign. Zero campaigns is valid
    // standalone work.
    let mut seen_campaigns = BTreeSet::new();
    for campaign_ref in &packet.campaign_refs {
        if !seen_campaigns.insert(campaign_ref.id.clone()) {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::CampaignRefIdentity,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "campaign `{}` is referenced more than once; canonicalize the references",
                    campaign_ref.id
                ),
            ));
        }
        let recorded = snapshot
            .campaigns
            .iter()
            .find(|campaign| campaign.id == campaign_ref.id);
        match recorded {
            Some(campaign) => {
                if campaign_ref.relation == RelevantCampaignRelationV1::Member
                    && let Some(issue) = &packet.issue
                    && subject_known
                    && !campaign.issues.contains(&issue.number)
                {
                    evaluation.violations.push(violation(
                        WorkSelectionLawV1::CampaignRefIdentity,
                        WorkSelectionRouteV1::ReconcileSelection,
                        format!(
                            "campaign `{}` does not record issue `#{}` as a member; \
                                     downgrade the relation to relevant or reconcile the selection",
                            campaign_ref.id, issue.number
                        ),
                    ));
                }
            }
            None => {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::CampaignRefIdentity,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "campaign `{}` is not a recorded campaign; campaign references must \
                         stay source-linked",
                        campaign_ref.id
                    ),
                ));
            }
        }
    }

    // Legacy compatibility references: resolve read-only, emit migration
    // posture, and reject any authority grant.
    if let Some(legacy_ref) = &packet.legacy_active_goal_ref {
        evaluation.migration_posture.push(format!(
            "legacy_active_goal_ref `{legacy_ref}` accepted as a read-only historical \
                 campaign reference; it grants no write, readiness, merge or closeout authority"
        ));
        let resolved = packet.campaign_refs.iter().any(|campaign_ref| {
            campaign_ref.id == *legacy_ref
                && campaign_ref.relation == RelevantCampaignRelationV1::Historical
        });
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *legacy_ref);
        if !resolved || !known {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{legacy_ref}` must resolve to a recorded campaign \
                     carried as a historical reference"
                ),
            ));
        }
    }
    if let Some(legacy_ref) = &packet.legacy_current_work_item_ref {
        evaluation.migration_posture.push(format!(
            "legacy_current_work_item_ref `{legacy_ref}` accepted as a read-only historical \
                 work-item reference; it grants no write, readiness, merge or closeout authority"
        ));
        let matches_subject = packet
            .issue
            .as_ref()
            .is_some_and(|issue| *legacy_ref == issue_identity(issue.number));
        if !matches_subject {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_current_work_item_ref `{legacy_ref}` does not match the selected \
                     issue identity"
                ),
            ));
        }
    }

    // Overlap visibility: every live overlap the portfolio shows for the
    // subject must be recorded in the packet before any mutation.
    if let Some(issue) = &packet.issue
        && subject_known
    {
        let live = if standalone_issue {
            live_overlaps_captured(snapshot, captured, issue.number)
        } else {
            live_overlaps(snapshot, issue.number)
        };
        for pr in &live.pull_requests {
            if !packet.overlaps.pull_requests.contains(pr) {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::OverlapVisibility,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "open PR `#{pr}` overlaps issue `#{}` but is not recorded in the \
                             packet overlaps; re-inspect the portfolio and reconcile the \
                             selection before any mutation",
                        issue.number
                    ),
                ));
            }
        }
        for claim in &live.claims {
            if !packet.overlaps.claims.contains(claim) {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::OverlapVisibility,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "active claim `{claim}` overlaps issue `#{}` but is not recorded in \
                             the packet overlaps; reconcile the selection before any mutation",
                        issue.number
                    ),
                ));
            }
        }
        for worktree in &live.worktrees {
            if !packet.overlaps.worktrees.contains(worktree) {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::OverlapVisibility,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "worktree `{worktree}` overlaps issue `#{}` but is not recorded in \
                             the packet overlaps; reconcile the selection before any mutation",
                        issue.number
                    ),
                ));
            }
        }
        for resource in &live.resources {
            if !packet.overlaps.resources.contains(resource) {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::OverlapVisibility,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "semantic resource `{resource}` overlaps issue `#{}` but is not \
                             recorded in the packet overlaps; reconcile the selection before \
                             any mutation",
                        issue.number
                    ),
                ));
            }
        }
        for overlap_issue in &live.issues {
            if !packet.overlaps.issues.contains(overlap_issue) {
                evaluation.violations.push(violation(
                    WorkSelectionLawV1::OverlapVisibility,
                    WorkSelectionRouteV1::ReconcileSelection,
                    format!(
                        "duplicate-family issue `#{overlap_issue}` overlaps issue `#{}` but is \
                             not recorded in the packet overlaps; reconcile the selection \
                             before any mutation",
                        issue.number
                    ),
                ));
            }
        }
    }

    evaluation
}

/// Evaluate one legacy compatibility packet. Any attempted authority grant
/// fails closed; references resolve read-only and emit migration posture.
fn check_legacy_packet(
    legacy: &LegacyWorkPacketV1,
    snapshot: &WorkPortfolioSnapshotV1,
) -> CaseEvaluation {
    let mut evaluation = CaseEvaluation {
        violations: Vec::new(),
        migration_posture: Vec::new(),
    };
    if !legacy.attempted_authorities.is_empty() {
        evaluation.violations.push(violation(
            WorkSelectionLawV1::LegacyCompatibility,
            WorkSelectionRouteV1::RejectLegacyAuthority,
            format!(
                "legacy packet attempts the authorities {:?}; legacy_active_goal_ref and \
                 legacy_current_work_item_ref are read-only compatibility references and grant \
                 no write, readiness, merge or closeout authority",
                legacy.attempted_authorities
            ),
        ));
    }
    if let Some(reference) = &legacy.legacy_active_goal_ref {
        let known = snapshot
            .campaigns
            .iter()
            .any(|campaign| campaign.id == *reference);
        if known {
            evaluation.migration_posture.push(format!(
                "legacy_active_goal_ref `{reference}` accepted read-only as a historical \
                 campaign reference with no authority"
            ));
        } else {
            evaluation.violations.push(violation(
                WorkSelectionLawV1::LegacyCompatibility,
                WorkSelectionRouteV1::ReconcileSelection,
                format!(
                    "legacy_active_goal_ref `{reference}` does not resolve to a recorded campaign"
                ),
            ));
            evaluation.migration_posture.push(format!(
                "legacy_active_goal_ref `{reference}` could not be resolved; migration posture \
                 unknown"
            ));
        }
    }
    if let Some(reference) = &legacy.legacy_current_work_item_ref {
        evaluation.migration_posture.push(format!(
            "legacy_current_work_item_ref `{reference}` accepted read-only as a historical \
             work-item reference with no authority"
        ));
    }
    evaluation
}

/// Plan-context acceptance note derived from issue/PR evidence. This is a
/// projection note for the PR C burn-down consumer, never completion
/// authority. Standalone issues resolve from the captured sources because
/// the snapshot only surfaces campaign-member issues.
fn acceptance_state(
    snapshot: &WorkPortfolioSnapshotV1,
    captured: &WorkCapturedDirV1,
    issue: Option<u64>,
) -> &'static str {
    let Some(number) = issue else {
        return "unknown";
    };
    let issue_state: Option<String> = snapshot_issue(snapshot, number)
        .map(|row| row.state.clone())
        .or_else(|| captured_issue(captured, number).map(|row| row.state.clone()));
    let Some(state) = issue_state else {
        return "unknown";
    };
    if state == "closed" {
        return "covered";
    }
    let linked = |pr_state: &str| -> bool {
        if let Some(body) = &captured.pull_requests
            && body
                .pull_requests
                .iter()
                .any(|pr| pr.state == pr_state && pr.linked_issues.contains(&number))
        {
            return true;
        }
        snapshot
            .pull_requests
            .iter()
            .any(|pr| pr.state == pr_state && pr.linked_issues.contains(&number))
    };
    if linked("merged") {
        return "acceptance_uncovered";
    }
    if linked("open") {
        return "in_flight";
    }
    "not_started"
}

// ---------------------------------------------------------------------------
// Corpus runner and projections.
// ---------------------------------------------------------------------------

fn corpus_root(corpus_dir: &str) -> std::path::PathBuf {
    // A user-supplied `--corpus` directory is cwd-relative (absolute paths
    // honored as-is); only when it does not exist relative to the cwd does
    // it fall back to the workspace root, which keeps the documented default
    // `fixtures/work_selection_identity` working from any directory.
    let as_given = Path::new(corpus_dir);
    if as_given.is_absolute() || as_given.is_dir() {
        as_given.to_path_buf()
    } else {
        workspace_path(corpus_dir)
    }
}

/// Resolve a scenario's captured directory. A corpus may carry its own
/// captured inputs under `<corpus>/captured/`; when the scenario's captured
/// path exists there it wins, otherwise the path stays relative to the
/// repository `fixtures/` root (the committed corpus mixes both). Paths must
/// be plain relative paths on every host platform: absolute paths, `..` or
/// backslash segments fail closed instead of escaping the fixture boundary.
fn resolve_scenario_captured(corpus_root: &Path, captured: &str) -> Result<PathBuf, String> {
    let path = Path::new(captured);
    if captured.trim().is_empty() || path.is_absolute() || captured.contains('\\') {
        return Err(format!(
            "scenario captured path `{captured}` is not a plain relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "scenario captured path `{captured}` contains a forbidden component"
            ));
        }
    }
    let local = corpus_root.join("captured").join(path);
    if local.is_dir() {
        Ok(local)
    } else {
        Ok(workspace_path("fixtures").join(path))
    }
}

fn run_scenario(
    scenario: &WorkSelectionScenarioV1,
    corpus: &WorkSelectionIdentityCorpusV1,
    corpus_root: &Path,
) -> Result<WorkSelectionScenarioResultV1, String> {
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
    let primary_issue = scenario
        .cases
        .first()
        .and_then(|case| case.packet.as_ref())
        .and_then(|packet| packet.issue.as_ref())
        .map(|issue| issue.number);
    let mut results = Vec::new();
    for (index, case) in scenario.cases.iter().enumerate() {
        let (evaluation, selection_digest, selection_key) =
            match (&case.packet, &case.legacy_packet) {
                (Some(packet), _) => {
                    let evaluation = check_packet(packet, &basis, &snapshot, &captured);
                    let selection_digest = if evaluation.violations.is_empty() {
                        Some(portable_identity("selected work identity", packet)?)
                    } else {
                        None
                    };
                    let selection_key = packet
                        .issue
                        .as_ref()
                        .map(|issue| selection_identity(packet.lifecycle_action, issue.number));
                    (evaluation, selection_digest, selection_key)
                }
                (None, Some(legacy)) => (check_legacy_packet(legacy, &snapshot), None, None),
                (None, None) => {
                    return Err(format!(
                        "scenario `{}` case {index} carries neither a packet nor a legacy packet",
                        scenario.id
                    ));
                }
            };
        let passed = match case.expect {
            WorkSelectionExpectV1::Pass => evaluation.violations.is_empty(),
            WorkSelectionExpectV1::Fail => {
                // Exact match on the law/route pair multiset: a negative
                // case pins every occurrence. A superset or subset pass
                // would let a regression that adds a spurious violation to
                // a wrong packet stay green, claiming discrimination the
                // corpus does not actually hold.
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
        results.push(WorkSelectionCaseResultV1 {
            case_index: index as u64,
            status: if passed { "passed" } else { "failed" }.to_string(),
            expected: match case.expect {
                WorkSelectionExpectV1::Pass => "pass",
                WorkSelectionExpectV1::Fail => "fail",
            }
            .to_string(),
            plan_disposition: case.plan_disposition,
            selection_key,
            violations: evaluation.violations,
            migration_posture: evaluation.migration_posture,
            selection_identity: selection_digest,
        });
    }
    Ok(WorkSelectionScenarioResultV1 {
        scenario_id: scenario.id.clone(),
        title: scenario.title.clone(),
        captured: scenario.captured.clone(),
        portfolio_identity: basis.snapshot_identity.clone(),
        basis_sha: basis.repository.default_branch_sha.clone(),
        snapshot_completeness: basis.snapshot_completeness,
        acceptance_state: acceptance_state(&snapshot, &captured, primary_issue).to_string(),
        cases: results,
    })
}

pub(crate) fn run_work_selection_check(
    corpus_dir: &str,
    corpus: &WorkSelectionIdentityCorpusV1,
) -> Result<WorkSelectionCheckViewV1, String> {
    let mut results = Vec::new();
    let mut passed = 0_u64;
    let mut cases_total = 0_u64;
    let root = corpus_root(corpus_dir);
    for scenario in &corpus.scenarios {
        let result = run_scenario(scenario, corpus, &root)?;
        for case in &result.cases {
            cases_total += 1;
            if case.status == "passed" {
                passed += 1;
            }
        }
        results.push(result);
    }
    let failed = cases_total - passed;
    let view = WorkSelectionCheckViewV1 {
        schema_version: WORK_SELECTION_CHECK_VIEW_SCHEMA_VERSION.to_string(),
        repository: corpus.repository.clone(),
        selection_policy: corpus.selection_policy.clone(),
        selection_policy_version: corpus.selection_policy_version.clone(),
        corpus: corpus_dir.to_string(),
        counts: WorkSelectionCheckCountsV1 {
            scenarios: results.len() as u64,
            cases: cases_total,
            passed,
            failed,
        },
        results,
        retrieval_commands: vec![
            "cargo xtask work selection check".to_string(),
            "cargo xtask work portfolio".to_string(),
        ],
        claim_boundary: WORK_SELECTION_CHECK_CLAIM_BOUNDARY.to_string(),
        portable_identity: String::new(),
    };
    let digest = portable_identity("work selection check view", &view)?;
    let mut view = view;
    view.portable_identity = digest;
    Ok(view)
}

pub(crate) fn work_selection_check_json(view: &WorkSelectionCheckViewV1) -> Result<String, String> {
    work_portfolio_json(view)
}

/// Render a serde-enum value by its stable wire name, never the Rust `Debug`
/// spelling, so the human Markdown projection matches the JSON wire forms
/// exactly and cannot drift from the corpus vocabulary.
fn selection_wire_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub(crate) fn work_selection_check_markdown(view: &WorkSelectionCheckViewV1) -> String {
    let mut out = String::new();
    out.push_str("# Selected-work identity check\n\n");
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
    let mut disposition_counts: BTreeMap<WorkPlanDispositionV1, u64> = BTreeMap::new();
    for case in view
        .results
        .iter()
        .flat_map(|scenario| scenario.cases.iter())
    {
        *disposition_counts.entry(case.plan_disposition).or_default() += 1;
    }
    out.push_str("Plan dispositions (frozen vocabulary pinned per case):\n\n");
    for disposition in WorkPlanDispositionV1::all() {
        let count = disposition_counts.get(&disposition).copied().unwrap_or(0);
        out.push_str(&format!(
            "- `{}`: {count}\n",
            selection_wire_name(&disposition)
        ));
    }
    out.push('\n');
    out.push_str("| scenario | captured | completeness | acceptance | cases |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for scenario in &view.results {
        let cases_passed = scenario
            .cases
            .iter()
            .filter(|case| case.status == "passed")
            .count();
        out.push_str(&format!(
            "| {} | {} | {} | {} | {}/{} |\n",
            scenario.scenario_id,
            scenario.captured,
            selection_wire_name(&scenario.snapshot_completeness),
            scenario.acceptance_state,
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

/// `cargo xtask work selection check [--corpus <dir>] [--json]` (#1706 PR A,
/// RIPR-SPEC-0244): validate the committed selection corpus against the
/// compiled portfolio bases and render JSON or the derived Markdown.
/// Read-only: no GitHub, branch, worktree, claim, spec, campaign or source
/// state is touched.
pub(crate) fn work_selection_check_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work selection check [--corpus <dir>] [--json]";
    let mut corpus_dir = DEFAULT_WORK_SELECTION_CORPUS_DIR.to_string();
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
    let corpus = load_work_selection_corpus(&body)?;
    let view = run_work_selection_check(&corpus_dir, &corpus)?;
    let json_body = work_selection_check_json(&view)?;
    crate::write_report("work-selection-check.json", &json_body)?;
    crate::write_report(
        "work-selection-check.md",
        &work_selection_check_markdown(&view),
    )?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_selection_check_markdown(&view));
    }
    if view.counts.failed > 0 {
        return Err(format!(
            "work selection check: {} of {} cases failed; see work-selection-check.md",
            view.counts.failed, view.counts.cases
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionProvenanceFileV1 {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionProvenanceV1 {
    pub schema_version: String,
    pub repository: String,
    pub captured_at: String,
    pub capture_method: String,
    pub files: Vec<WorkSelectionProvenanceFileV1>,
}

pub(crate) fn load_work_selection_provenance(
    body: &str,
) -> Result<WorkSelectionProvenanceV1, String> {
    let provenance: WorkSelectionProvenanceV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work selection identity provenance: {error}"))?;
    if provenance.schema_version != WORK_SELECTION_IDENTITY_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work selection identity provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "work selection identity provenance must record repository and capture_method"
                .to_string(),
        );
    }
    if provenance.captured_at.trim().is_empty() {
        return Err("work selection identity provenance must record captured_at".to_string());
    }
    let mut paths = BTreeSet::new();
    for file in &provenance.files {
        reject_unsafe_selection_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(format!(
                "work selection identity provenance duplicates path `{}`",
                file.path
            ));
        }
    }
    Ok(provenance)
}

/// Fail closed on provenance paths that could escape the corpus root:
/// absolute paths, `..` components, empty segments or empty strings, on
/// every host platform. Backslash is rejected everywhere (not only on
/// Windows) so a path authored for one platform can never silently bypass
/// the gate on another.
fn reject_unsafe_selection_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() || value.contains('\\') {
        return Err(format!(
            "work selection identity provenance path `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!(
                "work selection identity provenance path `{value}` contains a forbidden component"
            ));
        }
    }
    Ok(())
}

/// The twelve #1706 required scenario ids, pinned in declaration order.
pub(crate) const WORK_SELECTION_SCENARIO_IDS: [&str; 12] = [
    "scenario-01-selected-issue-one-campaign",
    "scenario-02-selected-issue-several-related-campaigns",
    "scenario-03-standalone-no-campaign-placement",
    "scenario-04-two-campaigns-independent-disjoint-resources",
    "scenario-05-existing-open-pr-selected-for-repair",
    "scenario-06-legacy-writer-authorization-fails",
    "scenario-07-snapshot-change-issue-head-compatible",
    "scenario-08-overlap-introduced-forces-reconcile",
    "scenario-09-wrong-identities-fail-visibly",
    "scenario-10-burndown-partial-acceptance-uncovered",
    "scenario-11-fresh-root-resume-from-artifacts",
    "scenario-12-narrow-single-agent-despite-active-campaigns",
];

#[cfg(test)]
fn committed_corpus() -> Result<WorkSelectionIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_SELECTION_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed selection corpus: {error}"))?;
    load_work_selection_corpus(&body)
}

/// Validate the committed selected-work identity corpus fail-closed:
/// provenance digests, the twelve required scenario ids, the read-only
/// authority posture (no `active.toml` or chat authority anywhere in the
/// corpus bytes), and every scenario's pinned laws. Registered in
/// `check-fixture-contracts`.
pub(crate) fn validate_work_selection_identity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_WORK_SELECTION_CORPUS_DIR);
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_selection_identity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_work_selection_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/work_selection_identity: {error}"));
            return;
        }
    };
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/work_selection_identity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/work_selection_identity: {} digest drifted: recorded `{}`, \
                 recomputed `work-selection:sha256:{}`",
                path.display(),
                file.sha256,
                crate::blind_journey::sha256_hex(&bytes)
            ));
        }
    }
    // Reverse direction: every corpus byte must be listed in the provenance,
    // or an unlisted file would bypass the digest gate entirely.
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
                        "fixtures/work_selection_identity: {} is not listed in provenance.json; \
                         unlisted fixture bytes bypass the digest gate",
                        path.display()
                    ));
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_selection_identity: corpus directory is unreadable: {error}"
        )),
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_selection_identity: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_work_selection_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/work_selection_identity: {error}"));
            return;
        }
    };
    let ids: Vec<&str> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect();
    if ids != WORK_SELECTION_SCENARIO_IDS {
        violations.push(format!(
            "fixtures/work_selection_identity: scenario ids {ids:?} drifted from {:?}",
            WORK_SELECTION_SCENARIO_IDS
        ));
    }
    // Fixture 11: a fresh root must resume from committed artifacts only;
    // no chat or singleton goal file may appear anywhere in the corpus.
    let lower = corpus_body.to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if lower.contains(forbidden) {
            violations.push(format!(
                "fixtures/work_selection_identity: corpus.json references `{forbidden}`; \
                 resume authority comes from committed artifacts only"
            ));
        }
    }
    match run_work_selection_check(DEFAULT_WORK_SELECTION_CORPUS_DIR, &corpus) {
        Ok(view) => {
            for scenario in &view.results {
                for case in &scenario.cases {
                    if case.status != "passed" {
                        violations.push(format!(
                            "fixtures/work_selection_identity: {} case {} {} (expected {})",
                            scenario.scenario_id, case.case_index, case.status, case.expected
                        ));
                    }
                }
            }
        }
        Err(error) => violations.push(format!("fixtures/work_selection_identity: {error}")),
    }
}

#[cfg(test)]
mod tests;
