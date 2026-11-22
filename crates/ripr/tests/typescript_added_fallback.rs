//! Added `|| d` / `?? d` fallbacks (#15285 case 4).
//!
//! `x` -> `x || d` changes behavior only when `x` is falsy, `x ?? d` only
//! when it is nullish. An exact assertion on any other input passes on both
//! versions, so the finding reads `exposed` only when a related test's call
//! is seen to supply that input. Every case runs `ripr check`, the production
//! path, and every credited case has a withheld twin that differs only in the
//! argument.

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

/// The classification of one finding and its whole JSON text.
struct OwnerFinding {
    class: String,
    text: String,
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
        "ripr-ts-added-fallback-{label}-{}-{}",
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
    Ok(OwnerFinding {
        class,
        text: finding.to_string(),
    })
}

const HEADER: &str =
    "import { expect, test } from 'vitest';\nimport { OWNER } from '../src/lib';\n";

/// The finding for an owner whose `new_line` (line `line` of `owner`) was
/// `old_line`, under one test asserting `expect(<call>).<matcher>`.
fn finding(
    label: &str,
    name: &str,
    owner: &str,
    (line, old_line): (usize, &str),
    assertion: &str,
) -> Result<OwnerFinding, String> {
    let test = format!(
        "{}test('case', () => {{\n  {assertion}\n}});\n",
        HEADER.replace("OWNER", name)
    );
    owner_finding(
        label,
        &[("src/lib.ts", owner), ("tests/lib.test.ts", &test)],
        "src/lib.ts",
        (line, old_line),
        name,
    )
}

/// Checks `(label, assertion, expected class)` rows and reports each miss.
fn assert_rows(
    name: &str,
    owner: &str,
    old: (usize, &str),
    operand: &str,
    rows: &[(&str, &str, &str)],
) -> Result<(), String> {
    let needle = format!("only differs when `{operand}` is");
    let mut misses = Vec::new();
    for (label, assertion, expected) in rows {
        let found = finding(label, name, owner, old, assertion)?;
        if found.class != *expected {
            misses.push(format!("{label}: expected {expected}, got {}", found.class));
        }
        // A withheld row names the input class; a credited row does not.
        if found.text.contains(&needle) != (*expected == "weakly_exposed") {
            misses.push(format!(
                "{label}: input-class limitation is on the wrong side"
            ));
        }
    }
    if misses.is_empty() {
        Ok(())
    } else {
        Err(misses.join("; "))
    }
}

const PARSE: &str = "export function parse(qs: string): Record<string, string> {\n  const out: Record<string, string> = {};\n  for (const pair of qs.split('&')) {\n    const kv = pair.split('=');\n    out[kv[0]] = kv[1] || '';\n  }\n  return out;\n}\n";
const PARSE_OLD: (usize, &str) = (5, "    out[kv[0]] = kv[1];");

#[test]
fn the_issue_shape_reads_weak_for_both_the_populated_and_the_missing_slot_test()
-> Result<(), String> {
    // `kv` is a local derived from the argument, so no call is read as
    // leaving `kv[1]` empty: the populated-pair test (passes on both
    // versions) and the missing-slot test (a real discriminator the analyzer
    // cannot trace) both stay below `exposed`.
    assert_rows(
        "parse",
        PARSE,
        PARSE_OLD,
        "kv[1]",
        &[
            (
                "populated-pair",
                "expect(parse('a=1&b=2')).toEqual({ a: '1', b: '2' });",
                "weakly_exposed",
            ),
            (
                "missing-slot",
                "expect(parse('a')).toEqual({ a: '' });",
                "weakly_exposed",
            ),
        ],
    )
}

#[test]
fn a_removed_fallback_is_the_same_input_class() -> Result<(), String> {
    let owner = PARSE.replace("kv[1] || ''", "kv[1]");
    assert_rows(
        "parse",
        &owner,
        (5, "    out[kv[0]] = kv[1] || '';"),
        "kv[1]",
        &[(
            "removed-populated-pair",
            "expect(parse('a=1&b=2')).toEqual({ a: '1', b: '2' });",
            "weakly_exposed",
        )],
    )
}

fn label_owner(signature: &str, operator: &str) -> String {
    format!("export function label({signature}): string {{\n  return name {operator} 'anon';\n}}\n")
}
const LABEL_OLD: (usize, &str) = (2, "  return name;");

