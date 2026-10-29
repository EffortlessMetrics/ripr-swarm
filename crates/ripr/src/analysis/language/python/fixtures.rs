//! pytest fixture definitions and test-to-fixture resolution (#4563).
//!
//! A test that receives the owner's result through a fixture parameter does
//! not call the owner in its own body. This module records module-level
//! `@pytest.fixture` functions per file and attaches, to each test, the
//! definitions its parameters name: the test's own module first, then
//! `conftest.py` files up the directory chain (nearest wins, as pytest
//! resolves them). Facts are advisory: a fixture that cannot be resolved
//! simply leaves the test unrelated, and a renamed fixture
//! (`@pytest.fixture(name=...)`) is not recorded.

use super::admission::PythonAssertionAdmission;
use super::module_constants::mentions_identifier;
use super::owners_tests::{decorator_names, test_imports};
use super::related_tests::{import_source_module_matches_owner, test_body_binds_local};
use super::source_utils::{SourceText, text_for_range};
use super::{PythonFixtureDef, PythonFramework, PythonImport, PythonOwner, PythonTest};
use crate::domain::OwnerKind;
use rustpython_parser::ast::{Constant, Expr, Mod, Ranged, Stmt, UnaryOp};
use rustpython_parser::text_size::TextRange;
use rustpython_parser::{Mode, parse};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Most resolved fixture definitions attached to one test.
const MAX_FIXTURE_DEFS_PER_TEST: usize = 8;

/// Module-level `@pytest.fixture` functions of one file.
pub(super) fn collect_fixture_defs(
    file: &Path,
    source: &SourceText<'_>,
    statements: &[Stmt],
    module_imports: &[PythonImport],
) -> Vec<PythonFixtureDef> {
    let mut defs = Vec::new();
    let mut aliases = fixture_aliases(module_imports);
    for stmt in statements {
        // An alias stops being pytest's decorator once a statement before the
        // decorated def rebinds the name (#10473), and starts again when a
        // later pytest import binds it.
        update_aliases_for_binding(stmt, &mut aliases);
        let (name, decorators, body, range) = match stmt {
            Stmt::FunctionDef(function) => (
                function.name.as_str(),
                &function.decorator_list,
                &function.body,
                function.range,
            ),
            Stmt::AsyncFunctionDef(function) => (
                function.name.as_str(),
                &function.decorator_list,
                &function.body,
                function.range,
            ),
            _ => continue,
        };
        if !is_plain_fixture(source, decorators, &aliases) {
            // A later def of the same name replaces the fixture as the
            // module attribute pytest collects, including a renamed
            // `@pytest.fixture(name=...)` def that is not itself recorded.
            defs.retain(|def: &PythonFixtureDef| def.binding != name);
            // A fixture renamed to `caplog` is recorded under that public
            // name so the captured-log readings see the override (#10418).
            // Other renamed fixtures stay unrecorded.
            if renames_to_caplog(decorators, &aliases) {
                defs.push(PythonFixtureDef {
                    name: "caplog".to_string(),
                    binding: name.to_string(),
                    file: file.to_path_buf(),
                    body_text: text_for_range(source, range),
                    imports: test_imports(file, module_imports, body),
                    literal_construction: None,
                    literal_params: None,
                    isolated: false,
                    scope_imports: Vec::new(),
                });
            }
            continue;
        }
        defs.push(PythonFixtureDef {
            name: name.to_string(),
            binding: name.to_string(),
            file: file.to_path_buf(),
            body_text: text_for_range(source, range),
            imports: test_imports(file, module_imports, body),
            literal_construction: literal_construction(stmt, statements, source, &aliases),
            literal_params: super::param_fixtures::literal_params(
                stmt,
                statements,
                source,
                |name| is_fixture_decorator_name(name, &aliases),
            ),
            isolated: false,
            scope_imports: Vec::new(),
        });
    }
    defs
}

