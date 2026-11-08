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
        if *path == TEST_FILE {
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

#[test]
fn given_a_module_name_the_test_can_rebind_or_the_import_cannot_prove_when_called_then_nothing_resolves()
-> Result<(), String> {
    let init_binds = "conftest = object()\n";
    let cases: [(&str, &str, Option<&str>); 6] = [
        // The test replaces the module's helper before the call.
        (
            "from . import conftest",
            "    conftest.check_equal = print\n    conftest.check_equal(sq(3), 9)",
            None,
        ),
        // The test body rebinds the module name.
        (
            "from . import conftest",
            "    conftest = object()\n    conftest.check_equal(sq(3), 9)",
            None,
        ),
        // The package `__init__` binds the name, so it is not the submodule.
        (
            "from . import conftest",
            "    conftest.check_equal(sq(3), 9)",
            Some(init_binds),
        ),
        // A bare `import conftest` resolves through `sys.path`.
        (
            "import conftest",
            "    conftest.check_equal(sq(3), 9)",
            None,
        ),
        // Two imports of one alias leave the bound module order-dependent.
        (
            "from . import conftest\nimport other as conftest",
            "    conftest.check_equal(sq(3), 9)",
            None,
        ),
        // A keyword argument is not substituted by position.
        (
            "from . import conftest",
            "    conftest.check_equal(sq(3), expected=9)",
            None,
        ),
    ];
    for (imports, body, init) in cases {
        let source = test_source(imports, body);
        let mut files = vec![(TEST_FILE, source.as_str()), (CONFTEST, EQ_HELPER)];
        if let Some(init) = init {
            files.push(("tests/__init__.py", init));
        }
        let test = resolved(&files)?;
        assert!(
            test.assertions
                .iter()
                .all(|assertion| assertion.gate_text != "assert sq(3) == 9"),
            "{imports} / {body}"
        );
    }
    Ok(())
}
