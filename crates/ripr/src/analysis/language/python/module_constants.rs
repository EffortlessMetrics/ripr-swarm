//! Module-level named constants for Python predicate boundaries (#4227).
//!
//! `if amount >= DISCOUNT_THRESHOLD:` with `DISCOUNT_THRESHOLD = 10_000` at
//! module scope compares against a value static evidence can see, the way the
//! Rust (`value_resolution::named_constant`) and TypeScript adapters resolve a
//! same-file `const`. Python has no `const`, so a name counts as a constant
//! only when nothing in the module can rebind it:
//!
//! - it is bound exactly once at module scope, by a top-level `NAME = <literal>`
//!   or `NAME: T = <literal>` statement;
//! - no other module-scope statement binds it (assignment, augmented
//!   assignment, `for`/`with`/`except` target, import, `def`/`class`, `del`);
//! - no function or class body declares it `global`, no walrus (`:=`)
//!   anywhere in the module targets it, and no attribute assignment in the
//!   module names it (`alias.NAME = ...` through a self-import);
//! - the module has no star import, no module-scope `match`, and never names
//!   a dynamic namespace writer (`exec`, `globals`, `vars`, `locals`,
//!   `setattr`, `sys.modules`, `__dict__`), any of which can bind names static
//!   evidence cannot see.
//!
//! Anything else stays unresolved, so the boundary rule keeps failing closed.
//! A function owner additionally drops every constant its own scope shadows
//! (a parameter or any local binding of the same name), and sees none when it
//! nests a `def`, `class` or `lambda`: a changed line inside a nested scope is
//! attributed to the owner, and that scope may shadow the name.
//!
//! Test files carry the other half: [`python_test_rebinding`] records which
//! names a test cannot be trusted to hold at their imported value, and which
//! attributes the test file assigns (`pricing.DISCOUNT_THRESHOLD = 5`). A
//! workspace file that writes the test module's constant (`conftest.py`
//! doing `module.THRESHOLD = 49`) is recorded as a foreign attribute write
//! so boundary credit stays fail-closed (#11424).

use super::boundary::literal_value;
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_start, text_for_range};
use super::{PythonImport, PythonTest, expr_full_name};
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};
use rustpython_parser::{Mode, Tok, lexer::lex};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A module-scope name bound once to a scalar literal and never rebound.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct PythonModuleConstant {
    pub(super) name: String,
    /// Canonical literal text (`10_000` becomes `10000`).
    pub(super) value: String,
    pub(super) line: usize,
}

/// A write of `<module>.<attribute>` in one file (#11424).
///
/// `module` is the dotted import path (`tests.test_pricing`), or `*` when
/// the write names a module this walk cannot resolve. `attribute` is the
/// written name, or `*` when the call can name any attribute.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct PythonModuleAttributeWrite {
    pub(super) module: String,
    pub(super) attribute: String,
}

/// The module-scope literal constants of one parsed module.
pub(super) fn module_literal_constants(
    source: &SourceText<'_>,
    statements: &[Stmt],
) -> Vec<PythonModuleConstant> {
    let Some(stable) = StableNames::of(source, statements, false) else {
        return Vec::new();
    };
    statements
        .iter()
        .filter_map(|stmt| literal_assignment(source, stmt))
        .filter(|constant| stable.holds(&constant.name))
        .collect()
}

/// The module-scope names bound once to a call result (`response =
/// SimpleNamespace(status_code=200)`) and never rebound: objects a function
/// can mutate through an attribute write that a test imports and reads back.
/// The same no-rebinding conditions as [`module_literal_constants`] apply.
pub(super) fn module_object_bindings(source: &SourceText<'_>, statements: &[Stmt]) -> Vec<String> {
    let Some(stable) = StableNames::of(source, statements, false) else {
        return Vec::new();
    };
    statements
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Assign(assign) => match (assign.targets.as_slice(), assign.value.as_ref()) {
                ([Expr::Name(name)], Expr::Call(_)) => Some(name.id.to_string()),
                _ => None,
            },
            _ => None,
        })
        .filter(|name| stable.holds(name))
        .collect()
}

/// A module-scope `NAME = <value>` binding nothing in the module rebinds,
/// with the byte offset where its statement starts.
#[derive(Clone, Copy, Debug)]
pub(super) struct PythonModuleValue<'a> {
    pub(super) name: &'a str,
    pub(super) value: &'a Expr,
    pub(super) offset: usize,
}

/// The module-scope names a decorator may lean on (#10685).
#[derive(Clone, Debug, Default)]
pub(super) struct PythonModuleNames<'a> {
    /// Plain `NAME = <value>` bindings nothing rebinds.
    pub(super) values: Vec<PythonModuleValue<'a>>,
    /// Every name bound exactly once at module scope (an import counts) and
    /// never rebound, under the same conditions as [`module_literal_constants`].
    pub(super) stable: std::collections::BTreeSet<String>,
}

impl PythonModuleNames<'_> {
    pub(super) fn holds(&self, name: &str) -> bool {
        self.stable.contains(name)
    }
}

/// The module-scope names bound once and never rebound, and the plain
/// `NAME = <value>` bindings among them. A decorator reads them (`@cases`,
/// `parametrize(NAMES, ...)`, an `import pytest as pt` root, #10685).
pub(super) fn module_stable_names<'a>(
    source: &SourceText<'_>,
    statements: &'a [Stmt],
) -> PythonModuleNames<'a> {
    let Some(stable) = StableNames::of(source, statements, true) else {
        return PythonModuleNames::default();
    };
    let values = statements
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Assign(assign) => match assign.targets.as_slice() {
                [Expr::Name(name)] => Some(PythonModuleValue {
                    name: name.id.as_str(),
                    value: assign.value.as_ref(),
                    offset: usize::from(assign.range.start()),
                }),
                _ => None,
            },
            _ => None,
        })
        .filter(|value| stable.holds(value.name))
        .collect();
    PythonModuleNames {
        values,
        stable: stable
            .counts
            .keys()
            .filter(|name| stable.holds(name))
            .cloned()
            .collect(),
    }
}

/// Every name a class body binds in its own scope, nested `def` and `class`
/// names included, or None when the walk cannot enumerate them. A decorator
/// in a class body looks a name up there before the module.
pub(super) fn class_scope_names(body: &[Stmt]) -> Option<Vec<String>> {
    let mut bindings = ScopeBindings::default();
    collect_scope_bindings(body, &mut bindings);
    (!bindings.opaque).then_some(bindings.names)
}

/// The facts that decide whether a module-scope name can still be rebound.
struct StableNames {
    counts: std::collections::HashMap<String, usize>,
    globals: Vec<String>,
    attributes: Vec<String>,
    walrus: Vec<String>,
}

impl StableNames {
    /// None when the module binds names this walk cannot see. `import_time`
    /// is set only for reads a decorator makes when the module is imported;
    /// call-time reads (constants, object and table bindings) keep the
    /// whole-module writer scan because a test body can rewrite them.
    fn of(source: &SourceText<'_>, statements: &[Stmt], import_time: bool) -> Option<Self> {
        let mut bindings = ScopeBindings::default();
        collect_scope_bindings(statements, &mut bindings);
        // A decorator reads these names when the module is imported; a writer
        // inside a test body runs later, so only the rest of the module
        // counts (#11018).
        let scanned = if import_time {
            without_test_bodies(source, statements)
        } else {
            source.to_string()
        };
        if bindings.opaque || writes_namespace_dynamically(&scanned) {
            return None;
        }
        let mut globals = Vec::new();
        collect_global_declarations(statements, &mut globals);
        Some(Self {
            counts: bindings
                .counts()
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            globals,
            // `alias.NAME = ...` can rebind the name through a self-import.
            // `statements` is the successful parse of `source`, so this reads
            // the same attributes `assigned_attributes(source)` would without
            // parsing the module again (#10441).
            attributes: assigned_attribute_names(statements),
            walrus: walrus_target_names(source)
                .into_iter()
                .map(str::to_string)
                .collect(),
        })
    }

