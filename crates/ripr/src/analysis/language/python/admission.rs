//! Whether RIPR can honestly say a Python test has no assertion (#5571).
//!
//! An empty `PythonTest::assertions` vector only means the oracle extractor
//! recognized nothing. That is a weaker fact than "this test asserts nothing":
//! a custom or imported helper, a same-module wrapper, a fixture, a setup
//! hook, an `assert` inside a nested function, or a dynamic call can all
//! check behavior the extractor does not model. A later related-test miss
//! (`no_assertion`, #5491) may only be claimed from
//! [`PythonAssertionAdmission::NoAssertionLike`], so this scan is
//! fail-closed. Every callee must resolve to something known not to assert:
//! a built-in, a name imported from a non-test module, a method on a value
//! the test itself bound, or a pytest built-in fixture. Anything else (an
//! unknown global, a same-module function or class, a `self.` method, a
//! test-support import, a dynamic callee) makes the state
//! [`PythonAssertionAdmission::Unresolved`].
//!
//! A file the parser refuses yields no `PythonTest` at all, so an extracted
//! test is never "partially extracted": the parse-limited state is carried by
//! the absence of the row and the file's `unsupported_syntax` limitation, not
//! by a value here.
//!
//! Known blind spots: the scan sees one file. An autouse fixture in a
//! `conftest.py`, an assertion installed by a pytest plugin, or a helper
//! imported by bare name from a sibling module that pytest's rootdir
//! insertion makes importable (`from utils import run_case` beside the test)
//! can still fail a `NoAssertionLike` test (#6657). A module imported from
//! the test file's own directory or below is treated as test support; a
//! test file at the repository root has no such directory, so its relative
//! imports (`from . import checks`) are not caught (#6657). A global filled
//! through a plain-name registering decorator (`@register` whose body
//! appends to `HANDLERS`) is not tainted either, and a `conftest.py` may
//! override a built-in fixture such as `tmp_path` (#6657). A
//! `no_assertion` miss must account for the rest before it is emitted.
//!
//! Test activation (skip / xfail / expected failure, #5389) is a separate
//! fact and is not folded into this state.

use super::expr_full_name;
use super::{PythonAssertion, PythonImport, PythonOracleShape};
use rustpython_parser::ast::{self, Expr, Stmt};
use std::collections::BTreeSet;

/// Assertion admission for one extracted Python test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PythonAssertionAdmission {
    /// The extractor recognized at least one assertion or observer it can
    /// grade (anything other than an unknown custom helper).
    Recognized,
    /// The whole test was walked and nothing assertion-like exists: every
    /// callee resolves to something known not to assert, and no fixture,
    /// decorator, base class or setup hook could assert around it.
    NoAssertionLike,
    /// Something assertion-like is present that RIPR cannot admit or resolve.
    Unresolved,
}

impl PythonAssertionAdmission {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Recognized => "recognized_assertion_present",
            Self::NoAssertionLike => "extraction_complete_no_assertion",
            Self::Unresolved => "assertion_like_present_but_unresolved",
        }
    }
}

/// Module-wide facts the per-test scan needs.
#[derive(Clone, Debug, Default)]
pub(super) struct PythonAdmissionContext {
    /// Every function, method and class name defined in the file, tests
    /// included. A test that calls or references one may assert through it.
    defined_names: BTreeSet<String>,
    /// Classes whose bases or keywords RIPR cannot see into (an imported
    /// base, a metaclass): inherited setup hooks may assert.
    opaque_classes: BTreeSet<String>,
    /// An autouse fixture, or a `pytestmark` that applies `usefixtures` or
    /// `filterwarnings` (which can turn a warning into a failure).
    has_implicit_fixture: bool,
    /// A setup or teardown hook (`setUp`, `teardown_method`, `setup_module`,
    /// ...) in this file whose body is assertion-like. The runner calls it
    /// around tests without the test naming it.
    has_assertion_like_lifecycle: bool,
    /// The dotted directory of the file (see `TestScope::module_dir`).
    module_dir: String,
    /// Class-base spellings known to be unittest's own test classes.
    safe_bases: BTreeSet<String>,
}

impl PythonAdmissionContext {
    pub(super) fn of_module(
        statements: &[Stmt],
        imports: &[PythonImport],
        module_dir: &str,
    ) -> Self {
        let mut context = Self {
            module_dir: module_dir.to_string(),
            ..Self::default()
        };
        context.safe_bases = safe_base_spellings(imports);
        let mut lifecycle_bodies = Vec::new();
        context.collect(statements, &mut lifecycle_bodies);
        context.taint_module_bindings(statements, imports);
        // Defined names are complete only after the whole module is seen.
        context.has_assertion_like_lifecycle = lifecycle_bodies.iter().any(|body| {
            let scope = TestScope::of_body(module_dir, &[], body, None);
            let mut scan = BodyScan {
                context: &context,
                imports,
                scope: &scope,
                assertion_like: false,
            };
            scan.statements(body);
            scan.assertion_like
        });
        context
    }

