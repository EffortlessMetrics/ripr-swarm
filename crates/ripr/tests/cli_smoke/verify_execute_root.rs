//! Root admission is an operational failure with the existing execution response.
use super::{DeclaredHunkScratch, producer_verify_packet, run_command, unique_temp_workspace};
use serde_json::Value;
use std::path::Path;
use std::process::Output;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn execute(root: &Path, selected: &str) -> Result<Output, std::io::Error> {
    run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        &[
            "agent",
            "verify-execute",
            "--root",
            selected,
            "--packet",
            "packet.json",
            "--result-json",
            "result.json",
            "--authorize",
            "--json",
        ],
    )
}

fn assert_root_failure(output: &Output, selected: &str) -> TestResult {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(document["schema_version"], "1");
    assert_eq!(document["disposition"], "verification_wrong_root");
    assert_eq!(document["executed"], false);
    assert_eq!(document["result_committed"], false);
    let reason = document["reason"].as_str().ok_or("missing root reason")?;
    assert!(reason.contains(selected), "{reason}");
    assert!(reason.contains("pass the directory that contains the workspace"));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("ripr: {reason}\n")
    );
    let object = document.as_object().ok_or("response must be an object")?;
    for field in [
        "source_artifact",
        "inputs",
        "preflight",
        "command_spec",
        "result",
        "observed_exit_status",
    ] {
        assert!(!object.contains_key(field), "refusal manufactured {field}");
    }
    assert!(document["claim_boundary"].as_str().is_some());
    assert!(document["static_evidence_boundary"].as_str().is_some());
    assert!(document["descendant_containment"].as_str().is_some());
    Ok(())
}

#[test]
fn missing_root_keeps_execution_json_without_creating_the_root() -> TestResult {
    let root = unique_temp_workspace("verify-execute-missing-root-json");
    std::fs::create_dir_all(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let selected = "missing root's directory";
    let output = execute(&root, selected)?;
    assert_root_failure(&output, selected)?;
    assert!(!root.join(selected).exists());
    assert!(!root.join("result.json").exists());
    Ok(())
}

#[test]
fn file_root_keeps_execution_json_and_preserves_the_file() -> TestResult {
    let root = unique_temp_workspace("verify-execute-file-root-json");
    std::fs::create_dir_all(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let selected = "file root's data";
    std::fs::write(root.join(selected), b"root canary")?;
    let output = execute(&root, selected)?;
    assert_root_failure(&output, selected)?;
    assert_eq!(std::fs::read(root.join(selected))?, b"root canary");
    assert!(!root.join("result.json").exists());
    Ok(())
}

#[test]
fn root_failure_can_recover_with_a_producer_packet_and_usage_stays_empty() -> TestResult {
    let (root, _) = producer_verify_packet("verify-execute-root-json-recovery")?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let failed = execute(&root, "missing")?;
    assert_root_failure(&failed, "missing")?;
    assert!(!root.join("result.json").exists());

    let malformed = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["agent", "verify-execute", "--root", ".", "--json"],
    )?;
    assert_eq!(malformed.status.code(), Some(2));
    assert!(
        malformed.stdout.is_empty(),
        "argv errors must not invent an attempt"
    );
    assert!(String::from_utf8_lossy(&malformed.stderr).contains("requires --packet"));
    assert!(!root.join("result.json").exists());

    let recovered = execute(&root, ".")?;
    assert_eq!(recovered.status.code(), Some(0), "{recovered:?}");
    let document: Value = serde_json::from_slice(&recovered.stdout)?;
    assert_eq!(document["schema_version"], "1");
    assert_eq!(document["disposition"], "verification_executed_pass");
    assert_eq!(document["executed"], true);
    assert_eq!(document["result_committed"], true);
    let committed: Value = serde_json::from_slice(&std::fs::read(root.join("result.json"))?)?;
    assert_eq!(committed, document);
    Ok(())
}
