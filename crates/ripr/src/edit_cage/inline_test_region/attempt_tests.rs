//! Repository-level controls for the inline test-module region bound to a
//! repair attempt (#5210): the production `capture_attempt_baseline` and
//! `evaluate_repository_edit_cage_with_head_movement` path over real Git.

use crate::edit_cage::{
    AttemptDelta, AttemptPathChange, CagePathRule, EditCagePolicy, EditCageVerdictStatus,
    EditCageViolationKind, HeadMovement, capture_attempt_baseline, evaluate_edit_cage,
    evaluate_repository_edit_cage_with_head_movement,
};
use crate::testing::fixture_git::fixture_git_ok as git_ok;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const LIB: &str = r#"pub fn price(cents: i32) -> i32 {
    if cents >= 100 { cents - 10 } else { cents }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing() {
        let _ = price(150);
    }
}
"#;

const NEW_TEST: &str = "        let _ = price(150);\n    }\n\n    #[test]\n    fn boundary() {\n        assert_eq!(price(100), 90);\n    }\n";

fn with_new_test(source: &str) -> String {
    source.replacen("        let _ = price(150);\n    }\n", NEW_TEST, 1)
}

struct Fixture {
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fixture(name: &str, lib: &str) -> Result<Fixture, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-inline-attempt-{name}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("src")).map_err(|e| format!("create src: {e}"))?;
    fs::write(root.join("src/lib.rs"), lib).map_err(|e| format!("write lib: {e}"))?;
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

fn inline_policy() -> Result<EditCagePolicy, String> {
    Ok(EditCagePolicy {
        selected_target: CagePathRule::exact("src/lib.rs")?,
        allowed_edit_surface: vec![CagePathRule::exact("src/lib.rs")?],
        forbidden_paths: Vec::new(),
        expected_operational_writes: vec![CagePathRule::subtree("target/ripr")?],
        ignored_build_output: None,
        untracked_build_lockfile: None,
        inline_test_module_target: true,
    })
}

fn write_lib(fixture: &Fixture, source: &str) -> Result<(), String> {
    fs::write(fixture.root.join("src/lib.rs"), source).map_err(|e| format!("write lib: {e}"))
}

fn inline_violation_reason(verdict: &crate::edit_cage::EditCageVerdict) -> Option<String> {
    verdict
        .violations
        .iter()
        .find(|violation| violation.kind == EditCageViolationKind::OutsideInlineTestRegion)
        .map(|violation| violation.reason.clone())
}

