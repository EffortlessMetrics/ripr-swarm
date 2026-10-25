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
    let mut tests = tests_of(test_files);
    // Match the workspace pass: semantic gates may differ from source text.
    super::bool_oracle::credit_bare_bool_asserts(&mut tests, &owners, |path| {
        if path == Path::new("hostile/settings.py") {
            Some(source)
        } else {
            test_files
                .iter()
                .find(|(file, _)| Path::new(file) == path)
                .map(|(_, source)| *source)
        }
    });
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

fn classify_in_workspace(
    others: &[(&str, &str)],
    unread_files: bool,
    edited_tests: bool,
    test_source: &str,
) -> Result<Finding, String> {
    let mut owners = owners_of(SETTINGS);
    let mut tests = tests_of(&[("tests/test_settings.py", test_source)]);
    let own = Path::new("hostile/settings.py");
    let foreign = super::module_value::ForeignWrites::scan(
        own,
        others
            .iter()
            .map(|(file, source)| (Path::new(file), *source))
            .chain([
                (own, SETTINGS),
                (Path::new("tests/test_settings.py"), test_source),
            ]),
        unread_files,
    );
    for owner in owners.iter_mut().filter(|owner| owner.is_module_owner()) {
        owner.foreign_writes = foreign.clone();
    }
    super::bool_oracle::credit_bare_bool_asserts(&mut tests, &owners, |path| {
        if path == own {
            Some(SETTINGS)
        } else if path == Path::new("tests/test_settings.py") {
            Some(test_source)
        } else {
            others
                .iter()
                .find(|(file, _)| Path::new(file) == path)
                .map(|(_, source)| *source)
        }
    });
    for test in &mut tests {
        test.constant_rebinding.edited_in_diff = edited_tests;
    }
    classify_change_with_context(
        own,
        1,
        SETTINGS_NEW,
        Some(SETTINGS_OLD),
        &owners,
        &tests,
        PythonNoBehaviorContext {
            sole_line_replacement: true,
            ..PythonNoBehaviorContext::default()
        },
        Some(SETTINGS),
    )
    .ok_or_else(|| "the edit must yield a finding".to_string())
}

const PINNING_TEST: &str =
    "from hostile.settings import over_limit\n\n\ndef test_it():\n    assert over_limit(11)\n";

