//! Whether RIPR can honestly say a TypeScript test has no assertion (#5524).
//!
//! An empty `TypeScriptTest::assertions` vector only means the oracle
//! extractor recognized nothing. That is a weaker fact than "this test asserts
//! nothing": a same-file helper, a test-support import, an assertion library
//! form the extractor does not model, a `throw`, a hook, a `done(err)` call or
//! a dynamic callee can all fail the test. A later related-test miss
//! (`no_assertion`, #5495) may only be claimed from
//! [`TypeScriptAssertionAdmission::NoAssertionLike`], so this scan is
//! fail-closed. Every callee and value reference in the test callback, and in
//! everything the runner runs for it, must resolve to something known not to
//! assert:
//!
//! - a binding the test itself made (a local, or a function it defines and
//!   whose body is walked here);
//! - a name imported from a relative module outside test-support paths (a
//!   production owner), whatever its name: `checkout` and `checkLimit` from
//!   `../src/cart` are equally inert, so an unrelated helper never becomes
//!   assertion-shaped by name coincidence;
//! - a Node built-in module from a short allowlist;
//! - a JavaScript built-in that cannot fail a test by itself (`Math`, `JSON`,
//!   `Object`, `console.log`, `Promise.resolve`, ...);
//! - a mock or timer control on the runner object (`vi.fn`, `jest.spyOn`,
//!   `vi.useFakeTimers`, ...);
//! - a value bound outside the test by a declaration or a hook (`let cart;
//!   beforeEach(() => { cart = new Cart() })`), used as a receiver.
//!
//! Anything else makes the state [`TypeScriptAssertionAdmission::Unresolved`]:
//! a same-file function or class (at any describe level), an import from a
//! test-support path (`tests/`, `__tests__`, `helpers`, `fixtures`, `*.spec`,
//! ...), a third-party package (testing-library queries and `supertest`'s
//! `.expect` assert), an unknown global (`fail`, a global `expect`), a direct
//! call of a parameter (`reject(err)`), a `done`/`t` context passed on or
//! called with an argument, a context member outside a tiny allowlist
//! (`t.plan`, `t.fail`), `throw`, a dynamic callee (`handlers[k]()`,
//! `make()()`), `import()`, JSX, a method whose name or receiver path reads
//! as an assertion on a local (`result.verify()`, `x.should.equal`), a custom
//! test registration (`const it = base.extend(...)`), or a nested test
//! registration inside the test body.
//!
//! The runner also runs code the test does not name. Every statement at file
//! level and in every `describe` callback that is not a test registration is
//! scanned with the same rules: hooks (`beforeEach`, `afterAll`, mocha
//! `before`, AVA `test.beforeEach`), top-level calls (`expect.extend(...)`),
//! variable initializers, side-effect imports (`import './setup'`) and any
//! import from a test-support module (whose own top level can register hooks).
//! When anything there is assertion-like, every test in the file is
//! `Unresolved`. This is per file, not per describe block: a hook in one
//! `describe` withholds the established state from tests in its siblings too,
//! which only ever errs toward `Unresolved`.
//!
//! A file the parser refuses yields no `TypeScriptTest` at all (see
//! `extract_tests`), so an extracted test is never "partially parsed": the
//! parse-limited state is carried by the absence of the row and the file's
//! parse limitation, not by a value here. A construct inside an extracted
//! body that this scan does not model is `Unresolved`, which is how an
//! incomplete extraction stays distinct from an established absence.
//!
//! Known blind spots: the scan sees one file. Project setup files
//! (`setupFiles`, `setupFilesAfterEach`, Vitest `setupFiles`, a global
//! `afterEach(() => expect.hasAssertions())`) and runner plugins can still fail
//! a `NoAssertionLike` test. A relative import outside test-support paths is
//! treated as production even when it is a test-only helper with a neutral
//! name (`./utils` beside a colocated `src/cart.test.ts`). A value bound
//! outside the test is judged by its declaration and hook writes; a write made
//! by a sibling test (`shared = helper` in another `it`) is not traced. A
//! `no_assertion` miss must account for these before it is emitted.
//!
//! Test activation (skip / todo / Vitest `fails` / Node `expectFailure`,
//! #3506, #4638, #5433) is a separate fact and is not folded into this state.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Assertion admission for one extracted TypeScript test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScriptAssertionAdmission {
    /// The extractor recognized at least one assertion whose oracle kind it
    /// can grade (anything other than `OracleKind::Unknown`).
    Recognized,
    /// The whole test and everything the runner runs around it were walked
    /// and nothing assertion-like exists.
    NoAssertionLike,
    /// Something assertion-like is present that RIPR cannot admit or resolve.
    Unresolved,
}

impl TypeScriptAssertionAdmission {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Recognized => "recognized_assertion_present",
            Self::NoAssertionLike => "extraction_complete_no_assertion",
            Self::Unresolved => "assertion_like_present_but_unresolved",
        }
    }
}

/// File-wide facts the per-test scan needs.
#[derive(Clone, Debug, Default)]
pub(crate) struct TypeScriptAdmissionContext {
    /// Functions and classes declared outside test callbacks (at file level,
    /// in a `describe` callback or a setup-level block), including `const f =
    /// () => ...`. A test that calls or references one may assert through it.
    helpers: BTreeSet<String>,
    /// Other names bound outside test callbacks: variables, `describe.each`
    /// and loop parameters. Their initializers and hook writes are scanned as
    /// setup, so a member call on one acts on a value RIPR has seen.
    enclosing: BTreeSet<String>,
    imports: Vec<TypeScriptImport>,
    /// Directory segments of the test file, relative to the analysis root.
    test_dir: Vec<String>,
    /// Something the runner runs for every test in the file is
    /// assertion-like (see the module docs).
    setup_assertion_like: bool,
}

impl TypeScriptAdmissionContext {
    pub(crate) fn of_program(
        statements: &[Statement<'_>],
        imports: &[TypeScriptImport],
        file: &Path,
    ) -> Self {
        let mut context = Self {
            imports: imports.to_vec(),
            test_dir: file
                .parent()
                .map(|dir| {
                    dir.components()
                        .filter_map(|component| match component {
                            std::path::Component::Normal(segment) => {
                                Some(segment.to_string_lossy().into_owned())
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            ..Self::default()
        };
        let mut helpers = BTreeSet::new();
        let mut enclosing = BTreeSet::new();
        collect_setup_names(statements, &mut helpers, &mut enclosing);
        context.helpers = helpers;
        context.enclosing = enclosing;
        // Names are complete only after the whole file is seen.
        let mut scan = Scan::new(&context, false);
        scan.setup_statements(statements);
        context.setup_assertion_like = scan.assertion_like;
        context
    }

    /// The admission state of the test registered by `call`, whose
    /// recognized assertions are `assertions`.
    pub(crate) fn admission_of(
        &self,
        call: &oxc_ast::ast::CallExpression<'_>,
        assertions: &[TypeScriptAssertion],
    ) -> TypeScriptAssertionAdmission {
        if assertions
            .iter()
            .any(|assertion| assertion.oracle_kind != OracleKind::Unknown)
        {
            return TypeScriptAssertionAdmission::Recognized;
        }
        // An extracted assertion of unknown kind (a custom matcher such as
        // `expect(x).toBeEven()`) is assertion-like but not gradable.
        if !assertions.is_empty() || self.setup_assertion_like {
            return TypeScriptAssertionAdmission::Unresolved;
        }
        let mut scan = Scan::new(self, true);
        scan.test_registration(call);
        if scan.assertion_like {
            TypeScriptAssertionAdmission::Unresolved
        } else {
            TypeScriptAssertionAdmission::NoAssertionLike
        }
    }

    fn import_class(&self, import: &TypeScriptImport) -> ImportClass {
        classify_source(&import.source, &self.test_dir)
    }
}

/// What a module specifier is, for admission purposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportClass {
    /// A relative module outside test-support paths.
    Production,
    /// A Node built-in that cannot fail a test by itself.
    NodeBuiltin,
    /// A test runner module (`vitest`, `@jest/globals`, `node:test`, ...).
    Runner,
    /// A relative module under a test-support path.
    TestSupport,
    /// Any other package: an assertion library, a testing utility, or a
    /// dependency RIPR cannot see into.
    Package,
}

const RUNNER_MODULES: &[&str] = &[
    "vitest",
    "@jest/globals",
    "bun:test",
    "node:test",
    "test",
    "ava",
    "tape",
    "mocha",
    "uvu",
];

/// Node built-ins whose members compute values or perform I/O. `assert`,
/// `test`, `process` and the subprocess module (whose sync calls throw on a failing
/// command) are deliberately absent.
const NODE_BUILTINS: &[&str] = &[
    "buffer",
    "crypto",
    "events",
    "fs",
    "fs/promises",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "querystring",
    "stream",
    "stream/promises",
    "string_decoder",
    "timers",
    "timers/promises",
    "url",
    "util",
    "zlib",
];

fn classify_source(source: &str, test_dir: &[String]) -> ImportClass {
    if source.starts_with('.') || source.starts_with('/') {
        return if is_test_support_path(&resolve_relative(test_dir, source)) {
            ImportClass::TestSupport
        } else {
            ImportClass::Production
        };
    }
    if RUNNER_MODULES.contains(&source) {
        return ImportClass::Runner;
    }
    if NODE_BUILTINS.contains(&source.strip_prefix("node:").unwrap_or(source)) {
        return ImportClass::NodeBuiltin;
    }
    ImportClass::Package
}

#[cfg(test)]
pub(super) fn classify_for_test(source: &str, test_dir: &[String]) -> &'static str {
    match classify_source(source, test_dir) {
        ImportClass::Production => "production",
        ImportClass::NodeBuiltin => "node_builtin",
        ImportClass::Runner => "runner",
        ImportClass::TestSupport => "test_support",
        ImportClass::Package => "package",
    }
}

/// The segments of `specifier` resolved against the test file's directory.
fn resolve_relative(test_dir: &[String], specifier: &str) -> Vec<String> {
    let mut segments: Vec<String> = if specifier.starts_with('/') {
        Vec::new()
    } else {
        test_dir.to_vec()
    };
    for part in specifier.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    segments.push("..".to_string());
                }
            }
            other => segments.push(other.to_string()),
        }
    }
    segments
}

