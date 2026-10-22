//! Assertions that cannot fail do not credit (#9691): dead branches, swallowed
//! `AssertionError`s, empty parametrize tables and module-level skips.

use super::owners_tests::extract_tests;
use super::test_activation::activation_control;
use super::*;
use std::path::Path;

fn parse(source: &str) -> Vec<PythonTest> {
    extract_tests(Path::new("tests/test_trap.py"), source)
}

fn first(source: &str) -> PythonTest {
    let mut tests = parse(source);
    assert_eq!(tests.len(), 1, "fixture must parse to one test: {source}");
    tests.remove(0)
}

fn assertion_count(body: &str) -> usize {
    first(&format!("from m import f\n\ndef test_x():\n{body}"))
        .assertions
        .len()
}

#[test]
fn given_a_constant_false_condition_when_it_guards_an_assert_then_none_is_read() {
    assert_eq!(
        assertion_count("    if False:\n        assert f(3) == 21\n"),
        0
    );
    assert_eq!(assertion_count("    if 0:\n        assert f(3) == 21\n"), 0);
    assert_eq!(
        assertion_count("    if None:\n        assert f(3) == 21\n"),
        0
    );
    assert_eq!(
        assertion_count("    while False:\n        assert f(3) == 21\n"),
        0
    );
}

#[test]
fn given_a_constant_condition_when_the_other_arm_asserts_then_only_the_live_arm_is_read() {
    assert_eq!(
        assertion_count("    if False:\n        pass\n    else:\n        assert f(3) == 21\n"),
        1
    );
    assert_eq!(
        assertion_count(
            "    if True:\n        assert f(3) == 21\n    else:\n        assert f(3) == 0\n"
        ),
        1
    );
}

#[test]
fn given_a_live_condition_when_it_guards_an_assert_then_the_assert_is_read() {
    assert_eq!(
        assertion_count("    value = f(3)\n    if value:\n        assert value == 21\n"),
        1
    );
}

#[test]
fn given_a_handler_that_swallows_assertion_errors_then_the_try_body_is_not_read() {
    for catch in [
        "AssertionError",
        "Exception",
        "BaseException",
        "(ValueError, AssertionError)",
    ] {
        let body =
            format!("    try:\n        assert f(3) == 21\n    except {catch}:\n        pass\n");
        assert_eq!(assertion_count(&body), 0, "except {catch}");
    }
    assert_eq!(
        assertion_count("    try:\n        assert f(3) == 21\n    except:\n        pass\n"),
        0
    );
}

#[test]
fn given_a_handler_that_fails_again_then_the_try_body_is_read() {
    assert_eq!(
        assertion_count(
            "    try:\n        assert f(3) == 21\n    except AssertionError:\n        raise\n"
        ),
        1
    );
    assert_eq!(
        assertion_count(
            "    try:\n        assert f(3) == 21\n    except AssertionError:\n        pytest.fail('x')\n"
        ),
        1
    );
}

#[test]
fn given_a_handler_that_asserts_or_nests_a_failure_then_the_try_body_is_read() {
    // (handler body, assertions read): a handler that asserts adds its own.
    for (handler, expected) in [
        ("assert False", 2),
        (
            "if f(1):\n            raise\n        else:\n            pass",
            1,
        ),
        ("self.assertTrue(False)", 2),
        ("self.assertIn('x', str(err))", 2),
    ] {
        let body = format!(
            "    try:\n        assert f(3) == 21\n    except AssertionError as err:\n        {handler}\n"
        );
        assert_eq!(assertion_count(&body), expected, "handler {handler}");
    }
}

#[test]
fn given_a_qualified_or_starred_catch_then_swallowing_still_hides_the_try_body() {
    assert_eq!(
        assertion_count(
            "    try:\n        assert f(3) == 21\n    except builtins.AssertionError:\n        pass\n"
        ),
        0
    );
    assert_eq!(
        assertion_count(
            "    try:\n        assert f(3) == 21\n    except* AssertionError:\n        pass\n"
        ),
        0
    );
    assert_eq!(
        assertion_count(
            "    try:\n        assert f(3) == 21\n    except AssertionError:\n        pass\n    finally:\n        assert f(4) == 28\n"
        ),
        1
    );
}

