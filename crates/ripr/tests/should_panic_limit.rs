//! Public-API controls for `#[should_panic]` panic outcomes (RIPR-SPEC-0240).
//!
//! A `#[should_panic]` test with no assertion that calls the owner reads as
//! `static_unknown` when the owner can panic at or after the changed line:
//! the panic outcome is an oracle ripr does not resolve. A change after the
//! owner's last panic, an `expected` message the owner raises only before
//! the change or never, a test without `#[should_panic]`, and a
//! `#[should_panic]` test that carries an assertion keep the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with `row` on lines 1-6 and the test's attributes and body.
fn source(attrs: &str, body: &str) -> String {
    with_owner(
        "pub fn row(index: usize) -> usize {
    if index >= 100 {",
        attrs,
        body,
    )
}

/// `head` (lines 1-2 of the owner) before lines 3-6 of `row`.
fn with_owner(head: &str, attrs: &str, body: &str) -> String {
    format!(
        "{head}
        panic!(\"index past the end\");
    }}
    index / 4
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    {attrs}
    fn rejects_index_one_hundred() {{
        {body}
    }}
}}
"
    )
}

/// The guard on line 2 changed.
const GUARD_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn row(index: usize) -> usize {
-    if index > 99 {
+    if index >= 100 {
         panic!(\"index past the end\");
";

/// The division on line 5, after the owner's last panic, changed.
const TAIL_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -4,3 +4,3 @@
     }
-    index >> 2
+    index / 4
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-should-panic-limit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"should-panic\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The finding on `line`, which must be the only one there.
fn finding_on(lib: &str, diff: &str, line: usize) -> Result<Finding, String> {
    let findings = TempRepo::create(lib, diff)?.findings()?;
    let mut on_line = findings
        .iter()
        .filter(|finding| finding.probe.location.line == line);
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one finding on line {line}, got {other:?} in {findings:?}"
        )),
    }
}

#[test]
fn a_should_panic_test_on_a_changed_panic_guard_reads_as_static_unknown() -> Result<(), String> {
    for attrs in [
        "#[should_panic]",
        "#[should_panic(expected = \"index past the end\")]",
        "#[should_panic(expected = \"past the\")]",
    ] {
        let finding = finding_on(&source(attrs, "row(100);"), GUARD_DIFF, 2)?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{attrs}");
        assert_eq!(
            finding.static_limit_kind.map(|kind| kind.as_str()),
            Some("rust_panic_outcome_unresolved"),
            "{attrs}"
        );
        assert!(
            finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("`rejects_index_one_hundred`")),
            "{attrs}: {:?}",
            finding.recommended_next_step
        );
    }
    Ok(())
}

#[test]
fn a_panic_outcome_the_rule_does_not_cover_keeps_the_gap() -> Result<(), String> {
    for (attrs, diff, line) in [
        // The changed line is after the owner's last panic.
        ("#[should_panic]", TAIL_DIFF, 5),
        // The expected message is not one the owner raises.
        (
            "#[should_panic(expected = \"grid needs columns\")]",
            GUARD_DIFF,
            2,
        ),
        // Not a `#[should_panic]` test.
        ("", GUARD_DIFF, 2),
    ] {
        keeps_gap(&source(attrs, "row(100);"), diff, line, attrs)?;
    }
    // The test carries an assertion, which other rules judge.
    keeps_gap(
        &source(
            "#[should_panic]",
            "assert_eq!(1 + 1, 2);\n        row(100);",
        ),
        GUARD_DIFF,
        2,
        "should_panic with an assertion",
    )?;
    // The expected message is raised before the changed line, and the
    // panicking macro after it raises another.
    keeps_gap(
        &with_owner(
            "pub fn row(index: usize) -> usize { if index == 7 { panic!(\"seven\"); }
    if index >= 100 {",
            "#[should_panic(expected = \"seven\")]",
            "row(100);",
        ),
        GUARD_DIFF,
        2,
        "expected before the change",
    )?;
    Ok(())
}

/// The finding on `line` stays a `reachable_unrevealed` gap with no limit.
fn keeps_gap(lib: &str, diff: &str, line: usize, label: &str) -> Result<(), String> {
    let finding = finding_on(lib, diff, line)?;
    assert_eq!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{label} line {line}: {finding:?}"
    );
    assert_eq!(finding.static_limit_kind, None, "{label} line {line}");
    Ok(())
}
