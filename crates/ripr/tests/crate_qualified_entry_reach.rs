//! An integration test that calls a tested wrapper through the package's own
//! library name (`qual_reach::order_discount(..)`, no `use`) is related to the
//! wrapper's private helper like the imported spelling is (#8999). Every
//! other qualifier stays unrelated (fail closed). Each case writes a small
//! package, changes the helper's boundary line, and reads the predicate
//! finding's related tests through `check_workspace`. Every refusal is paired
//! with a baseline (the same package with the `use` spelling, or the
//! qualified positive) so it cannot pass for an unrelated reason.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-crate-qualified-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation, then the guard owns the root before anything
        // else is written, so a failed setup step still cleans up and a
        // colliding directory is never deleted.
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

fn manifest(package: &str, lib_name: Option<&str>) -> String {
    let lib = lib_name
        .map(|name| format!("[lib]\nname = \"{name}\"\n"))
        .unwrap_or_default();
    format!(
        "[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n{lib}"
    )
}

/// A diff changing the helper's `qty >= 10` body line in `source`: the hunk
/// starts at the `fn is_bulk` line actually present in the file, so its line
/// numbers match the file at every nesting depth.
fn diff(path: &str, source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.trim_start().starts_with("fn is_bulk"))
        .unwrap_or(0);
    let signature = lines.get(at).copied().unwrap_or_default();
    let body_indent = " ".repeat(signature.len() - signature.trim_start().len() + 4);
    let close_indent = " ".repeat(signature.len() - signature.trim_start().len());
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{start},3 +{start},3 @@\n {signature}\n-{body_indent}qty >= 10\n+{body_indent}10 <= qty\n {close_indent}}}\n",
        start = at + 1,
    )
}

const LIB: &str = "fn is_bulk(qty: u32) -> bool {\n    10 <= qty\n}\n\npub fn order_discount(qty: u32) -> u32 {\n    if is_bulk(qty) { 5 } else { 0 }\n}\n";

/// The helper and its wrapper live in an inline module, so the wrapper is not
/// a crate-root item.
const NESTED_LIB: &str = "mod inner {\n    fn is_bulk(qty: u32) -> bool {\n        10 <= qty\n    }\n\n    pub fn order_discount(qty: u32) -> u32 {\n        if is_bulk(qty) { 5 } else { 0 }\n    }\n}\npub use inner::order_discount;\n";

fn test_file(body: &str) -> String {
    format!("#[test]\nfn discount_boundary() {{\n{body}\n}}\n")
}

fn qualified(crate_name: &str) -> String {
    format!(
        "    assert_eq!({crate_name}::order_discount(10), 5);\n    assert_eq!({crate_name}::order_discount(9), 0);"
    )
}

const BARE: &str = "    assert_eq!(order_discount(10), 5);\n    assert_eq!(order_discount(9), 0);";

/// A scratch package: manifest text, its diff path, and every file.
struct Package<'a> {
    manifest: String,
    diff_path: &'a str,
    files: Vec<(&'a str, String)>,
}

impl<'a> Package<'a> {
    /// The root package with `src/lib.rs` = `lib` and `tests/api.rs` = `tests`.
    fn root(lib: &str, tests: &str) -> Package<'a> {
        Package {
            manifest: manifest("qual_reach", None),
            diff_path: "src/lib.rs",
            files: vec![
                ("src/lib.rs", lib.to_string()),
                ("tests/api.rs", tests.to_string()),
            ],
        }
    }

    /// The relation reason of `discount_boundary` on the predicate finding,
    /// or `None` when the test is not a related test of it.
    fn relation(&self) -> Result<Option<String>, String> {
        Ok(self.outcome()?.0)
    }

    /// The rendered classification of the predicate finding.
    fn classification(&self) -> Result<String, String> {
        Ok(self.outcome()?.1)
    }

    fn outcome(&self) -> Result<(Option<String>, String), String> {
        let scratch = Scratch::create()?;
        scratch.write("Cargo.toml", &self.manifest)?;
        for (path, content) in &self.files {
            scratch.write(path, content)?;
        }
        let diff_file = scratch.0.join("change.diff");
        let source = self
            .files
            .iter()
            .find(|(path, _)| *path == self.diff_path)
            .map(|(_, content)| content.as_str())
            .unwrap_or_default();
        scratch.write("change.diff", &diff(self.diff_path, source))?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(diff_file),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
        let finding = json["findings"]
            .as_array()
            .ok_or("missing findings")?
            .iter()
            .find(|finding| finding["probe"]["family"] == "predicate")
            .cloned()
            .ok_or_else(|| format!("no predicate finding: {json}"))?;
        let classification = finding["classification"]
            .as_str()
            .ok_or("missing classification")?
            .to_string();
        let relation = finding["related_tests"]
            .as_array()
            .and_then(|related| {
                related
                    .iter()
                    // The test as the analyzer names it, `r#` kept for a raw name.
                    .find(|test| {
                        test["name"] == "discount_boundary" || test["name"] == "r#discount_boundary"
                    })
            })
            .and_then(|test| test["relation_reason"].as_str().map(str::to_string));
        Ok((relation, classification))
    }
}

const LINKED: Option<&str> = Some("helper_owner_call");

fn root_relation(lib: &str, tests: &str) -> Result<Option<String>, String> {
    Package::root(lib, tests).relation()
}

#[test]
fn imported_spelling_is_the_linked_baseline() -> Result<(), String> {
    let tests = format!("use qual_reach::order_discount;\n{}", test_file(BARE));
    assert_eq!(root_relation(LIB, &tests)?.as_deref(), LINKED);
    Ok(())
}

#[test]
/// Companion to the baseline: with the `calls_helper_entry` disjunct reverted
/// (production change removed, this file kept) this test fails while the
/// refusals keep passing; that revert run was recorded on #9139.
fn own_crate_qualified_entry_call_links() -> Result<(), String> {
    let tests = test_file(&qualified("qual_reach"));
    assert_eq!(root_relation(LIB, &tests)?.as_deref(), LINKED);
    Ok(())
}

#[test]
fn another_crate_qualifier_does_not_link() -> Result<(), String> {
    let tests = test_file(&qualified("other_crate"));
    assert_eq!(root_relation(LIB, &tests)?, None);
    Ok(())
}

#[test]
fn a_longer_path_does_not_link() -> Result<(), String> {
    let tests = test_file(&qualified("qual_reach::util"));
    assert_eq!(root_relation(LIB, &tests)?, None);
    Ok(())
}

/// Each of these lines, added to the otherwise linking qualified test, makes
/// the crate name possibly mean something else: a module, a rename, an
/// import, a glob, a local binding.
#[test]
fn a_rebindable_crate_name_does_not_link() -> Result<(), String> {
    let rebinds = [
        "mod qual_reach {\n    pub fn order_discount(_: u32) -> u32 { 5 }\n}\n",
        "use other_crate as qual_reach;\n",
        "use some_dep::qual_reach;\n",
        "use some_dep::*;\n",
        "use qual_reach_inner::order_discount as qual_reach;\n",
        "fn qual_reach() {}\n",
        "const qual_reach: u32 = 1;\n",
        "mod r#qual_reach {\n    pub fn order_discount(_: u32) -> u32 { 5 }\n}\n",
        "fn r#qual_reach() {}\n",
        "use dep::r#qual_reach;\n",
        "use x as r#qual_reach;\n",
        "extern crate x as r#qual_reach;\n",
        "extern crate x as qual_reach;\n",
    ];
    for rebind in rebinds {
        let tests = format!("{rebind}{}", test_file(&qualified("qual_reach")));
        assert_eq!(root_relation(LIB, &tests)?, None, "{rebind}");
    }
    // The same crate-name line inside the test body.
    let body = format!(
        "    let qual_reach = other_crate::make();\n{}",
        qualified("qual_reach")
    );
    assert_eq!(root_relation(LIB, &test_file(&body))?, None);
    for line in [
        "    use some_dep::qual_reach;",
        "    use inner::order_discount as qual_reach;",
        "    fn qual_reach() {}",
        "    let helper = |qual_reach: u32| qual_reach;",
    ] {
        let body = format!("{line}\n{}", qualified("qual_reach"));
        assert_eq!(root_relation(LIB, &test_file(&body))?, None, "{line}");
    }
    // Control: an unrelated import keeps the link.
    let tests = format!(
        "use std::fmt::Debug;\n{}",
        test_file(&qualified("qual_reach"))
    );
    assert_eq!(root_relation(LIB, &tests)?.as_deref(), LINKED);
    Ok(())
}

#[test]
fn an_entry_outside_the_crate_root_does_not_link() -> Result<(), String> {
    let baseline = format!("use qual_reach::order_discount;\n{}", test_file(BARE));
    assert_eq!(
        root_relation(NESTED_LIB, &baseline)?.as_deref(),
        LINKED,
        "the use spelling must link, or this negative proves nothing"
    );
    let tests = test_file(&qualified("qual_reach"));
    assert_eq!(root_relation(NESTED_LIB, &tests)?, None);
    Ok(())
}

#[test]
fn a_unit_test_inside_src_does_not_link() -> Result<(), String> {
    let unit = |call: &str| {
        format!(
            "{LIB}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{}\n}}\n",
            test_file(call)
        )
    };
    let baseline = Package {
        files: vec![("src/lib.rs", unit(BARE))],
        ..Package::root("", "")
    };
    assert_eq!(baseline.relation()?.as_deref(), LINKED);
    let qualified_unit = Package {
        files: vec![("src/lib.rs", unit(&qualified("qual_reach")))],
        ..Package::root("", "")
    };
    // File proximity may still relate a same-file test; the qualified spelling
    // earns no helper-chain link there.
    assert_eq!(
        qualified_unit.relation()?.as_deref(),
        Some("same_test_file")
    );
    Ok(())
}

