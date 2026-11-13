//! Actual native-entry/process controls using a synthetic provider. Narrow
//! execution is selected inside formal_runner; automation uses the full xtask.
//! Raw observations here are deliberately not native proof evidence.
#![cfg(target_os = "linux")]

use std::fs;
use std::time::Duration;
use xtask::formal_evidence::{
    ProcessState, Verdict,
    native::{self, ObservationStatus},
};

#[cfg(feature = "automation")]
#[path = "common/native_fixture.rs"]
mod native_fixture;
#[cfg(not(feature = "automation"))]
use crate::native_fixture;
use native_fixture::{Fixture, Observation, child_selector, lane_args, successful_block};

fn counterexample_block(qualified: &str) -> String {
    format!(
        "Checking harness {qualified}...\nCheck 1: {qualified}.assertion.1\n\t - Status: FAILURE\n\t - Description: \"synthetic assertion\"\n\t - Location: fixture.rs:1:1 in function {qualified}\nVERIFICATION:- FAILED\nVerification Time: 0.1s\n"
    )
}

fn fixture(harnesses: &[(&str, &str, &str)], output: &str) -> Result<Fixture, String> {
    let mut fixture = Fixture::new("process")?;
    for (file, name, qualified) in harnesses {
        fixture.add_harness("ripr", file, name, qualified)?;
    }
    fixture.write("ripr-output", output.as_bytes())?;
    Ok(fixture)
}

fn drive(fixture: &Fixture, mode: &str, args: &[String]) -> Result<Observation, String> {
    let observed = fixture.drive(
        child_selector(concat!(module_path!(), "::kani_process_watchdog_child"))?,
        "normal",
        mode,
        args,
        Duration::from_secs(30),
    )?;
    fixture.retain_on_error(observed.require_driver_completion())?;
    Ok(observed)
}

fn run(fixture: &Fixture, mode: &str, allow_empty: bool) -> Result<Observation, String> {
    let observed = drive(
        fixture,
        mode,
        &lane_args(if mode == "timeout" { 3 } else { 20 }, allow_empty),
    )?;
    assert_eq!(
        observed.calls.first().map(String::as_str),
        Some("kani --version")
    );
    assert_eq!(
        observed
            .calls
            .iter()
            .filter(|s| s.starts_with("metadata "))
            .count(),
        1
    );
    assert_eq!(
        observed
            .calls
            .iter()
            .filter(|s| s.ends_with("list --format json"))
            .count(),
        2
    );
    assert_eq!(
        String::from_utf8_lossy(&observed.output.stdout).trim_end(),
        observed.report.trim_end(),
        "the actual entry must print its canonical Markdown"
    );
    Ok(observed)
}

fn assert_row(
    observed: &Observation,
    file: &str,
    name: &str,
    qualified: &str,
    status: ObservationStatus,
) -> Result<(), String> {
    let file = format!("crates/ripr/src/{file}");
    let row = observed
        .run
        .unregistered_results
        .iter()
        .find(|row| {
            row.subject.file == file && row.subject.qualified_name.as_deref() == Some(qualified)
        })
        .ok_or_else(|| {
            format!(
                "missing exact native row {file}::{qualified}: {}",
                observed.report
            )
        })?;
    assert_eq!(row.status, status, "{file}::{qualified}");
    let label = match status {
        ObservationStatus::Verified => "VERIFIED",
        ObservationStatus::Failed => "FAILED",
        ObservationStatus::NotEstablished => "NOT-ESTABLISHED",
    };
    assert!(
        observed
            .report
            .contains(&format!("| `{name}` | `{file}` | {label} |")),
        "exact JSON row and Markdown must agree: {}",
        observed.report
    );
    Ok(())
}

fn artifact_directory(
    fixture: &Fixture,
    observed: &Observation,
) -> Result<std::path::PathBuf, String> {
    Ok(fixture
        .root
        .join("target/ripr/reports")
        .join(&observed.execution()?.artifact_directory))
}

#[test]
fn kani_process_watchdog_child() -> Result<(), String> {
    native_fixture::watchdog_child()
}

