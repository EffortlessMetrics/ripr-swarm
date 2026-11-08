//! A module-level non-function `const` (regex, symbol, lookup object) reached
//! through a same-module function a test calls (#13592 cause B2). The edge
//! exists only for a provably read-only, unshadowed use; the verdict is the
//! bounded `static_unknown` entry reach, never `exposed`.

use super::*;

const SRC: &str = "const SLASH_RE = /%252F/g;\nexport function decodeThing(text: string): string {\n  return decodeURIComponent(text.replace(SLASH_RE, \"%2F\"));\n}\n";
const TEST: &str = "import { it, expect } from 'vitest';\nimport { decodeThing } from '../src/c';\n\nit('decodes', () => {\n  expect(decodeThing('a%252Fb')).toBe('a%2Fb');\n});\n";

fn entry_names(source: &str, owner: &str) -> Vec<String> {
    extract_owners(Path::new("src/c.ts"), source)
        .into_iter()
        .find(|candidate| candidate.name == owner)
        .map(|candidate| {
            candidate
                .module_entries
                .into_iter()
                .map(|entry| entry.name)
                .collect()
        })
        .unwrap_or_default()
}

fn value_finding(label: &str, source: &str, test: &str, owner: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/c.ts"), source)?;
    ts_write_file(&root.join("tests/c.test.ts"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(
            "src/c.ts",
            &[(1, "const SLASH_RE = /%252X/g;")],
        )],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let suffix = format!("::{owner}");
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|candidate| candidate.0.ends_with(&suffix))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `{owner}`"))
}

#[test]
fn read_only_const_is_reached_through_the_exported_reader() {
    assert_eq!(entry_names(SRC, "SLASH_RE"), ["decodeThing"]);
}

#[test]
fn reader_call_forms_are_the_allowlist() {
    for (label, body) in [
        ("test", "return SLASH_RE.test(text) ? \"y\" : \"n\";"),
        ("exec", "return String(SLASH_RE.exec(text));"),
        ("match", "return String(text.match(SLASH_RE));"),
        ("split", "return text.split(SLASH_RE).join(\"\");"),
        ("source", "return SLASH_RE.source;"),
    ] {
        let source = format!(
            "const SLASH_RE = /x/g;\nexport function f(text: string): string {{\n  {body}\n}}\n"
        );
        assert_eq!(entry_names(&source, "SLASH_RE"), ["f"], "{label}");
    }
    let symbol = "const KEY = Symbol(\"k\");\nexport function f(o: any) {\n  return o[KEY];\n}\n";
    assert_eq!(entry_names(symbol, "KEY"), ["f"]);
    let table = "const TABLE = { a: 1 };\nexport function f(k: string) {\n  return TABLE[k];\n}\n";
    assert_eq!(entry_names(table, "TABLE"), ["f"]);
}

#[test]
fn reach_follows_same_module_helpers_within_the_hop_bound() {
    let source = "const SLASH_RE = /x/g;\nfunction clean(t: string) {\n  return t.replace(SLASH_RE, \"\");\n}\nexport function f(t: string) {\n  return clean(t);\n}\n";
    assert_eq!(entry_names(source, "SLASH_RE"), ["f"]);
}

#[test]
fn shadowed_name_gets_no_edge() {
    // A parameter, a local and a nested-callback parameter of the same name.
    for (label, body) in [
        (
            "param",
            "export function f(SLASH_RE: RegExp, t: string) {\n  return t.replace(SLASH_RE, \"\");\n}\n",
        ),
        (
            "local",
            "export function f(t: string) {\n  const SLASH_RE = /y/;\n  return t.replace(SLASH_RE, \"\");\n}\n",
        ),
        (
            "callback",
            "export function f(t: string[]) {\n  return t.map((SLASH_RE) => SLASH_RE.length);\n}\n",
        ),
        (
            "mixed",
            "export function f(t: string) {\n  const SLASH_RE = /y/;\n  return t.replace(SLASH_RE, \"\") + /x/g.source;\n}\n",
        ),
    ] {
        let source = format!("const SLASH_RE = /x/g;\n{body}");
        assert!(entry_names(&source, "SLASH_RE").is_empty(), "{label}");
    }
}

