//! Assertions that observe a changed side effect through the channel it
//! writes, rather than through the owner's return value.
//!
//! A changed `print(...)`, `path.write_text(...)`, `notifier.send(...)` or
//! `self.total = ...` line changes what the owner leaves behind, not what it
//! returns, so an exact assertion on that channel names neither the owner nor
//! a changed token: `capsys.readouterr().out == "Total: 12.50\n"`,
//! `report.read_text() == "total=5\n"`, `notifier.send.assert_called_once_with(
//! "bob", 25)`, `account.balance == 150`. Each admission below needs the test
//! holding the assertion to call the owner and, for a value the test passes
//! in, to pass the very local the assertion reads.

use super::no_behavior::{call_arglists_with_offsets, split_top_level_args};
use super::related_tests::{
    is_python_identifier_char, owner_method_bound_locals, owner_module_callees,
    python_text_hides_code,
};
use super::{PythonOwner, PythonTest};
use crate::domain::OwnerKind;

/// The side effect a changed line writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChangedEffect {
    /// `print(...)` to standard output, or `sys.stdout.write(...)`.
    Stdout,
    /// `print(..., file=sys.stderr)` or `sys.stderr.write(...)`.
    Stderr,
    /// `param.write_text(...)` / `param.write_bytes(...)` on an owner parameter.
    FileWrite { receiver: String },
    /// `param.method(...)` on an owner parameter, observable through a mock.
    CallOnParameter { receiver: String, method: String },
    /// `self.attr = ...` (or an augmented assignment) in a method owner.
    SelfField { attr: String },
}

impl ChangedEffect {
    /// The changed line's effect, when it is one of the observable shapes.
    pub(super) fn of(line_text: &str, owner: &PythonOwner) -> Option<Self> {
        let line = line_text.trim();
        if let Some(attr) = self_attribute_write(line)
            && matches!(owner.owner_kind, Some(OwnerKind::Method))
        {
            return Some(Self::SelfField {
                attr: attr.to_string(),
            });
        }
        let (callee, args) = statement_call(line)?;
        match callee {
            "print" => {
                let file = split_top_level_args(args).into_iter().find_map(|arg| {
                    let (name, value) = arg.split_once('=')?;
                    (name.trim() == "file").then(|| value.trim().to_string())
                });
                match file.as_deref() {
                    None | Some("sys.stdout") => Some(Self::Stdout),
                    Some("sys.stderr") => Some(Self::Stderr),
                    Some(_) => None,
                }
            }
            "sys.stdout.write" => Some(Self::Stdout),
            "sys.stderr.write" => Some(Self::Stderr),
            _ => {
                let (receiver, method) = callee.split_once('.')?;
                if method.contains('.')
                    || !owner
                        .parameters
                        .iter()
                        .any(|parameter| parameter.name == receiver)
                    || matches!(receiver, "self" | "cls")
                {
                    return None;
                }
                let receiver = receiver.to_string();
                Some(match method {
                    "write_text" | "write_bytes" => Self::FileWrite { receiver },
                    _ => Self::CallOnParameter {
                        receiver,
                        method: method.to_string(),
                    },
                })
            }
        }
    }

    /// Whether a mock assertion can be the observer: only a call made on a
    /// parameter the test may substitute.
    pub(super) fn admits_mock_assertion(&self) -> bool {
        matches!(self, Self::CallOnParameter { .. })
    }
}

/// What one test can read of a changed effect: the compared operands that
/// reach its channel and the mock methods it may pin. Built once per related
/// test, so each of its assertions is checked against it cheaply.
#[derive(Debug, Default)]
pub(super) struct EffectReads {
    operands: Vec<String>,
    mocked_calls: Vec<String>,
}

impl EffectReads {
    pub(super) fn of(effect: &ChangedEffect, test: &PythonTest, owner: &PythonOwner) -> Self {
        let mut reads = Self::default();
        match effect {
            ChangedEffect::Stdout => reads.operands = captured_stream_operands(test, owner, "out"),
            ChangedEffect::Stderr => reads.operands = captured_stream_operands(test, owner, "err"),
            ChangedEffect::FileWrite { receiver } => {
                for local in owner_call_argument_locals(test, owner, receiver) {
                    reads.operands.push(format!("{local}.read_text()"));
                    reads.operands.push(format!("{local}.read_bytes()"));
                }
            }
            ChangedEffect::CallOnParameter { receiver, method } => {
                reads.mocked_calls = owner_call_argument_locals(test, owner, receiver)
                    .into_iter()
                    .map(|local| format!("{local}.{method}"))
                    .collect();
            }
            ChangedEffect::SelfField { attr } => {
                if let Some((class, _)) = owner.qualified_name.rsplit_once('.') {
                    reads.operands = owner_method_bound_locals(test, owner, class, &owner.name)
                        .into_iter()
                        .map(|local| format!("{local}.{attr}"))
                        .collect();
                }
            }
        }
        reads
    }

