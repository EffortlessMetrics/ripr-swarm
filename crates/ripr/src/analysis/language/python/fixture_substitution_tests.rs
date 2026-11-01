//! A fixture, hook or setup the test runs under that replaces the owner keeps
//! the `mocked_module` limit, like an inline `monkeypatch.setattr`. Fixtures
//! that do not substitute anything, or that the test cannot reach, keep their
//! credit.

use super::fixtures_tests::workspace;
use super::*;
use std::path::Path;

const CALC: &str = "def price(x):\n    return x * 3\n";
const TEST: &str = "import calc\n\n\ndef test_p():\n    assert calc.price(3) == 6\n";
const PATCHING_BODY: &str = "    monkeypatch.setattr(calc, 'price', lambda x: x * 2)\n";

fn classify_price(files: &[(&str, &str)]) -> Result<Finding, String> {
    let mut all = vec![("calc.py", CALC)];
    all.extend_from_slice(files);
    let (owners, tests) = workspace(&all);
    classify_change(Path::new("calc.py"), 2, "    return x * 3", &owners, &tests)
        .ok_or_else(|| "the changed line must classify".to_string())
}

fn assert_mocked(files: &[(&str, &str)], case: &str) -> Result<(), String> {
    let finding = classify_price(files)?;
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::MockedModule),
        "{case}: {:?}",
        finding.evidence
    );
    assert_ne!(finding.class, ExposureClass::Exposed, "{case}");
    Ok(())
}

fn assert_credited(files: &[(&str, &str)], case: &str) -> Result<(), String> {
    let finding = classify_price(files)?;
    assert_eq!(
        finding.static_limit_kind, None,
        "{case}: {:?}",
        finding.evidence
    );
    assert_eq!(finding.class, ExposureClass::Exposed, "{case}");
    Ok(())
}

fn autouse_fixture(body: &str) -> String {
    format!(
        "import calc\nimport pytest\n\n\n@pytest.fixture(autouse=True)\ndef _m(monkeypatch):\n{body}"
    )
}

#[test]
fn a_plain_test_with_no_fixture_keeps_its_credit() -> Result<(), String> {
    assert_credited(&[("tests/test_calc.py", TEST)], "positive control")
}

#[test]
fn an_autouse_conftest_fixture_that_patches_the_owner_keeps_the_limit() -> Result<(), String> {
    assert_mocked(
        &[
            ("tests/conftest.py", &autouse_fixture(PATCHING_BODY)),
            ("tests/test_calc.py", TEST),
        ],
        "autouse monkeypatch in conftest",
    )?;
    // A conftest higher up the directory chain reaches the test too.
    assert_mocked(
        &[
            ("conftest.py", &autouse_fixture(PATCHING_BODY)),
            ("tests/test_calc.py", TEST),
        ],
        "autouse monkeypatch in the root conftest",
    )
}

#[test]
fn an_autouse_fixture_in_the_test_module_keeps_the_limit() -> Result<(), String> {
    let test = format!("{}\n\n{TEST}", autouse_fixture(PATCHING_BODY));
    assert_mocked(&[("tests/test_calc.py", &test)], "autouse in module")
}

#[test]
fn a_requested_fixture_that_patches_keeps_the_limit_even_when_unused_in_the_body()
-> Result<(), String> {
    let test = format!(
        "import calc\nimport pytest\n\n\n@pytest.fixture\ndef pp(monkeypatch):\n{PATCHING_BODY}\n\ndef test_p(pp):\n    assert calc.price(3) == 6\n"
    );
    assert_mocked(&[("tests/test_calc.py", &test)], "requested by name")
}

#[test]
fn mock_library_forms_in_a_fixture_keep_the_limit() -> Result<(), String> {
    let bodies = [
        "def pp(mocker):\n    mocker.patch('calc.price', return_value=6)\n",
        "def pp():\n    with patch('calc.price', return_value=6):\n        yield\n",
        "def pp():\n    patcher = patch.object(calc, 'price', return_value=6)\n    patcher.start()\n    yield\n    patcher.stop()\n",
        "def pp():\n    setattr(calc, 'price', lambda x: x * 2)\n",
        "def pp():\n    calc.price = lambda x: x * 2\n",
    ];
    for body in bodies {
        let test = format!(
            "import calc\nimport pytest\nfrom unittest.mock import patch\n\n\n@pytest.fixture\n{body}\n\ndef test_p(pp):\n    assert calc.price(3) == 6\n"
        );
        assert_mocked(&[("tests/test_calc.py", &test)], body)?;
    }
    Ok(())
}

