//! `node:assert` and chai assertion-library oracle extraction (#4547).

use super::*;

fn only_assertions(file: &str, source: &str) -> Vec<TypeScriptAssertion> {
    let tests = extract_tests(Path::new(file), source);
    assert_eq!(tests.len(), 1, "fixture must register one test: {tests:?}");
    tests.into_iter().flat_map(|test| test.assertions).collect()
}

fn assert_oracle(
    assertion: &TypeScriptAssertion,
    kind: OracleKind,
    strength: OracleStrength,
    rendered: &str,
) {
    assert_eq!(assertion.oracle_kind, kind, "{assertion:?}");
    assert_eq!(assertion.oracle_strength, strength, "{assertion:?}");
    assert_eq!(assertion_oracle_text(assertion), rendered, "{assertion:?}");
}

/// The jshttp/mime-types shape: CommonJS `require('assert')` in a mocha
/// suite. `assert.strictEqual(actual, expected)` is an exact-value oracle
/// whose observed expression is the owner call.
#[test]
fn commonjs_assert_strict_equal_is_exact_value_oracle() {
    let assertions = only_assertions(
        "test/test.js",
        r#"
var assert = require('assert')
var mimeTypes = require('..')

describe('mimeTypes', function () {
  describe('.charset(type)', function () {
    it('should return "UTF-8" for "text/html"', function () {
      assert.strictEqual(mimeTypes.charset('text/html'), 'UTF-8')
    })
  })
})
"#,
    );
    assert_eq!(assertions.len(), 1, "{assertions:?}");
    let assertion = &assertions[0];
    assert_oracle(
        assertion,
        OracleKind::ExactValue,
        OracleStrength::Strong,
        "assert.strictEqual(...)",
    );
    assert_eq!(assertion.matcher, "strictEqual");
    assert_eq!(
        assertion.observed_expression.as_deref(),
        Some("mimeTypes.charset('text/html')")
    );
    assert_eq!(
        assertion.expected_value_or_variant.as_deref(),
        Some("'UTF-8'")
    );
    assert_eq!(assertion.oracle_confidence, OracleConfidence::High);
    assert_eq!(assertion.line, 8);
}

/// The yargs-parser shape: a named ESM import of one assert method, called
/// bare, including a renamed import.
#[test]
fn named_assert_method_imports_are_exact_value_oracles() {
    let assertions = only_assertions(
        "test/parser.test.ts",
        r#"
import { strictEqual } from 'assert'
import { deepStrictEqual as same } from 'node:assert'
import { parse } from '../src/parse'

it('parses flags', () => {
  strictEqual(parse('--x').x, true)
  same(parse('--y 1'), { y: 1 })
})
"#,
    );
    assert_eq!(assertions.len(), 2, "{assertions:?}");
    assert_oracle(
        &assertions[0],
        OracleKind::ExactValue,
        OracleStrength::Strong,
        "strictEqual(...)",
    );
    assert_eq!(
        assertions[0].observed_expression.as_deref(),
        Some("parse('--x').x")
    );
    assert_oracle(
        &assertions[1],
        OracleKind::ExactValue,
        OracleStrength::Strong,
        "same(...)",
    );
    assert_eq!(assertions[1].matcher, "deepStrictEqual");
    assert_eq!(
        assertions[1].expected_value_or_variant.as_deref(),
        Some("{ y: 1 }")
    );
}

/// Every recognised `node:assert/strict` method maps to its oracle family;
/// the bare callable `assert(value)` is smoke; a truthiness assertion's
/// message argument is never read as an expected value.
#[test]
fn node_assert_method_table_maps_oracle_families() {
    let assertions = only_assertions(
        "test/score.test.mjs",
        r#"
import assert from 'node:assert/strict'
import test from 'node:test'

test('score', async () => {
  assert.deepStrictEqual(score(1), [1])
  assert.notStrictEqual(score(2), 3)
  assert.ok(score(3), 'must be truthy')
  assert.throws(() => score(-1))
  await assert.rejects(scoreAsync(-1))
  assert.match(label(1), /one/)
  assert(score(4))
  assert.fooBar(score(5), 6)
})
"#,
    );
    let summary: Vec<(String, OracleKind, String)> = assertions
        .iter()
        .map(|assertion| {
            (
                assertion.matcher.clone(),
                assertion.oracle_kind.clone(),
                assertion_oracle_text(assertion),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                "deepStrictEqual".to_string(),
                OracleKind::ExactValue,
                "assert.deepStrictEqual(...)".to_string()
            ),
            (
                "notStrictEqual".to_string(),
                OracleKind::RelationalCheck,
                "assert.notStrictEqual(...)".to_string()
            ),
            (
                "ok".to_string(),
                OracleKind::SmokeOnly,
                "assert.ok(...)".to_string()
            ),
            (
                "throws".to_string(),
                OracleKind::BroadError,
                "assert.throws(...)".to_string()
            ),
            (
                "rejects".to_string(),
                OracleKind::BroadError,
                "assert.rejects(...)".to_string()
            ),
            (
                "match".to_string(),
                OracleKind::RelationalCheck,
                "assert.match(...)".to_string()
            ),
            (
                "ok".to_string(),
                OracleKind::SmokeOnly,
                "assert(...)".to_string()
            ),
        ],
        "unknown `assert.fooBar` must not be credited"
    );
    // `assert.ok(value, 'message')`: the message is not an expected value.
    assert!(assertions[2].expected_value_or_variant.is_none());
    assert!(!assertions[2].has_dynamic_matcher_arg);
    // `assert.match(value, /re/)`: a regex expected side is dynamic.
    assert!(assertions[5].has_dynamic_matcher_arg);
}

