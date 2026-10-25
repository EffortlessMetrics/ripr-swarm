//! A direct bare assertion can discriminate a bounded `__bool__` edit (#9795).
//! Read both predicates for the integer stored by a plain constructor. Unknown
//! inputs, unchanged truth values and wider object behavior retain abstention.

use super::module_value::ModuleEdit;
use super::related_tests::{import_source_module_matches_owner, owner_class_locals};
use super::sink_alignment::function_scope_stand_in;
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, text_for_range};
use super::{PythonAssertion, PythonOwner, PythonTest};
use rustpython_parser::ast::{self, Expr, Mod, Stmt};

mod workspace_admission;

const PLAIN_METHODS: [&str; 2] = ["__init__", "__bool__"];

/// Bind this capability's observing file only after a bounded physical audit.
pub(super) fn bind_workspace_observer(
    root: &std::path::Path,
    owners: &mut [PythonOwner],
    sources: &std::collections::HashMap<std::path::PathBuf, String>,
    incomplete: bool,
) {
    let pair = workspace_admission::closed_sources(root, sources, incomplete);
    for owner in owners.iter_mut().filter(|owner| owner.name == "__bool__") {
        owner.class_facts.truthiness_observer = pair.as_ref().and_then(|pair| {
            if pair[0] == owner.file {
                Some(pair[1].clone())
            } else if pair[1] == owner.file {
                Some(pair[0].clone())
            } else {
                None
            }
        });
    }
}

/// This capability accepts one import and one plain test definition. Extra
/// definitions can execute defaults/annotations or supersede the selected test.
/// Retain the fact once at extraction; other Python classifiers do not read it.
pub(super) fn plain_test_module(statements: &[Stmt]) -> bool {
    let mut imports = 0;
    let mut functions = 0;
    for statement in statements {
        match statement {
            Stmt::ImportFrom(_) => imports += 1,
            Stmt::FunctionDef(function)
                if plain_method(function) && function.args.args.is_empty() =>
            {
                functions += 1
            }
            Stmt::Expr(expr) if matches!(expr.value.as_ref(), Expr::Constant(_)) => {}
            _ => return false,
        }
    }
    imports == 1 && functions == 1
}

pub(super) struct BoolCredit<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) assertion: &'a PythonAssertion,
}

pub(super) fn credit<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    module_owner: &PythonOwner,
    tests: &'a [PythonTest],
) -> Option<BoolCredit<'a>> {
    if owner.name != "__bool__" || !owner.class_facts.plain || !owner.class_facts.no_bases {
        return None;
    }
    let old_line = edit.old_line_text?;
    let indentation = |text: &str| text.len() - text.trim_start().len();
    if old_line[..indentation(old_line)] != edit.line_text[..indentation(edit.line_text)] {
        return None;
    }
    let Mod::Module(module) = parse_module_result(edit.file, edit.new_source).ok()? else {
        return None;
    };
    // A sole plain top-level class: imports, class attributes and other module
    // execution could replace the constructor or install a descriptor.
    let mut classes = Vec::new();
    for statement in &module.body {
        match statement {
            Stmt::ClassDef(class) => classes.push(class),
            Stmt::Expr(expr) if matches!(expr.value.as_ref(), Expr::Constant(_)) => {}
            _ => return None,
        }
    }
    let [class] = classes.as_slice() else {
        return None;
    };
    if class.name.as_str() != owner.class_path
        || !class.bases.is_empty()
        || !class.keywords.is_empty()
        || !class.decorator_list.is_empty()
        || !class.type_params.is_empty()
    {
        return None;
    }
    let mut methods = Vec::new();
    for statement in &class.body {
        match statement {
            Stmt::FunctionDef(method)
                if plain_method(method) && PLAIN_METHODS.contains(&method.name.as_str()) =>
            {
                methods.push(method)
            }
            Stmt::Expr(expr) if matches!(expr.value.as_ref(), Expr::Constant(_)) => {}
            _ => return None,
        }
    }
    let init = unique_method(&methods, "__init__")?;
    let truth = unique_method(&methods, "__bool__")?;
    let [receiver, parameter] = init.args.args.as_slice() else {
        return None;
    };
    let [Stmt::Assign(assign)] = init.body.as_slice() else {
        return None;
    };
    let [Expr::Attribute(target)] = assign.targets.as_slice() else {
        return None;
    };
    if !is_name(&target.value, receiver.def.arg.as_str())
        || !is_name(&assign.value, parameter.def.arg.as_str())
        || truth.args.args.len() != 1
    {
        return None;
    }
    let [Stmt::Return(ret)] = truth.body.as_slice() else {
        return None;
    };
    if SourceText::new(edit.new_source).line_for_offset(usize::from(ret.range.start())) != edit.line
        || text_for_range(edit.new_source, ret.range).trim() != edit.line_text.trim()
    {
        return None;
    }
    let new = predicate(edit.line_text, truth.args.args[0].def.arg.as_str())?;
    let old = predicate(old_line, truth.args.args[0].def.arg.as_str())?;
    if new.field != target.attr.as_str() || old.field != new.field || new.field.starts_with("__") {
        return None;
    }
    for name in [
        &owner.class_path,
        "__init__",
        "__bool__",
        new.field.as_str(),
    ] {
        if module_owner.foreign_writes.may_rebind(name, edit.file) {
            return None;
        }
    }
    for test in tests {
        if owner.class_facts.truthiness_observer.as_ref() != Some(&test.file)
            || !test.decorators.is_empty()
            || !test.fixtures.is_empty()
            || !test.constant_rebinding.plain_test_module
            || test.imports.len() != 1
            || test.constant_rebinding.module_rebinds(&test.name)
            || test.constant_rebinding.implicit_reach
            || test.constant_rebinding.edited_in_diff
            || test.constant_rebinding.module_opaque
            || super::test_activation::activation_control(test).is_some()
        {
            continue;
        }
        if [
            &owner.class_path,
            "__init__",
            "__bool__",
            new.field.as_str(),
        ]
        .iter()
        .any(|name| test.constant_rebinding.assigns_attribute(name))
        {
            continue;
        }
        let Ok(Mod::Module(body)) = parse_module_result(&test.file, &test.body_text) else {
            continue;
        };
        let [Stmt::FunctionDef(function)] = body.body.as_slice() else {
            continue;
        };
        if !plain_method(function)
            || !function.args.args.is_empty()
            || function.name.as_str() != test.name
        {
            continue;
        }
        let [Stmt::Assert(assertion)] = function.body.as_slice() else {
            continue;
        };
        let (condition, negated) = match assertion.test.as_ref() {
            Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::Not => (unary.operand.as_ref(), true),
            condition => (condition, false),
        };
        let Expr::Call(call) = condition else {
            continue;
        };
        let Expr::Name(callee) = call.func.as_ref() else {
            continue;
        };
        let [argument] = call.args.as_slice() else {
            continue;
        };
        if !call.keywords.is_empty()
            || !test.imports.iter().any(|import| {
                import.alias == callee.id.as_str()
                    && import.imported == owner.class_path
                    && import_source_module_matches_owner(import, owner, &test.file)
            })
            || !owner_class_locals(test, owner, &owner.class_path)
                .iter()
                .any(|name| name == callee.id.as_str())
            || function_scope_stand_in(test, callee.id.as_str(), owner)
            || test.constant_rebinding.rebinds(callee.id.as_str())
        {
            continue;
        }
        let Some(value) = integer(argument) else {
            continue;
        };
        if old.eval(value) != negated && new.eval(value) == negated {
            let text = text_for_range(&test.body_text, assertion.range);
            let extracted = test
                .assertions
                .iter()
                .find(|item| item.text.trim() == text.trim())?;
            return Some(BoolCredit {
                test,
                assertion: extracted,
            });
        }
    }
    None
}

