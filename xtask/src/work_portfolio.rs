//! Read-only deterministic multi-campaign work-portfolio compiler (#1704,
//! slice #1794, RIPR-SPEC-0234).
//!
//! `cargo xtask work portfolio [--captured <dir>] [--json]`,
//! `cargo xtask work candidates [--campaign <id>] [--surface <id>]
//! [--limit <n>] [--json]` and
//! `cargo xtask work explain --candidate <id> [--json]` compile one versioned
//! `WorkPortfolioSnapshotV1` from immutable captured inputs (default
//! `fixtures/work_portfolio/corpus`) and render it. All three commands share
//! one pure compilation: `portfolio` renders the whole snapshot, `candidates`
//! renders the ranked candidate array under neutral filters with a bounded
//! `--limit` (default 10), and `explain` renders exactly one candidate by
//! stable id.
//!
//! Selection law: this module selects no work, claims nothing, and mutates
//! nothing. It reads only committed captured bytes; no campaign record, label,
//! issue age, title keyword or release language is consulted for ranking, and
//! no default/current campaign is required or synthesized. Live `gh` output
//! never enters the portable identity. Missing, stale or unavailable sources
//! stay visible, degrade the affected candidates to `partial`/`not_proven`
//! with named reasons, and never produce fabricated readiness.
//!
//! Determinism law: fixed captured inputs produce byte-stable normalized JSON
//! independent of input ordering and absolute checkout spelling — every
//! output array is sorted by stable identity, local paths are relativized
//! against the captured root, and volatile timestamps and request ids stay
//! inside source observations but outside the portable identity digest.
//!
//! Ranking law: candidates order by the eight explicit factors from #1704 in
//! exactly that order, each factor inspectable on the candidate with its
//! contribution and source references; the sort is a lexicographic
//! application of the declared factor sequence and no hidden numeric score
//! exists anywhere in the DTOs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::issue_lifecycle_attempt::IssueLifecycleDispositionV1;

pub(crate) const WORK_CAPTURED_MANIFEST_SCHEMA_VERSION: &str = "work_captured_manifest.v1";
pub(crate) const WORK_CAMPAIGNS_SCHEMA_VERSION: &str = "work_campaigns.v1";
pub(crate) const WORK_ISSUES_SCHEMA_VERSION: &str = "work_issues.v1";
pub(crate) const WORK_PULL_REQUESTS_SCHEMA_VERSION: &str = "work_pull_requests.v1";
pub(crate) const WORK_CLAIMS_SCHEMA_VERSION: &str = "work_claims.v1";
pub(crate) const WORK_LOCAL_STATE_SCHEMA_VERSION: &str = "work_local_state.v1";
pub(crate) const WORK_SURFACES_SCHEMA_VERSION: &str = "work_surfaces.v1";
pub(crate) const WORK_CARGO_ALLOW_SCHEMA_VERSION: &str = "work_cargo_allow.v1";
pub(crate) const WORK_PORTFOLIO_SNAPSHOT_SCHEMA_VERSION: &str = "work_portfolio_snapshot.v1";
pub(crate) const WORK_CANDIDATES_VIEW_SCHEMA_VERSION: &str = "work_candidates_view.v1";
pub(crate) const WORK_EXPLAIN_VIEW_SCHEMA_VERSION: &str = "work_explain_view.v1";

pub(crate) const DEFAULT_WORK_CAPTURED_DIR: &str = "fixtures/work_portfolio/corpus";
pub(crate) const DEFAULT_CANDIDATE_LIMIT: usize = 10;

pub(crate) const WORK_PORTFOLIO_CLAIM_BOUNDARY: &str = "Read-only work-portfolio receipt: \
 the snapshot is a deterministic point-in-time projection over committed captured inputs; \
 it selects no work, claims nothing, authorizes nothing, mutates no GitHub state, branch, \
 worktree, claim, spec, campaign or source, and it never fabricates readiness from missing \
 evidence.";

const VOLATILE_IDENTITY_KEYS: [&str; 4] = [
    "observed_at",
    "request_id",
    "captured_at",
    "packet_entrypoint",
];

// ---------------------------------------------------------------------------
// Captured input DTOs (immutable committed bytes).
// ---------------------------------------------------------------------------

/// Closed captured-source vocabulary. One observation per source.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkCapturedSourceKindV1 {
    GithubIssues,
    GithubPullRequests,
    GithubClaims,
    LocalState,
    CargoAllow,
    Campaigns,
    Surfaces,
}

impl WorkCapturedSourceKindV1 {
    /// Canonical declaration order so projections keep a stable row per
    /// source even when a source is unobserved.
    pub(crate) fn all() -> [Self; 7] {
        [
            Self::GithubIssues,
            Self::GithubPullRequests,
            Self::GithubClaims,
            Self::LocalState,
            Self::CargoAllow,
            Self::Campaigns,
            Self::Surfaces,
        ]
    }

    pub(crate) fn file_name(self) -> Option<&'static str> {
        match self {
            Self::GithubIssues => Some("issues.json"),
            Self::GithubPullRequests => Some("pull_requests.json"),
            Self::GithubClaims => Some("claims.json"),
            Self::LocalState => Some("local_state.json"),
            Self::CargoAllow => Some("cargo_allow.json"),
            Self::Campaigns => Some("campaigns.json"),
            Self::Surfaces => Some("surfaces.json"),
        }
    }

    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::GithubIssues => "github_issues",
            Self::GithubPullRequests => "github_pull_requests",
            Self::GithubClaims => "github_claims",
            Self::LocalState => "local_state",
            Self::CargoAllow => "cargo_allow",
            Self::Campaigns => "campaigns",
            Self::Surfaces => "surfaces",
        }
    }
}

/// Observation state of one captured source. `Stale` means the file is
/// present and parseable but its `observed_at` predates the capture window.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkCapturedSourceStateV1 {
    Observed,
    Missing,
    Stale,
    Unavailable,
}

/// One source observation in the manifest. `observed_at` and `request_id`
/// are volatile: they render in the snapshot but never enter the portable
/// identity digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedSourceV1 {
    pub source: WorkCapturedSourceKindV1,
    pub state: WorkCapturedSourceStateV1,
    pub observed_at: Option<String>,
    pub request_id: Option<String>,
    pub note: Option<String>,
}

/// The captured manifest: repository identity, default branch and head, the
/// volatile checkout-root spelling, and every source observation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedManifestV1 {
    pub schema_version: String,
    pub repository: String,
    pub default_branch: String,
    pub default_branch_sha: String,
    pub root: String,
    pub captured_at: String,
    pub sources: Vec<WorkCapturedSourceV1>,
}

/// One durable campaign record. Campaigns carry durable intent only; there
/// is deliberately no default/current field (#1701 removed that authority).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedCampaignV1 {
    pub id: String,
    pub title: String,
    pub state: String,
    pub surfaces: Vec<String>,
    pub issues: Vec<u64>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedCampaignsV1 {
    pub schema_version: String,
    pub campaigns: Vec<WorkCapturedCampaignV1>,
}

/// One blocker on a captured issue.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedBlockerV1 {
    pub kind: String,
    pub reference: String,
    pub description: String,
}

/// Closed cost-class vocabulary; unsourced cost reads `not_proven`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkCostClassV1 {
    Low,
    Moderate,
    High,
    NotProven,
}

/// One captured GitHub issue. `semantic_paths` and `conflict_resources`
/// carry the #1632/#1634 surface/conflict metadata; `contract_state` and
/// `accepted_contracts` carry the cargo-allow contract relationship.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedIssueV1 {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub lifecycle_disposition: Option<IssueLifecycleDispositionV1>,
    pub campaigns: Vec<String>,
    pub blocked_by: Vec<WorkCapturedBlockerV1>,
    pub requirement_refs: Vec<String>,
    pub semantic_paths: Vec<String>,
    pub conflict_resources: Vec<String>,
    pub accepted_contracts: Vec<String>,
    pub contract_state: String,
    pub single_agent_preferred: bool,
    pub honesty_risk: Option<String>,
    pub proof_cost: Option<WorkCostClassV1>,
    pub review_ci_cost: Option<WorkCostClassV1>,
    pub readiness_source: Option<String>,
    pub regression_risks: Vec<String>,
    pub lane: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedIssuesV1 {
    pub schema_version: String,
    pub issues: Vec<WorkCapturedIssueV1>,
}

/// One captured pull request with review and check state.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedPullRequestV1 {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub draft: bool,
    pub head_branch: String,
    pub base_branch: String,
    pub linked_issues: Vec<u64>,
    pub registered_claim: Option<String>,
    pub review_state: String,
    pub unresolved_review_findings: u64,
    pub checks_state: String,
    pub worktree_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedPullRequestsV1 {
    pub schema_version: String,
    pub pull_requests: Vec<WorkCapturedPullRequestV1>,
}

/// One durable issue/work-item claim (#1647).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedClaimV1 {
    pub id: String,
    pub issue: Option<u64>,
    pub branch: String,
    pub worktree: Option<String>,
    pub exclusive: bool,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedClaimsV1 {
    pub schema_version: String,
    pub claims: Vec<WorkCapturedClaimV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedBranchV1 {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedWorktreeV1 {
    pub path: String,
}

/// Captured local truth. `root` is the volatile checkout spelling; both the
/// manifest root and this root relativize to the portable `<root>` token.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedLocalStateV1 {
    pub schema_version: String,
    pub root: String,
    pub branches: Vec<WorkCapturedBranchV1>,
    pub worktrees: Vec<WorkCapturedWorktreeV1>,
}

/// One named repository surface (#1632).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedSurfaceV1 {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedSurfacesV1 {
    pub schema_version: String,
    pub surfaces: Vec<WorkCapturedSurfaceV1>,
}

/// One implementation slice inside one cargo-allow requirement.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedSliceV1 {
    pub id: String,
    pub delta_id: String,
    pub issue_refs: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedRequirementV1 {
    pub id: String,
    pub spec_refs: Vec<String>,
    pub slices: Vec<WorkCapturedSliceV1>,
}

/// The optional cargo-allow requirement/spec/slice/evidence graph. When this
/// source is missing or unavailable, requirement-linked candidates degrade
/// to `partial` and duplicate-family evidence is weakened, never invented.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCapturedCargoAllowV1 {
    pub schema_version: String,
    pub requirements: Vec<WorkCapturedRequirementV1>,
}

/// The fully loaded captured directory.
#[derive(Clone, Debug)]
pub(crate) struct WorkCapturedDirV1 {
    pub manifest: WorkCapturedManifestV1,
    pub campaigns: Option<WorkCapturedCampaignsV1>,
    pub issues: Option<WorkCapturedIssuesV1>,
    pub pull_requests: Option<WorkCapturedPullRequestsV1>,
    pub claims: Option<WorkCapturedClaimsV1>,
    pub local_state: Option<WorkCapturedLocalStateV1>,
    pub surfaces: Option<WorkCapturedSurfacesV1>,
    pub cargo_allow: Option<WorkCapturedCargoAllowV1>,
}

pub(crate) fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn read_captured_file(root: &Path, file_name: &str) -> Result<String, String> {
    let path = root.join(file_name);
    fs::read_to_string(&path)
        .map_err(|error| format!("captured file {} is unreadable: {error}", path.display()))
}

/// Load and shape-check the manifest; every known source must appear exactly
/// once.
pub(crate) fn load_work_captured_manifest(body: &str) -> Result<WorkCapturedManifestV1, String> {
    let manifest: WorkCapturedManifestV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work captured manifest: {error}"))?;
    if manifest.schema_version != WORK_CAPTURED_MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work captured manifest schema `{}`",
            manifest.schema_version
        ));
    }
    if manifest.repository.trim().is_empty()
        || manifest.default_branch.trim().is_empty()
        || manifest.default_branch_sha.trim().is_empty()
        || manifest.root.trim().is_empty()
        || manifest.captured_at.trim().is_empty()
    {
        return Err(
            "work captured manifest must record repository, default_branch, default_branch_sha, root and captured_at"
                .to_string(),
        );
    }
    let mut seen = BTreeSet::new();
    for source in &manifest.sources {
        if !seen.insert(source.source) {
            return Err(format!(
                "work captured manifest duplicates source `{}`",
                source.source.wire_name()
            ));
        }
    }
    for required in WorkCapturedSourceKindV1::all() {
        if !seen.contains(&required) {
            return Err(format!(
                "work captured manifest is missing source `{}`",
                required.wire_name()
            ));
        }
    }
    Ok(manifest)
}

macro_rules! load_captured_body {
    ($body:expr, $ty:ty, $schema:expr, $label:expr) => {{
        let parsed: $ty =
            serde_json::from_str($body).map_err(|error| format!("parse {}: {error}", $label))?;
        if parsed.schema_version != $schema {
            return Err(format!(
                "unsupported {} schema `{}`",
                $label, parsed.schema_version
            ));
        }
        parsed
    }};
}

/// Load one captured directory fail-closed. `missing`/`unavailable` sources
/// must have no file on disk; `observed`/`stale` sources must parse. Every
/// input array is sorted by its stable identity here, so downstream
/// compilation is independent of input ordering.
pub(crate) fn load_work_captured_dir(root: &Path) -> Result<WorkCapturedDirV1, String> {
    let manifest_body = read_captured_file(root, "manifest.json")?;
    let manifest = load_work_captured_manifest(&manifest_body)?;
    let mut source_states = BTreeMap::new();
    for source in &manifest.sources {
        source_states.insert(source.source, source.state);
    }
    let source_label = |state: WorkCapturedSourceStateV1| match state {
        WorkCapturedSourceStateV1::Observed => "observed",
        WorkCapturedSourceStateV1::Missing => "missing",
        WorkCapturedSourceStateV1::Stale => "stale",
        WorkCapturedSourceStateV1::Unavailable => "unavailable",
    };
    let mut loaded = WorkCapturedDirV1 {
        manifest,
        campaigns: None,
        issues: None,
        pull_requests: None,
        claims: None,
        local_state: None,
        surfaces: None,
        cargo_allow: None,
    };
    for kind in WorkCapturedSourceKindV1::all() {
        let file_name = kind
            .file_name()
            .ok_or_else(|| format!("source `{}` has no file", kind.wire_name()))?;
        let present = root.join(file_name).is_file();
        let Some(state) = source_states.get(&kind).copied() else {
            return Err(format!("manifest is missing source `{}`", kind.wire_name()));
        };
        match state {
            WorkCapturedSourceStateV1::Observed | WorkCapturedSourceStateV1::Stale => {
                if !present {
                    return Err(format!(
                        "manifest marks `{}` {} but {} is absent",
                        kind.wire_name(),
                        source_label(state),
                        file_name
                    ));
                }
                let body = read_captured_file(root, file_name)?;
                match kind {
                    WorkCapturedSourceKindV1::GithubIssues => {
                        let mut parsed: WorkCapturedIssuesV1 = load_captured_body!(
                            &body,
                            WorkCapturedIssuesV1,
                            WORK_ISSUES_SCHEMA_VERSION,
                            "work issues"
                        );
                        parsed
                            .issues
                            .sort_by_key(|issue| (issue.number, issue.title.clone()));
                        loaded.issues = Some(parsed);
                    }
                    WorkCapturedSourceKindV1::GithubPullRequests => {
                        let mut parsed: WorkCapturedPullRequestsV1 = load_captured_body!(
                            &body,
                            WorkCapturedPullRequestsV1,
                            WORK_PULL_REQUESTS_SCHEMA_VERSION,
                            "work pull requests"
                        );
                        parsed
                            .pull_requests
                            .sort_by_key(|pr| (pr.number, pr.title.clone()));
                        loaded.pull_requests = Some(parsed);
                    }
                    WorkCapturedSourceKindV1::GithubClaims => {
                        let mut parsed: WorkCapturedClaimsV1 = load_captured_body!(
                            &body,
                            WorkCapturedClaimsV1,
                            WORK_CLAIMS_SCHEMA_VERSION,
                            "work claims"
                        );
                        parsed.claims.sort_by(|left, right| left.id.cmp(&right.id));
                        loaded.claims = Some(parsed);
                    }
                    WorkCapturedSourceKindV1::LocalState => {
                        let mut parsed: WorkCapturedLocalStateV1 = load_captured_body!(
                            &body,
                            WorkCapturedLocalStateV1,
                            WORK_LOCAL_STATE_SCHEMA_VERSION,
                            "work local state"
                        );
                        parsed
                            .branches
                            .sort_by(|left, right| left.name.cmp(&right.name));
                        parsed
                            .worktrees
                            .sort_by(|left, right| left.path.cmp(&right.path));
                        loaded.local_state = Some(parsed);
                    }
                    WorkCapturedSourceKindV1::Campaigns => {
                        let mut parsed: WorkCapturedCampaignsV1 = load_captured_body!(
                            &body,
                            WorkCapturedCampaignsV1,
                            WORK_CAMPAIGNS_SCHEMA_VERSION,
                            "work campaigns"
                        );
                        parsed
                            .campaigns
                            .sort_by(|left, right| left.id.cmp(&right.id));
                        loaded.campaigns = Some(parsed);
                    }
                    WorkCapturedSourceKindV1::Surfaces => {
                        let mut parsed: WorkCapturedSurfacesV1 = load_captured_body!(
                            &body,
                            WorkCapturedSurfacesV1,
                            WORK_SURFACES_SCHEMA_VERSION,
                            "work surfaces"
                        );
                        parsed
                            .surfaces
                            .sort_by(|left, right| left.id.cmp(&right.id));
                        loaded.surfaces = Some(parsed);
                    }
                    WorkCapturedSourceKindV1::CargoAllow => {
                        let mut parsed: WorkCapturedCargoAllowV1 = load_captured_body!(
                            &body,
                            WorkCapturedCargoAllowV1,
                            WORK_CARGO_ALLOW_SCHEMA_VERSION,
                            "work cargo-allow graph"
                        );
                        parsed
                            .requirements
                            .sort_by(|left, right| left.id.cmp(&right.id));
                        for requirement in &mut parsed.requirements {
                            requirement
                                .slices
                                .sort_by(|left, right| left.id.cmp(&right.id));
                        }
                        loaded.cargo_allow = Some(parsed);
                    }
                }
            }
            WorkCapturedSourceStateV1::Missing | WorkCapturedSourceStateV1::Unavailable => {
                if present {
                    return Err(format!(
                        "manifest marks `{}` {} but {} is present",
                        kind.wire_name(),
                        source_label(state),
                        file_name
                    ));
                }
            }
        }
    }
    Ok(loaded)
}

// ---------------------------------------------------------------------------
// Output DTOs (the versioned portfolio projection).
// ---------------------------------------------------------------------------

/// Freshness of one source observation, derived deterministically from the
/// manifest states and timestamps.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkSourceFreshnessV1 {
    Current,
    Stale,
    Unknown,
}

/// One rendered source observation. Volatile fields stay out of the
/// portable identity digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSourceObservationV1 {
    pub source: String,
    pub state: String,
    pub freshness: WorkSourceFreshnessV1,
    pub observed_at: Option<String>,
    pub request_id: Option<String>,
    pub partial: bool,
    pub note: Option<String>,
}

/// Repository and default-branch identity. The volatile checkout root
/// always renders as the literal `<root>` token.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkRepositoryIdentityV1 {
    pub repository: String,
    pub default_branch: String,
    pub default_branch_sha: String,
    pub root: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCampaignV1 {
    pub id: String,
    pub title: String,
    pub state: String,
    pub surfaces: Vec<String>,
    pub issues: Vec<u64>,
    pub candidates: u64,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkIssueV1 {
    pub number: u64,
    pub identity: String,
    pub title: String,
    pub state: String,
    pub lifecycle_stage: IssueLifecycleDispositionV1,
    pub campaigns: Vec<String>,
    pub pull_requests: Vec<u64>,
    pub claims: Vec<String>,
    pub blocked_by: Vec<WorkDependencyV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkPullRequestV1 {
    pub number: u64,
    pub identity: String,
    pub title: String,
    pub state: String,
    pub draft: bool,
    pub head_branch: String,
    pub base_branch: String,
    pub linked_issues: Vec<u64>,
    pub claim: Option<String>,
    pub review_state: String,
    pub unresolved_review_findings: u64,
    pub checks_state: String,
    pub worktree: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkClaimV1 {
    pub id: String,
    pub issue: Option<u64>,
    pub branch: String,
    pub worktree: Option<String>,
    pub exclusive: bool,
    pub state: String,
    pub colliding: bool,
}

/// One dependency or blocker edge on a candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkDependencyV1 {
    pub kind: String,
    pub reference: String,
    pub description: String,
}

/// Lane/capacity state. A lane is occupied by an active claim, conflicting
/// when a collision edge touches it, and `unknown` when local truth is
/// missing or stale.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkLaneStateV1 {
    Available,
    Occupied,
    Conflicting,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkLaneV1 {
    pub lane: String,
    pub state: WorkLaneStateV1,
    pub owner: Option<String>,
    pub collision_edge: Option<String>,
}

/// Closed conflict-edge vocabulary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkConflictEdgeKindV1 {
    DuplicateFamily,
    ClaimCollision,
    BranchCollision,
    SharedContract,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkConflictEdgeV1 {
    pub id: String,
    pub kind: WorkConflictEdgeKindV1,
    pub subjects: Vec<String>,
    pub evidence: Vec<String>,
    pub note: String,
}

/// Closed candidate-kind vocabulary from #1704.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkCandidateKindV1 {
    ResearchIssue,
    ChallengeContract,
    CompilePlan,
    ResumePr,
    RepairReview,
    VerifyCurrentHead,
    MergeReady,
    ReconcileCloseout,
    StartBuild,
    Blocked,
    Complete,
}

impl WorkCandidateKindV1 {
    /// Stable snake_case wire name, matching the serde `snake_case`
    /// representation; shared identity string forms derive from this so the
    /// wire vocabulary has one owner.
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::ResearchIssue => "research_issue",
            Self::ChallengeContract => "challenge_contract",
            Self::CompilePlan => "compile_plan",
            Self::ResumePr => "resume_pr",
            Self::RepairReview => "repair_review",
            Self::VerifyCurrentHead => "verify_current_head",
            Self::MergeReady => "merge_ready",
            Self::ReconcileCloseout => "reconcile_closeout",
            Self::StartBuild => "start_build",
            Self::Blocked => "blocked",
            Self::Complete => "complete",
        }
    }
}

