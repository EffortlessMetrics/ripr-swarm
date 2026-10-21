//! The ordinary eager table call that supplies an immutable slice argument.

use super::{
    constant_table_column, eager_path, macro_binding_candidates, opaque_macro_operand,
    parse_clean_source_file,
};
use crate::analysis::classify::{
    includes_external_docs, invokes_opaque_macro, names_identifier_outside_fn_definition,
};
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasArgList, HasGenericArgs, HasName},
};

/// Conservative reference completeness, not dispatch resolution. Imports,
/// function pointers, recursion and opaque macro tokens are additional uses.
pub(crate) fn owner_reference_count(source: &str, owner: &str) -> Option<usize> {
    let parse = parse_clean_source_file(source)?;
    if includes_external_docs(source)
        || invokes_opaque_macro(source)
        || macro_binding_candidates(&parse.tree()).may_bind_any_trusted()
        || parse.tree().syntax().descendants().any(|node| {
            // This family does not resolve user-defined operators or aliases
            // of primitive type names. Keep that uncertainty explicit across
            // the indexed files rather than treating their methods as intrinsic.
            if ast::Struct::can_cast(node.kind())
                || ast::Enum::can_cast(node.kind())
                || ast::Union::can_cast(node.kind())
                || ast::Trait::can_cast(node.kind())
                || ast::Impl::can_cast(node.kind())
                || ast::TypeAlias::can_cast(node.kind())
                || ast::ExternCrate::can_cast(node.kind())
            {
                return true;
            }
            if let Some(import) = ast::Use::cast(node.clone()) {
                return import
                    .syntax()
                    .text()
                    .to_string()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    != "usesuper::*;";
            }
            if let Some(attribute) = ast::Attr::cast(node.clone()) {
                return attribute.meta().is_none_or(|meta| {
                    !matches!(meta.simple_name().as_deref(), Some("test"))
                        && attribute
                            .syntax()
                            .text()
                            .to_string()
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect::<String>()
                            != "#[cfg(test)]"
                });
            }
            ast::MacroCall::cast(node).is_some_and(|call| {
                call.path().is_none_or(|path| {
                    !matches!(
                        path.syntax().text().to_string().as_str(),
                        "assert" | "assert_eq" | "assert_ne"
                    )
                }) || call
                    .token_tree()
                    .is_none_or(|tree| opaque_macro_operand(&tree))
            })
        })
    {
        return None;
    }
    parse
        .tree()
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .try_fold(0usize, |count, token| {
            if token.kind() == SyntaxKind::IDENT {
                if token.text().trim_start_matches("r#") == owner {
                    count.checked_add(1)
                } else {
                    Some(count)
                }
            } else if (token.kind() == SyntaxKind::COMMENT
                && (token.text().starts_with("/**") || token.text().starts_with("/*!")))
                || (token.kind() == SyntaxKind::COMMENT
                    && (token.text().starts_with("///") || token.text().starts_with("//!"))
                    && (token.text().contains("```")
                        || token.text().contains("~~~")
                        || token.text()[3..].starts_with("    ")
                        || token.text()[3..].trim_start_matches(' ').starts_with('\t')))
                || names_identifier_outside_fn_definition(token.text(), owner)
            {
                None
            } else {
                Some(count)
            }
        })
}

/// The test function and its assertion macros, or `None` when suspension,
/// nested execution or a non-assertion macro could skip or hide a later call.
fn assertion_only_function(
    parse: &ra_ap_syntax::Parse<ast::SourceFile>,
) -> Option<(ast::Fn, Vec<ast::MacroCall>)> {
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    if function.async_token().is_some() {
        return None;
    }
    // Suspension and nested execution may skip this call or later rows.
    // Tables without assertions must refuse these independently of the
    // assertion admission authority.
    if function.syntax().descendants().any(|node| {
        ast::TryExpr::can_cast(node.kind())
            || ast::YieldExpr::can_cast(node.kind())
            || ast::AwaitExpr::can_cast(node.kind())
            || ast::ClosureExpr::can_cast(node.kind())
            || ast::LoopExpr::can_cast(node.kind())
            || ast::WhileExpr::can_cast(node.kind())
    }) {
        return None;
    }
    let macros: Vec<_> = function
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .collect();
    if macros.iter().any(|call| {
        call.path().is_none_or(|path| {
            !matches!(
                path.syntax().text().to_string().as_str(),
                "assert" | "assert_eq" | "assert_ne"
            )
        })
    }) {
        return None;
    }
    Some((function, macros))
}

