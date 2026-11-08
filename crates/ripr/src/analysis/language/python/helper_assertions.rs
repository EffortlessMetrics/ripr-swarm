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

mod field_admission;

use super::admission::{PythonAssertionAdmission, bound_names};
use super::oracles::collect_assertions_from_statements;
use super::owners_tests::collect_imports_from_statements;
use super::related_tests::is_python_identifier_char;
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_start, text_for_range};
use super::{PythonAssertion, PythonOracleShape, PythonTest};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A module-level def that is nothing but `assert <comparison of params>`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct PythonAssertionHelper {
    name: String,
    params: Vec<String>,
    /// Source text of the asserted expression.
    expr_text: String,
    /// `(start, end, parameter index)` of each parameter use, as byte offsets
    /// into `expr_text`.
    slots: Vec<(usize, usize, usize)>,
    field_observer: bool,
    executable_retained: bool,
    replaced_attributes: Arc<Vec<String>>,
}

/// The assertion helpers of one module and every name the module binds.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct PythonHelperModule {
    pub(super) helpers: Vec<PythonAssertionHelper>,
    /// Module-level defs that call their one callable argument inside a
    /// `try` whose every handler re-raises (RIPR-SPEC-0238 rule 27).
    reraisers: BTreeSet<String>,
    bound: BTreeSet<String>,
    /// How many import statements bind each name anywhere in the module,
    /// nested and conditional ones included (`bound` skips imports).
    #[serde(default)]
    import_bindings: BTreeMap<String, usize>,
    /// Names (and attribute names) a `del` removes anywhere in the module.
    #[serde(default)]
    deleted: BTreeSet<String>,
    /// Attribute names assigned anywhere in the module (`x.attr = ...`).
    #[serde(default)]
    assigned_attributes: BTreeSet<String>,
    /// True only when the module has no star import and nowhere names a
    /// patcher or dynamic namespace writer (`patch`, `setattr`, `globals`,
    /// `sys.modules`, ...). A module-level or autouse-fixture patch of an
    /// imported module's attribute is otherwise invisible to the
    /// module-attribute helper read. Defaults to false so a stale cached
    /// module fails closed.
    #[serde(default)]
    patch_free: bool,
}

/// Counts every import binding and `del` target in `statements`, descending
/// into every nested block, def and class.
fn scan_import_rebinding(
    statements: &[Stmt],
    imports: &mut BTreeMap<String, usize>,
    deleted: &mut BTreeSet<String>,
) {
    fn deleted_targets(target: &Expr, deleted: &mut BTreeSet<String>) {
        match target {
            Expr::Name(name) => {
                deleted.insert(name.id.to_string());
            }
            Expr::Attribute(attribute) => {
                deleted.insert(attribute.attr.to_string());
            }
            Expr::Tuple(tuple) => tuple.elts.iter().for_each(|e| deleted_targets(e, deleted)),
            Expr::List(list) => list.elts.iter().for_each(|e| deleted_targets(e, deleted)),
            _ => {}
        }
    }
    fn handlers(
        handlers: &[ast::ExceptHandler],
        imports: &mut BTreeMap<String, usize>,
        deleted: &mut BTreeSet<String>,
    ) {
        for handler in handlers {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            scan_import_rebinding(&handler.body, imports, deleted);
        }
    }
    for stmt in statements {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let name = match &alias.asname {
                        Some(asname) => asname.to_string(),
                        None => alias.name.split('.').next().unwrap_or_default().to_string(),
                    };
                    *imports.entry(name).or_default() += 1;
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    let name = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                    *imports.entry(name).or_default() += 1;
                }
            }
            Stmt::Delete(delete) => delete
                .targets
                .iter()
                .for_each(|target| deleted_targets(target, deleted)),
            Stmt::FunctionDef(f) => scan_import_rebinding(&f.body, imports, deleted),
            Stmt::AsyncFunctionDef(f) => scan_import_rebinding(&f.body, imports, deleted),
            Stmt::ClassDef(c) => scan_import_rebinding(&c.body, imports, deleted),
            Stmt::For(f) => {
                scan_import_rebinding(&f.body, imports, deleted);
                scan_import_rebinding(&f.orelse, imports, deleted);
            }
            Stmt::AsyncFor(f) => {
                scan_import_rebinding(&f.body, imports, deleted);
                scan_import_rebinding(&f.orelse, imports, deleted);
            }
            Stmt::While(w) => {
                scan_import_rebinding(&w.body, imports, deleted);
                scan_import_rebinding(&w.orelse, imports, deleted);
            }
            Stmt::If(i) => {
                scan_import_rebinding(&i.body, imports, deleted);
                scan_import_rebinding(&i.orelse, imports, deleted);
            }
            Stmt::With(w) => scan_import_rebinding(&w.body, imports, deleted),
            Stmt::AsyncWith(w) => scan_import_rebinding(&w.body, imports, deleted),
            Stmt::Try(t) => {
                scan_import_rebinding(&t.body, imports, deleted);
                handlers(&t.handlers, imports, deleted);
                scan_import_rebinding(&t.orelse, imports, deleted);
                scan_import_rebinding(&t.finalbody, imports, deleted);
            }
            Stmt::TryStar(t) => {
                scan_import_rebinding(&t.body, imports, deleted);
                handlers(&t.handlers, imports, deleted);
                scan_import_rebinding(&t.orelse, imports, deleted);
                scan_import_rebinding(&t.finalbody, imports, deleted);
            }
            Stmt::Match(m) => m
                .cases
                .iter()
                .for_each(|case| scan_import_rebinding(&case.body, imports, deleted)),
            _ => {}
        }
    }
}

