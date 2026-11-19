//! #14495: a manifest `[lib] path` names the library's crate root, so module
//! paths are read from that root, not from the file's place under `src/`.
//!
//! The package sets `[lib] path = "src/core/engine.rs"`; `engine.rs` declares
//! `pub mod pricing;`, so `src/core/pricing.rs` is the module `pricing`
//! (`my_pkg::pricing::discount`), not `core::pricing`. The changed owner is
//! `discount` (`amount > 100` to `amount >= 100`). Each case asserts its
//! generated fixture (the finding sits on the changed line of the intended
//! file) before any claim about the reading.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str = "[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/core/engine.rs\"\n";
const OWNER: &str = "pub fn discount(amount: u32) -> u32 {\n    if amount >= 100 { amount - 10 } else { amount }\n}\n";

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The finding for the changed `amount >= 100` on `line` of `changed`, with
/// `root_source` as the library root, `OWNER` in every file of
/// `owner_files`, and a test calling `call(100)` and `call(101)`.
fn finding(
    root_source: &str,
    owner_files: &[&str],
    (changed, line): (&str, usize),
    call: &str,
) -> Result<Finding, String> {
    let mut files = vec![
        ("Cargo.toml", MANIFEST.to_string()),
        ("src/core/engine.rs", root_source.to_string()),
        (
            "tests/d.rs",
            format!(
                "#[test]\nfn discounts() {{\n    assert_eq!({call}(100), 100);\n    assert_eq!({call}(101), 91);\n}}\n"
            ),
        ),
    ];
    files.extend(owner_files.iter().map(|file| (*file, OWNER.to_string())));
    finding_in(&files, (changed, line))
}

/// The finding for the changed `amount >= 100` on `line` of `changed` in a
/// package made of `files`.
fn finding_in(files: &[(&str, String)], (changed, line): (&str, usize)) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-custom-lib-path-module-identity-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )),
    };
    let root = &repo.root;
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        let parent = path
            .parent()
            .ok_or_else(|| format!("no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent).map_err(|error| format!("dir {path:?}: {error}"))?;
        std::fs::write(&path, text).map_err(|error| format!("write {path:?}: {error}"))
    };
    for (path, text) in files {
        write(path, text)?;
    }
    let start = line - 1;
    let diff = format!(
        "diff --git a/{changed} b/{changed}\nindex 0000000..1111111 100644\n--- a/{changed}\n+++ b/{changed}\n@@ -{start},3 +{start},3 @@\n pub fn discount(amount: u32) -> u32 {{\n-    if amount > 100 {{ amount - 10 }} else {{ amount }}\n+    if amount >= 100 {{ amount - 10 }} else {{ amount }}\n }}\n"
    );
    write("diff.patch", &diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Draft,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check: {error}"))?;
    let found = output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.location.line == line
                && finding.probe.expression.contains("amount >= 100")
        })
        .ok_or_else(|| format!("no finding for the changed `amount >= 100` in {changed}"))?;
    let file = normalized(&found.probe.location.file);
    if !(file == changed || file.ends_with(&format!("/{changed}"))) {
        return Err(format!("finding sits in {file}, not {changed}"));
    }
    Ok(found)
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

fn assert_exposed(label: &str, found: &Finding) {
    assert_eq!(
        found.class,
        ExposureClass::Exposed,
        "{label}: {:?}",
        found.evidence
    );
}

#[test]
fn an_owner_in_the_declared_root_file_is_named_at_the_crate_root() -> Result<(), String> {
    // `engine.rs` itself holds the owner: `my_pkg::discount` names it.
    let found = finding(
        &format!("pub mod pricing;\n\n{OWNER}"),
        &["src/core/pricing.rs"],
        ("src/core/engine.rs", 4),
        "my_pkg::discount",
    )?;
    assert_exposed("root file", &found);
    Ok(())
}

#[test]
fn a_module_below_the_declared_root_is_named_from_that_root() -> Result<(), String> {
    let found = finding(
        "pub mod pricing;\n",
        &["src/core/pricing.rs"],
        ("src/core/pricing.rs", 2),
        "my_pkg::pricing::discount",
    )?;
    assert_exposed("module", &found);

    // With a same-named rival module, the spelled path still settles on it.
    let rival = finding(
        "pub mod pricing;\npub mod legacy;\n",
        &["src/core/pricing.rs", "src/core/legacy.rs"],
        ("src/core/pricing.rs", 2),
        "my_pkg::pricing::discount",
    )?;
    assert_exposed("module with rival", &rival);
    Ok(())
}

#[test]
fn a_path_shaped_or_rival_module_path_never_names_the_owner() -> Result<(), String> {
    // `my_pkg::core::pricing` is the directory shape, which does not compile
    // here: the crate root is `src/core/engine.rs`, so there is no `core`.
    let shaped = finding(
        "pub mod pricing;\n",
        &["src/core/pricing.rs"],
        ("src/core/pricing.rs", 2),
        "my_pkg::core::pricing::discount",
    )?;
    assert_ne!(
        shaped.class,
        ExposureClass::Exposed,
        "{:?}",
        shaped.evidence
    );

    // A test of the rival module's `discount` is not the owner's test.
    let rival = finding(
        "pub mod pricing;\npub mod legacy;\n",
        &["src/core/pricing.rs", "src/core/legacy.rs"],
        ("src/core/pricing.rs", 2),
        "my_pkg::legacy::discount",
    )?;
    assert_ne!(rival.class, ExposureClass::Exposed, "{:?}", rival.evidence);
    Ok(())
}

const UNIT_TEST: &str = "\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn discounts() {\n        assert_eq!(discount(100), 100);\n        assert_eq!(discount(101), 91);\n    }\n}\n";

#[test]
fn a_unit_test_beside_the_owner_reads_the_owner_module() -> Result<(), String> {
    // A same-named rival module puts the call through the rival identity;
    // the unit test's own module and the owner's are read alike.
    let found = finding_in(
        &[
            ("Cargo.toml", MANIFEST.to_string()),
            (
                "src/core/engine.rs",
                "pub mod pricing;\npub mod legacy;\n".to_string(),
            ),
            ("src/core/pricing.rs", format!("{OWNER}{UNIT_TEST}")),
            ("src/core/legacy.rs", OWNER.to_string()),
        ],
        ("src/core/pricing.rs", 2),
    )?;
    assert_exposed("unit test", &found);
    Ok(())
}

#[test]
fn a_unit_test_of_a_rival_module_never_names_the_owner() -> Result<(), String> {
    // The owner is `core::pricing` (`src/core/core/pricing.rs`); the rival
    // `pricing` (`src/core/pricing.rs`) holds the only test, whose bare call
    // is the rival's. Its directory shape (`core::pricing`) would collide.
    let found = finding_in(
        &[
            ("Cargo.toml", MANIFEST.to_string()),
            (
                "src/core/engine.rs",
                "pub mod core;\npub mod pricing;\n".to_string(),
            ),
            ("src/core/core.rs", "pub mod pricing;\n".to_string()),
            ("src/core/core/pricing.rs", OWNER.to_string()),
            ("src/core/pricing.rs", format!("{OWNER}{UNIT_TEST}")),
        ],
        ("src/core/core/pricing.rs", 2),
    )?;
    assert_ne!(found.class, ExposureClass::Exposed, "{:?}", found.evidence);
    Ok(())
}
