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
use super::static_limits::{CaplogRead, caplog_required_literals};
use super::{PythonOwner, PythonTest, expr_full_name};
use crate::domain::{OwnerKind, ValueContext};
use crate::value_origins::LocatedValueFact;
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
    pub(super) observed_values: Vec<LocatedValueFact>,
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
    if old.receiver != new.receiver || !receiver_reaches_root_capture(&old.receiver, owner_source) {
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
        let rows = owner_call_rows_for_candidate(owner, &constants, candidate)?;
        // A read through a local alias is the same read only where the alias
        // is a plain snapshot taken after every owner call (#10151).
        if rows.iter().any(|row| row.file != candidate.test.file) {
            return None;
        }
        let gate = snapshot_alias_gate(
            candidate.test,
            assertion.line,
            &assertion.gate_text,
            rows.iter().map(|row| row.line),
        )?;
        let literals = caplog_required_literals(&gate);
        if literals.is_empty() {
            // Another assertion may catch the edit too; this reading only
            // proves what the caplog assertions say.
            return None;
        }
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
                |(parameter, binding)| LocatedValueFact {
                    file: Some(row.file.clone()),
                    line: row.line,
                    text: row.text.clone(),
                    value: format!("{parameter} = {}", binding.value),
                    context: ValueContext::FunctionArgument,
                },
            ));
        }
        for membership in literals {
            let literal = membership.literal;
            // Only `caplog.text` carries the record header; a message read
            // (`caplog.messages`, `record.message`) holds the message alone.
            if membership.read == CaplogRead::Text && !anchored_in_message(&literal) {
                return None;
            }
            let holds = |text: &String| match membership.read {
                CaplogRead::Text | CaplogRead::MessageContains => text.contains(&literal),
                CaplogRead::MessageEquals => *text == literal,
            };
            // An absence assertion (`not in`) passes while nothing matches.
            let negated = membership.negated;
            let old_matches = old_texts.iter().any(holds);
            let new_matches = new_texts.iter().any(holds);
            // For an absence assertion the interesting text is the one that
            // now matches.
            let new_text = if negated {
                new_texts.iter().find(|text| holds(text)).cloned()
            } else {
                new_texts.first().cloned()
            };
            evaluations.push(Evaluation {
                old_passes: old_matches != negated,
                new_passes: new_matches != negated,
                negated,
                literal,
                old_text: old_texts.first().cloned(),
                new_text,
            });
        }
    }
    // A test that did not already see the text before the edit says nothing
    // about it afterwards.
    if evaluations.is_empty() || evaluations.iter().any(|evaluation| !evaluation.old_passes) {
        return None;
    }
    observed_values.sort_by(|a, b| a.line.cmp(&b.line).then(a.value.cmp(&b.value)));
    observed_values.dedup();
    if let Some(gone) = evaluations.iter().find(|evaluation| !evaluation.new_passes) {
        if gone.negated {
            return Some(PythonLogEvidence {
                verdict: PythonLogVerdict::Rejects,
                discriminator: String::new(),
                observed_values,
                reason: format!(
                    "the text `{}` the test asserts absent is not in the old logged message and is in the new one: the new message reads `{}`",
                    gone.literal,
                    gone.new_text.as_deref().unwrap_or_default(),
                ),
            });
        }
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
    if kept.negated {
        return Some(PythonLogEvidence {
            verdict: PythonLogVerdict::Absorbs,
            discriminator: "an asserted-absent text the edit adds to the logged message"
                .to_string(),
            observed_values,
            reason: format!(
                "the text `{}` the test asserts absent is in neither the old nor the new logged message, so the assertion passes either way",
                kept.literal
            ),
        });
    }
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
    /// Whether the assertion passes against the old and the new logged text.
    old_passes: bool,
    new_passes: bool,
    negated: bool,
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
    // `exception` appends the active traceback to the record, which this
    // reading does not evaluate, so asserted text may survive the edit.
    if level_name == "exception" {
        return None;
    }
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

/// Whether the receiver is the root `logging` module or a module-level name
/// bound to `logging.getLogger(...)`, whose records propagate to the root
/// handler `caplog` installs. A detached `logging.Logger(...)` or any other
/// object named `warning`-capable does not establish that path.
fn receiver_reaches_root_capture(receiver: &str, owner_source: &str) -> bool {
    if receiver == "logging" {
        return true;
    }
    let prefix = format!("{receiver} = logging.getLogger(");
    owner_source
        .lines()
        .any(|line| line.starts_with(&prefix) && line.trim_end().ends_with(')'))
}

/// Whether the test rebinds `caplog` (assignment, walrus, `as`, `for`,
/// `del`, `global`/`nonlocal`) so the level pin and the asserted
/// `caplog.text` may name different objects.
fn rebinds_caplog(body: &str) -> bool {
    let mut from = 0;
    while let Some(offset) = body[from..].find("caplog") {
        let at = from + offset;
        from = at + "caplog".len();
        let before = body[..at].chars().next_back();
        if before.is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || ch == '.') {
            continue;
        }
        let rest = body[from..]
            .trim_start_matches(|ch: char| ch.is_alphanumeric() || ch == '_' || ch == '.');
        let rest = rest.trim_start_matches([' ', '\t']);
        let assigned = rest.starts_with(":=")
            || (rest.starts_with('=') && !rest.starts_with("=="))
            || ["+=", "-=", "*=", "/=", "|=", "&="]
                .iter()
                .any(|op| rest.starts_with(op));
        let line_start = body[..at].rfind('\n').map_or(0, |index| index + 1);
        let head = body[line_start..at].trim_end();
        let bound = ["as", "for", "del", "global", "nonlocal"]
            .iter()
            .any(|word| head.ends_with(word) && head.len() >= word.len());
        let comma_bound = head.ends_with(',') && rest.starts_with('=');
        if assigned || bound || comma_bound {
            return true;
        }
    }
    false
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
        || rebinds_caplog(body)
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
            // `any(...)` over `caplog.records` and `record.getMessage()` read
            // captured records; neither logs.
            || chain == "any"
            || chain.ends_with(".getMessage")
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

/// The captured-log reads a test can bind to a local name.
const CAPLOG_READS: [&str; 3] = ["caplog.text", "caplog.messages", "caplog.records"];

struct CaplogAlias<'a> {
    name: &'a str,
    read: &'static str,
    /// Index of the `<name> = caplog.<read>` line in the test body.
    index: usize,
}