/// A resolved module path any of whose segments reads as test support. A
/// module there may be an assertion helper with any name, and its top level
/// may register hooks. False positives (`latest`, `inspect`) only withhold
/// the established state.
fn is_test_support_path(segments: &[String]) -> bool {
    segments.iter().any(|segment| {
        segment
            .to_ascii_lowercase()
            .split(['.', '-', '_'])
            .any(|token| {
                token.contains("test")
                    || token.contains("spec")
                    || token.contains("helper")
                    || token.contains("fixture")
                    || token.contains("mock")
                    || matches!(
                        token,
                        "support"
                            | "stub"
                            | "stubs"
                            | "fake"
                            | "fakes"
                            | "e2e"
                            | "harness"
                            | "setup"
                    )
            })
    })
}

/// Member names that read as an assertion or a failure. They only ever
/// withhold the established state for a member of a local or computed value
/// (`result.verify()`, `x.should.equal(1)`), never grant it, and are not
/// applied to production imports.
const ASSERTION_LIKE_PREFIXES: &[&str] = &[
    "assert", "check", "ensure", "expect", "fail", "must", "reject", "require", "should", "throw",
    "validate", "verify",
];

fn has_assertion_like_prefix(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    ASSERTION_LIKE_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
}

/// Global objects whose members compute values and cannot fail a test by
/// themselves, except the members `denied_global_member` lists.
const SAFE_GLOBAL_OBJECTS: &[&str] = &[
    "AbortController",
    "AbortSignal",
    "Array",
    "ArrayBuffer",
    "BigInt",
    "BigInt64Array",
    "BigUint64Array",
    "Boolean",
    "Buffer",
    "DataView",
    "Date",
    "Float32Array",
    "Float64Array",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "Intl",
    "JSON",
    "Map",
    "Math",
    "Number",
    "Object",
    "Promise",
    "RegExp",
    "Set",
    "String",
    "Symbol",
    "TextDecoder",
    "TextEncoder",
    "URL",
    "URLSearchParams",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "WeakMap",
    "WeakSet",
    "console",
    "performance",
    "process",
];

/// Global functions that compute values or schedule callbacks (which are
/// walked where they are written).
const SAFE_GLOBAL_FUNCTIONS: &[&str] = &[
    "Array",
    "BigInt",
    "Boolean",
    "Date",
    "Number",
    "Object",
    "String",
    "Symbol",
    "clearImmediate",
    "clearInterval",
    "clearTimeout",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
    "queueMicrotask",
    "setImmediate",
    "setInterval",
    "setTimeout",
    "structuredClone",
];

/// Plain global values.
const SAFE_GLOBAL_VALUES: &[&str] = &["undefined", "NaN", "Infinity"];

/// Members of a safe global that can fail a test: a rejected promise, an
/// assertion, or a process exit.
fn denied_global_member(object: &str, member: &str) -> bool {
    match object {
        "Promise" => matches!(member, "reject" | "withResolvers"),
        "console" => member == "assert",
        "process" => matches!(
            member,
            "abort" | "binding" | "dlopen" | "emit" | "exit" | "exitCode" | "kill" | "reallyExit"
        ),
        _ => false,
    }
}

/// `vi.*` / `jest.*` members that build mocks or drive timers. `waitFor`,
/// `waitUntil`, module loaders and every other member are absent.
const RUNNER_OBJECT_SAFE_MEMBERS: &[&str] = &[
    "advanceTimersByTime",
    "advanceTimersByTimeAsync",
    "advanceTimersToNextTimer",
    "clearAllMocks",
    "clearAllTimers",
    "doMock",
    "doUnmock",
    "fn",
    "getTimerCount",
    "hoisted",
    "isMockFunction",
    "mock",
    "mocked",
    "resetAllMocks",
    "resetModules",
    "restoreAllMocks",
    "runAllTicks",
    "runAllTimers",
    "runAllTimersAsync",
    "runOnlyPendingTimers",
    "runOnlyPendingTimersAsync",
    "setSystemTime",
    "spyOn",
    "stubEnv",
    "stubGlobal",
    "unmock",
    "unstubAllEnvs",
    "unstubAllGlobals",
    "useFakeTimers",
    "useRealTimers",
];

/// `node:test` `mock` members that build mocks.
const NODE_MOCK_SAFE_MEMBERS: &[&str] =
    &["fn", "getter", "method", "reset", "restoreAll", "setter"];

/// Hook registrations. Their callbacks are walked like any other callback.
const HOOKS: &[&str] = &[
    "after",
    "afterAll",
    "afterEach",
    "before",
    "beforeAll",
    "beforeEach",
];

const TEST_ROOTS: &[&str] = &[
    "bench", "fit", "it", "specify", "test", "xit", "xspecify", "xtest",
];
const DESCRIBE_ROOTS: &[&str] = &[
    "context",
    "describe",
    "fdescribe",
    "suite",
    "xcontext",
    "xdescribe",
    "xsuite",
];

/// Registration modifiers that keep a member chain a registration
/// (`it.only`, `describe.skip`). `extend`, hook members (`test.beforeEach`)
/// and anything unknown are not.
const REGISTRATION_MODIFIERS: &[&str] = &[
    "concurrent",
    "failing",
    "fails",
    "only",
    "sequential",
    "serial",
    "skip",
    "todo",
];

/// Modifiers that take arguments and return the registration function
/// (`test.each(table)('name', fn)`, `test.skipIf(cond)(...)`).
const REGISTRATION_FACTORIES: &[&str] = &["each", "for", "runIf", "skipIf"];

/// Members a test callback's context parameter may use without asserting:
/// `node:test`'s `t.diagnostic` / `t.mock.fn`, mocha's `this.timeout`.
const CONTEXT_SAFE_MEMBERS: &[&str] = &[
    "diagnostic",
    "mock.fn",
    "mock.getter",
    "mock.method",
    "mock.setter",
    "retries",
    "slow",
    "timeout",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegistrationKind {
    Test,
    Describe,
}

/// A recognized `test`/`describe` registration call.
struct Registration<'a, 'b> {
    kind: RegistrationKind,
    /// Arguments of a factory in the callee (`.each(table)`, `.skipIf(c)`).
    factory_args: &'b [Argument<'a>],
    /// The factory is `.each` / `.for`, so callback parameters take rows.
    table: bool,
}

/// How a name bound inside a scanned function is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    /// A declaration: its initializer or body is walked where it is written.
    Local,
    /// A function parameter: its value comes from the caller, so calling it
    /// directly (`reject(err)`, `fn(x)`) is assertion-like.
    Param,
    /// A test callback's context parameter (`done`, AVA / `node:test` `t`):
    /// only a bare `done()` and `CONTEXT_SAFE_MEMBERS` are inert.
    Context,
}

enum Name<'c> {
    Bound(Bound),
    Helper,
    Import(&'c TypeScriptImport),
    Enclosing,
    Global,
}

/// Bounds the recursive walk; anything deeper is not established.
const MAX_DEPTH: usize = 256;

/// Walks a test callback, or the file's setup, looking for anything
/// assertion-like.
struct Scan<'c> {
    context: &'c TypeScriptAdmissionContext,
    scopes: Vec<BTreeMap<String, Bound>>,
    /// Scanning a test callback (a nested registration then fails the test)
    /// rather than setup (where other tests' callbacks are skipped).
    in_test: bool,
    depth: usize,
    assertion_like: bool,
}

impl<'c> Scan<'c> {
    fn new(context: &'c TypeScriptAdmissionContext, in_test: bool) -> Self {
        Self {
            context,
            scopes: Vec::new(),
            in_test,
            depth: 0,
            assertion_like: false,
        }
    }

    fn flag(&mut self) {
        self.assertion_like = true;
    }

