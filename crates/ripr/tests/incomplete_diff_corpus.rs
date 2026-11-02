//! Versioned incomplete unified-diff corpus runner (#4514, parent #4506).
//!
//! Executes retained fixture bytes through file and stdin `ripr check --diff`
//! routes and checks JSON completeness, malformed_diff limitations, and human
//! output parity. Exact-candidate harness packets and removal controls remain
//! follow-ups under #4510.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: String,
    check_root: String,
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize)]
struct Row {
    id: String,
    role: String,
    diff_path: String,
    diff_sha256: String,
    expected_analysis_complete: bool,
    expected_exit_code: i32,
    expect_malformed_diff_limitation: bool,
    #[serde(default)]
    expect_nonzero_probe_count: bool,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_root() -> PathBuf {
    workspace_root().join("fixtures/incomplete_diff_corpus")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn spawn_ripr(args: &[&str], stdin: Option<&[u8]>) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(bytes) = stdin {
        command.stdin(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("spawn ripr: {error}"))?;
        if let Some(mut input) = child.stdin.take() {
            use std::io::Write;
            input
                .write_all(bytes)
                .map_err(|error| format!("write ripr stdin: {error}"))?;
        }
        child
            .wait_with_output()
            .map_err(|error| format!("wait ripr: {error}"))
    } else {
        command
            .output()
            .map_err(|error| format!("run ripr: {error}"))
    }
}

fn assert_route(
    row: &Row,
    channel: &str,
    output: &Output,
    report: &serde_json::Value,
    human: &str,
) -> Result<(), String> {
    let exit = output
        .status
        .code()
        .ok_or_else(|| format!("{}: ripr terminated without an exit status", row.id))?;
    if exit != row.expected_exit_code {
        return Err(format!(
            "{} / {channel}: expected exit {}, got {exit}; stderr={}",
            row.id,
            row.expected_exit_code,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let complete = report
        .pointer("/analysis_outcome/analysis_complete")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| format!("{} / {channel}: missing analysis_complete", row.id))?;
    if complete != row.expected_analysis_complete {
        return Err(format!(
            "{} / {channel}: expected analysis_complete={}, got {complete}; report={report}",
            row.id, row.expected_analysis_complete
        ));
    }
    let limitations = report
        .pointer("/analysis_outcome/outcome/limitations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{} / {channel}: missing limitations", row.id))?;
    let malformed = limitations.iter().any(|item| {
        item.get("kind").and_then(serde_json::Value::as_str) == Some("malformed_diff")
            && item
                .get("producer_stage")
                .and_then(serde_json::Value::as_str)
                == Some("diff_parse")
            && item
                .pointer("/recovery/kind")
                .and_then(serde_json::Value::as_str)
                == Some("retry")
    });
    if malformed != row.expect_malformed_diff_limitation {
        return Err(format!(
            "{} / {channel}: malformed_diff limitation mismatch (expected {}): {limitations:?}",
            row.id, row.expect_malformed_diff_limitation
        ));
    }
    if row.expect_nonzero_probe_count {
        let probes = report
            .pointer("/analysis_outcome/outcome/counts/probe_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if probes == 0 {
            return Err(format!(
                "{} / {channel}: expected nonzero probe_count for advisory retention",
                row.id
            ));
        }
    }
    if !complete
        && human.contains("analysis complete")
        && !human.to_ascii_lowercase().contains("incomplete")
        && !human.contains("not supported")
    {
        return Err(format!(
            "{} / {channel}: human output strengthened incomplete json state: {human:?}",
            row.id
        ));
    }
    Ok(())
}

fn run_row(row: &Row, check_root: &Path) -> Result<(), String> {
    let corpus = corpus_root();
    let diff_path = corpus.join(&row.diff_path);
    let bytes =
        fs::read(&diff_path).map_err(|error| format!("read {}: {error}", diff_path.display()))?;
    let digest = sha256_hex(&bytes);
    if digest != row.diff_sha256 {
        return Err(format!(
            "{}: diff digest mismatch expected {} got {digest}",
            row.id, row.diff_sha256
        ));
    }
    let root_arg = check_root.display().to_string();
    let file_arg = diff_path.display().to_string();
    let json_args = ["check", "--root", &root_arg, "--diff", &file_arg, "--json"];
    let human_args = ["check", "--root", &root_arg, "--diff", &file_arg];
    let stdin_json_args = ["check", "--root", &root_arg, "--diff", "-", "--json"];
    let stdin_human_args = ["check", "--root", &root_arg, "--diff", "-"];

    let file_json = spawn_ripr(&json_args, None)?;
    let file_report: serde_json::Value =
        serde_json::from_slice(&file_json.stdout).map_err(|error| {
            format!(
                "{} / file: invalid json ({error}): {}",
                row.id,
                String::from_utf8_lossy(&file_json.stdout)
            )
        })?;
    let file_human = spawn_ripr(&human_args, None)?;
    let file_human_text = String::from_utf8_lossy(&file_human.stdout).into_owned();
    assert_route(row, "file/json", &file_json, &file_report, &file_human_text)?;

    let stdin_json = spawn_ripr(&stdin_json_args, Some(&bytes))?;
    let stdin_report: serde_json::Value =
        serde_json::from_slice(&stdin_json.stdout).map_err(|error| {
            format!(
                "{} / stdin: invalid json ({error}): {}",
                row.id,
                String::from_utf8_lossy(&stdin_json.stdout)
            )
        })?;
    let stdin_human = spawn_ripr(&stdin_human_args, Some(&bytes))?;
    let stdin_human_text = String::from_utf8_lossy(&stdin_human.stdout).into_owned();
    assert_route(
        row,
        "stdin/json",
        &stdin_json,
        &stdin_report,
        &stdin_human_text,
    )?;

    let file_complete = file_report
        .pointer("/analysis_outcome/analysis_complete")
        .and_then(serde_json::Value::as_bool);
    let stdin_complete = stdin_report
        .pointer("/analysis_outcome/analysis_complete")
        .and_then(serde_json::Value::as_bool);
    if file_complete != stdin_complete {
        return Err(format!(
            "{}: file/stdin analysis_complete disagree ({file_complete:?} vs {stdin_complete:?})",
            row.id
        ));
    }
    Ok(())
}

#[test]
fn incomplete_diff_corpus_runs_file_and_stdin_routes() -> Result<(), String> {
    let corpus_path = corpus_root().join("corpus.json");
    let raw = fs::read_to_string(&corpus_path)
        .map_err(|error| format!("read {}: {error}", corpus_path.display()))?;
    let corpus: Corpus =
        serde_json::from_str(&raw).map_err(|error| format!("parse corpus.json: {error}"))?;
    if corpus.schema_version != "incomplete_diff_corpus.v1" {
        return Err(format!(
            "unexpected corpus schema_version {}",
            corpus.schema_version
        ));
    }
    if corpus.rows.is_empty() {
        return Err("corpus must retain at least one row".to_string());
    }
    let check_root = workspace_root().join(&corpus.check_root);
    if !check_root.is_dir() {
        return Err(format!(
            "missing check_root directory {}",
            check_root.display()
        ));
    }
    for row in &corpus.rows {
        if row.role != "incomplete" && row.role != "complete_control" {
            return Err(format!("{}: unknown role {}", row.id, row.role));
        }
        run_row(row, &check_root)?;
    }
    Ok(())
}
