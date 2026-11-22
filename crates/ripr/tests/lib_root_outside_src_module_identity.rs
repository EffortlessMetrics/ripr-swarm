//! #14495 and #15909: a library root outside `src/` names its own modules.
//!
//! A manifest `[lib] path = "source/entry.rs"` puts the crate root and its
//! module files outside `src/`, and so does a symlinked target path (`src ->
//! real`), where Git reports the change at the link target while Cargo names
//! the target by the link. Either way `source/pricing.rs` (or
//! `real/pricing.rs`) is the library module `pricing`, so a test calling
//! `my_pkg::pricing::discount(100)` gives the changed `amount >= 100` its
//! boundary input, as the same layout under `src/` does. The changed owner is
//! `discount` (`amount > 100` to `amount >= 100`). Each case asserts its
//! generated fixture (the finding sits on the changed line of the intended
//! file) before any claim about the reading.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const PACKAGE: &str = "[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const OUTSIDE_ROOT: &str = "\n[lib]\npath = \"source/entry.rs\"\n";
const OWNER: &str = "pub fn discount(amount: u32) -> u32 {\n    if amount >= 100 { amount - 10 } else { amount }\n}\n";

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// One entry of a generated package: a file with its text, or a symlink
/// (`link`, pointing at `target`, relative to the link's directory).
enum Entry {
    File(&'static str, String),
    #[cfg_attr(not(unix), allow(dead_code))]
    Link(&'static str, &'static str),
}

fn file(path: &'static str, text: impl Into<String>) -> Entry {
    Entry::File(path, text.into())
}

fn integration_test(call: &str) -> String {
    format!(
        "#[test]\nfn discounts() {{\n    assert_eq!({call}(100), 100);\n    assert_eq!({call}(101), 91);\n}}\n"
    )
}

/// The finding for the changed `amount >= 100` on `line` of `changed` in a
/// package made of `entries`.
fn finding_in(entries: &[Entry], (changed, line): (&str, usize)) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-lib-root-outside-src-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )),
    };
    let root = &repo.root;
    let parent_of = |path: &Path| -> Result<PathBuf, String> {
        let parent = path
            .parent()
            .ok_or_else(|| format!("no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent).map_err(|error| format!("dir {parent:?}: {error}"))?;
        Ok(parent.to_path_buf())
    };
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        parent_of(&path)?;
        std::fs::write(&path, text).map_err(|error| format!("write {path:?}: {error}"))
    };
    for entry in entries {
        match entry {
            Entry::File(path, text) => write(path, text)?,
            Entry::Link(link, target) => {
                let link = root.join(link);
                parent_of(&link)?;
                symlink(Path::new(target), &link)?;
            }
        }
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

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|error| format!("link {link:?}: {error}"))
}

