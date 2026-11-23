//! #14116: a test-local check helper with a `?` lends its assertion when
//! every eager call is `check(..)?` in a `Result` test. An error that skips
//! the helper's assertion then fails the test. A call that discards the
//! helper's result keeps the refusal: the helper's `?` could skip the
//! assertion and the test would still pass.

use super::{Scratch, source_runtime_control};
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn weight(x: i32) -> i32 {
-    x * 3
+    x * 3 + 1
 }
";

/// A helper whose `?` passes every non-negative input on.
const CHECKED: &str = "fn checked(x: i32) -> Result<i32, String> {
        if x < 0 { Err(format!(\"negative {x}\")) } else { Ok(x) }
    }";

/// A helper whose `?` leaves before the assertion for the tested input.
const REFUSING: &str = "fn checked(x: i32) -> Result<i32, String> {
        if x == 4 { Err(format!(\"refused {x}\")) } else { Ok(x) }
    }";

/// Each case's `checked`, the test's body, the class its changed return
/// value reads, and whether the compiled test catches the change.
const CASES: &[(&str, &str, &str, ExposureClass, bool)] = &[
    (
        "propagated",
        CHECKED,
        "check(4, 12)?;\n        Ok(())",
        ExposureClass::Exposed,
        true,
    ),
    // An argument leaves the test before the helper is entered: the test
    // passes on the changed body without running the assertion.
    (
        "argument_returns_first",
        CHECKED,
        "check(match None::<i32> { Some(x) => x, None => return Ok(()) }, 12)?;\n        Ok(())",
        ExposureClass::StaticUnknown,
        false,
    ),
    // The result is discarded: the assertion runs here, but nothing static
    // tells this helper from the refusing one below, so neither is credited.
    (
        "discarded",
        CHECKED,
        "let _ = check(4, 12);\n        Ok(())",
        ExposureClass::StaticUnknown,
        true,
    ),
    (
        "discarded_and_skipped",
        REFUSING,
        "let _ = check(4, 12);\n        Ok(())",
        ExposureClass::StaticUnknown,
        false,
    ),
];

fn source(checked: &str, body: &str) -> String {
    format!(
        "pub fn weight(x: i32) -> i32 {{
    x * 3 + 1
}}
#[cfg(test)]
mod tests {{
    use super::*;
    {checked}
    fn check(x: i32, want: i32) -> Result<(), String> {{
        let x = checked(x)?;
        assert_eq!(weight(x), want);
        Ok(())
    }}
    #[test]
    fn weighs() -> Result<(), String> {{
        {body}
    }}
}}
"
    )
}

#[test]
fn a_propagated_fallible_helper_lends_its_assertion() -> Result<(), String> {
    for (case, checked, body, expected, catches) in CASES {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"fallible-helper-owner\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), source(checked, body))
            .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("change.diff"), DIFF).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let changed: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
            })
            .collect();
        assert_eq!(changed.len(), 1, "{case}: one changed return value");
        assert_eq!(&changed[0].class, expected, "{case}: {:?}", changed[0]);
        // Static credit never outruns the compiled test: an `exposed` case
        // fails on the changed body, and every case passes on the old one.
        if *expected == ExposureClass::Exposed {
            assert!(*catches, "{case}");
        }
        let old = source(checked, body).replace("x * 3 + 1", "x * 3");
        source_runtime_control(&old, &format!("fallible helper {case} old"), 1, false)?;
        source_runtime_control(
            &source(checked, body),
            &format!("fallible helper {case} changed"),
            1,
            *catches,
        )?;
    }
    Ok(())
}
