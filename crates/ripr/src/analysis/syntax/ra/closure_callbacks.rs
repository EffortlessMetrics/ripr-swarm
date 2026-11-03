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

/// Whether any call to `helper` in `test_text`, reached or not, passes an
/// argument that contains a closure literal whose body holds a call, or a bare
/// name the test bound earlier to one. The plain-helper path asks this of
/// a helper that never invokes a parameter directly (#9997): such a helper
/// either never runs the closure or only forwards it, so its assertions cannot
/// be shown to observe what the closure called. Deliberately broader than
/// [`closure_argument_positions`]: no eager-reach, `async` or nested-call
/// exemption applies, because this only withholds. Text that does not
/// parse cleanly answers `true`.
///
/// A named argument (`c`, `&c`, `(c)`) is withheld when a `let c = <call-making
/// closure value>` — a closure literal with a call, a `Box`/`Rc`/`Arc::new` of
/// one, or an alias chain to either (up to four hops, then fail-closed) — (the
/// literal bare, `&`-borrowed or parenthesized) precedes the helper call in a
/// block that encloses it. Rebinding, `mut` and shadowing do not exempt it: any
/// such earlier binding withholds, so a later `let c = 5` is a missed advisory
/// rather than a guess about which binding the name reaches. A binding in a
/// block that does not enclose the call is another name and does not count.
/// Function items passed by name are not recognized.
pub(crate) fn passes_call_making_closure(test_text: &str, helper: &str) -> bool {
    let Some(parse) = parse_clean_source_file(test_text) else {
        return true;
    };
    let root = parse.tree().syntax().clone();
    root.descendants()
        .filter_map(ast::CallExpr::cast)
        .filter(|call| single_segment_callee(call).as_deref() == Some(helper))
        .any(|call| {
            call.arg_list().is_some_and(|arguments| {
                arguments.args().any(|argument| {
                    holds_call_making_closure(argument.syntax())
                        || bare_name(argument)
                            .is_some_and(|name| bound_to_call_making_closure(&call, &name))
                })
            })
        })
}

/// Whether `node` holds a closure literal whose body has a call or method call.
/// Any closure inside it counts, so `&|| ..`, `(|| ..)` and `Box::new(|| ..)`
/// do; one inside a nested `fn` item within `node` does not.
fn holds_call_making_closure(node: &ra_ap_syntax::SyntaxNode) -> bool {
    node.descendants()
        .filter_map(ast::ClosureExpr::cast)
        .filter(|closure| {
            !closure
                .syntax()
                .ancestors()
                .take_while(|ancestor| ancestor != node)
                .any(|ancestor| ast::Fn::can_cast(ancestor.kind()))
        })
        .any(|closure| {
            closure.body().is_some_and(|body| {
                body.syntax().descendants().any(|inner| {
                    ast::CallExpr::can_cast(inner.kind())
                        || ast::MethodCallExpr::can_cast(inner.kind())
                })
            })
        })
}

/// The identifier of an argument that is exactly a single-segment name, through
/// parentheses and borrows (`c`, `(c)`, `&c`, `&mut c`).
fn bare_name(argument: ast::Expr) -> Option<String> {
    match argument {
        ast::Expr::ParenExpr(paren) => bare_name(paren.expr()?),
        ast::Expr::RefExpr(borrow) => bare_name(borrow.expr()?),
        ast::Expr::PathExpr(path) => {
            let path = path.path()?;
            if path.qualifier().is_some() {
                return None;
            }
            Some(path.segment()?.name_ref()?.text().to_string())
        }
        _ => None,
    }
}

/// Whether `call` sits after a `let NAME = <call-making closure value>` in a
/// block that encloses it. The initializer is a closure literal, a smart-pointer
/// `new` of one, or a bare-name alias chain to either (depth cap four, then
/// fail-closed withhold).
fn bound_to_call_making_closure(call: &ast::CallExpr, name: &str) -> bool {
    bound_to_call_making_closure_before(call.syntax().text_range().start(), call.syntax(), name, 0)
}

const PLAIN_HELPER_NAMED_BINDING_DEPTH_CAP: u8 = 4;

fn bound_to_call_making_closure_before(
    before: ra_ap_syntax::TextSize,
    context: &ra_ap_syntax::SyntaxNode,
    name: &str,
    depth: u8,
) -> bool {
    if depth > PLAIN_HELPER_NAMED_BINDING_DEPTH_CAP {
        return true;
    }
    context
        .ancestors()
        .filter_map(ast::StmtList::cast)
        .flat_map(|list| list.statements())
        .filter_map(|statement| match statement {
            ast::Stmt::LetStmt(binding) => Some(binding),
            _ => None,
        })
        .filter(|binding| binding.syntax().text_range().end() <= before)
        .filter(|binding| {
            matches!(binding.pat(), Some(ast::Pat::IdentPat(pattern))
                if pattern.name().is_some_and(|bound| bound.text() == name))
        })
        .any(|binding| {
            binding.initializer().is_some_and(|initializer| {
                call_making_closure_initializer(initializer, binding, depth)
            })
        })
}

