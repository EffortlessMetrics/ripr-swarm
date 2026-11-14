use super::PythonOwner;
use super::probe_shape::looks_like_call_expression;
use super::related_tests::is_python_identifier_char;
use super::sink_alignment::parse_dict_literal_fields;
use super::static_limits::{
    contains_python_call_shape, is_simple_python_identifier, line_prefix_before,
    python_callee_start_has_boundary, python_prefix_hides_code,
};
use crate::domain::{FlowSinkFact, MissingDiscriminatorFact, ProbeFamily};

pub(super) fn python_missing_discriminators(
    probe_family: &ProbeFamily,
    line: usize,
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    flow_sink: Option<&FlowSinkFact>,
) -> Vec<MissingDiscriminatorFact> {
    let Some(value) =
        python_missing_discriminator_value(probe_family, line_text, old_line_text, owner)
    else {
        return Vec::new();
    };

    vec![MissingDiscriminatorFact::equality_spelled(
        value,
        python_missing_discriminator_reason(probe_family, line),
        flow_sink.cloned(),
    )]
}

fn python_missing_discriminator_value(
    probe_family: &ProbeFamily,
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
) -> Option<String> {
    match probe_family {
        ProbeFamily::Predicate => python_boundary_discriminator(line_text),
        ProbeFamily::ReturnValue => python_return_value_discriminator(line_text),
        ProbeFamily::ErrorPath => python_exception_discriminator(line_text),
        ProbeFamily::FieldConstruction => {
            python_field_value_discriminator(line_text, old_line_text, owner)
        }
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
            python_output_or_call_discriminator(line_text)
        }
        ProbeFamily::MatchArm | ProbeFamily::StaticUnknown => None,
    }
}

fn python_missing_discriminator_reason(probe_family: &ProbeFamily, line: usize) -> String {
    let shape = match probe_family {
        ProbeFamily::Predicate => "equality-boundary",
        ProbeFamily::ReturnValue => "returned-value",
        ProbeFamily::ErrorPath => "exception",
        ProbeFamily::FieldConstruction => "field/object value",
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => "output/log/call effect",
        ProbeFamily::MatchArm => "match-arm",
        ProbeFamily::StaticUnknown => "static",
    };
    format!("changed Python {shape} at line {line} lacks a concrete repair discriminator")
}

fn python_boundary_discriminator(line_text: &str) -> Option<String> {
    let expression = strip_python_control_prefix(line_text);
    for operator in [">=", "<=", ">", "<"] {
        if let Some(idx) = expression.find(operator) {
            let left = comparison_operand_before(&expression, idx)?;
            let right = comparison_operand_after(&expression, idx + operator.len())?;
            if is_simple_python_discriminator_operand(&left)
                && is_simple_python_discriminator_operand(&right)
            {
                return Some(format!("{left} == {right}"));
            }
        }
    }
    None
}

fn python_return_value_discriminator(line_text: &str) -> Option<String> {
    let expression = line_text.trim().strip_prefix("return ")?.trim();
    if expression.is_empty() {
        None
    } else {
        Some(format!(
            "return value == {}",
            python_expected_value_or_placeholder(expression)
        ))
    }
}

/// Placeholder for an expected value the syntax-first preview cannot derive.
/// Same `<...>` placeholder style the Python repair card already uses for
/// `<expected exception>` and `<instance>`.
const PYTHON_EXPECTED_VALUE_PLACEHOLDER: &str = "<expected value>";

/// The expected side of a synthesized equality discriminator. A literal is an
/// independent expected value (`return 42` → `== 42`). Any other expression is
/// the changed production computation itself: echoing it would make the
/// suggested oracle restate the changed code (`== sum(...) + 1`), which passes
/// for every mutant of that expression. Name no concrete value then (#4216 row 6).
fn python_expected_value_or_placeholder(value: &str) -> &str {
    if is_literal_python_model_field_value(value) {
        value.trim()
    } else {
        PYTHON_EXPECTED_VALUE_PLACEHOLDER
    }
}

fn python_exception_discriminator(line_text: &str) -> Option<String> {
    let raised = line_text.trim().strip_prefix("raise ")?.trim();
    if raised.is_empty() {
        return None;
    }
    let exception_type = raised
        .split_once('(')
        .map(|(ty, _)| ty.trim())
        .unwrap_or(raised)
        .trim();
    if exception_type.is_empty() {
        return None;
    }
    if let Some(message) = first_python_string_literal(raised) {
        Some(format!("raises {exception_type} matching {message}"))
    } else {
        Some(format!("raises {exception_type}"))
    }
}

