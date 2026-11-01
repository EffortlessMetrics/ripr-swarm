//! Fixtures and setup code that replace the owner around a test.
//!
//! A test that asserts `calc.price(3) == 6` while an autouse (or requested)
//! fixture runs `monkeypatch.setattr(calc, "price", lambda x: x * 2)` never
//! executes the changed `price`: the assertion is satisfied by the stand-in.
//! An inline substitution in the test body already keeps the `mocked_module`
//! limit (`static_limits::test_has_mocked_module`); this module extends that
//! same owner to code the test does not spell but pytest or unittest runs
//! around it:
//!
//! - an autouse fixture in the test's module, its class or a `conftest.py` up
//!   the directory chain;
//! - a fixture the test names as a parameter, or a `usefixtures` mark names
//!   (test, class or `pytestmark`), and every fixture those request;
//! - a `setUp` / `setup_method` style hook or a class decorator of the test's
//!   class, a module setup hook, a `pytest_*` hook, or top-level code of the
//!   test's module or a chain conftest.
//!
//! The read is a closed allow-list: reachable code that uses any substitution
//! form (the test-body forms, plus builtin `setattr`, `patch.dict`,
//! `sys.modules`, `reload`, `__dict__` and assignment to an attribute of an
//! imported module) keeps the limit, because the read cannot prove the target
//! unrelated to the owner. Fixtures that only create data, change directory,
//! set environment variables or the like use none of these forms and keep
//! their credit. A helper a fixture calls that itself substitutes is not
//! followed (a limit of this read, not a claim of absence).

use super::fixtures::conftest_chain;
use super::module_constants::mentions_identifier;
use super::owners_tests::decorator_names;
use super::source_utils::text_for_range;
use super::static_limits::text_has_mocked_module;
use super::{PythonImport, PythonTest};
use rustpython_parser::ast::{Constant, Expr, Ranged, Stmt};
use rustpython_parser::text_size::TextRange;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Names a `usefixtures` mark with a non-literal argument may request: every
/// substituting fixture in scope is then reachable.
const ANY_FIXTURE: &str = "*";

/// Setup hooks a class runs around each test (`unittest` and xunit styles).
const CLASS_SETUP_HOOKS: [&str; 8] = [
    "setUp",
    "setUpClass",
    "asyncSetUp",
    "setup_method",
    "setup_class",
    "setup",
    "asyncSetUpClass",
    "setUpTestData",
];

/// Setup hooks a module runs around its tests.
const MODULE_SETUP_HOOKS: [&str; 5] = [
    "setup_module",
    "setUpModule",
    "setup_function",
    "setup",
    "asyncSetUpModule",
];

/// One fixture def of a file, at module level or in a class.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct SubstitutionFixture {
    /// Public name and bound name (they differ under `name=`).
    names: Vec<String>,
    /// The class that defines it; only that class's tests can reach it.
    class: Option<String>,
    autouse: bool,
    /// Its text uses a substitution form.
    patches: bool,
    /// Fixtures it requests, by parameter or `usefixtures`.
    requests: Vec<String>,
}

/// A `usefixtures` mark and where it applies.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct UseFixturesMark {
    /// `None` for `pytestmark`, a class name, or a qualified test name.
    target: MarkTarget,
    names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum MarkTarget {
    Module,
    Class(String),
    Test(String),
}

/// What one file contributes to the substitution read.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct SubstitutionScope {
    fixtures: Vec<SubstitutionFixture>,
    /// Top-level code, a module setup hook or a `pytest_*` hook substitutes:
    /// every test of this file, or below this conftest, runs under it.
    ambient: bool,
    /// Classes whose decorators or setup hooks substitute.
    patching_classes: Vec<String>,
    marks: Vec<UseFixturesMark>,
}

