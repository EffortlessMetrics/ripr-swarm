//! A message-only `throw` change on a later line of a multi-line
//! replacement reads its own old line, so the RIPR-SPEC-0243 rule 4 guard
//! sees both messages (#7524). Before, only the first added line of a
//! replacement block found an old line; the rest read as pure additions and
//! kept `exact_error_variant` credit from a check that passes on both. An
//! uneven block pairs by content, so the `throw` meets the removed `throw`
//! even when a comment line was added or deleted beside it.

use super::*;

const LABEL_EDITED: &str = "export function parseLabel(s: string): string {\n  const label = s.trim();\n  if (label === \"\") {\n    // refuse a blank label\n    throw new Error(\"label is blank\");\n  }\n  return label;\n}\n";

/// The comment and the `throw` below it are replaced together.
const TWO_LINE_DIFF: &str = "diff --git a/src/label.ts b/src/label.ts\n--- a/src/label.ts\n+++ b/src/label.ts\n@@ -2,6 +2,6 @@ export function parseLabel(s: string): string {\n   const label = s.trim();\n   if (label === \"\") {\n-    // refuse an empty label\n-    throw new Error(\"label must not be blank\");\n+    // refuse a blank label\n+    throw new Error(\"label is blank\");\n   }\n   return label;\n }\n";

fn chai_test(check: &str) -> String {
    format!(
        "import {{ expect }} from \"chai\";\nimport {{ parseLabel }} from \"../src/label\";\n\ndescribe(\"parseLabel\", () => {{\n  it(\"rejects a blank label\", () => {{\n    expect(() => parseLabel(\"  \")).to.throw({check});\n  }});\n}});\n"
    )
}

/// Runs the adapter over a real unified diff of `src/label.ts` in a mocha
/// package and returns the finding on `line`.
fn finding_on(
    label: &str,
    new_source: &str,
    test_source: &str,
    diff: &str,
    line: usize,
) -> Result<Finding, String> {
    let root = super::tests::ts_unique_tempdir(label)?;
    let write =
        |path: &str, contents: &str| super::tests::ts_write_file(&root.join(path), contents);
    write(
        "package.json",
        r#"{"name":"pkg","scripts":{"test":"mocha"},"devDependencies":{"mocha":"^12.0.0","chai":"^6.0.0"}}"#,
    )?;
    write("src/label.ts", new_source)?;
    write("test/label.test.ts", test_source)?;
    let changed_files = crate::analysis::diff::parse_unified_diff(diff);
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let result = TypeScriptAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("no finding on line {line}"))
}

