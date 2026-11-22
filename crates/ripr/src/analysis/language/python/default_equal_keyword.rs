//! An added keyword argument that restates the callee's default (#16011).
//!
//! `current_time.shift(**changes)` to `current_time.shift(check_imaginary=True,
//! **changes)`, where every workspace `def shift` declares
//! `check_imaginary=True`, binds the same value as before for every call. The
//! only difference is a duplicate-keyword `TypeError` when `**changes` holds
//! that name, which a value assertion on an ordinary call does not see.

use super::PythonOwner;
use super::no_behavior::{call_segment_keyword_name, matching_call_paren, split_top_level_args};
use super::source_facts::parse_module_result;
use super::static_limits::is_simple_python_identifier;
use crate::domain::OwnerKind;
use rustpython_parser::ast::{Constant, Expr, Mod, Stmt, UnaryOp};
use std::path::Path;

/// The keyword argument the new line adds to one call, when removing it gives
/// back the old line and its literal value equals the declared default of
/// every workspace function or method with the callee's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DefaultEqualKeyword {
    pub(super) callee: String,
    pub(super) argument: String,
}

pub(super) fn default_equal_keyword(
    old_line: &str,
    new_line: &str,
    owners: &[PythonOwner],
) -> Option<DefaultEqualKeyword> {
    let old_compact = compact(old_line);
    if old_compact == compact(new_line) || old_line.contains('#') || new_line.contains('#') {
        return None;
    }
    for (open, _) in new_line.match_indices('(') {
        let Some(callee) = callee_name(&new_line[..open]) else {
            continue;
        };
        let Some(close) = matching_call_paren(new_line, open) else {
            continue;
        };
        let args = &new_line[open + 1..close];
        for segment in split_top_level_args(args) {
            let Some(name) = call_segment_keyword_name(segment.trim()) else {
                continue;
            };
            let Some(start) = (segment.as_ptr() as usize).checked_sub(args.as_ptr() as usize)
            else {
                continue;
            };
            let start = open + 1 + start;
            let end = start + segment.len();
            if !removal_gives(new_line, start, end, &old_compact) {
                continue;
            }
            let Some((_, value)) = segment.trim().split_once('=') else {
                continue;
            };
            let value = value.trim();
            if !is_plain_literal(value) || !every_definition_defaults(owners, callee, name, value) {
                continue;
            }
            return Some(DefaultEqualKeyword {
                callee: callee.to_string(),
                argument: compact(segment),
            });
        }
    }
    None
}

/// The last dotted segment of the name that ends right before a `(`, or None
/// when the bracket does not follow a name (a tuple or a call on a call).
fn callee_name(before: &str) -> Option<&str> {
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_alphanumeric() || *ch == '_')
        .last()
        .map(|(idx, _)| idx)?;
    let name = &before[start..];
    is_simple_python_identifier(name).then_some(name)
}

/// Whether deleting `new_line[start..end]` with one adjoining comma leaves the
/// old line, ignoring whitespace.
fn removal_gives(new_line: &str, start: usize, end: usize, old_compact: &str) -> bool {
    let before = &new_line[..start];
    let after = &new_line[end..];
    let trailing = after
        .strip_prefix(',')
        .map(|rest| format!("{before}{rest}"));
    let leading = before
        .trim_end()
        .strip_suffix(',')
        .map(|head| format!("{head}{after}"));
    [trailing, leading]
        .into_iter()
        .flatten()
        .any(|candidate| compact(&candidate) == old_compact)
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

/// A number, `True`, `False`, `None` or a plain (non f-) string, optionally
/// signed.
fn is_plain_literal(value: &str) -> bool {
    let snippet = format!("_ = {value}\n");
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<default-keyword>"), &snippet)
    else {
        return false;
    };
    let [Stmt::Assign(assign)] = module.body.as_slice() else {
        return false;
    };
    match assign.value.as_ref() {
        Expr::Constant(constant) => !matches!(constant.value, Constant::Ellipsis),
        Expr::UnaryOp(unary) => {
            matches!(unary.op, UnaryOp::USub | UnaryOp::UAdd)
                && matches!(
                    unary.operand.as_ref(),
                    Expr::Constant(constant) if matches!(
                        constant.value,
                        Constant::Int(_) | Constant::Float(_) | Constant::Complex { .. }
                    )
                )
        }
        _ => false,
    }
}

/// At least one workspace owner is named `callee`, and every one is a function
/// or method that declares `name` as a keyword-bindable parameter whose default
/// is `value`. A class of that name (its constructor) refuses.
fn every_definition_defaults(
    owners: &[PythonOwner],
    callee: &str,
    name: &str,
    value: &str,
) -> bool {
    let value = compact(value);
    let mut seen = false;
    owners
        .iter()
        .filter(|owner| owner.name == callee)
        .all(|owner| {
            seen = true;
            matches!(
                owner.owner_kind,
                Some(
                    OwnerKind::Function
                        | OwnerKind::Method
                        | OwnerKind::ClassMethod
                        | OwnerKind::ModuleFunction
                )
            ) && owner.parameters.iter().any(|parameter| {
                parameter.name == name
                    && !parameter.positional_only
                    && parameter
                        .default
                        .as_deref()
                        .is_some_and(|default| compact(default) == value)
            })
        })
        && seen
}
