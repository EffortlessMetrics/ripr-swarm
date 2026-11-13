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

pub(crate) use crate::formal_evidence::kani_output::{
    CoverSummary, Harness, HarnessResult, HarnessStatus, discover_harnesses, lane_verdict,
    matches_harness, parse_cover_summary, render_report, results_from_output, split_harness_blocks,
};
use crate::run;
use serde::{Deserialize, Serialize};

#[path = "kani_lane/machine.rs"]
mod machine;

const HARNESS_PACKAGES: [(&str, &str); 2] = [
    ("ripr", "crates/ripr/src"),
    ("ripr-contract", "crates/ripr-contract/src"),
];
const REPORT_DIR: &str = "target/ripr/reports";
/// Whole-run wall-clock deadline; a wedged solver is `NOT-ESTABLISHED`.
const DEFAULT_TIMEOUT_SECS: u64 = 3600;
/// An independent upper bound on the pre-run cargo-kani availability probe.
/// The caller's smaller requested run timeout also caps this preliminary step.
const MAX_KANI_VERSION_PROBE_SECS: u64 = 10;
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

/// One bounded version invocation for both legacy observation and canonical
/// capture. A subsecond remaining budget is preserved without rounding up.
fn capture_kani_version(
    cargo: &Path,
    root: &Path,
    budget: Duration,
) -> Result<run::ObservedByteCapture, run::TimedCaptureError> {
    run::capture_bytes_in_dir_with_env_budget_observed(
        cargo,
        &["kani".to_string(), "--version".to_string()],
        root,
        (&[("CARGO_TERM_COLOR", "never")], &[]),
        run::ByteCaptureBudget {
            timeout: budget.min(Duration::from_secs(MAX_KANI_VERSION_PROBE_SECS)),
            stdout_bytes: 4 * 1024 * 1024,
            stderr_bytes: 4 * 1024 * 1024,
        },
        "cargo kani --version",
    )
}

/// The legacy document deliberately drops the local capture capability and
/// retains its existing text projection. No second version process is started.
fn tool_available(
    timeout_secs: u64,
    report: &mut machine::Report,
) -> Result<run::TimedOutput, String> {
    let budget =
        Duration::from_secs(timeout_secs).min(Duration::from_secs(MAX_KANI_VERSION_PROBE_SECS));
    let started = Instant::now();
    let argv = ["kani".to_string(), "--version".to_string()];
    let captured = capture_kani_version(Path::new("cargo"), Path::new("."), budget)
        .map_err(|error| probe_capture_failure_reason(error.kind).to_string())
        .and_then(|observed| {
            observed.consume(|_, output, _, _, _| {
                let stdout = std::str::from_utf8(&output.stdout)
                    .map_err(|error| format!("cargo kani --version stdout is not UTF-8: {error}"))?
                    .to_string();
                let stderr = std::str::from_utf8(&output.stderr)
                    .map_err(|error| format!("cargo kani --version stderr is not UTF-8: {error}"))?
                    .to_string();
                Ok(run::TimedOutput {
                    status: output.status,
                    stdout,
                    stderr,
                    duration: output.duration,
                    timed_out: output.timed_out,
                })
            })
        });
    let outcome = match &captured {
        Ok(output) => machine::CaptureOutcome {
            exit_code: output.status.as_ref().and_then(|status| status.code()),
            exit_success: output.status.as_ref().map(|status| status.success()),
            timed_out: output.timed_out,
            stdout: Some(&output.stdout),
            stderr: Some(&output.stderr),
            error: None,
        },
        Err(error) => machine::CaptureOutcome {
            exit_code: None,
            exit_success: None,
            timed_out: false,
            stdout: None,
            stderr: None,
            error: Some(error),
        },
    };
    report.capture(
        machine::InvocationContext {
            package: "@preflight",
            argv: &argv,
            budget: Some(budget),
            elapsed: started.elapsed(),
            harnesses: &[],
        },
        outcome,
    );
    let output = captured?;
    admit_probe_output(output, budget)
}

fn admit_probe_output(
    output: run::TimedOutput,
    budget: Duration,
) -> Result<run::TimedOutput, String> {
    if output.timed_out {
        let units = if budget.as_secs() == 1 {
            "second"
        } else {
            "seconds"
        };
        return Err(format!(
            "cargo kani --version timed out after {} {units}",
            budget.as_secs()
        ));
    }
    // Do not echo untrusted version stdout/stderr into the Markdown report.
    match output.status {
        Some(status) if status.success() => Ok(output),
        Some(status)
            if status.code() == Some(101)
                && output
                    .stderr
                    .lines()
                    .any(|line| line.trim() == "error: no such command: `kani`") =>
        {
            Err("cargo is installed but the kani subcommand is unavailable".to_string())
        }
        Some(status) => Err(format!("cargo kani --version failed with {status}")),
        None => Err("cargo kani --version returned without a process exit status".to_string()),
    }
}

