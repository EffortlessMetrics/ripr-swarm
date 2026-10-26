//! Whether a function can only return a literal (#10302), so a comparison on
//! its returned value cannot observe any call made inside it.

use super::parse_clean_source_file;
use ra_ap_syntax::{AstNode, SyntaxKind, ast};

/// `true` when `fn_text` declares one function whose every exit yields the
/// same kind of value: a literal tail expression (optionally negated or
/// parenthesised), with no `return`, `?`, macro or closure that could leave
/// with something else. `false` for anything else, including text that does
/// not parse; callers keep their conservative reading then.
pub(crate) fn returns_only_a_literal(fn_text: &str) -> bool {
    let Some(parse) = parse_clean_source_file(fn_text) else {
        return false;
    };
    let Some(function) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return false;
    };
    let Some(body) = function.body() else {
        return false;
    };
    let can_leave_early = body.syntax().descendants().any(|node| {
        matches!(
            node.kind(),
            SyntaxKind::RETURN_EXPR
                | SyntaxKind::TRY_EXPR
                | SyntaxKind::MACRO_CALL
                | SyntaxKind::CLOSURE_EXPR
                | SyntaxKind::BECOME_EXPR
        )
    });
    !can_leave_early && body.tail_expr().is_some_and(|tail| is_literal(&tail))
}

fn is_literal(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::Literal(_) => true,
        ast::Expr::ParenExpr(inner) => inner.expr().is_some_and(|inner| is_literal(&inner)),
        ast::Expr::PrefixExpr(prefix) => {
            prefix.op_kind() == Some(ast::UnaryOp::Neg)
                && prefix.expr().is_some_and(|inner| is_literal(&inner))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::returns_only_a_literal;

    #[test]
    fn a_literal_tail_after_a_discarded_call_is_constant() {
        assert!(returns_only_a_literal(
            "fn check(x: u32) -> u32 {\n    let _ = weight(x);\n    12\n}\n"
        ));
        assert!(returns_only_a_literal("fn c() -> i32 { -(3) }\n"));
    }

    #[test]
    fn anything_that_may_carry_the_owner_value_is_not_constant() {
        for source in [
            "fn c(x: u32) -> u32 { weight(x) }\n",
            "fn c(x: u32) -> u32 { let r = weight(x); r }\n",
            "fn c(x: u32) -> u32 { if x > 1 { return weight(x); } 12 }\n",
            "fn c(x: u32) -> Option<u32> { let _ = weight(x)?; Some(1) }\n",
            "fn c(x: u32) -> u32 { note!(); 12 }\n",
            "fn c(x: u32) -> u32 { let f = || weight(x); f(); 12 }\n",
            "fn c(x: u32) -> u32 { weight(x); }\n",
            "fn c(x: u32) -> u32 { 12 + weight(x) }\n",
            "not a function {",
        ] {
            assert!(!returns_only_a_literal(source), "{source}");
        }
    }
}
