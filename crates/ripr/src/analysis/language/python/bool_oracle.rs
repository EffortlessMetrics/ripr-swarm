//! Bare boolean assertions on a `bool`-returning owner (#9542).
//!
//! `assert is_adult(18)` and `assert not is_adult(17)` are exact pins when
//! `is_adult` returns a `bool`: the first fails on `False`, the second on
//! `True`, and nothing else can reach either. Extraction cannot tell, because
//! the callee lives in another file, so it records the assertion as smoke and
//! keeps the call in [`PythonBareBoolCall`](super::PythonBareBoolCall). This
//! pass runs once owners and tests are both known and re-reads such an
//! assertion as `f(lit, ..) == True` / `== False` when exactly one workspace
//! function owner is its callee and that owner provably returns a `bool` for
//! the literal arguments the assertion passes.
//!
//! "Provably" is deliberately narrow, because a wrong exact credit hides a
//! missing test. The owner must be an undecorated, non-async, non-generator
//! function with no `*args` / `**kwargs`, and either
//!
//! - declare `-> bool`, or
//! - have a body of one `return <expr>` (after an optional docstring) where
//!   `<expr>` is a comparison, a `not`, a `True`/`False` literal, or an
//!   `and` / `or` whose every operand is one of those or a parameter that the
//!   assertion binds to a `True`/`False` literal.
//!
//! The name must also have no other binding anywhere in the owner's module
//! (imports, `if`/`try` redefinitions, `del`, loops), the owner must not be a
//! generator, and the test file must not patch, mock or rebind the callee. A
//! `Compare` is taken as a `bool`; a type with a custom `__eq__` returning
//! another value is an accepted residual.
//!
//! Anything else — a non-literal argument, a method, a decorated or
//! re-exported-under-another-name owner, two candidate owners, a `bool`
//! operand the assertion does not pin — stays smoke.

use super::import_source_module_matches_owner;
use super::related_tests::related_test_relation;
use super::sink_alignment::{binds_by_walrus, pattern_binds_name};
use super::source_facts::parse_module_result;
use super::{PythonAssertion, PythonBareBoolCall, PythonOracleShape, PythonOwner, PythonTest};
use crate::domain::{OracleKind, OracleStrength, OwnerKind};
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::{collections::BTreeMap, path::Path, path::PathBuf};

/// Credit every bare boolean assertion whose callee is a resolved `bool` owner.
pub(super) fn credit_bare_bool_asserts<'a>(
    tests: &mut [PythonTest],
    owners: &[PythonOwner],
    read_source: impl Fn(&Path) -> Option<&'a str>,
) {
    let mut parsed: BTreeMap<PathBuf, Option<(Mod, &'a str)>> = BTreeMap::new();
    let mut kept: BTreeMap<(PathBuf, String), bool> = BTreeMap::new();
    let mut credits: Vec<(usize, usize, bool)> = Vec::new();
    for (test_index, test) in tests.iter().enumerate() {
        for (assertion_index, assertion) in test.assertions.iter().enumerate() {
            let Some(call) = &assertion.bare_bool_call else {
                continue;
            };
            if assertion.oracle_strength != OracleStrength::Smoke {
                continue;
            }
            let Some(owner) = unique_callee_owner(test, owners, call) else {
                continue;
            };
            // A patched or locally rebound callee is not the owner under test.
            let keeps_callee = *kept
                .entry((test.file.clone(), call.callee.clone()))
                .or_insert_with(|| {
                    read_source(&test.file)
                        .is_some_and(|source| test_keeps_callee(source, &test.file, &call.callee))
                });
            if !keeps_callee {
                continue;
            }
            let module = parsed.entry(owner.file.clone()).or_insert_with(|| {
                let source = read_source(&owner.file)?;
                let module = parse_module_result(&owner.file, source).ok()?;
                Some((module, source))
            });
            if let Some((Mod::Module(module), source)) = module
                && owner_returns_bool(&module.body, source, owner, &call.bool_arguments)
            {
                credits.push((test_index, assertion_index, call.negated));
            }
        }
    }
    for (test_index, assertion_index, negated) in credits {
        let assertion = &mut tests[test_index].assertions[assertion_index];
        credit(assertion, negated);
    }
}

/// The one function owner the test's call resolves to, by the same relation
/// rules every other related-test consumer uses.
fn unique_callee_owner<'o>(
    test: &PythonTest,
    owners: &'o [PythonOwner],
    call: &PythonBareBoolCall,
) -> Option<&'o PythonOwner> {
    let import = test
        .imports
        .iter()
        .find(|import| import.alias == call.callee);
    let name = import.map_or(call.callee.as_str(), |import| import.imported.as_str());
    let mut matching = owners
        .iter()
        .filter(|owner| owner.name == name)
        .filter(|owner| related_test_relation(test, owner).is_some())
        // The relation also matches on call text alone; an imported callee must
        // come from the owner's own module, and a bare one from the test's file.
        .filter(|owner| match import {
            Some(import) => import_source_module_matches_owner(import, owner, &test.file),
            None => owner.file == test.file,
        });
    let owner = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    matches!(
        owner.owner_kind,
        Some(OwnerKind::Function | OwnerKind::ModuleFunction)
    )
    .then_some(owner)
}