#[test]
fn a_fixture_reached_through_another_fixture_or_a_mark_keeps_the_limit() -> Result<(), String> {
    let conftest = format!(
        "import calc\nimport pytest\n\n\n@pytest.fixture\ndef stub(monkeypatch):\n{PATCHING_BODY}\n\n@pytest.fixture\ndef world(stub):\n    return 1\n"
    );
    let cases = [
        (
            "transitive request",
            "def test_p(world):\n    assert calc.price(3) == 6\n".to_string(),
        ),
        (
            "usefixtures on the test",
            "@pytest.mark.usefixtures('stub')\ndef test_p():\n    assert calc.price(3) == 6\n"
                .to_string(),
        ),
        (
            "usefixtures on the class",
            "@pytest.mark.usefixtures('stub')\nclass TestCalc:\n    def test_p(self):\n        assert calc.price(3) == 6\n"
                .to_string(),
        ),
        (
            "pytestmark usefixtures",
            "pytestmark = pytest.mark.usefixtures('stub')\n\n\ndef test_p():\n    assert calc.price(3) == 6\n"
                .to_string(),
        ),
        (
            "non-literal usefixtures",
            "NAMES = ('stub',)\n\n\n@pytest.mark.usefixtures(*NAMES)\ndef test_p():\n    assert calc.price(3) == 6\n"
                .to_string(),
        ),
    ];
    for (case, body) in cases {
        let test = format!("import calc\nimport pytest\n\n\n{body}");
        assert_mocked(
            &[
                ("tests/conftest.py", &conftest),
                ("tests/test_calc.py", &test),
            ],
            case,
        )?;
    }
    Ok(())
}

#[test]
fn unittest_setup_and_class_decorators_that_patch_keep_the_limit() -> Result<(), String> {
    let cases = [
        "import unittest\nfrom unittest.mock import patch\nimport calc\n\n\nclass T(unittest.TestCase):\n    def setUp(self):\n        self.patcher = patch('calc.price', return_value=6)\n        self.patcher.start()\n        self.addCleanup(self.patcher.stop)\n\n    def test_p(self):\n        self.assertEqual(calc.price(3), 6)\n",
        "import unittest\nfrom unittest.mock import patch\nimport calc\n\n\n@patch.object(calc, 'price', return_value=6)\nclass T(unittest.TestCase):\n    def test_p(self, _price):\n        self.assertEqual(calc.price(3), 6)\n",
        "import calc\n\n\nclass TestCalc:\n    def setup_method(self):\n        calc.price = lambda x: x * 2\n\n    def test_p(self):\n        assert calc.price(3) == 6\n",
    ];
    for test in cases {
        assert_mocked(&[("tests/test_calc.py", test)], test)?;
    }
    Ok(())
}

#[test]
fn conftest_hooks_and_module_setup_that_patch_keep_the_limit() -> Result<(), String> {
    let hook =
        "import calc\n\n\ndef pytest_runtest_setup(item):\n    calc.price = lambda x: x * 2\n";
    assert_mocked(
        &[("tests/conftest.py", hook), ("tests/test_calc.py", TEST)],
        "pytest_runtest_setup hook",
    )?;
    let module_setup = "import calc\n\n\ndef setup_module(module):\n    calc.price = lambda x: x * 2\n\n\ndef test_p():\n    assert calc.price(3) == 6\n";
    assert_mocked(&[("tests/test_calc.py", module_setup)], "setup_module")?;
    let top_level = "import calc\nfrom unittest.mock import patch\n\npatch('calc.price', return_value=6).start()\n\n\ndef test_p():\n    assert calc.price(3) == 6\n";
    assert_mocked(&[("tests/test_calc.py", top_level)], "top-level start")
}