/// chai's TDD `assert`, destructured from `require('chai')` or read from a
/// chai namespace.
#[test]
fn chai_assert_interface_is_credited() {
    let assertions = only_assertions(
        "test/cart.spec.js",
        r#"
const { assert } = require('chai')
const chai = require('chai')

it('totals', function () {
  assert.equal(total([0, 1]), 1)
  assert.isTrue(isEmpty([]))
  chai.assert.deepEqual(items(), ['a'])
})
"#,
    );
    assert_eq!(assertions.len(), 3, "{assertions:?}");
    // chai's `assert.equal` is loose `==` (#4638 review): relational, not an
    // exact value, while `1 == true` (RIPR-SPEC-0247 rule 2). Its
    // `deepEqual` is strict deep equality.
    assert_oracle(
        &assertions[0],
        OracleKind::RelationalCheck,
        OracleStrength::Weak,
        "assert.equal(...)",
    );
    assert_oracle(
        &assertions[1],
        OracleKind::SmokeOnly,
        OracleStrength::Smoke,
        "assert.isTrue(...)",
    );
    assert_oracle(
        &assertions[2],
        OracleKind::ExactValue,
        OracleStrength::Strong,
        "chai.assert.deepEqual(...)",
    );
}

/// chai's BDD `expect` chains, bound by an ESM import.
#[test]
fn chai_expect_chains_map_oracle_families() {
    let assertions = only_assertions(
        "test/cart.spec.ts",
        r#"
import { expect } from 'chai'

it('totals', () => {
  expect(total([1, 2])).to.equal(3)
  expect(items()).to.deep.equal(['a'])
  expect(items()).to.eql(['a'])
  expect(isEmpty([])).to.be.true
  expect(() => total(null)).to.throw()
  expect(label(1)).to.include('one')
  expect(total([])).to.not.equal(1)
  expect(total([])).to.be.a('number')
})
"#,
    );
    let summary: Vec<(OracleKind, String)> = assertions
        .iter()
        .map(|assertion| {
            (
                assertion.oracle_kind.clone(),
                assertion_oracle_text(assertion),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                OracleKind::ExactValue,
                "expect(...).to.equal(...)".to_string()
            ),
            (
                OracleKind::ExactValue,
                "expect(...).to.deep.equal(...)".to_string()
            ),
            (
                OracleKind::ExactValue,
                "expect(...).to.eql(...)".to_string()
            ),
            (OracleKind::SmokeOnly, "expect(...).to.be.true".to_string()),
            (
                OracleKind::BroadError,
                "expect(...).to.throw(...)".to_string()
            ),
            (
                OracleKind::RelationalCheck,
                "expect(...).to.include(...)".to_string()
            ),
            (
                OracleKind::RelationalCheck,
                "expect(...).to.not.equal(...)".to_string()
            ),
        ],
        "unrecognised chai terminal `.a(...)` must not be credited"
    );
    assert_eq!(
        assertions[0].observed_expression.as_deref(),
        Some("total([1, 2])")
    );
    assert_eq!(
        assertions[0].expected_value_or_variant.as_deref(),
        Some("3")
    );
    assert_eq!(assertions[0].oracle_strength, OracleStrength::Strong);
}

/// `var expect = require('chai').expect` — the member-of-require form the
/// import extractor does not record.
#[test]
fn chai_expect_from_require_member_is_credited() {
    let assertions = only_assertions(
        "test/cart.spec.js",
        r#"
var expect = require('chai').expect

it('totals', function () {
  expect(total([1, 2])).to.equal(3)
})
"#,
    );
    assert_eq!(assertions.len(), 1, "{assertions:?}");
    assert_oracle(
        &assertions[0],
        OracleKind::ExactValue,
        OracleStrength::Strong,
        "expect(...).to.equal(...)",
    );
}

