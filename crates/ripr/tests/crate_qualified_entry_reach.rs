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

/// The changed helper is on line 2 of every library source below (line 3 for
/// the inline-module source).
fn diff(path: &str, nested: bool) -> String {
    let (hunk, old, new) = if nested {
        (
            "@@ -2,3 +2,3 @@\n     fn is_bulk(qty: u32) -> bool {",
            "        qty >= 10",
            "        10 <= qty",
        )
    } else {
        (
            "@@ -1,3 +1,3 @@\n fn is_bulk(qty: u32) -> bool {",
            "    qty >= 10",
            "    10 <= qty",
        )
    };
    let close = if nested { "     }" } else { " }" };
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n{hunk}\n-{old}\n+{new}\n{close}\n"
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
    nested: bool,
    files: Vec<(&'a str, String)>,
}

impl<'a> Package<'a> {
    /// The root package with `src/lib.rs` = `lib` and `tests/api.rs` = `tests`.
    fn root(lib: &str, nested: bool, tests: &str) -> Package<'a> {
        Package {
            manifest: manifest("qual_reach", None),
            diff_path: "src/lib.rs",
            nested,
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
        scratch.write("change.diff", &diff(self.diff_path, self.nested))?;
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
                    .find(|test| test["name"] == "discount_boundary")
            })
            .and_then(|test| test["relation_reason"].as_str().map(str::to_string));
        Ok((relation, classification))
    }
}

const LINKED: Option<&str> = Some("helper_owner_call");

fn root_relation(lib: &str, nested: bool, tests: &str) -> Result<Option<String>, String> {
    Package::root(lib, nested, tests).relation()
}

#[test]
fn imported_spelling_is_the_linked_baseline() -> Result<(), String> {
    let tests = format!("use qual_reach::order_discount;\n{}", test_file(BARE));
    assert_eq!(root_relation(LIB, false, &tests)?.as_deref(), LINKED);
    Ok(())
}

#[test]
/// Companion to the baseline: with the `calls_helper_entry` disjunct reverted
/// (production change removed, this file kept) this test fails while the
/// refusals keep passing; that revert run was recorded on #9139.
fn own_crate_qualified_entry_call_links() -> Result<(), String> {
    let tests = test_file(&qualified("qual_reach"));
    assert_eq!(root_relation(LIB, false, &tests)?.as_deref(), LINKED);
    Ok(())
}

#[test]
fn another_crate_qualifier_does_not_link() -> Result<(), String> {
    let tests = test_file(&qualified("other_crate"));
    assert_eq!(root_relation(LIB, false, &tests)?, None);
    Ok(())
}

#[test]
fn a_longer_path_does_not_link() -> Result<(), String> {
    let tests = test_file(&qualified("qual_reach::util"));
    assert_eq!(root_relation(LIB, false, &tests)?, None);
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
        assert_eq!(root_relation(LIB, false, &tests)?, None, "{rebind}");
    }
    // The same crate-name line inside the test body.
    let body = format!(
        "    let qual_reach = other_crate::make();\n{}",
        qualified("qual_reach")
    );
    assert_eq!(root_relation(LIB, false, &test_file(&body))?, None);
    for line in [
        "    use some_dep::qual_reach;",
        "    use inner::order_discount as qual_reach;",
        "    fn qual_reach() {}",
        "    let helper = |qual_reach: u32| qual_reach;",
    ] {
        let body = format!("{line}\n{}", qualified("qual_reach"));
        assert_eq!(
            root_relation(LIB, false, &test_file(&body))?,
            None,
            "{line}"
        );
    }
    // Control: an unrelated import keeps the link.
    let tests = format!(
        "use std::fmt::Debug;\n{}",
        test_file(&qualified("qual_reach"))
    );
    assert_eq!(root_relation(LIB, false, &tests)?.as_deref(), LINKED);
    Ok(())
}

#[test]
fn an_entry_outside_the_crate_root_does_not_link() -> Result<(), String> {
    let baseline = format!("use qual_reach::order_discount;\n{}", test_file(BARE));
    assert_eq!(
        root_relation(NESTED_LIB, true, &baseline)?.as_deref(),
        LINKED,
        "the use spelling must link, or this negative proves nothing"
    );
    let tests = test_file(&qualified("qual_reach"));
    assert_eq!(root_relation(NESTED_LIB, true, &tests)?, None);
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
        ..Package::root("", false, "")
    };
    assert_eq!(baseline.relation()?.as_deref(), LINKED);
    let qualified_unit = Package {
        files: vec![("src/lib.rs", unit(&qualified("qual_reach")))],
        ..Package::root("", false, "")
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
        nested: false,
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
        nested: false,
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
        ..Package::root(LIB, false, &test_file(&qualified("qual_reach")))
    };
    assert_eq!(package.relation()?.as_deref(), LINKED);
    Ok(())
}

