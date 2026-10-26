//! Request-client facts for the `ApiClientRouteCall` relation (#10110): which
//! names in a test body are route clients, and which application a client was
//! built over. Line-oriented over *logical* lines (a statement continued inside
//! brackets is one line), so a wrapped `TestClient(\n    app,\n)` reads like
//! its one-line form. A name counts as a client alias only when exactly one
//! statement in the body binds it.

use super::related_tests::is_python_identifier_char;

/// `text` with string contents blanked (quotes kept, newlines kept inside
/// triple-quoted strings) and `#` comments removed, so brackets, `as`, `:=` and
/// `#` inside a string or docstring cannot be read as code.
fn blank_strings_and_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0;
    while idx < chars.len() {
        let ch = chars[idx];
        match ch {
            '#' => {
                while idx < chars.len() && chars[idx] != '\n' {
                    idx += 1;
                }
            }
            '\'' | '"' => {
                let triple = chars.get(idx + 1) == Some(&ch) && chars.get(idx + 2) == Some(&ch);
                let width = if triple { 3 } else { 1 };
                for _ in 0..width {
                    out.push(ch);
                }
                idx += width;
                while idx < chars.len() {
                    let inner = chars[idx];
                    if inner == '\\' {
                        out.push(' ');
                        idx += 1;
                        if idx < chars.len() {
                            out.push(if chars[idx] == '\n' { '\n' } else { ' ' });
                            idx += 1;
                        }
                        continue;
                    }
                    if inner == ch
                        && (!triple
                            || (chars.get(idx + 1) == Some(&ch) && chars.get(idx + 2) == Some(&ch)))
                    {
                        for _ in 0..width {
                            out.push(ch);
                        }
                        idx += width;
                        break;
                    }
                    if inner == '\n' && !triple {
                        // An unterminated single-quoted string ends at the line.
                        break;
                    }
                    out.push(if inner == '\n' { '\n' } else { ' ' });
                    idx += 1;
                }
            }
            _ => {
                out.push(ch);
                idx += 1;
            }
        }
    }
    out
}

/// Statements of `text`: continued physical lines (inside brackets or after a
/// trailing backslash) joined, `;`-separated statements split, string contents
/// and comments removed.
fn logical_lines(text: &str) -> Vec<String> {
    let cleaned = blank_strings_and_comments(text);
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for physical in cleaned.lines() {
        let code = physical.trim_end();
        let (code, continued) = code
            .strip_suffix('\\')
            .map_or((code, false), |rest| (rest, true));
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(code.trim());
        depth = bracket_depth(code, depth);
        if depth == 0 && !continued {
            push_statements(&mut statements, &std::mem::take(&mut current));
        }
    }
    push_statements(&mut statements, &current);
    statements
}