    fn resolve(&self, name: &str) -> Name<'c> {
        if let Some(bound) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
        {
            return Name::Bound(bound);
        }
        if self.context.helpers.contains(name) {
            return Name::Helper;
        }
        if let Some(import) = self
            .context
            .imports
            .iter()
            .find(|import| import.local == name)
        {
            return Name::Import(import);
        }
        if self.context.enclosing.contains(name) {
            return Name::Enclosing;
        }
        Name::Global
    }

    // ---- registrations -------------------------------------------------

    /// Which registration `call` is, if its callee is an unshadowed runner
    /// `test`/`it`/`describe` chain, with the arguments of any factory call
    /// in the callee (`.each(table)`).
    fn registration<'a, 'b>(
        &self,
        call: &'b oxc_ast::ast::CallExpression<'a>,
    ) -> Option<Registration<'a, 'b>> {
        let (root, factory) = match call.callee.get_inner_expression() {
            // `test.each(table)(...)`, `test.skipIf(cond)(...)`.
            Expression::CallExpression(factory) => {
                let (root, segments) = static_chain(factory.callee.get_inner_expression())?;
                let (last, modifiers) = segments.split_last()?;
                if !REGISTRATION_FACTORIES.contains(last)
                    || !modifiers
                        .iter()
                        .all(|segment| REGISTRATION_MODIFIERS.contains(segment))
                {
                    return None;
                }
                (root, Some((*last, factory.arguments.as_slice())))
            }
            Expression::Identifier(_) => (call.callee.get_inner_expression(), None),
            other => {
                let (root, segments) = static_chain(other)?;
                if !segments
                    .iter()
                    .all(|segment| REGISTRATION_MODIFIERS.contains(segment))
                {
                    return None;
                }
                (root, None)
            }
        };
        let Expression::Identifier(root) = root else {
            return None;
        };
        let kind = match self.resolve(root.name.as_str()) {
            Name::Global => registration_kind(root.name.as_str())?,
            Name::Import(import) if self.context.import_class(import) == ImportClass::Runner => {
                match import.imported.as_deref() {
                    // `import test from 'ava'` / `'node:test'` / `'tape'`.
                    Some("default") | None => RegistrationKind::Test,
                    Some(imported) => registration_kind(imported)?,
                }
            }
            _ => return None,
        };
        let (factory_args, table) = match factory {
            Some((name, arguments)) => (arguments, matches!(name, "each" | "for")),
            None => (&[][..], false),
        };
        Some(Registration {
            kind,
            factory_args,
            table,
        })
    }

    /// The test registered by `call`: its root must be a runner registration,
    /// its non-callback arguments (title, options, `.each` table) are values,
    /// and its callback is walked with its parameters bound.
    fn test_registration(&mut self, call: &oxc_ast::ast::CallExpression<'_>) {
        let Some(Registration {
            kind: RegistrationKind::Test,
            factory_args,
            table,
        }) = self.registration(call)
        else {
            // A custom registration (`const it = base.extend({...})`) runs
            // fixtures RIPR cannot see.
            self.flag();
            return;
        };
        self.arguments(factory_args);
        let callback_index = super::tests_extract::declaration_callback_index(call);
        for (index, argument) in call.arguments.iter().enumerate() {
            if index != callback_index {
                self.argument(argument);
            }
        }
        let width = if table {
            table_row_width(factory_args)
        } else {
            Some(0)
        };
        match call.arguments.get(callback_index) {
            Some(Argument::ArrowFunctionExpression(arrow)) => {
                self.test_callback(&arrow.params, &arrow.body.statements, width);
            }
            Some(Argument::FunctionExpression(function)) => match &function.body {
                Some(body) => self.test_callback(&function.params, &body.statements, width),
                None => self.flag(),
            },
            _ => self.flag(),
        }
    }

    /// `width` is how many leading parameters are table data (`Some(0)` for
    /// an ordinary test, `None` when an `.each` table's row width is not
    /// literal: then only the last parameter is treated as a possible `done`).
    fn test_callback(
        &mut self,
        params: &FormalParameters<'_>,
        statements: &[Statement<'_>],
        width: Option<usize>,
    ) {
        let count = params.items.len();
        let data = width.unwrap_or(count.saturating_sub(1));
        let mut scope = BTreeMap::new();
        for (index, param) in params.items.iter().enumerate() {
            let bound = if index < data {
                Bound::Param
            } else {
                Bound::Context
            };
            match (&param.pattern, bound) {
                (BindingPattern::BindingIdentifier(identifier), _) => {
                    scope.insert(identifier.name.to_string(), bound);
                }
                // A destructured context (`({ expect, task }) =>`) can pull
                // Vitest fixtures, including custom ones that assert.
                (_, Bound::Context) => {
                    self.flag();
                    return;
                }
                (pattern, _) => {
                    let mut names = Vec::new();
                    pattern_names(pattern, &mut names);
                    scope.extend(names.into_iter().map(|name| (name, Bound::Param)));
                }
            }
        }
        if params.rest.is_some() {
            self.flag();
            return;
        }
        self.push_function_scope(scope, statements);
        self.parameter_defaults(params);
        self.statements(statements);
        self.scopes.pop();
    }

    // ---- setup ---------------------------------------------------------

    /// File-level and `describe`-level statements. Their declarations are
    /// the context's helpers and enclosing names, so no scope is pushed;
    /// helper bodies run only when called and are judged where referenced.
    fn setup_statements(&mut self, statements: &[Statement<'_>]) {
        for statement in statements {
            if self.assertion_like {
                return;
            }
            self.setup_statement(statement);
        }
    }

    fn setup_statement(&mut self, statement: &Statement<'_>) {
        match statement {
            Statement::ImportDeclaration(import) => {
                if import.import_kind == ImportOrExportKind::Type {
                    return;
                }
                // A side-effect import runs a module RIPR cannot see; a
                // test-support module's top level can register hooks.
                if import
                    .specifiers
                    .as_ref()
                    .is_none_or(|specifiers| specifiers.is_empty())
                    || classify_source(import.source.value.as_str(), &self.context.test_dir)
                        == ImportClass::TestSupport
                {
                    self.flag();
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(source) = &export.source
                    && classify_source(source.value.as_str(), &self.context.test_dir)
                        != ImportClass::Production
                {
                    self.flag();
                }
                if let Some(declaration) = &export.declaration {
                    self.setup_declaration(declaration);
                }
            }
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(_)
                | ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => {}
                ExportDefaultDeclarationKind::ClassDeclaration(class) => self.setup_class(class),
                other => match other.as_expression() {
                    Some(expression) => self.expr(expression),
                    None => self.flag(),
                },
            },
            Statement::ExportAllDeclaration(_)
            | Statement::TSExportAssignment(_)
            | Statement::TSNamespaceExportDeclaration(_)
            | Statement::WithStatement(_)
            | Statement::ThrowStatement(_) => self.flag(),
            Statement::ExpressionStatement(statement) => {
                self.setup_expression_statement(&statement.expression);
            }
            Statement::BlockStatement(block) => self.setup_statements(&block.body),
            Statement::IfStatement(statement) => {
                self.expr(&statement.test);
                self.setup_statement(&statement.consequent);
                if let Some(alternate) = &statement.alternate {
                    self.setup_statement(alternate);
                }
            }
            Statement::ForStatement(statement) => {
                match &statement.init {
                    Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(declaration)) => {
                        self.setup_variables(declaration);
                    }
                    Some(init) => {
                        if let Some(expression) = init.as_expression() {
                            self.expr(expression);
                        }
                    }
                    None => {}
                }
                self.opt_expr(statement.test.as_ref());
                self.opt_expr(statement.update.as_ref());
                self.setup_statement(&statement.body);
            }
            Statement::ForInStatement(statement) => {
                self.setup_for_left(&statement.left);
                self.expr(&statement.right);
                self.setup_statement(&statement.body);
            }
            Statement::ForOfStatement(statement) => {
                self.setup_for_left(&statement.left);
                self.expr(&statement.right);
                self.setup_statement(&statement.body);
            }
            Statement::WhileStatement(statement) => {
                self.expr(&statement.test);
                self.setup_statement(&statement.body);
            }
            Statement::DoWhileStatement(statement) => {
                self.setup_statement(&statement.body);
                self.expr(&statement.test);
            }
            Statement::TryStatement(statement) => {
                self.setup_statements(&statement.block.body);
                if let Some(handler) = &statement.handler {
                    self.setup_statements(&handler.body.body);
                }
                if let Some(finalizer) = &statement.finalizer {
                    self.setup_statements(&finalizer.body);
                }
            }
            Statement::LabeledStatement(statement) => self.setup_statement(&statement.body),
            Statement::SwitchStatement(statement) => {
                self.expr(&statement.discriminant);
                for case in &statement.cases {
                    self.opt_expr(case.test.as_ref());
                    self.setup_statements(&case.consequent);
                }
            }
            Statement::ReturnStatement(statement) => self.opt_expr(statement.argument.as_ref()),
            Statement::EmptyStatement(_)
            | Statement::DebuggerStatement(_)
            | Statement::BreakStatement(_)
            | Statement::ContinueStatement(_) => {}
            other => match other.as_declaration() {
                Some(declaration) => self.setup_declaration(declaration),
                None => self.flag(),
            },
        }
    }

    fn setup_declaration(&mut self, declaration: &Declaration<'_>) {
        match declaration {
            Declaration::VariableDeclaration(declaration) => self.setup_variables(declaration),
            Declaration::FunctionDeclaration(_)
            | Declaration::TSTypeAliasDeclaration(_)
            | Declaration::TSInterfaceDeclaration(_) => {}
            Declaration::ClassDeclaration(class) => self.setup_class(class),
            Declaration::TSEnumDeclaration(declaration) => {
                for member in &declaration.body.members {
                    self.opt_expr(member.initializer.as_ref());
                }
            }
            Declaration::TSModuleDeclaration(declaration) if declaration.declare => {}
            Declaration::TSGlobalDeclaration(_) => {}
            Declaration::TSModuleDeclaration(_) | Declaration::TSImportEqualsDeclaration(_) => {
                self.flag();
            }
        }
    }

    fn setup_variables(&mut self, declaration: &VariableDeclaration<'_>) {
        for declarator in &declaration.declarations {
            self.pattern(&declarator.id);
            let Some(init) = &declarator.init else {
                continue;
            };
            if is_function_like(init) {
                continue;
            }
            if let Some(source) = require_source(init) {
                if classify_source(source, &self.context.test_dir) == ImportClass::TestSupport {
                    self.flag();
                }
                continue;
            }
            self.expr(init);
        }
    }

    fn setup_for_left(&mut self, left: &oxc_ast::ast::ForStatementLeft<'_>) {
        match left {
            oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    self.pattern(&declarator.id);
                }
            }
            other => match other.as_assignment_target() {
                Some(target) => self.assignment_target(target),
                None => self.flag(),
            },
        }
    }

    /// A setup-level class runs its decorators, `extends` clause, computed
    /// keys, static blocks and static field initializers when declared.
    fn setup_class(&mut self, class: &Class<'_>) {
        if !class.decorators.is_empty() {
            self.flag();
            return;
        }
        self.opt_expr(class.super_class.as_ref());
        for element in &class.body.body {
            match element {
                ClassElement::StaticBlock(_) => self.flag(),
                ClassElement::MethodDefinition(method) => {
                    if !method.decorators.is_empty() {
                        self.flag();
                    }
                    self.computed_key(&method.key, method.computed);
                }
                ClassElement::PropertyDefinition(property) => {
                    if !property.decorators.is_empty() {
                        self.flag();
                    }
                    self.computed_key(&property.key, property.computed);
                    if property.r#static {
                        self.opt_expr(property.value.as_ref());
                    }
                }
                ClassElement::AccessorProperty(property) => {
                    if !property.decorators.is_empty() {
                        self.flag();
                    }
                    self.computed_key(&property.key, property.computed);
                    if property.r#static {
                        self.opt_expr(property.value.as_ref());
                    }
                }
                ClassElement::TSIndexSignature(_) => {}
            }
        }
    }

    /// A setup-level expression statement: a `describe` callback is more
    /// setup, a test callback is another test (only its title, options and
    /// table run now), and anything else (a hook, `expect.extend(...)`,
    /// `vi.mock(...)`) is walked as code that runs for every test.
    fn setup_expression_statement(&mut self, expression: &Expression<'_>) {
        let Expression::CallExpression(call) = expression.get_inner_expression() else {
            self.expr(expression);
            return;
        };
        match self.registration(call) {
            Some(registration) => self.setup_registration(call, &registration),
            None => {
                // `cases.forEach((c) => { it(c.name, ...) })` registers tests
                // from a callback; its body is setup too.
                if let Some(body) = for_each_callback_body(call) {
                    self.expr(&call.callee);
                    self.setup_statements(body);
                } else {
                    self.call(call);
                }
            }
        }
    }

    /// A registration met while scanning setup: a `describe` callback is
    /// more setup; of another test only its title, options and table run
    /// now.
    fn setup_registration(
        &mut self,
        call: &oxc_ast::ast::CallExpression<'_>,
        registration: &Registration<'_, '_>,
    ) {
        self.arguments(registration.factory_args);
        for argument in &call.arguments {
            match (registration.kind, argument) {
                (RegistrationKind::Describe, Argument::ArrowFunctionExpression(arrow)) => {
                    self.parameter_defaults(&arrow.params);
                    self.setup_statements(&arrow.body.statements);
                }
                (RegistrationKind::Describe, Argument::FunctionExpression(function)) => {
                    self.parameter_defaults(&function.params);
                    if let Some(body) = &function.body {
                        self.setup_statements(&body.statements);
                    }
                }
                (
                    RegistrationKind::Test,
                    Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_),
                ) => {}
                (_, other) => self.argument(other),
            }
        }
    }

    // ---- scopes --------------------------------------------------------

    /// Push a function scope: `params` plus every `var` in the body and the
    /// body's own lexical declarations.
    fn push_function_scope(
        &mut self,
        mut scope: BTreeMap<String, Bound>,
        statements: &[Statement<'_>],
    ) {
        let mut names = Vec::new();
        var_names(statements, &mut names);
        lexical_names(statements, &mut names);
        for name in names {
            scope.entry(name).or_insert(Bound::Local);
        }
        self.scopes.push(scope);
    }

    fn push_block_scope(&mut self, statements: &[Statement<'_>]) {
        let mut names = Vec::new();
        lexical_names(statements, &mut names);
        self.scopes
            .push(names.into_iter().map(|name| (name, Bound::Local)).collect());
    }

    fn function(&mut self, params: &FormalParameters<'_>, statements: &[Statement<'_>]) {
        let mut names = Vec::new();
        for param in &params.items {
            pattern_names(&param.pattern, &mut names);
        }
        if let Some(rest) = &params.rest {
            pattern_names(&rest.rest.argument, &mut names);
        }
        let scope = names.into_iter().map(|name| (name, Bound::Param)).collect();
        self.push_function_scope(scope, statements);
        self.parameter_defaults(params);
        self.statements(statements);
        self.scopes.pop();
    }

    fn parameter_defaults(&mut self, params: &FormalParameters<'_>) {
        for param in &params.items {
            if !param.decorators.is_empty() {
                self.flag();
            }
            self.pattern(&param.pattern);
            if let Some(initializer) = &param.initializer {
                self.expr(initializer);
            }
        }
        if let Some(rest) = &params.rest {
            self.pattern(&rest.rest.argument);
        }
    }

    // ---- statements ----------------------------------------------------

    fn statements(&mut self, statements: &[Statement<'_>]) {
        for statement in statements {
            if self.assertion_like {
                return;
            }
            self.statement(statement);
        }
    }

    fn block(&mut self, statements: &[Statement<'_>]) {
        self.push_block_scope(statements);
        self.statements(statements);
        self.scopes.pop();
    }

    fn statement(&mut self, statement: &Statement<'_>) {
        if self.assertion_like {
            return;
        }
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.flag();
        } else {
            self.statement_inner(statement);
        }
        self.depth -= 1;
    }

    fn statement_inner(&mut self, statement: &Statement<'_>) {
        match statement {
            // A `throw` fails the test exactly like an assertion.
            Statement::ThrowStatement(_) | Statement::WithStatement(_) => self.flag(),
            Statement::BlockStatement(block) => self.block(&block.body),
            Statement::ExpressionStatement(statement) => self.expr(&statement.expression),
            Statement::IfStatement(statement) => {
                self.expr(&statement.test);
                self.statement(&statement.consequent);
                if let Some(alternate) = &statement.alternate {
                    self.statement(alternate);
                }
            }
            Statement::ForStatement(statement) => {
                let mut names = Vec::new();
                if let Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(declaration)) =
                    &statement.init
                {
                    declaration_names(declaration, &mut names);
                }
                self.scopes
                    .push(names.into_iter().map(|name| (name, Bound::Local)).collect());
                match &statement.init {
                    Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(declaration)) => {
                        self.variables(declaration);
                    }
                    Some(init) => {
                        if let Some(expression) = init.as_expression() {
                            self.expr(expression);
                        }
                    }
                    None => {}
                }
                self.opt_expr(statement.test.as_ref());
                self.opt_expr(statement.update.as_ref());
                self.statement(&statement.body);
                self.scopes.pop();
            }
            Statement::ForInStatement(statement) => {
                self.for_each_loop(&statement.left, &statement.right, &statement.body);
            }
            Statement::ForOfStatement(statement) => {
                self.for_each_loop(&statement.left, &statement.right, &statement.body);
            }
            Statement::WhileStatement(statement) => {
                self.expr(&statement.test);
                self.statement(&statement.body);
            }
            Statement::DoWhileStatement(statement) => {
                self.statement(&statement.body);
                self.expr(&statement.test);
            }
            Statement::ReturnStatement(statement) => self.opt_expr(statement.argument.as_ref()),
            Statement::LabeledStatement(statement) => self.statement(&statement.body),
            Statement::SwitchStatement(statement) => {
                self.expr(&statement.discriminant);
                let mut names = Vec::new();
                for case in &statement.cases {
                    lexical_names(&case.consequent, &mut names);
                }
                self.scopes
                    .push(names.into_iter().map(|name| (name, Bound::Local)).collect());
                for case in &statement.cases {
                    self.opt_expr(case.test.as_ref());
                    self.statements(&case.consequent);
                }
                self.scopes.pop();
            }
            Statement::TryStatement(statement) => {
                self.block(&statement.block.body);
                if let Some(handler) = &statement.handler {
                    let mut names = Vec::new();
                    if let Some(param) = &handler.param {
                        pattern_names(&param.pattern, &mut names);
                    }
                    self.scopes
                        .push(names.into_iter().map(|name| (name, Bound::Local)).collect());
                    if let Some(param) = &handler.param {
                        self.pattern(&param.pattern);
                    }
                    self.block(&handler.body.body);
                    self.scopes.pop();
                }
                if let Some(finalizer) = &statement.finalizer {
                    self.block(&finalizer.body);
                }
            }
            Statement::EmptyStatement(_)
            | Statement::DebuggerStatement(_)
            | Statement::BreakStatement(_)
            | Statement::ContinueStatement(_) => {}
            other => match other.as_declaration() {
                Some(declaration) => self.declaration(declaration),
                None => self.flag(),
            },
        }
    }

    fn for_each_loop(
        &mut self,
        left: &oxc_ast::ast::ForStatementLeft<'_>,
        right: &Expression<'_>,
        body: &Statement<'_>,
    ) {
        self.expr(right);
        let mut names = Vec::new();
        if let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) = left {
            declaration_names(declaration, &mut names);
        }
        self.scopes
            .push(names.into_iter().map(|name| (name, Bound::Local)).collect());
        match left {
            oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    self.pattern(&declarator.id);
                }
            }
            other => match other.as_assignment_target() {
                Some(target) => self.assignment_target(target),
                None => self.flag(),
            },
        }
        self.statement(body);
        self.scopes.pop();
    }

    fn declaration(&mut self, declaration: &Declaration<'_>) {
        match declaration {
            Declaration::VariableDeclaration(declaration) => self.variables(declaration),
            Declaration::FunctionDeclaration(function) => self.function_value(function),
            Declaration::ClassDeclaration(class) => self.class(class),
            Declaration::TSTypeAliasDeclaration(_) | Declaration::TSInterfaceDeclaration(_) => {}
            Declaration::TSEnumDeclaration(declaration) => {
                for member in &declaration.body.members {
                    self.opt_expr(member.initializer.as_ref());
                }
            }
            _ => self.flag(),
        }
    }

    fn variables(&mut self, declaration: &VariableDeclaration<'_>) {
        for declarator in &declaration.declarations {
            self.pattern(&declarator.id);
            self.opt_expr(declarator.init.as_ref());
        }
    }

    fn function_value(&mut self, function: &Function<'_>) {
        let Some(body) = &function.body else {
            return;
        };
        // A named function expression binds its own name inside.
        let mut scope = BTreeMap::new();
        if let Some(id) = &function.id {
            scope.insert(id.name.to_string(), Bound::Local);
        }
        self.scopes.push(scope);
        self.function(&function.params, &body.statements);
        self.scopes.pop();
    }

    fn class(&mut self, class: &Class<'_>) {
        if !class.decorators.is_empty() {
            self.flag();
            return;
        }
        self.opt_expr(class.super_class.as_ref());
        let mut scope = BTreeMap::new();
        if let Some(id) = &class.id {
            scope.insert(id.name.to_string(), Bound::Local);
        }
        self.scopes.push(scope);
        for element in &class.body.body {
            match element {
                ClassElement::StaticBlock(block) => {
                    self.push_function_scope(BTreeMap::new(), &block.body);
                    self.statements(&block.body);
                    self.scopes.pop();
                }
                ClassElement::MethodDefinition(method) => {
                    if !method.decorators.is_empty() {
                        self.flag();
                    }
                    self.computed_key(&method.key, method.computed);
                    self.function_value(&method.value);
                }
                ClassElement::PropertyDefinition(property) => {
                    if !property.decorators.is_empty() {
                        self.flag();
                    }
                    self.computed_key(&property.key, property.computed);
                    self.opt_expr(property.value.as_ref());
                }
                ClassElement::AccessorProperty(property) => {
                    if !property.decorators.is_empty() {
                        self.flag();
                    }
                    self.computed_key(&property.key, property.computed);
                    self.opt_expr(property.value.as_ref());
                }
                ClassElement::TSIndexSignature(_) => {}
            }
        }
        self.scopes.pop();
    }

    fn computed_key(&mut self, key: &PropertyKey<'_>, computed: bool) {
        if computed && let Some(expression) = key.as_expression() {
            self.expr(expression);
        }
    }

    /// Defaults and computed keys inside a binding pattern.
    fn pattern(&mut self, pattern: &BindingPattern<'_>) {
        match pattern {
            BindingPattern::BindingIdentifier(_) => {}
            BindingPattern::ObjectPattern(object) => {
                for property in &object.properties {
                    self.computed_key(&property.key, property.computed);
                    self.pattern(&property.value);
                }
                if let Some(rest) = &object.rest {
                    self.pattern(&rest.argument);
                }
            }
            BindingPattern::ArrayPattern(array) => {
                for element in array.elements.iter().flatten() {
                    self.pattern(element);
                }
                if let Some(rest) = &array.rest {
                    self.pattern(&rest.argument);
                }
            }
            BindingPattern::AssignmentPattern(assignment) => {
                self.pattern(&assignment.left);
                self.expr(&assignment.right);
            }
        }
    }

    // ---- expressions ---------------------------------------------------

    fn opt_expr(&mut self, expression: Option<&Expression<'_>>) {
        if let Some(expression) = expression {
            self.expr(expression);
        }
    }

    fn arguments(&mut self, arguments: &[Argument<'_>]) {
        for argument in arguments {
            self.argument(argument);
        }
    }

    fn argument(&mut self, argument: &Argument<'_>) {
        match argument {
            Argument::SpreadElement(spread) => self.expr(&spread.argument),
            other => match other.as_expression() {
                Some(expression) => self.expr(expression),
                None => self.flag(),
            },
        }
    }

    fn expr(&mut self, expression: &Expression<'_>) {
        if self.assertion_like {
            return;
        }
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.flag();
        } else {
            self.expr_inner(expression);
        }
        self.depth -= 1;
    }

    fn expr_inner(&mut self, expression: &Expression<'_>) {
        match expression {
            Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::MetaProperty(_) => {}
            Expression::TemplateLiteral(template) => {
                for expression in &template.expressions {
                    self.expr(expression);
                }
            }
            Expression::Identifier(identifier) => self.value_name(identifier.name.as_str()),
            Expression::ArrayExpression(array) => {
                for element in &array.elements {
                    match element {
                        oxc_ast::ast::ArrayExpressionElement::SpreadElement(spread) => {
                            self.expr(&spread.argument);
                        }
                        oxc_ast::ast::ArrayExpressionElement::Elision(_) => {}
                        other => match other.as_expression() {
                            Some(expression) => self.expr(expression),
                            None => self.flag(),
                        },
                    }
                }
            }
            Expression::ObjectExpression(object) => {
                for property in &object.properties {
                    match property {
                        ObjectPropertyKind::ObjectProperty(property) => {
                            self.computed_key(&property.key, property.computed);
                            self.expr(&property.value);
                        }
                        ObjectPropertyKind::SpreadProperty(spread) => self.expr(&spread.argument),
                    }
                }
            }
            Expression::ArrowFunctionExpression(arrow) => {
                self.function(&arrow.params, &arrow.body.statements);
            }
            Expression::FunctionExpression(function) => self.function_value(function),
            Expression::ClassExpression(class) => self.class(class),
            Expression::AssignmentExpression(assignment) => {
                self.assignment_target(&assignment.left);
                self.expr(&assignment.right);
            }
            Expression::AwaitExpression(await_expression) => self.expr(&await_expression.argument),
            Expression::BinaryExpression(binary) => {
                self.expr(&binary.left);
                self.expr(&binary.right);
            }
            Expression::LogicalExpression(logical) => {
                self.expr(&logical.left);
                self.expr(&logical.right);
            }
            Expression::PrivateInExpression(private_in) => self.expr(&private_in.right),
            Expression::ConditionalExpression(conditional) => {
                self.expr(&conditional.test);
                self.expr(&conditional.consequent);
                self.expr(&conditional.alternate);
            }
            Expression::SequenceExpression(sequence) => {
                for expression in &sequence.expressions {
                    self.expr(expression);
                }
            }
            Expression::UnaryExpression(unary) => self.expr(&unary.argument),
            Expression::UpdateExpression(update) => self.simple_target(&update.argument),
            Expression::YieldExpression(yield_expression) => {
                self.opt_expr(yield_expression.argument.as_ref());
            }
            Expression::ParenthesizedExpression(parenthesized) => {
                self.expr(&parenthesized.expression);
            }
            Expression::TSAsExpression(inner) => self.expr(&inner.expression),
            Expression::TSSatisfiesExpression(inner) => self.expr(&inner.expression),
            Expression::TSTypeAssertion(inner) => self.expr(&inner.expression),
            Expression::TSNonNullExpression(inner) => self.expr(&inner.expression),
            Expression::TSInstantiationExpression(inner) => self.expr(&inner.expression),
            Expression::CallExpression(call) => self.call(call),
            Expression::NewExpression(new) => self.new_expression(new),
            Expression::TaggedTemplateExpression(tagged) => {
                if !self.callee_is_inert(tagged.tag.get_inner_expression(), 1) {
                    self.flag();
                    return;
                }
                self.callee_parts(tagged.tag.get_inner_expression());
                for expression in &tagged.quasi.expressions {
                    self.expr(expression);
                }
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                oxc_ast::ast::ChainElement::CallExpression(call) => self.call(call),
                oxc_ast::ast::ChainElement::TSNonNullExpression(inner) => {
                    self.expr(&inner.expression);
                }
                other => match other.as_member_expression() {
                    Some(member) => self.member_value(member),
                    None => self.flag(),
                },
            },
            Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::PrivateFieldExpression(_) => match expression.as_member_expression() {
                Some(member) => self.member_value(member),
                None => self.flag(),
            },
            // `this` as a value reaches a mocha context or an instance;
            // `import()`, JSX, `super` and V8 intrinsics run code RIPR does
            // not model.
            Expression::ThisExpression(_)
            | Expression::Super(_)
            | Expression::ImportExpression(_)
            | Expression::JSXElement(_)
            | Expression::JSXFragment(_)
            | Expression::V8IntrinsicExpression(_) => self.flag(),
        }
    }

    /// A name used as a value, not called here.
    fn value_name(&mut self, name: &str) {
        let inert = match self.resolve(name) {
            Name::Bound(Bound::Local | Bound::Param) | Name::Enclosing => true,
            // Passing `done` / `t` on lets the receiver fail the test.
            Name::Bound(Bound::Context) | Name::Helper => false,
            Name::Import(import) => matches!(
                self.context.import_class(import),
                ImportClass::Production | ImportClass::NodeBuiltin
            ),
            Name::Global => {
                SAFE_GLOBAL_VALUES.contains(&name)
                    || SAFE_GLOBAL_FUNCTIONS.contains(&name)
                    || (SAFE_GLOBAL_OBJECTS.contains(&name) && name != "process")
                    || is_error_constructor(name)
            }
        };
        if !inert {
            self.flag();
        }
    }

    /// A member read (or the target of a write), not called here.
    fn member_value(&mut self, member: &oxc_ast::ast::MemberExpression<'_>) {
        match member {
            oxc_ast::ast::MemberExpression::StaticMemberExpression(first) => {
                let (root, segments) = static_member_chain(first);
                match root {
                    Expression::Identifier(identifier) => {
                        if !self.member_path_inert(identifier.name.as_str(), &segments, false) {
                            self.flag();
                        }
                    }
                    Expression::ThisExpression(_) | Expression::Super(_) => self.flag(),
                    // A member of a computed value acts on that value, which
                    // is walked; an assertion-like path (`x.should`) is not
                    // inert.
                    other => {
                        if segments
                            .iter()
                            .any(|segment| has_assertion_like_prefix(segment))
                        {
                            self.flag();
                            return;
                        }
                        self.expr(other);
                    }
                }
            }
            oxc_ast::ast::MemberExpression::ComputedMemberExpression(computed) => {
                self.expr(&computed.object);
                self.expr(&computed.expression);
            }
            oxc_ast::ast::MemberExpression::PrivateFieldExpression(private) => {
                self.expr(&private.object);
            }
        }
    }

    /// Whether `root.segments...` is inert, as a callee (`called`) or a
    /// value.
    fn member_path_inert(&self, root: &str, segments: &[&str], called: bool) -> bool {
        match self.resolve(root) {
            Name::Bound(Bound::Local | Bound::Param) | Name::Enclosing => !segments
                .iter()
                .any(|segment| has_assertion_like_prefix(segment)),
            Name::Bound(Bound::Context) => {
                called && CONTEXT_SAFE_MEMBERS.contains(&segments.join(".").as_str())
            }
            Name::Helper => false,
            Name::Import(import) => match self.context.import_class(import) {
                // A production owner's member is inert whatever its name.
                ImportClass::Production | ImportClass::NodeBuiltin => true,
                ImportClass::Runner => called && runner_member_inert(import, segments),
                ImportClass::TestSupport | ImportClass::Package => false,
            },
            Name::Global => {
                if matches!(root, "vi" | "jest") {
                    return called
                        && segments.len() == 1
                        && RUNNER_OBJECT_SAFE_MEMBERS.contains(&segments[0]);
                }
                SAFE_GLOBAL_OBJECTS.contains(&root)
                    && segments
                        .first()
                        .is_none_or(|member| !denied_global_member(root, member))
            }
        }
    }

    fn assignment_target(&mut self, target: &oxc_ast::ast::AssignmentTarget<'_>) {
        if let Some(simple) = target.as_simple_assignment_target() {
            self.simple_target(simple);
            return;
        }
        match target.as_assignment_target_pattern() {
            Some(oxc_ast::ast::AssignmentTargetPattern::ArrayAssignmentTarget(array)) => {
                for element in array.elements.iter().flatten() {
                    self.maybe_default_target(element);
                }
                if let Some(rest) = &array.rest {
                    self.assignment_target(&rest.target);
                }
            }
            Some(oxc_ast::ast::AssignmentTargetPattern::ObjectAssignmentTarget(object)) => {
                for property in &object.properties {
                    match property {
                        oxc_ast::ast::AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(
                            property,
                        ) => {
                            self.target_name(property.binding.name.as_str());
                            self.opt_expr(property.init.as_ref());
                        }
                        oxc_ast::ast::AssignmentTargetProperty::AssignmentTargetPropertyProperty(
                            property,
                        ) => {
                            self.computed_key(&property.name, property.computed);
                            self.maybe_default_target(&property.binding);
                        }
                    }
                }
                if let Some(rest) = &object.rest {
                    self.assignment_target(&rest.target);
                }
            }
            None => self.flag(),
        }
    }

    fn maybe_default_target(&mut self, target: &oxc_ast::ast::AssignmentTargetMaybeDefault<'_>) {
        match target {
            oxc_ast::ast::AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(default) => {
                self.assignment_target(&default.binding);
                self.expr(&default.init);
            }
            other => match other.as_assignment_target() {
                Some(target) => self.assignment_target(target),
                None => self.flag(),
            },
        }
    }

    fn simple_target(&mut self, target: &oxc_ast::ast::SimpleAssignmentTarget<'_>) {
        match target {
            oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier) => {
                self.target_name(identifier.name.as_str());
            }
            oxc_ast::ast::SimpleAssignmentTarget::TSAsExpression(inner) => {
                self.expr(&inner.expression);
            }
            oxc_ast::ast::SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
                self.expr(&inner.expression);
            }
            oxc_ast::ast::SimpleAssignmentTarget::TSNonNullExpression(inner) => {
                self.expr(&inner.expression);
            }
            oxc_ast::ast::SimpleAssignmentTarget::TSTypeAssertion(inner) => {
                self.expr(&inner.expression);
            }
            other => match other.as_member_expression() {
                Some(member) => self.member_value(member),
                None => self.flag(),
            },
        }
    }

    /// A plain name written to. Only a local, a parameter or an enclosing
    /// binding may be written; anything else installs a global or rebinds a
    /// helper.
    fn target_name(&mut self, name: &str) {
        if !matches!(
            self.resolve(name),
            Name::Bound(Bound::Local | Bound::Param) | Name::Enclosing
        ) {
            self.flag();
        }
    }

    fn call(&mut self, call: &oxc_ast::ast::CallExpression<'_>) {
        if let Some(registration) = self.registration(call) {
            // Inside a test body a nested registration throws at runtime; in
            // setup, another test's callback is not this test's code.
            if self.in_test {
                self.flag();
            } else {
                self.setup_registration(call, &registration);
            }
            return;
        }
        let callee = call.callee.get_inner_expression();
        if !self.callee_is_inert(callee, call.arguments.len()) {
            self.flag();
            return;
        }
        self.callee_parts(callee);
        if !self.is_hook(callee) {
            self.arguments(&call.arguments);
            return;
        }
        // A hook callback receives the same context a test does (`done`,
        // AVA's `t`), so its parameters are bound like a test's.
        for argument in &call.arguments {
            match argument {
                Argument::ArrowFunctionExpression(arrow) => {
                    self.test_callback(&arrow.params, &arrow.body.statements, Some(0));
                }
                Argument::FunctionExpression(function) => match &function.body {
                    Some(body) => self.test_callback(&function.params, &body.statements, Some(0)),
                    None => self.flag(),
                },
                other => self.argument(other),
            }
        }
    }

    /// `beforeEach` / mocha `before` / AVA `test.beforeEach` /
    /// `test.after.always`, unshadowed or from a runner module.
    fn is_hook(&self, callee: &Expression<'_>) -> bool {
        match callee {
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                match self.resolve(name) {
                    Name::Global => HOOKS.contains(&name),
                    Name::Import(import) => {
                        self.context.import_class(import) == ImportClass::Runner
                            && import
                                .imported
                                .as_deref()
                                .is_some_and(|imported| HOOKS.contains(&imported))
                    }
                    _ => false,
                }
            }
            Expression::StaticMemberExpression(member) => {
                let (root, segments) = static_member_chain(member);
                let Expression::Identifier(root) = root else {
                    return false;
                };
                segments.len() <= 2
                    && segments.first().is_some_and(|first| HOOKS.contains(first))
                    && segments.get(1).is_none_or(|second| *second == "always")
                    && self.is_registration_root(root.name.as_str())
            }
            _ => false,
        }
    }

    fn new_expression(&mut self, new: &oxc_ast::ast::NewExpression<'_>) {
        let callee = new.callee.get_inner_expression();
        // A promise executor's `reject` parameter fails an awaited test
        // wherever it ends up; only a resolve-only executor is inert.
        if let Expression::Identifier(identifier) = callee
            && identifier.name == "Promise"
            && matches!(self.resolve("Promise"), Name::Global)
            && new
                .arguments
                .first()
                .is_some_and(|argument| match argument {
                    Argument::ArrowFunctionExpression(arrow) => {
                        arrow.params.items.len() > 1 || arrow.params.rest.is_some()
                    }
                    Argument::FunctionExpression(function) => {
                        function.params.items.len() > 1 || function.params.rest.is_some()
                    }
                    _ => true,
                })
        {
            self.flag();
            return;
        }
        let inert = match callee {
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                match self.resolve(name) {
                    Name::Bound(Bound::Local) => true,
                    Name::Bound(Bound::Param | Bound::Context) | Name::Helper | Name::Enclosing => {
                        false
                    }
                    Name::Import(import) => matches!(
                        self.context.import_class(import),
                        ImportClass::Production | ImportClass::NodeBuiltin
                    ),
                    Name::Global => {
                        (SAFE_GLOBAL_OBJECTS.contains(&name) && name != "process")
                            || SAFE_GLOBAL_FUNCTIONS.contains(&name)
                            || is_error_constructor(name)
                    }
                }
            }
            other => self.callee_is_inert(other, new.arguments.len()),
        };
        if !inert {
            self.flag();
            return;
        }
        if !matches!(callee, Expression::Identifier(_)) {
            self.callee_parts(callee);
        }
        self.arguments(&new.arguments);
    }

    /// Whether calling `callee` with `argument_count` arguments is inert.
    /// The callee's own subexpressions are walked by `callee_parts`.
    fn callee_is_inert(&self, callee: &Expression<'_>, argument_count: usize) -> bool {
        match callee {
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                match self.resolve(name) {
                    Name::Bound(Bound::Local) => true,
                    // `done()` ends the test; `done(err)` fails it.
                    Name::Bound(Bound::Context) => argument_count == 0,
                    Name::Bound(Bound::Param) | Name::Helper | Name::Enclosing => false,
                    Name::Import(import) => match self.context.import_class(import) {
                        ImportClass::Production | ImportClass::NodeBuiltin => true,
                        ImportClass::Runner => import
                            .imported
                            .as_deref()
                            .is_some_and(|imported| HOOKS.contains(&imported)),
                        ImportClass::TestSupport | ImportClass::Package => false,
                    },
                    Name::Global => {
                        SAFE_GLOBAL_FUNCTIONS.contains(&name)
                            || HOOKS.contains(&name)
                            || is_error_constructor(name)
                    }
                }
            }
            // An immediately invoked function is walked like any other.
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => true,
            Expression::StaticMemberExpression(member) => {
                let (root, segments) = static_member_chain(member);
                match root {
                    Expression::Identifier(identifier) => {
                        // AVA hooks hang off the registration root
                        // (`test.beforeEach(t => ...)`).
                        self.is_hook(callee)
                            || self.member_path_inert(identifier.name.as_str(), &segments, true)
                    }
                    // Mocha's `this.timeout(5000)`.
                    Expression::ThisExpression(_) => {
                        matches!(segments.as_slice(), ["timeout" | "slow" | "retries"])
                    }
                    // A method reached through a computed member, a private
                    // field or `super` can be any callable.
                    Expression::ComputedMemberExpression(_)
                    | Expression::PrivateFieldExpression(_)
                    | Expression::Super(_) => false,
                    // A method on a computed value (`make().run()`,
                    // `[1, 2].map(f)`): the receiver is walked separately.
                    _ => !segments
                        .iter()
                        .any(|segment| has_assertion_like_prefix(segment)),
                }
            }
            // `handlers[kind]()`, `this.#run()`, `make()()`, and the rest.
            _ => false,
        }
    }

    /// Walk the receiver of a member callee whose root is a computed value.
    fn callee_parts(&mut self, callee: &Expression<'_>) {
        match callee {
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                self.expr(callee);
            }
            Expression::StaticMemberExpression(member) => {
                let (root, _) = static_member_chain(member);
                if !matches!(
                    root,
                    Expression::Identifier(_) | Expression::ThisExpression(_)
                ) {
                    self.expr(root);
                }
            }
            _ => {}
        }
    }

    fn is_registration_root(&self, name: &str) -> bool {
        match self.resolve(name) {
            Name::Global => registration_kind(name) == Some(RegistrationKind::Test),
            Name::Import(import) => self.context.import_class(import) == ImportClass::Runner,
            _ => false,
        }
    }
}

