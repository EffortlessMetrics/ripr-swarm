//! Public-API controls for RIPR-SPEC-0260 rule 3: an owner whose whole body
//! is one admitted iterator-adaptor tail returns that tail's value, so the
//! changed tail completes a return witness and the changed closure
//! comparison flows to the owner's return. Each case writes a small crate,
//! changes the tail line, and reads the findings through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-iterator-adaptor-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"iterator_adaptor\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
const COMPLETE: &str = "Complete propagation witness reaches returned value";

/// The `return_value` and `predicate` findings for a change of the owner's
/// tail (line 2) from `old` to `new`.
fn findings(new: &str, old: &str, test: &str) -> Result<(Value, Value), String> {
    let source = format!(
        "pub fn count_positive(xs: &[i32]) -> usize {{\n    {new}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn owner_is_checked() {{\n        {test}\n    }}\n}}\n"
    );
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-    {old}\n+    {new}\n"
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
    let family = |name: &str| {
        json["findings"]
            .as_array()
            .and_then(|findings| {
                findings
                    .iter()
                    .find(|finding| finding["probe"]["family"] == name)
                    .cloned()
            })
            .ok_or_else(|| format!("no {name} finding: {json}"))
    };
    Ok((family("return_value")?, family("predicate")?))
}

fn propagate(finding: &Value) -> &str {
    finding["ripr"]["propagate"]["state"]
        .as_str()
        .unwrap_or_default()
}

fn propagate_summary(finding: &Value) -> &str {
    finding["ripr"]["propagate"]["summary"]
        .as_str()
        .unwrap_or_default()
}

#[test]
fn admitted_adaptor_tail_propagates_to_the_owner_return() -> Result<(), String> {
    let (returned, predicate) = findings(
        "xs.iter().filter(|&&x| x > 0).count()",
        "xs.iter().filter(|&&x| x >= 0).count()",
        "assert_eq!(count_positive(&[-1, 0, 3, 7]), 2);",
    )?;
    assert!(
        propagate_summary(&returned).contains(COMPLETE),
        "{returned}"
    );
    assert_eq!(returned["classification"], "exposed", "{returned}");
    assert_eq!(propagate(&predicate), "yes", "{predicate}");
    assert_eq!(predicate["classification"], "exposed", "{predicate}");
    Ok(())
}

#[test]
fn map_count_tail_keeps_the_pre_0260_reading() -> Result<(), String> {
    // `map` + `count` ignores the closure value: no rule applies.
    let (returned, predicate) = findings(
        "xs.iter().map(|&x| x > 0).count()",
        "xs.iter().map(|&x| x >= 0).count()",
        "assert_eq!(count_positive(&[0, 5]), 2);",
    )?;
    assert!(
        !propagate_summary(&returned).contains(COMPLETE),
        "{returned}"
    );
    assert_ne!(returned["classification"], "exposed", "{returned}");
    assert_ne!(propagate(&predicate), "yes", "{predicate}");
    assert_ne!(predicate["classification"], "exposed", "{predicate}");
    Ok(())
}
