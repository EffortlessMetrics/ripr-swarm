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

#[cfg(test)]
use std::fs;
use std::path::Path;
use std::time::Duration;

pub(crate) use crate::formal_evidence::kani_output::{
    Harness, HarnessResult, HarnessStatus, discover_harnesses, render_report,
};
#[cfg(test)]
use crate::formal_evidence::kani_output::{
    classify_output, results_from_output, split_harness_blocks,
};
use crate::run;

#[cfg(test)]
const HARNESS_PACKAGES: [(&str, &str); 2] = [
    ("ripr", "crates/ripr/src"),
    ("ripr-contract", "crates/ripr-contract/src"),
];
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KaniArgs {
    pub(crate) allow_empty: bool,
    pub(crate) harness: Option<String>,
    pub(crate) package: Option<String>,
    pub(crate) claim_registry: Option<String>,
    pub(crate) registered_claims: Vec<String>,
    pub(crate) default_unwind: Option<u32>,
    pub(crate) timeout_secs: u64,
    pub(crate) harness_timeout_secs: u64,
}

impl Default for KaniArgs {
    fn default() -> Self {
        Self {
            allow_empty: false,
            harness: None,
            package: None,
            claim_registry: None,
            registered_claims: Vec::new(),
            default_unwind: None,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            harness_timeout_secs: DEFAULT_HARNESS_TIMEOUT_SECS,
        }
    }
}

pub(crate) fn parse_args(args: &[String]) -> Result<KaniArgs, String> {
    let mut parsed = KaniArgs::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--allow-empty" => parsed.allow_empty = true,
            "--package" => {
                let package = next_value(&mut iter, "--package")?;
                if !matches!(package.as_str(), "ripr" | "ripr-contract") {
                    return Err("--package must be ripr or ripr-contract".to_string());
                }
                parsed.package = Some(package);
            }
            "--harness" => {
                parsed.harness = Some(next_value(&mut iter, "--harness")?);
            }
            "--claim-registry" => {
                parsed.claim_registry = Some(next_value(&mut iter, "--claim-registry")?);
            }
            "--registered-claim" => {
                let id = next_value(&mut iter, "--registered-claim")?;
                if parsed.registered_claims.contains(&id) {
                    return Err("--registered-claim must not repeat a claim ID".to_string());
                }
                parsed.registered_claims.push(id);
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
                    "unknown kani argument `{other}`; usage: cargo xtask kani [--allow-empty] [--package ripr|ripr-contract] [--harness <name>] [--claim-registry <path> --registered-claim <id> ...] [--default-unwind <n>] [--timeout-secs <n>] [--harness-timeout-secs <n>]"
                ));
            }
        }
    }
    if parsed.claim_registry.is_some() == parsed.registered_claims.is_empty() {
        return Err(
            "--claim-registry and at least one --registered-claim must be supplied together"
                .to_string(),
        );
    }
    parsed.registered_claims.sort();
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

#[cfg(test)]
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

#[cfg(test)]
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

/// The existing availability decision owns one bounded raw-byte capture. Even
/// native nonzero/timeout outcomes carry their captured streams to the receipt.
pub(crate) struct ProbeObservation {
    pub(crate) output: Option<run::TimedBytesOutput>,
    pub(crate) failure: Option<String>,
}
fn tool_available(remaining: Duration) -> ProbeObservation {
    let budget = remaining.min(Duration::from_secs(MAX_KANI_VERSION_PROBE_SECS));
    let output = match std::env::current_dir()
        .map_err(|error| format!("preflight working directory unavailable: {error}"))
        .and_then(|cwd| {
            run::capture_bytes_in_dir_with_env_budget_typed(
                Path::new("cargo"),
                &["kani".to_string(), "--version".to_string()],
                &cwd,
                (&[("CARGO_TERM_COLOR", "never")], &[]),
                run::ByteCaptureBudget {
                    timeout: budget,
                    stdout_bytes: 1024 * 1024,
                    stderr_bytes: 1024 * 1024,
                },
                "cargo kani --version",
            )
            .map_err(|error| probe_capture_failure_reason(error.kind).to_string())
        }) {
        Ok(output) => output,
        Err(failure) => {
            return ProbeObservation {
                output: None,
                failure: Some(failure),
            };
        }
    };
    let failure = if output.timed_out {
        Some(format!(
            "cargo kani --version timed out after {} milliseconds",
            budget.as_millis()
        ))
    } else if std::str::from_utf8(&output.stdout).is_err()
        || std::str::from_utf8(&output.stderr).is_err()
    {
        Some("cargo kani --version output is not strict UTF-8".to_string())
    } else {
        match output.status {
            Some(status) if status.success() => None,
            Some(status)
                if status.code() == Some(101)
                    && std::str::from_utf8(&output.stderr).is_ok_and(|stderr| {
                        stderr
                            .lines()
                            .any(|line| line.trim() == "error: no such command: `kani`")
                    }) =>
            {
                Some("cargo is installed but the kani subcommand is unavailable".to_string())
            }
            Some(status) => Some(format!("cargo kani --version failed with {status}")),
            None => Some("cargo kani --version returned without a process exit status".to_string()),
        }
    };
    ProbeObservation {
        output: Some(output),
        failure,
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

/// The exact `cargo` arguments for the lane, kept separate so a test can pin
/// what Kani actually receives.
pub(crate) fn kani_command(args: &KaniArgs, package: &str) -> Vec<String> {
    crate::formal_evidence::native::verification_arguments(
        package,
        args.harness.as_deref(),
        args.default_unwind,
        args.harness_timeout_secs,
    )
}

/// A process outcome is not an individual harness verdict.
#[cfg(test)]
struct KaniRun {
    results: Vec<HarnessResult>,
    process_problem: Option<String>,
}

#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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

pub(crate) fn run_lane(args: &[String]) -> Result<(), String> {
    if args.first().is_some_and(|arg| arg == "--replay") {
        if args.len() != 2 {
            return Err("usage: cargo xtask kani --replay <kani.json>".to_string());
        }
        return crate::kani_evidence::replay(Path::new(&args[1]));
    }
    crate::kani_evidence::invalidate_current_attempt()?;
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(error) => {
            crate::kani_evidence::record_argument_refusal(&error)?;
            return Err(error);
        }
    };
    crate::kani_evidence::run_lane(args, tool_available)
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
                package: None,
                claim_registry: None,
                registered_claims: Vec::new(),
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
