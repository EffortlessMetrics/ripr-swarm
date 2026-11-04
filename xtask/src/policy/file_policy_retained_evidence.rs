//! Retained `file-policy.md` evidence contract for native Windows admission (#5044).
//!
//! The policy facade and its report oracle already define which coverage rows
//! must appear for the current host. This module is the shared authority for
//! that report shape and for the small subject sidecar written only after a
//! successful `check-file-policy` producer run.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub(crate) const SUBJECT_RELATIVE: &str = "target/ripr/reports/file-policy.subject.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FilePolicyRetainedSubject {
    pub head: String,
    pub host: String,
    pub status: String,
}

pub(crate) fn host_label() -> &'static str {
    if cfg!(windows) { "windows" } else { "unix" }
}

pub(crate) fn write_retained_subject() -> Result<(), String> {
    let head = crate::run::run_output("git", &["rev-parse", "HEAD"])
        .map_err(|error| format!("file-policy subject: cannot read git HEAD: {error}"))?
        .trim()
        .to_string();
    if head.len() != 40 || !head.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "file-policy subject: git HEAD is not a 40-character commit: {head:?}"
        ));
    }
    let subject = FilePolicyRetainedSubject {
        head,
        host: host_label().to_string(),
        status: "pass".to_string(),
    };
    let json = serde_json::to_string_pretty(&subject)
        .map_err(|error| format!("file-policy subject: cannot encode JSON: {error}"))?;
    crate::write_report("file-policy.subject.json", &format!("{json}\n"))
}

pub(crate) fn remove_retained_subject() {
    let path = PathBuf::from(SUBJECT_RELATIVE);
    let _ = fs::remove_file(path);
}

pub(crate) fn read_retained_subject(path: &Path) -> Result<FilePolicyRetainedSubject, String> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "file-policy subject missing or unreadable at {}: {error}",
            path.display()
        )
    })?;
    if text.trim().is_empty() {
        return Err(format!(
            "file-policy subject at {} is empty; an empty sidecar is not evidence",
            path.display()
        ));
    }
    serde_json::from_str(&text).map_err(|error| {
        format!(
            "file-policy subject at {} is malformed JSON: {error}",
            path.display()
        )
    })
}

pub(crate) fn report_producer_status(report: &str) -> Result<&str, String> {
    report
        .lines()
        .find_map(|line| line.strip_prefix("Status: "))
        .ok_or_else(|| {
            "retained file-policy report is missing a Status line; upload success alone is not evidence".to_string()
        })
}

pub(crate) fn validate_nonempty_pass_report(report: &str) -> Result<(), String> {
    if report.trim().is_empty() {
        return Err(
            "retained file-policy report is empty; a missing or truncated report cannot qualify evidence"
                .to_string(),
        );
    }
    match report_producer_status(report)? {
        "pass" => Ok(()),
        other => Err(format!(
            "retained file-policy report Status is {other:?}; only a successful producer run qualifies evidence"
        )),
    }
}

pub(crate) fn validate_packet_coverage_report(report: &str, host: &str) -> Result<(), String> {
    let common = report
        .lines()
        .find(|line| {
            line.ends_with("`cargo test -p xtask --bin xtask --locked --offline portable_consumer`")
        })
        .ok_or("common packet coverage is absent from the policy report")?;
    let common_tests = packet_coverage_selected_tests(common)?;
    for subject in [
        "portable_consumer::tests::consumer_source_does_not_search_path_or_open_a_network_client",
        "portable_consumer::tests::packet_digest_matches_the_producer_formula",
    ] {
        if !common_tests.contains(&subject) || !common.contains("declared=all; applicable;") {
            return Err(format!(
                "common packet subject not selected: {subject}: {common}"
            ));
        }
    }
    let native = report
        .lines()
        .find(|line| {
            line.ends_with(
                "`cargo test -p ripr --locked --offline --test portable_consumer_packet`",
            )
        })
        .ok_or("native packet coverage is absent from the policy report")?;
    if packet_coverage_selected_count(common)? < 2 {
        return Err(format!(
            "common packet selection lost required subjects: {common}"
        ));
    }
    match host {
        "windows" => {
            if !common.contains("host=windows; declared=all; applicable;")
                || !native.contains(
                    "host=windows; declared=unix; not_applicable; selected=not_enumerated;",
                )
                || native.contains("tests=[")
            {
                return Err(format!(
                    "Windows packet applicability drifted: {common}\n{native}"
                ));
            }
        }
        "unix" => {
            let native_tests = packet_coverage_selected_tests(native)?;
            if !native.contains("host=unix; declared=unix; applicable;")
                || packet_coverage_selected_count(native)? < 2
            {
                return Err(format!(
                    "Unix native packet selector was not retained: {native}"
                ));
            }
            for subject in [
                "native_packet_analyzes_a_boundary_gap_without_path_or_compiler_fallback",
                "native_packet_pilot_consumes_the_summary_artifact",
            ] {
                if !native_tests.contains(&subject) {
                    return Err(format!(
                        "Unix native packet subject missing: {subject}: {native}"
                    ));
                }
            }
        }
        other => {
            return Err(format!(
                "retained file-policy host must be windows or unix, got {other:?}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_retained_bundle(
    report: &str,
    subject: &FilePolicyRetainedSubject,
    expected_head: &str,
) -> Result<(), String> {
    validate_nonempty_pass_report(report)?;
    if subject.status != "pass" {
        return Err(format!(
            "file-policy subject status is {:?}; only pass qualifies retained evidence",
            subject.status
        ));
    }
    if subject.head != expected_head {
        return Err(format!(
            "file-policy subject head {} does not match expected checkout {}",
            subject.head, expected_head
        ));
    }
    if subject.host != host_label() && subject.host != "windows" && subject.host != "unix" {
        return Err(format!(
            "file-policy subject host {:?} is not a known host label",
            subject.host
        ));
    }
    validate_packet_coverage_report(report, &subject.host)
}

fn packet_coverage_selected_count(row: &str) -> Result<usize, String> {
    row.split_once("; selected=")
        .and_then(|(_, rest)| rest.split_once(';'))
        .ok_or_else(|| format!("selected count absent from coverage row: {row}"))?
        .0
        .parse()
        .map_err(|error| format!("invalid selected count in {row}: {error}"))
}

fn packet_coverage_selected_tests(row: &str) -> Result<Vec<&str>, String> {
    let tests = row
        .rsplit_once("; `")
        .and_then(|(fields, _)| fields.split_once("; tests=["))
        .and_then(|(_, tests)| tests.strip_suffix(']'))
        .ok_or_else(|| format!("selected tests field missing or malformed: {row}"))?;
    let selected: Vec<_> = tests.split(", ").collect();
    if selected.iter().any(|test| {
        test.is_empty() || test.contains([',', '[', ']', ';']) || test.contains(char::is_whitespace)
    }) || selected.len() != packet_coverage_selected_count(row)?
    {
        return Err(format!("selected identities and count disagree: {row}"));
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_report(host: &str) -> String {
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

    #[test]
    fn pass_report_with_windows_rows_is_admitted() -> Result<(), String> {
        validate_packet_coverage_report(&fixture_report("windows"), "windows")
    }

    #[test]
    fn empty_report_is_refused() {
        assert!(validate_nonempty_pass_report("").is_err());
    }

    #[test]
    fn stopped_report_is_refused() {
        assert!(
            validate_nonempty_pass_report("# check-file-policy\n\nStatus: stopped during x\n")
                .is_err()
        );
    }
}
