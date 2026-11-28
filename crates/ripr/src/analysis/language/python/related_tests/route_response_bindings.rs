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
    // The names bound once to a call of each receiver and verb, computed once
    // per lookup rather than once per route or assignment.
    let mut bound_once: HashMap<(String, String), Vec<String>> = HashMap::new();
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
                if !http_verb
                    || !(verbs.is_empty() || verbs.contains(&verb))
                    || !python_route_matches_call_path(route, &path)
                    || names.contains(&name)
                {
                    continue;
                }
                let key = (receiver.clone(), verb.clone());
                let bound = bound_once.entry(key).or_insert_with(|| {
                    super::super::route_client::single_call_bindings(
                        &test.body_text,
                        &format!("{receiver}.{verb}"),
                    )
                    .into_iter()
                    .map(|(bound, _)| bound)
                    .collect()
                });
                if bound.contains(&name) && name_only_read_through(&test.body_text, &name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

/// Whether every use of `name` in the test body other than its one
/// assignment reads an attribute of it (`name.json()`, `name.status_code`)
/// and none assigns one (#13869). Any other use (a loop or `with` target in
/// any spacing, `del`, an argument, a nested `def`, a second assignment, an
/// attribute write such as `name._content = ...`) refuses the read, so a
/// rebinding the bound-once count misses cannot pose as the route call.
fn name_only_read_through(body: &str, name: &str) -> bool {
    let chars: Vec<char> = body.chars().collect();
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let name_chars: Vec<char> = name.chars().collect();
    let mut assignments = 0usize;
    let mut at = 0;
    while at + name_chars.len() <= chars.len() {
        let end = at + name_chars.len();
        if chars[at..end] != name_chars[..]
            || (at > 0 && (word(chars[at - 1]) || chars[at - 1] == '.'))
            || chars.get(end).is_some_and(|&ch| word(ch))
        {
            at += 1;
            continue;
        }
        let mut next = end;
        while chars.get(next).is_some_and(|&ch| ch == ' ' || ch == '\t') {
            next += 1;
        }
        match chars.get(next) {
            Some('.') => {
                // An attribute read; an attribute write (`name.x = ...`) is
                // refused.
                let mut after = next + 1;
                while chars.get(after).is_some_and(|&ch| word(ch)) {
                    after += 1;
                }
                while chars.get(after).is_some_and(|&ch| ch == ' ' || ch == '\t') {
                    after += 1;
                }
                if chars.get(after) == Some(&'=') && chars.get(after + 1) != Some(&'=') {
                    return false;
                }
            }
            Some('=') if chars.get(next + 1) != Some(&'=') => assignments += 1,
            Some(':') => assignments += 1,
            _ => return false,
        }
        at = end;
    }
    assignments == 1
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
        || compact.contains("._content")
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
