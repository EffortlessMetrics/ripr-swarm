//! Test registration is separate from execution and failure propagation.
//!
//! Conditions and runtime rebinding are not evaluated. Recognized controls
//! therefore withhold credit even when a conditional skip might be inactive,
//! except a condition that is itself a boolean literal and so provably inert
//! (`skipif(False)`), and `importorskip` of a module every interpreter has.

use super::oracles::iterates_nothing;
use super::sink_alignment::blank_quoted_strings;
use super::source_utils::{SourceText, text_for_range};
use super::{PythonFramework, PythonImport, PythonTest, expr_full_name};
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum DeclarationSite {
    Function,
    Class,
    Module,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct PythonActivationControl {
    name: String,
    site: DeclarationSite,
}

pub(super) fn activation_control(test: &PythonTest) -> Option<&str> {
    test.activation_controls
        .iter()
        .find(|control| match (control.name.as_str(), control.site) {
            ("unittest.expectedFailure", _)
            | (
                "unittest.skip" | "unittest.skipIf" | "unittest.skipUnless",
                DeclarationSite::Class,
            ) => test.framework == PythonFramework::Unittest,
            _ => true,
        })
        .map(|control| control.name.as_str())
}

pub(super) fn activation_controls(
    decorators: &[Expr],
    definition_imports: &[PythonImport],
    site: DeclarationSite,
) -> Vec<PythonActivationControl> {
    decorators
        .iter()
        .filter_map(|decorator| {
            let name = resolve_control(&expr_full_name(decorator)?, definition_imports)?;
            (!condition_is_inert(&name, decorator)).then_some(name)
        })
        .map(|name| PythonActivationControl { name, site })
        .collect()
}

/// A skip or xfail whose condition is a boolean literal that never triggers it:
/// `skipif(False, reason=...)`, `skipIf(False, ...)`, `skipUnless(True, ...)`,
/// `xfail(False, reason=...)`. Only the literal counts; a name, a comparison or
/// a string condition is evaluated at collection time and stays a control.
///
/// The call must also be one the runner accepts: pytest rejects a boolean
/// condition without a string `reason=`, ORs every positional condition and lets
/// `condition=` override them, and `unittest.skipIf` raises without its `reason`.
/// Any other shape keeps the control rather than guess what the runner does.
fn condition_is_inert(resolved: &str, decorator: &Expr) -> bool {
    let (inert_value, unittest_style) = match resolved {
        "pytest.mark.skipif" | "pytest.mark.xfail" => (false, false),
        "unittest.skipIf" => (false, true),
        "unittest.skipUnless" => (true, true),
        _ => return false,
    };
    let Expr::Call(call) = decorator else {
        return false;
    };
    if has_starred(&call.args) || call.keywords.iter().any(|keyword| keyword.arg.is_none()) {
        return false;
    }
    let keyword = |name: &str| {
        call.keywords
            .iter()
            .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg.as_str() == name))
            .map(|keyword| &keyword.value)
    };
    let positional = |index: usize| call.args.get(index);
    // `skipIf(condition, reason)` takes both positionally; pytest marks take
    // the condition alone and the reason only as a keyword.
    let max_positional = if unittest_style { 2 } else { 1 };
    if call.args.len() > max_positional {
        return false;
    }
    let condition = match (positional(0), keyword("condition")) {
        (Some(value), None) | (None, Some(value)) => value,
        _ => return false,
    };
    let reason = match (
        unittest_style.then(|| positional(1)).flatten(),
        keyword("reason"),
    ) {
        (Some(value), None) | (None, Some(value)) => value,
        _ => return false,
    };
    let is_string = matches!(
        reason,
        Expr::Constant(constant) if matches!(constant.value, ast::Constant::Str(_))
    );
    is_string
        && matches!(
            condition,
            Expr::Constant(constant)
                if matches!(constant.value, ast::Constant::Bool(value) if value == inert_value)
        )
}

fn has_starred(args: &[Expr]) -> bool {
    args.iter().any(|arg| matches!(arg, Expr::Starred(_)))
}

