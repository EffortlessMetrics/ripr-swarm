//! Workspace discovery for the TypeScript preview adapter.

use super::*;

const TEST_FILE_STEM_SUFFIXES: &[&str] =
    &[".test", "-test", "_test", ".spec", ".test-d", ".spec-d"];
const TEST_DIRECTORY_NAMES: &[&str] = &["test", "tests", "__tests__"];
// Every source extension routed to this adapter (router.rs), so Cypress
// `*.cy.<ext>` classification stays bounded to real adapter source surface.
const CYPRESS_SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"];
const JASMINE_SPEC_DIRECTORY_NAMES: &[&str] = &["spec"];

/// Whether a path is a test file by convention.
///
/// The adapter recognizes four bounded convention families:
///
/// 1. Jest/Vitest-style `*.test.*` and `*.spec.*` files, plus tsd/vitest
///    type-test `*.test-d.*` and `*.spec-d.*` stems, across every
///    TypeScript/JavaScript source extension routed to this adapter
///    (.ts/.tsx/.js/.jsx/.mts/.cts/.mjs/.cjs). Exact suffixes only: a plain
///    `*.d.ts` declaration and a production name like `foo.test-data.ts`
///    stay non-test.
/// 2. Node-style names: `test.*`, `test-*`, `*-test.*`, and `*_test.*`.
/// 3. Cypress `*.cy.{ts,tsx,js,jsx,mts,cts,mjs,cjs}` files.
/// 4. Source files under exact `test`, `tests`, or `__tests__` directory
///    components, plus Jasmine-style `spec/**/[sS]pec.*` paths.
///
/// Directory matching is component-based, not substring-based, so
/// `src/latest/foo.ts`, `test-utils/foo.ts`, and `src/contest.ts` remain
/// production paths. The language router is checked before any naming or
/// directory convention, preventing discovery policy from drifting beyond the
/// adapter's real source surface. Test extraction remains fail-closed: a
/// recognized path contributes test evidence only when parsing finds supported
/// `test()` / `it()` / `describe()` call shapes.
///
/// The exact test-directory rule retains the controlled ky dogfood case:
/// `test/body-size.ts` directly exercised a changed owner but was invisible
/// before directory classification, producing a false `no_static_path`.
pub(crate) fn is_test_file(path: &Path) -> bool {
    is_typescript_or_javascript_source(path)
        && (has_test_file_stem(path) || has_test_directory_component(path))
}

/// `true` for a TypeScript declaration file (`.d.ts`, `.d.mts`, `.d.cts`):
/// type-only, erased at compile time, so its changes carry no runtime
/// behavior.
pub(crate) fn is_typescript_declaration_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            [".d.ts", ".d.mts", ".d.cts"]
                .iter()
                .any(|s| name.ends_with(s))
        })
}

fn is_typescript_or_javascript_source(path: &Path) -> bool {
    let adapter = TypeScriptAdapter;
    adapter.accepts_path(path)
}

fn has_extension_in(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.contains(&extension))
}

fn has_test_file_stem(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };

    stem == "test"
        || stem
            .strip_prefix("test-")
            .is_some_and(|remainder| !remainder.is_empty())
        || TEST_FILE_STEM_SUFFIXES
            .iter()
            .any(|suffix| stem.ends_with(suffix))
        || (stem.ends_with(".cy") && has_extension_in(path, CYPRESS_SOURCE_EXTENSIONS))
        || has_jasmine_spec_stem(path, stem)
}

fn has_jasmine_spec_stem(path: &Path, stem: &str) -> bool {
    (stem.ends_with("Spec") || stem.ends_with("spec"))
        && has_directory_component(path, JASMINE_SPEC_DIRECTORY_NAMES)
}

fn has_test_directory_component(path: &Path) -> bool {
    has_directory_component(path, TEST_DIRECTORY_NAMES)
}

fn has_directory_component(path: &Path, names: &[&str]) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| names.contains(&name))
    })
}

// Directory prune authority for the workspace walk: a single table owned by
// `config::typescript` (`TYPESCRIPT_EXCLUDED_DIRS`), shared with the diff
// loop's pre-count refusal (#3743). The walk must prune exactly the trees
// the diff loop refuses, otherwise the denominator counts files no facts
// can back, or facts exist for files the denominator hides.
// `*.generated.*` files are additionally skipped at the file level below.

/// Env override for [`DEFAULT_TS_MAX_WORKSPACE_FILES`].
pub(crate) const TS_MAX_WORKSPACE_FILES_ENV: &str = "RIPR_TS_MAX_WORKSPACE_FILES";
/// Default ceiling on candidate TS/JS source files during workspace
/// discovery (routed extensions minus the generated-name family). Directories,
/// assets, and `*.generated.*` files do not consume this budget. A separate
/// total-entry backstop still terminates huge trees: the default 20_000
/// candidates imply a 100_000-entry walk before the walk gives up.
const DEFAULT_TS_MAX_WORKSPACE_FILES: usize = 20_000;
/// Multiplier for the total-entry walk backstop relative to the candidate
/// source-file limit. Default `20_000 × 5 = 100_000` directory entries.
/// A huge asset tree still terminates even when few (or no) TypeScript/JavaScript
/// sources exist. 5× (not 10×) is the largest integer that still trips the
/// #11767 producer fixture (`RIPR_TS_MAX_WORKSPACE_FILES=3`, one `.ts` file
/// plus a dozen `docs/*.txt`) via this backstop after candidate-only counting.
const TS_WORKSPACE_TOTAL_ENTRY_BACKSTOP_MULTIPLIER: usize = 5;

/// Total directory-entry backstop for a candidate-source `limit`
/// (`limit × 5`). Disclosure text names it beside the limit because the single
/// `truncated` flag cannot say which bound tripped.
pub(crate) fn ts_workspace_entry_backstop(limit: usize) -> usize {
    limit.saturating_mul(TS_WORKSPACE_TOTAL_ENTRY_BACKSTOP_MULTIPLIER)
}

#[cfg(test)]
thread_local! {
    /// Directory entries seen by the last [`visit_workspace_with_handwritten`]
    /// on this thread, including the entry that tripped a cap.
    static LAST_WALK_TOTAL_ENTRIES: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

/// Parse a positive workspace file-count limit, failing closed to the error
/// string on invalid input (mirrors `rust/mod.rs::positive_limit_from_env`).
pub(crate) fn ts_workspace_file_limit_from_env(
    value: Result<String, std::env::VarError>,
) -> Result<usize, String> {
    match value {
        Ok(raw) => {
            let parsed = raw.trim().parse::<usize>().map_err(|err| {
                format!("{TS_MAX_WORKSPACE_FILES_ENV} must be a positive integer: {err}; set it to a whole number above zero, or unset it to use the default")
            })?;
            if parsed == 0 {
                return Err(format!(
                    "{TS_MAX_WORKSPACE_FILES_ENV} must be a positive integer; set it to a whole number above zero, or unset it to use the default"
                ));
            }
            Ok(parsed)
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_TS_MAX_WORKSPACE_FILES),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "{TS_MAX_WORKSPACE_FILES_ENV} must be a positive integer; set it to a whole number above zero, or unset it to use the default"
        )),
    }
}

