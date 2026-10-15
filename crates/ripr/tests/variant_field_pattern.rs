//! Public-API controls for literal variant-field patterns (#6578): an
//! `assert_matches!` on the owner's result whose pattern binds the changed
//! field of the owner's one `ParseError::BadDigit { .. }` literal to a
//! literal pins that field. A pattern that leaves the field open, guards
//! it, or matches another value does not. Each case writes a small crate,
//! changes line 13 of `src/lib.rs`, and reads the field-construction
//! finding through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-variant-field-pattern-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // The guard owns only a directory this call created, and owns it
        // before anything is written inside.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const OLD: &str = "        let d = c.to_digit(10).ok_or(ParseError::BadDigit { pos: i })?;";
const NEW: &str = "        let d = c.to_digit(10).ok_or(ParseError::BadDigit { pos: 0 + i })?;";

/// The field-construction finding for the `pos` change, with `assertions`
/// as the body of the one test, `items` after `parse_num` and `test_items`
/// in the test module.
fn pos_finding(assertions: &str, items: &str, test_items: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let manifest = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[dev-dependencies]\nassert_matches = \"1.5\"\n";
    let lib = format!(
        "#[derive(Debug, PartialEq)]\npub enum ParseError {{\n    Empty,\n    BadDigit {{ pos: usize }},\n}}\n\npub fn parse_num(s: &str) -> Result<u32, ParseError> {{\n    if s.is_empty() {{\n        return Err(ParseError::Empty);\n    }}\n    let mut acc: u32 = 0;\n    for (i, c) in s.chars().enumerate() {{\n{NEW}\n        acc = acc * 10 + d;\n    }}\n    Ok(acc)\n}}\n{items}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    use assert_matches::assert_matches;\n{test_items}\n    #[test]\n    fn bad_digit_reports_position() {{\n        {assertions}\n    }}\n}}\n"
    );
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -13,1 +13,1 @@\n-{OLD}\n+{NEW}\n"
    );
    std::fs::write(scratch.0.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "field_construction")
        .cloned()
        .ok_or_else(|| format!("no field_construction finding: {json}"))
}

#[test]
fn a_literal_variant_field_pattern_on_the_owner_result_is_exposed() -> Result<(), String> {
    // Each pattern alone pins the field.
    for assertions in [
        "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));",
        "assert_matches!(crate::parse_num(\"1x\"), Err(ParseError::BadDigit { pos: 1 }));",
    ] {
        let finding = pos_finding(assertions, "", "")?;
        assert_eq!(finding["oracle_kind"], "exact_error_variant", "{finding}");
        assert_eq!(
            finding["classification"], "exposed",
            "{assertions}: {finding}"
        );
    }
    let finding = pos_finding(
        "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));\n        assert_matches!(parse_num(\"1x\"), Err(ParseError::BadDigit { pos: 1 }));",
        "",
        "",
    )?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn a_pattern_that_leaves_the_field_open_stays_unconfirmed() -> Result<(), String> {
    for assertions in [
        "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { .. }));",
        "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: _ }));",
        "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: p }) if p < 5);",
        "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 | 1 }));",
        "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0..5 }));",
        // A foreign namesake, with no local rival to resolve against.
        "let _ = parse_num(\"x1\");\n        assert_matches!(other::parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));",
        "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0.. }));",
    ] {
        let finding = pos_finding(assertions, "", "")?;
        assert_ne!(
            finding["classification"], "exposed",
            "{assertions}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn a_variant_other_production_code_builds_stays_unconfirmed() -> Result<(), String> {
    // `parse_num` is not the only production site that spells a
    // `BadDigit {` head, so the pattern alone does not say which
    // initializer it observed: a helper, a `const` or a macro body.
    for items in [
        "\npub fn first_bad() -> ParseError {\n    ParseError::BadDigit { pos: 0 }\n}\n",
        "\npub const FIRST_BAD: ParseError = ParseError::BadDigit { pos: 0 };\n",
        "\nmacro_rules! bad {\n    ($p:expr) => {\n        ParseError::BadDigit { pos: $p }\n    };\n}\n",
    ] {
        let finding = pos_finding(
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));",
            items,
            "",
        )?;
        assert_ne!(finding["classification"], "exposed", "{items}: {finding}");
    }
    Ok(())
}

#[test]
fn a_test_side_build_or_input_stays_unconfirmed() -> Result<(), String> {
    for (assertions, test_items) in [
        // A test-module helper builds the variant and might feed the owner.
        (
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));\n        let _ = expected;",
            "\n    fn expected() -> ParseError {\n        ParseError::BadDigit { pos: 0 }\n    }\n",
        ),
        // The owner reads a value the test holds, not a literal.
        (
            "let input = \"x1\";\n        assert_matches!(parse_num(input), Err(ParseError::BadDigit { pos: 0 }));",
            "",
        ),
        // A `#[should_panic]` test passes when the pattern fails.
        (
            "let _ = 0;",
            "\n    #[test]\n    #[should_panic]\n    fn wrong_position_panics() {\n        assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 7 }));\n    }\n",
        ),
    ] {
        let finding = pos_finding(assertions, "", test_items)?;
        assert_ne!(
            finding["classification"], "exposed",
            "{assertions}{test_items}: {finding}"
        );
    }
    Ok(())
}