/// Local names this file binds to pytest's fixture decorator with
/// `from pytest import fixture as fx` (also `pytest_asyncio`), so a decorator
/// spelled `@fx` is still a fixture (#10454).
fn fixture_aliases(imports: &[PythonImport]) -> Vec<String> {
    imports
        .iter()
        .filter(|import| {
            import.imported == "fixture"
                && matches!(import.source_module.as_str(), "pytest" | "pytest_asyncio")
        })
        // A later import that rebinds the same local name wins, so a shadowed
        // alias is not trusted (fail closed, as before aliases were read).
        .filter(|import| {
            imports
                .iter()
                .rev()
                .find(|other| other.alias == import.alias)
                .is_some_and(|last| last == *import)
        })
        .map(|import| import.alias.clone())
        .collect()
}

/// Apply the name bindings of one top-level statement to the fixture aliases:
/// an assignment, `def` or `class` of an alias drops it, and a pytest
/// `fixture` import (re)adds its local name. Other imports are already
/// resolved by `fixture_aliases` (last import wins).
fn update_aliases_for_binding(stmt: &Stmt, aliases: &mut Vec<String>) {
    let mut bound = Vec::new();
    match stmt {
        Stmt::Assign(assign) => {
            for target in &assign.targets {
                target_names(target, &mut bound);
            }
        }
        // A bare annotation (`fx: object`) binds nothing.
        Stmt::AnnAssign(assign) if assign.value.is_some() => {
            target_names(&assign.target, &mut bound);
        }
        Stmt::AugAssign(assign) => target_names(&assign.target, &mut bound),
        Stmt::ClassDef(class) => bound.push(class.name.to_string()),
        Stmt::FunctionDef(function) => bound.push(function.name.to_string()),
        Stmt::AsyncFunctionDef(function) => bound.push(function.name.to_string()),
        Stmt::Import(import) => {
            for name in &import.names {
                // `import a.b` binds `a`; `import a.b as c` binds `c`.
                let local = name.asname.as_ref().map_or_else(
                    || name.name.split('.').next().unwrap_or_default().to_string(),
                    |alias| alias.to_string(),
                );
                bound.push(local);
            }
        }
        Stmt::ImportFrom(import) => {
            let from_pytest = import.level.is_none_or(|level| level.to_u32() == 0)
                && import
                    .module
                    .as_ref()
                    .is_some_and(|module| matches!(module.as_str(), "pytest" | "pytest_asyncio"));
            for name in &import.names {
                let local = name.asname.as_ref().unwrap_or(&name.name).to_string();
                if from_pytest && name.name.as_str() == "fixture" {
                    if !aliases.contains(&local) {
                        aliases.push(local);
                    }
                } else {
                    bound.push(local);
                }
            }
        }
        _ => {}
    }
    aliases.retain(|alias| !bound.contains(alias));
}

/// Plain names bound by an assignment target, including tuple/list unpacking.
fn target_names(target: &Expr, out: &mut Vec<String>) {
    match target {
        Expr::Name(name) => out.push(name.id.to_string()),
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|elt| target_names(elt, out)),
        Expr::List(list) => list.elts.iter().for_each(|elt| target_names(elt, out)),
        Expr::Starred(starred) => target_names(&starred.value, out),
        _ => {}
    }
}

fn is_fixture_decorator_name(name: &str, aliases: &[String]) -> bool {
    name == "fixture" || name.ends_with(".fixture") || aliases.iter().any(|alias| alias == name)
}

/// A `fixture` / `pytest.fixture` / `pytest_asyncio.fixture` decorator (or an
/// alias imported from them) that does not rename the fixture.
fn is_plain_fixture(source: &str, decorators: &[Expr], aliases: &[String]) -> bool {
    decorators.iter().any(|decorator| {
        let names = decorator_names(std::slice::from_ref(decorator));
        names
            .iter()
            .any(|name| is_fixture_decorator_name(name, aliases))
            && !text_for_range(source, decorator.range())
                .replace(' ', "")
                .contains("name=")
    })
}