#[cfg(not(unix))]
fn symlink(_target: &Path, link: &Path) -> Result<(), String> {
    Err(format!("no symlink support for {link:?}"))
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

fn assert_not_exposed(label: &str, found: &Finding) {
    assert_ne!(
        found.class,
        ExposureClass::Exposed,
        "{label}: {:?}",
        found.evidence
    );
}

/// A package whose `[lib] path` is `source/entry.rs`, declaring `modules`,
/// with `OWNER` in each of `owner_files` and an integration test of `call`.
fn outside_root_package(modules: &str, owner_files: &[&'static str], call: &str) -> Vec<Entry> {
    let mut entries = vec![
        file("Cargo.toml", format!("{PACKAGE}{OUTSIDE_ROOT}")),
        file("source/entry.rs", modules),
        file("tests/d.rs", integration_test(call)),
    ];
    entries.extend(owner_files.iter().map(|path| file(path, OWNER)));
    entries
}

#[test]
fn a_module_below_a_root_outside_src_is_named_from_that_root() -> Result<(), String> {
    // Control: the same package with the default `src/lib.rs`.
    let control = finding_in(
        &[
            file("Cargo.toml", PACKAGE),
            file("src/lib.rs", "pub mod pricing;\n"),
            file("src/pricing.rs", OWNER),
            file("tests/d.rs", integration_test("my_pkg::pricing::discount")),
        ],
        ("src/pricing.rs", 2),
    )?;
    assert_exposed("src/lib.rs control", &control);

    let found = finding_in(
        &outside_root_package(
            "pub mod pricing;\n",
            &["source/pricing.rs"],
            "my_pkg::pricing::discount",
        ),
        ("source/pricing.rs", 2),
    )?;
    assert_exposed("module below source/entry.rs", &found);

    // The root file itself is the crate root: `my_pkg::discount`.
    let root = finding_in(
        &[
            file("Cargo.toml", format!("{PACKAGE}{OUTSIDE_ROOT}")),
            file("source/entry.rs", OWNER),
            file("tests/d.rs", integration_test("my_pkg::discount")),
        ],
        ("source/entry.rs", 2),
    )?;
    assert_exposed("owner in source/entry.rs", &root);
    Ok(())
}

#[test]
fn a_dependent_crate_names_a_member_whose_root_is_outside_src() -> Result<(), String> {
    // The issue's shape: member `core_lib` sets `[lib] path =
    // "source/entry.rs"`, and the dependent `app`'s integration test calls
    // `core_lib::pricing::discount`.
    let workspace = |core_manifest: &str, root: &'static str, owner: &'static str| {
        vec![
            file(
                "Cargo.toml",
                "[workspace]\nmembers = [\"core_lib\", \"app\"]\nresolver = \"2\"\n",
            ),
            file("core_lib/Cargo.toml", core_manifest.to_string()),
            file(root, "pub mod pricing;\n"),
            file(owner, OWNER),
            file(
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncore_lib = { path = \"../core_lib\" }\n",
            ),
            file("app/src/lib.rs", "pub fn run() {}\n"),
            file(
                "app/tests/d.rs",
                integration_test("core_lib::pricing::discount"),
            ),
        ]
    };
    let core = "[package]\nname = \"core_lib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
    let control = finding_in(
        &workspace(core, "core_lib/src/lib.rs", "core_lib/src/pricing.rs"),
        ("core_lib/src/pricing.rs", 2),
    )?;
    assert_exposed("src/lib.rs member control", &control);
    let found = finding_in(
        &workspace(
            &format!("{core}{OUTSIDE_ROOT}"),
            "core_lib/source/entry.rs",
            "core_lib/source/pricing.rs",
        ),
        ("core_lib/source/pricing.rs", 2),
    )?;
    assert_exposed("member root outside src", &found);

    // The root package's own `src/pricing.rs` holds a same-named `discount`
    // at the same module path. It is another package's function, so the
    // member's `source/` files must not read as the root package's.
    let mut beside_root_package = workspace(
        &format!("{core}{OUTSIDE_ROOT}"),
        "core_lib/source/entry.rs",
        "core_lib/source/pricing.rs",
    );
    beside_root_package[0] = file(
        "Cargo.toml",
        "[package]\nname = \"root-pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\nmembers = [\"core_lib\", \"app\"]\nresolver = \"2\"\n",
    );
    beside_root_package.push(file("src/lib.rs", "pub mod pricing;\n"));
    beside_root_package.push(file("src/pricing.rs", OWNER));
    let found = finding_in(&beside_root_package, ("core_lib/source/pricing.rs", 2))?;
    assert_exposed("member root beside a root package", &found);
    // And the root package's own owner keeps its credit: the member's
    // same-path `discount` is no rival in the root package.
    beside_root_package.push(file(
        "tests/root.rs",
        integration_test("root_pkg::pricing::discount"),
    ));
    let found = finding_in(&beside_root_package, ("src/pricing.rs", 2))?;
    assert_exposed("root package owner beside the member", &found);
    Ok(())
}

#[test]
fn a_directory_shaped_or_rival_path_never_names_an_outside_root_owner() -> Result<(), String> {
    // A same-named rival module puts every call through call identity.
    let modules = "pub mod pricing;\npub mod legacy;\n";
    let owners = ["source/pricing.rs", "source/legacy.rs"];
    let found = finding_in(
        &outside_root_package(modules, &owners, "my_pkg::pricing::discount"),
        ("source/pricing.rs", 2),
    )?;
    assert_exposed("owner beside a rival", &found);
    // `my_pkg::source::pricing` is the directory shape, which does not
    // compile: the crate root is `source/entry.rs`, so there is no `source`.
    for (label, call) in [
        ("directory shape", "my_pkg::source::pricing::discount"),
        ("rival module", "my_pkg::legacy::discount"),
    ] {
        let found = finding_in(
            &outside_root_package(modules, &owners, call),
            ("source/pricing.rs", 2),
        )?;
        assert_not_exposed(label, &found);
    }
    Ok(())
}

#[test]
fn a_binary_beside_a_root_outside_src_keeps_the_library_modules_placed() -> Result<(), String> {
    // `src/main.rs` composes only `src/` modules, which never include
    // `source/pricing.rs`, so the library reading stays established. A
    // same-named rival puts the calls through call identity.
    let mut entries = outside_root_package(
        "pub mod pricing;\npub mod legacy;\n",
        &["source/pricing.rs", "source/legacy.rs"],
        "my_pkg::pricing::discount",
    );
    entries.push(file("src/main.rs", "fn main() {}\n"));
    let found = finding_in(&entries, ("source/pricing.rs", 2))?;
    assert_exposed("library module beside a binary", &found);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_symlinked_src_directory_names_its_modules_from_the_link() -> Result<(), String> {
    // #15909: `src -> real`. Git reports `real/pricing.rs`; Cargo's root is
    // `src/lib.rs`, which is `real/lib.rs`, so the module is `pricing`.
    let package = |call: &str| {
        vec![
            file("Cargo.toml", PACKAGE),
            file("real/lib.rs", "pub mod pricing;\npub mod legacy;\n"),
            file("real/pricing.rs", OWNER),
            file("real/legacy.rs", OWNER),
            Entry::Link("src", "real"),
            file("tests/d.rs", integration_test(call)),
        ]
    };
    let found = finding_in(
        &package("my_pkg::pricing::discount"),
        ("real/pricing.rs", 2),
    )?;
    assert_exposed("module below a symlinked src", &found);
    for (label, call) in [
        ("link target shape", "my_pkg::real::pricing::discount"),
        ("rival module", "my_pkg::legacy::discount"),
    ] {
        let found = finding_in(&package(call), ("real/pricing.rs", 2))?;
        assert_not_exposed(label, &found);
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_symlinked_root_file_is_the_crate_root_but_lends_no_directory() -> Result<(), String> {
    // #15909: `src/lib.rs -> ../real/lib.rs`. The linked file is the crate
    // root, so an owner in it is `my_pkg::discount`.
    let found = finding_in(
        &[
            file("Cargo.toml", PACKAGE),
            file("real/lib.rs", OWNER),
            Entry::Link("src/lib.rs", "../real/lib.rs"),
            file("tests/d.rs", integration_test("my_pkg::discount")),
        ],
        ("real/lib.rs", 2),
    )?;
    assert_exposed("owner in a symlinked root file", &found);

    // rustc loads the root's `mod pricing;` beside the link (`src/`), not
    // beside its target, so `real/pricing.rs` is no module of this crate and
    // never names the owner. Today no crate composes it, so it has no
    // finding at all; either answer keeps the owner uncredited.
    let stray = finding_in(
        &[
            file("Cargo.toml", PACKAGE),
            file("real/lib.rs", "pub mod pricing;\n"),
            file("real/pricing.rs", OWNER),
            Entry::Link("src/lib.rs", "../real/lib.rs"),
            file("tests/d.rs", integration_test("my_pkg::pricing::discount")),
        ],
        ("real/pricing.rs", 2),
    );
    match stray {
        Ok(found) => assert_not_exposed("file beside the link target", &found),
        Err(error) => assert!(error.starts_with("no finding for"), "{error}"),
    }
    Ok(())
}
