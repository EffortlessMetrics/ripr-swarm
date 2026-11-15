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
//! dynamic namespace or frame write, a non-ASCII character, the name
//! spelled as an attribute anywhere (`tests.tomllib = other` replaces the
//! package attribute), or any statement besides a docstring, `pass`, a
//! literal `__all__` or an import of a non-workspace module refuses. Any
//! other read workspace module whose code can reach the package object or
//! the import machinery (it names the package, spells that attribute or a
//! class swap, has a reference writer, `__import__`, `builtins`, a
//! `mock.patch`, or a string naming the target) refuses too, as
//! does a submodule or subpackage of the same name, which another import
//! could load over the binding. Chains through further package
//! initializers stop after [`MAX_ALIAS_HOPS`].

use super::owners_tests::import_source_module;
use super::reexports::{import_binding, is_star_import, stmt_binds, writes_namespace_dynamically};
use super::source_facts::parse_module_result;
use super::{PythonImport, PythonTest};
use rustpython_parser::ast::{self, Mod, Stmt};
use rustpython_parser::lexer::lex;
use rustpython_parser::{Mode, StringKind, Tok};
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
        || identifier_tokens(text).any(|token| token == "global" || writer_token(token))
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

/// Whether another read workspace module (a test module, a conftest, a
/// helper on the root or under `src`) can replace the package attribute
/// `name` when it runs. Modules the bounded read skipped are not scanned.
fn replaced_elsewhere(
    package: &str,
    name: &str,
    init: &Path,
    sources: &HashMap<PathBuf, String>,
) -> bool {
    sources.iter().any(|(path, text)| {
        path != init
            && path.extension().is_some_and(|ext| ext == "py")
            && module_may_replace(text, name, package)
    })
}

/// Reads one module's code tokens, so a mention in a comment or a plain
/// string (tomli's `'tomli._re' not in sys.modules` inside a subprocess
/// script) is not code. It refuses on any way this module can reach and
/// rewrite the package object or the import machinery:
///
/// - code that spells `.name`, `.__class__` (a module class swap) or a
///   non-ASCII attribute (which Python may NFKC-fold to `name`);
/// - any reference writer (`replaces_by_reference`), whatever it names,
///   since its target can be computed (`sys.modules[__package__]`);
/// - `__import__`, `builtins`, `meta_path`, `path_hooks`, `reload` or
///   `patch`, which replace what an import returns;
/// - the package referenced by name (any part of its dotted spelling as an
///   identifier, as in `import tests`), outside a `from ... import` path;
/// - a string that is `name`, the package or a dotted path under it (a
///   `mock.patch("tests.tomllib")` target), or that spells a writer
///   (`getattr(m, "__setattr__")`);
/// - an f-string, whose fields run, that the initializer text rule refuses.
///
/// A module that does not lex refuses.
fn module_may_replace(text: &str, name: &str, package: &str) -> bool {
    let Ok(tokens) = lex(text, Mode::Module).collect::<Result<Vec<_>, _>>() else {
        return true;
    };
    let parts: Vec<&str> = package.split('.').collect();
    let first = parts.first().copied().unwrap_or_default();
    let mut after_dot = false;
    // The module path of `from tests import X` names the package without
    // binding it, so only an identifier outside that path is a reference.
    let mut from_path = false;
    for (token, _) in tokens {
        let dot = matches!(token, Tok::Dot);
        match token {
            Tok::From => from_path = true,
            // `raise E from e` has no `import`; a logical line ends either.
            Tok::Import | Tok::Newline => from_path = false,
            _ => {}
        }
        let replaces = match token {
            Tok::Name { name: ident } => {
                (after_dot && (ident == name || ident == "__class__" || !ident.is_ascii()))
                    || replaces_by_reference(&ident)
                    || matches!(
                        ident.as_str(),
                        "__import__"
                            | "builtins"
                            | "__builtins__"
                            | "meta_path"
                            | "path_hooks"
                            | "reload"
                            | "patch"
                    )
                    || (!from_path && parts.contains(&ident.as_str()))
            }
            Tok::String { value, kind, .. } => {
                value == name
                    || value == first
                    || value.starts_with(&format!("{first}."))
                    || ["setattr", "delattr", "__dict__", "__class__", "__import__"]
                        .iter()
                        .any(|part| value.contains(part))
                    || (matches!(kind, StringKind::FString | StringKind::RawFString)
                        && (rebinds_dynamically(&value) || names_attribute(&value, name)))
            }
            _ => false,
        };
        if replaces {
            return true;
        }
        after_dot = dot;
    }
    false
}

/// A token through which another module can write the package's namespace
/// by a computed name or swap the package itself: `setattr`, `delattr`,
/// `__dict__`, `vars`, `globals`/`locals`, a frame, `exec`/`eval` of a
/// code string, or the `sys.modules` table. An import (`importlib`) only
/// runs another module, which is scanned on its own.
fn replaces_by_reference(token: &str) -> bool {
    matches!(token, "vars" | "exec" | "eval" | "modules")
        || writes_through_namespace(token)
        || token.contains("frame")
}

/// The identifier tokens `rebinds_dynamically` refuses in an initializer.
fn writer_token(token: &str) -> bool {
    matches!(
        token,
        "vars" | "exec" | "eval" | "__getattr__" | "modules" | "__import__"
    ) || writes_through_namespace(token)
        || ["frame", "import_module", "importlib", "runpy"]
            .iter()
            .any(|part| token.contains(part))
}

/// Whether every top-level statement other than `binder` is inert: a
/// docstring, `pass`, an `__all__` of string literals, or a plain import of
/// a module that is not in the workspace (at the root or under `src`). Any
/// other statement (a call, a `def` or `class` whose body may run, a
/// relative import, an import of a workspace module) runs code that can
/// write the package attribute, so the initializer is not read.
fn only_inert_besides(
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
    let string = |expr: &ast::Expr| matches!(expr, ast::Expr::Constant(value) if matches!(value.value, ast::Constant::Str(_)));
    body.iter().all(|stmt| {
        if std::ptr::eq(stmt, binder) {
            return true;
        }
        match stmt {
            Stmt::Pass(_) => true,
            Stmt::Expr(expr) => string(&expr.value),
            Stmt::Assign(assign) => {
                let all = matches!(assign.targets.as_slice(),
                    [ast::Expr::Name(target)] if target.id.as_str() == "__all__");
                let literal = match assign.value.as_ref() {
                    ast::Expr::Tuple(tuple) => tuple.elts.iter().all(string),
                    ast::Expr::List(list) => list.elts.iter().all(string),
                    _ => false,
                };
                all && literal
            }
            Stmt::Import(import) => import
                .names
                .iter()
                .all(|alias| !in_workspace(alias.name.as_str())),
            Stmt::ImportFrom(import) => {
                let level = import.level.as_ref().map_or(0, |level| level.to_usize());
                let source = import_source_module(init, import);
                level == 0 && !source.is_empty() && !in_workspace(&source) && !is_star_import(stmt)
            }
            _ => false,
        }
    })
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
    // Another workspace module that spells the name as an attribute or
    // reaches it through a namespace writer can replace the package
    // attribute after the initializer ran.
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
    if !only_inert_besides(&module.body, binder, &init, is_present) {
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
