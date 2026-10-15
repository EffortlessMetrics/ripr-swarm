//! End-to-end witness for RIPR-SPEC-0227 rule 3 (guard side flips).
//!
//! A guard that returns `Err` reads `exposed` when one test asserts the
//! owner returns `Err` on the guard's edge input and `Ok` on the adjacent
//! input: moving the edge either way flips a side that test observes. An
//! oracle on one side only, far inputs, sides split across tests, a changed
//! error and a guard the rule cannot read all keep the weak reading.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with `guard` on line 8 inside `check`, then `tests`.
fn source(guard: &str, tests: &str) -> String {
    format!(
        "#[derive(Debug, PartialEq)]
pub enum E {{
    TooLong,
    Bad,
}}

pub fn check(len: usize) -> Result<usize, E> {{
    {guard}
    Ok(len)
}}

#[cfg(test)]
mod tests {{
    use super::check;

{tests}
}}
"
    )
}

const GUARD: &str = "if len >= 256 { return Err(E::TooLong); }";

/// Line 8 changed from `old` to the current guard.
fn diff(old: &str, new: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -7,3 +7,3 @@
 pub fn check(len: usize) -> Result<usize, E> {{
-    {old}
+    {new}
     Ok(len)
"
    )
}

const BOTH_SIDES: &str = "    #[test]
    fn edge() {
        assert!(check(256).is_err());
        assert!(check(255).is_ok());
    }";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-guard-side-flip-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"guard-flip\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The rule 3 evidence line.
const GUARD_SIDE_FLIP: &str = "observe the guard's side flip";

/// The findings on line 8, which must hold a finding of every family in
/// `families`.
fn guard_findings(lib: &str, diff: &str, families: &[ProbeFamily]) -> Result<Vec<Finding>, String> {
    let findings: Vec<Finding> = TempRepo::create(lib, diff)?
        .findings()?
        .into_iter()
        .filter(|finding| finding.probe.location.line == 8)
        .collect();
    for family in families {
        if !findings
            .iter()
            .any(|finding| finding.probe.family == *family)
        {
            return Err(format!(
                "premise: a {family:?} finding on the guard line, got {findings:?}"
            ));
        }
    }
    Ok(findings)
}

/// Every finding on line 8 reads `exposed` through rule 3, and both the
/// predicate and the guard's `error_path` probe are there.
fn exposes_guard(lib: &str, diff: &str, label: &str) -> Result<(), String> {
    for finding in guard_findings(lib, diff, &[ProbeFamily::Predicate, ProbeFamily::ErrorPath])? {
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label} {:?}: {:?}",
            finding.probe.family,
            finding.evidence
        );
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.contains(GUARD_SIDE_FLIP)),
            "{label} {:?}: {:?}",
            finding.probe.family,
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn err_and_ok_on_both_sides_of_the_edge_expose_the_guard() -> Result<(), String> {
    // A respelling (RIPR-SPEC-0227 example 1 in the verdict corpus) and a
    // moved edge.
    for old in [
        "if 256 <= len { return Err(E::TooLong); }",
        "if len > 256 { return Err(E::TooLong); }",
    ] {
        exposes_guard(&source(GUARD, BOTH_SIDES), &diff(old, GUARD), old)?;
    }
    // The current guard written literal-first.
    let flipped = "if 256 <= len { return Err(E::TooLong); }";
    exposes_guard(
        &source(flipped, BOTH_SIDES),
        &diff("if len > 256 { return Err(E::TooLong); }", flipped),
        "literal-first guard",
    )?;
    // `unwrap()` panics on `Err`, so it observes the `Ok` side.
    exposes_guard(
        &source(
            GUARD,
            "    #[test]\n    fn edge() {\n        check(256).unwrap_err();\n        check(255).unwrap();\n    }",
        ),
        &diff("if len > 256 { return Err(E::TooLong); }", GUARD),
        "unwrap sides",
    )?;
    Ok(())
}