#[test]
fn given_a_truthy_constant_loop_then_its_assert_is_read() {
    assert_eq!(
        assertion_count("    while 1:\n        assert f(3) == 21\n        break\n"),
        1
    );
}

#[test]
fn given_a_handler_for_an_unrelated_error_then_the_try_body_is_read() {
    assert_eq!(
        assertion_count(
            "    try:\n        assert f(3) == 21\n    except ValueError:\n        pass\n"
        ),
        1
    );
}

#[test]
fn given_a_try_else_or_finally_assert_then_it_is_still_read() {
    assert_eq!(
        assertion_count(
            "    try:\n        y = f(3)\n    except Exception:\n        pass\n    else:\n        assert y == 21\n"
        ),
        1
    );
}

#[test]
fn given_an_empty_parametrize_table_then_the_test_is_activation_controlled() {
    for table in ["[]", "()"] {
        let source = format!(
            "import pytest\n\n@pytest.mark.parametrize('a,b,want', {table})\ndef test_x(a, b, want):\n    assert a * b == want\n"
        );
        assert!(
            activation_control(&first(&source)).is_some(),
            "table {table}"
        );
    }
    let filled = "import pytest\n\n@pytest.mark.parametrize('a,b,want', [(3, 7, 21)])\ndef test_x(a, b, want):\n    assert a * b == want\n";
    assert_eq!(activation_control(&first(filled)), None);
}

#[test]
fn given_a_module_level_skip_then_every_test_in_the_file_is_activation_controlled() {
    for header in [
        "import pytest\npytest.importorskip('no_such_module')\n",
        "import pytest\npytest.skip('no', allow_module_level=True)\n",
        "import pytest\nnp = pytest.importorskip('no_such_module')\n",
        "from pytest import importorskip\nimportorskip('no_such_module')\n",
        "import pytest\npytestmark = pytest.mark.skip(reason='no')\n",
        "from pytest import mark\npytestmark = mark.skip(reason='no')\n",
        "import pytest\npytestmark = [pytest.mark.skipif(True, reason='no')]\n",
    ] {
        let source = format!(
            "{header}\n\ndef test_a():\n    assert 1 == 1\n\n\ndef test_b():\n    assert 2 == 2\n"
        );
        let tests = parse(&source);
        assert_eq!(tests.len(), 2, "{header}");
        assert!(
            tests.iter().all(|test| activation_control(test).is_some()),
            "{header}"
        );
    }
}

#[test]
fn given_a_guarded_or_unrelated_module_statement_then_no_test_is_controlled() {
    for header in [
        "import pytest\nif False:\n    pytest.importorskip('no_such_module')\n",
        "import pytest\npytest.skip('no')\n",
        "import pytest\npytestmark = pytest.mark.usefixtures('db')\n",
        "import pytest\npytestmark = pytest.mark.slow\n",
    ] {
        let source = format!("{header}\n\ndef test_a():\n    assert 1 == 1\n");
        let tests = parse(&source);
        assert_eq!(tests.len(), 1, "{header}");
        assert_eq!(activation_control(&tests[0]), None, "{header}");
    }
}

