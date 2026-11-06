//! Predicate-swap evidence for the Python preview adapter (#9186, #11890).
//!
//! A changed predicate that swaps one single-operand test of a bare owner
//! parameter for another (`if x is None:` -> `if not x:`) changes the branch
//! only for inputs the two tests tell apart. `is None` and `not x` agree on
//! `None` and on truthy values; a falsy non-`None` value (`0`, `""`, `False`)
//! is where they differ. A strong exact-value oracle whose every related call
//! passes only inputs on which the old and the new test agree
//! (`pick(None, 3)`, `pick(5, 3)`) still passes after the swap, so reach plus
//! a strong oracle does not discriminate it.
//!
//! The rule engages only when static evidence can read everything it needs:
//!
//! - the old and the new condition are each exactly `x is None`,
//!   `x is not None`, `not x` or `x` over the same bare owner parameter, with
//!   the rest of the line unchanged; or they are compound `and`/`or`
//!   conditions whose joining operators match, exactly one operand differs,
//!   and both differing operands parse as those single-operand tests;
//! - the tested name is a bare owner parameter that the owner never rebinds,
//!   or `self.<attr>` when the owner's class `__init__` assigns
//!   `self.<attr> = <param>` exactly once and nothing else in the class
//!   writes `self.<attr>`;
//! - every strong related call binds that parameter to a scalar literal
//!   (`None`, `True`/`False`, a number, a plain quoted string) — for a
//!   `self.<attr>` operand, from the inline constructor
//!   (`Addr("h", 8080).authority()`, including an omitted default) — and no
//!   strong call goes unread.
//!
//! When every such row gives the old and the new test the same truth value,
//! no observed input selects a different branch. A row that tells them apart,
//! an unreadable or non-literal argument, a rebound parameter, a second
//! attribute write, a receiver built elsewhere, two differing compound
//! operands, or any other unreadable shape fails open and keeps the existing
//! verdict.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::owner_call_names;
use super::boundary::{
    BoundaryActivation, CallRow, PythonBoundaryEvidence, strong_owner_call_rows_with_gaps,
    unrebound_constants,
};
use super::no_behavior::{
    call_arglists_with_offsets, call_segment_keyword_name, matching_call_paren,
    split_top_level_args,
};

use super::related_tests::PythonRelatedCandidate;
use crate::domain::OwnerKind;
use crate::{domain::ValueContext, value_origins::LocatedValueFact as ValueFact};

/// One single-operand test of a bare name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Test {
    IsNone,
    IsNotNone,
    Falsy,
    Truthy,
}

impl Test {
    fn holds(self, input: Input) -> bool {
        match self {
            Self::IsNone => input == Input::None,
            Self::IsNotNone => input != Input::None,
            Self::Falsy => !input.truthy(),
            Self::Truthy => input.truthy(),
        }
    }
}

/// A scalar literal as an input to a single-operand test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Input {
    None,
    Truthy,
    Falsy,
}

impl Input {
    fn truthy(self) -> bool {
        self == Self::Truthy
    }
}

/// The condition as `(name, test)`, or None for anything but exactly one
/// single-operand test of a bare identifier.
fn parse_test(condition: &str) -> Option<(String, Test)> {
    let condition = condition.trim();
    if let Some(rest) = condition.strip_suffix(" is not None") {
        return Some((identifier(rest)?, Test::IsNotNone));
    }
    if let Some(rest) = condition.strip_suffix(" is None") {
        return Some((identifier(rest)?, Test::IsNone));
    }
    if let Some(rest) = condition.strip_prefix("not ") {
        return Some((identifier(rest)?, Test::Falsy));
    }
    Some((identifier(condition)?, Test::Truthy))
}

fn identifier(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(attr) = text.strip_prefix("self.") {
        return identifier(attr)
            .filter(|plain| plain == attr)
            .map(|_| text.to_string());
    }
    let mut chars = text.chars();
    let first = chars.next()?;
    let plain = (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    let keyword = matches!(
        text,
        "None" | "True" | "False" | "not" | "and" | "or" | "is" | "in" | "if" | "else" | "lambda"
    );
    (plain && !keyword).then(|| text.to_string())
}

/// Top-level `and`/`or` operands and the joining operators between them.
fn split_boolean_condition(condition: &str) -> Option<(Vec<&str>, Vec<&str>)> {
    let mut operands = Vec::new();
    let mut operators = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut start = 0usize;
    let bytes = condition.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let ch = condition[idx..].chars().next()?;
        let ch_len = ch.len_utf8();
        if escaped {
            escaped = false;
            idx += ch_len;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            idx += ch_len;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            idx += ch_len;
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && keyword_at(condition, idx, "and") => {
                let operand = condition[start..idx].trim();
                if operand.is_empty() {
                    return None;
                }
                operands.push(operand);
                operators.push("and");
                idx += 3;
                start = idx;
                continue;
            }
            _ if depth == 0 && keyword_at(condition, idx, "or") => {
                let operand = condition[start..idx].trim();
                if operand.is_empty() {
                    return None;
                }
                operands.push(operand);
                operators.push("or");
                idx += 2;
                start = idx;
                continue;
            }
            _ => {}
        }
        idx += ch_len;
    }
    let last = condition[start..].trim();
    if last.is_empty() {
        return None;
    }
    operands.push(last);
    (operands.len() >= 2).then_some((operands, operators))
}

fn keyword_at(text: &str, idx: usize, word: &str) -> bool {
    let end = idx + word.len();
    text.get(idx..end) == Some(word)
        && (idx == 0 || !is_ident_byte(text.as_bytes()[idx - 1]))
        && text
            .as_bytes()
            .get(end)
            .is_none_or(|byte| !is_ident_byte(*byte))
}

