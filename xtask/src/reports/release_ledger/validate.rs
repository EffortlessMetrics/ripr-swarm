//! Fail-closed validation of a release ledger against its own retained range,
//! the optional #2766 selection packet, and its Markdown and manifest
//! projections. Offline: no Git or GitHub reads.

use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use super::dto::{
    ADJUDICATION_BASIS, BLOCKING, CLAIM_RESOLUTIONS, DISPOSITIONS, Digests, HISTORY_NO_BLOB_EFFECT,
    KIND, Ledger, NOT_REACHABLE, OPEN_PR_DISPOSITIONS, PRESENT_BUT_SUPERSEDED, PRESENT_EFFECTIVE,
    Record, SCHEMA_VERSION, SOURCE_SYNC_EFFECTS, STAGE_DRY_RUN, STAGE_FINAL, TREE_EFFECTS,
};
use crate::reports::release_denominator::{digest_json, is_sha};

/// The part of the #2766 selected-claim packet the ledger must agree with.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(super) struct SelectionPacket {
    pub(super) digest: String,
    pub(super) totals: BTreeMap<String, u64>,
    #[serde(default)]
    pub(super) open_prs: Vec<PacketOpenPr>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(super) struct PacketOpenPr {
    pub(super) number: u64,
    pub(super) head_sha: String,
    pub(super) disposition: String,
}

#[derive(Default)]
pub(super) struct Context {
    pub(super) packet: Option<SelectionPacket>,
    pub(super) markdown: Option<String>,
    pub(super) manifest: Option<Value>,
}

pub(super) fn compute_digests(ledger: &Ledger) -> Result<Digests, String> {
    let mut reachable = ledger.range.reachable.clone();
    reachable.sort();
    let mut records: Vec<&Record> = ledger.records.iter().collect();
    records.sort_by(|left, right| left.commit_sha.cmp(&right.commit_sha));
    Ok(Digests {
        first_parent: digest_json(&ledger.range.first_parent)?,
        reachable: digest_json(&reachable)?,
        records: digest_json(&records)?,
        tree: digest_json(&[&ledger.source.cut_sha, &ledger.source.cut_tree_sha])?,
    })
}

/// Canonical reachable order: first-parent order, then the rest sorted.
fn canonical_reachable(first_parent: &[String], reachable: &[String]) -> Vec<String> {
    let first: BTreeSet<&String> = first_parent.iter().collect();
    let mut rest: Vec<String> = reachable
        .iter()
        .filter(|sha| !first.contains(sha))
        .cloned()
        .collect();
    rest.sort();
    let mut canonical = first_parent.to_vec();
    canonical.extend(rest);
    canonical
}

pub(super) fn validate(ledger: &Ledger, context: &Context) -> Vec<String> {
    let mut reasons = Vec::new();
    validate_header(ledger, &mut reasons);
    let range_ok = validate_range(ledger, &mut reasons);
    validate_records(ledger, range_ok, &mut reasons);
    validate_merged_prs(ledger, &mut reasons);
    validate_selected_claims(ledger, context, &mut reasons);
    validate_open_prs(ledger, context, &mut reasons);
    validate_final(ledger, &mut reasons);
    validate_digests(ledger, context, &mut reasons);
    reasons
}

fn validate_header(ledger: &Ledger, reasons: &mut Vec<String>) {
    if ledger.schema_version != SCHEMA_VERSION {
        reasons.push(format!("schema_version must be {SCHEMA_VERSION}"));
    }
    if ledger.kind != KIND {
        reasons.push(format!("kind must be {KIND}"));
    }
    if ledger.adjudication_basis != ADJUDICATION_BASIS {
        reasons.push(format!(
            "adjudication_basis must be {ADJUDICATION_BASIS}: the ledger does not claim per-commit semantic review"
        ));
    }
    if ledger.rule_version.trim().is_empty() {
        reasons.push("rule_version is empty".to_string());
    }
    if ledger.stage != STAGE_DRY_RUN && ledger.stage != STAGE_FINAL {
        reasons.push(format!("stage `{}` is not dry_run or final", ledger.stage));
    }
    for (name, value) in [
        ("source.base_sha", &ledger.source.base_sha),
        ("source.cut_sha", &ledger.source.cut_sha),
        ("source.cut_tree_sha", &ledger.source.cut_tree_sha),
        ("source.main_sha", &ledger.source.main_sha),
    ] {
        if !is_sha(value) {
            reasons.push(format!("{name} is not a 40-hex SHA"));
        }
    }
    if ledger.source.cut_sha != ledger.source.main_sha {
        reasons.push(format!(
            "cut {} is not exact main {}",
            ledger.source.cut_sha, ledger.source.main_sha
        ));
    }
}

