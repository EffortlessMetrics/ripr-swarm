//! An exception assertion whose `with` body (or rule 13 callable) calls the
//! changed owner through its module (`levels.parse("")`) observes the changed
//! `raise` like a body that calls the imported name (`parse("")`).
//!
//! RIPR-SPEC-0238 rule 4: an exception assertion compares no operand, so the
//! text its gates read is the call together with its body. The
//! module-qualified credit branch read asserted operands only, so the same
//! test read `exposed` with `from src.levels import parse` and
//! `weakly_exposed` with `from src import levels`, with a next step naming the
//! `match=` check the test already holds.

use super::classify::classify_change_with_old;
use super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

type Check = Result<(), String>;

const OWNER_FILE: &str = "src/levels.py";
const OWNER: &str = "def parse(text):\n    if not text:\n        raise ValueError(\"blank level\")\n    return text.upper()\n\n\ndef other(text):\n    raise ValueError(\"blank level\")\n";
const OLD: &str = "        raise ValueError(\"empty level\")";
const NEW: &str = "        raise ValueError(\"blank level\")";
const MODULE_CALL: &str = "strong_oracle_observes_owner_call_through_module";

/// The finding for the message edit on line 3 of `src/levels.py`, with one
/// test module `tests/test_levels.py` holding `test_source`.
fn classify(test_source: &str) -> Result<Finding, String> {
    classify_owner(OWNER, test_source)
}

fn classify_owner(owner_source: &str, test_source: &str) -> Result<Finding, String> {
    let file = Path::new(OWNER_FILE);
    let owners = extract_owners(file, owner_source);
    let tests = extract_tests(Path::new("tests/test_levels.py"), test_source);
    if tests.is_empty() {
        return Err(format!("fixture must hold a test: {test_source}"));
    }
    classify_change_with_old(file, 3, NEW, Some(OLD), &owners, &tests)
        .ok_or_else(|| "the changed raise must yield a finding".to_string())
}

fn expect_module_call_credit(label: &str, test_source: &str) -> Check {
    let finding = classify(test_source)?;
    if finding.class != ExposureClass::Exposed
        || finding.alignment_reason.as_deref() != Some(MODULE_CALL)
    {
        return Err(format!(
            "{label}: expected exposed through {MODULE_CALL}, got {:?} / {:?}",
            finding.class, finding.alignment_reason
        ));
    }
    Ok(())
}

fn expect_no_credit(label: &str, test_source: &str) -> Check {
    let finding = classify(test_source)?;
    if finding.class == ExposureClass::Exposed {
        return Err(format!(
            "{label}: must not read exposed, got {:?}",
            finding.alignment_reason
        ));
    }
    Ok(())
}

#[test]
fn imported_name_in_a_raises_body_is_the_baseline() -> Check {
    let finding = classify(
        "import pytest\nfrom src.levels import parse\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        parse(\"\")\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(
        finding.alignment_reason.as_deref(),
        Some("strong_oracle_observes_owner_name")
    );
    Ok(())
}

