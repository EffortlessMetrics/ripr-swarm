//! `#[test_log::test]` defines a test whose unchanged body runs after the
//! macro's logging initializer, under libtest's `#[test]` or under the inner
//! attribute it is given. Before the fix the path was not a test-defining
//! attribute: the function was never registered and an owner it pins with
//! exact values read `no_static_path`.
//!
//! Each case writes a small crate, changes the owner's boundary line, and
//! reads both findings through `check_workspace`. The admission is the
//! crate-qualified path only: bare on a sync `fn`, or exactly
//! `#[test_log::test(tokio::test)]` on an `async fn`, with neither crate
//! name rebound in the test's module. Every other shape credits nothing.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-test-log-attribute-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"test_log_attribute\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

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

#[test]
fn the_fixture_pins_the_owner_under_the_builtin_controls() -> Result<(), String> {
    // The same body under `#[test]` and `#[tokio::test]` is the reading the
    // `test_log` forms must match; it also proves the fixture's diff and
    // owner line up.
    let sync = source(OWNER, "", "#[test]\n    fn subject()", PINS);
    assert_eq!(readings(&sync, DIFF)?, exposed_by_subject());
    let tokio = source(OWNER, "", "#[tokio::test]\n    async fn subject()", PINS);
    assert_eq!(readings(&tokio, DIFF)?, exposed_by_subject());
    Ok(())
}

#[test]
fn a_test_log_test_pins_the_owner_like_the_attribute_it_wraps() -> Result<(), String> {
    let mut mismatched = Vec::new();
    for header in [
        "#[test_log::test]\n    fn subject()",
        "#[::test_log::test]\n    fn subject()",
        "#[test_log::test()]\n    fn subject()",
        "#[test_log::test]\n    #[serial_test::serial]\n    fn subject()",
        "#[test_log::test(tokio::test)]\n    async fn subject()",
        "#[test_log::test(::tokio::test)]\n    async fn subject()",
        "#[test_log::test( tokio :: test )]\n    async fn subject()",
    ] {
        let reading = readings(&source(OWNER, "", header, PINS), DIFF)?;
        if reading != exposed_by_subject() {
            mismatched.push((header, reading));
        }
    }
    assert!(mismatched.is_empty(), "not exposed: {mismatched:?}");
    Ok(())
}

#[test]
fn a_weak_assertion_under_a_test_log_test_stays_weak() -> Result<(), String> {
    // The attribute admits the body; it does not strengthen the oracle.
    let header = "#[test_log::test]\n    fn subject()";
    let body = "        assert!(over(10) || true);";
    let [predicate, return_value] = readings(&source(OWNER, "", header, body), DIFF)?;
    assert_eq!(predicate.0, "weakly_exposed");
    assert_eq!(return_value.0, "weakly_exposed");
    Ok(())
}

#[test]
fn shapes_that_may_not_run_the_body_credit_nothing() -> Result<(), String> {
    // A sync `fn` under an async inner attribute and an `async fn` under the
    // default one do not compile; an inner attribute with arguments or of
    // another crate is not read; a shadowed crate name may be another macro;
    // `#[ignore]` skips the test; a `#[test_log(..)]` configuration
    // attribute is not admitted.
    let mut credited = Vec::new();
    for (case, preamble, header) in [
        (
            "async under default",
            "",
            "#[test_log::test]\n    async fn subject()",
        ),
        (
            "sync under tokio",
            "",
            "#[test_log::test(tokio::test)]\n    fn subject()",
        ),
        (
            "inner arguments",
            "",
            "#[test_log::test(tokio::test(flavor = \"multi_thread\"))]\n    async fn subject()",
        ),
        (
            "other inner attribute",
            "",
            "#[test_log::test(other::test)]\n    async fn subject()",
        ),
        (
            "inner path with a longer tail",
            "",
            "#[test_log::test(tokio::test::test)]\n    async fn subject()",
        ),
        (
            "shadowed test_log",
            "    mod test_log { pub use other::test; }",
            "#[test_log::test]\n    fn subject()",
        ),
        (
            "renamed test_log",
            "    use other as test_log;",
            "#[test_log::test]\n    fn subject()",
        ),
        (
            "shadowed tokio",
            "    mod tokio { pub use other::test; }",
            "#[test_log::test(tokio::test)]\n    async fn subject()",
        ),
        (
            "ignored",
            "",
            "#[test_log::test]\n    #[ignore]\n    fn subject()",
        ),
        (
            "configuration attribute",
            "",
            "#[test_log::test]\n    #[test_log(default_log_filter = \"info\")]\n    fn subject()",
        ),
        (
            "prefix lookalike",
            "",
            "#[test_log::test_helper]\n    fn subject()",
        ),
        ("longer path", "", "#[test_log::rt::test]\n    fn subject()"),
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
