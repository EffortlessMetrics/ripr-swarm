//! Row-local typed misses for Python related tests (#5491, RIPR-SPEC-0224).
//!
//! Every positive control has a discriminating negative: the same row shape
//! outside a concrete gap, or with the row's own fact unresolved, stays
//! `miss: None`. `miss` is explanation only, so each case also pins that the
//! class is the one the classifier chose without it.

use super::classify::classify_change_with_old;
use super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, Finding, RelatedTestMiss};
use std::path::Path;

fn classify(
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

fn misses(finding: &Finding) -> Vec<(String, Option<RelatedTestMiss>)> {
    finding
        .related_tests
        .iter()
        .map(|row| (row.name.clone(), row.miss))
        .collect()
}

fn miss_of(finding: &Finding, name: &str) -> Option<RelatedTestMiss> {
    let rows: Vec<_> = finding
        .related_tests
        .iter()
        .filter(|row| row.name == name)
        .collect();
    assert_eq!(rows.len(), 1, "one row named {name}: {:?}", misses(finding));
    rows[0].miss
}

const SCORE: &str = "def score(value):\n    return 8\n";
const SCORE_IMPORT: &str = "from src.subject import score\n";

fn score_case(tests: &str) -> Result<Finding, String> {
    classify(
        SCORE,
        &format!("{SCORE_IMPORT}{tests}"),
        2,
        "    return 7",
        "    return 8",
    )
}

#[test]
fn weak_family_selected_assertion_is_weak_assertion() -> Result<(), String> {
    let finding = score_case("def test_score():\n    assert score(0)\n")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_score"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

#[test]
fn exposed_finding_keeps_every_row_miss_none_even_beside_a_weak_row() -> Result<(), String> {
    let finding = score_case(
        "def test_exact():\n    assert score(0) == 8\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.related_tests.len(), 2, "{:?}", misses(&finding));
    assert!(
        finding.related_tests.iter().all(|row| row.miss.is_none()),
        "{:?}",
        misses(&finding)
    );
    Ok(())
}

#[test]
fn name_or_location_only_rows_never_get_no_call_path() -> Result<(), String> {
    // Referenced but never called: related by file stem only. The test body
    // cannot prove it never reaches the owner (callbacks, aliases, parametrize
    // and fixture `params`, `usefixtures`, `pytestmark`, conftest, plugins), so
    // the claim is withheld (#6657-class blind spots).
    let finding = score_case("def test_score_is_importable():\n    assert callable(score)\n")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].miss, None);
    Ok(())
}

#[test]
fn mixed_rows_keep_their_own_reasons() -> Result<(), String> {
    let finding = score_case(
        "def test_score_is_importable():\n    assert callable(score)\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(miss_of(&finding, "test_score_is_importable"), None);
    assert_eq!(
        miss_of(&finding, "test_weak"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

fn no_assertion_rows(finding: &Finding) -> Vec<String> {
    finding
        .related_tests
        .iter()
        .filter(|row| row.miss == Some(RelatedTestMiss::NoAssertion))
        .map(|row| row.name.clone())
        .collect()
}

#[test]
fn call_bearing_test_with_no_assertion_is_no_assertion() -> Result<(), String> {
    for tests in [
        "def test_score():\n    score(0)\n",
        "import pytest\n\n@pytest.mark.parametrize('value', [0, 1])\ndef test_score(value):\n    score(value)\n",
        "def test_score(tmp_path, monkeypatch):\n    path = tmp_path / 'out'\n    path.write_text(str(score(0)))\n",
    ] {
        let finding = score_case(tests)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{tests}");
        assert_eq!(finding.related_tests.len(), 1, "{tests}");
        let row = &finding.related_tests[0];
        assert_eq!(row.miss, Some(RelatedTestMiss::NoAssertion), "{tests}");
        assert_eq!(row.miss_support, None, "{tests}");
        // Parameters are activation evidence, never an oracle.
        assert_eq!(row.oracle, None, "{tests}");
    }
    Ok(())
}

/// Each pair is one withheld shape and its minimal direct control: the same
/// module with only the assertion-capable construct removed, which does emit
/// `no_assertion`. The withholding is therefore the construct's own.
const SHAPE_PAIRS: &[(&str, &str, &str)] = &[
    (
        "same-module helper",
        "def run_case(v):\n    assert v == 8\n\ndef test_shape():\n    run_case(score(0))\n",
        "def run_case(v):\n    assert v == 8\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "assert-named helper",
        "def test_shape():\n    assert_payload(score(0))\n",
        "def test_shape():\n    score(0)\n",
    ),
    (
        "unittest assertEqual",
        "import unittest\n\nclass T(unittest.TestCase):\n    def test_shape(self):\n        self.assertEqual(score(0), 8)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "unittest assertRaises",
        "import unittest\n\nclass T(unittest.TestCase):\n    def test_shape(self):\n        with self.assertRaises(ValueError):\n            score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def test_shape(self):\n        with self.subTest():\n            score(0)\n",
    ),
    (
        "self helper",
        "class TestX:\n    def _run(self, v):\n        assert v == 8\n    def test_shape(self):\n        self._run(score(0))\n",
        "class TestX:\n    def _run(self, v):\n        assert v == 8\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "pytest.raises",
        "import pytest\n\ndef test_shape():\n    with pytest.raises(ValueError):\n        score(0)\n",
        "import pytest\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "pytest.warns",
        "import pytest\n\ndef test_shape():\n    with pytest.warns(UserWarning):\n        score(0)\n",
        "import pytest\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "pytest.fail",
        "import pytest\n\ndef test_shape():\n    score(0)\n    pytest.fail('x')\n",
        "import pytest\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "raise AssertionError",
        "def test_shape():\n    if score(0) != 8:\n        raise AssertionError('bad')\n",
        "def test_shape():\n    if score(0) != 8:\n        pass\n",
    ),
    (
        "mock assertion",
        "from unittest import mock\n\ndef test_shape():\n    m = mock.Mock()\n    score(0)\n    m.assert_called_once()\n",
        "from unittest import mock\n\ndef test_shape():\n    m = mock.Mock()\n    score(0)\n    m.reset_mock()\n",
    ),
    (
        "nested def assert",
        "def test_shape():\n    def inner():\n        assert score(0) == 8\n    inner()\n",
        "def test_shape():\n    def inner():\n        return score(0)\n    inner()\n",
    ),
    (
        "lambda",
        "def test_shape():\n    f = lambda: verify(score(0))\n    f()\n",
        "def test_shape():\n    f = lambda: score(0)\n    f()\n",
    ),
    (
        "comprehension helper",
        "def test_shape():\n    [verify(score(v)) for v in (0, 1)]\n",
        "def test_shape():\n    [score(v) for v in (0, 1)]\n",
    ),
    (
        "project context manager",
        "class Ctx:\n    def __enter__(self):\n        return self\n    def __exit__(self, *a):\n        assert False\n\ndef test_shape():\n    with Ctx():\n        score(0)\n",
        "class Ctx:\n    def __enter__(self):\n        return self\n    def __exit__(self, *a):\n        assert False\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "same-module autouse fixture",
        "import pytest\n\n@pytest.fixture(autouse=True)\ndef guard():\n    yield\n    assert LEAKS == []\n\ndef test_shape():\n    score(0)\n",
        "import pytest\n\n@pytest.fixture\ndef guard():\n    yield\n    assert LEAKS == []\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "usefixtures",
        "import pytest\n\n@pytest.mark.usefixtures('db')\ndef test_shape():\n    score(0)\n",
        "import pytest\n\n@pytest.mark.slow\ndef test_shape():\n    score(0)\n",
    ),
    (
        "pytestmark usefixtures",
        "import pytest\n\npytestmark = pytest.mark.usefixtures('db')\n\ndef test_shape():\n    score(0)\n",
        "import pytest\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "opaque fixture parameter",
        "def test_shape(checked_client):\n    score(0)\n",
        "def test_shape(tmp_path):\n    score(0)\n",
    ),
    (
        "asserting teardown fixture",
        "import pytest\n\n@pytest.fixture\ndef res():\n    yield 1\n    assert False\n\ndef test_shape(res):\n    score(0)\n",
        "import pytest\n\n@pytest.fixture\ndef res():\n    yield 1\n    assert False\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "setUp that asserts",
        "import unittest\n\nclass T(unittest.TestCase):\n    def setUp(self):\n        self.assertTrue(True)\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def setUp(self):\n        self.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "pytest_plugins",
        "pytest_plugins = ['plugin_a']\n\ndef test_shape():\n    score(0)\n",
        "PLUGINS = ['plugin_a']\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "subprocess exit code",
        "import subprocess\n\ndef test_shape():\n    score(0)\n    subprocess.run(['prog'], check=True)\n",
        "import shlex\n\ndef test_shape():\n    score(0)\n    shlex.split('prog')\n",
    ),
    (
        "sys.exit",
        "import sys\n\ndef test_shape():\n    score(0)\n    sys.exit(1)\n",
        "import sys\n\ndef test_shape():\n    score(0)\n    sys.getsizeof(1)\n",
    ),
    (
        "nose eq_",
        "from nose.tools import eq_\n\ndef test_shape():\n    eq_(score(0), 8)\n",
        "from decimal import Decimal\n\ndef test_shape():\n    Decimal(score(0))\n",
    ),
    (
        "pytest_check",
        "from pytest_check import check\n\ndef test_shape():\n    check.equal(score(0), 8)\n",
        "from decimal import Decimal\n\ndef test_shape():\n    Decimal(score(0))\n",
    ),
    (
        "imported decorator",
        "from responses import activate\n\n@activate\ndef test_shape():\n    score(0)\n",
        "from functools import wraps\n\n@wraps(print)\ndef test_shape():\n    score(0)\n",
    ),
    (
        "filterwarnings mark",
        "import pytest\n\n@pytest.mark.filterwarnings('error')\ndef test_shape():\n    score(0)\n",
        "import pytest\n\n@pytest.mark.slow\ndef test_shape():\n    score(0)\n",
    ),
    (
        "test-support helper",
        "from tests.helpers import run_case\n\ndef test_shape():\n    run_case(score(0))\n",
        "from decimal import Decimal\n\ndef test_shape():\n    Decimal(score(0))\n",
    ),
    (
        "dynamic callee",
        "CHECKS = {}\n\ndef test_shape():\n    CHECKS['x'](score(0))\n",
        "def test_shape():\n    score(0)\n",
    ),
    // Module-level effects in the test module (B1).
    (
        "module warnings filter",
        "import warnings\n\nwarnings.filterwarnings('error')\n\ndef test_shape():\n    score(0)\n",
        "import warnings\n\nwarnings.catch_warnings\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "module-level assert",
        "assert True\n\ndef test_shape():\n    score(0)\n",
        "FLAG = True\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "module-level raise under a guard",
        "import sys\n\nif sys.platform == 'none':\n    raise RuntimeError('x')\n\ndef test_shape():\n    score(0)\n",
        "import sys\n\nif sys.platform == 'none':\n    FLAG = 1\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "module-level pytest.skip",
        "import pytest\n\npytest.skip('all', allow_module_level=True)\n\ndef test_shape():\n    score(0)\n",
        "import pytest\n\nMARK = pytest.mark.slow\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "module-level importorskip",
        "import pytest\n\nnp = pytest.importorskip('numpy')\n\ndef test_shape():\n    score(0)\n",
        "import pytest\n\nnp = None\n\ndef test_shape():\n    score(0)\n",
    ),
    (
        "star import",
        "from src.subject import *\n\ndef test_shape():\n    score(0)\n",
        "from src.subject import score\n\ndef test_shape():\n    score(0)\n",
    ),
    // Context-manager and decorator forms of assertion libraries (B3).
    (
        "responses context manager",
        "import responses\n\ndef test_shape():\n    with responses.RequestsMock() as rsps:\n        score(0)\n",
        "import tempfile\n\ndef test_shape():\n    with tempfile.TemporaryDirectory() as tmp:\n        score(0)\n",
    ),
    (
        "respx mock decorator",
        "import respx\n\n@respx.mock\ndef test_shape():\n    score(0)\n",
        "import functools\n\n@functools.cache\ndef test_shape():\n    score(0)\n",
    ),
    (
        "project guard context manager",
        "from src.guards import no_leaks\n\ndef test_shape():\n    with no_leaks():\n        score(0)\n",
        "from contextlib import suppress\n\ndef test_shape():\n    with suppress(KeyError):\n        score(0)\n",
    ),
    // A mark on one case (B4).
    (
        "pytest.param with marks",
        "import pytest\n\n@pytest.mark.parametrize('v', [pytest.param(0, marks=pytest.mark.xfail(strict=True))])\ndef test_shape(v):\n    score(v)\n",
        "import pytest\n\n@pytest.mark.parametrize('v', [pytest.param(0, id='zero')])\ndef test_shape(v):\n    score(v)\n",
    ),
    // Dynamic-import and doctest escapes (B5); `request` shapes are pinned in
    // the admission table because a `request` fixture is a static limit here.
    (
        "doctest run",
        "import doctest\n\ndef test_shape():\n    score(0)\n    doctest.run_docstring_examples(score, {})\n",
        "def test_shape():\n    score(0)\n",
    ),
    (
        "import_module bound to a local",
        "import importlib\n\ndef test_shape():\n    load = importlib.import_module\n    load('plugin_a')\n    score(0)\n",
        "def test_shape():\n    load = len\n    load('plugin_a')\n    score(0)\n",
    ),
    (
        "unittest __init__ registers an asserting cleanup",
        "import unittest\n\nclass T(unittest.TestCase):\n    def __init__(self, *a):\n        self.addCleanup(self.assertTrue, False)\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def __init__(self, *a):\n        self.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "unittest doCleanups override",
        "import unittest\n\nclass T(unittest.TestCase):\n    def doCleanups(self):\n        self.assertTrue(False)\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def doCleanups(self):\n        self.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "unittest __init_subclass__",
        "import unittest\n\nclass T(unittest.TestCase):\n    def __init_subclass__(cls, **kw):\n        cls.check_all()\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def __init_subclass__(cls, **kw):\n        cls.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "unittest debug override",
        "import unittest\n\nclass T(unittest.TestCase):\n    def debug(self):\n        self.assertTrue(False)\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def debug(self):\n        self.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "unittest __call__ override",
        "import unittest\n\nclass T(unittest.TestCase):\n    def __call__(self, *a):\n        self.assertTrue(False)\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def __call__(self, *a):\n        self.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "unittest _callSetUp override",
        "import unittest\n\nclass T(unittest.TestCase):\n    def _callSetUp(self):\n        self.assertTrue(False)\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def _callSetUp(self):\n        self.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "unittest _callTearDown override",
        "import unittest\n\nclass T(unittest.TestCase):\n    def _callTearDown(self):\n        self.assertTrue(False)\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def _callTearDown(self):\n        self.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "unittest run override",
        "import unittest\n\nclass T(unittest.TestCase):\n    def run(self, result=None):\n        self.assertTrue(False)\n    def test_shape(self):\n        score(0)\n",
        "import unittest\n\nclass T(unittest.TestCase):\n    def run(self, result=None):\n        self.x = 1\n    def test_shape(self):\n        score(0)\n",
    ),
    (
        "testdir member",
        "def test_shape(testdir):\n    score(0)\n    testdir.runpytest()\n",
        "def test_shape(testdir):\n    score(0)\n",
    ),
    (
        "recwarn.pop",
        "def test_shape(recwarn):\n    score(0)\n    recwarn.pop(UserWarning)\n",
        "def test_shape(recwarn):\n    score(0)\n",
    ),
];

#[test]
fn assertion_like_shapes_never_get_no_assertion_beside_a_minimal_control() -> Result<(), String> {
    for (label, shape, control) in SHAPE_PAIRS {
        let controlled = score_case(control)?;
        assert_eq!(
            miss_of(&controlled, "test_shape"),
            Some(RelatedTestMiss::NoAssertion),
            "{label} control: {:?}",
            misses(&controlled)
        );
        let finding = score_case(shape)?;
        assert!(
            finding
                .related_tests
                .iter()
                .any(|row| row.name == "test_shape"),
            "{label}: the shape must be a related row: {:?}",
            misses(&finding)
        );
        assert!(
            no_assertion_rows(&finding).is_empty(),
            "{label}: {:?}",
            misses(&finding)
        );
    }
    Ok(())
}

#[test]
fn no_assertion_is_per_row_beside_a_weak_sibling_and_never_in_an_exposed_finding()
-> Result<(), String> {
    let finding =
        score_case("def test_bare():\n    score(0)\n\ndef test_weak():\n    assert score(1)\n")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_bare"),
        Some(RelatedTestMiss::NoAssertion)
    );
    assert_eq!(
        miss_of(&finding, "test_weak"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    let exposed = score_case(
        "def test_bare():\n    score(0)\n\ndef test_exact():\n    assert score(1) == 8\n",
    )?;
    assert_eq!(exposed.class, ExposureClass::Exposed);
    assert!(
        exposed.related_tests.iter().all(|row| row.miss.is_none()),
        "{:?}",
        misses(&exposed)
    );
    Ok(())
}

#[test]
fn name_only_and_skipped_rows_never_get_no_assertion() -> Result<(), String> {
    for tests in [
        "def test_score_is_importable():\n    callable(score)\n",
        "import pytest\n\n@pytest.mark.skip(reason='later')\ndef test_score():\n    score(0)\n",
        "import pytest\n\n@pytest.mark.xfail\ndef test_score():\n    score(0)\n",
    ] {
        let finding = score_case(tests)?;
        assert!(
            no_assertion_rows(&finding).is_empty(),
            "{tests}: {:?}",
            misses(&finding)
        );
    }
    Ok(())
}

mod workspace {
    use super::*;
    use crate::analysis::language::python::fixtures::{
        record_bare_sibling_imports, taint_admission_by_conftest,
        taint_admission_by_conftest_declarations,
    };
    use crate::analysis::language::python::source_facts::extract_source_facts;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    const TEST_FILE: &str = "tests/test_subject.py";

    /// The row misses of the tests in `tests/test_subject.py`, after the
    /// workspace passes both the repo and diff analyses run.
    fn row_misses(
        conftests: &[(&str, &str)],
        unread: &[&str],
        siblings: &[&str],
        tests: &str,
    ) -> Result<Vec<(String, Option<RelatedTestMiss>)>, String> {
        row_misses_at(TEST_FILE, SCORE_IMPORT, conftests, unread, siblings, tests)
    }

    fn row_misses_at(
        test_file: &str,
        import: &str,
        conftests: &[(&str, &str)],
        unread: &[&str],
        siblings: &[&str],
        tests: &str,
    ) -> Result<Vec<(String, Option<RelatedTestMiss>)>, String> {
        let file = Path::new("src/subject.py");
        let owners = extract_owners(file, SCORE);
        let mut parsed = extract_tests(Path::new(test_file), &format!("{import}{tests}"));
        let mut implicit = BTreeSet::new();
        let mut reach = BTreeSet::new();
        for (path, source) in conftests {
            let facts = extract_source_facts(Path::new(path), source);
            if facts.conftest_implicit_setup {
                implicit.insert(PathBuf::from(path));
            }
            if facts.conftest_declares_fixture_or_hook {
                reach.insert(PathBuf::from(path));
            }
        }
        let is_unread = |conftest: &Path| unread.iter().any(|path| Path::new(path) == conftest);
        taint_admission_by_conftest(&mut parsed, &implicit, is_unread);
        taint_admission_by_conftest_declarations(&mut parsed, &reach, is_unread);
        record_bare_sibling_imports(&mut parsed, |path| {
            siblings.iter().any(|sibling| Path::new(sibling) == path)
        });
        let finding = classify_change_with_old(
            file,
            2,
            "    return 8",
            Some("    return 7"),
            &owners,
            &parsed,
        )
        .ok_or("behavioral fixture must produce a finding")?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        Ok(misses(&finding))
    }

    const BARE: &str = "def test_bare():\n    score(0)\n";
    const AUTOUSE: &str = "import pytest\n\n@pytest.fixture(autouse=True)\ndef no_leaks():\n    yield\n    assert LEAKS == []\n";

    fn bare_miss(
        conftests: &[(&str, &str)],
        unread: &[&str],
        siblings: &[&str],
        tests: &str,
    ) -> Result<Option<RelatedTestMiss>, String> {
        let rows = row_misses(conftests, unread, siblings, tests)?;
        rows.into_iter()
            .find(|(name, _)| name == "test_bare")
            .map(|(_, miss)| miss)
            .ok_or_else(|| "test_bare row".to_string())
    }

    #[test]
    fn a_conftest_that_can_assert_around_the_test_withholds_no_assertion() -> Result<(), String> {
        // Direct control: the same test with an inert conftest keeps the reason.
        assert_eq!(
            bare_miss(&[("tests/conftest.py", "import pytest\n")], &[], &[], BARE)?,
            Some(RelatedTestMiss::NoAssertion)
        );
        for (label, conftest) in [
            ("autouse", AUTOUSE),
            (
                "hook",
                "def pytest_runtest_teardown(item):\n    assert item\n",
            ),
            ("plugins", "pytest_plugins = ['plugin_a']\n"),
            ("reexport", "from fixtures_pkg import *\n"),
        ] {
            assert_eq!(
                bare_miss(&[("tests/conftest.py", conftest)], &[], &[], BARE)?,
                None,
                "{label}"
            );
        }
        // An ancestor conftest reaches nested tests; an unread one is unknown.
        assert_eq!(
            bare_miss(&[("conftest.py", AUTOUSE)], &[], &[], BARE)?,
            None
        );
        assert_eq!(bare_miss(&[], &["tests/conftest.py"], &[], BARE)?, None);
        Ok(())
    }

    #[test]
    fn a_conftest_declaration_withholds_no_assertion_even_for_a_test_naming_none_of_it()
    -> Result<(), String> {
        // Control: a conftest that declares nothing keeps the reason.
        assert_eq!(
            bare_miss(
                &[("tests/conftest.py", "import pytest\nMARK = 1\n")],
                &[],
                &[],
                BARE
            )?,
            Some(RelatedTestMiss::NoAssertion)
        );
        for (label, conftest) in [
            (
                "unrelated plain fixture",
                "import pytest\n\n@pytest.fixture\ndef other():\n    return 1\n",
            ),
            (
                "built-in override",
                "import pytest\n\n@pytest.fixture\ndef tmp_path():\n    yield 1\n    assert False\n",
            ),
            (
                "renamed built-in override",
                "import pytest\n\n@pytest.fixture(name='tmp_path')\ndef fake():\n    yield 1\n    assert False\n",
            ),
            (
                "configure hook",
                "def pytest_configure(config):\n    config.addinivalue_line('filterwarnings', 'error')\n",
            ),
            (
                "generate_tests hook",
                "def pytest_generate_tests(metafunc):\n    metafunc.fixturenames.append('guard')\n",
            ),
            (
                "session finish hook",
                "def pytest_sessionfinish(session):\n    assert not session.testsfailed\n",
            ),
            (
                "top-level filter",
                "import warnings\n\nwarnings.simplefilter('error')\n",
            ),
        ] {
            assert_eq!(
                bare_miss(&[("tests/conftest.py", conftest)], &[], &[], BARE)?,
                None,
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_bare_sibling_module_other_than_the_module_under_test_withholds_no_assertion()
    -> Result<(), String> {
        let helper = "from runner import run_case\n\ndef test_bare():\n    run_case(score(0))\n";
        // Control: the same import with no such module beside the test.
        assert_eq!(
            bare_miss(&[], &[], &[], helper)?,
            Some(RelatedTestMiss::NoAssertion)
        );
        assert_eq!(bare_miss(&[], &[], &["tests/runner.py"], helper)?, None);
        assert_eq!(bare_miss(&[], &[], &["tests/runner"], helper)?, None);
        let plain_import = "import runner\n\ndef test_bare():\n    runner.go(score(0))\n";
        assert_eq!(
            bare_miss(&[], &[], &["tests/runner.py"], plain_import)?,
            None
        );
        // A package test directory makes the parent the pytest rootdir, so a
        // root-level module is bare-importable too (B6); the module under test
        // and its package are exempt.
        let root_helper =
            "from runner import run_case\n\ndef test_bare():\n    run_case(score(0))\n";
        assert_eq!(
            bare_miss(&[], &[], &["tests/__init__.py", "runner.py"], root_helper)?,
            None
        );
        assert_eq!(
            bare_miss(&[], &[], &["runner.py"], root_helper)?,
            None,
            "the repository root is bare-importable (conftest, python -m pytest)"
        );

        let production_package =
            "from src.subject import score as s\n\ndef test_bare():\n    s(0)\n";
        let rows = row_misses_at(
            "tests/test_subject.py",
            "",
            &[],
            &[],
            &["tests/__init__.py", "src"],
            production_package,
        )?;
        assert_eq!(
            rows.iter().map(|(_, miss)| *miss).collect::<Vec<_>>(),
            vec![Some(RelatedTestMiss::NoAssertion)],
            "the owner's package is exempt: {rows:?}"
        );
        // Package containment is component-wise: package `sr` does not
        // contain `src/subject.py`, so it still withholds.
        let rows = row_misses_at(
            "tests/test_subject.py",
            "",
            &[],
            &[],
            &["tests/__init__.py", "sr"],
            "from sr import score as s\n\ndef test_bare():\n    s(0)\n",
        )?;
        assert_eq!(
            rows.iter().map(|(_, miss)| *miss).collect::<Vec<_>>(),
            vec![None],
            "{rows:?}"
        );
        // The module under test beside its test is not test support.
        let rows = row_misses_at(
            "src/test_subject.py",
            "from subject import score\n",
            &[],
            &[],
            &["src/subject.py"],
            BARE,
        )?;
        assert_eq!(
            rows.iter().map(|(_, miss)| *miss).collect::<Vec<_>>(),
            vec![Some(RelatedTestMiss::NoAssertion)],
            "{rows:?}"
        );
        Ok(())
    }
}

#[test]
fn unsupported_custom_assertion_form_gets_no_fabricated_miss() -> Result<(), String> {
    let finding = score_case(
        "def check(result):\n    assert result == 8\n\ndef test_score():\n    check(score(0))\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_score"),
        None,
        "{:?}",
        misses(&finding)
    );
    Ok(())
}

#[test]
fn skipped_and_xfail_rows_gain_no_miss() -> Result<(), String> {
    for decorator in ["pytest.mark.skip(reason='off')", "pytest.mark.xfail"] {
        let finding = score_case(&format!(
            "import pytest\n\n@{decorator}\ndef test_score():\n    assert score(0) == 8\n"
        ))?;
        // #5389: the class is the classifier's, and no row is credited a miss.
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{decorator}");
        assert!(
            finding.related_tests.iter().all(|row| row.miss.is_none()),
            "{decorator}: {:?}",
            misses(&finding)
        );
    }
    Ok(())
}

#[test]
fn strong_assertion_that_this_row_does_not_credit_is_observation_unconfirmed() -> Result<(), String>
{
    let finding = score_case(
        "from src.subject import other\n\ndef test_score():\n    score(0)\n    assert other(1) == 2\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // Never an affirmative mismatch: Python alignment is credit-only.
    assert_eq!(
        miss_of(&finding, "test_score"),
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

#[test]
fn several_strong_rows_are_aligned_independently() -> Result<(), String> {
    // Two strong rows asserting on another function, plus a weak row: each row
    // is aligned on its own from the shared prepared state.
    let finding = score_case(
        "from src.subject import other\n\ndef test_b():\n    score(0)\n    assert other(1) == 2\n\ndef test_c():\n    score(0)\n    assert other(2) == 3\n\ndef test_d():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_b"),
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    assert_eq!(
        miss_of(&finding, "test_c"),
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    assert_eq!(
        miss_of(&finding, "test_d"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

#[test]
fn hidden_call_shapes_never_get_no_call_path() -> Result<(), String> {
    for body in [
        "def test_score_map():\n    assert list(map(score, [0, 1])) == [8, 8]\n",
        "def test_score_alias():\n    f = score\n    assert [f(0)] == [8]\n",
        "from src.subject import score as s\n\ndef test_alias_map():\n    assert list(map(s, [0, 1])) == [8, 8]\n",
        "import pytest\n\n@pytest.mark.parametrize('cb', [score])\ndef test_score(cb):\n    assert callable(score)\n    assert cb(0) == 8\n",
        "import pytest\n\n@pytest.fixture(params=[score])\ndef cb(request):\n    return request.param\n\ndef test_score_param(cb):\n    assert callable(score)\n    assert cb(0) == 8\n",
        "import pytest\n\n@pytest.fixture\ndef setup():\n    return score(0)\n\n@pytest.mark.usefixtures('setup')\ndef test_score_setup():\n    assert callable(score)\n",
        "import pytest\n\npytestmark = pytest.mark.parametrize('cb', [score])\n\ndef test_score(cb):\n    assert callable(score)\n    assert cb(0) == 8\n",
        "import pytest\n\npytestmark = [pytest.mark.parametrize('cb', [score])]\n\ndef test_score(cb):\n    assert callable(score)\n    assert cb(0) == 8\n",
        "def test_nested_call():\n    class Box:\n        __call__ = score\n    assert Box()(0) == 8\n",
        "def test_kw():\n    assert run(\n        cb = score\n    ) == 8\n",
        "def test_unused_alias():\n    cb = score\n    assert 8 == 8\n",
    ] {
        let finding = score_case(body)?;
        assert!(!finding.related_tests.is_empty(), "{body}");
        assert!(
            finding
                .related_tests
                .iter()
                .all(|row| row.miss != Some(RelatedTestMiss::NoCallPath)),
            "{body}: {:?}",
            misses(&finding)
        );
    }
    Ok(())
}

const RESERVE: &str = "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n";

fn reserve_case(tests: &str) -> Result<Finding, String> {
    classify(
        RESERVE,
        &format!("from src.subject import can_reserve\n{tests}"),
        2,
        "    if qty > on_hand:",
        "    if qty >= on_hand:",
    )
}

#[test]
fn off_boundary_strong_row_is_missing_input_and_a_weak_sibling_is_not() -> Result<(), String> {
    let finding = reserve_case(
        "def test_off_boundary():\n    assert can_reserve(5, 3) == True\n\ndef test_weak_on_boundary():\n    assert can_reserve(4, 4)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_off_boundary"),
        Some(RelatedTestMiss::MissingInput)
    );
    // The weak sibling sits on the boundary but earns only its own reason; it
    // neither lends its input to the strong row nor takes missing_input.
    assert_eq!(
        miss_of(&finding, "test_weak_on_boundary"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

/// #5564: only the row whose own omission proof holds carries the boundary as
/// `miss_support`; a weak or unresolved sibling carries none, and the
/// finding-wide fact is not copied onto rows that did not establish it.
#[test]
fn missing_input_support_belongs_to_the_row_that_proved_the_omission() -> Result<(), String> {
    let finding = reserve_case(
        "def test_off_boundary():\n    assert can_reserve(5, 3) == True\n\ndef test_weak_on_boundary():\n    assert can_reserve(4, 4)\n\ndef test_unresolved(n):\n    assert can_reserve(n, 3) == True\n",
    )?;
    let support_of = |name: &str| {
        finding
            .related_tests
            .iter()
            .find(|row| row.name == name)
            .map(|row| row.miss_support.clone())
    };
    let fact = finding
        .activation
        .missing_discriminators
        .iter()
        .find(|fact| fact.value.contains(" == "))
        .ok_or("the boundary finding carries a boundary fact")?;
    assert_eq!(fact.value, "qty == on_hand");
    let owner = support_of("test_off_boundary")
        .flatten()
        .ok_or("the omitting row carries support")?;
    assert_eq!(owner.value, fact.value);
    assert_eq!(owner.flow_sink, fact.flow_sink);
    // Discriminating negatives: the weak sibling and the unresolved row keep
    // their own (or no) miss and no borrowed support.
    assert_eq!(support_of("test_weak_on_boundary"), Some(None));
    assert_eq!(support_of("test_unresolved"), Some(None));
    for row in &finding.related_tests {
        if row.miss != Some(RelatedTestMiss::MissingInput) {
            assert_eq!(row.miss_support, None, "{}", row.name);
        }
    }
    Ok(())
}

#[test]
fn unresolved_row_inside_a_boundary_gap_does_not_borrow_a_sibling_fact() -> Result<(), String> {
    let finding = reserve_case(
        "def test_off_boundary():\n    assert can_reserve(5, 3) == True\n\ndef test_unresolved(n):\n    assert can_reserve(n, 3) == True\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_off_boundary"),
        Some(RelatedTestMiss::MissingInput)
    );
    assert_eq!(
        miss_of(&finding, "test_unresolved"),
        None,
        "{:?}",
        misses(&finding)
    );
    Ok(())
}

#[test]
fn boundary_input_row_makes_the_finding_exposed_with_no_row_miss() -> Result<(), String> {
    let finding = reserve_case(
        "def test_off_boundary():\n    assert can_reserve(5, 3) == True\n\ndef test_on_boundary():\n    assert can_reserve(4, 4) == True\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert!(
        finding.related_tests.iter().all(|row| row.miss.is_none()),
        "{:?}",
        misses(&finding)
    );
    Ok(())
}

/// Producer-level shadowing and receiver-identity cases (#5491 review): the
/// `subject` row beside a strong direct `control` row. The control keeps
/// `missing_input`; the subject must not claim it.
fn assert_subject_withheld(imports: &str, subject: &str, label: &str) -> Result<(), String> {
    let finding = reserve_case(&format!(
        "{imports}def test_control():\n    assert can_reserve(5, 3) == True\n\n{subject}"
    ))?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    assert_eq!(
        miss_of(&finding, "test_control"),
        Some(RelatedTestMiss::MissingInput),
        "{label}: control row {:?}",
        misses(&finding)
    );
    assert_eq!(
        miss_of(&finding, "test_subject"),
        None,
        "{label}: subject row {:?}",
        misses(&finding)
    );
    Ok(())
}

#[test]
fn identifier_ending_in_def_or_class_is_not_a_definition_header() -> Result<(), String> {
    let probe = "from src.helpers import boundary_probe\n\n";
    for (label, setup) in [
        ("undef", "x = undef"),
        ("subclass", "kind = subclass"),
        ("my_class", "y = my_class"),
        ("trailing _def", "z = some_def"),
        ("unicode prefix", "w = \u{e9}class"),
        ("def as an argument tail", "v = [1, undef]"),
    ] {
        assert_subject_withheld(
            probe,
            &format!(
                "def test_subject():\n    assert can_reserve(5, 3) == True\n    {setup}\n    boundary_probe([1])\n"
            ),
            label,
        )?;
    }
    Ok(())
}

#[test]
fn rebound_builtin_or_foreign_receiver_api_never_gets_missing_input() -> Result<(), String> {
    let body = "def test_subject():\n    assert can_reserve(5, 3) == True\n    ";
    for (label, imports, subject) in [
        (
            "imported alias shadows a builtin",
            "from src.helpers import boundary_probe as list\n",
            format!("{body}list([1])\n"),
        ),
        (
            "local rebind of a builtin",
            "from src.helpers import boundary_probe\n",
            format!("{body}list = boundary_probe\n    list([1])\n"),
        ),
        (
            "module def shadows a builtin",
            "from src.helpers import boundary_probe\n\ndef list(values):\n    return boundary_probe(values)\n\n",
            format!("{body}list([1])\n"),
        ),
        (
            "approx on a project receiver",
            "import src.helpers as receiver\n",
            format!("{body}receiver.approx([1])\n"),
        ),
        (
            "assertEqual on a project receiver",
            "import src.helpers as receiver\n",
            format!("{body}receiver.assertEqual([1])\n"),
        ),
        (
            "raises on a project receiver",
            "import src.helpers as receiver\n",
            format!("{body}receiver.raises([1])\n"),
        ),
        (
            "approx on a pytest name that is rebound",
            "import pytest\nimport src.helpers\n\npytest = src.helpers\n\n",
            format!("{body}pytest.approx([1])\n"),
        ),
    ] {
        assert_subject_withheld(imports, &subject, label)?;
    }
    Ok(())
}

#[test]
fn callable_passed_by_reference_to_a_trusted_call_never_gets_missing_input() -> Result<(), String> {
    let body = "def test_subject():\n    assert can_reserve(5, 3) == True\n    ";
    let probe = "from src.helpers import boundary_probe\n";
    for (label, imports, subject) in [
        (
            "pytest.raises with a callable argument",
            format!("import pytest\n{probe}"),
            format!("{body}with pytest.raises(ValueError):\n        pass\n    pytest.raises(ValueError, boundary_probe, [1])\n"),
        ),
        (
            "assertRaises with a callable argument",
            format!("import unittest\n{probe}"),
            "class T(unittest.TestCase):\n    def test_subject(self):\n        self.assertEqual(can_reserve(5, 3), True)\n        self.assertRaises(ValueError, boundary_probe, [1])\n".to_string(),
        ),
        (
            "sorted with key=callable",
            probe.to_string(),
            format!("{body}sorted([1], key=boundary_probe)\n"),
        ),
        (
            "max with key=callable",
            probe.to_string(),
            format!("{body}max([1], key=boundary_probe)\n"),
        ),
        (
            "key= an attribute of a project module",
            "import src.helpers as receiver\n".to_string(),
            format!("{body}sorted([1], key=receiver.boundary_probe)\n"),
        ),
        (
            "keyword label continuation line",
            probe.to_string(),
            format!("{body}dict(\n        boundary_probe = 1,\n    )\n    sorted([1], key=boundary_probe)\n"),
        ),
        (
            "keyword label in a multi-line call",
            probe.to_string(),
            format!("{body}foo(\n        x = 1,\n    )\n    sorted([1], key=x)\n"),
        ),
        (
            "keyword label in a nested multi-line call",
            probe.to_string(),
            format!("{body}dict(\n        a=[\n            1,\n        ],\n        boundary_probe = 2,\n    )\n    sorted([1], key=boundary_probe)\n"),
        ),
        (
            "default argument continuation line",
            probe.to_string(),
            format!("{body}def helper(\n        boundary_probe = 1,\n    ):\n        pass\n    sorted([1], key=boundary_probe)\n"),
        ),
        (
            "multi-line list holding a callable",
            probe.to_string(),
            format!("{body}fns = [\n        boundary_probe,\n    ]\n    sorted([1], key=fns[0])\n"),
        ),
        (
            "paren-wrapped callable",
            probe.to_string(),
            format!("{body}fn = (\n        boundary_probe\n    )\n    sorted([1], key=fn)\n"),
        ),
        (
            "backslash continuation",
            probe.to_string(),
            format!("{body}fn = \\\n        boundary_probe\n    sorted([1], key=fn)\n"),
        ),
        (
            "copy of a non-plain name",
            probe.to_string(),
            format!("{body}a = boundary_probe\n    b = a\n    sorted([1], key=b)\n"),
        ),
        (
            "augmented list store",
            probe.to_string(),
            format!("{body}fns = []\n    fns += [boundary_probe]\n    sorted([1], key=fns[0])\n"),
        ),
        (
            "subscript store into a list",
            probe.to_string(),
            format!("{body}fns = [None]\n    fns[0] = boundary_probe\n    sorted([1], key=fns[0])\n"),
        ),
        (
            "subscript store into a dict",
            probe.to_string(),
            format!("{body}fns = {{}}\n    fns['k'] = boundary_probe\n    sorted([1], key=fns['k'])\n"),
        ),
        (
            "star-unpacked multi-line list",
            probe.to_string(),
            format!("{body}fns = [\n        boundary_probe,\n    ]\n    sorted([1], *fns)\n"),
        ),
        (
            "list append",
            probe.to_string(),
            format!("{body}fns = []\n    fns.append(boundary_probe)\n    sorted([1], key=fns[0])\n"),
        ),
        (
            "list extend",
            probe.to_string(),
            format!("{body}fns = []\n    fns.extend([boundary_probe])\n    sorted([1], key=fns[0])\n"),
        ),
        (
            "dict update",
            probe.to_string(),
            format!("{body}d = {{}}\n    d.update({{'k': boundary_probe}})\n    sorted([1], key=d['k'])\n"),
        ),
        (
            "dict in-place union",
            probe.to_string(),
            format!("{body}d = {{}}\n    d |= {{'k': boundary_probe}}\n    sorted([1], key=d['k'])\n"),
        ),
        (
            "multi-target unpack",
            probe.to_string(),
            format!("{body}fn, other = boundary_probe, 1\n    sorted([1], key=fn)\n"),
        ),
        (
            "key= a local bound from an import",
            probe.to_string(),
            format!("{body}fn = boundary_probe\n    sorted([1], key=fn)\n"),
        ),
    ] {
        assert_subject_withheld(&imports, &subject, label)?;
    }
    // A module that imports `builtins` can rewrite any bare builtin, so the
    // whole file withholds the claim, control row included.
    let finding = reserve_case(&format!(
        "import builtins\n{probe}def test_control():\n    assert can_reserve(5, 3) == True\n\n{body}builtins.list = boundary_probe\n    list([1])\n"
    ))?;
    assert!(
        finding
            .related_tests
            .iter()
            .all(|row| row.miss != Some(RelatedTestMiss::MissingInput)),
        "{:?}",
        misses(&finding)
    );
    Ok(())
}

#[test]
fn genuine_framework_and_builtin_calls_keep_the_claim() -> Result<(), String> {
    for (label, imports, tests) in [
        (
            "plain builtins",
            "",
            "def test_a():\n    assert can_reserve(5, 3) == True\n    assert sum([1]) == 1\n    assert len(list([1])) == 1\n",
        ),
        (
            "plain local and a builtin key",
            "",
            "def test_a():\n    assert can_reserve(5, 3) == True\n    values = [3, 1, 2]\n    assert sorted(values) == [1, 2, 3]\n    assert max(values, key=len) == 1 or True\n",
        ),
        (
            "pytest.approx via the real import",
            "import pytest\n\n",
            "def test_a():\n    assert can_reserve(5, 3) == True\n    assert 0.5 == pytest.approx(0.5)\n",
        ),
        (
            "pytest.raises via the real import",
            "import pytest\n\n",
            "def test_a():\n    assert can_reserve(5, 3) == True\n    with pytest.raises(ValueError):\n        int('x')\n",
        ),
        (
            "self.assertEqual on a unittest.TestCase",
            "import unittest\n\n",
            "class T(unittest.TestCase):\n    def test_a(self):\n        self.assertEqual(can_reserve(5, 3), True)\n        self.assertTrue(sum([1]))\n",
        ),
    ] {
        let finding = reserve_case(&format!("{imports}{tests}"))?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
        assert_eq!(
            miss_of(&finding, "test_a"),
            Some(RelatedTestMiss::MissingInput),
            "{label}: {:?}",
            misses(&finding)
        );
    }
    Ok(())
}

#[test]
fn medium_strength_mock_row_keeps_no_miss_in_a_weak_finding() -> Result<(), String> {
    let finding = score_case(
        "def test_mock():\n    cb = Mock()\n    score(cb)\n    cb.assert_called_once_with(1)\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(miss_of(&finding, "test_mock"), None);
    // Discriminating control: the weak sibling still carries its own reason.
    assert_eq!(
        miss_of(&finding, "test_weak"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

#[test]
fn credited_strong_row_keeps_no_miss_beside_a_weak_row() -> Result<(), String> {
    let finding = score_case(
        "def test_exact():\n    assert score(0) == 8\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    // A credited strong row exposes the finding; no row then carries a miss,
    // including the weak sibling (the class gate, not the row, decides).
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(miss_of(&finding, "test_exact"), None);
    assert_eq!(miss_of(&finding, "test_weak"), None);
    Ok(())
}

#[test]
fn helper_owner_call_row_keeps_no_miss_inside_a_weak_finding() -> Result<(), String> {
    // The test reaches the helper through its same-module caller: a path
    // exists, and the row has no oracle, so no reason is established.
    let finding = classify(
        "def _validate_epoch(epoch):\n    if epoch >= 0:\n        return epoch\n    raise ValueError('bad')\n\n\nclass Version:\n    def __init__(self, text):\n        self.epoch = _validate_epoch(int(text))\n",
        "from src.subject import Version\n\n\ndef test_epoch_zero():\n    assert Version('0').epoch == 0\n",
        2,
        "    if epoch > 0:",
        "    if epoch >= 0:",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].miss, None);
    Ok(())
}

// Direct reads of the pass, so a gate is exercised independently of the class
// the classifier would have chosen.
mod direct {
    use super::super::assertion_selection::PythonAssertionFocus;
    use super::super::boundary::python_row_boundary_omission;
    use super::super::owners_tests::{extract_owners, extract_tests};
    use super::super::probe_shape::classify_probe_shape;
    use super::super::related_tests::{
        PythonRelationKind, related_test_candidates, related_tests_for_candidates,
    };
    use super::super::row_miss::{RowMissChange, annotate_row_misses};
    use super::super::{PythonOwner, PythonTest, owner_for_changed_line};
    use crate::domain::{ExposureClass, RelatedTestMiss};
    use std::path::Path;

    struct Case {
        owner: PythonOwner,
        tests: Vec<PythonTest>,
        before: String,
        after: String,
    }

    fn case(
        source: &str,
        tests: &str,
        line: usize,
        before: &str,
        after: &str,
    ) -> Result<Case, String> {
        case_in(
            "src/subject.py",
            "tests/test_subject.py",
            source,
            tests,
            line,
            before,
            after,
        )
    }

    fn case_in(
        owner_file: &str,
        test_file: &str,
        source: &str,
        tests: &str,
        line: usize,
        before: &str,
        after: &str,
    ) -> Result<Case, String> {
        let file = Path::new(owner_file);
        let owners = extract_owners(file, source);
        let owner = owner_for_changed_line(file, line, &owners)
            .cloned()
            .ok_or_else(|| "changed line must have an owner".to_string())?;
        Ok(Case {
            owner,
            tests: extract_tests(Path::new(test_file), tests),
            before: before.to_string(),
            after: after.to_string(),
        })
    }

    /// The relation of each candidate and the miss the pass assigns under
    /// `class` and `has_static_limit`.
    fn run(
        case: &Case,
        class: &ExposureClass,
        has_static_limit: bool,
    ) -> Vec<(PythonRelationKind, Option<RelatedTestMiss>)> {
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
        let candidates = related_test_candidates(&case.owner, &case.tests);
        let mut rows = related_tests_for_candidates(&candidates, Some(&focus));
        annotate_row_misses(
            &mut rows,
            &candidates,
            class,
            has_static_limit,
            &RowMissChange {
                owner: &case.owner,
                line_text: &case.after,
                old_line_text: Some(&case.before),
                focus: &focus,
                boundary_fact: None,
            },
            &case.tests,
        );
        candidates
            .iter()
            .zip(&rows)
            .map(|(candidate, row)| (candidate.relation, row.miss))
            .collect()
    }

    fn score_case() -> Result<Case, String> {
        case(
            "def score(value):\n    return 8\n",
            "from src.subject import score\n\ndef test_score():\n    assert score(0)\n",
            2,
            "    return 7",
            "    return 8",
        )
    }

    #[test]
    fn a_static_limit_keeps_every_row_none_even_when_the_class_says_weak() -> Result<(), String> {
        let case = score_case()?;
        // Positive control: without a static limit the same row has its miss.
        assert_eq!(
            run(&case, &ExposureClass::WeaklyExposed, false),
            vec![(
                PythonRelationKind::SyntacticCall,
                Some(RelatedTestMiss::WeakAssertion)
            )]
        );
        let rows = run(&case, &ExposureClass::WeaklyExposed, true);
        assert_eq!(rows.len(), 1);
        assert!(rows.iter().all(|(_, miss)| miss.is_none()), "{rows:?}");
        Ok(())
    }

    #[test]
    fn dunder_protocol_and_fixture_name_rows_get_no_no_call_path() -> Result<(), String> {
        let dunder = case(
            "class Box:\n    def __init__(self, v):\n        self.v = v\n    def __eq__(self, other):\n        return self.v == other.v\n",
            "from src.subject import Box\n\ndef test_box():\n    assert Box(1).v == 1\n",
            5,
            "        return self.v != other.v",
            "        return self.v == other.v",
        )?;
        let rows = run(&dunder, &ExposureClass::WeaklyExposed, false);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].0, PythonRelationKind::DunderProtocol);
        assert_eq!(rows[0].1, None);

        let fixture = case_in(
            "src/pricing.py",
            "tests/test_checkout.py",
            "def calculate_fee(amount):\n    return amount + 2\n",
            "from src.pricing import calculate_fee\n\n\ndef test_checkout_total(pricing):\n    fee = calculate_fee\n    assert 102 == 102\n",
            2,
            "    return amount + 1",
            "    return amount + 2",
        )?;
        let rows = run(&fixture, &ExposureClass::WeaklyExposed, false);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].0, PythonRelationKind::FixtureName);
        assert_eq!(rows[0].1, None);
        Ok(())
    }

    #[test]
    fn module_constant_owner_row_gets_no_no_call_path() -> Result<(), String> {
        let constant = case(
            "BASE_DISCOUNT = 8\n",
            "from src.subject import BASE_DISCOUNT\n\ndef test_subject_base_discount():\n    assert BASE_DISCOUNT > 0\n",
            1,
            "BASE_DISCOUNT = 7",
            "BASE_DISCOUNT = 8",
        )?;
        let rows = run(&constant, &ExposureClass::WeaklyExposed, false);
        assert!(!rows.is_empty(), "fixture rows must parse");
        assert!(
            rows.iter()
                .all(|(_, miss)| *miss != Some(RelatedTestMiss::NoCallPath)),
            "{rows:?}"
        );
        Ok(())
    }

    fn omission(case: &Case, index: usize) -> bool {
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
        let candidates = related_test_candidates(&case.owner, &case.tests);
        python_row_boundary_omission(
            &case.after,
            Some(&case.before),
            &case.owner,
            &candidates[index],
            &candidates,
            &focus,
        )
    }

    #[test]
    fn same_name_function_from_another_module_is_not_an_owner_call() -> Result<(), String> {
        let source =
            "def bulk(quantity):\n    if quantity > 100:\n        return 1\n    return 0\n";
        let before = "    if quantity >= 100:";
        let after = "    if quantity > 100:";
        // Positive control: the import resolves to the owner's module.
        let resolved = case_in(
            "src/pricing/legacy.py",
            "tests/test_bulk.py",
            source,
            "from pricing.legacy import bulk\n\ndef test_bulk():\n    assert bulk(101) == 1\n",
            2,
            before,
            after,
        )?;
        assert!(
            omission(&resolved, 0),
            "import-resolved call is an owner call"
        );
        // Same name, different module: not the owner's call.
        let other = case_in(
            "src/pricing/legacy.py",
            "tests/test_bulk.py",
            source,
            "from pricing.discounts import bulk\n\ndef test_bulk():\n    assert bulk(101) == 1\n",
            2,
            before,
            after,
        )?;
        for index in 0..related_test_candidates(&other.owner, &other.tests).len() {
            assert!(!omission(&other, index), "row {index}");
        }
        Ok(())
    }

    #[test]
    fn non_predicate_family_never_gets_missing_input() -> Result<(), String> {
        let case = case(
            "def score(value):\n    return 8\n",
            "from src.subject import score\n\ndef test_score():\n    assert score(0) == 9\n",
            2,
            "    return 7",
            "    return 8",
        )?;
        let rows = run(&case, &ExposureClass::WeaklyExposed, false);
        assert!(!rows.is_empty());
        assert!(
            rows.iter()
                .all(|(_, miss)| *miss != Some(RelatedTestMiss::MissingInput)),
            "{rows:?}"
        );
        Ok(())
    }

    #[test]
    fn single_row_unresolved_or_computed_operand_gets_no_missing_input() -> Result<(), String> {
        let source = "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n";
        for body in [
            "def test_unresolved(n):\n    assert can_reserve(n, 3) == True\n",
            "def test_computed():\n    assert can_reserve(2 + 3, 3) == True\n",
        ] {
            let case = case(
                source,
                &format!("from src.subject import can_reserve\n{body}"),
                2,
                "    if qty > on_hand:",
                "    if qty >= on_hand:",
            )?;
            let rows = run(&case, &ExposureClass::WeaklyExposed, false);
            assert_eq!(rows.len(), 1, "{body}");
            assert_ne!(rows[0].1, Some(RelatedTestMiss::MissingInput), "{body}");
        }
        Ok(())
    }

    #[test]
    fn old_equality_operands_count_as_the_boundary() -> Result<(), String> {
        // `level == 2` -> `level == 3`: a call at 2 or 3 separates old from new.
        let case = case(
            "def is_pro(level):\n    if level == 3:\n        return True\n    return False\n",
            "from src.subject import is_pro\n\ndef test_a_off_both():\n    assert is_pro(1) == False\n\ndef test_b_on_old():\n    assert is_pro(2) == False\n\ndef test_c_on_new():\n    assert is_pro(3) == True\n",
            2,
            "    if level == 2:",
            "    if level == 3:",
        )?;
        let candidates = related_test_candidates(&case.owner, &case.tests);
        let mut names: Vec<_> = candidates.iter().map(|c| c.test.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["test_a_off_both", "test_b_on_old", "test_c_on_new"],
            "fixture rows must parse"
        );
        let index = |name: &str| candidates.iter().position(|c| c.test.name == name);
        let at = |name: &str| index(name).ok_or_else(|| format!("missing {name}"));
        assert!(omission(&case, at("test_a_off_both")?), "off both pairs");
        assert!(
            !omission(&case, at("test_b_on_old")?),
            "a call on the OLD pair separates old from new"
        );
        assert!(
            !omission(&case, at("test_c_on_new")?),
            "a call on the NEW pair"
        );
        Ok(())
    }

    const RES: &str = "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n";
    const RES_IMPORT: &str = "from src.subject import can_reserve\n";

    fn reserve(tests: &str) -> Result<Case, String> {
        case(
            RES,
            &format!("{RES_IMPORT}{tests}"),
            2,
            "    if qty > on_hand:",
            "    if qty >= on_hand:",
        )
    }

    /// No row of `case` may claim `missing_input`, directly or via the pass.
    fn assert_no_missing_input(case: &Case, label: &str) {
        let count = related_test_candidates(&case.owner, &case.tests).len();
        assert!(count > 0, "{label}: fixture rows must parse");
        for index in 0..count {
            assert!(!omission(case, index), "{label}: row {index} omission");
        }
        let rows = run(case, &ExposureClass::WeaklyExposed, false);
        assert!(
            rows.iter()
                .all(|(_, miss)| *miss != Some(RelatedTestMiss::MissingInput)),
            "{label}: {rows:?}"
        );
    }

    #[test]
    fn positive_control_off_boundary_direct_call_is_an_omission() -> Result<(), String> {
        let case = reserve("def test_a():\n    assert can_reserve(5, 3) == True\n")?;
        assert!(omission(&case, 0));
        Ok(())
    }

    #[test]
    fn unreadable_old_equality_operand_is_not_an_omission() -> Result<(), String> {
        let source =
            "def is_pro(level):\n    if level == 3:\n        return True\n    return False\n";
        let tests =
            "from src.subject import is_pro\n\ndef test_a():\n    assert is_pro(2) == False\n";
        let readable = case(source, tests, 2, "    if level == 2:", "    if level == 3:")?;
        // Control: with a readable old pair, 2 sits on the OLD boundary.
        assert!(!omission(&readable, 0));
        let unreadable = case(
            source,
            tests,
            2,
            "    if level == 2 * 1:",
            "    if level == 3:",
        )?;
        assert_no_missing_input(&unreadable, "old operand `2 * 1`");
        Ok(())
    }

    #[test]
    fn indirect_or_aliased_owner_reach_is_not_an_omission() -> Result<(), String> {
        for (label, tests) in [
            (
                "helper in the test file",
                "def helper():\n    return can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n    assert helper() == True\n",
            ),
            (
                "map reference",
                "def test_a():\n    assert can_reserve(5, 3) == True\n    assert list(map(can_reserve, [4], [4])) == [True]\n",
            ),
            (
                "functools.partial",
                "import functools\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n    g = functools.partial(can_reserve, 4)\n    assert g(4) == True\n",
            ),
            (
                "alias",
                "def test_a():\n    f = can_reserve\n    assert can_reserve(5, 3) == True\n    assert f(4, 4) == True\n",
            ),
            (
                "fixture parameter",
                "import pytest\n\n@pytest.fixture\ndef reserver():\n    return can_reserve\n\ndef test_a(reserver):\n    assert can_reserve(5, 3) == True\n    assert reserver(4, 4) == True\n",
            ),
        ] {
            assert_no_missing_input(&reserve(tests)?, label);
        }
        Ok(())
    }

    #[test]
    fn owner_that_rebinds_a_compared_parameter_is_not_an_omission() -> Result<(), String> {
        let source = "def can_reserve(qty, on_hand):\n    qty = qty - 2\n    if qty >= on_hand:\n        return True\n    return False\n";
        let tests = "from src.subject import can_reserve\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n";
        let rebound = case(
            source,
            tests,
            3,
            "    if qty > on_hand:",
            "    if qty >= on_hand:",
        )?;
        assert_no_missing_input(&rebound, "rebound qty");
        // Control: the same call against a body that never rebinds is an omission.
        assert!(omission(
            &reserve("def test_a():\n    assert can_reserve(5, 3) == True\n")?,
            0
        ));
        Ok(())
    }

    #[test]
    fn mixed_kind_or_escaped_literals_are_unknown_not_unequal() -> Result<(), String> {
        let flag = "def f(flag):\n    if flag == 1:\n        return True\n    return False\n";
        for (label, tests) in [
            (
                "True vs 1",
                "from src.subject import f\n\ndef test_a():\n    assert f(True) == True\n",
            ),
            (
                "False vs 0",
                "from src.subject import f\n\ndef test_a():\n    assert f(False) == False\n",
            ),
        ] {
            let case = case(flag, tests, 2, "    if flag == 0:", "    if flag == 1:")?;
            assert_no_missing_input(&case, label);
        }
        let name = "def f(name):\n    if name == 'a':\n        return True\n    return False\n";
        let escaped = case(
            name,
            "from src.subject import f\n\ndef test_a():\n    assert f('\\x61') == True\n",
            2,
            "    if name == 'b':",
            "    if name == 'a':",
        )?;
        assert_no_missing_input(&escaped, "escaped string");
        // Control: two plain strings that differ are an omission.
        let plain = case(
            name,
            "from src.subject import f\n\ndef test_a():\n    assert f('z') == False\n",
            2,
            "    if name == 'a':",
            "    if name == 'a':",
        )?;
        assert!(omission(&plain, 0));
        Ok(())
    }

    #[test]
    fn method_call_on_another_receiver_is_not_an_owner_call() -> Result<(), String> {
        let source = "class Cart:\n    def can_reserve(self, qty, on_hand):\n        if qty >= on_hand:\n            return True\n        return False\n\nclass Other:\n    def can_reserve(self, qty, on_hand):\n        return True\n";
        let build = |body: &str| {
            case(
                source,
                &format!("from src.subject import Cart, Other\n\n{body}"),
                3,
                "        if qty > on_hand:",
                "        if qty >= on_hand:",
            )
        };
        for (label, body) in [
            (
                "other class",
                "def test_a():\n    assert Other().can_reserve(5, 3) == True\n",
            ),
            (
                "unknown receiver",
                "def test_a(cart):\n    assert cart.can_reserve(5, 3) == True\n",
            ),
            (
                "variable receiver (binding is not read)",
                "def test_a():\n    c = Cart()\n    assert c.can_reserve(5, 3) == True\n",
            ),
            (
                "reassigned receiver",
                "def test_a():\n    c = Cart()\n    c = Other()\n    assert c.can_reserve(5, 3) == True\n",
            ),
        ] {
            assert_no_missing_input(&build(body)?, label);
        }
        // An instance method is never claimed (the constructor body is unread,
        // so `Cart(...)` could call the owner), even through the owner class.
        let owned = build("def test_a():\n    assert Cart().can_reserve(5, 3) == True\n")?;
        assert_no_missing_input(&owned, "instance method through its constructor");
        // Control: a classmethod called on its class reaches no constructor.
        let class_source = "class Cart:\n    @classmethod\n    def can_reserve(cls, qty, on_hand):\n        if qty >= on_hand:\n            return True\n        return False\n";
        let classmethod = case(
            class_source,
            "from src.subject import Cart\n\ndef test_a():\n    assert Cart.can_reserve(5, 3) == True\n",
            4,
            "        if qty > on_hand:",
            "        if qty >= on_hand:",
        )?;
        assert!(omission(&classmethod, 0), "classmethod on its class");
        Ok(())
    }

    #[test]
    fn relational_change_that_moves_the_operand_is_not_an_omission() -> Result<(), String> {
        let source = "def big(x):\n    if x > 20:\n        return True\n    return False\n";
        let tests = "from src.subject import big\n\ndef test_a():\n    assert big(5) == False\n";
        let moved = case(source, tests, 2, "    if x >= 10:", "    if x > 20:")?;
        assert_no_missing_input(&moved, "x >= 10 -> x > 20");
        // Control: same operands, operator change only, is an omission.
        let same = case(source, tests, 2, "    if x >= 20:", "    if x > 20:")?;
        assert!(omission(&same, 0));
        Ok(())
    }

    #[test]
    fn implicit_reach_outside_the_test_function_is_not_an_omission() -> Result<(), String> {
        let direct = "def test_a():\n    assert can_reserve(5, 3) == True\n";
        for (label, tests) in [
            (
                "module autouse fixture",
                "import pytest\n\n@pytest.fixture(autouse=True)\ndef boot():\n    can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "in-class autouse fixture",
                "import pytest\n\nclass TestReserve:\n    @pytest.fixture(autouse=True)\n    def boot(self):\n        can_reserve(4, 4)\n\n    def test_a(self):\n        assert can_reserve(5, 3) == True\n",
            ),
            (
                "pytestmark usefixtures",
                "import pytest\n\n@pytest.fixture\ndef boot():\n    can_reserve(4, 4)\n\npytestmark = pytest.mark.usefixtures('boot')\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "pytestmark parametrize",
                "import pytest\n\npytestmark = pytest.mark.parametrize('n', [4])\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "setUp hook",
                "import unittest\n\nclass T(unittest.TestCase):\n    def setUp(self):\n        can_reserve(4, 4)\n\n    def test_a(self):\n        self.assertEqual(can_reserve(5, 3), True)\n",
            ),
            (
                "setup_module hook",
                "def setup_module():\n    can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "module-level call",
                "RESULT = can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "test module redefines the owner name",
                "def can_reserve(qty, on_hand):\n    return True\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "test module rebinds the owner name",
                "can_reserve = lambda qty, on_hand: True\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "callee from a dict subscript",
                "H = {'x': can_reserve}\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n    assert H['x'](4, 4) == True\n",
            ),
            (
                "callee from a list subscript",
                "H = [can_reserve]\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n    assert H[0](4, 4) == True\n",
            ),
            (
                "custom test decorator",
                "from helpers import wrap\n\n@wrap\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "functools.lru_cache test decorator",
                "import functools\n\n@functools.lru_cache\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "pytest_generate_tests in the test module",
                "def pytest_generate_tests(metafunc):\n    can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "rebound bare import alias",
                "import src.subject as s\n\ns = None\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "assert-named helper method on the test class",
                "import unittest\n\nclass T(unittest.TestCase):\n    def assert_reserved(self, a, b):\n        self.assertEqual(can_reserve(a, b), True)\n\n    def test_a(self):\n        self.assertEqual(can_reserve(5, 3), True)\n        self.assert_reserved(4, 4)\n",
            ),
            (
                "camelCase helper method on the test class",
                "import unittest\n\nclass T(unittest.TestCase):\n    def assertReserves(self, a, b):\n        self.assertEqual(can_reserve(a, b), True)\n\n    def test_a(self):\n        self.assertEqual(can_reserve(5, 3), True)\n        self.assertReserves(4, 4)\n",
            ),
            (
                "star import into the test module",
                "from tests.fixtures import *\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "test-local shadow of the owner name",
                "def test_a():\n    def can_reserve(qty, on_hand):\n        return True\n    assert can_reserve(5, 3) == True\n",
            ),
        ] {
            assert_no_missing_input(&reserve(tests)?, label);
        }
        // Controls: the same direct call with none of those shapes is an
        // omission, and a module of only imports, definitions and constants
        // stays one.
        assert!(omission(&reserve(direct)?, 0));
        assert!(omission(
            &reserve(
                "import unittest\n\nclass T(unittest.TestCase):\n    def test_a(self):\n        self.assertEqual(can_reserve(5, 3), True)\n"
            )?,
            0
        ));
        assert!(omission(&reserve(&format!("LIMIT = 3\n\n{direct}"))?, 0));
        Ok(())
    }

    #[test]
    fn decorated_owner_is_not_an_omission() -> Result<(), String> {
        let tests = "from src.subject import can_reserve\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n";
        let build = |decorator: &str| {
            case(
                &format!(
                    "{decorator}def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n"
                ),
                tests,
                if decorator.is_empty() { 2 } else { 3 },
                "    if qty > on_hand:",
                "    if qty >= on_hand:",
            )
        };
        assert_no_missing_input(&build("@always_true\n")?, "decorated owner");
        assert!(omission(&build("")?, 0), "undecorated control");
        Ok(())
    }

    #[test]
    fn inherited_or_decorated_test_class_is_not_an_omission() -> Result<(), String> {
        for (label, tests) in [
            (
                "base from another module",
                "from helpers import Base\n\nclass TestReserve(Base):\n    def test_a(self):\n        assert can_reserve(5, 3) == True\n",
            ),
            (
                "mixin base",
                "class Mixin:\n    def setup_method(self, method):\n        can_reserve(4, 4)\n\nclass TestReserve(Mixin):\n    def test_a(self):\n        assert can_reserve(5, 3) == True\n",
            ),
            (
                "decorated test class",
                "from helpers import some_deco\n\n@some_deco\nclass TestReserve:\n    def test_a(self):\n        assert can_reserve(5, 3) == True\n",
            ),
            (
                "module attribute assignment",
                "import src.subject as s\n\ns.can_reserve = lambda a, b: True\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
        ] {
            assert_no_missing_input(&reserve(tests)?, label);
        }
        // Controls: a plain class and a unittest.TestCase subclass without
        // hooks are not implicit reach.
        assert!(omission(
            &reserve(
                "class TestReserve:\n    def test_a(self):\n        assert can_reserve(5, 3) == True\n"
            )?,
            0
        ));
        Ok(())
    }

    #[test]
    fn reversed_relational_direction_is_not_an_omission() -> Result<(), String> {
        let tests = "from src.subject import can_reserve\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n";
        let build = |before: &str, after: &str| {
            case(
                &format!(
                    "def can_reserve(qty, on_hand):\n{after}\n        return True\n    return False\n"
                ),
                tests,
                2,
                before,
                after,
            )
        };
        for (before, after) in [
            ("    if qty < on_hand:", "    if qty > on_hand:"),
            ("    if qty > on_hand:", "    if qty < on_hand:"),
            ("    if qty >= on_hand:", "    if qty <= on_hand:"),
            ("    if qty <= on_hand:", "    if qty >= on_hand:"),
        ] {
            assert_no_missing_input(&build(before, after)?, &format!("{before} -> {after}"));
        }
        // Control: an inclusivity-only change in the same direction is one.
        assert!(omission(
            &build("    if qty > on_hand:", "    if qty >= on_hand:")?,
            0
        ));
        Ok(())
    }

    #[test]
    fn recursive_owner_is_not_an_omission() -> Result<(), String> {
        let tests = "from src.subject import count_down\n\ndef test_a():\n    assert count_down(5, 3) == 3\n";
        let recursive = case(
            "def count_down(n, floor):\n    if n >= floor:\n        return count_down(n - 1, floor) + 1\n    return 0\n",
            tests,
            2,
            "    if n > floor:",
            "    if n >= floor:",
        )?;
        assert_no_missing_input(&recursive, "self-recursive owner");
        // Control: the same predicate without the self-call is an omission.
        let plain = case(
            "def count_down(n, floor):\n    if n >= floor:\n        return 1\n    return 0\n",
            tests,
            2,
            "    if n > floor:",
            "    if n >= floor:",
        )?;
        assert!(omission(&plain, 0));
        Ok(())
    }

    #[test]
    fn wider_line_change_is_not_an_omission() -> Result<(), String> {
        let tests =
            "from src.subject import f\n\ndef test_a():\n    assert f(5, 3, True) == True\n";
        let build = |source_line: &str, before: &str| {
            case(
                &format!(
                    "def f(qty, on_hand, force):\n{source_line}\n        return True\n    return False\n"
                ),
                tests,
                2,
                before,
                source_line,
            )
        };
        let widened = build(
            "    if qty >= on_hand or force:",
            "    if qty > on_hand and force:",
        )?;
        assert_no_missing_input(&widened, "and -> or with the operator");
        // Control: the connective unchanged, only inclusivity moves.
        let narrow = build(
            "    if qty >= on_hand and force:",
            "    if qty > on_hand and force:",
        )?;
        assert!(omission(&narrow, 0));
        // An equality change must be the whole condition on both sides.
        let eq_tests =
            "from src.subject import g\n\ndef test_a():\n    assert g(1, True) == True\n";
        let eq = case(
            "def g(level, force):\n    if level == 3 or force:\n        return True\n    return False\n",
            eq_tests,
            2,
            "    if level == 2 and force:",
            "    if level == 3 or force:",
        )?;
        assert_no_missing_input(&eq, "equality with a widened condition");
        Ok(())
    }

    #[test]
    fn keyword_that_is_not_a_declared_parameter_binds_no_operand() -> Result<(), String> {
        let tests = "from src.subject import f\n\ndef test_a():\n    assert f(10, limit=3) == 1\n";
        // `limit` is a body local; `limit=3` lands in `**opts`, and the real
        // comparison is 10 >= 10.
        let kwargs = case(
            "def f(qty, **opts):\n    limit = 10\n    if qty >= limit:\n        return 1\n    return 0\n",
            tests,
            3,
            "    if qty > limit:",
            "    if qty >= limit:",
        )?;
        assert_no_missing_input(&kwargs, "keyword captured by **opts");
        // Control: the same call against a declared `limit` parameter binds it.
        let declared = case(
            "def f(qty, limit):\n    if qty >= limit:\n        return 1\n    return 0\n",
            tests,
            2,
            "    if qty > limit:",
            "    if qty >= limit:",
        )?;
        assert!(omission(&declared, 0));
        Ok(())
    }

    /// #5564: beside a strong row that omits the boundary, a strong row that
    /// sits on it keeps neither the miss nor the support, even when the
    /// pass is handed the finding's boundary fact (with its sink).
    #[test]
    fn strong_on_boundary_row_gets_no_miss_and_no_support() -> Result<(), String> {
        use crate::domain::{FlowSinkFact, FlowSinkKind, MissingDiscriminatorFact};
        let case = reserve(
            "def test_off_boundary():\n    assert can_reserve(5, 3) == True\n\ndef test_on_boundary():\n    assert can_reserve(4, 4) == True\n",
        )?;
        let fact = MissingDiscriminatorFact {
            value: "qty == on_hand".to_string(),
            reason: "no related test passes the boundary".to_string(),
            flow_sink: Some(FlowSinkFact {
                kind: FlowSinkKind::ReturnValue,
                text: "return True".to_string(),
                line: 3,
                owner: None,
            }),
        };
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
        let candidates = related_test_candidates(&case.owner, &case.tests);
        let mut rows = related_tests_for_candidates(&candidates, Some(&focus));
        annotate_row_misses(
            &mut rows,
            &candidates,
            &ExposureClass::WeaklyExposed,
            false,
            &RowMissChange {
                owner: &case.owner,
                line_text: &case.after,
                old_line_text: Some(&case.before),
                focus: &focus,
                boundary_fact: Some(&fact),
            },
            &case.tests,
        );
        let row = |name: &str| rows.iter().find(|row| row.name == name);
        let off = row("test_off_boundary").ok_or("off-boundary row")?;
        let on = row("test_on_boundary").ok_or("on-boundary row")?;
        assert_eq!(off.miss, Some(RelatedTestMiss::MissingInput));
        let support = off.miss_support.as_ref().ok_or("omitting row support")?;
        assert_eq!(support.value, "qty == on_hand");
        assert_eq!(support.flow_sink, fact.flow_sink);
        assert_eq!(on.miss, None);
        assert_eq!(on.miss_support, None);
        Ok(())
    }

    #[test]
    fn miss_annotation_changes_nothing_but_the_miss_field() -> Result<(), String> {
        let cases = [
            case(
                "def score(value):\n    return 8\n",
                "from src.subject import score\n\ndef test_a():\n    assert score(0)\n\ndef test_score_is_importable():\n    assert callable(score)\n\ndef test_b():\n    assert score(1) == 8\n",
                2,
                "    return 7",
                "    return 8",
            )?,
            case(
                "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n",
                "from src.subject import can_reserve\n\ndef test_off():\n    assert can_reserve(5, 3) == True\n\ndef test_weak():\n    assert can_reserve(4, 4)\n",
                2,
                "    if qty > on_hand:",
                "    if qty >= on_hand:",
            )?,
        ];
        let mut annotated_any = false;
        for case in &cases {
            let family = classify_probe_shape(&case.after).0;
            let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
            let candidates = related_test_candidates(&case.owner, &case.tests);
            let stripped = related_tests_for_candidates(&candidates, Some(&focus));
            assert!(stripped.iter().all(|row| row.miss.is_none()));
            let mut rows = stripped.clone();
            annotate_row_misses(
                &mut rows,
                &candidates,
                &ExposureClass::WeaklyExposed,
                false,
                &RowMissChange {
                    owner: &case.owner,
                    line_text: &case.after,
                    old_line_text: Some(&case.before),
                    focus: &focus,
                    boundary_fact: None,
                },
                &case.tests,
            );
            assert_eq!(rows.len(), stripped.len());
            for (row, base) in rows.iter().zip(&stripped) {
                annotated_any |= row.miss.is_some();
                // Order, identity, oracle, relation and confidence are the
                // un-annotated row's; the role stays contributing.
                let mut reset = row.clone();
                reset.miss = None;
                reset.miss_support = None;
                assert_eq!(&reset, base);
                assert!(row.is_contributing());
            }
        }
        assert!(annotated_any, "positive control: some row gains a miss");
        Ok(())
    }

    fn two_row_case() -> Result<Case, String> {
        case(
            "def score(value):\n    return 8\n",
            "from src.subject import score\n\ndef test_a():\n    assert score(0)\n\ndef test_score_is_importable():\n    assert callable(score)\n",
            2,
            "    return 7",
            "    return 8",
        )
    }

    fn base_rows(case: &Case) -> Vec<crate::domain::RelatedTest> {
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
        let candidates = related_test_candidates(&case.owner, &case.tests);
        related_tests_for_candidates(&candidates, Some(&focus))
    }

    fn annotate(case: &Case, rows: &mut [crate::domain::RelatedTest]) {
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
        let candidates = related_test_candidates(&case.owner, &case.tests);
        annotate_row_misses(
            rows,
            &candidates,
            &ExposureClass::WeaklyExposed,
            false,
            &RowMissChange {
                owner: &case.owner,
                line_text: &case.after,
                old_line_text: Some(&case.before),
                focus: &focus,
                boundary_fact: None,
            },
            &case.tests,
        );
    }

    #[test]
    fn mismatched_length_or_reordered_rows_annotate_nothing() -> Result<(), String> {
        let case = two_row_case()?;
        // Positive control: aligned rows gain misses.
        let mut aligned = base_rows(&case);
        assert_eq!(aligned.len(), 2);
        annotate(&case, &mut aligned);
        assert!(aligned.iter().any(|row| row.miss.is_some()), "control");

        let mut truncated = base_rows(&case);
        truncated.truncate(1);
        annotate(&case, &mut truncated);
        assert!(truncated.iter().all(|row| row.miss.is_none()));

        let mut reordered = base_rows(&case);
        reordered.reverse();
        annotate(&case, &mut reordered);
        assert!(reordered.iter().all(|row| row.miss.is_none()));
        Ok(())
    }

    fn two_oracle_case() -> Result<Case, String> {
        case(
            "def score(value):\n    return 8\n",
            "from src.subject import score\n\ndef test_a():\n    assert score(0)\n\ndef test_b():\n    assert score(1)\n",
            2,
            "    return 7",
            "    return 8",
        )
    }

    #[test]
    fn reordered_rows_that_both_have_oracles_annotate_nothing() -> Result<(), String> {
        let case = two_oracle_case()?;
        let mut aligned = base_rows(&case);
        assert_eq!(aligned.len(), 2);
        assert!(aligned.iter().all(|row| row.oracle.is_some()));
        annotate(&case, &mut aligned);
        // Control: aligned rows each gain their own reason.
        assert!(aligned.iter().all(|row| row.miss.is_some()), "control");

        let mut reordered = base_rows(&case);
        reordered.reverse();
        annotate(&case, &mut reordered);
        assert!(reordered.iter().all(|row| row.miss.is_none()));
        Ok(())
    }

    #[test]
    fn weak_strength_row_gets_weak_assertion_like_smoke() -> Result<(), String> {
        let case = two_oracle_case()?;
        for strength in [
            crate::domain::OracleStrength::Weak,
            crate::domain::OracleStrength::Smoke,
        ] {
            let mut rows = base_rows(&case);
            for row in &mut rows {
                row.oracle_strength = strength.clone();
            }
            annotate(&case, &mut rows);
            assert!(
                rows.iter()
                    .all(|row| row.miss == Some(RelatedTestMiss::WeakAssertion)),
                "{strength:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn recognized_oracle_with_unknown_strength_gets_no_miss() -> Result<(), String> {
        let case = two_row_case()?;
        let mut rows = base_rows(&case);
        let row = rows
            .iter_mut()
            .find(|row| row.name == "test_a")
            .ok_or("test_a row")?;
        assert!(row.oracle.is_some());
        row.oracle_strength = crate::domain::OracleStrength::Unknown;
        annotate(&case, &mut rows);
        let row = rows
            .iter()
            .find(|row| row.name == "test_a")
            .ok_or("test_a row")?;
        assert_eq!(row.miss, None);
        Ok(())
    }
}
