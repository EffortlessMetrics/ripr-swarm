//! #14116: a `for` over a non-empty array runs its body, so its assertion is
//! on the test's path whatever the rows hold. Only execution is admitted: an
//! expected value read from a row that is not constant (here the owner's own
//! output) is never credited as a discriminator.

use super::Scratch;
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

/// Each case's body, and the class its changed return value reads.
const CASES: &[(&str, &str, ExposureClass)] = &[
    // The baseline: constant rows were already admitted.
    (
        "constant_rows",
        "for (x, want) in [(4, 12)] { assert_eq!(weight(x), want); }",
        ExposureClass::Exposed,
    ),
    // A call in the row no longer hides the assertion.
    (
        "call_row_literal_expectation",
        "for x in [four()] { assert_eq!(weight(x), 12); }",
        ExposureClass::Exposed,
    ),
    (
        "bound_call_rows",
        "let rows = [four(), 4];\n        for x in rows { assert_eq!(weight(x), 12); }",
        ExposureClass::Exposed,
    ),
    // The row holds the owner's own output: the assertion runs, but its
    // expected value proves nothing, as for the same `let` outside a loop.
    (
        "self_confirming_row",
        "for (x, want) in [(4, weight(4))] { assert_eq!(weight(x), want); }",
        ExposureClass::WeaklyExposed,
    ),
    (
        "self_confirming_let",
        "let want = weight(4);\n        assert_eq!(weight(4), want);",
        ExposureClass::WeaklyExposed,
    ),
    // An iterable whose length is not visible may be empty.
    (
        "vec_rows",
        "for x in vec![four()] { assert_eq!(weight(x), 12); }",
        ExposureClass::ReachableUnrevealed,
    ),
    (
        "empty_repeat",
        "for x in [four(); 0] { assert_eq!(weight(x), 12); }",
        ExposureClass::ReachableUnrevealed,
    ),
];

fn source(body: &str) -> String {
    format!(
        "pub fn weight(x: i32) -> i32 {{
    x * 3 + 1
}}
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
    for (case, body, expected) in CASES {
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
    }
    Ok(())
}
