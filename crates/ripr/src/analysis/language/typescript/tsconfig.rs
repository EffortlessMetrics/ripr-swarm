//! tsconfig.json / jsconfig.json alias-map loader (RIPR-SPEC-0099).
//!
//! This module is FAIL-CLOSED with a bounded `extends` chain:
//!
//! - Only `compilerOptions.baseUrl` and `compilerOptions.paths` are read,
//!   plus `module` / `moduleResolution` to decide whether a directory
//!   target may load its `index` file (#15914).
//! - The ROOT map reads only `<workspace-root>/tsconfig.json` and
//!   `<workspace-root>/jsconfig.json`. An importer whose nearest
//!   `tsconfig.json`/`jsconfig.json` (bounded upward walk) is not the root
//!   config and owns `compilerOptions.paths` (declares them, inherits them
//!   through a relative `extends` chain, or cannot be read/parsed)
//!   resolves through THAT config's own map ([`TsAliasMap::nested_for`],
//!   `baseUrl` anchored at the config's directory), never the root map
//!   (#12754). It fails closed (unresolved) when that config uses an
//!   unsupported `extends`/`references` form, or when `tsconfig.json` and
//!   `jsconfig.json` in one directory both own paths. Root fallback is
//!   unchanged when no nearer config owns `paths`.
//! - Without an effective `baseUrl`, `paths` templates resolve against the
//!   directory of the config that DECLARES `paths`, as `tsc` does since 4.1
//!   (#12760). `tsc` rejects a non-relative template there (TS5090), so a
//!   template that does not start with `./` or `../` keeps its key as an
//!   unresolved blocker instead of being guessed against some other anchor.
//! - `extends` chains are followed up to 8 hops when every target is a
//!   relative (`./`, `../`) regular file inside the workspace root; an
//!   inherited `baseUrl` stays anchored at the config that declares it, as
//!   `tsc` does (#11204); an explicit `null` clears an inherited value.
//!   Package-name `extends`, bases outside the root or behind
//!   a symlinked file or directory, cycles, and missing targets fail closed,
//!   except that an unfollowable base is ignored when the extending config
//!   declares a non-null `baseUrl` and `paths` itself (a cycle never is). A
//!   config that declares `paths` without `baseUrl` is not self-sufficient:
//!   the base it cannot read could supply the `baseUrl` that anchors them.
//! - `references` on the selected config are read one level deep. `tsc`
//!   compiles each referenced project with its own options, so a referenced
//!   config in the SAME directory (the Vite `tsconfig.app.json` /
//!   `tsconfig.node.json` layout) is compiled as a sibling project with its
//!   own map. Which project owns an importer (`include`/`files`) is not
//!   modeled; instead a specifier resolves only when every project whose
//!   `paths` claim it resolve it to the same file, and no project without a
//!   claim would resolve it as a workspace package. A solution-style config
//!   (`"files": []`, no `include`, no `extends`) compiles no file itself, so
//!   its own `paths` are not a project. A referenced config in another
//!   directory governs its own importers through the nearest-config walk
//!   above and is not consulted here. A reference that is not a regular
//!   in-root file, a malformed `references` value, or a sibling project that
//!   cannot be compiled or has `references` of its own fails closed.
//!   `references` on an `extends` base are ignored, as `tsc` never inherits
//!   them.
//! - Resolution succeeds ONLY when a specifier matches a SINGLE existing
//!   workspace file (.ts/.tsx/.js/.jsx/.mts/.cts/.mjs/.cjs).
//!   Zero or >1 matches → `None`.
//! - Exact keys win; otherwise the longest matching prefix before `*` wins.
//! - Tied longest prefixes and unsupported winning templates → `None`.
//! - A value array is a fallback list, tried in order as `tsc` does: the
//!   first template whose candidate is exactly one workspace file wins. A
//!   later template is reached only when every earlier candidate is PROVABLY
//!   absent (no file with that stem under any extension and no directory of
//!   that name, so nothing `tsc` could resolve there). An earlier candidate
//!   that is ambiguous, unsupported, unsafe, or present in a form this map
//!   does not model → `None`, never a later entry. An empty array → `None`.
//! - Multi-`*` glob patterns → `None`.
//! - Any parse error, or a chain with neither `baseUrl` nor `paths` → `None`.
//! - Candidates or `baseUrl` containing `..` segments, rooted components,
//!   or drive/UNC prefixes → `None` BEFORE any filesystem probe; an
//!   absolute `baseUrl` additionally surfaces the named limitation
//!   `typescript_base_url_absolute_unsupported`.
//!
//! The alias map is built once per analysis run and reused for every
//! `normalized_relative_import_module` call that encounters a non-relative
//! specifier.  Building it is on by default and opt-out: `AnalysisOptions::resolve_tsconfig_paths`
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
    // `extends` targets outside the supported relative in-root chain are
    // never followed; their presence is detected here and fails closed
    // downstream.
    extends: Option<serde_json::Value>,
    // Read for the selected config only ([`referenced_sibling_configs`]).
    references: Option<serde_json::Value>,
    // Only whether this config compiles any file itself ([`is_solution_only`]).
    files: Option<serde_json::Value>,
    include: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCompilerOptions {
    /// `None` when absent, `Some(None)` for an explicit `null`, which
    /// clears an inherited value as `tsc` does (#11204 review).
    #[serde(default, deserialize_with = "explicit_null")]
    base_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "explicit_null")]
    paths: Option<Option<HashMap<String, Vec<String>>>>,
    /// Read only to decide whether a directory `paths` target may load its
    /// `index` file. A JSON value, so an unusual one never fails the parse.
    module: Option<serde_json::Value>,
    module_resolution: Option<serde_json::Value>,
}

/// Keeps a present JSON `null` distinct from an absent key.
fn explicit_null<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

// ── Public types ──────────────────────────────────────────────────────────────

/// Compiled alias map derived from `tsconfig.json` / `jsconfig.json`.
///
/// Retains every literal and single-`*` key, whatever its templates.
/// Selection precedes file lookup: a winning key whose templates cannot be
/// resolved must block a broader match rather than lend its import to a
/// different source file.
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
    /// Literal entries: key → its templates in declaration order (`tsc`'s
    /// fallback list). An empty list blocks resolution.
    literal_entries: HashMap<String, Vec<String>>,
    /// Glob entries: (prefix, suffix) → templates in declaration order.
    ///
    /// A template may itself contain a `*`; the captured group from the
    /// specifier replaces that `*` in the template.
    glob_entries: Vec<GlobEntry>,
    /// `true` when no `baseUrl` is in effect: `tsc` then rejects a template
    /// that is not `./`- or `../`-relative (TS5090), so reaching one ends
    /// the fallback walk unresolved (#12760).
    templates_must_be_relative: bool,
    /// `true` when the entries above came from a loaded tsconfig/jsconfig.
    /// A map built only to carry `packages` has no tsconfig, so its
    /// unresolved advice stays the flag-off / load-gap advice.
    tsconfig_loaded: bool,
    /// Whether the config's module resolution performs directory lookup, so
    /// a `paths` target naming a directory loads its `index` file.
    directory_index: bool,
    /// In-workspace package names (#4554), consulted after `paths`.
    ///
    /// Shared, not owned: every cache miss hands the map to the parse worker,
    /// and the package table lists every indexed source path.
    packages: std::sync::Arc<super::workspace_packages::WorkspacePackages>,
    /// [`Self::cache_digest`], computed once. The digest renders the whole
    /// package table, so recomputing it for each of the workspace's files
    /// made a run quadratic in the file count. Every mutator resets it.
    digest: std::sync::OnceLock<(u64, u64)>,
    /// Workspace-relative directory (`/`-separated) of the config this map was
    /// compiled from; empty for the workspace-root config. `baseUrl` anchors
    /// here, not at `root` (#12754).
    config_dir: String,
    /// For a per-package map, the digest of the root map it was derived from:
    /// it stands in for the (shared, large) package table in [`Self::cache_digest`]
    /// so many package maps do not each render that table.
    parent_digest: Option<(u64, u64)>,
    /// Content fingerprint of every nested `paths`-declaring config in the
    /// workspace, so a per-package config edit invalidates cached facts that a
    /// root-only digest could not see (#12754).
    nested_configs: u64,
    /// The config files whose alias settings this map states exactly: the
    /// selected config and the bases its `extends` chain followed, minus any
    /// base whose own `paths` a later config replaced. Lexically normalized.
    /// A sibling such as `tsconfig.spec.json` or the unselected
    /// `jsconfig.json` is never here (#12760). Includes the configs of every
    /// entry in `referenced_projects`.
    modeled_configs: Vec<PathBuf>,
    /// Same-directory projects the selected config `references`, each
    /// compiled from its own config chain. Empty for a config without
    /// such references; never nested.
    referenced_projects: Vec<TsAliasMap>,
    /// `true` for a solution-style config, which compiles no file itself:
    /// its own entries above are then not a project, and only
    /// `referenced_projects` answer.
    solution_only: bool,
}

#[derive(Debug, Clone)]
struct GlobEntry {
    /// Part of the pattern key before the `*` (may be empty).
    prefix: String,
    /// Part of the pattern key after the `*` (may be empty).
    suffix: String,
    /// The fallback list in declaration order; an empty list retains the
    /// key as a blocker.
    templates: Vec<String>,
}

/// Typed fail-closed cause for a specifier the alias map did not resolve.
/// Drives the `typescript_path_alias_unresolved` advice text so it names
/// the actual reason (unknown baseUrl vs unmatched pattern vs
/// out-of-root/unresolved candidate) instead of a generic message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TsAliasUnresolveCause {
    /// No alias map was even attempted: the flag was explicitly set false (default is on). Every
    /// flag-ON unavailability carries a typed `TsAliasMapLoadGap` instead,
    /// so this cause never masks a parse failure behind "enable the flag"
    /// advice (#4106-B).
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
                "alias resolution is not enabled (`[typescript] resolve_tsconfig_paths` is false in ripr.toml)",
                "set `[typescript] resolve_tsconfig_paths = true` for credit",
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
                "the matched pattern's candidate did not resolve to exactly one in-root workspace file (zero or multiple files, an unsupported template, a target without a leading `./` in a config that sets no baseUrl, referenced projects that map it differently, or an out-of-root candidate rejected before probing)",
                "point the matched template at exactly one existing workspace file for credit (start it with `./` when the config sets no compilerOptions.baseUrl)",
            ),
        }
    }
}