fn python_field_value_discriminator(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
) -> Option<String> {
    let text = line_text.trim();
    if let Some(pick) = python_return_dict_discriminator_pick(text, old_line_text) {
        let routed = !owner.route_paths.is_empty();
        return Some(match pick {
            PythonDictDiscriminatorPick::Entry { key, value } => {
                let value = python_expected_value_or_placeholder(&value);
                if routed {
                    format!("response.json()[\"{key}\"] == {value}")
                } else {
                    format!("{key} == {value}")
                }
            }
            PythonDictDiscriminatorPick::WholeDict if routed => {
                format!("response.json() == {PYTHON_EXPECTED_VALUE_PLACEHOLDER}")
            }
            PythonDictDiscriminatorPick::WholeDict => {
                format!("return value == {PYTHON_EXPECTED_VALUE_PLACEHOLDER}")
            }
        });
    }
    if let Some((_constructor, field, value)) = python_return_constructor_field_parts(text) {
        let value = python_expected_value_or_placeholder(&value);
        return Some(format!("result.{field} == {value}"));
    }
    if let Some((target, _constructor, field, value)) =
        python_assignment_constructor_field_parts(text)
    {
        let value = python_expected_value_or_placeholder(&value);
        if !owner.route_paths.is_empty() {
            return python_route_response_field_discriminator(&field, value);
        }
        return Some(format!("{target}.{field} == {value}"));
    }
    let (lhs, rhs) = split_python_assignment(text)?;
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    Some(format!(
        "{lhs} == {}",
        python_expected_value_or_placeholder(rhs)
    ))
}

/// Which part of a changed `return {...}` dict the repair discriminator names.
#[derive(Debug, PartialEq, Eq)]
enum PythonDictDiscriminatorPick {
    /// One entry: the single changed entry when the paired old line localizes
    /// it, otherwise the unlocalized first-literal/first-entry pick.
    Entry { key: String, value: String },
    /// The paired old line shows the change is not inside one surviving entry
    /// (several entries changed, or an entry was removed without one new key
    /// replacing it). Naming any
    /// single key could name an unchanged sibling, so the whole returned dict
    /// is the discriminator (#13630).
    WholeDict,
}

/// The discriminator pick for a `return {...}` changed line. With a paired old
/// dict line, the changed entries are localized with the same dict-literal
/// parser the sink alignment uses (`parse_dict_literal_fields`): exactly one
/// changed new-side entry is named, including a renamed key (the new key
/// replaces one removed old key), never the unchanged sibling the first-entry
/// pick would choose; several changed entries, or a removal with no
/// replacing entry, name the whole dict. Without a paired old dict line, or
/// when no entry differs, the existing unlocalized pick stands.
fn python_return_dict_discriminator_pick(
    line_text: &str,
    old_line_text: Option<&str>,
) -> Option<PythonDictDiscriminatorPick> {
    let (fallback_key, fallback_value) = python_return_dict_field_parts(line_text)?;
    let fallback = PythonDictDiscriminatorPick::Entry {
        key: fallback_key,
        value: fallback_value,
    };
    let (Some(old), Some(new)) = (
        old_line_text.and_then(parse_dict_literal_fields),
        parse_dict_literal_fields(line_text),
    ) else {
        return Some(fallback);
    };
    let old_value = |key: &str| old.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let changed: Vec<&(String, String)> = new
        .iter()
        .filter(|(key, value)| old_value(key) != Some(value))
        .collect();
    let removed = old
        .iter()
        .filter(|(key, _)| !new.iter().any(|(k, _)| k == key))
        .count();
    let pick = match (changed.as_slice(), removed) {
        ([], 0) => fallback,
        ([(key, value)], 0) => PythonDictDiscriminatorPick::Entry {
            key: key.clone(),
            value: value.clone(),
        },
        ([(key, value)], 1) if old_value(key).is_none() => PythonDictDiscriminatorPick::Entry {
            key: key.clone(),
            value: value.clone(),
        },
        _ => PythonDictDiscriminatorPick::WholeDict,
    };
    Some(pick)
}

pub(super) fn python_route_response_field_discriminator(
    field: &str,
    value: &str,
) -> Option<String> {
    match field {
        "status" | "status_code" => Some(format!("response.status_code == {value}")),
        "detail" => Some(format!("response.json()[\"detail\"] == {value}")),
        _ => Some(format!("response.{field} == {value}")),
    }
}

