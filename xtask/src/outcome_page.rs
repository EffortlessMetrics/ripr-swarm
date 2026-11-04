//! `cargo xtask repair-start-page` (#12903): measures the pinned small-diff
//! repair-start set against a built `ripr` binary, appends one dated receipt,
//! and renders one outcome page with the four north-star numbers, their trend
//! and the bar for each. With `--gate` a repair-start regression beyond the
//! stated tolerance, or a broken instrument, exits nonzero.
//!
//! Scope and non-claims: the set is 24 small Cargo diffs rebuilt from
//! `benchmarks/repair_start/cases.json`; it measures whether bare
//! `ripr repair --root .` starts a repair after `ripr check`, not whether the
//! repair is correct. Agent fix success and the false-actionable rate are read
//! from an optional dx-scoreboard report and stay `not_measured` without one.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const CASES_PATH: &str = "benchmarks/repair_start/cases.json";
const BARS_PATH: &str = "benchmarks/repair_start/bars.json";
const SEED_DIR: &str = "metrics/outcome/seed";
const RECEIPT_SCHEMA: &str = "ripr-repair-start-receipt-v1";
const PAGE_SCHEMA: &str = "ripr-outcome-page-v1";
const COMMAND_TIMEOUT: Duration = Duration::from_mins(2);
const TREND_ROWS: usize = 14;
const USAGE: &str = "usage: cargo xtask repair-start-page [--ripr <binary>] [--date <YYYY-MM-DD>] [--receipts <dir>] [--dx-scoreboard <report.json>] [--work-dir <dir>] [--gate]
  --ripr          measure the pinned set against this binary and write a dated receipt (omit to render from existing receipts)
  --date          receipt date (default: today, UTC)
  --receipts      extra receipt directory read for the trend; the new receipt is also written here
  --dx-scoreboard dx-scoreboard report to read agent.* and trust.*false_actionable metrics from
  --work-dir      scratch directory for the rebuilt case repositories (default: target/ripr/repair-start)
  --gate          exit nonzero on a repair-start regression beyond tolerance or an instrument failure";

#[derive(Default)]
struct Options {
    ripr: Option<PathBuf>,
    date: Option<String>,
    receipts: Option<PathBuf>,
    dx_scoreboard: Option<PathBuf>,
    work_dir: Option<PathBuf>,
    gate: bool,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--ripr" => options.ripr = Some(PathBuf::from(value("--ripr")?)),
            "--date" => options.date = Some(value("--date")?),
            "--receipts" => options.receipts = Some(PathBuf::from(value("--receipts")?)),
            "--dx-scoreboard" => {
                options.dx_scoreboard = Some(PathBuf::from(value("--dx-scoreboard")?));
            }
            "--work-dir" => options.work_dir = Some(PathBuf::from(value("--work-dir")?)),
            "--gate" => options.gate = true,
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    if let Some(date) = &options.date
        && !valid_date(date)
    {
        return Err(format!("--date `{date}` is not YYYY-MM-DD\n{USAGE}"));
    }
    Ok(options)
}

fn valid_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    matches!(parts.as_slice(), [y, m, d]
        if y.len() == 4 && m.len() == 2 && d.len() == 2
            && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())))
}

/// UTC civil date from a unix day count (Howard Hinnant's algorithm).
fn civil_date(unix_days: i64) -> String {
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
    Ok(civil_date(days))
}

fn workspace_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask CARGO_MANIFEST_DIR has no parent workspace".to_string())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
    ms: f64,
    timed_out: bool,
}

/// Run one command with output redirected to files, killing it at the timeout.
fn run_bounded(program: &Path, args: &[&str], cwd: &Path, scratch: &Path) -> Result<Run, String> {
    let out_path = scratch.join("stdout.txt");
    let err_path = scratch.join("stderr.txt");
    let out = File::create(&out_path).map_err(|error| format!("create stdout file: {error}"))?;
    let err = File::create(&err_path).map_err(|error| format!("create stderr file: {error}"))?;
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|error| format!("spawn {}: {error}", program.display()))?;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("wait for {}: {error}", program.display()))?
        {
            break status;
        }
        if started.elapsed() > COMMAND_TIMEOUT {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| format!("reap {}: {error}", program.display()))?;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(Run {
        code: status.code().unwrap_or(-1),
        stdout: fs::read_to_string(&out_path).unwrap_or_default(),
        stderr: fs::read_to_string(&err_path).unwrap_or_default(),
        ms,
        timed_out,
    })
}

