//! Public-API controls for inline expect-test owner pins (#6578, #7856
//! review): an `expect![..].assert_eq(&owner(..))` comparison behind a
//! manifest-verified `use expect_test::expect;` pins the owner's return in
//! every spelling the parser accepts, and every other expect-test or
//! snapshot shape keeps its medium snapshot grade. Each case writes a small
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
        // The Debug comparison, a file expectation and an insta snapshot.
        (
            EXPECT_TEST,
            IMPORT,
            "",
            "expect![[r#\"\"w12\"\"#]].assert_debug_eq(&render(4));",
        ),
        (
            EXPECT_TEST,
            "use expect_test::expect_file;",
            "",
            "expect_file![\"w.txt\"].assert_eq(&render(4));",
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