fn call_making_closure_initializer(
    initializer: ast::Expr,
    binding: ast::LetStmt,
    depth: u8,
) -> bool {
    if closure_literal(initializer.clone()).is_some_and(closure_makes_call) {
        return true;
    }
    if smart_pointer_new_with_call_making_closure(initializer.clone()) {
        return true;
    }
    let Some(alias) = bare_name(initializer) else {
        return false;
    };
    bound_to_call_making_closure_before(
        binding.syntax().text_range().start(),
        binding.syntax(),
        &alias,
        depth.saturating_add(1),
    )
}

fn smart_pointer_new_with_call_making_closure(expression: ast::Expr) -> bool {
    let expression = match expression {
        ast::Expr::ParenExpr(paren) => {
            let Some(inner) = paren.expr() else {
                return false;
            };
            inner
        }
        ast::Expr::RefExpr(borrow) => {
            let Some(inner) = borrow.expr() else {
                return false;
            };
            inner
        }
        other => other,
    };
    let ast::Expr::CallExpr(call) = expression else {
        return false;
    };
    let Some(callee) = call.expr() else {
        return false;
    };
    if !smart_pointer_new_callee(callee) {
        return false;
    };
    let Some(arguments) = call.arg_list() else {
        return false;
    };
    let mut args = arguments.args();
    let Some(first) = args.next() else {
        return false;
    };
    if args.next().is_some() {
        return false;
    }
    closure_literal(first).is_some_and(closure_makes_call)
}

fn smart_pointer_new_callee(expression: ast::Expr) -> bool {
    let ast::Expr::PathExpr(path_expr) = expression else {
        return false;
    };
    let Some(path) = path_expr.path() else {
        return false;
    };
    let Some(names) = path_segment_names(&path) else {
        return false;
    };
    match names.as_slice() {
        [a, b] if a == "Box" && b == "new" => true,
        [a, b] if a == "Rc" && b == "new" => true,
        [a, b] if a == "Arc" && b == "new" => true,
        [a, b, c, d] if a == "std" && b == "boxed" && c == "Box" && d == "new" => true,
        _ => false,
    }
}

fn path_segment_names(path: &ast::Path) -> Option<Vec<String>> {
    let mut names = Vec::new();
    if let Some(qualifier) = path.qualifier() {
        names.extend(path_segment_names(&qualifier)?);
    }
    let segment = path.segment()?;
    if segment.generic_arg_list().is_some() {
        return None;
    }
    names.push(segment.name_ref()?.text().to_string());
    Some(names)
}

fn closure_literal(expression: ast::Expr) -> Option<ast::ClosureExpr> {
    match expression {
        ast::Expr::ParenExpr(paren) => closure_literal(paren.expr()?),
        ast::Expr::RefExpr(borrow) => closure_literal(borrow.expr()?),
        ast::Expr::ClosureExpr(closure) => Some(closure),
        _ => None,
    }
}