/// Split one logical line at top-level `;`.
fn push_statements(statements: &mut Vec<String>, line: &str) {
    let mut depth = 0usize;
    let mut start = 0;
    for (idx, ch) in line.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => {
                statements.push(line.get(start..idx).unwrap_or_default().trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let last = line.get(start..).unwrap_or_default().trim();
    if !last.is_empty() || start == 0 && !line.is_empty() {
        statements.push(last.to_string());
    }
}

/// Bracket depth after `code`, starting from `depth` (strings are already
/// blanked).
fn bracket_depth(code: &str, mut depth: usize) -> usize {
    for ch in code.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Byte offsets of the top-level `=` signs of an assignment statement: plain
/// and augmented, never `==`, `!=`, `<=`, `>=`, `:=` or a keyword argument.
fn assignment_equals(line: &str) -> Vec<usize> {
    // A `lambda` parameter default (`lambda api=1: api`) is not an assignment.
    let line = line
        .match_indices("lambda")
        .find(|(at, matched)| {
            let before_ok = line
                .get(..*at)
                .and_then(|before| before.chars().next_back())
                .is_none_or(|ch| !is_python_identifier_char(ch));
            let after_ok = line
                .get(*at + matched.len()..)
                .and_then(|rest| rest.chars().next())
                .is_none_or(|ch| !is_python_identifier_char(ch));
            before_ok && after_ok
        })
        .map_or(line, |(at, _)| line.get(..at).unwrap_or(line));
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
/// (`x.name`), not inside a subscript (`d[name]`) and not the root of an
/// attribute or subscript store (`x.name = v`, `x[0] = v`), which mutates the
/// object without rebinding `x`.
fn bare_names(region: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut token = String::new();
    let mut prev_dot = false;
    let chars: Vec<char> = region.chars().collect();
    for (at, &ch) in chars.iter().enumerate() {
        if is_python_identifier_char(ch) {
            token.push(ch);
            continue;
        }
        if !token.is_empty() {
            let accessed = matches!(
                chars[at..].iter().find(|next| !next.is_whitespace()),
                Some('.' | '[')
            );
            if depth == 0 && !prev_dot && !accessed {
                names.push(std::mem::take(&mut token));
            } else {
                token.clear();
            }
        }
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
    if !token.is_empty() && depth == 0 && !prev_dot {
        names.push(token);
    }
    names
}

/// Whether `line` (one logical statement) binds `name`: assignment targets
/// (plain, augmented, annotated, chained, tuple), `for` targets, `as` names,
/// walrus, `def`/`class`/`import` names and function parameters, including the
/// body of a one-line compound statement (`if flag: api = other()`).
fn line_binds(line: &str, name: &str) -> bool {
    let line = line.trim();
    let line = line.strip_prefix("async ").unwrap_or(line);
    let first = line.split_whitespace().next().unwrap_or_default();
    let first = first.trim_end_matches(':');
    match first {
        "def" => return def_binds(line, name),
        "class" | "import" | "from" | "del" => {
            return bare_names(line).iter().any(|bound| bound == name);
        }
        "for" => {
            if let Some((targets, _)) = line
                .strip_prefix("for ")
                .and_then(|rest| rest.split_once(" in "))
                && bare_names(targets).iter().any(|bound| bound == name)
            {
                return true;
            }
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
    if walrus_binds(line, name) {
        return true;
    }
    // A compound statement's one-line body binds like a statement of its own.
    if matches!(
        first,
        "if" | "elif" | "else" | "try" | "except" | "finally" | "while" | "with" | "for"
    ) && let Some(colon) = header_colon(line)
    {
        let body = line.get(colon + 1..).unwrap_or_default().trim();
        if !body.is_empty() && line_binds(body, name) {
            return true;
        }
        return false;
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

/// Byte offset of the colon that ends a compound statement's header.
fn header_colon(line: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in line.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ':' if depth == 0
                && line
                    .get(idx + 1..)
                    .is_none_or(|rest| !rest.starts_with('=')) =>
            {
                return Some(idx);
            }
            _ => {}
        }
    }
    None
}

/// Whether `name := value` appears with `name` as the whole target.
fn walrus_binds(line: &str, name: &str) -> bool {
    line.match_indices(":=").any(|(at, _)| {
        let before = line.get(..at).unwrap_or_default().trim_end();
        let tail: String = before
            .chars()
            .rev()
            .take_while(|ch| is_python_identifier_char(*ch))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        tail == name
    })
}

/// `def name(params)` binds the function name and each parameter name.
fn def_binds(line: &str, name: &str) -> bool {
    let rest = line.strip_prefix("def ").unwrap_or(line).trim_start();
    let Some(open) = rest.find('(') else {
        return rest.trim_end_matches(':').trim() == name;
    };
    if rest
        .get(..open)
        .is_some_and(|defined| defined.trim() == name)
    {
        return true;
    }
    let params = rest.get(open + 1..).unwrap_or_default();
    split_arguments(params).iter().any(|param| {
        param
            .trim_start_matches('*')
            .split(['=', ':', ')'])
            .next()
            .is_some_and(|bound| bound.trim() == name)
    })
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
    let bindings: Vec<(&str, &str)> = lines
        .iter()
        .filter_map(|line| simple_binding(line))
        .collect();
    let mut receivers = vec!["client".to_string()];
    // Repeat so a long alias chain resolves whatever order its links appear in.
    for _ in 0..=bindings.len() {
        let before = receivers.len();
        for &(name, value) in &bindings {
            if receivers.iter().any(|known| known == name) {
                continue;
            }
            let from_client = receivers.iter().any(|known| known == value)
                || client_construction(value).is_some();
            if from_client && binding_count(&lines, name) == 1 {
                receivers.push(name.to_string());
            }
        }
        if receivers.len() == before {
            break;
        }
    }
    receivers
}

/// Whether `name` is bound anywhere in the body other than as a parameter of
/// the first line's `def` (a local assignment, import, loop target, ...). An
/// application name bound this way is not the module-level import it spells.
pub(super) fn rebinds_name(body_text: &str, name: &str) -> bool {
    let lines = logical_lines(body_text);
    let signature = lines
        .first()
        .is_some_and(|line| line_binds(line, name) && line.trim_start().contains("def "));
    binding_count(&lines, name) > usize::from(signature)
}

/// Whether `name` is a parameter of the first line's `def` that nothing in
/// the body rebinds, so it is the value the caller (a fixture) supplies.
pub(super) fn is_unrebound_parameter(body_text: &str, name: &str) -> bool {
    let lines = logical_lines(body_text);
    lines
        .first()
        .is_some_and(|line| line_binds(line, name) && line.trim_start().contains("def "))
        && binding_count(&lines, name) == 1
}

/// The name an alias chain starts from: `c = client` leads to `client`; a name
/// with no single plain-name binding leads to itself.
pub(super) fn client_root(body_text: &str, receiver: &str) -> String {
    let lines = logical_lines(body_text);
    let mut target = receiver;
    for _ in 0..=lines.len() {
        if binding_count(&lines, target) != 1 {
            break;
        }
        let Some((_, value)) = lines
            .iter()
            .filter_map(|line| simple_binding(line))
            .find(|(name, _)| *name == target)
        else {
            break;
        };
        if !is_plain_name(value) {
            break;
        }
        target = value;
    }
    target.to_string()
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

/// The application name a client fixture builds its client over: every
/// `return` / `yield` of the fixture body is a client construction over the
/// same plain name, directly or through one bound name (`with TestClient(app)
/// as c: yield c`). `None` when any exit is not, so a fixture whose client the
/// adapter cannot read leaves the application unidentified.
pub(super) fn fixture_client_app_name(fixture_body: &str) -> Option<String> {
    let lines = logical_lines(fixture_body);
    let mut found: Option<String> = None;
    for line in lines.iter().flat_map(|line| line.split(';')) {
        let line = exit_statement(line.trim());
        let Some(value) = line
            .strip_prefix("return ")
            .or_else(|| line.strip_prefix("yield "))
        else {
            continue;
        };
        let value = value.trim();
        let app = if let Some(arguments) = client_construction(value) {
            application_argument(arguments)
                .filter(|application| is_plain_name(application))
                .map(str::to_string)
        } else if is_plain_name(value) {
            client_app_name(fixture_body, value)
        } else {
            None
        }?;
        match &found {
            Some(existing) if *existing != app => return None,
            _ => found = Some(app),
        }
    }
    found
}

/// The `return`/`yield` statement a line ends in, so that a one-line compound
/// suite (`if flag: return TestClient(other_app)`) counts as an exit of the
/// fixture rather than being skipped.
fn exit_statement(line: &str) -> &str {
    if line.starts_with("return ") || line.starts_with("yield ") {
        return line;
    }
    ["return ", "yield "]
        .iter()
        .filter_map(|keyword| {
            line.match_indices(keyword).map(|(at, _)| at).find(|at| {
                line[..*at]
                    .chars()
                    .next_back()
                    .is_some_and(|before| before == ':' || before.is_whitespace())
            })
        })
        .min()
        .map_or(line, |at| &line[at..])
}

#[cfg(test)]
mod tests;