#[test]
fn non_read_mentions_get_no_edge() {
    for (label, body) in [
        ("passed", "return Object.freeze(SLASH_RE) && t;"),
        ("returned", "return SLASH_RE;"),
        ("spread", "return [...SLASH_RE];"),
        ("shorthand", "return { SLASH_RE };"),
        ("alias", "const r = SLASH_RE;\n  return r.test(t);"),
        (
            "assigned",
            "SLASH_RE.lastIndex = 0;\n  return SLASH_RE.test(t);",
        ),
        ("mutating call", "SLASH_RE.compile(\"y\");\n  return t;"),
        ("callee arg", "return t.replace(SLASH_RE, SLASH_RE);"),
    ] {
        let source =
            format!("const SLASH_RE = /x/g;\nexport function f(t: string) {{\n  {body}\n}}\n");
        assert!(entry_names(&source, "SLASH_RE").is_empty(), "{label}");
    }
}

#[test]
fn reassigned_or_mutated_binding_gets_no_edge() {
    // `let`/`var` bindings are not followed at all.
    for keyword in ["let", "var"] {
        let source = format!(
            "{keyword} SLASH_RE = /x/g;\nexport function f(t: string) {{\n  return t.replace(SLASH_RE, \"\");\n}}\n"
        );
        assert!(entry_names(&source, "SLASH_RE").is_empty(), "{keyword}");
    }
    // A const object passed as an argument and then mutated elsewhere in the
    // module disables the edge, even though the reader only reads it.
    let mutated = "const TABLE = { a: 1 };\nexport function f(k: string) {\n  return TABLE[k];\n}\nexport function g() {\n  TABLE.a = 2;\n}\n";
    assert!(entry_names(mutated, "TABLE").is_empty());
    let pushed =
        "const LIST = [1];\nexport function f() {\n  return LIST.includes(1);\n}\nLIST.push(2);\n";
    assert!(entry_names(pushed, "LIST").is_empty());
    let assigned = "const OBJ = { a: 1 };\nexport function f() {\n  return OBJ.a;\n}\nObject.assign(OBJ, { a: 2 });\n";
    assert!(entry_names(assigned, "OBJ").is_empty());
    let updated = "const BOX = { n: 1 };\nexport function f() {\n  return BOX.n;\n}\nBOX.n++;\n";
    assert!(entry_names(updated, "BOX").is_empty());
}

#[test]
fn unused_const_and_a_callable_const_get_no_edge() {
    let unused = "const SLASH_RE = /x/g;\nexport function f(t: string) {\n  return t;\n}\n";
    assert!(entry_names(unused, "SLASH_RE").is_empty());
    // A comment is not a read.
    let comment = "const SLASH_RE = /x/g;\nexport function f(t: string) {\n  // SLASH_RE.test(t)\n  return t;\n}\n";
    assert!(entry_names(comment, "SLASH_RE").is_empty());
    // A member of another receiver is not the binding.
    let member = "const SLASH_RE = /x/g;\nexport function f(o: any) {\n  return o.SLASH_RE.test(\"x\");\n}\n";
    assert!(entry_names(member, "SLASH_RE").is_empty());
}

#[test]
fn factory_const_keeps_its_previous_no_entry_reading() {
    // A factory product is a function node, not a value: bare-call reach to it
    // must not start attaching entries to its `ModuleFunction` owner.
    let source = "function build() {\n  return (a: number) => a;\n}\nconst made = build();\nexport function f() {\n  return made(1);\n}\n";
    assert!(entry_names(source, "made").is_empty());
}

#[test]
fn called_reader_reads_static_unknown_never_exposed() -> Result<(), String> {
    let finding = value_finding("b2-called", SRC, TEST, "SLASH_RE")?;
    assert!(
        finding.related_tests.iter().any(
            |test| test.relation_reason == Some(crate::domain::RelationReason::HelperOwnerCall)
        ),
        "related: {:?}",
        finding.related_tests
    );
    assert_ne!(finding.class, ExposureClass::NoStaticPath);
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    Ok(())
}

