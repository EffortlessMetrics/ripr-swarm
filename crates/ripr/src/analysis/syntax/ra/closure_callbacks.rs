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
//! that makes a call. The helper side asks which of its parameters it
//! invokes as a plain statement-level call. The facts producer credits the
//! helper's assertion only when the two meet, so a closure that is never run
//! (the helper does not call the parameter) earns nothing.

use super::*;
use ra_ap_syntax::ast::HasArgList;

/// A helper parameter the helper body invokes directly (`f()`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InvokedCallbackParam {
    pub(crate) position: usize,
    pub(crate) name: String,
    /// 1-based line of the first invocation, relative to the parsed text.
    pub(crate) first_call_line: usize,
}

/// Argument positions at which a direct call to `helper` in `test_text`
/// passes a closure literal whose body makes a call. A call under a closure,
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
            let makes_call = closure.body().is_some_and(|closure_body| {
                closure_body.syntax().descendants().any(|node| {
                    ast::CallExpr::can_cast(node.kind())
                        || ast::MethodCallExpr::can_cast(node.kind())
                })
            });
            if makes_call {
                positions.insert(position);
            }
        }
    }
    Some(positions)
}

/// Parameters of the free function in `helper_text` that its body invokes as
/// `param(..)` outside any closure, `async` block or nested `fn`. A
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
        let first_call = body
            .syntax()
            .descendants()
            .filter_map(ast::CallExpr::cast)
            .filter(|call| {
                !enclosed_by_nested_fn_like(call.syntax(), &body)
                    && single_segment_callee(call).as_deref() == Some(name.as_str())
            })
            .map(|call| line_index.line(call.syntax().text_range().start()))
            .min();
        if let Some(first_call_line) = first_call {
            invoked.push(InvokedCallbackParam {
                position,
                name,
                first_call_line,
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
