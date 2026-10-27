//! Public-API controls for the `#[test_case(args => literal)]` row
//! (RIPR-SPEC-0231, harness-compared return).
//!
//! The harness compares the function's returned value with the literal, so a
//! clean row is read as `assert_eq!(<returned expression>, <literal>)`. Every
//! control below is the credited twin with one thing changed, so each negative
//! names the exact condition that withholds the credit.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-test-case-arrow-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"arrow\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The owner sits on line 2; the rewrite changes what it returns.
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,1 +2,1 @@
-    x + 1
+    x + 2
";

const IMPORT: &str = "    use test_case::test_case;\n";
const ROWS: &str = "    #[test_case(10 => 12)]\n    #[test_case(20 => 22)]\n";
const BODY: &str = "    fn rows(x: i64) -> i64 {\n        bump(x)\n    }\n";

fn lib(imports: &str, tests: &str) -> String {
    format!(
        "pub fn bump(x: i64) -> i64 {{\n    x + 2\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{imports}\n{tests}}}\n"
    )
}

fn classes(imports: &str, tests: &str) -> Result<Vec<ExposureClass>, String> {
    let found: Vec<ExposureClass> = TempRepo::create(&lib(imports, tests), DIFF)?
        .findings()?
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .map(|finding| finding.class)
        .collect();
    if found.is_empty() {
        return Err("premise: a finding on the rewritten line".to_string());
    }
    Ok(found)
}

fn credited(imports: &str, tests: &str) -> Result<bool, String> {
    Ok(classes(imports, tests)?.contains(&ExposureClass::Exposed))
}

fn require_withheld(label: &str, imports: &str, tests: &str) -> Result<(), String> {
    let found = classes(imports, tests)?;
    if found.contains(&ExposureClass::Exposed) {
        return Err(format!("{label} must not be credited: {found:?}"));
    }
    Ok(())
}

#[test]
fn a_literal_row_compared_with_the_returned_owner_value_is_credited() -> Result<(), String> {
    let found = classes(IMPORT, &format!("{ROWS}{BODY}"))?;
    if !found.contains(&ExposureClass::Exposed) {
        return Err(format!("the credited twin must be exposed: {found:?}"));
    }
    Ok(())
}

#[test]
fn a_single_row_is_credited_like_two() -> Result<(), String> {
    let one = "    #[test_case(10 => 12)]\n";
    if !credited(IMPORT, &format!("{one}{BODY}"))? {
        return Err("one row must be credited".to_string());
    }
    Ok(())
}

#[test]
fn a_matcher_or_predicate_expectation_is_not_a_literal_comparison() -> Result<(), String> {
    for (label, rows) in [
        ("matches", "    #[test_case(10 => matches 12)]\n"),
        (
            "with",
            "    #[test_case(10 => with |v: i64| assert!(v > 0))]\n",
        ),
        ("panics", "    #[test_case(10 => panics \"boom\")]\n"),
        ("it", "    #[test_case(10 => it is_positive())]\n"),
    ] {
        require_withheld(label, IMPORT, &format!("{rows}{BODY}"))?;
    }
    Ok(())
}

#[test]
fn a_character_expectation_is_not_read() -> Result<(), String> {
    // A written `assert_eq!(bump(x), 'c')` earns no exact value either.
    require_withheld(
        "a character",
        IMPORT,
        &format!("    #[test_case(10 => 'c')]\n{BODY}"),
    )
}

#[test]
fn a_computed_expectation_is_not_read() -> Result<(), String> {
    for (label, rows) in [
        ("a call", "    #[test_case(10 => bump(10))]\n"),
        ("an expression", "    #[test_case(10 => 11 + 1)]\n"),
        ("a constant", "    #[test_case(10 => EXPECTED)]\n"),
    ] {
        require_withheld(label, IMPORT, &format!("{rows}{BODY}"))?;
    }
    Ok(())
}

#[test]
fn a_row_that_the_harness_does_not_compare_is_not_read() -> Result<(), String> {
    for (label, attribute) in [
        ("should_panic", "    #[should_panic]\n"),
        ("ignore", "    #[ignore]\n"),
    ] {
        require_withheld(label, IMPORT, &format!("{ROWS}{attribute}{BODY}"))?;
    }
    Ok(())
}

#[test]
fn a_body_with_statements_is_not_the_compared_value() -> Result<(), String> {
    let body = "    fn rows(x: i64) -> i64 {\n        let _ = bump(x);\n        12\n    }\n";
    require_withheld("a constant return", IMPORT, &format!("{ROWS}{body}"))?;
    let body = "    fn rows(x: i64) -> i64 {\n        let y = bump(x);\n        y\n    }\n";
    require_withheld("a bound return", IMPORT, &format!("{ROWS}{body}"))
}

#[test]
fn an_unimported_test_case_attribute_is_not_trusted() -> Result<(), String> {
    require_withheld("an unimported attribute", "", &format!("{ROWS}{BODY}"))
}

#[test]
fn a_row_argument_that_is_not_a_literal_is_not_read() -> Result<(), String> {
    let rows = "    #[test_case(compute() => 12)]\n";
    let body = "    fn rows(x: i64) -> i64 {\n        bump(x)\n    }\n";
    require_withheld("a computed argument", IMPORT, &format!("{rows}{body}"))
}