#[test]
fn a_workspace_member_does_not_link() -> Result<(), String> {
    let member = |tests: String| Package {
        manifest: manifest("qual_reach", None),
        diff_path: "crates/inner/src/lib.rs",
        files: vec![
            ("src/lib.rs", "pub fn root() {}\n".to_string()),
            ("crates/inner/Cargo.toml", manifest("inner", None)),
            ("crates/inner/src/lib.rs", LIB.to_string()),
            ("crates/inner/tests/api.rs", tests),
        ],
    };
    // No linking baseline exists for a member: its own `use` spelling does
    // not link either, so this pins the current refusal of the qualified
    // spelling rather than proving a guard in isolation. The root-package
    // check (`target_root` must be the root `src/lib.rs`, and neither path
    // may carry a package prefix) is what refuses it.
    // Control: the member's files are indexed and its tests are related (a
    // unit test beside the member's library is related by file proximity; the
    // helper chain is not built for a member, so it is not `helper_owner_call`).
    let indexed = Package {
        files: vec![
            ("src/lib.rs", "pub fn root() {}\n".to_string()),
            ("crates/inner/Cargo.toml", manifest("inner", None)),
            (
                "crates/inner/src/lib.rs",
                format!(
                    "{LIB}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{}\n}}\n",
                    test_file(BARE)
                ),
            ),
        ],
        ..member(String::new())
    };
    assert_eq!(indexed.relation()?.as_deref(), Some("same_test_file"));
    // The member's own name and the root package's name are both refused.
    for name in ["inner", "qual_reach"] {
        let tests = test_file(&qualified(name));
        assert_eq!(member(tests).relation()?, None, "{name}");
    }
    Ok(())
}

#[test]
fn an_entry_in_a_binary_crate_does_not_link() -> Result<(), String> {
    let main_source = format!("{LIB}\nfn main() {{}}\n");
    let binary = |tests: String| Package {
        manifest: manifest("qual_reach", None),
        diff_path: "src/main.rs",
        files: vec![
            ("src/main.rs", main_source.clone()),
            ("tests/api.rs", tests),
        ],
    };
    let baseline = format!("use qual_reach::order_discount;\n{}", test_file(BARE));
    // A binary-only package has no library to import from: refused (the
    // imported route proves the library's declaration chain).
    assert_eq!(binary(baseline).relation()?, None);
    assert_eq!(
        binary(test_file(&qualified("qual_reach"))).relation()?,
        None
    );
    Ok(())
}

#[test]
fn a_hyphenated_package_links_through_its_underscored_name() -> Result<(), String> {
    let package = Package {
        manifest: manifest("qual-reach", None),
        ..Package::root(LIB, &test_file(&qualified("qual_reach")))
    };
    assert_eq!(package.relation()?.as_deref(), LINKED);
    Ok(())
}

#[test]
fn a_lib_name_is_the_only_admitted_spelling() -> Result<(), String> {
    let with_lib = |crate_name: &str| Package {
        manifest: manifest("qual_reach", Some("shared_lib")),
        ..Package::root(LIB, &test_file(&qualified(crate_name)))
    };
    assert_eq!(with_lib("shared_lib").relation()?.as_deref(), LINKED);
    assert_eq!(with_lib("qual_reach").relation()?, None);
    Ok(())
}

/// The call fact carries the whole source line: an own-crate spelling quoted
/// in a string or comment beside a foreign call must not stand in for a real
/// own-crate call.
#[test]
fn a_quoted_own_crate_call_beside_a_foreign_call_does_not_link() -> Result<(), String> {
    let quoted = [
        "    let r = other::order_discount(10); let n = \"qual_reach::order_discount(10)\";",
        "    let r = other::order_discount(10); // qual_reach::order_discount(10)",
    ];
    for line in quoted {
        assert_eq!(root_relation(LIB, &test_file(line))?, None, "{line}");
    }
    // Control: a real own-crate call beside a quoted foreign one still links.
    let control =
        "    let r = qual_reach::order_discount(10); let n = \"other::order_discount(10)\";";
    assert_eq!(root_relation(LIB, &test_file(control))?.as_deref(), LINKED);
    Ok(())
}

/// A test in a module child resolves paths from a parent file this scan does
/// not read: `tests/api.rs` declares `mod qual_reach`, and the child's
/// `qual_reach::order_discount` names that module.
#[test]
fn a_test_module_child_does_not_link() -> Result<(), String> {
    let package = Package {
        files: vec![
            ("src/lib.rs", LIB.to_string()),
            (
                "tests/api.rs",
                "mod child;\nmod qual_reach {\n    pub fn order_discount(_: u32) -> u32 { 5 }\n}\n"
                    .to_string(),
            ),
            ("tests/api/child.rs", test_file(&qualified("qual_reach"))),
        ],
        ..Package::root("", "")
    };
    assert_eq!(package.relation()?, None);
    // Control: the child file is indexed and reaches the chain (the bare
    // spelling links from a child), so the refusal above is the guard.
    let child_bare = Package {
        files: vec![
            ("src/lib.rs", LIB.to_string()),
            ("tests/api.rs", "mod child;\n".to_string()),
            (
                "tests/api/child.rs",
                format!("use qual_reach::order_discount;\n{}", test_file(BARE)),
            ),
        ],
        ..Package::root("", "")
    };
    assert_eq!(child_bare.relation()?.as_deref(), LINKED);
    // Control: the same test as a test-crate root links.
    let root = Package::root(LIB, &test_file(&qualified("qual_reach")));
    assert_eq!(root.relation()?.as_deref(), LINKED);
    Ok(())
}

/// A library whose entry lives in a `src/` file module (`src/inner.rs`).
const MODULE_LIB: &str = "pub mod inner;\n";
/// `src/inner.rs`: the same helper and entry as `LIB`.
const MODULE_SRC: &str = LIB;
/// The helper and entry in a public inline module (the changed line is 3).
const PUB_INLINE_LIB: &str = "pub mod inner {\n    fn is_bulk(qty: u32) -> bool {\n        10 <= qty\n    }\n\n    pub fn order_discount(qty: u32) -> u32 {\n        if is_bulk(qty) { 5 } else { 0 }\n    }\n}\n";

fn spelled(path: &str) -> String {
    test_file(&format!(
        "    assert_eq!({path}(10), 5);\n    assert_eq!({path}(9), 0);"
    ))
}

/// A package whose entry is `qual_reach::inner::order_discount`, in the
/// file module `src/inner.rs` (or `src/inner/mod.rs`).
fn module_package<'a>(
    module_file: &'a str,
    lib: &str,
    tests: String,
    extra: Vec<(&'a str, String)>,
) -> Package<'a> {
    let mut files = vec![
        ("src/lib.rs", lib.to_string()),
        (module_file, MODULE_SRC.to_string()),
        ("tests/api.rs", tests),
    ];
    files.extend(extra);
    Package {
        manifest: manifest("qual_reach", None),
        diff_path: module_file,
        files,
    }
}

/// #8999: a module-qualified own-crate call of an entry in a file module or
/// an inline module links, beside the `use`-spelled baseline.
#[test]
fn a_module_qualified_own_crate_call_links() -> Result<(), String> {
    let baseline = format!(
        "use qual_reach::inner::order_discount;\n{}",
        test_file(BARE)
    );
    assert_eq!(
        module_package("src/inner.rs", MODULE_LIB, baseline, vec![])
            .relation()?
            .as_deref(),
        LINKED,
        "the use spelling must link, or the negatives prove nothing"
    );
    let qualified_path = spelled("qual_reach::inner::order_discount");
    for module_file in ["src/inner.rs", "src/inner/mod.rs"] {
        let package = module_package(module_file, MODULE_LIB, qualified_path.clone(), vec![]);
        assert_eq!(package.relation()?.as_deref(), LINKED, "{module_file}");
    }
    let inline = Package {
        ..Package::root(PUB_INLINE_LIB, &qualified_path)
    };
    assert_eq!(inline.relation()?.as_deref(), LINKED);
    Ok(())
}

/// Each module-qualified spelling that is not exactly the entry's module path
/// stays unrelated, paired with the linking spelling above.
#[test]
fn a_module_path_that_is_not_the_entrys_does_not_link() -> Result<(), String> {
    for path in [
        "qual_reach::order_discount",
        "qual_reach::other::order_discount",
        "qual_reach::inner::deeper::order_discount",
        "other_crate::inner::order_discount",
        "inner::order_discount",
        "crate::inner::order_discount",
        "self::inner::order_discount",
    ] {
        let package = module_package("src/inner.rs", MODULE_LIB, spelled(path), vec![]);
        assert_eq!(package.relation()?, None, "{path}");
    }
    // A method-shaped or quoted spelling is not a call either.
    for line in [
        "    assert_eq!(x.qual_reach::inner::order_discount(10), 5);",
        "    let n = \"qual_reach::inner::order_discount(10)\";",
    ] {
        let package = module_package("src/inner.rs", MODULE_LIB, test_file(line), vec![]);
        assert_eq!(package.relation()?, None, "{line}");
    }
    Ok(())
}

