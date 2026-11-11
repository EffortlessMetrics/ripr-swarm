//! Public-API controls for a predicate assertion that calls the changed
//! function but cannot observe the call's value (#14206).
//!
//! The owner is `pub fn over(qty: u32) -> bool { qty >= 10 }`, changed from
//! `qty > 10`. Each case is the whole body of the one test in the crate and
//! calls `over(10)`, the input the two versions disagree on. A body whose
//! compared value does not carry the call's result passes for either
//! version, so the predicate finding must not read `exposed`; a body whose
//! compared value does carry it keeps `exposed`. Every case writes a small
//! crate and reads the predicate finding through `check_workspace`.
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
            "ripr-predicate-owner-value-{}-{}",
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

/// The predicate finding for `qty > 10` changed to `qty >= 10`, with `body`
/// as the body of the one unit test beside the owner.
fn predicate_finding(body: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let manifest =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
    let lib = format!(
        "pub fn over(qty: u32) -> bool {{\n    qty >= 10\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn first(a: bool, _b: bool) -> bool {{\n        a\n    }}\n\n    #[test]\n    fn t() {{\n        let _ = first(true, true);\n        {body}\n    }}\n}}\n"
    );
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-    qty > 10\n+    qty >= 10\n";
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

/// Bodies whose assertion passes for either version of `over`.
const MASKED: &[&str] = &[
    "assert_eq!(Some(over(10)).is_some(), true);",
    "assert_eq!(vec![over(10)].len(), 1);",
    "assert_eq!((over(10), 1).1, 1);",
    "assert_eq!({ over(10); true }, true);",
    "assert_eq!(first(true, over(10)), true);",
    "assert_eq!(Some(true).unwrap_or(over(10)), true);",
    "assert_eq!(Some(true).unwrap_or_else(|| over(10)), true);",
    "let r: Result<bool, ()> = Ok(true); assert_eq!(r.unwrap_or_else(|_| over(10)), true);",
    "assert_eq!(false.then(|| over(10)), None);",
    "let o: Option<u32> = None; assert_eq!(o.map(|_| over(10)), None);",
    "assert_eq!(None::<u32>.map_or(true, |_| over(10)), true);",
    "assert_eq!(if true { true } else { over(10) }, true);",
    "let v = match Some(1) { Some(_) => true, None => over(10) }; assert_eq!(v, true);",
    "assert_eq!(std::iter::repeat_with(|| over(10)).take(0).count(), 0);",
    "assert!(true || over(10));",
];

/// Bodies whose compared value carries the owner call's result.
const OBSERVING: &[&str] = &[
    "assert_eq!(over(10), true);",
    "assert!(over(10)); assert!(!over(9));",
    "assert_eq!(over(10) as u8, 1);",
    "let a = over(10); assert_eq!(a, true);",
    "assert_eq!((over(10), over(9)), (true, false));",
    "assert_eq!(Some(over(10)), Some(true));",
    "assert_eq!(!over(10), false);",
    "assert_eq!(over(10).then_some(1), Some(1));",
    "assert_eq!(if over(10) { 1 } else { 2 }, 1);",
    "assert_eq!(match over(10) { true => 'y', false => 'n' }, 'y');",
    "assert!(over(10) == true);",
    "assert_eq!(format!(\"a, b ({})\", over(10)), \"a, b (true)\");",
    "assert!(matches!(over(10), true));",
    "assert_eq!(u8::from(over(10)), 1);",
    "for (q, e) in [(10, true), (9, false)] { assert_eq!(over(q), e); }",
];

#[test]
fn an_assertion_that_discards_or_masks_the_owner_call_is_not_exposed() -> Result<(), String> {
    for body in MASKED {
        let finding = predicate_finding(body)?;
        assert_eq!(
            finding["classification"], "weakly_exposed",
            "{body}: {finding}"
        );
        let summary = finding["ripr"]["discriminate"]["summary"]
            .as_str()
            .ok_or("missing discriminate summary")?;
        assert!(
            summary.contains("discards or masks the call's value")
                && summary.contains("(observation_unverified)"),
            "{body}: {summary}"
        );
        assert_eq!(finding["ripr"]["discriminate"]["state"], "weak", "{body}");
    }
    Ok(())
}

#[test]
fn an_assertion_whose_compared_value_carries_the_owner_call_stays_exposed() -> Result<(), String> {
    for body in OBSERVING {
        let finding = predicate_finding(body)?;
        assert_eq!(finding["classification"], "exposed", "{body}: {finding}");
    }
    Ok(())
}

#[test]
fn an_observing_assertion_beside_a_masked_one_keeps_the_credit() -> Result<(), String> {
    for body in [
        "assert_eq!(Some(over(10)).is_some(), true); assert_eq!(over(10), true);",
        "assert_eq!(over(10), true); assert_eq!(Some(over(10)).is_some(), true);",
    ] {
        let finding = predicate_finding(body)?;
        assert_eq!(finding["classification"], "exposed", "{body}: {finding}");
    }
    Ok(())
}
