//! #10707: a boundary input held in a constant of the test's own module places
//! the input like the literal would, and every way that constant can differ at
//! the call keeps the finding `weakly_exposed`.

use super::module_constants::apply_foreign_module_attribute_writes;
use super::owners_tests::{extract_owners, extract_tests};
use super::source_facts::extract_source_facts;
use super::*;
use std::path::{Path, PathBuf};

const OWNER_NEW: &str =
    "def tier(score):\n    if score >= 50:\n        return \"gold\"\n    return \"basic\"\n";
const OLD_LINE: &str = "    if score > 50:";
const NEW_LINE: &str = "    if score >= 50:";

fn class_of(test_source: &str) -> Result<ExposureClass, String> {
    class_of_with_conftest(test_source, None)
}

/// Classify with optional `tests/conftest.py` writes applied the way the
/// workspace pass joins them onto the test (#11424).
fn class_of_with_conftest(
    test_source: &str,
    conftest: Option<&str>,
) -> Result<ExposureClass, String> {
    let owners = extract_owners(Path::new("src/pricing.py"), OWNER_NEW);
    let mut tests = extract_tests(Path::new("tests/test_pricing.py"), test_source);
    if let Some(conftest) = conftest {
        let facts = extract_source_facts(Path::new("tests/conftest.py"), conftest);
        apply_foreign_module_attribute_writes(
            &mut tests,
            &[(
                PathBuf::from("tests/conftest.py"),
                facts.module_attribute_writes,
            )],
        );
    }
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

#[test]
fn a_conftest_that_rebinds_the_constant_never_reads_exposed() -> Result<(), String> {
    let test = module("TH = 50", CALL);
    for conftest in [
        "import tests.test_pricing as module\n\nmodule.TH = 49\n",
        "from tests import test_pricing\n\ntest_pricing.TH = 49\n",
        "import tests.test_pricing as module\n\nsetattr(module, \"TH\", 49)\n",
        "import tests.test_pricing as module\n\ndef rebind(monkeypatch):\n    monkeypatch.setattr(module, \"TH\", 49)\n",
        "def rebind(monkeypatch):\n    monkeypatch.setattr(\"tests.test_pricing.TH\", 49)\n",
    ] {
        assert_ne!(
            class_of_with_conftest(&test, Some(conftest))?,
            ExposureClass::Exposed,
            "{conftest:?}"
        );
    }
    Ok(())
}

#[test]
fn an_unrelated_conftest_keeps_the_constant_credit() -> Result<(), String> {
    let test = module("TH = 50", CALL);
    assert_eq!(
        class_of(&test)?,
        ExposureClass::Exposed,
        "no conftest is the baseline credit"
    );
    for conftest in [
        "import other\n\nother.LIMIT = 1\n",
        "import tests.test_other as other\n\nother.TH = 49\n",
        "import tests.test_pricing as module\n\nmodule.OTHER = 1\n",
    ] {
        assert_eq!(
            class_of_with_conftest(&test, Some(conftest))?,
            ExposureClass::Exposed,
            "{conftest:?}"
        );
    }
    Ok(())
}
