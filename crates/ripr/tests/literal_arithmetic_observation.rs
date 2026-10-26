//! Public-API controls for a literal-only arithmetic rewrite (RIPR-SPEC-0158, #10479).
//!
//! `800` rewritten as `8 * 100` returns the same value, so an exact-value
//! assertion on `800` still observes the changed expression. The fold is exact
//! integer arithmetic over literals only: a wrong expected value, an overflow
//! and a non-literal operand do not confirm. The test reaches the owner through
//! an imported dependency type, where a pinned owner return does not by itself
//! confirm observation.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const METER: &str = "pub struct Lamp;

impl Lamp {
    pub fn lumens() -> u32 {
        AFTER
    }
}
";

const PANEL_TEST: &str = "use meter::*;

#[test]
fn lamp_lumens() {
    assert_eq!(Lamp::lumens(), EXPECTED);
}
";

/// The returned expression on line 5 of the meter crate changed.
fn diff(before: &str, after: &str) -> String {
    format!(
        "diff --git a/meter/src/lib.rs b/meter/src/lib.rs
--- a/meter/src/lib.rs
+++ b/meter/src/lib.rs
@@ -3,4 +3,4 @@
 impl Lamp {{
     pub fn lumens() -> u32 {{
-        {before}
+        {after}
     }}
"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(after: &str, patch: &str, expected: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-literal-arithmetic-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let write = |path: &str, text: &str| -> Result<(), String> {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(&full, text).map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"meter\", \"panel\"]\nresolver = \"2\"\n",
        )?;
        write(
            "meter/Cargo.toml",
            "[package]\nname = \"meter\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        write("meter/src/lib.rs", &METER.replace("AFTER", after))?;
        write(
            "panel/Cargo.toml",
            "[package]\nname = \"panel\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmeter = { path = \"../meter\" }\n",
        )?;
        write("panel/src/lib.rs", "")?;
        write(
            "panel/tests/direct.rs",
            &PANEL_TEST.replace("EXPECTED", expected),
        )?;
        write("diff.patch", patch)?;
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

/// The class of the one return-value finding on line 5 after `before` became `after`.
fn class_after(before: &str, after: &str, expected: &str) -> Result<ExposureClass, String> {
    let findings = TempRepo::create(after, &diff(before, after), expected)?.findings()?;
    let mut on_line = findings.iter().filter(|finding| {
        finding.probe.location.line == 5 && finding.probe.family == ProbeFamily::ReturnValue
    });
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.class.clone()),
        other => Err(format!(
            "premise: exactly one return-value finding on line 5, got {other:?} in {findings:?}"
        )),
    }
}

#[test]
fn a_folded_literal_expression_confirms_an_exact_value_assertion() -> Result<(), String> {
    // Independent witnesses: each rewrite returns the same value.
    assert_eq!(8 * 100, 800);
    assert_eq!((2 + 2) * 200, 800);
    for after in ["8 * 100", "(2 + 2) * 200", "1000 - 200"] {
        assert_eq!(
            class_after("800", after, "800")?,
            ExposureClass::Exposed,
            "{after}"
        );
    }
    Ok(())
}

#[test]
fn a_wrong_expected_value_does_not_confirm() -> Result<(), String> {
    assert_ne!(8 * 100, 801);
    assert_ne!(
        class_after("800", "8 * 100", "801")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_overflowing_or_non_literal_expression_does_not_confirm() -> Result<(), String> {
    // 100000 * 100000 overflows u32 and the fold's own bound.
    assert_eq!(100_000u64 * 100_000, 10_000_000_000);
    assert_ne!(
        class_after("800", "100000 * 100000", "800")?,
        ExposureClass::Exposed
    );
    // A named operand is not a literal-only expression.
    assert_ne!(
        class_after("800", "SCALE * 100", "800")?,
        ExposureClass::Exposed
    );
    Ok(())
}
