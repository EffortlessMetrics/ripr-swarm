//! Names a test package's `__init__.py` binds by import (#13927).
//!
//! tomli's suite binds the package under another name in the test package
//! and imports that name relatively:
//!
//! ```python
//! # tests/__init__.py
//! import tomli as tomllib
//! # tests/test_misc.py
//! from . import tomllib
//! ```
//!
//! `from . import tomllib` reads as the module `tests.tomllib`, which does
//! not exist, so the test loses its link to the code. Python returns the
//! package attribute the initializer bound, which is the `tomli` module.
//! This pass rewrites such an import to the import the initializer
//! performed (`import tomli as tomllib`), so every later reader sees the
//! same evidence it would see had the test imported the module directly.
//!
//! Only a package of the test file's own package chain is followed (its
//! directory and each parent while it has an `__init__.py`), and only when
//! the spelling's first name appears once on the test's directory path, so
//! under pytest's default prepend import mode the dotted name names the
//! package Python imports for that test. It fails closed. The initializer
//! must be read and parse, and bind the name exactly once, through one
//! unconditional top-level `import` or `from ... import`. Any other binding
//! of the name, a star import, a `match` or `type` statement at any depth
//! the module executes, a module `__getattr__`, `global`, a walrus, a
//! dynamic namespace write or the name spelled as an attribute anywhere
//! (`tests.tomllib = other` replaces the package attribute) refuses, as
//! does a submodule or subpackage of the same name, which another import
//! could load over the binding. Chains through further package
//! initializers stop after [`MAX_ALIAS_HOPS`].

use super::owners_tests::import_source_module;
use super::reexports::{import_binding, is_star_import, stmt_binds, writes_namespace_dynamically};
use super::source_facts::parse_module_result;
use super::{PythonImport, PythonTest};
use rustpython_parser::ast::{self, Mod, Stmt};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Maximum number of package initializers followed for one import.
const MAX_ALIAS_HOPS: usize = 3;

/// Whether the initializer can rebind a module global outside the
/// statements this reader checks. The shared owner scans identifier tokens,
/// so whitespace (`vars ()`) cannot hide a call; this rule also refuses
/// `locals`, `global`, `eval`, a module `__getattr__`, any `modules` name
/// (`from sys import modules`) and a walrus. Any mention, even in a comment
/// or string, refuses.
fn rebinds_dynamically(text: &str) -> bool {
    writes_namespace_dynamically(text)
        || text.contains(":=")
        || identifier_tokens(text).any(|token| {
            matches!(
                token,
                "global" | "eval" | "__getattr__" | "modules" | "__import__"
            ) || writes_through_namespace(token)
                || ["frame", "import_module", "importlib", "runpy"]
                    .iter()
                    .any(|part| token.contains(part))
        })
}

fn identifier_tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
}

/// A token that can write a namespace entry by a computed name:
/// `setattr`/`__setattr__`, `delattr`, `globals`, `locals`, a frame's
/// `f_globals`/`f_locals`, or `__dict__`.
fn writes_through_namespace(token: &str) -> bool {
    ["setattr", "delattr", "globals", "locals", "__dict__"]
        .iter()
        .any(|part| token.contains(part))
}

/// Whether a read module of the test tree (the directory of the package's
/// first name) or any conftest, other than the initializer itself, can
/// replace the package attribute `name`: it spells `.name` (or a non-ASCII
/// attribute, which Python may fold to `name`), or names `name` next to a
/// namespace writer such as `setattr`.
fn replaced_elsewhere(
    package: &str,
    name: &str,
    init: &Path,
    sources: &HashMap<PathBuf, String>,
) -> bool {
    let Some(first) = package.split('.').next() else {
        return true;
    };
    sources.iter().any(|(path, text)| {
        let in_scope =
            path.starts_with(first) || path.file_name().is_some_and(|f| f == "conftest.py");
        in_scope
            && path != init
            && path.extension().is_some_and(|ext| ext == "py")
            && (names_attribute(text, name)
                || (identifier_tokens(text).any(writes_through_namespace)
                    && identifier_tokens(text).any(|token| token == name)))
    })
}

