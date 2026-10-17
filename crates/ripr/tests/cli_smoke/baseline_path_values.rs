//! Public argv/status controls for baseline diff's path options (#7871).

use super::{ignore_remove_dir_all, probe_command, unique_temp_workspace};
use std::path::Path;
use std::process::Output;

fn run(root: &Path, args: &[&str]) -> Result<Output, String> {
    probe_command(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("run baseline diff: {error}"))
}

#[test]
fn missing_path_values_refuse_option_tokens_before_writing_reports() -> Result<(), String> {
    let root = unique_temp_workspace("baseline-missing-path");
    std::fs::create_dir_all(&root).map_err(|error| format!("create fixture: {error}"))?;
    std::fs::write(
        root.join("baseline.json"),
        r#"{"schema_version":"0.1","entries":[]}"#,
    )
    .map_err(|error| format!("write baseline: {error}"))?;
    std::fs::write(
        root.join("current.json"),
        r#"{"schema_version":"0.1","status":"advisory","decisions":[]}"#,
    )
    .map_err(|error| format!("write current: {error}"))?;

    for flag in ["--baseline", "--current", "--out", "--out-md"] {
        // Repeating --current after the missing value makes the old parser
        // complete successfully, instead of merely failing at a later token.
        let output = run(
            &root,
            &[
                "baseline",
                "diff",
                "--baseline",
                "baseline.json",
                "--current",
                "current.json",
                flag,
                "--current",
                "--current",
                "current.json",
                "--out",
                "delta.json",
                "--out-md",
                "delta.md",
            ],
        )?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "flag={flag}; {output:?}");
        assert!(
            stderr.contains(&format!("missing value for {flag}")),
            "flag={flag}; {stderr}"
        );
        assert!(
            output.stdout.is_empty(),
            "usage failure printed success: {output:?}"
        );
        assert!(!root.join("delta.json").exists());
        assert!(!root.join("delta.md").exists());

        let output = run(
            &root,
            &[
                "baseline",
                "diff",
                "--baseline",
                "baseline.json",
                "--current",
                "current.json",
                flag,
            ],
        )?;
        assert_eq!(
            output.status.code(),
            Some(2),
            "trailing flag={flag}; {output:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("missing value for {flag}"))
        );
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn explicit_option_named_paths_and_unreadable_inputs_keep_advisory_reports() -> Result<(), String> {
    let root = unique_temp_workspace("baseline-explicit-path");
    std::fs::create_dir_all(root.join("input-directory"))
        .map_err(|error| format!("create fixture: {error}"))?;
    std::fs::write(
        root.join("--baseline.json"),
        r#"{"schema_version":"0.1","entries":[]}"#,
    )
    .map_err(|error| format!("write baseline: {error}"))?;
    std::fs::write(
        root.join("--current.json"),
        r#"{"schema_version":"0.1","status":"advisory","decisions":[]}"#,
    )
    .map_err(|error| format!("write current: {error}"))?;
    let read_error = std::fs::read_to_string(root.join("input-directory"))
        .err()
        .ok_or("directory fixture unexpectedly readable")?
        .to_string();
    for (current, out, out_md, unreadable) in [
        ("./--current.json", "./--delta.json", "./--delta.md", false),
        (
            "./input-directory",
            "unreadable.json",
            "unreadable.md",
            true,
        ),
    ] {
        let output = run(
            &root,
            &[
                "baseline",
                "diff",
                "--baseline",
                "./--baseline.json",
                "--current",
                current,
                "--out",
                out,
                "--out-md",
                out_md,
            ],
        )?;
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = std::fs::read_to_string(root.join(out))
            .map_err(|error| format!("read report: {error}"))?;
        let report: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| format!("parse report: {error}"))?;
        assert_eq!(report["kind"], "baseline_debt_delta");
        assert!(root.join(out_md).is_file());
        let warnings = report["warnings"]
            .as_array()
            .ok_or("missing warnings array")?;
        if unreadable {
            assert!(
                warnings
                    .iter()
                    .any(|warning| warning.as_str().is_some_and(|text| text
                        .contains("input-directory")
                        && text.contains("read current gate-decision")
                        && text.contains("failed:")
                        && text.contains(&read_error)))
            );
        } else {
            assert!(warnings.is_empty(), "{warnings:?}");
        }
    }
    ignore_remove_dir_all(&root);
    Ok(())
}