/// A fixture decorator whose `name=` keyword is the literal string `caplog`.
/// Only the parsed keyword counts: the text of `params=` or another argument
/// that spells it is data, not a registration name.
fn renames_to_caplog(decorators: &[Expr], aliases: &[String]) -> bool {
    decorators.iter().any(|decorator| {
        let Expr::Call(call) = decorator else {
            return false;
        };
        let is_fixture = decorator_names(std::slice::from_ref(decorator))
            .iter()
            .any(|name| is_fixture_decorator_name(name, aliases));
        is_fixture
            && call.keywords.iter().any(|keyword| {
                keyword.arg.as_ref().is_some_and(|arg| arg == "name")
                    && matches!(
                        &keyword.value,
                        Expr::Constant(constant)
                            if matches!(&constant.value, Constant::Str(text) if text == "caplog")
                    )
            })
    })
}

/// The class a fixture returns a fresh, literal-built instance of (#9619):
/// `@pytest.fixture` with no decorator arguments (so function scope, no
/// `params=` and no `autouse=`) on a synchronous def with no parameters whose
/// body is an optional docstring and `return Class(<literals>)`. Such a
/// fixture hands each test the same object `account = Account(100)` would
/// build in its body. `None` for any other shape, and when the file binds the
/// fixture's name or the class name more than once (or star-imports), since
/// the value pytest registers is then not the one read here.
fn literal_construction(
    stmt: &Stmt,
    statements: &[Stmt],
    source: &str,
    aliases: &[String],
) -> Option<String> {
    let Stmt::FunctionDef(function) = stmt else {
        return None;
    };
    let [decorator] = function.decorator_list.as_slice() else {
        return None;
    };
    let decorator_target = match decorator {
        Expr::Call(call) if call.args.is_empty() && call.keywords.is_empty() => &*call.func,
        Expr::Call(_) => return None,
        other => other,
    };
    if !decorator_names(std::slice::from_ref(decorator_target))
        .iter()
        .any(|name| is_fixture_decorator_name(name, aliases))
    {
        return None;
    }
    let args = &function.args;
    if !args.posonlyargs.is_empty()
        || !args.args.is_empty()
        || args.vararg.is_some()
        || !args.kwonlyargs.is_empty()
        || args.kwarg.is_some()
    {
        return None;
    }
    let body = match function.body.as_slice() {
        [Stmt::Expr(docstring), rest @ ..]
            if matches!(
                &*docstring.value,
                Expr::Constant(constant) if matches!(constant.value, Constant::Str(_))
            ) =>
        {
            rest
        }
        body => body,
    };
    let [Stmt::Return(returned)] = body else {
        return None;
    };
    let Some(Expr::Call(call)) = returned.value.as_deref() else {
        return None;
    };
    let Expr::Name(class) = &*call.func else {
        return None;
    };
    if !call.args.iter().all(is_literal_argument)
        || !call
            .keywords
            .iter()
            .all(|keyword| keyword.arg.is_some() && is_literal_argument(&keyword.value))
    {
        return None;
    }
    let class = class.id.to_string();
    (module_binding_count(statements, source, function.name.as_str()) == Some(1)
        && module_binding_count(statements, source, &class) == Some(1))
    .then_some(class)
}

/// A constant, or a signed numeric constant (`-1`).
pub(super) fn is_literal_argument(expr: &Expr) -> bool {
    match expr {
        Expr::Constant(_) => true,
        Expr::UnaryOp(op) if matches!(op.op, UnaryOp::USub | UnaryOp::UAdd) => matches!(
            &*op.operand,
            Expr::Constant(constant)
                if matches!(constant.value, Constant::Int(_) | Constant::Float(_))
        ),
        _ => false,
    }
}

