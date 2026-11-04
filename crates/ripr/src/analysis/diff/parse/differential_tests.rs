//! #12918: Git supplies patch syntax; the generating edit supplies the oracle.
//! No expected coordinate is read back from a hunk header or another parser.
//!
//! Longer local run: `PROPTEST_CASES=400 cargo test -p ripr --lib
//! --no-default-features --features lang-rust
//! analysis::diff::parse::differential_tests`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;

use crate::testing::fixture_git::{fixture_git_ok, fixture_git_stdout};

use super::{ParsedDiff, parse_unified_diff_with_metadata};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

struct Repository(PathBuf);

impl Repository {
    fn new() -> Result<Self, String> {
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let path = parent.join(format!(
            "ripr-diff-oracle-{}-{}",
            std::process::id(),
            NEXT_CASE.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation: never overwrite or clean another run's fixture.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let root = Self(path);
        fixture_git_ok(&root.0, &["-c", "init.templateDir=", "init", "-q"])?;
        fixture_git_ok(&root.0, &["config", "core.autocrlf", "false"])?;
        Ok(root)
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) -> Result<(), String> {
        std::fs::write(self.0.join(path), bytes).map_err(|error| error.to_string())
    }

    fn stage(&self) -> Result<(), String> {
        fixture_git_ok(&self.0, &["add", "-A"])
    }

    fn tree(&self) -> Result<String, String> {
        self.stage()?;
        fixture_git_stdout(&self.0, &["write-tree"])
    }

    fn diff(&self, base: &str) -> Result<String, String> {
        self.stage()?;
        fixture_git_stdout(
            &self.0,
            &[
                "-c",
                "core.quotePath=true",
                "diff",
                "--cached",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--diff-algorithm=myers",
                "--no-indent-heuristic",
                "--find-renames=50%",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--unified=3",
                base,
                "--",
            ],
        )
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!("remove owned diff oracle {}: {error}", self.0.display());
        }
    }
}

fn source(prefix: usize, middle: &[String], suffix: usize, crlf: bool, newline: bool) -> String {
    let lines: Vec<_> = (0..prefix)
        .map(|line| format!("unchanged prefix {line:04}"))
        .chain(middle.iter().cloned())
        .chain((0..suffix).map(|line| format!("unchanged suffix {line:04}")))
        .collect();
    let eol = if crlf { "\r\n" } else { "\n" };
    let mut text = lines.join(eol);
    if newline && !lines.is_empty() {
        text.push_str(eol);
    }
    text
}

fn changed_lines(start: usize, lines: &[String]) -> Vec<(usize, String)> {
    lines
        .iter()
        .enumerate()
        .map(|(offset, text)| (start + offset, text.clone()))
        .collect()
}

/// Deliberately compare both sides, exact path, count, coordinates and payload.
/// A parser that returns no changes, credits context, or shifts by one fails.
fn check_edit(
    parsed: &ParsedDiff,
    path: &str,
    start: usize,
    removed: &[String],
    added: &[String],
) -> Result<(), String> {
    let [file] = parsed.changed_files.as_slice() else {
        return Err(format!("expected one changed file, got {parsed:?}"));
    };
    if file.path != Path::new(path) {
        return Err(format!("expected path {path}, got {:?}", file.path));
    }
    let actual_removed: Vec<_> = file
        .removed_lines
        .iter()
        .map(|line| (line.line, line.text.clone()))
        .collect();
    let actual_added: Vec<_> = file
        .added_lines
        .iter()
        .map(|line| (line.line, line.text.clone()))
        .collect();
    if actual_removed != changed_lines(start, removed)
        || actual_added != changed_lines(start, added)
    {
        return Err(format!("generated edit differs from parser: {file:?}"));
    }
    if file
        .added_lines
        .iter()
        .any(|line| line.new_side_line != line.line)
        || file
            .removed_lines
            .iter()
            .any(|line| line.new_side_line != start)
    {
        return Err(format!("wrong new-side anchors: {file:?}"));
    }
    if !parsed.limitations.is_empty() || parsed.truncated_file_sections != 0 {
        return Err(format!("complete generated patch was limited: {parsed:?}"));
    }
    Ok(())
}

fn exercise_edit(
    prefix: usize,
    removed_count: usize,
    added_count: usize,
    suffix: usize,
    crlf: bool,
    newline: bool,
    rename: bool,
) -> Result<(), String> {
    let repo = Repository::new()?;
    let removed: Vec<_> = (0..removed_count)
        .map(|line| format!("old value {line}"))
        .collect();
    let added: Vec<_> = (0..added_count)
        .map(|line| format!("new value {line}"))
        .collect();
    let old_path = "before space.rs";
    let new_path = if rename { "after space.rs" } else { old_path };
    repo.write(old_path, source(prefix, &removed, suffix, crlf, newline))?;
    let base = repo.tree()?;
    if rename {
        std::fs::remove_file(repo.0.join(old_path)).map_err(|error| error.to_string())?;
    }
    repo.write(new_path, source(prefix, &added, suffix, crlf, newline))?;
    let patch = repo.diff(&base)?;
    if !patch.contains("@@") || (rename && !patch.contains("rename to after space.rs")) {
        return Err(format!(
            "Git did not generate the required edit/rename: {patch}"
        ));
    }
    let parsed = parse_unified_diff_with_metadata(&patch);
    check_edit(&parsed, new_path, prefix + 1, &removed, &added)
        .map_err(|error| format!("{error}\nGit patch:\n{patch}"))?;
    let bounded = super::stream::parse_bounded_lines_scoped(patch.lines(), 100, 10_000, |_| true)
        .map_err(|error| error.to_string())?;
    check_edit(&bounded, new_path, prefix + 1, &removed, &added)
        .map_err(|error| format!("bounded {error}\nGit patch:\n{patch}"))?;
    if parsed.renamed_file_count != usize::from(rename) {
        return Err(format!("wrong rename count: {parsed:?}"));
    }
    Ok(())
}

fn proptest_config() -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = 32;
    }
    config
}

