//! Fixture comparison for #4781. The checked graph supplies policy, and an
//! explicitly supplied local snapshot supplies observations.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{RELEASE_TARGETS_MANIFEST_PATH, ReleaseGraph, evaluate_release_graph};

const REPOSITORY: &str = "EffortlessMetrics/ripr-swarm";
const CANDIDATE_DESCRIPTION: &str = "Candidate qualification in `ripr-swarm`; completion does not authorize or establish source publication.";
const REPORT_STEM: &str = "release-milestone-fixture";
const FIXTURE_SCHEMA_VERSION: u32 = 2;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u32,
    repository: String,
    complete: bool,
    pagination: Pagination,
    milestones: Vec<Milestone>,
    issues: Vec<Issue>,
    pull_requests: Vec<PullRequest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pagination {
    milestones: Vec<CapturedPage>,
    issues: Vec<CapturedPage>,
    pull_requests: Vec<CapturedPage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapturedPage {
    page: usize,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    next_page: Option<usize>,
    ids: Vec<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Milestone {
    number: u32,
    title: String,
    description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Issue {
    number: u32,
    closed: bool,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    milestone: Option<u32>,
    labels: BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PullRequest {
    number: u32,
    closed: bool,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    milestone: Option<u32>,
    closes: BTreeSet<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    InSync,
    Drift,
    NotComparable,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::InSync => "in_sync",
            Self::Drift => "drift",
            Self::NotComparable => "not_comparable",
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct Finding {
    code: &'static str,
    detail: String,
}

#[derive(Default, Serialize)]
struct Counts {
    milestones: usize,
    issues: usize,
    pull_requests: usize,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    evidence: &'static str,
    manifest_sha256: Option<String>,
    observed: Option<Counts>,
    status: Status,
    findings: BTreeSet<Finding>,
}

impl Report {
    fn new() -> Self {
        Self {
            schema_version: 1,
            evidence: "local_fixture",
            manifest_sha256: None,
            observed: None,
            status: Status::NotComparable,
            findings: BTreeSet::new(),
        }
    }

    fn finding(&mut self, code: &'static str, detail: String) {
        self.findings.insert(Finding { code, detail });
    }

    fn markdown(&self) -> String {
        let mut body = format!(
            "# Release milestone fixture comparison\n\nEvidence: local fixture\n\nStatus: `{}`\n",
            self.status.as_str()
        );
        if let Some(digest) = &self.manifest_sha256 {
            let _ = writeln!(body, "\nManifest SHA-256: `{digest}`");
        }
        if let Some(counts) = &self.observed {
            let _ = writeln!(
                body,
                "\nObserved: {} milestones, {} issues, {} pull requests.",
                counts.milestones, counts.issues, counts.pull_requests
            );
        }
        let _ = writeln!(body, "\nFindings: {}\n", self.findings.len());
        for finding in &self.findings {
            // Indented code preserves fixture diagnostics without interpreting
            // text from the input as Markdown or HTML.
            let _ = writeln!(
                body,
                "    {}: {}",
                finding.code,
                finding.detail.escape_default()
            );
        }
        body
    }
}

pub(crate) fn check_fixture(path: &str) -> Result<(), String> {
    let manifest = std::fs::read_to_string(RELEASE_TARGETS_MANIFEST_PATH);
    let snapshot = std::fs::read_to_string(path);
    let mut report = Report::new();
    match (manifest, snapshot) {
        (Ok(manifest), Ok(snapshot)) => report = evaluate(&manifest, &snapshot),
        (manifest, snapshot) => {
            if let Err(error) = manifest {
                report.finding("manifest_unreadable", error.to_string());
            }
            if let Err(error) = snapshot {
                report.finding("fixture_unreadable", error.to_string());
            }
        }
    }

    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("serialize milestone fixture report: {error}"))?;
    crate::write_report(&format!("{REPORT_STEM}.json"), &format!("{json}\n"))?;
    crate::write_report(&format!("{REPORT_STEM}.md"), &report.markdown())?;
    println!(
        "Release milestone fixture: {} ({} findings). Reports: target/ripr/reports/{REPORT_STEM}.{{json,md}}",
        report.status.as_str(),
        report.findings.len()
    );
    if report.status == Status::InSync {
        Ok(())
    } else {
        Err(format!(
            "release milestone fixture: {}",
            report.status.as_str()
        ))
    }
}

fn evaluate(manifest: &str, snapshot: &str) -> Report {
    let mut report = Report::new();
    report.manifest_sha256 = Some(format!("{:x}", Sha256::digest(manifest.as_bytes())));
    let (graph, violations) = evaluate_release_graph(RELEASE_TARGETS_MANIFEST_PATH, manifest);
    for violation in violations {
        report.finding("invalid_manifest", violation);
    }
    let snapshot: Snapshot = match serde_json::from_str(snapshot) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            report.finding("invalid_fixture", error.to_string());
            return report;
        }
    };
    report.observed = Some(Counts {
        milestones: snapshot.milestones.len(),
        issues: snapshot.issues.len(),
        pull_requests: snapshot.pull_requests.len(),
    });
    if snapshot.schema_version != FIXTURE_SCHEMA_VERSION {
        report.finding(
            "fixture_schema",
            format!(
                "expected schema version {FIXTURE_SCHEMA_VERSION}, found {}",
                snapshot.schema_version
            ),
        );
    }
    if !snapshot.repository.eq_ignore_ascii_case(REPOSITORY) {
        report.finding(
            "fixture_repository",
            format!("expected {REPOSITORY}, found {:?}", snapshot.repository),
        );
    }
    if !snapshot.complete {
        report.finding(
            "fixture_incomplete",
            "snapshot declares incomplete coverage".to_string(),
        );
    }
    for (stream, pages, ids) in [
        (
            "milestones",
            &snapshot.pagination.milestones,
            snapshot.milestones.iter().map(|row| row.number).collect(),
        ),
        (
            "issues",
            &snapshot.pagination.issues,
            snapshot.issues.iter().map(|row| row.number).collect(),
        ),
        (
            "pull_requests",
            &snapshot.pagination.pull_requests,
            snapshot
                .pull_requests
                .iter()
                .map(|row| row.number)
                .collect(),
        ),
    ] {
        if let Err(reason) = validate_pages(pages, ids) {
            report.finding("fixture_pagination", format!("{stream}: {reason}"));
        }
    }

    let milestones = index(&mut report, "milestone", &snapshot.milestones, |row| {
        row.number
    });
    let issues = index(&mut report, "issue", &snapshot.issues, |row| row.number);
    index(
        &mut report,
        "pull request",
        &snapshot.pull_requests,
        |row| row.number,
    );
    for pr in &snapshot.pull_requests {
        if issues.contains_key(&pr.number) {
            report.finding(
                "duplicate_identity",
                format!("#{} is both an issue and a pull request", pr.number),
            );
        }
        if pr.closes.contains(&0) {
            report.finding(
                "invalid_issue_reference",
                format!("pull request #{} closes issue #0", pr.number),
            );
        }
    }
    for (kind, number, milestone) in snapshot
        .issues
        .iter()
        .map(|row| ("issue", row.number, row.milestone))
        .chain(
            snapshot
                .pull_requests
                .iter()
                .map(|row| ("pull request", row.number, row.milestone)),
        )
    {
        if let Some(milestone) = milestone.filter(|number| !milestones.contains_key(number)) {
            report.finding(
                "unknown_milestone_reference",
                format!(
                    "{kind} #{number} references milestone #{milestone}, absent from the snapshot"
                ),
            );
        }
    }
    if !report.findings.is_empty() {
        return report;
    }

    compare(
        &graph,
        &milestones,
        &issues,
        &snapshot.pull_requests,
        &mut report,
    );
    report
}

fn validate_pages(pages: &[CapturedPage], observed_ids: BTreeSet<u32>) -> Result<(), String> {
    if pages.is_empty() {
        return Err("missing page 1".to_string());
    }
    let ordered: BTreeMap<_, _> = pages.iter().map(|page| (page.page, page)).collect();
    if ordered.len() != pages.len() {
        return Err("duplicate page number".to_string());
    }

    let mut ids = BTreeSet::new();
    let mut duplicate_id = false;
    for (index, &page) in ordered.values().enumerate() {
        let expected = index + 1;
        if page.page != expected {
            return Err(format!("expected page {expected}, found {}", page.page));
        }
        let next = if expected < pages.len() {
            Some(expected + 1)
        } else {
            None
        };
        if page.next_page != next {
            return Err(match next {
                Some(next) => format!("page {expected} must point to page {next}"),
                None => format!("page {expected} has no captured terminal marker"),
            });
        }
        if page.ids.is_empty() && pages.len() > 1 {
            return Err(format!("page {expected} is empty in a multi-page capture"));
        }
        for &id in &page.ids {
            duplicate_id |= !ids.insert(id);
        }
    }
    if duplicate_id {
        return Err("duplicate row ID in captured pages".to_string());
    }
    if ids != observed_ids {
        return Err("captured page IDs differ from observed rows".to_string());
    }
    Ok(())
}

fn index<'a, T>(
    report: &mut Report,
    kind: &str,
    rows: &'a [T],
    number: impl Fn(&T) -> u32,
) -> BTreeMap<u32, &'a T> {
    let mut index = BTreeMap::new();
    for row in rows {
        let number = number(row);
        if number == 0 {
            report.finding(
                "invalid_identity",
                format!("{kind} number must be positive"),
            );
        }
        if index.insert(number, row).is_some() {
            report.finding(
                "duplicate_identity",
                format!("{kind} #{number} appears more than once"),
            );
        }
    }
    index
}