/// How many top-level statements of a module bind `name`, or `None` when a
/// star import makes the module's bindings unknowable. A compound statement
/// (`if`, `try`, `for`, `with`, ...) or a bare expression counts as a binding
/// whenever its text spells the name: it may define or rebind it, and this
/// walk does not prove otherwise.
pub(super) fn module_binding_count(statements: &[Stmt], source: &str, name: &str) -> Option<usize> {
    let mut count = 0;
    for stmt in statements {
        let mut bound = Vec::new();
        match stmt {
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    target_names(target, &mut bound);
                }
            }
            Stmt::AnnAssign(assign) if assign.value.is_some() => {
                target_names(&assign.target, &mut bound);
            }
            Stmt::AnnAssign(_) => {}
            Stmt::AugAssign(assign) => target_names(&assign.target, &mut bound),
            Stmt::ClassDef(class) => bound.push(class.name.to_string()),
            Stmt::FunctionDef(function) => bound.push(function.name.to_string()),
            Stmt::AsyncFunctionDef(function) => bound.push(function.name.to_string()),
            Stmt::Import(import) => {
                for alias in &import.names {
                    bound.push(alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default().to_string(),
                        |local| local.to_string(),
                    ));
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    if alias.name.as_str() == "*" {
                        return None;
                    }
                    bound.push(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                }
            }
            other => {
                if mentions_identifier(&text_for_range(source, other.range()), name) {
                    bound.push(name.to_string());
                }
            }
        }
        count += bound.iter().filter(|bound| *bound == name).count();
    }
    Some(count)
}

/// What a module can do to a fixture instance besides its recorded plain
/// fixtures (#9619), read for the test's module and every `conftest.py` up
/// its chain before a literal-construction fixture counts as isolated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct FixtureSideEffects {
    /// Text, decorators included, of every fixture-decorated def that is not
    /// recorded as a plain fixture (a `name=` override) and of every
    /// top-level compound statement; a fixture name it spells is reachable.
    pub(super) texts: Vec<String>,
    /// Names the module binds at top level: an import or assignment of a
    /// fixture name in a nearer conftest replaces the resolved fixture.
    pub(super) bound: Vec<String>,
    /// `from M import y` imports other than pytest's own: an imported name
    /// may be a fixture (an autouse one included) that pytest registers.
    pub(super) from_imports: Vec<PythonImport>,
    /// The module can reach every fixture value without naming one, or bring
    /// in fixtures this walk cannot read: a star import, `pytest_plugins`,
    /// `pytestmark`, any `pytest_*` hook, a fixture that walks the request
    /// (`getfixturevalue`, `fixturenames`, `funcargs`, `node`), a `global`
    /// rebinding inside a def, or a module that only partly parsed.
    pub(super) opaque: bool,
}

/// Request and item attributes through which a fixture reaches other
/// fixtures' values without spelling their names.
const REQUEST_WALKS: [&str; 5] = [
    "getfixturevalue",
    "fixturenames",
    "funcargs",
    "node",
    "_fixturemanager",
];