/// Read one file's fixtures, hooks and marks.
pub(super) fn collect_substitution_scope(
    source: &str,
    statements: &[Stmt],
    module_imports: &[PythonImport],
) -> SubstitutionScope {
    let aliases = fixture_aliases(module_imports);
    let ctx = ReadContext {
        source,
        imports: module_imports,
        aliases: &aliases,
    };
    let mut scope = SubstitutionScope::default();
    let defs: Vec<&str> = statements
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::FunctionDef(function) => Some(function.name.as_str()),
            _ => None,
        })
        .collect();
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => read_function(
                &ctx,
                FunctionShape {
                    range: stmt.range(),
                    name: function.name.as_str(),
                    decorators: &function.decorator_list,
                    args: &function.args,
                },
                None,
                &mut scope,
            ),
            Stmt::AsyncFunctionDef(function) => read_function(
                &ctx,
                FunctionShape {
                    range: stmt.range(),
                    name: function.name.as_str(),
                    decorators: &function.decorator_list,
                    args: &function.args,
                },
                None,
                &mut scope,
            ),
            Stmt::ClassDef(class) => {
                let name = class.name.to_string();
                let header = decorators_text(source, &class.decorator_list);
                if text_substitutes(
                    &decorator_names(&class.decorator_list),
                    &header,
                    module_imports,
                ) {
                    scope.patching_classes.push(name.clone());
                }
                let mark_names = usefixtures_names(&class.decorator_list, source);
                if !mark_names.is_empty() {
                    scope.marks.push(UseFixturesMark {
                        target: MarkTarget::Class(name.clone()),
                        names: mark_names,
                    });
                }
                for member in &class.body {
                    match member {
                        Stmt::FunctionDef(function) => read_function(
                            &ctx,
                            FunctionShape {
                                range: member.range(),
                                name: function.name.as_str(),
                                decorators: &function.decorator_list,
                                args: &function.args,
                            },
                            Some(&name),
                            &mut scope,
                        ),
                        Stmt::AsyncFunctionDef(function) => read_function(
                            &ctx,
                            FunctionShape {
                                range: member.range(),
                                name: function.name.as_str(),
                                decorators: &function.decorator_list,
                                args: &function.args,
                            },
                            Some(&name),
                            &mut scope,
                        ),
                        _ => {}
                    }
                }
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => {}
            other => {
                let text = text_for_range(source, other.range());
                let names = usefixtures_names_in_text(&text);
                if !names.is_empty() {
                    scope.marks.push(UseFixturesMark {
                        target: MarkTarget::Module,
                        names,
                    });
                }
                // `Mod.attr = stand_in` where `stand_in` is a plain `def` of
                // this same file spells its replacement in the test module
                // itself; the existing replaced-owner reading classifies it
                // (RIPR-SPEC-0108 pins that class), so only the other
                // substitution forms count as ambient here.
                let spelled = spelled_standin_assignment(other, &defs, module_imports);
                scope.ambient |= if spelled {
                    text_substitutes_except_attribute(&text, module_imports)
                } else {
                    text_substitutes(&[], &text, module_imports)
                };
            }
        }
    }
    // A class's setup hook or a fixture it defines patching marks the class;
    // `read_function` records those on `patching_classes` and `fixtures`.
    scope.patching_classes.sort();
    scope.patching_classes.dedup();
    scope
}

struct ReadContext<'a> {
    source: &'a str,
    imports: &'a [PythonImport],
    aliases: &'a [String],
}

struct FunctionShape<'a> {
    range: TextRange,
    name: &'a str,
    decorators: &'a [Expr],
    args: &'a rustpython_parser::ast::Arguments,
}

fn read_function(
    ctx: &ReadContext<'_>,
    function: FunctionShape<'_>,
    class: Option<&str>,
    scope: &mut SubstitutionScope,
) {
    let ReadContext {
        source,
        imports: module_imports,
        aliases,
    } = *ctx;
    let FunctionShape {
        range,
        name,
        decorators,
        args,
    } = function;
    // A def's range starts at `def`; its decorators come first.
    let start = decorators
        .iter()
        .map(|decorator| decorator.range().start())
        .chain(std::iter::once(range.start()))
        .min()
        .unwrap_or_else(|| range.start());
    let text = text_for_range(source, TextRange::new(start, range.end()));
    let names = decorator_names(decorators);
    let patches = text_substitutes(&names, &text, module_imports);
    let mark_names = usefixtures_names(decorators, source);
    let fixture_decorator = decorators.iter().find(|decorator| {
        decorator_names(std::slice::from_ref(*decorator))
            .iter()
            .any(|decorator_name| is_fixture_decorator_name(decorator_name, aliases))
    });
    if let Some(decorator) = fixture_decorator {
        let decorator_text = text_for_range(source, decorator.range()).replace(' ', "");
        let mut public = vec![name.to_string()];
        if let Some(renamed) = renamed_to(decorator) {
            public.push(renamed);
        } else if decorator_text.contains("name=") {
            // A non-literal `name=`: the public name is unknown, so the
            // fixture answers to every request.
            public.push(ANY_FIXTURE.to_string());
        }
        let mut requests = parameter_names(args);
        requests.extend(mark_names);
        scope.fixtures.push(SubstitutionFixture {
            names: public,
            class: class.map(str::to_string),
            autouse: decorator_text.contains("autouse=")
                && !decorator_text.contains("autouse=False"),
            patches,
            requests,
        });
        return;
    }
    if !mark_names.is_empty() {
        scope.marks.push(UseFixturesMark {
            target: MarkTarget::Test(match class {
                Some(class) => format!("{class}.{name}"),
                None => name.to_string(),
            }),
            names: mark_names,
        });
    }
    match class {
        Some(class) => {
            if patches && CLASS_SETUP_HOOKS.contains(&name) {
                scope.patching_classes.push(class.to_string());
            }
        }
        None => {
            // A `pytest_*` hook can run around every test below a conftest;
            // a module setup hook around every test of the file.
            if patches && (name.starts_with("pytest_") || MODULE_SETUP_HOOKS.contains(&name)) {
                scope.ambient = true;
            }
        }
    }
}