/// Negative: helpers the file declares itself (or imports from elsewhere) are
/// not assertion libraries, so nothing is credited.
#[test]
fn local_or_foreign_assert_helpers_are_not_credited() {
    let assertions = only_assertions(
        "test/cart.test.ts",
        r#"
import { equal } from './helpers'

function assert(value) { return value }
assert.strictEqual = (a, b) => a === b
function strictEqual(a, b) { return a === b }

it('totals', () => {
  assert(total([1]))
  assert.strictEqual(total([1, 2]), 3)
  strictEqual(total([1, 2]), 3)
  equal(total([1, 2]), 3)
})
"#,
    );
    assert!(
        assertions.is_empty(),
        "non-imported helpers must not be credited: {assertions:?}"
    );
}

/// Negative: a Jest/Vitest `expect` keeps its own mapping, and chai chain
/// syntax on a non-chai `expect` is not credited.
#[test]
fn jest_expect_is_unchanged_and_not_read_as_chai() {
    let assertions = only_assertions(
        "test/cart.test.ts",
        r#"
import { expect, it } from 'vitest'

it('totals', () => {
  expect(total([1, 2])).toBe(3)
  expect(total([1, 2])).to.equal(3)
})
"#,
    );
    assert_eq!(assertions.len(), 1, "{assertions:?}");
    assert_eq!(assertions[0].matcher, "toBe");
    assert_eq!(assertions[0].oracle_kind, OracleKind::ExactValue);
    assert_eq!(assertions[0].rendered_call, None);
    assert_eq!(
        assertion_oracle_text(&assertions[0]),
        "expect(...).toBe(...)"
    );
}

/// End to end: a mocha test asserting the owner's return value with
/// `assert.strictEqual` exposes a changed return, where it was previously
/// weakly exposed with an `unknown` oracle.
#[test]
fn assert_strict_equal_exposes_changed_return_value() -> Result<(), String> {
    let owners = extract_owners(
        Path::new("src/mime.js"),
        "function charset(type) {\n  if (!type) return false\n  return 'UTF-8'\n}\nmodule.exports = { charset }\n",
    );
    let tests = extract_tests(
        Path::new("test/mime.test.js"),
        r#"
var assert = require('assert')
var mimeTypes = require('../src/mime')

describe('mimeTypes', function () {
  it('returns UTF-8 for text/html', function () {
    assert.strictEqual(mimeTypes.charset('text/html'), 'UTF-8')
  })
})
"#,
    );
    let finding = classify_change(
        Path::new("src/mime.js"),
        3,
        "  return 'UTF-8'",
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "expected a TypeScript preview finding".to_string())?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("typescript_oracle_observed: mimeTypes.charset('text/html')")),
        "{:?}",
        finding.evidence
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("strongest extracted oracle is `unknown`")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

/// Negative (#4638 review): an imported assertion binding re-declared in the
/// test body no longer reaches the library, so the local helper is not
/// credited.
#[test]
fn assertion_binding_shadowed_in_test_body_is_not_credited() {
    let assertions = only_assertions(
        "test/cart.test.js",
        r#"
const assert = require('node:assert')
const { strictEqual } = require('node:assert')
const { expect } = require('chai')

it('totals', () => {
  const assert = { strictEqual: () => {} }
  function strictEqual() {}
  const expect = () => ({ to: { equal: () => {} } })
  assert.strictEqual(total([1, 2]), 3)
  strictEqual(total([1, 2]), 3)
  expect(total([1, 2])).to.equal(3)
})
"#,
    );
    assert!(assertions.is_empty(), "{assertions:?}");
}

/// Negative (#4638 re-review): a destructured or inline test-body declaration
/// shadows the imported binding too. The line-start text guard misses both;
/// the AST walk over the body's declarations catches them.
#[test]
fn assertion_binding_shadowed_by_destructured_or_inline_declaration_is_not_credited() {
    let assertions = only_assertions(
        "test/cart.test.js",
        r#"
const assert = require('node:assert')
const { strictEqual } = require('node:assert')

it('totals', () => {
  const { assert } = helpers
  if (ready) { const strictEqual = () => {}; strictEqual(total([1, 2]), 3) }
  assert.strictEqual(total([1, 2]), 3)
})
"#,
    );
    assert!(assertions.is_empty(), "{assertions:?}");
}

/// Negative (#4638 review): a declaration in an enclosing describe body
/// shadows the imported binding for every test inside it; a sibling describe
/// without the declaration still credits the import (positive control).
#[test]
fn assertion_binding_shadowed_in_enclosing_describe_is_not_credited() {
    let tests = extract_tests(
        Path::new("test/cart.test.js"),
        r#"
const assert = require('node:assert')
const { strictEqual } = require('node:assert')
const { expect } = require('chai')

describe('shadowed', () => {
  const assert = { strictEqual: () => {} }
  const strictEqual = () => {}
  let expect
  describe('inner', () => {
    it('totals', () => {
      assert.strictEqual(total([1, 2]), 3)
      strictEqual(total([1, 2]), 3)
      expect(total([1, 2])).to.equal(3)
    })
  })
})
describe('control', () => {
  it('totals', () => {
    assert.strictEqual(total([1, 2]), 3)
    strictEqual(total([1, 2]), 3)
    expect(total([1, 2])).to.equal(3)
  })
})
"#,
    );
    assert_eq!(tests.len(), 2, "{tests:?}");
    assert_eq!(tests[0].name, "shadowed inner totals");
    assert!(tests[0].assertions.is_empty(), "{:?}", tests[0].assertions);
    assert_eq!(tests[1].name, "control totals");
    assert_eq!(tests[1].assertions.len(), 3, "{:?}", tests[1].assertions);
}

