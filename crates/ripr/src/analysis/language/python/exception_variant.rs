//! A class-only `pytest.raises` can distinguish a changed raised type (#12952).
//!
//! This is a finding-specific reading, not a stronger grade for arbitrary
//! exception names. Two inert sibling classes must directly extend the
//! unshadowed builtin `Exception`, and the test must import the new class and
//! owner without aliases from the owner's module. Its own `raises` context
//! must call the owner with literal inputs that reach the exact edited raise.
//! Unknown bindings, class behavior, test activation or function flow grant
//! no credit. The shared repair validator is unchanged.

use super::module_constants::{
    ModuleBindingFacts, function_bound_names, mentions_identifier, visit_exprs_in_statements,
};
use super::module_value::{
    ModuleEdit, Scope, Value, comparison, evaluate, mentions_reflection, names_may_differ,
    with_line_replaced,
};
use super::related_tests::{
    PythonRelatedCandidate, import_source_module_matches_owner, owner_module_paths,
};
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_start};
use super::{PythonAssertion, PythonFramework, PythonOracleShape, PythonOwner, PythonTest};
use crate::domain::{OracleKind, OwnerKind};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::BTreeSet;
use std::path::Path;

/// A local module or package can implement a different `pytest.raises`.
/// Reuse the workspace's bounded import roots and compiled-module index;
/// source entries are checked physically even when excluded from source facts.
pub(super) fn pytest_import_unshadowed(
    root: &Path,
    import_roots: &BTreeSet<&Path>,
    extension_names: Option<&BTreeSet<String>>,
) -> bool {
    let module = Path::new("pytest.py");
    import_roots.contains(Path::new(""))
        && extension_names
            .is_some_and(|names| !super::module_value::shadowed_by_extension(names, module))
        && !super::module_value::compiled_package_shadow(root, import_roots, module)
        && import_roots.iter().all(|directory| {
            matches!(
                std::fs::symlink_metadata(root.join(directory).join(module)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            )
        })
}

pub(super) struct ExceptionVariantCredit<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) assertion: &'a PythonAssertion,
    pub(super) old_type: String,
    pub(super) new_type: String,
}

pub(super) fn exception_variant_credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    module_owner: &PythonOwner,
    candidates: &[PythonRelatedCandidate<'a>],
) -> Vec<ExceptionVariantCredit<'a>> {
    credits(edit, owner, module_owner, candidates).unwrap_or_default()
}