/// `<name> = caplog.text|messages|records` lines of a test body.
fn caplog_aliases(body: &str) -> Vec<CaplogAlias<'_>> {
    body.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let (name, value) = line.trim().split_once('=')?;
            // A trailing comment (`# type: ignore`) is not part of the value.
            let value = value.split('#').next().unwrap_or_default();
            let (name, value) = (name.trim(), value.trim());
            let identifier = !name.is_empty()
                && !name.starts_with(|ch: char| ch.is_ascii_digit())
                && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_');
            let read = CAPLOG_READS.iter().find(|read| **read == value)?;
            (identifier && !value.starts_with('=')).then_some(CaplogAlias { name, read, index })
        })
        .collect()
}

/// Where a left-to-right scan of Python source stands relative to string
/// literals, including triple-quoted ones (`"""a "b" c"""`), which a plain
/// quote toggle reads as alternating open and closed strings.
#[derive(Default)]
struct StringScan {
    /// The open string's quote and whether it is triple-quoted.
    open: Option<(char, bool)>,
    escaped: bool,
}

impl StringScan {
    /// Consume the string character at the start of `rest` when the scan is
    /// inside a string or `rest` opens one; the bytes consumed, else 0 (plain
    /// code, left to the caller).
    fn step(&mut self, rest: &str) -> usize {
        let Some(ch) = rest.chars().next() else {
            return 0;
        };
        let triple = |quote: char| rest.starts_with(&quote.to_string().repeat(3));
        match self.open {
            Some((quote, is_triple)) => {
                if self.escaped {
                    self.escaped = false;
                } else if ch == '\\' {
                    self.escaped = true;
                } else if ch == quote && (!is_triple || triple(quote)) {
                    self.open = None;
                    return if is_triple { 3 } else { 1 };
                }
                ch.len_utf8()
            }
            None if ch == '\'' || ch == '"' => {
                let is_triple = triple(ch);
                self.open = Some((ch, is_triple));
                if is_triple { 3 } else { 1 }
            }
            None => 0,
        }
    }
}

