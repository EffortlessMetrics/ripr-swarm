//! A preview advisory must describe the existing selected analysis partition.
use super::{DeclaredHunkScratch, run_ripr_with_env, unique_temp_workspace, workspace_root};
use serde_json::{Value, json};
use std::{fs, path::Path};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture(root: &Path, language: &str, extension: &str) -> TestResult {
    let fixtures = workspace_root().join("fixtures");
    for directory in ["src", "tests"] {
        fs::create_dir_all(root.join(directory))?;
    }
    for relative in ["Cargo.toml", "src/lib.rs", "tests/pricing.rs"] {
        fs::copy(
            fixtures.join("boundary_gap/input").join(relative),
            root.join(relative),
        )?;
    }
    let preview = fixtures.join(format!("{language}_boundary_gap"));
    let source_path = format!("src/discount.{extension}");
    let source = fs::read_to_string(preview.join("input").join(&source_path))?;
    fs::write(root.join(&source_path), &source)?;
    let extra_path = format!("src/extra.{extension}");
    fs::write(
        root.join(&extra_path),
        source
            .replace("apply_discount", "extra_discount")
            .replace("applyDiscount", "extraDiscount"),
    )?;
    let preview_patch = fs::read_to_string(preview.join("diff.patch"))?;
    let extra_patch = preview_patch
        .replace(&source_path, &extra_path)
        .replace("apply_discount", "extra_discount")
        .replace("applyDiscount", "extraDiscount");
    fs::write(
        root.join("change.patch"),
        format!(
            "{}{}{}",
            fs::read_to_string(fixtures.join("boundary_gap/diff.patch"))?,
            preview_patch,
            extra_patch
        ),
    )?;
    Ok(())
}

fn assert_partition_claims(language: &str, extension: &str, display: &str) -> TestResult {
    let root = unique_temp_workspace(&format!("partial-preview-{language}"));
    let _cleanup = DeclaredHunkScratch(root.clone());
    fixture(&root, language, extension)?;
    let root_arg = root.to_string_lossy().into_owned();
    let diff_arg = root.join("change.patch").to_string_lossy().into_owned();
    let source_path = format!("src/discount.{extension}");
    let extra_path = format!("src/extra.{extension}");
    for (enabled, budget, expected_preview) in [
        (true, "1", 0),
        (true, "2", 1),
        (true, "3", 2),
        (false, "1", 0),
        (false, "3", 0),
    ] {
        fs::write(
            root.join("ripr.toml"),
            if enabled {
                format!("[languages]\nenabled = [\"rust\", \"{language}\"]\n")
            } else {
                "[languages]\nenabled = [\"rust\"]\n".to_string()
            },
        )?;
        let args = [
            "check", "--root", &root_arg, "--diff", &diff_arg, "--mode", "fast", "--format",
            "json", "--quiet",
        ];
        let env = [("RIPR_PARTIAL_DIFF_FILE_BUDGET", budget)];
        let output = run_ripr_with_env(&args, &env);
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let context = format!("{language}/enabled={enabled}/budget={budget}");
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert!(!findings.is_empty(), "{context}: Rust subject must survive");
        assert!(
            findings.iter().any(|finding| finding["probe"]["file"]
                .as_str()
                .is_some_and(|path| path.trim_start_matches("./") == "src/lib.rs")),
            "{context}"
        );
        let preview_findings = findings
            .iter()
            .filter(|finding| {
                finding["probe"]["file"]
                    .as_str()
                    .is_some_and(|path| path == source_path || path == extra_path)
            })
            .count();
        assert_eq!(
            preview_findings, expected_preview,
            "{context}: selected preview files must produce findings"
        );
        let changed = report["summary"]["changed_files_by_language"]
            .as_array()
            .ok_or("missing language counts")?;
        let count = changed
            .iter()
            .find(|entry| entry["language"] == language)
            .and_then(|entry| entry["files"].as_u64())
            .unwrap_or(0);
        assert_eq!(count, expected_preview as u64, "{context}");
        let complete = enabled && budget == "3";
        assert_eq!(
            report["analysis_outcome"]["analysis_complete"], complete,
            "{context}"
        );
        assert_eq!(
            report["analysis_outcome"]["outcome"]["kind"],
            if complete {
                "complete_with_findings"
            } else {
                "partial_with_limitations"
            },
            "{context}"
        );
        assert_eq!(
            report["analysis_outcome"]["outcome"]["counts"]["changed_file_count"], 3,
            "{context}: raw input count is unchanged"
        );
        if enabled && budget != "3" {
            let selected = if budget == "1" {
                json!(["src/lib.rs"])
            } else {
                json!([source_path, "src/lib.rs"])
            };
            assert_eq!(
                report["analysis_scope"]["selected_files"], selected,
                "{context}"
            );
            assert_eq!(
                report["analysis_scope"]["downstream_consumable"], false,
                "{context}"
            );
            assert_eq!(
                report["analysis_scope"]["gate_eligibility"], "ineligible",
                "{context}"
            );
        }
        let advisories = report.get("preview_languages").and_then(Value::as_array);
        if enabled && expected_preview == 0 {
            assert!(
                advisories.is_none_or(Vec::is_empty),
                "{context}: omitted preview files must not be advertised as analyzed: {advisories:?}"
            );
        } else {
            let advisory = advisories
                .and_then(|entries| entries.first())
                .ok_or("missing preview advisory")?;
            assert_eq!(advisory["enabled"], enabled, "{context}");
            assert_eq!(advisory["analyzed"], enabled, "{context}");
            let expected_count = if enabled { expected_preview } else { 2 };
            assert_eq!(advisory["file_count"], expected_count, "{context}");
            let expected_paths = if enabled && expected_preview == 1 {
                json!([source_path])
            } else {
                json!([source_path, extra_path])
            };
            assert_eq!(advisory["sample_paths"], expected_paths, "{context}");
        }
        // Exit assertions follow semantic/setup assertions: partial evidence stays advisory.
        assert_eq!(
            output.status.code(),
            Some(0),
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut human_args = args;
        human_args[8] = "human";
        let human = run_ripr_with_env(&human_args, &env);
        assert_eq!(
            human.status.code(),
            Some(0),
            "{context}: {}",
            String::from_utf8_lossy(&human.stderr)
        );
        let text = String::from_utf8(human.stdout)?;
        if enabled && expected_preview > 0 {
            let noun = if expected_preview == 1 {
                "file"
            } else {
                "files"
            };
            assert!(
                text.contains(&format!(
                    "{expected_preview} {display} {noun} analyzed under preview support"
                )),
                "{context}: {text}"
            );
        } else {
            assert!(
                !text.contains(&format!("{display} file analyzed under preview support"))
                    && !text.contains(&format!("{display} files analyzed under preview support")),
                "{context}: {text}"
            );
            if !enabled {
                assert!(
                    text.contains("files were not analyzed"),
                    "{context}: {text}"
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn python_partial_preview_claims_follow_selected_files() -> TestResult {
    assert_partition_claims("python", "py", "Python")
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_partial_preview_claims_follow_selected_files() -> TestResult {
    assert_partition_claims("typescript", "ts", "TypeScript")
}
