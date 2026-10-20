use super::*;

/// Exercise saved bytes, not stderr alone: foreign same-line rows retain their
/// confidence label, and a different-file record remains visibly unmatched.
#[test]
fn calibrate_saved_json_retains_head_provenance() -> Result<(), String> {
    assert_saved_calibration_provenance("json")
}

#[test]
fn calibrate_saved_markdown_retains_head_provenance() -> Result<(), String> {
    assert_saved_calibration_provenance("md")
}

fn assert_saved_calibration_provenance(format: &str) -> Result<(), String> {
    let workspace = unique_temp_workspace("calibrate-saved-provenance");
    std::fs::create_dir_all(&workspace).map_err(|e| format!("create workspace: {e}"))?;
    let head_a = "1111111111111111111111111111111111111111";
    let head_b = "2222222222222222222222222222222222222222";
    let cases = [
        (
            "matching",
            Some(head_a),
            Some(head_a),
            "matching_heads",
            false,
        ),
        (
            "different",
            Some(head_a),
            Some(head_b),
            "different_heads",
            false,
        ),
        ("snapshot-only", Some(head_a), None, "missing_head", false),
        ("mutants-only", None, Some(head_b), "missing_head", false),
        ("neither", None, None, "missing_head", false),
        (
            "short-snapshot",
            Some("abc1234"),
            Some(head_b),
            "missing_head",
            false,
        ),
        (
            "malformed-mutants",
            Some(head_a),
            Some("not-a-git-head"),
            "missing_head",
            false,
        ),
        (
            "invalid-both",
            Some("111111111111111111111111111111111111111g"),
            Some("abc1234"),
            "missing_head",
            false,
        ),
        // The existing head reader deliberately does not infer a head from a
        // directory's combined array, even when its members name matching SHAs.
        (
            "combined-directory",
            Some(head_a),
            Some(head_a),
            "missing_head",
            true,
        ),
    ];
    for (name, snapshot_head, mutants_head, relationship, directory) in cases {
        let case_dir = workspace.join(name);
        std::fs::create_dir_all(&case_dir).map_err(|e| format!("create case: {e}"))?;
        let mut snapshot: serde_json::Value =
            serde_json::from_str(
                &std::fs::read_to_string(workspace_root().join(
                    "fixtures/boundary_gap/calibration/after-targeted-test.repo-exposure.json",
                ))
                .map_err(|e| format!("read snapshot: {e}"))?,
            )
            .map_err(|e| format!("parse snapshot: {e}"))?;
        snapshot
            .as_object_mut()
            .ok_or("snapshot must be an object")?
            .remove("artifact");
        if let Some(head) = snapshot_head {
            snapshot["artifact"] = serde_json::json!({"repository": {"head": head}});
        }
        let repo_path = case_dir.join("repo.json");
        std::fs::write(&repo_path, snapshot.to_string()).map_err(|e| format!("write repo: {e}"))?;
        let mut mutants = serde_json::json!({"mutants": [
            {"id": "foreign-same-line", "file": "src/lib.rs", "line": 2,
             "operator": "replace - with +", "outcome": "missed"},
            {"id": "foreign-other-file", "file": "src/other.rs", "line": 2,
             "operator": "replace - with +", "outcome": "missed"}
        ]});
        if let Some(head) = mutants_head {
            mutants["artifact"] = serde_json::json!({"repository": {"head": head}});
        }
        let mutants_path = if directory {
            let path = case_dir.join("mutants.out");
            std::fs::create_dir_all(&path).map_err(|e| format!("create mutants.out: {e}"))?;
            std::fs::write(path.join("outcomes.json"), mutants.to_string())
                .map_err(|e| format!("write outcomes: {e}"))?;
            // A nonempty duplicate imports exactly once after combination.
            std::fs::write(path.join("mutants.json"), mutants.to_string())
                .map_err(|e| format!("write mutants: {e}"))?;
            path
        } else {
            let path = case_dir.join("mutants.json");
            std::fs::write(&path, mutants.to_string())
                .map_err(|e| format!("write mutants: {e}"))?;
            path
        };
        let expected_snapshot = snapshot_head.filter(|head| *head == head_a || *head == head_b);
        let expected_mutants =
            mutants_head.filter(|head| !directory && (*head == head_a || *head == head_b));
        let out_path = case_dir.join(format!("report.{format}"));
        let args = [
            "calibrate",
            "cargo-mutants",
            "--repo-exposure-json",
            &repo_path.display().to_string(),
            "--mutants-json",
            &mutants_path.display().to_string(),
            "--format",
            format,
        ];
        let stdout_run = run_ripr(&args);
        assert_success(&stdout_run);
        let mut saved_args = args.to_vec();
        let out_arg = out_path.display().to_string();
        saved_args.extend(["--out", &out_arg]);
        let saved_run = run_ripr(&saved_args);
        assert_success(&saved_run);
        assert!(saved_run.stdout.is_empty(), "--out must leave stdout empty");
        let saved = std::fs::read(&out_path).map_err(|e| format!("read saved report: {e}"))?;
        assert_eq!(
            saved, stdout_run.stdout,
            "saved/stdout parity: {name}/{format}"
        );
        assert_eq!(
            saved_run.stderr, stdout_run.stderr,
            "stderr parity: {name}/{format}"
        );
        let disclosure =
            String::from_utf8(saved_run.stderr).map_err(|e| format!("stderr UTF-8: {e}"))?;
        let disclosure = disclosure.trim();
        assert!(!disclosure.is_empty(), "provenance must remain on stderr");
        let saved_text = String::from_utf8(saved).map_err(|e| format!("report UTF-8: {e}"))?;
        if format == "json" {
            let report: serde_json::Value =
                serde_json::from_str(&saved_text).map_err(|e| format!("parse saved JSON: {e}"))?;
            assert_eq!(report["status"], "advisory");
            let provenance = &report["provenance"];
            assert_eq!(
                provenance["disclosure"], disclosure,
                "saved JSON must retain stderr caveat: {name}"
            );
            assert_eq!(report["schema_version"], "0.3");
            assert_eq!(
                provenance["repo_exposure_head"],
                serde_json::json!(expected_snapshot),
                "{name}"
            );
            assert_eq!(
                provenance["cargo_mutants_head"],
                serde_json::json!(expected_mutants),
                "{name}"
            );
            assert_eq!(provenance["head_relationship"], relationship, "{name}");
            assert_eq!(report["metrics"]["mutants_total"], 2);
            assert_eq!(report["metrics"]["matched_total"], 1);
            assert_eq!(report["metrics"]["unmatched_mutants_total"], 1);
            assert_eq!(report["matches"][0]["join_method"], "file_line");
            assert_eq!(
                report["matches"][0]["runtime"]["mutant_id"],
                "foreign-same-line"
            );
            assert_eq!(
                report["matches"][0]["confidence_label"],
                "contradicts_static_clean"
            );
            assert_eq!(
                report["unmatched_mutants"][0]["mutant_id"],
                "foreign-other-file"
            );
            assert_eq!(
                report["unmatched_mutants"][0]["unmatched_reason"],
                "no_seam_on_line"
            );
        } else {
            assert!(
                saved_text.contains("## Join provenance"),
                "{name}: {saved_text}"
            );
            assert!(
                saved_text.contains(disclosure),
                "saved Markdown must retain stderr caveat: {name}"
            );
            assert!(saved_text.contains("| mutants_total | 2 |"));
            assert!(saved_text.contains("| matched_total | 1 |"));
            assert!(saved_text.contains("| unmatched_mutants_total | 1 |"));
            assert!(saved_text.contains("`file_line` | `contradicts_static_clean`"));
            assert!(
                saved_text.contains("foreign-other-file") && saved_text.contains("no_seam_on_line")
            );
        }
        match relationship {
            "matching_heads" => assert!(
                disclosure.contains("same commit")
                    && disclosure.contains("working tree may still differ")
            ),
            "different_heads" => assert!(
                disclosure.contains("different repository heads")
                    && disclosure.contains("different subject")
            ),
            _ => assert!(
                disclosure.contains("does not carry a head SHA")
                    && disclosure.contains("same repository and adjacent commits")
            ),
        }
        for head in [expected_snapshot, expected_mutants].into_iter().flatten() {
            assert!(
                saved_text.contains(head),
                "saved head missing: {name}/{format}"
            );
        }
    }
    ignore_remove_dir_all(&workspace);
    Ok(())
}
