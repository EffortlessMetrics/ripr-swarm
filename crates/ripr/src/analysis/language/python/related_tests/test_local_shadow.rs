//! Test-local modules that shadow an owner's module name (#13706).
//!
//! Under pytest's default prepend import mode the directory of a test file
//! (and, while a directory is a package, each parent up to the first
//! directory that is not, plus the repository root) is on `sys.path` ahead of
//! any project root. A helper module there (`svc/a/tests/core.py`) therefore
//! answers `from core import limit` in `svc/a/tests/test_x.py` instead of the
//! project module `svc/a/core.py`. Rival modules are otherwise collected only
//! from non-test files, so this records the test-local ones.

use super::super::source_utils::is_test_file;
use super::{PythonOwner, owner_module_paths};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A top-level module name of an owner that a test-local module of the same
/// name shadows for the tests in `test_dirs`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::analysis::language::python) struct TestLocalShadow {
    /// The shadowed top-level module name (`core`).
    module: String,
    /// Directories of test files whose `sys.path` holds the shadowing module's
    /// directory ahead of any project root.
    test_dirs: Vec<PathBuf>,
}

/// The directories pytest's prepend import mode puts on `sys.path` for a test
/// in `dir`: `dir` itself and, while a directory is a package
/// (`__init__.py`), each parent up to and including the first directory that
/// is not. The repository root is always included, fail-closed (a root
/// conftest, `python -m pytest` or an editable install reach it).
fn prepend_path_dirs(dir: &Path, files: &BTreeSet<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut current = dir.to_path_buf();
    loop {
        dirs.push(current.clone());
        if current.as_os_str().is_empty() || !files.contains(current.join("__init__.py").as_path())
        {
            break;
        }
        current = current.parent().unwrap_or(Path::new("")).to_path_buf();
    }
    if !dirs.iter().any(|dir| dir.as_os_str().is_empty()) {
        dirs.push(PathBuf::new());
    }
    dirs
}

/// The top-level module a test-local file defines and the directory that
/// holds it: `D/name.py` is `name` in `D`; `D/name/__init__.py` is the
/// package `name` in `D`. Only test-flagged files qualify; a non-test rival is
/// handled by the ambiguity records.
fn test_local_module(file: &Path) -> Option<(String, PathBuf)> {
    if !is_test_file(file) || file.extension()?.to_str()? != "py" {
        return None;
    }
    let stem = file.file_stem()?.to_str()?;
    let parent = file.parent().unwrap_or(Path::new(""));
    if stem == "__init__" {
        let name = parent.file_name()?.to_str()?;
        let grandparent = parent.parent().unwrap_or(Path::new(""));
        return Some((name.to_string(), grandparent.to_path_buf()));
    }
    Some((stem.to_string(), parent.to_path_buf()))
}

/// Records on each owner the top-level names of its module paths that a
/// test-local module shadows, with the test directories affected. `files` is
/// every discovered workspace file, test files included.
pub(in crate::analysis::language::python) fn apply_test_local_shadows<'a>(
    owners: &mut [PythonOwner],
    files: impl Iterator<Item = &'a PathBuf>,
) {
    let files = files.map(PathBuf::as_path).collect::<BTreeSet<_>>();
    let mut local_dirs_by_name: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut test_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for file in &files {
        if !is_test_file(file) {
            continue;
        }
        test_dirs.insert(file.parent().unwrap_or(Path::new("")).to_path_buf());
        if let Some((name, dir)) = test_local_module(file) {
            local_dirs_by_name.entry(name).or_default().insert(dir);
        }
    }
    if local_dirs_by_name.is_empty() {
        return;
    }
    let path_dirs_by_test_dir = test_dirs
        .iter()
        .map(|dir| (dir.clone(), prepend_path_dirs(dir, &files)))
        .collect::<BTreeMap<_, _>>();
    for owner in owners.iter_mut() {
        let mut names = BTreeSet::new();
        let modules = owner_module_paths(&owner.file)
            .into_iter()
            .chain(owner.reexport_modules.iter().cloned())
            .chain(
                owner
                    .ambiguous_src_modules
                    .iter()
                    .map(|ambiguous| ambiguous.module.clone()),
            );
        for module in modules {
            if let Some(root) = module.split('.').next().filter(|root| !root.is_empty()) {
                names.insert(root.to_string());
            }
        }
        owner.test_local_shadows = names
            .into_iter()
            .filter_map(|module| {
                let local_dirs = local_dirs_by_name.get(&module)?;
                let affected = path_dirs_by_test_dir
                    .iter()
                    .filter(|(_, path_dirs)| path_dirs.iter().any(|dir| local_dirs.contains(dir)))
                    .map(|(test_dir, _)| test_dir.clone())
                    .collect::<Vec<_>>();
                (!affected.is_empty()).then_some(TestLocalShadow {
                    module,
                    test_dirs: affected,
                })
            })
            .collect();
    }
}

/// Whether a test-local module shadows the top-level name of `module` for
/// `test_file`, so the bare import names the helper and not the owner.
pub(super) fn test_local_module_shadows(
    owner: &PythonOwner,
    module: &str,
    test_file: &Path,
) -> bool {
    let Some(root) = module.split('.').next() else {
        return false;
    };
    let test_dir = test_file.parent().unwrap_or(Path::new(""));
    owner
        .test_local_shadows
        .iter()
        .any(|shadow| shadow.module == root && shadow.test_dirs.iter().any(|dir| dir == test_dir))
}
