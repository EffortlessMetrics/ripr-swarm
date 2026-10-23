//! Closure-callback helper shape (#9981).
//!
//! ```text
//! fn run_test(expected: &str, f: impl FnOnce() -> String) {
//!     let got = f();
//!     assert_eq!(got, expected);
//! }
//! #[test] fn a() { run_test("hi bob", || greet("bob")); }
//! ```
//!
//! Two parser-backed questions, one per side. The test side asks which
//! argument positions of a direct call to the helper hold a closure literal
//! whose body makes a call that runs when the closure is invoked (a call under
//! a nested closure, `async` block or `fn` inside it does not). The helper
//! side asks which of its parameters it invokes as a plain statement-level call. The facts producer credits the
//! helper's assertion only when the two meet, so a closure that is never run
//! (the helper does not call the parameter) earns nothing.

use super::*;
use ra_ap_syntax::ast::HasArgList;

/// A helper parameter the helper body invokes directly (`f()`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InvokedCallbackParam {
    pub(crate) position: usize,
    pub(crate) name: String,
    /// 1-based line, relative to the parsed text, where the invoking
    /// statement ends, and the byte column of that end within the line. An
    /// assertion observes the callback only when it starts at or after this
    /// point, which also orders two statements that share a line.
    pub(crate) end_line: usize,
    pub(crate) end_column: usize,
}

/// Argument positions at which a direct call to `helper` in `test_text`
/// passes a closure literal whose body makes a call that runs when the
/// closure is invoked (not one under a nested closure, `async` block or `fn`).
/// A call under a closure,
/// `async` block or nested `fn` of the test may never run and is ignored.
/// `None` when the text does not parse cleanly.
pub(crate) fn closure_argument_positions(test_text: &str, helper: &str) -> Option<BTreeSet<usize>> {
    let parse = parse_clean_source_file(test_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let body = function.body()?;
    let mut positions = BTreeSet::new();
    for call in body.syntax().descendants().filter_map(ast::CallExpr::cast) {
        if enclosed_by_nested_fn_like(call.syntax(), &body)
            || single_segment_callee(&call).as_deref() != Some(helper)
        {
            continue;
        }
        let Some(arguments) = call.arg_list() else {
            continue;
        };
        for (position, argument) in arguments.args().enumerate() {
            let ast::Expr::ClosureExpr(closure) = argument else {
                continue;
            };
            // The call must run when the closure is invoked, so a call under
            // a nested closure, `async` block or `fn` inside it does not count.
            let makes_call = closure.body().is_some_and(|closure_body| {
                closure_body.syntax().descendants().any(|node| {
                    (ast::CallExpr::can_cast(node.kind())
                        || ast::MethodCallExpr::can_cast(node.kind()))
                        && !node
                            .ancestors()
                            .take_while(|ancestor| ancestor != closure.syntax())
                            .any(|ancestor| {
                                ast::ClosureExpr::can_cast(ancestor.kind())
                                    || ast::Fn::can_cast(ancestor.kind())
                                    || ast::BlockExpr::cast(ancestor)
                                        .is_some_and(|block| block.async_token().is_some())
                            })
                })
            });
            if makes_call {
                positions.insert(position);
            }
        }
    }
    Some(positions)
}

/// End of the statement that eagerly invokes `name`, when the function body
/// reaches it unconditionally. The grammar is deliberately small: a direct
/// statement of the body block that is `name(..);`, a tail `name(..)`, or
/// `let PAT = name(..);` (no `else`), preceded only by statements free of
/// `return`, `?`, `break`, `continue` and loops. A call under `if`, `match`,
/// a loop, a closure, an `async` block, a nested `fn` or a macro argument is
/// not admitted, so it stays uncredited rather than guessed at.
fn eager_invocation_end(body: &ast::BlockExpr, name: &str) -> Option<TextSize> {
    let list = body.stmt_list()?;
    let invokes = |expr: Option<ast::Expr>| {
        matches!(&expr, Some(ast::Expr::CallExpr(call))
            if single_segment_callee(call).as_deref() == Some(name))
    };
    for statement in list.statements() {
        let invoking = match &statement {
            ast::Stmt::ExprStmt(expression) => invokes(expression.expr()),
            ast::Stmt::LetStmt(binding) => {
                binding.let_else().is_none() && invokes(binding.initializer())
            }
            ast::Stmt::Item(_) => false,
        };
        if invoking {
            return Some(statement.syntax().text_range().end());
        }
        if may_exit_early(statement.syntax()) {
            return None;
        }
    }
    let tail = list.tail_expr()?;
    invokes(Some(tail.clone())).then(|| tail.syntax().text_range().end())
}

/// Whether the callback parameter's identity can change before it is called:
/// an assignment to it, or a `&mut` borrow of it. For a `mut` parameter an
/// identifier followed by `=` inside a macro argument also counts, since
/// macro arguments are not parsed as expressions.
fn reassigned_or_mutably_borrowed(function: &ast::Fn, name: &str, is_mut: bool) -> bool {
    let is_name = |expr: Option<ast::Expr>| match expr {
        Some(ast::Expr::PathExpr(path)) => path.path().is_some_and(|path| {
            path.qualifier().is_none()
                && path
                    .segment()
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|reference| reference.text() == name)
        }),
        _ => false,
    };
    for node in function.syntax().descendants() {
        if let Some(binary) = ast::BinExpr::cast(node.clone())
            && matches!(binary.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
            && is_name(binary.lhs())
        {
            return true;
        }
        if let Some(borrow) = ast::RefExpr::cast(node.clone())
            && borrow.mut_token().is_some()
            && is_name(borrow.expr())
        {
            return true;
        }
        if is_mut && ast::TokenTree::can_cast(node.kind()) {
            let tokens = node
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|token| !token.kind().is_trivia())
                .collect::<Vec<_>>();
            if tokens.windows(2).any(|pair| {
                pair[0].text() == name && pair[1].kind() == ra_ap_syntax::SyntaxKind::EQ
            }) {
                return true;
            }
        }
    }
    false
}

