//! Real consumer checks for the constructor-callback owner wording
//! (RIPR-SPEC-0243, #13588).

use super::{DeclaredHunkScratch, run_command, unique_temp_workspace};
use serde_json::Value;
use std::{fs, path::Path};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const CALLBACK_SOURCE: &str = "import { constructor } from './core';\n\nexport const ZodTransform = constructor('ZodTransform', (inst, def) => {\n  inst.parse = (payload: { issues: string[] }) => {\n    const start = payload.issues.length + 1;\n    return { start };\n  };\n});\n";
const CALLBACK_PATCH: &str = "diff --git a/src/schema.ts b/src/schema.ts\n--- a/src/schema.ts\n+++ b/src/schema.ts\n@@ -2,7 +2,7 @@\n \n export const ZodTransform = constructor('ZodTransform', (inst, def) => {\n   inst.parse = (payload: { issues: string[] }) => {\n-    const start = payload.issues.length;\n+    const start = payload.issues.length + 1;\n     return { start };\n   };\n });\n";
const CONSTANT_SOURCE: &str =
    "export const skip = Symbol('skip');\nexport const stop = Symbol('stop');\n";
const CONSTANT_PATCH: &str = "diff --git a/src/schema.ts b/src/schema.ts\n--- a/src/schema.ts\n+++ b/src/schema.ts\n@@ -1,2 +1,2 @@\n export const skip = Symbol('skip');\n-export const stop = skip;\n+export const stop = Symbol('stop');\n";

fn prepare(root: &Path, source: &str, patch: &str) -> TestResult {
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"typescript\"]\n",
    )?;
    fs::write(root.join("src/schema.ts"), source)?;
    fs::write(
        root.join("tests/other.test.ts"),
        "test('unrelated', () => { expect(1 + 1).toBe(2); });\n",
    )?;
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

fn only_finding(report: &Value) -> TestResult<&Value> {
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    assert_eq!(findings.len(), 1, "{report}");
    Ok(&findings[0])
}

#[test]
fn constructor_callback_change_gets_per_use_wording_and_stays_no_static_path() -> TestResult {
    let root = unique_temp_workspace("ts-constructor-callback");
    let _cleanup = DeclaredHunkScratch(root.clone());
    prepare(&root, CALLBACK_SOURCE, CALLBACK_PATCH)?;
    let report: Value = serde_json::from_str(&check(&root, "json")?)?;
    let finding = only_finding(&report)?;
    // Wording-only: the classification and actionability stay fail-closed.
    assert_eq!(finding["classification"], "no_static_path");
    assert_eq!(
        finding["preview_actionability"]["repair_packet_ready"],
        false
    );
    assert_eq!(
        finding["typescript_preview_card"]["repair_packet_ready"],
        false
    );
    let missing = finding["missing"].to_string();
    assert!(missing.contains("runs when a value built from `ZodTransform` is used"));
    assert!(
        !missing.contains("module-initializer observer"),
        "{missing}"
    );
    let step = finding["recommended_next_step"]
        .as_str()
        .ok_or("missing next step")?;
    assert!(
        step.contains("builds a value from `ZodTransform`"),
        "{step}"
    );
    assert!(step.contains("parse a value"), "{step}");
    assert!(!step.contains("expect(IMPORTED_CONST)"), "{step}");
    let human = check(&root, "human-full")?;
    assert!(!human.contains("module initializer observer"), "{human}");
    assert!(!human.contains("when safe"), "{human}");
    Ok(())
}

#[test]
fn plain_module_constant_keeps_the_module_initializer_advice() -> TestResult {
    let root = unique_temp_workspace("ts-module-constant-control");
    let _cleanup = DeclaredHunkScratch(root.clone());
    prepare(&root, CONSTANT_SOURCE, CONSTANT_PATCH)?;
    let report: Value = serde_json::from_str(&check(&root, "json")?)?;
    let finding = only_finding(&report)?;
    assert_eq!(finding["classification"], "no_static_path");
    assert!(
        finding["missing"]
            .to_string()
            .contains("module-initializer observer for `stop`")
    );
    let step = finding["recommended_next_step"]
        .as_str()
        .ok_or("missing next step")?;
    assert!(step.contains("expect(IMPORTED_CONST).toBe"), "{step}");
    assert!(!step.contains("builds a value"), "{step}");
    Ok(())
}
