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
fn isolated_asyncio_testcase_uses_unittest_collection_and_selector() -> Result<(), String> {
    let tests = collect(
        "import unittest\n\nclass AsyncChecks(unittest.IsolatedAsyncioTestCase):\n    async def test_value(self):\n        self.assertEqual(2 + 2, 4)\n\nclass SyncChecks(unittest.TestCase):\n    def test_value(self):\n        self.assertEqual(2 + 2, 4)\n",
        &[
            ("AsyncChecks.test_value", "unittest"),
            ("SyncChecks.test_value", "unittest"),
        ],
    )?;
    let async_test = tests
        .iter()
        .find(|test| test.qualified_name == "AsyncChecks.test_value")
        .ok_or_else(|| "missing async unittest".to_string())?;
    assert_eq!(
        verify_command_for_test(async_test).as_deref(),
        Some("python -m unittest tests.test_collection.AsyncChecks.test_value")
    );
    Ok(())
}

#[test]
fn isolated_asyncio_base_requires_a_stable_stdlib_import() -> Result<(), String> {
    collect(
        "from unittest import IsolatedAsyncioTestCase as AsyncCase\n\nclass TestAsync(AsyncCase):\n    async def test_value(self):\n        self.assertEqual(2 + 2, 4)\n",
        &[("TestAsync.test_value", "unittest")],
    )?;
    collect(
        "import unittest\nclass Mixin:\n    async def test_shared(self):\n        self.assertEqual(2 + 2, 4)\nclass AsyncChecks(unittest.IsolatedAsyncioTestCase, Mixin):\n    pass\n",
        &[("AsyncChecks.test_shared", "unittest")],
    )?;
    for source in [
        "import fake as unittest\nclass AsyncChecks(unittest.IsolatedAsyncioTestCase):\n    async def test_value(self):\n        assert True\n",
        "import unittest\nunittest = fake\nclass AsyncChecks(unittest.IsolatedAsyncioTestCase):\n    async def test_value(self):\n        assert True\n",
        "import unittest\nunittest.IsolatedAsyncioTestCase = fake\nclass AsyncChecks(unittest.IsolatedAsyncioTestCase):\n    async def test_value(self):\n        assert True\n",
        "class IsolatedAsyncioTestCase: pass\nclass AsyncChecks(IsolatedAsyncioTestCase):\n    async def test_value(self):\n        assert True\n",
    ] {
        let facts = extract_source_facts(Path::new("tests/test_collection.py"), source);
        assert!(
            facts.tests.is_empty(),
            "unexpected collection: {:?}",
            facts.tests
        );
    }
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
