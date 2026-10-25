//! RIPR-SPEC-0360 loop accumulators (#6485): `for w in items { sum += w; }`
//! over a zero-initialized local that the owner returns as its tail.
//!
//! One reader serves the probe (the update is a `return_value` change) and
//! the owner pin (the update reaches the return and the pinned call must walk
//! the iterated parameter). Everything outside the one shape returns `None`
//! and keeps the update's existing `static_unknown`.

use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasArgList, HasName},
};

/// The accumulator update an owner returns through its tail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoopAccumulator {
    /// The local the loop adds into and the owner returns.
    pub(crate) local: String,
    /// Zero-based position of the slice-like parameter the loop walks,
    /// counted over the owner's non-`self` parameters.
    pub(crate) walked_position: usize,
}

/// The accumulator `update` belongs to, or `None` when any fact is missing.
///
/// The owner is `fn f(.., items: &[T], ..) -> N { let mut acc = 0; for x in
/// items { acc += <term>; } acc }` with nothing else in it: the update is the
/// loop body's only statement, `acc` appears only in its `let`, the update
/// and the tail, and the body holds no other loop, exit, macro, closure or
/// `unsafe`. `<term>` is call-free and does not mention `acc`. `acc op= term`
/// and `acc = acc op term` with `op` in `+`, `-` both qualify. Starting from
/// zero, a pinned nonzero result separates `+`/`-`, `*`, `/`, `%`, `&` and
/// statement deletion, because each of those produces the opposite sign or
/// zero.
pub(crate) fn loop_accumulator(
    function: &ast::Fn,
    update: &ast::BinExpr,
) -> Option<LoopAccumulator> {
    if function.async_token().is_some() || function.unsafe_token().is_some() {
        return None;
    }
    let local = updated_local(update)?;
    let statement = ast::ExprStmt::cast(update.syntax().parent()?)?;
    let loop_block = ast::StmtList::cast(statement.syntax().parent()?)?;
    let for_expr = ast::ForExpr::cast(loop_block.syntax().parent()?.parent()?)?;
    // The update is the loop body's only statement.
    if loop_block.statements().count() != 1 || loop_block.tail_expr().is_some() {
        return None;
    }
    let body = function.body()?;
    let stmts = body.stmt_list()?;
    // The loop is a direct statement of the owner body.
    let for_statement = for_expr.syntax().parent()?;
    if for_statement.kind() != SyntaxKind::EXPR_STMT
        || for_statement.parent().as_ref() != Some(stmts.syntax())
    {
        return None;
    }
    let walked = walked_parameter(&for_expr)?;
    let position = function
        .param_list()?
        .params()
        .position(|param| parameter_is_slice_named(&param, &walked))?;
    // The owner returns the local as its whole tail.
    let tail = stmts.tail_expr()?;
    if path_ident(&tail).as_deref() != Some(local.as_str()) {
        return None;
    }
    if !declares_zero(&stmts, &local) {
        return None;
    }
    // The shape allows nothing else: no exit, second loop, macro, closure
    // or block that could skip or repeat the update.
    for node in function.syntax().descendants() {
        if matches!(
            node.kind(),
            SyntaxKind::RETURN_EXPR
                | SyntaxKind::BREAK_EXPR
                | SyntaxKind::CONTINUE_EXPR
                | SyntaxKind::TRY_EXPR
                | SyntaxKind::AWAIT_EXPR
                | SyntaxKind::MACRO_CALL
                | SyntaxKind::CLOSURE_EXPR
                | SyntaxKind::LOOP_EXPR
                | SyntaxKind::WHILE_EXPR
                | SyntaxKind::IF_EXPR
                | SyntaxKind::MATCH_EXPR
        ) {
            return None;
        }
        if node.kind() == SyntaxKind::FOR_EXPR && node != *for_expr.syntax() {
            return None;
        }
        if node.kind() == SyntaxKind::BLOCK_EXPR
            && node.parent().map(|parent| parent.kind()) != Some(SyntaxKind::FN)
            && node.parent().map(|parent| parent.kind()) != Some(SyntaxKind::FOR_EXPR)
        {
            return None;
        }
    }
    // `acc` is named only by its `let`, the update and the tail.
    let named = function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT && token.text() == local)
        .count();
    let expected = if update.op_token().is_some_and(|op| op.text() == "=") {
        4
    } else {
        3
    };
    if named != expected {
        return None;
    }
    Some(LoopAccumulator {
        local,
        walked_position: position,
    })
}