fn registration_kind(name: &str) -> Option<RegistrationKind> {
    if TEST_ROOTS.contains(&name) {
        Some(RegistrationKind::Test)
    } else if DESCRIBE_ROOTS.contains(&name) {
        Some(RegistrationKind::Describe)
    } else {
        None
    }
}

/// Whether a runner-module import's member chain is a mock or timer control:
/// `vi.fn()`, `jest.spyOn(...)`, `node:test`'s `mock.fn()`.
fn runner_member_inert(import: &TypeScriptImport, segments: &[&str]) -> bool {
    let Some(first) = segments.first() else {
        return false;
    };
    match import.imported.as_deref() {
        Some("vi" | "jest") => segments.len() == 1 && RUNNER_OBJECT_SAFE_MEMBERS.contains(first),
        Some("mock") if import.source == "node:test" => {
            segments.len() == 1 && NODE_MOCK_SAFE_MEMBERS.contains(first)
        }
        _ => false,
    }
}

fn is_error_constructor(name: &str) -> bool {
    name.ends_with("Error") && SAFE_ERROR_CONSTRUCTORS.contains(&name)
}

const SAFE_ERROR_CONSTRUCTORS: &[&str] = &[
    "AggregateError",
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
];

/// The root and member names of a plain static member chain (`a.b.c`), or
/// `None` when `expression` is not a static member expression.
fn static_chain<'b, 'a>(
    expression: &'b Expression<'a>,
) -> Option<(&'b Expression<'a>, Vec<&'b str>)> {
    let Expression::StaticMemberExpression(_) = expression else {
        return None;
    };
    let mut segments = Vec::new();
    let mut current = expression;
    while let Expression::StaticMemberExpression(member) = current {
        segments.push(member.property.name.as_str());
        current = member.object.get_inner_expression();
    }
    segments.reverse();
    Some((current, segments))
}

