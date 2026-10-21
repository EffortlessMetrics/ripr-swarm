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
        Ok(finding["related_tests"]
            .as_array()
            .and_then(|related| {
                related
                    .iter()
                    .find(|test| test["name"] == "discount_boundary")
            })
            .and_then(|test| test["relation_reason"].as_str().map(str::to_string)))
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
    assert_eq!(binary(baseline).relation()?.as_deref(), LINKED);
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