proptest! {
    #![proptest_config(proptest_config())]

    #[test]
    fn git_edits_match_generated_line_ranges(
        prefix in 0usize..16,
        removed_count in 0usize..8,
        added_count in 0usize..8,
        suffix in 1usize..16,
        crlf in any::<bool>(),
        newline in any::<bool>(),
    ) {
        prop_assume!(removed_count + added_count > 0);
        exercise_edit(prefix, removed_count, added_count, suffix, crlf, newline, false)
            .map_err(TestCaseError::fail)?;
    }

    #[test]
    fn git_rename_edits_match_generated_line_ranges(
        removed_count in 1usize..5,
        added_count in 1usize..5,
        crlf in any::<bool>(),
        newline in any::<bool>(),
    ) {
        // Plenty of unique unchanged context guarantees Git's similarity gate.
        exercise_edit(20, removed_count, added_count, 20, crlf, newline, true)
            .map_err(TestCaseError::fail)?;
    }
}

#[test]
fn git_edit_boundary_matrix() -> Result<(), String> {
    for crlf in [false, true] {
        for newline in [false, true] {
            // Insertion/deletion/replacement at the first line and EOF, plus
            // a net line-count shift. Distinct lines prevent ambiguous LCSs.
            for (prefix, removed, added, suffix) in [
                (0, 0, 2, 3),
                (0, 2, 0, 3),
                (0, 2, 3, 3),
                (3, 2, 1, 0),
                (3, 0, 2, 0),
                (3, 2, 0, 0),
                (0, 0, 2, 0),
                (0, 2, 0, 0),
            ] {
                exercise_edit(prefix, removed, added, suffix, crlf, newline, false)?;
            }
        }
    }
    Ok(())
}

