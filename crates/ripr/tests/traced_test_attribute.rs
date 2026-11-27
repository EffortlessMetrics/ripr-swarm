//! `#[tracing_test::traced_test]` keeps a test's other attributes and runs
//! its unchanged body after installing a capturing subscriber and entering a
//! span. Before the fix the attribute was unread, so every assertion in such
//! a test was refused and a pinned owner read `static_unknown`.
//!
//! Each case writes a small crate, changes the owner's boundary line, and
//! reads both findings through `check_workspace`. The admission is the
//! crate-qualified path, or the bare name bound by exactly one unrenamed
//! `use tracing_test::traced_test` in the test's module, without arguments
//! and with `tracing_test` not rebound there. Every other shape credits
//! nothing.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-traced-test-attribute-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "[package]\nname = \"traced_test_attribute\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The changed owner line is `qty >= 10` (line 2).
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn over(qty: u32) -> bool {\n-    qty > 10\n+    qty >= 10\n }\n";
const OWNER: &str = "pub fn over(qty: u32) -> bool {\n    qty >= 10\n}\n";

/// Both sides of the changed boundary, pinned with exact values.
const PINS: &str = "        assert_eq!(over(10), true);\n        assert_eq!(over(9), false);";

/// `preamble` is module-level source before the test; `header` is the test's
/// attributes and signature.
fn source(owner: &str, preamble: &str, header: &str, body: &str) -> String {
    format!(
        "{owner}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{preamble}\n    {header} {{\n{body}\n    }}\n}}\n"
    )
}

/// `(classification, related test names)` for the `predicate` and
/// `return_value` findings, in that order.
type Reading = (String, Vec<String>);

fn readings(source: &str, diff: &str) -> Result<[Reading; 2], String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
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
    let findings = json["findings"].as_array().ok_or("missing findings")?;
    let reading = |family: &str| -> Result<Reading, String> {
        let finding = findings
            .iter()
            .find(|finding| finding["probe"]["family"] == family)
            .ok_or_else(|| format!("no {family} finding: {json}"))?;
        let class = finding["classification"]
            .as_str()
            .ok_or("missing classification")?
            .to_string();
        let mut related: Vec<String> = finding["related_tests"]
            .as_array()
            .map(|tests| {
                tests
                    .iter()
                    .filter_map(|test| test["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        related.sort();
        related.dedup();
        Ok((class, related))
    };
    Ok([reading("predicate")?, reading("return_value")?])
}

fn exposed_by_subject() -> [Reading; 2] {
    let reading = || ("exposed".to_string(), vec!["subject".to_string()]);
    [reading(), reading()]
}

const IMPORT: &str = "    use tracing_test::traced_test;";

#[test]
fn the_fixture_pins_the_owner_without_the_attribute() -> Result<(), String> {
    // The same body under `#[test]` alone is the reading the traced forms
    // must match; it also proves the fixture's diff and owner line up.
    let control = source(OWNER, "", "#[test]\n    fn subject()", PINS);
    assert_eq!(readings(&control, DIFF)?, exposed_by_subject());
    Ok(())
}

#[test]
fn a_traced_test_pins_the_owner_like_the_plain_test() -> Result<(), String> {
    let mut mismatched = Vec::new();
    for (preamble, header) in [
        (
            "",
            "#[test]\n    #[tracing_test::traced_test]\n    fn subject()",
        ),
        (
            "",
            "#[::tracing_test::traced_test]\n    #[test]\n    fn subject()",
        ),
        (IMPORT, "#[traced_test]\n    #[test]\n    fn subject()"),
        (IMPORT, "#[test]\n    #[traced_test]\n    fn subject()"),
        (
            "",
            "#[tokio::test]\n    #[tracing_test::traced_test]\n    async fn subject()",
        ),
        (
            IMPORT,
            "#[traced_test]\n    #[tokio::test]\n    async fn subject()",
        ),
    ] {
        let reading = readings(&source(OWNER, preamble, header, PINS), DIFF)?;
        if reading != exposed_by_subject() {
            mismatched.push((header, reading));
        }
    }
    assert!(mismatched.is_empty(), "not exposed: {mismatched:?}");
    Ok(())
}

#[test]
fn a_weak_assertion_under_a_traced_test_stays_weak() -> Result<(), String> {
    // The attribute admits the body; it does not strengthen the oracle.
    let header = "#[test]\n    #[tracing_test::traced_test]\n    fn subject()";
    let body = "        assert!(over(10) || true);";
    let [predicate, return_value] = readings(&source(OWNER, "", header, body), DIFF)?;
    assert_eq!(predicate.0, "weakly_exposed");
    assert_eq!(return_value.0, "weakly_exposed");
    Ok(())
}

#[test]
fn shapes_that_may_not_be_the_macro_credit_nothing() -> Result<(), String> {
    // An unbound, renamed, doubly bound, glob-imported or foreign bare name
    // may be another attribute macro; so may the path under a shadowed crate
    // name; an argument is not the macro's grammar; `#[ignore]` skips the
    // test.
    let mut credited = Vec::new();
    for (case, preamble, header) in [
        (
            "unbound bare name",
            "",
            "#[test]\n    #[traced_test]\n    fn subject()",
        ),
        (
            "renamed import",
            "    use tracing_test::traced_test as traced;",
            "#[test]\n    #[traced]\n    fn subject()",
        ),
        (
            "foreign import",
            "    use other::traced_test;",
            "#[test]\n    #[traced_test]\n    fn subject()",
        ),
        (
            "second binding",
            "    use tracing_test::traced_test;\n    use other::{traced_test};",
            "#[test]\n    #[traced_test]\n    fn subject()",
        ),
        (
            "glob import",
            "    use tracing_test::*;",
            "#[test]\n    #[traced_test]\n    fn subject()",
        ),
        (
            "argument",
            "",
            "#[test]\n    #[tracing_test::traced_test(scope)]\n    fn subject()",
        ),
        (
            "shadowed crate",
            "    mod tracing_test { pub use other::traced_test; }",
            "#[test]\n    #[tracing_test::traced_test]\n    fn subject()",
        ),
        (
            "ignored",
            IMPORT,
            "#[test]\n    #[traced_test]\n    #[ignore]\n    fn subject()",
        ),
        (
            "lookalike path",
            "",
            "#[test]\n    #[tracing_test::traced_test_helper]\n    fn subject()",
        ),
        (
            "no test attribute",
            "",
            "#[tracing_test::traced_test]\n    fn subject()",
        ),
    ] {
        for (class, _) in readings(&source(OWNER, preamble, header, PINS), DIFF)? {
            if class == "exposed" || class == "weakly_exposed" {
                credited.push((case, class));
            }
        }
    }
    assert!(credited.is_empty(), "credited: {credited:?}");
    Ok(())
}
