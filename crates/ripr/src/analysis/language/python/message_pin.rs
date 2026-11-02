//! Message-pin evidence for the Python preview adapter (#9185).
//!
//! A changed `raise Exc("old message")` -> `raise Exc("new message")` is
//! discriminated by an exception assertion only when the assertion's
//! `match=` / expected-regex pattern accepts one message and rejects the
//! other. A plain literal pattern that matches BOTH messages (the unchanged
//! prefix `negative value` of `negative value: must be at least zero`) pins
//! only the exception class, as the call without a pattern does, so a strong
//! error-variant oracle does not discriminate the message change.
//!
//! RIPR-SPEC-0238 rule 2 reads the context-free trivial patterns (`''`,
//! `.*`, alternations). This gate adds the change-aware reading: the old and
//! the new message are both known from the diff, so the pattern is evaluated
//! against each. It engages only when static evidence can read everything it
//! needs: a message-only change of one single-argument literal `raise`, and
//! for EVERY strong error-variant assertion of every strong related test a
//! literal pattern made of literal characters (`.` is read as the regex
//! wildcard). Any regex syntax, non-literal pattern, f-string message,
//! `str(exc.value) == ...` comparison or other assertion shape fails open and
//! keeps the existing verdict.
//!
//! The same reading covers the two-step form `msg = "..."` then
//! `raise Exc(msg)` (the attrs `default_if_none` instance of #9185). The
//! changed line is then a plain assignment, and the message is resolved
//! through the raise only when the owner mentions the name exactly twice: the
//! assignment and one single-line `raise Exc(name)`. Any other mention (a
//! rebind, a second use, a nested scope, a keyword) or dynamic scope access
//! leaves the message unresolved and keeps the existing verdict.
//!
//! The same reading covers a message-only edit of a multi-line
//! `raise Exc(\n    "message"\n)` when the changed line is the lone string
//! literal between the opening `raise Exc(` and the closing `)`.

use super::assertion_selection::PythonAssertionFocus;
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
use super::{PythonAssertion, PythonOwner};
use crate::domain::{OracleKind, OracleStrength};
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

/// Why a message-only change to a `raise` is not discriminated.
pub(super) struct MessagePinGap {
    /// The changed message a distinguishing `match=` pattern must reject
    /// the old text for.
    pub(super) discriminator: String,
    /// Which patterns accept both messages.
    pub(super) reason: String,
}

/// Evidence that every strong error-variant assertion pins the changed
/// `raise` with a literal pattern that accepts the old and the new message
/// alike. `None` when the change is not a readable message-only edit, when no
/// strong error-variant assertion exists, or when any one of them is not
/// readable or tells the two messages apart.
pub(super) fn python_message_pin_gap(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    owner_source: Option<&str>,
    line: usize,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<MessagePinGap> {
    let (new_exception, old_message, new_message) =
        changed_message(line_text, old_line_text?, owner, owner_source, line)?;
    if old_message == new_message {
        return None;
    }
    let mut patterns: Vec<String> = Vec::new();
    for candidate in related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        for assertion in &candidate.test.assertions {
            if !is_strong_error_variant(assertion) || !focus.admits(assertion) {
                continue;
            }
            // A comparison joined after the `with` (rule 14, `assert
            // str(exc.value) == "..."`) is a second discriminator this gate
            // does not evaluate, so it fails open.
            if assertion
                .gate_text
                .strip_prefix(assertion.text.as_str())
                .is_some_and(|rest| {
                    rest.lines()
                        .any(|line| line.trim_start().starts_with("assert "))
                })
            {
                return None;
            }
            let pattern = literal_message_pattern(&assertion.text)?;
            if !pattern_matches(&pattern, &old_message) || !pattern_matches(&pattern, &new_message)
            {
                return None;
            }
            if !patterns.contains(&pattern) {
                patterns.push(pattern);
            }
        }
    }
    if patterns.is_empty() {
        return None;
    }
    let names = patterns
        .iter()
        .map(|pattern| format!("`{pattern}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(MessagePinGap {
        discriminator: format!("message {new_message:?}"),
        reason: format!(
            "every strong related error assertion pins `{new_exception}` with a literal message pattern ({names}) that matches the old message {old_message:?} and the new message {new_message:?} alike, so it pins only the exception class"
        ),
    })
}

