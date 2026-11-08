//! Tests pytest never collects do not credit (#11527): `__test__ = False`,
//! conftest `collect_ignore`, and the pytest options that select files,
//! classes and functions. Anything the pass cannot evaluate withholds credit.

use super::collection_config::apply_collection_config;
use super::source_facts::extract_source_facts;
use super::test_activation::activation_control;
use super::*;
use std::path::Path;

const TEST_BODY: &str = "from calc import price\n\n\ndef test_price():\n    assert price(3) == 6\n";

fn scratch(label: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-py-collect-{label}-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    Ok(root)
}

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("create {parent:?}: {err}"))?;
    }
    std::fs::write(&path, text).map_err(|err| format!("write {relative}: {err}"))
}

/// Controls on the only test of `test_file` after the collection pass over a
/// scratch workspace holding `files`.
fn controls_of(
    label: &str,
    files: &[(&str, &str)],
    test_file: &str,
) -> Result<Vec<String>, String> {
    let root = scratch(label)?;
    for (path, text) in files {
        write(&root, path, text)?;
    }
    let source = files
        .iter()
        .find(|(path, _)| *path == test_file)
        .map(|(_, text)| *text)
        .ok_or_else(|| format!("{test_file} is not among the files"))?;
    let mut tests = extract_source_facts(Path::new(test_file), source).tests;
    apply_collection_config(&mut tests, &root);
    let _ = std::fs::remove_dir_all(&root);
    Ok(tests
        .iter()
        .filter_map(|test| activation_control(test).map(str::to_string))
        .collect())
}

fn assert_withheld(label: &str, files: &[(&str, &str)], reason: &str) -> Result<(), String> {
    let got = controls_of(label, files, "tests/test_calc.py")?;
    if got.iter().any(|name| name == reason) {
        return Ok(());
    }
    Err(format!("{label}: expected `{reason}`, got {got:?}"))
}

fn assert_credited(label: &str, files: &[(&str, &str)]) -> Result<(), String> {
    let got = controls_of(label, files, "tests/test_calc.py")?;
    if got.is_empty() {
        return Ok(());
    }
    Err(format!("{label}: expected no control, got {got:?}"))
}

fn control_names(file: &str, source: &str) -> Vec<String> {
    extract_source_facts(Path::new(file), source)
        .tests
        .iter()
        .filter_map(|test| activation_control(test).map(str::to_string))
        .collect()
}

const SWITCH: &str = "pytest.collection(__test__ = False)";

#[test]
fn class_test_switch_false_withholds_credit() {
    let source = "from calc import price\n\n\nclass TestP:\n    __test__ = False\n\n    def test_p(self):\n        assert price(3) == 6\n";
    assert_eq!(control_names("tests/test_calc.py", source), [SWITCH]);
}

#[test]
fn class_test_switch_true_and_unrelated_class_keep_credit() {
    let source = "from calc import price\n\n\nclass TestP:\n    __test__ = True\n\n    def test_p(self):\n        assert price(3) == 6\n\n\nclass TestQ:\n    def test_q(self):\n        assert price(3) == 6\n";
    assert!(control_names("tests/test_calc.py", source).is_empty());
}

#[test]
fn subclass_inherits_a_false_switch_unless_it_restores_true() {
    let inherits = "from calc import price\n\n\nclass Base:\n    __test__ = False\n\n\nclass TestP(Base):\n    def test_p(self):\n        assert price(3) == 6\n";
    assert_eq!(control_names("tests/test_calc.py", inherits), [SWITCH]);
    let restores = "from calc import price\n\n\nclass Base:\n    __test__ = False\n\n\nclass TestP(Base):\n    __test__ = True\n\n    def test_p(self):\n        assert price(3) == 6\n";
    assert!(control_names("tests/test_calc.py", restores).is_empty());
}