/// Resolved candidate-source limit, failing closed to the default so a
/// malformed operator override cannot abort analysis.
pub(crate) fn ts_workspace_file_limit() -> usize {
    ts_workspace_file_limit_from_env(std::env::var(TS_MAX_WORKSPACE_FILES_ENV))
        .unwrap_or(DEFAULT_TS_MAX_WORKSPACE_FILES)
}

/// Workspace file discovery result: the accepted source files plus whether
/// the candidate-source cap or the total-entry backstop tripped (making
/// `files` a partial set), and how many symlink/junction entries were seen
/// but deliberately not followed (#4104-D: the count feeds a disclosure so
/// link-hidden tests are not silently invisible).
pub(crate) struct WorkspaceScan {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) truncated: bool,
    pub(crate) skipped_links: usize,
}

/// The workspace scan with the `[languages.typescript] handwritten_files`
/// declarations (#7386): directories are still pruned at the excluded-name
/// boundary, except an ancestor of a declared hand-written file, which stays
/// reachable so the declared file is read and indexed.
pub(crate) fn collect_workspace_typescript_files_with_handwritten(
    root: &Path,
    handwritten: &[String],
) -> WorkspaceScan {
    let max_entries = ts_workspace_file_limit_from_env(std::env::var(TS_MAX_WORKSPACE_FILES_ENV))
        .unwrap_or(DEFAULT_TS_MAX_WORKSPACE_FILES);
    visit_workspace_with_handwritten(root, max_entries, handwritten)
}

/// Whether `path` is a TS/JS source the walker admits: a routed extension,
/// minus `*.generated.*` (and the same generated-name family the walk
/// already refuses). Directories, assets, and other non-source entries are
/// not candidates and do not consume [`TS_MAX_WORKSPACE_FILES_ENV`].
fn is_candidate_workspace_source(path: &Path) -> bool {
    TypeScriptAdapter.accepts_path(path) && !is_detectable_generated_typescript_path(path)
}

/// Iterative workspace walk over an explicit work stack.
///
/// Recursion was replaced by a loop so pathologically deep directory trees
/// cannot overflow the call stack (a hard abort). Only candidate TS/JS source
/// files count against `max_entries` (#11984). Every directory entry still
/// counts against a larger total-entry backstop (`5× max_entries`) so a huge
/// asset tree terminates. Exceeding either cap stops the walk and reports
/// `truncated: true` so the adapter can disclose the bound instead of
/// silently analyzing a partial workspace.
pub(crate) fn visit_workspace_with_handwritten(
    root: &Path,
    max_entries: usize,
    handwritten: &[String],
) -> WorkspaceScan {
    let max_total_entries = ts_workspace_entry_backstop(max_entries);
    let mut out = Vec::new();
    let mut candidates = 0usize;
    let mut total_entries = 0usize;
    let mut truncated = false;
    let mut skipped_links = 0usize;
    let mut alias_links: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        // Sorted names make DFS LIFO visit later names first (`src` before
        // `docs`), so a tiny operator cap still indexes the source file
        // before the docs tree trips the total-entry backstop.
        let mut dir_entries: Vec<_> = entries.flatten().collect();
        dir_entries.sort_by_key(|entry| entry.file_name());
        let mut stop = false;
        for entry in dir_entries {
            total_entries += 1;
            if total_entries > max_total_entries {
                truncated = true;
                stop = true;
                break;
            }
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            // `*.generated.*` files are additionally skipped at the file level
            // below. A declared hand-written file keeps an excluded-name
            // ancestor reachable instead (#7386).
            if is_typescript_dir_pruned_from_discovery(name)
                && !crate::config::declared_typescript_handwritten_under(
                    handwritten,
                    path.strip_prefix(root).unwrap_or(&path),
                )
            {
                continue;
            }
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                // Symlinks and NTFS junctions are NOT followed (fail-closed:
                // following them risks cycles and outside-root reads), but
                // the skip is counted so the adapter can disclose that
                // link-hidden tests/sources were not seen (#4104-D). A link
                // that is itself a regular file under a name the adapter
                // would never index (a docs `0.index.md -> README.md`) hides
                // no test or source: the walk indexes by the entry's own
                // path, so it is skipped without a disclosure. Anything
                // else (a directory, a source-named link, a dangling or
                // unreadable target) stays counted.
                match classify_symlink(&path, root) {
                    LinkKind::Harmless => {}
                    LinkKind::DirectoryAlias => {
                        alias_links.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
                    }
                    LinkKind::MayHideSources => skipped_links += 1,
                }
                continue;
            }
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() && is_candidate_workspace_source(&path) {
                candidates += 1;
                if candidates > max_entries {
                    truncated = true;
                    stop = true;
                    break;
                }
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                if !crate::config::is_detectable_excluded_typescript_path(&relative)
                    || crate::config::is_declared_handwritten_typescript_path(
                        handwritten,
                        &relative,
                    )
                {
                    out.push(relative);
                }
            } else if file_type.is_file() {
                // A regular file the filters above refused (a `*.generated.*`
                // source, a minified bundle, or a non-source asset): excluded
                // by policy, not a link-hidden entry, so it must not inflate
                // the skipped-link disclosure or the candidate-source cap.
            } else if is_special_non_link_file(&file_type) {
                // FIFOs, sockets and devices (Unix) are neither directories,
                // regular files, nor links and cannot carry TypeScript
                // sources. Counting them as skipped links would disclose a
                // symlink/junction limitation a workspace without any link
                // does not have (review #4138): they stay unanalyzed without
                // claiming link-hidden coverage loss.
            } else {
                // Reparse points std does not report as symlinks (notably
                // NTFS junctions on some toolchains) land here: neither dir
                // nor file. They are equally unfollowable — count them in
                // the same disclosure instead of silently dropping the
                // entry (#4104-D).
                skipped_links += 1;
            }
        }
        if stop {
            stack.clear();
            break;
        }
    }
    #[cfg(test)]
    LAST_WALK_TOTAL_ENTRIES.set(total_entries);
    out.sort();
    if !alias_links.is_empty() {
        skipped_links += alias_links
            .iter()
            .filter(|alias| any_file_imports_through(root, &out, alias))
            .count();
    }
    WorkspaceScan {
        files: out,
        truncated,
        skipped_links,
    }
}

