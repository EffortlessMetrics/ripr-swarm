//! Public-API controls for RIPR-SPEC-0265 rule 3: an owner whose whole body
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
    owner_findings("count_positive(xs: &[i32]) -> usize", new, old, test)
}

/// [`findings`] for an owner with the signature `signature`.
fn owner_findings(
    signature: &str,
    new: &str,
    old: &str,
    test: &str,
) -> Result<(Value, Value), String> {
    file_findings(signature, new, old, test, "")
}

/// [`owner_findings`] with `items` placed after the owner.
fn file_findings(
    signature: &str,
    new: &str,
    old: &str,
    test: &str,
    items: &str,
) -> Result<(Value, Value), String> {
    let source = format!(
        "pub fn {signature} {{\n    {new}\n}}\n{items}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn owner_is_checked() {{\n        {test}\n    }}\n}}\n"
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
fn map_count_tail_keeps_the_pre_0265_reading() -> Result<(), String> {
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

#[test]
fn element_boundary_pairs_only_the_whole_owner_call() -> Result<(), String> {
    // Every assertion passes the boundary element 0, but only the first
    // compares the whole owner result; an index, `len` or `is_empty` of it
    // cannot tell `x > 0` from `x >= 0`.
    let signature = "pos(xs: &[i32]) -> Vec<bool>";
    let new = "xs.iter().map(|&x| x > 0).collect()";
    let old = "xs.iter().map(|&x| x >= 0).collect()";
    let (_, whole) = owner_findings(
        signature,
        new,
        old,
        "assert_eq!(pos(&[0, 5]), [false, true]);",
    )?;
    assert_eq!(whole["classification"], "exposed", "{whole}");
    // Element pairing reads no `let`-bound result (fail closed), so the
    // bound projections below stay unpaired even if a whole-result binding
    // were ever paired.
    for test in [
        "assert_eq!(pos(&[0, 5])[1], true);",
        "assert_eq!(pos(&[0, 5]).len(), 2);",
        "assert!(!pos(&[0, 5]).is_empty());",
        "let r = pos(&[0, 5]);\n        assert_eq!(r[1], true);",
        "let r = pos(&[0, 5]);\n        assert_eq!(r.len(), 2);",
        "let r = pos(&[0, 5]);\n        assert_eq!(r, [false, true]);",
    ] {
        let (_, predicate) = owner_findings(signature, new, old, test)?;
        assert_ne!(
            predicate["classification"], "exposed",
            "{test}: {predicate}"
        );
    }
    Ok(())
}

#[test]
fn collect_into_a_rebound_vec_keeps_the_pre_0265_reading() -> Result<(), String> {
    // A `Vec` the owner's file binds to a set loses order and multiplicity.
    let signature = "pos(xs: &[i32]) -> Vec<bool>";
    let new = "xs.iter().map(|&x| x > 0).collect()";
    let old = "xs.iter().map(|&x| x >= 0).collect()";
    let test = "assert_eq!(pos(&[0, 5]), [false, true].into_iter().collect::<Vec<_>>());";
    for (items, rebound) in [
        ("", false),
        ("\ntype Vec<T> = std::collections::BTreeSet<T>;\n", true),
        ("\nuse std::collections::BTreeSet as Vec;\n", true),
        ("\npub struct Vec<T>(T);\n", true),
    ] {
        // The return value is compared whole, so its reading does not
        // depend on the closure; the element boundary does.
        let (_, predicate) = file_findings(signature, new, old, test, items)?;
        assert_eq!(
            predicate["classification"] == "exposed",
            !rebound,
            "{items}: {predicate}"
        );
    }
    Ok(())
}

#[test]
fn self_comparing_boundary_assertion_pairs_nothing() -> Result<(), String> {
    let new = "xs.iter().filter(|&&x| x > 0).count()";
    let old = "xs.iter().filter(|&&x| x >= 0).count()";
    let (_, control) = findings(new, old, "assert_eq!(count_positive(&[0, 3]), 1);")?;
    assert_eq!(control["classification"], "exposed", "{control}");
    for test in [
        // The boundary input is compared with the owner itself.
        "assert_eq!(count_positive(&[0, 3]), count_positive(&[0, 3]));",
        "let e = count_positive(&[0, 3]);\n        assert_eq!(count_positive(&[0, 3]), e);",
        // A tautological boundary assertion beside a discriminating one on
        // a far input must not combine into `exposed`.
        "assert_eq!(count_positive(&[0]), count_positive(&[0]));\n        assert_eq!(count_positive(&[5]), 1);",
    ] {
        let (_, predicate) = findings(new, old, test)?;
        assert_ne!(
            predicate["classification"], "exposed",
            "{test}: {predicate}"
        );
    }
    Ok(())
}

#[test]
fn hex_boundary_is_not_read_from_its_leading_digit() -> Result<(), String> {
    // `0x7f` is 127, not 0: the element 0 never sits on the boundary.
    let (_, predicate) = findings(
        "xs.iter().filter(|&&x| x > 0x7f).count()",
        "xs.iter().filter(|&&x| x >= 0x7f).count()",
        "assert_eq!(count_positive(&[0, 200]), 1);",
    )?;
    assert_ne!(predicate["classification"], "exposed", "{predicate}");
    Ok(())
}