#[test]
fn kani_runner_retains_unique_success_and_explicit_failure() -> Result<(), String> {
    let fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
    let observed = run(&fixture, "success", false)?;
    assert_eq!(
        observed.output.status.code(),
        Some(0),
        "{}",
        observed.report
    );
    assert_row(
        &observed,
        "one.rs",
        "one",
        "a::one",
        ObservationStatus::Verified,
    )?;
    assert!(
        observed
            .report
            .contains("within the harness bounds | 0.1s |")
    );
    assert!(
        observed.run.results.is_empty(),
        "synthetic unregistered rows establish no semantic claim"
    );
    native::verify_execution_artifacts(&observed.run, &artifact_directory(&fixture, &observed)?)?;
    let verification = observed
        .execution()?
        .invocations
        .iter()
        .find(|i| i.phase == "verification")
        .ok_or("verification observation absent")?;
    let raw = native::verify_artifact(
        &artifact_directory(&fixture, &observed)?,
        verification
            .stdout
            .as_ref()
            .ok_or("stdout artifact absent")?,
        16 * 1024 * 1024,
    )?;
    assert_eq!(
        raw,
        format!(
            "{}{}",
            native_fixture::expected_headers(),
            successful_block("a::one")
        )
        .as_bytes()
    );

    let fixture = self::fixture(
        &[("one.rs", "one", "a::one")],
        &counterexample_block("a::one"),
    )?;
    let observed = run(&fixture, "failure", false)?;
    assert_eq!(observed.output.status.code(), Some(1));
    assert_row(
        &observed,
        "one.rs",
        "one",
        "a::one",
        ObservationStatus::Failed,
    )?;
    Ok(())
}

#[test]
fn kani_runner_requires_observed_complete_cover_evidence() -> Result<(), String> {
    let cases = [
        ("", ObservationStatus::NotEstablished),
        (
            " ** 0 of 0 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            "Check 1: a::one.assertion.1\n\t - Status: FAILURE\nCheck 2: a::one.cover.1\n\t - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            " ** invalid of 1 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            " ** 1 of 1 cover properties satisfied\n ** 1 of 1 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            " ** 1 of 2 cover properties satisfied (1 undetermined)\n",
            ObservationStatus::NotEstablished,
        ),
        (
            "Check 1: a::one.cover.1\n\t - Status: UNDETERMINED\n ** 1 of 1 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            "Check 1: a::one.cover.1\n\t - Status: ERROR\n ** 1 of 1 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            " ** 1 of 1 cover properties satisfied (1 unreachable)\n",
            ObservationStatus::NotEstablished,
        ),
        (
            "Check 1: a::one.cover.1\n\t - Status: UNREACHABLE\n ** 1 of 1 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            "Check 1: a::one.cover.1\n\t - Status: COVERED\n ** 1 of 1 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            "Check 1: a::one.cover.1\n\t - Status: UNCOVERED\n ** 1 of 1 cover properties satisfied\n",
            ObservationStatus::NotEstablished,
        ),
        (
            "Check 1: a::one.cover.1\n\t - Status: UNSATISFIABLE\n ** 0 of 1 cover properties satisfied\n",
            ObservationStatus::Failed,
        ),
        (
            "Check 1: a::one.cover.1\n\t - Status: SATISFIED\nCheck 2: a::one.cover.2\n\t - Status: UNSATISFIABLE\n ** 1 of 2 cover properties satisfied (1 unreachable)\n",
            ObservationStatus::Failed,
        ),
    ];
    for (evidence, status) in cases {
        for allow_empty in [false, true] {
            let output =
                format!("Checking harness a::one...\n{evidence}VERIFICATION:- SUCCESSFUL\n");
            let fixture = fixture(&[("one.rs", "one", "a::one")], &output)?;
            let observed = run(&fixture, "success", allow_empty)?;
            assert_eq!(observed.output.status.code(), Some(1), "{output}");
            assert_row(&observed, "one.rs", "one", "a::one", status.clone())?;
            assert!(!observed.execution()?.lane_success);
            assert!(!observed.report.contains("| VERIFIED |"));
        }
    }
    Ok(())
}

#[test]
fn kani_runner_refuses_success_text_after_nonzero_or_signal() -> Result<(), String> {
    for mode in ["failure", "signal"] {
        for allow_empty in [false, true] {
            let fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
            let observed = run(&fixture, mode, allow_empty)?;
            assert_eq!(observed.output.status.code(), Some(1));
            assert_row(
                &observed,
                "one.rs",
                "one",
                "a::one",
                ObservationStatus::NotEstablished,
            )?;
            assert_eq!(
                observed.run.unregistered_results[0].native_verdict,
                Verdict::Verified
            );
            assert!(
                observed.run.unregistered_results[0]
                    .raw_block_digest
                    .is_some()
            );
            assert!(!observed.execution()?.lane_success);
            assert!(
                observed
                    .report
                    .contains("native verification did not complete successfully")
            );
            assert!(
                String::from_utf8_lossy(&observed.output.stderr).contains("native Kani execution")
            );
        }
    }
    Ok(())
}

