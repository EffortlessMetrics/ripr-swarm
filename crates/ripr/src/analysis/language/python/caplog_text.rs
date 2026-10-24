//! `caplog.text` membership evaluated for a changed logging call
//! (RIPR-SPEC-0238 rule 22, #9546).
//!
//! A test that reads `"<text>" in caplog.text` observes the formatted message
//! of the records its call emits. For one narrow shape that text is decidable
//! from the source alone, so the edit is credited when it provably removes the
//! asserted text and reported as a gap when it provably keeps it:
//!
//! * the changed line is a logging call `<receiver>.<level>("<fmt>", <args>)`
//!   whose arguments are string or integer literals or owner parameters the
//!   test's calls bind to such literals, formatted with `%s`, `%d`, `%i` and
//!   `%%`;
//! * the owner reaches that line through plain `if <comparison>:` blocks only
//!   (no `else`, loop, `try` or `with`), every one of which each of the test's
//!   owner calls provably enters, and nothing after the line can log;
//! * the test pins the capture level with exactly one `caplog.at_level(<level>)`
//!   block around its owner calls or one `caplog.set_level(<level>)` before
//!   them, so whether a record is captured is decided by the call's level
//!   alone, and the test makes no other call that could log;
//! * the module configures no logger (`setLevel`, `propagate`, filters,
//!   handlers or `logging` configuration).
//!
//! The asserted text must carry a space after a character the default record
//! header (`LEVEL    name:file:line `) cannot end a word with, so it is read
//! from the message and not from the header. A `log_format` configuration is
//! not read. Anything outside these bounds keeps the abstention.

use super::approx_band::parse_expression;
use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::boundary::{CallRow, owner_call_rows_for_candidate, unrebound_constants};
use super::discriminators::python_string_literal_value;
use super::guard_activation::{condition_holds, leading_whitespace};
use super::no_behavior::split_top_level_args;
use super::related_tests::PythonRelatedCandidate;
use super::static_limits::caplog_membership_literals;
use super::{PythonOwner, PythonTest, expr_full_name};
use crate::domain::{OwnerKind, ValueContext, ValueFact};
use rustpython_parser::ast::{self, Expr};

/// Module text that reconfigures a logger and so changes which records reach
/// the test's handler.
const LOGGER_CONFIGURATION: [&str; 10] = [
    "setLevel",
    "propagate",
    "disabled",
    "addFilter",
    "addHandler",
    "logging.disable",
    "basicConfig",
    "dictConfig",
    "fileConfig",
    "captureWarnings",
];

