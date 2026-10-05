//! `cargo xtask mutation-spot-check` — ground-truth spot check of static grip
//! verdicts against real cargo-mutants outcomes on real repositories.
//!
//! For each supplied checkout the command writes a `repo-exposure-json`
//! snapshot, obtains a cargo-mutants `mutants.out` directory (supplied, or run
//! with `--run-mutants`), and joins the two through the product
//! `ripr calibrate cargo-mutants` path so the join stays owned by one
//! implementation. Every runtime record then gets one disposition:
//!
//! - `canonical_precise`: a `BinaryOperator` or `UnaryOperator` mutant that
//!   the calibration joined by `seam_id` or by unique `span_containment` to a
//!   `predicate_boundary` or `return_value` seam, with a `caught` or `missed`
//!   outcome. The mutated range lies inside the seam, so its outcome tests the
//!   seam's verdict. Only these records are scored.
//! - an exclusion reason for everything else: `file_line_only` (the
//!   calibration's line fallback, compatibility evidence rather than a precise
//!   pairing), `ambiguous_span_overlap`, `ambiguous_file_line`,
//!   `unmatched_<reason>`, `unsupported_genre` (including `FnValue`
//!   function-body mutants), `unknown_genre` (absent from `mutants.json`),
//!   `unsupported_seam_kind`, `unscoreable_outcome`, or `unknown_join_method`.
//!
//! Scored verdict families: `strongly_gripped` claims a discriminator exists
//! (a caught mutant agrees, a missed one is an overclaim); `ungripped` and
//! `reachable_unrevealed` claim none does (a missed mutant agrees, a caught one
//! is a false gap). `weakly_gripped` and the unknown classes do not make a
//! claim a single operator mutant can settle, so their outcomes are counted
//! but not scored. Claims are limited to the recorded checkout revisions, the
//! cargo-mutants version that produced the outcomes, and this join rule.
//!
//! v1 paired a mutant with a seam when its original operator token appeared
//! in the seam's expression on the same line. v2 keeps that check only as the
//! `operator_token_diagnostic`, which never overrides the canonical join.

use crate::run::{
    capture_bytes_in_dir_with_timeout, capture_output_with_timeout,
    capture_stdout_to_file_with_timeout,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SCHEMA_VERSION: &str = "ripr-mutation-spot-check-v2";
/// Calibration schema that carries the span-containment join and explicit
/// span-overlap and unmatched reasons.
const CALIBRATION_SCHEMA_VERSION: &str = "0.2";
const CANONICAL_PRECISE: &str = "canonical_precise";
const CANONICAL_JOIN_METHODS: &[&str] = &["seam_id", "span_containment"];
const SUPPORTED_GENRES: &[&str] = &["BinaryOperator", "UnaryOperator"];
const SUPPORTED_SEAM_KINDS: &[&str] = &["predicate_boundary", "return_value"];
const SCOREABLE_OUTCOMES: &[&str] = &["caught", "missed"];
const DEFAULT_JOBS: usize = 2;
const DEFAULT_MUTANT_TIMEOUT_SECS: u64 = 60;
const DEFAULT_EXAMPLES: usize = 10;
const EXPOSURE_TIMEOUT: Duration = Duration::from_mins(15);
const CALIBRATE_TIMEOUT: Duration = Duration::from_mins(5);
const MUTANTS_TIMEOUT: Duration = Duration::from_hours(4);
const SCRATCH: &str = "target/ripr/reports/mutation-spot-check";
/// cargo-mutants options the harness sets itself, that would mutate a tree
/// other than the analyzed checkout, or that stop cargo-mutants from writing
/// outcomes. `--mutants-arg` is for selecting mutants.
const HARNESS_OWNED_MUTANTS_ARGS: &[&str] = &[
    "--dir",
    "--output",
    "--jobs",
    "--timeout",
    // The harness always passes --timeout: cargo-mutants rejects the
    // multiplier alongside it and ignores the minimum, so neither can apply.
    "--timeout-multiplier",
    "--minimum-test-timeout",
    "--manifest-path",
    "--shuffle",
    "--no-shuffle",
    "--in-place",
    "--list",
    "--list-files",
    "--check",
    "--json",
    "--completions",
    "--emit-schema",
    "--version",
];
/// Short forms of `--dir`, `--output`, `--jobs` and `--timeout`.
const HARNESS_OWNED_SHORT_FLAGS: &[char] = &['d', 'o', 'j', 't'];
/// cargo-mutants 27 short flags that take no value (`--caught`, `--unviable`,
/// `--help`), so a bundled argument such as `-vj8` continues past them.
const VALUELESS_SHORT_FLAGS: &[char] = &['v', 'V', 'h'];
const USAGE: &str = "usage: cargo xtask mutation-spot-check --repo <name>=<checkout> [--repo ...] [--mutants-out <name>=<mutants.out dir>] [--run-mutants] [--mutants-arg <name>=<arg>] [--jobs <n>] [--mutant-timeout-secs <n>] [--examples <n>] [--ripr <binary>]";

fn harness_owned_mutants_arg(arg: &str) -> bool {
    if let Some(bundle) = arg.strip_prefix('-').filter(|rest| !rest.starts_with('-')) {
        // clap reads `-vj8` as `-v -j 8`: scan valueless flags until the
        // first flag that takes a value, whose remainder is that value.
        for flag in bundle.chars() {
            if HARNESS_OWNED_SHORT_FLAGS.contains(&flag) {
                return true;
            }
            if !VALUELESS_SHORT_FLAGS.contains(&flag) {
                return false;
            }
        }
        return false;
    }
    HARNESS_OWNED_MUTANTS_ARGS.iter().any(|owned| {
        arg == *owned
            || arg
                .strip_prefix(owned)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

pub(crate) fn mutation_spot_check(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_options(args)?;
    let binary = match &options.ripr {
        Some(path) => path.clone(),
        None => {
            crate::run::run("cargo", &["build", "-p", "ripr"])?;
            crate::ripr_debug_binary()
        }
    };
    for (name, checkout) in &options.repos {
        reject_checkout_inside_workspace(name, checkout)?;
    }
    let scratch = PathBuf::from(SCRATCH);
    fs::create_dir_all(&scratch)
        .map_err(|err| format!("create spot-check scratch {}: {err}", scratch.display()))?;

    let mut repos = Vec::new();
    for (name, checkout) in &options.repos {
        repos.push(spot_check_repo(
            &binary, &scratch, name, checkout, &options,
        )?);
    }
    let report = build_report(&repos, options.examples);
    let json_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("serialize mutation spot check: {err}"))?;
    crate::write_report("mutation-spot-check.json", &format!("{json_text}\n"))?;
    crate::write_report("mutation-spot-check.md", &spot_check_markdown(&report))?;
    println!("Wrote target/ripr/reports/mutation-spot-check.json");
    println!("Wrote target/ripr/reports/mutation-spot-check.md");
    Ok(())
}

#[derive(Clone, Debug)]
struct Options {
    repos: Vec<(String, PathBuf)>,
    mutants_out: BTreeMap<String, PathBuf>,
    mutants_args: BTreeMap<String, Vec<String>>,
    run_mutants: bool,
    jobs: usize,
    mutant_timeout_secs: u64,
    examples: usize,
    ripr: Option<PathBuf>,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        repos: Vec::new(),
        mutants_out: BTreeMap::new(),
        mutants_args: BTreeMap::new(),
        run_mutants: false,
        jobs: DEFAULT_JOBS,
        mutant_timeout_secs: DEFAULT_MUTANT_TIMEOUT_SECS,
        examples: DEFAULT_EXAMPLES,
        ripr: None,
    };
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--repo" | "--mutants-out" => {
                index += 1;
                let (name, path) = parse_named_path(required_arg(args, index, flag)?, flag)?;
                if flag == "--repo" {
                    if options.repos.iter().any(|(existing, _)| *existing == name) {
                        return Err(format!("duplicate --repo name `{name}`"));
                    }
                    options.repos.push((name, path));
                } else if options.mutants_out.insert(name.clone(), path).is_some() {
                    return Err(format!("duplicate --mutants-out name `{name}`"));
                }
            }
            "--mutants-arg" => {
                index += 1;
                let value = required_arg(args, index, flag)?;
                let Some((name, arg)) = value
                    .split_once('=')
                    .filter(|(name, arg)| !name.trim().is_empty() && !arg.trim().is_empty())
                else {
                    return Err(format!("--mutants-arg expects <name>=<arg>, got `{value}`"));
                };
                if harness_owned_mutants_arg(arg) {
                    return Err(format!(
                        "--mutants-arg `{arg}` would override how the harness runs cargo-mutants; use the spot-check options (--jobs, --mutant-timeout-secs) or select mutants with --file, --re, --exclude, --package or --workspace"
                    ));
                }
                options
                    .mutants_args
                    .entry(name.trim().to_string())
                    .or_default()
                    .push(arg.to_string());
            }
            "--run-mutants" => options.run_mutants = true,
            "--jobs" => {
                index += 1;
                options.jobs = parse_positive(required_arg(args, index, flag)?, flag)? as usize;
            }
            "--mutant-timeout-secs" => {
                index += 1;
                options.mutant_timeout_secs =
                    parse_positive(required_arg(args, index, flag)?, flag)?;
            }
            "--examples" => {
                index += 1;
                options.examples = parse_positive(required_arg(args, index, flag)?, flag)? as usize;
            }
            "--ripr" => {
                index += 1;
                options.ripr = Some(PathBuf::from(required_arg(args, index, flag)?));
            }
            other => {
                return Err(format!(
                    "unknown mutation-spot-check argument `{other}`; {USAGE}"
                ));
            }
        }
        index += 1;
    }
    if options.repos.is_empty() {
        return Err(format!(
            "mutation-spot-check needs at least one --repo; {USAGE}"
        ));
    }
    for name in options.mutants_out.keys() {
        if !options.repos.iter().any(|(repo, _)| repo == name) {
            return Err(format!("--mutants-out `{name}` names no --repo"));
        }
    }
    for name in options.mutants_args.keys() {
        if !options.repos.iter().any(|(repo, _)| repo == name) {
            return Err(format!("--mutants-arg `{name}` names no --repo"));
        }
        if options.mutants_out.contains_key(name) || !options.run_mutants {
            return Err(format!(
                "--mutants-arg `{name}` only applies when --run-mutants produces that repo's outcomes"
            ));
        }
    }
    Ok(options)
}