/// Whether running `node` may leave the function or never finish.
fn may_exit_early(node: &ra_ap_syntax::SyntaxNode) -> bool {
    node.descendants().any(|descendant| {
        matches!(
            descendant.kind(),
            ra_ap_syntax::SyntaxKind::RETURN_EXPR
                | ra_ap_syntax::SyntaxKind::TRY_EXPR
                | ra_ap_syntax::SyntaxKind::BREAK_EXPR
                | ra_ap_syntax::SyntaxKind::CONTINUE_EXPR
                | ra_ap_syntax::SyntaxKind::LOOP_EXPR
                | ra_ap_syntax::SyntaxKind::WHILE_EXPR
                | ra_ap_syntax::SyntaxKind::FOR_EXPR
        )
    })
}

/// Parameters of the free function in `helper_text` that its body eagerly
/// invokes (see [`eager_invocation_end`]) as `param(..)`. A
/// parameter counts only as a plain identifier binding that nothing else in
/// the function rebinds, so a shadowing `let` or pattern cannot redirect the
/// call. A method (`self`) or an unparsable text yields `None`.
pub(crate) fn invoked_callback_params(helper_text: &str) -> Option<Vec<InvokedCallbackParam>> {
    let parse = parse_clean_source_file(helper_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let parameters = function.param_list()?;
    if parameters.self_param().is_some() {
        return None;
    }
    let body = function.body()?;
    let line_index = LineIndex::new(helper_text);
    let mut invoked = Vec::new();
    for (position, parameter) in parameters.params().enumerate() {
        let Some(ast::Pat::IdentPat(pattern)) = parameter.pat() else {
            continue;
        };
        if pattern.at_token().is_some() || pattern.ref_token().is_some() {
            continue;
        }
        let Some(name) = pattern.name().map(|name| name.text().to_string()) else {
            continue;
        };
        let bindings = function
            .syntax()
            .descendants()
            .filter(|node| node != function.syntax())
            .filter_map(|node| ast::AnyHasName::cast(node).and_then(|named| named.name()))
            .filter(|bound| bound.text() == name)
            .count();
        if bindings != 1 {
            continue;
        }
        if reassigned_or_mutably_borrowed(&function, &name, pattern.mut_token().is_some()) {
            continue;
        }
        if let Some(end) = eager_invocation_end(&body, &name) {
            let end_line = line_index.line_for_range_end(end);
            let line_start = line_index.starts.get(end_line - 1).copied().unwrap_or(0);
            invoked.push(InvokedCallbackParam {
                position,
                name,
                end_line,
                end_column: text_size_to_usize(end).saturating_sub(line_start),
            });
        }
    }
    Some(invoked)
}

fn single_segment_callee(call: &ast::CallExpr) -> Option<String> {
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    Some(path.segment()?.name_ref()?.text().to_string())
}

#[cfg(test)]
mod tests;
