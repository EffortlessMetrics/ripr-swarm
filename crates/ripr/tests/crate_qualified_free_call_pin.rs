//! A private integer owner reached through free direct calls, pinned by an
//! exact assertion on an entry that an integration test names through the
//! package's own library path (`qpin::plus_one(5)`, `qpin::tiers::bumped(3)`)
//! with no `use` (#9638; RIPR-SPEC-0159 free-call entry pin). The qualified
//! spelling is established by the relation's own-crate authority
//! (`own_crate_qualified_entry_path`), not by a second resolver here. Each
//! case writes a small package, changes the owner's returned expression and
//! reads the return-value finding through `check_workspace`. The positives
//! are the crate-root and child-module entry; every shadowed, imported,
//! aliased, macro-defined, ambiguous, trait-dispatched, swallowed or
//! misspelled form is a negative that must not read `exposed`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-qualified-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation: the guard owns the root before anything is
        // written, so a failed step still cleans up.
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

const MANIFEST: &str =
    "[package]\nname = \"qpin\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The crate root: a `tiers` child module, the entry `plus_one` over the
/// owner `plus_one_inner`, and `extra` after them. `owner` is the owner's
/// returned expression.
fn root(owner: &str, extra: &str) -> String {
    format!(
        "pub mod tiers;\n\npub fn plus_one(x: i64) -> i64 {{\n    plus_one_inner(x) + 1\n}}\n\nfn plus_one_inner(x: i64) -> i64 {{\n    {owner}\n}}\n{extra}"
    )
}

/// The child module: the entry `bumped` over the owner `bumped_inner`.
fn tiers(owner: &str) -> String {
    format!(
        "pub fn bumped(x: i64) -> i64 {{\n    bumped_inner(x) + 2\n}}\n\nfn bumped_inner(x: i64) -> i64 {{\n    {owner}\n}}\n"
    )
}

const BEFORE: &str = "x + x";
const AFTER: &str = "x * 3";
const TIERS_BEFORE: &str = "x * 4";
const TIERS_AFTER: &str = "x * 5";

const PIN: &str = "#[test]\nfn plus_one_is_pinned() {\n    assert_eq!(qpin::plus_one(5), 11);\n}\n";
const TIERS_PIN: &str =
    "#[test]\nfn bumped_is_pinned() {\n    assert_eq!(qpin::tiers::bumped(3), 14);\n}\n";

fn unified_diff(path: &str, before: &str, after: &str) -> String {
    let count = |text: &str| text.lines().count();
    let mut body = String::new();
    for (old, new) in before.lines().zip(after.lines()) {
        if old == new {
            body.push_str(&format!(" {old}\n"));
        } else {
            body.push_str(&format!("-{old}\n+{new}\n"));
        }
    }
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n{body}",
        count(before),
        count(after)
    )
}

/// The return-value finding of a package whose `files` hold `(path, before,
/// after)` and whose `tests/qualified.rs` is `tests`.
fn return_finding(files: &[(&str, &str, &str)], tests: &str) -> Result<Value, String> {
    return_finding_in(MANIFEST, files, tests)
}

fn return_finding_in(
    manifest: &str,
    files: &[(&str, &str, &str)],
    tests: &str,
) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    scratch.write("Cargo.toml", manifest)?;
    let mut diff = String::new();
    for (path, before, after) in files {
        scratch.write(path, after)?;
        diff.push_str(&unified_diff(path, before, after));
    }
    scratch.write("tests/qualified.rs", tests)?;
    scratch.write("change.diff", &diff)?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "return_value")
        .cloned()
        .ok_or_else(|| format!("no return_value finding: {json}"))
}

fn class_of(finding: &Value) -> &str {
    finding["classification"].as_str().unwrap_or_default()
}

/// The root entry's finding under `tests`, with `extra` after the library.
fn root_finding(extra: &str, tests: &str) -> Result<Value, String> {
    return_finding(
        &[
            ("src/lib.rs", &root(BEFORE, extra), &root(AFTER, extra)),
            ("src/tiers.rs", &tiers(TIERS_BEFORE), &tiers(TIERS_BEFORE)),
        ],
        tests,
    )
}

