//! RIPR-SPEC-0262: a same-file `expect.extend` matcher that passes only on
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
        // `return { pass: X }`, either operand order, a function expression.
        "expect.extend({ toBeCents(r, e) { return { pass: e === r, message: () => \"\" }; } });\n",
        "expect.extend({ toBeCents: function (r, e) { return { pass: r === e }; } });\n",
        "expect.extend({ toBeCents: (r, e) => { return { pass: r > 0 && (r === e) }; } });\n",
        // Inert values beside `pass`: a function, a name, a literal.
        "expect.extend({ toBeCents(r, e) { return { message: () => \"\", actual: r, kind: `cents`, n: 1, pass: r === e }; } });\n",
        // Terms of names, literals, member reads, calls and operators, an
        // `=` inside a string among them.
        "expect.extend({ toBeCents(r, e) { return { pass: typeof r === \"number\" && !Number.isNaN(r) && `${r}` !== \"a=b\" && (r === e) }; } });\n",
        // A plain `import { expect }`, array patterns, literals and types.
        "import { expect, test } from \"@jest/globals\";\nconst [a, b]: number[] = [1, 2];\nconst xs = [a, b][0];\nexpect.extend({ toBeCents(r, e) { return { pass: r === e }; } });\n",
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
        // A nested function or class rebinds a parameter without `=`.
        "expect.extend({ toBeCents(r, e) { return { pass: (function () { for (r of [e]); return true; })() && r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: (class { static { for (r in { x: 1 }); } }) && r === e }; } });\n",
        // `arguments` aliases the parameters in sloppy mode.
        "expect.extend({ toBeCents: function (r, e) { return { pass: Array.prototype.fill.call(arguments, e) && r === e }; } });\n",
        // Any form outside the listed ones, here an object literal.
        "expect.extend({ toBeCents(r, e) { return { pass: Boolean({ v: r }) && r === e }; } });\n",
        // Another property, or an earlier `pass`, evaluated before the
        // comparison.
        "expect.extend({ toBeCents(r, e) { return { pre: (r = e), pass: r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pre: fix(r), pass: r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: (r = e), pass: r === e }; } });\n",
        "expect.extend({ toBeCents(r, e) { return { pass: true, pass: r === e }; } });\n",
        // `Object.is` is not read: the file may replace `Object` unseen.
        "expect.extend({ toBeCents(r, e) { return { pass: Object.is(r, e) }; } });\n",
        // A default may bind `expected` to `received`.
        "expect.extend({ toBeCents(r, e = r) { return { pass: r === e }; } });\n",
        "expect.extend({ toBeCents(r = 150, e) { return { pass: r === e }; } });\n",
        "expect.extend({ toBeCents: (r = 150, e) => { return { pass: r === e }; } });\n",
        // An escaped name hides `eval` from the write scan.
        "expect.extend({ toBeCents: function (r, e) { return { pass: ev\\u0061l(\"r\" + String.fromCharCode(61) + \"e\") && r === e }; } });\n",
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
        "beforeAll(() => { expect [\"ext\" + \"end\"]({ toBeCents: () => ({ pass: true }) }); });\n",
        "beforeAll(() => { (expect)[\"ext\" + \"end\"]({ toBeCents: () => ({ pass: true }) }); });\n",
        "const e = expect;\nbeforeAll(() => { e[\"ext\" + \"end\"]({ toBeCents: () => ({ pass: true }) }); });\n",
        // An escaped name anywhere in the file.
        "beforeAll(() => { expect.ext\\u0065nd({ toBeCents: () => ({ pass: true }) }); });\n",
        // A computed member that builds the name from a module object.
        "import g from \"./g\";\nbeforeAll(() => { g[\"exp\" + \"ect\"][\"ext\" + \"end\"]({ toBeCents: () => ({ pass: true }) }); });\n",
        // `expect` other than as `expect(` or `expect.`, or imported other
        // than as a plain `import { expect }`.
        "beforeAll(() => { for (const f of Object.values(expect)) try { f({ toBeCents: () => ({ pass: true }) }); } catch {} });\n",
        "import { expect as e } from \"@jest/globals\";\n",
    ] {
        assert_eq!(
            cents_oracles(&format!("{CENTS_MATCHER}{extra}{CENTS_TEST}")),
            unknown(),
            "{extra}"
        );
    }
    // A module or global object that holds `expect` under no visible name:
    // a namespace import, a dynamic import, `global`, or a root the reader
    // refuses by name.
    for root in [
        "g",
        "await import(\"@jest/globals\")",
        "global",
        "globalThis",
        "window",
        "self",
        "this",
        "document.defaultView",
        "(() => 0).constructor(\"return th\" + \"is\")()",
        "Reflect.get(g, \"de\" + \"fault\")",
        "Function(\"return th\" + \"is\")()",
        "eval(\"th\" + \"is\")",
        "require(\"@jest/globals\")",
        "jest.requireActual(\"@jest/globals\")",
        "jest.requireMock(\"@jest/globals\")",
        "vi.importActual(\"@jest/globals\")",
        "vi.importMock(\"@jest/globals\")",
        "jest.createMockFromModule(\"@jest/globals\")",
        "expect.valueOf()",
        "Object.getOwnPropertyDescriptor(g, \"de\" + \"fault\")",
        "Object.getOwnPropertyDescriptors(g)",
        "Symbol.for(\"x\")",
        "chai",
    ] {
        let import = if root == "g" {
            "import * as g from \"@jest/globals\";\n"
        } else {
            ""
        };
        let hook = format!(
            "{import}beforeAll(async () => {{ for (const v of Object.values({root})) for (const f of Object.values(v)) try {{ f({{ toBeCents: () => ({{ pass: true }}) }}); }} catch {{}} }});\n"
        );
        assert_eq!(
            cents_oracles(&format!("{CENTS_MATCHER}{hook}{CENTS_TEST}")),
            unknown(),
            "{hook}"
        );
    }
    // A local `expect` imported under another name: the file's assertions
    // reach no runner matcher, and the registration is not read either.
    let renamed = format!("import {{ fake as expect }} from \"./fake\";\n{CENTS_MATCHER}");
    assert!(strict_equality_matchers_of(&renamed).is_empty());
    // `declare global` counts as a whole phrase, so `declare global_x`
    // cannot excuse a real `global`.
    let balanced = format!(
        "// declare global_x\n{CENTS_MATCHER}beforeAll(() => {{ for (const f of Object.values(global)) try {{ f({{ toBeCents: () => ({{ pass: true }}) }}); }} catch {{}} }});\n{CENTS_TEST}"
    );
    assert_eq!(cents_oracles(&balanced), unknown());
    // A keyword used as a property name, and a non-ASCII name, still index.
    for indexed in [
        "beforeAll(() => { ({ of: g }).of[\"ext\" + \"end\"]({ toBeCents: () => ({ pass: true }) }); });\n",
        "beforeAll(() => { const gé = g; gé[\"ext\" + \"end\"]({ toBeCents: () => ({ pass: true }) }); });\n",
    ] {
        let source = format!("import g from \"./g\";\n{CENTS_MATCHER}{indexed}{CENTS_TEST}");
        assert_eq!(cents_oracles(&source), unknown(), "{indexed}");
    }
    // A runner module's default export, an object literal, a comment or a
    // variable named `of` before `[`, and the prototype of an assertion.
    for extra in [
        "import g from \"@jest/globals\";\nbeforeAll(() => { for (const v of Object.values(g)) for (const f of Object.values(v)) try { f({ toBeCents: () => ({ pass: true }) }); } catch {} });\n",
        "import g from \"./g\";\nconst x = { ...g }[\"exp\" + \"ect\"];\nbeforeAll(() => { for (const f of Object.values(x)) try { f({ toBeCents: () => ({ pass: true }) }); } catch {} });\n",
        "import g from \"./g\";\nbeforeAll(() => { for (const f of Object.values(g /**/ [\"exp\" + \"ect\"])) try { f({ toBeCents: () => ({ pass: true }) }); } catch {} });\n",
        "import g from \"./g\";\nconst of = g;\nbeforeAll(() => { for (const f of Object.values(of[\"exp\" + \"ect\"])) try { f({ toBeCents: () => ({ pass: true }) }); } catch {} });\n",
        "Object.getPrototypeOf(expect(0)).toBeCents = () => {};\n",
        "expect(0).__proto__.toBeCents = () => {};\n",
    ] {
        let source = format!("{extra}{CENTS_MATCHER}{CENTS_TEST}");
        assert_eq!(cents_oracles(&source), unknown(), "{extra}");
    }
    // A default import from another module keeps the reading.
    let other_default = format!("import money from \"./money\";\n{CENTS_MATCHER}{CENTS_TEST}");
    assert_eq!(cents_oracles(&other_default), strong());
    // `declare global` alone keeps the reading.
    let declared = format!(
        "{CENTS_MATCHER}declare global {{ namespace jest {{ interface Matchers<R> {{ toBeCents(e: number): R; }} }} }}\n{CENTS_TEST}"
    );
    assert_eq!(cents_oracles(&declared), strong());
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
    // A negated assertion reads strong under no matcher name either.
    let negated = format!(
        "{CENTS_MATCHER}test(\"cents\", () => {{\n  expect(toCents(1.5)).not.toBeCents(150);\n}});\n"
    );
    assert!(
        extract_tests(Path::new("test/cents.test.ts"), &negated)
            .iter()
            .flat_map(|test| &test.assertions)
            .all(|assertion| assertion.oracle_strength != OracleStrength::Strong)
    );
    // A shadowed `expect` reaches no runner matcher at all.
    let shadowed = format!("const expect = (x: unknown) => x;\n{CENTS_MATCHER}{CENTS_TEST}");
    assert!(cents_oracles(&shadowed).is_empty());
}

fn strict_equality_matchers_of(source: &str) -> Vec<String> {
    let allocator = oxc_allocator::Allocator::default();
    let ret = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
    strict_equality_matchers(&ret.program.body, source)
}
