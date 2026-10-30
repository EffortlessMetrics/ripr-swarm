//! CLI regression for tsd/vitest type-test files (#8536).
//!
//! `*.test-d.ts` / `*.spec-d.ts` must be classified as tests so an edit there
//! does not emit a false `no_static_path` finding. Exact stem suffixes only:
//! a production name like `foo.test-data.ts` stays a production owner.
//!
//! Inline temp repo; no fixture goldens. A build without `lang-typescript`
//! cannot observe this adapter path.

#![cfg(feature = "lang-typescript")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct FixtureGuard {
    root: PathBuf,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique_root(label: &str) -> Result<PathBuf, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-type-test-stems-{label}-{}-{call}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).map_err(|error| format!("mkdir {}: {error}", root.display()))?;
    Ok(root)
}

fn write_file(root: &Path, relative: &str, body: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    }
    fs::write(&path, body).map_err(|error| format!("write {}: {error}", path.display()))
}

fn unified_diff(path: &str, old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut out = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
        old_lines.len(),
        new_lines.len()
    );
    for line in &old_lines {
        out.push('-');
        out.push_str(line);
        out.push('\n');
    }
    for line in &new_lines {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn write_js_workspace(
    root: &Path,
    changed_relative: &str,
    old: &str,
    new: &str,
) -> Result<(), String> {
    write_file(
        root,
        "ripr.toml",
        "[languages]\nenabled = [\"typescript\"]\n",
    )?;
    write_file(
        root,
        "package.json",
        "{\"name\":\"type-test-stems\",\"type\":\"module\"}\n",
    )?;
    write_file(
        root,
        "index.js",
        "export default function pLimit(concurrency) {\n  return { activeCount: concurrency };\n}\n",
    )?;
    write_file(
        root,
        "test.js",
        "import test from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport pLimit from \"./index.js\";\n\ntest(\"concurrency\", () => {\n  assert.equal(pLimit(1).activeCount, 1);\n});\n",
    )?;
    write_file(root, changed_relative, new)?;
    write_file(
        root,
        "change.diff",
        &unified_diff(changed_relative, old, new),
    )?;
    Ok(())
}

fn run_check(root: &Path) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--quiet", "--format", "json", "--root"])
        .arg(root)
        .arg("--diff")
        .arg(root.join("change.diff"))
        .output()
        .map_err(|error| format!("spawn ripr check: {error}"))
}

fn parse_check_json(output: &Output) -> Result<serde_json::Value, String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    serde_json::from_str(stdout.trim()).map_err(|error| {
        format!("check output is not JSON: {error}\nstdout: {stdout}\nstderr: {stderr}")
    })
}

fn typescript_changed_files(document: &serde_json::Value) -> Result<u64, String> {
    let counts = document
        .pointer("/summary/changed_files_by_language")
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("missing changed_files_by_language: {document}"))?;
    counts
        .iter()
        .find(|entry| entry.get("language").and_then(|value| value.as_str()) == Some("typescript"))
        .and_then(|entry| entry.get("files").and_then(|value| value.as_u64()))
        .ok_or_else(|| format!("typescript adapter did not count the changed file: {document}"))
}

fn findings(document: &serde_json::Value) -> Result<&Vec<serde_json::Value>, String> {
    document
        .get("findings")
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("missing findings array: {document}"))
}

const TYPE_TEST_OLD: &str = "\
import pLimit from \"./index.js\";

declare function expectType<T>(value: T): void;

const limit = pLimit(1);
expectType<number>(limit.activeCount);
";

const TYPE_TEST_NEW: &str = "\
import pLimit from \"./index.js\";

declare function expectType<T>(value: T): void;

const limit = pLimit(2);
expectType<number>(limit.activeCount);
";

const TEST_DATA_OLD: &str = "\
export function loadTestData(): number {
  return 1;
}
";

const TEST_DATA_NEW: &str = "\
export function loadTestData(): number {
  return 2;
}
";

#[test]
fn test_d_ts_edit_is_a_changed_test_file_with_no_finding() -> Result<(), String> {
    type_test_edit_is_a_changed_test_file_with_no_finding("index.test-d.ts")
}

#[test]
fn spec_d_ts_edit_is_a_changed_test_file_with_no_finding() -> Result<(), String> {
    type_test_edit_is_a_changed_test_file_with_no_finding("index.spec-d.ts")
}

fn type_test_edit_is_a_changed_test_file_with_no_finding(relative: &str) -> Result<(), String> {
    let root = unique_root(relative.replace('.', "-").as_str())?;
    let _guard = FixtureGuard { root: root.clone() };
    write_js_workspace(&root, relative, TYPE_TEST_OLD, TYPE_TEST_NEW)?;
    let output = run_check(&root)?;
    let document = parse_check_json(&output)?;
    let changed = typescript_changed_files(&document)?;
    if changed == 0 {
        return Err(format!(
            "setup: the type-test file must still count as a changed TypeScript file: {document}"
        ));
    }
    let findings = findings(&document)?;
    if !findings.is_empty() {
        return Err(format!(
            "a {relative} type-test edit must not emit findings, got {findings:?}"
        ));
    }
    let no_static_path = document
        .pointer("/summary/no_static_path")
        .and_then(|value| value.as_u64())
        .unwrap_or(u64::MAX);
    if no_static_path != 0 {
        return Err(format!(
            "a {relative} type-test edit must not increment no_static_path: {document}"
        ));
    }
    Ok(())
}

#[test]
fn test_data_ts_stays_production_and_emits_a_finding() -> Result<(), String> {
    let root = unique_root("test-data")?;
    let _guard = FixtureGuard { root: root.clone() };
    write_js_workspace(&root, "foo.test-data.ts", TEST_DATA_OLD, TEST_DATA_NEW)?;
    let output = run_check(&root)?;
    let document = parse_check_json(&output)?;
    let changed = typescript_changed_files(&document)?;
    if changed == 0 {
        return Err(format!(
            "setup: foo.test-data.ts must count as a changed TypeScript file: {document}"
        ));
    }
    let findings = findings(&document)?;
    if findings.is_empty() {
        return Err(format!(
            "a production foo.test-data.ts edit must still emit a finding: {document}"
        ));
    }
    let classifications: Vec<&str> = findings
        .iter()
        .filter_map(|finding| {
            finding
                .get("classification")
                .and_then(|value| value.as_str())
        })
        .collect();
    if !classifications.contains(&"no_static_path") {
        return Err(format!(
            "setup: expected a no_static_path production finding, got {classifications:?} in {document}"
        ));
    }
    Ok(())
}