/// The one swapped single-operand test, or None when the change is not a
/// readable truthiness swap (#9186, #11890).
fn swapped_operand(old_condition: &str, new_condition: &str) -> Option<(String, Test, Test)> {
    if let (Some((old_name, old_test)), Some((new_name, new_test))) =
        (parse_test(old_condition), parse_test(new_condition))
    {
        return (old_name == new_name && old_test != new_test)
            .then_some((old_name, old_test, new_test));
    }
    let (old_ops, old_joiners) = split_boolean_condition(old_condition)?;
    let (new_ops, new_joiners) = split_boolean_condition(new_condition)?;
    if old_ops.len() != new_ops.len() || old_joiners != new_joiners {
        return None;
    }
    let mut differ = None;
    for (old_op, new_op) in old_ops.iter().zip(new_ops.iter()) {
        if old_op == new_op {
            continue;
        }
        if differ.is_some() {
            return None;
        }
        let (old_name, old_test) = parse_test(old_op)?;
        let (new_name, new_test) = parse_test(new_op)?;
        if old_name != new_name || old_test == new_test {
            return None;
        }
        differ = Some((old_name, old_test, new_test));
    }
    differ
}

fn self_attr_name(name: &str) -> Option<&str> {
    name.strip_prefix("self.")
        .filter(|attr| identifier(attr).as_deref() == Some(*attr))
}

/// The input a literal argument text stands for, None when it is not a
/// scalar whose truthiness static evidence can read.
fn read_input(value: &str) -> Option<Input> {
    let value = value.trim();
    match value {
        "None" => return Some(Input::None),
        "True" => return Some(Input::Truthy),
        "False" => return Some(Input::Falsy),
        _ => {}
    }
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == '-')
    {
        let number = value.parse::<f64>().ok()?;
        return Some(if number == 0.0 {
            Input::Falsy
        } else {
            Input::Truthy
        });
    }
    let quote = value.chars().next().filter(|ch| matches!(ch, '\'' | '"'))?;
    let inner = value.strip_prefix(quote)?.strip_suffix(quote)?;
    // Triple-quoted, prefixed or escaped strings are not read.
    if inner.contains(quote) || inner.contains('\\') {
        return None;
    }
    Some(if inner.is_empty() {
        Input::Falsy
    } else {
        Input::Truthy
    })
}

/// The first example input the two tests tell apart, spelled as a
/// discriminator; a falsy non-`None` value comes first, which is where
/// `is None` and `not x` differ.
fn distinguishing_example(name: &str, old: Test, new: Test) -> Option<String> {
    [Input::Falsy, Input::None, Input::Truthy]
        .into_iter()
        .find(|input| old.holds(*input) != new.holds(*input))
        .map(|input| match input {
            Input::None => format!("{name} is None"),
            Input::Falsy => format!("{name} == 0"),
            Input::Truthy => format!("{name} == 1"),
        })
}

/// Evidence that no strong related call places an input on which the old and
/// the new single-operand test disagree. None when the change is not a
/// readable swap or any strong input tells the two apart.
pub(super) fn python_truthiness_swap_evidence(
    line_text: &str,
    old_line_text: &str,
    (condition, old_condition): (&str, &str),
    owner: &PythonOwner,
    owner_source: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonBoundaryEvidence> {
    let (name, old_test, new_test) = swapped_operand(old_condition, condition)?;
    // The line around the condition is unchanged: a branch value edit is a
    // different change. The condition must be one whole-identifier span.
    if outside(line_text, condition)? != outside(old_line_text, old_condition)? {
        return None;
    }
    let attr = self_attr_name(&name);
    let mapping = match attr {
        Some(attr) => Some(once_init_attr_mapping(owner_source?, owner, attr)?),
        None => {
            if !owner
                .parameters
                .iter()
                .any(|parameter| parameter.name == name)
            {
                return None;
            }
            if !parameter_only_read(owner_source?, owner, &name, line_text) {
                return None;
            }
            None
        }
    };
    let extra: Vec<&str> = mapping
        .as_ref()
        .map(|mapped| vec![mapped.class_name.as_str()])
        .unwrap_or_default();
    if owner_reached_indirectly(owner, related_candidates, &extra) {
        return None;
    }
    let constants = unrebound_constants(owner, related_candidates);
    let (rows, gaps) =
        strong_owner_call_rows_with_gaps(owner, &constants, related_candidates, focus);
    if gaps || rows.is_empty() {
        return None;
    }
    let mut observed_values = Vec::new();
    for row in &rows {
        let (input, from_default, value) = match &mapping {
            Some(mapped) => constructor_attr_input(owner, mapped, row)?,
            None => {
                let binding = row.bindings.get(&name)?;
                (
                    read_input(&binding.value)?,
                    binding.from_default,
                    binding.value.clone(),
                )
            }
        };
        if old_test.holds(input) != new_test.holds(input) {
            return None;
        }
        if !from_default {
            observed_values.push(ValueFact {
                file: Some(row.file.clone()),
                line: row.line,
                text: row.text.clone(),
                value: format!("{name} = {value}"),
                context: ValueContext::FunctionArgument,
            });
        }
    }
    let discriminator = distinguishing_example(&name, old_test, new_test)?;
    crate::analysis::classify::sort_value_facts(&mut observed_values);
    Some(PythonBoundaryEvidence {
        activation: BoundaryActivation::Missing,
        discriminator: Some(discriminator),
        observed_values,
        reason: format!(
            "Every strong related test call gives `{name}` a value on which `{old_condition}` and `{condition}` agree, so no observed input selects a different branch"
        ),
    })
}

struct InitParam {
    name: String,
    default: Option<String>,
}

struct AttrMapping {
    class_name: String,
    param: String,
    init_params: Vec<InitParam>,
}

/// `self.<attr>` maps to one `__init__` parameter assigned exactly once in
/// the class, with no other write to that attribute (#11890).
fn once_init_attr_mapping(source: &str, owner: &PythonOwner, attr: &str) -> Option<AttrMapping> {
    if !matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    ) {
        return None;
    }
    let class_name = owner.class_path.as_str();
    if class_name.is_empty() || class_name.contains('.') {
        return None;
    }
    let class_lines = class_body_lines(source, class_name)?;
    let (init_header, init_body) = init_block(&class_lines)?;
    let init_params = init_params(init_header)?;
    let writes: Vec<&str> = class_lines
        .iter()
        .copied()
        .filter(|line| line_writes_self_attr(line, attr))
        .collect();
    if writes.len() != 1 {
        return None;
    }
    let write = writes[0];
    if !init_body.iter().copied().any(|line| line == write) {
        return None;
    }
    let param = exact_init_assign(write, attr)?;
    if !init_params.iter().any(|declared| declared.name == param) {
        return None;
    }
    Some(AttrMapping {
        class_name: class_name.to_string(),
        param,
        init_params,
    })
}

