//! A direct same-file call can instantiate one comparison assertion (#9594).
//! Imported helpers, decorated functions, computed operands, rebinding and
//! control flow stay opaque. Oracle grading and sink alignment keep their owners.

use super::super::admission::PythonAssertionAdmission;
use super::super::module_constants::PythonTestRebinding;
use super::super::source_facts::parse_module_result;
use super::super::source_utils::{SourceText, line_for_range_start, text_for_range};
use super::super::{PythonAssertion, PythonTest};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const MAX_HELPER_ARGUMENTS: usize = 4;

struct ComparisonHelper<'a> {
    parameters: Vec<&'a str>,
    comparison: &'a ast::ExprCompare,
}

impl<'a> ComparisonHelper<'a> {
    fn from_function(function: &'a ast::StmtFunctionDef) -> Option<Self> {
        let args = &function.args;
        let parameters: Vec<_> = args.posonlyargs.iter().chain(&args.args).collect();
        if !function.decorator_list.is_empty()
            || !function_header_has_no_expressions(function)
            || parameters.is_empty()
            || parameters.len() > MAX_HELPER_ARGUMENTS
            || parameters
                .iter()
                .any(|parameter| parameter.default.is_some())
            || !args.kwonlyargs.is_empty()
            || args.vararg.is_some()
            || args.kwarg.is_some()
        {
            return None;
        }
        let [Stmt::Assert(assertion)] = function.body.as_slice() else {
            return None;
        };
        let Expr::Compare(comparison) = assertion.test.as_ref() else {
            return None;
        };
        (comparison.ops.len() == 1 && comparison.comparators.len() == 1).then(|| Self {
            parameters: parameters
                .iter()
                .map(|parameter| parameter.def.arg.as_str())
                .collect(),
            comparison,
        })
    }

    fn instantiate(
        &self,
        call: &ast::ExprCall,
        source: &SourceText<'_>,
        rebinding: &PythonTestRebinding,
    ) -> Option<PythonAssertion> {
        if call.args.len() != self.parameters.len()
            || !call.keywords.is_empty()
            || !call
                .args
                .iter()
                .all(|argument| matches!(argument, Expr::Name(_) | Expr::Constant(_)))
        {
            return None;
        }
        // Exactly one observed field, compared with a literal. Two fields or
        // a caller-derived expected value can be the same changed value twice.
        let left_field = matches!(self.comparison.left.as_ref(), Expr::Attribute(_));
        let right_field = matches!(self.comparison.comparators.first()?, Expr::Attribute(_));
        if left_field == right_field {
            return None;
        }
        let field = if left_field {
            self.comparison.left.as_ref()
        } else {
            self.comparison.comparators.first()?
        };
        let Expr::Attribute(field) = field else {
            return None;
        };
        if rebinding.assigns_attribute(field.attr.as_str()) {
            return None;
        }
        let operand = |expression: &Expr| self.operand(expression, &call.args, source);
        let left = operand(&self.comparison.left)?;
        let right = operand(self.comparison.comparators.first()?)?;
        let operator = match self.comparison.ops.first()? {
            ast::CmpOp::Eq => "==",
            ast::CmpOp::NotEq => "!=",
            ast::CmpOp::Lt => "<",
            ast::CmpOp::LtE => "<=",
            ast::CmpOp::Gt => ">",
            ast::CmpOp::GtE => ">=",
            ast::CmpOp::Is => "is",
            ast::CmpOp::IsNot => "is not",
            ast::CmpOp::In => "in",
            ast::CmpOp::NotIn => "not in",
        };
        let instantiated = format!("assert {left} {operator} {right}\n");
        let Ok(Mod::Module(module)) =
            parse_module_result(Path::new("<assertion-helper>"), &instantiated)
        else {
            return None;
        };
        let mut assertion = super::collect_assertions_from_statements(
            &module.body,
            &SourceText::new(&instantiated),
            false,
        )
        .into_iter()
        .next()?;
        // Effect observation must occur at the call, after the owner's write,
        // rather than at the helper declaration or synthetic source line 1.
        assertion.line = line_for_range_start(source, call.range);
        assertion.effect_only = true;
        Some(assertion)
    }

    fn operand(
        &self,
        expression: &Expr,
        arguments: &[Expr],
        source: &SourceText<'_>,
    ) -> Option<String> {
        let argument = |name: &str| {
            self.parameters
                .iter()
                .position(|parameter| *parameter == name)
                .and_then(|index| arguments.get(index))
        };
        match expression {
            Expr::Name(name) => {
                let actual = argument(name.id.as_str())?;
                if !matches!(actual, Expr::Constant(_)) {
                    return None;
                }
                Some(text_for_range(source, actual.range()).trim().to_string())
            }
            Expr::Attribute(attribute) => {
                let Expr::Name(parameter) = attribute.value.as_ref() else {
                    return None;
                };
                let Expr::Name(actual) = argument(parameter.id.as_str())? else {
                    return None;
                };
                Some(format!("{}.{}", actual.id, attribute.attr))
            }
            Expr::Constant(_) => Some(
                text_for_range(source, expression.range())
                    .trim()
                    .to_string(),
            ),
            _ => None,
        }
    }
}

