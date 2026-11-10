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

#[test]
fn a_nested_project_lib_module_is_not_its_bare_name() -> Result<(), String> {
    // Inside a project, `lib/core.py` is `lib.core`; `from core import` loads
    // the project's own `core.py`, so it is no evidence for `lib/core.py`.
    let finding = analyze_one_line(
        "layout-nested-lib",
        &[
            ("svc/a/pyproject.toml", PROJECT),
            ("svc/a/core.py", CORE_PY),
            ("svc/a/lib/core.py", CORE_PY),
            ("svc/a/tests/test_core.py", STRONG_TEST),
        ],
        "svc/a/lib/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("`from core import` credited `svc/a/lib/core.py`".to_string());
    }
    Ok(())
}

#[test]
fn a_module_beside_a_same_named_package_takes_no_credit_from_the_name() -> Result<(), String> {
    // `pkg.py` and `pkg/__init__.py` in one project both spell `pkg`; Python
    // loads the package, so a `from pkg import` test is not evidence for
    // `pkg.py`.
    let finding = analyze_one_line(
        "layout-module-vs-package",
        &[
            ("svc/a/pyproject.toml", PROJECT),
            ("svc/a/pkg.py", CORE_PY),
            ("svc/a/pkg/__init__.py", CORE_PY),
            (
                "svc/a/tests/test_pkg.py",
                "from pkg import limit\n\n\ndef test_limit():\n    assert limit(10) == False\n    assert limit(11) == True\n",
            ),
        ],
        "svc/a/pkg.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("a `pkg` package test credited the shadowed `pkg.py`".to_string());
    }
    Ok(())
}

#[test]
fn a_lone_nested_project_lib_module_is_not_its_bare_name() -> Result<(), String> {
    // No top-level `core.py` to collide with: the project's `lib/` is still
    // not an import root, so `from core import` names nothing in the project.
    let finding = analyze_one_line(
        "layout-lone-nested-lib",
        &[
            ("svc/a/pyproject.toml", PROJECT),
            ("svc/a/lib/core.py", CORE_PY),
            ("svc/a/tests/test_core.py", STRONG_TEST),
        ],
        "svc/a/lib/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("`from core import` credited a lone `svc/a/lib/core.py`".to_string());
    }
    Ok(())
}

const PKG_INIT: &str = "from .core import limit\n";
const PKG_TEST: &str = "from pkg import limit\n\n\ndef test_limit():\n    assert limit(10) == False\n    assert limit(11) == True\n";

fn project_package_files<'a>(test_path: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("svc/a/pyproject.toml", PROJECT),
        ("svc/b/pyproject.toml", PROJECT),
        ("svc/a/pkg/__init__.py", PKG_INIT),
        ("svc/b/pkg/__init__.py", PKG_INIT),
        ("svc/a/pkg/core.py", CORE_PY),
        ("svc/b/pkg/core.py", CORE_PY),
        (test_path, PKG_TEST),
    ]
}

#[test]
fn a_nested_project_package_reexport_resolves_for_its_own_project() -> Result<(), String> {
    // Positive control for #13737: `svc/a/pkg/__init__.py` re-exports
    // `limit`, so `from pkg import limit` in project a reaches it.
    let finding = analyze_one_line(
        "layout-reexport-own",
        &project_package_files("svc/a/tests/test_core.py"),
        "svc/a/pkg/core.py",
        2,
    )?;
    if finding.class != ExposureClass::Exposed {
        return Err(format!("own-project re-export gave {:?}", finding.class));
    }
    Ok(())
}

#[test]
fn a_sibling_project_package_reexport_credits_nothing() -> Result<(), String> {
    let finding = analyze_one_line(
        "layout-reexport-sibling",
        &project_package_files("svc/b/tests/test_core.py"),
        "svc/a/pkg/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("project b's `from pkg import limit` credited project a".to_string());
    }
    Ok(())
}

#[test]
fn a_package_reexport_name_is_not_claimed_outside_every_project() -> Result<(), String> {
    let finding = analyze_one_line(
        "layout-reexport-outside",
        &project_package_files("tests/test_core.py"),
        "svc/a/pkg/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("a repo-root test credited a project-relative package name".to_string());
    }
    Ok(())
}

#[test]
fn a_package_reexport_without_a_project_marker_claims_no_bare_name() -> Result<(), String> {
    let finding = analyze_one_line(
        "layout-reexport-no-marker",
        &[
            ("svc/a/pkg/__init__.py", PKG_INIT),
            ("svc/a/pkg/core.py", CORE_PY),
            ("svc/a/tests/test_core.py", PKG_TEST),
        ],
        "svc/a/pkg/core.py",
        2,
    )?;
    if finding.class == ExposureClass::Exposed {
        return Err("an unmarked project claimed a bare package name".to_string());
    }
    Ok(())
}