pub(super) fn collect_helper_module(
    file: &Path,
    source: &SourceText<'_>,
    statements: &[Stmt],
) -> PythonHelperModule {
    let mut module = PythonHelperModule::default();
    bound_names(statements, &mut module.bound);
    scan_import_rebinding(statements, &mut module.import_bindings, &mut module.deleted);
    module.assigned_attributes = super::module_constants::assigned_attribute_names(statements)
        .into_iter()
        .collect();
    let imports = collect_imports_from_statements(file, statements);
    // A star import may rebind any name, wrapper included.
    let star_import = imports.iter().any(|import| import.imported == "*");
    module.patch_free = !star_import
        && !source.contains("patch")
        && !super::module_constants::writes_namespace_in_text(source);
    let imported: BTreeSet<String> = imports.into_iter().map(|import| import.alias).collect();
    let bindings = super::module_constants::ModuleBindingFacts::of(source, statements);
    let attributes = Arc::new(super::module_constants::assigned_attribute_names(
        statements,
    ));
    let executable_retained = !["__code__", "__globals__", "__setattr__", "__getattribute__"]
        .iter()
        .any(|name| super::mentions_python_name(source, name));
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
        let reraiser = helper.is_none()
            && !star_import
            && is_reraising_wrapper(function)
            && wrapper_only_defined_and_called(source, &name);
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
        if let Some(mut helper) = helper {
            helper.executable_retained &= executable_retained && bindings.bound_once(&name);
            helper.replaced_attributes = Arc::clone(&attributes);
            module.helpers.push(helper);
        } else {
            module.reraisers.insert(name);
        }
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
    let field_observer = field_admission::reads_field(test);
    Some(PythonAssertionHelper {
        name: function.name.to_string(),
        params,
        expr_text: text_for_range(source, test.range()),
        slots,
        field_observer,
        replaced_attributes: Arc::default(),
        executable_retained: !field_observer
            || (function.returns.is_none()
                && function.type_params.is_empty()
                && args.args.iter().all(|arg| arg.def.annotation.is_none())),
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
            handler.type_.as_deref().is_none_or(plain_exception_type)
                && handler_ends_in_bare_raise(&handler.body)
        })
}

/// An exception class expression that cannot run code: a name, a dotted
/// attribute path or a tuple of those. The type expression is evaluated when
/// an exception reaches the handler, so `except pytest.skip('x'):` would skip.
fn plain_exception_type(expr: &Expr) -> bool {
    match expr {
        Expr::Name(_) => true,
        Expr::Attribute(attribute) => plain_exception_type(&attribute.value),
        Expr::Tuple(tuple) => tuple.elts.iter().all(plain_exception_type),
        _ => false,
    }
}

