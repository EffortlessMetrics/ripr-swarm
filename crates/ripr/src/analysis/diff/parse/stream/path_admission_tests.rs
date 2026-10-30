use super::{parse_bounded_lines, parse_unbounded};
use std::path::Path;

#[test]
fn path_like_added_and_removed_lines_stay_hunk_payload() -> Result<(), String> {
    let old = format!("-- {}", "/".repeat(600));
    let new = format!("++ {}", "/".repeat(600));
    let input = format!("--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-{old}\n+{new}\n");
    let parsed = parse_bounded_lines(input.lines(), 1)?;
    assert_eq!(parsed.changed_files.len(), 1);
    let file = &parsed.changed_files[0];
    assert_eq!(file.path, Path::new("src/a.rs"));
    assert_eq!(file.removed_lines[0].text, old);
    assert_eq!(file.added_lines[0].text, new);
    assert!(parsed.limitations.is_empty());
    assert_eq!(parsed.truncated_file_sections, 0);
    let compatibility = parse_unbounded(&input);
    assert_eq!(file.added_lines, compatibility.changed_files[0].added_lines);
    assert_eq!(
        file.removed_lines,
        compatibility.changed_files[0].removed_lines
    );
    Ok(())
}

#[test]
fn actual_deep_headers_still_refuse_after_hunk_payload() {
    let path = format!("{}lib.rs", "deep/".repeat(600));
    let complete = "--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let unfinished = "--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n";
    for input in [
        format!("{complete}--- a/{path}\n+++ b/src/b.rs\n"),
        format!("{complete}--- a/src/b.rs\n+++ b/{path}\n"),
        format!("{complete}diff --git a/{path} b/{path}\n"),
        format!("{unfinished}diff --git a/{path} b/{path}\n"),
    ] {
        assert!(
            matches!(parse_bounded_lines(input.lines(), 2), Err(error)
                if error.starts_with("diff_path_too_deep:")),
            "actual path headers must retain depth admission"
        );
    }
}