fn class_body_lines<'a>(source: &'a str, class_name: &str) -> Option<Vec<&'a str>> {
    let header = format!("class {class_name}");
    let mut found: Option<(usize, usize)> = None;
    for (index, line) in source.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(&header) else {
            continue;
        };
        let boundary = rest
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        if !boundary {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some((index, indent));
    }
    let (start, class_indent) = found?;
    let mut body = Vec::new();
    for line in source.lines().skip(start + 1) {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            body.push(line);
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= class_indent {
            break;
        }
        body.push(line);
    }
    Some(body)
}

fn init_block<'a>(class_lines: &[&'a str]) -> Option<(&'a str, Vec<&'a str>)> {
    let mut found: Option<(usize, usize, &'a str)> = None;
    for (index, line) in class_lines.iter().enumerate() {
        let trimmed = strip_trailing_comment(line).trim();
        if !trimmed.starts_with("def __init__(") {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some((index, line.len() - line.trim_start().len(), trimmed));
    }
    let (start, init_indent, header) = found?;
    let mut body = Vec::new();
    for line in class_lines.iter().skip(start + 1) {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            body.push(*line);
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= init_indent {
            break;
        }
        body.push(*line);
    }
    Some((header, body))
}

fn init_params(header: &str) -> Option<Vec<InitParam>> {
    let args = header.strip_prefix("def __init__(")?;
    let close = args.rfind(')')?;
    let inner = &args[..close];
    if inner.contains('*') || inner.contains('/') {
        return None;
    }
    let mut params = Vec::new();
    for (index, segment) in split_top_level_args(inner).into_iter().enumerate() {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if index == 0 {
            if segment != "self" && segment != "cls" {
                return None;
            }
            continue;
        }
        let (name, default) = match segment.split_once('=') {
            Some((name, default)) => (name.trim(), Some(default.trim().to_string())),
            None => (segment, None),
        };
        identifier(name).filter(|plain| plain == name)?;
        params.push(InitParam {
            name: name.to_string(),
            default,
        });
    }
    Some(params)
}

fn assignment_span(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    bytes.iter().enumerate().find_map(|(index, byte)| {
        if *byte != b'=' {
            return None;
        }
        let prev = index.checked_sub(1).map(|i| bytes[i]);
        let next = bytes.get(index + 1).copied();
        let comparison = next == Some(b'=')
            || matches!(prev, Some(b'=' | b'!'))
            || (matches!(prev, Some(b'<' | b'>'))
                && line.get(index.saturating_sub(2)..index) != Some("<<")
                && line.get(index.saturating_sub(2)..index) != Some(">>"));
        if comparison {
            return None;
        }
        let start = match prev {
            Some(b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^') => index - 1,
            Some(b'<') | Some(b'>')
                if line.get(index.saturating_sub(2)..index) == Some("<<")
                    || line.get(index.saturating_sub(2)..index) == Some(">>") =>
            {
                index - 2
            }
            _ => index,
        };
        Some((start, index + 1 - start))
    })
}

fn line_writes_self_attr(line: &str, attr: &str) -> bool {
    let line = strip_trailing_comment(line).trim();
    let Some((op_at, _)) = assignment_span(line) else {
        return false;
    };
    let left = line[..op_at].trim();
    mentions_identifier(left, &format!("self.{attr}"))
}

fn exact_init_assign(line: &str, attr: &str) -> Option<String> {
    let line = strip_trailing_comment(line).trim();
    let prefix = format!("self.{attr} =");
    let rhs = line.strip_prefix(&prefix)?.trim();
    identifier(rhs).filter(|plain| plain == rhs)
}

fn constructor_attr_input(
    owner: &PythonOwner,
    mapping: &AttrMapping,
    row: &CallRow,
) -> Option<(Input, bool, String)> {
    let arglist = inline_constructor_arglist(&row.text, &mapping.class_name, &owner.name)?;
    let (value, from_default) = bind_init_param(arglist, &mapping.init_params, &mapping.param)?;
    let input = read_input(&value)?;
    Some((input, from_default, value))
}

fn inline_constructor_arglist<'a>(
    text: &'a str,
    class_name: &str,
    method: &str,
) -> Option<&'a str> {
    let calls = call_arglists_with_offsets(text, method, true);
    if calls.len() != 1 {
        return None;
    }
    let method_start = calls[0].0;
    if method_start == 0 || text.as_bytes().get(method_start - 1) != Some(&b'.') {
        return None;
    }
    let dot = method_start - 1;
    if dot == 0 || text.as_bytes().get(dot - 1) != Some(&b')') {
        return None;
    }
    let close = dot - 1;
    let open = matching_open_paren(text, close)?;
    if open == 0 {
        return None;
    }
    let prefix = &text[..open];
    if !prefix.ends_with(class_name) {
        return None;
    }
    let name_start = prefix.len() - class_name.len();
    let whole = (name_start == 0 || !is_ident_byte(text.as_bytes()[name_start - 1]))
        && text
            .as_bytes()
            .get(open)
            .is_none_or(|byte| !is_ident_byte(*byte));
    whole.then_some(&text[open + 1..close])
}

fn matching_open_paren(text: &str, close: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut found = None;
    for (idx, ch) in text.get(..close)?.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' if matching_call_paren(text, idx) == Some(close) => found = Some(idx),
            _ => {}
        }
    }
    found
}

