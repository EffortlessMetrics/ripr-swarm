//! Replaced class methods and rebound receivers (#15950, #6980).
//!
//! A method the test file replaces (`c.apply = fake`, a `spyOn`/`stub` naming
//! it) or a receiver the body rebinds (`c = new Other()`) does not run the
//! changed method, so the test must not keep an owner-call relation. Every
//! case runs `ripr check`, the production path, and every withheld case has
//! a credited twin that differs only in the property under test.

#![cfg(feature = "lang-typescript")]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Workspace(PathBuf);

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_file(root: &Path, relative: &str, contents: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    }
    std::fs::write(&path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

/// The classification and related-test count of one finding.
struct OwnerFinding {
    class: String,
    related: usize,
}

/// Writes `files` plus a diff that changes line `line` of `owner_file` from
/// `old` to its current text, runs the check, and returns the finding whose
/// owner ends with `owner_name`.
fn owner_finding(
    label: &str,
    files: &[(&str, &str)],
    owner_file: &str,
    (line, old): (usize, &str),
    owner_name: &str,
) -> Result<OwnerFinding, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-replaced-method-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(|error| format!("mkdir {}: {error}", root.display()))?;
    let workspace = Workspace(root);
    let root = &workspace.0;
    write_file(root, "ripr.toml", "[languages]\nenabled = ['typescript']\n")?;
    let mut owner_source = None;
    for (path, contents) in files {
        write_file(root, path, contents)?;
        if *path == owner_file {
            owner_source = Some(*contents);
        }
    }
    let owner_source = owner_source.ok_or_else(|| format!("{label}: no owner file"))?;
    let lines: Vec<&str> = owner_source.lines().collect();
    let new = lines
        .get(line - 1)
        .ok_or_else(|| format!("{label}: owner file has no line {line}"))?;
    let mut body = String::new();
    for (index, text) in lines.iter().enumerate() {
        if index + 1 == line {
            body.push_str(&format!("-{old}\n+{new}\n"));
        } else {
            body.push_str(&format!(" {text}\n"));
        }
    }
    let count = lines.len();
    write_file(
        root,
        "change.diff",
        &format!(
            "diff --git a/{owner_file} b/{owner_file}\n--- a/{owner_file}\n+++ b/{owner_file}\n@@ -1,{count} +1,{count} @@\n{body}"
        ),
    )?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(["check", "--quiet", "--format", "json", "--root"])
        .arg(root)
        .arg("--diff")
        .arg(root.join("change.diff"))
        .output()
        .map_err(|error| format!("{label}: run ripr: {error}"))?;
    let report: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "{label}: check output is not JSON ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let findings = report["findings"]
        .as_array()
        .ok_or_else(|| format!("{label}: no findings array"))?;
    let finding = findings
        .iter()
        .find(|finding| {
            finding["probe"]["owner"]
                .as_str()
                .is_some_and(|owner| owner.ends_with(owner_name))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `{owner_name}` in {findings:?}"))?;
    let class = finding["classification"]
        .as_str()
        .ok_or_else(|| format!("{label}: finding has no classification"))?
        .to_string();
    let related = finding["related_tests"].as_array().map_or(0, Vec::len);
    Ok(OwnerFinding { class, related })
}

const CART: &str = "export class Cart {\n  total = 0;\n  apply(amount: number, threshold: number): number {\n    if (amount >= threshold) {\n      return amount - 10;\n    }\n    return amount;\n  }\n  static make(amount: number, threshold: number): number {\n    if (amount >= threshold) {\n      return amount - 10;\n    }\n    return amount;\n  }\n}\n";
const OTHER: &str = "export class Other {\n  apply(amount: number, threshold: number): number {\n    return amount;\n  }\n}\n";
const HEADER: &str = "import { describe, it, expect, vi, beforeEach } from 'vitest';\nimport { Cart } from '../src/cart';\nimport { Other } from '../src/other';\n";

#[derive(Clone, Copy)]
enum Owner {
    Apply,
    Make,
}

/// Whether the finding for `owner` keeps any related test, for a test file of
/// `HEADER`, `preamble` (file-level code) and one test with `body`.
fn related(label: &str, owner: Owner, preamble: &str, body: &str) -> Result<bool, String> {
    let test = format!(
        "{HEADER}{preamble}\ndescribe('cart', () => {{\n  it('keeps amount at threshold', () => {{\n{body}\n  }});\n}});\n"
    );
    let (line, name) = match owner {
        Owner::Apply => (4, "apply"),
        Owner::Make => (10, "make"),
    };
    let finding = owner_finding(
        label,
        &[
            ("src/cart.ts", CART),
            ("src/other.ts", OTHER),
            ("tests/cart.test.ts", &test),
        ],
        "src/cart.ts",
        (line, "    if (amount > threshold) {"),
        name,
    )?;
    Ok(finding.class != "no_static_path" && finding.related > 0)
}

/// Checks every `(label, preamble, body)` case and reports each miss.
fn assert_all(owner: Owner, cases: &[(&str, &str, &str)], expected: bool) -> Result<(), String> {
    let mut misses = Vec::new();
    for (label, preamble, body) in cases {
        let got = related(label, owner, preamble, body)?;
        if got != expected {
            misses.push(format!("{label}: expected related={expected}, got {got}"));
        }
    }
    if misses.is_empty() {
        Ok(())
    } else {
        Err(misses.join("; "))
    }
}

const CALL: &str = "    expect(c.apply(100, 100)).toBe(90);";

#[test]
fn an_unreplaced_method_on_a_constructed_receiver_relates() -> Result<(), String> {
    // Controls: each twin of a withheld case below, with the replacement
    // removed, aimed at another member, or the receiver left alone.
    assert_all(
        Owner::Apply,
        &[
            (
                "m-control",
                "",
                &format!("    const c = new Cart();\n{CALL}"),
            ),
            (
                "m-control-let",
                "",
                &format!("    let c: any = new Cart();\n{CALL}"),
            ),
            (
                "m-control-other-member-assigned",
                "",
                &format!("    const c = new Cart();\n    c.total = 5;\n{CALL}"),
            ),
            (
                "m-control-other-member-spied",
                "",
                &format!(
                    "    const c = new Cart();\n    vi.spyOn(c, 'total', 'get').mockReturnValue(1);\n{CALL}"
                ),
            ),
            (
                "m-control-other-receiver-rebound",
                "",
                &format!(
                    "    let o: any = new Other();\n    o = new Other();\n    const c = new Cart();\n{CALL}"
                ),
            ),
            // A block-scoped redeclaration shadows the receiver; the first
            // binding still holds the instance the earlier call ran on.
            (
                "m-control-shadowed-in-block",
                "",
                &format!(
                    "    const c = new Cart();\n{CALL}\n    {{\n      const c = new Other();\n      c.apply(1, 2);\n    }}"
                ),
            ),
            (
                "m-control-inline",
                "",
                "    expect(new Cart().apply(100, 100)).toBe(90);",
            ),
            (
                "m-control-hook-receiver",
                "let c: Cart;\nbeforeEach(() => { c = new Cart(); });\n",
                CALL,
            ),
            (
                "m-control-mock-calls-read",
                "",
                &format!(
                    "    const log = vi.fn();\n    const c = new Cart();\n{CALL}\n    expect(log.mock.calls.length).toBe(0);"
                ),
            ),
        ],
        true,
    )
}

#[test]
fn a_method_replaced_in_the_test_body_withholds() -> Result<(), String> {
    assert_all(
        Owner::Apply,
        &[
            (
                "m-assigned",
                "",
                &format!("    const c = new Cart();\n    c.apply = () => 100;\n{CALL}"),
            ),
            (
                "m-prototype-assigned",
                "",
                &format!(
                    "    Cart.prototype.apply = () => 100;\n    const c = new Cart();\n{CALL}"
                ),
            ),
            (
                "m-prototype-spy",
                "",
                &format!(
                    "    vi.spyOn(Cart.prototype, 'apply').mockReturnValue(100);\n    const c = new Cart();\n{CALL}"
                ),
            ),
            (
                "m-instance-spy",
                "",
                &format!(
                    "    const c = new Cart();\n    vi.spyOn(c, 'apply').mockImplementation(() => 100);\n{CALL}"
                ),
            ),
            (
                "m-define-property",
                "",
                &format!(
                    "    const c = new Cart();\n    Object.defineProperty(c, 'apply', {{ value: () => 100 }});\n{CALL}"
                ),
            ),
            // The `.mock.calls` read sets the file's any-name marker, which
            // hides the file-wide names; the body is still read.
            (
                "m-node-mock-method",
                "",
                &format!(
                    "    const log = vi.fn();\n    const c = new Cart();\n    mock.method(c, 'apply', () => 100);\n{CALL}\n    expect(log.mock.calls.length).toBe(0);"
                ),
            ),
            (
                "m-sinon-stub",
                "",
                &format!(
                    "    const log = vi.fn();\n    const c = new Cart();\n    sinon.stub(c, 'apply').returns(100);\n{CALL}\n    expect(log.mock.calls.length).toBe(0);"
                ),
            ),
        ],
        false,
    )
}

#[test]
fn a_method_replaced_in_a_hook_withholds() -> Result<(), String> {
    assert_all(
        Owner::Apply,
        &[
            (
                "m-hook-prototype-spy",
                "beforeEach(() => { vi.spyOn(Cart.prototype, 'apply').mockReturnValue(100); });\n",
                &format!("    const c = new Cart();\n{CALL}"),
            ),
            (
                "m-hook-assigned",
                "let c: Cart;\nbeforeEach(() => { c = new Cart(); c.apply = () => 100; });\n",
                CALL,
            ),
        ],
        false,
    )
}

#[test]
fn a_receiver_the_body_rebinds_withholds() -> Result<(), String> {
    assert_all(
        Owner::Apply,
        &[
            (
                "m-rebound",
                "",
                &format!("    let c: any = new Cart();\n    c = new Other();\n{CALL}"),
            ),
            (
                "m-rebound-untyped",
                "",
                &format!("    let c = new Cart();\n    c = makeOther();\n{CALL}"),
            ),
        ],
        false,
    )
}

#[test]
fn a_static_method_relates_unless_the_file_replaces_it() -> Result<(), String> {
    const STATIC_CALL: &str = "    expect(Cart.make(100, 100)).toBe(90);";
    assert_all(
        Owner::Make,
        &[
            ("s-control", "", STATIC_CALL),
            (
                "s-control-other-member",
                "",
                &format!("    vi.spyOn(Cart, 'other').mockReturnValue(1);\n{STATIC_CALL}"),
            ),
        ],
        true,
    )?;
    assert_all(
        Owner::Make,
        &[
            (
                "s-assigned",
                "",
                &format!("    Cart.make = () => 100;\n{STATIC_CALL}"),
            ),
            (
                "s-spy",
                "",
                &format!("    vi.spyOn(Cart, 'make').mockReturnValue(100);\n{STATIC_CALL}"),
            ),
            (
                "s-hook-spy",
                "beforeEach(() => { vi.spyOn(Cart, 'make').mockReturnValue(100); });\n",
                STATIC_CALL,
            ),
        ],
        false,
    )
}
