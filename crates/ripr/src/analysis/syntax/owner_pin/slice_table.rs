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
    ast::{self, HasArgList},
};

/// Conservative reference completeness, not dispatch resolution. Imports,
/// function pointers, recursion and opaque macro tokens are additional uses.
pub(crate) fn owner_reference_count(source: &str, owner: &str) -> Option<usize> {
    let parse = parse_clean_source_file(source)?;
    if includes_external_docs(source)
        || invokes_opaque_macro(source)
        || macro_binding_candidates(&parse.tree()).may_bind_any_trusted()
        || parse.tree().syntax().descendants().any(|node| {
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

pub(crate) fn constant_table_call_column(
    fn_text: &str,
    owner: &str,
    parameter: usize,
) -> Option<(Vec<String>, String, usize)> {
    let parse = parse_clean_source_file(fn_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    if function.async_token().is_some() {
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
    let mut calls = function.syntax().descendants().filter_map(ast::CallExpr::cast).filter(|call| {
        matches!(call.expr(), Some(ast::Expr::PathExpr(path))
            if path.path().is_some_and(|path| path.qualifier().is_none()
                && path.segment().and_then(|segment| segment.name_ref()).is_some_and(|name| name.text() == owner)))
    });
    let call = calls.next()?;
    if calls.next().is_some() {
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
    Some((column, call.syntax().text().to_string(), macros.len()))
}
