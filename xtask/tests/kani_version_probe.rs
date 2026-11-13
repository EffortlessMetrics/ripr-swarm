//! Actual availability-probe controls with synthetic provider observations.
//! Automation execs the complete xtask CLI. The narrow formal_runner target
//! execs the maintained formal-kani binary. Both use the independent session
//! watchdog; neither bounds the product with its own capture/deadline helper.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use xtask::formal_evidence::{
    ProcessState,
    native::{verify_artifact, verify_execution_artifacts},
};

#[cfg(feature = "automation")]
#[path = "common/native_fixture.rs"]
mod native_fixture;
#[cfg(not(feature = "automation"))]
use crate::native_fixture;
use native_fixture::{Fixture, Observation, child_selector, lane_args};

fn probe_fixture(has_harness: bool) -> Result<Fixture, String> {
    let mut fixture = Fixture::new("probe")?;
    if has_harness {
        fixture.add_harness("ripr", "proof.rs", "proof", "m::proof")?;
        fixture.write(
            "ripr-output",
            native_fixture::successful_block("m::proof").as_bytes(),
        )?;
    }
    Ok(fixture)
}

fn drive(
    fixture: &Fixture,
    mode: &str,
    allow_empty: bool,
    seconds: u64,
    watchdog: Duration,
) -> Result<Observation, String> {
    fixture.drive(
        child_selector(concat!(module_path!(), "::kani_probe_watchdog_child"))?,
        mode,
        "success",
        &lane_args(seconds, allow_empty),
        watchdog,
    )
}

fn observe(fixture: &Fixture, mode: &str, allow_empty: bool) -> Result<Observation, String> {
    let observed = drive(
        fixture,
        mode,
        allow_empty,
        if mode == "timeout" { 4 } else { 20 },
        Duration::from_secs(8),
    )?;
    fixture.retain_on_error(observed.require_driver_completion())?;
    assert_eq!(
        String::from_utf8_lossy(&observed.output.stdout).trim_end(),
        observed.report.trim_end(),
        "actual entry stdout and canonical Markdown must agree"
    );
    Ok(observed)
}

fn no_verification(observed: &Observation) -> Result<(), String> {
    assert_eq!(observed.run.process.state, ProcessState::NotStarted);
    assert!(observed.run.unregistered_results.is_empty());
    assert!(observed.run.results.is_empty());
    assert!(!observed.execution()?.lane_success);
    assert!(
        !observed
            .execution()?
            .invocations
            .iter()
            .any(|i| i.phase == "verification")
    );
    assert!(!observed.report.contains("| VERIFIED |"));
    Ok(())
}

fn copy_artifacts(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_artifacts(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        } else {
            return Err("fixture artifact copy encountered a non-regular entry".into());
        }
    }
    Ok(())
}

