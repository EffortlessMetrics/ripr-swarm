//! Tests for the weak `cli_runner_invoke` relation (#9226): a test that
//! passes a click command to `CliRunner.invoke` reaches the command, with
//! reach-only evidence and no oracle.

use super::fixtures_tests::workspace;
use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const APP_PY: &str = "import click\n\n\n@click.command()\n@click.option('--name')\ndef hello(name):\n    click.echo(greeting(name))\n\n\ndef greeting(name):\n    if name:\n        return 'hi ' + name\n    return 'hi'\n";

fn finding_at(line: usize, text: &str, tests_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/app.py"), APP_PY);
    let tests = extract_tests(Path::new("tests/test_commands.py"), tests_source);
    classify_change(Path::new("src/app.py"), line, text, &owners, &tests)
        .ok_or_else(|| "expected a finding".to_string())
}

const LOCAL_RUNNER: &str = "from click.testing import CliRunner\nfrom src.app import hello\n\n\ndef test_run_with_flag():\n    runner = CliRunner()\n    result = runner.invoke(hello, ['--name', 'x'])\n    assert result.output == 'hi x\\n'\n";

fn relation_evidence(finding: &Finding) -> Vec<&str> {
    finding
        .evidence
        .iter()
        .map(String::as_str)
        .filter(|line| line.starts_with("related_test_relation:"))
        .collect()
}

#[test]
fn a_command_passed_to_a_local_runner_is_reached_without_an_oracle() -> Result<(), String> {
    let finding = finding_at(7, "    click.echo(greeting(name))", LOCAL_RUNNER)?;
    assert_ne!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        finding.evidence
    );
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_eq!(
        relation_evidence(&finding),
        vec!["related_test_relation: cli_runner_invoke (test_run_with_flag)"]
    );
    assert_eq!(finding.related_tests[0].oracle, None);
    Ok(())
}

#[test]
fn a_helper_the_command_calls_is_reached_through_the_invoke() -> Result<(), String> {
    let finding = finding_at(12, "    if name:", LOCAL_RUNNER)?;
    assert_ne!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        finding.evidence
    );
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line == "helper_owner_call: hello calls greeting (test_run_with_flag)"),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

const RUNNER_CONFTEST: &str = "import pytest\nfrom click.testing import CliRunner\n\n\n@pytest.fixture\ndef runner():\n    return CliRunner()\n";
const RUNNER_TEST: &str = "from src.app import hello\n\n\ndef test_run_with_flag(runner):\n    result = runner.invoke(hello, [])\n    assert result.exit_code == 0\n";

fn fixture_finding(conftest: &str, test: &str) -> Result<Finding, String> {
    let (owners, tests) = workspace(&[
        ("src/app.py", APP_PY),
        ("tests/conftest.py", conftest),
        ("tests/test_commands.py", test),
    ]);
    classify_change(
        Path::new("src/app.py"),
        7,
        "    click.echo(greeting(name))",
        &owners,
        &tests,
    )
    .ok_or_else(|| "expected a finding".to_string())
}

#[test]
fn a_literal_runner_fixture_is_a_runner() -> Result<(), String> {
    let finding = fixture_finding(RUNNER_CONFTEST, RUNNER_TEST)?;
    assert_ne!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        finding.evidence
    );
    assert_eq!(
        relation_evidence(&finding),
        vec!["related_test_relation: cli_runner_invoke (test_run_with_flag)"]
    );
    Ok(())
}

