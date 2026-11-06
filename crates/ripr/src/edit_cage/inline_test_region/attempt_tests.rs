//! Repository-level controls for the inline test-module region bound to a
//! repair attempt (#5210): the production `capture_attempt_baseline` and
//! `evaluate_repository_edit_cage_with_head_movement` path over real Git.

use super::{InlineTargetCopies, InlineTestRegionBaseline, observe_attempt_inline_region};
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

    // Price boundary notes.

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

/// #6310: a clean filter in repository-local Git configuration can turn the
/// validated worktree bytes into a different staged blob while the filtered
/// object id still equals that blob. The staged copy must be compared with
/// the exact validated bytes, so the filtered stage is not admitted.
#[cfg(unix)]
#[test]
fn a_clean_filter_cannot_launder_a_staged_production_edit() -> Result<(), String> {
    let fixture = fixture("filter", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(&fixture, &with_new_test(LIB))?;
    git_ok(
        &fixture.root,
        &["config", "filter.launder.clean", "sed 's/- 10/- 9/'"],
    )?;
    fs::create_dir_all(fixture.root.join(".git/info")).map_err(|e| format!("info dir: {e}"))?;
    fs::write(
        fixture.root.join(".git/info/attributes"),
        "src/lib.rs filter=launder\n",
    )
    .map_err(|e| format!("write attributes: {e}"))?;
    git_ok(&fixture.root, &["add", "src/lib.rs"])?;
    // Precondition: the filter really rewrote the staged blob.
    let staged = crate::git::run_git_output_with_deadline_and_limit(
        &fixture.root,
        &["show", ":src/lib.rs"],
        std::time::Duration::from_secs(10),
        1024 * 1024,
    )?;
    let staged = String::from_utf8_lossy(&staged.stdout).into_owned();
    assert!(staged.contains("cents - 9"), "{staged}");
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("index_copy_not_validated"), "{reason}");
    Ok(())
}

/// #6310: a clean filter that rewrites the validated bytes into a different
/// blob must not make the COMMITTED copy match either. The filtered commit
/// moves HEAD, and the worktree holds exactly the validated bytes. Without
/// `--no-filters` the filtered object id equals the committed blob and the
/// edit is admitted.
#[cfg(unix)]
#[test]
fn a_clean_filter_cannot_launder_a_committed_production_edit() -> Result<(), String> {
    let fixture = fixture("filter-commit", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(&fixture, &with_new_test(LIB))?;
    git_ok(
        &fixture.root,
        &["config", "filter.launder.clean", "sed 's/- 10/- 9/'"],
    )?;
    fs::create_dir_all(fixture.root.join(".git/info")).map_err(|e| format!("info dir: {e}"))?;
    fs::write(
        fixture.root.join(".git/info/attributes"),
        "src/lib.rs filter=launder\n",
    )
    .map_err(|e| format!("write attributes: {e}"))?;
    git_ok(&fixture.root, &["add", "src/lib.rs"])?;
    git_ok(&fixture.root, &["commit", "-qm", "filtered commit"])?;
    // Precondition: the filter really rewrote the committed blob.
    let committed = crate::git::run_git_output_with_deadline_and_limit(
        &fixture.root,
        &["show", "HEAD:src/lib.rs"],
        std::time::Duration::from_secs(10),
        1024 * 1024,
    )?;
    let committed = String::from_utf8_lossy(&committed.stdout).into_owned();
    assert!(committed.contains("cents - 9"), "{committed}");
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::AdmitDescendantCommits,
    )?;
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("copy_not_validated"), "{reason}");
    Ok(())
}

/// #6310: under `core.autocrlf=true` a staged CRLF target is stored
/// LF-normalized, so it no longer equals the validated CRLF worktree bytes
/// and is refused (fail closed) rather than admitted.
#[test]
fn an_autocrlf_normalized_staged_target_is_refused() -> Result<(), String> {
    let fixture = fixture("autocrlf", LIB)?;
    // HEAD holds the LF form. With autocrlf on, a CRLF worktree copy of the
    // same text is "unchanged" to git, so the before phase accepts it and the
    // only difference left is the staged-copy comparison.
    git_ok(&fixture.root, &["config", "core.autocrlf", "true"])?;
    write_lib(&fixture, &LIB.replace('\n', "\r\n"))?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    let crlf = with_new_test(LIB).replace('\n', "\r\n");
    write_lib(&fixture, &crlf)?;
    git_ok(&fixture.root, &["add", "src/lib.rs"])?;
    // Precondition: git really normalized the staged blob to LF.
    let staged = crate::git::run_git_output_with_deadline_and_limit(
        &fixture.root,
        &["show", ":src/lib.rs"],
        std::time::Duration::from_secs(10),
        1024 * 1024,
    )?;
    assert!(!staged.stdout.contains(&b'\r'), "staged blob kept CR");
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("index_copy_not_validated"), "{reason}");
    Ok(())
}

