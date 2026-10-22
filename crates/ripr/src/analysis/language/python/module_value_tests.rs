//! Module-level constant edits and conditional definitions (#9423): credited
//! only where an assertion statically depends on the value, and only through
//! certain bindings.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use super::{PythonOwner, PythonTest};
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

type Check = Result<(), String>;

const SETTINGS: &str = "LIMIT = 12\n\n\ndef over_limit(n):\n    return n > LIMIT\n";
const SETTINGS_OLD: &str = "LIMIT = 10";
const SETTINGS_NEW: &str = "LIMIT = 12";

fn owners_of(source: &str) -> Vec<PythonOwner> {
    extract_owners(Path::new("hostile/settings.py"), source)
}

fn tests_of(files: &[(&str, &str)]) -> Vec<PythonTest> {
    files
        .iter()
        .flat_map(|(file, source)| extract_tests(Path::new(file), source))
        .collect()
}

/// Classify the edit of line `line` of `source` (new text `new`, old text
/// `old`) against the test files, as the diff pipeline does for a lone line
/// replacement.
fn classify_edit(
    source: &str,
    line: usize,
    new: &str,
    old: &str,
    test_files: &[(&str, &str)],
    sole: bool,
) -> Result<Finding, String> {
    let owners = owners_of(source);
    let tests = tests_of(test_files);
    let context = PythonNoBehaviorContext {
        sole_line_replacement: sole,
        ..PythonNoBehaviorContext::default()
    };
    classify_change_with_context(
        Path::new("hostile/settings.py"),
        line,
        new,
        Some(old),
        &owners,
        &tests,
        context,
        Some(source),
    )
    .ok_or_else(|| "the edit must yield a finding".to_string())
}

fn classify_limit(test_source: &str) -> Result<Finding, String> {
    classify_edit(
        SETTINGS,
        1,
        SETTINGS_NEW,
        SETTINGS_OLD,
        &[("tests/test_settings.py", test_source)],
        true,
    )
}

fn limit_test(body: &str) -> String {
    format!("from hostile.settings import over_limit\n\n\ndef test_it():\n{body}\n")
}

fn assert_credited(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("related_test_relation: module_value_call")),
        "{:?}",
        finding.evidence
    );
}

fn assert_abstains(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("missing_import_graph")),
        "{:?}",
        finding.evidence
    );
}

#[test]
fn constant_read_by_a_called_function_with_a_pinning_literal_is_credited() -> Check {
    // 11 > 10 holds, 11 > 12 fails: the assertion notices the edit.
    assert_credited(&classify_limit(&limit_test("    assert over_limit(11)"))?);
    assert_credited(&classify_limit(&limit_test(
        "    assert over_limit(11) == True",
    ))?);
    Ok(())
}

