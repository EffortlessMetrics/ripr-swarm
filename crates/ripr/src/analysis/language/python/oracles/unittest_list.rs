//! Bounded #7312 support. Broader unittest inheritance, helpers, messages and
//! multi-statement methods stay unknown; this is static shape evidence only.
use super::super::admission::PythonAssertionAdmission;
use super::super::related_tests::{oracle_operand_calls, owner_module_callees};
use super::super::{PythonImport, PythonOwner, PythonTest};
use crate::domain::{OracleKind, OracleStrength};
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

pub(in crate::analysis::language::python) fn eligible_class(
    class: &ast::StmtClassDef,
    imports: &[PythonImport],
) -> bool {
    if class.bases.len() != 1 || !class.keywords.is_empty() || !class.decorator_list.is_empty() {
        return false;
    }
    let authentic = match class.bases.first() {
        Some(Expr::Attribute(attr)) if attr.attr.as_str() == "TestCase" => {
            matches!(attr.value.as_ref(), Expr::Name(name) if name.id.as_str() == "unittest")
                && imports.iter().any(|import| {
                    import.source_module.is_empty()
                        && import.imported == "unittest"
                        && import.alias == "unittest"
                })
        }
        Some(Expr::Name(name)) if name.id.as_str() == "TestCase" => imports.iter().any(|import| {
            import.source_module == "unittest"
                && import.imported == "TestCase"
                && import.alias == "TestCase"
        }),
        _ => false,
    };
    authentic
        && class.body.iter().all(|statement| match statement {
            // Reject setup/teardown, overrides, descriptors and dynamic hooks, as
            // assertListEqual delegates through assertSequenceEqual and fail.
            Stmt::FunctionDef(function) => function.name.as_str().starts_with("test"),
            Stmt::Expr(expr) => matches!(expr.value.as_ref(), Expr::Constant(constant)
            if matches!(constant.value, ast::Constant::Str(_))),
            _ => false,
        })
}

pub(in crate::analysis::language::python) fn eligible_method(
    args: &ast::Arguments,
    decorators: &[Expr],
) -> bool {
    decorators.is_empty()
        && args.posonlyargs.is_empty()
        && args.args.len() == 1
        && args
            .args
            .first()
            .is_some_and(|arg| arg.def.arg.as_str() == "self")
        && args.kwonlyargs.is_empty()
        && args.vararg.is_none()
        && args.kwarg.is_none()
}

pub(in crate::analysis::language::python) fn literal_call(call: &ast::ExprCall) -> bool {
    matches!(call.func.as_ref(), Expr::Attribute(attr)
        if attr.attr.as_str() == "assertListEqual"
            && matches!(attr.value.as_ref(), Expr::Name(name) if name.id.as_str() == "self"))
        && call.keywords.is_empty()
        && call.args.len() == 2
        && matches!(call.args.first(), Some(Expr::Call(owner_call))
            if owner_call.args.is_empty() && owner_call.keywords.is_empty()
                && static_callee(&owner_call.func))
        && matches!(call.args.get(1), Some(Expr::List(list))
            if list.elts.iter().all(|element| matches!(element, Expr::Constant(_))))
}

// A derived call compares another result, even if evaluating its callee calls
// the owner: `labels().fixed_list()` and `labels()()` do not observe its return.
fn static_callee(expr: &Expr) -> bool {
    match expr {
        Expr::Name(_) => true,
        Expr::Attribute(attribute) => static_callee(&attribute.value),
        _ => false,
    }
}

pub(in crate::analysis::language::python) fn literal_text(text: &str) -> bool {
    let Ok(Mod::Module(module)) =
        super::super::source_facts::parse_module_result(Path::new("<unittest-list-oracle>"), text)
    else {
        return false;
    };
    matches!(module.body.as_slice(), [Stmt::Expr(expr)]
        if matches!(expr.value.as_ref(), Expr::Call(call) if literal_call(call)))
}

pub(in crate::analysis::language::python) fn withhold_modified_oracles(tests: &mut [PythonTest]) {
    for test in tests {
        let bindings = &test.constant_rebinding;
        if bindings.opaque
            || bindings.rebinds("unittest")
            || bindings.rebinds("TestCase")
            || [
                "TestCase",
                "assertListEqual",
                "assertSequenceEqual",
                "fail",
                "__getattribute__",
                "__getattr__",
            ]
            .iter()
            .any(|name| bindings.assigns_attribute(name))
        {
            for assertion in &mut test.assertions {
                if literal_text(&assertion.text) {
                    assertion.oracle_kind = OracleKind::Unknown;
                    assertion.oracle_strength = OracleStrength::Unknown;
                    assertion.oracle_shape = super::super::PythonOracleShape::UnknownCustomHelper;
                }
            }
            if test.assertions.iter().all(|assertion| {
                assertion.oracle_shape == super::super::PythonOracleShape::UnknownCustomHelper
            }) && !test.assertions.is_empty()
            {
                test.assertion_admission = PythonAssertionAdmission::Unresolved;
            }
        }
    }
}

/// A newly supported list assertion must compare this owner's result in this
/// row. Literal coincidence, a neighboring row and a nested wrapper cannot
/// grant the changed-sink-token fallback. Reuse existing call/import authority.
pub(in crate::analysis::language::python) fn observes_owner(
    text: &str,
    test: Option<&PythonTest>,
    owner: &PythonOwner,
) -> bool {
    if !literal_text(text) {
        return true;
    }
    if matches!(
        owner.owner_kind,
        Some(crate::domain::OwnerKind::Method | crate::domain::OwnerKind::ClassMethod)
    ) {
        return false;
    }
    test.is_some_and(|test| {
        let bindings = &test.constant_rebinding;
        !bindings.assigns_attribute(&owner.name)
            && owner_module_callees(test, owner).iter().any(|callee| {
                !bindings.rebinds(callee.split('.').next().unwrap_or(callee))
                    && oracle_operand_calls(text, callee)
            })
    })
}
