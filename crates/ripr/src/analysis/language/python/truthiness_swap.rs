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
use super::boundary::owner_call_names;
use super::boundary::{
    BoundaryActivation, PythonBoundaryEvidence, strong_owner_call_rows_with_gaps,
    unrebound_constants,
};
use super::guard_activation::{for_target_rebinds, line_rebinds_parameter, name_rebound_in_owner};
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
    // different change. The condition must be one whole-identifier span.
    if outside(line_text, condition)? != outside(old_line_text, old_condition)? {
        return None;
    }
    if !owner
        .parameters
        .iter()
        .any(|parameter| parameter.name == name)
    {
        return None;
    }
    if parameter_rebound(owner_source?, owner, &name) {
        return None;
    }
    if owner_reached_indirectly(owner, related_candidates) {
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

/// Whether the owner body may rebind `name`: the existing line reading, plus
/// any assignment after a `:` or `;` (`if flag: x = 0`, `else: x = 0`), an
/// `as name` target, and `del`/`global`/`nonlocal`. Over-refuses by design.
fn parameter_rebound(owner_source: &str, owner: &PythonOwner, name: &str) -> bool {
    if name_rebound_in_owner(owner_source, owner, name) {
        return true;
    }
    owner_source
        .lines()
        .skip(owner.start_line)
        .take(owner.end_line.saturating_sub(owner.start_line))
        .map(str::trim)
        .any(|line| {
            statement_names(line, name)
                || line.match_indices([':', ';']).any(|(index, _)| {
                    let tail = line[index + 1..].trim();
                    line_rebinds_parameter(tail, name)
                        || for_target_rebinds(tail, name)
                        || statement_names(tail, name)
                })
        })
}

/// An `as name` target or a `del`/`global`/`nonlocal` statement naming it.
fn statement_names(text: &str, name: &str) -> bool {
    let named = |rest: &str| {
        rest.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .any(|word| word == name)
    };
    if ["del ", "global ", "nonlocal "]
        .iter()
        .any(|keyword| text.strip_prefix(keyword).is_some_and(named))
    {
        return true;
    }
    text.match_indices(" as ")
        .any(|(index, _)| named(&text[index + 4..]))
}

/// Whether some related test mentions the owner other than as a bare direct
/// call (`map(pick, ..)`, `alias = pick`, a callback argument), or calls a
/// bare name the test may define itself (`w(0)` where `w` forwards to the
/// owner), so the rows do not cover every strong input.
fn owner_reached_indirectly(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
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
            names.iter().any(|name| mentions_without_call(text, name))
                || calls_unknown_bare_name(text, &names)
        })
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

/// Whether the body calls a bare name it could define itself. Builtins, the
/// owner's own call names, keywords, attribute calls and `def`/`class`
/// headers are not unknown.
fn calls_unknown_bare_name(text: &str, owner_names: &[String]) -> bool {
    const KNOWN: &[&str] = &[
        "assert",
        "not",
        "and",
        "or",
        "in",
        "is",
        "if",
        "elif",
        "while",
        "return",
        "with",
        "for",
        "else",
        "lambda",
        "yield",
        "await",
        "print",
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
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !is_ident_byte(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_ident_byte(bytes[index]) {
            index += 1;
        }
        let word = &text[start..index];
        let attribute = start > 0 && bytes[start - 1] == b'.';
        let called = text[index..].trim_start().starts_with('(');
        let before = text[..start].trim_end();
        let header = before.ends_with("def") || before.ends_with("class");
        let owner = owner_names
            .iter()
            .any(|name| name.rsplit('.').next() == Some(word));
        if called
            && !attribute
            && !header
            && !owner
            && !KNOWN.contains(&word)
            && !word.starts_with(|ch: char| ch.is_ascii_digit())
        {
            return true;
        }
    }
    false
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
        assert!(!calls_unknown_bare_name(
            "def test_pick():\n    assert len(pick(1)) == 1\n    pytest.raises(E)\n",
            &names
        ));
        assert!(calls_unknown_bare_name("assert w(0) == 3", &names));
    }

    #[test]
    fn statement_names_reads_as_targets_and_scope_statements() {
        assert!(statement_names("with open(p) as x:", "x"));
        assert!(statement_names("del x", "x"));
        assert!(statement_names("global x", "x"));
        assert!(!statement_names("with open(p) as xs:", "x"));
        assert!(!statement_names("return x", "x"));
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