/// Negative (#4638 review): a test or describe callback parameter named like
/// the imported binding shadows it. A bare first test parameter with that
/// name is also not read as an AVA / tape receiver: mocha passes `done`
/// there, so the call is credited through neither route.
#[test]
fn assertion_binding_shadowed_by_callback_parameter_is_not_credited() {
    let tests = extract_tests(
        Path::new("test/cart.test.js"),
        r#"
const assert = require('node:assert')
const { strictEqual } = require('node:assert')
const { expect } = require('chai')

it('test parameters', ({ assert, strictEqual, expect }) => {
  assert.strictEqual(total([1, 2]), 3)
  strictEqual(total([1, 2]), 3)
  expect(total([1, 2])).to.equal(3)
})
it('first parameter', function (assert) {
  assert.strictEqual(total([1, 2]), 3)
})
describe.each([[1]])('describe parameters', (assert, strictEqual, expect) => {
  it('totals', () => {
    assert.strictEqual(total([1, 2]), 3)
    strictEqual(total([1, 2]), 3)
    expect(total([1, 2])).to.equal(3)
  })
})
"#,
    );
    assert_eq!(tests.len(), 3, "{tests:?}");
    assert!(
        tests.iter().all(|test| test.assertions.is_empty()),
        "{tests:?}"
    );
}

/// Loose equality is not an exact-value oracle (#4638 review): legacy
/// `node:assert` `equal` / `deepEqual` compare with `==` and stay relational,
/// while the same methods bound from `node:assert/strict`, `assert/strict` or
/// the `strict` export are strict and pin the exact value. `strictEqual` /
/// `deepStrictEqual` stay exact everywhere.
#[test]
fn loose_equality_is_graded_by_assert_api() {
    let grades = |source: &str| -> Vec<(String, OracleKind, OracleStrength)> {
        only_assertions("test/calc.test.mjs", source)
            .into_iter()
            .map(|assertion| {
                (
                    assertion_oracle_text(&assertion),
                    assertion.oracle_kind,
                    assertion.oracle_strength,
                )
            })
            .collect()
    };
    let legacy = grades(
        r#"
import assert from 'node:assert'
import { equal, deepEqual as same, strictEqual } from 'assert'

it('adds', () => {
  assert.equal(add(0, 1), 1)
  assert.deepEqual(pair(), [1, 2])
  equal(add(0, 1), 1)
  same(pair(), [1, 2])
  strictEqual(add(1, 2), 3)
  assert.deepStrictEqual(pair(), [1, 2])
})
"#,
    );
    let relational = (OracleKind::RelationalCheck, OracleStrength::Weak);
    let exact = (OracleKind::ExactValue, OracleStrength::Strong);
    let expected: Vec<(String, OracleKind, OracleStrength)> = [
        ("assert.equal(...)", relational.clone()),
        ("assert.deepEqual(...)", relational.clone()),
        ("equal(...)", relational.clone()),
        ("same(...)", relational),
        ("strictEqual(...)", exact.clone()),
        ("assert.deepStrictEqual(...)", exact.clone()),
    ]
    .into_iter()
    .map(|(text, (kind, strength))| (text.to_string(), kind, strength))
    .collect();
    assert_eq!(legacy, expected);

    let strict = grades(
        r#"
import assert from 'node:assert/strict'
import { equal } from 'assert/strict'
import { strict as strictAssert } from 'node:assert'
const legacyStrict = require('assert').strict

it('adds', () => {
  assert.equal(add(1, 2), 3)
  assert.deepEqual(pair(), [1, 2])
  equal(add(1, 2), 3)
  strictAssert.equal(add(1, 2), 3)
  legacyStrict.deepEqual(pair(), [1, 2])
})
"#,
    );
    assert_eq!(strict.len(), 5, "{strict:?}");
    assert!(
        strict
            .iter()
            .all(|(_, kind, strength)| (kind.clone(), strength.clone()) == exact),
        "{strict:?}"
    );
}