pub(super) fn python_return_dict_field_discriminator(line_text: &str) -> Option<String> {
    let (key, value) = python_return_dict_field_parts(line_text)?;
    Some(format!("{key} == {value}"))
}

pub(super) fn python_return_constructor_field_discriminator(line_text: &str) -> Option<String> {
    let (_constructor, field, value) = python_return_constructor_field_parts(line_text)?;
    Some(format!("result.{field} == {value}"))
}

pub(super) fn python_return_dict_field_parts(line_text: &str) -> Option<(String, String)> {
    let expression = line_text.trim().strip_prefix("return ")?.trim();
    let body = expression
        .strip_prefix('{')?
        .trim_start()
        .trim_end_matches('}')
        .trim_end();
    let mut fallback = None;
    for segment in top_level_python_segments(body) {
        let Some((key, value)) = python_dict_field_segment_parts(segment) else {
            continue;
        };
        if fallback.is_none() {
            fallback = Some((key.to_string(), value.to_string()));
        }
        if is_literal_python_model_field_value(value) {
            return Some((key.to_string(), value.to_string()));
        }
    }
    fallback
}

pub(super) fn top_level_python_segments(text: &str) -> Vec<&str> {
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut segment_start = 0usize;
    let mut segments = Vec::new();
    for (idx, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                segments.push(text[segment_start..idx].trim());
                segment_start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    segments.push(text[segment_start..].trim());
    segments
}

pub(super) fn python_dict_field_segment_parts(segment: &str) -> Option<(&str, &str)> {
    let colon = top_level_colon(segment)?;
    let key = segment[..colon].trim().trim_matches('"').trim_matches('\'');
    let value = segment[colon + 1..].trim().trim_end_matches('}').trim();
    if key.is_empty() || value.is_empty() {
        None
    } else {
        Some((key, value))
    }
}

pub(super) fn python_return_constructor_field_parts(
    line_text: &str,
) -> Option<(String, String, String)> {
    let expression = line_text.trim().strip_prefix("return ")?.trim();
    let (constructor, args) = split_python_constructor_call(expression)?;
    if !is_python_constructor_callee(constructor) {
        return None;
    }
    let (field, value) = first_python_keyword_argument(args)?;
    if !is_simple_python_model_field_value(value) {
        return None;
    }
    Some((
        constructor.to_string(),
        field.to_string(),
        value.to_string(),
    ))
}

pub(super) fn python_assignment_constructor_field_parts(
    line_text: &str,
) -> Option<(String, String, String, String)> {
    let (target, expression) = split_python_assignment(line_text.trim())?;
    if !is_simple_python_identifier(target) {
        return None;
    }
    let (constructor, args) = split_python_constructor_call(expression)?;
    if !is_python_constructor_callee(constructor) {
        return None;
    }
    let (field, value) = first_python_keyword_argument(args)?;
    if !is_simple_python_model_field_value(value) {
        return None;
    }
    Some((
        target.to_string(),
        constructor.to_string(),
        field.to_string(),
        value.to_string(),
    ))
}

pub(super) fn split_python_constructor_call(expression: &str) -> Option<(&str, &str)> {
    let expression = expression.trim();
    if !looks_like_call_expression(expression) {
        return None;
    }
    let open = expression.find('(')?;
    let close = expression.rfind(')')?;
    if close <= open {
        return None;
    }
    let callee = expression[..open].trim();
    let args = expression[open + 1..close].trim();
    (!callee.is_empty() && !args.is_empty()).then_some((callee, args))
}

pub(super) fn is_python_constructor_callee(callee: &str) -> bool {
    let last_segment = callee.rsplit('.').next().unwrap_or(callee).trim();
    let mut chars = last_segment.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(super) fn first_python_keyword_argument(args: &str) -> Option<(&str, &str)> {
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut segment_start = 0usize;
    for (idx, ch) in args.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if let Some(parts) = python_keyword_argument_parts(&args[segment_start..idx]) {
                    return Some(parts);
                }
                segment_start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    python_keyword_argument_parts(&args[segment_start..])
}

pub(super) fn python_keyword_argument_parts(segment: &str) -> Option<(&str, &str)> {
    let segment = segment.trim();
    let equals = top_level_equals(segment)?;
    let field = segment[..equals].trim();
    let value = segment[equals + 1..].trim();
    (is_simple_python_identifier(field) && !value.is_empty()).then_some((field, value))
}

pub(super) fn top_level_equals(text: &str) -> Option<usize> {
    top_level_delimiter(text, '=')
}

fn top_level_colon(text: &str) -> Option<usize> {
    top_level_delimiter(text, ':')
}

fn top_level_delimiter(text: &str, delimiter: char) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    for (idx, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if ch == delimiter && depth == 0 => return Some(idx),
            _ => {}
        }
    }
    None
}

