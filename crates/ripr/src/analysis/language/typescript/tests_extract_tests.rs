//! Focused extraction tests for active Jest/Vitest declaration forms.

use super::*;

#[test]
fn extracts_active_test_modifiers_with_assertions() {
    let tests = extract_tests(
        Path::new("tests/pricing.test.ts"),
        r#"
test.only("focused", () => {
    expect(applyDiscount(100, 100)).toBe(90);
});
it.concurrent("parallel", () => {
    expect(add(1, 2)).toBe(3);
});
test.sequential("serial", () => {
    expect(normalize("x")).toBe("x");
});
"#,
    );

    assert_eq!(tests.len(), 3);
    assert_eq!(tests[0].local_name, "focused");
    assert_eq!(tests[1].local_name, "parallel");
    assert_eq!(tests[2].local_name, "serial");
    assert!(tests.iter().all(|test| test.assertions.len() == 1));
}

#[test]
fn recurses_active_describe_modifiers() {
    let tests = extract_tests(
        Path::new("tests/pricing.test.ts"),
        r#"
describe.only("focused suite", () => {
    test("focused case", () => {
        expect(applyDiscount(100, 100)).toBe(90);
    });
});
describe.concurrent("parallel suite", () => {
    it("parallel case", () => {
        expect(add(1, 2)).toBe(3);
    });
});
describe.sequential("serial suite", () => {
    test("serial case", () => {
        expect(normalize("x")).toBe("x");
    });
});
"#,
    );

    assert_eq!(tests.len(), 3);
    assert_eq!(tests[0].name, "focused suite focused case");
    assert_eq!(tests[1].name, "parallel suite parallel case");
    assert_eq!(tests[2].name, "serial suite serial case");
    assert!(tests.iter().all(|test| test.describe_names.len() == 1));
}

#[test]
fn recurses_bare_and_modified_parameterized_suites() {
    let tests = extract_tests(
        Path::new("tests/pricing.test.ts"),
        r#"
describe.each([[100], [150]])("amount %i", () => {
    test("discount", () => {
        expect(applyDiscount(100, 100)).toBe(90);
    });
});
describe.concurrent.each([[1], [2]])("parallel %i", () => {
    it.only("adds", () => {
        expect(add(1, 2)).toBe(3);
    });
});
"#,
    );

    assert_eq!(tests.len(), 2);
    assert_eq!(tests[0].name, "amount %i discount");
    assert_eq!(tests[0].describe_names, vec!["amount %i".to_string()]);
    assert_eq!(tests[1].name, "parallel %i adds");
    assert_eq!(tests[1].describe_names, vec!["parallel %i".to_string()]);
    assert!(tests.iter().all(|test| test.assertions.len() == 1));
}

#[test]
fn recognizes_active_modifier_before_each() {
    let tests = extract_tests(
        Path::new("tests/pricing.test.ts"),
        r#"
test.only.each([
    [100, 90],
    [150, 140],
])("discounts %#", (amount, expected) => {
    expect(applyDiscount(amount, 100)).toBe(expected);
});
"#,
    );

    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].local_name, "discounts %#");
    assert_eq!(tests[0].assertions.len(), 1);
}

#[test]
fn keeps_disabled_conditional_expected_failure_and_unknown_declarations_uncredited() {
    let tests = extract_tests(
        Path::new("tests/pricing.test.ts"),
        r#"
test.skip("skipped", () => {
    expect(applyDiscount(100, 100)).toBe(90);
});
it.todo("todo");
describe.skip("disabled suite", () => {
    test("nested", () => {
        expect(applyDiscount(100, 100)).toBe(90);
    });
});
test.runIf(true)("conditional", () => {
    expect(applyDiscount(100, 100)).toBe(90);
});
test.skipIf(false)("conditional skip", () => {
    expect(applyDiscount(100, 100)).toBe(90);
});
test.fails("expected failure", () => {
    expect(applyDiscount(100, 100)).toBe(90);
});
runner.only("unknown root", () => {
    expect(applyDiscount(100, 100)).toBe(90);
});
test.retry("unknown modifier", () => {
    expect(applyDiscount(100, 100)).toBe(90);
});
"#,
    );

    assert!(tests.is_empty());
}

#[test]
fn extracted_active_test_reaches_direct_owner_relation() {
    let tests = extract_tests(
        Path::new("tests/pricing.test.ts"),
        r#"
test.only("discount boundary", () => {
    const result = applyDiscount(100, 100);
    expect(result).toBe(90);
});
"#,
    );
    assert_eq!(tests.len(), 1);

    let owner = TypeScriptOwner {
        name: "applyDiscount".to_string(),
        file: PathBuf::from("src/pricing.ts"),
        start_line: 1,
        end_line: 20,
        owner_kind: OwnerKind::Function,
        class_name: None,
        decorated: false,
        imports: Vec::new(),
    };
    let candidates = related_test_candidates(
        &owner,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].relation, TypeScriptRelationKind::DirectOwnerCall);
    assert_eq!(candidates[0].test.name, "discount boundary");
}
