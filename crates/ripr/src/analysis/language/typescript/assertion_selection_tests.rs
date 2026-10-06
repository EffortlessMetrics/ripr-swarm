//! Family-relevant assertion selection controls (#5525, RIPR-SPEC-0224).
//!
//! Each case parses a real test source so the selector sees the extractor's
//! own kinds and strengths, asserts the parsed inventory first, then asserts
//! the selection. `old_differs` records whether the strength-only collapse
//! (`strongest_assertion`) picked something else, so every family-sensitive
//! case is discriminating against it.

use super::assertion_selection::{
    TypeScriptAssertionSelection, TypeScriptRowProjectionMove, row_projection_move,
    select_family_relevant_assertion,
};
use super::*;

const OWNER_FILE: &str = "src/discount.ts";
const OWNER_SOURCE: &str = "export function applyDiscount(amount: number): number {\n  if (amount < 0) {\n    throw new DiscountError('negative');\n  }\n  return amount * 0.9;\n}\n";
const RETURN_LINE: (usize, &str) = (5, "  return amount * 0.9;");
const THROW_LINE: (usize, &str) = (3, "    throw new DiscountError('negative');");
const JEST_IMPORT: &str = "import { applyDiscount, DiscountError } from '../src/discount';\n";

fn parse_test(header: &str, body: &str) -> Result<TypeScriptTest, String> {
    let source = format!("{header}{JEST_IMPORT}test('x', (t) => {{\n{body}}});\n");
    let mut tests = extract_tests(Path::new("tests/discount.test.ts"), &source);
    if tests.len() != 1 {
        return Err(format!(
            "fixture must parse to one test, got {}: {source}",
            tests.len()
        ));
    }
    Ok(tests.remove(0))
}

fn inventory(test: &TypeScriptTest) -> Vec<(OracleKind, OracleStrength)> {
    test.assertions
        .iter()
        .map(|assertion| {
            (
                assertion.oracle_kind.clone(),
                assertion.oracle_strength.clone(),
            )
        })
        .collect()
}

/// The identity of one assertion within its test: source line and matcher.
fn identity(assertion: &TypeScriptAssertion) -> (usize, String) {
    (assertion.line, assertion.matcher.clone())
}

fn selected(test: &TypeScriptTest, family: &ProbeFamily) -> Option<(usize, String)> {
    select_family_relevant_assertion(&test.assertions, Some(family))
        .assertion()
        .map(identity)
}

fn strength_only(test: &TypeScriptTest) -> Option<(usize, String)> {
    strongest_assertion(&test.assertions).map(identity)
}

