//! Input-file refusals name the flag and file, say what is wrong in plain
//! words and never leak raw OS errno or serde-only text (#15255).

use super::{run_ripr, unique_temp_workspace};
use std::path::Path;

fn fixture(label: &str) -> Result<std::path::PathBuf, String> {
    let dir = unique_temp_workspace(label);
    std::fs::create_dir_all(&dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    Ok(dir)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|err| format!("write {}: {err}", path.display()))
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_plain(stderr: &str) {
    assert!(!stderr.contains("os error"), "errno leaked:\n{stderr}");
}

#[test]
fn outcome_non_json_snapshot_names_role_file_and_serde_position() -> Result<(), String> {
    let dir = fixture("input-err-outcome")?;
    let before = dir.join("x.txt");
    write(&before, b"hello")?;
    let before_arg = before.display().to_string();
    let output = run_ripr(&["outcome", "--before", &before_arg, "--after", &before_arg]);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("before snapshot ")
            && stderr.contains("x.txt is not valid JSON (expected value at line 1 column 1)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("ripr check --format repo-exposure-json"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("failed to parse repo exposure JSON"),
        "{stderr}"
    );
    Ok(())
}

#[test]
fn outcome_binary_snapshot_says_not_utf8_text() -> Result<(), String> {
    let dir = fixture("input-err-binary")?;
    let before = dir.join("blob.bin");
    write(&before, &[b'a', 0xff, 0xfe, 0x00])?;
    let before_arg = before.display().to_string();
    let output = run_ripr(&["outcome", "--before", &before_arg, "--after", &before_arg]);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("not UTF-8 text"), "{stderr}");
    assert!(stderr.contains("pass a UTF-8 text file"), "{stderr}");
    assert!(!stderr.contains("stream did not contain"), "{stderr}");
    Ok(())
}

#[test]
fn calibrate_non_json_inputs_name_flag_and_file() -> Result<(), String> {
    let dir = fixture("input-err-calibrate")?;
    let bad = dir.join("x.txt");
    write(&bad, b"nope")?;
    let ok = dir.join("ok.json");
    write(&ok, b"[]")?;
    let bad_arg = bad.display().to_string();
    let ok_arg = ok.display().to_string();
    let output = run_ripr(&[
        "calibrate",
        "cargo-mutants",
        "--repo-exposure-json",
        &bad_arg,
        "--mutants-json",
        &ok_arg,
    ]);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("--repo-exposure-json ")
            && stderr.contains("x.txt is not valid JSON (expected ident at line 1 column 2)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("ripr check --format repo-exposure-json"),
        "{stderr}"
    );
    Ok(())
}

#[test]
fn agent_receipt_missing_verify_json_is_plain_words() -> Result<(), String> {
    let dir = fixture("input-err-receipt")?;
    let root = dir.display().to_string();
    let output = run_ripr(&[
        "agent",
        "receipt",
        "--root",
        &root,
        "--verify-json",
        "nope.json",
        "--seam-id",
        "seam-a",
        "--json",
    ]);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains(
            "agent receipt --verify-json nope.json does not exist; pass the verify JSON that `ripr agent verify` wrote"
        ),
        "{stderr}"
    );
    assert_plain(&stderr);
    assert!(!stderr.contains("canonicalize"), "{stderr}");
    Ok(())
}

#[test]
fn gate_evaluate_missing_acknowledgements_names_that_flag() -> Result<(), String> {
    let dir = fixture("input-err-ack")?;
    let guidance = dir.join("comments.json");
    write(&guidance, super::SMOKE_PR_GUIDANCE_JSON.as_bytes())?;
    let out = dir.join("gate-decision.json");
    let output = run_ripr(&[
        "gate",
        "evaluate",
        "--pr-guidance",
        &guidance.display().to_string(),
        "--acknowledgements",
        &dir.join("nope.toml").display().to_string(),
        "--out",
        &out.display().to_string(),
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr_of(&output));
    let decision = std::fs::read_to_string(&out).map_err(|err| format!("read decision: {err}"))?;
    assert!(
        decision.contains("failed to read --acknowledgements ")
            && decision.contains("does not exist; check that the path exists"),
        "{decision}"
    );
    assert!(!decision.contains("exception policy"), "{decision}");
    assert!(!decision.contains("os error"), "{decision}");
    Ok(())
}

#[test]
fn swarm_ingest_non_json_result_names_file() -> Result<(), String> {
    let dir = fixture("input-err-swarm")?;
    write(&dir.join("bad.json"), b"{ nope")?;
    let root = dir.display().to_string();
    let output = run_ripr(&["swarm", "ingest", "--root", &root, "--result", "bad.json"]);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("swarm ingest result ")
            && stderr
                .contains("bad.json is not valid JSON (key must be a string at line 1 column 3)"),
        "{stderr}"
    );
    Ok(())
}
