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
//! Only a package that contains the test file is followed, so the
//! dotted name always names the package Python imports for that test.
//! It fails closed. The initializer must be read and parse, and bind the
//! name exactly once, through one unconditional top-level `import` or
//! `from ... import`. Any other binding of the name, a star import, a
//! `match` or `type` statement, a module `__getattr__`, `global`, a walrus
//! or a dynamic namespace write refuses, as
//! does a submodule or subpackage of the same name, which another import
//! could load over the binding. Chains through further package
//! initializers stop after [`MAX_ALIAS_HOPS`].

use super::owners_tests::import_source_module;
use super::reexports::{is_star_import, stmt_binds, writes_namespace_dynamically};
use super::source_facts::parse_module_result;
use super::{PythonImport, PythonTest};
use rustpython_parser::ast::{Mod, Stmt};
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
        || text
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|token| {
                matches!(
                    token,
                    "locals" | "global" | "eval" | "__getattr__" | "modules"
                )
            })
}

/// Rewrites every test import of a name its package initializer binds by
/// import. `source` answers for a read workspace file and `is_present` for
/// any workspace-relative path.
pub(super) fn apply_package_import_aliases<'s>(
    tests: &mut [PythonTest],
    source: impl Fn(&Path) -> Option<&'s str>,
    is_present: impl Fn(&Path) -> bool,
) {
    let mut bindings: HashMap<(String, String), Option<PythonImport>> = HashMap::new();
    for test in tests {
        let file = test.file.clone();
        for import in &mut test.imports {
            if let Some(resolved) = resolve(&file, import, &source, &is_present, &mut bindings) {
                *import = resolved;
            }
        }
    }
}

/// Only a package containing the test file is followed: a dotted name is
/// read from the workspace root, and in a nested project that spelling can
/// name another package than the one Python imports for this test.
fn resolve<'s>(
    test_file: &Path,
    import: &PythonImport,
    source: &impl Fn(&Path) -> Option<&'s str>,
    is_present: &impl Fn(&Path) -> bool,
    bindings: &mut HashMap<(String, String), Option<PythonImport>>,
) -> Option<PythonImport> {
    let mut current: Option<PythonImport> = None;
    for _ in 0..MAX_ALIAS_HOPS {
        let step = current.as_ref().unwrap_or(import);
        if step.source_module.is_empty()
            || step.imported == "*"
            || !test_file.starts_with(package_directory(&step.source_module))
        {
            break;
        }
        let key = (step.source_module.clone(), step.imported.clone());
        let bound = bindings
            .entry(key)
            .or_insert_with(|| {
                initializer_binding(&step.source_module, &step.imported, source, is_present)
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

fn package_directory(package: &str) -> PathBuf {
    package.split('.').collect()
}

/// The import through which package `package`'s initializer binds `name`,
/// when that binding is the only one and nothing else can replace it.
fn initializer_binding<'s>(
    package: &str,
    name: &str,
    source: &impl Fn(&Path) -> Option<&'s str>,
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
    let text = source(&init)?;
    if rebinds_dynamically(text) {
        return None;
    }
    let Ok(Mod::Module(module)) = parse_module_result(&init, text) else {
        return None;
    };
    // A `match` capture and a `type` alias bind names the binder scan
    // below does not read.
    if module
        .body
        .iter()
        .any(|stmt| is_star_import(stmt) || matches!(stmt, Stmt::Match(_) | Stmt::TypeAlias(_)))
    {
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
    match binder {
        Stmt::Import(import) => {
            let mut aliases = import
                .names
                .iter()
                .filter(|alias| alias.asname.as_ref().unwrap_or(&alias.name).as_str() == name);
            let alias = aliases.next()?;
            // `import a.b` binds `a`, so a dotted name is never `name` here;
            // a second alias of the same name keeps only the last.
            if aliases.next().is_some() {
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
