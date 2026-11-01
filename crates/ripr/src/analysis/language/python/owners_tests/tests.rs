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
            "def defines_only(n):\n    def inner2(m):\n        return m\n    return n\n\n",
            "def recursion_only(n):\n    def inner3(m):\n        return inner3(m - 1) if m else m\n    return n\n\n",
            "def returns_callable(n):\n    def inner4(m):\n        return m\n    return inner4\n",
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
        !flag("calls")?,
        "a top-level owner is never a nested-called owner"
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
