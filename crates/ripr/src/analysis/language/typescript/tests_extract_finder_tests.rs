//! Finder contract for withheld shadowed-`undefined` registrations (#6631).
//!
//! The extractor already withholds a `skip`/`todo`/`fails`/`expectFailure`
//! option whose value is a bound `undefined`. The partial-extraction finder
//! must use the same fence: a withheld registration is not a dropped active
//! test. A genuinely unwalked loop registration stays disclosed.

use super::*;
use std::path::Path;

#[test]
fn shadowed_undefined_skip_option_is_not_a_partial_extraction_gap() {
    let file = Path::new("test/q.test.ts");
    let source = "const undefined = true;\ntest('q', { skip: undefined }, () => {\n  expect(1).toBe(1);\n});\n";
    let tests = extract_tests(file, source);
    assert!(
        tests.is_empty(),
        "extractor withholds the shadowed skip: {tests:?}"
    );
    assert!(
        detect_partial_test_extraction(file, source, &tests).is_none(),
        "withheld shadowed-undefined skip is not a dropped active test"
    );
}

#[test]
fn unshadowed_undefined_skip_option_stays_extracted() {
    let file = Path::new("test/q.test.ts");
    let source = "test('q', { skip: undefined }, () => {\n  expect(1).toBe(1);\n});\n";
    let tests = extract_tests(file, source);
    assert_eq!(
        tests.len(),
        1,
        "global undefined does not skip the registration: {tests:?}"
    );
    assert_eq!(tests[0].name, "q");
    assert!(detect_partial_test_extraction(file, source, &tests).is_none());
}

#[test]
fn nested_describe_callback_shadow_is_not_a_partial_extraction_gap() {
    let file = Path::new("test/q.test.ts");
    let source = "describe('s', () => {\n  const undefined = true;\n  test('q', { skip: undefined }, () => {\n    expect(1).toBe(1);\n  });\n});\n";
    let tests = extract_tests(file, source);
    assert!(
        tests.is_empty(),
        "nested-scope shadow withholds the test: {tests:?}"
    );
    assert!(
        detect_partial_test_extraction(file, source, &tests).is_none(),
        "nested-scope shadow is not a dropped active test"
    );
}

#[test]
fn unshadowed_undefined_skip_in_an_unwalked_loop_stays_a_partial_extraction_gap() {
    let file = Path::new("test/q.test.ts");
    let source = "for (let i = 0; i < n; i++) {\n  test('q', { skip: undefined }, () => {\n    expect(1).toBe(1);\n  });\n}\n";
    let tests = extract_tests(file, source);
    assert!(
        tests.is_empty(),
        "an unknown counted for is not walked: {tests:?}"
    );
    assert!(
        detect_partial_test_extraction(file, source, &tests).is_some(),
        "global undefined is not a skip; the dropped registration stays disclosed"
    );
}

#[test]
fn unknown_for_loop_registration_still_yields_the_partial_extraction_gap() {
    let file = Path::new("test/q.test.ts");
    let source =
        "for (let i = 0; i < n; i++) {\n  test('q', () => {\n    expect(1).toBe(1);\n  });\n}\n";
    let tests = extract_tests(file, source);
    assert!(
        tests.is_empty(),
        "an unknown iterable is not walked: {tests:?}"
    );
    let gap = detect_partial_test_extraction(file, source, &tests);
    assert!(
        gap.is_some(),
        "a genuinely dropped loop registration stays disclosed"
    );
    assert_eq!(
        gap.map(|gap| gap.shape),
        Some("test/it call in loop/callback/nested body")
    );
}

#[test]
fn shadowed_undefined_does_not_hide_a_genuinely_dropped_loop_registration() {
    let file = Path::new("test/q.test.ts");
    let source = "const undefined = true;\nfor (let i = 0; i < n; i++) {\n  test('q', () => {\n    expect(1).toBe(1);\n  });\n}\n";
    let tests = extract_tests(file, source);
    assert!(
        tests.is_empty(),
        "an unknown iterable is not walked: {tests:?}"
    );
    assert!(
        detect_partial_test_extraction(file, source, &tests).is_some(),
        "binding undefined must not suppress a dropped loop registration"
    );
}