fn assert_not_exposed(extra: &str, tests: &str, class: &str) -> Result<(), String> {
    let finding = root_finding(extra, tests)?;
    assert_eq!(class_of(&finding), class, "{tests}\n{finding}");
    Ok(())
}

#[test]
fn crate_qualified_root_and_child_module_entries_are_credited() -> Result<(), String> {
    let root_entry = root_finding("", PIN)?;
    assert_eq!(class_of(&root_entry), "exposed", "{root_entry}");
    let child = return_finding(
        &[
            ("src/lib.rs", &root(BEFORE, ""), &root(BEFORE, "")),
            ("src/tiers.rs", &tiers(TIERS_BEFORE), &tiers(TIERS_AFTER)),
        ],
        TIERS_PIN,
    )?;
    assert_eq!(class_of(&child), "exposed", "{child}");
    Ok(())
}

#[test]
fn crate_qualified_entry_that_swallows_or_reshapes_the_result_is_not_credited() -> Result<(), String>
{
    // The same qualified pin over a tail that drops, rescales or clamps the
    // owner's result must keep the abstention, as the bare spelling does.
    for entry in [
        "let _ = plus_one_inner(x);\n    11",
        "plus_one_inner(x) * 2",
        "plus_one_inner(x).min(11)",
    ] {
        let source = |owner: &str| {
            format!(
                "pub mod tiers;\n\npub fn plus_one(x: i64) -> i64 {{\n    {entry}\n}}\n\nfn plus_one_inner(x: i64) -> i64 {{\n    {owner}\n}}\n"
            )
        };
        let finding = return_finding(&[("src/lib.rs", &source(BEFORE), &source(AFTER))], PIN)?;
        assert_eq!(
            class_of(&finding),
            "propagation_unknown",
            "{entry}\n{finding}"
        );
    }
    Ok(())
}

/// The crate name a test spells is the library target's name: a hyphenated
/// package is spelled with underscores, and a `[lib] name` override replaces
/// the package name. Spelling the other name would not compile, so it must
/// never read `exposed`.
#[test]
fn crate_qualified_entry_follows_the_library_target_name() -> Result<(), String> {
    let owned = [
        ("src/lib.rs", root(BEFORE, ""), root(AFTER, "")),
        ("src/tiers.rs", tiers(TIERS_BEFORE), tiers(TIERS_BEFORE)),
    ];
    let files: Vec<(&str, &str, &str)> = owned
        .iter()
        .map(|(path, before, after)| (*path, before.as_str(), after.as_str()))
        .collect();
    let hyphenated =
        "[package]\nname = \"q-pin\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
    let renamed = format!("{MANIFEST}\n[lib]\nname = \"renamed\"\n");
    for (manifest, spelled, credited) in [
        (hyphenated, "q_pin", true),
        (hyphenated, "q-pin", false),
        (renamed.as_str(), "renamed", true),
        (renamed.as_str(), "qpin", false),
    ] {
        let tests = format!(
            "#[test]\nfn plus_one_is_pinned() {{\n    assert_eq!({spelled}::plus_one(5), 11);\n}}\n"
        );
        let finding = return_finding_in(manifest, &files, &tests)?;
        assert_eq!(
            class_of(&finding) == "exposed",
            credited,
            "{manifest}\n{tests}\n{finding}"
        );
    }
    Ok(())
}

/// A qualified call elsewhere in the test must not credit an assertion that
/// observes a different call: here a same-named local function.
#[test]
fn a_qualified_call_outside_the_assertion_does_not_credit_another_observed_call()
-> Result<(), String> {
    let tests = "fn plus_one(x: i64) -> i64 {\n    x + 6\n}\n\n#[test]\nfn local_is_pinned() {\n    let _ = qpin::plus_one(1);\n    assert_eq!(plus_one(5), 11);\n}\n";
    let finding = root_finding("", tests)?;
    assert_ne!(class_of(&finding), "exposed", "{tests}\n{finding}");
    Ok(())
}