#[test]
fn intermediate_class_test_switch_true_keeps_credit() {
    let source = "from calc import price\n\n\nclass Base:\n    __test__ = False\n\n\nclass TestMid(Base):\n    __test__ = True\n\n\nclass TestSub(TestMid):\n    def test_p(self):\n        assert price(3) == 6\n";
    assert!(control_names("tests/test_calc.py", source).is_empty());
    let disabled_mid = "from calc import price\n\n\nclass Base:\n    __test__ = True\n\n\nclass TestMid(Base):\n    __test__ = False\n\n\nclass TestSub(TestMid):\n    def test_p(self):\n        assert price(3) == 6\n";
    assert_eq!(control_names("tests/test_calc.py", disabled_mid), [SWITCH]);
}

#[test]
fn computed_or_branch_bound_test_switch_withholds_credit() {
    for binding in [
        "__test__ = bool(ENABLED)",
        "if ENABLED:\n        __test__ = True",
    ] {
        let source = format!(
            "from calc import price\n\n\nclass TestP:\n    {binding}\n\n    def test_p(self):\n        assert price(3) == 6\n"
        );
        assert_eq!(
            control_names("tests/test_calc.py", &source),
            [SWITCH],
            "{binding}"
        );
    }
}

#[test]
fn match_arm_destructured_and_bare_annotation_test_switch_bindings() {
    // A `match` arm or a destructuring target may set the switch: withheld.
    for binding in [
        "match 1:\n    case 1:\n        __test__ = False",
        "(__test__, marker) = (False, 1)",
        "[__test__, *rest] = [False, 1]",
    ] {
        let source = format!("{binding}\n\n{TEST_BODY}");
        assert_eq!(
            control_names("tests/test_calc.py", &source),
            [SWITCH],
            "{binding}"
        );
        let class_source = format!(
            "from calc import price\n\n\nclass TestP:\n    {}\n\n    def test_p(self):\n        assert price(3) == 6\n",
            binding.replace('\n', "\n    ")
        );
        assert_eq!(
            control_names("tests/test_calc.py", &class_source),
            [SWITCH],
            "{binding}"
        );
    }
    // A match that never binds the switch, and a bare annotation, create no
    // attribute: the test stays collected.
    for binding in ["match 1:\n    case 1:\n        other = 1", "__test__: bool"] {
        let source = format!("{binding}\n\n{TEST_BODY}");
        assert!(
            control_names("tests/test_calc.py", &source).is_empty(),
            "{binding}"
        );
    }
}

