//! `cargo xtask real-crate-dogfood`: `ripr check` on pinned real commits.
//!
//! `benchmarks/real_crate_dogfood/manifest.json` pins recent commits of
//! popular Rust crates. `run` analyzes each as `ripr check --base <rev>^` and
//! writes `target/ripr/reports/real-crate-dogfood.{json,md}` with counts and
//! timings, so an analyzer change is measured on real diffs before and after:
//! whether the tests a commit adds are named, how many test rows repeat a
//! test, how many findings sit on deleted lines, and how long a run takes.
//! The counts are not correctness verdicts.
//!
//! - `check` (offline) validates the manifest.
//! - `fetch --allow-network` shallow-fetches each pinned commit and its
//!   parent under `target/ripr/real-crate-dogfood/checkouts`.
//! - `run` analyzes each fetched checkout with `--ripr <binary>`.

use crate::run::run_output_owned_with_timeout;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const DEFAULT_MANIFEST: &str = "benchmarks/real_crate_dogfood/manifest.json";
const DEFAULT_ROOT: &str = "target/ripr/real-crate-dogfood/checkouts";
const REPORT_JSON: &str = "target/ripr/reports/real-crate-dogfood.json";
const REPORT_MD: &str = "target/ripr/reports/real-crate-dogfood.md";
const MANIFEST_SCHEMA_VERSION: &str = "ripr-real-crate-dogfood-v1";
const REPORT_SCHEMA_VERSION: &str = "ripr-real-crate-dogfood-report-v1";
const GIT_TIMEOUT: Duration = Duration::from_mins(10);

const USAGE: &str = "usage: cargo xtask real-crate-dogfood check [--manifest <path>]
       cargo xtask real-crate-dogfood fetch --allow-network [--manifest <path>] [--root <dir>] [--case <id>]...
       cargo xtask real-crate-dogfood run --ripr <binary> [--manifest <path>] [--root <dir>] [--case <id>]...

check validates the pinned manifest without network.
fetch shallow-fetches each pinned commit and its parent into <root>/<id>.
run executes `ripr check --base <rev>^ --format json` per case and writes
target/ripr/reports/real-crate-dogfood.{json,md}. Counts only; no verdicts.";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    corpus_version: String,
    description: String,
    limits: Vec<String>,
    timeout_secs: u64,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    #[serde(rename = "crate")]
    krate: String,
    url: String,
    revision: String,
    note: String,
}

#[derive(Debug, Default)]
struct Options {
    manifest: Option<PathBuf>,
    root: Option<PathBuf>,
    cases: Vec<String>,
    allow_network: bool,
    ripr: Option<PathBuf>,
}

