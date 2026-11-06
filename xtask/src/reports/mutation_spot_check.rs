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
//!
//! The report also scores `ripr pilot`'s top recommendations against the same
//! outcomes (see [`pilot`]), because a wrong top recommendation is the error a
//! developer meets first.

pub(crate) mod pilot;

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
/// Calibration schemas whose joined-record shape the spot check classifies.
/// The renderer owns its version at `MUTATION_CALIBRATION_SCHEMA_VERSION` in
/// `crates/ripr/src/output/mutation_calibration.rs`; xtask is a separate crate
/// and cannot import it, so the supported set is duplicated here with this
/// pointer at the owner (RIPR-SPEC-0292). 0.2 carries the span-containment
/// join and explicit span-overlap and unmatched reasons; 0.3 (RIPR-SPEC-0294,
/// PR #8988) adds provenance and how_to_read without changing that shape.
const SUPPORTED_CALIBRATION_SCHEMA_VERSIONS: &[&str] = &["0.2", "0.3"];
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
const NO_CONTAINING_SEAM: &str = "unmatched_no_containing_seam";
/// Grip classes `recall_of_missed` always reports, so a class with no missed
/// mutants is an explicit zero rather than an absent key.
const RECALL_GRIP_CLASSES: &[&str] = &[
    "ungripped",
    "activation_unknown",
    "strongly_gripped",
    "opaque",
    "weakly_gripped",
    "no_joined_seam",
];
/// Grip classes that count toward the lenient recall share: a gap flag
/// (`ungripped`, `weakly_gripped`) or an honest unknown (`activation_unknown`,
/// `opaque`). `strongly_gripped`, `no_joined_seam` and unknown classes do not.
const FLAGGED_OR_UNKNOWN_CLASSES: &[&str] = &[
    "ungripped",
    "weakly_gripped",
    "activation_unknown",
    "opaque",
];
/// Languages `recall_of_missed.by_language` always lists. Only a language with
/// a recorded real-mutant corpus is established.
const RECALL_LANGUAGES: &[&str] = &["rust", "python", "typescript"];
/// Language of every repo this harness scores: it drives cargo-mutants.
const HARNESS_LANGUAGE: &str = "rust";
const REDERIVE_USAGE: &str =
    "usage: cargo xtask mutation-spot-check --rederive-summaries <mutation-spot-check.json>";
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
    if let Some(index) = args.iter().position(|arg| arg == "--rederive-summaries") {
        if args.len() != 2 || index != 0 {
            return Err(format!(
                "--rederive-summaries takes exactly one path and no other argument; {REDERIVE_USAGE}"
            ));
        }
        return rederive_summaries(Path::new(&args[1]));
    }
    let options = parse_options(args)?;
    let binary = match &options.ripr {
        Some(path) => path.clone(),
        None => {
            crate::run::run("cargo", &["build", "-p", "ripr"])?;
            crate::ripr_debug_binary()
        }
    };
    // Read before the long runs so a binary that cannot identify itself fails
    // fast, and the recorded build is the one that does the classifying.
    let analyzer = analyzer_version(&binary)?;
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
    let report = build_report(&repos, options.examples, &analyzer);
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
    language: String,
    revision: String,
    diffs_checked: usize,
    exposure_run_status: Option<String>,
    /// `None` for a supplied `mutants.out`: its run's arguments are not
    /// recorded anywhere this harness can read.
    cargo_mutants_args: Option<Vec<String>>,
    cargo_mutants_version: Option<String>,
    /// The harness's per-mutant timeout; `None` for a supplied `mutants.out`.
    mutant_timeout_secs: Option<u64>,
    mutant_set_sha256: String,
    metrics: Value,
    records: Vec<Record>,
    /// Judged pilot recommendations, or why pilot produced none. A pilot
    /// failure does not discard the repo's validated mutation outcomes.
    pilot: Result<Vec<Value>, String>,
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
    let pilot_top = pilot::pilot_top_seams(binary, scratch, name, checkout);

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
        language: HARNESS_LANGUAGE.to_string(),
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
        mutant_timeout_secs: (!options.mutants_out.contains_key(name))
            .then_some(options.mutant_timeout_secs),
        mutant_set_sha256: mutant_set_sha256(&mutant_records)
            .map_err(|err| format!("`{name}`: {err}"))?,
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
        pilot: pilot_top.map(|top| {
            pilot::judge_recommendations(
                &top,
                &mutant_records,
                &outcomes,
                &seam_expressions(&exposure_json),
                &|file, line| {
                    let index = usize::try_from(line).ok()?.checked_sub(1)?;
                    std::fs::read_to_string(checkout.join(file))
                        .ok()?
                        .lines()
                        .nth(index)
                        .map(str::to_string)
                },
            )
        }),
    })
}