#[test]
fn a_package_reexport_beside_a_same_named_module_claims_no_bare_name() -> Result<(), String> {
    // `pkg.py` beside `pkg/__init__.py`: the name identifies neither.
    let mut files = project_package_files("svc/a/tests/test_core.py");
    files.push(("svc/a/pkg.py", CORE_PY));
    let finding = analyze_one_line("layout-reexport-twin", &files, "svc/a/pkg/core.py", 2)?;
    if finding.class == ExposureClass::Exposed {
        return Err("a `pkg` name shared with `pkg.py` credited the package".to_string());
    }
    Ok(())
}

/// Repo-scan counterpart: the class `build_repo_evidence` (the production
/// path of repository scans) gives line 2 of `owner_file` (#13737).
fn repo_scan_class(
    label: &str,
    files: &[(&str, &str)],
    owner_file: &str,
) -> Result<ExposureClass, String> {
    let root = super::reexport_tests::unique_tempdir(label)?;
    files.iter().try_for_each(|(path, contents)| {
        super::reexport_tests::write_file(&root.join(path), contents)
    })?;
    let input = repo::select_repo_input_with_limit(
        &root,
        repo::RepoWorkingSetLimit {
            limit: 800,
            source: repo::RepoWorkingSetCapSource::Default,
        },
    );
    let evidence = repo::build_repo_evidence(&input, &root);
    let _ = std::fs::remove_dir_all(&root);
    let mut matching = evidence.findings.iter().filter(|finding| {
        finding.probe.location.line == 2
            && normalized_path(&finding.probe.location.file) == owner_file
    });
    let finding = matching
        .next()
        .ok_or_else(|| format!("{label}: no finding for line 2 of {owner_file}"))?;
    if matching.next().is_some() {
        return Err(format!("{label}: more than one finding for {owner_file}:2"));
    }
    Ok(finding.class.clone())
}

fn flat_scan_files<'a>(test_project: &'a str) -> Vec<(&'a str, &'a str)> {
    let test_path = match test_project {
        "a" => "svc/a/tests/test_core.py",
        "b" => "svc/b/tests/test_core.py",
        _ => "tests/test_core.py",
    };
    vec![
        ("svc/a/pyproject.toml", PROJECT),
        ("svc/b/pyproject.toml", PROJECT),
        ("svc/a/core.py", CORE_PY),
        ("svc/b/core.py", CORE_PY),
        (test_path, STRONG_TEST),
    ]
}

#[test]
fn repo_scan_credits_a_nested_project_test_importing_a_flat_module() -> Result<(), String> {
    let class = repo_scan_class("scan-flat-own", &flat_scan_files("a"), "svc/a/core.py")?;
    if class != ExposureClass::Exposed {
        return Err(format!("own-project strong test gave {class:?}"));
    }
    Ok(())
}

#[test]
fn repo_scan_does_not_credit_a_sibling_project_test_for_a_flat_module() -> Result<(), String> {
    let class = repo_scan_class("scan-flat-sibling", &flat_scan_files("b"), "svc/a/core.py")?;
    if class == ExposureClass::Exposed {
        return Err("a sibling project's `core` test credited svc/a/core.py".to_string());
    }
    Ok(())
}

#[test]
fn repo_scan_does_not_credit_a_test_outside_the_project() -> Result<(), String> {
    let class = repo_scan_class(
        "scan-flat-outside",
        &flat_scan_files("root"),
        "svc/a/core.py",
    )?;
    if class == ExposureClass::Exposed {
        return Err("a repo-root test credited a project-relative name".to_string());
    }
    Ok(())
}

#[test]
fn repo_scan_does_not_credit_a_flat_name_without_a_project_marker() -> Result<(), String> {
    let files = [
        ("svc/a/core.py", CORE_PY),
        ("svc/a/tests/test_core.py", STRONG_TEST),
    ];
    let class = repo_scan_class("scan-no-marker", &files, "svc/a/core.py")?;
    if class == ExposureClass::Exposed {
        return Err("an unmarked project claimed a bare module name".to_string());
    }
    Ok(())
}

#[test]
fn repo_scan_credits_a_package_reexport_to_its_own_project_only() -> Result<(), String> {
    for (test_project, own) in [("a", true), ("b", false)] {
        let test_path = format!("svc/{test_project}/tests/test_core.py");
        let class = repo_scan_class(
            &format!("scan-reexport-{test_project}"),
            &project_package_files(&test_path),
            "svc/a/pkg/core.py",
        )?;
        if own != (class == ExposureClass::Exposed) {
            return Err(format!(
                "`from pkg import limit` in project {test_project} gave {class:?}"
            ));
        }
    }
    Ok(())
}
