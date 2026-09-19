//! Public-API witnesses for RIPR #1728 derived-local tuple-arm identity.
//!
//! Model the active UB Review `terminalize_proof` shape without rewriting the
//! consumer: two immutable booleans are derived inside a `filter_map` closure,
//! the `(false, false)` sibling returns `None`, and the selected relation flows
//! into the returned terminal projection. Unsupported mappings must remain
//! explicitly unverified.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CANDIDATE_SOURCE: &str = r#"use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq, Eq)]
pub struct Receipt {
    pub id: String,
}

pub fn terminalize_proof<'a>(
    receipts: &'a [Receipt],
    receipt_request_ids: &BTreeMap<String, Vec<String>>,
    request_set: &BTreeSet<String>,
    task_id: &str,
) -> Vec<(&'a Receipt, &'static str)> {
    receipts
        .iter()
        .filter_map(|receipt| {
            let request_identity_matches = receipt_request_ids
                .get(&receipt.id)
                .is_some_and(|receipt_requests| {
                    receipt_requests
                        .iter()
                        .any(|request_id| request_set.contains(request_id.as_str()))
                });
            let task_identity_matches = receipt.id == task_id;
            let relation = match (request_identity_matches, task_identity_matches) {
                (true, true) => "request_and_task_identity",
                (true, false) => "request_identity_v2",
                (false, true) => "task_identity",
                (false, false) => return None,
            };
            Some((receipt, relation))
        })
        .collect()
}
"#;

const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -23,7 +23,7 @@ pub fn terminalize_proof<'a>(
             let relation = match (request_identity_matches, task_identity_matches) {
                 (true, true) => "request_and_task_identity",
-                (true, false) => "request_identity_v1",
+                (true, false) => "request_identity_v2",
                 (false, true) => "task_identity",
                 (false, false) => return None,
             };
"#;

const REQUEST_ONLY_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn request_only_projection_observes_join() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let mut receipt_request_ids = BTreeMap::new();
    receipt_request_ids.insert(
        "receipt-1".to_string(),
        vec!["request-1".to_string()],
    );
    let request_set = BTreeSet::from(["request-1".to_string()]);

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "different-task",
    );

    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0].0.id, "receipt-1");
    assert_eq!(terminal[0].1, "request_identity_v2");
}
"#;

const TASK_ONLY_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn task_only_projection_observes_sibling() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let receipt_request_ids = BTreeMap::new();
    let request_set = BTreeSet::new();

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "receipt-1",
    );

    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0].0.id, "receipt-1");
    assert_eq!(terminal[0].1, "task_identity");
}
"#;

const BOTH_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn both_identity_projection_observes_sibling() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let mut receipt_request_ids = BTreeMap::new();
    receipt_request_ids.insert(
        "receipt-1".to_string(),
        vec!["request-1".to_string()],
    );
    let request_set = BTreeSet::from(["request-1".to_string()]);

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "receipt-1",
    );

    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0].0.id, "receipt-1");
    assert_eq!(terminal[0].1, "request_and_task_identity");
}
"#;

const NEITHER_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn neither_identity_is_excluded() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let receipt_request_ids = BTreeMap::new();
    let request_set = BTreeSet::new();

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "different-task",
    );

    assert!(terminal.is_empty());
}
"#;

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str, diff: &str, test_source: &str) -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("clock before Unix epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-match-arm-derived-local-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"match-arm-tuple-derived-local\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), source)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/terminal.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
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

