//! RIPR-SPEC-0243 / #12949: an array length does not observe a changed
//! appended value, for explicit Vitest and Jest globals imports.

#![cfg(feature = "lang-typescript")]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const OLD: &str = "buf.push(msg);";
const UPPER: &str = "buf.push(msg.toUpperCase());";

struct Workspace(PathBuf);

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_file(root: &Path, relative: &str, contents: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    }
    fs::write(&path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

impl Workspace {
    fn new(framework: &str, new: &str, body: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
        let root = parent.join(format!(
            "ripr-ts-push-argument-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| format!("mkdir {}: {error}", root.display()))?;
        let workspace = Self(root);
        let root = &workspace.0;
        write_file(root, "ripr.toml", "[languages]\nenabled = ['typescript']\n")?;
        let package = if framework == "@jest/globals" {
            r#"{"type":"module","devDependencies":{"jest":"*","@jest/globals":"*"}}"#
        } else {
            r#"{"type":"module","devDependencies":{"vitest":"*"}}"#
        };
        write_file(root, "package.json", package)?;
        write_file(
            root,
            "src/log.ts",
            &format!("export function logLine(buf: string[], msg: string): void {{\n  {new}\n}}\n"),
        )?;
        write_file(
            root,
            "test/log.test.ts",
            &format!(
                "import {{ test, expect }} from '{framework}';\nimport {{ logLine }} from '../src/log';\ntest('log', () => {{\n{body}\n}});\n"
            ),
        )?;
        write_file(
            root,
            "change.diff",
            &format!(
                "diff --git a/src/log.ts b/src/log.ts\n--- a/src/log.ts\n+++ b/src/log.ts\n@@ -1,3 +1,3 @@\n export function logLine(buf: string[], msg: string): void {{\n-  {OLD}\n+  {new}\n }}\n"
            ),
        )?;
        Ok(workspace)
    }

    fn check(&self, format: &str) -> Result<String, String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(["check", "--quiet", "--format", format, "--root"])
            .arg(&self.0)
            .arg("--diff")
            .arg(self.0.join("change.diff"))
            .output()
            .map_err(|error| format!("spawn ripr check: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr check exited {}:\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        String::from_utf8(output.stdout).map_err(|error| error.to_string())
    }

    fn finding(&self) -> Result<Value, String> {
        let report: Value =
            serde_json::from_str(&self.check("json")?).map_err(|error| error.to_string())?;
        assert_eq!(report["analysis_outcome"]["analysis_complete"], true);
        let findings = report["findings"]
            .as_array()
            .ok_or_else(|| format!("missing findings: {report}"))?;
        assert_eq!(
            findings.len(),
            1,
            "one changed owner must be analyzed: {report}"
        );
        let finding = findings.first().ok_or("missing changed-owner finding")?;
        assert_eq!(finding["probe"]["family"], "side_effect", "{finding}");
        assert_eq!(finding["probe"]["file"], "src/log.ts", "{finding}");
        assert_eq!(finding["probe"]["line"], 2, "{finding}");
        assert_eq!(finding["related_tests_total"], 1, "{finding}");
        assert_eq!(
            finding["related_tests"][0]["relation_reason"], "direct_owner_call",
            "{finding}"
        );
        assert_eq!(
            finding["related_tests"][0]["oracle_strength"], "strong",
            "extraction still sees an exact assertion: {finding}"
        );
        Ok(finding.clone())
    }
}

#[test]
fn length_only_reports_the_missing_appended_value_for_both_frameworks() -> Result<(), String> {
    for framework in ["@jest/globals", "vitest"] {
        let workspace = Workspace::new(
            framework,
            UPPER,
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);",
        )?;
        let finding = workspace.finding()?;
        assert_eq!(
            finding["classification"], "weakly_exposed",
            "{framework}: {finding}"
        );
        assert_eq!(finding["ripr"]["observe"]["state"], "weak", "{finding}");
        assert_eq!(
            finding["ripr"]["discriminate"]["state"], "weak",
            "{finding}"
        );
        assert!(
            finding["confidence_basis"]
                .as_array()
                .ok_or("missing confidence basis")?
                .iter()
                .any(|cap| cap["code"] == "observation_not_confirmed"),
            "{finding}"
        );
        assert!(
            finding["recommended_next_step"]
                .as_str()
                .is_some_and(|next| next.contains("appended element")),
            "{finding}"
        );
        let human = workspace.check("human-full")?;
        assert!(human.contains("appended element"), "{framework}: {human}");
    }
    Ok(())
}

#[test]
fn unrelated_and_nondiscriminating_values_do_not_restore_credit() -> Result<(), String> {
    for framework in ["@jest/globals", "vitest"] {
        for body in [
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);\nexpect(1).toBe(1);",
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b[1]).toBe('HI');",
            "const b: string[] = [];\nlogLine(b, 'HI');\nexpect(b[0]).toBe('HI');",
            r"const b: string[] = []; logLine(b, '\\nhi'); expect(b[0]).toBe('\nhi');",
        ] {
            let finding = Workspace::new(framework, UPPER, body)?.finding()?;
            assert_eq!(
                finding["classification"], "weakly_exposed",
                "{framework}: {body}\n{finding}"
            );
        }
    }
    Ok(())
}

#[test]
fn discriminating_indexed_values_keep_credit_for_both_frameworks() -> Result<(), String> {
    for framework in ["@jest/globals", "vitest"] {
        for body in [
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b[0]).toBe('HI');",
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);\nexpect(b[0]).toBe('HI');",
            r"const b: string[] = []; logLine(b, 'a\n'); expect(b[0]).toBe('A\n');",
            r"const b: string[] = []; logLine(b, '\x61'); expect(b[0]).toBe('\x41');",
        ] {
            let finding = Workspace::new(framework, UPPER, body)?.finding()?;
            assert_eq!(
                finding["classification"], "exposed",
                "{framework}: {body}\n{finding}"
            );
        }
    }
    Ok(())
}

#[test]
fn framework_registration_identity_keeps_the_extension_bounded() -> Result<(), String> {
    let body = "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(1);";
    for (framework, extra_import, registration, expected) in [
        (
            "@jest/globals",
            "import { test as run } from 'vitest';",
            "run",
            "exposed",
        ),
        (
            "@jest/globals",
            "import { test as run } from 'vitest';",
            "test",
            "weakly_exposed",
        ),
        ("@jest/globals", "", "test.only", "exposed"),
        ("vitest", "", "test.only", "weakly_exposed"),
        (
            "vitest",
            "import { test as run } from 'vitest';",
            "run",
            "weakly_exposed",
        ),
    ] {
        let workspace = Workspace::new(framework, UPPER, body)?;
        write_file(
            &workspace.0,
            "test/log.test.ts",
            &format!(
                "import {{ test, expect }} from '{framework}';\n{extra_import}\nimport {{ logLine }} from '../src/log';\n{registration}('log', () => {{\n{body}\n}});\n"
            ),
        )?;
        let finding = workspace.finding()?;
        assert_eq!(
            finding["classification"], expected,
            "{framework} / {registration}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn changed_cardinality_keeps_length_credit_for_both_frameworks() -> Result<(), String> {
    for framework in ["@jest/globals", "vitest"] {
        let finding = Workspace::new(
            framework,
            "buf.push(msg, msg);",
            "const b: string[] = [];\nlogLine(b, 'hi');\nexpect(b.length).toBe(2);",
        )?
        .finding()?;
        assert_eq!(
            finding["classification"], "exposed",
            "{framework}: {finding}"
        );
    }
    Ok(())
}
