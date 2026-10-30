//! Group-subcommand facts for the weak `cli_runner_invoke` relation (#9226).
//!
//! `@cli.command()` over a function registers a subcommand on the group `cli`.
//! A test reaches it by invoking the group with the subcommand's name
//! (`runner.invoke(cli, ["greet"])`), never by naming the function. The name a
//! test passes is only the function's when this file shows exactly one
//! registration under it, so the pass is structural and fail-closed:
//!
//! - `import click` is the file's only binding of `click`;
//! - the group is a top-level function bound once, decorated only by
//!   `click.group` (no `cls=`), `click.option` and `click.argument`;
//! - the subcommand is a top-level synchronous function bound once, decorated
//!   by one `<group>.command` registration and `click.option` /
//!   `click.argument` only;
//! - the registration names the command by a string literal or by the
//!   function's own name, which click lowers and hyphenates (a name click
//!   would also strip a `_command` / `_cmd` / `_group` / `_grp` suffix from
//!   differs between click versions, so it is refused);
//! - every registration on the group is readable and no two share a name;
//! - the text `<group>.command` / `<group>.group` appears only as those
//!   decorators, and the file never spells `add_command` or `.commands`,
//!   which can add, replace or remove a registration.

use super::fixtures::module_binding_count;
use super::{PythonImport, PythonOwner, expr_full_name};
use crate::domain::OwnerKind;
use rustpython_parser::ast::{self, Constant, Expr, Stmt};

/// The group a subcommand owner is registered on and the name a test passes
/// to the group to reach it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct CliSubcommand {
    pub(super) group: String,
    pub(super) name: String,
}

impl CliSubcommand {
    /// The decorator name (`cli.command`) that registers the subcommand.
    pub(super) fn decorator(&self) -> String {
        format!("{}.command", self.group)
    }
}

/// Keywords of `@group.command(...)` that only describe the command.
const DESCRIPTIVE_KEYWORDS: [&str; 6] = [
    "help",
    "short_help",
    "epilog",
    "hidden",
    "deprecated",
    "no_args_is_help",
];

const NAME_SUFFIXES: [&str; 4] = ["_command", "_cmd", "_group", "_grp"];

/// One `@<group>.command` or `@<group>.group` decorator.
struct Registration {
    group: String,
    is_command: bool,
    /// The name the registration gives, `None` when it cannot be read.
    name: Option<String>,
}

fn registration(decorator: &Expr, function_name: &str) -> Option<Registration> {
    let (attribute, call) = match decorator {
        Expr::Call(call) => (call.func.as_ref(), Some(call)),
        other => (other, None),
    };
    let Expr::Attribute(attribute) = attribute else {
        return None;
    };
    let is_command = match attribute.attr.as_str() {
        "command" => true,
        "group" => false,
        _ => return None,
    };
    let Expr::Name(group) = attribute.value.as_ref() else {
        return None;
    };
    Some(Registration {
        group: group.id.to_string(),
        is_command,
        name: registered_name(call, function_name),
    })
}

fn registered_name(call: Option<&ast::ExprCall>, function_name: &str) -> Option<String> {
    let Some(call) = call else {
        return derived_name(function_name);
    };
    let mut explicit: Option<String> = None;
    match call.args.as_slice() {
        [] => {}
        [Expr::Constant(constant)] => explicit = Some(string_literal(constant)?),
        _ => return None,
    }
    for keyword in &call.keywords {
        let argument = keyword.arg.as_ref()?;
        if argument.as_str() == "name" {
            let Expr::Constant(constant) = &keyword.value else {
                return None;
            };
            if explicit.is_some() {
                return None;
            }
            explicit = Some(string_literal(constant)?);
        } else if !DESCRIPTIVE_KEYWORDS.contains(&argument.as_str()) {
            return None;
        }
    }
    match explicit {
        Some(name) => Some(name),
        None => derived_name(function_name),
    }
}

fn string_literal(constant: &ast::ExprConstant) -> Option<String> {
    match &constant.value {
        Constant::Str(text) if !text.is_empty() => Some(text.to_string()),
        _ => None,
    }
}

/// The name click gives an unnamed command: the function name lowered with
/// underscores as hyphens.
fn derived_name(function_name: &str) -> Option<String> {
    if !function_name.is_ascii() {
        return None;
    }
    let lowered = function_name.to_ascii_lowercase();
    if NAME_SUFFIXES.iter().any(|suffix| lowered.ends_with(suffix)) {
        return None;
    }
    Some(lowered.replace('_', "-"))
}