/// `to.throw("blank")` is a substring check that both messages satisfy, so
/// the guard reads it broad on the block's second line, as it already did
/// for a one-line replacement (corpus case
/// `mocha-label-throw-second-replaced-line`).
#[test]
fn second_line_of_a_replaced_block_reaches_the_message_guard() -> Result<(), String> {
    let changed = crate::analysis::diff::parse_unified_diff(TWO_LINE_DIFF);
    assert_eq!(changed.len(), 1, "fixture must parse one file");
    assert_eq!(
        changed[0]
            .replaced_line_counterpart(5)
            .map(|line| line.text.trim()),
        Some("throw new Error(\"label must not be blank\");"),
        "fixture: the second added line pairs with the second removed line"
    );
    let finding = finding_on(
        "replaced-block-broad",
        LABEL_EDITED,
        &chai_test("\"blank\""),
        TWO_LINE_DIFF,
        5,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// The control: the same block with a test pinning the old message, which
/// the new message no longer includes, keeps the exact credit (corpus case
/// `mocha-title-throw-second-replaced-line`).
#[test]
fn second_line_of_a_replaced_block_keeps_a_check_that_tells_the_messages_apart()
-> Result<(), String> {
    let finding = finding_on(
        "replaced-block-pinned",
        LABEL_EDITED,
        &chai_test("\"label must not be blank\""),
        TWO_LINE_DIFF,
        5,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// An uneven block pairs by content: the added `throw` pairs with the
/// removed `throw`, which here is also the block's first removed line, and
/// the guard still reads the substring check broad.
#[test]
fn uneven_block_keeps_the_first_line_match() -> Result<(), String> {
    let new_source = "export function parseLabel(s: string): string {\n  const label = s.trim();\n  if (label === \"\") {\n    throw new Error(\"label is blank\");\n  }\n  return label;\n}\n";
    let diff = "diff --git a/src/label.ts b/src/label.ts\n--- a/src/label.ts\n+++ b/src/label.ts\n@@ -2,6 +2,5 @@ export function parseLabel(s: string): string {\n   const label = s.trim();\n   if (label === \"\") {\n-    throw new Error(\"label must not be blank\");\n-    // refuse an empty label\n+    throw new Error(\"label is blank\");\n   }\n   return label;\n }\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff);
    assert_eq!(changed.len(), 1, "fixture must parse one file");
    assert!(
        changed[0].replaced_line_counterpart(4).is_none(),
        "fixture: an uneven block has no positional counterpart"
    );
    assert_eq!(
        changed[0]
            .aligned_counterparts()
            .get(&4)
            .map(|line| line.text.trim()),
        Some("throw new Error(\"label must not be blank\");"),
        "fixture: the content pairing finds the removed `throw`"
    );
    let finding = finding_on(
        "replaced-block-uneven",
        new_source,
        &chai_test("\"blank\""),
        diff,
        4,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// `-a -b +x -c +y` (a hand-written or tool-generated diff; git groups a
/// block's removals first): `c` sits inside the added run, so `y` is not
/// `b`'s replacement. The same-coordinate match pairs `y` with `c`, the
/// `throw` it rewrote, and the guard reads the substring check broad. With
/// `b` as its old line the change is not message-only and kept strong
/// credit.
#[test]
fn a_removed_line_inside_the_run_keeps_the_same_coordinate_match() -> Result<(), String> {
    let new_source = "export function parseLabel(s: string): string {\n  const label = s.trim();\n  if (label === \"\") {\n    console.warn(\"blank\");\n    throw new Error(\"label is blank\");\n  }\n  return label;\n}\n";
    let diff = "diff --git a/src/label.ts b/src/label.ts\n--- a/src/label.ts\n+++ b/src/label.ts\n@@ -2,8 +2,7 @@ export function parseLabel(s: string): string {\n   const label = s.trim();\n   if (label === \"\") {\n-    console.log(\"blank\");\n-    console.log(\"again\");\n+    console.warn(\"blank\");\n-    throw new Error(\"label must not be blank\");\n+    throw new Error(\"label is blank\");\n   }\n   return label;\n }\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff);
    assert_eq!(changed.len(), 1, "fixture must parse one file");
    assert_eq!(
        changed[0]
            .removed_lines
            .iter()
            .map(|line| line.new_side_line)
            .collect::<Vec<_>>(),
        vec![4, 4, 5],
        "fixture: the third removed line sits inside the added run 4..=5"
    );
    assert!(changed[0].replaced_line_counterpart(5).is_none());
    let finding = finding_on(
        "replaced-block-interleaved",
        new_source,
        &chai_test("\"blank\""),
        diff,
        5,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// Corpus case `mocha-label-throw-uneven-shorter-block`: the comment above
/// the `throw` is deleted in the same block, so the removed comment is the
/// line recorded at the added `throw`'s coordinate. The content pairing
/// reads the removed `throw` instead, and the guard reads the substring
/// check broad. Before, the `throw` was compared with the comment and kept
/// exact credit.
#[test]
fn the_throw_of_an_uneven_block_pairs_with_the_removed_throw() -> Result<(), String> {
    let changed = crate::analysis::diff::parse_unified_diff(UNEVEN_SHORTER_DIFF);
    assert_eq!(changed.len(), 1, "fixture must parse one file");
    assert_eq!(
        changed[0]
            .aligned_counterparts()
            .get(&4)
            .map(|line| line.text.trim()),
        Some("throw new Error(\"label must not be blank\");"),
        "fixture: the added `throw` pairs with the removed `throw`"
    );
    let finding = finding_on(
        "replaced-block-uneven-shorter",
        UNEVEN_SHORTER_SOURCE,
        &chai_test("\"blank\""),
        UNEVEN_SHORTER_DIFF,
        4,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// The control (corpus case `mocha-title-throw-uneven-shorter-block`): a
/// test pinning the old message keeps the exact credit.
#[test]
fn the_throw_of_an_uneven_block_keeps_a_check_that_tells_the_messages_apart() -> Result<(), String>
{
    let finding = finding_on(
        "replaced-block-uneven-pinned",
        UNEVEN_SHORTER_SOURCE,
        &chai_test("\"label must not be blank\""),
        UNEVEN_SHORTER_DIFF,
        4,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// Corpus case `mocha-label-throw-uneven-longer-block`: two removed lines
/// meet three added lines and the `throw` is the third, so neither position
/// nor coordinate finds its old line; content does.
#[test]
fn the_throw_of_a_longer_uneven_block_pairs_with_the_removed_throw() -> Result<(), String> {
    let new_source = "export function parseLabel(s: string): string {\n  const label = s.trim();\n  if (label === \"\") {\n    // refuse a blank label\n    // (whitespace alone counts as blank)\n    throw new Error(\"label is blank\");\n  }\n  return label;\n}\n";
    let diff = "diff --git a/src/label.ts b/src/label.ts\n--- a/src/label.ts\n+++ b/src/label.ts\n@@ -2,6 +2,7 @@ export function parseLabel(s: string): string {\n   const label = s.trim();\n   if (label === \"\") {\n-    // refuse an empty label\n-    throw new Error(\"label must not be blank\");\n+    // refuse a blank label\n+    // (whitespace alone counts as blank)\n+    throw new Error(\"label is blank\");\n   }\n   return label;\n }\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff);
    assert_eq!(changed.len(), 1, "fixture must parse one file");
    assert_eq!(
        changed[0]
            .aligned_counterparts()
            .get(&6)
            .map(|line| line.text.trim()),
        Some("throw new Error(\"label must not be blank\");"),
        "fixture: the added `throw` pairs with the removed `throw`"
    );
    let finding = finding_on(
        "replaced-block-uneven-longer",
        new_source,
        &chai_test("\"blank\""),
        diff,
        6,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

const UNEVEN_SHORTER_SOURCE: &str = "export function parseLabel(s: string): string {\n  const label = s.trim();\n  if (label === \"\") {\n    throw new Error(\"label is blank\");\n  }\n  return label;\n}\n";

/// The comment and the `throw` below it are removed; one `throw` is added.
const UNEVEN_SHORTER_DIFF: &str = "diff --git a/src/label.ts b/src/label.ts\n--- a/src/label.ts\n+++ b/src/label.ts\n@@ -2,6 +2,5 @@ export function parseLabel(s: string): string {\n   const label = s.trim();\n   if (label === \"\") {\n-    // refuse an empty label\n-    throw new Error(\"label must not be blank\");\n+    throw new Error(\"label is blank\");\n   }\n   return label;\n }\n";

/// Review of #7554: `+// note -throw old +throw new`. The run starts with a
/// pure insertion, and the removed `throw` is recorded at the added
/// `throw`'s coordinate; the guard reads it and the substring check stays
/// broad, as on main.
#[test]
fn a_throw_after_an_inserted_comment_keeps_its_coordinate_match() -> Result<(), String> {
    let new_source = "export function parseLabel(s: string): string {\n  const label = s.trim();\n  if (label === \"\") {\n    // refuse a blank label\n    throw new Error(\"label is blank\");\n  }\n  return label;\n}\n";
    let diff = "diff --git a/src/label.ts b/src/label.ts\n--- a/src/label.ts\n+++ b/src/label.ts\n@@ -2,5 +2,6 @@ export function parseLabel(s: string): string {\n   const label = s.trim();\n   if (label === \"\") {\n+    // refuse a blank label\n-    throw new Error(\"label must not be blank\");\n+    throw new Error(\"label is blank\");\n   }\n   return label;\n }\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff);
    assert_eq!(changed.len(), 1, "fixture must parse one file");
    assert_eq!(
        changed[0]
            .aligned_counterparts()
            .get(&5)
            .map(|line| line.text.trim()),
        Some("throw new Error(\"label must not be blank\");"),
        "fixture: the removed `throw` is recorded at the added `throw`"
    );
    let finding = finding_on(
        "replaced-block-insertion-first",
        new_source,
        &chai_test("\"blank\""),
        diff,
        5,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}