    /// A module global bound from a same-module helper or a test-support
    /// import (`HANDLERS = [verify_one]`, `CASES = [Case(1)]`) carries that
    /// callable into any test that reads it, so it is treated like a defined
    /// name. Repeated until no new binding is tainted, for chains of globals.
    fn taint_module_bindings(&mut self, statements: &[Stmt], imports: &[PythonImport]) {
        let mut bindings = Vec::new();
        let mut registries = Vec::new();
        module_bindings(statements, &mut bindings, &mut registries);
        // `@HANDLERS.append` or `HANDLERS.append(verify)` registers a
        // same-module callable in a global without assigning it.
        self.defined_names.extend(
            registries
                .into_iter()
                .filter(|root| !imports.iter().any(|import| &import.alias == root)),
        );
        let scope = TestScope::of_body(&self.module_dir, &[], &[], None);
        loop {
            let tainted: Vec<String> = bindings
                .iter()
                .filter(|(targets, value)| {
                    targets
                        .iter()
                        .any(|target| !self.defined_names.contains(target))
                        && {
                            let mut scan = BodyScan {
                                context: self,
                                imports,
                                scope: &scope,
                                assertion_like: false,
                            };
                            scan.expr(value);
                            scan.assertion_like
                        }
                })
                .flat_map(|(targets, _)| targets.iter().cloned())
                .collect();
            let before = self.defined_names.len();
            self.defined_names.extend(tainted);
            if self.defined_names.len() == before {
                break;
            }
        }
    }

    fn collect<'s>(&mut self, statements: &'s [Stmt], lifecycle: &mut Vec<&'s [Stmt]>) {
        for stmt in statements {
            match stmt {
                Stmt::FunctionDef(function) => {
                    self.note_function(function.name.as_str(), &function.decorator_list);
                    if LIFECYCLE_HOOKS.contains(&function.name.as_str()) {
                        lifecycle.push(&function.body);
                    }
                }
                Stmt::AsyncFunctionDef(function) => {
                    self.note_function(function.name.as_str(), &function.decorator_list);
                    if LIFECYCLE_HOOKS.contains(&function.name.as_str()) {
                        lifecycle.push(&function.body);
                    }
                }
                Stmt::ClassDef(class) => {
                    self.defined_names.insert(class.name.to_string());
                    let opaque_base = class.bases.iter().any(|base| {
                        !expr_full_name(base).is_some_and(|name| self.safe_bases.contains(&name))
                    });
                    // A class decorator can rewrite or wrap every test
                    // method, so its tests are never extraction-complete.
                    let opaque_decorator = !class.decorator_list.is_empty();
                    if opaque_base || opaque_decorator || !class.keywords.is_empty() {
                        self.opaque_classes.insert(class.name.to_string());
                    }
                    // Class attributes (`checker = ApiChecker()`) run at
                    // class creation and are reached through `self.`, so
                    // they are scanned like setup hooks.
                    lifecycle.extend(
                        class
                            .body
                            .iter()
                            .filter(|stmt| {
                                !matches!(
                                    stmt,
                                    Stmt::FunctionDef(_)
                                        | Stmt::AsyncFunctionDef(_)
                                        | Stmt::ClassDef(_)
                                )
                            })
                            .map(std::slice::from_ref),
                    );
                    self.collect(&class.body, lifecycle);
                }
                Stmt::Assign(assign)
                    if assign.targets.iter().any(|target| {
                        matches!(target, Expr::Name(name) if name.id.as_str() == "pytestmark")
                    }) && applies_implicit_mark(&assign.value) =>
                {
                    self.has_implicit_fixture = true;
                }
                // Definitions under a module-level `if`, `try` or `with`.
                Stmt::If(if_stmt) => {
                    self.collect(&if_stmt.body, lifecycle);
                    self.collect(&if_stmt.orelse, lifecycle);
                }
                Stmt::Try(try_stmt) => self.collect_try(
                    &try_stmt.body,
                    &try_stmt.handlers,
                    &try_stmt.orelse,
                    &try_stmt.finalbody,
                    lifecycle,
                ),
                Stmt::TryStar(try_stmt) => self.collect_try(
                    &try_stmt.body,
                    &try_stmt.handlers,
                    &try_stmt.orelse,
                    &try_stmt.finalbody,
                    lifecycle,
                ),
                Stmt::With(with_stmt) => self.collect(&with_stmt.body, lifecycle),
                _ => {}
            }
        }
    }

    fn collect_try<'s>(
        &mut self,
        body: &'s [Stmt],
        handlers: &'s [ast::ExceptHandler],
        orelse: &'s [Stmt],
        finalbody: &'s [Stmt],
        lifecycle: &mut Vec<&'s [Stmt]>,
    ) {
        self.collect(body, lifecycle);
        for handler in handlers {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            self.collect(&handler.body, lifecycle);
        }
        self.collect(orelse, lifecycle);
        self.collect(finalbody, lifecycle);
    }

    fn note_function(&mut self, name: &str, decorators: &[Expr]) {
        if decorators.iter().any(is_autouse_fixture) {
            self.has_implicit_fixture = true;
        }
        self.defined_names.insert(name.to_string());
    }
}

/// unittest classes a test class may inherit without inheriting hooks RIPR
/// cannot see. Only a spelling the module's imports resolve to these counts:
/// a project's own `TestCase` can carry an asserting `setUp`.
const SAFE_UNITTEST_BASES: &[&str] = &["TestCase", "IsolatedAsyncioTestCase"];