pub(crate) fn real_crate_dogfood(args: &[String]) -> Result<(), String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(USAGE.to_string());
    };
    if matches!(command.as_str(), "--help" | "-h" | "help") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_options(command, rest)?;
    let manifest_path = options
        .manifest
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MANIFEST));
    let text = fs::read_to_string(&manifest_path)
        .map_err(|err| format!("read {}: {err}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|err| format!("parse {}: {err}", manifest_path.display()))?;
    validate_manifest(&manifest)?;
    match command.as_str() {
        "check" => {
            println!(
                "real-crate-dogfood: corpus {} ok: {} cases",
                manifest.corpus_version,
                manifest.cases.len()
            );
            Ok(())
        }
        "fetch" => fetch(&manifest, &options),
        "run" => run(&manifest, &options),
        other => Err(format!(
            "unknown real-crate-dogfood subcommand `{other}`\n{USAGE}"
        )),
    }
}

fn parse_options(command: &str, args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = || {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
        };
        match flag {
            "--manifest" => options.manifest = Some(PathBuf::from(value()?)),
            "--root" => options.root = Some(PathBuf::from(value()?)),
            "--ripr" => options.ripr = Some(PathBuf::from(value()?)),
            "--case" => options.cases.push(value()?),
            "--allow-network" => {
                options.allow_network = true;
                index += 1;
                continue;
            }
            _ => {
                return Err(format!(
                    "unknown real-crate-dogfood option `{flag}`\n{USAGE}"
                ));
            }
        }
        index += 2;
    }
    if command == "fetch" && !options.allow_network {
        return Err(
            "real-crate-dogfood fetch clones external repositories; pass --allow-network to opt in"
                .to_string(),
        );
    }
    if command == "run" && options.ripr.is_none() {
        return Err(format!(
            "real-crate-dogfood run needs --ripr <binary>\n{USAGE}"
        ));
    }
    Ok(options)
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "manifest schema_version `{}` is not `{MANIFEST_SCHEMA_VERSION}`",
            manifest.schema_version
        ));
    }
    if manifest.description.trim().is_empty() || manifest.limits.is_empty() {
        return Err("manifest needs a description and at least one limit".to_string());
    }
    if manifest.timeout_secs == 0 {
        return Err("manifest timeout_secs must be positive".to_string());
    }
    let mut seen = BTreeSet::new();
    for case in &manifest.cases {
        let id_ok = !case.id.is_empty()
            && case
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !id_ok || !seen.insert(case.id.as_str()) {
            return Err(format!(
                "case id `{}` is empty, malformed or repeated",
                case.id
            ));
        }
        if case.krate.is_empty() || !case.url.starts_with("https://") {
            return Err(format!("case `{}` needs a crate and an https url", case.id));
        }
        let full_sha =
            case.revision.len() == 40 && case.revision.bytes().all(|b| b.is_ascii_hexdigit());
        if !full_sha {
            return Err(format!(
                "case `{}` revision must be a full 40-hex commit id",
                case.id
            ));
        }
    }
    if manifest.cases.is_empty() {
        return Err("manifest has no cases".to_string());
    }
    Ok(())
}

fn select<'a>(manifest: &'a Manifest, only: &[String]) -> Result<Vec<&'a Case>, String> {
    if only.is_empty() {
        return Ok(manifest.cases.iter().collect());
    }
    only.iter()
        .map(|id| {
            manifest
                .cases
                .iter()
                .find(|case| &case.id == id)
                .ok_or_else(|| format!("no case `{id}` in the manifest"))
        })
        .collect()
}

fn root_of(options: &Options) -> PathBuf {
    options
        .root
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT))
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut owned = vec!["-C".to_string(), dir.to_string_lossy().into_owned()];
    owned.extend(args.iter().map(|arg| (*arg).to_string()));
    let context = format!(
        "real-crate-dogfood git {}",
        args.first().copied().unwrap_or("")
    );
    run_output_owned_with_timeout("git", &owned, GIT_TIMEOUT, &context)
        .map(|stdout| stdout.trim().to_string())
}

fn fetch(manifest: &Manifest, options: &Options) -> Result<(), String> {
    let root = root_of(options);
    fs::create_dir_all(&root).map_err(|err| format!("create {}: {err}", root.display()))?;
    let mut failures = Vec::new();
    for case in select(manifest, &options.cases)? {
        let dir = root.join(&case.id);
        match materialize(case, &dir) {
            Ok(reused) => eprintln!(
                "real-crate-dogfood: {} @ {} {}",
                case.id,
                &case.revision[..12],
                if reused { "(reused)" } else { "(fetched)" }
            ),
            Err(err) => {
                eprintln!("real-crate-dogfood: {} failed: {err}", case.id);
                failures.push(format!("{}: {err}", case.id));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "real-crate-dogfood fetch failed for {} case(s):\n  - {}",
            failures.len(),
            failures.join("\n  - ")
        ))
    }
}

fn at_revision(case: &Case, dir: &Path) -> bool {
    dir.join(".git").exists()
        && git(dir, &["rev-parse", "HEAD"]).is_ok_and(|head| head == case.revision)
        && git(dir, &["rev-parse", "--verify", "--quiet", "HEAD^"]).is_ok()
        && git(dir, &["status", "--porcelain", "--untracked-files=normal"])
            .is_ok_and(|status| status.is_empty())
}

