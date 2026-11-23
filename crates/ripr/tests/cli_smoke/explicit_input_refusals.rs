//! An input file the user names explicitly that cannot be used must refuse
//! with exit 2 and a stderr line naming the flag, the path and what to pass
//! (#15254). Reports are still written first where the command writes one.

use super::{ignore_remove_dir_all, probe_command, unique_temp_workspace};
use std::path::Path;
use std::process::Output;

fn run(root: &Path, args: &[&str]) -> Result<Output, String> {
    probe_command(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("run ripr: {error}"))
}

fn fixture(label: &str) -> Result<std::path::PathBuf, String> {
    let root = unique_temp_workspace(label);
    std::fs::create_dir_all(&root).map_err(|error| format!("create fixture: {error}"))?;
    std::fs::write(root.join("bad.json"), "{ not json")
        .map_err(|error| format!("write bad.json: {error}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"refusal-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| format!("write Cargo.toml: {error}"))?;
    Ok(root)
}

/// Exit 2 and every fragment on stderr.
fn assert_refusal(root: &Path, args: &[&str], fragments: &[&str]) -> Result<(), String> {
    let output = run(root, args)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() != Some(2) {
        return Err(format!(
            "{args:?} must exit 2, got {:?}\nstdout:\n{}\nstderr:\n{stderr}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    for fragment in fragments {
        if !stderr.contains(fragment) {
            return Err(format!("{args:?} stderr missing `{fragment}`:\n{stderr}"));
        }
    }
    Ok(())
}

#[test]
fn ts_reports_refuse_unusable_explicit_input_after_writing_blocked_report() -> Result<(), String> {
    let root = fixture("ts-reports-refusal")?;
    assert_refusal(
        &root,
        &["reports", "ts-limitations", "--check-output", "bad.json"],
        &["--check-output", "blocked", "Pass --check-output"],
    )?;
    let written =
        std::fs::read_to_string(root.join("target/ripr/reports/typescript-limitations.json"))
            .map_err(|error| format!("blocked report must be written first: {error}"))?;
    if !written.contains("\"status\": \"blocked\"") {
        return Err(format!("report must be blocked:\n{written}"));
    }
    assert_refusal(
        &root,
        &["reports", "ts-false-actionable", "--corpus", "bad.json"],
        &["--corpus", "blocked", "Pass --corpus"],
    )?;
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn coverage_grip_frontier_refuses_unusable_ledger() -> Result<(), String> {
    let root = fixture("frontier-refusal")?;
    for ledger in ["nope.json", "bad.json"] {
        assert_refusal(
            &root,
            &["coverage-grip", "frontier", "--ledger", ledger],
            &[
                "could not use 1 supplied input",
                ledger,
                "Pass a readable --coverage",
            ],
        )?;
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn suppression_health_refuses_named_missing_manifest_but_not_the_default() -> Result<(), String> {
    let root = fixture("suppression-manifest-refusal")?;
    assert_refusal(
        &root,
        &["policy", "suppression-health", "--manifest", "nope.toml"],
        &["--manifest nope.toml", "does not exist", "pass --manifest"],
    )?;
    let output = run(&root, &["policy", "suppression-health"])?;
    if output.status.code() != Some(0) {
        return Err(format!(
            "an absent default manifest stays a quiet skip: {:?}\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    std::fs::write(root.join("bad.toml"), "[[suppression]\nnot = toml")
        .map_err(|error| error.to_string())?;
    assert_refusal(
        &root,
        &["policy", "suppression-health", "--manifest", "bad.toml"],
        &["could not use --manifest bad.toml", "config_error"],
    )?;
    let absolute = root.join("absent.toml");
    let absolute = absolute.display().to_string();
    let output = run(
        &root,
        &["policy", "suppression-health", "--manifest", &absolute],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() != Some(2) || stderr.contains("under --root") {
        return Err(format!("absolute manifest refusal: {stderr}"));
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn policy_reports_refuse_unreadable_supplied_inputs() -> Result<(), String> {
    let root = fixture("policy-inputs-refusal")?;
    std::fs::write(root.join("ready.json"), "{}").map_err(|error| error.to_string())?;
    assert_refusal(
        &root,
        &[
            "policy",
            "operations",
            "--policy-readiness",
            "ready.json",
            "--waiver-aging",
            "nope.json",
        ],
        &["--waiver-aging", "nope.json", "Pass an existing file"],
    )?;
    assert_refusal(
        &root,
        &[
            "policy",
            "promote",
            "--to",
            "visible-only",
            "--operations",
            "nope.json",
        ],
        &["--operations", "nope.json", "Pass an existing file"],
    )?;
    assert_refusal(
        &root,
        &["policy", "waiver-aging", "--ledger", "nope.json"],
        &["--ledger", "nope.json", "Pass an existing file"],
    )?;
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn annotations_refuse_explicit_missing_comments_but_skip_the_default() -> Result<(), String> {
    let root = fixture("annotations-refusal")?;
    assert_refusal(
        &root,
        &["annotations", "--comments", "typo.json"],
        &["--comments typo.json", "does not exist", "pass --comments"],
    )?;
    let output = run(&root, &["annotations"])?;
    if output.status.code() != Some(0) {
        return Err(format!(
            "default skip must stay exit 0: {:?}",
            output.status.code()
        ));
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn pr_summary_refuses_unusable_baseline_before_writing() -> Result<(), String> {
    let root = fixture("pr-summary-baseline-refusal")?;
    // A UTF-16LE file one byte short of a code unit: the decoder refuses it
    // and the refusal names the encoding instead of reporting a missing file.
    let mut truncated = vec![0xff, 0xfe];
    for unit in "{}".encode_utf16() {
        truncated.extend_from_slice(&unit.to_le_bytes());
    }
    truncated.push(b'\n');
    std::fs::write(root.join("truncated-utf16.json"), &truncated)
        .map_err(|error| format!("write truncated-utf16.json: {error}"))?;
    for (baseline, reason) in [
        ("nope.json", "missing or unreadable"),
        ("bad.json", "not valid JSON"),
        ("truncated-utf16.json", "UTF-16"),
    ] {
        assert_refusal(
            &root,
            &["pr-summary", "--baseline", baseline],
            &["--baseline", baseline, reason, "pass --baseline"],
        )?;
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn required_input_commands_refuse_missing_inputs() -> Result<(), String> {
    let root = fixture("required-inputs-refusal")?;
    std::fs::write(
        root.join("good-baseline.json"),
        r#"{"schema_version":"0.1","entries":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("good-current.json"),
        r#"{"schema_version":"0.1","status":"advisory","decisions":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    // Each input alone, so neither refusal rides on the other.
    for (baseline, current, flag, path) in [
        ("nope.json", "good-current.json", "--baseline", "nope.json"),
        ("bad.json", "good-current.json", "--baseline", "bad.json"),
        ("good-baseline.json", "nope.json", "--current", "nope.json"),
        ("good-baseline.json", "bad.json", "--current", "bad.json"),
    ] {
        assert_refusal(
            &root,
            &[
                "baseline",
                "diff",
                "--baseline",
                baseline,
                "--current",
                current,
            ],
            &[flag, path, "Pass an existing file"],
        )?;
    }
    assert_refusal(
        &root,
        &["zero", "status", "--delta", "bad.json"],
        &["--delta", "bad.json", "not valid JSON"],
    )?;
    assert_refusal(
        &root,
        &["zero", "status", "--delta", "nope.json"],
        &["--delta", "nope.json", "Pass an existing file"],
    )?;
    assert_refusal(
        &root,
        &["first-action", "--pr-guidance", "nope.json"],
        &["--pr-guidance", "nope.json", "Pass an existing file"],
    )?;
    assert_refusal(
        &root,
        &["assistant-loop", "proof", "--before", "nope.json"],
        &["--before", "nope.json", "Pass an existing file"],
    )?;
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn gate_warns_on_stderr_when_an_optional_input_is_unavailable() -> Result<(), String> {
    let root = fixture("gate-optional-warning")?;
    std::fs::write(
        root.join("guidance.json"),
        r#"{"schema_version":"0.1","comments":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    let output = run(
        &root,
        &[
            "gate",
            "evaluate",
            "--pr-guidance",
            "guidance.json",
            "--labels-json",
            "nope.json",
        ],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("--labels-json nope.json is unavailable") {
        return Err(format!("stderr must name the dropped input:\n{stderr}"));
    }
    if !stderr.contains("Pass a readable file of the kind --labels-json names") {
        return Err(format!("stderr must say what to pass:\n{stderr}"));
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn gate_names_an_unavailable_mutation_calibration_once() -> Result<(), String> {
    let root = fixture("gate-mutation-calibration-once")?;
    std::fs::write(
        root.join("guidance.json"),
        r#"{"schema_version":"0.1","comments":[]}"#,
    )
    .map_err(|error| error.to_string())?;
    let output = run(
        &root,
        &[
            "gate",
            "evaluate",
            "--pr-guidance",
            "guidance.json",
            "--mutation-calibration",
            "nope.json",
        ],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mentions = stderr
        .matches("--mutation-calibration nope.json is unavailable")
        .count();
    if mentions != 1 {
        return Err(format!(
            "stderr must name the dropped input exactly once, saw {mentions}:\n{stderr}"
        ));
    }
    ignore_remove_dir_all(&root);
    Ok(())
}