#[test]
fn a_lib_name_is_the_only_admitted_spelling() -> Result<(), String> {
    let with_lib = |crate_name: &str| Package {
        manifest: manifest("qual_reach", Some("shared_lib")),
        ..Package::root(LIB, false, &test_file(&qualified(crate_name)))
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
        assert_eq!(root_relation(LIB, false, &test_file(line))?, None, "{line}");
    }
    // Control: a real own-crate call beside a quoted foreign one still links.
    let control =
        "    let r = qual_reach::order_discount(10); let n = \"other::order_discount(10)\";";
    assert_eq!(
        root_relation(LIB, false, &test_file(control))?.as_deref(),
        LINKED
    );
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
        ..Package::root("", false, "")
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
        ..Package::root("", false, "")
    };
    assert_eq!(child_bare.relation()?.as_deref(), LINKED);
    // Control: the same test as a test-crate root links.
    let root = Package::root(LIB, false, &test_file(&qualified("qual_reach")));
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
        nested: false,
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
        nested: true,
        ..Package::root(PUB_INLINE_LIB, true, &qualified_path)
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
        nested: false,
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
        nested: true,
        ..Package::root(nested_lib, true, &tests)
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
    let nested = |lib: &str| Package::root(lib, true, &reexport_tests);
    let valid_reexport = "mod inner {\n    fn is_bulk(qty: u32) -> bool {\n        10 <= qty\n    }\n\n    pub fn order_discount(qty: u32) -> u32 {\n        if is_bulk(qty) { 5 } else { 0 }\n    }\n}\npub use inner::order_discount;\n";
    let control = Package {
        nested: true,
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
        let package = Package {
            nested: true,
            ..nested(&lib)
        };
        assert_eq!(package.relation()?, None, "{extra}{tail}");
        assert_ne!(package.classification()?, "exposed", "{extra}{tail}");
    }
    let private = valid_reexport.replace("pub use inner", "use inner");
    let package = Package {
        nested: true,
        ..nested(&private)
    };
    assert_eq!(package.relation()?, None, "private re-export");
    // A unit test inside `src/` whose module shadows the std path.
    let unit_lib = format!(
        "{LIB}\n#[cfg(test)]\nmod tests {{\n    mod foreign {{\n        pub use std::cmp;\n    }}\n    use self::foreign::cmp::order_discount;\n\n    #[test]\n    fn discount_boundary() {{\n        assert_eq!(order_discount(10), 5);\n        assert_eq!(order_discount(9), 0);\n    }}\n}}\n"
    );
    let package = Package::root(&unit_lib, false, "");
    assert_ne!(package.relation()?.as_deref(), LINKED, "unit shadow");
    assert_ne!(package.classification()?, "exposed", "unit shadow");
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
        nested: false,
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
        "    mod order_discount {}\n    assert_eq!(order_discount(10), 5);",
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

/// A unit test in a separate `src/` file importing through `crate::` goes
/// through the same suffix proof (a `crate::foreign::` std re-export refuses;
/// the genuine `crate::cmp::` import exposes); a binding whose scope holds no
/// call does not suppress a real imported call; an out-of-line module inside
/// an inline ancestor (`pub mod outer { pub mod cmp; }`) still links.
#[test]
fn unit_crate_imports_scoped_bindings_and_inline_ancestors() -> Result<(), String> {
    let unit = |lib: &str, import: &str| Package {
        manifest: manifest("qual_reach", None),
        diff_path: "src/cmp.rs",
        nested: false,
        files: vec![
            ("src/lib.rs", lib.to_string()),
            ("src/cmp.rs", LIB.to_string()),
            (
                "src/unit.rs",
                format!(
                    "{import}\n#[test]\nfn discount_boundary() {{\n    assert_eq!(order_discount(10), 5);\n    assert_eq!(order_discount(9), 0);\n}}\n"
                ),
            ),
            ("tests/api.rs", String::new()),
        ],
    };
    let lib = "pub mod cmp;\npub mod foreign {\n    pub use std::cmp::max as order_discount;\n}\n#[cfg(test)]\nmod unit;\n";
    let genuine = unit(lib, "use crate::cmp::order_discount;");
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
    let foreign = unit(lib, "use crate::foreign::order_discount;");
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
    assert_eq!(
        unused.relation()?.as_deref(),
        LINKED,
        "unused closure param"
    );
    assert_eq!(unused.classification()?, "exposed", "unused closure param");

    let nested = Package {
        manifest: manifest("qual_reach", None),
        diff_path: "src/outer/cmp.rs",
        nested: false,
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