/// Markers of dynamic rebinding or patching. Textual on purpose: a hit in a
/// comment or string only leaves the assertion smoke.
const DYNAMIC_MARKERS: [&str; 12] = [
    "patch",
    "mock",
    "Mock",
    "setattr",
    "globals(",
    "vars(",
    "exec(",
    "eval(",
    "__code__",
    "__dict__",
    "sys.modules",
    "importlib",
];

fn may_rebind_dynamically(source: &str, name: &str) -> bool {
    DYNAMIC_MARKERS.iter().any(|marker| source.contains(marker)) || binds_by_walrus(source, name)
}

/// Whether the test file keeps `callee` bound to the one import that brought it
/// in: exactly one binding of the name anywhere in the file (the import itself)
/// and no dynamic rebinding. A file that does not parse is not trusted.
fn test_keeps_callee(source: &str, path: &Path, callee: &str) -> bool {
    if may_rebind_dynamically(source, callee) {
        return false;
    }
    let Ok(Mod::Module(module)) = parse_module_result(path, source) else {
        return false;
    };
    count_bindings(&module.body, callee) == 1
}

fn credit(assertion: &mut PythonAssertion, negated: bool) {
    let Some(call) = &assertion.bare_bool_call else {
        return;
    };
    // The reported oracle text stays the user's assertion; the gates read the
    // comparison it is equivalent to.
    assertion.gate_text = format!(
        "assert {} == {}",
        call.call_text,
        if negated { "False" } else { "True" }
    );
    assertion.oracle_kind = OracleKind::ExactValue;
    assertion.oracle_strength = OracleStrength::Strong;
    assertion.oracle_shape = PythonOracleShape::ExactAssertion;
}

fn owner_returns_bool(
    module: &[Stmt],
    source: &str,
    owner: &PythonOwner,
    bool_arguments: &[Option<bool>],
) -> bool {
    if !owner.decorators.is_empty() {
        return false;
    }
    let mut definitions = module.iter().filter_map(|stmt| match stmt {
        Stmt::FunctionDef(function) if function.name.as_str() == owner.name => Some(function),
        _ => None,
    });
    let (Some(function), None) = (definitions.next(), definitions.next()) else {
        return false;
    };
    // Dynamic rebinding, then any binding of the name besides the owner's own
    // definition, at any depth (the definition itself is the one allowed).
    if may_rebind_dynamically(source, &owner.name) || count_bindings(module, &owner.name) != 1 {
        return false;
    }
    let args = &function.args;
    if args.vararg.is_some() || args.kwarg.is_some() || !function.decorator_list.is_empty() {
        return false;
    }
    // A generator returns a generator object whatever it is annotated with.
    let body_text = source
        .get(usize::from(function.range.start())..usize::from(function.range.end()))
        .unwrap_or("yield");
    if body_text.contains("yield") {
        return false;
    }
    if function.returns.as_deref().is_some_and(
        |annotation| matches!(annotation, Expr::Name(name) if name.id.as_str() == "bool"),
    ) {
        return true;
    }
    let parameters: Vec<&str> = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .map(|arg| arg.def.arg.as_str())
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
        return false;
    };
    let Some(value) = ret.value.as_deref() else {
        return false;
    };
    bool_valued(value, &parameters, bool_arguments)
}

/// How many statements, at any depth including function and class bodies,
/// bind `name`. A star import counts as binding every name.
fn count_bindings(body: &[Stmt], name: &str) -> usize {
    body.iter().map(|stmt| stmt_bindings(stmt, name)).sum()
}

