//! Package re-export identity for Python owners.
//!
//! Real Python test suites rarely import a function from the file that defines
//! it. `humanize` tests write `import humanize` and call
//! `humanize.naturaldelta(...)`; `more-itertools` tests write
//! `import more_itertools as mi` and call `mi.one(...)`. Both names reach the
//! owner only because the package `__init__.py` re-exports it
//! (`from .time import naturaldelta`, `from .more import *`). Without that
//! link every changed line in such a package reads `no_static_path` although
//! the suite reaches and kills the mutant.
//!
//! This module computes the dotted package paths under which an owner is
//! re-exported, from the `__init__.py` module imports already parsed into the
//! workspace facts. It is bounded and fails closed:
//!
//! - only `__init__.py` files re-export, and only through `from M import ...`
//!   where `M` is the owner's module (or an earlier re-exporting package);
//! - an explicit re-export must keep the owner's name (`import x as y`
//!   renames are not followed);
//! - a star re-export never carries a `_private` name, and when the source
//!   module declares `__all__`, the name must appear quoted in that file;
//! - methods and module owners are never re-exported;
//! - the chain stops after [`MAX_REEXPORT_HOPS`] packages.
//!
//! The result only widens *which module path* identifies the owner. The
//! relation still requires the test to call the owner's name through that
//! path, so an unrelated name imported from the same package stays unrelated.

use super::related_tests::owner_module_paths;
use super::{PythonImport, PythonOwner};
use crate::domain::OwnerKind;
use std::path::{Path, PathBuf};

/// Maximum number of package `__init__.py` hops followed from the owner module.
const MAX_REEXPORT_HOPS: usize = 3;

/// One package `__init__.py` and the module imports it declares.
struct PackageExporter<'a> {
    file: &'a Path,
    module_paths: Vec<String>,
    imports: &'a [PythonImport],
}

/// Collects the package exporters (`__init__.py` module owners) from the
/// workspace owners.
fn package_exporters(owners: &[PythonOwner]) -> Vec<PackageExporter<'_>> {
    owners
        .iter()
        .filter(|owner| owner.is_module_owner() && is_package_init(&owner.file))
        .map(|owner| PackageExporter {
            file: &owner.file,
            module_paths: owner_module_paths(&owner.file),
            imports: &owner.imports,
        })
        .collect()
}

/// Sets `reexport_modules` on every owner, taking the package exporters from
/// the same owner set (diff mode reads the whole workspace up front).
///
/// `source_of` returns the source text of a workspace file; it is used only to
/// honor a declared `__all__` on a star re-export.
pub(super) fn apply_package_reexports<'s>(
    owners: &mut [PythonOwner],
    source_of: impl Fn(&Path) -> Option<&'s str>,
) {
    let init_owners: Vec<PythonOwner> = owners
        .iter()
        .filter(|owner| owner.is_module_owner() && is_package_init(&owner.file))
        .cloned()
        .collect();
    apply_package_reexports_from(owners, &init_owners, source_of);
}

/// Sets `reexport_modules` on `owners` from separately loaded package
/// `__init__.py` module owners (repo mode loads one production file at a time).
pub(super) fn apply_package_reexports_from<'s>(
    owners: &mut [PythonOwner],
    init_owners: &[PythonOwner],
    source_of: impl Fn(&Path) -> Option<&'s str>,
) {
    let exporters = package_exporters(init_owners);
    for owner in owners.iter_mut() {
        owner.reexport_modules = package_reexport_modules(owner, &exporters, &source_of);
    }
}

/// The dotted package paths that re-export `owner` under its own name.
fn package_reexport_modules<'s>(
    owner: &PythonOwner,
    exporters: &[PackageExporter<'_>],
    source_of: &impl Fn(&Path) -> Option<&'s str>,
) -> Vec<String> {
    if owner.is_module_owner()
        || matches!(
            owner.owner_kind,
            Some(OwnerKind::Method | OwnerKind::ClassMethod)
        )
        || owner.qualified_name != owner.name
        || owner.name.is_empty()
    {
        return Vec::new();
    }
    let name = owner.name.as_str();
    let mut frontier: Vec<(Vec<String>, PathBuf)> =
        vec![(owner_module_paths(&owner.file), owner.file.clone())];
    let mut visited: Vec<&Path> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for _ in 0..MAX_REEXPORT_HOPS {
        let mut next: Vec<(Vec<String>, PathBuf)> = Vec::new();
        for exporter in exporters {
            if visited.contains(&exporter.file) || exporter.file == owner.file.as_path() {
                continue;
            }
            let reexports = frontier.iter().any(|(paths, source_file)| {
                exporter.imports.iter().any(|import| {
                    paths.contains(&import.source_module)
                        && imports_name(import, name, source_file, source_of)
                })
            });
            if reexports {
                visited.push(exporter.file);
                for path in &exporter.module_paths {
                    if !out.contains(path) {
                        out.push(path.clone());
                    }
                }
                next.push((exporter.module_paths.clone(), exporter.file.to_path_buf()));
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    out
}

fn imports_name<'s>(
    import: &PythonImport,
    name: &str,
    source_file: &Path,
    source_of: &impl Fn(&Path) -> Option<&'s str>,
) -> bool {
    if import.imported == name {
        return import.alias == name;
    }
    import.imported == "*"
        && !name.starts_with('_')
        && star_exports_name(source_of(source_file), name)
}

/// A star import exports every public name unless the source module declares
/// `__all__`; then only names listed there. The `__all__` check is textual and
/// conservative: the name must appear as a quoted string somewhere in the
/// file. An unreadable source fails closed.
fn star_exports_name(source: Option<&str>, name: &str) -> bool {
    let Some(source) = source else {
        return false;
    };
    if !source.contains("__all__") {
        return true;
    }
    source.contains(&format!("\"{name}\"")) || source.contains(&format!("'{name}'"))
}

fn is_package_init(file: &Path) -> bool {
    file.file_name().and_then(|name| name.to_str()) == Some("__init__.py")
}