/// Positive control (#4638 review): an AVA / tape receiver that does not
/// collide with an imported assertion binding keeps its own mapping.
#[test]
fn non_colliding_receiver_keeps_its_mapping() {
    let assertions = only_assertions(
        "test/cart.test.js",
        r#"
const assert = require('node:assert')

test('receiver', (t) => {
  t.is(total([1, 2]), 3)
})
"#,
    );
    assert_eq!(assertions.len(), 1, "{assertions:?}");
    assert_eq!(assertions[0].matcher, "is");
    assert_eq!(assertions[0].oracle_kind, OracleKind::ExactValue);
}

/// RIPR-SPEC-0243 rule 10 (example 14): a non-negated chai `throw` with one
/// string literal pins the error; a class, no argument, or `not` stays broad.
#[test]
fn chai_throw_string_literal_is_exact_error_variant() {
    let assertions = only_assertions(
        "test/parse.test.ts",
        r#"
import { expect } from "chai";
import { parse } from "../src/parse";

it("parse", () => {
  expect(() => parse("")).to.throw("blank");
  expect(() => parse("")).to.throw(TypeError);
  expect(() => parse("")).to.throw();
  expect(() => parse("x")).to.not.throw("blank");
});
"#,
    );
    assert_eq!(assertions.len(), 4, "{assertions:?}");
    assert_oracle(
        &assertions[0],
        OracleKind::ExactErrorVariant,
        OracleStrength::Strong,
        "expect(...).to.throw(\"blank\")",
    );
    for broad in &assertions[1..] {
        assert_eq!(broad.oracle_kind, OracleKind::BroadError, "{broad:?}");
        assert_eq!(broad.oracle_strength, OracleStrength::Weak, "{broad:?}");
        assert!(broad.error_payload.is_none(), "{broad:?}");
    }
}

/// RIPR-SPEC-0243 rule 10 (examples 34, 35): `node:assert` `throws` with an
/// anchored regex and `rejects` with a literal `message` object pin the
/// error; an unanchored or alternating regex, an object without `message`
/// and a string second argument (the assertion's own message) stay broad.
#[test]
fn node_assert_anchored_regex_and_message_object_are_exact_error_variants() {
    let assertions = only_assertions(
        "test/parse.test.ts",
        r#"
import test from "node:test";
import assert from "node:assert";
import { parse, charge } from "../src/parse";

test("parse", async () => {
  assert.throws(() => parse(""), /^Error: blank$/);
  await assert.rejects(charge(-1), { message: "charge must be positive" });
  assert.throws(() => parse(""), /blank/);
  assert.throws(() => parse(""), /^Error: (empty|blank)$/);
  await assert.rejects(charge(-1), { name: "Error" });
  assert.throws(() => parse(""), "blank");
  assert.throws(() => parse(""), /^Error: blank$/i);
  assert.throws(() => parse(""), /^.*$/);
  assert.throws(() => parse(""), /^Error: [\s\S]+$/);
  assert.throws(() => parse(""), /^.{0,}$/);
  assert.throws(() => parse(""), /^(?:.)*$/);
  assert.throws(() => parse(""), /^Error: \S+$/);
});
"#,
    );
    assert_eq!(assertions.len(), 12, "{assertions:?}");
    assert_oracle(
        &assertions[0],
        OracleKind::ExactErrorVariant,
        OracleStrength::Strong,
        "assert.throws(..., /^Error: blank$/)",
    );
    assert_oracle(
        &assertions[1],
        OracleKind::ExactErrorVariant,
        OracleStrength::Strong,
        "assert.rejects(..., { message: \"charge must be positive\" })",
    );
    for broad in &assertions[2..] {
        assert_eq!(broad.oracle_kind, OracleKind::BroadError, "{broad:?}");
        assert!(broad.error_payload.is_none(), "{broad:?}");
    }
}

/// chai's `assert.throws(fn, "msg")` has a different signature: rule 10
/// does not apply to it.
#[test]
fn chai_assert_throws_stays_broad() {
    let assertions = only_assertions(
        "test/parse.test.ts",
        r#"
import { assert } from "chai";
import { parse } from "../src/parse";

it("parse", () => {
  assert.throws(() => parse(""), /^blank$/);
});
"#,
    );
    assert_eq!(assertions.len(), 1, "{assertions:?}");
    assert_eq!(assertions[0].oracle_kind, OracleKind::BroadError);
}

