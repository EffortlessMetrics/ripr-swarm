//! Shallow language-marker detection shared by `doctor`, `first-pr` and the
//! Perl preview. Owned by `config` (typed configuration and language
//! detection) so `output` and `app` need not import a CLI adapter (#5193).

use std::path::Path;

use crate::analysis;
use crate::domain::LanguageId;

/// Shallow marker scan: look for files/dirs that indicate a language is
/// present. Only inspects `root`, `root/src/`, and immediate child dirs of
/// `root` — no recursion, no AST parsing, no workspace pipeline.
///
/// Returns `false` (no marker found) when any `read_dir` call fails; doctor
/// must never panic or OOM on a scan error.
fn shallow_has_extension(root: &Path, extension: &str) -> bool {
    let dirs_to_scan: [&Path; 2] = [root, &root.join("src")];
    for dir in dirs_to_scan {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                    return true;
                }
            }
        }
    }
    // Also scan one level of child dirs of root.
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                let sub_entries = std::fs::read_dir(&child).into_iter().flatten().flatten();
                for sub in sub_entries {
                    let path = sub.path();
                    if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn shallow_has_file(root: &Path, name: &str) -> bool {
    root.join(name).exists() || root.join("src").join(name).exists()
}

/// Nested Python package roots for a root that no other language claims
/// (#13618). A root with Rust or TypeScript/JavaScript markers is that
/// language's project: a PyO3 `bindings/python/pyproject.toml` or tooling
/// package must not make it a Python monorepo.
pub(crate) fn nested_python_packages_unclaimed(root: &Path) -> Vec<std::path::PathBuf> {
    if crate::domain::rust_markers_at(root) || has_typescript_or_javascript_markers(root) {
        return Vec::new();
    }
    super::python::nested_python_package_roots(root)
}

fn has_typescript_or_javascript_markers(root: &Path) -> bool {
    shallow_has_file(root, "package.json")
        || shallow_has_file(root, "tsconfig.json")
        || shallow_has_file(root, "jsconfig.json")
        || analysis::TYPESCRIPT_SOURCE_EXTENSIONS
            .iter()
            .chain(analysis::JAVASCRIPT_SOURCE_EXTENSIONS.iter())
            .any(|extension| shallow_has_extension(root, extension))
}

/// Detect which languages have concrete file markers in this workspace.
/// Returns detected `LanguageId`s in a stable order (Rust first, then
/// TypeScript, JavaScript, Python, Perl).
pub(crate) fn detect_languages(root: &Path) -> Vec<LanguageId> {
    let mut found = Vec::new();

    // Rust: Cargo.toml at root OR .rs files in root/src (shared with the
    // enablement tip so a TypeScript-only root is not told to enable Rust).
    if crate::domain::rust_markers_at(root) {
        found.push(LanguageId::Rust);
    }

    // TypeScript: package.json, tsconfig.json, or a TypeScript-source
    // extension (.ts/.tsx/.mts/.cts) at a shallow depth. #4116: both
    // extension families consume the shared TS/JS extension authority.
    if shallow_has_file(root, "package.json")
        || shallow_has_file(root, "tsconfig.json")
        || analysis::TYPESCRIPT_SOURCE_EXTENSIONS
            .iter()
            .any(|extension| shallow_has_extension(root, extension))
    {
        found.push(LanguageId::TypeScript);
    }

    // JavaScript: JavaScript-source extensions (.js/.jsx/.mjs/.cjs), only
    // when no TypeScript markers were found.
    if !found.contains(&LanguageId::TypeScript)
        && analysis::JAVASCRIPT_SOURCE_EXTENSIONS
            .iter()
            .any(|extension| shallow_has_extension(root, extension))
    {
        found.push(LanguageId::JavaScript);
    }

    // Python: pyproject.toml, setup.py, setup.cfg, pytest.ini, or .py files
    if shallow_has_file(root, "pyproject.toml")
        || shallow_has_file(root, "setup.py")
        || shallow_has_file(root, "setup.cfg")
        || shallow_has_file(root, "pytest.ini")
        || shallow_has_extension(root, "py")
        || !nested_python_packages_unclaimed(root).is_empty()
    {
        found.push(LanguageId::Python);
    }

    // Perl: the same predicate that gates the Perl preview section, so a
    // CPAN layout with modules below `lib/Name/` is detected and a `.t` under
    // `target/` is not.
    if perl_project_detected(root) {
        found.push(LanguageId::Perl);
    }

    found
}

/// Count files with a given extension under the root (recursive). Used by the
/// Perl preview to report real .pm/.pl/.t counts. Campaign 31 item 5: the
/// prior `shallow_has_extension as usize` returned only 0/1, not a real count.
/// Hidden and build/dependency directories that the Perl file walks skip,
/// so vendored or generated files neither inflate counts nor detect Perl.
pub(crate) fn is_skipped_walk_dir(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.starts_with('.') || matches!(name, "target" | "node_modules" | "blib")
}

/// True for a real directory entry. `DirEntry::file_type` does not follow
/// symlinks, so a `src/loop -> .` link cannot send doctor's walks into an
/// unbounded descent (the other workspace walkers already skip links). An
/// entry whose type cannot be read is not descended. Non-directory entries
/// are counted only when `Path::is_file` holds, so a directory link named
/// `x.pm` is neither descended nor counted as a Perl file.
pub(crate) fn is_walkable_dir(entry: &std::fs::DirEntry) -> bool {
    entry.file_type().is_ok_and(|kind| kind.is_dir())
}

/// True when the workspace has a CPAN build marker or any `.pm`, `.pl` or
/// `.t` file outside skipped directories. One walk that stops at the first
/// hit; `detect_languages` and the Perl preview share this definition.
pub(crate) fn perl_project_detected(root: &Path) -> bool {
    fn any_perl_file(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            if is_walkable_dir(&entry) {
                !is_skipped_walk_dir(&path) && any_perl_file(&path)
            } else {
                path.is_file()
                    && matches!(
                        path.extension().and_then(|e| e.to_str()),
                        Some("pm" | "pl" | "t")
                    )
            }
        })
    }
    has_perl_project_markers(root) || any_perl_file(root)
}

