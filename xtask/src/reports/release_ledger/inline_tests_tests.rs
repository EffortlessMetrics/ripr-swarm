//! `#[cfg(test)]` span detection and zero-context diff parsing.

use super::inline_tests::{
    FileDiff, Hunk, is_candidate, is_inline_test_edit, parse_diff, test_spans,
};

fn require(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

#[test]
fn cfg_test_modules_and_items_are_spans() -> Result<(), String> {
    let source = "\
fn product() {}
#[cfg(test)]
mod tests {
    #[test]
    fn nested() {
        let _ = 1;
    }
}
fn more() {}
#[cfg(test)]
use std::fmt;
#[cfg( test )]
#[allow(dead_code)]
fn helper() -> [u8; 2] {
    [0; 2]
}
";
    require(
        test_spans(source) == vec![(2, 8), (10, 11), (12, 16)],
        &format!("{:?}", test_spans(source)),
    )
}

#[test]
fn braces_in_strings_chars_comments_and_raw_strings_do_not_end_a_span() -> Result<(), String> {
    let source = "\
#[cfg(test)]
mod tests {
    // an unmatched { in a comment
    /* and /* nested { */ here */
    const A: &str = \"{ not code }}}\";
    const B: char = '{';
    const C: &str = r#\"}} \"quoted\" }\"#;
    fn lifetime<'a>(x: &'a str) -> &'a str { x }
}
fn after() {}
";
    require(
        test_spans(source) == vec![(1, 9)],
        &format!("{:?}", test_spans(source)),
    )
}

#[test]
fn attributes_in_strings_and_other_cfgs_are_not_spans() -> Result<(), String> {
    let source = "\
const S: &str = \"#[cfg(test)] mod x {}\";
// #[cfg(test)]
#[cfg(feature = \"test\")]
mod gated {}
#[cfg(all(test, unix))]
mod platform {}
";
    require(
        test_spans(source).is_empty(),
        &format!("{:?}", test_spans(source)),
    )
}

#[test]
fn an_unclosed_item_is_not_a_span() -> Result<(), String> {
    require(
        test_spans("#[cfg(test)]\nmod tests {\n    fn open() {\n").is_empty(),
        "an item that never closes must stay product",
    )
}

#[test]
fn zero_context_diffs_parse_hunks_and_whole_files() -> Result<(), String> {
    let text = "\
diff --git a/crates/ripr/src/a.rs b/crates/ripr/src/a.rs
index 1..2 100644
--- a/crates/ripr/src/a.rs
+++ b/crates/ripr/src/a.rs
@@ -3 +3,2 @@ fn x
-old
+new
+more
@@ -10,2 +11,0 @@
-gone
-gone
diff --git a/crates/ripr/src/new.rs b/crates/ripr/src/new.rs
new file mode 100644
--- /dev/null
+++ b/crates/ripr/src/new.rs
@@ -0,0 +1,3 @@
+a
diff --git a/crates/ripr/src/old.rs b/crates/ripr/src/old.rs
deleted file mode 100644
--- a/crates/ripr/src/old.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-a
";
    let files = parse_diff(text);
    require(files.len() == 3, "three files")?;
    require(
        files[0]
            == FileDiff {
                path: "crates/ripr/src/a.rs".to_string(),
                whole_file: false,
                hunks: vec![
                    Hunk {
                        old_start: 3,
                        old_count: 1,
                        new_start: 3,
                        new_count: 2,
                    },
                    Hunk {
                        old_start: 10,
                        old_count: 2,
                        new_start: 11,
                        new_count: 0,
                    },
                ],
            },
        &format!("{:?}", files[0]),
    )?;
    require(
        files[1].whole_file && files[2].whole_file,
        "added and deleted files",
    )?;
    require(
        files[2].path == "crates/ripr/src/old.rs",
        "deleted path from the old side",
    )
}

#[test]
fn quoted_paths_are_never_inline_edits() -> Result<(), String> {
    let text = "\
diff --git \"a/crates/ripr/src/\\303.rs\" \"b/crates/ripr/src/\\303.rs\"
--- \"a/crates/ripr/src/\\303.rs\"
+++ \"b/crates/ripr/src/\\303.rs\"
@@ -1 +1 @@
-a
+b
";
    require(parse_diff(text).is_empty(), "a quoted path is skipped")
}

fn file(hunks: Vec<Hunk>) -> FileDiff {
    FileDiff {
        path: "crates/ripr/src/a.rs".to_string(),
        whole_file: false,
        hunks,
    }
}

fn hunk(old_start: u64, old_count: u64, new_start: u64, new_count: u64) -> Hunk {
    Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
    }
}

#[test]
fn hunks_must_sit_inside_a_span_on_every_side_that_has_lines() -> Result<(), String> {
    let spans = [(10, 20)];
    require(
        is_inline_test_edit(&file(vec![hunk(12, 2, 12, 3)]), &spans, &spans),
        "a modification inside the span",
    )?;
    require(
        is_inline_test_edit(&file(vec![hunk(14, 0, 15, 2)]), &[], &spans),
        "a pure insertion needs only the new side",
    )?;
    require(
        is_inline_test_edit(&file(vec![hunk(14, 2, 13, 0)]), &spans, &[]),
        "a pure deletion needs only the old side",
    )?;
    require(
        !is_inline_test_edit(
            &file(vec![hunk(12, 2, 12, 2), hunk(30, 1, 30, 1)]),
            &spans,
            &spans,
        ),
        "one hunk outside the span keeps the file product",
    )?;
    require(
        !is_inline_test_edit(&file(vec![hunk(19, 3, 19, 3)]), &spans, &spans),
        "a hunk running past the span end is not inside it",
    )?;
    require(
        !is_inline_test_edit(&file(vec![hunk(12, 1, 12, 1)]), &spans, &[(10, 11)]),
        "the new side must also be inside a span (the attribute was edited away)",
    )?;
    require(
        !is_inline_test_edit(&file(Vec::new()), &spans, &spans),
        "no hunks (binary or mode change) is not an inline test edit",
    )
}

#[test]
fn only_product_rust_files_are_candidates() -> Result<(), String> {
    require(is_candidate("crates/ripr/src/lib.rs"), "product rust")?;
    require(
        !is_candidate("crates/ripr/src/analysis/tests.rs"),
        "test file",
    )?;
    require(!is_candidate("xtask/src/main.rs"), "automation")?;
    require(!is_candidate("crates/ripr/src/README.md"), "not rust")
}