/// `text` with each whole-word `name` outside a string literal replaced by its
/// read, in one pass: an alias spelled inside a quoted literal is data, not the
/// alias, and a read inserted for one alias (`caplog.text`) is never rescanned
/// as another alias named `text`. An attribute (`caplog.text`) is never a local.
pub(super) fn replace_identifiers(text: &str, replacements: &[(&str, &str)]) -> String {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mut out = String::with_capacity(text.len());
    let mut scan = StringScan::default();
    let mut index = 0;
    while index < text.len() {
        let consumed = scan.step(&text[index..]);
        if consumed > 0 {
            out.push_str(&text[index..index + consumed]);
            index += consumed;
            continue;
        }
        if let Some((name, with)) = replacements.iter().find(|(name, _)| {
            text[index..].starts_with(name)
                && !text[..index].trim_end().ends_with('.')
                && !text[..index].chars().next_back().is_some_and(is_word)
                && !text[index + name.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_word)
        }) {
            out.push_str(with);
            index += name.len();
            continue;
        }
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

/// The assertion with every local alias of a captured-log read written as the
/// read itself. Only for deciding whether the assertion could be about the
/// changed message: nothing shows the alias is a snapshot of it.
pub(super) fn substitute_caplog_aliases(test: &PythonTest, gate_text: &str) -> String {
    let aliases = caplog_aliases(&test.body_text);
    let replacements: Vec<_> = aliases.iter().map(|a| (a.name, a.read)).collect();
    replace_identifiers(gate_text, &replacements)
}

/// Lines of the statement starting at `start`: through the line that closes
/// its brackets, so `assert (` / `"..." in msgs` / `)` is one assertion.
fn statement_lines(lines: &[&str], start: usize) -> std::ops::Range<usize> {
    let mut depth = 0i32;
    let mut scan = StringScan::default();
    for (offset, line) in lines.iter().skip(start).enumerate() {
        let mut in_comment = false;
        let mut index = 0;
        while index < line.len() {
            let consumed = scan.step(&line[index..]);
            if consumed > 0 {
                index += consumed;
                continue;
            }
            let Some(ch) = line[index..].chars().next() else {
                break;
            };
            match ch {
                '#' => {
                    in_comment = true;
                    break;
                }
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
            index += ch.len_utf8();
        }
        // A backslash ending a comment is comment text, not a continuation.
        let continued = !in_comment && line.trim_end().ends_with('\\');
        if depth <= 0 && scan.open.is_none() && !continued {
            return start..start + offset + 1;
        }
    }
    start..lines.len()
}

/// Whether `text` uses the local name `name`: a whole word that is not an
/// attribute (`caplog.text` does not use a local called `text`).
fn mentions_local_name(text: &str, name: &str) -> bool {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    text.match_indices(name).any(|(start, _)| {
        let before = text[..start].trim_end();
        !before.ends_with('.')
            && !text[..start].chars().next_back().is_some_and(is_word)
            && !text[start + name.len()..]
                .chars()
                .next()
                .is_some_and(is_word)
    })
}

/// The assertion text with its aliases of captured-log reads replaced by the
/// reads, or None when an alias is not provably a plain snapshot: the name is
/// bound once, to the read, at the assertion's own indentation after every
/// owner call, and appears nowhere else in the test (a mutation, rebinding,
/// deletion, nested use or other read keeps the abstention). `caplog.text`,
/// `.messages` and `.records` are rebuilt on each access, so an alias taken
/// before the call would hold the earlier records.
fn snapshot_alias_gate(
    test: &PythonTest,
    assertion_line: usize,
    gate_text: &str,
    owner_call_lines: impl Iterator<Item = usize>,
) -> Option<String> {
    let aliases = caplog_aliases(&test.body_text);
    let mentioned: Vec<_> = aliases
        .iter()
        .filter(|alias| mentions_local_name(gate_text, alias.name))
        .collect();
    if mentioned.is_empty() {
        return Some(gate_text.to_string());
    }
    let lines: Vec<&str> = test.body_text.lines().collect();
    let assertion_index = assertion_line.checked_sub(test.line)?;
    let assertion_lines = statement_lines(&lines, assertion_index);
    let last_call = owner_call_lines.max()?;
    let mut replacements = Vec::new();
    for alias in mentioned {
        let bound_once = aliases
            .iter()
            .filter(|other| other.name == alias.name)
            .count()
            == 1;
        let only_uses = lines.iter().enumerate().all(|(index, line)| {
            !mentions_local_name(line, alias.name)
                || index == alias.index
                || assertion_lines.contains(&index)
        });
        let indent = leading_whitespace(lines.get(alias.index)?);
        let level = lines
            .get(assertion_index)
            .is_some_and(|line| leading_whitespace(line) == indent);
        if !(bound_once && only_uses && level && test.line + alias.index > last_call) {
            return None;
        }
        replacements.push((alias.name, alias.read));
    }
    Some(replace_identifiers(gate_text, &replacements))
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