fn bind_init_param(arglist: &str, params: &[InitParam], wanted: &str) -> Option<(String, bool)> {
    if arglist.contains('#') {
        return None;
    }
    let mut bound = Vec::new();
    let mut positional = 0usize;
    let mut seen_keyword = false;
    for segment in split_top_level_args(arglist) {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if segment.starts_with('*') {
            return None;
        }
        if let Some(name) = call_segment_keyword_name(segment) {
            seen_keyword = true;
            if !params.iter().any(|param| param.name == name) {
                return None;
            }
            if bound.iter().any(|(bound_name, _)| bound_name == name) {
                return None;
            }
            let value = segment.split_once('=')?.1.trim();
            if value.is_empty() {
                return None;
            }
            bound.push((name.to_string(), value.to_string()));
        } else {
            if seen_keyword {
                return None;
            }
            let name = params.get(positional)?.name.clone();
            if bound.iter().any(|(bound_name, _)| bound_name == &name) {
                return None;
            }
            positional += 1;
            bound.push((name, segment.to_string()));
        }
    }
    for param in params {
        if bound.iter().any(|(name, _)| name == &param.name) {
            continue;
        }
        param.default.as_ref()?;
    }
    if let Some((_, value)) = bound.iter().find(|(name, _)| name == wanted) {
        return Some((value.clone(), false));
    }
    let default = params
        .iter()
        .find(|param| param.name == wanted)?
        .default
        .clone()?;
    Some((default, true))
}

/// The line with the condition replaced by a marker and whitespace
/// normalized; None unless the condition occurs exactly once as whole
/// identifiers (`x` is not found inside `max`, and `return x if x else 0`
/// is ambiguous).
fn outside(line: &str, condition: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let starts: Vec<usize> = line
        .match_indices(condition)
        .map(|(start, _)| start)
        .filter(|start| {
            let end = start + condition.len();
            (*start == 0 || !is_ident_byte(bytes[start - 1]))
                && bytes.get(end).is_none_or(|byte| !is_ident_byte(*byte))
        })
        .collect();
    let [start] = starts.as_slice() else {
        return None;
    };
    let joined = format!(
        "{}\u{0}{}",
        &line[..*start],
        &line[start + condition.len()..]
    );
    Some(joined.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The line without a trailing `# comment` (a `#` outside quotes).
pub(super) fn strip_trailing_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, ch) {
            (_, '\\') => escaped = true,
            (Some(active), _) if ch == active => quote = None,
            (None, '\'' | '"') => quote = Some(ch),
            (None, '#') => return line[..index].trim_end(),
            _ => {}
        }
    }
    line
}

/// Closed allow-list: the parameter may appear in the owner body (after the
/// `def` header line) only on the changed line, or on a line that is a plain
/// `return`/`if`/`elif`/`while`/`raise`/`assert` statement holding no
/// assignment, walrus, comprehension, lambda, `as` target or dynamic-scope
/// call. Any other mention (an assignment target in any shape, a nested
/// `def`/`class`/`import`, a `for`/`case`/`with` target, `global`, a
/// continuation line) refuses, so an unrecognized shape keeps the existing
/// verdict.
fn parameter_only_read(
    owner_source: &str,
    owner: &PythonOwner,
    name: &str,
    changed_line: &str,
) -> bool {
    let changed = strip_trailing_comment(changed_line).trim();
    // The changed line is a plain read statement itself: an assignment that
    // holds the swapped condition (`x = i if not x else x`) rebinds the
    // parameter. It is also subject to the f-string and non-ASCII refusals.
    if !plain_read_statement(changed) || has_fstring(changed_line) || has_unicode_name(changed) {
        return false;
    }
    owner_source
        .lines()
        .skip(owner.start_line)
        .take(owner.end_line.saturating_sub(owner.start_line))
        .all(|raw| {
            // Comment stripping tracks quotes naively, which an f-string
            // reusing its quote (Python 3.12) can defeat; refuse any
            // f-string. A non-ASCII letter can NFKC-normalize to the
            // parameter name (fullwidth `ｘ`), so refuse those too.
            // An owner docstring doctest (`>>>`) can call the owner too.
            if has_fstring(raw) || has_unicode_name(raw) || raw.contains(">>>") {
                return false;
            }
            let line = strip_trailing_comment(raw).trim();
            if DYNAMIC_SCOPE.iter().any(|word| line.contains(word)) {
                return false;
            }
            if !mentions_identifier(line, name) || line == changed {
                return true;
            }
            plain_read_statement(line)
        })
}

/// Whether the text holds a non-ASCII letter or digit (an identifier
/// character that Python NFKC-normalizes).
fn has_unicode_name(text: &str) -> bool {
    text.chars()
        .any(|ch| !ch.is_ascii() && ch.is_alphanumeric())
}

/// Whether the text holds an f-string literal (`f"`, `rf'`, `Fr"`).
fn has_fstring(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        if !matches!(byte, b'"' | b'\'') {
            return false;
        }
        let mut start = index;
        while start > 0 && matches!(bytes[start - 1], b'r' | b'R' | b'f' | b'F' | b'b' | b'B') {
            start -= 1;
        }
        let standalone = start == 0 || !is_ident_byte(bytes[start - 1]);
        standalone && bytes[start..index].iter().any(|p| matches!(p, b'f' | b'F'))
    })
}

/// Calls that read or write the local scope by name.
const DYNAMIC_SCOPE: &[&str] = &[
    "locals", "globals", "vars(", "exec", "eval", "setattr", "__dict__", "inspect",
];

fn mentions_identifier(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices(name).any(|(start, _)| {
        (start == 0 || !is_ident_byte(bytes[start - 1]))
            && bytes
                .get(start + name.len())
                .is_none_or(|byte| !is_ident_byte(*byte))
    })
}

/// A statement that starts with a read-only keyword and holds no binding
/// construct.
fn plain_read_statement(line: &str) -> bool {
    const STARTS: &[&str] = &["return ", "if ", "elif ", "while ", "raise ", "assert "];
    if !STARTS.iter().any(|start| line.starts_with(start)) {
        return false;
    }
    if [" for ", "lambda", " as ", ":=", ";", "\\", "yield", "await"]
        .iter()
        .any(|word| line.contains(word))
    {
        return false;
    }
    // A `:` inside a condition header must be its last character (an inline
    // body such as `if flag: x = 0` is not a plain read); `return` and the
    // other keywords may hold no colon at all (a dict, slice or annotation).
    let colons = line.matches(':').count();
    let header = ["if ", "elif ", "while "]
        .iter()
        .any(|start| line.starts_with(start));
    if colons > usize::from(header && line.ends_with(':')) {
        return false;
    }
    !has_assignment(line)
}

