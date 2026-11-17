//! Route-response credit for #13869: route overlap, workspace reshaping
//! and registration scans, and client response bindings. A child of
//! `related_tests` so its private helpers stay shared.

use super::*;

/// Withdraw the route-response credit (#13869) from an owner whose route may
/// share a request with another route of the workspace: FastAPI and Flask
/// dispatch in registration order, which is not read, so the response may
/// come from the other handler. Two routes overlap when their verbs can meet
/// and their paths can match one request (each segment pair equal, or either
/// a parameter; a `:path` catch-all overlaps everything). The credit is
/// withdrawn from every owner when `opaque` (some file was not read, or a
/// source reshapes responses or registers routes the owners do not record,
/// see [`workspace_reshapes_responses`]) or when any owner has a dynamic
/// route decorator.
pub(in super::super) fn apply_route_overlaps(
    owners: &mut [PythonOwner],
    opaque: bool,
    declared_route_decorators: usize,
) {
    // Every route decorator line in the workspace must be a route the owners
    // record; one they do not (an unrecognized receiver, a conditional
    // module-level definition, a test file) is a rival that cannot be ruled
    // out.
    let recorded: usize = owners.iter().map(|owner| owner.route_paths.len()).sum();
    let opaque = opaque
        || recorded != declared_route_decorators
        || owners
            .iter()
            .any(|owner| !owner.dynamic_route_decorators.is_empty());
    let routes = |owner: &PythonOwner| -> Vec<(String, Vec<String>)> {
        owner
            .route_paths
            .iter()
            .enumerate()
            .map(|(route, path)| {
                (
                    path.clone(),
                    owner.route_methods.get(route).cloned().unwrap_or_default(),
                )
            })
            .collect()
    };
    // FastAPI installs its schema and docs routes while the application is
    // constructed, before any user route, so a user route on one of their
    // default paths never answers (custom paths are refused as opaque by
    // [`workspace_reshapes_responses`]).
    const FRAMEWORK_ROUTES: [&str; 4] =
        ["/openapi.json", "/docs", "/docs/oauth2-redirect", "/redoc"];
    let shared: Vec<bool> = owners
        .iter()
        .enumerate()
        .map(|(index, owner)| {
            owner.route_returns_body
                && !opaque
                && routes(owner).iter().any(|(path, verbs)| {
                    FRAMEWORK_ROUTES
                        .iter()
                        .any(|framework| route_patterns_overlap(path, framework))
                        || owners.iter().enumerate().any(|(other_index, other)| {
                            other_index != index
                                && routes(other).iter().any(|(other_path, other_verbs)| {
                                    verbs_meet(verbs, other_verbs)
                                        && route_patterns_overlap(path, other_path)
                                })
                        })
                })
        })
        .collect();
    for (owner, shared) in owners.iter_mut().zip(shared) {
        if opaque || shared {
            owner.route_returns_body = false;
        }
    }
}

/// Whether two routes' verb lists can accept one request (an empty list
/// accepts any verb).
fn verbs_meet(left: &[String], right: &[String]) -> bool {
    left.is_empty() || right.is_empty() || left.iter().any(|verb| right.contains(verb))
}

/// Whether two route patterns can match one request path (#13869).
fn route_patterns_overlap(left: &str, right: &str) -> bool {
    let catch_all = |path: &str| path.contains(":path") || path.contains("path:");
    if catch_all(left) || catch_all(right) {
        return true;
    }
    let left: Vec<&str> = left.trim_end_matches('/').split('/').collect();
    let right: Vec<&str> = right.trim_end_matches('/').split('/').collect();
    let parameter = |segment: &str| segment.contains(['{', '<']);
    left.len() == right.len()
        && left
            .iter()
            .zip(&right)
            .all(|(a, b)| a == b || parameter(a) || parameter(b))
}

