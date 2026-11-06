//! Affirmative identity of an exact method-entry result, separate from its
//! boundary activation and discriminator. Reuse the receiver and hop producers.
use super::*;
use crate::analysis::facts::{OracleFact, TestSummary};
use crate::domain::{OracleKind, RelationReason};
use ra_ap_syntax::{AstNode, ast, ast::HasGenericArgs};

pub(in crate::analysis) fn method_entry_result_identity(
    owner_name: &str,
    chain: &HelperChain,
    related: &[(&TestSummary, RelationReason)],
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    identity: &std::cell::OnceCell<bool>,
) -> bool {
    if assertion.kind != OracleKind::ExactValue
        || !helper_only_reach(related)
        || !super::super::call_identity::closed_method_dispatch_imports(index)
    {
        return false;
    }
    let Some(entry) = chain.hops.last().map(|hop| &hop.caller) else {
        return false;
    };
    let Some(operands) = super::super::reveal::assertion_comparison_operands(&assertion.text)
    else {
        return false;
    };
    let reads_entry = |operand: &str| {
        let source = format!("fn observation() {{ {operand}; }}");
        let Some(parsed) = crate::analysis::syntax::parse_clean_source_file_memoized(&source)
        else {
            return false;
        };
        let Some(function) = parsed.tree().syntax().children().find_map(ast::Fn::cast) else {
            return false;
        };
        let Some(body) = function.body().and_then(|body| body.stmt_list()) else {
            return false;
        };
        let statements = body.statements().collect::<Vec<_>>();
        let [ast::Stmt::ExprStmt(statement)] = statements.as_slice() else {
            return false;
        };
        let Some(mut expression) = statement.expr() else {
            return false;
        };
        while let ast::Expr::ParenExpr(paren) = expression {
            let Some(inner) = paren.expr() else {
                return false;
            };
            expression = inner;
        }
        let ast::Expr::MethodCallExpr(call) = expression else {
            return false;
        };
        call.name_ref()
            .is_some_and(|name| name.text() == entry.name)
            && call.generic_arg_list().is_none()
            && receivers_are_entry_type(index, test, entry, operand)
            && !test_call_is_shadowed_for_entry(index, test, entry, assertion.line)
    };
    let expected = match (reads_entry(operands[0]), reads_entry(operands[1])) {
        (true, false) => operands[1],
        (false, true) => operands[0],
        _ => return false,
    };
    !super::super::owner_pin::expected_reaches_owner(test, &assertion.text, expected, &entry.name)
        && chain_forwards_return_to_observed_hops(owner_name, chain, related, index, identity)
}