/// RIPR-SPEC-0243 rule 3 (examples 7, 8, 9): a bare `expect` is read as the
/// Jest/Vitest matcher API only when it is the runner's.
#[test]
fn only_a_test_runner_expect_reads_jest_matchers() {
    let count = |source: &str| only_assertions("test/lib.test.ts", source).len();
    // Unbound global and a runner import: read.
    assert_eq!(
        count(
            "import { price } from \"../src/lib\";\ntest(\"p\", () => { expect(price(150)).toBe(130); });\n"
        ),
        1
    );
    assert_eq!(
        count(
            "import { expect, test } from \"vitest\";\nimport { price } from \"../src/lib\";\ntest(\"p\", () => { expect(price(150)).toBe(130); });\n"
        ),
        1
    );
    // Vitest test-context destructuring: read.
    assert_eq!(
        count(
            "import { test } from \"vitest\";\nimport { price } from \"../src/lib\";\ntest(\"p\", ({ expect }) => { expect(price(150)).toBe(130); });\n"
        ),
        1
    );
    // File-level local `expect`: not read.
    assert_eq!(
        count(
            "import { price } from \"../src/lib\";\nconst expect = (x: unknown) => ({ toBe(_: unknown) {} });\ntest(\"p\", () => { expect(price(150)).toBe(999); });\n"
        ),
        0
    );
    // chai's `expect` has no `toBe`: not read through the Jest table.
    assert_eq!(
        count(
            "import { expect } from \"chai\";\nimport { price } from \"../src/lib\";\nit(\"p\", () => { expect(price(150)).toBe(130); });\n"
        ),
        0
    );
    // A test-body declaration and another parameter shadow it too.
    assert_eq!(
        count(
            "import { price } from \"../src/lib\";\ntest(\"p\", () => { const expect = (x: unknown) => x; expect(price(150)).toBe(130); });\n"
        ),
        0
    );
    assert_eq!(
        count(
            "import { price } from \"../src/lib\";\ntest(\"p\", (expect) => { expect(price(150)).toBe(130); });\n"
        ),
        0
    );
}

/// RIPR-SPEC-0252 rule 1: `t.assert.<method>` on a `node:test` context reads
/// as legacy `node:assert`. `equal` is loose, so it is strong only against a
/// literal coercion cannot reach (RIPR-SPEC-0247 rule 2).
#[test]
fn node_test_context_assert_reads_as_legacy_node_assert() {
    let cases: [(&str, &str, OracleKind, OracleStrength, &str); 13] = [
        (
            "loose equal against a number",
            "t.assert.equal(lateFee(10), 20);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
            "t.assert.equal(...)",
        ),
        (
            "strictEqual",
            "t.assert.strictEqual(lateFee(10), 20);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
            "t.assert.strictEqual(...)",
        ),
        (
            "deepStrictEqual",
            "t.assert.deepStrictEqual(lateFee(10), 20);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
            "t.assert.deepStrictEqual(...)",
        ),
        (
            "loose equal against a test-local literal const",
            "const want = 20;\n  t.assert.equal(lateFee(10), want);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
            "t.assert.equal(...)",
        ),
        (
            "loose equal against a let variable stays weak",
            "let want = 20;\n  t.assert.equal(lateFee(10), want);",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
            "t.assert.equal(...)",
        ),
        (
            "string-literal computed member",
            "t[\"assert\"].equal(lateFee(10), 20);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
            "t[\"assert\"].equal(...)",
        ),
        (
            "single-quoted computed member",
            "t['assert'].equal(lateFee(10), 20);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
            "t[\"assert\"].equal(...)",
        ),
        (
            "awaited subtest reads its own literal const",
            "await t.test(\"sub\", (t2) => {\n    const want = 20;\n    t2.assert.equal(lateFee(10), want);\n  });",
            OracleKind::ExactValue,
            OracleStrength::Strong,
            "t2.assert.equal(...)",
        ),
        (
            "awaited subtest does not read the parent's const",
            "const want = 20;\n  await t.test(\"sub\", (t2) => {\n    t2.assert.equal(lateFee(10), want);\n  });",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
            "t2.assert.equal(...)",
        ),
        (
            "assertion inside try does not read the body's const",
            "const want = 20;\n  try {\n    t.assert.equal(lateFee(10), want);\n  } finally {}",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
            "t.assert.equal(...)",
        ),
        (
            "anchored throws pins the error",
            "t.assert.throws(() => lateFee(-1), /^Error: days must be positive$/);",
            OracleKind::ExactErrorVariant,
            OracleStrength::Strong,
            "t.assert.throws(..., /^Error: days must be positive$/)",
        ),
        (
            "awaited rejects with a literal message object",
            "await t.assert.rejects(async () => lateFee(-1), { message: \"days must be positive\" });",
            OracleKind::ExactErrorVariant,
            OracleStrength::Strong,
            "t.assert.rejects(..., { message: \"days must be positive\" })",
        ),
        (
            "awaited subtest receiver",
            "await t.test(\"sub\", (t2) => {\n    t2.assert.strictEqual(lateFee(10), 20);\n  });",
            OracleKind::ExactValue,
            OracleStrength::Strong,
            "t2.assert.strictEqual(...)",
        ),
    ];
    for (label, body, kind, strength, rendered) in cases {
        let source = format!(
            "import test from \"node:test\";\nimport {{ lateFee }} from \"../src/latefee\";\n\ntest(\"late fee\", async (t) => {{\n  {body}\n}});\n"
        );
        let assertions = only_assertions("test/latefee.test.ts", &source);
        let [assertion] = assertions.as_slice() else {
            assert_eq!(assertions.len(), 1, "{label}: {assertions:?}");
            continue;
        };
        assert_oracle(assertion, kind, strength, rendered);
        let observed = assertion.observed_expression.as_deref();
        assert!(
            observed.is_some_and(|observed| observed.contains("lateFee(")),
            "{label}: {observed:?}"
        );
    }
}

