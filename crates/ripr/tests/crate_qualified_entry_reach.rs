//! An integration test that calls a tested wrapper through the package's own
//! library name (`qual_reach::order_discount(..)`, no `use`) is related to the
//! wrapper's private helper like the imported spelling is (#8999). Every
//! other qualifier stays unrelated (fail closed). Each case writes a small
//! package, changes the helper's boundary line, and reads the predicate
//! finding's related tests through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-crate-qualified-{}-{}",
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

const MANIFEST: &str =
    "[package]\nname = \"qual_reach\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The changed helper is on line 2 in every library source below.
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn is_bulk(qty: u32) -> bool {\n-    qty >= 10\n+    10 <= qty\n }\n";

const LIB: &str = "fn is_bulk(qty: u32) -> bool {\n    10 <= qty\n}\n\npub fn order_discount(qty: u32) -> u32 {\n    if is_bulk(qty) { 5 } else { 0 }\n}\n";

/// The helper and its wrapper live in an inline module, so the wrapper is not
/// a crate-root item.
const NESTED_LIB: &str = "mod inner {\n    fn is_bulk(qty: u32) -> bool {\n        10 <= qty\n    }\n\n    pub fn order_discount(qty: u32) -> u32 {\n        if is_bulk(qty) { 5 } else { 0 }\n    }\n}\npub use inner::order_discount;\n";
const NESTED_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,3 +2,3 @@\n     fn is_bulk(qty: u32) -> bool {\n-        qty >= 10\n+        10 <= qty\n     }\n";

fn test_file(body: &str) -> String {
    format!("#[test]\nfn discount_boundary() {{\n{body}\n}}\n")
}

const QUALIFIED: &str = "    assert_eq!(qual_reach::order_discount(10), 5);\n    assert_eq!(qual_reach::order_discount(9), 0);";

/// The relation reason of `discount_boundary` on the predicate finding, or
/// `None` when the test is not a related test of it.
fn relation(lib: &str, diff: &str, tests: &str) -> Result<Option<String>, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("tests/api.rs"), tests).map_err(|error| error.to_string())?;
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
    let finding = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))?;
    Ok(finding["related_tests"]
        .as_array()
        .and_then(|related| {
            related
                .iter()
                .find(|test| test["name"] == "discount_boundary")
        })
        .and_then(|test| test["relation_reason"].as_str().map(str::to_string)))
}

#[test]
fn imported_spelling_is_the_linked_baseline() -> Result<(), String> {
    let tests = format!(
        "use qual_reach::order_discount;\n{}",
        test_file("    assert_eq!(order_discount(10), 5);\n    assert_eq!(order_discount(9), 0);")
    );
    assert_eq!(
        relation(LIB, DIFF, &tests)?.as_deref(),
        Some("helper_owner_call")
    );
    Ok(())
}

#[test]
fn own_crate_qualified_entry_call_links() -> Result<(), String> {
    assert_eq!(
        relation(LIB, DIFF, &test_file(QUALIFIED))?.as_deref(),
        Some("helper_owner_call")
    );
    Ok(())
}

#[test]
fn another_crate_qualifier_does_not_link() -> Result<(), String> {
    let tests = test_file(&QUALIFIED.replace("qual_reach::", "other_crate::"));
    assert_eq!(relation(LIB, DIFF, &tests)?, None);
    Ok(())
}

#[test]
fn a_longer_path_does_not_link() -> Result<(), String> {
    let tests = test_file(&QUALIFIED.replace("qual_reach::", "qual_reach::util::"));
    assert_eq!(relation(LIB, DIFF, &tests)?, None);
    Ok(())
}

#[test]
fn a_test_module_named_like_the_crate_does_not_link() -> Result<(), String> {
    let tests = format!(
        "mod qual_reach {{\n    pub fn order_discount(_: u32) -> u32 {{ 5 }}\n}}\n{}",
        test_file(QUALIFIED)
    );
    assert_eq!(relation(LIB, DIFF, &tests)?, None);
    Ok(())
}

#[test]
fn a_renamed_import_of_the_crate_name_does_not_link() -> Result<(), String> {
    let tests = format!("use other_crate as qual_reach;\n{}", test_file(QUALIFIED));
    assert_eq!(relation(LIB, DIFF, &tests)?, None);
    Ok(())
}

#[test]
fn an_entry_outside_the_crate_root_does_not_link() -> Result<(), String> {
    let tests = test_file(QUALIFIED);
    assert_eq!(relation(NESTED_LIB, NESTED_DIFF, &tests)?, None);
    Ok(())
}