/// Largest indexed file read when checking for imports through an alias; a
/// bigger file cannot be shown free of one, so it counts as traversing.
const ALIAS_IMPORT_SCAN_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// `true` when an indexed file may name a path through the alias link.
///
/// A parsed file counts when one of its string or template literals is a
/// relative specifier (`./x`, `../x`) that resolves, lexically from the
/// importing file's directory, to the link or a path beneath it. Resolving
/// the specifier decides which directory it names, so no character of the
/// link's or its target's name can be mistaken for a component boundary, and
/// prose, comments and bare specifiers (resolved through disclosed
/// resolution config) do not count. An absolute-looking literal (`/...`)
/// cannot be resolved lexically and counts when it spells `/<name>` as a path
/// component. A file that does not parse cleanly, an unreadable file and an
/// oversized file cannot be ruled out: the first counts on a raw-text
/// `/<name>` match or any character escape, the others always count. Decoded
/// literal values are used, so `'../al\u0069as/lib'` and `'..\u002falias/lib'`
/// match.
fn any_file_imports_through(root: &Path, files: &[PathBuf], alias: &Path) -> bool {
    let Some(name) = alias.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    let needle = format!("/{name}");
    let alias_parts = path_parts(alias);
    files.iter().any(|file| {
        let full = root.join(file);
        let Ok(metadata) = std::fs::metadata(&full) else {
            return true;
        };
        if metadata.len() > ALIAS_IMPORT_SCAN_MAX_BYTES {
            return true;
        }
        let Ok(text) = std::fs::read_to_string(&full) else {
            return true;
        };
        literal_reaches_alias(&text, file, &needle, &alias_parts)
    })
}

/// Normal components of a root-relative path as strings.
fn path_parts(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

/// Resolve the relative specifier `value` lexically from the directory of the
/// root-relative `file`, returning its root-relative components, or `None`
/// when it is not relative or leaves the root.
fn resolve_relative_specifier(file: &Path, value: &str) -> Option<Vec<String>> {
    if !(value == "." || value == ".." || value.starts_with("./") || value.starts_with("../")) {
        return None;
    }
    let mut parts = path_parts(file.parent().unwrap_or(Path::new("")));
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other.to_string()),
        }
    }
    Some(parts)
}

/// Whether one decoded literal value reaches the alias (see
/// [`any_file_imports_through`]).
fn value_reaches_alias(value: &str, file: &Path, needle: &str, alias_parts: &[String]) -> bool {
    if value.starts_with('/') {
        return names_alias_component(value, needle);
    }
    resolve_relative_specifier(file, value)
        .is_some_and(|resolved| resolved.starts_with(alias_parts))
}

/// `true` when `needle` (`/<name>`) is a path component of `haystack`, not
/// continued by another name character.
fn names_alias_component(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(start, _)| {
        haystack[start + needle.len()..]
            .chars()
            .next()
            .is_none_or(|next| !(next.is_alphanumeric() || matches!(next, '_' | '-' | '$' | '@')))
    })
}

/// Whether a literal of the parsed file reaches the alias. A file that does
/// not parse cleanly cannot be ruled out: it counts on a raw-text
/// `/<name>` match or any character escape.
fn literal_reaches_alias(text: &str, file: &Path, needle: &str, alias_parts: &[String]) -> bool {
    struct Finder<'n> {
        file: &'n Path,
        needle: &'n str,
        alias_parts: &'n [String],
        found: bool,
    }
    impl<'a> oxc_ast_visit::Visit<'a> for Finder<'_> {
        fn visit_string_literal(&mut self, literal: &oxc_ast::ast::StringLiteral<'a>) {
            self.found |= value_reaches_alias(
                literal.value.as_str(),
                self.file,
                self.needle,
                self.alias_parts,
            );
        }
        fn visit_template_element(&mut self, element: &oxc_ast::ast::TemplateElement<'a>) {
            self.found |= match element.value.cooked.as_deref() {
                Some(cooked) => {
                    value_reaches_alias(cooked, self.file, self.needle, self.alias_parts)
                }
                None => true,
            };
        }
    }
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, text, source_type_for(file)).parse();
    if parsed.panicked || !parsed.errors.is_empty() {
        return names_alias_component(text, needle) || has_character_escape(text);
    }
    let mut finder = Finder {
        file,
        needle,
        alias_parts,
        found: false,
    };
    oxc_ast_visit::Visit::visit_program(&mut finder, &parsed.program);
    finder.found
}

/// `true` when `text` has a backslash followed by `u`, `x` or an octal digit.
fn has_character_escape(text: &str) -> bool {
    text.match_indices('\\').any(|(start, _)| {
        text[start + 1..]
            .chars()
            .next()
            .is_some_and(|next| matches!(next, 'u' | 'x' | '0'..='7'))
    })
}

/// `true` when the symlink at `path` hides nothing the walk would index.
///
/// A file link whose own name the TypeScript adapter does not index (not a
/// source extension, or a generated or minified name the walk refuses) has
/// nothing indexable behind it (h3's `docs/5.migration/0.index.md` link made
/// every h3 analysis partial). Only the target's file-ness is read, never its
/// content.
///
/// A directory link is an alias when its canonical target is a directory
/// inside the canonical root with no excluded component: the walk visits
/// the target under its real path, so the link adds no file (zod's
/// `.tools/skills -> ../.agent/skills`). A target outside the root, in an
/// excluded directory, a dangling link or a failed stat is "could hide
/// sources" and stays disclosed.
fn classify_symlink(path: &Path, root: &Path) -> LinkKind {
    let Ok(metadata) = std::fs::metadata(path) else {
        return LinkKind::MayHideSources;
    };
    if metadata.is_file() {
        return if !TypeScriptAdapter.accepts_path(path)
            || is_detectable_generated_typescript_path(path)
        {
            LinkKind::Harmless
        } else {
            LinkKind::MayHideSources
        };
    }
    if metadata.is_dir() && directory_link_aliases_walked_directory(path, root) {
        LinkKind::DirectoryAlias
    } else {
        LinkKind::MayHideSources
    }
}

enum LinkKind {
    Harmless,
    /// Aliases a directory the walk indexes under its real path; hides
    /// nothing unless an indexed file imports through the link.
    DirectoryAlias,
    MayHideSources,
}

fn directory_link_aliases_walked_directory(path: &Path, root: &Path) -> bool {
    let (Ok(target), Ok(root)) = (std::fs::canonicalize(path), std::fs::canonicalize(root)) else {
        return false;
    };
    // The walk silently skips a directory it cannot list, so an alias of one
    // would hide sources the walk never saw.
    std::fs::read_dir(&target).is_ok()
        && target
            .strip_prefix(&root)
            .is_ok_and(|relative| !crate::config::is_detectable_excluded_typescript_path(relative))
}

/// `true` for directory entry types that are neither directories, regular
/// files, nor links (FIFOs, sockets, character/block devices on Unix). Such
/// entries cannot contain or stand in for TypeScript sources, so discovery
/// skips them without counting them toward the `skipped_links` disclosure.
#[cfg(unix)]
fn is_special_non_link_file(file_type: &std::fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;
    file_type.is_fifo()
        || file_type.is_socket()
        || file_type.is_char_device()
        || file_type.is_block_device()
}