fn static_member_chain<'b, 'a>(
    first: &'b oxc_ast::ast::StaticMemberExpression<'a>,
) -> (&'b Expression<'a>, Vec<&'b str>) {
    let mut segments = vec![first.property.name.as_str()];
    let mut current = first.object.get_inner_expression();
    while let Expression::StaticMemberExpression(next) = current {
        segments.push(next.property.name.as_str());
        current = next.object.get_inner_expression();
    }
    segments.reverse();
    (current, segments)
}

/// How many leading callback parameters an `.each` table fills: the common
/// length of literal array rows, `1` for literal non-array rows, `None` when
/// the table is not a literal array.
fn table_row_width(factory_args: &[Argument<'_>]) -> Option<usize> {
    let [Argument::ArrayExpression(table)] = factory_args else {
        return None;
    };
    let mut width = None;
    for element in &table.elements {
        let row = match element {
            oxc_ast::ast::ArrayExpressionElement::ArrayExpression(row) => {
                if row.elements.iter().any(|element| {
                    matches!(
                        element,
                        oxc_ast::ast::ArrayExpressionElement::SpreadElement(_)
                    )
                }) {
                    return None;
                }
                row.elements.len()
            }
            oxc_ast::ast::ArrayExpressionElement::SpreadElement(_)
            | oxc_ast::ast::ArrayExpressionElement::Elision(_) => return None,
            _ => 1,
        };
        match width {
            None => width = Some(row),
            Some(existing) if existing == row => {}
            Some(_) => return None,
        }
    }
    width
}

fn is_function_like(expression: &Expression<'_>) -> bool {
    matches!(
        expression.get_inner_expression(),
        Expression::ArrowFunctionExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::ClassExpression(_)
    )
}

/// `require("<literal>")`.
fn require_source<'a>(expression: &'a Expression<'_>) -> Option<&'a str> {
    let Expression::CallExpression(call) = expression.get_inner_expression() else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name != "require" || call.arguments.len() != 1 {
        return None;
    }
    match call.arguments.first() {
        Some(Argument::StringLiteral(literal)) => Some(literal.value.as_str()),
        _ => None,
    }
}

/// The body of a setup-level `<receiver>.forEach(callback)` call.
fn for_each_callback_body<'b, 'a>(
    call: &'b oxc_ast::ast::CallExpression<'a>,
) -> Option<&'b [Statement<'a>]> {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
        return None;
    };
    if member.property.name != "forEach" || call.arguments.len() != 1 {
        return None;
    }
    match call.arguments.first() {
        Some(Argument::ArrowFunctionExpression(arrow)) => Some(&arrow.body.statements),
        Some(Argument::FunctionExpression(function)) => function
            .body
            .as_ref()
            .map(|body| body.statements.as_slice()),
        _ => None,
    }
}

