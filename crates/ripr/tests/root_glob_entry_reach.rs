//! An integration test that imports its library through one root glob
//! (`use money::*;`) and calls a crate-root entry is related to that entry the
//! way `use money::cash_total;` is (#15073). The glob delivers the entry only
//! when it is the only glob in the test file, spells exactly `<lib>::*`, and
//! the entry is defined at the crate root or re-exported there once.
//!
//! Each case writes a small package, changes a private helper behind the
//! public `cash_total` entry, and reads the return-value finding through
//! `check_workspace`. Every refusal differs from the linking positive by one
//! stated thing, so it cannot pass for an unrelated reason.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        // The workspace redirects the temp directory into `target/`, which a
        // fresh worktree may not hold yet.
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let path = root.join(format!(
            "ripr-root-glob-entry-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation: a colliding directory is never reused or deleted.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let target = self.0.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, content).map_err(|error| error.to_string())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str =
    "[package]\nname = \"money\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The helper and its public entry, as written in `src/lib.rs` or `src/m.rs`.
const ENTRY: &str = "fn round_to_nickel(cents: u32) -> u32 {\n    (cents + 2) / 5 * 5\n}\n\npub fn cash_total(cents: u32) -> u32 {\n    round_to_nickel(cents)\n}\n";

/// The diff changing the helper's body line in `file`.
fn diff(file: &str) -> String {
    format!(
        "diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n@@ -1,3 +1,3 @@\n fn round_to_nickel(cents: u32) -> u32 {{\n-    cents / 5 * 5\n+    (cents + 2) / 5 * 5\n }}\n"
    )
}

/// `tests/total.rs`: `prelude` lines, then a test that pins the entry at an
/// input where the old and new helper differ (103 gives 100, then 105).
fn test_file(prelude: &str) -> String {
    format!("{prelude}\n#[test]\nfn rounds_up() {{\n    assert_eq!(cash_total(103), 105);\n}}\n")
}

struct Outcome {
    related: bool,
    classification: String,
}

fn outcome_of(owner_file: &str, files: &[(&str, &str)]) -> Result<Outcome, String> {
    let scratch = Scratch::create()?;
    scratch.write("Cargo.toml", MANIFEST)?;
    for (path, content) in files {
        scratch.write(path, content)?;
    }
    scratch.write("change.diff", &diff(owner_file))?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
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
        .find(|finding| finding["probe"]["family"] == "return_value")
        .cloned()
        .ok_or_else(|| format!("no return_value finding: {json}"))?;
    // The intended subject: the helper's changed line, not a neighbour.
    let file = finding["probe"]["file"].as_str().unwrap_or_default();
    let expression = finding["probe"]["expression"].as_str().unwrap_or_default();
    if !file.ends_with(owner_file) || expression != "(cents + 2) / 5 * 5" {
        return Err(format!(
            "return_value finding is `{expression}` in `{file}`, not the helper"
        ));
    }
    Ok(Outcome {
        related: finding["related_tests"]
            .as_array()
            .is_some_and(|tests| tests.iter().any(|test| test["name"] == "rounds_up")),
        classification: finding["classification"]
            .as_str()
            .ok_or("missing classification")?
            .to_string(),
    })
}

/// The entry defined at the crate root, reached through `prelude`.
fn at_root(prelude: &str) -> Result<Outcome, String> {
    let tests = test_file(prelude);
    outcome_of(
        "src/lib.rs",
        &[("src/lib.rs", ENTRY), ("tests/total.rs", &tests)],
    )
}

/// The entry defined in `src/m.rs` under the root `lib`, reached through
/// `prelude`.
fn in_module(lib: &str, prelude: &str) -> Result<Outcome, String> {
    let tests = test_file(prelude);
    outcome_of(
        "src/m.rs",
        &[
            ("src/lib.rs", lib),
            ("src/m.rs", ENTRY),
            ("tests/total.rs", &tests),
        ],
    )
}

#[test]
fn a_named_import_of_the_root_entry_is_the_linked_baseline() -> Result<(), String> {
    let outcome = at_root("use money::cash_total;\n")?;
    assert!(outcome.related);
    assert_eq!(outcome.classification, "exposed");
    Ok(())
}

#[test]
fn a_lone_root_glob_links_a_root_entry() -> Result<(), String> {
    let outcome = at_root("use money::*;\n")?;
    assert!(outcome.related);
    assert_eq!(outcome.classification, "exposed");
    Ok(())
}

#[test]
fn a_lone_root_glob_links_an_entry_the_root_re_exports() -> Result<(), String> {
    let outcome = in_module("mod m;\npub use m::cash_total;\n", "use money::*;\n")?;
    assert!(outcome.related);
    assert_eq!(outcome.classification, "exposed");
    Ok(())
}

#[test]
fn a_second_glob_may_bring_another_cash_total_and_does_not_link() -> Result<(), String> {
    let outcome = at_root("use money::*;\nuse std::cmp::*;\n")?;
    assert_ne!(outcome.classification, "exposed");
    assert!(!outcome.related);
    Ok(())
}

#[test]
fn a_glob_of_a_library_module_does_not_link() -> Result<(), String> {
    let outcome = in_module("pub mod m;\n", "use money::m::*;\n")?;
    assert_ne!(outcome.classification, "exposed");
    Ok(())
}

#[test]
fn a_root_glob_does_not_deliver_an_entry_the_root_does_not_re_export() -> Result<(), String> {
    let outcome = in_module("pub mod m;\n", "use money::*;\n")?;
    assert_ne!(outcome.classification, "exposed");
    assert!(!outcome.related);
    Ok(())
}

#[test]
fn a_test_file_definition_of_the_entry_name_shadows_the_glob() -> Result<(), String> {
    let outcome =
        at_root("use money::*;\n\nfn cash_total(cents: u32) -> u32 {\n    cents + 2\n}\n")?;
    assert_ne!(outcome.classification, "exposed");
    Ok(())
}