    /// Whether one assertion (its gate text) observes the effect.
    pub(super) fn observed_by(&self, assertion: &str) -> bool {
        if !self.operands.is_empty() {
            let compared = compared_operands(assertion);
            if self
                .operands
                .iter()
                .any(|operand| compared.contains(&operand.as_str()))
            {
                return true;
            }
        }
        self.mocked_calls
            .iter()
            .any(|mocked| pins_mock_call(assertion, mocked))
    }
}

/// `self.attr = value` or `self.attr op= value`: the written attribute.
fn self_attribute_write(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("self.")?;
    let attr_len = rest
        .find(|ch: char| !is_python_identifier_char(ch))
        .unwrap_or(rest.len());
    let attr = &rest[..attr_len];
    let after = rest[attr_len..].trim_start();
    let operator_len = after.find('=')?;
    let operator = &after[..operator_len];
    let augmented = matches!(
        operator,
        "" | "+" | "-" | "*" | "/" | "//" | "%" | "**" | "|" | "&" | "^" | "<<" | ">>" | "@"
    );
    (!attr.is_empty()
        && !attr.starts_with(|ch: char| ch.is_ascii_digit())
        && augmented
        && !after[operator_len + 1..].starts_with('='))
    .then_some(attr)
}

/// A line that is one call statement `callee(args)`: the dotted callee and the
/// argument text between its parentheses.
fn statement_call(line: &str) -> Option<(&str, &str)> {
    let open = line.find('(')?;
    let callee = line[..open].trim_end();
    if callee.is_empty()
        || !callee
            .chars()
            .all(|ch| ch == '.' || is_python_identifier_char(ch))
        || callee.starts_with('.')
        || callee.ends_with('.')
    {
        return None;
    }
    let close = super::no_behavior::matching_call_paren(line, open)?;
    line[close + 1..]
        .trim()
        .is_empty()
        .then(|| (callee, &line[open + 1..close]))
}

/// The owner calls the test makes, as (offset in the body, argument text): by
/// its own name, an import alias, or a module-qualified spelling. A method is
/// called only on a local bound to its class (rule 9), so a same-named method
/// of another class does not count.
fn owner_calls<'a>(test: &'a PythonTest, owner: &PythonOwner) -> Vec<(usize, &'a str)> {
    let mut names = Vec::new();
    if is_method_owner(owner) {
        if let Some((class, _)) = owner.qualified_name.rsplit_once('.') {
            names.extend(
                owner_method_bound_locals(test, owner, class, &owner.name)
                    .into_iter()
                    .map(|local| format!("{local}.{}", owner.name)),
            );
        }
    } else {
        names.push(owner.name.clone());
        names.extend(
            test.imports
                .iter()
                .filter(|import| import.imported == owner.name)
                .map(|import| import.alias.clone()),
        );
        names.extend(owner_module_callees(test, owner));
    }
    names.sort();
    names.dedup();
    let mut calls: Vec<(usize, &str)> = names
        .iter()
        .flat_map(|name| call_arglists_with_offsets(&test.body_text, name, false))
        .collect();
    calls.sort_unstable();
    calls
}

/// Whether the test calls a free-function owner through a module-identified
/// spelling (`pricing.print_receipt(...)`), which names the owner's module
/// as an import of it would.
pub(super) fn calls_owner_through_module(test: &PythonTest, owner: &PythonOwner) -> bool {
    !is_method_owner(owner)
        && owner_module_callees(test, owner)
            .iter()
            .any(|name| !call_arglists_with_offsets(&test.body_text, name, false).is_empty())
}

fn is_method_owner(owner: &PythonOwner) -> bool {
    matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    )
}

