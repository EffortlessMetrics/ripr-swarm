//! Assertion admission around behavior pytest installs from outside the test
//! file (#6657): conftest autouse fixtures, `pytest_runtest_*` hooks,
//! `pytest_plugins` and `usefixtures`, plus the single-file registry and
//! root-level relative-import blind spots found in the #6656 review.

use super::admission::PythonAssertionAdmission as A;
use super::fixtures::taint_admission_by_conftest;
use super::source_facts::extract_source_facts;
use super::source_utils::is_test_file;
use super::*;
use std::collections::BTreeSet;

const NONE: &str = "extraction_complete_no_assertion";
const UNRESOLVED: &str = "assertion_like_present_but_unresolved";
const RECOGNIZED: &str = "recognized_assertion_present";

const CALL_ONLY: &str =
    "from src.tax import tax_band\n\n\ndef test_tax_band():\n    tax_band(100)\n";
const AUTOUSE_LEAK_CHECK: &str = "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef no_leaks():\n    yield\n    assert LEAKS == []\n";

/// Admission of `test_name` in `target`, with workspace files processed the
/// way both the diff and repo passes do: per-file facts, then the conftest
/// chain pass. `unread` lists conftest paths that exist but were not read.
fn admission_in(
    files: &[(&str, &str)],
    target: &str,
    test_name: &str,
    unread: &[&str],
) -> Result<&'static str, String> {
    let mut tests = Vec::new();
    let mut implicit = BTreeSet::new();
    for (path, source) in files {
        let facts = extract_source_facts(Path::new(path), source);
        if facts.conftest_implicit_setup {
            implicit.insert(PathBuf::from(path));
        }
        if is_test_file(Path::new(path)) {
            tests.extend(facts.tests);
        }
    }
    taint_admission_by_conftest(&mut tests, &implicit, |conftest| {
        unread.iter().any(|path| Path::new(path) == conftest)
    });
    tests
        .iter()
        .find(|test| test.file == Path::new(target) && test.name == test_name)
        .map(|test| test.assertion_admission.as_str())
        .ok_or_else(|| format!("expected `{test_name}` in `{target}` to be extracted"))
}

fn tax_admission(files: &[(&str, &str)]) -> Result<&'static str, String> {
    admission_in(files, "tests/test_tax.py", "test_tax_band", &[])
}

#[test]
fn conftest_autouse_fixture_makes_admission_unresolved() -> Result<(), String> {
    let got = tax_admission(&[
        ("tests/conftest.py", AUTOUSE_LEAK_CHECK),
        ("tests/test_tax.py", CALL_ONLY),
    ])?;
    assert_eq!(got, UNRESOLVED);
    Ok(())
}

#[test]
fn without_conftest_the_same_test_stays_extraction_complete_no_assertion() -> Result<(), String> {
    assert_eq!(tax_admission(&[("tests/test_tax.py", CALL_ONLY)])?, NONE);
    Ok(())
}

#[test]
fn conftest_without_implicit_setup_does_not_taint() -> Result<(), String> {
    let plain = "import pytest\n\n\n@pytest.fixture\ndef db():\n    return 1\n\n\n@pytest.fixture(autouse=False)\ndef other():\n    return 2\n\n\ndef pytest_configure(config):\n    config.addinivalue_line('markers', 'slow')\n";
    let got = tax_admission(&[
        ("tests/conftest.py", plain),
        ("tests/test_tax.py", CALL_ONLY),
    ])?;
    assert_eq!(got, NONE);
    Ok(())
}

#[test]
fn conftest_autouse_in_an_unrelated_directory_does_not_taint() -> Result<(), String> {
    // A sibling directory and a child directory are not on the chain.
    let got = tax_admission(&[
        ("other/conftest.py", AUTOUSE_LEAK_CHECK),
        ("tests/sub/conftest.py", AUTOUSE_LEAK_CHECK),
        ("tests/test_tax.py", CALL_ONLY),
    ])?;
    assert_eq!(got, NONE);
    Ok(())
}

#[test]
fn conftest_on_an_ancestor_directory_taints_nested_tests() -> Result<(), String> {
    for conftest in ["conftest.py", "tests/conftest.py"] {
        let got = admission_in(
            &[
                (conftest, AUTOUSE_LEAK_CHECK),
                ("tests/unit/test_tax.py", CALL_ONLY),
            ],
            "tests/unit/test_tax.py",
            "test_tax_band",
            &[],
        )?;
        assert_eq!(got, UNRESOLVED, "{conftest}");
    }
    Ok(())
}