/// Test text that changes what `caplog` holds or captures.
const CAPLOG_INTERFERENCE: [&str; 6] = [
    "setLevel",
    "caplog.clear",
    "caplog.handler",
    "disable",
    "monkeypatch",
    "patch",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PythonLogVerdict {
    /// The asserted text is in the old message and not in the new one.
    Rejects,
    /// The asserted text is in both.
    Absorbs,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PythonLogEvidence {
    pub(super) verdict: PythonLogVerdict,
    pub(super) discriminator: String,
    pub(super) observed_values: Vec<ValueFact>,
    pub(super) reason: String,
}

struct LogCall {
    receiver: String,
    level: u32,
    level_name: String,
    format: String,
    arguments: Vec<Expr>,
}

/// Evidence for a changed logging call from the `caplog.text` assertions of
/// the related tests, or None when the shape is outside the bounded reading.
pub(super) fn python_log_evidence(
    old_line: Option<&str>,
    new_line: &str,
    changed_line: usize,
    owner: &PythonOwner,
    owner_source: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonLogEvidence> {
    let (old_line, owner_source) = (old_line?, owner_source?);
    if owner.owner_kind != Some(OwnerKind::Function)
        || LOGGER_CONFIGURATION
            .iter()
            .any(|needle| owner_source.contains(needle))
    {
        return None;
    }
    let old = parse_log_call(old_line)?;
    let new = parse_log_call(new_line)?;
    if old.receiver != new.receiver {
        return None;
    }
    let conditions = plain_if_conditions(owner, owner_source, changed_line)?;
    let constants = unrebound_constants(owner, related_candidates);
    let mut evaluations = Vec::new();
    let mut observed_values = Vec::new();
    for candidate in related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        let assertion =
            select_relevant_assertion(&candidate.test.assertions, Some(focus)).assertion()?;
        let literals = caplog_membership_literals(&assertion.gate_text);
        if literals.is_empty() {
            // Another assertion may catch the edit too; this reading only
            // proves what the caplog assertions say.
            return None;
        }
        let rows = owner_call_rows_for_candidate(owner, &constants, candidate)?;
        let pin = level_pin(candidate.test, owner)?;
        let mut old_texts = Vec::new();
        let mut new_texts = Vec::new();
        for row in &rows {
            if !pin.covers(row.line) {
                return None;
            }
            // A call the guards keep out of the changed line records nothing
            // for either version; an unreadable guard is not decided.
            let mut entered = true;
            for (index, condition) in &conditions {
                match condition_holds(
                    condition,
                    *index,
                    row,
                    &constants,
                    owner_source,
                    owner.start_line,
                ) {
                    Some(true) => {}
                    Some(false) => entered = false,
                    None => return None,
                }
            }
            if !entered {
                continue;
            }
            old_texts.extend(emitted_text(&old, row, pin.level)?);
            new_texts.extend(emitted_text(&new, row, pin.level)?);
            observed_values.extend(row.bindings.iter().filter(|(_, b)| !b.from_default).map(
                |(parameter, binding)| ValueFact {
                    file: None,
                    line: row.line,
                    text: row.text.clone(),
                    value: format!("{parameter} = {}", binding.value),
                    context: ValueContext::FunctionArgument,
                },
            ));
        }
        for literal in literals {
            if !anchored_in_message(&literal) {
                return None;
            }
            evaluations.push(Evaluation {
                old_present: old_texts.iter().any(|text| text.contains(&literal)),
                new_present: new_texts.iter().any(|text| text.contains(&literal)),
                literal,
                old_text: old_texts.first().cloned(),
                new_text: new_texts.first().cloned(),
            });
        }
    }
    // A test that did not already see the text before the edit says nothing
    // about it afterwards.
    if evaluations.is_empty() || evaluations.iter().any(|evaluation| !evaluation.old_present) {
        return None;
    }
    observed_values.sort_by(|a, b| a.line.cmp(&b.line).then(a.value.cmp(&b.value)));
    observed_values.dedup();
    if let Some(gone) = evaluations
        .iter()
        .find(|evaluation| !evaluation.new_present)
    {
        let new_detail = gone.new_text.as_deref().map_or_else(
            || {
                format!(
                    "the new `{}.{}` call is below the capture level the test pins, so no record is captured",
                    new.receiver, new.level_name
                )
            },
            |text| format!("the new message reads `{text}`"),
        );
        return Some(PythonLogEvidence {
            verdict: PythonLogVerdict::Rejects,
            discriminator: String::new(),
            observed_values,
            reason: format!(
                "the asserted text `{}` is in the old logged message `{}` and not after the edit: {new_detail}",
                gone.literal,
                gone.old_text.as_deref().unwrap_or_default(),
            ),
        });
    }
    let kept = evaluations.first()?;
    Some(PythonLogEvidence {
        verdict: PythonLogVerdict::Absorbs,
        discriminator: "an asserted text the edit removes from the logged message".to_string(),
        observed_values,
        reason: format!(
            "the asserted text `{}` is in both the old and the new logged message, so the assertion passes either way",
            kept.literal
        ),
    })
}

struct Evaluation {
    literal: String,
    old_present: bool,
    new_present: bool,
    old_text: Option<String>,
    new_text: Option<String>,
}

/// `<receiver>.<level>("<fmt>", <args>)` with no keyword arguments.
fn parse_log_call(line: &str) -> Option<LogCall> {
    let Expr::Call(call) = parse_expression(line.trim())? else {
        return None;
    };
    if !call.keywords.is_empty() {
        return None;
    }
    let Expr::Attribute(attribute) = call.func.as_ref() else {
        return None;
    };
    let level_name = attribute.attr.to_string();
    let level = level_number(&level_name)?;
    let receiver = expr_full_name(attribute.value.as_ref())?;
    let Some(Expr::Constant(first)) = call.args.first() else {
        return None;
    };
    let ast::Constant::Str(format) = &first.value else {
        return None;
    };
    Some(LogCall {
        receiver,
        level,
        level_name,
        format: format.clone(),
        arguments: call.args[1..].to_vec(),
    })
}

/// The numeric level of a logging method (`exception` logs at ERROR).
fn level_number(method: &str) -> Option<u32> {
    match method {
        "debug" => Some(10),
        "info" => Some(20),
        "warning" | "warn" => Some(30),
        "error" | "exception" => Some(40),
        "critical" => Some(50),
        _ => None,
    }
}

/// A level named by `logging.<NAME>`, a bare name, a string or an integer.
fn level_from_expr(expr: &Expr) -> Option<u32> {
    let name = match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Int(number) => return number.to_string().parse().ok(),
            ast::Constant::Str(text) => text.clone(),
            _ => return None,
        },
        other => {
            let full = expr_full_name(other)?;
            full.strip_prefix("logging.").unwrap_or(&full).to_string()
        }
    };
    match name.as_str() {
        "DEBUG" => Some(10),
        "INFO" => Some(20),
        "WARNING" | "WARN" => Some(30),
        "ERROR" => Some(40),
        "CRITICAL" | "FATAL" => Some(50),
        _ => None,
    }
}