fn reject_rehashed_startup_deadline(
    fixture: &Fixture,
    observed: &Observation,
) -> Result<(), String> {
    let reports = fixture.root.join("target/ripr/reports");
    let directory = reports.join(&observed.execution()?.artifact_directory);
    let original = fs::read(directory.join("kani.json")).map_err(|error| error.to_string())?;
    let canonical = fs::read(reports.join("kani.json")).map_err(|error| error.to_string())?;
    if let Some(archive) = std::env::var_os("RIPR_NATIVE_ARTIFACT_SAMPLES") {
        let archive = PathBuf::from(archive);
        fs::create_dir_all(&archive).map_err(|error| error.to_string())?;
        copy_artifacts(
            &directory,
            &archive.join(fixture.root.file_name().ok_or("fixture name absent")?),
        )?;
    }
    let variant_directory = fixture.root.join("replay-before-deadline");
    copy_artifacts(&directory, &variant_directory)?;
    let mut variant = observed.run.clone();
    assert_eq!(variant.process.state, ProcessState::NotStarted);
    assert!(variant.process.raw_output_digest.is_none());
    let execution = variant.execution.as_mut().ok_or("execution absent")?;
    execution
        .invocations
        .iter_mut()
        .find(|invocation| invocation.phase == "before" && invocation.program == "git")
        .ok_or("before Git invocation absent")?
        .timeout_ms = 1001;
    fs::write(
        variant_directory.join("raw-index.json"),
        serde_json::to_vec_pretty(&execution.invocations).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let variant_path = variant_directory.join("kani.json");
    fs::write(
        &variant_path,
        serde_json::to_vec_pretty(&variant).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let refused = fixture.drive(
        child_selector(concat!(module_path!(), "::kani_probe_watchdog_child"))?,
        "normal",
        "success",
        &[
            "--replay".into(),
            variant_path
                .to_str()
                .ok_or("variant path is not UTF-8")?
                .into(),
        ],
        Duration::from_secs(8),
    )?;
    refused.require_driver_completion()?;
    assert!(!refused.output.status.success());
    assert!(
        refused.calls.is_empty(),
        "passive replay must not run cargo"
    );
    assert!(
        String::from_utf8_lossy(&refused.output.stderr)
            .contains("source snapshot command or deadline is outside the admitted profile")
    );
    assert_eq!(
        fs::read(directory.join("kani.json")).map_err(|e| e.to_string())?,
        original
    );
    assert_eq!(
        fs::read(reports.join("kani.json")).map_err(|e| e.to_string())?,
        canonical
    );
    Ok(())
}

#[test]
fn kani_probe_watchdog_child() -> Result<(), String> {
    native_fixture::watchdog_child()
}

#[test]
fn kani_availability_probe_deadline_prevents_a_verification_launch() -> Result<(), String> {
    for allow_empty in [false, true] {
        let fixture = probe_fixture(true)?;
        let observed = observe(&fixture, "timeout", allow_empty)?;
        assert_eq!(observed.owned_descendants_observed, 2);
        assert_eq!(observed.live_descendants_before_driver_cleanup, 0);
        assert_eq!(observed.output.status.code(), Some(1));
        assert_eq!(observed.calls, ["kani --version"]);
        let preflight = observed
            .execution()?
            .invocations
            .iter()
            .find(|i| i.phase == "preflight")
            .ok_or("preflight observation absent")?;
        assert!(preflight.timed_out);
        // Startup has its own refusal control below. This experiment must
        // actually admit a hung version probe, not spend its budget before it.
        assert!(preflight.timeout_ms >= 500 && preflight.timeout_ms <= 4000);
        assert!(preflight.capture_complete);
        assert!(preflight.stdout.is_some() && preflight.stderr.is_some());
        let duration = preflight.duration_ms.ok_or("probe duration absent")?;
        assert!(duration >= preflight.timeout_ms);
        assert!(duration <= preflight.timeout_ms + 1000);
        assert!(
            preflight
                .error
                .as_deref()
                .is_some_and(|s| s.contains("timed out"))
        );
        no_verification(&observed)?;
    }
    Ok(())
}

#[test]
fn kani_source_startup_exhaustion_prevents_a_version_launch() -> Result<(), String> {
    for allow_empty in [false, true] {
        let mut fixture = probe_fixture(true)?;
        fixture
            .child_environment
            .insert("RIPR_NATIVE_EXHAUST_STARTUP".into(), "1".into());
        let observed = drive(&fixture, "timeout", allow_empty, 1, Duration::from_secs(8))?;
        observed.require_driver_completion()?;
        assert!(fixture.root.join("startup-delay").is_file());
        assert!(observed.calls.is_empty());
        assert_eq!(observed.owned_descendants_observed, 0);
        assert_eq!(observed.output.status.code(), Some(1));
        let execution = fixture.retain_on_error(observed.execution())?;
        assert!(!observed.run.run_id.ends_with("/initial"));
        assert!(!execution.configuration_admitted && !execution.source_current);
        assert!(execution.configuration_rejections.iter().any(|reason| {
            reason == "source capture is incomplete; verification configuration is not established"
        }));
        assert!(execution.source_before.manifest.is_none());
        assert_eq!(execution.source_before.file_count, 0);
        assert!(execution.source_after.manifest.is_some());
        assert!(execution.source_after.error.is_none());
        assert!(execution.explicit_inputs.is_empty() && execution.package_inputs.is_empty());
        assert!(execution.declared_subjects.is_empty() && execution.selected_subjects.is_empty());
        let source_inventory = fixture.retain_on_error(
            execution
                .invocations
                .iter()
                .find(|i| {
                    i.phase == "before"
                        && i.program == "git"
                        && i.arguments.first().is_some_and(|a| a == "ls-files")
                })
                .ok_or_else(|| {
                    format!(
                        "startup inventory invocation absent; actual entry stderr: {}",
                        String::from_utf8_lossy(&observed.output.stderr)
                    )
                }),
        )?;
        assert!(source_inventory.timed_out && source_inventory.capture_complete);
        assert!(source_inventory.timeout_ms > 0 && source_inventory.timeout_ms <= 1000);
        let preflight = fixture.retain_on_error(
            execution
                .invocations
                .iter()
                .find(|i| i.phase == "preflight")
                .ok_or_else(|| "preflight refusal absent".to_string()),
        )?;
        assert_eq!(
            preflight.error.as_deref(),
            Some("whole-run deadline exhausted before version preflight")
        );
        assert!(!preflight.timed_out && !preflight.capture_complete);
        assert!(preflight.stdout.is_none() && preflight.stderr.is_none());
        no_verification(&observed)?;
        fixture.retain_on_error(verify_execution_artifacts(
            &observed.run,
            &fixture
                .root
                .join("target/ripr/reports")
                .join(&execution.artifact_directory),
        ))?;
        fixture.retain_on_error(reject_rehashed_startup_deadline(&fixture, &observed))?;
    }
    Ok(())
}

#[test]
fn kani_availability_probe_refuses_nonzero_signal_and_missing_tool() -> Result<(), String> {
    let cases = [
        ("nonzero", "cargo kani --version failed with exit status: 7"),
        ("signal", "cargo kani --version failed with signal: 15"),
        (
            "missing",
            "cargo could not be launched: executable or interpreter not found",
        ),
        (
            "permission",
            "cargo could not be launched: permission denied",
        ),
        (
            "missing-kani",
            "cargo is installed but the kani subcommand is unavailable",
        ),
        (
            "misleading-missing-kani",
            "cargo kani --version failed with exit status: 101",
        ),
    ];
    for (mode, reason) in cases {
        for allow_empty in [false, true] {
            let fixture = probe_fixture(true)?;
            let observed = observe(&fixture, mode, allow_empty)?;
            assert_eq!(
                observed.output.status.code(),
                Some(1),
                "{mode}: {}",
                observed.report
            );
            assert_eq!(
                observed.calls,
                if mode == "missing" || mode == "permission" {
                    vec![]
                } else {
                    vec!["kani --version"]
                },
                "{mode}: verification must not run after failed preflight"
            );
            assert!(
                observed.report.contains(reason),
                "{mode}: {}",
                observed.report
            );
            let preflight = observed
                .execution()?
                .invocations
                .iter()
                .find(|i| i.phase == "preflight")
                .ok_or("preflight observation absent")?;
            assert!(
                preflight
                    .error
                    .as_deref()
                    .is_some_and(|error| error.starts_with(reason)),
                "{mode}: {:?}",
                preflight.error
            );
            assert!(
                !observed.report.contains("injected")
                    && !observed.report.contains("untrusted version"),
                "version output is retained as bytes, not promoted into report claims"
            );
            no_verification(&observed)?;
        }
    }
    Ok(())
}

#[test]
fn kani_availability_probe_preserves_success_and_the_empty_opt_out() -> Result<(), String> {
    let mut fixture = probe_fixture(true)?;
    fixture.add_harness(
        "ripr-contract",
        "contract_proof.rs",
        "contract_proof",
        "m::contract_proof",
    )?;
    fixture.write(
        "ripr-contract-output",
        native_fixture::successful_block("m::contract_proof").as_bytes(),
    )?;
    let observed = observe(&fixture, "normal", false)?;
    assert_eq!(
        observed.output.status.code(),
        Some(0),
        "{}",
        observed.report
    );
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
        observed
            .calls
            .iter()
            .filter(|s| s.ends_with("--harness-timeout 1s --verbose"))
            .count(),
        2
    );
    assert_eq!(observed.calls.len(), 6);
    assert_eq!(observed.run.unregistered_results.len(), 2);
    assert!(observed.execution()?.lane_success);
    assert!(
        observed.run.results.is_empty(),
        "synthetic observations establish no registered claim"
    );
    let preflight = observed
        .execution()?
        .invocations
        .iter()
        .find(|i| i.phase == "preflight")
        .ok_or("preflight observation absent")?;
    let raw = verify_artifact(
        &fixture
            .root
            .join("target/ripr/reports")
            .join(&observed.execution()?.artifact_directory),
        preflight.stdout.as_ref().ok_or("preflight bytes absent")?,
        1024 * 1024,
    )?;
    assert_eq!(raw, native_fixture::expected_headers().as_bytes());

    for allow_empty in [false, true] {
        let fixture = probe_fixture(false)?;
        let observed = observe(&fixture, "normal", allow_empty)?;
        assert_eq!(observed.output.status.success(), allow_empty);
        assert_eq!(
            observed
                .calls
                .iter()
                .filter(|s| s.ends_with("list --format json"))
                .count(),
            2
        );
        assert_eq!(observed.calls.len(), 4);
        assert_eq!(
            observed.execution()?.inventory_rejections,
            ["zero selected native harnesses"]
        );
        no_verification(&observed)?;
    }
    Ok(())
}

#[test]
fn kani_probe_watchdog_stops_a_hung_probe_and_pipe_inheriting_descendant() -> Result<(), String> {
    let fixture = probe_fixture(true)?;
    // The product can give preflight up to ten seconds. This independent
    // two-second driver must stop its primary and inherited-pipe descendant.
    let observed = drive(&fixture, "timeout", false, 600, Duration::from_secs(2))?;
    assert!(observed.watchdog_timed_out);
    assert_eq!(observed.owned_descendants_observed, 2);
    assert_eq!(observed.live_descendants_before_driver_cleanup, 2);
    assert!(
        observed.elapsed < Duration::from_secs(7),
        "{:?}",
        observed.elapsed
    );
    assert!(!observed.output.status.success());
    assert_eq!(observed.calls, ["kani --version"]);
    assert!(
        observed
            .execution()?
            .artifact_directory
            .ends_with("/initial")
    );
    assert!(observed.report.contains("NOT-ESTABLISHED"));
    no_verification(&observed)?;
    Ok(())
}

#[test]
fn kani_empty_inventory_cannot_override_unavailable_tooling() -> Result<(), String> {
    for allow_empty in [false, true] {
        let fixture = probe_fixture(false)?;
        let observed = observe(&fixture, "missing", allow_empty)?;
        assert_eq!(observed.output.status.code(), Some(1));
        assert!(observed.calls.is_empty());
        assert!(
            observed
                .report
                .contains("cargo could not be launched: executable or interpreter not found")
        );
        no_verification(&observed)?;
    }
    Ok(())
}
