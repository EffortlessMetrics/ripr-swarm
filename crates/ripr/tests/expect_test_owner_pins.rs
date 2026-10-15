//! Public-API controls for expect-test owner pins (#6578, #7856 review):
//! an `expect![..].assert_eq(&owner(..))` comparison behind a
//! manifest-verified `use expect_test::expect;` pins the owner's return in
//! every spelling the parser accepts, as does a golden-file
//! `expect_file!["path"].assert_eq(&owner(..))` behind a verified
//! `use expect_test::expect_file;`, and every other expect-test or snapshot
//! shape keeps its medium snapshot grade. Each case writes a small
//! crate, changes the owner's `format!` tail, and reads the return-value
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
            "ripr-expect-test-pins-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(path.join("tests")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const EXPECT_TEST: &str = "[dev-dependencies]\nexpect-test = \"1\"\n";
const INSTA: &str = "[dev-dependencies]\ninsta = \"1\"\n";

/// The return-value finding for a change of `old` to `new` on line 2 of
/// `pub fn render(x: u32) -> String { .. }`, with `test` as the body of the
/// one test in `tests/render.rs` under `imports` and `dev` dependencies.
fn render_finding(
    old: &str,
    new: &str,
    dev: &str,
    imports: &str,
    attributes: &str,
    test: &str,
) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let manifest = format!(
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n{dev}"
    );
    let lib = format!("pub fn render(x: u32) -> String {{\n    {new}\n}}\n");
    let tests = format!(
        "use demo::render;\n{imports}\n\n#[test]\n{attributes}fn renders() {{\n    {test}\n}}\n"
    );
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-    {old}\n+    {new}\n"
    );
    std::fs::write(scratch.0.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("tests/render.rs"), tests).map_err(|error| error.to_string())?;
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
        .find(|finding| finding["probe"]["family"] == "return_value")
        .cloned()
        .ok_or_else(|| format!("no return_value finding: {json}"))
}

const OLD: &str = "format!(\"w{}\", x * 2)";
const NEW: &str = "format!(\"w{}\", x * 3)";
const IMPORT: &str = "use expect_test::expect;";

