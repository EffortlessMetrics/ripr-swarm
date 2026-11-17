//! Route handler response-body detection for the route-response credit
//! (#13869).

use super::*;

/// Whether a route handler's response body is the dict it returns (#13869):
/// every decorator is a static route decorator call whose keywords cannot
/// reshape the body (any other decorator may wrap and rewrite it), and the
/// function has one `return`, of a dict literal, and no `yield` or `raise`
/// (a raised error's body is not the returned dict). Nested
/// functions and classes are not searched. The caller also requires
/// [`body_returning_annotation`].
pub(super) fn route_returns_body(decorators: &[Expr], body: &[Stmt]) -> bool {
    const BODY_NEUTRAL_KEYWORDS: [&str; 7] = [
        "methods",
        "status_code",
        "name",
        "tags",
        "summary",
        "description",
        "endpoint",
    ];
    let decorators_ok = decorators.iter().all(|decorator| {
        let Some(name) = expr_full_name(decorator) else {
            return false;
        };
        if !is_static_route_decorator(&name) {
            return false;
        }
        let Expr::Call(call) = decorator else {
            return false;
        };
        call.keywords.iter().all(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_some_and(|arg| BODY_NEUTRAL_KEYWORDS.contains(&arg.as_str()))
        })
    });
    if !decorators_ok {
        return false;
    }
    let mut returns = Vec::new();
    let mut escapes = false;
    collect_returns(body, &mut returns, &mut escapes);
    !escapes && matches!(returns.as_slice(), [Some(Expr::Dict(dict))] if plain_distinct_keys(dict))
}

/// Whether every entry of a dict display has its own string-literal key, none
/// repeated: a `**` unpack or a repeated key can override the changed entry,
/// so the returned body's value for it is not the edited one (#13869).
fn plain_distinct_keys(dict: &ast::ExprDict) -> bool {
    let mut seen: Vec<&str> = Vec::new();
    dict.keys.iter().all(|key| match key {
        Some(Expr::Constant(constant)) => match &constant.value {
            ast::Constant::Str(key) if !seen.contains(&key.as_str()) => {
                seen.push(key.as_str());
                true
            }
            _ => false,
        },
        _ => false,
    })
}

/// The value of each `return` in `body` (not inside a nested function or
/// class) and whether any statement yields or raises.
fn collect_returns<'a>(body: &'a [Stmt], returns: &mut Vec<Option<&'a Expr>>, escapes: &mut bool) {
    for statement in body {
        match statement {
            Stmt::Return(ret) => returns.push(ret.value.as_deref()),
            Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_) => {}
            Stmt::If(node) => {
                collect_returns(&node.body, returns, escapes);
                collect_returns(&node.orelse, returns, escapes);
            }
            Stmt::For(node) => {
                collect_returns(&node.body, returns, escapes);
                collect_returns(&node.orelse, returns, escapes);
            }
            Stmt::AsyncFor(node) => {
                collect_returns(&node.body, returns, escapes);
                collect_returns(&node.orelse, returns, escapes);
            }
            Stmt::While(node) => {
                collect_returns(&node.body, returns, escapes);
                collect_returns(&node.orelse, returns, escapes);
            }
            Stmt::With(node) => collect_returns(&node.body, returns, escapes),
            Stmt::AsyncWith(node) => collect_returns(&node.body, returns, escapes),
            Stmt::Try(node) => {
                collect_returns(&node.body, returns, escapes);
                collect_returns(&node.orelse, returns, escapes);
                collect_returns(&node.finalbody, returns, escapes);
                for handler in &node.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect_returns(&handler.body, returns, escapes);
                }
            }
            Stmt::TryStar(node) => {
                collect_returns(&node.body, returns, escapes);
                collect_returns(&node.orelse, returns, escapes);
                collect_returns(&node.finalbody, returns, escapes);
                for handler in &node.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect_returns(&handler.body, returns, escapes);
                }
            }
            Stmt::Match(node) => {
                for case in &node.cases {
                    collect_returns(&case.body, returns, escapes);
                }
            }
            Stmt::Expr(expr) => {
                if matches!(expr.value.as_ref(), Expr::Yield(_) | Expr::YieldFrom(_)) {
                    *escapes = true;
                }
            }
            Stmt::Raise(_) => *escapes = true,
            _ => {}
        }
    }
}

/// Whether a handler's return annotation leaves its returned dict as the
/// body: none, or a bare `dict`. FastAPI reads any other annotation as an
/// implicit response model that filters and coerces the dict (#13869).
pub(super) fn body_returning_annotation(returns: Option<&Expr>) -> bool {
    returns.is_none_or(|annotation| bare_dict_annotation(Some(annotation)))
}

/// Whether a return annotation is the bare name `dict`, whose credit the
/// workspace post-pass withdraws when any source may rebind `dict`.
pub(super) fn bare_dict_annotation(returns: Option<&Expr>) -> bool {
    matches!(returns, Some(Expr::Name(name)) if name.id.as_str() == "dict")
}
