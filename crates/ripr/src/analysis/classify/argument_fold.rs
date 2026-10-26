//! RIPR-SPEC-0158: an owner-call argument that computes over the test's own
//! literal bindings (`score(base + 1)` after `let base = 9;`).
//!
//! A computed argument is not one of its operands (RIPR-SPEC-0001, #6672), so
//! the row reader gives it no value. When every operand is a decimal literal
//! or a binding the test declares exactly once, with a plain decimal
//! initializer and no later write, the arithmetic is as exact as the literal
//! `10` would have been, and the shared evaluator folds it. Anything else
//! (a call, a method, a cast, a `mut` or shadowed name, a value that
//! over- or underflows) keeps the unreadable reading.

use super::value_transfer::{EvalOutcome, ExactInputs, TypedValue, evaluate_initializer};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};

/// The decimal value `argument` computes in a test whose body is
/// `test_body`, when every operand is exact; `None` otherwise.
pub(super) fn folded_argument(test_body: &str, argument: &str) -> Option<String> {
    let text = argument.trim();
    if text.is_empty()
        || !text.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || ch == '_'
                || ch.is_whitespace()
                || matches!(ch, '+' | '-' | '*' | '/' | '%' | '(' | ')')
        })
    {
        return None;
    }
    let mut inputs = ExactInputs::new();
    for token in text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')) {
        if token.is_empty() {
            continue;
        }
        if token.starts_with(|ch: char| ch.is_ascii_digit()) {
            // A suffixed or separated literal (`1u32`, `1_000`) is not read.
            if !token.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            continue;
        }
        if inputs.contains_key(token) {
            continue;
        }
        let value = crate::analysis::value_resolution::test_let_bound_literal(test_body, token)?;
        if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        if !bound_once_before_use(test_body, token, text) {
            return None;
        }
        inputs.insert(token.to_string(), value);
    }
    // Arithmetic over literals alone (`10 * 2`) names no binding; activation
    // keeps reading it as before.
    if inputs.is_empty() {
        return None;
    }
    match evaluate_initializer(text, &inputs) {
        EvalOutcome::Exact {
            value: TypedValue::Index(value),
            ..
        } => Some(value.to_string()),
        _ => None,
    }
}