/// Detect CPAN-style project markers beyond .pm/.pl/.t files: Makefile.PL,
/// Build.PL, cpanfile. These confirm a real CPAN-style project a producer can
/// index.
fn has_perl_project_markers(root: &Path) -> bool {
    ["Makefile.PL", "Build.PL", "cpanfile"]
        .iter()
        .any(|marker| root.join(marker).is_file())
}

#[cfg(test)]
mod nested_python_tests {
    use super::*;

    fn scratch(name: &str, files: &[&str]) -> Result<std::path::PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-13618-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().ok_or("no parent")?)
                .map_err(|err| format!("create dir: {err}"))?;
            std::fs::write(&path, "x = 1\n").map_err(|err| format!("write: {err}"))?;
        }
        Ok(root)
    }

    #[test]
    fn nested_package_detects_python_only_when_no_other_language_claims_the_root()
    -> Result<(), String> {
        let mono = scratch("mono", &["services/a/pyproject.toml"])?;
        assert_eq!(detect_languages(&mono), vec![LanguageId::Python]);
        // Negative controls: a PyO3-style Rust root and a TypeScript root with a
        // nested Python package are not Python monorepos.
        let rust = scratch("rust", &["Cargo.toml", "bindings/python/pyproject.toml"])?;
        assert!(!detect_languages(&rust).contains(&LanguageId::Python));
        assert!(nested_python_packages_unclaimed(&rust).is_empty());
        let ts = scratch("ts", &["tsconfig.json", "tools/py/pyproject.toml"])?;
        assert!(!detect_languages(&ts).contains(&LanguageId::Python));
        let js = scratch("js", &["jsconfig.json", "tools/py/pyproject.toml"])?;
        assert!(!detect_languages(&js).contains(&LanguageId::Python));
        for root in [mono, rust, ts, js] {
            std::fs::remove_dir_all(&root).map_err(|err| format!("remove: {err}"))?;
        }
        Ok(())
    }
}
