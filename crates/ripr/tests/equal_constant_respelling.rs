//! #14938: a returned constant respelled as the same value (`65` as `0x41`,
//! `"A"` as `"\x41"`, `1.0` as `1e0`) changes no behavior, so no test can
//! tell the two apart and the edit must not read `exposed`. It seeds no probe
//! on either side of the diff. A changed value under the same exact pin, and
//! a suffix change that can change the inferred type, keep their finding.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// A crate whose `value() -> ty` returns `new`, changed from `old`, and
    /// whose only test pins `value()` to `expected`.
    fn create(ty: &str, old: &str, new: &str, expected: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-equal-constant-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"equal-constant\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let lib = format!(
            "pub fn value() -> {ty} {{
    {new}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn pins() {{
        assert_eq!(value(), {expected});
    }}
}}
"
        );
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        let diff = format!(
            "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn value() -> {ty} {{
-    {old}
+    {new}
 }}
"
        );
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        Ok(check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?
        .findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn an_equal_value_respelling_seeds_no_finding() -> Result<(), String> {
    for (ty, old, new, expected) in [
        ("&'static str", "\"A\"", "\"\\x41\"", "\"A\""),
        ("&'static str", "\"A\"", "r#\"A\"#", "\"A\""),
        ("char", "'A'", "'\\u{41}'", "'A'"),
        ("u8", "b'A'", "b'\\x41'", "b'A'"),
        ("i32", "65", "0x41", "65"),
        ("i32", "return 65;", "return 0x41;", "65"),
        ("i32", "0", "-0", "0"),
        ("i32", "-128", "-0x80", "-128"),
        ("f64", "1.0", "1e0", "1.0"),
        ("[i32; 2]", "[1, 2]", "[0x1, 0b10]", "[1, 2]"),
        // A comparison inside the value is the same constant too.
        (
            "bool",
            "return true == false;",
            "return !false == false;",
            "false",
        ),
    ] {
        let findings = TempRepo::create(ty, old, new, expected)?.findings()?;
        if !findings.is_empty() {
            return Err(format!(
                "`{old}` respelled as `{new}` is the same value and must seed no finding, got {:?}",
                findings
                    .iter()
                    .map(|finding| (&finding.probe.expression, &finding.class))
                    .collect::<Vec<_>>()
            ));
        }
    }
    Ok(())
}

/// Controls: a different value under the same exact pin stays exposed, so
/// the test above discriminates the value rather than the shape.
#[test]
fn a_changed_value_keeps_its_exposed_finding() -> Result<(), String> {
    for (ty, old, new, expected) in [
        ("&'static str", "\"A\"", "\"B\"", "\"B\""),
        ("i32", "65", "0x42", "66"),
        ("f64", "1.0", "1.5", "1.5"),
    ] {
        let findings = TempRepo::create(ty, old, new, expected)?.findings()?;
        if !findings.iter().any(|finding| {
            finding.probe.location.line == 2 && finding.class == ExposureClass::Exposed
        }) {
            return Err(format!(
                "premise: `{old}` changed to `{new}` under an exact pin reads exposed, got {:?}",
                findings
                    .iter()
                    .map(|finding| (&finding.probe.expression, &finding.class))
                    .collect::<Vec<_>>()
            ));
        }
    }
    Ok(())
}

/// A suffix change can change the inferred type, so it stays a finding.
#[test]
fn a_suffix_change_keeps_its_finding() -> Result<(), String> {
    let findings = TempRepo::create("i8", "1", "1i8", "1")?.findings()?;
    if findings
        .iter()
        .any(|finding| finding.probe.location.line == 2)
    {
        Ok(())
    } else {
        Err(format!("`1` to `1i8` keeps a finding, got {findings:?}"))
    }
}

/// Review controls: an equal value whose statement or evaluation changed
/// keeps its finding. `65;` becoming `return 0x41;` adds an early return,
/// and `-(-128i8)` overflows `i8` where `128i8` wraps.
#[test]
fn an_equal_value_with_changed_behavior_keeps_its_finding() -> Result<(), String> {
    for (ty, old, new, expected) in [
        ("i32", "65;", "return 0x41;", "65"),
        ("i8", "-(-128i8)", "128i8", "-128"),
        (
            "bool",
            "return true == false;",
            "return true == true;",
            "true",
        ),
    ] {
        let findings = TempRepo::create(ty, old, new, expected)?.findings()?;
        if !findings
            .iter()
            .any(|finding| finding.probe.location.line == 2)
        {
            return Err(format!(
                "`{old}` to `{new}` keeps a finding, got {findings:?}"
            ));
        }
    }
    Ok(())
}
