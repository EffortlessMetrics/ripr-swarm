//! End-to-end product guard for the canonical predicate-boundary repair.
//!
//! The historical checked calibration artifacts predate the producer fix that
//! recognizes equal concrete arguments for parameter-to-parameter boundaries.
//! `equality_boundary_repair_closes_across_repo_exposure_and_outcome` remains
//! the ordinary built-binary repo-exposure/outcome regression (#3167). The
//! tests below close the remaining #3165 installed-boundary transaction:
//! installed-artifact identity, diff/repo witness parity, literal printed
//! command composition, harness-owned focused-test execution, and
//! receipt/status agreement. Caller-supplied `--test`/`--command` metadata
//! cannot substitute for that execution receipt.

#[path = "boundary_gap_closure/boolean_owner_pin.rs"]
mod boolean_owner_pin;
#[path = "common/boundary_gap.rs"]
mod boundary_gap;
#[path = "common/mod.rs"]
mod common;

#[path = "boundary_gap_closure/ordinary_daily_loop.rs"]
mod ordinary_daily_loop;

use boundary_gap::{
    AFTER_TESTS, BEFORE_TESTS, EQUALITY_DISCRIMINATOR, EQUALITY_SELECTOR, FocusedExecution,
    SIBLING_TESTS, WitnessFacts, assert_shared_witness, boundary_seam, commit_all,
    decoy_was_invoked, diff_witness, install_candidate, isolated_path, journey_roots,
    missing_equality, path_arg, plant_path_decoy, related_equality_test, repo_witness,
    require_success, run_diff_check, run_focused_cargo_test, run_installed, run_outcome,
    run_printed_ripr, run_repo_exposure, run_repo_exposure_text, run_workspace_ripr, sha256_hex,
    split_printed_ripr_command, stdout_text, two_commit_before_repo, write_crate,
};
use serde_json::Value;
use std::path::Path;

fn seam_id(seam: &Value) -> Result<&str, String> {
    seam.get("seam_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "boundary seam is missing seam_id".to_string())
}

fn printed_next_command(stderr: &str) -> Result<&str, String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("ripr: attempt next command: "))
        .ok_or_else(|| format!("before phase printed no attempt command:\n{stderr}"))
}

fn verification_status(receipt: &Value) -> Result<&str, String> {
    receipt
        .pointer("/verification/status")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("receipt is missing verification.status: {receipt}"))
}

fn assert_closed_outcome(outcome: &Value) -> Result<(), String> {
    if outcome.pointer("/summary/moved").and_then(Value::as_u64) != Some(1)
        || outcome
            .pointer("/summary/unchanged")
            .and_then(Value::as_u64)
            != Some(0)
        || outcome
            .pointer("/summary/gap_movement/closed")
            .and_then(Value::as_u64)
            != Some(1)
        || outcome
            .pointer("/moved/0/gap_movement")
            .and_then(Value::as_str)
            != Some("closed")
        || outcome.pointer("/moved/0/before").and_then(Value::as_str) != Some("weakly_gripped")
        || outcome.pointer("/moved/0/after").and_then(Value::as_str) != Some("strongly_gripped")
    {
        return Err(format!(
            "outcome did not report a single closed grip: {outcome}"
        ));
    }
    let resolved = outcome
        .pointer("/moved/0/missing_discriminators_resolved")
        .and_then(Value::as_array)
        .ok_or_else(|| "outcome is missing missing_discriminators_resolved".to_string())?;
    if !resolved.iter().any(|item| {
        item.as_str()
            .is_some_and(|value| value.contains(EQUALITY_DISCRIMINATOR))
    }) {
        return Err(format!(
            "outcome did not resolve the equality discriminator: {outcome}"
        ));
    }
    Ok(())
}

