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
//! side asks which of its parameters it invokes as a plain statement-level
//! call. The facts producer credits the
//! helper's assertion through this path only when the two meet, so a closure
//! that this path cannot show to run earns nothing from it. That statement is
//! limited to this closure-callback path. The plain-helper path (a helper that
//! invokes no parameter, so it never runs a closure or only forwards it, the
//! tracing `with_default` shape of #9981) asks [`passes_call_making_closure`]
//! and lends nothing when the test hands it a call-making closure (#9997).

use super::*;
use crate::analysis::syntax::node_has_macro_escape;
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
/// The helper call itself must be a statement the test body reaches
/// unconditionally and in order. `None` when the text does not parse cleanly.
pub(crate) fn closure_argument_positions(test_text: &str, helper: &str) -> Option<BTreeSet<usize>> {
    let parse = parse_clean_source_file(test_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let body = function.body()?;
    let mut positions = BTreeSet::new();
    for (expression, _) in eager_statement_exprs(&body) {
        let ast::Expr::CallExpr(call) = expression else {
            continue;
        };
        if single_segment_callee(&call).as_deref() != Some(helper) {
            continue;
        }
        let Some(arguments) = call.arg_list() else {
            continue;
        };
        for (position, argument) in arguments.args().enumerate() {
            let ast::Expr::ClosureExpr(closure) = argument else {
                continue;
            };
            // An `async` closure only builds a future when invoked.
            if closure.async_token().is_some() {
                continue;
            }
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

/// Whether any call to `helper` in `test_text`, reached or not, passes a
/// closure literal whose body holds a call. The plain-helper path asks this of
/// a helper that never invokes a parameter directly (#9997): such a helper
/// either never runs the closure or only forwards it, so its assertions cannot
/// be shown to observe what the closure called. Deliberately broader than
/// [`closure_argument_positions`]: no eager-reach, `async` or nested-call
/// exemption applies, because this only withholds credit. Text that does not
/// parse cleanly answers `true`.
pub(crate) fn passes_call_making_closure(test_text: &str, helper: &str) -> bool {
    let Some(parse) = parse_clean_source_file(test_text) else {
        return true;
    };
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
        .filter(|call| single_segment_callee(call).as_deref() == Some(helper))
        .filter_map(|call| call.arg_list())
        .flat_map(|arguments| arguments.args())
        .any(|argument| {
            let ast::Expr::ClosureExpr(closure) = argument else {
                return false;
            };
            closure.body().is_some_and(|body| {
                body.syntax().descendants().any(|node| {
                    ast::CallExpr::can_cast(node.kind())
                        || ast::MethodCallExpr::can_cast(node.kind())
                })
            })
        })
}

/// End of the statement that eagerly invokes `name`, when the function body
/// reaches it unconditionally. The grammar is deliberately small: a direct
/// statement of the body block that is `name(..);`, a tail `name(..)`, or
/// `let PAT = name(..);` (no `else`), preceded only by statements free of
/// `return`, `?`, `break`, `continue` and loops. A call under `if`, `match`,
/// a loop, a closure, an `async` block, a nested `fn` or a macro argument is
/// not admitted, so it stays uncredited rather than guessed at.
fn eager_invocation_end(body: &ast::BlockExpr, name: &str) -> Option<TextSize> {
    eager_statement_exprs(body)
        .into_iter()
        .find_map(|(expression, end)| {
            matches!(&expression, ast::Expr::CallExpr(call)
            if single_segment_callee(call).as_deref() == Some(name))
            .then_some(end)
        })
}

/// The expression of each statement a block reaches unconditionally and in
/// order, with the byte offset where the statement ends: `expr;`, a tail
/// expression, and the initializer of a `let` without `else`. The walk stops
/// after a statement that may leave the function or never finish, or that
/// holds a macro this producer cannot see through, so nothing past it is
/// admitted as eager.
fn eager_statement_exprs(body: &ast::BlockExpr) -> Vec<(ast::Expr, TextSize)> {
    let Some(list) = body.stmt_list() else {
        return Vec::new();
    };
    let mut reached = Vec::new();
    for statement in list.statements() {
        let expression = match &statement {
            ast::Stmt::ExprStmt(expression) => expression.expr(),
            ast::Stmt::LetStmt(binding) if binding.let_else().is_none() => binding.initializer(),
            _ => None,
        };
        // The statement's own operands run before the call: an exit or an
        // opaque macro anywhere in it (`run({ return; x }, ..)`) means the
        // call may never run, so it is not yielded either.
        let barrier =
            may_exit_early(statement.syntax()) || node_has_macro_escape(statement.syntax());
        if let Some(expression) = expression
            && !barrier
        {
            reached.push((expression, statement.syntax().text_range().end()));
        }
        if barrier {
            return reached;
        }
    }
    if let Some(tail) = list.tail_expr()
        && !may_exit_early(tail.syntax())
        && !node_has_macro_escape(tail.syntax())
    {
        let end = tail.syntax().text_range().end();
        reached.push((tail, end));
    }
    reached
}

/// Whether the callback parameter's identity could change before it is
/// called. Only immutable bare-identifier parameters are admitted, so a `mut`
/// binding is refused outright (assignment forms such as `(f) = g`,
/// `(f,) = (g,)` or `mem::replace(&mut (f), g)` all need `mut`), and so is any
/// `&mut` borrow that mentions the name.
fn identity_may_change(function: &ast::Fn, name: &str, is_mut: bool) -> bool {
    is_mut
        || function.syntax().descendants().any(|node| {
            ast::RefExpr::cast(node).is_some_and(|borrow| {
                borrow.mut_token().is_some()
                    && borrow.syntax().descendants().any(|inner| {
                        ast::NameRef::cast(inner).is_some_and(|reference| reference.text() == name)
                    })
            })
        })
}

/// Start column (byte offset within its line) of each macro invocation that
/// begins on the 1-based `line` of `text`, with the invocation's own text.
/// Comments and strings are not macro invocations, so only real parser sites
/// appear. `None` when the text does not parse cleanly.
pub(crate) fn macro_call_columns(text: &str, line: usize) -> Option<Vec<(String, usize)>> {
    let parse = parse_clean_source_file(text)?;
    let line_index = LineIndex::new(text);
    let line_start = line_index.starts.get(line.checked_sub(1)?).copied()?;
    Some(
        parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
            .filter_map(|call| {
                let start = call.path()?.syntax().text_range().start();
                (line_index.line(start) == line).then(|| {
                    (
                        call.syntax().text().to_string(),
                        text_size_to_usize(start).saturating_sub(line_start),
                    )
                })
            })
            .collect(),
    )
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
        ) && {
            // A nested fn, closure or `async` block (the statement itself
            // included) owns its own exits.
            let mut deferred = false;
            for ancestor in descendant.ancestors().skip(1) {
                if ast::ClosureExpr::can_cast(ancestor.kind())
                    || ast::Fn::can_cast(ancestor.kind())
                    || ast::BlockExpr::cast(ancestor.clone())
                        .is_some_and(|block| block.async_token().is_some())
                {
                    deferred = true;
                    break;
                }
                if ancestor == *node {
                    break;
                }
            }
            !deferred
        }
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
        if identity_may_change(&function, &name, pattern.mut_token().is_some()) {
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
