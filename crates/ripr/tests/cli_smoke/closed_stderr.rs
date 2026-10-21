//! A failed diagnostic sink cannot replace a completed JSON refusal's exit.
use super::{DeclaredHunkScratch, probe_command, unique_temp_workspace};
use serde_json::Value;
use std::path::Path;
use std::process::{Output, Stdio};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn run(root: &Path, args: &[&str], closed: bool) -> Result<Output, std::io::Error> {
    let mut command = probe_command(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(root).args(args).stdin(Stdio::null());
    if closed {
        // Close before spawning: dropping Child::stderr after spawn races
        // with the child's diagnostic and can accept the old reporter.
        let (reader, writer) = std::io::pipe()?;
        drop(reader);
        command.stderr(writer);
    }
    command.output()
}

#[test]
fn malformed_config_json_keeps_exit_two_with_closed_stderr() -> TestResult {
    let root = unique_temp_workspace("closed-stderr-config");
    std::fs::create_dir_all(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    std::fs::write(root.join("ripr.toml"), "[analysis\n")?;
    std::fs::write(root.join("empty.patch"), "")?;
    for args in [
        &[
            "check",
            "--root",
            ".",
            "--diff",
            "empty.patch",
            "--json",
            "--quiet",
        ][..],
        &["diff", "--root", ".", "--base", "HEAD", "--json"][..],
        &[
            "rerun",
            "--root",
            ".",
            "--changed-test",
            "tests/value.rs",
            "--json",
        ][..],
        &[
            "agent",
            "brief",
            "--root",
            ".",
            "--diff",
            "empty.patch",
            "--json",
        ][..],
    ] {
        let normal = run(&root, args, false)?;
        assert_eq!(normal.status.code(), Some(2), "{args:?}: {normal:?}");
        let document: Value = serde_json::from_slice(&normal.stdout)?;
        assert_eq!(document["analysis_scope"]["run_status"], "config_invalid");
        assert_eq!(document["analysis_scope"]["downstream_consumable"], false);
        assert_eq!(document["findings"], serde_json::json!([]));
        assert!(String::from_utf8_lossy(&normal.stderr).starts_with("ripr: "));
        let closed = run(&root, args, true)?;
        assert_eq!(closed.status.code(), Some(2), "{args:?}: {closed:?}");
        assert_eq!(closed.stdout, normal.stdout, "{args:?}: JSON changed");
    }
    Ok(())
}

#[test]
fn execution_failure_and_typed_refusal_keep_distinct_exits_with_closed_stderr() -> TestResult {
    let root = unique_temp_workspace("closed-stderr-exits");
    std::fs::create_dir_all(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    std::fs::write(root.join("ripr.toml"), "[languages]\nenabled = []\n")?;
    for (args, exit, pointer, identity) in [
        (
            &[
                "check",
                "--root",
                ".",
                "--diff",
                "missing.patch",
                "--json",
                "--quiet",
            ][..],
            2,
            "/analysis_scope/run_status",
            "analysis_failed",
        ),
        (
            &[
                "agent",
                "verify-execute",
                "--root",
                ".",
                "--packet",
                "missing.json",
                "--result-json",
                "result.json",
                "--json",
            ][..],
            3,
            "/disposition",
            "verification_rejected_policy",
        ),
    ] {
        let normal = run(&root, args, false)?;
        assert_eq!(normal.status.code(), Some(exit), "{args:?}: {normal:?}");
        let document: Value = serde_json::from_slice(&normal.stdout)?;
        assert_eq!(document.pointer(pointer), Some(&Value::from(identity)));
        let closed = run(&root, args, true)?;
        assert_eq!(closed.status.code(), Some(exit), "{args:?}: {closed:?}");
        assert_eq!(closed.stdout, normal.stdout, "{args:?}: JSON changed");
    }
    assert!(!root.join("result.json").exists(), "verification executed");
    Ok(())
}

/// The command has already reached a typed refusal before its optional JSON
/// diagnostic is sent. A failed diagnostic sink must not turn that answer into
/// an internal-error exit. Parse the readable envelope first to reject fixture
/// setup failures, then close the pipe reader before spawning the real CLI.
#[cfg(feature = "lang-rust")]
fn assert_refusal_survives_sink_loss(
    root: &Path,
    args: &[&str],
    version: &str,
    kind: &str,
    refusal_pointer: &str,
    refusal: &str,
) -> TestResult {
    let normal = run(root, args, false)?;
    let envelope: Value = serde_json::from_slice(&normal.stdout)
        .map_err(|error| format!("missing stdout refusal envelope: {error}: {normal:?}"))?;
    assert_eq!(envelope["schema_version"], version, "{args:?}");
    assert_eq!(envelope["kind"], kind, "{args:?}");
    assert_eq!(
        envelope.pointer(refusal_pointer),
        Some(&Value::from(refusal))
    );
    if kind == "rust_test_stub" {
        assert_eq!(envelope["state"], "refused");
    }
    if refusal == "seam_not_found" {
        let identity = if kind == "agent_card_refusal" {
            &envelope["error"]["seam_id"]
        } else {
            &envelope["seam_id"]
        };
        assert_eq!(identity, "missing-stderr-seam");
    }
    assert_eq!(normal.status.code(), Some(3), "{args:?}: {normal:?}");
    let prose = String::from_utf8_lossy(&normal.stderr);
    assert!(prose.starts_with("ripr: agent "), "{args:?}: {prose}");

    let closed = run(root, args, true)?;
    assert_eq!(closed.stdout, normal.stdout, "{args:?}: JSON changed");
    assert_eq!(closed.status.code(), Some(3), "{args:?}: {closed:?}");

    // The JSON path must retain the existing human-only refusal behavior.
    let human_args: Vec<&str> = args
        .iter()
        .copied()
        .filter(|arg| *arg != "--json")
        .collect();
    for closed in [false, true] {
        let human = run(root, &human_args, closed)?;
        assert!(human.stdout.is_empty(), "{human_args:?}: {human:?}");
        assert_eq!(human.status.code(), Some(3), "{human_args:?}: {human:?}");
    }
    Ok(())
}

#[cfg(feature = "lang-rust")]
#[test]
fn unknown_card_json_refusal_keeps_exit_three_with_closed_stderr() -> TestResult {
    let root = unique_temp_workspace("closed-stderr-unknown-card");
    super::init_producer_fixture_repo(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    assert_refusal_survives_sink_loss(
        &root,
        &[
            "agent",
            "card",
            "--root",
            ".",
            "--seam-id",
            "missing-stderr-seam",
            "--json",
        ],
        "0.1",
        "agent_card_refusal",
        "/error/kind",
        "seam_not_found",
    )
}

#[cfg(feature = "lang-rust")]
#[test]
fn unknown_stub_json_refusal_keeps_exit_three_with_closed_stderr() -> TestResult {
    let root = unique_temp_workspace("closed-stderr-unknown-stub");
    super::init_producer_fixture_repo(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    assert_refusal_survives_sink_loss(
        &root,
        &[
            "agent",
            "stub",
            "--root",
            ".",
            "--seam-id",
            "missing-stderr-seam",
            "--json",
        ],
        "0.2",
        "rust_test_stub",
        "/refusal/kind",
        "seam_not_found",
    )
}

#[cfg(feature = "lang-rust")]
#[test]
fn resolved_stub_json_refusal_keeps_exit_three_with_closed_stderr() -> TestResult {
    let root = unique_temp_workspace("closed-stderr-resolved-stub");
    super::init_producer_fixture_repo(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let source = "pub async fn value(n: i32) -> bool {\n    n > 3\n}\n";
    std::fs::write(root.join("src/lib.rs"), source)?;
    assert_refusal_survives_sink_loss(
        &root,
        &[
            "agent",
            "stub",
            "--root",
            ".",
            "--at",
            "src/lib.rs:2",
            "--kind",
            "predicate",
            "--write",
            "--json",
        ],
        "0.2",
        "rust_test_stub",
        "/refusal/kind",
        "owner_async",
    )?;
    assert_eq!(std::fs::read_to_string(root.join("src/lib.rs"))?, source);
    Ok(())
}

#[cfg(feature = "lang-rust")]
#[test]
fn ready_stub_json_keeps_exit_zero_with_closed_stderr() -> TestResult {
    let root = unique_temp_workspace("closed-stderr-ready-stub");
    super::init_producer_fixture_repo(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    for closed in [false, true] {
        let output = run(
            &root,
            &[
                "agent",
                "stub",
                "--root",
                ".",
                "--at",
                "src/lib.rs:2",
                "--kind",
                "predicate",
                "--json",
            ],
            closed,
        )?;
        let document: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(document["schema_version"], "0.2");
        assert_eq!(document["kind"], "rust_test_stub");
        assert_eq!(document["state"], "ready");
        assert_eq!(document["written"], false);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
    Ok(())
}
