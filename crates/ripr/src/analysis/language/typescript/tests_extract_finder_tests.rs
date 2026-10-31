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
