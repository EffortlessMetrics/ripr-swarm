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
        assertion_count("    x = f(3)\n    if x:\n        assert x == 21\n"),
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
        "from pytest import importorskip\nimportorskip('no_such_module')\n",
        "import pytest\npytestmark = pytest.mark.skip(reason='no')\n",
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