/// A standalone `=` or an augmented assignment operator.
fn has_assignment(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        if *byte != b'=' {
            return false;
        }
        let prev = index.checked_sub(1).map(|i| bytes[i]);
        let next = bytes.get(index + 1).copied();
        // `==`, `!=`, `<=`, `>=` are comparisons; `+=`, `-=` and the rest
        // are assignments.
        let comparison = next == Some(b'=')
            || matches!(prev, Some(b'=' | b'!'))
            || (matches!(prev, Some(b'<' | b'>'))
                && line.get(index.saturating_sub(2)..index) != Some("<<")
                && line.get(index.saturating_sub(2)..index) != Some(">>"));
        !comparison
    })
}

/// Whether some related test mentions the owner other than as a bare direct
/// call (`map(pick, ..)`, `alias = pick`, a callback argument), or calls a
/// bare name the test may define itself (`w(0)` where `w` forwards to the
/// owner), so the rows do not cover every strong input.
fn owner_reached_indirectly(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    extra_callees: &[&str],
) -> bool {
    let method_call = matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    );
    related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            let text = candidate.test.body_text.as_str();
            let names = owner_call_names(owner, candidate.test, method_call);
            has_unicode_name(text)
                || has_fstring(text)
                || calls_through_a_result(text)
                || owner_call_nested_in_owner_call(text, &names, method_call)
                || names.iter().any(|name| mentions_without_call(text, name))
                || calls_unrecognized_callee(text, &names, method_call, extra_callees)
        })
}

/// Whether an owner call appears inside another owner call's argument list
/// (`pick(None, pick(0, 7))`): the inner call's result is an input the row
/// reader does not bind.
fn owner_call_nested_in_owner_call(text: &str, names: &[String], method_call: bool) -> bool {
    names.iter().any(|name| {
        call_arglists_with_offsets(text, name, method_call)
            .into_iter()
            .any(|(_, arglist)| {
                names.iter().any(|other| {
                    mentions_identifier(arglist, other.rsplit('.').next().unwrap_or(other))
                })
            })
    })
}

/// Whether a call's callee is itself a subscript or a call result
/// (`RUNNERS[0](0)`, `make()(0)`): a callable the reader cannot name.
fn calls_through_a_result(text: &str) -> bool {
    text.match_indices('(')
        .any(|(index, _)| text[..index].trim_end().ends_with([')', ']']))
}

const UNITTEST_ASSERTS: &[&str] = &[
    "assertEqual",
    "assertNotEqual",
    "assertTrue",
    "assertFalse",
    "assertIs",
    "assertIsNot",
    "assertIsNone",
    "assertIsNotNone",
    "assertIn",
    "assertNotIn",
    "assertIsInstance",
    "assertNotIsInstance",
    "assertAlmostEqual",
    "assertNotAlmostEqual",
    "assertGreater",
    "assertGreaterEqual",
    "assertLess",
    "assertLessEqual",
    "assertRaises",
    "assertRaisesRegex",
    "assertListEqual",
    "assertTupleEqual",
    "assertSetEqual",
    "assertDictEqual",
    "assertCountEqual",
    "assertSequenceEqual",
    "assertRegex",
    "assertWarns",
];

/// Whether the body makes a call other than the recognized ones: the owner
/// (a method owner under any receiver; a function owner only under a name or
/// module alias the reader recognizes), known pure builtins, the bare `assert`
/// statement, the named `self.assert*` unittest methods, `pytest.raises`/
/// `approx`/`warns`/`param`/`mark.*`, and the `def`/`class` headers. Any other
/// callee, attribute calls included (`helpers.run(0)`, `self.check(0)`,
/// `m.pick(0, 3)` through an alias), may pass an input to the owner that the
/// call-row reader never sees.
fn calls_unrecognized_callee(
    text: &str,
    owner_names: &[String],
    method_call: bool,
    extra_callees: &[&str],
) -> bool {
    const BUILTINS: &[&str] = &[
        "len",
        "str",
        "int",
        "float",
        "bool",
        "list",
        "tuple",
        "dict",
        "set",
        "range",
        "sorted",
        "sum",
        "min",
        "max",
        "abs",
        "round",
        "repr",
        "isinstance",
        "type",
        "bytes",
        "enumerate",
        "zip",
        "reversed",
        "any",
        "all",
    ];
    const PYTEST: &[&str] = &[
        "pytest.raises",
        "pytest.approx",
        "pytest.warns",
        "pytest.param",
    ];
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !is_ident_byte(bytes[index]) && bytes[index] != b'.' {
            index += 1;
            continue;
        }
        // A callee chain: identifiers joined by dots, possibly starting with
        // a dot (a method on a call result).
        let start = index;
        while index < bytes.len() && (is_ident_byte(bytes[index]) || bytes[index] == b'.') {
            index += 1;
        }
        let chain = &text[start..index];
        if !text[index..].trim_start().starts_with('(') {
            continue;
        }
        let before = text[..start].trim_end();
        let last = chain.rsplit('.').next().unwrap_or(chain);
        let header = before.ends_with("def") || before.ends_with("class");
        let recognized = header
            || chain.starts_with(|ch: char| ch.is_ascii_digit())
            || BUILTINS.contains(&chain)
            || PYTEST.contains(&chain)
            || chain.starts_with("pytest.mark.")
            || chain == "assert"
            || chain
                .strip_prefix("self.")
                .is_some_and(|method| UNITTEST_ASSERTS.contains(&method))
            || extra_callees.contains(&chain)
            || owner_names.iter().any(|name| {
                if method_call {
                    name.rsplit('.').next() == Some(last)
                } else {
                    name == chain
                }
            });
        let keyword = matches!(
            chain,
            "not"
                | "and"
                | "or"
                | "in"
                | "is"
                | "if"
                | "elif"
                | "while"
                | "return"
                | "with"
                | "for"
                | "else"
                | "lambda"
                | "yield"
                | "await"
        );
        if !(recognized || keyword) {
            return true;
        }
    }
    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `name` (the last segment of a dotted alias) occurs as a whole
