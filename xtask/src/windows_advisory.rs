//! Summary for the advisory Windows lane (#2393).
//!
//! The lane runs `cargo test --workspace` twice and hands both logs here. This
//! module turns them into a reviewer-readable verdict rather than leaving a
//! wall of log for someone to interpret.
//!
//! Two distinctions carry the value:
//!
//! - **Deterministic vs unstable.** A test failing in both runs is a platform
//!   defect; failing in one is a flake. Conflating them is exactly how a
//!   deterministic failure got mis-filed as intermittent in #2419, so the
//!   summary never reports a bare failure list.
//! - **Infrastructure vs semantic.** A compile or toolchain failure is not a
//!   test result, and must not be read as "Windows is broken" in the product
//!   sense.
//!
//! It also reports which test targets executed, because `cargo test` stops
//! after the first failing target: a clean-looking later target may simply
//! never have run.

use std::collections::BTreeSet;
use std::path::Path;

/// One parsed `cargo test` run.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RunOutcome {
    /// Log was readable.
    pub(crate) available: bool,
    /// Test names reported as `... FAILED`.
    pub(crate) failed: BTreeSet<String>,
    /// Test binaries the run actually reached.
    pub(crate) targets: Vec<String>,
    /// Compilation or toolchain failure rather than a test result.
    pub(crate) build_failed: bool,
    /// `test result:` summary lines, in order.
    pub(crate) results: Vec<String>,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let run1 = flag_value(args, "--run1")?;
    let run2 = flag_value(args, "--run2")?;
    let first = parse_log_file(Path::new(&run1));
    let second = parse_log_file(Path::new(&run2));
    print!("{}", render(&first, &second));
    Ok(())
}

fn flag_value(args: &[String], flag: &str) -> Result<String, String> {
    let index = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| format!("windows-advisory-summary requires {flag} <path>"))?;
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("windows-advisory-summary requires a value for {flag}"))
}

fn parse_log_file(path: &Path) -> RunOutcome {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_log(&text),
        Err(_) => RunOutcome::default(),
    }
}

pub(crate) fn parse_log(text: &str) -> RunOutcome {
    let mut outcome = RunOutcome {
        available: true,
        ..RunOutcome::default()
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(name) = failed_test_name(trimmed) {
            outcome.failed.insert(name);
        }
        if let Some(target) = running_target(trimmed) {
            outcome.targets.push(target);
        }
        if trimmed.starts_with("test result:") {
            outcome.results.push(trimmed.to_string());
        }
        if trimmed.starts_with("error: could not compile")
            || trimmed.starts_with("error: linking with")
            || trimmed.starts_with("error: failed to run custom build command")
        {
            outcome.build_failed = true;
        }
    }
    outcome
}

/// `test some::path ... FAILED` -> `some::path`.
fn failed_test_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("test ")?;
    let name = rest.strip_suffix(" ... FAILED")?;
    let name = name.trim();
    (!name.is_empty() && !name.contains(' ')).then(|| name.to_string())
}

/// `Running unittests src\lib.rs (target\debug\deps\ripr-abc.exe)` -> the
/// source path, which identifies the target more stably than the hashed binary.
fn running_target(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("Running unittests ")
        .or_else(|| line.strip_prefix("Running "))?;
    let path = rest.split(" (").next()?.trim();
    (!path.is_empty()).then(|| path.replace('\\', "/"))
}

