//! A return-value owner reached through a chain of free direct calls, pinned
//! by an exact assertion on the entry (#9638; RIPR-SPEC-0159 free-call entry
//! pin). Each case writes a small crate, changes the owner's returned
//! expression and reads the return-value finding through `check_workspace`.
//! The positives are the unchanged-result and integer-offset tails; every
//! other tail, type, name binding and test form is a negative that keeps the
//! abstention (`propagation_unknown`) or the reading it had before.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-free-call-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(path.join("tests")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str =
    "[package]\nname = \"free_pin\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The library: the entry `outer` calls the private owner `inner` through
/// `body`; `extra` adds items after them.
fn library(body: &str, owner: &str, extra: &str) -> String {
    format!(
        "pub fn outer(x: i64) -> i64 {{\n    {body}\n}}\n\n{owner}\n{extra}",
        owner = owner
    )
}

const OWNER: &str = "fn inner(x: i64) -> i64 {\n    x + x\n}\n";
const OWNER_CHANGED: &str = "fn inner(x: i64) -> i64 {\n    x * 3\n}\n";
const PIN: &str =
    "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5), 11);\n}\n";

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

/// The return-value finding of a crate whose `src/lib.rs` is `before` on
/// disk-before and `after` on disk-now; `tests` is `tests/it.rs`.
fn return_finding(before: &str, after: &str, tests: &str) -> Result<Value, String> {
    return_finding_in_files(&[("src/lib.rs", before, after)], tests)
}

