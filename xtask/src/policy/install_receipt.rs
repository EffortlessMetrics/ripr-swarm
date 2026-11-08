//! `cargo xtask install-receipt [--check] | (--cells <dir> --run-id <id>)... [--write]`:
//! the checked-in receipt behind the "install to first useful result" release-ladder
//! row (docs/METRICS.md, R0).
//!
//! `.github/workflows/published-install-matrix.yml` (cargo, PyPI, npm, binstall) and
//! `.github/workflows/first-run-archive.yml` (release archive) upload one small JSON
//! file per cell (channel x platform): its status, the installed version and the
//! install seconds. This command merges one or more downloaded runs of those files,
//! each named by its run id, with the hand-kept dispositions for channels no
//! workflow installs (`metrics/install/dispositions.json`) into
//! `metrics/install/receipt.json` (and a Markdown view). Every channel x platform
//! cell must appear in at least one run; a run may not carry the same cell twice.
//!
//! Each cell is judged on all its samples. Any failed or broken sample fails it. A
//! working cell is timed by the median of its working samples: prebuilt channels
//! against the 300 s first-value bar, and `cargo install` (a source build) against
//! its own 900 s bar with at least three samples, because one noisy runner can double
//! a cold compile. `--check` re-derives the whole receipt from its own samples
//! without a network or a run. The R0 verdict is derived, never typed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

const USAGE: &str =
    "usage: cargo xtask install-receipt [--check] | (--cells <dir> --run-id <id>)... [--write]";