/// Fetch the pinned commit with its parent. Only a missing or empty directory
/// is populated; an existing directory that is not the clean pin is reported,
/// never deleted.
fn materialize(case: &Case, dir: &Path) -> Result<bool, String> {
    if at_revision(case, dir) {
        return Ok(true);
    }
    let empty =
        !dir.exists() || fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none());
    if !empty {
        return Err(format!(
            "{} exists and is not a clean checkout of the pinned revision; move it or pass a different --root",
            dir.display()
        ));
    }
    fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    git(dir, &["init", "--quiet"])?;
    git(dir, &["remote", "add", "origin", &case.url])?;
    git(
        dir,
        &[
            "fetch",
            "--quiet",
            "--depth",
            "2",
            "--no-tags",
            "origin",
            &case.revision,
        ],
    )?;
    git(dir, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])?;
    if !at_revision(case, dir) {
        return Err("the fetched checkout is not the pinned revision with a parent".to_string());
    }
    Ok(false)
}

fn run(manifest: &Manifest, options: &Options) -> Result<(), String> {
    let Some(ripr) = options.ripr.as_ref() else {
        return Err(format!(
            "real-crate-dogfood run needs --ripr <binary>\n{USAGE}"
        ));
    };
    let ripr = fs::canonicalize(ripr).map_err(|err| format!("--ripr {}: {err}", ripr.display()))?;
    let root = root_of(options);
    let timeout = Duration::from_secs(manifest.timeout_secs);
    let mut rows = Vec::new();
    for case in select(manifest, &options.cases)? {
        let dir = root.join(&case.id);
        let row = match measure_case(case, &dir, &ripr, timeout) {
            Ok(row) => row,
            Err(err) => case_row(case, json!({"status": "error", "error": err})),
        };
        eprintln!("real-crate-dogfood: {} {}", case.id, row["status"]);
        rows.push(row);
    }
    let report = build_report(manifest, &rows);
    write_report(&report)?;
    println!("real-crate-dogfood: wrote {REPORT_JSON} and {REPORT_MD}");
    Ok(())
}