/// Returns whether the range is internally coherent enough to key records by.
fn validate_range(ledger: &Ledger, reasons: &mut Vec<String>) -> bool {
    let range = &ledger.range;
    let start = reasons.len();
    if range.first_parent.is_empty() {
        reasons.push("range.first_parent is empty".to_string());
    }
    for (name, list) in [
        ("range.first_parent", &range.first_parent),
        ("range.reachable", &range.reachable),
    ] {
        let mut seen = BTreeSet::new();
        for sha in list {
            if !is_sha(sha) {
                reasons.push(format!("{name} entry `{sha}` is not a 40-hex SHA"));
            } else if !seen.insert(sha) {
                reasons.push(format!("{name} lists {sha} twice"));
            }
        }
    }
    let reachable: BTreeSet<&String> = range.reachable.iter().collect();
    for sha in &range.first_parent {
        if !reachable.contains(sha) {
            reasons.push(format!(
                "first-parent commit {sha} is not in range.reachable"
            ));
        }
    }
    if reachable.contains(&ledger.source.base_sha) {
        reasons.push("range.reachable contains the base commit".to_string());
    }
    if range.first_parent.last() != Some(&ledger.source.cut_sha) {
        reasons.push("range.first_parent does not end at source.cut_sha".to_string());
    }
    if canonical_reachable(&range.first_parent, &range.reachable) != range.reachable {
        reasons.push(
            "range.reachable is not first-parent order followed by the remaining commits sorted"
                .to_string(),
        );
    }
    reasons.len() == start
}

fn validate_records(ledger: &Ledger, range_ok: bool, reasons: &mut Vec<String>) {
    let range = &ledger.range;
    let reachable: BTreeSet<&String> = range.reachable.iter().collect();
    let position: BTreeMap<&String, u64> = range
        .first_parent
        .iter()
        .enumerate()
        .map(|(index, sha)| (sha, index as u64 + 1))
        .collect();
    let mut seen = BTreeSet::new();
    for record in &ledger.records {
        let sha = &record.commit_sha;
        if !seen.insert(sha) {
            reasons.push(format!("record for {sha} is duplicated"));
            continue;
        }
        if !reachable.contains(sha) {
            reasons.push(format!(
                "record names {sha}, which is outside the exact reachable range"
            ));
        }
        validate_record(ledger, record, position.get(sha).copied(), reasons);
    }
    for sha in &range.reachable {
        if !seen.contains(sha) {
            reasons.push(format!("reachable commit {sha} has no record"));
        }
    }
    if range_ok {
        let order: Vec<&String> = ledger.records.iter().map(|r| &r.commit_sha).collect();
        let expected: Vec<&String> = range.reachable.iter().collect();
        if order != expected && seen.len() == ledger.records.len() && order.len() == expected.len()
        {
            reasons.push("records are not in canonical range order".to_string());
        }
    }
}

fn validate_record(
    ledger: &Ledger,
    record: &Record,
    position: Option<u64>,
    reasons: &mut Vec<String>,
) {
    let sha = &record.commit_sha;
    if record.first_parent_position != position {
        reasons.push(format!(
            "{sha}: first_parent_position {:?} contradicts the range ({position:?})",
            record.first_parent_position
        ));
    }
    if !is_sha(&record.tree_sha) {
        reasons.push(format!("{sha}: tree_sha is not a 40-hex SHA"));
    }
    if record.parent_shas.iter().any(|parent| !is_sha(parent)) {
        reasons.push(format!("{sha}: parent_shas holds a non-SHA"));
    }
    if let Some(index) = position {
        let expected_parent = if index == 1 {
            Some(&ledger.source.base_sha)
        } else {
            ledger.range.first_parent.get(index as usize - 2)
        };
        if record.parent_shas.first() != expected_parent {
            reasons.push(format!(
                "{sha}: first parent does not match the preceding first-parent commit"
            ));
        }
    }
    if record.subject.trim().is_empty() {
        reasons.push(format!("{sha}: subject is empty"));
    }
    if !DISPOSITIONS.contains(&record.release_disposition.as_str()) {
        reasons.push(format!(
            "{sha}: release_disposition `{}` is outside the closed vocabulary",
            record.release_disposition
        ));
    } else if record.release_disposition.starts_with("not_in_cut_") {
        reasons.push(format!(
            "{sha}: a reachable commit cannot be `{}`",
            record.release_disposition
        ));
    }
    if !TREE_EFFECTS.contains(&record.final_tree_effect.as_str()) {
        reasons.push(format!(
            "{sha}: final_tree_effect `{}` is outside the closed vocabulary",
            record.final_tree_effect
        ));
    } else {
        validate_effect(record, reasons);
    }
    if !SOURCE_SYNC_EFFECTS.contains(&record.source_sync_authority_effect.as_str()) {
        reasons.push(format!(
            "{sha}: source_sync_authority_effect `{}` is outside the closed vocabulary",
            record.source_sync_authority_effect
        ));
    }
    if record.acceptance_owner.trim().is_empty() {
        reasons.push(format!("{sha}: acceptance_owner is empty"));
    }
    if record.release_disposition == "included_with_release_nonclaim" && record.claim_ref.is_none()
    {
        reasons.push(format!(
            "{sha}: included_with_release_nonclaim needs a claim_ref"
        ));
    }
    if let Some(claim) = &record.claim_ref
        && !ledger
            .selected_claims
            .rows
            .iter()
            .any(|row| &row.claim_ref == claim)
    {
        reasons.push(format!(
            "{sha}: claim_ref `{claim}` is not a selected-claim row"
        ));
    }
    validate_disposition_effect(record, reasons);
}