/// cargo-mutants records no source revision, so a supplied `mutants.out` from
/// another commit would join stale outcomes to this checkout's seams. Every
/// mutant diff carries the original lines it replaced; they must still match
/// the checkout, or nothing from this directory is scored.
pub(crate) fn require_mutants_match_checkout(
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

pub(crate) fn read_json(path: &Path) -> Result<Value, String> {
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

/// Seam id to source expression, from the repo exposure JSON.
pub(crate) fn seam_expressions(exposure: &Value) -> BTreeMap<&str, &str> {
    exposure
        .get("seams")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|seam| {
            Some((
                seam.get("seam_id")?.as_str()?,
                seam.get("expression").and_then(Value::as_str).unwrap_or(""),
            ))
        })
        .collect()
}

/// cargo-mutants genre per mutant name, from `mutants.out/mutants.json`.
/// Digest of the sorted mutant names cargo-mutants generated. The set is what
/// the rates are computed over, and it reflects selection arguments
/// (`--re`, `--exclude`, `--workspace`) even when a supplied run did not
/// record them, so the scoreboard keys comparability on it.
///
/// A nameless entry would leave the digest short of the real set, so it is
/// refused rather than skipped.
fn mutant_set_sha256(mutants: &Value) -> Result<String, String> {
    let entries = mutants
        .as_array()
        .ok_or("mutants.json is not an array of mutants")?;
    let mut names = entries
        .iter()
        .enumerate()
        .map(|(index, mutant)| {
            mutant
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("mutants.json entry {} has no string `name`", index + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.sort_unstable();
    let mut digest = Sha256::new();
    for name in names {
        digest.update(name.as_bytes());
        digest.update(b"\n");
    }
    Ok(format!("{:x}", digest.finalize()))
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
    if !SUPPORTED_CALIBRATION_SCHEMA_VERSIONS.contains(&schema) {
        return Err(format!(
            "mutation calibration schema `{schema}` is not one the spot check classifies; supported calibration schema versions: {}. The renderer's current schema version is owned by `MUTATION_CALIBRATION_SCHEMA_VERSION` in crates/ripr/src/output/mutation_calibration.rs (0.2 before RIPR-SPEC-0294, 0.3 after it); rebuild ripr and re-run `cargo xtask mutation-spot-check` so the calibration JSON it feeds the harness carries a supported schema",
            SUPPORTED_CALIBRATION_SCHEMA_VERSIONS.join(", ")
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

/// The pilot judge's `seam` tier: an operator mutant of a predicate or return
/// seam whose original operator token occurs in that seam's expression. This
/// is the v1 pairing rule, kept for pilot picks, which carry no calibration
/// join of their own; it never scores a verdict family.
fn operator_mutant_of_expression(
    genre: &str,
    mutant: &str,
    seam_kind: &str,
    expression: &str,
) -> bool {
    matches!(genre, "BinaryOperator" | "UnaryOperator")
        && matches!(seam_kind, "predicate_boundary" | "return_value")
        && original_operator(mutant)
            .is_some_and(|operator| contains_operator_token(expression, operator))
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

/// Identity of the ripr build that produced the static classifications: the
/// binary's own `--version` line (version plus commit). A binary that cannot
/// report one fails the run, so a receipt never carries an unidentifiable
/// build (#5982).
fn analyzer_version(binary: &Path) -> Result<String, String> {
    let output =
        crate::run::run_output_owned(&binary.display().to_string(), &["--version".to_string()])
            .map_err(|err| {
                format!(
                    "could not read the analyzer version from {}: {err}",
                    binary.display()
                )
            })?;
    let version = output.trim();
    if version.is_empty() {
        return Err(format!(
            "{} printed an empty --version, so the receipt cannot record the analyzer build",
            binary.display()
        ));
    }
    Ok(version.to_string())
}

fn build_report(repos: &[RepoRun], examples: usize, analyzer: &str) -> Value {
    let mut by_disposition: BTreeMap<String, BTreeMap<String, BTreeMap<String, usize>>> =
        BTreeMap::new();
    // Both families are always reported, so a run that scores only one
    // still ingests; the empty family carries a null rate (#6135 review).
    let mut scored: BTreeMap<&str, BTreeMap<&str, usize>> =
        ["claims_discriminator", "claims_no_discriminator"]
            .into_iter()
            .map(|family| (family, BTreeMap::new()))
            .collect();
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
    let by_disposition_value = json!(by_disposition);
    let mut language_outcomes: BTreeMap<String, Value> = BTreeMap::new();
    for repo in repos {
        merge_outcomes(
            language_outcomes
                .entry(repo.language.clone())
                .or_insert_with(|| json!({})),
            &json!(outcome_table(&repo.records)),
        );
    }
    let recall = recall_of_missed(&by_disposition_value, &language_outcomes);
    let coverage = seam_coverage(&by_disposition_value);
    let families = Value::Object(families);
    let replacement = replacement_criterion(Some(&families), &recall, &coverage, None);
    json!({
        "schema_version": SCHEMA_VERSION,
        "status": "advisory",
        "analyzer_version": analyzer,
        "claim_boundary": "Agreement is scored only on canonical_precise records: a BinaryOperator or UnaryOperator mutant that ripr calibrate joined by seam_id or by unique span containment to a predicate_boundary or return_value seam, with a caught or missed outcome. File/line fallback joins, span-overlap ties, unmatched records, and unsupported genres, seam kinds or outcomes are counted as exclusions and never scored. Claims are limited to the recorded checkout revisions, cargo-mutants versions, and this join rule; this is not a suite adequacy measure.",
        "repos": repos.iter().map(|repo| json!({
            "name": repo.name,
            "language": repo.language,
            "revision": repo.revision,
            "mutant_diffs_checked_against_revision": repo.diffs_checked,
            "exposure_run_status": repo.exposure_run_status,
            "cargo_mutants_args": repo.cargo_mutants_args,
            "cargo_mutants_version": repo.cargo_mutants_version,
            "mutant_timeout_secs": repo.mutant_timeout_secs,
            "mutant_set_sha256": repo.mutant_set_sha256,
            "calibration_metrics": repo.metrics,
            "pairings": pairing_counts(&repo.records),
            "outcomes_by_disposition_and_grip_class": outcome_table(&repo.records),
        })).collect::<Vec<_>>(),
        "outcomes_by_disposition_and_grip_class": by_disposition,
        "scored_families": families,
        "recall_of_missed": recall,
        "seam_coverage": coverage,
        "replacement_criterion": replacement,
        "disagreement_examples": disagreements,
        "operator_token_diagnostic": {
            "counts": operator_tokens,
            "absent_examples": operator_token_absent,
        },
        "pilot_top_recommendations": pilot::summarize(
            &repos
                .iter()
                .map(|repo| (repo.name.clone(), repo.pilot.clone()))
                .collect::<Vec<_>>(),
        ),
    })
}

/// Bars of the `replacement_criterion` row. Precision and flagged-or-unknown
/// recall are the R2 rows of the release ladder in docs/METRICS.md. The other
/// two are agent-chosen technical bars with the rationale in
/// `COVERAGE_RATIONALE` and `TIME_RATIO_RATIONALE`; change them here, never in
/// a published receipt.
const PRECISION_THRESHOLD: f64 = 0.97;
const RECALL_FLAGGED_OR_UNKNOWN_THRESHOLD: f64 = 0.80;
const SCORED_COVERAGE_THRESHOLD: f64 = 0.50;
/// Maximum ripr wall time as a share of full cargo-mutants wall time.
const TIME_RATIO_THRESHOLD: f64 = 0.10;
const LADDER_SOURCE: &str = "docs/METRICS.md release ladder R2";
const COVERAGE_RATIONALE: &str = "agent-chosen: a majority of generated mutants must reach a canonical_precise seam join and so receive a scored verdict";
const TIME_RATIO_RATIONALE: &str = "agent-chosen: ripr must cost under a tenth of a full cargo-mutants run on the same crate to be a usable substitute in the edit loop";

fn threshold_status(observed: Option<f64>, threshold: f64, at_most: bool) -> &'static str {
    match observed {
        None => "not_established",
        Some(value) if at_most && value <= threshold => "met",
        Some(value) if !at_most && value >= threshold => "met",
        Some(_) => "not_met",
    }
}

/// Verdict over criterion rows: `replaces` only when every thresholded
/// (non-informational) row is `met`; otherwise `not_yet` with the ids that are
/// `not_met` or `not_established`.
fn replacement_verdict(criteria: &[Value]) -> Value {
    let mut unmet = Vec::new();
    let mut not_established = Vec::new();
    for row in criteria {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match row.get("status").and_then(Value::as_str) {
            Some("met") | Some("informational") => {}
            Some("not_met") => unmet.push(id),
            _ => not_established.push(id),
        }
    }
    let verdict = if unmet.is_empty() && not_established.is_empty() {
        "replaces"
    } else {
        "not_yet"
    };
    json!({
        "verdict": verdict,
        "not_met": unmet,
        "not_established": not_established,
    })
}

/// Stated, machine-readable "replaces cargo-mutants" criterion with a status per
/// criterion, derived from the other summary rows of the same receipt. `timing`
/// is the optional recorded `timing_observation` (`ripr_seconds`,
/// `cargo_mutants_seconds`); no timing is ever invented.
fn replacement_criterion(
    report_families: Option<&Value>,
    recall: &Value,
    coverage: &Value,
    timing: Option<&Value>,
) -> Value {
    let scope = json!(["rust"]);
    let f = |value: Option<&Value>| value.and_then(Value::as_f64);
    let precision =
        f(report_families.and_then(|v| v.pointer("/claims_discriminator/agreement_rate")));
    let flagged = f(recall.get("flagged_or_unknown_share_of_missed"));
    let strict = f(recall.get("ungripped_share_of_missed"));
    let precise = f(coverage.get("canonical_precise_share"));
    let ripr_secs = f(timing.and_then(|t| t.get("ripr_seconds")));
    let mutants_secs = f(timing.and_then(|t| t.get("cargo_mutants_seconds")));
    // The bar is compared against the unrounded ratio; only the serialised
    // figure is rounded for display.
    let raw_ratio = match (ripr_secs, mutants_secs) {
        (Some(ripr), Some(full)) if full > 0.0 && ripr >= 0.0 => Some(ripr / full),
        _ => None,
    };
    let ratio = raw_ratio.map(|raw| (raw * 1000.0).round() / 1000.0);
    let mut time_row = json!({
        "id": "time_vs_full_cargo_mutants",
        "description": "ripr wall time on the diff as a share of full cargo-mutants wall time on the same checkout",
        "threshold": TIME_RATIO_THRESHOLD,
        "threshold_source": TIME_RATIO_RATIONALE,
        "recorded": timing.cloned(),
        "direction": "at_most",
        "observed": {"ripr_seconds": ripr_secs, "cargo_mutants_seconds": mutants_secs, "ratio": ratio},
        "status": threshold_status(raw_ratio, TIME_RATIO_THRESHOLD, true),
        "scope": scope,
    });
    if ratio.is_none() {
        time_row["reason"] = json!(
            "no paired timing of ripr and full cargo-mutants is recorded for these checkouts; no figure is claimed"
        );
    }
    let criteria = vec![
        json!({
            "id": "precision_of_discriminator_claims",
            "description": "agreement of ripr claims_discriminator seams with runtime outcomes",
            "threshold": PRECISION_THRESHOLD,
            "threshold_source": LADDER_SOURCE,
            "direction": "at_least",
            "observed": precision,
            "status": threshold_status(precision, PRECISION_THRESHOLD, false),
            "scope": scope,
        }),
        json!({
            "id": "recall_flagged_or_unknown",
            "description": "share of runtime-missed mutants ripr flagged as a gap or an honest unknown",
            "threshold": RECALL_FLAGGED_OR_UNKNOWN_THRESHOLD,
            "threshold_source": LADDER_SOURCE,
            "direction": "at_least",
            "observed": flagged,
            "status": threshold_status(flagged, RECALL_FLAGGED_OR_UNKNOWN_THRESHOLD, false),
            "scope": scope,
        }),
        json!({
            "id": "recall_strict_ungripped",
            "description": "share of runtime-missed mutants ripr flagged ungripped (informational, no threshold)",
            "threshold": null,
            "observed": strict,
            "status": "informational",
            "scope": scope,
        }),
        json!({
            "id": "scored_coverage",
            "description": "share of recorded mutants that reach a canonical_precise seam join",
            "threshold": SCORED_COVERAGE_THRESHOLD,
            "threshold_source": COVERAGE_RATIONALE,
            "direction": "at_least",
            "observed": precise,
            "status": threshold_status(precise, SCORED_COVERAGE_THRESHOLD, false),
            "scope": scope,
        }),
        time_row,
        json!({
            "id": "python_typescript_real_mutant_agreement",
            "description": "agreement with real mutants on Python and TypeScript",
            "threshold": null,
            "observed": null,
            "status": "not_established",
            "reason": "no real-mutant corpus has been run for Python or TypeScript; no figure is claimed",
            "scope": ["python", "typescript"],
        }),
    ];
    let verdict = replacement_verdict(&criteria);
    json!({
        "claim_boundary": "Advisory. Answers whether ripr usably replaces cargo-mutants on the recorded evidence. Precision and flagged-or-unknown recall use the R2 bars of the release ladder in docs/METRICS.md; scored coverage and the time ratio are agent-chosen technical bars whose rationale is recorded per criterion in threshold_source. A replaces verdict requires every thresholded criterion met and none not_established. It is fail-closed by design: `replaces` is currently unreachable because the Python/TypeScript criterion is not_established until a real-mutant corpus exists. The time criterion is met on one crate, one diff and one run; it is not a distribution or a general speed claim. It is not suite adequacy and does not mean ripr runs or substitutes for mutation testing. Scope is Rust on the recorded crates and checkout revisions only.",
        "criteria": criteria,
        "verdict": verdict["verdict"],
        "not_met": verdict["not_met"],
        "not_established": verdict["not_established"],
    })
}

/// Sum `outcomes_by_disposition_and_grip_class` cells. `keep` receives
/// (disposition, grip class, outcome); the result is the total count kept.
fn sum_outcomes(outcomes: &Value, keep: impl Fn(&str, &str, &str) -> bool) -> usize {
    let mut total = 0;
    for (disposition, classes) in outcomes.as_object().into_iter().flatten() {
        for (class, by_outcome) in classes.as_object().into_iter().flatten() {
            for (outcome, count) in by_outcome.as_object().into_iter().flatten() {
                if keep(disposition, class, outcome) {
                    total += count.as_u64().unwrap_or(0) as usize;
                }
            }
        }
    }
    total
}

/// Disposition, grip class and outcome counts of one repo's records. A record
/// with no grip class is `no_joined_seam`, as in the report-wide table.
fn outcome_table(
    records: &[Record],
) -> BTreeMap<String, BTreeMap<String, BTreeMap<String, usize>>> {
    let mut table: BTreeMap<String, BTreeMap<String, BTreeMap<String, usize>>> = BTreeMap::new();
    for record in records {
        let grip_class = if record.grip_class.is_empty() {
            "no_joined_seam"
        } else {
            record.grip_class.as_str()
        };
        *table
            .entry(record.disposition.clone())
            .or_default()
            .entry(grip_class.to_string())
            .or_default()
            .entry(record.outcome.clone())
            .or_default() += 1;
    }
    table
}

/// Add every count of `src` into `dst`, both `outcomes_by_disposition_and_grip_class`
/// shaped.
fn merge_outcomes(dst: &mut Value, src: &Value) {
    if let (Some(dst_map), Some(src_map)) = (dst.as_object_mut(), src.as_object()) {
        for (key, value) in src_map {
            match value.as_u64() {
                Some(count) => {
                    let current = dst_map.get(key).and_then(Value::as_u64).unwrap_or(0);
                    dst_map.insert(key.clone(), json!(current + count));
                }
                None => merge_outcomes(
                    dst_map.entry(key.clone()).or_insert_with(|| json!({})),
                    value,
                ),
            }
        }
    }
}

/// Recall counts and shares for one outcome table. `ungripped_share_of_missed`
/// is the strict figure; `flagged_or_unknown_share_of_missed` also credits an
/// honest unknown (`activation_unknown`, `opaque`) and a `weakly_gripped` gap.
fn recall_counts(outcomes: &Value) -> serde_json::Map<String, Value> {
    let missed_in =
        |class: &str| sum_outcomes(outcomes, |_, c, outcome| c == class && outcome == "missed");
    let missed_total = sum_outcomes(outcomes, |_, _, outcome| outcome == "missed");
    let mut by_class: BTreeMap<String, usize> = RECALL_GRIP_CLASSES
        .iter()
        .map(|class| (class.to_string(), missed_in(class)))
        .collect();
    // A grip class outside the known list is still counted, never dropped.
    let known: usize = by_class.values().sum();
    if known != missed_total {
        by_class.insert("other".to_string(), missed_total - known);
    }
    let ungripped = by_class.get("ungripped").copied().unwrap_or(0);
    let lenient: usize = FLAGGED_OR_UNKNOWN_CLASSES
        .iter()
        .map(|class| by_class.get(*class).copied().unwrap_or(0))
        .sum();
    let mut out = serde_json::Map::new();
    out.insert("missed_total".into(), json!(missed_total));
    out.insert("missed_by_grip_class".into(), json!(by_class));
    out.insert("ungripped_missed".into(), json!(ungripped));
    out.insert(
        "ungripped_share_of_missed".into(),
        rate(ungripped, missed_total),
    );
    out.insert("flagged_or_unknown_missed".into(), json!(lenient));
    out.insert("not_flagged_missed".into(), json!(missed_total - lenient));
    out.insert(
        "flagged_or_unknown_share_of_missed".into(),
        rate(lenient, missed_total),
    );
    out
}

/// Among runtime-missed mutants across every disposition, how many ripr's grip
/// class flagged. Two figures, never merged: the strict ungripped share and the
/// flagged-or-unknown share. `by_language` carries the same counts per language
/// with a recorded corpus; every other listed language is `not_established`.
fn recall_of_missed(outcomes: &Value, language_outcomes: &BTreeMap<String, Value>) -> Value {
    let mut report = recall_counts(outcomes);
    let mut languages = serde_json::Map::new();
    for language in RECALL_LANGUAGES
        .iter()
        .copied()
        .chain(language_outcomes.keys().map(String::as_str))
    {
        let entry = match language_outcomes.get(language) {
            Some(table) => {
                let mut counts = recall_counts(table);
                counts.insert("status".into(), json!("established"));
                Value::Object(counts)
            }
            None => json!({
                "status": "not_established",
                "reason": "no real-mutant corpus has been run for this language yet; no figure is claimed",
            }),
        };
        languages.insert(language.to_string(), entry);
    }
    report.insert("by_language".into(), Value::Object(languages));
    report.insert("claim_boundary".into(), json!("Advisory. Counts runtime-missed mutants of every disposition by the grip class ripr recorded for them; the recorded checkout revisions and cargo-mutants runs only. Two figures are reported and must be labelled: ungripped_share_of_missed is the strict share flagged ungripped; flagged_or_unknown_share_of_missed also counts weakly_gripped, activation_unknown and opaque, which are an honest gap or unknown rather than a confirmed gap, and excludes strongly_gripped and no_joined_seam (no seam) mutants. A missed mutant that ripr did not flag is not a ripr error and a share is not suite adequacy. by_language lists only languages with a recorded real-mutant corpus as established."));
    Value::Object(report)
}

/// How much of the mutant population reaches a canonical-precise seam join.
fn seam_coverage(outcomes: &Value) -> Value {
    let mutants_total = sum_outcomes(outcomes, |_, _, _| true);
    let scoreable = sum_outcomes(outcomes, |_, _, outcome| {
        SCOREABLE_OUTCOMES.contains(&outcome)
    });
    let caught = sum_outcomes(outcomes, |_, _, outcome| outcome == "caught");
    let missed = sum_outcomes(outcomes, |_, _, outcome| outcome == "missed");
    let precise = sum_outcomes(outcomes, |d, _, _| d == CANONICAL_PRECISE);
    let no_seam = sum_outcomes(outcomes, |d, _, _| d == NO_CONTAINING_SEAM);
    json!({
        "mutants_total": mutants_total,
        "with_caught_or_missed_outcome": scoreable,
        "caught": caught,
        "missed": missed,
        "canonical_precise": precise,
        "canonical_precise_share": rate(precise, mutants_total),
        "no_containing_seam": no_seam,
        "no_containing_seam_share": rate(no_seam, mutants_total),
        "claim_boundary": "Advisory. Shares are of every recorded mutant, including timeout and unviable outcomes; recorded checkout revisions only. Seam coverage is a join-rate fact about ripr's seam inventory, not suite adequacy.",
    })
}

/// Offline path: recompute the derived summary rows of a recorded report from
/// its own `outcomes_by_disposition_and_grip_class`, without re-running
/// mutation testing. Only `recall_of_missed`, `seam_coverage` and `replacement_criterion` change.
fn rederive_summaries(path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let mut report: Value =
        serde_json::from_str(&text).map_err(|err| format!("parse {}: {err}", path.display()))?;
    let schema = report.get("schema_version").and_then(Value::as_str);
    if schema != Some(SCHEMA_VERSION) {
        return Err(format!(
            "{} has schema_version {schema:?}, expected `{SCHEMA_VERSION}`",
            path.display()
        ));
    }
    let outcomes = report
        .get("outcomes_by_disposition_and_grip_class")
        .cloned()
        .ok_or_else(|| {
            format!(
                "{} has no outcomes_by_disposition_and_grip_class",
                path.display()
            )
        })?;
    let language_outcomes = recorded_language_outcomes(&mut report, &outcomes, path)?;
    let recall = recall_of_missed(&outcomes, &language_outcomes);
    let coverage = seam_coverage(&outcomes);
    report["replacement_criterion"] = replacement_criterion(
        report.get("scored_families"),
        &recall,
        &coverage,
        report.get("timing_observation"),
    );
    report["recall_of_missed"] = recall;
    report["seam_coverage"] = coverage;
    let json_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("serialize {}: {err}", path.display()))?;
    fs::write(path, format!("{json_text}\n"))
        .map_err(|err| format!("write {}: {err}", path.display()))?;
    println!("Re-derived summary rows in {}", path.display());
    Ok(())
}

/// Per-language outcome tables of a recorded report. A repo with no recorded
/// `language` is backfilled as `rust` (this harness only drives cargo-mutants).
/// A language's table is the sum of its repos' recorded tables; an old receipt
/// without them can only be split when it holds a single language.
fn recorded_language_outcomes(
    report: &mut Value,
    aggregate: &Value,
    path: &Path,
) -> Result<BTreeMap<String, Value>, String> {
    let repos = report
        .get_mut("repos")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("{} has no repos array", path.display()))?;
    let mut tables: BTreeMap<String, Value> = BTreeMap::new();
    let mut complete = true;
    for repo in repos.iter_mut() {
        if repo.get("language").and_then(Value::as_str).is_none() {
            repo["language"] = json!(HARNESS_LANGUAGE);
        }
        let language = repo["language"]
            .as_str()
            .unwrap_or(HARNESS_LANGUAGE)
            .to_string();
        let table = tables.entry(language).or_insert_with(|| json!({}));
        match repo.get("outcomes_by_disposition_and_grip_class") {
            Some(own) => merge_outcomes(table, own),
            None => complete = false,
        }
    }
    if complete {
        return Ok(tables);
    }
    if tables.len() == 1 {
        let language = tables.keys().next().cloned().unwrap_or_default();
        return Ok(BTreeMap::from([(language, aggregate.clone())]));
    }
    Err(format!(
        "{} mixes languages without per-repo outcome tables; re-run the harness to split recall by language",
        path.display()
    ))
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
    out.push_str(&derived_summaries_markdown(report));
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
    out.push_str(&pilot::markdown(report));
    out
}

fn derived_summaries_markdown(report: &Value) -> String {
    let mut out = String::new();
    if let Some(recall) = report.get("recall_of_missed") {
        let n = |key: &str| recall.get(key).and_then(Value::as_u64).unwrap_or(0);
        let cells = recall
            .get("missed_by_grip_class")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(class, count)| format!("{class} {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        let pct = |key: &str| {
            recall
                .get(key)
                .and_then(Value::as_f64)
                .map_or_else(|| "n/a".to_string(), |rate| format!("{:.1}%", rate * 100.0))
        };
        out.push_str(&format!(
            "\n## Recall of missed mutants\n\n{} runtime-missed mutants across all dispositions: {cells}.\n\n- Strict, flagged ungripped only: {} ({}).\n- Lenient, flagged or honest unknown (ungripped, weakly_gripped, activation_unknown, opaque): {} ({}). Not flagged (strongly_gripped, no seam): {}.\n\nAdvisory; not suite adequacy.\n",
            n("missed_total"),
            n("ungripped_missed"),
            pct("ungripped_share_of_missed"),
            n("flagged_or_unknown_missed"),
            pct("flagged_or_unknown_share_of_missed"),
            n("not_flagged_missed"),
        ));
        for (language, row) in recall
            .get("by_language")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if row.get("status").and_then(Value::as_str) == Some("established") {
                let m = |key: &str| row.get(key).and_then(Value::as_u64).unwrap_or(0);
                let p = |key: &str| {
                    row.get(key)
                        .and_then(Value::as_f64)
                        .map_or_else(|| "n/a".to_string(), |rate| format!("{:.1}%", rate * 100.0))
                };
                out.push_str(&format!(
                    "- {language}: {} missed; strict {} ({}); flagged or unknown {} ({}).\n",
                    m("missed_total"),
                    m("ungripped_missed"),
                    p("ungripped_share_of_missed"),
                    m("flagged_or_unknown_missed"),
                    p("flagged_or_unknown_share_of_missed"),
                ));
            } else {
                let reason = row.get("reason").and_then(Value::as_str).unwrap_or("");
                out.push_str(&format!("- {language}: not established ({reason}).\n"));
            }
        }
    }
    if let Some(cov) = report.get("seam_coverage") {
        let n = |key: &str| cov.get(key).and_then(Value::as_u64).unwrap_or(0);
        let pct = |key: &str| {
            cov.get(key)
                .and_then(Value::as_f64)
                .map_or_else(|| "n/a".to_string(), |rate| format!("{:.1}%", rate * 100.0))
        };
        out.push_str(&format!(
            "\n## Seam coverage\n\n{} mutants ({} caught, {} missed; {} with a caught or missed outcome). Canonical precise: {} ({}). No containing seam: {} ({}).\n",
            n("mutants_total"),
            n("caught"),
            n("missed"),
            n("with_caught_or_missed_outcome"),
            n("canonical_precise"),
            pct("canonical_precise_share"),
            n("no_containing_seam"),
            pct("no_containing_seam_share"),
        ));
    }
    if let Some(rc) = report.get("replacement_criterion") {
        let text = |row: &Value, key: &str| {
            row.get(key)
                .map(|v| match v {
                    Value::Null => "n/a".to_string(),
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "n/a".to_string())
        };
        let list = |key: &str| {
            let ids = rc
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            if ids.is_empty() {
                "none".to_string()
            } else {
                ids.join(", ")
            }
        };
        out.push_str(&format!(
            "\n## Replaces cargo-mutants?\n\nVerdict: `{}`. Not met: {}. Not established: {}.\n\n| Criterion | Bar and source | Observed | Status | Scope |\n| --- | --- | --- | --- | --- |\n",
            text(rc, "verdict"),
            list("not_met"),
            list("not_established"),
        ));
        for row in rc
            .get("criteria")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let scope = row
                .get("scope")
                .and_then(Value::as_array)
                .map(|langs| {
                    langs
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let observed = match row.get("observed") {
                Some(obs) if obs.is_object() => format!(
                    "ripr {}s, cargo-mutants {}s, ratio {}",
                    text(obs, "ripr_seconds"),
                    text(obs, "cargo_mutants_seconds"),
                    text(obs, "ratio"),
                ),
                _ => text(row, "observed"),
            };
            let observed = if row.get("id").and_then(Value::as_str)
                == Some("time_vs_full_cargo_mutants")
                && row.get("recorded").is_some_and(Value::is_object)
            {
                format!("{observed} (one crate, one diff, one run)")
            } else {
                observed
            };
            let bar = match row.get("threshold_source").and_then(Value::as_str) {
                Some(source) => format!("{} ({source})", text(row, "threshold")),
                None => text(row, "threshold"),
            };
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                text(row, "id"),
                bar,
                observed,
                text(row, "status"),
                scope,
            ));
        }
        out.push_str("\nBars: precision and recall from the R2 ladder rows in docs/METRICS.md, coverage and time ratio agent-chosen (see `threshold_source` in the JSON); a pass is not suite adequacy; scope is Rust on the recorded crates.\n");
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

    fn baseline_outcomes() -> Value {
        json!({
            "ambiguous_file_line": {"no_joined_seam": {"caught": 2, "missed": 1}},
            "ambiguous_span_overlap": {"no_joined_seam": {"caught": 31, "missed": 9, "unviable": 3}},
            "canonical_precise": {
                "activation_unknown": {"caught": 285, "missed": 86},
                "opaque": {"caught": 46, "missed": 1},
                "strongly_gripped": {"caught": 61, "missed": 2},
                "ungripped": {"missed": 12}
            },
            "file_line_only": {
                "activation_unknown": {"caught": 46, "missed": 8, "timeout": 3, "unviable": 1},
                "opaque": {"caught": 15, "timeout": 1},
                "strongly_gripped": {"caught": 49, "missed": 17, "unviable": 1}
            },
            "unmatched_no_containing_seam": {
                "no_joined_seam": {"caught": 697, "missed": 104, "timeout": 10, "unviable": 32}
            },
            "unscoreable_outcome": {
                "activation_unknown": {"timeout": 4},
                "strongly_gripped": {"timeout": 1, "unviable": 3}
            },
            "unsupported_genre": {
                "activation_unknown": {"caught": 24, "missed": 53, "unviable": 8},
                "opaque": {"caught": 8, "missed": 2, "unviable": 8},
                "strongly_gripped": {"caught": 47, "unviable": 21},
                "ungripped": {"missed": 22, "unviable": 1}
            },
            "unsupported_seam_kind": {
                "activation_unknown": {"caught": 6, "missed": 4},
                "opaque": {"caught": 4, "missed": 1, "unviable": 1},
                "weakly_gripped": {"caught": 4}
            }
        })
    }

    #[test]
    fn derived_summaries_match_the_recorded_baseline() {
        let outcomes = baseline_outcomes();
        let recall = recall_of_missed(&outcomes, &BTreeMap::new());
        assert_eq!(recall["missed_total"], 322);
        assert_eq!(recall["ungripped_missed"], 34);
        assert_eq!(recall["missed_by_grip_class"]["ungripped"], 34);
        assert_eq!(recall["missed_by_grip_class"]["activation_unknown"], 151);
        assert_eq!(recall["missed_by_grip_class"]["no_joined_seam"], 114);
        assert_eq!(recall["missed_by_grip_class"]["strongly_gripped"], 19);
        assert_eq!(recall["missed_by_grip_class"]["opaque"], 4);
        assert_eq!(recall["missed_by_grip_class"]["weakly_gripped"], 0);
        assert!(recall["missed_by_grip_class"].get("other").is_none());
        assert_eq!(recall["ungripped_share_of_missed"], 0.106);
        // Lenient: 34 ungripped + 151 activation_unknown + 4 opaque + 0 weak.
        assert_eq!(recall["flagged_or_unknown_missed"], 189);
        assert_eq!(recall["not_flagged_missed"], 133);
        assert_eq!(recall["flagged_or_unknown_share_of_missed"], 0.587);
        let coverage = seam_coverage(&outcomes);
        assert_eq!(coverage["mutants_total"], 1745);
        assert_eq!(coverage["canonical_precise"], 493);
        assert_eq!(coverage["no_containing_seam"], 843);
        assert_eq!(coverage["missed"], 322);
        assert_eq!(coverage["canonical_precise_share"], 0.283);
        assert_eq!(coverage["no_containing_seam_share"], 0.483);
    }

    #[test]
    fn derived_summaries_discriminate_dispositions_classes_and_outcomes() {
        // Positive: a missed ungripped mutant in a non-canonical disposition
        // still counts. Negative: caught/unviable ungripped and missed
        // strongly_gripped mutants must not inflate the ungripped count, and
        // a file_line_only record is not canonical precise.
        let outcomes = json!({
            "canonical_precise": {"ungripped": {"caught": 5}},
            "unsupported_genre": {
                "ungripped": {"missed": 1, "unviable": 4},
                "strongly_gripped": {"missed": 3}
            },
            "file_line_only": {"mystery_class": {"missed": 2}},
            "unmatched_no_containing_seam": {"no_joined_seam": {"timeout": 1}}
        });
        let recall = recall_of_missed(&outcomes, &BTreeMap::new());
        assert_eq!(recall["missed_total"], 6);
        assert_eq!(recall["ungripped_missed"], 1);
        assert_eq!(recall["missed_by_grip_class"]["strongly_gripped"], 3);
        assert_eq!(recall["missed_by_grip_class"]["other"], 2);
        let coverage = seam_coverage(&outcomes);
        assert_eq!(coverage["mutants_total"], 16);
        assert_eq!(coverage["with_caught_or_missed_outcome"], 11);
        assert_eq!(coverage["canonical_precise"], 5);
        assert_eq!(coverage["no_containing_seam"], 1);
        // Empty input: shares are null, never a divide by zero.
        let empty = json!({});
        assert!(recall_of_missed(&empty, &BTreeMap::new())["ungripped_share_of_missed"].is_null());
        assert!(seam_coverage(&empty)["canonical_precise_share"].is_null());
    }

    #[test]
    fn flagged_or_unknown_share_excludes_strong_and_no_seam_mutants() {
        let outcomes = json!({
            "canonical_precise": {
                "strongly_gripped": {"missed": 5, "caught": 9},
                "weakly_gripped": {"missed": 1},
                "opaque": {"missed": 2},
                "activation_unknown": {"missed": 3, "unviable": 7}
            },
            "unmatched_no_containing_seam": {"no_joined_seam": {"missed": 11}},
            "file_line_only": {"mystery_class": {"missed": 4}}
        });
        let recall = recall_of_missed(&outcomes, &BTreeMap::new());
        assert_eq!(recall["missed_total"], 26);
        // Only weakly_gripped (1), opaque (2) and activation_unknown (3).
        assert_eq!(recall["flagged_or_unknown_missed"], 6);
        assert_eq!(recall["not_flagged_missed"], 20);
        // The strict share counts none of them.
        assert_eq!(recall["ungripped_missed"], 0);
        assert_eq!(recall["ungripped_share_of_missed"], 0.0);
        assert!(
            recall["flagged_or_unknown_share_of_missed"]
                .as_f64()
                .is_some_and(|share| (share - 0.231).abs() < 1e-9)
        );
        // A strongly_gripped or no-seam miss alone is never flagged.
        let only_unflagged = json!({
            "canonical_precise": {"strongly_gripped": {"missed": 2}},
            "unmatched_no_containing_seam": {"no_joined_seam": {"missed": 3}}
        });
        let recall = recall_of_missed(&only_unflagged, &BTreeMap::new());
        assert_eq!(recall["flagged_or_unknown_missed"], 0);
        assert_eq!(recall["flagged_or_unknown_share_of_missed"], 0.0);
        assert!(
            recall_of_missed(&json!({}), &BTreeMap::new())["flagged_or_unknown_share_of_missed"]
                .is_null()
        );
    }

    #[test]
    fn by_language_establishes_only_recorded_languages() {
        let languages = BTreeMap::from([("rust".to_string(), baseline_outcomes())]);
        let recall = recall_of_missed(&baseline_outcomes(), &languages);
        assert_eq!(recall["by_language"]["rust"]["status"], "established");
        assert_eq!(recall["by_language"]["rust"]["missed_total"], 322);
        assert_eq!(
            recall["by_language"]["rust"]["flagged_or_unknown_missed"],
            189
        );
        for language in ["python", "typescript"] {
            let row = &recall["by_language"][language];
            assert_eq!(row["status"], "not_established");
            assert!(
                row["reason"]
                    .as_str()
                    .is_some_and(|reason| reason.contains("no real-mutant corpus"))
            );
            assert!(row.get("missed_total").is_none());
            assert!(row.get("flagged_or_unknown_share_of_missed").is_none());
        }
        // A fresh report splits recall by each repo's language.
        let report = build_report(&[repo_run(Vec::new())], 5, TEST_ANALYZER);
        assert_eq!(report["repos"][0]["language"], "rust");
        assert_eq!(
            report["recall_of_missed"]["by_language"]["rust"]["status"],
            "established"
        );
        assert_eq!(
            report["recall_of_missed"]["by_language"]["python"]["status"],
            "not_established"
        );
    }

    #[test]
    fn rederive_splits_by_language_only_when_it_can() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-rederive-lang-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let path = dir.join("report.json");
        let write = |repos: Value| {
            let report = json!({
                "schema_version": SCHEMA_VERSION,
                "outcomes_by_disposition_and_grip_class": baseline_outcomes(),
                "repos": repos,
            });
            fs::write(&path, report.to_string()).map_err(|err| err.to_string())
        };
        // Old receipt, no language recorded: backfilled rust, aggregate used.
        write(json!([{"name": "a"}, {"name": "b"}]))?;
        rederive_summaries(&path)?;
        let out: Value =
            serde_json::from_str(&fs::read_to_string(&path).map_err(|err| err.to_string())?)
                .map_err(|err| err.to_string())?;
        assert_eq!(out["repos"][0]["language"], "rust");
        assert_eq!(
            out["recall_of_missed"]["by_language"]["rust"]["missed_total"],
            322
        );
        assert_eq!(
            out["recall_of_missed"]["by_language"]["python"]["status"],
            "not_established"
        );
        // Mixed languages without per-repo tables cannot be split honestly.
        write(json!([{"name": "a", "language": "rust"}, {"name": "b", "language": "python"}]))?;
        assert!(
            rederive_summaries(&path).is_err_and(|err| err.contains("per-repo outcome tables"))
        );
        // With per-repo tables the split is exact.
        write(json!([
            {"name": "a", "language": "rust", "outcomes_by_disposition_and_grip_class":
                {"canonical_precise": {"ungripped": {"missed": 2}}}},
            {"name": "b", "language": "python", "outcomes_by_disposition_and_grip_class":
                {"canonical_precise": {"opaque": {"missed": 1}}}}
        ]))?;
        rederive_summaries(&path)?;
        let out: Value =
            serde_json::from_str(&fs::read_to_string(&path).map_err(|err| err.to_string())?)
                .map_err(|err| err.to_string())?;
        assert_eq!(
            out["recall_of_missed"]["by_language"]["rust"]["ungripped_missed"],
            2
        );
        assert_eq!(
            out["recall_of_missed"]["by_language"]["python"]["status"],
            "established"
        );
        assert_eq!(
            out["recall_of_missed"]["by_language"]["python"]["flagged_or_unknown_missed"],
            1
        );
        assert_eq!(
            out["recall_of_missed"]["by_language"]["typescript"]["status"],
            "not_established"
        );
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[test]
    fn rederive_summaries_refuses_other_schemas_and_rewrites_only_summary_rows()
    -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-rederive-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let path = dir.join("report.json");
        let write = |schema: &str| {
            let report = json!({
                "schema_version": schema,
                "outcomes_by_disposition_and_grip_class": baseline_outcomes(),
                "repos": [{"name": "demo"}],
                "status": "advisory",
            });
            fs::write(&path, report.to_string()).map_err(|err| err.to_string())
        };
        write("ripr-mutation-spot-check-v1")?;
        assert!(rederive_summaries(&path).is_err_and(|err| err.contains("expected")));
        write(SCHEMA_VERSION)?;
        rederive_summaries(&path)?;
        let out: Value =
            serde_json::from_str(&fs::read_to_string(&path).map_err(|err| err.to_string())?)
                .map_err(|err| err.to_string())?;
        assert_eq!(out["seam_coverage"]["mutants_total"], 1745);
        assert_eq!(out["recall_of_missed"]["ungripped_missed"], 34);
        assert_eq!(out["status"], "advisory");
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(())
    }

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
    fn mutant_set_digest_ignores_order_and_tracks_membership() -> Result<(), String> {
        let a = json!({"name": "src/a.rs:1:1: replace > with <", "genre": "BinaryOperator"});
        let b = json!({"name": "src/b.rs:2:2: replace f -> bool with true", "genre": "FnValue"});
        let forward = mutant_set_sha256(&json!([a.clone(), b.clone()]))?;
        assert_eq!(forward, mutant_set_sha256(&json!([b.clone(), a.clone()]))?);
        assert_ne!(forward, mutant_set_sha256(&json!([a.clone()]))?);
        assert_eq!(forward.len(), 64);
        // A nameless mutant or a non-array file cannot yield a digest of the
        // real set.
        assert!(
            mutant_set_sha256(&json!([a, {"genre": "FnValue"}]))
                .is_err_and(|err| err.contains("entry 2 has no string `name`"))
        );
        assert!(mutant_set_sha256(&json!({})).is_err_and(|err| err.contains("not an array")));
        Ok(())
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
        let mut v3 = calibration.clone();
        v3["schema_version"] = json!("0.3");
        v3["provenance"] = json!({
            "repo_exposure_head": null, "mutants_head": null, "verdict": "no_head_recorded",
            "disclosure": "Neither artifact names a head; qualification remains advisory."
        });
        let v3_records = classify_records(&v3, &exposure, &genres)?;
        assert_eq!(v3_records, records);
        assert_eq!(records[0].disposition, "canonical_precise");
        assert_eq!(records[0].operator_token, "absent");
        let report = build_report(&[repo_run(records)], 5, TEST_ANALYZER);
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
                .is_err_and(|err| err.contains("schema `0.1`")
                    && err.contains("supported calibration schema versions: 0.2, 0.3")
                    && err.contains("MUTATION_CALIBRATION_SCHEMA_VERSION"))
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
        let mut v3 = partial.clone();
        v3["schema_version"] = json!("0.3");
        v3["provenance"] = json!({"repo_exposure_head": null, "mutants_head": null, "verdict": "no_head_recorded", "disclosure": "No recorded heads."});
        assert_eq!(
            classify_records(&v3, &Value::Null, &genres),
            classify_records(&partial, &Value::Null, &genres)
        );
        v3["metrics"]["mutants_total"] = json!(2);
        assert!(
            classify_records(&v3, &Value::Null, &genres).is_err_and(
                |err| err.contains("lists 1 runtime records but reports mutants_total 2")
            )
        );
        v3["schema_version"] = json!("0.4");
        assert!(
            classify_records(&v3, &Value::Null, &genres)
                .is_err_and(|err| err.contains("schema `0.4`")
                    && err.contains("supported calibration schema versions: 0.2, 0.3")
                    && err.contains("MUTATION_CALIBRATION_SCHEMA_VERSION"))
        );
        partial["metrics"]["mutants_total"] = json!(2);
        assert!(
            classify_records(&partial, &Value::Null, &genres).is_err_and(
                |err| err.contains("lists 1 runtime records but reports mutants_total 2")
            )
        );
    }

    #[test]
    fn calibration_schema_0_3_classifies_and_unknown_versions_are_refused() -> Result<(), String> {
        let mut v3 = calibration_v2(
            json!([
                {"join_method": "seam_id",
                 "static": {"seam_id": "s1", "seam_kind": "predicate_boundary", "seam_grip_class": "strongly_gripped", "file": "src/a.rs", "line": 3},
                 "runtime": {"mutant_id": "src/a.rs:3:9: replace > with < in f", "runtime_outcome": "caught"}}
            ]),
            json!([]),
            json!([]),
            json!([]),
        );
        v3["schema_version"] = json!("0.3");
        let exposure = json!({"seams": [{"seam_id": "s1", "expression": "i > 0"}]});
        let genres = BTreeMap::from([(
            "src/a.rs:3:9: replace > with < in f".to_string(),
            "BinaryOperator".to_string(),
        )]);
        let records = classify_records(&v3, &exposure, &genres)?;
        if records.len() != 1 || records[0].disposition != CANONICAL_PRECISE {
            return Err(format!(
                "schema 0.3 classified {} records with disposition `{}`, expected one canonical_precise",
                records.len(),
                records
                    .first()
                    .map_or("none", |record| record.disposition.as_str())
            ));
        }
        // A version newer than the supported set is refused with the same
        // actionable message, naming the unsupported version, the supported
        // set, and the renderer's version owner.
        v3["schema_version"] = json!("0.4");
        match classify_records(&v3, &exposure, &genres) {
            Err(err)
                if err.contains("schema `0.4`")
                    && err.contains("supported calibration schema versions: 0.2, 0.3")
                    && err.contains("MUTATION_CALIBRATION_SCHEMA_VERSION") =>
            {
                Ok(())
            }
            Err(err) => Err(format!("unexpected refusal for schema 0.4: {err}")),
            Ok(_) => Err("schema 0.4 was classified but is not in the supported set".to_string()),
        }
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

    const TEST_ANALYZER: &str = "ripr 0.11.0 (0123456789abcdef)";

    #[test]
    fn report_records_the_analyzer_build() {
        let report = build_report(&[repo_run(Vec::new())], 5, TEST_ANALYZER);
        assert_eq!(report["analyzer_version"], TEST_ANALYZER);
    }

    #[test]
    fn analyzer_version_refuses_a_binary_that_cannot_identify_itself() -> Result<(), String> {
        let missing = Path::new("/nonexistent/ripr-for-5982");
        match analyzer_version(missing) {
            Ok(version) => Err(format!("a missing binary reported `{version}`")),
            Err(err) if err.contains("could not read the analyzer version") => Ok(()),
            Err(err) => Err(format!("unexpected refusal: {err}")),
        }
    }

    fn repo_run(records: Vec<Record>) -> RepoRun {
        RepoRun {
            name: "demo".to_string(),
            language: "rust".to_string(),
            revision: "abc".to_string(),
            diffs_checked: records.len(),
            exposure_run_status: None,
            cargo_mutants_args: Some(Vec::new()),
            cargo_mutants_version: Some("27.1.0".to_string()),
            mutant_timeout_secs: Some(60),
            mutant_set_sha256: "0".repeat(64),
            metrics: Value::Null,
            records,
            pilot: Ok(Vec::new()),
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
    fn a_run_scoring_one_family_still_ingests() -> Result<(), String> {
        let calibration = calibration_v2(
            json!([
                {"join_method": "span_containment",
                 "static": {"seam_id": "s1", "seam_kind": "predicate_boundary", "seam_grip_class": "ungripped", "file": "src/a.rs", "line": 3},
                 "runtime": {"mutant_id": "src/a.rs:3:9: replace > with < in f", "runtime_outcome": "missed"}}
            ]),
            json!([]),
            json!([]),
            json!([]),
        );
        let exposure = json!({"seams": [{"seam_id": "s1", "expression": "i > 0"}]});
        let genres = [(
            "src/a.rs:3:9: replace > with < in f".to_string(),
            "BinaryOperator".to_string(),
        )]
        .into_iter()
        .collect();
        let mut run = repo_run(classify_records(&calibration, &exposure, &genres)?);
        run.metrics = json!({"mutants_total": 1});
        let report = build_report(&[run], 5, TEST_ANALYZER);
        let clean = &report["scored_families"]["claims_discriminator"];
        assert_eq!(clean["mutants_scored"], 0);
        assert!(clean["agreement_rate"].is_null());
        assert_eq!(
            report["scored_families"]["claims_no_discriminator"]["mutants_scored"],
            1
        );
        let input = crate::reports::dx_scoreboard::mutation_spot_check_to_input(&report)?;
        let ids: Vec<_> = input["metrics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| row["id"].as_str())
            .collect();
        assert!(ids.contains(&"trust.gap_claim_agreement"), "{ids:?}");
        assert!(
            !ids.contains(&"trust.discriminator_claim_agreement"),
            "{ids:?}"
        );
        Ok(())
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
        let mut run = repo_run(records);
        run.pilot = Ok(vec![
            json!({"verdict": "refuted", "tier": "line", "grip_class": "weakly_gripped"}),
        ]);
        let report = build_report(&[run], 5, TEST_ANALYZER);

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
        assert_eq!(report["pilot_top_recommendations"]["counts"]["refuted"], 1);
        assert!(spot_check_markdown(&report).contains("## Pilot top recommendations"));
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
        let report = build_report(&[repo_run(records)], 10, TEST_ANALYZER);
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
        let report = build_report(&[repo_run(records)], 10, TEST_ANALYZER);
        let pairings = &report["repos"][0]["pairings"];
        assert_eq!(pairings["records_total"], 2);
        assert_eq!(pairings["canonical_precise"], 0);
        assert_eq!(
            pairings["excluded"],
            json!({"unmatched_no_containing_seam": 2})
        );
        // Nothing is scored, yet both families stay present with null rates.
        for family in ["claims_discriminator", "claims_no_discriminator"] {
            assert_eq!(report["scored_families"][family]["mutants_scored"], 0);
            assert!(report["scored_families"][family]["agreement_rate"].is_null());
        }
        Ok(())
    }

    fn families(rate: f64) -> Value {
        json!({"claims_discriminator": {"agreement_rate": rate}})
    }

    fn criterion(
        precision: Option<f64>,
        flagged: f64,
        covered: f64,
        timing: Option<Value>,
    ) -> Value {
        let families = precision.map(families);
        replacement_criterion(
            families.as_ref(),
            &json!({"flagged_or_unknown_share_of_missed": flagged, "ungripped_share_of_missed": 0.1}),
            &json!({"canonical_precise_share": covered}),
            timing.as_ref(),
        )
    }

    fn status_of<'a>(rc: &'a Value, id: &str) -> &'a str {
        rc["criteria"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["id"] == id))
            .and_then(|row| row["status"].as_str())
            .unwrap_or("missing")
    }

    #[test]
    fn replacement_criterion_is_not_yet_on_the_recorded_numbers() {
        let rc = criterion(Some(0.968), 0.587, 0.283, None);
        assert_eq!(rc["verdict"], "not_yet");
        assert_eq!(
            status_of(&rc, "precision_of_discriminator_claims"),
            "not_met"
        );
        assert_eq!(status_of(&rc, "recall_flagged_or_unknown"), "not_met");
        assert_eq!(status_of(&rc, "scored_coverage"), "not_met");
        assert_eq!(status_of(&rc, "recall_strict_ungripped"), "informational");
        assert_eq!(
            status_of(&rc, "time_vs_full_cargo_mutants"),
            "not_established"
        );
        assert_eq!(
            status_of(&rc, "python_typescript_real_mutant_agreement"),
            "not_established"
        );
        assert_eq!(
            rc["not_met"],
            json!([
                "precision_of_discriminator_claims",
                "recall_flagged_or_unknown",
                "scored_coverage"
            ])
        );
        assert_eq!(
            rc["not_established"],
            json!([
                "time_vs_full_cargo_mutants",
                "python_typescript_real_mutant_agreement"
            ])
        );
    }

    #[test]
    fn replacement_verdict_needs_every_thresholded_criterion_met() {
        let row = |id: &str, status: &str| json!({"id": id, "status": status});
        let all_met = [row("a", "met"), row("b", "informational"), row("c", "met")];
        assert_eq!(replacement_verdict(&all_met)["verdict"], "replaces");
        // One not_established blocks even when every other criterion is met.
        let one_unknown = [row("a", "met"), row("b", "not_established")];
        let verdict = replacement_verdict(&one_unknown);
        assert_eq!(verdict["verdict"], "not_yet");
        assert_eq!(verdict["not_established"], json!(["b"]));
        // A real timing observation clears the time criterion on its own.
        let timing = json!({"ripr_seconds": 5.0, "cargo_mutants_seconds": 100.0});
        let rc = criterion(Some(0.99), 0.9, 0.6, Some(timing));
        assert_eq!(status_of(&rc, "time_vs_full_cargo_mutants"), "met");
        assert_eq!(rc["criteria"][4]["observed"]["ratio"], 0.05);
        assert_eq!(status_of(&rc, "precision_of_discriminator_claims"), "met");
        // Python/TypeScript has no corpus, so the full verdict is still not_yet.
        assert_eq!(rc["verdict"], "not_yet");
        assert_eq!(
            rc["not_established"],
            json!(["python_typescript_real_mutant_agreement"])
        );
    }

    #[test]
    fn recorded_timing_flips_the_time_criterion_against_the_bar() {
        let time_status = |ripr: f64, full: f64| {
            let timing = json!({"ripr_seconds": ripr, "cargo_mutants_seconds": full});
            let rc = criterion(Some(0.99), 0.9, 0.6, Some(timing));
            status_of(&rc, "time_vs_full_cargo_mutants").to_string()
        };
        // Bar is a ratio of at most 0.10.
        assert_eq!(time_status(10.0, 100.0), "met"); // exactly at the bar
        assert_eq!(time_status(10.1, 100.0), "not_met");
        assert_eq!(time_status(50.0, 100.0), "not_met");
        // A true ratio just above the bar that rounds to 0.100 is still not met.
        assert_eq!(time_status(10.0004, 100.0), "not_met");
        // A zero full-run time cannot yield a ratio.
        assert_eq!(time_status(1.0, 0.0), "not_established");
    }

    #[test]
    fn markdown_time_row_carries_the_single_sample_caveat() {
        let timing = json!({"ripr_seconds": 5.0, "cargo_mutants_seconds": 100.0, "crate": "demo"});
        let rc = criterion(Some(0.99), 0.9, 0.6, Some(timing));
        let markdown = derived_summaries_markdown(&json!({"replacement_criterion": rc}));
        assert!(markdown.contains("ratio 0.05 (one crate, one diff, one run)"));
        let unmeasured = criterion(Some(0.99), 0.9, 0.6, None);
        let markdown = derived_summaries_markdown(&json!({"replacement_criterion": unmeasured}));
        assert!(!markdown.contains("one crate, one diff, one run"));
    }

    #[test]
    fn replacement_criterion_threshold_boundary_and_missing_inputs() {
        let at = criterion(Some(0.97), 0.80, 0.50, None);
        assert_eq!(status_of(&at, "precision_of_discriminator_claims"), "met");
        assert_eq!(status_of(&at, "recall_flagged_or_unknown"), "met");
        assert_eq!(status_of(&at, "scored_coverage"), "met");
        let below = criterion(Some(0.969), 0.799, 0.499, None);
        assert_eq!(
            status_of(&below, "precision_of_discriminator_claims"),
            "not_met"
        );
        assert_eq!(status_of(&below, "recall_flagged_or_unknown"), "not_met");
        assert_eq!(status_of(&below, "scored_coverage"), "not_met");
        // Missing inputs are not_established, never not_met.
        let rc = replacement_criterion(None, &json!({}), &json!({}), None);
        assert_eq!(
            status_of(&rc, "precision_of_discriminator_claims"),
            "not_established"
        );
        assert_eq!(
            status_of(&rc, "recall_flagged_or_unknown"),
            "not_established"
        );
        assert_eq!(status_of(&rc, "scored_coverage"), "not_established");
        assert!(rc["not_met"].as_array().is_some_and(Vec::is_empty));
        // A half-recorded timing is not a ratio.
        let half = criterion(Some(0.99), 0.9, 0.6, Some(json!({"ripr_seconds": 3.0})));
        assert_eq!(
            status_of(&half, "time_vs_full_cargo_mutants"),
            "not_established"
        );
    }

    #[test]
    fn rederive_adds_the_replacement_criterion_row() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-rederive-rc-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let path = dir.join("receipt.json");
        let receipt = json!({
            "schema_version": SCHEMA_VERSION,
            "repos": [],
            "scored_families": families(0.968),
            "outcomes_by_disposition_and_grip_class": baseline_outcomes(),
        });
        fs::write(&path, receipt.to_string()).map_err(|err| err.to_string())?;
        rederive_summaries(&path)?;
        let out: Value =
            serde_json::from_str(&fs::read_to_string(&path).map_err(|e| e.to_string())?)
                .map_err(|err| err.to_string())?;
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(out["replacement_criterion"]["verdict"], "not_yet");
        assert_eq!(
            out["replacement_criterion"]["criteria"][0]["observed"],
            0.968
        );
        assert_eq!(
            out["replacement_criterion"]["criteria"][3]["observed"],
            0.283
        );
        let markdown = spot_check_markdown(&out);
        assert!(markdown.contains("## Replaces cargo-mutants?"));
        assert!(markdown.contains("0.97 (docs/METRICS.md release ladder R2)"));
        assert!(markdown.contains("agent-chosen: a majority"));
        assert!(!markdown.contains("one crate, one diff, one run"));
        Ok(())
    }
}
