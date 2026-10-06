//! Family-relevant assertion selection controls (#5572, RIPR-SPEC-0224).
//!
//! Each case parses a real pytest source so the selector sees the extractor's
//! own kinds, strengths and shapes, asserts the parsed inventory first, then
//! asserts the selection. `old` records what the strength-only collapse picked,
//! so every family-sensitive case is discriminating against it.

use super::assertion_selection::{
    PythonAssertionFocus, PythonAssertionSelection, select_relevant_assertion,
};
use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use crate::domain::ProbeFamily;

const OWNER_SOURCE: &str =
    "def f(x):\n    if x < 0:\n        raise ValueError(\"neg\")\n    return x * 2\n";

fn parse_test(body: &str) -> PythonTest {
    let source = format!("import pytest\nfrom src.calc import f\n\ndef test_f():\n{body}");
    let mut tests = extract_tests(Path::new("tests/test_calc.py"), &source);
    assert_eq!(tests.len(), 1, "fixture must parse to one test: {source}");
    tests.remove(0)
}

/// The pre-#5572 strength-only collapse (`max_by_key` keeps the last max).
fn strength_only(test: &PythonTest) -> Option<&str> {
    test.assertions
        .iter()
        .max_by_key(|assertion| assertion.oracle_strength.rank())
        .map(|assertion| assertion.text.as_str())
}

fn selected<'a>(test: &'a PythonTest, focus: &PythonAssertionFocus) -> Option<&'a str> {
    select_relevant_assertion(&test.assertions, Some(focus))
        .assertion()
        .map(|assertion| assertion.text.as_str())
}

fn shapes(test: &PythonTest) -> Vec<(PythonOracleShape, OracleStrength)> {
    test.assertions
        .iter()
        .map(|assertion| (assertion.oracle_shape, assertion.oracle_strength.clone()))
        .collect()
}

struct Case {
    name: &'static str,
    focus: PythonAssertionFocus,
    body: &'static str,
    expected: Option<&'static str>,
    /// Whether the strength-only collapse picked something else.
    old_differs: bool,
}

