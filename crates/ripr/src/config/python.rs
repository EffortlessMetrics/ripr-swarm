//! Python project auto-detection for configuration defaults.

use std::path::Path;

pub(crate) const PYTHON_PROJECT_MARKERS: &[&str] = &[
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "requirements.txt",
    "pytest.ini",
    "tox.ini",
    "noxfile.py",
];
pub(crate) const PYTHON_SOURCE_DIR_MARKERS: &[&str] = &["src", "tests"];
pub(crate) const PYTHON_PROJECT_EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".ripr",
    ".direnv",
    "__pycache__",
    ".venv",
    "venv",
    "env",
    ".tox",
    ".nox",
    "site-packages",
    ".pytest_cache",
    ".mypy_cache",
    "dist",
    "build",
];

pub(crate) fn detect_python_project(root: &Path) -> bool {
    PYTHON_PROJECT_MARKERS
        .iter()
        .any(|marker| root.join(marker).is_file())
        || PYTHON_SOURCE_DIR_MARKERS
            .iter()
            .any(|marker| dir_contains_python_source(&root.join(marker)))
}

/// Canonical root project-marker file name for a candidate name. Marker
/// files are resolved through the filesystem by the detector, so on Windows
/// the comparison is case-insensitive. `None` when the name is not a marker.
pub(crate) fn python_project_marker_name(name: &str) -> Option<&'static str> {
    canonical_marker_name(PYTHON_PROJECT_MARKERS, name)
}

/// Canonical root source-directory marker (`src` or `tests`) for a path
/// component, with the same platform rule as the marker files. The detector
/// resolves these directories through the filesystem, so detection state can
/// change when detectable Python source appears anywhere below them.
pub(crate) fn python_source_dir_marker_name(name: &str) -> Option<&'static str> {
    canonical_marker_name(PYTHON_SOURCE_DIR_MARKERS, name)
}

/// Whether a directory component is excluded from detection traversal,
/// exactly as the recursive detector compares entry names.
pub(crate) fn is_python_project_excluded_dir(name: &str) -> bool {
    PYTHON_PROJECT_EXCLUDED_DIRS.contains(&name)
}

/// Whether a file name is a detectable Python source file name: a `.py`
/// extension that is not a generated-file name, exactly as the recursive
/// detector compares names. Presence classification only — content is never
/// read.
pub(crate) fn is_detectable_python_source_name(name: &str) -> bool {
    is_python_source_name(name) && !is_detectable_generated_python_name(name)
}

/// Whether a root source-directory marker contains detectable Python source,
/// exactly as `detect_python_project` consumes it. Presence-only: the bound
/// value is this boolean, never a per-file enumeration.
pub(crate) fn source_dir_contains_detectable_python(root: &Path, marker: &str) -> bool {
    dir_contains_python_source(&root.join(marker))
}

fn canonical_marker_name<'a>(candidates: &[&'a str], name: &str) -> Option<&'a str> {
    candidates.iter().copied().find(|candidate| {
        if cfg!(windows) {
            candidate.eq_ignore_ascii_case(name)
        } else {
            *candidate == name
        }
    })
}

fn dir_contains_python_source(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if is_python_project_excluded_dir(name) {
                continue;
            }
            if dir_contains_python_source(&path) {
                return true;
            }
        } else if file_type.is_file() && is_detectable_python_source_name(name) {
            return true;
        }
    }
    false
}

fn is_python_source_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "py")
}

fn is_detectable_generated_python_name(name: &str) -> bool {
    name.ends_with("_pb2.py")
        || name.ends_with("_pb2_grpc.py")
        || name.ends_with(".generated.py")
        || name.ends_with("_generated.py")
        || name.starts_with("generated_")
}
