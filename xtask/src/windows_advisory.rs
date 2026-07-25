//! Verdict for the advisory Windows lane (#2393).
//!
//! The lane runs `cargo test --workspace --no-fail-fast` twice and hands both
//! logs plus both captured exit statuses here.
//!
//! # Absence is not a pass
//!
//! The load-bearing rule is that a test missing from a log has **not** been
//! observed passing. `cargo test` can stop before later binaries, a run can die
//! mid-way, and a filter can exclude a name — so "failed in run 1, absent in
//! run 2" cannot be read as "passed in run 2, therefore flaky". Each test gets a
//! three-state observation per run ([`TestObservation`]) and the verdict is
//! derived from the pair, with an explicit `masked_unknown` outcome when one side
//! never observed the test at all.
//!
//! Verdicts are deliberately named for what two samples establish:
//! `repeated_failure` (reproduced in both samples) rather than "deterministic
//! defect", because a shared race can reproduce twice.
//!
//! # Missing evidence is a failure, not a pass
//!
//! Test failures are advisory: the lane does not gate merges on them. Failure to
//! *produce trustworthy evidence* is different, and this command exits non-zero
//! for it — a missing log, a missing exit status, or an unreadable file. A lane
//! that reported success while its own evidence was absent would be the exact
//! false-confidence condition it exists to prevent.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// What one run observed about one test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TestObservation {
    Failed,
    ObservedPass,
    /// The run never reported this test: masked by an earlier target, filtered,
    /// or the run ended first. Never treated as a pass.
    NotObserved,
}

/// How one run terminated, derived from its captured exit status rather than
/// inferred from log prose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunState {
    CompletedClean,
    CompletedWithTestFailures,
    /// Non-zero exit with no parsed test failure: compile, link, harness, or
    /// runner problem. Not a product verdict.
    CompileOrHarnessFailure,
    LogMissing,
    StatusMissing,
}

impl RunState {
    fn label(self) -> &'static str {
        match self {
            Self::CompletedClean => "completed_clean",
            Self::CompletedWithTestFailures => "completed_with_test_failures",
            Self::CompileOrHarnessFailure => "compile_or_harness_failure",
            Self::LogMissing => "log_missing",
            Self::StatusMissing => "status_missing",
        }
    }

    /// Whether this run produced evidence that can be compared at all.
    fn is_usable(self) -> bool {
        !matches!(self, Self::LogMissing | Self::StatusMissing)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RunOutcome {
    pub(crate) state: RunState,
    pub(crate) exit_status: Option<i32>,
    pub(crate) failed: BTreeSet<String>,
    pub(crate) passed: BTreeSet<String>,
    pub(crate) targets: Vec<String>,
    pub(crate) results: Vec<String>,
}

impl RunOutcome {
    fn missing(state: RunState) -> Self {
        Self {
            state,
            exit_status: None,
            failed: BTreeSet::new(),
            passed: BTreeSet::new(),
            targets: Vec::new(),
            results: Vec::new(),
        }
    }

    fn observe(&self, name: &str) -> TestObservation {
        if self.failed.contains(name) {
            TestObservation::Failed
        } else if self.passed.contains(name) {
            TestObservation::ObservedPass
        } else {
            TestObservation::NotObserved
        }
    }
}

/// What two samples establish about one test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// Failed in both samples. Reproduced twice — not yet demonstrated to be
    /// deterministic, since a shared race can also reproduce twice.
    RepeatedFailure,
    /// Failed in one sample and observed passing in the other.
    Unstable,
    /// Failed in one sample and never observed in the other. Cannot be
    /// classified without a run that actually reached it.
    MaskedUnknown,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Self::RepeatedFailure => "repeated_failure",
            Self::Unstable => "unstable",
            Self::MaskedUnknown => "masked_unknown",
        }
    }
}

