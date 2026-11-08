//! A helper reached as `module.helper(...)` through `from . import conftest`
//! or `import pkg.conftest as conftest` resolves like `from M import helper`
//! (#13629). Each opaque or rebindable shape stays unresolved.

use super::helper_assertions::apply_helper_assertions;
use super::source_facts::extract_source_facts;
use super::*;
use std::collections::BTreeMap;

const TEST_FILE: &str = "tests/test_sq.py";
const CONFTEST: &str = "tests/conftest.py";
const EQ_HELPER: &str = "def check_equal(actual, expected):\n    assert actual == expected\n";

fn test_source(imports: &str, body: &str) -> String {
    format!("from pkg.sq import sq\n{imports}\n\n\ndef test_sq():\n{body}\n")
}

/// The only test of the test file, after helper resolution over all files.
fn resolved(files: &[(&str, &str)]) -> Result<PythonTest, String> {
    let mut modules = BTreeMap::new();
    let mut tests = Vec::new();
    for (path, source) in files {
        let facts = extract_source_facts(Path::new(path), source);
        if path.ends_with("test_sq.py") {
            tests.extend(facts.tests);
        }
        modules.insert(PathBuf::from(*path), facts.helper_module);
    }
    apply_helper_assertions(&mut tests, &modules);
    if tests.len() != 1 {
        return Err("fixture must parse to one test".to_string());
    }
    Ok(tests.remove(0))
}

fn gate_texts(test: &PythonTest) -> Vec<&str> {
    test.assertions
        .iter()
        .map(|assertion| assertion.gate_text.as_str())
        .collect()
}

#[test]
fn given_a_helper_called_through_a_relative_conftest_module_when_resolved_then_it_reads_as_the_exact_assertion()
-> Result<(), String> {
    let source = test_source(
        "from . import conftest",
        "    conftest.check_equal(sq(3), 9)",
    );
    let test = resolved(&[(TEST_FILE, &source), (CONFTEST, EQ_HELPER)])?;

    assert_eq!(gate_texts(&test), ["assert sq(3) == 9"]);
    assert_eq!(
        test.assertions[0].oracle_shape,
        PythonOracleShape::ExactAssertion
    );
    assert_eq!(
        test.assertion_admission,
        admission::PythonAssertionAdmission::Recognized
    );
    Ok(())
}

#[test]
fn given_a_helper_called_through_an_aliased_dotted_import_when_resolved_then_it_reads_as_the_assertion()
-> Result<(), String> {
    let source = test_source(
        "import tests.conftest as support",
        "    support.check_equal(sq(3), 9)",
    );
    let test = resolved(&[(TEST_FILE, &source), (CONFTEST, EQ_HELPER)])?;

    assert_eq!(gate_texts(&test), ["assert sq(3) == 9"]);
    Ok(())
}

#[test]
fn given_a_weak_conftest_helper_when_called_through_the_module_then_it_stays_weak()
-> Result<(), String> {
    let weak = "def check_equal(actual, floor):\n    assert actual > floor\n";
    let source = test_source(
        "from . import conftest",
        "    conftest.check_equal(sq(3), 0)",
    );
    let test = resolved(&[(TEST_FILE, &source), (CONFTEST, weak)])?;

    assert_eq!(test.assertions.len(), 1);
    assert_eq!(test.assertions[0].oracle_strength, OracleStrength::Weak);
    Ok(())
}

#[test]
fn given_a_conftest_helper_that_does_not_assert_when_called_through_the_module_then_nothing_resolves()
-> Result<(), String> {
    let opaque = "def check_equal(actual, expected):\n    print(actual)\n";
    let source = test_source(
        "from . import conftest",
        "    conftest.check_equal(sq(3), 9)",
    );
    let test = resolved(&[(TEST_FILE, &source), (CONFTEST, opaque)])?;

    assert!(test.assertions.is_empty(), "{:?}", gate_texts(&test));
    Ok(())
}

/// Each case must leave the call unresolved: the findings carry no assertion.
fn assert_unresolved(cases: &[(&str, &str, Vec<(&str, &str)>)]) -> Result<(), String> {
    let mut credited = Vec::new();
    for (imports, body, extra) in cases {
        let source = test_source(imports, body);
        let mut files = vec![(TEST_FILE, source.as_str()), (CONFTEST, EQ_HELPER)];
        files.extend(extra.iter().copied());
        let test = resolved(&files)?;
        if !test.assertions.is_empty() {
            credited.push(format!("{imports} / {body}: {:?}", gate_texts(&test)));
        }
    }
    assert!(credited.is_empty(), "wrongly credited: {credited:#?}");
    Ok(())
}

const CALL: &str = "    conftest.check_equal(sq(3), 9)";

