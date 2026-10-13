//! Public-API controls for a constant owner tail (#6617): an owner whose
//! whole body is one literal-only expression returns exactly that value, so
//! a change to it propagates to the owner's return. Each case writes a small
//! crate, changes one line, and reads the return-value finding through
//! `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-constant-tail-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"constant_tail\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
const COMPLETE: &str = "Complete propagation witness reaches returned value";

/// `owner` is the owner item; `line` is the 1-based line of `new` in it.
/// The test module follows the owner.
fn return_finding(
    owner: &str,
    line: usize,
    old: &str,
    new: &str,
    test: &str,
) -> Result<Value, String> {
    let source = format!(
        "{owner}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn owner_is_checked() {{\n        {test}\n    }}\n}}\n"
    );
    let changed = source
        .lines()
        .nth(line - 1)
        .ok_or("changed line is outside the source")?;
    assert_eq!(
        changed.trim(),
        new,
        "fixture puts the change on line {line}"
    );
    let original = changed.replacen(new, old, 1);
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{original}\n+{changed}\n"
    );
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
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
    let finding = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "return_value")
        .cloned()
        .ok_or_else(|| format!("no return_value finding: {json}"))?;
    assert_eq!(finding["probe"]["line"], line, "{finding}");
    Ok(finding)
}

fn propagate_summary(finding: &Value) -> &str {
    finding["ripr"]["propagate"]["summary"]
        .as_str()
        .unwrap_or_default()
}

#[test]
fn exact_pin_on_a_constant_owner_tail_is_exposed() -> Result<(), String> {
    for (owner, old, new, test) in [
        (
            "pub fn answer() -> u32 {\n    40 + 2\n}",
            "42",
            "40 + 2",
            "assert_eq!(answer(), 42);",
        ),
        (
            "pub fn answer() -> u32 {\n    43\n}",
            "42",
            "43",
            "assert_eq!(answer(), 43);",
        ),
        (
            "pub fn offset() -> i32 {\n    -(1 << 3)\n}",
            "-8",
            "-(1 << 3)",
            "assert_eq!(offset(), -8);",
        ),
        (
            "pub fn name() -> &'static str {\n    \"ripr\"\n}",
            "\"ripR\"",
            "\"ripr\"",
            "assert_eq!(name(), \"ripr\");",
        ),
    ] {
        let finding = return_finding(owner, 2, old, new, test)?;
        assert!(
            propagate_summary(&finding).contains(COMPLETE),
            "{new}: {finding}"
        );
        assert_eq!(finding["classification"], "exposed", "{new}: {finding}");
    }
    Ok(())
}

#[test]
fn weak_oracle_on_a_constant_owner_tail_keeps_its_gap() -> Result<(), String> {
    // Propagation is now established, so the gap is the oracle's alone.
    let finding = return_finding(
        "pub fn answer() -> u32 {\n    40 + 2\n}",
        2,
        "42",
        "40 + 2",
        "assert!(answer() > 0);",
    )?;
    assert!(propagate_summary(&finding).contains(COMPLETE), "{finding}");
    assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    Ok(())
}

// Shapes outside the constant tail (a preceding statement, names, calls,
// casts, closures, macros, blocks, branches) never reach a `return_value`
// probe whose text is the whole tail, so their refusals are pinned by the
// unit tests on `constant_owner_tail_line` in `propagation_witness.rs`.