fn validate_effect(record: &Record, reasons: &mut Vec<String>) {
    let sha = &record.commit_sha;
    let basis = &record.effect_basis;
    let owned = basis.last_toucher_paths + basis.blamed_lines;
    let contradiction = match record.final_tree_effect.as_str() {
        PRESENT_EFFECTIVE if owned == 0 => {
            Some("present_effective needs a last-toucher path or a blamed line")
        }
        PRESENT_EFFECTIVE if basis.alive_paths == 0 => {
            Some("present_effective needs an alive touched path")
        }
        PRESENT_BUT_SUPERSEDED if basis.alive_paths == 0 || owned > 0 => {
            Some("present_but_superseded needs alive paths and no owned path or line")
        }
        HISTORY_NO_BLOB_EFFECT if basis.alive_paths > 0 || basis.absent_paths == 0 => {
            Some("history_reachable_no_final_blob_effect needs only absent touched paths")
        }
        NOT_REACHABLE => {
            Some("a record in the reachable range cannot be not_reachable_from_candidate")
        }
        _ => None,
    };
    if let Some(message) = contradiction {
        reasons.push(format!(
            "{sha}: final tree effect contradicts the cut tree: {message}"
        ));
    }
    if basis.last_toucher_paths > basis.alive_paths {
        reasons.push(format!("{sha}: last_toucher_paths exceeds alive_paths"));
    }
}

/// The rule's first two steps are exact consequences of the tree effect.
fn validate_disposition_effect(record: &Record, reasons: &mut Vec<String>) {
    if record.override_ref.is_some() {
        return;
    }
    let expected = match record.final_tree_effect.as_str() {
        PRESENT_BUT_SUPERSEDED => Some("included_superseded_by_later_fix"),
        HISTORY_NO_BLOB_EFFECT => Some("included_historical_reachable_only"),
        _ => None,
    };
    if let Some(expected) = expected
        && record.release_disposition != expected
        && record.release_disposition != BLOCKING
    {
        reasons.push(format!(
            "{}: effect {} requires disposition {expected} without an override_ref",
            record.commit_sha, record.final_tree_effect
        ));
    }
}

fn validate_merged_prs(ledger: &Ledger, reasons: &mut Vec<String>) {
    let reachable: BTreeSet<&String> = ledger.range.reachable.iter().collect();
    let mut seen = BTreeSet::new();
    for pr in &ledger.merged_prs {
        if !seen.insert(pr.number) {
            reasons.push(format!("merged PR #{} is listed twice", pr.number));
        }
        if pr.state != "merged" {
            reasons.push(format!(
                "PR #{} has state `{}`, not merged",
                pr.number, pr.state
            ));
        }
        if !reachable.contains(&pr.merge_sha) {
            reasons.push(format!(
                "merged PR #{} names {}, which is outside the reachable range",
                pr.number, pr.merge_sha
            ));
        }
    }
    for record in &ledger.records {
        let Some(pr) = record.first_parent_position.and(record.pr_refs.last()) else {
            continue;
        };
        match ledger.merged_prs.iter().find(|entry| entry.number == *pr) {
            Some(entry) if entry.merge_sha == record.commit_sha => {}
            Some(entry) => reasons.push(format!(
                "{}: PR #{pr} merged as {}, not this commit",
                record.commit_sha, entry.merge_sha
            )),
            None => reasons.push(format!(
                "{}: squash PR #{pr} has no merged-PR row",
                record.commit_sha
            )),
        }
    }
}

