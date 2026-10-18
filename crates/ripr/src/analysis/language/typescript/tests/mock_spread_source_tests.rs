//! RIPR-SPEC-0247 rule 1 (amended 2026-10-18): `{ m: jest.fn(), ...defaults }`
//! keeps its mock when `defaults` is a test-file `const` object literal with
//! plain keys, no `m` key and no other mention; every other spread refuses.

use super::*;

fn payload() -> TypeScriptMockPayload {
    TypeScriptMockPayload {
        target: "db.write".to_string(),
        expected: "2".to_string(),
        kind: TypeScriptMockPayloadKind::CalledWith,
    }
}

fn credited(declarations: &str, binding: &str) -> Result<bool, String> {
    let facts = MockCallFacts::read(
        "  db.write(v + 2);",
        Some("function save(db: Db, v: number) {\n  db.write(v + 2);\n}"),
        "save",
        &["db".to_string(), "v".to_string()],
    );
    let source = format!(
        "import {{ save }} from \"../src/save\";\n{declarations}\ntest(\"save\", () => {{\n  {binding}\n  save(db, 1);\n  expect(db.write).toHaveBeenCalledWith(2);\n}});\n"
    );
    let tests = extract_tests(Path::new("test/save.test.ts"), &source);
    let [test] = tests.as_slice() else {
        return Err(format!("expected one test in {source}"));
    };
    Ok(facts.credits_with_factories(
        &payload(),
        &test.body_text,
        &test.mock_factories_in_file,
        &test.mock_spread_sources_in_file,
    ))
}

const SPREAD: &str = "const db = { write: jest.fn(), ...defaults };";
const DEFAULTS: &str = "const defaults = { flush: jest.fn() };";

#[test]
fn spread_of_a_plain_const_without_the_mock_key_is_credited() {
    assert_eq!(credited(DEFAULTS, SPREAD), Ok(true));
    // Order and several sources do not matter once none carries `write`.
    assert_eq!(
        credited(
            "const a = { flush: 1 };\nconst b = { read: 2, size: 3 };",
            "const db = { ...a, write: jest.fn(), ...b };"
        ),
        Ok(true)
    );
}

#[test]
fn spread_that_could_replace_the_mock_refuses() {
    for (label, declarations, binding) in [
        (
            "source carries the key",
            "const defaults = { write: 1 };",
            SPREAD,
        ),
        (
            "key is quoted",
            "const defaults = { \"write\": 1 };",
            SPREAD,
        ),
        (
            "computed key",
            "const defaults = { [\"write\"]: 1 };",
            SPREAD,
        ),
        (
            "method of the key",
            "const defaults = { write() {} };",
            SPREAD,
        ),
        (
            "nested spread",
            "const other = { write: 1 };\nconst defaults = { ...other };",
            SPREAD,
        ),
        (
            "shorthand entry",
            "const flush = 1;\nconst defaults = { flush };",
            SPREAD,
        ),
        ("let binding", "let defaults = { flush: 1 };", SPREAD),
        ("not a literal", "const defaults = makeDefaults();", SPREAD),
        ("unresolved source", "", SPREAD),
        (
            "mutated before the copy",
            "const defaults = { flush: 1 };\n(defaults as any).write = 1;",
            SPREAD,
        ),
        (
            "member of the source",
            "const defaults = { flush: 1 };\nObject.assign(defaults, { write: 1 });",
            SPREAD,
        ),
        (
            "shadowed in the test",
            DEFAULTS,
            "const defaults = { write: 1 };\n  const db = { write: jest.fn(), ...defaults };",
        ),
        (
            "escape hatch",
            "const defaults = { flush: 1 };\neval(\"x\");",
            SPREAD,
        ),
        (
            "spread of an expression",
            DEFAULTS,
            "const db = { write: jest.fn(), ...(defaults) };",
        ),
        (
            "spread of a call",
            DEFAULTS,
            "const db = { write: jest.fn(), ...defaults() };",
        ),
    ] {
        assert_eq!(credited(declarations, binding), Ok(false), "{label}");
    }
}

#[test]
fn source_is_recorded_only_when_the_file_mentions_it_at_a_spread() {
    let names = |source: &str| {
        let tests = extract_tests(Path::new("test/save.test.ts"), source);
        tests
            .first()
            .map(|test| {
                test.mock_spread_sources_in_file
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let wrap = |declarations: &str| {
        format!("{declarations}\ntest(\"t\", () => {{\n  const x = {{ ...defaults }};\n}});\n")
    };
    assert_eq!(names(&wrap(DEFAULTS)), vec!["defaults".to_string()]);
    assert!(names(&wrap("const defaults = { a: 1 };\nexport { defaults };")).is_empty());
    assert!(names(&wrap("var defaults = { a: 1 };")).is_empty());
    assert!(names(&wrap("const { defaults } = { defaults: { a: 1 } };")).is_empty());
}

#[test]
fn rest_binding_in_a_parameter_list_or_assignment_is_not_a_spread_use() {
    let names = |extra: &str| {
        let source = format!(
            "const defaults = {{ flush: 1 }};\n{extra}\ntest(\"t\", () => {{\n  const x = {{ write: 1, ...defaults }};\n}});\n"
        );
        extract_tests(Path::new("test/save.test.ts"), &source)
            .first()
            .map_or(0, |test| test.mock_spread_sources_in_file.len())
    };
    assert_eq!(names(""), 1);
    assert_eq!(names("function f({ a, ...defaults }) {}"), 0);
    assert_eq!(names("const g = ({ a, ...defaults }) => a;"), 0);
    assert_eq!(names("const [h, { b, ...defaults }] = cfg();"), 0);
    // A spread used as a call argument is not the `const x = { ... }` form.
    assert_eq!(names("use({ ...defaults });"), 0);
}