struct Case {
    name: &'static str,
    family: ProbeFamily,
    /// Library import lines placed before the owner import.
    header: &'static str,
    body: &'static str,
    /// The parsed (kind, strength) inventory the case depends on.
    inventory: Vec<(OracleKind, OracleStrength)>,
    /// The selected assertion as (test-file line, matcher), or `None` for an
    /// explicit no-family-relevant result.
    expected: Option<(usize, &'static str)>,
    /// Whether the strength-only collapse picked something else.
    old_differs: bool,
}

const AVA: &str = "import test from 'ava';\n";
const NODE: &str = "import assert from 'node:assert';\n";
const CHAI: &str = "import { expect } from 'chai';\n";

// The test body starts on line 3 (line 1 imports the owner, line 2 opens the
// test), or on line 4 when a library import header comes first.
fn cases() -> Vec<Case> {
    use OracleKind as K;
    use OracleStrength as S;
    vec![
        Case {
            name: "return value: strong exact error + weaker value assertion -> value",
            family: ProbeFamily::ReturnValue,
            header: "",
            body: "  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n  expect(applyDiscount(100)).toBeGreaterThan(0);\n",
            inventory: vec![
                (K::ExactErrorVariant, S::Strong),
                (K::RelationalCheck, S::Weak),
            ],
            expected: Some((4, "toBeGreaterThan")),
            old_differs: true,
        },
        Case {
            name: "predicate: strong exact error + weaker value assertion -> value",
            family: ProbeFamily::Predicate,
            header: "",
            body: "  expect(applyDiscount(100)).toBeGreaterThan(0);\n  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n",
            inventory: vec![
                (K::RelationalCheck, S::Weak),
                (K::ExactErrorVariant, S::Strong),
            ],
            expected: Some((3, "toBeGreaterThan")),
            old_differs: true,
        },
        Case {
            name: "error path: strong exact value + broad error assertion -> error",
            family: ProbeFamily::ErrorPath,
            header: "",
            body: "  expect(() => applyDiscount(-1)).toThrow();\n  expect(applyDiscount(100)).toBe(90);\n",
            inventory: vec![(K::BroadError, S::Weak), (K::ExactValue, S::Strong)],
            expected: Some((3, "toThrow")),
            old_differs: true,
        },
        Case {
            name: "error path: a stronger mock expectation is not an error observer",
            family: ProbeFamily::ErrorPath,
            header: "",
            body: "  expect(() => applyDiscount(-1)).toThrow();\n  expect(notify).toHaveBeenCalledWith(1);\n",
            inventory: vec![(K::BroadError, S::Weak), (K::MockExpectation, S::Medium)],
            expected: Some((3, "toThrow")),
            old_differs: true,
        },
        Case {
            name: "error path: a snapshot observes the thrown path ahead of a weaker broad error",
            family: ProbeFamily::ErrorPath,
            header: "",
            body: "  expect(applyDiscount(100)).toBe(90);\n  expect(() => applyDiscount(-1)).toThrow();\n  expect(applyDiscount(-1)).toMatchSnapshot();\n",
            inventory: vec![
                (K::ExactValue, S::Strong),
                (K::BroadError, S::Weak),
                (K::Snapshot, S::Medium),
            ],
            expected: Some((5, "toMatchSnapshot")),
            old_differs: true,
        },
        Case {
            name: "same family, different sinks: one assertion, the later line",
            family: ProbeFamily::ReturnValue,
            header: "",
            body: "  expect(applyDiscount(100)).toBe(90);\n  expect(applyDiscount(200)).toBe(180);\n",
            inventory: vec![(K::ExactValue, S::Strong), (K::ExactValue, S::Strong)],
            expected: Some((4, "toBe")),
            old_differs: false,
        },
        Case {
            name: "return value: only an exact error assertion -> none, not the error",
            family: ProbeFamily::ReturnValue,
            header: "",
            body: "  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n",
            inventory: vec![(K::ExactErrorVariant, S::Strong)],
            expected: None,
            old_differs: true,
        },
        Case {
            name: "error path: only an exact value assertion -> none, not the value",
            family: ProbeFamily::ErrorPath,
            header: "",
            body: "  expect(applyDiscount(100)).toBe(90);\n",
            inventory: vec![(K::ExactValue, S::Strong)],
            expected: None,
            old_differs: true,
        },
        Case {
            name: "field construction: exact error + whole-object equality -> object",
            family: ProbeFamily::FieldConstruction,
            header: "",
            body: "  expect(applyDiscount(100)).toEqual({ total: 90 });\n  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n",
            inventory: vec![
                (K::ExactValue, S::Strong),
                (K::ExactErrorVariant, S::Strong),
            ],
            expected: Some((3, "toEqual")),
            old_differs: true,
        },
        Case {
            name: "side effect: every kind applies, the strongest wins",
            family: ProbeFamily::SideEffect,
            header: "",
            body: "  expect(notify).toHaveBeenCalledWith(1);\n  expect(applyDiscount(100)).toBe(90);\n",
            inventory: vec![(K::MockExpectation, S::Medium), (K::ExactValue, S::Strong)],
            expected: Some((4, "toBe")),
            old_differs: false,
        },
        Case {
            name: "static unknown: fail-open, the strongest wins",
            family: ProbeFamily::StaticUnknown,
            header: "",
            body: "  expect(applyDiscount(100)).toBe(90);\n  expect(() => applyDiscount(-1)).toThrow();\n",
            inventory: vec![(K::ExactValue, S::Strong), (K::BroadError, S::Weak)],
            expected: Some((3, "toBe")),
            old_differs: false,
        },
        Case {
            name: "AVA: error path takes t.throws over a stronger t.is",
            family: ProbeFamily::ErrorPath,
            header: AVA,
            body: "  t.is(applyDiscount(100), 90);\n  t.throws(() => applyDiscount(-1));\n",
            inventory: vec![(K::ExactValue, S::Strong), (K::BroadError, S::Weak)],
            expected: Some((5, "throws")),
            old_differs: true,
        },
        Case {
            name: "node assert: return value takes assert.ok over a stronger assert.throws",
            family: ProbeFamily::ReturnValue,
            header: NODE,
            body: "  assert.throws(() => applyDiscount(-1));\n  assert.ok(applyDiscount(100) > 0);\n",
            inventory: vec![(K::BroadError, S::Weak), (K::SmokeOnly, S::Smoke)],
            expected: Some((5, "ok")),
            old_differs: true,
        },
        Case {
            name: "chai: error path takes .to.throw() over a stronger .to.equal()",
            family: ProbeFamily::ErrorPath,
            header: CHAI,
            body: "  expect(applyDiscount(100)).to.equal(90);\n  expect(() => applyDiscount(-1)).to.throw();\n",
            inventory: vec![(K::ExactValue, S::Strong), (K::BroadError, S::Weak)],
            expected: Some((5, "throw")),
            old_differs: true,
        },
    ]
}

#[test]
fn family_relevant_assertion_selection_controls() -> Result<(), String> {
    for case in cases() {
        let test = parse_test(case.header, case.body)?;
        let parsed = inventory(&test);
        if parsed != case.inventory {
            return Err(format!(
                "{}: parsed inventory {parsed:?} (matchers {:?}) is not the intended {:?}",
                case.name,
                test.assertions
                    .iter()
                    .map(|assertion| (assertion.line, assertion.matcher.as_str()))
                    .collect::<Vec<_>>(),
                case.inventory
            ));
        }
        let expected = case
            .expected
            .map(|(line, matcher)| (line, matcher.to_string()));
        let got = selected(&test, &case.family);
        if got != expected {
            return Err(format!(
                "{}: selected {got:?}, expected {expected:?}",
                case.name
            ));
        }
        if case.expected.is_none()
            && select_family_relevant_assertion(&test.assertions, Some(&case.family))
                != TypeScriptAssertionSelection::NoFamilyRelevant
        {
            return Err(format!(
                "{}: expected an explicit NoFamilyRelevant",
                case.name
            ));
        }
        let old = strength_only(&test);
        if (old != expected) != case.old_differs {
            return Err(format!(
                "{}: strength-only pick {old:?} vs expected {expected:?} does not match old_differs={}",
                case.name, case.old_differs
            ));
        }
    }
    Ok(())
}

#[test]
fn an_empty_inventory_selects_no_assertion() {
    assert_eq!(
        select_family_relevant_assertion(&[], Some(&ProbeFamily::ReturnValue)),
        TypeScriptAssertionSelection::NoAssertion
    );
    assert_eq!(
        select_family_relevant_assertion(&[], None),
        TypeScriptAssertionSelection::NoAssertion
    );
}

/// Every rotation and the reversal of each case's assertion inventory selects
/// the same assertion: the inventory order never decides.
#[test]
fn reordering_assertions_never_changes_the_selection() -> Result<(), String> {
    for case in cases() {
        let test = parse_test(case.header, case.body)?;
        let baseline = selected(&test, &case.family);
        let count = test.assertions.len();
        let mut orders: Vec<Vec<TypeScriptAssertion>> = (0..count)
            .map(|shift| {
                let mut rotated = test.assertions.clone();
                rotated.rotate_left(shift);
                rotated
            })
            .collect();
        let mut reversed = test.assertions.clone();
        reversed.reverse();
        orders.push(reversed);
        for order in orders {
            let got = select_family_relevant_assertion(&order, Some(&case.family))
                .assertion()
                .map(identity);
            if got != baseline {
                return Err(format!(
                    "{}: reordered inventory selected {got:?}, baseline {baseline:?}",
                    case.name
                ));
            }
        }
    }
    Ok(())
}

/// Two equal family-relevant assertions in reversed source order select by
/// the documented tie-break (the later source line), and the selected row
/// carries that one assertion's observed expression and expected value —
/// facts are never merged across assertions.
#[test]
fn equal_family_relevant_assertions_tie_break_on_the_later_source_line() -> Result<(), String> {
    let forward = parse_test(
        "",
        "  expect(applyDiscount(100)).toBe(90);\n  expect(applyDiscount(200)).toBe(180);\n",
    )?;
    let reversed = parse_test(
        "",
        "  expect(applyDiscount(200)).toBe(180);\n  expect(applyDiscount(100)).toBe(90);\n",
    )?;
    for (test, observed, expected) in [
        (&forward, "applyDiscount(200)", "180"),
        (&reversed, "applyDiscount(100)", "90"),
    ] {
        let assertion =
            select_family_relevant_assertion(&test.assertions, Some(&ProbeFamily::ReturnValue))
                .assertion()
                .ok_or_else(|| "expected a selected assertion".to_string())?;
        assert_eq!(assertion.line, 4, "the later source line wins");
        assert_eq!(assertion.observed_expression.as_deref(), Some(observed));
        assert_eq!(
            assertion.expected_value_or_variant.as_deref(),
            Some(expected)
        );
    }
    Ok(())
}

/// Identical assertions on one line (same strength, line, rendered text)
/// still select deterministically through the remaining fact keys.
#[test]
fn same_line_equal_assertions_select_by_their_own_facts() -> Result<(), String> {
    let test = parse_test(
        "",
        "  expect(applyDiscount(200)).toBe(180); expect(applyDiscount(100)).toBe(90);\n",
    )?;
    let forward =
        select_family_relevant_assertion(&test.assertions, Some(&ProbeFamily::ReturnValue))
            .assertion()
            .ok_or_else(|| "expected a selected assertion".to_string())?;
    let mut reversed_inventory = test.assertions.clone();
    reversed_inventory.reverse();
    let reversed =
        select_family_relevant_assertion(&reversed_inventory, Some(&ProbeFamily::ReturnValue))
            .assertion()
            .ok_or_else(|| "expected a selected assertion".to_string())?;
    assert_eq!(forward, reversed);
    assert_eq!(
        forward.observed_expression.as_deref(),
        Some("applyDiscount(100)")
    );
    assert_eq!(forward.expected_value_or_variant.as_deref(), Some("90"));
    Ok(())
}

#[test]
fn row_projection_moves_only_when_the_row_strength_moves() -> Result<(), String> {
    use TypeScriptRowProjectionMove as Move;
    let cases: &[(&str, ProbeFamily, &str, Option<Move>)] = &[
        (
            "stronger exact error passed over for a weaker value",
            ProbeFamily::ReturnValue,
            "  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n  expect(applyDiscount(100)).toBeGreaterThan(0);\n",
            Some(Move::OtherBehaviorAssertionPassedOver),
        ),
        (
            "only wrong-family assertions",
            ProbeFamily::ErrorPath,
            "  expect(applyDiscount(100)).toBe(90);\n",
            Some(Move::NoFamilyRelevant),
        ),
        (
            "equal strength, different kind: target choice unchanged",
            ProbeFamily::ReturnValue,
            "  expect(applyDiscount(100)).toBe(90);\n  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n",
            None,
        ),
        (
            "the strongest assertion is already family-relevant",
            ProbeFamily::ReturnValue,
            "  expect(() => applyDiscount(-1)).toThrow();\n  expect(applyDiscount(100)).toBe(90);\n",
            None,
        ),
    ];
    for (name, family, body, expected) in cases {
        let test = parse_test("", body)?;
        let got = row_projection_move(&test.assertions, family);
        if got != *expected {
            return Err(format!("{name}: move {got:?}, expected {expected:?}"));
        }
    }
    assert_eq!(
        TypeScriptRowProjectionMove::NoFamilyRelevant.evidence_value(&ProbeFamily::ErrorPath),
        "no_error_path_relevant_assertion"
    );
    Ok(())
}

// ── End-to-end through the classifier ────────────────────────────────────────

fn classify(line: (usize, &str), test_bodies: &[&str]) -> Result<Finding, String> {
    let owners = extract_owners(Path::new(OWNER_FILE), OWNER_SOURCE);
    let tests = test_bodies
        .iter()
        .map(|body| parse_test("", body))
        .collect::<Result<Vec<_>, _>>()?;
    classify_change(
        Path::new(OWNER_FILE),
        line.0,
        line.1,
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "expected a finding".to_string())
}

fn row_oracle(finding: &Finding) -> Result<(OracleKind, OracleStrength, Option<String>), String> {
    let row = finding
        .related_tests
        .first()
        .ok_or_else(|| "expected one related row".to_string())?;
    Ok((
        row.oracle_kind.clone(),
        row.oracle_strength.clone(),
        row.oracle.clone(),
    ))
}

fn selection_evidence(finding: &Finding) -> Vec<&str> {
    finding
        .evidence
        .iter()
        .map(String::as_str)
        .filter(|line| line.starts_with("typescript_assertion_selection: "))
        .collect()
}

/// The row names the assertion classification judged: an `Exposed` return
/// value whose exact value is followed by a stronger-or-equal exact error
/// shows the exact value, not the error.
#[test]
fn classified_row_names_the_assertion_classification_judged() -> Result<(), String> {
    let finding = classify(
        RETURN_LINE,
        &[
            "  expect(applyDiscount(100)).toBe(90);\n  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n",
        ],
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    let (kind, strength, _) = row_oracle(&finding)?;
    assert_eq!(kind, OracleKind::ExactValue);
    assert_eq!(strength, OracleStrength::Strong);
    assert!(
        finding
            .ripr
            .reveal
            .observe
            .summary
            .contains("`exact_value`"),
        "observe summary: {}",
        finding.ripr.reveal.observe.summary
    );
    assert!(selection_evidence(&finding).is_empty());
    Ok(())
}

/// Adding a stronger wrong-family assertion to an otherwise unchanged test
/// leaves the selected row evidence and the finding verdict unchanged; only
/// the move disclosure is added.
#[test]
fn wrong_family_stronger_assertion_leaves_row_and_verdict_unchanged() -> Result<(), String> {
    let base = classify(
        RETURN_LINE,
        &["  expect(applyDiscount(100)).toBeGreaterThan(0);\n"],
    )?;
    let with_error = classify(
        RETURN_LINE,
        &[
            "  expect(applyDiscount(100)).toBeGreaterThan(0);\n  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n",
        ],
    )?;
    assert_eq!(base.class, ExposureClass::WeaklyExposed);
    assert_eq!(with_error.class, base.class);
    assert_eq!(row_oracle(&with_error)?, row_oracle(&base)?);
    assert_eq!(
        row_oracle(&base)?.0,
        OracleKind::RelationalCheck,
        "the row shows the value assertion"
    );
    assert_eq!(with_error.ripr.reveal.observe, base.ripr.reveal.observe);
    assert_eq!(
        with_error.ripr.reveal.discriminate,
        base.ripr.reveal.discriminate
    );
    assert_eq!(with_error.missing, base.missing);
    assert!(selection_evidence(&base).is_empty());
    assert_eq!(
        selection_evidence(&with_error),
        vec!["typescript_assertion_selection: other_behavior_assertion_passed_over (x)"]
    );
    Ok(())
}

/// A changed throw whose only related assertion is a normal value shows no
/// oracle on the row instead of the wrong-family exact value, and discloses
/// it; the verdict stays what the family-aware classifier already decided.
#[test]
fn a_test_with_only_wrong_family_assertions_shows_no_oracle() -> Result<(), String> {
    let finding = classify(THROW_LINE, &["  expect(applyDiscount(100)).toBe(90);\n"])?;
    assert_eq!(finding.probe.family, ProbeFamily::ErrorPath);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        row_oracle(&finding)?,
        (OracleKind::Unknown, OracleStrength::Unknown, None)
    );
    assert_eq!(
        selection_evidence(&finding),
        vec!["typescript_assertion_selection: no_error_path_relevant_assertion (x)"]
    );
    Ok(())
}

/// The strongest row equals the aggregate strongest-family result the
/// classifier reports: row projection and classification read one rule.
#[test]
fn strongest_row_matches_the_aggregate_family_result() -> Result<(), String> {
    for (line, bodies) in [
        (
            RETURN_LINE,
            vec![
                "  expect(() => applyDiscount(-1)).toThrow(DiscountError);\n  expect(applyDiscount(100)).toBeGreaterThan(0);\n",
                "  expect(applyDiscount(100)).toMatchSnapshot();\n",
            ],
        ),
        (
            THROW_LINE,
            vec![
                "  expect(applyDiscount(100)).toBe(90);\n  expect(() => applyDiscount(-1)).toThrow();\n",
            ],
        ),
    ] {
        let finding = classify(line, &bodies)?;
        let row_rank = finding
            .related_tests
            .iter()
            .map(|row| row.oracle_strength.rank())
            .max()
            .unwrap_or(0);
        let summary = &finding.ripr.reveal.observe.summary;
        assert!(
            summary.ends_with(&format!("(rank {row_rank})")),
            "aggregate `{summary}` vs strongest row rank {row_rank}"
        );
    }
    Ok(())
}
