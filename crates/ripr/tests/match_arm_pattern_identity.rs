//! End-to-end witness for RIPR #1714.
//!
//! The Rust parser already owns the exact match-arm pattern, but reveal
//! confirmation currently keeps only identifiers following `::`. A literal
//! arm therefore remains weak even when one exact test supplies the matching
//! input and asserts the arm result. This test is intentionally added before
//! the producer repair so hosted CI retains the real failing witness.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CANDIDATE_SOURCE: &str = r#"pub fn route(kind: &str) -> &'static str {
    match kind {
        "sensor" => "sensor-v2",
        "focused-test" => "proof",
        _ => "other",
    }
}
"#;

const ALIGNED_TEST: &str = r#"use match_arm_pattern_identity::route;

#[test]
fn exact_sensor_arm_is_observed() {
    assert_eq!(route("sensor"), "sensor-v2");
}
"#;

const SIBLING_TEST: &str = r#"use match_arm_pattern_identity::route;

#[test]
fn sibling_proof_arm_is_observed() {
    assert_eq!(route("focused-test"), "proof");
}
"#;

const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,7 +1,7 @@
 pub fn route(kind: &str) -> &'static str {
     match kind {
-        "sensor" => "sensor-v1",
+        "sensor" => "sensor-v2",
         "focused-test" => "proof",
         _ => "other",
     }
 }
"#;

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(test_source: &str) -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("clock before Unix epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-match-arm-pattern-identity-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"match-arm-pattern-identity\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), CANDIDATE_SOURCE)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/route.rs"), test_source)
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
            mode: Mode::Fast,
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

fn changed_sensor_arm(output: &CheckOutput) -> Result<&ripr::Finding, String> {
    output
        .findings
        .iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::MatchArm
                && finding.probe.expression.contains("\"sensor\"")
        })
        .ok_or_else(|| {
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
            format!("missing changed sensor match-arm finding; observed {observed}")
        })
}

#[test]
fn exact_literal_input_and_result_certify_the_same_match_arm() -> Result<(), String> {
    let repo = TempRepo::create(ALIGNED_TEST)?;
    let output = repo.check()?;
    let finding = changed_sensor_arm(&output)?;

    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "an exact call input and exact result assertion must certify the parser-owned literal arm; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "exact_sensor_arm_is_observed"
            && test
                .oracle
                .as_deref()
                .is_some_and(|oracle| oracle.contains("route(\"sensor\")"))
    }));
    Ok(())
}

#[test]
fn a_sibling_literal_arm_oracle_cannot_certify_the_changed_arm() -> Result<(), String> {
    let repo = TempRepo::create(SIBLING_TEST)?;
    let output = repo.check()?;
    let finding = changed_sensor_arm(&output)?;

    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "a focused-test assertion must not certify the changed sensor arm"
    );
    assert!(
        finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains("observation_unverified"),
        "the sibling-only witness must retain the explicit unverified discriminator"
    );
    Ok(())
}

#[test]
fn fixture_paths_remain_inside_the_ephemeral_root() -> Result<(), String> {
    let repo = TempRepo::create(ALIGNED_TEST)?;
    for relative in ["Cargo.toml", "src/lib.rs", "tests/route.rs", "diff.patch"] {
        let path = repo.root.join(relative);
        if !path.starts_with(&repo.root) || !path.is_file() {
            return Err(format!(
                "fixture path escaped or is missing: {}",
                path.display()
            ));
        }
    }
    Ok(())
}