fn assert_nonzero_intended_execution(run: &FocusedExecution) -> Result<(), String> {
    if run.selector != EQUALITY_SELECTOR {
        return Err(format!("harness ran the wrong selector: {}", run.selector));
    }
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

#[test]
fn equality_boundary_repair_closes_across_repo_exposure_and_outcome() -> Result<(), String> {
    let roots = journey_roots("boundary-gap-closure")?;
    write_crate(&roots.fixture, boundary_gap::LIB_CLOSED, BEFORE_TESTS)?;
    boundary_gap::init_git(&roots.fixture)?;
    commit_all(&roots.fixture, "initial boundary fixture")?;

    let before_output = run_workspace_ripr(
        &roots.fixture,
        &[
            "check",
            "--root",
            &path_arg(&roots.fixture),
            "--mode",
            "ready",
            "--format",
            "repo-exposure-json",
        ],
    )?;
    require_success("workspace repo exposure before", &before_output)?;
    let before_text = stdout_text("before repo exposure", &before_output)?;
    let before = boundary_gap::parse_json("before repo exposure", &before_text)?;
    let before_seam = boundary_seam(&before)?;
    assert_eq!(
        before_seam.get("grip_class").and_then(Value::as_str),
        Some("weakly_gripped")
    );
    assert_eq!(
        before_seam
            .get("headline_eligible")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(missing_equality(before_seam)?);

    let before_path = roots.fixture.join("before.repo-exposure.json");
    std::fs::write(&before_path, &before_text)
        .map_err(|error| format!("write before snapshot failed: {error}"))?;

    std::fs::write(roots.fixture.join("tests/pricing.rs"), AFTER_TESTS)
        .map_err(|error| format!("write equality boundary test failed: {error}"))?;

    let after_output = run_workspace_ripr(
        &roots.fixture,
        &[
            "check",
            "--root",
            &path_arg(&roots.fixture),
            "--mode",
            "ready",
            "--format",
            "repo-exposure-json",
        ],
    )?;
    require_success("workspace repo exposure after", &after_output)?;
    let after_text = stdout_text("after repo exposure", &after_output)?;
    let after = boundary_gap::parse_json("after repo exposure", &after_text)?;
    let after_seam = boundary_seam(&after)?;
    assert_eq!(
        after_seam.get("grip_class").and_then(Value::as_str),
        Some("strongly_gripped")
    );
    assert_eq!(
        after_seam.get("headline_eligible").and_then(Value::as_bool),
        Some(false)
    );
    assert!(!missing_equality(after_seam)?);
    assert!(related_equality_test(after_seam));

    let after_path = roots.fixture.join("after.repo-exposure.json");
    std::fs::write(&after_path, &after_text)
        .map_err(|error| format!("write after snapshot failed: {error}"))?;

    let outcome_output = run_workspace_ripr(
        &roots.fixture,
        &[
            "outcome",
            "--before",
            &path_arg(&before_path),
            "--after",
            &path_arg(&after_path),
            "--format",
            "json",
        ],
    )?;
    require_success("workspace outcome", &outcome_output)?;
    let outcome = boundary_gap::parse_json(
        "workspace outcome",
        &stdout_text("workspace outcome", &outcome_output)?,
    )?;
    assert_closed_outcome(&outcome)?;
    Ok(())
}

#[test]
fn installed_candidate_is_not_workspace_or_path_substitution() -> Result<(), String> {
    let roots = journey_roots("installed-identity")?;
    let marker = plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    let source_digest = sha256_hex(
        &std::fs::read(env!("CARGO_BIN_EXE_ripr"))
            .map_err(|error| format!("read workspace binary: {error}"))?,
    );
    if candidate.digest != source_digest {
        return Err(format!(
            "installed digest {} != workspace digest {source_digest}",
            candidate.digest
        ));
    }
    let workspace_bin = Path::new(env!("CARGO_BIN_EXE_ripr"));
    if candidate.binary == workspace_bin {
        return Err("installed candidate is still CARGO_BIN_EXE_ripr".to_string());
    }
    if workspace_bin
        .parent()
        .is_some_and(|bin_dir| candidate.binary.starts_with(bin_dir))
    {
        return Err(format!(
            "installed binary still lives in the cargo bin dir: {}",
            candidate.binary.display()
        ));
    }
    let path = isolated_path(&roots.decoy)?;
    if !path
        .to_string_lossy()
        .contains(&roots.decoy.display().to_string())
    {
        return Err("isolated PATH does not lead with the decoy directory".to_string());
    }
    write_crate(&roots.fixture, boundary_gap::LIB_CLOSED, BEFORE_TESTS)?;
    boundary_gap::init_git(&roots.fixture)?;
    commit_all(&roots.fixture, "identity fixture")?;
    let version = run_installed(&candidate, &roots.fixture, &roots.decoy, &["--version"])?;
    require_success("installed --version", &version)?;
    let printed = stdout_text("installed --version", &version)?;
    if printed.trim() != candidate.version {
        return Err(format!(
            "installed --version {} != recorded {}",
            printed.trim(),
            candidate.version
        ));
    }
    if decoy_was_invoked(&marker) {
        return Err(
            "PATH decoy was invoked; the harness used name lookup instead of the installed path"
                .to_string(),
        );
    }
    Ok(())
}

#[test]
fn diff_and_repo_share_equality_witness_facts_before_and_after() -> Result<(), String> {
    let roots = journey_roots("diff-repo-parity")?;
    plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    let production_base = two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;

    let before_repo = run_repo_exposure(&candidate, &roots.fixture, &roots.decoy)?;
    let before_diff = run_diff_check(&candidate, &roots.fixture, &roots.decoy, &production_base)?;
    let before_repo_witness = repo_witness(&before_repo)?;
    let before_diff_witness = diff_witness(&before_diff)?;
    if !before_repo_witness.missing_equality || before_repo_witness.related_equality_test {
        return Err(format!(
            "before repo witness should still miss equality: {before_repo_witness:?}"
        ));
    }
    if before_repo_witness.public_class != "weakly_gripped" {
        return Err(format!(
            "before repo class should be weakly_gripped: {before_repo_witness:?}"
        ));
    }
    assert_shared_witness(&before_diff_witness, &before_repo_witness)?;

    std::fs::write(roots.fixture.join("tests/pricing.rs"), AFTER_TESTS)
        .map_err(|error| format!("write equality test failed: {error}"))?;

    let after_repo = run_repo_exposure(&candidate, &roots.fixture, &roots.decoy)?;
    let after_diff = run_diff_check(&candidate, &roots.fixture, &roots.decoy, &production_base)?;
    let after_repo_witness = repo_witness(&after_repo)?;
    let after_diff_witness = diff_witness(&after_diff)?;
    if after_repo_witness.missing_equality || !after_repo_witness.related_equality_test {
        return Err(format!(
            "after repo witness should observe equality: {after_repo_witness:?}"
        ));
    }
    if after_repo_witness.public_class != "strongly_gripped" {
        return Err(format!(
            "after repo class should be strongly_gripped: {after_repo_witness:?}"
        ));
    }
    assert_shared_witness(&after_diff_witness, &after_repo_witness)?;
    if after_repo_witness.expression != before_repo_witness.expression
        || path_file(&after_repo_witness) != path_file(&before_repo_witness)
    {
        return Err(format!(
            "repair changed the production owner: before {before_repo_witness:?} after {after_repo_witness:?}"
        ));
    }
    Ok(())
}

fn path_file(witness: &WitnessFacts) -> String {
    witness
        .file
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(witness.file.as_str())
        .to_string()
}

#[test]
fn sibling_assertion_does_not_fabricate_repo_closure() -> Result<(), String> {
    let roots = journey_roots("sibling-negative")?;
    plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;
    std::fs::write(roots.fixture.join("tests/pricing.rs"), SIBLING_TESTS)
        .map_err(|error| format!("write sibling test failed: {error}"))?;
    let after = run_repo_exposure(&candidate, &roots.fixture, &roots.decoy)?;
    let witness = repo_witness(&after)?;
    if !witness.missing_equality || witness.public_class == "strongly_gripped" {
        return Err(format!(
            "sibling far-above assertion fabricated equality closure: {witness:?}"
        ));
    }
    Ok(())
}

#[test]
fn caller_supplied_receipt_metadata_is_not_an_execution_receipt() -> Result<(), String> {
    let roots = journey_roots("receipt-metadata")?;
    plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;
    let before_text = run_repo_exposure_text(&candidate, &roots.fixture, &roots.decoy)?;
    let before = boundary_gap::parse_json("before repo exposure", &before_text)?;
    if !missing_equality(boundary_seam(&before)?)? {
        return Err("before snapshot already lost the equality gap".to_string());
    }
    let before_path = roots.fixture.join("before.repo-exposure.json");
    std::fs::write(&before_path, &before_text)
        .map_err(|error| format!("write before snapshot failed: {error}"))?;
    std::fs::write(roots.fixture.join("tests/pricing.rs"), AFTER_TESTS)
        .map_err(|error| format!("write equality test failed: {error}"))?;
    common::fixture_git::fixture_git_ok(&roots.fixture, &["add", "tests/pricing.rs"])?;
    commit_all(&roots.fixture, "add equality test")?;
    let after_text = run_repo_exposure_text(&candidate, &roots.fixture, &roots.decoy)?;
    let after = boundary_gap::parse_json("after repo exposure", &after_text)?;
    let after_path = roots.fixture.join("after.repo-exposure.json");
    std::fs::write(&after_path, &after_text)
        .map_err(|error| format!("write after snapshot failed: {error}"))?;

    let verify = run_installed(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &[
            "agent",
            "verify",
            "--root",
            &path_arg(&roots.fixture),
            "--before",
            &path_arg(&before_path),
            "--after",
            &path_arg(&after_path),
            "--json",
        ],
    )?;
    require_success("agent verify", &verify)?;
    let verify_path = roots.fixture.join("agent-verify.json");
    std::fs::write(&verify_path, &verify.stdout)
        .map_err(|error| format!("write verify JSON failed: {error}"))?;
    let receipt_path = roots.fixture.join("agent-receipt.json");
    let receipt = run_installed(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &[
            "agent",
            "receipt",
            "--root",
            &path_arg(&roots.fixture),
            "--verify-json",
            &path_arg(&verify_path),
            "--seam-id",
            seam_id(boundary_seam(&after)?)?,
            "--test",
            EQUALITY_SELECTOR,
            "--command",
            "cargo test pricing equality_boundary_discounts -- --exact",
            "--json",
            "--out",
            &path_arg(&receipt_path),
        ],
    )?;
    require_success("agent receipt", &receipt)?;
    let receipt_value = boundary_gap::parse_json(
        "agent receipt",
        &std::fs::read_to_string(&receipt_path)
            .map_err(|error| format!("read receipt: {error}"))?,
    )?;
    if verification_status(&receipt_value)? != "verification_not_run" {
        return Err(format!(
            "--test/--command metadata must not become an execution receipt: {receipt_value}"
        ));
    }
    if receipt_value
        .pointer("/verification/commands_run")
        .and_then(Value::as_array)
        .is_some_and(|commands| commands.is_empty())
    {
        return Err("caller-supplied --command should remain metadata, not vanish".to_string());
    }
    if receipt_value
        .pointer("/verification/executed")
        .and_then(Value::as_u64)
        == Some(1)
        || receipt_value
            .get("focused_test_execution")
            .and_then(Value::as_str)
            == Some("pass")
    {
        return Err(format!(
            "receipt fabricated focused-test execution from metadata: {receipt_value}"
        ));
    }
    Ok(())
}

#[test]
fn harness_executes_intended_test_and_printed_repair_agrees_with_status() -> Result<(), String> {
    let roots = journey_roots("installed-journey")?;
    let marker = plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;

    let before_text = run_repo_exposure_text(&candidate, &roots.fixture, &roots.decoy)?;
    let before = boundary_gap::parse_json("before repo exposure", &before_text)?;
    let before_seam = boundary_seam(&before)?;
    if !missing_equality(before_seam)? {
        return Err("before snapshot already lost the equality gap".to_string());
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
    let printed_after =
        printed_next_command(&String::from_utf8_lossy(&repair_before.stderr))?.to_string();
    let tokens = split_printed_ripr_command(&printed_after)?;
    if tokens.get(1).map(String::as_str) != Some("continue")
        || !tokens.iter().any(|token| token == "--root")
        || !tokens.iter().any(|token| token == "--attempt")
        || printed_after.contains("--phase after")
    {
        return Err(format!(
            "before phase must print `ripr continue --root … --attempt …`, not `--phase after`: {printed_after}"
        ));
    }

    std::fs::write(roots.fixture.join("tests/pricing.rs"), AFTER_TESTS)
        .map_err(|error| format!("write equality test failed: {error}"))?;

    let execution = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, EQUALITY_SELECTOR)?;
    assert_nonzero_intended_execution(&execution)?;

    let missing_selector =
        run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "no_such_equality_test")?;
    if missing_selector.selected != 0
        && missing_selector.executed != 0
        && missing_selector.passed != 0
    {
        return Err(format!(
            "missing selector must not count as verified: {missing_selector:?}"
        ));
    }

    let after_phase = run_printed_ripr(&candidate, &roots.fixture, &roots.decoy, &printed_after)?;
    require_success("printed ripr continue", &after_phase)?;

    let before_path = roots
        .fixture
        .join("target/ripr/workflow/before.repo-exposure.json");
    let after_path = roots
        .fixture
        .join("target/ripr/workflow/after.repo-exposure.json");
    if !before_path.is_file() || !after_path.is_file() {
        return Err(format!(
            "printed repair sequence did not write before/after snapshots at {} and {}",
            before_path.display(),
            after_path.display()
        ));
    }
    let after_text = std::fs::read_to_string(&after_path)
        .map_err(|error| format!("read after snapshot: {error}"))?;
    let after = boundary_gap::parse_json("after snapshot", &after_text)?;
    let after_witness = repo_witness(&after)?;
    if after_witness.missing_equality || after_witness.public_class != "strongly_gripped" {
        return Err(format!(
            "after analysis did not close the equality gap: {after_witness:?}"
        ));
    }
    let outcome = run_outcome(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &before_path,
        &after_path,
    )?;
    assert_closed_outcome(&outcome)?;

    let status = run_installed(
        &candidate,
        &roots.fixture,
        &roots.decoy,
        &[
            "agent",
            "status",
            "--root",
            &path_arg(&roots.fixture),
            "--json",
        ],
    )?;
    require_success("agent status", &status)?;
    let status_value =
        boundary_gap::parse_json("agent status", &stdout_text("agent status", &status)?)?;
    let receipt_path = roots.fixture.join("target/ripr/reports/agent-receipt.json");
    if !receipt_path.is_file() {
        return Err("after phase did not write target/ripr/reports/agent-receipt.json".to_string());
    }
    let receipt_value = boundary_gap::parse_json(
        "attempt receipt",
        &std::fs::read_to_string(&receipt_path)
            .map_err(|error| format!("read attempt receipt: {error}"))?,
    )?;
    if verification_status(&receipt_value)? == "verification_executed_pass" {
        return Err(format!(
            "repair receipt claimed RIPR executed the focused test: {receipt_value}"
        ));
    }
    let verification = status_value
        .pointer("/repair_attempts/0/receipt/verification/status")
        .or_else(|| status_value.pointer("/verification/status"))
        .and_then(Value::as_str);
    if verification == Some("verification_executed_pass") {
        return Err(format!(
            "agent status claimed RIPR executed the focused test: {status_value}"
        ));
    }
    if status_value.pointer("/repair_attempts/0/receipt/issued_for_attempt")
        != Some(&Value::Bool(true))
        && status_value
            .pointer("/repair_attempts/0/receipt/movement")
            .is_none()
    {
        return Err(format!(
            "agent status did not retain the attempt receipt: {status_value}"
        ));
    }
    if decoy_was_invoked(&marker) {
        return Err("PATH decoy was invoked during the installed journey".to_string());
    }
    // Keep the independent axes honest in the test report itself.
    if execution.passed == 0 {
        return Err("harness execution was lost after the repair".to_string());
    }
    Ok(())
}
#[test]
fn broad_boundary_assertion_cannot_borrow_another_tests_exact_oracle() -> Result<(), String> {
    let roots = journey_roots("boundary-mixed-oracle")?;
    write_crate(&roots.fixture, boundary_gap::LIB_OPEN, BEFORE_TESTS)?;
    boundary_gap::init_git(&roots.fixture)?;
    let base = commit_all(&roots.fixture, "boundary baseline")?;
    for (case, assertion, extra, discriminates) in [
        ("baseline", "", "", false),
        (
            "broad",
            "assert!(discounted_total(100, 100) > 0);",
            "",
            false,
        ),
        (
            "broad_with_proximity",
            "assert!(discounted_total(100, 100) > 0);",
            "#[test] fn discounted_total_noise() { assert_eq!(1, 1); }",
            false,
        ),
        (
            "masked_wrapper",
            "let _ = discounted_total(100, 100);\nassert_eq!(wrapper(100, 100) * 0, 0);",
            "use boundary_gap_closure::wrapper;",
            false,
        ),
        (
            "same_result_wrapper",
            "let _ = discounted_total(100, 100);\nlet got = wrapper(100, 100);\nassert_eq!(wrapper(100, 100), got);",
            "use boundary_gap_closure::wrapper;",
            false,
        ),
        (
            "exact_wrapper",
            "let _ = discounted_total(100, 100);\nassert_eq!(wrapper(100, 100), 90);",
            "use boundary_gap_closure::wrapper;",
            true,
        ),
        (
            "exact_wrapper_statement",
            "let _ = discounted_total(100, 100);\nassert_eq!(wrapper(100, 100), 90);",
            "use boundary_gap_closure::wrapper;",
            true,
        ),
        (
            "exact",
            "assert_eq!(discounted_total(100, 100), 90);",
            "",
            true,
        ),
        (
            "helper",
            "check_boundary();",
            "fn check_boundary() { assert_eq!(discounted_total(100, 100), 90); }",
            true,
        ),
    ] {
        // The exact far-side assertions and equality assertion belong to distinct
        // test functions: combining their evidence would manufacture an oracle.
        let tests = format!(
            "{BEFORE_TESTS}\n#[test]\nfn equality_boundary_discounts() {{ {assertion} }}\n{extra}\n"
        );
        std::fs::write(roots.fixture.join("tests/pricing.rs"), tests)
            .map_err(|error| error.to_string())?;
        let wrapper = if matches!(
            case,
            "masked_wrapper" | "same_result_wrapper" | "exact_wrapper" | "exact_wrapper_statement"
        ) {
            if case == "exact_wrapper_statement" {
                "\npub fn wrapper(a: i32, b: i32) -> i32 { let _unused = 3; discounted_total(a, b) }\n"
            } else {
                "\npub fn wrapper(a: i32, b: i32) -> i32 { discounted_total(a, b) }\n"
            }
        } else {
            ""
        };
        std::fs::write(
            roots.fixture.join("src/lib.rs"),
            format!("{}{wrapper}", boundary_gap::LIB_CLOSED),
        )
        .map_err(|error| error.to_string())?;
        let root = path_arg(&roots.fixture);
        let diff = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--base",
                &base,
                "--worktree",
                "--mode",
                "ready",
                "--format",
                "json",
            ],
        )?;
        require_success("diff check", &diff)?;
        let diff_json = boundary_gap::parse_json("diff check", &stdout_text("diff check", &diff)?)?;
        let finding = boundary_gap::boundary_finding(&diff_json)?;
        assert_eq!(
            finding.get("classification").and_then(Value::as_str) == Some("exposed"),
            discriminates
                && !matches!(case, "helper" | "exact_wrapper" | "exact_wrapper_statement"),
            "{case}: {finding}"
        );
        let repo = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &repo)?;
        let repo_json =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &repo)?)?;
        let seam = boundary_seam(&repo_json)?;
        assert_eq!(
            seam.get("grip_class").and_then(Value::as_str) == Some("strongly_gripped"),
            discriminates && case != "helper",
            "{case}: {seam}"
        );
        if matches!(case, "broad" | "broad_with_proximity") {
            assert_eq!(
                seam.pointer("/evidence/discriminate")
                    .and_then(Value::as_str),
                Some("weak"),
                "{case}: {seam}"
            );
            assert_eq!(
                seam.get("headline_eligible").and_then(Value::as_bool),
                Some(true),
                "{case}: {seam}"
            );
            assert_ne!(
                seam.pointer("/evidence_record/canonical_item/gap_state")
                    .and_then(Value::as_str),
                Some("already_observed"),
                "{case}: {seam}"
            );
        }
        for (label, lib, passes) in [
            ("correct", boundary_gap::LIB_CLOSED, true),
            ("mutant", boundary_gap::LIB_OPEN, !discriminates),
        ] {
            std::fs::write(roots.fixture.join("src/lib.rs"), format!("{lib}{wrapper}"))
                .map_err(|error| error.to_string())?;
            for selector in [
                "below_threshold_has_no_discount",
                "far_above_threshold_discounts",
                "equality_boundary_discounts",
            ] {
                let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, selector)?;
                let expected_pass = selector != "equality_boundary_discounts" || passes;
                assert_eq!(
                    (
                        run.selected,
                        run.executed,
                        run.passed,
                        run.failed,
                        run.ignored,
                        run.exit_code
                    ),
                    (
                        1,
                        1,
                        u64::from(expected_pass),
                        u64::from(!expected_pass),
                        0,
                        Some(if expected_pass { 0 } else { 101 })
                    ),
                    "{case}/{label}/{selector}: {run:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn wrapper_nonnumeric_oracles_kill_the_boundary_mutant() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-nonnumeric")?;
    let mut withheld = Vec::new();
    for (label, ty, high, low, expected) in [
        ("string", "&'static str", "\"high\"", "\"low\"", "\"high\""),
        (
            "enum",
            "Outcome",
            "Outcome::High",
            "Outcome::Low",
            "Outcome::High",
        ),
        (
            "constant",
            "Outcome",
            "Outcome::High",
            "Outcome::Low",
            "EXPECTED",
        ),
    ] {
        let library = |op: &str| {
            format!(
                "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub const EXPECTED: Outcome = Outcome::High;\npub fn discounted_total(amount: i32, discount_threshold: i32) -> {ty} {{ if amount {op} discount_threshold {{ {high} }} else {{ {low} }} }}\npub fn wrapper(a: i32, b: i32) -> {ty} {{ discounted_total(a,b) }}\n"
            )
        };
        let tests = format!(
            "use boundary_gap_closure::{{discounted_total, wrapper, Outcome, EXPECTED}};\n#[test]\nfn boundary() {{ let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), {expected}); }}\n"
        );
        write_crate(&roots.fixture, &library(">="), &tests)?;
        for (op, passes) in [(">=", true), (">", false)] {
            std::fs::write(roots.fixture.join("src/lib.rs"), library(op))
                .map_err(|e| e.to_string())?;
            let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
            assert_eq!(
                (run.executed, run.passed, run.failed),
                (1, u64::from(passes), u64::from(!passes)),
                "{label}/{op}: {run:?}"
            );
        }
        std::fs::write(roots.fixture.join("src/lib.rs"), library(">="))
            .map_err(|e| e.to_string())?;
        let root = path_arg(&roots.fixture);
        let report = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &report)?;
        let report =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let seam = boundary_seam(&report)?;
        if seam.get("grip_class").and_then(Value::as_str) != Some("strongly_gripped") {
            withheld.push(format!(
                "{label}: {}",
                seam.get("grip_class").unwrap_or(&Value::Null)
            ));
        }
    }
    assert!(
        withheld.is_empty(),
        "valid wrapper oracles withheld: {withheld:?}"
    );
    Ok(())
}