#[test]
fn every_parsed_inline_expect_spelling_pins_the_owner() -> Result<(), String> {
    for test in [
        "expect![[\"w12\"]].assert_eq(&render(4));",
        "expect![\"w12\"].assert_eq(&render(4));",
        "expect! [[\"w12\"]].assert_eq(&render(4));",
        "expect![[\"w12\"]].assert_eq (&render(4));",
        "expect![[r#\"w12\"#]].assert_eq(&render(4));",
        "expect![[\"w12\"]]\n        .assert_eq(&render(4));",
        // The standard assertion is the control the pin reads like.
        "assert_eq!(render(4), \"w12\");",
    ] {
        let finding = render_finding(OLD, NEW, EXPECT_TEST, IMPORT, "", test)?;
        assert_eq!(finding["classification"], "exposed", "{test}: {finding}");
    }
    // An empty expectation pins an owner whose changed tail returns "".
    let finding = render_finding(
        "format!(\"{}\", \"w\".repeat(x as usize * 1))",
        "format!(\"{}\", \"w\".repeat(x as usize * 0))",
        EXPECT_TEST,
        IMPORT,
        "",
        "expect![[]].assert_eq(&render(4));",
    )?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn other_expect_test_and_snapshot_shapes_keep_their_medium_grade() -> Result<(), String> {
    let pinned = "expect![[\"w12\"]].assert_eq(&render(4));";
    for (dev, imports, attributes, test) in [
        // No manifest dependency verifies the import.
        ("", IMPORT, "", pinned),
        // A renamed package behind the `expect_test` name.
        (
            "[dev-dependencies]\nexpect-test = { package = \"fake-expect\", version = \"1\" }\n",
            IMPORT,
            "",
            pinned,
        ),
        // A path-qualified macro without the verified import.
        (
            EXPECT_TEST,
            "",
            "",
            "expect_test::expect![[\"w12\"]].assert_eq(&render(4));",
        ),
        // A test-file macro shadows the import.
        (
            EXPECT_TEST,
            "use expect_test::expect;\nmacro_rules! expect { ($($t:tt)*) => { expect_test::expect!($($t)*) }; }",
            "",
            pinned,
        ),
        // A test that passes by panicking.
        (EXPECT_TEST, IMPORT, "#[should_panic]\n", pinned),
        // A comparison nested where it may never run.
        (
            EXPECT_TEST,
            IMPORT,
            "",
            "let _ok = render(1).is_empty() || { expect![[\"w12\"]].assert_eq(&render(4)); true };",
        ),
        // The Debug comparisons and an insta snapshot.
        (
            EXPECT_TEST,
            IMPORT,
            "",
            "expect![[r#\"\"w12\"\"#]].assert_debug_eq(&render(4));",
        ),
        (
            EXPECT_TEST,
            FILE_IMPORT,
            "",
            "expect_file![\"w.txt\"].assert_debug_eq(&render(4));",
        ),
        (
            INSTA,
            "",
            "",
            "insta::assert_snapshot!(render(4), @\"w12\");",
        ),
    ] {
        let finding = render_finding(OLD, NEW, dev, imports, attributes, test)?;
        // The nested comparison sits under a deciding `||`, so it earns
        // no snapshot grade at all.
        let expected = if test.starts_with("let _ok") {
            "reachable_unrevealed"
        } else {
            "weakly_exposed"
        };
        assert_eq!(
            finding["classification"], expected,
            "{imports} {attributes}{test}: {finding}"
        );
    }
    Ok(())
}

const FILE_IMPORT: &str = "use expect_test::expect_file;";
const GOLDEN: &str = "expect_file![\"w.txt\"].assert_eq(&render(4));";

/// #6578: `ExpectFile::assert_eq` compares the actual text with the whole
/// committed file and panics on a mismatch, so a verified golden-file
/// comparison pins the owner's return as the inline one does. The file is
/// never read; whatever it holds, a changed return fails the comparison.
#[test]
fn a_verified_golden_file_comparison_pins_the_owner() -> Result<(), String> {
    for (imports, test) in [
        (FILE_IMPORT, GOLDEN),
        ("use expect_test::{expect, expect_file};", GOLDEN),
        (
            FILE_IMPORT,
            "expect_file![\"w.txt\"]\n        .assert_eq(&render(4));",
        ),
    ] {
        let finding = render_finding(OLD, NEW, EXPECT_TEST, imports, "", test)?;
        assert_eq!(
            finding["classification"], "exposed",
            "{imports} {test}: {finding}"
        );
    }
    for (dev, imports, attributes, test) in [
        // No manifest dependency verifies the import.
        ("", FILE_IMPORT, "", GOLDEN),
        // A renamed package behind the `expect_test` name.
        (
            "[dev-dependencies]\nexpect-test = { package = \"fake-expect\", version = \"1\" }\n",
            FILE_IMPORT,
            "",
            GOLDEN,
        ),
        // Some other crate's `expect_file!`.
        (
            "[dev-dependencies]\nexpect-test = \"1\"\nother = \"1\"\n",
            "use other::expect_file;",
            "",
            GOLDEN,
        ),
        // A path-qualified macro without the verified import.
        (
            EXPECT_TEST,
            "",
            "",
            "expect_test::expect_file![\"w.txt\"].assert_eq(&render(4));",
        ),
        // A renamed import, and a glob.
        (
            EXPECT_TEST,
            "use expect_test::expect_file as golden;",
            "",
            "golden![\"w.txt\"].assert_eq(&render(4));",
        ),
        (EXPECT_TEST, "use expect_test::*;", "", GOLDEN),
        // A test-file macro shadows the import.
        (
            EXPECT_TEST,
            "use expect_test::expect_file;\nmacro_rules! expect_file { ($($t:tt)*) => { expect_test::expect_file!($($t)*) }; }",
            "",
            GOLDEN,
        ),
        // A test that passes by panicking.
        (EXPECT_TEST, FILE_IMPORT, "#[should_panic]\n", GOLDEN),
        // A computed path.
        (
            EXPECT_TEST,
            FILE_IMPORT,
            "",
            "expect_file![concat!(\"w\", \".txt\")].assert_eq(&render(4));",
        ),
        // #6578 review: a golden the test writes from the owner first, or
        // lets expect-test rewrite, fixes nothing.
        (
            EXPECT_TEST,
            FILE_IMPORT,
            "",
            "std::fs::write(\"tests/w.txt\", render(4)).unwrap();\n    expect_file![\"w.txt\"].assert_eq(&render(4));",
        ),
        (
            EXPECT_TEST,
            "use expect_test::expect_file;\nuse std::fs::write;",
            "",
            "write(\"tests/w.txt\", render(4)).unwrap();\n    expect_file![\"w.txt\"].assert_eq(&render(4));",
        ),
        (
            EXPECT_TEST,
            FILE_IMPORT,
            "",
            "unsafe { std::env::set_var(\"UPDATE_EXPECT\", \"1\") };\n    expect_file![\"w.txt\"].assert_eq(&render(4));",
        ),
    ] {
        let finding = render_finding(OLD, NEW, dev, imports, attributes, test)?;
        // A renamed macro is not recognized as an expect-test comparison
        // at all, so it earns no snapshot grade either.
        let expected = if test.starts_with("golden!") {
            "reachable_unrevealed"
        } else {
            "weakly_exposed"
        };
        assert_eq!(
            finding["classification"], expected,
            "{imports} {attributes}{test}: {finding}"
        );
    }
    Ok(())
}

/// #7856 review: a lone format string with a captured identifier is a
/// plain `format!` owner tail, as a tuple of arguments is.
#[test]
fn a_captured_identifier_format_tail_propagates() -> Result<(), String> {
    for (old, new) in [
        ("format!(\"v{x}\")", "format!(\"w{x}\")"),
        ("format!(\"v{}\", x)", "format!(\"w{}\", x)"),
    ] {
        let finding = render_finding(old, new, "", "", "", "assert_eq!(render(4), \"w4\");")?;
        assert_eq!(finding["classification"], "exposed", "{new}: {finding}");
    }
    // A lone non-literal argument is not a plain format tail.
    let finding = render_finding(
        "format!(concat!(\"v{x}\"))",
        "format!(concat!(\"w{x}\"))",
        "",
        "",
        "",
        "assert_eq!(render(4), \"w4\");",
    )?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}