/// The eight explicit ranking factors from #1704, in exactly that order.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkRankingFactorKindV1 {
    UserRootDirectionAndOwnedResumableWork,
    NearMergeWorkWithUnresolvedBoundedResidue,
    StructuralBlockersAndDependencyFanOut,
    UserFacingCorrectnessHonestyRisk,
    AcceptedContractReadinessAndEvidenceCompleteness,
    LaneCapacityAndCollisionCost,
    ReviewCiMergeCostAndSaturation,
    StalenessAndObservationConfidence,
}

impl WorkRankingFactorKindV1 {
    pub(crate) fn order(self) -> u8 {
        match self {
            Self::UserRootDirectionAndOwnedResumableWork => 1,
            Self::NearMergeWorkWithUnresolvedBoundedResidue => 2,
            Self::StructuralBlockersAndDependencyFanOut => 3,
            Self::UserFacingCorrectnessHonestyRisk => 4,
            Self::AcceptedContractReadinessAndEvidenceCompleteness => 5,
            Self::LaneCapacityAndCollisionCost => 6,
            Self::ReviewCiMergeCostAndSaturation => 7,
            Self::StalenessAndObservationConfidence => 8,
        }
    }
}

/// One inspectable ranking-factor contribution. No numeric weight exists:
/// the contribution names the captured fact and its source references.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkRankingFactorV1 {
    pub order: u8,
    pub factor: WorkRankingFactorKindV1,
    pub contribution: String,
    pub sources: Vec<String>,
}

/// Candidate confidence: `partial` or `not_proven` whenever retained inputs
/// cannot support full confidence; never fabricated.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkConfidenceV1 {
    Complete,
    Partial,
    NotProven,
}

/// Explicitly sourced advisory readiness; never derived, never gate-shaped.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkReadinessV1 {
    pub source: String,
    pub advisory: bool,
    pub contribution: String,
}

/// One work candidate. The complete set is preserved; ordering is the
/// lexicographic application of the eight declared ranking factors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCandidateV1 {
    pub candidate_id: String,
    pub kind: WorkCandidateKindV1,
    pub issue: u64,
    pub issue_identity: String,
    pub campaigns: Vec<String>,
    pub accepted_contracts: Vec<String>,
    pub lifecycle_stage: IssueLifecycleDispositionV1,
    pub next_transition: String,
    pub pull_request: Option<u64>,
    pub branch: Option<String>,
    pub claim: Option<String>,
    pub worktree: Option<String>,
    pub dependencies: Vec<WorkDependencyV1>,
    pub conflict_edges: Vec<String>,
    pub conflict_resources: Vec<String>,
    pub overlapping_candidates: Vec<String>,
    pub proof_cost_class: WorkCostClassV1,
    pub review_ci_cost_class: WorkCostClassV1,
    pub lane: String,
    pub lane_state: WorkLaneStateV1,
    pub readiness: Option<WorkReadinessV1>,
    pub regression_risks: Vec<String>,
    pub ranking_factors: Vec<WorkRankingFactorV1>,
    pub confidence: WorkConfidenceV1,
    pub confidence_reasons: Vec<String>,
    pub single_agent_preferred: bool,
    pub duplicate_family: Option<String>,
    pub packet_entrypoint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkSelectionCountsV1 {
    pub total: u64,
    pub selected: u64,
    pub omitted: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkPortfolioCountsV1 {
    pub campaigns: u64,
    pub issues: u64,
    pub pull_requests: u64,
    pub claims: u64,
    pub lanes: u64,
    pub conflict_edges: u64,
    pub candidates: WorkSelectionCountsV1,
}

/// The versioned point-in-time portfolio snapshot.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkPortfolioSnapshotV1 {
    pub schema_version: String,
    pub repository: WorkRepositoryIdentityV1,
    pub captured_inputs: Vec<String>,
    pub source_observations: Vec<WorkSourceObservationV1>,
    pub campaigns: Vec<WorkCampaignV1>,
    pub issues: Vec<WorkIssueV1>,
    pub pull_requests: Vec<WorkPullRequestV1>,
    pub claims: Vec<WorkClaimV1>,
    pub lanes: Vec<WorkLaneV1>,
    pub conflict_edges: Vec<WorkConflictEdgeV1>,
    pub candidates: Vec<WorkCandidateV1>,
    pub counts: WorkPortfolioCountsV1,
    pub retrieval_commands: Vec<String>,
    pub partial_data_boundaries: Vec<String>,
    pub claim_boundary: String,
    pub portable_identity: String,
}

/// The bounded candidates view. Filters narrow the rendered rows only;
/// candidate identity, classification, ranking and authority never change.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkCandidatesViewV1 {
    pub schema_version: String,
    pub portfolio_identity: String,
    pub filter_campaign: Option<String>,
    pub filter_surface: Option<String>,
    pub limit: u64,
    pub counts: WorkSelectionCountsV1,
    pub retrieval_commands: Vec<String>,
    pub candidates: Vec<WorkCandidateV1>,
    pub partial_data_boundaries: Vec<String>,
    pub claim_boundary: String,
}

/// The single-candidate explain view.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkExplainViewV1 {
    pub schema_version: String,
    pub portfolio_identity: String,
    pub candidate: WorkCandidateV1,
    pub partial_data_boundaries: Vec<String>,
    pub claim_boundary: String,
}

// ---------------------------------------------------------------------------
// Portable path and identity helpers.
// ---------------------------------------------------------------------------

fn slash_normalize(path: &str) -> String {
    path.replace('\\', "/")
}

/// Relativize one absolute captured path against the volatile root spelling
/// so equivalent Windows/Unix roots render identically.
pub(crate) fn portable_path(path: &str, root: &str) -> String {
    let normalized = slash_normalize(path);
    let normalized_root = slash_normalize(root).trim_end_matches('/').to_string();
    if normalized == normalized_root {
        return "<root>".to_string();
    }
    let prefix = format!("{normalized_root}/");
    if let Some(rest) = normalized.strip_prefix(&prefix) {
        return format!("<root>/{rest}");
    }
    normalized
}

/// Recursively strip volatile keys before digesting, so portable identity
/// never binds timestamps or request ids.
fn strip_volatile(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for key in VOLATILE_IDENTITY_KEYS {
                map.remove(key);
            }
            for child in map.values_mut() {
                strip_volatile(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_volatile(item);
            }
        }
        _ => {}
    }
}

/// Digest one DTO into a portable identity: serialize, strip volatile keys,
/// hash. `serde_json` maps serialize in sorted-key order, so the digest is
/// byte-stable for a fixed DTO.
pub(crate) fn portable_identity<T: Serialize>(label: &str, dto: &T) -> Result<String, String> {
    let mut value =
        serde_json::to_value(dto).map_err(|error| format!("serialize {label}: {error}"))?;
    strip_volatile(&mut value);
    let canonical =
        serde_json::to_string(&value).map_err(|error| format!("canonicalize {label}: {error}"))?;
    Ok(format!(
        "work-portfolio:sha256:{}",
        crate::blind_journey::sha256_hex(canonical.as_bytes())
    ))
}

/// Deterministic pretty JSON with a trailing newline.
pub(crate) fn work_portfolio_json<T: Serialize>(dto: &T) -> Result<String, String> {
    let mut body = serde_json::to_string_pretty(dto)
        .map_err(|error| format!("serialize work portfolio output: {error}"))?;
    body.push('\n');
    Ok(body)
}

// ---------------------------------------------------------------------------
// Compiler: captured inputs -> WorkPortfolioSnapshotV1. Pure and read-only.
// ---------------------------------------------------------------------------

struct WorkCompileContext {
    freshness: BTreeMap<WorkCapturedSourceKindV1, WorkSourceFreshnessV1>,
}

impl WorkCompileContext {
    fn fresh(&self, kind: WorkCapturedSourceKindV1) -> bool {
        self.freshness
            .get(&kind)
            .is_some_and(|value| *value == WorkSourceFreshnessV1::Current)
    }

    fn issue_source_confidence(&self) -> Option<WorkConfidenceV1> {
        match self.freshness.get(&WorkCapturedSourceKindV1::GithubIssues) {
            Some(WorkSourceFreshnessV1::Current) | None => None,
            Some(WorkSourceFreshnessV1::Stale) => Some(WorkConfidenceV1::Partial),
            Some(WorkSourceFreshnessV1::Unknown) => Some(WorkConfidenceV1::NotProven),
        }
    }
}

/// Parse an RFC 3339 timestamp into UTC epoch nanoseconds. Accepts the
/// full profile (`YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`); any deviation
/// returns `None` so callers fail closed instead of trusting a malformed
/// stamp.
fn parse_rfc3339_utc(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: u32 = text.get(5..7)?.parse().ok()?;
    let day: u32 = text.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if bytes.get(10) != Some(&b'T') || bytes.get(13) != Some(&b':') || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let hour: i64 = text.get(11..13)?.parse().ok()?;
    let minute: i64 = text.get(14..16)?.parse().ok()?;
    let second: i64 = text.get(17..19)?.parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut index = 19;
    let mut nanos: i64 = 0;
    let mut fraction_digits = 0;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
            if fraction_digits < 9 {
                nanos = nanos * 10 + i64::from(*bytes.get(index)? - b'0');
                fraction_digits += 1;
            }
            index += 1;
        }
        if index == start {
            return None;
        }
        for _ in fraction_digits..9 {
            nanos *= 10;
        }
    }
    let offset_seconds = match bytes.get(index) {
        Some(b'Z') if index + 1 == bytes.len() => 0,
        Some(sign @ (b'+' | b'-')) if index + 6 == bytes.len() => {
            if bytes.get(index + 3) != Some(&b':') {
                return None;
            }
            let offset_hour: i64 = text.get(index + 1..index + 3)?.parse().ok()?;
            let offset_minute: i64 = text.get(index + 4..index + 6)?.parse().ok()?;
            if offset_hour > 23 || offset_minute > 59 {
                return None;
            }
            let magnitude = offset_hour * 3600 + offset_minute * 60;
            if *sign == b'-' { -magnitude } else { magnitude }
        }
        _ => return None,
    };
    let era = if month <= 2 { year - 1 } else { year };
    let epoch_year = if era >= 0 { era } else { era - 399 } / 400;
    let year_of_era = era - epoch_year * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = epoch_year * 146097 + day_of_era - 719468;
    let seconds = days * 86400 + hour * 3600 + minute * 60 + second - offset_seconds;
    seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
}

fn build_observations(
    manifest: &WorkCapturedManifestV1,
) -> Result<
    (
        Vec<WorkSourceObservationV1>,
        BTreeMap<WorkCapturedSourceKindV1, WorkSourceFreshnessV1>,
    ),
    String,
> {
    let mut observations = Vec::new();
    let mut freshness = BTreeMap::new();
    let captured_at = parse_rfc3339_utc(&manifest.captured_at);
    for kind in WorkCapturedSourceKindV1::all() {
        let Some(source) = manifest.sources.iter().find(|entry| entry.source == kind) else {
            return Err(format!("manifest is missing source `{}`", kind.wire_name()));
        };
        let value = match source.state {
            WorkCapturedSourceStateV1::Observed => {
                let current = match (source.observed_at.as_deref(), captured_at) {
                    (Some(observed_at), Some(captured)) => {
                        parse_rfc3339_utc(observed_at).is_some_and(|observed| observed >= captured)
                    }
                    _ => false,
                };
                if current {
                    WorkSourceFreshnessV1::Current
                } else {
                    WorkSourceFreshnessV1::Stale
                }
            }
            WorkCapturedSourceStateV1::Stale => WorkSourceFreshnessV1::Stale,
            WorkCapturedSourceStateV1::Missing | WorkCapturedSourceStateV1::Unavailable => {
                WorkSourceFreshnessV1::Unknown
            }
        };
        freshness.insert(kind, value);
        observations.push(WorkSourceObservationV1 {
            source: kind.wire_name().to_string(),
            state: format!("{:?}", source.state).to_ascii_lowercase(),
            freshness: value,
            observed_at: source.observed_at.clone(),
            request_id: source.request_id.clone(),
            partial: value != WorkSourceFreshnessV1::Current,
            note: source.note.clone(),
        });
    }
    Ok((observations, freshness))
}

fn next_transition_for_open_pr(pr: &WorkCapturedPullRequestV1) -> String {
    if pr.checks_state == "pending" {
        return "wait for the required checks to report, then rerun the proof commands on the published head".to_string();
    }
    if pr.checks_state == "failure" {
        return "inspect the failing checks, push a bounded fix, and rerun the proof commands"
            .to_string();
    }
    if pr.draft {
        return "finish the draft, mark the PR ready for review, and request review".to_string();
    }
    "advance the open PR through review and checks without opening parallel work".to_string()
}

/// Classify one campaign issue into the closed candidate-kind vocabulary and
/// its exact next durable transition. In-flight states (resume/repair/
/// merge-ready) always win over new-build kinds, so an existing PR or claim
/// never produces a duplicate build candidate. `merge_ready` additionally
/// requires fresh PR evidence and no unresolved claim collision, so stale
/// approvals and unarbitrated claims can never present as mergeable.
#[allow(
    clippy::too_many_arguments,
    reason = "classifier needs every captured evidence slice to decide the candidate kind"
)]
fn classify_candidate(
    issue: &WorkCapturedIssueV1,
    open_prs: &[&WorkCapturedPullRequestV1],
    merged_prs: &[&WorkCapturedPullRequestV1],
    active_claims: &[&WorkCapturedClaimV1],
    has_claim_collision: bool,
    has_unregistered_branch_collision: bool,
    has_slice: bool,
    pull_requests_fresh: bool,
    current_main: &str,
) -> (WorkCandidateKindV1, String) {
    if open_prs.len() > 1 {
        return (
            WorkCandidateKindV1::VerifyCurrentHead,
            "multiple open PRs are linked to this issue; root resolves which PR is current before any merge, repair or closeout proceeds on it".to_string(),
        );
    }
    if let Some(pr) = open_prs.first() {
        if pr.review_state == "changes_requested" || pr.unresolved_review_findings > 0 {
            return (
                WorkCandidateKindV1::RepairReview,
                format!(
                    "address each unresolved review finding on PR #{}, reply substantively, resolve the thread, then re-request review",
                    pr.number
                ),
            );
        }
        if !pr.draft && pr.review_state == "approved" && pr.checks_state == "success" {
            if !pull_requests_fresh {
                return (
                    WorkCandidateKindV1::VerifyCurrentHead,
                    "the captured approval and checks may predate the current published head; re-establish REVIEW_READY on the current head before any merge".to_string(),
                );
            }
            if has_claim_collision {
                return (
                    WorkCandidateKindV1::Blocked,
                    "root arbitrates the colliding durable claims on the issue and records the decision before any merge".to_string(),
                );
            }
            return (
                WorkCandidateKindV1::MergeReady,
                "root performs the normal protected squash merge; no admin bypass and no merge from the portfolio".to_string(),
            );
        }
        return (
            WorkCandidateKindV1::ResumePr,
            next_transition_for_open_pr(pr),
        );
    }
    if issue
        .lifecycle_disposition
        .is_some_and(|disposition| disposition == IssueLifecycleDispositionV1::Completed)
    {
        return (
            WorkCandidateKindV1::Complete,
            "no transition: the issue is complete and stays visible in the portfolio".to_string(),
        );
    }
    if !merged_prs.is_empty() {
        if issue.state == "closed" {
            return (
                WorkCandidateKindV1::ReconcileCloseout,
                format!(
                    "reconcile the closed issue against merged PR #{} at main head {}: confirm closeout state, records and residuals before archive",
                    merged_prs[0].number, current_main
                ),
            );
        }
        return (
            WorkCandidateKindV1::VerifyCurrentHead,
            format!(
                "verify the merged change from PR #{} against current main head {} before closeout",
                merged_prs[0].number, current_main
            ),
        );
    }
    if !issue.blocked_by.is_empty() {
        let external = issue
            .blocked_by
            .iter()
            .find(|blocker| blocker.kind == "external_dependency")
            .map(|blocker| blocker.reference.clone())
            .unwrap_or_else(|| issue.blocked_by[0].reference.clone());
        return (
            WorkCandidateKindV1::Blocked,
            format!(
                "wait for the named authority `{external}`; no local build may start until the blocker clears"
            ),
        );
    }
    if has_claim_collision {
        return (
            WorkCandidateKindV1::Blocked,
            "root arbitrates the colliding durable claims on the issue and records the decision before any build resumes".to_string(),
        );
    }
    if has_unregistered_branch_collision {
        return (
            WorkCandidateKindV1::Blocked,
            "register a durable claim for the existing branch or have root reassign the lane before building".to_string(),
        );
    }
    if active_claims.len() == 1 {
        return (
            WorkCandidateKindV1::ResumePr,
            format!(
                "push the claimed branch `{}` and open the PR under the existing claim",
                active_claims[0].branch
            ),
        );
    }
    match issue.contract_state.as_str() {
        "accepted" if has_slice => (
            WorkCandidateKindV1::StartBuild,
            "open a scoped branch under one exclusive claim, implement the accepted slice, push, open the PR, and run the recorded proof commands".to_string(),
        ),
        "accepted" => (
            WorkCandidateKindV1::CompilePlan,
            "draft the implementation plan first: work items, resolving dependency edges and acceptance coverage".to_string(),
        ),
        "challenged" | "draft" => (
            WorkCandidateKindV1::ChallengeContract,
            "settle the contested contract through the spec route, then recompile the portfolio".to_string(),
        ),
        _ => (
            WorkCandidateKindV1::ResearchIssue,
            "scope the issue and draft an accepted contract before any build is considered".to_string(),
        ),
    }
}

/// The captured blocker shape is identical to the rendered dependency shape;
/// convert explicitly so the two DTOs stay distinct types.
fn dependency_from_blocker(blocker: &WorkCapturedBlockerV1) -> WorkDependencyV1 {
    WorkDependencyV1 {
        kind: blocker.kind.clone(),
        reference: blocker.reference.clone(),
        description: blocker.description.clone(),
    }
}

fn cost_class(cost: Option<WorkCostClassV1>) -> WorkCostClassV1 {
    cost.unwrap_or(WorkCostClassV1::NotProven)
}

fn honesty_risk_rank(risk: Option<&str>) -> (i64, String, Vec<String>) {
    match risk {
        Some("high") => (
            0,
            "user-facing correctness/honesty risk sourced as high; factor 4 prioritizes it".to_string(),
            vec!["issue metadata `honesty_risk=high`".to_string()],
        ),
        Some("moderate") => (
            1,
            "user-facing correctness/honesty risk sourced as moderate".to_string(),
            vec!["issue metadata `honesty_risk=moderate`".to_string()],
        ),
        Some("low") => (
            2,
            "user-facing correctness/honesty risk sourced as low".to_string(),
            vec!["issue metadata `honesty_risk=low`".to_string()],
        ),
        _ => (
            3,
            "no user-facing correctness/honesty risk sourced; factor 4 treats it as unknown, never assumed low".to_string(),
            Vec::new(),
        ),
    }
}

fn cost_rank(cost: WorkCostClassV1) -> i64 {
    match cost {
        WorkCostClassV1::Low => 0,
        WorkCostClassV1::Moderate => 1,
        WorkCostClassV1::High => 2,
        WorkCostClassV1::NotProven => 3,
    }
}

fn confidence_rank(confidence: WorkConfidenceV1) -> i64 {
    match confidence {
        WorkConfidenceV1::Complete => 0,
        WorkConfidenceV1::Partial => 1,
        WorkConfidenceV1::NotProven => 2,
    }
}

fn lane_rank(lane_state: WorkLaneStateV1) -> i64 {
    match lane_state {
        WorkLaneStateV1::Available => 0,
        WorkLaneStateV1::Occupied => 1,
        WorkLaneStateV1::Conflicting => 2,
        WorkLaneStateV1::Unknown => 3,
    }
}

/// The lexicographic ranking tuple: one entry per declared factor, in the
/// #1704 order. Smaller is earlier. Never serialized — the factors on the
/// candidate are the inspectable record of exactly these entries.
fn ranking_tuple(
    candidate: &WorkCandidateV1,
    honesty_risk: Option<&str>,
    has_open_pr: bool,
) -> [i64; 8] {
    // Factor 1 credits already-owned resumable work: an open PR or a durable
    // active claim. A merged PR alone (verify_current_head / complete /
    // reconcile_closeout) is finished work, not resumable work.
    let factor_one = if has_open_pr || candidate.claim.is_some() {
        0
    } else {
        1
    };
    let factor_two = match candidate.kind {
        WorkCandidateKindV1::MergeReady | WorkCandidateKindV1::RepairReview => 0,
        _ => 1,
    };
    let factor_three = candidate.dependencies.len() as i64;
    let factor_four = honesty_risk_rank(honesty_risk).0;
    let factor_five = if candidate.accepted_contracts.is_empty() {
        2
    } else if candidate.confidence == WorkConfidenceV1::Complete {
        0
    } else {
        1
    };
    let factor_six = lane_rank(candidate.lane_state);
    let factor_seven = cost_rank(candidate.review_ci_cost_class);
    let factor_eight = confidence_rank(candidate.confidence);
    [
        factor_one,
        factor_two,
        factor_three,
        factor_four,
        factor_five,
        factor_six,
        factor_seven,
        factor_eight,
    ]
}