#[test]
fn conftest_hooks_plugins_and_nested_autouse_make_admission_unresolved() -> Result<(), String> {
    let cases = [
        (
            "pytest_runtest_teardown hook",
            "def pytest_runtest_teardown(item):\n    assert not LEAKS\n",
        ),
        (
            "pytest_runtest_call hookwrapper",
            "import pytest\n\n\n@pytest.hookimpl(hookwrapper=True)\ndef pytest_runtest_call(item):\n    yield\n",
        ),
        (
            "pytest_plugins list",
            "pytest_plugins = ['myplugin.checks']\n",
        ),
        (
            "pytest_plugins tuple",
            "pytest_plugins = ('myplugin.checks',)\n",
        ),
        (
            "autouse fixture under a conditional",
            "import pytest\nimport sys\nif sys.platform == 'linux':\n    @pytest.fixture(autouse=True)\n    def guard():\n        yield\n",
        ),
        (
            "autouse fixture via a bare fixture import",
            "from pytest import fixture\n\n\n@fixture(autouse=True)\ndef guard():\n    yield\n",
        ),
        (
            "non-literal autouse value",
            "import pytest\nFLAG = True\n\n\n@pytest.fixture(autouse=FLAG)\ndef guard():\n    yield\n",
        ),
        (
            "autouse fixture in a hook class",
            "import pytest\n\n\nclass Plugin:\n    @pytest.fixture(autouse=True)\n    def guard(self):\n        yield\n",
        ),
    ];
    for (label, conftest) in cases {
        let got = tax_admission(&[
            ("tests/conftest.py", conftest),
            ("tests/test_tax.py", CALL_ONLY),
        ])?;
        assert_eq!(got, UNRESOLVED, "{label}");
    }
    Ok(())
}

#[test]
fn recognized_assertion_stays_recognized_under_a_conftest_autouse() -> Result<(), String> {
    // The conftest can only withhold a clean no-assertion state; it never
    // downgrades or grants anything else.
    let got = tax_admission(&[
        ("tests/conftest.py", AUTOUSE_LEAK_CHECK),
        (
            "tests/test_tax.py",
            "from src.tax import tax_band\n\n\ndef test_tax_band():\n    assert tax_band(100) == 10\n",
        ),
    ])?;
    assert_eq!(got, RECOGNIZED);
    Ok(())
}

#[test]
fn a_chain_conftest_that_was_not_read_is_unresolved() -> Result<(), String> {
    let files = [("tests/test_tax.py", CALL_ONLY)];
    let unread = admission_in(
        &files,
        "tests/test_tax.py",
        "test_tax_band",
        &["tests/conftest.py"],
    )?;
    assert_eq!(unread, UNRESOLVED);
    // A conftest outside the chain being unread changes nothing.
    let elsewhere = admission_in(
        &files,
        "tests/test_tax.py",
        "test_tax_band",
        &["other/conftest.py"],
    )?;
    assert_eq!(elsewhere, NONE);
    Ok(())
}

#[test]
fn usefixtures_mark_and_in_file_plugins_make_admission_unresolved() -> Result<(), String> {
    let cases = [
        (
            "function usefixtures",
            "import pytest\nfrom src.tax import tax_band\n\n\n@pytest.mark.usefixtures('checked')\ndef test_tax_band():\n    tax_band(100)\n",
            UNRESOLVED,
        ),
        (
            "class usefixtures",
            "import pytest\nfrom src.tax import tax_band\n\n\n@pytest.mark.usefixtures('checked')\nclass TestTax:\n    def test_tax_band(self):\n        tax_band(100)\n",
            UNRESOLVED,
        ),
        (
            "module pytestmark usefixtures",
            "import pytest\nfrom src.tax import tax_band\npytestmark = pytest.mark.usefixtures('checked')\n\n\ndef test_tax_band():\n    tax_band(100)\n",
            UNRESOLVED,
        ),
        (
            "pytest_plugins in the test module",
            "from src.tax import tax_band\npytest_plugins = ['myplugin.checks']\n\n\ndef test_tax_band():\n    tax_band(100)\n",
            UNRESOLVED,
        ),
        (
            "unrelated mark stays assertion-free",
            "import pytest\nfrom src.tax import tax_band\n\n\n@pytest.mark.slow\ndef test_tax_band():\n    tax_band(100)\n",
            NONE,
        ),
    ];
    for (label, source, expected) in cases {
        assert_eq!(
            admission_in(
                &[("tests/test_tax.py", source)],
                "tests/test_tax.py",
                "test_tax_band",
                &[]
            )?,
            expected,
            "{label}"
        );
        // A test-module `pytest_plugins` is not a conftest and must not be
        // read as one.
    }
    Ok(())
}