fn family(family: ProbeFamily) -> PythonAssertionFocus {
    PythonAssertionFocus::for_family(family)
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "return value: strong exception + weaker value observer -> value",
            focus: family(ProbeFamily::ReturnValue),
            body: "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n    assert f(2) > 0\n",
            expected: Some("assert f(2) > 0"),
            old_differs: true,
        },
        Case {
            name: "error path: strong value + exception observer -> exception",
            focus: family(ProbeFamily::ErrorPath),
            body: "    assert f(2) == 4\n    with pytest.raises(ValueError):\n        f(-1)\n",
            expected: Some("pytest.raises(ValueError)"),
            old_differs: true,
        },
        Case {
            name: "attribute field: sibling exact field + relevant broad field observer -> relevant",
            focus: PythonAssertionFocus::for_change(
                ProbeFamily::FieldConstruction,
                "        self.total = amount",
                None,
            ),
            body: "    order = f(5)\n    assert order.count == 1\n    assert order.total > 0\n",
            expected: Some("assert order.total > 0"),
            old_differs: true,
        },
        Case {
            name: "dict field: sibling exact key + relevant broad key observer -> relevant",
            focus: PythonAssertionFocus::for_change(
                ProbeFamily::FieldConstruction,
                "    return {\"host\": \"a\", \"port\": 9090}",
                Some("    return {\"host\": \"a\", \"port\": 80}"),
            ),
            body: "    assert f(1)[\"host\"] == \"a\"\n    assert f(1)[\"port\"] > 0\n",
            expected: Some("assert f(1)[\"port\"] > 0"),
            old_differs: true,
        },
        Case {
            name: "dict field: a lone sibling-key observer stays selectable (same family, other sink)",
            focus: PythonAssertionFocus::for_change(
                ProbeFamily::FieldConstruction,
                "    return {\"host\": \"a\", \"port\": 9090}",
                Some("    return {\"host\": \"a\", \"port\": 80}"),
            ),
            body: "    assert f(1)[\"host\"] == \"a\"\n",
            expected: Some("assert f(1)[\"host\"] == \"a\""),
            old_differs: false,
        },
        Case {
            name: "predicate: a stronger mock expectation stays applicable (a predicate may gate a call)",
            focus: family(ProbeFamily::Predicate),
            body: "    f(3)\n    notifier.assert_called_once_with(3)\n    assert f(3) >= 0\n",
            expected: Some("notifier.assert_called_once_with(3)"),
            old_differs: false,
        },
        Case {
            name: "predicate: equal strength prefers the boundary observer",
            focus: family(ProbeFamily::Predicate),
            body: "    assert f(3) >= 0\n    self.assertIn(f(3), (0, 1))\n",
            expected: Some("assert f(3) >= 0"),
            old_differs: true,
        },
        Case {
            name: "error path: a stronger mock expectation is not an exception observer",
            focus: family(ProbeFamily::ErrorPath),
            body: "    notifier.assert_called_once_with(3)\n    with pytest.raises(ValueError):\n        f(-1)\n",
            expected: Some("pytest.raises(ValueError)"),
            old_differs: true,
        },
        Case {
            name: "predicate: an exception observer stays applicable (a predicate may guard a raise)",
            focus: family(ProbeFamily::Predicate),
            body: "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n",
            expected: Some("pytest.raises(ValueError, match=\"neg\")"),
            old_differs: false,
        },
        Case {
            name: "equal strength: whole value over a trailing len aggregate",
            focus: family(ProbeFamily::ReturnValue),
            body: "    assert f(1) == 2\n    assert len(f(1)) == 1\n",
            expected: Some("assert f(1) == 2"),
            old_differs: true,
        },
        Case {
            name: "equal strength: whole value over a leading len aggregate",
            focus: family(ProbeFamily::ReturnValue),
            body: "    assert len(f(1)) == 1\n    assert f(1) == 2\n",
            expected: Some("assert f(1) == 2"),
            old_differs: false,
        },
        Case {
            name: "return value: no applicable assertion -> none, not the exception",
            focus: family(ProbeFamily::ReturnValue),
            body: "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n",
            expected: None,
            old_differs: true,
        },
        Case {
            name: "error path: no applicable assertion -> none, not the exact value",
            focus: family(ProbeFamily::ErrorPath),
            body: "    assert f(2) == 4\n",
            expected: None,
            old_differs: true,
        },
        Case {
            name: "error path: a status code observes the error's visible effect",
            focus: family(ProbeFamily::ErrorPath),
            body: "    response = f(-1)\n    assert response.status_code == 400\n    assert response.count == 0\n",
            expected: Some("assert response.status_code == 400"),
            old_differs: true,
        },
    ]
}

