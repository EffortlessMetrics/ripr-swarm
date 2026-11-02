//! Tests for the doctest oracle (RIPR-SPEC-0238 rule 26, #9619). Negatives
//! come first: each states a way an example could be mistaken for an oracle
//! that notices the edit.

use super::doctest_tests::{collects_doctests, doctest_tests};
use crate::domain::OracleStrength;
use std::path::{Path, PathBuf};

const ENABLED: &str = "[tool.pytest.ini_options]\naddopts = \"--doctest-modules\"\n";

fn module(body: &str, doc: &str) -> String {
    format!("def {body}\n    \"\"\"Doc.\n\n{doc}    \"\"\"\n    return TAIL\n")
}

fn root(label: &str, config: &str) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-doctest-{label}-{nanos}"));
    std::fs::create_dir_all(&root).map_err(|err| format!("mkdir: {err}"))?;
    std::fs::write(root.join("pyproject.toml"), config).map_err(|err| format!("write: {err}"))?;
    Ok(root)
}

/// How many tests the examples yield for an old and a new source.
fn count(label: &str, config: &str, old: &str, new: &str) -> Result<usize, String> {
    let root = root(label, config)?;
    let tests = doctest_tests(Path::new("src/texts.py"), new, old, &root);
    let _ = std::fs::remove_dir_all(&root);
    Ok(tests.len())
}

fn pair(old_tail: &str, new_tail: &str, doc: &str) -> (String, String) {
    (
        module("clip(text, n):", doc).replace("TAIL", old_tail),
        module("clip(text, n):", doc).replace("TAIL", new_tail),
    )
}

const EXAMPLE: &str = "    >>> clip('abcdef', 3)\n    'abc'\n";

#[test]
fn an_example_the_edit_breaks_is_a_test() -> Result<(), String> {
    let (old, new) = pair("text[:n]", "text[:n + 1]", EXAMPLE);
    assert_eq!(count("breaks", ENABLED, &old, &new)?, 1);
    Ok(())
}

#[test]
fn an_example_both_functions_satisfy_is_not_a_test() -> Result<(), String> {
    let doc = "    >>> clip('abc', 5)\n    'abc'\n";
    let (old, new) = pair("text[:n]", "text[:n + 1]", doc);
    assert_eq!(count("agrees", ENABLED, &old, &new)?, 0);
    Ok(())
}

#[test]
fn doctests_not_collected_yield_nothing() -> Result<(), String> {
    let (old, new) = pair("text[:n]", "text[:n + 1]", EXAMPLE);
    for config in [
        "[tool.pytest.ini_options]\naddopts = \"-q\"\n",
        "[tool.pytest.ini_options]\naddopts = \"--doctest-modules --ignore=src\"\n",
        "",
    ] {
        assert_eq!(count("off", config, &old, &new)?, 0, "{config}");
    }
    Ok(())
}

#[test]
fn an_example_added_by_the_diff_is_not_its_own_oracle() -> Result<(), String> {
    let (old, _) = pair("text[:n]", "text[:n]", "");
    let (_, new) = pair("text[:n]", "text[:n + 1]", EXAMPLE);
    assert_eq!(count("added", ENABLED, &old, &new)?, 0);
    Ok(())
}

#[test]
fn a_doctest_directive_or_continuation_drops_the_docstring() -> Result<(), String> {
    for doc in [
        "    >>> clip('abcdef', 3)  # doctest: +SKIP\n    'abc'\n",
        "    >>> x = 1\n    >>> clip('abcdef', 3)\n    'abc'\n",
        "    >>> clip('abcdef',\n    ...      3)\n    'abc'\n",
        "    >>> clip('abcdef', 3)\n    'a\\\\x62c'\n",
    ] {
        let (old, new) = pair("text[:n]", "text[:n + 1]", doc);
        assert_eq!(count("form", ENABLED, &old, &new)?, 0, "{doc}");
    }
    Ok(())
}

#[test]
fn a_function_the_evaluator_cannot_run_is_not_read() -> Result<(), String> {
    let (old, new) = pair("text[:n]", "other(text)[:n]", EXAMPLE);
    assert_eq!(count("opaque", ENABLED, &old, &new)?, 0);
    Ok(())
}

#[test]
fn a_decorated_function_is_not_read() -> Result<(), String> {
    let (old, new) = pair("text[:n]", "text[:n + 1]", EXAMPLE);
    let (old, new) = (format!("@cache\n{old}"), format!("@cache\n{new}"));
    assert_eq!(count("decorated", ENABLED, &old, &new)?, 0);
    Ok(())
}

#[test]
fn a_rebound_name_is_not_read() -> Result<(), String> {
    let (old, new) = pair("text[:n]", "text[:n + 1]", EXAMPLE);
    let (old, new) = (
        format!("{old}\nclip = len\n"),
        format!("{new}\nclip = len\n"),
    );
    assert_eq!(count("rebound", ENABLED, &old, &new)?, 0);
    Ok(())
}

