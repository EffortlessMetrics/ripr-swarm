//! A name a test package's `__init__.py` binds by import (#13927),
//! established end to end through `analyze_diff` on tomli-shaped
//! workspaces: `tests/__init__.py` binds `import tomli as tomllib` and the
//! test module writes `from . import tomllib`.

use super::reexport_tests::analyze_one_line;
use super::*;

const PARSER_PY: &str =
    "def loads(text):\n    if text == \"\":\n        return {}\n    return {\"key\": text}\n";
const PACKAGE_INIT: &str = "from ._parser import loads\n";

/// Analyzes the changed `if text == "":` line of `_parser.loads` with the
/// given test package initializer, test import line and extra files.
fn tomli_case(
    label: &str,
    tests_init: Option<&str>,
    test_import: &str,
    extra: &[(&str, &str)],
) -> Result<Finding, String> {
    // The receiver is the test's local name for the bound module.
    let receiver = test_import.rsplit(' ').next().unwrap_or_default();
    let test = format!(
        "{test_import}\n\n\ndef test_loads():\n    assert {receiver}.loads(\"\") == {{}}\n"
    );
    let mut files: Vec<(&str, &str)> = vec![
        ("src/tomli/_parser.py", PARSER_PY),
        ("src/tomli/__init__.py", PACKAGE_INIT),
        ("tests/test_misc.py", &test),
    ];
    if let Some(init) = tests_init {
        files.push(("tests/__init__.py", init));
    }
    files.extend_from_slice(extra);
    analyze_one_line(label, &files, "src/tomli/_parser.py", 2)
}

fn related_names(finding: &Finding) -> Vec<&str> {
    finding
        .related_tests
        .iter()
        .map(|test| test.name.as_str())
        .collect()
}

fn expect_linked(label: &str, finding: &Finding) -> Result<(), String> {
    if finding.class != ExposureClass::Exposed || related_names(finding) != ["test_loads"] {
        return Err(format!(
            "{label}: expected exposed with test_loads related, got {:?} with {:?}; evidence: {:?}",
            finding.class,
            related_names(finding),
            finding.evidence
        ));
    }
    Ok(())
}

fn expect_unlinked(label: &str, finding: &Finding) -> Result<(), String> {
    if !finding.related_tests.is_empty() || finding.class == ExposureClass::Exposed {
        return Err(format!(
            "{label}: expected no related test, got {:?} with {:?}",
            finding.class,
            related_names(finding)
        ));
    }
    Ok(())
}

#[test]
fn direct_aliased_import_is_the_baseline_link() -> Result<(), String> {
    let finding = tomli_case("direct-alias", None, "import tomli as tomllib", &[])?;
    expect_linked("import tomli as tomllib", &finding)
}

#[test]
fn relative_import_of_a_name_the_test_package_binds_by_import_links_like_the_direct_import()
-> Result<(), String> {
    let finding = tomli_case(
        "init-alias",
        Some("__all__ = (\"tomllib\",)\n\nimport tomli as tomllib\n"),
        "from . import tomllib",
        &[],
    )?;
    expect_linked("from . import tomllib", &finding)
}

#[test]
fn a_local_rename_of_the_package_bound_name_keeps_the_link() -> Result<(), String> {
    let finding = tomli_case(
        "init-alias-local-rename",
        Some("import tomli as tomllib\n"),
        "from . import tomllib as toml",
        &[],
    )?;
    expect_linked("from . import tomllib as toml", &finding)
}

#[test]
fn a_from_import_binding_in_the_test_package_links_to_its_source_module() -> Result<(), String> {
    let finding = tomli_case(
        "init-from-alias",
        Some("from tomli import _parser as tomllib\n"),
        "from . import tomllib",
        &[],
    )?;
    expect_linked("from tomli import _parser as tomllib", &finding)
}

#[test]
fn an_unbound_name_in_the_test_package_stays_unlinked() -> Result<(), String> {
    let finding = tomli_case("init-unbound", Some(""), "from . import tomllib", &[])?;
    expect_unlinked("empty tests/__init__.py", &finding)?;
    let finding = tomli_case("init-missing", None, "from . import tomllib", &[])?;
    expect_unlinked("no tests/__init__.py", &finding)
}