#[test]
fn kani_runner_uses_compiled_identity_for_same_named_sources() -> Result<(), String> {
    let success = successful_block("a::same");
    let failure = counterexample_block("b::same");
    let unique = successful_block("c::unique");
    for output in [
        format!("{success}{unique}"),
        format!("{success}{failure}{unique}"),
        format!("{failure}{success}{unique}"),
    ] {
        let fixture = fixture(
            &[
                ("a.rs", "same", "a::same"),
                ("b.rs", "same", "b::same"),
                ("c.rs", "unique", "c::unique"),
            ],
            &output,
        )?;
        let observed = run(&fixture, "success", true)?;
        assert_eq!(observed.output.status.code(), Some(1));
        assert_row(
            &observed,
            "a.rs",
            "same",
            "a::same",
            ObservationStatus::Verified,
        )?;
        assert_row(
            &observed,
            "b.rs",
            "same",
            "b::same",
            if output.contains("b::same") {
                ObservationStatus::Failed
            } else {
                ObservationStatus::NotEstablished
            },
        )?;
        assert_row(
            &observed,
            "c.rs",
            "unique",
            "c::unique",
            ObservationStatus::Verified,
        )?;
    }
    Ok(())
}

#[test]
fn kani_runner_refuses_repeated_blocks_and_absent_results() -> Result<(), String> {
    let pass = successful_block("a::one");
    for output in [
        format!("{pass}{pass}"),
        "compiled without a verdict\n".into(),
        "Checking harness a::one...\nVERIFICATION:- FAILED\n".into(),
    ] {
        let fixture = fixture(&[("one.rs", "one", "a::one")], &output)?;
        let observed = run(&fixture, "success", false)?;
        assert_eq!(observed.output.status.code(), Some(1));
        assert_row(
            &observed,
            "one.rs",
            "one",
            "a::one",
            ObservationStatus::NotEstablished,
        )?;
    }
    Ok(())
}

#[test]
fn kani_runner_preserves_completed_rows_after_a_real_timeout() -> Result<(), String> {
    let output = format!(
        "{}Checking harness b::two...\nCompiling\n",
        successful_block("a::one")
    );
    let fixture = fixture(
        &[("one.rs", "one", "a::one"), ("two.rs", "two", "b::two")],
        &output,
    )?;
    let observed = run(&fixture, "timeout", false)?;
    assert_eq!(observed.output.status.code(), Some(1));
    assert_eq!(observed.run.process.state, ProcessState::TimedOut);
    assert_row(
        &observed,
        "one.rs",
        "one",
        "a::one",
        ObservationStatus::NotEstablished,
    )?;
    assert_eq!(
        observed.run.unregistered_results[0].native_verdict,
        Verdict::Verified
    );
    assert!(
        observed.run.unregistered_results[0]
            .raw_block_digest
            .is_some()
    );
    assert_row(
        &observed,
        "two.rs",
        "two",
        "b::two",
        ObservationStatus::NotEstablished,
    )?;
    assert!(observed.owned_descendants_observed >= 1);
    assert!(!observed.execution()?.lane_success);
    Ok(())
}

#[test]
fn kani_runner_requires_contract_package_results() -> Result<(), String> {
    for (output, success) in [
        (successful_block("domain::contract"), true),
        ("compiled without a verdict\n".into(), false),
        ("Checking harness domain::contract...\n** 0 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n".into(), false),
    ] {
        let mut fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
        fixture.add_harness("ripr-contract", "contract.rs", "contract", "domain::contract")?;
        fixture.write("ripr-contract-output", output.as_bytes())?;
        let observed = run(&fixture, "success", false)?;
        assert_eq!(observed.output.status.success(), success, "{}", observed.report);
        assert_eq!(observed.calls.iter().filter(|s| s.ends_with("--verbose")).count(), 2);
        assert!(observed.report.contains("crates/ripr-contract/src/contract.rs"));
    }
    Ok(())
}