/// Non-Unix platforms expose no FIFO/socket/device entry types through
/// `std::fs::read_dir`; every non-dir/non-file entry is a reparse point and
/// stays in the `skipped_links` disclosure.
#[cfg(not(unix))]
fn is_special_non_link_file(_file_type: &std::fs::FileType) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempWorkspace(PathBuf);

    impl TempWorkspace {
        fn new(label: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir().join(format!(
                "ripr-ts-discovery-{label}-{}-{stamp}",
                std::process::id()
            ));
            let created = fs::create_dir_all(&root);
            assert!(
                created.is_ok(),
                "create temp workspace {}: {:?}",
                root.display(),
                created.err()
            );
            Self(root)
        }

        fn write(&self, relative: &str, source: &str) {
            let absolute = self.0.join(relative);
            let parent = absolute.parent().unwrap_or(Path::new("."));
            let created = fs::create_dir_all(parent);
            assert!(
                created.is_ok(),
                "create fixture dir {}: {:?}",
                parent.display(),
                created.err()
            );
            let written = fs::write(&absolute, source);
            assert!(
                written.is_ok(),
                "write fixture file {}: {:?}",
                absolute.display(),
                written.err()
            );
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_file_names_cover_supported_runner_conventions() {
        for path in [
            "src/cart.test.ts",
            "src/cart.spec.tsx",
            "src/cart-test.js",
            "src/cart_test.jsx",
            "src/test-cart.ts",
            "src/test.tsx",
            "cypress/e2e/checkout.cy.ts",
            "src/Button.cy.tsx",
            // Modern ESM/CJS extensions ride the same conventions now that
            // the router accepts them.
            "src/cart.test.mts",
            "src/cart.spec.cts",
            "src/cart-test.mjs",
            "src/cart_test.cjs",
            "src/Button.cy.mts",
        ] {
            assert!(is_test_file(Path::new(path)), "expected test path: {path}");
        }
    }

    /// tsd/vitest type-test files use `*.test-d.*` / `*.spec-d.*` stems.
    /// Exact suffix only: a plain `*.d.ts` declaration and a production name
    /// like `foo.test-data.ts` stay non-test (#8536). `index.test-d.ts` does
    /// not end with `.d.ts`, so the declaration skip cannot cover it.
    #[test]
    fn type_test_stem_suffixes_match_exact_test_d_and_spec_d_only() {
        for path in [
            "index.test-d.ts",
            "src/index.spec-d.ts",
            "src/cart.test-d.tsx",
            "src/cart.spec-d.mts",
            "src/cart.test-d.js",
            "src/cart.spec-d.cts",
            "src/cart.test-d.mjs",
            "src/cart.spec-d.cjs",
        ] {
            assert!(
                is_test_file(Path::new(path)),
                "expected type-test path: {path}"
            );
            assert!(
                !is_typescript_declaration_file(Path::new(path)),
                "type-test `{path}` ends with -d.ts, not .d.ts, so it is not a declaration skip"
            );
        }
        for path in [
            "index.d.ts",
            "src/types.d.ts",
            "src/types.d.mts",
            "src/types.d.cts",
            "src/foo.test-data.ts",
            "src/foo.test-d-extra.ts",
            "src/foo.spec-data.ts",
            "src/request.spec.d.ts",
        ] {
            assert!(
                !is_test_file(Path::new(path)),
                "unexpected type-test path: {path}"
            );
        }
        assert!(is_typescript_declaration_file(Path::new("index.d.ts")));
        assert!(is_typescript_declaration_file(Path::new(
            "src/request.spec.d.ts"
        )));
    }

    #[test]
    fn test_directories_cover_feature_named_and_jasmine_test_files() {
        for path in [
            "test/body-size.ts",
            "tests/utils.ts",
            "src/__tests__/Header.tsx",
            "packages/core/test/index.js",
            "spec/requestContractSpec.js",
            "spec/request_contractspec.js",
        ] {
            assert!(is_test_file(Path::new(path)), "expected test path: {path}");
        }
    }

    #[test]
    fn test_layout_matching_stays_component_route_and_extension_bounded() {
        for path in [
            "src/latest/feature.ts",
            "test-utils/helper.ts",
            "src/contest.ts",
            "src/cart_test.txt",
            // Routed extensions stay NON-test when no test marker is present;
            // the ESM/CJS variants below were previously unrouted entirely
            // and pinned here as "not test files" — they are still not test
            // files by name, they are simply analyzed as production sources.
            "src/cart.mts",
            "src/cart.cts",
            "src/cart.mjs",
            "src/cart.cjs",
            "spec/request_contract.md",
            "spec/request_contract.js",
            "spec/helpers/setup.js",
            "src/requestContractSpec.js",
            "src/cypress.ts",
            "src/specification.ts",
        ] {
            assert!(
                !is_test_file(Path::new(path)),
                "unexpected test path: {path}"
            );
        }
    }

    /// Regression pins for the previously unrouted ESM/CJS extension family:
    /// with `.mts`/`.cts`/`.mjs`/`.cjs` now routed to this adapter, the
    /// ordinary test conventions apply to them (these paths used to be pinned
    /// as NOT-test files when the router dropped the extensions).
    #[test]
    fn esm_cjs_extensions_follow_test_conventions() {
        for path in [
            "src/cart.test.mts",
            "src/cart.spec.mjs",
            "test/cart.mjs",
            "tests/cart.cts",
            "src/__tests__/cart.cjs",
            "src/cart.cy.mjs",
        ] {
            assert!(is_test_file(Path::new(path)), "expected test path: {path}");
        }
        for path in [
            "src/cart.mts",
            "src/cart.cts",
            "src/cart.mjs",
            "src/cart.cjs",
        ] {
            assert!(
                !is_test_file(Path::new(path)),
                "unexpected test path: {path}"
            );
        }
    }

    #[test]
    fn newly_recognized_layouts_reach_supported_test_call_shapes() {
        let cases = [
            (
                "src/cart_test.ts",
                r#"test("node layout", () => { expect(cart()).toBe(1); });"#,
            ),
            (
                "cypress/e2e/cart.cy.ts",
                r#"describe("cart", () => { it("checks out", () => { expect(cart()).toBe(1); }); });"#,
            ),
            (
                "spec/cartContractSpec.js",
                r#"describe("cart", () => { it("keeps its contract", () => { expect(cart()).toBe(1); }); });"#,
            ),
        ];

        for (path, source) in cases {
            let path = Path::new(path);
            let display = path.display();
            assert!(is_test_file(path), "expected test path: {display}");
            assert_eq!(
                extract_tests(path, source).len(),
                1,
                "expected one extracted test for {display}"
            );
        }
    }

    #[test]
    fn nested_source_files_are_still_discovered() {
        let workspace = TempWorkspace::new("nested");
        workspace.write("src/main.ts", "export const a = 1;\n");
        workspace.write("packages/core/src/index.tsx", "export const b = 2;\n");
        workspace.write("packages/core/README.md", "not source\n");
        let scan = visit_workspace_with_handwritten(&workspace.0, 10_000, &[]);
        assert!(!scan.truncated, "scan must not trip the cap");
        assert_eq!(
            scan.files,
            vec![
                PathBuf::from("packages/core/src/index.tsx"),
                PathBuf::from("src/main.ts"),
            ]
        );
    }

    #[test]
    fn generated_output_directories_are_not_discovered() {
        let workspace = TempWorkspace::new("generated-dirs");
        workspace.write("src/keep.ts", "export const kept = 1;\n");
        for generated in [
            "dist/bundle.ts",
            "build/output.ts",
            "out/tsc.ts",
            "coverage/lcov.ts",
            ".next/server.ts",
            ".cache/sw.ts",
            "vendor/lib.ts",
            "node_modules/pkg/index.ts",
            "target/debug/build.ts",
        ] {
            workspace.write(generated, "export const generated = 1;\n");
        }
        let scan = visit_workspace_with_handwritten(&workspace.0, 10_000, &[]);
        assert!(!scan.truncated, "scan must not trip the cap");
        assert_eq!(
            scan.files,
            vec![PathBuf::from("src/keep.ts")],
            "generated/vendor dirs must be excluded; got {:?}",
            scan.files
        );
    }

    #[test]
    fn declared_handwritten_file_under_excluded_dir_is_discovered() {
        // #7386: `[languages.typescript] handwritten_files` admits an exact
        // repository-relative path past the excluded-directory conventions.
        // The declared file is indexed; sibling files under the same
        // excluded directory stay skipped.
        let workspace = TempWorkspace::new("handwritten-ts-walk");
        workspace.write("src/keep.ts", "export const kept = 1;\n");
        workspace.write("dist/pricing.ts", "export const price = 1;\n");
        workspace.write("dist/bundle.ts", "export const bundled = 1;\n");
        let declared = vec!["dist/pricing.ts".to_string()];
        let scan = visit_workspace_with_handwritten(&workspace.0, 10_000, &declared);
        assert!(!scan.truncated, "scan must not trip the cap");
        assert_eq!(
            scan.files,
            vec![
                PathBuf::from("dist/pricing.ts"),
                PathBuf::from("src/keep.ts"),
            ],
            "declared hand-written file must be discovered; got {:?}",
            scan.files
        );
    }

    #[test]
    fn workspace_file_limit_trips_truncation_disclosure() {
        let workspace = TempWorkspace::new("file-limit");
        workspace.write("a/one.ts", "export const a = 1;\n");
        workspace.write("b/two.ts", "export const b = 2;\n");
        workspace.write("c/three.ts", "export const c = 3;\n");
        // Cap below the number of admitted source files. Directories no
        // longer consume the budget, so a limit of 2 (not 4) is what
        // truncates three sibling sources.
        let scan = visit_workspace_with_handwritten(&workspace.0, 2, &[]);
        assert!(scan.truncated, "cap must trip the truncated flag");
        assert!(
            scan.files.len() < 3,
            "partial scan must not report the full set; got {:?}",
            scan.files
        );
        // Without a cap the same workspace scans clean.
        let full = visit_workspace_with_handwritten(&workspace.0, 10_000, &[]);
        assert!(!full.truncated);
        assert_eq!(full.files.len(), 3);
    }

    /// #11984 item 1: a large non-source tree must not consume the source
    /// cap. `RIPR_TS_MAX_WORKSPACE_FILES` counts candidate TS/JS files
    /// (routed extensions minus `*.generated.*`), so a limit below
    /// `N + sources` still finds every real source and does not report
    /// truncation.
    #[test]
    fn workspace_file_limit_counts_source_candidates_not_assets() {
        let workspace = TempWorkspace::new("candidate-count");
        workspace.write("src/keep.ts", "export const keep = 1;\n");
        workspace.write("src/also.ts", "export const also = 2;\n");
        workspace.write("src/extra.js", "export const extra = 3;\n");
        for i in 0..12 {
            workspace.write(&format!("assets/blob-{i}.png"), "not source\n");
        }
        for i in 0..6 {
            workspace.write(
                &format!("src/cart-{i}.generated.ts"),
                "export const generated = 1;\n",
            );
        }
        // 12 pngs + 6 generated + 3 sources + dirs is well above 8; counting
        // every directory entry therefore truncates before this change.
        let scan = visit_workspace_with_handwritten(&workspace.0, 8, &[]);
        assert!(
            !scan.truncated,
            "non-source and generated entries must not trip the source cap; files={:?}",
            scan.files
        );
        assert_eq!(
            scan.files,
            vec![
                PathBuf::from("src/also.ts"),
                PathBuf::from("src/extra.js"),
                PathBuf::from("src/keep.ts"),
            ],
            "every admitted source must be discovered; got {:?}",
            scan.files
        );
    }

    /// More candidate source files than the candidate limit still truncates.
    #[test]
    fn workspace_file_limit_truncates_when_sources_exceed_the_cap() {
        let workspace = TempWorkspace::new("source-cap");
        for i in 0..5 {
            workspace.write(&format!("src/f{i}.ts"), "export const v = 1;\n");
        }
        let scan = visit_workspace_with_handwritten(&workspace.0, 2, &[]);
        assert!(scan.truncated, "more sources than the cap must truncate");
        assert_eq!(
            scan.files.len(),
            2,
            "the walk must stop at the candidate cap; got {:?}",
            scan.files
        );
        let full = visit_workspace_with_handwritten(&workspace.0, 10_000, &[]);
        assert!(!full.truncated);
        assert_eq!(full.files.len(), 5);
    }

    /// A huge asset tree still terminates through the total-entry backstop
    /// (5× the candidate limit) and reports the existing `truncated` flag.
    #[test]
    fn workspace_walk_total_entry_backstop_trips_on_huge_nonsource_tree() {
        let workspace = TempWorkspace::new("entry-backstop");
        let limit = 2usize;
        let backstop = limit.saturating_mul(TS_WORKSPACE_TOTAL_ENTRY_BACKSTOP_MULTIPLIER);
        for i in 0..(backstop + 5) {
            workspace.write(&format!("assets/blob-{i}.png"), "not source\n");
        }
        let scan = visit_workspace_with_handwritten(&workspace.0, limit, &[]);
        assert!(
            scan.truncated,
            "a huge non-source tree must trip the total-entry backstop"
        );
        assert!(
            scan.files.is_empty(),
            "no TS/JS sources exist to discover; got {:?}",
            scan.files
        );
        assert_eq!(
            LAST_WALK_TOTAL_ENTRIES.get(),
            backstop + 1,
            "the walk must stop at the 5× total-entry backstop, not after one entry"
        );
    }

    /// #11767 producer fixture: one source, twelve docs files, cap 3.
    /// Candidate counting would finish this tree; the 5× backstop plus
    /// sorted-name DFS still truncates after indexing the `.ts` file so
    /// `typescript_truncated_discovery` keeps withholding `no_static_path`.
    #[test]
    fn workspace_walk_tiny_cap_still_truncates_docs_tree_after_indexing_source() {
        let workspace = TempWorkspace::new("producer-docs");
        workspace.write("ripr.toml", "[languages]\nenabled = [\"typescript\"]\n");
        workspace.write("src/discount.ts", "export const n = 1;\n");
        for i in 0..12 {
            workspace.write(&format!("docs/page{i}.txt"), "unrelated\n");
        }
        workspace.write("diff.patch", "diff\n");
        let scan = visit_workspace_with_handwritten(&workspace.0, 3, &[]);
        assert!(
            scan.truncated,
            "cap 3 plus a dozen docs files must still trip the 5× entry backstop; files={:?} entries={}",
            scan.files,
            LAST_WALK_TOTAL_ENTRIES.get()
        );
        assert!(
            scan.files.iter().any(|path| path.ends_with("discount.ts")),
            "sorted DFS must index the source before docs fill the backstop; files={:?}",
            scan.files
        );
    }

    #[test]
    fn workspace_file_limit_env_parsing_matches_repo_conventions() {
        assert_eq!(
            ts_workspace_file_limit_from_env(Err(std::env::VarError::NotPresent)),
            Ok(DEFAULT_TS_MAX_WORKSPACE_FILES)
        );
        assert_eq!(
            ts_workspace_file_limit_from_env(Ok(" 42 ".to_string())),
            Ok(42)
        );
        assert!(
            ts_workspace_file_limit_from_env(Ok("0".to_string())).is_err(),
            "zero limit must be rejected"
        );
        assert!(
            ts_workspace_file_limit_from_env(Ok("nope".to_string())).is_err(),
            "non-numeric limit must be rejected"
        );
    }

    /// Symlink/junction entries are not followed (#4104-D) but are COUNTED so
    /// the adapter can disclose the skip instead of leaving link-hidden tests
    /// silently invisible. On Windows, `symlink_dir` requires developer-mode
    /// or admin privileges; when the platform refuses, the assertion is
    /// skipped (Linux CI covers the deterministic path).
    #[test]
    fn discovery_counts_skipped_links_without_following() {
        let workspace = TempWorkspace::new("link-skip");
        workspace.write(
            "src/sum.ts",
            "export function sum(a: number, b: number): number {\n  return a + b;\n}\n",
        );
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../src/sum';\n\ntest('adds', () => {\n  expect(sum(2, 3)).toBe(5);\n});\n",
        );
        // A real directory outside the root holding a linked test file...
        let outside = TempWorkspace::new("link-skip-outside");
        outside.write(
            "real_tests/linked.test.ts",
            "import { sum } from '../src/sum';\n\ntest('linked adds', () => {\n  expect(sum(4, 1)).toBe(5);\n});\n",
        );
        // ...and a link pointing at it.
        #[cfg(unix)]
        let linked = {
            use std::os::unix::fs::symlink;
            let target = outside.0.join("real_tests");
            let link = workspace.0.join("tests_linked");
            if symlink(&target, &link).is_err() {
                return; // platform refused; nothing to assert here
            }
            link
        };
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::symlink_dir;
            let target = outside.0.join("real_tests");
            let link = workspace.0.join("tests_linked");
            if symlink_dir(&target, &link).is_err() {
                return; // privilege missing; nothing to assert here
            }
            link
        };
        #[cfg(not(any(unix, windows)))]
        let linked: std::path::PathBuf = match () {
            _ => return,
        };

        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 1, "the one link must be counted");
        assert!(
            !scan
                .files
                .iter()
                .any(|file| file.starts_with("tests_linked")),
            "linked files must not be followed into the index: {:?}",
            scan.files
        );
        assert!(
            scan.files
                .iter()
                .any(|file| file.ends_with("tests/sum.test.ts")),
            "the real test file must still be indexed: {:?}",
            scan.files
        );
        let _ = linked;
    }

    /// A file symlink under a non-source name (a docs link such as h3's
    /// `docs/5.migration/0.index.md`) hides nothing the walk would index, so it
    /// is not disclosed; a source-named file link, a directory link and a
    /// dangling link all still are.
    #[cfg(unix)]
    #[test]
    fn discovery_does_not_count_non_source_file_links_but_counts_every_other_link()
    -> std::io::Result<()> {
        use std::os::unix::fs::symlink;
        let workspace = TempWorkspace::new("link-nonsource");
        workspace.write("src/sum.ts", "export const sum = 1;\n");
        workspace.write("README.md", "# docs\n");
        workspace.write("real/linked.test.ts", "test('x', () => {});\n");
        let root = &workspace.0;
        workspace.write("docs/guide.md", "# guide\n");
        symlink(root.join("README.md"), root.join("docs/index.md"))?;
        let scan = visit_workspace_with_handwritten(root, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(
            scan.skipped_links, 0,
            "a markdown file link hides no source and must not be disclosed"
        );

        symlink(
            root.join("README.md"),
            root.join("docs/bundle.generated.ts"),
        )?;
        let scan = visit_workspace_with_handwritten(root, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(
            scan.skipped_links, 0,
            "a generated-named file link is a name the walk never indexes"
        );

        symlink(
            root.join("real/linked.test.ts"),
            root.join("linked.test.ts"),
        )?;
        let scan = visit_workspace_with_handwritten(root, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 1, "a source-named file link is counted");

        symlink(root.join("real"), root.join("docs/real.md"))?;
        let scan = visit_workspace_with_handwritten(root, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(
            scan.skipped_links, 1,
            "a link to a walked in-root directory is an alias and adds no file"
        );

        let outside = TempWorkspace::new("link-nonsource-outside");
        outside.write("pkg/other.test.ts", "test('y', () => {});\n");
        symlink(outside.0.join("pkg"), root.join("docs/outside.md"))?;
        let scan = visit_workspace_with_handwritten(root, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(
            scan.skipped_links, 2,
            "a directory link leaving the root is counted even under a non-source name"
        );

        workspace.write("node_modules/pkg/index.test.ts", "test('z', () => {});\n");
        symlink(root.join("node_modules/pkg"), root.join("docs/vendored.md"))?;
        let scan = visit_workspace_with_handwritten(root, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(
            scan.skipped_links, 3,
            "a link into an excluded directory is not a walked alias and is counted"
        );

        symlink(root.join("missing.md"), root.join("docs/dangling.md"))?;
        let scan = visit_workspace_with_handwritten(root, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(
            scan.skipped_links, 4,
            "a dangling link cannot be shown harmless and is counted"
        );
        Ok(())
    }

    /// zod's agent-tooling alias `.tools/skills -> ../.agent/skills`
    /// (relative target, hidden directories) is a walked directory under a
    /// second name, so it is not a skipped link (#10829).
    #[cfg(unix)]
    #[test]
    fn discovery_does_not_count_a_relative_alias_of_a_walked_directory() -> std::io::Result<()> {
        use std::os::unix::fs::symlink;
        let workspace = TempWorkspace::new("link-alias");
        workspace.write("src/sum.ts", "export const sum = 1;\n");
        workspace.write(".agent/skills/triage/helper.mjs", "export const t = 1;\n");
        workspace.write(".tools/README.md", "# codex\n");
        symlink("../.agent/skills", workspace.0.join(".tools/skills"))?;
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 0, "{:?}", scan.files);
        assert!(
            scan.files.iter().any(|file| file.ends_with("src/sum.ts")),
            "the real source is still indexed: {:?}",
            scan.files
        );
        assert!(
            !scan
                .files
                .iter()
                .any(|file| file.starts_with(".tools/skills")),
            "the alias is not followed into the index: {:?}",
            scan.files
        );
        Ok(())
    }

    /// An alias an indexed file imports through (`../alias/sum`) names a path
    /// the walk never indexes, so it is counted; the same alias that nothing
    /// imports through is not (#10829 review).
    #[cfg(unix)]
    #[test]
    fn discovery_counts_an_alias_that_an_indexed_file_imports_through() -> std::io::Result<()> {
        use std::os::unix::fs::symlink;
        let workspace = TempWorkspace::new("link-alias-import");
        workspace.write("src/sum.ts", "export const sum = 1;\n");
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../alias/sum';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        symlink("src", workspace.0.join("alias"))?;
        assert_eq!(
            std::fs::canonicalize(workspace.0.join("alias"))?,
            std::fs::canonicalize(workspace.0.join("src"))?,
            "the alias must resolve to the real indexed directory, not dangle"
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert!(scan.files.iter().any(|file| file.ends_with("src/sum.ts")));
        assert_eq!(scan.skipped_links, 1, "{:?}", scan.files);

        // Control: the same real alias that nothing imports through is not
        // counted, so the count above comes from the import scan alone.
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../src/sum';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 0, "{:?}", scan.files);

        // A comment quote before the real import must not hide it.
        workspace.write(
            "tests/sum.test.ts",
            "/* \" */ import { sum } from \"../alias/sum\";\ntest('s', () => {});\n",
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 1, "{:?}", scan.files);
        Ok(())
    }

    /// A file that spells the alias's real path (`real/skills`) is not an
    /// import through `link/skills`; an import through the link still counts
    /// (a real repository's triage script mentions its own tooling path, #10899).
    #[cfg(unix)]
    #[test]
    fn discovery_ignores_the_real_spelling_of_an_alias_target() -> std::io::Result<()> {
        use std::os::unix::fs::symlink;
        let workspace = TempWorkspace::new("link-alias-real-spelling");
        workspace.write(
            "real/skills/run.ts",
            "// usage: run real/skills/run.ts\nexport const real = 'real/skills/run';\n",
        );
        std::fs::create_dir_all(workspace.0.join("link"))?;
        symlink("../real/skills", workspace.0.join("link/skills"))?;
        assert_eq!(
            std::fs::canonicalize(workspace.0.join("link/skills"))?,
            std::fs::canonicalize(workspace.0.join("real/skills"))?,
            "the alias must resolve to the real indexed directory, not dangle"
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert!(
            scan.files
                .iter()
                .any(|file| file.ends_with("real/skills/run.ts"))
        );
        assert_eq!(scan.skipped_links, 0, "{:?}", scan.files);

        // The same file also importing through the link is still counted.
        workspace.write(
            "real/skills/run.ts",
            "// real/skills/run.ts\nimport { x } from '../../link/skills/x';\n",
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 1, "{:?}", scan.files);
        Ok(())
    }

    /// The real-spelling exemption matches whole path components: a link
    /// under `unreal/` is not mistaken for a spelling under `real/`, in raw
    /// text or in a decoded literal (#10901 review).
    #[cfg(unix)]
    #[test]
    fn discovery_does_not_exempt_a_parent_that_only_ends_with_the_target_parent()
    -> std::io::Result<()> {
        use std::os::unix::fs::symlink;
        let workspace = TempWorkspace::new("link-alias-suffix-parent");
        workspace.write("real/skills/x.ts", "export const x = 1;\n");
        std::fs::create_dir_all(workspace.0.join("unreal"))?;
        symlink("../real/skills", workspace.0.join("unreal/skills"))?;
        assert_eq!(
            std::fs::canonicalize(workspace.0.join("unreal/skills"))?,
            std::fs::canonicalize(workspace.0.join("real/skills"))?,
            "the alias must resolve to the real indexed directory, not dangle"
        );
        for import in [
            "import { x } from '../unreal/skills/x';\n",
            "import { x } from '../unreal\\u002fskills/x';\n",
        ] {
            workspace.write("tests/x.test.ts", import);
            let scan =
                visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
            assert_eq!(scan.skipped_links, 1, "{import}: {:?}", scan.files);
        }

        // Control: a spelling through the real parent is still the real path.
        workspace.write("tests/x.test.ts", "import { x } from '../real/skills/x';\n");
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 0, "{:?}", scan.files);
        Ok(())
    }

    /// Which directory a relative specifier names is decided by resolving it,
    /// not by characters around the target's name: a link under `my.real`,
    /// `my:real`, `my real`, `my[real` or `a+real` is reached by its own
    /// spelling in raw text and in a decoded literal, while the real spelling
    /// and a bare or prose mention are not imports through the link
    /// (#10901 review).
    #[cfg(unix)]
    #[test]
    fn discovery_resolves_specifiers_instead_of_guessing_component_boundaries()
    -> std::io::Result<()> {
        use std::os::unix::fs::symlink;
        for (index, parent) in ["my.real", "my:real", "my real", "my[real", "a+real"]
            .into_iter()
            .enumerate()
        {
            let workspace = TempWorkspace::new(&format!("link-alias-resolve-{index}"));
            workspace.write("real/skills/x.ts", "export const x = 1;\n");
            std::fs::create_dir_all(workspace.0.join(parent))?;
            symlink("../real/skills", workspace.0.join(parent).join("skills"))?;
            assert_eq!(
                std::fs::canonicalize(workspace.0.join(parent).join("skills"))?,
                std::fs::canonicalize(workspace.0.join("real/skills"))?,
                "the alias must resolve to the real indexed directory, not dangle"
            );
            let escaped = parent.replace('/', "");
            for (import, expected) in [
                (format!("import {{ x }} from '../{parent}/skills/x';\n"), 1),
                (
                    format!("import {{ x }} from '../{escaped}\\u002fskills/x';\n"),
                    1,
                ),
                ("import { x } from '../real/skills/x';\n".to_string(), 0),
                (
                    "import { x } from '..\\u002freal/skills/x';\n".to_string(),
                    0,
                ),
                (
                    format!(
                        "// {parent}/skills is a tooling path\nexport const note = 'see {parent}/skills';\n"
                    ),
                    0,
                ),
            ] {
                workspace.write("tests/x.test.ts", &import);
                let scan = visit_workspace_with_handwritten(
                    &workspace.0,
                    DEFAULT_TS_MAX_WORKSPACE_FILES,
                    &[],
                );
                assert_eq!(
                    scan.skipped_links, expected,
                    "{parent}: {import}: {:?}",
                    scan.files
                );
            }
        }
        Ok(())
    }

    /// The alias name is not limited to a restricted character set: `a$b`
    /// imported as `../a$b/sum` is still matched (#10829 review).
    #[cfg(unix)]
    #[test]
    fn discovery_counts_an_alias_whose_name_has_shell_characters() -> std::io::Result<()> {
        use std::os::unix::fs::symlink;
        let workspace = TempWorkspace::new("link-alias-dollar");
        workspace.write("src/sum.ts", "export const sum = 1;\n");
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../a$b/sum';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        symlink("src", workspace.0.join("a$b"))?;
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 1, "{:?}", scan.files);
        Ok(())
    }

    /// A space in the alias name is ordinary import spelling: `'../my lib/sum'`
    /// is matched, and the same alias with no importer is not counted.
    #[cfg(unix)]
    #[test]
    fn discovery_counts_an_alias_whose_name_has_a_space_only_when_imported() -> std::io::Result<()>
    {
        use std::os::unix::fs::symlink;
        let workspace = TempWorkspace::new("link-alias-space");
        workspace.write("src/sum.ts", "export const sum = 1;\n");
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../src/sum';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        symlink("src", workspace.0.join("my lib"))?;
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 0, "{:?}", scan.files);
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../my lib/sum';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 1, "{:?}", scan.files);
        Ok(())
    }

    /// An escaped spelling (`'../al\u0069as/lib'`) decodes to the alias name
    /// without the raw text containing it, so it is counted; an unrelated
    /// escape (`\n`) beside a direct import is not.
    #[cfg(unix)]
    #[test]
    fn discovery_counts_an_alias_named_through_a_character_escape() -> std::io::Result<()> {
        use std::os::unix::fs::symlink;
        let workspace = TempWorkspace::new("link-alias-escape");
        workspace.write("src/sum.ts", "export const sum = 1;\n");
        symlink("src", workspace.0.join("alias"))?;
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../src/sum';\nconst note = 'a\\nb';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 0, "{:?}", scan.files);
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../al\\u0069as/sum';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 1, "{:?}", scan.files);

        // An escaped separator leaves neither `./` nor `/alias` in the raw
        // text; the decoded literal still names the link.
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '..\\u002falias/sum';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 1, "{:?}", scan.files);

        // Escapes beside an unrelated, cleanly parsed import do not count
        // (zod's unicode-heavy tests must keep the tooling alias exempt).
        workspace.write(
            "tests/sum.test.ts",
            "import { sum } from '../src/sum';\nconst snow = '\\u2603';\ntest('s', () => { expect(sum).toBe(1); });\n",
        );
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(scan.skipped_links, 0, "{:?}", scan.files);
        Ok(())
    }

    /// A directory the walk cannot list hides its sources, so a link aliasing
    /// it stays counted (#10829). Skipped when the process can read it anyway
    /// (running as root).
    #[cfg(unix)]
    #[test]
    fn discovery_counts_an_alias_of_a_directory_it_cannot_list() -> std::io::Result<()> {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let workspace = TempWorkspace::new("link-unlistable");
        workspace.write("src/keep.ts", "export const keep = 1;\n");
        workspace.write("private/lost.test.ts", "test('x', () => {});\n");
        let private = workspace.0.join("private");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o300))?;
        symlink(&private, workspace.0.join("alias.md"))?;
        let unlistable = fs::read_dir(&private).is_err();
        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
        if unlistable {
            assert_eq!(scan.skipped_links, 1, "{:?}", scan.files);
        }
        Ok(())
    }

    /// A Unix socket is neither dir, file, nor link, so before the
    /// special-file exclusion it incremented `skipped_links` — a workspace
    /// with no link at all would have disclosed a link limitation
    /// (review #4138). The socket must be excluded without counting, and
    /// real sources stay indexed.
    #[cfg(unix)]
    #[test]
    fn discovery_does_not_count_sockets_as_skipped_links() {
        let workspace = TempWorkspace::new("socket-skip");
        workspace.write(
            "src/sum.ts",
            "export function sum(a: number, b: number): number {\n  return a + b;\n}\n",
        );
        let socket_path = workspace.0.join("agent.sock");
        let listener = match std::os::unix::net::UnixListener::bind(&socket_path) {
            Ok(listener) => listener,
            Err(_) => return, // platform refused; nothing to assert here
        };

        let scan =
            visit_workspace_with_handwritten(&workspace.0, DEFAULT_TS_MAX_WORKSPACE_FILES, &[]);
        assert_eq!(
            scan.skipped_links, 0,
            "a socket is not a link and must not disclose a link limitation"
        );
        assert!(
            scan.files.iter().any(|file| file.ends_with("src/sum.ts")),
            "the real source must still be indexed: {:?}",
            scan.files
        );
        drop(listener);
    }

    #[test]
    fn workspace_walk_skips_excluded_and_generated_files_but_keeps_near_misses() {
        let workspace = TempWorkspace::new("excluded-near-miss");
        for rel in [
            "src/ok.ts",
            "src/generated.ts",
            "src/regenerated.js",
            "src/build.ts",
            "node_modules/left-pad/index.js",
            "dist/bundle.js",
            "build/out.ts",
            "out/tsc.ts",
            "coverage/lcov.js",
            ".next/server.ts",
            ".cache/sw.js",
            "vendor/lib.ts",
            "src/__generated__/types.ts",
            "src/cart.generated.ts",
            "src/types.generated.d.ts",
        ] {
            workspace.write(rel, "export const value = 1;\n");
        }
        let scan = visit_workspace_with_handwritten(&workspace.0, 10_000, &[]);
        assert!(!scan.truncated, "scan must not trip the cap");
        for expected in [
            "src/ok.ts",
            "src/generated.ts",
            "src/regenerated.js",
            "src/build.ts",
        ] {
            assert!(
                scan.files.iter().any(|path| path == Path::new(expected)),
                "expected near-miss {expected} in {:?}",
                scan.files
            );
        }
        for excluded in [
            "node_modules/left-pad/index.js",
            "dist/bundle.js",
            "build/out.ts",
            "out/tsc.ts",
            "coverage/lcov.js",
            ".next/server.ts",
            ".cache/sw.js",
            "vendor/lib.ts",
            "src/__generated__/types.ts",
            "src/cart.generated.ts",
            "src/types.generated.d.ts",
        ] {
            assert!(
                !scan.files.iter().any(|path| path == Path::new(excluded)),
                "did not expect {excluded} in {:?}",
                scan.files
            );
        }
    }
}
