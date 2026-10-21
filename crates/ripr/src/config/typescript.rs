//! TypeScript/JavaScript path authorities for the preview adapter (#3743).
//!
//! One table owns the directories the diff loop must not count and the
//! workspace walk must not index. Python splits those decisions because repo
//! discovery keeps walking `vendor` so those files can be counted as an
//! excluded role. TypeScript preview has no excluded-role ledger and only the
//! diff-mode fact index, so the walk prunes every directory the diff loop
//! refuses, including `vendor`. A vendored, built, or generated tree therefore
//! contributes neither findings nor denominator counts.

use std::path::Path;

/// Directory components excluded from TypeScript/JavaScript diff analysis
/// and from the preview workspace walk (#3743):
///
/// - repository and tooling state: `.git`, `target`, `node_modules`,
///   `.ripr`, `.direnv`;
/// - build and coverage output: `dist`, `build`, `out`, `coverage`;
/// - toolchain caches and generated sites: `.next`, `.cache`;
/// - vendored third-party trees: `vendor`;
/// - generated-output directories: `__generated__`.
///
/// Matching is an exact component comparison, not a substring, so
/// `src/build.ts` and `src/vendor.ts` stay ordinary source.
pub(crate) const TYPESCRIPT_EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".ripr",
    ".direnv",
    "dist",
    "build",
    "out",
    "coverage",
    ".next",
    ".cache",
    "vendor",
    "__generated__",
];

/// Whether a directory component is excluded from every TypeScript preview
/// surface: diff counting, finding generation, and workspace indexing.
pub(crate) fn is_typescript_excluded_dir_everywhere(name: &str) -> bool {
    TYPESCRIPT_EXCLUDED_DIRS.contains(&name)
}

/// Whether a directory component is pruned from the TypeScript preview
/// workspace walk.
///
/// This is the same set as [`is_typescript_excluded_dir_everywhere`], on
/// purpose. Python repo discovery keeps walking `vendor` so the files can be
/// recorded as an excluded role. This adapter has no such ledger, and indexing
/// those trees would only feed untrusted files into owner and test extraction.
pub(crate) fn is_typescript_dir_pruned_from_discovery(name: &str) -> bool {
    is_typescript_excluded_dir_everywhere(name)
}

/// Path-level form of [`is_typescript_excluded_dir_everywhere`].
///
/// A component that cannot be read as UTF-8 is never compared against the
/// UTF-8 table and is therefore not excluded by this predicate.
pub(crate) fn is_detectable_excluded_typescript_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(is_typescript_excluded_dir_everywhere)
    })
}

/// Whether an exact normalized repository-relative declaration admits this
/// path past the built-in excluded-directory conventions (#7386).
/// Declarations are the `[languages.typescript] handwritten_files` entries:
/// exact paths only, no globs, normalized with `/` separators at parse time.
pub(crate) fn is_declared_handwritten_typescript_path(declared: &[String], path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    declared.iter().any(|entry| entry == &normalized)
}

/// Whether any declared hand-written TypeScript/JavaScript path lives under
/// `dir`, which is repository-relative and normalized. The workspace walk
/// consults this before pruning an excluded-name directory: a declared
/// hand-written file keeps its ancestor directories reachable so the file
/// can be read and indexed (#7386).
pub(crate) fn declared_typescript_handwritten_under(declared: &[String], dir: &Path) -> bool {
    let mut prefix = dir.to_string_lossy().replace('\\', "/");
    if !prefix.is_empty() && !prefix.ends_with('/') {
        prefix.push('/');
    }
    declared.iter().any(|entry| entry.starts_with(&prefix))
}

/// Whether a UTF-8 file name is a generated TypeScript/JavaScript artifact.
///
/// The families are `*.generated.*` (`cart.generated.ts`,
/// `types.generated.d.ts`) and minified bundles (`app.min.js`,
/// `vendor.min.mjs`, `lib.min.cjs`), which web apps check in under
/// `public/`, `static/` or `assets/`. `generated.ts`, `regenerated.ts`,
/// `min.js` and `admin.js` are near-misses and stay ordinary source.
pub(crate) fn is_detectable_generated_typescript_name(name: &str) -> bool {
    name.contains(".generated.")
        || [".min.js", ".min.mjs", ".min.cjs"]
            .iter()
            .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix))
}

/// Path form of [`is_detectable_generated_typescript_name`].
///
/// A non-UTF-8 file name can never match the UTF-8 `.generated.` family.
pub(crate) fn is_detectable_generated_typescript_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_detectable_generated_typescript_name)
}

/// Root project-marker files that select the TypeScript preview adapter when
/// no `ripr.toml` exists. `package.json` is deliberately not a marker: Rust
/// and Python repositories routinely carry one for docs or tooling, and it
/// does not say the repository's own tests run under a TS/JS runner.
pub(crate) const TYPESCRIPT_PROJECT_MARKERS: &[&str] = &["tsconfig.json", "jsconfig.json"];
pub(crate) const TYPESCRIPT_SOURCE_DIR_MARKERS: &[&str] = &["src", "tests"];

/// No-config project detection, mirroring `detect_python_project`: a root
/// marker file, or detectable TypeScript source below root `src`/`tests`.
pub(crate) fn detect_typescript_project(root: &Path) -> bool {
    TYPESCRIPT_PROJECT_MARKERS
        .iter()
        .any(|marker| root.join(marker).is_file())
        || TYPESCRIPT_SOURCE_DIR_MARKERS
            .iter()
            .any(|marker| source_dir_contains_detectable_typescript(root, marker))
}

