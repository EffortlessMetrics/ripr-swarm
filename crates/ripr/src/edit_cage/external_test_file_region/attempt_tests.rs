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

#[test]
fn new_external_test_file_survives_baseline_roundtrip_and_publication() -> Result<(), String> {
    for publication in ["worktree", "index", "commit"] {
        let fixture = fixture(publication)?;
        git_ok(&fixture.root, &["rm", "tests/pricing.rs"])?;
        git_ok(
            &fixture.root,
            &["commit", "-qm", "target does not exist yet"],
        )?;
        // Exercise the supported case where even the tests directory is new.
        let _ = fs::remove_dir(fixture.root.join("tests"));
        let baseline = capture_attempt_baseline(&fixture.root, &external_policy()?)?;
        let serialized = serde_json::to_vec(&baseline).map_err(|e| e.to_string())?;
        let baseline = serde_json::from_slice(&serialized).map_err(|e| e.to_string())?;
        fs::create_dir_all(fixture.root.join("tests")).map_err(|e| e.to_string())?;
        fs::write(fixture.root.join("tests/pricing.rs"), BASELINE).map_err(|e| e.to_string())?;
        if publication != "worktree" {
            git_ok(&fixture.root, &["add", "tests/pricing.rs"])?;
        }
        if publication == "commit" {
            git_ok(&fixture.root, &["commit", "-qm", "new focused test"])?;
        }
        let (delta, verdict) = evaluate_repository_edit_cage_with_head_movement(
            &baseline,
            HeadMovement::AdmitDescendantCommits,
        )?;
        assert_eq!(
            verdict.status,
            EditCageVerdictStatus::Compliant,
            "{publication}: {verdict:?}"
        );
        assert!(
            delta
                .external_test_file_region
                .is_some_and(|region| region.admitted)
        );
    }
    Ok(())
}

#[test]
fn deleted_or_uncommitted_external_test_cannot_become_an_empty_baseline() -> Result<(), String> {
    for state in [
        "deleted",
        "staged-deletion",
        "untracked",
        "staged-new",
        "directory",
    ] {
        let fixture = fixture(state)?;
        if matches!(state, "untracked" | "staged-new" | "directory") {
            git_ok(&fixture.root, &["rm", "tests/pricing.rs"])?;
            git_ok(&fixture.root, &["commit", "-qm", "remove old target"])?;
        } else {
            fs::remove_file(fixture.root.join("tests/pricing.rs")).map_err(|e| e.to_string())?;
        }
        match state {
            "staged-deletion" => git_ok(&fixture.root, &["add", "-u"])?,
            "untracked" | "staged-new" => {
                fs::write(fixture.root.join("tests/pricing.rs"), BASELINE)
                    .map_err(|e| e.to_string())?;
                if state == "staged-new" {
                    git_ok(&fixture.root, &["add", "tests/pricing.rs"])?;
                }
            }
            "directory" => fs::create_dir_all(fixture.root.join("tests/pricing.rs"))
                .map_err(|e| e.to_string())?,
            _ => {}
        }
        let result = capture_attempt_baseline(&fixture.root, &external_policy()?);
        if state == "directory" {
            // The repository snapshot owner makes non-regular targets
            // incomparable rather than turning them into empty source text.
            let baseline = result?;
            assert!(baseline.ambiguous);
            assert!(baseline.external_test_file_region.is_none());
            let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
                &baseline,
                HeadMovement::RequireBaselineHead,
            )?;
            assert_eq!(verdict.status, EditCageVerdictStatus::Incomparable);
        } else {
            assert!(result.is_err(), "{state}");
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn absent_external_target_below_a_symlink_is_not_a_new_file() -> Result<(), String> {
    use std::os::unix::fs::symlink;
    let fixture = fixture("symlink-parent")?;
    git_ok(&fixture.root, &["rm", "tests/pricing.rs"])?;
    git_ok(&fixture.root, &["commit", "-qm", "remove old target"])?;
    let _ = fs::remove_dir(fixture.root.join("tests"));
    fs::create_dir_all(fixture.root.join("elsewhere")).map_err(|e| e.to_string())?;
    symlink(fixture.root.join("elsewhere"), fixture.root.join("tests"))
        .map_err(|e| e.to_string())?;
    let result = capture_attempt_baseline(&fixture.root, &external_policy()?);
    assert!(result.is_err() || result.is_ok_and(|baseline| baseline.ambiguous));
    Ok(())
}

#[test]
fn a_new_external_target_still_requires_test_insertion_and_validated_copies() -> Result<(), String>
{
    for violation in ["non-test-item", "staged-other-bytes"] {
        let fixture = fixture(violation)?;
        git_ok(&fixture.root, &["rm", "tests/pricing.rs"])?;
        git_ok(&fixture.root, &["commit", "-qm", "target absent"])?;
        let baseline = capture_attempt_baseline(&fixture.root, &external_policy()?)?;
        fs::create_dir_all(fixture.root.join("tests")).map_err(|e| e.to_string())?;
        fs::write(fixture.root.join("tests/pricing.rs"), "fn helper() {}\n")
            .map_err(|e| e.to_string())?;
        if violation == "staged-other-bytes" {
            git_ok(&fixture.root, &["add", "tests/pricing.rs"])?;
            fs::write(fixture.root.join("tests/pricing.rs"), BASELINE)
                .map_err(|e| e.to_string())?;
        }
        let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
            &baseline,
            HeadMovement::RequireBaselineHead,
        )?;
        assert_eq!(
            verdict.status,
            EditCageVerdictStatus::Violated,
            "{violation}: {verdict:?}"
        );
    }
    Ok(())
}
