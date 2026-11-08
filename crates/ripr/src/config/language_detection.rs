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
