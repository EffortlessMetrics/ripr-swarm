//! An arithmetic operator edit around a call to an imported workspace
//! function, read through an exact assertion (#9619).
//!
//! `return lookup(tier) * units` -> `lookup(tier) + units` calls a function the
//! preview adapter does not follow (`missing_import_graph`). The call does not
//! have to be followed: the one changed line moves only the operator, so the
//! call returns the same value `X` before and after. An exact assertion
//! `quote("gold", 3) == 30` holds under the old operator, which fixes `X`
//! (here 10), and either holds or fails under the new one with that same `X`.
//! This pass credits only an assertion it can show fails:
//!
//! - the edit is the file's only changed line and swaps `+`, `-` or `*` for
//!   another of them in the single `return` of an undecorated module function
//!   that is bound once and not rewritten elsewhere;
//! - one operand is a call, by bare name, of an imported function that resolves
//!   to exactly one undecorated module-level function of the workspace, with
//!   parameters and literals as arguments, and the other operand is an
//!   expression over the parameters and integer literals;
//! - no file the owner or the callee imports, directly or through other workspace
//!   files, defines `__add__`, `__sub__`, `__mul__` or their reflected forms,
//!   which could answer the operator with the pinned value itself;
//! - a straight-line `assert owner(<literals>) == <integer literal>`
//!   of a test names the owner through an import the test does not rebind;
//! - the old operator, the literals and the assertion fix a whole number `X`,
//!   and the new operator over the same `X` does not give the asserted value.
//!
//! The old assertion is taken to hold, as for any test that passes before the
//! diff. Anything else keeps the `missing_import_graph` abstention.

use super::module_constants::ModuleBindingFacts;
use super::module_value::{
    ModuleEdit, ModuleValueCredit, Scope, Value, assertion_expression, evaluate, reader_spellings,
    straight_line_assertion, with_line_replaced,
};
use super::related_tests::{import_source_module_matches_owner, owner_module_paths};
use super::source_facts::parse_module_result;
use super::{PythonOwner, PythonTest, expr_full_name};
use crate::domain::OwnerKind;
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

/// The limit evidence this credit replaces.
pub(super) const IMPORTED_CALL_LIMIT_EVIDENCE: &str =
    "static_limit missing_import_graph: changed line calls an imported symbol";

/// Methods that let a workspace type answer `+`, `-` or `*` with something
/// other than the number the assertion pins, so the call result is not a
/// number the old operator can be solved for.
const ARITHMETIC_METHODS: [&str; 6] = [
    "__add__", "__radd__", "__sub__", "__rsub__", "__mul__", "__rmul__",
];

/// The workspace files `starts` import, directly or through other workspace
/// files: every file whose module path a `from M import Y` (as `M` or `M.Y`) or
/// `import M` of a reached file names.
fn reachable_files<'a>(owners: &'a [PythonOwner], starts: &[&'a Path]) -> Vec<&'a Path> {
    let mut reached: Vec<&Path> = starts.to_vec();
    let mut index = 0;
    while let Some(&path) = reached.get(index) {
        index += 1;
        let names: Vec<String> = owners
            .iter()
            .filter(|owner| owner.file == path)
            .flat_map(|owner| owner.imports.iter())
            .flat_map(|import| {
                [
                    import.source_module.clone(),
                    format!("{}.{}", import.source_module, import.imported),
                    import.imported.clone(),
                ]
            })
            .collect();
        for owner in owners {
            if !reached.iter().any(|seen| *seen == owner.file)
                && owner_module_paths(&owner.file)
                    .iter()
                    .any(|module| names.contains(module))
            {
                reached.push(owner.file.as_path());
            }
        }
    }
    reached
}

/// The one-`return` shape of the owner: `call OP other` or `other OP call`.
struct Shape {
    call: Expr,
    other: Expr,
    call_on_left: bool,
    operator: ast::Operator,
    parameters: Vec<String>,
}

/// The credits for an operator swap around an imported call. Empty when the
/// edit is not that swap or no assertion fails under the new operator.
pub(super) fn imported_call_credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    owners: &[PythonOwner],
    all_tests: &'a [PythonTest],
) -> Vec<ModuleValueCredit<'a>> {
    credits(edit, owner, owners, all_tests).unwrap_or_default()
}

