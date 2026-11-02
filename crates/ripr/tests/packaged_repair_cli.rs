//! Qualify the canonical `RepairAttempt` through an isolated installed CLI (#4650).
//!
//! Reuses the bounded boundary-gap fixture helpers (#3165) and emits the
//! #4648 channel-receipt shape without claiming VSIX parity.

#![expect(
    dead_code,
    reason = "policy:4650-packaged-repair-cli: shared boundary_gap helper surface; this harness uses a subset"
)]

#[path = "common/boundary_gap.rs"]
mod boundary_gap;
#[path = "common/mod.rs"]
mod common;

use boundary_gap::{
    AFTER_TESTS, BEFORE_TESTS, EQUALITY_SELECTOR, InstalledCandidate, JourneyRoots, boundary_seam,
    commit_all, decoy_was_invoked, install_candidate, isolated_path, journey_roots,
    missing_equality, parse_json, path_arg, plant_path_decoy, require_success,
    run_focused_cargo_test, run_installed, run_outcome, run_printed_ripr, run_repo_exposure_text,
    sha256_hex, split_printed_ripr_command, stdout_text, two_commit_before_repo, write_crate,
};
use serde_json::{Value, json};
use std::path::Path;

fn seam_id(seam: &Value) -> Result<&str, String> {
    seam.get("seam_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "boundary seam is missing seam_id".to_string())
}

fn printed_continue_command(stderr: &str) -> Result<String, String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("ripr: attempt next command: "))
        .map(str::to_string)
        .ok_or_else(|| format!("before phase printed no attempt command:\n{stderr}"))
}

fn attempt_id_from_continue(command: &str) -> Result<String, String> {
    let tokens = split_printed_ripr_command(command)?;
    tokens
        .iter()
        .position(|token| token == "--attempt")
        .and_then(|index| tokens.get(index + 1))
        .cloned()
        .ok_or_else(|| format!("no --attempt in `{command}`"))
}

fn agent_status(
    candidate: &InstalledCandidate,
    roots: &JourneyRoots,
    attempt_id: &str,
) -> Result<Value, String> {
    let output = run_installed(
        candidate,
        &roots.fixture,
        &roots.decoy,
        &[
            "agent",
            "status",
            "--root",
            &path_arg(&roots.fixture),
            "--attempt",
            attempt_id,
            "--json",
        ],
    )?;
    require_success("agent status --attempt", &output)?;
    parse_json("agent status", &stdout_text("agent status", &output)?)
}

fn build_channel_receipt(
    candidate: &InstalledCandidate,
    roots: &JourneyRoots,
    qualification_state: &str,
    static_movement: &str,
    attempt_id: &str,
    status: &Value,
    terminal_receipt: &Value,
) -> Value {
    json!({
        "schema_version": "packaged_repair_channel_receipt.v0",
        "qualification_state": qualification_state,
        "channel": "installed_cli",
        "vsix_parity_claimed": false,
        "issues": ["4648", "4650"],
        "artifact": {
            "installed_binary": candidate.binary,
            "sha256": candidate.digest,
            "full_version": candidate.version,
            "workspace_binary_excluded": true,
        },
        "attempt": {
            "attempt_id": attempt_id,
            "state": status.pointer("/attempt/state").or_else(|| status.pointer("/repair_attempts/0/state")),
            "status_class": status.pointer("/attempt/status_class").or_else(|| status.pointer("/repair_attempts/0/status_class")),
        },
        "semantic": {
            "static_movement": static_movement,
            "verification_status": terminal_receipt.pointer("/verification/status"),
            "gap_movement": terminal_receipt
                .pointer("/provenance/movement")
                .or_else(|| terminal_receipt.pointer("/seam/change")),
        },
        "cleanup": {
            "fixture_root": roots.fixture,
            "install_root": roots.install,
            "decoy_root": roots.decoy,
            "cargo_target_root": roots.cargo_target,
        },
        "limitations": [
            "No VSIX/editor-host parity is claimed for this installed-cli qualification receipt.",
            "Synthetic fixture journey only; not a real-repository dogfood row.",
        ],
    })
}

fn assert_verified_focused_execution(run: &boundary_gap::FocusedExecution) -> Result<(), String> {
    if run.selected == 0 || run.executed == 0 || run.passed == 0 || run.failed != 0 {
        return Err(format!(
            "focused execution is not verified: selected={} executed={} passed={} failed={} exit={:?} command={:?}",
            run.selected, run.executed, run.passed, run.failed, run.exit_code, run.command
        ));
    }
    if run.exit_code != Some(0) {
        return Err(format!(
            "focused execution exited {:?}: command={:?}",
            run.exit_code, run.command
        ));
    }
    Ok(())
}

