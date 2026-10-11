//! Discriminators for #7308: `sorted_allowlist_content` must keep later
//! comments and sort only within comment/blank-separated groups.
//!
//! Declared from `main.rs` (not `tests.rs`) so this file does not collide
//! with open PR #7306.

use super::sorted_allowlist_content;
use super::suggested_fixes_patch;
use crate::tests::with_temp_cwd;
use crate::tests::write;
use std::collections::BTreeMap;
use std::path::Path;

/// Pre-#7308 helper: keep comments only until the first entry, then drop
/// later comments and globally sort remaining entries.
fn dropped_later_comments(text: &str) -> String {
    let mut prefix = Vec::new();
    let mut entries = Vec::new();
    let mut saw_entry = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if !saw_entry && (trimmed.is_empty() || trimmed.starts_with('#')) {
            prefix.push(line.trim_end().to_string());
            continue;
        }
        saw_entry = true;
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            entries.push(trimmed.to_string());
        }
    }

    entries.sort();
    let mut output = String::new();
    if !prefix.is_empty() {
        output.push_str(&prefix.join("\n"));
        output.push('\n');
    }
    if !entries.is_empty() {
        if !output.ends_with("\n\n") {
            output.push('\n');
        }
        output.push_str(&entries.join("\n"));
        output.push('\n');
    }
    if output.is_empty() {
        output.push('\n');
    }
    output
}

fn comment_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.trim().starts_with('#'))
        .collect()
}

fn entry_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .collect()
}

fn entry_multiset(text: &str) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for entry in entry_lines(text) {
        *counts.entry(entry).or_insert(0) += 1;
    }
    counts
}

fn has_internal_comments(text: &str) -> bool {
    let mut saw_entry = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            if saw_entry {
                return true;
            }
        } else {
            saw_entry = true;
        }
    }
    false
}

fn production_source() -> &'static str {
    include_str!("../main.rs")
}

#[test]
fn predecessor_drops_internal_comments_current_keeps_them() {
    let input = "\
# header
zeta
# owner: group-a; reason: documented run; covered_by: this test
bravo
alpha
# trailing provenance
";
    let dropped = dropped_later_comments(input);
    let sorted = sorted_allowlist_content(input);

    assert!(
        !dropped.contains("# owner: group-a"),
        "the known-wrong helper must still demonstrate the #7308 loss"
    );
    assert!(
        !dropped.contains("# trailing provenance"),
        "the known-wrong helper must drop trailing comments after an entry"
    );
    assert_eq!(comment_lines(&dropped), ["# header"]);
    assert_ne!(
        sorted, dropped,
        "the retained helper must not match the comment-dropping predecessor"
    );
    assert_eq!(
        comment_lines(&sorted),
        [
            "# header",
            "# owner: group-a; reason: documented run; covered_by: this test",
            "# trailing provenance"
        ]
    );
}

#[test]
fn leading_header_still_globally_sorts_a_comment_free_entry_run() {
    let input = "# Header\n# More\n\nz|kind|owner|reason\na|kind|owner|reason\n";
    assert_eq!(
        sorted_allowlist_content(input),
        "# Header\n# More\n\na|kind|owner|reason\nz|kind|owner|reason\n"
    );
}

#[test]
fn sorts_entries_within_comment_groups_without_moving_comments() {
    let input = "\
# group-a
zeta
alpha
# group-b
mu
beta
";
    let sorted = sorted_allowlist_content(input);
    assert_eq!(
        sorted,
        "\
# group-a
alpha
zeta
# group-b
beta
mu
"
    );
    assert_eq!(
        comment_lines(&sorted),
        ["# group-a", "# group-b"],
        "comments stay as group separators rather than traveling with a globally sorted row"
    );
    assert!(
        sorted
            .find("# group-a")
            .zip(sorted.find("alpha"))
            .is_some_and(|(comment, entry)| comment < entry),
        "group-a comment must remain above its own entries"
    );
    assert!(
        sorted
            .find("zeta")
            .zip(sorted.find("# group-b"))
            .is_some_and(|(entry, comment)| entry < comment),
        "group-a entries must not move across the group-b comment"
    );
}