fn credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    module_owner: &PythonOwner,
    candidates: &[PythonRelatedCandidate<'a>],
) -> Option<Vec<ExceptionVariantCredit<'a>>> {
    if owner.owner_kind != Some(OwnerKind::Function)
        || owner.name != owner.qualified_name
        || !owner.decorators.is_empty()
    {
        return None;
    }
    let old_line = edit.old_line_text?;
    let (old_type, old_args) = raised_type(old_line)?;
    let (new_type, new_args) = raised_type(edit.line_text)?;
    let old_indent = &old_line[..old_line.len() - old_line.trim_start().len()];
    let new_indent = &edit.line_text[..edit.line_text.len() - edit.line_text.trim_start().len()];
    if old_type == new_type
        || old_args != new_args
        || old_indent != new_indent
        || edit.new_source.lines().nth(edit.line.checked_sub(1)?)? != edit.line_text
    {
        return None;
    }
    let old_source = with_line_replaced(edit.new_source, edit.line, old_line)?;
    let Mod::Module(new_module) = parse_module_result(edit.file, edit.new_source).ok()? else {
        return None;
    };
    let Mod::Module(old_module) = parse_module_result(edit.file, &old_source).ok()? else {
        return None;
    };
    let new_facts = ModuleBindingFacts::of(edit.new_source, &new_module.body);
    let old_facts = ModuleBindingFacts::of(&old_source, &old_module.body);
    let names = [old_type.as_str(), new_type.as_str(), owner.name.as_str()];
    // The caller requires the current per-file module-import admission.
    // That producer checks raw module identity; the shared owner below
    // retains the legacy configured-path flag for other module-value passes.
    if !new_facts.unbound("Exception")
        || !old_facts.unbound("Exception")
        || mentions_identifier(edit.new_source, "__builtins__")
        || mentions_reflection(edit.new_source)
        || names_may_differ(edit.new_source)
        || new_facts.assigns_dunder()
        || old_facts.assigns_dunder()
        || module_owner.foreign_writes.reflection_elsewhere()
        || module_owner.foreign_writes.names_may_differ_elsewhere()
        || module_owner.foreign_writes.writes_dunder()
        || module_owner.foreign_writes.may_replace_reader_code()
        || ["Exception", "pytest", "raises"]
            .iter()
            .any(|name| module_owner.foreign_writes.may_rebind(name, edit.file))
        || names.iter().any(|name| {
            !new_facts.bound_once(name)
                || !old_facts.bound_once(name)
                || module_owner.foreign_writes.may_rebind(name, edit.file)
        })
        || ![&new_module, &old_module].iter().all(|module| {
            plain_exception(&module.body, &old_type) && plain_exception(&module.body, &new_type)
        })
    {
        return None;
    }
    let source = SourceText::new(edit.new_source);
    let function = new_module.body.iter().find_map(|stmt| match stmt {
        Stmt::FunctionDef(function)
            if function.name.as_str() == owner.name
                && line_for_range_start(&source, function.range) == owner.start_line =>
        {
            Some(function)
        }
        _ => None,
    })?;
    let local_names = function_bound_names(&function.args, &function.body, edit.new_source)?;
    if names
        .iter()
        .any(|name| local_names.iter().any(|local| local.as_str() == *name))
        || deferred_body(&function.body)
        || !plain_parameters(function)
    {
        return None;
    }
    let len_intact = new_facts.unbound("len")
        && old_facts.unbound("len")
        && !local_names.iter().any(|name| name == "len")
        && !module_owner.foreign_writes.may_rebind("len", edit.file);
    let mut credits = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        let test = candidate.test;
        if !test_bindings_intact(test, owner, &old_type, &new_type) {
            continue;
        }
        let Ok(Mod::Module(module)) = parse_module_result(&test.file, &test.body_text) else {
            continue;
        };
        let [Stmt::FunctionDef(test_function)] = module.body.as_slice() else {
            continue;
        };
        if deferred_body(&test_function.body) {
            continue;
        }
        for statement in &test_function.body {
            let Stmt::With(context) = statement else {
                // Only a demonstrated passing assertion or inert prefix can precede
                // the context. An earlier failing assertion prevents its call.
                if inert_prefix(statement) {
                    continue;
                }
                break;
            };
            let [item] = context.items.as_slice() else {
                break;
            };
            let Expr::Call(raises) = &item.context_expr else {
                break;
            };
            if !pytest_raises(raises) {
                break;
            }
            let [Expr::Name(expected)] = raises.args.as_slice() else {
                break;
            };
            if expected.id.as_str() != new_type || !raises.keywords.is_empty() {
                break;
            }
            let [Stmt::Expr(body)] = context.body.as_slice() else {
                break;
            };
            let Expr::Call(call) = body.value.as_ref() else {
                break;
            };
            if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == owner.name) {
                break;
            }
            let Some(parameters) = literal_arguments(function, call) else {
                break;
            };
            let mut budget = 128;
            let path = RaisePath {
                source: &source,
                line: edit.line,
                parameters: &parameters,
                len_intact,
            };
            if path.walk(&function.body, 0, &mut budget) != Some(Flow::Target) {
                break;
            }
            let Some(assertion) = test.assertions.iter().find(|assertion| {
                assertion.source_range == Some(item.context_expr.range())
                    && assertion.oracle_shape == PythonOracleShape::ExceptionAssertion
                    && assertion.oracle_kind == OracleKind::BroadError
            }) else {
                break;
            };
            credits.push(ExceptionVariantCredit {
                test,
                assertion,
                old_type: old_type.clone(),
                new_type: new_type.clone(),
            });
            break;
        }
    }
    Some(credits)
}

