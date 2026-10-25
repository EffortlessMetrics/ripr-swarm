//! Bounded binding authority for unittest's `assertEqual` call.
use super::admission::PythonAssertionAdmission;
use super::module_constants::body_bound_names;
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_start};
use super::{PythonAssertion, PythonImport, PythonOracleShape, PythonTest, expr_full_name};
use crate::domain::{OracleKind, OracleStrength};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::path::Path;

/// An imported stdlib TestCase, including its supported module/type aliases.
pub(super) fn standard_base(base: &Expr, imports: &[PythonImport]) -> bool {
    let Some(name) = expr_full_name(base) else {
        return false;
    };
    imports.iter().any(|import| {
        (matches!(import.source_module.as_str(), "unittest" | "unittest.case")
            && import.imported == "TestCase"
            && name == import.alias)
            || (import.source_module.is_empty()
                && matches!(import.imported.as_str(), "unittest" | "unittest.case")
                && (name == format!("{}.TestCase", import.alias)
                    || (import.imported == "unittest"
                        && name == format!("{}.case.TestCase", import.alias))))
    })
}

fn replaces_equality(name: &str) -> bool {
    name.starts_with("assert")
        || matches!(
            name,
            "fail"
                | "_getAssertEqualityFunc"
                | "_baseAssertEqual"
                | "_type_equality_funcs"
                | "_formatMessage"
                | "_truncateMessage"
                | "addTypeEqualityFunc"
                | "__getattribute__"
                | "__getattr__"
                | "__setattr__"
                | "__init__"
        )
}

fn target_replaces_equality(target: &Expr) -> bool {
    match target {
        Expr::Name(name) => replaces_equality(name.id.as_str()),
        Expr::Tuple(tuple) => tuple.elts.iter().any(target_replaces_equality),
        Expr::List(list) => list.elts.iter().any(target_replaces_equality),
        Expr::Starred(starred) => target_replaces_equality(&starred.value),
        _ => false,
    }
}

/// Normal setup and unrelated class members are allowed; overriding the
/// standard method or its dispatch/failure machinery is not standard authority.
pub(super) fn class_preserves_equality(class: &ast::StmtClassDef) -> bool {
    class.decorator_list.is_empty()
        && class.keywords.is_empty()
        && class.body.iter().all(|statement| match statement {
            Stmt::FunctionDef(function) => !replaces_equality(function.name.as_str()),
            Stmt::AsyncFunctionDef(function) => !replaces_equality(function.name.as_str()),
            Stmt::Assign(assign) => !assign.targets.iter().any(target_replaces_equality),
            Stmt::AnnAssign(assign) => !target_replaces_equality(&assign.target),
            Stmt::Expr(expr) => matches!(expr.value.as_ref(), Expr::Constant(_)),
            Stmt::Pass(_) => true,
            _ => false,
        })
}

fn equal_callees(text: &str) -> Vec<String> {
    if !text.contains("assertEqual") {
        return Vec::new();
    }
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<unittest-equal-oracle>"), text)
    else {
        return Vec::new();
    };
    let expression = match module.body.as_slice() {
        [Stmt::Expr(statement)] => statement.value.as_ref(),
        [Stmt::Assert(statement)] => statement.test.as_ref(),
        _ => return Vec::new(),
    };
    fn collect(expression: &Expr, out: &mut Vec<String>) {
        match expression {
            Expr::Call(call) => {
                if let Some(name) = expr_full_name(&call.func)
                    && name.rsplit('.').next() == Some("assertEqual")
                {
                    out.push(name);
                }
            }
            Expr::BoolOp(boolean) if boolean.op == ast::BoolOp::And => {
                for value in &boolean.values {
                    collect(value, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    collect(expression, &mut out);
    out
}

fn withhold(assertion: &mut PythonAssertion) {
    assertion.oracle_kind = OracleKind::Unknown;
    assertion.oracle_strength = OracleStrength::Unknown;
    assertion.oracle_shape = PythonOracleShape::UnknownCustomHelper;
}

/// Do not infer unittest authority from a final method name. A bare helper
/// stays unresolved until the existing helper-body resolver authenticates it.
pub(super) fn withhold_unbound_equal(
    assertions: &mut [PythonAssertion],
    args: &ast::Arguments,
    decorators: &[Expr],
    body: &[Stmt],
    standard_class: bool,
    source: &SourceText<'_>,
) {
    let receiver = args
        .posonlyargs
        .first()
        .or_else(|| args.args.first())
        .map(|arg| arg.def.arg.as_str());
    let locals = body_bound_names(body);
    let standard = standard_class
        && decorators.is_empty()
        && receiver.is_some_and(|name| {
            locals
                .as_ref()
                .is_some_and(|names| !names.iter().any(|bound| bound == name))
        });
    let captured = receiver.and_then(|receiver| {
        if !locals.as_ref().is_some_and(|names| {
            names
                .iter()
                .filter(|name| name.as_str() == "assertEqual")
                .count()
                == 1
        }) {
            return None;
        }
        body.iter().find_map(|statement| {
            let Stmt::Assign(assign) = statement else {
                return None;
            };
            let [Expr::Name(target)] = assign.targets.as_slice() else {
                return None;
            };
            (target.id.as_str() == "assertEqual"
                && expr_full_name(&assign.value).as_deref()
                    == Some(format!("{receiver}.assertEqual").as_str()))
            .then(|| line_for_range_start(source, assign.range()))
        })
    });
    for assertion in assertions {
        if equal_callees(&assertion.text).iter().any(|callee| {
            !(standard
                && (receiver.is_some_and(|name| *callee == format!("{name}.assertEqual"))
                    || (*callee == "assertEqual"
                        && captured.is_some_and(|line| line < assertion.line))))
        }) {
            withhold(assertion);
        }
    }
}

/// Module-wide writes can replace inherited methods or their imported base.
/// Reuse producer-owned rebinding facts; keep this separate from owner reach.
pub(super) fn withhold_modified_equal(
    tests: &mut [PythonTest],
    source: &str,
    module_imports: &[PythonImport],
) {
    for test in tests {
        let bindings = &test.constant_rebinding;
        let modified = bindings.opaque
            || bindings
                .attributes
                .iter()
                .any(|name| name == "*" || replaces_equality(name) || name == "TestCase")
            || module_imports.iter().chain(&test.imports).any(|import| {
                (matches!(import.source_module.as_str(), "unittest" | "unittest.case")
                    || (import.source_module.is_empty()
                        && matches!(import.imported.as_str(), "unittest" | "unittest.case")))
                    && bindings.module_rebinds(&import.alias)
            })
            || source.contains("addTypeEqualityFunc");
        if modified {
            for assertion in &mut test.assertions {
                if !equal_callees(&assertion.text).is_empty() {
                    withhold(assertion);
                }
            }
            if !test.assertions.is_empty()
                && test.assertions.iter().all(|assertion| {
                    assertion.oracle_shape == PythonOracleShape::UnknownCustomHelper
                })
            {
                test.assertion_admission = PythonAssertionAdmission::Unresolved;
            }
        }
    }
}
