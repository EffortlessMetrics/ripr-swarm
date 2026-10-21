//! Row-local related-test miss controls (#5495, RIPR-SPEC-0224).
//!
//! Each case writes a real owner module and test file under a temporary
//! workspace root and classifies the changed line through the production
//! `classify_change` path, so rows carry the miss the classifier assigned.
//! Cases assert the fixture first (class, relation, admission) and then the
//! per-row miss.

use super::tests::ts_unique_tempdir;
use super::*;
use crate::domain::RelatedTestMiss;

const OWNER_FILE: &str = "src/pricing.ts";
const TEST_FILE: &str = "tests/pricing.test.ts";
const OWNER_SOURCE: &str = concat!(
    "export function applyDiscount(total: number): number {\n", // 1
    "  if (total >= 100) {\n",                                  // 2
    "    return total * 0.9;\n",                                // 3
    "  }\n",                                                    // 4
    "  return total;\n",                                        // 5
    "}\n",                                                      // 6
    "\n",                                                       // 7
    "function calc(n: number): number {\n",                     // 8
    "  return n * 2;\n",                                        // 9
    "}\n",                                                      // 10
    "\n",                                                       // 11
    "export function price(n: number): number {\n",             // 12
    "  return calc(n);\n",                                      // 13
    "}\n",                                                      // 14
);
const IMPORT: &str = "import { applyDiscount } from '../src/pricing';\n";

struct Change {
    line: usize,
    text: &'static str,
}

const RETURN_CHANGE: Change = Change {
    line: 3,
    text: "    return total * 0.9;",
};
const PREDICATE_CHANGE: Change = Change {
    line: 2,
    text: "  if (total >= 100) {",
};
const CALC_CHANGE: Change = Change {
    line: 9,
    text: "  return n * 2;",
};

/// The finding classified from `test_source`, with the tempdir removed.
fn classify(change: &Change, test_source: &str) -> Result<Finding, String> {
    classify_file(change, TEST_FILE, test_source)
}

fn classify_file(change: &Change, test_file: &str, test_source: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir("row-miss")?;
    let result = classify_in(&root, change, test_file, test_source);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn classify_in(
    root: &Path,
    change: &Change,
    test_file: &str,
    test_source: &str,
) -> Result<Finding, String> {
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("mkdir src: {err}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|err| format!("mkdir tests: {err}"))?;
    std::fs::write(root.join(OWNER_FILE), OWNER_SOURCE)
        .map_err(|err| format!("write owner: {err}"))?;
    std::fs::write(root.join(test_file), test_source)
        .map_err(|err| format!("write test: {err}"))?;
    let module_line = OWNER_SOURCE.lines().nth(change.line - 1);
    if module_line != Some(change.text) {
        return Err(format!(
            "fixture line {} is {module_line:?}, not {:?}",
            change.line, change.text
        ));
    }
    let owners = extract_owners(Path::new(OWNER_FILE), OWNER_SOURCE);
    let tests = extract_tests(Path::new(test_file), test_source);
    if tests.is_empty() {
        return Err(format!("fixture parsed to no tests: {test_source}"));
    }
    classify_change(
        Path::new(OWNER_FILE),
        change.line,
        change.text,
        &owners,
        &tests,
        Some(root),
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "fixture produced no finding".to_string())
}

fn miss_of(finding: &Finding, test: &str) -> Result<Option<RelatedTestMiss>, String> {
    finding
        .related_tests
        .iter()
        .find(|row| row.name == test)
        .map(|row| row.miss)
        .ok_or_else(|| {
            format!(
                "no row `{test}`; rows: {:?}",
                finding
                    .related_tests
                    .iter()
                    .map(|row| &row.name)
                    .collect::<Vec<_>>()
            )
        })
}

/// The relation the production candidate builder assigns `test`.
fn relation_of(
    change: &Change,
    test_source: &str,
    test: &str,
) -> Result<TypeScriptRelationKind, String> {
    let root = ts_unique_tempdir("row-miss-relation")?;
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("mkdir src: {err}"))?;
    std::fs::write(root.join(OWNER_FILE), OWNER_SOURCE)
        .map_err(|err| format!("write owner: {err}"))?;
    let owners = extract_owners(Path::new(OWNER_FILE), OWNER_SOURCE);
    let owner = owners
        .iter()
        .find(|owner| change.line >= owner.start_line && change.line <= owner.end_line)
        .ok_or("owner not extracted")?;
    let tests = extract_tests(Path::new(TEST_FILE), test_source);
    let candidates =
        related_test_candidates(owner, &tests, Some(&root), &ReExportIndex::empty(), None);
    let _ = std::fs::remove_dir_all(&root);
    candidates
        .iter()
        .find(|candidate| candidate.test.name == test)
        .map(|candidate| candidate.relation)
        .ok_or_else(|| format!("no candidate `{test}`"))
}

fn weakly(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
}

/// Heuristic name-only row names the missing path.
#[test]
fn heuristic_reference_row_is_no_call_path() -> Result<(), String> {
    let source = format!(
        "{IMPORT}test('works', () => {{\n  const fn = applyDiscount;\n  expect(typeof fn).toBe('function');\n}});\n"
    );
    assert!(
        relation_of(&RETURN_CHANGE, &source, "works")?.is_uncertain(),
        "fixture must relate by heuristic only"
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "works")?,
        Some(RelatedTestMiss::NoCallPath)
    );
    Ok(())
}

