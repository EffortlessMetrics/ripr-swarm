//! RIPR-SPEC-0259: a same-file `expect.extend` matcher that passes only on
//! `received === expected` reads `exact_value` / strong; every other
//! registration or body keeps `unknown`.

use super::*;

const CENTS_MATCHER: &str = "expect.extend({\n  toBeCents(received: number, expected: number) {\n    const pass = Number.isInteger(received) && received === expected;\n    return {\n      pass,\n      message: () => `expected ${received} to be ${expected}`,\n    };\n  },\n});\n";

const CENTS_TEST: &str = "test(\"cents\", () => {\n  expect(toCents(1.5)).toBeCents(150);\n});\n";

/// The oracle of every `toBeCents` assertion in `source`.
fn cents_oracles(source: &str) -> Vec<(OracleKind, OracleStrength)> {
    extract_tests(Path::new("test/cents.test.ts"), source)
        .iter()
        .flat_map(|test| &test.assertions)
        .filter(|assertion| assertion.matcher == "toBeCents")
        .map(|assertion| {
            (
                assertion.oracle_kind.clone(),
                assertion.oracle_strength.clone(),
            )
        })
        .collect()
}

fn strong() -> Vec<(OracleKind, OracleStrength)> {
    vec![(OracleKind::ExactValue, OracleStrength::Strong)]
}

fn unknown() -> Vec<(OracleKind, OracleStrength)> {
    vec![(OracleKind::Unknown, OracleStrength::Unknown)]
}

#[test]
fn strict_equality_custom_matcher_reads_exact() {
    assert_eq!(
        cents_oracles(&format!("{CENTS_MATCHER}{CENTS_TEST}")),
        strong()
    );
    for matcher in [
        // `return { pass: X }`, either operand order, `Object.is`.
        "expect.extend({ toBeCents(r, e) { return { pass: e === r, message: () => \"\" }; } });\n",
        "expect.extend({ toBeCents: function (r, e) { return { pass: Object.is(r, e) }; } });\n",
        "expect.extend({ toBeCents: (r, e) => { return { pass: r > 0 && (r === e) }; } });\n",
        // Inert values beside `pass`: a function, a name, a literal.
        "expect.extend({ toBeCents(r, e) { return { message: () => \"\", actual: r, kind: `cents`, n: 1, pass: r === e }; } });\n",
        // `Object` mentioned only as `Object.is(` calls.
        "expect.extend({ toBeCents(r, e) { return { pass: Object.is(r, e) && Object.is(e, r) }; } });\n",
    ] {
        assert_eq!(
            cents_oracles(&format!("{matcher}{CENTS_TEST}")),
            strong(),
            "{matcher}"
        );
    }
}

#[test]
fn custom_matcher_that_may_pass_without_strict_equality_stays_unknown() {
    for matcher in [
        // Loose, negated or optional equality.
        "expect.extend({ toBeCents(r, e) { return { pass: r == e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: r !== e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: r === e || r === 0 }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: !(r === e) }; } });\n",
        // Equality of something else.
        "expect.extend({ toBeCents(r, e) { return { pass: Math.round(r) === e }; } });\n",
        // Only the two-parameter `(received, expected)` signature is read.
        "expect.extend({ toBeCents(r, e, t) { return { pass: r === e && t === e }; } });\n",
        "expect.extend({ toBeCents(r) { return { pass: r === r }; } });\n",
        // A parameter written before the comparison, however spelled.
        "expect.extend({ toBeCents(r, e) { return { pass: (r = e, true) && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: ([r] = [e], true) && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: (r >>= 0, true) && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: (r <<= 0, true) && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: (r ||= e) && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: (r++, true) && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: (r--, true) && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: eval(\"r\\x3de\") && r === e }; } });\n",
        "expect.extend({ toBeCents: function (r, e) { return { pass: Object.assign(arguments, [e, e]) && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: ((() => { r = e; })(), true) && r === e }; } });\n",
        // Another property, or an earlier `pass`, evaluated before the
        // comparison.
        "expect.extend({ toBeCents(r, e) { return { pre: (r = e), pass: r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pre: fix(r), pass: r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: (r = e), pass: r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: true, pass: r === e }; } });\n",
        // A shadowed or replaced `Object`.
        "const Object = { is: () => true };\nexpect.extend({ toBeCents(r, e) { return { pass: Object.is(r, e) }; } });\n",
        "Object.is = () => true;\nexpect.extend({ toBeCents(r, e) { return { pass: Object.is(r, e) }; } });\n",
        // Duplicate parameter names (sloppy mode).
        "expect.extend({ toBeCents: function (r, r) { return { pass: r === r }; } });\n",
        // Another statement, a `let`, a renamed result, or two `pass` keys.
        "expect.extend({ toBeCents(r, e) { r = e; return { pass: r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { let pass = r === e; return { pass }; } });\n",
        "expect.extend({ toBeCents(r, e) { const ok = r === e; return { pass: ok }; } });\n",
        "expect.extend({ toBeCents(r, e) { const ok = r === e; return { pass }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { ...base, pass: r === e }; } });\n",
        // An async or expression-bodied matcher, a destructured parameter.
        "expect.extend({ async toBeCents(r, e) { return { pass: r === e }; } });\n",
        "expect.extend({ toBeCents: (r, e) => ({ pass: r === e }) });\n",
        "expect.extend({ toBeCents({ v }, e) { return { pass: v === e }; } });\n",
    ] {
        assert_eq!(
            cents_oracles(&format!("{matcher}{CENTS_TEST}")),
            unknown(),
            "{matcher}"
        );
    }
}

