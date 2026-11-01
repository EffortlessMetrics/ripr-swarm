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
        // The definite exit is read from the parsed statements, so the
        // assertion after it is never extracted and earns no credit.
        assert!(
            tests[0].assertions.is_empty(),
            "an assertion after a definite return must not be extracted"
        );
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
fn a_write_shared_with_another_file_survives_excluding_one_of_them() {
    // #10857: the index counts the sources behind each write and subtracts
    // the excluded file, so a write that file shares with another stays.
    let sources = [
        (Path::new("a.py"), "import c\nc.LIMIT = 1\n"),
        (Path::new("b.py"), "import c\nc.LIMIT = 2\nc.RATE = 3\n"),
        (Path::new("c.py"), "LIMIT = 10\nRATE = 5\n"),
    ];
    let index = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    let module = Path::new("c.py");
    let without_b = index.excluding(Path::new("b.py"));
    assert!(without_b.may_rebind("LIMIT", module));
    assert!(!without_b.may_rebind("RATE", module));
    let without_a = index.excluding(Path::new("a.py"));
    assert!(without_a.may_rebind("LIMIT", module));
    assert!(without_a.may_rebind("RATE", module));
    let without_c = index.excluding(module);
    assert!(without_c.may_rebind("LIMIT", module) && without_c.may_rebind("RATE", module));
}

#[test]
fn subtracting_the_excluded_file_matches_rebuilding_every_write_set() {
    // #10857: an independent oracle for all five write sets, not just
    // attributes. Each file shares some writes with another and holds some
    // alone, and excluding each file (or none) must match the per-file
    // rebuild the subtraction replaced.
    let sources = [
        (Path::new("c.py"), "RATES = {\"a\": 1}\nLIMIT = 10\n"),
        (
            Path::new("a.py"),
            "import c\nc.LIMIT = 1\nc.RATES[\"b\"] = 2\nsetattr(c, \"LIMIT\", 3)\n",
        ),
        (
            Path::new("b.py"),
            "import c\nc.LIMIT = 2\nc.RATE = 3\nvalue = getattr(c, \"RATES\")\nc.RATES.clear()\n",
        ),
        (
            Path::new("d.py"),
            "import c\nimport b\nnames = dir(c)\nsetattr(c, \"RATE\", 4)\n",
        ),
    ];
    let index = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    let all = index.excluding(Path::new("absent.py"));
    assert_eq!(all.written_sets(), [true; 5], "{all:?}");
    for own in ["absent.py", "a.py", "b.py", "c.py", "d.py"] {
        let own = Path::new(own);
        assert_eq!(
            index.excluding(own),
            index.rebuilt_excluding(own),
            "{own:?}"
        );
    }
}

#[test]
fn reflective_files_of_one_component_each_count_toward_its_reach() {
    // #10914: reflective files of one import component share one reach, and
    // the totals count it once per file holding it, so excluding one of
    // them keeps the reach the others still hold. The per-file rebuild is
    // the independent oracle; a separate component's file stays its own.
    let sources = [
        (Path::new("c.py"), "TABLE = {\"a\": 1}\n"),
        (Path::new("a.py"), "import c\nnames = dir(c)\n"),
        (
            Path::new("b.py"),
            "import c\nvalue = getattr(c, \"TABLE\")\n",
        ),
        (Path::new("solo.py"), "names = dir(object)\n"),
    ];
    let index = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    let module = Path::new("c.py");
    for own in ["absent.py", "a.py", "b.py", "c.py", "solo.py"] {
        let own = Path::new(own);
        assert_eq!(
            index.excluding(own),
            index.rebuilt_excluding(own),
            "{own:?}"
        );
    }
    assert!(
        index
            .excluding(Path::new("a.py"))
            .may_mutate_table("TABLE", module)
    );
    assert!(
        index
            .excluding(Path::new("b.py"))
            .may_mutate_table("TABLE", module)
    );
}

#[test]
fn excluding_views_share_the_word_counts_and_keep_the_sorted_wire_form() -> Check {
    // #10914: each changed file's view reads the workspace's shared per-word
    // counts and hides only the words its own file alone holds, so a view
    // is not a copy of every workspace word. It still serializes as the
    // sorted word list, and reads back to an equal view.
    let sources = [
        (Path::new("c.py"), "LIMIT = 10\n"),
        (
            Path::new("a.py"),
            "import c\nc.LIMIT = 1\nc.__defaults__ = ()\nc.__x__ = 0\nc._ = 1\n",
        ),
        (Path::new("b.py"), "import c\nc.LIMIT = 2\nc.RATE = 3\n"),
    ];
    let index = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    let module = Path::new("c.py");
    let without_a = index.excluding(Path::new("a.py"));
    let without_b = index.excluding(Path::new("b.py"));
    assert!(without_a.shares_word_counts(&without_b));
    assert!(without_a.shares_word_counts(&without_a.clone()));
    assert!(without_a.may_rebind("LIMIT", module) && without_a.may_rebind("RATE", module));
    assert!(!without_a.may_rebind("_", module));
    assert!(!without_a.writes_dunder() && !without_a.may_replace_reader_code());
    assert!(without_b.writes_dunder() && without_b.may_replace_reader_code());
    assert!(without_b.may_rebind("_", module) && !without_b.may_rebind("RATE", module));

    let wire = serde_json::to_value(&without_b).map_err(|error| error.to_string())?;
    assert_eq!(
        wire["attributes"],
        serde_json::json!(["LIMIT", "_", "__defaults__", "__x__"]),
        "{wire}"
    );
    let read: super::module_value::ForeignWrites =
        serde_json::from_value(wire).map_err(|error| error.to_string())?;
    assert_eq!(read, without_b);
    assert!(read.writes_dunder() && !read.may_rebind("RATE", module));
    Ok(())
}

