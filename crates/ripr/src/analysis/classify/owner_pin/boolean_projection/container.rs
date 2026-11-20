//! The standard single-element receivers whose exact Boolean payload is compared.

use super::{
    ComparedCall, OracleFact, OwnerPinSyntax, RustIndex, TestSummary, bool_literal, operand,
    standard_method_available, standard_variant_in_scope,
};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs},
};

pub(super) fn recognizes(side: &str) -> bool {
    [".unwrap_or(", ".eq(", ".contains(", ".first("]
        .iter()
        .any(|method| side.contains(method))
}

pub(super) fn comparison(
    projection: &str,
    expected: &str,
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<ComparedCall> {
    let ast::Expr::MethodCallExpr(method) = operand(projection)? else {
        return None;
    };
    if method.generic_arg_list().is_some() {
        return None;
    }
    let receiver = method.receiver()?;
    let arguments: Vec<_> = method.arg_list()?.args().collect();
    let method_name = method.name_ref()?;
    let uses_some = matches!(method_name.text(), "eq" | "unwrap_or" | "first");
    if uses_some && !standard_variant_in_scope("Some", test, assertion, index, syntax) {
        return None;
    }
    let (call, expectation) = match (method_name.text(), arguments.as_slice()) {
        ("unwrap_or", [fallback]) => {
            bool_literal(fallback)?;
            (some_argument(receiver)?, bool_literal(&operand(expected)?)?)
        }
        ("eq", [expected_option]) => {
            if !standard_method_available("eq", test, assertion, index, syntax) {
                return None;
            }
            let option = shared_reference(expected_option.clone())?;
            let peer = bool_literal(&some_argument(option)?)?;
            let result = bool_literal(&operand(expected)?)?;
            (some_argument(receiver)?, peer == result)
        }
        ("first", []) => {
            if !standard_method_available("first", test, assertion, index, syntax) {
                return None;
            }
            let ast::Expr::ArrayExpr(array) = receiver else {
                return None;
            };
            let ast::ArrayExprKind::ElementList(elements) = array.kind() else {
                return None;
            };
            let mut elements = elements;
            let call = elements.next()?;
            if elements.next().is_some() {
                return None;
            }
            let expected = shared_reference(some_argument(operand(expected)?)?)?;
            (call, bool_literal(&expected)?)
        }
        ("contains", [needle]) => {
            if !standard_method_available("contains", test, assertion, index, syntax)
                || !syntax.targeted_macro_binding_is_unambiguous(test, assertion.line, "vec", index)
            {
                return None;
            }
            let ast::Expr::MacroExpr(vector) = receiver else {
                return None;
            };
            let call = vector.macro_call()?;
            if call.path()?.syntax().text() != "vec" {
                return None;
            }
            let elements = super::encoding::macro_operands(&call)?;
            let [element] = elements.as_slice() else {
                return None;
            };
            let needle = bool_literal(&shared_reference(needle.clone())?)?;
            (
                operand(element)?,
                needle == bool_literal(&operand(expected)?)?,
            )
        }
        _ => return None,
    };
    let ast::Expr::CallExpr(call) = call else {
        return None;
    };
    Some(ComparedCall::equals(
        call.syntax().text().to_string(),
        expectation,
    ))
}

fn some_argument(expr: ast::Expr) -> Option<ast::Expr> {
    let ast::Expr::CallExpr(some) = expr else {
        return None;
    };
    if some.expr()?.syntax().text() != "Some" {
        return None;
    }
    let args = some.arg_list()?;
    let mut args = args.args();
    let argument = args.next()?;
    (args.next().is_none()).then_some(argument)
}

fn shared_reference(expr: ast::Expr) -> Option<ast::Expr> {
    let ast::Expr::RefExpr(reference) = expr else {
        return None;
    };
    if reference.mut_token().is_some() || reference.raw_token().is_some() {
        return None;
    }
    reference.expr()
}
