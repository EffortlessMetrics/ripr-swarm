//! Assertion-local identity for a last field write through `&mut self`.
//! Construction and mutation have different origins: a mutation observes the
//! receiver that ran the owner, rather than a value returned by that call.
use super::*;
use crate::analysis::classify::{OwnerPinSyntax, activation, owner_pin, related_tests};
use crate::analysis::facts::FunctionSummary;
use crate::domain::{Probe, ProbeFamily};

/// Establish this exact probe/receiver/field/assertion tuple. Workspace facts
/// remain owned by the index's receiver, field-type, expansion and Drop memos;
/// no affirmative result survives replacement of that index's source bytes.
pub(super) fn field_read_identity(
    probe: &Probe,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &OwnerPinSyntax,
    expansion: &super::super::implicit_effect::ImplicitExpansionContext<'_>,
) -> bool {
    mutation_read(probe, index, test, assertion, syntax, expansion).is_some()
}

fn mutation_read(
    probe: &Probe,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &OwnerPinSyntax,
    expansion: &super::super::implicit_effect::ImplicitExpansionContext<'_>,
) -> Option<()> {
    if probe.family != ProbeFamily::FieldConstruction
        || super::super::field_assignment_target(&probe.expression).is_none()
        || !syntax.admits(test, assertion, index)
    {
        return None;
    }
    let owner = index
        .functions()
        .iter()
        .find(|owner| Some(&owner.id) == probe.owner.as_ref())?;
    let call = OwnerCall::from_symbol(&owner.id.0)?;
    let ty = call.impl_type()?;
    let field = activation::assigned_field_of(owner, probe, index)?;
    // Nested projections and collection/key observers keep their own receipts.
    if field.contains('.')
        || !related_tests::inherent_owner_call_resolves(test, owner, index)
        || !activation::assigned_field_read_confirms(owner, &field, test, assertion, index)
        || crate::analysis::test_grip_evidence::names_non_plain_drop_type(
            owner,
            &format!("{} Self", probe.expression),
            index,
        )
    {
        return None;
    }
    // The receiver authority binds the mutating call, and this shared nominal
    // dispatch gate excludes competing methods. Drop closure above refuses
    // unread generated impls. Getter-specific import exclusions belong only to
    // getter reads, not to a direct field read through an imported owner type.
    if !owner_pin::getter_dispatch_is_unambiguous(owner, index) || !mutating_receiver(owner) {
        return None;
    }
    let tree = parse_clean_source_file(&test.body)?.tree();
    let assertion_call = assertion_macro_at_line(&tree, &test.body, test, assertion)?;
    let visible = visible_statements_before(assertion_call.syntax());
    let operands = super::super::reveal::assertion_comparison_operands(&assertion.text)?;
    for (subject, expected) in [(operands[0], operands[1]), (operands[1], operands[0])] {
        let subject = parsed_operand(subject)?;
        let (read_field, receiver) = match subject {
            ast::Expr::FieldExpr(read) => (read.name_ref()?.text().to_string(), read.expr()?),
            ast::Expr::MethodCallExpr(read) => {
                let read_field = primitive_getter_field(
                    owner,
                    ty,
                    read.name_ref()?.text(),
                    std::slice::from_ref(&field),
                    index,
                    syntax,
                    expansion,
                )?;
                if read.generic_arg_list().is_some() || read.arg_list()?.args().next().is_some() {
                    continue;
                }
                (read_field, read.receiver()?)
            }
            _ => continue,
        };
        let ast::Expr::PathExpr(receiver) = receiver else {
            continue;
        };
        let binding = single_identifier(&receiver)?;
        if read_field != field || !only_mutations(&visible, &binding, &owner.name) {
            continue;
        }
        let expected_expr = parsed_operand(expected)?;
        let primitive =
            owner_pin::indexed_owner_field_builtin_integer(owner, ty, &field, index, syntax)
                .is_some()
                && integer_literal(&expected_expr);
        if primitive || enum_expectation(owner, &field, probe, test, expected, index, syntax) {
            return Some(());
        }
    }
    None
}

