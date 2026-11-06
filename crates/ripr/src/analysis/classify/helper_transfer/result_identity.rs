//! Affirmative identity of an exact method-entry result, separate from its
//! boundary activation and discriminator. Reuse the receiver and hop producers.
use super::*;
use crate::analysis::facts::{OracleFact, TestSummary};
use crate::domain::{OracleKind, RelationReason};
use ra_ap_syntax::{AstNode, ast, ast::HasGenericArgs};

/// The existing asserting-entry producer proves the helper's sole assertion
/// observes the indexed method result. The loan binds that exact helper back
/// to an eager call in this test; it is not mere helper-name affinity.
pub(in crate::analysis) fn asserting_entry_result_identity(
    owner_name: &str,
    chain: &HelperChain,
    test: &TestSummary,
    assertion: &OracleFact,
    loan: &crate::analysis::syntax::HelperLoan,
    index: &RustIndex,
    expansion: &super::super::implicit_effect::ImplicitExpansionContext<'_>,
) -> bool {
    assertion.kind == OracleKind::ExactValue
        && !loan.eager_call_lines.is_empty()
        && super::super::call_identity::closed_method_dispatch_imports(index)
        && asserting_method_entry(owner_name, &chain.hops, index).is_some_and(|top| {
            loan.name == top.caller.name
                && loan.helper == top.caller.body.as_str()
                && loan.helper.contains(&assertion.text)
                && independent_expected_argument(top, test, assertion, loan, expansion)
        })
}

fn independent_expected_argument(
    top: &HelperHop,
    test: &TestSummary,
    assertion: &OracleFact,
    loan: &crate::analysis::syntax::HelperLoan,
    expansion: &super::super::implicit_effect::ImplicitExpansionContext<'_>,
) -> bool {
    let expected = loan
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, name)| {
            top.receiver.as_deref() != Some(name.as_str())
                && !top
                    .arguments
                    .iter()
                    .any(|argument| argument.trim() == name.as_str())
        })
        .collect::<Vec<_>>();
    let [(position, name)] = expected.as_slice() else {
        return false;
    };
    let Some(operands) = super::super::reveal::assertion_comparison_operands(&assertion.text)
    else {
        return false;
    };
    if !operands.iter().any(|operand| {
        operand.trim() == name.as_str()
            || (operand.trim() == format!("{name}.to_string()")
                && expansion.admits_getter_dispatch("str"))
    }) {
        return false;
    }
    // Every eager call must supply an independent literal. Otherwise a far
    // literal call could lend its helper oracle to a self-computed boundary row.
    loan.eager_call_lines.iter().all(|line| {
        let calls = test
            .calls
            .iter()
            .filter(|call| call.line == *line && call.name == loan.name)
            .collect::<Vec<_>>();
        let [call] = calls.as_slice() else {
            return false;
        };
        let Some(arguments) = split_call_arguments_text(&call.text, &loan.name) else {
            return false;
        };
        arguments.len() == loan.parameters.len()
            && arguments.get(*position).is_some_and(|argument| {
                matches!(whole_expression(argument), Some(ast::Expr::Literal(literal))
                    if matches!(literal.kind(), ast::LiteralKind::String(_)))
            })
    })
}

fn whole_expression(text: &str) -> Option<ast::Expr> {
    let parsed = crate::analysis::syntax::parse_clean_source_file_memoized(&format!(
        "fn observation() {{ {text}; }}"
    ))?;
    let function = parsed.tree().syntax().children().find_map(ast::Fn::cast)?;
    let statements = function
        .body()?
        .stmt_list()?
        .statements()
        .collect::<Vec<_>>();
    let [ast::Stmt::ExprStmt(statement)] = statements.as_slice() else {
        return None;
    };
    statement.expr()
}

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
        let Some(mut expression) = whole_expression(operand) else {
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
