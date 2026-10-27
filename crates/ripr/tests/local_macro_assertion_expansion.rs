//! A test-local `macro_rules!` assertion wrapper that expands exactly is read
//! as the assertion it stands for (#10650; RIPR-SPEC-0120). The parity
//! controls are the load-bearing ones: a wrapped assertion must classify
//! exactly like the same assertion written out, so expansion can only move a
//! finding to where the hand-written form already sits, never past it. Every
//! shape that does not expand exactly keeps the macro limitation.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-local-macro-{}-{}",
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

const MANIFEST: &str =
    "[package]\nname = \"local_macro\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The changed line is always line 3: `base + base / 10` becomes
/// `base + (base / 10)`.
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn surcharge(base: u32) -> u32 {\n-    base + base / 10\n+    base + (base / 10)\n }\n";

fn source(definition: &str, body: &str) -> String {
    format!(
        "pub fn surcharge(base: u32) -> u32 {{\n    base + (base / 10)\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{definition}\n    #[test]\n    fn pins_the_surcharge() {{\n        {body}\n    }}\n}}\n"
    )
}

fn finding(source: &str) -> Result<Value, String> {
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
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "return_value")
        .cloned()
        .ok_or_else(|| format!("no return_value finding: {json}"))
}

fn class(finding: &Value) -> &str {
    finding["classification"].as_str().unwrap_or_default()
}

const CHECK_EQ: &str = "    macro_rules! check_eq {\n        ($actual:expr, $expected:expr) => { assert_eq!($actual, $expected) };\n    }\n";

#[test]
fn an_exactly_expandable_wrapper_reads_as_its_assertion() -> Result<(), String> {
    let finding = finding(&source(CHECK_EQ, "check_eq!(surcharge(100), 110);"))?;
    assert_eq!(class(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn a_wrapped_assertion_classifies_exactly_like_the_written_out_one() -> Result<(), String> {
    for (shape, macro_call, written) in [
        (
            "exact value",
            "check_eq!(surcharge(100), 110);",
            "assert_eq!(surcharge(100), 110);",
        ),
        (
            "wrong-input exact value",
            "check_eq!(surcharge(0), 0);",
            "assert_eq!(surcharge(0), 0);",
        ),
        (
            "compound argument",
            "check_eq!(surcharge(50 + 50), 100 + 10);",
            "assert_eq!(surcharge(50 + 50), 100 + 10);",
        ),
    ] {
        let wrapped = finding(&source(CHECK_EQ, macro_call))?;
        let direct = finding(&source("", written))?;
        assert_eq!(class(&wrapped), class(&direct), "{shape}: {wrapped}");
        assert_eq!(
            wrapped["ripr"]["discriminate"]["state"], direct["ripr"]["discriminate"]["state"],
            "{shape}: {wrapped}"
        );
    }
    Ok(())
}

#[test]
fn shapes_that_do_not_expand_exactly_keep_the_macro_limitation() -> Result<(), String> {
    for (shape, definition, call) in [
        (
            "two arms",
            "    macro_rules! check_eq {\n        ($a:expr) => { assert_eq!($a, 110) };\n        ($a:expr, $b:expr) => { assert_eq!($a, $b) };\n    }\n",
            "check_eq!(surcharge(100), 110);",
        ),
        (
            "a literal fragment",
            "    macro_rules! check_eq {\n        ($a:expr, $b:literal) => { assert_eq!($a, $b) };\n    }\n",
            "check_eq!(surcharge(100), 110);",
        ),
        (
            "several statements",
            "    macro_rules! check_eq {\n        ($a:expr, $b:expr) => {{ let got = $a; assert_eq!(got, $b); }};\n    }\n",
            "check_eq!(surcharge(100), 110);",
        ),
        (
            "a tautology after substitution",
            CHECK_EQ,
            "check_eq!(surcharge(100), surcharge(100));",
        ),
    ] {
        let finding = finding(&source(definition, call))?;
        assert_ne!(class(&finding), "exposed", "{shape}: {finding}");
        assert_eq!(
            finding["static_limit_kind"], "rust_macro_wrapped_assertion_unresolved",
            "{shape}: {finding}"
        );
    }
    Ok(())
}