/// Why no alias map exists even though the flag is ON (#4106-B).
///
/// The flag-ON fail-closed paths used to collapse into the generic
/// "no alias map was available — enable the flag" advice, which told the
/// user to enable a flag that was already enabled and left the real cause
/// (unparseable JSON(C) / unsupported `extends` / missing
/// config / unreadable config) indistinguishable from an opt-out. This
/// typed gap names the actual cause so the `typescript_path_alias_unresolved`
/// advice is actionable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TsAliasMapLoadGap {
    /// Neither `tsconfig.json` nor `jsconfig.json` exists at the root.
    ConfigMissing,
    /// A config file exists but could not be parsed as JSONC (JSON plus the
    /// `//` / `/* */` comments and trailing commas `tsc` accepts, #4549).
    /// `config` records which file was loaded (`tsconfig.json` or the
    /// `jsconfig.json` fallback) so the advice names the real file;
    /// `detail` carries the bounded parser message.
    ConfigUnparseable {
        config: &'static str,
        detail: String,
    },
    /// The config uses an `extends`/`references` form resolution cannot
    /// follow exactly. `config` names the loaded file.
    ExtendsUnsupported { config: &'static str },
    /// The config parsed but its chain sets neither `compilerOptions.paths`
    /// nor `compilerOptions.baseUrl`, so there is no alias to resolve.
    /// `config` names the loaded file.
    IncompleteConfig { config: &'static str },
    /// The config exists but could not be read (size cap or IO error); the
    /// size-limit case is separately disclosed by the capped-read lane.
    /// `config` names the loaded file.
    ReadFailed { config: &'static str },
}

impl TsAliasMapLoadGap {
    /// Typed cause phrase for the limitation's `why_not_actionable` text,
    /// paired with a cause-specific recovery hint. The advice never asks
    /// the user to enable the flag here: the flag is already ON whenever a
    /// gap is produced. Both phrases name the config file that was actually
    /// loaded (`tsconfig.json` or the `jsconfig.json` fallback), so advice
    /// for a JavaScript-only project does not point at a file that does not
    /// exist (review #4138).
    pub(crate) fn parts(&self) -> (String, String) {
        match self {
            Self::ConfigMissing => (
                "no tsconfig.json or jsconfig.json exists at the workspace root, and the importing file's nearest package config does not supply a usable `paths` map; the root map reads only those two workspace-root configs".to_string(),
                "add compilerOptions.paths to the importing package's tsconfig.json (or jsconfig.json), or to the workspace root, for credit; without compilerOptions.baseUrl each target must start with `./`, and a package config is followed only through relative `extends` bases inside the workspace"
                    .to_string(),
            ),
            Self::ConfigUnparseable { config, detail } => (
                format!(
                    "the {config} at the workspace root could not be parsed as JSON with comments ({detail})"
                ),
                format!("fix the {config} syntax for credit"),
            ),
            Self::ExtendsUnsupported { config } => (
                format!(
                    "the {config} at the workspace root uses an `extends`/`references` form alias resolution cannot follow exactly (package-name, symlinked or out-of-workspace base, cycle, missing target, or a `references` entry that is not a readable config inside the workspace or names a project with `references` of its own)"
                ),
                format!(
                    "inline the extended compilerOptions.paths into the workspace-root {config} for credit; extends chains are followed only for relative bases inside the workspace that are regular files"
                ),
            ),
            Self::IncompleteConfig { config } => (
                format!(
                    "the {config} at the workspace root parsed but sets no compilerOptions.paths (its own or inherited) to resolve this specifier"
                ),
                format!(
                    "add a compilerOptions.paths entry for this specifier to {config} for credit; without compilerOptions.baseUrl its target must start with `./`"
                ),
            ),
            Self::ReadFailed { config } => (
                format!("the {config} at the workspace root exists but could not be read"),
                format!(
                    "restore read access to {config} (check permissions and encoding), then re-run the analysis"
                ),
            ),
        }
    }
}

// Counts full digest renders on this thread so a test can tell a memo hit
// from a recomputation that happens to return an equal value.
#[cfg(test)]
thread_local! {
    static DIGEST_COMPUTATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl TsAliasMap {
    /// Deterministic content digest for cache keys (the export-facts cache
    /// keys its entries by every input that can change resolution
    /// outcomes). `literal_entries` iterates in sorted order because
    /// `HashMap` iteration order is not stable; the remaining fields are
    /// order-stable (`Vec`, `BTreeMap`, `BTreeSet`) and hash directly.
    pub(crate) fn cache_digest(&self) -> (u64, u64) {
        *self.digest.get_or_init(|| self.compute_cache_digest())
    }

    fn compute_cache_digest(&self) -> (u64, u64) {
        use std::hash::{Hash, Hasher};
        #[cfg(test)]
        DIGEST_COMPUTATIONS.with(|count| count.set(count.get() + 1));
        let mut first = std::collections::hash_map::DefaultHasher::new();
        let mut second = std::collections::hash_map::DefaultHasher::new();
        first.write_u8(0xA7);
        second.write_u8(0x5C);
        self.root.hash(&mut first);
        self.base_url.hash(&mut first);
        self.base_url_absolute.hash(&mut first);
        self.tsconfig_loaded.hash(&mut first);
        self.solution_only.hash(&mut first);
        for project in &self.referenced_projects {
            project.cache_digest().hash(&mut first);
        }
        self.templates_must_be_relative.hash(&mut first);
        self.directory_index.hash(&mut first);
        self.config_dir.hash(&mut first);
        self.nested_configs.hash(&mut first);
        let mut literals: Vec<(&String, &Vec<String>)> = self.literal_entries.iter().collect();
        literals.sort();
        for (key, value) in literals {
            key.hash(&mut first);
            value.hash(&mut first);
        }
        for glob in &self.glob_entries {
            format!("{glob:?}").hash(&mut first);
        }
        if let Some(parent) = self.parent_digest {
            parent.hash(&mut first);
            parent.hash(&mut second);
        } else {
            format!("{:?}", self.packages).hash(&mut first);
            format!("{:?}", self.packages).hash(&mut second);
        }
        self.config_dir.hash(&mut second);
        self.nested_configs.hash(&mut second);
        for glob in &self.glob_entries {
            format!("{glob:?}").hash(&mut second);
        }
        let mut literals: Vec<(&String, &Vec<String>)> = self.literal_entries.iter().collect();
        literals.sort();
        for (key, value) in literals {
            key.hash(&mut second);
            value.hash(&mut second);
        }
        self.tsconfig_loaded.hash(&mut second);
        self.solution_only.hash(&mut second);
        for project in &self.referenced_projects {
            project.cache_digest().hash(&mut second);
        }
        self.templates_must_be_relative.hash(&mut second);
        self.directory_index.hash(&mut second);
        self.base_url_absolute.hash(&mut second);
        self.base_url.hash(&mut second);
        self.root.hash(&mut second);
        (first.finish(), second.finish())
    }

    /// A map that resolves only in-workspace package names: the tsconfig
    /// alias flag is explicitly false or its config did not load (#4554).
    pub(crate) fn workspace_packages_only(
        root: &Path,
        packages: super::workspace_packages::WorkspacePackages,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            base_url: ".".to_string(),
            packages: std::sync::Arc::new(packages),
            ..Self::default()
        }
    }

    /// Adds in-workspace package-name resolution after `paths` (#4554).
    pub(crate) fn with_workspace_packages(
        mut self,
        packages: super::workspace_packages::WorkspacePackages,
    ) -> Self {
        self.packages = std::sync::Arc::new(packages);
        self.digest = std::sync::OnceLock::new();
        self
    }

    /// Records the fingerprint of the workspace's nested `paths`-declaring
    /// configs ([`nested_configs_fingerprint`]) in the cache digest.
    pub(crate) fn with_nested_configs(mut self, fingerprint: u64) -> Self {
        self.nested_configs = fingerprint;
        self.digest = std::sync::OnceLock::new();
        self
    }

    /// Compile the alias map for the importer-owning config in `config_dir`
    /// (an absolute directory under `root`), per tsc's rule that each
    /// compilation unit owns its effective `compilerOptions` (#12754).
    ///
    /// `Err(())` fails closed — the caller leaves the specifier unresolved —
    /// when the directory does not hold exactly one `paths`-declaring
    /// conventional config (both `tsconfig.json` and `jsconfig.json` declaring
    /// is an unmodeled selection), or the config is unreadable, unparseable,
    /// uses an unsupported `extends`/`references` form, or has an unsafe
    /// `baseUrl`. Same-directory projects the config `references` are
    /// compiled with it. Workspace package names remain shared with the root map.
    pub(crate) fn nested_for(&self, config_dir: &Path) -> Result<Self, ()> {
        let declaring: Vec<PathBuf> = ["tsconfig.json", "jsconfig.json"]
            .iter()
            .map(|name| config_dir.join(name))
            .filter(|path| path.is_file() && config_file_owns_paths(&self.root, path))
            .collect();
        let [config_path] = declaring.as_slice() else {
            return Err(());
        };
        let text = read_config_capped(config_path).map_err(|_read_err| ())?;
        let mut map = parse_alias_map(&self.root, config_path, &text).map_err(|_blocker| ())?;
        map.packages = self.packages.clone();
        map.parent_digest = Some(self.cache_digest());
        Ok(map)
    }

    /// The workspace package directory a bare specifier names, when no
    /// `paths` key owns it (#4769): an unresolved import of it is a package
    /// manifest question, not a tsconfig one.
    pub(crate) fn workspace_package_dir_for(&self, specifier: &str) -> Option<&Path> {
        if self.tsconfig_loaded && self.paths_key_matches(specifier) {
            return None;
        }
        self.packages.package_dir_for(specifier)
    }

    pub(crate) fn build_source_in(&self, package: &Path) -> Option<&str> {
        self.packages.build_source_in(package)
    }

    pub(crate) fn own_build_entry(
        &self,
        importer: &Path,
        module: &str,
    ) -> Option<&super::workspace_packages::BuildEntry> {
        self.packages.own_build_entry(&self.root, importer, module)
    }

    /// The workspace root this map resolves within.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// `true` when the map's `paths` entries came from a loaded config.
    pub(crate) fn has_tsconfig(&self) -> bool {
        self.tsconfig_loaded
    }

    /// Whether this map states the alias settings of the config file at
    /// `path` exactly: the selected config, or a base its `extends` chain
    /// followed whose `paths` are the effective ones. Loading
    /// `tsconfig.json` says nothing about a sibling `tsconfig.spec.json` or
    /// the unselected `jsconfig.json` (#12760).
    pub(crate) fn models_config(&self, path: &Path) -> bool {
        self.tsconfig_loaded
            && normalize_lexically(path).is_some_and(|path| self.modeled_configs.contains(&path))
    }

    /// `true` when a `baseUrl` is in effect for this config or a project it
    /// references. `tsc` then resolves a bare specifier no `paths` key
    /// matches under that anchor, which this map does not model, so a caller
    /// must not read "no key matches" as "names nothing in the workspace".
    pub(crate) fn base_url_in_effect(&self) -> bool {
        self.tsconfig_loaded
            && (!self.templates_must_be_relative
                || self
                    .referenced_projects
                    .iter()
                    .any(|project| !project.templates_must_be_relative))
    }

    /// `true` when this map has no entries (opt-out / parse-failure path).
    pub(crate) fn is_empty(&self) -> bool {
        self.projects()
            .all(|project| project.literal_entries.is_empty() && project.glob_entries.is_empty())
    }

    /// The projects whose `paths` can govern an importer of this config's
    /// directory: the config itself unless it is solution-style, then each
    /// same-directory project it references.
    fn projects(&self) -> impl Iterator<Item = &TsAliasMap> {
        (!self.solution_only)
            .then_some(self)
            .into_iter()
            .chain(&self.referenced_projects)
    }

    /// `true` when `baseUrl` was absolute (or contained non-normal path
    /// components) and alias resolution therefore fail-closes. The caller
    /// surfaces this as the named limitation
    /// `typescript_base_url_absolute_unsupported`.
    pub(crate) fn base_url_absolute(&self) -> bool {
        self.projects().any(|project| project.base_url_absolute)
    }

    /// The typed fail-closed cause for a specifier that did not resolve,
    /// so the `typescript_path_alias_unresolved` advice can name the real
    /// reason instead of a generic message.
    pub(crate) fn unresolve_cause_for(&self, specifier: &str) -> TsAliasUnresolveCause {
        if self.base_url_absolute() {
            return TsAliasUnresolveCause::BaseUrlAbsolute;
        }
        if self.is_empty() {
            return TsAliasUnresolveCause::NoPatterns;
        }
        if self.paths_key_matches(specifier) {
            // A key owned this specifier; the candidate itself failed.
            return TsAliasUnresolveCause::CandidateUnresolved;
        }
        TsAliasUnresolveCause::PatternUnmatched
    }

    /// Resolve a non-relative specifier to a canonical workspace-relative path.
    ///
    /// Returns `None` (fail-closed) unless ALL of the following hold:
    /// 1. `specifier` is non-relative (not `.` / `..` and does not start
    ///    with `./` or `../`).
    /// 2. An exact key or a unique longest-prefix single-`*` key matches.
    /// 3. Walking the matched value array in order, every template before
    ///    the winning one names a location where provably nothing exists.
    /// 4. The winning template has at most one `*`.
    /// 5. After substituting the captured `*`, its candidate path resolves to
    ///    EXACTLY ONE existing workspace file (.ts/.tsx/.js/.jsx/.mts/.cts/
    ///    .mjs/.cjs). A target naming a directory adds that directory's
    ///    `index` file under the rules on `unique_file_for`.
    ///
    /// A specifier no `paths` key matches falls back to an in-workspace
    /// package name (`workspace_packages.rs`, #4554), as the compiler falls
    /// back to module resolution. A specifier a key does match never falls
    /// back: the compiler uses that key's substitutions, so when this map
    /// cannot pick one (an unprovable fallback, a tie, two files) the import
    /// stays unresolved rather than crediting the package's own entry.
    pub(crate) fn resolve(&self, specifier: &str) -> Option<PathBuf> {
        if super::paths::is_relative_specifier(specifier) {
            return None; // relative paths are handled by the normal resolver
        }
        if self.tsconfig_loaded && self.paths_key_matches(specifier) {
            return self.resolve_paths(specifier);
        }
        self.packages.resolve(specifier)
    }

    /// Whether a `paths` key (exact or single-`*` pattern) owns `specifier`.
    /// The wrong-owner safeguard (`normalized_relative_import_module`,
    /// #12754 slice) consults this to withhold the root map when a nearer
    /// config owns the specifier's compilation unit.
    pub(crate) fn paths_key_matches(&self, specifier: &str) -> bool {
        self.projects()
            .any(|project| project.own_key_matches(specifier))
    }

    /// Whether this one project's own `paths` keys own `specifier`.
    fn own_key_matches(&self, specifier: &str) -> bool {
        self.literal_entries.contains_key(specifier)
            || self
                .glob_entries
                .iter()
                .any(|entry| match_glob(specifier, &entry.prefix, &entry.suffix).is_some())
    }

    /// Resolve through every project that may own the importer. Project
    /// membership (`include`/`files`) is not modeled, so the answer must not
    /// depend on it: every project whose keys claim the specifier has to
    /// name the same file, and a project without a claim must not be able to
    /// resolve the specifier some other way.
    fn resolve_paths(&self, specifier: &str) -> Option<PathBuf> {
        if self.referenced_projects.is_empty() && !self.solution_only {
            return self.own_resolve_paths(specifier);
        }
        let mut agreed: Option<PathBuf> = None;
        let mut unclaimed = false;
        for project in self.projects() {
            if !project.own_key_matches(specifier) {
                // With a `baseUrl` in effect `tsc` tries the bare specifier
                // under that anchor before anything else, so this project
                // may name a different file unless nothing sits there.
                if !project.templates_must_be_relative
                    && !project.provably_absent(&strip_ts_ext(specifier))
                {
                    return None;
                }
                unclaimed = true;
                continue;
            }
            let found = project.own_resolve_paths(specifier)?;
            if agreed.as_ref().is_some_and(|previous| *previous != found) {
                return None;
            }
            agreed = Some(found);
        }
        // A project with no matching key falls back to module resolution; a
        // workspace package of that name is then a second, different answer.
        if unclaimed && self.packages.names_package(specifier) {
            return None;
        }
        agreed
    }

    fn own_resolve_paths(&self, specifier: &str) -> Option<PathBuf> {
        if self.literal_entries.is_empty() && self.glob_entries.is_empty() {
            return None;
        }

        // 1. Try literal match first.
        if let Some(templates) = self.literal_entries.get(specifier) {
            return self.first_existing(templates, None);
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
        self.first_existing(&entry.templates, Some(&captured))
    }

    /// Walk a `paths` fallback list as `tsc` does: the first substitution
    /// that resolves wins. The next template is tried only after this one's
    /// candidate is provably absent; anything less certain ends the walk
    /// unresolved, because `tsc` may have stopped there on a file this map
    /// does not model.
    fn first_existing(&self, templates: &[String], captured: Option<&str>) -> Option<PathBuf> {
        let mut remaining = templates.iter();
        while let Some(template) = remaining.next() {
            if template.chars().filter(|&c| c == '*').count() > 1
                || (self.templates_must_be_relative && !is_dot_relative(template))
            {
                return None;
            }
            let expanded = match captured {
                Some(captured) => template.replace('*', captured),
                None => template.clone(),
            };
            let candidate_base = strip_ts_ext(&expanded);
            if let Some(found) = self.unique_file_for(&candidate_base, candidate_base == expanded) {
                return Some(found);
            }
            // The directory listing is only paid for when a fallback exists.
            if remaining.len() == 0 || !self.provably_absent(&candidate_base) {
                return None;
            }
        }
        None
    }

    /// `true` only when nothing `tsc` could resolve sits at `candidate_base`:
    /// its directory holds no entry named `<stem>` or `<stem>.<anything>`,
    /// where `<stem>` is the candidate's last segment up to its first `.`,
    /// compared without regard to case. That rules out the routed source
    /// files, a declaration file (`owner.d.ts`, `style.d.css.ts`), any other
    /// extension `tsc` can be configured to load, a directory it would
    /// search for an index or a `package.json`, and a differently-cased
    /// file a case-insensitive host would open. An unsafe candidate, or a
    /// directory that cannot be listed, proves nothing.
    fn provably_absent(&self, candidate_base: &str) -> bool {
        if self.base_url_absolute
            || !is_safe_relative(Path::new(candidate_base))
            || !is_safe_relative(Path::new(&self.base_url))
        {
            return false;
        }
        let candidate = self
            .root
            .join(&self.config_dir)
            .join(self.base_url.trim_matches('/'))
            .join(candidate_base);
        let (Some(parent), Some(stem)) = (candidate.parent(), candidate.file_name()) else {
            return false;
        };
        let stem = stem.to_string_lossy().to_lowercase();
        let stem = stem.split('.').next().unwrap_or_default();
        let entries = match std::fs::read_dir(parent) {
            Ok(entries) => entries,
            // No such directory: nothing can be in it. Any other failure
            // (a file in the way, permissions) leaves the question open.
            Err(err) => {
                return err.kind() == std::io::ErrorKind::NotFound
                    && std::fs::symlink_metadata(parent).is_err();
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                return false;
            };
            let name = entry.file_name();
            if name
                .to_string_lossy()
                .to_lowercase()
                .strip_prefix(stem)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
            {
                return false;
            }
        }
        true
    }

    /// Given a base candidate string (extension-stripped, slash-separated),
    /// try each TS extension on the candidate and collect the unique matching
    /// file. With `extensionless_target` (the substituted `paths` target named
    /// no source extension, so it may name a directory) and a module
    /// resolution that performs directory lookup, the directory's `index` file
    /// is a candidate too.
    ///
    /// Returns `None` if zero files or more than one file match: `x.ts`
    /// beside `x/index.ts` is a resolver-order question this does not model.
    ///
    /// Candidates containing `..` segments, rooted components, or prefixes
    /// (drive letters / UNC) are rejected BEFORE any filesystem probe: they
    /// would make `is_file()` read outside the workspace root, and a
    /// component-wise `strip_prefix` credit could still escape the root.
    fn unique_file_for(&self, candidate_base: &str, extensionless_target: bool) -> Option<PathBuf> {
        if self.base_url_absolute || !is_safe_relative(Path::new(candidate_base)) {
            return None;
        }
        // The workspace root itself is trusted; only the configured baseUrl
        // segment must stay relative and free of `..` components.
        if !is_safe_relative(Path::new(&self.base_url)) {
            return None;
        }
        let base_dir = self
            .root
            .join(&self.config_dir)
            .join(self.base_url.trim_matches('/'));
        let extensions = [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"];
        // A target naming a directory (`"@acme/*": ["packages/*/src"]`) loads
        // that directory's `index` file. A `package.json` there would send
        // the compiler to its entry fields instead, which this does not read,
        // so such a directory keeps the import unresolved. A target written
        // with a source extension (`src/x.ts`) names a file, never `x/index`.
        // Directory lookup adds `.ts`/`.tsx`/`.js`/`.jsx` only; the compiler
        // does not try `index.mts` or `index.cjs` for a directory.
        let directory = base_dir.join(candidate_base);
        let index_base = (extensionless_target
            && self.directory_index
            && directory.is_dir()
            && !directory.join("package.json").exists())
        .then(|| format!("{candidate_base}/index"));
        let index_extensions = &extensions[..4];
        let mut found: Vec<PathBuf> = Vec::new();
        for (base, ext) in extensions.iter().map(|ext| (candidate_base, ext)).chain(
            index_base
                .iter()
                .flat_map(|base| index_extensions.iter().map(move |ext| (base.as_str(), ext))),
        ) {
            let candidate = base_dir.join(format!("{base}{ext}"));
            if candidate.is_file() {
                // Normalize to forward-slash workspace-relative path.
                if let Ok(rel) = candidate.strip_prefix(&self.root) {
                    // A `.` baseUrl under a package dir leaves `./` inside the
                    // relative path; drop it so the module spelling is canonical.
                    found.push(
                        rel.components()
                            .filter(|part| !matches!(part, std::path::Component::CurDir))
                            .collect::<PathBuf>(),
                    );
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
/// - The chain sets neither `baseUrl` nor `paths`.
/// - A `references` entry that cannot be read exactly, or an `extends` form outside the supported
///   relative in-root chain (package name, symlinked or out-of-root base,
///   cycle, missing target unless the config is self-sufficient, or deeper
///   than `MAX_EXTENDS_DEPTH`).
///
/// Test-only convenience over [`load_alias_map_with_read_error`] for the
/// existing fixture callers; production surfaces read outcomes through the
/// `_with_read_error` variant.
#[cfg(test)]
pub(crate) fn load_alias_map(root: &Path) -> Option<TsAliasMap> {
    load_alias_map_with_read_error(root).0
}

/// Like [`load_alias_map`], but also reports the config path and read error
/// when reading the first existing config file fails, plus the typed reason
/// no map exists when the fail-closed path fires (#4106-B).
///
/// The read error is preserved so `analyze_diff` can surface size-limit
/// outcomes (`OverFileLimit` / `OverWorkspaceBudget`) as named limitations
/// instead of failing silently closed. Plain IO failures stay in the second
/// slot too; disclosure for those is owned by the read-error lane, which
/// filters on `CappedReadError::is_size_limit`. The third slot carries the
/// `TsAliasMapLoadGap` for every flag-ON unavailability so the alias-gap
/// advice names the real cause instead of a generic "enable the flag".
pub(crate) fn load_alias_map_with_read_error(
    root: &Path,
) -> (
    Option<TsAliasMap>,
    Option<(PathBuf, super::bounded_read::CappedReadError)>,
    Option<TsAliasMapLoadGap>,
) {
    for &filename in &["tsconfig.json", "jsconfig.json"] {
        let path = root.join(filename);
        if !path.is_file() {
            continue;
        }
        // Capped read: a read failure fail-closes the alias map; size-limit
        // outcomes are disclosed by the caller through the second slot.
        return match read_config_capped(&path) {
            Ok(text) => match parse_alias_map(root, &path, &text) {
                Ok(map) => (Some(map), None, None),
                Err(blocker) => {
                    let gap = match blocker {
                        TsAliasMapBlocker::Unparseable { detail } => {
                            TsAliasMapLoadGap::ConfigUnparseable {
                                config: filename,
                                detail,
                            }
                        }
                        TsAliasMapBlocker::ExtendsUnsupported | TsAliasMapBlocker::ExtendsCycle => {
                            TsAliasMapLoadGap::ExtendsUnsupported { config: filename }
                        }
                        TsAliasMapBlocker::IncompleteConfig => {
                            TsAliasMapLoadGap::IncompleteConfig { config: filename }
                        }
                    };
                    (None, None, Some(gap))
                }
            },
            Err(err) => (
                None,
                Some((path, err)),
                Some(TsAliasMapLoadGap::ReadFailed { config: filename }),
            ),
        };
    }
    (None, None, Some(TsAliasMapLoadGap::ConfigMissing))
}

/// Why the alias-map compiler refused a config that was read
/// successfully. Internal to `parse_alias_map`; surfaced to callers as
/// a `TsAliasMapLoadGap`.
enum TsAliasMapBlocker {
    Unparseable {
        detail: String,
    },
    ExtendsUnsupported,
    /// A cycle in the `extends` chain. Surfaced as `ExtendsUnsupported`, but
    /// never tolerated by a self-sufficient config, since `tsc` rejects it.
    ExtendsCycle,
    IncompleteConfig,
}

/// `compilerOptions` after merging an `extends` chain (#11204): the
/// extending config wins per key, a declared `paths` object replaces the
/// inherited one whole (as `tsc` treats it, even when empty), and
/// `baseUrl` is the nearest declaration. `paths` is `None` until some
/// config in the chain declares it.
///
/// `baseUrl` keeps its provenance: `tsc` resolves it against the directory
/// of the config that DECLARES it, not the extending one, so an inherited
/// cross-directory `baseUrl` is rebased exactly (#11204). `paths` keeps the
/// same provenance, because without a `baseUrl` its templates resolve against
/// the config that declares them (#12760).
struct MergedCompilerOptions {
    /// `(declaring config directory, raw baseUrl)`.
    base_url: Option<(PathBuf, String)>,
    paths: Option<DeclaredPaths>,
    /// See [`TsAliasMap::modeled_configs`].
    modeled_configs: Vec<PathBuf>,
    /// Whether the effective module resolution loads a directory's `index`
    /// file. See [`directory_index_lookup`].
    directory_index: bool,
}

/// A `paths` object and the config file that declares it.
struct DeclaredPaths {
    config: PathBuf,
    entries: HashMap<String, Vec<String>>,
}

/// Whether the effective module resolution performs directory lookup, so a
/// `paths` target naming a directory loads its `index` file.
///
/// True for `moduleResolution` `node`, `node10` or `bundler`, and, with no
/// `moduleResolution`, for a `module` that is absent, `commonjs` or
/// `preserve`. False otherwise: `node16`/`nodenext` skip directory lookup for
/// an ES-module importer, `classic` never performs it, and another `module`
/// value selects one of those two when `moduleResolution` is absent. False
/// too when an unfollowed `extends` base could have set `moduleResolution`.
fn directory_index_lookup(chain: &ChainOptions) -> bool {
    if chain.module_resolution_unknown {
        return false;
    }
    match (chain.module_resolution.as_deref(), chain.module.as_deref()) {
        (Some(resolution), _) => matches!(resolution, "node" | "node10" | "bundler"),
        (None, None) => true,
        (None, Some(module)) => matches!(module, "commonjs" | "preserve"),
    }
}

/// A `module` / `moduleResolution` value as the compiler compares it:
/// case-insensitively. A non-string value becomes an empty name, which no
/// directory-lookup rule accepts.
fn option_name(value: serde_json::Value) -> String {
    value.as_str().unwrap_or_default().to_ascii_lowercase()
}

/// One chain level's merge state: `None` = not set by this chain,
/// `Some(None)` = cleared by an explicit `null`, `Some(Some(_))` = set.
struct ChainOptions {
    base_url: Option<Option<(PathBuf, String)>>,
    paths: Option<Option<DeclaredPaths>>,
    /// Every config merged so far, with the config that declares the
    /// `paths` in effect for it (`None` when it has none).
    followed: Vec<(PathBuf, Option<PathBuf>)>,
    /// Lowercased `module` / `moduleResolution`, nearest declaration.
    module: Option<String>,
    module_resolution: Option<String>,
    /// An `extends` base this chain did not follow could have set
    /// `moduleResolution`, and no config above it declares one.
    module_resolution_unknown: bool,
}

/// Maximum `extends` hops followed from the workspace-root config.
const MAX_EXTENDS_DEPTH: usize = 8;

fn parse_raw_tsconfig(text: &str) -> Result<RawTsConfig, TsAliasMapBlocker> {
    // `tsc` reads tsconfig.json / jsconfig.json as JSONC (#4549): strip
    // comments and trailing commas first, then parse strictly. A lexical
    // failure (unterminated block comment or string) and a strict-parse
    // failure of the normalized text both fail closed as `Unparseable`.
    let normalized = strip_jsonc(text).map_err(|detail| TsAliasMapBlocker::Unparseable {
        detail: detail.to_string(),
    })?;
    serde_json::from_str(&normalized).map_err(|err| TsAliasMapBlocker::Unparseable {
        // Bound the parser message: it can quote long input spans.
        detail: err.to_string().chars().take(160).collect(),
    })
}

/// Merge the `compilerOptions` of the config at `config_path` (already
/// read as `text`) with every config it extends, bases first.
///
/// Relative `extends` targets (`./`, `../`) are followed anywhere inside
/// `root` when the target is a regular file (not a symlink), up to
/// [`MAX_EXTENDS_DEPTH`] hops; an inherited `baseUrl` stays anchored at its
/// declaring config (#11204). Fail-closed as `ExtendsUnsupported` for
/// a cycle, an over-deep chain, and any target this loader
/// cannot read exactly (package-name or `node_modules` base, a target outside
/// `root`, a symlink, a missing, unreadable or invalid base) — EXCEPT when
/// the extending config itself declares both `baseUrl` and `paths`: nothing
/// it inherits can then change alias resolution, so an unfollowable base is
/// ignored instead of refusing the whole config.
fn merge_compiler_options(
    root: &Path,
    config_path: &Path,
    text: &str,
    visited: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<MergedCompilerOptions, TsAliasMapBlocker> {
    let chain = merge_chain(root, config_path, text, visited, depth)?;
    let directory_index = directory_index_lookup(&chain);
    let paths = chain.paths.flatten();
    // A followed config is modeled only when the `paths` in effect for it
    // are the ones this map states. A base whose own `paths` a later config
    // replaced is not, and neither is a config between them that inherits
    // the replaced `paths` without declaring any.
    let modeled_configs = chain
        .followed
        .into_iter()
        .filter(|(_, effective)| {
            effective.as_ref().is_none_or(|effective| {
                paths
                    .as_ref()
                    .is_some_and(|declared| declared.config == *effective)
            })
        })
        .filter_map(|(config, _)| normalize_lexically(&config))
        .collect();
    Ok(MergedCompilerOptions {
        directory_index,
        base_url: chain.base_url.flatten(),
        paths,
        modeled_configs,
    })
}

fn merge_chain(
    root: &Path,
    config_path: &Path,
    text: &str,
    visited: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<ChainOptions, TsAliasMapBlocker> {
    if visited.iter().any(|seen| seen == config_path) {
        return Err(TsAliasMapBlocker::ExtendsCycle);
    }
    if depth > MAX_EXTENDS_DEPTH {
        return Err(TsAliasMapBlocker::ExtendsUnsupported);
    }
    visited.push(config_path.to_path_buf());
    let merged = merge_chain_level(root, config_path, text, visited, depth);
    // Pop on every outcome so a failed branch cannot flag a later sibling
    // base as a cycle.
    visited.pop();
    merged
}

fn merge_chain_level(
    root: &Path,
    config_path: &Path,
    text: &str,
    visited: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<ChainOptions, TsAliasMapBlocker> {
    // `references` are not inherited through `extends`, so a base's are
    // irrelevant here; the selected config's are handled by `parse_alias_map`.
    let raw = parse_raw_tsconfig(text)?;
    let config_dir = config_path
        .parent()
        .ok_or(TsAliasMapBlocker::ExtendsUnsupported)?;
    let mut merged = ChainOptions {
        base_url: None,
        paths: None,
        followed: Vec::new(),
        module: None,
        module_resolution: None,
        module_resolution_unknown: false,
    };
    // Only a non-null own `baseUrl` AND own `paths` make inheritance
    // irrelevant; a `null` clears instead of setting.
    let self_sufficient = raw.compiler_options.as_ref().is_some_and(|options| {
        matches!(options.base_url, Some(Some(_))) && matches!(options.paths, Some(Some(_)))
    });
    if let Some(extends) = &raw.extends {
        let targets: Vec<Option<&str>> = match extends {
            serde_json::Value::String(single) => vec![Some(single.as_str())],
            serde_json::Value::Array(many) => many.iter().map(serde_json::Value::as_str).collect(),
            _ => vec![None],
        };
        for target in targets {
            let base = target
                .and_then(|target| extends_target_path(root, config_dir, target))
                .ok_or(TsAliasMapBlocker::ExtendsUnsupported)
                .and_then(|base_path| {
                    let base_text = read_config_capped(&base_path)
                        .map_err(|_read_err| TsAliasMapBlocker::ExtendsUnsupported)?;
                    merge_chain(root, &base_path, &base_text, visited, depth + 1)
                });
            let base = match base {
                Ok(base) => base,
                // A cycle anywhere below is never tolerated: `tsc` rejects it.
                Err(TsAliasMapBlocker::ExtendsCycle) => {
                    return Err(TsAliasMapBlocker::ExtendsCycle);
                }
                Err(_blocker) if self_sufficient => {
                    merged.module_resolution_unknown = true;
                    continue;
                }
                Err(blocker) => return Err(blocker),
            };
            // Later array entries override earlier ones (a `null` included);
            // the extending config itself is applied after the whole chain.
            if base.base_url.is_some() {
                merged.base_url = base.base_url;
            }
            if base.paths.is_some() {
                merged.paths = base.paths;
            }
            merged.followed.extend(base.followed);
            if base.module.is_some() {
                merged.module = base.module;
            }
            if base.module_resolution.is_some() || base.module_resolution_unknown {
                merged.module_resolution = base.module_resolution;
                merged.module_resolution_unknown = base.module_resolution_unknown;
            }
        }
    }
    if let Some(options) = raw.compiler_options {
        if let Some(base_url) = options.base_url {
            merged.base_url = Some(base_url.map(|raw| (config_dir.to_path_buf(), raw)));
        }
        if let Some(paths) = options.paths {
            merged.paths = Some(paths.map(|entries| {
                DeclaredPaths {
                    config: config_path.to_path_buf(),
                    // `tsc` normalizes `\` to `/` in a substitution before it
                    // joins or tests it, so `.\src\*` is `./src/*`.
                    entries: entries
                        .into_iter()
                        .map(|(key, templates)| {
                            let templates = templates
                                .into_iter()
                                .map(|template| template.replace('\\', "/"))
                                .collect();
                            (key, templates)
                        })
                        .collect(),
                }
            }));
        }
        if let Some(module) = options.module {
            merged.module = Some(option_name(module));
        }
        if let Some(resolution) = options.module_resolution {
            merged.module_resolution = Some(option_name(resolution));
            merged.module_resolution_unknown = false;
        }
    }
    let effective_paths_config = merged
        .paths
        .as_ref()
        .and_then(Option::as_ref)
        .map(|declared| declared.config.clone());
    merged
        .followed
        .push((config_path.to_path_buf(), effective_paths_config));
    Ok(merged)
}

/// The config file a relative `extends` target names, when it is a regular
/// (non-symlink) file lexically inside `root`. Package-name targets and
/// anything else return `None`.
fn extends_target_path(root: &Path, config_dir: &Path, target: &str) -> Option<PathBuf> {
    if !target.starts_with("./") && !target.starts_with("../") {
        return None;
    }
    let named = normalize_lexically(&config_dir.join(target))?;
    let root = normalize_lexically(root)?;
    // `tsc` appends `.json` to a target that does not end in it and uses
    // that file when it exists, else the target as written.
    let candidates = if named.to_string_lossy().ends_with(".json") {
        vec![named]
    } else {
        let mut with_json = named.clone().into_os_string();
        with_json.push(".json");
        vec![PathBuf::from(with_json), named]
    };
    // The first spelling that exists in any form decides: a symlinked or
    // otherwise unsafe `<target>.json` refuses the chain rather than letting
    // a second spelling stand in for the file `tsc` would read.
    let chosen = candidates
        .into_iter()
        .find(|candidate| std::fs::symlink_metadata(candidate).is_ok())?;
    regular_file_without_symlinks(&root, &chosen).then_some(chosen)
}

/// `true` when `path` lies lexically inside `root`, every component below
/// `root` (directories included) is not a symlink, and the last one is a
/// regular file, so the read cannot leave the workspace.
fn regular_file_without_symlinks(root: &Path, path: &Path) -> bool {
    let Ok(below) = path.strip_prefix(root) else {
        return false;
    };
    let mut current = root.to_path_buf();
    let mut components = below.components().peekable();
    while let Some(part) = components.next() {
        current.push(part.as_os_str());
        let Ok(meta) = std::fs::symlink_metadata(&current) else {
            return false;
        };
        let file_type = meta.file_type();
        let acceptable = if components.peek().is_some() {
            file_type.is_dir()
        } else {
            file_type.is_file()
        };
        if !acceptable {
            return false;
        }
    }
    true
}

/// `path` with `.` dropped and `..` applied lexically; `None` when `..`
/// would climb above the path's own start.
fn normalize_lexically(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            // Popping back to an empty path is fine (a relative root such
            // as `.`); popping past it is an escape.
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// Maximum `references` entries read from the selected config.
const MAX_REFERENCES: usize = 32;

fn parse_alias_map(
    root: &Path,
    config_path: &Path,
    text: &str,
) -> Result<TsAliasMap, TsAliasMapBlocker> {
    let raw = parse_raw_tsconfig(text)?;
    let (mut map, own_declares) = compile_project(root, config_path, text)?;
    let Some(references) = &raw.references else {
        return own_declares
            .then_some(map)
            .ok_or(TsAliasMapBlocker::IncompleteConfig);
    };
    map.solution_only = is_solution_only(&raw);
    let mut declares = own_declares && !map.solution_only;
    for sibling in referenced_sibling_configs(root, config_path, references)? {
        // Any sibling this loader cannot state exactly leaves the importer's
        // project unknown, so the whole config is refused.
        let unsupported = TsAliasMapBlocker::ExtendsUnsupported;
        let Ok(sibling_text) = read_config_capped(&sibling) else {
            return Err(unsupported);
        };
        match parse_raw_tsconfig(&sibling_text) {
            Ok(sibling_raw) if sibling_raw.references.is_none() => {}
            _ => return Err(unsupported),
        }
        let Ok((project, project_declares)) = compile_project(root, &sibling, &sibling_text) else {
            return Err(unsupported);
        };
        declares |= project_declares;
        map.modeled_configs
            .extend(project.modeled_configs.iter().cloned());
        map.referenced_projects.push(project);
    }
    declares
        .then_some(map)
        .ok_or(TsAliasMapBlocker::IncompleteConfig)
}

/// `true` for a solution-style config: `"files": []` with no `include`
/// entries, which compiles no file itself. A config with `extends` could
/// inherit `include`, so it is treated as compiling files; that only adds a
/// project that must agree.
fn is_solution_only(raw: &RawTsConfig) -> bool {
    let empty_array = |value: &serde_json::Value| value.as_array().is_some_and(Vec::is_empty);
    raw.extends.is_none()
        && raw.files.as_ref().is_some_and(empty_array)
        && raw.include.as_ref().is_none_or(empty_array)
}

/// The same-directory configs `references` names, lexically normalized.
///
/// `tsc` reads `path` as a config file when it ends in `.json` and as a
/// directory holding `tsconfig.json` otherwise. Every entry must name a
/// regular, non-symlinked file inside `root`, or the whole value is refused:
/// an entry this loader cannot read is a project it cannot rule out. Entries
/// in other directories are checked and then left to the nearest-config walk.
fn referenced_sibling_configs(
    root: &Path,
    config_path: &Path,
    references: &serde_json::Value,
) -> Result<Vec<PathBuf>, TsAliasMapBlocker> {
    let unsupported = || TsAliasMapBlocker::ExtendsUnsupported;
    let items = references.as_array().ok_or_else(unsupported)?;
    if items.len() > MAX_REFERENCES {
        return Err(unsupported());
    }
    let root = normalize_lexically(root).ok_or_else(unsupported)?;
    let config_path = normalize_lexically(config_path).ok_or_else(unsupported)?;
    let config_dir = config_path.parent().ok_or_else(unsupported)?;
    let mut siblings: Vec<PathBuf> = Vec::new();
    for item in items {
        let path = item
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(unsupported)?;
        let named = normalize_lexically(&config_dir.join(path)).ok_or_else(unsupported)?;
        let target = if path.ends_with(".json") {
            named
        } else {
            named.join("tsconfig.json")
        };
        if target == config_path || !regular_file_without_symlinks(&root, &target) {
            return Err(unsupported());
        }
        if target.parent() == Some(config_dir) && !siblings.contains(&target) {
            siblings.push(target);
        }
    }
    Ok(siblings)
}

/// Compile one config's own alias map from its `extends` chain, ignoring
/// `references`. The flag is `false` when the chain sets neither `baseUrl`
/// nor `paths`: the map is then empty and anchored at the config directory.
fn compile_project(
    root: &Path,
    config_path: &Path,
    text: &str,
) -> Result<(TsAliasMap, bool), TsAliasMapBlocker> {
    let merged = merge_compiler_options(root, config_path, text, &mut Vec::new(), 0)?;
    let declares = merged.base_url.is_some() || merged.paths.is_some();
    // With no effective `baseUrl`, `tsc` resolves each template against the
    // directory of the config that declares `paths`, and rejects a template
    // that is not relative (TS5090). Such a template keeps its key below as a
    // blocker rather than being resolved against an anchor `tsc` never uses.
    let templates_must_be_relative = merged.base_url.is_none();
    let paths_dir = merged
        .paths
        .as_ref()
        .and_then(|declared| declared.config.parent().map(Path::to_path_buf));
    let own_dir = config_path.parent().map(Path::to_path_buf);
    let (declaring_dir, raw_base_url) = merged
        .base_url
        .or_else(|| paths_dir.or(own_dir).map(|dir| (dir, ".".to_string())))
        .ok_or(TsAliasMapBlocker::IncompleteConfig)?;
    let paths = merged
        .paths
        .map(|declared| declared.entries)
        .unwrap_or_default();
    let directory_index = merged.directory_index;

    // `baseUrl` resolves against its DECLARING config (#11204). An absolute
    // baseUrl (POSIX `/…`, drive-letter, or UNC), or one whose anchor
    // escapes the workspace root, cannot be anchored in-root. Keep the map
    // so the caller can surface the named limitation, but flag every lookup
    // to fail closed instead of guessing an in-root path.
    let raw_is_rooted = Path::new(&raw_base_url).components().any(|part| {
        matches!(
            part,
            std::path::Component::RootDir | std::path::Component::Prefix(_)
        )
    });
    let anchor = if raw_is_rooted {
        None
    } else {
        normalize_lexically(&declaring_dir.join(&raw_base_url))
            .zip(normalize_lexically(root))
            .and_then(|(anchor, root)| {
                anchor.strip_prefix(&root).ok().map(|rel| {
                    rel.components()
                        .map(|part| part.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/")
                })
            })
    };
    let base_url_absolute = anchor.is_none();
    let (config_dir, base_url) = match anchor {
        Some(anchor) => (anchor, ".".to_string()),
        None => (config_dir_relative(root, config_path), raw_base_url),
    };

    let mut literal_entries: HashMap<String, Vec<String>> = HashMap::new();
    let mut glob_entries: Vec<GlobEntry> = Vec::new();

    for (key, values) in &paths {
        // Every key is retained with its templates in declaration order.
        // Dropping a more-specific key whose templates turn out unusable
        // would let a broader alias claim an import that it does not own.
        // Determine if key is literal or single-`*` glob.
        let star_count = key.chars().filter(|&c| c == '*').count();
        if star_count == 0 {
            // Literal key.
            literal_entries.insert(key.clone(), values.clone());
        } else if star_count == 1 {
            // Single-`*` glob: split into prefix/suffix.
            if let Some(star_pos) = key.find('*') {
                glob_entries.push(GlobEntry {
                    prefix: key[..star_pos].to_string(),
                    suffix: key[star_pos + 1..].to_string(),
                    templates: values.clone(),
                });
            }
        }
        // Multi-`*` keys → silently skipped (fail-closed).
    }

    let map = TsAliasMap {
        root: root.to_path_buf(),
        base_url,
        base_url_absolute,
        literal_entries,
        glob_entries,
        templates_must_be_relative,
        tsconfig_loaded: true,
        directory_index,
        packages: Default::default(),
        digest: Default::default(),
        config_dir,
        parent_digest: None,
        nested_configs: 0,
        modeled_configs: merged.modeled_configs,
        referenced_projects: Vec::new(),
        solution_only: false,
    };
    Ok((map, declares))
}

/// `tsc`'s `pathIsRelative`: `.`, `..`, or a `./` / `../` prefix.
fn is_dot_relative(template: &str) -> bool {
    matches!(template, "." | "..") || template.starts_with("./") || template.starts_with("../")
}

/// Workspace-relative, `/`-separated directory of `config_path`; empty for a
/// config directly in `root` (or outside it, which callers never load).
fn config_dir_relative(root: &Path, config_path: &Path) -> String {
    config_path
        .parent()
        .and_then(|dir| dir.strip_prefix(root).ok())
        .map(|rel| {
            rel.components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

// ── Build-output (outDir) mapping (#4551) ─────────────────────────────────────

/// Maps a relative import that names `tsc` build output back to its
/// TypeScript source, from the root `tsconfig.json`'s own
/// `compilerOptions.outDir` / `compilerOptions.rootDir` (#4551).
///
/// Tests of a compiled package often import the emitted file
/// (`import { f } from '../build/lib/x.js'`). The build tree is excluded
/// from the workspace walk, so without this mapping the import names an
/// unindexed module and a change to `lib/x.ts` reads as `no_static_path`.
///
/// Independent of `[typescript] resolve_tsconfig_paths`: that default-on, opt-out flag governs
/// non-relative `paths` aliases, whose templates can redirect arbitrary
/// specifiers. This mapping only rewrites a RELATIVE import whose target
/// does not exist, and only onto a source file that does exist, following
/// the fixed `tsc` emit layout (`<rootDir>/<p>.ts` → `<outDir>/<p>.js`).
///
/// `extends` is not followed, but the root file's own `outDir`/`rootDir`
/// are still read when it is present: `tsc` lets the extending file's own
/// `compilerOptions` override the extended ones, so the root file's values
/// are the effective ones. A root file without its own `outDir` and
/// `rootDir` yields no mapping: an inherited value is a miss, never a guess,
/// and an unset `rootDir` is inferred by `tsc` from its inputs, which this
/// reader does not model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TsOutDirMap {
    /// Workspace-relative `outDir`, `/`-separated, never empty.
    out_dir: String,
    /// Workspace-relative `rootDir`, `/`-separated; empty for the root.
    root_dir: String,
}

impl TsOutDirMap {
    /// Whether the workspace-relative path `joined` lies under `outDir`.
    pub(crate) fn contains(&self, joined: &str) -> bool {
        joined
            .strip_prefix(self.out_dir.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
    }

    /// Source module (workspace-relative, extension stripped) for the
    /// workspace-relative import target `joined`, when `joined` lies under
    /// `outDir` and exactly one TypeScript source file exists at the mapped
    /// path. The caller checks that nothing exists at `joined` itself.
    pub(crate) fn source_module_for(&self, root: &Path, joined: &str) -> Option<String> {
        let rest = joined
            .strip_prefix(self.out_dir.as_str())?
            .strip_prefix('/')?;
        let (stem, extensions): (&str, &[&str]) = if let Some(stem) = rest.strip_suffix(".mjs") {
            (stem, &["mts"])
        } else if let Some(stem) = rest.strip_suffix(".cjs") {
            (stem, &["cts"])
        } else if let Some(stem) = rest.strip_suffix(".jsx") {
            (stem, &["tsx"])
        } else if let Some(stem) = rest.strip_suffix(".js") {
            (stem, &["ts", "tsx"])
        } else if [".ts", ".tsx", ".mts", ".cts"]
            .iter()
            .any(|extension| rest.ends_with(extension))
        {
            // `tsc` never emits TypeScript sources into outDir.
            return None;
        } else {
            (rest, &["ts", "tsx"])
        };
        if stem.is_empty() || stem.ends_with('/') {
            return None;
        }
        let module = if self.root_dir.is_empty() {
            stem.to_string()
        } else {
            format!("{}/{stem}", self.root_dir)
        };
        let mut found = extensions.iter().filter(|extension| {
            std::fs::symlink_metadata(root.join(format!("{module}.{extension}")))
                .is_ok_and(|metadata| metadata.file_type().is_file())
        });
        // Exactly one source file: `x.ts` and `x.tsx` both present is
        // ambiguous and fails closed.
        match (found.next(), found.next()) {
            (Some(_), None) => Some(module),
            _ => None,
        }
    }
}

/// Load the root `tsconfig.json` outDir mapping. `None` (no mapping) when
/// the file is absent, unreadable, not JSONC, lacks its own
/// `compilerOptions.outDir` or `compilerOptions.rootDir`, or names an
/// absolute, root-escaping or
/// root-identical `outDir`, or an absolute or root-escaping `rootDir`.
pub(crate) fn load_out_dir_map(root: &Path) -> Option<TsOutDirMap> {
    let text = read_config_capped(&root.join("tsconfig.json")).ok()?;
    parse_out_dir_map(&text)
}

fn parse_out_dir_map(text: &str) -> Option<TsOutDirMap> {
    let normalized = strip_jsonc(text).ok()?;
    let value: serde_json::Value = serde_json::from_str(&normalized).ok()?;
    let options = value.get("compilerOptions")?;
    let out_dir = normalize_config_dir(options.get("outDir")?.as_str()?)?;
    if out_dir.is_empty() {
        return None;
    }
    // `rootDir` must be the root file's own: without it `tsc` infers the
    // common directory of the included inputs (or inherits one through
    // `extends`), and guessing the workspace root would map `dist/index.js`
    // onto a root-level `index.ts` that never produced it (#4800 review).
    let root_dir = normalize_config_dir(options.get("rootDir")?.as_str()?)?;
    Some(TsOutDirMap { out_dir, root_dir })
}

/// Workspace-relative, `/`-separated form of a config directory (`./build`,
/// `build/` → `build`; `.` → empty). `None` for an absolute, drive/UNC or
/// `..`-bearing path.
fn normalize_config_dir(raw: &str) -> Option<String> {
    let raw = raw.replace('\\', "/");
    if raw.starts_with('/') || raw.contains(':') || !is_safe_relative(Path::new(&raw)) {
        return None;
    }
    let parts: Vec<&str> = raw
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.contains(&"..") {
        return None;
    }
    Some(parts.join("/"))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Normalize JSONC (the dialect `tsc` accepts for tsconfig.json /
/// jsconfig.json) to strict JSON: remove `//` line comments, `/* */` block
/// comments, and trailing commas before `}` / `]`, all outside string
/// literals. String contents (including `//`, `/*`, `*/`, escaped quotes)
/// are copied verbatim. Removed comments become whitespace (line comments
/// keep their newline) so parser error positions stay meaningful.
///
/// Fails closed on an unterminated block comment or string literal; the
/// caller reports that as an unparseable config. Everything else is left
/// for the strict parser to accept or reject.
///
/// A leading UTF-8 byte-order mark is dropped (`tsc` accepts one, and editors
/// on Windows write it; #4638 review): the strict parser rejects it.
/// Shared JSONC normalization for alias loading and conservative gap scoping.
pub(super) fn strip_jsonc(text: &str) -> Result<String, &'static str> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = String::with_capacity(text.len());
    // Byte offset in `out` of a comma seen since the last significant
    // token; dropped if the next significant character closes a container.
    let mut pending_comma: Option<usize> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                pending_comma = None;
                out.push(ch);
                let mut escaped = false;
                let mut closed = false;
                for inner in chars.by_ref() {
                    out.push(inner);
                    if escaped {
                        escaped = false;
                    } else if inner == '\\' {
                        escaped = true;
                    } else if inner == '"' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err("unterminated string literal");
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                // Line comment: skip to (but keep) either line terminator.
                for inner in chars.by_ref() {
                    if matches!(inner, '\n' | '\r') {
                        out.push(inner);
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev_star = false;
                let mut closed = false;
                for inner in chars.by_ref() {
                    if prev_star && inner == '/' {
                        closed = true;
                        break;
                    }
                    prev_star = inner == '*';
                    if inner == '\n' {
                        out.push('\n');
                    }
                }
                if !closed {
                    return Err("unterminated block comment");
                }
                out.push(' ');
            }
            ',' => {
                pending_comma = Some(out.len());
                out.push(ch);
            }
            '}' | ']' => {
                if let Some(pos) = pending_comma.take() {
                    out.replace_range(pos..pos + 1, " ");
                }
                out.push(ch);
            }
            _ => {
                if !ch.is_whitespace() {
                    pending_comma = None;
                }
                out.push(ch);
            }
        }
    }
    Ok(out)
}

/// `true` only when every component of `p` is a normal name or `.` — no
/// `..`, no rooted component, no drive letter / UNC prefix. Joining such a
/// path onto the trusted workspace root cannot escape it.
fn is_safe_relative(p: &Path) -> bool {
    use std::path::Component;
    p.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Content fingerprint of every non-root `tsconfig.json`/`jsconfig.json` that
/// declares `compilerOptions.paths`, probed in the directories that hold or
/// contain `files` (workspace-relative source paths). `None` when there is
/// none. Bounded by the directory count; each config is read through the
/// capped reader.
pub(crate) fn nested_configs_fingerprint(root: &Path, files: &[PathBuf]) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let mut dirs: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    for file in files {
        let mut dir = file.parent();
        while let Some(current) = dir {
            if current.as_os_str().is_empty() || !dirs.insert(current.to_path_buf()) {
                break;
            }
            dir = current.parent();
        }
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut found = false;
    for dir in dirs {
        for name in ["tsconfig.json", "jsconfig.json"] {
            let path = root.join(&dir).join(name);
            if !path.is_file() || !config_file_owns_paths(root, &path) {
                continue;
            }
            found = true;
            dir.hash(&mut hasher);
            name.hash(&mut hasher);
            let Ok(text) = read_config_capped(&path) else {
                // Unreadable configs fail closed in the resolver; they must
                // still move the digest away from "no config".
                "unreadable".hash(&mut hasher);
                continue;
            };
            text.hash(&mut hasher);
            // A same-directory project the config references resolves its
            // importers, so its text and chain move the digest too.
            let siblings = parse_raw_tsconfig(&text)
                .ok()
                .and_then(|raw| raw.references)
                .and_then(|references| referenced_sibling_configs(root, &path, &references).ok())
                .unwrap_or_default();
            for sibling in siblings {
                sibling.hash(&mut hasher);
                match read_config_capped(&sibling) {
                    Ok(sibling_text) => {
                        sibling_text.hash(&mut hasher);
                        hash_merged_chain(root, &sibling, &sibling_text, &mut hasher);
                    }
                    Err(_read_err) => "unreadable".hash(&mut hasher),
                }
            }
            hash_merged_chain(root, &path, &text, &mut hasher);
        }
    }
    found.then(|| hasher.finish())
}

/// Hash the merged `extends` chain of one config: an edit to a followed base
/// changes resolution without touching the config's own text.
fn hash_merged_chain(
    root: &Path,
    path: &Path,
    text: &str,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    use std::hash::Hash;
    match merge_compiler_options(root, path, text, &mut Vec::new(), 0) {
        Ok(merged) => {
            merged.base_url.hash(hasher);
            merged.directory_index.hash(hasher);
            // The declaring config anchors the templates when no `baseUrl`
            // is in effect (#12760).
            let (declared_in, entries) = match merged.paths {
                Some(declared) => (Some(declared.config), declared.entries),
                None => (None, HashMap::new()),
            };
            declared_in.hash(hasher);
            let mut paths: Vec<_> = entries.into_iter().collect();
            paths.sort();
            paths.hash(hasher);
        }
        Err(_blocker) => "unmodeled".hash(hasher),
    }
}

/// Whether the paths-owning config(s) in `config_dir` may claim `specifier`
/// through their own `compilerOptions.paths` keys (#12754). Fail-closed: an
/// unreadable or unparseable config, a non-object `paths`, or `paths`
/// inherited only through `extends` counts as claiming, since its effective
/// keys are not observable here. `"paths": null` declares no keys.
pub(crate) fn package_config_claims(root: &Path, config_dir: &Path, specifier: &str) -> bool {
    for name in ["tsconfig.json", "jsconfig.json"] {
        let path = config_dir.join(name);
        if !path.is_file() || !config_file_owns_paths(root, &path) {
            continue;
        }
        let Some(value) = read_config_capped(&path)
            .ok()
            .and_then(|text| strip_jsonc(&text).ok())
            .and_then(|normalized| serde_json::from_str::<serde_json::Value>(&normalized).ok())
        else {
            return true;
        };
        match value
            .get("compilerOptions")
            .and_then(|options| options.get("paths"))
        {
            Some(serde_json::Value::Object(keys)) => {
                let claims = keys.keys().any(|key| match key.matches('*').count() {
                    0 => key == specifier,
                    1 => key.split_once('*').is_some_and(|(prefix, suffix)| {
                        match_glob(specifier, prefix, suffix).is_some()
                    }),
                    _ => false,
                });
                if claims {
                    return true;
                }
            }
            Some(serde_json::Value::Null) => {}
            _ => return true,
        }
    }
    false
}

/// `true` when the config at `path` determines its importers' `paths`, so the
/// root map must not stand in for them: it declares `compilerOptions.paths`,
/// inherits them through a relative in-root `extends` chain
/// ([`merge_compiler_options`]), or cannot be read or parsed (an unobservable
/// config is not evidence that the root map applies; the importer stays
/// unresolved instead). An `extends`-only config that parses but whose chain
/// declares no followable `paths` (package-name, symlinked or out-of-root
/// base, cycle) does NOT claim ownership, so today's root fallback is kept for it (#12754).
/// A config also owns `paths` when a same-directory project it `references` does.
pub(crate) fn config_file_owns_paths(root: &Path, path: &Path) -> bool {
    config_owns_paths(root, path, true)
}

fn config_owns_paths(root: &Path, path: &Path, follow_references: bool) -> bool {
    let Ok(text) = read_config_capped(path) else {
        return true;
    };
    let Ok(raw) = parse_raw_tsconfig(&text) else {
        return true;
    };
    // Presence of the key counts, even `"paths": null` or `{}`: such a config
    // still owns its importers, so the root map must not stand in for it.
    let declares = strip_jsonc(&text)
        .ok()
        .and_then(|normalized| serde_json::from_str::<serde_json::Value>(&normalized).ok())
        .is_some_and(|value| {
            value
                .get("compilerOptions")
                .is_some_and(|options| options.get("paths").is_some())
        });
    declares
        || (raw.extends.is_some()
            && merge_compiler_options(root, path, &text, &mut Vec::new(), 0)
                .is_ok_and(|merged| merged.paths.is_some()))
        // A same-directory project it references owns `paths` for the
        // importers here. Unreadable `references` keep the root fallback, as
        // an unfollowable `extends` does.
        || (follow_references
            && raw.references.as_ref().is_some_and(|references| {
                referenced_sibling_configs(root, path, references).is_ok_and(|siblings| {
                    siblings
                        .iter()
                        .any(|sibling| config_owns_paths(root, sibling, false))
                })
            }))
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
mod out_dir_tests;
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

    // ── cache_digest (#10943) ─────────────────────────────────────────────

    #[test]
    fn cache_digest_is_stable_across_clones_and_follows_the_package_table() {
        let root = temp_dir("digest-once");
        write(
            &root,
            "pkg/package.json",
            r#"{"name":"pkg-a","main":"src/a.ts"}"#,
        );
        write(&root, "pkg/src/a.ts", "export const a = 1;\n");
        let files = [PathBuf::from("pkg/src/a.ts")];
        let packages = super::super::workspace_packages::WorkspacePackages::discover(&root, &files);
        assert!(!packages.is_empty(), "fixture must discover its package");
        let computations = || DIGEST_COMPUTATIONS.with(|count| count.get());
        let map = TsAliasMap::workspace_packages_only(&root, packages.clone());
        let start = computations();
        let first = map.cache_digest();
        assert_eq!(computations(), start + 1, "first call renders the table");
        assert_eq!(map.cache_digest(), first);
        assert_eq!(
            map.clone().cache_digest(),
            first,
            "a clone keeps the digest"
        );
        assert_eq!(
            computations(),
            start + 1,
            "repeat and cloned calls must hit the memo, not re-render the table"
        );
        let empty = TsAliasMap::workspace_packages_only(&root, Default::default());
        let empty_digest = empty.cache_digest();
        assert_ne!(empty_digest, first, "fixture table must change the digest");
        let replaced = empty.with_workspace_packages(packages.clone());
        assert_eq!(
            replaced.cache_digest(),
            first,
            "replacing the package table must reset the memoized digest"
        );
        assert_eq!(
            computations(),
            start + 3,
            "the empty map and the replaced map each render once"
        );
        let _ = fs::remove_dir_all(&root);
    }

    // ── load_alias_map ────────────────────────────────────────────────────

    #[test]
    fn nested_fingerprint_follows_extends_base_and_unreadable_configs() -> Result<(), String> {
        let root = temp_dir("nested-fingerprint");
        let files = vec![PathBuf::from("packages/a/src/x.ts")];
        assert_eq!(nested_configs_fingerprint(&root, &files), None);
        write(
            &root,
            "packages/a/tsconfig.json",
            r#"{"extends":"./tsconfig.base.json"}"#,
        );
        write(
            &root,
            "packages/a/tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        let first = nested_configs_fingerprint(&root, &files).ok_or("base paths own")?;
        write(
            &root,
            "packages/a/tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["lib/*"]}}}"#,
        );
        let second = nested_configs_fingerprint(&root, &files).ok_or("base paths own")?;
        assert_ne!(
            first, second,
            "an extends-base edit must move the fingerprint"
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn returns_none_when_no_config_file_present() {
        let root = temp_dir("no-config");
        assert!(load_alias_map(&root).is_none());
    }

    #[test]
    fn config_missing_advice_discloses_discovery_scope() {
        // #8715/#12754: a monorepo with only package-level tsconfigs that
        // cannot be used hits ConfigMissing; the advice must name the root
        // discovery scope and point at the importing package's config.
        let (why, recovery) = TsAliasMapLoadGap::ConfigMissing.parts();
        assert!(
            why.contains("only those two workspace-root configs"),
            "why must name the discovery scope: {why}"
        );
        assert!(
            recovery.contains("importing package's tsconfig.json"),
            "recovery must name the scope boundary: {recovery}"
        );
        let (why, recovery) = TsAliasMapLoadGap::ExtendsUnsupported {
            config: "tsconfig.json",
        }
        .parts();
        assert!(
            recovery.contains(
                "extends chains are followed only for relative bases inside the workspace"
            ),
            "extends recovery must name the scope boundary: {recovery}"
        );
        assert!(
            why.contains("workspace root"),
            "extends why must name the workspace-root config: {why}"
        );
    }

    #[test]
    fn missing_base_is_ignored_only_when_the_config_sets_base_url_and_paths() {
        // #11204: a base that cannot be read cannot change alias resolution
        // when the extending config declares both keys itself.
        let root = temp_dir("extends");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./base","compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        assert!(load_alias_map(&root).is_some());
        let root = temp_dir("extends-package-name");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"@tsconfig/node20/tsconfig.json","compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        assert!(load_alias_map(&root).is_some());
        // Without its own baseUrl the inherited value is unknown: fail closed.
        let root = temp_dir("extends-missing-base-url");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./base","compilerOptions":{"paths":{"@/*":["src/*"]}}}"#,
        );
        assert!(load_alias_map(&root).is_none());
    }

    #[test]
    fn extends_chain_resolves_paths_from_base_config() -> Result<(), String> {
        let root = temp_dir("extends-chain");
        write(
            &root,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json"}"#,
        );
        write(&root, "src/owner.ts", "export function owner() {}");
        let map = load_alias_map(&root).ok_or("extends chain should load")?;
        let resolved = map.resolve("@/owner").ok_or("alias should resolve")?;
        assert_eq!(
            resolved.to_string_lossy().replace('\\', "/"),
            "src/owner.ts"
        );
        Ok(())
    }

    #[test]
    fn extends_child_paths_override_base_paths() -> Result<(), String> {
        let root = temp_dir("extends-override");
        write(
            &root,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/owner":["src/base-owner"]}}}"#,
        );
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"paths":{"@/owner":["src/child-owner"]}}}"#,
        );
        write(&root, "src/base-owner.ts", "export const v = 1;");
        write(&root, "src/child-owner.ts", "export const v = 2;");
        let map = load_alias_map(&root).ok_or("extends chain should load")?;
        let resolved = map.resolve("@/owner").ok_or("alias should resolve")?;
        assert_eq!(
            resolved.to_string_lossy().replace('\\', "/"),
            "src/child-owner.ts"
        );
        Ok(())
    }

    #[test]
    fn extends_child_empty_paths_replaces_base_paths() -> Result<(), String> {
        // `tsc` replaces the inherited `paths` object whole when the
        // extending config declares `paths`, even when it declares it
        // empty: base aliases the child does not redeclare are gone.
        let root = temp_dir("extends-empty-paths");
        write(
            &root,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/owner":["src/base-owner"]}}}"#,
        );
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"paths":{}}}"#,
        );
        write(&root, "src/base-owner.ts", "export const v = 1;");
        let map = load_alias_map(&root).ok_or("extends chain should load")?;
        assert!(map.resolve("@/owner").is_none());
        Ok(())
    }

    #[test]
    fn extends_child_paths_replace_base_paths_partially() -> Result<(), String> {
        // A child that redeclares one of two base aliases keeps its own and
        // drops the other: whole-object replacement, not a per-key merge.
        let root = temp_dir("extends-partial-paths");
        write(
            &root,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/owner":["src/base-owner"],"@/other":["src/other"]}}}"#,
        );
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"paths":{"@/owner":["src/child-owner"]}}}"#,
        );
        write(&root, "src/base-owner.ts", "export const v = 1;");
        write(&root, "src/child-owner.ts", "export const v = 2;");
        write(&root, "src/other.ts", "export const v = 3;");
        let map = load_alias_map(&root).ok_or("extends chain should load")?;
        let resolved = map.resolve("@/owner").ok_or("own alias should resolve")?;
        assert_eq!(
            resolved.to_string_lossy().replace('\\', "/"),
            "src/child-owner.ts"
        );
        assert!(
            map.resolve("@/other").is_none(),
            "base alias the child does not redeclare must be gone"
        );
        Ok(())
    }

    #[test]
    fn extends_chain_at_max_hops_loads_but_one_more_fails_closed() -> Result<(), String> {
        // Pins MAX_EXTENDS_DEPTH exactly: root plus 8 bases (deepest at
        // depth 8) loads; root plus 9 bases (deepest at depth 9) fails
        // closed with no map.
        for (label, depth) in [("extends-max-loads", 8), ("extends-max-plus-one", 9)] {
            let root = temp_dir(label);
            for i in (0..depth).rev() {
                let body = if i + 1 == depth {
                    r#"{"compilerOptions":{"baseUrl":".","paths":{}}}"#.to_string()
                } else {
                    format!("{{\"extends\":\"./c{}.json\"}}", i + 1)
                };
                write(&root, &format!("c{i}.json"), &body);
            }
            write(&root, "tsconfig.json", r#"{"extends":"./c0.json"}"#);
            if depth == 8 {
                assert!(
                    load_alias_map(&root).is_some(),
                    "an 8-deep chain is within the bound and must load"
                );
            } else {
                assert!(
                    load_alias_map(&root).is_none(),
                    "a 9-deep chain exceeds the bound and must fail closed"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn extends_cycle_fails_closed() {
        let root = temp_dir("extends-cycle");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json"}"#,
        );
        write(
            &root,
            "tsconfig.base.json",
            r#"{"extends":"./tsconfig.json","compilerOptions":{"baseUrl":".","paths":{}}}"#,
        );
        assert!(load_alias_map(&root).is_none());
    }

    #[test]
    fn inherited_base_url_is_anchored_at_the_declaring_config() -> Result<(), String> {
        // #11204: `baseUrl` resolves against the config that declares it, so a
        // base in `config/` maps `@/*` under `config/src`, never `src`.
        let root = temp_dir("extends-subdir");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./config/tsconfig.base.json"}"#,
        );
        write(
            &root,
            "config/tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "config/src/x.ts", "export const x = 1;");
        write(&root, "src/x.ts", "export const x = 2;");
        let map = load_alias_map(&root).ok_or("subdirectory base must load")?;
        assert_eq!(map.resolve("@/x"), Some(PathBuf::from("config/src/x.ts")));
        // A child that sets its own baseUrl rebases the inherited paths there.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./config/tsconfig.base.json","compilerOptions":{"baseUrl":"."}}"#,
        );
        let map = load_alias_map(&root).ok_or("child baseUrl must load")?;
        assert_eq!(map.resolve("@/x"), Some(PathBuf::from("src/x.ts")));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn extends_target_outside_the_root_or_symlinked_fails_closed() {
        let outer = temp_dir("extends-outside");
        let root = outer.join("repo");
        write(
            &outer,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"../tsconfig.base.json"}"#,
        );
        assert!(load_alias_map(&root).is_none());
        // A base whose baseUrl climbs above the root keeps the named
        // absolute/escaping limitation instead of resolving outside.
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":"..","paths":{"@/*":["src/*"]}}}"#,
        );
        assert!(load_alias_map(&root).is_some_and(|map| map.base_url_absolute()));
        #[cfg(unix)]
        {
            let linked = temp_dir("extends-symlink");
            write(
                &linked,
                "real/tsconfig.base.json",
                r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
            );
            assert!(
                std::os::unix::fs::symlink(
                    linked.join("real/tsconfig.base.json"),
                    linked.join("base.json"),
                )
                .is_ok(),
                "fixture symlink must be created"
            );
            write(&linked, "tsconfig.json", r#"{"extends":"./base.json"}"#);
            assert!(load_alias_map(&linked).is_none());
            // Control: the same base reached through its real path loads.
            write(
                &linked,
                "tsconfig.json",
                r#"{"extends":"./real/tsconfig.base.json"}"#,
            );
            assert!(load_alias_map(&linked).is_some());
            // A symlinked DIRECTORY in the middle of the path is refused too.
            assert!(
                std::os::unix::fs::symlink(linked.join("real"), linked.join("shared")).is_ok(),
                "fixture directory symlink must be created"
            );
            write(
                &linked,
                "tsconfig.json",
                r#"{"extends":"./shared/tsconfig.base.json"}"#,
            );
            assert!(load_alias_map(&linked).is_none());
            // An extensionless target whose preferred `<target>.json` is a
            // symlink must not fall back to the bare `<target>` file.
            write(
                &linked,
                "fallback",
                r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["wrong/*"]}}}"#,
            );
            assert!(
                std::os::unix::fs::symlink(
                    linked.join("real/tsconfig.base.json"),
                    linked.join("fallback.json"),
                )
                .is_ok(),
                "fixture symlink must be created"
            );
            write(&linked, "tsconfig.json", r#"{"extends":"./fallback"}"#);
            assert!(load_alias_map(&linked).is_none());
            let _ = std::fs::remove_dir_all(&linked);
        }
        let _ = std::fs::remove_dir_all(&outer);
    }

    #[test]
    fn explicit_null_clears_an_inherited_option() {
        let root = temp_dir("extends-null");
        write(
            &root,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "src/x.ts", "export const x = 1;\n");
        // Control: the inherited map resolves.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json"}"#,
        );
        let map = load_alias_map(&root);
        assert_eq!(
            map.as_ref().and_then(|map| map.resolve("@/x")),
            Some(PathBuf::from("src/x.ts"))
        );
        // `paths: null` clears the inherited paths: nothing resolves.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"baseUrl":".","paths":null}}"#,
        );
        let map = load_alias_map(&root);
        assert!(
            map.as_ref().and_then(|map| map.resolve("@/x")).is_none(),
            "a null paths must not keep the inherited aliases"
        );
        // `baseUrl: null` clears the inherited anchor. The inherited `src/*`
        // is then a non-relative template with no `baseUrl`, which `tsc`
        // rejects (#12760): the key is kept and nothing resolves through it.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"baseUrl":null}}"#,
        );
        let map = load_alias_map(&root);
        assert!(map.as_ref().is_some_and(|map| map.paths_key_matches("@/x")));
        assert!(map.as_ref().and_then(|map| map.resolve("@/x")).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cycle_below_a_self_sufficient_config_still_fails_closed() {
        let root = temp_dir("extends-nested-cycle");
        write(&root, "b.json", r#"{"extends":"./tsconfig.json"}"#);
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./b.json","compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        assert!(load_alias_map(&root).is_none());
        // Control: a missing (not cyclic) base is ignored for the same config.
        write(&root, "b.json", r#"{"extends":"./missing.json"}"#);
        assert!(load_alias_map(&root).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn extensionless_target_and_relative_root_follow_the_chain() {
        let root = temp_dir("extends-extensionless");
        write(
            &root,
            "config/tsconfig.base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        write(&root, "config/src/x.ts", "export const x = 1;\n");
        write(
            &root,
            "packages/a/tsconfig.json",
            r#"{"extends":"../../config/tsconfig.base"}"#,
        );
        let target = extends_target_path(
            &root,
            &root.join("packages/a"),
            "../../config/tsconfig.base",
        );
        assert_eq!(target, Some(root.join("config/tsconfig.base.json")));
        // A relative root (`.`-style) normalizes to the empty path; a chain
        // climbing back to it stays inside, one more `..` escapes.
        assert_eq!(
            normalize_lexically(Path::new("./packages/a/../..")),
            Some(PathBuf::new())
        );
        assert_eq!(normalize_lexically(Path::new("./packages/../..")), None);
        let _ = std::fs::remove_dir_all(&root);
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

    /// The Vite template layout: a solution-style root that references
    /// `tsconfig.app.json` and `tsconfig.node.json` beside it.
    fn solution_root(label: &str, app: &str) -> PathBuf {
        let root = temp_dir(label);
        write(
            &root,
            "tsconfig.json",
            r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{"path":"./tsconfig.node.json"}]}"#,
        );
        write(&root, "tsconfig.app.json", app);
        write(
            &root,
            "tsconfig.node.json",
            r#"{"compilerOptions":{"strict":true},"include":["vite.config.ts"]}"#,
        );
        write(&root, "src/owner.ts", "export const owner = 1;\n");
        root
    }

    #[test]
    fn solution_style_root_resolves_through_its_referenced_sibling() -> Result<(), String> {
        for (label, app) in [
            (
                "references-no-base-url",
                r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}},"include":["src"]}"#,
            ),
            (
                "references-base-url",
                r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
            ),
        ] {
            let root = solution_root(label, app);
            let map = load_alias_map(&root).ok_or("the solution config must load")?;
            assert_eq!(
                resolved(&map, "@/owner").as_deref(),
                Some("src/owner.ts"),
                "{label}"
            );
            assert_eq!(resolved(&map, "@/missing"), None, "{label}");
            assert!(map.paths_key_matches("@/owner") && !map.is_empty());
            for config in ["tsconfig.json", "tsconfig.app.json", "tsconfig.node.json"] {
                assert!(map.models_config(&root.join(config)), "{label}: {config}");
            }
            assert!(!map.models_config(&root.join("tsconfig.spec.json")));
            assert!(config_file_owns_paths(&root, &root.join("tsconfig.json")));
            let _ = std::fs::remove_dir_all(&root);
        }
        Ok(())
    }

    #[test]
    fn solution_style_root_own_paths_are_not_a_project() -> Result<(), String> {
        // `"files": []` compiles nothing, so the root's own `paths` govern no
        // importer: only the referenced project answers.
        let root = solution_root(
            "references-solution-own-paths",
            r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        write(&root, "other/owner.ts", "export const decoy = 1;\n");
        write(
            &root,
            "tsconfig.json",
            r#"{"files":[],"compilerOptions":{"paths":{"@/*":["./other/*"],"root-only":["./other/owner.ts"]}},"references":[{"path":"./tsconfig.app.json"}]}"#,
        );
        let map = load_alias_map(&root).ok_or("the solution config must load")?;
        assert_eq!(resolved(&map, "@/owner").as_deref(), Some("src/owner.ts"));
        assert_eq!(resolved(&map, "root-only"), None);
        assert!(!map.paths_key_matches("root-only"));

        // Without `"files": []` the root compiles files too, so it is a
        // project whose different answer must be respected.
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./other/*"]}},"references":[{"path":"./tsconfig.app.json"}]}"#,
        );
        let map = load_alias_map(&root).ok_or("the referencing config must load")?;
        assert_eq!(resolved(&map, "@/owner"), None);
        assert!(map.paths_key_matches("@/owner"));
        // An `extends` could inherit `include`, so `"files": []` alone does
        // not make the config solution-style.
        write(&root, "base.json", r#"{"include":["other"]}"#);
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./base.json","files":[],"compilerOptions":{"paths":{"@/*":["./other/*"]}},"references":[{"path":"./tsconfig.app.json"}]}"#,
        );
        let map = load_alias_map(&root).ok_or("the extending config must load")?;
        assert_eq!(resolved(&map, "@/owner"), None);
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn referenced_siblings_must_agree_on_a_specifier() -> Result<(), String> {
        let root = solution_root(
            "references-disagree",
            r#"{"compilerOptions":{"paths":{"@/*":["./src/*"],"app-only/*":["./src/*"]}}}"#,
        );
        write(&root, "spec/owner.ts", "export const decoy = 1;\n");
        write(
            &root,
            "tsconfig.json",
            r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{"path":"./tsconfig.spec.json"}]}"#,
        );
        // Same key, different file: which one applies depends on project
        // membership, which is not modeled.
        write(
            &root,
            "tsconfig.spec.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./spec/*"]}}}"#,
        );
        let map = load_alias_map(&root).ok_or("the solution config must load")?;
        assert_eq!(resolved(&map, "@/owner"), None);
        assert_eq!(
            map.unresolve_cause_for("@/owner"),
            TsAliasUnresolveCause::CandidateUnresolved
        );
        // A key only one project declares is still answered by that project.
        assert_eq!(
            resolved(&map, "app-only/owner").as_deref(),
            Some("src/owner.ts")
        );
        // One claiming project that cannot resolve leaves it unresolved.
        write(
            &root,
            "tsconfig.spec.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./nowhere/*"]}}}"#,
        );
        let map = load_alias_map(&root).ok_or("the solution config must load")?;
        assert_eq!(resolved(&map, "@/owner"), None);
        // Agreement, here through a shared base both siblings extend.
        write(
            &root,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        for sibling in ["tsconfig.app.json", "tsconfig.spec.json"] {
            write(&root, sibling, r#"{"extends":"./tsconfig.base.json"}"#);
        }
        let map = load_alias_map(&root).ok_or("the solution config must load")?;
        assert_eq!(resolved(&map, "@/owner").as_deref(), Some("src/owner.ts"));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn siblings_extending_the_referencing_root_inherit_its_paths() -> Result<(), String> {
        // The Angular layout: the root declares `paths` and references
        // projects that extend it. A base's `references` are not inherited.
        let root = temp_dir("references-angular");
        write(
            &root,
            "tsconfig.json",
            r#"{"files":[],"compilerOptions":{"baseUrl":"./","paths":{"@app/*":["src/app/*"]}},"references":[{"path":"./tsconfig.app.json"},{"path":"./tsconfig.spec.json"}]}"#,
        );
        for sibling in ["tsconfig.app.json", "tsconfig.spec.json"] {
            write(&root, sibling, r#"{"extends":"./tsconfig.json"}"#);
        }
        write(&root, "src/app/owner.ts", "export const owner = 1;\n");
        let map = load_alias_map(&root).ok_or("the Angular layout must load")?;
        assert_eq!(
            resolved(&map, "@app/owner").as_deref(),
            Some("src/app/owner.ts")
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn an_unclaiming_project_with_a_base_url_can_still_answer() -> Result<(), String> {
        // The tools project has no key for `lib/*`, but its `baseUrl` makes
        // `tsc` look for `tools/lib/owner` first. Membership is not modeled,
        // so the app's alias is credited only when nothing sits there.
        let app = r#"{"compilerOptions":{"paths":{"lib/*":["./src/*"]}}}"#;
        let root = solution_root("references-base-url-competes", app);
        write(
            &root,
            "tsconfig.node.json",
            r#"{"compilerOptions":{"baseUrl":"./tools"},"include":["tools"]}"#,
        );
        let map = load_alias_map(&root).ok_or("the solution-style root must load")?;
        assert_eq!(resolved(&map, "lib/owner").as_deref(), Some("src/owner.ts"));
        write(&root, "tools/lib/owner.ts", "export const owner = 2;\n");
        assert_eq!(resolved(&map, "lib/owner"), None);
        assert!(map.paths_key_matches("lib/owner"));
        // A declaration file there is just as much an answer.
        let _ = fs::remove_file(root.join("tools/lib/owner.ts"));
        assert_eq!(resolved(&map, "lib/owner").as_deref(), Some("src/owner.ts"));
        write(&root, "tools/lib/owner.d.ts", "export {};\n");
        assert_eq!(resolved(&map, "lib/owner"), None);
        assert!(map.base_url_in_effect());
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn unreadable_or_nested_references_fail_closed() {
        let app = r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#;
        for (label, root_config, extra) in [
            (
                "missing-target",
                r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{"path":"./tsconfig.gone.json"}]}"#,
                None,
            ),
            (
                "outside-root",
                r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{"path":"../outside/tsconfig.json"}]}"#,
                None,
            ),
            (
                "not-an-array",
                r#"{"files":[],"references":{"path":"./tsconfig.app.json"}}"#,
                None,
            ),
            (
                "entry-without-path",
                r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{}]}"#,
                None,
            ),
            (
                "self-reference",
                r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{"path":"./tsconfig.json"}]}"#,
                None,
            ),
            (
                "sibling-with-references",
                r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{"path":"./tsconfig.lib.json"}]}"#,
                Some((
                    "tsconfig.lib.json",
                    r#"{"references":[{"path":"./tsconfig.app.json"}]}"#,
                )),
            ),
            (
                "sibling-unparseable",
                r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{"path":"./tsconfig.lib.json"}]}"#,
                Some(("tsconfig.lib.json", "{ not json")),
            ),
            (
                "sibling-package-extends",
                r#"{"files":[],"references":[{"path":"./tsconfig.app.json"},{"path":"./tsconfig.lib.json"}]}"#,
                Some(("tsconfig.lib.json", r#"{"extends":"@scope/tsconfig"}"#)),
            ),
        ] {
            let root = solution_root(&format!("references-closed-{label}"), app);
            // Control: the same root loads before the reference is broken,
            // so a refusal below is about the reference and not the setup.
            assert!(load_alias_map(&root).is_some(), "{label}: control");
            write(&root, "tsconfig.json", root_config);
            assert_eq!(
                fs::read_to_string(root.join("tsconfig.json"))
                    .ok()
                    .as_deref(),
                Some(root_config),
                "{label}: fixture"
            );
            if let Some((name, body)) = extra {
                write(&root, name, body);
                assert_eq!(
                    fs::read_to_string(root.join(name)).ok().as_deref(),
                    Some(body),
                    "{label}: fixture"
                );
            }
            assert!(load_alias_map(&root).is_none(), "{label}");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn references_to_other_directories_leave_the_config_its_own_paths() -> Result<(), String> {
        let root = temp_dir("references-other-dirs");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}},"references":[{"path":"./packages/lib"},{"path":"packages/tool/tsconfig.build.json"}]}"#,
        );
        write(
            &root,
            "packages/lib/tsconfig.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./lib-src/*"]}}}"#,
        );
        write(&root, "packages/tool/tsconfig.build.json", "{}");
        write(&root, "src/owner.ts", "export const owner = 1;\n");
        let map = load_alias_map(&root).ok_or("in-root references must not refuse the config")?;
        assert_eq!(resolved(&map, "@/owner").as_deref(), Some("src/owner.ts"));
        assert!(!map.models_config(&root.join("packages/lib/tsconfig.json")));
        // A solution-style root with no same-directory project declares
        // nothing an importer here could use.
        write(
            &root,
            "tsconfig.json",
            r#"{"files":[],"references":[{"path":"./packages/lib"}]}"#,
        );
        assert!(load_alias_map(&root).is_none());
        // `references` on an `extends` base are never inherited.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json"}"#,
        );
        write(
            &root,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}},"references":[{"path":"./tsconfig.gone.json"}]}"#,
        );
        let map = load_alias_map(&root).ok_or("a base's references must be ignored")?;
        assert_eq!(resolved(&map, "@/owner").as_deref(), Some("src/owner.ts"));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn nested_fingerprint_and_digest_follow_a_referenced_sibling() -> Result<(), String> {
        let root = temp_dir("references-fingerprint");
        let files = vec![PathBuf::from("packages/a/src/owner.ts")];
        write(
            &root,
            "packages/a/tsconfig.json",
            r#"{"files":[],"references":[{"path":"./tsconfig.app.json"}]}"#,
        );
        write(
            &root,
            "packages/a/tsconfig.app.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        write(
            &root,
            "packages/a/src/owner.ts",
            "export const owner = 1;\n",
        );
        write(
            &root,
            "packages/a/lib/owner.ts",
            "export const other = 1;\n",
        );
        let empty = TsAliasMap::workspace_packages_only(&root, Default::default());
        let nested = empty
            .nested_for(&root.join("packages/a"))
            .map_err(|()| "the package solution config must compile")?;
        assert_eq!(
            resolved(&nested, "@/owner").as_deref(),
            Some("packages/a/src/owner.ts")
        );
        let first = nested_configs_fingerprint(&root, &files).ok_or("the sibling owns paths")?;
        write(
            &root,
            "packages/a/tsconfig.app.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./lib/*"]}}}"#,
        );
        let second = nested_configs_fingerprint(&root, &files).ok_or("the sibling owns paths")?;
        assert_ne!(first, second, "a sibling edit must move the fingerprint");
        let edited = empty
            .nested_for(&root.join("packages/a"))
            .map_err(|()| "the edited package config must compile")?;
        assert_eq!(
            resolved(&edited, "@/owner").as_deref(),
            Some("packages/a/lib/owner.ts")
        );
        assert_ne!(nested.cache_digest(), edited.cache_digest());
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn returns_none_when_neither_base_url_nor_paths_is_set() {
        let root = temp_dir("no-base-url-no-paths");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true}}"#,
        );
        assert!(load_alias_map(&root).is_none());
        // `"paths": null` clears, so nothing is declared either.
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"paths":null}}"#,
        );
        assert!(load_alias_map(&root).is_none());
    }

    fn resolved(map: &TsAliasMap, specifier: &str) -> Option<String> {
        map.resolve(specifier)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
    }

    #[test]
    fn relative_paths_resolve_without_base_url() -> Result<(), String> {
        // #12760: the create-next-app default. `tsc` anchors the template at
        // the directory of the config that declares `paths`.
        for config in ["tsconfig.json", "jsconfig.json"] {
            let root = temp_dir(&format!("no-base-url-{config}"));
            write(
                &root,
                config,
                r#"{"compilerOptions":{"paths":{"@/*":["./src/*"],"@owner":["./src/owner.ts"],"@win/*":[".\\src\\*"]}}}"#,
            );
            write(&root, "src/owner.ts", "export const owner = 1;\n");
            // A same-named file at a different anchor must not be chosen.
            write(&root, "owner.ts", "export const decoy = 1;\n");
            let map = load_alias_map(&root).ok_or("a no-baseUrl config must load")?;
            assert_eq!(
                resolved(&map, "@/owner").as_deref(),
                Some("src/owner.ts"),
                "{config}"
            );
            assert_eq!(
                resolved(&map, "@owner").as_deref(),
                Some("src/owner.ts"),
                "{config}"
            );
            // `tsc` reads a backslash-separated target as the same path.
            assert_eq!(
                resolved(&map, "@win/owner").as_deref(),
                Some("src/owner.ts"),
                "{config}"
            );
            assert_eq!(resolved(&map, "@/missing"), None, "{config}");
            assert!(map.models_config(&root.join(config)));
            let _ = std::fs::remove_dir_all(&root);
        }
        Ok(())
    }

    #[test]
    fn non_relative_template_without_base_url_blocks_its_key() -> Result<(), String> {
        // `tsc` rejects `src/*` here (TS5090). The key still wins selection,
        // so neither the broader alias nor a workspace package may take it.
        let root = temp_dir("no-base-url-non-relative");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"paths":{"@/lib/*":["src/*"],"@/*":["./fallback/*"],"plain":["src/owner.ts"]}}}"#,
        );
        write(&root, "src/owner.ts", "export const owner = 1;\n");
        write(&root, "fallback/lib/owner.ts", "export const decoy = 1;\n");
        write(&root, "fallback/other.ts", "export const other = 1;\n");
        let map = load_alias_map(&root).ok_or("the config must still load")?;
        assert_eq!(resolved(&map, "@/lib/owner"), None);
        assert_eq!(resolved(&map, "plain"), None);
        assert!(map.paths_key_matches("@/lib/owner"));
        assert_eq!(
            map.unresolve_cause_for("@/lib/owner"),
            TsAliasUnresolveCause::CandidateUnresolved
        );
        // The relative sibling key is unaffected.
        assert_eq!(
            resolved(&map, "@/other").as_deref(),
            Some("fallback/other.ts")
        );
        // The same template is valid once a `baseUrl` anchors it.
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/lib/*":["src/*"]}}}"#,
        );
        let map = load_alias_map(&root).ok_or("the baseUrl config must load")?;
        assert_eq!(
            resolved(&map, "@/lib/owner").as_deref(),
            Some("src/owner.ts")
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn inherited_paths_without_base_url_anchor_at_the_declaring_config() -> Result<(), String> {
        // Same-directory base, then a base in `config/`: the template is
        // relative to the file that declares `paths`, not the extending one.
        let root = temp_dir("no-base-url-inherited");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json"}"#,
        );
        write(
            &root,
            "tsconfig.base.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        write(&root, "src/owner.ts", "export const owner = 1;\n");
        write(&root, "config/src/owner.ts", "export const decoy = 1;\n");
        let map = load_alias_map(&root).ok_or("same-directory inherited paths must load")?;
        assert_eq!(resolved(&map, "@/owner").as_deref(), Some("src/owner.ts"));
        assert!(map.models_config(&root.join("tsconfig.json")));
        assert!(map.models_config(&root.join("tsconfig.base.json")));

        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./config/tsconfig.base.json"}"#,
        );
        write(
            &root,
            "config/tsconfig.base.json",
            r#"{"compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        let map = load_alias_map(&root).ok_or("cross-directory inherited paths must load")?;
        assert_eq!(
            resolved(&map, "@/owner").as_deref(),
            Some("config/src/owner.ts")
        );
        // The same-directory base is no longer part of the chain.
        assert!(!map.models_config(&root.join("tsconfig.base.json")));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn base_url_from_either_side_of_the_chain_still_anchors_paths() -> Result<(), String> {
        let root = temp_dir("no-base-url-mixed-chain");
        write(&root, "src/owner.ts", "export const owner = 1;\n");
        write(&root, "config/src/owner.ts", "export const decoy = 1;\n");
        // Inherited `baseUrl` (declared in `config/`), own non-relative paths.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./config/base.json","compilerOptions":{"paths":{"@/*":["src/*"]}}}"#,
        );
        write(
            &root,
            "config/base.json",
            r#"{"compilerOptions":{"baseUrl":"."}}"#,
        );
        let map = load_alias_map(&root).ok_or("inherited baseUrl must load")?;
        assert_eq!(
            resolved(&map, "@/owner").as_deref(),
            Some("config/src/owner.ts")
        );
        // Own `baseUrl`, inherited paths: the extending config's anchor wins.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./config/base.json","compilerOptions":{"baseUrl":"."}}"#,
        );
        write(
            &root,
            "config/base.json",
            r#"{"compilerOptions":{"paths":{"@/*":["src/*"]}}}"#,
        );
        let map = load_alias_map(&root).ok_or("own baseUrl must load")?;
        assert_eq!(resolved(&map, "@/owner").as_deref(), Some("src/owner.ts"));
        // An explicit `null` clears the inherited `baseUrl`; the inherited
        // non-relative template then has no anchor `tsc` accepts.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./config/base.json","compilerOptions":{"baseUrl":null}}"#,
        );
        write(
            &root,
            "config/base.json",
            r#"{"compilerOptions":{"baseUrl":"..","paths":{"@/*":["src/*"]}}}"#,
        );
        let map = load_alias_map(&root).ok_or("cleared baseUrl must still load")?;
        assert_eq!(resolved(&map, "@/owner"), None);
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn unfollowable_base_is_not_ignored_for_paths_without_base_url() {
        // The package base could supply the `baseUrl` that anchors these
        // templates, so own `paths` alone is not self-sufficient.
        let root = temp_dir("no-base-url-package-base");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"@scope/tsconfig","compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        write(&root, "src/owner.ts", "export const owner = 1;\n");
        assert!(load_alias_map(&root).is_none());
    }

    #[test]
    fn a_map_models_only_the_configs_it_was_compiled_from() -> Result<(), String> {
        let root = temp_dir("modeled-configs");
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        write(
            &root,
            "tsconfig.base.json",
            r#"{"extends":"./tsconfig.options.json","compilerOptions":{"paths":{"old/*":["./old/*"]}}}"#,
        );
        write(
            &root,
            "tsconfig.options.json",
            r#"{"compilerOptions":{"strict":true}}"#,
        );
        for sibling in ["tsconfig.spec.json", "jsconfig.json"] {
            write(
                &root,
                sibling,
                r#"{"compilerOptions":{"paths":{"spec/*":["./spec/*"]}}}"#,
            );
        }
        let map = load_alias_map(&root).ok_or("the chain must load")?;
        assert!(map.models_config(&root.join("tsconfig.json")));
        assert!(map.models_config(&root.join("./tsconfig.options.json")));
        assert!(
            !map.models_config(&root.join("tsconfig.base.json")),
            "a base whose paths were replaced is not stated by the map"
        );
        assert!(!map.models_config(&root.join("tsconfig.spec.json")));
        assert!(!map.models_config(&root.join("jsconfig.json")));
        let packages_only = TsAliasMap::workspace_packages_only(&root, Default::default());
        assert!(!packages_only.models_config(&root.join("tsconfig.json")));
        // A config between the selected one and the replaced base inherits
        // the replaced `paths` without declaring any: not modeled either.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.mid.json","compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        write(
            &root,
            "tsconfig.mid.json",
            r#"{"extends":"./tsconfig.base.json","include":["test"]}"#,
        );
        let map = load_alias_map(&root).ok_or("the longer chain must load")?;
        assert!(map.models_config(&root.join("tsconfig.json")));
        assert!(map.models_config(&root.join("tsconfig.options.json")));
        assert!(!map.models_config(&root.join("tsconfig.mid.json")));
        assert!(!map.models_config(&root.join("tsconfig.base.json")));
        // Once the selected config inherits those `paths` instead, the
        // whole chain states the same aliases.
        write(
            &root,
            "tsconfig.json",
            r#"{"extends":"./tsconfig.mid.json"}"#,
        );
        let map = load_alias_map(&root).ok_or("the inheriting chain must load")?;
        assert!(map.models_config(&root.join("tsconfig.mid.json")));
        assert!(map.models_config(&root.join("tsconfig.base.json")));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
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

    /// A root config whose only alias is `@/*` with `value` as its target
    /// list, anchored by `baseUrl: "."`.
    fn fallback_map(root: &Path, value: &str) -> Result<TsAliasMap, String> {
        write(
            root,
            "tsconfig.json",
            &format!(r#"{{"compilerOptions":{{"baseUrl":".","paths":{{"@/*":{value}}}}}}}"#),
        );
        load_alias_map(root).ok_or_else(|| "the fallback config must load".to_string())
    }

    /// The monorepo idiom `"@acme/*": ["packages/*/src"]` names a directory;
    /// the compiler loads its `index` file. Pinned for a pattern key and an
    /// exact key, with a sibling package as the wrong answer.
    #[test]
    fn directory_target_resolves_to_its_index_file() -> Result<(), String> {
        let root = temp_dir("directory-index");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{
                "@acme/*":["packages/*/src"],
                "core":["packages/core/src"]
            }}}"#,
        );
        write(&root, "packages/core/src/index.ts", "export {}");
        write(&root, "packages/ui/src/index.tsx", "export {}");
        write(&root, "packages/empty/src/other.ts", "export {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        let resolved = |specifier: &str| {
            map.resolve(specifier)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        };
        assert_eq!(
            resolved("@acme/core").as_deref(),
            Some("packages/core/src/index.ts")
        );
        assert_eq!(
            resolved("@acme/ui").as_deref(),
            Some("packages/ui/src/index.tsx")
        );
        assert_eq!(
            resolved("core").as_deref(),
            Some("packages/core/src/index.ts")
        );
        // A directory with no index file resolves nothing.
        assert_eq!(resolved("@acme/empty"), None);
        Ok(())
    }

    /// Fail-closed neighbors of the directory `index` probe: a file beside
    /// the directory, two index files, and a directory whose `package.json`
    /// would pick the entry instead.
    #[test]
    fn directory_target_with_a_competing_answer_fails_closed() -> Result<(), String> {
        let root = temp_dir("directory-index-closed");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@acme/*":["packages/*"]}}}"#,
        );
        write(&root, "packages/both.ts", "export {}");
        write(&root, "packages/both/index.ts", "export {}");
        write(&root, "packages/twice/index.ts", "export {}");
        write(&root, "packages/twice/index.js", "export {}");
        write(
            &root,
            "packages/manifest/package.json",
            r#"{"main":"lib.ts"}"#,
        );
        write(&root, "packages/manifest/index.ts", "export {}");
        write(&root, "packages/plain/index.ts", "export {}");

        let map = load_alias_map(&root).ok_or("should parse")?;
        assert_eq!(map.resolve("@acme/both"), None);
        assert_eq!(map.resolve("@acme/twice"), None);
        assert_eq!(map.resolve("@acme/manifest"), None);
        // The control: the same key resolves a plain directory.
        assert_eq!(
            map.resolve("@acme/plain"),
            Some(PathBuf::from("packages/plain/index.ts"))
        );
        Ok(())
    }

    /// The directory `index` probe follows the effective module resolution,
    /// and an unfollowed `extends` base that could set it fails closed.
    #[test]
    fn directory_target_follows_the_module_resolution() -> Result<(), String> {
        let resolves = |name: &str, config: &str| -> Result<bool, String> {
            let root = temp_dir(name);
            write(&root, "tsconfig.json", config);
            write(
                &root,
                "tsconfig.base.json",
                r#"{"compilerOptions":{"moduleResolution":"nodenext"}}"#,
            );
            write(&root, "packages/core/index.ts", "export {}");
            let map = load_alias_map(&root).ok_or("should parse")?;
            Ok(map.resolve("@acme/core").is_some())
        };
        let config = |head: &str| {
            format!(
                r#"{{{head}"compilerOptions":{{"baseUrl":".","paths":{{"@acme/*":["packages/*"]}}}}}}"#
            )
        };
        assert!(resolves("resolution-default", &config(""))?);
        assert!(!resolves(
            "resolution-inherited",
            &config(r#""extends":"./tsconfig.base.json","#)
        )?);
        // A package-name base is not followed; it could set the resolution.
        assert!(!resolves(
            "resolution-unfollowed",
            &config(r#""extends":"@acme/tsconfig","#)
        )?);
        let own = r#"{"extends":"@acme/tsconfig","compilerOptions":{"moduleResolution":"node10","baseUrl":".","paths":{"@acme/*":["packages/*"]}}}"#;
        assert!(resolves("resolution-own", own)?);
        Ok(())
    }

    #[test]
    fn fallback_array_resolves_the_first_existing_target_in_order() -> Result<(), String> {
        let root = temp_dir("fallback-order");
        write(&root, "src/owner.ts", "export function owner() {}");
        // An unrelated neighbour whose name merely starts with the stem.
        write(&root, "generated/ownership.ts", "export {}");
        let map = fallback_map(&root, r#"["generated/*","src/*"]"#)?;
        assert_eq!(resolved(&map, "@/owner").as_deref(), Some("src/owner.ts"));
        // A missing first directory is also provably absent.
        let map = fallback_map(&root, r#"["nowhere/deep/*","src/*"]"#)?;
        assert_eq!(resolved(&map, "@/owner").as_deref(), Some("src/owner.ts"));
        // Once the earlier target exists, `tsc` stops there: the later
        // same-named file is not what the import names.
        write(&root, "generated/owner.ts", "export function owner() {}");
        let map = fallback_map(&root, r#"["generated/*","src/*"]"#)?;
        assert_eq!(
            resolved(&map, "@/owner").as_deref(),
            Some("generated/owner.ts")
        );
        // No target exists: unresolved, and the key still owns the import.
        assert_eq!(resolved(&map, "@/missing"), None);
        assert!(map.paths_key_matches("@/missing"));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn fallback_stops_at_an_earlier_target_it_cannot_rule_out() -> Result<(), String> {
        // Each earlier target is something `tsc` may resolve but this map
        // does not model. Falling through to `src/owner.ts` would credit a
        // file the import does not name.
        for (label, earlier, file) in [
            ("declaration", "generated/*", Some("generated/owner.d.ts")),
            (
                "directory-manifest",
                "generated/*",
                Some("generated/owner/package.json"),
            ),
            (
                "other-extension",
                "generated/*",
                Some("generated/owner.vue"),
            ),
            ("json-module", "generated/*", Some("generated/owner.json")),
            ("other-case", "generated/*", Some("generated/Owner.ts")),
            ("multi-star", "generated/*/*", None),
            ("parent-escape", "../generated/*", None),
            ("absolute", "/generated/*", None),
        ] {
            let root = temp_dir(&format!("fallback-unknown-{label}"));
            write(&root, "src/owner.ts", "export function owner() {}");
            if let Some(file) = file {
                write(&root, file, "export {}");
            }
            let map = fallback_map(&root, &format!(r#"["{earlier}","src/*"]"#))?;
            assert_eq!(resolved(&map, "@/owner"), None, "{label}");
            let _ = std::fs::remove_dir_all(&root);
        }
        // A dotted specifier's declaration file carries the extra extension
        // in the middle of its name (`allowArbitraryExtensions`).
        let root = temp_dir("fallback-unknown-arbitrary-extension");
        write(&root, "src/style.css.ts", "export {}");
        let map = fallback_map(&root, r#"["generated/*","src/*"]"#)?;
        assert_eq!(
            resolved(&map, "@/style.css").as_deref(),
            Some("src/style.css.ts")
        );
        write(&root, "generated/style.d.css.ts", "export {}");
        assert_eq!(resolved(&map, "@/style.css"), None);
        let _ = std::fs::remove_dir_all(&root);
        // Two routed files at the earlier target are ambiguous, not absent.
        let root = temp_dir("fallback-unknown-ambiguous");
        write(&root, "src/owner.ts", "export function owner() {}");
        write(&root, "generated/owner.ts", "export {}");
        write(&root, "generated/owner.js", "export {}");
        let map = fallback_map(&root, r#"["generated/*","src/*"]"#)?;
        assert_eq!(resolved(&map, "@/owner"), None);
        // An empty list has no target at all.
        let map = fallback_map(&root, "[]")?;
        assert_eq!(resolved(&map, "@/owner"), None);
        assert!(map.paths_key_matches("@/owner"));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn literal_key_fallback_and_no_base_url_follow_the_same_walk() -> Result<(), String> {
        let root = temp_dir("fallback-literal");
        write(&root, "src/owner.ts", "export function owner() {}");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"paths":{"owner":["./generated/owner.ts","./src/owner.ts"],"bad":["generated/owner.ts","./src/owner.ts"]}}}"#,
        );
        let map = load_alias_map(&root).ok_or("the no-baseUrl config must load")?;
        assert_eq!(resolved(&map, "owner").as_deref(), Some("src/owner.ts"));
        // Without `baseUrl`, `tsc` rejects the non-relative first target
        // (TS5090); the walk ends there instead of skipping it.
        assert_eq!(resolved(&map, "bad"), None);
        let _ = std::fs::remove_dir_all(&root);
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

    /// `.` / `..` are relative specifiers (#4546, #4638 review): even a
    /// `paths` key spelled `..` never routes them through alias resolution.
    #[test]
    fn bare_dot_specifiers_are_relative_not_aliases() -> Result<(), String> {
        let root = temp_dir("relative-dots");
        write(
            &root,
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"..":["src/calc"],".":["src/calc"],"@calc":["src/calc"]}}}"#,
        );
        write(&root, "src/calc.ts", "export const calc = 1;\n");
        let map = load_alias_map(&root).ok_or("should parse")?;
        assert!(map.resolve("@calc").is_some(), "control alias must resolve");
        assert!(map.resolve("..").is_none());
        assert!(map.resolve(".").is_none());
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

    // ── JSONC (#4549) ─────────────────────────────────────────────────────

    #[test]
    fn jsonc_line_comment_endings_preserve_alias_resolution() -> Result<(), String> {
        for filename in ["tsconfig.json", "jsconfig.json"] {
            for (label, ending) in [("lf", "\n"), ("crlf", "\r\n"), ("cr", "\r")] {
                let root = temp_dir(&format!("jsonc-{filename}-{label}"));
                fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
                let config = "{ // config\n\"compilerOptions\": {\"baseUrl\": \".\",\"paths\": {\"@owner\": [\"src/owner\"]}, // paths\n}, // options\n} // eof".replace('\n', ending);
                fs::write(root.join(filename), config).map_err(|err| err.to_string())?;
                fs::write(root.join("src/owner.ts"), "export const owner = 1;")
                    .map_err(|err| err.to_string())?;

                let (map, error, gap) = load_alias_map_with_read_error(&root);
                assert!(error.is_none(), "{filename}/{label}: {error:?}");
                assert!(gap.is_none(), "{filename}/{label}: {gap:?}");
                let map = map.ok_or_else(|| format!("{filename}/{label} must parse"))?;
                assert_eq!(map.resolve("@owner"), Some(PathBuf::from("src/owner.ts")));
                assert!(map.resolve("@other").is_none());
                fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }

    /// `tsc --init`-shaped config: line comments, a block comment, and
    /// trailing commas. The alias must resolve exactly as the strict
    /// equivalent does.
    #[test]
    fn jsonc_tsconfig_with_comments_and_trailing_commas_resolves() -> Result<(), String> {
        let root = temp_dir("jsonc-resolves");
        write(
            &root,
            "tsconfig.json",
            "{\n  // Visit https://aka.ms/tsconfig to read more\n  /* Language and Environment */\n  \"compilerOptions\": {\n    \"target\": \"es2016\", /* Set the JS language version. */\n    \"baseUrl\": \".\", // anchor\n    \"paths\": {\n      \"@/*\": [\"src/*\",],\n    },\n  },\n}\n",
        );
        write(&root, "src/owner.ts", "export function owner() {}");
        let (map, err, gap) = load_alias_map_with_read_error(&root);
        assert!(err.is_none(), "no read error expected");
        assert!(
            gap.is_none(),
            "JSONC config must not produce a gap: {gap:?}"
        );
        let map = map.ok_or("JSONC config should parse")?;
        let resolved = map.resolve("@/owner").ok_or("should resolve")?;
        assert_eq!(
            resolved.to_string_lossy().replace('\\', "/"),
            "src/owner.ts"
        );
        Ok(())
    }

    #[test]
    fn strip_jsonc_drops_leading_byte_order_mark() -> Result<(), String> {
        let out = strip_jsonc("\u{feff}{ // bom\n \"a\": 1, }")?;
        let value: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
        assert_eq!(value, serde_json::json!({ "a": 1 }));
        // Only a LEADING mark is dropped; one inside a string is content.
        let inner = strip_jsonc("{ \"a\": \"\u{feff}x\" }")?;
        assert!(inner.contains('\u{feff}'));
        Ok(())
    }

    #[test]
    fn bom_prefixed_tsconfig_resolves_aliases() -> Result<(), String> {
        let root = temp_dir("bom");
        write(
            &root,
            "tsconfig.json",
            "\u{feff}{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@calc\":[\"src/calc\"]}}}",
        );
        write(&root, "src/calc.ts", "export const calc = 1;\n");
        let map = load_alias_map(&root).ok_or("a BOM-prefixed tsconfig must parse")?;
        assert!(map.resolve("@calc").is_some());
        Ok(())
    }

    #[test]
    fn strip_jsonc_removes_line_and_block_comments() -> Result<(), String> {
        let out = strip_jsonc("{ // line\n \"a\": /* block\n spans */ 1 }")?;
        let value: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
        assert_eq!(value, serde_json::json!({ "a": 1 }));
        Ok(())
    }

    #[test]
    fn strip_jsonc_removes_trailing_commas_before_close() -> Result<(), String> {
        let out = strip_jsonc("{ \"a\": [1, 2, /* c */ ], \"b\": {\"c\": 3,\n // x\n },\n}")?;
        let value: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
        assert_eq!(value, serde_json::json!({ "a": [1, 2], "b": { "c": 3 } }));
        Ok(())
    }

    /// Comment-looking and comma-looking text inside string literals is data,
    /// including after an escaped quote.
    #[test]
    fn strip_jsonc_preserves_string_contents() -> Result<(), String> {
        let text = r#"{"@/*": ["src/*"], "u": "http://x/*y*/", "q": "a\"// b,}", "e": "c\\"}"#;
        let out = strip_jsonc(text)?;
        assert_eq!(out, text, "no byte outside a comment may change");
        let value: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
        assert_eq!(value["u"], "http://x/*y*/");
        assert_eq!(value["q"], "a\"// b,}");
        assert_eq!(value["e"], "c\\");
        Ok(())
    }

    /// Lexical damage fails closed instead of guessing where the comment or
    /// string was meant to end.
    #[test]
    fn strip_jsonc_fails_closed_on_unterminated_comment_or_string() {
        assert_eq!(
            strip_jsonc("{ \"a\": 1 /* never closed }"),
            Err("unterminated block comment")
        );
        assert_eq!(strip_jsonc("{ /*/ }"), Err("unterminated block comment"));
        assert_eq!(
            strip_jsonc("{ \"a\": \"open }"),
            Err("unterminated string literal")
        );
    }

    /// An unterminated block comment in tsconfig.json yields the typed
    /// unparseable gap and still blocks the jsconfig.json fallback.
    #[test]
    fn unterminated_block_comment_fails_closed_as_unparseable() {
        let root = temp_dir("jsonc-unterminated");
        write(
            &root,
            "tsconfig.json",
            "{ /* open\n \"compilerOptions\": {\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"src/*\"]}} }",
        );
        write(
            &root,
            "jsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        );
        let (map, err, gap) = load_alias_map_with_read_error(&root);
        assert!(map.is_none() && err.is_none());
        assert_eq!(
            gap,
            Some(TsAliasMapLoadGap::ConfigUnparseable {
                config: "tsconfig.json",
                detail: "unterminated block comment".to_string(),
            })
        );
    }

    /// Lone commas that are not trailing (`[,1]`, `{,}` with content, `,,`)
    /// are not "fixed" — they still reach the strict parser and fail.
    #[test]
    fn strip_jsonc_leaves_non_trailing_comma_errors_for_the_parser() -> Result<(), String> {
        for bad in ["[,1]", "[1,,2]", "{\"a\":1,,}"] {
            let out = strip_jsonc(bad)?;
            assert!(
                serde_json::from_str::<serde_json::Value>(&out).is_err(),
                "{bad:?} must stay malformed after normalization, got {out:?}"
            );
        }
        Ok(())
    }
}