#[test]
fn given_a_module_name_the_test_can_rebind_or_the_import_cannot_prove_when_called_then_nothing_resolves()
-> Result<(), String> {
    assert_unresolved(&[
        // The test replaces the module's helper before the call.
        (
            "from . import conftest",
            "    conftest.check_equal = print\n    conftest.check_equal(sq(3), 9)",
            vec![],
        ),
        // The test body rebinds the module name.
        (
            "from . import conftest",
            "    conftest = object()\n    conftest.check_equal(sq(3), 9)",
            vec![],
        ),
        // The test module rebinds the name after the import (`module.bound`).
        ("from . import conftest\nconftest = object()", CALL, vec![]),
        // The package `__init__` binds the name, so it is not the submodule.
        (
            "from . import conftest",
            CALL,
            vec![("tests/__init__.py", "conftest = object()\n")],
        ),
        // A bare `import conftest` resolves through `sys.path`.
        ("import conftest", CALL, vec![]),
        // Two imports of one alias leave the bound module order-dependent.
        (
            "from . import conftest\nimport other as conftest",
            CALL,
            vec![],
        ),
        // A keyword argument is not substituted by position.
        (
            "from . import conftest",
            "    conftest.check_equal(sq(3), expected=9)",
            vec![],
        ),
    ])
}

#[test]
fn given_an_alias_another_import_or_del_can_rebind_when_called_then_nothing_resolves()
-> Result<(), String> {
    assert_unresolved(&[
        // A conditional import elsewhere in the file may win the binding.
        (
            "from . import conftest\ntry:\n    import fastconf as conftest\nexcept ImportError:\n    pass",
            CALL,
            vec![],
        ),
        // An import inside the test body rebinds before the call.
        (
            "from . import conftest",
            "    import fastconf as conftest\n    conftest.check_equal(sq(3), 9)",
            vec![],
        ),
        // A module-level `del` unbinds the alias.
        ("from . import conftest\ndel conftest", CALL, vec![]),
        // A `del` inside the test body unbinds the alias.
        (
            "from . import conftest",
            "    del conftest\n    conftest.check_equal(sq(3), 9)",
            vec![],
        ),
    ])
}

#[test]
fn given_an_undotted_or_escaping_import_when_called_then_it_does_not_credit_the_root_module()
-> Result<(), String> {
    let root = vec![("conftest.py", EQ_HELPER)];
    let call = "    c.check_equal(sq(3), 9)";
    assert_unresolved(&[
        // An undotted aliased import resolves through `sys.path`.
        ("import conftest as c", call, root.clone()),
        // A relative import above the top package has no workspace module.
        ("from ... import conftest as c", call, root.clone()),
        // Likewise one that lands on the workspace root.
        ("from .. import conftest as c", call, root),
    ])
}

#[test]
fn given_a_relative_import_inside_a_nested_package_when_resolved_then_it_reads_as_the_assertion()
-> Result<(), String> {
    let source = "from pkg.sq import sq\nfrom .. import conftest as c\n\n\ndef test_sq():\n    c.check_equal(sq(3), 9)\n";
    let test = resolved(&[
        ("pkg/tests/test_sq.py", source),
        ("pkg/conftest.py", EQ_HELPER),
    ])?;

    assert_eq!(gate_texts(&test), ["assert sq(3) == 9"]);
    Ok(())
}

#[test]
fn given_the_helper_attribute_is_patched_when_called_then_nothing_resolves() -> Result<(), String> {
    assert_unresolved(&[
        (
            "from . import conftest",
            "    monkeypatch.setattr(conftest, \"check_equal\", print)\n    conftest.check_equal(sq(3), 9)",
            vec![],
        ),
        (
            "from . import conftest",
            "    setattr(conftest, \"check_equal\", print)\n    conftest.check_equal(sq(3), 9)",
            vec![],
        ),
        (
            "from . import conftest\nfrom unittest import mock",
            "    with mock.patch.object(conftest, \"check_equal\"):\n        pass\n    conftest.check_equal(sq(3), 9)",
            vec![],
        ),
        (
            "from . import conftest\nfrom unittest.mock import patch",
            "    with patch(\"tests.conftest.check_equal\"):\n        pass\n    conftest.check_equal(sq(3), 9)",
            vec![],
        ),
        // A module-level assignment replaces the helper for every test.
        (
            "from . import conftest\nconftest.check_equal = print",
            CALL,
            vec![],
        ),
    ])
}

#[test]
fn given_the_helper_is_patched_outside_the_test_body_when_called_then_nothing_resolves()
-> Result<(), String> {
    assert_unresolved(&[
        // A module-level started patch replaces the helper for every test.
        (
            "from . import conftest\nfrom unittest import mock\nmock.patch.object(conftest, \"check_equal\").start()",
            CALL,
            vec![],
        ),
        // An autouse fixture in the same file patches the helper.
        (
            "from . import conftest\nimport pytest\n\n\n@pytest.fixture(autouse=True)\ndef _quiet(monkeypatch):\n    monkeypatch.setattr(conftest, \"check_equal\", print)",
            CALL,
            vec![],
        ),
        // A module-level `setattr` replaces the helper.
        (
            "from . import conftest\nsetattr(conftest, \"check_equal\", print)",
            CALL,
            vec![],
        ),
        // A star import after the module import may rebind its name.
        (
            "from . import conftest\nfrom fastconf import *",
            CALL,
            vec![],
        ),
    ])
}
