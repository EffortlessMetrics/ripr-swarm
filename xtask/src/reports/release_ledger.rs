//! Direct-main release ledger (#2768): one record per commit in the exact
//! `base..cut` range, with a closed disposition vocabulary and a separate
//! final-tree-effect axis. Dispositions are `rule_derived`; the ledger does not
//! claim per-commit semantic review.
//!
//! This command validates a captured ledger offline and writes the JSON and
//! Markdown projections from one normalized DTO. It does not select, qualify,
//! merge, tag or publish a candidate. The historical C/T model stays in
//! `release-denominator` (RIPR-SPEC-0146).

mod capture;
mod dto;
mod inline_tests;
mod rule;
mod validate;

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod inline_tests_tests;
#[cfg(test)]
mod rule_tests;
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
    expected_base: Option<&'a str>,
    require_final: bool,
}

pub(crate) fn release_ledger(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("capture") => return run_capture(&args[1..]),
        Some("derive") => return run_derive(&args[1..]),
        _ => {}
    }
    let args = parse_args(args)?;
    let ledger: Ledger = read_json(args.input, "ledger")?;
    let context = Context {
        expected_base: Some(
            args.expected_base
                .unwrap_or(validate::RELEASE_BASE_0_11)
                .to_string(),
        ),
        packet: args.packet.map(read_packet).transpose()?,
        markdown: args
            .markdown
            .map(|path| read_text(path, "markdown"))
            .transpose()?,
        manifest: args
            .manifest
            .map(|path| read_json::<Value>(path, "manifest"))
            .transpose()?,
        audit_file_sha256: ledger.audit.as_ref().map(|audit| {
            read_bounded(&audit.path, "audit")
                .map(|bytes| format!("{:x}", Sha256::digest(&bytes)))
        }),
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

fn run_derive(args: &[String]) -> Result<(), String> {
    let mut facts = None;
    let mut packet = None;
    let mut overrides = None;
    let mut audit = None;
    let mut stage = dto::STAGE_DRY_RUN;
    let mut output = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .map(String::as_str)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        if !seen.insert(flag) {
            return Err(format!("{flag} was given more than once"));
        }
        match flag {
            "--facts" => facts = Some(value),
            "--packet" => packet = Some(value),
            "--overrides" => overrides = Some(value),
            "--audit" => audit = Some(value),
            "--stage" => stage = value,
            "--output" => output = Some(value),
            _ => return Err(format!("unknown release-ledger derive argument `{flag}`")),
        }
        index += 2;
    }
    let facts_path = facts.ok_or("release-ledger derive requires --facts <facts.json>")?;
    let packet_path =
        packet.ok_or("release-ledger derive requires --packet <release-selection.json>")?;
    let output = output.ok_or("release-ledger derive requires --output <ledger.json>")?;
    let facts: capture::GitFacts = read_json(facts_path, "facts")?;
    if facts.kind != capture::FACTS_KIND {
        return Err(format!(
            "{facts_path} is not a {} document",
            capture::FACTS_KIND
        ));
    }
    let packet = read_packet(packet_path)?;
    let overrides: Option<rule::Overrides> = overrides
        .map(|path| read_json(path, "overrides"))
        .transpose()?;
    let audit = audit
        .map(|path| {
            let bytes = read_bounded(path, "audit")?;
            Ok::<_, String>(dto::AuditRef {
                path: path.to_string(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
            })
        })
        .transpose()?;
    if stage != dto::STAGE_DRY_RUN && stage != STAGE_FINAL {
        return Err(format!(
            "--stage must be {} or {STAGE_FINAL}",
            dto::STAGE_DRY_RUN
        ));
    }
    let ledger = rule::derive(&rule::DeriveInput {
        facts: &facts,
        packet: &packet,
        overrides: overrides.as_ref(),
        stage,
        audit,
    })?;
    let context = Context {
        expected_base: Some(validate::RELEASE_BASE_0_11.to_string()),
        packet: Some(packet),
        ..Context::default()
    };
    let reasons = validate(&ledger, &context);
    if let Some(first) = reasons.first() {
        // Leave --output untouched: a failed derivation must not replace a valid ledger.
        return Err(format!(
            "derived ledger is invalid ({} reasons); first: {first}",
            reasons.len()
        ));
    }
    let text = serde_json::to_string_pretty(&ledger)
        .map_err(|error| format!("failed to serialize release-ledger: {error}"))?;
    write_output(output, &format!("{text}\n"))?;
    let by_disposition = counts(
        ledger
            .records
            .iter()
            .map(|r| r.release_disposition.as_str()),
    );
    println!(
        "Wrote {output}: {} records, rule {}",
        ledger.records.len(),
        ledger.rule_version
    );
    for (name, count) in by_disposition {
        println!("  {name}: {count}");
    }
    Ok(())
}

fn write_output(output: &str, text: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(output).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(output, text).map_err(|error| format!("failed to write {output}: {error}"))
}

fn run_capture(args: &[String]) -> Result<(), String> {
    let mut base = None;
    let mut cut = "origin/main";
    let mut main = "origin/main";
    let mut repo = None;
    let mut repository = "EffortlessMetrics/ripr-swarm";
    let mut output = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .map(String::as_str)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        if !seen.insert(flag) {
            return Err(format!("release-ledger capture got {flag} more than once"));
        }
        match flag {
            "--base" => base = Some(value),
            "--cut" => cut = value,
            "--main" => main = value,
            "--repo" => repo = Some(value),
            "--repository" => repository = value,
            "--output" => output = Some(value),
            _ => return Err(format!("unknown release-ledger capture argument `{flag}`")),
        }
        index += 2;
    }
    let base = base.ok_or("release-ledger capture requires --base <sha>")?;
    let output = output.ok_or("release-ledger capture requires --output <facts.json>")?;
    let default_repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(
            || std::path::PathBuf::from("."),
            std::path::Path::to_path_buf,
        );
    let repo = repo.map_or(default_repo, std::path::PathBuf::from);
    let facts = capture::capture(&capture::CaptureArgs {
        repo: &repo,
        repository,
        base,
        cut,
        main,
    })?;
    let text = serde_json::to_string_pretty(&facts)
        .map_err(|error| format!("failed to serialize release-ledger facts: {error}"))?;
    write_output(output, &format!("{text}\n"))?;
    println!(
        "Wrote {output}: {} first-parent, {} reachable commits, {} alive paths, {} blamed paths",
        facts.range.first_parent.len(),
        facts.range.reachable.len(),
        facts.alive_paths_at_cut,
        facts.blamed_paths
    );
    Ok(())
}