fn parameter_names(args: &rustpython_parser::ast::Arguments) -> Vec<String> {
    args.posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
        .map(|arg| arg.def.arg.to_string())
        .filter(|name| !matches!(name.as_str(), "self" | "cls" | "request"))
        .collect()
}

fn decorators_text(source: &str, decorators: &[Expr]) -> String {
    decorators
        .iter()
        .map(|decorator| text_for_range(source, decorator.range()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_fixture_decorator_name(name: &str, aliases: &[String]) -> bool {
    name == "fixture" || name.ends_with(".fixture") || aliases.iter().any(|alias| alias == name)
}

fn fixture_aliases(imports: &[PythonImport]) -> Vec<String> {
    imports
        .iter()
        .filter(|import| {
            matches!(import.source_module.as_str(), "pytest" | "pytest_asyncio")
                && import.imported == "fixture"
        })
        .map(|import| import.alias.clone())
        .collect()
}

/// The literal `name="x"` keyword of a fixture decorator.
fn renamed_to(decorator: &Expr) -> Option<String> {
    let Expr::Call(call) = decorator else {
        return None;
    };
    call.keywords.iter().find_map(|keyword| {
        let arg = keyword.arg.as_ref()?;
        if arg.as_str() != "name" {
            return None;
        }
        match &keyword.value {
            Expr::Constant(constant) => match &constant.value {
                Constant::Str(text) => Some(text.clone()),
                _ => None,
            },
            _ => None,
        }
    })
}

/// Names the `usefixtures` marks among `decorators` request; a non-literal
/// argument requests [`ANY_FIXTURE`].
fn usefixtures_names(decorators: &[Expr], source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for decorator in decorators {
        names.extend(usefixtures_names_in_text(&text_for_range(
            source,
            decorator.range(),
        )));
    }
    names
}

fn usefixtures_names_in_text(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("usefixtures(") {
        let after = &rest[at + "usefixtures(".len()..];
        let end = after.find(')').unwrap_or(after.len());
        let args = &after[..end];
        let mut remainder = String::new();
        let mut quote: Option<char> = None;
        let mut current = String::new();
        for ch in args.chars() {
            match quote {
                Some(open) if ch == open => {
                    names.push(std::mem::take(&mut current));
                    quote = None;
                }
                Some(_) => current.push(ch),
                None if ch == '"' || ch == '\'' => quote = Some(ch),
                None if ch == ',' || ch.is_whitespace() => {}
                None => remainder.push(ch),
            }
        }
        if !remainder.is_empty() || quote.is_some() {
            names.push(ANY_FIXTURE.to_string());
        }
        rest = &after[end..];
    }
    // `usefixtures` named without a call (`mark.usefixtures` alone) or any
    // shape the scan above did not read.
    if names.is_empty() && mentions_identifier(text, "usefixtures") {
        names.push(ANY_FIXTURE.to_string());
    }
    names
}

/// Whether `text` uses a substitution form: the test-body forms the
/// `mocked_module` limit reads, plus the forms a fixture or hook uses to
/// replace a module attribute without a mock library.
fn text_substitutes(decorators: &[String], text: &str, imports: &[PythonImport]) -> bool {
    text_has_mocked_module(decorators, text, imports)
        || ["setattr", "reload"]
            .iter()
            .any(|name| call_at_name_boundary(text, name))
        || text.contains("patch.dict(")
        || text.contains("sys.modules")
        || text.contains("__dict__")
        || assigns_imported_attribute(text, imports)
}

/// Whether `text` uses a substitution form other than assigning an attribute
/// of an imported name.
fn text_substitutes_except_attribute(text: &str, imports: &[PythonImport]) -> bool {
    let attribute_lines_removed: String = text
        .lines()
        .filter(|line| !assigns_imported_attribute(line, imports))
        .collect::<Vec<_>>()
        .join("\n");
    text_substitutes(&[], &attribute_lines_removed, imports)
}

/// A top-level `Imported.attr = name` whose value is a plain module-level
/// `def` of the same file.
fn spelled_standin_assignment(stmt: &Stmt, defs: &[&str], imports: &[PythonImport]) -> bool {
    let Stmt::Assign(assign) = stmt else {
        return false;
    };
    let [Expr::Attribute(target)] = assign.targets.as_slice() else {
        return false;
    };
    let Expr::Name(root) = target.value.as_ref() else {
        return false;
    };
    let Expr::Name(value) = assign.value.as_ref() else {
        return false;
    };
    imports.iter().any(|import| import.alias == root.id.as_str())
        && defs.contains(&value.id.as_str())
}

/// `name(` where `name` starts at an identifier boundary or follows a dot.
fn call_at_name_boundary(text: &str, name: &str) -> bool {
    let call = format!("{name}(");
    text.match_indices(&call).any(|(idx, _)| {
        !text[..idx]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

/// A line assigning an attribute of an imported name (`calc.price = fake`).
fn assigns_imported_attribute(text: &str, imports: &[PythonImport]) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        let Some(eq) = line.find('=') else {
            return false;
        };
        let after = line[eq + 1..].chars().next();
        let before = line[..eq].chars().next_back();
        if after == Some('=') || matches!(before, Some('=' | '!' | '<' | '>')) {
            return false;
        }
        let target = line[..eq].trim_end_matches(|ch: char| "+-*/%&|^@:".contains(ch));
        let target = target.trim();
        let Some((root, _)) = target.split_once('.') else {
            return false;
        };
        target
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
            && imports.iter().any(|import| import.alias == root)
    })
}

/// Set `fixture_substitution` on every test that runs under a fixture, hook
/// or setup that substitutes: see the module documentation for what counts as
/// reachable.
pub(super) fn apply_fixture_substitution(
    tests: &mut [PythonTest],
    scopes: &BTreeMap<PathBuf, SubstitutionScope>,
    mut is_unreadable_conftest: impl FnMut(&Path) -> bool,
) {
    for test in tests {
        // A chain conftest that exists but was never read (capped, excluded
        // or unreadable) may hold a patching autouse fixture: fail closed.
        let unread = conftest_chain(&test.file).any(|conftest| is_unreadable_conftest(&conftest));
        test.constant_rebinding.fixture_substitution =
            unread || (!scopes.is_empty() && test_runs_under_substitution(test, scopes));
    }
}

fn test_runs_under_substitution(
    test: &PythonTest,
    scopes: &BTreeMap<PathBuf, SubstitutionScope>,
) -> bool {
    let chain: Vec<&SubstitutionScope> = std::iter::once(test.file.clone())
        .chain(conftest_chain(&test.file))
        .filter_map(|file| scopes.get(&file))
        .collect();
    if chain.is_empty() {
        return false;
    }
    if chain.iter().any(|scope| scope.ambient) {
        return true;
    }
    let class = test
        .qualified_name
        .rsplit_once('.')
        .map(|(class, _)| class.to_string());
    let own = scopes.get(&test.file);
    if let (Some(class), Some(own)) = (&class, own)
        && own
            .patching_classes
            .iter()
            .any(|patching| patching == class)
    {
        return true;
    }
    // Fixtures the test is built on: autouse ones in reach, its parameters
    // and the `usefixtures` marks that apply to it.
    let reaches = |fixture: &SubstitutionFixture, in_own_file: bool| match &fixture.class {
        None => true,
        Some(owner) => in_own_file && class.as_deref() == Some(owner.as_str()),
    };
    let mut wanted: Vec<String> = test.fixtures.clone();
    for (index, scope) in chain.iter().enumerate() {
        let in_own_file = own.is_some_and(|own| std::ptr::eq(own, *scope)) && index == 0;
        for fixture in &scope.fixtures {
            if fixture.autouse && reaches(fixture, in_own_file) {
                wanted.push(fixture.names[0].clone());
            }
        }
    }
    // A module-level `pytestmark` in a chain conftest applies to every test
    // below it; class and test targets only exist in the test's own file.
    for scope in &chain {
        let is_own = own.is_some_and(|own| std::ptr::eq(own, *scope));
        for mark in &scope.marks {
            let applies = match &mark.target {
                MarkTarget::Module => true,
                MarkTarget::Class(name) => is_own && class.as_deref() == Some(name.as_str()),
                MarkTarget::Test(name) => is_own && *name == test.qualified_name,
            };
            if applies {
                wanted.extend(mark.names.iter().cloned());
            }
        }
    }
    let mut seen: Vec<String> = Vec::new();
    while let Some(name) = wanted.pop() {
        if seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        for (index, scope) in chain.iter().enumerate() {
            let in_own_file = index == 0 && own.is_some_and(|own| std::ptr::eq(own, *scope));
            for fixture in &scope.fixtures {
                let answers = name == ANY_FIXTURE
                    || fixture
                        .names
                        .iter()
                        .any(|candidate| *candidate == name || candidate == ANY_FIXTURE);
                if !answers || !reaches(fixture, in_own_file) {
                    continue;
                }
                if fixture.patches {
                    return true;
                }
                wanted.extend(fixture.requests.iter().cloned());
            }
        }
    }
    false
}
