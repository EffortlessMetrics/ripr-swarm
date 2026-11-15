//! `cargo xtask dx-trend` (#14795): keeps one compact dated receipt per
//! nightly dx-scoreboard run and renders one trend page that ranks every
//! measured metric by its distance from its declared target, with the previous
//! value, the change over the last week and a short sparkline.
//!
//! Scope and non-claims: the page reads only the dx-scoreboard report it is
//! given and the receipts in `--history`. It adds no gate and never turns a
//! missing, failed or not-measured value into a pass or a zero. A
//! runner-dependent metric (wall time, memory) trends only against receipts
//! from the same runner class. Targets come from
//! `benchmarks/dx_scoreboard/scoreboards.toml` through the report; this page
//! does not set or move them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const RECEIPT_SCHEMA: &str = "ripr-dx-trend-receipt-v1";
const PAGE_SCHEMA: &str = "ripr-dx-trend-page-v1";
const DEFAULT_KEEP: usize = 120;
const SPARK_NIGHTS: usize = 14;
const WEEK_DAYS: i64 = 7;
/// A "week ago" value must be 7 to 9 days old; after a broken chain an older
/// night is not shown as last week.
const WEEK_SLACK_DAYS: i64 = 2;
const USAGE: &str = "usage: cargo xtask dx-trend --report <dx-scoreboard.json> --history <dir> [--date <YYYY-MM-DD>] [--keep <n>]";

struct Options {
    report: PathBuf,
    history: PathBuf,
    date: Option<String>,
    keep: usize,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut report = None;
    let mut history = None;
    let mut date = None;
    let mut keep = DEFAULT_KEEP;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
        };
        match flag.as_str() {
            "--report" => report = Some(PathBuf::from(value()?)),
            "--history" => history = Some(PathBuf::from(value()?)),
            "--date" => {
                let raw = value()?;
                if days_from_civil(&raw).is_none() {
                    return Err(format!("--date {raw} is not YYYY-MM-DD"));
                }
                date = Some(raw);
            }
            "--keep" => {
                let raw = value()?;
                keep = raw
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| format!("--keep {raw} is not a positive integer"))?;
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    Ok(Options {
        report: report.ok_or_else(|| format!("--report is required\n{USAGE}"))?,
        history: history.ok_or_else(|| format!("--history is required\n{USAGE}"))?,
        date,
        keep,
    })
}

/// Unix day count for a `YYYY-MM-DD` date (Howard Hinnant's algorithm).
fn days_from_civil(date: &str) -> Option<i64> {
    let parts: Vec<&str> = date.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return None;
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let (year, month, day): (i64, i64, i64) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    // Rejects impossible days (2026-02-30) and signed parts (+1).
    (civil_from_days(days) == date).then_some(days)
}

fn civil_from_days(unix_days: i64) -> String {
    let z = unix_days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn today_utc() -> Result<String, String> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock before 1970: {error}"))?
        .as_secs();
    let days = i64::try_from(secs / 86_400).map_err(|error| format!("clock overflow: {error}"))?;
    Ok(civil_from_days(days))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

/// A value is evidence only when its row completed against its target. A
/// `failed` row can still carry its worst numeric sample, which must not
/// trend as a result.
fn admissible(status: &str) -> bool {
    matches!(status, "meets_target" | "below_target")
}

/// Repositories whose sample counted toward a per-repository value.
fn measured_repos(row: &Value) -> Vec<String> {
    let mut repos: Vec<String> = row["samples"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["status"].as_str().is_some_and(admissible))
        .filter_map(|s| s["repo"].as_str().map(str::to_string))
        .collect();
    repos.sort();
    repos.dedup();
    repos
}

/// Each corpus repository's workload identity (the fields the scoreboard's
/// own baseline pairing compares), so a moved pin is not read as movement.
fn workloads(report: &Value) -> Value {
    let map: serde_json::Map<String, Value> = report["corpus"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry["id"].as_str()?;
            Some((
                id.to_string(),
                json!({
                    "url": entry["url"],
                    "sha": entry["sha"],
                    "base": entry["base"],
                    "language": entry["language"],
                    "expected_partial": entry["expected_partial"],
                }),
            ))
        })
        .collect();
    Value::Object(map)
}

/// Whether a past per-repository value measured the same population on the
/// same workloads as the current one.
fn same_workload(past_receipt: &Value, past: &Value, current: &Value, metric: &Value) -> bool {
    if metric["per_repo"] != json!(true) {
        return true;
    }
    if past["repos"] != metric["repos"] {
        return false;
    }
    metric["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|repo| {
            let Some(repo) = repo.as_str() else {
                return false;
            };
            let now = &current["workloads"][repo];
            !now.is_null() && past_receipt["workloads"][repo] == *now
        })
}

