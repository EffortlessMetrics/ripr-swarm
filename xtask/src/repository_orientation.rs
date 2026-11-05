//! Cold-start repository orientation integration proof (#2331).
//!
//! `cargo test -p xtask repository_orientation` exercises committed corpora that
//! pin how a fresh reader recovers authority, limitations, overlap rules, and one
//! bounded next action without `.ripr/goals`, live chat, or fabricated GitHub
//! state. Selection law: this module selects no work in production paths beyond
//! the receipt's advisory `selected_next_action`; it mutates no GitHub state,
//! branches, issues, or tracked files during orientation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::current_state::{
    CURRENT_STATE_SCHEMA_VERSION, DEVELOPMENT_AUTHORITY, RELEASE_AUTHORITY, produce_current_state,
    release_authority_issue,
};
use crate::issue_lifecycle_attempt::IssueLifecycleDispositionV1;
use crate::work_portfolio::{
    WorkCandidateKindV1, WorkCandidateV1, WorkPortfolioSnapshotV1, compile_work_portfolio_corpus,
};

pub(crate) const REPOSITORY_ORIENTATION_PROVENANCE_SCHEMA_VERSION: &str =
    "repository_orientation_provenance.v1";

pub(crate) const REPOSITORY_ORIENTATION_CORPUS_SCHEMA_VERSION: &str =
    "repository_orientation_corpus.v1";
pub(crate) const REPOSITORY_ORIENTATION_RECEIPT_SCHEMA_VERSION: &str =
    "repository_orientation_receipt.v1";
pub(crate) const DEFAULT_REPOSITORY_ORIENTATION_CORPUS_DIR: &str =
    "fixtures/repository_orientation";

pub(crate) const REPOSITORY_ORIENTATION_CLAIM_BOUNDARY: &str = "Read-only orientation \
 receipt: recovers authority, limitations, overlap posture, and one bounded next action \
 from retained artifacts plus captured inputs; it is not execution authority and does not \
 prove implementation readiness.";

