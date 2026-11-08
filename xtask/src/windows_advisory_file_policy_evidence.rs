//! `cargo xtask windows-advisory-file-policy-evidence`: fail-closed admission
//! for the nightly Windows advisory lane's retained `file-policy.md` (#5044).
//!
//! Uploading the report path with `if-no-files-found: ignore` does not establish
//! that the matching native policy facade ran or that the retained bytes belong
//! to the current checkout. This command binds the artifact to:
//!
//! - a successful `policy_checker_facade_runs_current_repo_checks` observation
//!   in the facade log;
//! - the facade cargo exit status captured by `windows-advisory-run`;
//! - a present, nonempty `Status: pass` report;
//! - the subject sidecar written only after a successful `check-file-policy`
//!   producer on the same host, binding the exact report bytes by SHA-256; and
//! - the existing packet-coverage oracle shared with the facade test.

use std::fs;
use std::path::PathBuf;

use crate::policy::file_policy_retained_evidence::validate_retained_bundle;

const SPAWN_FAILED_STATUS: i32 = 127;
pub(crate) const FACADE_TEST: &str = "tests::policy_checker_facade_runs_current_repo_checks";

struct Options {
    report: PathBuf,
    subject: PathBuf,
    log: PathBuf,
    status: PathBuf,
    head: String,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let status = read_status(&options.status)?;
    if status == SPAWN_FAILED_STATUS {
        return Err(format!(
            "windows-advisory-file-policy-evidence: windows-advisory-run reported spawn status {SPAWN_FAILED_STATUS}; the policy facade never started, so the retained report is not evidence"
        ));
    }
    let log_text = fs::read_to_string(&options.log).map_err(|error| {
        format!(
            "windows-advisory-file-policy-evidence: cannot read the log file {}: {error}; a missing log is not evidence",
            options.log.display()
        )
    })?;
    let facade = facade_observation(&log_text);
    if status != 0 {
        return refuse_incomplete(
            &options,
            &facade,
            status,
            "the policy facade cargo run did not succeed; unrelated advisory test failures stay advisory, but successful file-policy evidence was not produced",
        );
    }
    let Some(observation) = facade else {
        return refuse_incomplete(
            &options,
            &None,
            status,
            "the policy facade test was not observed exactly once as ok in the facade log",
        );
    };
    if observation.outcome != "ok" {
        return refuse_incomplete(
            &options,
            &Some(observation),
            status,
            "the policy facade test did not report ok; a failed or ignored facade cannot qualify retained file-policy evidence",
        );
    }
    let report = fs::read_to_string(&options.report).map_err(|error| {
        format!(
            "windows-advisory-file-policy-evidence: cannot read the report {}: {error}; a missing report cannot qualify evidence even when upload succeeded elsewhere",
            options.report.display()
        )
    })?;
    let subject =
        crate::policy::file_policy_retained_evidence::read_retained_subject(&options.subject)?;
    if subject.host != "windows" {
        return Err(format!(
            "file-policy subject host {:?} is not windows-native retained evidence",
            subject.host
        ));
    }
    validate_retained_bundle(&report, &subject, &options.head)?;
    println!(
        "windows-advisory-file-policy-evidence: admitted retained file-policy evidence for head {} (facade cargo status {status}; test {} ... ok)",
        options.head, FACADE_TEST
    );
    Ok(())
}

struct FacadeObservation {
    outcome: String,
}

