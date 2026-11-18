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
//! The dropped literal still counts toward the single-assertion fallback, so
//! dropping it never hands that fallback to an unrelated sibling
//! (`assert!(true); assert_eq!(other(), 4)`). Only an assertion or implicit
//! observer that reads the owner's result (calls the owner, as in
//! `assert!(g(2).is_err())` or `g(2).unwrap_err()`, or reads a binding bound
//! from an owner call) gets the count the test has without the literal.

use crate::analysis::classifier::oracle_binds_sink_identity;
use crate::analysis::extract::{
    assertion_compares_only_literals, assertion_oracle_text, mask_comments_and_strings,
};
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
    let top_level = list
        .statements()
        .any(|statement| at_assertion(statement.syntax()))
        || list
            .tail_expr()
            .is_some_and(|tail| at_assertion(tail.syntax()));
    // The fact records only a line and text, so a nested assertion on the
    // same line with the same squashed text (`if g(2) { assert_eq!("a b",
    // "ab"); } assert_eq!("ab", "ab");`) may be the fact's subject. Keep it
    // as before (fail closed).
    let nested_twin = list
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .any(|call| {
            let node = call.syntax();
            at_assertion(node)
                && node
                    .ancestors()
                    .filter_map(ast::StmtList::cast)
                    .next()
                    .is_some_and(|inner| inner.syntax() != list.syntax())
        });
    top_level && !nested_twin
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .trim_end_matches(';')
        .to_string()
}

/// Whether `assertion` reads the owner's result: its masked text calls the
/// owner `owner` by name (`assert!(g(2).is_err())`, `g(2).unwrap_err()`,
/// `g(2).expect_err(&format!("x"))`), or names a test `let` bound from an
/// owner call (`let r = g(2); .. assert!(r.is_err())`). Macro or not, the
/// decision is the shared sink-identity binding
/// (`oracle_binds_sink_identity`) over the assertion and test body with
/// comments and strings masked, so an owner name inside a string literal
/// binds nothing. A known assertion macro is read through its compared
/// operands only (`assertion_oracle_text`): a call in its diagnostic
/// arguments runs only when the assertion fails, so it reads nothing the
/// passing assertion checks. Without a known owner name this answers `false`.
pub(super) fn reads_owner_result(
    test_body: &str,
    assertion: &OracleFact,
    owner: Option<&str>,
) -> bool {
    let Some(owner) = owner.filter(|owner| !owner.is_empty()) else {
        return false;
    };
    let compared = assertion_oracle_text(&assertion.text).unwrap_or_else(|| assertion.text.clone());
    oracle_binds_sink_identity(
        &mask_comments_and_strings(&compared),
        &mask_comments_and_strings(test_body),
        Some(owner),
    )
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
    fn owner_result_readers_call_the_owner_or_read_its_binding() {
        let unbound = "fn t() {}";
        for text in [
            "g(2).unwrap_err()",
            "g(3).unwrap()",
            "let _e = g (2).expect_err(\"x\");",
            "assert!(g(2).is_err())",
            "g(2).expect_err(&format!(\"x\"))",
        ] {
            assert!(
                reads_owner_result(unbound, &fact(text), Some("g")),
                "{text}"
            );
        }
        let bound = "fn t() { let r = g(2); assert!(true); assert!(r.is_err()); }";
        assert!(reads_owner_result(
            bound,
            &fact("assert!(r.is_err())"),
            Some("g")
        ));
        for (text, owner) in [
            ("other().unwrap()", Some("g")),
            ("gg(2).unwrap()", Some("g")),
            ("assert_eq!(other(), 4);", Some("g")),
            ("g.unwrap()", Some("g")),
            ("g(2).unwrap()", None),
            ("\"g(2)\".len()", Some("g")),
            ("assert!(!o.is_empty())", Some("add")),
            ("assert_eq!(other(), 4, \"{:?}\", g(9))", Some("g")),
            ("assert!(o.is_empty(), \"{}\", g(9).is_ok())", Some("g")),
        ] {
            assert!(!reads_owner_result(unbound, &fact(text), owner), "{text}");
        }
        let unrelated = "fn t() { let _ = g(2); let o = other(); let s = \"let q = g(2);\"; }";
        for text in [
            "assert_eq!(other(), 4);",
            "assert!(o > 0)",
            "assert!(q.is_err())",
        ] {
            assert!(
                !reads_owner_result(unrelated, &fact(text), Some("g")),
                "{text}"
            );
        }
    }
}
