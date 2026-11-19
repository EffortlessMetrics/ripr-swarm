//! #15268: a path whose hunks arrive from more than one file section (a
//! `git format-patch` series or `git log -p` output that edits the file in
//! two commits) has no single coordinate space, so none of its lines are
//! read and a located diff-parse limitation names it.

use std::path::PathBuf;

use super::{ParsedDiff, parse_unbounded};
use crate::analysis_outcome::{AnalysisLimitationKind, AnalysisRecoveryKind, AnalysisStage};

/// Commit 1 edits line 3; commit 2 inserts three lines above it, so commit
/// 1's `+3` names what is now line 6 of the result.
const EDIT_THEN_SHIFT: &str = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -3 +3 @@\n-        qty * 9\n+        qty * 8\ndiff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -0,0 +1,3 @@\n+pub const A: u8 = 1;\n+pub const B: u8 = 2;\n+pub const C: u8 = 3;\n";

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
    let parsed = parse_unbounded(&format!("{EDIT_THEN_SHIFT}{ONCE_B}"));
    if paths(&parsed) != [PathBuf::from("src/b.rs")]
        || repeated_limitation_paths(&parsed) != [Some("src/a.rs".to_string())]
        || parsed.limitations.len() != 1
    {
        return Err(format!("repeated path was not refused alone: {parsed:?}"));
    }
    // Section order does not matter: `git log -p` lists the newest commit first.
    let mut sections = EDIT_THEN_SHIFT.splitn(3, "diff --git");
    let (_, first, second) = (sections.next(), sections.next(), sections.next());
    let (Some(first), Some(second)) = (first, second) else {
        return Err("fixture lost a section".to_string());
    };
    let reversed = parse_unbounded(&format!("diff --git{second}diff --git{first}"));
    if !paths(&reversed).is_empty()
        || repeated_limitation_paths(&reversed) != [Some("src/a.rs".to_string())]
    {
        return Err(format!("newest-first series was read: {reversed:?}"));
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
    let parsed = parse_unbounded(&format!("{edit_a}{rename}"));
    if paths(&parsed) != [PathBuf::from("src/c.rs")]
        || repeated_limitation_paths(&parsed) != [Some("src/a.rs".to_string())]
    {
        return Err(format!("edit-then-rename parsed unexpectedly: {parsed:?}"));
    }
    // Deleted afterwards.
    let delete = "diff --git a/src/a.rs b/src/a.rs\ndeleted file mode 100644\n--- a/src/a.rs\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-one\n-two\n-new\n";
    let parsed = parse_unbounded(&format!("{edit_a}{delete}"));
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
    let parsed = parse_unbounded(&format!("{edit_b}{copy}"));
    if !paths(&parsed).is_empty()
        || !parsed.copy_destination_paths.is_empty()
        || repeated_limitation_paths(&parsed) != [Some("src/b.rs".to_string())]
    {
        return Err(format!("edit-then-copy parsed unexpectedly: {parsed:?}"));
    }
    // Deleted after being emptied: git writes no `---`/`+++` pair.
    let emptied =
        "diff --git a/src/a.rs b/src/a.rs\ndeleted file mode 100644\nindex 1111111..0000000\n";
    let parsed = parse_unbounded(&format!("{edit_a}{emptied}"));
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
    let parsed = parse_unbounded(&format!("{edit_b}{pure}"));
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