pub(super) fn collect_fixture_side_effects(
    source: &str,
    statements: &[Stmt],
    module_imports: &[PythonImport],
) -> FixtureSideEffects {
    let aliases = fixture_aliases(module_imports);
    let mut side = FixtureSideEffects {
        from_imports: module_imports
            .iter()
            .filter(|import| {
                !import.source_module.is_empty()
                    && !matches!(import.source_module.as_str(), "pytest" | "pytest_asyncio")
            })
            .cloned()
            .collect(),
        ..FixtureSideEffects::default()
    };
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_) => {
                let (name, decorators) = match stmt {
                    Stmt::FunctionDef(function) => {
                        (function.name.as_str(), &function.decorator_list)
                    }
                    Stmt::AsyncFunctionDef(function) => {
                        (function.name.as_str(), &function.decorator_list)
                    }
                    Stmt::ClassDef(class) => (class.name.as_str(), &class.decorator_list),
                    _ => continue,
                };
                side.bound.push(name.to_string());
                // A def's range starts at `def`; its decorators come first.
                let start = decorators
                    .iter()
                    .map(|decorator| decorator.range().start())
                    .chain(std::iter::once(stmt.range().start()))
                    .min()
                    .unwrap_or_else(|| stmt.range().start());
                let text = text_for_range(source, TextRange::new(start, stmt.range().end()));
                let fixture_shaped = decorator_names(decorators)
                    .iter()
                    .any(|decorator| decorator.ends_with("fixture") || aliases.contains(decorator));
                side.opaque |= name.starts_with("pytest_")
                    || mentions_identifier(&text, "global")
                    || (fixture_shaped
                        && REQUEST_WALKS
                            .iter()
                            .any(|walk| mentions_identifier(&text, walk)));
                if fixture_shaped && !is_plain_fixture(source, decorators, &aliases) {
                    side.texts.push(text);
                }
            }
            Stmt::Import(import) => {
                for alias in &import.names {
                    side.bound.push(alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default().to_string(),
                        |local| local.to_string(),
                    ));
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    side.opaque |= alias.name.as_str() == "*";
                    side.bound
                        .push(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                }
            }
            Stmt::Assign(_) | Stmt::AnnAssign(_) | Stmt::AugAssign(_) | Stmt::Expr(_) => {
                match stmt {
                    Stmt::Assign(assign) => {
                        for target in &assign.targets {
                            target_names(target, &mut side.bound);
                        }
                    }
                    Stmt::AnnAssign(assign) => target_names(&assign.target, &mut side.bound),
                    Stmt::AugAssign(assign) => target_names(&assign.target, &mut side.bound),
                    _ => {}
                }
                let text = text_for_range(source, stmt.range());
                side.opaque |= ["pytest_plugins", "pytestmark", "globals", "setattr"]
                    .iter()
                    .any(|name| mentions_identifier(&text, name));
            }
            other => {
                let text = text_for_range(source, other.range());
                side.opaque |= ["pytest_plugins", "pytestmark", "global"]
                    .iter()
                    .any(|name| mentions_identifier(&text, name));
                side.texts.push(text);
            }
        }
    }
    side
}

/// The side effects the workspace pass records for one parsed file: a file
/// with parse limitations may hold fixtures this walk never saw.
pub(super) fn side_effects_of(
    facts: &super::source_facts::PythonSourceFacts,
) -> FixtureSideEffects {
    let mut side = facts.fixture_side_effects.clone();
    side.opaque |= !facts.limitations.is_empty();
    side
}

/// Mark each attached literal-construction fixture that nothing else visible
/// to the test can reach (#9619), and record the `from` imports of its scope
/// (the test's module and every `conftest.py` up its chain) for the owner
/// check. A fixture is isolated when every chain conftest that exists was
/// read, no scope file is opaque, no other fixture or statement in the scope
/// spells its name (an autouse fixture that takes `account` could deposit
/// into it first), and no scope file but its own binds the name (a nearer
/// conftest's `from helpers import account` replaces it). A third-party
/// plugin fixture outside the workspace is not visible to this read.
pub(super) fn mark_isolated_fixture_defs(
    tests: &mut [PythonTest],
    defs_by_file: &BTreeMap<PathBuf, Vec<PythonFixtureDef>>,
    side_by_file: &BTreeMap<PathBuf, FixtureSideEffects>,
    mut is_unreadable_conftest: impl FnMut(&Path) -> bool,
) {
    // Tests of one file share a scope, so the answer is per (file, fixture).
    type Scope = (bool, Vec<(PathBuf, PythonImport)>);
    let mut cache: BTreeMap<(PathBuf, PathBuf, String), Scope> = BTreeMap::new();
    for test in tests {
        let test_file = test.file.clone();
        for def in &mut test.fixture_defs {
            if def.literal_construction.is_none() && def.literal_params.is_none() {
                continue;
            }
            let key = (test_file.clone(), def.file.clone(), def.binding.clone());
            let (isolated, scope_imports) = cache
                .entry(key)
                .or_insert_with(|| {
                    let mut scope_imports = Vec::new();
                    let isolated = std::iter::once(test_file.clone())
                        .chain(conftest_chain(&test_file))
                        .all(|file| {
                            if file != test_file && is_unreadable_conftest(&file) {
                                return false;
                            }
                            if let Some(side) = side_by_file.get(&file) {
                                scope_imports.extend(
                                    side.from_imports
                                        .iter()
                                        .map(|import| (file.clone(), import.clone())),
                                );
                                if side.opaque
                                    || side
                                        .texts
                                        .iter()
                                        .any(|text| mentions_identifier(text, &def.name))
                                    || (file != def.file && side.bound.contains(&def.name))
                                {
                                    return false;
                                }
                            }
                            !defs_by_file.get(&file).is_some_and(|defs| {
                                defs.iter().any(|other| {
                                    !(other.file == def.file && other.binding == def.binding)
                                        && mentions_identifier(&other.body_text, &def.name)
                                })
                            })
                        });
                    (isolated, scope_imports)
                })
                .clone();
            def.isolated = isolated;
            def.scope_imports = scope_imports;
        }
    }
}