#[test]
fn uncalled_reader_stays_no_static_path() -> Result<(), String> {
    // The reader is exported but no test calls it.
    let finding = value_finding(
        "b2-uncalled",
        SRC,
        "import { it, expect } from 'vitest';\nimport { other } from '../src/other';\n\nit('x', () => {\n  expect(other()).toBe(1);\n});\n",
        "SLASH_RE",
    )?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert!(
        finding.related_tests.is_empty(),
        "{:?}",
        finding.related_tests
    );
    Ok(())
}

#[test]
fn shadowed_reader_stays_no_static_path() -> Result<(), String> {
    let shadowed = "const SLASH_RE = /%252F/g;\nexport function decodeThing(text: string, SLASH_RE: RegExp): string {\n  return text.replace(SLASH_RE, \"%2F\");\n}\n";
    let finding = value_finding("b2-shadowed", shadowed, TEST, "SLASH_RE")?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn destructuring_pattern_is_not_an_index_access() {
    for (label, head) in [
        ("const", "const[SLASH_RE] = arr;"),
        ("let", "let[SLASH_RE] = arr;"),
        ("for-of", "for (const[SLASH_RE] of arr) {}"),
    ] {
        let source = format!(
            "const SLASH_RE = /x/g;\nconst arr = [/y/];\nexport function f(t: string) {{\n  {head}\n  return SLASH_RE.test(t);\n}}\n"
        );
        assert!(entry_names(&source, "SLASH_RE").is_empty(), "{label}");
    }
}

#[test]
fn destructuring_and_reflect_writes_get_no_edge() {
    for (label, tail) in [
        ("array target", "[TABLE.a] = [2];"),
        ("object target", "({ k: TABLE.a } = { k: 2 });"),
        ("for-of target", "for (TABLE.a of [1, 2]) {}"),
        ("reflect delete", "Reflect.deleteProperty(TABLE, \"a\");"),
        ("reflect set", "Reflect.set(TABLE, \"a\", 2);"),
        (
            "reflect define",
            "Reflect.defineProperty(TABLE, \"a\", {});",
        ),
        ("object define", "Object.defineProperty(TABLE, \"a\", {});"),
    ] {
        let source = format!(
            "const TABLE = {{ a: 1 }};\nexport function f(k: string) {{\n  return TABLE[k];\n}}\nexport function g() {{\n  {tail}\n}}\n"
        );
        assert!(entry_names(&source, "TABLE").is_empty(), "{label}");
    }
}

#[test]
fn escaping_aliasing_or_exported_value_gets_no_edge() {
    let reader = "export function f(k: string) {\n  return TABLE[k];\n}\n";
    for (label, extra) in [
        ("module alias", "const alias = TABLE;\nalias.a = 2;\n"),
        ("passed to unknown", "mutate(TABLE);\n"),
        ("call chain write", "TABLE.get(\"x\").prop = 1;\n"),
        ("re-export", "export { TABLE as T };\n"),
    ] {
        let source = format!("const TABLE = {{ a: 1 }};\n{reader}{extra}");
        assert!(entry_names(&source, "TABLE").is_empty(), "{label}");
    }
    let exported =
        "export const TABLE = { a: 1 };\nexport function f(k: string) {\n  return TABLE[k];\n}\n";
    assert_eq!(entry_names(exported, "TABLE"), Vec::<String>::new());
}

#[test]
fn weak_oracle_through_same_module_caller_names_the_caller_not_an_exported_value()
-> Result<(), String> {
    let weak = TEST.replace(".toBe('a%2Fb')", ".toBeDefined()");
    let finding = value_finding("b2-weak", SRC, &weak, "SLASH_RE")?;
    let step = finding.recommended_next_step.clone().unwrap_or_default();
    assert!(step.contains("only through a same-module caller"), "{step}");
    assert!(!step.contains("exported value"), "{step}");
    Ok(())
}