fn credits<'a>(
    edit: &ModuleEdit<'_>,
    owner: &PythonOwner,
    owners: &[PythonOwner],
    all_tests: &'a [PythonTest],
) -> Option<Vec<ModuleValueCredit<'a>>> {
    let ModuleEdit {
        file,
        line,
        old_line_text,
        new_source,
        ..
    } = *edit;
    let old_line_text = old_line_text?;
    if owner.owner_kind != Some(OwnerKind::Function)
        || owner.qualified_name != owner.name
        || owner.is_module_owner()
    {
        return None;
    }
    let module_owner = owners
        .iter()
        .find(|candidate| candidate.is_module_owner() && candidate.file == file)?;
    let old_source = with_line_replaced(new_source, line, old_line_text)?;
    let (Mod::Module(new_module), Mod::Module(old_module)) = (
        parse_module_result(file, new_source).ok()?,
        parse_module_result(file, &old_source).ok()?,
    ) else {
        return None;
    };
    let new_shape = shape_of(&new_module.body, &owner.name)?;
    let old_shape = shape_of(&old_module.body, &owner.name)?;
    // Only the operator moves: the call, the other operand, their order and the
    // parameters are the same text.
    if !same_expression(&new_shape.call, &old_shape.call)
        || !same_expression(&new_shape.other, &old_shape.other)
        || new_shape.call_on_left != old_shape.call_on_left
        || new_shape.parameters != old_shape.parameters
    {
        return None;
    }
    let Expr::Call(call) = &new_shape.call else {
        return None;
    };
    let callee = match call.func.as_ref() {
        Expr::Name(name) => name.id.to_string(),
        _ => return None,
    };
    if !call.keywords.is_empty()
        || !call.args.iter().all(|argument| match argument {
            Expr::Name(name) => new_shape
                .parameters
                .iter()
                .any(|parameter| parameter == name.id.as_str()),
            Expr::Constant(_) => true,
            _ => false,
        })
    {
        return None;
    }
    // The call names an import that resolves to one undecorated workspace
    // function, and nothing rebinds it or the owner.
    for (source, module) in [
        (new_source, &new_module),
        (old_source.as_str(), &old_module),
    ] {
        let facts = ModuleBindingFacts::of(source, &module.body);
        if !facts.bound_once(&callee) || !facts.bound_once(&owner.name) {
            return None;
        }
    }
    if module_owner.foreign_writes.may_rebind(&callee, file)
        || module_owner.foreign_writes.may_rebind(&owner.name, file)
        || module_owner.foreign_writes.may_replace_reader_code()
    {
        return None;
    }
    let imports: Vec<_> = owner
        .imports
        .iter()
        .filter(|import| import.alias == callee)
        .collect();
    let [import] = imports.as_slice() else {
        return None;
    };
    let mut resolved = owners.iter().filter(|candidate| {
        candidate.owner_kind == Some(OwnerKind::Function)
            && candidate.name == import.imported
            && candidate.qualified_name == candidate.name
            && import_source_module_matches_owner(import, candidate, file)
    });
    let target = resolved.next()?;
    if resolved.next().is_some() || !target.decorators.is_empty() {
        return None;
    }
    // The integer reading of the call result holds only while no type the
    // callee can reach defines its own arithmetic.
    let reachable = reachable_files(owners, &[file, target.file.as_path()]);
    if owners.iter().any(|candidate| {
        ARITHMETIC_METHODS.contains(&candidate.name.as_str())
            && reachable.iter().any(|path| *path == candidate.file)
    }) {
        return None;
    }

    let mut out = Vec::new();
    for test in all_tests {
        if super::test_activation::activation_control(test).is_some()
            || test.constant_rebinding.edited_in_diff
            || test.constant_rebinding.module_opaque
        {
            continue;
        }
        let spellings = reader_spellings(test, owner, &module_owner.foreign_writes);
        if spellings.is_empty() {
            continue;
        }
        for assertion in &test.assertions {
            let Some(expression) = assertion_expression(assertion) else {
                continue;
            };
            if !straight_line_assertion(test, assertion) {
                continue;
            }
            let Some((arguments, expected)) = pinned_call(&expression, &spellings) else {
                continue;
            };
            if arguments.len() != new_shape.parameters.len() {
                continue;
            }
            let bound: Vec<(String, Value)> = new_shape
                .parameters
                .iter()
                .cloned()
                .zip(arguments)
                .collect();
            if !fails_under_new_operator(&old_shape, &new_shape, &bound, expected) {
                continue;
            }
            out.push(ModuleValueCredit {
                test,
                assertion,
                constant: format!("{callee}(..)"),
                old: operator_text(old_shape.operator).to_string(),
                new: operator_text(new_shape.operator).to_string(),
                reader: Some(owner.name.clone()),
            });
            break;
        }
    }
    Some(out)
}

/// `call(<literals>) == <integer literal>`, either way round: the
/// argument values and the asserted value.
fn pinned_call(expression: &Expr, spellings: &[String]) -> Option<(Vec<Value>, i128)> {
    let Expr::Compare(compare) = expression else {
        return None;
    };
    let ([ast::CmpOp::Eq], [right]) = (compare.ops.as_slice(), compare.comparators.as_slice())
    else {
        return None;
    };
    let (call, expected) = match (compare.left.as_ref(), right) {
        (Expr::Call(call), value) | (value, Expr::Call(call)) => (call, value),
        _ => return None,
    };
    let callee = expr_full_name(&call.func)?;
    if !spellings.contains(&callee) || !call.keywords.is_empty() {
        return None;
    }
    let arguments = call.args.iter().map(literal).collect::<Option<Vec<_>>>()?;
    match literal(expected)? {
        Value::Int(expected) => Some((arguments, expected)),
        _ => None,
    }
}

