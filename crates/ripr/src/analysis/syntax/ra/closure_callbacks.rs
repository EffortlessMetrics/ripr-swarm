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
//! helper's assertion only when the two meet on eager paths and the
//! assertion's unique parser range follows the invocation. The existing
//! plain-helper path is separate; its unused/forwarded callback limitation
//! remains tracked by #9997.

use super::*;
use crate::analysis::syntax::owner_pin::callback_node_is_eager;
use ra_ap_syntax::ast::HasArgList;

/// A helper parameter the helper body invokes directly (`f()`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InvokedCallbackParam {
    pub(crate) position: usize,
    pub(crate) name: String,
    /// Unique parser-backed observation sites after the invocation, keyed as
    /// the helper oracle producer keys them (relative line and exact text).
    pub(crate) following_assertions: BTreeSet<(usize, String)>,
}

/// Argument positions at which a direct call to `helper` in `test_text`
/// passes a synchronous closure literal whose body makes a call. The helper
/// call must be on the test's eager path, outside deferred function-like code.
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
            || !callback_node_is_eager(call.syntax(), &function)
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
            // Calling an async closure creates a future; it does not run
            // the owner call in its body. That needs a separately proved poll.
            if closure.async_token().is_some() {
                continue;
            }
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
/// parameter counts only as an immutable plain identifier binding that nothing
/// else in the function rebinds, so assignment, a shadowing `let` or a pattern
/// cannot redirect the call. The invocation shares ordinary eager/escape
/// admission. A method (`self`) or an unparsable text yields `None`.
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
        if pattern.at_token().is_some()
            || pattern.ref_token().is_some()
            || pattern.mut_token().is_some()
        {
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
                    && callback_node_is_eager(call.syntax(), &function)
            })
            .map(|call| call.syntax().text_range().end())
            .min();
        if let Some(first_call_end) = first_call {
            invoked.push(InvokedCallbackParam {
                position,
                name,
                following_assertions: following_assertion_sites(
                    &function,
                    helper_text,
                    &line_index,
                    first_call_end,
                ),
            });
        }
    }
    Some(invoked)
}

/// OracleFact stores line/text, so a matching spelling must identify exactly
/// one parsed node. Aliases from one range do not create false duplicates;
/// repeated identical assertions on one line cannot borrow each other's order.
fn following_assertion_sites(
    function: &ast::Fn,
    text: &str,
    lines: &LineIndex,
    invocation_end: TextSize,
) -> BTreeSet<(usize, String)> {
    let mut sites: BTreeMap<(usize, String), Vec<ra_ap_syntax::SyntaxNode>> = BTreeMap::new();
    for node in function.syntax().descendants().filter(|node| {
        ast::MacroCall::can_cast(node.kind())
            || ast::MethodCallExpr::can_cast(node.kind())
            || ast::CallExpr::can_cast(node.kind())
    }) {
        let range = node.text_range();
        let raw = slice_text(text, range.start(), range.end())
            .trim()
            .trim_end_matches(';')
            .to_string();
        // Macro and expect-test observations include the statement's `;`;
        // primitive method/free-call observations omit it. Match both real
        // producer spellings, but require their shared unique AST range.
        for spelling in [raw, slice_macro_call_text(text, range.start(), range.end())] {
            let matches = sites
                .entry((lines.line(range.start()), spelling))
                .or_default();
            if !matches
                .iter()
                .any(|existing| existing.text_range() == range)
            {
                matches.push(node.clone());
            }
        }
    }
    sites
        .into_iter()
        .filter_map(|(key, nodes)| match nodes.as_slice() {
            [node]
                if node.text_range().start() >= invocation_end
                    && callback_node_is_eager(node, function) =>
            {
                Some(key)
            }
            _ => None,
        })
        .collect()
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
