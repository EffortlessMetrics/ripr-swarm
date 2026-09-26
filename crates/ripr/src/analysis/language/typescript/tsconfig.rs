//! tsconfig.json / jsconfig.json alias-map loader (RIPR-SPEC-0099).
//!
//! This module is SINGLE-HOP and FAIL-CLOSED:
//!
//! - Only `compilerOptions.baseUrl` and `compilerOptions.paths` are read.
//! - `extends` and `references` are NOT followed.
//! - Resolution succeeds ONLY when a specifier matches a SINGLE existing
//!   workspace file (.ts/.tsx/.js/.jsx/.mts/.cts/.mjs/.cjs).
//!   Zero or >1 matches → `None`.
//! - Exact keys win; otherwise the longest matching prefix before `*` wins.
//! - Tied longest prefixes and unsupported winning templates → `None`.
//! - Multi-entry value arrays (more than one candidate template) → `None`.
//! - Multi-`*` glob patterns → `None`.
//! - Any parse error or missing field → `None`.
//! - Candidates or `baseUrl` containing `..` segments, rooted components,
//!   or drive/UNC prefixes → `None` BEFORE any filesystem probe; an
//!   absolute `baseUrl` additionally surfaces the named limitation
//!   `typescript_base_url_absolute_unsupported`.
//!
//! The alias map is built once per analysis run and reused for every
//! `normalized_relative_import_module` call that encounters a non-relative
//! specifier.  Building it is opt-in: `AnalysisOptions::resolve_tsconfig_paths`
//! must be `true`; otherwise `TsAliasMap::empty()` (no-op) is returned.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::bounded_read::read_config_capped;

// ── Wire types for parsing ────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTsConfig {
    compiler_options: Option<RawCompilerOptions>,
    // If `extends` or `references` are present we MUST NOT follow them;
    // we detect their presence and bail out (fail-closed).
    extends: Option<serde_json::Value>,
    references: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCompilerOptions {
    base_url: Option<String>,
    paths: Option<HashMap<String, Vec<String>>>,
}

// ── Public types ──────────────────────────────────────────────────────────────

/// Compiled alias map derived from `tsconfig.json` / `jsconfig.json`.
///
/// Retains literal and single-`*` keys even when their templates are unsupported.
/// Selection precedes file lookup: an unsupported winning key must block a
/// broader match rather than lend its import to a different source file.
#[derive(Debug, Default, Clone)]
pub(crate) struct TsAliasMap {
    /// Workspace root so that `resolve` can check file existence.
    root: PathBuf,
    /// `baseUrl`, relative to `root` (often `.`).
    base_url: String,
    /// `true` when `baseUrl` was absolute or contained non-normal path
    /// components: single-hop resolution cannot anchor it to `root`, so
    /// every lookup fails closed instead of silently trimming the leading
    /// slash into a wrong in-root path. Callers surface this as the named
    /// limitation `typescript_base_url_absolute_unsupported`.
    base_url_absolute: bool,
    /// Literal entries: key → supported template, or `None` to block resolution.
    literal_entries: HashMap<String, Option<String>>,
    /// Glob entries: (prefix, suffix) → supported template or blocker.
    ///
    /// The template may itself contain a `*`; the captured group from the
    /// specifier replaces that `*` in the template.
    glob_entries: Vec<GlobEntry>,
}

#[derive(Debug, Clone)]
struct GlobEntry {
    /// Part of the pattern key before the `*` (may be empty).
    prefix: String,
    /// Part of the pattern key after the `*` (may be empty).
    suffix: String,
    /// The single supported template; `None` retains an unsupported key.
    template: Option<String>,
}

/// Typed fail-closed cause for a specifier the alias map did not resolve.
/// Drives the `typescript_path_alias_unresolved` advice text so it names
/// the actual reason (unknown baseUrl vs unmatched pattern vs
/// out-of-root/unresolved candidate) instead of a generic message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TsAliasUnresolveCause {
    /// No alias map: opt-out flag, missing config, or unparseable config.
    MapUnavailable,
    /// `baseUrl` is absolute or non-normal; resolution deliberately
    /// fail-closes (`typescript_base_url_absolute_unsupported`).
    BaseUrlAbsolute,
    /// The map parsed but has no `paths` entries to match against.
    NoPatterns,
    /// No literal or single-`*` key matches the specifier.
    PatternUnmatched,
    /// A key owned the specifier, but its candidate did not resolve to
    /// exactly one in-root workspace file (zero files, >1 files, an
    /// unsupported template, or a `..`/absolute candidate rejected before
    /// probing).
    CandidateUnresolved,
}