/// Every occurrence of `name` in the module is its one `def` or a plain call
/// `name(`. Any other mention (an assignment, `global`, an import at any
/// depth, a walrus, a `case` capture, a decorator, an attribute, a string for
/// `setattr`) could rebind it, and the binding scan does not see inside
/// function bodies, expressions or nested imports. Dynamic namespace access
/// (`globals()`, `vars()`, `exec`, `eval`, a star import) refuses the module.
fn wrapper_only_defined_and_called(text: &str, name: &str) -> bool {
    if [
        "import *", "globals(", "vars(", "exec(", "eval(", "__dict__",
    ]
    .iter()
    .any(|dynamic| text.contains(dynamic))
    {
        return false;
    }
    let mut defs = 0usize;
    let mut from = 0usize;
    while let Some(found) = text[from..].find(name) {
        let start = from + found;
        let end = start + name.len();
        from = end;
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if before.is_some_and(is_python_identifier_char)
            || after.is_some_and(is_python_identifier_char)
        {
            continue;
        }
        let head = text[..start].trim_end_matches([' ', '\t']);
        if head.ends_with("def") && head.len() < text[..start].len() && after == Some('(') {
            defs += 1;
        } else if before == Some('.') || after != Some('(') {
            return false;
        }
    }
    defs == 1
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
        // `helper(...)` or `module.helper(...)` where `module` is an imported
        // workspace module (`from . import conftest`).
        let (name, module_attr) = match call.func.as_ref() {
            Expr::Name(callee) => (callee.id.as_str(), None),
            Expr::Attribute(attribute) => match attribute.value.as_ref() {
                Expr::Name(base) => (base.id.as_str(), Some(attribute.attr.as_str())),
                _ => continue,
            },
            _ => continue,
        };
        if local.contains(name)
            || !call.keywords.is_empty()
            || call.args.iter().any(|arg| matches!(arg, Expr::Starred(_)))
        {
            continue;
        }
        let helper = match module_attr {
            None => resolve_helper(test, name, modules),
            Some(attr) => resolve_module_helper(test, name, attr, modules),
        };
        let Some(helper) = helper else {
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
        assertion.helper_origin = if !helper.field_observer || helper.executable_retained {
            field_admission::origin(
                test,
                function,
                &assertion.gate_text,
                helper.field_observer,
                &helper.replaced_attributes,
                &source,
            )
        } else {
            super::PythonHelperOrigin::FieldHelperRefused
        };
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
            if import.source_module.is_empty() {
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

/// The helper `module_name.helper_name` names when `module_name` is bound by
/// exactly one import of a workspace module: `from . import conftest` /
/// `from pkg import helpers` (a submodule file, with no same-named binding in
/// the package `__init__`) or `import pkg.helpers as helpers`. The test must
/// not rebind the module name or assign the attribute (#13629).
fn resolve_module_helper<'a>(
    test: &PythonTest,
    module_name: &str,
    helper_name: &str,
    modules: &'a BTreeMap<PathBuf, PythonHelperModule>,
) -> Option<&'a PythonAssertionHelper> {
    let mut matching = test
        .imports
        .iter()
        .filter(|import| import.alias == module_name);
    let import = matching.next()?;
    let own = modules.get(&test.file)?;
    if matching.next().is_some()
        // Any other import of the alias (nested, conditional, in the test
        // body), or a `del` of it, makes the bound module order-dependent.
        || own.import_bindings.get(module_name).copied() != Some(1)
        || own.deleted.contains(module_name)
        || own.deleted.contains(helper_name)
        || own.bound.contains(module_name)
        // The helper attribute replaced anywhere in the test file.
        || own.assigned_attributes.contains(helper_name)
        // A star import, or a patcher anywhere in the test file (module
        // level, an autouse fixture, a decorator), may replace the attribute.
        || !own.patch_free
        || test.body_text.contains("setattr")
        || test.body_text.contains("patch")
        || assigns_module_attribute(&test.body_text, module_name, helper_name)
    {
        return None;
    }
    let dotted = if import.source_module.is_empty() {
        // `import pkg.helpers as helpers`. An undotted module (`import
        // helpers`, `import helpers as h`) resolves through `sys.path`, and
        // an empty source from `from ... import x` is a relative import that
        // escaped the package; neither names a workspace module.
        if !import.imported.contains('.') {
            return None;
        }
        import.imported.clone()
    } else {
        // `from pkg import helpers` names a submodule only when the package
        // `__init__` does not bind the same name itself.
        let package = format!("{}/__init__.py", import.source_module.replace('.', "/"));
        if modules
            .get(Path::new(&package))
            .is_some_and(|init| init.bound.contains(&import.imported))
        {
            return None;
        }
        format!("{}.{}", import.source_module, import.imported)
    };
    module_for(&dotted, modules)?
        .helpers
        .iter()
        .find(|helper| helper.name == helper_name)
}

/// Whether the text assigns `module.attr` (`module.attr = ...`, not `==`).
fn assigns_module_attribute(text: &str, module: &str, attr: &str) -> bool {
    let target = format!("{module}.{attr}");
    text.match_indices(&target).any(|(index, _)| {
        let rest = text[index + target.len()..].trim_start();
        rest.starts_with('=') && !rest.starts_with("==")
    })
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