/// Whether `name` is bound by exactly one pattern in the test body, never
/// assigned, and that binding ends before the first place `argument` is
/// written: the `let` itself. A `for` pattern, a closure parameter, an
/// `if let` or a match arm that reuses the name is a second binding that may
/// shadow the `let` at the call, and a `let` that follows the call cannot be
/// the value the call saw (a same-named item may). A body that does not parse
/// counts as rebinding.
fn bound_once_before_use(test_body: &str, name: &str, argument: &str) -> bool {
    let wrapped;
    let source =
        if test_body.trim_start().starts_with("fn ") || test_body.trim_start().starts_with("#[") {
            test_body
        } else {
            wrapped = format!("fn __test() {{\n{test_body}\n}}");
            &wrapped
        };
    let Some(parse) = parse_clean_source_file(source) else {
        return false;
    };
    let mut patterns = Vec::new();
    for node in parse.tree().syntax().descendants() {
        if ast::IdentPat::cast(node.clone())
            .and_then(|pattern| pattern.name())
            .is_some_and(|bound| bound.text() == name)
        {
            patterns.push(usize::from(node.text_range().end()));
        }
        if ast::BinExpr::cast(node).is_some_and(|expr| {
            matches!(expr.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                && expr
                    .lhs()
                    .is_some_and(|lhs| lhs.syntax().text().to_string().trim() == name)
        }) {
            return false;
        }
    }
    let [declared_end] = patterns[..] else {
        return false;
    };
    source
        .match_indices(argument)
        .next()
        .is_some_and(|(at, _)| at >= declared_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(body: &str, argument: &str) -> Option<String> {
        folded_argument(body, argument)
    }

    #[test]
    fn arithmetic_over_a_single_literal_binding_folds() {
        let calls = "score(base + 1); score(base * 2); score((base + 1) * 2); score(base-1); score(2 + 3); score(base + 2 * 3);";
        let body = format!("fn t() {{\n    let base = 9;\n    {calls}\n}}");
        assert_eq!(fold(&body, "base + 1").as_deref(), Some("10"));
        assert_eq!(fold(&body, "base * 2").as_deref(), Some("18"));
        assert_eq!(fold(&body, "(base + 1) * 2").as_deref(), Some("20"));
        assert_eq!(fold(&body, "base-1").as_deref(), Some("8"));
        assert_eq!(fold(&body, "2 + 3"), None, "literals alone name no binding");
        assert_eq!(fold(&body, "base + 2 * 3").as_deref(), Some("15"));
        assert_eq!(
            fold(&body, "base + 7"),
            None,
            "an argument the body never writes"
        );
    }

    #[test]
    fn two_bindings_fold_together() {
        let body = "fn t() {\n    let width = 4;\n    let height = 5;\n    let _ = score(width * height); let _ = score(width * width);\n}";
        assert_eq!(fold(body, "width * height").as_deref(), Some("20"));
        assert_eq!(fold(body, "width * width").as_deref(), Some("16"));
    }

    #[test]
    fn a_binding_that_may_hold_another_value_does_not_fold() {
        let mutable = "fn t() {\n    let mut base = 9;\n    base += 1;\n    score(base + 1);\n}";
        assert_eq!(fold(mutable, "base + 1"), None);
        let shadowed =
            "fn t() {\n    let base = 9;\n    let base = base + 5;\n    score(base + 1);\n}";
        assert_eq!(fold(shadowed, "base + 1"), None);
        let looped = "fn t() {\n    let base = 9;\n    for base in 0..3 {\n        score(base + 1);\n    }\n}";
        assert_eq!(fold(looped, "base + 1"), None);
        let closure = "fn t() {\n    let base = 9;\n    let f = |base: u32| score(base + 1);\n}";
        assert_eq!(fold(closure, "base + 1"), None);
        let pattern = "fn t() {\n    let base = 9;\n    if let Some(base) = lookup() {\n        score(base + 1);\n    }\n}";
        assert_eq!(fold(pattern, "base + 1"), None);
        let later = "fn t() {\n    assert!(score(base + 1));\n    let base = 9;\n}";
        assert_eq!(fold(later, "base + 1"), None, "the let follows the call");
        let assigned = "fn t() {\n    let base = 9;\n    base = 3;\n    score(base + 1);\n}";
        assert_eq!(fold(assigned, "base + 1"), None);
    }

    #[test]
    fn an_operand_that_is_not_a_plain_decimal_binding_does_not_fold() {
        let body = "fn t() {\n    let base = 9;\n    let name = \"ab\";\n    let ratio = 1.5;\n    let sum = base + 1;\n    let big = 9u32;\n}";
        assert_eq!(fold(body, "name + 1"), None);
        assert_eq!(fold(body, "ratio + 1"), None);
        assert_eq!(fold(body, "sum + 1"), None);
        assert_eq!(fold(body, "big + 1"), None);
        assert_eq!(fold(body, "unbound + 1"), None);
        assert_eq!(fold(body, "base + unbound"), None);
    }

    #[test]
    fn a_call_a_cast_or_a_method_around_the_operands_does_not_fold() {
        let body = "fn t() {\n    let base = 9;\n}";
        for argument in [
            "load(base) + 1",
            "base.max(3) + 1",
            "base as u8 + 1",
            "base + 1u32",
            "base + 1_0",
            "base + 1.5",
            "base + helper!()",
            "&base",
            "[base; 2]",
            "-base",
            "",
        ] {
            assert_eq!(fold(body, argument), None, "`{argument}`");
        }
    }

    #[test]
    fn arithmetic_that_does_not_stay_exact_does_not_fold() {
        let body = "fn t() {\n    let base = 9;\n    let zero = 0;\n    let huge = 2147483647;\n}";
        assert_eq!(fold(body, "base - 10"), None, "underflow");
        assert_eq!(fold(body, "base / zero"), None, "division by zero");
        assert_eq!(fold(body, "huge + 1"), None, "past the evaluator's bound");
    }

    #[test]
    fn a_body_that_does_not_parse_folds_nothing() {
        assert_eq!(fold("fn t() { let base = 9; ", "base + 1"), None);
    }
}