/// Bare locals the test passes, in owner calls, as the owner parameter
/// `parameter`: by position after any implicit receiver, or by keyword. A
/// local passed for another parameter is a different collaborator.
fn owner_call_argument_locals(
    test: &PythonTest,
    owner: &PythonOwner,
    parameter: &str,
) -> Vec<String> {
    let skip = usize::from(
        is_method_owner(owner)
            && owner
                .parameters
                .first()
                .is_some_and(|first| matches!(first.name.as_str(), "self" | "cls")),
    );
    let position = owner
        .parameters
        .iter()
        .filter(|candidate| !candidate.keyword_only)
        .skip(skip)
        .position(|candidate| candidate.name == parameter && !candidate.keyword_only);
    let mut locals: Vec<String> = owner_calls(test, owner)
        .into_iter()
        .filter_map(|(_, arglist)| {
            let mut positional = 0usize;
            for argument in split_top_level_args(arglist) {
                let argument = argument.trim();
                if argument.starts_with('*') {
                    return None;
                }
                match argument.split_once('=').filter(|(name, value)| {
                    super::static_limits::is_simple_python_identifier(name.trim())
                        && !value.starts_with('=')
                }) {
                    Some((name, value)) => {
                        if name.trim() == parameter {
                            return Some(value.trim());
                        }
                    }
                    None => {
                        if Some(positional) == position {
                            return Some(argument);
                        }
                        positional += 1;
                    }
                }
            }
            None
        })
        .filter(|local| super::static_limits::is_simple_python_identifier(local))
        .map(str::to_string)
        .collect();
    locals.sort();
    locals.dedup();
    locals
}

/// The compared operands that read the captured `stream` (`out` or `err`)
/// of `capsys`/`capfd` after the test calls the owner: inline
/// (`capsys.readouterr().out`) or through a local the test binds once to
/// `readouterr()` after an owner call. A capture taken before the owner runs
/// holds none of its output.
fn captured_stream_operands(test: &PythonTest, owner: &PythonOwner, stream: &str) -> Vec<String> {
    let captures: Vec<&str> = ["capsys", "capfd", "capsysbinary", "capfdbinary"]
        .into_iter()
        .filter(|capture| test.fixtures.iter().any(|fixture| fixture == capture))
        .collect();
    if captures.is_empty() {
        return Vec::new();
    }
    let Some(first_call) = owner_calls(test, owner).first().map(|(offset, _)| *offset) else {
        return Vec::new();
    };
    let mut operands: Vec<String> = captures
        .iter()
        .map(|capture| format!("{capture}.readouterr().{stream}"))
        .collect();
    let mut offset = 0usize;
    for line in test.body_text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let Some((target, value)) = line.split_once('=') else {
            continue;
        };
        let target = target.trim();
        if line_start > first_call
            && super::static_limits::is_simple_python_identifier(target)
            && captures
                .iter()
                .any(|capture| value.trim() == format!("{capture}.readouterr()"))
            && test.body_text.matches(&format!("{target} =")).count() == 1
        {
            operands.push(format!("{target}.{stream}"));
        }
    }
    operands
}

pub(super) fn compared_operands(assertion: &str) -> Vec<&str> {
    if let Some(expr) = assertion.strip_prefix("assert ") {
        return split_top_level(expr, " and ")
            .into_iter()
            .flat_map(|conjunct| {
                let operands = split_top_level(conjunct, "==");
                if operands.len() >= 2 {
                    operands
                } else {
                    Vec::new()
                }
            })
            .map(str::trim)
            .collect();
    }
    let Some(open) = assertion.find('(') else {
        return Vec::new();
    };
    let callee = &assertion[..open];
    if !(callee.ends_with("assertEqual") || callee.ends_with("assertDictEqual")) {
        return Vec::new();
    }
    let Some(inner) = assertion[open + 1..].strip_suffix(')') else {
        return Vec::new();
    };
    split_top_level_args(inner)
        .into_iter()
        .take(2)
        .map(str::trim)
        .collect()
}

/// Split `text` on `separator` where it occurs outside brackets and strings.
fn split_top_level<'a>(text: &'a str, separator: &str) -> Vec<&'a str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut start = 0usize;
    let mut idx = 0usize;
    while idx < bytes.len() {
        let byte = bytes[idx];
        match quote {
            Some(open) => {
                if byte == b'\\' {
                    idx += 1;
                } else if byte == open {
                    quote = None;
                }
            }
            None => match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                _ if depth == 0 && bytes[idx..].starts_with(separator.as_bytes()) => {
                    parts.push(&text[start..idx]);
                    idx += separator.len();
                    start = idx;
                    continue;
                }
                _ => {}
            },
        }
        idx += 1;
    }
    parts.push(&text[start..]);
    parts
}