fn parse_named_path(value: &str, flag: &str) -> Result<(String, PathBuf), String> {
    match value.split_once('=') {
        Some((name, path)) if is_repo_name(name.trim()) && !path.trim().is_empty() => {
            Ok((name.trim().to_string(), PathBuf::from(path.trim())))
        }
        _ => Err(format!(
            "{flag} expects <name>=<path> with a name of letters, digits, `.`, `_` or `-` (it names files under {SCRATCH}), got `{value}`"
        )),
    }
}

/// Repo names become scratch file names, so they may not carry separators.
fn is_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn parse_positive(value: &str, flag: &str) -> Result<u64, String> {
    match value.parse::<u64>() {
        Ok(parsed) if parsed > 0 => Ok(parsed),
        _ => Err(format!("{flag} must be a positive integer, got `{value}`")),
    }
}

fn required_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    match args.get(index) {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("missing value for {flag}; {USAGE}")),
    }
}

struct RepoRun {
    name: String,
    revision: String,
    diffs_checked: usize,
    exposure_run_status: Option<String>,
    /// `None` for a supplied `mutants.out`: its run's arguments are not
    /// recorded anywhere this harness can read.
    cargo_mutants_args: Option<Vec<String>>,
    cargo_mutants_version: Option<String>,
    mutant_set_sha256: String,
    metrics: Value,
    records: Vec<Record>,
}

fn spot_check_repo(
    binary: &Path,
    scratch: &Path,
    name: &str,
    checkout: &Path,
    options: &Options,
) -> Result<RepoRun, String> {
    let revision = run_text(
        "git",
        &[
            "-C".to_string(),
            path_arg(checkout),
            "rev-parse".to_string(),
            "HEAD".to_string(),
        ],
        Duration::from_secs(30),
        "git rev-parse for spot-check checkout",
    )?
    .trim()
    .to_string();

    let exposure_path = scratch.join(format!("{name}.repo-exposure.json"));
    let exposure = capture_stdout_to_file_with_timeout(
        &path_arg(binary),
        &[
            "check".to_string(),
            "--root".to_string(),
            path_arg(checkout),
            "--format".to_string(),
            "repo-exposure-json".to_string(),
        ],
        &[],
        &exposure_path,
        EXPOSURE_TIMEOUT,
        "ripr repo-exposure-json for spot check",
    )?;
    if exposure.timed_out || !exposure.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "ripr repo-exposure-json failed for `{name}`: {}",
            exposure.stderr.trim()
        ));
    }
    let exposure_json = read_json(&exposure_path)?;

    let mutants_dir = match options.mutants_out.get(name) {
        Some(dir) => dir.clone(),
        None if options.run_mutants => run_cargo_mutants(scratch, name, checkout, options)?,
        None => {
            return Err(format!(
                "no cargo-mutants outcomes for `{name}`: pass --mutants-out {name}=<dir> or --run-mutants"
            ));
        }
    };
    let read_mutants_out = |file: &str| {
        read_json(&mutants_dir.join(file)).map_err(|err| {
            format!(
                "{err}\nexpected a cargo-mutants mutants.out directory for `{name}` containing outcomes.json and mutants.json"
            )
        })
    };
    let outcomes = read_mutants_out("outcomes.json")?;
    let mutant_records = read_mutants_out("mutants.json")?;
    let diffs_checked = require_mutants_match_checkout(name, checkout, &revision, &mutant_records)?;

    let calibration = run_text(
        &path_arg(binary),
        &[
            "calibrate".to_string(),
            "cargo-mutants".to_string(),
            "--mutants-json".to_string(),
            path_arg(&mutants_dir),
            "--repo-exposure-json".to_string(),
            path_arg(&exposure_path),
            "--format".to_string(),
            "json".to_string(),
        ],
        CALIBRATE_TIMEOUT,
        "ripr calibrate cargo-mutants for spot check",
    )?;
    let calibration: Value = serde_json::from_str(&calibration)
        .map_err(|err| format!("parse calibration JSON for `{name}`: {err}"))?;

    Ok(RepoRun {
        name: name.to_string(),
        revision,
        diffs_checked,
        exposure_run_status: exposure_json
            .get("run_status")
            .and_then(Value::as_str)
            .map(str::to_string),
        cargo_mutants_args: if options.mutants_out.contains_key(name) {
            None
        } else {
            Some(options.mutants_args.get(name).cloned().unwrap_or_default())
        },
        mutant_set_sha256: mutant_set_sha256(&mutant_records),
        cargo_mutants_version: outcomes
            .get("cargo_mutants_version")
            .and_then(Value::as_str)
            .map(str::to_string),
        metrics: calibration.get("metrics").cloned().unwrap_or(Value::Null),
        records: classify_records(
            &calibration,
            &exposure_json,
            &mutant_genres(&mutant_records),
        )
        .map_err(|err| format!("`{name}`: {err}"))?,
    })
}

/// cargo-mutants records no source revision, so a supplied `mutants.out` from
/// another commit would join stale outcomes to this checkout's seams. Every
/// mutant diff carries the original lines it replaced; they must still match
/// the checkout, or nothing from this directory is scored.
fn require_mutants_match_checkout(
    name: &str,
    checkout: &Path,
    revision: &str,
    mutant_records: &Value,
) -> Result<usize, String> {
    let records = match mutant_records.as_array() {
        Some(records) if !records.is_empty() => records,
        _ => {
            return Err(format!(
                "mutants.json for `{name}` is not a non-empty array of mutants, so nothing can be checked against the checkout at {revision}. Pass the mutants.out directory of a cargo-mutants 27 or later run that found mutants."
            ));
        }
    };
    check_mutant_diffs(records, |file| {
        fs::read_to_string(checkout.join(file))
            .ok()
            .map(|text| source_lines(&text))
    })
    .map_err(|problem| {
        format!("mutants.out for `{name}` does not match the checkout at {revision}: {problem}")
    })
}

