//! Real consumer checks for private-owner uncertainty (RIPR-SPEC-0243).

use super::{DeclaredHunkScratch, run_command, unique_temp_workspace};
use serde_json::Value;
use std::{fs, path::Path};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn prepare(root: &Path, source: &str, patch: &str) -> TestResult {
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"typescript\"]\n",
    )?;
    fs::write(root.join("src/counter.ts"), source)?;
    fs::write(root.join("change.diff"), patch)?;
    Ok(())
}

fn check(root: &Path, format: &str) -> TestResult<String> {
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        &[
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--mode",
            "fast",
            "--format",
            format,
        ],
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

#[test]
fn private_method_cli_output_keeps_unknown_reach_without_invented_test_evidence() -> TestResult {
    for (key, modifier, receiver) in [
        ("#limit", "", "this"),
        ("#limit", "static ", "Counter"),
        ("#调整", "", "this"),
    ] {
        let source = format!(
            "export class Counter {{\n  {modifier}{key}(value: number): number {{\n    return value >= 50 ? 55 : 0;\n  }}\n  entry(value: number): number {{ return {receiver}.{key}(value); }}\n}}\n"
        );
        let patch = format!(
            "diff --git a/src/counter.ts b/src/counter.ts\n--- a/src/counter.ts\n+++ b/src/counter.ts\n@@ -1,6 +1,6 @@\n export class Counter {{\n   {modifier}{key}(value: number): number {{\n-    return value >= 61 ? 55 : 0;\n+    return value >= 50 ? 55 : 0;\n   }}\n   entry(value: number): number {{ return {receiver}.{key}(value); }}\n }}\n"
        );
        for mode in ["none", "public", "masked"] {
            let root = unique_temp_workspace("private-method-output");
            let _cleanup = DeclaredHunkScratch(root.clone());
            prepare(&root, &source, &patch)?;
            if mode == "public" {
                fs::write(
                    root.join("tests/counter.test.ts"),
                    "import { Counter } from '../src/counter';\ntest('entry', () => { const counter = new Counter(); expect(counter.entry(60)).toBe(55); });\n",
                )?;
            } else if mode == "masked" {
                fs::write(
                    root.join("tests/counter.test.ts"),
                    format!(
                        "test('text only', () => {{ const text = 'counter.{key}(60)'; /* counter.{key}(60) */ expect(text).toBe('counter.{key}(60)'); }});\n"
                    ),
                )?;
            }
            let report: Value = serde_json::from_str(&check(&root, "json")?)?;
            let findings = report["findings"].as_array().ok_or("missing findings")?;
            assert_eq!(findings.len(), 1, "{key}/{modifier}/{mode}: {report}");
            let finding = &findings[0];
            assert_eq!(finding["classification"], "static_unknown");
            for stage in ["reach", "observe", "discriminate"] {
                assert_eq!(finding["ripr"][stage]["state"], "unknown");
            }
            assert_eq!(finding["related_tests_total"], 0);
            assert_eq!(
                finding["probe"]["owner"],
                format!("typescript:src/counter.ts::{key}")
            );
            let card = &finding["typescript_preview_card"];
            assert_eq!(card["owner"], key);
            assert_eq!(card["related_test"], Value::Null);
            assert_eq!(card["repair_packet_ready"], false);
            let suggestion = card["suggested_assertion_shape"]
                .as_str()
                .ok_or("missing suggestion")?;
            assert!(!suggestion.starts_with("Add "));
            assert!(suggestion.contains("reach"));
            let actionability = &finding["preview_actionability"];
            assert_eq!(
                actionability["actionability_category"],
                "private_member_reach_unknown"
            );
            assert_eq!(actionability["repair_packet_ready"], false);
            let human = check(&root, "human-full")?;
            let id = finding["id"].as_str().ok_or("missing finding id")?;
            let explain = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&root),
                &[
                    "explain",
                    "--root",
                    ".",
                    "--diff",
                    "change.diff",
                    "--mode",
                    "fast",
                    id,
                ],
            )?;
            assert!(explain.status.success(), "{explain:?}");
            for rendered in [human, String::from_utf8(explain.stdout)?] {
                assert!(
                    rendered.contains("could not establish whether a test reaches"),
                    "{rendered}"
                );
                assert!(
                    !rendered.contains("a candidate test path exists"),
                    "{rendered}"
                );
                assert!(
                    !rendered.contains("related-test link is heuristic-only"),
                    "{rendered}"
                );
                assert!(!rendered.contains("only matched by name"), "{rendered}");
            }
        }
    }
    Ok(())
}

#[test]
fn deleted_private_method_body_has_no_candidate_preview_card() -> TestResult {
    let root = unique_temp_workspace("private-method-deletion-output");
    let _cleanup = DeclaredHunkScratch(root.clone());
    prepare(
        &root,
        "export class Counter {\n  #limit(value: number): number {\n    return value;\n  }\n}\n",
        "diff --git a/src/counter.ts b/src/counter.ts\n--- a/src/counter.ts\n+++ b/src/counter.ts\n@@ -1,6 +1,5 @@\n export class Counter {\n   #limit(value: number): number {\n-    if (value >= 50) return value - 5;\n     return value;\n   }\n }\n",
    )?;
    let report: Value = serde_json::from_str(&check(&root, "json")?)?;
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    assert_eq!(findings.len(), 1, "{report}");
    let finding = &findings[0];
    assert_eq!(finding["classification"], "static_unknown");
    assert_eq!(finding["source_currentness"], "base_deleted");
    assert_eq!(finding["related_tests_total"], 0);
    assert_eq!(finding["typescript_preview_card"], Value::Null);
    Ok(())
}