#[test]
fn crate_qualified_entry_with_a_test_that_does_not_pin_it_exactly_is_not_credited()
-> Result<(), String> {
    for (tests, class) in [
        // A projection, a buried call, a weak assertion and a foreign path.
        (
            "#[test]\nfn projected() {\n    assert_eq!(qpin::plus_one(5) % 2, 1);\n}\n",
            "weakly_exposed",
        ),
        (
            "#[test]\nfn buried() {\n    assert_eq!(1 + qpin::plus_one(5), 12);\n}\n",
            "weakly_exposed",
        ),
        (
            "#[test]\nfn weak() {\n    assert!(qpin::plus_one(5) > 0);\n}\n",
            "weakly_exposed",
        ),
        (
            "#[test]\nfn foreign() {\n    assert_eq!(other::qpin::plus_one(5), 11);\n}\n",
            "no_static_path",
        ),
        // The expected value is computed through the entry itself.
        (
            "#[test]\nfn circular() {\n    assert_eq!(qpin::plus_one(5), qpin::plus_one(5));\n}\n",
            "weakly_exposed",
        ),
        // The assertion names a different entry than the one changed.
        (
            "#[test]\nfn other_entry() {\n    assert_eq!(qpin::tiers::bumped(3), 14);\n}\n",
            "no_static_path",
        ),
    ] {
        let finding = root_finding("", tests)?;
        assert_eq!(class_of(&finding), class, "{tests}\n{finding}");
    }
    Ok(())
}

#[test]
fn crate_qualified_entry_with_the_crate_name_rebound_in_the_test_is_not_credited()
-> Result<(), String> {
    for tests in [
        // A local module of the crate's name shadows the extern crate.
        "mod qpin {\n    pub fn plus_one(x: i64) -> i64 {\n        x + 6\n    }\n}\n\n#[test]\nfn local_module() {\n    assert_eq!(qpin::plus_one(5), 11);\n}\n",
        // A rename of the crate, then a call through the alias.
        "use qpin as cp;\n\n#[test]\nfn aliased() {\n    assert_eq!(cp::plus_one(5), 11);\n}\n",
        "extern crate qpin as cp;\n\n#[test]\nfn extern_alias() {\n    assert_eq!(cp::plus_one(5), 11);\n}\n",
        // A glob import can supply a module of the name.
        "mod shim {\n    pub mod qpin {\n        pub fn plus_one(x: i64) -> i64 {\n            x + 6\n        }\n    }\n}\nuse shim::*;\n\n#[test]\nfn glob_shadow() {\n    assert_eq!(qpin::plus_one(5), 11);\n}\n",
        // A macro defined in the test file can define the module.
        "macro_rules! shadow {\n    () => {\n        mod qpin {\n            pub fn plus_one(x: i64) -> i64 {\n                x + 6\n            }\n        }\n    };\n}\nshadow!();\n\n#[test]\nfn macro_shadow() {\n    assert_eq!(qpin::plus_one(5), 11);\n}\n",
        // The same rebinding through a module that only re-exports the real
        // function (no second `plus_one` declaration), so the refusal rests
        // on the binding, never on a duplicated function name.
        "mod helpers {\n    pub mod qpin {\n        pub use ::qpin::plus_one;\n    }\n}\nuse helpers::qpin;\n\n#[test]\nfn imported_module() {\n    assert_eq!(qpin::plus_one(5), 11);\n}\n",
        "mod shim {\n    pub mod qpin {\n        pub use ::qpin::plus_one;\n    }\n}\nuse shim::*;\n\n#[test]\nfn glob_reexport() {\n    assert_eq!(qpin::plus_one(5), 11);\n}\n",
        "macro_rules! reexport {\n    () => {\n        pub mod qpin {\n            pub use ::qpin::plus_one;\n        }\n    };\n}\nreexport!();\n\n#[test]\nfn macro_reexport() {\n    assert_eq!(qpin::plus_one(5), 11);\n}\n",
        // A local binding of the name inside the test body.
        "#[test]\nfn local_binding() {\n    let qpin = 1;\n    let _ = qpin;\n    assert_eq!(qpin::plus_one(5), 11);\n}\n",
    ] {
        let finding = root_finding("", tests)?;
        assert_eq!(class_of(&finding), "no_static_path", "{tests}\n{finding}");
    }
    Ok(())
}