const REGISTRY_TEST: &str = "def test_x():\n    for fn in HANDLERS:\n        fn(1)\n";

fn single_file(path: &str, source: &str) -> Result<&'static str, String> {
    admission_in(&[(path, source)], path, "test_x", &[])
}

#[test]
fn plain_name_registering_decorator_taints_the_registry() -> Result<(), String> {
    let registers = format!(
        "HANDLERS = []\n\n\ndef register(f):\n    HANDLERS.append(f)\n    return f\n\n\n@register\ndef verify(x):\n    assert x\n\n\n{REGISTRY_TEST}"
    );
    assert_eq!(single_file("tests/test_m.py", &registers)?, UNRESOLVED);
    let factory = format!(
        "HANDLERS = {{}}\n\n\ndef register(key):\n    def wrap(f):\n        HANDLERS[key] = f\n        return f\n    return wrap\n\n\n@register('a')\ndef verify(x):\n    assert x\n\n\n{REGISTRY_TEST}"
    );
    assert_eq!(single_file("tests/test_m.py", &factory)?, UNRESOLVED);
    // A decorator that stores nothing leaves the registry empty.
    let inert = format!(
        "HANDLERS = []\n\n\ndef passthrough(f):\n    return f\n\n\n@passthrough\ndef verify(x):\n    assert x\n\n\n{REGISTRY_TEST}"
    );
    assert_eq!(single_file("tests/test_m.py", &inert)?, NONE);
    Ok(())
}

#[test]
fn module_level_while_and_match_registry_fills_taint_the_registry() -> Result<(), String> {
    let fills = [
        (
            "while",
            "HANDLERS = []\n\n\ndef verify(x):\n    assert x\n\n\ni = 0\nwhile i < 1:\n    HANDLERS.append(verify)\n    i += 1\n",
            UNRESOLVED,
        ),
        (
            "match",
            "HANDLERS = []\nMODE = 1\n\n\ndef verify(x):\n    assert x\n\n\nmatch MODE:\n    case 1:\n        HANDLERS.append(verify)\n    case _:\n        pass\n",
            UNRESOLVED,
        ),
        (
            "try handler",
            "HANDLERS = []\n\n\ndef verify(x):\n    assert x\n\n\ntry:\n    import fastcheck\nexcept ImportError:\n    HANDLERS.append(verify)\n",
            UNRESOLVED,
        ),
        (
            "while that fills nothing",
            "HANDLERS = []\ni = 0\nwhile i < 1:\n    i += 1\n",
            NONE,
        ),
        (
            "match that fills nothing",
            "HANDLERS = []\nMODE = 1\nmatch MODE:\n    case 1:\n        MODE = 2\n",
            NONE,
        ),
    ];
    for (label, prelude, expected) in fills {
        let source = format!("{prelude}\n\n{REGISTRY_TEST}");
        assert_eq!(
            single_file("tests/test_m.py", &source)?,
            expected,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn definitions_under_module_level_loops_and_match_are_known_names() -> Result<(), String> {
    let source = "for _ in range(1):\n    def verify(x):\n        assert x\n\n\ndef test_x():\n    verify(1)\n";
    assert_eq!(single_file("tests/test_m.py", source)?, UNRESOLVED);
    let in_match = "MODE = 1\nmatch MODE:\n    case 1:\n        def verify(x):\n            assert x\n\n\ndef test_x():\n    verify(1)\n";
    assert_eq!(single_file("tests/test_m.py", in_match)?, UNRESOLVED);
    Ok(())
}

#[test]
fn root_level_test_file_treats_relative_imports_as_test_support() -> Result<(), String> {
    let relative_module = "from . import checks\n\n\ndef test_x():\n    checks.run(1)\n";
    assert_eq!(single_file("test_m.py", relative_module)?, UNRESOLVED);
    let relative_name = "from .checks import run\n\n\ndef test_x():\n    run(1)\n";
    assert_eq!(single_file("test_m.py", relative_name)?, UNRESOLVED);
    let relative_local = "def test_x():\n    from . import checks\n    checks.run(1)\n";
    assert_eq!(single_file("test_m.py", relative_local)?, UNRESOLVED);
    // An absolute production import in the same root-level file is unchanged.
    let absolute =
        "from src.pricing import apply_discount\n\n\ndef test_x():\n    apply_discount(1)\n";
    assert_eq!(single_file("test_m.py", absolute)?, NONE);
    // Below the root the directory prefix already recognises test support,
    // and a relative production import above it keeps its old reading.
    let nested = "from ..src import pricing\n\n\ndef test_x():\n    pricing.apply_discount(1)\n";
    assert_eq!(single_file("tests/test_m.py", nested)?, NONE);
    Ok(())
}

#[test]
fn repo_mode_withholds_no_assertion_under_a_conftest_autouse() -> Result<(), String> {
    let run = |label: &str, conftest: Option<&str>| -> Result<Vec<String>, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-py-conftest-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let write = |relative: &str, contents: &str| -> Result<(), String> {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| format!("create parent: {err}"))?;
            }
            std::fs::write(&path, contents).map_err(|err| format!("write {relative}: {err}"))
        };
        write(
            "src/tax.py",
            "def tax_band(amount):\n    if amount > 50:\n        return 2\n    return 1\n",
        )?;
        write("tests/test_tax.py", CALL_ONLY)?;
        if let Some(conftest) = conftest {
            write("tests/conftest.py", conftest)?;
        }
        let adapter = PythonAdapter;
        let options = AnalysisOptions {
            root: root.clone(),
            base: None,
            diff_file: None,
            mode: crate::analysis::AnalysisMode::Deep,
            include_unchanged_tests: false,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            perl_producer_failure: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
            resolved_subject_identity: None,
            open_rust_index_paths: Default::default(),
        };
        let result = adapter.analyze_repo(&options, &OraclePolicy::default());
        let cleanup = std::fs::remove_dir_all(&root);
        let result = result?;
        cleanup.map_err(|err| format!("remove root: {err}"))?;
        Ok(result
            .findings
            .iter()
            .flat_map(|finding| finding.evidence.iter().cloned())
            .filter(|line| line.starts_with("test_assertion_admission:"))
            .collect())
    };
    let clean = run("clean", None)?;
    assert!(
        clean.iter().any(|line| line.contains(NONE)),
        "without a conftest the call-only test is assertion-free, got {clean:?}"
    );
    let tainted = run("autouse", Some(AUTOUSE_LEAK_CHECK))?;
    assert!(
        tainted.iter().any(|line| line.contains(UNRESOLVED))
            && !tainted.iter().any(|line| line.contains(NONE)),
        "a conftest autouse must withhold the no-assertion state, got {tainted:?}"
    );
    // Keep the enum import exercised by the state mapping.
    assert_eq!(A::NoAssertionLike.as_str(), NONE);
    Ok(())
}