#[test]
fn inserting_a_test_function_into_the_governed_inline_module_is_compliant() -> Result<(), String> {
    let fixture = fixture("admitted", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(&fixture, &with_new_test(LIB))?;
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
        .inline_test_region
        .ok_or("a changed confined target must carry its region observation")?;
    assert!(observation.admitted, "{observation:?}");
    assert_eq!(observation.module, "tests");
    Ok(())
}

/// Negative experiment: the same worktree change under the file-level rule
/// alone (the policy without inline confinement) is compliant, so only the
/// region binding refuses the production edit.
#[test]
fn a_production_edit_beside_the_inserted_test_is_violated() -> Result<(), String> {
    let fixture = fixture("production", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(
        &fixture,
        &with_new_test(LIB).replace("cents - 10", "cents - 9"),
    )?;
    let (delta, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    assert_eq!(
        verdict.status,
        EditCageVerdictStatus::Violated,
        "{verdict:?}"
    );
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("production_edit"), "{reason}");

    let mut file_level = inline_policy()?;
    file_level.inline_test_module_target = false;
    assert_eq!(
        evaluate_edit_cage(&file_level, &delta).status,
        EditCageVerdictStatus::Compliant,
        "without the region binding the file-level cage admits the production edit"
    );
    Ok(())
}

#[test]
fn rewriting_the_existing_inline_test_is_violated() -> Result<(), String> {
    let fixture = fixture("rewrite", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(
        &fixture,
        &with_new_test(LIB).replace("let _ = price(150);", "assert_eq!(price(150), 140);"),
    )?;
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("existing_authority_rewritten"), "{reason}");
    Ok(())
}

#[test]
fn an_inline_module_without_a_test_cfg_cannot_be_captured() -> Result<(), String> {
    let ungoverned = LIB.replace("#[cfg(test)]\n", "");
    let fixture = fixture("ungoverned", &ungoverned)?;
    let error = match capture_attempt_baseline(&fixture.root, &inline_policy()?) {
        Ok(_) => return Err("an ungoverned module must not become an edit region".to_string()),
        Err(error) => error,
    };
    assert!(error.contains("no inline `#[cfg(test)]` module"), "{error}");
    assert!(error.contains("No attempt was created"), "{error}");
    Ok(())
}

#[test]
fn two_governed_inline_modules_cannot_be_captured() -> Result<(), String> {
    let two =
        format!("{LIB}\n#[cfg(test)]\nmod more_tests {{\n    #[test]\n    fn other() {{}}\n}}\n");
    let fixture = fixture("ambiguous", &two)?;
    let error = match capture_attempt_baseline(&fixture.root, &inline_policy()?) {
        Ok(_) => return Err("two candidate modules must not pick one".to_string()),
        Err(error) => error,
    };
    assert!(
        error.contains("more than one candidate test module"),
        "{error}"
    );
    Ok(())
}

/// A staged production edit behind a worktree that holds only the inserted
/// test would let a commit carry unvalidated bytes.
#[test]
fn a_staged_production_edit_behind_a_clean_worktree_is_violated() -> Result<(), String> {
    let fixture = fixture("index", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(
        &fixture,
        &with_new_test(LIB).replace("cents - 10", "cents - 9"),
    )?;
    git_ok(&fixture.root, &["add", "src/lib.rs"])?;
    write_lib(&fixture, &with_new_test(LIB))?;
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("index_copy_not_validated"), "{reason}");

    // Control: staging exactly the validated bytes is compliant.
    git_ok(&fixture.root, &["add", "src/lib.rs"])?;
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    assert_eq!(
        verdict.status,
        EditCageVerdictStatus::Compliant,
        "{verdict:?}"
    );
    Ok(())
}

#[test]
fn a_committed_production_edit_reverted_only_in_the_worktree_is_violated() -> Result<(), String> {
    let fixture = fixture("commit", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(
        &fixture,
        &with_new_test(LIB).replace("cents - 10", "cents - 9"),
    )?;
    git_ok(&fixture.root, &["commit", "-qam", "test and production"])?;
    write_lib(&fixture, &with_new_test(LIB))?;
    git_ok(&fixture.root, &["add", "src/lib.rs"])?;
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::AdmitDescendantCommits,
    )?;
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("committed_copy_not_validated"), "{reason}");

    // Control: committing exactly the validated bytes is compliant.
    git_ok(&fixture.root, &["commit", "-qm", "keep only the test"])?;
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::AdmitDescendantCommits,
    )?;
    assert_eq!(
        verdict.status,
        EditCageVerdictStatus::Compliant,
        "{verdict:?}"
    );
    Ok(())
}

/// The receipt recomputes the delta; a production edit made after the
/// compliant after phase must move it, or the binding would not notice.
#[test]
fn a_later_production_edit_moves_the_recomputed_delta() -> Result<(), String> {
    let fixture = fixture("later", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(&fixture, &with_new_test(LIB))?;
    let (bound, _) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    write_lib(
        &fixture,
        &with_new_test(LIB).replace("cents - 10", "cents - 9"),
    )?;
    let (recomputed, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    assert_eq!(bound.changes, recomputed.changes);
    assert_ne!(bound, recomputed);
    assert_eq!(verdict.status, EditCageVerdictStatus::Violated);
    Ok(())
}

#[test]
fn a_confined_target_change_without_an_observation_fails_closed() -> Result<(), String> {
    let verdict = evaluate_edit_cage(
        &inline_policy()?,
        &AttemptDelta {
            comparable: true,
            inline_test_region: None,
            changes: vec![AttemptPathChange::modified("src/lib.rs")],
        },
    );
    assert_eq!(verdict.status, EditCageVerdictStatus::Violated);
    assert!(inline_violation_reason(&verdict).is_some(), "{verdict:?}");
    Ok(())
}
