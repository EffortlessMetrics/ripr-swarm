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
/// `unsafe`. `<term>` is the loop variable, `*variable` or an integer literal. `acc op= term`
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
    let (local, term) = updated_local(update)?;
    let statement = ast::ExprStmt::cast(update.syntax().parent()?)?;
    let loop_block = ast::StmtList::cast(statement.syntax().parent()?)?;
    let for_expr = ast::ForExpr::cast(loop_block.syntax().parent()?.parent()?)?;
    // The update is the loop body's only statement.
    if loop_block.statements().count() != 1 || loop_block.tail_expr().is_some() {
        return None;
    }
    let body = function.body()?;
    let stmts = body.stmt_list()?;
    // The body is exactly `let mut acc = 0; for .. { update } acc`, in that
    // order, so nothing can shadow the walked parameter or run before the
    // initialization.
    let statements: Vec<ast::Stmt> = stmts.statements().collect();
    let [
        ast::Stmt::LetStmt(initialization),
        ast::Stmt::ExprStmt(loop_statement),
    ] = statements.as_slice()
    else {
        return None;
    };
    if loop_statement.expr().as_ref().map(AstNode::syntax) != Some(for_expr.syntax())
        || !declares_zero(initialization, &local)
    {
        return None;
    }
    let loop_variable = loop_variable(&for_expr)?;
    if loop_variable == local || !is_plain_term(&term, &loop_variable) {
        return None;
    }
    let walked = walked_parameter(&for_expr)?;
    if walked == local || walked == loop_variable {
        return None;
    }
    let position = function
        .param_list()?
        .params()
        .position(|param| parameter_is_integer_slice_named(&param, &walked))?;
    // The owner returns the local as its whole tail.
    let tail = stmts.tail_expr()?;
    if path_ident(&tail).as_deref() != Some(local.as_str()) {
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
    // The walked parameter is named by its declaration and the loop only.
    let walked_names = function
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT && token.text() == walked)
        .count();
    if walked_names != 2 {
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
fn updated_local(update: &ast::BinExpr) -> Option<(String, ast::Expr)> {
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
    if term.syntax().text().contains_char('\n') {
        return None;
    }
    Some((local, term))
}

/// A term whose arithmetic is built in: the loop variable, `*variable` or an
/// integer literal. The elements are a built-in integer type, so `acc op=
/// term` is the primitive operator; a field, a second parameter or any other
/// name could carry a user `AddAssign`/`SubAssign` impl, and an operator
/// inside the term has mutants (`x * 2` to `x + 2`) a pin of the total does
/// not separate.
fn is_plain_term(term: &ast::Expr, loop_variable: &str) -> bool {
    match term {
        ast::Expr::PathExpr(_) => path_ident(term).as_deref() == Some(loop_variable),
        ast::Expr::Literal(literal) => matches!(literal.kind(), ast::LiteralKind::IntNumber(_)),
        ast::Expr::PrefixExpr(prefix) => {
            prefix.op_token().is_some_and(|op| op.text() == "*")
                && prefix
                    .expr()
                    .is_some_and(|inner| path_ident(&inner).as_deref() == Some(loop_variable))
        }
        _ => false,
    }
}

/// The name the loop binds to each element: `w` or `&w`.
fn loop_variable(for_expr: &ast::ForExpr) -> Option<String> {
    let pattern = for_expr.pat()?;
    let ident = match pattern {
        ast::Pat::IdentPat(ident) => ident,
        ast::Pat::RefPat(reference) => match reference.pat()? {
            ast::Pat::IdentPat(ident) => ident,
            _ => return None,
        },
        _ => return None,
    };
    if ident.at_token().is_some() || ident.ref_token().is_some() || ident.mut_token().is_some() {
        return None;
    }
    Some(ident.name()?.text().to_string())
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

fn parameter_is_integer_slice_named(param: &ast::Param, name: &str) -> bool {
    let Some(ast::Pat::IdentPat(pattern)) = param.pat() else {
        return false;
    };
    if pattern.name().is_none_or(|ident| ident.text() != name) {
        return false;
    }
    let Some(ty) = param.ty() else {
        return false;
    };
    let ty: String = ty
        .syntax()
        .text()
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    let ty = ty.trim_start_matches('&');
    let ty = ty.strip_prefix("mut").unwrap_or(ty);
    let element = ty
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .or_else(|| {
            ty.strip_prefix("Vec<")
                .and_then(|rest| rest.strip_suffix('>'))
        });
    element.is_some_and(|element| {
        matches!(
            element,
            "i8" | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
        )
    })
}

/// `let mut <local> = 0;` (an integer zero, optionally suffixed, with no type
/// annotation).
fn declares_zero(let_stmt: &ast::LetStmt, local: &str) -> bool {
    let Some(ast::Pat::IdentPat(pattern)) = let_stmt.pat() else {
        return false;
    };
    if pattern.mut_token().is_none()
        || pattern.name().is_none_or(|name| name.text() != local)
        || let_stmt.ty().is_some()
        || let_stmt.let_else().is_some()
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
            "sum += *w;",
            "sum += 1;",
        ] {
            let source = SUM.replace("sum += w;", body);
            assert!(accumulator(&source).is_some(), "{body}");
        }
        let walked = SUM.replace("for w in items", "for w in items.iter()");
        assert!(accumulator(&walked).is_some());
        let by_value = SUM.replace("for w in items", "for &w in items");
        assert!(accumulator(&by_value).is_some());
        let mutable = SUM.replace("&[u32]", "&mut [u32]");
        assert!(accumulator(&mutable).is_some());
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
            // An operator in the term has mutants a total pin does not separate.
            SUM.replace("sum += w;", "sum += *w * 2;"),
            SUM.replace("sum += w;", "sum += w + 1;"),
            SUM.replace("sum += w;", "sum += w[0];"),
            // A field or a second name could carry a user operator impl.
            SUM.replace("sum += w;", "sum += w.weight;"),
            SUM.replace("sum += w;", "sum += scale;")
                .replace("items:", "scale: u32, items:"),
            // Elements that are not a built-in integer type.
            SUM.replace("&[u32]", "&[Money]"),
            SUM.replace("&[u32]", "&[f64]"),
            // A shadowed or rebound walked parameter, or a body in another order.
            SUM.replace(
                "let mut sum = 0;",
                "let items = &[1u32][..];\n    let mut sum = 0;",
            ),
            SUM.replace(
                "let mut sum = 0;",
                "let mut sum = 0;\n    let items = &[1u32][..];",
            ),
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
