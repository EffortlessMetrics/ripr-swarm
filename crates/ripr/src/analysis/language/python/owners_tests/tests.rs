use super::super::related_tests::{find_related_tests, verify_command_for_test};
use super::super::source_facts::extract_source_facts;
use super::extract_owners;
use std::path::Path;

// RIPR-SPEC-0028: discovery must follow the default framework name prefix,
// not impose an underscore. These fixtures also pin non-test boundaries.
const PYTEST_SOURCE: &str = r#"
def test():
    assert 2 + 2 == 4

def testCamelCase():
    assert 2 + 2 == 4

def testlowercase():
    assert 2 + 2 == 4

def test_with_underscore():
    assert 2 + 2 == 4

def TestWrongCase():
    assert 2 + 2 == 4

def contest():
    assert 2 + 2 == 4

def _test_private():
    assert 2 + 2 == 4

def helper():
    def testNested():
        assert 2 + 2 == 4

class TestCart:
    def test(self):
        assert 2 + 2 == 4

    def testCamelCase(self):
        assert 2 + 2 == 4

    def testlowercase(self):
        assert 2 + 2 == 4

    def test_with_underscore(self):
        assert 2 + 2 == 4

    def helper(self):
        assert 2 + 2 == 4

class CartHelper:
    def testNotCollected(self):
        assert 2 + 2 == 4
"#;

const UNITTEST_SOURCE: &str = r#"
import unittest

class CartChecks(unittest.TestCase):
    def test(self):
        self.assertEqual(2 + 2, 4)

    def testCamelCase(self):
        self.assertEqual(2 + 2, 4)

    def testlowercase(self):
        self.assertEqual(2 + 2, 4)

    def test_with_underscore(self):
        self.assertEqual(2 + 2, 4)

    def TestWrongCase(self):
        self.assertEqual(2 + 2, 4)

    def contest(self):
        self.assertEqual(2 + 2, 4)

    def _test_private(self):
        self.assertEqual(2 + 2, 4)

    def helper(self):
        self.assertEqual(2 + 2, 4)
"#;

const ASYNC_SOURCE: &str = r#"
async def testAsync():
    assert 2 + 2 == 4

async def test_async():
    assert 2 + 2 == 4

async def contest():
    assert 2 + 2 == 4

async def _test_private():
    assert 2 + 2 == 4

class TestAsync:
    async def testAsync(self):
        assert 2 + 2 == 4

    async def test_async(self):
        assert 2 + 2 == 4

    async def helper(self):
        assert 2 + 2 == 4
"#;

const RELATION_SOURCE: &str = r#"
import unittest
from src.pricing import price, sibling

def testPytestPrice():
    assert price(10) == 20

class CartChecks(unittest.TestCase):
    def testUnittestPrice(self):
        self.assertEqual(price(10), 20)

def testSibling():
    assert sibling(10) == 30

def helper_price():
    assert price(10) == 20
"#;

fn collect(
    source: &str,
    expected: &[(&str, &str)],
) -> Result<Vec<super::super::PythonTest>, String> {
    let file = Path::new("tests/test_collection.py");
    let facts = extract_source_facts(file, source);
    if !facts.limitations.is_empty() {
        return Err(format!(
            "fixture must parse without limits: {:?}",
            facts.limitations
        ));
    }
    let mut actual = facts
        .tests
        .iter()
        .map(|test| (test.qualified_name.as_str(), test.framework.as_str()))
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    if expected.is_empty() || actual != expected {
        return Err(format!("expected nonempty {expected:?}, got {actual:?}"));
    }
    for test in &facts.tests {
        if test.file != file || test.line == 0 || test.assertions.len() != 1 {
            return Err(format!("test lost its location or assertion: {test:?}"));
        }
        if !test.fixtures.is_empty() {
            return Err(format!("self must not become a fixture: {test:?}"));
        }
    }
    Ok(facts.tests)
}

#[test]
fn python_default_name_prefix_matches_pytest_collection() -> Result<(), String> {
    collect(
        PYTEST_SOURCE,
        &[
            ("test", "pytest"),
            ("testCamelCase", "pytest"),
            ("testlowercase", "pytest"),
            ("test_with_underscore", "pytest"),
            ("TestCart.test", "pytest"),
            ("TestCart.testCamelCase", "pytest"),
            ("TestCart.testlowercase", "pytest"),
            ("TestCart.test_with_underscore", "pytest"),
        ],
    )?;
    Ok(())
}