fn closure_makes_call(closure: ast::ClosureExpr) -> bool {
    holds_call_making_closure(closure.syntax())
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
        let Some(name) = plain_callback_param(&function, &parameter) else {
            continue;
        };
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

/// The name of a parameter that is a plain, immutable identifier binding
/// nothing else in the function rebinds, so a call spelled with that name can
/// only be a call of the argument the caller passed at that position.
fn plain_callback_param(function: &ast::Fn, parameter: &ast::Param) -> Option<String> {
    let Some(ast::Pat::IdentPat(pattern)) = parameter.pat() else {
        return None;
    };
    if pattern.at_token().is_some() || pattern.ref_token().is_some() {
        return None;
    }
    let name = pattern.name().map(|name| name.text().to_string())?;
    let bindings = function
        .syntax()
        .descendants()
        .filter(|node| node != function.syntax())
        .filter_map(|node| ast::AnyHasName::cast(node).and_then(|named| named.name()))
        .filter(|bound| bound.text() == name)
        .count();
    if bindings != 1 || identity_may_change(function, &name, pattern.mut_token().is_some()) {
        return None;
    }
    Some(name)
}

/// Positions of the parameters of the free function in `helper_text` that its
/// body invokes as `param(..)` on a path every call of the function runs
/// (#11199). Unlike [`invoked_callback_params`] the call may be nested in an
/// argument, an operand or a standard assertion macro
/// (`assert_eq!(f(x), want)`), because the question is only whether the
/// argument is run, not what an assertion observes afterwards. A call under
/// `if`/`match`/a loop, the right side of `&&`/`||`, a closure, an `async`
/// block or a nested `fn`, in an `async fn`, or after a statement that may
/// leave the function or hold an opaque macro, is not admitted. A parameter
/// counts only as a plain identifier binding nothing else rebinds. A method
/// (`self`) or an unparsable text yields `None`.
pub(crate) fn unconditionally_invoked_param_positions(
    helper_text: &str,
) -> Option<BTreeSet<usize>> {
    let parse = parse_clean_source_file(helper_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    // Calling an `async fn` builds a future; the body does not run.
    if function.async_token().is_some() {
        return None;
    }
    let parameters = function.param_list()?;
    if parameters.self_param().is_some() {
        return None;
    }
    let body = function.body()?;
    let eager = eager_statement_exprs(&body);
    let mut positions = BTreeSet::new();
    for (position, parameter) in parameters.params().enumerate() {
        let Some(name) = plain_callback_param(&function, &parameter) else {
            continue;
        };
        if eager
            .iter()
            .any(|(expression, _)| invokes_param_unconditionally(expression.syntax(), &name))
        {
            positions.insert(position);
        }
    }
    Some(positions)
}

/// Whether `root` (one eagerly reached statement expression) calls `name` on
/// a path that always evaluates the call: directly, or as an operand of a
/// standard assertion macro.
fn invokes_param_unconditionally(root: &ra_ap_syntax::SyntaxNode, name: &str) -> bool {
    root.descendants().any(|node| {
        if let Some(call) = ast::CallExpr::cast(node.clone()) {
            single_segment_callee(&call).as_deref() == Some(name)
                && evaluates_unconditionally_within(&node, root)
        } else if let Some(call) = ast::MacroCall::cast(node.clone()) {
            assertion_operands_invoke(&call, name) && evaluates_unconditionally_within(&node, root)
        } else {
            false
        }
    })
}

/// Whether every node from `node` up to `root` evaluates its operand
/// unconditionally and in the enclosing function's own frame: calls, call
/// arguments, borrows, parentheses, field and index reads, tuples, casts,
/// unary operators and binary operators other than the right side of `&&` or
/// `||`. Anything else (a branch, loop, closure, block, `async`, nested item
/// or macro) may skip or defer the call.
fn evaluates_unconditionally_within(
    node: &ra_ap_syntax::SyntaxNode,
    root: &ra_ap_syntax::SyntaxNode,
) -> bool {
    use ra_ap_syntax::SyntaxKind as Kind;
    let mut child = node.clone();
    while child != *root {
        let Some(parent) = child.parent() else {
            return false;
        };
        let admitted = match parent.kind() {
            Kind::CALL_EXPR
            | Kind::ARG_LIST
            | Kind::METHOD_CALL_EXPR
            | Kind::REF_EXPR
            | Kind::PAREN_EXPR
            | Kind::FIELD_EXPR
            | Kind::INDEX_EXPR
            | Kind::TUPLE_EXPR
            | Kind::CAST_EXPR
            | Kind::PREFIX_EXPR
            | Kind::MACRO_EXPR => true,
            Kind::BIN_EXPR => ast::BinExpr::cast(parent.clone()).is_some_and(|binary| {
                let lazy = matches!(
                    binary.op_kind(),
                    Some(ast::BinaryOp::LogicOp(ast::LogicOp::And | ast::LogicOp::Or))
                );
                !lazy || binary.lhs().is_some_and(|lhs| *lhs.syntax() == child)
            }),
            _ => false,
        };
        if !admitted {
            return false;
        }
        child = parent;
    }
    true
}

/// Whether a standard assertion macro evaluates a call of `name` among the
/// operands it always runs (`assert!(a)`, `assert_eq!(a, b)`; the message
/// arguments are not read). The operands are parsed as the arguments of a
/// synthetic call so strings, nesting and commas follow the real grammar; a
/// tree that does not parse cleanly, or a macro this reader does not know,
/// answers `false`.
fn assertion_operands_invoke(call: &ast::MacroCall, name: &str) -> bool {
    let Some(path) = call.path() else {
        return false;
    };
    let operands = match path.syntax().text().to_string().trim() {
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => 2,
        "assert" | "debug_assert" => 1,
        _ => return false,
    };
    let Some(tree) = call.token_tree() else {
        return false;
    };
    let text = tree.syntax().text().to_string();
    let Some(inner) = text.get(1..text.len().saturating_sub(1)) else {
        return false;
    };
    let Some(parse) = parse_clean_source_file(&format!(
        "fn ripr_operands() {{ ripr_operands_call({inner}); }}"
    )) else {
        return false;
    };
    let Some(outer) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::CallExpr::cast)
    else {
        return false;
    };
    // The synthetic call must be the wrapper's own, so an operand list that
    // closed the wrapper early cannot smuggle a different call in.
    if single_segment_callee(&outer).as_deref() != Some("ripr_operands_call") {
        return false;
    }
    let Some(arguments) = outer.arg_list() else {
        return false;
    };
    arguments.args().take(operands).any(|operand| {
        operand.syntax().descendants().any(|node| {
            ast::CallExpr::cast(node.clone()).is_some_and(|inner| {
                single_segment_callee(&inner).as_deref() == Some(name)
                    && evaluates_unconditionally_within(&node, operand.syntax())
            })
        })
    })
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
