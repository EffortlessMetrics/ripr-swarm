//! `cargo xtask windows-advisory-acl-evidence`: fail-closed evidence guard for
//! the nightly Windows advisory lane's real-ACL pwsh step (#10621, review
//! finding on PR #10755).
//!
//! The pwsh step runs the four tests that need a real `icacls (AD)` deny and
//! records cargo's exit status through `windows-advisory-run`. A status file
//! alone proves nothing:
//!
//! - `windows-advisory-run` writes a status even when cargo could not start
//!   (spawn failures report `127`), so an existing status file can mean no
//!   test ran at all;
//! - the step's name filters (`denies_creation`, `denying_subdirectories`,
//!   `real_unreadable_subtree`) are libtest substrings: renaming a test
//!   without updating them is a zero-match filter, which cargo treats as a
//!   silent green run — the same condition the subset evidence check rejects
//!   for the `subset-*` labels (#4938).
//!
//! This command closes both holes the same way the lane closes them
//! elsewhere: a missing or unreadable log or status is an evidence failure,
//! spawn status `127` is an evidence failure, and each expected test must be
//! observed exactly once in the log as `ok` or `FAILED`. An `ignored` row is
//! an evidence failure too: the deny was never exercised, so there is no
//! real-ACL observation to report. A `FAILED` observation stays advisory —
//! the test ran and its outcome is a test result, not missing evidence.
//!
//! The pwsh step pins the four expected full libtest paths as `--expect`
//! arguments, so the workflow names the exact rows that constitute the
//! evidence and this command refuses everything else (missing, duplicated,
//! ignored). Nothing is written; the command prints one summary line and
//! exits non-zero on any evidence failure.

use std::fs;
use std::path::PathBuf;

const SPAWN_FAILED_STATUS: i32 = 127;

struct Options {
    log: PathBuf,
    status: PathBuf,
    expected: Vec<String>,
}

/// One `test <path> ... <outcome>` row as libtest writes it into the log.
struct Observation {
    test: String,
    outcome: String,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let status_text = fs::read_to_string(&options.status)
        .map_err(|err| format!("windows-advisory-acl-evidence: cannot read the status file {}: {err}; a missing status is not evidence", options.status.display()))?;
    let status: i32 = status_text.trim().parse().map_err(|err| {
        format!(
            "windows-advisory-acl-evidence: the status file {} does not contain an exit status ({err}); an unparseable status is not evidence",
            options.status.display()
        )
    })?;
    if status == SPAWN_FAILED_STATUS {
        return Err(format!(
            "windows-advisory-acl-evidence: windows-advisory-run reported spawn status {SPAWN_FAILED_STATUS}; cargo never started, so no real-ACL test ran and the status file is not evidence"
        ));
    }
    let log_text = fs::read_to_string(&options.log)
        .map_err(|err| format!("windows-advisory-acl-evidence: cannot read the log file {}: {err}; a missing log is not evidence", options.log.display()))?;
    let observations = parse_observations(&log_text);
    let problems = evidence_problems(&options.expected, &observations, status);
    let observed = options
        .expected
        .iter()
        .filter(|name| {
            let runs: Vec<&Observation> = observations
                .iter()
                .filter(|observation| &observation.test == *name)
                .collect();
            runs.len() == 1 && runs[0].outcome != "ignored"
        })
        .count();
    println!(
        "windows-advisory-acl-evidence: observed {observed} of {} expected real-ACL tests (cargo status {status}); test outcomes stay advisory",
        options.expected.len()
    );
    if problems.is_empty() {
        return Ok(());
    }
    for problem in &problems {
        println!("::error::{problem}");
    }
    Err(format!(
        "windows-advisory-acl-evidence: {} real-ACL evidence failure(s); the pwsh step ran without the proof it exists to produce",
        problems.len()
    ))
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut log: Option<PathBuf> = None;
    let mut status: Option<PathBuf> = None;
    let mut expected: Vec<String> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let value = |iter: &mut std::slice::Iter<String>, name: &str| {
            iter.next()
                .ok_or_else(|| format!("windows-advisory-acl-evidence: {name} requires a value"))
                .cloned()
        };
        match arg.as_str() {
            "--log" => log = Some(PathBuf::from(value(&mut iter, "--log")?)),
            "--status" => status = Some(PathBuf::from(value(&mut iter, "--status")?)),
            "--expect" => expected.push(value(&mut iter, "--expect")?),
            other => {
                return Err(format!(
                    "windows-advisory-acl-evidence: unknown argument {other}; usage: windows-advisory-acl-evidence --log <path> --status <path> --expect <test-path> [--expect <test-path>...]"
                ));
            }
        }
    }
    let log =
        log.ok_or_else(|| "windows-advisory-acl-evidence: --log <path> is required".to_string())?;
    let status = status
        .ok_or_else(|| "windows-advisory-acl-evidence: --status <path> is required".to_string())?;
    if expected.is_empty() {
        return Err(
            "windows-advisory-acl-evidence: at least one --expect <test-path> is required; a guard with no expectations proves nothing".to_string(),
        );
    }
    Ok(Options {
        log,
        status,
        expected,
    })
}