fn normalized(text: &str) -> String {
    text.trim()
        .trim_end_matches(',')
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn changed_request_only_arm(output: &CheckOutput) -> Result<&ripr::Finding, String> {
    let expected_full = normalized("(true, false) => \"request_identity_v2\",");
    let expected_boundary = normalized("(true, false) =>");
    let expected_before = normalized("(true, false) => \"request_identity_v1\",");
    let is_current = |text: &str| {
        let text = normalized(text);
        text == expected_full || text == expected_boundary
    };
    let matches = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ProbeFamily::MatchArm
                && is_current(&finding.probe.expression)
                && finding
                    .probe
                    .after
                    .as_deref()
                    .is_some_and(&is_current)
                && finding
                    .probe
                    .before
                    .as_deref()
                    .is_none_or(|before| normalized(before) == expected_before)
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
                        "id={:?}; file={}; line={}; before={:?}; after={:?}; expression={:?}; class={:?}",
                        finding.probe.id,
                        finding.probe.location.file.display(),
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
                "expected exactly one candidate-current request-only arm, found {}; all match subjects: {observed}",
                matches.len()
            ))
        }
    }
}

fn assert_unverified(finding: &ripr::Finding, context: &str) {
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{context}: {:?}",
        finding.ripr.reveal.discriminate
    );
    assert!(
        finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains("observation_unverified"),
        "{context}: {:?}",
        finding.ripr.reveal.discriminate
    );
}

#[test]
fn request_only_derived_locals_observe_the_changed_relation() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, DIFF, REQUEST_ONLY_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "the exact request-only receipt route must certify the current arm; probe={:#?}; stages={:#?}; related={:#?}",
        finding.probe,
        finding.ripr,
        finding.related_tests
    );
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "request_only_projection_observes_join"
            && test.oracle.as_deref().is_some_and(|oracle| {
                oracle.contains("terminalize_proof")
                    && oracle.contains("request_identity_v2")
                    && oracle.contains("receipt-1")
            })
    }));
    Ok(())
}

#[test]
fn task_only_sibling_cannot_certify_the_request_arm() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, DIFF, TASK_ONLY_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "task-only sibling");
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "task_only_projection_observes_sibling"
            && test
                .oracle
                .as_deref()
                .is_some_and(|oracle| oracle.contains("task_identity"))
    }));
    Ok(())
}

#[test]
fn both_identity_sibling_cannot_certify_the_request_arm() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, DIFF, BOTH_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "both-identity sibling");
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "both_identity_projection_observes_sibling"
            && test
                .oracle
                .as_deref()
                .is_some_and(|oracle| oracle.contains("request_and_task_identity"))
    }));
    Ok(())
}

#[test]
fn neither_identity_exclusion_cannot_certify_the_request_arm() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, DIFF, NEITHER_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "return-None sibling");
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "neither_identity_is_excluded"
            && test
                .oracle
                .as_deref()
                .is_some_and(|oracle| oracle.contains("is_empty"))
    }));
    Ok(())
}

fn check_unsupported_mapping(
    scrutinee: &str,
    test_source: &str,
    context: &str,
) -> Result<(), String> {
    let original = "match (request_identity_matches, task_identity_matches)";
    assert_eq!(CANDIDATE_SOURCE.matches(original).count(), 1);
    assert_eq!(DIFF.matches(original).count(), 1);
    let replacement = format!("match {scrutinee}");
    let source = CANDIDATE_SOURCE.replace(original, &replacement);
    let diff = DIFF.replace(original, &replacement);
    let repo = TempRepo::create(&source, &diff, test_source)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, context);
    Ok(())
}

#[test]
fn reordered_local_tuple_does_not_borrow_owner_call_inputs() -> Result<(), String> {
    check_unsupported_mapping(
        "(task_identity_matches, request_identity_matches)",
        TASK_ONLY_TEST,
        "reordered local tuple",
    )
}

#[test]
fn transformed_local_tuple_does_not_borrow_derived_values() -> Result<(), String> {
    check_unsupported_mapping(
        "(!request_identity_matches, task_identity_matches)",
        NEITHER_TEST,
        "transformed local tuple",
    )
}

#[test]
fn fixture_paths_remain_inside_the_ephemeral_root() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, DIFF, REQUEST_ONLY_TEST)?;
    for relative in [
        "Cargo.toml",
        "src/lib.rs",
        "tests/terminal.rs",
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