fn build_ranking_factors(
    issue: &WorkCapturedIssueV1,
    open_pr: Option<&WorkCapturedPullRequestV1>,
    candidate: &WorkCandidateV1,
) -> Vec<WorkRankingFactorV1> {
    let resumable = open_pr.is_some() || candidate.claim.is_some();
    let mut factors = Vec::new();
    let mut push = |factor: WorkRankingFactorKindV1, contribution: String, sources: Vec<String>| {
        factors.push(WorkRankingFactorV1 {
            order: factor.order(),
            factor,
            contribution,
            sources,
        });
    };
    if resumable {
        let (basis, sources): (String, Vec<String>) =
            match (&candidate.pull_request, &candidate.claim) {
                (Some(pr), Some(claim)) => (
                    format!("candidate resumes already-owned work: PR #{pr} under claim `{claim}`"),
                    vec![format!("pr:{pr}"), format!("claim:{claim}")],
                ),
                (Some(pr), None) => (
                    format!("candidate resumes already-owned work: open PR #{pr}"),
                    vec![format!("pr:{pr}")],
                ),
                (None, Some(claim)) => (
                    format!("candidate resumes already-owned work under durable claim `{claim}`"),
                    vec![format!("claim:{claim}")],
                ),
                (None, None) => (
                    "candidate resumes already-owned work".to_string(),
                    Vec::new(),
                ),
            };
        push(
            WorkRankingFactorKindV1::UserRootDirectionAndOwnedResumableWork,
            format!(
                "{basis}; factor 1 prioritizes user/root direction and already-owned resumable work"
            ),
            sources,
        );
    } else {
        push(
            WorkRankingFactorKindV1::UserRootDirectionAndOwnedResumableWork,
            "no already-owned resumable work sourced for this issue; factor 1 contributes nothing beyond campaign membership".to_string(),
            Vec::new(),
        );
    }
    match candidate.kind {
        WorkCandidateKindV1::MergeReady => push(
            WorkRankingFactorKindV1::NearMergeWorkWithUnresolvedBoundedResidue,
            "exact near-merge work: the PR is approved with successful checks; only the protected merge remains".to_string(),
            candidate
                .pull_request
                .map(|pr| vec![format!("pr:{pr}")])
                .unwrap_or_default(),
        ),
        WorkCandidateKindV1::RepairReview => {
            let findings = open_pr
                .map(|pr| pr.unresolved_review_findings)
                .unwrap_or(0);
            push(
                WorkRankingFactorKindV1::NearMergeWorkWithUnresolvedBoundedResidue,
                format!(
                    "exact near-merge work with unresolved bounded residue: {findings} review finding(s) on the open PR"
                ),
                candidate
                    .pull_request
                    .map(|pr| vec![format!("pr:{pr}")])
                    .unwrap_or_default(),
            );
        }
        _ => push(
            WorkRankingFactorKindV1::NearMergeWorkWithUnresolvedBoundedResidue,
            "no near-merge work with bounded residue; factor 2 contributes nothing".to_string(),
            Vec::new(),
        ),
    }
    if candidate.dependencies.is_empty() {
        push(
            WorkRankingFactorKindV1::StructuralBlockersAndDependencyFanOut,
            "no structural blockers or open dependency fan-out sourced".to_string(),
            Vec::new(),
        );
    } else {
        push(
            WorkRankingFactorKindV1::StructuralBlockersAndDependencyFanOut,
            format!(
                "{} structural blocker(s)/dependency edge(s) sourced; factor 3 deprioritizes blocked work",
                candidate.dependencies.len()
            ),
            candidate
                .dependencies
                .iter()
                .map(|dependency| dependency.reference.clone())
                .collect(),
        );
    }
    let (_risk_rank, risk_contribution, risk_sources) =
        honesty_risk_rank(issue.honesty_risk.as_deref());
    push(
        WorkRankingFactorKindV1::UserFacingCorrectnessHonestyRisk,
        risk_contribution,
        risk_sources,
    );
    if candidate.accepted_contracts.is_empty() {
        push(
            WorkRankingFactorKindV1::AcceptedContractReadinessAndEvidenceCompleteness,
            "no accepted contract sourced; factor 5 cannot credit contract readiness".to_string(),
            Vec::new(),
        );
    } else {
        push(
            WorkRankingFactorKindV1::AcceptedContractReadinessAndEvidenceCompleteness,
            format!(
                "accepted contract(s) {} sourced with {} evidence confidence",
                candidate.accepted_contracts.join(", "),
                wire_label(&candidate.confidence)
            ),
            candidate.accepted_contracts.clone(),
        );
    }
    let lane_contribution = if issue.single_agent_preferred {
        format!(
            "single-agent-preferred: orchestration cost dominates, so one agent owns lane `{}` and parallel assignment would cost more than the task",
            candidate.lane
        )
    } else {
        match candidate.lane_state {
            WorkLaneStateV1::Available => {
                "lane capacity available; no collision cost sourced".to_string()
            }
            WorkLaneStateV1::Occupied => {
                "lane occupied by an active claim; parallel work would collide".to_string()
            }
            WorkLaneStateV1::Conflicting => {
                "lane in collision; parallelism is reduced until root arbitrates".to_string()
            }
            WorkLaneStateV1::Unknown => {
                "lane state unknown because local truth is missing or stale".to_string()
            }
        }
    };
    push(
        WorkRankingFactorKindV1::LaneCapacityAndCollisionCost,
        lane_contribution,
        if candidate.claim.is_some() {
            vec![format!(
                "claim:{}",
                candidate.claim.clone().unwrap_or_default()
            )]
        } else {
            Vec::new()
        },
    );
    push(
        WorkRankingFactorKindV1::ReviewCiMergeCostAndSaturation,
        match candidate.review_ci_cost_class {
            WorkCostClassV1::Low => "review/CI/merge cost sourced as low".to_string(),
            WorkCostClassV1::Moderate => "review/CI/merge cost sourced as moderate".to_string(),
            WorkCostClassV1::High => {
                "review/CI/merge cost sourced as high; factor 7 deprioritizes saturated lanes"
                    .to_string()
            }
            WorkCostClassV1::NotProven => {
                "no review/CI/merge cost sourced; factor 7 reads not_proven, never assumed low"
                    .to_string()
            }
        },
        Vec::new(),
    );
    push(
        WorkRankingFactorKindV1::StalenessAndObservationConfidence,
        match candidate.confidence {
            WorkConfidenceV1::Complete => {
                "all sources behind this candidate are current and observed".to_string()
            }
            WorkConfidenceV1::Partial => format!(
                "confidence partial: {}",
                candidate.confidence_reasons.join("; ")
            ),
            WorkConfidenceV1::NotProven => format!(
                "confidence not_proven: {}",
                candidate.confidence_reasons.join("; ")
            ),
        },
        Vec::new(),
    );
    factors.sort_by_key(|factor| factor.order);
    factors
}

fn issues_by_campaign(captured: &WorkCapturedDirV1) -> BTreeMap<String, Vec<u64>> {
    let mut map: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    if let Some(campaigns) = &captured.campaigns {
        for campaign in &campaigns.campaigns {
            for issue in &campaign.issues {
                map.entry(campaign.id.clone()).or_default().push(*issue);
            }
        }
    }
    for issues in map.values_mut() {
        issues.sort_unstable();
        issues.dedup();
    }
    map
}

/// A slice only proves build readiness when it sits under a requirement the
/// issue structurally references; a stale cross-reference into another
/// requirement is incomplete evidence and must not promote the candidate.
fn issue_has_accepted_slice(graph: &WorkCapturedCargoAllowV1, issue: &WorkCapturedIssueV1) -> bool {
    graph.requirements.iter().any(|requirement| {
        issue.requirement_refs.contains(&requirement.id)
            && requirement
                .slices
                .iter()
                .any(|slice| slice.issue_refs.contains(&issue.number))
    })
}

/// Classify one captured issue with the same candidate-kind law the compiler
/// applies to portfolio candidates, so identity checks for issues no
/// campaign records (#1706 standalone work, RIPR-SPEC-0235) never fork the
/// action taxonomy. Collision marks are compiler-internal edge derivations;
/// a standalone issue is classified without them, and its claim/branch
/// collisions stay visible through the selection law's overlap checks
/// instead of being re-derived here.
pub(crate) fn classify_captured_issue(
    captured: &WorkCapturedDirV1,
    issue: &WorkCapturedIssueV1,
) -> WorkCandidateKindV1 {
    let empty_prs = Vec::new();
    let pull_requests: &[WorkCapturedPullRequestV1] = captured
        .pull_requests
        .as_ref()
        .map(|body| body.pull_requests.as_slice())
        .unwrap_or(&empty_prs);
    let empty_claims = Vec::new();
    let claims: &[WorkCapturedClaimV1] = captured
        .claims
        .as_ref()
        .map(|body| body.claims.as_slice())
        .unwrap_or(&empty_claims);
    let linked_prs: Vec<&WorkCapturedPullRequestV1> = pull_requests
        .iter()
        .filter(|pr| pr.linked_issues.contains(&issue.number))
        .collect();
    let open_prs: Vec<&WorkCapturedPullRequestV1> = linked_prs
        .iter()
        .copied()
        .filter(|pr| pr.state == "open")
        .collect();
    let merged_prs: Vec<&WorkCapturedPullRequestV1> = linked_prs
        .iter()
        .copied()
        .filter(|pr| pr.state == "merged")
        .collect();
    let issue_claims: Vec<&WorkCapturedClaimV1> = claims
        .iter()
        .filter(|claim| claim.issue == Some(issue.number) && claim.state == "active")
        .collect();
    let has_slice = captured
        .cargo_allow
        .as_ref()
        .is_some_and(|graph| issue_has_accepted_slice(graph, issue));
    let freshness = build_observations(&captured.manifest)
        .map(|(_, freshness)| freshness)
        .unwrap_or_default();
    let context = WorkCompileContext { freshness };
    classify_candidate(
        issue,
        &open_prs,
        &merged_prs,
        &issue_claims,
        false,
        false,
        has_slice,
        context.fresh(WorkCapturedSourceKindV1::GithubPullRequests),
        &captured.manifest.default_branch_sha,
    )
    .0
}

/// A PR's branch is genuinely claimed only when the active claim on that
/// branch belongs to one of the PR's own linked issues; a claim by a
/// different issue on the reused branch is itself the collision the lane
/// must surface.
fn same_issue_branch_claim_exists(
    pr: &WorkCapturedPullRequestV1,
    claims: &[WorkCapturedClaimV1],
) -> bool {
    claims.iter().any(|claim| {
        claim.branch == pr.head_branch
            && claim.state == "active"
            && claim
                .issue
                .is_some_and(|issue| pr.linked_issues.contains(&issue))
    })
}

/// Candidate campaign memberships derive from the campaign records (the
/// same map that decided portfolio inclusion), never from the issue's own
/// list, so the two views cannot contradict each other.
fn campaign_memberships(campaign_map: &BTreeMap<String, Vec<u64>>, issue: u64) -> Vec<String> {
    let mut memberships: Vec<String> = campaign_map
        .iter()
        .filter(|(_, members)| members.contains(&issue))
        .map(|(campaign, _)| campaign.clone())
        .collect();
    memberships.sort();
    memberships.dedup();
    memberships
}

/// Semantically unordered string metadata is canonicalized so input ordering
/// can never change normalized JSON, ranking text or portable identity.
fn canonical_strings(values: &[String]) -> Vec<String> {
    let mut canonical = values.to_vec();
    canonical.sort();
    canonical.dedup();
    canonical
}

/// Semantically unordered blocker metadata is canonicalized by structural
/// identity so reordered captured inputs stay byte-stable.
fn canonical_blockers(values: &[WorkCapturedBlockerV1]) -> Vec<WorkCapturedBlockerV1> {
    let mut canonical = values.to_vec();
    canonical.sort_by(|left, right| {
        (&left.kind, &left.reference, &left.description).cmp(&(
            &right.kind,
            &right.reference,
            &right.description,
        ))
    });
    canonical.dedup_by(|left, right| {
        left.kind == right.kind
            && left.reference == right.reference
            && left.description == right.description
    });
    canonical
}

/// A readiness contribution is only emitted when the captured issue carries
/// acceptance evidence for the named artifact; an unchecked string never
/// manufactures accepted readiness.
fn readiness_for(issue: &WorkCapturedIssueV1) -> Option<WorkReadinessV1> {
    issue
        .readiness_source
        .as_ref()
        .filter(|source| {
            issue.contract_state == "accepted"
                && issue
                    .accepted_contracts
                    .iter()
                    .any(|contract| contract == *source)
        })
        .map(|source| WorkReadinessV1 {
            source: source.clone(),
            advisory: true,
            contribution: format!(
                "readiness contribution sourced from the explicit accepted artifact `{source}`; advisory only and never gate-shaped"
            ),
        })
}

/// The collision edge advertised by a lane must match a member's exact
/// candidate identity; prefix substrings (issues 91 vs 910) must not leak a
/// collision across lanes.
fn lane_collision_edge(edges: &[WorkConflictEdgeV1], members: &[u64]) -> Option<String> {
    edges
        .iter()
        .find(|edge| {
            matches!(
                edge.kind,
                WorkConflictEdgeKindV1::ClaimCollision | WorkConflictEdgeKindV1::BranchCollision
            ) && edge.subjects.iter().any(|subject| {
                members
                    .iter()
                    .any(|member| subject == &format!("candidate:issue:{member}"))
            })
        })
        .map(|edge| edge.id.clone())
}