fn is_owner_call(call: &ast::CallExpr, owner: &str) -> bool {
    matches!(call.expr(), Some(ast::Expr::PathExpr(path))
        if path.path().is_some_and(|path| path.qualifier().is_none()
            && path.segment().and_then(|segment| segment.name_ref()).is_some_and(|name| name.text() == owner)))
}

/// Straight-line direct calls: every use of the owner is an ordinary call inside
/// an assertion macro, and its slice argument is spelled as a literal `&[..]`.
/// Returns each call's argument text in source order and the assertion count.
pub(crate) fn direct_literal_call_column(
    fn_text: &str,
    owner: &str,
    parameter: usize,
) -> Option<(Vec<String>, usize)> {
    let parse = parse_clean_source_file(fn_text)?;
    let (function, macros) = assertion_only_function(&parse)?;
    // Macro arguments are token trees, so the owner's calls live in the parsed
    // operands. Every use must be one of those calls: an alias, qualified or
    // foreign call or local shadow cannot disappear from the count.
    let uses = function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text().trim_start_matches("r#") == owner)
        .count();
    // Other calls can terminate or change control flow without an explicit
    // return/try token; this reader does not resolve their effects.
    if function.syntax().descendants().any(|node| {
        ast::MethodCallExpr::can_cast(node.kind()) || ast::CallExpr::can_cast(node.kind())
    }) {
        return None;
    }
    let first_return = function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text() == "return")
        .map(|token| token.text_range().start())
        .min();
    let mut column = Vec::new();
    for assertion in &macros {
        eager_path(assertion.syntax().clone(), &function, false, first_return).ok()?;
        let tokens = assertion.token_tree()?.syntax().text().to_string();
        let inner = [('(', ')'), ('[', ']'), ('{', '}')]
            .iter()
            .find_map(|(open, close)| tokens.strip_prefix(*open)?.strip_suffix(*close))?;
        let operands = parse_clean_source_file(&format!("fn operands() {{ let _ = ({inner}); }}"))?;
        let initializer = operands
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::LetStmt::cast)?
            .initializer()?;
        // Only the compared operands run unconditionally; format arguments
        // after them are evaluated only when the assertion fails.
        let compared = match assertion
            .path()
            .map(|path| path.syntax().text().to_string())
        {
            Some(name) if name == "assert" => 1,
            _ => 2,
        };
        let operands: Vec<ast::Expr> = match &initializer {
            ast::Expr::TupleExpr(tuple) => tuple.fields().collect(),
            other => vec![other.clone()],
        };
        if operands.iter().skip(compared).any(|operand| {
            operand
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .any(|token| token.text().trim_start_matches("r#") == owner)
        }) {
            return None;
        }
        for node in operands
            .iter()
            .take(compared)
            .flat_map(|operand| operand.syntax().descendants())
        {
            if let Some(call) = ast::CallExpr::cast(node.clone()) {
                if !is_owner_call(&call, owner) {
                    return None;
                }
                let argument = call.arg_list()?.args().nth(parameter)?;
                column.push(argument.syntax().text().to_string());
            } else if let Some(path) = ast::PathExpr::cast(node.clone()) {
                // Only the callee names a path; a variable could hold a computed value.
                if !path.syntax().parent().is_some_and(|parent| {
                    ast::CallExpr::cast(parent).is_some_and(|call| is_owner_call(&call, owner))
                }) {
                    return None;
                }
            } else if ast::ClosureExpr::can_cast(node.kind())
                || ast::AwaitExpr::can_cast(node.kind())
                || ast::YieldExpr::can_cast(node.kind())
                || ast::LoopExpr::can_cast(node.kind())
                || ast::WhileExpr::can_cast(node.kind())
                || ast::ForExpr::can_cast(node.kind())
                || ast::RecordExpr::can_cast(node.kind())
                || ast::FieldExpr::can_cast(node.kind())
                || ast::IndexExpr::can_cast(node.kind())
                || ast::LetStmt::can_cast(node.kind())
                || ast::Item::can_cast(node.kind())
                || ast::MethodCallExpr::can_cast(node.kind())
                || ast::MacroCall::can_cast(node.kind())
                // A branch or short-circuit operand may never run, so a call
                // inside it cannot witness an input.
                || ast::IfExpr::can_cast(node.kind())
                || ast::MatchExpr::can_cast(node.kind())
                || ast::BlockExpr::can_cast(node.kind())
                || ast::BinExpr::cast(node.clone())
                    .is_some_and(|bin| matches!(bin.op_kind(), Some(ast::BinaryOp::LogicOp(_))))
            {
                return None;
            }
        }
    }
    (!column.is_empty() && column.len() == uses && column.len() <= 64)
        .then_some((column, macros.len()))
}

