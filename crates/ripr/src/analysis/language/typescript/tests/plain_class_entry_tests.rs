//! Methods of a module-private class that an exported entry constructs and
//! calls (`new Serializer().write(input)`), and the functions only those
//! methods call (#15928 rows B1 and B2). The reach is the bounded
//! `static_unknown` entry relation, never `exposed`.

use super::*;

const SRC: &str = "export function serialize(input: any): string {\n  return new Serializer().write(input);\n}\nfunction compareKeys(a: string, b: string): number {\n  return a < b ? -1 : 1;\n}\nclass Serializer {\n  write(record: any): string {\n    return this.order(Object.keys(record)).join(\",\");\n  }\n  order(keys: string[]): string[] {\n    return keys.sort(compareKeys);\n  }\n  describe(keys: string[]): string {\n    return keys.join(\"+\");\n  }\n}\n";
const TEST: &str = "import { it, expect } from 'vitest';\nimport { serialize } from '../src/c';\n\nit('orders keys', () => {\n  expect(serialize({ b: 1, a: 2 })).toBe('a,b');\n});\n";

fn entry_hops(source: &str, owner: &str) -> Vec<(String, u8)> {
    extract_owners(Path::new("src/c.ts"), source)
        .into_iter()
        .find(|candidate| candidate.name == owner)
        .map(|candidate| {
            candidate
                .module_entries
                .into_iter()
                .filter(|entry| entry.class_member.is_none())
                .map(|entry| (entry.name, entry.hops))
                .collect()
        })
        .unwrap_or_default()
}