#[test]
fn the_source_fact_parse_gives_the_foreign_index_the_same_attribute_writes() -> Check {
    // #10914: the workspace pass hands the index each file's assigned
    // attributes from its source-fact parse instead of parsing again. A test
    // file whose helper is inlined, and a file that does not parse, must give
    // the same foreign writes as the index's own parse.
    let sources = [
        (
            Path::new("a.py"),
            "import b\nb.LIMIT = 1\nb.other.depth += 2\n",
        ),
        (Path::new("b.py"), "LIMIT = 10\nRATE = {\"x\": 1}\n"),
        (
            Path::new("tests/test_b.py"),
            "import b\n\nb.RATE = 3\n\n\ndef check(value):\n    assert value == 3\n\n\ndef test_limit():\n    check(3)\n",
        ),
        (Path::new("broken.py"), "def (:\n"),
    ];
    let assigned: std::collections::BTreeMap<std::path::PathBuf, Option<Vec<String>>> = sources
        .iter()
        .map(|(file, source)| {
            (
                file.to_path_buf(),
                super::source_facts::extract_source_facts(file, source).assigned_attributes,
            )
        })
        .collect();
    let a_writes = assigned
        .get(Path::new("a.py"))
        .cloned()
        .flatten()
        .ok_or("a.py parses")?;
    if !a_writes.iter().any(|name| name == "LIMIT") {
        return Err(format!("a.py assigns LIMIT: {a_writes:?}"));
    }
    // The test file takes the helper-inlining branch, and its writes still
    // come from the file as written.
    let test_file = Path::new("tests/test_b.py");
    let test_source = sources
        .iter()
        .find(|(file, _)| *file == test_file)
        .map(|(_, source)| *source)
        .ok_or("test file fixture")?;
    let Ok(rustpython_parser::ast::Mod::Module(test_module)) =
        super::source_facts::parse_module_result(test_file, test_source)
    else {
        return Err("test file parses".to_string());
    };
    if super::helper_inline::inline_test_helpers(test_source, &test_module.body).is_none() {
        return Err("the test file's helper call is inlined".to_string());
    }
    if assigned.get(test_file) != Some(&Some(vec!["RATE".to_string()])) {
        return Err(format!("test file assigns RATE: {assigned:?}"));
    }
    if assigned.get(Path::new("broken.py")) != Some(&None) {
        return Err(format!("broken.py does not parse: {assigned:?}"));
    }
    let parsed = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    let prior = assigned
        .iter()
        .map(|(file, assigned)| {
            (
                file.clone(),
                super::module_value::PriorFileFacts {
                    assigned: assigned.clone(),
                    lexes: None,
                },
            )
        })
        .collect();
    let shared =
        super::module_value::ForeignIndex::build_with_prior(sources.iter().copied(), false, &prior);
    for (own, _) in &sources {
        if shared.excluding(own) != parsed.excluding(own) {
            return Err(format!(
                "{own:?}: shared {:?} != parsed {:?}",
                shared.excluding(own),
                parsed.excluding(own)
            ));
        }
    }
    // The workspace pass also hands over its lexes of each file (dict-literal
    // names, import words), which a warm run serves from the fact store.
    let lexed_prior: std::collections::BTreeMap<_, _> = sources
        .iter()
        .map(|(file, source)| {
            let facts = super::source_facts::extract_workspace_source_facts(file, source);
            (
                file.to_path_buf(),
                super::module_value::PriorFileFacts {
                    assigned: facts.assigned_attributes,
                    lexes: facts.foreign_lexes,
                },
            )
        })
        .collect();
    if lexed_prior.values().any(|prior| prior.lexes.is_none()) {
        return Err("every workspace file carries its lexes".to_string());
    }
    let b_lexes = lexed_prior
        .get(Path::new("b.py"))
        .and_then(|prior| prior.lexes.as_ref())
        .ok_or("b.py lexes")?;
    if b_lexes.dict_literal_names != ["RATE"] {
        return Err(format!("b.py binds the RATE table: {b_lexes:?}"));
    }
    let lexed = super::module_value::ForeignIndex::build_with_prior(
        sources.iter().copied(),
        false,
        &lexed_prior,
    );
    for (own, _) in &sources {
        if lexed.excluding(own) != parsed.excluding(own) {
            return Err(format!(
                "{own:?}: lexed {:?} != parsed {:?}",
                lexed.excluding(own),
                parsed.excluding(own)
            ));
        }
    }
    Ok(())
}

#[test]
fn the_foreign_index_reads_supplied_attribute_writes_instead_of_parsing() {
    // The supplied writes are what the index uses: a.py's text assigns
    // nothing, but its recorded parse says it rebinds LIMIT.
    let sources = [
        (Path::new("a.py"), "import b\nx = 1\n"),
        (Path::new("b.py"), "LIMIT = 10\n"),
    ];
    let assigned = std::collections::BTreeMap::from([(
        std::path::PathBuf::from("a.py"),
        Some(vec!["LIMIT".to_string()]),
    )]);
    let parsed = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    let prior = assigned
        .iter()
        .map(|(file, assigned)| {
            (
                file.clone(),
                super::module_value::PriorFileFacts {
                    assigned: assigned.clone(),
                    lexes: None,
                },
            )
        })
        .collect();
    let shared =
        super::module_value::ForeignIndex::build_with_prior(sources.iter().copied(), false, &prior);
    assert!(
        !parsed
            .excluding(Path::new("b.py"))
            .may_rebind("LIMIT", Path::new("b.py"))
    );
    assert!(
        shared
            .excluding(Path::new("b.py"))
            .may_rebind("LIMIT", Path::new("b.py"))
    );
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
    classify_table_edit(source, others, test_source, TABLE_OLD, new)
}