#[test]
fn a_test_module_def_stand_in_assigned_at_top_level_is_not_ambient() -> Result<(), String> {
    // The stand-in is spelled in the test module (RIPR-SPEC-0108 keeps that
    // class); an unspelled value is still ambient.
    let spelled = "import calc\n\n\ndef fake(x):\n    return 0\n\n\ncalc.price = fake\n\n\ndef test_p():\n    assert calc.price(3) == 6\n";
    let finding = classify_price(&[("tests/test_calc.py", spelled)])?;
    assert_ne!(
        finding.static_limit_kind,
        Some(StaticLimitKind::MockedModule),
        "{:?}",
        finding.evidence
    );
    let unspelled = "import calc\nimport other\n\n\ncalc.price = other.fake\n\n\ndef test_p():\n    assert calc.price(3) == 6\n";
    assert_mocked(&[("tests/test_calc.py", unspelled)], "unspelled stand-in")
}

#[test]
fn fixtures_that_do_not_substitute_or_are_not_reachable_keep_their_credit() -> Result<(), String> {
    let env_only = "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef _env(monkeypatch):\n    monkeypatch.setenv('TZ', 'UTC')\n    monkeypatch.chdir('/')\n";
    assert_credited(
        &[
            ("tests/conftest.py", env_only),
            ("tests/test_calc.py", TEST),
        ],
        "autouse that only sets environment",
    )?;
    let patching_unrequested = format!(
        "import calc\nimport pytest\n\n\n@pytest.fixture\ndef stub(monkeypatch):\n{PATCHING_BODY}"
    );
    assert_credited(
        &[
            ("tests/conftest.py", &patching_unrequested),
            ("tests/test_calc.py", TEST),
        ],
        "patching fixture nobody requests",
    )?;
    assert_credited(
        &[
            ("other/conftest.py", &autouse_fixture(PATCHING_BODY)),
            ("tests/test_calc.py", TEST),
        ],
        "autouse conftest in an unrelated directory",
    )?;
    let other_class = format!(
        "import calc\nimport pytest\n\n\nclass TestOther:\n    @pytest.fixture(autouse=True)\n    def _m(self, monkeypatch):\n    {PATCHING_BODY}\n    def test_other(self):\n        assert True\n\n\nclass TestCalc:\n    def test_p(self):\n        assert calc.price(3) == 6\n"
    );
    assert_credited(
        &[("tests/test_calc.py", &other_class)],
        "class-scoped autouse of a sibling class",
    )?;
    let data = "import pytest\n\n\n@pytest.fixture\ndef rows():\n    return [1, 2, 3]\n";
    let uses_data =
        "import calc\n\n\ndef test_p(rows):\n    assert rows\n    assert calc.price(3) == 6\n";
    assert_credited(
        &[
            ("tests/conftest.py", data),
            ("tests/test_calc.py", uses_data),
        ],
        "data fixture",
    )
}

#[test]
fn a_conftest_pytestmark_usefixtures_reaches_tests_below_it() -> Result<(), String> {
    let conftest = format!(
        "import calc\nimport pytest\n\n\n@pytest.fixture\ndef stub(monkeypatch):\n{PATCHING_BODY}\n\npytestmark = pytest.mark.usefixtures('stub')\n"
    );
    assert_mocked(
        &[
            ("tests/conftest.py", &conftest),
            ("tests/test_calc.py", TEST),
        ],
        "conftest pytestmark usefixtures",
    )
}

#[test]
fn an_unread_chain_conftest_fails_closed() -> Result<(), String> {
    let (_, mut tests) = workspace(&[("calc.py", CALC), ("tests/test_calc.py", TEST)]);
    let scopes = BTreeMap::new();
    super::fixture_substitution::apply_fixture_substitution(&mut tests, &scopes, |_| false);
    assert!(
        tests
            .iter()
            .all(|t| !t.constant_rebinding.fixture_substitution)
    );
    super::fixture_substitution::apply_fixture_substitution(&mut tests, &scopes, |conftest| {
        conftest == Path::new("tests/conftest.py")
    });
    assert!(!tests.is_empty());
    assert!(
        tests
            .iter()
            .all(|t| t.constant_rebinding.fixture_substitution)
    );
    Ok(())
}
