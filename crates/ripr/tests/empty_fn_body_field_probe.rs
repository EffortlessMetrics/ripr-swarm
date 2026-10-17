//! #8450: a literally empty `fn` body in the owner file must not widen a
//! later field-construction probe to the whole changed line. The element
//! pin (RIPR-SPEC-0225 decision 9) reads the initializer `total: a + b + 0`;
//! the whole-line array `[Row { .. }, Row { .. }]` never matches.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-empty-fn-body-field-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"empty-fn-body-field\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn lib_with(prefix: &str) -> String {
    format!(
        "pub struct Row {{
    pub total: u32,
}}
{prefix}
pub fn rows(a: u32, b: u32) -> [Row; 2] {{
    [Row {{ total: 0 }}, Row {{ total: a + b + 0 }}]
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn pins_second_row() {{
        let rows = rows(30, 3);
        assert_eq!(rows[1].total, 33);
    }}
}}
"
    )
}

fn diff_for(lib: &str) -> Result<String, String> {
    let start = lib
        .lines()
        .position(|line| line.starts_with("pub fn rows("))
        .map(|index| index + 1)
        .ok_or("fixture lost the owner")?;
    Ok(format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{start},3 +{start},3 @@
 pub fn rows(a: u32, b: u32) -> [Row; 2] {{
-    [Row {{ total: 0 }}, Row {{ total: a + b }}]
+    [Row {{ total: 0 }}, Row {{ total: a + b + 0 }}]
 }}
"
    ))
}

fn field_finding(prefix: &str) -> Result<Finding, String> {
    let lib = lib_with(prefix);
    let findings = TempRepo::create(&lib, &diff_for(&lib)?)?.findings()?;
    findings
        .into_iter()
        .find(|finding| finding.probe.family == ProbeFamily::FieldConstruction)
        .ok_or_else(|| "premise: a field_construction finding".to_string())
}

#[test]
fn an_empty_fn_body_in_the_file_still_synthesizes_the_field_initializer() -> Result<(), String> {
    for (label, prefix) in [
        (
            "empty Drop",
            "impl Drop for Row {\n    fn drop(&mut self) {}\n}\n",
        ),
        (
            "one-line empty Drop",
            "impl Drop for Row { fn drop(&mut self) {} }\n",
        ),
        ("free empty fn", "fn helper() {}\n"),
        (
            "comment-only Drop",
            "impl Drop for Row {\n    fn drop(&mut self) { /* no-op */ }\n}\n",
        ),
    ] {
        let finding = field_finding(prefix)?;
        if finding.probe.expression != "total: a + b + 0" {
            return Err(format!(
                "{label} must keep the field initializer, got {:?}",
                finding.probe.expression
            ));
        }
        if finding.class != ExposureClass::Exposed {
            return Err(format!(
                "{label} must still evaluate the element pin, got {:?}",
                finding.class
            ));
        }
    }
    Ok(())
}