const RECEIPT: &str = "metrics/install/receipt.json";
const RECEIPT_MD: &str = "metrics/install/receipt.md";
const DISPOSITIONS: &str = "metrics/install/dispositions.json";
const SCHEMA: &str = "ripr-install-receipt-v2";
/// The R0 first-value bar in docs/METRICS.md for prebuilt channels.
const PREBUILT_THRESHOLD_SECONDS: f64 = 300.0;
/// The R0 bar for `cargo install`, which compiles from source.
const SOURCE_BUILD_THRESHOLD_SECONDS: f64 = 900.0;
/// A source-build cell is judged only on a median of at least this many samples.
const SOURCE_BUILD_MIN_SAMPLES: usize = 3;
const CHANNELS: [&str; 5] = ["archive", "binstall", "cargo", "npm", "pypi"];
/// Channels that compile ripr on the user's machine.
const SOURCE_BUILD_CHANNELS: [&str; 1] = ["cargo"];
const PLATFORMS: [&str; 5] = [
    "ubuntu-latest",
    "ubuntu-24.04-arm",
    "macos-latest",
    "macos-15-intel",
    "windows-latest",
];
/// A cell is one of these; `known_gap` is a documented unavailable combination,
/// `broken` an installed package that does not start.
const CELL_STATUSES: [&str; 4] = ["works", "known_gap", "broken", "failed"];
/// Channels no workflow installs; the checked-in dispositions must each account for them.
const OTHER_CHANNELS: [&str; 3] = ["vscode_extension", "ci_init", "homebrew"];
const DISPOSITION_STATUSES: [&str; 3] = ["tested_elsewhere", "not_tested", "not_shipped"];

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Cell {
    pub(super) channel: String,
    pub(super) platform: String,
    pub(super) status: String,
    pub(super) version: Option<String>,
    pub(super) install_seconds: Option<f64>,
    pub(super) run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Disposition {
    pub(super) channel: String,
    pub(super) status: String,
    pub(super) reason: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Check,
    Build {
        runs: Vec<(PathBuf, String)>,
        write: bool,
    },
}

pub(crate) fn install_receipt(args: &[String]) -> Result<(), String> {
    match parse_args(args)? {
        Mode::Check => check_receipt(Path::new(".")),
        Mode::Build { runs, write } => {
            let root = Path::new(".");
            let mut cells = Vec::new();
            for (dir, run_id) in &runs {
                cells.extend(read_cells(dir, run_id)?);
            }
            let dispositions = read_dispositions(&root.join(DISPOSITIONS))?;
            let receipt = build_receipt(&cells, &dispositions)?;
            let json_text = serde_json::to_string_pretty(&receipt)
                .map_err(|error| format!("render receipt: {error}"))?
                + "\n";
            let markdown = render_markdown(&receipt);
            println!("{markdown}");
            if write {
                write_file(&root.join(RECEIPT), &json_text)?;
                write_file(&root.join(RECEIPT_MD), &markdown)?;
                println!("wrote {RECEIPT} and {RECEIPT_MD}");
            } else {
                println!("dry run: nothing written; pass --write to record the receipt");
            }
            Ok(())
        }
    }
}

pub(super) fn parse_args(args: &[String]) -> Result<Mode, String> {
    if args.is_empty() || args == ["--check"] {
        return Ok(Mode::Check);
    }
    let mut dirs = Vec::new();
    let mut run_ids = Vec::new();
    let mut write = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--cells" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("--cells needs a directory\n{USAGE}"))?;
                dirs.push(PathBuf::from(value));
            }
            "--run-id" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("--run-id needs a value\n{USAGE}"))?;
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(format!("--run-id must be a numeric run id\n{USAGE}"));
                }
                run_ids.push(value.clone());
            }
            "--write" => write = true,
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    if dirs.is_empty() {
        return Err(format!("--run-id and --write need --cells\n{USAGE}"));
    }
    if dirs.len() != run_ids.len() {
        return Err(format!(
            "each --cells needs its own --run-id ({} --cells, {} --run-id) so every sample names its run\n{USAGE}",
            dirs.len(),
            run_ids.len()
        ));
    }
    let mut seen = BTreeSet::new();
    for run_id in &run_ids {
        if !seen.insert(run_id) {
            return Err(format!("run {run_id} given twice\n{USAGE}"));
        }
    }
    Ok(Mode::Build {
        runs: dirs.into_iter().zip(run_ids).collect(),
        write,
    })
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    fs::write(path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

fn string_field(value: &Value, key: &str, origin: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{origin}: missing string field `{key}`"))
}

/// Parse one uploaded cell file; `run_id` names the run it was downloaded from.
pub(super) fn parse_cell(text: &str, origin: &str, run_id: &str) -> Result<Cell, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("{origin}: not JSON: {error}"))?;
    let status = string_field(&value, "status", origin)?;
    if !CELL_STATUSES.contains(&status.as_str()) {
        return Err(format!("{origin}: unknown status `{status}`"));
    }
    let install_seconds = match value.get("install_seconds") {
        None | Some(Value::Null) => None,
        Some(seconds) => Some(
            seconds
                .as_f64()
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                .ok_or_else(|| {
                    format!("{origin}: install_seconds must be a non-negative number")
                })?,
        ),
    };
    let version = match value.get("version") {
        None | Some(Value::Null) => None,
        Some(version) => Some(
            version
                .as_str()
                .filter(|version| !version.is_empty())
                .ok_or_else(|| format!("{origin}: version must be a non-empty string"))?
                .to_string(),
        ),
    };
    if status == "works" && (version.is_none() || install_seconds.is_none()) {
        return Err(format!(
            "{origin}: a `works` cell must record its version and install seconds"
        ));
    }
    Ok(Cell {
        channel: string_field(&value, "channel", origin)?,
        platform: string_field(&value, "os", origin)?,
        status,
        version,
        install_seconds,
        run_id: run_id.to_string(),
    })
}

fn read_cells(dir: &Path, run_id: &str) -> Result<Vec<Cell>, String> {
    let mut files = Vec::new();
    collect_json(dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!("no cell files (*.json) under {}", dir.display()));
    }
    let mut cells = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file)
            .map_err(|error| format!("read {}: {error}", file.display()))?;
        cells.push(parse_cell(&text, &file.display().to_string(), run_id)?);
    }
    Ok(cells)
}

fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|error| format!("read directory {}: {error}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("read directory {}: {error}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_json(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            out.push(path);
        }
    }
    Ok(())
}

