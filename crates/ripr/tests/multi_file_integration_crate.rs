//! Owner-return pins from a multi-file integration test crate (#15327;
//! RIPR-SPEC-0197). Cargo autodiscovers `tests/NAME/main.rs` as the root of
//! integration target `NAME`, with its modules beside it. A test there that
//! pins the changed owner's exact result reads like the same test in
//! `tests/NAME.rs`; a crate that rebinds the library name, a weak assertion
//! and a module shared by several integration roots keep their readings.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-multi-file-it-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str =
    "[package]\nname = \"billing\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
const OWNER: &str = "pub fn fee(x: u32) -> u32 {\n    x * 3\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn fee(x: u32) -> u32 {\n-    x * 2\n+    x * 3\n }\n";
const PIN: &str = "#[test]\nfn fee_doubles() {\n    assert_eq!(billing::fee(4), 8);\n}\n";

/// The return-value finding for `x * 2` -> `x * 3` in `fee`, with `files`
/// written beside the library.
fn return_finding(files: &[(&str, &str)]) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let write = |path: &str, text: &str| -> Result<(), String> {
        let target = scratch.0.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, text).map_err(|error| error.to_string())
    };
    write("Cargo.toml", MANIFEST)?;
    write("src/lib.rs", OWNER)?;
    for (path, text) in files {
        write(path, text)?;
    }
    write("change.diff", DIFF)?;
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
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "return_value")
        .cloned()
        .ok_or_else(|| format!("no return_value finding: {json}"))
}

fn class_of(finding: &Value) -> &str {
    finding["classification"].as_str().unwrap_or_default()
}

/// The related test rows the finding names, as `file name` pairs, so a
/// reading is tied to the test the case wrote rather than to any test.
fn related_tests(finding: &Value) -> Vec<String> {
    finding["related_tests"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|test| {
            format!(
                "{} {}",
                test["file"].as_str().unwrap_or_default(),
                test["name"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

fn assert_reading(files: &[(&str, &str)], class: &str, test_file: &str) -> Result<(), String> {
    let finding = return_finding(files)?;
    assert_eq!(class_of(&finding), class, "{files:?}: {finding}");
    assert_names_test(&finding, test_file)
}

/// A rebinding crate must not credit the pin; how far the reading falls
/// (`weakly_exposed` or `reachable_unrevealed`) is not this case's claim.
fn assert_not_credited(files: &[(&str, &str)], test_file: &str) -> Result<(), String> {
    let finding = return_finding(files)?;
    assert_ne!(class_of(&finding), "exposed", "{files:?}: {finding}");
    assert_names_test(&finding, test_file)
}

fn assert_names_test(finding: &Value, test_file: &str) -> Result<(), String> {
    let related = related_tests(finding);
    assert!(
        related
            .iter()
            .any(|row| row.starts_with(&format!("{test_file} "))
                && (row.ends_with(" fee_doubles") || row.ends_with(" fee_runs"))),
        "premise: the finding names the written test in {test_file}: {related:?}"
    );
    Ok(())
}

#[test]
fn a_flat_integration_test_is_credited_as_the_control() -> Result<(), String> {
    assert_reading(&[("tests/it.rs", PIN)], "exposed", "tests/it.rs")
}

#[test]
fn a_pin_in_a_multi_file_integration_root_is_credited() -> Result<(), String> {
    assert_reading(&[("tests/it/main.rs", PIN)], "exposed", "tests/it/main.rs")
}

#[test]
fn a_pin_in_a_module_of_a_multi_file_integration_root_is_credited() -> Result<(), String> {
    assert_reading(
        &[
            ("tests/it/main.rs", "mod fees;\n"),
            ("tests/it/fees.rs", PIN),
        ],
        "exposed",
        "tests/it/fees.rs",
    )?;
    assert_reading(
        &[
            ("tests/it/main.rs", "mod api;\n"),
            ("tests/it/api/mod.rs", "mod fees;\n"),
            ("tests/it/api/fees.rs", PIN),
        ],
        "exposed",
        "tests/it/api/fees.rs",
    )
}

#[test]
fn a_multi_file_integration_crate_that_rebinds_the_library_name_is_not_credited()
-> Result<(), String> {
    // `crate::billing` is the crate's own module, which returns 8 for any
    // input, so the assertion never reads the library's `fee`.
    assert_not_credited(
        &[
            ("tests/it/main.rs", "mod billing;\nmod pins;\n"),
            (
                "tests/it/billing.rs",
                "pub fn fee(_x: u32) -> u32 {\n    8\n}\n",
            ),
            ("tests/it/pins.rs", &format!("use crate::billing;\n\n{PIN}")),
        ],
        "tests/it/pins.rs",
    )?;
    assert_not_credited(
        &[(
            "tests/it/main.rs",
            &format!(
                "mod billing {{\n    pub fn fee(_x: u32) -> u32 {{\n        8\n    }}\n}}\n\n{PIN}"
            ),
        )],
        "tests/it/main.rs",
    )
}

#[test]
fn a_weak_assertion_in_a_multi_file_integration_crate_stays_weak() -> Result<(), String> {
    assert_reading(
        &[
            ("tests/it/main.rs", "mod fees;\n"),
            (
                "tests/it/fees.rs",
                "#[test]\nfn fee_runs() {\n    assert!(billing::fee(4) > 0);\n}\n",
            ),
        ],
        "weakly_exposed",
        "tests/it/fees.rs",
    )
}

#[test]
fn a_test_module_shared_by_two_integration_roots_keeps_its_reading() -> Result<(), String> {
    // `tests/common/mod.rs` compiles into both `a` and `b`, so it has no
    // single crate root; the pin stays unconfirmed as before #15327.
    assert_reading(
        &[
            ("tests/a.rs", "mod common;\n"),
            ("tests/b.rs", "mod common;\n"),
            ("tests/common/mod.rs", PIN),
        ],
        "weakly_exposed",
        "tests/common/mod.rs",
    )
}