fn classify_table_edit(
    source: &str,
    others: &[(&str, &str)],
    test_source: &str,
    old: &str,
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
        Some(old),
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
    let subscripted = format!("{int_keys}\n\n\ndef price(tier):\n    return RATES[tier]\n");
    assert_not_credited(&classify_table(
        &subscripted,
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
        "\n\nimport sys\n\ngetattr(sys.modules[__name__], \"RATES\")[\"silver\"] = 5\n",
        "\n\nfrom hostile import settings as self_module\n\ngetattr(self_module, \"RATES\")[\"silver\"] = 5\n",
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
        "from hostile.settings import RATES; RATES[\"silver\"] = 5\n",
        "from hostile.settings import RATES\nx = f\"\"\"\n{RATES.clear()}\n\"\"\"\n",
        "from hostile import settings\n\nfor n in dir(settings):\n    v = getattr(settings, n)\n    isinstance(v, dict) and v.clear()\n",
        "import inspect\nfrom hostile import settings\n\nfor _, v in inspect.getmembers(settings):\n    getattr(v, \"clear\", print)()\n",
        "from hostile.settings import *\n\nfor v in list(locals().values()):\n    getattr(v, \"clear\", print)()\n",
        "import operator\nfrom hostile import settings\n\noperator.attrgetter(\"RA\" + \"TES\")(settings).clear()\n",
        "from hostile import settings\n\nsettings.price.__globals__[\"RATES\"].clear()\n",
        // Definitions before the mutation must not hide it from the scan.
        "from hostile.settings import RATES\n\n\ndef f():\n    return 1\n\n\nclass Box:\n    pass\n\n\nRATES[\"silver\"] = 5\n",
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

#[test]
fn a_bool_key_is_unknown_not_absent_for_get_fallbacks() -> Check {
    // `True` selects integer key 1, so the assertion holds under both tables;
    // the changed fallback must not be read as the answer.
    let old = "RATES = {1: 5, 2: 5}";
    let new = "RATES = {1: 5, 2: 6}";
    let source = format!("{new}\n\n\ndef price(tier):\n    return RATES.get(tier, RATES[2])\n");
    assert_not_credited(&classify_table_edit(
        &source,
        &[],
        &silver_test("assert price(True) == 5"),
        old,
        new,
    )?);
    // Controls: a present integer key is unaffected by the edit, and a
    // genuinely missing key reads the changed fallback.
    assert_not_credited(&classify_table_edit(
        &source,
        &[],
        &silver_test("assert price(1) == 5"),
        old,
        new,
    )?);
    assert_credited(&classify_table_edit(
        &source,
        &[],
        &silver_test("assert price(3) == 5"),
        old,
        new,
    )?);
    Ok(())
}

#[test]
fn get_on_a_scalar_receiver_is_unknown_not_the_default() -> Check {
    // `5.get(...)` raises AttributeError in Python; the default is never read.
    let old = "RATES = {\"gold\": 10, \"silver\": 5}";
    let new = "RATES = {\"gold\": 10, \"silver\": 6}";
    let source = format!(
        "{new}\n\n\ndef price(tier):\n    return tier.get(\"silver\", RATES[\"silver\"])\n"
    );
    assert_not_credited(&classify_table_edit(
        &source,
        &[],
        &silver_test("assert price(5) == 5"),
        old,
        new,
    )?);
    Ok(())
}

#[test]
fn a_parenthesized_dict_is_guarded_like_a_plain_one() -> Check {
    let old = "RATES = ({\"gold\": 10, \"silver\": 5})";
    let new = "RATES = ({\"gold\": 10, \"silver\": 6})";
    let source = table_source(new, "");
    let test = silver_test("assert price(\"silver\") == 5");
    let store = "from hostile.settings import RATES\n\nRATES[\"silver\"] = 5\n";
    assert_not_credited(&classify_table_edit(
        &source,
        &[("hostile/other.py", store)],
        &test,
        old,
        new,
    )?);
    // The same edit with no foreign write keeps its credit.
    assert_credited(&classify_table_edit(&source, &[], &test, old, new)?);
    Ok(())
}

#[test]
fn foreign_index_keeps_writes_and_word_cap_across_parse_batches() {
    // #10441: sources are parsed in parallel batches. Writes from any batch
    // reach every other file's view, and the word cap still makes the whole
    // view unknown wherever in the input it trips.
    let mut owned: Vec<(String, String)> = vec![("m.py".to_string(), "LIMIT = 10\n".to_string())];
    for index in 1..600 {
        owned.push((
            format!("pad{index:03}.py"),
            format!("PAD{index} = {index}\n"),
        ));
    }
    owned[400].1 = "import m\nm.LIMIT = 1\n".to_string();
    owned[550].1 = "setattr(target, 'OTHER', 1)\nOTHER\n".to_string();
    let sources: Vec<(&Path, &str)> = owned
        .iter()
        .map(|(file, source)| (Path::new(file.as_str()), source.as_str()))
        .collect();
    let index = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    let module = Path::new("m.py");
    let view = index.excluding(module);
    assert!(
        view.may_rebind("LIMIT", module),
        "a second-batch attribute write reaches the first-batch module"
    );
    assert!(
        view.may_rebind("OTHER", module),
        "a third-batch dynamic writer's words reach the first-batch module"
    );
    assert!(
        !view.may_rebind("UNSEEN", module),
        "control: a name no file writes stays stable"
    );
    assert!(
        !index
            .excluding(Path::new(owned[400].0.as_str()))
            .may_rebind("LIMIT", module),
        "the writing file's own view excludes its write"
    );

    let flood: String = (0..200_001).map(|word| format!("w{word}\n")).collect();
    let mut capped = sources.clone();
    capped[300] = (Path::new("flood.py"), flood.as_str());
    let capped_index = super::module_value::ForeignIndex::build(capped.iter().copied(), false);
    assert!(
        capped_index.excluding(module).may_rebind("UNSEEN", module),
        "a word cap tripped in a later batch leaves the absence of writes unknown"
    );
}

// A body-only edit of a top-level `NAME = lambda ...` binding (#9619).

const LAMBDA_OLD: &str = "triple = lambda x: x * 3";
const LAMBDA_NEW: &str = "triple = lambda x: x * 4";
const LAMBDA_TEST: &str =
    "from hostile.settings import triple\n\n\ndef test_it():\n    assert triple(4) == 12\n";

fn lambda_test(body: &str) -> String {
    format!("from hostile.settings import triple\n\n\ndef test_it():\n{body}\n")
}

/// Classify a one-line edit of `source` (new text at line 1) against the test
/// and any other workspace files, with the workspace's foreign-write scan.
fn classify_lambda(
    source: &str,
    new: &str,
    old: &str,
    others: &[(&str, &str)],
    test_source: &str,
) -> Result<Finding, String> {
    classify_line(source, 1, new, old, others, test_source)
}

fn classify_line(
    source: &str,
    line: usize,
    new: &str,
    old: &str,
    others: &[(&str, &str)],
    test_source: &str,
) -> Result<Finding, String> {
    let mut owners = owners_of(source);
    let mut tests = tests_of(&[("tests/test_settings.py", test_source)]);
    let own = Path::new("hostile/settings.py");
    let foreign = super::module_value::ForeignWrites::scan(
        own,
        others
            .iter()
            .map(|(file, text)| (Path::new(file), *text))
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
        } else if path == Path::new("tests/test_settings.py") {
            Some(test_source)
        } else {
            others
                .iter()
                .find(|(file, _)| Path::new(file) == path)
                .map(|(_, text)| *text)
        }
    });
    classify_change_with_context(
        own,
        line,
        new,
        Some(old),
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

fn lambda_source(line: &str) -> String {
    format!("{line}\n")
}

fn assert_lambda_withheld(finding: &Finding, label: &str) {
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{label}: {finding:?}"
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("related_test_relation: module_value_call")),
        "{label}: {:?}",
        finding.evidence
    );
}

#[test]
fn a_lambda_body_edit_no_assertion_distinguishes_is_not_credited() -> Check {
    for body in [
        // 0 * 3 and 0 * 4 agree.
        "    assert triple(0) == 0",
        // Fails before the edit, so it is not an oracle that the edit broke.
        "    assert triple(4) == 13",
        // Holds under both bodies.
        "    assert triple(4) > 0",
    ] {
        let finding = classify_lambda(
            &lambda_source(LAMBDA_NEW),
            LAMBDA_NEW,
            LAMBDA_OLD,
            &[],
            &lambda_test(body),
        )?;
        assert_lambda_withheld(&finding, body);
    }
    Ok(())
}

#[test]
fn a_lambda_edit_that_is_not_a_body_only_change_is_not_credited() -> Check {
    for (label, new, old) in [
        (
            "parameter renamed",
            "triple = lambda y: y * 4",
            "triple = lambda x: x * 3",
        ),
        (
            "arity changed",
            "triple = lambda x: x * 4",
            "triple = lambda x, y: x * 3",
        ),
        (
            "default added",
            "triple = lambda x=1: x * 4",
            "triple = lambda x: x * 3",
        ),
        (
            "vararg added",
            "triple = lambda *x: 4",
            "triple = lambda *x: 3",
        ),
        (
            "name changed",
            "other = lambda x: x * 4",
            "triple = lambda x: x * 3",
        ),
        ("was not a lambda", "triple = lambda x: x * 4", "triple = 3"),
        (
            "annotation changed (the old one may not import)",
            "triple: int = lambda x: x * 4",
            "triple: MissingType = lambda x: x * 3",
        ),
        (
            "annotation added",
            "triple: int = lambda x: x * 4",
            "triple = lambda x: x * 3",
        ),
    ] {
        let finding = classify_lambda(&lambda_source(new), new, old, &[], LAMBDA_TEST)?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_lambda_body_outside_the_evaluator_is_not_credited() -> Check {
    for (label, new, old) in [
        (
            "call",
            "triple = lambda x: abs(x) * 4",
            "triple = lambda x: abs(x) * 3",
        ),
        (
            "attribute",
            "triple = lambda x: x.real * 4",
            "triple = lambda x: x.real * 3",
        ),
        (
            "subscript",
            "triple = lambda x: x[0] * 4",
            "triple = lambda x: x[0] * 3",
        ),
        (
            "global name",
            "triple = lambda x: x * K",
            "triple = lambda x: x * 3",
        ),
        (
            "float",
            "triple = lambda x: x * 4.0",
            "triple = lambda x: x * 3.0",
        ),
        (
            "nested lambda",
            "triple = lambda x: (lambda y: y * 4)(x)",
            "triple = lambda x: (lambda y: y * 3)(x)",
        ),
    ] {
        let finding = classify_lambda(&lambda_source(new), new, old, &[], LAMBDA_TEST)?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_lambda_bound_twice_or_conditionally_is_not_credited() -> Check {
    for (label, source) in [
        (
            "rebound later",
            "triple = lambda x: x * 4\ntriple = print\n",
        ),
        (
            "rebound by def",
            "triple = lambda x: x * 4\n\n\ndef triple(x):\n    return 0\n",
        ),
        ("augmented", "triple = lambda x: x * 4\ntriple += 1\n"),
        (
            "loop target",
            "triple = lambda x: x * 4\nfor triple in []:\n    pass\n",
        ),
        (
            "global write",
            "triple = lambda x: x * 4\n\n\ndef patch():\n    global triple\n    triple = print\n",
        ),
    ] {
        let finding = classify_lambda(source, LAMBDA_NEW, LAMBDA_OLD, &[], LAMBDA_TEST)?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn another_file_that_rewrites_the_lambda_withholds_the_credit() -> Check {
    for (label, file) in [
        (
            "attribute write",
            (
                "app/main.py",
                "from hostile import settings\n\nsettings.triple = print\n",
            ),
        ),
        (
            "setattr",
            (
                "app/main.py",
                "from hostile import settings\n\nsetattr(settings, 'triple', print)\n",
            ),
        ),
        (
            "autouse monkeypatch",
            (
                "tests/conftest.py",
                "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef off(monkeypatch):\n    monkeypatch.setattr('hostile.settings.triple', print)\n",
            ),
        ),
        (
            "reload",
            (
                "app/reloader.py",
                "import importlib\nfrom hostile import settings\n\nimportlib.reload(settings)\n",
            ),
        ),
    ] {
        let finding = classify_lambda(
            &lambda_source(LAMBDA_NEW),
            LAMBDA_NEW,
            LAMBDA_OLD,
            &[file],
            LAMBDA_TEST,
        )?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_test_that_shadows_or_may_skip_the_lambda_is_not_credited() -> Check {
    for (label, test) in [
        (
            "local rebind",
            "from hostile.settings import triple\n\n\ndef test_it():\n    triple = lambda x: x * 3\n    assert triple(4) == 12\n".to_string(),
        ),
        (
            "parameter",
            "from hostile.settings import triple\n\n\ndef test_it(triple):\n    assert triple(4) == 12\n".to_string(),
        ),
        (
            "other module",
            "from other.settings import triple\n\n\ndef test_it():\n    assert triple(4) == 12\n".to_string(),
        ),
        (
            "patched in the test",
            "from unittest.mock import patch\nfrom hostile.settings import triple\n\n\ndef test_it():\n    with patch('hostile.settings.triple', print):\n        assert triple(4) == 12\n".to_string(),
        ),
        ("guarded", lambda_test("    if flag:\n        assert triple(4) == 12")),
        ("after return", lambda_test("    return\n    assert triple(4) == 12")),
        ("non-literal argument", lambda_test("    n = 4\n    assert triple(n) == 12")),
        ("keyword call", lambda_test("    assert triple(x=4) == 12")),
        ("wrong arity", lambda_test("    assert triple(4, 1) == 12")),
    ] {
        let finding = classify_lambda(
            &lambda_source(LAMBDA_NEW),
            LAMBDA_NEW,
            LAMBDA_OLD,
            &[],
            &test,
        )?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_pinned_lambda_body_edit_is_credited() -> Check {
    for body in [
        "    assert triple(4) == 12",
        "    assert triple(2) == 6\n    assert triple(0) == 0",
        "    assert 12 == triple(4)",
        "    assert triple(1) < 4",
    ] {
        let finding = classify_lambda(
            &lambda_source(LAMBDA_NEW),
            LAMBDA_NEW,
            LAMBDA_OLD,
            &[],
            &lambda_test(body),
        )?;
        assert_credited(&finding);
    }
    // Read through the module path and an alias.
    for test in [
        "from hostile import settings\n\n\ndef test_it():\n    assert settings.triple(4) == 12\n",
        "from hostile.settings import triple as t3\n\n\ndef test_it():\n    assert t3(4) == 12\n",
    ] {
        let finding = classify_lambda(
            &lambda_source(LAMBDA_NEW),
            LAMBDA_NEW,
            LAMBDA_OLD,
            &[],
            test,
        )?;
        assert_credited(&finding);
    }
    Ok(())
}

// A default edit of a plain `@dataclass` field (#9619).

const DC_OLD: &str = "    tax_rate: float = 0.2";
const DC_NEW: &str = "    tax_rate: float = 0.25";
const DC_METHOD: &str =
    "    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n";
const DC_TEST: &str = "from hostile.settings import Invoice\n\n\ndef test_it():\n    assert Invoice(100).total() == 120\n";

fn dc_source(head: &str, tax: &str, rest: &str) -> String {
    format!("from dataclasses import dataclass\n\n\n{head}\n    amount: int\n{tax}\n\n{rest}")
}

fn dc_test(body: &str) -> String {
    format!("from hostile.settings import Invoice\n\n\ndef test_it():\n{body}\n")
}

/// Classify the edit of the `tax_rate` line of `source` (new text `new`, the
/// old text `DC_OLD`).
fn classify_dc(
    source: &str,
    new: &str,
    others: &[(&str, &str)],
    test_source: &str,
) -> Result<Finding, String> {
    let line = source
        .lines()
        .position(|text| text == new)
        .ok_or_else(|| format!("the edited line is not in the source: {new}"))?
        + 1;
    classify_line(source, line, new, DC_OLD, others, test_source)
}

fn dc_default() -> String {
    dc_source("@dataclass\nclass Invoice:", DC_NEW, DC_METHOD)
}

#[test]
fn a_dataclass_default_no_assertion_distinguishes_is_not_credited() -> Check {
    for (label, body) in [
        (
            "rate passed positionally",
            "    assert Invoice(100, 0.5).total() == 150",
        ),
        (
            "rate passed by keyword",
            "    assert Invoice(100, tax_rate=0.5).total() == 150",
        ),
        (
            "the same rate passed explicitly",
            "    assert Invoice(100, 0.2).total() == 120",
        ),
        ("another field", "    assert Invoice(100).amount == 100"),
        (
            "both rates round alike",
            "    assert Invoice(1).total() == 1",
        ),
        (
            "not the pinned value",
            "    assert Invoice(100).total() > 0",
        ),
    ] {
        let finding = classify_dc(&dc_default(), DC_NEW, &[], &dc_test(body))?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_dataclass_that_is_not_plain_is_not_credited() -> Check {
    let methods = |extra: &str| format!("{DC_METHOD}\n{extra}");
    for (label, source) in [
        (
            "post init",
            dc_source(
                "@dataclass\nclass Invoice:",
                DC_NEW,
                &methods("    def __post_init__(self):\n        self.tax_rate = 0.3\n"),
            ),
        ),
        (
            "custom init",
            dc_source(
                "@dataclass\nclass Invoice:",
                DC_NEW,
                &methods(
                    "    def __init__(self, amount):\n        self.amount = amount\n        self.tax_rate = 0.3\n",
                ),
            ),
        ),
        (
            "setattr hook",
            dc_source(
                "@dataclass\nclass Invoice:",
                DC_NEW,
                &methods(
                    "    def __setattr__(self, name, value):\n        object.__setattr__(self, name, 0.3)\n",
                ),
            ),
        ),
        (
            "base class",
            dc_source("@dataclass\nclass Invoice(Base):", DC_NEW, DC_METHOD),
        ),
        (
            "metaclass",
            dc_source(
                "@dataclass\nclass Invoice(metaclass=Meta):",
                DC_NEW,
                DC_METHOD,
            ),
        ),
        (
            "frozen call decorator",
            dc_source("@dataclass(frozen=True)\nclass Invoice:", DC_NEW, DC_METHOD),
        ),
        (
            "init disabled",
            dc_source("@dataclass(init=False)\nclass Invoice:", DC_NEW, DC_METHOD),
        ),
        (
            "second decorator",
            dc_source("@wrap\n@dataclass\nclass Invoice:", DC_NEW, DC_METHOD),
        ),
        (
            "plain class variable",
            dc_source(
                "@dataclass\nclass Invoice:",
                DC_NEW,
                &methods("    rounding = 2\n"),
            ),
        ),
        (
            "slots",
            dc_source(
                "@dataclass\nclass Invoice:",
                DC_NEW,
                &methods("    __slots__ = ('amount',)\n"),
            ),
        ),
        (
            "nested class",
            dc_source(
                "@dataclass\nclass Invoice:",
                DC_NEW,
                &methods("    class Meta:\n        pass\n"),
            ),
        ),
        (
            "keyword-only field",
            dc_source(
                "@dataclass\nclass Invoice:",
                DC_NEW,
                &format!("    discount: float = field(default=0.0, kw_only=True)\n\n{DC_METHOD}"),
            ),
        ),
        (
            "decorator is not the dataclasses one",
            dc_default().replace(
                "from dataclasses import dataclass",
                "from attrs import dataclass",
            ),
        ),
        (
            "decorator is rebound",
            dc_default().replace(
                "@dataclass\nclass",
                "dataclass = lambda cls: cls\n\n\n@dataclass\nclass",
            ),
        ),
    ] {
        let line_new = DC_NEW;
        let finding = classify_dc(&source, line_new, &[], DC_TEST)?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_dataclass_default_that_is_not_a_plain_literal_swap_is_not_credited() -> Check {
    for (label, new, old) in [
        (
            "field default",
            "    tax_rate: float = field(default=0.25)",
            DC_OLD,
        ),
        (
            "computed default",
            "    tax_rate: float = 0.2 + 0.05",
            DC_OLD,
        ),
        (
            "annotation changed",
            "    tax_rate: Decimal = 0.25",
            "    tax_rate: Missing = 0.2",
        ),
        ("field renamed", "    rate: float = 0.25", DC_OLD),
        (
            "default added",
            "    tax_rate: float = 0.25",
            "    tax_rate: float",
        ),
    ] {
        let source = dc_source("@dataclass\nclass Invoice:", new, DC_METHOD);
        let line = source
            .lines()
            .position(|text| text == new)
            .ok_or_else(|| format!("{label}: line missing"))?
            + 1;
        let finding = classify_line(&source, line, new, old, &[], DC_TEST)?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_dataclass_method_outside_the_evaluator_is_not_credited() -> Check {
    for (label, method) in [
        (
            "property",
            "    @property\n    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n",
        ),
        (
            "two statements",
            "    def total(self):\n        rate = self.tax_rate\n        return round(self.amount * (1 + rate))\n",
        ),
        (
            "calls another method",
            "    def total(self):\n        return round(self.amount * (1 + self.rate()))\n\n    def rate(self):\n        return self.tax_rate\n",
        ),
        (
            "default argument",
            "    def total(self, extra=0):\n        return round(self.amount * (1 + self.tax_rate)) + extra\n",
        ),
        (
            "defined twice",
            "    def total(self):\n        return 0\n\n    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n",
        ),
        (
            "unknown call",
            "    def total(self):\n        return fmt(self.amount * (1 + self.tax_rate))\n",
        ),
        (
            "attribute of an attribute",
            "    def total(self):\n        return round(self.amount * (1 + self.tax_rate.real))\n",
        ),
        (
            "method named like the field",
            "    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n\n    def tax_rate(self):\n        return 0.9\n",
        ),
        (
            "shadowed round",
            "    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n\n\ndef round(value):\n        return 0\n",
        ),
    ] {
        let source = dc_source("@dataclass\nclass Invoice:", DC_NEW, method);
        let finding = classify_dc(&source, DC_NEW, &[], DC_TEST)?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_rebound_dataclass_or_field_is_not_credited() -> Check {
    let source = dc_default();
    let rebound = format!("{source}\n\nInvoice = other\n");
    let finding = classify_dc(&rebound, DC_NEW, &[], DC_TEST)?;
    assert_lambda_withheld(&finding, "class bound twice");
    for (label, other) in [
        (
            "class attribute write",
            "from hostile.settings import Invoice\nInvoice.tax_rate = 0.5\n",
        ),
        (
            "setattr",
            "import hostile.settings as s\nsetattr(s.Invoice, 'tax_rate', 0.5)\n",
        ),
        (
            "module attribute write",
            "import hostile.settings as s\ns.Invoice = other\n",
        ),
    ] {
        let finding = classify_dc(&source, DC_NEW, &[("tests/conftest.py", other)], DC_TEST)?;
        assert_lambda_withheld(&finding, label);
    }
    for (label, body) in [
        (
            "test writes the field",
            "    Invoice.tax_rate = 0.5\n    assert Invoice(100).total() == 120",
        ),
        (
            "test shadows the class",
            "    Invoice = other\n    assert Invoice(100).total() == 120",
        ),
        (
            "test may skip",
            "    if cond:\n        return\n    assert Invoice(100).total() == 120",
        ),
        (
            "extra positional argument",
            "    assert Invoice(100, 0.2, 7).total() == 120",
        ),
        (
            "unknown keyword",
            "    assert Invoice(100, rate=0.2).total() == 120",
        ),
        (
            "missing required field",
            "    assert Invoice().total() == 120",
        ),
        (
            "non-literal argument",
            "    assert Invoice(compute()).total() == 120",
        ),
        ("star arguments", "    assert Invoice(*args).total() == 120"),
    ] {
        let finding = classify_dc(&source, DC_NEW, &[], &dc_test(body))?;
        assert_lambda_withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_replaced_dataclass_method_or_shadowed_round_is_not_credited() -> Check {
    let source = dc_default();
    for (label, other) in [
        (
            "method replaced in another file",
            "from hostile.settings import Invoice\nInvoice.total = lambda self: 120\n",
        ),
        (
            "method replaced through setattr",
            "import hostile.settings as s\nsetattr(s.Invoice, 'total', lambda self: 120)\n",
        ),
    ] {
        let finding = classify_dc(&source, DC_NEW, &[("tests/conftest.py", other)], DC_TEST)?;
        assert_lambda_withheld(&finding, label);
    }
    let replaced_in_module = format!("{source}\n\nInvoice.total = lambda self: 120\n");
    let finding = classify_dc(&replaced_in_module, DC_NEW, &[], DC_TEST)?;
    assert_lambda_withheld(&finding, "method replaced in the owner module");
    let finding = classify_dc(
        &source,
        DC_NEW,
        &[],
        &dc_test("    Invoice.total = lambda self: 120\n    assert Invoice(100).total() == 120"),
    )?;
    assert_lambda_withheld(&finding, "method replaced by the test");
    let shadowed = "from hostile.settings import Invoice\n\nround = lambda value: 120\n\n\ndef test_it():\n    assert Invoice(100).total() == 120\n";
    let finding = classify_dc(&source, DC_NEW, &[], shadowed)?;
    assert_lambda_withheld(&finding, "round bound once by the test module");
    Ok(())
}

#[test]
fn a_required_field_call_is_not_a_default() -> Check {
    let source = format!(
        "from dataclasses import dataclass, field\n\n\n@dataclass\nclass Invoice:\n    amount: int = field(repr=False)\n{DC_NEW}\n\n{DC_METHOD}"
    );
    let finding = classify_dc(
        &source,
        DC_NEW,
        &[],
        &dc_test("    assert Invoice().total() == 0"),
    )?;
    assert_lambda_withheld(&finding, "field() without default leaves amount required");
    Ok(())
}

#[test]
fn a_pinned_dataclass_default_edit_is_credited() -> Check {
    let source = dc_default();
    for (label, test) in [
        ("method through the default", DC_TEST.to_string()),
        (
            "keyword constructor",
            dc_test("    assert Invoice(amount=100).total() == 120"),
        ),
        (
            "attribute read",
            dc_test("    assert Invoice(100).tax_rate == 0.2"),
        ),
        (
            "alias import",
            "from hostile.settings import Invoice as Bill\n\n\ndef test_it():\n    assert Bill(100).total() == 120\n"
                .to_string(),
        ),
        (
            "other fields passed",
            dc_test("    assert Invoice(100).total() == 120\n    assert Invoice(100, 0.5).total() == 150"),
        ),
    ] {
        let finding = classify_dc(&source, DC_NEW, &[], &test)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}: {finding:?}");
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with("related_test_relation: module_value_call")),
            "{label}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn a_reflective_store_through_a_reexported_module_withholds_credit() -> Check {
    let source = table_source(TABLE_NEW, "");
    let test = silver_test("assert price(\"silver\") == 5");
    let bridge = ("hostile/bridge.py", "from hostile import settings as hub\n");
    // The owner is named only in the bridge and `getattr` only in the store.
    let reflective = (
        "hostile/touch.py",
        "from hostile.bridge import hub\n\ngetattr(hub, \"RA\" + \"TES\")[\"silver\"] = 5\n",
    );
    assert_not_credited(&classify_table(
        &source,
        &[bridge, reflective],
        &test,
        TABLE_NEW,
    )?);
    // Control: the same re-export with no reflective access keeps the credit.
    let plain = (
        "hostile/touch.py",
        "from hostile.bridge import hub  # noqa: F401\n",
    );
    assert_credited(&classify_table(
        &source,
        &[bridge, plain],
        &test,
        TABLE_NEW,
    )?);
    Ok(())
}

#[test]
fn replacing_the_readers_code_withholds_credit() -> Check {
    let source = table_source(TABLE_NEW, "");
    for replacement in [
        "price.__code__ = frozen.__code__",
        "price.__defaults__ = (1,)",
        "price.__kwdefaults__ = {}",
        "price.__closure__ = ()",
    ] {
        let test = format!(
            "from hostile.settings import price\n\n\ndef frozen(tier):\n    return 5\n\n\n{replacement}\n\n\ndef test_it():\n    assert price(\"silver\") == 5\n"
        );
        assert_not_credited(&classify_table(&source, &[], &test, TABLE_NEW)?);
    }
    // Control: the same test without the replacement keeps the credit.
    assert_credited(&classify_table(
        &source,
        &[],
        &silver_test("assert price(\"silver\") == 5"),
        TABLE_NEW,
    )?);
    Ok(())
}

#[test]
fn replacing_a_scalar_readers_code_withholds_credit() -> Check {
    let source = "RATES = 6\n\n\ndef price(tier):\n    return RATES\n";
    for replacement in [
        "price.__code__ = frozen.__code__",
        "price.__defaults__ = (1,)",
        "price.__kwdefaults__ = {}",
        "price.__closure__ = ()",
    ] {
        let test = format!(
            "from hostile.settings import price\n\n\ndef frozen(tier):\n    return 5\n\n\n{replacement}\n\n\ndef test_it():\n    assert price(\"silver\") == 5\n"
        );
        assert_not_credited(&classify_table_edit(
            source,
            &[],
            &test,
            "RATES = 5",
            "RATES = 6",
        )?);
    }
    // Control: the same test without the replacement keeps the credit.
    assert_credited(&classify_table_edit(
        source,
        &[],
        &silver_test("assert price(\"silver\") == 5"),
        "RATES = 5",
        "RATES = 6",
    )?);
    Ok(())
}

#[test]
fn a_code_slot_assignment_replaces_readers_but_does_not_rebind_plain_names() {
    let own = Path::new("hostile/settings.py");
    let other = Path::new("tests/test_helper.py");
    let foreign = super::module_value::ForeignWrites::scan(
        own,
        [
            (own, "LIMIT = 1\n"),
            (other, "def f():\n    pass\n\nf.__code__ = f.__code__\n"),
        ]
        .into_iter(),
        false,
    );
    assert!(foreign.may_replace_reader_code());
    assert!(!foreign.may_rebind("version_info", Path::new("sys.py")));
    assert!(!foreign.may_rebind("LIMIT", own));
}

#[test]
fn reflection_only_withholds_credit_for_modules_it_can_reach() -> Check {
    let source = table_source(TABLE_NEW, "");
    let test = silver_test("assert price(\"silver\") == 5");
    // Unconnected reflection over an unrelated module keeps the credit.
    let unrelated = (
        "hostile/util.py",
        "import math\n\nPI = getattr(math, \"pi\")\n",
    );
    assert_credited(&classify_table(&source, &[unrelated], &test, TABLE_NEW)?);
    // A reflective helper that a test imports next to the owner may be handed
    // the module as an argument, so the component reaches the owner.
    let helper = (
        "hostile/util.py",
        "def read(module, name):\n    return getattr(module, name)\n",
    );
    let connected = "import hostile.util\nfrom hostile.settings import price\n\n\ndef test_it():\n    assert price(\"silver\") == 5\n";
    assert_not_credited(&classify_table(&source, &[helper], connected, TABLE_NEW)?);
    // Loading a module by name makes the reachable set unknown.
    let loader = (
        "hostile/util.py",
        "import importlib\n\nMODULE = getattr(importlib.import_module(\"x\"), \"y\")\n",
    );
    assert_not_credited(&classify_table(&source, &[loader], &test, TABLE_NEW)?);
    Ok(())
}

#[test]
fn multiline_imports_and_evaluated_code_withhold_credit() -> Check {
    let source = table_source(TABLE_NEW, "");
    let test = silver_test("assert price(\"silver\") == 5");
    // The bridge names the owner only on the continuation line of a
    // parenthesized import.
    // Brackets inside a comment or a string do not end that statement early.
    for bridge_source in [
        "from hostile import (\n    settings as hub,\n)\n",
        "from hostile import (  # )\n    settings as hub,\n)\n",
        "MARKER = \")\"; from hostile import (\n    settings as hub,\n)\n",
    ] {
        let bridge = ("hostile/bridge.py", bridge_source);
        let touch = (
            "hostile/touch.py",
            "from hostile.bridge import hub\n\ngetattr(hub, \"RA\" + \"TES\")[\"silver\"] = 5\n",
        );
        assert_not_credited(&classify_table(
            &source,
            &[bridge, touch],
            &test,
            TABLE_NEW,
        )?);
    }
    // Evaluated code can import any module, whatever the file names.
    for evaluated in [
        "import math\n\nexec(input())\n",
        "import math\n\neval(input())\n",
        "import math\n\ncompile(input(), \"x\", \"exec\")\n",
    ] {
        assert_not_credited(&classify_table(
            &source,
            &[("hostile/util.py", evaluated)],
            &test,
            TABLE_NEW,
        )?);
    }
    Ok(())
}

// Class-creation and construction effects the model does not evaluate (#9619).
#[test]
fn a_dataclass_whose_creation_or_construction_may_raise_is_not_credited() -> Check {
    let base = dc_default();
    let factory = "from dataclasses import dataclass, field\n\n\n@dataclass\nclass Invoice:\n    amount: int\n    tax_rate: float = 0.25\n    tags: list = field(default_factory=fail)\n\n    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n";
    for (label, source, test) in [
        ("failing default_factory", factory.to_string(), DC_TEST),
        (
            "call default on an async method",
            format!("{base}\n    async def other(self, extra=boom()):\n        return 1\n"),
            DC_TEST,
        ),
        (
            "call evaluated at class creation as a parameter default",
            format!("{base}\n    def other(self, extra=boom()):\n        return 1\n"),
            DC_TEST,
        ),
        (
            "builtin annotation rebound to a marker",
            base.replace("\n\n\n@dataclass", "\nfloat = ClassVar\n\n\n@dataclass"),
            DC_TEST,
        ),
        (
            "unknown decorator on another method",
            format!("{base}\n    @tweak\n    def other(self):\n        return 1\n"),
            DC_TEST,
        ),
        (
            "test takes a fixture that may patch the class",
            base.clone(),
            "from hostile.settings import Invoice\n\n\ndef test_it(rate):\n    assert Invoice(100).total() == 120\n",
        ),
    ] {
        let finding = classify_dc(&source, DC_NEW, &[], test)?;
        assert_lambda_withheld(&finding, label);
    }
    // A directly parametrized argument is not a fixture request.
    let parametrized = "import pytest\nfrom hostile.settings import Invoice\n\n\n@pytest.mark.parametrize(\"case\", [1])\ndef test_it(case):\n    assert Invoice(100).total() == 120\n";
    let finding = classify_dc(&base, DC_NEW, &[], parametrized)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // Builtin descriptors and `list`/`dict` factories keep the credit.
    let plain = format!(
        "{}\n    @property\n    def big(self):\n        return self.amount >= 1000\n",
        base.replace(
            "    amount: int\n",
            "    amount: int\n    tags: list = field(default_factory=list)\n"
        )
        .replace("import dataclass", "import dataclass, field")
    );
    let finding = classify_dc(&plain, DC_NEW, &[], DC_TEST)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn persisted_lexes_carry_the_loose_name_answer_the_index_uses() {
    // a.py spells a non-ASCII name, which only a lex finds. The workspace
    // pass records that answer with the file's lexes and a warm run serves
    // it from the fact store; the index built from it must match a fresh one.
    let sources = [
        (Path::new("a.py"), "import b\ncaf\u{e9} = 1\n"),
        (Path::new("b.py"), "LIMIT = 10\n"),
    ];
    assert!(super::module_value::ForeignLexes::of(sources[0].1).loose_names);
    assert!(!super::module_value::ForeignLexes::of("x = 'caf\u{e9}'\n").loose_names);
    assert!(!super::module_value::ForeignLexes::of(sources[1].1).loose_names);
    let parsed = super::module_value::ForeignIndex::build(sources.iter().copied(), false);
    assert!(
        parsed
            .excluding(Path::new("b.py"))
            .names_may_differ_elsewhere()
    );
    let prior: std::collections::BTreeMap<_, _> = sources
        .iter()
        .map(|(file, source)| {
            let facts = super::source_facts::extract_workspace_source_facts(file, source);
            (
                file.to_path_buf(),
                super::module_value::PriorFileFacts {
                    assigned: facts.assigned_attributes,
                    lexes: facts.foreign_lexes,
                },
            )
        })
        .collect();
    assert!(prior.values().all(|prior| prior.lexes.is_some()));
    let shared =
        super::module_value::ForeignIndex::build_with_prior(sources.iter().copied(), false, &prior);
    for (own, _) in &sources {
        assert_eq!(shared.excluding(own), parsed.excluding(own), "{own:?}");
    }
}

#[test]
fn foreign_index_records_package_init_submodule_shadows_on_sibling_modules() {
    let (shadows, opaque) = super::module_value::package_init_submodule_facts_for_test(
        Path::new("shapes/rates/__init__.py"),
        "from .table import lookup as table\n",
    );
    assert!(shadows.contains("table"), "{shadows:?}");
    assert!(!opaque);
    let index = super::module_value::ForeignIndex::build(
        [
            (
                Path::new("shapes/rates/__init__.py"),
                "from .table import lookup as table\n",
            ),
            (Path::new("shapes/rates/table.py"), "RATE_TABLE = {}\n"),
        ]
        .into_iter(),
        false,
    );
    let foreign = index.excluding(Path::new("shapes/rates/table.py"));
    assert!(
        super::module_value::package_init_refuses_submodule_for_test(
            Path::new("shapes/rates/table.py"),
            &foreign,
        )
    );
}

const RATES_TABLE_OLD: &str = "RATE_TABLE = {\"gold\": 10, \"silver\": 5}";

fn rates_table_source(first_line: &str, tail: &str) -> String {
    format!("{first_line}\n\n\ndef lookup(tier):\n    return RATE_TABLE.get(tier, 0)\n{tail}")
}

fn classify_shapes_rates_table(
    source: &str,
    others: &[(&str, &str)],
    test_source: &str,
    old: &str,
    new: &str,
) -> Result<Finding, String> {
    let own = Path::new("shapes/rates/table.py");
    let mut owners = extract_owners(own, source);
    let mut tests = tests_of(&[("tests/test_rates.py", test_source)]);
    let foreign = super::module_value::ForeignWrites::scan(
        own,
        others
            .iter()
            .map(|(file, source)| (Path::new(file), *source))
            .chain([
                (own, source),
                (Path::new("tests/test_rates.py"), test_source),
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
            (path == Path::new("tests/test_rates.py")).then_some(test_source)
        }
    });
    classify_change_with_context(
        own,
        1,
        new,
        Some(old),
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

#[test]
fn a_module_receiver_is_refused_when_the_package_init_rebinds_the_submodule_name() -> Check {
    let source = rates_table_source(RATES_TABLE_OLD, "");
    let test = "import shapes.rates.table as t\n\n\ndef test_gold():\n    assert t.lookup(\"gold\") == 10\n";
    let others = [
        ("shapes/__init__.py", ""),
        (
            "shapes/rates/__init__.py",
            "from .table import lookup as table\n",
        ),
    ];
    assert_not_credited(&classify_shapes_rates_table(
        &source,
        &others,
        test,
        RATES_TABLE_OLD,
        "RATE_TABLE = {\"gold\": 11, \"silver\": 5}",
    )?);
    Ok(())
}

#[test]
fn a_module_receiver_still_reaches_the_submodule_when_the_init_does_not_bind_its_name() -> Check {
    let source = rates_table_source(RATES_TABLE_OLD, "");
    let test = "import shapes.rates.table as t\n\n\ndef test_gold():\n    assert t.lookup(\"gold\") == 10\n";
    let others = [("shapes/__init__.py", ""), ("shapes/rates/__init__.py", "")];
    assert_credited(&classify_shapes_rates_table(
        &source,
        &others,
        test,
        RATES_TABLE_OLD,
        "RATE_TABLE = {\"gold\": 11, \"silver\": 5}",
    )?);
    Ok(())
}

#[test]
fn a_module_receiver_still_reaches_a_submodule_reexported_from_the_package_init() -> Check {
    let source = rates_table_source(RATES_TABLE_OLD, "");
    let test = "import shapes.rates.table as t\n\n\ndef test_gold():\n    assert t.lookup(\"gold\") == 10\n";
    let others = [
        ("shapes/__init__.py", ""),
        ("shapes/rates/__init__.py", "from . import table\n"),
    ];
    assert_credited(&classify_shapes_rates_table(
        &source,
        &others,
        test,
        RATES_TABLE_OLD,
        "RATE_TABLE = {\"gold\": 11, \"silver\": 5}",
    )?);
    Ok(())
}

const RATES_RECEIVER_TEST: &str =
    "import shapes.rates.table as t\n\n\ndef test_gold():\n    assert t.lookup(\"gold\") == 10\n";
const RATES_TABLE_NEW: &str = "RATE_TABLE = {\"gold\": 11, \"silver\": 5}";

fn rates_receiver_with_inits(shapes_init: &str, rates_init: &str) -> Result<Finding, String> {
    let source = rates_table_source(RATES_TABLE_OLD, "");
    let others = [
        ("shapes/__init__.py", shapes_init),
        ("shapes/rates/__init__.py", rates_init),
    ];
    classify_shapes_rates_table(
        &source,
        &others,
        RATES_RECEIVER_TEST,
        RATES_TABLE_OLD,
        RATES_TABLE_NEW,
    )
}

#[test]
fn a_nested_or_unpacked_init_binding_of_the_submodule_name_refuses_the_receiver() -> Check {
    let shadowing_inits = [
        "try:\n    from .table import lookup as table\nexcept ImportError:\n    table = None\n",
        "if True:\n    table = alternate_table\n",
        "if flag:\n    pass\nelse:\n    table = 1\n",
        "table, other = alternate_table, 1\n",
        "[other, table] = pair\n",
        "other, *table = pair\n",
        "del table\n",
        "for table in tables:\n    pass\n",
        "with opener() as table:\n    pass\n",
        "while go():\n    table = 1\n",
        "try:\n    pass\nexcept ImportError as table:\n    pass\n",
        "if flag:\n    from .other import *\n",
        "match command:\n    case other:\n        pass\n",
    ];
    for init in shadowing_inits {
        assert_not_credited(&rates_receiver_with_inits("", init)?);
    }
    Ok(())
}

#[test]
fn a_nested_init_binding_of_other_names_keeps_the_receiver_credited() -> Check {
    let benign_inits = [
        "try:\n    from .other import thing\nexcept ImportError:\n    thing = None\n",
        "if flag:\n    other = 1\n",
        "other, extra = 1, 2\n",
        "for item in tables:\n    pass\n",
        "def configure():\n    table = 1\n",
        "class Holder:\n    table = 1\n",
        "if flag:\n    from . import table\n",
    ];
    for init in benign_inits {
        assert_credited(&rates_receiver_with_inits("", init)?);
    }
    Ok(())
}

#[test]
fn an_ancestor_package_init_rebinding_a_path_segment_refuses_the_receiver() -> Check {
    for shapes_init in [
        "rates = alternate_rates\n",
        "if flag:\n    rates = alternate_rates\n",
        "from .elsewhere import *\n",
    ] {
        assert_not_credited(&rates_receiver_with_inits(shapes_init, "")?);
    }
    Ok(())
}

#[test]
fn an_ancestor_package_init_binding_other_names_keeps_the_receiver_credited() -> Check {
    for shapes_init in [
        "other = 1\n",
        "from . import rates\n",
        "if flag:\n    extra = 1\n",
    ] {
        assert_credited(&rates_receiver_with_inits(shapes_init, "")?);
    }
    Ok(())
}

#[test]
fn package_init_prefilter_skips_inert_sources_without_parsing() {
    let init = Path::new("pkg/__init__.py");
    let skipped = [
        "",
        "   \n\t\n",
        "# just a comment\n",
        "#!/usr/bin/env python3\n# coding: utf-8\n",
        "    # indented comment only\n",
        "\"\"\"package docs\"\"\"\n",
        "'''package docs'''\n",
        "\"docs\"\n",
        "r\"\"\"raw docs\"\"\"\n",
        "# comment\n\n\"\"\"docs\"\"\"\n# trailing\n",
    ];
    for source in skipped {
        let (shadows, opaque, parsed) =
            super::module_value::package_init_submodule_facts_parsed_for_test(init, source);
        assert!(
            shadows.is_empty() && !opaque && !parsed,
            "prefilter skip {source:?}: shadows={shadows:?} opaque={opaque} parsed={parsed}"
        );
        let parsed_facts =
            super::module_value::package_init_submodule_facts_from_parse_for_test(init, source);
        assert_eq!(
            parsed_facts,
            (shadows, opaque),
            "prefilter skip disagrees with parse for {source:?}"
        );
    }
}

#[test]
fn package_init_prefilter_parses_sources_that_may_bind() {
    let init = Path::new("pkg/__init__.py");
    let must_parse = [
        "from .a import *\n",
        "x = 1\n",
        "def a():\n    return 1\n",
        "from . import a as b\n",
        "# comment\n    x = 1\n",
        "s = \"#\"\n",
    ];
    for source in must_parse {
        let (shadows, opaque, parsed) =
            super::module_value::package_init_submodule_facts_parsed_for_test(init, source);
        assert!(parsed, "must parse {source:?}");
        let parsed_facts =
            super::module_value::package_init_submodule_facts_from_parse_for_test(init, source);
        assert_eq!(
            parsed_facts,
            (shadows, opaque),
            "parsed facts drifted for {source:?}"
        );
    }
    let (shadows, opaque, parsed) =
        super::module_value::package_init_submodule_facts_parsed_for_test(
            init,
            "from .a import *\n",
        );
    assert!(parsed && opaque && shadows.is_empty(), "{shadows:?}");
    let (shadows, opaque, _) =
        super::module_value::package_init_submodule_facts_parsed_for_test(init, "x = 1\n");
    assert!(!opaque && shadows.contains("x"), "{shadows:?}");
    let (shadows, opaque, _) = super::module_value::package_init_submodule_facts_parsed_for_test(
        init,
        "def a():\n    return 1\n",
    );
    assert!(!opaque && shadows.contains("a"), "{shadows:?}");
    let (shadows, opaque, _) = super::module_value::package_init_submodule_facts_parsed_for_test(
        init,
        "from . import a as b\n",
    );
    assert!(!opaque && shadows.contains("b"), "{shadows:?}");
    let (shadows, opaque, _) =
        super::module_value::package_init_submodule_facts_parsed_for_test(init, "s = \"#\"\n");
    assert!(!opaque && shadows.contains("s"), "{shadows:?}");
}

#[test]
fn package_init_prefilter_skips_parse_across_many_inert_inits() {
    let init = Path::new("pkg/__init__.py");
    let mut parsed = 0usize;
    for i in 0..400 {
        let source = if i % 2 == 0 { "" } else { "# pkg\n" };
        let (_, _, did_parse) =
            super::module_value::package_init_submodule_facts_parsed_for_test(init, source);
        if did_parse {
            parsed += 1;
        }
    }
    assert_eq!(parsed, 0, "inert package inits must not reparse");
}

#[test]
fn package_init_prefilter_ignores_non_init_files() {
    let (shadows, opaque, parsed) =
        super::module_value::package_init_submodule_facts_parsed_for_test(
            Path::new("pkg/mod.py"),
            "from .a import *\n",
        );
    assert!(shadows.is_empty(), "{shadows:?}");
    assert!(!opaque);
    assert!(!parsed);
}