fn is_strong_error_variant(assertion: &PythonAssertion) -> bool {
    assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
        && matches!(assertion.oracle_kind, OracleKind::ExactErrorVariant)
}

/// The message argument of a one-line `raise Exc(argument)`.
enum RaiseArgument {
    Literal(String),
    Name(String),
}

/// The exception expression text and the literal message of a line that is
/// exactly `raise Exc("message")`.
fn single_raise_message(line: &str) -> Option<(String, String)> {
    match single_raise_argument(line)? {
        (exception, RaiseArgument::Literal(message)) => Some((exception, message)),
        _ => None,
    }
}

/// The name and literal of a line that is exactly `name = "literal"`.
fn assigned_message(line: &str) -> Option<(String, String)> {
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<message-assign>"), line.trim())
    else {
        return None;
    };
    let [Stmt::Assign(assign)] = module.body.as_slice() else {
        return None;
    };
    let [Expr::Name(name)] = assign.targets.as_slice() else {
        return None;
    };
    let Expr::Constant(constant) = assign.value.as_ref() else {
        return None;
    };
    let ast::Constant::Str(message) = &constant.value else {
        return None;
    };
    Some((name.id.to_string(), message.clone()))
}

/// The raised exception and the old and new message of the changed line: a
/// message-only edit of `raise Exc("...")`, or of the `name = "..."` the
/// owner raises through `raise Exc(name)`.
fn changed_message(
    line_text: &str,
    old_line_text: &str,
    owner: &PythonOwner,
    owner_source: Option<&str>,
    line: usize,
) -> Option<(String, String, String)> {
    if let (Some((old_exception, old_message)), Some((new_exception, new_message))) = (
        single_raise_message(old_line_text),
        single_raise_message(line_text),
    ) {
        return (old_exception == new_exception).then_some((
            new_exception,
            old_message,
            new_message,
        ));
    }
    if let (Some((old_name, old_message)), Some((new_name, new_message))) =
        (assigned_message(old_line_text), assigned_message(line_text))
        && old_name == new_name
    {
        return raised_through_local(owner, owner_source?, line, &new_name)
            .map(|exception| (exception, old_message, new_message));
    }
    multiline_raise_message(line_text, old_line_text, owner, owner_source, line)
}

/// The raised exception and the old and new message when the changed line is
/// the lone string literal of a multi-line `raise Exc(\n    "..."\n)`.
fn multiline_raise_message(
    line_text: &str,
    old_line_text: &str,
    owner: &PythonOwner,
    owner_source: Option<&str>,
    line: usize,
) -> Option<(String, String, String)> {
    let old_message = plain_str_literal_line(old_line_text)?;
    let new_message = plain_str_literal_line(line_text)?;
    let owner_source = owner_source?;
    let lines: Vec<&str> = owner_source.lines().collect();
    let start = owner.start_line.checked_sub(1)?;
    let changed = line.checked_sub(1)?;
    let end = owner.end_line.min(lines.len());
    if changed <= start || changed >= end {
        return None;
    }
    let current_line = lines.get(changed)?;
    if plain_str_literal_line(current_line)? != new_message {
        return None;
    }
    let above = lines.get(changed.checked_sub(1)?)?.trim();
    if above.is_empty() || above.starts_with('#') {
        return None;
    }
    let exception = multiline_raise_opening(above)?;
    let below = lines.get(changed + 1)?.trim();
    if below != ")" {
        return None;
    }
    Some((exception, old_message, new_message))
}

/// One plain `str` literal on a line, optionally with one trailing `,`.
fn plain_str_literal_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let (source, trailing_comma) = if let Some(without) = trimmed.strip_suffix(',') {
        (without.trim_end(), true)
    } else {
        (trimmed, false)
    };
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<str-literal-line>"), source)
    else {
        return None;
    };
    let [Stmt::Expr(expr)] = module.body.as_slice() else {
        return None;
    };
    plain_str_literal_expr(expr.value.as_ref(), trailing_comma)
}

fn plain_str_literal_expr(expr: &Expr, trailing_comma: bool) -> Option<String> {
    match expr {
        Expr::Constant(constant) => {
            let ast::Constant::Str(message) = &constant.value else {
                return None;
            };
            Some(message.clone())
        }
        Expr::JoinedStr(_) => None,
        Expr::Tuple(tuple) if trailing_comma && tuple.elts.len() == 1 => {
            plain_str_literal_expr(&tuple.elts[0], false)
        }
        _ => None,
    }
}