#[test]
fn python_default_name_prefix_matches_unittest_loader() -> Result<(), String> {
    collect(
        UNITTEST_SOURCE,
        &[
            ("CartChecks.test", "unittest"),
            ("CartChecks.testCamelCase", "unittest"),
            ("CartChecks.testlowercase", "unittest"),
            ("CartChecks.test_with_underscore", "unittest"),
        ],
    )?;
    Ok(())
}

#[test]
fn async_python_tests_use_default_name_prefix() -> Result<(), String> {
    collect(
        ASYNC_SOURCE,
        &[
            ("testAsync", "pytest"),
            ("test_async", "pytest"),
            ("TestAsync.testAsync", "pytest"),
            ("TestAsync.test_async", "pytest"),
        ],
    )?;
    Ok(())
}

#[test]
fn nested_function_owners_get_qualified_names() -> Result<(), String> {
    let owners = extract_owners(
        Path::new("src/factory.py"),
        "def outer():\n    def inner():\n        return 2\n    return inner\n\nclass Builder:\n    def build(self):\n        def step():\n            return 3\n        return step()\n",
    );
    let qualified: Vec<&str> = owners
        .iter()
        .map(|owner| owner.qualified_name.as_str())
        .collect();
    assert!(
        qualified.contains(&"outer.inner"),
        "nested def must be an owner qualified by its enclosing function: {qualified:?}"
    );
    assert!(
        qualified.contains(&"Builder.build.step"),
        "def nested in a method must not be credited to the method owner: {qualified:?}"
    );
    let inner = owners
        .iter()
        .find(|owner| owner.qualified_name == "outer.inner")
        .ok_or_else(|| "inner owner missing".to_string())?;
    let outer = owners
        .iter()
        .find(|owner| owner.qualified_name == "outer")
        .ok_or_else(|| "outer owner missing".to_string())?;
    assert!(
        inner.start_line > outer.start_line && inner.end_line <= outer.end_line,
        "inner owner span must lie inside outer: {inner:?} vs {outer:?}"
    );
    Ok(())
}

#[test]
fn nested_owner_records_whether_the_enclosing_body_calls_it() -> Result<(), String> {
    let owners = extract_owners(
        Path::new("src/nested.py"),
        concat!(
            "def calls(n):\n    def inner(m):\n        return m\n    return inner(n)\n\n",
            "def spaced(n):\n    def inner1(m):\n        return m\n    return inner1 (n)\n\n",
            "def defines_only(n):\n    def inner2(m):\n        return m\n    return n\n\n",
            "def recursion_only(n):\n    def inner3(m):\n        return inner3(m - 1) if m else m\n    return n\n\n",
            "def returns_callable(n):\n    def inner4(m):\n        return m\n    return inner4\n\n",
            "def unused_sibling(n):\n    def inner5(m):\n        return m\n    def unused():\n        return inner5(n)\n    return n\n\n",
            "def rebound(n):\n    def inner6(m):\n        return m\n    inner6 = lambda m: 2\n    return inner6(n)\n\n",
            "def used_sibling(n):\n    def inner7(m):\n        return m\n    def helper():\n        return inner7(n)\n    return helper()\n",
        ),
    );
    let flag = |qualified: &str| -> Result<bool, String> {
        owners
            .iter()
            .find(|owner| owner.qualified_name == qualified)
            .map(|owner| owner.nested_called_by_parent)
            .ok_or_else(|| format!("owner {qualified} missing"))
    };
    assert!(
        flag("calls.inner")?,
        "an enclosing body that calls the nested def reaches it"
    );
    assert!(
        flag("spaced.inner1")?,
        "a call spelled with a space before the parenthesis is still a call"
    );
    assert!(
        !flag("defines_only.inner2")?,
        "an enclosing body that only defines the nested def gives no path"
    );
    assert!(
        !flag("recursion_only.inner3")?,
        "a call inside the nested body itself is not a call from the enclosing body"
    );
    assert!(
        !flag("returns_callable.inner4")?,
        "returning the nested def without calling it stays fail-closed"
    );
    assert!(
        !flag("unused_sibling.inner5")?,
        "a call from a sibling def the enclosing body never calls is not an edge"
    );
    assert!(
        !flag("rebound.inner6")?,
        "a rebound name may call a different binding, so it stays fail-closed"
    );
    assert!(
        !flag("used_sibling.inner7")?,
        "a call that only a sibling def makes stays fail-closed even when that sibling runs"
    );
    assert!(
        !flag("calls")?,
        "a top-level owner is never a nested-called owner"
    );
    Ok(())
}

