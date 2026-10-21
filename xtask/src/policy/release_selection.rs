//! Offline check for the 0.11 selected-claim packet (#2766).
//!
//! `policy/release-selection.json` is the machine-readable answer to "what must
//! be in or explicitly outside the final swarm candidate, and why". It binds
//! each selected claim to exactly one resolution from the #2766 vocabulary and
//! fails closed on the stage law: a packet may not claim `scope_closed` or
//! `hard_cut_eligible` while a row is `pending`, while the packet is empty,
//! or (for `hard_cut_eligible`) while a release hold is unresolved, an open PR
//! is not terminally dispositioned, or the open-PR rows were not captured at
//! the observed main commit.
//!
//! The check is deterministic and network-free. It does not read GitHub, so it
//! cannot establish that a cited PR merged or that an issue is closed; those
//! facts are captured into the packet by whoever edits it, and the live
//! comparison belongs to the cut transaction. A passing check therefore means
//! the packet is internally consistent and honestly staged, not that its
//! evidence is current.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{FixKind, PolicyReportSpec, finish_policy_report, write_report};

pub(crate) const RELEASE_SELECTION_PATH: &str = "policy/release-selection.json";

const RULE_SCHEMA: &str = "schema";
const RULE_VOCABULARY: &str = "vocabulary";
const RULE_UNIQUENESS: &str = "uniqueness";
const RULE_EVIDENCE: &str = "evidence";
const RULE_DEPENDENCY: &str = "dependency";
const RULE_ISSUE_PARTITION: &str = "issue_partition";
const RULE_STAGE_LAW: &str = "stage_law";

/// Used by the negative-fixture coverage test so a rule cannot ship without one.
#[cfg(test)]
const RULE_IDS: &[&str] = &[
    RULE_SCHEMA,
    RULE_VOCABULARY,
    RULE_UNIQUENESS,
    RULE_EVIDENCE,
    RULE_DEPENDENCY,
    RULE_ISSUE_PARTITION,
    RULE_STAGE_LAW,
];

const SCHEMA_VERSION: &str = "1";
const KIND: &str = "ripr_release_selection_packet";
const RELEASE: &str = "0.11.0";
const AUTHORITY_ISSUE: u64 = 2766;

/// Candidate-state stages in order; the packet may name only the first three.
/// `candidate_materialized` and `qualification_eligible` belong to #1609 and
/// #2769 and are never asserted here.
const STAGES: &[&str] = &["scope_pending", "scope_closed", "hard_cut_eligible"];

const RESOLUTIONS: &[&str] = &[
    "pending",
    "landed_and_accepted",
    "accepted_defer_with_non_claim",
    "candidate_exclusion_with_manifest",
    "failed_release_hold",
    "final_candidate_execution_only",
    "historical_evidence_only",
];

const MATERIALITY: &[&str] = &["release_required", "release_optional"];

const PR_DISPOSITIONS: &[&str] = &[
    "release_required_product",
    "release_required_control",
    "release_required_test_infrastructure",
    "release_optional_pending_decision",
    "accepted_defer_with_non_claim",
    "candidate_exclusion_with_manifest",
    "blocked_on_named_authority",
    "superseded",
    "unrelated_to_selected_candidate",
];

/// Open-PR dispositions that carry no unfinished work; the rest mean a PR is
/// still expected to land or is waiting, which `hard_cut_eligible` forbids.
const TERMINAL_PR_DISPOSITIONS: &[&str] = &[
    "accepted_defer_with_non_claim",
    "candidate_exclusion_with_manifest",
    "superseded",
    "unrelated_to_selected_candidate",
];

const OUTSIDE_REASONS: &[&str] = &[
    "downstream_release_control",
    "post_release",
    "later_release_candidate",
    "rolling",
    "superseded_by_claim",
];

/// A reason sentence shorter than this is a label, not a statement of what the
/// release does not claim.
const MIN_STATEMENT_CHARS: usize = 24;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Packet {
    schema_version: String,
    kind: String,
    release: String,
    authority_issue: u64,
    stage: String,
    observed_main: ObservedMain,
    claims: Vec<Claim>,
    outside_selection: Vec<Outside>,
    open_prs: OpenPrs,
    census: Census,
    non_claims: Vec<String>,
}

