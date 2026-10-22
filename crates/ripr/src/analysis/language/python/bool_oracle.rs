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
//! Anything else — a non-literal argument, a method, a decorated or
//! re-exported-under-another-name owner, two candidate owners, a `bool`
//! operand the assertion does not pin — stays smoke.

use super::import_source_module_matches_owner;
use super::related_tests::related_test_relation;
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
    let mut parsed: BTreeMap<PathBuf, Option<Mod>> = BTreeMap::new();
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
            let module = parsed.entry(owner.file.clone()).or_insert_with(|| {
                read_source(&owner.file)
                    .and_then(|source| parse_module_result(&owner.file, source).ok())
            });
            if let Some(Mod::Module(module)) = module
                && owner_returns_bool(&module.body, owner, &call.bool_arguments)
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
    // A rebinding of the name after the definition can swap the function out.
    if module.iter().any(|stmt| rebinds_name(stmt, &owner.name)) {
        return false;
    }
    let args = &function.args;
    if args.vararg.is_some() || args.kwarg.is_some() || !function.decorator_list.is_empty() {
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

fn rebinds_name(stmt: &Stmt, name: &str) -> bool {
    match stmt {
        Stmt::Assign(assign) => assign
            .targets
            .iter()
            .any(|target| matches!(target, Expr::Name(target) if target.id.as_str() == name)),
        Stmt::AnnAssign(assign) => {
            matches!(assign.target.as_ref(), Expr::Name(target) if target.id.as_str() == name)
        }
        Stmt::ClassDef(class) => class.name.as_str() == name,
        Stmt::AsyncFunctionDef(function) => function.name.as_str() == name,
        _ => false,
    }
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