/// Compile the loaded captured directory into the versioned snapshot. This
/// function is pure: it reads the already-loaded DTOs and mutates nothing.
pub(crate) fn compile_work_portfolio(
    captured: &WorkCapturedDirV1,
    captured_dir: Option<&str>,
) -> Result<WorkPortfolioSnapshotV1, String> {
    let (observations, freshness) = build_observations(&captured.manifest)?;
    let context = WorkCompileContext { freshness };
    let current_main = captured.manifest.default_branch_sha.clone();
    let root = captured.manifest.root.clone();
    let local_root = captured
        .local_state
        .as_ref()
        .map(|state| state.root.clone())
        .unwrap_or_else(|| root.clone());

    let empty_issues = Vec::new();
    let issues: &[WorkCapturedIssueV1] = captured
        .issues
        .as_ref()
        .map(|body| body.issues.as_slice())
        .unwrap_or(&empty_issues);
    let empty_prs = Vec::new();
    let pull_requests: &[WorkCapturedPullRequestV1] = captured
        .pull_requests
        .as_ref()
        .map(|body| body.pull_requests.as_slice())
        .unwrap_or(&empty_prs);
    let empty_claims = Vec::new();
    let claims: &[WorkCapturedClaimV1] = captured
        .claims
        .as_ref()
        .map(|body| body.claims.as_slice())
        .unwrap_or(&empty_claims);

    let campaign_map = issues_by_campaign(captured);
    let mut boundaries: BTreeSet<String> = BTreeSet::new();

    let mut issue_rows: Vec<WorkIssueV1> = Vec::new();
    let mut seen_issues: BTreeSet<u64> = BTreeSet::new();
    let mut campaign_issues: Vec<(String, u64)> = Vec::new();
    for (campaign, members) in &campaign_map {
        for issue in members {
            campaign_issues.push((campaign.clone(), *issue));
        }
    }
    campaign_issues.sort();
    for (_, issue_number) in &campaign_issues {
        if !seen_issues.insert(*issue_number) {
            continue;
        }
        let Some(issue) = issues.iter().find(|entry| entry.number == *issue_number) else {
            boundaries.insert(format!(
                "campaign references issue #{issue_number} that is absent from the captured issue source"
            ));
            continue;
        };
        let issue_prs: Vec<u64> = pull_requests
            .iter()
            .filter(|pr| pr.linked_issues.contains(&issue.number))
            .map(|pr| pr.number)
            .collect();
        let issue_claims: Vec<String> = claims
            .iter()
            .filter(|claim| claim.issue == Some(issue.number))
            .map(|claim| claim.id.clone())
            .collect();
        let mut memberships: Vec<String> = issue.campaigns.clone();
        memberships.sort();
        memberships.dedup();
        issue_rows.push(WorkIssueV1 {
            number: issue.number,
            identity: format!("issue:{}", issue.number),
            title: issue.title.clone(),
            state: issue.state.clone(),
            lifecycle_stage: issue
                .lifecycle_disposition
                .unwrap_or(IssueLifecycleDispositionV1::NotRun),
            campaigns: memberships,
            pull_requests: issue_prs,
            claims: issue_claims,
            blocked_by: issue
                .blocked_by
                .iter()
                .map(dependency_from_blocker)
                .collect(),
        });
    }
    issue_rows.sort_by_key(|issue| issue.number);

    // Pull-request rows: every captured PR linked to at least one portfolio
    // issue; sorted by number.
    let portfolio_issue_numbers: BTreeSet<u64> = seen_issues.clone();
    let mut pr_rows: Vec<WorkPullRequestV1> = Vec::new();
    for pr in pull_requests {
        if !pr
            .linked_issues
            .iter()
            .any(|issue| portfolio_issue_numbers.contains(issue))
        {
            continue;
        }
        let mut linked_issues = pr.linked_issues.clone();
        linked_issues.sort_unstable();
        linked_issues.dedup();
        pr_rows.push(WorkPullRequestV1 {
            number: pr.number,
            identity: format!("pr:{}", pr.number),
            title: pr.title.clone(),
            state: pr.state.clone(),
            draft: pr.draft,
            head_branch: pr.head_branch.clone(),
            base_branch: pr.base_branch.clone(),
            linked_issues,
            claim: pr.registered_claim.clone(),
            review_state: pr.review_state.clone(),
            unresolved_review_findings: pr.unresolved_review_findings,
            checks_state: pr.checks_state.clone(),
            worktree: pr
                .worktree_path
                .as_deref()
                .map(|path| portable_path(path, &local_root)),
        });
    }
    pr_rows.sort_by_key(|pr| pr.number);

    // Conflict edges.
    let mut edges: Vec<WorkConflictEdgeV1> = Vec::new();
    let mut candidate_edge_membership: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut claim_collision_lanes: BTreeSet<String> = BTreeSet::new();
    let mut branch_collision_lanes: BTreeSet<String> = BTreeSet::new();

    // Duplicate families: distinct open campaign issues sharing an accepted
    // requirement reference.
    let mut requirement_issues: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for issue in issues {
        if issue.state != "open" || !portfolio_issue_numbers.contains(&issue.number) {
            continue;
        }
        for requirement in &issue.requirement_refs {
            requirement_issues
                .entry(requirement.clone())
                .or_default()
                .push(issue.number);
        }
    }
    let mut duplicate_family_by_issue: BTreeMap<u64, String> = BTreeMap::new();
    for (requirement, mut members) in requirement_issues {
        members.sort_unstable();
        members.dedup();
        if members.len() < 2 {
            continue;
        }
        // Every member must hold an accepted contract in both paths: a
        // duplicate-family edge never rests on draft or challenged work.
        let accepted: Vec<u64> = members
            .iter()
            .copied()
            .filter(|number| {
                issues
                    .iter()
                    .find(|issue| issue.number == *number)
                    .is_some_and(|issue| issue.contract_state == "accepted")
            })
            .collect();
        if accepted.len() < 2 {
            boundaries.insert(format!(
                "duplicate-family grouping for `{requirement}` withheld: fewer than two member issues hold an accepted contract"
            ));
            continue;
        }
        let requirement_row = captured
            .cargo_allow
            .as_ref()
            .and_then(|graph| graph.requirements.iter().find(|row| row.id == requirement));
        match requirement_row {
            Some(requirement_row) => {
                // The captured authority exists: refuse to group distinct
                // deltas of one requirement as duplicates.
                let has_delta_identity = requirement_row
                    .slices
                    .iter()
                    .any(|slice| !slice.delta_id.is_empty());
                if has_delta_identity {
                    let common: BTreeSet<String> = accepted
                        .iter()
                        .map(|number| {
                            requirement_row
                                .slices
                                .iter()
                                .filter(|slice| slice.issue_refs.contains(number))
                                .map(|slice| slice.delta_id.clone())
                                .collect::<BTreeSet<_>>()
                        })
                        .reduce(|left, right| left.intersection(&right).cloned().collect())
                        .unwrap_or_default();
                    if common.is_empty() {
                        boundaries.insert(format!(
                            "duplicate-family grouping for `{requirement}` withheld: member issues resolve distinct deltas; no shared accepted delta"
                        ));
                        continue;
                    }
                }
            }
            None => {
                // Degraded path: the graph or requirement row is unavailable,
                // so the edge stays visible as advisory evidence with the
                // degraded basis named (never fabricated authority).
                if captured.cargo_allow.is_none() {
                    boundaries.insert(format!(
                        "cargo-allow graph unavailable: duplicate-family evidence for `{requirement}` rests on issue metadata only"
                    ));
                } else {
                    boundaries.insert(format!(
                        "requirement `{requirement}` is absent from the captured cargo-allow graph: duplicate-family evidence rests on issue metadata only"
                    ));
                }
            }
        }
        let members = accepted;
        let edge_id = format!("edge:duplicate_family:{requirement}");
        let mut evidence = vec![format!("shared requirement `{requirement}`")];
        if let Some(requirement_row) = requirement_row {
            for slice in &requirement_row.slices {
                evidence.push(format!(
                    "slice `{}` delta `{}` issues {:?}",
                    slice.id, slice.delta_id, slice.issue_refs
                ));
            }
            for spec in &requirement_row.spec_refs {
                evidence.push(format!("spec `{spec}`"));
            }
        }
        let subjects: Vec<String> = members
            .iter()
            .map(|issue| format!("candidate:issue:{issue}"))
            .collect();
        for issue in &members {
            duplicate_family_by_issue.insert(*issue, edge_id.clone());
        }
        edges.push(WorkConflictEdgeV1 {
            id: edge_id.clone(),
            kind: WorkConflictEdgeKindV1::DuplicateFamily,
            subjects: subjects.clone(),
            evidence,
            note: "distinct issues resolve the same accepted requirement; grouped as a possible duplicate family, never auto-superseded".to_string(),
        });
        for subject in subjects {
            candidate_edge_membership
                .entry(subject)
                .or_default()
                .push(edge_id.clone());
        }
    }

    // Claim collisions: two or more active claims on one issue, or active
    // claims sharing one branch with different identities.
    let active_claims: Vec<&WorkCapturedClaimV1> = claims
        .iter()
        .filter(|claim| claim.state == "active")
        .collect();
    let mut claims_by_issue: BTreeMap<u64, Vec<&WorkCapturedClaimV1>> = BTreeMap::new();
    let mut claims_by_branch: BTreeMap<String, Vec<&WorkCapturedClaimV1>> = BTreeMap::new();
    for claim in &active_claims {
        if let Some(issue) = claim.issue {
            claims_by_issue.entry(issue).or_default().push(*claim);
        }
        claims_by_branch
            .entry(claim.branch.clone())
            .or_default()
            .push(*claim);
    }
    let mut claim_collision_by_issue: BTreeSet<u64> = BTreeSet::new();
    for (issue, mut colliding) in claims_by_issue {
        colliding.sort_by_key(|claim| claim.id.clone());
        if colliding.len() < 2 || !portfolio_issue_numbers.contains(&issue) {
            continue;
        }
        claim_collision_by_issue.insert(issue);
        let edge_id = format!("edge:claim_collision:issue-{issue}");
        let subjects: Vec<String> = colliding
            .iter()
            .map(|claim| format!("claim:{}", claim.id))
            .collect();
        let evidence: Vec<String> = colliding
            .iter()
            .map(|claim| {
                format!(
                    "active {} claim `{}` on branch `{}`",
                    if claim.exclusive {
                        "exclusive"
                    } else {
                        "shared"
                    },
                    claim.id,
                    claim.branch
                )
            })
            .collect();
        candidate_edge_membership
            .entry(format!("candidate:issue:{issue}"))
            .or_default()
            .push(edge_id.clone());
        claim_collision_lanes.insert(lane_name(issues, &campaign_map, issue));
        edges.push(WorkConflictEdgeV1 {
            id: edge_id,
            kind: WorkConflictEdgeKindV1::ClaimCollision,
            subjects,
            evidence,
            note: "durable claim collision: root must arbitrate before any build resumes"
                .to_string(),
        });
    }
    for (branch, mut colliding) in claims_by_branch {
        colliding.sort_by_key(|claim| claim.id.clone());
        let distinct_issues: BTreeSet<Option<u64>> =
            colliding.iter().map(|claim| claim.issue).collect();
        if colliding.len() < 2 || distinct_issues.len() < 2 {
            continue;
        }
        let edge_id = format!("edge:claim_collision:branch-{branch}");
        let subjects: Vec<String> = colliding
            .iter()
            .map(|claim| format!("claim:{}", claim.id))
            .collect();
        let evidence: Vec<String> = colliding
            .iter()
            .map(|claim| format!("claim `{}` holds branch `{branch}`", claim.id))
            .collect();
        edges.push(WorkConflictEdgeV1 {
            id: edge_id.clone(),
            kind: WorkConflictEdgeKindV1::ClaimCollision,
            subjects: subjects.clone(),
            evidence,
            note:
                "two distinct issues hold active claims on one branch; the lane is conflict-visible"
                    .to_string(),
        });
        for claim in &colliding {
            if let Some(issue) = claim.issue
                && portfolio_issue_numbers.contains(&issue)
            {
                candidate_edge_membership
                    .entry(format!("candidate:issue:{issue}"))
                    .or_default()
                    .push(edge_id.clone());
                claim_collision_lanes.insert(lane_name(issues, &campaign_map, issue));
            }
        }
    }

    // Unregistered branch/PR collisions: an open PR whose head branch exists
    // locally but carries no registered claim.
    let local_branches: BTreeSet<String> = captured
        .local_state
        .as_ref()
        .map(|state| {
            state
                .branches
                .iter()
                .map(|branch| branch.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut branch_collision_by_issue: BTreeMap<u64, String> = BTreeMap::new();
    for pr in pull_requests {
        if pr.state != "open" || pr.registered_claim.is_some() {
            continue;
        }
        let touches_portfolio = pr
            .linked_issues
            .iter()
            .any(|issue| portfolio_issue_numbers.contains(issue));
        if !touches_portfolio {
            continue;
        }
        let registered_elsewhere = same_issue_branch_claim_exists(pr, claims);
        if !local_branches.contains(&pr.head_branch) || registered_elsewhere {
            continue;
        }
        let edge_id = format!("edge:branch_collision:pr-{}", pr.number);
        let subjects: Vec<String> = pr
            .linked_issues
            .iter()
            .filter(|issue| portfolio_issue_numbers.contains(issue))
            .map(|issue| format!("candidate:issue:{issue}"))
            .chain(std::iter::once(format!("branch:{}", pr.head_branch)))
            .collect();
        for issue in &pr.linked_issues {
            if portfolio_issue_numbers.contains(issue) {
                branch_collision_by_issue.insert(*issue, edge_id.clone());
                branch_collision_lanes.insert(lane_name(issues, &campaign_map, *issue));
                candidate_edge_membership
                    .entry(format!("candidate:issue:{issue}"))
                    .or_default()
                    .push(edge_id.clone());
            }
        }
        edges.push(WorkConflictEdgeV1 {
            id: edge_id,
            kind: WorkConflictEdgeKindV1::BranchCollision,
            subjects,
            evidence: vec![format!(
                "open PR #{} heads `{}` which exists locally with no registered durable claim",
                pr.number, pr.head_branch
            )],
            note: "unregistered branch/PR collision: parallelism and confidence are reduced until a claim is registered".to_string(),
        });
    }

    // Shared-contract conflicts: path-disjoint candidates sharing a named
    // conflict resource (output contract, golden family, spec ledger,
    // workflow, release asset or mutable report/build root).
    let mut resource_issues: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for issue in issues {
        if issue.state != "open" || !portfolio_issue_numbers.contains(&issue.number) {
            continue;
        }
        for resource in &issue.conflict_resources {
            resource_issues
                .entry(resource.clone())
                .or_default()
                .push(issue.number);
        }
    }
    for (resource, mut members) in resource_issues {
        members.sort_unstable();
        members.dedup();
        if members.len() < 2 {
            continue;
        }
        let path_sets: Vec<BTreeSet<String>> = members
            .iter()
            .map(|issue| {
                issues
                    .iter()
                    .find(|entry| entry.number == *issue)
                    .map(|entry| entry.semantic_paths.iter().cloned().collect())
                    .unwrap_or_default()
            })
            .collect();
        let mut pair_edges = Vec::new();
        for left in 0..members.len() {
            for right in (left + 1)..members.len() {
                if path_sets[left].is_disjoint(&path_sets[right]) {
                    pair_edges.push((members[left], members[right]));
                }
            }
        }
        if pair_edges.is_empty() {
            continue;
        }
        let edge_id = format!("edge:shared_contract:{resource}");
        let subjects: Vec<String> = members
            .iter()
            .map(|issue| format!("candidate:issue:{issue}"))
            .collect();
        let evidence: Vec<String> = pair_edges
            .iter()
            .map(|(left, right)| {
                format!("issues #{left} and #{right} are path-disjoint yet share `{resource}`")
            })
            .collect();
        for (left, right) in &pair_edges {
            for issue in [*left, *right] {
                candidate_edge_membership
                    .entry(format!("candidate:issue:{issue}"))
                    .or_default()
                    .push(edge_id.clone());
            }
        }
        edges.push(WorkConflictEdgeV1 {
            id: edge_id,
            kind: WorkConflictEdgeKindV1::SharedContract,
            subjects,
            evidence,
            note: "path-disjoint work sharing one output contract, golden family, spec/ledger, workflow, release asset or mutable report/build root is conflict-visible".to_string(),
        });
    }
    edges.sort_by(|left, right| left.id.cmp(&right.id));
    for membership in candidate_edge_membership.values_mut() {
        membership.sort();
        membership.dedup();
    }

    // Lanes: one row per distinct lane, with membership gathered across
    // every portfolio issue in the lane (an owner claim may belong to any
    // member, not just the first issue encountered).
    let mut lane_members: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for issue in issues {
        if !portfolio_issue_numbers.contains(&issue.number) {
            continue;
        }
        lane_members
            .entry(lane_name(issues, &campaign_map, issue.number))
            .or_default()
            .push(issue.number);
    }
    let mut lanes: Vec<WorkLaneV1> = Vec::new();
    for (lane, members) in lane_members {
        let owner = active_claims
            .iter()
            .find(|claim| {
                (match claim.issue {
                    Some(number) => members.contains(&number),
                    None => false,
                }) || members
                    .iter()
                    .any(|member| issue_branch_matches(issues, claim, *member))
            })
            .map(|claim| claim.id.clone());
        let state = if !context.fresh(WorkCapturedSourceKindV1::LocalState)
            || !context.fresh(WorkCapturedSourceKindV1::GithubClaims)
        {
            WorkLaneStateV1::Unknown
        } else if claim_collision_lanes.contains(&lane) || branch_collision_lanes.contains(&lane) {
            WorkLaneStateV1::Conflicting
        } else if owner.is_some() {
            WorkLaneStateV1::Occupied
        } else {
            WorkLaneStateV1::Available
        };
        let collision_edge = lane_collision_edge(&edges, &members);
        lanes.push(WorkLaneV1 {
            lane,
            state,
            owner,
            collision_edge,
        });
    }
    lanes.sort_by(|left, right| left.lane.cmp(&right.lane));

    // Candidates: one per portfolio issue, complete set preserved.
    let mut candidates: Vec<WorkCandidateV1> = Vec::new();
    for issue in issues {
        if !portfolio_issue_numbers.contains(&issue.number) {
            continue;
        }
        let linked_prs: Vec<&WorkCapturedPullRequestV1> = pull_requests
            .iter()
            .filter(|pr| pr.linked_issues.contains(&issue.number))
            .collect();
        let open_prs: Vec<&WorkCapturedPullRequestV1> = linked_prs
            .iter()
            .copied()
            .filter(|pr| pr.state == "open")
            .collect();
        let merged_prs: Vec<&WorkCapturedPullRequestV1> = linked_prs
            .iter()
            .copied()
            .filter(|pr| pr.state == "merged")
            .collect();
        let issue_claims: Vec<&WorkCapturedClaimV1> = active_claims
            .iter()
            .copied()
            .filter(|claim| claim.issue == Some(issue.number))
            .collect();
        let has_claim_collision = claim_collision_by_issue.contains(&issue.number);
        let has_branch_collision = branch_collision_by_issue.contains_key(&issue.number);
        let has_slice = captured
            .cargo_allow
            .as_ref()
            .is_some_and(|graph| issue_has_accepted_slice(graph, issue));
        let (kind, next_transition) = classify_candidate(
            issue,
            &open_prs,
            &merged_prs,
            &issue_claims,
            has_claim_collision,
            has_branch_collision,
            has_slice,
            context.fresh(WorkCapturedSourceKindV1::GithubPullRequests),
            &current_main,
        );
        let primary_pr = open_prs.first().copied().or(merged_prs.first().copied());
        let lane = lane_name(issues, &campaign_map, issue.number);
        let lane_state = lanes
            .iter()
            .find(|entry| entry.lane == lane)
            .map(|entry| entry.state)
            .unwrap_or(WorkLaneStateV1::Unknown);
        let confidence_reasons = confidence_reasons_for(
            &context,
            issue,
            primary_pr,
            &issue_claims,
            candidate_edge_membership
                .get(&format!("candidate:issue:{}", issue.number))
                .cloned()
                .unwrap_or_default(),
            &edges,
        );
        let confidence = if confidence_reasons
            .iter()
            .any(|reason| reason.starts_with("not_proven:"))
        {
            WorkConfidenceV1::NotProven
        } else if !confidence_reasons.is_empty() {
            WorkConfidenceV1::Partial
        } else {
            WorkConfidenceV1::Complete
        };
        let memberships = campaign_memberships(&campaign_map, issue.number);
        let accepted_contracts = canonical_strings(&issue.accepted_contracts);
        let blockers = canonical_blockers(&issue.blocked_by);
        let regression_risks = canonical_strings(&issue.regression_risks);
        let mut conflict_resources = issue.conflict_resources.clone();
        conflict_resources.sort();
        conflict_resources.dedup();
        let candidate_id = format!("candidate:issue:{}", issue.number);
        let mut overlapping: Vec<String> = candidate_edge_membership
            .get(&candidate_id)
            .cloned()
            .unwrap_or_default()
            .iter()
            .flat_map(|edge_id| {
                edges
                    .iter()
                    .find(|edge| &edge.id == edge_id)
                    .map(|edge| edge.subjects.clone())
                    .unwrap_or_default()
            })
            .filter(|subject| subject != &candidate_id)
            .collect();
        overlapping.sort();
        overlapping.dedup();
        let mut candidate = WorkCandidateV1 {
            candidate_id: candidate_id.clone(),
            kind,
            issue: issue.number,
            issue_identity: format!("issue:{}", issue.number),
            campaigns: memberships,
            accepted_contracts,
            lifecycle_stage: issue
                .lifecycle_disposition
                .unwrap_or(IssueLifecycleDispositionV1::NotRun),
            next_transition,
            pull_request: primary_pr.map(|pr| pr.number),
            branch: primary_pr
                .map(|pr| pr.head_branch.clone())
                .or_else(|| issue_claims.first().map(|claim| claim.branch.clone())),
            claim: primary_pr
                .and_then(|pr| pr.registered_claim.clone())
                .or_else(|| issue_claims.first().map(|claim| claim.id.clone())),
            worktree: primary_pr
                .and_then(|pr| pr.worktree_path.as_deref())
                .map(|path| portable_path(path, &local_root))
                .or_else(|| {
                    issue_claims
                        .first()
                        .and_then(|claim| claim.worktree.as_deref())
                        .map(|path| portable_path(path, &local_root))
                }),
            dependencies: blockers.iter().map(dependency_from_blocker).collect(),
            conflict_edges: candidate_edge_membership
                .get(&candidate_id)
                .cloned()
                .unwrap_or_default(),
            conflict_resources,
            overlapping_candidates: overlapping,
            proof_cost_class: cost_class(issue.proof_cost),
            review_ci_cost_class: cost_class(issue.review_ci_cost),
            lane: lane.clone(),
            lane_state,
            readiness: readiness_for(issue),
            regression_risks,
            ranking_factors: Vec::new(),
            confidence,
            confidence_reasons: confidence_reasons
                .iter()
                .map(|reason| {
                    reason
                        .trim_start_matches("partial:")
                        .trim_start_matches("not_proven:")
                        .trim()
                        .to_string()
                })
                .collect(),
            single_agent_preferred: issue.single_agent_preferred,
            duplicate_family: duplicate_family_by_issue.get(&issue.number).cloned(),
            packet_entrypoint: {
                let mut command = format!("cargo xtask work explain --candidate {candidate_id}");
                if let Some(dir) = captured_dir {
                    command.push_str(&format!(" --captured {}", shell_escape_arg(dir)));
                }
                command
            },
        };
        candidate.ranking_factors =
            build_ranking_factors(issue, open_prs.first().copied(), &candidate);
        candidates.push(candidate);
    }
    let rank_key = |candidate: &WorkCandidateV1| {
        let risk = issues
            .iter()
            .find(|issue| issue.number == candidate.issue)
            .and_then(|issue| issue.honesty_risk.as_deref());
        let has_open_pr = pull_requests
            .iter()
            .any(|pr| pr.state == "open" && pr.linked_issues.contains(&candidate.issue));
        ranking_tuple(candidate, risk, has_open_pr)
    };
    candidates.sort_by(|left, right| {
        rank_key(left)
            .cmp(&rank_key(right))
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });

    // Campaign rows with live candidate counts.
    let mut campaign_rows: Vec<WorkCampaignV1> = Vec::new();
    if let Some(campaigns) = &captured.campaigns {
        for campaign in &campaigns.campaigns {
            let count = candidates
                .iter()
                .filter(|candidate| candidate.campaigns.contains(&campaign.id))
                .count() as u64;
            let mut surfaces = campaign.surfaces.clone();
            surfaces.sort();
            surfaces.dedup();
            for surface in &surfaces {
                let known = captured
                    .surfaces
                    .as_ref()
                    .is_some_and(|body| body.surfaces.iter().any(|row| row.id == *surface));
                if !known {
                    boundaries.insert(format!(
                        "campaign `{}` references surface `{surface}` absent from the captured surface source",
                        campaign.id
                    ));
                }
            }
            let mut campaign_issues = campaign.issues.clone();
            campaign_issues.sort_unstable();
            campaign_issues.dedup();
            campaign_rows.push(WorkCampaignV1 {
                id: campaign.id.clone(),
                title: campaign.title.clone(),
                state: campaign.state.clone(),
                surfaces,
                issues: campaign_issues,
                candidates: count,
                note: campaign.note.clone(),
            });
        }
    }
    campaign_rows.sort_by(|left, right| left.id.cmp(&right.id));

    // Claim rows.
    let colliding_claims: BTreeSet<String> = edges
        .iter()
        .filter(|edge| edge.kind == WorkConflictEdgeKindV1::ClaimCollision)
        .flat_map(|edge| {
            edge.subjects
                .iter()
                .filter_map(|subject| subject.strip_prefix("claim:").map(str::to_string))
        })
        .collect();
    let mut claim_rows: Vec<WorkClaimV1> = claims
        .iter()
        .map(|claim| WorkClaimV1 {
            id: claim.id.clone(),
            issue: claim.issue,
            branch: claim.branch.clone(),
            worktree: claim
                .worktree
                .as_deref()
                .map(|path| portable_path(path, &local_root)),
            exclusive: claim.exclusive,
            state: claim.state.clone(),
            colliding: colliding_claims.contains(&claim.id),
        })
        .collect();
    claim_rows.sort_by(|left, right| left.id.cmp(&right.id));

    // Global boundaries from degraded sources.
    for observation in &observations {
        if observation.freshness != WorkSourceFreshnessV1::Current {
            let freshness = wire_label(&observation.freshness);
            boundaries.insert(format!(
                "source `{}` is {} (freshness {freshness}): dependent candidates carry partial or not_proven confidence",
                observation.source, observation.state
            ));
        }
    }
    let total = candidates.len() as u64;
    let identity_source = WorkPortfolioIdentitySource {
        schema_version: WORK_PORTFOLIO_SNAPSHOT_SCHEMA_VERSION.to_string(),
        repository: WorkRepositoryIdentityV1 {
            repository: captured.manifest.repository.clone(),
            default_branch: captured.manifest.default_branch.clone(),
            default_branch_sha: captured.manifest.default_branch_sha.clone(),
            root: "<root>".to_string(),
        },
        captured_inputs: captured_inputs_list(captured),
        source_observations: observations.clone(),
        campaigns: campaign_rows.clone(),
        issues: issue_rows.clone(),
        pull_requests: pr_rows.clone(),
        claims: claim_rows.clone(),
        lanes: lanes.clone(),
        conflict_edges: edges.clone(),
        candidates: candidates.clone(),
        counts: WorkPortfolioCountsV1 {
            campaigns: campaign_rows.len() as u64,
            issues: issue_rows.len() as u64,
            pull_requests: pr_rows.len() as u64,
            claims: claim_rows.len() as u64,
            lanes: lanes.len() as u64,
            conflict_edges: edges.len() as u64,
            candidates: WorkSelectionCountsV1 {
                total,
                selected: total,
                omitted: 0,
            },
        },
        partial_data_boundaries: boundaries.iter().cloned().collect(),
        claim_boundary: WORK_PORTFOLIO_CLAIM_BOUNDARY.to_string(),
    };
    let portable_identity = portable_identity("work portfolio snapshot", &identity_source)?;
    Ok(WorkPortfolioSnapshotV1 {
        schema_version: WORK_PORTFOLIO_SNAPSHOT_SCHEMA_VERSION.to_string(),
        repository: identity_source.repository,
        captured_inputs: identity_source.captured_inputs,
        source_observations: identity_source.source_observations,
        campaigns: identity_source.campaigns,
        issues: identity_source.issues,
        pull_requests: identity_source.pull_requests,
        claims: identity_source.claims,
        lanes: identity_source.lanes,
        conflict_edges: identity_source.conflict_edges,
        candidates: identity_source.candidates,
        counts: identity_source.counts,
        retrieval_commands: Vec::new(),
        partial_data_boundaries: identity_source.partial_data_boundaries,
        claim_boundary: identity_source.claim_boundary,
        portable_identity,
    })
}

/// The captured-input file list actually present, in canonical order. The
/// volatile root spelling never appears: only portable relative names.
fn captured_inputs_list(captured: &WorkCapturedDirV1) -> Vec<String> {
    let mut present: Vec<String> = Vec::new();
    for kind in WorkCapturedSourceKindV1::all() {
        let state = captured
            .manifest
            .sources
            .iter()
            .find(|source| source.source == kind)
            .map(|source| source.state);
        if matches!(
            state,
            Some(WorkCapturedSourceStateV1::Observed | WorkCapturedSourceStateV1::Stale)
        ) {
            present.extend(kind.file_name().map(str::to_string));
        }
    }
    present.sort();
    present
}

/// Lane naming: the issue's explicit lane, else a lane derived from its
/// first sorted campaign.
fn lane_name(
    issues: &[WorkCapturedIssueV1],
    campaign_map: &BTreeMap<String, Vec<u64>>,
    issue: u64,
) -> String {
    if let Some(explicit) = issues
        .iter()
        .find(|entry| entry.number == issue)
        .and_then(|entry| entry.lane.clone())
    {
        return explicit;
    }
    let mut campaigns: Vec<String> = campaign_map
        .iter()
        .filter(|(_, members)| members.contains(&issue))
        .map(|(campaign, _)| campaign.clone())
        .collect();
    campaigns.sort();
    match campaigns.first() {
        Some(campaign) => format!("lane:{campaign}"),
        None => format!("lane:issue-{issue}"),
    }
}

/// Match a claim's branch to an issue by exact path segments, never
/// substrings: `feat/work-9101` matches issue 9101, but `feat/19101-cleanup`
/// must not (the number appears only inside an unrelated segment).
fn issue_branch_matches(
    issues: &[WorkCapturedIssueV1],
    claim: &WorkCapturedClaimV1,
    issue: u64,
) -> bool {
    let needle = issue.to_string();
    issues.iter().any(|entry| {
        entry.number == issue
            && claim
                .branch
                .split(['-', '/', '_'])
                .any(|segment| segment == needle)
    })
}

/// Confidence reasons for one candidate, named and honest. `partial:` and
/// `not_proven:` prefixes drive the confidence level.
fn confidence_reasons_for(
    context: &WorkCompileContext,
    issue: &WorkCapturedIssueV1,
    primary_pr: Option<&WorkCapturedPullRequestV1>,
    issue_claims: &[&WorkCapturedClaimV1],
    edge_ids: Vec<String>,
    edges: &[WorkConflictEdgeV1],
) -> Vec<String> {
    let mut reasons = Vec::new();
    if let Some(level) = context.issue_source_confidence() {
        let prefix = match level {
            WorkConfidenceV1::Partial => "partial:",
            WorkConfidenceV1::NotProven => "not_proven:",
            WorkConfidenceV1::Complete => "",
        };
        reasons.push(format!(
            "{prefix} the captured issue source is not current for issue #{}",
            issue.number
        ));
    }
    if primary_pr.is_some() && !context.fresh(WorkCapturedSourceKindV1::GithubPullRequests) {
        reasons.push(
            "partial: the captured pull-request source is not current for the linked PR"
                .to_string(),
        );
    }
    if primary_pr.is_none() && !context.fresh(WorkCapturedSourceKindV1::GithubPullRequests) {
        reasons.push(
            "partial: no pull request is captured for this issue while the pull-request source is not current; existing PR work may be uncaptured"
                .to_string(),
        );
    }
    if !issue_claims.is_empty() && !context.fresh(WorkCapturedSourceKindV1::GithubClaims) {
        reasons.push(
            "partial: the captured claim source is not current for the linked claim".to_string(),
        );
    }
    if issue_claims.is_empty() && !context.fresh(WorkCapturedSourceKindV1::GithubClaims) {
        reasons.push(
            "partial: no durable claim is captured for this issue while the claim source is not current; existing ownership may be uncaptured"
                .to_string(),
        );
    }
    if (primary_pr.is_some() || !issue_claims.is_empty())
        && !context.fresh(WorkCapturedSourceKindV1::LocalState)
    {
        reasons
            .push("partial: local branch/worktree truth is not current for the lane".to_string());
    }
    if !issue.requirement_refs.is_empty() && !context.fresh(WorkCapturedSourceKindV1::CargoAllow) {
        reasons.push("partial: the cargo-allow graph is not observed; requirement and duplicate-family evidence is weakened".to_string());
    }
    if !context.fresh(WorkCapturedSourceKindV1::Campaigns) {
        reasons.push(
            "partial: the campaign source is not current; portfolio membership may have drifted"
                .to_string(),
        );
    }
    for edge_id in edge_ids {
        if let Some(edge) = edges.iter().find(|edge| edge.id == edge_id)
            && matches!(
                edge.kind,
                WorkConflictEdgeKindV1::ClaimCollision | WorkConflictEdgeKindV1::BranchCollision
            )
        {
            reasons.push(format!("partial: unresolved collision `{}`", edge.id));
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

/// Identity source: the snapshot minus its own `portable_identity` and
/// retrieval fields, digested for the portable identity.
#[derive(Clone, Debug, Serialize)]
struct WorkPortfolioIdentitySource {
    schema_version: String,
    repository: WorkRepositoryIdentityV1,
    captured_inputs: Vec<String>,
    source_observations: Vec<WorkSourceObservationV1>,
    campaigns: Vec<WorkCampaignV1>,
    issues: Vec<WorkIssueV1>,
    pull_requests: Vec<WorkPullRequestV1>,
    claims: Vec<WorkClaimV1>,
    lanes: Vec<WorkLaneV1>,
    conflict_edges: Vec<WorkConflictEdgeV1>,
    candidates: Vec<WorkCandidateV1>,
    counts: WorkPortfolioCountsV1,
    partial_data_boundaries: Vec<String>,
    claim_boundary: String,
}

// ---------------------------------------------------------------------------
// Views, renderers and commands.
// ---------------------------------------------------------------------------

fn known_campaign_ids(snapshot: &WorkPortfolioSnapshotV1) -> Vec<String> {
    snapshot
        .campaigns
        .iter()
        .map(|campaign| campaign.id.clone())
        .collect()
}

fn known_surface_ids(snapshot: &WorkPortfolioSnapshotV1) -> Vec<String> {
    let mut surfaces: BTreeSet<String> = BTreeSet::new();
    for campaign in &snapshot.campaigns {
        for surface in &campaign.surfaces {
            surfaces.insert(surface.clone());
        }
    }
    surfaces.into_iter().collect()
}

fn campaign_surfaces(snapshot: &WorkPortfolioSnapshotV1, campaign: &str) -> Vec<String> {
    snapshot
        .campaigns
        .iter()
        .find(|row| row.id == campaign)
        .map(|row| row.surfaces.clone())
        .unwrap_or_default()
}

/// Shell-escape one argument for the generated retrieval commands. Simple
/// portable tokens stay bare so committed output stays byte-stable; anything
/// else is single-quoted so the advertised command replays exactly.
fn shell_escape_arg(value: &str) -> String {
    let bare = !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':' | '=' | '+')
        });
    if bare {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Build the bounded candidates view. Filters narrow the rendered rows only;
/// candidate identity, classification, ranking and authority never change.
pub(crate) fn build_candidates_view(
    snapshot: &WorkPortfolioSnapshotV1,
    campaign: Option<&str>,
    surface: Option<&str>,
    limit: usize,
    captured: Option<&str>,
) -> Result<WorkCandidatesViewV1, String> {
    if let Some(campaign) = campaign
        && !snapshot.campaigns.iter().any(|row| row.id == campaign)
    {
        return Err(format!(
            "unknown campaign `{campaign}`; known campaigns: {:?}",
            known_campaign_ids(snapshot)
        ));
    }
    if let Some(surface) = surface
        && !known_surface_ids(snapshot).iter().any(|id| id == surface)
    {
        return Err(format!(
            "unknown surface `{surface}`; known surfaces: {:?}",
            known_surface_ids(snapshot)
        ));
    }
    let filtered: Vec<WorkCandidateV1> = snapshot
        .candidates
        .iter()
        .filter(|candidate| campaign.is_none_or(|id| candidate.campaigns.contains(&id.to_string())))
        .filter(|candidate| {
            surface.is_none_or(|id| {
                candidate.campaigns.iter().any(|campaign_id| {
                    campaign_surfaces(snapshot, campaign_id).contains(&id.to_string())
                })
            })
        })
        .cloned()
        .collect();
    let total = filtered.len() as u64;
    let selected = filtered.len().min(limit) as u64;
    let omitted = total.saturating_sub(selected);
    let mut retrieval_commands = Vec::new();
    if omitted > 0 {
        let mut command = "cargo xtask work candidates".to_string();
        if let Some(captured) = captured {
            command.push_str(&format!(" --captured {}", shell_escape_arg(captured)));
        }
        if let Some(campaign) = campaign {
            command.push_str(&format!(" --campaign {}", shell_escape_arg(campaign)));
        }
        if let Some(surface) = surface {
            command.push_str(&format!(" --surface {}", shell_escape_arg(surface)));
        }
        command.push_str(&format!(" --limit {}", total as usize));
        retrieval_commands.push(command);
        let mut portfolio_command = "cargo xtask work portfolio".to_string();
        if let Some(captured) = captured {
            portfolio_command.push_str(&format!(" --captured {}", shell_escape_arg(captured)));
        }
        portfolio_command.push_str(" --json");
        retrieval_commands.push(portfolio_command);
    }
    let boundaries = snapshot.partial_data_boundaries.clone();
    let view = WorkCandidatesViewV1 {
        schema_version: WORK_CANDIDATES_VIEW_SCHEMA_VERSION.to_string(),
        portfolio_identity: snapshot.portable_identity.clone(),
        filter_campaign: campaign.map(str::to_string),
        filter_surface: surface.map(str::to_string),
        limit: limit as u64,
        counts: WorkSelectionCountsV1 {
            total,
            selected,
            omitted,
        },
        retrieval_commands,
        candidates: filtered.into_iter().take(limit).collect(),
        partial_data_boundaries: boundaries,
        claim_boundary: WORK_PORTFOLIO_CLAIM_BOUNDARY.to_string(),
    };
    Ok(view)
}

/// Build the single-candidate explain view; unknown ids fail closed with the
/// stable candidate id list.
pub(crate) fn build_explain_view(
    snapshot: &WorkPortfolioSnapshotV1,
    candidate_id: &str,
) -> Result<WorkExplainViewV1, String> {
    let Some(candidate) = snapshot
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == candidate_id)
    else {
        let known: Vec<String> = snapshot
            .candidates
            .iter()
            .map(|candidate| candidate.candidate_id.clone())
            .collect();
        return Err(format!(
            "unknown candidate `{candidate_id}`; known candidates: {known:?}"
        ));
    };
    Ok(WorkExplainViewV1 {
        schema_version: WORK_EXPLAIN_VIEW_SCHEMA_VERSION.to_string(),
        portfolio_identity: snapshot.portable_identity.clone(),
        candidate: candidate.clone(),
        partial_data_boundaries: snapshot.partial_data_boundaries.clone(),
        claim_boundary: WORK_PORTFOLIO_CLAIM_BOUNDARY.to_string(),
    })
}

/// Render one closed-vocabulary value by its serde wire spelling
/// (`repair_review`, not the Debug `RepairReview`).
fn wire_label(value: &impl Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        Ok(other) => other.to_string(),
        Err(_) => "unknown".to_string(),
    }
}

fn md_table_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}

/// Human Markdown for the portfolio. Derived from the same DTO; it can never
/// strengthen readiness or ranking.
pub(crate) fn work_portfolio_markdown(snapshot: &WorkPortfolioSnapshotV1) -> String {
    let mut body = String::new();
    body.push_str("# Work portfolio\n\n");
    body.push_str(&format!(
        "- repository: `{}`\n",
        snapshot.repository.repository
    ));
    body.push_str(&format!(
        "- default branch: `{}` @ `{}`\n",
        snapshot.repository.default_branch, snapshot.repository.default_branch_sha
    ));
    body.push_str(&format!(
        "- portable identity: `{}`\n",
        snapshot.portable_identity
    ));
    body.push_str(&format!(
        "- claim boundary: {}\n\n",
        snapshot.claim_boundary
    ));

    body.push_str("## Sources\n\n");
    body.push_str(&md_table_row(&[
        "source".to_string(),
        "state".to_string(),
        "freshness".to_string(),
        "partial".to_string(),
        "request id".to_string(),
    ]));
    body.push_str("\n| --- | --- | --- | --- | --- |\n");
    for observation in &snapshot.source_observations {
        body.push_str(&md_table_row(&[
            observation.source.clone(),
            observation.state.clone(),
            wire_label(&observation.freshness),
            observation.partial.to_string(),
            observation.request_id.clone().unwrap_or_default(),
        ]));
        body.push('\n');
    }
    body.push('\n');

    body.push_str("## Campaigns\n\n");
    body.push_str(&md_table_row(&[
        "campaign".to_string(),
        "state".to_string(),
        "surfaces".to_string(),
        "issues".to_string(),
        "candidates".to_string(),
    ]));
    body.push_str("\n| --- | --- | --- | --- | --- |\n");
    for campaign in &snapshot.campaigns {
        body.push_str(&md_table_row(&[
            campaign.id.clone(),
            campaign.state.clone(),
            campaign.surfaces.join(", "),
            format!("{:?}", campaign.issues),
            campaign.candidates.to_string(),
        ]));
        body.push('\n');
    }
    body.push('\n');

    body.push_str("## Candidates (ranked, advisory)\n\n");
    body.push_str(&md_table_row(&[
        "rank".to_string(),
        "candidate".to_string(),
        "kind".to_string(),
        "stage".to_string(),
        "lane".to_string(),
        "confidence".to_string(),
    ]));
    body.push_str("\n| --- | --- | --- | --- | --- | --- |\n");
    for (rank, candidate) in snapshot.candidates.iter().enumerate() {
        body.push_str(&md_table_row(&[
            (rank + 1).to_string(),
            candidate.candidate_id.clone(),
            wire_label(&candidate.kind),
            wire_label(&candidate.lifecycle_stage),
            candidate.lane.clone(),
            wire_label(&candidate.confidence),
        ]));
        body.push('\n');
    }
    body.push('\n');

    body.push_str("## Conflict edges\n\n");
    for edge in &snapshot.conflict_edges {
        body.push_str(&format!(
            "- `{}` ({}): {} — {}\n",
            edge.id,
            wire_label(&edge.kind),
            edge.subjects.join(", "),
            edge.note
        ));
    }
    if snapshot.conflict_edges.is_empty() {
        body.push_str("- none\n");
    }
    body.push('\n');

    body.push_str("## Partial-data boundaries\n\n");
    for boundary in &snapshot.partial_data_boundaries {
        body.push_str(&format!("- {boundary}\n"));
    }
    if snapshot.partial_data_boundaries.is_empty() {
        body.push_str("- none: every source is current and observed\n");
    }
    body.push('\n');
    body
}

fn candidate_markdown(candidate: &WorkCandidateV1, heading: &str) -> String {
    let mut body = String::new();
    body.push_str(&format!("{heading}\n\n"));
    body.push_str(&format!("- candidate: `{}`\n", candidate.candidate_id));
    body.push_str(&format!("- kind: `{}`\n", wire_label(&candidate.kind)));
    body.push_str(&format!(
        "- issue: #{} ({}) campaigns {:?}\n",
        candidate.issue, candidate.issue_identity, candidate.campaigns
    ));
    body.push_str(&format!(
        "- lifecycle stage: `{}`; next transition: {}\n",
        wire_label(&candidate.lifecycle_stage),
        candidate.next_transition
    ));
    if let Some(pr) = candidate.pull_request {
        body.push_str(&format!("- open PR: #{pr}\n"));
    }
    if let Some(claim) = &candidate.claim {
        body.push_str(&format!("- claim: `{claim}`\n"));
    }
    if let Some(branch) = &candidate.branch {
        body.push_str(&format!("- branch: `{branch}`\n"));
    }
    if let Some(worktree) = &candidate.worktree {
        body.push_str(&format!("- worktree: `{worktree}`\n"));
    }
    body.push_str(&format!(
        "- lane: `{}` ({})\n",
        candidate.lane,
        wire_label(&candidate.lane_state)
    ));
    body.push_str(&format!(
        "- confidence: `{}` — {}\n",
        wire_label(&candidate.confidence),
        candidate.confidence_reasons.join("; ")
    ));
    body.push_str(&format!(
        "- proof cost: `{}`; review/CI cost: `{}`\n",
        wire_label(&candidate.proof_cost_class),
        wire_label(&candidate.review_ci_cost_class)
    ));
    if !candidate.dependencies.is_empty() {
        body.push_str("- dependencies/blockers:\n");
        for dependency in &candidate.dependencies {
            body.push_str(&format!(
                "  - `{}` `{}` — {}\n",
                dependency.kind, dependency.reference, dependency.description
            ));
        }
    }
    if !candidate.conflict_edges.is_empty() {
        body.push_str(&format!(
            "- conflict edges: {:?}\n",
            candidate.conflict_edges
        ));
    }
    if let Some(family) = &candidate.duplicate_family {
        body.push_str(&format!("- duplicate family: `{family}`\n"));
    }
    if let Some(readiness) = &candidate.readiness {
        body.push_str(&format!(
            "- readiness (advisory, sourced from `{}`): {}\n",
            readiness.source, readiness.contribution
        ));
    }
    if !candidate.regression_risks.is_empty() {
        body.push_str(&format!(
            "- regression risks: {:?}\n",
            candidate.regression_risks
        ));
    }
    body.push_str("- ranking factors (ordered, no hidden score):\n");
    for factor in &candidate.ranking_factors {
        body.push_str(&format!(
            "  {}. `{}` — {}\n",
            factor.order,
            wire_label(&factor.factor),
            factor.contribution
        ));
    }
    body.push_str(&format!(
        "- packet entrypoint: `{}`\n",
        candidate.packet_entrypoint
    ));
    body
}

pub(crate) fn work_candidates_markdown(view: &WorkCandidatesViewV1) -> String {
    let mut body = String::new();
    body.push_str("# Work candidates\n\n");
    body.push_str(&format!(
        "- portfolio identity: `{}`\n",
        view.portfolio_identity
    ));
    body.push_str(&format!(
        "- filter: campaign={:?} surface={:?} limit={}\n",
        view.filter_campaign, view.filter_surface, view.limit
    ));
    body.push_str(&format!(
        "- counts: total={} selected={} omitted={}\n",
        view.counts.total, view.counts.selected, view.counts.omitted
    ));
    for command in &view.retrieval_commands {
        body.push_str(&format!("- retrieval: `{command}`\n"));
    }
    body.push_str(&format!("- claim boundary: {}\n\n", view.claim_boundary));
    for (rank, candidate) in view.candidates.iter().enumerate() {
        let heading = format!("## {}. `{}`", rank + 1, candidate.candidate_id);
        body.push_str(&candidate_markdown(candidate, &heading));
        body.push('\n');
    }
    if view.candidates.is_empty() {
        body.push_str(
            "No candidates match the filter; the portfolio and other campaigns remain available.\n",
        );
    }
    body
}

pub(crate) fn work_explain_markdown(view: &WorkExplainViewV1) -> String {
    let heading = format!("# Work candidate `{}`", view.candidate.candidate_id);
    let mut body = candidate_markdown(&view.candidate, &heading);
    body.push_str(&format!(
        "\n- portfolio identity: `{}`\n",
        view.portfolio_identity
    ));
    if !view.partial_data_boundaries.is_empty() {
        body.push_str("- partial-data boundaries:\n");
        for boundary in &view.partial_data_boundaries {
            body.push_str(&format!("  - {boundary}\n"));
        }
    }
    body
}

// ---------------------------------------------------------------------------
// Command handlers. Every portfolio command is read-only: it loads captured
// bytes, compiles, and renders to stdout and `target/ripr/reports` only.
// ---------------------------------------------------------------------------

fn parse_captured_dir(args: &[String], usage: &str) -> Result<(String, usize), String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == "--captured" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("missing value for --captured\n{usage}"))?;
            return Ok((value.clone(), index));
        }
    }
    Ok((DEFAULT_WORK_CAPTURED_DIR.to_string(), usize::MAX))
}

