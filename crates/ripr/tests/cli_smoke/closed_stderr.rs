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
