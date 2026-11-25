//! #15026: a single-file crate relates every inline test to a changed owner
//! by file location. Within that tier, tests that call the owner's wrapper
//! must fill the eight-row related-test window ahead of tests that only share
//! the file and sort earlier by name.

use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
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
    let output = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == 4)
        .collect())
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
