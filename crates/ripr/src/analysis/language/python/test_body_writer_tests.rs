//! A namespace writer inside a test body runs after every decorator was
//! applied, so it does not unsettle the parametrize argnames of other tests
//! in the module (#11018). Every other writer position keeps the limit.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const PARAMETRIZED: &str = "@pytest.mark.parametrize((\"amount\", \"expected\"), [(5, 5), (50, 40)])\ndef test_apply_discount(amount, expected):\n    assert apply_discount(amount, 10) == expected\n";

fn classify_module(module_text: &str) -> Result<Finding, String> {
    let owners = extract_owners(
        Path::new("src/pricing.py"),
        "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
    );
    let tests = extract_tests(Path::new("tests/test_pricing.py"), module_text);
    assert!(
        tests.iter().any(|test| test.name == "test_apply_discount"
            && test.direct_parametrized == ["amount", "expected"]),
        "fixture must parse the parametrized test: {tests:?}"
    );
    classify_change(
        Path::new("src/pricing.py"),
        2,
        "    if amount >= threshold:",
        &owners,
        &tests,
    )
    .ok_or_else(|| "changed predicate inside owner should classify".to_string())
}

fn module_with(other: &str) -> String {
    format!("import pytest\nfrom src.pricing import apply_discount\n\n{PARAMETRIZED}\n{other}")
}

fn is_fixture_limited(finding: &Finding) -> bool {
    finding.static_limit_kind == Some(StaticLimitKind::UnresolvedPytestFixture)
}

#[test]
fn a_writer_inside_a_test_body_does_not_unsettle_the_argnames() -> Result<(), String> {
    for other in [
        "def test_isatty(monkeypatch):\n    monkeypatch.setattr(object(), \"isatty\", lambda: True)\n",
        "async def test_later():\n    setattr(object(), \"x\", 1)\n",
        "class TestCompat:\n    def test_reload(self):\n        globals()[\"x\"] = 1\n",
    ] {
        let finding = classify_module(&module_with(other))?;
        assert!(
            !is_fixture_limited(&finding),
            "{other:?}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn a_writer_anywhere_else_keeps_the_limit() -> Result<(), String> {
    for other in [
        // Module scope.
        "setattr(object(), \"x\", 1)\n",
        "globals()[\"pytest\"] = object()\n",
        // A non-test helper, which module code may call at import.
        "def helper():\n    setattr(object(), \"x\", 1)\n",
        // A class body and a non-test class's test-named method.
        "class Holder:\n    setattr(object(), \"x\", 1)\n",
        "class Helper:\n    def test_x(self):\n        setattr(object(), \"x\", 1)\n",
        // A decorator argument, which runs at import.
        "@pytest.mark.skipif(setattr(object(), \"x\", 1) is None, reason=\"r\")\ndef test_other():\n    pass\n",
        // A fixture.
        "@pytest.fixture\ndef fix():\n    setattr(object(), \"x\", 1)\n",
        // A test-named function that is not a test body at module level.
        "test_value = vars()\n",
    ] {
        let finding = classify_module(&module_with(other))?;
        assert!(
            is_fixture_limited(&finding),
            "{other:?} must keep the limit: {:?}",
            finding.evidence
        );
    }
    Ok(())
}