#[test]
fn crate_qualified_entry_that_two_modules_declare_is_not_credited() -> Result<(), String> {
    // `plus_one` is declared at the root and in `tiers`; the name no longer
    // identifies one workspace function, so neither spelling is credited.
    let tiers_with_twin = |owner: &str| {
        format!(
            "{}\npub fn plus_one(x: i64) -> i64 {{\n    x + 100\n}}\n",
            tiers(owner)
        )
    };
    for tests in [
        PIN,
        "#[test]\nfn twin() {\n    assert_eq!(qpin::tiers::plus_one(5), 105);\n}\n",
    ] {
        let finding = return_finding(
            &[
                ("src/lib.rs", &root(BEFORE, ""), &root(AFTER, "")),
                (
                    "src/tiers.rs",
                    &tiers_with_twin(TIERS_BEFORE),
                    &tiers_with_twin(TIERS_BEFORE),
                ),
            ],
            tests,
        )?;
        assert_eq!(class_of(&finding), "no_static_path", "{tests}\n{finding}");
    }
    Ok(())
}

#[test]
fn crate_qualified_entry_spelled_with_the_wrong_module_path_is_not_credited() -> Result<(), String>
{
    // The entry sits at the crate root and in `tiers` respectively; the
    // other module path names no such item.
    assert_not_exposed(
        "",
        "#[test]\nfn wrong_module() {\n    assert_eq!(qpin::tiers::plus_one(5), 11);\n}\n",
        "no_static_path",
    )?;
    let finding = return_finding(
        &[
            ("src/lib.rs", &root(BEFORE, ""), &root(BEFORE, "")),
            ("src/tiers.rs", &tiers(TIERS_BEFORE), &tiers(TIERS_AFTER)),
        ],
        "#[test]\nfn missing_module() {\n    assert_eq!(qpin::bumped(3), 14);\n}\n",
    )?;
    assert_eq!(class_of(&finding), "no_static_path", "{finding}");
    Ok(())
}

#[test]
fn crate_qualified_entry_that_is_a_trait_method_is_not_credited() -> Result<(), String> {
    let extra = "\npub trait Plus {\n    fn plus_one(&self, x: i64) -> i64;\n}\n\npub struct Wrapper;\n\nimpl Plus for Wrapper {\n    fn plus_one(&self, x: i64) -> i64 {\n        plus_one_inner(x) + 1\n    }\n}\n";
    for tests in [
        "use qpin::Plus;\n\n#[test]\nfn through_trait() {\n    assert_eq!(qpin::Wrapper.plus_one(5), 11);\n}\n",
        "#[test]\nfn through_path() {\n    assert_eq!(qpin::Plus::plus_one(&qpin::Wrapper, 5), 11);\n}\n",
    ] {
        let finding = root_finding(extra, tests)?;
        assert_eq!(class_of(&finding), "no_static_path", "{tests}\n{finding}");
    }
    Ok(())
}

#[test]
fn bare_imported_spelling_keeps_its_credit_beside_the_qualified_one() -> Result<(), String> {
    // The baseline the refusals are read against: the `use` spelling of the
    // same pin is the established credit.
    let finding = root_finding(
        "",
        "use qpin::plus_one;\n\n#[test]\nfn imported() {\n    assert_eq!(plus_one(5), 11);\n}\n",
    )?;
    assert_eq!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn crate_qualified_entry_with_cfg_gated_variants_is_not_credited() -> Result<(), String> {
    // Two cfg arms declare the entry; the name no longer identifies one
    // workspace function, so the qualified pin is refused.
    let gated = |owner: &str| {
        format!(
            "pub mod tiers;\n\n#[cfg(feature = \"wide\")]\npub fn plus_one(x: i64) -> i64 {{\n    plus_one_inner(x) + 1\n}}\n\n#[cfg(not(feature = \"wide\"))]\npub fn plus_one(x: i64) -> i64 {{\n    plus_one_inner(x) + 1\n}}\n\nfn plus_one_inner(x: i64) -> i64 {{\n    {owner}\n}}\n"
        )
    };
    let finding = return_finding(&[("src/lib.rs", &gated(BEFORE), &gated(AFTER))], PIN)?;
    assert_eq!(class_of(&finding), "no_static_path", "{finding}");
    Ok(())
}