/// Only the constructor's name changes; evaluating its arguments runs no code.
fn raised_type(line: &str) -> Option<(String, Vec<ast::Constant>)> {
    let Mod::Module(module) = parse_module_result(Path::new("raise.py"), line.trim()).ok()? else {
        return None;
    };
    let [Stmt::Raise(raise)] = module.body.as_slice() else {
        return None;
    };
    if raise.cause.is_some() {
        return None;
    }
    let Expr::Call(call) = raise.exc.as_deref()? else {
        return None;
    };
    let Expr::Name(name) = call.func.as_ref() else {
        return None;
    };
    if !call.keywords.is_empty() {
        return None;
    }
    let arguments = call
        .args
        .iter()
        .map(|argument| match argument {
            Expr::Constant(constant)
                if matches!(
                    constant.value,
                    ast::Constant::Str(_)
                        | ast::Constant::Int(_)
                        | ast::Constant::Bool(_)
                        | ast::Constant::None
                ) =>
            {
                Some(constant.value.clone())
            }
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((name.id.to_string(), arguments))
}

fn plain_exception(body: &[Stmt], name: &str) -> bool {
    body.iter().any(|statement| {
        let Stmt::ClassDef(class) = statement else {
            return false;
        };
        class.name.as_str() == name
            && matches!(
                class.bases.as_slice(),
                [Expr::Name(base)] if base.id.as_str() == "Exception"
            )
            && class.keywords.is_empty()
            && class.decorator_list.is_empty()
            && class.type_params.is_empty()
            && class
                .body
                .iter()
                .all(|statement| matches!(statement, Stmt::Pass(_)) || docstring(statement))
    })
}

fn docstring(statement: &Stmt) -> bool {
    let Stmt::Expr(statement) = statement else {
        return false;
    };
    let Expr::Constant(constant) = statement.value.as_ref() else {
        return false;
    };
    matches!(constant.value, ast::Constant::Str(_))
}

fn inert_prefix(statement: &Stmt) -> bool {
    match statement {
        Stmt::Pass(_) => true,
        Stmt::Assert(assertion) => {
            evaluate(&assertion.test, &scope(&[])).is_some_and(|value| value.truthy())
        }
        _ => docstring(statement),
    }
}

fn plain_parameters(function: &ast::StmtFunctionDef) -> bool {
    function.args.vararg.is_none()
        && function.args.kwarg.is_none()
        && function.args.kwonlyargs.is_empty()
        && function.type_params.is_empty()
        && function.returns.is_none()
        && function
            .args
            .posonlyargs
            .iter()
            .chain(&function.args.args)
            .all(|argument| argument.default.is_none() && argument.def.annotation.is_none())
}

fn deferred_body(body: &[Stmt]) -> bool {
    let mut deferred = false;
    visit_exprs_in_statements(body, &mut |expression| {
        deferred |= matches!(
            expression,
            Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_)
        );
    });
    deferred
}

fn test_bindings_intact(
    test: &PythonTest,
    owner: &PythonOwner,
    old_type: &str,
    new_type: &str,
) -> bool {
    let rebinding = &test.constant_rebinding;
    let owner_modules = owner_module_paths(&owner.file);
    test.framework == PythonFramework::Pytest
        && test.name == test.qualified_name
        && test.decorators.is_empty()
        && test.fixtures.is_empty()
        && !test.parametrized
        && !rebinding.implicit_reach
        && !rebinding.header_reach
        && !rebinding.fixture_substitution
        && super::test_activation::activation_control(test).is_none()
        && !mentions_identifier(&test.body_text, "__builtins__")
        && !mentions_reflection(&test.body_text)
        && !names_may_differ(&test.body_text)
        && [
            owner.name.as_str(),
            old_type,
            new_type,
            "pytest",
            "raises",
            "Exception",
            "len",
            "__builtins__",
        ]
        .iter()
        .all(|name| !rebinding.rebinds(name) && !rebinding.assigns_attribute(name))
        && rebinding
            .attributes
            .iter()
            .chain(rebinding.foreign_attributes.iter())
            .all(|attribute| !(attribute.starts_with("__") && attribute.ends_with("__")))
        && test.imports.iter().any(|import| {
            import.imported == "pytest"
                && import.alias == "pytest"
                && import.source_module.is_empty()
        })
        && test.imports.iter().any(|import| {
            import.imported == owner.name
                && import.alias == owner.name
                && owner_modules.contains(&import.source_module)
                && import_source_module_matches_owner(import, owner, &test.file)
                && test.imports.iter().any(|class_import| {
                    class_import.imported == new_type
                        && class_import.alias == new_type
                        && class_import.source_module == import.source_module
                })
        })
}

fn pytest_raises(call: &ast::ExprCall) -> bool {
    matches!(call.func.as_ref(), Expr::Attribute(attribute)
        if attribute.attr.as_str() == "raises"
            && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == "pytest"))
}

