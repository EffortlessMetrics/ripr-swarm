//! `cargo xtask kani`: honest runner for the Kani bounded-model-checking lane
//! (RIPR-SPEC-0371).
//!
//! The lane runs Kani once per owning package and reads
//! one result block per discovered `#[kani::proof]` harness:
//!
//! - `VERIFIED` (within the harness's documented bounds), or
//! - `FAILED` (native verification failure or an unsatisfied required cover), or
//! - `NOT-ESTABLISHED` (tool missing, timed out, no verdict or no complete
//!   positive cover evidence in the output).
//!
//! Absence never passes: a missing tool, zero harnesses (unless the operator
//! passes `--allow-empty`) and every `NOT-ESTABLISHED` harness exit nonzero.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::run;
use serde::{Deserialize, Serialize};

mod machine;

const HARNESS_PACKAGES: [(&str, &str); 2] = [
    ("ripr", "crates/ripr/src"),
    ("ripr-contract", "crates/ripr-contract/src"),
];
const REPORT_DIR: &str = "target/ripr/reports";
/// Whole-run wall-clock deadline; a wedged solver is `NOT-ESTABLISHED`.
const DEFAULT_TIMEOUT_SECS: u64 = 3600;
/// Per-harness deadline handed to Kani, so one non-terminating harness is
/// `NOT-ESTABLISHED` for itself and the rest of the lane still reports. The
/// slowest verified harness runs about four minutes; this leaves headroom
/// without letting one wedge consume the whole-run deadline.
const DEFAULT_HARNESS_TIMEOUT_SECS: u64 = 900;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KaniArgs {
    pub(crate) allow_empty: bool,
    pub(crate) harness: Option<String>,
    pub(crate) default_unwind: Option<u32>,
    pub(crate) timeout_secs: u64,
    pub(crate) harness_timeout_secs: u64,
}

pub(crate) fn parse_args(args: &[String]) -> Result<KaniArgs, String> {
    let mut parsed = KaniArgs {
        allow_empty: false,
        harness: None,
        default_unwind: None,
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        harness_timeout_secs: DEFAULT_HARNESS_TIMEOUT_SECS,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--allow-empty" => parsed.allow_empty = true,
            "--harness" => {
                parsed.harness = Some(next_value(&mut iter, "--harness")?);
            }
            "--default-unwind" => {
                let value = next_value(&mut iter, "--default-unwind")?;
                parsed.default_unwind = Some(parse_positive(&value, "--default-unwind")?);
            }
            "--timeout-secs" => {
                let value = next_value(&mut iter, "--timeout-secs")?;
                parsed.timeout_secs = u64::from(parse_positive(&value, "--timeout-secs")?);
            }
            "--harness-timeout-secs" => {
                let value = next_value(&mut iter, "--harness-timeout-secs")?;
                parsed.harness_timeout_secs =
                    u64::from(parse_positive(&value, "--harness-timeout-secs")?);
            }
            other => {
                return Err(format!(
                    "unknown kani argument `{other}`; usage: cargo xtask kani [--allow-empty] [--harness <name>] [--default-unwind <n>] [--timeout-secs <n>] [--harness-timeout-secs <n>]"
                ));
            }
        }
    }
    Ok(parsed)
}