/// The base spellings that resolve, through this module's imports, to a
/// `SAFE_UNITTEST_BASES` class, plus `object`.
fn safe_base_spellings(imports: &[PythonImport]) -> BTreeSet<String> {
    let mut spellings = BTreeSet::from(["object".to_string()]);
    for import in imports {
        if import.source_module.is_empty() && import.imported == "unittest" {
            for base in SAFE_UNITTEST_BASES {
                spellings.insert(format!("{}.{base}", import.alias));
            }
        } else if import.source_module == "unittest"
            && SAFE_UNITTEST_BASES.contains(&import.imported.as_str())
        {
            spellings.insert(import.alias.clone());
        }
    }
    spellings
}

/// unittest, pytest xunit-style and nose setup/teardown hooks the runner
/// calls around a test without the test naming them.
const LIFECYCLE_HOOKS: &[&str] = &[
    "setUp",
    "tearDown",
    "setUpClass",
    "tearDownClass",
    "setUpModule",
    "tearDownModule",
    "asyncSetUp",
    "asyncTearDown",
    "setup",
    "teardown",
    "setup_method",
    "teardown_method",
    "setup_class",
    "teardown_class",
    "setup_function",
    "teardown_function",
    "setup_module",
    "teardown_module",
];

/// Module path segments that mark test-support code. A name imported from
/// such a module may be an assertion helper with any name.
fn is_test_support_path(path: &str) -> bool {
    path.split('.').any(|segment| {
        segment.starts_with("test")
            || segment.ends_with("_test")
            || segment.ends_with("_tests")
            || matches!(
                segment,
                "conftest" | "helpers" | "helper" | "fixtures" | "support" | "testing"
            )
    })
}

/// Qualified callees that end or fail a test without an `assert`.
/// `warnings.simplefilter("error")` turns any later warning into a failure;
/// the filter action is not read, so every filter call counts.
const FAILING_CALLEES: &[&str] = &[
    "_thread.interrupt_main",
    "os._exit",
    "os.abort",
    "os.kill",
    "os.killpg",
    "signal.raise_signal",
    "sys.exit",
    "warnings.filterwarnings",
    "warnings.simplefilter",
];

/// The `pytest.` members that only build values or marks. Every other
/// `pytest.` call (`warns`, `deprecated_call`, `fail`, `exit`, ...) can fail
/// the test.
fn is_inert_pytest_member(qualified: &str) -> bool {
    matches!(qualified, "pytest.approx" | "pytest.param")
        || (qualified.starts_with("pytest.mark.")
            && !qualified.contains("usefixtures")
            && !qualified.contains("filterwarnings"))
}

/// Built-ins that compute values and cannot fail a test by themselves.
/// `exit`, `quit`, `eval`, `exec`, `compile`, `breakpoint` and `__import__`
/// are deliberately absent.
const INERT_BUILTINS: &[&str] = &[
    "abs",
    "all",
    "any",
    "ascii",
    "bin",
    "bool",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "complex",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "filter",
    "float",
    "format",
    "frozenset",
    "hasattr",
    "hash",
    "hex",
    "id",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "list",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "zip",
];

/// pytest's built-in fixtures. They supply inputs and capture state; they do
/// not assert on their own, so requesting one does not make admission
/// unresolved. Every other requested fixture might.
const PYTEST_BUILTIN_FIXTURES: &[&str] = &[
    "cache",
    "capfd",
    "capfdbinary",
    "caplog",
    "capsys",
    "capsysbinary",
    "doctest_namespace",
    "monkeypatch",
    "pytestconfig",
    "pytester",
    "record_property",
    "record_testsuite_property",
    "record_xml_attribute",
    "recwarn",
    "request",
    "testdir",
    "tmp_path",
    "tmp_path_factory",
    "tmpdir",
    "tmpdir_factory",
];

/// Module-level assignments, including those under a module-level `if`,
/// `try` or `with`, as (bound names, value).
fn module_bindings<'s>(
    statements: &'s [Stmt],
    out: &mut Vec<(Vec<String>, &'s Expr)>,
    registries: &mut Vec<String>,
) {
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => registry_roots(&function.decorator_list, registries),
            Stmt::AsyncFunctionDef(function) => {
                registry_roots(&function.decorator_list, registries)
            }
            Stmt::ClassDef(class) => registry_roots(&class.decorator_list, registries),
            // A module-level method call on a global (`HANDLERS.append(f)`).
            Stmt::Expr(expr) => {
                if let Expr::Call(call) = expr.value.as_ref()
                    && let Expr::Attribute(attribute) = call.func.as_ref()
                {
                    let mut names = Vec::new();
                    binding_roots(&attribute.value, &mut names);
                    out.push((names, expr.value.as_ref()));
                }
            }
            Stmt::Assign(assign) => {
                let mut names = Vec::new();
                for target in &assign.targets {
                    binding_roots(target, &mut names);
                }
                out.push((names, assign.value.as_ref()));
            }
            Stmt::AnnAssign(assign) => {
                if let Some(value) = assign.value.as_deref() {
                    let mut names = Vec::new();
                    binding_roots(&assign.target, &mut names);
                    out.push((names, value));
                }
            }
            Stmt::AugAssign(assign) => {
                let mut names = Vec::new();
                binding_roots(&assign.target, &mut names);
                out.push((names, assign.value.as_ref()));
            }
            Stmt::If(if_stmt) => {
                module_bindings(&if_stmt.body, out, registries);
                module_bindings(&if_stmt.orelse, out, registries);
            }
            Stmt::Try(try_stmt) => {
                module_bindings(&try_stmt.body, out, registries);
                module_bindings(&try_stmt.orelse, out, registries);
                module_bindings(&try_stmt.finalbody, out, registries);
            }
            Stmt::With(with_stmt) => module_bindings(&with_stmt.body, out, registries),
            // `for f in (verify_a, verify_b): HANDLERS.append(f)`.
            Stmt::For(for_stmt) => {
                let mut names = Vec::new();
                binding_roots(&for_stmt.target, &mut names);
                out.push((names, for_stmt.iter.as_ref()));
                module_bindings(&for_stmt.body, out, registries);
                module_bindings(&for_stmt.orelse, out, registries);
            }
            _ => {}
        }
    }
}