/// A `#[path]` remap, a binary that shares the module, or a binary-only
/// package makes the module path unreadable from the layout: no link, while
/// the same package with a `use` import and bare call still links.
#[test]
fn an_unplaceable_module_does_not_link() -> Result<(), String> {
    let qualified_path = spelled("qual_reach::inner::order_discount");
    let bare = format!(
        "use qual_reach::inner::order_discount;\n{}",
        test_file(BARE)
    );
    // A `#[path]` anywhere in the package.
    let remap = |tests: String| {
        module_package(
            "src/inner.rs",
            MODULE_LIB,
            tests,
            vec![(
                "src/util.rs",
                "#[path = \"elsewhere.rs\"]\nmod moved;\n".to_string(),
            )],
        )
    };
    assert_eq!(remap(bare.clone()).relation()?.as_deref(), LINKED);
    assert_eq!(remap(qualified_path.clone()).relation()?, None);
    // A binary root that declares the same module: the file composes into
    // both crates.
    let shared = |tests: String| {
        module_package(
            "src/inner.rs",
            MODULE_LIB,
            tests,
            vec![("src/main.rs", "mod inner;\nfn main() {}\n".to_string())],
        )
    };
    assert_eq!(shared(bare.clone()).relation()?.as_deref(), LINKED);
    assert_eq!(shared(qualified_path.clone()).relation()?, None);
    // A binary-only package.
    let binary = |tests: String| Package {
        manifest: manifest("qual_reach", None),
        diff_path: "src/inner.rs",
        files: vec![
            ("src/main.rs", "mod inner;\nfn main() {}\n".to_string()),
            ("src/inner.rs", MODULE_SRC.to_string()),
            ("tests/api.rs", tests),
        ],
    };
    // A binary-only package has no library to import from: refused.
    assert_eq!(binary(bare).relation()?, None);
    assert_eq!(binary(qualified_path).relation()?, None);
    Ok(())
}

/// The module-qualified spelling keeps every crate-name refusal: a rebinding
/// (raw `r#` spellings included), a glob or an `include!` in the test file, and
/// a raw-identifier module segment (not the spelling the matcher reads).
#[test]
fn a_module_qualified_call_fails_closed_on_rebinding_and_include() -> Result<(), String> {
    let qualified_path = spelled("qual_reach::inner::order_discount");
    let linked = module_package("src/inner.rs", MODULE_LIB, qualified_path.clone(), vec![]);
    assert_eq!(
        linked.relation()?.as_deref(),
        LINKED,
        "control: the unshadowed spelling links"
    );
    for prefix in [
        "use other_crate as qual_reach;\n",
        "use x as r#qual_reach;\n",
        "mod qual_reach {\n    pub mod inner {\n        pub fn order_discount(_: u32) -> u32 { 5 }\n    }\n}\n",
        "fn r#qual_reach() {}\n",
        "use some_dep::*;\n",
        "include!(\"generated.rs\");\n",
    ] {
        let tests = format!("{prefix}{qualified_path}");
        let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![]);
        assert_eq!(package.relation()?, None, "{prefix}");
    }
    let raw_segment = spelled("qual_reach::r#inner::order_discount");
    let package = module_package("src/inner.rs", MODULE_LIB, raw_segment, vec![]);
    assert_eq!(package.relation()?, None);
    Ok(())
}

/// A nested inline module path links; a cfg-gated or doubly declared module
/// does not (a cfg-selected alternate may be a different item), nor does a
/// test crate that declares a module named like the library.
#[test]
fn nested_cfg_and_shadowing_module_shapes_fail_closed() -> Result<(), String> {
    let nested_lib = "pub mod outer {\n    pub mod inner {\n        fn is_bulk(qty: u32) -> bool {\n            10 <= qty\n        }\n\n        pub fn order_discount(qty: u32) -> u32 {\n            if is_bulk(qty) { 5 } else { 0 }\n        }\n    }\n}\n";
    let nested = |tests: String| Package {
        ..Package::root(nested_lib, &tests)
    };
    assert_eq!(
        nested(spelled("qual_reach::outer::inner::order_discount"))
            .relation()?
            .as_deref(),
        LINKED,
        "control: the nested inline path links"
    );
    assert_eq!(
        nested(spelled("qual_reach::inner::order_discount")).relation()?,
        None
    );
    let qualified_path = spelled("qual_reach::inner::order_discount");
    for lib in [
        "#[cfg(feature = \"alt\")]\npub mod inner;\n#[cfg(not(feature = \"alt\"))]\npub mod inner {}\n",
        "#[cfg(feature = \"alt\")]\npub mod inner;\n",
        "#[cfg_attr(docsrs, doc(hidden))]\npub mod inner;\n",
        "pub mod inner;\nmod other {\n    pub mod inner {}\n}\n",
    ] {
        let package = module_package("src/inner.rs", lib, qualified_path.clone(), vec![]);
        assert_eq!(package.relation()?, None, "{lib}");
    }
    // A test crate that declares a module named like the library.
    let shadow = format!("mod qual_reach;\n{qualified_path}");
    let package = module_package(
        "src/inner.rs",
        MODULE_LIB,
        shadow,
        vec![("tests/qual_reach.rs", "pub mod inner {}\n".to_string())],
    );
    assert_eq!(package.relation()?, None);
    Ok(())
}

/// Include macros spelled with token separation, an `include`-family macro,
/// and an included module shadowing the library name all refuse; the same
/// package without an include links and renders `exposed`.
#[test]
fn include_spellings_in_the_test_crate_or_library_fail_closed() -> Result<(), String> {
    let qualified_path = spelled("qual_reach::inner::order_discount");
    let control = module_package("src/inner.rs", MODULE_LIB, qualified_path.clone(), vec![]);
    assert_eq!(control.relation()?.as_deref(), LINKED, "control links");
    assert_eq!(control.classification()?, "exposed", "control renders");
    let shadow = (
        "tests/generated.inc",
        "pub mod qual_reach {\n    pub mod inner {\n        pub fn order_discount(_: u32) -> u32 { 5 }\n    }\n}\n".to_string(),
    );
    for include in [
        "include!(\"generated.inc\");",
        "include /* c */ !(\"generated.inc\");",
        "include\n!(\"generated.inc\");",
        "include ! (\"generated.inc\");",
        "core::include!(\"generated.inc\");",
        "include_str!(\"generated.inc\");",
    ] {
        let tests = format!("{include}\n{qualified_path}");
        let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![shadow.clone()]);
        assert_eq!(package.relation()?, None, "{include}");
        assert_ne!(package.classification()?, "exposed", "{include}");
    }
    // An include (or an item-position macro) in a library file.
    for lib in [
        "pub mod inner;\ninclude /* c */ !(\"generated.inc\");\n",
        "pub mod inner;\nmake_items!();\n",
    ] {
        let package = module_package("src/inner.rs", lib, qualified_path.clone(), vec![]);
        assert_eq!(package.relation()?, None, "{lib}");
        assert_ne!(package.classification()?, "exposed", "{lib}");
    }
    Ok(())
}

/// A cfg or cfg_attr on the module declaration, hidden or not by comments, and
/// an alternate re-export spelled in a comment, refuse by parsed attributes.
#[test]
fn cfg_on_module_declarations_is_read_from_the_syntax_tree() -> Result<(), String> {
    let qualified_path = spelled("qual_reach::inner::order_discount");
    for lib in [
        "#[cfg(any())]\npub mod inner;\n",
        "#[cfg(any())]\n// {\npub mod inner;\n",
        "#[cfg(any())]\n/* ; } */\npub mod inner;\n",
        "#[cfg(feature = \"alt\")]\n/// note; a doc comment with a semicolon\npub mod inner;\n",
        "#[doc = \"{\"]\n#[cfg(any())]\npub mod inner;\n",
        "#[cfg(any())] // pub use other as inner;\npub mod inner;\n",
        "#[cfg_attr(all(), cfg(any()))]\npub mod inner;\n",
        "#[cfg(any())]\npub mod wrapper {\n    pub mod inner {}\n}\npub mod inner;\n",
    ] {
        let package = module_package("src/inner.rs", lib, qualified_path.clone(), vec![]);
        assert_eq!(package.relation()?, None, "{lib}");
        assert_ne!(package.classification()?, "exposed", "{lib}");
    }
    // An inner cfg in the module's own file.
    let gated_file = Package {
        files: vec![
            ("src/lib.rs", MODULE_LIB.to_string()),
            ("src/inner.rs", format!("#![cfg(any())]\n{MODULE_SRC}")),
            ("tests/api.rs", qualified_path.clone()),
        ],
        ..module_package("src/inner.rs", MODULE_LIB, String::new(), vec![])
    };
    assert_eq!(gated_file.relation()?, None);
    // A comment-only mention of cfg beside an unconditional module links.
    let comment = "// #[cfg(any())] is not applied\npub mod inner;\n";
    let package = module_package("src/inner.rs", comment, qualified_path, vec![]);
    assert_eq!(package.relation()?.as_deref(), LINKED, "control");
    assert_eq!(package.classification()?, "exposed", "control");
    Ok(())
}

/// A qualified path that appears only inside a multiline raw string or block
/// comment, on a line that ends with a real foreign call of the same name,
/// does not link; the own-library positive control does.
#[test]
fn multiline_non_code_text_does_not_admit_a_foreign_call() -> Result<(), String> {
    let control = module_package(
        "src/inner.rs",
        MODULE_LIB,
        spelled("qual_reach::inner::order_discount"),
        vec![],
    );
    assert_eq!(control.relation()?.as_deref(), LINKED, "control links");
    for body in [
        "    let _doc = r#\"\n    see qual_reach::inner::order_discount(10)\"#; foreign::order_discount(10);",
        "    /*\n    qual_reach::inner::order_discount(10) */ foreign::order_discount(10);",
    ] {
        let package = module_package("src/inner.rs", MODULE_LIB, test_file(body), vec![]);
        assert_eq!(package.relation()?, None, "{body}");
        assert_ne!(package.classification()?, "exposed", "{body}");
    }
    Ok(())
}

