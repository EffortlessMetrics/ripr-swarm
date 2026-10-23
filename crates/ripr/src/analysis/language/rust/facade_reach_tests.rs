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

/// A test that spells the owner by name becomes a related test of it (a
/// weakly exposed finding, not `no_static_path`), so the facade route's tests
/// reach the owner only through a glob or a type, never by its name.
const GLOB_TEST: &str = "use facade::*;\n\n#[test]\nfn go_pushes() {\n    let mut v = Vec::new();\n    assert_eq!(v.go(3), 1);\n}\n";
const NORMAL_DEP: &str = "\n[dependencies]\ninner = { path = \"../inner\" }\n";
const DEV_DEP: &str = "\n[dev-dependencies]\ninner = { path = \"../inner\" }\n";

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, text).map_err(|e| e.to_string())
}

/// Where the changed owner `bump_counter` lives inside the `inner` package.
#[derive(Clone, Copy)]
enum Layout {
    /// `pub fn` at the root of `src/lib.rs`.
    Free,
    /// `pub fn` of a `pub struct Counter` at the root.
    Method,
    /// `pub fn` of a private `struct Counter`.
    PrivateType,
    /// `pub fn` in `src/sub.rs`, declared `pub mod sub;`.
    PubMod,
    /// `pub fn` in `src/sub.rs`, declared `mod sub;`.
    PrivateMod,
    /// `pub fn` in the binary target `src/main.rs`.
    Main,
}

/// The owner's file, the lines before the owner, and the owner's four lines
/// (signature, changed body line, tail, close) with the pre-change body line.
struct OwnerText {
    path: &'static str,
    prefix: String,
    signature: String,
    old_body: String,
    new_body: String,
}

fn owner_text(layout: Layout, vis: &str) -> OwnerText {
    let free = |path: &'static str, prefix: &str| OwnerText {
        path,
        prefix: prefix.to_string(),
        signature: format!("{vis}fn bump_counter(buf: &mut Vec<u8>, byte: u8) -> usize {{"),
        old_body: "    buf.push(byte);".to_string(),
        new_body: "    buf.push(byte + 1);".to_string(),
    };
    let method = |prefix: &str| OwnerText {
        path: "inner/src/lib.rs",
        prefix: prefix.to_string(),
        signature: format!("    {vis}fn bump_counter(&mut self, byte: u8) -> usize {{"),
        old_body: "        self.buf.push(byte);".to_string(),
        new_body: "        self.buf.push(byte + 1);".to_string(),
    };
    match layout {
        Layout::Free => free("inner/src/lib.rs", ""),
        Layout::Method => method("pub struct Counter {\n    buf: Vec<u8>,\n}\n\nimpl Counter {\n"),
        Layout::PrivateType => method("struct Counter {\n    buf: Vec<u8>,\n}\n\nimpl Counter {\n"),
        Layout::PubMod | Layout::PrivateMod => free("inner/src/sub.rs", "pub struct Other;\n\n"),
        Layout::Main => free("inner/src/main.rs", ""),
    }
}

/// `inner` holds the owner; the facade package depends on it per
/// `facade_deps`, re-exports it per `facade_lib`, and holds `facade_test`.
struct Setup {
    label: &'static str,
    facade_deps: &'static str,
    facade_lib: &'static str,
    facade_test: &'static str,
    /// Directory and `[package] name` of the facade package.
    facade_dir: &'static str,
    facade_package: &'static str,
    owner_visibility: &'static str,
    layout: Layout,
    mode: AnalysisMode,
    /// An unparseable manifest that makes the dependency graph Limited.
    broken_manifest: bool,
    /// Extra test file in the OWNER's own package.
    owner_package_test: Option<&'static str>,
}

impl Setup {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            facade_deps: NORMAL_DEP,
            facade_lib: "pub use inner::*;\n",
            facade_test: GLOB_TEST,
            facade_dir: "facade",
            facade_package: "facade",
            owner_visibility: "pub ",
            layout: Layout::Free,
            mode: AnalysisMode::Ready,
            broken_manifest: false,
            owner_package_test: None,
        }
    }
}