/// Whether any import statement other than `binder`, at any depth the
/// module executes, is relative or names a module present in the workspace
/// (at the root or under `src`).
fn imports_workspace_module(
    body: &[Stmt],
    binder: &Stmt,
    init: &Path,
    is_present: &impl Fn(&Path) -> bool,
) -> bool {
    let in_workspace = |dotted: &str| {
        let path: PathBuf = dotted.split('.').collect();
        [PathBuf::new(), PathBuf::from("src")].iter().any(|base| {
            let module = base.join(&path);
            is_present(&module.with_extension("py")) || is_present(&module.join("__init__.py"))
        })
    };
    let mut pending: Vec<&Stmt> = body.iter().collect();
    while let Some(stmt) = pending.pop() {
        if std::ptr::eq(stmt, binder) {
            continue;
        }
        match stmt {
            Stmt::Import(import) => {
                if import
                    .names
                    .iter()
                    .any(|alias| in_workspace(alias.name.as_str()))
                {
                    return true;
                }
            }
            Stmt::ImportFrom(import) => {
                let level = import.level.as_ref().map_or(0, |level| level.to_usize());
                if level > 0 {
                    return true;
                }
                let source = import_source_module(init, import);
                if source.is_empty() || in_workspace(&source) {
                    return true;
                }
            }
            Stmt::If(s) => pending.extend(s.body.iter().chain(&s.orelse)),
            Stmt::For(s) => pending.extend(s.body.iter().chain(&s.orelse)),
            Stmt::AsyncFor(s) => pending.extend(s.body.iter().chain(&s.orelse)),
            Stmt::While(s) => pending.extend(s.body.iter().chain(&s.orelse)),
            Stmt::With(s) => pending.extend(&s.body),
            Stmt::AsyncWith(s) => pending.extend(&s.body),
            Stmt::Try(s) => {
                pending.extend(s.body.iter().chain(&s.orelse).chain(&s.finalbody));
                for handler in &s.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    pending.extend(&handler.body);
                }
            }
            Stmt::TryStar(s) => {
                pending.extend(s.body.iter().chain(&s.orelse).chain(&s.finalbody));
                for handler in &s.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    pending.extend(&handler.body);
                }
            }
            _ => {}
        }
    }
    false
}

/// Whether `name` appears as an attribute (`.name`, whitespace or line
/// continuations allowed after the dot) anywhere in `text`, comments and
/// strings included, so any attribute write of that name refuses.
fn names_attribute(text: &str, name: &str) -> bool {
    text.match_indices('.').any(|(dot, _)| {
        let rest = text[dot + 1..].trim_start_matches(|c: char| c.is_whitespace() || c == '\\');
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let attribute = &rest[..end];
        attribute == name || !attribute.is_ascii()
    })
}

/// Rewrites every test import of a name its package initializer binds by
/// import. `sources` holds every read workspace file and `is_present`
/// answers for any workspace-relative path.
pub(super) fn apply_package_import_aliases(
    tests: &mut [PythonTest],
    sources: &HashMap<PathBuf, String>,
    is_present: impl Fn(&Path) -> bool,
) {
    let mut bindings: HashMap<(String, String), Option<PythonImport>> = HashMap::new();
    for test in tests {
        let file = test.file.clone();
        for import in &mut test.imports {
            if let Some(resolved) = resolve(&file, import, sources, &is_present, &mut bindings) {
                *import = resolved;
            }
        }
    }
}

/// Only a package of the test file's own package chain is followed: the
/// directories from the test's directory upward while each has an
/// `__init__.py`. A dotted name is read from the workspace root, and pytest
/// imports a test package from the parent of its topmost package, so a
/// root-relative spelling names the package Python imports only when it is
/// one of those chain directories. A spelling whose first part also names
/// another directory on the test's path (`tests/project/tests/...`) is
/// ambiguous under another import root and refuses.
fn resolve(
    test_file: &Path,
    import: &PythonImport,
    sources: &HashMap<PathBuf, String>,
    is_present: &impl Fn(&Path) -> bool,
    bindings: &mut HashMap<(String, String), Option<PythonImport>>,
) -> Option<PythonImport> {
    let mut current: Option<PythonImport> = None;
    for _ in 0..MAX_ALIAS_HOPS {
        let step = current.as_ref().unwrap_or(import);
        if step.source_module.is_empty()
            || step.imported == "*"
            || !in_own_package_chain(test_file, &step.source_module, is_present)
        {
            break;
        }
        let key = (step.source_module.clone(), step.imported.clone());
        let bound = bindings
            .entry(key)
            .or_insert_with(|| {
                initializer_binding(&step.source_module, &step.imported, sources, is_present)
            })
            .clone();
        let Some(mut bound) = bound else {
            break;
        };
        // The test's own local name is kept: `from . import tomllib as t`
        // binds `t` to whatever the initializer bound as `tomllib`.
        bound.alias = import.alias.clone();
        current = Some(bound);
    }
    current
}

fn in_own_package_chain(
    test_file: &Path,
    package: &str,
    is_present: &impl Fn(&Path) -> bool,
) -> bool {
    let directory = package_directory(package);
    let Some(first) = package.split('.').next() else {
        return false;
    };
    let Some(parent) = test_file.parent() else {
        return false;
    };
    let repeats = parent
        .components()
        .filter(|component| component.as_os_str() == first)
        .count();
    repeats == 1
        && parent
            .ancestors()
            .take_while(|dir| !dir.as_os_str().is_empty() && is_present(&dir.join("__init__.py")))
            .any(|dir| dir == directory)
}

fn package_directory(package: &str) -> PathBuf {
    package.split('.').collect()
}