/// An import whose first segment is the library name trusts that name: when
/// the name is aliased, redeclared or bound through an included file, the
/// bare call of the imported entry stays unrelated.
#[test]
fn an_own_looking_import_prefix_must_name_the_library() -> Result<(), String> {
    let import = "use qual_reach::inner::order_discount;\n";
    let control = module_package(
        "src/inner.rs",
        MODULE_LIB,
        format!("{import}{}", test_file(BARE)),
        vec![],
    );
    assert_eq!(control.relation()?.as_deref(), LINKED, "control links");
    assert_eq!(control.classification()?, "exposed", "control renders");
    for prefix in [
        "use std as qual_reach;\n",
        "use std as r#qual_reach;\n",
        "mod qual_reach {\n    pub use std::cmp;\n    pub mod inner {\n        pub fn order_discount(_: u32) -> u32 { 5 }\n    }\n}\n",
        "use some_dep::qual_reach;\n",
        "use some_dep::*;\n",
        "include ! (\"bindings.rs\");\n",
    ] {
        let tests = format!("{prefix}{import}{}", test_file(BARE));
        let package = module_package(
            "src/inner.rs",
            MODULE_LIB,
            tests,
            vec![("tests/bindings.rs", "use std as qual_reach;\n".to_string())],
        );
        assert_eq!(package.relation()?, None, "{prefix}");
        assert_ne!(package.classification()?, "exposed", "{prefix}");
    }
    Ok(())
}

/// A bare imported-entry spelling that appears only in a string or comment
/// (single or multiline) while a foreign call of the same terminal name is the
/// only real call does not link; a real imported call does.
#[test]
fn bare_spelling_in_non_code_text_does_not_admit_an_import() -> Result<(), String> {
    let import = "use qual_reach::inner::order_discount;\n";
    let control = module_package(
        "src/inner.rs",
        MODULE_LIB,
        format!("{import}{}", test_file(BARE)),
        vec![],
    );
    assert_eq!(control.relation()?.as_deref(), LINKED, "control links");
    assert_eq!(control.classification()?, "exposed", "control renders");
    for body in [
        "    let _text = r#\"a quoted bare entry\norder_discount(10)\"#;\n    assert_eq!(foreign::order_discount(10), 5);",
        "    let _text = \"order_discount(10)\";\n    assert_eq!(foreign::order_discount(10), 5);",
        "    /*\n    order_discount(10)\n    */\n    assert_eq!(foreign::order_discount(10), 5);",
    ] {
        let package = module_package(
            "src/inner.rs",
            MODULE_LIB,
            format!("{import}{}", test_file(body)),
            vec![],
        );
        assert_eq!(package.relation()?, None, "{body}");
        assert_ne!(package.classification()?, "exposed", "{body}");
    }
    Ok(())
}

/// Known limit (RIPR-SPEC-0159): a relative-prefixed import of the entry
/// (`self::`, `crate::`, `super::` in an integration test), an inline-module
/// route, an extern-crate alias or a re-export is never resolved, even when it
/// is valid. Each refuses, with rendered classification; the direct list
/// import is the supported control.
#[test]
fn relative_and_reexport_import_routes_are_refused() -> Result<(), String> {
    let local = "mod qual_reach {\n    pub mod inner {\n        pub use std::cmp::max as order_discount;\n    }\n}\n";
    for (module, import) in [
        (local, "use self::qual_reach::inner::order_discount;\n"),
        (local, "use crate::qual_reach::inner::order_discount;\n"),
        (local, "use self::qual_reach::inner::{order_discount};\n"),
        (
            "mod api {\n    pub use qual_reach::inner;\n    pub use std::cmp as foreign;\n}\n",
            "use self::api::foreign::order_discount;\n",
        ),
        (
            "mod api {\n    pub use qual_reach::inner;\n    pub use std::cmp::*;\n}\n",
            "use self::api::inner::order_discount;\n",
        ),
        (
            "extern crate qual_reach;\nmod checks {\n    mod qual_reach {\n        pub use std::cmp::max as order_discount;\n    }\n    use self::qual_reach::order_discount;\n}\n",
            "use self::checks::qual_reach::order_discount;\n",
        ),
        (
            "mod checks {\n    extern crate core as lib_alias;\n}\n",
            "use self::checks::lib_alias::cmp::max as order_discount;\n",
        ),
        // Valid routes that the minimal boundary refuses (known limits).
        (
            "extern crate qual_reach;\n",
            "use self::qual_reach::inner::order_discount;\n",
        ),
        (
            "extern crate qual_reach as lib_alias;\n",
            "use crate::lib_alias::inner::order_discount;\n",
        ),
        (
            "mod api {\n    pub use ::qual_reach::inner;\n}\n",
            "use self::api::inner::order_discount;\n",
        ),
        (
            "mod api {\n    pub use qual_reach::inner;\n}\n",
            "use self::api::inner::order_discount;\n",
        ),
    ] {
        let tests = format!("{module}{import}{}", test_file(BARE));
        let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![]);
        assert_eq!(package.relation()?, None, "{module}{import}");
        assert_ne!(package.classification()?, "exposed", "{module}{import}");
    }
    // The direct list import still links, and an unrelated relative import
    // never suppresses an exact qualified call.
    let list = format!(
        "use qual_reach::inner::{{order_discount, other}};\n{}",
        test_file(BARE)
    );
    let package = module_package("src/inner.rs", MODULE_LIB, list, vec![]);
    assert_eq!(
        package.relation()?.as_deref(),
        LINKED,
        "list import control"
    );
    assert_eq!(package.classification()?, "exposed");
    let tests = format!(
        "mod arbitrary {{\n    pub mod cmp {{}}\n}}\nuse self::arbitrary::cmp::order_discount;\n{}",
        spelled("qual_reach::inner::order_discount")
    );
    let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![]);
    assert_eq!(package.relation()?.as_deref(), LINKED, "unrelated import");
    Ok(())
}

/// A nearer callable binding of the entry's name (a `const` or `static`)
/// refuses the imported bare call; the call-fact line text is the whole line,
/// so a quote or comment ending on a line with a real foreign call of the same
/// name does not admit it either.
#[test]
fn a_nearer_binding_or_same_line_non_code_does_not_admit_a_bare_call() -> Result<(), String> {
    let import = "use qual_reach::inner::order_discount;\n";
    for binding in [
        "const order_discount: fn(u32) -> u32 = other::order_discount;\n",
        "static order_discount: fn(u32) -> u32 = other::order_discount;\n",
    ] {
        let tests = format!("{import}{binding}{}", test_file(BARE));
        let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![]);
        assert_eq!(package.relation()?, None, "{binding}");
        assert_ne!(package.classification()?, "exposed", "{binding}");
    }
    // Known limit: the const/static scan is file-wide and fail-closed, so a
    // same-named value in a module or the test body refuses too.
    for extra in [
        "mod unrelated {\n    pub const order_discount: u32 = 7;\n}\n",
        "mod checks {\n    pub static order_discount: u32 = 7;\n}\n",
    ] {
        let tests = format!("{import}{extra}{}", test_file(BARE));
        let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![]);
        assert_eq!(package.relation()?, None, "{extra}");
        assert_ne!(package.classification()?, "exposed", "{extra}");
    }
    let body_const = format!(
        "{import}{}",
        test_file(&format!(
            "    const order_discount: fn(u32) -> u32 = other::order_discount;\n{BARE}"
        ))
    );
    let package = module_package("src/inner.rs", MODULE_LIB, body_const, vec![]);
    assert_eq!(package.relation()?, None, "body const");
    for body in [
        "    let _t = \"order_discount(10)\"; assert_eq!(foreign::order_discount(10), 5);",
        "    /* order_discount(10) */ assert_eq!(foreign::order_discount(10), 5);",
        "    let _t = r#\"\n    order_discount(10)\"#; assert_eq!(foreign::order_discount(10), 5);",
    ] {
        let tests = format!("{import}{}", test_file(body));
        let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![]);
        assert_eq!(package.relation()?, None, "{body}");
        assert_ne!(package.classification()?, "exposed", "{body}");
    }
    Ok(())
}

/// An absolute `use ::qual_reach::..` import goes through the same proof as
/// the bare prefix: a positive when nothing rebinds the name, refused when the
/// file also aliases or redeclares it.
#[test]
fn an_absolute_import_prefix_goes_through_the_same_proof() -> Result<(), String> {
    let import = "use ::qual_reach::inner::order_discount;\n";
    let control = module_package(
        "src/inner.rs",
        MODULE_LIB,
        format!("{import}{}", test_file(BARE)),
        vec![],
    );
    assert_eq!(control.relation()?.as_deref(), LINKED, "control links");
    assert_eq!(control.classification()?, "exposed", "control renders");
    for prefix in [
        "use std as qual_reach;\n",
        "mod qual_reach {\n    pub use std::cmp;\n}\n",
    ] {
        let tests = format!("{prefix}{import}{}", test_file(BARE));
        let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![]);
        assert_eq!(package.relation()?, None, "{prefix}");
        assert_ne!(package.classification()?, "exposed", "{prefix}");
    }
    Ok(())
}

/// A macro exported from a test-crate module and invoked in item position may
/// expand to `mod qual_reach { .. }`; the test file refuses on both the
/// qualified and the imported bare route.
#[test]
fn an_item_position_macro_in_the_test_crate_fails_closed() -> Result<(), String> {
    let qualified_path = spelled("qual_reach::inner::order_discount");
    let common = (
        "tests/common/mod.rs",
        "#[macro_export]\nmacro_rules! mk {\n    () => {\n        mod qual_reach {\n            pub mod inner {\n                pub fn order_discount(_: u32) -> u32 { 5 }\n            }\n        }\n    };\n}\n".to_string(),
    );
    let tests = format!("#[macro_use]\nmod common;\nmk!();\n{qualified_path}");
    let package = module_package("src/inner.rs", MODULE_LIB, tests, vec![common.clone()]);
    assert_eq!(package.relation()?, None);
    assert_ne!(package.classification()?, "exposed");
    let bare = format!(
        "#[macro_use]\nmod common;\nmk!();\nuse qual_reach::inner::order_discount;\n{}",
        test_file(BARE)
    );
    let package = module_package("src/inner.rs", MODULE_LIB, bare, vec![common]);
    assert_eq!(package.relation()?, None, "bare route");
    assert_ne!(package.classification()?, "exposed", "bare route");
    Ok(())
}

