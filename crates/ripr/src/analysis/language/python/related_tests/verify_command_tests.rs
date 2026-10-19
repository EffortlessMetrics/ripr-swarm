use super::super::owners_tests::extract_tests;
use super::{
    PythonRelatedCandidate, PythonRelationKind, python_repair_placement_on,
    verify_command_for_test_on,
};
use crate::domain::ExposureClass;
use std::path::Path;

const UNITTEST: &str = "import unittest\nclass CheckoutTests(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(checkout())\n";

#[test]
fn unittest_literal_path_dots_cannot_name_another_module() {
    // Each literal path has the same old dotted spelling as a different
    // directory/module layout. The extension's separator is the only dot
    // that can be removed without changing filesystem identity.
    for path in [
        "tests/test_checkout.v2.py",
        "tests.v2/test_checkout.py",
        "tests/test_checkout.py.py",
        "tests.v2\\test_checkout.py",
    ] {
        let tests = extract_tests(Path::new(path), UNITTEST);
        assert_eq!(tests.len(), 1, "{path}");
        let test = &tests[0];
        assert_eq!(test.framework, "unittest");
        assert_eq!(test.qualified_name, "CheckoutTests.test_same");
        assert_eq!(verify_command_for_test_on(test, false), None, "{path}");
        assert_eq!(
            python_repair_placement_on(
                &ExposureClass::WeaklyExposed,
                &[PythonRelatedCandidate {
                    test,
                    relation: PythonRelationKind::SyntacticCall,
                }],
                false,
            ),
            None,
            "a refused selector cannot become a high-confidence repair: {path}"
        );
    }
}

#[test]
fn unittest_dot_free_paths_preserve_qualified_and_quoted_selectors() {
    for (path, command) in [
        (
            "tests/test_checkout.py",
            "python -m unittest tests.test_checkout.CheckoutTests.test_same",
        ),
        (
            "./tests/test_checkout.py",
            "python -m unittest tests.test_checkout.CheckoutTests.test_same",
        ),
        (
            "tests\\test_checkout.py",
            "python -m unittest tests.test_checkout.CheckoutTests.test_same",
        ),
        (
            "tests/my file.py",
            "python -m unittest 'tests.my file'.CheckoutTests.test_same",
        ),
        (
            "tests/o'brien.py",
            "python -m unittest 'tests.o'\\''brien'.CheckoutTests.test_same",
        ),
        (
            "tests/foo$(id).py",
            "python -m unittest 'tests.foo$(id)'.CheckoutTests.test_same",
        ),
    ] {
        let tests = extract_tests(Path::new(path), UNITTEST);
        assert_eq!(tests.len(), 1, "{path}");
        assert_eq!(
            verify_command_for_test_on(&tests[0], false).as_deref(),
            Some(command)
        );
    }
}