pub(super) fn parse_dispositions(text: &str, origin: &str) -> Result<Vec<Disposition>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("{origin}: not JSON: {error}"))?;
    let items = value
        .get("channels")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{origin}: missing `channels` array"))?;
    let mut out = Vec::new();
    for item in items {
        let channel = string_field(item, "channel", origin)?;
        let status = string_field(item, "status", origin)?;
        if !DISPOSITION_STATUSES.contains(&status.as_str()) {
            return Err(format!(
                "{origin}: channel `{channel}`: unknown status `{status}`"
            ));
        }
        if !OTHER_CHANNELS.contains(&channel.as_str()) {
            return Err(format!(
                "{origin}: channel `{channel}` is not an out-of-matrix channel ({})",
                OTHER_CHANNELS.join(", ")
            ));
        }
        if out.iter().any(|seen: &Disposition| seen.channel == channel) {
            return Err(format!("{origin}: channel `{channel}` listed twice"));
        }
        let reason = string_field(item, "reason", origin)?;
        if reason.trim().is_empty() {
            return Err(format!("{origin}: channel `{channel}`: empty reason"));
        }
        out.push(Disposition {
            channel,
            status,
            reason,
        });
    }
    if out.is_empty() {
        return Err(format!("{origin}: no channel dispositions"));
    }
    Ok(out)
}

fn read_dispositions(path: &Path) -> Result<Vec<Disposition>, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let origin = path.display().to_string();
    let parsed = parse_dispositions(&text, &origin)?;
    for channel in OTHER_CHANNELS {
        if !parsed.iter().any(|item| item.channel == channel) {
            return Err(format!(
                "{origin}: no disposition for channel `{channel}`; every shipped channel needs one"
            ));
        }
    }
    Ok(parsed)
}

/// The median of a sample set; `None` when it is empty.
pub(super) fn median(samples: &[f64]) -> Option<f64> {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        len if len % 2 == 1 => sorted.get(middle).copied(),
        _ => Some((sorted.get(middle - 1)? + sorted.get(middle)?) / 2.0),
    }
}

