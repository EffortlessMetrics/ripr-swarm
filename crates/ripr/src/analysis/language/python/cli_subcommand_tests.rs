//! Tests for group-subcommand reach through `CliRunner.invoke(group, [name])`
//! (#9226): a weak, reach-only relation that never earns `exposed`.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const GROUP_PY: &str = "import click\n\n\n@click.group()\ndef cli():\n    pass\n\n\n@cli.command()\n@click.option('--n', default=1)\ndef greet(n):\n    return n + 1\n";
const CHANGED_TEXT: &str = "    return n + 1";

const INVOKE_TEST: &str = "from click.testing import CliRunner\nfrom src.app import cli\n\n\ndef test_greet():\n    runner = CliRunner()\n    result = runner.invoke(cli, ['greet', '--n', '2'])\n    assert result.exit_code == 0\n";

fn finding_for(app: &str, tests_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/app.py"), app);
    let tests = extract_tests(Path::new("tests/test_app.py"), tests_source);
    let line = app
        .lines()
        .position(|text| text == CHANGED_TEXT)
        .map(|index| index + 1)
        .ok_or("the changed line is not in the app")?;
    let finding = classify_change(Path::new("src/app.py"), line, CHANGED_TEXT, &owners, &tests)
        .ok_or_else(|| "expected a finding".to_string())?;
    // The edit must land in the subcommand function, not fall to the module.
    assert!(
        finding
            .evidence
            .iter()
            .any(|text| text.starts_with("owner: ") && !text.starts_with("owner: <module>")),
        "{:?}",
        finding.evidence
    );
    Ok(finding)
}

fn invoke_relation(finding: &Finding) -> bool {
    finding
        .evidence
        .iter()
        .any(|line| line.starts_with("related_test_relation: cli_runner_invoke"))
}

fn limit_kind(finding: &Finding) -> Option<&str> {
    finding
        .evidence
        .iter()
        .find_map(|line| line.strip_prefix("static_limit "))
}

#[test]
fn a_subcommand_named_in_the_invoke_arguments_is_reached_without_an_oracle() -> Result<(), String> {
    let finding = finding_for(GROUP_PY, INVOKE_TEST)?;
    assert!(invoke_relation(&finding), "{:?}", finding.evidence);
    assert_ne!(finding.class, ExposureClass::NoStaticPath);
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_eq!(limit_kind(&finding), None, "{:?}", finding.evidence);
    assert_eq!(finding.related_tests[0].oracle, None);
    Ok(())
}

#[test]
fn the_registration_spellings_the_relation_accepts() -> Result<(), String> {
    for (app, tests) in [
        // explicit name
        (
            GROUP_PY.replace("@cli.command()", "@cli.command('hello')"),
            INVOKE_TEST.replace("'greet'", "'hello'"),
        ),
        (
            GROUP_PY.replace("@cli.command()", "@cli.command(name='hello')"),
            INVOKE_TEST.replace("'greet'", "'hello'"),
        ),
        // bare decorator, and descriptive keywords
        (
            GROUP_PY.replace("@cli.command()", "@cli.command"),
            INVOKE_TEST.to_string(),
        ),
        (
            GROUP_PY.replace("@cli.command()", "@cli.command(help='x')"),
            INVOKE_TEST.to_string(),
        ),
        // click lowers and hyphenates an unnamed command
        (
            GROUP_PY.replace("def greet(", "def Say_Hi("),
            INVOKE_TEST.replace("'greet'", "'say-hi'"),
        ),
        // args= keyword, double quotes, the group through its module
        (
            GROUP_PY.to_string(),
            INVOKE_TEST.replace("cli, ['greet', '--n', '2']", "cli, args=[\"greet\"]"),
        ),
        (
            GROUP_PY.to_string(),
            INVOKE_TEST
                .replace("from src.app import cli", "from src import app")
                .replace("invoke(cli,", "invoke(app.cli,"),
        ),
    ] {
        let finding = finding_for(&app, &tests)?;
        assert!(
            invoke_relation(&finding),
            "{app}\n{tests}\n{:?}",
            finding.evidence
        );
        assert_ne!(finding.class, ExposureClass::Exposed);
    }
    Ok(())
}