/// #6310: a failed `hash-object` is its own refusal, not a copy that merely
/// differs, and it outranks the copy comparisons it could not make.
#[test]
fn an_unreadable_worktree_object_id_is_refused_as_a_git_failure() {
    let baseline = InlineTestRegionBaseline {
        path: "src/lib.rs".to_string(),
        module_name: "tests".to_string(),
        package_identity: String::new(),
        source: LIB.to_string(),
    };
    let observe = |object_unreadable: Option<String>| {
        observe_attempt_inline_region(InlineTargetCopies {
            worktree: Ok(with_new_test(LIB)),
            index_matches: false,
            head_matches: false,
            object_unreadable,
            baseline: &baseline,
        })
    };
    let failed = observe(Some("git hash-object failed".to_string()));
    assert!(!failed.admitted, "{failed:?}");
    assert_eq!(
        failed.reason.as_deref(),
        Some("git_object_unreadable: git hash-object failed")
    );
    // Control: without the git failure the same copies read as a plain
    // differing index copy.
    let differs = observe(None);
    assert_eq!(
        differs.reason.as_deref(),
        Some("index_copy_not_validated"),
        "{differs:?}"
    );
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
    let changed_paths = |delta: &AttemptDelta| {
        delta
            .changes
            .iter()
            .map(|change| {
                (
                    change.path.clone(),
                    change.kind,
                    change.previous_path.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(changed_paths(&bound), changed_paths(&recomputed));
    assert_ne!(bound.changes, recomputed.changes);
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

/// Devin 4179587099: a helper alone adds no test, so the attempt is not a
/// completed repair even though the helper sits inside the governed module.
#[test]
fn inserting_only_a_helper_function_is_violated() -> Result<(), String> {
    let fixture = fixture("helper", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(
        &fixture,
        &LIB.replace(
            "        let _ = price(150);\n    }\n",
            "        let _ = price(150);\n    }\n\n    fn helper() -> i32 {\n        price(100)\n    }\n",
        ),
    )?;
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    assert_eq!(
        verdict.status,
        EditCageVerdictStatus::Violated,
        "{verdict:?}"
    );
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("no_test_function_added"), "{reason}");
    Ok(())
}

/// Devin 4179587144: a comment between existing items is existing bytes, so
/// rewriting it beside an inserted test is not a pure insertion.
#[test]
fn rewriting_an_existing_comment_beside_an_inserted_test_is_violated() -> Result<(), String> {
    let fixture = fixture("comment", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    let after = with_new_test(LIB).replace("// Price boundary notes.", "// Rewritten notes.");
    assert_ne!(after, with_new_test(LIB));
    write_lib(&fixture, &after)?;
    let (_, verdict) = evaluate_repository_edit_cage_with_head_movement(
        &baseline,
        HeadMovement::RequireBaselineHead,
    )?;
    assert_eq!(
        verdict.status,
        EditCageVerdictStatus::Violated,
        "{verdict:?}"
    );
    let reason = inline_violation_reason(&verdict).ok_or("expected an inline-region violation")?;
    assert!(reason.contains("not_pure_insertion"), "{reason}");
    Ok(())
}

/// Positive control for the byte-preservation law: a new test may bring a
/// doc comment, a new comment, and blank lines.
#[test]
fn a_new_test_with_comments_and_blank_lines_is_compliant() -> Result<(), String> {
    let fixture = fixture("doc-comment", LIB)?;
    let baseline = capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    write_lib(
        &fixture,
        &LIB.replace(
            "        let _ = price(150);\n    }\n",
            "        let _ = price(150);\n    }\n\n\n    // Equality boundary.\n\n    /// A discount applies at exactly 100.\n    #[test]\n    fn boundary() {\n        assert_eq!(price(100), 90);\n    }\n\n",
        ),
    )?;
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

/// Devin 4179587290: a file the repository configures as generated is not an
/// inline edit region, even when a packet names it.
#[test]
fn a_configured_generated_target_cannot_be_captured() -> Result<(), String> {
    let fixture = fixture("generated", LIB)?;
    fs::write(
        fixture.root.join("ripr.toml"),
        "[languages.rust]\ngenerated_file_patterns = [\"src/lib.rs\"]\n",
    )
    .map_err(|e| format!("write config: {e}"))?;
    git_ok(&fixture.root, &["add", "ripr.toml"])?;
    git_ok(&fixture.root, &["commit", "-qm", "configure generated"])?;
    let error = match capture_attempt_baseline(&fixture.root, &inline_policy()?) {
        Ok(_) => {
            return Err("a configured-generated file must not become an edit region".to_string());
        }
        Err(error) => error,
    };
    assert!(error.contains("generated_file_patterns"), "{error}");
    assert!(error.contains("No attempt was created"), "{error}");

    // Control: the same file without the pattern is captured.
    fs::write(fixture.root.join("ripr.toml"), "").map_err(|e| format!("clear config: {e}"))?;
    capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    Ok(())
}

/// An uncommitted production edit present before the before phase would
/// otherwise become part of the captured "before" text, so the after-phase
/// validator and the receipt would never see it. Capture refuses a confined
/// target whose worktree or index copy differs from HEAD; the clean capture
/// in `inserting_a_test_function_into_the_governed_inline_module_is_compliant`
/// is the control.
#[test]
fn a_production_edit_made_before_capture_is_refused() -> Result<(), String> {
    let fixture = fixture("dirty-premise", LIB)?;
    write_lib(&fixture, &LIB.replace("cents - 10", "cents - 9"))?;
    let error = match capture_attempt_baseline(&fixture.root, &inline_policy()?) {
        Ok(_) => return Err("a dirty production premise must not be captured".to_string()),
        Err(error) => error,
    };
    assert!(error.contains("uncommitted changes"), "{error}");
    assert!(error.contains("No attempt was created"), "{error}");

    // Staged but not committed is refused the same way.
    git_ok(&fixture.root, &["add", "src/lib.rs"])?;
    let error = match capture_attempt_baseline(&fixture.root, &inline_policy()?) {
        Ok(_) => return Err("a staged production premise must not be captured".to_string()),
        Err(error) => error,
    };
    assert!(error.contains("uncommitted changes"), "{error}");

    // Committing the change makes it the premise, and capture proceeds.
    git_ok(&fixture.root, &["commit", "-qm", "premise"])?;
    capture_attempt_baseline(&fixture.root, &inline_policy()?)?;
    Ok(())
}