/// Names bound outside test callbacks, split into helpers (functions and
/// classes) and other enclosing bindings.
fn collect_setup_names(
    statements: &[Statement<'_>],
    helpers: &mut BTreeSet<String>,
    enclosing: &mut BTreeSet<String>,
) {
    for statement in statements {
        collect_setup_statement_names(statement, helpers, enclosing);
    }
}

fn collect_setup_statement_names(
    statement: &Statement<'_>,
    helpers: &mut BTreeSet<String>,
    enclosing: &mut BTreeSet<String>,
) {
    match statement {
        Statement::ExportNamedDeclaration(export) => {
            if let Some(declaration) = &export.declaration {
                collect_declaration_names(declaration, helpers, enclosing);
            }
        }
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    helpers.insert(id.name.to_string());
                }
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    helpers.insert(id.name.to_string());
                }
            }
            _ => {}
        },
        Statement::ExpressionStatement(statement) => {
            let Expression::CallExpression(call) = statement.expression.get_inner_expression()
            else {
                return;
            };
            // A describe-like callback (or a `forEach` that registers tests)
            // binds names for every test inside. Shadowing is ignored here,
            // which only adds names.
            let describe_like = static_chain(call.callee.get_inner_expression())
                .or_else(|| match call.callee.get_inner_expression() {
                    Expression::CallExpression(factory) => {
                        static_chain(factory.callee.get_inner_expression())
                    }
                    _ => None,
                })
                .is_some_and(|(root, _)| {
                    matches!(root, Expression::Identifier(identifier)
                        if DESCRIBE_ROOTS.contains(&identifier.name.as_str()))
                })
                || matches!(call.callee.get_inner_expression(), Expression::Identifier(identifier)
                    if DESCRIBE_ROOTS.contains(&identifier.name.as_str()))
                || for_each_callback_body(call).is_some();
            if !describe_like {
                return;
            }
            for argument in &call.arguments {
                let (params, body) = match argument {
                    Argument::ArrowFunctionExpression(arrow) => {
                        (&arrow.params, Some(&arrow.body.statements))
                    }
                    Argument::FunctionExpression(function) => (
                        &function.params,
                        function.body.as_ref().map(|body| &body.statements),
                    ),
                    _ => continue,
                };
                let mut names = Vec::new();
                for param in &params.items {
                    pattern_names(&param.pattern, &mut names);
                }
                enclosing.extend(names);
                if let Some(body) = body {
                    collect_setup_names(body, helpers, enclosing);
                }
            }
        }
        Statement::BlockStatement(block) => collect_setup_names(&block.body, helpers, enclosing),
        Statement::IfStatement(statement) => {
            collect_setup_statement_names(&statement.consequent, helpers, enclosing);
            if let Some(alternate) = &statement.alternate {
                collect_setup_statement_names(alternate, helpers, enclosing);
            }
        }
        Statement::ForStatement(statement) => {
            if let Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(declaration)) =
                &statement.init
            {
                collect_variable_names(declaration, helpers, enclosing);
            }
            collect_setup_statement_names(&statement.body, helpers, enclosing);
        }
        Statement::ForInStatement(statement) => {
            if let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) =
                &statement.left
            {
                collect_variable_names(declaration, helpers, enclosing);
            }
            collect_setup_statement_names(&statement.body, helpers, enclosing);
        }
        Statement::ForOfStatement(statement) => {
            if let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) =
                &statement.left
            {
                collect_variable_names(declaration, helpers, enclosing);
            }
            collect_setup_statement_names(&statement.body, helpers, enclosing);
        }
        Statement::WhileStatement(statement) => {
            collect_setup_statement_names(&statement.body, helpers, enclosing);
        }
        Statement::DoWhileStatement(statement) => {
            collect_setup_statement_names(&statement.body, helpers, enclosing);
        }
        Statement::LabeledStatement(statement) => {
            collect_setup_statement_names(&statement.body, helpers, enclosing);
        }
        Statement::TryStatement(statement) => {
            collect_setup_names(&statement.block.body, helpers, enclosing);
            if let Some(handler) = &statement.handler {
                if let Some(param) = &handler.param {
                    let mut names = Vec::new();
                    pattern_names(&param.pattern, &mut names);
                    enclosing.extend(names);
                }
                collect_setup_names(&handler.body.body, helpers, enclosing);
            }
            if let Some(finalizer) = &statement.finalizer {
                collect_setup_names(&finalizer.body, helpers, enclosing);
            }
        }
        Statement::SwitchStatement(statement) => {
            for case in &statement.cases {
                collect_setup_names(&case.consequent, helpers, enclosing);
            }
        }
        other => {
            if let Some(declaration) = other.as_declaration() {
                collect_declaration_names(declaration, helpers, enclosing);
            }
        }
    }
}