/// Whether a workspace source can answer a request with a body other than the
/// handler's returned dict, or registers a route the owners do not record
/// (#13869): an exception handler, middleware or request hook (which may
/// answer with the changed key on an error path), a dependency override, a
/// custom route class, an imperative or declarative route registration, a
/// router or blueprint inclusion or prefix, a mount, a catch-all `:path`
/// parameter, a route object (`Route(...)`, `APIRoute` used as a value or
/// base, `routes += [...]`), a route verb named in a string (`getattr(app,
/// "get")`), or a patch of the `json` attribute or a response class
/// (`monkeypatch.setattr(Response, "json", ...)`, `"httpx.Response.json"`,
/// `Response.json = ...`, `response.__class__ = ...`), or a `**` unpack in
/// call arguments, which may pass application options (a custom schema or
/// docs path) under a name the source does not spell.
pub(in super::super) fn workspace_reshapes_responses(source: &str) -> bool {
    const MARKERS: [&str; 32] = [
        "openapi_url",
        "docs_url",
        "redoc_url",
        "swagger_ui_oauth2_redirect_url",
        "openapi_prefix",
        "exception_handler",
        "errorhandler(",
        "middleware",
        "Middleware(",
        "after_request",
        "before_request",
        "add_api_route(",
        "add_url_rule(",
        "add_route(",
        "add_api_websocket_route(",
        ".mount(",
        ":path}",
        "dependency_overrides",
        "include_router(",
        "register_blueprint(",
        "routes=",
        "route_class",
        "Route(",
        ".routes.",
        ".routes+=",
        ".routes[",
        ",\"json\"",
        ",'json'",
        ".json=",
        ".json\"",
        ".json'",
        "__class__",
    ];
    // Whitespace may sit inside a call (`app.add_api_route ("/x", f)`), so
    // the markers are matched against the source without it.
    // A `#` comment can sit between call arguments, so each marker is also
    // matched with comments removed (either reading only adds refusals).
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();
    let uncommented: String = source
        .lines()
        .flat_map(|line| strip_line_comment(line).chars())
        .filter(|ch| !ch.is_whitespace())
        .collect();
    let contains = |marker: &str| compact.contains(marker) || uncommented.contains(marker);
    MARKERS.iter().any(|marker| contains(marker))
        // A `**` unpack in call arguments (`FastAPI(**{"openapi" "_url":
        // "/price"})`, `FastAPI(**options)`) passes options the source does
        // not spell, such as a schema or docs path that answers before a user
        // route, so the application's configuration is not established.
        || contains("(**")
        || contains(",**")
        || compact
            .match_indices("APIRoute")
            .any(|(at, _)| !compact[at + "APIRoute".len()..].starts_with('r'))
        // A route verb named as a call's last string argument (`getattr(app,
        // "get")`, `attrgetter("post")`) reaches the route method dynamically.
        || ROUTE_VERBS.iter().any(|verb| {
            let name = verb.trim_start_matches('.').trim_end_matches('(');
            compact.contains(&format!("\"{name}\")")) || compact.contains(&format!("'{name}')"))
        })
}

/// Withdraws the response-body credit from every `-> dict` route handler
/// when any read source may rebind `dict` (#13869). Only those owners lose
/// it: the rebinding scan reads broadly (any f-string, any `patch` name), and
/// an unannotated handler's body does not depend on the name.
pub(in super::super) fn withdraw_dict_annotated_routes(
    owners: &mut [PythonOwner],
    dict_may_be_rebound: bool,
) {
    if dict_may_be_rebound {
        for owner in owners
            .iter_mut()
            .filter(|owner| owner.route_dict_annotation)
        {
            owner.route_returns_body = false;
        }
    }
}