fn git(cwd: &Path, args: &[&str]) -> Result<(), String> {
    let status = Command::new("git")
        .args(["-c", "user.email=a@b", "-c", "user.name=n"])
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("spawn git {}: {error}", args.join(" ")))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed in {}",
            args.join(" "),
            cwd.display()
        ))
    }
}

fn write_tree(root: &Path, files: &Value) -> Result<(), String> {
    let map = files
        .as_object()
        .ok_or_else(|| "case tree is not an object".to_string())?;
    for (relative, body) in map {
        if relative.starts_with('/') || relative.split('/').any(|part| part == "..") {
            return Err(format!("case path `{relative}` escapes the case directory"));
        }
        let text = body
            .as_str()
            .ok_or_else(|| format!("case file `{relative}` is not text"))?;
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))?;
    }
    Ok(())
}

fn build_case(dir: &Path, case: &Value) -> Result<(), String> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|error| format!("clear {}: {error}", dir.display()))?;
    }
    fs::create_dir_all(dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    git(dir, &["init", "-q"])?;
    write_tree(dir, &case["base"])?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", "base"])?;
    write_tree(dir, &case["change"])?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", "change"])
}

fn first_line(text: &str, limit: usize) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(limit).collect()
}

fn classes_and_action(report: &Value) -> (Vec<String>, Option<String>) {
    let mut classes: Vec<String> = Vec::new();
    for finding in report["findings"].as_array().into_iter().flatten() {
        if let Some(class) = finding["classification"].as_str()
            && !classes.iter().any(|seen| seen == class)
        {
            classes.push(class.to_string());
        }
    }
    let action = report["canonical_next_action"]["action_class"]
        .as_str()
        .map(str::to_string);
    (classes, action)
}

fn measure_case(ripr: &Path, dir: &Path, id: &str) -> Value {
    let scratch = dir.join(".outcome-scratch");
    if let Err(error) = fs::create_dir_all(&scratch) {
        return json!({"id": id, "instrument_failure": format!("scratch: {error}")});
    }
    let check = run_bounded(
        ripr,
        &["check", "--base", "HEAD~1", "--format", "json"],
        dir,
        &scratch,
    );
    let repair = run_bounded(ripr, &["repair", "--root", "."], dir, &scratch);
    let (check, repair) = match (check, repair) {
        (Ok(check), Ok(repair)) => (check, repair),
        (Err(error), _) | (_, Err(error)) => {
            return json!({"id": id, "instrument_failure": error});
        }
    };
    let parsed = serde_json::from_str::<Value>(&check.stdout);
    let (classes, action, failure) = match &parsed {
        Ok(report) => {
            let (classes, action) = classes_and_action(report);
            (classes, action, None)
        }
        Err(error) => (
            Vec::new(),
            None,
            Some(format!(
                "check JSON did not parse (rc={}): {error}",
                check.code
            )),
        ),
    };
    let timed_out = check.timed_out || repair.timed_out;
    let reason = if repair.code == 0 {
        String::new()
    } else {
        first_line(&format!("{}\n{}", repair.stdout, repair.stderr), 160)
    };
    json!({
        "id": id,
        "classes": classes,
        "next_action": action,
        "check_rc": check.code,
        "check_ms": (check.ms * 10.0).round() / 10.0,
        "repair_rc": repair.code,
        "started": repair.code == 0 && !timed_out,
        "reason": reason,
        "instrument_failure": if timed_out { Some("command timed out".to_string()) } else { failure },
    })
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    values.get((values.len() - 1) / 2).copied()
}

