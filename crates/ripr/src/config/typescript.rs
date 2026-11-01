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
/// - generated-output directories: `__generated__`;
/// - Jest snapshot output: `__snapshots__` (never TypeScript/JavaScript
///   sources; #11984 item 2).
///
/// Matching is an exact component comparison, not a substring, so
/// `src/build.ts` and `src/vendor.ts` stay ordinary source. Compiler
/// reference-output trees (`baselines/reference`, `baselines/local`) are
/// path-shaped and live beside this table: a bare `baselines` name is too
/// broad to add here.
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
    "__snapshots__",
];

/// Consecutive directory components that name TypeScript compiler
/// reference-output trees (#11984 item 2). A bare `baselines` name is too
/// broad (a real source directory can use that name); only these children
/// are pruned. `__snapshots__` is a name-table entry instead: Jest snapshot
/// directories never hold TS/JS sources.
const TYPESCRIPT_REFERENCE_OUTPUT_DIR_PAIRS: &[(&str, &str)] =
    &[("baselines", "reference"), ("baselines", "local")];

/// Whether `path` contains a consecutive `parent/child` directory pair.
///
/// A component that cannot be read as UTF-8 is skipped (fail-open), matching
/// [`is_detectable_excluded_typescript_path`]. Non-normal components reset
/// the pair so `baselines/./reference` is not treated as the compiler tree.
fn path_has_consecutive_dir_pair(path: &Path, parent: &str, child: &str) -> bool {
    let mut previous: Option<&str> = None;
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            previous = None;
            continue;
        };
        let Some(name) = part.to_str() else {
            previous = None;
            continue;
        };
        if previous == Some(parent) && name == child {
            return true;
        }
        previous = Some(name);
    }
    false
}

/// Path-shaped TypeScript compiler reference-output trees.
fn is_typescript_reference_output_path(path: &Path) -> bool {
    TYPESCRIPT_REFERENCE_OUTPUT_DIR_PAIRS
        .iter()
        .any(|(parent, child)| path_has_consecutive_dir_pair(path, parent, child))
}

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
/// Compiled only with the adapter or for tests: nothing outside the
/// feature-gated discovery walk and this module's tests calls it.
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn is_typescript_dir_pruned_from_discovery(name: &str) -> bool {
    is_typescript_excluded_dir_everywhere(name)
}

/// Path-level form of [`is_typescript_excluded_dir_everywhere`], plus the
/// path-shaped reference-output trees (`baselines/reference`,
/// `baselines/local`).
///
/// A component that cannot be read as UTF-8 is never compared against the
/// UTF-8 table and is therefore not excluded by this predicate. The same
/// helper feeds the workspace walk prune and the diff loop's pre-count
/// refusal, so a pruned tree is also named by the existing
/// generated/excluded-path limitation when a changed file sits under it.
pub(crate) fn is_detectable_excluded_typescript_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(is_typescript_excluded_dir_everywhere)
    }) || is_typescript_reference_output_path(path)
}

/// Whether an exact normalized repository-relative declaration admits this
/// path past the built-in excluded-directory conventions (#7386).
///
/// Declarations are stored strings: `[languages.typescript]
/// handwritten_files` entries, exact paths only, no globs, parse-time
/// `/`-normalized. The candidate side uses the shared separator rule
/// (`path_identity_key_on(path, cfg!(windows), false)`): `\` is a separator
/// only on Windows, and matching does not case-fold, so exact-entry
/// semantics stay. A non-UTF-8 candidate is never admitted.
pub(crate) fn is_declared_handwritten_typescript_path(declared: &[String], path: &Path) -> bool {
    let normalized = crate::output::path::path_identity_key_on(path, cfg!(windows), false);
    declared.iter().any(|entry| entry == &normalized)
}

