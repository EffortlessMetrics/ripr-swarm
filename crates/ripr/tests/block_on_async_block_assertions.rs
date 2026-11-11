//! Exact assertions inside `block_on(async { .. })` in a synchronous test
//! (#14106). A crate-qualified `block_on` free function polls the block to
//! completion on the test thread before the call returns, so a failing
//! assertion in the block fails the test. Before the fix every `async` block
//! was refused, and a test that pins both sides of a changed boundary read
//! the same as one whose block is never polled.
//!
//! Each case writes a small crate, changes the owner's boundary line, and
//! reads both findings through `check_workspace`. Only the listed
//! crate-qualified paths are admitted; a bare or method `block_on`, a
//! shadowed crate name, a block that can exit early and a block nothing
//! polls keep their earlier reading and credit nothing.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-block-on-async-block-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"block_on_async_block\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The changed owner line is `qty >= 10` (line 2).
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn over(qty: u32) -> bool {\n-    qty > 10\n+    qty >= 10\n }\n";
const OWNER: &str = "pub fn over(qty: u32) -> bool {\n    qty >= 10\n}\n";

/// Both sides of the changed boundary, pinned with exact values.
const PINS: &str = "assert_eq!(over(10), true);\n            assert_eq!(over(9), false);";

/// `preamble` is module-level source before the test; `body` is the body of
/// the synchronous `#[test] fn subject()`.
fn source(preamble: &str, body: &str) -> String {
    format!(
        "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{preamble}\n    #[test]\n    fn subject() {{\n{body}\n    }}\n}}\n"
    )
}

/// The pins inside an `async` block handed to `call` (`{}` is the block).
fn blocked_on(call: &str) -> String {
    let block = format!("async {{\n            {PINS}\n        }}");
    format!("        {};", call.replace("{}", &block))
}

/// `(classification, related test names)` for the `predicate` and
/// `return_value` findings, in that order.
type Reading = (String, Vec<String>);

fn readings(source: &str) -> Result<[Reading; 2], String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, DIFF).map_err(|error| error.to_string())?;
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
fn the_fixture_pins_the_owner_when_the_asserts_sit_in_the_test_body() -> Result<(), String> {
    // The same assertions directly in the body are the reading an admitted
    // `block_on` must match; this also proves the diff and owner line up.
    let control = source("", &format!("        {}", PINS.replace("    ", "")));
    assert_eq!(readings(&control)?, exposed_by_subject());
    Ok(())
}

#[test]
fn a_crate_qualified_block_on_runs_the_async_block() -> Result<(), String> {
    let mut mismatched = Vec::new();
    for call in [
        "futures::executor::block_on({})",
        "::futures::executor::block_on({})",
        "futures_executor::block_on({})",
        "tokio_test::block_on({})",
        "pollster::block_on({})",
        "smol::block_on({})",
        "async_std::task::block_on({})",
    ] {
        let reading = readings(&source("", &blocked_on(call)))?;
        if reading != exposed_by_subject() {
            mismatched.push((call, reading));
        }
    }
    assert!(mismatched.is_empty(), "not exposed: {mismatched:?}");
    Ok(())
}

#[test]
fn a_block_that_may_not_run_to_its_assertions_credits_nothing() -> Result<(), String> {
    // Nothing polls the first two blocks; the rest hand the block to a
    // `block_on` whose identity or completion source cannot show.
    let never_polled = format!(
        "        let _pending = async {{\n            {PINS}\n        }};"
    );
    let dropped = format!(
        "        drop(async {{\n            {PINS}\n        }});"
    );
    let early_exit = "        let _: Option<()> = futures::executor::block_on(async {\n            None::<u32>?;\n            assert_eq!(over(10), true);\n            assert_eq!(over(9), false);\n            Some(())\n        });".to_string();
    let mut credited = Vec::new();
    for (case, preamble, body) in [
        ("never polled", "", never_polled),
        ("dropped", "", dropped),
        ("bare block_on", "", blocked_on("block_on({})")),
        (
            "bare imported block_on",
            "    use futures::executor::block_on;",
            blocked_on("block_on({})"),
        ),
        (
            "partially qualified",
            "    use futures::executor;",
            blocked_on("executor::block_on({})"),
        ),
        (
            "method on a runtime value",
            "",
            blocked_on("rt().block_on({})"),
        ),
        (
            "local counterfeit module",
            "    mod futures { pub mod executor { pub fn block_on<F>(_f: F) {} } }",
            blocked_on("futures::executor::block_on({})"),
        ),
        (
            "renamed import",
            "    use other as pollster;",
            blocked_on("pollster::block_on({})"),
        ),
        (
            "unlisted path",
            "",
            blocked_on("futures::executor::block_on_stream({})"),
        ),
        (
            "second argument",
            "",
            blocked_on("smol::block_on({}, 1)"),
        ),
        ("block owns a `?`", "", early_exit),
    ] {
        for (class, _) in readings(&source(preamble, &body))? {
            if class == "exposed" || class == "weakly_exposed" {
                credited.push((case, class));
            }
        }
    }
    assert!(credited.is_empty(), "credited: {credited:?}");
    Ok(())
}

#[test]
fn a_block_on_inside_an_async_test_stays_refused() -> Result<(), String> {
    // A nested executor may refuse to start inside a running runtime, so the
    // admission is for synchronous tests only.
    let source = format!(
        "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[tokio::test]\n    async fn subject() {{\n{}\n    }}\n}}\n",
        blocked_on("tokio_test::block_on({})")
    );
    for (class, _) in readings(&source)? {
        assert_ne!(class, "exposed");
    }
    Ok(())
}

#[test]
fn a_weak_assertion_inside_block_on_stays_weak() -> Result<(), String> {
    // The admission runs the block; it does not strengthen the oracle.
    let body = "        futures::executor::block_on(async {\n            assert!(over(10) || true);\n        });";
    let [predicate, return_value] = readings(&source("", body))?;
    assert_eq!(predicate.0, "weakly_exposed");
    assert_eq!(return_value.0, "weakly_exposed");
    Ok(())
}
