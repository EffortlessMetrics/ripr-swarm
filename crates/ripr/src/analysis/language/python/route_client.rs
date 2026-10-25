//! Request-client facts for the `ApiClientRouteCall` relation (#10110): which
//! names in a test body are route clients, and which application a client was
//! built over. Line-oriented over *logical* lines (a statement continued inside
//! brackets is one line), so a wrapped `TestClient(\n    app,\n)` reads like
//! its one-line form. A name counts as a client alias only when exactly one
//! statement in the body binds it.

use super::related_tests::is_python_identifier_char;

/// Statements of `text`, each continued physical line joined, with `#` comments
/// removed.
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for physical in text.lines() {
        let code = strip_comment(physical);
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(code.trim());
        depth = bracket_depth(code, depth);
        if depth == 0 {
            lines.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// `line` without a trailing `#` comment (a `#` inside a string stays).
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (idx, ch) in line.char_indices() {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (None, '\'' | '"') => quote = Some(ch),
            (None, '#') => return line.get(..idx).unwrap_or(line),
            _ => {}
        }
    }
    line
}

/// Bracket depth after `code`, starting from `depth`, outside strings.
fn bracket_depth(code: &str, mut depth: usize) -> usize {
    let mut quote: Option<char> = None;
    for ch in code.chars() {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(ch),
            (None, '(' | '[' | '{') => depth += 1,
            (None, ')' | ']' | '}') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Byte offsets of the top-level `=` signs of an assignment statement: plain
/// and augmented, never `==`, `!=`, `<=`, `>=`, `:=` or a keyword argument.
fn assignment_equals(line: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    for (idx, &byte) in bytes.iter().enumerate() {
        match (quote, byte) {
            (Some(open), _) if byte == open => quote = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'(' | b'[' | b'{') => depth += 1,
            (None, b')' | b']' | b'}') => depth = depth.saturating_sub(1),
            (None, b'=') if depth == 0 => {
                let prev = idx.checked_sub(1).and_then(|at| bytes.get(at)).copied();
                let prev2 = idx.checked_sub(2).and_then(|at| bytes.get(at)).copied();
                let next = bytes.get(idx + 1).copied();
                let comparison = next == Some(b'=')
                    || matches!(prev, Some(b'=' | b'!' | b':'))
                    || (matches!(prev, Some(b'<' | b'>')) && prev2 != prev);
                if !comparison {
                    found.push(idx);
                }
            }
            _ => {}
        }
    }
    found
}

/// Identifier tokens of `region` that are bare names: not an attribute
/// (`x.name`), not inside a subscript (`d[name]`).
fn bare_names(region: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut token = String::new();
    let mut prev_dot = false;
    let flush = |token: &mut String, prev_dot: &mut bool, depth: usize, names: &mut Vec<String>| {
        if !token.is_empty() {
            if depth == 0 && !*prev_dot {
                names.push(std::mem::take(token));
            } else {
                token.clear();
            }
        }
    };
    for ch in region.chars() {
        if is_python_identifier_char(ch) {
            token.push(ch);
            continue;
        }
        flush(&mut token, &mut prev_dot, depth, &mut names);
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '.' => {
                prev_dot = true;
                continue;
            }
            c if c.is_whitespace() => continue,
            _ => {}
        }
        prev_dot = false;
    }
    flush(&mut token, &mut prev_dot, depth, &mut names);
    names
}

/// Whether `line` (one logical statement) binds `name`: assignment targets
/// (plain, augmented, annotated, chained, tuple), `for` targets, `as` names,
/// walrus, `def`/`class`/`import` names and function parameters.
fn line_binds(line: &str, name: &str) -> bool {
    let line = line.trim();
    let first = line.split_whitespace().next().unwrap_or_default();
    match first {
        "for" | "async" => {
            if let Some(targets) = line
                .split_once("for ")
                .and_then(|(_, rest)| rest.split_once(" in "))
                .map(|(targets, _)| targets)
                && bare_names(targets).iter().any(|bound| bound == name)
            {
                return true;
            }
        }
        "def" | "class" | "import" | "from" | "del" => {
            return bare_names(line).iter().any(|bound| bound == name);
        }
        _ => {}
    }
    if line.contains(" as ")
        && line
            .split(" as ")
            .skip(1)
            .any(|rest| bare_names(rest).first().is_some_and(|bound| bound == name))
    {
        return true;
    }
    if line.replace(' ', "").contains(&format!("{name}:=")) {
        return true;
    }
    let equals = assignment_equals(line);
    let mut start = 0;
    for &at in &equals {
        let segment = line.get(start..at).unwrap_or_default();
        // `name: Type = value` binds `name`; cut the annotation.
        let target = segment.split(':').next().unwrap_or(segment);
        if bare_names(target).iter().any(|bound| bound == name) {
            return true;
        }
        start = at + 1;
    }
    // A bare annotation (`name: Type`) also rebinds the name.
    equals.is_empty()
        && line
            .split_once(':')
            .is_some_and(|(target, _)| target.trim() == name)
}

/// How many statements of the body bind `name`.
fn binding_count(lines: &[String], name: &str) -> usize {
    lines.iter().filter(|line| line_binds(line, name)).count()
}

