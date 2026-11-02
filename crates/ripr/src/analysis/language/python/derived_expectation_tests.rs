//! Tests for demoting expectations computed from the code under test.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNERS: &str = "src/m.py";
const TESTS: &str = "tests/test_m.py";
const SOURCE: &str = "def f(x):\n    return x * 3\n\ndef g(x):\n    return f(x)\n\ndef ref(x):\n    return x + x\n\ndef get(x):\n    return f(x)\n\ndef join(x):\n    return f(x)\n";

fn strengths(tests_source: &str) -> Vec<OracleStrength> {
    let owners = extract_owners(Path::new(OWNERS), SOURCE);
    let mut tests = extract_tests(Path::new(TESTS), tests_source);
    derived_expectation::demote_derived_expectations(&mut tests, &owners, |path| {
        Some(if path == Path::new(TESTS) {
            tests_source
        } else {
            SOURCE
        })
    });
    tests
        .iter()
        .flat_map(|test| test.assertions.iter())
        .map(|assertion| assertion.oracle_strength.clone())
        .collect()
}

fn unit(body: &str) -> String {
    format!("import unittest\nfrom src.m import f, g, ref\n\nclass T(unittest.TestCase):\n{body}")
}

fn demoted(tests_source: &str) -> bool {
    let found = strengths(tests_source);
    assert_eq!(
        found.len(),
        1,
        "fixture must carry one assertion: {tests_source}"
    );
    found[0] == OracleStrength::Weak
}

fn credited(tests_source: &str) -> bool {
    let found = strengths(tests_source);
    assert_eq!(
        found.len(),
        1,
        "fixture must carry one assertion: {tests_source}"
    );
    found[0] == OracleStrength::Strong
}

#[test]
fn given_an_expectation_bound_from_the_owner_then_it_is_demoted() {
    let shapes = [
        "    def setUp(self):\n        self.expected = f(2)\n    def test_a(self):\n        self.assertEqual(f(2), self.expected)\n",
        "    expected = f(2)\n    def test_a(self):\n        self.assertEqual(f(2), self.expected)\n",
        "    def test_a(self):\n        e = f(2)\n        self.assertEqual(f(2), e)\n",
        "    def test_a(self):\n        e = f(2)\n        self.assertEqual(e, f(2))\n",
        "    def setUpClass(cls):\n        cls.expected = f(2)\n    def test_a(self):\n        self.assertEqual(f(2), self.expected)\n",
    ];
    for shape in shapes {
        assert!(demoted(&unit(shape)), "{shape}");
    }
    let module = "import unittest\nfrom src.m import f\nEXP = f(2)\n\nclass T(unittest.TestCase):\n    def test_a(self):\n        self.assertEqual(f(2), EXP)\n";
    assert!(demoted(module));
}

#[test]
fn given_an_expectation_rewritten_from_the_owner_then_it_is_demoted() {
    for body in [
        "self.assertEqual(f(2), g(2))",
        "self.assertEqual(g(2), f(2))",
        "self.assertEqual(f(2) - f(2), 0)",
        "self.assertEqual(f(2), f(2) + 0)",
        "self.assertEqual(f(2), f(1) * 2)",
        "self.assertEqual(f(2), int(f'{f(2)}'))",
    ] {
        assert!(
            demoted(&unit(&format!("    def test_a(self):\n        {body}\n"))),
            "{body}"
        );
    }
}

#[test]
fn given_a_pytest_expectation_computed_from_the_owner_then_it_is_demoted() {
    let local =
        "from src.m import f\n\ndef test_a():\n    expected = f(2)\n    assert f(2) == expected\n";
    assert!(demoted(local));
    let forwarded = "from src.m import f, g\n\ndef test_a():\n    assert f(2) == g(2)\n";
    assert!(demoted(forwarded));
}

#[test]
fn given_an_independent_expectation_then_the_credit_stays() {
    for body in [
        "self.assertEqual(f(2), 6)",
        "self.assertEqual(f(2), ref(2))",
        "self.assertEqual(f(2) + f(3), 15)",
    ] {
        assert!(
            credited(&unit(&format!("    def test_a(self):\n        {body}\n"))),
            "{body}"
        );
    }
    let setup = "    def setUp(self):\n        self.expected = 6\n    def test_a(self):\n        self.assertEqual(f(2), self.expected)\n";
    assert!(credited(&unit(setup)));
    let reference = "from src.m import f, ref\n\ndef test_a():\n    expected = ref(2)\n    assert f(2) == expected\n";
    assert!(credited(reference));
    let literal = "from src.m import f\n\ndef test_a():\n    assert f(2) == 6\n";
    assert!(credited(literal));
}

#[test]
fn given_a_binding_that_names_the_owner_only_in_a_string_then_the_credit_stays() {
    let source = unit(
        "    def setUp(self):\n        self.expected = 'f(2)'\n    def test_a(self):\n        self.assertEqual(f(2), self.expected)\n",
    );
    assert!(credited(&source));
}

#[test]
fn given_an_unrelated_call_on_each_side_then_the_credit_stays() {
    let source = "from src.m import f\n\ndef test_a():\n    assert str(f(2)) == str(6)\n";
    assert!(credited(source));
}

#[test]
fn given_an_input_both_sides_read_then_the_credit_stays() {
    for source in [
        "from src.m import f, ref\n\ndef test_a():\n    n = ref(2)\n    assert f(n) == n * 2\n",
        "from src.m import f, ref\n\ndef test_a():\n    t = ref(2)\n    assert f(t) == ref(t)\n",
        "from src.m import f, ref\n\ndef test_a():\n    sq = ref(3)\n    assert f(sq) == sq.side ** 2\n",
        "import unittest\nfrom src.m import f, ref\n\nclass T(unittest.TestCase):\n    def test_a(self):\n        n = ref(2)\n        self.assertEqual(f(n), n * 2)\n",
    ] {
        assert!(credited(source), "{source}");
    }
}

#[test]
fn given_a_value_rooted_in_the_other_sides_call_then_it_is_still_demoted() {
    for source in [
        "from src.m import f\n\ndef test_a():\n    e = f(2)\n    assert f(2) == e + 0\n",
        "from src.m import f, g\n\ndef test_a():\n    e = g(2)\n    assert f(2) == e\n",
        "from src.m import f\n\ndef test_a():\n    assert abs(f(2)) == f(2)\n",
    ] {
        assert!(demoted(source), "{source}");
    }
}

#[test]
fn given_a_receiver_of_unknown_type_then_a_same_named_owner_is_not_matched() {
    for source in [
        "from src.m import g\n\ndef test_a():\n    d = {}\n    assert g(2) == d.get('k')\n",
        "from src.m import g\n\ndef test_a():\n    assert g(2) == ''.join(['a'])\n",
        "from src.m import g\n\ndef test_a():\n    assert g(2) == [6].index(6).get(1)\n",
    ] {
        assert!(credited(source), "{source}");
    }
}

#[test]
fn given_an_imported_or_owner_built_receiver_then_the_owner_is_matched() {
    for source in [
        "import src.m as m\n\ndef test_a():\n    assert m.f(2) == m.g(2)\n",
        "from src.m import f\nimport src.m as m\n\ndef test_a():\n    assert f(2) == m.get(2)\n",
    ] {
        assert!(demoted(source), "{source}");
    }
}