/// The imported suffix must resolve to the entry's own definition: a path
/// through another module (a library re-export that leaves the crate) refuses,
/// the entry's exact module path links, and a unit test inside `src/` that
/// shadows a module still refuses.
#[test]
fn an_imported_suffix_must_spell_the_entrys_module_path() -> Result<(), String> {
    let control = module_package(
        "src/inner.rs",
        MODULE_LIB,
        format!(
            "use qual_reach::inner::order_discount;\n{}",
            test_file(BARE)
        ),
        vec![],
    );
    assert_eq!(control.relation()?.as_deref(), LINKED, "control links");
    assert_eq!(control.classification()?, "exposed", "control renders");
    // A library re-export that leaves the crate: `foreign::order_discount` is
    // not the entry's module path.
    let foreign_lib =
        "pub mod inner;\npub mod foreign {\n    pub use std::cmp::max as order_discount;\n}\n";
    for import in [
        "use qual_reach::foreign::order_discount;\n",
        "use qual_reach::order_discount;\n",
        "use qual_reach::foreign::max as order_discount;\n",
        "use qual_reach::inner::deeper::order_discount;\n",
    ] {
        let package = module_package(
            "src/inner.rs",
            foreign_lib,
            format!("{import}{}", test_file(BARE)),
            vec![],
        );
        assert_eq!(package.relation()?, None, "{import}");
        assert_ne!(package.classification()?, "exposed", "{import}");
    }
    // Root re-export scope: a decoy module's `pub use`, a competing root
    // value binding, or a non-public re-export never credits the entry.
    let reexport_tests = format!("use qual_reach::order_discount;\n{}", test_file(BARE));
    let nested = |lib: &str| Package::root(lib, &reexport_tests);
    let valid_reexport = "mod inner {\n    fn is_bulk(qty: u32) -> bool {\n        10 <= qty\n    }\n\n    pub fn order_discount(qty: u32) -> u32 {\n        if is_bulk(qty) { 5 } else { 0 }\n    }\n}\npub use inner::order_discount;\n";
    let control = Package {
        ..nested(valid_reexport)
    };
    assert_eq!(
        control.relation()?.as_deref(),
        LINKED,
        "root re-export control"
    );
    assert_eq!(
        control.classification()?,
        "exposed",
        "root re-export control"
    );
    for (extra, tail) in [
        (
            "pub mod decoy {\n    pub use crate::inner::order_discount;\n}\n",
            "mod inner2 {}\n",
        ),
        (
            "",
            "pub const order_discount: fn(u32) -> u32 = std::cmp::max::<u32>;\n",
        ),
    ] {
        let lib = valid_reexport.replace(
            "pub use inner::order_discount;\n",
            &format!("{extra}{tail}"),
        );
        let package = Package { ..nested(&lib) };
        assert_eq!(package.relation()?, None, "{extra}{tail}");
        assert_ne!(package.classification()?, "exposed", "{extra}{tail}");
    }
    let private = valid_reexport.replace("pub use inner", "use inner");
    let package = Package { ..nested(&private) };
    assert_eq!(package.relation()?, None, "private re-export");
    // A unit test inside `src/` whose module shadows the std path.
    let unit_lib = format!(
        "{LIB}\n#[cfg(test)]\nmod tests {{\n    mod foreign {{\n        pub use std::cmp::max as order_discount;\n    }}\n    use self::foreign::order_discount;\n\n    #[test]\n    fn discount_boundary() {{\n        assert_eq!(order_discount(10, 5), 10);\n        assert_eq!(order_discount(9, 5), 9);\n    }}\n}}\n"
    );
    let package = Package::root(&unit_lib, "");
    assert_ne!(package.relation()?.as_deref(), LINKED, "unit shadow");
    assert_ne!(package.classification()?, "exposed", "unit shadow");
    Ok(())
}

/// An entry defined in a PRIVATE inline module and re-exported at the crate
/// root: the bare import of the re-export links (see the root re-export
/// control above); the fully qualified spelling of the re-export path is a
/// shorter path than the entry's own module path and is refused (a conservative
/// loss, named in RIPR-SPEC-0159). Module visibility is not
/// checked: a spelling through a private module would not compile, and is a named
/// limit rather than asserted here.
#[test]
fn an_inline_module_entry_reexported_at_the_root_links_only_by_import() -> Result<(), String> {
    let lib = "mod inner {\n    fn is_bulk(qty: u32) -> bool {\n        10 <= qty\n    }\n\n    pub fn order_discount(qty: u32) -> u32 {\n        if is_bulk(qty) { 5 } else { 0 }\n    }\n}\npub use inner::order_discount;\n";
    let bare = format!("use qual_reach::order_discount;\n{}", test_file(BARE));
    assert_eq!(
        Package::root(lib, &bare).relation()?.as_deref(),
        LINKED,
        "bare import of the root re-export"
    );
    let qualified_reexport = spelled("qual_reach::order_discount");
    assert_eq!(
        Package::root(lib, &qualified_reexport).relation()?,
        None,
        "qualified re-export spelling is a conservative refusal"
    );
    Ok(())
}

/// The imported route proves the library binds the module path to the
/// entry's file: a cfg-selected alternate or a `#[path]` remap that rebinds
/// the module to a std re-export refuses (rendered non-promotion), while a
/// genuine unconditional declaration, a harmless unrelated remap and a binary
/// that shares the module name still link.
#[test]
fn the_library_must_bind_the_imported_module_path_to_the_entrys_file() -> Result<(), String> {
    let import = "use qual_reach::inner::order_discount;\n";
    let tests = format!("{import}{}", test_file(BARE));
    let genuine = module_package("src/inner.rs", MODULE_LIB, tests.clone(), vec![]);
    assert_eq!(
        genuine.relation()?.as_deref(),
        LINKED,
        "genuine import links"
    );
    assert_eq!(
        genuine.classification()?,
        "exposed",
        "genuine import renders"
    );
    let harmless = module_package(
        "src/inner.rs",
        MODULE_LIB,
        tests.clone(),
        vec![(
            "src/util.rs",
            "#[path = \"elsewhere.rs\"]\nmod moved;\n".to_string(),
        )],
    );
    assert_eq!(harmless.relation()?.as_deref(), LINKED, "harmless remap");
    let shared = module_package(
        "src/inner.rs",
        MODULE_LIB,
        tests.clone(),
        vec![("src/main.rs", "mod inner;\nfn main() {}\n".to_string())],
    );
    assert_eq!(shared.relation()?.as_deref(), LINKED, "shared binary");
    for lib in [
        "#[cfg(any())]\npub mod inner;\n#[cfg(not(any()))]\npub use std::cmp as inner;\n",
        "#[path = \"foreign.rs\"]\npub mod inner;\n",
        "#[cfg(feature = \"alt\")]\npub mod inner;\n",
    ] {
        let package = module_package(
            "src/inner.rs",
            lib,
            tests.clone(),
            vec![(
                "src/foreign.rs",
                "pub use std::cmp::max as order_discount;\n".to_string(),
            )],
        );
        // Either no finding is produced for the unreachable file (nothing can
        // be credited) or the finding is not related and not exposed.
        match package.outcome() {
            Err(error) if error.contains("no predicate finding") => {}
            Err(error) => return Err(error),
            Ok((relation, classification)) => {
                assert_eq!(relation, None, "{lib}");
                assert_ne!(classification, "exposed", "{lib}");
            }
        }
    }
    Ok(())
}

/// The declaration chain from the library root must reach the indexed
/// definition's file: a `#[path]` that compiles `src/cmp.rs` under another
/// name while an inline `cmp` re-exports std, a matching-name direct remap and
/// a wrong-ancestry orphan all refuse; the genuine `pub mod cmp;` links.
#[test]
fn the_declaration_chain_must_reach_the_definitions_file() -> Result<(), String> {
    let tests = format!("use qual_reach::cmp::order_discount;\n{}", test_file(BARE));
    let package = |lib: &str| Package {
        manifest: manifest("qual_reach", None),
        diff_path: "src/cmp.rs",
        files: vec![
            ("src/lib.rs", lib.to_string()),
            ("src/cmp.rs", LIB.to_string()),
            ("tests/api.rs", tests.clone()),
        ],
    };
    let genuine = package("pub mod cmp;\n");
    assert_eq!(
        genuine.relation()?.as_deref(),
        LINKED,
        "genuine chain links"
    );
    assert_eq!(
        genuine.classification()?,
        "exposed",
        "genuine chain renders"
    );
    for lib in [
        "#[path = \"cmp.rs\"]\npub mod actual;\npub mod cmp {\n    pub use std::cmp::max as order_discount;\n}\n",
        "#[path = \"cmp.rs\"]\npub mod cmp;\n",
        "pub mod wrapper {\n    pub mod cmp {}\n}\n",
        "pub mod cmp;\npub mod other {\n    pub mod cmp {}\n}\n",
    ] {
        let candidate = package(lib);
        match candidate.outcome() {
            Err(error) if error.contains("no predicate finding") => {}
            Err(error) => return Err(error),
            Ok((relation, classification)) => {
                assert_eq!(relation, None, "{lib}");
                assert_ne!(classification, "exposed", "{lib}");
            }
        }
    }
    Ok(())
}

