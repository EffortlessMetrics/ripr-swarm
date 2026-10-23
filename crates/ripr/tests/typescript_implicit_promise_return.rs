//! RIPR-SPEC-0246: an expression-bodied test arrow returns its promise;
//! blocks, discarded callbacks, and unsupported promise expressions do not.

#![cfg(feature = "lang-typescript")]

use ripr::app::check_workspace_with_config;
use ripr::config::RiprConfig;
use ripr::domain::{LanguageId, OracleKind};
use ripr::{CheckInput, ExposureClass, Finding, ProbeFamily};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let path = parent.join(format!(
            "ripr-ts-implicit-promise-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir(scratch.0.join("tests")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn finding(extension: &str, runner: &str, callback: &str) -> Result<Finding, String> {
    let scratch = Scratch::create()?;
    let source = "export async function total() {\n  return 30;\n}\n";
    let imports = if runner == "node" {
        "import test from 'node:test';\nimport assert from 'node:assert/strict';"
    } else {
        "import { test, expect } from 'vitest';"
    };
    let test = format!(
        "{imports}\nimport {{ total }} from '../src/total.{extension}';\ntest('total', {callback});\n"
    );
    let diff = format!(
        "diff --git a/src/total.{extension} b/src/total.{extension}\n--- a/src/total.{extension}\n+++ b/src/total.{extension}\n@@ -1,3 +1,3 @@\n export async function total() {{\n-  return 20;\n+  return 30;\n }}\n"
    );
    let config_text = "[languages]\nenabled = ['typescript']\n";
    for (path, contents) in [
        ("ripr.toml".to_string(), config_text.to_string()),
        (format!("src/total.{extension}"), source.to_string()),
        (format!("tests/total.test.{extension}"), test),
        ("change.diff".to_string(), diff),
    ] {
        std::fs::write(scratch.0.join(path), contents).map_err(|error| error.to_string())?;
    }
    let mut config = RiprConfig::default();
    config.languages.enabled = vec![LanguageId::TypeScript];
    let report = check_workspace_with_config(
        CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            ..CheckInput::default()
        },
        &config,
    )?;
    assert_eq!(
        report.findings.len(),
        1,
        "nonempty, single changed owner: {report:?}"
    );
    let finding = report
        .findings
        .into_iter()
        .next()
        .ok_or("missing finding")?;
    assert_eq!(finding.probe.family, ProbeFamily::ReturnValue);
    assert_eq!(finding.probe.location.line, 2);
    Ok(finding)
}

#[test]
fn implicit_return_keeps_the_supported_then_discriminator() -> Result<(), String> {
    for extension in ["ts", "js"] {
        for runner in ["vitest", "node"] {
            let assertion = if runner == "node" {
                "assert.strictEqual(value, 30)"
            } else {
                "expect(value).toBe(30)"
            };
            for callback in [
                format!("() => total().then(value => {{ {assertion}; }})"),
                format!("async () => (total().then(value => {{ {assertion}; }}))"),
                format!("async () => {{ return total().then(value => {{ {assertion}; }}); }}"),
                format!("async () => {{ await total().then(value => {{ {assertion}; }}); }}"),
            ] {
                let got = finding(extension, runner, &callback)?;
                assert_eq!(
                    got.class,
                    ExposureClass::Exposed,
                    "{extension} {runner} {callback}: {got:?}"
                );
                assert_eq!(got.related_tests.len(), 1, "test registration must resolve");
                assert_eq!(got.related_tests[0].oracle_kind, OracleKind::ExactValue);
            }
        }
    }
    Ok(())
}

