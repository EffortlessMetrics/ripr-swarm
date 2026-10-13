use super::super::extract_tests;
use super::{
    PythonRelatedCandidate, PythonRelationKind, python_repair_placement, verify_command_for_test,
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
        assert_eq!(verify_command_for_test(test), None, "{path}");
        assert_eq!(
            python_repair_placement(
                &ExposureClass::WeaklyExposed,
                &[PythonRelatedCandidate {
                    test,
                    relation: PythonRelationKind::SyntacticCall,
                }],
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
        assert_eq!(verify_command_for_test(&tests[0]).as_deref(), Some(command));
    }
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
        assert_eq!(verify_command_for_test(&tests[0]).as_deref(), Some(command));
    }
}
