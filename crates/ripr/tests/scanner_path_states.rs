//! Public-API controls for a path-qualified scanner state compared against an
//! enum variant (RIPR-SPEC-0163 example 2, #10108). Each case writes a small
//! crate, changes the compared variant, and reads the predicate finding
//! through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-scanner-path-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"scanner_path\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

fn source(compared: &str, tests: &str) -> String {
    format!(
        "#[derive(Clone, Copy, PartialEq, Eq)]\npub enum Scan {{\n    Text,\n    Word,\n}}\n\npub fn scan_path(input: &str) -> Scan {{\n    let mut state = Scan::Text;\n    for symbol in input.chars() {{\n        state = match (state, symbol) {{\n            (Scan::Text, ' ') => Scan::Text,\n            (Scan::Text, _) => Scan::Word,\n            (Scan::Word, ' ') => Scan::Text,\n            (Scan::Word, _) => Scan::Word,\n        }};\n    }}\n    state\n}}\n\npub fn classify_path(input: &str) -> &'static str {{\n    let final_state = scan_path(input);\n    if final_state == {compared} {{\n        \"word\"\n    }} else {{\n        \"blank\"\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn classify_reads_the_last_state() {{\n{tests}\n    }}\n}}\n"
    )
}

/// The compared line is always line 22.
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -20,6 +20,6 @@\n pub fn classify_path(input: &str) -> &'static str {\n     let final_state = scan_path(input);\n-    if final_state == Scan::Word {\n+    if final_state == Scan::Text {\n         \"word\"\n     } else {\n         \"blank\"\n";

const BOTH_ROWS: &str = "        assert_eq!(classify_path(\"ab\"), \"word\");\n        assert_eq!(classify_path(\"ab \"), \"blank\");";
const WORD_ROW_ONLY: &str = "        assert_eq!(classify_path(\"ab\"), \"word\");";

fn predicate_finding(source: &str) -> Result<Value, String> {
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
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))
}

// Both states are pinned by a row, so the changed comparison is observed.
#[test]
fn variant_operand_beside_a_path_state_scanner_exposes() -> Result<(), String> {
    let finding = predicate_finding(&source("Scan::Text", BOTH_ROWS))?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

// Control: with only the word row the text state is never reached, so the
// variant operand must not manufacture an observed equality.
#[test]
fn variant_operand_does_not_credit_a_row_that_misses_the_changed_state() -> Result<(), String> {
    let finding = predicate_finding(&source("Scan::Text", WORD_ROW_ONLY))?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}
