//! Tests registered inside a `for`, `for...of`, `for...in` loop or a
//! `.forEach` callback, often with a computed title. unjs/ufo declares most of
//! its tests this way (`for (const t of tests) { test(`${t.input}`, ...) }`),
//! and the extractor used to skip every one of them, so well-tested owners
//! read `no_static_path`.

use super::*;

const URL_OWNER: &str = "export function withBase(input: string, base: string): string {\n  return base + input;\n}\n\nexport function withoutBase(input: string, base: string): string {\n  return input.slice(base.length);\n}\n";
const WITH_BASE_LINE: (usize, &str) = (2, "  return base + input;");

fn with_base_finding(label: &str, test_source: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/url.ts"), URL_OWNER)?;
    ts_write_file(&root.join("test/url.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/url.ts", &[WITH_BASE_LINE])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("withBase"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `withBase`"))
}

fn assert_with_base_exposed(label: &str, test_source: &str) -> Result<(), String> {
    let finding = with_base_finding(label, test_source)?;
    assert_eq!(
        finding.ripr.reach.state,
        StageState::Yes,
        "{label}: a loop-declared test calls `withBase`, evidence: {:?}",
        finding.evidence
    );
    assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
    Ok(())
}

const UFO_FOR_OF: &str = "import { describe, expect, test } from 'vitest';\nimport { withBase } from '../src/url';\n\ndescribe('withBase', () => {\n  const tests = [\n    { base: '/', input: '/', out: '/' },\n    { base: '/foo', input: '/bar', out: '/foo/bar' },\n  ];\n\n  for (const t of tests) {\n    test(`${t.input} -> ${t.out}`, () => {\n      expect(withBase(t.input, t.base)).toBe(t.out);\n    });\n  }\n\n  test('literal title', () => {\n    expect(1).toBe(1);\n  });\n});\n";

#[test]
fn for_of_test_with_template_title_is_extracted_with_placeholder_name() {
    let file = Path::new("test/url.test.ts");
    let tests = extract_tests(file, UFO_FOR_OF);
    let names: Vec<&str> = tests.iter().map(|test| test.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "withBase <computed title, line 11>",
            "withBase literal title"
        ],
        "the loop-declared test is extracted in source order under its describe"
    );
    let looped = &tests[0];
    assert_eq!(looped.line, 11);
    assert_eq!(looped.local_name, "<computed title, line 11>");
    assert_eq!(looped.describe_names, vec!["withBase".to_string()]);
    assert!(
        looped
            .body_text
            .contains("expect(withBase(t.input, t.base)).toBe(t.out)"),
        "body: {}",
        looped.body_text
    );
    assert_eq!(looped.assertions.len(), 1, "the `toBe` oracle is read");
    assert!(
        detect_partial_test_extraction(file, UFO_FOR_OF, &tests).is_none(),
        "every registration is extracted, so the index is not partial"
    );
}

#[test]
fn for_each_for_and_for_in_bodies_are_extracted() {
    let file = Path::new("test/url.test.ts");
    let source = "import { it, expect } from 'vitest';\nimport { withBase } from '../src/url';\n\ncases.forEach((c) => it(c.name, () => {\n  expect(withBase(c.input, '/')).toBe(c.out);\n}));\n\nfor (let i = 0; i < 2; i++) {\n  it('indexed ' + i, () => {\n    expect(withBase(String(i), '/')).toBe('/' + i);\n  });\n}\n\nfor (const key in table) it(`key ${key}`, () => {\n  expect(withBase(key, '/')).toBe(table[key]);\n});\n\nfor (const row of rows) {\n  describe(`row ${row.id}`, () => {\n    it('keeps the base', () => {\n      expect(withBase(row.input, '/')).toBe(row.out);\n    });\n  });\n}\n";
    let tests = extract_tests(file, source);
    let names: Vec<&str> = tests.iter().map(|test| test.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "<computed title, line 4>",
            "<computed title, line 9>",
            "<computed title, line 14>",
            "<computed title, line 19> keeps the base",
        ]
    );
    assert!(
        tests.iter().all(|test| test.assertions.len() == 1),
        "each body keeps its oracle: {tests:?}"
    );
    assert!(detect_partial_test_extraction(file, source, &tests).is_none());
}