/// The diff pass (`analyze_diff`) wires the same conftest taint as the repo
/// pass; removing that call in `python.rs` makes the tainted run keep the
/// no-assertion state.
#[test]
fn diff_mode_withholds_no_assertion_under_a_conftest_autouse() -> Result<(), String> {
    let run = |label: &str, conftest: Option<&str>| -> Result<Vec<String>, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-py-conftest-diff-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let source = "def tax_band(amount):\n    if amount > 50:\n        return 2\n    return 1\n";
        let write = |relative: &str, contents: &str| -> Result<(), String> {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| format!("create parent: {err}"))?;
            }
            std::fs::write(&path, contents).map_err(|err| format!("write {relative}: {err}"))
        };
        write("src/tax.py", source)?;
        write("tests/test_tax.py", CALL_ONLY)?;
        if let Some(conftest) = conftest {
            write("tests/conftest.py", conftest)?;
        }
        let options = AnalysisOptions {
            root: root.clone(),
            base: None,
            diff_file: None,
            mode: crate::analysis::AnalysisMode::Draft,
            include_unchanged_tests: false,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            perl_producer_failure: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
            resolved_subject_identity: None,
            open_rust_index_paths: Default::default(),
        };
        let changed_files = vec![ChangedFile {
            path: PathBuf::from("src/tax.py"),
            added_lines: vec![crate::analysis::diff::ChangedLine {
                line: 2,
                new_side_line: 2,
                text: "    if amount > 50:".to_string(),
            }],
            removed_lines: Vec::new(),
        }];
        let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
        let cleanup = std::fs::remove_dir_all(&root);
        let result = result?;
        cleanup.map_err(|err| format!("remove root: {err}"))?;
        Ok(result
            .findings
            .iter()
            .flat_map(|finding| finding.evidence.iter().cloned())
            .filter(|line| line.starts_with("test_assertion_admission:"))
            .collect())
    };
    let clean = run("clean", None)?;
    assert!(
        clean.iter().any(|line| line.contains(NONE)),
        "without a conftest the call-only test is assertion-free, got {clean:?}"
    );
    let tainted = run("autouse", Some(AUTOUSE_LEAK_CHECK))?;
    assert!(
        tainted.iter().any(|line| line.contains(UNRESOLVED))
            && !tainted.iter().any(|line| line.contains(NONE)),
        "a conftest autouse must withhold the no-assertion state in diff mode, got {tainted:?}"
    );
    Ok(())
}
