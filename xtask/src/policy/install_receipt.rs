//! `cargo xtask install-receipt [--check] | --cells <dir> [--run-id <id>] [--write]`:
//! the checked-in receipt behind the "install to first useful result" release-ladder
//! row (docs/METRICS.md, R0).
//!
//! `.github/workflows/published-install-matrix.yml` uploads one small JSON file per
//! matrix cell (channel x platform): its status, the installed version and the
//! install seconds. This command merges a downloaded set of those files with the
//! hand-kept channel dispositions in `metrics/install/dispositions.json` into
//! `metrics/install/receipt.json` (and a Markdown view), refusing a set that is
//! missing a cell or carries an unknown status. `--check` re-validates the
//! checked-in receipt without a network or a run. The R0 verdict is derived, never
//! typed: `met` only when every cell works within the threshold and no channel
//! disposition is left untested.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

const USAGE: &str =
    "usage: cargo xtask install-receipt [--check] | --cells <dir> [--run-id <id>] [--write]";
const RECEIPT: &str = "metrics/install/receipt.json";
const RECEIPT_MD: &str = "metrics/install/receipt.md";
const DISPOSITIONS: &str = "metrics/install/dispositions.json";
const SCHEMA: &str = "ripr-install-receipt-v1";
/// The R0 ceiling in docs/METRICS.md: install to first useful result.
const THRESHOLD_SECONDS: f64 = 300.0;
const CHANNELS: [&str; 3] = ["cargo", "pypi", "npm"];
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
const DISPOSITION_STATUSES: [&str; 3] = ["tested_elsewhere", "not_tested", "not_shipped"];

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Cell {
    pub(super) channel: String,
    pub(super) platform: String,
    pub(super) status: String,
    pub(super) version: Option<String>,
    pub(super) install_seconds: Option<f64>,
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
        cells: PathBuf,
        run_id: Option<String>,
        write: bool,
    },
}

pub(crate) fn install_receipt(args: &[String]) -> Result<(), String> {
    match parse_args(args)? {
        Mode::Check => check_receipt(Path::new(".")),
        Mode::Build {
            cells,
            run_id,
            write,
        } => {
            let root = Path::new(".");
            let cells = read_cells(&cells)?;
            let dispositions = read_dispositions(&root.join(DISPOSITIONS))?;
            let receipt = build_receipt(&cells, &dispositions, run_id.as_deref())?;
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
    let mut cells = None;
    let mut run_id = None;
    let mut write = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--cells" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("--cells needs a directory\n{USAGE}"))?;
                cells = Some(PathBuf::from(value));
            }
            "--run-id" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("--run-id needs a value\n{USAGE}"))?;
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(format!("--run-id must be a numeric run id\n{USAGE}"));
                }
                run_id = Some(value.clone());
            }
            "--write" => write = true,
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    let cells = cells.ok_or_else(|| format!("--run-id and --write need --cells\n{USAGE}"))?;
    Ok(Mode::Build {
        cells,
        run_id,
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

pub(super) fn parse_cell(text: &str, origin: &str) -> Result<Cell, String> {
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
    })
}

fn read_cells(dir: &Path) -> Result<Vec<Cell>, String> {
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
        cells.push(parse_cell(&text, &file.display().to_string())?);
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
    parse_dispositions(&text, &path.display().to_string())
}