pub(super) fn is_simple_python_model_field_value(value: &str) -> bool {
    let value = value.trim();
    is_literal_python_model_field_value(value) || is_simple_python_identifier(value)
}

pub(super) fn is_literal_python_model_field_value(value: &str) -> bool {
    let value = value.trim();
    python_string_literal_value(value).is_some()
        || matches!(value, "True" | "False" | "None")
        || is_simple_python_numeric_literal(value)
}

fn is_simple_python_numeric_literal(value: &str) -> bool {
    let value = value.trim().strip_prefix('-').unwrap_or(value.trim());
    if value.is_empty() {
        return false;
    }
    let mut digits = 0usize;
    let mut dots = 0usize;
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
        } else if ch == '.' {
            dots += 1;
            if dots > 1 {
                return false;
            }
        } else {
            return false;
        }
    }
    digits > 0
}

pub(super) fn python_output_or_call_discriminator(line_text: &str) -> Option<String> {
    let text = line_text.trim();
    if let Some(exit_code) = python_exit_code_discriminator(text) {
        return Some(format!("exit_code == {exit_code}"));
    }
    let literal = first_python_string_literal(text)?;
    if python_stdout_output_call(text) {
        Some(format!("stdout contains {literal}"))
    } else if python_stderr_output_call(text) {
        Some(format!("stderr contains {literal}"))
    } else if python_cli_output_call(text) || text.starts_with("print(") {
        Some(format!("output contains {literal}"))
    } else if text.contains("logger.") || text.contains("logging.") {
        Some(format!("log contains {literal}"))
    } else {
        Some(format!("call includes {literal}"))
    }
}

fn python_cli_output_call(text: &str) -> bool {
    contains_python_call_shape(text, "click.echo") || contains_python_call_shape(text, "typer.echo")
}

fn python_stdout_output_call(text: &str) -> bool {
    contains_python_call_shape(text, "sys.stdout.write")
}

fn python_stderr_output_call(text: &str) -> bool {
    contains_python_call_shape(text, "sys.stderr.write")
}

pub(super) fn python_exit_code_discriminator(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(argument) = first_python_call_argument(text, "sys.exit") {
        return normalize_python_exit_code(argument);
    }
    if let Some(rest) = text.strip_prefix("raise SystemExit") {
        let rest = rest.trim_start();
        if let Some(argument) = first_parenthesized_argument(rest) {
            return normalize_python_exit_code(argument);
        }
    }
    None
}

fn first_python_call_argument<'a>(text: &'a str, callee: &str) -> Option<&'a str> {
    let idx = text.find(callee)?;
    if !python_callee_start_has_boundary(text, idx)
        || python_prefix_hides_code(line_prefix_before(text, idx))
    {
        return None;
    }
    first_parenthesized_argument(text.get(idx + callee.len()..)?.trim_start())
}

fn first_parenthesized_argument(text: &str) -> Option<&str> {
    let body = text.strip_prefix('(')?;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (idx, ch) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                let argument = body[..idx].split(',').next()?.trim();
                return (!argument.is_empty()).then_some(argument);
            }
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn normalize_python_exit_code(argument: &str) -> Option<String> {
    let value = argument.trim();
    if value == "None" {
        return Some("0".to_string());
    }
    if value.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(value.to_string());
    }
    None
}

pub(super) fn split_python_assignment(text: &str) -> Option<(&str, &str)> {
    if text.contains("==") || text.contains("!=") || text.contains(">=") || text.contains("<=") {
        return None;
    }
    let (lhs, rhs) = text.split_once('=')?;
    Some((lhs.trim(), rhs.trim()))
}