fn case_row(case: &Case, metrics: Value) -> Value {
    let mut row = json!({
        "id": case.id,
        "crate": case.krate,
        "revision": case.revision,
        "note": case.note,
    });
    if let (Some(target), Some(source)) = (row.as_object_mut(), metrics.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    row
}

fn measure_case(case: &Case, dir: &Path, ripr: &Path, timeout: Duration) -> Result<Value, String> {
    if !at_revision(case, dir) {
        return Err(format!(
            "{} is not a clean checkout of {}; run `fetch --allow-network` first",
            dir.display(),
            &case.revision[..12]
        ));
    }
    let base = format!("{}^", case.revision);
    let diff_files: BTreeSet<String> = git(dir, &["diff", "--name-only", &base, "HEAD"])?
        .lines()
        .map(str::to_string)
        .collect();
    let args = vec![
        "check".to_string(),
        "--root".to_string(),
        dir.to_string_lossy().into_owned(),
        "--base".to_string(),
        base,
        "--format".to_string(),
        "json".to_string(),
        "--quiet".to_string(),
    ];
    let started = Instant::now();
    let outcome = run_output_owned_with_timeout(
        &ripr.to_string_lossy(),
        &args,
        timeout,
        &format!("ripr check {}", case.id),
    );
    let seconds = started.elapsed().as_secs_f64();
    let stdout = match outcome {
        Ok(stdout) => stdout,
        Err(err) => {
            return Ok(case_row(
                case,
                json!({"status": "failed", "seconds": round1(seconds), "error": first_line(&err)}),
            ));
        }
    };
    let report: Value =
        serde_json::from_str(&stdout).map_err(|err| format!("ripr output is not JSON: {err}"))?;
    Ok(case_row(case, metrics_of(&report, &diff_files, seconds)))
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").to_string()
}

/// Counts read from one `ripr check --format json` report. `diff_test_files`
/// are changed files that some listed related test lives in; a diff that adds a
/// test for its change should have it named. `duplicate_test_rows` counts
/// tests listed more than once for one finding.
fn metrics_of(report: &Value, diff_files: &BTreeSet<String>, seconds: f64) -> Value {
    let summary = &report["summary"];
    let findings = report["findings"].as_array().cloned().unwrap_or_default();
    let mut base_deleted = 0usize;
    let mut related_rows = 0usize;
    let mut duplicate_rows = 0usize;
    let mut not_credited_rows = 0usize;
    let mut findings_naming_diff_test = 0usize;
    let mut diff_test_files = BTreeSet::new();
    for finding in &findings {
        if finding["source_currentness"] == "base_deleted" {
            base_deleted += 1;
        }
        let mut per_test = BTreeMap::<(String, String, u64), usize>::new();
        let mut names_diff_test = false;
        for test in finding["related_tests"].as_array().into_iter().flatten() {
            related_rows += 1;
            let file = test["file"].as_str().unwrap_or("").trim_start_matches("./");
            let key = (
                file.to_string(),
                test["name"].as_str().unwrap_or("").to_string(),
                test["line"].as_u64().unwrap_or(0),
            );
            *per_test.entry(key).or_default() += 1;
            if diff_files.contains(file) {
                names_diff_test = true;
                diff_test_files.insert(file.to_string());
            }
            if test["miss"] == "assertion_not_credited" {
                not_credited_rows += 1;
            }
        }
        duplicate_rows += per_test.values().map(|rows| rows - 1).sum::<usize>();
        findings_naming_diff_test += usize::from(names_diff_test);
    }
    json!({
        "status": "ok",
        "seconds": round1(seconds),
        "analysis_outcome": report["analysis_outcome"]["outcome"]["kind"],
        "probes": summary["probes"],
        "findings": findings.len(),
        "classes": {
            "exposed": summary["exposed"],
            "weakly_exposed": summary["weakly_exposed"],
            "reachable_unrevealed": summary["reachable_unrevealed"],
            "no_static_path": summary["no_static_path"],
            "infection_unknown": summary["infection_unknown"],
            "propagation_unknown": summary["propagation_unknown"],
            "static_unknown": summary["static_unknown"],
        },
        "base_deleted_findings": base_deleted,
        "related_test_rows": related_rows,
        "duplicate_test_rows": duplicate_rows,
        "not_credited_rows": not_credited_rows,
        "findings_naming_a_diff_file_test": findings_naming_diff_test,
        "diff_files_with_named_tests": diff_test_files.len(),
    })
}

fn build_report(manifest: &Manifest, rows: &[Value]) -> Value {
    let ok: Vec<&Value> = rows.iter().filter(|row| row["status"] == "ok").collect();
    let total = |key: &str| -> u64 { ok.iter().filter_map(|row| row[key].as_u64()).sum() };
    let seconds: f64 = ok.iter().filter_map(|row| row["seconds"].as_f64()).sum();
    json!({
        "schema_version": REPORT_SCHEMA_VERSION,
        "corpus_version": manifest.corpus_version,
        "limits": manifest.limits,
        "cases": rows.len(),
        "cases_ok": ok.len(),
        "totals": {
            "seconds": round1(seconds),
            "findings": total("findings"),
            "base_deleted_findings": total("base_deleted_findings"),
            "related_test_rows": total("related_test_rows"),
            "duplicate_test_rows": total("duplicate_test_rows"),
            "not_credited_rows": total("not_credited_rows"),
            "findings_naming_a_diff_file_test": total("findings_naming_a_diff_file_test"),
        },
        "rows": rows,
    })
}

fn write_report(report: &Value) -> Result<(), String> {
    let json_text = serde_json::to_string_pretty(report).map_err(|err| err.to_string())?;
    for (path, text) in [
        (REPORT_JSON, format!("{json_text}\n")),
        (REPORT_MD, markdown(report)),
    ] {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("create {}: {err}", parent.display()))?;
        }
        fs::write(path, text).map_err(|err| format!("write {path}: {err}"))?;
    }
    Ok(())
}

