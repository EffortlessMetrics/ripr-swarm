//! Per-version equality for primitive Boolean projections and tuples.
//! Every element must agree on one version of the owner and the equality
//! must change across versions. A permanently wrong sibling masks the result.

use super::{
    FunctionContainer, FunctionSummary, OracleFact, OwnerPinSyntax, OwnerReturnPin, RustIndex,
    TestSummary, assertion_comparison_operands, builtin_type_unshadowed,
    function_declaration_is_async, is_plain_or_std_macro, is_whole_free_owner_call,
    parse_clean_source_file,
};
use crate::analysis::test_grip_evidence::fold_boolean_owner_call;
use ra_ap_syntax::{AstNode, ast};

pub(in crate::analysis) fn boolean_projection_matches_versions(
    owner: &FunctionSummary,
    baseline: &FunctionSummary,
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<(bool, bool)> {
    let unequal = is_plain_or_std_macro(&assertion.text, "assert_ne");
    if !matches!(owner.item.container, FunctionContainer::Free)
        || !(is_plain_or_std_macro(&assertion.text, "assert_eq") || unequal)
    {
        return None;
    }
    let compared = assertion_comparison_operands(&assertion.text)?;
    let cells = if compared
        .iter()
        .all(|side| side.trim_start().starts_with('('))
    {
        tuple_cells(compared)?
    } else {
        let normalized = super::primitive_bool_negated_operands(compared)
            .or_else(|| super::bool_to_u8_operands(compared, index, syntax, test, assertion))
            .unwrap_or(compared);
        let [left, right] = normalized;
        let (call, expected) = match left.parse::<bool>() {
            Ok(expected) => (right, expected),
            Err(_) => (left, right.parse::<bool>().ok()?),
        };
        vec![(call.to_string(), expected)]
    };
    if function_declaration_is_async(owner, index) != Some(false)
        || !(test.start_line..=test.end_line).contains(&assertion.line)
        || !syntax.admits(test, assertion, index)
        || !builtin_type_unshadowed("bool", index, syntax)
        || !primitive_parameters(owner, index, syntax)
    {
        return None;
    }
    let (mut before_matches, mut after_matches) = (true, true);
    for (call, expected) in cells {
        if !is_whole_free_owner_call(&call, &owner.name)
            || !OwnerReturnPin::field_call_binds_owner(owner, index, test, &call, syntax)
        {
            return None;
        }
        let arguments = super::super::activation::call_arguments(&call, &owner.name)?;
        let (Some(before), Some(after)) = (
            fold_boolean_owner_call(baseline, &arguments),
            fold_boolean_owner_call(owner, &arguments),
        ) else {
            return None;
        };
        before_matches &= before == expected;
        after_matches &= after == expected;
    }
    Some((before_matches != unequal, after_matches != unequal))
}

fn tuple_cells([left, right]: [&str; 2]) -> Option<Vec<(String, bool)>> {
    let (left, right) = (tuple(left)?, tuple(right)?);
    let (actual, expected) = if right.fields().all(|field| bool_literal(&field).is_some()) {
        (left, right)
    } else {
        (right, left)
    };
    // The standard tuple PartialEq implementations cover arities through 12.
    let arity = actual.fields().count();
    if !(1..=12).contains(&arity) || arity != expected.fields().count() {
        return None;
    }
    actual
        .fields()
        .zip(expected.fields())
        .map(|(call, literal)| {
            let ast::Expr::CallExpr(call) = call else {
                return None;
            };
            Some((call.syntax().text().to_string(), bool_literal(&literal)?))
        })
        .collect()
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
