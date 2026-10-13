//! The ordinary eager table call that supplies an immutable slice argument.

use super::{constant_table_column, eager_path, parse_clean_source_file};
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasArgList},
};

/// Conservative reference completeness, not dispatch resolution. Imports,
/// function pointers, recursion and opaque macro tokens are additional uses.
pub(crate) fn owner_reference_count(source: &str, owner: &str) -> Option<usize> {
    let parse = parse_clean_source_file(source)?;
    Some(
        parse
            .tree()
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| {
                token.kind() == SyntaxKind::IDENT && token.text().trim_start_matches("r#") == owner
            })
            .count(),
    )
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