fn classify(first: TestObservation, second: TestObservation) -> Option<Verdict> {
    use TestObservation::{Failed, NotObserved, ObservedPass};
    match (first, second) {
        (Failed, Failed) => Some(Verdict::RepeatedFailure),
        (Failed, ObservedPass) | (ObservedPass, Failed) => Some(Verdict::Unstable),
        (Failed, NotObserved) | (NotObserved, Failed) => Some(Verdict::MaskedUnknown),
        // Nothing failed anywhere: not reported.
        _ => None,
    }
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let run1_log = flag_value(args, "--run1")?;
    let run1_status = flag_value(args, "--run1-status")?;
    let run2_log = flag_value(args, "--run2")?;
    let run2_status = flag_value(args, "--run2-status")?;
    if run1_log == run2_log {
        return Err("windows-advisory-summary requires two distinct run logs".to_string());
    }

    let first = load_run(Path::new(&run1_log), Path::new(&run1_status));
    let second = load_run(Path::new(&run2_log), Path::new(&run2_status));
    print!("{}", render(&first, &second));

    // Advisory applies to test outcomes, not to evidence. A run whose log or
    // status is missing means this lane cannot be trusted, so fail loudly.
    let mut unusable = Vec::new();
    for (label, outcome) in [("run 1", &first), ("run 2", &second)] {
        if !outcome.state.is_usable() {
            unusable.push(format!("{label} is {}", outcome.state.label()));
        }
    }
    if unusable.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "windows-advisory-summary could not produce trustworthy evidence: {}",
            unusable.join("; ")
        ))
    }
}

/// Read one flag's value, rejecting a value that is itself a flag.
///
/// `--run1 --run2 path` would otherwise silently consume `--run2` as run 1's
/// path and then fail to find `--run2`, or worse, compare a log against itself.
fn flag_value(args: &[String], flag: &str) -> Result<String, String> {
    let occurrences = args.iter().filter(|arg| arg.as_str() == flag).count();
    if occurrences == 0 {
        return Err(format!("windows-advisory-summary requires {flag} <path>"));
    }
    if occurrences > 1 {
        return Err(format!(
            "windows-advisory-summary got {flag} {occurrences} times; pass it once"
        ));
    }
    let index = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| format!("windows-advisory-summary requires {flag} <path>"))?;
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("windows-advisory-summary requires a value for {flag}"))?;
    if value.starts_with('-') {
        return Err(format!(
            "windows-advisory-summary got {flag} followed by {value}, which looks like a flag rather than a path"
        ));
    }
    Ok(value.clone())
}

fn load_run(log: &Path, status: &Path) -> RunOutcome {
    let text = match std::fs::read_to_string(log) {
        Ok(text) => text,
        Err(error) => {
            eprintln!(
                "windows-advisory-summary: read {} failed: {error}",
                log.display()
            );
            return RunOutcome::missing(RunState::LogMissing);
        }
    };
    let exit_status = match std::fs::read_to_string(status) {
        Ok(raw) => match raw.trim().parse::<i32>() {
            Ok(code) => Some(code),
            Err(error) => {
                eprintln!(
                    "windows-advisory-summary: {} is not an integer exit status: {error}",
                    status.display()
                );
                None
            }
        },
        Err(error) => {
            eprintln!(
                "windows-advisory-summary: read {} failed: {error}",
                status.display()
            );
            None
        }
    };
    let Some(exit_status) = exit_status else {
        let mut outcome = parse_log(&text);
        outcome.state = RunState::StatusMissing;
        return outcome;
    };
    let mut outcome = parse_log(&text);
    outcome.exit_status = Some(exit_status);
    outcome.state = if exit_status == 0 {
        RunState::CompletedClean
    } else if outcome.failed.is_empty() {
        RunState::CompileOrHarnessFailure
    } else {
        RunState::CompletedWithTestFailures
    };
    outcome
}

