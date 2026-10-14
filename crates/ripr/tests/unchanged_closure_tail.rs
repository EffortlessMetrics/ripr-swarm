//! Public-API controls for unchanged closures in a returned owner tail
//! (#6578): when only a closure-free operand of a one-expression tail
//! changes (`it.map(|l| l.cents).sum::<u64>() + 50` → `.. + 51`), the
//! unchanged closure runs the same way in both versions, so a test that
//! pins the owner's return reveals the change. A change inside the closure,
//! a value a closure captures, or an operand only a combinator may evaluate
//! stays unrevealed. Each case writes a small crate, changes line 6 of
//! `src/lib.rs`, and reads the return-value finding through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-unchanged-closure-tail-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // The guard owns only a directory this call created, and owns it
        // before anything is written inside.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir(scratch.0.join("tests")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The return-value finding for a change of `old` to `new` as the tail of
/// `pub fn total(lines: &[Line], extra: Option<u64>) -> u64`, with `test`
/// as the body of the one test in `tests/total.rs`.
fn total_finding(old: &str, new: &str, test: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let manifest =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
    let lib = format!(
        "pub struct Line {{\n    pub cents: u64,\n}}\n\npub fn total(lines: &[Line], extra: Option<u64>) -> u64 {{\n    {new}\n}}\n"
    );
    let tests = format!("use demo::{{Line, total}};\n\n#[test]\nfn totals() {{\n    {test}\n}}\n");
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -6,1 +6,1 @@\n-    {old}\n+    {new}\n"
    );
    std::fs::write(scratch.0.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("tests/total.rs"), tests).map_err(|error| error.to_string())?;
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
        .find(|finding| finding["probe"]["family"] == "return_value")
        .cloned()
        .ok_or_else(|| format!("no return_value finding: {json}"))
}

const PINS: &str = "assert_eq!(total(&[Line { cents: 3 }], None), 54);";

#[test]
fn a_changed_operand_beside_an_unchanged_closure_is_exposed() -> Result<(), String> {
    let finding = total_finding(
        "lines.iter().map(|l| l.cents).sum::<u64>() + 50",
        "lines.iter().map(|l| l.cents).sum::<u64>() + 51",
        PINS,
    )?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn a_change_the_closure_decides_or_carries_stays_unrevealed() -> Result<(), String> {
    for (old, new) in [
        // A closure captures the changed value.
        (
            "{ let k = 50; lines.iter().map(|l| l.cents + k).sum::<u64>() }",
            "{ let k = 51; lines.iter().map(|l| l.cents + k).sum::<u64>() }",
        ),
        // The changed receiver flows through the unchanged closure.
        (
            "lines.iter().map(|l| l.cents).sum::<u64>() + 50",
            "lines[..1].iter().map(|l| l.cents).sum::<u64>() + 50",
        ),
        // The change removed a closure that gave the same value.
        (
            "lines.iter().map(|l| l.cents).sum::<u64>() + (|| 51)()",
            "lines.iter().map(|l| l.cents).sum::<u64>() + 51",
        ),
        // String whitespace changed alongside the operand.
        (
            "\"a b\".len() as u64 + lines.iter().map(|l| l.cents).sum::<u64>() + 2",
            "\"a  b\".len() as u64 + lines.iter().map(|l| l.cents).sum::<u64>() + 1",
        ),
        // The changed initial value passes through the folding closure.
        (
            "lines.iter().fold(50, |acc, l| acc + l.cents)",
            "lines.iter().fold(51, |acc, l| acc + l.cents)",
        ),
        // The changed default runs only when `extra` is `None`, and the
        // combinator decides that.
        (
            "extra.map_or(50, |v| v) + lines.iter().map(|l| l.cents).sum::<u64>()",
            "extra.map_or(51, |v| v) + lines.iter().map(|l| l.cents).sum::<u64>()",
        ),
    ] {
        let finding = total_finding(old, new, PINS)?;
        assert_ne!(finding["classification"], "exposed", "{new}: {finding}");
    }
    Ok(())
}

/// A change inside the closure body of one admitted RIPR-SPEC-0265 adaptor
/// tail (`map` + `sum` over an arithmetic closure body keeps every closure
/// value in the owner's result) is no longer a refusal control: #7914's
/// closure-boundary reading witnesses it, and the pinned assertion
/// discriminates it. This control was written before #7914 landed; on the
/// tree without it the same shape reads `reachable_unrevealed`.
#[test]
fn a_change_inside_an_admitted_adaptor_closure_reads_exposed() -> Result<(), String> {
    let finding = total_finding(
        "lines.iter().map(|l| l.cents + 50).sum::<u64>()",
        "lines.iter().map(|l| l.cents + 51).sum::<u64>()",
        PINS,
    )?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}
