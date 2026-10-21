//! Direct-main release ledger (#2768): one record per commit in the exact
//! `base..cut` range, with a closed disposition vocabulary and a separate
//! final-tree-effect axis. Dispositions are `rule_derived`; the ledger does not
//! claim per-commit semantic review.
//!
//! This command validates a captured ledger offline and writes the JSON and
//! Markdown projections from one normalized DTO. It does not select, qualify,
//! merge, tag or publish a candidate. The historical C/T model stays in
//! `release-denominator` (RIPR-SPEC-0146).

mod dto;
mod validate;

#[cfg(test)]
mod tests;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;

use dto::{Ledger, STAGE_FINAL};
use validate::{Context, SelectionPacket, validate};

const REPORT_NAME: &str = "release-ledger";
const JSON_FILE: &str = "release-ledger.json";
const MARKDOWN_FILE: &str = "release-ledger.md";

struct Args<'a> {
    input: &'a str,
    packet: Option<&'a str>,
    markdown: Option<&'a str>,
    manifest: Option<&'a str>,
    require_final: bool,
}

pub(crate) fn release_ledger(args: &[String]) -> Result<(), String> {
    let args = parse_args(args)?;
    let ledger: Ledger = read_json(args.input, "ledger")?;
    let context = Context {
        packet: args.packet.map(read_packet).transpose()?,
        markdown: args
            .markdown
            .map(|path| {
                fs::read_to_string(path).map_err(|error| format!("failed to read {path}: {error}"))
            })
            .transpose()?,
        manifest: args
            .manifest
            .map(|path| read_json::<Value>(path, "manifest"))
            .transpose()?,
    };
    let mut reasons = validate(&ledger, &context);
    if args.require_final && ledger.stage != STAGE_FINAL {
        reasons.push("--final requires a ledger at stage final".to_string());
    }
    let status = if reasons.is_empty() {
        "valid"
    } else {
        "invalid"
    };
    let json_text = serde_json::to_string_pretty(&report_json(&ledger, status, &reasons))
        .map_err(|error| format!("failed to serialize release-ledger JSON: {error}"))?;
    crate::write_report(JSON_FILE, &format!("{json_text}\n"))?;
    crate::write_report(MARKDOWN_FILE, &report_markdown(&ledger, status, &reasons))?;
    println!("Wrote target/ripr/reports/{JSON_FILE}");
    println!("Wrote target/ripr/reports/{MARKDOWN_FILE}");
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "release-ledger is invalid ({} reasons); first: {}",
            reasons.len(),
            reasons[0]
        ))
    }
}

fn parse_args(args: &[String]) -> Result<Args<'_>, String> {
    let mut parsed = Args {
        input: "",
        packet: None,
        markdown: None,
        manifest: None,
        require_final: false,
    };
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if flag == "--final" {
            parsed.require_final = true;
            index += 1;
            continue;
        }
        let value = args
            .get(index + 1)
            .map(String::as_str)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag {
            "--input" => parsed.input = value,
            "--packet" => parsed.packet = Some(value),
            "--markdown" => parsed.markdown = Some(value),
            "--manifest" => parsed.manifest = Some(value),
            _ => return Err(format!("unknown release-ledger argument `{flag}`")),
        }
        index += 2;
    }
    if parsed.input.is_empty() {
        return Err("release-ledger requires --input <ledger.json>".to_string());
    }
    Ok(parsed)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &str, what: &str) -> Result<T, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {what} {path}: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("failed to parse {what} {path}: {error}"))
}

/// The packet digest is over the raw file bytes, so any edit to the packet is a
/// different identity.
fn read_packet(path: &str) -> Result<SelectionPacket, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read packet {path}: {error}"))?;
    let mut packet: SelectionPacket = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse packet {path}: {error}"))?;
    packet.digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    Ok(packet)
}

fn counts<'a>(values: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, u64> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts
}