#[test]
fn a_test_dictionary_replaces_the_docstring_examples() -> Result<(), String> {
    let (old, new) = pair("text[:n]", "text[:n + 1]", EXAMPLE);
    let (old, new) = (
        format!("{old}\n__test__ = {{}}\n"),
        format!("{new}\n__test__ = {{}}\n"),
    );
    assert_eq!(count("dunder-test", ENABLED, &old, &new)?, 0);
    Ok(())
}

#[test]
fn a_conftest_that_ignores_collection_stops_the_read() -> Result<(), String> {
    let root = root("conftest", ENABLED)?;
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("mkdir: {err}"))?;
    std::fs::write(
        root.join("src/conftest.py"),
        "collect_ignore = ['texts.py']\n",
    )
    .map_err(|err| format!("write: {err}"))?;
    let collected = collects_doctests(&root, Path::new("src/texts.py"));
    let _ = std::fs::remove_dir_all(&root);
    assert!(!collected);
    Ok(())
}

#[test]
fn collection_is_read_from_pytest_ini_and_cfg_headers() {
    for (name, text, expected) in [
        (
            "pytest.ini",
            "[pytest]\naddopts = --doctest-modules\n",
            true,
        ),
        (
            "setup.cfg",
            "[tool:pytest]\naddopts = --doctest-modules\n",
            true,
        ),
        (
            "setup.cfg",
            "[flake8]\naddopts = --doctest-modules\n",
            false,
        ),
    ] {
        let dir = std::env::temp_dir().join(format!("ripr-doctest-cfg-{name}-{expected}"));
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(name), text);
        let collected = collects_doctests(&dir, Path::new("src/texts.py"));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(collected, expected, "{name}: {text}");
    }
}

#[test]
fn a_module_that_binds_len_is_not_read() -> Result<(), String> {
    let (old, new) = pair(
        "len(text)",
        "len(text) + 1",
        "    >>> clip('abc', 3)\n    3\n",
    );
    let (old, new) = (
        format!("{old}\ndef len(x):\n    return 0\n"),
        format!("{new}\ndef len(x):\n    return 0\n"),
    );
    assert_eq!(count("len", ENABLED, &old, &new)?, 0);
    let (old, new) = (format!("len = abs\n{old}"), format!("len = abs\n{new}"));
    assert_eq!(count("len-assign", ENABLED, &old, &new)?, 0);
    Ok(())
}

#[test]
fn a_positional_collection_target_in_addopts_stops_the_read() -> Result<(), String> {
    let (old, new) = pair("text[:n]", "text[:n + 1]", EXAMPLE);
    for config in [
        "[tool.pytest.ini_options]\naddopts = \"--doctest-modules tests/\"\n",
        "[tool.pytest.ini_options]\naddopts = \"--doctest-modules --import-mode=importlib\"\n",
        "[tool.pytest.ini_options]\naddopts = \"--doctest-modules\"\nnorecursedirs = [\"src\"]\n",
    ] {
        assert_eq!(count("positional", config, &old, &new)?, 0, "{config}");
    }
    Ok(())
}

#[test]
fn conftest_hooks_and_namespaces_outside_the_file_path_stop_the_read() -> Result<(), String> {
    for (label, name, body) in [
        (
            "glob",
            "src/conftest.py",
            "collect_ignore_glob = ['*.py']\n",
        ),
        (
            "namespace",
            "src/conftest.py",
            "@pytest.fixture\ndef doctest_namespace(): ...\n",
        ),
        (
            "sibling",
            "tests/conftest.py",
            "def pytest_collection_modifyitems(items): ...\n",
        ),
    ] {
        let root = root(label, ENABLED)?;
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().ok_or("parent")?).map_err(|err| format!("{err}"))?;
        std::fs::write(&path, body).map_err(|err| format!("{err}"))?;
        let collected = collects_doctests(&root, Path::new("src/texts.py"));
        let _ = std::fs::remove_dir_all(&root);
        assert!(!collected, "{label}");
    }
    Ok(())
}

#[test]
fn newer_pytest_config_files_leave_collection_unknown() -> Result<(), String> {
    for name in [".pytest.ini", "pytest.toml", ".pytest.toml"] {
        let root = root("newer", ENABLED)?;
        std::fs::write(root.join(name), "").map_err(|err| format!("{err}"))?;
        let collected = collects_doctests(&root, Path::new("src/texts.py"));
        let _ = std::fs::remove_dir_all(&root);
        assert!(!collected, "{name}");
    }
    Ok(())
}

#[test]
fn attached_short_flags_and_cache_selectors_stop_the_read() -> Result<(), String> {
    let (old, new) = pair("text[:n]", "text[:n + 1]", EXAMPLE);
    for flags in ["-kfoo", "-mslow", "--lf", "-o doctest_optionflags=X"] {
        let config =
            format!("[tool.pytest.ini_options]\naddopts = \"--doctest-modules {flags}\"\n");
        assert_eq!(count("flags", &config, &old, &new)?, 0, "{flags}");
    }
    Ok(())
}

#[test]
fn an_escape_in_the_docstring_source_drops_it() -> Result<(), String> {
    let (old, new) = pair(
        "text[:n]",
        "text[:n + 1]",
        "    >>> clip('abcdef', 3)\\n    'abc'\n",
    );
    assert_eq!(count("escape", ENABLED, &old, &new)?, 0);
    Ok(())
}