/// RIPR-SPEC-0252: each supported `node:test` registration form reads the
/// context assert; the same registration shadowed by a local `test` in the
/// enclosing `describe` reads nothing.
#[test]
fn node_test_context_assert_follows_the_registration_binding() {
    let call = "t.assert.equal(lateFee(10), 20);";
    for (label, imports, registration, credited) in [
        (
            "named it",
            "import { it } from \"node:test\";",
            "it(\"late fee\", (t) => {\n  CALL\n});",
            true,
        ),
        (
            "require member",
            "const test = require(\"node:test\").test;",
            "test(\"late fee\", (t) => {\n  CALL\n});",
            true,
        ),
        (
            "whole-module require",
            "const test = require(\"node:test\");",
            "test(\"late fee\", (t) => {\n  CALL\n});",
            true,
        ),
        (
            "destructured require",
            "const { test } = require(\"node:test\");",
            "test(\"late fee\", (t) => {\n  CALL\n});",
            true,
        ),
        (
            "only modifier",
            "import test from \"node:test\";",
            "test.only(\"late fee\", (t) => {\n  CALL\n});",
            true,
        ),
        (
            "it nested in describe",
            "import { describe, it } from \"node:test\";",
            "describe(\"fees\", () => {\n  it(\"late fee\", (t) => {\n    CALL\n  });\n});",
            true,
        ),
        (
            "callback body reuses the runner name",
            "import test from \"node:test\";",
            "test(\"late fee\", (t) => {\n  const test = 1;\n  CALL\n});",
            true,
        ),
        (
            "describe imported as test",
            "import { describe as test } from \"node:test\";",
            "test(\"late fee\", (t) => {\n  CALL\n});",
            false,
        ),
        (
            "test re-exported from node:test, imported from another runner",
            "export { test } from \"node:test\";\nimport { test } from \"./fixtures\";",
            "test(\"late fee\", (t) => {\n  CALL\n});",
            false,
        ),
        (
            "test shadowed in the enclosing describe",
            "import { describe, test } from \"node:test\";\nimport { makeRunner } from \"./fixtures\";",
            "describe(\"fees\", () => {\n  const test = makeRunner();\n  test(\"late fee\", (t) => {\n    CALL\n  });\n});",
            false,
        ),
    ] {
        let registration = registration.replace("CALL", call);
        let source =
            format!("{imports}\nimport {{ lateFee }} from \"../src/latefee\";\n\n{registration}\n");
        let tests = extract_tests(Path::new("test/latefee.test.ts"), &source);
        let read: Vec<_> = tests.iter().flat_map(|test| &test.assertions).collect();
        if credited {
            assert_eq!(tests.len(), 1, "{label}: {tests:?}");
            let [assertion] = read.as_slice() else {
                assert_eq!(read.len(), 1, "{label}: {read:?}");
                continue;
            };
            assert_oracle(
                assertion,
                OracleKind::ExactValue,
                OracleStrength::Strong,
                "t.assert.equal(...)",
            );
        } else {
            assert_eq!(tests.len(), 1, "{label}: {tests:?}");
            assert!(read.is_empty(), "{label}: {read:?}");
        }
    }
}

/// RIPR-SPEC-0252 negatives: without a `node:test` import, on a receiver
/// that is not the callback's first parameter, or for a method outside the
/// `node:assert` table, `x.assert.<method>` yields no assertion.
#[test]
fn node_test_context_assert_needs_the_runner_receiver_and_a_known_method() {
    for (label, imports, body) in [
        (
            "AVA file",
            "import test from \"ava\";",
            "t.assert.equal(lateFee(10), 20);",
        ),
        ("no runner import", "", "t.assert.equal(lateFee(10), 20);"),
        (
            "registered through another runner",
            "import { mock } from \"node:test\";\nimport { test } from \"./fixtures\";",
            "t.assert.equal(lateFee(10), 20);",
        ),
        (
            "type-only runner import",
            "import type { TestContext } from \"node:test\";",
            "t.assert.equal(lateFee(10), 20);",
        ),
        (
            "another receiver",
            "import test from \"node:test\";\nconst helper = { assert: { equal: () => {} } };",
            "helper.assert.equal(lateFee(10), 20);",
        ),
        (
            "another member of the context",
            "import test from \"node:test\";",
            "t.check.equal(lateFee(10), 20);",
        ),
        (
            "no strict member on the context assert",
            "import test from \"node:test\";",
            "t.assert.strict.equal(lateFee(10), 20);",
        ),
        (
            "snapshot is outside the table",
            "import test from \"node:test\";",
            "t.assert.snapshot(lateFee(10));",
        ),
        (
            "unknown method",
            "import test from \"node:test\";",
            "t.assert.verifies(lateFee(10), 20);",
        ),
        (
            "computed member through a variable key",
            "import test from \"node:test\";\nconst key = \"assert\";",
            "t[key].equal(lateFee(10), 20);",
        ),
        (
            "string-literal key naming another member",
            "import test from \"node:test\";",
            "t[\"check\"].equal(lateFee(10), 20);",
        ),
        (
            "computed member through a template key",
            "import test from \"node:test\";",
            "t[`assert`].equal(lateFee(10), 20);",
        ),
    ] {
        let source = format!(
            "{imports}\nimport {{ lateFee }} from \"../src/latefee\";\n\ntest(\"late fee\", (t) => {{\n  {body}\n}});\n"
        );
        let tests = extract_tests(Path::new("test/latefee.test.ts"), &source);
        assert_eq!(tests.len(), 1, "{label}: {tests:?}");
        assert!(
            tests[0].assertions.is_empty(),
            "{label}: {:?}",
            tests[0].assertions
        );
    }
}

