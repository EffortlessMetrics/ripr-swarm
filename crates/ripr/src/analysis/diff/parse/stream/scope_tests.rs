use std::path::{Path, PathBuf};

use super::{parse_bounded_lines, parse_bounded_lines_scoped};

fn section(path: &str) -> String {
    format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n")
}

fn vendored_tree_with_one_real_edit(vendored: usize) -> String {
    let mut input = section("src/lib.rs");
    for index in 0..vendored {
        input.push_str(&section(&format!("vendor/dep{index}/lib.rs")));
    }
    input
}

fn outside_vendor(path: &Path) -> bool {
    !path.starts_with("vendor")
}

#[test]
fn scope_applied_at_registration_keeps_an_oversized_diff_under_the_file_bound() -> Result<(), String>
{
    let input = vendored_tree_with_one_real_edit(50);
    // The unscoped control trips the bound, so the scoped success below is
    // the scope's doing and not a generous limit.
    let Err(error) = parse_bounded_lines(input.lines(), 2) else {
        return Err("control: 51 files under a limit of 2 must be refused".to_string());
    };
    assert!(error.contains("diff_scope_oversized"), "{error}");

    let parsed = parse_bounded_lines_scoped(input.lines(), 2, outside_vendor)?;
    assert_eq!(parsed.changed_files.len(), 1);
    assert_eq!(parsed.changed_files[0].path, PathBuf::from("src/lib.rs"));
    assert_eq!(parsed.out_of_scope_file_count, 50);
    assert_eq!(
        parsed.out_of_scope_sample,
        vec![
            PathBuf::from("vendor/dep0/lib.rs"),
            PathBuf::from("vendor/dep1/lib.rs"),
            PathBuf::from("vendor/dep2/lib.rs"),
        ]
    );
    Ok(())
}

#[test]
fn the_bound_still_fires_on_in_scope_files() {
    let mut input = String::new();
    for index in 0..5 {
        input.push_str(&section(&format!("src/m{index}.rs")));
    }
    input.push_str(&section("vendor/dep/lib.rs"));
    let result = parse_bounded_lines_scoped(input.lines(), 3, outside_vendor);
    assert!(
        matches!(&result, Err(error) if error.contains("diff_scope_oversized")),
        "{result:?}"
    );
}

#[test]
fn a_dropped_section_leaves_no_body_in_the_kept_files() -> Result<(), String> {
    let input = format!("{}{}", section("vendor/dep/lib.rs"), section("src/lib.rs"));
    let parsed = parse_bounded_lines_scoped(input.lines(), 10, outside_vendor)?;
    assert_eq!(parsed.changed_files.len(), 1);
    let kept = &parsed.changed_files[0];
    assert_eq!(kept.path, PathBuf::from("src/lib.rs"));
    assert_eq!(kept.added_lines.len(), 1);
    assert_eq!(kept.removed_lines.len(), 1);
    assert_eq!(parsed.out_of_scope_file_count, 1);
    Ok(())
}

#[test]
fn a_scoped_out_rename_is_counted_once() -> Result<(), String> {
    // Rename metadata and the `+++` header register the same destination
    // back to back; the disclosure must not count that one file twice.
    let input = "diff --git a/src/old.rs b/vendor/new.rs\nsimilarity index 80%\nrename from src/old.rs\nrename to vendor/new.rs\n--- a/src/old.rs\n+++ b/vendor/new.rs\n@@ -1 +1 @@\n-x\n+y\n";
    let parsed = parse_bounded_lines_scoped(input.lines(), 10, outside_vendor)?;
    assert!(parsed.changed_files.is_empty());
    assert_eq!(parsed.out_of_scope_file_count, 1);
    Ok(())
}

#[test]
fn an_admit_all_scope_matches_the_unscoped_parse() -> Result<(), String> {
    let input = vendored_tree_with_one_real_edit(3);
    let scoped = parse_bounded_lines_scoped(input.lines(), 10, |_| true)?;
    let plain = parse_bounded_lines(input.lines(), 10)?;
    assert_eq!(scoped.changed_files.len(), plain.changed_files.len());
    assert_eq!(scoped.out_of_scope_file_count, 0);
    assert!(scoped.out_of_scope_sample.is_empty());
    Ok(())
}

#[test]
fn a_dropped_section_is_not_read_as_a_truncated_stream() -> Result<(), String> {
    let input = vendored_tree_with_one_real_edit(4);
    let parsed = parse_bounded_lines_scoped(input.lines(), 10, outside_vendor)?;
    assert_eq!(parsed.truncated_file_sections, 0);

    // A kept section cut off before its body still counts as truncated, so
    // the scope cannot be used to mask a genuinely truncated producer.
    let cut = format!(
        "{}diff --git a/src/cut.rs b/src/cut.rs\n--- a/src/cut.rs\n+++ b/src/cut.rs\n",
        section("vendor/dep/lib.rs")
    );
    let parsed = parse_bounded_lines_scoped(cut.lines(), 10, outside_vendor)?;
    assert_eq!(parsed.truncated_file_sections, 1);
    Ok(())
}
