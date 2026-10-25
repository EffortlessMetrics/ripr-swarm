//! A discriminated test that reaches the owner only through a wrapper in
//! another package has no helper-transfer chain, so the name-token relation
//! alone must not read as an actionable `reachable_unrevealed` gap (#10127).
//! The finding names the transitive-reach limit and keeps its class. A
//! wrapper in the owner's own package keeps the inspect-first gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-cross-package-wrapper-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let target = self.0.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, content).map_err(|error| error.to_string())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const WORKSPACE: &str = "[workspace]\nmembers = [\"tally\", \"ledger\"]\nresolver = \"2\"\n";
const TALLY_MANIFEST: &str =
    "[package]\nname = \"tally\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n";
/// The corpus shape of the row: the owner's library root is a custom `[lib]`
/// path, so no source layout marks its package root.
const CUSTOM_LIB_MANIFEST: &str = "[package]\nname = \"tally\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\npath = \"lib/core.rs\"\n";
const LEDGER_MANIFEST: &str = "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\ntally = { path = \"../tally\" }\n";
const TALLY_LIB: &str = "pub fn weight(count: u32) -> u32 {\n    3 * count\n}\n";
const DIFF: &str = "diff --git a/tally/src/lib.rs b/tally/src/lib.rs\n--- a/tally/src/lib.rs\n+++ b/tally/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn weight(count: u32) -> u32 {\n-    count + count + count\n+    3 * count\n }\n";

fn findings(files: &[(&str, &str)]) -> Result<Vec<Finding>, String> {
    findings_with(TALLY_MANIFEST, "tally/src/lib.rs", files)
}

fn findings_with(
    tally_manifest: &str,
    owner_path: &str,
    files: &[(&str, &str)],
) -> Result<Vec<Finding>, String> {
    let scratch = Scratch::create()?;
    scratch.write("Cargo.toml", WORKSPACE)?;
    scratch.write("tally/Cargo.toml", tally_manifest)?;
    scratch.write("ledger/Cargo.toml", LEDGER_MANIFEST)?;
    for (path, content) in files {
        scratch.write(path, content)?;
    }
    scratch.write("change.diff", &DIFF.replace("tally/src/lib.rs", owner_path))?;
    let output = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .collect())
}

fn limit(finding: &Finding) -> Option<&'static str> {
    finding.static_limit_kind.map(|kind| kind.as_str())
}

#[test]
fn a_test_through_a_wrapper_in_another_package_names_the_reach_limit() -> Result<(), String> {
    let found = findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            ("tally/lib/core.rs", TALLY_LIB),
            (
                "ledger/src/lib.rs",
                "pub fn heavy(count: u32) -> u32 {\n    tally::weight(count)\n}\n",
            ),
            (
                "ledger/tests/ledger.rs",
                "#[test]\nfn heavy_triples_the_count() {\n    assert_eq!(ledger::heavy(5), 15);\n}\n",
            ),
        ],
    )?;
    let summary: Vec<_> = found
        .iter()
        .map(|finding| (finding.class.as_str(), limit(finding)))
        .collect();
    if found.iter().any(|finding| {
        finding.class == ExposureClass::ReachableUnrevealed && limit(finding).is_none()
    }) || !found.iter().any(|finding| limit(finding).is_some())
    {
        return Err(format!(
            "expected the transitive-reach limit, got {summary:?}"
        ));
    }
    Ok(())
}

#[test]
fn a_wrapper_in_the_owners_own_package_keeps_the_gap() -> Result<(), String> {
    let found = findings(&[
        (
            "tally/src/lib.rs",
            "pub fn weight(count: u32) -> u32 {\n    3 * count\n}\npub fn heavy(count: u32) -> u32 {\n    weight(count)\n}\n",
        ),
        (
            "tally/tests/tally.rs",
            "#[test]\nfn heavy_triples_the_count() {\n    assert_eq!(tally::heavy(5), 15);\n}\n",
        ),
        ("ledger/src/lib.rs", "pub fn unrelated() {}\n"),
    ])?;
    let summary: Vec<_> = found
        .iter()
        .map(|finding| (finding.class.as_str(), limit(finding)))
        .collect();
    if found.iter().any(|finding| limit(finding).is_some())
        || !found
            .iter()
            .any(|finding| finding.class == ExposureClass::ReachableUnrevealed)
    {
        return Err(format!("expected the kept gap, got {summary:?}"));
    }
    Ok(())
}