#[test]
fn blank_lines_separate_groups_and_are_kept() {
    let input = "# header\n\nzeta\n\nalpha\n\n# trailing\n";
    let sorted = sorted_allowlist_content(input);
    assert_eq!(sorted, "# header\n\nzeta\n\nalpha\n\n# trailing\n");
}

#[test]
fn duplicate_entries_are_kept_inside_a_group() {
    let input = "# header\nbravo\nalpha\nalpha\n";
    let sorted = sorted_allowlist_content(input);
    assert_eq!(sorted, "# header\nalpha\nalpha\nbravo\n");
    assert_eq!(entry_multiset(input), entry_multiset(&sorted));
}

#[test]
fn trailing_comment_without_a_following_entry_is_kept() {
    let input = "zeta\nalpha\n# leftover owner note\n";
    let sorted = sorted_allowlist_content(input);
    assert_eq!(sorted, "alpha\nzeta\n# leftover owner note\n");
}

#[test]
fn repeated_normalization_is_idempotent_and_preserves_entry_multisets() {
    let input = "\
# header

zeta
# mid
bravo
alpha
bravo
# trailing

";
    let once = sorted_allowlist_content(input);
    let twice = sorted_allowlist_content(&once);
    assert_eq!(once, twice);
    assert_eq!(entry_multiset(input), entry_multiset(&once));
    assert_eq!(comment_lines(input), comment_lines(&once));
}

#[test]
fn empty_input_still_emits_a_newline() {
    assert_eq!(sorted_allowlist_content(""), "\n");
}

#[test]
fn both_shape_and_suggested_fix_callers_still_use_the_shared_helper() {
    let source = production_source();
    let call_sites = source
        .matches("let sorted = sorted_allowlist_content(&original);")
        .count();
    assert_eq!(
        call_sites, 2,
        "shape (`sort_allowlist_files`) and suggested-fixes must keep sharing sorted_allowlist_content; found {call_sites}"
    );
    assert!(
        source.contains("fn sort_allowlist_files()"),
        "shape-owned writer must remain the sort_allowlist_files consumer"
    );
    assert!(
        source.contains("fn suggested_fixes_patch()"),
        "suggested-fixes must remain a consumer of the shared helper"
    );
}

#[test]
fn exact_base_public_api_allowlist_keeps_every_explanatory_comment() -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../policy/public_api.txt");
    let original = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if !has_internal_comments(&original) {
        return Err(
            "policy/public_api.txt must still contain comments after the first entry so this fixture discriminates #7308"
                .to_string(),
        );
    }
    let dropped = dropped_later_comments(&original);
    if comment_lines(&dropped) == comment_lines(&original) {
        return Err(
            "the known-wrong helper must still drop public_api.txt internal comments".to_string(),
        );
    }
    let sorted = sorted_allowlist_content(&original);
    if comment_lines(&sorted) != comment_lines(&original) {
        return Err(format!(
            "normalization dropped or reordered comments: before {} after {}",
            comment_lines(&original).len(),
            comment_lines(&sorted).len()
        ));
    }
    if entry_multiset(&original) != entry_multiset(&sorted) {
        return Err("normalization must not add, drop, or rewrite public API entries".to_string());
    }
    Ok(())
}

#[test]
fn suggested_fixes_patch_keeps_internal_allowlist_comments() -> Result<(), String> {
    with_temp_cwd("suggested-fixes-keep-comments", |root| {
        write(
            &root.join("policy/public_api.txt"),
            "# header\nzeta\n# owner: group-a\nbravo\nalpha\n",
        );
        let (patch, files) = suggested_fixes_patch()?;
        if files != ["policy/public_api.txt".to_string()] {
            return Err(format!(
                "expected only the unsorted allowlist, got {files:?}"
            ));
        }
        if !patch.contains("# owner: group-a") {
            return Err(format!(
                "suggested-fixes must retain the internal comment in the replacement file:\n{patch}"
            ));
        }
        if !patch.contains("+alpha") || !patch.contains("+bravo") {
            return Err(format!(
                "suggested-fixes must still sort the commented group:\n{patch}"
            ));
        }
        Ok(())
    })
}
