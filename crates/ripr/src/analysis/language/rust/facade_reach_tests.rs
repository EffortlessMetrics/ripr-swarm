//! #9982: a workspace facade crate that re-exports the owner's crate and
//! carries the tests names the unresolved re-export path instead of reading
//! as a definite `no_static_path`. Production path: `analyze_diff` over real
//! manifests and sources. The limitation never changes the class and never
//! adds a related test.
use super::RustAdapter;
use crate::analysis::language::LanguageAdapter;
use crate::analysis::{AnalysisMode, AnalysisOptions, diff};
use crate::config::OraclePolicy;
use crate::domain::{ExposureClass, Finding, StaticLimitKind, StopReason};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const INNER_LIB: &str = "pub fn bump_counter(buf: &mut Vec<u8>, byte: u8) -> usize {\n    buf.push(byte + 1);\n    buf.len()\n}\n\npub trait Ext {\n    fn go(&mut self, byte: u8) -> usize;\n}\n\nimpl Ext for Vec<u8> {\n    fn go(&mut self, byte: u8) -> usize {\n        self.push(byte);\n        self.len()\n    }\n}\n";
const FACADE_TEST: &str = "use facade::Ext;\n\n#[test]\nfn go_pushes() {\n    let mut v = Vec::new();\n    assert_eq!(v.go(3), 1);\n}\n";

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, text).map_err(|e| e.to_string())
}

/// `inner` holds the owner; `facade` optionally depends on it
/// (`facade_edge`), re-exports it per `facade_lib`, and holds `facade_test`.
fn finding_for(
    label: &str,
    facade_edge: bool,
    facade_lib: &str,
    facade_test: &str,
) -> Result<Finding, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root: PathBuf = std::env::temp_dir().join(format!("ripr-facade-reach-{label}-{stamp}"));
    write(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"inner\", \"facade\"]\nresolver = \"2\"\n",
    )?;
    write(
        &root.join("inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    let dependencies = if facade_edge {
        "\n[dependencies]\ninner = { path = \"../inner\" }\n"
    } else {
        ""
    };
    write(
        &root.join("facade/Cargo.toml"),
        &format!(
            "[package]\nname = \"facade\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{dependencies}"
        ),
    )?;
    write(&root.join("inner/src/lib.rs"), INNER_LIB)?;
    write(&root.join("facade/src/lib.rs"), facade_lib)?;
    write(&root.join("facade/tests/t.rs"), facade_test)?;
    let changed_files = diff::parse_unified_diff(
        "diff --git a/inner/src/lib.rs b/inner/src/lib.rs\n--- a/inner/src/lib.rs\n+++ b/inner/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn bump_counter(buf: &mut Vec<u8>, byte: u8) -> usize {\n-    buf.push(byte);\n+    buf.push(byte + 1);\n     buf.len()\n",
    );
    let result = RustAdapter.analyze_diff(
        &AnalysisOptions {
            root,
            base: None,
            diff_file: None,
            mode: AnalysisMode::Ready,
            resolved_subject_identity: None,
            open_rust_index_paths: Default::default(),
            include_unchanged_tests: true,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            perl_producer_failure: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
        },
        &OraclePolicy::default(),
        &changed_files,
    )?;
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("bump_counter"))
        })
        .ok_or_else(|| "no finding on the changed owner".to_string())
}

fn assert_unlimited_no_path(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert_eq!(finding.static_limit_kind, None, "{finding:?}");
    assert!(
        !finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved)
    );
}

#[test]
fn facade_reexport_with_a_test_naming_the_facade_names_the_limit_and_keeps_the_class()
-> Result<(), String> {
    let finding = finding_for("positive", true, "pub use inner::*;\n", FACADE_TEST)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustIntegrationPublicApiPathUnresolved)
    );
    assert!(
        finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved)
    );
    // No promotion: the test is not a related test and no repair gap stands.
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    assert!(finding.canonical_gap.is_none());
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("`go_pushes`") && line.contains("re-exports `inner`")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn braced_item_reexport_is_a_reexport() -> Result<(), String> {
    let finding = finding_for(
        "braced",
        true,
        "pub use inner::{Ext, bump_counter};\n",
        FACADE_TEST,
    )?;
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustIntegrationPublicApiPathUnresolved)
    );
    Ok(())
}