/// Whether any declared hand-written TypeScript/JavaScript path lives under
/// `dir`.
///
/// `dir` is a repository-relative candidate path. Declarations are stored
/// strings (parse-time `/`-normalized); the candidate side uses the shared
/// separator rule (`path_identity_key_on(dir, cfg!(windows), false)`), not
/// ad hoc `\` rewriting. The workspace walk consults this before pruning an
/// excluded-name directory: a declared hand-written file keeps its ancestor
/// directories reachable so the file can be read and indexed (#7386). Only
/// the feature-gated discovery walk calls it.
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn declared_typescript_handwritten_under(declared: &[String], dir: &Path) -> bool {
    let mut prefix = crate::output::path::path_identity_key_on(dir, cfg!(windows), false);
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
/// no `ripr.toml` exists. `package.json` is deliberately not a marker on its own
/// (see `detect_javascript_led_root` for the JavaScript-led exception): Rust
/// and Python repositories routinely carry one for docs or tooling, and it
/// does not say the repository's own tests run under a TS/JS runner.
///
/// The detection surface (this constant through
/// `dir_contains_typescript_source` below) compiles only with the adapter or
/// for tests: without `lang-typescript` the inert `typescript_absent` forms
/// in `config.rs` keep no-config detection from ever selecting the adapter,
/// so the real functions would be dead code in a rust-only build. The
/// exclusion authorities above stay real in every lane (#10730).
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) const TYPESCRIPT_PROJECT_MARKERS: &[&str] = &["tsconfig.json", "jsconfig.json"];
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) const TYPESCRIPT_SOURCE_DIR_MARKERS: &[&str] = &["src", "tests"];

/// No-config project detection, mirroring `detect_python_project`: a root
/// marker file, or detectable TypeScript source below root `src`/`tests`.
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn detect_typescript_project(root: &Path) -> bool {
    detect_typescript_package_root(root, false)
        || detect_nested_typescript_package(root)
        || detect_javascript_led_root(root)
}

/// A package-led root (#11336): `package.json` at the root plus JavaScript or
/// TypeScript source (not tooling config files) at the root or under
/// `src`/`test`/`tests` (beside a `Cargo.toml`, only the root and `src`: Rust
/// `tests/` hold integration tests and fixtures, #11418). A Cargo or Python
/// project at the same root does not exclude it: a polyglot repository whose JS/TS changes were reported as
/// "not analyzed" is the case this exists for. `package.json` alone stays a
/// non-marker (#10941), as does JavaScript with no manifest.
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn detect_javascript_led_root(root: &Path) -> bool {
    JAVASCRIPT_LED_MANIFESTS
        .iter()
        .any(|manifest| root.join(manifest).is_file())
        && (dir_has_package_source_file(root) || {
            let has_cargo_toml = root.join("Cargo.toml").is_file();
            JAVASCRIPT_LED_SOURCE_DIRS
                .iter()
                // Beside a Rust crate, `test`/`tests` hold Rust integration
                // tests and fixtures, so JavaScript there is not the
                // project's own source (#11418).
                .filter(|dir| !(has_cargo_toml && **dir != "src"))
                .any(|dir| dir_contains_package_source(&root.join(dir)))
        })
}

/// Root manifest and source directories that `detect_javascript_led_root`
/// reads, shared with the LSP watcher classification and the seam-cache
/// identity so they cannot drift from the detector.
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) const JAVASCRIPT_LED_MANIFESTS: &[&str] = &["package.json"];
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) const JAVASCRIPT_LED_SOURCE_DIRS: &[&str] = &["src", "test", "tests"];

/// Canonical root manifest name for a candidate name (host path rule).
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn javascript_led_manifest_name(name: &str) -> Option<&'static str> {
    canonical_marker_name(JAVASCRIPT_LED_MANIFESTS, name)
}

/// Canonical root source-directory marker (`src`, `test` or `tests`) for a
/// path component, with the same host path rule as the TypeScript and Python
/// marker names. The detector resolves these directories through the
/// filesystem (`root.join(dir)`), so on a case-insensitive host a
/// differently-cased spelling (`Test`, `Src`) detects the same source the
/// watcher must classify (#11419).
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn javascript_led_source_dir_marker_name(name: &str) -> Option<&'static str> {
    canonical_marker_name(JAVASCRIPT_LED_SOURCE_DIRS, name)
}