#[test]
fn a_rebound_or_conditional_package_name_stays_unlinked() -> Result<(), String> {
    let cases = [
        ("rebound", "import tomli as tomllib\ntomllib = None\n"),
        (
            "conditional",
            "try:\n    import tomllib\nexcept ImportError:\n    import tomli as tomllib\n",
        ),
        ("star", "import tomli as tomllib\nfrom helpers import *\n"),
        ("deleted", "import tomli as tomllib\ndel tomllib\n"),
        (
            "nested-only",
            "import sys\nif sys.version_info >= (3, 8):\n    import tomli as tomllib\n",
        ),
        ("two-aliases", "import json as tomllib, tomli as tomllib\n"),
        ("dotted", "import tomllib.sub\n"),
        ("walrus", "import tomli as tomllib\n(tomllib := None)\n"),
        (
            "locals",
            "import tomli as tomllib\nlocals()[\"tomllib\"] = None\n",
        ),
        (
            "vars-spaced",
            "import tomli as tomllib\nvars ()[\"tomllib\"] = None\n",
        ),
        (
            "exec-spaced",
            "import tomli as tomllib\nexec (\"tomllib = None\")\n",
        ),
        (
            "locals-spaced",
            "import tomli as tomllib\nlocals ()[\"tomllib\"] = None\n",
        ),
        (
            "eval-walrus",
            "import tomli as tomllib\neval (\"(tomllib := None)\")\n",
        ),
        (
            "sys-modules-spaced",
            "import sys\nimport tomli as tomllib\nsetattr (sys . modules[__name__], \"tomllib\", None)\n",
        ),
        (
            "from-sys-modules",
            "from sys import modules\nimport tomli as tomllib\nmodules[__name__].tomllib = None\n",
        ),
        (
            "match-capture",
            "import tomli as tomllib\nmatch 1:\n    case tomllib:\n        pass\n",
        ),
        (
            "dynamic",
            "import tomli as tomllib\nglobals()[\"tomllib\"] = object()\n",
        ),
        (
            "module-getattr",
            "import tomli as tomllib\n\n\ndef __getattr__(name):\n    return None\n",
        ),
    ];
    for (label, init) in cases {
        let finding = tomli_case(label, Some(init), "from . import tomllib", &[])?;
        expect_unlinked(label, &finding)?;
    }
    Ok(())
}

#[test]
fn a_same_named_submodule_of_the_test_package_stays_unlinked() -> Result<(), String> {
    let finding = tomli_case(
        "init-submodule-shadow",
        Some("import tomli as tomllib\n"),
        "from . import tomllib",
        &[("tests/tomllib.py", "def loads(text):\n    return {}\n")],
    )?;
    expect_unlinked("tests/tomllib.py beside the binding", &finding)
}

#[test]
fn a_same_named_subpackage_of_the_test_package_stays_unlinked() -> Result<(), String> {
    let finding = tomli_case(
        "init-subpackage-shadow",
        Some("import tomli as tomllib\n"),
        "from . import tomllib",
        &[("tests/tomllib/__init__.py", "")],
    )?;
    expect_unlinked("tests/tomllib/ beside the binding", &finding)
}

#[test]
fn an_absolute_import_of_the_containing_test_package_links() -> Result<(), String> {
    let finding = tomli_case(
        "init-absolute",
        Some("import tomli as tomllib\n"),
        "from tests import tomllib",
        &[],
    )?;
    expect_linked("from tests import tomllib", &finding)
}

#[test]
fn a_package_that_does_not_contain_the_test_is_not_followed() -> Result<(), String> {
    // `support/__init__.py` binds the name, but the test lives in `tests/`:
    // in a nested project the dotted name can be another package.
    let finding = tomli_case(
        "init-not-ancestor",
        None,
        "from support import tomllib",
        &[("support/__init__.py", "import tomli as tomllib\n")],
    )?;
    expect_unlinked("from support import tomllib", &finding)
}

#[test]
fn a_parent_package_binding_links_through_a_two_level_relative_import() -> Result<(), String> {
    let finding = analyze_one_line(
        "init-level-two",
        &[
            ("src/tomli/_parser.py", PARSER_PY),
            ("src/tomli/__init__.py", PACKAGE_INIT),
            ("tests/__init__.py", "import tomli as tomllib\n"),
            ("tests/unit/__init__.py", ""),
            (
                "tests/unit/test_misc.py",
                "from .. import tomllib\n\n\ndef test_loads():\n    assert tomllib.loads(\"\") == {}\n",
            ),
        ],
        "src/tomli/_parser.py",
        2,
    )?;
    expect_linked("from .. import tomllib", &finding)
}

#[test]
fn a_chain_through_the_parent_package_initializer_links() -> Result<(), String> {
    // `tests/unit/__init__.py` re-binds the parent's name; two hops.
    let finding = analyze_one_line(
        "init-two-hops",
        &[
            ("src/tomli/_parser.py", PARSER_PY),
            ("src/tomli/__init__.py", PACKAGE_INIT),
            ("tests/__init__.py", "import tomli as tomllib\n"),
            ("tests/unit/__init__.py", "from .. import tomllib\n"),
            (
                "tests/unit/test_misc.py",
                "from . import tomllib\n\n\ndef test_loads():\n    assert tomllib.loads(\"\") == {}\n",
            ),
        ],
        "src/tomli/_parser.py",
        2,
    )?;
    expect_linked("two package hops", &finding)
}

#[test]
fn a_name_bound_to_an_unrelated_module_stays_unlinked() -> Result<(), String> {
    let finding = tomli_case(
        "init-other-module",
        Some("import json as tomllib\n"),
        "from . import tomllib",
        &[],
    )?;
    expect_unlinked("import json as tomllib", &finding)
}