fn markdown(report: &Value) -> String {
    let mut out = String::from("# Real-crate dogfood\n\n");
    out.push_str(&format!(
        "Corpus {}: {} of {} cases ran. Counts are static-analysis outputs, not verdicts.\n\n",
        report["corpus_version"].as_str().unwrap_or(""),
        report["cases_ok"],
        report["cases"]
    ));
    out.push_str(
        "| case | status | secs | findings | base-deleted | test rows | duplicate rows | not credited | names a diff-file test |\n|---|---|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    for row in report["rows"].as_array().into_iter().flatten() {
        let cell = |key: &str| match &row[key] {
            Value::Null => "-".to_string(),
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            cell("id"),
            cell("status"),
            cell("seconds"),
            cell("findings"),
            cell("base_deleted_findings"),
            cell("related_test_rows"),
            cell("duplicate_test_rows"),
            cell("not_credited_rows"),
            cell("findings_naming_a_diff_file_test"),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(id: &str, revision: &str) -> Case {
        Case {
            id: id.to_string(),
            krate: "demo".to_string(),
            url: "https://example.invalid/demo".to_string(),
            revision: revision.to_string(),
            note: String::new(),
        }
    }

    fn manifest(cases: Vec<Case>) -> Manifest {
        Manifest {
            schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
            corpus_version: "t".to_string(),
            description: "d".to_string(),
            limits: vec!["l".to_string()],
            timeout_secs: 1,
            cases,
        }
    }

    #[test]
    fn the_checked_in_manifest_validates() -> Result<(), String> {
        let text = fs::read_to_string(DEFAULT_MANIFEST)
            .or_else(|_| fs::read_to_string(format!("../{DEFAULT_MANIFEST}")))
            .map_err(|err| err.to_string())?;
        let manifest: Manifest = serde_json::from_str(&text).map_err(|err| err.to_string())?;
        validate_manifest(&manifest)
    }

    #[test]
    fn validation_rejects_short_revisions_and_repeated_ids() {
        let good = "a".repeat(40);
        assert!(validate_manifest(&manifest(vec![case("a-1", &good)])).is_ok());
        assert!(validate_manifest(&manifest(vec![case("a-1", "abc1234")])).is_err());
        assert!(
            validate_manifest(&manifest(vec![case("a-1", &good), case("a-1", &good)])).is_err()
        );
        assert!(validate_manifest(&manifest(vec![case("A B", &good)])).is_err());
        assert!(validate_manifest(&manifest(vec![])).is_err());
    }

    #[test]
    fn metrics_count_duplicate_rows_base_deleted_and_diff_file_tests() {
        let report = json!({
            "summary": {"probes": 2, "exposed": 0, "weakly_exposed": 1, "reachable_unrevealed": 0,
                "no_static_path": 0, "infection_unknown": 0, "propagation_unknown": 0, "static_unknown": 1},
            "analysis_outcome": {"outcome": {"kind": "complete_with_findings"}},
            "findings": [
                {"source_currentness": "candidate_current", "related_tests": [
                    {"file": "tests/t.rs", "name": "a", "line": 3, "miss": null},
                    {"file": "tests/t.rs", "name": "a", "line": 3, "miss": "weak_assertion"},
                    {"file": "tests/other.rs", "name": "b", "line": 1, "miss": "assertion_not_credited"}]},
                {"source_currentness": "base_deleted", "related_tests": []}
            ]
        });
        let diff = BTreeSet::from(["tests/t.rs".to_string()]);
        let metrics = metrics_of(&report, &diff, 1.25);
        assert_eq!(metrics["findings"], 2);
        assert_eq!(metrics["base_deleted_findings"], 1);
        assert_eq!(metrics["related_test_rows"], 3);
        assert_eq!(metrics["duplicate_test_rows"], 1);
        assert_eq!(metrics["not_credited_rows"], 1);
        assert_eq!(metrics["findings_naming_a_diff_file_test"], 1);
        assert_eq!(metrics["diff_files_with_named_tests"], 1);
        assert_eq!(metrics["classes"]["static_unknown"], 1);
    }

    #[test]
    fn fetch_requires_network_opt_in_and_run_requires_a_binary() {
        assert!(parse_options("fetch", &[]).is_err());
        assert!(parse_options("fetch", &["--allow-network".to_string()]).is_ok());
        assert!(parse_options("run", &[]).is_err());
        assert!(parse_options("run", &["--ripr".to_string(), "x".to_string()]).is_ok());
        assert!(parse_options("check", &["--bogus".to_string()]).is_err());
    }
}
