//! A chain edge that hands the owner a closed integer expression over the
//! caller's own parameter (`inner(x * 10)`), reached by a test that pins the
//! entry on an exact literal (#9638; RIPR-SPEC-0159 computed chain-edge
//! argument). Each case writes a small crate, changes the owner's returned
//! expression and reads the return-value finding through `check_workspace`.
//! The positives fold the argument exactly; every other argument, type, name
//! binding and test input is a negative that keeps the infection abstention.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-edge-fold-{}-{}",
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
    "[package]\nname = \"edge_fold\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

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

/// A test that pins `outer(input)` to `expected`, the value before the change.
fn pin(input: &str, expected: i64) -> String {
    format!(
        "use edge_fold::outer;\n\n#[test]\nfn outer_is_pinned() {{\n    assert_eq!(outer({input}), {expected});\n}}\n"
    )
}

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

fn read(body: &str, extra: &str, tests: &str) -> Result<Value, String> {
    return_finding(
        &library(body, OWNER, extra),
        &library(body, OWNER_CHANGED, extra),
        tests,
    )
}

#[test]
fn edge_argument_over_the_callers_parameter_is_credited() -> Result<(), String> {
    // outer(5): the owner receives 50, 6, 12 and 24; every owner result is
    // 2x, so the pinned value is the doubled argument.
    for (body, expected) in [
        ("inner(x * 10)", 100),
        ("inner(x + 1)", 12),
        ("inner(10 * x)", 100),
        ("inner((x + 1) * 2)", 24),
        ("inner(x - 1)", 8),
        ("inner(x / 5)", 2),
        ("inner(x % 3)", 4),
    ] {
        let finding = read(body, "", &pin("5", expected))?;
        assert_eq!(class_of(&finding), "exposed", "{body}: {finding}");
    }
    Ok(())
}

#[test]
fn edge_argument_two_hops_down_is_credited() -> Result<(), String> {
    let make = |owner: &str| {
        format!(
            "pub fn outer(x: i64) -> i64 {{\n    middle(x * 2)\n}}\n\nfn middle(y: i64) -> i64 {{\n    inner(y + 1)\n}}\n\n{owner}"
        )
    };
    // outer(5): middle(10), inner(11) = 22.
    let finding = return_finding(&make(OWNER), &make(OWNER_CHANGED), &pin("5", 22))?;
    assert_eq!(class_of(&finding), "exposed", "{finding}");
    Ok(())
}

#[test]
fn edge_argument_that_cannot_be_folded_exactly_is_not_credited() -> Result<(), String> {
    for body in [
        // another call, a method, a cast, a suffixed literal, a unary sign
        "inner(x * helper())",
        "inner(x.wrapping_mul(10))",
        "inner((x as i64) * 10)",
        "inner(x * 10i64)",
        "inner(-x)",
        "inner(x.pow(2))",
        // shared state and constants the evaluator does not read here
        "inner(x * K)",
        "inner(x * counter())",
        // overflow, underflow and division by zero on this input
        "inner(x * 1000000000 * 10)",
        "inner(x - 6)",
        "inner(x / (x - 5))",
        // an expression naming no parameter is not this reading
        "inner(3 * 10)",
    ] {
        let finding = read(
            body,
            "const K: i64 = 10;\nfn helper() -> i64 {\n    10\n}\nfn counter() -> i64 {\n    static_value()\n}\nfn static_value() -> i64 {\n    10\n}\n",
            &pin("5", 100),
        )?;
        assert_ne!(class_of(&finding), "exposed", "{body}: {finding}");
    }
    Ok(())
}

#[test]
fn edge_argument_over_a_rebound_or_shadowed_parameter_is_not_credited() -> Result<(), String> {
    for body in [
        "let x = x + 1;\n    inner(x * 10)",
        "let mut x = x;\n    x += 1;\n    inner(x * 10)",
        "let x = helper();\n    inner(x * 10)",
    ] {
        let finding = read(body, "fn helper() -> i64 {\n    7\n}\n", &pin("5", 100))?;
        assert_ne!(class_of(&finding), "exposed", "{body}: {finding}");
    }
    Ok(())
}

#[test]
fn edge_argument_over_a_narrow_or_generic_parameter_is_not_credited() -> Result<(), String> {
    for ty in ["u8", "i16", "u16", "i8", "Wrapped", "T"] {
        let make = |owner: &str| {
            format!(
                "type Wrapped = u8;\npub fn outer<T: Into<i64>>(x: {ty}) -> {ty} {{\n    inner(x * 2)\n}}\n\n{}",
                owner.replace("i64", ty)
            )
        };
        let finding = return_finding(&make(OWNER), &make(OWNER_CHANGED), &pin("5", 20));
        // The generic form may not compile into a finding at all; a finding
        // that does exist must not be credited.
        if let Ok(finding) = finding {
            assert_ne!(class_of(&finding), "exposed", "{ty}: {finding}");
        }
    }
    Ok(())
}

/// The folded edge never reads more than the same chain with the argument
/// passed straight through: where the return-value reading already ignores
/// the input (a non-literal test input, or an input on which the owners
/// agree), the computed edge keeps that reading and adds no credit of its own.
#[test]
fn edge_argument_adds_no_credit_beyond_the_straight_through_edge() -> Result<(), String> {
    let non_literal = |expression: &str| {
        format!(
            "use edge_fold::outer;\n\n#[test]\nfn outer_is_pinned() {{\n    let n = {expression};\n    assert_eq!(outer(n), 100);\n}}\n"
        )
    };
    for tests in [
        non_literal("std::env::args().count() as i64"),
        non_literal("5 + std::env::args().count() as i64"),
        pin("0", 0),
    ] {
        let direct = read("inner(x)", "", &tests)?;
        let folded = read("inner(x * 10)", "", &tests)?;
        let rank = |class: &str| usize::from(class == "exposed");
        assert!(
            rank(class_of(&folded)) <= rank(class_of(&direct)),
            "{tests}: {folded} vs {direct}"
        );
    }
    Ok(())
}

#[test]
fn edge_argument_with_two_owner_calls_is_not_credited() -> Result<(), String> {
    for body in [
        "inner(x * 10) + inner(x * 20)",
        "inner(x * 10);\n    inner(x * 20)",
    ] {
        let finding = read(body, "", &pin("5", 100))?;
        assert_ne!(class_of(&finding), "exposed", "{body}: {finding}");
    }
    Ok(())
}

#[test]
fn edge_argument_built_by_a_macro_is_not_credited() -> Result<(), String> {
    let extra = "macro_rules! tenfold {\n    ($e:expr) => {\n        $e * 10\n    };\n}\n";
    for body in ["inner(tenfold!(x))", "inner(x * line!() as i64)"] {
        let finding = read(body, extra, &pin("5", 100))?;
        assert_ne!(class_of(&finding), "exposed", "{body}: {finding}");
    }
    Ok(())
}