/// Projects a dx-scoreboard report into one dated trend receipt.
fn receipt_from_report(report: &Value, date: &str) -> Result<Value, String> {
    if report["report"] != json!("dx-scoreboard") {
        return Err(
            "--report is not a dx-scoreboard report (`report` != \"dx-scoreboard\")".into(),
        );
    }
    let rows = report["metrics"]
        .as_array()
        .ok_or("dx-scoreboard report has no `metrics` array")?;
    let mut metrics = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row["id"]
            .as_str()
            .ok_or("dx-scoreboard metric row has no string `id`")?;
        let status = row["status"].as_str().unwrap_or("not_measured");
        // The scoreboard keeps an incomplete sample's elapsed number for
        // context and forces the row to `below_target`; that number can sit
        // under the target (a run that stopped early is fast), so it is not a
        // result and must never read as at the bar.
        let incomplete = row["samples"]
            .as_array()
            .is_some_and(|samples| samples.iter().any(|s| s["status"] == json!("incomplete")));
        let status = if incomplete && admissible(status) {
            "incomplete"
        } else {
            status
        };
        let value = if admissible(status) {
            row["value"].as_f64().map_or(Value::Null, |v| json!(v))
        } else {
            Value::Null
        };
        // An admissible status without a number is an instrument gap, not a pass.
        let status = if admissible(status) && value.is_null() {
            "not_measured"
        } else {
            status
        };
        metrics.push(json!({
            "id": id,
            "board": row["board"],
            "title": row["title"],
            "unit": row["unit"],
            "direction": row["direction"],
            "target": row["target"],
            "runner_dependent": row["runner_dependent"].as_bool().unwrap_or(true),
            "partial": row["partial"].as_bool().unwrap_or(false),
            "per_repo": row["per_repo"].as_bool().unwrap_or(false),
            "repos": measured_repos(row),
            "status": status,
            "value": value,
            "reason": if status == "incomplete" {
                json!("a sample did not complete, so its number is not a result")
            } else {
                row["reason"].clone()
            },
        }));
    }
    Ok(json!({
        "schema_version": RECEIPT_SCHEMA,
        "date": date,
        "revision": report["revision"],
        "analyzer_version": report["analyzer_version"],
        "runner_class": report["runner_class"],
        "gate_status": report["gate"]["status"],
        "workloads": workloads(report),
        "metrics": metrics,
    }))
}

fn is_receipt_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("dx-trend-") && n.ends_with(".json"))
}

/// Receipts by date. Files of another schema are ignored; an unreadable or
/// undated receipt is skipped with a warning. The current receipt owns its
/// date.
fn load_history(dir: &Path, current: &Value) -> Result<BTreeMap<String, Value>, String> {
    let mut by_date = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(dir) {
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| is_receipt_name(path))
            .collect();
        paths.sort();
        for path in paths {
            // One unreadable night must not stop every later trend page; it
            // is named on stderr and left out of the history.
            let receipt = match read_json(&path) {
                Ok(receipt) if receipt["schema_version"] == json!(RECEIPT_SCHEMA) => receipt,
                Ok(_) => continue,
                Err(error) => {
                    eprintln!("warning: skipping trend receipt: {error}");
                    continue;
                }
            };
            let Some(date) = receipt["date"]
                .as_str()
                .filter(|d| days_from_civil(d).is_some())
                .map(str::to_string)
            else {
                eprintln!(
                    "warning: skipping trend receipt {}: no valid `date`",
                    path.display()
                );
                continue;
            };
            by_date.insert(date, receipt);
        }
    }
    let date = current["date"]
        .as_str()
        .ok_or("current receipt has no date")?;
    by_date.insert(date.to_string(), current.clone());
    Ok(by_date)
}

/// Shortfall from the target in the metric's own unit (0 at or past it).
fn shortfall(direction: &str, value: f64, target: f64) -> Option<f64> {
    match direction {
        "lower_is_better" => Some((value - target).max(0.0)),
        "higher_is_better" => Some((target - value).max(0.0)),
        _ => None,
    }
}

/// Sort key for "how far from the bar": relative shortfall, with any miss of
/// a zero target (false confidence, wrong verdicts) ranked above every
/// relative miss because no ratio can express it.
fn distance(shortfall: f64, target: f64) -> f64 {
    if shortfall <= 0.0 {
        0.0
    } else if target == 0.0 {
        f64::INFINITY
    } else {
        shortfall / target.abs()
    }
}

fn better(direction: &str, from: f64, to: f64) -> &'static str {
    let delta = to - from;
    if delta.abs() <= f64::EPSILON * from.abs().max(1.0) {
        return "same";
    }
    let improved = match direction {
        "lower_is_better" => delta < 0.0,
        _ => delta > 0.0,
    };
    if improved { "better" } else { "worse" }
}

fn metric_in<'a>(receipt: &'a Value, id: &str) -> Option<&'a Value> {
    receipt["metrics"]
        .as_array()?
        .iter()
        .find(|m| m["id"].as_str() == Some(id))
}

/// Earlier (date, value) pairs for one metric that are comparable with the
/// current receipt, oldest first.
fn comparable_series(
    history: &BTreeMap<String, Value>,
    current: &Value,
    metric: &Value,
) -> Vec<(String, f64)> {
    let id = metric["id"].as_str().unwrap_or_default();
    let date = current["date"].as_str().unwrap_or_default();
    let runner_dependent = metric["runner_dependent"].as_bool().unwrap_or(true);
    history
        .iter()
        .filter(|(day, _)| day.as_str() < date)
        .filter(|(_, receipt)| {
            !runner_dependent || receipt["runner_class"] == current["runner_class"]
        })
        .filter_map(|(day, receipt)| {
            let past = metric_in(receipt, id)?;
            if past["target"] != metric["target"]
                || past["direction"] != metric["direction"]
                || !same_workload(receipt, past, current, metric)
            {
                return None;
            }
            Some((day.clone(), past["value"].as_f64()?))
        })
        .collect()
}

