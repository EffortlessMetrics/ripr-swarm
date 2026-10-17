//! Public-API controls for RIPR-SPEC-0197 check-helper argument pins
//! (RIPR-SPEC-0231 example 12).
//!
//! A test-local helper whose assertion compares two of its own parameters
//! pins the owner when the test's eager call passes the owner's result (or
//! a template rendering of it) and a literal, each read as written or
//! through one once-used `let`. Decoys that feed the owner into both sides,
//! rebind an argument, shadow a parameter or `String`, or render the value
//! into different text stay unconfirmed.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with `score` on lines 1-3 and `tests` as the test module.
fn source(tests: &str) -> String {
    format!(
        "pub fn score(x: u32) -> u32 {{
    x * 2
}}

#[cfg(test)]
mod tests {{
    use super::*;
{tests}
}}
"
    )
}

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn score(x: u32) -> u32 {
-    x * 3
+    x * 2
 }
";

const JSON_HELPER: &str = "    fn assert_json_eq(actual: String, expected: String) {
        assert!(actual == expected, \"json differs\");
    }
";

const SCALAR_HELPER: &str = "    fn check(actual: u32, expected: u32) {
        assert_eq!(actual, expected);
    }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-check-helper-argument-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"check-helper\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    /// The one finding on the changed line 2.
    fn changed(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        // The removed `x * 3` line keeps its own static_unknown finding.
        let mut on_line = output.findings.iter().filter(|finding| {
            finding.probe.location.line == 2 && finding.probe.family == ProbeFamily::ReturnValue
        });
        match (on_line.next(), on_line.next()) {
            (Some(finding), None) => Ok(finding.clone()),
            other => Err(format!(
                "premise: exactly one return_value finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn test_fn(body: &str) -> String {
    format!("    #[test]\n    fn pins_score() {{\n{body}    }}\n")
}

#[test]
fn a_helper_comparing_its_parameters_pins_the_arguments_the_test_passes() -> Result<(), String> {
    for (name, tests) in [
        (
            "spec0231 example 12: rendered template against String::from",
            format!(
                "{JSON_HELPER}{}",
                test_fn(
                    "        let actual = format!(\"{{\\\"score\\\":{}}}\", score(2));
        let expected = String::from(\"{\\\"score\\\":4}\");
        assert_json_eq(actual, expected);
"
                )
            ),
        ),
        (
            "let-bound owner call and literal",
            format!(
                "{SCALAR_HELPER}{}",
                test_fn(
                    "        let actual = score(2);
        let expected = 4;
        check(actual, expected);
"
                )
            ),
        ),
        (
            "let-bound owner call and inline literal, swapped order",
            format!(
                "{SCALAR_HELPER}{}",
                test_fn(
                    "        let actual = score(2);
        check(4, actual);
"
                )
            ),
        ),
    ] {
        let finding = TempRepo::create(&source(&tests))?.changed()?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{name}: {finding:?}");
    }
    Ok(())
}

#[test]
fn a_direct_template_rendering_pins_a_scalar_owner() -> Result<(), String> {
    let tests = test_fn(
        "        assert_eq!(format!(\"{{\\\"score\\\":{}}}\", score(2)), \"{\\\"score\\\":4}\");
",
    );
    let finding = TempRepo::create(&source(&tests))?.changed()?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn decoy_helper_arguments_lend_no_pin() -> Result<(), String> {
    let string_helper = "    fn check(actual: String, expected: String) {
        assert!(actual == expected, \"differs\");
    }
";
    for (name, tests) in [
        (
            "template text differs from the expected text",
            format!(
                "{string_helper}{}",
                test_fn(
                    "        let actual = format!(\"{{\\\"total\\\":{}}}\", score(2));
        let expected = String::from(\"{\\\"score\\\":4}\");
        check(actual, expected);
"
                )
            ),
        ),
        (
            "expected computed through the owner",
            format!(
                "{SCALAR_HELPER}{}",
                test_fn(
                    "        let actual = score(2);
        let expected = score(2);
        check(actual, expected);
"
                )
            ),
        ),
        (
            "a workspace String shadows the std one",
            format!(
                "{string_helper}    struct String(u8);\n{}",
                test_fn(
                    "        let actual = format!(\"{}\", score(2));
        let expected = String::from(\"4\");
        check(actual, expected);
"
                )
            ),
        ),
        (
            "mutable binding reassigned before the call",
            format!(
                "{SCALAR_HELPER}{}",
                test_fn(
                    "        let mut actual = score(2);
        actual = 4;
        check(actual, 4);
"
                )
            ),
        ),
        (
            "owner binding used again besides the call",
            format!(
                "{SCALAR_HELPER}{}",
                test_fn(
                    "        let actual = score(2);
        let copy = actual;
        check(actual, 4);
        assert!(copy > 0);
"
                )
            ),
        ),
        (
            "a workspace item rebinds the parameter type",
            format!(
                "{SCALAR_HELPER}    #[allow(non_camel_case_types)]\n    type u32 = core::primitive::u32;\n{}",
                test_fn(
                    "        let actual = score(2);
        check(actual, 4);
"
                )
            ),
        ),
        (
            "helper shadows a compared parameter",
            format!(
                "    fn check(actual: u32, expected: u32) {{
        let expected = actual;
        assert_eq!(actual, expected);
    }}
{}",
                test_fn(
                    "        let actual = score(2);
        check(actual, 4);
"
                )
            ),
        ),
        (
            "helper names the owner",
            format!(
                "    fn check(actual: u32, expected: u32) {{
        assert_eq!(actual, expected);
        let _ = score(1);
    }}
{}",
                test_fn(
                    "        let actual = score(2);
        check(actual, 4);
"
                )
            ),
        ),
        (
            "a compared operand is a conversion",
            format!(
                "    fn check(actual: u32, expected: u64) {{
        assert!(u64::from(actual) == expected);
    }}
{}",
                test_fn(
                    "        let actual = score(2);
        check(actual, 4);
"
                )
            ),
        ),
    ] {
        let finding = TempRepo::create(&source(&tests))?.changed()?;
        // Each decoy keeps the unconfirmed reading it had before the pin.
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{name}: {finding:?}"
        );
    }
    Ok(())
}
