//! Project-relative module names (`related_tests::apply_project_module_names`),
//! established end to end through `analyze_diff`: a module of a nested Python
//! project is imported by its project-relative name (`from core import limit`
//! in `svc/b/tests`), and a same-named module of a sibling project is not it.

use super::reexport_tests::analyze_one_line;
use super::*;

const PROJECT: &str = "[project]\nname = \"p\"\nversion = \"0\"\n";
const CORE_PY: &str = "def limit(n):\n    return n > 10\n";
const STRONG_TEST: &str = "from core import limit\n\n\ndef test_limit():\n    assert limit(10) == False\n    assert limit(11) == True\n";

fn classify(label: &str, test_project: &str) -> Result<ExposureClass, String> {
    let test_path = format!("svc/{test_project}/tests/test_core.py");
    let finding = analyze_one_line(
        label,
        &[
            ("svc/a/pyproject.toml", PROJECT),
            ("svc/b/pyproject.toml", PROJECT),
            ("svc/a/core.py", CORE_PY),
            ("svc/b/core.py", CORE_PY),
            (&test_path, STRONG_TEST),
        ],
        "svc/a/core.py",
        2,
    )?;
    Ok(finding.class)
}

#[test]
fn a_nested_project_test_importing_a_flat_module_credits_its_own_project() -> Result<(), String> {
    let class = classify("layout-own-project", "a")?;
    if class != ExposureClass::Exposed {
        return Err(format!("own-project strong test gave {class:?}"));
    }
    Ok(())
}

#[test]
fn a_sibling_project_test_importing_the_same_flat_name_credits_nothing() -> Result<(), String> {
    let class = classify("layout-sibling-project", "b")?;
    if class == ExposureClass::Exposed {
        return Err("a sibling project's `core` test credited this project's `core`".to_string());
    }
    Ok(())
}

#[test]
fn a_nested_project_flat_module_without_a_marker_keeps_the_old_behavior() -> Result<(), String> {
    // No project marker: no project-relative name is recorded, so the bare
    // import is not claimed for the module.
    let finding = analyze_one_line(
        "layout-no-marker",
        &[
            ("svc/a/core.py", CORE_PY),
            ("svc/a/tests/test_core.py", STRONG_TEST),
        ],
        "svc/a/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("an unmarked nested directory claimed a bare module name".to_string());
    }
    Ok(())
}

#[test]
fn a_rival_definition_import_ending_in_the_project_name_stays_withheld() -> Result<(), String> {
    // `other.core` ends with the owner's project-relative name `core`; the
    // short name must not lift the rival-definition withholding.
    let finding = analyze_one_line(
        "layout-rival-suffix",
        &[
            ("svc/b/pyproject.toml", PROJECT),
            ("svc/b/core.py", CORE_PY),
            ("other/core.py", CORE_PY),
            (
                "tests/test_other.py",
                "from other.core import limit\n\n\ndef test_limit():\n    assert limit(10) == False\n    assert limit(11) == True\n",
            ),
        ],
        "svc/b/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("a test of `other.core` credited `svc/b/core.py`".to_string());
    }
    Ok(())
}

#[test]
fn a_sibling_project_package_attribute_call_credits_nothing() -> Result<(), String> {
    // `import pkg` then `pkg.core.limit(...)` in project b names b's own
    // `pkg/core.py`, never project a's.
    let attribute_test = "import pkg\n\n\ndef test_limit():\n    assert pkg.core.limit(10) == False\n    assert pkg.core.limit(11) == True\n";
    for (test_project, own) in [("a", true), ("b", false)] {
        let test_path = format!("svc/{test_project}/tests/test_core.py");
        let finding = analyze_one_line(
            &format!("layout-package-attribute-{test_project}"),
            &[
                ("svc/a/pyproject.toml", PROJECT),
                ("svc/b/pyproject.toml", PROJECT),
                ("svc/a/pkg/__init__.py", ""),
                ("svc/b/pkg/__init__.py", ""),
                ("svc/a/pkg/core.py", CORE_PY),
                ("svc/b/pkg/core.py", CORE_PY),
                (&test_path, attribute_test),
            ],
            "svc/a/pkg/core.py",
            2,
        )?;
        let exposed = finding.class == ExposureClass::Exposed;
        if own != exposed {
            return Err(format!(
                "`pkg.core.limit` test in project {test_project} gave {:?}",
                finding.class
            ));
        }
    }
    Ok(())
}

#[test]
fn a_project_name_without_a_rival_credits_only_tests_inside_the_project() -> Result<(), String> {
    // No other `core` exists, yet a repo-root test importing `core` is not a
    // test of `svc/a/core.py`: the project-relative name is scoped to `svc/a`.
    let finding = analyze_one_line(
        "layout-outside-root",
        &[
            ("svc/a/pyproject.toml", PROJECT),
            ("svc/a/core.py", CORE_PY),
            ("tests/test_core.py", STRONG_TEST),
        ],
        "svc/a/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("a test outside `svc/a` credited its project-relative name".to_string());
    }
    Ok(())
}

#[test]
fn a_rival_definition_module_equal_to_the_project_name_stays_withheld() -> Result<(), String> {
    // The rival `x/src/core.py` is importable as `core`, the owner's
    // project-relative name too; a test outside the owner's project importing
    // `from core import limit` is the rival's.
    let finding = analyze_one_line(
        "layout-rival-exact",
        &[
            ("svc/b/pyproject.toml", PROJECT),
            ("svc/b/core.py", CORE_PY),
            ("x/src/core.py", CORE_PY),
            ("x/tests/test_core.py", STRONG_TEST),
        ],
        "svc/b/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("a rival `core` test credited `svc/b/core.py`".to_string());
    }
    Ok(())
}