/// identifier not directly followed by `(`.
fn mentions_without_call(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let last = name.rsplit('.').next().unwrap_or(name);
    text.match_indices(last).any(|(start, _)| {
        let end = start + last.len();
        let whole = (start == 0 || !is_ident_byte(bytes[start - 1]))
            && bytes.get(end).is_none_or(|byte| !is_ident_byte(*byte));
        whole && !text[end..].trim_start().starts_with('(')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_test_reads_only_one_single_operand_test_of_a_bare_name() {
        assert_eq!(parse_test("x is None"), Some(("x".into(), Test::IsNone)));
        assert_eq!(
            parse_test("x is not None"),
            Some(("x".into(), Test::IsNotNone))
        );
        assert_eq!(parse_test("not x"), Some(("x".into(), Test::Falsy)));
        assert_eq!(parse_test("x"), Some(("x".into(), Test::Truthy)));
        // #11890: a `self.<attr>` operand is the tested name, not a compound.
        assert_eq!(
            parse_test("self.port"),
            Some(("self.port".into(), Test::Truthy))
        );
        assert_eq!(
            parse_test("self.port is not None"),
            Some(("self.port".into(), Test::IsNotNone))
        );
        assert_eq!(
            parse_test("not self.port"),
            Some(("self.port".into(), Test::Falsy))
        );
        for compound in [
            "x is None or y",
            "not x.y",
            "not (x)",
            "x == 1",
            "len(x)",
            "not None",
            "True",
            "x is y",
            "self.port != 80",
            "self.port.x",
            "cls.port",
        ] {
            assert_eq!(parse_test(compound), None, "{compound}");
        }
    }

    #[test]
    fn split_boolean_condition_keeps_top_level_and_or_operands() {
        assert_eq!(
            split_boolean_condition("self.port and self.port != 80"),
            Some((vec!["self.port", "self.port != 80"], vec!["and"]))
        );
        assert_eq!(
            split_boolean_condition("self.port is not None and self.port != 80"),
            Some((
                vec!["self.port is not None", "self.port != 80"],
                vec!["and"]
            ))
        );
        assert_eq!(split_boolean_condition("x"), None);
        assert_eq!(
            swapped_operand(
                "self.port and self.port != 80",
                "self.port is not None and self.port != 80"
            ),
            Some(("self.port".into(), Test::Truthy, Test::IsNotNone))
        );
        assert_eq!(
            swapped_operand("x and y", "x is not None and y is not None"),
            None
        );
    }

    #[test]
    fn outside_matches_whole_identifiers_once() {
        assert_eq!(outside("    if x:", "x").as_deref(), Some("if \u{0}:"));
        // `x` inside `max` is not the condition, so the span is still unique.
        assert_eq!(
            outside("    return max(1, 2) if x else 0", "x").as_deref(),
            Some("return max(1, 2) if \u{0} else 0")
        );
        // Two whole occurrences are ambiguous.
        assert_eq!(outside("    return x if x else 0", "x"), None);
        assert_eq!(outside("    return 0", "x"), None);
    }

    #[test]
    fn strip_trailing_comment_ignores_hashes_in_strings() {
        assert_eq!(strip_trailing_comment("if x:  # note"), "if x:");
        assert_eq!(
            strip_trailing_comment("if x == '#':  # note"),
            "if x == '#':"
        );
        assert_eq!(strip_trailing_comment("if x:"), "if x:");
    }

    #[test]
    fn indirect_reach_is_any_mention_other_than_a_direct_call() {
        let names = vec!["pick".to_string()];
        assert!(!mentions_without_call("assert pick(1, 2) == 1", "pick"));
        assert!(!mentions_without_call("assert test_pick(1)", "pick"));
        assert!(mentions_without_call("list(map(pick, a, b))", "pick"));
        assert!(mentions_without_call("alias = pick", "pick"));
        assert!(!calls_unrecognized_callee(
            "def test_pick():\n    assert len(pick(1)) == 1\n    pytest.raises(E)\n    self.assertEqual(pick(1), 1)\n",
            &names,
            false,
            &[]
        ));
        for unknown in [
            "assert w(0) == 3",
            "assert helpers.run(0) == 3",
            "assert self.check(0) == 3",
            "mock.patch('a')",
            "assert str(pick(1)).strip()",
        ] {
            assert!(
                calls_unrecognized_callee(unknown, &names, false, &[]),
                "{unknown}"
            );
        }
    }

    #[test]
    fn plain_read_statements_are_a_closed_allow_list() {
        for read in [
            "return x",
            "if x is None:",
            "elif not x:",
            "while x > 0:",
            "return d if x >= 1 else 0",
            "raise ValueError(x)",
        ] {
            assert!(plain_read_statement(read), "{read}");
        }
        for binding in [
            "(a, x) = d",
            "[a, x] = d",
            "x = 0",
            "if flag: x = 0",
            "return [x for x in d]",
            "with open(p) as x:",
            "return (x := 1)",
            "x) = d",
            "for x in d:",
            "del x",
            "def inner(x):",
            "case x:",
            "import x",
            "return lambda x: x",
            "x <<= 1",
        ] {
            assert!(!plain_read_statement(binding), "{binding}");
        }
        // The changed line is held to the same rule.
        assert!(!plain_read_statement("x = i if not x else x"));
        assert!(plain_read_statement("return i if not x else x"));
        assert!(has_fstring("s = f'{d}'"));
        assert!(has_fstring("print(Rf\"{d}\")"));
        assert!(!has_fstring("s = 'if'") && !has_fstring("buf = b'a'"));
        assert!(has_unicode_name("\u{ff58} = 0") && !has_unicode_name("x = 'a'"));
        assert!(calls_through_a_result("RUNNERS[0](0)"));
        assert!(calls_through_a_result("make()(0)"));
        assert!(!calls_through_a_result("assert pick(1)[0] == 1"));
        let names = vec!["pick".to_string()];
        // A function owner accepts only a recognized name; a method owner
        // accepts any receiver.
        assert!(calls_unrecognized_callee(
            "m.pick(0, 3)",
            &names,
            false,
            &[]
        ));
        assert!(!calls_unrecognized_callee(
            "m.pick(0, 3)",
            &names,
            true,
            &[]
        ));
        assert!(has_fstring("f\"{pick(0, 3)}\""));
        assert!(calls_unrecognized_callee(
            "assert_pick(0, 3)",
            &names,
            false,
            &[]
        ));
        assert!(calls_unrecognized_callee(
            "self.assertHelper(0)",
            &names,
            false,
            &[]
        ));
        assert!(!calls_unrecognized_callee(
            "self.assertEqual(pick(1), 1)",
            &names,
            false,
            &[]
        ));
        assert!(!calls_unrecognized_callee(
            "assert Addr(\"h\").authority() == \"h\"",
            &["authority".to_string()],
            true,
            &["Addr"]
        ));
        assert!(has_assignment("x <<= 1"));
        assert!(!has_assignment("a >= b and c <= d != e == f"));
    }

    #[test]
    fn parameter_only_read_refuses_every_alias_or_rebind_of_the_parameter() {
        let changed = "    return 1 if not x else 2";
        let owner_for = |lines: usize| {
            let mut owner = super::super::static_limits::tests::owner_with(&[], Vec::new());
            owner.start_line = 1;
            owner.end_line = lines;
            owner
        };
        let check = |body: &str| {
            let source = format!("def f(x):\n{body}\n{changed}\n");
            let owner = owner_for(source.lines().count());
            parameter_only_read(&source, &owner, "x", changed)
        };
        // Positive control: a plain read elsewhere in the body is accepted.
        assert!(check("    if x is None:\n        pass"));
        for shape in [
            "    y = x",
            "    y = [x]",
            "    x = x or 1",
            "    x = other",
            "    x: int = x",
            "    for x in range(3):",
            "    with ctx() as x:",
            "    del x",
            "    global x",
            "    nonlocal x",
            "    a, x = pair",
            "    (a, x) = pair",
            "    x += 1",
            "    x",
            "    other = lambda: x",
            "    if (y := x):",
        ] {
            assert!(!check(shape), "{shape}");
        }
    }

    #[test]
    fn read_input_reads_only_plain_scalar_literals() {
        assert_eq!(read_input("None"), Some(Input::None));
        assert_eq!(read_input("5"), Some(Input::Truthy));
        assert_eq!(read_input("0"), Some(Input::Falsy));
        assert_eq!(read_input("0.0"), Some(Input::Falsy));
        assert_eq!(read_input("''"), Some(Input::Falsy));
        assert_eq!(read_input("\"a\""), Some(Input::Truthy));
        assert_eq!(read_input("False"), Some(Input::Falsy));
        for unread in ["[]", "x", "\"\"\"a\"\"\"", "'a\\n'", "1.2.3", "-", "f'{a}'"] {
            assert_eq!(read_input(unread), None, "{unread}");
        }
    }

    #[test]
    fn tests_disagree_only_where_the_swap_changes_the_branch() {
        // `x is None` -> `not x` differ on a falsy non-None value alone.
        assert_eq!(
            distinguishing_example("x", Test::IsNone, Test::Falsy),
            Some("x == 0".to_string())
        );
        for input in [Input::None, Input::Truthy] {
            assert_eq!(Test::IsNone.holds(input), Test::Falsy.holds(input));
        }
        // `x` -> `not x` inverts every input.
        assert_eq!(
            distinguishing_example("x", Test::Truthy, Test::Falsy),
            Some("x == 0".to_string())
        );
        // `x is None` -> `x` agree on a falsy non-None value only.
        assert_eq!(
            distinguishing_example("x", Test::IsNone, Test::Truthy),
            Some("x is None".to_string())
        );
        assert_eq!(
            Test::IsNone.holds(Input::Falsy),
            Test::Truthy.holds(Input::Falsy)
        );
    }

    use super::super::classify_change_with_source;
    use super::super::owners_tests::{extract_owners, extract_tests};
    use crate::domain::{ExposureClass, Finding};
    use std::path::Path;

    fn classify_swap(
        source: &str,
        old_line: &str,
        new_line: &str,
        tests: &str,
        file: &str,
        test_file: &str,
    ) -> Result<Finding, String> {
        let line = source
            .lines()
            .position(|line| line == new_line)
            .map(|index| index + 1)
            .ok_or_else(|| format!("no new line in source: {new_line}"))?;
        let owners = extract_owners(Path::new(file), source);
        let extracted = extract_tests(Path::new(test_file), tests);
        classify_change_with_source(
            Path::new(file),
            line,
            new_line,
            Some(old_line),
            &owners,
            &extracted,
            Some(source),
        )
        .ok_or_else(|| "swap must produce a finding".to_string())
    }

    const ADDR_OLD: &str = "        if self.port and self.port != 80:";
    const ADDR_NEW: &str = "        if self.port is not None and self.port != 80:";
    const ADDR_SOURCE: &str = concat!(
        "class Addr:\n",
        "    def __init__(self, host, port=None):\n",
        "        self.host = host\n",
        "        self.port = port\n",
        "\n",
        "    def authority(self):\n",
        "        out = self.host\n",
        "        if self.port is not None and self.port != 80:\n",
        "            out += \":\" + str(self.port)\n",
        "        return out\n",
    );
    const ADDR_TESTS: &str = concat!(
        "from src.addr import Addr\n",
        "\n",
        "def test_authority():\n",
        "    assert Addr(\"example.com\").authority() == \"example.com\"\n",
        "    assert Addr(\"example.com\", 8080).authority() == \"example.com:8080\"\n",
    );

    fn addr_finding(source: &str, tests: &str) -> Result<Finding, String> {
        classify_swap(
            source,
            ADDR_OLD,
            ADDR_NEW,
            tests,
            "src/addr.py",
            "tests/test_addr.py",
        )
    }

    fn discriminator_names_self_port_zero(finding: &Finding) -> bool {
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "self.port == 0" || fact.value == "self.port = 0")
            || finding
                .evidence
                .iter()
                .any(|line| line.contains("`self.port == 0`") || line.contains("`self.port = 0`"))
            || finding
                .missing
                .iter()
                .any(|line| line.contains("`self.port == 0`") || line.contains("`self.port = 0`"))
    }

    // #11890: ports None/8080 agree under `self.port` and `self.port is not None`.
    #[test]
    fn self_attr_compound_truthiness_swap_with_constructor_literals_is_weakly_exposed()
    -> Result<(), String> {
        let finding = addr_finding(ADDR_SOURCE, ADDR_TESTS)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        assert!(
            discriminator_names_self_port_zero(&finding),
            "expected a falsy-value discriminator naming self.port = 0, got {finding:?}"
        );
        Ok(())
    }

    // #11890: Addr("h", 0) is the first input that separates the two tests.
    #[test]
    fn self_attr_compound_truthiness_swap_stays_exposed_on_a_falsy_constructor_argument()
    -> Result<(), String> {
        let tests = concat!(
            "from src.addr import Addr\n",
            "\n",
            "def test_authority():\n",
            "    assert Addr(\"h\", 0).authority() == \"h:0\"\n",
        );
        let finding = addr_finding(ADDR_SOURCE, tests)?;
        assert_ne!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        Ok(())
    }

    // #11890: a later write to self.port makes the constructor value unreadable.
    #[test]
    fn self_attr_compound_truthiness_swap_stays_exposed_when_the_attr_is_rewritten()
    -> Result<(), String> {
        let source = concat!(
            "class Addr:\n",
            "    def __init__(self, host, port=None):\n",
            "        self.host = host\n",
            "        self.port = port\n",
            "\n",
            "    def reset(self):\n",
            "        self.port = 0\n",
            "\n",
            "    def authority(self):\n",
            "        out = self.host\n",
            "        if self.port is not None and self.port != 80:\n",
            "            out += \":\" + str(self.port)\n",
            "        return out\n",
        );
        let finding = addr_finding(source, ADDR_TESTS)?;
        assert_ne!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        Ok(())
    }

    // #11890 step (a): a compound with two swapped operands is not a single-test swap.
    #[test]
    fn compound_truthiness_swap_stays_exposed_when_two_operands_differ() -> Result<(), String> {
        let source = concat!(
            "def pick(x, y, d):\n",
            "    if x is not None and y is not None:\n",
            "        return x\n",
            "    return d\n",
        );
        let tests = concat!(
            "from src.lib import pick\n",
            "\n",
            "def test_pick():\n",
            "    assert pick(None, 1, 3) == 3\n",
            "    assert pick(5, 1, 3) == 5\n",
        );
        let finding = classify_swap(
            source,
            "    if x and y:",
            "    if x is not None and y is not None:",
            tests,
            "src/lib.py",
            "tests/test_lib.py",
        )?;
        assert_ne!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        Ok(())
    }

    // #11890 step (a): a bare-parameter compound with one swapped operand demotes.
    #[test]
    fn bare_parameter_compound_truthiness_swap_with_unseparated_calls_is_weakly_exposed()
    -> Result<(), String> {
        let source = concat!(
            "def pick(x, d):\n",
            "    if x is not None and x != 80:\n",
            "        return x\n",
            "    return d\n",
        );
        let tests = concat!(
            "from src.lib import pick\n",
            "\n",
            "def test_pick():\n",
            "    assert pick(None, 3) == 3\n",
            "    assert pick(8080, 3) == 8080\n",
        );
        let finding = classify_swap(
            source,
            "    if x and x != 80:",
            "    if x is not None and x != 80:",
            tests,
            "src/lib.py",
            "tests/test_lib.py",
        )?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        Ok(())
    }

    // Keyword names must match `__init__`; an extra name is an unreadable
    // construction and must not demote (#11890 fail-open).
    #[test]
    fn self_attr_compound_truthiness_swap_stays_exposed_on_an_unknown_constructor_keyword()
    -> Result<(), String> {
        let tests = concat!(
            "from src.addr import Addr\n",
            "\n",
            "def test_authority():\n",
            "    assert Addr(\"h\", extra=1).authority() == \"h\"\n",
            "    assert Addr(\"h\", 8080).authority() == \"h:8080\"\n",
        );
        let finding = addr_finding(ADDR_SOURCE, tests)?;
        assert_ne!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        Ok(())
    }

    // A missing required constructor argument is unreadable even when the
    // wanted parameter is a keyword literal.
    #[test]
    fn self_attr_compound_truthiness_swap_stays_exposed_when_a_required_constructor_arg_is_missing()
    -> Result<(), String> {
        let tests = concat!(
            "from src.addr import Addr\n",
            "\n",
            "def test_authority():\n",
            "    assert Addr(port=8080).authority() == \":8080\"\n",
        );
        let finding = addr_finding(ADDR_SOURCE, tests)?;
        assert_ne!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        Ok(())
    }

    // Keyword order is readable when every `__init__` parameter is bound
    // exactly once, including an omitted default.
    #[test]
    fn self_attr_compound_keyword_constructor_literals_are_weakly_exposed() -> Result<(), String> {
        let tests = concat!(
            "from src.addr import Addr\n",
            "\n",
            "def test_authority():\n",
            "    assert Addr(port=8080, host=\"h\").authority() == \"h:8080\"\n",
            "    assert Addr(host=\"example.com\").authority() == \"example.com\"\n",
        );
        let finding = addr_finding(ADDR_SOURCE, tests)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        assert!(
            discriminator_names_self_port_zero(&finding),
            "expected a falsy-value discriminator naming self.port = 0, got {finding:?}"
        );
        Ok(())
    }
}