#[test]
fn custom_matcher_another_registration_may_replace_stays_unknown() {
    for extra in [
        // A second registration of the name, from an object this reader
        // cannot see, or inside a hook.
        "expect.extend({ toBeCents(r, e) { return { pass: true }; } });\n",
        "expect.extend(matchers);\n",
        "expect.extend({ ...matchers });\n",
        "beforeAll(() => { expect.extend({ toBeCents: () => ({ pass: true }) }); });\n",
        "beforeAll(() => { expect.extend ({ toBeCents: () => ({ pass: true }) }); });\n",
        "beforeAll(() => { expect[\"extend\"]({ toBeCents: () => ({ pass: true }) }); });\n",
        "beforeAll(() => { expect[\"ext\" + \"end\"]({ toBeCents: () => ({ pass: true }) }); });\n",
    ] {
        assert_eq!(
            cents_oracles(&format!("{CENTS_MATCHER}{extra}{CENTS_TEST}")),
            unknown(),
            "{extra}"
        );
    }
    // An escaped `extend` hides from the mention count, which would let the
    // hook's registration go unseen.
    let escaped = "beforeAll(() => { expect.extend({ toBeCents: () => ({ pass: true }) }); });\nexpect.ext\\u0065nd({ toBeCents(r, e) { return { pass: r === e }; } });\n";
    assert_eq!(cents_oracles(&format!("{escaped}{CENTS_TEST}")), unknown());
    // A built-in name keeps its own reading.
    let to_be = "expect.extend({ toBe(r, e) { return { pass: r === e }; } });\n";
    assert!(strict_equality_matchers_of(to_be).is_empty());
}

#[test]
fn custom_matcher_assertion_shape_must_compare_one_value() {
    // Each shape still records its `toBeCents` assertion (a negated one is
    // not recorded under the matcher name), and none reads strong.
    for (test, expected) in [
        (
            "test(\"cents\", () => {\n  expect(toCents(1.5)).not.toBeCents(150);\n});\n",
            Vec::new(),
        ),
        (
            "test(\"cents\", () => {\n  expect(toCents(1.5)).toBeCents(150, 2);\n});\n",
            unknown(),
        ),
        (
            "test(\"cents\", () => {\n  expect(toCents(1.5)).toBeCents(...args);\n});\n",
            unknown(),
        ),
        (
            "test(\"cents\", async () => {\n  await expect(toCents(1.5)).rejects.toBeCents(150);\n});\n",
            unknown(),
        ),
    ] {
        assert_eq!(
            cents_oracles(&format!("{CENTS_MATCHER}{test}")),
            expected,
            "{test}"
        );
    }
    // A shadowed `expect` reaches no runner matcher at all.
    let shadowed = format!("const expect = (x: unknown) => x;\n{CENTS_MATCHER}{CENTS_TEST}");
    assert!(cents_oracles(&shadowed).is_empty());
}

fn strict_equality_matchers_of(source: &str) -> Vec<String> {
    let allocator = oxc_allocator::Allocator::default();
    let ret = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
    strict_equality_matchers(&ret.program.body, source)
}