/// A literal, with an optional unary minus.
fn literal(expr: &Expr) -> Option<Value> {
    let unused = Value::Bool(false);
    let scope = Scope {
        constant: "",
        value: &unused,
        parameters: None,
        constant_spellings: &[],
        reader_spellings: &[],
        reader: None,
        model: None,
        receiver: None,
        floats: false,
        callee: None,
    };
    match expr {
        Expr::Constant(_) | Expr::UnaryOp(_) => evaluate(expr, &scope),
        _ => None,
    }
}

/// Whether the asserted value fixes the call result under the old operator
/// and the new operator then gives another value for that same result.
fn fails_under_new_operator(
    old: &Shape,
    new: &Shape,
    parameters: &[(String, Value)],
    expected: i128,
) -> bool {
    let unused = Value::Bool(false);
    let scope = Scope {
        constant: "",
        value: &unused,
        parameters: Some(parameters),
        constant_spellings: &[],
        reader_spellings: &[],
        reader: None,
        model: None,
        receiver: None,
        floats: false,
        callee: None,
    };
    let Some(Value::Int(other)) = evaluate(&old.other, &scope) else {
        return false;
    };
    let Some(call) = solve(old.operator, old.call_on_left, other, expected) else {
        return false;
    };
    apply(new.operator, new.call_on_left, call, other).is_some_and(|result| result != expected)
}

/// The call result `x` with `x OP other == expected` (or `other OP x`), when
/// it is one whole number.
fn solve(operator: ast::Operator, call_on_left: bool, other: i128, expected: i128) -> Option<i128> {
    match operator {
        ast::Operator::Add => expected.checked_sub(other),
        ast::Operator::Sub if call_on_left => expected.checked_add(other),
        ast::Operator::Sub => other.checked_sub(expected),
        ast::Operator::Mult => {
            (other != 0 && expected.checked_rem(other)? == 0).then(|| expected / other)
        }
        _ => None,
    }
}

fn apply(operator: ast::Operator, call_on_left: bool, call: i128, other: i128) -> Option<i128> {
    let (left, right) = if call_on_left {
        (call, other)
    } else {
        (other, call)
    };
    match operator {
        ast::Operator::Add => left.checked_add(right),
        ast::Operator::Sub => left.checked_sub(right),
        ast::Operator::Mult => left.checked_mul(right),
        _ => None,
    }
}

fn operator_text(operator: ast::Operator) -> &'static str {
    match operator {
        ast::Operator::Add => "+",
        ast::Operator::Sub => "-",
        ast::Operator::Mult => "*",
        _ => "?",
    }
}

/// Whether two expressions are the same tree, wherever they sit in their
/// files: the debug text with its source ranges removed.
fn same_expression(left: &Expr, right: &Expr) -> bool {
    structure(left) == structure(right)
}

fn structure(expr: &Expr) -> String {
    let text = format!("{expr:?}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find("range: ") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "range: ".len()..];
        let end = after
            .find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// The module function `name` as a `Shape`: positional parameters without
/// defaults, an optional docstring and one `return call OP other`.
fn shape_of(module: &[Stmt], name: &str) -> Option<Shape> {
    let function = module.iter().find_map(|stmt| match stmt {
        Stmt::FunctionDef(function) if function.name.as_str() == name => Some(function),
        _ => None,
    })?;
    let args = &function.args;
    if !function.decorator_list.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || !args.kwonlyargs.is_empty()
        || args
            .posonlyargs
            .iter()
            .chain(args.args.iter())
            .any(|arg| arg.default.is_some())
    {
        return None;
    }
    let parameters: Vec<String> = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .map(|arg| arg.def.arg.to_string())
        .collect();
    let body = match function.body.as_slice() {
        [Stmt::Expr(docstring), rest @ ..]
            if matches!(docstring.value.as_ref(), Expr::Constant(_)) && !rest.is_empty() =>
        {
            rest
        }
        all => all,
    };
    let [Stmt::Return(ret)] = body else {
        return None;
    };
    let Expr::BinOp(binary) = ret.value.as_deref()? else {
        return None;
    };
    if !matches!(
        binary.op,
        ast::Operator::Add | ast::Operator::Sub | ast::Operator::Mult
    ) {
        return None;
    }
    let (call, other, call_on_left) = match (binary.left.as_ref(), binary.right.as_ref()) {
        (call @ Expr::Call(_), other) if !matches!(other, Expr::Call(_)) => {
            (call.clone(), other.clone(), true)
        }
        (other, call @ Expr::Call(_)) if !matches!(other, Expr::Call(_)) => {
            (call.clone(), other.clone(), false)
        }
        _ => return None,
    };
    Some(Shape {
        call,
        other,
        call_on_left,
        operator: binary.op,
        parameters,
    })
}