/// The open `release-check` issues recorded when the packet was last
/// reconciled. The offline check cannot read GitHub, so this recorded census is
/// what makes "every release-check issue has a place" checkable; refreshing it
/// from GitHub is part of editing the packet.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Census {
    captured_on: String,
    open_release_check_issues: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ObservedMain {
    sha: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    claim_id: String,
    owner_issue: u64,
    #[serde(default)]
    related_issues: Vec<u64>,
    materiality: String,
    resolution: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    implementation_refs: Vec<String>,
    #[serde(default)]
    proof_refs: Vec<String>,
    #[serde(default)]
    execution_refs: Vec<String>,
    #[serde(default)]
    manifest_ref: Option<String>,
    candidate_effect: String,
    #[serde(default)]
    non_claim: Option<String>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Outside {
    issue: u64,
    reason: String,
    note: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OpenPrs {
    /// Main commit the open-PR rows were captured against; `null` until the
    /// roster is captured at the cut.
    captured_at_main: Option<String>,
    rows: Vec<OpenPr>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OpenPr {
    pr: u64,
    head: String,
    disposition: String,
    note: String,
}

#[derive(Debug, Default, Serialize)]
struct Summary {
    claims: usize,
    by_resolution: BTreeMap<String, usize>,
    outside_selection: usize,
    open_pr_rows: usize,
    stage: String,
}

struct Outcome {
    violations: Vec<String>,
    summary: Summary,
}

pub(crate) fn check_release_selection() -> Result<(), String> {
    let text = match std::fs::read_to_string(RELEASE_SELECTION_PATH) {
        Ok(text) => text,
        Err(err) => {
            let violations = vec![format!(
                "{RELEASE_SELECTION_PATH} [{RULE_SCHEMA}] cannot be read: {err}"
            )];
            return finish_policy_report(report_spec(), &violations);
        }
    };
    let outcome = evaluate_release_selection(RELEASE_SELECTION_PATH, &text);
    write_report("release-selection-packet.json", &selection_json(&outcome))?;
    finish_policy_report(report_spec(), &outcome.violations)
}

fn report_spec() -> PolicyReportSpec<'static> {
    PolicyReportSpec {
        report_file: "release-selection-packet.md",
        check: "check-release-selection",
        why_it_matters: "The selected-claim packet is what the hard cut consumes: every claim has one resolution, every deferral names what the release does not claim, and the stage cannot advance past a pending row or an unresolved release hold. Without this check the packet can say `hard_cut_eligible` while a row is still pending, which is the false-ready state the cut exists to prevent. The check is offline and does not read GitHub, so it proves internal consistency and honest staging, not that cited evidence is current.",
        fix_kind: FixKind::AuthorDecisionRequired,
        recommended_fixes: &[
            "Give every claim exactly one resolution from the #2766 vocabulary; keep `pending` rows visible with a `next` owner rather than guessing a terminal state.",
            "State the public non-claim for every `accepted_defer_with_non_claim` row and bind every `landed_and_accepted` row to a merged PR or commit and a proof reference.",
            "Keep every release-check issue in exactly one place: a claim row or `outside_selection`.",
            "Only advance `stage` after the rows it requires are terminal; `hard_cut_eligible` also needs the open-PR roster captured at an exact main commit.",
        ],
        rerun_command: "cargo xtask check-release-targets",
        exception_template: None,
    }
}

fn rule(id: &str, message: impl AsRef<str>) -> String {
    format!("{id} :: {}", message.as_ref())
}

fn is_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn blank(value: &str) -> bool {
    value.trim().is_empty()
}

fn statement_ok(value: Option<&str>) -> bool {
    value.is_some_and(|text| text.trim().chars().count() >= MIN_STATEMENT_CHARS)
}

/// `pr:<n>` for a merged PR or `commit:<40 hex>`; nothing else counts as an
/// implementation identity, so an issue link cannot stand in for a landing.
fn implementation_ref_ok(value: &str) -> bool {
    if let Some(number) = value.strip_prefix("pr:") {
        return !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
    }
    value.strip_prefix("commit:").is_some_and(is_sha)
}

fn proof_ref_ok(value: &str) -> bool {
    !blank(value) && (value.starts_with("https://github.com/") || value.starts_with("docs/"))
}

/// Evaluate one packet document. Pure: no filesystem, no network, no clock.
fn evaluate_release_selection(path: &str, text: &str) -> Outcome {
    let mut violations = Vec::new();
    let packet: Packet = match serde_json::from_str(text) {
        Ok(packet) => packet,
        Err(err) => {
            violations.push(format!("{path} [{RULE_SCHEMA}] {err}"));
            return Outcome {
                violations,
                summary: Summary::default(),
            };
        }
    };

    check_header(&packet, &mut violations);
    check_claims(&packet, &mut violations);
    check_dependency_cycles(&packet, &mut violations);
    check_outside(&packet, &mut violations);
    check_partition(&packet, &mut violations);
    check_open_prs(&packet, &mut violations);
    check_stage(&packet, &mut violations);

    let mut by_resolution = BTreeMap::new();
    for claim in &packet.claims {
        *by_resolution.entry(claim.resolution.clone()).or_insert(0) += 1;
    }
    let summary = Summary {
        claims: packet.claims.len(),
        by_resolution,
        outside_selection: packet.outside_selection.len(),
        open_pr_rows: packet.open_prs.rows.len(),
        stage: packet.stage.clone(),
    };
    let violations = violations
        .into_iter()
        .map(|violation| match violation.split_once(" :: ") {
            Some((id, message)) => format!("{path} [{id}] {message}"),
            None => format!("{path} {violation}"),
        })
        .collect();
    Outcome {
        violations,
        summary,
    }
}

fn check_header(packet: &Packet, violations: &mut Vec<String>) {
    if packet.schema_version != SCHEMA_VERSION {
        violations.push(rule(
            RULE_SCHEMA,
            format!("schema_version must be \"{SCHEMA_VERSION}\""),
        ));
    }
    if packet.kind != KIND {
        violations.push(rule(RULE_SCHEMA, format!("kind must be \"{KIND}\"")));
    }
    if packet.release != RELEASE {
        violations.push(rule(RULE_SCHEMA, format!("release must be \"{RELEASE}\"")));
    }
    if packet.authority_issue != AUTHORITY_ISSUE {
        violations.push(rule(
            RULE_SCHEMA,
            format!("authority_issue must be {AUTHORITY_ISSUE}"),
        ));
    }
    if !STAGES.contains(&packet.stage.as_str()) {
        violations.push(rule(
            RULE_VOCABULARY,
            format!(
                "stage `{}` is not one of {}; later stages belong to #1609 and #2769",
                packet.stage,
                STAGES.join(", ")
            ),
        ));
    }
    if let Some(sha) = &packet.observed_main.sha
        && !is_sha(sha)
    {
        violations.push(rule(
            RULE_SCHEMA,
            "observed_main.sha must be a 40-character lowercase commit SHA or null",
        ));
    }
    let date = packet.census.captured_on.as_bytes();
    let date_ok = date.len() == 10
        && date.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        });
    if !date_ok {
        violations.push(rule(
            RULE_SCHEMA,
            "census.captured_on must be a YYYY-MM-DD date",
        ));
    }
    if packet.non_claims.is_empty() || packet.non_claims.iter().any(|text| blank(text)) {
        violations.push(rule(
            RULE_SCHEMA,
            "non_claims must be a non-empty list of non-blank statements",
        ));
    }
}