/// A fixture that hands the test a fresh `OwnerClass(<literals>)` (#9619):
/// an isolated plain fixture returning a literal-built instance of the
/// method owner's class, imported under its own name from the owner's
/// module. The test then reads exactly what `account = Account(100)` in its
/// body would give it, so the fixture hides no discriminator or expected
/// value, and the fixture name is a receiver bound to the owner class.
/// Shared by the `unresolved_pytest_fixture` limit and receiver identity
/// (`related_tests.rs`). Module-level tests only (a class-level fixture
/// override is not recorded), and the test must not rebind the parameter,
/// carry a decorator other than a pytest mark, or have its module bind the
/// name over a conftest fixture.
pub(super) fn literal_owner_instance_fixture(
    test: &PythonTest,
    owner: &PythonOwner,
    fixture: &str,
) -> bool {
    if !matches!(owner.owner_kind, Some(OwnerKind::Method))
        || test.framework != PythonFramework::Pytest
        || test.parametrized
        || test.qualified_name != test.name
    {
        return false;
    }
    let Some((class, _)) = owner.qualified_name.rsplit_once('.') else {
        return false;
    };
    if class.contains('.')
        || test_body_binds_local(test, fixture)
        || !injected_parameter(test, fixture)
    {
        return false;
    }
    let Some(def) = test.fixture_defs.iter().find(|def| def.name == fixture) else {
        return false;
    };
    let rebinding = &test.constant_rebinding;
    if def.file != test.file
        && (rebinding.module_opaque || rebinding.module_bound.iter().any(|name| name == fixture))
    {
        return false;
    }
    // A test decorator other than a pytest mark may swap the argument.
    def.isolated
        && test
            .decorators
            .iter()
            .all(|decorator| decorator.starts_with("pytest.mark."))
        && def.literal_construction.as_deref() == Some(class)
        && def.imports.iter().any(|import| {
            import.imported == class
                && import.alias == class
                && import_source_module_matches_owner(import, owner, &def.file)
        })
        // Any other imported name in the scope could be a fixture pytest
        // registers; only the owner's own module is trusted not to be.
        && def
            .scope_imports
            .iter()
            .all(|(file, import)| import_source_module_matches_owner(import, owner, file))
}

/// Whether the test's signature takes `fixture` as a parameter pytest
/// injects: a positional or keyword parameter without a default. pytest
/// leaves `account=FakeAccount(100)` alone, so the default is what runs.
pub(super) fn injected_parameter(test: &PythonTest, fixture: &str) -> bool {
    let Ok(Mod::Module(module)) = parse(&test.body_text, Mode::Module, "<python-test-signature>")
    else {
        return false;
    };
    let args = match module.body.first() {
        Some(Stmt::FunctionDef(function)) => &function.args,
        Some(Stmt::AsyncFunctionDef(function)) => &function.args,
        _ => return false,
    };
    args.posonlyargs
        .iter()
        .chain(&args.args)
        .chain(&args.kwonlyargs)
        .any(|arg| arg.def.arg.as_str() == fixture && arg.default.is_none())
}

