//! Positive import custody for the bounded Python readings (#12952).
//!
//! Legacy module-value passes keep their stricter configured-path refusal.
//! This owner admits one additional root only when its source and every
//! competing spelling of the changed module can be checked physically.

use super::module_value::{self, ForeignWrites};
use super::related_tests::owner_module_paths;
use super::source_facts::parse_module_result;
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const CONFIG_NAMES: [&str; 7] = [
    "pytest.ini",
    ".pytest.ini",
    "pytest.toml",
    ".pytest.toml",
    "pyproject.toml",
    "setup.cfg",
    "tox.ini",
];
const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImportPathAdmission {
    Existing,
    CanonicalSrc,
    Refused,
}

impl ImportPathAdmission {
    pub(super) fn admitted(self) -> bool {
        self != Self::Refused
    }
}

/// Keep the established configuration parser as the default. Its sole
/// exception is the root pyproject's literal one-entry `src` setting, with
/// no other configuration file or unresolved import-path spelling.
pub(super) fn import_path_admission(
    root: &Path,
    roots: &BTreeSet<&Path>,
    strict_configured: bool,
) -> ImportPathAdmission {
    if !roots.contains(Path::new("")) || !regular_path(root, Path::new(""), false) {
        return ImportPathAdmission::Refused;
    }
    let mut configs = Vec::new();
    for directory in roots {
        if !regular_path(root, directory, false) {
            return ImportPathAdmission::Refused;
        }
        for name in CONFIG_NAMES {
            let path = directory.join(name);
            match std::fs::symlink_metadata(root.join(&path)) {
                Ok(metadata) if metadata.file_type().is_file() => configs.push(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return ImportPathAdmission::Refused,
            }
        }
    }
    if !strict_configured {
        return ImportPathAdmission::Existing;
    }
    if configs.as_slice() != [PathBuf::from("pyproject.toml")]
        || !roots.contains(Path::new("src"))
        || !regular_path(root, Path::new("src"), false)
    {
        return ImportPathAdmission::Refused;
    }
    let path = root.join("pyproject.toml");
    let Ok(metadata) = std::fs::metadata(&path) else {
        return ImportPathAdmission::Refused;
    };
    if metadata.len() > MAX_CONFIG_BYTES {
        return ImportPathAdmission::Refused;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return ImportPathAdmission::Refused;
    };
    let mut text = String::new();
    if file
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut text)
        .is_err()
        || text.len() as u64 > MAX_CONFIG_BYTES
    {
        return ImportPathAdmission::Refused;
    }
    if canonical_src_setting(&text) {
        ImportPathAdmission::CanonicalSrc
    } else {
        ImportPathAdmission::Refused
    }
}

fn canonical_src_setting(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("addopts")
        || lowered.contains("python_paths")
        || lowered.matches("pythonpath").count() != 1
    {
        return false;
    }
    let Ok(mut table) = text.parse::<toml::Table>() else {
        return false;
    };
    let Some(setting) = table
        .get_mut("tool")
        .and_then(|value| value.get_mut("pytest"))
        .and_then(|value| value.get_mut("ini_options"))
        .and_then(|value| value.get_mut("pythonpath"))
    else {
        return false;
    };
    if !matches!(setting, toml::Value::Array(entries)
        if matches!(entries.as_slice(), [toml::Value::String(entry)] if entry == "src"))
    {
        return false;
    }
    *setting = toml::Value::Array(vec![toml::Value::String(".".to_string())]);
    let Ok(remaining) = toml::to_string(&table) else {
        return false;
    };
    // Parsing exposes escaped keys and values before the same strict reader
    // checks the rest. A second path setting, including an own-directory
    // alias, is outside this one-root admission.
    let remaining_lowered = remaining.to_ascii_lowercase();
    remaining_lowered.matches("pythonpath").count() == 1
        && !remaining_lowered.contains("python_paths")
        && !module_value::sets_unread_import_path(true, &remaining)
}

/// `foreign` is the raw per-file view, before the legacy configured-path
/// refusal marks its overloaded module-name bit. Never clear that bit on
/// the shared owner; only callers holding this positive fact may use it.
pub(super) fn module_import_unshadowed(
    root: &Path,
    file: &Path,
    sources: &HashMap<PathBuf, String>,
    roots: &BTreeSet<&Path>,
    extensions: Option<&BTreeSet<String>>,
    admission: ImportPathAdmission,
    foreign: &ForeignWrites,
) -> bool {
    if !admission.admitted()
        || !sources.contains_key(file)
        || !module_file_components(file)
        || !regular_path(root, file, true)
        || foreign.shares_module_name()
        || module_value::extension_beside(root, file)
        || module_value::compiled_package_shadow(root, roots, file)
        || extensions.is_none_or(|names| module_value::shadowed_by_extension(names, file))
        || (admission == ImportPathAdmission::CanonicalSrc
            && !file.starts_with(Path::new("src")))
        || !own_package_chain(root, file, sources)
    {
        return false;
    }
    // Every spelling accepted by the consumers must refer to this file.
    // This includes `src.fx.core` as well as the src-layout `fx.core`.
    owner_module_paths(file).iter().all(|module| {
        !module.is_empty()
            && !module_value::names_stdlib_module(module)
            && roots.iter().all(|base| {
                regular_path(root, base, false)
                    && module_route_unshadowed(root, base, module, file, sources)
            })
    })
}