#[test]
fn wrapper_branch_mapping_observes_boundary_mutant() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-branch")?;
    let library = |op: &str| {
        format!(
            "pub fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub fn wrapper(a: i32, b: i32) -> i32 {{ if discounted_total(a,b) {{ 5 }} else {{ 0 }} }}\n"
        )
    };
    let tests = "use boundary_gap_closure::{discounted_total, wrapper};\n#[test]\nfn boundary() { let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), 5); }\n";
    write_crate(&roots.fixture, &library(">="), tests)?;
    for (op, passes) in [(">=", true), (">", false)] {
        std::fs::write(roots.fixture.join("src/lib.rs"), library(op)).map_err(|e| e.to_string())?;
        let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
        assert_eq!(
            (run.executed, run.passed, run.failed),
            (1, u64::from(passes), u64::from(!passes)),
            "{op}: {run:?}"
        );
    }
    std::fs::write(roots.fixture.join("src/lib.rs"), library(">=")).map_err(|e| e.to_string())?;
    let root = path_arg(&roots.fixture);
    let report = run_workspace_ripr(
        &roots.fixture,
        &[
            "check",
            "--root",
            &root,
            "--mode",
            "ready",
            "--format",
            "repo-exposure-json",
        ],
    )?;
    require_success("repo exposure", &report)?;
    let report =
        boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
    let seam = boundary_seam(&report)?;
    assert_eq!(
        seam.get("grip_class").and_then(Value::as_str),
        Some("strongly_gripped"),
        "{seam}"
    );
    Ok(())
}

