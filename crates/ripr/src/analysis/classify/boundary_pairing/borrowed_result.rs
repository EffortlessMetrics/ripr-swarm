//! Pair a borrowed boolean comparison with the exact argument it observes.
//!
//! HelperLoan supplies execution, macro binding and helper identity. This
//! module supplies the missing value edge, never generic helper reach.
use super::*;
use crate::analysis::syntax::{
    parse_clean_source_file_memoized as parse_clean_source_file, ra::LineIndex,
};
use ra_ap_syntax::ast::{HasArgList, HasGenericArgs, HasName};

pub(super) fn pairs(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    loan: &HelperLoan,
    assertion: &OracleFact,
    index: &RustIndex,
) -> bool {
    let Some(pin) = super::super::owner_pin::OwnerReturnPin::establish(probe, owner, index) else {
        return false;
    };
    if !pin.is_unconditional_boolean_tail()
        || super::super::owner_pin::wrapper_may_be_rebound("bool", index)
    {
        return false;
    }
    let Some((result_slot, expected_slot, callback)) = slots(loan, assertion) else {
        return false;
    };
    let Some(parsed) = parse_clean_source_file(&test.body) else {
        return false;
    };
    let Some(function) = parsed.tree().syntax().children().find_map(ast::Fn::cast) else {
        return false;
    };
    let Some(body) = function.body() else {
        return false;
    };
    let lines = LineIndex::new(&test.body);
    let scope = super::super::argument_fold::FoldScope::of_test(test, index);
    body.statements().any(|statement| {
        let ast::Stmt::ExprStmt(statement) = statement else { return false };
        let Some(ast::Expr::CallExpr(call)) = statement.expr() else { return false };
        let line = test.start_line + lines.line(call.syntax().text_range().start()) - 1;
        if !loan.eager_call_lines.contains(&line)
            || call.expr().is_none_or(|callee| callee.syntax().text() != loan.name.as_str())
        { return false; }
        let arguments = call.arg_list().map(|list| list.args().collect::<Vec<_>>()).unwrap_or_default();
        if arguments.len() != 2 { return false; }
        let expected = arguments[expected_slot].syntax().text().to_string();
        if !matches!(expected.trim(), "true" | "false") { return false; }
        let mut result = arguments[result_slot].clone();
        if callback {
            let ast::Expr::ClosureExpr(closure) = result else { return false };
            if closure.async_token().is_some()
                || closure.param_list().is_none_or(|list| list.params().next().is_some())
            { return false; }
            let Some(value) = closure.body() else { return false };
            result = value;
        } else if let ast::Expr::MethodCallExpr(clone) = &result {
            // The established owner returns primitive bool, whose std Clone
            // preserves the value. A disabled prelude has different dispatch.
            if clone.name_ref().is_none_or(|name| name.text() != "clone")
                || clone.generic_arg_list().is_some()
                || clone.arg_list().is_none_or(|list| list.args().next().is_some())
                || index.files().values().any(|file| file.source.contains("no_implicit_prelude"))
            { return false; }
            let Some(receiver) = clone.receiver() else { return false };
            result = receiver;
        }
        let ast::Expr::CallExpr(owner_call) = result else { return false };
        let text = owner_call.syntax().text().to_string();
        if !super::super::call_identity::boundary_owner_calls_resolve(index, owner, test, &text, line) {
            return false;
        }
        let sites = owner_call_sites(&text, owner);
        matches!(sites.as_slice(), [arguments] if argument_list_activates_boundary(probe, owner, test, arguments, scope))
    })
}

/// Exactly one bool equality, optionally preceded by `let got = f();`.
/// No statements can overwrite the result or substitute an unrelated observer.
fn slots(loan: &HelperLoan, assertion: &OracleFact) -> Option<(usize, usize, bool)> {
    if loan.parameters.len() != 2 {
        return None;
    }
    let parsed = parse_clean_source_file(&loan.helper)?;
    let helper = parsed.tree().syntax().children().find_map(ast::Fn::cast)?;
    let params = helper.param_list()?.params().collect::<Vec<_>>();
    let body = helper.body()?.stmt_list()?;
    if body.tail_expr().is_some() {
        return None;
    }
    let statements = body.statements().collect::<Vec<_>>();
    let (binding, oracle) = match statements.as_slice() {
        [ast::Stmt::ExprStmt(oracle)] => (None, oracle),
        [ast::Stmt::LetStmt(binding), ast::Stmt::ExprStmt(oracle)] => (Some(binding), oracle),
        _ => return None,
    };
    let ast::Expr::MacroExpr(oracle) = oracle.expr()? else {
        return None;
    };
    let oracle = oracle.macro_call()?;
    if oracle.syntax().text().to_string().trim_end_matches(';')
        != assertion.text.trim().trim_end_matches(';')
    {
        return None;
    }
    let compared = assertion_comparison_operands(&assertion.text)?;
    let compact_type = |slot: usize| -> Option<String> {
        Some(
            params
                .get(slot)?
                .ty()?
                .syntax()
                .text()
                .to_string()
                .split_whitespace()
                .collect(),
        )
    };
    for expected in 0..2 {
        let result = 1 - expected;
        if compact_type(expected).as_deref() != Some("bool") {
            continue;
        }
        let Some(other) = compared
            .iter()
            .position(|operand| operand.trim() == loan.parameters[expected])
        else {
            continue;
        };
        let result_operand = compared[1 - other].trim();
        if let Some(binding) = binding {
            let ast::Pat::IdentPat(name) = binding.pat()? else {
                continue;
            };
            if name.mut_token().is_some()
                || name.ref_token().is_some()
                || name.name()?.text() != result_operand
                || compact_type(result).as_deref() != Some("implFnOnce()->bool")
            {
                continue;
            }
            let ast::Expr::CallExpr(call) = binding.initializer()? else {
                continue;
            };
            if call.expr()?.syntax().text() == loan.parameters[result].as_str()
                && call.arg_list()?.args().next().is_none()
            {
                return Some((result, expected, true));
            }
        } else if result_operand == loan.parameters[result]
            && compact_type(result).as_deref() == Some("bool")
        {
            return Some((result, expected, false));
        }
    }
    None
}