fn assert_channel_receipt_shape(
    receipt: &Value,
    candidate: &InstalledCandidate,
) -> Result<(), String> {
    if receipt["vsix_parity_claimed"].as_bool() != Some(false) {
        return Err("channel receipt must not claim VSIX parity".into());
    }
    let digest = receipt
        .pointer("/artifact/sha256")
        .and_then(Value::as_str)
        .ok_or("channel receipt missing artifact.sha256")?;
    if digest != candidate.digest {
        return Err(format!(
            "channel receipt digest must match installed candidate: {digest} != {}",
            candidate.digest
        ));
    }
    if !digest.starts_with("sha256:") || digest.len() != "sha256:".len() + 64 {
        return Err(format!(
            "channel receipt digest must use sha256: prefix: {digest}"
        ));
    }
    if receipt
        .pointer("/semantic/gap_movement")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err(format!(
            "qualified channel receipt must carry terminal gap_movement: {receipt}"
        ));
    }
    Ok(())
}

fn write_channel_receipt(root: &Path, receipt: &Value) -> Result<(), String> {
    let directory = root.join("target/ripr/reports");
    std::fs::create_dir_all(&directory).map_err(|error| format!("create reports dir: {error}"))?;
    let path = directory.join("packaged-repair-channel-receipt.json");
    let bytes = serde_json::to_vec_pretty(receipt).map_err(|error| error.to_string())?;
    std::fs::write(&path, bytes).map_err(|error| format!("write channel receipt: {error}"))
}

fn assert_closed_outcome(outcome: &Value) -> Result<(), String> {
    if outcome
        .pointer("/summary/gap_movement/closed")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Err(format!("outcome did not close the gap: {outcome}"));
    }
    Ok(())
}

#[test]
fn installed_binary_is_distinct_from_workspace_and_path_decoy() -> Result<(), String> {
    let roots = journey_roots("packaged-repair-identity")?;
    let marker = plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    let workspace_digest = sha256_hex(
        &std::fs::read(env!("CARGO_BIN_EXE_ripr"))
            .map_err(|error| format!("read workspace binary: {error}"))?,
    );
    if candidate.digest != workspace_digest {
        return Err("installed digest must match the packaged workspace build".into());
    }
    if candidate.binary == Path::new(env!("CARGO_BIN_EXE_ripr")) {
        return Err("installed candidate must not be CARGO_BIN_EXE_ripr".into());
    }
    write_crate(&roots.fixture, boundary_gap::LIB_CLOSED, BEFORE_TESTS)?;
    boundary_gap::init_git(&roots.fixture)?;
    commit_all(&roots.fixture, "identity fixture")?;
    let version = run_installed(&candidate, &roots.fixture, &roots.decoy, &["--version"])?;
    require_success("installed --version", &version)?;
    if stdout_text("installed --version", &version)?.trim() != candidate.version {
        return Err("installed --version disagreed with recorded version".into());
    }
    if decoy_was_invoked(&marker) {
        return Err("PATH decoy was invoked".into());
    }
    let path = isolated_path(&roots.decoy)?;
    if !path
        .to_string_lossy()
        .contains(&roots.decoy.display().to_string())
    {
        return Err("isolated PATH must lead with the decoy directory".into());
    }
    Ok(())
}

#[test]
fn canonical_positive_journey_completes_before_edit_after_with_closed_movement()
-> Result<(), String> {
    let roots = journey_roots("packaged-repair-positive")?;
    let marker = plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;

    let before_text = run_repo_exposure_text(&candidate, &roots.fixture, &roots.decoy)?;
    let before = parse_json("before repo exposure", &before_text)?;
    let before_seam = boundary_seam(&before)?;
    if !missing_equality(before_seam)? {
        return Err("before snapshot already lost the equality gap".into());
    }

    let repair_before = run_installed(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &[
            "agent",
            "repair",
            "--root",
            &path_arg(&roots.fixture),
            "--seam-id",
            seam_id(before_seam)?,
            "--phase",
            "before",
        ],
    )?;
    require_success("agent repair --phase before", &repair_before)?;
    let continue_command =
        printed_continue_command(&String::from_utf8_lossy(&repair_before.stderr))?;
    let attempt_id = attempt_id_from_continue(&continue_command)?;

    std::fs::write(roots.fixture.join("tests/pricing.rs"), AFTER_TESTS)
        .map_err(|error| format!("write equality test: {error}"))?;
    let execution = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, EQUALITY_SELECTOR)?;
    assert_verified_focused_execution(&execution)?;

    let after_phase =
        run_printed_ripr(&candidate, &roots.fixture, &roots.decoy, &continue_command)?;
    require_success("printed ripr continue", &after_phase)?;

    let before_path = roots
        .fixture
        .join("target/ripr/workflow/before.repo-exposure.json");
    let after_path = roots
        .fixture
        .join("target/ripr/workflow/after.repo-exposure.json");
    if !before_path.is_file() || !after_path.is_file() {
        return Err("printed repair did not retain workflow snapshots".into());
    }
    let outcome = run_outcome(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &before_path,
        &after_path,
    )?;
    assert_closed_outcome(&outcome)?;

    let receipt_path = roots.fixture.join("target/ripr/reports/agent-receipt.json");
    let terminal_receipt = parse_json(
        "terminal receipt",
        &std::fs::read_to_string(&receipt_path)
            .map_err(|error| format!("read terminal receipt: {error}"))?,
    )?;
    let status = agent_status(&candidate, &roots, &attempt_id)?;
    let channel = build_channel_receipt(
        &candidate,
        &roots,
        "qualified",
        "closed",
        &attempt_id,
        &status,
        &terminal_receipt,
    );
    write_channel_receipt(&roots.fixture, &channel)?;
    assert_channel_receipt_shape(&channel, &candidate)?;
    let receipt_path = roots
        .fixture
        .join("target/ripr/reports/packaged-repair-channel-receipt.json");
    let written = parse_json(
        "written channel receipt",
        &std::fs::read_to_string(&receipt_path)
            .map_err(|error| format!("read channel receipt: {error}"))?,
    )?;
    assert_channel_receipt_shape(&written, &candidate)?;
    if decoy_was_invoked(&marker) {
        return Err("PATH decoy was invoked during the positive journey".into());
    }
    candidate.verify_final_identity()?;
    Ok(())
}

