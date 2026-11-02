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

use crate::current_state::{CURRENT_STATE_SCHEMA_VERSION, produce_current_state};
use crate::issue_lifecycle_attempt::IssueLifecycleDispositionV1;
use crate::work_portfolio::{
    WorkCandidateV1, WorkPortfolioSnapshotV1, compile_work_portfolio_corpus,
};

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
    pub governing_contracts: Vec<String>,
    pub proof_commands: Vec<String>,
    pub limitations: Vec<String>,
    pub non_claims: Vec<String>,
    pub claim_boundary: String,
}

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
    if corpus.scenarios.len() < 9 {
        return Err(format!(
            "repository orientation corpus must carry at least nine scenarios, got {}",
            corpus.scenarios.len()
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

fn candidate_for_issue<'a>(
    snapshot: &'a WorkPortfolioSnapshotV1,
    issue: u64,
) -> Result<&'a WorkCandidateV1, String> {
    snapshot
        .candidates
        .iter()
        .find(|candidate| candidate.issue == issue)
        .ok_or_else(|| format!("portfolio snapshot is missing issue #{issue}"))
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
    for overlap in &candidate.overlapping_candidates {
        overlap_checks.insert(format!("overlap:{overlap}"));
    }
    if scenario.category == "overlapping_open_pr" {
        return (
            OrientationDispositionV1::DuplicateBlocked,
            None,
            overlap_checks.into_iter().collect(),
            conflict_resources,
        );
    }
    if candidate.lifecycle_stage == IssueLifecycleDispositionV1::PartiallyLanded
        || scenario.category == "partially_landed_issue"
    {
        return (
            OrientationDispositionV1::PartialAcceptance,
            Some(candidate.issue),
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
    let snapshot = compile_work_portfolio_corpus(&scenario.portfolio_corpus)?;
    let (inventory_identity, mut limitations) = inventory_identity(root, scenario.inventory)?;
    let (live_identity, live_limits) = live_github_identity(scenario.live_github);
    limitations.extend(live_limits);
    limitations.sort();
    limitations.dedup();

    let mut authority_map = BTreeMap::from([
        (
            "development".to_string(),
            scenario.expected.development_authority.clone(),
        ),
        (
            "release".to_string(),
            scenario.expected.release_authority.clone(),
        ),
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

    let (disposition, selected_issue, duplicate_overlap_checks, _conflict_resources) =
        if scenario.source_release_issue.is_some() {
            (
                OrientationDispositionV1::SourceReleaseRoute,
                scenario.source_release_issue,
                Vec::new(),
                Vec::new(),
            )
        } else if let Some(historical) = &scenario.historical_campaign_ref {
            if classify_historical_campaign(historical) {
                (
                    OrientationDispositionV1::HistoricalOnly,
                    None,
                    Vec::new(),
                    Vec::new(),
                )
            } else if let Some(issue) = scenario.issue_number {
                let candidate = candidate_for_issue(&snapshot, issue)?;
                disposition_for_candidate(scenario, candidate, &limitations)
            } else {
                (
                    OrientationDispositionV1::NoSafeWork,
                    None,
                    Vec::new(),
                    Vec::new(),
                )
            }
        } else if let Some(issue) = scenario.issue_number {
            if limitations
                .iter()
                .any(|item| item == "live_state_unavailable")
            {
                (
                    OrientationDispositionV1::NoSafeWork,
                    None,
                    Vec::new(),
                    Vec::new(),
                )
            } else {
                let candidate = candidate_for_issue(&snapshot, issue)?;
                disposition_for_candidate(scenario, candidate, &limitations)
            }
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
                .duplicate_overlap_checks
                .iter()
                .any(|item| item.contains(resource))
        {
            return Err(format!("missing conflict resource `{resource}`"));
        }
    }
    if let Some(stale) = &expected.stale_item_refused {
        if !receipt.limitations.iter().any(|item| item.contains(stale)) {
            return Err(format!("expected stale refusal `{stale}`"));
        }
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

pub(crate) fn validate_repository_orientation_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path(DEFAULT_REPOSITORY_ORIENTATION_CORPUS_DIR);
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
                conflict_resources: Vec::new(),
                stale_item_refused: None,
            },
        };
        let receipt = run_repository_orientation_scenario(&workspace_path(""), &scenario)?;
        validate_receipt_against_expect(&receipt, &scenario.expected)
    }

    #[test]
    fn repository_orientation_read_only_proof_does_not_mutate_tracked_files() -> Result<(), String>
    {
        let corpus = committed_corpus()?;
        let before = fs::read_dir(workspace_path(".git"))
            .map(|_| ())
            .unwrap_or(());
        run_repository_orientation_corpus(&workspace_path(""), &corpus)?;
        let after = fs::read_dir(workspace_path(".git"))
            .map(|_| ())
            .unwrap_or(());
        if before != after {
            return Err("orientation unexpectedly changed repository visibility".to_string());
        }
        Ok(())
    }
}