/// A nearer binding inside the test body (a function-local import of another
/// path, a `let`, a closure parameter) shadows the exact-looking file-level
/// import: unrelated, with rendered classification. The plain import links.
#[test]
fn a_nearer_binding_inside_the_test_body_shadows_the_import() -> Result<(), String> {
    let import = "use qual_reach::inner::order_discount;\n";
    for body in [
        "    use foreign::order_discount;\n    assert_eq!(order_discount(10), 5);",
        "    let order_discount = foreign::order_discount;\n    assert_eq!(order_discount(10), 5);",
        "    let f = |order_discount: fn(u32) -> u32| order_discount(10);\n    assert_eq!(f(foreign::order_discount), 5);",
        "    #[allow(non_camel_case_types)]\n    struct order_discount(u32);\n    assert_eq!(order_discount(10).0, 10);",
        "    #[allow(non_camel_case_types)]\n    struct order_discount(u32);\n    let _ = order_discount(10);",
        "    fn order_discount(_: u32) -> u32 { 5 }\n    assert_eq!(order_discount(10), 5);",
        "    const order_discount: fn(u32) -> u32 = |x| x;\n    assert_eq!(order_discount(10), 10);",
    ] {
        let package = module_package(
            "src/inner.rs",
            MODULE_LIB,
            format!("{import}{}", test_file(body)),
            vec![],
        );
        assert_ne!(package.relation()?.as_deref(), LINKED, "{body}");
        assert_ne!(package.classification()?, "exposed", "{body}");
    }
    Ok(())
}

/// Names in the type namespace (type alias, trait, enum, union, module, braced
/// struct) cannot shadow a bare value call of the imported entry.
#[test]
fn type_namespace_items_do_not_shadow_the_imported_call() -> Result<(), String> {
    let import = "use qual_reach::inner::order_discount;\n";
    let calls = "    assert_eq!(order_discount(10), 5);\n    assert_eq!(order_discount(9), 0);";
    for item in [
        "    #[allow(non_camel_case_types)]\n    type order_discount = u32;\n    let _: order_discount = 0;",
        "    #[allow(non_camel_case_types)]\n    struct order_discount { _x: u32 }",
        "    #[allow(non_camel_case_types)]\n    trait order_discount {}",
        "    #[allow(non_camel_case_types)]\n    enum order_discount { A }",
        "    mod order_discount {}",
    ] {
        let package = module_package(
            "src/inner.rs",
            MODULE_LIB,
            format!("{import}{}", test_file(&format!("{item}\n{calls}"))),
            vec![],
        );
        assert_eq!(package.relation()?.as_deref(), LINKED, "{item}");
        assert_eq!(package.classification()?, "exposed", "{item}");
    }
    Ok(())
}

/// A closure parameter named like the entry shadows only the calls inside the
/// closure; outer calls (spelled with an interior comment, on their own lines)
/// still reach the imported entry, while a plain or comment-split call inside
/// the shadow scope refuses.
#[test]
fn a_closure_shadow_does_not_veto_commented_outer_calls() -> Result<(), String> {
    let import = "use qual_reach::inner::order_discount;\n";
    let outer = "    assert_eq!(order_discount /* outer */ (10), 5);\n    assert_eq!(order_discount\n        (9), 0);\n    assert_eq!(order_discount /* again */(11), 5);";
    let closure = "    let _unused = |order_discount: fn(u32) -> u32| order_discount(10);\n";
    let package = module_package(
        "src/inner.rs",
        MODULE_LIB,
        format!("{import}{}", test_file(&format!("{closure}{outer}"))),
        vec![],
    );
    assert_eq!(package.relation()?.as_deref(), LINKED);
    for shadowed in [
        "    let f = |order_discount: fn(u32) -> u32| order_discount /* c */ (10);\n    assert_eq!(f(foreign::order_discount), 5);",
        "    let order_discount = foreign::order_discount;\n    assert_eq!(order_discount /* c */ (10), 5);",
    ] {
        let package = module_package(
            "src/inner.rs",
            MODULE_LIB,
            format!("{import}{}", test_file(shadowed)),
            vec![],
        );
        assert_ne!(package.relation()?.as_deref(), LINKED, "{shadowed}");
    }
    Ok(())
}

/// A unit test in a separate `src/` file importing through `crate::` goes
/// through the same suffix proof (a `crate::foreign::` std re-export refuses;
/// the genuine `crate::cmp::` import exposes); a binding whose scope holds no
/// call does not suppress a real imported call; an out-of-line module inside
/// an inline ancestor (`pub mod outer { pub mod cmp; }`) still links.
#[test]
fn unit_crate_imports_scoped_bindings_and_inline_ancestors() -> Result<(), String> {
    let unit = |lib: &str, import: &str, calls: &str| Package {
        manifest: manifest("qual_reach", None),
        diff_path: "src/cmp.rs",
        files: vec![
            ("src/lib.rs", lib.to_string()),
            ("src/cmp.rs", LIB.to_string()),
            (
                "src/unit.rs",
                format!("{import}\n#[test]\nfn discount_boundary() {{\n{calls}\n}}\n"),
            ),
            ("tests/api.rs", String::new()),
        ],
    };
    let lib = "pub mod cmp;\npub mod foreign {\n    pub use std::cmp::max as order_discount;\n}\n#[cfg(test)]\nmod unit;\n";
    let own = "    assert_eq!(order_discount(10), 5);\n    assert_eq!(order_discount(9), 0);";
    let genuine = unit(lib, "use crate::cmp::order_discount;", own);
    assert_eq!(
        genuine.relation()?.as_deref(),
        LINKED,
        "genuine crate:: import"
    );
    assert_eq!(
        genuine.classification()?,
        "exposed",
        "genuine crate:: import"
    );
    let two_args =
        "    assert_eq!(order_discount(10, 5), 10);\n    assert_eq!(order_discount(9, 5), 9);";
    let foreign = unit(lib, "use crate::foreign::order_discount;", two_args);
    assert_ne!(foreign.relation()?.as_deref(), LINKED, "crate::foreign");
    assert_ne!(foreign.classification()?, "exposed", "crate::foreign");

    let import = "use qual_reach::inner::order_discount;\n";
    let unused = module_package(
        "src/inner.rs",
        MODULE_LIB,
        format!(
            "{import}{}",
            test_file(&format!(
                "    let _unused = |order_discount: u32| order_discount + 1;\n{BARE}"
            ))
        ),
        vec![],
    );
    // A shadowed call in one closure does not veto the outer real calls.
    let split = module_package(
        "src/inner.rs",
        MODULE_LIB,
        format!(
            "{import}{}",
            test_file(&format!(
                "    let _unused = |order_discount: fn(u32, u32) -> u32| order_discount(10, 5);\n{BARE}"
            ))
        ),
        vec![],
    );
    assert_eq!(split.relation()?.as_deref(), LINKED, "outer calls reach");
    assert_eq!(split.classification()?, "exposed", "outer calls reach");
    assert_eq!(
        unused.relation()?.as_deref(),
        LINKED,
        "unused closure param"
    );
    assert_eq!(unused.classification()?, "exposed", "unused closure param");

    let nested = Package {
        manifest: manifest("qual_reach", None),
        diff_path: "src/outer/cmp.rs",
        files: vec![
            (
                "src/lib.rs",
                "pub mod outer {\n    pub mod cmp;\n}\n".to_string(),
            ),
            ("src/outer/cmp.rs", LIB.to_string()),
            (
                "tests/api.rs",
                format!(
                    "use qual_reach::outer::cmp::order_discount;\n{}",
                    test_file(BARE)
                ),
            ),
        ],
    };
    assert_eq!(nested.relation()?.as_deref(), LINKED, "inline ancestor");
    assert_eq!(nested.classification()?, "exposed", "inline ancestor");
    Ok(())
}

// ---- #9971: relative `self::`/`super::` imports in unit tests under `src/` ----

const UNIT_GATE: &str = "#[cfg(test)]\nmod unit;\n";

fn unit_source(import: &str, body: &str) -> String {
    format!("{import}\n{}", test_file(body))
}

fn unit_package(diff_path: &'static str, files: Vec<(&'static str, String)>) -> Package<'static> {
    Package {
        manifest: manifest("qual_reach", None),
        diff_path,
        files,
    }
}

/// The recovered state: related by the helper-owner call and exposed.
fn assert_recovered(package: &Package<'_>, label: &str) -> Result<(), String> {
    assert_eq!(package.relation()?.as_deref(), LINKED, "{label}");
    assert_eq!(package.classification()?, "exposed", "{label}");
    Ok(())
}

/// The refused state: the helper-owner relation is withheld (a weaker
/// name-affinity relation may remain) and the finding is not exposed.
fn assert_refused(package: &Package<'_>, label: &str) -> Result<(), String> {
    let (relation, classification) = package
        .outcome()
        .map_err(|error| format!("{label}: {}", error.chars().take(200).collect::<String>()))?;
    assert_ne!(relation.as_deref(), LINKED, "{label}");
    assert_ne!(classification, "exposed", "{label}");
    Ok(())
}

/// The root-module unit-test layout the verdict-corpus row uses.
fn root_unit(lib_tail: &str, unit: String) -> Package<'static> {
    unit_package(
        "src/lib.rs",
        vec![
            ("src/lib.rs", format!("{LIB}\n{lib_tail}")),
            ("src/unit.rs", unit),
        ],
    )
}