fn check_claims(packet: &Packet, violations: &mut Vec<String>) {
    let ids: BTreeSet<&str> = packet
        .claims
        .iter()
        .map(|claim| claim.claim_id.as_str())
        .collect();
    let mut seen_ids = BTreeSet::new();
    for claim in &packet.claims {
        let id = claim.claim_id.as_str();
        if blank(id) {
            violations.push(rule(RULE_SCHEMA, "a claim has a blank claim_id"));
            continue;
        }
        if !seen_ids.insert(id) {
            violations.push(rule(
                RULE_UNIQUENESS,
                format!("claim_id `{id}` is repeated"),
            ));
        }
        if !RESOLUTIONS.contains(&claim.resolution.as_str()) {
            violations.push(rule(
                RULE_VOCABULARY,
                format!(
                    "claim `{id}` resolution `{}` is not in the #2766 vocabulary",
                    claim.resolution
                ),
            ));
        }
        if !MATERIALITY.contains(&claim.materiality.as_str()) {
            violations.push(rule(
                RULE_VOCABULARY,
                format!(
                    "claim `{id}` materiality `{}` must be release_required or release_optional",
                    claim.materiality
                ),
            ));
        }
        if blank(&claim.candidate_effect) {
            violations.push(rule(
                RULE_EVIDENCE,
                format!("claim `{id}` states no candidate_effect"),
            ));
        }
        for dependency in &claim.depends_on {
            if dependency == id || !ids.contains(dependency.as_str()) {
                violations.push(rule(
                    RULE_DEPENDENCY,
                    format!("claim `{id}` depends on unknown or self claim `{dependency}`"),
                ));
            }
        }
        check_claim_evidence(claim, violations);
    }
}