fn build_receipt(ripr: &Path, date: &str, work: &Path) -> Result<Value, String> {
    let root = workspace_root()?;
    let cases = read_json(&root.join(CASES_PATH))?;
    let list = cases["cases"]
        .as_array()
        .ok_or_else(|| format!("{CASES_PATH} has no `cases` array"))?;
    fs::create_dir_all(work).map_err(|error| format!("create {}: {error}", work.display()))?;
    let version = Command::new(ripr)
        .arg("--version")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .map_err(|error| format!("run {} --version: {error}", ripr.display()))?;
    let mut rows = Vec::new();
    for (index, case) in list.iter().enumerate() {
        let id = case["id"].as_str().unwrap_or("?");
        eprintln!("[{}/{}] {id}", index + 1, list.len());
        let dir = work.join(id);
        build_case(&dir, case)?;
        rows.push(measure_case(ripr, &dir, id));
    }
    Ok(summarize(date, &version, &rows))
}

fn summarize(date: &str, version: &str, rows: &[Value]) -> Value {
    let started = rows.iter().filter(|r| r["started"] == json!(true)).count();
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|r| {
            r["instrument_failure"]
                .as_str()
                .map(|f| format!("{}: {f}", r["id"].as_str().unwrap_or("?")))
        })
        .collect();
    let mut check_ms: Vec<f64> = rows.iter().filter_map(|r| r["check_ms"].as_f64()).collect();
    let actionable: Vec<&Value> = rows
        .iter()
        .filter(|r| r["next_action"] == json!("inspect_details"))
        .collect();
    let actionable_started = actionable
        .iter()
        .filter(|r| r["started"] == json!(true))
        .count();
    json!({
        "schema_version": RECEIPT_SCHEMA,
        "date": date,
        "source": "xtask_measure",
        "ripr_version": version,
        "revision": std::env::var("GITHUB_SHA").unwrap_or_default(),
        "cases": rows.len(),
        "started": started,
        "not_ready": rows.len() - started,
        "actionable_cases": actionable.len(),
        "actionable_started": actionable_started,
        "check_ms_p50": median(&mut check_ms),
        "instrument_failures": failures,
        "rows": rows,
    })
}

fn load_receipts(dirs: &[PathBuf]) -> Result<BTreeMap<String, Value>, String> {
    let mut by_date: BTreeMap<String, Value> = BTreeMap::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();
        for path in paths {
            let receipt = read_json(&path)?;
            if receipt["schema_version"] != json!(RECEIPT_SCHEMA) {
                continue;
            }
            let date = receipt["date"]
                .as_str()
                .filter(|d| valid_date(d))
                .ok_or_else(|| format!("{} has no valid `date`", path.display()))?
                .to_string();
            // A measured receipt beats a manual seed for the same date; among
            // equals the later directory wins.
            let keep_existing = by_date.get(&date).is_some_and(|old| {
                old["source"] == json!("xtask_measure")
                    && receipt["source"] != json!("xtask_measure")
            });
            if !keep_existing {
                by_date.insert(date, receipt);
            }
        }
    }
    Ok(by_date)
}

fn dx_metric(report: &Value, id: &str) -> Option<f64> {
    report["metrics"]
        .as_array()?
        .iter()
        .find(|m| m["id"] == json!(id))
        .filter(|m| m["status"] != json!("not_measured"))
        .and_then(|m| m["value"].as_f64())
}

fn pct(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn state(met: Option<bool>) -> &'static str {
    match met {
        Some(true) => "meets bar",
        Some(false) => "below bar",
        None => "not measured",
    }
}

struct Evaluation {
    page: Value,
    failures: Vec<String>,
}