/// Root names of attribute decorators (`@HANDLERS.append`,
/// `@registry.register("k")`): the decorated definition is stored there.
fn registry_roots(decorators: &[Expr], out: &mut Vec<String>) {
    for decorator in decorators {
        let callee = match decorator {
            Expr::Call(call) => call.func.as_ref(),
            other => other,
        };
        if let Expr::Attribute(attribute) = callee {
            binding_roots(&attribute.value, out);
        }
    }
}

/// Names bound by an assignment target; a subscript or attribute target
/// (`REGISTRY["k"] = f`) taints its root name.
fn binding_roots(target: &Expr, out: &mut Vec<String>) {
    match target {
        Expr::Name(name) => out.push(name.id.to_string()),
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|elt| binding_roots(elt, out)),
        Expr::List(list) => list.elts.iter().for_each(|elt| binding_roots(elt, out)),
        Expr::Starred(starred) => binding_roots(&starred.value, out),
        Expr::Subscript(subscript) => binding_roots(&subscript.value, out),
        Expr::Attribute(attribute) => binding_roots(&attribute.value, out),
        _ => {}
    }
}

/// A plain dotted name. Unlike `expr_full_name`, a call inside the chain
/// (`type(self)._check`) yields `None`: the call's result, not the callee's
/// name, decides what runs.
fn dotted_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => dotted_name(attribute.value.as_ref())
            .map(|prefix| format!("{prefix}.{}", attribute.attr)),
        _ => None,
    }
}

/// A dunder member (`self.__class__._check`, `sys.exit.__call__`) reaches
/// the object model, so the chain no longer names what runs.
fn has_dunder_member(segments: &[&str]) -> bool {
    segments
        .iter()
        .skip(1)
        .any(|segment| segment.starts_with("__"))
}

fn has_assertion_like_prefix(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    ASSERTION_LIKE_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
}

/// Built-ins that can fail or end a test, or run code RIPR cannot read.
const UNSAFE_BUILTINS: &[&str] = &[
    "__import__",
    "breakpoint",
    "compile",
    "eval",
    "exec",
    "exit",
    "getattr",
    "globals",
    "locals",
    "quit",
    "vars",
];

/// Callee name prefixes that read as an assertion or a failure. They only
/// ever withhold `NoAssertionLike` (a method on a test-bound value such as
/// `result.verify()`), never grant it.
const ASSERTION_LIKE_PREFIXES: &[&str] = &[
    "assert", "check", "compare", "ensure", "expect", "fail", "must", "raise", "require", "should",
    "validate", "verify",
];

/// The parts of one test function the admission scan reads.
pub(super) struct PythonTestFunction<'a> {
    pub(super) body: &'a [Stmt],
    pub(super) decorators: &'a [Expr],
    /// The test's non-`self` parameters.
    pub(super) parameters: &'a [String],
    /// Module imports plus the test body's own imports.
    pub(super) imports: &'a [PythonImport],
    /// The enclosing test class path (`TestOuter.TestInner`), if any.
    pub(super) class_path: Option<&'a str>,
}

/// The admission state of one test.
///
/// `parametrize_argnames` are the names a statically certain
/// `@pytest.mark.parametrize` binds (`None` when the decorators cannot be
/// enumerated). A parameter that is neither a built-in fixture nor a known
/// parametrize argname is a fixture RIPR cannot see into.
pub(super) fn assertion_admission(
    test: &PythonTestFunction<'_>,
    assertions: &[PythonAssertion],
    parametrize_argnames: Option<&BTreeSet<String>>,
    context: &PythonAdmissionContext,
) -> PythonAssertionAdmission {
    if assertions
        .iter()
        .any(|assertion| assertion.oracle_shape != PythonOracleShape::UnknownCustomHelper)
    {
        return PythonAssertionAdmission::Recognized;
    }
    let opaque_class = test.class_path.is_some_and(|path| {
        path.split('.')
            .any(|class| context.opaque_classes.contains(class))
    });
    if !assertions.is_empty()
        || context.has_implicit_fixture
        || context.has_assertion_like_lifecycle
        || opaque_class
    {
        return PythonAssertionAdmission::Unresolved;
    }
    // A built-in fixture name the module redefines (`def tmp_path(): ...`)
    // runs that fixture instead.
    let opaque_fixture = test.parameters.iter().any(|name| {
        (!PYTEST_BUILTIN_FIXTURES.contains(&name.as_str()) || context.defined_names.contains(name))
            && !parametrize_argnames.is_some_and(|argnames| argnames.contains(name))
    });
    if opaque_fixture {
        return PythonAssertionAdmission::Unresolved;
    }
    let scope = TestScope::of_body(
        &context.module_dir,
        test.parameters,
        test.body,
        parametrize_argnames,
    );
    let mut scan = BodyScan {
        context,
        imports: test.imports,
        scope: &scope,
        assertion_like: false,
    };
    // A decorator wraps the test: a same-module or unknown decorator may
    // assert around it, and `usefixtures` requests fixtures by name.
    for decorator in test.decorators {
        scan.decorator(decorator);
    }
    scan.statements(test.body);
    if scan.assertion_like {
        PythonAssertionAdmission::Unresolved
    } else {
        PythonAssertionAdmission::NoAssertionLike
    }
}

