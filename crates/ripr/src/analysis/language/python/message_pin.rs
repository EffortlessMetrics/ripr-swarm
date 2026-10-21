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

use super::PythonAssertion;
use super::assertion_selection::PythonAssertionFocus;
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
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
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<MessagePinGap> {
    let (old_exception, old_message) = single_raise_message(old_line_text?)?;
    let (new_exception, new_message) = single_raise_message(line_text)?;
    if old_exception != new_exception || old_message == new_message {
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

/// The exception expression text and the literal message of a line that is
/// exactly `raise Exc("message")`.
fn single_raise_message(line: &str) -> Option<(String, String)> {
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
    let [Expr::Constant(constant)] = call.args.as_slice() else {
        return None;
    };
    let ast::Constant::Str(message) = &constant.value else {
        return None;
    };
    Some((super::expr_full_name(&call.func)?, message.clone()))
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
    use super::*;

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