#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn is_detectable_package_source_name(name: &str) -> bool {
    let is_javascript = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            crate::analysis::ts_js_source_kind(extension)
                == Some(crate::analysis::TsJsSourceKind::JavaScript)
        });
    (is_javascript && !is_detectable_generated_typescript_name(name)
        || is_detectable_typescript_source_name(name))
        && !is_javascript_tooling_config_name(name)
}

/// Tooling config files (`eslint.config.js`, `webpack.config.mjs`,
/// `.eslintrc.cjs`) say a project uses a bundler or linter, not that its own
/// code is JavaScript, so they never select the adapter on their own. Only
/// dotfiles and `*.config.*` names count; `src.js` and `rc.js` are source.
#[cfg(any(feature = "lang-typescript", test))]
fn is_javascript_tooling_config_name(name: &str) -> bool {
    name.starts_with('.') || name.contains(".config.")
}

/// Direct children only: root-level `index.js` / `test.js` style packages.
#[cfg(any(feature = "lang-typescript", test))]
fn dir_has_package_source_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.file_type().is_ok_and(|kind| kind.is_file())
            && entry
                .file_name()
                .to_str()
                .is_some_and(is_detectable_package_source_name)
    })
}

#[cfg(any(feature = "lang-typescript", test))]
fn dir_contains_package_source(dir: &Path) -> bool {
    dir_has_package_source_file(dir)
        || child_directories(dir)
            .iter()
            .any(|child| dir_contains_package_source(child))
}

#[cfg(any(feature = "lang-typescript", test))]
fn has_typescript_project_marker(dir: &Path) -> bool {
    TYPESCRIPT_PROJECT_MARKERS
        .iter()
        .any(|marker| dir.join(marker).is_file())
}

/// The root-level markers: a project marker file, or detectable TypeScript
/// below a root source directory.
#[cfg(any(feature = "lang-typescript", test))]
///
/// `skip_hidden` keeps nested candidates from enabling the adapter through
/// source that lives only under a hidden directory; the root keeps its
/// historical walk.
fn detect_typescript_package_root(dir: &Path, skip_hidden: bool) -> bool {
    has_typescript_project_marker(dir)
        || TYPESCRIPT_SOURCE_DIR_MARKERS
            .iter()
            .any(|marker| dir_contains_typescript_source(&dir.join(marker), skip_hidden))
}

/// Workspace containers whose children are packages (npm/pnpm/yarn
/// workspaces, Nx and Turborepo layouts).
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) const TYPESCRIPT_WORKSPACE_CONTAINERS: &[&str] =
    &["packages", "apps", "libs", "libraries"];

/// A TypeScript package one level below the root, or one level below a
/// workspace container (`packages/*`, `apps/*`, `libs/*`, `libraries/*`):
/// valibot keeps its sources in `library/`, remeda in `packages/remeda`.
/// Each candidate directory is judged by the same presence-only markers as
/// the root. Excluded, vendored and hidden directories never count, so a
/// `node_modules` or `dist` tree cannot enable the adapter; `package.json`
/// stays a non-marker (#10941).
#[cfg(any(feature = "lang-typescript", test))]
fn detect_nested_typescript_package(root: &Path) -> bool {
    child_directories(root).into_iter().any(|child| {
        let name = child
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if has_typescript_project_marker(&child) {
            return true;
        }
        // `src`/`tests` are the root source markers themselves; the root
        // already walked them, so only their own marker counts. Every other
        // child, workspace container included, gets the root test.
        let is_source_dir = TYPESCRIPT_SOURCE_DIR_MARKERS.contains(&name);
        if !is_source_dir && detect_typescript_package_root(&child, true) {
            return true;
        }
        TYPESCRIPT_WORKSPACE_CONTAINERS.contains(&name)
            && child_directories(&child)
                .iter()
                .any(|package| detect_typescript_package_root(package, true))
    })
}

/// Immediate subdirectories of `dir` that a TypeScript walk would enter:
/// not excluded, not hidden, not a symlink.
#[cfg(any(feature = "lang-typescript", test))]
fn child_directories(dir: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            entry.file_name().to_str().is_some_and(|name| {
                !name.starts_with('.') && !is_typescript_excluded_dir_everywhere(name)
            })
        })
        .map(|entry| entry.path())
        .collect()
}