/// The exception name of a line that is exactly `raise <dotted.Name>(`.
fn multiline_raise_opening(line: &str) -> Option<String> {
    let text = line.trim();
    let rest = text.strip_prefix("raise ")?.trim();
    if !rest.ends_with('(') {
        return None;
    }
    let name = rest.strip_suffix('(')?.trim();
    if name.is_empty() {
        return None;
    }
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<raise-open>"), name) else {
        return None;
    };
    let [Stmt::Expr(expr)] = module.body.as_slice() else {
        return None;
    };
    super::expr_full_name(expr.value.as_ref())
}

/// How many times `text` mentions `name` as a whole identifier.
fn mentions(text: &str, name: &str) -> usize {
    let is_identifier = |ch: char| ch.is_alphanumeric() || ch == '_';
    text.match_indices(name)
        .filter(|(index, _)| {
            let before = text[..*index].chars().next_back();
            let after = text[index + name.len()..].chars().next();
            !before.is_some_and(is_identifier) && !after.is_some_and(is_identifier)
        })
        .count()
}

/// The exception of the single `raise Exc(name)` an owner raises through the
/// local `name` assigned on `line`. `None` unless the owner is a function
/// whose source mentions `name` exactly twice (the assignment, then that one
/// raise), so a rebind, a second use, a nested scope, a keyword argument or a
/// dynamic scope read leaves the message unresolved.
fn raised_through_local(
    owner: &PythonOwner,
    owner_source: &str,
    line: usize,
    name: &str,
) -> Option<String> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let start = owner.start_line.checked_sub(1)?;
    let changed = line.checked_sub(1)?;
    let end = owner.end_line.min(lines.len());
    let header = lines.get(start)?.trim_start();
    if !(header.starts_with("def ") || header.starts_with("async def "))
        || changed <= start
        || changed >= end
    {
        return None;
    }
    let body = lines.get(start..end)?;
    if body.iter().any(|text| {
        ["locals", "vars", "globals", "exec", "eval", "setattr"]
            .iter()
            .any(|dynamic| mentions(text, dynamic) > 0)
    }) {
        return None;
    }
    let mut raised: Option<(usize, &str)> = None;
    for (offset, text) in body.iter().enumerate() {
        let index = start + offset;
        match (index == changed, mentions(text, name)) {
            (_, 0) | (true, 1) => {}
            (false, 1) if raised.is_none() => raised = Some((index, text)),
            _ => return None,
        }
    }
    let (index, text) = raised?;
    if index <= changed {
        return None;
    }
    match single_raise_argument(text)? {
        (exception, RaiseArgument::Name(raised_name)) if raised_name == name => Some(exception),
        _ => None,
    }
}

/// The exception expression text and the message argument of a line that is
/// exactly `raise Exc(<one string literal or one bare name>)`.
fn single_raise_argument(line: &str) -> Option<(String, RaiseArgument)> {
    let text = line.trim();
    if !text.starts_with("raise ") {
        return None;
    }
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<raise-message>"), text) else {
        return None;
    };
    let [Stmt::Raise(raise)] = module.body.as_slice() else {
        return None;
    };
    if raise.cause.is_some() {
        return None;
    }
    let Expr::Call(call) = raise.exc.as_deref()? else {
        return None;
    };
    if !call.keywords.is_empty() {
        return None;
    }
    let argument = match call.args.as_slice() {
        [Expr::Constant(constant)] => {
            let ast::Constant::Str(message) = &constant.value else {
                return None;
            };
            RaiseArgument::Literal(message.clone())
        }
        [Expr::Name(name)] => RaiseArgument::Name(name.id.to_string()),
        _ => return None,
    };
    Some((super::expr_full_name(&call.func)?, argument))
}