fn render(first: &RunOutcome, second: &RunOutcome) -> String {
    let mut out = String::from("### Result\n\n");

    if !first.available && !second.available {
        out.push_str("No run log was readable, so this lane produced no signal. Treat as **unknown**, not as a pass.\n");
        return out;
    }
    if first.build_failed || second.build_failed {
        out.push_str("**Infrastructure failure:** the workspace did not compile on Windows. This is a build or toolchain problem, not a test verdict — do not read it as a product regression.\n\n");
    }

    let deterministic: Vec<&String> = first.failed.intersection(&second.failed).collect();
    let unstable: Vec<&String> = first.failed.symmetric_difference(&second.failed).collect();

    if deterministic.is_empty()
        && unstable.is_empty()
        && !first.build_failed
        && !second.build_failed
    {
        out.push_str("No test failures in either run.\n\n");
    }

    if !deterministic.is_empty() {
        out.push_str(&format!(
            "**Deterministic failures ({}).** Failed in both runs — platform defects, not flakes:\n\n",
            deterministic.len()
        ));
        for name in &deterministic {
            out.push_str(&format!("- `{name}`\n"));
        }
        out.push('\n');
    }

    if !unstable.is_empty() {
        out.push_str(&format!(
            "**Unstable ({}).** Failed in exactly one of two runs — treat as a load-dependent flake and confirm in isolation before filing as a defect:\n\n",
            unstable.len()
        ));
        for name in &unstable {
            out.push_str(&format!("- `{name}`\n"));
        }
        out.push('\n');
    }

    out.push_str("### Targets reached\n\n");
    out.push_str("`cargo test` stops after the first failing target, so a target absent here did not run and its state is unknown.\n\n");
    for (label, outcome) in [("Run 1", first), ("Run 2", second)] {
        if !outcome.available {
            out.push_str(&format!("- {label}: log unavailable\n"));
            continue;
        }
        let targets = if outcome.targets.is_empty() {
            "none reported".to_string()
        } else {
            outcome.targets.join(", ")
        };
        out.push_str(&format!("- {label}: {targets}\n"));
    }
    out.push('\n');

    out.push_str("### Totals\n\n");
    for (label, outcome) in [("Run 1", first), ("Run 2", second)] {
        if outcome.results.is_empty() {
            out.push_str(&format!("- {label}: no `test result:` line\n"));
            continue;
        }
        out.push_str(&format!("- {label}: {}\n", outcome.results.join(" | ")));
    }
    out.push_str("\nThis lane is advisory and never fails; it is not a required check (#2393).\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN_WITH_TWO_FAILURES: &str = "\
     Running unittests src\\lib.rs (target\\debug\\deps\\ripr-abc.exe)
test cli::rerun::tests::alpha ... ok
test cli::rerun::tests::beta ... FAILED
test lsp::tests::gamma ... FAILED
test result: FAILED. 10 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1s
";

    #[test]
    fn parses_failures_targets_and_totals() {
        let outcome = parse_log(RUN_WITH_TWO_FAILURES);
        assert!(outcome.available);
        assert!(!outcome.build_failed);
        assert_eq!(outcome.targets, vec!["src/lib.rs".to_string()]);
        assert_eq!(outcome.failed.len(), 2);
        assert!(outcome.failed.contains("cli::rerun::tests::beta"));
        assert!(outcome.failed.contains("lsp::tests::gamma"));
        assert_eq!(outcome.results.len(), 1);
    }

    /// The distinction this command exists for: the same failure in both runs is
    /// a defect; a failure in one run is a flake. Reporting them together as
    /// "failures" is what mis-filed a deterministic failure as intermittent.
    #[test]
    fn separates_deterministic_failures_from_unstable_ones() {
        let first = parse_log(RUN_WITH_TWO_FAILURES);
        let second = parse_log(
            "test cli::rerun::tests::beta ... FAILED\ntest result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1s\n",
        );
        let rendered = render(&first, &second);
        assert!(
            rendered.contains("**Deterministic failures (1)")
                && rendered.contains("`cli::rerun::tests::beta`"),
            "{rendered}"
        );
        assert!(
            rendered.contains("**Unstable (1)") && rendered.contains("`lsp::tests::gamma`"),
            "{rendered}"
        );
    }

    #[test]
    fn reports_a_clean_pair_of_runs_without_failure_sections() {
        let clean = parse_log(
            "     Running unittests src\\lib.rs (target\\debug\\deps\\ripr-abc.exe)\ntest result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1s\n",
        );
        let rendered = render(&clean, &clean);
        assert!(
            rendered.contains("No test failures in either run."),
            "{rendered}"
        );
        assert!(!rendered.contains("Deterministic failures"), "{rendered}");
        assert!(!rendered.contains("Unstable ("), "{rendered}");
    }

    /// A build failure must not read as a product verdict.
    #[test]
    fn names_a_build_failure_as_infrastructure() {
        let broken =
            parse_log("error: could not compile `ripr` (lib test) due to 2 previous errors\n");
        assert!(broken.build_failed);
        let rendered = render(&broken, &broken);
        assert!(rendered.contains("Infrastructure failure"), "{rendered}");
    }

    /// An absent log is unknown, never a pass. A lane that silently reports
    /// success when it produced no signal is the failure mode this lane exists
    /// to prevent.
    #[test]
    fn missing_logs_report_unknown_rather_than_pass() {
        let missing = RunOutcome::default();
        let rendered = render(&missing, &missing);
        assert!(rendered.contains("**unknown**"), "{rendered}");
        assert!(!rendered.contains("No test failures"), "{rendered}");
    }

    #[test]
    fn ignores_lines_that_only_resemble_a_failure_report() {
        let outcome = parse_log(
            "test result: FAILED. 1 passed; 1 failed; 0 ignored\ntest some::name ... ok\nfailures:\n    some::name\n",
        );
        assert!(
            outcome.failed.is_empty(),
            "only `... FAILED` lines name a failing test, got {:?}",
            outcome.failed
        );
    }

    /// Asserted on the error value rather than `is_err`, so the message a CI
    /// operator sees is pinned too — and without reaching for the panic-family
    /// `unwrap_err` that clippy would otherwise suggest.
    #[test]
    fn flag_value_requires_a_path() {
        assert_eq!(
            flag_value(&["--run1".to_string()], "--run1"),
            Err("windows-advisory-summary requires a value for --run1".to_string())
        );
        assert_eq!(
            flag_value(&[], "--run2"),
            Err("windows-advisory-summary requires --run2 <path>".to_string())
        );
        assert_eq!(
            flag_value(&["--run1".to_string(), "a.log".to_string()], "--run1"),
            Ok("a.log".to_string())
        );
    }
}