fn split_captured_arg(
    args: &[String],
    usage: &str,
) -> Result<(String, Option<String>, Vec<String>), String> {
    let (dir, captured_index) = parse_captured_dir(args, usage)?;
    // Keep the caller's explicit spelling (when provided) so generated
    // packet entrypoints and retrieval commands replay against the same
    // corpus the snapshot was compiled from.
    let explicit = (captured_index != usize::MAX).then(|| dir.clone());
    let mut rest: Vec<String> = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        if index == captured_index || (captured_index != usize::MAX && index == captured_index + 1)
        {
            continue;
        }
        rest.push(arg.clone());
    }
    Ok((dir, explicit, rest))
}

fn has_json_flag(args: &[String], usage: &str) -> Result<bool, String> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--help" | "-h" => return Err(usage.to_string()),
            other => return Err(format!("unknown argument `{other}`\n{usage}")),
        }
    }
    Ok(json)
}

fn load_and_compile(
    dir: &str,
    captured_dir: Option<&str>,
) -> Result<WorkPortfolioSnapshotV1, String> {
    // A user-supplied `--captured` directory is cwd-relative (absolute paths
    // honored as-is); only when it does not exist relative to the cwd does
    // it fall back to the workspace root, which keeps the documented default
    // `fixtures/work_portfolio/corpus` working from any directory.
    let as_given = Path::new(dir);
    let root = if as_given.is_absolute() || as_given.is_dir() {
        as_given.to_path_buf()
    } else {
        workspace_path(dir)
    };
    let captured = load_work_captured_dir(&root)?;
    compile_work_portfolio(&captured, captured_dir)
}

/// `cargo xtask work portfolio [--captured <dir>] [--json]` (#1704,
/// RIPR-SPEC-0234): compile the captured directory into the versioned
/// snapshot, write the standard reports, and render JSON or the derived
/// Markdown to stdout. Read-only: no GitHub, branch, worktree, claim, spec,
/// campaign or source state is touched.
pub(crate) fn work_portfolio_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work portfolio [--captured <dir>] [--json]";
    let (dir, explicit_captured, rest) = split_captured_arg(args, USAGE)?;
    let json = has_json_flag(&rest, USAGE)?;
    let snapshot = load_and_compile(&dir, explicit_captured.as_deref())?;
    let json_body = work_portfolio_json(&snapshot)?;
    crate::write_report("work-portfolio.json", &json_body)?;
    crate::write_report("work-portfolio.md", &work_portfolio_markdown(&snapshot))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_portfolio_markdown(&snapshot));
    }
    Ok(())
}

