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

fn assert_limit_kind(finding: &Finding, case: &str, expected: StaticLimitKind) {
    assert_eq!(
        finding.static_limit_kind,
        Some(expected),
        "{case}: {:?} {:?}",
        finding.static_limit_kind,
        finding.evidence
    );
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
            finding.static_limit_kind != Some(StaticLimitKind::UnresolvedPytestFixture),
            "{other:?}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn a_writer_anywhere_else_keeps_the_limit() -> Result<(), String> {
    for (case, other, expected) in [
        (
            "module-scope setattr",
            "setattr(object(), \"x\", 1)\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "module-scope globals write",
            "globals()[\"pytest\"] = object()\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "helper called at import",
            "def helper():\n    setattr(object(), \"x\", 1)\n\n\nhelper()\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "helper aliased at import",
            "def helper():\n    setattr(object(), \"x\", 1)\n\n\nalias = helper\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "decorated helper",
            "def helper(f):\n    setattr(object(), \"x\", 1)\n    return f\n\n\n@helper\ndef test_wrapped():\n    pass\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "async helper scheduled at import",
            "async def helper():\n    setattr(object(), \"x\", 1)\n\n\nrun(helper)\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "nested helpers called at import",
            "def inner():\n    setattr(object(), \"x\", 1)\n\n\ndef outer():\n    inner()\n\n\nouter()\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "duplicate helper bodies called at import",
            "def _twin():\n    globals()[\"x\"] = 1\n\n\ndef _twin():\n    return 1\n\n\n_twin()\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "comment naming a helper",
            "def _quiet():\n    setattr(object(), \"x\", 1)\n\n\n# _quiet\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "register-decorated helper",
            "@register\ndef helper():\n    setattr(object(), \"x\", 1)\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "class body writer",
            "class Holder:\n    setattr(object(), \"x\", 1)\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "non-test class method writer",
            "class Helper:\n    def test_x(self):\n        setattr(object(), \"x\", 1)\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "decorator-argument writer",
            "@pytest.mark.skipif(setattr(object(), \"x\", 1) is None, reason=\"r\")\ndef test_other():\n    pass\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "fixture body writer",
            "@pytest.fixture\ndef fix():\n    setattr(object(), \"x\", 1)\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "module-level vars() assignment",
            "test_value = vars()\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "default-argument writer",
            "def test_default(x=setattr(object(), \"x\", 1)):\n    pass\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "Test* class body writer",
            "class TestHolder:\n    setattr(object(), \"x\", 1)\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "test-named fixture writer",
            "@pytest.fixture\ndef test_fix():\n    setattr(object(), \"x\", 1)\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "test function called at import",
            "def test_helper():\n    setattr(object(), \"x\", 1)\n\n\ntest_helper()\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
        (
            "test function used as decorator",
            "def test_wrap(f):\n    globals()[\"pytest\"] = f\n    return f\n\n\n@test_wrap\ndef test_wrapped():\n    pass\n",
            StaticLimitKind::UnresolvedPytestFixture,
        ),
    ] {
        assert_limit_kind(&classify_module(&module_with(other))?, case, expected);
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
