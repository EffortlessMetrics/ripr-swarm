//! A test in an explicitly declared Cargo target outside the autodiscovered
//! layout (`[[test]] path = "it/pin.rs"`) is its own crate root, so its
//! crate-qualified call of the changed public owner is credited like the same
//! test under `tests/`. A file no target declares stays unplaced.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const PACKAGE: &str = r###"[package]
name = "declared-target-pin"
version = "0.0.0"
edition = "2024"

[workspace]
"###;

const DECLARED_TEST: &str = r###"
[[test]]
name = "pin"
path = "it/pin.rs"
"###;

const LIB: &str = "pub fn total(amount: i32) -> i32 {\n    amount + 2\n}\n";

const PIN: &str =
    "#[test]\nfn total_adds_two() {\n    assert_eq!(declared_target_pin::total(50), 52);\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn total(amount: i32) -> i32 {\n-    amount + 1\n+    amount + 2\n }\n";

struct Fixture(PathBuf);

impl Fixture {
    fn with_files(files: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-declared-target-owner-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir_all(&fixture.0)
            .map_err(|error| format!("create fixture root {}: {error}", fixture.0.display()))?;
        for (relative, text) in files {
            let path = fixture.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create directory {}: {error}", parent.display()))?;
            }
            std::fs::write(&path, text)
                .map_err(|error| format!("write file {}: {error}", path.display()))?;
        }
        Ok(fixture)
    }

    fn changed_return(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        output
            .findings
            .into_iter()
            .find(|finding| {
                finding.probe.location.file.ends_with("src/lib.rs")
                    && finding.probe.family == ProbeFamily::ReturnValue
            })
            .ok_or_else(|| "missing changed return-value probe in src/lib.rs".to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn changed_return(manifest: &str, test_path: &str) -> Result<Finding, String> {
    Fixture::with_files(&[
        ("Cargo.toml", manifest),
        ("src/lib.rs", LIB),
        (test_path, PIN),
        ("diff.patch", DIFF),
    ])?
    .changed_return()
}

/// The finding's class, after checking the pinning test is its related test.
fn pinned_class(manifest: &str, test_path: &str) -> Result<ExposureClass, String> {
    let finding = changed_return(manifest, test_path)?;
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.name == "total_adds_two" && test.file == PathBuf::from(test_path)),
        "the pinning test must be the related test under {test_path}: {finding:?}"
    );
    Ok(finding.class)
}

#[test]
fn a_declared_test_target_outside_tests_dir_pins_like_an_autodiscovered_one() -> Result<(), String>
{
    // Baseline: the autodiscovered layout credits the exact pin.
    assert_eq!(
        pinned_class(PACKAGE, "tests/pin.rs")?,
        ExposureClass::Exposed,
        "the same test under tests/ is the control"
    );
    // The same test in a declared `[[test]] path` target is its own crate
    // root too, so the crate-qualified call names the library.
    let declared = format!("{PACKAGE}{DECLARED_TEST}");
    assert_eq!(
        pinned_class(&declared, "it/pin.rs")?,
        ExposureClass::Exposed,
        "a declared test target is a recognized crate root"
    );
    Ok(())
}

#[test]
fn an_undeclared_file_outside_the_layout_stays_unplaced() -> Result<(), String> {
    // No Cargo target names `it/pin.rs`, so no crate is known to compile it
    // and its assertion earns no credit.
    assert_ne!(
        changed_return(PACKAGE, "it/pin.rs")?.class,
        ExposureClass::Exposed,
        "an orphan file is not a crate root"
    );
    Ok(())
}