/// Live, identifier-bounded occurrences of `needle`, as (before, after) text.
fn operand_occurrences<'a>(
    text: &'a str,
    needle: &'a str,
) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
    text.match_indices(needle).filter_map(move |(idx, _)| {
        let end = idx + needle.len();
        let bounded_before = !text[..idx]
            .chars()
            .next_back()
            .is_some_and(|ch| ch == '.' || is_python_identifier_char(ch));
        (bounded_before && !python_text_hides_code(text, idx)).then(|| (&text[..idx], &text[end..]))
    })
}

/// Whether a mock assertion pins the arguments of `mocked.method`:
/// `mocked.method.assert_called_once_with(...)` with at least one argument,
/// every one pinned. An `ANY` matcher or a starred argument accepts whatever
/// the changed call passes, so it pins nothing. `assert_called_with` reads only
/// the last call and `assert_any_call` any call, so either can be satisfied by
/// another call the owner makes on the same method; `assert_called_once_with`
/// also fails when the owner makes a second call.
fn pins_mock_call(assertion: &str, mocked: &str) -> bool {
    let call = format!("{mocked}.assert_called_once_with(");
    operand_occurrences(assertion, &call).any(|(before, after)| {
        let reopened = format!("({after}");
        before.trim().is_empty()
            && super::no_behavior::matching_call_paren(&reopened, 0)
                .is_some_and(|close| all_arguments_pinned(&reopened[1..close]))
    })
}

fn all_arguments_pinned(arguments: &str) -> bool {
    let arguments = split_top_level_args(arguments);
    !arguments.is_empty()
        && arguments.iter().all(|argument| {
            let value = argument
                .split_once('=')
                .filter(|(name, _)| super::static_limits::is_simple_python_identifier(name.trim()))
                .map_or(*argument, |(_, value)| value)
                .trim();
            !value.is_empty()
                && !value.starts_with('*')
                && !matches!(value, "ANY" | "mock.ANY" | "unittest.mock.ANY")
        })
}

/// Whether the assertion computes its expected value through the owner
/// (RIPR-SPEC-0035 "self-computed expected value"): every compared operand
/// that names the owner is a computed expression (`1000 + tax(1000)`), and
/// another operand is a call to something else (`total(1000)`). The owner then
/// serves as the reference, not the observed value, so its change can move
/// both sides at once.
pub(super) fn expected_computed_through_owner(assertion: &str, owner_tokens: &[String]) -> bool {
    let operands = compared_operands(assertion);
    let names_owner = |operand: &str| {
        owner_tokens.iter().any(|token| {
            operand_occurrences(operand, token)
                .any(|(_, after)| !after.chars().next().is_some_and(is_python_identifier_char))
        })
    };
    let owner_operands: Vec<&str> = operands
        .iter()
        .copied()
        .filter(|op| names_owner(op))
        .collect();
    !owner_operands.is_empty()
        && owner_operands.iter().all(|op| has_top_level_arithmetic(op))
        && operands
            .iter()
            .any(|op| !names_owner(op) && is_whole_call(op))
}

/// An operand with a binary arithmetic operator outside brackets and strings;
/// a leading sign (`-x`) is unary, not binary.
fn has_top_level_arithmetic(operand: &str) -> bool {
    let trimmed = operand.trim();
    ["+", "-", "*", "/", "%"].iter().any(|operator| {
        let parts = split_top_level(trimmed, operator);
        parts.len() > 1 && !parts[0].trim().is_empty()
    })
}

/// An operand that is one call `name(...)` / `a.b(...)`, nothing around it.
fn is_whole_call(operand: &str) -> bool {
    let operand = operand.trim();
    let Some(open) = operand.find('(') else {
        return false;
    };
    let callee = &operand[..open];
    !callee.is_empty()
        && callee
            .chars()
            .all(|ch| ch == '.' || is_python_identifier_char(ch))
        && super::no_behavior::matching_call_paren(operand, open)
            .is_some_and(|close| close + 1 == operand.len())
}