/// #8752: on Windows hosts the suggested command must use the cmd /
/// PowerShell double-quote grammar, and the node id must sit inside the
/// quotes — cmd only strips surrounding quotes when an argument both starts
/// and ends with one, so `pytest "path"::test` would pass the quote bytes to
/// pytest. Plain paths stay bare on every grammar.
#[test]
fn verify_command_quotes_for_the_windows_host_grammar() {
    let pytest_source = "def test_ok():\n    assert True\n";
    let spaced = extract_tests(Path::new("tests/my file.py"), pytest_source);
    assert_eq!(
        verify_command_for_test_on(&spaced[0], true).as_deref(),
        Some(r#"python -m pytest "tests/my file.py::test_ok""#)
    );
    let apostrophe = extract_tests(Path::new("tests/o'brien.py"), pytest_source);
    assert_eq!(
        verify_command_for_test_on(&apostrophe[0], true).as_deref(),
        Some(r#"python -m pytest "tests/o'brien.py::test_ok""#)
    );
    // An embedded double quote is doubled per the PowerShell grammar.
    let embedded_quote = extract_tests(Path::new("tests/we\"ird.py"), pytest_source);
    assert_eq!(
        verify_command_for_test_on(&embedded_quote[0], true).as_deref(),
        Some(r#"python -m pytest "tests/we""ird.py::test_ok""#)
    );
    // Separators normalize to `/` before quoting, so a backslash path is
    // bare forward-slash spelling on Windows.
    let backslash = extract_tests(Path::new("tests\\sub\\test_ok.py"), pytest_source);
    assert_eq!(
        verify_command_for_test_on(&backslash[0], true).as_deref(),
        Some("python -m pytest tests/sub/test_ok.py::test_ok")
    );
    // The unittest selector is quoted whole so cmd strips the quotes.
    let unittest_spaced = extract_tests(Path::new("tests/my file.py"), UNITTEST);
    assert_eq!(
        verify_command_for_test_on(&unittest_spaced[0], true).as_deref(),
        Some(r#"python -m unittest "tests.my file.CheckoutTests.test_same""#)
    );
    // A plain selector stays byte-identical to the POSIX rendering.
    let plain = extract_tests(Path::new("tests/test_checkout.py"), pytest_source);
    assert_eq!(
        verify_command_for_test_on(&plain[0], true).as_deref(),
        Some("python -m pytest tests/test_checkout.py::test_ok")
    );
}

#[test]
fn repair_placement_uses_first_runnable_oracle_relation() -> Result<(), String> {
    let dotted = extract_tests(Path::new("tests/test_checkout.v2.py"), UNITTEST);
    let plain = extract_tests(Path::new("tests/test_plain.py"), UNITTEST);
    let later = extract_tests(Path::new("tests/test_later.py"), UNITTEST);
    let candidates = [
        PythonRelatedCandidate {
            test: &dotted[0],
            relation: PythonRelationKind::SyntacticCall,
        },
        PythonRelatedCandidate {
            test: &later[0],
            relation: PythonRelationKind::SameStem,
        },
        PythonRelatedCandidate {
            test: &plain[0],
            relation: PythonRelationKind::SyntacticCall,
        },
        PythonRelatedCandidate {
            test: &later[0],
            relation: PythonRelationKind::SyntacticCall,
        },
    ];
    let placement = python_repair_placement_on(&ExposureClass::WeaklyExposed, &candidates, false)
        .ok_or("later runnable oracle relation must retain a repair route")?;
    assert_eq!(placement.suggested_test_file, "tests/test_plain.py");
    assert_eq!(placement.suggested_test_name, "test_same");
    assert_eq!(placement.suggested_test_line, None);
    assert_eq!(
        placement.verify_command,
        "python -m unittest tests.test_plain.CheckoutTests.test_same"
    );
    assert_eq!(
        python_repair_placement_on(&ExposureClass::WeaklyExposed, &candidates[..2], false),
        None,
        "an advisory relation cannot replace a refused oracle relation"
    );
    Ok(())
}

#[test]
fn repair_placement_quotes_the_verify_command_for_windows_hosts() -> Result<(), String> {
    // #8752: the stored node id keeps the raw spelling while the verify
    // command uses the host grammar, so a Windows consumer can paste it.
    let tests = extract_tests(
        Path::new("tests/my file.py"),
        "def test_same():\n    assert checkout()\n",
    );
    assert_eq!(tests.len(), 1);
    let candidates = [PythonRelatedCandidate {
        test: &tests[0],
        relation: PythonRelationKind::SyntacticCall,
    }];
    let placement = python_repair_placement_on(&ExposureClass::WeaklyExposed, &candidates, true)
        .ok_or("spaced pytest relation must retain a repair route")?;
    assert_eq!(placement.suggested_test_file, "tests/my file.py");
    assert_eq!(
        placement.suggested_test_node_id.as_deref(),
        Some("tests/my file.py::test_same")
    );
    assert_eq!(
        placement.verify_command,
        r#"python -m pytest "tests/my file.py::test_same""#
    );
    Ok(())
}

#[test]
fn repair_placement_distinguishes_same_file_method_names() -> Result<(), String> {
    let tests = extract_tests(
        Path::new("tests/test_mixed.v2.py"),
        "import unittest\nfrom checkout import checkout\nclass Bad(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(checkout())\nclass TestGood:\n    def test_same(self):\n        checkout()\n",
    );
    assert_eq!(tests.len(), 2);
    let candidates: Vec<_> = tests
        .iter()
        .map(|test| PythonRelatedCandidate {
            test,
            relation: PythonRelationKind::SyntacticCall,
        })
        .collect();
    let placement = python_repair_placement_on(&ExposureClass::WeaklyExposed, &candidates, false)
        .ok_or("pytest fallback must keep its source identity")?;
    assert_eq!(placement.suggested_test_file, "tests/test_mixed.v2.py");
    assert_eq!(placement.suggested_test_name, "test_same");
    assert_eq!(placement.suggested_test_line, Some(7));
    assert_eq!(
        placement.verify_command,
        "python -m pytest tests/test_mixed.v2.py::TestGood::test_same"
    );
    Ok(())
}

#[test]
fn pytest_keeps_literal_path_dots_in_its_file_selector() {
    for (path, command) in [
        (
            "tests/test_checkout.v2.py",
            "python -m pytest tests/test_checkout.v2.py::test_same",
        ),
        (
            "tests.v2/test_checkout.py",
            "python -m pytest tests.v2/test_checkout.py::test_same",
        ),
    ] {
        let tests = extract_tests(Path::new(path), "def test_same():\n    assert checkout()\n");
        assert_eq!(tests.len(), 1, "{path}");
        assert_eq!(tests[0].framework, "pytest");
        assert_eq!(
            verify_command_for_test_on(&tests[0], false).as_deref(),
            Some(command)
        );
    }
}