/// Checks every mutant record's diff against the source `read_source`
/// returns, and returns how many were checked. A record without a diff
/// (cargo-mutants before 27 omits it) cannot be checked, so it refuses the
/// directory rather than scoring unverified outcomes.
fn check_mutant_diffs(
    records: &[Value],
    mut read_source: impl FnMut(&str) -> Option<Vec<String>>,
) -> Result<usize, String> {
    let mut sources: BTreeMap<String, Option<Vec<String>>> = BTreeMap::new();
    let mut stale = Vec::new();
    let mut missing_files = BTreeSet::new();
    for record in records {
        let (Some(file), Some(diff)) = (
            record.get("file").and_then(Value::as_str),
            record.get("diff").and_then(Value::as_str),
        ) else {
            return Err(
                "mutants.json has a mutant without `file` or `diff`, so its source revision cannot be checked. Re-run with cargo-mutants 27 or later, which records each mutant's diff.".to_string(),
            );
        };
        let lines = sources
            .entry(file.to_string())
            .or_insert_with(|| read_source(file));
        match lines {
            None => {
                missing_files.insert(file.to_string());
            }
            Some(lines) => {
                if let Some(line) = first_stale_line(diff, lines) {
                    stale.push(format!("{file}:{line}"));
                }
            }
        }
    }
    if let Some(file) = missing_files.first() {
        return Err(format!(
            "{} mutated file(s) are missing from the checkout, first `{file}`. Pass the checkout root cargo-mutants ran in.",
            missing_files.len()
        ));
    }
    if let Some(first) = stale.first() {
        return Err(format!(
            "{} of {} mutant diffs no longer apply (first at {first}). Check out the revision cargo-mutants ran on, or re-run cargo mutants on this one.",
            stale.len(),
            records.len()
        ));
    }
    Ok(records.len())
}

/// Splits source the way cargo-mutants' diff library counts lines: `\n`,
/// `\r\n`, and a bare `\r` each end a line, so hunk line numbers agree.
fn source_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Returns the first source line where a mutant diff's context or removed
/// lines differ from `source`, or `None` when the whole diff still applies.
fn first_stale_line(diff: &str, source: &[String]) -> Option<usize> {
    let mut cursor: Option<usize> = None;
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("@@ -") {
            let start = header
                .split([',', ' '])
                .next()
                .and_then(|value| value.parse::<usize>().ok());
            match start {
                Some(start) => cursor = Some(start),
                None => return Some(0),
            }
            continue;
        }
        let Some(current) = cursor else {
            continue;
        };
        let original = match line.chars().next() {
            Some(' ') | Some('-') => line.get(1..).unwrap_or_default(),
            None => "",
            Some(_) => continue,
        };
        if source.get(current.wrapping_sub(1)).map(String::as_str) != Some(original) {
            return Some(current);
        }
        cursor = Some(current + 1);
    }
    None
}

/// A checkout under this workspace inherits its Cargo workspace, so neither
/// the baseline build nor ripr's root resolution would see the crate alone.
fn reject_checkout_inside_workspace(name: &str, checkout: &Path) -> Result<(), String> {
    let checkout = fs::canonicalize(checkout).map_err(|err| {
        format!(
            "--repo {name}: cannot resolve {}: {err}",
            checkout.display()
        )
    })?;
    let workspace = std::env::current_dir()
        .and_then(fs::canonicalize)
        .map_err(|err| format!("resolve current directory: {err}"))?;
    if checkout.starts_with(&workspace) {
        return Err(format!(
            "--repo {name}: {} is inside this workspace; clone spot-check repos outside it",
            checkout.display()
        ));
    }
    Ok(())
}

fn run_cargo_mutants(
    scratch: &Path,
    name: &str,
    checkout: &Path,
    options: &Options,
) -> Result<PathBuf, String> {
    let output_root = scratch.join(format!("{name}-mutants"));
    let _ = fs::remove_dir_all(&output_root);
    fs::create_dir_all(&output_root)
        .map_err(|err| format!("create {}: {err}", output_root.display()))?;
    // The child runs from the checkout, so hand it absolute paths.
    let output_root = fs::canonicalize(&output_root)
        .map_err(|err| format!("resolve {}: {err}", output_root.display()))?;
    let checkout = &fs::canonicalize(checkout)
        .map_err(|err| format!("resolve checkout {}: {err}", checkout.display()))?;
    // The repository's `.cargo/config.toml` forces TMPDIR into this
    // workspace's `target/`. cargo-mutants copies the crate under TMPDIR, and
    // a copy inside this workspace fails its baseline build with "believes
    // it's in a workspace when it's not". Put the copies beside the checkout.
    let temp_root = checkout
        .parent()
        .unwrap_or(checkout)
        .join(format!(".ripr-spot-check-tmp-{name}"));
    fs::create_dir_all(&temp_root)
        .map_err(|err| format!("create {}: {err}", temp_root.display()))?;
    let temp_dir = path_arg(&temp_root);
    // Run from the checkout with the toolchain selectors that `cargo xtask`
    // exports removed, so rustup picks the subject's own toolchain (its
    // rust-toolchain file, else the default) instead of this repository's pin.
    let output = capture_bytes_in_dir_with_timeout(
        Path::new("cargo"),
        &[
            "mutants".to_string(),
            "--dir".to_string(),
            path_arg(checkout),
            "--output".to_string(),
            path_arg(&output_root),
            "--jobs".to_string(),
            options.jobs.to_string(),
            "--timeout".to_string(),
            options.mutant_timeout_secs.to_string(),
            "--no-shuffle".to_string(),
        ]
        .into_iter()
        .chain(
            options
                .mutants_args
                .get(name)
                .into_iter()
                .flatten()
                .cloned(),
        )
        .collect::<Vec<_>>(),
        checkout,
        &[
            ("TMPDIR", &temp_dir),
            ("TMP", &temp_dir),
            ("TEMP", &temp_dir),
        ],
        &["RUSTUP_TOOLCHAIN", "CARGO"],
        MUTANTS_TIMEOUT,
        "cargo mutants for spot check",
    );
    let _ = fs::remove_dir_all(&temp_root);
    let output = output?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    // cargo-mutants exits 2 when mutants were missed and 3 when some timed
    // out; both are results. Any other non-zero exit (1 usage, 4 baseline
    // failure) means there is no outcome set to score.
    let code = output.status.and_then(|status| status.code());
    if output.timed_out || !matches!(code, Some(0 | 2 | 3)) {
        return Err(format!(
            "cargo mutants failed for `{name}` (exit {code:?}); check that cargo-mutants is installed and that `cargo test` passes in {}; see {}/mutants.out/log/baseline.log\n{}",
            checkout.display(),
            output_root.display(),
            stderr.trim()
        ));
    }
    let mutants_out = output_root.join("mutants.out");
    if !mutants_out.join("outcomes.json").exists() {
        return Err(format!(
            "cargo mutants found no mutants for `{name}`, so there is nothing to score; for a multi-crate workspace, run `cargo mutants --workspace` yourself and pass its output with --mutants-out {name}=<dir>\n{}",
            stderr.trim()
        ));
    }
    Ok(mutants_out)
}

fn run_text(
    program: &str,
    args: &[String],
    timeout: Duration,
    context: &str,
) -> Result<String, String> {
    let output = capture_output_with_timeout(program, args, &[], timeout, context)?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!("{context} failed: {}", output.stderr.trim()));
    }
    Ok(output.stdout)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    serde_json::from_str(&text).map_err(|err| format!("parse {}: {err}", path.display()))
}

fn path_arg(path: &Path) -> String {
    path.display().to_string()
}

/// One runtime record from the calibration join and the harness's
/// disposition for it: `canonical_precise` when it can be scored, otherwise
/// the explicit exclusion reason.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Record {
    disposition: String,
    join_method: String,
    grip_class: String,
    outcome: String,
    seam_id: String,
    seam_kind: String,
    file: String,
    line: u64,
    expression: Option<String>,
    mutant: String,
    operator_token: &'static str,
}

/// cargo-mutants genre per mutant name, from `mutants.out/mutants.json`.
/// Digest of the sorted mutant names cargo-mutants generated. The set is what
/// the rates are computed over, and it reflects selection arguments
/// (`--re`, `--exclude`, `--workspace`) even when a supplied run did not
/// record them, so the scoreboard keys comparability on it.
fn mutant_set_sha256(mutants: &Value) -> String {
    let mut names = mutants
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|mutant| mutant.get("name")?.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    let mut digest = Sha256::new();
    for name in names {
        digest.update(name.as_bytes());
        digest.update(b"\n");
    }
    format!("{:x}", digest.finalize())
}