/// Canonical root marker file name for a candidate name. Markers resolve
/// through the filesystem, so the comparison follows the host path rule.
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn typescript_project_marker_name(name: &str) -> Option<&'static str> {
    canonical_marker_name(TYPESCRIPT_PROJECT_MARKERS, name)
}

/// Canonical root source-directory marker for a path component.
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn typescript_source_dir_marker_name(name: &str) -> Option<&'static str> {
    canonical_marker_name(TYPESCRIPT_SOURCE_DIR_MARKERS, name)
}

#[cfg(any(feature = "lang-typescript", test))]
fn canonical_marker_name(candidates: &[&'static str], name: &str) -> Option<&'static str> {
    canonical_marker_name_on(
        candidates,
        name,
        crate::output::path::host_paths_case_insensitive(),
    )
}

/// Explicit-rule form of [`canonical_marker_name`] so tests discriminate the
/// case-insensitive spelling on any host, mirroring the Python helper.
#[cfg(any(feature = "lang-typescript", test))]
fn canonical_marker_name_on(
    candidates: &[&'static str],
    name: &str,
    case_insensitive: bool,
) -> Option<&'static str> {
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
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn is_detectable_typescript_source_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            crate::analysis::ts_js_source_kind(extension)
                == Some(crate::analysis::TsJsSourceKind::TypeScript)
        })
        && !is_detectable_generated_typescript_name(name)
        && !is_typescript_declaration_name(name)
}

/// `.d.ts`, `.d.mts` and `.d.cts` declaration files. The adapter skips them
/// when generating findings, so declarations alone never select it.
#[cfg(any(feature = "lang-typescript", test))]
fn is_typescript_declaration_name(name: &str) -> bool {
    [".d.ts", ".d.mts", ".d.cts"]
        .iter()
        .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix))
}

/// Whether a root source-directory marker contains detectable TypeScript
/// source. Presence-only; the bound value is this boolean.
#[cfg(any(feature = "lang-typescript", test))]
pub(crate) fn source_dir_contains_detectable_typescript(root: &Path, marker: &str) -> bool {
    dir_contains_typescript_source(&root.join(marker), false)
}