#[test]
fn kani_runner_invalid_arguments_replace_prior_success_with_an_initial_checkpoint()
-> Result<(), String> {
    let fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
    let first = run(&fixture, "success", false)?;
    assert!(first.output.status.success(), "{}", first.report);
    let prior = artifact_directory(&fixture, &first)?.join("kani.json");
    let retained = fs::read(&prior).map_err(|e| e.to_string())?;
    let refused = drive(&fixture, "success", &["--invalid-fixture-option".into()])?;
    assert!(!refused.output.status.success());
    assert!(
        refused.calls.is_empty(),
        "argument refusal must precede tool invocation"
    );
    assert_ne!(refused.run.run_id, first.run.run_id);
    assert_eq!(refused.run.process.state, ProcessState::NotStarted);
    assert!(!refused.execution()?.lane_success);
    assert!(refused.run.unregistered_results.is_empty());
    assert!(
        refused
            .execution()?
            .artifact_directory
            .ends_with("/initial")
    );
    assert!(refused.report.contains("NOT-ESTABLISHED") && !refused.report.contains("| VERIFIED |"));
    assert_eq!(
        fs::read(prior).map_err(|e| e.to_string())?,
        retained,
        "prior immutable evidence changed"
    );
    Ok(())
}

#[test]
fn kani_runner_midrun_publication_error_cannot_republish_prior_success() -> Result<(), String> {
    let fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
    let first = run(&fixture, "success", false)?;
    assert!(first.output.status.success(), "{}", first.report);
    let prior = artifact_directory(&fixture, &first)?.join("kani.json");
    let retained = fs::read(&prior).map_err(|e| e.to_string())?;
    let refused = drive(&fixture, "midrun-error", &lane_args(20, false))?;
    assert!(!refused.output.status.success());
    assert_eq!(
        refused
            .calls
            .iter()
            .filter(|s| s.ends_with("--verbose"))
            .count(),
        1
    );
    assert!(String::from_utf8_lossy(&refused.output.stderr).contains("raw-index.json"));
    assert_ne!(refused.run.run_id, first.run.run_id);
    assert!(!refused.execution()?.lane_success);
    assert_eq!(refused.run.process.state, ProcessState::NotStarted);
    assert!(
        refused
            .execution()?
            .artifact_directory
            .ends_with("/initial")
    );
    assert_eq!(fs::read(prior).map_err(|e| e.to_string())?, retained);
    Ok(())
}

#[test]
fn kani_runner_source_drift_retains_native_bytes_without_current_success() -> Result<(), String> {
    let fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
    let observed = run(&fixture, "drift", false)?;
    assert!(!observed.output.status.success());
    assert!(!observed.execution()?.source_current && !observed.execution()?.lane_success);
    assert_ne!(
        observed.execution()?.source_before.manifest,
        observed.execution()?.source_after.manifest
    );
    assert_eq!(
        observed.run.unregistered_results[0].native_verdict,
        Verdict::Verified
    );
    assert!(
        observed.run.unregistered_results[0]
            .raw_block_digest
            .is_some()
    );
    assert!(
        observed
            .report
            .contains("source/configuration identity changed")
    );
    Ok(())
}

#[test]
fn kani_runner_accepts_a_legitimate_git_inventory_larger_than_one_mebibyte() -> Result<(), String> {
    let mut fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
    for number in 0..6000 {
        let path = format!("notes/{number:04}-{}.txt", "n".repeat(180));
        fixture.write(&path, b"")?;
        fixture.source_paths.insert(path);
    }
    let observed = run(&fixture, "success", false)?;
    assert!(observed.output.status.success(), "{}", observed.report);
    let listing = fs::metadata(fixture.root.join("source-list"))
        .map_err(|e| e.to_string())?
        .len();
    assert!(listing > 1024 * 1024 && listing < 4 * 1024 * 1024);
    let git_lists: Vec<_> = observed
        .execution()?
        .invocations
        .iter()
        .filter(|i| i.program == "git" && i.arguments.first().is_some_and(|a| a == "ls-files"))
        .collect();
    assert_eq!(
        git_lists.len(),
        2,
        "both source observations must retain the nonempty inventory"
    );
    for invocation in git_lists {
        assert!(invocation.succeeded());
        assert_eq!(
            invocation.stdout.as_ref().ok_or("Git stream absent")?.bytes,
            listing
        );
    }
    Ok(())
}

#[test]
fn kani_runner_rejects_malformed_native_inventory_before_verification() -> Result<(), String> {
    let fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
    let observed = run(&fixture, "malformed-list", false)?;
    assert!(!observed.output.status.success());
    assert_eq!(
        observed
            .calls
            .iter()
            .filter(|s| s.ends_with("--verbose"))
            .count(),
        0
    );
    assert_eq!(observed.run.process.state, ProcessState::NotStarted);
    assert!(observed.run.unregistered_results.is_empty());
    assert!(!observed.execution()?.inventory_rejections.is_empty());
    Ok(())
}