/// Established absence of any assertion-like construct on a real owner-call
/// path still gets no `no_assertion`: the single-file admission scan cannot
/// see setup files or runner plugins (`admission.rs` blind spots).
#[test]
fn established_no_assertion_row_keeps_no_miss() -> Result<(), String> {
    let source = format!("{IMPORT}test('calls only', () => {{\n  applyDiscount(1);\n}});\n");
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(miss_of(&finding, "calls only")?, None);
    Ok(())
}

/// Discriminating negative: a custom matcher is assertion-like but not
/// admitted, so the row keeps no fabricated reason.
#[test]
fn custom_matcher_row_is_not_no_assertion() -> Result<(), String> {
    let source =
        format!("{IMPORT}test('custom', () => {{\n  expect(applyDiscount(1)).toBeEven();\n}});\n");
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(miss_of(&finding, "custom")?, None);
    Ok(())
}

/// Discriminating negative: a same-file helper may assert, so admission is
/// unresolved and the row keeps no reason.
#[test]
fn helper_call_row_is_not_no_assertion() -> Result<(), String> {
    let source = format!(
        "{IMPORT}function check(v: number) {{ if (v !== 1) throw new Error('bad'); }}\ntest('helper', () => {{\n  check(applyDiscount(1));\n}});\n"
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(miss_of(&finding, "helper")?, None);
    Ok(())
}

#[test]
fn weak_family_assertion_row_is_weak_assertion() -> Result<(), String> {
    let source = format!(
        "{IMPORT}test('truthy', () => {{\n  expect(applyDiscount(1)).toBeTruthy();\n}});\n"
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "truthy")?,
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

/// Two rows in one finding keep their own reason; neither borrows the other.
#[test]
fn mixed_rows_keep_their_own_miss() -> Result<(), String> {
    let source = format!(
        "{IMPORT}test('truthy', () => {{\n  expect(applyDiscount(1)).toBeTruthy();\n}});\ntest('bare', () => {{\n  applyDiscount(2);\n}});\ntest('custom', () => {{\n  expect(applyDiscount(3)).toBeEven();\n}});\n"
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "truthy")?,
        Some(RelatedTestMiss::WeakAssertion)
    );
    assert_eq!(miss_of(&finding, "bare")?, None);
    assert_eq!(miss_of(&finding, "custom")?, None);
    Ok(())
}

/// A strong assertion whose own input misses the boundary the owner pins.
/// `missing_input` is withheld (its shared prose over-claims), so the row
/// reports the unconfirmed observation.
#[test]
fn strong_row_off_the_boundary_is_observation_unconfirmed() -> Result<(), String> {
    let source =
        format!("{IMPORT}test('below', () => {{\n  expect(applyDiscount(50)).toBe(50);\n}});\n");
    let finding = classify(&PREDICATE_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "below")?,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

/// Another test's boundary input is never borrowed: the missing-input row
/// keeps its reason beside a weak row that does supply the boundary.
#[test]
fn boundary_input_from_another_row_is_not_borrowed() -> Result<(), String> {
    let source = format!(
        "{IMPORT}test('below', () => {{\n  expect(applyDiscount(50)).toBe(50);\n}});\ntest('at boundary weak', () => {{\n  expect(applyDiscount(100)).toBeTruthy();\n}});\n"
    );
    let finding = classify(&PREDICATE_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "below")?,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    assert_eq!(
        miss_of(&finding, "at boundary weak")?,
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

/// A strong assertion of another behavior family (an error variant for a
/// changed return) observes another sink.
#[test]
fn strong_wrong_family_row_is_observation_unconfirmed() -> Result<(), String> {
    let source = format!(
        "{IMPORT}test('throws', () => {{\n  expect(() => applyDiscount(1)).toThrow(RangeError);\n}});\n"
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "throws")?,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

/// The reason follows the family-relevant assertion, not the stronger
/// wrong-family one.
#[test]
fn wrong_family_strong_plus_relevant_weak_follows_the_relevant_assertion() -> Result<(), String> {
    let source = format!(
        "{IMPORT}test('both', () => {{\n  expect(() => applyDiscount(1)).toThrow(RangeError);\n  expect(applyDiscount(1)).toBeTruthy();\n}});\n"
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "both")?,
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

/// Module-entry reach is a real path: never `no_call_path`. A strong
/// entry assertion cannot confirm the owner's effect.
#[test]
fn module_entry_row_is_observation_unconfirmed_never_no_call_path() -> Result<(), String> {
    let source = "import { price } from '../src/pricing';\ntest('via entry', () => {\n  expect(price(2)).toBe(4);\n});\n";
    let finding = classify(&CALC_CHANGE, source)?;
    weakly(&finding);
    assert_eq!(
        relation_of(&CALC_CHANGE, source, "via entry")?,
        TypeScriptRelationKind::ModuleEntryCall
    );
    assert_eq!(
        miss_of(&finding, "via entry")?,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

/// Module-entry reach with an unresolved assertion keeps no reason: the
/// path is real, but no admitted assertion relates to it.
#[test]
fn module_entry_row_with_unresolved_assertion_has_no_miss() -> Result<(), String> {
    let source = "import { price } from '../src/pricing';\ntest('via entry', () => {\n  expect(price(2)).toBeEven();\n});\n";
    let finding = classify(&CALC_CHANGE, source)?;
    weakly(&finding);
    assert_eq!(
        relation_of(&CALC_CHANGE, source, "via entry")?,
        TypeScriptRelationKind::ModuleEntryCall
    );
    assert_eq!(miss_of(&finding, "via entry")?, None);
    Ok(())
}

/// Class gate: an exposed finding keeps every row at `None`.
#[test]
fn exposed_finding_rows_have_no_miss() -> Result<(), String> {
    let source = format!(
        "{IMPORT}test('exact', () => {{\n  expect(applyDiscount(100)).toBe(90);\n}});\ntest('bare', () => {{\n  expect(applyDiscount(2)).toBeTruthy();\n}});\n"
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // The weak row would be `weak_assertion` inside a gap finding.
    assert!(
        finding.related_tests.iter().all(|row| row.miss.is_none()),
        "{:?}",
        finding.related_tests
    );
    Ok(())
}

/// Class gate on the mapper itself: the same rows that earn a reason inside a
/// `weakly_exposed` finding earn none under any other class or a static limit.
#[test]
fn class_gate_withholds_every_row_miss() -> Result<(), String> {
    let source =
        format!("{IMPORT}test('bare', () => {{\n  expect(applyDiscount(1)).toBeTruthy();\n}});\n");
    let root = ts_unique_tempdir("row-miss-gate")?;
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("mkdir: {err}"))?;
    let owner = extract_owners(Path::new(OWNER_FILE), OWNER_SOURCE)
        .into_iter()
        .find(|owner| owner.name == "applyDiscount")
        .ok_or("owner not extracted")?;
    let tests = extract_tests(Path::new(TEST_FILE), &source);
    let candidates =
        related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(candidates.len(), 1);
    let shape = classify_probe_shape_detail(RETURN_CHANGE.text);
    let mock_facts = MockCallFacts::read(RETURN_CHANGE.text, None, &owner.name, &owner.params);
    let pin_facts = PinFacts::default();
    let run = |class: ExposureClass, static_limited: bool| {
        let mut rows = related_tests_for_candidates(&candidates, Some(&shape.family));
        assign_row_misses(
            &TsRowMissContext {
                class: &class,
                static_limited,
                probe_shape: &shape,
                line_text: RETURN_CHANGE.text,
                owner: &owner,
                alias_map: None,
                workspace_root: None,
                credits: CreditFacts {
                    mock_facts: &mock_facts,
                    pin_facts: &pin_facts,
                },
            },
            &candidates,
            &[TypeScriptPredicateActivation::NotApplicable],
            &mut rows,
        );
        rows.into_iter().map(|row| row.miss).collect::<Vec<_>>()
    };
    assert_eq!(
        run(ExposureClass::WeaklyExposed, false),
        vec![Some(RelatedTestMiss::WeakAssertion)]
    );
    assert_eq!(run(ExposureClass::WeaklyExposed, true), vec![None]);
    assert_eq!(run(ExposureClass::Exposed, false), vec![None]);
    assert_eq!(run(ExposureClass::StaticUnknown, false), vec![None]);
    assert_eq!(run(ExposureClass::NoStaticPath, false), vec![None]);
    Ok(())
}

fn rejected_source(body: &str) -> String {
    format!("import {{ applyDiscount }} from '../src/pricing';\n{body}")
}

/// A body-local declaration shadows the owner name: the call reaches the
/// local, so the row has no path to the changed owner.
#[test]
fn local_shadow_row_is_no_call_path() -> Result<(), String> {
    let source = "import { applyDiscount as ad } from '../src/pricing';\ntest('discounts', () => {\n  function applyDiscount(x: number) { return 42; }\n  expect(ad).toBeDefined();\n  expect(applyDiscount(100)).toBe(42);\n});\n";
    let finding = classify(&RETURN_CHANGE, source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "discounts")?,
        Some(RelatedTestMiss::NoCallPath)
    );
    Ok(())
}

#[test]
fn unrelated_destructure_row_is_no_call_path() -> Result<(), String> {
    let source = rejected_source(
        "test('discounts', () => {\n  const { applyDiscount } = require('../src/factory');\n  expect(applyDiscount(100)).toBe(90);\n});\n",
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "discounts")?,
        Some(RelatedTestMiss::NoCallPath)
    );
    Ok(())
}

/// A mocked owner module or a fabricated spy value is a call to a
/// replacement. It is never `no_call_path` (its prose says no call was
/// found), and never `assertion_not_observing`.
#[test]
fn mock_and_spy_rows_are_never_no_call_path_or_not_observing() -> Result<(), String> {
    let mock = rejected_source(
        "vi.mock('../src/pricing');\ntest('discounts', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n",
    );
    let spy = "import * as pricing from '../src/pricing';\nimport { applyDiscount } from '../src/pricing';\ntest('discounts', () => {\n  vi.spyOn(pricing, 'applyDiscount').mockReturnValue(42);\n  expect(applyDiscount(100)).toBe(42);\n});\n".to_string();
    // The mock registration is assertion-like setup, so admission is
    // unresolved and the row keeps no reason; the spy row has an admitted
    // assertion whose observation of the owner is unconfirmed.
    for (source, expected) in [
        (mock, None),
        (spy, Some(RelatedTestMiss::ObservationUnconfirmed)),
    ] {
        let finding = classify(&RETURN_CHANGE, &source)?;
        weakly(&finding);
        assert_eq!(miss_of(&finding, "discounts")?, expected, "{source}");
    }
    Ok(())
}

/// A bare owner-name call with no declaration anchor: identity is possible
/// but unconfirmed.
#[test]
fn unanchored_owner_name_row_is_observation_unconfirmed() -> Result<(), String> {
    let source =
        "test('applyDiscount works', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n";
    let finding = classify_file(&RETURN_CHANGE, "tests/helper.test.ts", source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "applyDiscount works")?,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

#[test]
fn unresolved_alias_row_is_observation_unconfirmed() -> Result<(), String> {
    let source = "test('discounts', () => {\n  const { applyDiscount } = require('@app/pricing');\n  expect(applyDiscount(100)).toBe(90);\n});\n";
    let finding = classify(&RETURN_CHANGE, source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "discounts")?,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

/// A weak assertion of another family says nothing about any sink.
#[test]
fn weak_only_wrong_family_row_keeps_no_miss() -> Result<(), String> {
    let source = rejected_source(
        "test('throws', () => {\n  expect(() => applyDiscount(1)).toThrow();\n});\n",
    );
    let finding = classify(&RETURN_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(miss_of(&finding, "throws")?, None);
    Ok(())
}

/// A strong predicate row whose boundary input is computed is not the
/// witness; its own observation stays unconfirmed.
#[test]
fn non_witness_predicate_row_is_observation_unconfirmed() -> Result<(), String> {
    let source = rejected_source(
        "test('computed', () => {\n  expect(applyDiscount(Number('100'))).toBe(90);\n});\n",
    );
    let finding = classify(&PREDICATE_CHANGE, &source)?;
    weakly(&finding);
    assert_eq!(
        miss_of(&finding, "computed")?,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

/// The miss is non-decisional: no row is `is_unmatched`, and the decision
/// readers (gap next step, witness) give the same answer with the misses
/// stripped. Row order is the candidate order and is compared too.
#[test]
fn stripping_the_miss_changes_no_decision_surface() -> Result<(), String> {
    let cases = [
        (
            &RETURN_CHANGE,
            rejected_source(
                "test('truthy', () => {\n  expect(applyDiscount(1)).toBeTruthy();\n});\ntest('throws', () => {\n  expect(() => applyDiscount(1)).toThrow(RangeError);\n});\n",
            ),
        ),
        (
            &PREDICATE_CHANGE,
            rejected_source("test('below', () => {\n  expect(applyDiscount(50)).toBe(50);\n});\n"),
        ),
    ];
    for (change, source) in cases {
        let finding = classify(change, &source)?;
        assert!(
            finding.related_tests.iter().any(|row| row.miss.is_some()),
            "fixture must carry a miss: {:?}",
            finding.related_tests
        );
        assert!(finding.related_tests.iter().all(|row| !row.is_unmatched()));
        let mut stripped = finding.clone();
        for row in &mut stripped.related_tests {
            row.miss = None;
        }
        let order = |f: &Finding| {
            f.related_tests
                .iter()
                .map(|row| (row.name.clone(), row.line, row.oracle_strength.rank()))
                .collect::<Vec<_>>()
        };
        assert_eq!(order(&finding), order(&stripped));
        assert_eq!(
            crate::domain::DiagnosticWitness::from_finding(&finding),
            crate::domain::DiagnosticWitness::from_finding(&stripped)
        );
        let next = |f: &Finding| {
            crate::analysis::classify::exact_oracle_gap_next_step(
                &f.probe,
                &f.class,
                &f.related_tests,
                &f.activation.missing_discriminators,
                true,
            )
        };
        assert_eq!(next(&finding), next(&stripped));
    }
    Ok(())
}

/// Pins the blessed `typescript_negated_t_oracle` rows: both negated-`t`
/// assertion rows carry `observation_unconfirmed`, never a decision-bearing
/// reason.
#[test]
fn negated_t_oracle_fixture_pins_row_misses() -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/typescript_negated_t_oracle/expected/check.json");
    let text =
        std::fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    let misses: Vec<&str> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("\"miss\": \""))
        .filter_map(|rest| rest.split('"').next())
        .collect();
    assert_eq!(misses, vec!["observation_unconfirmed"; 2], "{text}");
    Ok(())
}