#[test]
fn nested_owner_records_when_the_enclosing_body_discards_the_result() -> Result<(), String> {
    let owners = extract_owners(
        Path::new("src/nested_result.py"),
        concat!(
            "def statement_only(n):\n    def a1(m):\n        return m\n    a1(n)\n    return 2\n\n",
            "def returned(n):\n    def a2(m):\n        return m\n    return a2(n)\n\n",
            "def bound(n):\n    def a3(m):\n        return m\n    x = a3(n)\n    return x\n\n",
            "def statement_and_return(n):\n    def a4(m):\n        return m\n    a4(n)\n    return a4(n)\n\n",
            "def statement_in_if(n):\n    def a5(m):\n        return m\n    if (n):\n        a5(n)\n    return 2\n\n",
            "def passed_on(n):\n    def a6(m):\n        return m\n    print(a6(n))\n    return 2\n\n",
            "def never_called(n):\n    def a7(m):\n        return m\n    return 2\n\n",
            "def returns_the_def(n):\n    def a8(m):\n        return m\n    a8(n)\n    return a8\n\n",
            "def registers_the_def(n):\n    def a9(m):\n        return m\n    a9(n)\n    register(a9)\n    return 2\n",
        ),
    );
    let discarded = |qualified: &str| -> Result<bool, String> {
        owners
            .iter()
            .find(|owner| owner.qualified_name == qualified)
            .map(|owner| owner.nested_result_discarded)
            .ok_or_else(|| format!("owner {qualified} missing"))
    };
    assert!(
        discarded("statement_only.a1")?,
        "a call that is a statement of its own discards the result"
    );
    assert!(
        discarded("statement_in_if.a5")?,
        "a statement call inside an `if` body still discards the result"
    );
    for (qualified, why) in [
        ("returned.a2", "a returned call"),
        ("bound.a3", "a bound call"),
        ("statement_and_return.a4", "one call that is returned"),
        ("passed_on.a6", "a call passed to another call"),
        ("never_called.a7", "no call at all"),
        ("returns_the_def.a8", "a returned def that a caller can run"),
        ("registers_the_def.a9", "a def handed to another call"),
    ] {
        assert!(
            !discarded(qualified)?,
            "{why} may carry the result on, so the fact stays false"
        );
    }
    Ok(())
}

