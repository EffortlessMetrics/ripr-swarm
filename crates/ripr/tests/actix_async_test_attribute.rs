//! `#[actix_rt::test]` and `#[actix_web::test]` define an async test whose
//! body runs to completion on an Actix `System` (#14073). Each expands to a
//! `#[test]` that blocks on the body, so an exact assertion in it pins a
//! changed owner as it does under `#[tokio::test]`. Before the fix neither
//! path was a test-defining attribute: the function was never registered and
//! the owner read `no_static_path`.
//!
//! Each case writes a small crate, changes the owner's boundary line, and
//! reads both findings through `check_workspace`. The admission is the
//! crate-qualified path only, on an `async fn`, with no arguments and the
//! crate name not rebound in the test's module; every other shape keeps its
//! earlier reading and credits nothing.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-actix-async-test-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"actix_async_test\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The changed owner line is `qty >= 10` (line 2).
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn over(qty: u32) -> bool {\n-    qty > 10\n+    qty >= 10\n }\n";
const OWNER: &str = "pub fn over(qty: u32) -> bool {\n    qty >= 10\n}\n";
const ASYNC_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub async fn over(qty: u32) -> bool {\n-    qty > 10\n+    qty >= 10\n }\n";
const ASYNC_OWNER: &str = "pub async fn over(qty: u32) -> bool {\n    qty >= 10\n}\n";

/// Both sides of the changed boundary, pinned with exact values.
const PINS: &str = "        assert_eq!(over(10), true);\n        assert_eq!(over(9), false);";
const AWAITED_PINS: &str =
    "        assert_eq!(over(10).await, true);\n        assert_eq!(over(9).await, false);";

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
fn the_fixture_pins_the_owner_under_the_tokio_control() -> Result<(), String> {
    // The same body under `#[tokio::test]` is the reading the Actix
    // attributes must match; it also proves the fixture's diff and owner
    // line up, so a later `exposed` is not an artifact of the harness.
    let control = source(OWNER, "", "#[tokio::test]\n    async fn subject()", PINS);
    assert_eq!(readings(&control, DIFF)?, exposed_by_subject());
    Ok(())
}

#[test]
fn an_actix_async_test_pins_the_owner_like_a_tokio_test() -> Result<(), String> {
    let mut mismatched = Vec::new();
    for attribute in [
        "#[actix_rt::test]",
        "#[::actix_rt::test]",
        "#[actix_rt::test()]",
        "#[actix_web::test]",
        "#[::actix_web::test]",
        "#[actix_rt::test]\n    #[serial_test::serial]",
    ] {
        let header = format!("{attribute}\n    async fn subject()");
        let reading = readings(&source(OWNER, "", &header, PINS), DIFF)?;
        if reading != exposed_by_subject() {
            mismatched.push((attribute, reading));
        }
    }
    assert!(mismatched.is_empty(), "not exposed: {mismatched:?}");
    Ok(())
}

#[test]
fn an_actix_async_test_awaits_an_async_owner() -> Result<(), String> {
    for attribute in ["#[actix_rt::test]", "#[actix_web::test]"] {
        let header = format!("{attribute}\n    async fn subject()");
        let reading = readings(&source(ASYNC_OWNER, "", &header, AWAITED_PINS), ASYNC_DIFF)?;
        assert_eq!(reading, exposed_by_subject(), "{attribute}");
    }
    Ok(())
}

#[test]
fn a_weak_assertion_under_an_actix_test_stays_weak() -> Result<(), String> {
    // The attribute admits the body; it does not strengthen the oracle.
    let header = "#[actix_rt::test]\n    async fn subject()";
    let body = "        assert!(over(10) || true);";
    let [predicate, return_value] = readings(&source(OWNER, "", header, body), DIFF)?;
    assert_eq!(predicate.0, "weakly_exposed");
    assert_eq!(return_value.0, "weakly_exposed");
    Ok(())
}

#[test]
fn shapes_that_may_not_run_the_body_credit_nothing() -> Result<(), String> {
    // An argument swaps the runtime, a sync `fn` does not compile under the
    // macro, a local `actix_rt` shadows the crate, `#[ignore]` skips the
    // test, and the remaining paths are not the admitted attributes.
    let mut credited = Vec::new();
    for (case, preamble, header) in [
        (
            "system argument",
            "",
            "#[actix_rt::test(system = \"other::System\")]\n    async fn subject()",
        ),
        ("sync fn", "", "#[actix_rt::test]\n    fn subject()"),
        (
            "shadowed crate",
            "    mod actix_rt { pub use other::test; }",
            "#[actix_rt::test]\n    async fn subject()",
        ),
        (
            "renamed import",
            "    use other as actix_web;",
            "#[actix_web::test]\n    async fn subject()",
        ),
        (
            "ignored",
            "",
            "#[actix_rt::test]\n    #[ignore]\n    async fn subject()",
        ),
        (
            "prefix lookalike",
            "",
            "#[actix_rt::test_helper]\n    async fn subject()",
        ),
        (
            "longer path",
            "",
            "#[actix_web::rt::test]\n    async fn subject()",
        ),
        ("other crate", "", "#[actix::test]\n    async fn subject()"),
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
