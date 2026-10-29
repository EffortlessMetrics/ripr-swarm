//! A conftest fixture that only builds a `CliRunner` with literal arguments
//! hides no discriminator or expected value, so it does not trigger the
//! `unresolved_pytest_fixture` limit (#11016, pallets/click's `runner`). Every
//! other shape, and anything that could make the runner differ from an inlined
//! `CliRunner()`, keeps the limit.

use super::fixtures_tests::workspace;
use super::*;
use std::path::Path;

const SHOP: &str = "def total(amount):\n    return amount * 2\n";

const CONFTEST: &str = "import pytest\n\nfrom click.testing import CliRunner\n\n\n@pytest.fixture(scope=\"function\")\ndef runner(request):\n    return CliRunner()\n";

const TEST: &str = "from shop import total\n\n\ndef test_total(runner):\n    assert runner is not None\n    assert total(3) == 6\n";

fn classify_total(conftest: &str, test: &str) -> Result<Finding, String> {
    let (owners, tests) = workspace(&[
        ("shop/__init__.py", SHOP),
        ("tests/conftest.py", conftest),
        ("tests/test_shop.py", test),
    ]);
    classify_change(
        Path::new("shop/__init__.py"),
        2,
        "    return amount * 3",
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed line must classify".to_string())
}

fn assert_limited(finding: &Finding, case: &str) {
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::UnresolvedPytestFixture),
        "{case}: {:?}",
        finding.evidence
    );
}

#[test]
fn a_clirunner_fixture_does_not_hide_the_expected_value() -> Result<(), String> {
    let finding = classify_total(CONFTEST, TEST)?;
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    assert_eq!(finding.class, ExposureClass::Exposed);
    // Without the parameter, and with a literal keyword, the shape holds.
    let variant = CONFTEST
        .replace("def runner(request):", "def runner():")
        .replace("CliRunner()", "CliRunner(echo_stdin=True)");
    assert_eq!(classify_total(&variant, TEST)?.static_limit_kind, None);
    Ok(())
}

#[test]
fn shapes_that_can_hide_state_keep_the_limit() -> Result<(), String> {
    let cases = [
        (
            "env mapping",
            CONFTEST.replace("CliRunner()", "CliRunner(env={\"A\": \"1\"})"),
        ),
        (
            "non-literal argument",
            CONFTEST.replace("CliRunner()", "CliRunner(START)"),
        ),
        (
            "request.param is not read, but extra statements are",
            CONFTEST.replace(
                "    return CliRunner()",
                "    runner = CliRunner()\n    runner.env = request.param\n    return runner",
            ),
        ),
        (
            "module scope",
            CONFTEST.replace("scope=\"function\"", "scope=\"module\""),
        ),
        (
            "autouse keyword",
            CONFTEST.replace("scope=\"function\"", "autouse=True"),
        ),
        (
            "another module's CliRunner",
            CONFTEST.replace("from click.testing import", "from helpers import"),
        ),
        (
            "other class from click.testing",
            CONFTEST.replace("CliRunner", "Result"),
        ),
        (
            "star import in the conftest",
            format!("from helpers import *\n{CONFTEST}"),
        ),
        (
            "another fixture takes the runner",
            format!(
                "{CONFTEST}\n\n@pytest.fixture(autouse=True)\ndef seed(runner):\n    runner.env = {{}}\n"
            ),
        ),
    ];
    for (case, conftest) in cases {
        assert_limited(&classify_total(&conftest, TEST)?, case);
    }
    Ok(())
}

#[test]
fn a_test_that_rebinds_the_runner_or_overrides_it_keeps_the_limit() -> Result<(), String> {
    let rebound = TEST.replace(
        "    assert runner is not None",
        "    runner = make()\n    assert runner is not None",
    );
    assert_limited(&classify_total(CONFTEST, &rebound)?, "rebound local");
    let overridden = format!(
        "import pytest\nfrom shop import total\n\n\n@pytest.fixture\ndef runner():\n    return object()\n\n\n{}",
        TEST.trim_start_matches("from shop import total\n\n\n")
    );
    assert_limited(
        &classify_total(CONFTEST, &overridden)?,
        "same-file override",
    );
    Ok(())
}