fn is_autouse_fixture(decorator: &Expr) -> bool {
    let Expr::Call(call) = decorator else {
        return false;
    };
    // `fixture`, `pytest.fixture` and the old `pytest.yield_fixture`.
    let is_fixture =
        expr_full_name(call.func.as_ref()).is_some_and(|name| name.ends_with("fixture"));
    // A non-literal `autouse=` value cannot be ruled out, so it counts.
    is_fixture
        && call.keywords.iter().any(|keyword| {
            keyword.arg.as_ref().is_some_and(|arg| arg == "autouse")
                && !matches!(
                    &keyword.value,
                    Expr::Constant(constant)
                        if matches!(constant.value, ast::Constant::Bool(false))
                )
        })
}

fn applies_implicit_mark(expr: &Expr) -> bool {
    match expr {
        Expr::Call(call) => applies_implicit_mark(&call.func),
        Expr::Attribute(attribute) => {
            matches!(attribute.attr.as_str(), "usefixtures" | "filterwarnings")
                || applies_implicit_mark(&attribute.value)
        }
        // A bare name may be an alias bound to any mark
        // (`mark = pytest.mark.usefixtures("db")`).
        Expr::Name(_) => true,
        Expr::List(list) => list.elts.iter().any(applies_implicit_mark),
        Expr::Tuple(tuple) => tuple.elts.iter().any(applies_implicit_mark),
        // Anything else may compute marks RIPR cannot read.
        _ => true,
    }
}

/// Names a test binds: its parameters, and every name its body assigns,
/// loops over, imports, catches or defines. A method on one of these values
/// is the test's own computation, not a hidden helper.
struct TestScope {
    /// The dotted directory of the test file (`e2e` for `e2e/test_m.py`).
    /// A module imported from there or below is test support.
    module_dir: String,
    builtin_fixtures: BTreeSet<String>,
    argnames: BTreeSet<String>,
    locals: BTreeSet<String>,
}

impl TestScope {
    fn of_body(
        module_dir: &str,
        parameters: &[String],
        body: &[Stmt],
        parametrize_argnames: Option<&BTreeSet<String>>,
    ) -> Self {
        let mut locals = BTreeSet::new();
        bound_names(body, &mut locals);
        Self {
            module_dir: module_dir.to_string(),
            builtin_fixtures: parameters
                .iter()
                .filter(|name| PYTEST_BUILTIN_FIXTURES.contains(&name.as_str()))
                .cloned()
                .collect(),
            argnames: parametrize_argnames.cloned().unwrap_or_default(),
            locals,
        }
    }
}

fn bound_names(statements: &[Stmt], out: &mut BTreeSet<String>) {
    for stmt in statements {
        match stmt {
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    target_names(target, out);
                }
            }
            Stmt::AnnAssign(assign) => target_names(&assign.target, out),
            Stmt::AugAssign(assign) => target_names(&assign.target, out),
            Stmt::For(for_stmt) => {
                target_names(&for_stmt.target, out);
                bound_names(&for_stmt.body, out);
                bound_names(&for_stmt.orelse, out);
            }
            Stmt::AsyncFor(for_stmt) => {
                target_names(&for_stmt.target, out);
                bound_names(&for_stmt.body, out);
                bound_names(&for_stmt.orelse, out);
            }
            Stmt::While(while_stmt) => {
                bound_names(&while_stmt.body, out);
                bound_names(&while_stmt.orelse, out);
            }
            Stmt::If(if_stmt) => {
                bound_names(&if_stmt.body, out);
                bound_names(&if_stmt.orelse, out);
            }
            Stmt::With(with_stmt) => {
                for item in &with_stmt.items {
                    if let Some(vars) = &item.optional_vars {
                        target_names(vars, out);
                    }
                }
                bound_names(&with_stmt.body, out);
            }
            Stmt::AsyncWith(with_stmt) => {
                for item in &with_stmt.items {
                    if let Some(vars) = &item.optional_vars {
                        target_names(vars, out);
                    }
                }
                bound_names(&with_stmt.body, out);
            }
            Stmt::Try(try_stmt) => {
                bound_names(&try_stmt.body, out);
                handler_names(&try_stmt.handlers, out);
                bound_names(&try_stmt.orelse, out);
                bound_names(&try_stmt.finalbody, out);
            }
            Stmt::TryStar(try_stmt) => {
                bound_names(&try_stmt.body, out);
                handler_names(&try_stmt.handlers, out);
                bound_names(&try_stmt.orelse, out);
                bound_names(&try_stmt.finalbody, out);
            }
            Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    bound_names(&case.body, out);
                }
            }
            // A nested def or class is walked by the scan itself.
            Stmt::FunctionDef(function) => {
                out.insert(function.name.to_string());
            }
            Stmt::AsyncFunctionDef(function) => {
                out.insert(function.name.to_string());
            }
            Stmt::ClassDef(class) => {
                out.insert(class.name.to_string());
            }
            _ => {}
        }
    }
}

