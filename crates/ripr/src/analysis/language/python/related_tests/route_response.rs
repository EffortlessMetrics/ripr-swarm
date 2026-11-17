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
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();
    MARKERS.iter().any(|marker| compact.contains(marker))
        // A `**` unpack in call arguments (`FastAPI(**{"openapi" "_url":
        // "/price"})`, `FastAPI(**options)`) passes options the source does
        // not spell, such as a schema or docs path that answers before a user
        // route, so the application's configuration is not established.
        || compact.contains("(**")
        || compact.contains(",**")
        || rebinds_builtin_dict(source)
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

/// Whether a source may rebind the name `dict` (#13869): FastAPI reads a
/// `-> dict` return annotation as a response model when the name is a model
/// class, which may coerce the returned values. The scan runs over logical
/// lines (a bracketed or continued statement joined) and fails closed: a
/// `dict` token anywhere in an assignment target (`dict = ...`, `dict, x =
/// ...`, `(dict, x) = ...`, `dict: T`, `dict := ...`, `dict += ...`), in a
/// `for` target, after `as`, `class`, `def`, `global` or `nonlocal`, in a
/// `case` pattern or on an `import` line is a rebinding, as is a reach into
/// the builtins (`builtins`, `__builtins__`) or the name in a string
/// (`globals()["dict"]`). An over-read only refuses.
fn rebinds_builtin_dict(source: &str) -> bool {
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let tokens_of = |text: &str| -> Vec<String> {
        text.split(|ch: char| !word(ch))
            .filter(|token| !token.is_empty())
            .map(str::to_string)
            .collect()
    };
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.contains("\"dict\"")
        || compact.contains("'dict'")
        || tokens_of(source)
            .iter()
            .any(|token| token == "builtins" || token == "__builtins__")
    {
        return true;
    }
    logical_lines(source).iter().any(|line| {
        let tokens = tokens_of(line);
        if !tokens.iter().any(|token| token == "dict") {
            return false;
        }
        let first = tokens.first().map(String::as_str);
        if matches!(
            first,
            Some("import" | "from" | "case" | "global" | "nonlocal")
        ) {
            return true;
        }
        // A `dict` after `as`, `class` or `def`, or between `for` and `in`.
        let mut in_for_target = false;
        for pair in tokens.windows(2) {
            match pair[0].as_str() {
                "for" => in_for_target = true,
                "in" => in_for_target = false,
                _ => {}
            }
            if pair[1] == "dict"
                && (in_for_target || matches!(pair[0].as_str(), "as" | "class" | "def" | "for"))
            {
                return true;
            }
        }
        assignment_targets(line)
            .iter()
            .any(|target| tokens_of(target).iter().any(|token| token == "dict"))
    })
}

/// The statements of a source with each `#` comment dropped and each
/// bracketed or backslash-continued statement joined onto one line.
fn logical_lines(source: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut depth = 0i64;
    for physical in source.lines() {
        let line = strip_line_comment(physical);
        let mut quote: Option<char> = None;
        let mut escaped = false;
        for ch in line.chars() {
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
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => depth -= 1,
                    _ => {}
                },
            }
        }
        let continued = line.trim_end().ends_with('\\');
        current.push_str(line.trim_end().trim_end_matches('\\'));
        current.push(' ');
        if depth <= 0 && !continued {
            lines.push(std::mem::take(&mut current));
            depth = 0;
        }
    }
    if !current.trim().is_empty() {
        lines.push(current);
    }
    lines
}

/// The assignment targets of a logical line: the text before each `=` that
/// sits outside brackets and strings (not `==`, `!=`, `<=`, `>=`), the text
/// before a top-level annotation `:` of an annotated assignment, and the
/// name before each `:=` at any depth. An augmented `+=` keeps its operator
/// in the target, which only over-reads.
fn assignment_targets(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut targets = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0i64;
    for (at, &ch) in chars.iter().enumerate() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        let prev = at
            .checked_sub(1)
            .and_then(|index| chars.get(index))
            .copied();
        let next = chars.get(at + 1).copied();
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if next == Some('=') || matches!(prev, Some('=' | '!' | '<' | '>')) => {}
            '=' if prev == Some(':') => {
                // A walrus binds the name just before it, at any depth.
                let before: String = chars[..at - 1].iter().collect();
                let name: String = before
                    .trim_end()
                    .chars()
                    .rev()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                    .collect();
                targets.push(name.chars().rev().collect());
            }
            '=' if depth == 0 => targets.push(chars[..at].iter().collect()),
            ':' if depth == 0 && next != Some('=') => {
                // An annotated assignment `name: T` binds the text before the
                // colon unless the line opens a compound statement.
                let before: String = chars[..at].iter().collect();
                let first = before.split_whitespace().next().unwrap_or_default();
                if !matches!(
                    first,
                    "def"
                        | "async"
                        | "class"
                        | "if"
                        | "elif"
                        | "else"
                        | "for"
                        | "while"
                        | "with"
                        | "try"
                        | "except"
                        | "finally"
                        | "match"
                        | "case"
                        | "lambda"
                ) && !before.contains("lambda")
                {
                    targets.push(before);
                }
            }
            _ => {}
        }
    }
    targets
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
