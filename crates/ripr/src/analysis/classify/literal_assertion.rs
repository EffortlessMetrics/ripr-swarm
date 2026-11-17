//! #14968: an unconditional literal-only assertion observes nothing.
//!
//! `assert!(true)`, `assert!(false)` or `assert_eq!(2 + 2, 4)` written as a
//! top-level statement of the test body reads no runtime value: it neither
//! discriminates the owner's behavior nor depends on it. Reveal treats such an
//! assertion as absent, so the test reads exactly as it would without it.
//!
//! The same literal inside an `if`, `match` arm, loop or closure is not
//! dropped: there it runs only on a path the test's own reads select
//! (`if let Ok(_) = g(2) { assert!(false) }`), so it can still observe the
//! owner through its guard.

use crate::analysis::extract::assertion_compares_only_literals;
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
