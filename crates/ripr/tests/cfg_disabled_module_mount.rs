//! A `mod` declaration behind a `#[cfg]` that no build satisfies mounts
//! nothing (#15877): rustc strips `#[cfg(any())] mod helper;` before it
//! loads `helper.rs`, so a change there is not behavior any build runs.
//! Each case pairs the disabled gate with an unconditional or
//! feature-gated control that still reports the same change.

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str =
    "[package]\nname = \"price\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n";

const BULK: &str = "pub fn bulk(qty: u32) -> bool {\n    qty >= 10\n}\n";

fn predicate_diff(path: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n pub fn bulk(qty: u32) -> bool {{\n-    qty > 10\n+    qty >= 10\n }}\n"
    )
}

struct Fixture(PathBuf);

impl Fixture {
    /// A library whose `src/lib.rs` is `lib`, with `changed` holding the
    /// changed predicate.
    fn library(lib: &str, changed: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-cfg-disabled-module-mount-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let diff = predicate_diff(changed);
        for (relative, text) in [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", lib),
            (changed, BULK),
            ("diff.patch", diff.as_str()),
        ] {
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

    /// Whether the check reported at least one finding in `changed`.
    fn reports(&self, changed: &str) -> Result<bool, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        Ok(output.findings.iter().any(|finding| {
            let file = &finding.probe.location.file;
            let relative = file.strip_prefix(&self.0).unwrap_or(file);
            let relative = relative.to_string_lossy().replace('\\', "/");
            Path::new(relative.trim_start_matches("./")) == Path::new(changed)
        }))
    }
}

impl Fixture {
    /// The exposure class of every finding in `changed`.
    fn classes(&self, changed: &str) -> Result<Vec<ExposureClass>, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        Ok(output
            .findings
            .iter()
            .filter(|finding| {
                let file = &finding.probe.location.file;
                let relative = file.strip_prefix(&self.0).unwrap_or(file);
                let relative = relative.to_string_lossy().replace('\\', "/");
                Path::new(relative.trim_start_matches("./")) == Path::new(changed)
            })
            .map(|finding| finding.class.clone())
            .collect())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_mod_behind_a_cfg_no_build_satisfies_mounts_nothing() -> Result<(), String> {
    for (lib, expected, why) in [
        ("pub mod helper;\n", true, "control: unconditional mount"),
        (
            "#[cfg(feature = \"extra\")]\npub mod helper;\n",
            true,
            "control: a feature gate may be enabled",
        ),
        (
            "#[cfg(not(test))]\npub mod helper;\n",
            true,
            "control: `not(test)` is enabled in every non-test build",
        ),
        (
            "#[cfg(any())]\npub mod helper;\npub fn unrelated() {}\n",
            false,
            "`any()` is false in every build",
        ),
        (
            "#[cfg(all(unix, not(all())))]\npub mod helper;\npub fn unrelated() {}\n",
            false,
            "`not(all())` makes the conjunction false whatever `unix` is",
        ),
    ] {
        let fixture = Fixture::library(lib, "src/helper.rs")?;
        assert_eq!(fixture.reports("src/helper.rs")?, expected, "{why}: {lib}");
    }
    Ok(())
}

#[test]
fn a_disabled_inline_module_mounts_none_of_its_children() -> Result<(), String> {
    let enabled = Fixture::library(
        "pub mod outer {\n    pub mod helper;\n}\n",
        "src/outer/helper.rs",
    )?;
    assert!(
        enabled.reports("src/outer/helper.rs")?,
        "control: enabled inline module"
    );

    let disabled = Fixture::library(
        "#[cfg(any())]\npub mod outer {\n    pub mod helper;\n}\npub fn unrelated() {}\n",
        "src/outer/helper.rs",
    )?;
    assert!(
        !disabled.reports("src/outer/helper.rs")?,
        "rustc strips the inline module before loading `outer/helper.rs`"
    );
    Ok(())
}

/// A test file mounted only behind a `#[cfg]` no build satisfies never
/// runs, so it supplies no discriminator; the same test under
/// `#[cfg(test)]` runs and keeps the credit (#15877).
#[test]
fn a_test_file_behind_a_cfg_no_build_satisfies_credits_nothing() -> Result<(), String> {
    let checks = "use super::bulk;\n\n#[test]\nfn threshold() {\n    assert_eq!(bulk(10), true);\n    assert_eq!(bulk(9), false);\n}\n";
    let class_for = |gate: &str| -> Result<Vec<ExposureClass>, String> {
        let lib = format!("{BULK}#[cfg({gate})]\nmod checks;\n");
        let fixture = Fixture::library(&lib, "src/lib.rs")?;
        std::fs::write(fixture.0.join("src/checks.rs"), checks)
            .map_err(|error| format!("write checks: {error}"))?;
        // `Fixture::library` wrote `BULK` over `src/lib.rs`; restore the
        // mount after it.
        std::fs::write(fixture.0.join("src/lib.rs"), &lib)
            .map_err(|error| format!("write lib: {error}"))?;
        fixture.classes("src/lib.rs")
    };
    let enabled = class_for("test")?;
    assert!(
        !enabled.is_empty() && enabled.iter().all(|class| *class == ExposureClass::Exposed),
        "control: the cfg(test) module's exact test notices the change: {enabled:?}"
    );
    let disabled = class_for("any()")?;
    assert!(
        !disabled.is_empty()
            && disabled
                .iter()
                .all(|class| *class == ExposureClass::NoStaticPath),
        "a test no build compiles supplies no path: {disabled:?}"
    );
    Ok(())
}