/// `(name, value)` of a one-target assignment or a single-item `with`.
fn simple_binding(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    let equals = assignment_equals(line);
    if let [at] = equals.as_slice() {
        let lhs = line.get(..*at)?;
        // An augmented assignment has an operator before the `=`.
        if lhs.ends_with(|ch: char| "+-*/%&|^@<>".contains(ch)) {
            return None;
        }
        let name = lhs.split(':').next().unwrap_or(lhs).trim();
        let value = line.get(at + 1..)?.trim();
        return is_plain_name(name).then_some((name, value));
    }
    let rest = line.strip_prefix("async ").unwrap_or(line);
    let rest = rest.strip_prefix("with ")?.strip_suffix(':')?;
    let (value, name) = rest.rsplit_once(" as ")?;
    let name = name.trim();
    is_plain_name(name).then_some((name, value.trim()))
}

fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(is_python_identifier_char)
        && !name.starts_with(|ch: char| ch.is_ascii_digit())
}

/// Comma-separated top-level arguments of a call's argument text.
fn split_arguments(arguments: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (idx, ch) in arguments.char_indices() {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(ch),
            (None, '(' | '[' | '{') => depth += 1,
            (None, ')' | ']' | '}') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                parts.push(arguments.get(start..idx).unwrap_or_default().trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let last = arguments.get(start..).unwrap_or_default().trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// `name` and `value` of a `name=value` keyword argument.
fn keyword_argument(argument: &str) -> Option<(&str, &str)> {
    let at = *assignment_equals(argument).first()?;
    let name = argument.get(..at)?.trim();
    is_plain_name(name).then(|| (name, argument.get(at + 1..).unwrap_or_default().trim()))
}

/// The argument text of a `TestClient(...)` / `AsyncClient(...)` construction
/// that is the whole of `value` (any dotted prefix). An `AsyncClient` is a
/// route client only over an application (`app=` or an `ASGITransport`);
/// `AsyncClient(base_url=...)` calls an external service.
fn client_construction(value: &str) -> Option<&str> {
    let value = value.trim();
    let open = value.find('(')?;
    let class = value.get(..open)?.trim().rsplit('.').next()?;
    if !matches!(class, "TestClient" | "AsyncClient") || !value.ends_with(')') {
        return None;
    }
    let arguments = value.get(open + 1..value.len() - 1)?;
    // The closing bracket must close the opening one: `TestClient(app).get(x)`
    // is a response, not a client.
    if bracket_depth(arguments, 0) != 0 || arguments.contains(')') && !balanced(arguments) {
        return None;
    }
    if class == "AsyncClient" && application_argument(arguments).is_none() {
        return None;
    }
    Some(arguments)
}

fn balanced(text: &str) -> bool {
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            _ => {}
        }
    }
    depth == 0
}

/// The application expression a client construction passes: the `app=`
/// keyword, an `ASGITransport(app=...)` / `ASGITransport(...)` transport, or a
/// first positional argument.
fn application_argument(arguments: &str) -> Option<&str> {
    let parts = split_arguments(arguments);
    if let Some((_, value)) = parts
        .iter()
        .filter_map(|part| keyword_argument(part))
        .find(|(name, _)| *name == "app")
    {
        return Some(value);
    }
    if let Some(transport) = parts.iter().find_map(|part| {
        let (_, value) = keyword_argument(part)?;
        let open = value.find('(')?;
        let callee = value.get(..open)?.trim();
        (callee.rsplit('.').next()? == "ASGITransport" && value.ends_with(')'))
            .then(|| value.get(open + 1..value.len() - 1))?
    }) {
        return application_argument(transport);
    }
    parts
        .first()
        .copied()
        .filter(|part| keyword_argument(part).is_none())
}

/// A request client's receiver names in a test body: the conventional
/// `client` plus each name bound exactly once, from that client, from another
/// receiver, or from a client construction.
pub(super) fn client_receivers(body_text: &str) -> Vec<String> {
    let lines = logical_lines(body_text);
    let mut receivers = vec!["client".to_string()];
    // Repeat so a long alias chain resolves whatever order its links appear in.
    for _ in 0..=lines.len() {
        let before = receivers.len();
        for (name, value) in lines.iter().filter_map(|line| simple_binding(line)) {
            if receivers.iter().any(|known| known == name) || binding_count(&lines, name) != 1 {
                continue;
            }
            if receivers.iter().any(|known| known == value) || client_construction(value).is_some()
            {
                receivers.push(name.to_string());
            }
        }
        if receivers.len() == before {
            break;
        }
    }
    receivers
}

/// The application name a receiver's client construction passes, following
/// aliases of the client back to the construction. `None` when the chain does
/// not end in a construction over a plain name (a fixture, a dotted or called
/// application, a name bound more than once).
pub(super) fn client_app_name(body_text: &str, receiver: &str) -> Option<String> {
    let lines = logical_lines(body_text);
    let mut target = receiver;
    for _ in 0..=lines.len() {
        if binding_count(&lines, target) != 1 {
            return None;
        }
        let value = lines
            .iter()
            .filter_map(|line| simple_binding(line))
            .find(|(name, _)| *name == target)
            .map(|(_, value)| value)?;
        if let Some(arguments) = client_construction(value) {
            let application = application_argument(arguments)?;
            return is_plain_name(application).then(|| application.to_string());
        }
        target = value;
    }
    None
}

#[cfg(test)]
mod tests;