#[test]
fn mapped_wrapper_wrong_and_self_derived_expectations_do_not_grip() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-branch-negative")?;
    let library = |op: &str| {
        format!(
            "pub const fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub const fn wrapper(a: i32, b: i32) -> i32 {{ if discounted_total(a,b) {{ 5 }} else {{ 0 }} }}\n"
        )
    };
    for (label, expected, declarations, expected_pass) in [
        ("wrong", "7", "", false),
        (
            "shadowed_variant",
            "Outcome::High",
            "struct Outcome; impl Outcome { const High: i32 = wrapper(100, 100); }",
            true,
        ),
    ] {
        let tests = format!(
            "use boundary_gap_closure::{{discounted_total, wrapper}};\n{declarations}\n#[test]\nfn boundary() {{ let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), {expected}); }}\n"
        );
        write_crate(&roots.fixture, &library(">="), &tests)?;
        for op in [">=", ">"] {
            std::fs::write(roots.fixture.join("src/lib.rs"), library(op))
                .map_err(|e| e.to_string())?;
            let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
            assert_eq!(
                (run.executed, run.passed, run.failed),
                (1, u64::from(expected_pass), u64::from(!expected_pass)),
                "{label}/{op}: {run:?}"
            );
        }
        std::fs::write(roots.fixture.join("src/lib.rs"), library(">="))
            .map_err(|e| e.to_string())?;
        let root = path_arg(&roots.fixture);
        let report = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &report)?;
        let report =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let seam = boundary_seam(&report)?;
        assert_ne!(
            seam.get("grip_class").and_then(Value::as_str),
            Some("strongly_gripped"),
            "{label}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn wrapper_enum_branch_observes_boundary_mutant() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-enum-branch")?;
    let library = |op: &str| {
        format!(
            "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub const EXPECTED: Outcome = Outcome::High;\npub fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub fn wrapper(a: i32, b: i32) -> Outcome {{ if discounted_total(a,b) {{ Outcome::High }} else {{ Outcome::Low }} }}\n"
        )
    };
    for (import, expected) in [
        ("Outcome", "Outcome::High"),
        ("Outcome as Alias", "Alias::High"),
        ("EXPECTED", "EXPECTED"),
    ] {
        let tests = format!(
            "use boundary_gap_closure::{{discounted_total, wrapper, {import}}};\n#[test]\nfn boundary() {{ let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), {expected}); }}\n"
        );
        write_crate(&roots.fixture, &library(">="), &tests)?;
        for (op, passes) in [(">=", true), (">", false)] {
            std::fs::write(roots.fixture.join("src/lib.rs"), library(op))
                .map_err(|e| e.to_string())?;
            let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
            assert_eq!(
                (run.executed, run.passed, run.failed),
                (1, u64::from(passes), u64::from(!passes)),
                "{op}: {run:?}"
            );
        }
        std::fs::write(roots.fixture.join("src/lib.rs"), library(">="))
            .map_err(|e| e.to_string())?;
        let root = path_arg(&roots.fixture);
        let report = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &report)?;
        let report =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let seam = boundary_seam(&report)?;
        assert_eq!(
            seam.get("grip_class").and_then(Value::as_str),
            Some("strongly_gripped"),
            "{import}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn enum_branch_local_crate_shadow_does_not_grip() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-enum-root-shadow")?;
    let library = |op: &str| {
        format!(
            "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub const fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub const fn wrapper(a: i32, b: i32) -> Outcome {{ if discounted_total(a,b) {{ Outcome::High }} else {{ Outcome::Low }} }}\n"
        )
    };
    let tests = "use ::boundary_gap_closure::{discounted_total, wrapper};\nmod source { pub struct Fake; impl Fake { pub const High: ::boundary_gap_closure::Outcome = ::boundary_gap_closure::wrapper(100, 100); } }\nmod boundary_gap_closure { pub use super::source::Fake as Outcome; }\nuse boundary_gap_closure::Outcome;\n#[test]\nfn boundary() { let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), Outcome::High); }\n";
    write_crate(&roots.fixture, &library(">="), tests)?;
    for op in [">=", ">"] {
        std::fs::write(roots.fixture.join("src/lib.rs"), library(op)).map_err(|e| e.to_string())?;
        let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
        assert_eq!(
            (run.executed, run.passed, run.failed),
            (1, 1, 0),
            "{op}: {run:?}"
        );
    }
    std::fs::write(roots.fixture.join("src/lib.rs"), library(">=")).map_err(|e| e.to_string())?;
    let root = path_arg(&roots.fixture);
    let report = run_workspace_ripr(
        &roots.fixture,
        &[
            "check",
            "--root",
            &root,
            "--mode",
            "ready",
            "--format",
            "repo-exposure-json",
        ],
    )?;
    require_success("repo exposure", &report)?;
    let report =
        boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
    let seam = boundary_seam(&report)?;
    assert_ne!(
        seam.get("grip_class").and_then(Value::as_str),
        Some("strongly_gripped"),
        "{seam}"
    );
    Ok(())
}