fn sparkline(values: &[f64]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let (lo, hi) = values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        });
    values
        .iter()
        .map(|v| {
            if hi - lo <= f64::EPSILON {
                BARS[3]
            } else {
                // Nearest of the eight levels, without a float-to-index cast.
                let scaled = (v - lo) / (hi - lo) * 7.0;
                let level = (0_u8..8)
                    .find(|i| f64::from(*i) + 0.5 > scaled)
                    .unwrap_or(7);
                BARS[usize::from(level)]
            }
        })
        .collect()
}

/// Builds the trend page from the accumulated receipts.
fn evaluate(history: &BTreeMap<String, Value>, current: &Value) -> Result<Value, String> {
    let date = current["date"]
        .as_str()
        .ok_or("current receipt has no date")?;
    let today = days_from_civil(date).ok_or("current receipt has an invalid date")?;
    let week_cut = civil_from_days(today - WEEK_DAYS);
    let week_floor = civil_from_days(today - WEEK_DAYS - WEEK_SLACK_DAYS);
    let metrics = current["metrics"]
        .as_array()
        .ok_or("current receipt has no metrics")?;
    let mut off_bar = Vec::new();
    let mut at_bar = Vec::new();
    let mut unmeasured: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut failed = Vec::new();
    let mut moves = BTreeMap::from([("better", 0_u64), ("worse", 0), ("same", 0)]);
    let mut partial_count = 0_usize;
    for metric in metrics {
        let id = metric["id"].as_str().unwrap_or_default();
        let board = metric["board"].as_str().unwrap_or("unknown").to_string();
        let status = metric["status"].as_str().unwrap_or("not_measured");
        if status == "failed" {
            failed.push(json!({"id": id, "board": board, "reason": metric["reason"]}));
            continue;
        }
        let (Some(value), Some(target)) = (metric["value"].as_f64(), metric["target"].as_f64())
        else {
            unmeasured
                .entry(board)
                .or_default()
                .push(json!({"id": id, "reason": metric["reason"]}));
            continue;
        };
        let direction = metric["direction"].as_str().unwrap_or_default();
        let Some(short) = shortfall(direction, value, target) else {
            unmeasured
                .entry(board)
                .or_default()
                .push(json!({"id": id, "reason": format!("unknown direction `{direction}`")}));
            continue;
        };
        // Any miss of a zero target ranks first, including one that rounding
        // exported as exactly 0 under a `below_target` verdict.
        let missed = short > 0.0 || status == "below_target";
        let zero_target_miss = missed && target == 0.0;
        let series = comparable_series(history, current, metric);
        let previous = series.last().map(|(_, v)| *v);
        let week_entry = series
            .iter()
            .rev()
            .find(|(day, _)| day.as_str() <= week_cut.as_str())
            .filter(|(day, _)| day.as_str() >= week_floor.as_str());
        let week_ago = week_entry.map(|(_, v)| *v);
        if let Some(prev) = previous
            && let Some(slot) = moves.get_mut(better(direction, prev, value))
        {
            *slot += 1;
        }
        let mut spark: Vec<f64> = series
            .iter()
            .rev()
            .take(SPARK_NIGHTS - 1)
            .map(|(_, v)| *v)
            .collect();
        spark.reverse();
        spark.push(value);
        let row = json!({
            "id": id,
            "board": board,
            "title": metric["title"],
            "unit": metric["unit"],
            "direction": direction,
            "target": target,
            "value": value,
            "partial": metric["partial"],
            "shortfall": short,
            "distance": (!zero_target_miss).then(|| distance(short, target)),
            "zero_target_miss": zero_target_miss,
            "previous": previous,
            "since_previous": previous.map(|p| better(direction, p, value)),
            "week_ago": week_ago,
            "week_ago_date": week_entry.map(|(day, _)| day.clone()),
            "since_week_ago": week_ago.map(|w| better(direction, w, value)),
            "nights": series.len() + 1,
            "sparkline": sparkline(&spark),
        });
        // The scoreboard judges the target before rounding the exported
        // value, so a `below_target` verdict stays a miss even when the
        // rounded number equals the target.
        if missed {
            off_bar.push(row);
        } else {
            if metric["partial"] == json!(true) {
                partial_count += 1;
            }
            at_bar.push(row);
        }
    }
    // Zero-target misses first (largest miss first), then relative distance.
    // The JSON `distance` of a zero-target miss is null (JSON has no
    // infinity); the explicit flag carries that rank.
    off_bar.sort_by(|a, b| {
        let zero = |row: &Value| row["zero_target_miss"] == json!(true);
        let num = |row: &Value, key: &str| row[key].as_f64().unwrap_or_default();
        zero(b)
            .cmp(&zero(a))
            .then_with(|| num(b, "distance").total_cmp(&num(a, "distance")))
            .then_with(|| num(b, "shortfall").total_cmp(&num(a, "shortfall")))
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    at_bar.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let unmeasured_count: usize = unmeasured.values().map(Vec::len).sum();
    Ok(json!({
        "schema_version": PAGE_SCHEMA,
        "date": date,
        "revision": current["revision"],
        "runner_class": current["runner_class"],
        "gate_status": current["gate_status"],
        "nights": history.keys().filter(|day| day.as_str() <= date).count(),
        "first_night": history.keys().find(|day| day.as_str() <= date),
        "counts": {
            "total": metrics.len(),
            "off_bar": off_bar.len(),
            "at_bar": at_bar.len(),
            "at_bar_partial": partial_count,
            "not_measured": unmeasured_count,
            "failed": failed.len(),
        },
        "since_previous": moves,
        "off_bar": off_bar,
        "at_bar": at_bar,
        "not_measured": unmeasured,
        "failed": failed,
    }))
}

fn fmt_value(value: f64, unit: &str) -> String {
    if unit == "ratio" {
        return format!("{:.1}%", value * 100.0);
    }
    let suffix = if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    };
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}{suffix}")
    } else {
        format!("{value:.2}{suffix}")
    }
}

