//! Whether RIPR can honestly say a Python test has no assertion (#5571).
//!
//! An empty `PythonTest::assertions` vector only means the oracle extractor
//! recognized nothing. That is a weaker fact than "this test asserts nothing":
//! a custom helper, a same-module wrapper, a fixture, an `assert` inside a
//! nested function, or a dynamic call can all check behavior the extractor
//! does not model. A later related-test miss (`no_assertion`, #5491) may only
//! be claimed from [`PythonAssertionAdmission::NoAssertionLike`], so this scan
//! is fail-closed: anything that could be an assertion it does not recognize
//! makes the state [`PythonAssertionAdmission::Unresolved`].
//!
//! A file the parser refuses yields no `PythonTest` at all, so an extracted
//! test is never "partially extracted": the parse-limited state is carried by
//! the absence of the row and the file's `unsupported_syntax` limitation, not
//! by a value here.
//!
//! Test activation (skip / xfail / expected failure, #5389) is a separate
//! fact and is not folded into this state.

use super::expr_full_name;
use super::{PythonAssertion, PythonOracleShape};
use rustpython_parser::ast::{self, Expr, Stmt};
use std::collections::BTreeSet;

/// Assertion admission for one extracted Python test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PythonAssertionAdmission {
    /// The extractor recognized at least one assertion or observer it can
    /// grade (anything other than an unknown custom helper).
    Recognized,
    /// The whole body was walked and nothing assertion-like exists: no
    /// `assert`, no `raise`, no assertion-named or dynamic call, no call to a
    /// helper defined in the same module, and no fixture that could assert.
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

/// Module-wide facts the per-test scan needs: the names of functions defined
/// in the same file that are not themselves tests (a test calling one may be
/// asserting through it), and whether the module declares an autouse fixture
/// (which can assert around every test without being named).
#[derive(Clone, Debug, Default)]
pub(super) struct PythonAdmissionContext {
    helper_names: BTreeSet<String>,
    has_autouse_fixture: bool,
}

impl PythonAdmissionContext {
    pub(super) fn of_module(statements: &[Stmt]) -> Self {
        let mut context = Self::default();
        context.collect(statements);
        context
    }

    fn collect(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            match stmt {
                Stmt::FunctionDef(function) => {
                    self.note_function(function.name.as_str(), &function.decorator_list);
                }
                Stmt::AsyncFunctionDef(function) => {
                    self.note_function(function.name.as_str(), &function.decorator_list);
                }
                Stmt::ClassDef(class) => self.collect(&class.body),
                _ => {}
            }
        }
    }

    fn note_function(&mut self, name: &str, decorators: &[Expr]) {
        if decorators.iter().any(is_autouse_fixture) {
            self.has_autouse_fixture = true;
        }
        if !name.starts_with("test") {
            self.helper_names.insert(name.to_string());
        }
    }
}

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

/// Callee name prefixes that read as an assertion or a failure. A call whose
/// last segment starts with one of these and that the oracle extractor did
/// not grade is assertion-like but unresolved.
const ASSERTION_LIKE_PREFIXES: &[&str] = &[
    "assert", "check", "compare", "ensure", "expect", "fail", "must", "require", "should",
    "validate", "verify",
];

/// The admission state of one test body.
///
/// `parameters` are the test function's non-`self` parameters and
/// `parametrize_argnames` the names a statically certain
/// `@pytest.mark.parametrize` binds (`None` when the decorators cannot be
/// enumerated). A parameter that is neither a built-in fixture nor a known
/// parametrize argname is a fixture RIPR cannot see into.
pub(super) fn assertion_admission(
    body: &[Stmt],
    assertions: &[PythonAssertion],
    parameters: &[String],
    parametrize_argnames: Option<&BTreeSet<String>>,
    context: &PythonAdmissionContext,
) -> PythonAssertionAdmission {
    if assertions
        .iter()
        .any(|assertion| assertion.oracle_shape != PythonOracleShape::UnknownCustomHelper)
    {
        return PythonAssertionAdmission::Recognized;
    }
    if !assertions.is_empty() || context.has_autouse_fixture {
        return PythonAssertionAdmission::Unresolved;
    }
    let opaque_fixture = parameters.iter().any(|name| {
        !PYTEST_BUILTIN_FIXTURES.contains(&name.as_str())
            && !parametrize_argnames.is_some_and(|argnames| argnames.contains(name))
    });
    if opaque_fixture {
        return PythonAssertionAdmission::Unresolved;
    }
    let mut scan = BodyScan {
        context,
        assertion_like: false,
    };
    scan.statements(body);
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
    let is_fixture = expr_full_name(call.func.as_ref())
        .is_some_and(|name| name == "fixture" || name.ends_with(".fixture"));
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

/// Walks every statement and expression of a test body, including nested
/// functions, lambdas and classes, looking for anything assertion-like.
struct BodyScan<'a> {
    context: &'a PythonAdmissionContext,
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
            Stmt::FunctionDef(function) => {
                self.exprs(&function.decorator_list);
                self.statements(&function.body);
            }
            Stmt::AsyncFunctionDef(function) => {
                self.exprs(&function.decorator_list);
                self.statements(&function.body);
            }
            Stmt::ClassDef(class) => {
                self.exprs(&class.bases);
                self.exprs(&class.decorator_list);
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
            Expr::Lambda(lambda) => self.expr(&lambda.body),
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
            Expr::Attribute(attribute) => self.expr(&attribute.value),
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
            Expr::Constant(_) | Expr::Name(_) => {}
        }
    }

    /// A call is assertion-like when its callee cannot be named statically
    /// (`checks[kind](value)`), when its last segment reads as an assertion
    /// or a failure, or when it names a function defined in the same module
    /// that is not itself a test (a helper that may assert).
    fn call_is_assertion_like(&self, call: &ast::ExprCall) -> bool {
        let Some(name) = expr_full_name(call.func.as_ref()) else {
            return true;
        };
        let last = name.rsplit('.').next().unwrap_or(name.as_str());
        let lowered = last.to_ascii_lowercase();
        ASSERTION_LIKE_PREFIXES
            .iter()
            .any(|prefix| lowered.starts_with(prefix))
            || self.context.helper_names.contains(last)
    }
}
