//! #11255: an all-literal object or array expected value (nested objects and
//! arrays included) is a concrete literal for Jest/Vitest `toEqual`,
//! `toStrictEqual` and `toMatchObject`. Every new form has a discriminating
//! negative: one identifier, call, spread, hole, computed or shorthand key,
//! accessor, template or wrapped expression keeps the value dynamic, and an
//! all-zero-like container still never witnesses a field update.

use super::*;

fn first_assertion(call: &str) -> Result<TypeScriptAssertion, String> {
    let source = format!("{call};");
    let allocator = Allocator::default();
    let parse_result = Parser::new(&allocator, &source, SourceType::ts()).parse();
    let mut assertions = collect_expect_assertions_in_statements(
        &parse_result.program.body,
        &SourceText::new(&source),
        None,
    );
    if assertions.len() != 1 {
        return Err(format!(
            "`{call}` collected {} assertions",
            assertions.len()
        ));
    }
    Ok(assertions.remove(0))
}

#[test]
fn all_literal_containers_are_concrete_for_the_equality_matchers() -> Result<(), String> {
    for call in [
        // citty parser.test.ts:11
        r#"expect(r).toEqual({ _: [], name: "John", admin: true })"#,
        // defu test/defu.test.ts:28
        r#"expect(r).toEqual({ a: { b: "c", d: "e" } })"#,
        // pathe test/utils.spec.ts:77
        r#"expect(r).toMatchObject(["~/some/assets/smth.jpg", "~assets/smth.jpg"])"#,
        r#"expect(r).toStrictEqual({ a: [1, { b: null }], c: -2 })"#,
        r#"expect(r).toMatchObject({ a: { b: 1 } })"#,
        r#"expect(r).toEqual([{ a: 1 }, [2, "x"]])"#,
        r#"expect(r).toEqual({})"#,
        r#"expect(r).toEqual({ "k-1": 1, 2: [] })"#,
    ] {
        let assertion = first_assertion(call)?;
        if assertion.has_dynamic_matcher_arg || assertion.expected_value_or_variant.is_none() {
            return Err(format!(
                "`{call}` must be a concrete literal: {assertion:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn one_non_literal_anywhere_keeps_the_container_dynamic() -> Result<(), String> {
    for call in [
        "expect(r).toEqual({ _: [], name })",
        "expect(r).toEqual({ _: [], name: n })",
        "expect(r).toEqual({ a: { b: x } })",
        "expect(r).toEqual({ a: { b: 1 }, ...rest })",
        "expect(r).toEqual({ [k]: 1 })",
        "expect(r).toEqual({ a: f() })",
        "expect(r).toEqual({ a: [x] })",
        "expect(r).toEqual({ a: [1, , 2] })",
        "expect(r).toEqual({ a: [...xs] })",
        "expect(r).toEqual({ a: `t` })",
        "expect(r).toEqual({ a: -x })",
        "expect(r).toEqual({ a: !true })",
        "expect(r).toEqual({ a: ({ b: 1 }) })",
        "expect(r).toEqual({ a: { b: 1 } as const })",
        "expect(r).toEqual({ get a() { return 1; } })",
        "expect(r).toEqual({ a() { return 1; } })",
        "expect(r).toEqual({ a: () => 1 })",
        r#"expect(r).toEqual({ __proto__: { a: 1 } })"#,
        r#"expect(r).toEqual({ "__proto__": {} })"#,
        "expect(r).toMatchObject([1, y])",
        "expect(r).toMatchObject({ a: { b: y } })",
        "expect(r).toStrictEqual([{ a: z }])",
        // Reference equality never matches a fresh literal: stays dynamic for
        // the nested forms newly credited elsewhere.
        "expect(r).toBe({ a: { b: 1 } })",
        "expect(r).toBe([1])",
        "expect(r).toBeCloseTo([1])",
    ] {
        let assertion = first_assertion(call)?;
        if !assertion.has_dynamic_matcher_arg || assertion.expected_value_or_variant.is_some() {
            return Err(format!("`{call}` must stay dynamic: {assertion:?}"));
        }
    }
    Ok(())
}

#[test]
fn zero_like_containers_include_empty_and_all_zero_objects() {
    for zero in [
        "{}",
        "{ }",
        "{ _: [] }",
        "{ a: 0, b: '', c: { d: [], e: null } }",
        "[{}]",
        "[{ a: [] }, 0]",
        r#"{ "k": false }"#,
    ] {
        assert!(ts_expected_is_zero_like(zero), "{zero}");
    }
    for live in [
        "{ a: 1 }",
        r#"{ _: [], name: "John" }"#,
        "{ _: [], admin: true }",
        "{ a: { b: 'c' } }",
        "[{ a: 1 }]",
        "[{}, 2]",
        "{ a: 0, b: [3] }",
        "{ a: -1 }",
        // An unreadable shape is never claimed zero-like by accident of
        // stripping tokens: a key named like a zero token carries no value.
        "{ null: 5 }",
        "{ a: ",
    ] {
        assert!(!ts_expected_is_zero_like(live), "{live}");
    }
}

/// An empty or all-zero container matches a field's initial state, so it never
/// pins a field update; the same container with a live leaf still does.
#[test]
fn field_update_pinned_by_zero_like_object_stays_unwitnessed() {
    let owner = TypeScriptOwner {
        name: "record".to_string(),
        file: PathBuf::from("src/stats.ts"),
        start_line: 1,
        end_line: 5,
        owner_kind: OwnerKind::Method,
        class_name: Some("Stats".to_string()),
        arity: Some(1),
        params: vec!["n".to_string()],
        source_text: Some(
            concat!("record(n: number): void {\n", "    this.hits = n;\n", "}",).to_string(),
        ),
        ..test_owner("record", "src/stats.ts")
    };
    let change = FieldUpdateChange {
        field: "hits".to_string(),
        class_name: "Stats".to_string(),
    };
    let witnessed = |expected: &str| {
        let mut test = direct_test_with_assertion(
            "records",
            "record",
            format!(
                "const stats = new Stats();\nstats.record(3);\nexpect(stats.hits).toEqual({expected});"
            ),
            "toEqual",
            1,
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        test.assertions[0].line = 3;
        test.assertions[0].observed_expression = Some("stats.hits".to_string());
        test.assertions[0].expected_value_or_variant = Some(expected.to_string());
        let candidate = TypeScriptRelatedCandidate::with_owner_path(
            &test,
            TypeScriptRelationKind::DirectOwnerCall,
            TypeScriptOwnerPathDisposition::TrustedOwnerPath,
        );
        ts_field_update_witnessed(&change, &owner, &[candidate])
    };
    assert!(
        witnessed("{ total: 3 }"),
        "control: a live object pin witnesses"
    );
    for zero in ["{}", "{ list: [] }", "{ a: { b: 0 } }", "[{}]"] {
        assert!(!witnessed(zero), "{zero} must stay unwitnessed");
    }
}

/// An object expectation equals no scalar branch value, exactly like an array.
#[test]
fn expected_side_is_live_treats_an_object_expectation_as_dead_for_scalar_branches() {
    let owner = boundary_witness_owner();
    for expected in ["{ total: 90 }", "{}", "{ a: { b: [1] } }"] {
        assert_eq!(
            expected_side_is_live(
                &owner,
                "  if (total >= 100) {",
                "total",
                "100",
                &["100".to_string()],
                expected,
            ),
            Some(false),
            "{expected}"
        );
    }
}

/// A scalar ternary arm is never an object or an array, so a container
/// expectation cannot witness a changed arm; a member-path arm stays unknown.
#[test]
fn container_expectation_is_dead_against_scalar_ternary_arms_only() {
    let owner = TypeScriptOwner {
        name: "pick".to_string(),
        file: PathBuf::from("src/pick.ts"),
        start_line: 1,
        end_line: 3,
        owner_kind: OwnerKind::Function,
        arity: Some(1),
        params: vec!["p".to_string()],
        source_text: Some(
            concat!(
                "export function pick(p: number): number {\n",
                "    return p > 5 ? 3 : 2;\n",
                "}",
            )
            .to_string(),
        ),
        ..test_owner("pick", "src/pick.ts")
    };
    let change = ts_ternary_arm_change(
        &owner,
        "    return p > 5 ? 3 : 2;",
        Some("    return p > 5 ? 1 : 2;"),
    );
    assert!(
        change.is_some(),
        "the one-arm ternary edit must read as a TernaryArmChange"
    );
    let Some(change) = change else {
        return;
    };
    assert!(change.expected_is_dead("{ a: 3 }"));
    assert!(change.expected_is_dead("{}"));
    assert!(!change.expected_is_dead("3"));
}