fn parse_args(args: &[String]) -> Result<Args<'_>, String> {
    let mut parsed = Args {
        input: "",
        packet: None,
        markdown: None,
        manifest: None,
        expected_base: None,
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
            "--expected-base" => parsed.expected_base = Some(value),
            _ => return Err(format!("unknown release-ledger argument `{flag}`")),
        }
        index += 2;
    }
    if parsed.input.is_empty() {
        return Err("release-ledger requires --input <ledger.json>".to_string());
    }
    Ok(parsed)
}

/// Largest input any release-ledger command reads. The 0.11 facts file (2,213
/// commits) is about 4.6 MB, so this is more than ten times real use while
/// still failing closed on a hostile or accidental huge file.
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

/// Reads at most `MAX_INPUT_BYTES`; a larger file is an error, never truncated.
fn read_bounded(path: &str, what: &str) -> Result<Vec<u8>, String> {
    read_capped(path, what, MAX_INPUT_BYTES)
}

fn read_capped(path: &str, what: &str, cap: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file =
        fs::File::open(path).map_err(|error| format!("failed to read {what} {path}: {error}"))?;
    let mut bytes = Vec::new();
    file.take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read {what} {path}: {error}"))?;
    if bytes.len() as u64 > cap {
        return Err(format!(
            "{what} {path} exceeds the {cap}-byte release-ledger input limit"
        ));
    }
    Ok(bytes)
}

fn read_text(path: &str, what: &str) -> Result<String, String> {
    String::from_utf8(read_bounded(path, what)?)
        .map_err(|error| format!("failed to read {what} {path}: {error}"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &str, what: &str) -> Result<T, String> {
    let bytes = read_bounded(path, what)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse {what} {path}: {error}"))
}

/// The packet digest is over the raw file bytes, so any edit to the packet is a
/// different identity.
fn read_packet(path: &str) -> Result<SelectionPacket, String> {
    let bytes = read_bounded(path, "packet")?;
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
    markdown.push_str("\n| Theme | Count |\n| --- | ---: |\n");
    for (name, count) in counts(ledger.records.iter().map(|r| r.theme.as_str())) {
        markdown.push_str(&format!("| `{}` | {count} |\n", escape_cell(name)));
    }
    markdown.push_str("\n## Records\n\n| Position | Commit | Disposition | Theme | Tree effect | Subject |\n| ---: | --- | --- | --- | --- | --- |\n");
    for record in &ledger.records {
        let position = record
            .first_parent_position
            .map_or_else(|| "-".to_string(), |position| position.to_string());
        markdown.push_str(&format!(
            "| {position} | `{}` | `{}` | `{}` | `{}` | {} |\n",
            record.commit_sha,
            escape_cell(&record.release_disposition),
            escape_cell(&record.theme),
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
