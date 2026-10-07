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
    /// The parsed (shape, strength) inventory the case depends on.
    inventory: Vec<(PythonOracleShape, OracleStrength)>,
    expected: Option<&'static str>,
    /// Whether the strength-only collapse picked something else.
    old_differs: bool,
}

const EXACT: PythonOracleShape = PythonOracleShape::ExactAssertion;
const BOUND: PythonOracleShape = PythonOracleShape::BoundaryAssertion;
const EXC: PythonOracleShape = PythonOracleShape::ExceptionAssertion;
const FIELD: PythonOracleShape = PythonOracleShape::FieldAssertion;
const STATUS: PythonOracleShape = PythonOracleShape::StatusCodeAssertion;
const MOCK: PythonOracleShape = PythonOracleShape::MockExpectation;
const STRONG: OracleStrength = OracleStrength::Strong;
const MEDIUM: OracleStrength = OracleStrength::Medium;
const WEAK: OracleStrength = OracleStrength::Weak;

fn family(family: ProbeFamily) -> PythonAssertionFocus {
    PythonAssertionFocus::for_family(family)
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "return value: strong exception + weaker value observer -> value",
            focus: family(ProbeFamily::ReturnValue),
            body: "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n    assert f(2) > 0\n",
            inventory: vec![(EXC, STRONG), (BOUND, WEAK)],
            expected: Some("assert f(2) > 0"),
            old_differs: true,
        },
        Case {
            name: "error path: strong value + exception observer -> exception",
            focus: family(ProbeFamily::ErrorPath),
            body: "    assert f(2) == 4\n    with pytest.raises(ValueError):\n        f(-1)\n",
            inventory: vec![(EXACT, STRONG), (EXC, WEAK)],
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
            inventory: vec![(FIELD, STRONG), (FIELD, WEAK)],
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
            inventory: vec![(FIELD, STRONG), (FIELD, WEAK)],
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
            inventory: vec![(FIELD, STRONG)],
            expected: Some("assert f(1)[\"host\"] == \"a\""),
            old_differs: false,
        },
        Case {
            name: "predicate: a stronger mock expectation stays applicable (a predicate may gate a call)",
            focus: family(ProbeFamily::Predicate),
            body: "    f(3)\n    notifier.assert_called_once_with(3)\n    assert f(3) >= 0\n",
            inventory: vec![(MOCK, MEDIUM), (BOUND, WEAK)],
            expected: Some("notifier.assert_called_once_with(3)"),
            old_differs: false,
        },
        Case {
            name: "predicate: equal strength prefers the boundary observer",
            focus: family(ProbeFamily::Predicate),
            body: "    assert f(3) >= 0\n    self.assertIn(f(3), (0, 1))\n",
            inventory: vec![(BOUND, WEAK), (FIELD, WEAK)],
            expected: Some("assert f(3) >= 0"),
            old_differs: true,
        },
        Case {
            name: "error path: a stronger mock expectation is not an exception observer",
            focus: family(ProbeFamily::ErrorPath),
            body: "    notifier.assert_called_once_with(3)\n    with pytest.raises(ValueError):\n        f(-1)\n",
            inventory: vec![(MOCK, MEDIUM), (EXC, WEAK)],
            expected: Some("pytest.raises(ValueError)"),
            old_differs: true,
        },
        Case {
            name: "predicate: an exception observer stays applicable (a predicate may guard a raise)",
            focus: family(ProbeFamily::Predicate),
            body: "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n",
            inventory: vec![(EXC, STRONG)],
            expected: Some("pytest.raises(ValueError, match=\"neg\")"),
            old_differs: false,
        },
        Case {
            name: "equal strength: whole value over a trailing len aggregate",
            focus: family(ProbeFamily::ReturnValue),
            body: "    assert f(1) == 2\n    assert len(f(1)) == 1\n",
            inventory: vec![(EXACT, STRONG), (EXACT, STRONG)],
            expected: Some("assert f(1) == 2"),
            old_differs: true,
        },
        Case {
            name: "equal strength: whole value over a leading len aggregate",
            focus: family(ProbeFamily::ReturnValue),
            body: "    assert len(f(1)) == 1\n    assert f(1) == 2\n",
            inventory: vec![(EXACT, STRONG), (EXACT, STRONG)],
            expected: Some("assert f(1) == 2"),
            old_differs: false,
        },
        Case {
            name: "return value: no applicable assertion -> none, not the exception",
            focus: family(ProbeFamily::ReturnValue),
            body: "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n",
            inventory: vec![(EXC, STRONG)],
            expected: None,
            old_differs: true,
        },
        Case {
            name: "error path: no applicable assertion -> none, not the exact value",
            focus: family(ProbeFamily::ErrorPath),
            body: "    assert f(2) == 4\n",
            inventory: vec![(EXACT, STRONG)],
            expected: None,
            old_differs: true,
        },
        Case {
            name: "error path: a status code observes the error's visible effect",
            focus: family(ProbeFamily::ErrorPath),
            body: "    response = f(-1)\n    assert response.status_code == 400\n    assert response.count == 0\n",
            inventory: vec![(STATUS, STRONG), (FIELD, STRONG)],
            expected: Some("assert response.status_code == 400"),
            old_differs: true,
        },
        Case {
            name: "except line: a normal-value observer of the handler result applies",
            focus: PythonAssertionFocus::for_change(
                ProbeFamily::ErrorPath,
                "    except (ValueError, TypeError):",
                Some("    except ValueError:"),
            ),
            body: "    assert f(None) == 0\n",
            inventory: vec![(EXACT, STRONG)],
            expected: Some("assert f(None) == 0"),
            old_differs: false,
        },
        Case {
            name: "try line: a mock expectation applies",
            focus: PythonAssertionFocus::for_change(ProbeFamily::ErrorPath, "    try:", None),
            body: "    f(1)\n    notifier.assert_called_once_with(3)\n",
            inventory: vec![(MOCK, MEDIUM)],
            expected: Some("notifier.assert_called_once_with(3)"),
            old_differs: false,
        },
        Case {
            name: "finally line: the stronger value observer is kept over a weak exception observer",
            focus: PythonAssertionFocus::for_change(ProbeFamily::ErrorPath, "    finally:", None),
            body: "    with pytest.raises(ValueError):\n        f(-1)\n    assert f(2) == 4\n",
            inventory: vec![(EXC, WEAK), (EXACT, STRONG)],
            expected: Some("assert f(2) == 4"),
            old_differs: false,
        },
        Case {
            name: "raise line: the same normal-value observer does not apply",
            focus: PythonAssertionFocus::for_change(
                ProbeFamily::ErrorPath,
                "        raise TypeError(\"bad\")",
                None,
            ),
            body: "    assert f(None) == 0\n",
            inventory: vec![(EXACT, STRONG)],
            expected: None,
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
        if shapes(&test) != case.inventory {
            failures.push(format!(
                "{}: parsed inventory {:?}, expected {:?}",
                case.name,
                shapes(&test),
                case.inventory
            ));
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

/// When only the source line differs, the later source line wins.
#[test]
fn identical_assertions_tie_break_on_the_later_source_line() {
    let test = parse_test("    assert f(1) == 2\n    assert f(1) == 2\n");
    assert_eq!(shapes(&test), vec![(EXACT, STRONG), (EXACT, STRONG)]);
    let lines: Vec<usize> = test
        .assertions
        .iter()
        .map(|assertion| assertion.line)
        .collect();
    assert!(lines[0] < lines[1], "{lines:?}");
    for probe_family in [ProbeFamily::ReturnValue, ProbeFamily::Predicate] {
        let line = select_relevant_assertion(&test.assertions, Some(&family(probe_family)))
            .assertion()
            .map(|assertion| assertion.line);
        assert_eq!(line, Some(lines[1]));
    }
}

/// A strong assertion of another behavior family never suppresses a
/// test-side static limit (#5572): the limit reads the same family filter
/// as the rows.
#[test]
fn wrong_family_strong_assertion_does_not_suppress_test_side_limits() {
    let owners = extract_owners(Path::new("src/calc.py"), OWNER_SOURCE);
    let tests = vec![parse_test(
        "    with pytest.raises(ValueError, match=\"neg\"):\n        f(-1)\n    assert_valid(f(2))\n",
    )];
    assert_eq!(
        shapes(&tests[0]),
        vec![
            (EXC, STRONG),
            (
                PythonOracleShape::UnknownCustomHelper,
                OracleStrength::Unknown
            )
        ]
    );
    let candidates = super::related_tests::related_test_candidates(&owners[0], &tests);
    assert_eq!(candidates.len(), 1);
    // Changed return: the exception assertion is not a known oracle, so the
    // opaque helper limits the finding.
    assert_eq!(
        static_limit_for_change("    return x * 2", &owners[0], &candidates)
            .map(|limit| limit.kind),
        Some(StaticLimitKind::OpaqueCustomAssertionHelper)
    );
    // Changed raise: the matching exception assertion is a known oracle.
    assert_eq!(
        static_limit_for_change("        raise ValueError(\"neg\")", &owners[0], &candidates)
            .map(|limit| limit.kind),
        None
    );
}

/// A changed `except` clause is observed by the handler's result.
#[test]
fn changed_except_clause_keeps_the_handler_value_assertion() -> Result<(), String> {
    let source = "def to_int(text):\n    try:\n        return int(text)\n    except (ValueError, TypeError):\n        return 0\n";
    let owners = extract_owners(Path::new("src/conv.py"), source);
    let mut tests = extract_tests(
        Path::new("tests/test_conv.py"),
        "from src.conv import to_int\n\ndef test_to_int_none():\n    assert to_int(None) == 0\n",
    );
    assert_eq!(tests.len(), 1);
    let tests = vec![tests.remove(0)];
    let finding = classify_change_with_old(
        Path::new("src/conv.py"),
        4,
        "    except (ValueError, TypeError):",
        Some("    except ValueError:"),
        &owners,
        &tests,
    )
    .ok_or("a changed except clause must classify")?;
    assert_eq!(finding.probe.family, ProbeFamily::ErrorPath);
    assert_eq!(
        finding.related_tests[0].oracle.as_deref(),
        Some("assert to_int(None) == 0")
    );
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line == "test_oracle: exact_value strong (test_to_int_none)"),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

/// The class of a changed raise does not depend on assertion source order.
#[test]
fn changed_raise_class_is_independent_of_assertion_order() -> Result<(), String> {
    let source = "def parse(text):\n    if not text:\n        raise KeyError(\"empty\")\n    return int(text)\n";
    let owners = extract_owners(Path::new("src/app.py"), source);
    let raises = "    with pytest.raises(KeyError, match=\"empty\"):\n        parse(\"\")\n";
    let value = "    assert parse(\"42\") == 42\n";
    let mut classes = Vec::new();
    for body in [format!("{raises}{value}"), format!("{value}{raises}")] {
        let mut tests = extract_tests(
            Path::new("tests/test_app.py"),
            &format!("import pytest\nfrom src.app import parse\n\ndef test_parse():\n{body}"),
        );
        assert_eq!(tests.len(), 1);
        let tests = vec![tests.remove(0)];
        let finding = classify_change_with_old(
            Path::new("src/app.py"),
            3,
            "        raise KeyError(\"empty\")",
            Some("        raise ValueError(\"empty\")"),
            &owners,
            &tests,
        )
        .ok_or("a changed raise must classify")?;
        classes.push(finding.class);
    }
    assert_eq!(
        classes,
        vec![ExposureClass::Exposed, ExposureClass::Exposed]
    );
    Ok(())
}

/// A strong sibling-field assertion the selector passes over is not a known
/// oracle for the changed field, so it does not suppress the property-based
/// or opaque-helper limit (#5572): suppression reads the selected assertion.
#[test]
fn passed_over_sibling_field_assertion_does_not_suppress_test_side_limits() {
    let owners = extract_owners(Path::new("src/calc.py"), OWNER_SOURCE);
    let changed = "        self.total = amount";
    let property_source = "import pytest\nfrom hypothesis import given, strategies as st\nfrom src.calc import f\n\n@given(st.integers())\ndef test_f(value):\n    order = f(value)\n    assert order.count == 1\n    assert order.total > 0\n";
    let helper_source = "import pytest\nfrom src.calc import f\n\ndef test_f():\n    order = f(5)\n    assert order.count == 1\n    assert order.total > 0\n    assert_valid(order)\n";
    let property_tests = extract_tests(Path::new("tests/test_calc.py"), property_source);
    let helper_tests = extract_tests(Path::new("tests/test_calc.py"), helper_source);
    assert_eq!(property_tests.len(), 1);
    assert_eq!(helper_tests.len(), 1);
    assert_eq!(
        shapes(&property_tests[0]),
        vec![(FIELD, STRONG), (FIELD, WEAK)]
    );
    assert_eq!(
        shapes(&helper_tests[0]),
        vec![
            (FIELD, STRONG),
            (FIELD, WEAK),
            (
                PythonOracleShape::UnknownCustomHelper,
                OracleStrength::Unknown
            )
        ]
    );
    let focus = PythonAssertionFocus::for_change(ProbeFamily::FieldConstruction, changed, None);
    assert_eq!(
        selected(&property_tests[0], &focus),
        Some("assert order.total > 0")
    );
    let property_candidates =
        super::related_tests::related_test_candidates(&owners[0], &property_tests);
    let helper_candidates =
        super::related_tests::related_test_candidates(&owners[0], &helper_tests);
    assert_eq!(property_candidates.len(), 1);
    assert_eq!(helper_candidates.len(), 1);
    assert_eq!(
        static_limit_for_change(changed, &owners[0], &property_candidates).map(|limit| limit.kind),
        Some(StaticLimitKind::PropertyBasedTest)
    );
    assert_eq!(
        static_limit_for_change(changed, &owners[0], &helper_candidates).map(|limit| limit.kind),
        Some(StaticLimitKind::OpaqueCustomAssertionHelper)
    );
    // Control: a strong assertion on the changed field itself suppresses
    // the property-based and opaque-helper limits.
    let strong_property = extract_tests(
        Path::new("tests/test_calc.py"),
        &property_source.replace("order.total > 0", "order.total == 5"),
    );
    let strong_helper = extract_tests(
        Path::new("tests/test_calc.py"),
        &helper_source.replace("order.total > 0", "order.total == 5"),
    );
    assert_eq!(
        shapes(&strong_property[0]),
        vec![(FIELD, STRONG), (FIELD, STRONG)]
    );
    assert_eq!(
        static_limit_for_change(
            changed,
            &owners[0],
            &super::related_tests::related_test_candidates(&owners[0], &strong_property)
        )
        .map(|limit| limit.kind),
        // The `@given` parameter is still an unresolved fixture input, a
        // separate limit; the property-based limit itself is suppressed.
        Some(StaticLimitKind::UnresolvedPytestFixture)
    );
    assert_eq!(
        static_limit_for_change(
            changed,
            &owners[0],
            &super::related_tests::related_test_candidates(&owners[0], &strong_helper)
        )
        .map(|limit| limit.kind),
        None
    );
}

/// A changed dict key is localized from the old and new lines, so the
/// static-limit check must use the classifier's focus: without the old line a
/// strong sibling-key assertion would be selected and hide the limit (#5572).
#[test]
fn changed_dict_key_limit_suppression_uses_the_classifier_focus() {
    let owners = extract_owners(Path::new("src/calc.py"), OWNER_SOURCE);
    let old = "    return {\"host\": \"a\", \"port\": 80}";
    let new = "    return {\"host\": \"a\", \"port\": 9090}";
    let source = "import pytest\nfrom hypothesis import given, strategies as st\nfrom src.calc import f\n\n@given(st.integers())\ndef test_f(value):\n    config = f(value)\n    assert config[\"host\"] == \"a\"\n    assert config[\"port\"] > 0\n";
    let tests = extract_tests(Path::new("tests/test_calc.py"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(shapes(&tests[0]), vec![(FIELD, STRONG), (FIELD, WEAK)]);
    let candidates = super::related_tests::related_test_candidates(&owners[0], &tests);
    assert_eq!(candidates.len(), 1);
    let focus = PythonAssertionFocus::for_change(ProbeFamily::FieldConstruction, new, Some(old));
    assert_eq!(
        selected(&tests[0], &focus),
        Some("assert config[\"port\"] > 0")
    );
    assert_eq!(
        super::static_limits::static_limit_for_focused_change(new, &focus, &owners[0], &candidates)
            .map(|limit| limit.kind),
        Some(StaticLimitKind::PropertyBasedTest)
    );
    // Without the old line the sibling key is not ruled out, which is why the
    // classifier passes its own focus.
    assert_eq!(
        static_limit_for_change(new, &owners[0], &candidates).map(|limit| limit.kind),
        Some(StaticLimitKind::UnresolvedPytestFixture)
    );
}