/// The click decorators that build a command without changing its call.
fn is_click_decorator(decorator: &Expr) -> bool {
    expr_full_name(decorator).is_some_and(|name| {
        matches!(
            name.as_str(),
            "click.command" | "click.group" | "click.option" | "click.argument"
        )
    })
}

/// Whether the decorator is `click.group` without a replacement `cls`.
fn is_plain_group_decorator(decorator: &Expr) -> bool {
    if expr_full_name(decorator).as_deref() != Some("click.group") {
        return false;
    }
    match decorator {
        Expr::Call(call) => {
            call.args.len() <= 1
                && call.keywords.iter().all(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() != "cls")
                })
        }
        _ => true,
    }
}

fn is_group_function(function: &ast::StmtFunctionDef) -> bool {
    function
        .decorator_list
        .iter()
        .any(|decorator| expr_full_name(decorator).as_deref() == Some("click.group"))
        && function.decorator_list.iter().all(is_click_decorator)
        && function
            .decorator_list
            .iter()
            .filter(|decorator| expr_full_name(decorator).as_deref() == Some("click.group"))
            .all(is_plain_group_decorator)
}

/// How often `needle` is written at an identifier boundary: not after a name
/// character or `.`, and not continuing into a longer name.
fn spelled_count(source: &str, needle: &str) -> usize {
    source
        .match_indices(needle)
        .filter(|(index, _)| {
            source[..*index]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_' || ch == '.'))
                && source[index + needle.len()..]
                    .chars()
                    .next()
                    .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'))
        })
        .count()
}

/// Records on each owner that is a click subcommand registered on a group of
/// its own file the group and the name a test passes to reach it.
pub(super) fn mark_cli_subcommands(
    source: &str,
    statements: &[Stmt],
    imports: &[PythonImport],
    owners: &mut [PythonOwner],
) {
    if !imports.iter().any(super::cli_runner::is_plain_click_import)
        || module_binding_count(statements, source, "click") != Some(1)
        || source.contains("add_command")
        || source.contains(".commands")
    {
        return;
    }
    let functions: Vec<&ast::StmtFunctionDef> = statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::FunctionDef(function) => Some(function),
            _ => None,
        })
        .collect();
    for function in &functions {
        let registrations: Vec<Registration> = function
            .decorator_list
            .iter()
            .filter_map(|decorator| registration(decorator, function.name.as_str()))
            .collect();
        let [only] = registrations.as_slice() else {
            continue;
        };
        let Some(name) = only.name.clone().filter(|_| only.is_command) else {
            continue;
        };
        if !function.decorator_list.iter().all(|decorator| {
            expr_full_name(decorator).is_some_and(|spelled| {
                matches!(spelled.as_str(), "click.option" | "click.argument")
            }) || registration(decorator, function.name.as_str()).is_some()
        }) || module_binding_count(statements, source, function.name.as_str()) != Some(1)
            || !group_is_trusted(source, statements, &functions, &only.group)
        {
            continue;
        }
        for owner in owners.iter_mut().filter(|owner| {
            owner.name == function.name.as_str()
                && owner.class_path.is_empty()
                && owner.owner_kind == Some(OwnerKind::Function)
        }) {
            owner.cli_subcommand = Some(CliSubcommand {
                group: only.group.clone(),
                name: name.clone(),
            });
        }
    }
}

/// Whether `group` is a plain click group bound once whose registrations in
/// this file are all readable, distinct and the only spelling of its
/// `.command` / `.group`.
fn group_is_trusted(
    source: &str,
    statements: &[Stmt],
    functions: &[&ast::StmtFunctionDef],
    group: &str,
) -> bool {
    if module_binding_count(statements, source, group) != Some(1)
        || !functions
            .iter()
            .any(|function| function.name.as_str() == group && is_group_function(function))
    {
        return false;
    }
    let mut names: Vec<String> = Vec::new();
    let mut decorators = 0;
    for function in functions {
        for decorator in &function.decorator_list {
            let Some(found) = registration(decorator, function.name.as_str())
                .filter(|found| found.group == group)
            else {
                continue;
            };
            decorators += 1;
            match found.name {
                Some(name) if !names.contains(&name) => names.push(name),
                _ => return false,
            }
        }
    }
    spelled_count(source, &format!("{group}.command"))
        + spelled_count(source, &format!("{group}.group"))
        == decorators
}
