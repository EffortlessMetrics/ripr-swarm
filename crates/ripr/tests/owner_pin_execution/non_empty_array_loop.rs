//! #14116: a `for` over a non-empty array of call-free rows runs its body,
//! so its assertion is on the test's path whatever paths the rows name. Only
//! execution is admitted: an expected value read from rows that are not
//! constant (here a `const` holding the owner's own output) is never
//! credited as a discriminator, and a row that runs code keeps the refusal.

use super::{Scratch, source_runtime_control};
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub const fn weight(x: i32) -> i32 {
-    x * 3
+    x * 3 + 1
 }
";

/// Each case's body, the class its changed return value reads, and whether
/// the compiled test catches the change (`None`: not run, the test exits).
const CASES: &[(&str, &str, ExposureClass, Option<bool>)] = &[
    // The baseline: constant rows were already admitted.
    (
        "constant_rows",
        "for (x, want) in [(4, 12)] { assert_eq!(weight(x), want); }",
        ExposureClass::Exposed,
        Some(true),
    ),
    // A path in a row no longer hides the assertion.
    (
        "const_row_literal_expectation",
        "for x in [FOUR, 4] { assert_eq!(weight(x), 12); }",
        ExposureClass::Exposed,
        Some(true),
    ),
    (
        "bound_local_rows",
        "let four = 4;\n        let rows = [(four, \"four\")];\n        for (x, _) in rows { assert_eq!(weight(x), 12); }",
        ExposureClass::Exposed,
        Some(true),
    ),
    // The row names the owner's own output: the assertion runs, but its
    // expected value proves nothing, as when compared directly.
    (
        "self_confirming_row",
        "for want in [EXPECTED] { assert_eq!(weight(4), want); }",
        ExposureClass::WeaklyExposed,
        Some(false),
    ),
    (
        "self_confirming_row_through_let",
        "for row in [(4, EXPECTED)] { let want = row.1; assert_eq!(weight(4), want); }",
        ExposureClass::WeaklyExposed,
        Some(false),
    ),
    (
        "self_confirming_direct",
        "assert_eq!(weight(4), EXPECTED);",
        ExposureClass::WeaklyExposed,
        Some(false),
    ),
    // A row that runs code may leave the test before the loop starts.
    (
        "call_row",
        "for x in [four()] { assert_eq!(weight(x), 12); }",
        ExposureClass::ReachableUnrevealed,
        Some(true),
    ),
    (
        "exiting_row",
        "for _ in [std::process::exit(0)] { assert_eq!(weight(4), 12); }",
        ExposureClass::ReachableUnrevealed,
        None,
    ),
    (
        "bound_exiting_row",
        "let rows = [std::process::exit(0)];\n        for _ in rows { assert_eq!(weight(4), 12); }",
        ExposureClass::ReachableUnrevealed,
        None,
    ),
    // An iterable whose length is not visible may be empty.
    (
        "vec_rows",
        "for x in vec![FOUR] { assert_eq!(weight(x), 12); }",
        ExposureClass::ReachableUnrevealed,
        Some(true),
    ),
    (
        "empty_repeat",
        "for x in [FOUR; 0] { assert_eq!(weight(x), 12); }",
        ExposureClass::ReachableUnrevealed,
        Some(false),
    ),
];

fn source(body: &str) -> String {
    format!(
        "pub const fn weight(x: i32) -> i32 {{
    x * 3 + 1
}}
pub const FOUR: i32 = 4;
pub const EXPECTED: i32 = weight(4);
pub fn four() -> i32 {{
    4
}}
#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn weighs() {{
        {body}
    }}
}}
"
    )
}

#[test]
fn a_non_empty_array_loop_admits_execution_only() -> Result<(), String> {
    for (case, body, expected, catches) in CASES {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"array-loop-owner\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), source(body))
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
            assert_eq!(*catches, Some(true), "{case}");
        }
        if let Some(catches) = catches {
            let old = source(body).replace("x * 3 + 1", "x * 3");
            source_runtime_control(&old, &format!("array loop {case} old"), 1, false)?;
            source_runtime_control(
                &source(body),
                &format!("array loop {case} changed"),
                1,
                *catches,
            )?;
        }
    }
    Ok(())
}
