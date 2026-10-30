//! Weak `cli_runner_invoke` relation (#9226): a test that runs a click command
//! through `CliRunner.invoke(command, ...)` reaches the command function by
//! passing it, not by calling it, so the direct-call relations never see it.
//!
//! Structural and fail-closed. The owner must be a top-level function that
//! `click.command` / `click.group` decorates (click imported by the owner's
//! module); the test must name that function through a provenance-checked
//! import (`owner_module_callees`) and pass it as the first argument of
//! `<receiver>.invoke(`, where the receiver is a local bound once to
//! `CliRunner(...)` imported from `click.testing` / `typer.testing`, or a
//! literal `CliRunner` fixture. Reach evidence only: the relation carries no
//! oracle, so it never earns `exposed`.

use super::related_tests::{
    owner_module_callees, python_text_hides_code, test_body_binds_local, test_imports_owner_module,
    test_rebinds_import_alias, text_binds_beyond_assignment,
};
use super::{PythonImport, PythonOwner, PythonTest};
use crate::domain::OwnerKind;

const RUNNER_MODULES: [&str; 2] = ["click.testing", "typer.testing"];

pub(super) fn cli_runner_invokes_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    if !is_click_command(owner) || !test.body_text.contains(".invoke(") {
        return false;
    }
    let callees = owner_module_callees(test, owner);
    if callees.is_empty() {
        return false;
    }
    runner_receivers(test).iter().any(|receiver| {
        callees
            .iter()
            .any(|callee| invokes_command(&test.body_text, receiver, callee))
    })
}

/// #11069: a test that imports the owner's module or a package above it and
/// calls `.invoke(` on a recognised fresh `CliRunner`. The command it passes is
/// not followed, so the owner stays `no_static_path`; the test is only named so
/// the repair text does not ask for a test the project may already have.
/// The earliest such test by file and line, for a stable pointer.
pub(super) fn runner_invoking_test<'a>(
    owner: &PythonOwner,
    tests: &'a [PythonTest],
) -> Option<&'a PythonTest> {
    tests
        .iter()
        .filter(|test| {
            test.body_text.contains(".invoke(") && test_imports_owner_module(test, owner)
        })
        .filter(|test| {
            runner_receivers(test)
                .iter()
                .any(|receiver| invokes_receiver(&test.body_text, receiver))
        })
        .min_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)))
}

fn is_click_command(owner: &PythonOwner) -> bool {
    owner.owner_kind == Some(OwnerKind::Function)
        && owner.class_path.is_empty()
        && owner.qualified_name == owner.name
        && owner.imports.iter().any(is_plain_click_import)
        && owner
            .decorators
            .iter()
            .any(|decorator| matches!(decorator.as_str(), "click.command" | "click.group"))
        && owner.decorators.iter().all(|decorator| {
            matches!(
                decorator.as_str(),
                "click.command" | "click.group" | "click.option" | "click.argument"
            )
        })
}

/// `import click`, not `from helpers import click`.
pub(super) fn is_plain_click_import(import: &PythonImport) -> bool {
    import.imported == "click" && import.alias == "click" && import.source_module.is_empty()
}

/// Whether `rhs` is exactly one `alias(...)` construction and nothing else:
/// no conditional, boolean, attribute or call chained after it.
fn is_sole_construction(rhs: &str, alias: &str) -> bool {
    let Some(rest) = rhs.strip_prefix(alias).filter(|rest| rest.starts_with('(')) else {
        return false;
    };
    let mut depth = 0usize;
    for (index, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let after = rest[index + 1..].trim();
                    return after.is_empty() || after.starts_with('#');
                }
            }
            _ => {}
        }
    }
    false
}

fn runner_alias(imports: &[PythonImport]) -> Option<&str> {
    imports
        .iter()
        .find(|import| {
            import.imported == "CliRunner"
                && RUNNER_MODULES.contains(&import.source_module.as_str())
        })
        .map(|import| import.alias.as_str())
}

/// Names the test can call `.invoke(` on that are a fresh `CliRunner`.
fn runner_receivers(test: &PythonTest) -> Vec<String> {
    let mut receivers = Vec::new();
    if let Some(alias) = runner_alias(&test.imports) {
        let mut offset = 0;
        for line in test.body_text.split_inclusive('\n') {
            let start = offset;
            offset += line.len();
            let indent = line.len() - line.trim_start().len();
            if python_text_hides_code(&test.body_text, start + indent) {
                continue;
            }
            let Some((name, rhs)) = line.trim().split_once('=') else {
                continue;
            };
            let (name, rhs) = (name.trim(), rhs.trim_start());
            let constructs =
                is_sole_construction(rhs, alias) && !test_rebinds_import_alias(test, alias);
            if constructs && is_identifier(name) && !bound_beyond_one_assignment(test, name) {
                receivers.push(name.to_string());
            }
        }
    }
    for def in &test.fixture_defs {
        let Some(alias) = runner_alias(&def.imports) else {
            continue;
        };
        if def.isolated
            && def.literal_construction.as_deref() == Some(alias)
            && test.fixtures.contains(&def.name)
            && !test_body_binds_local(test, &def.name)
        {
            receivers.push(def.name.clone());
        }
    }
    receivers.sort();
    receivers.dedup();
    receivers
}

/// Whether `name` is bound anywhere but its single `name = CliRunner(...)`
/// assignment: a parameter or fixture, a second assignment, a `with ... as`
/// target or an import alias.
fn bound_beyond_one_assignment(test: &PythonTest, name: &str) -> bool {
    let assignments = test
        .body_text
        .lines()
        .filter(|line| {
            line.trim().strip_prefix(name).is_some_and(|rest| {
                let rest = rest.trim_start();
                rest.starts_with('=') && !rest.starts_with("==")
            })
        })
        .count();
    assignments != 1
        || test.fixtures.iter().any(|fixture| fixture == name)
        || test.imports.iter().any(|import| import.alias == name)
        || text_binds_beyond_assignment(&test.body_text, name)
}

/// `<receiver>.invoke(<callee>` followed by `,` or `)`: the command object
/// itself is the first argument, not a call or attribute of it.
fn invokes_command(body: &str, receiver: &str, callee: &str) -> bool {
    let needle = format!("{receiver}.invoke(");
    body.match_indices(&needle).any(|(index, _)| {
        let rest = body[index + needle.len()..].trim_start();
        invoke_at_boundary(body, index)
            && rest
                .strip_prefix(callee)
                .is_some_and(|after| after.trim_start().starts_with([',', ')']))
    })
}

/// Whether `<receiver>.invoke(` at `index` is live code on a whole name.
fn invoke_at_boundary(body: &str, index: usize) -> bool {
    !python_text_hides_code(body, index)
        && body[..index]
            .chars()
            .next_back()
            .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_' || ch == '.'))
}

fn invokes_receiver(body: &str, receiver: &str) -> bool {
    let needle = format!("{receiver}.invoke(");
    body.match_indices(&needle)
        .any(|(index, _)| invoke_at_boundary(body, index))
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}