/// The literal message pattern of a `pytest.raises(E, match=...)` or
/// `assertRaisesRegex(E, pattern, ...)` assertion text. `None` for every
/// other shape (the callable form of `pytest.raises` hands `match=` to the
/// callable, RIPR-SPEC-0238 rule 13).
fn literal_message_pattern(assertion_text: &str) -> Option<String> {
    let Ok(Mod::Module(module)) =
        parse_module_result(Path::new("<message-pattern>"), assertion_text.trim())
    else {
        return None;
    };
    let [Stmt::Expr(statement)] = module.body.as_slice() else {
        return None;
    };
    let Expr::Call(call) = statement.value.as_ref() else {
        return None;
    };
    let callee = super::expr_full_name(&call.func)?;
    let last = callee.rsplit('.').next().unwrap_or(&callee);
    let keyword = |name: &str| {
        call.keywords
            .iter()
            .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg.as_str() == name))
            .map(|keyword| &keyword.value)
    };
    let pattern = match last {
        "raises" if call.args.len() < 2 => keyword("match")?,
        "assertRaisesRegex" => call.args.get(1).or_else(|| keyword("expected_regex"))?,
        _ => return None,
    };
    let Expr::Constant(constant) = pattern else {
        return None;
    };
    let ast::Constant::Str(text) = &constant.value else {
        return None;
    };
    Some(text.clone())
}

