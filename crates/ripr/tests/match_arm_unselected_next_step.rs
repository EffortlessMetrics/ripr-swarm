//! End-to-end witness for #11024 (RIPR-SPEC-0229).
//!
//! A changed match arm that no related test input selects needs an input, not
//! a stronger assertion, even when the existing oracle is already exact.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub fn fee(kind: &str) -> u32 {\n    match kind {\n        \"wire\" => 25,\n        \"ach\" => 2,\n        _ => 0,\n    }\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,7 +1,7 @@\n pub fn fee(kind: &str) -> u32 {\n     match kind {\n         \"wire\" => 25,\n-        \"ach\" => 1,\n+        \"ach\" => 2,\n         _ => 0,\n     }\n }\n";
const SELECTING_STEP: &str =
    "Add a test that calls the changed code with an input that selects the arm";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(test_source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-match-arm-unselected-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| format!("mkdir src: {e}"))?;
        std::fs::create_dir_all(root.join("tests")).map_err(|e| format!("mkdir tests: {e}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"arm-unselected\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|e| format!("write Cargo.toml: {e}"))?;
        std::fs::write(root.join("src/lib.rs"), SOURCE).map_err(|e| format!("write src: {e}"))?;
        std::fs::write(root.join("tests/fee.rs"), test_source)
            .map_err(|e| format!("write test: {e}"))?;
        std::fs::write(root.join("diff.patch"), DIFF).map_err(|e| format!("write diff: {e}"))?;
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

fn arm_findings(test_source: &str) -> Result<Vec<ripr::Finding>, String> {
    let repo = TempRepo::create(test_source)?;
    let output = repo.check()?;
    let findings = output
        .findings
        .iter()
        .filter(|f| f.probe.family == ProbeFamily::MatchArm && f.probe.location.line == 4)
        .cloned()
        .collect::<Vec<_>>();
    if findings.is_empty() {
        return Err("no match-arm finding on the changed arm".to_string());
    }
    Ok(findings)
}

#[test]
fn an_arm_no_test_input_selects_asks_for_a_selecting_input() -> Result<(), String> {
    let test = "use arm_unselected::fee;\n\n#[test]\nfn fee_wire() {\n    assert_eq!(fee(\"wire\"), 25);\n}\n";
    for finding in arm_findings(test)? {
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        let step = finding.recommended_next_step.as_deref().unwrap_or_default();
        assert!(step.starts_with(SELECTING_STEP), "{step}");
        assert!(step.contains("`\"ach\" =>`"), "{step}");
        assert!(!step.contains("Strengthen the related assertion"), "{step}");
    }
    Ok(())
}

#[test]
fn a_selected_arm_never_gets_the_selecting_input_advice() -> Result<(), String> {
    let test =
        "use arm_unselected::fee;\n\n#[test]\nfn fee_ach() {\n    assert!(fee(\"ach\") > 0);\n}\n";
    for finding in arm_findings(test)? {
        let step = finding.recommended_next_step.as_deref().unwrap_or_default();
        assert!(!step.starts_with(SELECTING_STEP), "{step}");
    }
    Ok(())
}