#[cfg(any(feature = "lang-typescript", test))]
fn dir_contains_typescript_source(dir: &Path, skip_hidden: bool) -> bool {
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
            let hidden = skip_hidden && name.starts_with('.');
            if !hidden
                && !is_typescript_excluded_dir_everywhere(name)
                && dir_contains_typescript_source(&path, skip_hidden)
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
    fn slash_separated_handwritten_typescript_path_is_still_admitted() {
        let declared = ["a/b.ts".to_string()];
        assert!(is_declared_handwritten_typescript_path(
            &declared,
            Path::new("a/b.ts")
        ));
        assert!(declared_typescript_handwritten_under(
            &["pkg/gen/x.ts".to_string()],
            Path::new("pkg/gen")
        ));
        assert!(!is_declared_handwritten_typescript_path(
            &["A/B.ts".to_string()],
            Path::new("a/b.ts")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn unix_literal_backslash_typescript_path_is_not_rewritten_to_slash() {
        let declared = ["a/b.ts".to_string()];
        assert!(
            !is_declared_handwritten_typescript_path(&declared, Path::new("a\\b.ts")),
            "Unix `\\` is a filename byte, so `a\\b.ts` must not match stored `a/b.ts`"
        );
        assert!(
            !declared_typescript_handwritten_under(
                &["pkg/gen/x.ts".to_string()],
                Path::new("pkg\\gen")
            ),
            "Unix `pkg\\gen` is not a prefix of stored `pkg/gen/x.ts`"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_backslash_typescript_path_matches_slash_declaration() {
        assert!(is_declared_handwritten_typescript_path(
            &["a/b.ts".to_string()],
            Path::new("a\\b.ts")
        ));
        assert!(declared_typescript_handwritten_under(
            &["pkg/gen/x.ts".to_string()],
            Path::new("pkg\\gen")
        ));
    }

    #[test]
    fn non_utf8_typescript_path_is_never_admitted_as_handwritten() {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            let name = std::ffi::OsString::from_wide(&[
                0x0061, 0x005C, 0xD800, 0x0062, 0x002E, 0x0074, 0x0073,
            ]);
            assert!(name.to_str().is_none(), "premise: the name is not UTF-8");
            let path = Path::new(&name);
            let lossy = path.to_string_lossy().replace('\\', "/");
            assert!(!is_declared_handwritten_typescript_path(&[lossy], path));
            assert!(!declared_typescript_handwritten_under(
                &["a/b.ts".to_string()],
                path
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let name = std::ffi::OsStr::from_bytes(b"a/\xFFb.ts");
            assert!(name.to_str().is_none(), "premise: the name is not UTF-8");
            let path = Path::new(name);
            let lossy = path.to_string_lossy().replace('\\', "/");
            assert!(
                !is_declared_handwritten_typescript_path(&[lossy], path),
                "lossy U+FFFD spelling must not admit a non-UTF-8 candidate"
            );
            assert!(!declared_typescript_handwritten_under(
                &["a/b.ts".to_string()],
                path
            ));
        }
    }

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
            // A bare `baselines` directory is ordinary source; only the
            // compiler's `baselines/reference` and `baselines/local` children
            // are reference-output trees (#11984 item 2).
            "src/baselines/foo.ts",
            "src/reference/foo.ts",
            "src/local/foo.ts",
            "src/baselines/reference.ts",
        ] {
            assert!(
                !is_detectable_excluded_typescript_path(Path::new(path)),
                "{path} is ordinary source"
            );
        }
        for path in [
            "tests/baselines/reference/case.js",
            "tests/baselines/local/case.js",
            "src/tests/baselines/reference/nested.ts",
            "src/__snapshots__/lib.test.ts.snap",
            "packages/app/src/__snapshots__/a.ts.snap",
        ] {
            assert!(
                is_detectable_excluded_typescript_path(Path::new(path)),
                "{path} is a reference-output subtree"
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
        // Path-shaped reference-output trees must not exclude a bare
        // `baselines`, `reference`, or `local` name: a real source directory
        // can use those names (#11984 item 2).
        for name in ["baselines", "reference", "local"] {
            assert!(
                !is_typescript_dir_pruned_from_discovery(name),
                "{name} is too broad to prune by name"
            );
            assert!(!is_typescript_excluded_dir_everywhere(name));
        }
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
        assert!(
            !detect_typescript_project(&root),
            "package.json alone does not select the adapter"
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

    /// A Rust crate's `tests/` holds Rust integration tests and fixtures, so
    /// JavaScript there beside a `package.json` (wasm, napi, docs tooling) is
    /// not the project's own source; `src/` and the root still count (#11418).
    #[test]
    fn rust_crate_fixture_javascript_does_not_select_the_adapter() -> Result<(), String> {
        let root = detection_root("js-fixture-in-rust")?;
        write(&root, "package.json")?;
        write(&root, "Cargo.toml")?;
        write(&root, "tests/fixtures/app/foo.js")?;
        write(&root, "test/fixtures/bar.ts")?;
        assert!(
            !detect_typescript_project(&root),
            "fixture JavaScript under tests/ beside Cargo.toml"
        );
        write(&root, "src/pricing.js")?;
        assert!(
            detect_typescript_project(&root),
            "JavaScript under src/ beside Cargo.toml still counts"
        );
        std::fs::remove_file(root.join("Cargo.toml")).map_err(|err| err.to_string())?;
        std::fs::remove_file(root.join("src/pricing.js")).map_err(|err| err.to_string())?;
        assert!(
            detect_typescript_project(&root),
            "the same tests/ JavaScript counts when there is no Cargo.toml"
        );
        Ok(())
    }

    /// #11336: a package-led root (manifest plus JavaScript/TypeScript source,
    /// Cargo or Python project or not) selects the adapter; a manifest alone,
    /// tooling config files, or excluded/generated JavaScript do not (#10941).
    #[test]
    fn javascript_led_roots_select_the_adapter() -> Result<(), String> {
        let root = detection_root("js-led")?;
        write(&root, "package.json")?;
        assert!(!detect_typescript_project(&root), "manifest only");
        write(&root, "node_modules/dep/index.js")?;
        write(&root, "dist/bundle.js")?;
        write(&root, "public/app.min.js")?;
        assert!(
            !detect_typescript_project(&root),
            "excluded and generated JavaScript"
        );
        write(&root, "eslint.config.js")?;
        write(&root, "webpack.config.mjs")?;
        write(&root, ".eslintrc.cjs")?;
        assert!(
            !detect_typescript_project(&root),
            "tooling config files are not source"
        );
        write(&root, "vite.config.ts")?;
        write(&root, "jest.config.ts")?;
        assert!(
            !detect_typescript_project(&root),
            "TypeScript tooling config is not source either"
        );
        write(&root, "index.js")?;
        assert!(detect_typescript_project(&root), "root-level JavaScript");
        write(&root, "Cargo.toml")?;
        assert!(detect_typescript_project(&root), "polyglot Rust root");
        std::fs::remove_file(root.join("Cargo.toml")).map_err(|err| err.to_string())?;
        write(&root, "pyproject.toml")?;
        assert!(detect_typescript_project(&root), "polyglot Python root");
        std::fs::remove_file(root.join("pyproject.toml")).map_err(|err| err.to_string())?;
        std::fs::remove_file(root.join("index.js")).map_err(|err| err.to_string())?;
        write(&root, "src.js")?;
        assert!(
            detect_typescript_project(&root),
            "`src.js` is source, not rc"
        );
        std::fs::remove_file(root.join("src.js")).map_err(|err| err.to_string())?;
        write(&root, "index.ts")?;
        assert!(detect_typescript_project(&root), "root-level TypeScript");
        std::fs::remove_file(root.join("index.ts")).map_err(|err| err.to_string())?;
        write(&root, "test/unit.mjs")?;
        assert!(detect_typescript_project(&root), "JavaScript under test/");
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    /// #11419: package-led source-directory markers canonicalize with the
    /// host path rule, like the manifest and the other marker names. On a
    /// case-insensitive host (`Test`, `Src`) canonicalizes; on a
    /// case-sensitive host only the canonical spelling does.
    #[test]
    fn javascript_led_source_dir_markers_canonicalize_with_the_host_path_rule() {
        for dir in JAVASCRIPT_LED_SOURCE_DIRS {
            assert_eq!(javascript_led_source_dir_marker_name(dir), Some(*dir));
        }
        assert_eq!(javascript_led_source_dir_marker_name("lib"), None);
        let insensitive = crate::output::path::host_paths_case_insensitive();
        for (spelling, canonical) in [("Src", "src"), ("Test", "test"), ("TESTS", "tests")] {
            assert_eq!(
                javascript_led_source_dir_marker_name(spelling),
                insensitive.then_some(canonical),
                "{spelling} canonicalizes only on a case-insensitive host"
            );
        }
        // Explicit-rule form discriminates on any host, mirroring the Python
        // marker test: the exact pre-fix comparison rejected these outright.
        for (spelling, canonical) in [("Src", "src"), ("Test", "test"), ("TESTS", "tests")] {
            assert_eq!(
                canonical_marker_name_on(JAVASCRIPT_LED_SOURCE_DIRS, spelling, true),
                Some(canonical)
            );
            assert_eq!(
                canonical_marker_name_on(JAVASCRIPT_LED_SOURCE_DIRS, spelling, false),
                None
            );
        }
        assert_eq!(
            canonical_marker_name_on(JAVASCRIPT_LED_SOURCE_DIRS, "src", false),
            Some("src")
        );
    }

    #[test]
    fn excluded_and_generated_source_does_not_enable_detection() -> Result<(), String> {
        let root = detection_root("excluded")?;
        write(&root, "src/vendor/lib.ts")?;
        write(&root, "src/node_modules/pkg/index.ts")?;
        write(&root, "src/api.generated.ts")?;
        write(&root, "src/types.d.ts")?;
        assert!(!detect_typescript_project(&root));
        write(&root, "tests/cart.test.ts")?;
        assert!(detect_typescript_project(&root), "harness sanity");
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }
}