#[test]
fn module_qualified_call_in_a_raises_body_credits_the_changed_raise() -> Check {
    expect_module_call_credit(
        "from-import of the module",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        levels.parse(\"\")\n",
    )?;
    expect_module_call_credit(
        "aliased module import",
        "import pytest\nimport src.levels as lv\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        lv.parse(\"\")\n",
    )?;
    expect_module_call_credit(
        "dotted module import",
        "import pytest\nimport src.levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        src.levels.parse(\"\")\n",
    )?;
    expect_module_call_credit(
        "unittest context manager",
        "import unittest\nfrom src import levels\n\n\nclass LevelCase(unittest.TestCase):\n    def test_blank(self):\n        with self.assertRaisesRegex(ValueError, \"blank level\"):\n            levels.parse(\"\")\n",
    )?;
    expect_module_call_credit(
        "the word lambda in a string defers nothing",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        note = \"lambda\"\n        levels.parse(\"\")\n",
    )?;
    expect_module_call_credit(
        "nested call still runs the owner first",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        print(levels.parse(\"\"))\n",
    )
}

#[test]
fn module_qualified_call_under_a_bound_exception_comparison_credits() -> Check {
    expect_module_call_credit(
        "rule 14 value comparison",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError) as excinfo:\n        levels.parse(\"\")\n    assert str(excinfo.value) == \"blank level\"\n",
    )
}

#[test]
fn one_letter_owner_in_a_raises_body_credits() -> Check {
    // A one-letter name is no identity token, so only the module-identified
    // call branch can credit it.
    let owner = "def f(text):\n    if not text:\n        raise ValueError(\"blank level\")\n    return text.upper()\n";
    let finding = classify_owner(
        owner,
        "import pytest\nfrom src.levels import f\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        f(\"\")\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.alignment_reason.as_deref(), Some(MODULE_CALL));
    // The rule 13 callable form calls the owner the same way.
    let finding = classify_owner(
        owner,
        "import unittest\nfrom src.levels import f\n\n\nclass LevelCase(unittest.TestCase):\n    def test_blank(self):\n        self.assertRaisesRegex(ValueError, \"blank level\", f, \"\")\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.alignment_reason.as_deref(), Some(MODULE_CALL));
    Ok(())
}

#[test]
fn a_raises_body_that_does_not_run_the_owner_stays_uncredited() -> Check {
    expect_no_credit(
        "another function of the module",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        levels.other(\"\")\n",
    )?;
    expect_no_credit(
        "owner called only before the with",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    levels.parse(\"x\")\n    with pytest.raises(ValueError, match=\"blank level\"):\n        levels.other(\"\")\n",
    )?;
    expect_no_credit(
        "same-named module of another package",
        "import pytest\nfrom vendor import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        levels.parse(\"\")\n",
    )?;
    expect_no_credit(
        "module name rebound in the test",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    levels = Fake()\n    with pytest.raises(ValueError, match=\"blank level\"):\n        levels.parse(\"\")\n",
    )?;
    expect_no_credit(
        "call deferred by a lambda",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        check = lambda: levels.parse(\"\")\n        levels.other(\"\")\n",
    )?;
    expect_no_credit(
        "call deferred by a nested def",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        def check():\n            levels.parse(\"\")\n        levels.other(\"\")\n",
    )?;
    expect_no_credit(
        "owner called only in the comparison after the with",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError) as excinfo:\n        levels.other(\"\")\n    assert str(excinfo.value) == levels.parse(\"blank level\").lower()\n",
    )?;
    expect_no_credit(
        "call spelled in a triple-quoted string",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    levels.parse(\"ok\")\n    with pytest.raises(ValueError, match=\"blank level\"):\n        note = \"\"\"\n        levels.parse(\"\")\n        \"\"\"\n        levels.other(\"\")\n",
    )?;
    expect_no_credit(
        "call named only in a comment",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"blank level\"):\n        # levels.parse(\"\")\n        levels.other(\"\")\n",
    )
}

#[test]
fn a_pattern_matching_both_messages_still_pins_only_the_class() -> Check {
    // The #9185 message-pin gate reads the same assertion: `level` matches
    // the old and the new message alike.
    expect_no_credit(
        "shared-substring pattern",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError, match=\"level\"):\n        levels.parse(\"\")\n",
    )
}

#[test]
fn a_class_only_raises_stays_a_broad_error() -> Check {
    expect_no_credit(
        "no match pattern",
        "import pytest\nfrom src import levels\n\n\ndef test_blank():\n    with pytest.raises(ValueError):\n        levels.parse(\"\")\n",
    )
}

#[test]
fn a_value_assertion_on_a_module_call_still_needs_an_asserted_operand() -> Check {
    // The body reading is for exception assertions only: a value assertion
    // keeps the operand-depth rule (#4567).
    let owner = "def parse(text):\n    if not text:\n        raise ValueError(\"blank level\")\n    return text.lower()\n";
    let file = Path::new(OWNER_FILE);
    let owners = extract_owners(file, owner);
    let tests = extract_tests(
        Path::new("tests/test_levels.py"),
        "from src import levels\n\n\ndef test_wrapped():\n    assert always_true(levels.parse(\"A\")) == True\n",
    );
    let finding = classify_change_with_old(
        file,
        4,
        "    return text.lower()",
        Some("    return text.upper()"),
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed return must yield a finding".to_string())?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}