/// A literal an owner call binds, as logging formats it.
enum Value {
    Int(String),
    Text(String),
}

fn argument_value(expr: &Expr, row: &CallRow) -> Option<Value> {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Int(number) => Some(Value::Int(number.to_string())),
            ast::Constant::Str(text) => Some(Value::Text(text.clone())),
            _ => None,
        },
        Expr::Name(name) => {
            let bound = &row.bindings.get(name.id.as_str())?.value;
            if let Some(text) = python_string_literal_value(bound) {
                return Some(Value::Text(text));
            }
            let digits = bound.strip_prefix('-').unwrap_or(bound);
            (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| Value::Int(bound.clone()))
        }
        _ => None,
    }
}

/// `format % arguments` as logging applies it: with no arguments the message
/// stays as written; otherwise `%s`, `%d`, `%i` and `%%` only.
fn format_message(format: &str, arguments: &[Value]) -> Option<String> {
    if arguments.is_empty() {
        return Some(format.to_string());
    }
    let mut out = String::new();
    let mut next = arguments.iter();
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next()? {
            '%' => out.push('%'),
            's' => match next.next()? {
                Value::Int(text) | Value::Text(text) => out.push_str(text),
            },
            'd' | 'i' => match next.next()? {
                Value::Int(text) => out.push_str(text),
                Value::Text(_) => return None,
            },
            _ => return None,
        }
    }
    next.next().is_none().then_some(out)
}

/// The message a call records for one owner call, or an empty list when the
/// call's level is below the capture level; None when it cannot be read.
fn emitted_text(call: &LogCall, row: &CallRow, capture_level: u32) -> Option<Vec<String>> {
    let arguments = call
        .arguments
        .iter()
        .map(|argument| argument_value(argument, row))
        .collect::<Option<Vec<_>>>()?;
    let message = format_message(&call.format, &arguments)?;
    Some(if call.level >= capture_level {
        vec![message]
    } else {
        Vec::new()
    })
}

/// The text must read from the message, not the record header. A header word
/// is upper-case letters, digits and spaces around `name:file:line`, so a
/// space that follows any other character cannot lie in or straddle it.
fn anchored_in_message(literal: &str) -> bool {
    if literal.contains('\n') {
        return false;
    }
    let bytes = literal.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        *byte == b' '
            && index > 0
            && !(bytes[index - 1].is_ascii_uppercase()
                || bytes[index - 1].is_ascii_digit()
                || bytes[index - 1] == b' ')
    })
}

/// How a test fixes the capture level.
struct LevelPin {
    level: u32,
    /// Absolute lines the pin covers: a `with` block, or everything after a
    /// `set_level` call.
    first: usize,
    last: usize,
}

impl LevelPin {
    fn covers(&self, line: usize) -> bool {
        self.first <= line && line <= self.last
    }
}