#[test]
fn nested_owner_call_check_skips_defs_in_compound_blocks_lambdas_and_rebinds() -> Result<(), String>
{
    let owners = extract_owners(
        Path::new("src/nested_compound.py"),
        concat!(
            "def called_in_if(n):\n    def a1(m):\n        return m\n    if (n):\n        return a1(n)\n    return 0\n\n",
            "def called_in_if_header(n):\n    def a2(m):\n        return m\n    if a2(n):\n        return 1\n    return 0\n\n",
            "def unused_def_in_if(n):\n    def b1(m):\n        return m\n    if (n):\n        def unused():\n            return b1(n)\n    return n\n\n",
            "def unused_def_in_with(n):\n    def b2(m):\n        return m\n    with open(n):\n        def unused():\n            return b2(n)\n    return n\n\n",
            "def unused_def_in_for(n):\n    def b3(m):\n        return m\n    for x in (n):\n        def unused():\n            return b3(x)\n    return n\n\n",
            "def unused_def_in_try(n):\n    def b4(m):\n        return m\n    try:\n        def unused():\n            return b4(n)\n    except ValueError:\n        pass\n    return n\n\n",
            "def unused_class_in_if(n):\n    def b5(m):\n        return m\n    if (n):\n        class Klass:\n            v = b5(n)\n    return n\n\n",
            "def unused_lambda(n):\n    def c1(m):\n        return m\n    f = lambda: c1(n)\n    return n\n\n",
            "def call_before_lambda(n):\n    def c2(m):\n        return m\n    r = c2(n); f = lambda: 0\n    return r\n\n",
            "def lambda_in_with_header(n):\n    def c3(m):\n        return m\n    with open(lambda: c3(n)):\n        pass\n    return n\n\n",
            "def lambda_word_in_string(n):\n    def e1(m):\n        return m\n    return ('lambda', e1(n))[1]\n\n",
            "def lambda_as_if_test(n):\n    def e2(m):\n        return m\n    if lambda: e2(n):\n        pass\n    return n\n\n",
            "def decorated_nested_in_if(n):\n    def e3(m):\n        return m\n    if (n):\n        @decorate(e3(n))\n        def inner():\n            return 1\n    return n\n\n",
            "def defined_twice(n):\n    def d1(m):\n        return m\n    def d1(m):\n        return m + 1\n    return d1(n)\n\n",
            "def redefined_in_if(n):\n    def d2(m):\n        return m\n    if (n):\n        def d2(m):\n            return m + 1\n    return d2(n)\n",
        ),
    );
    let flag = |qualified: &str| -> Result<bool, String> {
        owners
            .iter()
            .find(|owner| owner.qualified_name == qualified)
            .map(|owner| owner.nested_called_by_parent)
            .ok_or_else(|| format!("owner {qualified} missing"))
    };
    assert!(
        flag("called_in_if.a1")?,
        "a call inside an `if` body of the enclosing function is still a call"
    );
    assert!(
        flag("called_in_if_header.a2")?,
        "a call in an `if` test runs with the enclosing body"
    );
    for (qualified, why) in [
        ("unused_def_in_if.b1", "if"),
        ("unused_def_in_with.b2", "with"),
        ("unused_def_in_for.b3", "for"),
        ("unused_def_in_try.b4", "try"),
        ("unused_class_in_if.b5", "class under if"),
    ] {
        assert!(
            !flag(qualified)?,
            "a call from a def nested under `{why}` that nothing calls is not an edge"
        );
    }
    assert!(
        !flag("unused_lambda.c1")?,
        "a call inside a lambda that nothing calls is not an edge"
    );
    assert!(
        flag("call_before_lambda.c2")?,
        "a call before a lambda in the same statement still runs"
    );
    assert!(
        !flag("lambda_in_with_header.c3")?,
        "a call inside a lambda in a compound statement's header is not an edge"
    );
    assert!(
        flag("lambda_word_in_string.e1")?,
        "the word lambda inside a string literal does not mask a real call"
    );
    assert!(
        !flag("lambda_as_if_test.e2")?,
        "a call inside a lambda used as an `if` test is deferred, not an edge"
    );
    assert!(
        flag("decorated_nested_in_if.e3")?,
        "a decorator expression of a nested def is evaluated when the enclosing body runs"
    );
    assert!(
        !flag("defined_twice.d1")?,
        "a name defined twice may call the second binding, so it stays fail-closed"
    );
    assert!(
        !flag("redefined_in_if.d2")?,
        "a conditional redefinition may call the other binding, so it stays fail-closed"
    );
    Ok(())
}

#[test]
fn non_underscore_names_preserve_selectors_and_owner_relations() -> Result<(), String> {
    let tests = collect(
        RELATION_SOURCE,
        &[
            ("testPytestPrice", "pytest"),
            ("CartChecks.testUnittestPrice", "unittest"),
            ("testSibling", "pytest"),
        ],
    )?;
    let owners = extract_owners(
        Path::new("src/pricing.py"),
        "def price(amount):\n    return amount * 2\n",
    );
    let owner = owners
        .iter()
        .find(|owner| owner.qualified_name == "price")
        .ok_or_else(|| "fixture must produce the price owner".to_string())?;
    let related = find_related_tests(owner, &tests, None);
    let mut names = related
        .iter()
        .map(|test| test.name.as_str())
        .collect::<Vec<_>>();
    names.sort();
    if names != ["testPytestPrice", "testUnittestPrice"] {
        return Err(format!(
            "only direct price observers should relate: {related:?}"
        ));
    }
    if related.iter().any(|test| test.oracle.is_none()) {
        return Err(format!(
            "related tests must preserve their oracles: {related:?}"
        ));
    }
    for (name, expected) in [
        (
            "testPytestPrice",
            "python -m pytest tests/test_collection.py::testPytestPrice",
        ),
        (
            "testUnittestPrice",
            "python -m unittest tests.test_collection.CartChecks.testUnittestPrice",
        ),
    ] {
        let test = tests
            .iter()
            .find(|test| test.name == name)
            .ok_or_else(|| format!("missing collected test {name}"))?;
        let command = verify_command_for_test(test);
        if command.as_deref() != Some(expected) {
            return Err(format!("expected {expected}, got {command:?}"));
        }
    }
    Ok(())
}
