//! A conftest fixture that only builds a `CliRunner` with literal arguments
//! hides no discriminator or expected value, so it does not trigger the
//! `unresolved_pytest_fixture` limit (#11016, pallets/click's `runner`). Every
//! other shape, and anything that could make the runner differ from an inlined
//! `CliRunner()`, keeps the limit.

use super::fixtures_tests::workspace;
use super::*;
use std::path::Path;

const SHOP: &str = "def total(amount):\n    return amount * 2\n";

const CONFTEST: &str = "import pytest\n\nfrom click.testing import CliRunner\n\n\n@pytest.fixture(scope=\"function\")\ndef runner(request):\n    return CliRunner()\n";

const TEST: &str = "from shop import total\n\n\ndef test_total(runner):\n    assert runner is not None\n    assert total(3) == 6\n";

fn classify_total(conftest: &str, test: &str) -> Result<Finding, String> {
    let (owners, tests) = workspace(&[
        ("shop/__init__.py", SHOP),
        ("tests/conftest.py", conftest),
        ("tests/test_shop.py", test),
    ]);
    classify_change(
        Path::new("shop/__init__.py"),
        2,
        "    return amount * 3",
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed line must classify".to_string())
}

/// The finding keeps a static limit. Top-level code that writes an attribute
/// of an imported name, or rebinds it, is also a substitution form
/// (`fixture_substitution.rs`), so the `mocked_module` limit may speak first;
/// either limit withholds the credit the inert-tool read would restore.
fn assert_limited(finding: &Finding, case: &str) {
    assert!(
        matches!(
            finding.static_limit_kind,
            Some(StaticLimitKind::UnresolvedPytestFixture | StaticLimitKind::MockedModule)
        ),
        "{case}: {:?} {:?}",
        finding.static_limit_kind,
        finding.evidence
    );
}

#[test]
fn a_clirunner_fixture_does_not_hide_the_expected_value() -> Result<(), String> {
    let finding = classify_total(CONFTEST, TEST)?;
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    assert_eq!(finding.class, ExposureClass::Exposed);
    // Without the parameter, and with a literal keyword, the shape holds.
    let variant = CONFTEST
        .replace("def runner(request):", "def runner():")
        .replace("CliRunner()", "CliRunner(echo_stdin=True)");
    assert_eq!(classify_total(&variant, TEST)?.static_limit_kind, None);
    Ok(())
}

#[test]
fn shapes_that_can_hide_state_keep_the_limit() -> Result<(), String> {
    let cases = [
        (
            "env mapping",
            CONFTEST.replace("CliRunner()", "CliRunner(env={\"A\": \"1\"})"),
        ),
        (
            "non-literal argument",
            CONFTEST.replace("CliRunner()", "CliRunner(START)"),
        ),
        (
            "request.param is not read, but extra statements are",
            CONFTEST.replace(
                "    return CliRunner()",
                "    runner = CliRunner()\n    runner.env = request.param\n    return runner",
            ),
        ),
        (
            "module scope",
            CONFTEST.replace("scope=\"function\"", "scope=\"module\""),
        ),
        (
            "autouse keyword",
            CONFTEST.replace("scope=\"function\"", "autouse=True"),
        ),
        (
            "another module's CliRunner",
            CONFTEST.replace("from click.testing import", "from helpers import"),
        ),
        (
            "other class from click.testing",
            CONFTEST.replace("CliRunner", "Result"),
        ),
        (
            "star import in the conftest",
            format!("from helpers import *\n{CONFTEST}"),
        ),
        (
            "another fixture takes the runner",
            format!(
                "{CONFTEST}\n\n@pytest.fixture(autouse=True)\ndef seed(runner):\n    runner.env = {{}}\n"
            ),
        ),
    ];
    for (case, conftest) in cases {
        assert_limited(&classify_total(&conftest, TEST)?, case);
    }
    Ok(())
}

#[test]
fn a_test_that_rebinds_the_runner_or_overrides_it_keeps_the_limit() -> Result<(), String> {
    let rebound = TEST.replace(
        "    assert runner is not None",
        "    runner = make()\n    assert runner is not None",
    );
    assert_limited(&classify_total(CONFTEST, &rebound)?, "rebound local");
    let overridden = format!(
        "import pytest\nfrom shop import total\n\n\n@pytest.fixture\ndef runner():\n    return object()\n\n\n{}",
        TEST.trim_start_matches("from shop import total\n\n\n")
    );
    assert_limited(
        &classify_total(CONFTEST, &overridden)?,
        "same-file override",
    );
    Ok(())
}

#[test]
fn a_workspace_local_or_relative_clirunner_is_not_the_inert_tool() -> Result<(), String> {
    // `from ..click.testing import CliRunner` in tests/conftest.py resolves to
    // the dotted path `click.testing` but names a workspace-local module.
    for (case, import) in [
        ("relative", "from ..click.testing import CliRunner"),
        ("renamed", "from click.testing import CliRunner as Runner"),
        ("module import", "import click.testing"),
    ] {
        let conftest = CONFTEST.replace("from click.testing import CliRunner", import);
        let conftest = if case == "module import" {
            conftest.replace("CliRunner()", "click.testing.CliRunner()")
        } else {
            conftest
        };
        assert_limited(&classify_total(&conftest, TEST)?, case);
    }
    Ok(())
}

#[test]
fn patching_the_runner_class_keeps_the_limit() -> Result<(), String> {
    let patched_conftest = format!("{CONFTEST}\n\nCliRunner.invoke = lambda self, *a, **k: None\n");
    assert_limited(
        &classify_total(&patched_conftest, TEST)?,
        "class patched in the conftest",
    );
    let patched_test = format!("{TEST}\n\nCliRunner.invoke = lambda self, *a, **k: None\n");
    assert_limited(
        &classify_total(CONFTEST, &patched_test)?,
        "class patched in the test module",
    );
    Ok(())
}

#[test]
fn a_nearer_conftest_or_a_class_method_test_keeps_the_limit() -> Result<(), String> {
    let (owners, tests) = workspace(&[
        ("shop/__init__.py", SHOP),
        ("tests/conftest.py", CONFTEST),
        (
            "tests/sub/conftest.py",
            "import pytest\n\n\n@pytest.fixture\ndef runner():\n    return object()\n",
        ),
        ("tests/sub/test_shop.py", TEST),
    ]);
    let nearer = classify_change(
        Path::new("shop/__init__.py"),
        2,
        "    return amount * 3",
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed line must classify".to_string())?;
    assert_limited(&nearer, "nearer conftest override");
    let method = "from shop import total\n\n\nclass TestShop:\n    def test_total(self, runner):\n        assert runner is not None\n        assert total(3) == 6\n";
    assert_limited(&classify_total(CONFTEST, method)?, "class-method test");
    Ok(())
}

#[test]
fn an_imported_autouse_fixture_that_takes_the_runner_keeps_the_limit() -> Result<(), String> {
    // `from helpers import prime` registers an autouse fixture that can seed
    // the runner before the test reads it; without that import the same
    // scope reads as inert.
    let helpers = "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef prime(runner):\n    runner.expected = 9\n";
    let with_import = format!("from helpers import prime\n{TEST}");
    let build = |test: &str| {
        let (owners, tests) = workspace(&[
            ("shop/__init__.py", SHOP),
            ("tests/conftest.py", CONFTEST),
            ("tests/helpers.py", helpers),
            ("tests/test_shop.py", test),
        ]);
        classify_change(
            Path::new("shop/__init__.py"),
            2,
            "    return amount * 3",
            &owners,
            &tests,
        )
        .ok_or_else(|| "the changed line must classify".to_string())
    };
    assert_limited(&build(&with_import)?, "imported autouse fixture");
    let control = build(TEST)?;
    assert_eq!(control.static_limit_kind, None, "{:?}", control.evidence);
    Ok(())
}

fn classify_files(files: &[(&str, &str)]) -> Result<Finding, String> {
    let (owners, tests) = workspace(files);
    classify_change(
        Path::new("shop/__init__.py"),
        2,
        "    return amount * 3",
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed line must classify".to_string())
}

#[test]
fn indirect_class_patching_keeps_the_limit() -> Result<(), String> {
    let autouse = |body: &str| {
        format!("{CONFTEST}\n\n@pytest.fixture(autouse=True)\ndef _p(monkeypatch):\n    {body}\n")
    };
    let cases = [
        (
            "dotted base",
            format!("{CONFTEST}\nimport click.testing\n\nclick.testing.CliRunner.invoke = None\n"),
        ),
        (
            "alias",
            format!("{CONFTEST}\nRunner = CliRunner\nRunner.invoke = None\n"),
        ),
        (
            "setattr",
            format!("{CONFTEST}\nsetattr(CliRunner, \"invoke\", None)\n"),
        ),
        (
            "monkeypatch",
            autouse("monkeypatch.setattr(CliRunner, \"invoke\", None)"),
        ),
        (
            "mock.patch.object",
            autouse("mock.patch.object(CliRunner, \"invoke\", None)"),
        ),
        (
            "patch string",
            autouse("mock.patch(\"click.testing.CliRunner.invoke\")"),
        ),
    ];
    for (case, conftest) in cases {
        assert_limited(&classify_total(&conftest, TEST)?, case);
    }
    Ok(())
}

#[test]
fn an_imported_module_that_patches_or_re_exports_keeps_the_limit() -> Result<(), String> {
    let patches = "from click.testing import CliRunner\n\nCliRunner.invoke = None\n\n\ndef apply():\n    pass\n";
    let run = |conftest: &str, extra: Option<(&str, &str)>| {
        let mut files = vec![
            ("shop/__init__.py", SHOP),
            ("tests/test_shop.py", TEST),
            ("tests/patches.py", patches),
            ("tests/conftest.py", conftest),
        ];
        files.extend(extra);
        classify_files(&files)
    };
    assert_limited(
        &run(&format!("{CONFTEST}\nfrom patches import apply\n"), None)?,
        "from import of a patching module",
    );
    assert_limited(
        &run(&format!("{CONFTEST}\nimport patches\n"), None)?,
        "plain import of a patching module",
    );
    assert_limited(
        &run(
            &format!("{CONFTEST}\nfrom helpers import apply\n"),
            Some(("tests/helpers.py", "from patches import apply\n")),
        )?,
        "patching module reached through a re-export",
    );
    let control = run(
        &format!("{CONFTEST}\nfrom helpers import apply\n"),
        Some(("tests/helpers.py", "def apply():\n    pass\n")),
    )?;
    assert_eq!(control.static_limit_kind, None, "{:?}", control.evidence);
    Ok(())
}

#[test]
fn a_semicolon_or_redefinition_does_not_hide_a_class_patch() -> Result<(), String> {
    for (case, tail) in [
        (
            "semicolon after an import",
            "\nimport os; CliRunner.invoke = None\n",
        ),
        (
            "def redefinition",
            "\ndef CliRunner(*a, **k):\n    return object()\n",
        ),
        ("class redefinition", "\nclass CliRunner:\n    pass\n"),
    ] {
        assert_limited(&classify_total(&format!("{CONFTEST}{tail}"), TEST)?, case);
    }
    Ok(())
}

#[test]
fn the_tool_library_in_the_workspace_is_not_a_patch() -> Result<(), String> {
    // pallets/click's own tree holds `class CliRunner` in click/testing.py.
    let testing = "class CliRunner:\n    def invoke(self):\n        return None\n";
    let finding = classify_files(&[
        ("shop/__init__.py", SHOP),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_shop.py", TEST),
        ("src/click/testing.py", testing),
    ])?;
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn stored_class_writes_keep_only_inert_tool_names_once() {
    // Every file's facts are persisted and decoded on each warm run, so the
    // stored list holds only the names `inert_tool` can be, each once.
    let patched = "from click.testing import CliRunner\nimport helpers\n\nCliRunner.invoke = helpers.fake\nsetattr(CliRunner, \"invoke\", helpers.fake)\nother = helpers.value + 1\n";
    let facts = source_facts::extract_source_facts(Path::new("tests/conftest.py"), patched);
    assert_eq!(
        facts.fixture_side_effects.class_writes,
        vec!["CliRunner".to_string()]
    );

    let quoted = "target = \"click.testing.CliRunner.invoke\"\n";
    let facts = source_facts::extract_source_facts(Path::new("tests/conftest.py"), quoted);
    assert_eq!(
        facts.fixture_side_effects.class_writes,
        vec!["CliRunner".to_string()]
    );

    let constructs_only =
        "from click.testing import CliRunner\n\nrunner = CliRunner()\nvalue = runner.invoke\n";
    let facts = source_facts::extract_source_facts(Path::new("tests/conftest.py"), constructs_only);
    assert!(facts.fixture_side_effects.class_writes.is_empty());
}
