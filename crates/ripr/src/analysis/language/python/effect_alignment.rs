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
    is_python_identifier_char, owner_class_locals, owner_method_bound_locals, owner_module_callees,
    owner_result_locals, python_text_hides_code,
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
    /// `param.method(...)`, or `param(...)` with an empty `method`, on an
    /// owner parameter, observable through a mock. `pinnable_rewrite`
    /// records whether the argument rewrite is one a pinned expectation
    /// reads as a different call (`arguments_rewritten_visibly`).
    CallOnParameter {
        receiver: String,
        method: String,
        pinnable_rewrite: bool,
    },
    /// `self.attr = ...` (or an augmented assignment) in a method owner.
    SelfField { attr: String },
}

impl ChangedEffect {
    /// The changed line's effect, when it is one of the observable shapes.
    /// `old_line_text` is the line it replaced, when there is one.
    pub(super) fn of(
        line_text: &str,
        old_line_text: Option<&str>,
        owner: &PythonOwner,
    ) -> Option<Self> {
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
                // `param(...)` calls the parameter itself (RIPR-SPEC-0238
                // rule 17, #7586): a mock passed for it records the call.
                let (receiver, method) = callee.split_once('.').unwrap_or((callee, ""));
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
                        // No old line: the call is new. An old line that is
                        // not a call statement (a comment, an assignment)
                        // cannot be compared, so it fails closed. A call to
                        // another callee never made the pinned call.
                        pinnable_rewrite: match old_line_text {
                            None => true,
                            Some(old) => {
                                statement_call(old.trim()).is_some_and(|(old_callee, old_args)| {
                                    old_callee != callee
                                        || arguments_rewritten_visibly(old_args, args)
                                })
                            }
                        },
                    },
                })
            }
        }
    }

    /// Whether a mock assertion can be the observer: only a call made on a
    /// parameter the test may substitute, whose arguments the change alters
    /// for every input. A rewrite such as `abs(n)` may hand the mock the old
    /// value at the test's input, so the pinned expectation still passes.
    pub(super) fn admits_mock_assertion(&self) -> bool {
        matches!(
            self,
            Self::CallOnParameter {
                pinnable_rewrite: true,
                ..
            }
        )
    }
}

/// What one test can read of a changed effect: the compared operands that
/// reach its channel and the mock methods it may pin. Built once per related
/// test, so each of its assertions is checked against it cheaply.
#[derive(Debug, Default)]
pub(super) struct EffectReads {
    operands: Vec<String>,
    mocked_calls: Vec<String>,
    /// The file line of the test's first owner call. A read on an earlier
    /// line sees the state before the changed effect ran, so it observes
    /// nothing of it.
    first_call_line: Option<usize>,
}