/// A boolean-literal condition that never triggers the skip leaves the test
/// running (`skipif(False)`); every other condition is evaluated at collection
/// time and keeps the control.
#[test]
fn given_a_skip_condition_that_is_a_inert_literal_then_the_test_is_not_controlled() {
    for (decorator, controlled) in [
        ("pytest.mark.skipif(False, reason='never')", false),
        ("pytest.mark.skipif(condition=False, reason='never')", false),
        ("pytest.mark.xfail(False, reason='never')", false),
        ("pytest.mark.skipif(True, reason='always')", true),
        (
            "pytest.mark.skipif(sys.platform == 'win32', reason='os')",
            true,
        ),
        ("pytest.mark.skipif(DISABLED, reason='flag')", true),
        ("pytest.mark.skipif('False', reason='string')", true),
        ("pytest.mark.skipif(not False, reason='expr')", true),
        ("pytest.mark.skip(reason='always')", true),
        ("pytest.mark.xfail(reason='unconditional')", true),
        ("pytest.mark.xfail(True, reason='always')", true),
        // The runner rejects a boolean condition without a string reason, so the
        // body never runs; a second positional is a condition, not a reason; a
        // `condition=` keyword overrides the positional one.
        ("pytest.mark.skipif(False)", true),
        ("pytest.mark.skipif(condition=False)", true),
        ("pytest.mark.skipif(False, reason=None)", true),
        ("pytest.mark.skipif(False, 'never')", true),
        ("pytest.mark.skipif(False, True, reason='r')", true),
        (
            "pytest.mark.skipif(False, sys.platform == 'win32', reason='r')",
            true,
        ),
        (
            "pytest.mark.skipif(False, reason='r', condition=True)",
            true,
        ),
        ("pytest.mark.xfail(False)", true),
        ("pytest.mark.xfail(False, run=False)", true),
        ("pytest.mark.xfail(False, True, reason='r')", true),
        ("pytest.mark.skipif(*FLAGS, reason='r')", true),
    ] {
        let source = format!(
            "import pytest\nimport sys\n\n@{decorator}\ndef test_x():\n    assert 1 == 1\n"
        );
        assert_eq!(
            activation_control(&first(&source)).is_some(),
            controlled,
            "{decorator}"
        );
    }
    for (decorator, controlled) in [
        ("unittest.skipIf(False, 'never')", false),
        ("unittest.skipUnless(True, 'never')", false),
        ("unittest.skipIf(False, reason='never')", false),
        ("unittest.skipIf(False)", true),
        ("unittest.skipIf(False, 'never', 'extra')", true),
        ("unittest.skipIf(True, 'always')", true),
        ("unittest.skipUnless(False, 'always')", true),
    ] {
        let source = format!(
            "import unittest\n\nclass T(unittest.TestCase):\n    @{decorator}\n    def test_x(self):\n        self.assertEqual(1, 1)\n"
        );
        assert_eq!(
            activation_control(&first(&source)).is_some(),
            controlled,
            "{decorator}"
        );
    }
}

#[test]
fn given_an_inert_skip_on_a_module_mark_then_the_file_stays_active() {
    let inert = "import pytest\npytestmark = pytest.mark.skipif(False, reason='never')\n\n\ndef test_a():\n    assert 1 == 1\n";
    assert_eq!(activation_control(&first(inert)), None);
    let live = "import pytest\npytestmark = [pytest.mark.skipif(False, reason='never'), pytest.mark.skip(reason='no')]\n\n\ndef test_a():\n    assert 1 == 1\n";
    assert!(activation_control(&first(live)).is_some());
}

/// `importorskip` of a module every interpreter ships never skips; an
/// installable module, a `minversion` or a `reason` is a real collection-time
/// check and keeps the control.
#[test]
fn given_importorskip_of_an_always_present_module_then_the_file_stays_active() {
    for (header, controlled) in [
        ("import pytest\npytest.importorskip('json')\n", false),
        ("import pytest\npytest.importorskip(\"math\")\n", false),
        (
            "from pytest import importorskip\nimportorskip('re')\n",
            false,
        ),
        ("import pytest\npytest.importorskip('numpy')\n", true),
        ("import pytest\nnp = pytest.importorskip('numpy')\n", true),
        ("import pytest\nj = pytest.importorskip('json')\n", false),
        (
            "import pytest\npytest.importorskip('json', minversion='99')\n",
            true,
        ),
        (
            "import pytest\npytest.importorskip('json', reason='x')\n",
            true,
        ),
        ("import pytest\npytest.importorskip(name)\n", true),
        ("import pytest\npytest.importorskip('tkinter')\n", true),
    ] {
        let source = format!("{header}\n\ndef test_a():\n    assert 1 == 1\n");
        assert_eq!(
            activation_control(&first(&source)).is_some(),
            controlled,
            "{header}"
        );
    }
}
