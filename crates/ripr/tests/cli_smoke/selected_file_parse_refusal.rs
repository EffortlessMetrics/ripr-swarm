//! Real after-phase refusal consumers for RIPR-SPEC-0289.
use super::{
    assert_success, repair_route_after, repair_route_attempt_id, repair_route_before,
    repair_route_commit, repair_route_manifest, repair_route_status, repair_route_workspace,
    run_command, run_git,
};

/// An unparseable observation concerns the whole selected file and may be an
/// analysis limit. Real after runs must preserve that uncertainty on stderr,
/// JSON and the durable refusal; ordinary production edits get no parse hint.
#[test]
fn selected_file_parse_refusal_reaches_actual_consumers() -> Result<(), Box<dyn std::error::Error>>
{
    const HINT: &str = "the selected file could not be parsed for the inline test region (syntax errors or an analysis limit); if there is a syntax error, fix it, then start a new attempt.";
    const RECORDED_HINT: &str = "The selected file could not be parsed for the inline test region (syntax errors or an analysis limit); if there is a syntax error, fix it, then start a new attempt";
    const INLINE: &str = "\n#[cfg(test)]\nmod tests {\n    use super::discounted_total;\n    #[test]\n    fn below() { assert_eq!(discounted_total(50, 100), 50); }\n}\n";
    for case in [
        "test_syntax",
        "production_syntax",
        "analysis_limit",
        "production_edit",
        "valid_test",
    ] {
        let root = repair_route_workspace(&format!("parse-refusal-{case}"))?;
        std::fs::remove_dir_all(root.join("tests"))?;
        let manifest_path = root.join("Cargo.toml");
        let manifest_text = std::fs::read_to_string(&manifest_path)?;
        std::fs::write(&manifest_path, format!("{manifest_text}\n[workspace]\n"))?;
        let production = std::fs::read_to_string(root.join("src/lib.rs"))?;
        let before_source = format!("{production}{INLINE}");
        std::fs::write(root.join("src/lib.rs"), &before_source)?;
        run_git(&root, &["add", "-A"])?;
        repair_route_commit(&root, "inline-only target")?;
        let before = repair_route_before(&root)?;
        let attempt_id = repair_route_attempt_id(&before)?;
        let after_source = match case {
            "test_syntax" => before_source.replace("mod tests {", "mod tests {\n    #[test] fn broken() { let = 1; }"),
            "production_syntax" => format!("fn broken() {{ let = 1; }}\n{before_source}"),
            "analysis_limit" => format!("fn deep() {{ let _ = {}0{}; }}\n{before_source}", "{".repeat(300), "}".repeat(300)),
            "valid_test" => before_source.replace("mod tests {", "mod tests {\n    #[test] fn boundary() { assert_eq!(discounted_total(100, 100), 90); }"),
            _ => before_source.replace("amount - 10", "amount - 11"),
        };
        // No new test exists in either production-only parse control; the inline
        // test module remains byte-identical, so attribution to it is invalid.
        if case != "test_syntax" && case != "valid_test" {
            assert!(after_source.ends_with(INLINE), "{case}");
        }
        if case == "analysis_limit" {
            assert!(
                ra_ap_syntax::SourceFile::parse(&after_source, ra_ap_syntax::Edition::CURRENT)
                    .errors()
                    .is_empty()
            );
        }
        std::fs::write(root.join("src/lib.rs"), after_source)?;
        if case == "valid_test" {
            let target = root.join("target").display().to_string();
            let tests = run_command(
                "cargo",
                Some(&root),
                &["test", "--offline", "--lib", "--target-dir", &target],
            )?;
            assert_success(&tests);
            assert!(
                String::from_utf8_lossy(&tests.stdout).contains("2 passed"),
                "{tests:?}"
            );
        }
        let after = repair_route_after(&root, &attempt_id);
        if case == "valid_test" {
            assert_success(&after);
            assert!(!String::from_utf8_lossy(&after.stderr).contains(HINT));
            let manifest = repair_route_manifest(&root, &attempt_id)?;
            assert_eq!(manifest["state"], "ready_to_finish", "{manifest}");
            assert_eq!(
                manifest["after"]["verdict"]["status"], "compliant",
                "{manifest}"
            );
            assert!(manifest.get("last_after_refusal").is_none(), "{manifest}");
            eprintln!("control {case}: two fixture tests passed, compliant finish, no refusal");
            std::fs::remove_dir_all(root)?;
            continue;
        }
        assert!(!after.status.success(), "{case}: {after:?}");
        let stderr = String::from_utf8_lossy(&after.stderr);
        let document: serde_json::Value =
            serde_json::from_slice(&after.stdout).map_err(|error| {
                format!(
                    "{case}: {error}; stderr={stderr}; stdout={}",
                    String::from_utf8_lossy(&after.stdout)
                )
            })?;
        assert_eq!(
            document["kind"], "repair_after_failure",
            "{case}: {document}; {stderr}"
        );
        let manifest = repair_route_manifest(&root, &attempt_id)?;
        assert_eq!(manifest["state"], "failed", "{case}: {manifest}");
        let recorded = manifest["last_after_refusal"]["reason"]
            .as_str()
            .ok_or("missing recorded refusal")?;
        let narration = document["narration"]
            .as_array()
            .ok_or("missing narration")?;
        let expected_hint = case != "production_edit";
        assert_eq!(
            narration.first().and_then(serde_json::Value::as_str) == Some(HINT),
            expected_hint,
            "{case}: {document}"
        );
        assert_eq!(stderr.contains(HINT), expected_hint, "{case}: {stderr}");
        assert_eq!(
            recorded.contains(RECORDED_HINT),
            expected_hint,
            "{case}: {recorded}"
        );
        assert!(
            !recorded
                .to_lowercase()
                .contains("the test written for this attempt does not parse"),
            "{case}: {recorded}"
        );
        for line in narration.iter().filter_map(serde_json::Value::as_str) {
            assert!(stderr.contains(line), "{case}: stderr omitted {line}");
            assert!(
                recorded
                    .to_lowercase()
                    .contains(&line.trim().trim_end_matches('.').to_lowercase()),
                "{case}: recording omitted {line}"
            );
        }
        let reason = if expected_hint {
            "`: unparseable)"
        } else {
            "`: production_edit)"
        };
        assert!(recorded.contains(reason), "{case}: {recorded}");
        assert!(
            recorded.contains("undo the refused changes"),
            "{case}: {recorded}"
        );
        assert!(recorded.contains("`core.autocrlf`"), "{case}: {recorded}");
        let status = repair_route_status(&root)?;
        assert!(
            status.to_string().contains(if expected_hint {
                RECORDED_HINT
            } else {
                "production_edit"
            }),
            "{case}: {status}"
        );
        eprintln!(
            "control {case}: observed terminal refusal across stderr, JSON, record and status"
        );
        std::fs::remove_dir_all(root)?;
    }
    Ok(())
}