/// A dependency cycle means no member can be the first to be terminal.
fn check_dependency_cycles(packet: &Packet, violations: &mut Vec<String>) {
    let edges: BTreeMap<&str, Vec<&str>> = packet
        .claims
        .iter()
        .map(|claim| {
            (
                claim.claim_id.as_str(),
                claim.depends_on.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    for start in edges.keys() {
        let mut stack: Vec<&str> = edges[start].clone();
        let mut seen = BTreeSet::new();
        while let Some(next) = stack.pop() {
            if next == *start {
                violations.push(rule(
                    RULE_DEPENDENCY,
                    format!("claim `{start}` is part of a dependency cycle"),
                ));
                break;
            }
            if seen.insert(next)
                && let Some(more) = edges.get(next)
            {
                stack.extend(more.iter().copied());
            }
        }
    }
}

fn check_claim_evidence(claim: &Claim, violations: &mut Vec<String>) {
    let id = claim.claim_id.as_str();
    for reference in &claim.implementation_refs {
        if !implementation_ref_ok(reference) {
            violations.push(rule(
                RULE_EVIDENCE,
                format!(
                    "claim `{id}` implementation ref `{reference}` is not `pr:<number>` or `commit:<40-hex>`; an issue link is not a landing"
                ),
            ));
        }
    }
    for reference in claim.proof_refs.iter().chain(&claim.execution_refs) {
        if !proof_ref_ok(reference) {
            violations.push(rule(
                RULE_EVIDENCE,
                format!(
                    "claim `{id}` evidence ref `{reference}` must be a github.com URL or a docs/ path"
                ),
            ));
        }
    }
    let needs = |ok: bool, what: &str, violations: &mut Vec<String>| {
        if !ok {
            violations.push(rule(
                RULE_EVIDENCE,
                format!("claim `{id}` is `{}` but {what}", claim.resolution),
            ));
        }
    };
    match claim.resolution.as_str() {
        "landed_and_accepted" => {
            needs(
                !claim.implementation_refs.is_empty(),
                "names no merged implementation (`pr:` or `commit:`)",
                violations,
            );
            needs(
                !claim.proof_refs.is_empty(),
                "names no proof or review reference",
                violations,
            );
        }
        "accepted_defer_with_non_claim" => needs(
            statement_ok(claim.non_claim.as_deref()),
            "states no public non-claim of at least a sentence",
            violations,
        ),
        "candidate_exclusion_with_manifest" => {
            needs(
                claim.manifest_ref.as_deref().is_some_and(|m| !blank(m)),
                "names no exclusion manifest_ref",
                violations,
            );
            needs(
                statement_ok(claim.non_claim.as_deref()),
                "states no public non-claim of at least a sentence",
                violations,
            );
        }
        "failed_release_hold" => needs(
            statement_ok(claim.next.as_deref()),
            "does not name the failing predicate and the smallest repair owner in `next`",
            violations,
        ),
        "final_candidate_execution_only" => {
            needs(
                !claim.execution_refs.is_empty(),
                "names no execution owner reference (the issue that runs it on the candidate)",
                violations,
            );
            needs(
                statement_ok(claim.non_claim.as_deref()),
                "states no non-claim for the pre-cut state",
                violations,
            );
        }
        "historical_evidence_only" => needs(
            !claim.proof_refs.is_empty(),
            "names no retained evidence reference",
            violations,
        ),
        "pending" => needs(
            statement_ok(claim.next.as_deref()),
            "names no owner and next transition in `next`",
            violations,
        ),
        _ => {}
    }
}

fn check_outside(packet: &Packet, violations: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    for outside in &packet.outside_selection {
        if !seen.insert(outside.issue) {
            violations.push(rule(
                RULE_UNIQUENESS,
                format!(
                    "issue #{} appears twice in outside_selection",
                    outside.issue
                ),
            ));
        }
        if !OUTSIDE_REASONS.contains(&outside.reason.as_str()) {
            violations.push(rule(
                RULE_VOCABULARY,
                format!(
                    "outside_selection #{} reason `{}` is not one of {}",
                    outside.issue,
                    outside.reason,
                    OUTSIDE_REASONS.join(", ")
                ),
            ));
        }
        if !statement_ok(Some(&outside.note)) {
            violations.push(rule(
                RULE_EVIDENCE,
                format!(
                    "outside_selection #{} gives no reason of at least a sentence",
                    outside.issue
                ),
            ));
        }
    }
}

/// An issue is either a selected claim's owner or explicitly outside the
/// selection, never both and never counted twice as an owner.
fn check_partition(packet: &Packet, violations: &mut Vec<String>) {
    let mut owners = BTreeMap::new();
    for claim in &packet.claims {
        if let Some(previous) = owners.insert(claim.owner_issue, claim.claim_id.as_str()) {
            violations.push(rule(
                RULE_ISSUE_PARTITION,
                format!(
                    "issue #{} owns both `{previous}` and `{}`; one owner issue backs one claim",
                    claim.owner_issue, claim.claim_id
                ),
            ));
        }
    }
    for outside in &packet.outside_selection {
        if let Some(claim) = owners.get(&outside.issue) {
            violations.push(rule(
                RULE_ISSUE_PARTITION,
                format!(
                    "issue #{} is outside_selection and also owns claim `{claim}`",
                    outside.issue
                ),
            ));
        }
    }
    let mut census_seen = BTreeSet::new();
    for issue in &packet.census.open_release_check_issues {
        if !census_seen.insert(*issue) {
            violations.push(rule(
                RULE_UNIQUENESS,
                format!("census lists issue #{issue} twice"),
            ));
        }
    }
    let placed: BTreeSet<u64> = packet
        .claims
        .iter()
        .flat_map(|claim| std::iter::once(claim.owner_issue).chain(claim.related_issues.clone()))
        .chain(packet.outside_selection.iter().map(|outside| outside.issue))
        .collect();
    for issue in &census_seen {
        if !placed.contains(issue) {
            violations.push(rule(
                RULE_ISSUE_PARTITION,
                format!(
                    "open release-check issue #{issue} is neither a claim, a related issue nor outside_selection"
                ),
            ));
        }
    }
    let mut related_seen: BTreeMap<u64, &str> = BTreeMap::new();
    for claim in &packet.claims {
        let mut own_related = BTreeSet::new();
        for related in &claim.related_issues {
            if !own_related.insert(*related) {
                violations.push(rule(
                    RULE_ISSUE_PARTITION,
                    format!(
                        "issue #{related} is listed twice in related_issues of `{}`",
                        claim.claim_id
                    ),
                ));
            }
            if packet.outside_selection.iter().any(|o| o.issue == *related) {
                violations.push(rule(
                    RULE_ISSUE_PARTITION,
                    format!(
                        "issue #{related} is related to `{}` and also outside_selection",
                        claim.claim_id
                    ),
                ));
            }
            if let Some(owner) = owners.get(related) {
                violations.push(rule(
                    RULE_ISSUE_PARTITION,
                    format!(
                        "issue #{related} is related to `{}` but owns claim `{owner}`",
                        claim.claim_id
                    ),
                ));
            }
            if let Some(previous) = related_seen.insert(*related, claim.claim_id.as_str())
                && previous != claim.claim_id
            {
                violations.push(rule(
                    RULE_ISSUE_PARTITION,
                    format!(
                        "issue #{related} is related to both `{previous}` and `{}`",
                        claim.claim_id
                    ),
                ));
            }
        }
    }
}

fn check_open_prs(packet: &Packet, violations: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    for row in &packet.open_prs.rows {
        if !seen.insert(row.pr) {
            violations.push(rule(
                RULE_UNIQUENESS,
                format!("open PR #{} has two rows", row.pr),
            ));
        }
        if !PR_DISPOSITIONS.contains(&row.disposition.as_str()) {
            violations.push(rule(
                RULE_VOCABULARY,
                format!(
                    "open PR #{} disposition `{}` is not in the #2766 vocabulary",
                    row.pr, row.disposition
                ),
            ));
        }
        if !is_sha(&row.head) {
            violations.push(rule(
                RULE_EVIDENCE,
                format!("open PR #{} head must be an exact 40-hex SHA", row.pr),
            ));
        }
        if !statement_ok(Some(&row.note)) {
            violations.push(rule(
                RULE_EVIDENCE,
                format!("open PR #{} gives no disposition note", row.pr),
            ));
        }
    }
    if let Some(sha) = &packet.open_prs.captured_at_main
        && !is_sha(sha)
    {
        violations.push(rule(
            RULE_SCHEMA,
            "open_prs.captured_at_main must be a 40-character lowercase commit SHA or null",
        ));
    }
    if packet.open_prs.captured_at_main.is_none() && !packet.open_prs.rows.is_empty() {
        violations.push(rule(
            RULE_STAGE_LAW,
            "open_prs has rows but no captured_at_main; a roster is only meaningful against an exact main",
        ));
    }
}

fn check_stage(packet: &Packet, violations: &mut Vec<String>) {
    let stage = packet.stage.as_str();
    let closed = stage == "scope_closed" || stage == "hard_cut_eligible";
    let eligible = stage == "hard_cut_eligible";
    if closed {
        if packet.claims.is_empty() || packet.census.open_release_check_issues.is_empty() {
            violations.push(rule(
                RULE_STAGE_LAW,
                format!("stage is {stage} but the packet has no claims or no recorded census; an empty packet is not a closed one"),
            ));
        }
        for claim in packet.claims.iter().filter(|c| c.resolution == "pending") {
            violations.push(rule(
                RULE_STAGE_LAW,
                format!(
                    "stage is {stage} but claim `{}` is pending; pending is absent before scope_closed",
                    claim.claim_id
                ),
            ));
        }
    }
    if eligible {
        for claim in packet
            .claims
            .iter()
            .filter(|c| c.resolution == "failed_release_hold")
        {
            violations.push(rule(
                RULE_STAGE_LAW,
                format!(
                    "stage is hard_cut_eligible but claim `{}` is a failed_release_hold",
                    claim.claim_id
                ),
            ));
        }
        if packet.observed_main.sha.is_none() {
            violations.push(rule(
                RULE_STAGE_LAW,
                "hard_cut_eligible requires observed_main.sha",
            ));
        }
        if packet.open_prs.captured_at_main.is_none() {
            violations.push(rule(
                RULE_STAGE_LAW,
                "hard_cut_eligible requires the open-PR roster captured at an exact main commit",
            ));
        } else if packet.open_prs.captured_at_main != packet.observed_main.sha {
            violations.push(rule(
                RULE_STAGE_LAW,
                "open_prs.captured_at_main must equal observed_main.sha at hard_cut_eligible",
            ));
        }
        for row in &packet.open_prs.rows {
            if PR_DISPOSITIONS.contains(&row.disposition.as_str())
                && !TERMINAL_PR_DISPOSITIONS.contains(&row.disposition.as_str())
            {
                violations.push(rule(
                    RULE_STAGE_LAW,
                    format!(
                        "hard_cut_eligible but open PR #{} is `{}`; only {} are terminal",
                        row.pr,
                        row.disposition,
                        TERMINAL_PR_DISPOSITIONS.join(", ")
                    ),
                ));
            }
        }
    }
}

fn selection_json(outcome: &Outcome) -> String {
    #[derive(Serialize)]
    struct Report<'a> {
        check: &'a str,
        status: &'a str,
        summary: &'a Summary,
        violations: &'a [String],
    }
    let status = if outcome.violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let report = Report {
        check: "check-release-selection",
        status,
        summary: &outcome.summary,
        violations: &outcome.violations,
    };
    // Serializing owned strings and integers cannot fail.
    serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests;