#[test]
fn a_parameter_count_that_the_row_does_not_fill_is_not_read() -> Result<(), String> {
    let body = "    fn rows(x: i64, y: i64) -> i64 {\n        bump(x)\n    }\n";
    require_withheld("a short row", IMPORT, &format!("{ROWS}{body}"))
}

#[test]
fn every_plain_literal_spelling_is_credited() -> Result<(), String> {
    for (label, rows) in [
        ("negative integer", "    #[test_case(10 => -3)]\n"),
        ("suffixed integer", "    #[test_case(10 => 12i64)]\n"),
        ("boolean", "    #[test_case(10 => true)]\n"),
        ("string", "    #[test_case(10 => \"ab\")]\n"),
        ("named row", "    #[test_case(10 => 12 ; \"ten\")]\n"),
        ("spaced attribute", "    #[test_case (10 => 12)]\n"),
    ] {
        if !credited(IMPORT, &format!("{rows}{BODY}"))? {
            return Err(format!("a {label} expectation must be credited"));
        }
    }
    Ok(())
}

#[test]
fn a_function_with_one_unreadable_row_reads_none_of_its_rows() -> Result<(), String> {
    let rows = "    #[test_case(10 => 12)]\n    #[test_case(20 => matches 22)]\n";
    require_withheld("a mixed function", IMPORT, &format!("{rows}{BODY}"))
}

#[test]
fn the_crate_qualified_attribute_is_read_like_the_imported_one() -> Result<(), String> {
    let rows = "    #[test_case::test_case(10 => 12)]\n";
    if !credited("", &format!("{rows}{BODY}"))? {
        return Err("a qualified attribute must be credited".to_string());
    }
    Ok(())
}

/// The owner sits on line 2; the rewrite scales what it returns.
const SCALE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,1 +2,1 @@
-    x * 2
+    x * 3
";

fn scale_classes(rows: &str, body: &str) -> Result<Vec<ExposureClass>, String> {
    let lib = format!(
        "pub fn scale(x: i64) -> i64 {{\n    x * 3\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{IMPORT}\n{rows}{body}}}\n"
    );
    let found: Vec<ExposureClass> = TempRepo::create(&lib, SCALE_DIFF)?
        .findings()?
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .map(|finding| finding.class)
        .collect();
    if found.is_empty() {
        return Err("premise: a finding on the rewritten line".to_string());
    }
    Ok(found)
}

const SCALE_BODY: &str = "    fn rows(x: i64) -> i64 {\n        scale(x)\n    }\n";

// Runtime label: `0 * 2 == 0 * 3`, so no input of the row can tell the edit
// apart (#10797). The plain `assert_eq!(scale(0), 0)` grades the same way.
#[test]
fn a_row_whose_input_cannot_tell_the_edit_apart_is_not_credited() -> Result<(), String> {
    let found = scale_classes("    #[test_case(0 => 0)]\n", SCALE_BODY)?;
    if found.contains(&ExposureClass::Exposed) {
        return Err(format!("a coincident row must not be credited: {found:?}"));
    }
    Ok(())
}

#[test]
fn one_discriminating_row_among_coincident_rows_is_credited() -> Result<(), String> {
    let rows = "    #[test_case(0 => 0)]\n    #[test_case(2 => 6)]\n";
    let found = scale_classes(rows, SCALE_BODY)?;
    if !found.contains(&ExposureClass::Exposed) {
        return Err(format!("the discriminating row must credit: {found:?}"));
    }
    Ok(())
}

#[test]
fn a_parameter_the_body_may_rebind_or_read_through_a_field_is_not_read() -> Result<(), String> {
    for (label, body) in [
        (
            "closure parameter",
            "    fn rows(x: i64) -> i64 {\n        Some(x).map(|x| scale(x)).unwrap_or(0)\n    }\n",
        ),
        (
            "field of the same name",
            "    fn rows(x: i64) -> i64 {\n        scale(Pair { x: 1, y: 2 }.x)\n    }\n",
        ),
        (
            "macro argument",
            "    fn rows(x: i64) -> i64 {\n        scale(dbg!(x))\n    }\n",
        ),
    ] {
        let found = scale_classes("    #[test_case(2 => 6)]\n", body)?;
        if found.contains(&ExposureClass::Exposed) {
            return Err(format!("{label} must not be credited: {found:?}"));
        }
    }
    Ok(())
}

// A negated argument keeps its sign as a method receiver: `scale((-5).abs())`
// is 15 under the edit and 10 before, so the row discriminates. Spliced bare
// (`-5.abs()` is `-(5.abs())`) the oracle would read -15 and miss it.
#[test]
fn a_negated_row_argument_keeps_its_sign_as_a_receiver() -> Result<(), String> {
    let found = scale_classes(
        "    #[test_case(-5 => 15)]\n",
        "    fn rows(x: i64) -> i64 {\n        scale(x.abs())\n    }\n",
    )?;
    if !found.contains(&ExposureClass::Exposed) {
        return Err(format!("the discriminating row must credit: {found:?}"));
    }
    Ok(())
}