/// `cargo xtask work candidates [--campaign <id>] [--surface <id>]
/// [--limit <n>] [--json]`: render the ranked candidate array under neutral
/// filters with a bounded limit. Filters never change candidate identity,
/// classification, ranking or authority.
pub(crate) fn work_candidates_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask work candidates [--captured <dir>] [--campaign <id>] [--surface <id>] [--limit <n>] [--json]";
    let (dir, explicit_captured, rest) = split_captured_arg(args, USAGE)?;
    let mut campaign = None;
    let mut surface = None;
    let mut limit = DEFAULT_CANDIDATE_LIMIT;
    let mut json = false;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--campaign" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for --campaign\n{USAGE}"))?;
                if campaign.replace(value.clone()).is_some() {
                    return Err(format!("duplicate --campaign\n{USAGE}"));
                }
                index += 2;
            }
            "--surface" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for --surface\n{USAGE}"))?;
                if surface.replace(value.clone()).is_some() {
                    return Err(format!("duplicate --surface\n{USAGE}"));
                }
                index += 2;
            }
            "--limit" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for --limit\n{USAGE}"))?;
                let parsed: usize = value
                    .parse()
                    .map_err(|error| format!("invalid --limit `{value}`: {error}\n{USAGE}"))?;
                limit = parsed;
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
    let snapshot = load_and_compile(&dir, explicit_captured.as_deref())?;
    let view = build_candidates_view(
        &snapshot,
        campaign.as_deref(),
        surface.as_deref(),
        limit,
        explicit_captured.as_deref(),
    )?;
    let json_body = work_portfolio_json(&view)?;
    crate::write_report("work-candidates.json", &json_body)?;
    crate::write_report("work-candidates.md", &work_candidates_markdown(&view))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_candidates_markdown(&view));
    }
    Ok(())
}

/// `cargo xtask work explain --candidate <id> [--json]`: render exactly one
/// candidate by stable id; unknown ids fail closed with the stable id list.
pub(crate) fn work_explain_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str =
        "usage: cargo xtask work explain --candidate <id> [--captured <dir>] [--json]";
    let (dir, explicit_captured, rest) = split_captured_arg(args, USAGE)?;
    let mut candidate = None;
    let mut json = false;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--candidate" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for --candidate\n{USAGE}"))?;
                if candidate.replace(value.clone()).is_some() {
                    return Err(format!("duplicate --candidate\n{USAGE}"));
                }
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
    let Some(candidate) = candidate else {
        return Err(format!("missing required --candidate <id>\n{USAGE}"));
    };
    let snapshot = load_and_compile(&dir, explicit_captured.as_deref())?;
    let view = build_explain_view(&snapshot, &candidate)?;
    let json_body = work_portfolio_json(&view)?;
    crate::write_report("work-explain.json", &json_body)?;
    crate::write_report("work-explain.md", &work_explain_markdown(&view))?;
    if json {
        print!("{json_body}");
    } else {
        print!("{}", work_explain_markdown(&view));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fixture provenance and the committed-corpus validator shared by
// `check-fixture-contracts` and the test suite.
// ---------------------------------------------------------------------------

pub(crate) const WORK_PORTFOLIO_PROVENANCE_SCHEMA_VERSION: &str = "work_portfolio_provenance.v1";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkPortfolioProvenanceFileV1 {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkPortfolioProvenanceCorpusV1 {
    pub name: String,
    pub files: Vec<WorkPortfolioProvenanceFileV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkPortfolioProvenanceV1 {
    pub schema_version: String,
    pub repository: String,
    pub captured_at: String,
    pub capture_method: String,
    pub corpora: Vec<WorkPortfolioProvenanceCorpusV1>,
}

pub(crate) fn load_work_portfolio_provenance(
    body: &str,
) -> Result<WorkPortfolioProvenanceV1, String> {
    let provenance: WorkPortfolioProvenanceV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse work portfolio provenance: {error}"))?;
    if provenance.schema_version != WORK_PORTFOLIO_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported work portfolio provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "work portfolio provenance must record repository and capture_method".to_string(),
        );
    }
    if provenance.captured_at.trim().is_empty() {
        return Err("work portfolio provenance must record captured_at".to_string());
    }
    let mut names = BTreeSet::new();
    for corpus in &provenance.corpora {
        if !names.insert(corpus.name.clone()) {
            return Err(format!(
                "work portfolio provenance duplicates corpus `{}`",
                corpus.name
            ));
        }
        reject_unsafe_provenance_path("corpus name", &corpus.name)?;
        for file in &corpus.files {
            reject_unsafe_provenance_path("file path", &file.path)?;
        }
    }
    Ok(provenance)
}

/// Fail closed on provenance names/paths that could escape the corpus root:
/// absolute paths, `..` components, empty segments or empty strings.
fn reject_unsafe_provenance_path(kind: &str, value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() {
        return Err(format!(
            "work portfolio provenance {kind} `{value}` is not a relative path"
        ));
    }
    for component in path.components() {
        match component {
            std::path::Component::Normal(_) => {}
            _ => {
                return Err(format!(
                    "work portfolio provenance {kind} `{value}` contains a forbidden component"
                ));
            }
        }
    }
    Ok(())
}

fn digest_matches(recorded: &str, bytes: &[u8]) -> bool {
    recorded == crate::blind_journey::sha256_hex(bytes)
}

/// Load and compile one committed corpus directory by name
/// (`corpus`, `variants/<name>`).
pub(crate) fn compile_work_portfolio_corpus(name: &str) -> Result<WorkPortfolioSnapshotV1, String> {
    let root = workspace_path(&format!("fixtures/work_portfolio/{name}"));
    let captured = load_work_captured_dir(&root)?;
    // The committed corpus is the documented default: entrypoints stay bare
    // so portable identities and cross-variant byte equality hold.
    compile_work_portfolio(&captured, None)
}

fn candidate_by_issue(
    snapshot: &WorkPortfolioSnapshotV1,
    issue: u64,
) -> Result<&WorkCandidateV1, String> {
    snapshot
        .candidates
        .iter()
        .find(|candidate| candidate.issue == issue)
        .ok_or_else(|| format!("snapshot is missing candidate for issue #{issue}"))
}

/// Validate the committed work-portfolio corpora fail-closed: provenance
/// digest bindings, the twelve required scenarios, and every variant's
/// invariant (root portability, reordering neutrality, degraded-input
/// confidence). Registered in `check-fixture-contracts`.
pub(crate) fn validate_work_portfolio_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path("fixtures/work_portfolio");
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/work_portfolio: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_work_portfolio_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/work_portfolio: {error}"));
            return;
        }
    };
    for corpus in &provenance.corpora {
        for file in &corpus.files {
            let path = root.join(&corpus.name).join(&file.path);
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    violations.push(format!(
                        "fixtures/work_portfolio: {} is unreadable: {error}",
                        path.display()
                    ));
                    continue;
                }
            };
            if !digest_matches(&file.sha256, &bytes) {
                violations.push(format!(
                    "fixtures/work_portfolio: {} digest drifted: recorded `{}`, recomputed `work-portfolio:sha256:{}`",
                    path.display(),
                    file.sha256,
                    crate::blind_journey::sha256_hex(&bytes)
                ));
            }
        }
    }
    let canonical = match compile_work_portfolio_corpus("corpus") {
        Ok(snapshot) => snapshot,
        Err(error) => {
            violations.push(format!("fixtures/work_portfolio: corpus: {error}"));
            return;
        }
    };
    validate_canonical_scenarios(&canonical, violations);
    let canonical_json = match work_portfolio_json(&canonical) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!("fixtures/work_portfolio: corpus: {error}"));
            return;
        }
    };
    for variant in ["variants/unix_root", "variants/reordered"] {
        match compile_work_portfolio_corpus(variant) {
            Ok(snapshot) => {
                if snapshot.portable_identity != canonical.portable_identity {
                    violations.push(format!(
                        "fixtures/work_portfolio: {variant} portable identity `{}` drifts from the canonical corpus `{}`",
                        snapshot.portable_identity, canonical.portable_identity
                    ));
                }
                match work_portfolio_json(&snapshot) {
                    Ok(body) if body == canonical_json => {}
                    Ok(_) => violations.push(format!(
                        "fixtures/work_portfolio: {variant} normalized JSON drifts from the canonical corpus"
                    )),
                    Err(error) => violations.push(format!("fixtures/work_portfolio: {variant}: {error}")),
                }
            }
            Err(error) => violations.push(format!("fixtures/work_portfolio: {variant}: {error}")),
        }
    }
    match compile_work_portfolio_corpus("variants/missing_github") {
        Ok(snapshot) => {
            if !snapshot.candidates.is_empty() {
                violations.push(
                    "fixtures/work_portfolio: variants/missing_github must compile to zero candidates"
                        .to_string(),
                );
            }
            if snapshot.campaigns.len() != 4 {
                violations.push(format!(
                    "fixtures/work_portfolio: variants/missing_github must keep all four campaigns visible, got {}",
                    snapshot.campaigns.len()
                ));
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_portfolio: variants/missing_github: {error}"
        )),
    }
    match compile_work_portfolio_corpus("variants/stale_local") {
        Ok(snapshot) => {
            for (issue, expected) in [
                (9101, WorkConfidenceV1::Partial),
                (1693, WorkConfidenceV1::Complete),
            ] {
                match candidate_by_issue(&snapshot, issue) {
                    Ok(candidate) if candidate.confidence == expected => {}
                    Ok(candidate) => violations.push(format!(
                        "fixtures/work_portfolio: variants/stale_local issue #{issue} confidence {:?}, expected {:?}",
                        candidate.confidence, expected
                    )),
                    Err(error) => violations.push(format!("fixtures/work_portfolio: variants/stale_local: {error}")),
                }
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_portfolio: variants/stale_local: {error}"
        )),
    }
    match compile_work_portfolio_corpus("variants/no_cargo_allow") {
        Ok(snapshot) => {
            match candidate_by_issue(&snapshot, 9105) {
                Ok(candidate) if candidate.confidence == WorkConfidenceV1::Partial => {}
                Ok(candidate) => violations.push(format!(
                    "fixtures/work_portfolio: variants/no_cargo_allow issue #9105 confidence {:?}, expected partial",
                    candidate.confidence
                )),
                Err(error) => violations.push(format!("fixtures/work_portfolio: variants/no_cargo_allow: {error}")),
            }
            if !snapshot
                .conflict_edges
                .iter()
                .any(|edge| edge.id == "edge:duplicate_family:REQ-dup-family")
            {
                violations.push(
                    "fixtures/work_portfolio: variants/no_cargo_allow must keep the duplicate family visible"
                        .to_string(),
                );
            }
        }
        Err(error) => violations.push(format!(
            "fixtures/work_portfolio: variants/no_cargo_allow: {error}"
        )),
    }
}