#[test]
fn the_credited_row_names_the_value_read() -> Check {
    let finding = classify_limit(&limit_test("    assert over_limit(11)"))?;
    assert!(
        finding.evidence.iter().any(
            |line| line.starts_with("module_value_read: `LIMIT` 10 -> 12 through `over_limit`")
        ),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_value_no_assertion_distinguishes_abstains() -> Check {
    // 11 > 9 and 5 > 9 give the same answers as 11 > 10 and 5 > 10.
    let finding = classify_edit(
        "LIMIT = 9\n\n\ndef over_limit(n):\n    return n > LIMIT\n",
        1,
        "LIMIT = 9",
        SETTINGS_OLD,
        &[(
            "tests/test_settings.py",
            &limit_test("    assert over_limit(11)\n    assert not over_limit(5)"),
        )],
        true,
    )?;
    assert_abstains(&finding);
    // An assertion on the far side of both values does not notice it either.
    assert_abstains(&classify_limit(&limit_test(
        "    assert not over_limit(5)",
    ))?);
    assert_abstains(&classify_limit(&limit_test("    assert over_limit(100)"))?);
    // An assertion that already fails under the old value, or that the edit
    // would only make pass, is not a discriminator of the edit.
    assert_abstains(&classify_limit(&limit_test("    assert over_limit(5)"))?);
    assert_abstains(&classify_limit(&limit_test(
        "    assert not over_limit(11)",
    ))?);
    Ok(())
}

#[test]
fn constant_read_directly_by_an_exact_assertion_is_credited() -> Check {
    let test = "from hostile.settings import LIMIT\n\n\ndef test_it():\n    assert LIMIT == 10\n";
    assert_credited(&classify_limit(test)?);
    let attribute =
        "from hostile import settings\n\n\ndef test_it():\n    assert settings.LIMIT == 10\n";
    assert_credited(&classify_limit(attribute)?);
    // An assertion the new value also satisfies does not.
    assert_abstains(&classify_limit(
        "from hostile.settings import LIMIT\n\n\ndef test_it():\n    assert LIMIT > 5\n",
    )?);
    Ok(())
}

#[test]
fn a_module_attribute_call_in_a_test_class_is_credited() -> Check {
    let test = "from hostile import settings\n\n\nclass TestLimit:\n    def test_above(self):\n        assert settings.over_limit(11)\n\n    def test_below(self):\n        assert not settings.over_limit(5)\n";
    let finding = classify_limit(test)?;
    assert_credited(&finding);
    // The credited row is the discriminating assertion, not the other one.
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("fails `assert settings.over_limit(11)` (test_above)")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_local_shadow_of_the_constant_is_not_the_module_value() -> Check {
    for test in [
        "from hostile.settings import LIMIT\n\n\ndef test_it():\n    LIMIT = 10\n    assert LIMIT == 10\n",
        "from hostile.settings import LIMIT\n\n\ndef test_it(LIMIT):\n    assert LIMIT == 10\n",
        "from hostile.settings import LIMIT\n\nLIMIT = 10\n\n\ndef test_it():\n    assert LIMIT == 10\n",
    ] {
        let finding = classify_limit(test)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{test}");
    }
    Ok(())
}

#[test]
fn constant_read_through_arithmetic_and_strings_is_credited() -> Check {
    let rate = "RATE = 6\n\n\ndef fee(x):\n    \"\"\"The fee.\"\"\"\n    return x * RATE\n";
    let finding = classify_edit(
        rate,
        1,
        "RATE = 6",
        "RATE = 5",
        &[(
            "tests/test_settings.py",
            "from hostile.settings import fee\n\n\ndef test_it():\n    assert fee(4) == 20\n",
        )],
        true,
    )?;
    assert_credited(&finding);
    let greeting = "GREETING = \"hello \"\n\n\ndef greet(name):\n    return GREETING + name\n";
    let finding = classify_edit(
        greeting,
        1,
        "GREETING = \"hello \"",
        "GREETING = \"hi \"",
        &[(
            "tests/test_settings.py",
            "from hostile.settings import greet\n\n\ndef test_it():\n    assert greet('x') == 'hi x'\n",
        )],
        true,
    )?;
    assert_credited(&finding);
    Ok(())
}

#[test]
fn the_edit_must_be_the_files_only_changed_line() -> Check {
    let finding = classify_edit(
        SETTINGS,
        1,
        SETTINGS_NEW,
        SETTINGS_OLD,
        &[(
            "tests/test_settings.py",
            &limit_test("    assert over_limit(11)"),
        )],
        false,
    )?;
    assert_abstains(&finding);
    Ok(())
}

#[test]
fn a_test_that_rebinds_the_function_or_constant_is_not_credited() -> Check {
    for test in [
        // A parameter, a local and a second module binding shadow the import.
        "from hostile.settings import over_limit\n\n\ndef test_it(over_limit):\n    assert over_limit(11)\n",
        "from hostile.settings import over_limit\n\n\ndef test_it():\n    over_limit = lambda n: True\n    assert over_limit(11)\n",
        "from hostile.settings import over_limit\n\n\ndef over_limit(n):\n    return True\n\n\ndef test_it():\n    assert over_limit(11)\n",
        // Writing the constant, or patching it, changes what the function reads.
        "from hostile import settings\nfrom hostile.settings import over_limit\n\n\ndef test_it():\n    settings.LIMIT = 5\n    assert over_limit(11)\n",
        "from hostile.settings import over_limit\n\n\ndef test_it(monkeypatch):\n    monkeypatch.setattr('hostile.settings.LIMIT', 5)\n    assert over_limit(11)\n",
        "from unittest.mock import patch\nfrom hostile.settings import over_limit\n\n\ndef test_it():\n    with patch('hostile.settings.LIMIT', 5):\n        assert over_limit(11)\n",
        // A different module's function of the same name.
        "from other.settings import over_limit\n\n\ndef test_it():\n    assert over_limit(11)\n",
    ] {
        let finding = classify_limit(test)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{test}");
    }
    Ok(())
}

#[test]
fn another_test_file_writing_the_constant_withholds_every_credit() -> Check {
    let finding = classify_edit(
        SETTINGS,
        1,
        SETTINGS_NEW,
        SETTINGS_OLD,
        &[
            (
                "tests/test_settings.py",
                &limit_test("    assert over_limit(11)"),
            ),
            (
                "tests/test_other.py",
                "from hostile import settings\n\n\ndef test_it():\n    settings.LIMIT = 3\n",
            ),
        ],
        true,
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_assertion_that_may_not_run_or_decide_is_not_credited() -> Check {
    for body in [
        // Not a direct statement of the body.
        "    if flag:\n        assert over_limit(11)",
        "    with ctx():\n        assert over_limit(11)",
        "    for n in [11]:\n        assert over_limit(11)",
        // Something earlier can leave the test.
        "    return\n    assert over_limit(11)",
        "    pytest.skip('later')\n    assert over_limit(11)",
        "    raise RuntimeError\n    assert over_limit(11)",
        // The argument is not a literal.
        "    n = 11\n    assert over_limit(n)",
        "    assert over_limit(compute())",
        // A keyword call is not modeled.
        "    assert over_limit(n=11)",
    ] {
        let finding = classify_limit(&limit_test(body))?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn a_skipped_test_is_not_credited() -> Check {
    let test = "import pytest\nfrom hostile.settings import over_limit\n\n\n@pytest.mark.skip\ndef test_it():\n    assert over_limit(11)\n";
    let finding = classify_limit(test)?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_constant_the_module_can_rebind_is_not_read() -> Check {
    for source in [
        // Reassigned.
        "LIMIT = 10\nLIMIT = 12\n\n\ndef over_limit(n):\n    return n > LIMIT\n",
        // Declared `global` in a function.
        "LIMIT = 12\n\n\ndef bump():\n    global LIMIT\n    LIMIT += 1\n\n\ndef over_limit(n):\n    return n > LIMIT\n",
        // A star import can bind it.
        "from os.path import *\n\nLIMIT = 12\n\n\ndef over_limit(n):\n    return n > LIMIT\n",
        // A dynamic namespace writer can bind it.
        "LIMIT = 12\nglobals()['LIMIT'] = 3\n\n\ndef over_limit(n):\n    return n > LIMIT\n",
        // An augmented assignment.
        "LIMIT = 12\nLIMIT += 0\n\n\ndef over_limit(n):\n    return n > LIMIT\n",
    ] {
        let line = source
            .lines()
            .position(|text| text == "LIMIT = 12")
            .map(|index| index + 1)
            .ok_or("fixture needs the changed line")?;
        let finding = classify_edit(
            source,
            line,
            SETTINGS_NEW,
            SETTINGS_OLD,
            &[(
                "tests/test_settings.py",
                &limit_test("    assert over_limit(11)"),
            )],
            true,
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{source}");
    }
    Ok(())
}

#[test]
fn a_constant_defined_conditionally_is_not_read() -> Check {
    let source = "if flag:\n    LIMIT = 12\nelse:\n    LIMIT = 20\n\n\ndef over_limit(n):\n    return n > LIMIT\n";
    let finding = classify_edit(
        source,
        2,
        "    LIMIT = 12",
        "    LIMIT = 10",
        &[(
            "tests/test_settings.py",
            &limit_test("    assert over_limit(11)"),
        )],
        true,
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_reader_that_is_not_a_single_certain_return_is_not_read() -> Check {
    for function in [
        // Two definitions of the function: the second one wins at runtime.
        "def over_limit(n):\n    return n > LIMIT\n\n\ndef over_limit(n):\n    return True\n",
        "def over_limit(n):\n    return n > LIMIT\n\n\nover_limit = wrap(over_limit)\n",
        // A parameter shadows the constant.
        "def over_limit(n, LIMIT=3):\n    return n > LIMIT\n",
        // A local rebinding.
        "def over_limit(n):\n    LIMIT = 3\n    return n > LIMIT\n",
        // More than one statement.
        "def over_limit(n):\n    print(n)\n    return n > LIMIT\n",
        // A decorator can replace the function.
        "@wrap\ndef over_limit(n):\n    return n > LIMIT\n",
        // A call inside the body is not modeled.
        "def over_limit(n):\n    return abs(n) > LIMIT\n",
        // A global name other than the constant.
        "def over_limit(n):\n    return n > LIMIT + OFFSET\n",
        // `*args` binds no positional name.
        "def over_limit(*n):\n    return n[0] > LIMIT\n",
    ] {
        let source = format!("LIMIT = 12\n\n\n{function}");
        let finding = classify_edit(
            &source,
            1,
            SETTINGS_NEW,
            SETTINGS_OLD,
            &[(
                "tests/test_settings.py",
                &limit_test("    assert over_limit(11)"),
            )],
            true,
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{function}");
    }
    Ok(())
}

#[test]
fn star_imported_names_are_not_read_by_a_test() -> Check {
    let test = "from hostile.settings import *\n\n\ndef test_it():\n    assert over_limit(11)\n";
    let finding = classify_limit(test)?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

const VERSIONED: &str = "import sys\n\nif sys.version_info >= (3, 0):\n\n    def versioned():\n        return \"newer\"\n\nelse:\n\n    def versioned():\n        return \"old\"\n";

fn versioned_test(expected: &str) -> String {
    format!(
        "from hostile.versioned import versioned\n\n\ndef test_versioned():\n    assert versioned() == \"{expected}\"\n"
    )
}

fn classify_versioned(
    line: usize,
    new: &str,
    old: &str,
    expected: &str,
) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("hostile/versioned.py"), VERSIONED);
    let tests = tests_of(&[("tests/test_versioned.py", &versioned_test(expected))]);
    classify_change_with_context(
        Path::new("hostile/versioned.py"),
        line,
        new,
        Some(old),
        &owners,
        &tests,
        PythonNoBehaviorContext {
            sole_line_replacement: true,
            ..PythonNoBehaviorContext::default()
        },
        Some(VERSIONED),
    )
    .ok_or_else(|| "the edit must yield a finding".to_string())
}

#[test]
fn the_live_conditional_definition_is_an_owner_a_test_can_pin() -> Check {
    let owners = extract_owners(Path::new("hostile/versioned.py"), VERSIONED);
    let named: Vec<&str> = owners
        .iter()
        .filter(|owner| owner.name == "versioned")
        .map(|owner| owner.qualified_name.as_str())
        .collect();
    assert_eq!(named, ["versioned"], "only the live arm is an owner");
    let finding = classify_versioned(
        6,
        "        return \"newer\"",
        "        return \"new\"",
        "new",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn the_dead_conditional_definition_keeps_the_module_abstention() -> Check {
    let finding = classify_versioned(
        11,
        "        return \"older\"",
        "        return \"old\"",
        "new",
    )?;
    assert_abstains(&finding);
    Ok(())
}