/// Merge cell samples from one or more runs with channel dispositions into the receipt value.
pub(super) fn build_receipt(cells: &[Cell], dispositions: &[Disposition]) -> Result<Value, String> {
    let mut by_cell: BTreeMap<(String, String), Vec<&Cell>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for cell in cells {
        if !CHANNELS.contains(&cell.channel.as_str()) {
            return Err(format!("unknown channel `{}`", cell.channel));
        }
        if !PLATFORMS.contains(&cell.platform.as_str()) {
            return Err(format!("unknown platform `{}`", cell.platform));
        }
        if cell.run_id.is_empty() || !cell.run_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!(
                "{} on {}: run id `{}` is not a numeric run id",
                cell.channel, cell.platform, cell.run_id
            ));
        }
        if !seen.insert((&cell.channel, &cell.platform, &cell.run_id)) {
            return Err(format!(
                "duplicate cell {} on {} in run {}",
                cell.channel, cell.platform, cell.run_id
            ));
        }
        by_cell
            .entry((cell.channel.clone(), cell.platform.clone()))
            .or_default()
            .push(cell);
    }
    let mut missing = Vec::new();
    for channel in CHANNELS {
        for platform in PLATFORMS {
            if !by_cell.contains_key(&(channel.to_string(), platform.to_string())) {
                missing.push(format!("{channel} on {platform}"));
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!("missing matrix cells: {}", missing.join(", ")));
    }

    let mut reasons = Vec::new();
    let mut not_met = false;
    let mut not_established = false;
    let mut rows = Vec::new();
    for ((channel, platform), samples) in &mut by_cell {
        // Run ids are all digits, so length then text is numeric order.
        samples.sort_by(|a, b| (a.run_id.len(), &a.run_id).cmp(&(b.run_id.len(), &b.run_id)));
        let source_build = SOURCE_BUILD_CHANNELS.contains(&channel.as_str());
        let (threshold, min_samples) = if source_build {
            (SOURCE_BUILD_THRESHOLD_SECONDS, SOURCE_BUILD_MIN_SAMPLES)
        } else {
            (PREBUILT_THRESHOLD_SECONDS, 1)
        };
        let bad: Vec<&&Cell> = samples
            .iter()
            .filter(|cell| cell.status == "failed" || cell.status == "broken")
            .collect();
        let working: Vec<&&Cell> = samples
            .iter()
            .filter(|cell| cell.status == "works")
            .collect();
        let seconds: Vec<f64> = working
            .iter()
            .filter_map(|cell| cell.install_seconds)
            .collect();
        let median_seconds = median(&seconds);
        let status = if let Some(first) = bad.first() {
            not_met = true;
            for cell in &bad {
                reasons.push(format!(
                    "{channel} on {platform}: {} in run {}",
                    cell.status, cell.run_id
                ));
            }
            first.status.clone()
        } else if working.is_empty() {
            not_established = true;
            reasons.push(format!(
                "{channel} on {platform} is an unavailable platform"
            ));
            "known_gap".to_string()
        } else {
            let median_seconds = median_seconds.unwrap_or_default();
            if median_seconds > threshold {
                not_met = true;
                reasons.push(format!(
                    "{channel} on {platform}: median {median_seconds:.0} s over {} sample(s), over {threshold:.0} s",
                    seconds.len()
                ));
            } else if seconds.len() < min_samples {
                not_established = true;
                reasons.push(format!(
                    "{channel} on {platform} has {} of the {min_samples} samples a source build is judged on",
                    seconds.len()
                ));
            }
            "works".to_string()
        };
        let version = working.last().and_then(|cell| cell.version.clone());
        rows.push(json!({
            "channel": channel,
            "platform": platform,
            "build": if source_build { "source" } else { "prebuilt" },
            "status": status,
            "version": version,
            "install_seconds": median_seconds,
            "threshold_seconds": threshold,
            "samples": samples
                .iter()
                .map(|cell| json!({
                    "run_id": cell.run_id,
                    "status": cell.status,
                    "version": cell.version,
                    "install_seconds": cell.install_seconds,
                }))
                .collect::<Vec<_>>(),
        }));
    }
    for disposition in dispositions {
        if disposition.status == "not_tested" {
            not_established = true;
            reasons.push(format!("channel {} is not tested", disposition.channel));
        }
    }
    let r0 = if not_met {
        "not_met"
    } else if not_established {
        "not_established"
    } else {
        "met"
    };
    let runs: BTreeSet<(usize, &str)> = cells
        .iter()
        .map(|cell| (cell.run_id.len(), cell.run_id.as_str()))
        .collect();
    Ok(json!({
        "schema_version": SCHEMA,
        "source_run_ids": runs.into_iter().map(|(_, run)| run).collect::<Vec<_>>(),
        "thresholds": {
            "prebuilt_seconds": PREBUILT_THRESHOLD_SECONDS,
            "source_build_seconds": SOURCE_BUILD_THRESHOLD_SECONDS,
            "source_build_min_samples": SOURCE_BUILD_MIN_SAMPLES,
        },
        "r0_install": { "status": r0, "reasons": reasons },
        "cells": rows,
        "channel_dispositions": dispositions
            .iter()
            .map(|d| json!({"channel": d.channel, "status": d.status, "reason": d.reason}))
            .collect::<Vec<_>>(),
    }))
}

pub(super) fn render_markdown(receipt: &Value) -> String {
    let mut out = String::from("# Install receipt\n\n");
    let status = receipt["r0_install"]["status"]
        .as_str()
        .unwrap_or("unknown");
    out.push_str(&format!("R0 install row: **{status}**"));
    let runs: Vec<&str> = receipt["source_run_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !runs.is_empty() {
        out.push_str(&format!(" (runs {})", runs.join(", ")));
    }
    out.push_str(&format!(
        "\n\nPrebuilt channels are judged against {:.0} s. `cargo install` builds from source and is judged on the median of at least {} samples against {:.0} s.\n",
        receipt["thresholds"]["prebuilt_seconds"]
            .as_f64()
            .unwrap_or_default(),
        receipt["thresholds"]["source_build_min_samples"]
            .as_u64()
            .unwrap_or_default(),
        receipt["thresholds"]["source_build_seconds"]
            .as_f64()
            .unwrap_or_default(),
    ));
    out.push_str("\n| channel | build | platform | status | version | median seconds | samples |\n| --- | --- | --- | --- | --- | ---: | --- |\n");
    for cell in receipt["cells"].as_array().into_iter().flatten() {
        let seconds = cell["install_seconds"]
            .as_f64()
            .map_or_else(|| "-".to_string(), |s| format!("{s:.0}"));
        let samples: Vec<String> = cell["samples"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|sample| {
                sample["install_seconds"].as_f64().map_or_else(
                    || sample["status"].as_str().unwrap_or("").to_string(),
                    |s| format!("{s:.0}"),
                )
            })
            .collect();
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            cell["channel"].as_str().unwrap_or(""),
            cell["build"].as_str().unwrap_or(""),
            cell["platform"].as_str().unwrap_or(""),
            cell["status"].as_str().unwrap_or(""),
            cell["version"].as_str().unwrap_or("-"),
            seconds,
            samples.join(", ")
        ));
    }
    out.push_str("\n| other channel | disposition | reason |\n| --- | --- | --- |\n");
    for item in receipt["channel_dispositions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            item["channel"].as_str().unwrap_or(""),
            item["status"].as_str().unwrap_or(""),
            item["reason"].as_str().unwrap_or("")
        ));
    }
    if let Some(reasons) = receipt["r0_install"]["reasons"].as_array()
        && !reasons.is_empty()
    {
        out.push_str("\nWhy not met:\n\n");
        for reason in reasons {
            out.push_str(&format!("- {}\n", reason.as_str().unwrap_or("")));
        }
    }
    out
}