fn resolve_control(decorator: &str, imports: &[PythonImport]) -> Option<String> {
    let (root, tail) = decorator.split_once('.').unwrap_or((decorator, ""));
    // Avoid resolving ordinary fixture/parametrization metadata at every query.
    if !tail.is_empty()
        && !matches!(
            tail.rsplit('.').next(),
            Some("skip" | "skipIf" | "skipUnless" | "expectedFailure" | "skipif" | "xfail")
        )
    {
        return None;
    }
    let Some(import) = imports.iter().rev().find(|import| import.alias == root) else {
        // Unresolved canonical spellings retain conservative unestablished
        // activation; an explicit different import must never be bypassed.
        return is_control_name(decorator).then(|| decorator.to_string());
    };
    let mut resolved = if import.source_module.is_empty() {
        import.imported.clone()
    } else {
        format!("{}.{}", import.source_module, import.imported)
    };
    if !tail.is_empty() {
        resolved.push('.');
        resolved.push_str(tail);
    }
    is_control_name(&resolved).then_some(resolved)
}

fn is_control_name(name: &str) -> bool {
    matches!(
        name,
        "unittest.skip"
            | "unittest.skipIf"
            | "unittest.skipUnless"
            | "unittest.expectedFailure"
            | "pytest.mark.skip"
            | "pytest.mark.skipif"
            | "pytest.mark.xfail"
    )
}

/// How a class or module body binds the `__test__` collection switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TestSwitch {
    /// No binding: pytest collects by its usual name rules.
    Unset,
    /// Bound to the literal `True`, which keeps (or restores) collection.
    Enabled,
    /// Bound to `False`, or to anything not provably `True` (a name, a call,
    /// an augmented or deleted binding, a binding under a branch): pytest
    /// may skip the scope, so credit stays withheld.
    Disabled,
}

fn is_test_switch(target: &Expr) -> bool {
    matches!(target, Expr::Name(name) if name.id.as_str() == "__test__")
}

/// Whether `target` binds `__test__` directly or inside a tuple, list or
/// starred destructuring target.
fn target_binds_test_switch(target: &Expr) -> bool {
    match target {
        Expr::Tuple(tuple) => tuple.elts.iter().any(target_binds_test_switch),
        Expr::List(list) => list.elts.iter().any(target_binds_test_switch),
        Expr::Starred(starred) => target_binds_test_switch(starred.value.as_ref()),
        other => is_test_switch(other),
    }
}

fn is_literal_true(value: &Expr) -> bool {
    matches!(
        value,
        Expr::Constant(constant) if matches!(constant.value, ast::Constant::Bool(true))
    )
}

/// The `__test__` binding of a class or module `body`. A top-level binding
/// follows normal rebinding order; any `__test__` binding nested under a
/// compound statement is not evaluated, so it counts as `Disabled`.
pub(super) fn test_switch(body: &[Stmt]) -> TestSwitch {
    let mut switch = TestSwitch::Unset;
    for stmt in body {
        match stmt {
            Stmt::Assign(assign) if assign.targets.iter().any(target_binds_test_switch) => {
                // A destructured binding (`(__test__, x) = (False, 1)`) is not
                // evaluated, so it counts as Disabled even for a `True` value.
                switch = if assign.targets.iter().any(is_test_switch)
                    && is_literal_true(assign.value.as_ref())
                {
                    TestSwitch::Enabled
                } else {
                    TestSwitch::Disabled
                };
            }
            // A bare annotation (`__test__: bool`) creates no attribute.
            Stmt::AnnAssign(assign)
                if is_test_switch(assign.target.as_ref()) && assign.value.is_some() =>
            {
                switch = if assign.value.as_deref().is_some_and(is_literal_true) {
                    TestSwitch::Enabled
                } else {
                    TestSwitch::Disabled
                };
            }
            Stmt::AugAssign(assign) if is_test_switch(assign.target.as_ref()) => {
                switch = TestSwitch::Disabled;
            }
            Stmt::Delete(delete) if delete.targets.iter().any(is_test_switch) => {
                switch = TestSwitch::Disabled;
            }
            Stmt::If(_)
            | Stmt::Try(_)
            | Stmt::TryStar(_)
            | Stmt::With(_)
            | Stmt::For(_)
            | Stmt::While(_)
            | Stmt::Match(_)
                if nested_binds_test_switch(stmt) =>
            {
                switch = TestSwitch::Disabled;
            }
            _ => {}
        }
    }
    switch
}