/// The one `caplog.at_level(<level>)` block or `caplog.set_level(<level>)`
/// call a test makes, when it makes no other call that could log or reshape
/// the captured records.
fn level_pin(test: &PythonTest, owner: &PythonOwner) -> Option<LevelPin> {
    let body = test.body_text.as_str();
    if CAPLOG_INTERFERENCE
        .iter()
        .any(|needle| body.contains(needle))
        || !body_calls_only_the_owner(body, test, owner)
    {
        return None;
    }
    let lines: Vec<&str> = body.lines().collect();
    let mut pins = lines.iter().enumerate().filter(|(_, line)| {
        let trimmed = line.trim();
        trimmed.contains("caplog.at_level(") || trimmed.contains("caplog.set_level(")
    });
    let (index, line) = pins.next()?;
    if pins.next().is_some() {
        return None;
    }
    let trimmed = line.trim();
    let block = trimmed.starts_with("with caplog.at_level(") && trimmed.ends_with("):");
    let call = trimmed.starts_with("caplog.set_level(") && trimmed.ends_with(')');
    let open = trimmed.find('(')? + 1;
    let close = if block {
        trimmed.len() - 2
    } else {
        trimmed.len() - 1
    };
    let argument = trimmed.get(open..close)?;
    let arguments = split_top_level_args(argument);
    if arguments.len() != 1 || arguments[0].contains('=') || !(block || call) {
        return None;
    }
    let level = level_from_expr(&parse_expression(arguments[0].trim())?)?;
    let first = test.line + index + 1;
    let last = if block {
        let indent = leading_whitespace(line);
        let mut end = index;
        for (offset, following) in lines.iter().enumerate().skip(index + 1) {
            if following.trim().is_empty() {
                continue;
            }
            if leading_whitespace(following) <= indent {
                break;
            }
            end = offset;
        }
        test.line + end
    } else {
        usize::MAX
    };
    Some(LevelPin { level, first, last })
}

/// Whether every call in the test body is the owner, the `caplog` level
/// control or the test's own header: another call may log the same text.
fn body_calls_only_the_owner(body: &str, test: &PythonTest, owner: &PythonOwner) -> bool {
    let bytes = body.as_bytes();
    let mut index = 0;
    while let Some(offset) = body[index..].find('(') {
        let at = index + offset;
        index = at + 1;
        let mut start = at;
        while start > 0
            && (bytes[start - 1].is_ascii_alphanumeric()
                || bytes[start - 1] == b'_'
                || bytes[start - 1] == b'.')
        {
            start -= 1;
        }
        let chain = &body[start..at];
        if chain.is_empty() {
            continue;
        }
        let allowed = chain == owner.name
            || chain.ends_with(&format!(".{}", owner.name))
            || chain == test.name
            || chain == "caplog.at_level"
            || chain == "caplog.set_level"
            || matches!(
                chain,
                "assert" | "not" | "and" | "or" | "in" | "is" | "if" | "with"
            );
        if !allowed {
            return false;
        }
    }
    true
}

/// The `if <condition>:` lines, outermost first, between the owner's `def`
/// and the changed line when they are the only code there; None otherwise.
fn plain_if_conditions(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed_index = changed_line.checked_sub(1)?;
    let header = lines.get(owner.start_line.checked_sub(1)?)?;
    if !header.trim_end().ends_with(':') || changed_index >= lines.len() {
        return None;
    }
    let mut conditions = Vec::new();
    let mut indent = leading_whitespace(header);
    for (index, raw) in lines
        .iter()
        .enumerate()
        .take(changed_index)
        .skip(owner.start_line)
    {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let condition = trimmed
            .strip_prefix("if ")?
            .trim_end()
            .strip_suffix(':')?
            .trim();
        if leading_whitespace(raw) <= indent {
            return None;
        }
        indent = leading_whitespace(raw);
        conditions.push((index, condition.to_string()));
    }
    if leading_whitespace(lines[changed_index]) <= indent {
        return None;
    }
    // Nothing after the changed line may call anything that could log.
    for raw in lines.iter().take(owner.end_line).skip(changed_index + 1) {
        let trimmed = raw.trim();
        if !(trimmed.is_empty()
            || (trimmed.starts_with("return") || trimmed.starts_with("pass"))
                && !trimmed.contains('('))
        {
            return None;
        }
    }
    Some(conditions)
}