#[test]
fn an_or_fallback_on_a_parameter_needs_a_falsy_or_null_argument() -> Result<(), String> {
    assert_rows(
        "label",
        &label_owner("name?: string | null", "||"),
        LABEL_OLD,
        "name",
        &[
            ("or-empty", "expect(label('')).toBe('anon');", "exposed"),
            ("or-null", "expect(label(null)).toBe('anon');", "exposed"),
            (
                "or-string",
                "expect(label('bob')).toBe('bob');",
                "weakly_exposed",
            ),
            (
                "or-variable",
                "expect(label(who)).toBe('bob');",
                "weakly_exposed",
            ),
            // A parameter default would absorb these on both versions, and
            // the owner's defaults are not recorded.
            (
                "or-omitted",
                "expect(label()).toBe('anon');",
                "weakly_exposed",
            ),
            (
                "or-undefined",
                "expect(label(undefined)).toBe('anon');",
                "weakly_exposed",
            ),
        ],
    )
}

#[test]
fn a_nullish_fallback_on_a_parameter_needs_a_null_argument() -> Result<(), String> {
    assert_rows(
        "label",
        &label_owner("name?: string | null", "??"),
        LABEL_OLD,
        "name",
        &[
            (
                "nullish-null",
                "expect(label(null)).toBe('anon');",
                "exposed",
            ),
            // `''` is not nullish: `name ?? 'anon'` still returns it.
            (
                "nullish-empty",
                "expect(label('')).toBe('');",
                "weakly_exposed",
            ),
            (
                "nullish-string",
                "expect(label('bob')).toBe('bob');",
                "weakly_exposed",
            ),
        ],
    )
}

#[test]
fn a_defaulted_parameter_absorbs_an_omitted_argument() -> Result<(), String> {
    // `label()` returns the default `'x'` on both versions.
    assert_rows(
        "label",
        &label_owner("name: string = 'x'", "||"),
        LABEL_OLD,
        "name",
        &[
            (
                "default-omitted",
                "expect(label()).toBe('x');",
                "weakly_exposed",
            ),
            (
                "default-undefined",
                "expect(label(undefined)).toBe('x');",
                "weakly_exposed",
            ),
            (
                "default-empty",
                "expect(label('')).toBe('anon');",
                "exposed",
            ),
        ],
    )
}

#[test]
fn a_fallback_on_a_field_reads_the_object_literal_argument() -> Result<(), String> {
    let owner = "export function title(opts: { name?: string; id?: number }): string {\n  return opts.name || 'untitled';\n}\n";
    assert_rows(
        "title",
        owner,
        (2, "  return opts.name;"),
        "opts.name",
        &[
            (
                "field-absent",
                "expect(title({})).toBe('untitled');",
                "exposed",
            ),
            (
                "field-other-key",
                "expect(title({ id: 1 })).toBe('untitled');",
                "exposed",
            ),
            (
                "field-empty",
                "expect(title({ name: '' })).toBe('untitled');",
                "exposed",
            ),
            (
                "field-present",
                "expect(title({ name: 'a' })).toBe('a');",
                "weakly_exposed",
            ),
            // A spread may carry the field.
            (
                "field-spread",
                "expect(title({ ...base })).toBe('a');",
                "weakly_exposed",
            ),
        ],
    )
}

#[test]
fn a_fallback_on_a_computed_operand_is_never_read_as_supplied() -> Result<(), String> {
    let owner =
        "export function pick(a: string, b: string): string {\n  return a.trim() || b;\n}\n";
    assert_rows(
        "pick",
        owner,
        (2, "  return a.trim();"),
        "a.trim()",
        &[(
            "computed-operand",
            "expect(pick('', 'z')).toBe('z');",
            "weakly_exposed",
        )],
    )
}

#[test]
fn a_fallback_inside_a_conditional_or_object_literal_is_still_detected() -> Result<(), String> {
    // The default ends at a ternary `:` or a closing brace, and the padding
    // before it stays with the line. A ternary arm is also a predicate
    // probe, which the boundary witness keeps below `exposed` on its own, so
    // only the detection (the named operand) is checked there.
    let arm = "export function label(flag: boolean, name: string): string {\n  return flag ? name || 'anon' : 'other';\n}\n";
    assert_rows(
        "label",
        arm,
        (2, "  return flag ? name : 'other';"),
        "name",
        &[(
            "arm-string",
            "expect(label(true, 'bob')).toBe('bob');",
            "weakly_exposed",
        )],
    )?;
    let object = "export function wrap(name: string): { name: string } {\n  return { name: name || 'anon' };\n}\n";
    assert_rows(
        "wrap",
        object,
        (2, "  return { name: name };"),
        "name",
        &[
            (
                "object-string",
                "expect(wrap('Bob')).toEqual({ name: 'Bob' });",
                "weakly_exposed",
            ),
            (
                "object-empty",
                "expect(wrap('')).toEqual({ name: 'anon' });",
                "exposed",
            ),
        ],
    )
}

