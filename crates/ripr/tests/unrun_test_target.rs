//! Public-API controls for `[[test]]` targets `cargo test` never runs (#8016).
//!
//! A target declared `test = false`, `harness = false` or with unmet
//! `required-features` is a real Cargo target whose `#[test]` items libtest
//! never collects, so a boundary assertion inside it is not evidence.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const GATE: &str = "pub fn gate(input: u32) -> bool {\n    input >= 10\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn gate(input: u32) -> bool {
-    input > 10
+    input >= 10
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create_files(files: &[(&str, String)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-unrun-test-target-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        std::fs::create_dir_all(this.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            this.root.join("Cargo.toml"),
            "[package]\nname = \"assertion-helper-credit\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        for (relative, source) in files {
            let path = this.root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {relative} parent failed: {error}"))?;
            }
            std::fs::write(&path, source)
                .map_err(|error| format!("write {relative} failed: {error}"))?;
        }
        std::fs::write(this.root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(this)
    }

    /// The finding for the changed `input >= 10` predicate.
    fn predicate(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut predicates = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::Predicate && finding.probe.location.line == 2
        });
        match (predicates.next(), predicates.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one predicate finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn package_manifest(extra: &str) -> String {
    format!(
        "[package]\nname = \"unrun-test-target\"\nversion = \"0.0.0\"\nedition = \"2024\"\n{extra}\n[workspace]\n"
    )
}

const BOUNDARY_TEST: &str = "use unrun_test_target::gate;\n\n#[test]\nfn boundary() {\n    assert_eq!(gate(10), true);\n}\n";

fn finding_with(manifest_extra: &str, test_source: String) -> Result<Finding, String> {
    TempRepo::create_files(&[
        ("Cargo.toml", package_manifest(manifest_extra)),
        ("src/lib.rs", GATE.to_string()),
        ("tests/x.rs", test_source),
    ])?
    .predicate()
}

#[test]
fn live_target_baseline_exposes() -> Result<(), String> {
    let finding = finding_with("", BOUNDARY_TEST.to_string())?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "premise: {finding:?}"
    );
    Ok(())
}

#[test]
fn unrun_declared_targets_are_not_credited() -> Result<(), String> {
    let mut failures = Vec::new();
    for (label, extra, source) in [
        (
            "test = false",
            "\n[[test]]\nname = \"x\"\ntest = false\n",
            BOUNDARY_TEST.to_string(),
        ),
        (
            "harness = false",
            "\n[[test]]\nname = \"x\"\nharness = false\n",
            format!("{BOUNDARY_TEST}fn main() {{}}\n"),
        ),
        (
            "unmet required-features",
            "\n[features]\ndefault = []\nspecial = []\n[[test]]\nname = \"x\"\nrequired-features = [\"special\"]\n",
            BOUNDARY_TEST.to_string(),
        ),
        (
            "test = false with explicit path",
            "\n[[test]]\nname = \"x\"\npath = \"tests/x.rs\"\ntest = false\n",
            BOUNDARY_TEST.to_string(),
        ),
    ] {
        let finding = finding_with(extra, source)?;
        if finding.class == ExposureClass::Exposed {
            failures.push(label);
        }
    }
    assert!(
        failures.is_empty(),
        "never run by cargo test yet credited: {failures:?}"
    );
    Ok(())
}

#[test]
fn satisfied_required_features_stay_credited() -> Result<(), String> {
    let finding = finding_with(
        "\n[features]\ndefault = [\"special\"]\nspecial = []\n[[test]]\nname = \"x\"\nrequired-features = [\"special\"]\n",
        BOUNDARY_TEST.to_string(),
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