fn stays_unreached(app: &str, tests: &str) -> Result<(), String> {
    let finding = finding_for(app, tests)?;
    assert!(
        !invoke_relation(&finding),
        "{app}\n{tests}\n{:?}",
        finding.evidence
    );
    // The registration stays an indirection: nothing lifted its limit.
    assert!(
        limit_kind(&finding).is_some_and(|kind| kind.starts_with("decorator_indirection")),
        "{app}\n{tests}\n{:?}",
        finding.evidence
    );
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn an_invoke_that_does_not_name_the_subcommand_first_makes_no_edge() -> Result<(), String> {
    // A different subcommand name, an option before the name, a prefix of the
    // name, a name built at run time, a command line string, the subcommand
    // function itself and a name beyond the first element.
    for invoke in [
        "cli, ['other']",
        "cli, ['--verbose', 'greet']",
        "cli, ['gre']",
        "cli, ['greeting']",
        "cli, [name]",
        "cli, 'greet'",
        "cli, ['x', 'greet']",
        "cli",
        "cli, []",
        "greet, []",
    ] {
        stays_unreached(
            GROUP_PY,
            &INVOKE_TEST.replace("cli, ['greet', '--n', '2']", invoke),
        )?;
    }
    Ok(())
}

#[test]
fn a_test_that_does_not_prove_the_group_or_the_runner_makes_no_edge() -> Result<(), String> {
    // The group imported from another module, a runner from another module, a
    // receiver that is not a runner and a group name a parameter shadows.
    stays_unreached(GROUP_PY, &INVOKE_TEST.replace("src.app", "src.other"))?;
    stays_unreached(GROUP_PY, &INVOKE_TEST.replace("click.testing", "helpers"))?;
    stays_unreached(
        GROUP_PY,
        &INVOKE_TEST.replace("CliRunner()", "make_runner()"),
    )?;
    stays_unreached(
        GROUP_PY,
        &INVOKE_TEST.replace("def test_greet():", "def test_greet(cli):"),
    )?;
    Ok(())
}

#[test]
fn a_registration_the_file_does_not_prove_makes_no_edge() -> Result<(), String> {
    for app in [
        // not a click group
        GROUP_PY.replace("@click.group()\n", ""),
        GROUP_PY.replace("@click.group()", "@click.group(cls=MyGroup)"),
        // a decorator that may wrap the command
        GROUP_PY.replace("@click.option('--n', default=1)", "@retry(3)"),
        // a second registration under the same name
        format!("{GROUP_PY}\n\n@cli.command('greet')\ndef again():\n    pass\n"),
        // a registration whose name cannot be read
        format!("{GROUP_PY}\n\n@cli.command(NAME)\ndef later():\n    pass\n"),
        // a registration added or replaced by other means
        format!("{GROUP_PY}\n\ncli.add_command(other, 'greet')\n"),
        format!("{GROUP_PY}\n\ncli.commands['greet'] = other\n"),
        format!("{GROUP_PY}\n\ncli.command()(extra)\n"),
        // the group or the command rebound
        format!("{GROUP_PY}\n\ncli = other_group\n"),
        format!("{GROUP_PY}\n\ngreet = other\n"),
        // an async command
        GROUP_PY.replace("def greet(", "async def greet("),
        // click is not the plain import
        GROUP_PY.replace("import click", "import click_helpers as click"),
    ] {
        stays_unreached(&app, INVOKE_TEST)?;
    }
    Ok(())
}

#[test]
fn a_derived_name_click_versions_disagree_on_is_refused_but_an_explicit_one_is_read()
-> Result<(), String> {
    let suffixed = GROUP_PY.replace("def greet(", "def greet_command(");
    stays_unreached(&suffixed, INVOKE_TEST)?;
    stays_unreached(
        &suffixed,
        &INVOKE_TEST.replace("'greet'", "'greet-command'"),
    )?;
    let named = suffixed.replace("@cli.command()", "@cli.command('greet')");
    let finding = finding_for(&named, INVOKE_TEST)?;
    assert!(invoke_relation(&finding), "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn a_subcommand_no_test_invokes_keeps_its_decorator_limit() -> Result<(), String> {
    let finding = finding_for(GROUP_PY, "def test_unrelated():\n    assert True\n")?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.evidence
    );
    assert!(
        limit_kind(&finding).is_some_and(|kind| kind.starts_with("decorator_indirection")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}
