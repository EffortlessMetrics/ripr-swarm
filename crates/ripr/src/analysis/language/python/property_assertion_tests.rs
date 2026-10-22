//! A property check pins no value (#9594): an assertion that relates the owner
//! to itself or to the loop variable it was handed holds for many wrong edits.

use super::owners_tests::extract_tests;
use super::*;
use std::path::Path;

fn strength_of(body: &str) -> OracleStrength {
    let source = format!("from m import f\n\ndef test_x():\n{body}");
    let tests = extract_tests(Path::new("tests/test_prop.py"), &source);
    assert_eq!(tests.len(), 1, "fixture must parse to one test: {source}");
    assert_eq!(tests[0].assertions.len(), 1, "one assertion: {source}");
    tests[0].assertions[0].oracle_strength.clone()
}

#[test]
fn given_an_idempotence_check_then_the_owner_compared_with_itself_pins_nothing() {
    for body in [
        "    for t in ('a', 'b'):\n        assert f(f(t)) == f(t)\n",
        "    assert f(f('a')) == f('a')\n",
        "    assert f('a') == f(f('a'))\n",
        "    assert f(g(f('a'))) == f('a')\n",
    ] {
        assert_eq!(strength_of(body), OracleStrength::Weak, "{body}");
    }
}

#[test]
fn given_a_loop_variable_returned_by_the_owner_then_the_identity_property_pins_nothing() {
    for body in [
        "    for n in range(1, 5):\n        assert f(1, n) == n\n",
        "    for n in range(1, 5):\n        assert n == f(1, n)\n",
        "    for a, n in [(1, 2)]:\n        assert f(a, n) == n\n",
        "    for n in range(3):\n        for m in range(3):\n            assert f(n, m) == m\n",
    ] {
        assert_eq!(strength_of(body), OracleStrength::Weak, "{body}");
    }
}

#[test]
fn given_an_independent_expected_value_then_the_equality_still_pins() {
    for body in [
        "    assert f(3, 7) == 21\n",
        "    assert f(3) == 3\n",
        "    for n in range(1, 5):\n        assert f(n) == n * 3\n",
        "    for n in range(1, 5):\n        assert f(1, n) == 5\n",
        "    n = 4\n    assert f(1, n) == n\n",
        "    assert f(g(3)) == 21\n",
    ] {
        assert_eq!(strength_of(body), OracleStrength::Strong, "{body}");
    }
}
