//! Public-API controls for value-guarded returns (RIPR-SPEC-0240, #7974).
//!
//! A test whose only `return` is `if weight(2) <op> k { return; }` before
//! `assert_eq!(weight(2), e)` reads as `static_unknown` when the guard spares
//! `e` and some wrong value. A guard that holds at `e`, a `!=` guard, a
//! compared value that is not the owner call's own return, and a guard over
//! another function keep the certain gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with `weight` on lines 1-3 and `body` as the test's body.
fn source(body: &str) -> String {
    format!(
        "pub fn weight(base: u32) -> u32 {{
    9 * base
}}

pub fn other(base: u32) -> u32 {{
    base + 1
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn weighs_two() {{
        {body}
    }}
}}
"
    )
}

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn weight(base: u32) -> u32 {
-    base * 9
+    9 * base
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-value-guard-limit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"value-guard\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
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

/// The class of the one finding on the changed line 2.
fn line_two_class(body: &str) -> Result<ExposureClass, String> {
    let findings = TempRepo::create(&source(body))?.findings()?;
    let mut on_line = findings
        .iter()
        .filter(|finding| finding.probe.location.line == 2);
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.class.clone()),
        other => Err(format!(
            "premise: exactly one finding on line 2 for {body}, got {other:?} in {findings:?}"
        )),
    }
}

#[test]
fn a_guard_that_spares_the_expected_value_reads_as_static_unknown() -> Result<(), String> {
    for body in [
        "if weight(2) > 1000 { return; }\n        assert_eq!(weight(2), 18);",
        "if 1000 < weight(2) { return; }\n        assert_eq!(18, weight(2));",
    ] {
        assert_eq!(
            line_two_class(body)?,
            ExposureClass::StaticUnknown,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn a_guard_the_rule_does_not_cover_keeps_the_gap() -> Result<(), String> {
    for body in [
        // The guard holds at the expected value.
        "if weight(2) < 1000 { return; }\n        assert_eq!(weight(2), 18);",
        // The guard returns for every wrong value.
        "if weight(2) != 18 { return; }\n        assert_eq!(weight(2), 18);",
        // The compared value is not the owner call's own return.
        "if weight(2).wrapping_sub(weight(2)) > 1000 { return; }\n        \
         assert_eq!(weight(2).wrapping_sub(weight(2)), 0);",
        // The guard reads another function; the owner is reached elsewhere.
        "let _ = weight(2);\n        if other(2) > 1000 { return; }\n        \
         assert_eq!(other(2), 3);",
    ] {
        assert_ne!(
            line_two_class(body)?,
            ExposureClass::StaticUnknown,
            "{body}"
        );
    }
    Ok(())
}
