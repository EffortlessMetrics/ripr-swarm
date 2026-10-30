//! A parametrize decorator runs when the module is imported, so a writer that
//! only lives inside a `test*` body (`monkeypatch.setattr(...)`) cannot change
//! what it saw. The module's `pytest` import then stays settled and the
//! decorated argnames read as parameters, not fixtures (#11018, pallets/click's
//! `test_formatting.py`). A writer that can run before the decorator keeps the
//! `unresolved_pytest_fixture` limit.

use super::fixtures_tests::workspace;
use super::*;
use std::path::Path;

const SHOP: &str = "def total(amount):\n    return amount * 2\n";

const HEAD: &str = "import os\nimport pytest\n\nfrom shop import total\n\n\n";

const PARAMETRIZED: &str = "@pytest.mark.parametrize((\"amount\", \"expected\"), [(3, 6), (4, 8)])\ndef test_total(amount, expected):\n    assert total(amount) == expected\n";

const PATCHING_TEST: &str = "\n\ndef test_separator(monkeypatch):\n    monkeypatch.setattr(\"os.sep\", \"/\")\n    assert os.sep == \"/\"\n";

fn classify_total(test: &str) -> Result<Finding, String> {
    let (owners, tests) = workspace(&[("shop/__init__.py", SHOP), ("tests/test_shop.py", test)]);
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
fn a_setattr_inside_a_test_body_does_not_unsettle_the_decorator() -> Result<(), String> {
    let control = classify_total(&format!("{HEAD}{PARAMETRIZED}"))?;
    assert_eq!(control.static_limit_kind, None, "{:?}", control.evidence);
    let patched = classify_total(&format!("{HEAD}{PARAMETRIZED}{PATCHING_TEST}"))?;
    assert_eq!(patched.static_limit_kind, None, "{:?}", patched.evidence);
    assert_eq!(patched.class, control.class);
    let async_patched = classify_total(&format!(
        "{HEAD}{PARAMETRIZED}{}",
        PATCHING_TEST.replace("def test_separator", "async def test_separator")
    ))?;
    assert_eq!(
        async_patched.static_limit_kind, None,
        "{:?}",
        async_patched.evidence
    );
    let in_class = classify_total(&format!(
        "{HEAD}{PARAMETRIZED}\n\nclass TestSep:\n    def test_separator(self, monkeypatch):\n        monkeypatch.setattr(\"os.sep\", \"/\")\n        assert os.sep == \"/\"\n"
    ))?;
    assert_eq!(in_class.static_limit_kind, None, "{:?}", in_class.evidence);
    Ok(())
}

#[test]
fn a_writer_that_can_run_before_the_decorator_keeps_the_limit() -> Result<(), String> {
    let cases = [
        (
            "module-scope setattr",
            format!("{HEAD}setattr(pytest, \"mark\", None)\n\n\n{PARAMETRIZED}"),
        ),
        (
            "module-scope globals write",
            format!("{HEAD}globals()[\"pytest\"] = None\n\n\n{PARAMETRIZED}"),
        ),
        (
            "helper called at import",
            format!(
                "{HEAD}def _patch():\n    globals()[\"pytest\"] = None\n\n\n_patch()\n\n\n{PARAMETRIZED}"
            ),
        ),
        (
            "test-named helper called at import",
            format!(
                "{HEAD}def test_setup():\n    globals()[\"pytest\"] = None\n\n\ntest_setup()\n\n\n{PARAMETRIZED}"
            ),
        ),
        (
            "test-named function run by a decorator",
            format!(
                "{HEAD}def run_now(function):\n    function()\n    return function\n\n\n@run_now\ndef test_setup():\n    globals()[\"pytest\"] = None\n\n\n{PARAMETRIZED}"
            ),
        ),
        (
            "writer in a decorator argument",
            format!(
                "{HEAD}@pytest.mark.parametrize((\"amount\", \"expected\"), [(setattr(pytest, \"x\", 1), 6)])\ndef test_total(amount, expected):\n    assert total(amount) == expected\n"
            ),
        ),
    ];
    for (case, test) in cases {
        assert_limited(&classify_total(&test)?, case);
    }
    Ok(())
}