fn nested_binds_test_switch(stmt: &Stmt) -> bool {
    let mut blocks: Vec<&[Stmt]> = Vec::new();
    match stmt {
        Stmt::If(node) => blocks.extend([node.body.as_slice(), node.orelse.as_slice()]),
        Stmt::Try(node) => {
            blocks.extend([
                node.body.as_slice(),
                node.orelse.as_slice(),
                node.finalbody.as_slice(),
            ]);
            for handler in &node.handlers {
                let ast::ExceptHandler::ExceptHandler(handler) = handler;
                blocks.push(handler.body.as_slice());
            }
        }
        Stmt::TryStar(node) => {
            blocks.extend([
                node.body.as_slice(),
                node.orelse.as_slice(),
                node.finalbody.as_slice(),
            ]);
            for handler in &node.handlers {
                let ast::ExceptHandler::ExceptHandler(handler) = handler;
                blocks.push(handler.body.as_slice());
            }
        }
        Stmt::With(node) => blocks.push(node.body.as_slice()),
        Stmt::For(node) => blocks.extend([node.body.as_slice(), node.orelse.as_slice()]),
        Stmt::While(node) => blocks.extend([node.body.as_slice(), node.orelse.as_slice()]),
        Stmt::Match(node) => blocks.extend(node.cases.iter().map(|case| case.body.as_slice())),
        _ => {}
    }
    blocks
        .into_iter()
        .any(|block| test_switch(block) != TestSwitch::Unset)
}

/// The control pytest's own `__test__ = False` opt-out applies to a class or
/// a module: it is never collected, so no test below it runs.
pub(super) fn test_switch_control(site: DeclarationSite) -> PythonActivationControl {
    collection_control("pytest.collection(__test__ = False)", site)
}

/// A control for a test pytest's collection rules do not select (project
/// configuration, `conftest.py` ignores, `__test__`).
pub(super) fn collection_control(name: &str, site: DeclarationSite) -> PythonActivationControl {
    PythonActivationControl {
        name: name.to_string(),
        site,
    }
}

/// A parametrize decorator over an empty literal table collects the test as
/// skipped, so its body never runs (#9691).
pub(super) fn empty_parametrize_control(decorators: &[Expr]) -> Option<PythonActivationControl> {
    decorators.iter().find_map(|decorator| {
        let Expr::Call(call) = decorator else {
            return None;
        };
        if !expr_full_name(call.func.as_ref()).is_some_and(|name| {
            matches!(
                name.as_str(),
                "pytest.mark.parametrize" | "mark.parametrize"
            )
        }) {
            return None;
        }
        let values = call.args.get(1).or_else(|| {
            call.keywords
                .iter()
                .find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|arg| arg.as_str() == "argvalues")
                })
                .map(|keyword| &keyword.value)
        })?;
        let empty = match values {
            Expr::List(list) => list.elts.is_empty(),
            Expr::Tuple(tuple) => tuple.elts.is_empty(),
            _ => false,
        };
        empty.then(|| PythonActivationControl {
            name: "pytest.mark.parametrize(empty table)".to_string(),
            site: DeclarationSite::Function,
        })
    })
}

/// A parametrize mark pytest refuses at collection (`indirect=None`, an
/// unknown `indirect` name, an argname parametrized twice): the test is
/// reported as a collection error and its body never runs (#10867).
pub(super) fn parametrize_collection_error_control(
    site: DeclarationSite,
) -> PythonActivationControl {
    PythonActivationControl {
        name: "pytest.mark.parametrize(collection error)".to_string(),
        site,
    }
}

/// A later binding of the same name in the same scope replaced this test
/// definition before collection, so pytest and unittest never run its body.
pub(super) fn shadowed_by_later_binding_control(site: DeclarationSite) -> PythonActivationControl {
    PythonActivationControl {
        name: "later binding of the test name".to_string(),
        site,
    }
}

