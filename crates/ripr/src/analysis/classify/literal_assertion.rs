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
    assertion_compared_operands, assertion_compares_only_literals, assertion_oracle_text,
    mask_comments_and_strings,
};
use crate::analysis::facts::{OracleFact, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{AstNode, ast, ast::HasArgList, ast::HasGenericArgs};

/// Whether `assertion` compares only literals and is itself a top-level
/// statement (or the tail expression) of `test`'s body, located at the
/// assertion's own line. An assertion produced from elsewhere (a helper or
/// macro expansion), an unparseable body, or any nested position answers
/// `false`, which keeps the assertion as before.
pub(super) fn is_unconditional_literal_only_assertion(
    test: &TestSummary,
    assertion: &OracleFact,
) -> bool {
    assertion_compares_only_literals(&assertion.text) && is_unconditional_statement(test, assertion)
}

/// #15476: whether `assertion` is an unconditional top-level statement of
/// `test` whose compared operands compute a constant: literals, `!`, `-`,
/// parentheses and binary operators over them, and at least one call of a
/// function `closed_callee` vouches for, given literal-only (or again closed)
/// arguments (`assert_eq!(label(0), "zero")`).
///
/// `closed_callee(path)` answers for the spelled path of each called
/// function (`label`, `crate::label`).
/// It must hold only for a function whose result is a pure function of its
/// primitive arguments: then the operand's value is the same constant on
/// every run, so the assertion reads no runtime value of the changed code,
/// exactly like a literal-only assertion (#14968). Any other expression (a
/// binding, a field, a method call, a reference, a macro, a turbofish or a
/// type-qualified path) leaves the assertion as it was.
pub(super) fn is_unconditional_closed_call_assertion(
    test: &TestSummary,
    assertion: &OracleFact,
    closed_callee: &dyn Fn(&str) -> bool,
) -> bool {
    let Some(operands) = assertion_compared_operands(&assertion.text) else {
        return false;
    };
    let mut calls = 0usize;
    !operands.is_empty()
        && operands.iter().all(|operand| {
            parse_operand(operand)
                .is_some_and(|expr| closed_operand(&expr, closed_callee, &mut calls, 0))
        })
        && calls > 0
        && is_unconditional_statement(test, assertion)
}

/// Nesting deeper than this is not read.
const CLOSED_OPERAND_DEPTH: usize = 16;

fn closed_operand(
    expr: &ast::Expr,
    closed_callee: &dyn Fn(&str) -> bool,
    calls: &mut usize,
    depth: usize,
) -> bool {
    if depth > CLOSED_OPERAND_DEPTH {
        return false;
    }
    let operand = |expr: Option<ast::Expr>, calls: &mut usize| {
        expr.is_some_and(|expr| closed_operand(&expr, closed_callee, calls, depth + 1))
    };
    match expr {
        ast::Expr::Literal(_) => true,
        ast::Expr::ParenExpr(paren) => operand(paren.expr(), calls),
        ast::Expr::PrefixExpr(prefix) => {
            matches!(
                prefix.op_kind(),
                Some(ast::UnaryOp::Not | ast::UnaryOp::Neg)
            ) && operand(prefix.expr(), calls)
        }
        ast::Expr::BinExpr(binary) => {
            !matches!(
                binary.op_kind(),
                None | Some(ast::BinaryOp::Assignment { .. })
            ) && operand(binary.lhs(), calls)
                && operand(binary.rhs(), calls)
        }
        ast::Expr::CallExpr(call) => {
            let Some(name) = call.expr().and_then(|callee| closed_callee_name(&callee)) else {
                return false;
            };
            let Some(arguments) = call.arg_list() else {
                return false;
            };
            if !closed_callee(&name) {
                return false;
            }
            *calls += 1;
            arguments
                .args()
                .all(|argument| operand(Some(argument), calls))
        }
        _ => false,
    }
}

/// The spelled path of a plain function call: `label`, `crate::label`,
/// `demo::label`, `super::label`. A segment with generic arguments, a
/// type-like (uppercase) qualifier, `<T as Trait>` or a raw identifier is
/// refused, since it may name an associated or foreign function.
fn closed_callee_name(callee: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(path) = callee else {
        return None;
    };
    let path = path.path()?;
    let mut segment_path = Some(path.clone());
    while let Some(current) = segment_path {
        let segment = current.segment()?;
        if segment.generic_arg_list().is_some()
            || segment.type_anchor().is_some()
            || segment.parenthesized_arg_list().is_some()
        {
            return None;
        }
        let word = match segment.name_ref() {
            Some(name) => name.text().to_string(),
            None => segment.syntax().text().to_string(),
        };
        if word.starts_with("r#") || word.starts_with(|ch: char| ch.is_ascii_uppercase()) {
            return None;
        }
        segment_path = current.qualifier();
    }
    let spelled: String = path
        .syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect();
    (!spelled.is_empty() && path.segment()?.name_ref().is_some()).then_some(spelled)
}

fn parse_operand(operand: &str) -> Option<ast::Expr> {
    let wrapped = format!("fn ripr_closed_operand() {{ let _ = {operand}\n; }}");
    let parse = parse_clean_source_file(&wrapped)?;
    parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?
        .initializer()
}

/// Whether `assertion` is itself a top-level statement (or the tail
/// expression) of `test`'s body, located at the assertion's own line.
fn is_unconditional_statement(test: &TestSummary, assertion: &OracleFact) -> bool {
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
    // The fact records only a line and text, so any second macro call on the
    // same line with the same squashed text, nested (`if g(2) {
    // assert_eq!("a b", "ab"); } assert_eq!("ab", "ab");`, a closure body)
    // or not, may be the fact's subject. Keep it as before (fail closed).
    let twins = list
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| at_assertion(call.syntax()))
        .count();
    top_level && twins <= 1
}

