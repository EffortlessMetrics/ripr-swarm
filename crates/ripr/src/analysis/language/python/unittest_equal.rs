//! Bounded binding authority for unittest's `assertEqual` call.
use super::admission::PythonAssertionAdmission;
use super::module_constants::body_bound_names;
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_start};
use super::{PythonAssertion, PythonImport, PythonOracleShape, PythonTest, expr_full_name};
use crate::domain::{OracleKind, OracleStrength};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::path::Path;

pub(super) fn literal_path(expr: &Expr) -> bool {
    match expr {
        Expr::Name(_) => true,
        Expr::Attribute(attribute) => literal_path(&attribute.value),
        _ => false,
    }
}

/// An imported stdlib TestCase, including its supported module/type aliases.
pub(super) fn standard_base(base: &Expr, imports: &[PythonImport]) -> bool {
    if !literal_path(base) {
        return false;
    }
    let Some(name) = expr_full_name(base) else {
        return false;
    };
    imports.iter().any(|import| {
        (matches!(import.source_module.as_str(), "unittest" | "unittest.case")
            && import.imported == "TestCase"
            && name == import.alias)
            || (import.source_module == "unittest"
                && import.imported == "case"
                && name == format!("{}.TestCase", import.alias))
            || (import.source_module.is_empty()
                && matches!(import.imported.as_str(), "unittest" | "unittest.case")
                && (name == format!("{}.TestCase", import.alias)
                    || (import.imported == "unittest"
                        && name == format!("{}.case.TestCase", import.alias))))
    })
}

fn replaces_equality(name: &str) -> bool {
    matches!(
        name,
        "assertEqual"
            | "assertDictEqual"
            | "assertListEqual"
            | "assertTupleEqual"
            | "assertSetEqual"
            | "assertMultiLineEqual"
            | "assertSequenceEqual"
            | "fail"
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

fn equal_callees(text: &str) -> Vec<Option<String>> {
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
    fn collect(expression: &Expr, out: &mut Vec<Option<String>>) {
        match expression {
            Expr::Call(call) => {
                if let Some(name) = expr_full_name(&call.func)
                    && name.rsplit('.').next() == Some("assertEqual")
                {
                    out.push(literal_path(&call.func).then_some(name));
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
                && literal_path(&assign.value)
                && expr_full_name(&assign.value).as_deref()
                    == Some(format!("{receiver}.assertEqual").as_str()))
            .then(|| line_for_range_start(source, assign.range()))
        })
    });
    for assertion in assertions {
        if equal_callees(&assertion.text).iter().any(|callee| {
            !(standard
                && callee.as_deref().is_some_and(|callee| {
                    receiver.is_some_and(|name| callee == format!("{name}.assertEqual"))
                        || (callee == "assertEqual"
                            && captured.is_some_and(|line| line < assertion.line))
                }))
        }) {
            withhold(assertion);
        }
    }
}

struct ModuleCapture {
    receiver: Option<String>,
}

fn standard_instance(expr: &Expr, imports: &[PythonImport]) -> bool {
    matches!(expr, Expr::Call(call) if call.args.is_empty() && call.keywords.is_empty()
        && standard_base(&call.func, imports))
}

/// Only unique straight-line module bindings are considered. Shared rebinding
/// facts below reject another assignment, import, or global writer of either name.
fn module_capture(statements: &[Stmt], imports: &[PythonImport]) -> Option<ModuleCapture> {
    let mut captured: Option<Option<String>> = None;
    for (index, statement) in statements.iter().enumerate() {
        let Stmt::Assign(assign) = statement else {
            continue;
        };
        let [Expr::Name(target)] = assign.targets.as_slice() else {
            continue;
        };
        if target.id.as_str() != "assertEqual" {
            continue;
        }
        captured = None;
        let Expr::Attribute(attribute) = assign.value.as_ref() else {
            continue;
        };
        if attribute.attr.as_str() != "assertEqual" {
            continue;
        }
        if standard_instance(&attribute.value, imports) {
            captured = Some(None);
        } else if let Expr::Name(receiver) = attribute.value.as_ref()
            && statements[..index].iter().any(|previous| {
                matches!(previous, Stmt::Assign(previous)
                    if matches!(previous.targets.as_slice(), [Expr::Name(name)] if name.id == receiver.id)
                        && standard_instance(&previous.value, imports))
            })
        {
            captured = Some(Some(receiver.id.to_string()));
        }
    }
    captured.map(|receiver| ModuleCapture { receiver })
}

fn captured_statement_oracle(text: &str) -> Option<PythonAssertion> {
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<captured-unittest-equal>"), text)
    else {
        return None;
    };
    let [Stmt::Expr(statement)] = module.body.as_slice() else {
        return None;
    };
    let Expr::Call(call) = statement.value.as_ref() else {
        return None;
    };
    if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "assertEqual") {
        return None;
    }
    // Reuse the oracle owner, including its operand/tautology rules.
    super::oracles::collect_assertions_from_statements(&module.body, &SourceText::new(text), false)
        .into_iter()
        .next()
}

/// Apply module writes and standard bound-method captures using producer-owned
/// rebinding facts. This changes oracle authority, never owner-call reach.
pub(super) fn apply_module_bindings(
    tests: &mut [PythonTest],
    source: &str,
    module_imports: &[PythonImport],
    statements: &[Stmt],
) {
    let capture = module_capture(statements, module_imports);
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
                    && bindings.module_rebinds(if import.source_module.is_empty() {
                        // A plain dotted import binds its first component.
                        import.alias.split('.').next().unwrap_or(&import.alias)
                    } else {
                        &import.alias
                    })
            })
            || source.contains("addTypeEqualityFunc")
            || source.contains("._type_equality_funcs");
        let captured_standard = !modified
            && !bindings.rebinds("assertEqual")
            && capture.as_ref().is_some_and(|capture| {
                capture
                    .receiver
                    .as_ref()
                    .is_none_or(|name| !bindings.module_rebinds(name))
            });
        let mut restored = false;
        for assertion in &mut test.assertions {
            if modified && !equal_callees(&assertion.text).is_empty() {
                withhold(assertion);
            } else if captured_standard
                && let Some(raw) = captured_statement_oracle(&assertion.text)
            {
                assertion.oracle_kind = raw.oracle_kind;
                assertion.oracle_strength = raw.oracle_strength;
                assertion.oracle_shape = raw.oracle_shape;
                restored = true;
            }
        }
        if restored {
            test.assertion_admission = PythonAssertionAdmission::Recognized;
        } else if modified
            && !test.assertions.is_empty()
            && test
                .assertions
                .iter()
                .all(|assertion| assertion.oracle_shape == PythonOracleShape::UnknownCustomHelper)
        {
            test.assertion_admission = PythonAssertionAdmission::Unresolved;
        }
    }
}