/// The import through which package `package`'s initializer binds `name`,
/// when that binding is the only one and nothing else can replace it.
fn initializer_binding(
    package: &str,
    name: &str,
    sources: &HashMap<PathBuf, String>,
    is_present: &impl Fn(&Path) -> bool,
) -> Option<PythonImport> {
    let directory = package_directory(package);
    if package.split('.').any(str::is_empty) || package == "src" {
        return None;
    }
    // A submodule or subpackage of that name can be loaded over the
    // attribute by any import of it, so it is never the initializer's.
    if is_present(&directory.join(format!("{name}.py"))) || is_present(&directory.join(name)) {
        return None;
    }
    let init = directory.join("__init__.py");
    let text = sources.get(&init).map(String::as_str)?;
    // An attribute spelled `name` (`tests.tomllib = other`, through any
    // receiver) can replace the package attribute the test later reads.
    // Identifiers are NFKC-folded by Python (`ｔｏｍｌｌｉｂ` is
    // `tomllib`), so a non-ASCII initializer is not read at all.
    if !text.is_ascii() || rebinds_dynamically(text) || names_attribute(text, name) {
        return None;
    }
    // Another module of the test tree, or a conftest, that spells the name as
    // an attribute or reaches it through a namespace writer can replace the
    // package attribute after the initializer ran.
    if replaced_elsewhere(package, name, &init, sources) {
        return None;
    }
    let Ok(Mod::Module(module)) = parse_module_result(&init, text) else {
        return None;
    };
    // A `match` capture and a `type` alias bind names the binder scan
    // below does not read, at any depth the module body executes.
    if module.body.iter().any(is_star_import) || has_unread_binder(&module.body) {
        return None;
    }
    let mut binders = module
        .body
        .iter()
        .filter(|stmt| stmt_binds(stmt, name, true));
    let binder = binders.next()?;
    if binders.next().is_some() {
        return None;
    }
    // Any other import of a workspace module (the package itself, one of its
    // submodules, or a module on the root or `src`) runs code that can write
    // the package attribute; only the binding import itself is admitted.
    if imports_workspace_module(&module.body, binder, &init, is_present) {
        return None;
    }
    match binder {
        Stmt::Import(import) => {
            // Count by the name Python binds: `import a.b` binds `a`, so
            // `import owner as json, json.tool` leaves `json` the stdlib
            // package and is refused as a second binding.
            let mut aliases = import
                .names
                .iter()
                .filter(|alias| import_binding(alias) == name);
            let alias = aliases.next()?;
            if aliases.next().is_some() {
                return None;
            }
            // An unaliased dotted import binds only its root package, which
            // is not the module the full spelling names.
            if alias.asname.is_none() && alias.name.contains('.') {
                return None;
            }
            Some(PythonImport {
                imported: alias.name.to_string(),
                alias: name.to_string(),
                source_module: String::new(),
            })
        }
        Stmt::ImportFrom(import) => {
            let mut aliases = import
                .names
                .iter()
                .filter(|alias| alias.asname.as_ref().unwrap_or(&alias.name).as_str() == name);
            let alias = aliases.next()?;
            if aliases.next().is_some() {
                return None;
            }
            let source_module = import_source_module(&init, import);
            if source_module.is_empty() {
                return None;
            }
            Some(PythonImport {
                imported: alias.name.to_string(),
                alias: name.to_string(),
                source_module,
            })
        }
        _ => None,
    }
}

/// Whether a module-executed statement, at any nesting outside a `def` or
/// `class` body, is a `match` or `type` statement. Either binds names
/// (capture patterns, the alias) the binder scan does not read.
fn has_unread_binder(body: &[Stmt]) -> bool {
    let handlers = |handlers: &[ast::ExceptHandler]| {
        handlers.iter().any(|handler| {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            has_unread_binder(&handler.body)
        })
    };
    body.iter().any(|stmt| match stmt {
        Stmt::Match(_) | Stmt::TypeAlias(_) => true,
        Stmt::If(s) => has_unread_binder(&s.body) || has_unread_binder(&s.orelse),
        Stmt::For(s) => has_unread_binder(&s.body) || has_unread_binder(&s.orelse),
        Stmt::AsyncFor(s) => has_unread_binder(&s.body) || has_unread_binder(&s.orelse),
        Stmt::While(s) => has_unread_binder(&s.body) || has_unread_binder(&s.orelse),
        Stmt::With(s) => has_unread_binder(&s.body),
        Stmt::AsyncWith(s) => has_unread_binder(&s.body),
        Stmt::Try(s) => {
            has_unread_binder(&s.body)
                || handlers(&s.handlers)
                || has_unread_binder(&s.orelse)
                || has_unread_binder(&s.finalbody)
        }
        Stmt::TryStar(s) => {
            has_unread_binder(&s.body)
                || handlers(&s.handlers)
                || has_unread_binder(&s.orelse)
                || has_unread_binder(&s.finalbody)
        }
        _ => false,
    })
}
