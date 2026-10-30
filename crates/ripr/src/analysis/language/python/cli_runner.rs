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

use super::related_tests::{owner_module_callees, test_body_binds_local};
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

fn is_click_command(owner: &PythonOwner) -> bool {
    owner.owner_kind == Some(OwnerKind::Function)
        && owner.class_path.is_empty()
        && owner.qualified_name == owner.name
        && owner
            .imports
            .iter()
            .any(|import| import.imported == "click" && import.alias == "click")
        && owner
            .decorators
            .iter()
            .any(|decorator| matches!(decorator.as_str(), "click.command" | "click.group"))
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
        for line in test.body_text.lines() {
            let Some((name, rhs)) = line.trim().split_once('=') else {
                continue;
            };
            let (name, rhs) = (name.trim(), rhs.trim_start());
            let constructs = rhs
                .strip_prefix(alias)
                .is_some_and(|rest| rest.starts_with('('));
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
        || test.body_text.contains(&format!(" as {name}"))
}

/// `<receiver>.invoke(<callee>` followed by `,` or `)`: the command object
/// itself is the first argument, not a call or attribute of it.
fn invokes_command(body: &str, receiver: &str, callee: &str) -> bool {
    let needle = format!("{receiver}.invoke(");
    body.match_indices(&needle).any(|(index, _)| {
        let boundary = body[..index]
            .chars()
            .next_back()
            .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_' || ch == '.'));
        let rest = body[index + needle.len()..].trim_start();
        boundary
            && rest
                .strip_prefix(callee)
                .is_some_and(|after| after.trim_start().starts_with([',', ')']))
    })
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}
