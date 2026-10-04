//! End-to-end witness for RIPR-SPEC-0229 owner identity.
//!
//! A changed arm is named as unselected only when every related test's
//! owner call is a call to the changed owner. A test file that imports a
//! same-named function from another crate may call that function instead,
//! so its inputs say nothing about which arm of the changed owner ran.

use ripr::{CheckInput, CheckOutput, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const CANDIDATE_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum Kind {
    Alpha,
    Beta,
}

pub fn flip(k: Kind) -> Kind {
    match k {
        Kind::Alpha => Kind::Beta,
        Kind::Beta => Kind::Alpha,
    }
}
"#;

const OWN_CRATE_TEST: &str = r#"use match_arm_unselected_identity::{Kind, flip};

#[test]
fn alpha_flips_to_beta() {
    assert_eq!(flip(Kind::Alpha), Kind::Beta);
}
"#;

const FOREIGN_IMPORT_TEST: &str = r#"use match_arm_unselected_identity::Kind;
use other_crate::flip;

#[test]
fn alpha_flips_to_beta() {
    assert_eq!(flip(Kind::Alpha), Kind::Beta);
}
"#;

const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 0000000..1111111 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -7,6 +7,6 @@ pub enum Kind {
 pub fn flip(k: Kind) -> Kind {
     match k {
         Kind::Alpha => Kind::Beta,
-        Kind::Beta => Kind::Beta,
+        Kind::Beta => Kind::Alpha,
     }
 }
"#;

const UNSELECTED_PREFIX: &str = "No related test call selects arm";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(test_source: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests on Windows
        // read the same `SystemTime`, share one root, and overwrite each
        // other's fixture.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-match-arm-unselected-identity-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"match-arm-unselected-identity\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), CANDIDATE_SOURCE)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/flip.rs"), test_source)
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

fn changed_beta_arm(output: &CheckOutput) -> Result<&ripr::Finding, String> {
    let mut matches = output.findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::MatchArm
            && finding.probe.expression.starts_with("Kind::Beta =>")
    });
    let finding = matches.next().ok_or_else(|| {
        let observed = output
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{}:{}:{}",
                    finding.probe.family.as_str(),
                    finding.probe.location.line,
                    finding.probe.expression
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        format!("missing changed Kind::Beta arm; observed {observed}")
    })?;
    if matches.next().is_some() {
        return Err("duplicate changed Kind::Beta match-arm findings".to_string());
    }
    Ok(finding)
}

#[test]
fn own_crate_call_with_a_sibling_input_names_the_unselected_arm() -> Result<(), String> {
    let repo = TempRepo::create(OWN_CRATE_TEST)?;
    let output = repo.check()?;
    let finding = changed_beta_arm(&output)?;
    assert!(
        finding
            .ripr
            .infect
            .summary
            .starts_with("No related test call selects arm `Kind::Beta =>`"),
        "the control must name the unselected arm: {:?}",
        finding.ripr.infect
    );
    Ok(())
}

#[test]
fn a_foreign_same_named_import_does_not_name_the_unselected_arm() -> Result<(), String> {
    let repo = TempRepo::create(FOREIGN_IMPORT_TEST)?;
    let output = repo.check()?;
    let finding = changed_beta_arm(&output)?;
    assert!(
        !finding.ripr.infect.summary.contains(UNSELECTED_PREFIX),
        "a test calling another crate's `flip` gives no input to this owner: {:?}",
        finding.ripr.infect
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains(UNSELECTED_PREFIX)),
        "no evidence line may name the arm as unselected: {:?}",
        finding.evidence
    );
    Ok(())
}