fn evaluate(
    receipts: &BTreeMap<String, Value>,
    bars: &Value,
    dx: Option<&Value>,
) -> Result<Evaluation, String> {
    let (latest_date, latest) = receipts
        .iter()
        .next_back()
        .ok_or_else(|| "no repair-start receipts to render".to_string())?;
    let cases = latest["cases"].as_u64().unwrap_or(0);
    let started = latest["started"].as_u64().unwrap_or(0);
    let rate = (cases > 0).then(|| started as f64 / cases as f64);
    let start_bar = &bars["repair_start_rate"];
    let bar_rate = start_bar["bar"].as_f64().unwrap_or(0.8);
    let floor = start_bar["floor_starts"].as_u64().unwrap_or(0);
    let tolerance = start_bar["tolerance_cases"].as_u64().unwrap_or(0);
    let p50 = latest["check_ms_p50"].as_f64();
    let p50_bar = bars["first_useful_result_p50_ms"]["bar"]
        .as_f64()
        .unwrap_or(1000.0);
    let agent = dx.and_then(|r| dx_metric(r, "agent.fix_success_rate"));
    let agent_bar = bars["agent_fix_success_rate"]["bar"]
        .as_f64()
        .unwrap_or(1.0);
    let fa_bar = bars["false_actionable_rate"]["bar"]
        .as_f64()
        .unwrap_or(0.03);
    let fa_ids = [
        ("rust", "trust.verdict_corpus_false_actionable"),
        (
            "typescript",
            "trust.verdict_corpus_typescript_false_actionable",
        ),
        ("python", "trust.verdict_corpus_python_false_actionable"),
    ];
    let fa: Vec<(&str, Option<f64>)> = fa_ids
        .iter()
        .map(|(lang, id)| (*lang, dx.and_then(|r| dx_metric(r, id))))
        .collect();

    let mut failures = Vec::new();
    let instrument = latest["instrument_failures"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    if latest["source"] == json!("xtask_measure") && (cases == 0 || instrument > 0) {
        let listed = latest["instrument_failures"]
            .as_array()
            .map(|v| {
                v.iter()
                    .filter_map(Value::as_str)
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        failures.push(format!(
            "instrument failure on {latest_date}: {instrument} of {cases} case(s) did not run (first: {listed})"
        ));
    }
    if started + tolerance < floor {
        failures.push(format!(
            "repair-start regression on {latest_date}: {started}/{cases} start a repair, floor is {floor} with a tolerance of {tolerance} case(s)"
        ));
    }
    let raise_floor = started > floor;

    let trend: Vec<Value> = receipts
        .iter()
        .rev()
        .take(TREND_ROWS)
        .map(|(date, r)| {
            json!({
                "date": date,
                "source": r["source"],
                "started": r["started"],
                "cases": r["cases"],
                "check_ms_p50": r["check_ms_p50"],
            })
        })
        .collect();

    let page = json!({
        "schema_version": PAGE_SCHEMA,
        "as_of": latest_date,
        "latest_receipt_source": latest["source"],
        "numbers": {
            "repair_start_rate": {
                "value": rate, "started": started, "cases": cases, "bar": bar_rate,
                "met": rate.map(|r| r >= bar_rate),
                "gate": {"floor_starts": floor, "tolerance_cases": tolerance, "raise_floor_to": if raise_floor { Some(started) } else { None } },
            },
            "first_useful_result_p50_ms": {"value": p50, "bar": p50_bar, "met": p50.map(|v| v < p50_bar), "gated": false},
            "agent_fix_success_rate": {"value": agent, "bar": agent_bar, "met": agent.map(|v| v >= agent_bar), "source": "dx-scoreboard agent.fix_success_rate"},
            "false_actionable_rate": {
                "bar": fa_bar,
                "by_language": fa.iter().map(|(lang, v)| json!({"language": lang, "value": v, "met": v.map(|x| x <= fa_bar)})).collect::<Vec<_>>(),
                "source": "dx-scoreboard trust.verdict_corpus_*false_actionable",
            },
        },
        "trend": trend,
        "gate": {"status": if failures.is_empty() { "pass" } else { "fail" }, "failures": failures},
        "non_claims": [
            "Repair start means bare `ripr repair --root .` exited 0 on a small rebuilt diff; it does not say the repair is correct.",
            "Static movement only; no runtime mutation outcome is claimed.",
            "First-useful-result time is the wall time of `ripr check --base HEAD~1 --format json` on a tiny crate and is runner-dependent, so it is shown against its bar but never gates.",
        ],
    });
    Ok(Evaluation { page, failures })
}

fn render_markdown(page: &Value) -> String {
    let n = &page["numbers"];
    let rs = &n["repair_start_rate"];
    let rate_text = rs["value"].as_f64().map_or("not measured".to_string(), pct);
    let mut out = String::new();
    out.push_str(&format!(
        "# ripr outcome page\n\nAs of {} (receipt source: {}). Generated by `cargo xtask repair-start-page`; do not edit.\n\n",
        page["as_of"].as_str().unwrap_or("?"),
        page["latest_receipt_source"].as_str().unwrap_or("?"),
    ));
    out.push_str("| Outcome | Now | Bar | State |\n|---|---|---|---|\n");
    out.push_str(&format!(
        "| Repair-start rate (ripr repair starts after ripr check, 24 small diffs) | {rate_text} ({}/{}) | at least {} | {} |\n",
        rs["started"], rs["cases"], pct(rs["bar"].as_f64().unwrap_or(0.0)),
        state(rs["met"].as_bool()),
    ));
    let p = &n["first_useful_result_p50_ms"];
    out.push_str(&format!(
        "| First useful result, p50 of `ripr check` | {} | under {} ms | {} |\n",
        p["value"]
            .as_f64()
            .map_or("not measured".to_string(), |v| format!("{v:.0} ms")),
        p["bar"].as_f64().unwrap_or(0.0),
        state(p["met"].as_bool()),
    ));
    let a = &n["agent_fix_success_rate"];
    out.push_str(&format!(
        "| Agent fix success | {} | at least {} | {} |\n",
        a["value"].as_f64().map_or("not measured".to_string(), pct),
        pct(a["bar"].as_f64().unwrap_or(0.0)),
        state(a["met"].as_bool()),
    ));
    let f = &n["false_actionable_rate"];
    for row in f["by_language"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| False-actionable rate, {} verdict corpus | {} | at most {} | {} |\n",
            row["language"].as_str().unwrap_or("?"),
            row["value"]
                .as_f64()
                .map_or("not measured".to_string(), pct),
            pct(f["bar"].as_f64().unwrap_or(0.0)),
            state(row["met"].as_bool()),
        ));
    }
    out.push_str(
        "\n## Trend (newest first)\n\n| Date | Starts | p50 check | Source |\n|---|---|---|---|\n",
    );
    for row in page["trend"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "| {} | {}/{} | {} | {} |\n",
            row["date"].as_str().unwrap_or("?"),
            row["started"],
            row["cases"],
            row["check_ms_p50"]
                .as_f64()
                .map_or("-".to_string(), |v| format!("{v:.0} ms")),
            row["source"].as_str().unwrap_or("?"),
        ));
    }
    let gate = &page["gate"];
    out.push_str(&format!(
        "\n## Gate\n\nRepair-start floor: {} start(s), tolerance {} case(s). Status: **{}**.\n",
        rs["gate"]["floor_starts"],
        rs["gate"]["tolerance_cases"],
        gate["status"].as_str().unwrap_or("?"),
    ));
    for failure in gate["failures"].as_array().into_iter().flatten() {
        out.push_str(&format!("- {}\n", failure.as_str().unwrap_or("?")));
    }
    if let Some(raise) = rs["gate"]["raise_floor_to"].as_u64() {
        out.push_str(&format!(
            "- The latest run beats the floor; raise `floor_starts` to {raise} in `{BARS_PATH}` in the change that earned it.\n"
        ));
    }
    out.push_str("\n## What this does not say\n\n");
    for line in page["non_claims"].as_array().into_iter().flatten() {
        out.push_str(&format!("- {}\n", line.as_str().unwrap_or("?")));
    }
    out
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let root = workspace_root()?;
    let mut dirs = vec![root.join(SEED_DIR)];
    if let Some(extra) = &options.receipts {
        dirs.push(extra.clone());
    }
    if let Some(ripr) = &options.ripr {
        let date = match &options.date {
            Some(date) => date.clone(),
            None => today_utc()?,
        };
        let work = options
            .work_dir
            .clone()
            .unwrap_or_else(|| root.join("target/ripr/repair-start"));
        // Cases run in their own directories, so a relative path must be resolved first.
        let ripr = fs::canonicalize(ripr)
            .map_err(|error| format!("--ripr {}: {error}", ripr.display()))?;
        let receipt = build_receipt(&ripr, &date, &work)?;
        let body = serde_json::to_string_pretty(&receipt)
            .map_err(|error| format!("serialize receipt: {error}"))?;
        let name = format!("repair-start-{date}.json");
        crate::write_report(&name, &format!("{body}\n"))?;
        if let Some(extra) = &options.receipts {
            fs::create_dir_all(extra)
                .map_err(|error| format!("create {}: {error}", extra.display()))?;
            fs::write(extra.join(&name), format!("{body}\n"))
                .map_err(|error| format!("write receipt: {error}"))?;
        } else {
            dirs.push(root.join("target/ripr/reports"));
        }
    } else if options.receipts.is_none() {
        dirs.push(root.join("target/ripr/reports"));
    }
    let receipts = load_receipts(&dirs)?;
    let bars = read_json(&root.join(BARS_PATH))?;
    let dx = options
        .dx_scoreboard
        .as_deref()
        .map(read_json)
        .transpose()?;
    let evaluation = evaluate(&receipts, &bars, dx.as_ref())?;
    let body = serde_json::to_string_pretty(&evaluation.page)
        .map_err(|error| format!("serialize page: {error}"))?;
    crate::write_report("outcome-page.json", &format!("{body}\n"))?;
    crate::write_report("outcome-page.md", &render_markdown(&evaluation.page))?;
    println!("{}", render_markdown(&evaluation.page));
    if options.gate && !evaluation.failures.is_empty() {
        return Err(format!(
            "outcome gate failed:\n  {}\nsee target/ripr/reports/outcome-page.md",
            evaluation.failures.join("\n  ")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(date: &str, source: &str, started: u64, failures: Vec<&str>) -> Value {
        json!({
            "schema_version": RECEIPT_SCHEMA, "date": date, "source": source,
            "cases": 24, "started": started, "check_ms_p50": 120.0,
            "instrument_failures": failures,
        })
    }

    fn bars() -> Value {
        json!({"repair_start_rate": {"bar": 0.8, "floor_starts": 7, "tolerance_cases": 1}})
    }

    fn by_date(items: Vec<Value>) -> BTreeMap<String, Value> {
        items
            .into_iter()
            .map(|r| (r["date"].as_str().unwrap_or("").to_string(), r))
            .collect()
    }

    #[test]
    fn civil_date_matches_known_days() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_453), "2025-12-31");
        assert_eq!(civil_date(20_514), "2026-03-02");
        assert_eq!(civil_date(20_761), "2026-11-04");
    }

    #[test]
    fn date_validation_rejects_shapes() {
        assert!(valid_date("2026-11-04"));
        assert!(!valid_date("2026-1-4"));
        assert!(!valid_date("26-11-04"));
        assert!(!valid_date("2026-11-0x"));
    }

    #[test]
    fn regression_within_tolerance_passes_and_beyond_fails() {
        let within = evaluate(
            &by_date(vec![receipt("2026-11-04", "xtask_measure", 6, vec![])]),
            &bars(),
            None,
        );
        assert!(matches!(within, Ok(e) if e.failures.is_empty()));
        let beyond = evaluate(
            &by_date(vec![receipt("2026-11-04", "xtask_measure", 5, vec![])]),
            &bars(),
            None,
        );
        assert!(
            matches!(beyond, Ok(e) if e.failures.len() == 1 && e.failures[0].contains("repair-start regression"))
        );
    }

    #[test]
    fn broken_instrument_fails_even_when_starts_hold() {
        let result = evaluate(
            &by_date(vec![receipt(
                "2026-11-04",
                "xtask_measure",
                9,
                vec!["arith: command timed out"],
            )]),
            &bars(),
            None,
        );
        assert!(
            matches!(result, Ok(e) if e.failures.iter().any(|f| f.contains("instrument failure")))
        );
    }

    #[test]
    fn earlier_dates_do_not_gate_and_trend_is_newest_first() -> Result<(), String> {
        let receipts = by_date(vec![
            receipt("2026-11-01", "manual_harness", 2, vec![]),
            receipt("2026-11-03", "xtask_measure", 8, vec![]),
        ]);
        let e = evaluate(&receipts, &bars(), None)?;
        assert!(e.failures.is_empty());
        assert_eq!(e.page["trend"][0]["date"], json!("2026-11-03"));
        assert_eq!(
            e.page["numbers"]["repair_start_rate"]["gate"]["raise_floor_to"],
            json!(8)
        );
        Ok(())
    }

    #[test]
    fn missing_dx_report_is_not_measured_not_zero() -> Result<(), String> {
        let e = evaluate(
            &by_date(vec![receipt("2026-11-04", "xtask_measure", 7, vec![])]),
            &bars(),
            None,
        )?;
        assert_eq!(
            e.page["numbers"]["agent_fix_success_rate"]["value"],
            Value::Null
        );
        assert_eq!(
            e.page["numbers"]["agent_fix_success_rate"]["met"],
            Value::Null
        );
        assert!(render_markdown(&e.page).contains("not measured"));
        Ok(())
    }

    #[test]
    fn dx_not_measured_status_hides_a_stale_value() {
        let report = json!({"metrics": [
            {"id": "agent.fix_success_rate", "status": "not_measured", "value": 1.0},
            {"id": "trust.verdict_corpus_false_actionable", "status": "meets_target", "value": 0.0},
        ]});
        assert_eq!(dx_metric(&report, "agent.fix_success_rate"), None);
        assert_eq!(
            dx_metric(&report, "trust.verdict_corpus_false_actionable"),
            Some(0.0)
        );
    }

    #[test]
    fn measured_receipt_beats_manual_seed_for_the_same_date() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("outcome-page-test-{}", std::process::id()));
        let seed = dir.join("seed");
        let live = dir.join("live");
        let io = |e: std::io::Error| e.to_string();
        fs::create_dir_all(&seed).map_err(io)?;
        fs::create_dir_all(&live).map_err(io)?;
        let write = |p: PathBuf, v: Value| fs::write(p, v.to_string()).map_err(|e| e.to_string());
        write(
            seed.join("a.json"),
            receipt("2026-11-02", "manual_harness", 7, vec![]),
        )?;
        write(
            live.join("a.json"),
            receipt("2026-11-02", "xtask_measure", 8, vec![]),
        )?;
        let loaded = load_receipts(&[live.clone(), seed.clone()]);
        let _ = fs::remove_dir_all(&dir);
        let loaded = loaded?;
        assert_eq!(loaded["2026-11-02"]["started"], json!(8));
        Ok(())
    }

    #[test]
    fn summarize_counts_actionable_and_instrument_failures() {
        let rows = vec![
            json!({"id": "a", "started": true, "check_ms": 100.0, "next_action": "inspect_details"}),
            json!({"id": "b", "started": false, "check_ms": 300.0, "next_action": "inspect_details"}),
            json!({"id": "c", "started": false, "check_ms": 200.0, "instrument_failure": "boom"}),
        ];
        let r = summarize("2026-11-04", "ripr 0.0.0", &rows);
        assert_eq!(r["started"], json!(1));
        assert_eq!(r["not_ready"], json!(2));
        assert_eq!(r["actionable_cases"], json!(2));
        assert_eq!(r["actionable_started"], json!(1));
        assert_eq!(r["check_ms_p50"], json!(200.0));
        assert_eq!(r["instrument_failures"], json!(["c: boom"]));
    }

    #[test]
    fn pinned_cases_and_bars_parse() -> Result<(), String> {
        let root = workspace_root()?;
        let cases = read_json(&root.join(CASES_PATH));
        let cases = cases?;
        let list = cases["cases"].as_array().map_or(0, Vec::len);
        assert_eq!(list, 24);
        for case in cases["cases"].as_array().into_iter().flatten() {
            assert!(case["base"].as_object().is_some_and(|m| !m.is_empty()));
            assert_ne!(
                case["base"], case["change"],
                "{} must change something",
                case["id"]
            );
        }
        let bars = read_json(&root.join(BARS_PATH));
        assert!(matches!(bars, Ok(b) if b["repair_start_rate"]["floor_starts"].is_u64()));
        Ok(())
    }
}
