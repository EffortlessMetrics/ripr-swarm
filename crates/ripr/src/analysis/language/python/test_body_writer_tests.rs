//! A namespace writer inside a test body runs after every decorator was
//! applied, so it does not unsettle the parametrize argnames of other tests
//! in the module (#11018). An undecorated helper nothing names at module scope is the same
//! (#11144). Every other writer position keeps the limit.

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
        "class TestCompat:\n    async def test_reload(self):\n        globals()[\"x\"] = 1\n",
        // An undecorated helper only runs when something calls it (#11144).
        "def _build(**kw):\n    for k, v in kw.items():\n        setattr(object(), k, v)\n",
        "def _drive(monkeypatch):\n    monkeypatch.setattr(object(), \"isatty\", lambda: True)\n\n\ndef test_uses(monkeypatch):\n    _drive(monkeypatch)\n",
        "async def _drive():\n    globals()[\"x\"] = 1\n",
        // Two unreferenced helpers sharing a name only declare it.
        "def _twin():\n    globals()[\"x\"] = 1\n\n\ndef _twin():\n    return 1\n",
        // Naming one helper at import keeps only that helper's body.
        "def _a():\n    globals()[\"x\"] = 1\n\n\ndef _b():\n    return 1\n\n\nrun(_b)\n",
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
        // A helper that module code calls at import, aliases or decorates with.
        "def helper():\n    setattr(object(), \"x\", 1)\n\n\nhelper()\n",
        "def helper():\n    setattr(object(), \"x\", 1)\n\n\nalias = helper\n",
        "def helper(f):\n    setattr(object(), \"x\", 1)\n    return f\n\n\n@helper\ndef test_wrapped():\n    pass\n",
        "async def helper():\n    setattr(object(), \"x\", 1)\n\n\nrun(helper)\n",
        // A kept helper's body names a second helper, which is kept as well.
        "def inner():\n    setattr(object(), \"x\", 1)\n\n\ndef outer():\n    inner()\n\n\nouter()\n",
        // A helper defined twice and called at import keeps both bodies.
        "def _twin():\n    globals()[\"x\"] = 1\n\n\ndef _twin():\n    return 1\n\n\n_twin()\n",
        // A comment naming a helper keeps it too (the safe direction).
        "def _quiet():\n    setattr(object(), \"x\", 1)\n\n\n# _quiet\n",
        // A decorated helper may be invoked by its decorator.
        "@register\ndef helper():\n    setattr(object(), \"x\", 1)\n",
        // A class body and a non-test class's test-named method.
        "class Holder:\n    setattr(object(), \"x\", 1)\n",
        "class Helper:\n    def test_x(self):\n        setattr(object(), \"x\", 1)\n",
        // A decorator argument, which runs at import.
        "@pytest.mark.skipif(setattr(object(), \"x\", 1) is None, reason=\"r\")\ndef test_other():\n    pass\n",
        // A fixture.
        "@pytest.fixture\ndef fix():\n    setattr(object(), \"x\", 1)\n",
        // Module-level assignment of a writer result.
        "test_value = vars()\n",
        // Writers in a default argument and in `Test*` class-level code.
        "def test_default(x=setattr(object(), \"x\", 1)):\n    pass\n",
        "class TestHolder:\n    setattr(object(), \"x\", 1)\n",
        // A test-named fixture runs when requested, not as a test.
        "@pytest.fixture\ndef test_fix():\n    setattr(object(), \"x\", 1)\n",
        // A test function called at import, or used as a decorator.
        "def test_helper():\n    setattr(object(), \"x\", 1)\n\n\ntest_helper()\n",
        "def test_wrap(f):\n    globals()[\"pytest\"] = f\n    return f\n\n\n@test_wrap\ndef test_wrapped():\n    pass\n",
    ] {
        let finding = classify_module(&module_with(other))?;
        // Module-scope substitution forms (`fixture_substitution.rs`) may also
        // speak as the stronger `mocked_module` limit; either withholds credit.
        assert!(
            is_fixture_limited(&finding)
                || finding.static_limit_kind == Some(StaticLimitKind::MockedModule),
            "{other:?} must keep the limit: {:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn call_time_constants_keep_the_whole_module_scan() {
    // A writer in a test-named function rewrites a constant a production
    // function reads when it runs, so only the import-time read is relaxed.
    let count = |source: &str| {
        extract_owners(Path::new("src/pricing.py"), source)
            .iter()
            .flat_map(|owner| owner.module_constants.iter())
            .count()
    };
    // Object and table bindings read at call time keep the scan as well.
    let bindings = |source: &str| {
        let parsed = super::source_facts::parse_module_result(Path::new("m.py"), source);
        match parsed {
            Ok(rustpython_parser::ast::Mod::Module(module)) => {
                let text = super::source_utils::SourceText::new(source);
                super::module_constants::module_object_bindings(&text, &module.body).len()
            }
            _ => usize::MAX,
        }
    };
    let object = "CONFIG = make()\n\n\ndef testbed():\n    globals()[\"CONFIG\"] = 0\n";
    assert_eq!(bindings(object), 0);
    assert_eq!(
        bindings(&object.replace("globals()[\"CONFIG\"] = 0", "return 1")),
        1
    );
    let quiet = "DISCOUNT_THRESHOLD = 10\n\n\ndef testbed_reset():\n    return 1\n";
    assert!(count(quiet) > 0, "control: the constant is visible");
    let writing = "DISCOUNT_THRESHOLD = 10\n\n\ndef testbed_reset():\n    globals()[\"DISCOUNT_THRESHOLD\"] = 0\n";
    assert_eq!(count(writing), 0);
}
