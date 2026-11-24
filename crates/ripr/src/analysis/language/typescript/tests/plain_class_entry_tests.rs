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
            "unexecuted arrow",
            "export function serialize(input: any): string {\n  const never = () => new Serializer().write(input);\n  return \"\";\n}",
        ),
        (
            "deferred callback",
            "export function serialize(input: any): string {\n  setTimeout(() => new Serializer().write(input));\n  return \"\";\n}",
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
            "  constructor() {\n    this.order = (k: string[]) => k;\n  }\n",
        ),
        (
            "accessor",
            "  get order(): any {\n    return (k: string[]) => k;\n  }\n",
        ),
        // A quoted key names the same property at runtime.
        ("quoted field", "  \"order\" = (k: string[]) => k;\n"),
    ] {
        let source = SRC.replacen(
            "  describe(keys: string[]): string {",
            &format!("{member}  describe(keys: string[]): string {{"),
            1,
        );
        assert_ne!(source, SRC, "{label}: fixture not rewritten");
        // A declared member of the same name makes `this.order(...)` a call of
        // anything, so the class is not followed at all. A constructor
        // assignment leaves `write` a node with no edge to `order`.
        let write = entry_hops(&source, "write");
        if label == "constructor assignment" {
            assert_eq!(write, [("serialize".to_string(), 1)], "{label}");
        } else {
            assert!(write.is_empty(), "{label}: write");
        }
        assert!(entry_hops(&source, "order").is_empty(), "{label}: order");
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
fn a_class_whose_this_escapes_has_no_method_node() {
    // `this` other than a plain member access can replace any method on the
    // instance the entry calls, so no method of the class is followed.
    for (label, member) in [
        (
            "assign",
            "  constructor(o?: any) {\n    Object.assign(this, o);\n  }\n",
        ),
        (
            "dynamic key",
            "  constructor() {\n    const k = \"order\";\n    (this as any)[k] = (x: any) => x;\n  }\n",
        ),
        (
            "prototype through constructor",
            "  constructor(o?: any) {\n    Object.assign(this.constructor.prototype, o);\n  }\n",
        ),
        (
            "prototype through __proto__",
            "  constructor(o?: any) {\n    Object.assign(this.__proto__, o);\n  }\n",
        ),
        (
            "instance through valueOf",
            "  constructor(o?: any) {\n    Object.assign(this?.valueOf(), o);\n  }\n",
        ),
        (
            "prototype in a static block",
            "  static {\n    Object.assign(this.prototype, { order: (k: string[]) => k });\n  }\n",
        ),
        (
            "inherited getter definer",
            "  constructor() {\n    this.__defineGetter__(\"order\", () => (k: string[]) => k);\n  }\n",
        ),
        (
            "escaped member name",
            "  constructor(o?: any) {\n    Object.assign(this.co\\u006estructor.prototype, o);\n  }\n",
        ),
        (
            "field called for the receiver",
            "  toString: any = Object.prototype.valueOf;\n  constructor(o?: any) {\n    Object.assign(this.toString(), o);\n  }\n",
        ),
        (
            "inherited call returning the receiver",
            "  constructor(o?: any) {\n    Object.assign(this.toLocaleString(), o);\n  }\n",
        ),
        (
            "private field called for the receiver",
            "  #self: any = Object.prototype.valueOf;\n  constructor(o?: any) {\n    Object.assign(this.#self(), o);\n  }\n",
        ),
        (
            "parenthesized field call",
            "  self: any = Object.prototype.valueOf;\n  constructor(o?: any) {\n    Object.assign((this.self)(), o);\n  }\n",
        ),
        (
            "field call behind a comment",
            "  self: any = Object.prototype.valueOf;\n  constructor(o?: any) {\n    Object.assign(this.self/**/(), o);\n  }\n",
        ),
        (
            "non-null field call",
            "  self: any = Object.prototype.valueOf;\n  constructor(o?: any) {\n    Object.assign(this.self!(), o);\n  }\n",
        ),
        (
            "field call with type arguments",
            "  self: any = Object.prototype.valueOf;\n  constructor(o?: any) {\n    Object.assign(this.self<{ a: 1 }>(), o);\n  }\n",
        ),
        (
            "static method shadowing an inherited name",
            "  static valueOf() {\n    return 0;\n  }\n  constructor(o?: any) {\n    Object.assign(this.valueOf(), o);\n  }\n",
        ),
        (
            "field shadowing a called method",
            "  self(): any {\n    return 0;\n  }\n  self = Object.prototype.valueOf;\n  constructor(o?: any) {\n    Object.assign(this.self(), o);\n  }\n",
        ),
        (
            "getter shadowing a called method",
            "  self(): any {\n    return 0;\n  }\n  get self(): any {\n    return Object.prototype.valueOf;\n  }\n  constructor(o?: any) {\n    Object.assign(this.self(), o);\n  }\n",
        ),
        (
            "prototype of a constructed instance",
            "  p = Object.getPrototypeOf(new Serializer());\n  constructor() {\n    Object.assign(this.p, { order: (k: string[]) => k });\n  }\n",
        ),
        (
            "super",
            "  constructor(o?: any) {\n    Object.assign(super.valueOf(), o);\n  }\n",
        ),
        (
            "new.target",
            "  constructor(o?: any) {\n    Object.assign(new.target.prototype, o);\n  }\n",
        ),
        (
            "alias",
            "  constructor() {\n    const self = this;\n    void self;\n  }\n",
        ),
    ] {
        let source = SRC.replacen(
            "  describe(keys: string[]): string {",
            &format!("{member}  describe(keys: string[]): string {{"),
            1,
        );
        assert_ne!(source, SRC, "{label}: fixture not rewritten");
        assert!(entry_hops(&source, "write").is_empty(), "{label}");
        assert!(entry_hops(&source, "order").is_empty(), "{label}");
        assert!(entry_hops(&source, "compareKeys").is_empty(), "{label}");
    }
    // Bracket and cast writes are refused by the member-write guards even
    // where `this` alone would not be.
    for (label, member) in [
        (
            "literal key",
            "  constructor() {\n    (this as any)[\"order\"] = (k: string[]) => k;\n  }\n",
        ),
        (
            "cast",
            "  constructor() {\n    (this as any).order = (k: string[]) => k;\n  }\n",
        ),
    ] {
        let source = SRC.replacen(
            "  describe(keys: string[]): string {",
            &format!("{member}  describe(keys: string[]): string {{"),
            1,
        );
        assert_ne!(source, SRC, "{label}: fixture not rewritten");
        assert!(entry_hops(&source, "order").is_empty(), "{label}");
    }
    // A private field read, an optional read of a declared field and a field
    // the constructor assigns are plain member accesses.
    let private = SRC.replacen(
        "  describe(keys: string[]): string {",
        "  #n = 0;\n  count(): number {\n    return this.#n + (this?.constructorCount ?? 0) + this.seen.length;\n  }\n  constructorCount = 0;\n  constructor() {\n    this.seen = [];\n  }\n  describe(keys: string[]): string {",
        1,
    );
    assert_eq!(
        entry_hops(&private, "compareKeys"),
        [("serialize".to_string(), 3)]
    );
}

#[test]
fn a_reference_the_module_reassigns_is_no_call() {
    assert_eq!(
        entry_hops(SRC, "compareKeys"),
        [("serialize".to_string(), 3)],
        "twin"
    );
    // A setter replaces the comparator: `keys.sort(compareKeys)` may run the
    // replacement, so the reference is no edge.
    let reassigned = format!(
        "{SRC}export function configure(): void {{\n  compareKeys = (a: string, b: string) => 0;\n}}\n"
    );
    assert!(entry_hops(&reassigned, "compareKeys").is_empty());
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