fn next_value<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<String, String> {
    iter.next()
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive(value: &str, flag: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(number) if number > 0 => Ok(number),
        _ => Err(format!("{flag} requires a positive integer, got `{value}`")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Harness {
    pub(crate) file: String,
    pub(crate) name: String,
}

/// Finds `#[kani::proof]` harnesses in `(path, source)` pairs: the attribute
/// line followed (after any further attributes or doc comments) by `fn name`.
pub(crate) fn discover_harnesses(files: &[(String, String)]) -> Vec<Harness> {
    let mut found = Vec::new();
    for (file, text) in files {
        let mut pending = false;
        let mut in_raw_string = false;
        for line in text.lines() {
            let trimmed = line.trim();
            // Text inside a `r#"..."#` literal is fixture data (for example the
            // test-style fixtures that mention `#[kani::proof]`), not a harness.
            if in_raw_string {
                if trimmed.contains("\"#") {
                    in_raw_string = false;
                }
                continue;
            }
            if let Some(open) = trimmed.find("r#\"")
                && !trimmed[open + 3..].contains("\"#")
            {
                in_raw_string = true;
                pending = false;
                continue;
            }
            if trimmed.starts_with("#[kani::proof") {
                pending = true;
                continue;
            }
            if !pending {
                continue;
            }
            if trimmed.starts_with("#[") || trimmed.starts_with("//") || trimmed.is_empty() {
                continue;
            }
            pending = false;
            if let Some(name) = function_name(trimmed) {
                found.push(Harness {
                    file: file.clone(),
                    name,
                });
            }
        }
    }
    found.sort_by(|left, right| (&left.file, &left.name).cmp(&(&right.file, &right.name)));
    found
}

fn function_name(line: &str) -> Option<String> {
    let rest = line.split("fn ").nth(1)?;
    let name: String = rest
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "status",
    content = "detail",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub(crate) enum HarnessStatus {
    Verified,
    Failed(String),
    #[serde(rename = "NOT-ESTABLISHED")]
    NotEstablished(String),
}

impl HarnessStatus {
    fn label(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Failed(_) => "FAILED",
            Self::NotEstablished(_) => "NOT-ESTABLISHED",
        }
    }

    fn detail(&self) -> &str {
        match self {
            Self::Verified => "within the harness bounds",
            Self::Failed(detail) | Self::NotEstablished(detail) => detail,
        }
    }
}

/// Classifies one harness result block from Kani's output.
/// Only an explicit success verdict with positive evidence that every required
/// `cover!` was satisfied is `Verified`; missing evidence is never a pass.
pub(crate) fn classify_output(output: &str) -> HarnessStatus {
    // Kani prints `VERIFICATION:- FAILED` for a solver timeout too; that is no
    // counterexample, so it is checked first and stays `NOT-ESTABLISHED`.
    if output.contains("CBMC timed out") {
        return HarnessStatus::NotEstablished(
            "CBMC exceeded the per-harness timeout; no verdict".to_string(),
        );
    }
    if output.contains("VERIFICATION:- FAILED") {
        return HarnessStatus::Failed(
            "native verification failed; inspect the reported properties".to_string(),
        );
    }
    if output.contains("No proof harnesses") {
        return HarnessStatus::NotEstablished(
            "kani found no proof harness with this name".to_string(),
        );
    }
    if output.contains("VERIFICATION:- SUCCESSFUL") {
        return classify_cover_evidence(output);
    }
    HarnessStatus::NotEstablished(
        "no successful verification verdict in the kani output".to_string(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverSummary {
    satisfied: u64,
    total: u64,
    undetermined: u64,
    unreachable: u64,
}

/// Kani 0.68.0 emits one positive-total summary per harness. Its optional
/// suffix lists undetermined then unreachable counts; unsatisfiable covers
/// contribute to the total without a suffix. Parse that format fail-closed.
fn parse_cover_summary(line: &str) -> Option<CoverSummary> {
    fn count(text: &str) -> Option<u64> {
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse().ok()
    }

    let body = line.trim().strip_prefix("** ")?;
    let (counts, suffix) = body.split_once(" cover properties satisfied")?;
    let mut words = counts.split_whitespace();
    let satisfied = count(words.next()?)?;
    if words.next()? != "of" {
        return None;
    }
    let total = count(words.next()?)?;
    if words.next().is_some() || total == 0 || satisfied > total {
        return None;
    }

    let mut undetermined = 0;
    let mut unreachable = 0;
    if !suffix.is_empty() {
        let details = suffix.strip_prefix(" (")?.strip_suffix(')')?;
        for detail in details.split(',') {
            if let Some(value) = detail.strip_suffix(" undetermined") {
                // The native renderer emits each nonzero count at most once,
                // and emits undetermined before unreachable.
                if undetermined != 0 || unreachable != 0 {
                    return None;
                }
                undetermined = count(value)?;
                if undetermined == 0 {
                    return None;
                }
            } else if let Some(value) = detail.strip_suffix(" unreachable") {
                if unreachable != 0 {
                    return None;
                }
                unreachable = count(value)?;
                if unreachable == 0 {
                    return None;
                }
            } else {
                return None;
            }
        }
    }
    if undetermined.checked_add(unreachable)? > total - satisfied {
        return None;
    }
    Some(CoverSummary {
        satisfied,
        total,
        undetermined,
        unreachable,
    })
}

fn classify_cover_evidence(output: &str) -> HarnessStatus {
    let missing = |reason: &str| HarnessStatus::NotEstablished(reason.to_string());
    let mut summaries = output
        .lines()
        .filter(|line| line.contains("cover properties satisfied"));
    let Some(line) = summaries.next() else {
        return missing("kani reported no positive cover summary for this harness");
    };
    if summaries.next().is_some() {
        return missing("kani printed multiple cover summaries for this harness");
    }
    let Some(summary) = parse_cover_summary(line) else {
        return missing("kani cover summary is malformed, inconsistent or has zero properties");
    };

    // The regular renderer's total omits raw Unknown/Error properties. Unknown
    // is printed as UNDETERMINED, so even an N-of-N summary cannot override it.
    // An ordinary assertion can legitimately be UNREACHABLE. Associate each
    // status with its regular Check header so a complete summary cannot hide
    // an explicitly unreachable cover; match the final property class only.
    let mut is_cover_property = false;
    for line in output.lines() {
        let text = line.trim();
        if let Some(check) = text.strip_prefix("Check ") {
            is_cover_property = check
                .split_once(": ")
                .is_some_and(|(_, property)| property.rsplit('.').nth(1) == Some("cover"));
            continue;
        }
        let Some(status) = text
            .strip_prefix("- Status:")
            .or_else(|| text.strip_prefix("Status:"))
        else {
            continue;
        };
        match status.trim() {
            "UNDETERMINED" | "ERROR" => {
                return missing("kani reported an undetermined or erroneous property");
            }
            "UNSATISFIABLE" if summary.satisfied == summary.total => {
                return missing("kani property status contradicts its complete cover summary");
            }
            "UNREACHABLE" if is_cover_property && summary.satisfied == summary.total => {
                return missing("kani property status contradicts its complete cover summary");
            }
            "SUCCESS" | "FAILURE" | "COVERED" | "UNCOVERED" if is_cover_property => {
                return missing("kani reported an unsupported cover property status");
            }
            // FAILURE can accompany SUCCESSFUL for an expected-panic harness.
            "SUCCESS" | "FAILURE" | "SATISFIED" | "UNSATISFIABLE" | "UNREACHABLE" | "COVERED"
            | "UNCOVERED" => {}
            _ => return missing("kani reported an unrecognized property status"),
        }
    }
    if summary.undetermined > 0 {
        return missing("kani did not determine every required cover property");
    }
    if summary.satisfied < summary.total {
        return HarnessStatus::Failed("a required cover property was not satisfied".to_string());
    }
    HarnessStatus::Verified
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HarnessResult {
    pub(crate) harness: Harness,
    pub(crate) status: HarnessStatus,
    pub(crate) time: String,
}

/// Whether the lane as a whole is acceptable.
pub(crate) fn lane_verdict(
    results: &[HarnessResult],
    tool_available: bool,
    allow_empty: bool,
) -> Result<(), String> {
    if !tool_available {
        return Err("NOT-ESTABLISHED: cargo-kani is not installed or not runnable".to_string());
    }
    if results.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(
                "NOT-ESTABLISHED: zero kani harnesses found (pass --allow-empty to accept)"
                    .to_string(),
            )
        };
    }
    let failed = results
        .iter()
        .filter(|result| matches!(result.status, HarnessStatus::Failed(_)))
        .count();
    let unestablished = results
        .iter()
        .filter(|result| matches!(result.status, HarnessStatus::NotEstablished(_)))
        .count();
    if failed > 0 || unestablished > 0 {
        return Err(format!(
            "kani lane not clean: {failed} failed, {unestablished} not established of {} harnesses",
            results.len()
        ));
    }
    Ok(())
}

pub(crate) fn render_report(results: &[HarnessResult], note: &str) -> String {
    let mut out = String::from("# Kani lane (RIPR-SPEC-0371)\n\n");
    out.push_str("Bounded model checking of pure cores. A VERIFIED row means no violation within the harness bounds, not a full-program or runtime-mutation claim.\n\n");
    if !note.is_empty() {
        out.push_str(&format!("{note}\n\n"));
    }
    out.push_str(
        "| Harness | File | Status | Detail | Kani time |\n| --- | --- | --- | --- | ---: |\n",
    );
    for result in results {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} |\n",
            machine::escape(&result.harness.name),
            machine::escape(&result.harness.file),
            result.status.label(),
            machine::escape(result.status.detail()),
            machine::escape(&result.time)
        ));
    }
    out
}

fn collect_sources(root: &Path, files: &mut Vec<(String, String)>) -> Result<(), String> {
    let entries =
        fs::read_dir(root).map_err(|err| format!("failed to read {}: {err}", root.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|err| format!("failed to read entry in {}: {err}", root.display()))?
            .path();
        if path.is_dir() {
            collect_sources(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            if text.contains("#[kani::proof") {
                files.push((path.to_string_lossy().replace('\\', "/"), text));
            }
        }
    }
    Ok(())
}

fn tool_available(report: &mut machine::Report) -> bool {
    let started = Instant::now();
    let argv = vec!["kani".to_string(), "--version".to_string()];
    // Keep the existing preflight executor. #12581 owns its bounded successor;
    // timeout_ms=null explicitly discloses this precursor's missing deadline.
    match run::capture_output("cargo", &["kani", "--version"], "cargo kani --version") {
        Ok(captured) => {
            let available = captured.status.success();
            report.capture(
                machine::InvocationContext {
                    package: "@preflight",
                    argv: &argv,
                    budget: None,
                    elapsed: started.elapsed(),
                    harnesses: &[],
                },
                machine::CaptureOutcome {
                    exit_code: captured.status.code(),
                    exit_success: Some(available),
                    timed_out: false,
                    stdout: Some(&captured.stdout),
                    stderr: Some(&captured.stderr),
                    error: None,
                },
            );
            available
        }
        Err(error) => {
            report.capture(
                machine::InvocationContext {
                    package: "@preflight",
                    argv: &argv,
                    budget: None,
                    elapsed: started.elapsed(),
                    harnesses: &[],
                },
                machine::CaptureOutcome {
                    exit_code: None,
                    exit_success: None,
                    timed_out: false,
                    stdout: None,
                    stderr: None,
                    error: Some(&error),
                },
            );
            false
        }
    }
}

/// Splits Kani's output into `(qualified harness name, block text)` pairs at
/// each `Checking harness <name>...` header.
pub(crate) fn split_harness_blocks(output: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Checking harness ") {
            let name = rest.trim_end().trim_end_matches("...").to_string();
            blocks.push((name, String::new()));
        } else if let Some((_, text)) = blocks.last_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    blocks
}

fn verification_time(block: &str) -> String {
    block
        .lines()
        .find_map(|line| line.trim().strip_prefix("Verification Time:"))
        .map_or_else(|| "n/a".to_string(), |time| time.trim().to_string())
}

fn matches_harness(qualified: &str, harness: &Harness) -> bool {
    qualified == harness.name || qualified.ends_with(&format!("::{}", harness.name))
}

/// Joins discovered harnesses to Kani's result blocks. A missing or ambiguous
/// name-based relationship is `NOT-ESTABLISHED`, never assumed verified.
/// Source paths do not establish native module identity.
pub(crate) fn results_from_output(harnesses: &[Harness], output: &str) -> Vec<HarnessResult> {
    let blocks = split_harness_blocks(output);
    harnesses
        .iter()
        .map(|harness| {
            let duplicate_name = harnesses
                .iter()
                .filter(|other| other.name == harness.name)
                .nth(1)
                .is_some();
            let mut matching = blocks
                .iter()
                .filter(|(name, _)| matches_harness(name, harness));
            let first = matching.next();
            let (status, time) = if duplicate_name {
                (
                    HarnessStatus::NotEstablished(
                        "multiple discovered harnesses share this name; result identity is ambiguous"
                            .to_string(),
                    ),
                    "n/a".to_string(),
                )
            } else if matching.next().is_some() {
                (
                    HarnessStatus::NotEstablished(
                        "kani printed multiple matching result blocks; result identity is ambiguous"
                            .to_string(),
                    ),
                    "n/a".to_string(),
                )
            } else {
                match first {
                    Some((_, text)) => (classify_output(text), verification_time(text)),
                    None => (
                        HarnessStatus::NotEstablished(
                            "kani printed no result block for this harness".to_string(),
                        ),
                        "n/a".to_string(),
                    ),
                }
            };
            HarnessResult {
                harness: harness.clone(),
                status,
                time,
            }
        })
        .collect()
}

fn all_not_established(harnesses: &[Harness], reason: &str) -> Vec<HarnessResult> {
    harnesses
        .iter()
        .map(|harness| HarnessResult {
            harness: harness.clone(),
            status: HarnessStatus::NotEstablished(reason.to_string()),
            time: "n/a".to_string(),
        })
        .collect()
}

/// The exact `cargo` arguments for the lane, kept separate so a test can pin
/// what Kani actually receives.
fn kani_command(args: &KaniArgs, package: &str) -> Vec<String> {
    let mut command = vec!["kani".to_string(), "-p".to_string(), package.to_string()];
    if let Some(wanted) = &args.harness {
        command.push("--harness".to_string());
        command.push(wanted.clone());
    }
    if let Some(unwind) = args.default_unwind {
        command.push("--default-unwind".to_string());
        command.push(unwind.to_string());
    }
    // `--harness-timeout` is gated behind `-Z unstable-options` in Kani 0.68.
    command.push("-Z".to_string());
    command.push("unstable-options".to_string());
    command.push("--harness-timeout".to_string());
    command.push(format!("{}s", args.harness_timeout_secs));
    command
}

/// A process outcome is not an individual harness verdict.
struct KaniRun {
    results: Vec<HarnessResult>,
    process_problem: Option<String>,
}

fn process_completion_problem(
    timed_out: bool,
    exit_success: Option<bool>,
    exit_label: &str,
    timeout_secs: u64,
) -> Option<String> {
    if timed_out {
        return Some(format!("cargo kani timed out after {timeout_secs} seconds"));
    }
    match exit_success {
        Some(true) => None,
        Some(false) => Some(format!(
            "cargo kani exited with {exit_label}; text verdicts do not establish a successful run"
        )),
        None => Some("cargo kani returned without a process exit status".to_string()),
    }
}

/// Keep independently observed rows even when a run fails or is incomplete.
fn reconcile_kani_output(
    harnesses: &[Harness],
    output: &str,
    process_problem: Option<String>,
) -> KaniRun {
    KaniRun {
        results: results_from_output(harnesses, output),
        process_problem,
    }
}

/// A failed or incomplete native process cannot be hidden by success-looking rows.
fn complete_lane_verdict(
    results: &[HarnessResult],
    tool_available: bool,
    allow_empty: bool,
    process_problem: Option<&str>,
) -> Result<(), String> {
    let rows = lane_verdict(results, tool_available, allow_empty);
    match (rows, process_problem) {
        (Ok(()), None) => Ok(()),
        (Err(reason), None) => Err(reason),
        (Ok(()), Some(problem)) => Err(format!("NOT-ESTABLISHED: {problem}")),
        (Err(reason), Some(problem)) => Err(format!("NOT-ESTABLISHED: {problem}; {reason}")),
    }
}

fn run_kani(
    harnesses: &[Harness],
    args: &KaniArgs,
    package: &str,
    timeout: Duration,
    report: &mut machine::Report,
) -> KaniRun {
    let command = kani_command(args, package);
    let started = Instant::now();
    let output =
        match run::capture_output_with_timeout("cargo", &command, &[], timeout, "cargo kani") {
            Err(error) => {
                report.capture(
                    machine::InvocationContext {
                        package,
                        argv: &command,
                        budget: Some(timeout),
                        elapsed: started.elapsed(),
                        harnesses,
                    },
                    machine::CaptureOutcome {
                        exit_code: None,
                        exit_success: None,
                        timed_out: false,
                        stdout: None,
                        stderr: None,
                        error: Some(&error),
                    },
                );
                return KaniRun {
                    results: all_not_established(harnesses, &error),
                    process_problem: Some(error),
                };
            }
            Ok(output) => output,
        };
    let exit_success = output.status.as_ref().map(|status| status.success());
    let exit_label = output
        .status
        .as_ref()
        .map_or_else(String::new, ToString::to_string);
    let problem = process_completion_problem(
        output.timed_out,
        exit_success,
        &exit_label,
        // The budget this invocation actually received, not the whole-run total.
        timeout.as_secs(),
    );
    report.capture(
        machine::InvocationContext {
            package,
            argv: &command,
            budget: Some(timeout),
            elapsed: started.elapsed(),
            harnesses,
        },
        machine::CaptureOutcome {
            exit_code: output.status.as_ref().and_then(|status| status.code()),
            exit_success,
            timed_out: output.timed_out,
            stdout: Some(&output.stdout),
            stderr: Some(&output.stderr),
            error: None,
        },
    );
    let combined = format!("{}\n{}", output.stdout, output.stderr);
    reconcile_kani_output(harnesses, &combined, problem)
}

pub(crate) fn run_lane(args: &[String]) -> Result<(), String> {
    // Publish a non-success marker before discovery or any child process. A
    // subsequent interruption cannot leave the previous run as this run's result.
    let (mut journal, mut report) = machine::Journal::start(Path::new(REPORT_DIR))?;
    eprintln!("kani: run ID {}", report.run_id());
    let lane = run_lane_observed(args, &mut report, &mut journal);
    report.recheck_sources();
    let verdict = report.finish(lane);
    let markdown = journal.publish(&report)?;
    println!("{markdown}");
    verdict
}

fn run_lane_observed(
    args: &[String],
    report: &mut machine::Report,
    journal: &mut machine::Journal,
) -> Result<(), String> {
    let args = parse_args(args)?;
    report.request(&args);
    let mut packages = Vec::new();
    for (package, root) in HARNESS_PACKAGES {
        let mut files = Vec::new();
        collect_sources(Path::new(root), &mut files)?;
        let mut harnesses = discover_harnesses(&files);
        if let Some(wanted) = &args.harness {
            harnesses.retain(|harness| harness.name.contains(wanted.as_str()));
        }
        report.select(package, &files, &harnesses)?;
        packages.push((package, harnesses));
    }
    journal.publish(report)?;
    let count: usize = packages.iter().map(|(_, harnesses)| harnesses.len()).sum();
    let available = tool_available(report);
    report.availability(available);
    journal.publish(report)?;
    let run = if available && count > 0 {
        eprintln!("kani: checking {count} harness(es)");
        let deadline = Instant::now() + Duration::from_secs(args.timeout_secs);
        let mut results = Vec::new();
        let mut problems = Vec::new();
        for (package, harnesses) in &packages {
            if harnesses.is_empty() {
                continue;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let run = if remaining.is_zero() {
                KaniRun {
                    results: all_not_established(harnesses, "whole-run deadline exhausted"),
                    process_problem: Some("whole-run deadline exhausted".to_string()),
                }
            } else {
                run_kani(harnesses, &args, package, remaining, report)
            };
            report.rows(package, &run.results);
            journal.publish(report)?;
            results.extend(run.results);
            if let Some(problem) = run.process_problem {
                problems.push(format!("{package}: {problem}"));
            }
        }
        KaniRun {
            results,
            process_problem: if problems.is_empty() {
                None
            } else {
                Some(problems.join("; "))
            },
        }
    } else {
        let mut results = Vec::new();
        for (package, harnesses) in &packages {
            let rows =
                all_not_established(harnesses, "tool unavailable; verification did not start");
            report.rows(package, &rows);
            results.extend(rows);
        }
        KaniRun {
            results,
            process_problem: None,
        }
    };
    complete_lane_verdict(
        &run.results,
        available,
        args.allow_empty,
        run.process_problem.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn result(status: HarnessStatus) -> HarnessResult {
        HarnessResult {
            harness: Harness {
                file: "a.rs".to_string(),
                name: "h".to_string(),
            },
            status,
            time: "1s".to_string(),
        }
    }

    #[test]
    fn discovery_retains_domain_proofs_and_ripr_smoke() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("missing workspace root")?;
        let mut by_package = Vec::new();
        for (package, path) in HARNESS_PACKAGES {
            let mut files = Vec::new();
            collect_sources(&root.join(path), &mut files)?;
            by_package.push((package, discover_harnesses(&files)));
        }
        let contract = &by_package[1].1;
        for name in [
            "currentness_action_and_base_side_are_disjoint",
            "from_probe_delta_never_upgrades_without_after",
            "stop_reason_for_unknown_class_is_exactly_the_unknown_classes",
            "probe_family_wire_roundtrip",
            "reason_judgment_mapping_is_total_and_note_rule_holds",
            "judgment_label_roundtrip",
            "reason_label_roundtrip",
            "classify_reference_is_total_and_fail_closed",
            "unavailable_snapshot_is_always_unknown",
            "same_identity_decision_table",
            "identity_mismatch_is_unknown",
            "compare_is_total_and_unknown_is_never_fixture_backed",
            "gap_state_parse_roundtrip_and_unknown_fallback",
            "unknown_gap_input_never_becomes_causal",
            "result_state_satisfaction_matches_contract",
            "substitute_outcome_execution_partition_matches_contract",
        ] {
            assert_eq!(
                contract.iter().filter(|h| h.name == name).count(),
                1,
                "{name}"
            );
        }
        assert!(
            by_package[0]
                .1
                .iter()
                .any(|h| h.file.ends_with("kani_smoke.rs"))
        );
        Ok(())
    }

    #[test]
    fn discovery_finds_named_harnesses_past_attributes_and_docs() {
        let source = "#[kani::proof]\nfn one() {}\n\n#[kani::proof]\n#[kani::unwind(4)]\n/// doc\nfn two() {}\nfn not_a_harness() {}\n";
        let found = discover_harnesses(&[("x.rs".to_string(), source.to_string())]);
        let names: Vec<&str> = found.iter().map(|harness| harness.name.as_str()).collect();
        assert_eq!(names, vec!["one", "two"]);
    }

    #[test]
    fn discovery_ignores_harness_text_inside_raw_string_fixtures() {
        let source = "let f = r#\"\n#[kani::proof]\nfn fixture_only() {}\n\"#;\n\n#[kani::proof]\nfn real() {}\n";
        let found = discover_harnesses(&[("x.rs".to_string(), source.to_string())]);
        let names: Vec<&str> = found.iter().map(|harness| harness.name.as_str()).collect();
        assert_eq!(names, vec!["real"]);
    }

    #[test]
    fn classification_requires_an_explicit_success_verdict() {
        let ok = "** 2 of 2 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n";
        assert_eq!(classify_output(ok), HarnessStatus::Verified);
        assert!(matches!(
            classify_output("VERIFICATION:- FAILED"),
            HarnessStatus::Failed(_)
        ));
        // Absence of a verdict is never a pass, even with a zero exit.
        assert!(matches!(
            classify_output("compiled fine"),
            HarnessStatus::NotEstablished(_)
        ));
        assert!(matches!(
            classify_output("No proof harnesses (functions with #[kani::proof]) were found"),
            HarnessStatus::NotEstablished(_)
        ));
    }

    #[test]
    fn classification_requires_positive_complete_cover_evidence() {
        for summary in [
            "",
            " ** 0 of 0 cover properties satisfied\n",
            " ** invalid of 1 cover properties satisfied\n",
            " ** 1 of invalid cover properties satisfied\n",
            " ** 2 of 1 cover properties satisfied\n",
            " ** 1 / 1 cover properties satisfied\n",
            " ** +1 of 1 cover properties satisfied\n",
            " ** 18446744073709551616 of 18446744073709551616 cover properties satisfied\n",
            " ** 1 of 1 cover properties satisfied (1 undetermined)\n",
            " ** 1 of 1 cover properties satisfied (1 unreachable)\n",
            " ** 1 of 1 cover properties satisfied (0 undetermined)\n",
            " ** 1 of 1 cover properties satisfied (unknown)\n",
            " ** 1 of 1 cover properties satisfied unexpectedly\n",
            " ** 0 of 2 cover properties satisfied (1 unreachable,1 undetermined)\n",
            " ** 0 of 2 cover properties satisfied (1 unreachable,1 unreachable)\n",
            " ** 0 of 2 cover properties satisfied (1 undetermined,1 undetermined)\n",
            " ** 0 of 18446744073709551615 cover properties satisfied (18446744073709551615 undetermined,1 unreachable)\n",
            " ** 1 of 1 cover properties satisfied\n ** 1 of 1 cover properties satisfied\n",
            " ** 1 of 1 cover properties satisfied\n ** 0 of 1 cover properties satisfied\n",
            " ** invalid of 1 cover properties satisfied\n ** 1 of 1 cover properties satisfied\n",
        ] {
            let output = format!("{summary}VERIFICATION:- SUCCESSFUL\n");
            assert!(
                matches!(classify_output(&output), HarnessStatus::NotEstablished(_)),
                "missing or ambiguous cover evidence was accepted: {output:?}",
            );
        }
    }

    #[test]
    fn undetermined_properties_never_become_verified() {
        for evidence in [
            " ** 1 of 2 cover properties satisfied (1 undetermined)\n",
            " ** 1 of 3 cover properties satisfied (1 undetermined,1 unreachable)\n",
            "Check 1: proof.cover.1\n\t - Status: UNDETERMINED\n ** 1 of 1 cover properties satisfied\n",
            "Check 1: proof.cover.1\n\t - Status: ERROR\n ** 1 of 1 cover properties satisfied\n",
            "Check 1: proof.cover.1\n\t - Status: UNRECOGNIZED\n ** 1 of 1 cover properties satisfied\n",
        ] {
            let output = format!("{evidence}VERIFICATION:- SUCCESSFUL\n");
            assert!(
                matches!(classify_output(&output), HarnessStatus::NotEstablished(_)),
                "undetermined property evidence was accepted: {output:?}",
            );
        }
    }

    #[test]
    fn complete_cover_evidence_allows_unreachable_ordinary_assertions() {
        for properties in [
            "Check 1: proof.assertion.1\n\t - Status: UNREACHABLE\nCheck 2: proof.cover.1\n\t - Status: SATISFIED\n",
            "Check 1: proof.cover.1\n\t - Status: SATISFIED\nCheck 2: proof.assertion.1\n\t - Status: UNREACHABLE\n",
            "Check 1: proof.cover.1\n\t - Status: SATISFIED\nCheck 2: proof.cover.1.assertion.2\n\t - Status: UNREACHABLE\n",
        ] {
            let output = format!(
                "{properties} ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n"
            );
            assert_eq!(classify_output(&output), HarnessStatus::Verified);
        }
    }

    #[test]
    fn complete_cover_evidence_preserves_expected_panic_success() {
        let output = "Check 1: proof.assertion.1\n\t - Status: FAILURE\nCheck 2: proof.cover.1\n\t - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL (encountered one or more panics as expected)\n";
        assert_eq!(classify_output(output), HarnessStatus::Verified);
    }

    #[test]
    fn cover_property_and_summary_must_not_contradict() {
        for property in ["proof.cover.1", "module::proof.cover.2", "cover.1"] {
            for status in [
                "UNSATISFIABLE",
                "UNREACHABLE",
                "SUCCESS",
                "FAILURE",
                "COVERED",
                "UNCOVERED",
            ] {
                let output = format!(
                    "Check 1: {property}\n\t - Status: {status}\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n"
                );
                assert!(
                    matches!(classify_output(&output), HarnessStatus::NotEstablished(_)),
                    "a cover status cannot be overridden by a complete summary: {output:?}",
                );
            }
        }
    }

    #[test]
    fn pinned_native_cover_captures_keep_distinct_lane_dispositions() {
        // Unmodified output captured from the pinned native Kani 0.68.0. This
        // test replays bytes, not the verifier; see the fixture provenance.
        for (name, stdout, stderr, expected) in [
            (
                "cover_pass_compact",
                include_str!("../tests/fixtures/kani-cover-evidence/cover-pass-compact.stdout.txt"),
                include_str!("../tests/fixtures/kani-cover-evidence/cover-pass-compact.stderr.txt"),
                "VERIFIED",
            ),
            (
                "cover_overconstrained",
                include_str!("../tests/fixtures/kani-cover-evidence/cover-fail.stdout.txt"),
                include_str!("../tests/fixtures/kani-cover-evidence/cover-fail.stderr.txt"),
                "FAILED",
            ),
            (
                "cover_unreachable",
                include_str!("../tests/fixtures/kani-cover-evidence/cover-unreachable.stdout.txt"),
                include_str!("../tests/fixtures/kani-cover-evidence/cover-unreachable.stderr.txt"),
                "FAILED",
            ),
            (
                "cover_undetermined_compact",
                include_str!(
                    "../tests/fixtures/kani-cover-evidence/cover-undetermined-compact.stdout.txt"
                ),
                include_str!(
                    "../tests/fixtures/kani-cover-evidence/cover-undetermined-compact.stderr.txt"
                ),
                // This native run fails verification before cover admission;
                // success-looking undetermined output has separate controls.
                "FAILED",
            ),
            (
                "no_cover_control",
                include_str!("../tests/fixtures/kani-cover-evidence/no-cover.stdout.txt"),
                include_str!("../tests/fixtures/kani-cover-evidence/no-cover.stderr.txt"),
                "NOT-ESTABLISHED",
            ),
        ] {
            let output = format!("{stdout}\n{stderr}");
            let harnesses = [Harness {
                file: "native-fixture.rs".to_string(),
                name: name.to_string(),
            }];
            assert_eq!(split_harness_blocks(&output).len(), 1, "{name}");
            let rows = results_from_output(&harnesses, &output);
            assert_eq!(rows.len(), 1, "{name}");
            assert_eq!(rows[0].status.label(), expected, "{name}");
        }
    }

    #[test]
    fn a_solver_timeout_is_not_a_counterexample() {
        // Kani reports a timed-out harness as FAILED; it has no verdict.
        let timed_out = "CBMC failed\nVERIFICATION:- FAILED\nCBMC timed out. You may want to rerun your proof with a larger timeout\n";
        assert!(matches!(
            classify_output(timed_out),
            HarnessStatus::NotEstablished(_)
        ));
        // A real counterexample without the timeout text stays FAILED.
        assert!(matches!(
            classify_output("VERIFICATION:- FAILED\n"),
            HarnessStatus::Failed(_)
        ));
    }

    #[test]
    fn the_per_harness_timeout_reaches_the_kani_command() {
        let parsed = parse_args(&strings(&["--harness-timeout-secs", "60"]));
        assert!(parsed.is_ok());
        let command = parsed
            .map(|args| kani_command(&args, "ripr"))
            .unwrap_or_default();
        let position = command.iter().position(|arg| arg == "--harness-timeout");
        assert_eq!(
            position
                .and_then(|index| command.get(index + 1))
                .map(String::as_str),
            Some("60s")
        );
        let gate = command.iter().position(|arg| arg == "unstable-options");
        assert!(gate.is_some() && gate < position);
    }

    #[test]
    fn harness_timeout_flag_parses() {
        let parsed = parse_args(&strings(&["--harness-timeout-secs", "60"]));
        assert_eq!(parsed.map(|args| args.harness_timeout_secs), Ok(60));
        assert!(
            parse_args(&strings(&["--harness-timeout-secs", "0"]))
                .err()
                .is_some()
        );
    }

    #[test]
    fn unsatisfied_cover_fails_the_reachability_policy() {
        for output in [
            " ** 1 of 2 cover properties satisfied (1 unreachable)\nVERIFICATION:- SUCCESSFUL\n",
            "Check 1: proof.cover.1\n\t - Status: UNSATISFIABLE\n ** 0 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n",
            " ** 1 of 2 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n",
        ] {
            assert_eq!(
                classify_output(output),
                HarnessStatus::Failed("a required cover property was not satisfied".to_string()),
            );
        }
    }

    #[test]
    fn lane_never_passes_by_absence() {
        assert!(lane_verdict(&[], false, true).is_err());
        assert!(lane_verdict(&[], true, false).is_err());
        assert_eq!(lane_verdict(&[], true, true).err(), None);
        assert_eq!(
            lane_verdict(&[result(HarnessStatus::Verified)], true, false).err(),
            None
        );
        let failed = result(HarnessStatus::Failed("x".to_string()));
        assert!(lane_verdict(&[failed], true, true).is_err());
        let skipped = result(HarnessStatus::NotEstablished("x".to_string()));
        assert!(lane_verdict(&[skipped], true, true).is_err());
    }

    #[test]
    fn arguments_parse_and_reject_unknown_flags() {
        let parsed = parse_args(&strings(&["--allow-empty", "--default-unwind", "8"]));
        assert_eq!(
            parsed,
            Ok(KaniArgs {
                allow_empty: true,
                harness: None,
                default_unwind: Some(8),
                timeout_secs: DEFAULT_TIMEOUT_SECS,
                harness_timeout_secs: DEFAULT_HARNESS_TIMEOUT_SECS,
            })
        );
        assert!(parse_args(&strings(&["--bogus"])).err().is_some());
        assert!(
            parse_args(&strings(&["--default-unwind", "0"]))
                .err()
                .is_some()
        );
        assert!(parse_args(&strings(&["--harness"])).err().is_some());
    }

    #[test]
    fn blocks_join_to_harnesses_and_a_missing_block_is_not_established() {
        let output = "Checking harness m::one...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\nChecking harness m::two...\nVERIFICATION:- FAILED\n";
        let harness = |name: &str| Harness {
            file: "f.rs".to_string(),
            name: name.to_string(),
        };
        let harnesses = [harness("one"), harness("two"), harness("three")];
        let results = results_from_output(&harnesses, output);
        let statuses: Vec<&HarnessStatus> = results.iter().map(|r| &r.status).collect();
        assert!(matches!(statuses.first(), Some(HarnessStatus::Verified)));
        assert!(matches!(statuses.get(1), Some(HarnessStatus::Failed(_))));
        assert!(matches!(
            statuses.get(2),
            Some(HarnessStatus::NotEstablished(_))
        ));
        assert_eq!(results.first().map(|r| r.time.as_str()), Some("0.5s"));
    }

    #[test]
    fn duplicate_source_names_never_share_a_native_result() {
        let success = "Checking harness a::same...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n";
        let failure = "Checking harness b::same...\nVERIFICATION:- FAILED\n";
        let unique = "Checking harness m::unique...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n";
        // Duplicate names in separate files and in one file are both ambiguous:
        // the current discovery model does not carry native module identities.
        for second_file in ["a.rs", "b.rs"] {
            let harnesses = [
                Harness {
                    file: "a.rs".to_string(),
                    name: "same".to_string(),
                },
                Harness {
                    file: second_file.to_string(),
                    name: "same".to_string(),
                },
                Harness {
                    file: "unique.rs".to_string(),
                    name: "unique".to_string(),
                },
            ];
            for output in [
                format!("{success}{unique}"),
                format!("{success}{failure}{unique}"),
                format!("{failure}{success}{unique}"),
            ] {
                let results = results_from_output(&harnesses, &output);
                assert_eq!(results.len(), 3);
                for (row, expected) in results.iter().zip(&harnesses).take(2) {
                    assert_eq!(&row.harness, expected);
                    assert_eq!(
                        row.status,
                        HarnessStatus::NotEstablished(
                            "multiple discovered harnesses share this name; result identity is ambiguous"
                                .to_string(),
                        ),
                    );
                    assert_eq!(row.time, "n/a");
                }
                assert!(matches!(
                    results.get(2).map(|row| &row.status),
                    Some(HarnessStatus::Verified)
                ));
                for allow_empty in [false, true] {
                    assert!(lane_verdict(&results, true, allow_empty).is_err());
                }
                let report = render_report(&results, "");
                assert!(report.contains("| `same` | `a.rs` | NOT-ESTABLISHED |"));
                assert!(
                    report.contains(&format!("| `same` | `{second_file}` | NOT-ESTABLISHED |"))
                );
                assert!(!report.contains("| `same` | `a.rs` | VERIFIED |"));
                assert!(report.contains("| `unique` | `unique.rs` | VERIFIED |"));
            }
        }
    }

    #[test]
    fn duplicate_native_blocks_are_not_first_match_wins() {
        let harnesses = [Harness {
            file: "one.rs".to_string(),
            name: "same".to_string(),
        }];
        let success = "Checking harness a::same...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n";
        let other_failure = "Checking harness b::same...\nVERIFICATION:- FAILED\n";
        let repeated_failure = "Checking harness a::same...\nVERIFICATION:- FAILED\n";
        for output in [
            format!("{success}{other_failure}"),
            format!("{other_failure}{success}"),
            format!("{success}{repeated_failure}"),
            format!("{repeated_failure}{success}"),
            format!("{success}{success}"),
        ] {
            let results = results_from_output(&harnesses, &output);
            assert_eq!(results.len(), 1);
            assert_eq!(
                results.first().map(|row| &row.status),
                Some(&HarnessStatus::NotEstablished(
                    "kani printed multiple matching result blocks; result identity is ambiguous"
                        .to_string(),
                )),
            );
            assert_eq!(results.first().map(|row| row.time.as_str()), Some("n/a"));
            assert!(lane_verdict(&results, true, false).is_err());
        }
    }

    #[test]
    fn unique_native_join_retains_status_time_and_separator_boundary() {
        let harnesses = [Harness {
            file: "one.rs".to_string(),
            name: "same".to_string(),
        }];
        for selector in ["same", "m::same"] {
            let output = format!(
                "Checking harness {selector}...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n"
            );
            let results = results_from_output(&harnesses, &output);
            assert_eq!(results.len(), 1);
            assert!(matches!(
                results.first().map(|row| &row.status),
                Some(HarnessStatus::Verified)
            ));
            assert_eq!(results.first().map(|row| row.time.as_str()), Some("0.5s"));
            assert_eq!(lane_verdict(&results, true, false).err(), None);
        }
        for selector in ["not_same", "m::notsame", "same::nested"] {
            let output = format!(
                "Checking harness {selector}...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n"
            );
            let results = results_from_output(&harnesses, &output);
            assert!(matches!(
                results.first().map(|row| &row.status),
                Some(HarnessStatus::NotEstablished(_))
            ));
            assert!(lane_verdict(&results, true, false).is_err());
        }
        let output = "Checking harness m::same...\nVERIFICATION:- FAILED\nCBMC timed out\n";
        let results = results_from_output(&harnesses, output);
        assert!(matches!(
            results.first().map(|row| &row.status),
            Some(HarnessStatus::NotEstablished(_))
        ));
        assert!(lane_verdict(&results, true, false).is_err());
    }

    #[test]
    fn native_process_error_cannot_be_hidden_by_verified_rows() {
        let harnesses = [Harness {
            file: "proof.rs".to_string(),
            name: "proof".to_string(),
        }];
        let output = "Checking harness m::proof...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 1s\n";
        let success = reconcile_kani_output(&harnesses, output, None);
        assert_eq!(
            complete_lane_verdict(&success.results, true, false, None).err(),
            None
        );

        for (timed_out, status, expected) in [
            (false, Some(false), "exited"),
            (false, None, "without a process exit status"),
            (true, Some(true), "timed out"),
        ] {
            let problem = process_completion_problem(timed_out, status, "exit status: 1", 5);
            assert!(
                problem
                    .as_deref()
                    .is_some_and(|text| text.contains(expected))
            );
            let run = reconcile_kani_output(&harnesses, output, problem);
            assert!(matches!(&run.results[0].status, HarnessStatus::Verified));
            assert_eq!(run.results[0].time, "1s");
            assert!(
                render_report(&run.results, run.process_problem.as_deref().unwrap_or(""))
                    .contains(expected)
            );
            for allow_empty in [false, true] {
                assert!(
                    complete_lane_verdict(
                        &run.results,
                        true,
                        allow_empty,
                        run.process_problem.as_deref(),
                    )
                    .is_err()
                );
            }
        }
        assert!(process_completion_problem(false, Some(true), "exit status: 0", 5).is_none());
    }

    #[test]
    fn partial_timeout_preserves_completed_rows_but_never_passes() -> Result<(), String> {
        let harnesses = [
            Harness {
                file: "first.rs".to_string(),
                name: "first".to_string(),
            },
            Harness {
                file: "second.rs".to_string(),
                name: "second".to_string(),
            },
        ];
        let text = "Checking harness a::first...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 2s\nChecking harness b::second...\nCompiling\n";
        let problem = process_completion_problem(true, None, "", 10);
        let run = reconcile_kani_output(&harnesses, text, problem);
        assert_eq!(run.results.len(), 2);
        assert!(matches!(&run.results[0].status, HarnessStatus::Verified));
        assert!(matches!(
            &run.results[1].status,
            HarnessStatus::NotEstablished(_)
        ));
        assert_eq!(run.results[1].time, "n/a");
        assert!(
            render_report(&run.results, "NOT-ESTABLISHED: process timed out").contains("second.rs")
        );
        let error =
            complete_lane_verdict(&run.results, true, false, run.process_problem.as_deref())
                .err()
                .ok_or("partial timeout cannot pass")?;
        assert!(error.contains("timed out"));
        assert!(error.contains("1 not established"));
        Ok(())
    }

    #[test]
    fn mixed_failure_and_absent_results_keep_distinct_dispositions() -> Result<(), String> {
        let harnesses = [
            Harness {
                file: "first.rs".to_string(),
                name: "first".to_string(),
            },
            Harness {
                file: "second.rs".to_string(),
                name: "second".to_string(),
            },
        ];
        let mixed = reconcile_kani_output(
            &harnesses,
            "Checking harness a::first...\nVERIFICATION:- FAILED\nChecking harness b::second...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n",
            Some("cargo kani exited with exit status: 1".to_string()),
        );
        assert!(matches!(&mixed.results[0].status, HarnessStatus::Failed(_)));
        assert!(matches!(&mixed.results[1].status, HarnessStatus::Verified));
        let err = complete_lane_verdict(
            &mixed.results,
            true,
            false,
            mixed.process_problem.as_deref(),
        )
        .err()
        .ok_or("failed harness must fail aggregate")?;
        assert!(err.contains("1 failed"));

        let absent = reconcile_kani_output(&harnesses, "", None);
        assert!(
            absent
                .results
                .iter()
                .all(|row| matches!(&row.status, HarnessStatus::NotEstablished(_)))
        );
        assert!(complete_lane_verdict(&absent.results, true, false, None).is_err());
        assert!(complete_lane_verdict(&[], true, true, Some("abnormal exit")).is_err());
        assert_eq!(complete_lane_verdict(&[], true, true, None).err(), None);
        Ok(())
    }

    #[test]
    fn report_names_every_status_word() {
        let results = [
            result(HarnessStatus::Verified),
            result(HarnessStatus::Failed("f".to_string())),
            result(HarnessStatus::NotEstablished("n".to_string())),
        ];
        let report = render_report(&results, "");
        for word in ["VERIFIED", "FAILED", "NOT-ESTABLISHED"] {
            assert!(report.contains(word), "{word} missing from {report}");
        }
    }
}