fn compare(
    graph: &ReleaseGraph,
    milestones: &BTreeMap<u32, &Milestone>,
    issues: &BTreeMap<u32, &Issue>,
    pull_requests: &[PullRequest],
    report: &mut Report,
) {
    let committed: BTreeMap<_, _> = graph
        .releases
        .iter()
        .flat_map(|release| {
            release
                .committed()
                .into_iter()
                .map(move |issue| (issue, release))
        })
        .collect();
    let conditional: BTreeSet<_> = graph
        .releases
        .iter()
        .flat_map(|release| release.role("conditional_issues").iter().copied())
        .collect();
    let rolling: BTreeSet<_> = graph.rolling.iter().filter_map(|row| row.issue).collect();
    let outside_parents: BTreeSet<_> = graph
        .parents
        .iter()
        .filter(|row| row.counted_in.as_deref() == Some("none"))
        .filter_map(|row| row.issue)
        .collect();
    for number in BTreeSet::from_iter(
        committed
            .keys()
            .copied()
            .chain(conditional.iter().copied())
            .chain(rolling.iter().copied())
            .chain(outside_parents.iter().copied()),
    ) {
        if !issues.contains_key(&number) {
            report.finding(
                "issue_unavailable",
                format!("manifest issue #{number} is absent from the snapshot"),
            );
        }
    }
    if !report.findings.is_empty() {
        return;
    }

    let titles: BTreeSet<_> = graph
        .releases
        .iter()
        .map(|release| release.milestone.as_str())
        .collect();
    let mut candidate_ids = BTreeSet::new();
    let mut unique_candidates = BTreeMap::new();
    for title in titles {
        let matches: Vec<_> = milestones
            .values()
            .filter(|row| row.title == title)
            .collect();
        let numbers: Vec<_> = matches.iter().map(|row| row.number).collect();
        candidate_ids.extend(numbers.iter().copied());
        match matches.as_slice() {
            [] => report.finding(
                "milestone_missing",
                format!("candidate {title:?} is absent"),
            ),
            [milestone] => {
                unique_candidates.insert(title, milestone.number);
            }
            _ => report.finding(
                "milestone_duplicated",
                format!("candidate {title:?} matches milestones {numbers:?}"),
            ),
        }
        for milestone in matches {
            if milestone.description != CANDIDATE_DESCRIPTION {
                report.finding(
                    "milestone_nonclaim",
                    format!(
                        "milestone #{} ({title}) must have description {CANDIDATE_DESCRIPTION:?}",
                        milestone.number
                    ),
                );
            }
        }
    }

    for issue in issues.values() {
        let number = issue.number;
        if let Some(release) = committed.get(&number) {
            let label = format!("target/{}", release.version);
            if !issue.labels.contains(&label) {
                report.finding(
                    "target_label_missing",
                    format!("issue #{number} requires label {label:?}"),
                );
            }
            if let Some(expected) = unique_candidates.get(release.milestone.as_str())
                && issue.milestone != Some(*expected)
            {
                report.finding(
                    "issue_milestone",
                    format!(
                        "issue #{number} requires milestone #{expected}; observed {}",
                        milestone_text(issue.milestone)
                    ),
                );
            }
        }
        if let Some(milestone) = issue.milestone.filter(|id| candidate_ids.contains(id)) {
            let is_conditional = conditional.contains(&number);
            let is_rolling = rolling.contains(&number);
            let is_outside_parent = outside_parents.contains(&number);
            let detail = || format!("issue #{number} is in candidate milestone #{milestone}");
            if is_conditional || issue.labels.contains("target/conditional") {
                report.finding("conditional_issue_in_candidate", detail());
            }
            if is_rolling || issue.labels.contains("target/rolling") {
                report.finding("rolling_issue_in_candidate", detail());
            }
            if is_outside_parent {
                report.finding("uncounted_parent_in_candidate", detail());
            }
            if !is_conditional
                && !is_rolling
                && !is_outside_parent
                && !committed.contains_key(&number)
            {
                report.finding("unmanifested_issue_in_candidate", detail());
            }
        }
    }

    for pr in pull_requests.iter().filter(|pr| !pr.closed) {
        for issue in &pr.closes {
            if let Some(release) = committed.get(issue)
                && let Some(expected) = unique_candidates.get(release.milestone.as_str())
                && pr.milestone != Some(*expected)
            {
                report.finding("pull_request_milestone", format!("open pull request #{} closes issue #{issue} and requires milestone #{expected}; observed {}", pr.number, milestone_text(pr.milestone)));
            }
        }
    }

    for release in &graph.releases {
        if let Some(goal) = release.goal_issue
            && issues.get(&goal).is_some_and(|row| row.closed)
        {
            for number in release
                .committed()
                .into_iter()
                .filter(|number| *number != goal)
            {
                if issues.get(&number).is_some_and(|row| !row.closed) {
                    report.finding(
                        "closed_goal_with_open_work",
                        format!("closed goal #{goal} has open committed issue #{number}"),
                    );
                }
            }
        }
    }
    report.status = if report.findings.is_empty() {
        Status::InSync
    } else {
        Status::Drift
    };
}

fn milestone_text(number: Option<u32>) -> String {
    number.map_or_else(|| "none".to_string(), |number| format!("#{number}"))
}

#[cfg(test)]
mod tests;
