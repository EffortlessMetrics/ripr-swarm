//! A `?` inside an assertion's operands in a test that returns `Result`
//! (`assert_eq!(parse(..)?, 91)`) can only leave the test with an error,
//! which fails it, so it cannot skip the comparison and let the test pass.
//! Such a test keeps its exact assertion's credit like a root `?` already
//! does (RIPR-SPEC-0197 rule 3, #14116). A `?` whose error a caller may
//! discard (in a closure, an `async` block, a nested `fn`, or a helper) still
//! refuses the pin.

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str =
    "[package]\nname = \"demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n";

const LIB: &str = "pub fn discount(amount: i32) -> Result<i32, String> {\n    if amount >= 100 { Ok(amount - 10) } else { Ok(amount) }\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(amount: i32) -> Result<i32, String> {\n-    if amount > 100 { Ok(amount - 10) } else { Ok(amount) }\n+    if amount >= 100 { Ok(amount - 10) } else { Ok(amount) }\n }\n";

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The class of the changed predicate when `tests/pin.rs` holds `test`,
/// which must define `discount_boundary`.
fn class_with(test: &str) -> Result<ExposureClass, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "ripr-try-operand-in-test-assertion-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    for (relative, text) in [
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", LIB),
        ("tests/pin.rs", test),
        ("diff.patch", DIFF),
    ] {
        let path = fixture.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create directory {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, text)
            .map_err(|error| format!("write file {}: {error}", path.display()))?;
    }
    let output = check_workspace(CheckInput {
        root: fixture.0.clone(),
        diff_file: Some(fixture.0.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let finding = output
        .findings
        .into_iter()
        .find(|finding| finding.probe.family == ProbeFamily::Predicate)
        .ok_or("missing changed predicate probe")?;
    assert!(
        finding
            .related_tests
            .iter()
            .any(|related| related.name == "discount_boundary"),
        "the boundary test must be the related test: {finding:?}"
    );
    Ok(finding.class)
}

#[test]
fn a_try_in_an_assertion_operand_of_a_result_test_pins_like_a_root_try() -> Result<(), String> {
    // Control: the value is unwrapped by a root `?` before the comparison.
    assert_eq!(
        class_with(
            "#[test]\nfn discount_boundary() -> Result<(), String> {\n    let above = demo::discount(101)?;\n    assert_eq!(above, 91);\n    let at = demo::discount(100)?;\n    assert_eq!(at, 90);\n    Ok(())\n}\n"
        )?,
        ExposureClass::Exposed,
        "the root `?` is the control"
    );
    assert_eq!(
        class_with(
            "#[test]\nfn discount_boundary() -> Result<(), String> {\n    assert_eq!(demo::discount(101)?, 91);\n    assert_eq!(demo::discount(100)?, 90);\n    Ok(())\n}\n"
        )?,
        ExposureClass::Exposed,
        "an error from the operand's `?` fails the test"
    );
    Ok(())
}

#[test]
fn a_try_whose_error_may_be_discarded_keeps_the_assertion_uncredited() -> Result<(), String> {
    for test in [
        // A closure's `?` returns to the closure's caller, which drops it.
        "#[test]\nfn discount_boundary() {\n    let check = || -> Result<(), String> {\n        assert_eq!(demo::discount(101)?, 91);\n        assert_eq!(demo::discount(100)?, 90);\n        Ok(())\n    };\n    let _ = check();\n}\n",
        // A `return` in the operand may still leave the test passing.
        "#[test]\nfn discount_boundary() -> Result<(), String> {\n    assert_eq!(demo::discount(101)?, 91);\n    assert_eq!(match demo::discount(100) { Ok(v) => v, Err(_) => return Ok(()) }, 90);\n    Ok(())\n}\n",
        // An `async` block's `?` is the block's own error.
        "#[test]\nfn discount_boundary() {\n    let check = async {\n        assert_eq!(demo::discount(101)?, 91);\n        assert_eq!(demo::discount(100)?, 90);\n        Ok::<(), String>(())\n    };\n    drop(check);\n}\n",
    ] {
        assert_ne!(
            class_with(test)?,
            ExposureClass::Exposed,
            "the `?` may leave without failing the test: {test}"
        );
    }
    Ok(())
}
