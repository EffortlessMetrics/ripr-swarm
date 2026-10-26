//! Field-helper comparisons need retained bindings and eager test execution.
//! Result helpers keep the existing resolver and ordinary alignment route.

use super::super::module_constants::PythonTestRebinding;
use super::super::source_facts::parse_module_result;
use super::super::source_utils::{SourceText, text_for_range};
use super::super::{PythonHelperOrigin, PythonTest};
use rustpython_parser::ast::{Expr, Mod, Ranged, Stmt};
use std::{collections::BTreeSet, path::Path};

pub(super) fn origin(
    test: &PythonTest,
    function: &Stmt,
    gate: &str,
    field_observer: bool,
    source: &SourceText<'_>,
) -> PythonHelperOrigin {
    if !field_observer {
        return PythonHelperOrigin::ValueHelper;
    }
    let Ok(Mod::Module(parsed)) = parse_module_result(Path::new("<field-helper>"), gate) else {
        return PythonHelperOrigin::FieldHelperRefused;
    };
    let [Stmt::Assert(assertion)] = parsed.body.as_slice() else {
        return PythonHelperOrigin::FieldHelperRefused;
    };
    let rebinding = &test.constant_rebinding;
    let admitted = literal_field(&assertion.test).is_some_and(|field| {
        !rebinding.assigns_attribute(field)
            && !rebinding.opaque
            && !rebinding
                .names
                .iter()
                .any(|name| rebinding.module_bound.contains(name))
            && !rebinding
                .attributes
                .iter()
                .any(|name| name.starts_with("__"))
            && test.decorators.is_empty()
            && eager_test(function, rebinding, source)
    });
    if admitted {
        PythonHelperOrigin::FieldHelperSupported
    } else {
        PythonHelperOrigin::FieldHelperRefused
    }
}

fn literal_field(expr: &Expr) -> Option<&str> {
    let Expr::Compare(compare) = expr else {
        return None;
    };
    let [other] = compare.comparators.as_slice() else {
        return None;
    };
    let field = match (compare.left.as_ref(), other) {
        (Expr::Attribute(field), Expr::Constant(_))
        | (Expr::Constant(_), Expr::Attribute(field)) => field,
        _ => return None,
    };
    matches!(field.value.as_ref(), Expr::Name(_)).then_some(field.attr.as_str())
}

pub(super) fn reads_field(expr: &Expr) -> bool {
    match expr {
        Expr::Attribute(_) | Expr::Subscript(_) => true,
        Expr::UnaryOp(expr) => reads_field(&expr.operand),
        Expr::BinOp(expr) => reads_field(&expr.left) || reads_field(&expr.right),
        Expr::Compare(expr) => reads_field(&expr.left) || expr.comparators.iter().any(reads_field),
        Expr::Tuple(expr) => expr.elts.iter().any(reads_field),
        Expr::List(expr) => expr.elts.iter().any(reads_field),
        // A call's returned value remains the ordinary result-helper route.
        _ => false,
    }
}

fn eager_test(stmt: &Stmt, rebinding: &PythonTestRebinding, source: &SourceText<'_>) -> bool {
    let Stmt::FunctionDef(function) = stmt else {
        return false;
    };
    let args = &function.args;
    if function.returns.is_some()
        || !function.type_params.is_empty()
        || !function.decorator_list.is_empty()
        || args
            .posonlyargs
            .iter()
            .chain(&args.args)
            .chain(&args.kwonlyargs)
            .any(|arg| arg.default.is_some() || arg.def.annotation.is_some())
        || args
            .vararg
            .as_ref()
            .is_some_and(|arg| arg.annotation.is_some())
        || args
            .kwarg
            .as_ref()
            .is_some_and(|arg| arg.annotation.is_some())
    {
        return false;
    }
    let mut assigned = BTreeSet::new();
    function.body.iter().all(|stmt| match stmt {
        Stmt::Expr(expr) => eager_expression(&expr.value, rebinding, source),
        Stmt::Assign(assign) => match assign.targets.as_slice() {
            [Expr::Name(name)] => {
                assigned.insert(name.id.as_str())
                    && eager_expression(&assign.value, rebinding, source)
            }
            _ => false,
        },
        // Messages and compound predicates execute conditionally and must not
        // supply an owner call to the changed-effect observer.
        Stmt::Assert(assertion) => {
            assertion.msg.is_none() && eager_expression(&assertion.test, rebinding, source)
        }
        Stmt::Pass(_) => true,
        _ => false,
    })
}

fn eager_expression(expr: &Expr, rebinding: &PythonTestRebinding, source: &SourceText<'_>) -> bool {
    match expr {
        Expr::Name(name) => !rebinding.module_rebinds(name.id.as_str()),
        Expr::Constant(_) => !text_for_range(source, expr.range()).contains('\n'),
        Expr::Attribute(attr) => {
            !rebinding.assigns_attribute(attr.attr.as_str())
                && !attr.attr.as_str().starts_with("__")
                && eager_expression(&attr.value, rebinding, source)
        }
        Expr::Call(call) => {
            let callee_retained = match call.func.as_ref() {
                Expr::Name(name) => !rebinding.module_rebinds(name.id.as_str()),
                Expr::Attribute(_) => true,
                _ => false,
            };
            callee_retained
                && eager_expression(&call.func, rebinding, source)
                && call
                    .args
                    .iter()
                    .all(|arg| eager_expression(arg, rebinding, source))
                && call.keywords.iter().all(|keyword| {
                    keyword.arg.is_some() && eager_expression(&keyword.value, rebinding, source)
                })
        }
        Expr::Compare(compare) => {
            compare.ops.len() == 1
                && eager_expression(&compare.left, rebinding, source)
                && compare
                    .comparators
                    .iter()
                    .all(|expr| eager_expression(expr, rebinding, source))
        }
        _ => false,
    }
}
