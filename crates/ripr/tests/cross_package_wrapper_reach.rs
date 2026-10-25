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
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
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
    if !found.iter().any(|finding| {
        finding.class == ExposureClass::ReachableUnrevealed && limit(finding).is_some()
    }) {
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

/// Asserts the owner's `reachable_unrevealed` gap is kept: no static limit
/// and the class unchanged.
fn assert_gap_kept(found: &[Finding]) -> Result<(), String> {
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

#[test]
fn a_custom_lib_owner_with_a_same_package_wrapper_keeps_the_gap() -> Result<(), String> {
    // The owner file has no source-layout marker, so a path-shape package
    // comparison would read the package's own `tests/` file as foreign.
    assert_gap_kept(&findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            (
                "tally/lib/core.rs",
                "pub fn weight(count: u32) -> u32 {\n    3 * count\n}\npub fn heavy(count: u32) -> u32 {\n    weight(count)\n}\n",
            ),
            (
                "tally/tests/tally.rs",
                "#[test]\nfn heavy_triples_the_count() {\n    assert_eq!(tally::heavy(5), 15);\n}\n",
            ),
            ("ledger/src/lib.rs", "pub fn unrelated() {}\n"),
        ],
    )?)
}

#[test]
fn a_dependent_that_defines_its_own_function_of_the_owners_name_keeps_the_gap() -> Result<(), String>
{
    // `ledger::heavy` calls ledger's own `weight`, never `tally::weight`.
    assert_gap_kept(&findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            ("tally/lib/core.rs", TALLY_LIB),
            (
                "ledger/src/lib.rs",
                "pub fn weight(count: u32) -> u32 {\n    3 * count\n}\npub fn heavy(count: u32) -> u32 {\n    weight(count)\n}\n",
            ),
            (
                "ledger/tests/ledger.rs",
                "#[test]\nfn heavy_triples_the_count() {\n    assert_eq!(ledger::heavy(5), 15);\n}\n",
            ),
        ],
    )?)
}

#[test]
fn a_dependent_calling_a_wrapper_defined_in_the_owners_package_keeps_the_gap() -> Result<(), String>
{
    // `tally::heavy` and the changed `tally::weight` share a package; the
    // test merely sits in a dependent, so no cross-package wrapper exists.
    assert_gap_kept(&findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            (
                "tally/lib/core.rs",
                "pub fn weight(count: u32) -> u32 {\n    3 * count\n}\npub fn heavy(count: u32) -> u32 {\n    weight(count)\n}\n",
            ),
            ("ledger/src/lib.rs", "pub fn unrelated() {}\n"),
            (
                "ledger/tests/ledger.rs",
                "#[test]\nfn heavy_triples_the_count() {\n    assert_eq!(tally::heavy(5), 15);\n}\n",
            ),
        ],
    )?)
}

#[test]
fn an_unused_namesake_wrapper_in_the_dependent_does_not_hide_an_owner_package_wrapper()
-> Result<(), String> {
    // The test calls `tally::heavy`; ledger's own `heavy` is never used.
    assert_gap_kept(&findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            (
                "tally/lib/core.rs",
                "pub fn weight(count: u32) -> u32 {\n    3 * count\n}\npub fn heavy(count: u32) -> u32 {\n    weight(count)\n}\n",
            ),
            (
                "ledger/src/lib.rs",
                "pub fn heavy(count: u32) -> u32 {\n    3 * count\n}\n",
            ),
            (
                "ledger/tests/ledger.rs",
                "#[test]\nfn heavy_triples_the_count() {\n    assert_eq!(tally::heavy(5), 15);\n}\n",
            ),
        ],
    )?)
}

/// The stop reason names which witness route produced the limit, so each
/// route's admission is pinned separately (#10288).
fn stop_reasons(found: &[Finding]) -> Vec<String> {
    found
        .iter()
        .filter(|finding| finding.class == ExposureClass::ReachableUnrevealed)
        .flat_map(|finding| {
            finding
                .stop_reasons
                .iter()
                .map(|reason| format!("{reason:?}"))
        })
        .collect()
}

