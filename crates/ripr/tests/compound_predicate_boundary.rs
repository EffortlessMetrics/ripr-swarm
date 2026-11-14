//! Public-API controls for the equality boundary ripr names on a changed
//! `&&`/`||` predicate (#9637).
//!
//! Comparison binds tighter than `&&` and `||`, so `s.starts_with('-') &&
//! s.len() > 1` compares `s.len()` with `1`. The boundary reading once split
//! the whole text at `>` and asked for a test where
//! `s.starts_with('-') && s.len()` equals `1`, which no input can satisfy.
//! The reading now uses only the one comparison operand the edit changed.
//! Every case writes a small crate and reads the predicate finding through
//! `check_workspace`.
//!
//! Static evidence only: no case here claims a runtime mutation outcome.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-compound-predicate-boundary-{}-{}",
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

/// The predicate finding for `owner` (whose line 2 changed from `before` to
/// `after`) with `body` as the body of the one unit test beside it.
fn predicate(signature: &str, before: &str, after: &str, body: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let manifest =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
    let lib = format!(
        "{signature} {{\n    {after}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn t() {{\n        {body}\n    }}\n}}\n"
    );
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-    {before}\n+    {after}\n"
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
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))
}

fn missing(finding: &Value) -> Vec<String> {
    finding["missing_discriminators"]
        .as_array()
        .map(|facts| {
            facts
                .iter()
                .map(|fact| format!("{} | {}", fact["value"], fact["reason"]))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_added_length_conjunct_never_names_the_whole_conjunction_as_its_operand() -> Result<(), String>
{
    let finding = predicate(
        "pub fn is_flag(s: &str) -> bool",
        "s.starts_with('-')",
        "s.starts_with('-') && s.len() > 1",
        "assert_eq!(is_flag(\"-\"), false);\n        assert_eq!(is_flag(\"-v\"), true);",
    )?;
    let missing = missing(&finding);
    assert!(
        missing.iter().all(|fact| !fact.contains("&&")),
        "{missing:?}: {finding}"
    );
    assert!(
        finding["evidence_path"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|line| line.as_str().unwrap_or_default()
                == "source function argument value s.len() == 1 at line 11"),
        "the `\"-\"` input is read at the `s.len()` boundary: {finding}"
    );
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn a_changed_equality_beside_an_untouched_gate_names_only_that_equality() -> Result<(), String> {
    let finding = predicate(
        "pub fn gated(label: &str, on: bool) -> bool",
        "label == \"alpha\" && on",
        "label == \"beta\" && on",
        "assert_eq!(gated(\"beta\", false), false);",
    )?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    let missing = missing(&finding);
    assert!(
        missing.iter().all(|fact| !fact.contains("&&")),
        "{missing:?}: {finding}"
    );
    Ok(())
}

/// A sibling operand that every input leaves false masks the changed
/// comparison, so an input at its boundary is not a discriminator.
#[test]
fn an_input_at_the_boundary_behind_a_false_gate_is_not_exposed() -> Result<(), String> {
    for (signature, before, after, body) in [
        (
            "pub fn gated(label: &str, on: bool) -> bool",
            "label == \"alpha\" && on",
            "label == \"beta\" && on",
            "assert_eq!(gated(\"beta\", false), false);",
        ),
        (
            "pub fn big(x: u32, on: bool) -> bool",
            "x > 5 && on",
            "x >= 5 && on",
            "assert_eq!(big(5, false), false);",
        ),
    ] {
        let finding = predicate(signature, before, after, body)?;
        assert_ne!(finding["classification"], "exposed", "{after}: {finding}");
    }
    Ok(())
}
