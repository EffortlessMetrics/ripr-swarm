//! Discriminator narration must follow the final classification, not oracle rank.

use super::super::owners_tests::{extract_owners, extract_tests};
use super::classify_change_with_old;
use super::classify_change_with_source;
use crate::domain::{ExposureClass, Finding, OracleStrength, StageState};
use std::path::Path;

// #7312: independently authored from CPython ebf955df7a89ed0c7968f79faec1de49f61ed7cb
// Lib/unittest/case.py::assertListEqual and test_unittest/test_case.py::testEquality.
#[test]
fn unittest_list_equal_observes_whole_return_including_short_elements() -> Result<(), String> {
    for (before, after, expected) in [
        ("    return [1, 1]", "    return [1, 2]", "[1, 2]"),
        (
            "    return [0, 'pending']",
            "    return [0, 'ready']",
            "[0, 'ready']",
        ),
    ] {
        let source = format!("def labels():\n{after}\n");
        let tests = format!(
            "import unittest\nfrom src.subject import labels\nclass Checks(unittest.TestCase):\n    def test_labels(self):\n        self.assertListEqual(labels(), {expected})\n"
        );
        let extracted = extract_tests(Path::new("tests/test_subject.py"), &tests);
        assert_eq!(extracted.len(), 1);
        let finding = classify_case(&source, &tests, 2, before, after)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{expected}");
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    }
    Ok(())
}