/// Remove ANSI SGR escape sequences from one line.
///
/// CI sets `CARGO_TERM_COLOR: always`, so cargo's own progress lines arrive as
/// `\x1b[1m\x1b[92m     Running\x1b[0m unittests src\lib.rs (...)`. Matching a
/// prefix like `Running ` against that fails, which is how the first real lane
/// run reported no targets while parsing every failure correctly.
fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        // Peek rather than consume: an ESC that does not introduce a CSI
        // sequence must not swallow the character after it. Consuming
        // unconditionally silently deleted real text — an ESC followed by `X`
        // lost both.
        if chars.peek() != Some(&'[') {
            continue;
        }
        let _ = chars.next();
        for next in chars.by_ref() {
            if next.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

pub(crate) fn parse_log(text: &str) -> RunOutcome {
    let mut outcome = RunOutcome::missing(RunState::StatusMissing);
    for raw_line in text.lines() {
        let line = strip_ansi(raw_line);
        let trimmed = line.trim();
        if let Some((name, failed)) = test_result_line(trimmed) {
            if failed {
                outcome.failed.insert(name);
            } else {
                outcome.passed.insert(name);
            }
        }
        if let Some(target) = running_target(trimmed) {
            outcome.targets.push(target);
        }
        if trimmed.starts_with("test result:") {
            outcome.results.push(trimmed.to_string());
        }
    }
    outcome
}

/// `test some::path ... FAILED` / `... ok` -> (name, failed).
///
/// Both outcomes are collected: knowing a test was observed *passing* is what
/// separates a flake from a test that was never reached.
fn test_result_line(line: &str) -> Option<(String, bool)> {
    let rest = line.strip_prefix("test ")?;
    let (name, failed) = if let Some(name) = rest.strip_suffix(" ... FAILED") {
        (name, true)
    } else if let Some(name) = rest.strip_suffix(" ... ok") {
        (name, false)
    } else {
        return None;
    };
    let name = name.trim();
    (!name.is_empty() && !name.contains(' ')).then(|| (name.to_string(), failed))
}

/// `Running unittests src\lib.rs (target\debug\deps\ripr-abc.exe)` -> the
/// source path, which identifies the target more stably than the hashed binary.
///
/// The ` (` requirement matters: other lines can begin with `Running ` (build
/// scripts, custom commands), and only a cargo test-target line carries the
/// binary in parentheses.
fn running_target(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("Running unittests ")
        .or_else(|| line.strip_prefix("Running "))?;
    let (path, _binary) = rest.split_once(" (")?;
    let path = path.trim();
    (!path.is_empty()).then(|| path.replace('\\', "/"))
}

fn render(first: &RunOutcome, second: &RunOutcome) -> String {
    let mut out = String::from("### Run states\n\n");
    for (label, outcome) in [("Run 1", first), ("Run 2", second)] {
        let status = match outcome.exit_status {
            Some(code) => format!("cargo exit {code}"),
            None => "cargo exit unknown".to_string(),
        };
        out.push_str(&format!(
            "- {label}: `{}` ({status})\n",
            outcome.state.label()
        ));
    }
    out.push('\n');

    if !first.state.is_usable() || !second.state.is_usable() {
        out.push_str("**Evidence failure.** At least one run did not produce a usable log and exit status, so no verdict can be derived. This is reported as a workflow failure, not as a pass — a lane that goes green without evidence is worse than no lane.\n\n");
    }
    if first.state == RunState::CompileOrHarnessFailure
        || second.state == RunState::CompileOrHarnessFailure
    {
        out.push_str("**Infrastructure failure.** A run exited non-zero with no parsed test failure, which indicates a compile, link, harness, or runner problem rather than a product regression.\n\n");
    }

    out.push_str("### Verdicts\n\n");
    let mut verdicts: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let candidates: BTreeSet<&String> = first.failed.iter().chain(second.failed.iter()).collect();
    for name in candidates {
        if let Some(verdict) = classify(first.observe(name), second.observe(name)) {
            verdicts
                .entry(verdict.label())
                .or_default()
                .push(name.clone());
        }
    }

    if verdicts.is_empty() {
        if first.state.is_usable() && second.state.is_usable() {
            out.push_str("No test failed in either run.\n\n");
        } else {
            out.push_str("No verdict: see the evidence failure above.\n\n");
        }
    }

    for (label, explanation) in [
        (
            "repeated_failure",
            "Failed in both runs. Reproduced twice; a shared race can also reproduce twice, so this is not yet demonstrated to be deterministic.",
        ),
        (
            "unstable",
            "Failed in one run and observed passing in the other. Confirm in isolation before filing as a defect.",
        ),
        (
            "masked_unknown",
            "Failed in one run and never observed in the other, so it cannot be classified. A test absent from a log was not observed passing.",
        ),
    ] {
        let Some(names) = verdicts.get(label) else {
            continue;
        };
        out.push_str(&format!(
            "**{label} ({})** — {explanation}\n\n",
            names.len()
        ));
        for name in names {
            out.push_str(&format!("- `{name}`\n"));
        }
        out.push('\n');
    }

    out.push_str("### Targets reached\n\n");
    out.push_str("Recorded because a compile or harness failure can still stop a run before later targets. With `--no-fail-fast` an ordinary test failure no longer hides them.\n\n");
    for (label, outcome) in [("Run 1", first), ("Run 2", second)] {
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
        out.push_str(&format!(
            "- {label}: observed {} pass, {} fail across {} target result line(s)\n",
            outcome.passed.len(),
            outcome.failed.len(),
            outcome.results.len()
        ));
    }
    out.push_str("\nTest outcomes here are advisory and do not gate merges (#2393). Missing evidence is not advisory and fails this lane.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real bytes from a Windows lane run (#2393): cargo's `Running` lines are
    /// ANSI-coloured because CI sets `CARGO_TERM_COLOR: always`, libtest's are
    /// not. Copied verbatim, because synthetic uncoloured fixtures hid a parser
    /// gap that only real runner output exposed.
    const REAL_RUNNER_LOG: &str = concat!(
        "\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m unittests src\\lib.rs (target\\debug\\deps\\ripr-b675962642118180.exe)\n",
        "test some::alpha ... ok\n",
        "test result: ok. 3777 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.15s\n",
        "\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m tests\\lsp_lifecycle.rs (target\\debug\\deps\\lsp_lifecycle-d7f865dad16cc0c7.exe)\n",
        "test compat_journey_collect_workspace_status_over_real_wire ... FAILED\n",
        "test result: FAILED. 25 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.30s\n",
    );

    fn outcome(state: RunState, failed: &[&str], passed: &[&str]) -> RunOutcome {
        RunOutcome {
            state,
            exit_status: Some(if failed.is_empty() { 0 } else { 101 }),
            failed: failed.iter().map(|name| (*name).to_string()).collect(),
            passed: passed.iter().map(|name| (*name).to_string()).collect(),
            targets: vec!["src/lib.rs".to_string()],
            results: vec!["test result: FAILED. 1 passed; 1 failed".to_string()],
        }
    }

    #[test]
    fn parses_ansi_coloured_runner_output_including_passes() {
        let parsed = parse_log(REAL_RUNNER_LOG);
        assert_eq!(
            parsed.targets,
            vec![
                "src/lib.rs".to_string(),
                "tests/lsp_lifecycle.rs".to_string()
            ]
        );
        assert!(
            parsed
                .failed
                .contains("compat_journey_collect_workspace_status_over_real_wire")
        );
        assert!(
            parsed.passed.contains("some::alpha"),
            "observed passes must be collected too: {:?}",
            parsed.passed
        );
        assert_eq!(parsed.results.len(), 2);
    }

    /// The rule this module exists for: a test absent from one run has not been
    /// observed passing, so it cannot be called a flake.
    #[test]
    fn absence_is_masked_unknown_not_unstable() {
        let first = outcome(RunState::CompletedWithTestFailures, &["x::y"], &[]);
        let second = outcome(RunState::CompletedClean, &[], &[]); // never reported x::y
        assert_eq!(
            classify(first.observe("x::y"), second.observe("x::y")),
            Some(Verdict::MaskedUnknown)
        );
        let rendered = render(&first, &second);
        assert!(rendered.contains("masked_unknown (1)"), "{rendered}");
        assert!(!rendered.contains("unstable ("), "{rendered}");
    }

    /// Only an explicitly observed pass on the other side makes a failure a flake.
    #[test]
    fn observed_pass_on_the_other_side_is_unstable() {
        let first = outcome(RunState::CompletedWithTestFailures, &["x::y"], &[]);
        let second = outcome(RunState::CompletedClean, &[], &["x::y"]);
        assert_eq!(
            classify(first.observe("x::y"), second.observe("x::y")),
            Some(Verdict::Unstable)
        );
        let rendered = render(&first, &second);
        assert!(rendered.contains("unstable (1)"), "{rendered}");
        assert!(!rendered.contains("masked_unknown ("), "{rendered}");
    }

    #[test]
    fn failing_twice_is_reported_as_repeated_not_deterministic() {
        let both = outcome(RunState::CompletedWithTestFailures, &["x::y"], &[]);
        assert_eq!(
            classify(both.observe("x::y"), both.observe("x::y")),
            Some(Verdict::RepeatedFailure)
        );
        let rendered = render(&both, &both);
        assert!(rendered.contains("repeated_failure (1)"), "{rendered}");
        assert!(
            !rendered.contains("deterministic platform defect"),
            "two samples do not establish determinism: {rendered}"
        );
    }

    #[test]
    fn non_zero_exit_without_parsed_failures_is_infrastructure() {
        let mut broken = outcome(RunState::CompileOrHarnessFailure, &[], &[]);
        broken.exit_status = Some(101);
        let clean = outcome(RunState::CompletedClean, &[], &["x::y"]);
        let rendered = render(&broken, &clean);
        assert!(rendered.contains("Infrastructure failure"), "{rendered}");
    }

    #[test]
    fn missing_evidence_renders_an_evidence_failure_and_no_pass_claim() {
        let missing = RunOutcome::missing(RunState::LogMissing);
        let clean = outcome(RunState::CompletedClean, &[], &["x::y"]);
        let rendered = render(&missing, &clean);
        assert!(rendered.contains("**Evidence failure.**"), "{rendered}");
        assert!(
            !rendered.contains("No test failed in either run."),
            "{rendered}"
        );
    }

    #[test]
    fn run_state_labels_are_stable_wire_strings() {
        assert_eq!(RunState::CompletedClean.label(), "completed_clean");
        assert_eq!(
            RunState::CompletedWithTestFailures.label(),
            "completed_with_test_failures"
        );
        assert_eq!(
            RunState::CompileOrHarnessFailure.label(),
            "compile_or_harness_failure"
        );
        assert_eq!(RunState::LogMissing.label(), "log_missing");
        assert_eq!(RunState::StatusMissing.label(), "status_missing");
        assert!(RunState::CompletedWithTestFailures.is_usable());
        assert!(!RunState::LogMissing.is_usable());
        assert!(!RunState::StatusMissing.is_usable());
    }

    #[test]
    fn strip_ansi_removes_sgr_sequences_and_keeps_text() {
        assert_eq!(
            strip_ansi("\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m unittests src/lib.rs"),
            "     Running unittests src/lib.rs"
        );
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    /// An ESC that does not introduce a CSI sequence must drop only the ESC.
    /// Consuming the next character unconditionally deleted real text, which
    /// could silently truncate a test name.
    #[test]
    fn strip_ansi_keeps_the_character_after_a_lone_escape() {
        assert_eq!(strip_ansi("a\u{1b}Xb"), "aXb");
        assert_eq!(strip_ansi("\u{1b}"), "");
        assert_eq!(strip_ansi("test some::name\u{1b}"), "test some::name");
        // The pathological case this protects: a stray ESC before the shape the
        // parser matches on.
        assert_eq!(
            strip_ansi("test \u{1b}some::name ... FAILED"),
            "test some::name ... FAILED"
        );
    }

    /// A line beginning `Running ` without a parenthesised binary is not a test
    /// target — build scripts and custom commands can produce one.
    #[test]
    fn running_target_requires_a_parenthesised_binary() {
        assert_eq!(
            running_target("Running unittests src/lib.rs (target/debug/deps/a-1.exe)"),
            Some("src/lib.rs".to_string())
        );
        assert_eq!(running_target("Running a custom build command"), None);
        assert_eq!(running_target("Running"), None);
    }

    #[test]
    fn ignores_lines_that_only_resemble_a_test_result() {
        let parsed = parse_log("failures:\n    some::name\ntest result: FAILED. 1 failed\n");
        assert!(parsed.failed.is_empty(), "{:?}", parsed.failed);
        assert!(parsed.passed.is_empty(), "{:?}", parsed.passed);
    }

    /// Argument validation, asserted on error values so the operator-facing
    /// message is pinned and no panic-family call is needed.
    #[test]
    fn flag_value_rejects_missing_duplicate_and_flag_shaped_values() {
        let args = |values: &[&str]| -> Vec<String> {
            values.iter().map(|value| (*value).to_string()).collect()
        };
        assert_eq!(
            flag_value(&[], "--run1"),
            Err("windows-advisory-summary requires --run1 <path>".to_string())
        );
        assert_eq!(
            flag_value(&args(&["--run1"]), "--run1"),
            Err("windows-advisory-summary requires a value for --run1".to_string())
        );
        // The Gemini finding: a flag consumed as a path.
        assert_eq!(
            flag_value(&args(&["--run1", "--run2", "b.log"]), "--run1"),
            Err("windows-advisory-summary got --run1 followed by --run2, which looks like a flag rather than a path".to_string())
        );
        assert_eq!(
            flag_value(&args(&["--run1", "a.log", "--run1", "b.log"]), "--run1"),
            Err("windows-advisory-summary got --run1 2 times; pass it once".to_string())
        );
        assert_eq!(
            flag_value(&args(&["--run1", "a.log"]), "--run1"),
            Ok("a.log".to_string())
        );
    }
}
