//! End-to-end controls for the boolean-owner next step (#15149, #15537
//! review, RIPR-SPEC-0259).
//!
//! A `-> bool` owner whose related tests assert its own exact result gets the
//! deciding-input step. A test that calls the owner but asserts a same-named
//! rival in another module reaches the owner (test-level relation) without
//! asserting it, so it keeps the family template.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub mod a {\n    pub fn is_small(n: u32) -> bool {\n        n == 1 || n == 2\n    }\n}\n\npub mod b {\n    pub fn is_small(n: u32) -> bool {\n        n == 1\n    }\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,11 +1,11 @@\n pub mod a {\n     pub fn is_small(n: u32) -> bool {\n-        n == 1\n+        n == 1 || n == 2\n     }\n }\n \n pub mod b {\n     pub fn is_small(n: u32) -> bool {\n         n == 1\n     }\n }\n";
const DECIDING: &str = "The related tests already assert the exact boolean result of `is_small`";
const TEMPLATE: &str = "Replace broad assertions with exact equality";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(test_source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-bool-owner-next-step-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"bool-owner-next-step\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), SOURCE)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/owner.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The class and next step of the single return-value finding on the
/// changed tail (line 3), after asserting the fixture produced it.
fn return_value_step(test_source: &str) -> Result<(ExposureClass, String), String> {
    let repo = TempRepo::create(test_source)?;
    let output = repo.check()?;
    let mut matches = output.findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 3
    });
    let finding = matches
        .next()
        .ok_or_else(|| "missing return_value finding at line 3".to_string())?;
    if matches.next().is_some() {
        return Err("duplicate return_value findings at line 3".to_string());
    }
    if finding.probe.expression.trim() != "n == 1 || n == 2" {
        return Err(format!(
            "return_value finding is `{}`, not the changed tail",
            finding.probe.expression
        ));
    }
    Ok((
        finding.class.clone(),
        finding.recommended_next_step.clone().unwrap_or_default(),
    ))
}

#[test]
fn the_owners_own_boolean_assertions_get_the_deciding_input_step() -> Result<(), String> {
    let test = "use bool_owner_next_step::a;\n\n#[test]\nfn one_is_small() {\n    assert!(a::is_small(1));\n}\n\n#[test]\nfn three_is_not_small() {\n    assert!(!a::is_small(3));\n}\n";
    let (class, step) = return_value_step(test)?;
    assert_eq!(class, ExposureClass::WeaklyExposed, "{step}");
    assert!(step.contains(DECIDING), "{step}");
    assert!(!step.contains(TEMPLATE), "{step}");
    Ok(())
}

#[test]
fn a_rivals_assertion_beside_an_owner_call_keeps_the_template() -> Result<(), String> {
    // The test reaches `a::is_small` but asserts `b::is_small`: the owner's
    // result is never asserted, so the step must not claim it is.
    let test = "use bool_owner_next_step::{a, b};\n\n#[test]\nfn mixed() {\n    let _ = a::is_small(1);\n    assert!(b::is_small(1));\n}\n";
    let (class, step) = return_value_step(test)?;
    assert_ne!(class, ExposureClass::Exposed, "{step}");
    assert!(!step.contains(DECIDING), "{step}");
    Ok(())
}