#[test]
fn a_relative_import_resolved_to_the_entrys_module_links() -> Result<(), String> {
    let import = "use super::order_discount;";
    assert_recovered(
        &root_unit(UNIT_GATE, unit_source(import, BARE)),
        "super to the crate root",
    )?;
    // The entry in a file module; the test module is a child of it.
    assert_recovered(
        &unit_package(
            "src/cmp.rs",
            vec![
                ("src/lib.rs", "pub mod cmp;\n".to_string()),
                ("src/cmp.rs", format!("{LIB}\n{UNIT_GATE}")),
                ("src/cmp/unit.rs", unit_source(import, BARE)),
            ],
        ),
        "super to a file module",
    )?;
    // Two levels up to the crate root.
    assert_recovered(
        &unit_package(
            "src/lib.rs",
            vec![
                ("src/lib.rs", format!("{LIB}\npub mod cmp;\n")),
                ("src/cmp.rs", UNIT_GATE.to_string()),
                (
                    "src/cmp/unit.rs",
                    unit_source("use super::super::order_discount;", BARE),
                ),
            ],
        ),
        "super::super",
    )?;
    // A sibling module through `super::<module>::`.
    assert_recovered(
        &unit_package(
            "src/inner.rs",
            vec![
                ("src/lib.rs", format!("mod inner;\n{UNIT_GATE}")),
                ("src/inner.rs", LIB.to_string()),
                (
                    "src/unit.rs",
                    unit_source("use super::inner::order_discount;", BARE),
                ),
            ],
        ),
        "super::module::entry",
    )?;
    // An entry in an inline module of the parent.
    let inline_lib = NESTED_LIB.replace("pub use inner::order_discount;\n", "");
    assert_recovered(
        &unit_package(
            "src/lib.rs",
            vec![
                ("src/lib.rs", format!("{inline_lib}{UNIT_GATE}")),
                (
                    "src/unit.rs",
                    unit_source("use super::inner::order_discount;", BARE),
                ),
            ],
        ),
        "super::inline_module::entry",
    )?;
    // `self::` into a child module the test file itself declares.
    assert_recovered(
        &unit_package(
            "src/cmp/inner.rs",
            vec![
                ("src/lib.rs", "pub mod cmp;\n".to_string()),
                (
                    "src/cmp.rs",
                    format!(
                        "mod inner;\n{}",
                        unit_source("use self::inner::order_discount;", BARE)
                    ),
                ),
                ("src/cmp/inner.rs", LIB.to_string()),
            ],
        ),
        "self::child::entry",
    )?;
    // A list import resolves like the plain one.
    assert_recovered(
        &root_unit(
            UNIT_GATE,
            unit_source("use super::{order_discount, is_bulk};", BARE),
        ),
        "list import",
    )
}

#[test]
fn a_relative_import_that_does_not_name_the_entrys_module_stays_refused() -> Result<(), String> {
    let gate = |lib: &str| format!("{lib}\n{UNIT_GATE}");
    let at = |import: &str| unit_source(import, BARE);
    // Baseline: the matching import links, so each refusal below differs in
    // exactly the shape it names.
    assert_recovered(
        &root_unit(UNIT_GATE, at("use super::order_discount;")),
        "baseline",
    )?;
    // `super` is the crate root, the entry is in `inner`.
    let sibling = |import: &str| {
        unit_package(
            "src/inner.rs",
            vec![
                ("src/lib.rs", gate("mod inner;")),
                ("src/inner.rs", LIB.to_string()),
                ("src/unit.rs", at(import)),
            ],
        )
    };
    assert_recovered(
        &sibling("use super::inner::order_discount;"),
        "sibling baseline",
    )?;
    assert_refused(&sibling("use super::order_discount;"), "wrong module")?;
    assert_refused(
        &sibling("use self::order_discount;"),
        "self is the test module",
    )?;
    assert_refused(
        &sibling("use super::super::inner::order_discount;"),
        "ascends past the crate root",
    )?;
    assert_refused(
        &sibling("use super::inner::self::order_discount;"),
        "self mid-path",
    )?;
    assert_refused(
        &sibling("use super::inner::super::order_discount;"),
        "super mid-path",
    )?;
    assert_refused(
        &sibling("use super::inner::order_discount as order_discount;"),
        "renamed",
    )?;
    // The entry reached through a module that only re-exports it.
    let reexport = unit_package(
        "src/inner.rs",
        vec![
            (
                "src/lib.rs",
                gate("mod inner;\npub mod api {\n    pub use crate::inner::order_discount;\n}"),
            ),
            ("src/inner.rs", LIB.to_string()),
            ("src/unit.rs", at("use super::api::order_discount;")),
        ],
    );
    assert_refused(&reexport, "re-export module")?;
    // An integration test is not a unit test: `super` is not resolved there.
    let integration = Package::root(LIB, &unit_source("use super::order_discount;", BARE));
    assert_refused(&integration, "integration test")
}

#[test]
fn nearer_bindings_and_scope_changes_keep_a_relative_import_refused() -> Result<(), String> {
    let import = "use super::order_discount;";
    let with_body =
        |body: &str| root_unit(UNIT_GATE, unit_source(import, &format!("{body}\n{BARE}")));
    assert_recovered(&with_body(""), "baseline")?;
    for (label, shadow) in [
        ("let", "    let order_discount = |qty: u32| qty;"),
        ("fn item", "    fn order_discount(qty: u32) -> u32 { qty }"),
        ("tuple constructor", "    struct order_discount(u32);"),
        ("const item", "    const order_discount: u32 = 7;"),
        ("local import", "    use std::cmp::max as order_discount;"),
    ] {
        assert_refused(&with_body(shadow), label)?;
    }
    // A file-level value of the entry's name.
    assert_refused(
        &root_unit(
            UNIT_GATE,
            format!(
                "const order_discount: u32 = 7;\n{}",
                unit_source(import, BARE)
            ),
        ),
        "file const",
    )?;
    // A function-local relative import is not resolved.
    assert_refused(
        &root_unit(
            UNIT_GATE,
            test_file(&format!("    use super::order_discount;\n{BARE}")),
        ),
        "function-local import",
    )?;
    // An inline module moves `super`: here it names the test module.
    assert_refused(
        &root_unit(
            UNIT_GATE,
            format!(
                "mod nested {{\n    use super::order_discount;\n{}\n}}\n",
                test_file(BARE)
            ),
        ),
        "inline module in the test file",
    )?;
    // A glob beside the import may bind or unbind the name.
    assert_refused(
        &root_unit(
            UNIT_GATE,
            format!("use std::collections::*;\n{}", unit_source(import, BARE)),
        ),
        "glob import",
    )?;
    assert_refused(
        &root_unit(
            UNIT_GATE,
            format!("use super::*;\n{}", unit_source(import, BARE)),
        ),
        "super glob",
    )?;
    // A macro in item position or a macro definition may splice items.
    assert_refused(
        &root_unit(
            UNIT_GATE,
            format!(
                "macro_rules! m {{ () => {{}}; }}\nm!();\n{}",
                unit_source(import, BARE)
            ),
        ),
        "item macro",
    )?;
    assert_refused(
        &root_unit(
            UNIT_GATE,
            format!("include!(\"more.rs\");\n{}", unit_source(import, BARE)),
        ),
        "include",
    )
}

#[test]
fn conditional_remapped_and_duplicate_chains_keep_a_relative_import_refused() -> Result<(), String>
{
    let import = "use super::order_discount;";
    let unit = || unit_source(import, BARE);
    assert_recovered(&root_unit(UNIT_GATE, unit()), "baseline")?;
    // The test module declared under another cfg, a compound cfg or a
    // cfg_attr, or beside a #[path].
    for (label, gate) in [
        ("feature cfg", "#[cfg(feature = \"x\")]\nmod unit;\n"),
        ("compound cfg", "#[cfg(all(test, unix))]\nmod unit;\n"),
        ("negated cfg", "#[cfg(not(test))]\nmod unit;\n"),
        ("cfg_attr", "#[cfg_attr(test, doc = \"x\")]\nmod unit;\n"),
        (
            "duplicate",
            "#[cfg(test)]\nmod unit;\n#[cfg(test)]\nmod unit;\n",
        ),
        (
            "path remap of the test module",
            "#[cfg(test)]\n#[path = \"unit.rs\"]\nmod unit;\n",
        ),
        (
            "path remap elsewhere in the package",
            "#[cfg(test)]\nmod unit;\n#[path = \"other.rs\"]\nmod other;\n",
        ),
        ("inline test module", "#[cfg(test)]\nmod unit {}\n"),
    ] {
        assert_refused(&root_unit(gate, unit()), label)?;
    }
    // A cfg-gated import (the name is bound only under the feature).
    assert_refused(
        &root_unit(
            UNIT_GATE,
            format!("#[cfg(feature = \"x\")]\n{}", unit_source(import, BARE)),
        ),
        "cfg-gated use",
    )?;
    // An inner cfg on the test file.
    assert_refused(
        &root_unit(UNIT_GATE, format!("#![cfg(feature = \"x\")]\n{}", unit())),
        "inner cfg",
    )?;
    // The test module reached only through a cfg-gated parent.
    let nested = |cmp_decl: &str, cmp_inner: &str| {
        unit_package(
            "src/cmp.rs",
            vec![
                ("src/lib.rs", cmp_decl.to_string()),
                ("src/cmp.rs", format!("{LIB}\n{UNIT_GATE}{cmp_inner}")),
                ("src/cmp/unit.rs", unit()),
            ],
        )
    };
    assert_recovered(&nested("pub mod cmp;\n", ""), "nested baseline")?;
    assert_refused(
        &nested("#[cfg(feature = \"x\")]\npub mod cmp;\n", ""),
        "cfg-gated entry module",
    )?;
    assert_refused(
        &nested("pub mod cmp;\nmod cmp {}\n", ""),
        "duplicate entry module",
    )?;
    assert_refused(
        &nested("#[path = \"cmp.rs\"]\npub mod cmp;\n", ""),
        "path-remapped entry module",
    )?;
    assert_refused(
        &nested("pub mod cmp;\n", "mod cmp {}\n"),
        "same-named inline module in the file",
    )?;
    // An inline module on the declaration chain of the test module.
    let through_inline =
        |files: Vec<(&'static str, String)>| unit_package("src/outer/cmp.rs", files);
    let outer = |lib: &str| {
        through_inline(vec![
            ("src/lib.rs", lib.to_string()),
            ("src/outer/cmp.rs", format!("{LIB}\n{UNIT_GATE}")),
            ("src/outer/cmp/unit.rs", unit()),
        ])
    };
    assert_refused(
        &outer("pub mod outer {\n    pub mod cmp;\n}\n"),
        "inline module between",
    )?;
    // The test module inside an inline module of the library root: `super`
    // would be the inline module, not the file's directory module.
    let inline_parent = unit_package(
        "src/lib.rs",
        vec![
            (
                "src/lib.rs",
                format!("{LIB}\npub mod outer {{\n    #[cfg(test)]\n    mod unit;\n}}\n"),
            ),
            (
                "src/outer/unit.rs",
                unit_source("use super::super::order_discount;", BARE),
            ),
        ],
    );
    assert_refused(&inline_parent, "test module under an inline module")?;
    // An unrelated module of the same name in another crate of the workspace
    // earns nothing: the test sits in a member crate, not the entry's.
    let member = unit_package(
        "src/cmp.rs",
        vec![
            ("src/lib.rs", "pub mod cmp;\n".to_string()),
            ("src/cmp.rs", LIB.to_string()),
            ("crates/other/Cargo.toml", manifest("other", None)),
            ("crates/other/src/lib.rs", "pub mod cmp;\n".to_string()),
            ("crates/other/src/cmp.rs", UNIT_GATE.to_string()),
            (
                "crates/other/src/cmp/unit.rs",
                unit_source("use super::order_discount;", BARE),
            ),
        ],
    );
    assert_refused(&member, "member crate's same-named module")
}

// ---- #10279: in-file `#[cfg(test)] mod tests { use super::entry; }` ----

/// `src/lib.rs` holding `lib` and then `tests`: the default in-file layout.
fn in_file(lib: &str, tests: &str) -> Package<'static> {
    unit_package(
        "src/lib.rs",
        vec![("src/lib.rs", format!("{lib}\n{tests}"))],
    )
}