#[test]
fn vitest_test_for_registration_is_a_partial_extraction_gap() {
    let file = Path::new("tests/tier.test.ts");
    let source = "import { test, expect } from 'vitest';\nimport { tier } from '../src/tier';\ntest.for([\n  [49, 'basic'],\n  [50, 'basic'],\n])(\"tier row\", ([score, expected]) => {\n  expect(tier(score)).toBe(expected);\n});\n";
    let tests = extract_tests(file, source);
    assert!(
        tests.is_empty(),
        "test.for registrations are not indexed yet: {tests:?}"
    );
    let gap = detect_partial_test_extraction(file, source, &tests);
    assert!(
        gap.as_ref()
            .is_some_and(|gap| { gap.shape == "test.for registration" && gap.sample_line == 3 }),
        "top-level Vitest test.for must be disclosed, got {gap:?}"
    );
}

#[test]
fn vitest_it_for_registration_is_a_partial_extraction_gap() {
    let file = Path::new("tests/tier.test.ts");
    let source = "import { it, expect } from 'vitest';\nit.for([[1, 2]])(\"row\", ([a, b]) => {\n  expect(a + b).toBe(3);\n});\n";
    let tests = extract_tests(file, source);
    assert!(tests.is_empty(), "it.for is not indexed: {tests:?}");
    let gap = detect_partial_test_extraction(file, source, &tests);
    assert!(
        gap.as_ref()
            .is_some_and(|gap| gap.shape == "test.for registration"),
        "it.for disclosed, got {gap:?}"
    );
}

#[test]
fn vitest_test_for_empty_table_is_not_a_dropped_active_registration() {
    let file = Path::new("tests/tier.test.ts");
    let source = "import { test, expect } from 'vitest';\ntest.for([])(\"dead\", () => {\n  expect(1).toBe(1);\n});\n";
    let tests = extract_tests(file, source);
    assert!(tests.is_empty());
    assert!(
        detect_partial_test_extraction(file, source, &tests).is_none(),
        "a known-empty .for table registers no callbacks"
    );
}

#[test]
fn vitest_test_for_skip_option_is_not_a_dropped_active_registration() {
    let file = Path::new("tests/tier.test.ts");
    let source = "import { test, expect } from 'vitest';\ntest.for([[1]])(\"row\", { skip: true }, () => {\n  expect(1).toBe(1);\n});\n";
    let tests = extract_tests(file, source);
    assert!(tests.is_empty());
    assert!(
        detect_partial_test_extraction(file, source, &tests).is_none(),
        "skipped .for registration is inactive"
    );
}

#[test]
fn unrelated_for_method_is_not_a_partial_extraction_gap() {
    let file = Path::new("tests/tier.test.ts");
    let source = "import { expect } from 'vitest';\nconst chain = {\n  for: (rows: number[][]) => (title: string, fn: () => void) => fn(),\n};\nchain.for([[1]])(\"row\", () => {\n  expect(1).toBe(1);\n});\n";
    let tests = extract_tests(file, source);
    assert!(tests.is_empty());
    assert!(
        detect_partial_test_extraction(file, source, &tests).is_none(),
        "a local .for helper is not a runner registration"
    );
}

#[test]
fn vitest_test_only_for_registration_is_a_partial_extraction_gap() {
    let file = Path::new("tests/tier.test.ts");
    let source = "import { test, expect } from 'vitest';\ntest.only.for([[1]])(\"row\", () => {\n  expect(1).toBe(1);\n});\n";
    let tests = extract_tests(file, source);
    assert!(tests.is_empty());
    let gap = detect_partial_test_extraction(file, source, &tests);
    assert!(
        gap.as_ref()
            .is_some_and(|gap| gap.shape == "test.for registration"),
        "active .for chain, got {gap:?}"
    );
}

#[test]
fn ordinary_test_each_and_direct_test_stay_fully_extracted() {
    let file = Path::new("tests/tier.test.ts");
    let direct = "import { test, expect } from 'vitest';\ntest('plain', () => {\n  expect(1).toBe(1);\n});\n";
    let direct_tests = extract_tests(file, direct);
    assert_eq!(direct_tests.len(), 1);
    assert!(detect_partial_test_extraction(file, direct, &direct_tests).is_none());

    let each = "import { test, expect } from 'vitest';\ntest.each([[1]])(\"row\", (n) => {\n  expect(n).toBe(1);\n});\n";
    let each_tests = extract_tests(file, each);
    assert_eq!(
        each_tests.len(),
        1,
        "call-form .each stays indexed: {each_tests:?}"
    );
    assert!(detect_partial_test_extraction(file, each, &each_tests).is_none());
}