/// Whether `re.search(pattern, message)` holds for a pattern made only of
/// literal characters and the `.` wildcard. A pattern with any other regex
/// syntax is not evaluated and reports `false`, which keeps the gate off.
fn pattern_matches(pattern: &str, message: &str) -> bool {
    if pattern.chars().any(|ch| {
        matches!(
            ch,
            '\\' | '^' | '$' | '*' | '+' | '?' | '{' | '}' | '[' | ']' | '|' | '(' | ')'
        )
    }) {
        return false;
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let message: Vec<char> = message.chars().collect();
    if pattern.len() > message.len() {
        return false;
    }
    (0..=message.len() - pattern.len()).any(|start| {
        pattern
            .iter()
            .zip(&message[start..])
            .all(|(expected, actual)| (*expected == '.' && *actual != '\n') || expected == actual)
    })
}

#[cfg(test)]
mod tests {
    use super::super::assertion_selection::PythonAssertionFocus;
    use super::super::owners_tests::{extract_owners, extract_tests};
    use super::super::probe_shape::classify_probe_shape;
    use super::super::related_tests::related_test_candidates;
    use super::*;
    use crate::domain::ProbeFamily;

    fn owner_from(source: &str) -> PythonOwner {
        let owners: Vec<_> = extract_owners(Path::new("subject.py"), source)
            .into_iter()
            .filter(|owner| owner.name == "check")
            .collect();
        assert_eq!(owners.len(), 1, "{source}");
        owners[0].clone()
    }

    const MULTILINE_RAISE_SUBJECT: &str = "def check(x):
    if x:
        raise ValueError(
            \"negative value: must be at most zero\"
        )
";

    const MULTILINE_RAISE_SUBJECT_COMMA: &str = "def check(x):
    if x:
        raise ValueError(
            \"negative value: must be at most zero\",
        )
";

    const IMPLICIT_CONCAT_SUBJECT: &str = "def check(x):
    if x:
        raise ValueError(
            \"negative value: must be at least zero\"
            \" suffix\"
        )
";

    const TRAILING_ARG_SUBJECT: &str = "def check(x):
    if x:
        raise ValueError(
            \"negative value: must be at most zero\",
            1
        )
";

    const KEYWORD_MESSAGE_SUBJECT: &str = "def check(x):
    if x:
        raise ValueError(
            msg=\"negative value: must be at most zero\"
        )
";

    const RAISE_FROM_SUBJECT: &str = "def check(x):
    if x:
        raise ValueError(
            \"negative value: must be at most zero\"
        ) from err
";

    const NON_RAISE_OPEN_SUBJECT: &str = "def check(x):
    if x:
        x = ValueError(
            \"negative value: must be at most zero\"
        )
";

    const PYTEST_SHARED_MATCH: &str = "import pytest

from subject import check

def test_check():
    with pytest.raises(ValueError, match=\"negative value\"):
        check(True)
";

    const PYTEST_DISTINGUISHING_MATCH: &str = "import pytest

from subject import check

def test_check():
    with pytest.raises(ValueError, match=\"must be at most\"):
        check(True)
";

    fn multiline_owner() -> (&'static str, PythonOwner) {
        (MULTILINE_RAISE_SUBJECT, owner_from(MULTILINE_RAISE_SUBJECT))
    }

    #[test]
    fn multiline_raise_message_line_classifies_as_predicate() {
        let (family, _) = classify_probe_shape("    \"negative value: must be at most zero\"");
        assert_eq!(family, ProbeFamily::Predicate);
    }

    #[test]
    fn changed_message_reads_multiline_raise_literal_with_and_without_trailing_comma() {
        let (source, owner) = multiline_owner();
        let old = "            \"negative value: must be at least zero\"";
        let new = "            \"negative value: must be at most zero\"";
        assert_eq!(
            changed_message(new, old, &owner, Some(source), 4),
            Some((
                "ValueError".to_string(),
                "negative value: must be at least zero".to_string(),
                "negative value: must be at most zero".to_string(),
            ))
        );
        let source_comma = MULTILINE_RAISE_SUBJECT_COMMA;
        let owner_comma = owner_from(source_comma);
        let old_comma = "            \"negative value: must be at least zero\",";
        let new_comma = "            \"negative value: must be at most zero\",";
        assert_eq!(
            changed_message(new_comma, old_comma, &owner_comma, Some(source_comma), 4),
            Some((
                "ValueError".to_string(),
                "negative value: must be at least zero".to_string(),
                "negative value: must be at most zero".to_string(),
            ))
        );
    }

    #[test]
    fn changed_message_rejects_non_multiline_raise_literal_lines() {
        let (source, owner) = multiline_owner();
        let resolve = |new: &str, old: &str, line: usize, src: &str, own: &PythonOwner| {
            changed_message(new, old, own, Some(src), line)
        };
        let base_old = "            \"negative value: must be at least zero\"";
        let base_new = "            \"negative value: must be at most zero\"";
        for (new, old, line, src) in [
            (
                "            f\"negative value: must be at most zero\"",
                base_old,
                4,
                source,
            ),
            (
                "            b\"negative value: must be at most zero\"",
                base_old,
                4,
                source,
            ),
            (
                "            \"negative value: must be at least zero\"",
                "            \"negative value: must be at least\"",
                4,
                IMPLICIT_CONCAT_SUBJECT,
            ),
            (base_new, base_old, 4, TRAILING_ARG_SUBJECT),
            (base_new, base_old, 4, KEYWORD_MESSAGE_SUBJECT),
            (base_new, base_old, 4, RAISE_FROM_SUBJECT),
            (base_new, base_old, 4, NON_RAISE_OPEN_SUBJECT),
            (base_new, base_old, 99, source),
        ] {
            let own = owner_from(src);
            assert_eq!(resolve(new, old, line, src, &own), None, "{src}");
        }
        assert_eq!(
            changed_message(base_new, base_old, &owner, None, 4),
            None,
            "owner_source None"
        );
        assert_eq!(
            changed_message(base_new, base_old, &owner, Some(source), 3),
            None,
            "changed line is the opening raise"
        );
    }

    #[test]
    fn python_message_pin_gap_reads_multiline_raise_literal_edit() {
        let subject = MULTILINE_RAISE_SUBJECT;
        let owner = owner_from(subject);
        let test_source = PYTEST_SHARED_MATCH;
        let tests = extract_tests(Path::new("tests/test_check.py"), test_source);
        let candidates = related_test_candidates(&owner, &tests);
        assert!(!candidates.is_empty());
        let old = "            \"negative value: must be at least zero\"";
        let new = "            \"negative value: must be at most zero\"";
        let focus = PythonAssertionFocus::for_change(classify_probe_shape(new).0, new, Some(old));
        let gap = python_message_pin_gap(
            new,
            Some(old),
            &owner,
            Some(subject),
            4,
            &candidates,
            &focus,
        );
        assert!(
            gap.as_ref().is_some_and(|gap| {
                gap.reason.contains("ValueError") && gap.reason.contains("negative value")
            }),
            "multiline message-only edit under shared match pattern"
        );
    }

    #[test]
    fn python_message_pin_gap_fails_open_when_match_pattern_distinguishes_messages() {
        let subject = MULTILINE_RAISE_SUBJECT;
        let owner = owner_from(subject);
        let test_source = PYTEST_DISTINGUISHING_MATCH;
        let tests = extract_tests(Path::new("tests/test_check.py"), test_source);
        let candidates = related_test_candidates(&owner, &tests);
        let old = "            \"negative value: must be at least zero\"";
        let new = "            \"negative value: must be at most zero\"";
        let focus = PythonAssertionFocus::for_change(classify_probe_shape(new).0, new, Some(old));
        assert!(
            python_message_pin_gap(
                new,
                Some(old),
                &owner,
                Some(subject),
                4,
                &candidates,
                &focus,
            )
            .is_none()
        );
    }

    #[test]
    fn pattern_matches_reads_literals_and_the_dot_wildcard() {
        assert!(pattern_matches(
            "negative value",
            "negative value: must be positive"
        ));
        assert!(pattern_matches("", "anything"));
        assert!(pattern_matches("a.c", "xxabcxx"));
        assert!(!pattern_matches("a.c", "ac"));
        assert!(!pattern_matches("must be at least", "must be at most zero"));
        // Regex syntax is not evaluated.
        for pattern in [
            "a|b", "^a", "a$", "a*", "a+", "a?", "a{2}", "[a]", "(a)", "\\d",
        ] {
            assert!(!pattern_matches(pattern, "a"), "{pattern}");
        }
    }

    #[test]
    fn single_raise_message_reads_only_one_literal_argument() {
        assert_eq!(
            single_raise_message("    raise ValueError(\"bad\")"),
            Some(("ValueError".to_string(), "bad".to_string()))
        );
        for line in [
            "raise ValueError",
            "raise ValueError(\"a\", \"b\")",
            "raise ValueError(f\"bad {x}\")",
            "raise ValueError(msg)",
            "raise ValueError(\"a\") from err",
            "raise ValueError(\"a\", code=1)",
            "return ValueError(\"a\")",
        ] {
            assert_eq!(single_raise_message(line), None, "{line}");
        }
    }

    #[test]
    fn assigned_message_reads_only_one_name_and_literal() {
        assert_eq!(
            assigned_message("    msg = \"bad\""),
            Some(("msg".to_string(), "bad".to_string()))
        );
        for line in [
            "msg = bad",
            "msg = f\"bad {x}\"",
            "a = b = \"bad\"",
            "self.msg = \"bad\"",
            "msg += \"bad\"",
            "msg: str = \"bad\"",
            "raise ValueError(\"bad\")",
        ] {
            assert_eq!(assigned_message(line), None, "{line}");
        }
    }

    #[test]
    fn raised_through_local_resolves_only_a_single_raise_of_the_name() {
        let owner = |source: &str| {
            super::super::owners_tests::extract_owners(Path::new("src/subject.py"), source)
                .into_iter()
                .next()
        };
        let resolve = |source: &str| {
            let owner = owner(source)?;
            raised_through_local(&owner, source, 3, "msg")
        };
        assert_eq!(
            resolve("def f(arg):\n    if arg:\n        msg = \"a\"\n        raise KeyError(msg)\n"),
            Some("KeyError".to_string())
        );
        for source in [
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        raise KeyError(msg)\n        raise KeyError(msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        print(msg)\n        raise KeyError(msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        raise KeyError(msg + \"b\")\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        raise KeyError(msg=msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        vars()\n        raise KeyError(msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n    return msg\n",
            // A walrus, for-target, with-target, except-target or augmented
            // rebind adds a mention, so the name stays unresolved.
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        (msg := other())\n        raise KeyError(msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        for msg in items():\n            pass\n        raise KeyError(msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        with ctx() as msg:\n            pass\n        raise KeyError(msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        try:\n            go()\n        except Oops as msg:\n            pass\n        raise KeyError(msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        msg += \"b\"\n        raise KeyError(msg)\n",
            // A comment or string mention is counted too (fail closed).
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        # msg is shown to the user\n        raise KeyError(msg)\n",
            "def f(arg):\n    if arg:\n        msg = \"a\"\n        note = \"see msg\"\n        raise KeyError(msg)\n",
        ] {
            assert_eq!(resolve(source), None, "{source}");
        }
    }

    #[test]
    fn literal_message_pattern_reads_both_assertion_forms() {
        assert_eq!(
            literal_message_pattern("pytest.raises(ValueError, match=\"bad\")"),
            Some("bad".to_string())
        );
        assert_eq!(
            literal_message_pattern("self.assertRaisesRegex(ValueError, 'bad', f, 1)"),
            Some("bad".to_string())
        );
        assert_eq!(
            literal_message_pattern("self.assertRaisesRegex(ValueError, expected_regex='bad')"),
            Some("bad".to_string())
        );
        for text in [
            "pytest.raises(ValueError)",
            "pytest.raises(ValueError, match=pattern)",
            "pytest.raises(ValueError, f, match=\"bad\")",
            "assert str(exc.value) == \"bad\"",
        ] {
            assert_eq!(literal_message_pattern(text), None, "{text}");
        }
    }
}