/// Collects `test <path> ... <outcome>` rows. Cargo interleaves target
/// headers and libtest output; only these rows are observations, exactly the
/// rows the pwsh step renders into its summary.
fn parse_observations(log: &str) -> Vec<Observation> {
    log.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("test ")?;
            let (test, outcome) = rest.split_once(" ... ")?;
            Some(Observation {
                test: test.to_string(),
                outcome: outcome.trim().to_string(),
            })
        })
        .collect()
}

fn evidence_problems(
    expected: &[String],
    observations: &[Observation],
    status: i32,
) -> Vec<String> {
    let mut problems = Vec::new();
    for name in expected {
        let runs: Vec<&Observation> = observations
            .iter()
            .filter(|observation| &observation.test == name)
            .collect();
        match runs.as_slice() {
            [] => problems.push(format!(
                "expected real-ACL test did not run: {name}; a renamed test or a stale filter matches nothing, and cargo reports that silence as exit {status}"
            )),
            [only] if only.outcome == "ignored" => problems.push(format!(
                "expected real-ACL test reported ignored: {name}; an ignored test never exercised the deny, so there is no real-ACL observation"
            )),
            [_] => {}
            many => problems.push(format!(
                "expected real-ACL test observed {} times: {name}; the evidence must name exactly one run",
                many.len()
            )),
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCTOR_DENIES_CREATION: &str = "cli::commands::doctor::tests::probe_creatable_dir_reports_a_directory_that_denies_creation";
    const DOCTOR_SHARDED_DIR: &str = "cli::commands::doctor::tests::cache_unwritable_reason_reports_existing_sharded_dir_denying_subdirectories";
    const DOCTOR_KEY_DIR: &str = "cli::commands::doctor::tests::cache_unwritable_reason_reports_key_dir_denying_subdirectories";
    const SWEEP: &str = "reports::eval_sweep_refresh::python_eval_sweep_refresh::real_unreadable_subtree_marks_corpus_selection_incomplete";

    /// The four rows exactly as the pinned head's nightly run wrote them
    /// (run 40383529235, artifact `windows-acl.log`).
    fn expected() -> Vec<String> {
        vec![
            DOCTOR_DENIES_CREATION.to_string(),
            DOCTOR_SHARDED_DIR.to_string(),
            DOCTOR_KEY_DIR.to_string(),
            SWEEP.to_string(),
        ]
    }

    fn log_with(rows: &[(&str, &str)]) -> String {
        rows.iter()
            .map(|(test, outcome)| format!("test {test} ... {outcome}\n"))
            .collect()
    }

    #[test]
    fn the_four_observed_rows_are_accepted() {
        let log = log_with(&[
            (DOCTOR_DENIES_CREATION, "ok"),
            (DOCTOR_SHARDED_DIR, "ok"),
            (DOCTOR_KEY_DIR, "ok"),
            (SWEEP, "ok"),
        ]);
        let problems = evidence_problems(&expected(), &parse_observations(&log), 0);
        assert!(
            problems.is_empty(),
            "the observed nightly evidence must be accepted: {problems:?}"
        );
    }

    /// A failed real-ACL test stays a test outcome, not missing evidence:
    /// the deny was exercised and the test noticed.
    #[test]
    fn a_failed_outcome_is_still_an_observation() {
        let log = log_with(&[
            (DOCTOR_DENIES_CREATION, "ok"),
            (DOCTOR_SHARDED_DIR, "FAILED"),
            (DOCTOR_KEY_DIR, "ok"),
            (SWEEP, "ok"),
        ]);
        let problems = evidence_problems(&expected(), &parse_observations(&log), 101);
        assert!(
            problems.is_empty(),
            "a FAILED row proves the test ran; the outcome is advisory: {problems:?}"
        );
    }

    /// The intentionally missing-filter control: renaming
    /// `real_unreadable_subtree_marks_corpus_selection_incomplete` away from
    /// the step's filters leaves three observed rows and a zero-match filter
    /// for the sweep. The guard must refuse, not accept the run as the
    /// status-only step did.
    #[test]
    fn a_missing_observation_is_an_evidence_failure() {
        let log = log_with(&[
            (DOCTOR_DENIES_CREATION, "ok"),
            (DOCTOR_SHARDED_DIR, "ok"),
            (DOCTOR_KEY_DIR, "ok"),
        ]);
        let problems = evidence_problems(&expected(), &parse_observations(&log), 0);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains(SWEEP) && problems[0].contains("did not run"),
            "the refusal must name the unobserved test: {problems:?}"
        );
    }

    #[test]
    fn a_duplicated_observation_is_an_evidence_failure() {
        let log = log_with(&[
            (DOCTOR_DENIES_CREATION, "ok"),
            (DOCTOR_SHARDED_DIR, "ok"),
            (DOCTOR_KEY_DIR, "ok"),
            (SWEEP, "ok"),
            (SWEEP, "ok"),
        ]);
        let problems = evidence_problems(&expected(), &parse_observations(&log), 0);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains(SWEEP) && problems[0].contains("observed 2 times"),
            "the refusal must name the duplicated test: {problems:?}"
        );
    }

    #[test]
    fn an_ignored_observation_is_an_evidence_failure() {
        let log = log_with(&[
            (DOCTOR_DENIES_CREATION, "ok"),
            (DOCTOR_SHARDED_DIR, "ok"),
            (DOCTOR_KEY_DIR, "ok"),
            (SWEEP, "ignored"),
        ]);
        let problems = evidence_problems(&expected(), &parse_observations(&log), 0);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains(SWEEP) && problems[0].contains("ignored"),
            "an ignored test never exercised the deny: {problems:?}"
        );
    }

    /// `windows-advisory-run` writes a status even when cargo could not
    /// start; `127` must be refused even though a status file exists.
    #[test]
    fn spawn_status_127_is_an_evidence_failure() {
        let log = log_with(&[(DOCTOR_DENIES_CREATION, "ok")]);
        let problems = evidence_problems(&expected(), &parse_observations(&log), 127);
        assert_eq!(problems.len(), 3, "{problems:?}");
        let log_path = write_temp("acl-guard-127.log", &log);
        let status_path = write_temp("acl-guard-127.status", "127");
        let guard = run(&[
            "--log".to_string(),
            log_path,
            "--status".to_string(),
            status_path,
            "--expect".to_string(),
            DOCTOR_DENIES_CREATION.to_string(),
        ]);
        assert!(
            guard.is_err(),
            "a spawn-failure status must fail the guard even with a status file present"
        );
    }

    #[test]
    fn a_missing_log_or_status_file_is_an_evidence_failure() {
        let status = write_temp("acl-guard-missing.status", "0");
        let missing_log = run(&[
            "--log".to_string(),
            "target/definitely-not-written.log".to_string(),
            "--status".to_string(),
            status,
            "--expect".to_string(),
            DOCTOR_DENIES_CREATION.to_string(),
        ]);
        assert!(missing_log.is_err(), "a missing log is not evidence");

        let log = write_temp("acl-guard-present.log", "test x ... ok\n");
        let missing_status = run(&[
            "--log".to_string(),
            log,
            "--status".to_string(),
            "target/definitely-not-written.status".to_string(),
            "--expect".to_string(),
            DOCTOR_DENIES_CREATION.to_string(),
        ]);
        assert!(missing_status.is_err(), "a missing status is not evidence");
    }

    #[test]
    fn the_step_pins_every_expected_real_acl_test_and_the_guard_fails_it() {
        let workflow = include_str!("../../.github/workflows/windows-advisory.yml");
        let step: Vec<&str> = workflow
            .lines()
            .skip_while(|line| {
                !line.contains("Run the real-ACL tests outside bash (advisory, #10621)")
            })
            .take_while(|line| !line.contains("- name: Summarize environment"))
            .collect();
        assert!(
            !step.is_empty(),
            "the pwsh real-ACL step must exist in the nightly workflow"
        );
        let step_text = step.join("\n");
        assert!(
            step_text.contains("windows-advisory-acl-evidence"),
            "the step must invoke the evidence guard: {step_text}"
        );
        for name in expected() {
            assert!(
                step_text.contains(&format!("--expect {name}")),
                "the step must pin the exact full libtest path {name}: {step_text}"
            );
        }
        let guard_lines: Vec<&str> = step
            .iter()
            .copied()
            .filter(|line| line.contains("windows-advisory-acl-evidence"))
            .collect();
        assert_eq!(
            guard_lines.len(),
            1,
            "the guard must be invoked exactly once: {guard_lines:?}"
        );
        assert!(
            !guard_lines[0].contains("|| true") && !guard_lines[0].contains("; exit 0"),
            "an evidence refusal must fail the step: {}",
            guard_lines[0]
        );
        assert!(
            !step_text.contains("|| true") && !step_text.contains("continue-on-error"),
            "the step may not swallow its own evidence refusal: {step_text}"
        );
    }

    fn write_temp(name: &str, contents: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "ripr-xtask-acl-guard-{}-{name}",
            std::process::id()
        ));
        let written = std::fs::write(&path, contents);
        assert!(
            written.is_ok(),
            "cannot write {}: {:?}",
            path.display(),
            written.err()
        );
        path.display().to_string()
    }
}