/// Whether `test` holds at least one assertion and every one is an
/// unconditional literal-only assertion that reveal drops (#14968), so the
/// test reads nothing. A `reachable_unrevealed` finding whose related tests
/// are all such tests had its only credit from that literal, and keeps the
/// reach limitation the credited reading named (RIPR-SPEC-0094).
pub(in crate::analysis) fn asserts_only_dropped_literals(test: &TestSummary) -> bool {
    !test.assertions.is_empty()
        && test
            .assertions
            .iter()
            .all(|assertion| is_unconditional_literal_only_assertion(test, assertion))
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

    fn summary(body: &str) -> TestSummary {
        TestSummary {
            name: "t".to_string(),
            body: body.to_string().into(),
            assertions: Vec::new(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: 1 + body.matches('\n').count(),
            calls: crate::analysis::facts::SharedFactSlice::empty(),
            literals: crate::analysis::facts::SharedFactSlice::empty(),
            attrs: crate::analysis::facts::SharedFactSlice::empty(),
            nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
            let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
            generated: None,
        }
    }

    /// #15476: an unconditional assertion over calls the callee predicate
    /// vouches for, on literal arguments, is a constant; anything that may
    /// read a runtime value, or sits on a path a read selects, is not.
    #[test]
    fn closed_call_assertions_are_constants_only_on_literal_arguments() {
        let closed = |path: &str| path.rsplit("::").next() == Some("label");
        let constant = |assertion: &str| {
            let body = format!("fn t() {{\n    let _ = discount(50);\n    {assertion}\n}}");
            let mut fact = fact(assertion);
            fact.line = 3;
            is_unconditional_closed_call_assertion(&summary(&body), &fact, &closed)
        };
        for assertion in [
            "assert_eq!(label(0), \"zero\");",
            "assert_eq!(crate::label(0), \"zero\");",
            "assert_eq!(demo::label(0), \"zero\");",
            "assert_ne!(label(1 + 2), \"zero\");",
            "assert!(label(0) == \"zero\");",
            "assert!(!(label(0) == \"many\"));",
            "assert_eq!(label(-1), \"zero\", \"{:?}\", discount(50));",
        ] {
            assert!(constant(assertion), "{assertion}");
        }
        for assertion in [
            // The owner, or any function the predicate does not vouch for.
            "assert_eq!(discount(50), 50);",
            "assert_eq!(label(discount(50)), \"zero\");",
            // A binding, field, method call or reference may carry state.
            "assert_eq!(label(n), \"zero\");",
            "assert_eq!(label(cart.n), \"zero\");",
            "assert_eq!(cart.label(0), \"zero\");",
            "assert_eq!(label(0).len(), 4);",
            "assert_eq!(label(*n), \"zero\");",
            // Type-qualified, generic or raw paths may name another function.
            "assert_eq!(Demo::label(0), \"zero\");",
            "assert_eq!(label::<u8>(0), \"zero\");",
            "assert_eq!(r#label(0), \"zero\");",
            // Nested macros, literal-only assertions and other macros.
            "assert_eq!(label(0), format!(\"zero\"));",
            "assert_eq!(1, 1);",
            "assert!(matches!(label(0), \"zero\"));",
            "check_eq!(label(0), \"zero\");",
        ] {
            assert!(!constant(assertion), "{assertion}");
        }
        // The same constant on a path the owner's value selects is not
        // dropped: there it runs only when that read allows.
        let guarded = "fn t() {\n    if discount(50) > 1 {\n        assert_eq!(label(0), \"many\");\n    }\n}";
        let mut fact = fact("assert_eq!(label(0), \"many\");");
        fact.line = 3;
        assert!(!is_unconditional_closed_call_assertion(
            &summary(guarded),
            &fact,
            &closed
        ));
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