#[test]
fn enum_branch_generated_scope_shadow_does_not_grip() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-enum-macro-shadow")?;
    let library = |op: &str| {
        format!(
            "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub const fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub const fn wrapper(a: i32, b: i32) -> Outcome {{ if discounted_total(a,b) {{ Outcome::High }} else {{ Outcome::Low }} }}\n"
        )
    };
    let tests = "use ::boundary_gap_closure::{discounted_total, wrapper};\ngenerated::make_shadow!();\nuse crate::boundary_gap_closure::Outcome;\n#[test]\nfn boundary() { let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), Outcome::High); }\n";
    write_crate(&roots.fixture, &library(">="), tests)?;
    let generated = roots.fixture.join("generated");
    std::fs::create_dir_all(generated.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        generated.join("Cargo.toml"),
        "[package]\nname=\"generated\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(generated.join("src/lib.rs"), "#[macro_export]\nmacro_rules! make_shadow { () => { mod source { pub struct Fake; impl Fake { pub const High: ::boundary_gap_closure::Outcome = ::boundary_gap_closure::wrapper(100,100); } } mod boundary_gap_closure { pub use super::source::Fake as Outcome; } } }\n").map_err(|e| e.to_string())?;
    std::fs::write(roots.fixture.join("Cargo.toml"), "[package]\nname=\"boundary-gap-closure\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[dependencies]\ngenerated={path=\"generated\"}\n").map_err(|e| e.to_string())?;
    for op in [">=", ">"] {
        std::fs::write(roots.fixture.join("src/lib.rs"), library(op)).map_err(|e| e.to_string())?;
        let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
        assert_eq!(
            (run.executed, run.passed, run.failed),
            (1, 1, 0),
            "{op}: {run:?}"
        );
    }
    std::fs::write(roots.fixture.join("src/lib.rs"), library(">=")).map_err(|e| e.to_string())?;
    let root = path_arg(&roots.fixture);
    let report = run_workspace_ripr(
        &roots.fixture,
        &[
            "check",
            "--root",
            &root,
            "--mode",
            "ready",
            "--format",
            "repo-exposure-json",
        ],
    )?;
    require_success("repo exposure", &report)?;
    let report =
        boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
    let seam = boundary_seam(&report)?;
    assert_ne!(
        seam.get("grip_class").and_then(Value::as_str),
        Some("strongly_gripped"),
        "{seam}"
    );
    Ok(())
}