#[test]
fn addopts_ignore_missing_its_value_is_not_evaluated() -> Result<(), String> {
    let got = controls_of(
        "ignore-missing-value",
        &[
            ("pytest.ini", "[pytest]\naddopts = --ignore -q\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "tests/test_calc.py",
    )?;
    if got.is_empty() {
        return Err("`--ignore -q` must withhold credit, got no control".to_string());
    }
    Ok(())
}

#[test]
fn module_test_switch_false_withholds_credit() {
    let source = format!("__test__ = False\n\n{TEST_BODY}");
    assert_eq!(control_names("tests/test_calc.py", &source), [SWITCH]);
    let enabled = format!("__test__ = True\n\n{TEST_BODY}");
    assert!(control_names("tests/test_calc.py", &enabled).is_empty());
}

#[test]
fn conftest_collect_ignore_withholds_credit_for_ignored_files_only() -> Result<(), String> {
    let ignored = [
        ("tests/conftest.py", "collect_ignore = ['test_calc.py']\n"),
        ("tests/test_calc.py", TEST_BODY),
    ];
    assert_withheld("ignore-file", &ignored, "pytest.collection(collect_ignore)")?;
    let other = [
        ("tests/conftest.py", "collect_ignore = ['test_other.py']\n"),
        ("tests/test_calc.py", TEST_BODY),
    ];
    assert_credited("ignore-other", &other)?;
    let directory = [
        ("tests/conftest.py", "collect_ignore = ['unit']\n"),
        ("tests/unit/test_calc.py", TEST_BODY),
    ];
    let got = controls_of("ignore-dir", &directory, "tests/unit/test_calc.py")?;
    assert_eq!(got, ["pytest.collection(collect_ignore)"]);
    Ok(())
}

#[test]
fn conftest_collect_ignore_glob_and_mutations_are_read() -> Result<(), String> {
    for (label, conftest) in [
        ("glob", "collect_ignore_glob = ['test_c*.py']\n"),
        (
            "append",
            "collect_ignore = []\ncollect_ignore.append('test_calc.py')\n",
        ),
        (
            "augment",
            "collect_ignore = ['a.py']\ncollect_ignore += ['test_calc.py']\n",
        ),
        (
            "extend",
            "collect_ignore = []\ncollect_ignore.extend(['test_calc.py'])\n",
        ),
    ] {
        assert_withheld(
            label,
            &[
                ("tests/conftest.py", conftest),
                ("tests/test_calc.py", TEST_BODY),
            ],
            "pytest.collection(collect_ignore)",
        )?;
    }
    assert_credited(
        "glob-miss",
        &[
            (
                "tests/conftest.py",
                "collect_ignore_glob = ['legacy_*.py']\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )
}

#[test]
fn conftest_ignore_outside_the_test_directory_does_not_apply() -> Result<(), String> {
    assert_credited(
        "sibling-conftest",
        &[
            ("other/conftest.py", "collect_ignore = ['test_calc.py']\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )
}

#[test]
fn unevaluable_conftest_collection_forms_withhold_credit() -> Result<(), String> {
    for (label, conftest) in [
        ("computed", "collect_ignore = [name for name in NAMES]\n"),
        (
            "conditional",
            "if SLOW:\n    collect_ignore = ['test_calc.py']\n",
        ),
        ("class-set", "collect_ignore_glob = ['[a-z]*.py']\n"),
        ("parent", "collect_ignore = ['../x.py']\n"),
        (
            "hook",
            "def pytest_ignore_collect(collection_path):\n    return True\n",
        ),
        ("unparsable", "collect_ignore = [\n"),
    ] {
        assert_withheld(
            label,
            &[
                ("tests/conftest.py", conftest),
                ("tests/test_calc.py", TEST_BODY),
            ],
            "pytest.collection(unevaluated conftest)",
        )?;
    }
    Ok(())
}

#[test]
fn conftest_without_collection_names_keeps_credit() -> Result<(), String> {
    assert_credited(
        "plain-conftest",
        &[
            (
                "tests/conftest.py",
                "import pytest\n\n\n@pytest.fixture\ndef rate():\n    return 2\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )
}

#[test]
fn python_files_option_withholds_credit_for_unmatched_file_names() -> Result<(), String> {
    assert_withheld(
        "python-files-ini",
        &[
            ("pytest.ini", "[pytest]\npython_files = check_*.py\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(python_files)",
    )?;
    assert_credited(
        "python-files-match",
        &[
            (
                "pytest.ini",
                "[pytest]\npython_files = check_*.py test_*.py\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )?;
    assert_withheld(
        "python-files-pyproject",
        &[
            (
                "pyproject.toml",
                "[tool.pytest.ini_options]\npython_files = [\"check_*.py\"]\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(python_files)",
    )?;
    assert_withheld(
        "python-files-setup-cfg",
        &[
            (
                "setup.cfg",
                "[tool:pytest]\npython_files =\n    check_*.py\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(python_files)",
    )?;
    assert_withheld(
        "python-files-tox",
        &[
            (
                "tox.ini",
                "[tox]\nenvlist = py\n\n[pytest]\npython_files = check_*.py\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(python_files)",
    )
}

#[test]
fn testpaths_and_ignore_options_withhold_credit() -> Result<(), String> {
    assert_withheld(
        "testpaths",
        &[
            ("pytest.ini", "[pytest]\ntestpaths = other\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(testpaths)",
    )?;
    assert_credited(
        "testpaths-match",
        &[
            ("pytest.ini", "[pytest]\ntestpaths = other tests\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )?;
    assert_withheld(
        "ignore",
        &[
            ("pytest.ini", "[pytest]\naddopts = -q --ignore=tests\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(--ignore)",
    )?;
    assert_withheld(
        "ignore-separate",
        &[
            (
                "pytest.ini",
                "[pytest]\naddopts = --ignore tests/test_calc.py\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(--ignore)",
    )?;
    assert_withheld(
        "ignore-glob",
        &[
            (
                "pytest.ini",
                "[pytest]\naddopts = --ignore-glob=*/test_c*.py\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(--ignore)",
    )?;
    assert_withheld(
        "norecursedirs",
        &[
            ("pytest.ini", "[pytest]\nnorecursedirs = tests\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(norecursedirs)",
    )?;
    for (label, addopts) in [
        ("junitxml", "--junitxml reports/junit.xml"),
        ("html", "--html x.html"),
        ("log-file", "--log-file logs/a.log"),
        ("basetemp", "--basetemp .tmp/x"),
    ] {
        assert_credited(
            label,
            &[
                ("pytest.ini", &format!("[pytest]\naddopts = {addopts}\n")),
                ("tests/test_calc.py", TEST_BODY),
            ],
        )?;
    }
    assert_credited(
        "benign-options",
        &[
            (
                "pytest.ini",
                "[pytest]\naddopts = -ra -q --strict-markers -W error::UserWarning\nminversion = 7\ntestpaths = tests\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )
}

#[test]
fn python_classes_and_functions_options_apply_to_pytest_tests() -> Result<(), String> {
    let class_test = "from calc import price\n\n\nclass TestP:\n    def test_p(self):\n        assert price(3) == 6\n";
    let got = controls_of(
        "python-classes",
        &[
            ("pytest.ini", "[pytest]\npython_classes = Check\n"),
            ("tests/test_calc.py", class_test),
        ],
        "tests/test_calc.py",
    )?;
    assert_eq!(got, ["pytest.collection(python_classes)"]);
    assert_withheld(
        "python-functions",
        &[
            ("pytest.ini", "[pytest]\npython_functions = check_*\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(python_functions)",
    )?;
    assert_credited(
        "python-functions-prefix",
        &[
            ("pytest.ini", "[pytest]\npython_functions = test_ check_\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )?;
    let unittest_case = "import unittest\nfrom calc import price\n\n\nclass PriceCase(unittest.TestCase):\n    def test_price(self):\n        self.assertEqual(price(3), 6)\n";
    let got = controls_of(
        "unittest-exempt",
        &[
            (
                "pytest.ini",
                "[pytest]\npython_classes = Check\npython_functions = check\n",
            ),
            ("tests/test_calc.py", unittest_case),
        ],
        "tests/test_calc.py",
    )?;
    assert!(got.is_empty(), "unittest classes ignore python_*: {got:?}");
    Ok(())
}

#[test]
fn unevaluable_pytest_configuration_withholds_credit() -> Result<(), String> {
    for (label, file, text) in [
        (
            "char-class",
            "pytest.ini",
            "[pytest]\npython_files = [tc]est_*.py\n",
        ),
        ("empty", "pytest.ini", "[pytest]\npython_files =\n"),
        ("quote", "pytest.ini", "[pytest]\ntestpaths = \"tests\n"),
        (
            "override",
            "pytest.ini",
            "[pytest]\naddopts = -o python_files=x\n",
        ),
        (
            "rootdir",
            "pytest.ini",
            "[pytest]\naddopts = --rootdir=sub\n",
        ),
        (
            "positional",
            "pytest.ini",
            "[pytest]\naddopts = tests/unit\n",
        ),
        ("bad-header", "pytest.ini", "[pytest\npython_files = x\n"),
        ("bad-toml", "pyproject.toml", "[tool.pytest.ini_options\n"),
        (
            "toml-type",
            "pyproject.toml",
            "[tool.pytest.ini_options]\ntestpaths = 3\n",
        ),
    ] {
        assert_withheld(
            label,
            &[(file, text), ("tests/test_calc.py", TEST_BODY)],
            "pytest.collection(unevaluated config)",
        )?;
    }
    Ok(())
}

#[test]
fn irrelevant_configuration_keeps_credit() -> Result<(), String> {
    assert_credited(
        "no-pytest-section",
        &[
            ("tox.ini", "[tox]\nenvlist = py\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )?;
    assert_credited(
        "pyproject-other-tool",
        &[
            ("pyproject.toml", "[tool.black]\nline-length = 88\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )?;
    assert_credited("no-config", &[("tests/test_calc.py", TEST_BODY)])
}

#[test]
fn nested_package_configuration_applies_below_its_directory() -> Result<(), String> {
    let files = [
        ("pkg/pytest.ini", "[pytest]\npython_files = check_*.py\n"),
        ("pkg/tests/test_calc.py", TEST_BODY),
    ];
    let got = controls_of("nested", &files, "pkg/tests/test_calc.py")?;
    assert_eq!(got, ["pytest.collection(python_files)"]);
    Ok(())
}

/// pytest selects one inifile per directory; a lower-precedence file must not
/// withhold credit for a test the selected file would collect (#12138).
#[test]
fn empty_pytest_ini_beats_setup_cfg_python_files() -> Result<(), String> {
    assert_credited(
        "empty-ini-over-setup-cfg",
        &[
            ("pytest.ini", ""),
            ("setup.cfg", "[tool:pytest]\npython_files = x_*.py\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )
}

#[test]
fn pyproject_without_pytest_table_defers_to_tox_ini() -> Result<(), String> {
    assert_withheld(
        "pyproject-no-table-tox-wins",
        &[
            ("pyproject.toml", "[tool.black]\nline-length = 88\n"),
            (
                "tox.ini",
                "[tox]\nenvlist = py\n\n[pytest]\npython_files = check_*.py\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(python_files)",
    )
}

#[test]
fn pyproject_with_pytest_table_beats_tox_ini() -> Result<(), String> {
    assert_credited(
        "pyproject-table-over-tox",
        &[
            (
                "pyproject.toml",
                "[tool.pytest.ini_options]\npython_files = [\"test_*.py\"]\n",
            ),
            (
                "tox.ini",
                "[tox]\nenvlist = py\n\n[pytest]\npython_files = x_*.py\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )?;
    assert_credited(
        "native-toml-table-over-tox",
        &[
            (
                "pyproject.toml",
                "[tool.pytest]\npython_files = [\"test_*.py\"]\n",
            ),
            ("tox.ini", "[pytest]\npython_files = x_*.py\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )
}

#[test]
fn unreadable_higher_precedence_inifile_fails_closed() -> Result<(), String> {
    let root = scratch("unreadable-ini")?;
    write(
        &root,
        "setup.cfg",
        "[tool:pytest]\npython_files = test_*.py\n",
    )?;
    write(&root, "tests/test_calc.py", TEST_BODY)?;
    // Larger than `MAX_CONFIG_BYTES` (1 MiB): the reader treats it as
    // unreadable without parsing, so a matching `setup.cfg` must not credit.
    let oversized = root.join("pytest.ini");
    let file = std::fs::File::create(&oversized)
        .map_err(|err| format!("create oversized pytest.ini: {err}"))?;
    file.set_len((1 << 20) + 1)
        .map_err(|err| format!("truncate oversized pytest.ini: {err}"))?;
    drop(file);
    let mut tests = extract_source_facts(Path::new("tests/test_calc.py"), TEST_BODY).tests;
    apply_collection_config(&mut tests, &root);
    let _ = std::fs::remove_dir_all(&root);
    let got: Vec<String> = tests
        .iter()
        .filter_map(|test| activation_control(test).map(str::to_string))
        .collect();
    if got
        .iter()
        .any(|name| name == "pytest.collection(unevaluated config)")
    {
        Ok(())
    } else {
        Err(format!("unreadable pytest.ini must withhold, got {got:?}"))
    }
}

#[test]
fn setup_cfg_without_tool_pytest_section_is_not_a_match() -> Result<(), String> {
    assert_credited(
        "setup-cfg-metadata-only",
        &[
            (
                "setup.cfg",
                "[metadata]\nname = demo\npython_files = x_*.py\n",
            ),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )
}

#[test]
fn single_inifile_collection_rules_are_unchanged() -> Result<(), String> {
    assert_withheld(
        "solo-pytest-ini",
        &[
            ("pytest.ini", "[pytest]\npython_files = check_*.py\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(python_files)",
    )?;
    assert_withheld(
        "solo-setup-cfg",
        &[
            ("setup.cfg", "[tool:pytest]\npython_files = check_*.py\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
        "pytest.collection(python_files)",
    )?;
    assert_credited(
        "solo-matching-ini",
        &[
            ("pytest.ini", "[pytest]\npython_files = test_*.py\n"),
            ("tests/test_calc.py", TEST_BODY),
        ],
    )
}
