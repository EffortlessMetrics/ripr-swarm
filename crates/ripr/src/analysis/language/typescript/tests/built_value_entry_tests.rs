//! A module-private class wrapped in an immediately invoked function and
//! constructed by an exported entry, and a function handed by reference to a
//! synchronous array callback method (#15928). Both are the bounded
//! `static_unknown` entry reach, never `exposed`.

use super::*;

/// The unjs/ohash `serialize` shape.
const SRC: &str = "export function serialize(input: any): string {\n  return new Serializer().serialize(input);\n}\nfunction compareStrings(a: string, b: string): number {\n  return a < b ? -1 : 1;\n}\nconst Serializer = /*@__PURE__*/ (function () {\n  class Serializer {\n    compare(a: any, b: any): number {\n      return compareStrings(a, b);\n    }\n    serialize(value: any): string {\n      return Object.keys(value).sort(compareStrings).join(\",\");\n    }\n  }\n  Serializer.prototype.extra = function () {\n    return 1;\n  };\n  return Serializer;\n})();\n";
const TEST: &str = "import { it, expect } from 'vitest';\nimport { serialize } from '../src/c';\n\nit('orders keys', () => {\n  expect(serialize({ b: 1, a: 2 })).toBe('a,b');\n});\n";

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

fn entry_hops(source: &str, owner: &str) -> Vec<(String, u8)> {
    extract_owners(Path::new("src/c.ts"), source)
        .into_iter()
        .find(|candidate| candidate.name == owner)
        .map(|candidate| {
            candidate
                .module_entries
                .into_iter()
                .map(|entry| (entry.name, entry.hops))
                .collect()
        })
        .unwrap_or_default()
}