fn plain_method(method: &ast::StmtFunctionDef) -> bool {
    method.decorator_list.is_empty()
        && method.type_params.is_empty()
        && method.returns.is_none()
        && method.args.posonlyargs.is_empty()
        && method.args.kwonlyargs.is_empty()
        && method.args.vararg.is_none()
        && method.args.kwarg.is_none()
        && method
            .args
            .args
            .iter()
            .all(|arg| arg.default.is_none() && arg.def.annotation.is_none())
}

fn unique_method<'a>(
    methods: &[&'a ast::StmtFunctionDef],
    name: &str,
) -> Option<&'a ast::StmtFunctionDef> {
    let mut found = methods
        .iter()
        .copied()
        .filter(|method| method.name.as_str() == name);
    let method = found.next()?;
    found.next().is_none().then_some(method)
}

struct Predicate {
    field: String,
    operator: ast::CmpOp,
    threshold: i64,
}

fn predicate(line: &str, receiver: &str) -> Option<Predicate> {
    let Mod::Module(module) =
        parse_module_result(std::path::Path::new("<bool-return>"), line.trim()).ok()?
    else {
        return None;
    };
    let [Stmt::Return(ret)] = module.body.as_slice() else {
        return None;
    };
    let Expr::Compare(compare) = ret.value.as_deref()? else {
        return None;
    };
    let Expr::Attribute(attribute) = compare.left.as_ref() else {
        return None;
    };
    let [operator] = compare.ops.as_slice() else {
        return None;
    };
    let [right] = compare.comparators.as_slice() else {
        return None;
    };
    if !is_name(&attribute.value, receiver)
        || !matches!(
            operator,
            ast::CmpOp::Eq
                | ast::CmpOp::NotEq
                | ast::CmpOp::Lt
                | ast::CmpOp::LtE
                | ast::CmpOp::Gt
                | ast::CmpOp::GtE
        )
    {
        return None;
    }
    Some(Predicate {
        field: attribute.attr.to_string(),
        operator: *operator,
        threshold: integer(right)?,
    })
}

impl Predicate {
    fn eval(&self, value: i64) -> bool {
        match self.operator {
            ast::CmpOp::Eq => value == self.threshold,
            ast::CmpOp::NotEq => value != self.threshold,
            ast::CmpOp::Lt => value < self.threshold,
            ast::CmpOp::LtE => value <= self.threshold,
            ast::CmpOp::Gt => value > self.threshold,
            ast::CmpOp::GtE => value >= self.threshold,
            _ => false,
        }
    }
}

fn is_name(expr: &Expr, expected: &str) -> bool {
    matches!(expr, Expr::Name(name) if name.id.as_str() == expected)
}

fn integer(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Int(value) => value.to_string().parse().ok(),
            _ => None,
        },
        Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::USub => {
            integer(&unary.operand)?.checked_neg()
        }
        _ => None,
    }
}