fn stmt_bindings(stmt: &Stmt, name: &str) -> usize {
    let target = |target: &Expr| usize::from(names_target(target, name));
    let nested = |body: &[Stmt]| count_bindings(body, name);
    let parameters = |args: &ast::Arguments| {
        args.posonlyargs
            .iter()
            .chain(&args.args)
            .chain(&args.kwonlyargs)
            .map(|arg| arg.def.arg.as_str())
            .chain(args.vararg.iter().map(|arg| arg.arg.as_str()))
            .chain(args.kwarg.iter().map(|arg| arg.arg.as_str()))
            .filter(|parameter| *parameter == name)
            .count()
    };
    match stmt {
        Stmt::FunctionDef(function) => {
            usize::from(function.name.as_str() == name)
                + parameters(&function.args)
                + nested(&function.body)
        }
        Stmt::AsyncFunctionDef(function) => {
            usize::from(function.name.as_str() == name)
                + parameters(&function.args)
                + nested(&function.body)
        }
        Stmt::ClassDef(class) => usize::from(class.name.as_str() == name) + nested(&class.body),
        Stmt::Assign(assign) => assign.targets.iter().map(target).sum(),
        Stmt::TypeAlias(alias) => target(&alias.name),
        Stmt::AugAssign(assign) => target(&assign.target),
        Stmt::AnnAssign(assign) => target(&assign.target),
        Stmt::Delete(delete) => delete.targets.iter().map(target).sum(),
        Stmt::Import(import) => import
            .names
            .iter()
            .filter(|alias| alias_binds(alias, name))
            .count(),
        Stmt::ImportFrom(import) => import
            .names
            .iter()
            .filter(|alias| alias_binds(alias, name))
            .count(),
        Stmt::For(stmt) => target(&stmt.target) + nested(&stmt.body) + nested(&stmt.orelse),
        Stmt::AsyncFor(stmt) => target(&stmt.target) + nested(&stmt.body) + nested(&stmt.orelse),
        Stmt::While(stmt) => nested(&stmt.body) + nested(&stmt.orelse),
        Stmt::If(stmt) => nested(&stmt.body) + nested(&stmt.orelse),
        Stmt::With(stmt) => {
            stmt.items
                .iter()
                .filter(|item| item.optional_vars.as_deref().is_some_and(names_name(name)))
                .count()
                + nested(&stmt.body)
        }
        Stmt::AsyncWith(stmt) => {
            stmt.items
                .iter()
                .filter(|item| item.optional_vars.as_deref().is_some_and(names_name(name)))
                .count()
                + nested(&stmt.body)
        }
        Stmt::Try(stmt) => try_bindings(
            &stmt.body,
            &stmt.handlers,
            &stmt.orelse,
            &stmt.finalbody,
            name,
        ),
        Stmt::TryStar(stmt) => try_bindings(
            &stmt.body,
            &stmt.handlers,
            &stmt.orelse,
            &stmt.finalbody,
            name,
        ),
        Stmt::Match(stmt) => stmt
            .cases
            .iter()
            .map(|case| usize::from(pattern_binds_name(&case.pattern, name)) + nested(&case.body))
            .sum(),
        Stmt::Global(global) => {
            usize::from(global.names.iter().any(|bound| bound.as_str() == name))
        }
        Stmt::Nonlocal(nonlocal) => {
            usize::from(nonlocal.names.iter().any(|bound| bound.as_str() == name))
        }
        _ => 0,
    }
}

fn names_name(name: &str) -> impl Fn(&Expr) -> bool + '_ {
    move |target| names_target(target, name)
}

fn try_bindings(
    body: &[Stmt],
    handlers: &[ast::ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
    name: &str,
) -> usize {
    count_bindings(body, name)
        + count_bindings(orelse, name)
        + count_bindings(finalbody, name)
        + handlers
            .iter()
            .map(|handler| {
                let ast::ExceptHandler::ExceptHandler(handler) = handler;
                usize::from(
                    handler
                        .name
                        .as_ref()
                        .is_some_and(|bound| bound.as_str() == name),
                ) + count_bindings(&handler.body, name)
            })
            .sum::<usize>()
}

fn names_target(target: &Expr, name: &str) -> bool {
    match target {
        Expr::Name(target) => target.id.as_str() == name,
        Expr::Tuple(tuple) => tuple.elts.iter().any(|elt| names_target(elt, name)),
        Expr::List(list) => list.elts.iter().any(|elt| names_target(elt, name)),
        Expr::Starred(starred) => names_target(&starred.value, name),
        // `module.name = ...` replaces the attribute the import resolved through.
        Expr::Attribute(attribute) => attribute.attr.as_str() == name,
        _ => false,
    }
}

fn alias_binds(alias: &ast::Alias, name: &str) -> bool {
    if alias.name.as_str() == "*" {
        return true;
    }
    let bound = alias.asname.as_ref().map_or_else(
        || alias.name.as_str().split('.').next().unwrap_or(""),
        |asname| asname.as_str(),
    );
    bound == name
}

/// Whether `expr` is a `bool` for the literal arguments the assertion passes.
fn bool_valued(expr: &Expr, parameters: &[&str], bool_arguments: &[Option<bool>]) -> bool {
    match expr {
        Expr::Compare(_) => true,
        Expr::UnaryOp(unary) => unary.op == ast::UnaryOp::Not,
        Expr::Constant(ast::ExprConstant {
            value: ast::Constant::Bool(_),
            ..
        }) => true,
        Expr::BoolOp(bool_op) => bool_op.values.iter().all(|operand| {
            bool_valued(operand, parameters, bool_arguments)
                || bound_bool_parameter(operand, parameters, bool_arguments)
        }),
        _ => false,
    }
}

/// A bare parameter name the assertion binds to a `True`/`False` literal.
fn bound_bool_parameter(
    operand: &Expr,
    parameters: &[&str],
    bool_arguments: &[Option<bool>],
) -> bool {
    let Expr::Name(name) = operand else {
        return false;
    };
    parameters
        .iter()
        .position(|parameter| *parameter == name.id.as_str())
        .and_then(|position| bool_arguments.get(position))
        .is_some_and(Option::is_some)
}
