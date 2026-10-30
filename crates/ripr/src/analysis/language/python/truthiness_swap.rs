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
    if !parameter_only_read(owner_source?, owner, &name, line_text) {
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
            if has_fstring(raw) || has_unicode_name(raw) {
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
                || calls_through_a_result(text)
                || names.iter().any(|name| mentions_without_call(text, name))
                || calls_unrecognized_callee(text, &names)
        })
}

/// Whether a call's callee is itself a subscript or a call result
/// (`RUNNERS[0](0)`, `make()(0)`): a callable the reader cannot name.
fn calls_through_a_result(text: &str) -> bool {
    text.match_indices('(').any(|(index, _)| {
        text[..index]
            .trim_end()
            .ends_with(|ch: char| ch == ')' || ch == ']')
    })
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
/// itself (any receiver), known pure builtins, the bare `assert` statement
/// and the named `self.assert*` unittest methods, a
/// `pytest.raises`/`approx`/`warns`/`param`/`mark.*` helper, `pytest.raises`/`approx`/`warns`/`param`/`mark.*`, and the
/// `def`/`class` headers. Any other callee, attribute calls included
/// (`helpers.run(0)`, `self.check(0)`, `mock.patch(..)`), may pass an input
/// to the owner that the call-row reader never sees.
fn calls_unrecognized_callee(text: &str, owner_names: &[String]) -> bool {
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
            || owner_names
                .iter()
                .any(|name| name.rsplit('.').next() == Some(last));
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
        assert!(!calls_unrecognized_callee(
            "def test_pick():\n    assert len(pick(1)) == 1\n    pytest.raises(E)\n    self.assertEqual(pick(1), 1)\n",
            &names
        ));
        for unknown in [
            "assert w(0) == 3",
            "assert helpers.run(0) == 3",
            "assert self.check(0) == 3",
            "mock.patch('a')",
            "assert str(pick(1)).strip()",
        ] {
            assert!(calls_unrecognized_callee(unknown, &names), "{unknown}");
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
        assert!(calls_unrecognized_callee("assert_pick(0, 3)", &names));
        assert!(calls_unrecognized_callee("self.assertHelper(0)", &names));
        assert!(!calls_unrecognized_callee(
            "self.assertEqual(pick(1), 1)",
            &names
        ));
        assert!(has_assignment("x <<= 1"));
        assert!(!has_assignment("a >= b and c <= d != e == f"));
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
