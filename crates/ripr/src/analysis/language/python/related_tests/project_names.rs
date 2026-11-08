//! Project-relative module names of owners inside nested Python projects
//! (#13675).
//!
//! In a monorepo where each package is its own project (`svc/b/pyproject.toml`)
//! a test imports `svc/b/core.py` as `core`. These names go through the #4566
//! ambiguity record, so a name identifies the owner only for a test under the
//! owner's project root.

use super::super::source_utils::normalized_path;
use super::{
    AmbiguousSrcModule, PythonOwner, module_name_identifies_owner_for, owner_module_paths,
    rival_roots_for,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Files whose directory marks a Python project: pytest puts that directory
/// (or a `pythonpath` entry under it) on `sys.path`, so a module below it is
/// importable by its project-relative name.
const PYTHON_PROJECT_MARKERS: [&str; 5] = [
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "pytest.ini",
    "tox.ini",
];

/// The deepest non-root ancestor directory of `file` that holds a project
/// marker. The repository root is excluded: its project-relative names are the
/// full dotted paths [`owner_module_paths`] already yields.
fn nearest_project_root(
    file: &Path,
    has_marker: &mut impl FnMut(&Path) -> bool,
) -> Option<PathBuf> {
    let mut dir = file.parent();
    while let Some(current) = dir {
        if current.as_os_str().is_empty() {
            return None;
        }
        if has_marker(current) {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
}

/// The dotted names `file` has relative to its project `root`.
///
/// The repository-root `lib/` import root of [`owner_module_paths`] is not
/// applied: a project's `lib/core.py` is `lib.core`, never `core` (#13698).
/// The `src/` layout root applies at any depth, as it does there.
fn project_relative_module_names(file: &Path, root: &Path) -> Vec<String> {
    let Ok(relative) = file.strip_prefix(root) else {
        return Vec::new();
    };
    let Some(full) = owner_module_paths(relative).into_iter().next() else {
        return Vec::new();
    };
    let parts = full.split('.').collect::<Vec<_>>();
    let mut names = vec![full.clone()];
    // Only directory segments are import roots, as in `owner_module_paths`.
    let directory_count = relative.components().count().saturating_sub(1);
    for (idx, part) in parts.iter().enumerate().take(directory_count) {
        if *part == "src" {
            let below = parts.get(idx + 1..).unwrap_or_default().join(".");
            if !below.is_empty() && !names.contains(&below) {
                names.push(below);
            }
        }
    }
    names
}

/// Records each owner's project-relative module names (`svc/b/core.py` in the
/// project `svc/b` imports as `core`) so a test inside that project resolves
/// `from core import limit`. A project-relative name identifies the owner only
/// for a test under the owner's project root, which also keeps a same-named
/// module of a sibling project from crediting this one. Runs after
/// [`apply_src_module_ambiguity`]; names already recorded there keep their
/// meaning.
pub(in crate::analysis::language::python) fn apply_project_module_names<'a>(
    owners: &mut [PythonOwner],
    source_files: impl Iterator<Item = &'a PathBuf>,
    mut is_present: impl FnMut(&Path) -> bool,
) {
    let mut marker_cache: BTreeMap<PathBuf, bool> = BTreeMap::new();
    let mut has_marker = |dir: &Path| {
        *marker_cache.entry(dir.to_path_buf()).or_insert_with(|| {
            PYTHON_PROJECT_MARKERS
                .iter()
                .any(|marker| is_present(&dir.join(marker)))
        })
    };
    let mut files_by_module: BTreeMap<String, Vec<(PathBuf, PathBuf)>> = BTreeMap::new();
    for file in source_files {
        let Some(root) = nearest_project_root(file, &mut has_marker) else {
            continue;
        };
        for module in project_relative_module_names(file, &root) {
            files_by_module
                .entry(module)
                .or_default()
                .push((file.clone(), root.clone()));
        }
    }
    for owner in owners.iter_mut() {
        let Some(owner_root) = nearest_project_root(&owner.file, &mut has_marker) else {
            continue;
        };
        let owner_file = normalized_path(&owner.file);
        let known = owner_module_paths(&owner.file);
        for module in project_relative_module_names(&owner.file, &owner_root) {
            if known.contains(&module)
                || owner
                    .ambiguous_src_modules
                    .iter()
                    .any(|ambiguous| ambiguous.module == module)
            {
                continue;
            }
            let rival_roots = rival_roots_for(&files_by_module, &module, |file, _| {
                normalized_path(file) == owner_file
            });
            // Another file of the same project yields the same name
            // (`pkg.py` beside `pkg/__init__.py`): the name alone does not say
            // which one an import loads, so it identifies neither (#13698).
            if rival_roots.contains(&owner_root) {
                continue;
            }
            owner.ambiguous_src_modules.push(AmbiguousSrcModule {
                module,
                owner_root: owner_root.clone(),
                rival_roots,
                shadowed: false,
            });
        }
    }
}

/// Every dotted module path under which `test_file` may import the owner: its
/// repository-relative paths plus the project-relative names recorded by
/// [`apply_project_module_names`] whose project root holds `test_file`.
/// Project-relative names are short (`core`, `pkg.main`), so they are only
/// ever offered with that root check and are never used for suffix or package
/// prefix matching elsewhere.
pub(in crate::analysis::language::python) fn owner_import_modules_for(
    owner: &PythonOwner,
    test_file: &Path,
) -> Vec<String> {
    let mut paths = owner_module_paths(&owner.file);
    for ambiguous in &owner.ambiguous_src_modules {
        if !paths.contains(&ambiguous.module)
            && !owner.reexport_modules.contains(&ambiguous.module)
            && module_name_identifies_owner_for(owner, &ambiguous.module, test_file)
        {
            paths.push(ambiguous.module.clone());
        }
    }
    paths
}