#[test]
fn unreferenced_malformed_modules_do_not_erase_exact_credit() -> Check {
    let malformed = [("hostile/broken.py", "def broken(:\n    pass\n")];
    assert_credited(&classify_in_workspace(
        &malformed,
        false,
        false,
        PINNING_TEST,
    )?);
    let mut owners = owners_of(VERSIONED);
    let file = Path::new("hostile/settings.py");
    let foreign = super::module_value::ForeignWrites::scan(
        file,
        malformed
            .iter()
            .map(|(file, source)| (Path::new(file), *source)),
        false,
    );
    super::conditional_defs::retain_workspace_certain(&mut owners, file, &foreign);
    assert_eq!(
        owners
            .iter()
            .filter(|owner| owner.version_conditional)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn referenced_or_implicitly_loaded_malformed_modules_withhold_credit() -> Check {
    for (file, prefix) in [
        ("hostile/broken.py", "import hostile.broken\n"),
        ("hostile/broken.py", "from hostile import broken\n"),
        (
            "hostile/broken.py",
            "import importlib\nimportlib.import_module('bro' 'ken')\n",
        ),
        (
            "hostile/broken.py",
            "getattr(__builtins__, '__im'+'port__')('bro'+'ken')\n",
        ),
        ("tests/__init__.py", ""),
        ("conftest.py", ""),
        ("tests/test_broken.py", ""),
    ] {
        let test = format!("{prefix}{PINNING_TEST}");
        assert_abstains(&classify_in_workspace(
            &[(file, "def broken(:\n    pass\n")],
            false,
            false,
            &test,
        )?);
    }
    assert_abstains(&classify_in_workspace(
        &[
            ("pkg/broken.py", "def broken(:\n    pass\n"),
            ("pkg/__init__.py", "__all__ = ['bro'+'ken']\n"),
            ("helper.py", "from pkg import *\n"),
        ],
        false,
        false,
        PINNING_TEST,
    )?);
    Ok(())
}

#[test]
fn files_that_only_read_the_constant_leave_the_credit() -> Check {
    let finding = classify_in_workspace(
        &[
            (
                "app/main.py",
                "import os\nfrom hostile import settings\n\nprint(settings.LIMIT + 1)\nif settings.LIMIT == 3:\n    pass\n",
            ),
            (
                "tests/conftest.py",
                "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef quiet(monkeypatch):\n    monkeypatch.setenv('X', '1')\n",
            ),
        ],
        false,
        false,
        PINNING_TEST,
    )?;
    assert_credited(&finding);
    Ok(())
}

#[test]
fn another_file_that_rewrites_the_constant_withholds_the_credit() -> Check {
    for (label, file) in [
        (
            "production attribute write",
            (
                "app/main.py",
                "import os\nfrom hostile import settings\n\nsettings.LIMIT = int(os.environ['L'])\n",
            ),
        ),
        (
            "production augmented write",
            (
                "app/main.py",
                "from hostile import settings\n\nsettings.LIMIT += 100\n",
            ),
        ),
        (
            "production setattr",
            (
                "app/main.py",
                "from hostile import settings\n\nsetattr(settings, 'LIMIT', 100)\n",
            ),
        ),
        (
            "conftest autouse monkeypatch",
            (
                "tests/conftest.py",
                "import pytest\nfrom hostile import settings\n\n\n@pytest.fixture(autouse=True)\ndef big(monkeypatch):\n    monkeypatch.setattr(settings, 'LIMIT', 100)\n",
            ),
        ),
        (
            "conftest string-target patch",
            (
                "tests/conftest.py",
                "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef big(monkeypatch):\n    monkeypatch.setattr('hostile.settings.LIMIT', 100)\n",
            ),
        ),
        (
            "helper module without tests",
            (
                "tests/support.py",
                "from hostile import settings\n\n\ndef raise_limit():\n    setattr(settings, 'LIMIT', 100)\n",
            ),
        ),
        (
            "reload of the module",
            (
                "app/reloader.py",
                "import importlib\nfrom hostile import settings\n\nimportlib.reload(settings)\n",
            ),
        ),
        (
            "sys.modules swap",
            (
                "app/swap.py",
                "import sys\nimport types\n\nsys.modules['hostile.settings'] = types.ModuleType('settings')\n",
            ),
        ),
    ] {
        let finding = classify_in_workspace(&[file], false, false, PINNING_TEST)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn a_dynamic_writer_that_names_neither_the_constant_nor_the_module_is_ignored() -> Check {
    let finding = classify_in_workspace(
        &[(
            "app/other.py",
            "from unittest.mock import patch\n\n\ndef run():\n    with patch('os.getcwd'):\n        pass\n",
        )],
        false,
        false,
        PINNING_TEST,
    )?;
    assert_credited(&finding);
    Ok(())
}

#[test]
fn an_unread_workspace_file_withholds_the_credit() -> Check {
    let finding = classify_in_workspace(&[], true, false, PINNING_TEST)?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_test_file_edited_in_the_same_diff_is_not_read_as_an_oracle() -> Check {
    // The head assertion may have been written for the new value, so the
    // pre-existing-oracle reading does not apply to a test the diff touches.
    let finding = classify_in_workspace(&[], false, true, PINNING_TEST)?;
    assert_abstains(&finding);
    Ok(())
}

#[test]
fn statements_that_leave_the_test_before_the_assertion_are_not_credited() -> Check {
    for body in [
        "    x = 1; return\n    assert over_limit(11)",
        "    x = 1; raise RuntimeError\n    assert over_limit(11)",
        "    self.skipTest('later')\n    assert over_limit(11)",
        "    raise unittest.SkipTest('later')\n    assert over_limit(11)",
        "    x = 1; self.fail('now')\n    assert over_limit(11)",
        "    sys.exit(0)\n    assert over_limit(11)",
    ] {
        let finding = classify_limit(&limit_test(body))?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{body}");
    }
    // Plain setup statements before the assertion still credit.
    assert_credited(&classify_limit(&limit_test(
        "    x = 1; y = 2\n    assert over_limit(11)",
    ))?);
    Ok(())
}

#[test]
fn same_line_guards_around_the_assertion_do_not_credit() -> Check {
    for body in [
        "    if flag: assert over_limit(11)",
        "    x = 1; assert over_limit(11)",
        "    flag and assert_it(over_limit(11))",
        "    assertTrue(over_limit(11))",
    ] {
        let finding = classify_limit(&limit_test(body))?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn live_definitions_are_owners_exactly_once_in_every_arm_shape() -> Check {
    let count = |source: &str, name: &str| {
        extract_owners(Path::new("hostile/versioned.py"), source)
            .iter()
            .filter(|owner| owner.name == name)
            .count()
    };
    let if_else = "import sys\n\nif sys.version_info >= (3, 0):\n    def live():\n        return 1\nelse:\n    def dead():\n        return 2\n";
    assert_eq!((count(if_else, "live"), count(if_else, "dead")), (1, 0));
    let else_live = "import sys\n\nif sys.version_info < (3,):\n    def dead():\n        return 2\nelse:\n    def live():\n        return 1\n";
    assert_eq!((count(else_live, "live"), count(else_live, "dead")), (1, 0));
    let elif = "import sys\n\nif sys.version_info < (3,):\n    def dead_a():\n        return 0\nelif sys.version_info >= (3, 0):\n    def live():\n        return 1\nelse:\n    def dead_b():\n        return 2\n";
    assert_eq!(
        (
            count(elif, "live"),
            count(elif, "dead_a"),
            count(elif, "dead_b")
        ),
        (1, 0, 0)
    );
    Ok(())
}

#[test]
fn review_9861_workspace_reader_replacement_withholds_credit() -> Check {
    let test = "import app.main\nfrom hostile.settings import over_limit\n\ndef test_it():\n    assert over_limit(11)\n";
    for source in [
        "from hostile import settings\nsettings.over_limit = lambda n: True\n",
        "from hostile import settings\nsetattr(settings, 'over_limit', lambda n: True)\n",
    ] {
        let finding = classify_in_workspace(&[("app/main.py", source)], false, false, test)?;
        assert_abstains(&finding);
    }
    Ok(())
}

#[test]
fn review_9861_walrus_reader_replacement_withholds_credit() -> Check {
    let source = format!("{SETTINGS}\n(over_limit := lambda n: True)\n");
    assert_abstains(&classify_edit(
        &source,
        1,
        SETTINGS_NEW,
        SETTINGS_OLD,
        &[("tests/test_settings.py", PINNING_TEST)],
        true,
    )?);
    Ok(())
}

#[test]
fn review_9861_replaced_version_guard_or_callable_keeps_module_abstention() -> Check {
    let original = "import sys\n\nif sys.version_info >= (3, 0):\n    def versioned():\n        return 'newer'\nelse:\n    def versioned():\n        return 'new'\n";
    let test = "from hostile.settings import versioned\n\ndef test_it():\n    assert versioned() == 'new'\n";
    for source in [
        original.replace("import sys\n", "import sys\nsys.version_info = (2, 7)\n"),
        original.replace(
            "import sys\n",
            "import sys\nsys.version_info: tuple = (2, 7)\n",
        ),
        format!("{original}\n(versioned := lambda: 'new')\n"),
        original.replace("import sys\n", "import sys\n(sys := replacement)\n"),
    ] {
        let line = source
            .lines()
            .position(|line| line.contains("return 'newer'"))
            .ok_or_else(|| "changed return must be present".to_string())?
            + 1;
        let finding = classify_edit(
            &source,
            line,
            "        return 'newer'",
            "        return 'new'",
            &[("tests/test_settings.py", test)],
            true,
        )?;
        assert_abstains(&finding);
    }
    Ok(())
}

#[test]
fn continuation_lines_cannot_hide_a_return_before_the_assertion() -> Check {
    for body in [
        "    value = (\n        1); return\n    assert over_limit(11)",
        "    value = \\\n        1; return\n    assert over_limit(11)",
    ] {
        let source = limit_test(body);
        let tests = tests_of(&[("tests/test_settings.py", &source)]);
        assert_eq!(tests.len(), 1, "the fixture must extract a test");
        assert_eq!(tests[0].assertions.len(), 1, "keep the target assertion");
        assert_abstains(&classify_limit(&source)?);
    }
    for body in [
        "    value = (\n        1)\n    assert over_limit(11)",
        "    value = \\\n        1\n    assert over_limit(11)",
    ] {
        assert_credited(&classify_limit(&limit_test(body))?);
    }
    Ok(())
}

#[test]
fn structural_foreign_and_test_reader_targets_withhold_credit() -> Check {
    for write in [
        "(settings.over_limit) = lambda n: True",
        "settings.over_limit, other = (lambda n: True), 0",
        "settings . over_limit = lambda n: True",
    ] {
        let source = format!("from hostile import settings\n{write}\n");
        assert_abstains(&classify_in_workspace(
            &[("app.py", &source)],
            false,
            false,
            PINNING_TEST,
        )?);
        let test = format!(
            "from hostile import settings\n{write}\n\ndef test_it():\n    assert settings.over_limit(11)\n"
        );
        assert_ne!(classify_limit(&test)?.class, ExposureClass::Exposed);
    }
    Ok(())
}

#[test]
fn foreign_conditional_bindings_are_not_certain_owners() -> Check {
    let source = "import sys\nif sys.version_info >= (3, 0):\n    def versioned():\n        return 'newer'\nelse:\n    def versioned():\n        return 'new'\n";
    let file = Path::new("hostile/settings.py");
    for writer in [
        "import sys\nsys.version_info = (2, 7)\n",
        "from hostile import settings\nsettings.versioned = lambda: 'new'\n",
    ] {
        let mut owners = owners_of(source);
        assert_eq!(
            owners
                .iter()
                .filter(|owner| owner.version_conditional)
                .count(),
            1
        );
        let foreign = super::module_value::ForeignWrites::scan(
            file,
            [(Path::new("app.py"), writer)].into_iter(),
            false,
        );
        super::conditional_defs::retain_workspace_certain(&mut owners, file, &foreign);
        assert!(owners.iter().all(|owner| !owner.version_conditional));
    }
    let mut owners = owners_of(source);
    let read = "import sys\nversions = {sys.version_info: 'current'}\n";
    let foreign = super::module_value::ForeignWrites::scan(
        file,
        [(Path::new("app.py"), read)].into_iter(),
        false,
    );
    super::conditional_defs::retain_workspace_certain(&mut owners, file, &foreign);
    assert_eq!(
        owners
            .iter()
            .filter(|owner| owner.version_conditional)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn prior_assertions_and_generator_tests_cannot_lend_unreached_credit() -> Check {
    for body in [
        "    assert False\n    assert over_limit(11)",
        "    value = (yield 1)\n    assert over_limit(11)",
        "    value = f\"{(yield 1)}\"\n    assert over_limit(11) == True",
        "    assert over_limit(11) == True\n    value = rf\"{(yield 1)}\"",
    ] {
        assert_abstains(&classify_limit(&limit_test(body))?);
    }
    assert_credited(&classify_limit(&limit_test(
        "    assert over_limit(100)\n    assert over_limit(11)",
    ))?);
    Ok(())
}

#[test]
fn generator_keyword_in_test_data_or_comments_preserves_credit() -> Check {
    for body in [
        "    label = 'yield'\n    assert over_limit(11)",
        "    # yield is data here\n    assert over_limit(11)",
    ] {
        assert_credited(&classify_limit(&limit_test(body))?);
    }
    Ok(())
}

#[test]
fn the_target_assertion_is_identified_before_crediting_its_line() -> Check {
    for body in [
        "    assert False; assert over_limit(11)",
        "    assert True; assert over_limit(11)",
    ] {
        assert_abstains(&classify_limit(&limit_test(body))?);
    }
    for body in [
        "    assert over_limit(11) # a pin",
        "    assert (\n        over_limit(11)\n    )",
    ] {
        assert_credited(&classify_limit(&limit_test(body))?);
    }
    Ok(())
}

#[test]
fn one_workspace_index_gives_each_changed_file_its_own_foreign_view() {
    // #10125: the index is built once; each changed file sees every other
    // file's writes and none of its own.
    let sources = [
        (Path::new("a.py"), "import b\nb.LIMIT = 1\n"),
        (Path::new("b.py"), "LIMIT = 10\n"),
    ];
    let index = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    assert!(
        index
            .excluding(Path::new("b.py"))
            .may_rebind("LIMIT", Path::new("b.py"))
    );
    assert!(
        !index
            .excluding(Path::new("a.py"))
            .may_rebind("LIMIT", Path::new("b.py"))
    );
    for own in ["a.py", "b.py"] {
        assert_eq!(
            index.excluding(Path::new(own)),
            super::module_value::ForeignWrites::scan(
                Path::new(own),
                sources.iter().copied(),
                false
            ),
            "{own}"
        );
    }
}

// A dict-literal constant read by a module function (#9619).

const TABLE_OLD: &str = "RATES = {\"gold\": 10, \"silver\": 5}";
const TABLE_NEW: &str = "RATES = {\"gold\": 10, \"silver\": 6}";

fn table_source(first_line: &str, tail: &str) -> String {
    format!("{first_line}\n\n\ndef price(tier):\n    return RATES.get(tier, 0)\n{tail}")
}

fn silver_test(assertion: &str) -> String {
    format!("from hostile.settings import price\n\n\ndef test_it():\n    {assertion}\n")
}

fn classify_table(
    source: &str,
    others: &[(&str, &str)],
    test_source: &str,
    new: &str,
) -> Result<Finding, String> {
    let own = Path::new("hostile/settings.py");
    let mut owners = owners_of(source);
    let mut tests = tests_of(&[("tests/test_settings.py", test_source)]);
    let foreign = super::module_value::ForeignWrites::scan(
        own,
        others
            .iter()
            .map(|(file, source)| (Path::new(file), *source))
            .chain([
                (own, source),
                (Path::new("tests/test_settings.py"), test_source),
            ]),
        false,
    );
    for owner in owners.iter_mut().filter(|owner| owner.is_module_owner()) {
        owner.foreign_writes = foreign.clone();
    }
    super::bool_oracle::credit_bare_bool_asserts(&mut tests, &owners, |path| {
        if path == own {
            Some(source)
        } else {
            (path == Path::new("tests/test_settings.py")).then_some(test_source)
        }
    });
    classify_change_with_context(
        own,
        1,
        new,
        Some(TABLE_OLD),
        &owners,
        &tests,
        PythonNoBehaviorContext {
            sole_line_replacement: true,
            ..PythonNoBehaviorContext::default()
        },
        Some(source),
    )
    .ok_or_else(|| "the edit must yield a finding".to_string())
}

fn assert_not_credited(finding: &Finding) {
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("module_value_call")),
        "{:?}",
        finding.evidence
    );
}

#[test]
fn a_dict_constant_read_through_a_function_is_credited_for_the_pinned_key() -> Check {
    let source = table_source(TABLE_NEW, "");
    let finding = classify_table(
        &source,
        &[],
        &silver_test("assert price(\"silver\") == 5"),
        TABLE_NEW,
    )?;
    assert_credited(&finding);
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("module_value_read: `RATES` {\"gold\": 10, \"silver\": 5} -> {\"gold\": 10, \"silver\": 6} through `price`")),
        "{:?}",
        finding.evidence
    );
    // Subscript reads decide the same way.
    let subscripted = format!("{TABLE_NEW}\n\n\ndef price(tier):\n    return RATES[tier]\n");
    assert_credited(&classify_table(
        &subscripted,
        &[],
        &silver_test("assert price(\"silver\") == 5"),
        TABLE_NEW,
    )?);
    Ok(())
}

#[test]
fn a_dict_edit_no_assertion_distinguishes_is_not_credited() -> Check {
    let source = table_source(TABLE_NEW, "");
    // The test pins the key the edit left alone.
    assert_not_credited(&classify_table(
        &source,
        &[],
        &silver_test("assert price(\"gold\") == 10"),
        TABLE_NEW,
    )?);
    // A key the table lacks falls to the default both before and after.
    assert_not_credited(&classify_table(
        &source,
        &[],
        &silver_test("assert price(\"bronze\") == 0"),
        TABLE_NEW,
    )?);
    // A missing key raises, which decides nothing.
    let subscripted = format!("{TABLE_NEW}\n\n\ndef price(tier):\n    return RATES[tier]\n");
    assert_not_credited(&classify_table(
        &subscripted,
        &[],
        &silver_test("assert price(\"bronze\") == 0"),
        TABLE_NEW,
    )?);
    Ok(())
}

#[test]
fn a_test_reading_the_table_itself_is_not_credited() -> Check {
    let source = table_source(TABLE_NEW, "");
    assert_not_credited(&classify_table(
        &source,
        &[],
        "from hostile.settings import RATES\n\n\ndef test_it():\n    assert RATES[\"silver\"] == 5\n",
        TABLE_NEW,
    )?);
    Ok(())
}

#[test]
fn dict_literals_outside_the_modelled_shape_are_not_credited() -> Check {
    let test = silver_test("assert price(\"silver\") == 5");
    for line in [
        // A computed value, a duplicate key, a spread, a bool key, a float.
        "RATES = {\"gold\": 10, \"silver\": rate()}",
        "RATES = {\"gold\": 10, \"silver\": 6, \"silver\": 5}",
        "RATES = {**BASE, \"silver\": 6}",
        "RATES = {True: 10, \"silver\": 6}",
        "RATES = {\"gold\": 10, \"silver\": 6.0}",
    ] {
        assert_not_credited(&classify_table(&table_source(line, ""), &[], &test, line)?);
    }
    // A bool lookup key is Python's `1`: left undecided.
    let int_keys = "RATES = {1: 10, 2: 6}";
    assert_not_credited(&classify_table(
        &table_source(int_keys, ""),
        &[],
        &silver_test("assert price(True) == 10"),
        int_keys,
    )?);
    // A multi-line literal changes on an entry line, not a statement.
    let multiline = "RATES = {\n    \"silver\": 6,\n}";
    assert_not_credited(&classify_table(
        &table_source(multiline, ""),
        &[],
        &test,
        "    \"silver\": 6,",
    )?);
    Ok(())
}

#[test]
fn a_dict_mutated_in_the_module_is_not_credited() -> Check {
    let test = silver_test("assert price(\"silver\") == 5");
    for tail in [
        "\n\ndef tune():\n    RATES[\"silver\"] = 5\n",
        "\n\ndef tune():\n    RATES.update(silver=5)\n",
        "\n\ndef tune():\n    RATES.setdefault(\"bronze\", 1)\n",
        "\n\ndef tune():\n    RATES.pop(\"silver\")\n",
        "\n\ndef tune():\n    del RATES[\"silver\"]\n",
        "\n\ndef tune():\n    alias = RATES\n    alias[\"silver\"] = 5\n",
        "\n\ndef tune(extra):\n    RATES |= extra\n",
        "\n\nRATES[\"silver\"] = 5\n",
        "\n\nRATES = {\"silver\": 5}\n",
    ] {
        assert_not_credited(&classify_table(
            &table_source(TABLE_NEW, tail),
            &[],
            &test,
            TABLE_NEW,
        )?);
    }
    Ok(())
}

#[test]
fn a_dict_mutated_or_aliased_elsewhere_is_not_credited() -> Check {
    let source = table_source(TABLE_NEW, "");
    let test = silver_test("assert price(\"silver\") == 5");
    for other in [
        "from hostile.settings import RATES\n\nRATES[\"silver\"] = 5\n",
        "from hostile.settings import RATES\n\nRATES.update(silver=5)\n",
        "from hostile.settings import RATES as table\n",
        "import hostile.settings as settings\n\nsettings.RATES[\"silver\"] = 5\n",
        "import hostile.settings as settings\n\nalias = settings.RATES\n",
        "import hostile.settings as settings\n\nstate = getattr(settings, \"RATES\")\nstate.clear()\n",
        "from hostile import settings\n\nprint(f\"{settings.RATES.clear()}\")\n",
    ] {
        assert_not_credited(&classify_table(
            &source,
            &[("hostile/other.py", other)],
            &test,
            TABLE_NEW,
        )?);
    }
    // A test file may not mutate it either.
    assert_not_credited(&classify_table(
        &source,
        &[],
        "from hostile.settings import RATES, price\n\n\ndef test_it():\n    RATES[\"silver\"] = 5\n    assert price(\"silver\") == 5\n",
        TABLE_NEW,
    )?);
    Ok(())
}

#[test]
fn pure_reads_and_re_exports_elsewhere_keep_the_credit() -> Check {
    let source = table_source(TABLE_NEW, "");
    let test = silver_test("assert price(\"silver\") == 5");
    for other in [
        "from hostile.settings import RATES, price\n\n__all__ = [\"RATES\", \"price\"]\n",
        "import hostile.settings as settings\n\nGOLD = settings.RATES.get(\"gold\")\n",
        "from hostile.settings import RATES\n\n\ndef gold():\n    return RATES[\"gold\"]\n",
    ] {
        assert_credited(&classify_table(
            &source,
            &[("hostile/other.py", other)],
            &test,
            TABLE_NEW,
        )?);
    }
    Ok(())
}