/// Whether a statement after `index` in the same list rebinds `name` without
/// reading the earlier definition (a wrapper such as `name = mark(name)` keeps
/// it running).
pub(super) fn rebound_later(
    statements: &[Stmt],
    index: usize,
    name: &str,
    source: &SourceText<'_>,
) -> bool {
    // Quoted text (`factory('test_p')`) is not a read of the earlier definition.
    let reads_name =
        |range| mentions_identifier(&blank_quoted_strings(&text_for_range(source, range)), name);
    let targets_name = |target: &Expr| binds_name(target, name);
    statements[index + 1..].iter().any(|stmt| match stmt {
        Stmt::FunctionDef(def) => {
            def.name.as_str() == name && !def.decorator_list.iter().any(|d| reads_name(d.range()))
        }
        Stmt::AsyncFunctionDef(def) => {
            def.name.as_str() == name && !def.decorator_list.iter().any(|d| reads_name(d.range()))
        }
        Stmt::ClassDef(def) => {
            def.name.as_str() == name
                && !def.decorator_list.iter().any(|d| reads_name(d.range()))
                && !def.bases.iter().any(|b| reads_name(b.range()))
        }
        Stmt::Assign(assign) => {
            assign.targets.iter().any(targets_name) && !reads_name(assign.value.range())
        }
        Stmt::AnnAssign(assign) => {
            targets_name(assign.target.as_ref())
                && assign
                    .value
                    .as_ref()
                    .is_some_and(|value| !reads_name(value.range()))
        }
        // `del name` unbinds the earlier definition before collection.
        Stmt::Delete(delete) => delete.targets.iter().any(targets_name),
        // A loop or `with` target rebinds the name to a value that is not the
        // earlier definition (the iterable or context manager could read it,
        // so those stay credited).
        // An empty literal iterable never binds the target, so the earlier
        // definition survives.
        Stmt::For(for_stmt) => {
            targets_name(for_stmt.target.as_ref())
                && !reads_name(for_stmt.iter.range())
                && !iterates_nothing(for_stmt.iter.as_ref())
        }
        Stmt::With(with) => with.items.iter().any(|item| {
            item.optional_vars.as_deref().is_some_and(targets_name)
                && !reads_name(item.context_expr.range())
        }),
        // `import name` / `import name.sub` (binds the root) / `import x as
        // name` / `from m import name [as alias]` rebind the name in this scope.
        Stmt::Import(import) => import.names.iter().any(|alias| match &alias.asname {
            Some(asname) => asname.as_str() == name,
            None => alias.name.as_str().split('.').next() == Some(name),
        }),
        Stmt::ImportFrom(import) => import
            .names
            .iter()
            .any(|alias| alias.asname.as_ref().unwrap_or(&alias.name).as_str() == name),
        _ => false,
    })
}

/// Whether `target` binds `name` directly or inside a tuple/list target.
fn binds_name(target: &Expr, name: &str) -> bool {
    match target {
        Expr::Name(bound) => bound.id.as_str() == name,
        Expr::Tuple(tuple) => tuple.elts.iter().any(|elt| binds_name(elt, name)),
        Expr::List(list) => list.elts.iter().any(|elt| binds_name(elt, name)),
        Expr::Starred(starred) => binds_name(starred.value.as_ref(), name),
        _ => false,
    }
}

