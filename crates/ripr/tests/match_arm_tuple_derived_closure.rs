//! Public-API red witness for RIPR #1728.
//!
//! The merged direct-function tuple discriminator must remain fail-closed for
//! derived local booleans inside a filter_map closure. This fixture pins the
//! exact consumer shape before any production data-flow support is added.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CANDIDATE_SOURCE: &str = r#"#[derive(Clone, Copy)]
pub struct Receipt<'a> {
    pub id: &'a str,
    pub request_id: Option<&'a str>,
}

pub fn terminalize_proof(
    receipts: &[Receipt<'_>],
    request_id: &str,
    task_id: &str,
) -> String {
    receipts
        .iter()
        .filter_map(|receipt| {
            let request_identity_matches = receipt
                .request_id
                .is_some_and(|candidate| candidate == request_id);
            let task_identity_matches = receipt.id == task_id;
            let relation = match (request_identity_matches, task_identity_matches) {
                (true, true) => "request_and_task_identity",
                (true, false) => "request_identity_v2",
                (false, true) => "task_identity",
                (false, false) => return None,
            };
            Some(format!("{}:join={relation}", receipt.id))
        })
        .next()
        .unwrap_or_default()
}
"#;

const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -17,7 +17,7 @@ pub fn terminalize_proof(
             let task_identity_matches = receipt.id == task_id;
             let relation = match (request_identity_matches, task_identity_matches) {
                 (true, true) => "request_and_task_identity",
-                (true, false) => "request_identity_v1",
+                (true, false) => "request_identity_v2",
                 (false, true) => "task_identity",
                 (false, false) => return None,
             };
"#;

const REQUEST_ONLY_TEST: &str = r#"use derived_local_tuple::{Receipt, terminalize_proof};

#[test]
fn request_only_projection_observes_the_changed_relation() {
    let receipts = [Receipt {
        id: "receipt-1",
        request_id: Some("request-1"),
    }];
    assert_eq!(
        terminalize_proof(&receipts, "request-1", "different-task"),
        "receipt-1:join=request_identity_v2"
    );
}
"#;

const TASK_ONLY_TEST: &str = r#"use derived_local_tuple::{Receipt, terminalize_proof};

#[test]
fn task_only_projection_observes_the_sibling_relation() {
    let receipts = [Receipt {
        id: "receipt-1",
        request_id: Some("different-request"),
    }];
    assert_eq!(
        terminalize_proof(&receipts, "request-1", "receipt-1"),
        "receipt-1:join=task_identity"
    );
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
            "ripr-derived-local-tuple-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"derived-local-tuple\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), CANDIDATE_SOURCE)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/projection.rs"), test_source)
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

fn normalized_arm(text: &str) -> String {
    text.trim()
        .trim_end_matches(',')
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn changed_request_only_arm(output: &CheckOutput) -> Result<&ripr::Finding, String> {
    let expected_full = normalized_arm("(true, false) => \"request_identity_v2\",");
    let expected_boundary = normalized_arm("(true, false) =>");
    let expected_before = normalized_arm("(true, false) => \"request_identity_v1\",");
    let is_current_claim = |text: &str| {
        let text = normalized_arm(text);
        text == expected_full || text == expected_boundary
    };
    let matches = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ProbeFamily::MatchArm
                && is_current_claim(&finding.probe.expression)
                && finding
                    .probe
                    .after
                    .as_deref()
                    .is_some_and(&is_current_claim)
                && finding
                    .probe
                    .before
                    .as_deref()
                    .is_none_or(|before| normalized_arm(before) == expected_before)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [finding] => Ok(*finding),
        _ => {
            let observed = output
                .findings
                .iter()
                .filter(|finding| finding.probe.family == ProbeFamily::MatchArm)
                .map(|finding| {
                    format!(
                        "id={:?}; owner={:?}; line={}; before={:?}; after={:?}; expression={:?}; class={:?}",
                        finding.probe.id,
                        finding.probe.owner,
                        finding.probe.location.line,
                        finding.probe.before,
                        finding.probe.after,
                        finding.probe.expression,
                        finding.class
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            Err(format!(
                "expected exactly one candidate-current request arm, found {}; all match subjects: {observed}",
                matches.len()
            ))
        }
    }
}

#[test]
fn derived_request_identity_must_bind_through_the_exact_closure_projection() -> Result<(), String> {
    let repo = TempRepo::create(REQUEST_ONLY_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert!(finding.related_tests.iter().any(|test| {
        test.name == "request_only_projection_observes_the_changed_relation"
            && test.oracle.as_deref().is_some_and(|oracle| {
                oracle.contains("terminalize_proof") && oracle.contains("request_identity_v2")
            })
    }));
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "the exact request-only projection should become exposed only after the bounded derived-local/closure witness exists; probe={:#?}; stages={:#?}; related={:#?}",
        finding.probe,
        finding.ripr,
        finding.related_tests
    );
    Ok(())
}

#[test]
fn task_identity_sibling_cannot_certify_the_changed_request_arm() -> Result<(), String> {
    let repo = TempRepo::create(TASK_ONLY_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert!(finding.related_tests.iter().any(|test| {
        test.name == "task_only_projection_observes_the_sibling_relation"
            && test.oracle.as_deref().is_some_and(|oracle| {
                oracle.contains("terminalize_proof") && oracle.contains("task_identity")
            })
    }));
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains("observation_unverified")
    );
    Ok(())
}

#[test]
fn fixture_paths_remain_inside_the_ephemeral_root() -> Result<(), String> {
    let repo = TempRepo::create(REQUEST_ONLY_TEST)?;
    for relative in [
        "Cargo.toml",
        "src/lib.rs",
        "tests/projection.rs",
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