pub(crate) fn constant_table_call_column(
    fn_text: &str,
    owner: &str,
    parameter: usize,
) -> Option<(Vec<String>, String, usize)> {
    let parse = parse_clean_source_file(fn_text)?;
    let (function, macros) = assertion_only_function(&parse)?;
    // One use must be the actual call. An alias, macro-hidden call,
    // qualified/foreign call or local shadow cannot disappear from it.
    if function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text().trim_start_matches("r#") == owner)
        .count()
        != 1
    {
        return None;
    }
    let mut calls = function
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
        .filter(|call| is_owner_call(call, owner));
    let call = calls.next()?;
    if calls.next().is_some() {
        return None;
    }
    // Calls can terminate successfully or change control flow without an
    // explicit return/try token. This reader does not resolve their effects.
    if function.syntax().descendants().any(|node| {
        (ast::CallExpr::can_cast(node.kind()) && &node != call.syntax())
            || ast::MethodCallExpr::can_cast(node.kind())
    }) {
        return None;
    }
    let first_return = function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text() == "return")
        .map(|token| token.text_range().start())
        .min();
    eager_path(call.syntax().clone(), &function, false, first_return).ok()?;
    let argument = call.arg_list()?.args().nth(parameter)?;
    let ast::Expr::PathExpr(path) = argument else {
        return None;
    };
    let name = path
        .path()
        .filter(|path| path.qualifier().is_none())?
        .segment()?
        .name_ref()?
        .text()
        .to_string();
    let column = constant_table_column(fn_text, &name)?;
    if column.len() > 64 || column.iter().any(|row| row.len() > 4096) {
        return None;
    }
    let result = call
        .syntax()
        .parent()
        .and_then(ast::LetStmt::cast)
        .and_then(|binding| binding.pat())
        .and_then(|pattern| match pattern {
            ast::Pat::IdentPat(pattern)
                if pattern.mut_token().is_none()
                    && pattern.ref_token().is_none()
                    && pattern.at_token().is_none() =>
            {
                pattern.name()
            }
            _ => None,
        })
        .filter(|result| {
            function
                .syntax()
                .descendants()
                .filter_map(|node| ast::AnyHasName::cast(node).and_then(|item| item.name()))
                .filter(|name| name.text() == result.text())
                .count()
                == 1
        });
    for assertion in &macros {
        if !assertion_operands_are_bounded(assertion, &name, result.as_ref(), &column)? {
            return None;
        }
    }
    Some((column, call.syntax().text().to_string(), macros.len()))
}