fn fmt_opt(value: &Value, unit: &str) -> String {
    value
        .as_f64()
        .map_or_else(|| "—".to_string(), |v| fmt_value(v, unit))
}

fn fmt_move(row: &Value, key: &str) -> &'static str {
    match row[key].as_str() {
        Some("better") => " ↑",
        Some("worse") => " ↓",
        Some("same") => " =",
        _ => "",
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn render_markdown(page: &Value) -> String {
    let counts = &page["counts"];
    let mut out = String::new();
    out.push_str(&format!(
        "# DX scoreboard trend — {}\n\n",
        page["date"].as_str().unwrap_or("?")
    ));
    out.push_str(&format!(
        "Revision `{}` on `{}`; gate `{}`; {} night(s) of receipts since {}.\n\n",
        page["revision"].as_str().unwrap_or("?"),
        page["runner_class"].as_str().unwrap_or("?"),
        page["gate_status"].as_str().unwrap_or("not_run"),
        page["nights"],
        page["first_night"].as_str().unwrap_or("?"),
    ));
    out.push_str(&format!(
        "**{} of {} metrics are off their bar**, {} at it ({} of those partially measured), {} not measured, {} failed. \
         Since the previous comparable night: {} better, {} worse, {} unchanged.\n\n",
        counts["off_bar"],
        counts["total"],
        counts["at_bar"],
        counts["at_bar_partial"],
        counts["not_measured"],
        counts["failed"],
        page["since_previous"]["better"],
        page["since_previous"]["worse"],
        page["since_previous"]["same"],
    ));
    out.push_str("Arrows read against each metric's direction (↑ better, ↓ worse); sparkline height is the raw value, oldest first. \"Week ago\" is a night 7 to 9 days back. Wall time and memory compare only with the same runner class.\n\n");
    out.push_str("## Off the bar, farthest first\n\n");
    if page["off_bar"].as_array().is_none_or(Vec::is_empty) {
        out.push_str("Every measured metric is at its bar.\n\n");
    } else {
        out.push_str("| # | Metric | Now | Target | Short by | Previous | Week ago | Trend |\n");
        out.push_str("|---|---|---|---|---|---|---|---|\n");
        for (rank, row) in page["off_bar"].as_array().into_iter().flatten().enumerate() {
            let unit = row["unit"].as_str().unwrap_or_default();
            let short = if row["zero_target_miss"] == json!(true) {
                "any (target 0)".to_string()
            } else if row["shortfall"].as_f64().unwrap_or_default() <= 0.0 {
                "rounds to 0".to_string()
            } else {
                format!(
                    "{:.0}%",
                    row["distance"].as_f64().unwrap_or_default() * 100.0
                )
            };
            let partial = if row["partial"] == json!(true) {
                " (partial)"
            } else {
                ""
            };
            out.push_str(&format!(
                "| {} | `{}` {}{} | {} | {} | {} | {}{} | {}{} | {} |\n",
                rank + 1,
                row["id"].as_str().unwrap_or_default(),
                cell(row["title"].as_str().unwrap_or_default()),
                partial,
                fmt_opt(&row["value"], unit),
                fmt_opt(&row["target"], unit),
                short,
                fmt_opt(&row["previous"], unit),
                fmt_move(row, "since_previous"),
                fmt_opt(&row["week_ago"], unit),
                fmt_move(row, "since_week_ago"),
                row["sparkline"].as_str().unwrap_or_default(),
            ));
        }
        out.push('\n');
    }
    out.push_str("## At the bar\n\n");
    let at: Vec<&Value> = page["at_bar"].as_array().into_iter().flatten().collect();
    if at.is_empty() {
        out.push_str("None.\n\n");
    } else {
        out.push_str("| Metric | Now | Target | Previous | Week ago | Trend |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for row in at {
            let unit = row["unit"].as_str().unwrap_or_default();
            let partial = if row["partial"] == json!(true) {
                " (partial)"
            } else {
                ""
            };
            out.push_str(&format!(
                "| `{}`{} | {} | {} | {}{} | {}{} | {} |\n",
                row["id"].as_str().unwrap_or_default(),
                partial,
                fmt_opt(&row["value"], unit),
                fmt_opt(&row["target"], unit),
                fmt_opt(&row["previous"], unit),
                fmt_move(row, "since_previous"),
                fmt_opt(&row["week_ago"], unit),
                fmt_move(row, "since_week_ago"),
                row["sparkline"].as_str().unwrap_or_default(),
            ));
        }
        out.push('\n');
    }
    if let Some(failed) = page["failed"].as_array().filter(|f| !f.is_empty()) {
        out.push_str("## Failed instruments\n\n");
        for row in failed {
            out.push_str(&format!(
                "- `{}`: {}\n",
                row["id"].as_str().unwrap_or_default(),
                cell(row["reason"].as_str().unwrap_or("no reason recorded"))
            ));
        }
        out.push('\n');
    }
    out.push_str("## Not measured this night\n\n");
    let boards = page["not_measured"].as_object();
    if boards.is_none_or(serde_json::Map::is_empty) {
        out.push_str("None.\n");
    } else {
        out.push_str("| Board | Count | Metrics |\n|---|---|---|\n");
        for (board, rows) in boards.into_iter().flatten() {
            let rows = rows.as_array().map(Vec::as_slice).unwrap_or_default();
            let ids: Vec<String> = rows
                .iter()
                .map(|r| format!("`{}`", r["id"].as_str().unwrap_or_default()))
                .collect();
            out.push_str(&format!(
                "| {board} | {} | {} |\n",
                rows.len(),
                ids.join(", ")
            ));
        }
    }
    out
}

/// Keeps the newest `keep` receipts in the history directory, never
/// removing the receipt this run wrote (a backdated `--date` stays).
fn prune(dir: &Path, keep: usize, current: &Path) -> Result<(), String> {
    let mut receipts: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|error| format!("read {}: {error}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_receipt_name(path) && path.as_path() != current)
        .collect();
    receipts.sort();
    let excess = receipts.len().saturating_sub(keep.saturating_sub(1));
    for path in receipts.into_iter().take(excess) {
        fs::remove_file(&path).map_err(|error| format!("remove {}: {error}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let date = match options.date {
        Some(date) => date,
        None => today_utc()?,
    };
    let report = read_json(&options.report)?;
    let receipt = receipt_from_report(&report, &date)?;
    let history = load_history(&options.history, &receipt)?;
    let page = evaluate(&history, &receipt)?;
    fs::create_dir_all(&options.history)
        .map_err(|error| format!("create {}: {error}", options.history.display()))?;
    let body = serde_json::to_string_pretty(&receipt)
        .map_err(|error| format!("serialize receipt: {error}"))?;
    let name = format!("dx-trend-{date}.json");
    fs::write(options.history.join(&name), format!("{body}\n"))
        .map_err(|error| format!("write receipt: {error}"))?;
    prune(&options.history, options.keep, &options.history.join(&name))?;
    let page_body =
        serde_json::to_string_pretty(&page).map_err(|error| format!("serialize page: {error}"))?;
    crate::write_report("dx-trend.json", &format!("{page_body}\n"))?;
    let markdown = render_markdown(&page);
    crate::write_report("dx-trend.md", &markdown)?;
    println!("{markdown}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, direction: &str, target: f64, value: Option<f64>, status: &str) -> Value {
        json!({
            "id": id, "board": id.split('.').next().unwrap_or("x"), "title": id,
            "unit": if id.starts_with("speed") { "ms" } else { "ratio" },
            "direction": direction, "target": target, "value": value, "status": status,
            "runner_dependent": id.starts_with("speed"), "reason": "why",
        })
    }

    fn report(rows: Vec<Value>, runner: &str) -> Value {
        json!({"report": "dx-scoreboard", "revision": "abc", "runner_class": runner,
               "gate": {"status": "pass"}, "metrics": rows})
    }

    fn night(date: &str, runner: &str, rows: Vec<Value>) -> Result<Value, String> {
        receipt_from_report(&report(rows, runner), date)
    }

    #[test]
    fn dates_round_trip_and_reject_garbage() {
        for date in [
            "1970-01-01",
            "2026-02-28",
            "2026-03-01",
            "2026-11-15",
            "2000-02-29",
        ] {
            let days = days_from_civil(date);
            assert!(days.is_some(), "{date}");
            assert_eq!(civil_from_days(days.unwrap_or_default()), date);
        }
        assert_eq!(
            days_from_civil("2026-11-15")
                .map(|d| d - 7)
                .map(civil_from_days)
                .as_deref(),
            Some("2026-11-08")
        );
        for bad in [
            "2026-13-01",
            "26-11-15",
            "2026-11-1x",
            "2026/11/15",
            "2026-02-30",
            "2026-04-31",
            "+026-11-15",
            "",
        ] {
            assert!(days_from_civil(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_failed_or_valueless_row_never_trends_as_a_value() -> Result<(), String> {
        let r = night(
            "2026-11-15",
            "linux-x64",
            vec![
                row("trust.a", "lower_is_better", 0.2, Some(0.9), "failed"),
                row("trust.b", "lower_is_better", 0.2, None, "below_target"),
                row("trust.c", "lower_is_better", 0.2, Some(0.1), "not_measured"),
            ],
        )?;
        let metrics = r["metrics"].as_array().ok_or("metrics")?;
        assert!(metrics.iter().all(|m| m["value"].is_null()));
        assert_eq!(metrics[0]["status"], "failed");
        assert_eq!(metrics[1]["status"], "not_measured");
        let page = evaluate(&load_history(Path::new("/nonexistent"), &r)?, &r)?;
        assert_eq!(page["counts"]["failed"], 1);
        assert_eq!(page["counts"]["not_measured"], 2);
        assert_eq!(page["counts"]["off_bar"], 0);
        assert_eq!(page["counts"]["at_bar"], 0);
        Ok(())
    }

    #[test]
    fn a_report_of_another_kind_is_refused() {
        let result = receipt_from_report(
            &json!({"report": "outcome-page", "metrics": []}),
            "2026-11-15",
        );
        assert!(matches!(result, Err(m) if m.contains("not a dx-scoreboard report")));
    }

    #[test]
    fn off_bar_ranks_zero_target_misses_first_then_relative_distance() -> Result<(), String> {
        let r = night(
            "2026-11-15",
            "linux-x64",
            vec![
                row(
                    "speed.warm",
                    "lower_is_better",
                    1000.0,
                    Some(1500.0),
                    "below_target",
                ),
                row(
                    "trust.abstention",
                    "lower_is_better",
                    0.2,
                    Some(0.38),
                    "below_target",
                ),
                row(
                    "trust.false_exposed",
                    "lower_is_better",
                    0.0,
                    Some(0.01),
                    "below_target",
                ),
                row(
                    "trust.recall",
                    "higher_is_better",
                    0.8,
                    Some(0.6),
                    "below_target",
                ),
                row(
                    "trust.precision",
                    "higher_is_better",
                    0.95,
                    Some(1.0),
                    "meets_target",
                ),
            ],
        )?;
        let page = evaluate(&load_history(Path::new("/nonexistent"), &r)?, &r)?;
        let ids: Vec<&str> = page["off_bar"]
            .as_array()
            .ok_or("off")?
            .iter()
            .filter_map(|r| r["id"].as_str())
            .collect();
        // 0.38 vs 0.2 is 90% short, 1500 vs 1000 is 50%, 0.6 vs 0.8 is 25%.
        assert_eq!(
            ids,
            [
                "trust.false_exposed",
                "trust.abstention",
                "speed.warm",
                "trust.recall"
            ]
        );
        assert!(page["off_bar"][0]["distance"].is_null());
        assert_eq!(page["off_bar"][0]["zero_target_miss"], true);
        assert_eq!(page["at_bar"][0]["id"], "trust.precision");
        let md = render_markdown(&page);
        assert!(md.contains("**4 of 5 metrics are off their bar**"));
        assert!(md.contains("any (target 0)"));
        assert!(md.contains("| 2 | `trust.abstention`"));
        assert!(md.contains("90%"));
        Ok(())
    }

    #[test]
    fn trend_uses_previous_and_week_old_nights_and_skips_other_runners() -> Result<(), String> {
        let mut history = BTreeMap::new();
        for (date, runner, ms, ratio) in [
            ("2026-11-06", "linux-x64", 3000.0, 0.40),
            ("2026-11-08", "linux-x64", 2500.0, 0.39),
            ("2026-11-14", "macos-arm64", 100.0, 0.30),
        ] {
            history.insert(
                date.to_string(),
                night(
                    date,
                    runner,
                    vec![
                        row(
                            "speed.warm",
                            "lower_is_better",
                            1000.0,
                            Some(ms),
                            "below_target",
                        ),
                        row(
                            "trust.abstention",
                            "lower_is_better",
                            0.2,
                            Some(ratio),
                            "below_target",
                        ),
                    ],
                )?,
            );
        }
        let current = night(
            "2026-11-15",
            "linux-x64",
            vec![
                row(
                    "speed.warm",
                    "lower_is_better",
                    1000.0,
                    Some(2000.0),
                    "below_target",
                ),
                row(
                    "trust.abstention",
                    "lower_is_better",
                    0.2,
                    Some(0.35),
                    "below_target",
                ),
            ],
        )?;
        history.insert("2026-11-15".to_string(), current.clone());
        let page = evaluate(&history, &current)?;
        let find = |id: &str| {
            page["off_bar"]
                .as_array()
                .and_then(|rows| rows.iter().find(|r| r["id"] == id))
                .cloned()
                .unwrap_or_default()
        };
        let speed = find("speed.warm");
        // The macOS night is not comparable wall time; the previous is 11-08.
        assert_eq!(speed["previous"], 2500.0);
        assert_eq!(speed["since_previous"], "better");
        assert_eq!(speed["week_ago"], 2500.0);
        assert_eq!(speed["nights"], 3);
        let ratio = find("trust.abstention");
        // Runner-independent: the macOS night counts and is worse than 0.35.
        assert_eq!(ratio["previous"], 0.30);
        assert_eq!(ratio["since_previous"], "worse");
        assert_eq!(ratio["week_ago"], 0.39);
        assert_eq!(ratio["since_week_ago"], "better");
        assert_eq!(page["since_previous"]["better"], 1);
        assert_eq!(page["since_previous"]["worse"], 1);
        Ok(())
    }

    #[test]
    fn a_moved_target_starts_a_new_series() -> Result<(), String> {
        let mut history = BTreeMap::new();
        history.insert(
            "2026-11-14".to_string(),
            night(
                "2026-11-14",
                "linux-x64",
                vec![row(
                    "trust.abstention",
                    "lower_is_better",
                    0.3,
                    Some(0.25),
                    "meets_target",
                )],
            )?,
        );
        let current = night(
            "2026-11-15",
            "linux-x64",
            vec![row(
                "trust.abstention",
                "lower_is_better",
                0.2,
                Some(0.25),
                "below_target",
            )],
        )?;
        let page = evaluate(&history, &current)?;
        assert!(page["off_bar"][0]["previous"].is_null());
        Ok(())
    }

    #[test]
    fn run_writes_a_receipt_reads_it_back_and_prunes_the_oldest() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-dx-trend-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let report_path = dir.join("dx-scoreboard.json");
        let history = dir.join("history");
        fs::create_dir_all(&history).map_err(|e| e.to_string())?;
        fs::write(
            history.join("unrelated.json"),
            "{\"schema_version\": \"other\"}",
        )
        .map_err(|e| e.to_string())?;
        for (date, value) in [
            ("2026-11-13", 0.40),
            ("2026-11-14", 0.38),
            ("2026-11-15", 0.36),
        ] {
            let body = report(
                vec![row(
                    "trust.abstention",
                    "lower_is_better",
                    0.2,
                    Some(value),
                    "below_target",
                )],
                "linux-x64",
            );
            fs::write(&report_path, body.to_string()).map_err(|e| e.to_string())?;
            let args: Vec<String> = [
                "--report",
                report_path.to_str().unwrap_or_default(),
                "--history",
                history.to_str().unwrap_or_default(),
                "--date",
                date,
                "--keep",
                "2",
            ]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
            run(&args)?;
        }
        let mut names: Vec<String> = fs::read_dir(&history)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "dx-trend-2026-11-14.json",
                "dx-trend-2026-11-15.json",
                "unrelated.json"
            ]
        );
        let saved = read_json(&history.join("dx-trend-2026-11-15.json"))?;
        assert_eq!(saved["metrics"][0]["value"], 0.36);
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn options_need_report_and_history() {
        assert!(matches!(parse_options(&[]), Err(m) if m.contains("--report is required")));
        let args: Vec<String> = ["--report", "r.json", "--history", "h", "--date", "2026-1-1"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert!(matches!(parse_options(&args), Err(m) if m.contains("is not YYYY-MM-DD")));
    }

    #[test]
    fn sparkline_maps_the_range_onto_eight_levels() {
        assert_eq!(
            sparkline(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]),
            "▁▂▃▄▅▆▇█"
        );
        assert_eq!(sparkline(&[10.0, 0.0, 5.0]), "█▁▅");
        assert_eq!(sparkline(&[2.0, 2.0]), "▄▄");
    }

    #[test]
    fn an_incomplete_sample_is_never_at_the_bar() -> Result<(), String> {
        // A pilot that stopped after 130 ms is under its 1000 ms target, and
        // the scoreboard marks the row below_target with that number.
        let mut early = row(
            "speed.pilot",
            "lower_is_better",
            1000.0,
            Some(130.0),
            "below_target",
        );
        early["samples"] = json!([{"repo": "a", "value": 130.0, "status": "incomplete"}]);
        let mut done = row(
            "speed.check",
            "lower_is_better",
            1000.0,
            Some(900.0),
            "meets_target",
        );
        done["samples"] = json!([{"repo": "a", "value": 900.0, "status": "meets_target"}]);
        let r = night("2026-11-15", "linux-x64", vec![early, done])?;
        assert_eq!(r["metrics"][0]["status"], "incomplete");
        assert!(r["metrics"][0]["value"].is_null());
        assert_eq!(r["metrics"][1]["value"], 900.0);
        let page = evaluate(&load_history(Path::new("/nonexistent"), &r)?, &r)?;
        assert_eq!(page["counts"]["at_bar"], 1);
        assert_eq!(page["at_bar"][0]["id"], "speed.check");
        assert_eq!(page["counts"]["not_measured"], 1);
        assert_eq!(page["not_measured"]["speed"][0]["id"], "speed.pilot");
        Ok(())
    }

    #[test]
    fn week_ago_ignores_nights_older_than_nine_days() -> Result<(), String> {
        let mut history = BTreeMap::new();
        for (date, value) in [
            ("2026-10-01", 0.50),
            ("2026-11-05", 0.45),
            ("2026-11-14", 0.40),
        ] {
            history.insert(
                date.to_string(),
                night(
                    date,
                    "linux-x64",
                    vec![row(
                        "trust.a",
                        "lower_is_better",
                        0.2,
                        Some(value),
                        "below_target",
                    )],
                )?,
            );
        }
        let current = night(
            "2026-11-15",
            "linux-x64",
            vec![row(
                "trust.a",
                "lower_is_better",
                0.2,
                Some(0.35),
                "below_target",
            )],
        )?;
        // 11-05 is 10 days back: outside the 7-9 day window, so no week-ago value.
        let page = evaluate(&history, &current)?;
        assert!(page["off_bar"][0]["week_ago"].is_null());
        assert_eq!(page["off_bar"][0]["previous"], 0.40);
        history.insert(
            "2026-11-07".to_string(),
            night(
                "2026-11-07",
                "linux-x64",
                vec![row(
                    "trust.a",
                    "lower_is_better",
                    0.2,
                    Some(0.42),
                    "below_target",
                )],
            )?,
        );
        let page = evaluate(&history, &current)?;
        assert_eq!(page["off_bar"][0]["week_ago"], 0.42);
        assert_eq!(page["off_bar"][0]["week_ago_date"], "2026-11-07");
        Ok(())
    }

    #[test]
    fn an_unreadable_receipt_is_skipped_not_fatal() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-dx-trend-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(dir.join("dx-trend-2026-11-10.json"), "{not json").map_err(|e| e.to_string())?;
        fs::write(
            dir.join("dx-trend-2026-11-11.json"),
            json!({"schema_version": RECEIPT_SCHEMA, "date": "2026-02-30", "metrics": []})
                .to_string(),
        )
        .map_err(|e| e.to_string())?;
        let good = night("2026-11-12", "linux-x64", vec![])?;
        fs::write(dir.join("dx-trend-2026-11-12.json"), good.to_string())
            .map_err(|e| e.to_string())?;
        let current = night("2026-11-15", "linux-x64", vec![])?;
        let history = load_history(&dir, &current)?;
        assert_eq!(
            history.keys().cloned().collect::<Vec<_>>(),
            ["2026-11-12", "2026-11-15"]
        );
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn zero_target_misses_rank_by_size_and_units_render_cleanly() -> Result<(), String> {
        let r = night(
            "2026-11-15",
            "linux-x64",
            vec![
                row(
                    "trust.a_small",
                    "lower_is_better",
                    0.0,
                    Some(0.01),
                    "below_target",
                ),
                row(
                    "trust.b_large",
                    "lower_is_better",
                    0.0,
                    Some(0.05),
                    "below_target",
                ),
            ],
        )?;
        let page = evaluate(&BTreeMap::new(), &r)?;
        assert_eq!(page["off_bar"][0]["id"], "trust.b_large");
        assert_eq!(page["off_bar"][1]["id"], "trust.a_small");
        assert_eq!(fmt_value(5.0, ""), "5");
        assert_eq!(fmt_value(5.5, "ms"), "5.50 ms");
        assert_eq!(fmt_value(0.368, "ratio"), "36.8%");
        Ok(())
    }

    #[test]
    fn a_producer_miss_hidden_by_rounding_stays_off_the_bar() -> Result<(), String> {
        // 0.94999 vs 0.95 is judged below_target, then exported as 0.95.
        let r = night(
            "2026-11-15",
            "linux-x64",
            vec![
                row(
                    "trust.recall",
                    "higher_is_better",
                    0.95,
                    Some(0.95),
                    "below_target",
                ),
                row(
                    "trust.false_rate",
                    "lower_is_better",
                    0.05,
                    Some(0.05),
                    "below_target",
                ),
                row(
                    "trust.precision",
                    "higher_is_better",
                    0.95,
                    Some(0.95),
                    "meets_target",
                ),
                // 0.00004 against a zero target, exported as 0.0.
                row(
                    "trust.false_exposed",
                    "lower_is_better",
                    0.0,
                    Some(0.0),
                    "below_target",
                ),
                row(
                    "trust.false_silent",
                    "lower_is_better",
                    0.0,
                    Some(0.0),
                    "meets_target",
                ),
            ],
        )?;
        let page = evaluate(&BTreeMap::new(), &r)?;
        let off: Vec<&str> = page["off_bar"]
            .as_array()
            .ok_or("off")?
            .iter()
            .filter_map(|r| r["id"].as_str())
            .collect();
        assert_eq!(
            off,
            ["trust.false_exposed", "trust.false_rate", "trust.recall"]
        );
        assert_eq!(page["off_bar"][0]["zero_target_miss"], true);
        assert_eq!(page["at_bar"][0]["id"], "trust.false_silent");
        assert_eq!(page["at_bar"][1]["id"], "trust.precision");
        let md = render_markdown(&page);
        assert!(md.contains("| 1 | `trust.false_exposed`"));
        assert!(md.contains("any (target 0)"));
        assert!(md.contains("rounds to 0"));
        Ok(())
    }

    #[test]
    fn a_partial_pass_is_counted_and_marked() -> Result<(), String> {
        let mut partial = row(
            "speed.check",
            "lower_is_better",
            1000.0,
            Some(900.0),
            "meets_target",
        );
        partial["partial"] = json!(true);
        let r = night("2026-11-15", "linux-x64", vec![partial])?;
        let page = evaluate(&BTreeMap::new(), &r)?;
        assert_eq!(page["counts"]["at_bar"], 1);
        assert_eq!(page["counts"]["at_bar_partial"], 1);
        let md = render_markdown(&page);
        assert!(md.contains("1 at it (1 of those partially measured)"));
        assert!(md.contains("| `speed.check` (partial) | 900 ms | 1000 ms |"));
        Ok(())
    }

    #[test]
    fn a_moved_corpus_pin_or_repo_set_breaks_the_series() -> Result<(), String> {
        let per_repo = |value: f64, repos: &[&str]| {
            let mut m = row(
                "speed.warm",
                "lower_is_better",
                1000.0,
                Some(value),
                "below_target",
            );
            m["per_repo"] = json!(true);
            m["samples"] = json!(
                repos
                    .iter()
                    .map(|r| json!({"repo": r, "status": "below_target", "value": value}))
                    .collect::<Vec<_>>()
            );
            m
        };
        let corpus = |sha: &str| {
            json!([
                {"id": "serde", "url": "u1", "sha": sha, "base": "HEAD~1", "language": "rust", "expected_partial": null},
                {"id": "regex", "url": "u2", "sha": "r1", "base": "HEAD~1", "language": "rust", "expected_partial": null},
            ])
        };
        let with_corpus = |date: &str, sha: &str, metric: Value| -> Result<Value, String> {
            let mut rep = report(vec![metric], "linux-x64");
            rep["corpus"] = corpus(sha);
            receipt_from_report(&rep, date)
        };
        let current = with_corpus("2026-11-15", "s2", per_repo(1500.0, &["serde", "regex"]))?;
        let mut history = BTreeMap::new();
        // Same metric metadata, but serde's pin moved: not comparable.
        history.insert(
            "2026-11-13".to_string(),
            with_corpus("2026-11-13", "s1", per_repo(3000.0, &["serde", "regex"]))?,
        );
        // Same pins, but one repository dropped out: not comparable.
        history.insert(
            "2026-11-14".to_string(),
            with_corpus("2026-11-14", "s2", per_repo(2000.0, &["serde"]))?,
        );
        let page = evaluate(&history, &current)?;
        assert!(page["off_bar"][0]["previous"].is_null());
        assert_eq!(page["off_bar"][0]["nights"], 1);
        assert_eq!(page["since_previous"]["better"], 0);
        // Same pins and repository set: comparable.
        history.insert(
            "2026-11-12".to_string(),
            with_corpus("2026-11-12", "s2", per_repo(1800.0, &["regex", "serde"]))?,
        );
        let page = evaluate(&history, &current)?;
        assert_eq!(page["off_bar"][0]["previous"], 1800.0);
        Ok(())
    }
}