const HISTORICAL_CAMPAIGN_MARKERS: [&str; 2] = [
    "Historical Operating Sequence",
    "historical release-control campaign",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LiveGithubAvailabilityV1 {
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GeneratedInventoryAvailabilityV1 {
    Available,
    Stale,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrientationDispositionV1 {
    BoundedNextAction,
    DuplicateBlocked,
    PartialAcceptance,
    HistoricalOnly,
    SourceReleaseRoute,
    NoSafeWork,
    SupportClaimRejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepositoryOrientationExpectV1 {
    pub disposition: OrientationDispositionV1,
    pub selected_issue: Option<u64>,
    pub requires_goals_machinery: bool,
    pub development_authority: String,
    pub release_authority: String,
    pub limitations: Vec<String>,
    pub conflict_resources: Vec<String>,
    pub stale_item_refused: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepositoryOrientationScenarioV1 {
    pub id: String,
    pub category: String,
    pub portfolio_corpus: String,
    pub issue_number: Option<u64>,
    pub live_github: LiveGithubAvailabilityV1,
    pub inventory: GeneratedInventoryAvailabilityV1,
    pub historical_campaign_ref: Option<String>,
    pub source_release_issue: Option<u64>,
    pub support_tier_overlay: Option<String>,
    pub expected: RepositoryOrientationExpectV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepositoryOrientationCorpusV1 {
    pub schema_version: String,
    pub issue: u64,
    pub scenarios: Vec<RepositoryOrientationScenarioV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepositoryOrientationReceiptV1 {
    pub schema_version: String,
    pub repository: String,
    pub retained_artifact_identities: BTreeMap<String, String>,
    pub generated_inventory_identity: Option<String>,
    pub live_github_identity: Option<String>,
    pub authority_map: BTreeMap<String, String>,
    pub disposition: OrientationDispositionV1,
    pub selected_issue: Option<u64>,
    pub duplicate_overlap_checks: Vec<String>,
    pub conflict_resources: Vec<String>,
    pub governing_contracts: Vec<String>,
    pub proof_commands: Vec<String>,
    pub limitations: Vec<String>,
    pub non_claims: Vec<String>,
    pub claim_boundary: String,
}

/// The nine pinned scenarios of #2331. The set is exact: a duplicate,
/// substituted or extra scenario must fail instead of passing a length check.
const REQUIRED_SCENARIO_IDS: [&str; 9] = [
    "open-issue-no-overlap",
    "partially-landed-umbrella",
    "closed-premise-historical-doc",
    "historical-campaign-no-authority",
    "source-release-owned-by-ripr",
    "support-tier-backed-by-proof",
    "support-tier-inconsistency-rejected",
    "generated-inventory-stale",
    "live-github-unavailable",
];

pub(crate) fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

pub(crate) fn load_repository_orientation_corpus(
    body: &str,
) -> Result<RepositoryOrientationCorpusV1, String> {
    let corpus: RepositoryOrientationCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse repository orientation corpus: {error}"))?;
    if corpus.schema_version != REPOSITORY_ORIENTATION_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported repository orientation corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.issue != 2331 {
        return Err("repository orientation corpus must anchor issue 2331".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    for scenario in &corpus.scenarios {
        if !seen.insert(scenario.id.as_str()) {
            return Err(format!(
                "repository orientation corpus repeats scenario id `{}`",
                scenario.id
            ));
        }
    }
    for required in REQUIRED_SCENARIO_IDS {
        if !seen.contains(required) {
            return Err(format!(
                "repository orientation corpus is missing required scenario `{required}`"
            ));
        }
    }
    if let Some(extra) = corpus
        .scenarios
        .iter()
        .find(|scenario| !REQUIRED_SCENARIO_IDS.contains(&scenario.id.as_str()))
    {
        return Err(format!(
            "repository orientation corpus carries unexpected scenario `{}`; the pinned set is exact",
            extra.id
        ));
    }
    Ok(corpus)
}

fn inventory_identity(
    root: &Path,
    availability: GeneratedInventoryAvailabilityV1,
) -> Result<(Option<String>, Vec<String>), String> {
    let mut limitations = Vec::new();
    match availability {
        GeneratedInventoryAvailabilityV1::Unavailable => {
            limitations.push("generated_inventory_unavailable".to_string());
            Ok((None, limitations))
        }
        GeneratedInventoryAvailabilityV1::Stale => {
            let _inventory = produce_current_state(root)?;
            limitations.push("generated_inventory_stale".to_string());
            Ok((
                Some(format!("{CURRENT_STATE_SCHEMA_VERSION}:stale:deadbeef")),
                limitations,
            ))
        }
        GeneratedInventoryAvailabilityV1::Available => {
            let inventory = produce_current_state(root)?;
            let digest = crate::blind_journey::sha256_hex(
                serde_json::to_string(&inventory)
                    .map_err(|error| format!("serialize inventory identity: {error}"))?
                    .as_bytes(),
            );
            Ok((
                Some(format!("{CURRENT_STATE_SCHEMA_VERSION}:{digest}")),
                limitations,
            ))
        }
    }
}

fn live_github_identity(availability: LiveGithubAvailabilityV1) -> (Option<String>, Vec<String>) {
    match availability {
        LiveGithubAvailabilityV1::Available => (
            Some("captured:fixtures/work_portfolio/corpus".to_string()),
            Vec::new(),
        ),
        LiveGithubAvailabilityV1::Unavailable => (None, vec!["live_state_unavailable".to_string()]),
    }
}

fn classify_historical_campaign(reference: &str) -> bool {
    HISTORICAL_CAMPAIGN_MARKERS
        .iter()
        .any(|marker| reference.contains(marker))
}

fn candidate_for_issue(
    snapshot: &WorkPortfolioSnapshotV1,
    issue: u64,
) -> Result<&WorkCandidateV1, String> {
    snapshot
        .candidates
        .iter()
        .find(|candidate| candidate.issue == issue)
        .ok_or_else(|| format!("portfolio snapshot is missing issue #{issue}"))
}

fn duplicate_work_blocked(candidate: &WorkCandidateV1) -> bool {
    !candidate.overlapping_candidates.is_empty()
        || candidate.lifecycle_stage == IssueLifecycleDispositionV1::Duplicate
}

fn partial_acceptance_candidate(candidate: &WorkCandidateV1) -> bool {
    matches!(
        candidate.lifecycle_stage,
        IssueLifecycleDispositionV1::PartiallyLanded
            | IssueLifecycleDispositionV1::MergedPendingCloseout
    )
}

fn disposition_for_candidate(
    scenario: &RepositoryOrientationScenarioV1,
    candidate: &WorkCandidateV1,
    limitations: &[String],
) -> (
    OrientationDispositionV1,
    Option<u64>,
    Vec<String>,
    Vec<String>,
) {
    let mut overlap_checks = BTreeSet::new();
    let mut conflict_resources = candidate.conflict_resources.clone();
    conflict_resources.sort();
    conflict_resources.dedup();
    for overlap in &candidate.overlapping_candidates {
        overlap_checks.insert(format!("overlap:{overlap}"));
    }
    if duplicate_work_blocked(candidate) {
        return (
            OrientationDispositionV1::DuplicateBlocked,
            None,
            overlap_checks.into_iter().collect(),
            conflict_resources,
        );
    }
    if partial_acceptance_candidate(candidate) {
        return (
            OrientationDispositionV1::PartialAcceptance,
            Some(candidate.issue),
            overlap_checks.into_iter().collect(),
            conflict_resources,
        );
    }
    if matches!(
        candidate.kind,
        WorkCandidateKindV1::Blocked | WorkCandidateKindV1::Complete
    ) {
        return (
            OrientationDispositionV1::NoSafeWork,
            None,
            overlap_checks.into_iter().collect(),
            conflict_resources,
        );
    }
    if limitations
        .iter()
        .any(|item| item == "live_state_unavailable")
    {
        return (
            OrientationDispositionV1::NoSafeWork,
            None,
            overlap_checks.into_iter().collect(),
            conflict_resources,
        );
    }
    if scenario.source_release_issue.is_some() {
        return (
            OrientationDispositionV1::SourceReleaseRoute,
            scenario.source_release_issue,
            overlap_checks.into_iter().collect(),
            conflict_resources,
        );
    }
    if scenario.support_tier_overlay.as_deref() == Some("reject") {
        return (
            OrientationDispositionV1::SupportClaimRejected,
            None,
            overlap_checks.into_iter().collect(),
            conflict_resources,
        );
    }
    (
        OrientationDispositionV1::BoundedNextAction,
        Some(candidate.issue),
        overlap_checks.into_iter().collect(),
        conflict_resources,
    )
}

pub(crate) fn run_repository_orientation_scenario(
    root: &Path,
    scenario: &RepositoryOrientationScenarioV1,
) -> Result<RepositoryOrientationReceiptV1, String> {
    if scenario.expected.requires_goals_machinery {
        return Err("orientation must never require deleted goal machinery".to_string());
    }
    if let Some(issue) = scenario.source_release_issue
        && Some(issue) != release_authority_issue()
    {
        return Err(format!(
            "source release issue #{issue} conflicts with the release authority {RELEASE_AUTHORITY}"
        ));
    }
    let snapshot = compile_work_portfolio_corpus(&scenario.portfolio_corpus)?;
    let (inventory_identity, mut limitations) = inventory_identity(root, scenario.inventory)?;
    let (live_identity, live_limits) = live_github_identity(scenario.live_github);
    limitations.extend(live_limits);
    limitations.sort();
    limitations.dedup();

    let mut authority_map = BTreeMap::from([
        // Authorities come from the repository's own constants, never from
        // the expectation they are later compared against (#13074).
        ("development".to_string(), DEVELOPMENT_AUTHORITY.to_string()),
        ("release".to_string(), RELEASE_AUTHORITY.to_string()),
        (
            "work_selection".to_string(),
            "GitHub issues, PRs, checks, reviews, retained slices".to_string(),
        ),
        (
            "goals_machinery".to_string(),
            "deleted; not required".to_string(),
        ),
    ]);
    if let Some(reference) = &scenario.historical_campaign_ref {
        authority_map.insert(
            "historical_campaign".to_string(),
            if classify_historical_campaign(reference) {
                "historical; no execution authority".to_string()
            } else {
                "live campaign reference".to_string()
            },
        );
        if classify_historical_campaign(reference) {
            limitations.push("stale_historical_item_refused".to_string());
        }
    }

    let (disposition, selected_issue, duplicate_overlap_checks, conflict_resources) =
        if let Some(historical) = &scenario.historical_campaign_ref
            && classify_historical_campaign(historical)
        {
            (
                OrientationDispositionV1::HistoricalOnly,
                None,
                Vec::new(),
                Vec::new(),
            )
        } else if limitations
            .iter()
            .any(|item| item == "live_state_unavailable")
        {
            (
                OrientationDispositionV1::NoSafeWork,
                None,
                Vec::new(),
                Vec::new(),
            )
        } else if let Some(issue) = scenario.issue_number {
            let candidate = candidate_for_issue(&snapshot, issue)?;
            disposition_for_candidate(scenario, candidate, &limitations)
        } else if scenario.source_release_issue.is_some() {
            (
                OrientationDispositionV1::SourceReleaseRoute,
                scenario.source_release_issue,
                Vec::new(),
                Vec::new(),
            )
        } else {
            (
                OrientationDispositionV1::NoSafeWork,
                None,
                Vec::new(),
                Vec::new(),
            )
        };

    let mut retained = BTreeMap::from([
        (
            "docs/ROADMAP.md".to_string(),
            "retained:roadmap".to_string(),
        ),
        (
            "docs/REPO_TRACKING_MODEL.md".to_string(),
            "retained:tracking-model".to_string(),
        ),
        (
            "docs/status/SUPPORT_TIERS.md".to_string(),
            "retained:support-tiers".to_string(),
        ),
    ]);
    if let Some(overlay) = &scenario.support_tier_overlay {
        retained.insert(
            "fixtures/repository_orientation/support-tier-overlay.md".to_string(),
            overlay.clone(),
        );
    }

    Ok(RepositoryOrientationReceiptV1 {
        schema_version: REPOSITORY_ORIENTATION_RECEIPT_SCHEMA_VERSION.to_string(),
        repository: "EffortlessMetrics/ripr-swarm".to_string(),
        retained_artifact_identities: retained,
        generated_inventory_identity: inventory_identity,
        live_github_identity: live_identity,
        authority_map,
        disposition,
        selected_issue,
        duplicate_overlap_checks,
        conflict_resources,
        governing_contracts: vec![
            "docs/REPO_TRACKING_MODEL.md".to_string(),
            "docs/ROADMAP.md".to_string(),
        ],
        proof_commands: vec![
            "cargo test -p xtask repository_orientation -- --test-threads=1".to_string(),
            "cargo xtask current-state --check".to_string(),
        ],
        limitations,
        non_claims: vec![
            "This receipt is orientation only, not execution authority.".to_string(),
            "It does not prove the selected implementation is correct.".to_string(),
        ],
        claim_boundary: REPOSITORY_ORIENTATION_CLAIM_BOUNDARY.to_string(),
    })
}

fn validate_receipt_against_expect(
    receipt: &RepositoryOrientationReceiptV1,
    expected: &RepositoryOrientationExpectV1,
) -> Result<(), String> {
    if receipt.disposition != expected.disposition {
        return Err(format!(
            "disposition drift: got {:?}, expected {:?}",
            receipt.disposition, expected.disposition
        ));
    }
    if receipt.selected_issue != expected.selected_issue {
        return Err(format!(
            "selected issue drift: got {:?}, expected {:?}",
            receipt.selected_issue, expected.selected_issue
        ));
    }
    if receipt.authority_map.get("development") != Some(&expected.development_authority) {
        return Err("development authority drift".to_string());
    }
    if receipt.authority_map.get("release") != Some(&expected.release_authority) {
        return Err("release authority drift".to_string());
    }
    if receipt.authority_map.get("goals_machinery") != Some(&"deleted; not required".to_string()) {
        return Err("orientation presented deleted goals machinery as live".to_string());
    }
    for limitation in &expected.limitations {
        if !receipt.limitations.iter().any(|item| item == limitation) {
            return Err(format!("missing limitation `{limitation}`"));
        }
    }
    for resource in &expected.conflict_resources {
        if !resource.is_empty()
            && !receipt
                .conflict_resources
                .iter()
                .any(|item| item == resource)
        {
            return Err(format!("missing conflict resource `{resource}`"));
        }
    }
    if let Some(stale) = &expected.stale_item_refused
        && !receipt.limitations.iter().any(|item| item.contains(stale))
    {
        return Err(format!("expected stale refusal `{stale}`"));
    }
    Ok(())
}

pub(crate) fn run_repository_orientation_corpus(
    root: &Path,
    corpus: &RepositoryOrientationCorpusV1,
) -> Result<(), String> {
    for scenario in &corpus.scenarios {
        let receipt = run_repository_orientation_scenario(root, scenario)?;
        validate_receipt_against_expect(&receipt, &scenario.expected)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RepositoryOrientationProvenanceFileV1 {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RepositoryOrientationProvenanceV1 {
    schema_version: String,
    repository: String,
    captured_at: String,
    capture_method: String,
    files: Vec<RepositoryOrientationProvenanceFileV1>,
}

fn load_repository_orientation_provenance(
    body: &str,
) -> Result<RepositoryOrientationProvenanceV1, String> {
    let provenance: RepositoryOrientationProvenanceV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse repository orientation provenance: {error}"))?;
    if provenance.schema_version != REPOSITORY_ORIENTATION_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported repository orientation provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty() || provenance.capture_method.trim().is_empty() {
        return Err(
            "repository orientation provenance must record repository and capture_method"
                .to_string(),
        );
    }
    Ok(provenance)
}

fn provenance_digest_matches(recorded: &str, bytes: &[u8]) -> bool {
    recorded == crate::blind_journey::sha256_hex(bytes)
}

pub(crate) fn validate_repository_orientation_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_REPOSITORY_ORIENTATION_CORPUS_DIR);
    let provenance_body = match fs::read_to_string(root.join("provenance.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/repository_orientation: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance = match load_repository_orientation_provenance(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/repository_orientation: {error}"));
            return;
        }
    };
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/repository_orientation: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if !provenance_digest_matches(&file.sha256, &bytes) {
            violations.push(format!(
                "fixtures/repository_orientation: {} digest drifted: recorded `{}`, recomputed `{}`",
                path.display(),
                file.sha256,
                crate::blind_journey::sha256_hex(&bytes)
            ));
        }
    }
    let corpus_body = match fs::read_to_string(root.join("corpus.json")) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/repository_orientation: corpus.json is unreadable: {error}"
            ));
            return;
        }
    };
    let corpus = match load_repository_orientation_corpus(&corpus_body) {
        Ok(corpus) => corpus,
        Err(error) => {
            violations.push(format!("fixtures/repository_orientation: {error}"));
            return;
        }
    };
    let repo_root = workspace_path("");
    if let Err(error) = run_repository_orientation_corpus(&repo_root, &corpus) {
        violations.push(format!("fixtures/repository_orientation: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed_corpus() -> Result<RepositoryOrientationCorpusV1, String> {
        let body = fs::read_to_string(
            workspace_path(DEFAULT_REPOSITORY_ORIENTATION_CORPUS_DIR).join("corpus.json"),
        )
        .map_err(|error| format!("read committed orientation corpus: {error}"))?;
        load_repository_orientation_corpus(&body)
    }

    #[test]
    fn repository_orientation_committed_corpus_nine_scenarios_hold() -> Result<(), String> {
        let corpus = committed_corpus()?;
        run_repository_orientation_corpus(&workspace_path(""), &corpus)
    }

    #[test]
    fn repository_orientation_receipt_is_deterministic_for_equal_inputs() -> Result<(), String> {
        let corpus = committed_corpus()?;
        let scenario = corpus
            .scenarios
            .first()
            .ok_or_else(|| "missing first scenario".to_string())?;
        let first = run_repository_orientation_scenario(&workspace_path(""), scenario)?;
        let second = run_repository_orientation_scenario(&workspace_path(""), scenario)?;
        let first_json = serde_json::to_string(&first)
            .map_err(|error| format!("serialize first receipt: {error}"))?;
        let second_json = serde_json::to_string(&second)
            .map_err(|error| format!("serialize second receipt: {error}"))?;
        if first_json != second_json {
            return Err("orientation receipt must be deterministic".to_string());
        }
        Ok(())
    }

    #[test]
    fn repository_orientation_overlapping_open_pr_blocks_duplicate_selection() -> Result<(), String>
    {
        let scenario = RepositoryOrientationScenarioV1 {
            id: "overlapping-open-pr-blocks-duplicate".to_string(),
            category: "overlapping_open_pr".to_string(),
            portfolio_corpus: "corpus".to_string(),
            issue_number: Some(9101),
            live_github: LiveGithubAvailabilityV1::Available,
            inventory: GeneratedInventoryAvailabilityV1::Available,
            historical_campaign_ref: None,
            source_release_issue: None,
            support_tier_overlay: None,
            expected: RepositoryOrientationExpectV1 {
                disposition: OrientationDispositionV1::DuplicateBlocked,
                selected_issue: None,
                requires_goals_machinery: false,
                development_authority: "EffortlessMetrics/ripr-swarm".to_string(),
                release_authority: "EffortlessMetrics/ripr#1463".to_string(),
                limitations: Vec::new(),
                conflict_resources: vec!["output-contract:rust-json".to_string()],
                stale_item_refused: None,
            },
        };
        let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
        validate_receipt_against_expect(&receipt, &scenario.expected)
    }

    #[test]
    fn repository_orientation_wrong_category_still_blocks_portfolio_overlap() -> Result<(), String>
    {
        let scenario = RepositoryOrientationScenarioV1 {
            id: "overlap-not-category-gated".to_string(),
            category: "open_issue_no_pr".to_string(),
            portfolio_corpus: "corpus".to_string(),
            issue_number: Some(9101),
            live_github: LiveGithubAvailabilityV1::Available,
            inventory: GeneratedInventoryAvailabilityV1::Available,
            historical_campaign_ref: None,
            source_release_issue: None,
            support_tier_overlay: None,
            expected: RepositoryOrientationExpectV1 {
                disposition: OrientationDispositionV1::DuplicateBlocked,
                selected_issue: None,
                requires_goals_machinery: false,
                development_authority: "EffortlessMetrics/ripr-swarm".to_string(),
                release_authority: "EffortlessMetrics/ripr#1463".to_string(),
                limitations: Vec::new(),
                conflict_resources: vec!["output-contract:rust-json".to_string()],
                stale_item_refused: None,
            },
        };
        let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
        validate_receipt_against_expect(&receipt, &scenario.expected)
    }

    #[test]
    fn repository_orientation_blocked_portfolio_candidate_is_no_safe_work() -> Result<(), String> {
        let scenario = RepositoryOrientationScenarioV1 {
            id: "blocked-upstream-dependency".to_string(),
            category: "blocked_issue".to_string(),
            portfolio_corpus: "corpus".to_string(),
            issue_number: Some(9104),
            live_github: LiveGithubAvailabilityV1::Available,
            inventory: GeneratedInventoryAvailabilityV1::Available,
            historical_campaign_ref: None,
            source_release_issue: None,
            support_tier_overlay: None,
            expected: RepositoryOrientationExpectV1 {
                disposition: OrientationDispositionV1::NoSafeWork,
                selected_issue: None,
                requires_goals_machinery: false,
                development_authority: "EffortlessMetrics/ripr-swarm".to_string(),
                release_authority: "EffortlessMetrics/ripr#1463".to_string(),
                limitations: Vec::new(),
                conflict_resources: Vec::new(),
                stale_item_refused: None,
            },
        };
        let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
        validate_receipt_against_expect(&receipt, &scenario.expected)
    }

    fn release_scenario() -> Result<RepositoryOrientationScenarioV1, String> {
        committed_corpus()?
            .scenarios
            .into_iter()
            .find(|scenario| scenario.id == "source-release-owned-by-ripr")
            .ok_or_else(|| "missing committed source-release scenario".to_string())
    }

    #[test]
    fn repository_orientation_release_preserves_historical_refusal() -> Result<(), String> {
        let mut scenario = release_scenario()?;
        scenario.historical_campaign_ref = Some("historical release-control campaign".to_string());
        let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
        assert_eq!(
            receipt.disposition,
            OrientationDispositionV1::HistoricalOnly
        );
        assert_eq!(receipt.selected_issue, None);
        assert!(
            receipt
                .limitations
                .iter()
                .any(|item| item == "stale_historical_item_refused")
        );
        Ok(())
    }

    #[test]
    fn repository_orientation_release_hint_requires_available_live_state() -> Result<(), String> {
        let mut scenario = release_scenario()?;
        scenario.live_github = LiveGithubAvailabilityV1::Unavailable;
        let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
        assert_eq!(receipt.disposition, OrientationDispositionV1::NoSafeWork);
        assert_eq!(receipt.selected_issue, None);
        assert_eq!(receipt.live_github_identity, None);
        assert!(
            receipt
                .limitations
                .iter()
                .any(|item| item == "live_state_unavailable")
        );
        Ok(())
    }

    #[test]
    fn repository_orientation_release_hint_preserves_candidate_constraints() -> Result<(), String> {
        let mut scenario = release_scenario()?;
        for (issue, disposition, selected_issue) in [
            (9101, OrientationDispositionV1::DuplicateBlocked, None),
            (9104, OrientationDispositionV1::NoSafeWork, None),
            (9301, OrientationDispositionV1::NoSafeWork, None),
            (
                9109,
                OrientationDispositionV1::PartialAcceptance,
                Some(9109),
            ),
            (
                9202,
                OrientationDispositionV1::SourceReleaseRoute,
                Some(1463),
            ),
        ] {
            scenario.issue_number = Some(issue);
            let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
            assert_eq!(receipt.disposition, disposition, "candidate #{issue}");
            assert_eq!(receipt.selected_issue, selected_issue, "candidate #{issue}");
            if issue == 9101 {
                assert!(!receipt.duplicate_overlap_checks.is_empty());
                assert!(
                    receipt
                        .conflict_resources
                        .iter()
                        .any(|resource| resource == "output-contract:rust-json")
                );
            }
        }
        Ok(())
    }

    #[test]
    fn repository_orientation_release_hint_preserves_inventory_limitations() -> Result<(), String> {
        let mut scenario = release_scenario()?;
        // Inventory limits remain named limitations, as in the committed stale
        // inventory scenario; they do not acquire a new routing veto here.
        for (inventory, limitation) in [
            (
                GeneratedInventoryAvailabilityV1::Stale,
                "generated_inventory_stale",
            ),
            (
                GeneratedInventoryAvailabilityV1::Unavailable,
                "generated_inventory_unavailable",
            ),
        ] {
            scenario.inventory = inventory;
            let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
            assert_eq!(
                receipt.disposition,
                OrientationDispositionV1::SourceReleaseRoute
            );
            assert_eq!(receipt.selected_issue, Some(1463));
            assert!(receipt.limitations.iter().any(|item| item == limitation));
        }
        // The clean source-release-only case remains a positive control.
        scenario.inventory = GeneratedInventoryAvailabilityV1::Available;
        let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
        validate_receipt_against_expect(&receipt, &scenario.expected)
    }

    /// Relative path and bytes (`None` when absent) for each input the run reads.
    type InputSnapshot = Vec<(String, Option<Vec<u8>>)>;

    /// The orientation run reads exactly these repository inputs. Comparing
    /// their contents, rather than whole-workspace `git status`, keeps the proof
    /// independent of unrelated files that parallel test shards write.
    fn orientation_input_snapshot(root: &Path) -> Result<InputSnapshot, String> {
        [
            "docs/ROADMAP.md",
            "Cargo.toml",
            "docs/status/SUPPORT_TIERS.md",
        ]
        .iter()
        .map(|relative| {
            let path = root.join(relative);
            match fs::read(&path) {
                Ok(bytes) => Ok(((*relative).to_string(), Some(bytes))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(((*relative).to_string(), None))
                }
                Err(error) => Err(format!("read {}: {error}", path.display())),
            }
        })
        .collect()
    }

    #[test]
    fn repository_orientation_read_only_proof_does_not_mutate_read_inputs() -> Result<(), String> {
        let corpus = committed_corpus()?;
        let root = workspace_path("");
        let before = orientation_input_snapshot(&root)?;
        run_repository_orientation_corpus(&root, &corpus)?;
        let after = orientation_input_snapshot(&root)?;
        if before != after {
            return Err("orientation changed a repository input it reads".to_string());
        }
        Ok(())
    }

    fn committed_corpus_json() -> Result<serde_json::Value, String> {
        let body = fs::read_to_string(
            workspace_path(DEFAULT_REPOSITORY_ORIENTATION_CORPUS_DIR).join("corpus.json"),
        )
        .map_err(|error| format!("read committed orientation corpus: {error}"))?;
        serde_json::from_str(&body).map_err(|error| format!("parse corpus json: {error}"))
    }

    fn load_mutated(
        mutate: impl FnOnce(&mut Vec<serde_json::Value>),
    ) -> Result<RepositoryOrientationCorpusV1, String> {
        let mut value = committed_corpus_json()?;
        let scenarios = value
            .get_mut("scenarios")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| "corpus has no scenarios array".to_string())?;
        mutate(scenarios);
        load_repository_orientation_corpus(&value.to_string())
    }

    #[test]
    fn repository_orientation_corpus_rejects_a_duplicated_scenario_id() {
        let result = load_mutated(|scenarios| {
            let first = scenarios[0]["id"].clone();
            scenarios[1]["id"] = first;
        });
        assert!(
            result
                .as_ref()
                .err()
                .is_some_and(|e| e.contains("repeats scenario id")),
            "{result:?}"
        );
    }

    #[test]
    fn repository_orientation_corpus_rejects_a_substituted_scenario() {
        // Nine unique ids, but one is not in the pinned set.
        let result = load_mutated(|scenarios| {
            scenarios[8]["id"] = serde_json::json!("some-other-scenario");
        });
        assert!(
            result
                .as_ref()
                .err()
                .is_some_and(|e| e.contains("missing required scenario")),
            "{result:?}"
        );
    }

    #[test]
    fn repository_orientation_corpus_rejects_an_extra_scenario() {
        let result = load_mutated(|scenarios| {
            let mut extra = scenarios[0].clone();
            extra["id"] = serde_json::json!("a-tenth-scenario");
            scenarios.push(extra);
        });
        assert!(
            result
                .as_ref()
                .err()
                .is_some_and(|e| e.contains("unexpected scenario")),
            "{result:?}"
        );
    }

    #[test]
    fn repository_orientation_corpus_rejects_a_missing_scenario() {
        let result = load_mutated(|scenarios| {
            scenarios.pop();
        });
        assert!(
            result
                .as_ref()
                .err()
                .is_some_and(|e| e.contains("missing required scenario")),
            "{result:?}"
        );
    }

    /// The receipt's authorities come from the repository's constants, so a
    /// corpus whose expectation disagrees fails instead of echoing itself.
    #[test]
    fn repository_orientation_expected_authority_is_checked_not_copied() -> Result<(), String> {
        let mut value = committed_corpus_json()?;
        value["scenarios"][0]["expected"]["release_authority"] =
            serde_json::json!("EffortlessMetrics/ripr#9999");
        let corpus = load_repository_orientation_corpus(&value.to_string())?;
        match run_repository_orientation_corpus(&workspace_path(""), &corpus) {
            Err(error) if error.contains("release authority drift") => Ok(()),
            other => Err(format!("expected release authority drift, got {other:?}")),
        }
    }

    #[test]
    fn repository_orientation_conflicting_source_release_issue_is_refused() -> Result<(), String> {
        let mut value = committed_corpus_json()?;
        let scenarios = value["scenarios"]
            .as_array_mut()
            .ok_or_else(|| "no scenarios".to_string())?;
        let routed = scenarios
            .iter_mut()
            .find(|s| s["id"] == "source-release-owned-by-ripr")
            .ok_or_else(|| "missing source-release scenario".to_string())?;
        routed["source_release_issue"] = serde_json::json!(9999);
        let corpus = load_repository_orientation_corpus(&value.to_string())?;
        match run_repository_orientation_corpus(&workspace_path(""), &corpus) {
            Err(error) if error.contains("conflicts with the release authority") => Ok(()),
            other => Err(format!(
                "expected a release-authority conflict, got {other:?}"
            )),
        }
    }
}