#[test]
fn a_ternary_around_the_fallback_is_not_a_fallback_only_edit() -> Result<(), String> {
    // `(name || 'anon') ? 'present' : 'missing'` also differs for a truthy
    // name, so the input-class limitation must not withhold the credit.
    let owner = "export function label(name: string): string {\n  return name || 'anon' ? 'present' : 'missing';\n}\n";
    let found = finding(
        "ternary-wrap",
        "label",
        owner,
        (2, "  return name;"),
        "expect(label('Bob')).toBe('present');",
    )?;
    if found.text.contains("only differs when `name` is") {
        return Err(format!(
            "ternary-wrap: read as a fallback-only edit: {}",
            found.text
        ));
    }
    Ok(())
}

#[test]
fn a_concise_arrow_owner_reads_its_own_parameter() -> Result<(), String> {
    assert_rows(
        "label",
        "export const label = (name: string) => name || 'anon';\n",
        (1, "export const label = (name: string) => name;"),
        "name",
        &[
            ("arrow-empty", "expect(label('')).toBe('anon');", "exposed"),
            (
                "arrow-string",
                "expect(label('bob')).toBe('bob');",
                "weakly_exposed",
            ),
        ],
    )?;
    assert_rows(
        "title",
        "export const title = (opts: { name?: string }): string => opts.name || 'untitled';\n",
        (
            1,
            "export const title = (opts: { name?: string }): string => opts.name;",
        ),
        "opts.name",
        &[
            (
                "arrow-field-absent",
                "expect(title({})).toBe('untitled');",
                "exposed",
            ),
            (
                "arrow-field-present",
                "expect(title({ name: 'a' })).toBe('a');",
                "weakly_exposed",
            ),
        ],
    )
}

#[test]
fn a_nested_callback_parameter_is_not_the_owner_parameter() -> Result<(), String> {
    // The inner `name` shadows the owner's: `label('')` does not reach it.
    let owner = "export function label(name: string, xs: string[]): string[] {\n  return xs.map((name) => name || 'anon');\n}\n";
    assert_rows(
        "label",
        owner,
        (2, "  return xs.map((name) => name);"),
        "(name) => name",
        &[(
            "callback-shadow",
            "expect(label('', ['a'])).toEqual(['a']);",
            "weakly_exposed",
        )],
    )
}

#[test]
fn a_dynamic_subscript_or_reassigned_operand_is_never_read_as_supplied() -> Result<(), String> {
    let dynamic = "export function get(opts: Record<string, string>, key: string): string {\n  return opts[key] || 'none';\n}\n";
    assert_rows(
        "get",
        dynamic,
        (2, "  return opts[key];"),
        "opts[key]",
        &[(
            "dynamic-key",
            "expect(get({ key: '', name: 'a' }, 'name')).toBe('a');",
            "weakly_exposed",
        )],
    )?;
    let reassigned = "export function label(name: string): string {\n  name = name.trim();\n  return name || 'anon';\n}\n";
    assert_rows(
        "label",
        reassigned,
        (3, "  return name;"),
        "name",
        &[(
            "reassigned-empty",
            "expect(label('')).toBe('anon');",
            "weakly_exposed",
        )],
    )
}

#[test]
fn spreads_computed_keys_and_accessors_are_unreadable_literals() -> Result<(), String> {
    let pick =
        "export function pick(parts: string[]): string {\n  return parts[1] || 'default';\n}\n";
    assert_rows(
        "pick",
        pick,
        (2, "  return parts[1];"),
        "parts[1]",
        &[
            (
                "position-short",
                "expect(pick(['a'])).toBe('default');",
                "exposed",
            ),
            (
                "position-empty",
                "expect(pick(['a', ''])).toBe('default');",
                "exposed",
            ),
            (
                "position-after-spread",
                "expect(pick([...['a', 'b'], ''])).toBe('b');",
                "weakly_exposed",
            ),
            (
                "position-commented-spread",
                "expect(pick([/* c */ ...['a', 'b'], ''])).toBe('b');",
                "weakly_exposed",
            ),
        ],
    )?;
    let title = "export function title(opts: { name?: string }): string {\n  return opts.name || 'untitled';\n}\n";
    assert_rows(
        "title",
        title,
        (2, "  return opts.name;"),
        "opts.name",
        &[
            (
                "computed-override",
                "expect(title({ name: '', ['na' + 'me']: 'Bob' })).toBe('Bob');",
                "weakly_exposed",
            ),
            (
                "getter",
                "expect(title({ get name() { return 'Bob'; } })).toBe('Bob');",
                "weakly_exposed",
            ),
        ],
    )
}