/// Whether a source may rebind the name `dict` (#13869): FastAPI reads a
/// `-> dict` return annotation as a response model when the name is a model
/// class, which may coerce the returned values. The scan reads the Python
/// lexer's tokens (so strings, comments, brackets and one-line suites cannot
/// hide a statement) and fails closed. A `dict` name token is a read only as
/// a return annotation (after `->`), a bracketed annotation or value (after a
/// `:` inside brackets, outside a `case` pattern), or a call (before `(`, not
/// after `def` or `class`); any other `dict` token may bind it. A source that
/// does not lex, a star import, a non-ASCII name (Python folds names under
/// NFKC, so `ｄｉｃｔ` is `dict`), a string holding the word `dict` or a
/// writer name (`patch("shop.app.dict")`, code run by `timeit`), any f-string (the
/// lexer keeps its braces whole, so code inside them is unread), and any name
/// that can reach or write a namespace without spelling the bound name
/// (`globals`, `vars`, `__dict__`, `getattr`, `setattr`, `exec`,
/// `sys.modules`, frame and function globals, `gc` referents, `importlib`,
/// and the like) or that runs code or resolves a dotted target held in a
/// string (`patch`, `timeit`, `pdb`, `doctest`, `InteractiveConsole`,
/// `resolve_name`, and the like) all count as a rebinding. An over-read only refuses.
pub(in super::super) fn rebinds_builtin_dict(source: &str) -> bool {
    use rustpython_parser::{Mode, Tok, lexer::lex};
    // Names that reach or write a namespace without spelling the bound
    // name, or run code or resolve a dotted target held in a string.
    const NAMESPACE_WRITERS: [&str; 48] = [
        "builtins",
        "__builtins__",
        "globals",
        "locals",
        "vars",
        "__dict__",
        "modules",
        "setattr",
        "__setattr__",
        "delattr",
        "getattr",
        "__getattribute__",
        "attrgetter",
        "operator",
        "exec",
        "eval",
        "compile",
        "__import__",
        "importlib",
        "import_module",
        "f_globals",
        "f_locals",
        "f_back",
        "__globals__",
        "_getframe",
        "currentframe",
        "inspect",
        "gc",
        "get_referents",
        "get_referrers",
        "__subclasses__",
        "pickle",
        "marshal",
        "ctypes",
        "runpy",
        "InteractiveInterpreter",
        "runsource",
        "patch",
        "mock",
        "timeit",
        "Timer",
        "pdb",
        "cProfile",
        "profile",
        "doctest",
        "InteractiveConsole",
        "interact",
        "resolve_name",
    ];
    let mut tokens = Vec::new();
    for result in lex(source, Mode::Module) {
        match result {
            Ok((token, _)) => tokens.push(token),
            Err(_) => return true,
        }
    }
    let mut depth = 0i64;
    let mut in_case_pattern = false;
    let mut previous: Option<&Tok> = None;
    for (at, token) in tokens.iter().enumerate() {
        match token {
            Tok::Lpar | Tok::Lsqb | Tok::Lbrace => depth += 1,
            Tok::Rpar | Tok::Rsqb | Tok::Rbrace => depth -= 1,
            Tok::Case => in_case_pattern = true,
            Tok::Colon if depth <= 0 => in_case_pattern = false,
            Tok::Newline => in_case_pattern = false,
            Tok::Import if matches!(tokens.get(at + 1), Some(Tok::Star)) => return true,
            // The lexer keeps an f-string whole, so a binding or a call in its
            // braces (`f"{(dict := Model)}"`) never reaches the name checks. A
            // string can also hold code or a dotted target that a runner or a
            // patcher executes (`patch("shop.app.dict", ...)`), so a word
            // `dict` or a writer name anywhere in one refuses.
            Tok::String { value, kind, .. }
                if kind.is_any_fstring()
                    || value
                        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                        .any(|word| word == "dict" || NAMESPACE_WRITERS.contains(&word)) =>
            {
                return true;
            }
            Tok::Name { name } => {
                if !name.is_ascii() || NAMESPACE_WRITERS.contains(&name.as_str()) {
                    return true;
                }
                if name == "dict" {
                    let annotation = matches!(previous, Some(Tok::Rarrow))
                        || (depth > 0 && !in_case_pattern && matches!(previous, Some(Tok::Colon)));
                    let call = matches!(tokens.get(at + 1), Some(Tok::Lpar))
                        && !in_case_pattern
                        && !matches!(previous, Some(Tok::Def | Tok::Class));
                    if !(annotation || call) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        previous = Some(token);
    }
    false
}

/// A physical line without its `#` comment (a `#` inside a one-line string
/// is kept). A string spanning lines is read as code, which can only add
/// refusals.
fn strip_line_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (at, ch) in line.char_indices() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == open {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '#' => return line.get(..at).unwrap_or(line),
                _ => {}
            },
        }
    }
    line
}