    fn holds(&self, name: &str) -> bool {
        self.counts.get(name) == Some(&1)
            && !self.globals.iter().any(|global| global == name)
            && !self.attributes.iter().any(|attribute| attribute == name)
            && !self.walrus.iter().any(|target| target == name)
    }
}

/// `source` with the body of every `test*` function, every `test*` method of
/// a `Test*` class and every undecorated module-level helper blanked
/// (decorators, signatures and all other code stay). Such a body runs only when
/// something calls it, after every decorator in the module was applied, so a
/// namespace writer there cannot change what a decorator read at import.
/// Module scope, class bodies, fixtures, decorated functions and decorator
/// arguments are untouched, so those writers still count. A module that names
/// a `test*` function outside its own `def` (called at import, aliased, used
/// as a decorator or argument) keeps the whole-module scan (#11018). A helper
/// named that way keeps only its own body, and so does any helper that body
/// names in turn (#11144).
fn without_test_bodies(source: &str, statements: &[Stmt]) -> String {
    let mut ranges = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    let mut helpers: Vec<(&str, (usize, usize))> = Vec::new();
    for statement in statements {
        match statement {
            Stmt::FunctionDef(function)
                if function.name.starts_with("test")
                    && !is_fixture(source, &function.decorator_list) =>
            {
                ranges.extend(body_range(&function.body));
                names.push(function.name.as_str());
            }
            Stmt::AsyncFunctionDef(function)
                if function.name.starts_with("test")
                    && !is_fixture(source, &function.decorator_list) =>
            {
                ranges.extend(body_range(&function.body));
                names.push(function.name.as_str());
            }
            // An undecorated module-level helper runs only when something
            // calls it. One that anything outside a blanked body names (a
            // call at import, an alias, an argument) keeps its body (#11144).
            Stmt::FunctionDef(function)
                if function.decorator_list.is_empty() && !function.name.starts_with("test") =>
            {
                helpers.extend(
                    body_range(&function.body).map(|range| (function.name.as_str(), range)),
                );
            }
            Stmt::AsyncFunctionDef(function)
                if function.decorator_list.is_empty() && !function.name.starts_with("test") =>
            {
                helpers.extend(
                    body_range(&function.body).map(|range| (function.name.as_str(), range)),
                );
            }
            Stmt::ClassDef(class) if class.name.starts_with("Test") => {
                for member in &class.body {
                    match member {
                        Stmt::FunctionDef(method)
                            if method.name.starts_with("test")
                                && !is_fixture(source, &method.decorator_list) =>
                        {
                            ranges.extend(body_range(&method.body));
                            names.push(method.name.as_str());
                        }
                        Stmt::AsyncFunctionDef(method)
                            if method.name.starts_with("test")
                                && !is_fixture(source, &method.decorator_list) =>
                        {
                            ranges.extend(body_range(&method.body));
                            names.push(method.name.as_str());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    if ranges.is_empty() && helpers.is_empty() {
        return source.to_string();
    }
    // Whole statements are replaced byte for byte by spaces, so the text is
    // valid UTF-8; the fallback is only a guard.
    let blank = |helpers: &[(&str, (usize, usize))]| {
        let mut blanked = source.as_bytes().to_vec();
        for &(start, end) in ranges.iter().chain(helpers.iter().map(|(_, range)| range)) {
            let end = end.min(blanked.len());
            for byte in blanked.get_mut(start..end).unwrap_or_default() {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
        String::from_utf8(blanked).unwrap_or_else(|_| source.to_string())
    };
    // A helper named anywhere outside a blanked body keeps its body, and a
    // kept body can name another helper. Count words once and follow the
    // names a kept body adds, so the work stays linear in the module size.
    let mut blanked = blank(&helpers);
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for word in words(&blanked) {
        *counts.entry(word).or_insert(0) += 1;
    }
    let mut by_name: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
    for (index, (name, _)) in helpers.iter().enumerate() {
        by_name.entry(name).or_default().push(index);
    }
    let mut pending: Vec<usize> = helpers
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| {
            counts.get(name).copied().unwrap_or(0) > by_name.get(name).map_or(0, Vec::len)
        })
        .map(|(index, _)| index)
        .collect();
    let mut kept = vec![false; helpers.len()];
    while let Some(index) = pending.pop() {
        if std::mem::replace(&mut kept[index], true) {
            continue;
        }
        let (start, end) = helpers[index].1;
        let body = source.get(start..end.min(source.len())).unwrap_or_default();
        for word in words(body) {
            let count = counts.entry(word).or_insert(0);
            *count += 1;
            // One mention per `def` of the name is the declaration itself.
            if let Some(indices) = by_name.get(word).filter(|found| *count > found.len()) {
                pending.extend(indices.iter().copied());
            }
        }
    }
    if kept.iter().any(|&keep| keep) {
        let mut index = 0;
        helpers.retain(|_| {
            index += 1;
            !kept[index - 1]
        });
        blanked = blank(&helpers);
    }
    // A test function named anywhere but its own `def` (called at import,
    // used as a decorator) may run before the decorators: scan everything.
    // Count the words once: a whole-word occurrence of an identifier is a word
    // equal to it, so this is the same test as scanning for each name without
    // a pass over the module per test (a module of N tests cost N scans). A
    // name with a character `words` splits on (Python allows identifier
    // characters that `char::is_alphanumeric` rejects, such as `\u{2118}`) is
    // not a word of its own, so it keeps the per-name scan.
    let named_outside_its_def = {
        let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
        let mut final_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for word in words(&blanked) {
            *final_counts.entry(word).or_insert(0) += 1;
        }
        names.iter().any(|name| {
            if name.chars().all(is_word) {
                final_counts.get(name).copied().unwrap_or(0) > 1
            } else {
                identifier_offsets(&blanked, name).count() > 1
            }
        })
    };
    if named_outside_its_def {
        return source.to_string();
    }
    blanked
}

/// A `test*`-named function that a decorator spelling `fixture` makes a
/// fixture runs when a test requests it, not as a test; it is not blanked.
fn is_fixture(source: &str, decorators: &[Expr]) -> bool {
    decorators.iter().any(|decorator| {
        let range = decorator.range();
        source
            .get(usize::from(range.start())..usize::from(range.end()))
            .is_none_or(|text| text.contains("fixture"))
    })
}

fn body_range(body: &[Stmt]) -> Option<(usize, usize)> {
    let first = body.first()?;
    let last = body.last()?;
    Some((
        usize::from(first.range().start()),
        usize::from(last.range().end()),
    ))
}

/// How a module binds its names, for the conditional-definition and
/// module-value reads (#9423): whether a name is bound exactly once, with no
/// `global`, walrus or attribute replacement, no star import and no dynamic
/// namespace writer that could rebind it behind the walk's back.
pub(super) struct ModuleBindingFacts {
    counts: std::collections::HashMap<String, usize>,
    rebound: std::collections::HashSet<String>,
    attributes: std::collections::HashSet<String>,
    opaque: bool,
}

impl ModuleBindingFacts {
    /// The module's binding facts over `statements`.
    pub(super) fn of(source: &str, statements: &[Stmt]) -> Self {
        let mut bindings = ScopeBindings::default();
        collect_scope_bindings(statements, &mut bindings);
        let mut globals = Vec::new();
        collect_global_declarations(statements, &mut globals);
        let mut counts = std::collections::HashMap::new();
        for name in bindings.names {
            *counts.entry(name).or_insert(0) += 1;
        }
        Self {
            counts,
            rebound: globals
                .into_iter()
                .chain(walrus_target_names(source).into_iter().map(str::to_string))
                .collect(),
            attributes: assigned_attribute_names(statements).into_iter().collect(),
            opaque: bindings.opaque || writes_namespace_dynamically(source),
        }
    }

    /// Whether `name` is bound exactly once at module scope and nothing in
    /// the module can rebind it.
    pub(super) fn bound_once(&self, name: &str) -> bool {
        !self.opaque
            && !self.rebound.contains(name)
            && !self.may_assign_attribute(name)
            && self.counts.get(name) == Some(&1)
    }

    /// Whether an attribute may be replaced, even with its receiver still bound.
    pub(super) fn may_assign_attribute(&self, name: &str) -> bool {
        self.opaque || self.attributes.contains(name) || self.attributes.contains("*")
    }

    /// Whether a walrus or `global` anywhere in the module may bind `name`,
    /// including into a class namespace, or the module is opaque.
    pub(super) fn may_rebind(&self, name: &str) -> bool {
        self.opaque || self.rebound.contains(name)
    }

    /// Whether the module assigns a dunder attribute (`X.__new__ = ...`),
    /// which can change what a class builds or looks up (#9226).
    pub(super) fn assigns_dunder(&self) -> bool {
        self.opaque
            || self.attributes.iter().any(|attribute| {
                attribute == "*"
                    || (attribute.len() > 4
                        && attribute.starts_with("__")
                        && attribute.ends_with("__"))
            })
    }
}

/// The constants still visible inside a function owner: those its parameters
/// and local bindings do not shadow. `body_text` is the function's source,
/// used only to catch a walrus binding inside it.
pub(super) fn constants_visible_in_function(
    constants: &[PythonModuleConstant],
    args: &ast::Arguments,
    body: &[Stmt],
    body_text: &str,
) -> Vec<PythonModuleConstant> {
    if constants.is_empty()
        || contains_nested_scope(body)
        || mentions_identifier(body_text, "lambda")
    {
        return Vec::new();
    }
    let Some(locals) = function_scope_names(args, body) else {
        return Vec::new();
    };
    let walrus = walrus_target_names(body_text);
    constants
        .iter()
        .filter(|constant| {
            !locals.contains(&constant.name) && !walrus.contains(constant.name.as_str())
        })
        .cloned()
        .collect()
}

/// The names a function body binds itself: its parameters, locals and walrus
/// targets, so a module name spelled in it may refer to a local. None when the
/// body nests a scope or lambda, or binds names the walk cannot enumerate.
pub(super) fn function_bound_names(
    args: &ast::Arguments,
    body: &[Stmt],
    body_text: &str,
) -> Option<Vec<String>> {
    if contains_nested_scope(body) || mentions_identifier(body_text, "lambda") {
        return None;
    }
    let mut names = function_scope_names(args, body)?;
    names.extend(
        walrus_target_names(body_text)
            .into_iter()
            .map(str::to_string),
    );
    Some(names)
}

/// What a test file can do to a module constant's value, for one test.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct PythonTestRebinding {
    /// Names the test cannot be trusted to hold at their imported value: its
    /// own parameters and locals. Names its module binds more than once are in
    /// `module_names`; [`Self::rebinds`] reads both.
    pub(super) names: Vec<String>,
    /// Names the test's module binds more than once (an import later replaced
    /// by `fee = ...` or `def fee`) or any function declares `global`. Kept
    /// apart from `names` so a function-local import of an owner is not
    /// mistaken for a module-level replacement (#8247).
    /// Shared by every test of the module: a per-test copy made a file with N
    /// tests hold N copies of N names (quadratic memory and sort time).
    pub(super) module_names: Arc<Vec<String>>,
    /// Every name the test's module binds at least once (imports, `def`,
    /// `class`, assignments, loop and `with` targets), so a builtin it replaces
    /// is visible even when bound a single time.
    pub(super) module_bound: Arc<Vec<String>>,
    /// The module's bindings cannot be enumerated (a star import, a dynamic
    /// namespace writer, a module-scope `match`): no builtin name is trusted.
    pub(super) module_opaque: bool,
    /// Attribute names the test file assigns anywhere (`pricing.LIMIT = 5`),
    /// or `*` when it names a dynamic namespace writer (`setattr`,
    /// `sys.modules`, ...); either can rewrite another module's constant at
    /// runtime.
    pub(super) attributes: Arc<Vec<String>>,
    /// The test or its module binds names this walk cannot enumerate, so no
    /// name is trusted.
    pub(super) opaque: bool,
    /// The test's file is also changed in this diff, so its assertions may
    /// have been written for the new value (`module_value.rs`, #9423).
    pub(super) edited_in_diff: bool,
    /// Code outside the test function may run at the owner implicitly (#5491
    /// `missing_input` fails closed on it): an autouse fixture, `pytestmark`,
    /// a setup or teardown hook, a module-level call, or a `conftest.py` on
    /// the directory chain that declares a fixture or hook. Decided by
    /// existence, never by reading what that code calls.
    pub(super) implicit_reach: bool,
    /// A module `def`/`class` header (decorator, default, annotation, base) or
    /// a conditional import can run code at import (#10040 `no_call_path`).
    pub(super) header_reach: bool,
    /// One import and one argument-free test with no definition-time
    /// expressions; only bounded dunder truthiness consumes this fact.
    pub(super) plain_test_module: bool,
    /// A fixture, setup hook or class decorator that reaches the test
    /// substitutes the owner or its module (`fixture_substitution.rs`), so the
    /// test may assert on a stand-in rather than the changed code. Filled by
    /// the workspace pass.
    pub(super) fixture_substitution: bool,
    /// The test module's own scalar-literal constants: bound once at module
    /// scope, never `global`-rebound or attribute-assigned (#10707). Shared by
    /// every test of the module.
    #[serde(default)]
    pub(super) module_constants: Arc<Vec<PythonModuleConstant>>,
    /// Attribute names another workspace file assigns on this test's module
    /// (`conftest.py` doing `module.TH = 49`), or `*` when a write cannot
    /// name the module or attribute (#11424). Empty until the workspace pass.
    #[serde(default)]
    pub(super) foreign_attributes: Arc<Vec<String>>,
}

impl PythonTestRebinding {
    /// Whether the test may see `name` at a value other than its import.
    pub(super) fn rebinds(&self, name: &str) -> bool {
        self.opaque
            || self.names.iter().any(|bound| bound == name)
            || self.module_names.iter().any(|bound| bound == name)
    }

    /// Whether the test's module rebinds `name` after, or besides, importing it.
    pub(super) fn module_rebinds(&self, name: &str) -> bool {
        self.module_names.iter().any(|bound| bound == name)
    }

    /// Whether the test file may assign attribute `name` on another module,
    /// or another workspace file assigns it on this test's module (#11424).
    pub(super) fn assigns_attribute(&self, name: &str) -> bool {
        self.attributes
            .iter()
            .chain(self.foreign_attributes.iter())
            .any(|attribute| attribute == name || attribute == "*")
    }
}

/// Rebinding facts from one test function's own scope: its parameters and
/// locals. [`with_module_rebinding`] adds the facts of its module.
pub(super) fn python_test_rebinding(
    args: &ast::Arguments,
    body: &[Stmt],
    body_text: &str,
) -> PythonTestRebinding {
    let locals = function_scope_names(args, body);
    PythonTestRebinding {
        opaque: locals.is_none() || contains_nested_scope(body) || body_text.contains(":="),
        names: locals.unwrap_or_default(),
        module_names: Arc::default(),
        module_bound: Arc::default(),
        module_opaque: false,
        attributes: Arc::default(),
        edited_in_diff: false,
        implicit_reach: false,
        header_reach: false,
        plain_test_module: false,
        fixture_substitution: false,
        module_constants: Arc::default(),
        foreign_attributes: Arc::default(),
    }
}

/// Add one test module's facts to each of its tests: names the module binds
/// more than once or any of its functions declares `global`, attributes it
/// assigns, and constructs that bind names the walk cannot enumerate.
/// Computed once per module.
pub(super) fn with_module_rebinding(
    source: &str,
    statements: &[Stmt],
    module_constants: &[PythonModuleConstant],
    tests: &mut [PythonTest],
) {
    let mut module = ScopeBindings::default();
    collect_scope_bindings(statements, &mut module);
    let dynamic = writes_namespace_dynamically(source);
    let plain_test_module = super::dunder_truthiness::plain_test_module(statements);
    // A `global NAME` in any function (an autouse fixture, a helper) can
    // rebind an imported name before another test in the module runs.
    let mut globals = Vec::new();
    collect_global_declarations(statements, &mut globals);
    let counts = module.counts();
    let rebound: Vec<&String> = module
        .names
        .iter()
        .filter(|name| counts.get(name.as_str()).is_some_and(|&count| count > 1))
        .chain(globals.iter())
        .collect();
    let attributes = if dynamic {
        vec!["*".to_string()]
    } else {
        assigned_attribute_names(statements)
    };
    // Built once and shared: these are the same for every test of the module.
    let mut module_names: Vec<String> = rebound.iter().map(|name| (*name).clone()).collect();
    module_names.sort();
    module_names.dedup();
    let module_names = Arc::new(module_names);
    let mut module_bound = module.names.clone();
    module_bound.sort();
    module_bound.dedup();
    let module_bound = Arc::new(module_bound);
    let attributes = Arc::new(attributes);
    let module_constants = Arc::new(module_constants.to_vec());
    for test in tests {
        let rebinding = &mut test.constant_rebinding;
        rebinding.module_constants = Arc::clone(&module_constants);
        rebinding.opaque |= module.opaque || dynamic;
        rebinding.names.sort();
        rebinding.names.dedup();
        rebinding.module_names = Arc::clone(&module_names);
        rebinding.module_opaque = module.opaque || dynamic;
        rebinding.plain_test_module = plain_test_module;
        rebinding.module_bound = Arc::clone(&module_bound);
        rebinding.attributes = Arc::clone(&attributes);
    }
}

/// Record, per test, the attributes other workspace files assign on that
/// test's module so a test-module constant another file rewrote is not
/// credited (#11424).
pub(super) fn apply_foreign_module_attribute_writes(
    tests: &mut [PythonTest],
    writes_by_file: &[(PathBuf, Vec<PythonModuleAttributeWrite>)],
) {
    if tests.is_empty() || writes_by_file.iter().all(|(_, writes)| writes.is_empty()) {
        return;
    }
    for test in tests {
        let modules = test_module_import_paths(&test.file);
        let mut attributes: Vec<String> = writes_by_file
            .iter()
            .filter(|(file, _)| file != &test.file)
            .flat_map(|(_, writes)| writes)
            .filter(|write| write_targets_modules(write, &modules))
            .map(|write| write.attribute.clone())
            .collect();
        if attributes.is_empty() {
            continue;
        }
        attributes.sort();
        attributes.dedup();
        test.constant_rebinding.foreign_attributes = Arc::new(attributes);
    }
}

/// Writes of `<module>.<attr>` in `statements`: attribute assignment,
/// `setattr(<mod>, "NAME", ...)`, and `monkeypatch.setattr` (#11424).
pub(super) fn module_attribute_writes(
    statements: &[Stmt],
    imports: &[PythonImport],
) -> Vec<PythonModuleAttributeWrite> {
    let mut writes = Vec::new();
    collect_attribute_assignment_writes(statements, imports, &mut writes);
    visit_exprs_in_statements(statements, &mut |expr| {
        if let Expr::Call(call) = expr {
            collect_setattr_write(call, imports, &mut writes);
        }
    });
    writes.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| left.attribute.cmp(&right.attribute))
    });
    writes.dedup();
    writes
}

fn write_targets_modules(write: &PythonModuleAttributeWrite, modules: &[String]) -> bool {
    write.module == "*" || modules.iter().any(|module| module == &write.module)
}

/// Import paths under which a test file can be the target of `<mod>.NAME`.
/// The full repository-relative path is first; a `src`/`lib` layout root
/// also yields the path below it, matching [`super::related_tests::owner_module_paths`].
/// The file stem is last so `import test_pricing` in a pytest rootdir still
/// names `tests/test_pricing.py`.
fn test_module_import_paths(file: &Path) -> Vec<String> {
    let paths = super::related_tests::owner_module_paths(file);
    let Some(stem) = file.file_stem().and_then(|stem| stem.to_str()) else {
        return paths;
    };
    if stem == "__init__" || paths.iter().any(|path| path == stem) {
        return paths;
    }
    let mut paths = paths;
    paths.push(stem.to_string());
    paths
}

fn collect_attribute_assignment_writes(
    statements: &[Stmt],
    imports: &[PythonImport],
    out: &mut Vec<PythonModuleAttributeWrite>,
) {
    assigned_attribute_targets(statements, &|attribute| {
        let module = resolve_module_expr(attribute.value.as_ref(), imports)?;
        Some(format!("{module}\0{}", attribute.attr))
    })
    .into_iter()
    .for_each(|encoded| {
        if let Some((module, attribute)) = encoded.split_once('\0') {
            out.push(PythonModuleAttributeWrite {
                module: module.to_string(),
                attribute: attribute.to_string(),
            });
        }
    });
}

fn collect_setattr_write(
    call: &ast::ExprCall,
    imports: &[PythonImport],
    out: &mut Vec<PythonModuleAttributeWrite>,
) {
    let Some(func) = expr_full_name(call.func.as_ref()) else {
        return;
    };
    if func != "setattr" && !func.ends_with(".setattr") {
        return;
    }
    match call.args.as_slice() {
        [] => out.push(PythonModuleAttributeWrite {
            module: "*".to_string(),
            attribute: "*".to_string(),
        }),
        [target] => match string_constant(target) {
            Some(path) => push_dotted_write(&path, out),
            None => out.push(PythonModuleAttributeWrite {
                module: "*".to_string(),
                attribute: "*".to_string(),
            }),
        },
        [target, second, ..] => {
            if let Some(path) = string_constant(target) {
                if let Some(attribute) = string_constant(second) {
                    out.push(PythonModuleAttributeWrite {
                        module: path,
                        attribute,
                    });
                } else {
                    push_dotted_write(&path, out);
                }
                return;
            }
            let Some(module) = resolve_module_expr(target, imports) else {
                return;
            };
            out.push(PythonModuleAttributeWrite {
                module,
                attribute: string_constant(second).unwrap_or_else(|| "*".to_string()),
            });
        }
    }
}

fn push_dotted_write(path: &str, out: &mut Vec<PythonModuleAttributeWrite>) {
    match dotted_module_and_attr(path) {
        Some((module, attribute)) => out.push(PythonModuleAttributeWrite { module, attribute }),
        None => out.push(PythonModuleAttributeWrite {
            module: "*".to_string(),
            attribute: "*".to_string(),
        }),
    }
}

fn dotted_module_and_attr(path: &str) -> Option<(String, String)> {
    let (module, attribute) = path.rsplit_once('.')?;
    if module.is_empty() || attribute.is_empty() {
        return None;
    }
    Some((module.to_string(), attribute.to_string()))
}

fn string_constant(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Str(text) => Some(text.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn resolve_module_expr(expr: &Expr, imports: &[PythonImport]) -> Option<String> {
    match expr {
        Expr::Name(_) | Expr::Attribute(_) => {
            let name = expr_full_name(expr)?;
            Some(resolve_imported_module(&name, imports).unwrap_or(name))
        }
        _ => None,
    }
}

fn resolve_imported_module(name: &str, imports: &[PythonImport]) -> Option<String> {
    for import in imports {
        let path = imported_module_path(import);
        if import.alias == name || import.imported == name || path == name {
            return Some(path);
        }
        if let Some(rest) = name
            .strip_prefix(&import.alias)
            .and_then(|rest| rest.strip_prefix('.'))
        {
            return Some(if path.is_empty() {
                rest.to_string()
            } else {
                format!("{path}.{rest}")
            });
        }
        if !path.is_empty()
            && let Some(rest) = name
                .strip_prefix(&path)
                .and_then(|rest| rest.strip_prefix('.'))
        {
            return Some(format!("{path}.{rest}"));
        }
    }
    None
}

fn imported_module_path(import: &PythonImport) -> String {
    if import.source_module.is_empty() || import.imported == "*" {
        if import.source_module.is_empty() {
            import.imported.clone()
        } else {
            import.source_module.clone()
        }
    } else {
        format!("{}.{}", import.source_module, import.imported)
    }
}

pub(super) fn visit_exprs_in_statements(statements: &[Stmt], visit: &mut impl FnMut(&Expr)) {
    visit_statements(statements, &mut |stmt| match stmt {
        Stmt::FunctionDef(function) => {
            for decorator in &function.decorator_list {
                walk_expr(decorator, visit);
            }
            walk_arguments(&function.args, visit);
            if let Some(returns) = &function.returns {
                walk_expr(returns, visit);
            }
        }
        Stmt::AsyncFunctionDef(function) => {
            for decorator in &function.decorator_list {
                walk_expr(decorator, visit);
            }
            walk_arguments(&function.args, visit);
            if let Some(returns) = &function.returns {
                walk_expr(returns, visit);
            }
        }
        Stmt::ClassDef(class) => {
            for decorator in &class.decorator_list {
                walk_expr(decorator, visit);
            }
            for base in &class.bases {
                walk_expr(base, visit);
            }
            for keyword in &class.keywords {
                walk_expr(&keyword.value, visit);
            }
        }
        Stmt::Assign(assign) => {
            for target in &assign.targets {
                walk_expr(target, visit);
            }
            walk_expr(&assign.value, visit);
        }
        Stmt::AnnAssign(assign) => {
            walk_expr(&assign.target, visit);
            walk_expr(&assign.annotation, visit);
            if let Some(value) = assign.value.as_deref() {
                walk_expr(value, visit);
            }
        }
        Stmt::AugAssign(assign) => {
            walk_expr(&assign.target, visit);
            walk_expr(&assign.value, visit);
        }
        Stmt::For(stmt) => {
            walk_expr(&stmt.target, visit);
            walk_expr(&stmt.iter, visit);
        }
        Stmt::AsyncFor(stmt) => {
            walk_expr(&stmt.target, visit);
            walk_expr(&stmt.iter, visit);
        }
        Stmt::While(stmt) => walk_expr(&stmt.test, visit),
        Stmt::If(stmt) => walk_expr(&stmt.test, visit),
        Stmt::With(stmt) => {
            for item in &stmt.items {
                walk_expr(&item.context_expr, visit);
                if let Some(vars) = &item.optional_vars {
                    walk_expr(vars, visit);
                }
            }
        }
        Stmt::AsyncWith(stmt) => {
            for item in &stmt.items {
                walk_expr(&item.context_expr, visit);
                if let Some(vars) = &item.optional_vars {
                    walk_expr(vars, visit);
                }
            }
        }
        Stmt::Raise(stmt) => {
            if let Some(exc) = &stmt.exc {
                walk_expr(exc, visit);
            }
            if let Some(cause) = &stmt.cause {
                walk_expr(cause, visit);
            }
        }
        Stmt::Assert(stmt) => {
            walk_expr(&stmt.test, visit);
            if let Some(msg) = &stmt.msg {
                walk_expr(msg, visit);
            }
        }
        Stmt::Expr(stmt) => walk_expr(&stmt.value, visit),
        Stmt::Return(stmt) => {
            if let Some(value) = &stmt.value {
                walk_expr(value, visit);
            }
        }
        Stmt::Delete(stmt) => {
            for target in &stmt.targets {
                walk_expr(target, visit);
            }
        }
        Stmt::Match(stmt) => walk_expr(&stmt.subject, visit),
        _ => {}
    });
}

fn walk_arguments(args: &ast::Arguments, visit: &mut impl FnMut(&Expr)) {
    for default in args
        .posonlyargs
        .iter()
        .chain(&args.args)
        .chain(&args.kwonlyargs)
        .filter_map(|arg| arg.default.as_deref())
    {
        walk_expr(default, visit);
    }
    if let Some(vararg) = &args.vararg
        && let Some(annotation) = vararg.annotation.as_deref()
    {
        walk_expr(annotation, visit);
    }
    if let Some(kwarg) = &args.kwarg
        && let Some(annotation) = kwarg.annotation.as_deref()
    {
        walk_expr(annotation, visit);
    }
}

fn walk_expr(expr: &Expr, visit: &mut impl FnMut(&Expr)) {
    visit(expr);
    match expr {
        Expr::BoolOp(bool_op) => {
            for value in &bool_op.values {
                walk_expr(value, visit);
            }
        }
        Expr::NamedExpr(named) => {
            walk_expr(&named.target, visit);
            walk_expr(&named.value, visit);
        }
        Expr::BinOp(bin_op) => {
            walk_expr(&bin_op.left, visit);
            walk_expr(&bin_op.right, visit);
        }
        Expr::UnaryOp(unary) => walk_expr(&unary.operand, visit),
        Expr::Lambda(lambda) => walk_expr(&lambda.body, visit),
        Expr::IfExp(if_exp) => {
            walk_expr(&if_exp.test, visit);
            walk_expr(&if_exp.body, visit);
            walk_expr(&if_exp.orelse, visit);
        }
        Expr::Dict(dict) => {
            for key in dict.keys.iter().flatten() {
                walk_expr(key, visit);
            }
            for value in &dict.values {
                walk_expr(value, visit);
            }
        }
        Expr::Set(set) => {
            for value in &set.elts {
                walk_expr(value, visit);
            }
        }
        Expr::ListComp(list_comp) => {
            walk_expr(&list_comp.elt, visit);
            walk_comprehensions(&list_comp.generators, visit);
        }
        Expr::SetComp(set_comp) => {
            walk_expr(&set_comp.elt, visit);
            walk_comprehensions(&set_comp.generators, visit);
        }
        Expr::DictComp(dict_comp) => {
            walk_expr(&dict_comp.key, visit);
            walk_expr(&dict_comp.value, visit);
            walk_comprehensions(&dict_comp.generators, visit);
        }
        Expr::GeneratorExp(generator) => {
            walk_expr(&generator.elt, visit);
            walk_comprehensions(&generator.generators, visit);
        }
        Expr::Await(await_expr) => walk_expr(&await_expr.value, visit),
        Expr::Yield(yield_expr) => {
            if let Some(value) = &yield_expr.value {
                walk_expr(value, visit);
            }
        }
        Expr::YieldFrom(yield_expr) => walk_expr(&yield_expr.value, visit),
        Expr::Compare(compare) => {
            walk_expr(&compare.left, visit);
            for comparator in &compare.comparators {
                walk_expr(comparator, visit);
            }
        }
        Expr::Call(call) => {
            walk_expr(&call.func, visit);
            for arg in &call.args {
                walk_expr(arg, visit);
            }
            for keyword in &call.keywords {
                walk_expr(&keyword.value, visit);
            }
        }
        Expr::FormattedValue(formatted) => {
            walk_expr(&formatted.value, visit);
            if let Some(format_spec) = &formatted.format_spec {
                walk_expr(format_spec, visit);
            }
        }
        Expr::JoinedStr(joined) => {
            for value in &joined.values {
                walk_expr(value, visit);
            }
        }
        Expr::Attribute(attribute) => walk_expr(&attribute.value, visit),
        Expr::Subscript(subscript) => {
            walk_expr(&subscript.value, visit);
            walk_expr(&subscript.slice, visit);
        }
        Expr::Starred(starred) => walk_expr(&starred.value, visit),
        Expr::List(list) => {
            for value in &list.elts {
                walk_expr(value, visit);
            }
        }
        Expr::Tuple(tuple) => {
            for value in &tuple.elts {
                walk_expr(value, visit);
            }
        }
        Expr::Slice(slice) => {
            if let Some(lower) = &slice.lower {
                walk_expr(lower, visit);
            }
            if let Some(upper) = &slice.upper {
                walk_expr(upper, visit);
            }
            if let Some(step) = &slice.step {
                walk_expr(step, visit);
            }
        }
        Expr::Constant(_) | Expr::Name(_) => {}
    }
}

fn walk_comprehensions(generators: &[ast::Comprehension], visit: &mut impl FnMut(&Expr)) {
    for comprehension in generators {
        walk_expr(&comprehension.target, visit);
        walk_expr(&comprehension.iter, visit);
        for guard in &comprehension.ifs {
            walk_expr(guard, visit);
        }
    }
}

/// Names a function body binds in its own scope (not its parameters), or
/// None when the body binds names the walk cannot enumerate or defines a
/// nested function or class (#4612 review).
pub(super) fn body_bound_names(body: &[Stmt]) -> Option<Vec<String>> {
    let mut bindings = ScopeBindings::default();
    collect_scope_bindings(body, &mut bindings);
    (!bindings.opaque && !contains_nested_scope(body)).then_some(bindings.names)
}

/// Parameters plus every name bound in a function's own scope, or None when
/// the scope binds names the walk cannot enumerate.
fn function_scope_names(args: &ast::Arguments, body: &[Stmt]) -> Option<Vec<String>> {
    let mut locals = ScopeBindings::default();
    collect_scope_bindings(body, &mut locals);
    if locals.opaque {
        return None;
    }
    locals.names.extend(
        args.posonlyargs
            .iter()
            .chain(&args.args)
            .chain(&args.kwonlyargs)
            .map(|arg| arg.def.arg.to_string())
            .chain(args.vararg.iter().map(|arg| arg.arg.to_string()))
            .chain(args.kwarg.iter().map(|arg| arg.arg.to_string())),
    );
    Some(locals.names)
}

/// Whether `statements` (through compound statements) define a nested
/// function or class.
fn contains_nested_scope(statements: &[Stmt]) -> bool {
    statements.iter().any(|stmt| match stmt {
        Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_) => true,
        Stmt::For(stmt) => contains_nested_scope(&stmt.body) || contains_nested_scope(&stmt.orelse),
        Stmt::AsyncFor(stmt) => {
            contains_nested_scope(&stmt.body) || contains_nested_scope(&stmt.orelse)
        }
        Stmt::While(stmt) => {
            contains_nested_scope(&stmt.body) || contains_nested_scope(&stmt.orelse)
        }
        Stmt::If(stmt) => contains_nested_scope(&stmt.body) || contains_nested_scope(&stmt.orelse),
        Stmt::With(stmt) => contains_nested_scope(&stmt.body),
        Stmt::AsyncWith(stmt) => contains_nested_scope(&stmt.body),
        Stmt::Try(stmt) => {
            contains_nested_scope(&stmt.body)
                || stmt
                    .handlers
                    .iter()
                    .any(|ast::ExceptHandler::ExceptHandler(handler)| {
                        contains_nested_scope(&handler.body)
                    })
                || contains_nested_scope(&stmt.orelse)
                || contains_nested_scope(&stmt.finalbody)
        }
        Stmt::TryStar(stmt) => {
            contains_nested_scope(&stmt.body)
                || stmt
                    .handlers
                    .iter()
                    .any(|ast::ExceptHandler::ExceptHandler(handler)| {
                        contains_nested_scope(&handler.body)
                    })
                || contains_nested_scope(&stmt.orelse)
                || contains_nested_scope(&stmt.finalbody)
        }
        Stmt::Match(stmt) => stmt
            .cases
            .iter()
            .any(|case| contains_nested_scope(&case.body)),
        _ => false,
    })
}

/// Whether `source` names a writer that can bind module names at runtime
/// without an assignment statement, including `unittest.mock` patching
/// (`patch("pkg.LIMIT", 5)`, `patch.object(pkg, "LIMIT", 5)`), and
/// `importlib.reload`, which re-executes the module. Reads code tokens, so a
/// word in a comment or an ordinary string (a docstring saying "env vars",
/// a comment about `--reload`) is not a writer (#10979); an f-string can run
/// code, so its text still counts, and a module the lexer rejects falls back
/// to the whole-text scan. A false match only leaves constants unresolved.
fn writes_namespace_dynamically(source: &str) -> bool {
    let mut tokens = Vec::new();
    for result in lex(source, Mode::Module) {
        match result {
            Ok(pair) => tokens.push(pair),
            Err(_) => return writes_namespace_in_text(source),
        }
    }
    // 0: nothing pending, 1: just read `sys`, 2: just read `sys.`.
    let mut after_sys = 0;
    // An `import` statement binds a name; it writes nothing (#11102).
    let mut in_import = false;
    let mut line_start = true;
    for (index, (token, range)) in tokens.iter().enumerate() {
        match token {
            Tok::Newline | Tok::Semi => {
                in_import = false;
                line_start = true;
                after_sys = 0;
                continue;
            }
            Tok::Indent | Tok::Dedent => continue,
            Tok::Import | Tok::From if line_start => in_import = true,
            // `from unittest.mock import patch` only names the writer; an
            // alias (`import patch as p`) hides later calls, so it counts.
            Tok::Name { .. }
                if in_import && !matches!(tokens.get(index + 1), Some((Tok::As, _))) => {}
            Tok::Name { name } => {
                if DYNAMIC_NAMESPACE_WRITERS.contains(&name.as_str())
                    || (after_sys == 2 && name == "modules")
                {
                    return true;
                }
                after_sys = usize::from(name == "sys");
                line_start = false;
                continue;
            }
            Tok::Dot if after_sys == 1 => {
                after_sys = 2;
                line_start = false;
                continue;
            }
            // `getattr(importlib, "reload")(m)` and `locate("importlib.reload")`
            // name the writer only as a string, so a whitespace-free string
            // with a writer name among its dotted parts still counts. Prose
            // (a docstring saying "env vars") has whitespace and does not.
            Tok::String { value, kind, .. }
                if !kind.is_any_fstring()
                    && !value.trim().contains(char::is_whitespace)
                    && value
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|part| DYNAMIC_NAMESPACE_WRITERS.contains(&part)) =>
            {
                return true;
            }
            Tok::String { kind, .. } if kind.is_any_fstring() => {
                let text = &source[usize::from(range.start())..usize::from(range.end())];
                if writes_namespace_in_text(text) {
                    return true;
                }
            }
            _ => {}
        }
        after_sys = 0;
        line_start = false;
    }
    false
}

const DYNAMIC_NAMESPACE_WRITERS: [&str; 11] = [
    "exec",
    "eval",
    "globals",
    "vars",
    "locals",
    "setattr",
    "__setattr__",
    "delattr",
    "reload",
    "__dict__",
    "patch",
];

fn writes_namespace_in_text(text: &str) -> bool {
    DYNAMIC_NAMESPACE_WRITERS
        .into_iter()
        .any(|identifier| mentions_identifier(text, identifier))
        || text.contains("sys.modules")
}

/// Attribute assignment/deletion targets, including unpacking, annotations,
/// parentheses and whitespace around dots. Reads and annotations without values
/// do not replace an attribute. The caller retains parse uncertainty separately.
pub(super) fn assigned_attributes(source: &str) -> Option<Vec<String>> {
    match parse_module_result(std::path::Path::new("attribute-writes.py"), source) {
        Ok(ast::Mod::Module(module)) => Some(assigned_attribute_names(&module.body)),
        _ => None,
    }
}

pub(super) fn assigned_attribute_names(statements: &[Stmt]) -> Vec<String> {
    assigned_attribute_targets(statements, &|attribute| Some(attribute.attr.to_string()))
}

/// The names the module uses other than to import them or to construct them
/// (`CliRunner(...)`): `CliRunner.invoke = fake`, `setattr(CliRunner, ...)`,
/// `Runner = CliRunner` and a string such as `"click.testing.CliRunner.invoke"`
/// all name a class in a way that can change what it builds. A module the
/// lexer rejects names every identifier it contains, so nothing is hidden.
/// Only names `watched` accepts are kept, each once: the caller asks about a
/// fixed set of classes, and every file's facts are stored and decoded.
pub(super) fn watched_names_used_beyond_construction(
    source: &str,
    watched: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let mut names = BTreeSet::new();
    let mut tokens = Vec::new();
    for result in lex(source, Mode::Module) {
        match result {
            Ok((token, _)) => tokens.push(token),
            Err(_) => {
                return source
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .filter(|word| !word.is_empty() && watched(word))
                    .map(str::to_string)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
            }
        }
    }
    let mut in_import = false;
    let mut line_start = true;
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Tok::Newline => {
                in_import = false;
                line_start = true;
                continue;
            }
            Tok::Semi => {
                in_import = false;
                line_start = true;
                continue;
            }
            Tok::Indent | Tok::Dedent => continue,
            Tok::Import | Tok::From if line_start => in_import = true,
            // `def CliRunner(...)` and `class CliRunner(...)` redefine the
            // name; only a call constructs it.
            Tok::Name { name }
                if !in_import
                    && (!matches!(tokens.get(index + 1), Some(Tok::Lpar))
                        || index
                            .checked_sub(1)
                            .and_then(|before| tokens.get(before))
                            .is_some_and(|before| matches!(before, Tok::Def | Tok::Class)))
                    && watched(name.as_str()) =>
            {
                names.insert(name.to_string());
            }
            Tok::String { value, .. } => names.extend(
                value
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .filter(|word| !word.is_empty() && watched(word))
                    .map(str::to_string),
            ),
            _ => {}
        }
        line_start = false;
    }
    names.into_iter().collect()
}

/// The dotted modules a file imports, at any nesting depth: `import a.b` names
/// `a.b`, `from a import b` names `a` and `a.b`. A relative import names the
/// written module and each imported name, which only over-matches.
pub(super) fn imported_module_names(statements: &[Stmt]) -> Vec<String> {
    let mut modules = Vec::new();
    visit_statements(statements, &mut |stmt| match stmt {
        Stmt::Import(import) => {
            modules.extend(import.names.iter().map(|alias| alias.name.to_string()));
        }
        Stmt::ImportFrom(import) => {
            let module = import.module.as_ref().map(|module| module.to_string());
            if let Some(module) = &module {
                modules.push(module.clone());
            }
            for alias in &import.names {
                modules.push(match &module {
                    Some(module) => format!("{module}.{}", alias.name),
                    None => alias.name.to_string(),
                });
            }
        }
        _ => {}
    });
    modules
}

fn assigned_attribute_targets(
    statements: &[Stmt],
    pick: &dyn Fn(&ast::ExprAttribute) -> Option<String>,
) -> Vec<String> {
    let mut attributes = Vec::new();
    visit_statements(statements, &mut |stmt| match stmt {
        Stmt::Assign(assign) => {
            for target in &assign.targets {
                attribute_target_names(target, pick, &mut attributes);
            }
        }
        Stmt::AnnAssign(assign) if assign.value.is_some() => {
            attribute_target_names(&assign.target, pick, &mut attributes);
        }
        Stmt::AugAssign(assign) => attribute_target_names(&assign.target, pick, &mut attributes),
        Stmt::Delete(delete) => {
            for target in &delete.targets {
                attribute_target_names(target, pick, &mut attributes);
            }
        }
        Stmt::For(stmt) => attribute_target_names(&stmt.target, pick, &mut attributes),
        Stmt::AsyncFor(stmt) => attribute_target_names(&stmt.target, pick, &mut attributes),
        Stmt::With(stmt) => {
            for target in stmt
                .items
                .iter()
                .filter_map(|item| item.optional_vars.as_deref())
            {
                attribute_target_names(target, pick, &mut attributes);
            }
        }
        Stmt::AsyncWith(stmt) => {
            for target in stmt
                .items
                .iter()
                .filter_map(|item| item.optional_vars.as_deref())
            {
                attribute_target_names(target, pick, &mut attributes);
            }
        }
        _ => {}
    });
    attributes.sort();
    attributes.dedup();
    attributes
}

fn attribute_target_names(
    target: &Expr,
    pick: &dyn Fn(&ast::ExprAttribute) -> Option<String>,
    out: &mut Vec<String>,
) {
    match target {
        Expr::Attribute(attribute) => out.extend(pick(attribute)),
        Expr::Tuple(tuple) => {
            for element in &tuple.elts {
                attribute_target_names(element, pick, out);
            }
        }
        Expr::List(list) => {
            for element in &list.elts {
                attribute_target_names(element, pick, out);
            }
        }
        Expr::Starred(starred) => attribute_target_names(&starred.value, pick, out),
        Expr::Subscript(subscript) => attribute_target_names(&subscript.value, pick, out),
        _ => {}
    }
}

fn literal_assignment(source: &SourceText<'_>, stmt: &Stmt) -> Option<PythonModuleConstant> {
    let (target, value) = match stmt {
        Stmt::Assign(assign) => match assign.targets.as_slice() {
            [target] => (target, assign.value.as_ref()),
            _ => return None,
        },
        Stmt::AnnAssign(assign) if assign.simple => {
            (assign.target.as_ref(), assign.value.as_deref()?)
        }
        _ => return None,
    };
    let Expr::Name(name) = target else {
        return None;
    };
    Some(PythonModuleConstant {
        name: name.id.to_string(),
        value: literal_value(&text_for_range(source, value.range()))?,
        line: line_for_range_start(source, stmt.range()),
    })
}

/// Whether `name` is a module-scope name bound once, by the one `NAME = {...}`
/// (or annotated) statement that starts on `line`, with the stability the
/// scalar constants need (#9619). In-place mutation of the table is a separate
/// question (`table_reads`).
pub(super) fn module_table_binding_at(
    source: &SourceText<'_>,
    statements: &[Stmt],
    name: &str,
    line: usize,
) -> bool {
    let Some(stable) = StableNames::of(source, statements, false) else {
        return false;
    };
    stable.holds(name)
        && statements.iter().any(|stmt| {
            let (target, value) = match stmt {
                Stmt::Assign(assign) => match assign.targets.as_slice() {
                    [target] => (target, Some(assign.value.as_ref())),
                    _ => return false,
                },
                Stmt::AnnAssign(assign) if assign.simple => {
                    (assign.target.as_ref(), assign.value.as_deref())
                }
                _ => return false,
            };
            matches!(target, Expr::Name(target) if target.id.as_str() == name)
                && matches!(value, Some(Expr::Dict(_)))
                && line_for_range_start(source, stmt.range()) == line
        })
}

/// Every binding of a name in one scope, counted per occurrence. `opaque`
/// marks a construct that can bind names this walk does not enumerate.
#[derive(Default)]
struct ScopeBindings {
    names: Vec<String>,
    opaque: bool,
}

impl ScopeBindings {
    /// How many times each name is bound, counted in one pass. Counting per
    /// name instead rescans every binding and is quadratic on a module with
    /// thousands of constants.
    fn counts(&self) -> std::collections::HashMap<&str, usize> {
        let mut counts = std::collections::HashMap::new();
        for name in &self.names {
            *counts.entry(name.as_str()).or_insert(0) += 1;
        }
        counts
    }
}

/// Names bound in the scope that owns `statements`. Nested `def` and `class`
/// bind their own name here but open a new scope, so their bodies are not
/// walked; compound statements (`if`, `for`, `try`, `with`) share the scope.
fn collect_scope_bindings(statements: &[Stmt], out: &mut ScopeBindings) {
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => out.names.push(function.name.to_string()),
            Stmt::AsyncFunctionDef(function) => out.names.push(function.name.to_string()),
            Stmt::ClassDef(class) => out.names.push(class.name.to_string()),
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    collect_target_names(target, out);
                }
            }
            Stmt::AugAssign(assign) => collect_target_names(&assign.target, out),
            Stmt::AnnAssign(assign) => collect_target_names(&assign.target, out),
            Stmt::TypeAlias(alias) => collect_target_names(&alias.name, out),
            Stmt::Delete(delete) => {
                for target in &delete.targets {
                    collect_target_names(target, out);
                }
            }
            Stmt::For(stmt) => {
                collect_target_names(&stmt.target, out);
                collect_scope_bindings(&stmt.body, out);
                collect_scope_bindings(&stmt.orelse, out);
            }
            Stmt::AsyncFor(stmt) => {
                collect_target_names(&stmt.target, out);
                collect_scope_bindings(&stmt.body, out);
                collect_scope_bindings(&stmt.orelse, out);
            }
            Stmt::While(stmt) => {
                collect_scope_bindings(&stmt.body, out);
                collect_scope_bindings(&stmt.orelse, out);
            }
            Stmt::If(stmt) => {
                collect_scope_bindings(&stmt.body, out);
                collect_scope_bindings(&stmt.orelse, out);
            }
            Stmt::With(stmt) => {
                collect_with_targets(&stmt.items, out);
                collect_scope_bindings(&stmt.body, out);
            }
            Stmt::AsyncWith(stmt) => {
                collect_with_targets(&stmt.items, out);
                collect_scope_bindings(&stmt.body, out);
            }
            Stmt::Try(stmt) => collect_try_bindings(
                &stmt.body,
                &stmt.handlers,
                &stmt.orelse,
                &stmt.finalbody,
                out,
            ),
            Stmt::TryStar(stmt) => collect_try_bindings(
                &stmt.body,
                &stmt.handlers,
                &stmt.orelse,
                &stmt.finalbody,
                out,
            ),
            Stmt::Import(import) => {
                for alias in &import.names {
                    let bound = alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default().to_string(),
                        ToString::to_string,
                    );
                    out.names.push(bound);
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    if alias.name.as_str() == "*" {
                        out.opaque = true;
                    }
                    out.names
                        .push(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                }
            }
            // Case patterns capture names in forms this walk does not
            // enumerate, so a `match` in scope resolves nothing.
            Stmt::Match(_) => out.opaque = true,
            _ => {}
        }
    }
}