#[test]
fn loops_that_register_nothing_or_write_an_outer_name_are_not_walked() {
    let file = Path::new("test/url.test.ts");
    let source = "let t;\nfor (const c of []) {\n  it('never runs', () => {\n    expect(withBase('a', '/')).toBe('/a');\n  });\n}\n[].forEach((c) => it('never runs either', () => {}));\nfor (t of tests) {\n  it('outer target', () => {\n    expect(withBase(t, '/')).toBe(t);\n  });\n}\n";
    let tests = extract_tests(file, source);
    assert!(tests.is_empty(), "got {tests:?}");
    assert!(
        detect_partial_test_extraction(file, source, &tests).is_some(),
        "the skipped registrations stay disclosed"
    );
}

#[test]
fn for_of_loop_declared_test_exposes_the_owner_it_calls() -> Result<(), String> {
    assert_with_base_exposed("loop-for-of", UFO_FOR_OF)
}

#[test]
fn for_each_declared_test_exposes_the_owner_it_calls() -> Result<(), String> {
    assert_with_base_exposed(
        "loop-for-each",
        "import { describe, expect, it } from 'vitest';\nimport { withBase } from '../src/url';\n\nconst tests = [{ base: '/foo', input: '/bar', out: '/foo/bar' }];\n\ndescribe('withBase', () => {\n  tests.forEach((t) => {\n    it(`${t.input} -> ${t.out}`, () => {\n      expect(withBase(t.input, t.base)).toBe(t.out);\n    });\n  });\n});\n",
    )
}

/// Negative control: the loop body calls a different function, so the
/// extracted test stays unrelated to `withBase`.
#[test]
fn loop_declared_test_calling_another_function_stays_unrelated() -> Result<(), String> {
    let finding = with_base_finding(
        "loop-other-owner",
        "import { describe, expect, test } from 'vitest';\nimport { withoutBase } from '../src/url';\n\ndescribe('withoutBase', () => {\n  for (const t of [{ base: '/foo', input: '/foo/bar', out: '/bar' }]) {\n    test(`${t.input} -> ${t.out}`, () => {\n      expect(withoutBase(t.input, t.base)).toBe(t.out);\n    });\n  }\n});\n",
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "no loop body names `withBase`, evidence: {:?}",
        finding.evidence
    );
    Ok(())
}

/// The loop variable shadows a receiver the enclosing describe constructs,
/// so a call through it is not credited to the owner class.
#[test]
fn loop_variable_shadows_an_enclosing_receiver() -> Result<(), String> {
    let root = ts_unique_tempdir("loop-shadow")?;
    ts_write_file(
        &root.join("src/cart.ts"),
        "export class Cart {\n  total(): number {\n    return 1 + 1;\n  }\n}\n",
    )?;
    ts_write_file(
        &root.join("tests/cart.test.ts"),
        "import { describe, expect, it } from 'vitest';\nimport { Cart } from '../src/cart';\n\ndescribe('Cart', () => {\n  const cart = new Cart();\n  for (const cart of [{ total: () => 2 }]) {\n    it(`fake ${cart.total()}`, () => {\n      expect(cart.total()).toBe(2);\n    });\n  }\n});\n",
    )?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(
            "src/cart.ts",
            &[(3, "    return 1 + 1;")],
        )],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let finding = result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("total"))
        })
        .ok_or_else(|| "expected a finding for `total`".to_string())?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "the loop's `cart` is not a Cart, evidence: {:?}",
        finding.evidence
    );
    Ok(())
}