#[test]
fn family_relevant_assertion_selection_controls() {
    // Every case runs and every miss is reported, so a regression names each
    // control it breaks.
    let mut failures = Vec::new();
    for case in cases() {
        let test = parse_test(case.body);
        if test.assertions.is_empty() {
            failures.push(format!("{}: fixture extracted no assertions", case.name));
            continue;
        }
        let actual = selected(&test, &case.focus);
        if actual != case.expected {
            failures.push(format!(
                "{}: selected {actual:?}, expected {:?}; inventory {:?}",
                case.name,
                case.expected,
                shapes(&test)
            ));
        }
        if (strength_only(&test) != case.expected) != case.old_differs {
            failures.push(format!(
                "{}: strength-only picked {:?}; old_differs should be {}",
                case.name,
                strength_only(&test),
                !case.old_differs
            ));
        }
        if case.expected.is_none()
            && select_relevant_assertion(&test.assertions, Some(&case.focus))
                != PythonAssertionSelection::NoFamilyRelevant
        {
            failures.push(format!("{}: the none state is not explicit", case.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_fixtures_parse_to_the_intended_oracle_shapes() {
    let mixed = parse_test(
        "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n    assert f(2) > 0\n    assert f(2) == 4\n    notifier.assert_called_once_with(3)\n",
    );
    assert_eq!(
        shapes(&mixed),
        vec![
            (
                PythonOracleShape::ExceptionAssertion,
                OracleStrength::Strong
            ),
            (PythonOracleShape::BoundaryAssertion, OracleStrength::Weak),
            (PythonOracleShape::ExactAssertion, OracleStrength::Strong),
            (PythonOracleShape::MockExpectation, OracleStrength::Medium),
        ]
    );
    let fields =
        parse_test("    order = f(5)\n    assert order.count == 1\n    assert order.total > 0\n");
    assert_eq!(
        shapes(&fields),
        vec![
            (PythonOracleShape::FieldAssertion, OracleStrength::Strong),
            (PythonOracleShape::FieldAssertion, OracleStrength::Weak),
        ]
    );
    // Without the changed field the sibling is not ruled out, so the field
    // case above is decided by the changed-field evidence, not the family.
    assert_eq!(
        selected(&fields, &family(ProbeFamily::FieldConstruction)),
        Some("assert order.count == 1")
    );
}

fn semantic(
    test: &PythonTest,
    focus: Option<&PythonAssertionFocus>,
) -> Option<(OracleKind, OracleStrength, PythonOracleShape)> {
    select_relevant_assertion(&test.assertions, focus)
        .assertion()
        .map(|assertion| {
            (
                assertion.oracle_kind.clone(),
                assertion.oracle_strength.clone(),
                assertion.oracle_shape,
            )
        })
}

#[test]
fn reordering_assertions_never_changes_the_selection() {
    // Semantically distinguishable assertions: the selected text itself is
    // order-independent.
    let distinct = [
        "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n",
        "    assert f(2) > 0\n",
        "    assert f(1) == 2\n",
        "    assert len(f(1)) == 1\n",
        "    with pytest.raises(KeyError):\n        f(None)\n",
        "    notifier.assert_called_once_with(3)\n",
        "    assert_valid(f(1))\n",
    ];
    // Equivalent assertions (same kind, strength, shape and family
    // preference): the semantic result is order-independent.
    let equivalent = ["    assert f(1) == 2\n", "    assert f(2) == 4\n"];
    let families = [
        ProbeFamily::ReturnValue,
        ProbeFamily::ErrorPath,
        ProbeFamily::Predicate,
        ProbeFamily::FieldConstruction,
        ProbeFamily::SideEffect,
    ];
    let forward = parse_test(&distinct.concat());
    let backward = parse_test(&distinct.iter().rev().copied().collect::<String>());
    let equivalent_forward = parse_test(&equivalent.concat());
    let equivalent_backward = parse_test(&equivalent.iter().rev().copied().collect::<String>());
    for probe_family in families {
        let focus = family(probe_family.clone());
        assert!(selected(&forward, &focus).is_some(), "{probe_family:?}");
        assert_eq!(
            selected(&forward, &focus),
            selected(&backward, &focus),
            "{probe_family:?}"
        );
        assert_eq!(
            semantic(&equivalent_forward, Some(&focus)),
            semantic(&equivalent_backward, Some(&focus)),
            "{probe_family:?}"
        );
    }
    assert_eq!(
        select_relevant_assertion(&forward.assertions, None)
            .assertion()
            .map(|assertion| assertion.text.as_str()),
        select_relevant_assertion(&backward.assertions, None)
            .assertion()
            .map(|assertion| assertion.text.as_str()),
    );
}

#[test]
fn a_stronger_orthogonal_assertion_never_displaces_the_relevant_one() {
    let pairs = [
        (
            ProbeFamily::ReturnValue,
            "    assert f(2) > 0\n",
            "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n",
        ),
        (
            ProbeFamily::ErrorPath,
            "    with pytest.raises(ValueError):\n        f(-1)\n",
            "    assert f(2) == 4\n",
        ),
        (
            ProbeFamily::ErrorPath,
            "    with pytest.raises(ValueError):\n        f(-1)\n",
            "    notifier.assert_called_once_with(3)\n",
        ),
    ];
    for (probe_family, relevant, orthogonal) in pairs {
        let focus = family(probe_family.clone());
        let alone = parse_test(relevant);
        let with_orthogonal = parse_test(&format!("{relevant}{orthogonal}"));
        let orthogonal_first = parse_test(&format!("{orthogonal}{relevant}"));
        let expected = selected(&alone, &focus);
        assert!(
            expected.is_some(),
            "{probe_family:?}: relevant alone selects"
        );
        assert_eq!(
            selected(&with_orthogonal, &focus),
            expected,
            "{probe_family:?}"
        );
        assert_eq!(
            selected(&orthogonal_first, &focus),
            expected,
            "{probe_family:?}"
        );
        // The strength-only collapse is displaced by the orthogonal assertion.
        assert_ne!(
            strength_only(&with_orthogonal),
            expected,
            "{probe_family:?}"
        );
    }
}

#[test]
fn single_applicable_assertion_keeps_its_strength_only_projection() {
    let cases: [(&str, &[ProbeFamily]); 4] = [
        (
            "    assert f(1) == 2\n",
            &[
                ProbeFamily::ReturnValue,
                ProbeFamily::Predicate,
                ProbeFamily::FieldConstruction,
                ProbeFamily::SideEffect,
            ],
        ),
        (
            "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n",
            &[
                ProbeFamily::ErrorPath,
                ProbeFamily::Predicate,
                ProbeFamily::SideEffect,
            ],
        ),
        (
            "    notifier.assert_called_once_with(3)\n",
            &[
                ProbeFamily::ReturnValue,
                ProbeFamily::FieldConstruction,
                ProbeFamily::Predicate,
                ProbeFamily::SideEffect,
                ProbeFamily::CallDeletion,
            ],
        ),
        (
            "    assert_valid(f(1))\n",
            &[
                ProbeFamily::ReturnValue,
                ProbeFamily::ErrorPath,
                ProbeFamily::Predicate,
                ProbeFamily::FieldConstruction,
                ProbeFamily::SideEffect,
            ],
        ),
    ];
    for (body, families) in cases {
        let test = parse_test(body);
        assert_eq!(test.assertions.len(), 1, "{body}");
        for probe_family in families {
            assert_eq!(
                selected(&test, &family(probe_family.clone())),
                strength_only(&test),
                "{body} under {probe_family:?}"
            );
        }
    }
    assert_eq!(
        select_relevant_assertion(&[], Some(&family(ProbeFamily::ReturnValue))),
        PythonAssertionSelection::NoAssertion
    );
}

/// The public row, the `test_oracle` evidence and the classifier all judge
/// the selected assertion, not the strength-only one.
#[test]
fn classified_row_and_evidence_share_the_selected_assertion() -> Result<(), String> {
    let owners = extract_owners(Path::new("src/calc.py"), OWNER_SOURCE);
    let test = parse_test(
        "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n    assert f(2) > 0\n",
    );
    let tests = vec![test];
    let finding = classify_change(
        Path::new("src/calc.py"),
        4,
        "    return x * 2",
        &owners,
        &tests,
    )
    .ok_or("a return change in a related owner must classify")?;
    assert_eq!(finding.probe.family, ProbeFamily::ReturnValue);
    assert_eq!(finding.related_tests.len(), 1);
    let row = &finding.related_tests[0];
    assert_eq!(row.oracle.as_deref(), Some("assert f(2) > 0"));
    assert_eq!(row.oracle_kind, OracleKind::RelationalCheck);
    assert_eq!(row.oracle_strength, OracleStrength::Weak);
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line == "test_oracle: relational_check weak (test_f)"),
        "{:?}",
        finding.evidence
    );
    // The stronger wrong-family exception assertion no longer credits a
    // strong oracle for the changed return value.
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    let raise = classify_change(
        Path::new("src/calc.py"),
        3,
        "        raise ValueError(\"neg\")",
        &owners,
        &tests,
    );
    let raise = raise.ok_or("a raise change in a related owner must classify")?;
    assert_eq!(raise.probe.family, ProbeFamily::ErrorPath);
    assert_eq!(
        raise.related_tests[0].oracle.as_deref(),
        Some("pytest.raises(ValueError, match=\"neg\")")
    );
    Ok(())
}

#[test]
fn a_test_with_only_wrong_family_assertions_reports_none_in_evidence() -> Result<(), String> {
    let owners = extract_owners(Path::new("src/calc.py"), OWNER_SOURCE);
    let tests = vec![parse_test(
        "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n",
    )];
    let finding = classify_change(
        Path::new("src/calc.py"),
        4,
        "    return x * 2",
        &owners,
        &tests,
    )
    .ok_or("a return change in a related owner must classify")?;
    let row = &finding.related_tests[0];
    assert_eq!(row.oracle, None);
    assert_eq!(row.oracle_strength, OracleStrength::Unknown);
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line == "test_oracle_shape: no_return_value_relevant_assertion (test_f)"),
        "{:?}",
        finding.evidence
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("test_oracle: ")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}
