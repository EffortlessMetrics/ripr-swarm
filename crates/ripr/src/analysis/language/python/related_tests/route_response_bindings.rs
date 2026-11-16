//! Client response bindings for the route-response credit (#13869).

use super::*;

/// Local names that hold the response of a call to one of the owner's routes
/// through a client built over the owner's own application (#13869): the
/// name is bound once, to that call, in the test body. Only these names let a
/// response assertion stand for the handler's returned value; an
/// unidentified or rival application, an inline call or a rebound name
/// yields nothing.
pub(in super::super) fn identified_route_response_names(
    test: &PythonTest,
    owner: &PythonOwner,
) -> Vec<String> {
    if owner.route_paths.is_empty() {
        return Vec::new();
    }
    let receivers = super::super::route_client::client_receivers(&test.body_text);
    let mut names = Vec::new();
    for (index, route) in owner.route_paths.iter().enumerate() {
        let verbs = owner
            .route_methods
            .get(index)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for receiver in &receivers {
            if !body_calls_api_client_route(&test.body_text, receiver, route, verbs)
                || client_app_identity(
                    test,
                    owner,
                    receiver,
                    owner.route_receivers.get(index).map(String::as_str),
                ) != ClientApp::Own
            {
                continue;
            }
            for (name, verb, path) in route_response_assignments(test, receiver) {
                let http_verb = ["get", "post", "put", "patch", "delete", "options", "head"]
                    .contains(&verb.as_str());
                if http_verb
                    && (verbs.is_empty() || verbs.contains(&verb))
                    && python_route_matches_call_path(route, &path)
                    && super::super::route_client::single_call_bindings(
                        &test.body_text,
                        &format!("{receiver}.{verb}"),
                    )
                    .iter()
                    .any(|(bound, _)| *bound == name)
                    && !names.contains(&name)
                {
                    names.push(name);
                }
            }
        }
    }
    names
}

/// `(name, verb, path)` of each top-level statement of the test function
/// that assigns one plain name to `receiver.<verb>("<path>", ...)`, read from
/// the parsed body so text inside strings or comments cannot pose as the
/// binding (#13869). The caller still requires the name to be bound once.
fn route_response_assignments(test: &PythonTest, receiver: &str) -> Vec<(String, String, String)> {
    use rustpython_parser::ast::{Constant, Expr, Mod, Stmt};
    let Ok(Mod::Module(module)) =
        super::super::source_facts::parse_module_result(&test.file, &test.body_text)
    else {
        return Vec::new();
    };
    let body = match module.body.as_slice() {
        [Stmt::FunctionDef(function)] => &function.body,
        [Stmt::AsyncFunctionDef(function)] => &function.body,
        _ => return Vec::new(),
    };
    // A `match` capture or a walrus rebinds a name without an assignment
    // statement, which the bound-once count does not read, and an assignment
    // to `.json` (any `.json` not called at once, as in a tuple target, or any
    // `setattr`) replaces the method the read calls, so each refuses the read
    // (fail closed).
    let compact: String = test
        .body_text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if test.body_text.contains(":=")
        || compact
            .match_indices(".json")
            .any(|(at, _)| !compact[at + ".json".len()..].starts_with('('))
        || compact.contains("setattr")
        || compact.contains("\"json\"")
        || compact.contains("'json'")
        || test.body_text.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("match ") || line.starts_with("match(") || line.starts_with("case ")
        })
    {
        return Vec::new();
    }
    body.iter()
        .filter_map(|statement| {
            let (target, value) = match statement {
                Stmt::Assign(assign) => match assign.targets.as_slice() {
                    [target] => (target, assign.value.as_ref()),
                    _ => return None,
                },
                Stmt::AnnAssign(assign) => (assign.target.as_ref(), assign.value.as_deref()?),
                _ => return None,
            };
            let Expr::Name(name) = target else {
                return None;
            };
            let Expr::Call(call) = value else {
                return None;
            };
            let Expr::Attribute(method) = call.func.as_ref() else {
                return None;
            };
            if !matches!(method.value.as_ref(), Expr::Name(client) if client.id.as_str() == receiver)
            {
                return None;
            }
            let Some(Expr::Constant(first)) = call.args.first() else {
                return None;
            };
            let Constant::Str(path) = &first.value else {
                return None;
            };
            Some((
                name.id.to_string(),
                method.attr.to_string(),
                path.clone(),
            ))
        })
        .collect()
}
