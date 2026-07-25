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

/// Remove ANSI SGR escape sequences from one line.
///
/// CI sets `CARGO_TERM_COLOR: always`, so cargo's own progress lines arrive as
/// `\x1b[1m\x1b[92m     Running\x1b[0m unittests src\lib.rs (...)`. Matching a
/// prefix like `Running ` against that fails, which is how the first real lane
/// run reported "no targets" while parsing every failure correctly. libtest's
/// own lines happen to be uncolored, but stripping unconditionally keeps the
/// parser from depending on that.
fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        // Consume `[` plus parameter bytes up to and including the final byte.
        if chars.next() != Some('[') {
            continue;
        }
        for next in chars.by_ref() {
            if next.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

pub(crate) fn parse_log(text: &str) -> RunOutcome {
    let mut outcome = RunOutcome {
        available: true,
        ..RunOutcome::default()
    };
    for raw_line in text.lines() {
        let line = strip_ansi(raw_line);
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

    /// The gate must catch the exact YAML bug that shipped in this lane's first
    /// revision: a plain-scalar `run:` whose ` #` was read as a comment, cutting
    /// the `printf` mid-string and leaving an unterminated quote.
    #[test]
    fn plain_scalar_comment_gate_catches_the_shipped_bug() {
        let broken = "    steps:\n      - name: Summarize\n        run: printf 'see #2393 done\\n' >> \"$GITHUB_STEP_SUMMARY\"\n";
        let found = crate::workflow_plain_scalar_comment_violations("wf.yml", broken);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("block scalar"), "{found:?}");

        // A block scalar carries the same text safely.
        let fixed = "    steps:\n      - name: Summarize\n        run: |\n          printf 'see #2393 done\\n' >> \"$GITHUB_STEP_SUMMARY\"\n";
        assert!(
            crate::workflow_plain_scalar_comment_violations("wf.yml", fixed).is_empty(),
            "a block scalar has no comment rule and must not be flagged"
        );
    }

    /// `#` without a preceding space is not a comment, and a quoted scalar
    /// carries its own delimiters — neither should be flagged.
    #[test]
    fn plain_scalar_comment_gate_does_not_flag_safe_shapes() {
        for safe in [
            "        run: cargo xtask check-workflows\n",
            "        run: printf '## Heading\\n'\n",
            "        run: \"echo a # b\"\n",
            "        run: cargo test --workspace -- --test-threads=1\n",
        ] {
            assert!(
                crate::workflow_plain_scalar_comment_violations("wf.yml", safe).is_empty(),
                "false positive on {safe:?}"
            );
        }
    }

    const RUN_WITH_TWO_FAILURES: &str = "\
     Running unittests src\\lib.rs (target\\debug\\deps\\ripr-abc.exe)
test cli::rerun::tests::alpha ... ok
test cli::rerun::tests::beta ... FAILED
test lsp::tests::gamma ... FAILED
test result: FAILED. 10 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1s
";

    /// Real bytes from the first Windows lane run (#2393). The `Running` lines
    /// are ANSI-coloured because CI sets `CARGO_TERM_COLOR: always`; the libtest
    /// lines are not. Against synthetic uncoloured fixtures the parser looked
    /// correct and reported "no targets" on the real thing, so this fixture is
    /// copied verbatim rather than hand-written.
    const REAL_RUNNER_LOG: &str = concat!(
        "\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m unittests src\\lib.rs (target\\debug\\deps\\ripr-b675962642118180.exe)\n",
        "test result: ok. 3777 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.15s\n",
        "\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m tests\\lsp_lifecycle.rs (target\\debug\\deps\\lsp_lifecycle-d7f865dad16cc0c7.exe)\n",
        "test compat_journey_collect_workspace_status_over_real_wire ... FAILED\n",
        "test result: FAILED. 25 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.30s\n",
    );

    #[test]
    fn parses_ansi_coloured_runner_output() {
        let outcome = parse_log(REAL_RUNNER_LOG);
        assert_eq!(
            outcome.targets,
            vec![
                "src/lib.rs".to_string(),
                "tests/lsp_lifecycle.rs".to_string()
            ],
            "ANSI-coloured `Running` lines must still yield targets"
        );
        assert_eq!(outcome.failed.len(), 1);
        assert!(
            outcome
                .failed
                .contains("compat_journey_collect_workspace_status_over_real_wire")
        );
        assert_eq!(outcome.results.len(), 2);
        assert!(!outcome.build_failed);
    }

    #[test]
    fn strip_ansi_removes_sgr_sequences_and_keeps_text() {
        assert_eq!(
            strip_ansi("\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m unittests src/lib.rs"),
            "     Running unittests src/lib.rs"
        );
        assert_eq!(strip_ansi("plain text"), "plain text");
        assert_eq!(strip_ansi(""), "");
    }

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