impl TsAliasUnresolveCause {
    /// Typed cause phrase for the limitation's `why_not_actionable` text,
    /// paired with a cause-specific recovery hint.
    pub(crate) fn parts(self) -> (&'static str, &'static str) {
        match self {
            Self::MapUnavailable => (
                "no tsconfig.json/jsconfig.json alias map was available (opt-out flag, missing config, or unparseable config)",
                "enable `[typescript] resolve_tsconfig_paths = true` with a parseable config for credit",
            ),
            Self::BaseUrlAbsolute => (
                "compilerOptions.baseUrl is absolute or non-normal, so single-hop resolution fails closed rather than guessing an in-root anchor",
                "change compilerOptions.baseUrl to a workspace-relative path, then re-run the analysis",
            ),
            Self::NoPatterns => (
                "the alias map has no compilerOptions.paths entries to match this specifier",
                "add a compilerOptions.paths entry naming this specifier for credit",
            ),
            Self::PatternUnmatched => (
                "no compilerOptions.paths key (literal or single-`*`) matches this specifier",
                "add a compilerOptions.paths key matching this specifier for credit",
            ),
            Self::CandidateUnresolved => (
                "the matched pattern's candidate did not resolve to exactly one in-root workspace file (zero or multiple files, an unsupported template, or an out-of-root candidate rejected before probing)",
                "point the matched template at exactly one existing workspace file for credit",
            ),
        }
    }
}

impl TsAliasMap {
    /// `true` when this map has no entries (opt-out / parse-failure path).
    pub(crate) fn is_empty(&self) -> bool {
        self.literal_entries.is_empty() && self.glob_entries.is_empty()
    }

    /// `true` when `baseUrl` was absolute (or contained non-normal path
    /// components) and alias resolution therefore fail-closes. The caller
    /// surfaces this as the named limitation
    /// `typescript_base_url_absolute_unsupported`.
    pub(crate) fn base_url_absolute(&self) -> bool {
        self.base_url_absolute
    }

    /// The typed fail-closed cause for a specifier that did not resolve,
    /// so the `typescript_path_alias_unresolved` advice can name the real
    /// reason instead of a generic message.
    pub(crate) fn unresolve_cause_for(&self, specifier: &str) -> TsAliasUnresolveCause {
        if self.base_url_absolute {
            return TsAliasUnresolveCause::BaseUrlAbsolute;
        }
        if self.is_empty() {
            return TsAliasUnresolveCause::NoPatterns;
        }
        if self.literal_entries.contains_key(specifier)
            || self
                .glob_entries
                .iter()
                .any(|entry| match_glob(specifier, &entry.prefix, &entry.suffix).is_some())
        {
            // A key owned this specifier; the candidate itself failed.
            return TsAliasUnresolveCause::CandidateUnresolved;
        }
        TsAliasUnresolveCause::PatternUnmatched
    }

    /// Resolve a non-relative specifier to a canonical workspace-relative path.
    ///
    /// Returns `None` (fail-closed) unless ALL of the following hold:
    /// 1. `specifier` is non-relative (does not start with `./` or `../`).
    /// 2. An exact key or a unique longest-prefix single-`*` key matches.
    /// 3. The matched value array has exactly one entry.
    /// 4. The value template has at most one `*`.
    /// 5. After substituting the captured `*`, the candidate path resolves to
    ///    EXACTLY ONE existing workspace file (.ts/.tsx/.js/.jsx/.mts/.cts/
    ///    .mjs/.cjs).
    pub(crate) fn resolve(&self, specifier: &str) -> Option<PathBuf> {
        if specifier.starts_with("./") || specifier.starts_with("../") {
            return None; // relative paths are handled by the normal resolver
        }
        if self.is_empty() {
            return None;
        }

        // 1. Try literal match first.
        if let Some(template) = self.literal_entries.get(specifier) {
            let candidate_str = strip_ts_ext(template.as_deref()?);
            return self.unique_file_for(&candidate_str);
        }

        // 2. Select by TypeScript's longest-prefix rule, not HashMap order or
        // whichever target exists. Equal-prefix ties depend on declaration
        // order, which this map does not retain, so those stay unresolved.
        let mut best: Option<(&GlobEntry, String)> = None;
        let mut ambiguous = false;
        for entry in &self.glob_entries {
            let Some(captured) = match_glob(specifier, &entry.prefix, &entry.suffix) else {
                continue;
            };
            match best.as_ref() {
                Some((current, _)) if current.prefix.len() > entry.prefix.len() => {}
                Some((current, _)) if current.prefix.len() == entry.prefix.len() => {
                    ambiguous = true;
                }
                _ => {
                    best = Some((entry, captured));
                    ambiguous = false;
                }
            }
        }
        if ambiguous {
            return None;
        }
        let (entry, captured) = best?;
        let expanded = entry.template.as_deref()?.replace('*', &captured);
        self.unique_file_for(&strip_ts_ext(&expanded))
    }