fn finding_for(setup: &Setup) -> Result<Finding, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root: PathBuf =
        std::env::temp_dir().join(format!("ripr-facade-reach-{}-{stamp}", setup.label));
    let dir = setup.facade_dir;
    write(
        &root.join("Cargo.toml"),
        &format!("[workspace]\nmembers = [\"inner\", \"{dir}\"]\nresolver = \"2\"\n"),
    )?;
    write(
        &root.join("inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    write(
        &root.join(dir).join("Cargo.toml"),
        &format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{}",
            setup.facade_package, setup.facade_deps
        ),
    )?;
    let owner = owner_text(setup.layout, setup.owner_visibility);
    let indent = if matches!(setup.layout, Layout::Method | Layout::PrivateType) {
        "    "
    } else {
        ""
    };
    let body_tail = if indent.is_empty() {
        "    buf.len()\n}\n".to_string()
    } else {
        "        self.buf.len()\n    }\n}\n".to_string()
    };
    let owner_source = format!(
        "{}{}\n{}\n{}",
        owner.prefix, owner.signature, owner.new_body, body_tail
    );
    let start = owner.prefix.lines().count() + 1;
    let extra_lib = match setup.layout {
        Layout::PubMod => "pub mod sub;\n",
        Layout::PrivateMod => "mod sub;\n",
        _ => "",
    };
    let trait_text = "\npub trait Ext {\n    fn go(&mut self, byte: u8) -> usize;\n}\n\nimpl Ext for Vec<u8> {\n    fn go(&mut self, byte: u8) -> usize {\n        self.push(byte);\n        self.len()\n    }\n}\n";
    match owner.path {
        "inner/src/lib.rs" => write(
            &root.join("inner/src/lib.rs"),
            &format!("{owner_source}{trait_text}"),
        )?,
        other => {
            write(&root.join(other), &owner_source)?;
            let lib = if other.ends_with("main.rs") {
                format!("pub fn unrelated() {{}}\n{trait_text}")
            } else {
                format!("{extra_lib}{trait_text}")
            };
            write(&root.join("inner/src/lib.rs"), &lib)?;
        }
    }
    write(&root.join(dir).join("src/lib.rs"), setup.facade_lib)?;
    write(&root.join(dir).join("tests/t.rs"), setup.facade_test)?;
    if let Some(test) = setup.owner_package_test {
        write(&root.join("inner/tests/own.rs"), test)?;
    }
    if setup.broken_manifest {
        write(&root.join("broken/Cargo.toml"), "[package\nname = ")?;
    }
    let third = if indent.is_empty() {
        "     buf.len()"
    } else {
        "         self.buf.len()"
    };
    let changed_files = diff::parse_unified_diff(&format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{start},3 +{start},3 @@\n {sig}\n-{old}\n+{new}\n{third}\n",
        path = owner.path,
        sig = owner.signature,
        old = owner.old_body,
        new = owner.new_body,
    ));
    let result = RustAdapter.analyze_diff(
        &AnalysisOptions {
            root,
            base: None,
            diff_file: None,
            mode: setup.mode,
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

fn assert_limited(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustIntegrationPublicApiPathUnresolved),
        "{finding:?}"
    );
}