/// The accumulator update on `changed_line` of `body` (the owner's source,
/// the line counted from the `fn` line), when the owner has the
/// RIPR-SPEC-0360 shape.
pub(crate) fn loop_accumulator_at(body: &str, changed_line: usize) -> Option<LoopAccumulator> {
    let parse = super::parse_clean_source_file(body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    function
        .syntax()
        .descendants()
        .filter_map(ast::BinExpr::cast)
        .find(|bin_expr| {
            let start = usize::from(bin_expr.syntax().text_range().start());
            body.get(..start)
                .is_some_and(|before| before.matches('\n').count() == changed_line)
        })
        .and_then(|bin_expr| loop_accumulator(&function, &bin_expr))
}

/// The bare local an update adds to or subtracts from, when its right side is
/// a call-free term that does not mention that local.
fn updated_local(update: &ast::BinExpr) -> Option<String> {
    let local = path_ident(&update.lhs()?)?;
    let op = update.op_token()?;
    let term = match op.text() {
        "+=" | "-=" => update.rhs()?,
        "=" => {
            let inner = ast::BinExpr::cast(update.rhs()?.syntax().clone())?;
            if !matches!(inner.op_token()?.text(), "+" | "-")
                || path_ident(&inner.lhs()?).as_deref() != Some(local.as_str())
            {
                return None;
            }
            inner.rhs()?
        }
        _ => return None,
    };
    if term.syntax().text().contains_char('\n') || !is_plain_term(&term) {
        return None;
    }
    Some(local)
}

/// A term built only from names, literals, field and index reads, `*`/`&`
/// and arithmetic operators, so nothing in it can run code or exit.
fn is_plain_term(term: &ast::Expr) -> bool {
    term.syntax()
        .descendants()
        .filter_map(ast::Expr::cast)
        .all(|expr| match expr {
            ast::Expr::PathExpr(_)
            | ast::Expr::Literal(_)
            | ast::Expr::FieldExpr(_)
            | ast::Expr::IndexExpr(_)
            | ast::Expr::ParenExpr(_) => true,
            ast::Expr::PrefixExpr(prefix) => prefix
                .op_token()
                .is_some_and(|op| matches!(op.text(), "*" | "-")),
            ast::Expr::RefExpr(_) => true,
            ast::Expr::BinExpr(bin) => bin
                .op_token()
                .is_some_and(|op| matches!(op.text(), "+" | "-" | "*")),
            _ => false,
        })
}

fn path_ident(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(path) = expr else {
        return None;
    };
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    Some(path.segment()?.name_ref()?.text().to_string())
}

/// The parameter name `for _ in items` / `items.iter()` walks.
fn walked_parameter(for_expr: &ast::ForExpr) -> Option<String> {
    let iterable = for_expr.iterable()?;
    if let Some(name) = path_ident(&iterable) {
        return Some(name);
    }
    let ast::Expr::MethodCallExpr(call) = iterable else {
        return None;
    };
    if call.name_ref()?.text() != "iter" || call.arg_list()?.args().next().is_some() {
        return None;
    }
    path_ident(&call.receiver()?)
}

fn parameter_is_slice_named(param: &ast::Param, name: &str) -> bool {
    let Some(ast::Pat::IdentPat(pattern)) = param.pat() else {
        return false;
    };
    if pattern.name().is_none_or(|ident| ident.text() != name) {
        return false;
    }
    let Some(ty) = param.ty() else {
        return false;
    };
    let ty = ty.syntax().text().to_string();
    let ty: String = ty.chars().filter(|ch| !ch.is_whitespace()).collect();
    let ty = ty.trim_start_matches('&');
    ty.starts_with('[') || ty.starts_with("Vec<")
}

/// The body holds `let mut <local> = 0;` (an integer zero, optionally
/// suffixed) as a direct statement.
fn declares_zero(stmts: &ast::StmtList, local: &str) -> bool {
    stmts.statements().any(|stmt| {
        let ast::Stmt::LetStmt(let_stmt) = stmt else {
            return false;
        };
        let Some(ast::Pat::IdentPat(pattern)) = let_stmt.pat() else {
            return false;
        };
        if pattern.mut_token().is_none()
            || pattern.name().is_none_or(|name| name.text() != local)
            || let_stmt.ty().is_some()
        {
            return false;
        }
        let Some(ast::Expr::Literal(literal)) = let_stmt.initializer() else {
            return false;
        };
        let text: String = literal
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|ch| *ch != '_')
            .collect();
        text.strip_prefix('0').is_some_and(|suffix| {
            matches!(
                suffix,
                "" | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::syntax::parse_clean_source_file;

    /// The accumulator any update in `source`'s first `fn` belongs to.
    fn accumulator(source: &str) -> Option<LoopAccumulator> {
        let parse = parse_clean_source_file(source)?;
        let function = parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::Fn::cast)?;
        function
            .syntax()
            .descendants()
            .filter_map(ast::BinExpr::cast)
            .find_map(|bin_expr| loop_accumulator(&function, &bin_expr))
    }

    const SUM: &str = "fn total(items: &[u32]) -> u32 {\n    let mut sum = 0;\n    for w in items {\n        sum += w;\n    }\n    sum\n}\n";

    #[test]
    fn a_zero_initialized_returned_local_is_an_accumulator() {
        let found = accumulator(SUM);
        assert_eq!(
            found,
            Some(LoopAccumulator {
                local: "sum".to_string(),
                walked_position: 0
            })
        );
    }

    #[test]
    fn plain_and_subtracting_forms_and_iter_walks_qualify() {
        for body in [
            "sum = sum + w;",
            "sum = sum - w;",
            "sum -= w;",
            "sum += *w * 2;",
            "sum += w.weight;",
        ] {
            let source = SUM.replace("sum += w;", body);
            assert!(accumulator(&source).is_some(), "{body}");
        }
        let walked = SUM.replace("for w in items", "for w in items.iter()");
        assert!(accumulator(&walked).is_some());
        let second = "fn total(scale: u32, items: Vec<u32>) -> u32 {\n    let mut sum = 0u32;\n    for w in items {\n        sum += w;\n    }\n    sum\n}\n";
        assert_eq!(
            accumulator(second).map(|found| found.walked_position),
            Some(1)
        );
    }

    #[test]
    fn anything_beyond_the_one_shape_is_refused() {
        let refused = [
            // A nonzero start can make `*=` agree with `+=` on a pinned value.
            SUM.replace("let mut sum = 0;", "let mut sum = 10;"),
            SUM.replace("let mut sum = 0;", "let mut sum = 0.0;"),
            SUM.replace("let mut sum = 0;", "let mut sum: u32 = 0;"),
            // The returned value is not the accumulator.
            SUM.replace("\n    sum\n}", "\n    sum + 1\n}"),
            SUM.replace("\n    sum\n}", "\n    sum * 2\n}"),
            // A call in the term can run code or exit.
            SUM.replace("sum += w;", "sum += weigh(w);"),
            SUM.replace("sum += w;", "sum += w.get()?;"),
            // Operators other than `+` and `-` are not the shape.
            SUM.replace("sum += w;", "sum *= w;"),
            SUM.replace("sum += w;", "sum = w;"),
            SUM.replace("sum += w;", "sum = sum * w;"),
            SUM.replace("sum += w;", "sum = sum + w - 1;"),
            // The update must be the loop body's only statement, unconditional.
            SUM.replace("sum += w;", "if *w > 0 { sum += w; }"),
            SUM.replace("sum += w;", "sum += w; sum += 1;"),
            SUM.replace("sum += w;", "continue; sum += w;"),
            SUM.replace("sum += w;", "sum += w;\n        println!(\"{sum}\");"),
            // The accumulator is named nowhere else.
            SUM.replace("let mut sum = 0;", "let mut sum = 0;\n    let _copy = sum;"),
            // The loop walks a parameter that is not slice-like.
            SUM.replace("items: &[u32]", "items: u32"),
            SUM.replace("for w in items", "for w in 0..3"),
            // A second loop, an early exit or an async owner.
            SUM.replace("    sum\n}", "    for _ in items {}\n    sum\n}"),
            SUM.replace(
                "let mut sum = 0;",
                "if items.is_empty() { return 0; }\n    let mut sum = 0;",
            ),
            SUM.replace("fn total", "async fn total"),
            SUM.replace("fn total", "unsafe fn total"),
        ];
        for source in refused {
            assert!(accumulator(&source).is_none(), "{source}");
        }
    }

    #[test]
    fn the_accumulator_at_a_line_is_found_by_its_offset_from_the_fn_line() {
        assert!(loop_accumulator_at(SUM, 3).is_some());
        assert!(loop_accumulator_at(SUM, 2).is_none());
        assert!(loop_accumulator_at(SUM, 5).is_none());
    }
}
