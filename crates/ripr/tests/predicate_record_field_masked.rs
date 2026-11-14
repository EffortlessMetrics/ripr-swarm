//! Public-API controls for a predicate that sets one field of the record the
//! changed function returns (#14562).
//!
//! The owner is `pub fn opts(n: u32) -> Opts { Opts { retries: 3, big: n >= 10 } }`,
//! changed from `n > 10`. Each case is the whole body of the one test in the
//! crate and calls `opts(10)`, the input the two versions disagree on. An
//! assertion that reads another field of the record (or a method that may
//! never read `big`) passes for either version, so the predicate finding must
//! not read `exposed`. An assertion that reads `big` or compares the whole
//! record keeps `exposed`. Every case writes a small crate and reads the
//! predicate finding through `check_workspace`.
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
            "ripr-predicate-record-field-{}-{}",
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

/// The finding of `family` for `big: n > 10` changed to `big: n >= 10`,
/// with `body` as the body of the one unit test beside the owner.
fn finding(body: &str, family: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let manifest =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
    let lib = format!(
        "#[derive(Clone, Debug, PartialEq)]\npub struct Opts {{\n    pub retries: u32,\n    pub big: bool,\n}}\n\nimpl Opts {{\n    pub fn summary(&self) -> u32 {{\n        self.retries\n    }}\n}}\n\npub fn opts(n: u32) -> Opts {{\n    Opts {{\n        retries: 3,\n        big: n >= 10,\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn t() {{\n        {body}\n    }}\n}}\n"
    );
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -16,1 +16,1 @@\n-        big: n > 10,\n+        big: n >= 10,\n";
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
        .find(|finding| finding["probe"]["family"] == family)
        .cloned()
        .ok_or_else(|| format!("no {family} finding: {json}"))
}

/// Bodies whose assertion passes for either version of `opts`.
const MASKED: &[&str] = &[
    "assert_eq!(opts(10).retries, 3);",
    "let o = opts(10); assert_eq!(o.retries, 3);",
    "assert_eq!(opts(10).clone().retries, 3);",
    "assert_eq!(opts(10).summary(), 3);",
];

/// Bodies whose compared value carries the changed field.
const OBSERVING: &[&str] = &[
    "assert_eq!(opts(10).big, true);",
    "let o = opts(10); assert_eq!(o.big, true);",
    "assert_eq!(opts(10).clone().big, true);",
    "assert_eq!(opts(10), Opts { retries: 3, big: true });",
];

#[test]
fn a_read_of_another_field_of_the_returned_record_is_not_exposed() -> Result<(), String> {
    for body in MASKED {
        let predicate = finding(body, "predicate")?;
        assert_eq!(
            predicate["classification"], "weakly_exposed",
            "{body}: {predicate}"
        );
        assert_eq!(predicate["ripr"]["discriminate"]["state"], "weak", "{body}");
        let summary = predicate["ripr"]["discriminate"]["summary"]
            .as_str()
            .ok_or("missing discriminate summary")?;
        assert!(
            summary.contains("discards or masks the call's value"),
            "{body}: {summary}"
        );
        // The field_construction twin on the same line never credited it.
        let field = finding(body, "field_construction")?;
        assert_ne!(field["classification"], "exposed", "{body}: {field}");
    }
    Ok(())
}

#[test]
fn a_read_of_the_changed_field_or_the_whole_record_stays_exposed() -> Result<(), String> {
    for body in OBSERVING {
        let predicate = finding(body, "predicate")?;
        assert_eq!(
            predicate["classification"], "exposed",
            "{body}: {predicate}"
        );
    }
    Ok(())
}

/// #14701: a bare `assert!` on the changed bool field is
/// `assert_eq!(opts(10).big, true)` spelled another way, so both findings on
/// the line read `exposed`.
#[test]
fn a_bare_assert_on_the_changed_bool_field_is_exposed() -> Result<(), String> {
    for body in [
        "assert!(opts(10).big);",
        "let o = opts(10); assert!(o.big);",
        "assert!(opts(10).big, \"big at ten\");",
        "assert!(!opts(9).big);\n        assert!(opts(10).big);",
    ] {
        for family in ["predicate", "field_construction"] {
            let finding = finding(body, family)?;
            assert_eq!(
                finding["classification"], "exposed",
                "{family} {body}: {finding}"
            );
        }
    }
    Ok(())
}

/// #14701: a bare `assert!` that does not read the changed field whole stays
/// weak.
#[test]
fn a_bare_assert_on_anything_else_stays_weak() -> Result<(), String> {
    for body in [
        "assert!(opts(10).retries == 3);",
        "assert!(opts(10).big || opts(10).retries == 3);",
        "assert!(opts(10).summary() == 3);",
    ] {
        for family in ["predicate", "field_construction"] {
            let finding = finding(body, family)?;
            assert_ne!(
                finding["classification"], "exposed",
                "{family} {body}: {finding}"
            );
        }
    }
    Ok(())
}