#[test]
fn only_the_returned_supported_expression_can_supply_credit() -> Result<(), String> {
    for extension in ["ts", "js"] {
        for callback in [
            "() => { total().then(value => { expect(value).toBe(30); }); }",
            "() => void total().then(value => { expect(value).toBe(30); })",
            "() => (total().then(value => { expect(value).toBe(30); }), undefined)",
            "() => (() => total().then(value => { expect(value).toBe(30); }))",
            "() => [1].forEach(n => total().then(value => { expect(value).toBe(30); }))",
            "() => [1].forEach(async n => { await total().then(value => { expect(value).toBe(30); }); })",
            "() => total().then(value => { expect(value).toBe(30); }, () => {})",
            "() => total().then(value => { expect(value).toBe(30); }).catch(() => {})",
            "() => total().then(() => 30).then(value => { expect(value).toBe(30); })",
            "() => total().then(function* (value) { expect(value).toBe(30); })",
            "() => run(() => total().then(value => { expect(value).toBe(30); }))",
        ] {
            let got = finding(extension, "vitest", callback)?;
            assert_ne!(
                got.class,
                ExposureClass::Exposed,
                "{extension} {callback}: {got:?}"
            );
            assert_eq!(got.related_tests.len(), 1, "test registration must resolve");
            assert_eq!(got.related_tests[0].oracle_kind, OracleKind::Unknown);
        }
    }
    Ok(())
}

#[test]
fn inline_receiver_bindings_cannot_supply_promise_credit() -> Result<(), String> {
    for extension in ["ts", "js"] {
        for runner in ["vitest", "node"] {
            let assertion = if runner == "node" {
                "assert.strictEqual(total(), 30)"
            } else {
                "expect(total()).toBe(30)"
            };
            for parameters in ["()", "(value)"] {
                for declaration in [
                    "const total = () => 30;",
                    "let total = () => 30;",
                    "var total = () => 30;",
                    "void 0; const total = () => 30;",
                    "/* local */ const total = () => 30;",
                    "const unused = 0, total = () => 30;",
                    "function total() { return 30; }",
                ] {
                    for callback in [
                        format!(
                            "() => total().then({parameters} => {{ {declaration} {assertion}; }})"
                        ),
                        format!(
                            "() => {{ return total().then({parameters} => {{ {declaration} {assertion}; }}); }}"
                        ),
                        format!(
                            "async () => {{ await total().then({parameters} => {{ {declaration} {assertion}; }}); }}"
                        ),
                    ] {
                        let got = finding(extension, runner, &callback)?;
                        assert_ne!(
                            got.class,
                            ExposureClass::Exposed,
                            "{extension} {runner} {callback}: {got:?}"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn independent_nested_bindings_keep_promise_credit() -> Result<(), String> {
    for extension in ["ts", "js"] {
        for runner in ["vitest", "node"] {
            let assertion = if runner == "node" {
                "assert.strictEqual(value, 30)"
            } else {
                "expect(value).toBe(30)"
            };
            for declaration in [
                "const total = () => 0;",
                "const unused = 0, total = () => 0;",
                "\nconst total = () => 0;\n",
            ] {
                for callback in [
                    format!(
                        "() => {{ {{ {declaration} }} return total().then(value => {{ {assertion}; }}); }}"
                    ),
                    format!("() => total().then(value => {{ {{ {declaration} }} {assertion}; }})"),
                    format!(
                        "() => {{ return total().then(value => {{ {{ {declaration} }} {assertion}; }}); }}"
                    ),
                ] {
                    let got = finding(extension, runner, &callback)?;
                    assert_eq!(
                        got.class,
                        ExposureClass::Exposed,
                        "{extension} {runner} {callback}: {got:?}"
                    );
                    assert_eq!(got.related_tests.len(), 1);
                    assert_eq!(got.related_tests[0].oracle_kind, OracleKind::ExactValue);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn nested_binding_used_inside_its_scope_withholds_credit() -> Result<(), String> {
    for extension in ["ts", "js"] {
        for callback in [
            "() => total().then(value => { { const total = () => 30; expect(total()).toBe(30); } })",
            "() => total().then(() => { { const total = () => 30; expect(total()).toBe(30); } })",
        ] {
            let got = finding(extension, "vitest", callback)?;
            assert_ne!(
                got.class,
                ExposureClass::Exposed,
                "{extension} {callback}: {got:?}"
            );
        }
    }
    Ok(())
}