/// The route-registering method calls the owners record (#13869).
const ROUTE_VERBS: [&str; 10] = [
    ".get(",
    ".post(",
    ".put(",
    ".patch(",
    ".delete(",
    ".options(",
    ".head(",
    ".route(",
    ".api_route(",
    ".websocket(",
];

/// The number of decorator lines in a source that may register a route
/// (`@app.get(...)`, `@bp.route(...)`, `@router.api_route(...)`,
/// `@app.websocket(...)`). It counts the verbs the owners record, so a route
/// one side misses cannot be balanced by a route the other side misses.
pub(in super::super) fn route_decorator_lines(source: &str) -> usize {
    source
        .lines()
        .filter(|line| {
            let line: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
            line.starts_with('@') && ROUTE_VERBS.iter().any(|verb| line.contains(verb))
        })
        .count()
}

/// Whether the workspace may register a route without a route decorator line
/// (#13869), which neither the decorator count nor the owners would see: a
/// route call applied directly (`app.get("/price")(legacy)`, on one line or
/// several), a route method used without being called (`partial(app.get,
/// "/price")`, `r = app.get`), or a route call bound to a name that is later
/// called or used as a decorator (`legacy = app.get("/price")` then
/// `legacy(view)` or `@legacy`). Any of them makes the run opaque (fail
/// closed); a dispatch such as `handlers.get(name)(args)` or `key=d.get`
/// refuses as well.
pub(in super::super) fn workspace_registers_routes_indirectly<'a>(
    sources: impl Iterator<Item = &'a str> + Clone,
) -> bool {
    let compact = |source: &str| -> Vec<char> {
        source
            .lines()
            .flat_map(|line| strip_line_comment(line).chars())
            .filter(|ch| !ch.is_whitespace())
            .collect()
    };
    let identifier = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mut aliases: std::collections::BTreeSet<Vec<char>> = std::collections::BTreeSet::new();
    for source in sources.clone() {
        for line in source.lines() {
            let line: String = strip_line_comment(line)
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            let Some((target, value)) = line.split_once('=') else {
                continue;
            };
            // An attribute target (`self.legacy = app.get(...)`) is
            // tracked by its last segment.
            let name = target.split(':').next().unwrap_or(target);
            let name = name.rsplit('.').next().unwrap_or(name);
            let Some((callee, _)) = value.split_once('(') else {
                continue;
            };
            let callee = format!("{callee}(");
            if !name.is_empty()
                && name.chars().all(identifier)
                && callee
                    .chars()
                    .all(|ch| identifier(ch) || matches!(ch, '.' | '('))
                && ROUTE_VERBS.iter().any(|verb| callee.ends_with(verb))
            {
                aliases.insert(name.chars().collect());
            }
        }
    }
    // An alias of an alias (`reg = legacy`) is an alias too, to a fixpoint.
    let mut grew = !aliases.is_empty();
    while grew {
        grew = false;
        for source in sources.clone() {
            for line in source.lines() {
                let line: String = strip_line_comment(line)
                    .chars()
                    .filter(|ch| !ch.is_whitespace())
                    .collect();
                let Some((target, value)) = line.split_once('=') else {
                    continue;
                };
                let name = target.split(':').next().unwrap_or(target);
                let name: Vec<char> = name.rsplit('.').next().unwrap_or(name).chars().collect();
                let value: Vec<char> = value.rsplit('.').next().unwrap_or(value).chars().collect();
                if !name.is_empty()
                    && name.iter().all(|&ch| identifier(ch))
                    && aliases.contains(&value)
                    && aliases.insert(name)
                {
                    grew = true;
                }
            }
        }
    }
    for source in sources {
        // Any alias token on a decorator line (`@self.legacy`, `@reg`) uses
        // the route call.
        if !aliases.is_empty()
            && source.lines().any(|line| {
                let line = strip_line_comment(line).trim_start();
                line.starts_with('@')
                    && line
                        .split(|ch: char| !identifier(ch))
                        .any(|token| aliases.contains(&token.chars().collect::<Vec<char>>()))
            })
        {
            return true;
        }
        let chars = compact(source);
        for verb in ROUTE_VERBS {
            let stem: Vec<char> = verb.trim_end_matches('(').chars().collect();
            let mut at = 0;
            while at + stem.len() <= chars.len() {
                if chars[at..at + stem.len()] != stem[..] {
                    at += 1;
                    continue;
                }
                let after = at + stem.len();
                match chars.get(after) {
                    Some('(') => {
                        // Find the call's closing parenthesis (strings
                        // skipped); a call right after it applies the route.
                        let mut depth = 0usize;
                        let mut quote: Option<char> = None;
                        let mut index = after;
                        while let Some(&ch) = chars.get(index) {
                            match quote {
                                Some(open) => {
                                    if ch == open {
                                        quote = None;
                                    }
                                }
                                None => match ch {
                                    '"' | '\'' => quote = Some(ch),
                                    '(' => depth += 1,
                                    ')' => {
                                        depth = depth.saturating_sub(1);
                                        if depth == 0 {
                                            break;
                                        }
                                    }
                                    _ => {}
                                },
                            }
                            index += 1;
                        }
                        // An unclosed call or quote is not established.
                        if index >= chars.len() || chars.get(index + 1) == Some(&'(') {
                            return true;
                        }
                        // A route call wrapped in parentheses or a line
                        // continuation on an assignment's right side
                        // (`register = (\n app.get("/x")\n)`) is an alias
                        // the per-line scan does not see.
                        let mut before = at;
                        while before > 0
                            && (identifier(chars[before - 1]) || chars[before - 1] == '.')
                        {
                            before -= 1;
                        }
                        let receiver_start = before;
                        while before > 0 && matches!(chars[before - 1], '(' | '\\') {
                            before -= 1;
                        }
                        if before < receiver_start
                            && before > 0
                            && chars[before - 1] == '='
                            && !(before > 1 && "=!<>".contains(chars[before - 2]))
                        {
                            return true;
                        }
                    }
                    Some(&next) if identifier(next) => {}
                    _ => return true,
                }
                at = after;
            }
        }
        // One pass over identifier tokens of the comment-stripped source with
        // its whitespace kept (compaction would merge `@alias` with the next
        // `def`): an alias token called or after `@` uses the route call.
        if !aliases.is_empty() {
            let spaced: Vec<char> = source
                .lines()
                .flat_map(|line| {
                    strip_line_comment(line)
                        .chars()
                        .chain(std::iter::once('\n'))
                })
                .collect();
            let neighbour = |mut index: usize, forward: bool| loop {
                let ch = *spaced.get(index)?;
                if !ch.is_whitespace() {
                    return Some(ch);
                }
                index = if forward {
                    index + 1
                } else {
                    index.checked_sub(1)?
                };
            };
            let mut at = 0;
            while at < spaced.len() {
                if !spaced.get(at).is_some_and(|&ch| identifier(ch)) {
                    at += 1;
                    continue;
                }
                let start = at;
                while spaced.get(at).is_some_and(|&ch| identifier(ch)) {
                    at += 1;
                }
                if spaced
                    .get(start..at)
                    .is_some_and(|token| aliases.contains(token))
                    && (neighbour(at, true) == Some('(')
                        || start
                            .checked_sub(1)
                            .and_then(|before| neighbour(before, false))
                            == Some('@'))
                {
                    return true;
                }
            }
        }
    }
    false
}
