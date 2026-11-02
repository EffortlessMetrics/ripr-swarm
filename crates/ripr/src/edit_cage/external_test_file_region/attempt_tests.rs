//! Repository-level controls for external Rust test-file pure insertion (#8710).

use super::{ExternalTargetCopies, ExternalTestFileBaseline, observe_attempt_external_region};
use crate::edit_cage::{
    CagePathRule, EditCagePolicy, EditCageVerdictStatus, EditCageViolationKind, HeadMovement,
    capture_attempt_baseline, evaluate_edit_cage, evaluate_repository_edit_cage_with_head_movement,
};
use crate::testing::fixture_git::fixture_git_ok as git_ok;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const BASELINE: &str = "#[test]\nfn existing() {\n    assert_eq!(2 + 2, 4);\n}\n";

const COMPLIANT: &str = "#[test]\nfn existing() {\n    assert_eq!(2 + 2, 4);\n}\n\n#[test]\nfn boundary() {\n    assert_eq!(3 + 3, 6);\n}\n";

struct Fixture {
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fixture(name: &str) -> Result<Fixture, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-external-test-cage-{name}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("tests")).map_err(|e| format!("create tests: {e}"))?;
    fs::write(root.join("tests/pricing.rs"), BASELINE).map_err(|e| format!("write test: {e}"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"p\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|e| format!("write manifest: {e}"))?;
    fs::write(root.join(".gitignore"), "target/\n").map_err(|e| format!("ignore: {e}"))?;
    git_ok(&root, &["-c", "init.templateDir=", "init", "-q"])?;
    git_ok(&root, &["config", "user.email", "ripr@example.invalid"])?;
    git_ok(&root, &["config", "user.name", "RIPR Test"])?;
    git_ok(&root, &["config", "commit.gpgSign", "false"])?;
    git_ok(&root, &["config", "core.autocrlf", "false"])?;
    git_ok(&root, &["config", "core.hooksPath", ".no-hooks"])?;
    git_ok(&root, &["add", "."])?;
    git_ok(&root, &["commit", "-qm", "baseline"])?;
    Ok(Fixture { root })
}

fn external_policy() -> Result<EditCagePolicy, String> {
    Ok(EditCagePolicy {
        selected_target: CagePathRule::exact("tests/pricing.rs")?,
        allowed_edit_surface: vec![CagePathRule::exact("tests/pricing.rs")?],
        forbidden_paths: Vec::new(),
        expected_operational_writes: vec![CagePathRule::subtree("target/ripr")?],
        ignored_build_output: None,
        untracked_build_lockfile: None,
        inline_test_module_target: false,
        external_test_file_target: true,
    })
}

fn region_violation_reason(verdict: &crate::edit_cage::EditCageVerdict) -> Option<String> {
    verdict
        .violations
        .iter()
        .find(|violation| violation.kind == EditCageViolationKind::OutsideInlineTestRegion)
        .map(|violation| violation.reason.clone())
}

#[test]
fn pure_insertion_into_external_test_file_is_compliant() -> Result<(), String> {
    let fixture = fixture("admitted")?;
    let baseline = capture_attempt_baseline(&fixture.root, &external_policy()?)?;
    fs::write(fixture.root.join("tests/pricing.rs"), COMPLIANT)
        .map_err(|e| format!("write test: {e}"))?;
    let (delta, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    assert_eq!(
        verdict.status,
        EditCageVerdictStatus::Compliant,
        "{verdict:?}"
    );
    let observation = delta
        .external_test_file_region
        .ok_or("expected an external test-file observation")?;
    assert!(observation.admitted, "{observation:?}");
    Ok(())
}

#[test]
fn rewriting_existing_external_test_is_violated() -> Result<(), String> {
    let fixture = fixture("rewrite")?;
    let baseline = capture_attempt_baseline(&fixture.root, &external_policy()?)?;
    fs::write(fixture.root.join("tests/pricing.rs"), "fn repaired() {}\n")
        .map_err(|e| format!("write test: {e}"))?;
    let (delta, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    assert_eq!(
        verdict.status,
        EditCageVerdictStatus::Violated,
        "{verdict:?}"
    );
    let reason = region_violation_reason(&verdict).ok_or("expected a region violation")?;
    assert!(
        reason.contains("existing_authority_rewritten") || reason.contains("not_pure_insertion"),
        "{reason}"
    );
    let mut path_only = external_policy()?;
    path_only.external_test_file_target = false;
    assert_eq!(
        evaluate_edit_cage(&path_only, &delta).status,
        EditCageVerdictStatus::Compliant,
        "without the content cage a full rewrite stays path-only compliant"
    );
    Ok(())
}

#[test]
fn observe_attempt_external_region_matches_inline_copy_checks() -> Result<(), String> {
    let baseline = ExternalTestFileBaseline {
        path: "tests/pricing.rs".to_string(),
        source: BASELINE.to_string(),
    };
    let observation = observe_attempt_external_region(ExternalTargetCopies {
        worktree: Ok(COMPLIANT.to_string()),
        index_matches: true,
        head_matches: true,
        object_unreadable: None,
        baseline: &baseline,
    });
    assert!(observation.admitted, "{observation:?}");
    Ok(())
}