#[test]
fn enum_branch_unknown_item_macro_withholds_valid_import() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-enum-macro-control")?;
    let library = |op: &str| {
        format!(
            "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub fn wrapper(a: i32, b: i32) -> Outcome {{ if discounted_total(a,b) {{ Outcome::High }} else {{ Outcome::Low }} }}\n"
        )
    };
    let tests = |extra: &str| {
        format!(
            "use boundary_gap_closure::{{discounted_total, wrapper, Outcome}};\n{extra}\n#[test]\nfn boundary() {{ let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), Outcome::High); }}\n"
        )
    };
    write_crate(&roots.fixture, &library(">="), &tests(""))?;
    let generated = roots.fixture.join("generated");
    std::fs::create_dir_all(generated.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        generated.join("Cargo.toml"),
        "[package]\nname=\"generated\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        generated.join("src/lib.rs"),
        "#[macro_export] macro_rules! noop { () => {} }\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(roots.fixture.join("Cargo.toml"), "[package]\nname=\"boundary-gap-closure\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[dependencies]\ngenerated={path=\"generated\"}\n").map_err(|e| e.to_string())?;
    for (extra, expected_strong) in [
        ("", true),
        ("generated::noop!();", false),
        ("fn unused() { generated::noop!(); }", false),
    ] {
        std::fs::write(roots.fixture.join("tests/pricing.rs"), tests(extra))
            .map_err(|e| e.to_string())?;
        for (op, passes) in [(">=", true), (">", false)] {
            std::fs::write(roots.fixture.join("src/lib.rs"), library(op))
                .map_err(|e| e.to_string())?;
            let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
            assert_eq!(
                (run.executed, run.passed, run.failed),
                (1, u64::from(passes), u64::from(!passes)),
                "{extra}/{op}: {run:?}"
            );
        }
        std::fs::write(roots.fixture.join("src/lib.rs"), library(">="))
            .map_err(|e| e.to_string())?;
        let root = path_arg(&roots.fixture);
        let report = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &report)?;
        let report =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let seam = boundary_seam(&report)?;
        assert_eq!(
            seam.get("grip_class").and_then(Value::as_str) == Some("strongly_gripped"),
            expected_strong,
            "{extra}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn enum_branch_sibling_import_does_not_prove_test_scope() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-sibling-scope")?;
    let library = |op: &str| {
        format!(
            "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub const EXPECTED: Outcome = Outcome::High;\npub const fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub const fn wrapper(a: i32, b: i32) -> Outcome {{ if discounted_total(a,b) {{ Outcome::High }} else {{ Outcome::Low }} }}\n"
        )
    };
    for (declarations, expected) in [
        (
            "mod unused { use boundary_gap_closure::Outcome; } mod Outcome { pub const High: ::boundary_gap_closure::Outcome = ::boundary_gap_closure::wrapper(100,100); }",
            "Outcome::High",
        ),
        (
            "mod unused { use boundary_gap_closure::EXPECTED; } const EXPECTED: ::boundary_gap_closure::Outcome = ::boundary_gap_closure::wrapper(100,100);",
            "EXPECTED",
        ),
    ] {
        let tests = format!(
            "use boundary_gap_closure::{{discounted_total, wrapper}};\n{declarations}\n#[test]\nfn boundary() {{ let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), {expected}); }}\n"
        );
        write_crate(&roots.fixture, &library(">="), &tests)?;
        for op in [">=", ">"] {
            std::fs::write(roots.fixture.join("src/lib.rs"), library(op))
                .map_err(|e| e.to_string())?;
            let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
            assert_eq!(
                (run.executed, run.passed, run.failed),
                (1, 1, 0),
                "{expected}/{op}: {run:?}"
            );
        }
        std::fs::write(roots.fixture.join("src/lib.rs"), library(">="))
            .map_err(|e| e.to_string())?;
        let root = path_arg(&roots.fixture);
        let report = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &report)?;
        let report =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let seam = boundary_seam(&report)?;
        assert_ne!(
            seam.get("grip_class").and_then(Value::as_str),
            Some("strongly_gripped"),
            "{expected}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn enum_branch_existing_import_and_module_forms_observe_mutant() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-enum-scope-positive")?;
    let library = |op: &str| {
        format!(
            "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub fn wrapper(a: i32, b: i32) -> Outcome {{ if discounted_total(a,b) {{ Outcome::High }} else {{ Outcome::Low }} }}\n"
        )
    };
    let body = "#[test]\nfn boundary() { let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), Outcome::High); }\n";
    for (label, tests, selector) in [
        (
            "unrelated_glob",
            format!("mod unrelated {{ pub const OTHER: i32 = 1; }}\nuse unrelated::*;\nuse boundary_gap_closure::{{discounted_total, wrapper, Outcome}};\n{body}"),
            "boundary",
        ),
        (
            "benign_attr",
            format!("use boundary_gap_closure::{{discounted_total, wrapper, Outcome}};\n#[allow(dead_code)] fn unused() {{}}\n{body}"),
            "boundary",
        ),
        (
            "unrelated_macro",
            format!("use boundary_gap_closure::{{discounted_total, wrapper, Outcome}};\nfn unused() {{ println!(\"unused\"); }}\n{body}"),
            "boundary",
        ),
        (
            "duplicate_test_names",
            format!("mod a {{ use boundary_gap_closure::{{discounted_total, wrapper, Outcome}};\n{body}}}\nmod b {{ #[test] fn boundary() {{ assert_eq!(1,1); }} }}\n"),
            "a::boundary",
        ),
        (
            "local",
            "use boundary_gap_closure::{discounted_total, wrapper};\n#[test]\nfn boundary() { use boundary_gap_closure::Outcome; let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), Outcome::High); }\n".to_string(),
            "boundary",
        ),
        (
            "glob",
            format!("use boundary_gap_closure::*;\n{body}"),
            "boundary",
        ),
        (
            "reexport",
            format!(
                "use boundary_gap_closure::{{discounted_total, wrapper}};\npub use boundary_gap_closure::Outcome;\n{body}"
            ),
            "boundary",
        ),
        (
            "nested",
            format!(
                "mod tests {{ use boundary_gap_closure::{{discounted_total, wrapper, Outcome}};\n{body}}}"
            ),
            "tests::boundary",
        ),
        (
            "cfg_nested",
            format!(
                "#[cfg(test)] mod tests {{ use boundary_gap_closure::{{discounted_total, wrapper, Outcome}};\n{body}}}"
            ),
            "tests::boundary",
        ),
    ] {
        write_crate(&roots.fixture, &library(">="), &tests)?;
        for (op, passes) in [(">=", true), (">", false)] {
            std::fs::write(roots.fixture.join("src/lib.rs"), library(op))
                .map_err(|e| e.to_string())?;
            let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, selector)?;
            assert_eq!(
                (run.executed, run.passed, run.failed),
                (1, u64::from(passes), u64::from(!passes)),
                "{label}/{op}: {run:?}"
            );
        }
        std::fs::write(roots.fixture.join("src/lib.rs"), library(">="))
            .map_err(|e| e.to_string())?;
        let root = path_arg(&roots.fixture);
        let report = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &report)?;
        let report =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let seam = boundary_seam(&report)?;
        assert_eq!(
            seam.get("grip_class").and_then(Value::as_str),
            Some("strongly_gripped"),
            "{label}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn enum_branch_block_import_shadow_does_not_grip() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-block-shadow")?;
    let library = |op: &str| {
        format!(
            "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub const EXPECTED: Outcome = Outcome::High;\npub const fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub const fn wrapper(a: i32, b: i32) -> Outcome {{ if discounted_total(a,b) {{ Outcome::High }} else {{ Outcome::Low }} }}\n"
        )
    };
    for (outer, inner, expected) in [
        ("Outcome", "use fake::*;", "Outcome::High"),
        ("EXPECTED", "use fake::EXPECTED;", "EXPECTED"),
    ] {
        let tests = format!(
            "use boundary_gap_closure::{{discounted_total, wrapper, {outer}}};\n#[test]\nfn boundary() {{ {inner} let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), {expected}); }}\n"
        );
        write_crate(&roots.fixture, &library(">="), &tests)?;
        let fake = roots.fixture.join("fake");
        std::fs::create_dir_all(fake.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(fake.join("Cargo.toml"), "[package]\nname=\"fake\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nboundary-gap-closure={path=\"..\"}\n").map_err(|e| e.to_string())?;
        std::fs::write(fake.join("src/lib.rs"), "pub struct Other; impl Other { pub const High: boundary_gap_closure::Outcome = boundary_gap_closure::wrapper(100,100); } pub use Other as Outcome; pub const EXPECTED: boundary_gap_closure::Outcome = boundary_gap_closure::wrapper(100,100);\n").map_err(|e| e.to_string())?;
        std::fs::write(roots.fixture.join("Cargo.toml"), "[package]\nname=\"boundary-gap-closure\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[dev-dependencies]\nfake={path=\"fake\"}\n").map_err(|e| e.to_string())?;
        for op in [">=", ">"] {
            std::fs::write(roots.fixture.join("src/lib.rs"), library(op))
                .map_err(|e| e.to_string())?;
            let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
            assert_eq!(
                (run.executed, run.passed, run.failed),
                (1, 1, 0),
                "{inner}/{op}: {run:?}"
            );
        }
        std::fs::write(roots.fixture.join("src/lib.rs"), library(">="))
            .map_err(|e| e.to_string())?;
        let root = path_arg(&roots.fixture);
        let report = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &report)?;
        let report =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let seam = boundary_seam(&report)?;
        assert_ne!(
            seam.get("grip_class").and_then(Value::as_str),
            Some("strongly_gripped"),
            "{inner}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn enum_branch_public_out_of_line_module_preserves_identity() -> Result<(), String> {
    let roots = journey_roots("boundary-wrapper-public-module")?;
    let library = |op: &str| {
        format!(
            "#[derive(Debug, PartialEq)] pub enum Outcome {{ Low, High }}\npub const fn discounted_total(amount: i32, discount_threshold: i32) -> bool {{ amount {op} discount_threshold }}\npub const fn wrapper(a: i32, b: i32) -> Outcome {{ if discounted_total(a,b) {{ Outcome::High }} else {{ Outcome::Low }} }}\n"
        )
    };
    for (declarations, expected, strong) in [
        (
            "use boundary_gap_closure::pricing::{discounted_total, wrapper, Outcome};",
            "Outcome::High",
            true,
        ),
        (
            "use boundary_gap_closure::pricing::{discounted_total, wrapper}; use boundary_gap_closure::Outcome;",
            "Outcome::High",
            false,
        ),
    ] {
        let tests = format!(
            "{declarations}\n#[test]\nfn boundary() {{ let _ = discounted_total(100,100);\nassert_eq!(wrapper(100,100), {expected}); }}\n"
        );
        write_crate(
            &roots.fixture,
            "pub mod pricing;\npub struct Fake; impl Fake { pub const High: pricing::Outcome = pricing::wrapper(100,100); } pub use Fake as Outcome;\n",
            &tests,
        )?;
        for (op, passes) in [("target", true), ("mutant", !strong)] {
            std::fs::write(
                roots.fixture.join("src/pricing.rs"),
                library(if op == "target" { ">=" } else { ">" }),
            )
            .map_err(|e| e.to_string())?;
            let run = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, "boundary")?;
            assert_eq!(
                (run.executed, run.passed, run.failed),
                (1, u64::from(passes), u64::from(!passes)),
                "{declarations}/{op}: {run:?}"
            );
        }
        std::fs::write(roots.fixture.join("src/pricing.rs"), library(">="))
            .map_err(|e| e.to_string())?;
        let root = path_arg(&roots.fixture);
        let report = run_workspace_ripr(
            &roots.fixture,
            &[
                "check",
                "--root",
                &root,
                "--mode",
                "ready",
                "--format",
                "repo-exposure-json",
            ],
        )?;
        require_success("repo exposure", &report)?;
        let report =
            boundary_gap::parse_json("repo exposure", &stdout_text("repo exposure", &report)?)?;
        let seam = boundary_seam(&report)?;
        assert_eq!(
            seam.get("grip_class").and_then(Value::as_str) == Some("strongly_gripped"),
            strong,
            "{declarations}: {seam}"
        );
    }
    Ok(())
}