fn mutant_genres(mutants: &Value) -> BTreeMap<String, String> {
    mutants
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|mutant| {
            Some((
                mutant.get("name")?.as_str()?.to_string(),
                mutant.get("genre")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

/// Give every runtime record in the calibration report one disposition. The
/// join itself stays owned by `ripr calibrate cargo-mutants`; the harness only
/// decides which joined records it can score. Every record is accounted for,
/// so the dispositions always sum to the calibration's `mutants_total`.
fn classify_records(
    calibration: &Value,
    exposure: &Value,
    genres: &BTreeMap<String, String>,
) -> Result<Vec<Record>, String> {
    let schema = calibration
        .get("schema_version")
        .and_then(Value::as_str)
        .unwrap_or("missing");
    if schema != CALIBRATION_SCHEMA_VERSION {
        return Err(format!(
            "mutation calibration schema {schema} has no span-containment join; the spot check needs calibration schema {CALIBRATION_SCHEMA_VERSION}"
        ));
    }
    let expressions: BTreeMap<&str, &str> = exposure
        .get("seams")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|seam| {
            Some((
                seam.get("seam_id")?.as_str()?,
                seam.get("expression")?.as_str()?,
            ))
        })
        .collect();
    let text = |value: &Value, pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let list = |key: &str| {
        calibration
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
    };
    let mut records = Vec::new();
    for record in list("matches") {
        let mutant = text(record, "/runtime/mutant_id");
        let join_method = text(record, "/join_method");
        let seam_id = text(record, "/static/seam_id");
        let seam_kind = text(record, "/static/seam_kind");
        let outcome = text(record, "/runtime/runtime_outcome");
        let genre = genres.get(&mutant).map_or("", String::as_str);
        let expression = expressions
            .get(seam_id.as_str())
            .map(|expression| (*expression).to_string());
        let disposition = match_disposition(&join_method, genre, &seam_kind, &outcome);
        let operator_token = if disposition == CANONICAL_PRECISE {
            operator_token_diagnostic(&mutant, expression.as_deref())
        } else {
            "not_applicable"
        };
        records.push(Record {
            disposition: disposition.to_string(),
            join_method,
            grip_class: text(record, "/static/seam_grip_class"),
            outcome,
            seam_id,
            seam_kind,
            file: text(record, "/static/file"),
            line: record
                .pointer("/static/line")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            expression,
            mutant,
            operator_token,
        });
    }
    let unjoined = |runtime: &Value, disposition: String| Record {
        disposition,
        join_method: String::new(),
        grip_class: String::new(),
        outcome: text(runtime, "/runtime_outcome"),
        seam_id: String::new(),
        seam_kind: String::new(),
        file: text(runtime, "/file"),
        line: runtime.get("line").and_then(Value::as_u64).unwrap_or(0),
        expression: None,
        mutant: text(runtime, "/mutant_id"),
        operator_token: "not_applicable",
    };
    for (key, disposition) in [
        ("ambiguous_file_line_matches", "ambiguous_file_line"),
        ("ambiguous_span_overlap_matches", "ambiguous_span_overlap"),
    ] {
        for record in list(key) {
            let runtime = record.get("runtime").unwrap_or(&Value::Null);
            records.push(unjoined(runtime, disposition.to_string()));
        }
    }
    for record in list("unmatched_mutants") {
        let reason = record
            .get("unmatched_reason")
            .and_then(Value::as_str)
            .unwrap_or("unknown_reason");
        records.push(unjoined(record, format!("unmatched_{reason}")));
    }
    let total = calibration
        .pointer("/metrics/mutants_total")
        .and_then(Value::as_u64);
    if total != Some(records.len() as u64) {
        return Err(format!(
            "mutation calibration lists {} runtime records but reports mutants_total {}; refusing to score a partial join",
            records.len(),
            total.map_or_else(|| "missing".to_string(), |total| total.to_string())
        ));
    }
    Ok(records)
}

/// A matched record is scoreable only when the calibration joined it by an
/// authoritative method and its genre, seam kind and outcome are ones this
/// benchmark scores. A file/line fallback join is compatibility evidence, not
/// a precise pairing, so it is excluded however plausible the pair looks.
fn match_disposition(
    join_method: &str,
    genre: &str,
    seam_kind: &str,
    outcome: &str,
) -> &'static str {
    if join_method == "file_line" {
        "file_line_only"
    } else if !CANONICAL_JOIN_METHODS.contains(&join_method) {
        "unknown_join_method"
    } else if genre.is_empty() {
        // Not in mutants.json: an input problem, not an operator family.
        "unknown_genre"
    } else if !SUPPORTED_GENRES.contains(&genre) {
        "unsupported_genre"
    } else if !SUPPORTED_SEAM_KINDS.contains(&seam_kind) {
        "unsupported_seam_kind"
    } else if !SCOREABLE_OUTCOMES.contains(&outcome) {
        "unscoreable_outcome"
    } else {
        CANONICAL_PRECISE
    }
}

/// Audit diagnostic only: whether the mutant's original operator token
/// appears in the joined seam's serialized expression. v1 used this text
/// match as the pairing rule; v2 records it so a disagreement with the
/// canonical join stays visible, and it never changes a disposition.
fn operator_token_diagnostic(mutant: &str, expression: Option<&str>) -> &'static str {
    let Some(operator) = original_operator(mutant) else {
        return "no_original_operator";
    };
    match expression {
        None | Some("") => "expression_unavailable",
        Some(expression) if contains_operator_token(expression, operator) => "present",
        Some(_) => "absent",
    }
}

/// True when `operator` occurs in `expression` with no adjacent operator
/// character, so `>` does not match inside `>=`, `->`, `=>`, or `>>`, and `!`
/// does not match inside `!=`.
fn contains_operator_token(expression: &str, operator: &str) -> bool {
    const OPERATOR_CHARS: &str = "!%&*+-/<=>^|";
    expression.match_indices(operator).any(|(start, _)| {
        let before = expression[..start].chars().next_back();
        let after = expression[start + operator.len()..].chars().next();
        !before.is_some_and(|ch| OPERATOR_CHARS.contains(ch))
            && !after.is_some_and(|ch| OPERATOR_CHARS.contains(ch))
    })
}

/// The operator a cargo-mutants operator mutant replaces or deletes, from its
/// name: `src/a.rs:3:9: replace > with >= in f` → `>`,
/// `src/a.rs:3:9: delete ! in f` → `!`.
fn original_operator(mutant: &str) -> Option<&str> {
    let (_, description) = mutant.split_once(": ")?;
    let rest = description
        .strip_prefix("replace ")
        .or_else(|| description.strip_prefix("delete "))?;
    let operator = rest.split_whitespace().next()?;
    (!operator.is_empty() && !operator.chars().any(char::is_alphanumeric)).then_some(operator)
}

fn verdict_family(grip_class: &str) -> &'static str {
    match grip_class {
        "strongly_gripped" => "claims_discriminator",
        "ungripped" | "reachable_unrevealed" => "claims_no_discriminator",
        "weakly_gripped" => "weak",
        "intentional" | "suppressed" => "excluded",
        _ => "unknown",
    }
}

/// `agree`, `overclaim` (claimed a discriminator, mutant missed), `false_gap`
/// (claimed none, mutant caught), or `unscored`.
fn agreement(family: &str, outcome: &str) -> &'static str {
    match (family, outcome) {
        ("claims_discriminator", "caught") | ("claims_no_discriminator", "missed") => "agree",
        ("claims_discriminator", "missed") => "overclaim",
        ("claims_no_discriminator", "caught") => "false_gap",
        _ => "unscored",
    }
}

