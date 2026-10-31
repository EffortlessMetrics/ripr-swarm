//! #10707: a boundary input held in a constant of the test's own module places
//! the input like the literal would, and every way that constant can differ at
//! the call keeps the finding `weakly_exposed`.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_NEW: &str =
    "def tier(score):\n    if score >= 50:\n        return \"gold\"\n    return \"basic\"\n";
const OLD_LINE: &str = "    if score > 50:";
const NEW_LINE: &str = "    if score >= 50:";

fn class_of(test_source: &str) -> Result<ExposureClass, String> {
    let owners = extract_owners(Path::new("src/pricing.py"), OWNER_NEW);
    let tests = extract_tests(Path::new("tests/test_pricing.py"), test_source);
    classify_change_with_old(
        Path::new("src/pricing.py"),
        2,
        NEW_LINE,
        Some(OLD_LINE),
        &owners,
        &tests,
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "expected a finding".to_string())
}

/// A test module with `module_lines` above one boundary test whose body is
/// `statements`.
fn module(module_lines: &str, statements: &str) -> String {
    format!(
        "from src.pricing import tier\n{module_lines}\n\ndef test_gold():\n    assert tier(80) == \"gold\"\n\n\ndef test_edge():\n{statements}\n"
    )
}

const CALL: &str = "    assert tier(TH) == \"basic\"";

#[test]
fn a_module_constant_places_the_boundary_input() -> Result<(), String> {
    assert_eq!(class_of(&module("TH = 50", CALL))?, ExposureClass::Exposed);
    assert_eq!(
        class_of(&module("TH = 50", "    assert tier(score=TH) == \"basic\""))?,
        ExposureClass::Exposed
    );
    // The constant may sit after the tests that read it: it is bound by the
    // time they run.
    assert_eq!(
        class_of(&format!("{}\nTH = 50\n", module("", CALL)))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_constant_that_is_not_the_boundary_value_does_not_credit() -> Result<(), String> {
    assert_eq!(
        class_of(&module("TH = 49", CALL))?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn every_other_way_the_constant_can_differ_never_reads_exposed() -> Result<(), String> {
    for (module_lines, statements) in [
        // Bound twice, rebound or augmented at module scope.
        ("TH = 50\nTH = 49", CALL),
        ("TH = 50\nTH += 1", CALL),
        ("TH = 50\nfor TH in (1, 2):\n    pass", CALL),
        ("TH = 50\ndef TH():\n    pass", CALL),
        ("TH = 50\nimport os as TH", CALL),
        // Not one top-level literal assignment.
        ("if True:\n    TH = 50", CALL),
        ("TH = 25 * 2", CALL),
        ("TH = int('50')", CALL),
        ("from limits import TH", CALL),
        // Another function can rebind it before this test runs.
        ("TH = 50\n\ndef set_it():\n    global TH\n    TH = 60", CALL),
        // The module writes names dynamically or assigns the attribute.
        ("TH = 50\nglobals()['TH'] = 60", CALL),
        ("import tests.test_pricing as me\nTH = 50\nme.TH = 60", CALL),
        // Code that runs outside the test body.
        (
            "import pytest\nTH = 50\n\n@pytest.fixture(autouse=True)\ndef reset():\n    pass",
            CALL,
        ),
        ("TH = 50\n\ndef setup_module():\n    pass", CALL),
        (
            "import pytest\npytestmark = pytest.mark.slow\nTH = 50",
            CALL,
        ),
        // The test itself binds or replaces the name.
        ("TH = 50", "    TH = 60\n    assert tier(TH) == \"basic\""),
        (
            "TH = 50",
            "    global TH\n    TH = 60\n    assert tier(TH) == \"basic\"",
        ),
        ("TH = 50", "    TH += 1\n    assert tier(TH) == \"basic\""),
        (
            "TH = 50",
            "    for TH in (60,):\n        pass\n    assert tier(TH) == \"basic\"",
        ),
        (
            "TH = 50",
            "    assert (lambda TH: tier(TH))(60) == \"gold\"",
        ),
        (
            "TH = 50",
            "    import src.pricing as m\n    m.TH = 60\n    assert tier(TH) == \"basic\"",
        ),
        // A comprehension target is a name of its own scope.
        (
            "TH = 50",
            "    assert [tier(TH) for TH in (60,)] == [\"gold\"]",
        ),
        (
            "TH = 50",
            "    assert list(tier(TH) for TH in (60,)) == [\"gold\"]",
        ),
        // The test's own text patches or reflects over the module.
        (
            "TH = 50",
            "    monkeypatch.setattr('tests.test_pricing.TH', 60)\n    assert tier(TH) == \"basic\"",
        ),
        (
            "TH = 50",
            "    globals()['TH'] = 60\n    assert tier(TH) == \"basic\"",
        ),
        (
            "TH = 50",
            "    exec('TH = 60')\n    assert tier(TH) == \"basic\"",
        ),
    ] {
        // A static limit such as a patched module reads `static_unknown`; the
        // contract is that none of these shapes reads `exposed`.
        assert_ne!(
            class_of(&module(module_lines, statements))?,
            ExposureClass::Exposed,
            "{module_lines:?} / {statements:?}"
        );
    }
    Ok(())
}

#[test]
fn a_parameter_of_the_test_is_not_the_module_constant() -> Result<(), String> {
    let source = "from src.pricing import tier\n\nTH = 50\n\n\ndef test_gold():\n    assert tier(80) == \"gold\"\n\n\ndef test_edge(TH):\n    assert tier(TH) == \"basic\"\n";
    assert_eq!(class_of(source)?, ExposureClass::WeaklyExposed);
    Ok(())
}