/// Re-derive the whole receipt from its own samples and the dispositions file; fails
/// when the checked-in JSON or Markdown differs in any field from that derivation.
pub(super) fn check_receipt(root: &Path) -> Result<(), String> {
    let path = root.join(RECEIPT);
    let text = fs::read_to_string(&path).map_err(|error| {
        format!(
            "read {}: {error}; record one with `cargo xtask install-receipt --cells <dir> --run-id <id> --write`",
            path.display()
        )
    })?;
    let value: Value =
        serde_json::from_str(&text).map_err(|error| format!("{RECEIPT}: not JSON: {error}"))?;
    if value["schema_version"] != SCHEMA {
        return Err(format!("{RECEIPT}: schema_version must be `{SCHEMA}`"));
    }
    let mut cells = Vec::new();
    for row in value["cells"].as_array().into_iter().flatten() {
        for sample in row["samples"].as_array().into_iter().flatten() {
            let run_id = sample["run_id"]
                .as_str()
                .ok_or_else(|| format!("{RECEIPT}: a sample has no run_id"))?;
            let text = json!({
                "channel": row["channel"],
                "os": row["platform"],
                "status": sample["status"],
                "version": sample["version"],
                "install_seconds": sample["install_seconds"],
            })
            .to_string();
            cells.push(parse_cell(&text, RECEIPT, run_id)?);
        }
    }
    let dispositions = read_dispositions(&root.join(DISPOSITIONS))?;
    let rebuilt = build_receipt(&cells, &dispositions)?;
    if rebuilt["channel_dispositions"] != value["channel_dispositions"] {
        return Err(format!(
            "{RECEIPT}: channel_dispositions are stale against {DISPOSITIONS}; re-run install-receipt --write"
        ));
    }
    if rebuilt["r0_install"] != value["r0_install"] {
        return Err(format!(
            "{RECEIPT}: recorded r0_install {} does not match the verdict its samples and {DISPOSITIONS} give ({})",
            value["r0_install"], rebuilt["r0_install"]
        ));
    }
    if rebuilt != value {
        return Err(format!(
            "{RECEIPT}: a recorded field (threshold, median, run list or row) does not match what its samples give; re-run install-receipt --write"
        ));
    }
    let markdown_path = root.join(RECEIPT_MD);
    let markdown = fs::read_to_string(&markdown_path)
        .map_err(|error| format!("read {}: {error}", markdown_path.display()))?;
    if markdown != render_markdown(&rebuilt) {
        return Err(format!(
            "{RECEIPT_MD} does not match {RECEIPT}; re-run install-receipt --write"
        ));
    }
    println!(
        "install-receipt: {} samples over {} cells, R0 install row `{}`",
        cells.len(),
        rebuilt["cells"].as_array().map_or(0, Vec::len),
        rebuilt["r0_install"]["status"]
            .as_str()
            .unwrap_or("unknown")
    );
    Ok(())
}

#[cfg(test)]
mod tests;
