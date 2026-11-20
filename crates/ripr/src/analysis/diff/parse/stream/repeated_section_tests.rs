//! #15268: a path whose hunks arrive from file sections on both sides of a
//! commit boundary (a `git format-patch` series or `git log -p` output that
//! edits the file in two commits) has no single coordinate space, so none of
//! its lines are read and a located diff-parse limitation names it. Sections
//! of one two-way diff that repeat a path stay readable.

use std::path::PathBuf;

use super::{ParsedDiff, parse_unbounded};
use crate::analysis_outcome::{AnalysisLimitationKind, AnalysisRecoveryKind, AnalysisStage};

/// Commit 1 edits line 3; commit 2 inserts three lines above it, so commit
/// 1's `+3` names what is now line 6 of the result.
const EDIT: &str = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -3 +3 @@\n-        qty * 9\n+        qty * 8\n";
const SHIFT: &str = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -0,0 +1,3 @@\n+pub const A: u8 = 1;\n+pub const B: u8 = 2;\n+pub const C: u8 = 3;\n";

/// The mbox separator `git format-patch --stdout` writes before each message.
const MBOX: &str = "From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001\nSubject: [PATCH] next\n\n---\n";
/// The header `git log -p` writes before each commit's patch.
const LOG: &str = "commit 0123456789abcdef0123456789abcdef01234567 (HEAD -> main)\nAuthor: A U Thor <author@example.invalid>\n\n    next\n\n";

const ONCE_B: &str = "diff --git a/src/b.rs b/src/b.rs\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -2 +2 @@\n-before\n+after\n";

fn paths(parsed: &ParsedDiff) -> Vec<PathBuf> {
    parsed
        .changed_files
        .iter()
        .map(|file| file.path.clone())
        .collect()
}

fn repeated_limitation_paths(parsed: &ParsedDiff) -> Vec<Option<String>> {
    parsed
        .limitations
        .iter()
        .filter(|limitation| {
            limitation.kind == AnalysisLimitationKind::LanguageScopeUnsupported
                && limitation.producer_stage == AnalysisStage::DiffParse
                && limitation.recovery.kind == AnalysisRecoveryKind::UseTwoWayDiff
                && limitation.recovery.detail.contains("more than one section")
        })
        .map(|limitation| limitation.path.clone())
        .collect()
}

#[test]
fn a_file_edited_in_two_sections_reads_none_of_its_lines() -> Result<(), String> {
    // A path changed in one section of the same series keeps its lines: its
    // only section's new side is the result's coordinate space.
    for boundary in [MBOX, LOG] {
        let parsed = parse_unbounded(&format!("{MBOX}{EDIT}{boundary}{SHIFT}{ONCE_B}"));
        if paths(&parsed) != [PathBuf::from("src/b.rs")]
            || repeated_limitation_paths(&parsed) != [Some("src/a.rs".to_string())]
            || parsed.limitations.len() != 1
        {
            return Err(format!("repeated path was not refused alone: {parsed:?}"));
        }
        // Section order does not matter: `git log -p` lists the newest commit
        // first.
        let reversed = parse_unbounded(&format!("{boundary}{SHIFT}{boundary}{EDIT}"));
        if !paths(&reversed).is_empty()
            || repeated_limitation_paths(&reversed) != [Some("src/a.rs".to_string())]
        {
            return Err(format!("newest-first series was read: {reversed:?}"));
        }
    }
    // `git log -p --oneline` writes no `commit` header, but each commit's
    // section starts from a different old blob.
    let oneline = format!(
        "1111111 first\n{}2222222 second\n{}",
        with_index(EDIT, "aaaaaaa..bbbbbbb"),
        with_index(SHIFT, "bbbbbbb..ccccccc")
    );
    let parsed = parse_unbounded(&oneline);
    if !paths(&parsed).is_empty()
        || repeated_limitation_paths(&parsed) != [Some("src/a.rs".to_string())]
    {
        return Err(format!("headerless log series was read: {parsed:?}"));
    }
    Ok(())
}

/// Adds an `index <old>..<new>` line after the section's `diff --git` header.
fn with_index(section: &str, range: &str) -> String {
    section.replacen("\n", &format!("\nindex {range} 100644\n"), 1)
}

#[test]
fn sections_of_one_diff_that_repeat_a_path_are_read() -> Result<(), String> {
    // Control: without a commit boundary both sections describe the same
    // old/new pair (a hand-assembled diff with one section per hunk), so both
    // are read in one coordinate space, as before #15268.
    let parsed = parse_unbounded(&format!("{EDIT}{SHIFT}"));
    let Some(file) = parsed.changed_files.first() else {
        return Err(format!("repeated path in one diff was dropped: {parsed:?}"));
    };
    let added: Vec<usize> = file.added_lines.iter().map(|line| line.line).collect();
    if !parsed.limitations.is_empty() || !added.contains(&1) || !added.contains(&3) {
        return Err(format!(
            "repeated path in one diff was not read: {parsed:?}"
        ));
    }
    // Repeating the same `index` range, or an abbreviation of it, keeps one
    // old blob and stays readable.
    let parsed = parse_unbounded(&format!(
        "{}{}",
        with_index(EDIT, "aaaaaaa..bbbbbbb"),
        with_index(SHIFT, "aaaaaaaaa..bbbbbbbbb")
    ));
    if !parsed.limitations.is_empty() || parsed.changed_files.is_empty() {
        return Err(format!(
            "one old blob in two sections was refused: {parsed:?}"
        ));
    }
    // A boundary before the first edit separates nothing.
    let parsed = parse_unbounded(&format!("{LOG}{EDIT}{SHIFT}"));
    if !parsed.limitations.is_empty() || parsed.changed_files.is_empty() {
        return Err(format!(
            "a leading commit header refused the diff: {parsed:?}"
        ));
    }
    Ok(())
}