/// Parse an attribute-assignment changed line `recv.attr = value` into
/// `(receiver, attr, rhs)`. Returns `None` for non-attribute assignments — a bare
/// local assign (`status = 0`, no `.`), an augmented assign (`x.n += 1`, whose LHS
/// keeps the operator and fails the identifier check), a comparison, or any line
/// without `=`. Used to scope the changed-sink receiver/value identity gate to
/// attribute writes only; everything else keeps the existing alignment behavior.
pub(super) fn parse_attribute_assignment(line_text: &str) -> Option<(&str, &str, &str)> {
    let (lhs, rhs) = split_python_assignment(line_text)?;
    let (receiver, attr) = lhs.rsplit_once('.')?;
    let receiver = receiver.trim();
    let attr = attr.trim();
    if receiver.is_empty()
        || attr.is_empty()
        || !receiver.chars().all(is_python_identifier_char)
        || !attr.chars().all(is_python_identifier_char)
    {
        return None;
    }
    Some((receiver, attr, rhs))
}

pub(super) fn first_python_string_literal(text: &str) -> Option<String> {
    let mut start = None;
    let mut escaped = false;
    for (idx, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' || ch == '\'' {
            start = Some((idx, ch));
            break;
        }
    }
    let (start_idx, quote) = start?;
    escaped = false;
    for (relative_idx, ch) in text[start_idx + quote.len_utf8()..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            let end_idx = start_idx + quote.len_utf8() + relative_idx + quote.len_utf8();
            return text.get(start_idx..end_idx).map(str::to_string);
        }
    }
    None
}

pub(super) fn python_string_literal_value(text: &str) -> Option<String> {
    let trimmed = text.trim();
    // One triple-quoted literal: the first unescaped closing delimiter must
    // end the text. Adjacent-string concatenation (`"a" "b"`) stays
    // non-literal.
    for delimiter in [r#"""""#, "'''"] {
        if let Some(body) = trimmed.strip_prefix(delimiter) {
            let close = python_closing_delimiter_offset(body, delimiter)?;
            return (close + delimiter.len() == body.len())
                .then(|| body.get(..close).map(str::to_string))
                .flatten();
        }
    }
    let mut chars = trimmed.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    if !trimmed.ends_with(quote) || trimmed.len() < quote.len_utf8() * 2 {
        return None;
    }
    // One literal only: the first unescaped closing quote must be the last
    // character. `"Hello, " + name + "!"` or `"yes" if flag else "no"` start
    // and end with a quote but are compound expressions (#4216 row 6).
    let close = quote.len_utf8() + python_closing_quote_offset(chars.as_str(), quote)?;
    if close != trimmed.len() - quote.len_utf8() {
        return None;
    }
    trimmed.get(quote.len_utf8()..close).map(str::to_string)
}

/// Byte offset in `body` of the first unescaped triple-quote `delimiter`.
fn python_closing_delimiter_offset(body: &str, delimiter: &str) -> Option<usize> {
    let mut escaped = false;
    for (idx, ch) in body.char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if body
            .get(idx..)
            .is_some_and(|rest| rest.starts_with(delimiter))
        {
            return Some(idx);
        }
    }
    None
}

/// Byte offset in `body` of the first unescaped `quote`.
fn python_closing_quote_offset(body: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (idx, ch) in body.char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(idx);
        }
    }
    None
}

fn strip_python_control_prefix(line_text: &str) -> String {
    let mut text = line_text.trim().trim_end_matches(':').trim().to_string();
    for prefix in ["if ", "elif ", "while ", "case ", "return "] {
        if let Some(stripped) = text.strip_prefix(prefix) {
            text = stripped.trim().to_string();
            break;
        }
    }
    text
}

pub(super) fn is_python_control_predicate_line(line_text: &str) -> bool {
    let trimmed = line_text.trim_start();
    (trimmed.contains(" if ") && trimmed.contains(" else "))
        || trimmed.starts_with("if ")
        || trimmed.starts_with("elif ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("case ")
}

fn comparison_operand_before(expression: &str, operator_start: usize) -> Option<String> {
    let left = expression.get(..operator_start)?.trim_end();
    let operand = left
        .rsplit(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '(' | ')' | '[' | ']' | '{' | '}' | ',' | ':' | '+' | '-' | '*' | '/' | '%'
                )
        })
        .find(|part| !part.is_empty())?;
    Some(operand.trim().to_string())
}

fn comparison_operand_after(expression: &str, operator_end: usize) -> Option<String> {
    let right = expression.get(operator_end..)?.trim_start();
    let operand = right
        .split(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '(' | ')' | '[' | ']' | '{' | '}' | ',' | ':' | '+' | '-' | '*' | '/' | '%'
                )
        })
        .find(|part| !part.is_empty())?;
    Some(operand.trim().to_string())
}

fn is_simple_python_discriminator_operand(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '"' || ch == '\''
        })
}