fn facade_observation(log: &str) -> Option<FacadeObservation> {
    let mut matches = log.lines().filter_map(|line| {
        let rest = line.strip_prefix("test ")?;
        let (test, outcome) = rest.split_once(" ... ")?;
        (test == FACADE_TEST).then(|| FacadeObservation {
            outcome: outcome.trim().to_string(),
        })
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(first)
}

fn read_status(path: &PathBuf) -> Result<i32, String> {
    let status_text = fs::read_to_string(path).map_err(|error| {
        format!(
            "windows-advisory-file-policy-evidence: cannot read the status file {}: {error}; a missing status is not evidence",
            path.display()
        )
    })?;
    status_text.trim().parse().map_err(|error| {
        format!(
            "windows-advisory-file-policy-evidence: the status file {} does not contain an exit status ({error}); an unparseable status is not evidence",
            path.display()
        )
    })
}

fn refuse_incomplete(
    options: &Options,
    facade: &Option<FacadeObservation>,
    status: i32,
    reason: &str,
) -> Result<(), String> {
    let facade_detail = match facade {
        None => "facade test not observed".to_string(),
        Some(observation) => format!("facade test observed ... {}", observation.outcome),
    };
    println!("::warning::{reason} ({facade_detail}; cargo status {status})");
    Err(format!(
        "windows-advisory-file-policy-evidence: incomplete evidence for head {}; {reason}",
        options.head
    ))
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut report: Option<PathBuf> = None;
    let mut subject: Option<PathBuf> = None;
    let mut log: Option<PathBuf> = None;
    let mut status: Option<PathBuf> = None;
    let mut head: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let value = |iter: &mut std::slice::Iter<String>, name: &str| {
            iter.next()
                .ok_or_else(|| {
                    format!("windows-advisory-file-policy-evidence: {name} requires a value")
                })
                .cloned()
        };
        match arg.as_str() {
            "--report" => report = Some(PathBuf::from(value(&mut iter, "--report")?)),
            "--subject" => subject = Some(PathBuf::from(value(&mut iter, "--subject")?)),
            "--log" => log = Some(PathBuf::from(value(&mut iter, "--log")?)),
            "--status" => status = Some(PathBuf::from(value(&mut iter, "--status")?)),
            "--head" => head = Some(value(&mut iter, "--head")?),
            other => {
                return Err(format!(
                    "windows-advisory-file-policy-evidence: unknown argument {other}; usage: windows-advisory-file-policy-evidence --report <path> --subject <path> --log <path> --status <path> --head <sha>"
                ));
            }
        }
    }
    Ok(Options {
        report: report.ok_or_else(|| {
            "windows-advisory-file-policy-evidence: --report <path> is required".to_string()
        })?,
        subject: subject.ok_or_else(|| {
            "windows-advisory-file-policy-evidence: --subject <path> is required".to_string()
        })?,
        log: log.ok_or_else(|| {
            "windows-advisory-file-policy-evidence: --log <path> is required".to_string()
        })?,
        status: status.ok_or_else(|| {
            "windows-advisory-file-policy-evidence: --status <path> is required".to_string()
        })?,
        head: {
            let head = head.ok_or_else(|| {
                "windows-advisory-file-policy-evidence: --head <sha> is required".to_string()
            })?;
            if head.len() != 40 || !head.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Err(format!(
                    "windows-advisory-file-policy-evidence: --head must be a 40-character commit, got {head:?}"
                ));
            }
            head
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::file_policy_retained_evidence::{
        SUBJECT_RELATIVE, report_sha256, validate_packet_coverage_report,
    };
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_temp(name: &str, body: &str) -> Result<String, String> {
        let path = std::env::temp_dir().join(format!(
            "ripr-file-policy-evidence-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        fs::write(&path, body).map_err(|error| {
            format!(
                "file-policy evidence test fixture write failed for {}: {error}",
                path.display()
            )
        })?;
        Ok(path.to_string_lossy().into_owned())
    }

    fn valid_report(host: &str) -> String {
        let native = if host == "windows" {
            "not_applicable; selected=not_enumerated".to_string()
        } else {
            concat!(
                "applicable; selected=2; tests=[",
                "native_packet_analyzes_a_boundary_gap_without_path_or_compiler_fallback, ",
                "native_packet_pilot_consumes_the_summary_artifact]",
            )
            .to_string()
        };
        format!(
            concat!(
                "# check-file-policy\n\nStatus: pass\n\n",
                "- line 1; host={host}; declared=all; applicable; selected=2; tests=[",
                "portable_consumer::tests::consumer_source_does_not_search_path_or_open_a_network_client, ",
                "portable_consumer::tests::packet_digest_matches_the_producer_formula]; ",
                "`cargo test -p xtask --bin xtask --locked --offline portable_consumer`\n",
                "- line 1; host={host}; declared=unix; {native}; ",
                "`cargo test -p ripr --locked --offline --test portable_consumer_packet`\n",
            ),
            host = host,
            native = native,
        )
    }

    fn valid_subject(head: &str, host: &str) -> String {
        serde_json::json!({
            "head": head,
            "host": host,
            "status": "pass",
            "report_sha256": report_sha256(&valid_report(host)),
        })
        .to_string()
    }

    fn ok_log() -> String {
        // The parent replays the captured child diagnostic before libtest
        // prints the parent's own result. Exercise that real producer format.
        let child = crate::run::TimedOutput {
            status: None,
            stdout: format!(
                "\nrunning 1 test\ntest {FACADE_TEST} ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
            ),
            stderr:
                "policy facade phase: check_file_policy\npolicy facade child checks completed\n"
                    .to_string(),
            duration: std::time::Duration::ZERO,
            timed_out: false,
        };
        format!(
            "{}\ntest {FACADE_TEST} ... ok\n",
            crate::tests::policy_facade_child_diagnostic(&child)
        )
    }

    #[test]
    fn child_diagnostic_does_not_duplicate_the_parent_observation() {
        let log = ok_log();
        assert!(facade_observation(&log).is_some_and(|row| row.outcome == "ok"));
        assert!(log.contains("policy facade phase: check_file_policy"));
        let duplicate = format!("{log}test {FACADE_TEST} ... ok\n");
        assert!(facade_observation(&duplicate).is_none());
    }

    #[test]
    fn present_matching_bundle_is_admitted() -> Result<(), String> {
        let head = "a".repeat(40);
        let report = write_temp("report", &valid_report("windows"))?;
        let subject = write_temp("subject", &valid_subject(&head, "windows"))?;
        let log = write_temp("log", &ok_log())?;
        let status = write_temp("status", "0")?;
        run(&[
            "--report".to_string(),
            report,
            "--subject".to_string(),
            subject,
            "--log".to_string(),
            log,
            "--status".to_string(),
            status,
            "--head".to_string(),
            head,
        ])
    }

    #[test]
    fn substituted_pass_report_with_matching_subject_is_refused() -> Result<(), String> {
        let head = "a".repeat(40);
        let original = valid_report("windows");
        let substituted = format!("{original}\n## Diagnostics from another execution\n");
        // Both reports independently satisfy the current packet oracle. The
        // subject must bind to the original bytes, not just the same shape.
        validate_packet_coverage_report(&substituted, "windows")?;
        let report = write_temp("report-substituted", &substituted)?;
        let subject = write_temp("subject-original", &valid_subject(&head, "windows"))?;
        let log = write_temp("log", &ok_log())?;
        let status = write_temp("status", "0")?;
        let guard = run(&[
            "--report".to_string(),
            report,
            "--subject".to_string(),
            subject,
            "--log".to_string(),
            log,
            "--status".to_string(),
            status,
            "--head".to_string(),
            head,
        ]);
        let Err(refusal) = guard else {
            return Err(
                "a pass-shaped report from another execution must not substitute".to_string(),
            );
        };
        assert!(refusal.contains("report digest"));
        Ok(())
    }

    #[test]
    fn subject_without_report_digest_is_refused() -> Result<(), String> {
        let head = "b".repeat(40);
        let report = write_temp("report", &valid_report("windows"))?;
        let subject = write_temp(
            "legacy-subject",
            &serde_json::json!({"head": head, "host": "windows", "status": "pass"}).to_string(),
        )?;
        let log = write_temp("log", &ok_log())?;
        let status = write_temp("status", "0")?;
        let guard = run(&[
            "--report".to_string(),
            report,
            "--subject".to_string(),
            subject,
            "--log".to_string(),
            log,
            "--status".to_string(),
            status,
            "--head".to_string(),
            head,
        ]);
        let Err(refusal) = guard else {
            return Err("an unbound sidecar must not qualify evidence".to_string());
        };
        assert!(refusal.contains("report_sha256"));
        Ok(())
    }

    #[test]
    fn removed_report_is_refused() -> Result<(), String> {
        let head = "b".repeat(40);
        let subject = write_temp("subject", &valid_subject(&head, "windows"))?;
        let log = write_temp("log", &ok_log())?;
        let status = write_temp("status", "0")?;
        let guard = run(&[
            "--report".to_string(),
            "target/definitely-missing-file-policy.md".to_string(),
            "--subject".to_string(),
            subject,
            "--log".to_string(),
            log,
            "--status".to_string(),
            status,
            "--head".to_string(),
            head,
        ]);
        assert!(guard.is_err(), "a missing report must not qualify evidence");
        Ok(())
    }

    #[test]
    fn empty_report_is_refused() -> Result<(), String> {
        let head = "c".repeat(40);
        let report = write_temp("report", "")?;
        let subject = write_temp("subject", &valid_subject(&head, "windows"))?;
        let log = write_temp("log", &ok_log())?;
        let status = write_temp("status", "0")?;
        let guard = run(&[
            "--report".to_string(),
            report,
            "--subject".to_string(),
            subject,
            "--log".to_string(),
            log,
            "--status".to_string(),
            status,
            "--head".to_string(),
            head,
        ]);
        assert!(guard.is_err());
        Ok(())
    }

    #[test]
    fn stale_head_in_subject_is_refused() -> Result<(), String> {
        let head = "d".repeat(40);
        let stale = "e".repeat(40);
        let report = write_temp("report", &valid_report("windows"))?;
        let subject = write_temp("subject", &valid_subject(&stale, "windows"))?;
        let log = write_temp("log", &ok_log())?;
        let status = write_temp("status", "0")?;
        let guard = run(&[
            "--report".to_string(),
            report,
            "--subject".to_string(),
            subject,
            "--log".to_string(),
            log,
            "--status".to_string(),
            status,
            "--head".to_string(),
            head,
        ]);
        assert!(guard.is_err(), "stale subject head must not substitute");
        Ok(())
    }

    #[test]
    fn wrong_host_rows_are_refused() -> Result<(), String> {
        let head = "f".repeat(40);
        let report = write_temp("report", &valid_report("unix"))?;
        let subject = write_temp("subject", &valid_subject(&head, "unix"))?;
        let log = write_temp("log", &ok_log())?;
        let status = write_temp("status", "0")?;
        let guard = run(&[
            "--report".to_string(),
            report,
            "--subject".to_string(),
            subject,
            "--log".to_string(),
            log,
            "--status".to_string(),
            status,
            "--head".to_string(),
            head,
        ]);
        assert!(guard.is_err(), "wrong host identity must not substitute");
        validate_packet_coverage_report(&valid_report("unix"), "unix")?;
        Ok(())
    }

    #[test]
    fn failed_facade_producer_is_refused() -> Result<(), String> {
        let head = "0".repeat(40);
        let report = write_temp("report", &valid_report("windows"))?;
        let subject = write_temp("subject", &valid_subject(&head, "windows"))?;
        let log = write_temp("log", &format!("test {FACADE_TEST} ... FAILED\n"))?;
        let status = write_temp("status", "101")?;
        let guard = run(&[
            "--report".to_string(),
            report,
            "--subject".to_string(),
            subject,
            "--log".to_string(),
            log,
            "--status".to_string(),
            status,
            "--head".to_string(),
            head,
        ]);
        assert!(guard.is_err());
        Ok(())
    }

    #[test]
    fn the_nightly_workflow_pins_the_admission_command() {
        let workflow = include_str!("../../.github/workflows/windows-advisory.yml");
        assert!(
            workflow.contains("windows-advisory-file-policy-evidence"),
            "the nightly workflow must invoke the retained file-policy admission guard"
        );
        assert!(
            workflow.contains("--head \"$(git rev-parse HEAD)\""),
            "the guard must bind to the checkout head"
        );
        assert!(
            workflow.contains(SUBJECT_RELATIVE) || workflow.contains("file-policy.subject.json"),
            "the guard must read the subject sidecar"
        );
    }

    #[test]
    fn subject_sidecar_path_matches_report_directory() {
        assert!(
            Path::new(SUBJECT_RELATIVE)
                .parent()
                .is_some_and(|dir| dir.ends_with("reports")),
            "subject sidecar must live beside the retained report"
        );
    }
}
