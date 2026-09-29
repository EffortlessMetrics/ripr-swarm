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
  assert.equal(total([1, 2]), 3)
  assert.isTrue(isEmpty([]))
  chai.assert.deepEqual(items(), ['a'])
})
"#,
    );
    assert_eq!(assertions.len(), 3, "{assertions:?}");
    assert_oracle(
        &assertions[0],
        OracleKind::ExactValue,
        OracleStrength::Strong,
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