fn collect_declaration_names(
    declaration: &Declaration<'_>,
    helpers: &mut BTreeSet<String>,
    enclosing: &mut BTreeSet<String>,
) {
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            collect_variable_names(declaration, helpers, enclosing);
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                helpers.insert(id.name.to_string());
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                helpers.insert(id.name.to_string());
            }
        }
        Declaration::TSEnumDeclaration(declaration) => {
            enclosing.insert(declaration.id.name.to_string());
        }
        _ => {}
    }
}

fn collect_variable_names(
    declaration: &VariableDeclaration<'_>,
    helpers: &mut BTreeSet<String>,
    enclosing: &mut BTreeSet<String>,
) {
    for declarator in &declaration.declarations {
        let mut names = Vec::new();
        pattern_names(&declarator.id, &mut names);
        if declarator.init.as_ref().is_some_and(is_function_like) {
            helpers.extend(names);
        } else {
            enclosing.extend(names);
        }
    }
}

fn declaration_names(declaration: &VariableDeclaration<'_>, out: &mut Vec<String>) {
    for declarator in &declaration.declarations {
        pattern_names(&declarator.id, out);
    }
}

fn pattern_names(pattern: &BindingPattern<'_>, out: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => out.push(identifier.name.to_string()),
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                pattern_names(&property.value, out);
            }
            if let Some(rest) = &object.rest {
                pattern_names(&rest.argument, out);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                pattern_names(element, out);
            }
            if let Some(rest) = &array.rest {
                pattern_names(&rest.argument, out);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => pattern_names(&assignment.left, out),
    }
}