#[test]
fn facade_reexport_with_a_test_naming_the_facade_names_the_limit_and_keeps_the_class()
-> Result<(), String> {
    let finding = finding_for(&Setup::new("positive"))?;
    assert_limited(&finding);
    assert!(
        finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved)
    );
    // No promotion: the test is not a related test and no repair gap stands.
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    assert!(finding.canonical_gap.is_none());
    assert!(
        finding.evidence.iter().any(|line| {
            line.contains("`go_pushes`")
                && line.contains("`pub use` names `inner`")
                && line.contains("does not resolve")
        }),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn baseline_without_a_tying_facade_test_keeps_the_repair_gap() -> Result<(), String> {
    // The same workspace with a facade test that never ties to the owner:
    // the definite result and its repair gap stand, so the limitation above
    // is what withholds the gap.
    let mut setup = Setup::new("baseline");
    setup.facade_test = "#[test]\nfn unrelated_test() {\n    assert_eq!(1 + 1, 2);\n}\n";
    let finding = finding_for(&setup)?;
    assert_unlimited_no_path(&finding);
    assert!(finding.canonical_gap.is_some(), "{finding:?}");
    Ok(())
}

#[test]
fn narrowed_index_scopes_still_name_the_limit() -> Result<(), String> {
    for (label, mode) in [("draft", AnalysisMode::Draft), ("fast", AnalysisMode::Fast)] {
        let mut setup = Setup::new(label);
        setup.mode = mode;
        assert_limited(&finding_for(&setup)?);
    }
    Ok(())
}

#[test]
fn reexports_that_cover_the_owner_fire() -> Result<(), String> {
    for (label, lib) in [
        ("list-names-owner", "pub use inner::{Ext, bump_counter};\n"),
        ("single-item", "pub use inner::bump_counter;\n"),
        ("crate-alias", "pub use inner as other;\n"),
        ("crate-self", "pub use inner::{self};\n"),
        ("doc-attr", "#[doc(hidden)]\npub use inner::*;\n"),
    ] {
        let mut setup = Setup::new(label);
        setup.facade_lib = lib;
        assert_limited(&finding_for(&setup)?);
    }
    Ok(())
}

#[test]
fn facade_tests_tie_to_the_owner_by_glob_or_type() -> Result<(), String> {
    let mut braced = Setup::new("braced-glob-test");
    braced.facade_test = "use facade::{Ext, *};\n#[test]\nfn t() {\n    let mut v = Vec::new();\n    assert_eq!(v.go(1), 1);\n}\n";
    assert_limited(&finding_for(&braced)?);

    // Naming the facade is not enough: an unrelated facade item or trait.
    for (label, lib, test) in [
        (
            "unrelated-item",
            "pub use inner::*;\npub const VERSION: u8 = 1;\n",
            "#[test]\nfn version() {\n    assert_eq!(facade::VERSION, 1);\n}\n",
        ),
        (
            "trait-only",
            "pub use inner::*;\n",
            "use facade::Ext;\n#[test]\nfn go() {\n    let mut v = Vec::new();\n    assert_eq!(v.go(1), 1);\n}\n",
        ),
    ] {
        let mut setup = Setup::new(label);
        setup.facade_lib = lib;
        setup.facade_test = test;
        assert_unlimited_no_path(&finding_for(&setup)?);
    }
    Ok(())
}

#[test]
fn inherent_method_owners_are_named_by_their_type_not_the_method() -> Result<(), String> {
    let method_test = "use facade::Counter;\n#[test]\nfn counts() {\n    let _ = Counter { };\n}\n";
    let mut by_type = Setup::new("method-by-type");
    by_type.layout = Layout::Method;
    by_type.facade_lib = "pub use inner::Counter;\n";
    by_type.facade_test = method_test;
    assert_limited(&finding_for(&by_type)?);

    // A re-export of an item that only shares the method's name covers
    // nothing: the method is reached through `Counter`.
    let mut by_method_name = Setup::new("method-by-name");
    by_method_name.layout = Layout::Method;
    by_method_name.facade_lib = "pub use inner::bump_counter;\n";
    by_method_name.facade_test = method_test;
    assert_unlimited_no_path(&finding_for(&by_method_name)?);

    let mut private_type = Setup::new("private-type");
    private_type.layout = Layout::PrivateType;
    private_type.facade_lib = "pub use inner::*;\n";
    private_type.facade_test = GLOB_TEST;
    assert_unlimited_no_path(&finding_for(&private_type)?);
    Ok(())
}

#[test]
fn owner_module_chain_must_be_public() -> Result<(), String> {
    let mut public = Setup::new("pub-mod");
    public.layout = Layout::PubMod;
    public.facade_lib = "pub use inner::sub::bump_counter;\n";
    assert_limited(&finding_for(&public)?);

    let mut private = Setup::new("private-mod");
    private.layout = Layout::PrivateMod;
    private.facade_lib = "pub use inner::*;\n";
    assert_unlimited_no_path(&finding_for(&private)?);

    // A re-export of a sibling item in the owner's module is not the owner.
    let mut sibling = Setup::new("sibling-in-mod");
    sibling.layout = Layout::PubMod;
    sibling.facade_lib = "pub use inner::sub::Other;\n";
    assert_unlimited_no_path(&finding_for(&sibling)?);
    Ok(())
}

#[test]
fn binary_target_owner_never_fires() -> Result<(), String> {
    let mut setup = Setup::new("main-owner");
    setup.layout = Layout::Main;
    assert_unlimited_no_path(&finding_for(&setup)?);
    Ok(())
}

#[test]
fn package_name_comes_from_the_manifest_not_the_directory() -> Result<(), String> {
    // The crate a test names is the package's, whatever its directory is.
    let mut renamed = Setup::new("renamed-package");
    renamed.facade_package = "renamed";
    let finding = finding_for(&renamed)?;
    assert_unlimited_no_path(&finding);

    // Contract: the crate a test must spell is the manifest's package (or
    // `[lib]`) name, not the directory's. A renamed package whose test
    // spells that name fires; the same layout whose test spells only the
    // directory name (`facade::`) does not, which is what separates renamed
    // crate support from a directory-name regression.
    let mut renamed_spelled = Setup::new("renamed-package-spelled");
    renamed_spelled.facade_package = "renamed";
    renamed_spelled.facade_test = "use renamed::*;\n\n#[test]\nfn go_pushes() {\n    let mut v = Vec::new();\n    assert_eq!(v.go(3), 1);\n}\n";
    assert_limited(&finding_for(&renamed_spelled)?);

    let mut renamed_dir = Setup::new("renamed-dir");
    renamed_dir.facade_dir = "crates-facade-dir";
    renamed_dir.facade_package = "facade";
    assert_limited(&finding_for(&renamed_dir)?);

    // A directory named like a std crate whose package is not.
    let mut std_named = Setup::new("std-named");
    std_named.facade_dir = "core";
    std_named.facade_package = "other-name";
    std_named.facade_test = "#[test]\nfn swaps() {\n    let (mut a, mut b) = (1, 2);\n    core::mem::swap(&mut a, &mut b);\n    assert_eq!(a, 2);\n}\n";
    assert_unlimited_no_path(&finding_for(&std_named)?);
    Ok(())
}

#[test]
fn absent_or_dev_only_dependency_edge_keeps_the_definite_result() -> Result<(), String> {
    let mut none = Setup::new("no-edge");
    none.facade_deps = "";
    assert_unlimited_no_path(&finding_for(&none)?);
    let mut dev = Setup::new("dev-edge");
    dev.facade_deps = DEV_DEP;
    assert_unlimited_no_path(&finding_for(&dev)?);
    Ok(())
}

#[test]
fn reexports_that_do_not_provably_cover_the_owner_are_unchanged() -> Result<(), String> {
    for (label, lib) in [
        ("no-reexport", "pub fn unrelated() {}\n"),
        ("private-use", "pub(crate) use inner::*;\n"),
        ("plain-use", "use inner::*;\npub fn unrelated() {}\n"),
        ("cfg-gated", "#[cfg(feature = \"x\")]\npub use inner::*;\n"),
        (
            "nested-module",
            "pub mod nested {\n    pub use inner::*;\n}\n",
        ),
        ("fn-local", "pub fn f() {\n    pub use inner::*;\n}\n"),
        (
            "macro-body",
            "macro_rules! m {\n    () => {\n        pub use inner::*;\n    };\n}\n",
        ),
        ("braced-list-without-owner", "pub use inner::{Ext};\n"),
        ("other-item", "pub use inner::Ext;\n"),
    ] {
        let mut setup = Setup::new(label);
        setup.facade_lib = lib;
        assert_unlimited_no_path(&finding_for(&setup)?);
    }
    Ok(())
}

#[test]
fn private_or_restricted_owner_never_fires() -> Result<(), String> {
    for (label, vis) in [("private-owner", ""), ("crate-owner", "pub(crate) ")] {
        let mut setup = Setup::new(label);
        setup.owner_visibility = vis;
        assert_unlimited_no_path(&finding_for(&setup)?);
    }
    Ok(())
}

#[test]
fn limited_graph_keeps_the_definite_result() -> Result<(), String> {
    let mut setup = Setup::new("limited-graph");
    setup.broken_manifest = true;
    assert_unlimited_no_path(&finding_for(&setup)?);
    Ok(())
}

#[test]
fn test_that_never_names_the_facade_crate_is_unchanged() -> Result<(), String> {
    let mut setup = Setup::new("unrelated-test");
    setup.facade_test = "#[test]\nfn unrelated_test() {\n    assert_eq!(1 + 1, 2);\n}\n";
    assert_unlimited_no_path(&finding_for(&setup)?);
    Ok(())
}

#[test]
fn test_in_the_owners_own_package_naming_the_facade_does_not_count() -> Result<(), String> {
    let mut setup = Setup::new("owner-package-test");
    setup.facade_test = "#[test]\nfn unrelated_test() {\n    assert_eq!(1 + 1, 2);\n}\n";
    setup.owner_package_test = Some(GLOB_TEST);
    assert_unlimited_no_path(&finding_for(&setup)?);
    Ok(())
}

/// One package: `ready.rs` documents `ready_value` with a runnable example,
/// `reader.rs` holds the changed owner whose expression spells `Poll::Ready`.
/// `reader_doc` is an extra doctest in the owner's own file.
fn doctest_related_tests(
    label: &str,
    reader_doc: &str,
    ready_assertion: &str,
) -> Result<Vec<(String, String)>, String> {
    doctest_related_tests_for(
        label,
        reader_doc,
        ready_assertion,
        &format!(
            "{reader_doc}pub fn read_line(x: u8) -> std::task::Poll<u8> {{\n    std::task::Poll::Ready(x + 1)\n}}\n"
        ),
        (
            "pub fn read_line(x: u8) -> std::task::Poll<u8> {",
            "    std::task::Poll::Ready(x)",
            "    std::task::Poll::Ready(x + 1)",
        ),
        reader_doc.lines().count() + 1,
    )
}

/// `reader_source` is `reader.rs`; the hunk `(signature, old, new)` changes
/// the owner whose line starts at `start`.
fn doctest_related_tests_for(
    label: &str,
    reader_doc: &str,
    ready_assertion: &str,
    reader_source: &str,
    hunk: (&str, &str, &str),
    start: usize,
) -> Result<Vec<(String, String)>, String> {
    let _ = reader_doc;
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
        &format!(
            "/// ```\n/// {ready_assertion}\n/// ```\npub fn ready_value() -> u8 {{\n    1\n}}\n"
        ),
    )?;
    write(&root.join("src/reader.rs"), reader_source)?;
    let (signature, old, new) = hunk;
    let changed_files = diff::parse_unified_diff(&format!(
        "diff --git a/src/reader.rs b/src/reader.rs\n--- a/src/reader.rs\n+++ b/src/reader.rs\n@@ -{start},3 +{start},3 @@\n {signature}\n-{old}\n+{new}\n }}\n"
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
    let related = doctest_related_tests(
        "other-module",
        "",
        "assert_eq!(dt::ready::ready_value(), 1);",
    )?;
    assert!(
        related.iter().all(|(file, _)| !file.ends_with("ready.rs")),
        "{related:?}"
    );
    Ok(())
}

#[test]
fn doctest_in_the_owners_own_file_stays_related() -> Result<(), String> {
    let doc = "/// ```\n/// assert_eq!(1 + 1, 2);\n/// ```\npub fn doc_anchor() {}\n\n";
    let related =
        doctest_related_tests("own-file", doc, "assert_eq!(dt::ready::ready_value(), 1);")?;
    assert!(
        related.iter().any(|(file, _)| file.ends_with("reader.rs")),
        "{related:?}"
    );
    Ok(())
}

/// Control for the negative above: the same `ready.rs` doctest is indexed and
/// relatable, so the negative cannot pass only because doctests are absent.
/// Calling the owner from another file's doctest keeps it related.
#[test]
fn doctest_of_another_module_that_calls_the_owner_stays_related() -> Result<(), String> {
    let related = doctest_related_tests(
        "calls-owner",
        "",
        "assert!(matches!(dt::reader::read_line(1), std::task::Poll::Ready(2)));",
    )?;
    assert!(
        related.iter().any(|(file, _)| file.ends_with("ready.rs")),
        "{related:?}"
    );
    Ok(())
}