fn mutating_receiver(owner: &FunctionSummary) -> bool {
    let Some(parse) = parse_clean_source_file(&owner.body) else {
        return false;
    };
    let Some(function) = parse.tree().syntax().children().find_map(ast::Fn::cast) else {
        return false;
    };
    function.generic_param_list().is_none()
        && function
            .param_list()
            .and_then(|params| params.self_param())
            .is_some_and(|receiver| {
                receiver.amp_token().is_some() && receiver.mut_token().is_some()
            })
}

/// No alias, snapshot, reset, borrowed argument or opaque receiver use may
/// intervene, including before the first owner call. Repeated direct calls on
/// this binding are admitted; each argument is an independent integer literal.
fn only_mutations(visible: &[ast::Stmt], binding: &str, owner: &str) -> bool {
    let Some((at, _)) = binding_let(visible, binding) else {
        return false;
    };
    let mut calls = 0;
    for statement in &visible[at + 1..] {
        if !mentions_identifier(statement.syntax(), binding) {
            continue;
        }
        let ast::Stmt::ExprStmt(statement) = statement else {
            return false;
        };
        let Some(ast::Expr::MethodCallExpr(call)) = statement.expr() else {
            return false;
        };
        let Some(ast::Expr::PathExpr(receiver)) = call.receiver() else {
            return false;
        };
        if single_identifier(&receiver).as_deref() != Some(binding)
            || call.name_ref().is_none_or(|name| name.text() != owner)
            || call.generic_arg_list().is_some()
            || call
                .arg_list()
                .is_none_or(|args| !args.args().all(|arg| integer_literal(&arg)))
        {
            return false;
        }
        calls += 1;
    }
    calls > 0
}

fn integer_literal(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::Literal(literal) => matches!(literal.kind(), ast::LiteralKind::IntNumber(_)),
        ast::Expr::PrefixExpr(prefix) if prefix.op_token().is_some_and(|op| op.text() == "-") => {
            prefix.expr().is_some_and(|value| integer_literal(&value))
        }
        _ => false,
    }
}

fn enum_expectation(
    owner: &FunctionSummary,
    field: &str,
    probe: &Probe,
    test: &TestSummary,
    expected: &str,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    let Some(ty) = OwnerCall::from_symbol(&owner.id.0).and_then(|call| call.impl_type()) else {
        return false;
    };
    let Some(item) = declared_once(index, ty) else {
        return false;
    };
    if item.generic_param_list().is_some() {
        return false;
    }
    let Some(ast::FieldList::RecordFieldList(fields)) = item.field_list() else {
        return false;
    };
    let Some(slot) = fields
        .fields()
        .find(|slot| slot.name().is_some_and(|name| name.text() == field))
    else {
        return false;
    };
    let Some(slot_type) = slot.ty().map(|ty| ty.syntax().text().to_string()) else {
        return false;
    };
    let Some((expected_type, variant)) = expected.trim().split_once("::") else {
        return false;
    };
    if !is_identifier(expected_type)
        || !is_identifier(variant)
        || slot.attrs().next().is_some()
        || slot_type != expected_type
        || !owner_pin::error_enum_compares_payload(expected_type, index)
        || !owner_pin::owner_builds_declared_error_variant(expected, owner, index)
        || !owner_pin::test_names_declared_type(test, expected_type, owner, index, syntax)
    {
        return false;
    }
    // The bounded enum slice observes exactly the assigned unit variant.
    // A different or expected-side owner-derived value supplies no receipt.
    let Some(rhs) = probe.expression.split_once('=').map(|(_, rhs)| rhs.trim()) else {
        return false;
    };
    let Some(mut value) = parsed_operand(rhs) else {
        return false;
    };
    while let ast::Expr::ParenExpr(paren) = value {
        let Some(inner) = paren.expr() else {
            return false;
        };
        value = inner;
    }
    matches!(value, ast::Expr::PathExpr(_)) && value.syntax().text() == expected.trim()
}
