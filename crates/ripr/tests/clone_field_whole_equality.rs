//! Public-API controls for clone-field whole-value credit (#6692).
//!
//! A field set in a hand-written `impl Clone` is observed by
//! `assert_eq!(recv.clone(), recv)` when `==` is the derived field-by-field
//! comparison. A hand-written `PartialEq`, `assert_ne!`, or a comparison
//! with some other value gives no such credit.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const DERIVED: &str = "#[derive(Debug, PartialEq, Eq)]";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -14,7 +14,7 @@ impl Window {
 impl Clone for Window {
     fn clone(&self) -> Self {
         Window {
-            start: self.end,
+            start: self.start,
             end: self.end,
         }
     }
";

/// `src/lib.rs` with the changed `start: self.start,` on line 17.
fn source(derive: &str, extra: &str, assertion: &str) -> String {
    format!(
        "{derive}
pub struct Window {{
    start: u32,
    end: u32,
}}

impl Window {{
    pub fn new(start: u32, end: u32) -> Self {{
        Window {{ start, end }}
    }}
}}


impl Clone for Window {{
    fn clone(&self) -> Self {{
        Window {{
            start: self.start,
            end: self.end,
        }}
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn a_clone_equals_its_original() {{
        let window = Window::new(3, 9);
        {assertion}
    }}
}}
{extra}"
    )
}

/// #6905: `src/lib.rs` with the changed `start: self.start,` on line 17
/// and a `mod tests` declaring its own same-name `Window` with a derived
/// `Clone`. The test's `window.clone()` runs the test-local clone, never
/// the changed owner, so the field must stay non-exposed.
fn shadowed_source(assertion: &str) -> String {
    format!(
        "{DERIVED}
pub struct Window {{
    start: u32,
    end: u32,
}}

impl Window {{
    pub fn new(start: u32, end: u32) -> Self {{
        Window {{ start, end }}
    }}
}}


impl Clone for Window {{
    fn clone(&self) -> Self {{
        Window {{
            start: self.start,
            end: self.end,
        }}
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Window {{
        start: u32,
        end: u32,
    }}

    #[test]
    fn a_clone_equals_its_original() {{
        let window = Window {{ start: 3, end: 9 }};
        {assertion}
    }}
}}
"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-clone-field-whole-equality-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"clone-field\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    /// The field-construction finding for the changed line 17.
    fn field(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut fields = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::FieldConstruction
                && finding.probe.location.line == 17
                && finding.probe.expression.contains("self.start")
        });
        match (fields.next(), fields.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one field finding on line 17, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_clone_compared_with_its_original_through_derived_equality_is_exposed() -> Result<(), String> {
    let finding =
        TempRepo::create(&source(DERIVED, "", "assert_eq!(window.clone(), window);"))?.field()?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding.activation.missing_discriminators.is_empty(),
        "the clone pin observes the field: {finding:?}"
    );
    Ok(())
}

#[test]
fn a_test_module_shadow_of_the_receiver_is_not_exposed() -> Result<(), String> {
    let finding =
        TempRepo::create(&shadowed_source("assert_eq!(window.clone(), window);"))?.field()?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "the shadowed clone runs the test-local type: {finding:?}"
    );
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| format!("{:?}", fact.flow_sink).contains("StructField")),
        "the struct-field gap must survive without a credited pin: {finding:?}"
    );
    Ok(())
}

#[test]
fn clone_field_credit_refuses_manual_equality_inequality_and_other_values() -> Result<(), String> {
    let manual = "impl PartialEq for Window {\n    fn eq(&self, other: &Self) -> bool {\n        self.end == other.end\n    }\n}\n";
    for (derive, extra, assertion) in [
        (
            "#[derive(Debug, Eq)]",
            manual,
            "assert_eq!(window.clone(), window);",
        ),
        (
            DERIVED,
            "",
            "assert_ne!(window.clone(), Window::new(0, 0));",
        ),
        (
            DERIVED,
            "",
            "assert_eq!(window.clone(), Window::new(3, 9));",
        ),
    ] {
        let finding = TempRepo::create(&source(derive, extra, assertion))?.field()?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{derive} {assertion}: {finding:?}"
        );
        assert!(
            finding
                .activation
                .missing_discriminators
                .iter()
                .any(|fact| format!("{:?}", fact.flow_sink).contains("StructField")),
            "the struct-field gap must survive without a credited pin: {derive} {assertion}: {finding:?}"
        );
    }
    Ok(())
}
