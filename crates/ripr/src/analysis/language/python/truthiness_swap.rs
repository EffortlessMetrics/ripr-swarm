//! Predicate-swap evidence for the Python preview adapter (#9186).
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
//!   the rest of the line unchanged;
//! - the owner never rebinds that parameter;
//! - every strong related call binds that parameter to a scalar literal
//!   (`None`, `True`/`False`, a number, a plain quoted string) and no strong
//!   call goes unread.
//!
//! When every such row gives the old and the new test the same truth value,
//! no observed input selects a different branch. A row that tells them apart,
//! an unreadable or non-literal argument, a rebound parameter or a compound
//! condition fails open and keeps the existing verdict.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    BoundaryActivation, PythonBoundaryEvidence, strong_owner_call_rows_with_gaps,
    unrebound_constants,
};
use super::guard_activation::name_rebound_in_owner;
use super::related_tests::PythonRelatedCandidate;
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
    let (name, new_test) = parse_test(condition)?;
    let (old_name, old_test) = parse_test(old_condition)?;
    if name != old_name || new_test == old_test {
        return None;
    }
    // The line around the condition is unchanged: a branch value edit is a
    // different change.
    let outside = |line: &str, condition: &str| {
        line.replacen(condition, "\u{0}", 1)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    if outside(line_text, condition) != outside(old_line_text, old_condition) {
        return None;
    }
    if !owner
        .parameters
        .iter()
        .any(|parameter| parameter.name == name)
    {
        return None;
    }
    if name_rebound_in_owner(owner_source?, owner, &name) {
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
        let binding = row.bindings.get(&name)?;
        let input = read_input(&binding.value)?;
        if old_test.holds(input) != new_test.holds(input) {
            return None;
        }
        if !binding.from_default {
            observed_values.push(ValueFact {
                file: Some(row.file.clone()),
                line: row.line,
                text: row.text.clone(),
                value: format!("{name} = {}", binding.value),
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
        for compound in [
            "x is None or y",
            "not x.y",
            "not (x)",
            "x == 1",
            "len(x)",
            "not None",
            "True",
            "x is y",
        ] {
            assert_eq!(parse_test(compound), None, "{compound}");
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
}