#[test]
fn an_oracle_short_of_both_sides_keeps_the_gap() -> Result<(), String> {
    let respelled = diff("if 256 <= len { return Err(E::TooLong); }", GUARD);
    for (tests, label) in [
        (
            "    #[test]\n    fn edge() {\n        assert!(check(256).is_err());\n    }",
            "Err side only (example 2)",
        ),
        (
            "    #[test]\n    fn far() {\n        assert!(check(300).is_err());\n        assert!(check(3).is_ok());\n    }",
            "far inputs (example 3)",
        ),
        (
            "    #[test]\n    fn edge() {\n        assert!(check(256).is_err());\n    }\n\n    #[test]\n    fn below() {\n        assert!(check(255).is_ok());\n    }",
            "sides split across tests (example 4)",
        ),
        (
            "    #[test]\n    fn edge() {\n        let n = 256;\n        assert!(check(n).is_err());\n        assert!(check(255).is_ok());\n    }",
            "non-literal input",
        ),
        (
            "    #[test]\n    fn edge() {\n        assert!(check(256).is_err());\n        assert!(check(255).is_ok());\n        assert!(check(9).is_err());\n    }",
            "an assertion that disagrees with the guard",
        ),
    ] {
        keeps_gap(
            &source(GUARD, tests),
            &respelled,
            &[ProbeFamily::Predicate, ProbeFamily::ErrorPath],
            label,
        )?;
    }
    Ok(())
}

#[test]
fn a_changed_error_or_an_unreadable_guard_keeps_the_gap() -> Result<(), String> {
    // Rule 1: the old guard returned another error. Only the payload
    // changed, so only the `error_path` probe is on the line.
    keeps_gap(
        &source(GUARD, BOTH_SIDES),
        &diff("if len >= 256 { return Err(E::Bad); }", GUARD),
        &[ProbeFamily::ErrorPath],
        "changed error",
    )?;
    // An old equality guard agrees with the new one at 255 and 256 and
    // differs only above the edge, which no oracle reads.
    keeps_gap(
        &source(GUARD, BOTH_SIDES),
        &diff("if len == 256 { return Err(E::TooLong); }", GUARD),
        &[ProbeFamily::Predicate],
        "old equality guard",
    )?;
    // An assertion message runs only when the condition fails and may
    // leave the test.
    keeps_gap(
        &source(
            GUARD,
            "    #[test]\n    fn edge() {\n        assert!(check(256).is_err(), \"{}\", { if true { return; } 0 });\n        assert!(check(255).is_ok());\n    }",
        ),
        &diff("if len > 256 { return Err(E::TooLong); }", GUARD),
        &[ProbeFamily::Predicate, ProbeFamily::ErrorPath],
        "leaving message",
    )?;
    // The edge and the error both changed: the predicate probe's old text
    // is the whole guard, whose payload differs.
    keeps_gap(
        &source(GUARD, BOTH_SIDES),
        &diff("if len > 256 { return Err(E::Bad); }", GUARD),
        &[ProbeFamily::Predicate, ProbeFamily::ErrorPath],
        "changed edge and error",
    )?;
    // Another `Err` after the guard: the untaken guard may not return `Ok`.
    let later_err = source(GUARD, BOTH_SIDES).replace(
        "    Ok(len)\n}",
        "    if len == 255 { return Err(E::Bad); }\n    Ok(len)\n}",
    );
    keeps_gap(
        &later_err,
        &diff("if 256 <= len { return Err(E::TooLong); }", GUARD),
        &[ProbeFamily::Predicate, ProbeFamily::ErrorPath],
        "later Err",
    )?;
    Ok(())
}

/// No finding on the guard line reads `exposed`, and the line holds a
/// finding of every family in `families`.
fn keeps_gap(lib: &str, diff: &str, families: &[ProbeFamily], label: &str) -> Result<(), String> {
    for finding in guard_findings(lib, diff, families)? {
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label} {:?}: {:?}",
            finding.probe.family,
            finding.evidence
        );
    }
    Ok(())
}