fn finding_for(
    label: &str,
    source: &str,
    test: &str,
    changed: (usize, &str),
    owner: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/c.ts"), source)?;
    ts_write_file(&root.join("tests/c.test.ts"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/c.ts", &[changed])],
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
    let names: Vec<String> = extract_owners(Path::new("src/c.ts"), SRC)
        .into_iter()
        .map(|owner| owner.name)
        .collect();
    for expected in ["serialize", "compareStrings", "Serializer"] {
        assert!(names.iter().any(|name| name == expected), "{names:?}");
    }
}

#[test]
fn constructing_entry_reaches_the_built_class() {
    assert_eq!(
        entry_hops(SRC, "Serializer"),
        [("serialize".to_string(), 1)]
    );
}

#[test]
fn built_class_reaches_the_functions_its_code_calls() {
    // serialize -> Serializer (constructed) -> compareStrings (bare call and
    // `.sort(compareStrings)`).
    assert_eq!(
        entry_hops(SRC, "compareStrings"),
        [("serialize".to_string(), 2)]
    );
}

#[test]
fn arrow_built_value_is_followed_too() {
    let source = "function weigh(c: string) {\n  return c.length;\n}\nconst WEIGHTS = (() => {\n  const out = [];\n  out[0] = weigh(\"a\");\n  return out;\n})();\nexport function f(i: number) {\n  return WEIGHTS[i];\n}\n";
    assert_eq!(entry_names(source, "WEIGHTS"), ["f"]);
    assert_eq!(entry_names(source, "weigh"), ["f"]);
}

#[test]
fn construction_that_is_not_the_only_mention_gets_no_edge() {
    for (label, entry) in [
        (
            "parameter",
            "export function serialize(input: any, Serializer: any): string {\n  return new Serializer().serialize(input);\n}",
        ),
        (
            "local",
            "export function serialize(input: any): string {\n  const Serializer = Other;\n  return new Serializer().serialize(input);\n}",
        ),
        (
            "passed along",
            "export function serialize(input: any): string {\n  register(Serializer);\n  return new Serializer().serialize(input);\n}",
        ),
        (
            "not constructed",
            "export function serialize(input: any): string {\n  return String(input);\n}",
        ),
    ] {
        let source = SRC.replacen(
            "export function serialize(input: any): string {\n  return new Serializer().serialize(input);\n}",
            entry,
            1,
        );
        assert_ne!(source, SRC, "{label}: fixture not rewritten");
        assert!(entry_names(&source, "Serializer").is_empty(), "{label}");
        assert!(entry_names(&source, "compareStrings").is_empty(), "{label}");
    }
}

#[test]
fn module_level_write_to_the_built_value_disables_it() {
    // Outside the building function the name is the module binding.
    for (label, tail) in [
        ("prototype write", "Serializer.prototype.late = () => 2;\n"),
        ("alias", "const Alias = Serializer;\n"),
        ("re-export", "export { Serializer };\n"),
    ] {
        let source = format!("{SRC}{tail}");
        assert!(entry_names(&source, "Serializer").is_empty(), "{label}");
        assert!(entry_names(&source, "compareStrings").is_empty(), "{label}");
    }
}

#[test]
fn building_function_that_does_not_declare_the_name_keeps_the_module_scan() {
    // No inner `class Serializer`: the write inside the function is a write
    // to the module binding once the function is called later.
    let source = "export function make(): any {\n  return new Box().get();\n}\nconst Box = (function () {\n  class Inner {\n    get() {\n      return 1;\n    }\n  }\n  setTimeout(() => {\n    Box.prototype.get = () => 2;\n  });\n  return Inner;\n})();\n";
    assert!(entry_names(source, "Box").is_empty());
}

#[test]
fn member_declared_under_a_function_name_is_not_a_call_of_it() {
    // `compare(a, b) {` declares a method; only `this.compare` runs it.
    let source = "export function serialize(input: any): string {\n  return new Serializer().run(input);\n}\nfunction compare(a: string, b: string): number {\n  return a < b ? -1 : 1;\n}\nconst Serializer = (function () {\n  class Serializer {\n    compare(a: any, b: any) {\n      return 0;\n    }\n    run(value: any) {\n      return this.compare(value, value);\n    }\n  }\n  return Serializer;\n})();\n";
    assert_eq!(entry_names(source, "Serializer"), ["serialize"]);
    assert!(entry_names(source, "compare").is_empty());
}

#[test]
fn function_reference_to_a_synchronous_array_callback_is_a_call() {
    for method in ["sort", "map", "filter", "forEach", "some", "every", "find"] {
        let source = format!(
            "function cmp(a: any, b: any) {{\n  return a < b ? -1 : 1;\n}}\nexport function order(keys: any[]) {{\n  return keys.{method}(cmp);\n}}\n"
        );
        assert_eq!(entry_names(&source, "cmp"), ["order"], "{method}");
    }
    let with_initial = "function add(a: number, b: number) {\n  return a + b;\n}\nexport function sum(values: number[]) {\n  return values.reduce(add, 0);\n}\n";
    assert_eq!(entry_names(with_initial, "add"), ["sum"]);
}

#[test]
fn function_reference_in_a_comment_or_literal_is_no_call() {
    for (label, body) in [
        ("trailing comment", "return keys; // keys.sort(cmp)"),
        ("block comment", "return keys; /* keys.sort(cmp) */"),
        ("string", "return [\"keys.sort(cmp)\"];"),
    ] {
        let source = format!(
            "function cmp(a: any, b: any) {{\n  return a < b ? -1 : 1;\n}}\nexport function order(keys: any[]) {{\n  {body}\n}}\n"
        );
        assert!(entry_names(&source, "cmp").is_empty(), "{label}");
    }
    // The live control: the same line with the call as code.
    let live = "function cmp(a: any, b: any) {\n  return a < b ? -1 : 1;\n}\nexport function order(keys: any[]) {\n  return keys.sort(cmp); // sorted\n}\n";
    assert_eq!(entry_names(live, "cmp"), ["order"]);
}

#[test]
fn type_arguments_and_whitespace_before_the_argument_list_are_read_through() {
    let typed_callback = "function format(value: number): string {\n  return String(value);\n}\nexport function render(values: number[]) {\n  return values.map<string>(format);\n}\n";
    assert_eq!(entry_names(typed_callback, "format"), ["render"]);
    for construction in [
        "new Serializer<string>()",
        "new Serializer ()",
        "new Serializer<Map<string, number>> ()",
    ] {
        let source = SRC.replacen("new Serializer()", construction, 1);
        assert_ne!(source, SRC, "{construction}");
        assert_eq!(
            entry_hops(&source, "Serializer"),
            [("serialize".to_string(), 1)],
            "{construction}"
        );
        assert_eq!(
            entry_hops(&source, "compareStrings"),
            [("serialize".to_string(), 2)],
            "{construction}"
        );
    }
    // A comparison is not a type argument list.
    let compared = SRC.replacen(
        "return new Serializer().serialize(input);",
        "return String(Serializer < input);",
        1,
    );
    assert_ne!(compared, SRC);
    assert!(entry_names(&compared, "Serializer").is_empty());
}

#[test]
fn spaced_or_generic_member_declaration_is_not_a_call() {
    for member in ["compare (a: any, b: any) {", "compare<T>(a: T, b: T) {"] {
        let source = format!(
            "export function serialize(input: any): string {{\n  return new Serializer().run(input);\n}}\nfunction compare(a: string, b: string): number {{\n  return a < b ? -1 : 1;\n}}\nconst Serializer = (function () {{\n  class Serializer {{\n    {member}\n      return 0;\n    }}\n    run(value: any) {{\n      return this.compare(value, value);\n    }}\n  }}\n  return Serializer;\n}})();\n"
        );
        assert_eq!(
            entry_names(&source, "Serializer"),
            ["serialize"],
            "{member}"
        );
        assert!(entry_names(&source, "compare").is_empty(), "{member}");
    }
}

#[test]
fn built_class_that_alone_reads_a_module_constant_reaches_it() {
    let source = "export function lookup(key: string): number {\n  return new Reader().get(key);\n}\nconst TABLE: Record<string, number> = { a: 1 };\nconst Reader = (function () {\n  class Reader {\n    get(key: string): number {\n      return TABLE[key];\n    }\n  }\n  return Reader;\n})();\n";
    assert_eq!(entry_hops(source, "TABLE"), [("lookup".to_string(), 2)]);
    // Control: a write to the table anywhere in the module refuses it.
    let mutated =
        format!("{source}export function set(key: string): void {{\n  TABLE[key] = 2;\n}}\n");
    assert!(entry_names(&mutated, "TABLE").is_empty());
}

#[test]
fn stored_or_deferred_function_reference_gets_no_edge() {
    for (label, body) in [
        ("timer", "setTimeout(cmp);\n  return keys;"),
        ("promise", "return Promise.resolve(keys).then(cmp);"),
        ("listener", "emitter.on(\"x\", cmp);\n  return keys;"),
        ("stored", "const later = cmp;\n  return keys.sort(later);"),
        ("bare call argument", "return apply(cmp);"),
        ("this argument", "return keys.map(String, cmp);"),
        ("array element", "return [cmp];"),
        (
            "also stored",
            "registry.push(cmp);\n  return keys.sort(cmp);",
        ),
    ] {
        let source = format!(
            "function cmp(a: any, b: any) {{\n  return a < b ? -1 : 1;\n}}\nexport function order(keys: any[]) {{\n  {body}\n}}\n"
        );
        assert!(entry_names(&source, "cmp").is_empty(), "{label}");
    }
}

#[test]
fn tested_entry_relates_the_built_class_and_its_callee_weakly() -> Result<(), String> {
    for (label, changed, owner) in [
        (
            "class",
            (
                13,
                "      return Object.keys(value).sort(compareStrings).join(\";\");",
            ),
            "Serializer",
        ),
        ("callee", (5, "  return a <= b ? -1 : 1;"), "compareStrings"),
    ] {
        let source = SRC.replacen(
            SRC.lines().nth(changed.0 - 1).unwrap_or_default(),
            changed.1,
            1,
        );
        assert_ne!(source, SRC, "{label}: fixture not rewritten");
        let finding = finding_for(label, &source, TEST, changed, owner)?;
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| test.relation_reason
                    == Some(crate::domain::RelationReason::HelperOwnerCall)),
            "{label}: {:?}",
            finding.related_tests
        );
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{label}");
    }
    Ok(())
}

#[test]
fn uncalled_entry_keeps_no_static_path() -> Result<(), String> {
    let test = "import { it, expect } from 'vitest';\nimport { other } from '../src/other';\n\nit('x', () => {\n  expect(other()).toBe(1);\n});\n";
    let changed = (5, "  return a <= b ? -1 : 1;");
    let source = SRC.replacen("  return a < b ? -1 : 1;", changed.1, 1);
    let finding = finding_for("entry-not-called", &source, test, changed, "compareStrings")?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert!(
        finding.related_tests.is_empty(),
        "{:?}",
        finding.related_tests
    );
    Ok(())
}