#[test]
fn a_runner_fixture_that_is_not_a_bare_literal_construction_is_not_trusted() -> Result<(), String> {
    let conftest = RUNNER_CONFTEST.replace("CliRunner()", "CliRunner(env=ENV)");
    let finding = fixture_finding(&conftest, RUNNER_TEST)?;
    assert!(
        !relation_evidence(&finding)
            .iter()
            .any(|line| line.contains("cli_runner_invoke")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

fn stays_unreached(tests: &str) -> Result<(), String> {
    let finding = finding_at(7, "    click.echo(greeting(name))", tests)?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        finding.evidence
    );
    assert!(finding.related_tests.is_empty());
    Ok(())
}

#[test]
fn shapes_that_do_not_prove_the_invoke_keep_no_static_path() -> Result<(), String> {
    // A runner from another module's `CliRunner`.
    stays_unreached(&LOCAL_RUNNER.replace("click.testing", "myhelpers"))?;
    // A command imported from another module.
    stays_unreached(&LOCAL_RUNNER.replace("src.app", "src.other"))?;
    // A receiver that is not a runner.
    stays_unreached(&LOCAL_RUNNER.replace("CliRunner()", "make_runner()"))?;
    // The command is not the first argument.
    stays_unreached(&LOCAL_RUNNER.replace("invoke(hello,", "invoke(other, hello,"))?;
    // An attribute of the command is passed, not the command.
    stays_unreached(&LOCAL_RUNNER.replace("invoke(hello,", "invoke(hello.callback,"))?;
    // The runner name is rebound before the invoke.
    stays_unreached(&LOCAL_RUNNER.replace("    result =", "    runner = other()\n    result ="))?;
    // The command name is shadowed by a parameter.
    stays_unreached(&LOCAL_RUNNER.replace(
        "def test_run_with_flag():",
        "def test_run_with_flag(hello):",
    ))?;
    Ok(())
}

#[test]
fn an_undecorated_function_passed_to_invoke_is_not_a_command() -> Result<(), String> {
    let source = APP_PY.replace("@click.command()\n@click.option('--name')\n", "");
    let owners = extract_owners(Path::new("src/app.py"), &source);
    let tests = extract_tests(Path::new("tests/test_commands.py"), LOCAL_RUNNER);
    let finding = classify_change(
        Path::new("src/app.py"),
        5,
        "    click.echo(greeting(name))",
        &owners,
        &tests,
    )
    .ok_or("expected a finding")?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn text_that_only_spells_the_runner_or_the_invoke_is_not_an_invoke() -> Result<(), String> {
    for tests in [
        LOCAL_RUNNER.replace(
            "    result = runner.invoke",
            "    # runner.invoke(hello, [])\n    result = other.invoke",
        ),
        LOCAL_RUNNER.replace(
            "    result = runner.invoke(hello, ['--name', 'x'])",
            "    print(\"runner.invoke(hello, [])\")",
        ),
        LOCAL_RUNNER.replace(
            "    runner = CliRunner()\n",
            "    \"\"\"\n    runner = CliRunner()\n    \"\"\"\n",
        ),
        LOCAL_RUNNER.replace(
            "    result =",
            "    for runner in make_runners():\n        pass\n    result =",
        ),
    ] {
        stays_unreached(&tests)?;
    }
    Ok(())
}

#[test]
fn spellings_the_relation_accepts() -> Result<(), String> {
    for tests in [
        LOCAL_RUNNER.replace("invoke(hello,", "invoke(\n        hello,"),
        LOCAL_RUNNER
            .replace("import CliRunner", "import CliRunner as CR")
            .replace("CliRunner()", "CR()"),
        LOCAL_RUNNER.replace("from click.testing", "from typer.testing"),
        LOCAL_RUNNER
            .replace("from src.app import hello", "import src.app as app")
            .replace("invoke(hello,", "invoke(app.hello,"),
    ] {
        let finding = finding_at(7, "    click.echo(greeting(name))", &tests)?;
        assert_ne!(
            finding.class,
            ExposureClass::NoStaticPath,
            "{tests}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn a_decorator_argument_call_is_not_a_helper_edge() -> Result<(), String> {
    let source = "import click\n\n\ndef make_default():\n    return 'x'\n\n\n@click.command()\n@click.option('--name', default=make_default())\ndef hello(name):\n    click.echo(name)\n";
    let owners = extract_owners(Path::new("src/app.py"), source);
    let tests = extract_tests(Path::new("tests/test_commands.py"), LOCAL_RUNNER);
    let finding = classify_change(
        Path::new("src/app.py"),
        5,
        "    return 'y'",
        &owners,
        &tests,
    )
    .ok_or("expected a finding")?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_click_name_that_is_not_the_click_module_is_not_a_command() -> Result<(), String> {
    let source = APP_PY.replace("import click", "from helpers import click");
    let owners = extract_owners(Path::new("src/app.py"), &source);
    let tests = extract_tests(Path::new("tests/test_commands.py"), LOCAL_RUNNER);
    let finding = classify_change(
        Path::new("src/app.py"),
        7,
        "    click.echo(greeting(name))",
        &owners,
        &tests,
    )
    .ok_or("expected a finding")?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        finding.evidence
    );
    Ok(())
}
