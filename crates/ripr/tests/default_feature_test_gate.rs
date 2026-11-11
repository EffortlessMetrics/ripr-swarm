//! A test behind a plain `#[cfg(feature = "..")]` gate is compiled by a plain
//! `cargo test` exactly when the package's own default features switch the
//! gate on. Such a test's exact assertion is read like any other test's
//! (#8780, the rule RIPR-SPEC-0279 uses for doc examples); a gate the
//! defaults do not settle keeps the test's assertion uncredited.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIB: &str = "pub fn discount(amount: i32) -> i32 {\n    if amount >= 100 { amount - 10 } else { amount }\n}\n";

const DIFF_HEAD: &str = "@@ -1,3 +1,3 @@\n pub fn discount(amount: i32) -> i32 {\n-    if amount > 100 { amount - 10 } else { amount }\n+    if amount >= 100 { amount - 10 } else { amount }\n }\n";

const PIN_BODY: &str = "#[test]\nfn discount_boundary() {\n    assert_eq!(demo::discount(101), 91);\n    assert_eq!(demo::discount(100), 90);\n}\n";

fn manifest(features: &str) -> String {
    format!(
        "[package]\nname = \"demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[features]\n{features}\n[workspace]\n"
    )
}

fn diff(lib: &str) -> String {
    format!("diff --git a/{lib} b/{lib}\n--- a/{lib}\n+++ b/{lib}\n{DIFF_HEAD}")
}

struct Fixture(PathBuf);

impl Fixture {
    fn with_files(files: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-default-feature-test-gate-{}-{}",
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

    /// The changed-predicate finding in `lib`, after checking the gated test
    /// is its related test.
    fn changed_predicate(&self, lib: &str, test: &str) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let finding = output
            .findings
            .into_iter()
            .find(|finding| {
                finding.probe.location.file.ends_with(lib)
                    && finding.probe.family == ProbeFamily::Predicate
            })
            .ok_or_else(|| format!("missing changed predicate probe in {lib}"))?;
        assert!(
            finding
                .related_tests
                .iter()
                .any(|related| related.name == "discount_boundary"
                    && related.file == PathBuf::from(test)),
            "the gated test must be the related test under {test}: {finding:?}"
        );
        Ok(finding)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The class of the changed predicate in a single-package fixture whose
/// integration test is `gate` followed by the pinning test.
fn class_with(features: &str, gate: &str) -> Result<ExposureClass, String> {
    let fixture = Fixture::with_files(&[
        ("Cargo.toml", &manifest(features)),
        ("src/lib.rs", LIB),
        ("tests/pin.rs", &format!("{gate}{PIN_BODY}")),
        ("diff.patch", &diff("src/lib.rs")),
    ])?;
    Ok(fixture
        .changed_predicate("src/lib.rs", "tests/pin.rs")?
        .class)
}

#[test]
fn a_test_behind_a_default_enabled_feature_gate_pins_like_an_ungated_one() -> Result<(), String> {
    // Control: the same test with no gate is credited.
    assert_eq!(
        class_with("slow = []\n", "")?,
        ExposureClass::Exposed,
        "the ungated test is the control"
    );
    for (features, gate) in [
        // The file gate names a default feature directly.
        (
            "default = [\"slow\"]\nslow = []\n",
            "#![cfg(feature = \"slow\")]\n",
        ),
        // The default closure reaches the feature through another one.
        (
            "default = [\"std\"]\nstd = [\"slow\"]\nslow = []\n",
            "#![cfg(feature = \"slow\")]\n",
        ),
        // The gate sits on the test function itself.
        (
            "default = [\"slow\"]\nslow = []\n",
            "#[cfg(feature = \"slow\")]\n",
        ),
        (
            "default = [\"slow\"]\nslow = []\n",
            "#![cfg(all(test, feature = \"slow\"))]\n",
        ),
    ] {
        assert_eq!(
            class_with(features, gate)?,
            ExposureClass::Exposed,
            "a plain `cargo test` compiles this test: {features:?} {gate:?}"
        );
    }
    Ok(())
}

#[test]
fn a_gate_the_default_features_do_not_settle_keeps_the_assertion_uncredited() -> Result<(), String>
{
    for (features, gate) in [
        // The feature exists but is off by default.
        ("slow = []\n", "#![cfg(feature = \"slow\")]\n"),
        (
            "default = [\"std\"]\nstd = []\nslow = []\n",
            "#[cfg(feature = \"slow\")]\n",
        ),
        // A negation is never resolved, whichever way the defaults point.
        (
            "default = [\"slow\"]\nslow = []\n",
            "#![cfg(not(feature = \"absent\"))]\n",
        ),
        // A second, unresolved atom keeps the whole gate unresolved.
        (
            "default = [\"slow\"]\nslow = []\n",
            "#![cfg(all(feature = \"slow\", target_os = \"none\"))]\n",
        ),
    ] {
        assert_ne!(
            class_with(features, gate)?,
            ExposureClass::Exposed,
            "the default run is not known to compile this test: {features:?} {gate:?}"
        );
    }
    Ok(())
}

/// The feature set is the gated file's own package's: a workspace sibling's
/// defaults say nothing about it.
#[test]
fn the_default_features_are_read_from_the_gated_files_own_package() -> Result<(), String> {
    let member = |features: &str| {
        format!(
            "[package]\nname = \"demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[features]\n{features}"
        )
    };
    let root = |features: &str| {
        format!(
            "[package]\nname = \"root\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[features]\n{features}\n[workspace]\nmembers = [\"demo\"]\n"
        )
    };
    let class = |root_features: &str, member_features: &str| -> Result<ExposureClass, String> {
        let fixture = Fixture::with_files(&[
            ("Cargo.toml", &root(root_features)),
            ("src/lib.rs", "pub fn unrelated() {}\n"),
            ("demo/Cargo.toml", &member(member_features)),
            ("demo/src/lib.rs", LIB),
            (
                "demo/tests/pin.rs",
                &format!("#![cfg(feature = \"slow\")]\n{PIN_BODY}"),
            ),
            ("diff.patch", &diff("demo/src/lib.rs")),
        ])?;
        Ok(fixture
            .changed_predicate("demo/src/lib.rs", "demo/tests/pin.rs")?
            .class)
    };
    assert_eq!(
        class("slow = []\n", "default = [\"slow\"]\nslow = []\n")?,
        ExposureClass::Exposed,
        "the member's own default switches its gate on"
    );
    assert_ne!(
        class("default = [\"slow\"]\nslow = []\n", "slow = []\n")?,
        ExposureClass::Exposed,
        "the root package's default is not the member's"
    );
    Ok(())
}
