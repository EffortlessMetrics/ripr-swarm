//! #15026: a single-file crate relates every inline test to a changed owner
//! by file location. Within that tier, tests that call the owner's wrapper
//! must fill the eight-row related-test window ahead of tests that only share
//! the file and sort earlier by name.

use ripr::app::check_workspace_with_config;
use ripr::config::RiprConfig;
use ripr::domain::LanguageId;
use ripr::{CheckInput, Finding, Mode, OutputFormat};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-related-window-near-owner-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let target = self.0.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, content).map_err(|error| error.to_string())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str =
    "[package]\nname = \"sim\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n";

/// The owner has two reached callers (`score` and `score_twice`), so the
/// bounded helper chain declines and the wrapper tests stay `same_test_file`
/// rows, like strsim's `jaro_*` tests over `generic_jaro`.
fn source(owner_op: &str) -> String {
    let mut text = format!(
        "pub fn generic_score(a: &[u8], b: &[u8]) -> usize {{\n    let mut n = 0;\n    for (x, y) in a.iter().zip(b) {{\n        if x {owner_op} y {{\n            n += 1;\n        }}\n    }}\n    n\n}}\n\npub fn score(a: &str, b: &str) -> usize {{\n    generic_score(a.as_bytes(), b.as_bytes())\n}}\n\npub fn score_twice(a: &str) -> usize {{\n    2 * generic_score(a.as_bytes(), a.as_bytes())\n}}\n\npub fn bump(value: usize) -> usize {{\n    value + 1\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn generic_score_direct() {{\n        assert_eq!(generic_score(b\"ab\", b\"ab\"), 2);\n    }}\n"
    );
    for index in 0..8 {
        text.push_str(&format!(
            "\n    #[test]\n    fn bump_case_{index}() {{\n        assert_eq!(bump({index}), {});\n    }}\n",
            index + 1
        ));
    }
    for index in 0..4 {
        text.push_str(&format!(
            "\n    #[test]\n    fn score_case_{index}() {{\n        assert_eq!(score(\"a\", \"a\"), 1);\n    }}\n"
        ));
    }
    text.push_str("\n    #[test]\n    fn score_twice_case() {\n        assert_eq!(score_twice(\"a\"), 2);\n    }\n}\n");
    text
}

fn findings() -> Result<Vec<Finding>, String> {
    let scratch = Scratch::create()?;
    scratch.write("Cargo.toml", MANIFEST)?;
    let after = source("<");
    scratch.write("src/lib.rs", &after)?;
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,1 +4,1 @@\n-        if x <= y {\n+        if x < y {\n";
    scratch.write("change.diff", diff)?;
    let output = analyze_change(&scratch, LanguageId::Rust)?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == 4)
        .collect())
}

fn analyze_change(scratch: &Scratch, language: LanguageId) -> Result<ripr::CheckOutput, String> {
    let mut config = RiprConfig::default();
    config.languages.enabled = vec![language];
    check_workspace_with_config(
        CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        },
        &config,
    )
}

#[test]
fn wrapper_tests_fill_the_window_before_file_only_neighbours() -> Result<(), String> {
    let found = findings()?;
    let finding = found
        .first()
        .ok_or_else(|| "setup: no finding on the changed predicate".to_string())?;
    let names: Vec<&str> = finding
        .related_tests
        .iter()
        .map(|test| test.name.as_str())
        .collect();
    assert!(
        finding.related_tests_total() > names.len(),
        "setup: the window is capped: {names:?}"
    );
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.name.starts_with("bump_case_")),
        "setup: file-only rows still fill free slots: {names:?}"
    );
    let first_bump = names
        .iter()
        .position(|name| name.starts_with("bump_case_"))
        .unwrap_or(names.len());
    let wrapper_rows: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.starts_with("score_"))
        .map(|(position, _)| position)
        .collect();
    assert_eq!(
        wrapper_rows.len(),
        5,
        "every wrapper test is in the window: {names:?}"
    );
    assert!(
        wrapper_rows.iter().all(|position| *position < first_bump),
        "wrapper tests lead the file-only rows: {names:?}"
    );
    Ok(())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn javascript_boundary_headline_follows_the_asserted_input() -> Result<(), String> {
    let scratch = Scratch::create()?;
    scratch.write("package.json", r#"{"type":"module"}"#)?;
    scratch.write(
        "index.js",
        "export function admitted(n) {\n  return n > 10;\n}\n",
    )?;
    scratch.write("change.diff", "diff --git a/index.js b/index.js\n--- a/index.js\n+++ b/index.js\n@@ -2 +2 @@\n-  return n >= 10;\n+  return n > 10;\n")?;
    // Exchange only the inputs: the credited test must change even though the
    // names, locations, call paths and oracle strengths are identical.
    for (first, second, expected) in [(20, 10, "second"), (10, 20, "first")] {
        scratch.write(
            "index.test.js",
            &format!(
                r#"import {{test}} from 'node:test';
import assert from 'node:assert/strict';
import {{admitted}} from './index.js';
test('first', () => {{
  assert.equal(admitted({first}), true);
}});
test('second', () => {{
  assert.equal(admitted({second}), true);
}});
"#
            ),
        )?;
        let output = analyze_change(&scratch, LanguageId::TypeScript)?;
        let [finding] = output.findings.as_slice() else {
            return Err(format!(
                "expected one changed predicate, got {}",
                output.findings.len()
            ));
        };
        assert_eq!(finding.related_tests.len(), 2);
        assert_eq!(
            finding.headline_related_test().map(|row| row.name.as_str()),
            Some(expected)
        );
    }
    Ok(())
}

/// The first row calls itertools.count; only the second calls the changed owner.
#[cfg(feature = "lang-python")]
#[test]
fn python_headline_uses_the_credited_count_call() -> Result<(), String> {
    let scratch = Scratch::create()?;
    scratch.write(
        "seq.py",
        "def count(seq):\n    return sum(2 for _ in seq)\n",
    )?;
    scratch.write("change.diff", "diff --git a/seq.py b/seq.py\n--- a/seq.py\n+++ b/seq.py\n@@ -2 +2 @@\n-    return sum(1 for _ in seq)\n+    return sum(2 for _ in seq)\n")?;
    scratch.write(
        "sandbox/tests/test_core.py",
        "from itertools import count\n\ndef test_unzip():\n    assert next(count()) == 0\n",
    )?;
    scratch.write(
        "tests/test_seq.py",
        "from seq import count\n\ndef test_count():\n    assert count(iter([1, 2, 3, 4])) == 4\n",
    )?;
    let output = analyze_change(&scratch, LanguageId::Python)?;
    let [finding] = output.findings.as_slice() else {
        return Err(format!(
            "expected one changed generator increment, got {}",
            output.findings.len()
        ));
    };
    let names: Vec<_> = finding
        .related_tests
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, ["test_unzip", "test_count"]);
    assert_eq!(
        finding.headline_related_test().map(|row| row.name.as_str()),
        Some("test_count")
    );
    Ok(())
}