impl EffectReads {
    pub(super) fn of(effect: &ChangedEffect, test: &PythonTest, owner: &PythonOwner) -> Self {
        let mut reads = Self {
            first_call_line: owner_calls(test, owner)
                .first()
                .map(|(offset, _)| test.line + test.body_text[..*offset].matches('\n').count()),
            ..Self::default()
        };
        match effect {
            ChangedEffect::Stdout => reads.operands = captured_stream_operands(test, owner, "out"),
            ChangedEffect::Stderr => reads.operands = captured_stream_operands(test, owner, "err"),
            ChangedEffect::FileWrite { receiver } => {
                for local in owner_call_argument_locals(test, owner, receiver) {
                    reads.operands.push(format!("{local}.read_text()"));
                    reads.operands.push(format!("{local}.read_bytes()"));
                }
            }
            ChangedEffect::CallOnParameter {
                receiver,
                method,
                pinnable_rewrite: true,
            } => {
                let mut mocks = owner_call_argument_locals(test, owner, receiver);
                // An owner that hands the parameter back makes its result the
                // very object the test passed: `m = on_parse('1', Mock())`.
                if owner.term_facts.returned_parameter() == Some(receiver.as_str()) {
                    mocks.extend(owner_result_locals(test, &owner_call_names(test, owner)));
                }
                reads.mocked_calls = mocks
                    .into_iter()
                    .map(|local| {
                        if method.is_empty() {
                            local
                        } else {
                            format!("{local}.{method}")
                        }
                    })
                    .collect();
            }
            ChangedEffect::CallOnParameter { .. } => {}
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

    /// Whether one assertion (its gate text, starting on file line `line`)
    /// observes the effect. It must run after an owner call: an assertion
    /// with no known line, or one above the first owner call, reads the
    /// channel before the changed effect could write it.
    pub(super) fn observed_by(&self, assertion: &str, line: Option<usize>) -> bool {
        if self
            .first_call_line
            .zip(line)
            .is_none_or(|(first_call, line)| line < first_call)
        {
            return false;
        }
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
    let names = owner_call_names(test, owner);
    let mut calls: Vec<(usize, &str)> = names
        .iter()
        .flat_map(|name| call_arglists_with_offsets(&test.body_text, name, false))
        .collect();
    calls.sort_unstable();
    calls
}

/// The spellings a test calls the owner by: its own name, an import alias, or
/// a module-qualified name for a function; a local bound to its class for a
/// method, and the imported class itself for a classmethod or staticmethod
/// (rule 9, #7634).
fn owner_call_names(test: &PythonTest, owner: &PythonOwner) -> Vec<String> {
    let mut names = Vec::new();
    if is_method_owner(owner) {
        if let Some((class, _)) = owner.qualified_name.rsplit_once('.') {
            names.extend(
                owner_method_bound_locals(test, owner, class, &owner.name)
                    .into_iter()
                    .map(|local| format!("{local}.{}", owner.name)),
            );
            if owner
                .decorators
                .iter()
                .any(|decorator| matches!(decorator.as_str(), "classmethod" | "staticmethod"))
            {
                names.extend(
                    owner_class_locals(test, owner, class)
                        .into_iter()
                        .map(|local| format!("{local}.{}", owner.name)),
                );
            }
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
    names
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
    // The implicit receiver is bound by the owner's kind, not its name: a
    // classmethod's `klass` is skipped, a staticmethod's `cls` is not (#7634).
    let skip = usize::from(
        is_method_owner(owner)
            && !owner
                .decorators
                .iter()
                .any(|decorator| decorator.ends_with("staticmethod")),
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

/// Whether a pinned expectation reads the new argument list as a different
/// call (#7586). A changed argument count or keyword set does. So does a
/// rewritten argument that does not keep the old one inside it (`'hi'` to
/// `'welcome'`), or keeps it only under literal arithmetic (`n + 1`,
/// `n * 100`). Any other rewrite that keeps the old argument inside it, such
/// as `abs(n)`, `max(n, 0)`, `n or 0` or `(n)`, can return `n` unchanged
/// at the input the test passes, so the pinned expectation may still hold.
/// Literal arithmetic is credited as RIPR-SPEC-0238 Decision 15 records:
/// `n * 100` equals `n` only at 0.
fn arguments_rewritten_visibly(old: &str, new: &str) -> bool {
    let (old, new) = (split_top_level_args(old), split_top_level_args(new));
    if old
        .iter()
        .chain(new.iter())
        .any(|argument| argument.trim().starts_with('*'))
    {
        return false;
    }
    let keyword = |argument: &str| {
        argument
            .split_once('=')
            .filter(|(name, value)| {
                super::static_limits::is_simple_python_identifier(name.trim())
                    && !value.starts_with('=')
            })
            .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
    };
    // Positional values in order, then keyword values sorted by name.
    let values = |arguments: &[&str]| -> (Vec<String>, Vec<(String, String)>) {
        let mut keywords: Vec<(String, String)> = arguments
            .iter()
            .filter_map(|argument| keyword(argument))
            .collect();
        keywords.sort();
        let positional = arguments
            .iter()
            .filter(|argument| keyword(argument).is_none())
            .map(|argument| argument.trim().to_string())
            .collect();
        (positional, keywords)
    };
    let ((old_positional, old_keywords), (new_positional, new_keywords)) =
        (values(&old), values(&new));
    let names = |keywords: &[(String, String)]| -> Vec<String> {
        keywords.iter().map(|(name, _)| name.clone()).collect()
    };
    if old_positional.len() != new_positional.len() || names(&old_keywords) != names(&new_keywords)
    {
        return true;
    }
    old_positional
        .iter()
        .zip(&new_positional)
        .chain(
            old_keywords
                .iter()
                .map(|(_, value)| value)
                .zip(new_keywords.iter().map(|(_, value)| value)),
        )
        .any(|(old, new)| old.trim() != new.trim() && visible_rewrite(old, new))
}

/// Whether one rewritten argument reads as a different value: neither side
/// keeps the other inside it, or one side is the other under literal
/// arithmetic.
fn visible_rewrite(old: &str, new: &str) -> bool {
    let (old, new) = (old.trim(), new.trim());
    if is_literal(old) && is_literal(new) {
        // Compare values, not spellings: `2` is `2.0` and `'hi'` is `"hi"`.
        // A literal whose value is not read fails closed.
        match (literal_value(old), literal_value(new)) {
            (Some(Literal::Number(old)), Some(Literal::Number(new))) => {
                old.partial_cmp(&new) != Some(std::cmp::Ordering::Equal)
            }
            (Some(Literal::Text(old)), Some(Literal::Text(new))) => old != new,
            (Some(_), Some(_)) => true,
            _ => false,
        }
    } else if contains_expression(new, old) {
        literal_arithmetic(old, new)
    } else if contains_expression(old, new) {
        literal_arithmetic(new, old)
    } else {
        true
    }
}

/// A plain numeric or string literal: `2`, `2.5`, `'hi'`.
fn is_literal(text: &str) -> bool {
    let text = text.trim();
    let digits = text.strip_prefix('-').unwrap_or(text);
    (digits.starts_with(|ch: char| ch.is_ascii_digit())
        && digits
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '_')))
        || text.chars().next().is_some_and(|quote| {
            matches!(quote, '\'' | '"')
                && text.len() >= 2
                && text.ends_with(quote)
                && !text[1..text.len() - 1].contains([quote, '\\'])
        })
}

enum Literal<'a> {
    Number(f64),
    Text(&'a str),
}

/// The value of a plain literal (`is_literal`).
fn literal_value(text: &str) -> Option<Literal<'_>> {
    let text = text.trim();
    match text.chars().next() {
        Some(quote @ ('\'' | '"')) => text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
            .map(Literal::Text),
        _ => text
            .replace('_', "")
            .parse::<f64>()
            .ok()
            .map(Literal::Number),
    }
}

/// Whether `operator` with `literal` leaves its operand unchanged: `+ 0`,
/// `- 0`, `* 1`, `/ 1`, `// 1`, `** 1`, or `+ ''`. `literal_first` reads
/// `literal operator operand`, where only `0 +`, `1 *` and `'' +` are.
fn is_identity(operator: &str, literal: &str, literal_first: bool) -> bool {
    let equals = |value: f64| {
        matches!(literal_value(literal), Some(Literal::Number(number))
            if number.partial_cmp(&value) == Some(std::cmp::Ordering::Equal))
    };
    let empty_text = matches!(literal_value(literal), Some(Literal::Text("")));
    match operator {
        "+" => equals(0.0) || empty_text,
        "-" => !literal_first && equals(0.0),
        "*" => equals(1.0),
        "/" | "//" | "**" => !literal_first && equals(1.0),
        _ => false,
    }
}

/// Whether `outer` holds `inner` as an identifier-bounded piece of code.
fn contains_expression(outer: &str, inner: &str) -> bool {
    !inner.is_empty()
        && outer.match_indices(inner).any(|(start, _)| {
            let end = start + inner.len();
            !outer[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch == '.' || is_python_identifier_char(ch))
                && !outer[end..]
                    .chars()
                    .next()
                    .is_some_and(is_python_identifier_char)
                && !python_text_hides_code(outer, start)
        })
}

/// Whether `outer` is `base <op> literal` or `literal <op> base` for an
/// arithmetic operator and a numeric or string literal that is not an
/// identity for it (`+ 0`, `* 1`), with `base` one
/// operand that binds tighter than any operator: `int(text) + 1`.
fn literal_arithmetic(base: &str, outer: &str) -> bool {
    const OPERATORS: [&str; 7] = ["**", "//", "+", "-", "*", "/", "%"];
    let atomic = !base.is_empty()
        && [
            " ", "+", "-", "*", "/", "%", "<", ">", "=", "|", "&", "^", ",", "~",
        ]
        .iter()
        .all(|operator| split_top_level(base, operator).len() == 1);
    let operator_then_literal = |rest: &str| {
        let rest = rest.trim();
        OPERATORS.iter().any(|operator| {
            rest.strip_prefix(operator).is_some_and(|literal| {
                !literal.trim_start().starts_with(['*', '/'])
                    && is_literal(literal)
                    && !is_identity(operator, literal, false)
            })
        })
    };
    let literal_then_operator = |rest: &str| {
        let rest = rest.trim();
        OPERATORS.iter().any(|operator| {
            rest.strip_suffix(operator).is_some_and(|literal| {
                !literal.trim_end().ends_with(['*', '/'])
                    && is_literal(literal)
                    && !is_identity(operator, literal, true)
            })
        })
    };
    atomic
        && (outer.strip_prefix(base).is_some_and(operator_then_literal)
            || outer.strip_suffix(base).is_some_and(literal_then_operator))
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