/// Lexical declarations made directly in `statements` (`let`, `const`,
/// `var`, functions, classes, enums).
fn lexical_names(statements: &[Statement<'_>], out: &mut Vec<String>) {
    for statement in statements {
        let declaration = match statement {
            Statement::ExportNamedDeclaration(export) => export.declaration.as_ref(),
            other => other.as_declaration(),
        };
        match declaration {
            Some(Declaration::VariableDeclaration(declaration)) => {
                declaration_names(declaration, out);
            }
            Some(Declaration::FunctionDeclaration(function)) => {
                if let Some(id) = &function.id {
                    out.push(id.name.to_string());
                }
            }
            Some(Declaration::ClassDeclaration(class)) => {
                if let Some(id) = &class.id {
                    out.push(id.name.to_string());
                }
            }
            Some(Declaration::TSEnumDeclaration(declaration)) => {
                out.push(declaration.id.name.to_string());
            }
            _ => {}
        }
    }
}

/// `var` declarations anywhere in a function body outside nested functions.
fn var_names(statements: &[Statement<'_>], out: &mut Vec<String>) {
    for statement in statements {
        var_names_in(statement, out);
    }
}

fn var_names_in(statement: &Statement<'_>, out: &mut Vec<String>) {
    match statement {
        Statement::VariableDeclaration(declaration)
            if declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var =>
        {
            declaration_names(declaration, out);
        }
        Statement::BlockStatement(block) => var_names(&block.body, out),
        Statement::IfStatement(statement) => {
            var_names_in(&statement.consequent, out);
            if let Some(alternate) = &statement.alternate {
                var_names_in(alternate, out);
            }
        }
        Statement::ForStatement(statement) => {
            if let Some(oxc_ast::ast::ForStatementInit::VariableDeclaration(declaration)) =
                &statement.init
                && declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var
            {
                declaration_names(declaration, out);
            }
            var_names_in(&statement.body, out);
        }
        Statement::ForInStatement(statement) => {
            if let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) =
                &statement.left
                && declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var
            {
                declaration_names(declaration, out);
            }
            var_names_in(&statement.body, out);
        }
        Statement::ForOfStatement(statement) => {
            if let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) =
                &statement.left
                && declaration.kind == oxc_ast::ast::VariableDeclarationKind::Var
            {
                declaration_names(declaration, out);
            }
            var_names_in(&statement.body, out);
        }
        Statement::WhileStatement(statement) => var_names_in(&statement.body, out),
        Statement::DoWhileStatement(statement) => var_names_in(&statement.body, out),
        Statement::LabeledStatement(statement) => var_names_in(&statement.body, out),
        Statement::TryStatement(statement) => {
            var_names(&statement.block.body, out);
            if let Some(handler) = &statement.handler {
                var_names(&handler.body.body, out);
            }
            if let Some(finalizer) = &statement.finalizer {
                var_names(&finalizer.body, out);
            }
        }
        Statement::SwitchStatement(statement) => {
            for case in &statement.cases {
                var_names(&case.consequent, out);
            }
        }
        _ => {}
    }
}