#[test]
fn kani_runner_retains_non_utf8_bytes_without_a_successful_native_row() -> Result<(), String> {
    let fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
    let observed = run(&fixture, "nonutf8", false)?;
    assert!(!observed.output.status.success());
    assert_row(
        &observed,
        "one.rs",
        "one",
        "a::one",
        ObservationStatus::NotEstablished,
    )?;
    let invocation = observed
        .execution()?
        .invocations
        .iter()
        .find(|i| i.phase == "verification")
        .ok_or("verification invocation absent")?;
    let raw = native::verify_artifact(
        &artifact_directory(&fixture, &observed)?,
        invocation.stdout.as_ref().ok_or("native stdout absent")?,
        16 * 1024 * 1024,
    )?;
    assert_eq!(raw.last(), Some(&255));
    assert!(matches!(
        std::str::from_utf8(&raw),
        Err(error) if error.valid_up_to() == raw.len() - 1 && error.error_len() == Some(1)
    ));
    assert!(!observed.execution()?.lane_success);
    Ok(())
}

#[test]
fn kani_runner_rejects_cargo_alias_override_before_verification() -> Result<(), String> {
    for allow_empty in [false, true] {
        let mut fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
        fixture
            .child_environment
            .insert("CARGO_ALIAS_KANI".into(), "version".into());
        let observed = drive(&fixture, "success", &lane_args(20, allow_empty))?;
        assert!(!observed.output.status.success());
        assert_eq!(observed.run.process.state, ProcessState::NotStarted);
        assert!(!observed.execution()?.configuration_admitted);
        assert!(
            observed
                .execution()?
                .configuration_rejections
                .iter()
                .any(|reason| reason.contains("CARGO_ALIAS_KANI"))
        );
        assert!(
            observed.calls.iter().all(|call| call == "kani --version"),
            "configuration refusal must precede metadata, discovery and verification"
        );
        assert!(observed.run.unregistered_results.is_empty());
        assert!(observed.run.results.is_empty());
        assert!(!observed.execution()?.lane_success);
    }
    Ok(())
}

#[test]
fn kani_runner_refuses_partial_source_capture_without_borrowing_scopes() -> Result<(), String> {
    let mut fixture = fixture(&[("one.rs", "one", "a::one")], &successful_block("a::one"))?;
    // This sorts after existing runner inputs and exceeds the unchanged8MiB
    // per-file source cap. It is one bounded partial-read stimulus.
    let path = "xtask/src/zz_fixture_oversize.bin";
    fixture.write(path, &vec![b'x'; 9 * 1024 * 1024])?;
    fixture.source_paths.insert(path.into());
    let observed = drive(&fixture, "success", &lane_args(20, false))?;
    assert_eq!(observed.output.status.code(), Some(1));
    assert!(
        !observed.run.run_id.ends_with("/initial"),
        "partial source capture needs a final refusal; stderr: {}",
        String::from_utf8_lossy(&observed.output.stderr)
    );
    assert_eq!(observed.run.process.state, ProcessState::NotStarted);
    assert!(observed.run.source_closure_digest.is_none());
    let execution = fixture.retain_on_error(observed.execution())?;
    assert!(!execution.configuration_admitted && !execution.source_current);
    assert!(execution.configuration_rejections.iter().any(|reason| {
        reason == "source capture is incomplete; verification configuration is not established"
    }));
    assert!(execution.source_before.manifest.is_none());
    assert_eq!(execution.source_before.file_count, 0);
    assert!(
        execution
            .source_before
            .error
            .as_deref()
            .is_some_and(|error| error.contains("byte budget"))
    );
    assert!(execution.explicit_inputs.is_empty() && execution.package_inputs.is_empty());
    assert!(execution.declared_subjects.is_empty() && execution.native_subjects.is_empty());
    assert!(execution.selected_subjects.is_empty());
    assert!(observed.run.unregistered_results.is_empty() && observed.run.results.is_empty());
    assert!(!execution.lane_success);
    assert!(observed.calls.iter().all(|call| call == "kani --version"));
    fixture.retain_on_error(native::verify_execution_artifacts(
        &observed.run,
        &artifact_directory(&fixture, &observed)?,
    ))?;
    Ok(())
}
