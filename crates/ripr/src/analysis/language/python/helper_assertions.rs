//! Bounded helper-assertion resolution (#9419).
//!
//! A test that ends in `check_equal(f(3), 9)` asserts through a helper whose
//! body is a single `assert actual == expected`. When the helper resolves to a
//! module-level def whose only statement is an `assert` of a comparison over
//! its own parameters, the call is read as that assertion with the arguments
//! substituted for the parameters, at the strength the inline form earns.
//!
//! Everything else stays opaque: a helper with defaults, `*args`/`**kwargs`,
//! decorators, extra statements, a free name, a call, a rebound name or an
//! argument-count mismatch is not resolved, and the call keeps whatever
//! (unknown) reading the oracle scan gave it. Only bare-name calls made as a
//! top-level statement of the test body resolve; the helper comes from the
//! test's own module or a `from M import name` of a workspace module.

use super::admission::{PythonAssertionAdmission, bound_names};
use super::oracles::collect_assertions_from_statements;
use super::owners_tests::collect_imports_from_statements;
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_start, text_for_range};
use super::{PythonAssertion, PythonOracleShape, PythonTest};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A module-level def that is nothing but `assert <comparison of params>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonAssertionHelper {
    name: String,
    params: Vec<String>,
    /// Source text of the asserted expression.
    expr_text: String,
    /// `(start, end, parameter index)` of each parameter use, as byte offsets
    /// into `expr_text`.
    slots: Vec<(usize, usize, usize)>,
}

/// The assertion helpers of one module and every name the module binds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PythonHelperModule {
    pub(super) helpers: Vec<PythonAssertionHelper>,
    /// Module-level defs that call their one callable argument inside a
    /// `try` whose every handler re-raises (RIPR-SPEC-0238 rule 27).
    reraisers: BTreeSet<String>,
    bound: BTreeSet<String>,
}

pub(super) fn collect_helper_module(
    file: &Path,
    source: &SourceText<'_>,
    statements: &[Stmt],
) -> PythonHelperModule {
    let mut module = PythonHelperModule::default();
    bound_names(statements, &mut module.bound);
    let imports = collect_imports_from_statements(file, statements);
    // A star import may rebind any name, wrapper included.
    let star_import = imports.iter().any(|import| import.imported == "*");
    let imported: BTreeSet<String> = imports.into_iter().map(|import| import.alias).collect();
    for (index, stmt) in statements.iter().enumerate() {
        let Stmt::FunctionDef(function) = stmt else {
            continue;
        };
        let name = function.name.to_string();
        if imported.contains(&name) {
            continue;
        }
        // Unsupported shapes are skipped before the binding scan.
        let helper = helper_of(source, function);
        let reraiser = helper.is_none() && !star_import && is_reraising_wrapper(function);
        if helper.is_none() && !reraiser {
            continue;
        }
        // The def must be the module's only binding of the name.
        let mut others = BTreeSet::new();
        bound_names(&statements[..index], &mut others);
        bound_names(&statements[index + 1..], &mut others);
        if others.contains(&name) {
            continue;
        }
        if reraiser {
            module.reraisers.insert(name);
        }
        module.helpers.extend(helper);
    }
    module
}

fn helper_of(
    source: &SourceText<'_>,
    function: &ast::StmtFunctionDef,
) -> Option<PythonAssertionHelper> {
    let args = function.args.as_ref();
    if !function.decorator_list.is_empty()
        || !args.posonlyargs.is_empty()
        || !args.kwonlyargs.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || args.args.is_empty()
        || args.args.iter().any(|arg| arg.default.is_some())
    {
        return None;
    }
    let params: Vec<String> = args
        .args
        .iter()
        .map(|arg| arg.def.arg.to_string())
        .collect();
    let mut body = function.body.as_slice();
    // A leading docstring does not change what the helper does.
    if let [Stmt::Expr(first), rest @ ..] = body
        && matches!(
            first.value.as_ref(),
            Expr::Constant(ast::ExprConstant {
                value: ast::Constant::Str(_),
                ..
            })
        )
    {
        body = rest;
    }
    let [Stmt::Assert(assert_stmt)] = body else {
        return None;
    };
    let test = assert_stmt.test.as_ref();
    if !matches!(test, Expr::Compare(_)) {
        return None;
    }
    let base = usize::from(test.range().start());
    let mut slots = Vec::new();
    if !only_params(test, &params, base, &mut slots) {
        return None;
    }
    Some(PythonAssertionHelper {
        name: function.name.to_string(),
        params,
        expr_text: text_for_range(source, test.range()),
        slots,
    })
}

/// A def of one plain parameter `fn` whose whole body is
/// `try: return fn()` (or `fn()`) with every handler a bare `raise`,
/// so a failure inside the callable still reaches the runner. Any `else`,
/// `finally`, other statement, parameter shape or handler that can return
/// instead of raising leaves the def unread (RIPR-SPEC-0238 rule 27).
fn is_reraising_wrapper(function: &ast::StmtFunctionDef) -> bool {
    let args = function.args.as_ref();
    if !function.decorator_list.is_empty()
        || !args.posonlyargs.is_empty()
        || !args.kwonlyargs.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || args.args.len() != 1
        || args.args[0].default.is_some()
    {
        return false;
    }
    let param = args.args[0].def.arg.as_str();
    let mut body = function.body.as_slice();
    if let [Stmt::Expr(first), rest @ ..] = body
        && matches!(
            first.value.as_ref(),
            Expr::Constant(ast::ExprConstant {
                value: ast::Constant::Str(_),
                ..
            })
        )
    {
        body = rest;
    }
    let [Stmt::Try(try_stmt)] = body else {
        return false;
    };
    if !try_stmt.orelse.is_empty() || !try_stmt.finalbody.is_empty() || try_stmt.handlers.is_empty()
    {
        return false;
    }
    let called = match try_stmt.body.as_slice() {
        [Stmt::Return(ret)] => ret.value.as_deref(),
        [Stmt::Expr(expr)] => Some(expr.value.as_ref()),
        _ => None,
    };
    let Some(Expr::Call(call)) = called else {
        return false;
    };
    let calls_param = matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == param)
        && call.args.is_empty()
        && call.keywords.is_empty();
    calls_param
        && try_stmt.handlers.iter().all(|handler| {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            handler_ends_in_bare_raise(&handler.body)
        })
}

/// The handler is a bare `raise` and nothing else. A statement before it
/// could end the test another way (`pytest.skip()`, `sys.exit(0)`), so no
/// other statement is allowed.
fn handler_ends_in_bare_raise(body: &[Stmt]) -> bool {
    matches!(body, [Stmt::Raise(raise)] if raise.exc.is_none() && raise.cause.is_none())
}

/// The expression reads only parameters, literals and plain operators.
fn only_params(
    expr: &Expr,
    params: &[String],
    base: usize,
    slots: &mut Vec<(usize, usize, usize)>,
) -> bool {
    match expr {
        Expr::Name(name) => {
            let Some(index) = params.iter().position(|param| param == name.id.as_str()) else {
                return false;
            };
            slots.push((
                usize::from(name.range.start()) - base,
                usize::from(name.range.end()) - base,
                index,
            ));
            true
        }
        Expr::Constant(_) => true,
        Expr::Attribute(attribute) => only_params(&attribute.value, params, base, slots),
        Expr::Subscript(subscript) => {
            only_params(&subscript.value, params, base, slots)
                && only_params(&subscript.slice, params, base, slots)
        }
        Expr::UnaryOp(unary) => only_params(&unary.operand, params, base, slots),
        Expr::BinOp(binary) => {
            only_params(&binary.left, params, base, slots)
                && only_params(&binary.right, params, base, slots)
        }
        Expr::Compare(compare) => {
            only_params(&compare.left, params, base, slots)
                && compare
                    .comparators
                    .iter()
                    .all(|comparator| only_params(comparator, params, base, slots))
        }
        Expr::Tuple(tuple) => tuple
            .elts
            .iter()
            .all(|elt| only_params(elt, params, base, slots)),
        Expr::List(list) => list
            .elts
            .iter()
            .all(|elt| only_params(elt, params, base, slots)),
        _ => false,
    }
}

/// Read each resolvable helper call of every test as the helper's assertion.
pub(super) fn apply_helper_assertions(
    tests: &mut [PythonTest],
    modules: &BTreeMap<PathBuf, PythonHelperModule>,
) {
    let names: BTreeSet<&str> = modules
        .values()
        .flat_map(|module| module.helpers.iter().map(|helper| helper.name.as_str()))
        .collect();
    if names.is_empty() {
        return;
    }
    for test in tests {
        if !names.iter().any(|name| test.body_text.contains(name)) {
            continue;
        }
        resolve_test(test, modules);
    }
}

fn resolve_test(test: &mut PythonTest, modules: &BTreeMap<PathBuf, PythonHelperModule>) {
    let source_text = test.body_text.clone();
    let Ok(Mod::Module(parsed)) = parse_module_result(&test.file, &source_text) else {
        return;
    };
    let [function] = parsed.body.as_slice() else {
        return;
    };
    let (args, body) = match function {
        Stmt::FunctionDef(function) => (function.args.as_ref(), function.body.as_slice()),
        Stmt::AsyncFunctionDef(function) => (function.args.as_ref(), function.body.as_slice()),
        _ => return,
    };
    let mut local = BTreeSet::new();
    bound_names(body, &mut local);
    local.extend(
        args.posonlyargs
            .iter()
            .chain(args.args.iter())
            .chain(args.kwonlyargs.iter())
            .map(|arg| arg.def.arg.to_string()),
    );
    local.extend(args.vararg.iter().map(|arg| arg.arg.to_string()));
    local.extend(args.kwarg.iter().map(|arg| arg.arg.to_string()));
    let source = SourceText::new(&source_text);
    for stmt in body {
        let Stmt::Expr(expr_stmt) = stmt else {
            continue;
        };
        let Expr::Call(call) = expr_stmt.value.as_ref() else {
            continue;
        };
        let call = unwrapped_call(test, call, &local, modules).unwrap_or(call);
        let Expr::Name(callee) = call.func.as_ref() else {
            continue;
        };
        let name = callee.id.as_str();
        if local.contains(name)
            || !call.keywords.is_empty()
            || call.args.iter().any(|arg| matches!(arg, Expr::Starred(_)))
        {
            continue;
        }
        let Some(helper) = resolve_helper(test, name, modules) else {
            continue;
        };
        if helper.params.len() != call.args.len() {
            continue;
        }
        let call_line = test.line + line_for_range_start(&source, call.range) - 1;
        let Some(mut assertion) = substituted_assertion(helper, &call.args, &source) else {
            continue;
        };
        assertion.text = text_for_range(&source, call.range).trim().to_string();
        assertion.line = call_line;
        assertion.source_range = Some(call.range);
        assertion.observed_operands.clear();
        assertion.helper_derived = true;
        let recognized = assertion.oracle_shape != PythonOracleShape::UnknownCustomHelper;
        // An `assert_*` helper name already reads as an unknown custom helper
        // at this line; the resolved body replaces that reading.
        let existing = test
            .assertions
            .iter()
            .position(|existing| existing.source_range == Some(call.range));
        match existing {
            Some(position)
                if test.assertions[position].oracle_shape
                    == PythonOracleShape::UnknownCustomHelper =>
            {
                test.assertions[position] = assertion;
            }
            Some(_) => continue,
            None => test.assertions.push(assertion),
        }
        if recognized {
            test.assertion_admission = PythonAssertionAdmission::Recognized;
        }
    }
}

/// `wrapper(lambda: inner(...))` where `wrapper` is a re-raising wrapper
/// defined in the test's own module: the inner call runs and its failure
/// reaches the runner, so it is read as the statement itself. Anything else
/// (an imported or rebound name, a lambda with parameters, a second argument)
/// returns `None` and the call keeps its reading.
fn unwrapped_call<'a>(
    test: &PythonTest,
    call: &'a ast::ExprCall,
    local: &BTreeSet<String>,
    modules: &BTreeMap<PathBuf, PythonHelperModule>,
) -> Option<&'a ast::ExprCall> {
    let Expr::Name(callee) = call.func.as_ref() else {
        return None;
    };
    let name = callee.id.as_str();
    if local.contains(name)
        || !call.keywords.is_empty()
        || test.imports.iter().any(|import| import.alias == name)
        || !modules.get(&test.file)?.reraisers.contains(name)
    {
        return None;
    }
    let [Expr::Lambda(lambda)] = call.args.as_slice() else {
        return None;
    };
    let params = lambda.args.as_ref();
    if !params.posonlyargs.is_empty()
        || !params.args.is_empty()
        || !params.kwonlyargs.is_empty()
        || params.vararg.is_some()
        || params.kwarg.is_some()
    {
        return None;
    }
    match lambda.body.as_ref() {
        Expr::Call(inner) => Some(inner),
        _ => None,
    }
}

fn resolve_helper<'a>(
    test: &PythonTest,
    name: &str,
    modules: &'a BTreeMap<PathBuf, PythonHelperModule>,
) -> Option<&'a PythonAssertionHelper> {
    let own = modules.get(&test.file);
    // Two imports of one alias leave the bound target order-dependent.
    let mut matching = test.imports.iter().filter(|import| import.alias == name);
    let import = matching.next();
    if matching.next().is_some() {
        return None;
    }
    let (module, helper_name) = match import {
        Some(import) => {
            // A name the test module also defines or rebinds is ambiguous.
            if own.is_some_and(|module| module.bound.contains(name)) {
                return None;
            }
            if import.source_module.is_empty() || import.imported == "*" {
                return None;
            }
            (
                module_for(&import.source_module, modules)?,
                import.imported.as_str(),
            )
        }
        None => (own?, name),
    };
    module
        .helpers
        .iter()
        .find(|helper| helper.name == helper_name)
}

/// The workspace module a dotted `from` source names, as a plain module file
/// or a package `__init__`; both existing is ambiguous.
fn module_for<'a>(
    dotted: &str,
    modules: &'a BTreeMap<PathBuf, PythonHelperModule>,
) -> Option<&'a PythonHelperModule> {
    let path = dotted.replace('.', "/");
    let file = modules.get(Path::new(&format!("{path}.py")));
    let package = modules.get(Path::new(&format!("{path}/__init__.py")));
    match (file, package) {
        (Some(module), None) | (None, Some(module)) => Some(module),
        _ => None,
    }
}

fn substituted_assertion(
    helper: &PythonAssertionHelper,
    arguments: &[Expr],
    call_source: &SourceText<'_>,
) -> Option<PythonAssertion> {
    let mut text = helper.expr_text.clone();
    for &(start, end, index) in helper.slots.iter().rev() {
        let argument = arguments.get(index)?;
        let spelled = text_for_range(call_source, argument.range());
        let spelled = spelled.trim();
        let atom = matches!(
            argument,
            Expr::Name(_)
                | Expr::Constant(_)
                | Expr::Call(_)
                | Expr::Attribute(_)
                | Expr::Subscript(_)
        );
        let replacement = if atom {
            spelled.to_string()
        } else {
            format!("({spelled})")
        };
        text.replace_range(start..end, &replacement);
    }
    let snippet = format!("assert {text}\n");
    let Ok(Mod::Module(parsed)) = parse_module_result(Path::new("<helper>"), &snippet) else {
        return None;
    };
    let snippet_source = SourceText::new(&snippet);
    let mut assertions = collect_assertions_from_statements(&parsed.body, &snippet_source, false);
    if assertions.len() != 1 {
        return None;
    }
    assertions.pop()
}