fn handler_names(handlers: &[ast::ExceptHandler], out: &mut BTreeSet<String>) {
    for handler in handlers {
        let ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(name) = &handler.name {
            out.insert(name.to_string());
        }
        bound_names(&handler.body, out);
    }
}

fn target_names(target: &Expr, out: &mut BTreeSet<String>) {
    match target {
        Expr::Name(name) => {
            out.insert(name.id.to_string());
        }
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|elt| target_names(elt, out)),
        Expr::List(list) => list.elts.iter().for_each(|elt| target_names(elt, out)),
        Expr::Starred(starred) => target_names(&starred.value, out),
        _ => {}
    }
}

/// Walks every statement and expression of a test body, including nested
/// functions, lambdas, defaults and classes, looking for anything
/// assertion-like.
struct BodyScan<'a> {
    context: &'a PythonAdmissionContext,
    imports: &'a [PythonImport],
    scope: &'a TestScope,
    assertion_like: bool,
}

impl BodyScan<'_> {
    fn statements(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            if self.assertion_like {
                return;
            }
            self.statement(stmt);
        }
    }

    fn statement(&mut self, stmt: &Stmt) {
        match stmt {
            // Any `assert` the oracle extractor did not record (one in a
            // nested function, for example) and any `raise` can fail the test.
            Stmt::Assert(_) | Stmt::Raise(_) => self.assertion_like = true,
            // Nested decorators, bases and metaclasses run when the
            // definition executes, so they are judged as calls.
            Stmt::FunctionDef(function) => {
                function
                    .decorator_list
                    .iter()
                    .for_each(|d| self.decorator(d));
                self.arguments(&function.args);
                self.statements(&function.body);
            }
            Stmt::AsyncFunctionDef(function) => {
                function
                    .decorator_list
                    .iter()
                    .for_each(|d| self.decorator(d));
                self.arguments(&function.args);
                self.statements(&function.body);
            }
            Stmt::ClassDef(class) => {
                class.bases.iter().for_each(|base| self.decorator(base));
                for keyword in &class.keywords {
                    self.decorator(&keyword.value);
                }
                class.decorator_list.iter().for_each(|d| self.decorator(d));
                self.statements(&class.body);
            }
            Stmt::Return(ret) => self.opt_expr(ret.value.as_deref()),
            Stmt::Delete(delete) => self.exprs(&delete.targets),
            Stmt::Assign(assign) => {
                self.exprs(&assign.targets);
                self.expr(&assign.value);
            }
            Stmt::TypeAlias(alias) => self.expr(&alias.value),
            Stmt::AugAssign(assign) => {
                self.expr(&assign.target);
                self.expr(&assign.value);
            }
            Stmt::AnnAssign(assign) => {
                self.expr(&assign.target);
                self.opt_expr(assign.value.as_deref());
            }
            Stmt::For(for_stmt) => {
                self.expr(&for_stmt.target);
                self.expr(&for_stmt.iter);
                self.statements(&for_stmt.body);
                self.statements(&for_stmt.orelse);
            }
            Stmt::AsyncFor(for_stmt) => {
                self.expr(&for_stmt.target);
                self.expr(&for_stmt.iter);
                self.statements(&for_stmt.body);
                self.statements(&for_stmt.orelse);
            }
            Stmt::While(while_stmt) => {
                self.expr(&while_stmt.test);
                self.statements(&while_stmt.body);
                self.statements(&while_stmt.orelse);
            }
            Stmt::If(if_stmt) => {
                self.expr(&if_stmt.test);
                self.statements(&if_stmt.body);
                self.statements(&if_stmt.orelse);
            }
            Stmt::With(with_stmt) => {
                self.with_items(&with_stmt.items);
                self.statements(&with_stmt.body);
            }
            Stmt::AsyncWith(with_stmt) => {
                self.with_items(&with_stmt.items);
                self.statements(&with_stmt.body);
            }
            Stmt::Match(match_stmt) => {
                self.expr(&match_stmt.subject);
                for case in &match_stmt.cases {
                    self.opt_expr(case.guard.as_deref());
                    self.statements(&case.body);
                }
            }
            Stmt::Try(try_stmt) => {
                self.statements(&try_stmt.body);
                self.handlers(&try_stmt.handlers);
                self.statements(&try_stmt.orelse);
                self.statements(&try_stmt.finalbody);
            }
            Stmt::TryStar(try_stmt) => {
                self.statements(&try_stmt.body);
                self.handlers(&try_stmt.handlers);
                self.statements(&try_stmt.orelse);
                self.statements(&try_stmt.finalbody);
            }
            Stmt::Expr(expr_stmt) => self.expr(&expr_stmt.value),
            Stmt::Import(_)
            | Stmt::ImportFrom(_)
            | Stmt::Global(_)
            | Stmt::Nonlocal(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_) => {}
        }
    }

    /// Default values run when a nested function or lambda is defined.
    fn arguments(&mut self, args: &ast::Arguments) {
        for arg in args
            .posonlyargs
            .iter()
            .chain(args.args.iter())
            .chain(args.kwonlyargs.iter())
        {
            self.opt_expr(arg.default.as_deref());
        }
    }

    fn decorator(&mut self, decorator: &Expr) {
        if let Expr::Call(_) = decorator {
            self.expr(decorator);
        } else if let Some(name) = dotted_name(decorator) {
            if self.name_is_assertion_like(&name) {
                self.assertion_like = true;
            }
        } else {
            self.assertion_like = true;
        }
    }

    fn with_items(&mut self, items: &[ast::WithItem]) {
        for item in items {
            self.expr(&item.context_expr);
            self.opt_expr(item.optional_vars.as_deref());
        }
    }

    fn handlers(&mut self, handlers: &[ast::ExceptHandler]) {
        for handler in handlers {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            self.opt_expr(handler.type_.as_deref());
            self.statements(&handler.body);
        }
    }

    fn exprs(&mut self, exprs: &[Expr]) {
        for expr in exprs {
            self.expr(expr);
        }
    }

    fn opt_expr(&mut self, expr: Option<&Expr>) {
        if let Some(expr) = expr {
            self.expr(expr);
        }
    }

    fn comprehensions(&mut self, generators: &[ast::Comprehension]) {
        for generator in generators {
            self.expr(&generator.target);
            self.expr(&generator.iter);
            self.exprs(&generator.ifs);
        }
    }

    fn expr(&mut self, expr: &Expr) {
        if self.assertion_like {
            return;
        }
        match expr {
            Expr::Call(call) => {
                if self.call_is_assertion_like(call) {
                    self.assertion_like = true;
                    return;
                }
                self.expr(&call.func);
                self.exprs(&call.args);
                for keyword in &call.keywords {
                    self.expr(&keyword.value);
                }
            }
            Expr::BoolOp(op) => self.exprs(&op.values),
            Expr::NamedExpr(named) => {
                self.expr(&named.target);
                self.expr(&named.value);
            }
            Expr::BinOp(op) => {
                self.expr(&op.left);
                self.expr(&op.right);
            }
            Expr::UnaryOp(op) => self.expr(&op.operand),
            Expr::Lambda(lambda) => {
                self.arguments(&lambda.args);
                self.expr(&lambda.body);
            }
            Expr::IfExp(if_exp) => {
                self.expr(&if_exp.test);
                self.expr(&if_exp.body);
                self.expr(&if_exp.orelse);
            }
            Expr::Dict(dict) => {
                for key in dict.keys.iter().flatten() {
                    self.expr(key);
                }
                self.exprs(&dict.values);
            }
            Expr::Set(set) => self.exprs(&set.elts),
            Expr::ListComp(comp) => {
                self.expr(&comp.elt);
                self.comprehensions(&comp.generators);
            }
            Expr::SetComp(comp) => {
                self.expr(&comp.elt);
                self.comprehensions(&comp.generators);
            }
            Expr::DictComp(comp) => {
                self.expr(&comp.key);
                self.expr(&comp.value);
                self.comprehensions(&comp.generators);
            }
            Expr::GeneratorExp(comp) => {
                self.expr(&comp.elt);
                self.comprehensions(&comp.generators);
            }
            Expr::Await(await_expr) => self.expr(&await_expr.value),
            Expr::Yield(yield_expr) => self.opt_expr(yield_expr.value.as_deref()),
            Expr::YieldFrom(yield_from) => self.expr(&yield_from.value),
            Expr::Compare(compare) => {
                self.expr(&compare.left);
                self.exprs(&compare.comparators);
            }
            Expr::FormattedValue(value) => {
                self.expr(&value.value);
                self.opt_expr(value.format_spec.as_deref());
            }
            Expr::JoinedStr(joined) => self.exprs(&joined.values),
            // `self._verify` passed as a callback (`self.addCleanup(...)`).
            Expr::Attribute(attribute) => {
                if matches!(attribute.value.as_ref(), Expr::Name(name) if matches!(name.id.as_str(), "self" | "cls"))
                    && self.context.defined_names.contains(attribute.attr.as_str())
                {
                    self.assertion_like = true;
                    return;
                }
                // An attribute of a call result (`import_module("sys").exit`)
                // can name any callable.
                if matches!(attribute.value.as_ref(), Expr::Call(_)) {
                    self.assertion_like = true;
                    return;
                }
                if let Some(name) = dotted_name(expr) {
                    if self.value_is_assertion_like(&name) {
                        self.assertion_like = true;
                        return;
                    }
                    // The whole dotted chain was judged as one name; its
                    // prefixes (`pytest.mark` in `pytest.mark.skip`) are not
                    // separate references.
                    if matches!(attribute.value.as_ref(), Expr::Name(_) | Expr::Attribute(_)) {
                        return;
                    }
                }
                self.expr(&attribute.value);
            }
            Expr::Subscript(subscript) => {
                self.expr(&subscript.value);
                self.expr(&subscript.slice);
            }
            Expr::Starred(starred) => self.expr(&starred.value),
            Expr::List(list) => self.exprs(&list.elts),
            Expr::Tuple(tuple) => self.exprs(&tuple.elts),
            Expr::Slice(slice) => {
                self.opt_expr(slice.lower.as_deref());
                self.opt_expr(slice.upper.as_deref());
                self.opt_expr(slice.step.as_deref());
            }
            // A same-module function or class passed or stored as a value
            // (`run = run_case`) may be called later.
            // A forbidden callee bound or passed as a value (`run = run_case`,
            // `map(run_case, xs)`, `stop = sys.exit`) may be called later.
            Expr::Name(name) => {
                if self.value_is_assertion_like(name.id.as_str()) {
                    self.assertion_like = true;
                }
            }
            Expr::Constant(_) => {}
        }
    }

    /// A call is assertion-like unless its callee resolves to something known
    /// not to assert. Calling the result of a call (`make_checker()(x)`) or a
    /// subscript (`checks[kind](x)`) is dynamic and so assertion-like.
    fn call_is_assertion_like(&self, call: &ast::ExprCall) -> bool {
        if matches!(call.func.as_ref(), Expr::Call(_)) {
            return true;
        }
        match (dotted_name(call.func.as_ref()), call.func.as_ref()) {
            (Some(name), _) => self.name_is_assertion_like(&name),
            // A method on a computed value (`(tmp_path / "x").write_text(...)`,
            // `"a,b".split(",")`) acts on that value; the value itself is
            // walked separately. A method on a call or subscript result
            // (`type(self)._check(x)`, `import_module("sys").exit()`) can
            // reach any callable, so it stays assertion-like.
            (None, Expr::Attribute(attribute))
                if !matches!(attribute.value.as_ref(), Expr::Subscript(_) | Expr::Call(_)) =>
            {
                has_assertion_like_prefix(attribute.attr.as_str())
            }
            (None, _) => true,
        }
    }

    /// Whether a name used as a value, not called here, is something that
    /// could fail the test once called. Unlike a call, an unknown global used
    /// as a value (a module constant) is not suspect by itself.
    fn value_is_assertion_like(&self, name: &str) -> bool {
        let segments: Vec<&str> = name.split('.').collect();
        let root = segments.first().copied().unwrap_or(name);
        let last = segments.last().copied().unwrap_or(name);
        if has_dunder_member(&segments) {
            return true;
        }
        if matches!(root, "self" | "cls") {
            // The bare instance or class as a value (`t = self`,
            // `type(self)`) lets a local reach every method on it; an
            // inherited assertion method passed as a callback
            // (`d.addCallback(self.assertEqual, 5)`) is one by name.
            return segments.len() == 1 || has_assertion_like_prefix(last);
        }
        // A bare local holding a forbidden value is caught where it is
        // bound, so its own name (`expected = 10`) is not suspect. A member
        // of a local (`m.assert_called_once_with`, `v.validate`) still is.
        if self.scope.argnames.contains(root) || self.scope.locals.contains(root) {
            return segments.len() > 1 && has_assertion_like_prefix(last);
        }
        if has_assertion_like_prefix(last) {
            return true;
        }
        if self.scope.builtin_fixtures.contains(root) {
            return matches!(last, "getfixturevalue" | "getfuncargvalue");
        }
        if let Some(qualified) = self.imported_name(name) {
            return self.import_is_assertion_like(&qualified);
        }
        // A star import (`from tests.helpers import *`) can bind any
        // otherwise unknown global to a helper.
        let star_imported = self.imports.iter().any(|import| import.alias == "*")
            && !INERT_BUILTINS.contains(&root);
        self.context.defined_names.contains(root)
            || UNSAFE_BUILTINS.contains(&root)
            || star_imported
    }

    fn import_is_assertion_like(&self, qualified: &str) -> bool {
        is_test_support_path(qualified)
            || (!self.scope.module_dir.is_empty()
                && qualified.starts_with(&format!("{}.", self.scope.module_dir)))
            || FAILING_CALLEES.contains(&qualified)
            || (qualified.starts_with("pytest.") && !is_inert_pytest_member(qualified))
    }

    fn name_is_assertion_like(&self, name: &str) -> bool {
        let segments: Vec<&str> = name.split('.').collect();
        let root = segments.first().copied().unwrap_or(name);
        let last = segments.last().copied().unwrap_or(name);
        if has_assertion_like_prefix(last) || has_dunder_member(&segments) {
            return true;
        }
        if matches!(root, "self" | "cls") {
            // `self.helper()` may be defined here or inherited; only
            // unittest's `subTest` is known not to assert. A method on an
            // attribute (`self.client.get()`) acts on a value the test or its
            // setup built, and setup hooks are scanned separately.
            return segments.len() == 2 && last != "subTest";
        }
        if self.scope.locals.contains(root) {
            return false;
        }
        if self.scope.builtin_fixtures.contains(root) {
            // `request.getfixturevalue("name")` pulls in any fixture.
            return matches!(last, "getfixturevalue" | "getfuncargvalue");
        }
        if self.scope.argnames.contains(root) {
            // Calling a parameter runs whatever the case supplies.
            return segments.len() == 1;
        }
        if let Some(qualified) = self.imported_name(name) {
            return self.import_is_assertion_like(&qualified);
        }
        if self.context.defined_names.contains(root) {
            return true;
        }
        if INERT_BUILTINS.contains(&root)
            || root.ends_with("Error")
            || root.ends_with("Exception")
            || root.ends_with("Warning")
        {
            return false;
        }
        // An unknown global: a module variable, a star import, `exit`,
        // `eval`, or a name bound somewhere RIPR does not track.
        true
    }

    /// The fully qualified spelling of a name whose root is imported:
    /// `from pytest import warns` makes `warns` read `pytest.warns`, and
    /// `import tests.helpers as h` makes `h.run` read `tests.helpers.run`.
    fn imported_name(&self, name: &str) -> Option<String> {
        self.imports
            .iter()
            .filter(|import| {
                name == import.alias || name.starts_with(&format!("{}.", import.alias))
            })
            .max_by_key(|import| import.alias.len())
            .map(|import| {
                let rest = &name[import.alias.len()..];
                if import.source_module.is_empty() {
                    format!("{}{rest}", import.imported)
                } else {
                    format!("{}.{}{rest}", import.source_module, import.imported)
                }
            })
    }
}