fn validate_selected_claims(ledger: &Ledger, context: &Context, reasons: &mut Vec<String>) {
    let claims = &ledger.selected_claims;
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for row in &claims.rows {
        if !seen.insert(&row.claim_ref) {
            reasons.push(format!(
                "selected claim `{}` is listed twice",
                row.claim_ref
            ));
        }
        if !CLAIM_RESOLUTIONS.contains(&row.resolution.as_str()) {
            reasons.push(format!(
                "selected claim `{}` has resolution `{}` outside the #2766 vocabulary",
                row.claim_ref, row.resolution
            ));
        }
        *totals.entry(row.resolution.clone()).or_default() += 1;
    }
    if totals != claims.totals {
        reasons.push("selected_claims.totals do not match its rows".to_string());
    }
    if let Some(packet) = &context.packet {
        if packet.digest != claims.packet_digest {
            reasons.push("selected_claims.packet_digest differs from the #2766 packet".to_string());
        }
        if packet.totals != claims.totals {
            reasons.push("selected-claim totals differ from the #2766 packet".to_string());
        }
    }
}

fn validate_open_prs(ledger: &Ledger, context: &Context, reasons: &mut Vec<String>) {
    let reachable: BTreeSet<&String> = ledger.range.reachable.iter().collect();
    let mut seen = BTreeSet::new();
    for pr in &ledger.open_prs {
        if !seen.insert(pr.number) {
            reasons.push(format!("open PR #{} is listed twice", pr.number));
        }
        if !is_sha(&pr.head_sha) {
            reasons.push(format!("open PR #{} head is not a 40-hex SHA", pr.number));
        } else if reachable.contains(&pr.head_sha) {
            reasons.push(format!(
                "open PR #{} head {} is inside the reachable range",
                pr.number, pr.head_sha
            ));
        }
        if !OPEN_PR_DISPOSITIONS.contains(&pr.disposition.as_str()) {
            reasons.push(format!(
                "open PR #{} disposition `{}` is not valid for a PR outside the cut",
                pr.number, pr.disposition
            ));
        }
    }
    if let Some(packet) = &context.packet {
        let ledger_rows: BTreeSet<(u64, &str, &str)> = ledger
            .open_prs
            .iter()
            .map(|pr| (pr.number, pr.head_sha.as_str(), pr.disposition.as_str()))
            .collect();
        let packet_rows: BTreeSet<(u64, &str, &str)> = packet
            .open_prs
            .iter()
            .map(|pr| (pr.number, pr.head_sha.as_str(), pr.disposition.as_str()))
            .collect();
        if ledger_rows != packet_rows {
            reasons.push("open_prs differ from the #2766 packet roster".to_string());
        }
    }
}

fn validate_final(ledger: &Ledger, reasons: &mut Vec<String>) {
    if ledger.stage != STAGE_FINAL {
        return;
    }
    let unresolved = ledger
        .records
        .iter()
        .filter(|record| record.release_disposition == BLOCKING)
        .count()
        + ledger
            .open_prs
            .iter()
            .filter(|pr| pr.disposition == BLOCKING)
            .count();
    if unresolved > 0 {
        reasons.push(format!(
            "final ledger holds {unresolved} blocking_unresolved rows"
        ));
    }
    match &ledger.audit {
        Some(audit)
            if audit.sha256.len() == 64 && audit.sha256.chars().all(|c| c.is_ascii_hexdigit()) => {}
        _ => reasons.push("final ledger needs an audit reference with a sha256 digest".to_string()),
    }
}

fn validate_digests(ledger: &Ledger, context: &Context, reasons: &mut Vec<String>) {
    let computed = match compute_digests(ledger) {
        Ok(digests) => digests,
        Err(error) => {
            reasons.push(error);
            return;
        }
    };
    for (name, recorded, actual) in [
        (
            "first_parent",
            &ledger.digests.first_parent,
            &computed.first_parent,
        ),
        ("reachable", &ledger.digests.reachable, &computed.reachable),
        ("records", &ledger.digests.records, &computed.records),
        ("tree", &ledger.digests.tree, &computed.tree),
    ] {
        if recorded != actual {
            reasons.push(format!(
                "{name} digest {recorded} does not match recomputed {actual}"
            ));
        }
    }
    if let Some(markdown) = &context.markdown {
        for (name, recorded) in digest_pairs(&ledger.digests) {
            if !markdown.contains(&format!("{name} digest: `{recorded}`")) {
                reasons.push(format!(
                    "Markdown projection disagrees on the {name} digest"
                ));
            }
        }
    }
    if let Some(manifest) = &context.manifest {
        for (name, recorded) in digest_pairs(&ledger.digests) {
            let key = name.replace('-', "_");
            if manifest["digests"][key.as_str()].as_str() != Some(recorded) {
                reasons.push(format!("manifest disagrees on the {name} digest"));
            }
        }
    }
}

fn digest_pairs(digests: &Digests) -> [(&'static str, &str); 4] {
    [
        ("first-parent", &digests.first_parent),
        ("reachable", &digests.reachable),
        ("records", &digests.records),
        ("tree", &digests.tree),
    ]
}