fn report_json(ledger: &Ledger, status: &str, reasons: &[String]) -> Value {
    let unresolved = ledger
        .records
        .iter()
        .filter(|record| record.release_disposition == dto::BLOCKING)
        .count();
    json!({
        "report": REPORT_NAME,
        "schema_version": dto::SCHEMA_VERSION,
        "status": status,
        "stage": ledger.stage,
        "final_eligible": status == "valid" && ledger.stage == STAGE_FINAL,
        "adjudication_basis": ledger.adjudication_basis,
        "rule_version": ledger.rule_version,
        "first_parent_commits": ledger.range.first_parent.len(),
        "reachable_commits": ledger.range.reachable.len(),
        "blocking_unresolved_records": unresolved,
        "digests": ledger.digests,
        "counts_by_disposition": counts(ledger.records.iter().map(|r| r.release_disposition.as_str())),
        "counts_by_final_tree_effect": counts(ledger.records.iter().map(|r| r.final_tree_effect.as_str())),
        "reconciliation_reasons": reasons,
        "ledger": ledger,
        "authority_boundary": "direct_main_ledger_only",
        "must_not_claim": [
            "per-commit semantic review",
            "candidate qualification",
            "merge approval",
            "release publication",
            "source integration",
        ],
    })
}

fn escape_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn report_markdown(ledger: &Ledger, status: &str, reasons: &[String]) -> String {
    let mut markdown = format!(
        "# Direct-main release ledger\n\n- Status: `{status}`\n- Stage: `{}`\n- Adjudication basis: `{}`\n- Rule version: `{}`\n- Base: `{}`\n- Cut (exact main): `{}`\n- Cut tree: `{}`\n- First-parent commits: {}\n- Reachable commits: {}\n- first-parent digest: `{}`\n- reachable digest: `{}`\n- records digest: `{}`\n- tree digest: `{}`\n\n",
        ledger.stage,
        ledger.adjudication_basis,
        ledger.rule_version,
        ledger.source.base_sha,
        ledger.source.cut_sha,
        ledger.source.cut_tree_sha,
        ledger.range.first_parent.len(),
        ledger.range.reachable.len(),
        ledger.digests.first_parent,
        ledger.digests.reachable,
        ledger.digests.records,
        ledger.digests.tree,
    );
    markdown.push_str("## Counts\n\n| Disposition | Count |\n| --- | ---: |\n");
    for (name, count) in counts(
        ledger
            .records
            .iter()
            .map(|r| r.release_disposition.as_str()),
    ) {
        markdown.push_str(&format!("| `{}` | {count} |\n", escape_cell(name)));
    }
    markdown.push_str("\n| Final tree effect | Count |\n| --- | ---: |\n");
    for (name, count) in counts(ledger.records.iter().map(|r| r.final_tree_effect.as_str())) {
        markdown.push_str(&format!("| `{}` | {count} |\n", escape_cell(name)));
    }
    markdown.push_str("\n## Records\n\n| Position | Commit | Disposition | Tree effect | Subject |\n| ---: | --- | --- | --- | --- |\n");
    for record in &ledger.records {
        let position = record
            .first_parent_position
            .map_or_else(|| "-".to_string(), |position| position.to_string());
        markdown.push_str(&format!(
            "| {position} | `{}` | `{}` | `{}` | {} |\n",
            record.commit_sha,
            escape_cell(&record.release_disposition),
            escape_cell(&record.final_tree_effect),
            escape_cell(&record.subject),
        ));
    }
    markdown.push_str("\n## Open PRs at the cut\n\n");
    if ledger.open_prs.is_empty() {
        markdown.push_str("None.\n");
    }
    for pr in &ledger.open_prs {
        markdown.push_str(&format!(
            "- #{} `{}` `{}`\n",
            pr.number, pr.head_sha, pr.disposition
        ));
    }
    markdown.push_str("\n## Reconciliation\n\n");
    if reasons.is_empty() {
        markdown.push_str("The retained range, records, claims, open PRs and digests reconcile.\n");
    }
    for reason in reasons {
        markdown.push_str(&format!("- {}\n", escape_cell(reason)));
    }
    markdown.push_str("\n## Must not claim\n\n- per-commit semantic review\n- candidate qualification\n- merge approval\n- release publication\n- source integration\n");
    markdown
}