fn literal_arguments(
    function: &ast::StmtFunctionDef,
    call: &ast::ExprCall,
) -> Option<Vec<(String, Value)>> {
    let arguments = function.args.posonlyargs.iter().chain(&function.args.args);
    if !call.keywords.is_empty() || arguments.clone().count() != call.args.len() {
        return None;
    }
    arguments
        .zip(&call.args)
        .map(|(parameter, argument)| {
            let literal = matches!(argument, Expr::Constant(_))
                || matches!(argument, Expr::UnaryOp(unary)
                    if matches!(unary.op, ast::UnaryOp::USub | ast::UnaryOp::UAdd)
                        && matches!(unary.operand.as_ref(), Expr::Constant(_)));
            if !literal {
                return None;
            }
            Some((
                parameter.def.arg.to_string(),
                evaluate(argument, &scope(&[]))?,
            ))
        })
        .collect()
}

static UNBOUND_VALUE: Value = Value::Bool(false);

fn scope(parameters: &[(String, Value)]) -> Scope<'_> {
    Scope {
        constant: "",
        value: &UNBOUND_VALUE,
        parameters: Some(parameters),
        constant_spellings: &[],
        reader_spellings: &[],
        reader: None,
        model: None,
        receiver: None,
        floats: false,
        callee: None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Next,
    Target,
    Exit,
}

struct RaisePath<'a> {
    source: &'a SourceText<'a>,
    line: usize,
    parameters: &'a [(String, Value)],
    len_intact: bool,
}

impl RaisePath<'_> {
    fn walk(&self, body: &[Stmt], depth: usize, budget: &mut usize) -> Option<Flow> {
        if depth > 16 {
            return None;
        }
        for statement in body {
            *budget = budget.checked_sub(1)?;
            match statement {
                Stmt::If(branch) => {
                    let body = if self.value(&branch.test, 0)?.truthy() {
                        &branch.body
                    } else {
                        &branch.orelse
                    };
                    match self.walk(body, depth + 1, budget)? {
                        Flow::Next => {}
                        exit => return Some(exit),
                    }
                }
                Stmt::Raise(raise) => {
                    return Some(
                        if line_for_range_start(self.source, raise.range) == self.line {
                            Flow::Target
                        } else {
                            Flow::Exit
                        },
                    );
                }
                Stmt::Return(_) => return Some(Flow::Exit),
                Stmt::Pass(_) => {}
                _ if docstring(statement) => {}
                _ => return None,
            }
        }
        Some(Flow::Next)
    }

    fn value(&self, expression: &Expr, depth: usize) -> Option<Value> {
        if depth > 16 {
            return None;
        }
        match expression {
            Expr::Constant(_) | Expr::Name(_) => evaluate(expression, &scope(self.parameters)),
            Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::Not => Some(Value::Bool(
                !self.value(&unary.operand, depth + 1)?.truthy(),
            )),
            Expr::Compare(compare) => {
                let mut left = self.value(&compare.left, depth + 1)?;
                for (operator, right) in compare.ops.iter().zip(&compare.comparators) {
                    let right = self.value(right, depth + 1)?;
                    if !comparison(operator, &left, &right, false)? {
                        return Some(Value::Bool(false));
                    }
                    left = right;
                }
                Some(Value::Bool(true))
            }
            Expr::Call(call)
                if self.len_intact
                    && call.keywords.is_empty()
                    && matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "len") =>
            {
                let [argument] = call.args.as_slice() else {
                    return None;
                };
                let Value::Str(value) = self.value(argument, depth + 1)? else {
                    return None;
                };
                Some(Value::Int(value.chars().count().try_into().ok()?))
            }
            _ => None,
        }
    }
}