/// Whether `name` occurs in `text` as a whole identifier, so `test_p_helper`
/// or `xtest_p` does not count as a read of `test_p`.
fn mentions_identifier(text: &str, name: &str) -> bool {
    let is_ident = |c: char| c == '_' || c.is_alphanumeric();
    text.match_indices(name).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + name.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

/// Whether `controls` hold [`parametrize_collection_error_control`].
pub(super) fn has_collection_error(controls: &[PythonActivationControl]) -> bool {
    controls
        .iter()
        .any(|control| control.name == "pytest.mark.parametrize(collection error)")
}

/// Module-level controls skip every test in the file: `pytest.importorskip`,
/// `pytest.skip(..., allow_module_level=True)` and a `pytestmark` that applies
/// a recognized skip or xfail mark. Only unconditional top-level statements
/// count; a guarded call may not run (#9691).
pub(super) fn apply_module_controls(
    statements: &[Stmt],
    imports: &[PythonImport],
    tests: &mut [PythonTest],
) {
    let mut controls: Vec<PythonActivationControl> = statements
        .iter()
        .flat_map(|stmt| module_statement_controls(stmt, imports))
        .collect();
    if test_switch(statements) == TestSwitch::Disabled {
        controls.push(test_switch_control(DeclarationSite::Module));
    }
    if controls.is_empty() {
        return;
    }
    for test in tests {
        test.activation_controls.extend(controls.iter().cloned());
    }
}

/// The module-level control a bare or bound call statement applies.
fn module_call_controls(value: &Expr, imports: &[PythonImport]) -> Vec<PythonActivationControl> {
    let Expr::Call(call) = value else {
        return Vec::new();
    };
    let Some(name) = expr_full_name(call.func.as_ref()) else {
        return Vec::new();
    };
    let (root, tail) = name.split_once('.').unwrap_or((name.as_str(), ""));
    let resolved = match imports.iter().rev().find(|import| import.alias == root) {
        Some(import) if import.source_module.is_empty() => join_tail(&import.imported, tail),
        Some(import) => join_tail(
            &format!("{}.{}", import.source_module, import.imported),
            tail,
        ),
        None => name.clone(),
    };
    let module_skip = match resolved.as_str() {
        "pytest.importorskip" => !imports_always_present_module(call),
        "pytest.skip" => call.keywords.iter().any(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_some_and(|arg| arg.as_str() == "allow_module_level")
        }),
        _ => false,
    };
    if module_skip {
        vec![PythonActivationControl {
            name: resolved,
            site: DeclarationSite::Module,
        }]
    } else {
        Vec::new()
    }
}

fn module_statement_controls(
    stmt: &Stmt,
    imports: &[PythonImport],
) -> Vec<PythonActivationControl> {
    match stmt {
        Stmt::Expr(expr) => module_call_controls(expr.value.as_ref(), imports),
        Stmt::Assign(assign) => {
            let is_pytestmark = assign.targets.iter().any(
                |target| matches!(target, Expr::Name(name) if name.id.as_str() == "pytestmark"),
            );
            if !is_pytestmark {
                // `np = pytest.importorskip("numpy")` skips the module the same way.
                return module_call_controls(assign.value.as_ref(), imports);
            }
            let marks: Vec<&Expr> = match assign.value.as_ref() {
                Expr::List(list) => list.elts.iter().collect(),
                Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                other => vec![other],
            };
            let marks: Vec<Expr> = marks.into_iter().cloned().collect();
            activation_controls(&marks, imports, DeclarationSite::Module)
        }
        _ => Vec::new(),
    }
}

/// Modules every supported interpreter ships, so `importorskip` of one never
/// skips. Any other argument, a `minversion` or a `reason` keeps the control.
const ALWAYS_PRESENT_MODULES: &[&str] = &[
    "abc",
    "collections",
    "contextlib",
    "copy",
    "dataclasses",
    "datetime",
    "decimal",
    "enum",
    "fractions",
    "functools",
    "hashlib",
    "io",
    "itertools",
    "json",
    "logging",
    "math",
    "operator",
    "os",
    "pathlib",
    "random",
    "re",
    "statistics",
    "string",
    "sys",
    "textwrap",
    "time",
    "typing",
    "uuid",
];

fn imports_always_present_module(call: &ast::ExprCall) -> bool {
    call.keywords.is_empty()
        && call.args.len() == 1
        && matches!(
            call.args.first(),
            Some(Expr::Constant(constant))
                if matches!(&constant.value, ast::Constant::Str(name)
                    if ALWAYS_PRESENT_MODULES.contains(&name.as_str()))
        )
}

fn join_tail(base: &str, tail: &str) -> String {
    if tail.is_empty() {
        base.to_string()
    } else {
        format!("{base}.{tail}")
    }
}