fn finding_for(label: &str, source: &str, line: usize, owner: &str) -> Result<Finding, String> {
    let text = source
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("{label}: no line {line}"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/c.ts"), source)?;
    ts_write_file(&root.join("tests/c.test.ts"), TEST)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/c.ts", &[(line, text)])],
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
fn fixture_parses_the_intended_owners() {
    let owners = extract_owners(Path::new("src/c.ts"), SRC);
    for expected in ["serialize", "compareKeys", "write", "order", "describe"] {
        assert!(
            owners.iter().any(|owner| owner.name == expected),
            "missing {expected}"
        );
    }
    assert!(owners.iter().any(|owner| {
        owner.name == "write" && owner.class_name.as_deref() == Some("Serializer")
    }));
}

#[test]
fn entry_reaches_the_method_it_calls_on_the_constructed_class() {
    assert_eq!(entry_hops(SRC, "write"), [("serialize".to_string(), 1)]);
}

#[test]
fn entry_reaches_a_sibling_the_method_calls_on_this() {
    assert_eq!(entry_hops(SRC, "order"), [("serialize".to_string(), 2)]);
}

#[test]
fn entry_reaches_a_function_only_a_method_calls() {
    // serialize -> Serializer.write -> Serializer.order -> compareKeys.
    assert_eq!(
        entry_hops(SRC, "compareKeys"),
        [("serialize".to_string(), 3)]
    );
}

#[test]
fn typed_or_spaced_construction_is_read_through() {
    for construction in ["new Serializer<string>()", "new Serializer ()"] {
        let source = SRC.replacen("new Serializer()", construction, 1);
        assert_ne!(source, SRC, "{construction}");
        assert_eq!(
            entry_hops(&source, "write"),
            [("serialize".to_string(), 1)],
            "{construction}"
        );
    }
}

#[test]
fn method_that_alone_reads_a_module_constant_reaches_it() {
    let source = SRC.replacen(
        "    return keys.join(\"+\");",
        "    return keys.join(SEPARATOR[0]);",
        1,
    ) + "const SEPARATOR = [\"+\"];\n";
    // `describe` is off the call path, so its constant is not reached.
    assert!(entry_hops(&source, "SEPARATOR").is_empty());
    let reached = SRC.replacen(
        "    return keys.sort(compareKeys);",
        "    return keys.sort(compareKeys).concat(SEPARATOR[0]);",
        1,
    ) + "const SEPARATOR = [\"+\"];\n";
    assert_eq!(
        entry_hops(&reached, "SEPARATOR"),
        [("serialize".to_string(), 3)]
    );
}

#[test]
fn method_off_the_call_path_gets_no_entry() {
    assert!(entry_hops(SRC, "describe").is_empty());
}

#[test]
fn hop_bound_holds_through_methods() {
    // serialize -> write -> order -> compareKeys -> weigh is four edges.
    let source = SRC.replacen(
        "  return a < b ? -1 : 1;\n}",
        "  return weigh(a) < weigh(b) ? -1 : 1;\n}\nfunction weigh(a: string): number {\n  return a.length;\n}",
        1,
    );
    assert_ne!(source, SRC);
    assert_eq!(
        entry_hops(&source, "compareKeys"),
        [("serialize".to_string(), 3)]
    );
    assert!(entry_hops(&source, "weigh").is_empty());
}

#[test]
fn entry_that_does_not_call_the_method_directly_gets_no_edge() {
    let entry = "export function serialize(input: any): string {\n  return new Serializer().write(input);\n}";
    for (label, replacement) in [
        (
            "parameter",
            "export function serialize(input: any, Serializer: any): string {\n  return new Serializer().write(input);\n}",
        ),
        (
            "local class",
            "export function serialize(input: any): string {\n  class Serializer {\n    write(i: any): string {\n      return \"\";\n    }\n  }\n  return new Serializer().write(input);\n}",
        ),
        (
            "stored instance",
            "export function serialize(input: any): string {\n  const s = new Serializer();\n  return s.write(input);\n}",
        ),
        (
            "method reference",
            "export function serialize(input: any): string {\n  return String(setTimeout(new Serializer().write));\n}",
        ),
        (
            "other namespace",
            "export function serialize(input: any): string {\n  return new input.Serializer().write(input);\n}",
        ),
        (
            "inside a string",
            "export function serialize(input: any): string {\n  return \"new Serializer().write(input)\";\n}",
        ),
    ] {
        let source = SRC.replacen(entry, replacement, 1);
        assert_ne!(source, SRC, "{label}: fixture not rewritten");
        for owner in ["write", "order", "compareKeys"] {
            assert!(entry_hops(&source, owner).is_empty(), "{label}: {owner}");
        }
    }
}

#[test]
fn class_that_may_be_replaced_or_aliased_is_not_followed() {
    for (label, tail) in [
        (
            "prototype write",
            "Serializer.prototype.order = function (k: string[]) {\n  return k;\n};\n",
        ),
        ("alias", "const Alias = Serializer;\n"),
        ("re-export", "export { Serializer };\n"),
        (
            "type position",
            "export function other(s: Serializer): number {\n  return 1;\n}\n",
        ),
        ("subclass", "class Loud extends Serializer {}\n"),
    ] {
        let source = format!("{SRC}{tail}");
        for owner in ["write", "order", "compareKeys"] {
            assert!(entry_hops(&source, owner).is_empty(), "{label}: {owner}");
        }
    }
    let derived = SRC.replacen(
        "class Serializer {",
        "class Base {}\nclass Serializer extends Base {",
        1,
    );
    assert_ne!(derived, SRC);
    assert!(entry_hops(&derived, "write").is_empty(), "extends");
}

#[test]
fn method_an_instance_member_may_replace_is_no_node() {
    for (label, member) in [
        ("field", "  order = (k: string[]) => k;\n"),
        (
            "constructor assignment",
            "  constructor() {\n    (this as any).order = (k: string[]) => k;\n  }\n",
        ),
        (
            "accessor",
            "  get order(): any {\n    return (k: string[]) => k;\n  }\n",
        ),
    ] {
        let source = SRC.replacen(
            "  describe(keys: string[]): string {",
            &format!("{member}  describe(keys: string[]): string {{"),
            1,
        );
        assert_ne!(source, SRC, "{label}: fixture not rewritten");
        // `write` is still called by the entry; what it calls on `this` is not
        // known to be the method.
        assert_eq!(
            entry_hops(&source, "write"),
            [("serialize".to_string(), 1)],
            "{label}"
        );
        assert!(entry_hops(&source, "compareKeys").is_empty(), "{label}");
    }
    let assigned = format!("{SRC}export function patch(o: any): void {{\n  o.order = 1;\n}}\n");
    assert!(entry_hops(&assigned, "order").is_empty(), "member assigned");
    assert!(
        entry_hops(&assigned, "compareKeys").is_empty(),
        "member assigned"
    );
}

#[test]
fn class_whose_constructor_returns_is_not_followed() {
    let source = SRC.replacen(
        "  describe(keys: string[]): string {",
        "  constructor() {\n    return Object.create(null);\n  }\n  describe(keys: string[]): string {",
        1,
    );
    assert_ne!(source, SRC);
    for owner in ["write", "order", "compareKeys"] {
        assert!(entry_hops(&source, owner).is_empty(), "{owner}");
    }
}

#[test]
fn deferred_this_call_adds_no_edge() {
    let source = SRC.replacen(
        "    return this.order(Object.keys(record)).join(\",\");",
        "    setTimeout(() => this.order(Object.keys(record)));\n    return \"\";",
        1,
    );
    assert_ne!(source, SRC);
    assert!(entry_hops(&source, "order").is_empty());
    assert!(entry_hops(&source, "compareKeys").is_empty());
}

#[test]
fn exported_function_of_the_method_name_is_not_a_callee() {
    // `write(record) {` is the method's own header, not a call of a top-level
    // `write`.
    let source = format!("{SRC}function write(x: number): number {{\n  return x;\n}}\n");
    assert_eq!(entry_hops(&source, "write"), [("serialize".to_string(), 1)]);
    let top_level: Vec<TypeScriptOwner> = extract_owners(Path::new("src/c.ts"), &source)
        .into_iter()
        .filter(|owner| owner.name == "write" && owner.class_name.is_none())
        .collect();
    assert_eq!(top_level.len(), 1);
    assert!(top_level[0].module_entries.is_empty());
}

#[test]
fn method_and_function_read_static_unknown_through_the_entry() -> Result<(), String> {
    for (line, owner) in [(5, "compareKeys"), (9, "write"), (12, "order")] {
        let finding = finding_for("plain-class-entry", SRC, line, owner)?;
        assert_eq!(
            finding.class,
            ExposureClass::StaticUnknown,
            "{owner}: {finding:?}"
        );
        assert!(!finding.related_tests.is_empty(), "{owner}");
    }
    Ok(())
}

#[test]
fn method_off_the_call_path_keeps_no_static_path() -> Result<(), String> {
    let finding = finding_for("plain-class-entry-off-path", SRC, 15, "describe")?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty());
    Ok(())
}