/// An inline test module around `inner` (a test or further modules).
fn tests_module(header: &str, inner: &str) -> String {
    format!("#[cfg(test)]\nmod tests {{\n{header}\n{inner}\n}}\n")
}

#[test]
fn an_in_file_test_module_import_resolves_from_its_own_scope() -> Result<(), String> {
    let import = "use super::order_discount;";
    assert_recovered(
        &in_file(LIB, &tests_module(import, &test_file(BARE))),
        "super from mod tests beside the entry",
    )?;
    // The use item's own attribute is not path text.
    assert_recovered(
        &in_file(
            LIB,
            &tests_module(&format!("#[doc(hidden)]\n{import}"), &test_file(BARE)),
        ),
        "attributed use item",
    )?;
    // Two levels of inline module: `super::super` from `tests::deeper`.
    assert_recovered(
        &in_file(
            LIB,
            &tests_module(
                "",
                &format!(
                    "mod deeper {{\n    use super::super::order_discount;\n{}\n}}",
                    test_file(BARE)
                ),
            ),
        ),
        "super::super from a nested inline module",
    )?;
    // The entry inside an inline module, its test module nested beside it.
    let pricing = PUB_INLINE_LIB.trim_end().trim_end_matches('}').to_string();
    assert_recovered(
        &in_file(
            &format!("{pricing}{}}}\n", tests_module(import, &test_file(BARE))),
            "",
        ),
        "super inside an inline entry module",
    )?;
    // The in-file test module of a file module that holds the entry.
    assert_recovered(
        &unit_package(
            "src/cmp.rs",
            vec![
                ("src/lib.rs", "pub mod cmp;\n".to_string()),
                (
                    "src/cmp.rs",
                    format!("{LIB}\n{}", tests_module(import, &test_file(BARE))),
                ),
            ],
        ),
        "super from mod tests in a file module",
    )?;
    // A grouped import resolves like the plain one.
    assert_recovered(
        &in_file(
            LIB,
            &tests_module("use super::{is_bulk, order_discount};", &test_file(BARE)),
        ),
        "grouped import",
    )?;
    // A raw-identifier module name is the same scope.
    assert_recovered(
        &in_file(
            LIB,
            &tests_module("use super::order_discount;", &test_file(BARE))
                .replace("mod tests", "mod r#tests"),
        ),
        "raw module name",
    )?;
    // A one-line module header and test signature: the `fn` token is not the
    // line's first token, so the test is found by name and `fn` line.
    assert_recovered(
        &in_file(
            LIB,
            &format!(
                "#[cfg(test)] mod tests {{ use super::order_discount; #[test] fn discount_boundary() {{\n{BARE}\n}} }}\n"
            ),
        ),
        "one-line module header",
    )?;
    // A raw test name: the test fn lookup compares the name as the producer
    // records it, `r#` included (#10377).
    let raw = tests_module("use super::order_discount;", &test_file(BARE))
        .replace("fn discount_boundary", "fn r#discount_boundary");
    assert!(
        raw.contains("fn r#discount_boundary"),
        "raw test name fixture"
    );
    assert_recovered(&in_file(LIB, &raw), "raw test name")?;
    // Regression guard (no named relative import, so the resolver is not
    // consulted): the glob-only layout links.
    assert_recovered(
        &in_file(LIB, &tests_module("use super::*;", &test_file(BARE))),
        "super glob only",
    )
}

#[test]
fn an_in_file_relative_import_of_another_scope_stays_refused() -> Result<(), String> {
    let baseline = in_file(
        LIB,
        &tests_module("use super::order_discount;", &test_file(BARE)),
    );
    assert_recovered(&baseline, "baseline")?;
    // `super` from `flat::tests` is `flat`, whose own `order_discount` the
    // test calls; the changed helper's entry is the crate root's.
    let flat = format!(
        "pub mod flat {{\n    pub fn order_discount(qty: u32) -> u32 {{ qty / 2 }}\n{}}}\n",
        tests_module("use super::order_discount;", &test_file(BARE))
    );
    assert_refused(&in_file(LIB, &flat), "super names a sibling inline module")?;
    // The same scope re-exporting a foreign function under the entry's name:
    // no second definition of the name exists, so only the scope decides.
    let foreign = format!(
        "pub mod flat {{\n    pub use std::convert::identity as order_discount;\n{}}}\n",
        tests_module(
            "use super::order_discount;",
            &test_file(
                "    assert_eq!(order_discount(10), 10);\n    assert_eq!(order_discount(9), 9);"
            )
        )
    );
    assert_refused(&in_file(LIB, &foreign), "super names a foreign re-export")?;
    // `super::super` from `mod tests` at the crate root goes past it.
    assert_refused(
        &in_file(
            LIB,
            &tests_module("use super::super::order_discount;", &test_file(BARE)),
        ),
        "super past the crate root",
    )?;
    // `self::` inside `mod tests` names the test module itself.
    assert_refused(
        &in_file(
            LIB,
            &tests_module("use self::order_discount;", &test_file(BARE)),
        ),
        "self names the test module",
    )?;
    // Any spelling of the import that resolves elsewhere refuses the file,
    // here beside a foreign re-export so no second definition decides.
    let both = format!(
        "{}pub mod flat {{\n    pub use std::convert::identity as order_discount;\n    mod checks {{\n        use super::order_discount;\n    }}\n}}\n",
        tests_module("use super::order_discount;", &test_file(BARE))
    );
    assert_refused(&in_file(LIB, &both), "a second spelling in another scope")?;
    // The import must share the test's own scope: one in a sibling module
    // does not bind the test's bare call.
    assert_refused(
        &in_file(
            LIB,
            &format!(
                "mod a {{\n    use super::order_discount;\n}}\n{}",
                tests_module("", &test_file(BARE))
            ),
        ),
        "import only in a sibling module",
    )?;
    // The same on one line: the sibling module's header comes first there.
    assert_refused(
        &in_file(
            LIB,
            &format!(
                "mod a {{ use super::order_discount; }} #[cfg(test)] mod tests {{ #[test] fn discount_boundary() {{\n{BARE}\n}} }}\n"
            ),
        ),
        "one-line header, import only in a sibling module",
    )?;
    // A nearer foreign binding in the test's scope beside a resolved import
    // elsewhere (sibling and nested). End-to-end pins: the file-wide renamed-
    // import veto (`test_call_is_shadowed`) decides these, not the scope check.
    let identity =
        test_file("    assert_eq!(order_discount(10), 10);\n    assert_eq!(order_discount(9), 9);");
    assert_refused(
        &in_file(
            LIB,
            &format!(
                "{}mod other {{\n    use std::convert::identity as order_discount;\n{identity}\n}}\n",
                tests_module("use super::order_discount;", "")
            ),
        ),
        "foreign binding in a sibling test module",
    )?;
    assert_refused(
        &in_file(
            LIB,
            &tests_module(
                "use super::order_discount;",
                &format!(
                    "mod deeper {{\n    use std::convert::identity as order_discount;\n{identity}\n}}"
                ),
            ),
        ),
        "foreign binding in a nested test module",
    )?;
    // Accepted conservative loss: `super` names a module that only
    // re-exports the entry.
    assert_refused(
        &in_file(
            LIB,
            &format!(
                "pub mod flat {{\n    pub use crate::order_discount;\n{}}}\n",
                tests_module("use super::order_discount;", &test_file(BARE))
            ),
        ),
        "re-exporting module",
    )?;
    // Accepted conservative loss: a test fn nested in another fn has no
    // module scope this resolver reads.
    assert_refused(
        &in_file(
            LIB,
            &tests_module(
                "use super::order_discount;",
                &format!(
                    "fn outer() {{
{}
}}",
                    test_file(BARE)
                ),
            ),
        ),
        "test fn nested in a fn",
    )?;
    // Guards that predate the in-file scope (glob, cfg-gated use, body
    // binding) still refuse beside the in-file layout.
    assert_refused(
        &in_file(
            LIB,
            &tests_module(
                "use super::*;\nuse super::order_discount;",
                &test_file(BARE),
            ),
        ),
        "glob beside the import",
    )?;
    assert_refused(
        &in_file(
            LIB,
            &tests_module(
                "#[cfg(feature = \"x\")]\nuse super::order_discount;",
                &test_file(BARE),
            ),
        ),
        "cfg-gated use",
    )?;
    assert_refused(
        &in_file(
            LIB,
            &tests_module(
                "use super::order_discount;",
                &test_file(&format!("    let order_discount = |qty: u32| qty;\n{BARE}")),
            ),
        ),
        "nearer let binding",
    )
}