fn return_finding_in_files(files: &[(&str, &str, &str)], tests: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    let mut diff = String::new();
    for (path, before, after) in files {
        if let Some(parent) = scratch.0.join(path).parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(scratch.0.join(path), after).map_err(|error| error.to_string())?;
        diff.push_str(&unified_diff(path, before, after));
    }
    std::fs::write(scratch.0.join("tests/it.rs"), tests).map_err(|error| error.to_string())?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
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

fn changed(body: &str, extra: &str) -> Result<Value, String> {
    return_finding(
        &library(body, OWNER, extra),
        &library(body, OWNER_CHANGED, extra),
        PIN,
    )
}

fn assert_class(body: &str, extra: &str, class: &str) -> Result<(), String> {
    let finding = changed(body, extra)?;
    assert_eq!(class_of(&finding), class, "{body}: {finding}");
    Ok(())
}

#[test]
fn free_call_entry_that_shifts_the_owner_by_an_integer_literal_is_credited() -> Result<(), String> {
    for body in ["inner(x) + 1", "1 + inner(x)", "inner(x) - 1", "inner(x)"] {
        assert_class(body, "", "exposed")?;
    }
    Ok(())
}

#[test]
fn free_call_entry_chain_through_two_hops_is_credited() -> Result<(), String> {
    let extra = "";
    let before = format!(
        "pub fn outer(x: i64) -> i64 {{\n    middle(x) + 1\n}}\n\nfn middle(x: i64) -> i64 {{\n    inner(x)\n}}\n\n{OWNER}{extra}"
    );
    let after = before.replace(OWNER, OWNER_CHANGED);
    let finding = return_finding(&before, &after, PIN)?;
    assert_eq!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn free_call_entry_that_swallows_or_reshapes_the_result_is_not_credited() -> Result<(), String> {
    for body in [
        "let _ = inner(x);\n    1",
        "inner(x);\n    11",
        "inner(x) * 2",
        "inner(x) / 2",
        "inner(x) % 7",
        "inner(x) & 1",
        "inner(x) + x",
        "inner(x) + inner(x)",
        "-inner(x)",
        "1 - inner(x)",
        "inner(x) + 1 - 1",
        "inner(x) as i64 + 1",
        "inner(x) + 1usize",
        "inner(x) + K",
        "inner(x) + _1",
        "inner(x).min(11)",
        "if x > 100 { 3 } else { inner(x) + 1 }",
        "if x > 100 {\n        return 3;\n    }\n    inner(x) + 1",
    ] {
        assert_class(
            body,
            "const K: i64 = 1;\nconst _1: i64 = 1;\n",
            "propagation_unknown",
        )?;
    }
    Ok(())
}

#[test]
fn free_call_entry_over_a_non_integer_owner_is_not_credited() -> Result<(), String> {
    // A float, a custom type whose `Add<i64>` is not injective, and a
    // workspace type that shadows the integer name keep the abstention.
    let float = |inner_body: &str| {
        format!(
            "pub fn outer(x: f64) -> f64 {{\n    inner(x) + 1\n}}\n\nfn inner(x: f64) -> f64 {{\n    {inner_body}\n}}\n"
        )
    };
    let finding = return_finding(
        &float("x + x"),
        &float("x * 3.0"),
        "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5.0), 11.0);\n}\n",
    )?;
    assert_ne!(class_of(&finding), "exposed", "{finding}");
    let custom = |owner: &str| {
        format!(
            "pub struct Wide(pub i64);\nimpl std::ops::Add<i64> for Wide {{\n    type Output = i64;\n    fn add(self, other: i64) -> i64 {{\n        if other > 0 {{ 11 }} else {{ self.0 }}\n    }}\n}}\npub fn outer(x: i64) -> i64 {{\n    inner(x) + 1\n}}\n\n{owner}"
        )
    };
    let finding = return_finding(
        &custom("fn inner(x: i64) -> Wide {\n    Wide(x + x)\n}\n"),
        &custom("fn inner(x: i64) -> Wide {\n    Wide(x * 3)\n}\n"),
        PIN,
    )?;
    assert_ne!(class_of(&finding), "exposed", "{finding}");
    assert_class(
        "inner(x) + 1",
        "type i64 = Wide;\npub struct Wide;\n",
        "propagation_unknown",
    )?;
    Ok(())
}

#[test]
fn free_call_entry_whose_callee_name_is_bound_elsewhere_is_not_credited() -> Result<(), String> {
    for (body, extra) in [
        // A closure, a nested fn and a parameter of the entry take the name.
        ("let inner = |value: i64| value;\n    inner(x) + 1", ""),
        (
            "fn inner(value: i64) -> i64 {\n        value\n    }\n    inner(x) + 1",
            "",
        ),
        // A second function of the name in another module.
        (
            "inner(x) + 1",
            "mod other {\n    pub fn inner(x: i64) -> i64 {\n        x\n    }\n}\n",
        ),
        // A foreign import of the name.
        ("inner(x) + 1", "use external::inner;\n"),
    ] {
        let finding = changed(body, extra)?;
        assert_ne!(class_of(&finding), "exposed", "{body}: {finding}");
        if body.ends_with("+ 1") {
            assert_ne!(class_of(&finding), "weakly_exposed", "{body}: {finding}");
        }
    }
    let param = |owner: &str| {
        format!(
            "pub fn outer(x: i64, inner: fn(i64) -> i64) -> i64 {{\n    inner(x) + 1\n}}\n\n{owner}"
        )
    };
    let finding = return_finding(
        &param(OWNER),
        &param(OWNER_CHANGED),
        "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5, |v| v + 5), 11);\n}\n",
    )?;
    assert_eq!(class_of(&finding), "propagation_unknown", "{finding}");
    let pure = |owner: &str| {
        format!(
            "pub fn outer(x: i64, inner: fn(i64) -> i64) -> i64 {{\n    inner(x)\n}}\n\n{owner}"
        )
    };
    let finding = return_finding(
        &pure(OWNER),
        &pure(OWNER_CHANGED),
        "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5, |v| v + 5), 10);\n}\n",
    )?;
    assert_ne!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn free_call_entry_resolved_through_a_use_glob_or_another_file_is_not_credited()
-> Result<(), String> {
    // The owner lives in `src/a.rs`; the entry calls the bare name from
    // `src/lib.rs`, where a foreign glob, a foreign import, a workspace
    // import or nothing at all could be what binds it.
    let owner_before = OWNER.replace("fn inner", "pub fn inner");
    let owner_after = OWNER_CHANGED.replace("fn inner", "pub fn inner");
    for head in [
        "use external::*;\n",
        "use external::inner;\n",
        "use a::inner;\n",
        "",
    ] {
        let lib = format!("mod a;\n{head}pub fn outer(x: i64) -> i64 {{\n    inner(x) + 1\n}}\n");
        let finding = return_finding_in_files(
            &[
                ("src/lib.rs", &lib, &lib),
                ("src/a.rs", &owner_before, &owner_after),
            ],
            PIN,
        )?;
        assert_eq!(
            class_of(&finding),
            "propagation_unknown",
            "{head}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn free_call_entry_classes_follow_the_tests_shape() -> Result<(), String> {
    let before = library("inner(x) + 1", OWNER, "");
    let after = library("inner(x) + 1", OWNER_CHANGED, "");
    for (tests, class) in [
        // A weak oracle and an expectation computed through the entry keep
        // reach and read `weakly_exposed` (before the free-call pin they read
        // `propagation_unknown`); the crate-qualified entry call is credited
        // since #9638 (`crate_qualified_free_call_pin`); a rebound entry
        // name, an alias, a foreign glob and an ignored test do not relate.
        (
            "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {\n    assert!(outer(5) > 0);\n}\n",
            "weakly_exposed",
        ),
        (
            "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5), outer(5));\n}\n",
            "weakly_exposed",
        ),
        (
            "#[test]\nfn outer_is_pinned() {\n    assert_eq!(free_pin::outer(5), 11);\n}\n",
            "exposed",
        ),
        (
            "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {\n    let outer = |x: i64| x + 6;\n    assert_eq!(outer(5), 11);\n}\n",
            "no_static_path",
        ),
        (
            "use free_pin::outer as o;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(o(5), 11);\n}\n",
            "no_static_path",
        ),
        (
            "use free_pin::*;\nuse external::*;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5), 11);\n}\n",
            "no_static_path",
        ),
        (
            "use free_pin::outer;\n\n#[test]\n#[ignore]\nfn outer_is_pinned() {\n    assert_eq!(outer(5), 11);\n}\n",
            "reachable_unrevealed",
        ),
    ] {
        let finding = return_finding(&before, &after, tests)?;
        assert_eq!(class_of(&finding), class, "{tests}: {finding}");
    }
    Ok(())
}

#[test]
fn free_call_entry_that_is_a_trait_method_is_not_credited() -> Result<(), String> {
    let lib = |owner: &str| {
        format!(
            "pub trait Shift {{\n    fn outer(&self, x: i64) -> i64;\n}}\npub struct Unit;\nimpl Shift for Unit {{\n    fn outer(&self, x: i64) -> i64 {{\n        inner(x) + 1\n    }}\n}}\n\n{owner}"
        )
    };
    let finding = return_finding(
        &lib(OWNER),
        &lib(OWNER_CHANGED),
        "use free_pin::{Shift, Unit};\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(Unit.outer(5), 11);\n}\n",
    )?;
    assert_ne!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn free_call_entry_whose_callee_is_declared_in_another_scope_is_not_credited() -> Result<(), String>
{
    // The owner is declared in an inline module; the entry outside it names
    // the bare `inner` only through a glob import, so the scopes differ and
    // nothing says which `inner` that is.
    let lib = |owner: &str| {
        format!(
            "mod inside {{\n    {}\n}}\nuse inside::*;\n\npub fn outer(x: i64) -> i64 {{\n    inner(x) + 1\n}}\n",
            owner
                .replace('\n', "\n    ")
                .replace("fn inner", "pub fn inner")
        )
    };
    let finding = return_finding(&lib(OWNER), &lib(OWNER_CHANGED), PIN)?;
    assert_ne!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn free_call_entry_method_beside_a_same_named_associated_fn_is_not_credited() -> Result<(), String>
{
    // Inside an `impl`, a bare `inner(x)` is a free function, never the
    // associated `fn inner` declared beside it.
    let lib = |owner: &str| {
        format!(
            "pub struct Unit;\nimpl Unit {{\n    pub fn outer(x: i64) -> i64 {{\n        inner(x) + 1\n    }}\n    {}\n}}\n",
            owner.replace('\n', "\n    ")
        )
    };
    let finding = return_finding(
        &lib(OWNER),
        &lib(OWNER_CHANGED),
        "use free_pin::Unit;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(Unit::outer(5), 11);\n}\n",
    )?;
    assert_ne!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn free_call_from_a_method_entry_never_binds_an_associated_fn_of_the_same_impl()
-> Result<(), String> {
    // A method entered through a receiver makes a bare `inner(x)` call. The
    // associated `fn inner` in the same `impl` is not what that names.
    let lib = |owner: &str| {
        format!(
            "pub struct Unit;\nimpl Unit {{\n    pub fn new() -> Self {{\n        Unit\n    }}\n    pub fn outer(&self, x: i64) -> i64 {{\n        inner(x) + 1\n    }}\n    {}\n}}\n",
            owner.replace('\n', "\n    ")
        )
    };
    let finding = return_finding(
        &lib(OWNER),
        &lib(OWNER_CHANGED),
        "use free_pin::Unit;\n\n#[test]\nfn outer_is_pinned() {\n    let unit = Unit::new();\n    assert_eq!(unit.outer(5), 11);\n}\n",
    )?;
    assert_ne!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn free_call_entry_whose_callee_is_bound_by_a_parameter_pattern_is_not_credited()
-> Result<(), String> {
    for parameters in [
        "x: i64, (inner, _y): (fn(i64) -> i64, i64)",
        "x: i64, mut inner: fn(i64) -> i64",
        "x: i64, &inner: &fn(i64) -> i64",
        "x: i64, ref inner: fn(i64) -> i64",
        "x: i64, [inner, _]: [fn(i64) -> i64; 2]",
    ] {
        let lib = |owner: &str| {
            format!("pub fn outer({parameters}) -> i64 {{\n    inner(x) + 1\n}}\n\n{owner}")
        };
        let finding = return_finding(
            &lib(OWNER),
            &lib(OWNER_CHANGED),
            "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5, Default::default()), 11);\n}\n",
        )?;
        assert_ne!(class_of(&finding), "exposed", "{parameters}: {finding}");
    }
    Ok(())
}

#[test]
fn free_call_entry_whose_callee_is_rebound_in_a_body_is_not_credited() -> Result<(), String> {
    for body in [
        "const inner: fn(i64) -> i64 = |v| v;\n    inner(x) + 1",
        "static inner: fn(i64) -> i64 = |v| v;\n    inner(x) + 1",
        "let (inner, _) = (|v: i64| v, 0);\n    inner(x) + 1",
        "use other::*;\n    inner(x) + 1",
    ] {
        let finding = changed(body, "")?;
        assert_ne!(class_of(&finding), "exposed", "{body}: {finding}");
    }
    // The same rebinding in a middle hop.
    for rebind in [
        "const inner: fn(i64) -> i64 = |v| v;\n    inner(x)",
        "use other::*;\n    inner(x)",
    ] {
        let lib = |owner: &str| {
            format!(
                "pub fn outer(x: i64) -> i64 {{\n    middle(x) + 1\n}}\n\nfn middle(x: i64) -> i64 {{\n    {rebind}\n}}\n\n{owner}"
            )
        };
        let finding = return_finding(&lib(OWNER), &lib(OWNER_CHANGED), PIN)?;
        assert_ne!(class_of(&finding), "exposed", "{rebind}: {finding}");
    }
    Ok(())
}

#[test]
fn free_call_entry_over_an_integer_name_bound_by_a_use_is_not_credited() -> Result<(), String> {
    for import in [
        "use external::Wide as i64;\n",
        "use external::i64;\n",
        "use external::*;\n",
    ] {
        let finding = return_finding(
            &format!("{import}{}", library("inner(x) + 1", OWNER, "")),
            &format!("{import}{}", library("inner(x) + 1", OWNER_CHANGED, "")),
            PIN,
        )?;
        assert_ne!(class_of(&finding), "exposed", "{import}: {finding}");
    }
    // The counterexample that motivated the import refusal: `Wide` implements
    // `Add<u8>` and always answers `Wide(11)`, so the entry's `i64` is not an
    // integer and the pin on `Wide(11)` says nothing about the owner. The
    // same source with `type i64 = Wide;` is the declared-type control.
    let wide = "#[derive(Debug, PartialEq)]\npub struct Wide(pub i64);\nimpl std::ops::Add<u8> for Wide {\n    type Output = Wide;\n    fn add(self, _other: u8) -> Wide {\n        Wide(11)\n    }\n}\n";
    for alias in ["use Wide as i64;\n", "type i64 = Wide;\n"] {
        let lib = |owner: &str| {
            format!("{wide}{alias}pub fn outer(x: i64) -> i64 {{\n    inner(x) + 1\n}}\n\n{owner}")
        };
        let finding = return_finding(
            &lib("fn inner(x: i64) -> i64 {\n    Wide(x + x)\n}\n"),
            &lib("fn inner(x: i64) -> i64 {\n    Wide(x * 3)\n}\n"),
            "use free_pin::{Wide, outer};\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5), Wide(11));\n}\n",
        )?;
        assert_ne!(class_of(&finding), "exposed", "{alias}: {finding}");
    }
    Ok(())
}

#[test]
fn free_call_entry_offset_literal_follows_rust_integer_lexing() -> Result<(), String> {
    // `1_0` and `1_` are decimal literals; the pinned value moves with the
    // owner by the literal, so only the expected value changes.
    for (literal, pinned, class) in [
        ("1_0", 20, "exposed"),
        ("1_", 11, "exposed"),
        ("_1", 11, "propagation_unknown"),
        ("0x1", 11, "propagation_unknown"),
        ("0b1", 11, "propagation_unknown"),
        ("1u8", 11, "propagation_unknown"),
        ("1_u8", 11, "propagation_unknown"),
        ("1.0", 11, "propagation_unknown"),
        ("1e0", 11, "propagation_unknown"),
    ] {
        let body = format!("inner(x) + {literal}");
        let tests = format!(
            "use free_pin::outer;\n\n#[test]\nfn outer_is_pinned() {{\n    assert_eq!(outer(5), {pinned});\n}}\n"
        );
        let extra = "const _1: i64 = 1;\n";
        let finding = return_finding(
            &library(&body, OWNER, extra),
            &library(&body, OWNER_CHANGED, extra),
            &tests,
        )?;
        assert_eq!(class_of(&finding), class, "{literal}: {finding}");
    }
    Ok(())
}

#[test]
fn free_call_entry_under_a_parent_glob_that_may_import_an_integer_name_is_not_credited()
-> Result<(), String> {
    let wide = "#[derive(Debug, PartialEq)]\npub struct Wide(pub u8);\nimpl std::ops::Add<u8> for Wide {\n    type Output = Wide;\n    fn add(self, _other: u8) -> Wide {\n        Wide(11)\n    }\n}\n";
    let child = |owner_body: &str, glob: &str| {
        format!(
            "{glob}\npub fn outer(x: i64) -> u8 {{\n    inner(x) + 1\n}}\n\nfn inner(x: i64) -> u8 {{\n    {owner_body}\n}}\n"
        )
    };
    let pin = "use free_pin::parent::child::outer;\nuse free_pin::Wide;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5), Wide(11));\n}\n";
    let lib = format!("{wide}pub mod parent;\n");
    let parent = "use crate::Wide as u8;\npub mod child;\n";
    // An out-of-line child whose `use super::*;` brings the parent's rebound
    // `u8` in, and the `self::*` spelling of the same glob.
    for glob in ["use super::*;", "use self::*;"] {
        let finding = return_finding_in_files(
            &[
                ("src/lib.rs", &lib, &lib),
                ("src/parent.rs", parent, parent),
                (
                    "src/parent/child.rs",
                    &child("Wide(x * 2)", glob),
                    &child("Wide(x * 3)", glob),
                ),
            ],
            pin,
        )?;
        assert_ne!(class_of(&finding), "exposed", "{glob}: {finding}");
    }
    // An inline module holding the glob together with the owner and entry.
    let inline = |owner_body: &str| {
        format!(
            "{wide}use Wide as u8;\npub mod parent {{\n    use super::*;\n    pub fn outer(x: i64) -> u8 {{\n        inner(x) + 1\n    }}\n    fn inner(x: i64) -> u8 {{\n        {owner_body}\n    }}\n}}\n"
        )
    };
    let finding = return_finding(
        &inline("Wide(x * 2)"),
        &inline("Wide(x * 3)"),
        "use free_pin::Wide;\nuse free_pin::parent::outer;\n\n#[test]\nfn outer_is_pinned() {\n    assert_eq!(outer(5), Wide(11));\n}\n",
    )?;
    assert_ne!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}