    /// Given a base candidate string (extension-stripped, slash-separated),
    /// try each TS extension and collect the unique matching file.
    ///
    /// Returns `None` if zero files or more than one file match.
    ///
    /// Candidates containing `..` segments, rooted components, or prefixes
    /// (drive letters / UNC) are rejected BEFORE any filesystem probe: they
    /// would make `is_file()` read outside the workspace root, and a
    /// component-wise `strip_prefix` credit could still escape the root.
    fn unique_file_for(&self, candidate_base: &str) -> Option<PathBuf> {
        if self.base_url_absolute || !is_safe_relative(Path::new(candidate_base)) {
            return None;
        }
        // The workspace root itself is trusted; only the configured baseUrl
        // segment must stay relative and free of `..` components.
        if !is_safe_relative(Path::new(&self.base_url)) {
            return None;
        }
        let base_dir = self.root.join(self.base_url.trim_matches('/'));
        let extensions = [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"];
        let mut found: Vec<PathBuf> = Vec::new();
        for ext in &extensions {
            let candidate = base_dir.join(format!("{candidate_base}{ext}"));
            if candidate.is_file() {
                // Normalize to forward-slash workspace-relative path.
                if let Ok(rel) = candidate.strip_prefix(&self.root) {
                    found.push(rel.to_path_buf());
                }
            }
        }
        if found.len() == 1 {
            Some(found.remove(0))
        } else {
            None // zero or ambiguous — fail-closed
        }
    }
}

// ── Loader ────────────────────────────────────────────────────────────────────

/// Load and compile an alias map from `root/tsconfig.json` then
/// `root/jsconfig.json`.
///
/// Returns `None` (fail-closed) on any of:
/// - Neither file exists.
/// - JSON parse error.
/// - `compilerOptions` absent.
/// - `baseUrl` absent.
/// - `extends` or `references` present (single-hop only — do NOT follow).
///
/// Test-only convenience over [`load_alias_map_with_read_error`] for the
/// existing fixture callers; production surfaces read outcomes through the
/// `_with_read_error` variant.
#[cfg(test)]
pub(crate) fn load_alias_map(root: &Path) -> Option<TsAliasMap> {
    load_alias_map_with_read_error(root).0
}

/// Like [`load_alias_map`], but also reports the config path and read error
/// when reading the first existing config file fails.
///
/// The error is preserved so `analyze_diff` can surface size-limit outcomes
/// (`OverFileLimit` / `OverWorkspaceBudget`) as named limitations instead of
/// failing silently closed. Plain IO failures stay in the second slot too;
/// disclosure for those is owned by the read-error lane, which filters on
/// `CappedReadError::is_size_limit`.
pub(crate) fn load_alias_map_with_read_error(
    root: &Path,
) -> (
    Option<TsAliasMap>,
    Option<(PathBuf, super::bounded_read::CappedReadError)>,
) {
    for filename in &["tsconfig.json", "jsconfig.json"] {
        let path = root.join(filename);
        if !path.is_file() {
            continue;
        }
        // Capped read: a read failure fail-closes the alias map; size-limit
        // outcomes are disclosed by the caller through the second slot.
        return match read_config_capped(&path) {
            Ok(text) => (parse_alias_map(root, &text), None),
            Err(err) => (None, Some((path, err))),
        };
    }
    (None, None)
}

fn parse_alias_map(root: &Path, text: &str) -> Option<TsAliasMap> {
    let raw: RawTsConfig = serde_json::from_str(text).ok()?;

    // Fail-closed: if extends/references are present, do NOT follow them.
    if raw.extends.is_some() || raw.references.is_some() {
        return None;
    }

    let compiler_opts = raw.compiler_options?;
    let base_url = compiler_opts.base_url?;
    let paths = compiler_opts.paths.unwrap_or_default();

    // An absolute baseUrl (POSIX `/…`, drive-letter, or UNC) cannot be
    // anchored to `root` by single-hop resolution. Keep the map so the
    // caller can surface the named limitation, but flag every lookup to
    // fail closed instead of silently trimming the leading slash into a
    // wrong in-root path.
    let base_url_absolute = !is_safe_relative(Path::new(&base_url));

    let mut literal_entries: HashMap<String, Option<String>> = HashMap::new();
    let mut glob_entries: Vec<GlobEntry> = Vec::new();

    for (key, values) in &paths {
        // Retain unsupported keys as blockers. Dropping a more-specific key
        // would let a broader alias claim an import that it does not own.
        let template = match values.as_slice() {
            [template] if template.chars().filter(|&c| c == '*').count() <= 1 => {
                Some(template.clone())
            }
            _ => None,
        };

        // Determine if key is literal or single-`*` glob.
        let star_count = key.chars().filter(|&c| c == '*').count();
        if star_count == 0 {
            // Literal key.
            literal_entries.insert(key.clone(), template.clone());
        } else if star_count == 1 {
            // Single-`*` glob: split into prefix/suffix.
            if let Some(star_pos) = key.find('*') {
                glob_entries.push(GlobEntry {
                    prefix: key[..star_pos].to_string(),
                    suffix: key[star_pos + 1..].to_string(),
                    template: template.clone(),
                });
            }
        }
        // Multi-`*` keys → silently skipped (fail-closed).
    }

    Some(TsAliasMap {
        root: root.to_path_buf(),
        base_url,
        base_url_absolute,
        literal_entries,
        glob_entries,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// `true` only when every component of `p` is a normal name or `.` — no
/// `..`, no rooted component, no drive letter / UNC prefix. Joining such a
/// path onto the trusted workspace root cannot escape it.
fn is_safe_relative(p: &Path) -> bool {
    use std::path::Component;
    p.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Strip the file extension from a path string, preserving the rest.
fn strip_ts_ext(s: &str) -> String {
    for ext in &[".tsx", ".mts", ".cts", ".ts", ".jsx", ".mjs", ".cjs", ".js"] {
        if let Some(stripped) = s.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    s.to_string()
}

/// Try to match `specifier` against a single-`*` pattern given its
/// `prefix` and `suffix`.  Returns the captured group on success.
fn match_glob(specifier: &str, prefix: &str, suffix: &str) -> Option<String> {
    let after_prefix = specifier.strip_prefix(prefix)?;
    if suffix.is_empty() {
        return Some(after_prefix.to_string());
    }
    let captured = after_prefix.strip_suffix(suffix)?;
    Some(captured.to_string())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod precedence_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("ripr-tsconfig-{label}-{stamp}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn write(dir: &Path, name: &str, content: &str) {
        if let Some(parent) = PathBuf::from(name).parent() {
            let _ = fs::create_dir_all(dir.join(parent));
        }
        let _ = fs::write(dir.join(name), content);
    }

    // ── load_alias_map ────────────────────────────────────────────────────

    #[test]
    fn returns_none_when_no_config_file_present() {
        let root = temp_dir("no-config");
        assert!(load_alias_map(&root).is_none());
    }

    #[test]
    fn returns_none_when_extends_present() {
        let root = temp_dir("extends");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./base","compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        assert!(load_alias_map(&root).is_none());
    }

    #[test]
    fn malformed_tsconfig_returns_none_and_blocks_jsconfig_fallback() {
        // A malformed tsconfig.json fails closed: `load_alias_map` returns
        // None and must NOT fall through to a well-formed jsconfig.json —
        // silently honoring a different config than the one the project
        // declares would manufacture alias evidence.
        let root = temp_dir("malformed-tsconfig");
        write(&root, "tsconfig.json", "{ not valid json");
        write(
            &root,
            "jsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        assert!(
            load_alias_map(&root).is_none(),
            "malformed tsconfig.json must fail closed and block the jsconfig.json fallback"
        );
    }

    #[test]
    fn returns_none_when_references_present() {
        let root = temp_dir("refs");
        write(
            &root,
            "tsconfig.json",
            r#"{"references":[{"path":"./pkg"}],"compilerOptions":{"baseUrl":".","paths":{}}}"#,
        );
        assert!(load_alias_map(&root).is_none());
    }

    #[test]
    fn returns_none_when_base_url_absent() {
        let root = temp_dir("no-base-url");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"paths":{"@/*":["src/*"]}}}"#,
        );
        assert!(load_alias_map(&root).is_none());
    }

    #[test]
    fn single_star_glob_resolves_to_unique_file() -> Result<(), String> {
        let root = temp_dir("single-star");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "src/owner.ts", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        let resolved = map.resolve("@/owner").ok_or("should resolve")?;
        // Normalize separators for assertion
        let resolved_str = resolved.to_string_lossy().replace('\\', "/");
        assert_eq!(resolved_str, "src/owner.ts");
        Ok(())
    }

    #[test]
    fn multi_entry_value_fails_closed() -> Result<(), String> {
        let root = temp_dir("multi-entry");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*","lib/*"]}}}"#,
        );
        write(&root, "src/owner.ts", "export function owner() {}");
        // Even if one path would resolve, multi-entry value → None
        let map = load_alias_map(&root).ok_or("should parse")?;
        // The key is retained as a blocker, so resolution returns None
        assert!(map.resolve("@/owner").is_none());
        Ok(())
    }

    #[test]
    fn ambiguous_two_files_fails_closed() -> Result<(), String> {
        let root = temp_dir("ambiguous");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "src/owner.ts", "export function owner() {}");
        write(&root, "src/owner.tsx", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        // Two matching extensions → fail-closed
        assert!(map.resolve("@/owner").is_none());
        Ok(())
    }

    /// A lone modern-suffix file must resolve through the alias, and the
    /// resolved path must be that exact file. Pinned literally so removing a
    /// suffix from `unique_file_for` fails the assertion.
    #[test]
    fn modern_suffixes_resolve_to_their_unique_file() -> Result<(), String> {
        for (label, relative) in [
            ("mts", "src/owner.mts"),
            ("cts", "src/owner.cts"),
            ("mjs", "src/owner.mjs"),
            ("cjs", "src/owner.cjs"),
        ] {
            let root = temp_dir(label);
            write(
                &root,
                "tsconfig.json",
                r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
            );
            write(&root, relative, "export function owner() {}");

            let map = load_alias_map(&root).ok_or("should parse")?;
            let resolved = map
                .resolve("@/owner")
                .ok_or_else(|| format!("{label} alias must resolve"))?;
            assert_eq!(
                resolved.to_string_lossy().replace('\\', "/"),
                relative,
                "{label} alias must resolve to the exact workspace file"
            );
        }
        Ok(())
    }

    /// Two-match negative: when BOTH modern suffixes exist for one alias
    /// base, resolution must fail closed rather than picking a suffix by
    /// list order. This is the "always-suffix" wrong behavior this control
    /// rejects — an implementation that always returns the first candidate
    /// would resolve instead of returning `None`.
    #[test]
    fn two_modern_suffix_matches_fail_closed() -> Result<(), String> {
        for (label, first, second) in [
            ("mts-cts", "src/owner.mts", "src/owner.cts"),
            ("mjs-cjs", "src/owner.mjs", "src/owner.cjs"),
            ("ts-mts", "src/owner.ts", "src/owner.mts"),
        ] {
            let root = temp_dir(label);
            write(
                &root,
                "tsconfig.json",
                r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
            );
            write(&root, first, "export function owner() {}");
            write(&root, second, "export function owner() {}");

            let map = load_alias_map(&root).ok_or("should parse")?;
            assert!(
                map.resolve("@/owner").is_none(),
                "{first} and {second} both match @/owner; resolution must fail closed"
            );
        }
        Ok(())
    }

    /// A near-miss suffix is not an alias candidate: only `owner.mts` exists,
    /// so the alias resolves to it. A `.mtsx` sibling must NOT create a
    /// second match and must NOT be substituted for `.mts`.
    #[test]
    fn near_miss_suffix_is_not_an_alias_candidate() -> Result<(), String> {
        let root = temp_dir("near-miss");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "src/owner.mts", "export function owner() {}");
        write(&root, "src/owner.mtsx", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        let resolved = map.resolve("@/owner").ok_or("should resolve")?;
        assert_eq!(
            resolved.to_string_lossy().replace('\\', "/"),
            "src/owner.mts",
            "a .mtsx near-miss must not be an alias candidate or resolve the alias"
        );
        Ok(())
    }

    #[test]
    fn no_matching_file_returns_none() -> Result<(), String> {
        let root = temp_dir("no-file");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );

        let map = load_alias_map(&root).ok_or("should parse")?;
        assert!(map.resolve("@/nonexistent").is_none());
        Ok(())
    }

    #[test]
    fn relative_specifier_returns_none() -> Result<(), String> {
        let root = temp_dir("relative");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{}}}"#,
        );
        let map = load_alias_map(&root).ok_or("should parse")?;
        assert!(map.resolve("./owner").is_none());
        assert!(map.resolve("../owner").is_none());
        Ok(())
    }

    #[test]
    fn empty_map_returns_none() {
        let map = TsAliasMap::default();
        assert!(map.resolve("@/owner").is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn jsconfig_json_used_as_fallback() -> Result<(), String> {
        let root = temp_dir("jsconfig");
        write(
            &root,
            "jsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "src/owner.ts", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse jsconfig.json")?;
        assert!(map.resolve("@/owner").is_some());
        Ok(())
    }

    #[test]
    fn tsconfig_json_takes_priority_over_jsconfig_json() -> Result<(), String> {
        let root = temp_dir("priority");
        // tsconfig has empty paths; jsconfig has the alias
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{}}}"#,
        );
        write(
            &root,
            "jsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "src/owner.ts", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse tsconfig.json")?;
        // tsconfig takes priority and has no @/* entry → None
        assert!(map.resolve("@/owner").is_none());
        Ok(())
    }

    #[test]
    fn parent_dir_base_url_escapes_root_and_fails_closed() -> Result<(), String> {
        let root = temp_dir("dotdot-base-url");
        // A `..` baseUrl reaches a sibling directory of the workspace root.
        // Before hardening, the in-root-existence check credited the file
        // via a component-wise `strip_prefix`; it must now fail closed.
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":"..","paths":{"@/*":["sibling/*"]}}}"#,
        );
        write(&root, "../sibling/owner.ts", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        assert!(
            map.resolve("@/owner").is_none(),
            "a `..` baseUrl must not credit files outside the workspace root"
        );
        Ok(())
    }

    #[test]
    fn parent_dir_specifier_capture_fails_closed() -> Result<(), String> {
        let root = temp_dir("dotdot-capture");
        // A captured `../` group from the specifier must not steer the
        // candidate outside the workspace root (or smuggle an in-root file
        // through a traversal segment).
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "src/lib.ts", "export function lib() {}");
        write(&root, "owner.ts", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        assert!(
            map.resolve("@/../owner").is_none(),
            "a captured `..` group must fail closed, not credit a traversal path"
        );
        Ok(())
    }

    #[test]
    fn absolute_base_url_fails_closed_with_named_cause() -> Result<(), String> {
        let root = temp_dir("absolute-base-url");
        // The trimmed absolute baseUrl would collide with a REAL in-root
        // directory on some platforms (`abs/base` below); resolution must
        // not silently credit it.
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":"/abs/base","paths":{"@/*":["*"]}}}"#,
        );
        write(&root, "abs/base/owner.ts", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        assert!(
            map.base_url_absolute(),
            "an absolute baseUrl must be flagged for the named limitation"
        );
        assert!(
            map.resolve("@/owner").is_none(),
            "an absolute baseUrl must fail closed instead of trimming into a wrong in-root path"
        );
        Ok(())
    }

    #[test]
    fn relative_base_url_unaffected_by_hardening() -> Result<(), String> {
        let root = temp_dir("relative-base-url-ok");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["*"]}}}"#,
        );
        write(&root, "src/owner.ts", "export function owner() {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        assert!(!map.base_url_absolute());
        let resolved = map
            .resolve("@/owner")
            .ok_or("relative baseUrl must still resolve")?;
        let resolved_str = resolved.to_string_lossy().replace('\\', "/");
        assert_eq!(resolved_str, "src/owner.ts");
        Ok(())
    }
}