#[test]
fn a_pytest_ini_without_a_pytest_section_leaves_collection_unknown() -> Result<(), String> {
    let root = root("empty-ini", ENABLED)?;
    std::fs::write(root.join("pytest.ini"), "").map_err(|err| format!("{err}"))?;
    let collected = collects_doctests(&root, Path::new("src/texts.py"));
    let _ = std::fs::remove_dir_all(&root);
    assert!(!collected);
    Ok(())
}

#[test]
fn a_root_with_more_subdirectories_than_the_scan_cap_is_unknown() -> Result<(), String> {
    let root = root("many-dirs", ENABLED)?;
    for index in 0..300 {
        std::fs::create_dir_all(root.join(format!("d{index}"))).map_err(|err| format!("{err}"))?;
    }
    let collected = collects_doctests(&root, Path::new("src/texts.py"));
    let _ = std::fs::remove_dir_all(&root);
    assert!(!collected);
    Ok(())
}

#[test]
fn a_trailing_comment_on_the_example_line_does_not_hide_the_comparison() -> Result<(), String> {
    let doc = "    >>> clip('abcdef', 3)  # == 'zzz'\n    'abc'\n";
    let (old, new) = pair("text[:n]", "text[:n + 1]", doc);
    let root = root("comment", ENABLED)?;
    let tests = doctest_tests(Path::new("src/texts.py"), &new, &old, &root);
    let _ = std::fs::remove_dir_all(&root);
    let [test] = tests.as_slice() else {
        return Err(format!("expected one test, got {}", tests.len()));
    };
    // The comment must not swallow `== 'abc'`: a commented-out comparison is a
    // bare call assertion, not the exact value the example states.
    let [assertion] = test.assertions.as_slice() else {
        return Err("expected one assertion".to_string());
    };
    assert!(assertion.text.contains("== 'abc'"), "{}", assertion.text);
    assert_eq!(assertion.oracle_strength, OracleStrength::Strong);
    let (old, new) = pair("text[:n]", "text[:n]", doc);
    assert_eq!(count("comment-same", ENABLED, &old, &new)?, 0);
    Ok(())
}

/// Whether `file` is collected with `tox.ini` and `pyproject.toml` as given.
fn tox_collects(label: &str, tox: &str, pyproject: &str) -> Result<bool, String> {
    let root = root(label, pyproject)?;
    std::fs::write(root.join("tox.ini"), tox).map_err(|err| format!("write: {err}"))?;
    let collected = collects_doctests(&root, Path::new("src/texts.py"));
    let _ = std::fs::remove_dir_all(&root);
    Ok(collected)
}

#[test]
fn a_tox_testenv_pytest_command_with_doctest_modules_is_collection() -> Result<(), String> {
    for tox in [
        "[testenv]\ncommands = pytest --doctest-modules --cov=pkg {posargs}\n",
        "[testenv]\ndeps = pytest\ncommands =\n    python -m pytest --doctest-modules\n",
        "[tox]\nenvlist = py\n\n[testenv]\ncommands = py.test --doctest-modules\n",
        // Each line is its own command: a value that starts inline and goes
        // on below is read line by line.
        "[testenv]\ncommands = python -c 'pass'\n    pytest --doctest-modules\n",
        "[testenv]\ncommands = pytest --doctest-modules\n    python -c 'pass'\n",
    ] {
        assert!(tox_collects("tox-on", tox, "")?, "{tox}");
    }
    Ok(())
}

#[test]
fn a_tox_command_that_cannot_be_read_as_one_bare_run_is_not_collection() -> Result<(), String> {
    for tox in [
        "[testenv]\ncommands = pytest\n",
        "[testenv]\ncommands = pytest --doctest-modules src\n",
        "[testenv]\ncommands = pytest --doctest-modules --ignore=src\n",
        "[testenv]\ncommands = pytest --doctest-modules && echo done\n",
        // A backslash joins the next line into the same command, which may
        // name a target; the stray token refuses the read.
        "[testenv]\ncommands = pytest --doctest-modules \\\n    src\n",
        "[testenv]\ncommands = py3: pytest --doctest-modules\n",
        "[testenv]\ncommands = {envpython} -m pytest --doctest-modules\n",
        "[testenv:docs]\ncommands = pytest --doctest-modules\n",
        "[testenv]\ndescription = pytest --doctest-modules\n",
    ] {
        assert!(!tox_collects("tox-off", tox, "")?, "{tox}");
    }
    Ok(())
}

#[test]
fn a_pyproject_pytest_table_overrides_the_tox_command() -> Result<(), String> {
    let tox = "[testenv]\ncommands = pytest --doctest-modules\n";
    for pyproject in [
        "[tool.pytest]\ntestpaths = [\"tests\"]\n",
        "[tool.pytest.ini_options]\ntestpaths = [\"tests\"]\n",
    ] {
        assert!(
            !tox_collects("tox-pyproject", tox, pyproject)?,
            "{pyproject}"
        );
    }
    Ok(())
}
