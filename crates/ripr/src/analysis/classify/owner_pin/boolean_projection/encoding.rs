//! Exactly how a standard scalar consumer compares a Boolean predicate's value.

use super::{ComparedCall, OracleFact, OwnerPinSyntax, RustIndex, TestSummary, operand};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

pub(super) fn recognizes(side: &str) -> bool {
    let side = side.trim_start();
    side.starts_with("u8::from(") || side.starts_with("format!(")
}

pub(super) fn comparison(
    projection: &str,
    expected: &str,
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<ComparedCall> {
    let (call, when_false, when_true) = match operand(projection)? {
        ast::Expr::CallExpr(from) => {
            if from.expr()?.syntax().text() != "u8::from"
                || !super::super::builtin_type_unshadowed("u8", index, syntax)
                || !super::standard_variant_in_scope("From", test, assertion, index, syntax)
                || !super::standard_method_available("from", test, assertion, index, syntax)
            {
                return None;
            }
            let args = from.arg_list()?;
            let mut args = args.args();
            let call = args.next()?;
            if args.next().is_some() {
                return None;
            }
            let ast::Expr::Literal(value) = operand(expected)? else {
                return None;
            };
            let ast::LiteralKind::IntNumber(number) = value.kind() else {
                return None;
            };
            if number.suffix().is_some_and(|suffix| suffix != "u8") {
                return None;
            }
            let expected = u8::try_from(number.value().ok()?).ok()?;
            (call, expected == 0, expected == 1)
        }
        ast::Expr::MacroExpr(fmt) => {
            let invocation = fmt.macro_call()?;
            if invocation.path()?.syntax().text() != "format"
                || !syntax.targeted_macro_binding_is_unambiguous(
                    test,
                    assertion.line,
                    "format",
                    index,
                )
            {
                return None;
            }
            let args = macro_operands(&invocation)?;
            let [template, call] = args.as_slice() else {
                return None;
            };
            let template = string_literal(template)?;
            let expected = string_literal(expected)?;
            let (prefix, suffix) = template.split_once("{}")?;
            if [prefix, suffix]
                .iter()
                .any(|part| part.contains(['{', '}']))
            {
                return None;
            }
            (
                operand(call)?,
                format!("{prefix}false{suffix}") == expected,
                format!("{prefix}true{suffix}") == expected,
            )
        }
        _ => return None,
    };
    let ast::Expr::CallExpr(call) = call else {
        return None;
    };
    Some(ComparedCall {
        call: call.syntax().text().to_string(),
        when_false,
        when_true,
    })
}

pub(super) fn string_literal(text: &str) -> Option<String> {
    let ast::Expr::Literal(value) = operand(text)? else {
        return None;
    };
    let ast::LiteralKind::String(string) = value.kind() else {
        return None;
    };
    Some(string.value().ok()?.into_owned())
}

// Parse only the expression list reached through the actual macro token tree.
// A tolerant fallback for unknown macro grammars cannot authenticate a result.
pub(super) fn macro_operands(call: &ast::MacroCall) -> Option<Vec<String>> {
    let args = call.token_tree()?.syntax().text().to_string();
    let inner = args.get(1..args.len().checked_sub(1)?)?;
    let tuple = match operand(&format!("({inner})"))? {
        ast::Expr::TupleExpr(tuple) => tuple,
        ast::Expr::ParenExpr(single) => {
            return Some(vec![single.expr()?.syntax().text().to_string()]);
        }
        _ => return None,
    };
    Some(
        tuple
            .fields()
            .map(|arg| arg.syntax().text().to_string())
            .collect(),
    )
}
