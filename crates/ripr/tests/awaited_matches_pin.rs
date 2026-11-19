//! Literal `matches!` owner-return pins on an async owner (#15397;
//! RIPR-SPEC-0197). `assert!(matches!(owner(..).await, Ok(8)))` pins the
//! awaited value as `assert_eq!(owner(..).await, Ok(8))` does, so it reads
//! like its synchronous twin. A wildcard pattern stays weak, and a bare call
//! of an async owner (its future) is no pin.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-awaited-matches-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"billing\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[dev-dependencies]\ntokio = { version = \"1\", features = [\"macros\", \"rt\"] }\n";

/// The return-value finding for `Ok(x * 2)` -> `Ok(x * 3)` in `fee`
/// (`async` when `asynchronous`), with `tests` as `tests/it.rs`.
fn return_finding(asynchronous: bool, tests: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let write = |path: &str, text: &str| -> Result<(), String> {
        let target = scratch.0.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, text).map_err(|error| error.to_string())
    };
    let keyword = if asynchronous { "async " } else { "" };
    let signature = format!("pub {keyword}fn fee(x: u32) -> Result<u32, String> {{");
    write("Cargo.toml", MANIFEST)?;
    write("src/lib.rs", &format!("{signature}\n    Ok(x * 3)\n}}\n"))?;
    write("tests/it.rs", tests)?;
    write(
        "change.diff",
        &format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n {signature}\n-    Ok(x * 2)\n+    Ok(x * 3)\n }}\n"
        ),
    )?;
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

/// The class, after checking the finding names `fee_ok` in `tests/it.rs`
/// with `assertion` as its oracle, so the reading belongs to the case.
fn class_for(asynchronous: bool, assertion: &str) -> Result<String, String> {
    let attribute = if asynchronous {
        "#[tokio::test]\nasync fn"
    } else {
        "#[test]\nfn"
    };
    let tests = format!("{attribute} fee_ok() {{\n    {assertion}\n}}\n");
    let finding = return_finding(asynchronous, &tests)?;
    let names_case = finding["related_tests"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|test| {
            test["file"] == "tests/it.rs"
                && test["name"] == "fee_ok"
                && test["oracle"].as_str() == Some(assertion)
        });
    assert!(
        names_case,
        "premise: {assertion} is the related oracle: {finding}"
    );
    Ok(finding["classification"]
        .as_str()
        .unwrap_or_default()
        .to_string())
}

#[test]
fn an_awaited_literal_matches_pin_on_an_async_owner_is_credited() -> Result<(), String> {
    assert_eq!(
        class_for(true, "assert!(matches!(billing::fee(4).await, Ok(8)));")?,
        "exposed"
    );
    // The `assert_eq!` twin was already credited; the forms now agree.
    assert_eq!(
        class_for(true, "assert_eq!(billing::fee(4).await, Ok(8));")?,
        "exposed"
    );
    Ok(())
}

#[test]
fn the_synchronous_matches_pin_keeps_its_credit() -> Result<(), String> {
    assert_eq!(
        class_for(false, "assert!(matches!(billing::fee(4), Ok(8)));")?,
        "exposed"
    );
    Ok(())
}

#[test]
fn an_awaited_wildcard_matches_stays_weak() -> Result<(), String> {
    assert_eq!(
        class_for(true, "assert!(matches!(billing::fee(4).await, Ok(_)));")?,
        "weakly_exposed"
    );
    Ok(())
}

#[test]
fn a_bare_call_of_an_async_owner_in_matches_is_no_pin() -> Result<(), String> {
    // Without `.await` the operand is the owner's future, not its result.
    assert_ne!(
        class_for(true, "assert!(matches!(billing::fee(4), Ok(8)));")?,
        "exposed"
    );
    Ok(())
}

#[test]
fn an_awaited_pin_on_a_literal_neither_version_returns_is_not_credited() -> Result<(), String> {
    // `fee(4)` is `Ok(8)` before and `Ok(12)` after, so `Ok(99)` fails on
    // both versions and notices nothing (#12716); `.await` must not hide it.
    for assertion in [
        "assert!(matches!(fee(4).await, Ok(99)));",
        "assert_eq!(fee(4).await, Ok(99));",
    ] {
        let tests = format!(
            "use billing::fee;\n\n#[tokio::test]\nasync fn fee_ok() {{\n    {assertion}\n}}\n"
        );
        let finding = return_finding(true, &tests)?;
        assert_ne!(
            finding["classification"], "exposed",
            "{assertion}: {finding}"
        );
    }
    // Control: the same import form with the baseline value is credited.
    let tests = "use billing::fee;\n\n#[tokio::test]\nasync fn fee_ok() {\n    assert!(matches!(fee(4).await, Ok(8)));\n}\n";
    assert_eq!(return_finding(true, tests)?["classification"], "exposed");
    Ok(())
}