/// RIPR-SPEC-0247 rule 2, amended: a loose equality whose expected value is
/// a `const` the test body declares once, at its top level and before the
/// assertion, bound to a literal coercion cannot reach, reads as that
/// literal. Any other binding keeps the variable reading.
#[test]
fn loose_equality_reads_a_test_local_literal_const() {
    for (label, body, credited, expected) in [
        (
            "string const",
            "const want = \"silver\";\n  assert.equal(tier(10), want);",
            true,
            Some("\"silver\""),
        ),
        (
            "number const",
            "const want = 30;\n  assert.equal(tier(10), want);",
            true,
            Some("30"),
        ),
        (
            "string const under deepEqual",
            "const want = \"silver\";\n  assert.deepEqual(tier(10), want);",
            true,
            Some("\"silver\""),
        ),
        (
            "parenthesized const with a type assertion",
            "const want = (30 as number);\n  assert.equal(tier(10), want);",
            true,
            Some("30"),
        ),
        (
            "object const under deepEqual can be mutated",
            "const want = { total: 30 };\n  want.total = tier(10);\n  assert.deepEqual(tier(10), want);",
            false,
            None,
        ),
        (
            "array const filled by the test",
            "const want = [];\n  want.push(tier(10));\n  assert.deepEqual(tiers([10]), want);",
            false,
            None,
        ),
        (
            "coercible number const",
            "const want = 1;\n  assert.equal(tier(10), want);",
            false,
            None,
        ),
        (
            "object const under equal compares references",
            "const want = { total: 30 };\n  assert.equal(tier(10), want);",
            false,
            None,
        ),
        (
            "let binding",
            "let want = 30;\n  assert.equal(tier(10), want);",
            false,
            None,
        ),
        (
            "const bound to a call",
            "const want = price(30);\n  assert.equal(tier(10), want);",
            false,
            None,
        ),
        (
            "const declared after the assertion",
            "assert.equal(tier(10), want);\n  const want = 30;",
            false,
            None,
        ),
        (
            "const declared in a nested block",
            "{\n    const want = 30;\n    assert.equal(tier(10), want);\n  }",
            false,
            None,
        ),
        (
            "assertion in a returned then callback",
            "const want = 30;\n  return load().then(() => {\n    assert.equal(tier(10), want);\n  });",
            false,
            None,
        ),
        (
            "assertion in a nested block a redeclaration could shadow",
            "const want = 30;\n  if (ready) {\n    assert.equal(tier(10), want);\n  }",
            false,
            None,
        ),
    ] {
        let source = format!(
            "import assert from \"node:assert\";\nimport {{ tier }} from \"../src/tier\";\n\nit(\"tier\", () => {{\n  {body}\n}});\n"
        );
        let assertions = only_assertions("test/tier.test.ts", &source);
        let [assertion] = assertions.as_slice() else {
            assert_eq!(assertions.len(), 1, "{label}: {assertions:?}");
            continue;
        };
        if credited {
            assert_eq!(assertion.oracle_kind, OracleKind::ExactValue, "{label}");
            assert_eq!(assertion.oracle_strength, OracleStrength::Strong, "{label}");
            assert!(!assertion.has_dynamic_matcher_arg, "{label}");
        } else {
            assert_eq!(
                assertion.oracle_kind,
                OracleKind::RelationalCheck,
                "{label}: {assertion:?}"
            );
            assert_eq!(assertion.oracle_strength, OracleStrength::Weak, "{label}");
        }
        if let Some(expected) = expected {
            assert_eq!(
                assertion.expected_value_or_variant.as_deref(),
                Some(expected),
                "{label}"
            );
        }
    }
}