#[test]
fn attempt_and_terminal_receipt_survive_fresh_installed_processes() -> Result<(), String> {
    let roots = journey_roots("packaged-repair-processes")?;
    plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;
    let before = parse_json(
        "before repo exposure",
        &run_repo_exposure_text(&candidate, &roots.fixture, &roots.decoy)?,
    )?;
    let repair_before = run_installed(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &[
            "agent",
            "repair",
            "--root",
            &path_arg(&roots.fixture),
            "--seam-id",
            seam_id(boundary_seam(&before)?)?,
            "--phase",
            "before",
        ],
    )?;
    require_success("process A repair before", &repair_before)?;
    let continue_command =
        printed_continue_command(&String::from_utf8_lossy(&repair_before.stderr))?;
    let attempt_id = attempt_id_from_continue(&continue_command)?;

    let resumed = agent_status(&candidate, &roots, &attempt_id)?;
    if resumed.pointer("/attempt/state").and_then(Value::as_str) != Some("awaiting_edit")
        && resumed
            .pointer("/repair_attempts/0/state")
            .and_then(Value::as_str)
            != Some("awaiting_edit")
    {
        return Err(format!(
            "fresh status did not resume awaiting_edit: {resumed}"
        ));
    }

    std::fs::write(roots.fixture.join("tests/pricing.rs"), AFTER_TESTS)
        .map_err(|error| format!("write equality test: {error}"))?;
    run_focused_cargo_test(&roots.fixture, &roots.cargo_target, EQUALITY_SELECTOR)?;

    require_success(
        "process C continue",
        &run_printed_ripr(&candidate, &roots.fixture, &roots.decoy, &continue_command)?,
    )?;

    let finished = agent_status(&candidate, &roots, &attempt_id)?;
    let issued = finished
        .pointer("/attempt/receipt/issued_for_attempt")
        .or_else(|| finished.pointer("/repair_attempts/0/receipt/issued_for_attempt"));
    if issued != Some(&Value::Bool(true)) {
        return Err(format!("terminal receipt was not retained: {finished}"));
    }
    Ok(())
}

#[test]
fn noop_edit_reports_unchanged_not_improved() -> Result<(), String> {
    let roots = journey_roots("packaged-repair-noop")?;
    plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;
    let before = parse_json(
        "before repo exposure",
        &run_repo_exposure_text(&candidate, &roots.fixture, &roots.decoy)?,
    )?;
    let repair_before = run_installed(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &[
            "agent",
            "repair",
            "--root",
            &path_arg(&roots.fixture),
            "--seam-id",
            seam_id(boundary_seam(&before)?)?,
            "--phase",
            "before",
        ],
    )?;
    require_success("noop repair before", &repair_before)?;
    let continue_command =
        printed_continue_command(&String::from_utf8_lossy(&repair_before.stderr))?;
    // Deliberately skip the equality edit; running an unrelated passing test must
    // not let the after phase claim movement.
    run_focused_cargo_test(
        &roots.fixture,
        &roots.cargo_target,
        "below_threshold_has_no_discount",
    )?;
    let after_phase =
        run_printed_ripr(&candidate, &roots.fixture, &roots.decoy, &continue_command)?;
    if after_phase.status.success() {
        return Err("no-op journey must not finish the repair attempt".into());
    }
    let stderr = String::from_utf8_lossy(&after_phase.stderr);
    if !stderr.contains("awaiting_edit")
        && !stderr.contains("nothing has changed since the before phase")
    {
        return Err(format!(
            "no-op after phase should refuse movement: {stderr}"
        ));
    }
    Ok(())
}

#[test]
fn wrong_root_before_phase_fails_closed() -> Result<(), String> {
    let roots = journey_roots("packaged-repair-wrong-root")?;
    plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;
    let before = parse_json(
        "before repo exposure",
        &run_repo_exposure_text(&candidate, &roots.fixture, &roots.decoy)?,
    )?;
    let output = run_installed(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &[
            "agent",
            "repair",
            "--root",
            "/tmp/ripr-packaged-repair-missing-root",
            "--seam-id",
            seam_id(boundary_seam(&before)?)?,
            "--phase",
            "before",
        ],
    )?;
    if output.status.success() {
        return Err("wrong root must not start a repair attempt".into());
    }
    Ok(())
}
