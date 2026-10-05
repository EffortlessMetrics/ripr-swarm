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

/// Whether one assertion of `test` observes `effect` written by `owner`.
pub(super) fn assertion_observes_effect(
    effect: &ChangedEffect,
    assertion: &str,
    test: &PythonTest,
    owner: &PythonOwner,
) -> bool {
    match effect {
        ChangedEffect::Stdout => {
            calls_owner(test, owner) && reads_captured_stream(assertion, test, "out")
        }
        ChangedEffect::Stderr => {
            calls_owner(test, owner) && reads_captured_stream(assertion, test, "err")
        }
        ChangedEffect::FileWrite { .. } => {
            owner_call_argument_locals(test, owner).iter().any(|local| {
                compared_operand_is(assertion, &format!("{local}.read_text()"))
                    || compared_operand_is(assertion, &format!("{local}.read_bytes()"))
            })
        }
        ChangedEffect::CallOnParameter { method, .. } => owner_call_argument_locals(test, owner)
            .iter()
            .any(|local| pins_mock_call(assertion, &format!("{local}.{method}"))),
        ChangedEffect::SelfField { attr } => {
            let Some((class, _)) = owner.qualified_name.rsplit_once('.') else {
                return false;
            };
            owner_method_bound_locals(test, owner, class, &owner.name)
                .iter()
                .any(|local| compared_operand_is(assertion, &format!("{local}.{attr}")))
        }
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

/// The names the test calls the owner by: its own name, an import alias, or a
/// module-qualified spelling; a method only through its attribute name.
fn owner_calls<'a>(test: &'a PythonTest, owner: &PythonOwner) -> Vec<&'a str> {
    let method_call = matches!(
        owner.owner_kind,
        Some(OwnerKind::Method | OwnerKind::ClassMethod)
    );
    let mut names = vec![owner.name.clone()];
    if !method_call {
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
    names
        .iter()
        .flat_map(|name| call_arglists_with_offsets(&test.body_text, name, method_call))
        .map(|(_, arglist)| arglist)
        .collect()
}

fn calls_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    !owner_calls(test, owner).is_empty()
}

/// Bare local names the test passes as a whole argument to an owner call.
fn owner_call_argument_locals(test: &PythonTest, owner: &PythonOwner) -> Vec<String> {
    let mut locals: Vec<String> = owner_calls(test, owner)
        .into_iter()
        .flat_map(split_top_level_args)
        .map(|arg| {
            arg.split_once('=')
                .filter(|(name, _)| super::static_limits::is_simple_python_identifier(name.trim()))
                .map_or(arg, |(_, value)| value)
                .trim()
        })
        .filter(|arg| super::static_limits::is_simple_python_identifier(arg))
        .map(str::to_string)
        .collect();
    locals.sort();
    locals.dedup();
    locals
}

/// Whether an assertion compares the captured `stream` (`out` or `err`) of
/// `capsys`/`capfd`: inline (`capsys.readouterr().out == ...`) or through a
/// local the test binds once (`captured = capsys.readouterr()`).
fn reads_captured_stream(assertion: &str, test: &PythonTest, stream: &str) -> bool {
    let captures = ["capsys", "capfd", "capsysbinary", "capfdbinary"];
    if captures.iter().any(|capture| {
        test.fixtures.iter().any(|fixture| fixture == capture)
            && compared_operand_is(assertion, &format!("{capture}.readouterr().{stream}"))
    }) {
        return true;
    }
    test.body_text.lines().any(|line| {
        let Some((target, value)) = line.split_once('=') else {
            return false;
        };
        let target = target.trim();
        super::static_limits::is_simple_python_identifier(target)
            && captures.iter().any(|capture| {
                test.fixtures.iter().any(|fixture| fixture == capture)
                    && value.trim() == format!("{capture}.readouterr()")
            })
            && test.body_text.matches(&format!("{target} =")).count() == 1
            && compared_operand_is(assertion, &format!("{target}.{stream}"))
    })
}

/// Whether `operand` is a whole compared operand of the assertion's `==`
/// (`assert OPERAND == x`, `assert x == OPERAND`, one conjunct of an `and`,
/// or `assertEqual(OPERAND, x)`). Reads the assertion's gate text, whose shape
/// is `assert <expr>` or `callee(first, second)`.
fn compared_operand_is(assertion: &str, operand: &str) -> bool {
    compared_operands(assertion).contains(&operand)
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
                _ if depth == 0 && text[idx..].starts_with(separator) => {
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
/// `mocked.method.assert_called_once_with(...)`, `.assert_called_with(...)` or
/// `.assert_any_call(...)` with at least one argument, every one pinned. An
/// `ANY` matcher or a starred argument accepts whatever the changed call
/// passes, so it pins nothing.
fn pins_mock_call(assertion: &str, mocked: &str) -> bool {
    [
        "assert_called_once_with",
        "assert_called_with",
        "assert_any_call",
    ]
    .iter()
    .any(|check| {
        let call = format!("{mocked}.{check}(");
        operand_occurrences(assertion, &call).any(|(before, after)| {
            let reopened = format!("({after}");
            before.trim().is_empty()
                && super::no_behavior::matching_call_paren(&reopened, 0)
                    .is_some_and(|close| all_arguments_pinned(&reopened[1..close]))
        })
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
