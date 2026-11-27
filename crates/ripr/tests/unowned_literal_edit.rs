//! #13843: a copy edit of string-literal text outside every function is not
//! reported as a gap.
//!
//! A help-table row (`summary: "..."` in a `static`) has no owning function,
//! so the only tests ripr relates are the ones in the same file. The finding
//! read `weakly_exposed` beside a test that has nothing to do with the text.
//! It now reads `static_unknown` under `rust_unowned_literal_reader_unresolved`.
//!
//! Each case writes a small crate, changes one line, reads the findings on
//! that line through `check_workspace`, and checks class and limit. The
//! controls change one link at a time: code outside the literal, an added
//! line, a literal inside a function, and a test that quotes the literal.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIMIT: &str = "rust_unowned_literal_reader_unresolved";

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-unowned-literal-{}-{}",
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
    "[package]\nname = \"help_table\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

const SUMMARY: &str = "        summary: \"Write a starter config.\",";
const SUMMARY_BEFORE: &str = "        summary: \"Write a config.\",";
const WEIGHT: &str = "        weight: 2,";
const WEIGHT_BEFORE: &str = "        weight: 1,";
const BANNER: &str = "    \"ripr help\"";
const BANNER_BEFORE: &str = "    \"help\"";

/// A same-file test that never touches a summary.
const UNRELATED_ASSERTION: &str = "assert_eq!(ids(), vec![\"init\", \"check\"]);";
/// A same-file test that pins the changed summary whole.
const QUOTING_ASSERTION: &str = "assert_eq!(COMMANDS[0].summary, \"Write a starter config.\");";

fn source(assertion: &str) -> String {
    format!(
        "pub struct Command {{\n    pub id: &'static str,\n    pub summary: &'static str,\n    pub weight: u32,\n}}\n\npub static COMMANDS: &[Command] = &[\n    Command {{\n        id: \"init\",\n{SUMMARY}\n{WEIGHT}\n    }},\n    Command {{\n        id: \"check\",\n        summary: \"Analyze the diff.\",\n        weight: 1,\n    }},\n];\n\npub fn ids() -> Vec<&'static str> {{\n    COMMANDS.iter().map(|command| command.id).collect()\n}}\n\npub fn banner() -> &'static str {{\n{BANNER}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn ids_are_listed_in_order() {{\n        {assertion}\n    }}\n}}\n"
    )
}

/// Every finding on the line holding `after`, for a diff that replaces
/// `before` with it (or only adds it when `before` is `None`).
fn findings(source: &str, before: Option<&str>, after: &str) -> Result<Vec<Value>, String> {
    let line = source
        .lines()
        .position(|text| text == after)
        .map(|index| index + 1)
        .ok_or("changed line not in source")?;
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let hunk = match before {
        Some(before) => format!("@@ -{line},1 +{line},1 @@\n-{before}\n+{after}\n"),
        None => format!("@@ -{},0 +{line},1 @@\n+{after}\n", line - 1),
    };
    let diff =
        format!("diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n{hunk}");
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
    let on_line: Vec<Value> = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .filter(|finding| finding["probe"]["line"].as_u64() == Some(line as u64))
        .cloned()
        .collect();
    if on_line.is_empty() {
        return Err(format!("no finding on line {line}: {json}"));
    }
    Ok(on_line)
}

fn class(finding: &Value) -> &str {
    finding["classification"].as_str().unwrap_or("")
}

fn limit(finding: &Value) -> Option<&str> {
    finding["static_limit_kind"].as_str()
}

fn relations(finding: &Value) -> Vec<&str> {
    finding["related_tests"]
        .as_array()
        .map(|tests| {
            tests
                .iter()
                .filter_map(|test| test["relation_reason"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_help_table_copy_edit_is_withheld_not_a_gap() -> Result<(), String> {
    let found = findings(&source(UNRELATED_ASSERTION), Some(SUMMARY_BEFORE), SUMMARY)?;
    for finding in &found {
        assert_eq!(class(finding), "static_unknown", "{finding}");
        assert_eq!(limit(finding), Some(LIMIT), "{finding}");
        // The rule only withholds: reach is still the weak same-file reading
        // and the neighbour is still listed, as a place to look.
        assert_eq!(finding["ripr"]["reach"]["state"], "weak", "{finding}");
        assert_eq!(relations(finding), ["same_test_file"], "{finding}");
        let next = finding["recommended_next_step"].as_str().unwrap_or("");
        assert!(next.contains("golden file"), "{next}");
        assert!(
            finding["missing_discriminators"]
                .as_array()
                .is_none_or(Vec::is_empty),
            "{finding}"
        );
    }
    Ok(())
}

#[test]
fn each_missing_link_keeps_its_own_reading() -> Result<(), String> {
    let unrelated = source(UNRELATED_ASSERTION);
    // A changed number in the same table is code: the same shape, the same
    // same-file relation, and today's gap reading.
    let number = findings(&unrelated, Some(WEIGHT_BEFORE), WEIGHT)?;
    // An added row has no base side to compare.
    let added = findings(&unrelated, None, SUMMARY)?;
    for finding in number.iter().chain(&added) {
        assert_eq!(class(finding), "weakly_exposed", "{finding}");
        assert_eq!(finding["ripr"]["reach"]["state"], "weak", "{finding}");
        assert_eq!(limit(finding), None, "{finding}");
    }
    // A literal inside a function has an owner a test could call.
    for finding in &findings(&unrelated, Some(BANNER_BEFORE), BANNER)? {
        assert_eq!(class(finding), "no_static_path", "{finding}");
        assert_eq!(limit(finding), None, "{finding}");
    }
    // A test that quotes the changed literal whole reaches it.
    let quoted = findings(&source(QUOTING_ASSERTION), Some(SUMMARY_BEFORE), SUMMARY)?;
    for finding in &quoted {
        assert_eq!(finding["ripr"]["reach"]["state"], "yes", "{finding}");
        assert_eq!(
            relations(finding),
            ["assertion_target_affinity"],
            "{finding}"
        );
        assert_ne!(class(finding), "static_unknown", "{finding}");
        assert_eq!(limit(finding), None, "{finding}");
    }
    assert!(
        quoted.iter().any(|finding| class(finding) == "exposed"),
        "{quoted:?}"
    );
    Ok(())
}