/// Attach to every test the fixture definitions its parameters name.
pub(super) fn attach_fixture_defs(
    tests: &mut [PythonTest],
    defs_by_file: &BTreeMap<PathBuf, Vec<PythonFixtureDef>>,
) {
    if defs_by_file.is_empty() {
        return;
    }
    for test in tests {
        let mut attached: Vec<PythonFixtureDef> = Vec::new();
        // A direct (non-indirect) parametrize argname is supplied by pytest,
        // not resolved as a fixture.
        for parameter in &test.fixtures {
            if test.direct_parametrized.contains(parameter) {
                continue;
            }
            // An override of `caplog` is always resolved: the log reading
            // depends on it however many other fixtures the test requests.
            if attached.len() >= MAX_FIXTURE_DEFS_PER_TEST && parameter != "caplog" {
                continue;
            }
            if let Some(def) = resolve(&test.file, parameter, defs_by_file) {
                attached.push(def.clone());
            }
        }
        test.fixture_defs = attached;
    }
}

fn resolve<'a>(
    test_file: &Path,
    name: &str,
    defs_by_file: &'a BTreeMap<PathBuf, Vec<PythonFixtureDef>>,
) -> Option<&'a PythonFixtureDef> {
    let find = |file: &Path| {
        defs_by_file
            .get(file)
            // The last same-name definition in a file is the one pytest binds.
            .and_then(|defs| defs.iter().rev().find(|def| def.name == name))
    };
    if let Some(def) = find(test_file) {
        return Some(def);
    }
    conftest_chain(test_file).find_map(|conftest| find(&conftest))
}

/// The `conftest.py` paths pytest consults for a test file, nearest first:
/// one per directory from the file's own up to the workspace root. Shared by
/// fixture resolution and conftest-driven assertion admission (#6657).
pub(super) fn conftest_chain(test_file: &Path) -> impl Iterator<Item = PathBuf> {
    std::iter::successors(test_file.parent(), |dir| dir.parent()).map(|dir| dir.join("conftest.py"))
}

/// Withhold `extraction_complete_no_assertion` from every test whose
/// directory chain holds a conftest that can run assertion-like code around
/// it: an autouse fixture, a `pytest_runtest_*` hook, `pytest_plugins`, or a
/// conftest RIPR could not read (#6657). A recognized assertion stays
/// recognized; only a clean no-assertion state is downgraded, so a conftest
/// can never grant `NoAssertionLike`.
///
/// `implicit_conftests` are the workspace-relative conftest paths known to
/// install implicit setup. `is_unreadable_conftest` answers for a chain
/// conftest that is not among the implicit set: true when it exists but was
/// not read (capped, excluded or unreadable), which counts the same way.
pub(super) fn taint_admission_by_conftest(
    tests: &mut [PythonTest],
    implicit_conftests: &BTreeSet<PathBuf>,
    mut is_unreadable_conftest: impl FnMut(&Path) -> bool,
) {
    for test in tests {
        if test.assertion_admission != PythonAssertionAdmission::NoAssertionLike {
            continue;
        }
        if conftest_chain(&test.file).any(|conftest| {
            implicit_conftests.contains(&conftest) || is_unreadable_conftest(&conftest)
        }) {
            test.assertion_admission = PythonAssertionAdmission::Unresolved;
        }
    }
}

/// Mark every test whose directory chain holds a conftest declaring any
/// fixture or hook, or one RIPR could not read, as implicitly reachable
/// (#5491 `missing_input` fails closed by existence).
pub(super) fn mark_implicit_reach_by_conftest(
    tests: &mut [PythonTest],
    reach_conftests: &BTreeSet<PathBuf>,
    mut is_unreadable_conftest: impl FnMut(&Path) -> bool,
) {
    for test in tests {
        if conftest_chain(&test.file).any(|conftest| {
            reach_conftests.contains(&conftest) || is_unreadable_conftest(&conftest)
        }) {
            test.constant_rebinding.implicit_reach = true;
        }
    }
}