/// Canonical root marker file name for a candidate name. Markers resolve
/// through the filesystem, so the comparison follows the host path rule.
pub(crate) fn typescript_project_marker_name(name: &str) -> Option<&'static str> {
    canonical_marker_name(TYPESCRIPT_PROJECT_MARKERS, name)
}

/// Canonical root source-directory marker for a path component.
pub(crate) fn typescript_source_dir_marker_name(name: &str) -> Option<&'static str> {
    canonical_marker_name(TYPESCRIPT_SOURCE_DIR_MARKERS, name)
}

fn canonical_marker_name(candidates: &[&'static str], name: &str) -> Option<&'static str> {
    let case_insensitive = crate::output::path::host_paths_case_insensitive();
    candidates.iter().copied().find(|candidate| {
        if case_insensitive {
            candidate.eq_ignore_ascii_case(name)
        } else {
            *candidate == name
        }
    })
}

/// Whether a file name is detectable TypeScript source: a `.ts`/`.tsx`/
/// `.mts`/`.cts` extension that is not a generated-file name. Presence
/// classification only; content is never read. JavaScript alone does not
/// select the adapter.
pub(crate) fn is_detectable_typescript_source_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            crate::analysis::ts_js_source_kind(extension)
                == Some(crate::analysis::TsJsSourceKind::TypeScript)
        })
        && !is_detectable_generated_typescript_name(name)
}

/// Whether a root source-directory marker contains detectable TypeScript
/// source. Presence-only; the bound value is this boolean.
pub(crate) fn source_dir_contains_detectable_typescript(root: &Path, marker: &str) -> bool {
    dir_contains_typescript_source(&root.join(marker))
}

fn dir_contains_typescript_source(dir: &Path) -> bool {
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
            if !is_typescript_excluded_dir_everywhere(name) && dir_contains_typescript_source(&path)
            {
                return true;
            }
        } else if file_type.is_file() && is_detectable_typescript_source_name(name) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_dirs_match_components_not_substrings() {
        for path in [
            "node_modules/left-pad/index.js",
            "dist/bundle.js",
            "build/out.ts",
            "coverage/lcov-report/index.js",
            "vendor/lib.ts",
            "src/__generated__/types.ts",
            "packages/app/dist/index.js",
        ] {
            assert!(
                is_detectable_excluded_typescript_path(Path::new(path)),
                "{path} is an excluded subtree"
            );
        }
        for path in [
            "src/ok.ts",
            "src/build.ts",
            "src/vendor.ts",
            "src/coverage.ts",
            "packages/app/src/index.tsx",
        ] {
            assert!(
                !is_detectable_excluded_typescript_path(Path::new(path)),
                "{path} is ordinary source"
            );
        }
    }

    #[test]
    fn generated_names_match_the_dotted_family_only() {
        for name in [
            "cart.generated.ts",
            "Button.generated.tsx",
            "client.generated.js",
            "view.generated.jsx",
            "types.generated.d.ts",
            "app.min.js",
            "vendor.min.mjs",
            "lib.min.cjs",
        ] {
            assert!(
                is_detectable_generated_typescript_name(name),
                "{name} is generated"
            );
        }
        for name in [
            "generated.ts",
            "regenerated.ts",
            "notgenerated.js",
            "cart.ts",
            "min.js",
            ".min.js",
            "admin.js",
            "app.min.ts",
            "app.minified.js",
        ] {
            assert!(
                !is_detectable_generated_typescript_name(name),
                "{name} is a near-miss"
            );
        }
    }

    #[test]
    fn discovery_prunes_the_same_directories_the_diff_refuses() {
        for name in TYPESCRIPT_EXCLUDED_DIRS {
            assert!(is_typescript_dir_pruned_from_discovery(name));
            assert!(is_typescript_excluded_dir_everywhere(name));
        }
        assert!(!is_typescript_dir_pruned_from_discovery("src"));
    }

    fn detection_root(label: &str) -> Result<std::path::PathBuf, String> {
        let root =
            std::env::temp_dir().join(format!("ripr-ts-detect-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        Ok(root)
    }

    fn write(root: &Path, relative: &str) -> Result<(), String> {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("mkdir: {err}"))?;
        }
        std::fs::write(&path, "export {};\n").map_err(|err| format!("write: {err}"))
    }

    #[test]
    fn detection_requires_a_marker_or_typescript_source() -> Result<(), String> {
        let root = detection_root("shape")?;
        assert!(!detect_typescript_project(&root), "empty root");
        write(&root, "package.json")?;
        write(&root, "src/app.js")?;
        assert!(
            !detect_typescript_project(&root),
            "package.json and plain JavaScript do not select the adapter"
        );
        write(&root, "tsconfig.json")?;
        assert!(detect_typescript_project(&root), "tsconfig.json marker");
        std::fs::remove_file(root.join("tsconfig.json")).map_err(|err| err.to_string())?;
        for extension in ["tsx", "mts", "cts"] {
            let file = format!("src/deep/app.{extension}");
            write(&root, &file)?;
            assert!(
                detect_typescript_project(&root),
                "nested {extension} source"
            );
            std::fs::remove_file(root.join(&file)).map_err(|err| err.to_string())?;
            assert!(!detect_typescript_project(&root), "removed {extension}");
        }
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    #[test]
    fn excluded_and_generated_source_does_not_enable_detection() -> Result<(), String> {
        let root = detection_root("excluded")?;
        write(&root, "src/vendor/lib.ts")?;
        write(&root, "src/node_modules/pkg/index.ts")?;
        write(&root, "src/api.generated.ts")?;
        assert!(!detect_typescript_project(&root));
        write(&root, "tests/cart.test.ts")?;
        assert!(detect_typescript_project(&root), "harness sanity");
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }
}
