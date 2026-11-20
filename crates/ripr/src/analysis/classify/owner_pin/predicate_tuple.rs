//! Exact equality of a tuple of Boolean owner calls with a tuple of literals.
//! Every element must agree on one version of the owner and the equality
//! must change across versions. A permanently wrong sibling masks the result.

use super::{
    FunctionSummary, OwnerPinSyntax, OwnerReturnPin, Probe, RustIndex, TestSummary,
    builtin_type_unshadowed, function_declaration_is_async, is_whole_free_owner_call,
    parse_clean_source_file,
};
use crate::analysis::test_grip_evidence::{baseline_owner, fold_boolean_owner_call};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

pub(super) fn distinguishes(
    owner: &FunctionSummary,
    probe: &Probe,
    test: &TestSummary,
    compared: [&str; 2],
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    let [left, right] = compared;
    let (Some(left), Some(right)) = (tuple(left), tuple(right)) else {
        return false;
    };
    let (actual, expected) = if right.fields().all(|field| bool_literal(&field).is_some()) {
        (left, right)
    } else {
        (right, left)
    };
    // The standard tuple PartialEq implementations cover arities through 12.
    let arity = actual.fields().count();
    if !(1..=12).contains(&arity)
        || arity != expected.fields().count()
        || function_declaration_is_async(owner, index) != Some(false)
        || !primitive_parameters(owner, index, syntax)
    {
        return false;
    }
    let Some(baseline) = baseline_owner(owner, probe.before.as_deref(), probe.after.as_deref())
    else {
        return false;
    };
    let (mut before_matches, mut after_matches) = (true, true);
    for (call, literal) in actual.fields().zip(expected.fields()) {
        let (ast::Expr::CallExpr(call), Some(expected)) = (call, bool_literal(&literal)) else {
            return false;
        };
        let call_text = call.syntax().text().to_string();
        if !is_whole_free_owner_call(&call_text, &owner.name)
            || !OwnerReturnPin::field_call_binds_owner(owner, index, test, &call_text, syntax)
        {
            return false;
        }
        let Some(arguments) = call.arg_list() else {
            return false;
        };
        let arguments = arguments
            .args()
            .map(|arg| arg.syntax().text().to_string())
            .collect::<Vec<_>>();
        let (Some(before), Some(after)) = (
            fold_boolean_owner_call(&baseline, &arguments),
            fold_boolean_owner_call(owner, &arguments),
        ) else {
            return false;
        };
        before_matches &= before == expected;
        after_matches &= after == expected;
    }
    before_matches != after_matches
}

fn bool_literal(expr: &ast::Expr) -> Option<bool> {
    let ast::Expr::Literal(literal) = expr else {
        return None;
    };
    match literal.kind() {
        ast::LiteralKind::Bool(value) => Some(value),
        _ => None,
    }
}

fn tuple(text: &str) -> Option<ast::TupleExpr> {
    let source = format!("fn ripr_tuple() {{ {text} }}");
    super::tuple_tail(&source)
}

fn primitive_parameters(
    owner: &FunctionSummary,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    let Some(function) = parse_clean_source_file(&owner.body)
        .and_then(|parse| parse.tree().syntax().children().find_map(ast::Fn::cast))
    else {
        return false;
    };
    function.param_list().is_some_and(|params| {
        params.params().all(|param| {
            param.ty().is_some_and(|ty| {
                builtin_type_unshadowed(&ty.syntax().text().to_string(), index, syntax)
            })
        })
    })
}