fn build_report(repos: &[RepoRun], examples: usize) -> Value {
    let mut by_disposition: BTreeMap<String, BTreeMap<String, BTreeMap<String, usize>>> =
        BTreeMap::new();
    let mut scored: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    let mut seams_scored: BTreeMap<&str, BTreeSet<(String, String)>> = BTreeMap::new();
    let mut disagreements: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    let mut example_seams = BTreeSet::new();
    let mut operator_tokens: BTreeMap<&str, usize> = BTreeMap::new();
    let mut operator_token_absent = Vec::new();
    for repo in repos {
        for record in &repo.records {
            let grip_class = if record.grip_class.is_empty() {
                "no_joined_seam"
            } else {
                record.grip_class.as_str()
            };
            *by_disposition
                .entry(record.disposition.clone())
                .or_default()
                .entry(grip_class.to_string())
                .or_default()
                .entry(record.outcome.clone())
                .or_default() += 1;
            if record.disposition != CANONICAL_PRECISE {
                continue;
            }
            *operator_tokens.entry(record.operator_token).or_default() += 1;
            if record.operator_token == "absent" && operator_token_absent.len() < examples {
                operator_token_absent.push(example(repo, record));
            }
            let family = verdict_family(&record.grip_class);
            let verdict = agreement(family, &record.outcome);
            if verdict == "unscored" {
                continue;
            }
            *scored
                .entry(family)
                .or_default()
                .entry(verdict)
                .or_default() += 1;
            seams_scored
                .entry(family)
                .or_default()
                .insert((repo.name.clone(), record.seam_id.clone()));
            let list = disagreements.entry(verdict).or_default();
            if verdict != "agree"
                && list.len() < examples
                && example_seams.insert((repo.name.clone(), record.seam_id.clone()))
            {
                list.push(example(repo, record));
            }
        }
    }
    disagreements.remove("agree");
    let families = scored
        .iter()
        .map(|(family, counts)| {
            let agree = counts.get("agree").copied().unwrap_or(0);
            let total: usize = counts.values().sum();
            (
                family.to_string(),
                json!({
                    "mutants_scored": total,
                    "seams_scored": seams_scored.get(family).map_or(0, BTreeSet::len),
                    "counts": counts,
                    "agreement_rate": rate(agree, total),
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "schema_version": SCHEMA_VERSION,
        "status": "advisory",
        "claim_boundary": "Agreement is scored only on canonical_precise records: a BinaryOperator or UnaryOperator mutant that ripr calibrate joined by seam_id or by unique span containment to a predicate_boundary or return_value seam, with a caught or missed outcome. File/line fallback joins, span-overlap ties, unmatched records, and unsupported genres, seam kinds or outcomes are counted as exclusions and never scored. Claims are limited to the recorded checkout revisions, cargo-mutants versions, and this join rule; this is not a suite adequacy measure.",
        "repos": repos.iter().map(|repo| json!({
            "name": repo.name,
            "revision": repo.revision,
            "mutant_diffs_checked_against_revision": repo.diffs_checked,
            "exposure_run_status": repo.exposure_run_status,
            "cargo_mutants_args": repo.cargo_mutants_args,
            "cargo_mutants_version": repo.cargo_mutants_version,
            "mutant_set_sha256": repo.mutant_set_sha256,
            "calibration_metrics": repo.metrics,
            "pairings": pairing_counts(&repo.records),
        })).collect::<Vec<_>>(),
        "outcomes_by_disposition_and_grip_class": by_disposition,
        "scored_families": families,
        "disagreement_examples": disagreements,
        "operator_token_diagnostic": {
            "counts": operator_tokens,
            "absent_examples": operator_token_absent,
        },
    })
}

fn example(repo: &RepoRun, record: &Record) -> Value {
    json!({
        "repo": repo.name,
        "revision": repo.revision,
        "file": record.file,
        "line": record.line,
        "join_method": record.join_method,
        "seam_kind": record.seam_kind,
        "seam_id": record.seam_id,
        "expression": record.expression,
        "grip_class": record.grip_class,
        "mutant": record.mutant,
        "runtime_outcome": record.outcome,
    })
}

/// The denominator stays every runtime record: `canonical_precise` plus every
/// exclusion always equals `records_total`.
fn pairing_counts(records: &[Record]) -> Value {
    let mut by_join_method: BTreeMap<&str, usize> = CANONICAL_JOIN_METHODS
        .iter()
        .map(|method| (*method, 0))
        .collect();
    let mut excluded: BTreeMap<&str, usize> = BTreeMap::new();
    for record in records {
        if record.disposition == CANONICAL_PRECISE {
            *by_join_method
                .entry(record.join_method.as_str())
                .or_default() += 1;
        } else {
            *excluded.entry(record.disposition.as_str()).or_default() += 1;
        }
    }
    json!({
        "records_total": records.len(),
        "canonical_precise": by_join_method.values().sum::<usize>(),
        "canonical_precise_by_join_method": by_join_method,
        "excluded": excluded,
    })
}

fn rate(numerator: usize, denominator: usize) -> Value {
    if denominator == 0 {
        Value::Null
    } else {
        json!(((numerator as f64 / denominator as f64) * 1000.0).round() / 1000.0)
    }
}

fn spot_check_markdown(report: &Value) -> String {
    let mut out = String::from("# Mutation spot check\n\n");
    if let Some(boundary) = report.get("claim_boundary").and_then(Value::as_str) {
        out.push_str(&format!("{boundary}\n\n"));
    }
    let repos = || {
        report
            .get("repos")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
    };
    let name = |repo: &Value| {
        repo.get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    out.push_str("## Repositories\n\n| Repo | Revision | cargo-mutants | Mutants | Canonical precise | By seam ID | By span containment | Excluded |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for repo in repos() {
        let count = |pointer: &str| {
            repo.pointer(&format!("/pairings/{pointer}"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        let excluded: u64 = repo
            .pointer("/pairings/excluded")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(_, count)| count.as_u64())
            .sum();
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {} | {} |\n",
            name(repo),
            repo.get("revision")
                .and_then(Value::as_str)
                .unwrap_or("")
                .chars()
                .take(12)
                .collect::<String>(),
            repo.get("cargo_mutants_version")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            count("records_total"),
            count("canonical_precise"),
            count("canonical_precise_by_join_method/seam_id"),
            count("canonical_precise_by_join_method/span_containment"),
            excluded,
        ));
    }
    for repo in repos() {
        let sampled = repo
            .get("cargo_mutants_args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|arg| format!("`{arg}`"))
            .collect::<Vec<_>>();
        if !sampled.is_empty() {
            out.push_str(&format!(
                "\n{} ran cargo-mutants with {}.\n",
                name(repo),
                sampled.join(" ")
            ));
        }
    }
    out.push_str("\n## Exclusions\n\n| Repo | Reason | Mutants |\n| --- | --- | --- |\n");
    for repo in repos() {
        for (reason, count) in repo
            .pointer("/pairings/excluded")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            out.push_str(&format!(
                "| {} | `{reason}` | {} |\n",
                name(repo),
                count.as_u64().unwrap_or(0)
            ));
        }
    }
    out.push_str("\n## Scored verdicts (canonical precise joins)\n\n| Verdict family | Seams | Mutants | Agree | Overclaim | False gap | Agreement |\n| --- | --- | --- | --- | --- | --- | --- |\n");
    if let Some(families) = report.get("scored_families").and_then(Value::as_object) {
        for (family, row) in families {
            let count = |key: &str| {
                row.pointer(&format!("/counts/{key}"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            };
            out.push_str(&format!(
                "| {family} | {} | {} | {} | {} | {} | {} |\n",
                row.get("seams_scored").and_then(Value::as_u64).unwrap_or(0),
                row.get("mutants_scored")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                count("agree"),
                count("overclaim"),
                count("false_gap"),
                row.get("agreement_rate")
                    .and_then(Value::as_f64)
                    .map_or_else(|| "n/a".to_string(), |rate| format!("{:.1}%", rate * 100.0)),
            ));
        }
    }
    out.push_str("\n## Outcomes by disposition and grip class\n\n| Disposition | Grip class | Outcomes |\n| --- | --- | --- |\n");
    if let Some(dispositions) = report
        .get("outcomes_by_disposition_and_grip_class")
        .and_then(Value::as_object)
    {
        for (disposition, classes) in dispositions {
            for (class, outcomes) in classes.as_object().into_iter().flatten() {
                let cells = outcomes
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(outcome, count)| format!("{outcome} {count}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("| {disposition} | {class} | {cells} |\n"));
            }
        }
    }
    out.push_str("\n## Disagreement examples\n\n");
    let mut any = false;
    for (verdict, rows) in report
        .get("disagreement_examples")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        for row in rows.as_array().into_iter().flatten() {
            any = true;
            out.push_str(&format!("- **{verdict}** {}\n", example_line(row)));
        }
    }
    if !any {
        out.push_str("None in the scored joins.\n");
    }
    out.push_str("\n## Operator-token diagnostic\n\nWhether each canonical precise mutant's original operator appears in the joined seam's expression. Audit only: it never changes a disposition.\n\n");
    let counts = report
        .pointer("/operator_token_diagnostic/counts")
        .and_then(Value::as_object);
    if counts.is_none_or(serde_json::Map::is_empty) {
        out.push_str("No canonical precise joins.\n");
    } else {
        let cells = counts
            .into_iter()
            .flatten()
            .map(|(state, count)| format!("{state} {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("{cells}.\n"));
    }
    for row in report
        .pointer("/operator_token_diagnostic/absent_examples")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        out.push_str(&format!("- **absent** {}\n", example_line(row)));
    }
    out
}

fn example_line(row: &Value) -> String {
    let field = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("");
    format!(
        "{} `{}:{}` {} `{}` is `{}` (joined by {}); mutant `{}` was {}.",
        field("repo"),
        field("file"),
        row.get("line").and_then(Value::as_u64).unwrap_or(0),
        field("seam_kind"),
        // A multi-line seam expression would break the Markdown list item.
        field("expression")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        field("grip_class"),
        field("join_method"),
        field("mutant")
            .split_once(": ")
            .map_or(field("mutant"), |(_, rest)| rest),
        field("runtime_outcome"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutant_diffs_must_match_the_checkout_lines_they_replaced() -> Result<(), String> {
        let diff = "--- src/a.rs\n+++ replace > with < in f\n@@ -2,3 +2,3 @@\n fn f(a: u8) -> bool {\n-    a > 1\n+    a < 1\n }\n";
        let source = |body: &str| -> Vec<String> {
            ["// header", "fn f(a: u8) -> bool {", body, "}"]
                .iter()
                .map(|line| line.to_string())
                .collect()
        };
        assert_eq!(first_stale_line(diff, &source("    a > 1")), None);
        assert_eq!(first_stale_line(diff, &source("    a >= 1")), Some(3));
        assert_eq!(first_stale_line(diff, &source("    a > 1")[..2]), Some(3));

        assert_eq!(
            source_lines("a\r\nb\rc\nd"),
            ["a", "b", "c", "d"].map(String::from)
        );

        let records = [json!({"file": "src/a.rs", "diff": diff})];
        assert_eq!(
            check_mutant_diffs(&records, |_| Some(source("    a > 1"))),
            Ok(1)
        );
        let Err(stale) = check_mutant_diffs(&records, |_| Some(source("    a >= 1"))) else {
            return Err("a changed source line must refuse the directory".to_string());
        };
        assert!(stale.contains("1 of 1 mutant diffs no longer apply (first at src/a.rs:3)"));
        let Err(missing) = check_mutant_diffs(&records, |_| None) else {
            return Err("a missing source file must refuse the directory".to_string());
        };
        assert!(missing.contains("missing from the checkout, first `src/a.rs`"));
        // cargo-mutants 26.x writes mutants.json without diffs.
        let Err(no_diff) = check_mutant_diffs(&[json!({"file": "src/a.rs"})], |_| {
            Some(source("    a > 1"))
        }) else {
            return Err("a mutant without a diff must refuse the directory".to_string());
        };
        assert!(no_diff.contains("cargo-mutants 27 or later"));
        Ok(())
    }

    #[test]
    fn repo_names_cannot_escape_the_scratch_directory() -> Result<(), String> {
        let (name, _) = parse_named_path("semver=../corpus/semver", "--repo")?;
        assert_eq!(name, "semver");
        for bad in ["../x=/tmp/x", "a/b=/tmp/x", "..=/tmp/x", "=/tmp/x"] {
            let Err(err) = parse_named_path(bad, "--repo") else {
                return Err(format!("`{bad}` should be refused"));
            };
            assert!(err.contains("letters, digits"), "{err}");
        }
        Ok(())
    }

    #[test]
    fn mutants_args_pass_through_only_to_a_run_this_harness_starts() -> Result<(), String> {
        let args = |extra: &[&str]| -> Vec<String> {
            ["--repo", "hex=/tmp/hex"]
                .iter()
                .chain(extra)
                .map(|arg| arg.to_string())
                .collect()
        };
        let options = parse_options(&args(&[
            "--run-mutants",
            "--mutants-arg",
            "hex=--file=src/lib.rs",
            "--mutants-arg",
            "hex=--re=decode",
            "--mutants-arg",
            "hex=--jobserver=false",
            "--mutants-arg",
            "hex=-Dx.diff",
            "--mutants-arg",
            "hex=-Fdecode",
            "--mutants-arg",
            "hex=-vfsrc/output.rs",
            "--mutants-arg",
            "hex=-V",
        ]))?;
        assert_eq!(
            options.mutants_args.get("hex"),
            Some(&vec![
                "--file=src/lib.rs".to_string(),
                "--re=decode".to_string(),
                "--jobserver=false".to_string(),
                "-Dx.diff".to_string(),
                "-Fdecode".to_string(),
                "-vfsrc/output.rs".to_string(),
                "-V".to_string()
            ])
        );
        for (extra, expected) in [
            (
                vec!["--mutants-arg", "hex=--re=x"],
                "only applies when --run-mutants",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "other=--re=x"],
                "names no --repo",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex="],
                "expects <name>=<arg>",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex= "],
                "expects <name>=<arg>",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=--in-place"],
                "would override how the harness runs",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=--output=/tmp/x"],
                "would override how the harness runs",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=-j8"],
                "would override how the harness runs",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=-t5"],
                "would override how the harness runs",
            ),
            (
                vec![
                    "--run-mutants",
                    "--mutants-arg",
                    "hex=--timeout-multiplier=2",
                ],
                "would override how the harness runs",
            ),
            (
                vec![
                    "--run-mutants",
                    "--mutants-arg",
                    "hex=--minimum-test-timeout=5",
                ],
                "would override how the harness runs",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=-dfoo"],
                "would override how the harness runs",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=-o=/tmp/x"],
                "would override how the harness runs",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=-vj8"],
                "would override how the harness runs",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=-Vt5"],
                "would override how the harness runs",
            ),
            (
                vec!["--run-mutants", "--mutants-arg", "hex=--list-files"],
                "would override how the harness runs",
            ),
        ] {
            let Err(err) = parse_options(&args(&extra)) else {
                return Err(format!("{extra:?} should be refused"));
            };
            assert!(err.contains(expected), "{err}");
        }
        Ok(())
    }

    #[test]
    fn original_operator_reads_replace_and_delete_names() {
        assert_eq!(
            original_operator("src/a.rs:3:9: replace > with >= in f"),
            Some(">")
        );
        assert_eq!(original_operator("src/a.rs:3:9: delete ! in f"), Some("!"));
        assert_eq!(
            original_operator("src/a.rs:3:9: replace f -> bool with true"),
            None
        );
        assert_eq!(
            original_operator("src/a.rs:3:9: delete match arm Some(x) in f"),
            None
        );
    }

    #[test]
    fn only_canonical_joins_of_supported_families_are_scoreable() {
        let disposition =
            |join, genre, kind, outcome| match_disposition(join, genre, kind, outcome);
        assert_eq!(
            disposition(
                "span_containment",
                "BinaryOperator",
                "predicate_boundary",
                "missed"
            ),
            "canonical_precise"
        );
        assert_eq!(
            disposition("seam_id", "UnaryOperator", "return_value", "caught"),
            "canonical_precise"
        );
        // A line fallback is excluded even for a supported genre and seam kind.
        assert_eq!(
            disposition(
                "file_line",
                "BinaryOperator",
                "predicate_boundary",
                "missed"
            ),
            "file_line_only"
        );
        assert_eq!(
            disposition(
                "line_guess",
                "BinaryOperator",
                "predicate_boundary",
                "missed"
            ),
            "unknown_join_method"
        );
        assert_eq!(
            disposition("span_containment", "FnValue", "return_value", "missed"),
            "unsupported_genre"
        );
        assert_eq!(
            disposition("span_containment", "", "return_value", "missed"),
            "unknown_genre"
        );
        assert_eq!(
            disposition(
                "span_containment",
                "UnaryOperator",
                "call_presence",
                "missed"
            ),
            "unsupported_seam_kind"
        );
        assert_eq!(
            disposition(
                "span_containment",
                "BinaryOperator",
                "predicate_boundary",
                "unviable"
            ),
            "unscoreable_outcome"
        );
    }

    #[test]
    fn mutant_set_digest_ignores_order_and_tracks_membership() {
        let a = json!({"name": "src/a.rs:1:1: replace > with <", "genre": "BinaryOperator"});
        let b = json!({"name": "src/b.rs:2:2: replace f -> bool with true", "genre": "FnValue"});
        let forward = mutant_set_sha256(&json!([a.clone(), b.clone()]));
        assert_eq!(forward, mutant_set_sha256(&json!([b.clone(), a.clone()])));
        assert_ne!(forward, mutant_set_sha256(&json!([a])));
        assert_eq!(forward.len(), 64);
    }

    #[test]
    fn genres_come_from_the_mutants_json_array() {
        let mutants = json!([
            {"name": "src/a.rs:3:9: replace > with < in f", "file": "src/a.rs", "genre": "BinaryOperator"},
            {"name": "src/a.rs:5:5: replace f -> bool with true", "file": "src/a.rs", "genre": "FnValue"},
            {"file": "src/a.rs", "genre": "UnaryOperator"}
        ]);
        assert_eq!(
            mutant_genres(&mutants),
            BTreeMap::from([
                (
                    "src/a.rs:3:9: replace > with < in f".to_string(),
                    "BinaryOperator".to_string()
                ),
                (
                    "src/a.rs:5:5: replace f -> bool with true".to_string(),
                    "FnValue".to_string()
                ),
            ])
        );
    }

    #[test]
    fn operator_token_is_a_diagnostic_that_cannot_veto_a_canonical_join() -> Result<(), String> {
        // `>` appears in the expression only inside `>=`, which v1 refused to
        // pair; the span join is authoritative, so the record is still scored.
        let calibration = calibration_v2(
            json!([
                {"join_method": "span_containment",
                 "static": {"seam_id": "s1", "seam_kind": "predicate_boundary", "seam_grip_class": "ungripped", "file": "src/a.rs", "line": 3},
                 "runtime": {"mutant_id": "src/a.rs:3:9: replace > with < in f", "runtime_outcome": "caught"}}
            ]),
            json!([]),
            json!([]),
            json!([]),
        );
        let exposure = json!({"seams": [{"seam_id": "s1", "expression": "x >= y"}]});
        let genres = BTreeMap::from([(
            "src/a.rs:3:9: replace > with < in f".to_string(),
            "BinaryOperator".to_string(),
        )]);
        let records = classify_records(&calibration, &exposure, &genres)?;
        assert_eq!(records[0].disposition, "canonical_precise");
        assert_eq!(records[0].operator_token, "absent");
        let report = build_report(&[repo_run(records)], 5);
        assert_eq!(
            report["scored_families"]["claims_no_discriminator"]["counts"]["false_gap"],
            1
        );
        assert_eq!(report["operator_token_diagnostic"]["counts"]["absent"], 1);
        assert_eq!(
            report["operator_token_diagnostic"]["absent_examples"][0]["seam_id"],
            "s1"
        );
        assert_eq!(
            operator_token_diagnostic("src/a.rs:3:9: replace > with < in f", Some("i > 0")),
            "present"
        );
        assert_eq!(
            operator_token_diagnostic("src/a.rs:3:9: replace > with < in f", None),
            "expression_unavailable"
        );
        assert_eq!(
            operator_token_diagnostic("src/a.rs:3:9: replace f -> bool with true", Some("x")),
            "no_original_operator"
        );
        Ok(())
    }

    #[test]
    fn calibration_without_span_joins_or_with_missing_records_is_refused() {
        let genres = BTreeMap::new();
        let mut v1 = calibration_v2(json!([]), json!([]), json!([]), json!([]));
        v1["schema_version"] = json!("0.1");
        assert!(
            classify_records(&v1, &Value::Null, &genres)
                .is_err_and(|err| err.contains("calibration schema 0.2"))
        );
        // mutants_total counts a record the lists do not carry.
        let mut partial = calibration_v2(
            json!([]),
            json!([]),
            json!([]),
            json!([{"mutant_id": "src/a.rs:3:9: replace > with < in f", "file": "src/a.rs", "line": 3,
                    "runtime_outcome": "missed", "unmatched_reason": "no_seam_on_line"}]),
        );
        assert_eq!(
            classify_records(&partial, &Value::Null, &genres).map(|records| records.len()),
            Ok(1)
        );
        partial["metrics"]["mutants_total"] = json!(2);
        assert!(
            classify_records(&partial, &Value::Null, &genres).is_err_and(
                |err| err.contains("lists 1 runtime records but reports mutants_total 2")
            )
        );
    }

    #[test]
    fn agreement_separates_overclaims_from_false_gaps() {
        assert_eq!(
            agreement(verdict_family("strongly_gripped"), "caught"),
            "agree"
        );
        assert_eq!(
            agreement(verdict_family("strongly_gripped"), "missed"),
            "overclaim"
        );
        assert_eq!(agreement(verdict_family("ungripped"), "missed"), "agree");
        assert_eq!(
            agreement(verdict_family("ungripped"), "caught"),
            "false_gap"
        );
        assert_eq!(
            agreement(verdict_family("weakly_gripped"), "missed"),
            "unscored"
        );
        assert_eq!(
            agreement(verdict_family("activation_unknown"), "caught"),
            "unscored"
        );
        assert_eq!(
            agreement(verdict_family("strongly_gripped"), "timeout"),
            "unscored"
        );
    }

    /// A calibration 0.2 report whose `mutants_total` equals its records.
    fn calibration_v2(
        matches: Value,
        ambiguous_file_line: Value,
        ambiguous_span_overlap: Value,
        unmatched: Value,
    ) -> Value {
        let total = [
            &matches,
            &ambiguous_file_line,
            &ambiguous_span_overlap,
            &unmatched,
        ]
        .iter()
        .map(|list| list.as_array().map_or(0, Vec::len))
        .sum::<usize>();
        json!({
            "schema_version": "0.2",
            "metrics": {"mutants_total": total},
            "matches": matches,
            "ambiguous_file_line_matches": ambiguous_file_line,
            "ambiguous_span_overlap_matches": ambiguous_span_overlap,
            "unmatched_mutants": unmatched,
        })
    }

    fn repo_run(records: Vec<Record>) -> RepoRun {
        RepoRun {
            name: "demo".to_string(),
            revision: "abc".to_string(),
            diffs_checked: records.len(),
            exposure_run_status: None,
            cargo_mutants_args: Some(Vec::new()),
            cargo_mutants_version: Some("27.1.0".to_string()),
            mutant_set_sha256: "0".repeat(64),
            metrics: Value::Null,
            records,
        }
    }

    fn fixture(dir: &str, file: &str) -> Result<Value, String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/boundary_gap/calibration")
            .join(dir)
            .join(file);
        read_json(&path)
    }

    /// Genres from a cargo-mutants `outcomes.json`-shaped fixture, standing in
    /// for the `mutants.json` a real `mutants.out` directory carries.
    fn fixture_genres(dir: &str) -> Result<BTreeMap<String, String>, String> {
        let outcomes = fixture(dir, "runtime-mutants.json")?;
        let mutants = outcomes["outcomes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|outcome| outcome.pointer("/scenario/Mutant").cloned())
            .collect::<Vec<_>>();
        Ok(mutant_genres(&Value::Array(mutants)))
    }

    #[test]
    fn report_scores_canonical_joins_and_keeps_examples_per_seam() -> Result<(), String> {
        let at = |seam: &str, kind: &str, class: &str, line: u64| json!({"seam_id": seam, "seam_kind": kind, "seam_grip_class": class, "file": "src/a.rs", "line": line});
        let calibration = calibration_v2(
            json!([
                {"join_method": "span_containment", "static": at("s1", "predicate_boundary", "ungripped", 3),
                 "runtime": {"mutant_id": "src/a.rs:3:9: replace > with < in f", "runtime_outcome": "caught"}},
                {"join_method": "span_containment", "static": at("s1", "predicate_boundary", "ungripped", 3),
                 "runtime": {"mutant_id": "src/a.rs:3:9: replace > with >= in f", "runtime_outcome": "missed"}},
                {"join_method": "seam_id", "static": at("s2", "return_value", "strongly_gripped", 7),
                 "runtime": {"mutant_id": "src/a.rs:7:5: replace + with - in g", "runtime_outcome": "caught"}},
                {"join_method": "span_containment", "static": at("s3", "call_presence", "strongly_gripped", 9),
                 "runtime": {"mutant_id": "src/a.rs:9:5: replace + with - in h", "runtime_outcome": "missed"}},
                {"join_method": "file_line", "static": at("s4", "predicate_boundary", "strongly_gripped", 11),
                 "runtime": {"mutant_id": "src/a.rs:11:5: replace < with > in k", "runtime_outcome": "missed"}}
            ]),
            json!([{"runtime": {"mutant_id": "src/a.rs:13:5: replace - with + in m", "file": "src/a.rs", "line": 13, "runtime_outcome": "missed"}}]),
            json!([{"runtime": {"mutant_id": "src/a.rs:15:5: replace && with || in n", "file": "src/a.rs", "line": 15, "runtime_outcome": "missed"}}]),
            json!([{"mutant_id": "src/a.rs:17:5: replace * with + in p", "file": "src/a.rs", "line": 17,
                    "runtime_outcome": "caught", "unmatched_reason": "no_containing_seam"}]),
        );
        let exposure = json!({"seams": [
            {"seam_id": "s1", "expression": "i > 0"},
            {"seam_id": "s2", "expression": "a + b"},
            {"seam_id": "s3", "expression": "h(a + b)"}
        ]});
        let genres = [
            "src/a.rs:3:9: replace > with < in f",
            "src/a.rs:3:9: replace > with >= in f",
            "src/a.rs:7:5: replace + with - in g",
            "src/a.rs:9:5: replace + with - in h",
            "src/a.rs:11:5: replace < with > in k",
        ]
        .map(|name| (name.to_string(), "BinaryOperator".to_string()))
        .into_iter()
        .collect();
        let records = classify_records(&calibration, &exposure, &genres)?;
        let report = build_report(&[repo_run(records)], 5);

        let pairings = &report["repos"][0]["pairings"];
        assert_eq!(pairings["records_total"], 8);
        assert_eq!(pairings["canonical_precise"], 3);
        assert_eq!(pairings["canonical_precise_by_join_method"]["seam_id"], 1);
        assert_eq!(
            pairings["canonical_precise_by_join_method"]["span_containment"],
            2
        );
        assert_eq!(
            pairings["excluded"],
            json!({
                "ambiguous_file_line": 1,
                "ambiguous_span_overlap": 1,
                "file_line_only": 1,
                "unmatched_no_containing_seam": 1,
                "unsupported_seam_kind": 1,
            })
        );
        let gap = &report["scored_families"]["claims_no_discriminator"];
        assert_eq!(gap["mutants_scored"], 2);
        assert_eq!(gap["seams_scored"], 1);
        assert_eq!(gap["counts"]["false_gap"], 1);
        assert_eq!(gap["agreement_rate"], 0.5);
        // The call-presence and file/line misses are excluded, so neither can
        // count as an overclaim against a strongly gripped verdict.
        let clean = &report["scored_families"]["claims_discriminator"];
        assert_eq!(clean["mutants_scored"], 1);
        assert_eq!(clean["agreement_rate"], 1.0);
        assert_eq!(
            report["outcomes_by_disposition_and_grip_class"]["file_line_only"]["strongly_gripped"]
                ["missed"],
            1
        );
        assert_eq!(
            report["outcomes_by_disposition_and_grip_class"]["unmatched_no_containing_seam"]["no_joined_seam"]
                ["caught"],
            1
        );
        let example = &report["disagreement_examples"]["false_gap"][0];
        assert_eq!(example["seam_id"], "s1");
        assert_eq!(example["join_method"], "span_containment");
        assert!(report["disagreement_examples"].get("overclaim").is_none());
        let markdown = spot_check_markdown(&report);
        assert!(markdown.contains("| demo | `abc` | 27.1.0 | 8 | 3 | 1 | 2 | 5 |"));
        assert!(markdown.contains("| demo | `file_line_only` | 1 |"));
        assert!(markdown.contains("**false_gap** demo `src/a.rs:3`"));
        assert!(markdown.contains("(joined by span_containment)"));
        assert!(markdown.contains("present 3."));
        assert!(!markdown.contains("ran cargo-mutants with"));

        let mut report = report;
        report["disagreement_examples"]["false_gap"][0]["expression"] = json!("i\n        > 0");
        assert!(spot_check_markdown(&report).contains("predicate_boundary `i > 0` is"));
        report["repos"][0]["cargo_mutants_args"] = json!(["--re=decode", "--workspace"]);
        assert!(
            spot_check_markdown(&report)
                .contains("demo ran cargo-mutants with `--re=decode` `--workspace`.")
        );
        Ok(())
    }

    /// atuinsh/atuin@90f590b9235556363ffb5b2c66728f8af3c27afe, 27 cargo-mutants
    /// 27.1.0 records. v1 (operator text over file/line joins) scored 1 of
    /// them; the canonical join scores 7 and attributes the other 20.
    #[test]
    fn atuin_multi_seam_lines_score_from_the_canonical_join() -> Result<(), String> {
        let dir = "span-containment-atuin";
        let records = classify_records(
            &fixture(dir, "mutation-calibration.json")?,
            &fixture(dir, "repo-exposure.json")?,
            &fixture_genres(dir)?,
        )?;
        let report = build_report(&[repo_run(records)], 10);
        let pairings = &report["repos"][0]["pairings"];
        assert_eq!(pairings["records_total"], 27);
        assert_eq!(pairings["canonical_precise"], 7);
        assert_eq!(
            pairings["canonical_precise_by_join_method"],
            json!({"seam_id": 0, "span_containment": 7})
        );
        // No exclusion is the catch-all `ambiguous_file_line`: each names why.
        assert_eq!(
            pairings["excluded"],
            json!({
                "ambiguous_span_overlap": 6,
                "file_line_only": 5,
                "unmatched_no_containing_seam": 8,
                "unsupported_seam_kind": 1,
            })
        );
        let families = &report["scored_families"];
        assert_eq!(
            families["claims_discriminator"]["counts"],
            json!({"overclaim": 2})
        );
        assert_eq!(families["claims_discriminator"]["seams_scored"], 2);
        assert_eq!(
            families["claims_no_discriminator"]["counts"],
            json!({"agree": 5})
        );
        assert_eq!(families["claims_no_discriminator"]["seams_scored"], 4);
        let overclaims = report["disagreement_examples"]["overclaim"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|row| format!("{}:{}", row["file"].as_str().unwrap_or(""), row["line"]))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(
            overclaims,
            [
                "crates/atuin-ai/src/context.rs:72",
                "crates/atuin/src/logs/otel/enabled.rs:62"
            ]
        );
        // The reduced fixture omits seam expressions, so the diagnostic says
        // so instead of guessing.
        assert_eq!(
            report["operator_token_diagnostic"]["counts"],
            json!({"expression_unavailable": 7})
        );
        Ok(())
    }

    /// semver `display.rs:20`: the `+` mutants sit beside, not inside, the
    /// call seam that starts on the same line, so neither joins nor scores.
    #[test]
    fn semver_same_line_call_seam_is_not_a_precise_pair() -> Result<(), String> {
        let dir = "span-containment-semver-display";
        let records = classify_records(
            &fixture(dir, "mutation-calibration.json")?,
            &fixture(dir, "repo-exposure.json")?,
            &fixture_genres(dir)?,
        )?;
        let report = build_report(&[repo_run(records)], 10);
        let pairings = &report["repos"][0]["pairings"];
        assert_eq!(pairings["records_total"], 2);
        assert_eq!(pairings["canonical_precise"], 0);
        assert_eq!(
            pairings["excluded"],
            json!({"unmatched_no_containing_seam": 2})
        );
        assert_eq!(report["scored_families"], json!({}));
        Ok(())
    }
}