#[test]
fn unittest_list_equal_preserves_ast_call_identity() -> Result<(), String> {
    for (imports, call) in [
        ("from src.subject import labels", "labels ()"),
        ("from src.subject import labels", "labels\t()"),
        ("from src.subject import labels", "(labels) ()"),
        ("from src.subject import labels as values", "(values) ()"),
        ("import src.subject as subject", "subject . labels ()"),
        ("import src.subject as subject", "(subject . labels) ()"),
        ("from src import subject", "(subject . labels) ()"),
    ] {
        let tests = format!(
            "import unittest\n{imports}\nclass Checks(unittest.TestCase):\n    def test_labels(self):\n        self.assertListEqual({call}, [1, 2])\n"
        );
        let finding = classify_case(
            "def labels():\n    return [1, 2]\n",
            &tests,
            2,
            "    return [1, 1]",
            "    return [1, 2]",
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{tests}");
        assert_eq!(finding.related_tests.len(), 1, "{tests}");
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong,
            "{tests}"
        );
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    }
    for (imports, call) in [
        ("from other import labels", "labels ()"),
        (
            "from src.subject import labels\nlabels = other",
            "labels ()",
        ),
        (
            "import src.subject as subject\nsubject = other",
            "subject . labels ()",
        ),
        (
            "import src.subject as subject\nsubject.labels = other",
            "subject . labels ()",
        ),
        ("from src.subject import labels", "labels () ()"),
        (
            "from src.subject import labels",
            "labels () . fixed_list ()",
        ),
        ("from src.subject import labels", "[labels][0] ()"),
    ] {
        let tests = format!(
            "import unittest\n{imports}\nclass Checks(unittest.TestCase):\n    def test_labels(self):\n        self.assertListEqual({call}, [1, 2])\n"
        );
        let finding = classify_case(
            "def labels():\n    return [1, 2]\n",
            &tests,
            2,
            "    return [1, 1]",
            "    return [1, 2]",
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{tests}");
        assert_ne!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    }
    Ok(())
}

#[test]
fn unittest_list_equal_withholds_unresolved_or_nonobserving_calls() -> Result<(), String> {
    let before = "    return [10, 21]";
    let after = "    return [10, 22]";
    for (declaration, members, body) in [
        ("class Checks(unittest.TestCase):", "", "labels()"),
        (
            "class Checks(unittest.TestCase):",
            "",
            "labels()\n        self.assertListEqual([10, 22], [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self.assertListEqual(ignore(labels()), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self.assertListEqual(labels(ignore()), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self.assertListEqual(labels(value=ignore()), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "fake.assertListEqual(labels(), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self().assertListEqual(labels(), [10, 22])",
        ),
        (
            "class Checks(fake.TestCase):",
            "",
            "self.assertListEqual(labels(), [10, 22])",
        ),
        (
            "class TestChecks:",
            "",
            "self.assertListEqual(labels(), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "    def assertListEqual(self, *args):\n        pass\n",
            "self.assertListEqual(labels(), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "    def assertSequenceEqual(self, *args):\n        pass\n",
            "self.assertListEqual(labels(), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "    def fail(self, *args):\n        pass\n",
            "self.assertListEqual(labels(), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self = fake\n        self.assertListEqual(labels(), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self.assertListEqual = ignore\n        self.assertListEqual(labels(), [10, 22])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self.assertListEqual(labels(), labels())",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self.assertListEqual(labels()[:1], [10])",
        ),
        (
            "class Checks(unittest.TestCase):",
            "",
            "self.assertListEqual(labels(), [10, 22], msg='unsupported message form')",
        ),
        (
            "class Checks(unittest.TestCase):",
            "    @unittest.skip('disabled')\n",
            "self.assertListEqual(labels(), [10, 22])",
        ),
    ] {
        let tests = format!(
            "import unittest\nimport fake\nfrom src.subject import labels, ignore\n{declaration}\n{members}    def test_labels(self):\n        {body}\n"
        );
        let extracted = extract_tests(Path::new("tests/test_subject.py"), &tests);
        assert_eq!(extracted.len(), 1, "{tests}");
        let finding = classify_case(
            "def labels():\n    return [10, 22]\n",
            &tests,
            2,
            before,
            after,
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{tests}");
        assert_ne!(
            finding.ripr.reveal.discriminate.state,
            StageState::Yes,
            "{tests}"
        );
    }
    Ok(())
}

#[test]
fn unittest_list_equal_withholds_module_binding_and_framework_writes() -> Result<(), String> {
    for mutation in [
        "unittest = fake",
        "unittest.TestCase = fake.TestCase",
        "unittest.TestCase.assertListEqual = ignore",
        "unittest.TestCase.assertSequenceEqual = ignore",
        "unittest.TestCase.fail = ignore",
        "labels = ignore",
        "setattr(unittest.TestCase, 'assertListEqual', ignore)",
    ] {
        let tests = format!(
            "import unittest\nimport fake\nfrom src.subject import labels, ignore\n{mutation}\nclass Checks(unittest.TestCase):\n    def test_labels(self):\n        self.assertListEqual(labels(), [1, 2])\n"
        );
        assert_eq!(
            extract_tests(Path::new("tests/test_subject.py"), &tests).len(),
            1
        );
        let finding = classify_case(
            "def labels():\n    return [1, 2]\n",
            &tests,
            2,
            "    return [1, 1]",
            "    return [1, 2]",
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{mutation}");
        assert_ne!(
            finding.ripr.reveal.discriminate.state,
            StageState::Yes,
            "{mutation}"
        );
    }
    Ok(())
}

#[test]
fn unittest_list_equal_withholds_nested_unittest_registration() -> Result<(), String> {
    let tests = "import unittest\nfrom src.subject import labels\nclass Outer(unittest.TestCase):\n    class Inner(unittest.TestCase):\n        def test_labels(self):\n            self.assertListEqual(labels(), [1, 2])\n";
    assert_eq!(
        extract_tests(Path::new("tests/test_subject.py"), tests).len(),
        1
    );
    let finding = classify_case(
        "def labels():\n    return [1, 2]\n",
        tests,
        2,
        "    return [1, 1]",
        "    return [1, 2]",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    Ok(())
}

#[test]
fn unittest_list_equal_keeps_duplicate_method_declarations_distinct() -> Result<(), String> {
    let source = "def alpha():\n    return [3, 4]\n\ndef beta():\n    return [1, 2]\n";
    for (first, second) in [("alpha", "beta"), ("beta", "alpha")] {
        let expected = |name| if name == "alpha" { "[3, 4]" } else { "[1, 2]" };
        let tests = format!(
            "import unittest\nfrom src.subject import alpha, beta\nclass A(unittest.TestCase):\n    def test_labels(self):\n        self.assertListEqual({first}(), {})\nclass B(unittest.TestCase):\n    def test_labels(self):\n        self.assertListEqual({second}(), {})\n",
            expected(first),
            expected(second)
        );
        assert_eq!(
            extract_tests(Path::new("tests/test_subject.py"), &tests).len(),
            2
        );
        let finding = classify_case(source, &tests, 5, "    return [1, 1]", "    return [1, 2]")?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{tests}");
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    }
    Ok(())
}

#[test]
fn unittest_list_equal_withholds_derived_owner_results() -> Result<(), String> {
    let source = "class Answer:\n    def __init__(self, flag):\n        self.flag = flag\n    def fixed_list(self):\n        return [1, 2]\n    def __call__(self):\n        return [1, 2]\n    def __getitem__(self, index):\n        return self.fixed_list\n\ndef labels():\n    return Answer(1)\n";
    for call in ["labels().fixed_list()", "labels()()", "labels()[0]()"] {
        let tests = format!(
            "import unittest\nfrom src.subject import labels\nclass Checks(unittest.TestCase):\n    def test_labels(self):\n        self.assertListEqual({call}, [1, 2])\n"
        );
        assert_eq!(
            extract_tests(Path::new("tests/test_subject.py"), &tests).len(),
            1
        );
        let finding = classify_case(
            source,
            &tests,
            12,
            "    return Answer(0)",
            "    return Answer(1)",
        )?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{call}");
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Unknown
        );
    }
    Ok(())
}

#[test]
fn unittest_list_equal_requires_changed_python_literal_value() -> Result<(), String> {
    for (before, after, expected_class, expected_discrimination) in [
        (
            "[1, 1]",
            "[1, True]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        (
            "[1, False]",
            "[1, 0]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        (
            "[1]",
            "[1.0]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        (
            "[1_0]",
            "[10]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        (
            "['a']",
            "[\"a\"]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        (
            "[1.0]",
            "[1e0]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        (
            "[16]",
            "[0x10]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        (
            "[9007199254740993]",
            "[9007199254740992.0]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        // Mixed scalar types stay conservative, even when Python distinguishes them.
        (
            "[2]",
            "[True]",
            ExposureClass::WeaklyExposed,
            StageState::Weak,
        ),
        ("[1, 1]", "[1, 2]", ExposureClass::Exposed, StageState::Yes),
        ("[False]", "[True]", ExposureClass::Exposed, StageState::Yes),
        ("[1.0]", "[2.0]", ExposureClass::Exposed, StageState::Yes),
        ("[1]", "[]", ExposureClass::Exposed, StageState::Yes),
        ("[]", "[1]", ExposureClass::Exposed, StageState::Yes),
    ] {
        let tests = format!(
            "import unittest\nfrom src.subject import labels\nclass Checks(unittest.TestCase):\n    def test_labels(self):\n        self.assertListEqual(labels(), {after})\n"
        );
        assert_eq!(
            extract_tests(Path::new("tests/test_subject.py"), &tests).len(),
            1
        );
        let finding = classify_case(
            &format!("def labels():\n    return {after}\n"),
            &tests,
            2,
            &format!("    return {before}"),
            &format!("    return {after}"),
        )?;
        assert_eq!(finding.class, expected_class, "{before} -> {after}");
        if expected_class == ExposureClass::WeaklyExposed {
            assert!(finding.activation.missing_discriminators.is_empty());
            assert!(finding.recommended_next_step.is_none());
            assert!(
                !finding
                    .evidence
                    .iter()
                    .any(|item| item.starts_with("suggested_repair_action:"))
            );
            assert_eq!(
                finding.missing[0],
                "A strong Python list oracle reaches `labels`, but static evidence cannot establish that list equality discriminates this change. Review the old and new return values under Python equality; mixed scalar kinds and nonliteral returns remain outside this bounded evidence."
            );
        }
        assert_eq!(
            finding.ripr.reveal.discriminate.state,
            expected_discrimination
        );
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
    }
    Ok(())
}

/// Exercise the existing source-fact producer and final classifier together.
fn classify_case(
    source: &str,
    tests: &str,
    line: usize,
    before: &str,
    after: &str,
) -> Result<Finding, String> {
    let file = Path::new("src/subject.py");
    let owners = extract_owners(file, source);
    let tests = extract_tests(Path::new("tests/test_subject.py"), tests);
    classify_change_with_old(file, line, after, Some(before), &owners, &tests)
        .ok_or_else(|| "behavioral fixture must produce a finding".to_string())
}

// RIPR-SPEC-0028 / #5389: a registered test may not run, or may suppress
// the assertion failure. Its exact oracle cannot lend credit to other tests.
#[test]
fn activation_controlled_python_tests_do_not_credit_their_assertions() -> Result<(), String> {
    for decorator in [
        "unittest.skip('disabled')",
        "unittest.skipIf(True, 'disabled')",
        "unittest.skipIf(False, 'runtime condition not evaluated')",
        "unittest.skipUnless(False, 'disabled')",
        "unittest.expectedFailure",
        "pytest.mark.skip(reason='disabled')",
        "pytest.mark.skipif(True, reason='disabled')",
        "pytest.mark.skipif(False, reason='runtime condition not evaluated')",
        "pytest.mark.xfail(strict=False)",
    ] {
        let test = if decorator == "unittest.expectedFailure" {
            format!(
                "class ScoreChecks(unittest.TestCase):\n    @{decorator}\n    def test_score(self):\n        self.assertEqual(score(0), 8)\n"
            )
        } else {
            format!("@{decorator}\ndef test_score():\n    assert score(0) == 8\n")
        };
        let tests =
            format!("import unittest\nimport pytest\nfrom src.subject import score\n{test}");
        let finding = classify_case(
            "def score(value):\n    return 8\n",
            &tests,
            2,
            "    return 7",
            "    return 8",
        )?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{decorator}");
        assert_ne!(
            finding.ripr.reveal.discriminate.state,
            StageState::Yes,
            "{decorator}"
        );
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.contains("test activation")),
            "{decorator}"
        );
    }
    Ok(())
}

#[test]
fn unittest_metadata_ignored_by_ordinary_pytest_keeps_active_oracle_credit() -> Result<(), String> {
    for test in [
        "@unittest.expectedFailure\ndef test_score():\n    assert score(0) == 8\n",
        "@unittest.skip('ignored class flag')\nclass TestScore:\n    def test_score(self):\n        assert score(0) == 8\n",
        "@unittest.skipIf(True, 'ignored class flag')\nclass TestScore:\n    def test_score(self):\n        assert score(0) == 8\n",
        "@unittest.skipUnless(False, 'ignored class flag')\nclass TestScore:\n    def test_score(self):\n        assert score(0) == 8\n",
        "@unittest.expectedFailure\nclass TestScore:\n    def test_score(self):\n        assert score(0) == 8\n",
    ] {
        let tests = format!("import unittest\nfrom src.subject import score\n{test}");
        let finding = classify_case(
            "def score(value):\n    return 8\n",
            &tests,
            2,
            "    return 7",
            "    return 8",
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{tests}");
    }
    Ok(())
}

#[test]
fn explicit_nonframework_imports_cannot_disable_python_oracle_credit() -> Result<(), String> {
    for declaration in [
        "import project_marks as pytest\n@ pytest.mark.skip(reason='custom')",
        "from local_marks import pytest\n@ pytest.mark.skip(reason='custom')",
        "import project_marks as unittest\n@ unittest.skip('custom')",
        "import pytest\nfrom local_marks import pytest\n@ pytest.mark.skip(reason='custom')",
    ] {
        let tests = format!(
            "from src.subject import score\n{declaration}\ndef test_score():\n    assert score(0) == 8\n"
        );
        let finding = classify_case(
            "def score(value):\n    return 8\n",
            &tests,
            2,
            "    return 7",
            "    return 8",
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{tests}");
    }
    Ok(())
}

#[test]
fn class_and_aliased_activation_controls_do_not_credit_python_tests() -> Result<(), String> {
    for tests in [
        "import unittest\nfrom src.subject import score\n@unittest.skip('disabled')\nclass ScoreChecks(unittest.TestCase):\n    def test_score(self):\n        self.assertEqual(score(0), 8)\n",
        "import pytest as pt\nfrom src.subject import score\n@pt.mark.skip(reason='disabled')\nclass TestScore:\n    def test_score(self):\n        assert score(0) == 8\n",
        "from unittest import skip as disabled\nfrom src.subject import score\n@disabled('disabled')\ndef test_score():\n    assert score(0) == 8\n",
        "from pytest import mark as marks\nfrom src.subject import score\n@marks.skip(reason='disabled')\nclass Mixin:\n    def test_score(self):\n        assert score(0) == 8\nclass TestScore(Mixin):\n    pass\n",
    ] {
        let finding = classify_case(
            "def score(value):\n    return 8\n",
            tests,
            2,
            "    return 7",
            "    return 8",
        )?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{tests}");
    }
    Ok(())
}

#[test]
fn unaliased_dotted_python_imports_use_bound_root_for_activation() -> Result<(), String> {
    for (declaration, controlled) in [
        (
            "import project_marks as unittest\nimport unittest.mock",
            true,
        ),
        (
            "import unittest.mock\nimport project_marks as unittest",
            false,
        ),
        (
            "import project_marks as unittest\nimport unittest.mock as helper",
            false,
        ),
        (
            "import unittest.mock as helper\nimport project_marks as unittest",
            false,
        ),
    ] {
        let tests = format!(
            "from src.subject import score\n{declaration}\n@unittest.skip('control')\ndef test_score():\n    assert score(0) == 8\n"
        );
        let finding = classify_case(
            "def score(value):\n    return 8\n",
            &tests,
            2,
            "    return 7",
            "    return 8",
        )?;
        assert_eq!(
            finding.class,
            if controlled {
                ExposureClass::StaticUnknown
            } else {
                ExposureClass::Exposed
            },
            "{tests}"
        );
    }
    Ok(())
}

#[test]
fn active_python_oracle_keeps_credit_beside_a_disabled_test() -> Result<(), String> {
    let tests = "import unittest\nfrom src.subject import score\n@unittest.skip('disabled')\ndef test_disabled_score():\n    assert score(0) == 8\n\ndef test_active_score():\n    assert score(0) == 8\n";
    let finding = classify_case(
        "def score(value):\n    return 8\n",
        tests,
        2,
        "    return 7",
        "    return 8",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    let disabled = finding
        .related_tests
        .iter()
        .find(|test| test.name == "test_disabled_score")
        .ok_or_else(|| "disabled test pointer must remain visible".to_string())?;
    assert_eq!(
        disabled.oracle_strength,
        crate::domain::OracleStrength::Unknown
    );
    Ok(())
}

#[test]
fn disabled_exact_python_oracle_cannot_lend_credit_to_an_active_smoke_test() -> Result<(), String> {
    let tests = "import unittest\nfrom src.subject import score\n@unittest.skip('disabled')\ndef test_disabled_score():\n    assert score(0) == 8\n\ndef test_active_score():\n    assert score(0)\n";
    let finding = classify_case(
        "def score(value):\n    return 8\n",
        tests,
        2,
        "    return 7",
        "    return 8",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    Ok(())
}

#[test]
fn unrelated_python_decorator_names_do_not_disable_active_oracles() -> Result<(), String> {
    let tests = "from src.subject import score\n\ndef skip_notes(f):\n    return f\n\n@skip_notes\ndef test_score():\n    assert score(0) == 8\n";
    let finding = classify_case(
        "def score(value):\n    return 8\n",
        tests,
        2,
        "    return 7",
        "    return 8",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn disabled_python_boundary_input_cannot_activate_an_active_neighbor_oracle() -> Result<(), String>
{
    let tests = "import unittest\nfrom src.subject import score\n@unittest.skip('disabled')\ndef test_disabled_score():\n    assert score(10) == 1\n\ndef test_active_score():\n    assert score(11) == 1\n";
    let finding = classify_case(
        "def score(value):\n    return 1 if value >= 10 else 0\n",
        tests,
        2,
        "    return 1 if value > 10 else 0",
        "    return 1 if value >= 10 else 0",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    Ok(())
}

/// A runner skip marker does not make the marked function uncallable.
/// Withholding its oracle must not erase potential writes before an active call.
#[test]
fn marked_python_test_body_rebinding_cannot_manufacture_an_active_boundary() -> Result<(), String> {
    let tests = "import pytest\nimport src.subject as subject\nfrom src.subject import score\n@ pytest.mark.skip(reason='runner skipped')\ndef test_marked_mutation():\n    subject.LIMIT = 5\n\ndef test_active_score():\n    test_marked_mutation()\n    assert score(10) == 1\n";
    let finding = classify_case(
        "LIMIT = 10\ndef score(value):\n    return 1 if value >= LIMIT else 0\n",
        tests,
        3,
        "    return 1 if value > LIMIT else 0",
        "    return 1 if value >= LIMIT else 0",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    Ok(())
}

/// Keep the exact non-credit wording separate from the positive control below.
#[test]
fn disabled_python_import_alias_cannot_supply_an_active_tests_observer_identity()
-> Result<(), String> {
    let tests = "import unittest\nfrom src.subject import score\n\nclass TestDisabled:\n    @unittest.skip('disabled')\n    def test_score(self):\n        from src.subject import score as observer\n        assert observer(0) == 8\n\nclass TestActive:\n    def test_score(self):\n        from other import observer\n        score(0)\n        assert observer(0) == 8\n";
    let finding = classify_case(
        "def score(value):\n    return 8\n",
        tests,
        2,
        "    return 7",
        "    return 8",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    Ok(())
}

#[test]
fn skipped_python_base_class_controls_an_overridden_test() -> Result<(), String> {
    let tests = "import unittest\nfrom src.subject import score\n@unittest.skip('disabled')\nclass Base(unittest.TestCase):\n    def test_score(self):\n        self.assertEqual(score(0), 8)\n\nclass Child(Base):\n    def test_score(self):\n        self.assertEqual(score(0), 8)\n";
    let finding = classify_case(
        "def score(value):\n    return 8\n",
        tests,
        2,
        "    return 7",
        "    return 8",
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    Ok(())
}

#[test]
fn python_test_body_imports_cannot_rebind_definition_scope_activation() -> Result<(), String> {
    for tests in [
        "from unittest import skip as disabled\nfrom src.subject import score\n@disabled('skip')\ndef test_score():\n    from other import disabled\n    assert score(0) == 8\n",
        "import unittest as control\nfrom src.subject import score\n@control.skip('skip')\ndef test_score():\n    import other as control\n    assert score(0) == 8\n",
        "import unittest\nfrom unittest import skip as disabled\nfrom src.subject import score\n@disabled('skip')\nclass TestScore(unittest.TestCase):\n    from other import disabled\n    def test_score(self):\n        assert score(0) == 8\n",
    ] {
        let finding = classify_case(
            "def score(value):\n    return 8\n",
            tests,
            2,
            "    return 7",
            "    return 8",
        )?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{tests}");
    }
    // The inverse body alias must not retroactively turn a transparent
    // definition-scope decorator into a skip.
    for tests in [
        "from other import disabled\nfrom src.subject import score\n@disabled('note')\ndef test_score():\n    from unittest import skip as disabled\n    assert score(0) == 8\n",
        "from src.subject import score\nclass TestScore:\n    from unittest import skip as disabled\n    @disabled('skip')\n    def test_score(self):\n        from other import disabled\n        assert score(0) == 8\n",
    ].into_iter().enumerate() {
        let (index, tests) = tests;
        let finding = classify_case("def score(value):\n    return 8\n", tests, 2, "    return 7", "    return 8")?;
        assert_eq!(finding.class, if index == 0 { ExposureClass::Exposed } else { ExposureClass::StaticUnknown }, "{tests}");
    }
    Ok(())
}

#[test]
fn disabled_python_mock_cannot_suppress_an_independent_active_oracle() -> Result<(), String> {
    let tests = "import unittest\nfrom unittest.mock import patch\nfrom src.subject import score\n@unittest.skip('disabled')\n@patch('src.subject.score')\ndef test_disabled_score(fake):\n    assert score(0) == 8\n\ndef test_active_score():\n    assert score(0) == 8\n";
    let finding = classify_case(
        "def score(value):\n    return 8\n",
        tests,
        2,
        "    return 7",
        "    return 8",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn python_method_body_does_not_use_class_local_owner_imports() -> Result<(), String> {
    for tests in [
        "from other import score\nclass TestScore:\n    from src.subject import score\n    def test_score(self):\n        assert score(0) == 8\n",
        "from other import score\nclass Mixin:\n    from src.subject import score\n    def test_score(self):\n        assert score(0) == 8\nclass TestScore(Mixin):\n    pass\n",
    ] {
        let finding = classify_case(
            "def score(value):\n    return 8\n",
            tests,
            2,
            "    return 7",
            "    return 8",
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{tests}");
    }
    let tests = "from src.subject import score\nclass TestScore:\n    from other import score\n    def test_score(self):\n        assert score(0) == 8\n";
    let finding = classify_case(
        "def score(value):\n    return 8\n",
        tests,
        2,
        "    return 7",
        "    return 8",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn python_later_imports_cannot_rewrite_applied_activation_controls() -> Result<(), String> {
    for tests in [
        "from unittest import skip as disabled\nfrom src.subject import score\n@disabled('skip')\ndef test_score():\n    assert score(0) == 8\nfrom other import disabled\n",
        "from unittest import skip as disabled\nfrom src.subject import score\nclass TestScore:\n    @disabled('skip')\n    def test_score(self):\n        assert score(0) == 8\n    from other import disabled\n",
        "import unittest\nfrom unittest import skip as disabled\nfrom src.subject import score\n@disabled('skip')\nclass Base(unittest.TestCase):\n    pass\nfrom other import disabled\nclass Child(Base):\n    def test_score(self):\n        self.assertEqual(score(0), 8)\n",
        "from unittest import skip as disabled\nfrom src.subject import score\nclass Mixin:\n    @disabled('skip')\n    def test_score(self):\n        assert score(0) == 8\nfrom other import disabled\nclass TestScore(Mixin):\n    pass\n",
    ] {
        let finding = classify_case(
            "def score(value):\n    return 8\n",
            tests,
            2,
            "    return 7",
            "    return 8",
        )?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{tests}");
    }
    let tests = "from other import disabled\nfrom src.subject import score\nclass TestScore:\n    @disabled('note')\n    def test_score(self):\n        assert score(0) == 8\n    from unittest import skip as disabled\n";
    let finding = classify_case(
        "def score(value):\n    return 8\n",
        tests,
        2,
        "    return 7",
        "    return 8",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

/// Keep the exact non-credit wording separate from the positive control below.
fn assert_strong_oracle_is_not_discrimination(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
    assert_eq!(
        finding.ripr.reveal.discriminate.summary,
        "Related Python test uses an `exact_value` oracle, but static evidence does not establish discrimination of the changed behavior."
    );
}

/// Explicit positional and keyword arguments both bypass the changed default.
#[test]
fn overridden_default_does_not_get_a_positive_discriminator_summary() -> Result<(), String> {
    let source =
        "def render(name, verbose=True):\n    return f\"[debug] {name}\" if verbose else name\n";
    for invocation in ["render(\"Sam\", verbose=False)", "render(\"Sam\", False)"] {
        let tests = format!(
            "from src.subject import render\n\ndef test_render():\n    assert {invocation} == \"Sam\"\n"
        );
        let finding = classify_case(
            source,
            &tests,
            1,
            "def render(name, verbose=False):",
            "def render(name, verbose=True):",
        )?;
        assert_strong_oracle_is_not_discrimination(&finding);
        assert!(
            finding
                .activation
                .missing_discriminators
                .iter()
                .any(|missing| missing.value == "call `render` without `verbose`"),
            "the existing omission repair must survive the narration fix"
        );
    }
    Ok(())
}

/// Omitting the parameter reaches the changed default under an exact observer.
#[test]
fn exercised_default_retains_its_positive_discriminator_summary() -> Result<(), String> {
    let finding = classify_case(
        "def render(name, verbose=True):\n    return f\"[debug] {name}\" if verbose else name\n",
        "from src.subject import render\n\ndef test_render():\n    assert render(\"Sam\") == \"[debug] Sam\"\n",
        1,
        "def render(name, verbose=False):",
        "def render(name, verbose=True):",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    assert_eq!(
        finding.ripr.reveal.discriminate.summary,
        "Related Python test uses an `exact_value` oracle; static evidence suggests the changed behavior is discriminated."
    );
    Ok(())
}

/// Observing the unchanged input is not observing the changed return value.
#[test]
fn orthogonal_strong_oracle_does_not_get_a_positive_discriminator_summary() -> Result<(), String> {
    let finding = classify_case(
        "def next_value(count):\n    return count - 1\n",
        "from src.subject import next_value\n\ndef test_input():\n    count = 5\n    result = next_value(count)\n    assert count == 5\n    assert result > 0\n",
        2,
        "    return count + 1",
        "    return count - 1",
    )?;
    assert_strong_oracle_is_not_discrimination(&finding);
    Ok(())
}

/// An exact normal-path result does not discriminate a changed raised exception.
#[test]
fn normal_path_oracle_does_not_credit_the_changed_error_path_summary() -> Result<(), String> {
    let finding = classify_case(
        "def parse(text):\n    if not text:\n        raise KeyError(\"empty\")\n    return int(text)\n",
        "from src.subject import parse\n\ndef test_parse():\n    assert parse(\"42\") == 42\n",
        3,
        "        raise ValueError(\"empty\")",
        "        raise KeyError(\"empty\")",
    )?;
    // The exact value assertion observes another behavior family, so it is
    // neither the row's oracle nor credited as a strong-but-orthogonal
    // observer: the finding names the missing exception discriminator (#5572).
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
    assert_eq!(
        finding.ripr.reveal.discriminate.summary,
        "Python preview adapter found no strong discriminator; missing proof: `raises KeyError matching \"empty\"`."
    );
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].oracle, None);
    assert_eq!(
        finding.related_tests[0].oracle_strength,
        OracleStrength::Unknown
    );
    Ok(())
}

/// A named static limitation remains the first authority even with a strong test.
#[test]
fn static_limit_keeps_its_named_discriminator_summary() -> Result<(), String> {
    let finding = classify_case(
        "def call_named(client, name):\n    return getattr(client, name)()\n",
        "from src.subject import call_named\n\ndef test_call():\n    assert call_named(client, \"total\") == 10\n",
        2,
        "    return client.total()",
        "    return getattr(client, name)()",
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
    assert_eq!(
        finding.ripr.reveal.discriminate.summary,
        "Static limit `dynamic_dispatch` prevents a safe Python discriminator claim."
    );
    Ok(())
}

/// Assert the fixture parsed into the intended owner before the verdict is read.
fn assert_owner(finding: &Finding, qualified: &str) {
    assert_eq!(
        finding.probe.owner.as_ref().map(|owner| owner.0.as_str()),
        Some(format!("python:src/subject.py::{qualified}").as_str()),
        "fixture must parse the intended owner"
    );
}

fn missing_boundary<'a>(finding: &'a Finding, value: &str) -> Option<&'a str> {
    finding
        .activation
        .missing_discriminators
        .iter()
        .find(|missing| missing.value == value)
        .map(|missing| missing.reason.as_str())
}

fn observed(finding: &Finding, value: &str) -> bool {
    finding
        .activation
        .observed_values
        .iter()
        .any(|fact| fact.value == value)
}

const INVENTORY_SOURCE: &str = "from dataclasses import dataclass\n\n\n@dataclass\nclass Item:\n    sku: str\n    on_hand: int\n\n\ndef reserve(item: Item, qty: int) -> int:\n    if qty >= item.on_hand:\n        raise ValueError(\"insufficient stock\")\n    item.on_hand -= qty\n    return item.on_hand\n";

/// The walkthrough inventory shape: `qty > item.on_hand` -> `qty >= item.on_hand`
/// with one exact assertion far from the boundary (10 on hand, reserve 3). The
/// old and new predicates agree on that input, so the exact oracle does not
/// discriminate the change and the finding must not be `exposed`.
#[test]
fn predicate_boundary_off_boundary_exact_oracle_is_weakly_exposed() -> Result<(), String> {
    let finding = classify_case(
        INVENTORY_SOURCE,
        "import unittest\n\nfrom src.subject import Item, reserve\n\n\nclass ItemTests(unittest.TestCase):\n    def test_reserve_reduces(self):\n        item = Item(\"a\", 10)\n        self.assertEqual(reserve(item, 3), 7)\n",
        11,
        "    if qty > item.on_hand:",
        "    if qty >= item.on_hand:",
    )?;
    assert_owner(&finding, "reserve");
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    assert_eq!(finding.ripr.reveal.observe.state, StageState::Yes);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
    // `item.on_hand` is unresolved, so the boundary is not established either
    // way: the finding is weak, the reason names the unresolved operand, and
    // no typed repair target (and so no repair card) is produced.
    assert!(
        finding.activation.missing_discriminators.is_empty(),
        "an unresolved operand must not become a typed repair target: {:?}",
        finding.activation.missing_discriminators
    );
    assert!(
        finding.missing.iter().any(|line| line.contains(
            "No strong related test call places qty equal to item.on_hand; observed qty values: 3; observed item.on_hand values: unresolved"
        )),
        "{:?}",
        finding.missing
    );
    assert!(observed(&finding, "qty = 3"), "{finding:?}");
    assert!(!observed(&finding, "qty == item.on_hand"));
    Ok(())
}

/// Unresolved attribute operands fail closed even when the test input happens
/// to sit on the boundary at runtime: static evidence cannot bind
/// `item.on_hand` through the `Item(...)` constructor.
#[test]
fn predicate_boundary_unresolved_attribute_operand_fails_closed() -> Result<(), String> {
    let finding = classify_case(
        INVENTORY_SOURCE,
        "from src.subject import Item, reserve\n\ndef test_reserve_all():\n    assert reserve(Item(\"a\", 3), 3) == 0\n",
        11,
        "    if qty > item.on_hand:",
        "    if qty >= item.on_hand:",
    )?;
    assert_owner(&finding, "reserve");
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // This test is on the boundary at runtime; naming the equality as missing
    // would hand out a repair card for a test that already exists.
    // The Python repair card requires a first missing discriminator, so an
    // empty list is what keeps the card off this finding.
    assert!(
        finding.activation.missing_discriminators.is_empty(),
        "{:?}",
        finding.activation.missing_discriminators
    );
    Ok(())
}

const DISCOUNT_SOURCE: &str =
    "def bulk_discount(quantity):\n    if quantity >= 100:\n        return 0.15\n    return 0.0\n";

/// Positive control: an exact assertion that calls the owner exactly at the
/// literal boundary pins the changed predicate and keeps `exposed`.
#[test]
fn predicate_boundary_literal_boundary_call_stays_exposed() -> Result<(), String> {
    let finding = classify_case(
        DISCOUNT_SOURCE,
        "from src.subject import bulk_discount\n\ndef test_bulk_discount_at_threshold():\n    assert bulk_discount(100) == 0.15\n",
        2,
        "    if quantity > 100:",
        "    if quantity >= 100:",
    )?;
    assert_owner(&finding, "bulk_discount");
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    assert!(finding.activation.missing_discriminators.is_empty());
    assert!(observed(&finding, "quantity == 100"), "{finding:?}");
    assert!(observed(&finding, "quantity = 100"), "{finding:?}");
    Ok(())
}

/// Discriminating negative for the control above: the same exact oracle one
/// step away from the literal boundary does not pin it.
#[test]
fn predicate_boundary_literal_off_boundary_call_is_weakly_exposed() -> Result<(), String> {
    let finding = classify_case(
        DISCOUNT_SOURCE,
        "from src.subject import bulk_discount\n\ndef test_bulk_discount_large():\n    assert bulk_discount(150) == 0.15\n",
        2,
        "    if quantity > 100:",
        "    if quantity >= 100:",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        missing_boundary(&finding, "quantity == 100"),
        Some(
            "No strong related test call places quantity equal to 100; observed quantity values: 150"
        )
    );
    assert!(!observed(&finding, "quantity == 100"));
    Ok(())
}

/// Two parameters bound to equal literals (positional and keyword forms, and
/// numeric forms Python compares equal) pin a parameter-to-parameter boundary.
#[test]
fn predicate_boundary_equal_parameter_bindings_stay_exposed() -> Result<(), String> {
    let source = "def reserve(on_hand, qty):\n    if qty >= on_hand:\n        return 0\n    return on_hand - qty\n";
    for call in [
        "reserve(3, 3)",
        "reserve(3, qty=3)",
        "reserve(qty=3, on_hand=3.0)",
    ] {
        let tests = format!(
            "from src.subject import reserve\n\ndef test_reserve_boundary():\n    assert {call} == 0\n"
        );
        let finding = classify_case(
            source,
            &tests,
            2,
            "    if qty > on_hand:",
            "    if qty >= on_hand:",
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{call}: {finding:?}");
        assert!(observed(&finding, "qty == on_hand"), "{call}");
    }
    Ok(())
}

/// A literal default binds an omitted parameter; an explicit override that
/// moves off the boundary does not.
#[test]
fn predicate_boundary_uses_literal_defaults_for_omitted_parameters() -> Result<(), String> {
    let source = "def over_limit(total, limit=10):\n    if total > limit:\n        return True\n    return False\n";
    let at_default = classify_case(
        source,
        "from src.subject import over_limit\n\ndef test_at_limit():\n    assert over_limit(10) == False\n",
        2,
        "    if total >= limit:",
        "    if total > limit:",
    )?;
    assert_eq!(at_default.class, ExposureClass::Exposed, "{at_default:?}");
    let overridden = classify_case(
        source,
        "from src.subject import over_limit\n\ndef test_at_limit():\n    assert over_limit(10, limit=5) == True\n",
        2,
        "    if total >= limit:",
        "    if total > limit:",
    )?;
    assert_eq!(overridden.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        missing_boundary(&overridden, "total == limit"),
        Some(
            "No strong related test call places total equal to limit; observed total values: 10; observed limit values: 5"
        )
    );
    Ok(())
}

/// Method calls bind arguments after the implicit `self` receiver.
#[test]
fn predicate_boundary_binds_method_arguments_after_self() -> Result<(), String> {
    let source = "class Cart:\n    def is_bulk(self, count):\n        if count >= 3:\n            return True\n        return False\n";
    for (count, expected) in [
        ("3", ExposureClass::Exposed),
        ("4", ExposureClass::WeaklyExposed),
    ] {
        let tests = format!(
            "from src.subject import Cart\n\ndef test_is_bulk():\n    cart = Cart()\n    assert cart.is_bulk({count}) == True\n"
        );
        let finding = classify_case(
            source,
            &tests,
            3,
            "        if count > 3:",
            "        if count >= 3:",
        )?;
        assert_owner(&finding, "Cart.is_bulk");
        assert_eq!(finding.class, expected, "count={count}: {finding:?}");
    }
    Ok(())
}

/// A computed operand cannot be bound to a test input: with literal owner
/// inputs visible it fails closed without inventing a typed discriminator.
/// A non-relational predicate is outside the boundary rule and keeps the
/// existing strong-oracle verdict.
#[test]
fn predicate_boundary_scope_computed_and_non_relational_predicates() -> Result<(), String> {
    let computed = classify_case(
        "def is_long(name):\n    if len(name) > 3:\n        return True\n    return False\n",
        "from src.subject import is_long\n\ndef test_is_long():\n    assert is_long(\"abcd\") == True\n",
        2,
        "    if len(name) >= 3:",
        "    if len(name) > 3:",
    )?;
    assert_eq!(computed.class, ExposureClass::WeaklyExposed);
    assert_eq!(computed.ripr.infect.state, StageState::Weak);
    assert!(
        computed.activation.missing_discriminators.is_empty(),
        "a computed operand must not become a typed repair target: {:?}",
        computed.activation.missing_discriminators
    );
    assert!(observed(&computed, "name = \"abcd\""));

    let equality = classify_case(
        "def is_zero(value):\n    if value == 0:\n        return True\n    return False\n",
        "from src.subject import is_zero\n\ndef test_is_zero():\n    assert is_zero(5) == False\n",
        2,
        "    if value != 0:",
        "    if value == 0:",
    )?;
    assert_eq!(equality.class, ExposureClass::Exposed);
    assert!(equality.activation.observed_values.is_empty());
    Ok(())
}

/// Without any literal owner argument the gate cannot see the activating
/// input in either direction (Rust's empty `call_values` rule). The verdict
/// stays with the oracle rules and the finding names the limitation, so the
/// unresolved activation stays visible rather than silently credited.
#[test]
fn predicate_boundary_without_literal_inputs_names_the_limitation() -> Result<(), String> {
    for tests in [
        "from src.subject import bulk_discount\n\ndef test_bulk_discount_local():\n    quantity = 150\n    assert bulk_discount(quantity) == 0.15\n",
        "from src.subject import bulk_discount\n\ndef test_bulk_discount_list():\n    assert bulk_discount(*[100]) == 0.15\n",
    ] {
        let finding = classify_case(
            DISCOUNT_SOURCE,
            tests,
            2,
            "    if quantity > 100:",
            "    if quantity >= 100:",
        )?;
        assert_owner(&finding, "bulk_discount");
        assert_eq!(finding.class, ExposureClass::Exposed, "{tests}");
        assert!(finding.activation.observed_values.is_empty());
        assert!(
            finding.evidence.iter().any(|line| line
                == "boundary_activation_unresolved: no strong related test call to `bulk_discount` binds a literal argument, so static evidence cannot place an input at the changed boundary of `quantity >= 100`"),
            "{:?}",
            finding.evidence
        );
    }
    Ok(())
}

/// Import aliases are followed when binding call arguments.
#[test]
fn predicate_boundary_follows_import_alias_calls() -> Result<(), String> {
    let finding = classify_case(
        DISCOUNT_SOURCE,
        "from src.subject import bulk_discount as bd\n\ndef test_alias_at_threshold():\n    assert bd(100) == 0.15\n",
        2,
        "    if quantity > 100:",
        "    if quantity >= 100:",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// #4559: a parametrized test whose cases never sit on the changed boundary
/// is not `exposed`. Before the fix `bulk_discount(quantity)` bound nothing,
/// activation stayed unresolved and the exact oracle credited `exposed`,
/// while the `>` -> `>=` mutant survives both cases.
#[test]
fn parametrize_cases_off_boundary_are_weakly_exposed() -> Result<(), String> {
    let finding = classify_case(
        DISCOUNT_SOURCE,
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity, rate\", [(50, 0.0), pytest.param(150, 0.15, id=\"bulk\")])\ndef test_bulk_discount(quantity, rate):\n    assert bulk_discount(quantity) == rate\n",
        2,
        "    if quantity > 100:",
        "    if quantity >= 100:",
    )?;
    assert_owner(&finding, "bulk_discount");
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_eq!(
        missing_boundary(&finding, "quantity == 100"),
        Some(
            "No strong related test call places quantity equal to 100; observed quantity values: 150, 50"
        )
    );
    assert!(observed(&finding, "quantity = 50"), "{finding:?}");
    assert!(observed(&finding, "quantity = 150"), "{finding:?}");
    Ok(())
}

/// Positive control for #4559: one case on the boundary keeps `exposed`, also
/// through a stacked decorator (the cases are the product).
#[test]
fn parametrize_case_on_boundary_stays_exposed() -> Result<(), String> {
    let finding = classify_case(
        DISCOUNT_SOURCE,
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity, rate\", [(50, 0.0), (100, 0.15)])\n@pytest.mark.parametrize(\"unused\", [\"a\", \"b\"])\ndef test_bulk_discount(quantity, rate, unused):\n    assert bulk_discount(quantity) == rate\n",
        2,
        "    if quantity > 100:",
        "    if quantity >= 100:",
    )?;
    assert_owner(&finding, "bulk_discount");
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(observed(&finding, "quantity == 100"), "{finding:?}");
    assert!(finding.activation.missing_discriminators.is_empty());
    Ok(())
}

/// Shapes whose cases are not statically certain keep the pre-#4559
/// unresolved verdict: argvalues named by a variable, `indirect=`, and an
/// argname the test body rebinds before the call.
#[test]
fn uncertain_parametrize_cases_stay_unresolved() -> Result<(), String> {
    for tests in [
        "import pytest\nfrom src.subject import bulk_discount\n\nCASES = [(50, 0.0)]\n\n@pytest.mark.parametrize(\"quantity, rate\", CASES)\ndef test_bulk_discount(quantity, rate):\n    assert bulk_discount(quantity) == rate\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [50], indirect=True)\ndef test_bulk_discount(quantity):\n    assert bulk_discount(quantity) == 0.0\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [50])\ndef test_bulk_discount(quantity):\n    quantity = quantity * 2\n    assert bulk_discount(quantity) == 0.0\n",
    ] {
        let finding = classify_case(
            DISCOUNT_SOURCE,
            tests,
            2,
            "    if quantity > 100:",
            "    if quantity >= 100:",
        )?;
        assert_owner(&finding, "bulk_discount");
        assert_eq!(finding.class, ExposureClass::Exposed, "{tests}");
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with("boundary_activation_unresolved: ")),
            "{tests}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}

/// #4612 review: an on-boundary case does not count as observed when the
/// case may never run (`marks=` skip), when the name reaching the owner call
/// may be another binding (a lambda parameter, a tuple or loop target), or
/// when the test is a unittest method, which pytest does not parametrize.
#[test]
fn shadowed_or_unrun_parametrize_cases_are_not_observed() -> Result<(), String> {
    for tests in [
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [50, pytest.param(100, marks=pytest.mark.skip)])\ndef test_bulk_discount(quantity):\n    assert bulk_discount(quantity) == 0.0\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    check = lambda quantity: bulk_discount(quantity) == 0.0\n    assert check(50)\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    for _, quantity in [(0, 50)]:\n        assert bulk_discount(quantity) == 0.0\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    _, quantity = (0, 50)\n    assert bulk_discount(quantity) == 0.0\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    tags[\"#\"], quantity = \"x\", 50\n    assert bulk_discount(quantity) == 0.0\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    with ctx() as (_, quantity):\n        assert bulk_discount(quantity) == 0.0\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    for (\n        _, quantity\n    ) in [(0, 50)]:\n        assert bulk_discount(quantity) == 0.0\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    assert all(bulk_discount(quantity) == 0.0 for quantity in [50])\n",
        "import pytest\nimport unittest\nfrom src.subject import bulk_discount\n\nclass TestBulk(unittest.TestCase):\n    @pytest.mark.parametrize(\"quantity\", [100])\n    def test_bulk_discount(self, quantity):\n        self.assertEqual(bulk_discount(quantity), 0.0)\n",
    ] {
        let finding = classify_case(
            DISCOUNT_SOURCE,
            tests,
            2,
            "    if quantity > 100:",
            "    if quantity >= 100:",
        )?;
        assert!(
            !observed(&finding, "quantity == 100"),
            "{tests}: {finding:?}"
        );
    }
    // Positive controls: a keyword argument passing the case name, and an
    // `=` inside the assertion message, are not rebindings, so the
    // on-boundary case is still observed.
    for tests in [
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    assert bulk_discount(quantity=quantity) == 0.0\n",
        "import pytest\nfrom src.subject import bulk_discount\n\n@pytest.mark.parametrize(\"quantity\", [100])\ndef test_bulk_discount(quantity):\n    assert bulk_discount(quantity) == 0.0, \"rate=0\"\n",
    ] {
        let finding = classify_case(
            DISCOUNT_SOURCE,
            tests,
            2,
            "    if quantity > 100:",
            "    if quantity >= 100:",
        )?;
        assert!(
            observed(&finding, "quantity == 100"),
            "{tests}: {finding:?}"
        );
    }
    Ok(())
}

/// #4565: `patch.object(module, "owner")` replaces the owner, so the call in
/// the test hits the mock. It is the same runtime substitution limit as
/// `monkeypatch.setattr`, in context-manager and decorator form; the same
/// test without the patch is not limited.
#[test]
fn patch_object_on_the_owner_is_a_mocked_module_limit() -> Result<(), String> {
    let cases = [
        (
            "from unittest import mock\nfrom src import subject\n\ndef test_bulk():\n    with mock.patch.object(subject, \"bulk_discount\", return_value=0.15):\n        assert subject.bulk_discount(100) == 0.15\n",
            ExposureClass::StaticUnknown,
        ),
        (
            "from unittest.mock import patch\nfrom src import subject\n\n@patch.object(subject, \"bulk_discount\", return_value=0.15)\ndef test_bulk(mocked):\n    assert subject.bulk_discount(100) == 0.15\n",
            ExposureClass::StaticUnknown,
        ),
        (
            "from src import subject\n\ndef test_bulk():\n    assert subject.bulk_discount(100) == 0.15\n",
            ExposureClass::Exposed,
        ),
        // `dispatch.object(` is not `patch.object(`.
        (
            "from src import subject\n\ndef test_bulk():\n    dispatch.object(subject)\n    dispatch(subject)\n    assert subject.bulk_discount(100) == 0.15\n",
            ExposureClass::Exposed,
        ),
    ];
    for (tests, expected) in cases {
        let finding = classify_case(
            DISCOUNT_SOURCE,
            tests,
            2,
            "    if quantity > 100:",
            "    if quantity >= 100:",
        )?;
        assert_owner(&finding, "bulk_discount");
        assert_eq!(finding.class, expected, "{tests}");
    }
    Ok(())
}

/// #4567: an exact assertion on the owner's own output credits `exposed`
/// whether the call goes through the owner's module, a result local bound
/// once from the call, or a function-local import. Before the fix these read
/// as "does not observe the changed owner's output" and stayed weak.
#[test]
fn module_qualified_and_result_local_owner_output_is_observed() -> Result<(), String> {
    for tests in [
        "from src import subject\n\ndef test_bulk():\n    assert subject.bulk_discount(100) == 0.15\n",
        "import src.subject as s\n\ndef test_bulk():\n    assert s.bulk_discount(100) == 0.15\n",
        "import src\n\ndef test_bulk():\n    assert src.subject.bulk_discount(100) == 0.15\n",
        "from src.subject import bulk_discount\n\ndef test_bulk():\n    rate = bulk_discount(100)\n    assert rate == 0.15\n",
        "from src import subject\n\ndef test_bulk():\n    rate = subject.bulk_discount(100)\n    assert rate == 0.15\n",
        "def test_bulk():\n    from src.subject import bulk_discount\n    assert bulk_discount(100) == 0.15\n",
        "from src.subject import bulk_discount\n\ndef test_bulk():\n    rate = bulk_discount(\n        100,\n    )  # whole call\n    assert rate == 0.15\n",
        "def test_bulk():\n    import src.subject as s\n    assert s.bulk_discount(100) == 0.15\n",
    ] {
        let finding = classify_case(
            DISCOUNT_SOURCE,
            tests,
            2,
            "    if quantity > 100:",
            "    if quantity >= 100:",
        )?;
        assert_owner(&finding, "bulk_discount");
        assert_eq!(finding.class, ExposureClass::Exposed, "{tests}");
        assert!(
            observed(&finding, "quantity == 100"),
            "{tests}: {finding:?}"
        );
    }
    Ok(())
}

/// Discriminating negatives for #4567. The boundary gate still applies to a
/// module-qualified call (off-boundary input stays weak and names the
/// boundary), a result local that is rebound is not the owner's output, and a
/// same-named function from another module carries no identity.
#[test]
fn module_call_credit_keeps_boundary_and_identity_guards() -> Result<(), String> {
    let off_boundary = classify_case(
        DISCOUNT_SOURCE,
        "from src import subject\n\ndef test_bulk():\n    assert subject.bulk_discount(150) == 0.15\n",
        2,
        "    if quantity > 100:",
        "    if quantity >= 100:",
    )?;
    assert_eq!(off_boundary.class, ExposureClass::WeaklyExposed);
    assert!(
        missing_boundary(&off_boundary, "quantity == 100").is_some(),
        "{off_boundary:?}"
    );
    for tests in [
        "from src.subject import bulk_discount\n\ndef test_bulk():\n    rate = bulk_discount(100)\n    rate += 0\n    assert rate == 0.15\n",
        "from src import other\n\ndef test_bulk():\n    assert other.bulk_discount(100) == 0.15\n",
        // The local holds the owner's result only on some paths.
        "from src.subject import bulk_discount\n\ndef test_bulk():\n    rate = bulk_discount(100) or 0.15\n    assert rate == 0.15\n",
        "from src.subject import bulk_discount\n\ndef test_bulk():\n    rate = bulk_discount(100) if FLAG else 0.15\n    assert rate == 0.15\n",
        // A keyword argument inside a multi-line call is not an assignment,
        // also after a docstring with an odd count of one quote character.
        "from src.subject import bulk_discount\n\ndef test_bulk():\n    check(\n        rate=bulk_discount(100)\n    )\n    assert rate == 0.15\n",
        "from src.subject import bulk_discount\n\ndef test_bulk():\n    \"\"\"It's the rate.\"\"\"\n    check(\n        rate=bulk_discount(100)\n    )\n    assert rate == 0.15\n",
        // The asserted value is a wrapper's result, not the owner's.
        "from src import subject\n\ndef test_bulk():\n    assert always_true(subject.bulk_discount(100)) == True\n",
        "from src import subject\n\ndef test_bulk():\n    rate = subject.bulk_discount(100)\n    assert always_true(rate) == True\n",
        // A later local import of the same name is the one the call uses.
        "def test_bulk():\n    from src.subject import bulk_discount\n    from src.other import bulk_discount\n    assert bulk_discount(100) == 0.15\n",
    ] {
        let finding = classify_case(
            DISCOUNT_SOURCE,
            tests,
            2,
            "    if quantity > 100:",
            "    if quantity >= 100:",
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{tests}");
    }
    Ok(())
}

// #7392: a return-value change inside a guarding `if` arm executes only for
// call inputs that satisfy the guard. When every strong related call provably
// fails the guard, the changed line never runs under an observed input, so a
// strong exact-value oracle still passes either way.
const QUOTE_SOURCE: &str = "def quote(total):\n    if total > 1000:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n";

/// Added-line classification with the new-side source threaded in, matching
/// the production diff path where the guarded-arm gate can read the owner
/// body.
fn classify_added_case(
    source: &str,
    tests: &str,
    line: usize,
    after: &str,
) -> Result<Finding, String> {
    let file = Path::new("src/subject.py");
    let owners = extract_owners(file, source);
    let tests = extract_tests(Path::new("tests/test_subject.py"), tests);
    classify_change_with_source(file, line, after, None, &owners, &tests, Some(source))
        .ok_or_else(|| "behavioral fixture must produce a finding".to_string())
}

#[test]
fn guarded_arm_return_value_below_guard_is_weakly_exposed() -> Result<(), String> {
    let finding = classify_added_case(
        QUOTE_SOURCE,
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 105.0\n",
        3,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    assert_eq!(finding.ripr.reveal.observe.state, StageState::Yes);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
    assert!(
        missing_boundary(&finding, "total > 1000").is_some(),
        "{finding:?}"
    );
    assert!(observed(&finding, "total = 100.0"), "{finding:?}");
    Ok(())
}

#[test]
fn guarded_arm_return_value_with_activating_input_stays_exposed() -> Result<(), String> {
    let finding = classify_added_case(
        QUOTE_SOURCE,
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(2000.0) == 2040.0\n",
        3,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

// The gate's premise is "EVERY strong related call provably fails the
// guard". Each of the following shapes leaves that premise unprovable, so
// the gate must fail open and keep the oracle verdict rather than
// downgrading a return an observed input actually exercises.

#[test]
fn or_guard_with_activating_keyword_operand_stays_exposed() -> Result<(), String> {
    // `quote(100.0, True)` enters the arm through `or force`; reading only
    // the `total > 1000` comparison would exclude it.
    let finding = classify_added_case(
        "def quote(total, force):\n    if total > 1000 or force:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0, True) == 102.0\n",
        3,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn unreadable_strong_call_keeps_oracle_credit() -> Result<(), String> {
    // `quote(*[2000.0])` binds no row (a `*args` unpack). The readable row
    // alone cannot establish that every strong input fails the guard.
    let finding = classify_added_case(
        QUOTE_SOURCE,
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 105.0\n    assert quote(*[2000.0]) == 2040.0\n",
        3,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn rebound_parameter_keeps_oracle_credit() -> Result<(), String> {
    // The owner rebinds `total` before the guard, so the guard reads 2000
    // regardless of the call binding; the call input cannot be excluded.
    let finding = classify_added_case(
        "def quote(total):\n    total = 2000\n    if total > 1000:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 2040.0\n",
        4,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

// #7874 counterexamples end to end: each call below enters the guarded arm
// at runtime and its assertion discriminates the changed return, so the
// verdict must stay `exposed` rather than a false `weakly_exposed`.

#[test]
fn mixed_boolean_conjunct_guard_keeps_oracle_credit() -> Result<(), String> {
    // `quote(100.0, True, False)` enters `(total > 1000 and ready) or force`
    // through `force`; the first conjunct evaluating false must not exclude
    // the row before the whole-condition `or` is noticed.
    let finding = classify_added_case(
        "def quote(total, force, ready):\n    if total > 1000 and ready or force:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0, True, False) == 102.0\n",
        3,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn multi_character_augmented_rebinding_keeps_oracle_credit() -> Result<(), String> {
    // `total **= 2` rebinds the parameter before the guard reads it.
    let finding = classify_added_case(
        "def quote(total):\n    total **= 2\n    if total > 1000:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 10200.0\n",
        4,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn tuple_rebinding_keeps_oracle_credit() -> Result<(), String> {
    // `total, other = 2000, 0` rebinds `total` through a destructuring
    // target, so the call binding cannot exclude the row.
    let finding = classify_added_case(
        "def quote(total):\n    total, other = 2000, 0\n    if total > 1000:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 2040.0\n",
        4,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn large_integer_guard_orders_exactly() -> Result<(), String> {
    // Python orders `9007199254740993 > 9007199254740992` exactly; rounding
    // both through f64 would falsely exclude the discriminating call.
    let finding = classify_added_case(
        "def quote(total):\n    if total > 9007199254740992:\n        return 42\n    return 0\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(9007199254740993) == 42\n",
        3,
        "        return 42",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn loop_body_rebinding_after_guard_keeps_oracle_credit() -> Result<(), String> {
    // `total *= 20` after the guard inside the loop changes what the guard
    // reads on the next iteration (the changed return executes for
    // `quote(100.0)`), so the call binding cannot exclude the row.
    let finding = classify_added_case(
        "def quote(total):\n    for _ in range(3):\n        if total > 1000:\n            return round(total * 1.02, 2)\n        total *= 20\n    return round(total * 1.05, 2)\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 2040.0\n",
        4,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn reversed_tuple_rebinding_keeps_oracle_credit() -> Result<(), String> {
    // The destructuring target with the parameter later on the line
    // (`other, total = 2000, 0`) rebinds it the same as the leading form.
    let finding = classify_added_case(
        "def quote(total):\n    other, total = 2000, 0\n    if total > 1000:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 2040.0\n",
        4,
        "        return round(total * 1.02, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn bool_numeric_guard_equality_follows_python() -> Result<(), String> {
    // Python enters `flag == 1` for `flag=True`; mixed-kind literal equality
    // would wrongly exclude the call.
    let finding = classify_added_case(
        "def quote(flag):\n    if flag == 1:\n        return 42\n    return 0\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(True) == 42\n",
        3,
        "        return 42",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn unguarded_return_value_with_source_keeps_oracle_credit() -> Result<(), String> {
    let finding = classify_added_case(
        "def quote(total):\n    return round(total * 1.05, 2)\n",
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 105.0\n",
        2,
        "    return round(total * 1.05, 2)",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn guarded_arm_gate_stays_off_without_source() -> Result<(), String> {
    // No-source entries (repo mode, deleted lines, the old test harness)
    // keep the pre-gate verdict: without the owner body the gate cannot
    // read the guard, so it fails open rather than downgrading.
    let file = Path::new("src/subject.py");
    let owners = extract_owners(file, QUOTE_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_subject.py"),
        "from src.subject import quote\n\ndef test_quote():\n    assert quote(100.0) == 105.0\n",
    );
    let finding = classify_change_with_old(
        file,
        3,
        "        return round(total * 1.02, 2)",
        None,
        &owners,
        &tests,
    )
    .ok_or_else(|| "behavioral fixture must produce a finding".to_string())?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

// RIPR-SPEC-0238 example 22 / #6585 rule 10: a changed dict value that equals an
// unchanged sibling must not credit `exposed` just because the sibling assertion
// contains that literal.
#[test]
fn equal_sibling_dict_value_stays_weakly_exposed() -> Result<(), String> {
    let finding = classify_case(
        "def build():\n    return {'port': 80, 'timeout': 80}\n",
        "from src.subject import build\n\ndef test_timeout():\n    assert build()['timeout'] == 80\n",
        2,
        "    return {'port': 8080, 'timeout': 80}",
        "    return {'port': 80, 'timeout': 80}",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.oracle_alignment.as_deref(), Some("orthogonal"));
    Ok(())
}

#[test]
fn equal_sibling_dict_changed_key_stays_exposed() -> Result<(), String> {
    let finding = classify_case(
        "def build():\n    return {'port': 80, 'timeout': 80}\n",
        "from src.subject import build\n\ndef test_port():\n    assert build()['port'] == 80\n",
        2,
        "    return {'port': 8080, 'timeout': 80}",
        "    return {'port': 80, 'timeout': 80}",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.oracle_alignment.as_deref(), Some("orthogonal"));
    Ok(())
}

#[test]
fn equal_sibling_dict_whole_collection_stays_exposed() -> Result<(), String> {
    let finding = classify_case(
        "def build():\n    return {'port': 80, 'timeout': 80}\n",
        "from src.subject import build\n\ndef test_cfg():\n    assert build() == {'port': 80, 'timeout': 80}\n",
        2,
        "    return {'port': 8080, 'timeout': 80}",
        "    return {'port': 80, 'timeout': 80}",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn equal_sibling_list_value_stays_weakly_exposed() -> Result<(), String> {
    let finding = classify_case(
        "def build():\n    return [80, 80]\n",
        "from src.subject import build\n\ndef test_second():\n    assert build()[1] == 80\n",
        2,
        "    return [8080, 80]",
        "    return [80, 80]",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.oracle_alignment.as_deref(), Some("orthogonal"));
    Ok(())
}

#[test]
fn equal_sibling_list_changed_index_stays_exposed() -> Result<(), String> {
    let finding = classify_case(
        "def build():\n    return [80, 80]\n",
        "from src.subject import build\n\ndef test_first():\n    assert build()[0] == 80\n",
        2,
        "    return [8080, 80]",
        "    return [80, 80]",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn equal_sibling_list_whole_collection_stays_exposed() -> Result<(), String> {
    let finding = classify_case(
        "def build():\n    return [80, 80]\n",
        "from src.subject import build\n\ndef test_all():\n    assert build() == [80, 80]\n",
        2,
        "    return [8080, 80]",
        "    return [80, 80]",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

const LAST_SOURCE: &str = "def last(iterable, default=None):\n    try:\n        return next(iter(iterable))\n    except StopIteration:\n        return None\n";
const LAST_OLD: &str = "        return default";
const LAST_NEW: &str = "        return None";

fn last_finding(tests: &str) -> Result<Finding, String> {
    classify_case(LAST_SOURCE, tests, 5, LAST_OLD, LAST_NEW)
}

fn last_does_not_vary_default(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert!(
        finding
            .missing
            .iter()
            .any(|entry| entry.contains("table rows do not vary default")),
        "{finding:?}"
    );
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.reason == "table rows do not vary default"),
        "{finding:?}"
    );
}

/// #6651: a unittest subTest table whose every row passes the same default
/// cannot catch `return default` -> `return None`. Before the gate the exact
/// oracle credited `exposed`.
#[test]
fn subtest_table_that_never_varies_default_is_weakly_exposed() -> Result<(), String> {
    let finding = last_finding(
        "import unittest\nfrom src.subject import last\n\nclass LastTests(unittest.TestCase):\n    def test_default(self):\n        for iterable, default in [\n            ([], None),\n            (iter([]), None),\n        ]:\n            with self.subTest(iterable=iterable, default=default):\n                self.assertEqual(last(iterable, default=default), default)\n",
    )?;
    last_does_not_vary_default(&finding);
    Ok(())
}

/// An unvaried nonzero default still fails after `return None`, so the table
/// remains a discriminator. Constant is not the same as undetectable.
#[test]
fn subtest_table_that_never_varies_nonzero_default_stays_exposed() -> Result<(), String> {
    let finding = last_finding(
        "import unittest\nfrom src.subject import last\n\nclass LastTests(unittest.TestCase):\n    def test_default(self):\n        for iterable, default in [\n            ([], 0),\n            (iter([]), 0),\n        ]:\n            with self.subTest(iterable=iterable, default=default):\n                self.assertEqual(last(iterable, default=default), default)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(finding.activation.missing_discriminators.is_empty());
    Ok(())
}

/// Positive control for #6651: one row that varies `default` keeps `exposed`.
#[test]
fn subtest_table_that_varies_default_stays_exposed() -> Result<(), String> {
    let finding = last_finding(
        "import unittest\nfrom src.subject import last\n\nclass LastTests(unittest.TestCase):\n    def test_default(self):\n        for iterable, default in [\n            ([], None),\n            (iter([]), 0),\n        ]:\n            with self.subTest(iterable=iterable, default=default):\n                self.assertEqual(last(iterable, default=default), default)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(finding.activation.missing_discriminators.is_empty());
    Ok(())
}

/// The same unvaried-default shape through pytest parametrize, which #4559
/// did not cover: that fix was the predicate-boundary miss, not a returned
/// default argument.
#[test]
fn parametrize_table_that_never_varies_default_is_weakly_exposed() -> Result<(), String> {
    let finding = last_finding(
        "import pytest\nfrom src.subject import last\n\n@pytest.mark.parametrize(\"iterable, default\", [([], None), (iter([]), None)])\ndef test_default(iterable, default):\n    assert last(iterable, default=default) == default\n",
    )?;
    last_does_not_vary_default(&finding);
    Ok(())
}

#[test]
fn parametrize_table_that_varies_default_stays_exposed() -> Result<(), String> {
    let finding = last_finding(
        "import pytest\nfrom src.subject import last\n\n@pytest.mark.parametrize(\"iterable, default\", [([], None), (iter([]), 0)])\ndef test_default(iterable, default):\n    assert last(iterable, default=default) == default\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// A loop table without `subTest` is still a table of expected values.
#[test]
fn loop_table_that_never_varies_default_is_weakly_exposed() -> Result<(), String> {
    let finding = last_finding(
        "from src.subject import last\n\ndef test_default():\n    for iterable, default in [([], None), (iter([]), None)]:\n        assert last(iterable, default=default) == default\n",
    )?;
    last_does_not_vary_default(&finding);
    Ok(())
}

/// A later unvaried table in the same test must not hide an earlier exact
/// oracle. `select_relevant_assertion` would prefer the later equal-strength
/// line and wrongly downgrade.
#[test]
fn unvaried_table_does_not_hide_an_earlier_exact_oracle_in_the_same_test() -> Result<(), String> {
    let finding = last_finding(
        "import unittest\nfrom src.subject import last\n\nclass LastTests(unittest.TestCase):\n    def test_default(self):\n        self.assertEqual(last([], default=0), 0)\n        for iterable, default in [([], None), (iter([]), None)]:\n            with self.subTest(iterable=iterable):\n                self.assertEqual(last(iterable, default=default), default)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// A second, non-table exact oracle may still discriminate; do not downgrade
/// the finding merely because a sibling table is unvaried.
#[test]
fn unvaried_table_does_not_hide_a_varied_exact_oracle() -> Result<(), String> {
    let finding = last_finding(
        "import unittest\nfrom src.subject import last\n\nclass LastTests(unittest.TestCase):\n    def test_default(self):\n        for iterable, default in [([], None), (iter([]), None)]:\n            with self.subTest(iterable=iterable):\n                self.assertEqual(last(iterable, default=default), default)\n    def test_nonzero_default(self):\n        self.assertEqual(last([], default=0), 0)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

const YY_SOURCE: &str = "def yy(year):\n    return year % 1000\n";
const YY_OLD: &str = "    return year % 100";
const YY_NEW: &str = "    return year % 1000";

fn yy_finding(tests: &str) -> Result<Finding, String> {
    classify_case(YY_SOURCE, tests, 2, YY_OLD, YY_NEW)
}

fn table_does_not_vary_year(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert!(
        finding
            .missing
            .iter()
            .any(|entry| entry.contains("table rows do not vary year")),
        "{finding:?}"
    );
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.reason == "table rows do not vary year"),
        "{finding:?}"
    );
}

/// #6651 loguru slice: every parametrized year is 20xx, so `year % 100` and
/// `year % 1000` agree on every row. Before per-row arithmetic evaluation the
/// exact oracle credited `exposed`.
#[test]
fn parametrize_20xx_years_do_not_distinguish_year_modulo() -> Result<(), String> {
    let finding = yy_finding(
        "import pytest\nfrom src.subject import yy\n\n@pytest.mark.parametrize(\"year\", [2015, 2016, 2020])\ndef test_yy(year):\n    assert yy(year) == year % 100\n",
    )?;
    table_does_not_vary_year(&finding);
    Ok(())
}

/// A year such as 1999 makes `x % 100` differ from `x % 1000`, so the table
/// remains a discriminator.
#[test]
fn parametrize_year_that_distinguishes_modulo_stays_exposed() -> Result<(), String> {
    let finding = yy_finding(
        "import pytest\nfrom src.subject import yy\n\n@pytest.mark.parametrize(\"year\", [2015, 1999])\ndef test_yy(year):\n    assert yy(year) == year % 100\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(finding.activation.missing_discriminators.is_empty());
    Ok(())
}

#[test]
fn subtest_20xx_years_do_not_distinguish_year_modulo() -> Result<(), String> {
    let finding = yy_finding(
        "import unittest\nfrom src.subject import yy\n\nclass YearTests(unittest.TestCase):\n    def test_yy(self):\n        for year in [2015, 2016, 2020]:\n            with self.subTest(year=year):\n                self.assertEqual(yy(year), year % 100)\n",
    )?;
    table_does_not_vary_year(&finding);
    Ok(())
}

/// Arithmetic that still changes on 20xx years (`+ 1` vs `+ 2`) must not be
/// weakened just because the table is a year column.
#[test]
fn parametrize_20xx_years_still_distinguish_plus_offset() -> Result<(), String> {
    let finding = classify_case(
        "def yy(year):\n    return year + 2\n",
        "import pytest\nfrom src.subject import yy\n\n@pytest.mark.parametrize(\"year\", [2015, 2016, 2020])\ndef test_yy(year):\n    assert yy(year) == year + 1\n",
        2,
        "    return year + 1",
        "    return year + 2",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// A table whose year cells are not integer literals stays on today's verdict.
#[test]
fn variable_year_table_does_not_claim_unvaried_modulo() -> Result<(), String> {
    let finding = yy_finding(
        "from src.subject import yy\n\nYEARS = [2015, 2016, 2020]\n\ndef test_yy():\n    for year in YEARS:\n        assert yy(year) == year % 100\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

const TOKENS_SOURCE: &str =
    "def tokens(t):\n    return {\n        \"YY\": t.tm_year % 1000,\n    }\n";
const TOKENS_OLD: &str = "        \"YY\": t.tm_year % 100,";
const TOKENS_NEW: &str = "        \"YY\": t.tm_year % 1000,";

/// Loguru shape: the changed dict field is `t.tm_year % 100`, and every
/// parametrized datetime is constructed from a 20xx year literal.
#[test]
fn datetime_20xx_rows_do_not_distinguish_tm_year_modulo() -> Result<(), String> {
    let finding = classify_case(
        TOKENS_SOURCE,
        "import pytest\nfrom datetime import datetime\nfrom src.subject import tokens\n\n@pytest.mark.parametrize(\"year\", [2015, 2016, 2020])\ndef test_yy(year):\n    assert tokens(datetime(year, 1, 1))[\"YY\"] == year % 100\n",
        3,
        TOKENS_OLD,
        TOKENS_NEW,
    )?;
    table_does_not_vary_year(&finding);
    Ok(())
}

#[test]
fn datetime_row_that_distinguishes_tm_year_modulo_stays_exposed() -> Result<(), String> {
    let finding = classify_case(
        TOKENS_SOURCE,
        "import pytest\nfrom datetime import datetime\nfrom src.subject import tokens\n\n@pytest.mark.parametrize(\"year\", [2015, 1999])\ndef test_yy(year):\n    assert tokens(datetime(year, 1, 1))[\"YY\"] == year % 100\n",
        3,
        TOKENS_OLD,
        TOKENS_NEW,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