fn probe_capture_failure_reason(kind: run::TimedCaptureErrorKind) -> &'static str {
    use run::TimedCaptureErrorKind;
    match kind {
        TimedCaptureErrorKind::Launch(std::io::ErrorKind::NotFound) => {
            "cargo could not be launched: executable or interpreter not found"
        }
        TimedCaptureErrorKind::Launch(std::io::ErrorKind::PermissionDenied) => {
            "cargo could not be launched: permission denied"
        }
        TimedCaptureErrorKind::Launch(_) => "cargo could not be launched: other launch failure",
        TimedCaptureErrorKind::ProcessControl => {
            "cargo kani --version process observation or termination failed"
        }
        TimedCaptureErrorKind::Capture => {
            "cargo kani --version output capture failed or was incomplete"
        }
        TimedCaptureErrorKind::Cleanup => "cargo kani --version descendant cleanup failed",
    }
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
    let availability = tool_available(args.timeout_secs, report);
    let available = availability.is_ok();
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
        let reason = availability
            .as_ref()
            .err()
            .map(String::as_str)
            .unwrap_or("tool unavailable; verification did not start");
        for (package, harnesses) in &packages {
            let rows = all_not_established(harnesses, reason);
            report.rows(package, &rows);
            results.extend(rows);
        }
        KaniRun {
            results,
            process_problem: availability.err(),
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

    pub(super) fn refuses<T>(result: Result<T, String>, expected: &str) -> Result<(), String> {
        assert!(!expected.is_empty());
        let diagnostic = result
            .err()
            .ok_or_else(|| format!("expected refusal containing {expected}"))?;
        assert!(diagnostic.contains(expected), "{diagnostic}");
        Ok(())
    }

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

    #[cfg(target_os = "linux")]
    #[test]
    fn subsecond_probe_retains_one_actual_invocation_and_exact_budget() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = std::env::temp_dir();
        fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let root = temporary.join(format!(
            "kani-subsecond-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let result = (|| {
            let cargo = root.join("cargo");
            fs::write(
                &cargo,
                b"#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> calls\n[ \"${CARGO_TERM_COLOR-}\" = never ] || exit 96\nexec /bin/sleep 30\n",
            )
            .map_err(|error| error.to_string())?;
            fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755))
                .map_err(|error| error.to_string())?;
            let expected = Duration::from_millis(200);
            let observed = capture_kani_version(&cargo, &root, expected)
                .map_err(|error| format!("subsecond probe capture: {error:?}"))?;
            observed.consume(|request, output, budget, _, _| {
                assert_eq!(request.program, cargo);
                assert_eq!(request.cwd, root);
                assert_eq!(
                    request.arguments,
                    ["kani", "--version"].map(std::ffi::OsString::from)
                );
                assert_eq!(budget.timeout, expected);
                assert!(output.timed_out);
            });
            assert_eq!(
                fs::read_to_string(root.join("calls")).map_err(|error| error.to_string())?,
                "kani --version\n"
            );
            Ok(())
        })();
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        result
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
    fn the_per_harness_timeout_reaches_the_kani_command() -> Result<(), String> {
        let parsed = parse_args(&strings(&["--harness-timeout-secs", "60"]))?;
        let command = kani_command(&parsed, "ripr");
        let position = command.iter().position(|arg| arg == "--harness-timeout");
        assert_eq!(
            position
                .and_then(|index| command.get(index + 1))
                .map(String::as_str),
            Some("60s")
        );
        let gate = command.iter().position(|arg| arg == "unstable-options");
        assert!(gate.is_some() && gate < position);
        Ok(())
    }

    #[test]
    fn harness_timeout_flag_parses() -> Result<(), String> {
        let parsed = parse_args(&strings(&["--harness-timeout-secs", "60"]));
        assert_eq!(parsed.map(|args| args.harness_timeout_secs), Ok(60));
        refuses(
            parse_args(&strings(&["--harness-timeout-secs", "0"])),
            "--harness-timeout-secs requires a positive integer",
        )
    }

    #[test]
    fn lane_never_passes_by_absence() -> Result<(), String> {
        refuses(
            lane_verdict(&[], false, true),
            "not installed or not runnable",
        )?;
        refuses(lane_verdict(&[], true, false), "zero kani harnesses found")?;
        assert_eq!(lane_verdict(&[], true, true), Ok(()));
        assert_eq!(
            lane_verdict(&[result(HarnessStatus::Verified)], true, false),
            Ok(())
        );
        let failed = result(HarnessStatus::Failed("x".to_string()));
        refuses(lane_verdict(&[failed], true, true), "1 failed")?;
        let skipped = result(HarnessStatus::NotEstablished("x".to_string()));
        refuses(lane_verdict(&[skipped], true, true), "1 not established")
    }

    #[test]
    fn arguments_parse_and_reject_unknown_flags() -> Result<(), String> {
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
        refuses(parse_args(&strings(&["--bogus"])), "unknown kani argument")?;
        refuses(
            parse_args(&strings(&["--default-unwind", "0"])),
            "--default-unwind requires a positive integer",
        )?;
        refuses(
            parse_args(&strings(&["--harness"])),
            "--harness requires a value",
        )
    }

    #[test]
    fn native_process_error_cannot_be_hidden_by_verified_rows() -> Result<(), String> {
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
                refuses(
                    complete_lane_verdict(
                        &run.results,
                        true,
                        allow_empty,
                        run.process_problem.as_deref(),
                    ),
                    expected,
                )?;
            }
        }
        assert!(process_completion_problem(false, Some(true), "exit status: 0", 5).is_none());
        Ok(())
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
        refuses(
            complete_lane_verdict(&absent.results, true, false, None),
            "2 not established",
        )?;
        refuses(
            complete_lane_verdict(&[], true, true, Some("abnormal exit")),
            "abnormal exit",
        )?;
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