/// Merge matrix cells with channel dispositions into the receipt value.
pub(super) fn build_receipt(
    cells: &[Cell],
    dispositions: &[Disposition],
    run_id: Option<&str>,
) -> Result<Value, String> {
    let mut by_cell: BTreeMap<(String, String), &Cell> = BTreeMap::new();
    for cell in cells {
        if !CHANNELS.contains(&cell.channel.as_str()) {
            return Err(format!("unknown channel `{}`", cell.channel));
        }
        if !PLATFORMS.contains(&cell.platform.as_str()) {
            return Err(format!("unknown platform `{}`", cell.platform));
        }
        let key = (cell.channel.clone(), cell.platform.clone());
        if by_cell.insert(key, cell).is_some() {
            return Err(format!(
                "duplicate cell {} on {}",
                cell.channel, cell.platform
            ));
        }
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
    for ((channel, platform), cell) in &by_cell {
        match cell.status.as_str() {
            "works" => {
                if cell
                    .install_seconds
                    .is_some_and(|seconds| seconds > THRESHOLD_SECONDS)
                {
                    not_met = true;
                    reasons.push(format!(
                        "{channel} on {platform} took {:.0} s, over {THRESHOLD_SECONDS:.0} s",
                        cell.install_seconds.unwrap_or_default()
                    ));
                }
            }
            "known_gap" => {
                not_established = true;
                reasons.push(format!(
                    "{channel} on {platform} is an unavailable platform"
                ));
            }
            _ => {
                not_met = true;
                reasons.push(format!("{channel} on {platform}: {}", cell.status));
            }
        }
        rows.push(json!({
            "channel": channel,
            "platform": platform,
            "status": cell.status,
            "version": cell.version,
            "install_seconds": cell.install_seconds,
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
    Ok(json!({
        "schema_version": SCHEMA,
        "source_run_id": run_id,
        "threshold_seconds": THRESHOLD_SECONDS,
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
    if let Some(run) = receipt["source_run_id"].as_str() {
        out.push_str(&format!(" (matrix run {run})"));
    }
    out.push_str("\n\n| channel | platform | status | version | install seconds |\n| --- | --- | --- | --- | ---: |\n");
    for cell in receipt["cells"].as_array().into_iter().flatten() {
        let seconds = cell["install_seconds"]
            .as_f64()
            .map_or_else(|| "-".to_string(), |s| format!("{s:.0}"));
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            cell["channel"].as_str().unwrap_or(""),
            cell["platform"].as_str().unwrap_or(""),
            cell["status"].as_str().unwrap_or(""),
            cell["version"].as_str().unwrap_or("-"),
            seconds
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
    if let Some(reasons) = receipt["r0_install"]["reasons"].as_array() {
        if !reasons.is_empty() {
            out.push_str("\nWhy not met:\n\n");
            for reason in reasons {
                out.push_str(&format!("- {}\n", reason.as_str().unwrap_or("")));
            }
        }
    }
    out
}

/// Re-derive the receipt's verdict from its own rows and dispositions; fails when
/// the checked-in file is malformed, incomplete, or states a verdict its rows do not.
pub(super) fn check_receipt(root: &Path) -> Result<(), String> {
    let path = root.join(RECEIPT);
    let text = fs::read_to_string(&path).map_err(|error| {
        format!(
            "read {}: {error}; record one with `cargo xtask install-receipt --cells <dir> --write`",
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
        let text = json!({
            "channel": row["channel"],
            "os": row["platform"],
            "status": row["status"],
            "version": row["version"],
            "install_seconds": row["install_seconds"],
        })
        .to_string();
        cells.push(parse_cell(&text, RECEIPT)?);
    }
    let dispositions = read_dispositions(&root.join(DISPOSITIONS))?;
    let rebuilt = build_receipt(&cells, &dispositions, value["source_run_id"].as_str())?;
    if rebuilt["r0_install"] != value["r0_install"] {
        return Err(format!(
            "{RECEIPT}: recorded r0_install {} does not match the verdict its rows and {DISPOSITIONS} give ({})",
            value["r0_install"], rebuilt["r0_install"]
        ));
    }
    if rebuilt["channel_dispositions"] != value["channel_dispositions"] {
        return Err(format!(
            "{RECEIPT}: channel_dispositions are stale against {DISPOSITIONS}; re-run install-receipt --write"
        ));
    }
    println!(
        "install-receipt: {} cells, R0 install row `{}`",
        cells.len(),
        rebuilt["r0_install"]["status"]
            .as_str()
            .unwrap_or("unknown")
    );
    Ok(())
}

#[cfg(test)]
mod tests;