/// The caller has already populated module/test rebinding facts. Keep the
/// retained test body and relation identity; only add instantiated assertions.
pub(in crate::analysis::language::python) fn attach_same_file_assertions(
    source: &SourceText<'_>,
    statements: &[Stmt],
    tests: &mut [PythonTest],
) {
    if tests.is_empty() {
        return;
    }
    let functions: BTreeMap<_, _> = statements
        .iter()
        .filter_map(|statement| {
            let Stmt::FunctionDef(function) = statement else {
                return None;
            };
            Some((line_for_range_start(source, function.range), function))
        })
        .collect();
    let helpers: BTreeMap<_, _> = functions
        .values()
        .filter_map(|function| {
            (!function.name.as_str().starts_with("test"))
                .then(|| ComparisonHelper::from_function(function))
                .flatten()
                .map(|helper| (function.name.as_str(), helper))
        })
        .collect();
    if helpers.is_empty() {
        return;
    }
    for test in tests {
        if test.constant_rebinding.opaque
            // A rebound or locally shadowed module binding can replace the
            // imported class or helper behind the existing receiver facts.
            || test.constant_rebinding.names.iter().any(|name| {
                test.constant_rebinding.module_bound.contains(name)
            })
            || test
                .constant_rebinding
                .attributes
                .iter()
                .any(|name| name.starts_with("__"))
            || ["__code__", "__globals__", "__setattr__", "__getattribute__"]
                .iter()
                .any(|name| super::super::mentions_python_name(source, name))
        {
            continue;
        }
        let Some(function) = functions.get(&test.line) else {
            continue;
        };
        if !function.decorator_list.is_empty()
            || !function_header_has_no_expressions(function)
            || !function.body.iter().all(|statement| {
                straight_line_statement(statement, &test.constant_rebinding, source)
            })
        {
            continue;
        }
        let mut statement_lines = BTreeSet::new();
        let mut assigned_names = BTreeSet::new();
        if function.body.iter().any(|statement| {
            if !statement_lines.insert(line_for_range_start(source, statement.range())) {
                return true;
            }
            if let Stmt::Assign(assignment) = statement
                && let [Expr::Name(name)] = assignment.targets.as_slice()
            {
                return !assigned_names.insert(name.id.as_str());
            }
            false
        }) {
            continue;
        }
        let assertions: Vec<_> = function
            .body
            .iter()
            .filter_map(|statement| {
                let Stmt::Expr(expression) = statement else {
                    return None;
                };
                let Expr::Call(call) = expression.value.as_ref() else {
                    return None;
                };
                let Expr::Name(name) = call.func.as_ref() else {
                    return None;
                };
                if test
                    .constant_rebinding
                    .names
                    .iter()
                    .any(|bound| bound == name.id.as_str())
                {
                    return None;
                }
                helpers
                    .get(name.id.as_str())?
                    .instantiate(call, source, &test.constant_rebinding)
            })
            .collect();
        if !assertions.is_empty() {
            test.assertions.extend(assertions);
            test.assertions.sort_by_key(|assertion| assertion.line);
            test.assertion_admission = PythonAssertionAdmission::Recognized;
        }
    }
}

fn function_header_has_no_expressions(function: &ast::StmtFunctionDef) -> bool {
    let args = &function.args;
    // Header calls run at definition time (or remain deferred), not on the
    // local receiver constructed by the test body. Do not let body-text scans
    // attribute their effects to that later receiver.
    function.returns.is_none()
        && function.type_params.is_empty()
        && args
            .posonlyargs
            .iter()
            .chain(&args.args)
            .chain(&args.kwonlyargs)
            .all(|arg| arg.default.is_none() && arg.def.annotation.is_none())
        && args
            .vararg
            .as_ref()
            .is_none_or(|arg| arg.annotation.is_none())
        && args
            .kwarg
            .as_ref()
            .is_none_or(|arg| arg.annotation.is_none())
}

fn straight_line_statement(
    statement: &Stmt,
    rebinding: &PythonTestRebinding,
    source: &SourceText<'_>,
) -> bool {
    match statement {
        Stmt::Expr(expression) => eager_expression(&expression.value, rebinding, source),
        Stmt::Assign(assignment) => {
            matches!(assignment.targets.as_slice(), [Expr::Name(_)])
                && eager_expression(&assignment.value, rebinding, source)
        }
        Stmt::Pass(_) => true,
        _ => false,
    }
}

// Statement shape alone does not establish execution: BoolOp, IfExp, lambda
// and comprehensions can hide an owner call from the effect scan's ordering.
// Admit only eager construction/call expressions in this bounded helper path.
fn eager_expression(
    expression: &Expr,
    rebinding: &PythonTestRebinding,
    source: &SourceText<'_>,
) -> bool {
    match expression {
        Expr::Name(_) => true,
        // Existing line-based call scans cannot distinguish text on an interior
        // literal line from code. Keep those fake calls out of helper credit.
        Expr::Constant(_) => !text_for_range(source, expression.range()).contains('\n'),
        Expr::Attribute(attribute) => {
            !rebinding.assigns_attribute(attribute.attr.as_str())
                && eager_expression(&attribute.value, rebinding, source)
        }
        Expr::Call(call) => {
            eager_expression(&call.func, rebinding, source)
                && call
                    .args
                    .iter()
                    .all(|arg| eager_expression(arg, rebinding, source))
                && call.keywords.iter().all(|keyword| {
                    keyword.arg.is_some() && eager_expression(&keyword.value, rebinding, source)
                })
        }
        _ => false,
    }
}
