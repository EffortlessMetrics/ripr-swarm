//! Literal/comment contents cannot create or hide a Rust FFI binding attribute.

use ripr::domain::StaticLimitKind;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = include_str!("../../../fixtures/rust_ffi_attribute_literals/input/src/lib.rs");
const MANIFEST: &str =
    include_str!("../../../fixtures/rust_ffi_attribute_literals/input/Cargo.toml");
const DIFF: &str = include_str!("../../../fixtures/rust_ffi_attribute_literals/diff.patch");

struct Fixture(PathBuf);

impl Fixture {
    fn create() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-ffi-attribute-literals-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Self(root);
        std::fs::create_dir_all(fixture.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("Cargo.toml"), MANIFEST).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("src/lib.rs"), SOURCE).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("diff.patch"), DIFF).map_err(|e| e.to_string())?;
        Ok(fixture)
    }

    fn finding(&self, line: usize) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        assert_eq!(output.findings.len(), 3, "three changed predicates");
        let finding = output
            .findings
            .into_iter()
            .find(|finding| {
                finding.probe.location.line == line
                    && finding.probe.location.file.ends_with("src/lib.rs")
            })
            .ok_or_else(|| format!("missing predicate at src/lib.rs:{line}"))?;
        assert_eq!(finding.class, ExposureClass::NoStaticPath);
        assert!(finding.related_tests.is_empty());
        Ok(finding)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn raw_doc_string_cannot_invent_a_binding_attribute() -> Result<(), String> {
    let finding = Fixture::create()?.finding(3)?;
    assert_eq!(finding.static_limit_kind, None);
    assert!(
        !finding
            .recommended_next_step
            .as_deref()
            .unwrap_or_default()
            .contains("language boundary")
    );
    Ok(())
}

#[test]
fn raw_doc_string_cannot_hide_a_real_binding_attribute() -> Result<(), String> {
    let finding = Fixture::create()?.finding(8)?;
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
    );
    assert!(
        finding
            .recommended_next_step
            .as_deref()
            .unwrap_or_default()
            .contains("language boundary")
    );
    Ok(())
}

#[test]
fn comment_cannot_invent_a_binding_attribute() -> Result<(), String> {
    let finding = Fixture::create()?.finding(13)?;
    assert_eq!(finding.static_limit_kind, None);
    Ok(())
}