fn assert_route_limited(found: &[Finding], route: &str) -> Result<(), String> {
    if !stop_reasons(found).iter().any(|reason| reason == route) {
        return Err(format!(
            "expected {route}, got {:?} limits {:?}",
            stop_reasons(found),
            found.iter().map(limit).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

/// `ledger::heavy` reaches the owner only through a `macro_rules!` macro, so
/// the macro-blocked route (not the lexical transitive one) is the witness.
const MACRO_WRAPPER: &str = "macro_rules! tripled {\n    ($n:expr) => {\n        tally::weight($n)\n    };\n}\npub fn heavy(count: u32) -> u32 {\n    tripled!(count)\n}\n";
const MACRO_TEST: &str =
    "#[test]\nfn heavy_triples_the_count() {\n    assert_eq!(ledger::heavy(5), 15);\n}\n";

#[test]
fn a_macro_wrapper_in_another_package_names_the_macro_reach_limit() -> Result<(), String> {
    let found = findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            ("tally/lib/core.rs", TALLY_LIB),
            ("ledger/src/lib.rs", MACRO_WRAPPER),
            ("ledger/tests/ledger.rs", MACRO_TEST),
        ],
    )?;
    assert_route_limited(&found, "MacroReachUnresolved")
}

#[test]
fn a_macro_wrapper_in_a_dependent_that_never_calls_the_owner_keeps_the_gap() -> Result<(), String> {
    // The macro mentions a `weight` that resolves to ledger's own function.
    assert_gap_kept(&findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            ("tally/lib/core.rs", TALLY_LIB),
            (
                "ledger/src/lib.rs",
                "pub fn weight(count: u32) -> u32 {\n    3 * count\n}\nmacro_rules! tripled {\n    ($n:expr) => {\n        weight($n)\n    };\n}\npub fn heavy(count: u32) -> u32 {\n    tripled!(count)\n}\n",
            ),
            ("ledger/tests/ledger.rs", MACRO_TEST),
        ],
    )?)
}

#[test]
fn a_macro_wrapper_in_the_owners_own_package_keeps_the_gap() -> Result<(), String> {
    assert_gap_kept(&findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            (
                "tally/lib/core.rs",
                &format!("pub fn weight(count: u32) -> u32 {{\n    3 * count\n}}\n{MACRO_WRAPPER}")
                    .replace("tally::weight", "weight"),
            ),
            (
                "tally/tests/tally.rs",
                &MACRO_TEST.replace("ledger::", "tally::"),
            ),
            ("ledger/src/lib.rs", "pub fn unrelated() {}\n"),
        ],
    )?)
}

/// Six wrappers deep, past `MAX_TRANSITIVE_DEPTH`, so the deep-transitive
/// route supplies the witness.
fn deep_chain(final_call: &str) -> String {
    let mut source = String::new();
    for hop in 1..6 {
        source.push_str(&format!(
            "pub fn hop{hop}(count: u32) -> u32 {{\n    hop{}(count)\n}}\n",
            hop + 1
        ));
    }
    source.push_str(&format!(
        "pub fn hop6(count: u32) -> u32 {{\n    {final_call}(count)\n}}\n"
    ));
    source
}

const DEEP_TEST: &str =
    "#[test]\nfn hop_triples_the_count() {\n    assert_eq!(ledger::hop1(5), 15);\n}\n";

#[test]
fn a_deep_wrapper_chain_in_another_package_names_the_reach_limit() -> Result<(), String> {
    let found = findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            ("tally/lib/core.rs", TALLY_LIB),
            ("ledger/src/lib.rs", &deep_chain("tally::weight")),
            ("ledger/tests/ledger.rs", DEEP_TEST),
        ],
    )?;
    assert_route_limited(&found, "TransitiveReachUnresolved")
}

#[test]
fn a_deep_wrapper_chain_in_the_owners_own_package_keeps_the_gap() -> Result<(), String> {
    assert_gap_kept(&findings_with(
        CUSTOM_LIB_MANIFEST,
        "tally/lib/core.rs",
        &[
            (
                "tally/lib/core.rs",
                &format!(
                    "pub fn weight(count: u32) -> u32 {{\n    3 * count\n}}\n{}",
                    deep_chain("weight")
                ),
            ),
            (
                "tally/tests/tally.rs",
                &DEEP_TEST.replace("ledger::", "tally::"),
            ),
            ("ledger/src/lib.rs", "pub fn unrelated() {}\n"),
        ],
    )?)
}
