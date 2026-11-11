//! Public controls for dormant generator `.then` callbacks (#8286).
//!
//! A native promise invokes a `function*` callback only to create an
//! iterator; the callback body, and any assertion in it, never runs.
//!
//! Runtime truth (Node 22.23.4, `node --test`, one test per run, no skips):
//! every dormant body below passes with `return 31` in `src/total.ts`; every
//! control body fails there at its strict equality, and all pass with 30.

use super::{run_command, unique_temp_workspace};
use serde_json::Value;
use std::{fs, path::PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const PACKAGE: &str = r#"{
  "name": "ripr-generator-promise-callback-controls",
  "private": true,
  "type": "module",
  "scripts": {
    "test": "node --test tests/total.test.ts"
  }
}
"#;

const SOURCE: &str = "export async function total(): Promise<number> {\n  return 30;\n}\n";

const DIFF: &str = "diff --git a/src/total.ts b/src/total.ts\n--- a/src/total.ts\n+++ b/src/total.ts\n@@ -1,3 +1,3 @@\n export async function total(): Promise<number> {\n-  return 29;\n+  return 30;\n }\n";

struct Subject(PathBuf);

impl Subject {
    fn new(body: &str) -> TestResult<Self> {
        let root = unique_temp_workspace("typescript-generator-promise");
        fs::create_dir_all(std::env::temp_dir())?;
        fs::create_dir(&root)?;
        let subject = Self(root);
        fs::create_dir_all(subject.0.join("src"))?;
        fs::create_dir_all(subject.0.join("tests"))?;
        for (path, text) in [
            ("package.json", PACKAGE),
            ("ripr.toml", "[languages]\nenabled = [\"typescript\"]\n"),
            ("src/total.ts", SOURCE),
            ("change.diff", DIFF),
        ] {
            fs::write(subject.0.join(path), text)?;
        }
        fs::write(
            subject.0.join("tests/total.test.ts"),
            format!(
                "import test from 'node:test';\nimport assert from 'node:assert/strict';\nimport {{ total }} from '../src/total.ts';\ntest('total', async () => {{\n{body}\n}});\n"
            ),
        )?;
        Ok(subject)
    }

    fn check(&self, class: &str, kind: &str, strength: &str) -> TestResult {
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&self.0),
            &[
                "check",
                "--root",
                ".",
                "--diff",
                "change.diff",
                "--mode",
                "fast",
                "--format",
                "json",
            ],
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "nonempty intended subject: {report}");
        let finding = &findings[0];
        assert_eq!(finding["probe"]["file"], "src/total.ts");
        assert_eq!(finding["probe"]["line"], 2);
        assert_eq!(finding["probe"]["owner"], "typescript:src/total.ts::total");
        assert_eq!(finding["probe"]["family"], "return_value");
        assert_eq!(finding["classification"], class, "{finding}");
        assert_eq!(finding["related_tests_total"], 1);
        let related = finding["related_tests"]
            .as_array()
            .ok_or("missing related tests")?;
        assert_eq!(related.len(), 1);
        assert_eq!(related[0]["name"], "total");
        assert_eq!(related[0]["oracle_kind"], kind, "{finding}");
        assert_eq!(related[0]["oracle_strength"], strength, "{finding}");
        assert_eq!(finding["ripr"]["reach"]["state"], "yes");
        if kind == "unknown" {
            let evidence = finding["evidence"].as_array().ok_or("missing evidence")?;
            assert!(
                evidence
                    .iter()
                    .any(|line| line.as_str().is_some_and(|line| {
                        line.starts_with(
                            "test_assertion_admission: assertion_like_present_but_unresolved",
                        )
                    })),
                "withheld assertions must not become established absence: {finding}"
            );
        }
        Ok(())
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const CHECK: &str = "assert.strictEqual(sum, 30);";

/// The one-parameter form binds the settled value; the zero-parameter form
/// returns an inner ordinary `.then`, so both collector branches are covered.
fn then_body(lead: &str, head: &str, one_parameter: bool) -> String {
    if one_parameter {
        format!("{lead} total().then({head} {{ {CHECK} }});")
    } else {
        format!(
            "{lead} Promise.resolve().then({head} {{ return total().then((sum) => {{ {CHECK} }}); }});"
        )
    }
}

#[test]
fn dormant_generator_then_callbacks_cannot_confirm_the_changed_return() -> TestResult {
    for lead in ["return", "await"] {
        for (one, zero) in [
            ("function* (sum)", "function* ()"),
            ("async function* (sum)", "async function* ()"),
        ] {
            for body in [then_body(lead, one, true), then_body(lead, zero, false)] {
                Subject::new(&body)?
                    .check("static_unknown", "unknown", "unknown")
                    .map_err(|error| format!("{body}: {error}"))?;
            }
        }
    }
    Ok(())
}

#[test]
fn executing_then_callbacks_keep_credit() -> TestResult {
    for lead in ["return", "await"] {
        for (one, zero) in [
            ("function (sum)", "function ()"),
            ("async function (sum)", "async function ()"),
            ("(sum) =>", "() =>"),
        ] {
            for body in [then_body(lead, one, true), then_body(lead, zero, false)] {
                Subject::new(&body)?
                    .check("exposed", "exact_value", "strong")
                    .map_err(|error| format!("{body}: {error}"))?;
            }
        }
    }
    Ok(())
}

#[test]
fn an_independent_assertion_beside_a_dormant_generator_keeps_credit() -> TestResult {
    let dormant = then_body("await", "function* (sum)", true);
    for body in [
        format!("{dormant}\nassert.strictEqual(await total(), 30);"),
        format!("assert.strictEqual(await total(), 30);\n{dormant}"),
    ] {
        Subject::new(&body)?.check("exposed", "exact_value", "strong")?;
    }
    Ok(())
}