fn module_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && characters.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn module_file_components(file: &Path) -> bool {
    file.extension().is_some_and(|extension| extension == "py")
        && file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(module_identifier)
        && file.parent().is_some_and(|parent| {
            parent.components().all(|component| match component {
                Component::Normal(name) => name.to_str().is_some_and(module_identifier),
                _ => false,
            })
        })
}

fn own_package_chain(root: &Path, file: &Path, sources: &HashMap<PathBuf, String>) -> bool {
    let Some(parent) = file.parent() else {
        return false;
    };
    for directory in parent
        .ancestors()
        .take_while(|path| !path.as_os_str().is_empty())
    {
        let Some(initializer) = source_initializer(root, directory) else {
            return false;
        };
        if !initializer {
            // A top-level src layout directory is an import root, not a
            // required package. All packages below it must be read and inert.
            if directory != Path::new("src") {
                return false;
            }
        } else {
            let init = directory.join("__init__.py");
            if init != file && !inert_initializer(root, &init, sources) {
                return false;
            }
        }
    }
    true
}

fn module_route_unshadowed(
    root: &Path,
    base: &Path,
    module: &str,
    own: &Path,
    sources: &HashMap<PathBuf, String>,
) -> bool {
    let parts: Vec<&str> = module.split('.').collect();
    let mut directory = base.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        if !module_identifier(part) {
            return false;
        }
        directory.push(part);
        let source = directory.with_extension("py");
        match std::fs::symlink_metadata(root.join(&source)) {
            Ok(metadata)
                if source == own
                    && index + 1 == parts.len()
                    && metadata.file_type().is_file() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return false,
        }
        match std::fs::symlink_metadata(root.join(&directory)) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
            _ => return false,
        }
        let Some(initializer) = source_initializer(root, &directory) else {
            return false;
        };
        if initializer {
            let init = directory.join("__init__.py");
            if !own.starts_with(&directory)
                || (init != own && !inert_initializer(root, &init, sources))
            {
                return false;
            }
        }
    }
    true
}

/// Absence is known only after enumerating the bounded physical directory.
/// Compiled, alternate-name, unreadable and ambiguous initializers refuse.
fn source_initializer(root: &Path, directory: &Path) -> Option<bool> {
    let mut source = false;
    for (count, entry) in std::fs::read_dir(root.join(directory)).ok()?.enumerate() {
        if count >= MAX_DIRECTORY_ENTRIES {
            return None;
        }
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_str()?;
        if name.split('.').next() == Some("__init__")
            && [".py", ".pyc", ".so", ".pyd"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
        {
            if name != "__init__.py" || !entry.file_type().ok()?.is_file() || source {
                return None;
            }
            source = true;
        }
    }
    Some(source)
}

fn inert_initializer(root: &Path, file: &Path, sources: &HashMap<PathBuf, String>) -> bool {
    let Some(source) = sources.get(file) else {
        return false;
    };
    if !regular_path(root, file, true) {
        return false;
    }
    let Ok(Mod::Module(module)) = parse_module_result(file, source) else {
        return false;
    };
    module.body.iter().all(|statement| match statement {
        Stmt::Pass(_) => true,
        Stmt::Expr(statement) => matches!(statement.value.as_ref(),
            Expr::Constant(value) if matches!(value.value, ast::Constant::Str(_))),
        _ => false,
    })
}

/// Check every component, because leaf metadata alone follows parent links.
fn regular_path(root: &Path, relative: &Path, file: bool) -> bool {
    if !std::fs::symlink_metadata(root).is_ok_and(|meta| meta.file_type().is_dir()) {
        return false;
    }
    let parts: Vec<_> = relative.components().collect();
    let mut path = root.to_path_buf();
    for (index, component) in parts.iter().enumerate() {
        let Component::Normal(part) = component else {
            return false;
        };
        path.push(part);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            return false;
        };
        if file && index + 1 == parts.len() {
            if !metadata.file_type().is_file() {
                return false;
            }
        } else if !metadata.file_type().is_dir() {
            return false;
        }
    }
    !file || !parts.is_empty()
}
