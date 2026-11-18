//! #14968: an unconditional literal-only assertion observes nothing.
//!
//! `assert!(true)`, `assert!(false)` or `assert_eq!(2 + 2, 4)` written as a
//! top-level statement of the test body reads no runtime value: it neither
//! discriminates the owner's behavior nor depends on it. Reveal never credits
//! such an assertion.
//!
//! The same literal inside an `if`, `match` arm, loop or closure is not
//! dropped: there it runs only on a path the test's own reads select
//! (`if let Ok(_) = g(2) { assert!(false) }`), so it can still observe the
//! owner through its guard.
//!
//! The dropped literal still counts toward the single-assertion fallback for
//! every assertion macro, so dropping it never hands that fallback to an
//! unrelated sibling (`assert!(true); assert_eq!(other(), 4)`). Only an
//! implicit observer that calls the owner itself (`g(2).unwrap_err()`,
//! `g(3).unwrap()`) gets the count the test has without the literal.

use crate::analysis::extract::{assertion_compares_only_literals, mask_comments_and_strings};
use crate::analysis::facts::{OracleFact, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{AstNode, ast};

/// Whether `assertion` compares only literals and is itself a top-level
/// statement (or the tail expression) of `test`'s body, located at the
/// assertion's own line. An assertion produced from elsewhere (a helper or
/// macro expansion), an unparseable body, or any nested position answers
/// `false`, which keeps the assertion as before.
pub(super) fn is_unconditional_literal_only_assertion(
    test: &TestSummary,
    assertion: &OracleFact,
) -> bool {
    if !assertion_compares_only_literals(&assertion.text) {
        return false;
    }
    let body = test.body.as_str();
    let Some(parse) = parse_clean_source_file(body) else {
        return false;
    };
    let Some(list) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)
        .and_then(|function| function.body())
        .and_then(|block| block.stmt_list())
    else {
        return false;
    };
    let wanted = squash(&assertion.text);
    let at_assertion = |node: &ra_ap_syntax::SyntaxNode| {
        let start = usize::from(node.text_range().start());
        let line = body.get(..start).map(|before| before.matches('\n').count());
        line.is_some_and(|line| test.start_line.saturating_add(line) == assertion.line)
            && squash(&node.text().to_string()) == wanted
    };
    list.statements()
        .any(|statement| at_assertion(statement.syntax()))
        || list
            .tail_expr()
            .is_some_and(|tail| at_assertion(tail.syntax()))
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .trim_end_matches(';')
        .to_string()
}

/// Whether `assertion` is an implicit observer (no macro: an `unwrap()`,
/// `unwrap_err()` or similar read extracted from a plain statement) whose own
/// text calls the owner `owner` by name. Without a known owner name, or for
/// any macro assertion, this answers `false`.
pub(super) fn is_implicit_owner_call_observer(assertion: &OracleFact, owner: Option<&str>) -> bool {
    let Some(owner) = owner.filter(|owner| !owner.is_empty()) else {
        return false;
    };
    let masked = mask_comments_and_strings(&assertion.text);
    if masked.contains('!') {
        return false;
    }
    let is_ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    masked.match_indices(owner).any(|(at, _)| {
        let before = masked.get(..at).and_then(|text| text.chars().next_back());
        let after = masked.get(at + owner.len()..).unwrap_or_default();
        before.is_none_or(|ch| !is_ident(ch)) && after.trim_start().starts_with('(')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OracleKind, OracleStrength};

    fn fact(text: &str) -> OracleFact {
        OracleFact {
            line: 1,
            text: text.to_string(),
            kind: OracleKind::Unknown,
            strength: OracleStrength::None,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        }
    }

    #[test]
    fn implicit_owner_call_observers_are_plain_reads_of_the_owner_call() {
        for text in [
            "g(2).unwrap_err()",
            "g(3).unwrap()",
            "let _e = g (2).expect_err(\"x\");",
        ] {
            assert!(
                is_implicit_owner_call_observer(&fact(text), Some("g")),
                "{text}"
            );
        }
        for (text, owner) in [
            ("other().unwrap()", Some("g")),
            ("gg(2).unwrap()", Some("g")),
            ("assert!(g(2).is_err())", Some("g")),
            ("assert_eq!(other(), 4);", Some("g")),
            ("g.unwrap()", Some("g")),
            ("g(2).unwrap()", None),
            ("\"g(2)\".len()", Some("g")),
        ] {
            assert!(
                !is_implicit_owner_call_observer(&fact(text), owner),
                "{text}"
            );
        }
    }
}
