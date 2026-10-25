use super::{run_command_with_env, unique_temp_workspace};
use serde_json::json;

/// RIPR-SPEC-0085: a report over a findings prefix must disclose omitted
/// results even when the only limitation signal was omitted from the input.
#[test]
fn ts_limitations_discloses_bounded_findings_without_changing_counts() -> Result<(), String> {
    let root = unique_temp_workspace("ts-limitations-truncation");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let result = (|| {
        let clean = json!({"id": "clear", "language": "typescript", "evidence": []});
        let limited = json!({
            "id": "omitted", "language": "typescript",
            "evidence": ["typescript_limitation: unique_hidden_limitation"]
        });
        let legacy = json!([{"category": "limited_findings_bound"}]);
        let cases = [
            ("full", json!({"findings": [clean, limited]}), false, 2, 1),
            (
                "legacy",
                json!({"findings": [clean], "run_limitations": legacy}),
                true,
                1,
                0,
            ),
            (
                "legacy-status-only",
                json!({"findings": [clean], "run_limitations": [{"run_status": "limited_findings_bound"}]}),
                true,
                1,
                0,
            ),
            (
                "typed",
                json!({"findings": [clean], "analysis_outcome": {"document_complete": false, "findings_total": 2}}),
                true,
                1,
                0,
            ),
            (
                "malformed",
                json!({"findings": [clean], "analysis_outcome": {"document_complete": "false"}}),
                true,
                1,
                0,
            ),
            (
                "retained-limit",
                json!({"findings": [limited], "run_limitations": legacy}),
                true,
                1,
                1,
            ),
            (
                "empty",
                json!({"findings": [], "analysis_outcome": {"document_complete": true}}),
                false,
                0,
                0,
            ),
            (
                "empty-bounded",
                json!({"findings": [], "analysis_outcome": {"document_complete": false}}),
                true,
                0,
                0,
            ),
            (
                "unrelated",
                json!({"findings": [], "run_limitations": [{"category": "unrelated"}]}),
                false,
                0,
                0,
            ),
        ];
        for (name, input, bounded, findings, limitations) in cases {
            let input_path = root.join(format!("{name}-input.json"));
            let output_path = root.join(format!("{name}-output.json"));
            let markdown_path = output_path.with_extension("md");
            std::fs::write(
                &input_path,
                serde_json::to_vec(&input).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let output = run_command_with_env(
                env!("CARGO_BIN_EXE_ripr"),
                &root,
                &[
                    "reports",
                    "ts-limitations",
                    "--check-output",
                    &input_path.to_string_lossy(),
                    "--out",
                    &output_path.to_string_lossy(),
                    "--out-md",
                    &markdown_path.to_string_lossy(),
                ],
                &[],
            )
            .map_err(|error| error.to_string())?;
            assert!(
                output.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&output_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let markdown =
                std::fs::read_to_string(&markdown_path).map_err(|error| error.to_string())?;
            assert_eq!(report["status"], "advisory", "{name}");
            assert_eq!(report["summary"]["findings_total"], findings, "{name}");
            assert_eq!(
                report["summary"]["limitations_total"], limitations,
                "{name}"
            );
            let warnings = report["warnings"].as_array().ok_or("missing warnings")?;
            assert_eq!(warnings.len(), usize::from(bounded), "{name}: {warnings:?}");
            assert_eq!(
                markdown.contains("RIPR_CHECK_FINDINGS_BYTES=0"),
                bounded,
                "{name}"
            );
            if bounded {
                let warning = warnings[0].as_str().ok_or("warning must be text")?;
                assert!(warning.contains("incomplete"), "{name}: {warning}");
                assert!(warning.contains("Omitted findings"), "{name}: {warning}");
                assert!(
                    warning.contains("RIPR_CHECK_FINDINGS_BYTES=0"),
                    "{name}: {warning}"
                );
                assert!(
                    markdown.contains(warning),
                    "{name}: Markdown must retain the warning"
                );
            }
        }
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root).map_err(|error| error.to_string());
    result.and(cleanup)
}