fn validate_canonical_scenarios(snapshot: &WorkPortfolioSnapshotV1, violations: &mut Vec<String>) {
    let expect_campaigns = [
        "campaign-doc-polish",
        "campaign-editor-ux",
        "campaign-infra-hardening",
        "campaign-rust-repair",
    ];
    let campaign_ids: Vec<String> = snapshot
        .campaigns
        .iter()
        .map(|campaign| campaign.id.clone())
        .collect();
    if campaign_ids != expect_campaigns {
        violations.push(format!(
            "fixtures/work_portfolio: corpus campaigns {campaign_ids:?} drifted from {expect_campaigns:?}"
        ));
    }
    let expected_kinds: [(u64, WorkCandidateKindV1); 12] = [
        (9101, WorkCandidateKindV1::RepairReview),
        (9102, WorkCandidateKindV1::ResumePr),
        (9103, WorkCandidateKindV1::MergeReady),
        (9104, WorkCandidateKindV1::Blocked),
        (9105, WorkCandidateKindV1::StartBuild),
        (9106, WorkCandidateKindV1::StartBuild),
        (9107, WorkCandidateKindV1::Blocked),
        (9108, WorkCandidateKindV1::ResumePr),
        (9109, WorkCandidateKindV1::VerifyCurrentHead),
        (1693, WorkCandidateKindV1::StartBuild),
        (9202, WorkCandidateKindV1::StartBuild),
        (9301, WorkCandidateKindV1::Complete),
    ];
    for (issue, expected) in expected_kinds {
        match candidate_by_issue(snapshot, issue) {
            Ok(candidate) if candidate.kind == expected => {}
            Ok(candidate) => violations.push(format!(
                "fixtures/work_portfolio: corpus issue #{issue} kind {:?}, expected {:?}",
                candidate.kind, expected
            )),
            Err(error) => violations.push(format!("fixtures/work_portfolio: corpus: {error}")),
        }
    }
    for edge in [
        "edge:branch_collision:pr-8804",
        "edge:claim_collision:issue-9107",
        "edge:duplicate_family:REQ-dup-family",
        "edge:shared_contract:output-contract:rust-json",
    ] {
        if !snapshot.conflict_edges.iter().any(|row| row.id == edge) {
            violations.push(format!(
                "fixtures/work_portfolio: corpus is missing conflict edge `{edge}`"
            ));
        }
    }
    let expected_order = [
        "candidate:issue:9101",
        "candidate:issue:9103",
        "candidate:issue:9102",
        "candidate:issue:9107",
        "candidate:issue:9108",
        "candidate:issue:1693",
        "candidate:issue:9105",
        "candidate:issue:9106",
        "candidate:issue:9109",
        "candidate:issue:9202",
        "candidate:issue:9301",
        "candidate:issue:9104",
    ];
    let order: Vec<String> = snapshot
        .candidates
        .iter()
        .map(|candidate| candidate.candidate_id.clone())
        .collect();
    if order != expected_order {
        violations.push(format!(
            "fixtures/work_portfolio: corpus candidate order {order:?} drifted from {expected_order:?}"
        ));
    }
    let counts: Vec<(String, u64)> = snapshot
        .campaigns
        .iter()
        .map(|campaign| (campaign.id.clone(), campaign.candidates))
        .collect();
    let expected_counts = [
        ("campaign-doc-polish".to_string(), 1),
        ("campaign-editor-ux".to_string(), 2),
        ("campaign-infra-hardening".to_string(), 0),
        ("campaign-rust-repair".to_string(), 9),
    ];
    if counts != expected_counts {
        violations.push(format!(
            "fixtures/work_portfolio: corpus campaign candidate counts {counts:?} drifted from {expected_counts:?}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn committed() -> Result<WorkPortfolioSnapshotV1, String> {
        compile_work_portfolio_corpus("corpus")
    }

    fn candidate(
        snapshot: &WorkPortfolioSnapshotV1,
        issue: u64,
    ) -> Result<&WorkCandidateV1, String> {
        candidate_by_issue(snapshot, issue)
    }

    fn factor(
        candidate: &WorkCandidateV1,
        kind: WorkRankingFactorKindV1,
    ) -> Result<&WorkRankingFactorV1, String> {
        candidate
            .ranking_factors
            .iter()
            .find(|factor| factor.factor == kind)
            .ok_or_else(|| {
                format!(
                    "candidate `{}` is missing ranking factor {:?}",
                    candidate.candidate_id, kind
                )
            })
    }

    /// Box 1 + fixture rows 1/2/10/11: multiple campaigns with eligible
    /// issues in different surfaces appear in one snapshot, one lane full of
    /// collisions and one independent available lane, an empty campaign that
    /// stays visible, and the #1693 follow-ups visible next to the Rust
    /// repair campaign with no default campaign anywhere.
    #[test]
    fn work_portfolio_multiple_campaigns_and_independent_lanes_one_snapshot() -> Result<(), String>
    {
        let snapshot = committed()?;
        if snapshot.campaigns.len() != 4 {
            return Err(format!(
                "expected four campaigns, got {}",
                snapshot.campaigns.len()
            ));
        }
        let editor = snapshot
            .campaigns
            .iter()
            .find(|campaign| campaign.id == "campaign-editor-ux")
            .ok_or_else(|| "missing campaign-editor-ux".to_string())?;
        let rust = snapshot
            .campaigns
            .iter()
            .find(|campaign| campaign.id == "campaign-rust-repair")
            .ok_or_else(|| "missing campaign-rust-repair".to_string())?;
        let empty = snapshot
            .campaigns
            .iter()
            .find(|campaign| campaign.id == "campaign-infra-hardening")
            .ok_or_else(|| "missing campaign-infra-hardening".to_string())?;
        if editor.candidates == 0 || rust.candidates == 0 {
            return Err("both editor and rust campaigns must carry eligible work".to_string());
        }
        if empty.candidates != 0 || !empty.issues.is_empty() {
            return Err("the empty campaign must stay visible with zero candidates".to_string());
        }
        let issue1693 = candidate(&snapshot, 1693)?;
        if !issue1693
            .campaigns
            .contains(&"campaign-editor-ux".to_string())
        {
            return Err("issue 1693 must be visible under the editor campaign".to_string());
        }
        // The editor lane is available; the rust lane is collision-marked.
        let editor_lane = snapshot
            .lanes
            .iter()
            .find(|lane| lane.lane == "lane:campaign-editor-ux")
            .ok_or_else(|| "missing editor lane".to_string())?;
        let rust_lane = snapshot
            .lanes
            .iter()
            .find(|lane| lane.lane == "lane:campaign-rust-repair")
            .ok_or_else(|| "missing rust lane".to_string())?;
        if editor_lane.state != WorkLaneStateV1::Available {
            return Err(format!(
                "editor lane must be available, got {:?}",
                editor_lane.state
            ));
        }
        if rust_lane.state != WorkLaneStateV1::Conflicting {
            return Err(format!(
                "rust lane must be conflicting, got {:?}",
                rust_lane.state
            ));
        }
        Ok(())
    }

    /// Box 2: no global current/default campaign is required or synthesized.
    #[test]
    fn work_portfolio_no_global_default_campaign_required_or_synthesized() -> Result<(), String> {
        let snapshot = committed()?;
        if snapshot.campaigns.len() < 2 {
            return Err("the snapshot needs at least two campaigns for this pin".to_string());
        }
        let body = work_portfolio_json(&snapshot)?;
        for line in body.lines() {
            let lower = line.to_ascii_lowercase();
            if lower.contains("\"default\"") || lower.contains("\"current_campaign\"") {
                return Err(format!(
                    "snapshot synthesized a default/current campaign field: {line}"
                ));
            }
        }
        for observation in &snapshot.source_observations {
            if observation.source == "campaigns"
                && observation.freshness != WorkSourceFreshnessV1::Current
            {
                return Err("campaign records must be observed and current".to_string());
            }
        }
        Ok(())
    }

    /// Box 3: every candidate explains lifecycle stage, blockers, conflicts,
    /// capacity, evidence and ranking factors.
    #[test]
    fn work_portfolio_every_candidate_explains_stage_blockers_conflicts_capacity_evidence_ranking()
    -> Result<(), String> {
        let snapshot = committed()?;
        if snapshot.candidates.len() != 12 {
            return Err(format!(
                "expected 12 candidates, got {}",
                snapshot.candidates.len()
            ));
        }
        for candidate in &snapshot.candidates {
            if candidate.next_transition.trim().is_empty() {
                return Err(format!(
                    "candidate `{}` names no next durable transition",
                    candidate.candidate_id
                ));
            }
            let expected: Vec<u8> = (1..=8).collect();
            let orders: Vec<u8> = candidate
                .ranking_factors
                .iter()
                .map(|factor| factor.order)
                .collect();
            if orders != expected {
                return Err(format!(
                    "candidate `{}` ranking factors {orders:?} are not the eight ordered factors",
                    candidate.candidate_id
                ));
            }
            for factor in &candidate.ranking_factors {
                if factor.contribution.trim().is_empty() {
                    return Err(format!(
                        "candidate `{}` factor {} has an empty contribution",
                        candidate.candidate_id, factor.order
                    ));
                }
            }
            if candidate.packet_entrypoint
                != format!(
                    "cargo xtask work explain --candidate {}",
                    candidate.candidate_id
                )
            {
                return Err(format!(
                    "candidate `{}` packet entrypoint is wrong",
                    candidate.candidate_id
                ));
            }
            if candidate.confidence != WorkConfidenceV1::Complete
                && candidate.confidence_reasons.is_empty()
            {
                return Err(format!(
                    "candidate `{}` is degraded without named reasons",
                    candidate.candidate_id
                ));
            }
        }
        let blocked = candidate(&snapshot, 9104)?;
        if blocked.dependencies.is_empty() {
            return Err("the externally blocked issue must carry its blocker".to_string());
        }
        let collided = candidate(&snapshot, 9107)?;
        if collided.conflict_edges.is_empty() {
            return Err("the claim-collision candidate must name its conflict edge".to_string());
        }
        Ok(())
    }

    /// Box 4 + fixture rows 4/5/6: existing PRs and claims become
    /// resume/repair/merge/verify states, never new-build duplicates.
    #[test]
    fn work_portfolio_existing_prs_and_claims_become_resume_states_not_new_builds()
    -> Result<(), String> {
        let snapshot = committed()?;
        for (issue, expected) in [
            (9101, WorkCandidateKindV1::RepairReview),
            (9102, WorkCandidateKindV1::ResumePr),
            (9103, WorkCandidateKindV1::MergeReady),
            (9108, WorkCandidateKindV1::ResumePr),
            (9109, WorkCandidateKindV1::VerifyCurrentHead),
        ] {
            let row = candidate(&snapshot, issue)?;
            if row.kind != expected {
                return Err(format!(
                    "issue #{issue} kind {:?}, expected {:?}",
                    row.kind, expected
                ));
            }
            if row.pull_request.is_none() && row.claim.is_none() {
                return Err(format!(
                    "issue #{issue} must bind its resumable PR/claim relationship"
                ));
            }
        }
        // No candidate for an issue with an open PR may classify as a
        // new-build kind.
        for row in &snapshot.candidates {
            if row.pull_request.is_some()
                && matches!(
                    row.kind,
                    WorkCandidateKindV1::StartBuild
                        | WorkCandidateKindV1::ResearchIssue
                        | WorkCandidateKindV1::CompilePlan
                )
            {
                return Err(format!(
                    "candidate `{}` has an open PR but classifies as a new build",
                    row.candidate_id
                ));
            }
        }
        Ok(())
    }

    /// Fixture rows 3/5/16/17/18: external dependency, collisions, duplicate
    /// family and path-disjoint shared-contract visibility.
    #[test]
    fn work_portfolio_blockers_collisions_duplicate_family_and_shared_contracts_visible()
    -> Result<(), String> {
        let snapshot = committed()?;
        let external = candidate(&snapshot, 9104)?;
        if external.kind != WorkCandidateKindV1::Blocked
            || !external
                .dependencies
                .iter()
                .any(|dependency| dependency.kind == "external_dependency")
        {
            return Err("issue 9104 must be blocked on the named external dependency".to_string());
        }
        let claim_collision = snapshot
            .conflict_edges
            .iter()
            .find(|edge| edge.id == "edge:claim_collision:issue-9107")
            .ok_or_else(|| "missing the claim-collision edge".to_string())?;
        if claim_collision.subjects
            != vec![
                "claim:claim-9107-a".to_string(),
                "claim:claim-9107-b".to_string(),
            ]
        {
            return Err(format!(
                "claim-collision subjects drifted: {:?}",
                claim_collision.subjects
            ));
        }
        let branch_collision = snapshot
            .conflict_edges
            .iter()
            .find(|edge| edge.id == "edge:branch_collision:pr-8804")
            .ok_or_else(|| "missing the branch-collision edge".to_string())?;
        if !branch_collision
            .subjects
            .contains(&"branch:feat/work-9108".to_string())
        {
            return Err("branch-collision must name the unregistered branch".to_string());
        }
        let family = snapshot
            .conflict_edges
            .iter()
            .find(|edge| edge.id == "edge:duplicate_family:REQ-dup-family")
            .ok_or_else(|| "missing the duplicate-family edge".to_string())?;
        if family.subjects
            != vec![
                "candidate:issue:9105".to_string(),
                "candidate:issue:9106".to_string(),
            ]
        {
            return Err(format!(
                "duplicate-family subjects drifted: {:?}",
                family.subjects
            ));
        }
        let shared = snapshot
            .conflict_edges
            .iter()
            .find(|edge| edge.id == "edge:shared_contract:output-contract:rust-json")
            .ok_or_else(|| "missing the shared-contract edge".to_string())?;
        if !shared
            .evidence
            .iter()
            .any(|line| line.contains("path-disjoint"))
        {
            return Err("shared-contract evidence must name the path-disjoint pair".to_string());
        }
        let left = candidate(&snapshot, 9105)?;
        let right = candidate(&snapshot, 9106)?;
        if left.duplicate_family.is_none() || right.duplicate_family.is_none() {
            return Err("both duplicate-family members must carry the family identity".to_string());
        }
        Ok(())
    }

    /// Box 5 + fixture row 8: missing GitHub data, stale local data and an
    /// unavailable cargo-allow graph stay visible and lower confidence.
    #[test]
    fn work_portfolio_partial_and_unavailable_inputs_lower_confidence() -> Result<(), String> {
        let missing = compile_work_portfolio_corpus("variants/missing_github")?;
        if !missing.candidates.is_empty() {
            return Err(
                "missing GitHub data must not fabricate candidates; the degraded state stays visible"
                    .to_string(),
            );
        }
        if missing.campaigns.len() != 4 || missing.partial_data_boundaries.is_empty() {
            return Err(
                "missing GitHub data must keep campaigns visible with named boundaries".to_string(),
            );
        }
        let stale = compile_work_portfolio_corpus("variants/stale_local")?;
        for issue in [9101, 9102, 9103, 9108] {
            let row = candidate(&stale, issue)?;
            if row.confidence != WorkConfidenceV1::Partial {
                return Err(format!(
                    "stale local data must degrade issue #{issue} to partial, got {:?}",
                    row.confidence
                ));
            }
        }
        let untouched = candidate(&stale, 9202)?;
        if untouched.confidence != WorkConfidenceV1::Complete {
            return Err("issue 9202 has no local relationship and must stay complete".to_string());
        }
        let no_graph = compile_work_portfolio_corpus("variants/no_cargo_allow")?;
        for issue in [9104, 9105, 9106, 1693, 9202] {
            let row = candidate(&no_graph, issue)?;
            if row.confidence != WorkConfidenceV1::Partial {
                return Err(format!(
                    "unavailable cargo-allow graph must degrade issue #{issue} to partial, got {:?}",
                    row.confidence
                ));
            }
        }
        if !no_graph
            .partial_data_boundaries
            .iter()
            .any(|boundary| boundary.contains("cargo_allow") || boundary.contains("cargo-allow"))
        {
            return Err("the cargo-allow boundary must stay named".to_string());
        }
        Ok(())
    }

    /// Box 6: filters do not change authority or candidate identity — the
    /// filtered rows are an order-preserving subsequence of the full array
    /// with identical candidate bytes.
    #[test]
    fn work_portfolio_filters_do_not_change_authority_or_candidate_identity() -> Result<(), String>
    {
        let snapshot = committed()?;
        let full = snapshot.candidates.clone();
        for (campaign, surface, expected_total) in [
            (Some("campaign-rust-repair"), None, 9_u64),
            (Some("campaign-editor-ux"), None, 2_u64),
            (None, Some("surface-rust-cli"), 9_u64),
            (
                Some("campaign-rust-repair"),
                Some("surface-rust-cli"),
                9_u64,
            ),
            (Some("campaign-infra-hardening"), None, 0_u64),
        ] {
            let view = build_candidates_view(&snapshot, campaign, surface, full.len(), None)?;
            if view.counts.total != expected_total {
                return Err(format!(
                    "filter campaign={campaign:?} surface={surface:?} total {}, expected {expected_total}",
                    view.counts.total
                ));
            }
            let full_ids: Vec<String> = full.iter().map(|row| row.candidate_id.clone()).collect();
            let mut cursor = 0;
            for row in &view.candidates {
                let position = full_ids[cursor..]
                    .iter()
                    .position(|id| id == &row.candidate_id)
                    .ok_or_else(|| {
                        format!(
                            "filtered candidate {} broke the full-array ordering",
                            row.candidate_id
                        )
                    })?;
                cursor += position + 1;
                let original = full
                    .iter()
                    .find(|entry| entry.candidate_id == row.candidate_id)
                    .ok_or_else(|| "filtered candidate vanished from the snapshot".to_string())?;
                if original != row {
                    return Err(format!(
                        "filter changed candidate identity for {}",
                        row.candidate_id
                    ));
                }
            }
        }
        let Err(_error) = build_candidates_view(&snapshot, Some("campaign-nope"), None, 10, None)
        else {
            return Err("unknown campaign must fail closed".to_string());
        };
        Ok(())
    }

    /// Box 7: no portfolio command mutates the captured corpus, the fixture
    /// tree, or anything outside the sandbox directory. The three commands
    /// run end to end inside a temporary cwd; every byte under the sandbox
    /// before and after is compared, and the captured corpus copy must be
    /// bit-identical.
    #[test]
    fn work_portfolio_commands_mutate_nothing_mutation_negative() -> Result<(), String> {
        let _cwd_guard = crate::acquire_test_cwd_write_guard();
        let sandbox = std::env::temp_dir().join(format!(
            "work_portfolio_mutation_negative_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&sandbox);
        fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
        let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
        let restore = || std::env::set_current_dir(&original_dir);
        std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

        let result = (|| -> Result<(), String> {
            let captured_src = workspace_path(DEFAULT_WORK_CAPTURED_DIR);
            let captured_dst = sandbox.join("captured");
            copy_dir(&captured_src, &captured_dst)?;
            let before = hash_dir(&captured_dst)?;
            work_portfolio_command(&[
                "--captured".to_string(),
                "captured".to_string(),
                "--json".to_string(),
            ])?;
            work_candidates_command(&[
                "--captured".to_string(),
                "captured".to_string(),
                "--limit".to_string(),
                "3".to_string(),
                "--json".to_string(),
            ])?;
            work_explain_command(&[
                "--candidate".to_string(),
                "candidate:issue:9101".to_string(),
                "--captured".to_string(),
                "captured".to_string(),
                "--json".to_string(),
            ])?;
            let after = hash_dir(&captured_dst)?;
            if before != after {
                return Err(format!(
                    "portfolio commands mutated the captured corpus: before {before:?} after {after:?}"
                ));
            }
            // The only writes under the sandbox are the six reports; nothing
            // else may appear (no GitHub, branch, worktree, claim, spec,
            // campaign or source state is touched).
            let written = hash_dir(&sandbox)?;
            let allowed: Vec<String> = [
                "target/ripr/reports/work-portfolio.json",
                "target/ripr/reports/work-portfolio.md",
                "target/ripr/reports/work-candidates.json",
                "target/ripr/reports/work-candidates.md",
                "target/ripr/reports/work-explain.json",
                "target/ripr/reports/work-explain.md",
            ]
            .iter()
            .map(|name| name.to_string())
            .collect();
            for key in written.keys() {
                if !allowed.contains(key) && !key.starts_with("captured/") {
                    return Err(format!(
                        "portfolio command wrote outside the reports directory: {key}"
                    ));
                }
            }
            for name in &allowed {
                if !sandbox.join(name).is_file() {
                    return Err(format!("missing report {name}"));
                }
            }
            Ok(())
        })();

        let _ = restore();
        let _ = fs::remove_dir_all(&sandbox);
        result
    }

    fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
        fs::create_dir_all(dst).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(src).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let target = dst.join(entry.file_name());
            if path.is_dir() {
                copy_dir(&path, &target)?;
            } else {
                fs::copy(&path, &target).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    fn hash_dir(root: &Path) -> Result<BTreeMap<String, String>, String> {
        let mut map = BTreeMap::new();
        fn walk(root: &Path, dir: &Path, map: &mut BTreeMap<String, String>) -> Result<(), String> {
            for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                let path = entry.path();
                let key = path
                    .strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                if path.is_dir() {
                    walk(root, &path, map)?;
                } else {
                    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
                    map.insert(key, crate::blind_journey::sha256_hex(&bytes));
                }
            }
            Ok(())
        }
        walk(root, root, &mut map)?;
        Ok(map)
    }

    /// Box 8 + fixture row 9: deterministic fixture output, Windows/Unix
    /// root portability and reordered-input neutrality — byte-identical
    /// normalized JSON and one portable identity.
    #[test]
    fn work_portfolio_deterministic_fixture_and_root_portable_output() -> Result<(), String> {
        let canonical = committed()?;
        let canonical_json = work_portfolio_json(&canonical)?;
        let repeat = work_portfolio_json(&committed()?)?;
        if repeat != canonical_json {
            return Err("recompiling the same corpus must give byte-identical JSON".to_string());
        }
        for variant in ["variants/unix_root", "variants/reordered"] {
            let snapshot = compile_work_portfolio_corpus(variant)?;
            let body = work_portfolio_json(&snapshot)?;
            if body != canonical_json {
                return Err(format!(
                    "{variant} must compile to byte-identical normalized JSON"
                ));
            }
            if snapshot.portable_identity != canonical.portable_identity {
                return Err(format!("{variant} portable identity drifted"));
            }
        }
        if !canonical
            .portable_identity
            .starts_with("work-portfolio:sha256:")
        {
            return Err("portable identity must use the work-portfolio digest scheme".to_string());
        }
        Ok(())
    }

    /// Box 9: the root can choose a bounded execution wave from the default
    /// candidates view without reading every campaign or issue body — total/
    /// selected/omitted are reported with stable retrieval commands, and a
    /// larger limit retrieves the rest unchanged.
    #[test]
    fn work_portfolio_bounded_wave_retrievable_without_reading_every_body() -> Result<(), String> {
        let snapshot = committed()?;
        let view = build_candidates_view(&snapshot, None, None, DEFAULT_CANDIDATE_LIMIT, None)?;
        if view.counts.total != 12 || view.counts.selected != 10 || view.counts.omitted != 2 {
            return Err(format!(
                "bounded view counts {:?}, expected total=12 selected=10 omitted=2",
                view.counts
            ));
        }
        if view.retrieval_commands.is_empty() {
            return Err("overflow must expose stable retrieval commands".to_string());
        }
        if !view
            .retrieval_commands
            .iter()
            .any(|command| command.contains("--limit 12"))
        {
            return Err(format!(
                "retrieval commands must name the exact rerun: {:?}",
                view.retrieval_commands
            ));
        }
        let full = build_candidates_view(&snapshot, None, None, 25, None)?;
        if full.counts.selected != 12 || full.counts.omitted != 0 {
            return Err("the larger limit must retrieve the complete set".to_string());
        }
        let scoped = build_candidates_view(
            &snapshot,
            None,
            None,
            1,
            Some("fixtures/work_portfolio/corpus"),
        )?;
        if scoped.counts.omitted == 0
            || !scoped
                .retrieval_commands
                .iter()
                .all(|command| command.contains("--captured fixtures/work_portfolio/corpus"))
        {
            return Err(format!(
                "every retrieval command must preserve the caller's captured directory: {:?}",
                scoped.retrieval_commands
            ));
        }
        let default_ids: Vec<String> = view
            .candidates
            .iter()
            .map(|row| row.candidate_id.clone())
            .collect();
        let full_ids: Vec<String> = full
            .candidates
            .iter()
            .take(default_ids.len())
            .map(|row| row.candidate_id.clone())
            .collect();
        if default_ids != full_ids {
            return Err("the bounded wave must be the prefix of the full ranked array".to_string());
        }
        Ok(())
    }

    /// Box 10 + fixture rows 10/12: a blocked, complete or low-ranked
    /// campaign cannot hide unrelated eligible work; the single-agent
    /// narrow task stays first-class.
    #[test]
    fn work_portfolio_blocked_complete_or_low_ranked_campaign_never_hides_eligible_work()
    -> Result<(), String> {
        let snapshot = committed()?;
        let last = snapshot
            .candidates
            .last()
            .ok_or_else(|| "snapshot must carry candidates".to_string())?;
        if last.issue != 9104 || last.kind != WorkCandidateKindV1::Blocked {
            return Err("the externally blocked candidate must rank last".to_string());
        }
        let complete = candidate(&snapshot, 9301)?;
        if complete.kind != WorkCandidateKindV1::Complete {
            return Err("the complete campaign's candidate must classify complete".to_string());
        }
        // The complete candidate stays visible and the editor campaign's
        // eligible work is not pushed out of the bounded default wave by the
        // blocked/complete rows.
        let view = build_candidates_view(&snapshot, None, None, DEFAULT_CANDIDATE_LIMIT, None)?;
        let ids: Vec<String> = view
            .candidates
            .iter()
            .map(|row| row.candidate_id.clone())
            .collect();
        for required in ["candidate:issue:1693", "candidate:issue:9202"] {
            if !ids.iter().any(|id| id == required) {
                return Err(format!("bounded wave lost eligible work {required}"));
            }
        }
        let narrow = candidate(&snapshot, 9202)?;
        if !narrow.single_agent_preferred {
            return Err("issue 9202 must carry the single_agent_preferred flag".to_string());
        }
        let lane_factor = factor(
            narrow,
            WorkRankingFactorKindV1::LaneCapacityAndCollisionCost,
        )?;
        if !lane_factor
            .contribution
            .contains("orchestration cost dominates")
        {
            return Err(
                "the single-agent-preferred task must keep its orchestration-cost note visible"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// Fixture rows 12 + ranking law: the eight factors are explicit,
    /// ordered and inspectable; no hidden numeric score exists in the DTO.
    #[test]
    fn work_portfolio_ranking_is_explicit_ordered_factors_without_hidden_score()
    -> Result<(), String> {
        let snapshot = committed()?;
        let body = work_portfolio_json(&snapshot)?;
        for banned in ["\"score\"", "\"rank_score\"", "\"weight\""] {
            if body.contains(banned) {
                return Err(format!("snapshot carries a hidden ranking field {banned}"));
            }
        }
        for candidate in &snapshot.candidates {
            let orders: Vec<u8> = candidate
                .ranking_factors
                .iter()
                .map(|factor| factor.order)
                .collect();
            if orders != (1..=8).collect::<Vec<u8>>() {
                return Err(format!(
                    "candidate {} factors are not exactly the eight ordered factors",
                    candidate.candidate_id
                ));
            }
        }
        // Deterministic advisory order: the merge-ready and repair-review
        // rows lead the resumable group; the externally blocked row is last.
        let ids: Vec<String> = snapshot
            .candidates
            .iter()
            .map(|row| row.candidate_id.clone())
            .collect();
        let expected = [
            "candidate:issue:9101",
            "candidate:issue:9103",
            "candidate:issue:9102",
            "candidate:issue:9107",
            "candidate:issue:9108",
            "candidate:issue:1693",
            "candidate:issue:9105",
            "candidate:issue:9106",
            "candidate:issue:9109",
            "candidate:issue:9202",
            "candidate:issue:9301",
            "candidate:issue:9104",
        ];
        if ids != expected {
            return Err(format!("ranked order {ids:?} drifted from {expected:?}"));
        }
        Ok(())
    }

    /// The explain view derives from the same snapshot DTO and fails closed
    /// on unknown ids with the stable candidate list.
    #[test]
    fn work_portfolio_explain_view_matches_snapshot_and_fails_closed() -> Result<(), String> {
        let snapshot = committed()?;
        let view = build_explain_view(&snapshot, "candidate:issue:9101")?;
        let original = candidate(&snapshot, 9101)?;
        if view.candidate != *original {
            return Err("explain view must derive from the same candidate DTO".to_string());
        }
        if view.portfolio_identity != snapshot.portable_identity {
            return Err("explain view must bind the portfolio identity".to_string());
        }
        let Err(error) = build_explain_view(&snapshot, "candidate:issue:9999") else {
            return Err("unknown candidate must fail closed".to_string());
        };
        if !error.contains("candidate:issue:9101") {
            return Err(format!(
                "unknown-candidate error must list the stable ids, got: {error}"
            ));
        }
        Ok(())
    }

    /// Human Markdown derives from the same DTO and cannot strengthen
    /// readiness or ranking: a degraded candidate stays degraded in prose.
    #[test]
    fn work_portfolio_markdown_derives_from_same_dto_without_strengthening() -> Result<(), String> {
        let snapshot = committed()?;
        let markdown = work_portfolio_markdown(&snapshot);
        for needle in [
            "candidate:issue:9101",
            "repair_review",
            "work-portfolio:sha256:",
            "partial",
            "claim boundary",
        ] {
            if !markdown.contains(needle) {
                return Err(format!("portfolio markdown lost `{needle}`"));
            }
        }
        let view = build_explain_view(&snapshot, "candidate:issue:9107")?;
        let explain = work_explain_markdown(&view);
        // The rendered prose carries the snake_case wire identity of the
        // conflict edge (`edge:claim_collision:issue-9107`), not a kebab-case
        // label; the needle must match what the contract actually renders.
        if !explain.contains("partial") || !explain.contains("edge:claim_collision:issue-9107") {
            return Err(format!(
                "the degraded candidate must stay degraded in the human rendering: {explain}"
            ));
        }
        let candidates_md =
            work_candidates_markdown(&build_candidates_view(&snapshot, None, None, 3, None)?);
        if !candidates_md.contains("omitted=9") {
            return Err("candidates markdown must report the omitted count honestly".to_string());
        }
        Ok(())
    }

    /// The committed corpus satisfies the provenance digest bindings and the
    /// twelve pinned scenarios; the fixture-contract gate stays green.
    #[test]
    fn work_portfolio_committed_corpus_provenance_and_scenarios_hold() -> Result<(), String> {
        let mut violations = Vec::new();
        validate_work_portfolio_fixture_corpus(&mut violations);
        if !violations.is_empty() {
            return Err(format!(
                "committed corpus failed validation: {violations:?}"
            ));
        }
        Ok(())
    }

    /// Unknown flags and missing values fail closed with usage.
    #[test]
    fn work_portfolio_argument_errors_fail_closed() -> Result<(), String> {
        let Err(_error) = run_work_command_for_test(&["portfolio", "--bogus"]) else {
            return Err("unknown portfolio flag must fail closed".to_string());
        };
        let Err(_error) = run_work_command_for_test(&["explain"]) else {
            return Err("explain without --candidate must fail closed".to_string());
        };
        let Err(_error) = run_work_command_for_test(&["candidates", "--limit", "nope"]) else {
            return Err("a non-numeric limit must fail closed".to_string());
        };
        let Err(_error) = run_work_command_for_test(&["portfolio", "--captured=/tmp/x"]) else {
            return Err(
                "a glued --captured value must fail closed, not read the default corpus"
                    .to_string(),
            );
        };
        let Err(_error) = run_work_command_for_test(&["portfolio", "--captured-old"]) else {
            return Err(
                "a --captured typo must fail closed, not read the default corpus".to_string(),
            );
        };
        Ok(())
    }

    /// Missing PR or claim sources must degrade a candidate that shows no
    /// captured ownership: absence of the source never reports complete
    /// confidence for a fresh-build recommendation.
    #[test]
    fn work_portfolio_missing_ownership_sources_lower_confidence() -> Result<(), String> {
        let mut freshness = BTreeMap::new();
        for kind in WorkCapturedSourceKindV1::all() {
            freshness.insert(kind, WorkSourceFreshnessV1::Current);
        }
        freshness.insert(
            WorkCapturedSourceKindV1::GithubPullRequests,
            WorkSourceFreshnessV1::Unknown,
        );
        freshness.insert(
            WorkCapturedSourceKindV1::GithubClaims,
            WorkSourceFreshnessV1::Unknown,
        );
        let context = WorkCompileContext { freshness };
        let issue = WorkCapturedIssueV1 {
            number: 7002,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            lifecycle_disposition: None,
            campaigns: vec!["campaign-synthetic".to_string()],
            blocked_by: Vec::new(),
            requirement_refs: Vec::new(),
            semantic_paths: Vec::new(),
            conflict_resources: Vec::new(),
            accepted_contracts: Vec::new(),
            contract_state: "accepted".to_string(),
            single_agent_preferred: false,
            honesty_risk: None,
            proof_cost: None,
            review_ci_cost: None,
            readiness_source: None,
            regression_risks: Vec::new(),
            lane: None,
        };
        let no_claims: Vec<&WorkCapturedClaimV1> = Vec::new();
        let reasons = confidence_reasons_for(&context, &issue, None, &no_claims, Vec::new(), &[]);
        if !reasons
            .iter()
            .any(|reason| reason.contains("pull-request source is not current"))
        {
            return Err(format!(
                "missing PR source must lower confidence for an uncaptured PR: {reasons:?}"
            ));
        }
        if !reasons
            .iter()
            .any(|reason| reason.contains("claim source is not current"))
        {
            return Err(format!(
                "missing claim source must lower confidence for an uncaptured claim: {reasons:?}"
            ));
        }
        Ok(())
    }

    /// An open PR on a lifecycle-completed issue keeps the lane in flight:
    /// unresolved findings or pending checks must never be hidden behind a
    /// terminal `complete` disposition.
    #[test]
    fn work_portfolio_completed_issue_with_open_pr_stays_in_flight() -> Result<(), String> {
        let issue = WorkCapturedIssueV1 {
            number: 7003,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            lifecycle_disposition: Some(IssueLifecycleDispositionV1::Completed),
            campaigns: vec!["campaign-synthetic".to_string()],
            blocked_by: Vec::new(),
            requirement_refs: Vec::new(),
            semantic_paths: Vec::new(),
            conflict_resources: Vec::new(),
            accepted_contracts: Vec::new(),
            contract_state: "accepted".to_string(),
            single_agent_preferred: false,
            honesty_risk: None,
            proof_cost: None,
            review_ci_cost: None,
            readiness_source: None,
            regression_risks: Vec::new(),
            lane: None,
        };
        let pr = WorkCapturedPullRequestV1 {
            number: 7901,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            draft: false,
            head_branch: "feat/synthetic".to_string(),
            base_branch: "main".to_string(),
            linked_issues: vec![7003],
            review_state: "changes_requested".to_string(),
            unresolved_review_findings: 1,
            checks_state: "success".to_string(),
            registered_claim: None,
            worktree_path: None,
        };
        let (kind, _transition) = classify_candidate(
            &issue,
            &[&pr],
            &[],
            &[],
            false,
            false,
            false,
            true,
            "abcdef0123456789abcdef0123456789abcdef01",
        );
        if kind == WorkCandidateKindV1::Complete {
            return Err(
                "an open PR with unresolved findings on a completed issue must stay in flight"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// A cargo-allow slice only promotes `start_build` when it sits under a
    /// requirement the issue structurally references; a slice listed under
    /// another requirement is incomplete evidence.
    #[test]
    fn work_portfolio_slice_requires_requirement_membership() -> Result<(), String> {
        let requirement = |id: &str, issue: u64| WorkCapturedRequirementV1 {
            id: id.to_string(),
            spec_refs: Vec::new(),
            slices: vec![WorkCapturedSliceV1 {
                id: format!("slice-{issue}"),
                delta_id: "DELTA-x".to_string(),
                issue_refs: vec![issue],
            }],
        };
        let graph = WorkCapturedCargoAllowV1 {
            schema_version: "work_cargo_allow.v1".to_string(),
            requirements: vec![
                requirement("REQ-owned", 7004),
                requirement("REQ-other", 7004),
            ],
        };
        let mut issue = WorkCapturedIssueV1 {
            number: 7004,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            lifecycle_disposition: None,
            campaigns: vec!["campaign-synthetic".to_string()],
            blocked_by: Vec::new(),
            requirement_refs: vec!["REQ-owned".to_string()],
            semantic_paths: Vec::new(),
            conflict_resources: Vec::new(),
            accepted_contracts: Vec::new(),
            contract_state: "accepted".to_string(),
            single_agent_preferred: false,
            honesty_risk: None,
            proof_cost: None,
            review_ci_cost: None,
            readiness_source: None,
            regression_risks: Vec::new(),
            lane: None,
        };
        if !issue_has_accepted_slice(&graph, &issue) {
            return Err("the referenced requirement's slice must count".to_string());
        }
        issue.requirement_refs = vec!["REQ-absent".to_string()];
        if issue_has_accepted_slice(&graph, &issue) {
            return Err(
                "a slice under an unreferenced requirement must not count as accepted evidence"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// A PR is only exempt from branch-collision handling when the active
    /// claim on its branch belongs to one of the PR's own linked issues.
    #[test]
    fn work_portfolio_cross_issue_branch_claim_stays_conflict_visible() -> Result<(), String> {
        let pr = WorkCapturedPullRequestV1 {
            number: 7902,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            draft: false,
            head_branch: "feat/shared".to_string(),
            base_branch: "main".to_string(),
            linked_issues: vec![7005],
            review_state: "none".to_string(),
            unresolved_review_findings: 0,
            checks_state: "pending".to_string(),
            registered_claim: None,
            worktree_path: None,
        };
        let make_claim = |issue: Option<u64>| WorkCapturedClaimV1 {
            id: "claim-x".to_string(),
            issue,
            branch: "feat/shared".to_string(),
            worktree: None,
            exclusive: true,
            state: "active".to_string(),
        };
        if !same_issue_branch_claim_exists(&pr, &[make_claim(Some(7005))]) {
            return Err("a same-issue claim must exempt the PR".to_string());
        }
        if same_issue_branch_claim_exists(&pr, &[make_claim(Some(7006))]) {
            return Err(
                "a different issue's claim on the same branch must stay conflict-visible"
                    .to_string(),
            );
        }
        if same_issue_branch_claim_exists(&pr, &[make_claim(None)]) {
            return Err("an issue-less claim must not exempt the PR".to_string());
        }
        Ok(())
    }

    /// Reordered semantically-unordered metadata canonicalizes to identical
    /// bytes, so normalized JSON and portable identity stay stable.
    #[test]
    fn work_portfolio_candidate_metadata_arrays_are_canonical() -> Result<(), String> {
        let strings = |values: &[&str]| values.iter().map(|value| value.to_string()).collect();
        let left: Vec<String> = strings(&["b-risk", "a-risk", "b-risk"]);
        let right: Vec<String> = strings(&["a-risk", "b-risk"]);
        if canonical_strings(&left) != right {
            return Err("string metadata must sort and dedup".to_string());
        }
        let blocker = |reference: &str| WorkCapturedBlockerV1 {
            kind: "external_dependency".to_string(),
            reference: reference.to_string(),
            description: "d".to_string(),
        };
        let left_blockers = vec![blocker("b"), blocker("a"), blocker("b")];
        let right_blockers = vec![blocker("a"), blocker("b")];
        if canonical_blockers(&left_blockers) != right_blockers {
            return Err("blocker metadata must sort and dedup by structural identity".to_string());
        }
        Ok(())
    }

    /// Freshness compares parsed instants, not lexicographic text: offset
    /// timestamps that predate the capture in UTC and malformed stamps both
    /// fail closed to stale instead of reading as current.
    #[test]
    fn work_portfolio_timestamp_freshness_parses_instants() -> Result<(), String> {
        if parse_rfc3339_utc("2026-10-05T12:00:00Z")
            != parse_rfc3339_utc("2026-10-05T14:00:00+02:00")
        {
            return Err("equivalent instants must parse equal".to_string());
        }
        // 13:00+02:00 == 11:00Z, strictly before the 12:00Z capture.
        if parse_rfc3339_utc("2026-10-05T13:00:00+02:00")
            >= parse_rfc3339_utc("2026-10-05T12:00:00Z")
        {
            return Err("an earlier instant must not compare current".to_string());
        }
        for malformed in [
            "2026-10-05T12:00:00",
            "not-a-timestamp",
            "2026-10-05T12:00:00Q",
            "2026-13-05T12:00:00Z",
        ] {
            if parse_rfc3339_utc(malformed).is_some() {
                return Err(format!("malformed stamp `{malformed}` must fail closed"));
            }
        }
        Ok(())
    }

    /// Memberships derive from the campaign records even when the issue's
    /// own campaign list disagrees or is empty.
    #[test]
    fn work_portfolio_memberships_derive_from_campaign_records() -> Result<(), String> {
        let campaign_map: BTreeMap<String, Vec<u64>> = BTreeMap::from([
            ("campaign-a".to_string(), vec![7007]),
            ("campaign-b".to_string(), vec![7007, 7008]),
        ]);
        if campaign_memberships(&campaign_map, 7007) != vec!["campaign-a", "campaign-b"] {
            return Err("both campaigns listing the issue must be derived".to_string());
        }
        if campaign_memberships(&campaign_map, 7008) != vec!["campaign-b"] {
            return Err("only the listing campaign must be derived".to_string());
        }
        Ok(())
    }

    /// Lane collision edges bind by exact candidate identity: issue 910's
    /// lane must not advertise issue 9101's edge through prefix matching.
    #[test]
    fn work_portfolio_collision_subjects_match_exact_issue_identity() -> Result<(), String> {
        let edge = WorkConflictEdgeV1 {
            id: "edge:claim_collision:x".to_string(),
            kind: WorkConflictEdgeKindV1::ClaimCollision,
            subjects: vec!["candidate:issue:9101".to_string()],
            evidence: Vec::new(),
            note: String::new(),
        };
        if lane_collision_edge(std::slice::from_ref(&edge), &[910]).is_some() {
            return Err("issue 910 must not inherit issue 9101's collision".to_string());
        }
        if lane_collision_edge(&[edge], &[9101]).as_deref() != Some("edge:claim_collision:x") {
            return Err("issue 9101 must see its own collision edge".to_string());
        }
        Ok(())
    }

    /// Generated retrieval commands shell-escape arguments with spaces or
    /// metacharacters so the advertised command replays exactly.
    #[test]
    fn work_portfolio_retrieval_commands_escape_arguments() -> Result<(), String> {
        let snapshot = committed()?;
        let view = build_candidates_view(&snapshot, None, None, 3, Some("/tmp/my corpus"))?;
        let command = view
            .retrieval_commands
            .first()
            .ok_or_else(|| "a bounded view must advertise a retrieval command".to_string())?;
        if !command.contains("--captured '/tmp/my corpus'") {
            return Err(format!("captured dir must be quoted, got: {command}"));
        }
        if shell_escape_arg("campaign-rust-repair") != "campaign-rust-repair" {
            return Err("simple tokens must stay bare for byte-stable output".to_string());
        }
        if shell_escape_arg("it's") != "'it'\\''s'" {
            return Err("embedded quotes must use the single-quote escape".to_string());
        }
        Ok(())
    }

    /// Multiple open PRs on one issue can never emit an actionable
    /// merge-ready state from the lowest-numbered PR alone; the root must
    /// resolve which PR is current first.
    #[test]
    fn work_portfolio_multiple_open_prs_never_merge_ready() -> Result<(), String> {
        let issue = WorkCapturedIssueV1 {
            number: 7009,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            lifecycle_disposition: None,
            campaigns: vec!["campaign-synthetic".to_string()],
            blocked_by: Vec::new(),
            requirement_refs: Vec::new(),
            semantic_paths: Vec::new(),
            conflict_resources: Vec::new(),
            accepted_contracts: Vec::new(),
            contract_state: "accepted".to_string(),
            single_agent_preferred: false,
            honesty_risk: None,
            proof_cost: None,
            review_ci_cost: None,
            readiness_source: None,
            regression_risks: Vec::new(),
            lane: None,
        };
        let make_pr = |number: u64| WorkCapturedPullRequestV1 {
            number,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            draft: false,
            head_branch: format!("feat/synthetic-{number}"),
            base_branch: "main".to_string(),
            linked_issues: vec![7009],
            review_state: "approved".to_string(),
            unresolved_review_findings: 0,
            checks_state: "success".to_string(),
            registered_claim: None,
            worktree_path: None,
        };
        let first = make_pr(7901);
        let second = make_pr(7902);
        let (kind, _transition) = classify_candidate(
            &issue,
            &[&first, &second],
            &[],
            &[],
            false,
            false,
            false,
            true,
            "abcdef0123456789abcdef0123456789abcdef01",
        );
        if kind == WorkCandidateKindV1::MergeReady || kind == WorkCandidateKindV1::RepairReview {
            return Err(
                "multiple open PRs must demote to a non-actionable state, never merge-ready"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// A stale campaign source lowers every candidate's confidence because
    /// portfolio membership itself derives from the campaign records.
    #[test]
    fn work_portfolio_stale_campaign_source_lowers_confidence() -> Result<(), String> {
        let mut freshness = BTreeMap::new();
        for kind in WorkCapturedSourceKindV1::all() {
            freshness.insert(kind, WorkSourceFreshnessV1::Current);
        }
        freshness.insert(
            WorkCapturedSourceKindV1::Campaigns,
            WorkSourceFreshnessV1::Stale,
        );
        let context = WorkCompileContext { freshness };
        let issue = WorkCapturedIssueV1 {
            number: 7010,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            lifecycle_disposition: None,
            campaigns: vec!["campaign-synthetic".to_string()],
            blocked_by: Vec::new(),
            requirement_refs: Vec::new(),
            semantic_paths: Vec::new(),
            conflict_resources: Vec::new(),
            accepted_contracts: Vec::new(),
            contract_state: "accepted".to_string(),
            single_agent_preferred: false,
            honesty_risk: None,
            proof_cost: None,
            review_ci_cost: None,
            readiness_source: None,
            regression_risks: Vec::new(),
            lane: None,
        };
        let no_claims: Vec<&WorkCapturedClaimV1> = Vec::new();
        let reasons = confidence_reasons_for(&context, &issue, None, &no_claims, Vec::new(), &[]);
        if !reasons
            .iter()
            .any(|reason| reason.contains("campaign source is not current"))
        {
            return Err(format!(
                "a stale campaign source must lower confidence: {reasons:?}"
            ));
        }
        Ok(())
    }

    /// A readiness contribution requires captured acceptance evidence: the
    /// contract must be accepted and the named artifact must appear in the
    /// issue's accepted contracts, so an unchecked string never manufactures
    /// accepted readiness.
    #[test]
    fn work_portfolio_readiness_requires_captured_acceptance() -> Result<(), String> {
        let mut issue = WorkCapturedIssueV1 {
            number: 7011,
            title: "synthetic".to_string(),
            state: "open".to_string(),
            lifecycle_disposition: None,
            campaigns: vec!["campaign-synthetic".to_string()],
            blocked_by: Vec::new(),
            requirement_refs: Vec::new(),
            semantic_paths: Vec::new(),
            conflict_resources: Vec::new(),
            accepted_contracts: vec!["RIPR-SPEC-0999".to_string()],
            contract_state: "accepted".to_string(),
            single_agent_preferred: false,
            honesty_risk: None,
            proof_cost: None,
            review_ci_cost: None,
            readiness_source: Some("RIPR-SPEC-0999".to_string()),
            regression_risks: Vec::new(),
            lane: None,
        };
        if readiness_for(&issue).is_none() {
            return Err("accepted evidence must emit the readiness contribution".to_string());
        }
        issue.contract_state = "draft".to_string();
        if readiness_for(&issue).is_some() {
            return Err("a draft contract must not emit readiness".to_string());
        }
        issue.contract_state = "accepted".to_string();
        issue.readiness_source = Some("RIPR-SPEC-5000".to_string());
        if readiness_for(&issue).is_some() {
            return Err("readiness for an unaccepted artifact must fail closed".to_string());
        }
        Ok(())
    }

    /// Rendered source observations retain the captured request id; the
    /// portable identity digest strips it as volatile.
    #[test]
    fn work_portfolio_observations_retain_request_ids() -> Result<(), String> {
        let snapshot = committed()?;
        if snapshot
            .source_observations
            .iter()
            .any(|observation| observation.request_id.is_none())
        {
            return Err("every captured observation must retain its request id".to_string());
        }
        Ok(())
    }

    /// Packet entrypoints produced from an explicit corpus carry that corpus
    /// forward so the advertised command explains the candidate just
    /// rendered; default-corpus entrypoints stay bare and the volatile path
    /// never enters the portable identity.
    #[test]
    fn work_portfolio_packet_entrypoint_preserves_captured_corpus() -> Result<(), String> {
        let root = workspace_path("fixtures/work_portfolio/corpus");
        let captured = load_work_captured_dir(&root)?;
        let default_snapshot = compile_work_portfolio(&captured, None)?;
        let explicit_snapshot =
            compile_work_portfolio(&captured, Some("fixtures/work_portfolio/corpus"))?;
        let Some(candidate) = explicit_snapshot.candidates.first() else {
            return Err("the corpus must produce candidates".to_string());
        };
        let expected = format!(
            "cargo xtask work explain --candidate {} --captured fixtures/work_portfolio/corpus",
            candidate.candidate_id
        );
        if candidate.packet_entrypoint != expected {
            return Err(format!(
                "entrypoint must preserve the explicit corpus: {}",
                candidate.packet_entrypoint
            ));
        }
        if default_snapshot.portable_identity != explicit_snapshot.portable_identity {
            return Err(
                "the volatile captured path must stay out of portable identity".to_string(),
            );
        }
        Ok(())
    }

    /// `merge_ready` is an actionable signal and must never fire on stale PR
    /// evidence or while durable claims on the issue still collide; both
    /// demote to non-merge states with an explicit re-verification step.
    #[test]
    fn work_portfolio_merge_ready_requires_fresh_evidence_and_clear_claims() -> Result<(), String> {
        fn captured_issue() -> WorkCapturedIssueV1 {
            WorkCapturedIssueV1 {
                number: 7001,
                title: "synthetic".to_string(),
                state: "open".to_string(),
                lifecycle_disposition: None,
                campaigns: vec!["campaign-synthetic".to_string()],
                blocked_by: Vec::new(),
                requirement_refs: Vec::new(),
                semantic_paths: Vec::new(),
                conflict_resources: Vec::new(),
                accepted_contracts: Vec::new(),
                contract_state: "accepted".to_string(),
                single_agent_preferred: false,
                honesty_risk: None,
                proof_cost: None,
                review_ci_cost: None,
                readiness_source: None,
                regression_risks: Vec::new(),
                lane: None,
            }
        }
        fn approved_pr() -> WorkCapturedPullRequestV1 {
            WorkCapturedPullRequestV1 {
                number: 8001,
                title: "synthetic".to_string(),
                state: "open".to_string(),
                draft: false,
                head_branch: "feat/synthetic".to_string(),
                base_branch: "main".to_string(),
                linked_issues: vec![7001],
                registered_claim: None,
                review_state: "approved".to_string(),
                unresolved_review_findings: 0,
                checks_state: "success".to_string(),
                worktree_path: None,
            }
        }
        let approved = approved_pr();
        let open = vec![&approved];
        let empty_prs: Vec<&WorkCapturedPullRequestV1> = Vec::new();
        let no_claims: Vec<&WorkCapturedClaimV1> = Vec::new();
        let (kind, _) = classify_candidate(
            &captured_issue(),
            &open,
            &empty_prs,
            &no_claims,
            false,
            false,
            true,
            true,
            "abc",
        );
        if kind != WorkCandidateKindV1::MergeReady {
            return Err(format!(
                "fresh approved green PR with no collisions must be merge_ready, got {kind:?}"
            ));
        }
        let (kind, transition) = classify_candidate(
            &captured_issue(),
            &open,
            &empty_prs,
            &no_claims,
            false,
            false,
            true,
            false,
            "abc",
        );
        if kind != WorkCandidateKindV1::VerifyCurrentHead || !transition.contains("published head")
        {
            return Err(format!(
                "stale PR evidence must demote merge_ready to current-head verification, got {kind:?}"
            ));
        }
        let (kind, _) = classify_candidate(
            &captured_issue(),
            &open,
            &empty_prs,
            &no_claims,
            true,
            false,
            true,
            true,
            "abc",
        );
        if kind != WorkCandidateKindV1::Blocked {
            return Err(format!(
                "an unresolved claim collision must block the merge path, got {kind:?}"
            ));
        }
        Ok(())
    }

    /// Provenance corpus names and file paths must stay relative to the
    /// corpus root: absolute paths and `..` components fail closed before
    /// the validator ever joins them onto the workspace.
    #[test]
    fn work_portfolio_provenance_rejects_path_traversal() -> Result<(), String> {
        let sha = "0".repeat(64);
        let template = format!(
            r##"{{
            "schema_version": "work_portfolio_provenance.v1",
            "repository": "EffortlessMetrics/ripr-swarm",
            "captured_at": "2026-10-05T12:00:00Z",
            "capture_method": "test",
            "corpora": [
                {{
                    "name": "@NAME@",
                    "files": [
                        {{ "path": "@PATH@", "sha256": "{sha}" }}
                    ]
                }}
            ]
        }}"##
        );
        for (name, path) in [
            ("../escape", "campaigns.json"),
            ("corpus", "/abs/campaigns.json"),
            ("corpus", "sub/../../campaigns.json"),
        ] {
            let body = template.replace("@NAME@", name).replace("@PATH@", path);
            if load_work_portfolio_provenance(&body).is_ok() {
                return Err(format!(
                    "provenance with name `{name}` and path `{path}` must fail closed"
                ));
            }
        }
        Ok(())
    }

    /// Route the work subcommands the same way dispatch does. Handlers print
    /// to stdout, which the test harness captures; only the Result matters.
    fn run_work_command_for_test(args: &[&str]) -> Result<(), String> {
        let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        match owned.first().map(|arg| arg.as_str()) {
            Some("portfolio") => work_portfolio_command(&owned[1..]),
            Some("candidates") => work_candidates_command(&owned[1..]),
            Some("explain") => work_explain_command(&owned[1..]),
            _ => Err("unknown work subcommand".to_string()),
        }
    }
}