/// A conftest that declares any fixture or hook, has top-level effects, or
/// could not be read can change what a test does without the test naming it:
/// override a built-in fixture (`def tmp_path()`, `@pytest.fixture(name=...)`),
/// append fixtures in `pytest_generate_tests`, add `filterwarnings` in
/// `pytest_configure`, or call `warnings.simplefilter("error")` at import.
/// Withhold `extraction_complete_no_assertion` from every test below such a
/// conftest (#5491). Fails closed by existence, like the implicit-reach pass,
/// and only downgrades: a conftest never grants the state.
pub(super) fn taint_admission_by_conftest_declarations(
    tests: &mut [PythonTest],
    reach_conftests: &BTreeSet<PathBuf>,
    mut is_unreadable_conftest: impl FnMut(&Path) -> bool,
) {
    for test in tests {
        if test.assertion_admission != PythonAssertionAdmission::NoAssertionLike {
            continue;
        }
        if conftest_chain(&test.file).any(|conftest| {
            reach_conftests.contains(&conftest) || is_unreadable_conftest(&conftest)
        }) {
            test.assertion_admission = PythonAssertionAdmission::Unresolved;
        }
    }
}

/// Record, per test, the modules it imports by a bare name that exist in a
/// directory pytest puts on `sys.path` for it (#5491): the test file's own
/// directory and, while a directory is a package (`__init__.py`), each parent
/// up to and including the first directory that is not (the rootdir pytest's
/// prepend import mode inserts), and always the repository root. `is_present` answers for a
/// workspace-relative path. Single-file modules are recorded by file and
/// packages by directory; the row pass exempts only the owner's own module
/// or package.
pub(super) fn record_bare_sibling_imports(
    tests: &mut [PythonTest],
    mut is_present: impl FnMut(&Path) -> bool,
) {
    let mut resolved: BTreeMap<(PathBuf, String), Option<PathBuf>> = BTreeMap::new();
    let mut dirs_by_file: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for test in tests {
        if test.assertion_admission != PythonAssertionAdmission::NoAssertionLike {
            continue;
        }
        let dirs = dirs_by_file
            .entry(test.file.clone())
            .or_insert_with(|| {
                let mut dirs = Vec::new();
                let mut dir = test.file.parent().unwrap_or(Path::new("")).to_path_buf();
                loop {
                    dirs.push(dir.clone());
                    if dir.as_os_str().is_empty() || !is_present(&dir.join("__init__.py")) {
                        break;
                    }
                    dir = dir.parent().unwrap_or(Path::new("")).to_path_buf();
                }
                // The repository root is on `sys.path` through a root
                // conftest, `python -m pytest` or an editable install, so a
                // root-level module is bare-importable too (fail-closed).
                if !dirs.iter().any(|dir| dir.as_os_str().is_empty()) {
                    dirs.push(PathBuf::new());
                }
                dirs
            })
            .clone();
        let mut found: BTreeSet<PathBuf> = BTreeSet::new();
        for import in &test.imports {
            let module = if import.source_module.is_empty() {
                import.imported.as_str()
            } else {
                import.source_module.as_str()
            };
            let Some(root) = module.split('.').next().filter(|root| !root.is_empty()) else {
                continue;
            };
            for dir in &dirs {
                let hit = resolved
                    .entry((dir.clone(), root.to_string()))
                    .or_insert_with(|| {
                        let file = dir.join(format!("{root}.py"));
                        if is_present(&file) {
                            return Some(file);
                        }
                        let package = dir.join(root);
                        is_present(&package).then_some(package)
                    })
                    .clone();
                if let Some(path) = hit {
                    found.insert(path);
                }
            }
        }
        test.bare_sibling_modules = found.into_iter().collect();
    }
}