#[test]
fn git_separated_hunks_keep_old_and_new_coordinates_distinct() -> Result<(), String> {
    let repo = Repository::new()?;
    // The first replacement grows by two lines; eight unique unchanged lines
    // separate the edits so Git emits two hunks with three lines of context.
    let common: Vec<_> = (0..8).map(|line| format!("middle {line}")).collect();
    let old = [
        vec!["old first".to_string()],
        common.clone(),
        vec!["old second".to_string(), "old third".to_string()],
    ]
    .concat();
    let new = [
        vec![
            "new first".to_string(),
            "new extra".to_string(),
            "new third".to_string(),
        ],
        common,
        vec!["new second".to_string()],
    ]
    .concat();
    repo.write("multi.rs", source(2, &old, 2, false, true))?;
    let base = repo.tree()?;
    repo.write("multi.rs", source(2, &new, 2, false, true))?;
    let patch = repo.diff(&base)?;
    assert_eq!(
        patch.matches("\n@@").count(),
        2,
        "two-hunk setup failed: {patch}"
    );
    let bounded = super::stream::parse_bounded_lines_scoped(patch.lines(), 100, 10_000, |_| true)
        .map_err(|error| error.to_string())?;
    for parsed in [parse_unified_diff_with_metadata(&patch), bounded] {
        let [file] = parsed.changed_files.as_slice() else {
            return Err(format!("expected multi.rs, got {parsed:?}"));
        };
        assert_eq!(file.path, Path::new("multi.rs"));
        let removed: Vec<_> = file
            .removed_lines
            .iter()
            .map(|line| (line.line, line.new_side_line, line.text.as_str()))
            .collect();
        let added: Vec<_> = file
            .added_lines
            .iter()
            .map(|line| (line.line, line.new_side_line, line.text.as_str()))
            .collect();
        assert_eq!(
            removed,
            [
                (3, 3, "old first"),
                (12, 14, "old second"),
                (13, 14, "old third")
            ]
        );
        assert_eq!(
            added,
            [
                (3, 3, "new first"),
                (4, 4, "new extra"),
                (5, 5, "new third"),
                (14, 14, "new second")
            ]
        );
        assert!(parsed.limitations.is_empty());
        assert_eq!(parsed.truncated_file_sections, 0);
    }
    Ok(())
}

#[test]
fn git_binary_deletion_and_pure_rename_are_not_text_edits() -> Result<(), String> {
    let repo = Repository::new()?;
    repo.write("binary.rs", b"old\0binary\n")?;
    repo.write("deleted.rs", "deleted source\n")?;
    repo.write("old.rs", "unchanged rename\n")?;
    let base = repo.tree()?;
    repo.write("binary.rs", b"new\0binary\n")?;
    std::fs::remove_file(repo.0.join("deleted.rs")).map_err(|error| error.to_string())?;
    std::fs::rename(repo.0.join("old.rs"), repo.0.join("renamed.rs"))
        .map_err(|error| error.to_string())?;
    let patch = repo.diff(&base)?;
    assert!(
        patch.contains("Binary files"),
        "binary setup failed: {patch}"
    );
    assert!(
        patch.contains("+++ /dev/null"),
        "deletion setup failed: {patch}"
    );
    assert!(
        patch.contains("similarity index 100%"),
        "rename setup failed: {patch}"
    );
    let parsed = parse_unified_diff_with_metadata(&patch);
    assert_eq!(parsed.deleted_file_count, 1);
    assert_eq!(parsed.pure_rename_file_count, 1);
    assert_eq!(parsed.renamed_file_count, 1);
    assert_eq!(parsed.pure_rename_paths, [PathBuf::from("renamed.rs")]);
    assert!(
        parsed
            .changed_files
            .iter()
            .all(|file| file.added_lines.is_empty() && file.removed_lines.is_empty()),
        "non-text edits credited as text: {parsed:?}"
    );
    Ok(())
}

#[test]
fn generated_oracle_rejects_empty_shifted_and_context_credited_results() -> Result<(), String> {
    let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -2 +2 @@\n-old\n+new\n";
    let removed = vec!["old".to_string()];
    let added = vec!["new".to_string()];
    let mut parsed = parse_unified_diff_with_metadata(patch);
    check_edit(&parsed, "file.rs", 2, &removed, &added)?;
    assert!(check_edit(&ParsedDiff::default(), "file.rs", 2, &removed, &added).is_err());
    let Some(file) = parsed.changed_files.first_mut() else {
        return Err("positive control unexpectedly empty".to_string());
    };
    file.added_lines[0].line += 1;
    assert!(check_edit(&parsed, "file.rs", 2, &removed, &added).is_err());
    let Some(file) = parsed.changed_files.first_mut() else {
        return Err("shifted control unexpectedly empty".to_string());
    };
    file.added_lines[0].line -= 1;
    file.removed_lines[0].text = "context".to_string();
    assert!(check_edit(&parsed, "file.rs", 2, &removed, &added).is_err());
    Ok(())
}
