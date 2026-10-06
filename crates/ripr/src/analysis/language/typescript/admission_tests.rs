//! Assertion admission for extracted TypeScript tests (#5524).

use super::admission::classify_for_test;
use super::*;

const OWNER_IMPORT: &str = "import { checkout, checkLimit, Cart } from '../src/cart';\n";

fn admission_of(source: &str) -> Result<&'static str, String> {
    let full = format!("{OWNER_IMPORT}{source}");
    let tests = extract_tests(Path::new("tests/cart.test.ts"), &full);
    tests
        .iter()
        .find(|test| test.local_name == "x")
        .map(|test| test.assertion_admission.as_str())
        .ok_or_else(|| {
            format!(
                "expected test `x` to be extracted, got {:?}",
                tests.iter().map(|test| &test.name).collect::<Vec<_>>()
            )
        })
}

#[test]
fn assertion_admission_separates_no_assertion_from_unresolved_assertion_like_forms()
-> Result<(), String> {
    use TypeScriptAssertionAdmission as A;
    let recognized = A::Recognized.as_str();
    let none = A::NoAssertionLike.as_str();
    let unresolved = A::Unresolved.as_str();
    let cases: &[(&str, &str, &str)] = &[
        // -- recognized assertions (control 2, 4) --
        (
            "jest/vitest expect().toBe",
            "test('x', () => { expect(checkout(1)).toBe(2); });",
            recognized,
        ),
        (
            "snapshot matcher",
            "test('x', () => { expect(checkout(1)).toMatchSnapshot(); });",
            recognized,
        ),
        (
            "AVA t.is",
            "import test from 'ava';\ntest('x', (t) => { t.is(checkout(1), 2); });",
            recognized,
        ),
        (
            "tape t.equal",
            "import test from 'tape';\ntest('x', (t) => { t.equal(checkout(1), 2); t.end(); });",
            recognized,
        ),
        (
            "node assert.strictEqual",
            "import assert from 'node:assert';\ntest('x', () => { assert.strictEqual(checkout(1), 2); });",
            recognized,
        ),
        (
            "chai expect().to.equal",
            "import { expect } from 'chai';\nit('x', () => { expect(checkout(1)).to.equal(2); });",
            recognized,
        ),
        // -- established absence (control 1, 7) --
        (
            "owner call only",
            "test('x', () => { checkout(1); });",
            none,
        ),
        (
            "production import named like an assertion",
            "test('x', () => { checkLimit(checkout(1)); });",
            none,
        ),
        (
            "builtins, locals and console",
            "test('x', () => { const items = [1, 2].map((n) => n * 2); const total = Math.max(...items); console.log(JSON.stringify({ total })); checkout(total); });",
            none,
        ),
        (
            "vitest mock factory",
            "import { vi } from 'vitest';\ntest('x', () => { const spy = vi.fn(); checkout(spy); spy.mockReturnValue(1); });",
            none,
        ),
        (
            "global jest.fn",
            "test('x', () => { const spy = jest.fn(() => 1); checkout(spy); });",
            none,
        ),
        (
            "receiver built by an inert beforeEach",
            "let cart;\nbeforeEach(() => { cart = new Cart(); });\ntest('x', () => { cart.add(1); });",
            none,
        ),
        (
            "awaited owner call",
            "test('x', async () => { await checkout(1); });",
            none,
        ),
        (
            "bare done()",
            "it('x', (done) => { checkout(1); done(); });",
            none,
        ),
        (
            "options object before the callback",
            "it('x', { timeout: 100 }, () => { checkout(1); });",
            none,
        ),
        (
            "each with literal rows",
            "test.each([[1, 2], [3, 4]])('x', (a, b) => { checkout(a, b); });",
            none,
        ),
        (
            "node built-in module",
            "import path from 'node:path';\ntest('x', () => { checkout(path.join('a', 'b')); });",
            none,
        ),
        (
            "mocha this.timeout",
            "it('x', function () { this.timeout(1000); checkout(1); });",
            none,
        ),
        (
            "sibling test assertions do not taint",
            "test('other', () => { expect(checkout(1)).toBe(1); });\ntest('x', () => { checkout(2); });",
            none,
        ),
        (
            "describe-level constant",
            "describe('d', () => { const input = 3; test('x', () => { checkout(input); }); });",
            none,
        ),
        (
            "local function is walked",
            "test('x', () => { function run(n) { return checkout(n); } run(1); });",
            none,
        ),
        (
            "caught error is logged",
            "test('x', () => { try { checkout(1); } catch (error) { console.error(error); } });",
            none,
        ),
        (
            "commonjs require of the owner",
            "const cart = require('../src/cart');\ntest('x', () => { cart.checkout(1); });",
            none,
        ),
        (
            "concurrent modifier",
            "test.concurrent('x', async () => { await checkout(1); });",
            none,
        ),
        // -- activation modifiers do not change admission (control 8); the
        // extractor drops skip/fails/todo forms before admission runs --
        (
            "only test without an assertion",
            "it.only('x', () => { checkout(1); });",
            none,
        ),
        (
            "var in a for loop is function-scoped",
            "test('x', () => { for (var i = 0; i < 2; i++) { checkout(i); } checkout(i); });",
            none,
        ),
        (
            "commonjs require of a runner",
            "const { test } = require('vitest');\ntest('x', () => { checkout(1); });",
            none,
        ),
        // -- assertion-like but unresolved (control 3, 6) --
        (
            "commonjs require of an assertion package",
            "const tap = require('tap');\ntest('x', () => { tap.same(checkout(1), 2); });",
            unresolved,
        ),
        (
            "commonjs require of a test-support helper",
            "const h = require('./helpers');\ntest('x', () => { h.same(checkout(1), 2); });",
            unresolved,
        ),
        (
            "node:assert required inside a describe callback",
            "describe('d', () => {\n  const assert = require('node:assert');\n  it('x', () => { assert.strictEqual(checkout(1), 2); });\n});",
            unresolved,
        ),
        (
            "chai assert destructured from a require inside describe",
            "describe('d', () => {\n  const { assert } = require('chai');\n  it('x', () => { assert.equal(checkout(1), 2); });\n});",
            unresolved,
        ),
        (
            "require of a package inside the test",
            "test('x', () => { const { same } = require('tap'); same(checkout(1), 2); });",
            unresolved,
        ),
        (
            "custom matcher",
            "test('x', () => { expect(checkout(1)).toBeEven(); });",
            unresolved,
        ),
        (
            "expect.assertions without a matcher",
            "test('x', () => { expect.assertions(1); checkout(1); });",
            unresolved,
        ),
        (
            "same-file helper function",
            "function verifyTotal(n) { if (n < 0) throw new Error('bad'); }\ntest('x', () => { verifyTotal(checkout(1)); });",
            unresolved,
        ),
        (
            "same-file arrow helper with a neutral name",
            "const run = (n) => checkout(n);\ntest('x', () => { run(1); });",
            unresolved,
        ),
        (
            "describe-level helper",
            "describe('d', () => { function step(n) { return n; } test('x', () => { step(checkout(1)); }); });",
            unresolved,
        ),
        (
            "same-file class",
            "class FakeCart { add() {} }\ntest('x', () => { new FakeCart().add(checkout(1)); });",
            unresolved,
        ),
        (
            "helper passed to map",
            "function step(n) { return n; }\ntest('x', () => { [checkout(1)].map(step); });",
            unresolved,
        ),
        (
            "block-scoped local does not hide an outer helper",
            "function run(n) { return n; }\ntest('x', () => { run(1); { const run = checkout; run(2); } });",
            unresolved,
        ),
        (
            "test-support import",
            "import { runCase } from './helpers';\ntest('x', () => { runCase(checkout(1)); });",
            unresolved,
        ),
        (
            "test-utils directory import",
            "import { build } from '../test-utils/build';\ntest('x', () => { checkout(build()); });",
            unresolved,
        ),
        (
            "required test-support module",
            "const { runCase } = require('./fixtures/cases');\ntest('x', () => { checkout(1); });",
            unresolved,
        ),
        (
            "third-party package",
            "import { render } from '@testing-library/react';\ntest('x', () => { render(checkout(1)); });",
            unresolved,
        ),
        (
            "unknown global",
            "test('x', () => { helperFromGlobals(checkout(1)); });",
            unresolved,
        ),
        (
            "global fail",
            "test('x', () => { if (!checkout(1)) fail('no'); });",
            unresolved,
        ),
        (
            "throw statement",
            "test('x', () => { if (!checkout(1)) throw new Error('no'); });",
            unresolved,
        ),
        (
            "computed-member callee",
            "test('x', () => { const fns = { a: checkout }; fns['a'](1); });",
            unresolved,
        ),
        (
            "call-result callee",
            "test('x', () => { checkout(1)(); });",
            unresolved,
        ),
        (
            "dynamic import",
            "test('x', async () => { const m = await import('../src/cart'); m.checkout(1); });",
            unresolved,
        ),
        (
            "done called with an error",
            "it('x', (done) => { checkout(1); done(new Error('x')); });",
            unresolved,
        ),
        (
            "done passed on",
            "it('x', (done) => { checkout(1).then(done); });",
            unresolved,
        ),
        (
            "AVA t.plan",
            "import test from 'ava';\ntest('x', (t) => { t.plan(1); checkout(1); });",
            unresolved,
        ),
        (
            "node:test t.assert",
            "import { test } from 'node:test';\ntest('x', (t) => { t.assert.ok(checkout(1)); });",
            unresolved,
        ),
        (
            "destructured vitest context",
            "test('x', ({ task }) => { checkout(task.name); });",
            unresolved,
        ),
        (
            "Promise.reject",
            "test('x', async () => { await Promise.reject(checkout(1)); });",
            unresolved,
        ),
        (
            "promise executor with reject",
            "test('x', () => new Promise((resolve, reject) => { checkout(1) ? resolve() : reject(); }));",
            unresolved,
        ),
        (
            "process.exit",
            "test('x', () => { checkout(1); process.exit(1); });",
            unresolved,
        ),
        (
            "assertion-named method on a local",
            "test('x', () => { const result = checkout(1); result.verify(); });",
            unresolved,
        ),
        (
            "should chain on a local",
            "test('x', () => { const result = checkout(1); result.should.equal(2); });",
            unresolved,
        ),
        (
            "vi.waitFor",
            "import { vi } from 'vitest';\ntest('x', async () => { await vi.waitFor(() => checkout(1)); });",
            unresolved,
        ),
        (
            "nested registration",
            "test('x', () => { checkout(1); test('inner', () => {}); });",
            unresolved,
        ),
        (
            "each with a non-literal table passing done on",
            "const cases = [1];\ntest.each(cases)('x', (n, done) => { checkout(n).then(done); });",
            unresolved,
        ),
        // -- what the runner runs around the test --
        (
            "asserting beforeEach",
            "beforeEach(() => { expect(checkout(0)).toBe(0); });\ntest('x', () => { checkout(1); });",
            unresolved,
        ),
        (
            "afterEach calling an unknown global",
            "afterEach(() => { cleanup(); });\ntest('x', () => { checkout(1); });",
            unresolved,
        ),
        (
            "AVA test.beforeEach with t.is",
            "import test from 'ava';\ntest.beforeEach((t) => { t.is(checkout(0), 0); });\ntest('x', () => { checkout(1); });",
            unresolved,
        ),
        (
            "hook done(err)",
            "beforeEach((done) => { done(new Error('setup')); });\ntest('x', () => { checkout(1); });",
            unresolved,
        ),
        (
            "expect.extend at top level",
            "expect.extend({ toBeEven() { return { pass: true, message: () => '' }; } });\ntest('x', () => { checkout(1); });",
            unresolved,
        ),
        (
            "side-effect import",
            "import './setup';\ntest('x', () => { checkout(1); });",
            unresolved,
        ),
        (
            "top-level value from a helper",
            "function makeInput() { return 1; }\nconst input = makeInput();\ntest('x', () => { checkout(input); });",
            unresolved,
        ),
        (
            "custom registration from test.extend",
            "import { test as base } from 'vitest';\nconst it = base.extend({});\nit('x', () => { checkout(1); });",
            unresolved,
        ),
        (
            "registration imported from a fixture module",
            "import { it } from './fixtures';\nit('x', () => { checkout(1); });",
            unresolved,
        ),
    ];
    let mut failures = Vec::new();
    for (label, source, expected) in cases {
        match admission_of(source) {
            Ok(actual) if actual == *expected => {}
            Ok(actual) => failures.push(format!("{label}: expected {expected}, got {actual}")),
            Err(error) => failures.push(format!("{label}: {error}")),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn test_support_paths_resolve_against_the_test_file() {
    let dir = |segments: &[&str]| segments.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // A colocated test importing its sibling owner is production.
    assert_eq!(classify_for_test("./cart", &dir(&["src"])), "production");
    // The same specifier beside a test under `tests/` is test support.
    assert_eq!(
        classify_for_test("./cart", &dir(&["tests"])),
        "test_support"
    );
    assert_eq!(
        classify_for_test("../src/cart", &dir(&["tests"])),
        "production"
    );
    assert_eq!(
        classify_for_test("../../__tests__/shared", &dir(&["src", "a"])),
        "test_support"
    );
    assert_eq!(classify_for_test("node:fs", &[]), "node_builtin");
    assert_eq!(classify_for_test("fs", &[]), "node_builtin");
    assert_eq!(classify_for_test("node:assert", &[]), "package");
    assert_eq!(classify_for_test("vitest", &[]), "runner");
    assert_eq!(classify_for_test("lodash", &[]), "package");
}

#[test]
fn parse_error_files_yield_no_admission_row() {
    // A file the parser refuses yields no test at all, so no test can carry
    // an established absence from a partial parse.
    let tests = extract_tests(
        Path::new("tests/cart.test.ts"),
        "import { checkout } from '../src/cart';\ntest('x', () => { checkout(1 });\n",
    );
    assert!(tests.is_empty(), "got {tests:?}");
}

/// Control 8 holds because inactive registrations never reach admission:
/// the extractor drops them, so no admission row can depend on activation.
#[test]
fn inactive_registrations_are_not_extracted() -> Result<(), String> {
    for source in [
        "test.skip('x', () => { checkout(1); });",
        "test.fails('x', () => { checkout(1); });",
        "test.todo('x');",
        "import { it } from 'node:test';\nit('x', { expectFailure: true }, () => { checkout(1); });",
    ] {
        let full = format!("{OWNER_IMPORT}{source}");
        let tests = extract_tests(Path::new("tests/cart.test.ts"), &full);
        if tests.iter().any(|test| test.local_name == "x") {
            return Err(format!("expected `{source}` to be dropped by extraction"));
        }
    }
    // The active form of the same registration is extracted.
    admission_of("test('x', () => { checkout(1); });").map(|_| ())
}