#[test]
fn absent_dependency_edge_keeps_the_definite_result() -> Result<(), String> {
    let finding = finding_for("no-edge", false, "pub use inner::*;\n", FACADE_TEST)?;
    assert_unlimited_no_path(&finding);
    Ok(())
}

#[test]
fn dependent_that_does_not_reexport_the_owner_crate_is_unchanged() -> Result<(), String> {
    for (label, lib) in [
        ("no-reexport", "pub fn unrelated() {}\n"),
        ("private-use", "pub(crate) use inner::*;\n"),
        ("plain-use", "use inner::*;\npub fn unrelated() {}\n"),
    ] {
        let finding = finding_for(label, true, lib, FACADE_TEST)?;
        assert_unlimited_no_path(&finding);
    }
    Ok(())
}

#[test]
fn test_that_never_names_the_facade_crate_is_unchanged() -> Result<(), String> {
    let test = "#[test]\nfn unrelated_test() {\n    assert_eq!(1 + 1, 2);\n}\n";
    let finding = finding_for("unrelated-test", true, "pub use inner::*;\n", test)?;
    assert_unlimited_no_path(&finding);
    Ok(())
}

/// One package: `ready.rs` documents `ready_value` with a runnable example,
/// `reader.rs` holds the changed owner whose expression spells `Poll::Ready`.
/// `reader_doc` is an extra doctest in the owner's own file.
fn doctest_related_tests(label: &str, reader_doc: &str) -> Result<Vec<(String, String)>, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root: PathBuf = std::env::temp_dir().join(format!("ripr-doctest-token-{label}-{stamp}"));
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"dt\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    write(
        &root.join("src/lib.rs"),
        "pub mod ready;\npub mod reader;\n",
    )?;
    write(
        &root.join("src/ready.rs"),
        "/// ```\n/// assert_eq!(dt::ready::ready_value(), 1);\n/// ```\npub fn ready_value() -> u8 {\n    1\n}\n",
    )?;
    write(
        &root.join("src/reader.rs"),
        &format!(
            "{reader_doc}pub fn read_line(x: u8) -> std::task::Poll<u8> {{\n    std::task::Poll::Ready(x + 1)\n}}\n"
        ),
    )?;
    let start = reader_doc.lines().count() + 1;
    let changed_files = diff::parse_unified_diff(&format!(
        "diff --git a/src/reader.rs b/src/reader.rs\n--- a/src/reader.rs\n+++ b/src/reader.rs\n@@ -{start},3 +{start},3 @@\n pub fn read_line(x: u8) -> std::task::Poll<u8> {{\n-    std::task::Poll::Ready(x)\n+    std::task::Poll::Ready(x + 1)\n }}\n"
    ));
    let result = RustAdapter.analyze_diff(
        &AnalysisOptions {
            root,
            base: None,
            diff_file: None,
            mode: AnalysisMode::Ready,
            resolved_subject_identity: None,
            open_rust_index_paths: Default::default(),
            include_unchanged_tests: true,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            perl_producer_failure: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
        },
        &OraclePolicy::default(),
        &changed_files,
    )?;
    Ok(result
        .findings
        .iter()
        .flat_map(|finding| finding.related_tests.iter())
        .map(|test| {
            (
                test.file.to_string_lossy().replace('\\', "/"),
                test.name.clone(),
            )
        })
        .collect())
}

#[test]
fn doctest_of_another_module_sharing_a_name_token_is_not_related() -> Result<(), String> {
    let related = doctest_related_tests("other-module", "")?;
    assert!(
        related.iter().all(|(file, _)| !file.ends_with("ready.rs")),
        "{related:?}"
    );
    Ok(())
}

#[test]
fn doctest_in_the_owners_own_file_stays_related() -> Result<(), String> {
    let doc = "/// ```\n/// assert_eq!(1 + 1, 2);\n/// ```\npub fn doc_anchor() {}\n\n";
    let related = doctest_related_tests("own-file", doc)?;
    assert!(
        related.iter().any(|(file, _)| file.ends_with("reader.rs")),
        "{related:?}"
    );
    Ok(())
}