#[test]
fn one_section_per_path_is_read_as_before() -> Result<(), String> {
    // Control: a single section, and a rename whose metadata registers the
    // destination before its own `+++` header in the same section.
    let rename = "diff --git a/src/a.rs b/src/c.rs\nsimilarity index 90%\nrename from src/a.rs\nrename to src/c.rs\n--- a/src/a.rs\n+++ b/src/c.rs\n@@ -3 +3 @@\n-old\n+new\n";
    for diff in [ONCE_B.to_string(), format!("{ONCE_B}{rename}")] {
        let parsed = parse_unbounded(&diff);
        if !parsed.limitations.is_empty()
            || parsed.changed_files.is_empty()
            || parsed
                .changed_files
                .iter()
                .any(|file| file.added_lines.is_empty())
        {
            return Err(format!("single-section path was refused: {parsed:?}"));
        }
    }
    Ok(())
}

#[test]
fn a_later_rename_copy_or_delete_of_an_edited_path_refuses_its_lines() -> Result<(), String> {
    let edit_a = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -3 +3 @@\n-old\n+new\n";
    // Renamed and edited afterwards: the old path's lines describe a file
    // that no longer exists; the destination's only section is current.
    let rename = "diff --git a/src/a.rs b/src/c.rs\nsimilarity index 90%\nrename from src/a.rs\nrename to src/c.rs\n--- a/src/a.rs\n+++ b/src/c.rs\n@@ -5 +5 @@\n-x\n+y\n";
    let parsed = parse_unbounded(&format!("{edit_a}{MBOX}{rename}"));
    if paths(&parsed) != [PathBuf::from("src/c.rs")]
        || repeated_limitation_paths(&parsed) != [Some("src/a.rs".to_string())]
    {
        return Err(format!("edit-then-rename parsed unexpectedly: {parsed:?}"));
    }
    // Deleted afterwards.
    let delete = "diff --git a/src/a.rs b/src/a.rs\ndeleted file mode 100644\n--- a/src/a.rs\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-one\n-two\n-new\n";
    let parsed = parse_unbounded(&format!("{edit_a}{MBOX}{delete}"));
    if !paths(&parsed).is_empty()
        || repeated_limitation_paths(&parsed) != [Some("src/a.rs".to_string())]
    {
        return Err(format!("edit-then-delete parsed unexpectedly: {parsed:?}"));
    }
    // Copied onto a path an earlier section edited: the copy destination is
    // no longer expanded as a whole added file either.
    let edit_b =
        "diff --git a/src/b.rs b/src/b.rs\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -1 +1 @@\n-p\n+q\n";
    let copy = "diff --git a/src/z.rs b/src/b.rs\nsimilarity index 100%\ncopy from src/z.rs\ncopy to src/b.rs\n";
    let parsed = parse_unbounded(&format!("{edit_b}{MBOX}{copy}"));
    if !paths(&parsed).is_empty()
        || !parsed.copy_destination_paths.is_empty()
        || repeated_limitation_paths(&parsed) != [Some("src/b.rs".to_string())]
    {
        return Err(format!("edit-then-copy parsed unexpectedly: {parsed:?}"));
    }
    // Deleted after being emptied: git writes no `---`/`+++` pair.
    let emptied =
        "diff --git a/src/a.rs b/src/a.rs\ndeleted file mode 100644\nindex 1111111..0000000\n";
    let parsed = parse_unbounded(&format!("{edit_a}{MBOX}{emptied}"));
    if !paths(&parsed).is_empty()
        || repeated_limitation_paths(&parsed) != [Some("src/a.rs".to_string())]
    {
        return Err(format!(
            "edit-then-empty-delete parsed unexpectedly: {parsed:?}"
        ));
    }
    // Pure-renamed onto a path an earlier section edited: it is no longer
    // listed as an unchanged pure rename.
    let pure = "diff --git a/src/z.rs b/src/b.rs\nsimilarity index 100%\nrename from src/z.rs\nrename to src/b.rs\n";
    let parsed = parse_unbounded(&format!("{edit_b}{MBOX}{pure}"));
    if !paths(&parsed).is_empty()
        || !parsed.pure_rename_paths.is_empty()
        || repeated_limitation_paths(&parsed) != [Some("src/b.rs".to_string())]
    {
        return Err(format!(
            "edit-then-pure-rename parsed unexpectedly: {parsed:?}"
        ));
    }
    Ok(())
}