fn assertion_operands_are_bounded(
    assertion: &ast::MacroCall,
    column: &str,
    result: Option<&ast::Name>,
    rows: &[String],
) -> Option<bool> {
    let tokens = assertion.token_tree()?.syntax().text().to_string();
    let inner = [('(', ')'), ('[', ']'), ('{', '}')]
        .iter()
        .find_map(|(open, close)| tokens.strip_prefix(*open)?.strip_suffix(*close))?;
    let parse = parse_clean_source_file(&format!("fn operands() {{ let _ = ({inner}); }}"))?;
    let operands = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?
        .initializer()?;
    Some(operands.syntax().descendants().all(|node| {
        if let Some(path) = ast::PathExpr::cast(node.clone()) {
            return path.path().is_some_and(|path| {
                path.qualifier().is_none()
                    && path.segment().is_some_and(|segment| {
                        segment.generic_arg_list().is_none()
                            && segment.name_ref().is_some_and(|name| {
                                name.text() == column
                                    || result.is_some_and(|result| name.text() == result.text())
                            })
                    })
            });
        }
        if let Some(method) = ast::MethodCallExpr::cast(node.clone()) {
            return slice_observation(&method, column, rows);
        }
        !ast::CallExpr::can_cast(node.kind())
            && !ast::ClosureExpr::can_cast(node.kind())
            && !ast::AwaitExpr::can_cast(node.kind())
            && !ast::YieldExpr::can_cast(node.kind())
            && !ast::LoopExpr::can_cast(node.kind())
            && !ast::WhileExpr::can_cast(node.kind())
            && !ast::ForExpr::can_cast(node.kind())
            && !ast::RecordExpr::can_cast(node.kind())
            && !ast::FieldExpr::can_cast(node.kind())
            && !ast::IndexExpr::can_cast(node.kind())
            && !ast::LetStmt::can_cast(node.kind())
            && !ast::Item::can_cast(node.kind())
    }))
}

// Only intrinsic slice observations needed by the retained property are
// admitted inside assertions. A matching method name on another receiver is
// not evidence that its implementation returns or preserves the iteration.
fn slice_observation(method: &ast::MethodCallExpr, column: &str, rows: &[String]) -> bool {
    if method.generic_arg_list().is_some() {
        return false;
    }
    let Some(name) = method.name_ref() else {
        return false;
    };
    let Some(arguments) = method.arg_list() else {
        return false;
    };
    let args: Vec<_> = arguments.args().collect();
    let Some(receiver) = method.receiver() else {
        return false;
    };
    if name.text() == "iter" {
        return args.is_empty()
            && matches!(receiver, ast::Expr::PathExpr(path)
                if path.path().is_some_and(|path| path.qualifier().is_none()
                    && path.segment().is_some_and(|segment| segment.generic_arg_list().is_none()
                        && segment.name_ref().is_some_and(|name| name.text() == column))));
    }
    let ast::Expr::MethodCallExpr(previous) = receiver else {
        return false;
    };
    let Some(previous_name) = previous.name_ref() else {
        return false;
    };
    let shape = match name.text() {
        "min" | "max" => args.is_empty() && previous_name.text() == "iter",
        "unwrap" => {
            args.is_empty()
                && matches!(previous_name.text(), "min" | "max")
                && rows.iter().all(|row| {
                    parse_clean_source_file(&format!("fn input() {{ let row = {row}; }}"))
                        .and_then(|parse| {
                            parse
                                .tree()
                                .syntax()
                                .descendants()
                                .find_map(ast::ArrayExpr::cast)
                        })
                        .is_some_and(|array| array.exprs().next().is_some())
                })
        }
        "unwrap_or" => {
            matches!(previous_name.text(), "min" | "max")
                && matches!(args.as_slice(), [ast::Expr::RefExpr(value)]
                if value.mut_token().is_none() && value.raw_token().is_none()
                    && matches!(value.expr(), Some(ast::Expr::Literal(_))))
        }
        _ => false,
    };
    shape && slice_observation(&previous, column, rows)
}
