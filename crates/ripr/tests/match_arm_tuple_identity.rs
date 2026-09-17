//! Test-first public-API witness for RIPR #1714 tuple-pattern identity.
//!
//! The positive case intentionally fails on the pre-repair analyzer: an exact
//! `(true, false)` owner call and exact result assertion must eventually bind to
//! that parser-owned arm, while the sibling `(false, true)` arm must remain
//! distinct. This file changes no production behavior or gate policy.

#[path = "support/tuple_match_arm_source_generator.rs"]
mod tuple_match_arm_source_generator;

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CANDIDATE_SOURCE: &str = r#"pub fn relation(request_match: bool, task_match: bool) -> &'static str {
    match (request_match, task_match) {
        (true, true) => "request_and_task_identity",
        (true, false) => "request_identity_v2",
        (false, true) => "task_identity",
        (false, false) => "none",
    }
}
"#;

const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,8 +1,8 @@
 pub fn relation(request_match: bool, task_match: bool) -> &'static str {
     match (request_match, task_match) {
         (true, true) => "request_and_task_identity",
-        (true, false) => "request_identity_v1",
+        (true, false) => "request_identity_v2",
         (false, true) => "task_identity",
         (false, false) => "none",
     }
 }
"#;

const ALIGNED_TEST: &str = r#"use match_arm_tuple_identity::relation;

#[test]
fn exact_request_only_tuple_is_observed() {
    assert_eq!(relation(true, false), "request_identity_v2");
}
"#;

const SIBLING_TEST: &str = r#"use match_arm_tuple_identity::relation;

#[test]
fn task_only_sibling_tuple_is_observed() {
    assert_eq!(relation(false, true), "task_identity");
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
            "ripr-match-arm-tuple-identity-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"match-arm-tuple-identity\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), CANDIDATE_SOURCE)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/relation.rs"), test_source)
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

fn changed_request_only_arm(output: &CheckOutput) -> Result<&ripr::Finding, String> {
    let mut matches = output.findings.iter().filter(|finding| {
        if finding.probe.family != ProbeFamily::MatchArm || finding.probe.location.line != 4 {
            return false;
        }
        let normalized = finding
            .probe
            .expression
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        normalized.starts_with("(true,false)=>")
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
        format!("missing changed `(true, false)` arm at line 4; observed {observed}")
    })?;
    if matches.next().is_some() {
        return Err("duplicate changed `(true, false)` match-arm findings".to_string());
    }
    Ok(finding)
}

#[test]
fn exact_tuple_input_and_result_certify_the_same_match_arm() -> Result<(), String> {
    tuple_match_arm_source_generator::emit_candidate()?;

    let repo = TempRepo::create(ALIGNED_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "an exact `(true, false)` owner input and exact result must certify only that parser-owned arm; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "exact_request_only_tuple_is_observed"
            && test.oracle.as_deref().is_some_and(|oracle| {
                oracle.contains("relation(true, false)")
                    && oracle.contains("\"request_identity_v2\"")
            })
    }));
    Ok(())
}

#[test]
fn sibling_tuple_input_cannot_certify_the_changed_arm() -> Result<(), String> {
    let repo = TempRepo::create(SIBLING_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "the `(false, true)` sibling must not certify the changed `(true, false)` arm"
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
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "task_only_sibling_tuple_is_observed"
            && test.oracle.as_deref().is_some_and(|oracle| {
                oracle.contains("relation(false, true)") && oracle.contains("\"task_identity\"")
            })
    }));
    Ok(())
}

#[test]
fn fixture_paths_remain_inside_the_ephemeral_root() -> Result<(), String> {
    let repo = TempRepo::create(ALIGNED_TEST)?;
    for relative in [
        "Cargo.toml",
        "src/lib.rs",
        "tests/relation.rs",
        "diff.patch",
    ] {
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