fn collect_try_bindings(
    body: &[Stmt],
    handlers: &[ast::ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
    out: &mut ScopeBindings,
) {
    collect_scope_bindings(body, out);
    for ast::ExceptHandler::ExceptHandler(handler) in handlers {
        if let Some(name) = &handler.name {
            out.names.push(name.to_string());
        }
        collect_scope_bindings(&handler.body, out);
    }
    collect_scope_bindings(orelse, out);
    collect_scope_bindings(finalbody, out);
}

fn collect_with_targets(items: &[ast::WithItem], out: &mut ScopeBindings) {
    for item in items {
        if let Some(target) = &item.optional_vars {
            collect_target_names(target, out);
        }
    }
}

/// Names an assignment target binds: a bare name, or the names inside a
/// tuple, list or starred unpacking. Attribute and subscript targets bind no
/// name in this scope.
fn collect_target_names(target: &Expr, out: &mut ScopeBindings) {
    match target {
        Expr::Name(name) => out.names.push(name.id.to_string()),
        Expr::Tuple(tuple) => {
            for element in &tuple.elts {
                collect_target_names(element, out);
            }
        }
        Expr::List(list) => {
            for element in &list.elts {
                collect_target_names(element, out);
            }
        }
        Expr::Starred(starred) => collect_target_names(&starred.value, out),
        _ => {}
    }
}

/// Every name any function or class body in the module declares global.
fn collect_global_declarations(statements: &[Stmt], out: &mut Vec<String>) {
    visit_statements(statements, &mut |stmt| {
        if let Stmt::Global(global) = stmt {
            out.extend(global.names.iter().map(ToString::to_string));
        }
    });
}

/// Visit statement suites at every nesting depth, without crossing expressions.
fn visit_statements<F: FnMut(&Stmt)>(statements: &[Stmt], out: &mut F) {
    for stmt in statements {
        out(stmt);
        match stmt {
            Stmt::FunctionDef(function) => visit_statements(&function.body, out),
            Stmt::AsyncFunctionDef(function) => visit_statements(&function.body, out),
            Stmt::ClassDef(class) => visit_statements(&class.body, out),
            Stmt::For(stmt) => {
                visit_statements(&stmt.body, out);
                visit_statements(&stmt.orelse, out);
            }
            Stmt::AsyncFor(stmt) => {
                visit_statements(&stmt.body, out);
                visit_statements(&stmt.orelse, out);
            }
            Stmt::While(stmt) => {
                visit_statements(&stmt.body, out);
                visit_statements(&stmt.orelse, out);
            }
            Stmt::If(stmt) => {
                visit_statements(&stmt.body, out);
                visit_statements(&stmt.orelse, out);
            }
            Stmt::With(stmt) => visit_statements(&stmt.body, out),
            Stmt::AsyncWith(stmt) => visit_statements(&stmt.body, out),
            Stmt::Try(stmt) => {
                visit_statements(&stmt.body, out);
                for ast::ExceptHandler::ExceptHandler(handler) in &stmt.handlers {
                    visit_statements(&handler.body, out);
                }
                visit_statements(&stmt.orelse, out);
                visit_statements(&stmt.finalbody, out);
            }
            Stmt::TryStar(stmt) => {
                visit_statements(&stmt.body, out);
                for ast::ExceptHandler::ExceptHandler(handler) in &stmt.handlers {
                    visit_statements(&handler.body, out);
                }
                visit_statements(&stmt.orelse, out);
                visit_statements(&stmt.finalbody, out);
            }
            Stmt::Match(stmt) => {
                for case in &stmt.cases {
                    visit_statements(&case.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Whether `text` holds a walrus (`NAME :=`) targeting `name`. Textual on
/// purpose: an assignment expression can sit inside any expression, and a
/// false match only leaves the constant unresolved.
/// Whether `text` binds `name` with `:=`: a whole-word `name`, optional
/// whitespace, then `:=`. Kept as the reference for [`walrus_target_names`].
#[cfg(test)]
fn walrus_targets(text: &str, name: &str) -> bool {
    identifier_offsets(text, name).any(|end| text[end..].trim_start().starts_with(":="))
}

/// Every name `text` binds with `:=`, from one pass over the text.
///
/// Asking `walrus_targets` once per module constant rescanned the whole
/// source per constant, which is quadratic on a module with many constants.
/// Walking back from each `:=` over whitespace to the maximal word run before
/// it finds exactly the names `walrus_targets` accepts: that run is a
/// whole-word occurrence followed by whitespace and `:=`.
fn walrus_target_names(text: &str) -> std::collections::HashSet<&str> {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    text.match_indices(":=")
        .filter_map(|(start, _)| {
            let before = text[..start].trim_end();
            let name_start = before
                .char_indices()
                .rev()
                .take_while(|&(_, ch)| is_word(ch))
                .last()
                .map(|(idx, _)| idx)?;
            Some(&before[name_start..])
        })
        .collect()
}

/// Whether `text` holds `identifier` as a whole word.
pub(super) fn mentions_identifier(text: &str, identifier: &str) -> bool {
    identifier_offsets(text, identifier).next().is_some()
}

/// The identifier-like words of `text`, split on anything that is not
/// alphanumeric or `_` (the same boundary [`identifier_offsets`] uses).
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty())
}

/// End offsets of every whole-word occurrence of `identifier` in `text`.
fn identifier_offsets<'a>(text: &'a str, identifier: &'a str) -> impl Iterator<Item = usize> + 'a {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    text.match_indices(identifier)
        .filter_map(move |(start, _)| {
            let end = start + identifier.len();
            let before_ok = !text[..start].chars().next_back().is_some_and(is_word);
            let after_ok = !text[end..].chars().next().is_some_and(is_word);
            (before_ok && after_ok).then_some(end)
        })
}

#[cfg(test)]
mod tests;
