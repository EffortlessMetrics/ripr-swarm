//! Typed Cargo path-dependency capture and workspace graph provenance (#2968, #3037, #5058).
//!
//! Owned submodule extracted from the seam cache monolith; `seam_cache.rs` retains
//! cache layers, store limits, envelopes, and workspace fingerprint hashing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::collect_named_workspace_files;
use super::stable_input_hash;

/// Cargo section scope of a path-dependency declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PathDependencySection {
    Dependencies,
    DevDependencies,
    BuildDependencies,
}

impl PathDependencySection {
    fn from_table_name(name: &str) -> Option<Self> {
        match name {
            "dependencies" => Some(Self::Dependencies),
            "dev-dependencies" => Some(Self::DevDependencies),
            "build-dependencies" => Some(Self::BuildDependencies),
            _ => None,
        }
    }
}

/// Where the path authority for a dependency edge lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PathDependencySource {
    /// `path = "..."` spelled directly in the declaring package manifest.
    Package,
    /// `workspace = true` in the declaring manifest, with the `path` resolved
    /// from the nearest ancestor workspace root's `[workspace.dependencies]`.
    WorkspaceInherited,
}

/// Resolution outcome for one captured path-dependency edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PathDependencyResolution {
    /// Resolved lexically to a repo-relative path that stays inside the scan
    /// root and names an existing directory.
    Resolved,
    /// Resolved lexically but the path climbs above the scan root; the
    /// repo-relative identity keeps its leading `..` segments.
    ResolvedOutsideWorkspace,
    /// Resolved lexically inside the scan root, but the target is missing or
    /// is not a directory on disk.
    TargetMissing,
    /// The manifest declares an absolute POSIX or Windows path. It is a valid
    /// Cargo path dependency, but it has no repo-relative identity for this
    /// inventory and is therefore retained as an explicit boundary.
    UnsupportedAbsolutePath,
    /// `workspace = true` was declared but no nearest-ancestor workspace root
    /// with a matching `[workspace.dependencies]` entry was found, so no path
    /// authority exists. This is a typed limitation, not a missing edge.
    UnresolvedWorkspaceInheritance,
    /// The declaration carries both `path` and `workspace = true`. Cargo
    /// rejects this form when the name is absent from
    /// `[workspace.dependencies]`, and otherwise ignores the member's own
    /// `path` (the workspace entry wins), so honoring the explicit `path`
    /// would misstate Cargo semantics. The declaration is retained as a typed
    /// invalid edge with no resolved identity, never as a resolvable
    /// `Package` path edge (the `source` still records where the declaration
    /// was seen).
    InvalidDeclaration,
}

/// Inheritance authority for `workspace = true` dependencies: per workspace
/// root, the repo-relative directory text and the `(name, path)` entries of
/// its `[workspace.dependencies]` (`None` path = version-only registry
/// entry, not a path edge).
type WorkspaceDependencyTables = Vec<(String, Vec<(String, Option<String>)>)>;

/// One captured Cargo path-dependency declaration (#2968, typed in #3037).
///
/// V1 denominator: an edge is recorded for every path-bearing dependency
/// declaration visible in a scanned manifest:
///
/// - `dep = { path = "...", ... }` inline tables in `[dependencies]`,
///   `[dev-dependencies]`, and `[build-dependencies]` (target = `None`);
/// - the same inline-table form under `[target.<spec>.<section>]`
///   (target = `Some(spec)`);
/// - `dep = { workspace = true }` in any of those scopes whose nearest
///   ancestor workspace root declares `dep` with a `path` in
///   `[workspace.dependencies]` (source = `WorkspaceInherited`; the path is
///   resolved relative to the workspace root manifest directory, per Cargo).
///
/// Explicitly out of the denominator (documented boundaries, never silent):
///
/// - string-form `dep = "1.0"` and table-form entries without `path` or
///   `workspace = true` are registry/git declarations, not path edges;
/// - `workspace = true` resolving to a workspace entry without `path` is a
///   registry inheritance, not a path edge;
/// - bare `[workspace.dependencies]` entries are inheritance authority only —
///   they have no consumer, so they are not edges;
/// - a manifest whose repo-relative path is not UTF-8 contributes no edges;
///   the gap is surfaced in `path_dependency_limitations` instead of a
///   lossy-corrupted path identity;
/// - absolute POSIX and Windows drive-prefixed paths remain explicit edges with
///   `UnsupportedAbsolutePath` and no fabricated repo-relative identity;
/// - a `[package] workspace = "..."` redirect is not followed: `workspace =
///   true` dependencies resolve against the nearest ancestor manifest with a
///   `[workspace]` table only, and a manifest whose inherited dependencies are
///   affected by an unfollowed redirect contributes an
///   `UnfollowedWorkspaceRedirect` entry to `path_dependency_limitations`;
/// - an entry carrying both `path` and `workspace = true` is Cargo-invalid
///   (Cargo rejects it when the name is absent from `[workspace.dependencies]`
///   and ignores the member's own `path` otherwise); it is retained as an
///   `InvalidDeclaration` edge with no resolved identity, never honored as a
///   `Package` path edge.
///
/// Ordering is fully sorted and exact duplicate records are deduplicated, so
/// the inventory is deterministic under manifest enumeration changes. Cycles
/// are preserved as-is (a cycle is distinct records, never a duplicate);
/// later traversal (#2969/#2970) must see them rather than have capture
/// erase them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PathDependencyEdge {
    /// Repo-relative manifest that declares the dependency, `/`-separated.
    pub(crate) from_manifest: String,
    /// Section scope of the declaration.
    pub(crate) section: PathDependencySection,
    /// Target-specific scope (the `[target.<spec>...]` key), if any.
    pub(crate) target: Option<String>,
    pub(crate) dependency_name: String,
    /// Path as spelled in the manifest holding the path authority,
    /// normalized to `/` separators. `None` when no path authority was found
    /// (`UnresolvedWorkspaceInheritance`).
    pub(crate) declared_path: Option<String>,
    /// Lexically resolved repo-relative identity, `/`-separated. `None` when
    /// no path authority was found, when the declared path is absolute
    /// (`UnsupportedAbsolutePath`), or when the declaration is Cargo-invalid
    /// (`InvalidDeclaration`).
    pub(crate) resolved_path: Option<String>,
    pub(crate) resolution: PathDependencyResolution,
    pub(crate) source: PathDependencySource,
}

/// Machine-visible kind of a path-dependency capture limitation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PathDependencyLimitationKind {
    /// A manifest's repo-relative path is not UTF-8, so no portable identity
    /// exists and edge capture was skipped for that manifest.
    NonUtf8ManifestPath,
    /// A manifest could not be decoded as UTF-8 or parsed as TOML, so no
    /// declarations were visible and edge capture was skipped for it.
    ManifestUnparsed,
    /// A `[package] workspace = "..."` redirect was not followed: the
    /// manifest's `workspace = true` dependencies were resolved against the
    /// nearest ancestor workspace root instead of the redirected one, so the
    /// inherited edge set for this manifest may be wrong or incomplete.
    UnfollowedWorkspaceRedirect,
}

/// One typed path-dependency capture limitation. A non-empty
/// [`WorkspaceGraphProvenance::path_dependency_limitations`] list means
/// `path_dependency_edges` is a partial inventory and must not be treated as
/// a complete path-dependency graph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PathDependencyLimitation {
    /// Repo-relative manifest the limitation belongs to, `/`-separated.
    /// Empty when the manifest path itself is not UTF-8 (the lossy spelling
    /// is carried in `detail` instead of a fabricated identity).
    pub(crate) manifest: String,
    pub(crate) kind: PathDependencyLimitationKind,
    /// Human-readable detail for diagnostics; `kind` is the machine contract.
    pub(crate) detail: String,
}

/// Producer-owned provenance for the local Cargo package and feature graphs.
/// External dependency metadata is never resolved here: targeted reruns must
/// not perform network work or imply that registry graph facts were observed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceGraphProvenance {
    pub(crate) package_graph_status: String,
    pub(crate) package_graph_hash: Option<String>,
    pub(crate) package_graph_detail: Option<String>,
    pub(crate) feature_graph_status: String,
    pub(crate) feature_graph_hash: Option<String>,
    pub(crate) feature_graph_detail: Option<String>,
    pub(crate) external_dependency_graph_status: String,
    pub(crate) external_dependency_graph_detail: String,
    /// #2968/#3037: typed path-dependency edges captured from Cargo manifests;
    /// see [`PathDependencyEdge`] for the exact V1 denominator. Preserved but
    /// not yet walked (#2969/#2970 consume this).
    ///
    /// Currentness contract: this field is deliberately excluded from
    /// `package_graph_hash` (the hash predates the field and covers package /
    /// feature facts only). Edges are recomputed on every provenance build,
    /// so consumers must rebuild provenance for current edges and must not
    /// read hash stability as edge stability.
    pub(crate) path_dependency_edges: Vec<PathDependencyEdge>,
    /// Typed capture limitations (for example a non-UTF-8 manifest path or an
    /// unfollowed `[package] workspace = "..."` redirect). A non-empty list
    /// means `path_dependency_edges` is a partial inventory and must not be
    /// treated as a complete path-dependency graph.
    pub(crate) path_dependency_limitations: Vec<PathDependencyLimitation>,
}

/// Read local Cargo manifests and derive deterministic package/feature graph
/// facts without invoking Cargo, rustc, a registry, or a network client.
pub(crate) fn workspace_graph_provenance(root: &Path) -> WorkspaceGraphProvenance {
    let mut manifests = Vec::new();
    collect_named_workspace_files(root, root, "Cargo.toml", &mut manifests);
    manifests.sort_by(|left, right| left.0.cmp(&right.0));

    if manifests.is_empty() {
        return WorkspaceGraphProvenance {
            package_graph_status: "unavailable".to_string(),
            package_graph_detail: Some("no local Cargo.toml manifest was found".to_string()),
            feature_graph_status: "unavailable".to_string(),
            feature_graph_detail: Some("no local Cargo.toml manifest was found".to_string()),
            external_dependency_graph_status: "unavailable".to_string(),
            external_dependency_graph_detail:
                "external dependency metadata is not resolved; no network access was used"
                    .to_string(),
            path_dependency_edges: Vec::new(),
            ..WorkspaceGraphProvenance::default()
        };
    }

    let mut parse_errors = Vec::new();
    let mut unparsed_manifests = Vec::new();
    let mut parsed_manifests = Vec::new();
    for (path, bytes) in &manifests {
        let path_text = path.to_string_lossy().replace('\\', "/");
        match std::str::from_utf8(bytes)
            .map_err(|err| err.to_string())
            .and_then(|text| toml::from_str::<toml::Value>(text).map_err(|err| err.to_string()))
        {
            Ok(value) => parsed_manifests.push((path, path_text, value)),
            Err(err) => {
                parse_errors.push(format!("{path_text}: {err}"));
                unparsed_manifests.push((path_text.clone(), err));
            }
        }
    }

    // Inheritance authority for `workspace = true` dependencies: for every
    // manifest with a `[workspace]` table, the repo-relative directory text
    // and the `(name, path)` entries of its `[workspace.dependencies]`
    // (`None` path = version-only registry entry). A non-UTF-8 workspace
    // root is not an authority (the same boundary as edge capture); its
    // members report `UnresolvedWorkspaceInheritance`.
    let mut workspace_tables: WorkspaceDependencyTables = Vec::new();
    for (path, _, value) in &parsed_manifests {
        let Some(workspace) = value.get("workspace").and_then(toml::Value::as_table) else {
            continue;
        };
        let Some(dir_text) = portable_manifest_dir_text(path) else {
            continue;
        };
        let entries = workspace
            .get("dependencies")
            .and_then(toml::Value::as_table)
            .map(|table| {
                table
                    .iter()
                    .map(|(name, dep)| {
                        let declared = dep
                            .as_table()
                            .and_then(|dep_table| dep_table.get("path"))
                            .and_then(toml::Value::as_str)
                            .map(str::to_string);
                        (name.clone(), declared)
                    })
                    .collect()
            })
            .unwrap_or_default();
        workspace_tables.push((dir_text, entries));
    }

    let mut package_facts = Vec::new();
    let mut path_dep_edges: Vec<PathDependencyEdge> = Vec::new();
    let mut path_dep_limitations: Vec<PathDependencyLimitation> = Vec::new();
    let mut feature_facts = Vec::new();
    for (path, path_text, value) in &parsed_manifests {
        let package = value.get("package").and_then(toml::Value::as_table);
        let package_name = package
            .and_then(|table| table.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("<workspace-only>");
        let workspace_members = value
            .get("workspace")
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("members"))
            .map(canonical_toml_value)
            .unwrap_or_default();
        let dependencies = ["dependencies", "dev-dependencies", "build-dependencies"]
            .into_iter()
            .filter_map(|section| value.get(section).and_then(toml::Value::as_table))
            .flat_map(|table| table.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        // #2968/#3037: capture typed path-dependency edges for #2969/#2970.
        // A non-UTF-8 manifest path cannot produce a portable repo-relative
        // identity, so it contributes a limitation instead of lossy edges.
        match portable_manifest_dir_text(path) {
            Some(dir_text) => {
                let workspace_redirect = package
                    .and_then(|table| table.get("workspace"))
                    .and_then(toml::Value::as_str);
                let inherited = collect_path_dependency_edges(
                    root,
                    path_text,
                    &dir_text,
                    value,
                    &workspace_tables,
                    &mut path_dep_edges,
                );
                // A `[package] workspace = "..."` redirect is never followed:
                // inherited dependencies resolve against the nearest ancestor
                // workspace root. When the manifest actually declares
                // `workspace = true` dependencies, that gap makes the
                // inventory potentially partial and must be machine-visible.
                if let Some(redirect) = workspace_redirect
                    && inherited > 0
                {
                    path_dep_limitations.push(PathDependencyLimitation {
                        manifest: path_text.clone(),
                        kind: PathDependencyLimitationKind::UnfollowedWorkspaceRedirect,
                        detail: format!(
                            "`[package] workspace = \"{redirect}\"` redirect not followed; \
                             {inherited} `workspace = true` dependencies resolved against \
                             the nearest ancestor workspace root instead"
                        ),
                    });
                }
            }
            None => path_dep_limitations.push(PathDependencyLimitation {
                manifest: String::new(),
                kind: PathDependencyLimitationKind::NonUtf8ManifestPath,
                detail: format!(
                    "path-dependency edges not captured for a manifest with a non-UTF-8 \
                     repo-relative path (lossy spelling: {path_text})"
                ),
            }),
        }
        package_facts.push(format!(
            "{path_text}\0package={package_name}\0members={workspace_members}\0dependencies={dependencies:?}"
        ));

        let feature_values = value
            .get("features")
            .and_then(toml::Value::as_table)
            .map(|table| {
                table
                    .iter()
                    .map(|(name, value)| format!("{name}={}", canonical_toml_value(value)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        feature_facts.push(format!("{path_text}\0features={feature_values:?}"));
    }

    let parse_detail = (!parse_errors.is_empty()).then(|| parse_errors.join("; "));
    let package_graph_status = if package_facts.is_empty() {
        "unavailable"
    } else if parse_detail.is_some() {
        "limited"
    } else {
        "complete"
    };
    let feature_graph_status = if parse_detail.is_some() {
        "limited"
    } else {
        "complete"
    };
    package_facts.sort();
    feature_facts.sort();
    path_dep_edges.sort();
    path_dep_edges.dedup();
    let graph_context = UnparsedManifestContext::new(root, &parsed_manifests, &path_dep_edges);
    for (manifest, detail) in unparsed_manifests {
        if graph_context.proves_outside_graph(&manifest) {
            continue;
        }
        path_dep_limitations.push(PathDependencyLimitation {
            manifest,
            kind: PathDependencyLimitationKind::ManifestUnparsed,
            detail: format!("path-dependency edges not captured: {detail}"),
        });
    }
    path_dep_limitations.sort();
    WorkspaceGraphProvenance {
        package_graph_status: package_graph_status.to_string(),
        package_graph_hash: (!package_facts.is_empty())
            .then(|| stable_input_hash(package_facts.join("\n").as_bytes())),
        package_graph_detail: parse_detail.clone(),
        feature_graph_status: feature_graph_status.to_string(),
        feature_graph_hash: Some(stable_input_hash(feature_facts.join("\n").as_bytes())),
        feature_graph_detail: parse_detail,
        external_dependency_graph_status: "unavailable".to_string(),
        external_dependency_graph_detail:
            "external dependency metadata is not resolved; no network access was used".to_string(),
        path_dependency_edges: path_dep_edges,
        path_dependency_limitations: path_dep_limitations,
    }
}

/// Repo-relative, `/`-separated directory text of a manifest path, or `None`
/// when the path is not valid UTF-8 (the supported-path boundary: no lossy
/// conversion). The root manifest's directory is the empty string.
/// What the parsed manifests prove about where an unparsed manifest can sit.
struct UnparsedManifestContext {
    /// `(workspace root directory, member patterns)` for every parsed manifest
    /// with a `[workspace]` table; `None` patterns mean `members` is present
    /// but not a plain string array and so cannot be evaluated.
    workspaces: Vec<(String, Option<Vec<String>>)>,
    /// Directories of every parsed manifest.
    parsed_dirs: Vec<String>,
    /// Directories named as a path-dependency, `[patch]` or `[replace]` target.
    targets: BTreeSet<String>,
}

impl UnparsedManifestContext {
    fn new(
        root: &Path,
        parsed_manifests: &[(&PathBuf, String, toml::Value)],
        edges: &[PathDependencyEdge],
    ) -> Self {
        let mut workspaces = Vec::new();
        let mut parsed_dirs = Vec::new();
        let mut targets: BTreeSet<String> = edges
            .iter()
            .filter_map(|edge| edge.resolved_path.clone())
            .collect();
        for (path, _, value) in parsed_manifests {
            let Some(dir) = portable_manifest_dir_text(path) else {
                continue;
            };
            if let Some(workspace) = value.get("workspace").and_then(toml::Value::as_table) {
                let patterns = match workspace.get("members") {
                    None => Some(Vec::new()),
                    Some(members) => members.as_array().and_then(|items| {
                        items
                            .iter()
                            .map(|item| item.as_str().map(str::to_string))
                            .collect::<Option<Vec<_>>>()
                    }),
                };
                workspaces.push((dir.clone(), patterns));
            }
            // `[patch.<source>]` and `[replace]` entries are graph members
            // that the edge inventory does not capture.
            let mut patched = Vec::new();
            if let Some(patch) = value.get("patch").and_then(toml::Value::as_table) {
                patched.extend(patch.values().filter_map(toml::Value::as_table));
            }
            if let Some(replace) = value.get("replace").and_then(toml::Value::as_table) {
                patched.push(replace);
            }
            for entries in patched {
                for entry in entries.values() {
                    if let Some(declared) = entry
                        .as_table()
                        .and_then(|table| table.get("path"))
                        .and_then(toml::Value::as_str)
                    {
                        let declared = declared.replace('\\', "/");
                        targets.insert(resolve_repo_relative(&dir, &declared).0);
                    }
                }
            }
            parsed_dirs.push(dir);
        }
        // The scanned manifest identity replaces `\\` with `/` and does not
        // follow symlinked directories, so a target can spell the same
        // directory differently from the unparsed manifest: add the
        // separator-normalised spelling and the resolved real location.
        let canonical_root = root.canonicalize().ok();
        let mut spellings = BTreeSet::new();
        for target in &targets {
            spellings.insert(target.replace('\\', "/"));
            if let (Some(canonical_root), Ok(real)) =
                (&canonical_root, root.join(target).canonicalize())
                && let Ok(relative) = real.strip_prefix(canonical_root)
            {
                spellings.insert(relative.to_string_lossy().replace('\\', "/"));
            }
        }
        targets.extend(spellings);
        Self {
            workspaces,
            parsed_dirs,
            targets,
        }
    }

    /// True when an unparsed manifest provably cannot be part of the
    /// path-dependency graph: no path-dependency, `[patch]` or `[replace]`
    /// target is its directory, no parsed manifest lives beneath it (it could
    /// be a workspace root whose members inherit from it), and at least one
    /// ancestor workspace exists, every ancestor workspace enumerates
    /// `members` with patterns this matcher fully understands, and none
    /// selects it. Cargo refuses a workspace whose member or path-dependency
    /// target fails to parse, so a stray fixture manifest (for example a
    /// deliberately invalid `Cargo.toml` under a test directory) is outside
    /// the graph and must not mark the inventory partial. Anything unproven
    /// stays a limitation (fail closed).
    fn proves_outside_graph(&self, manifest: &str) -> bool {
        let dir = manifest.rsplit_once('/').map_or("", |(dir, _)| dir);
        if dir.is_empty() || self.targets.contains(dir) {
            return false;
        }
        let beneath = format!("{dir}/");
        if self
            .parsed_dirs
            .iter()
            .any(|parsed| parsed.starts_with(&beneath))
        {
            return false;
        }
        let mut ancestors = 0usize;
        for (root_dir, patterns) in &self.workspaces {
            let relative = if root_dir.is_empty() {
                dir
            } else {
                match dir
                    .strip_prefix(root_dir.as_str())
                    .and_then(|rest| rest.strip_prefix('/'))
                {
                    Some(rest) => rest,
                    None => continue,
                }
            };
            ancestors += 1;
            let Some(patterns) = patterns else {
                return false;
            };
            if !patterns
                .iter()
                .all(|pattern| matches!(member_pattern_selects(pattern, relative), Some(false)))
            {
                return false;
            }
        }
        ancestors > 0
    }
}

/// Evaluate one `[workspace] members` pattern against a root-relative
/// directory. Supports literal segments and `*` wildcards inside a segment;
/// returns `None` for syntax it does not model (`**`, `?`, classes, braces,
/// absolute or `..` paths) so the caller fails closed.
pub(super) fn member_pattern_selects(pattern: &str, relative: &str) -> Option<bool> {
    let pattern = pattern.replace('\\', "/");
    let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
    if pattern.is_empty()
        || pattern.starts_with('/')
        || pattern.contains(['?', '[', '{'])
        || pattern.contains("**")
        || pattern
            .split('/')
            .any(|segment| segment == ".." || segment == ".")
        || pattern.chars().nth(1) == Some(':')
    {
        return None;
    }
    let pattern_segments: Vec<&str> = pattern.split('/').collect();
    let dir_segments: Vec<&str> = relative.split('/').collect();
    Some(
        pattern_segments.len() == dir_segments.len()
            && pattern_segments
                .iter()
                .zip(&dir_segments)
                .all(|(pattern, segment)| segment_matches(pattern, segment)),
    )
}

/// Match one path segment against a pattern whose only metacharacter is `*`.
pub(super) fn segment_matches(pattern: &str, segment: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(mut rest) = segment.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        let Some(at) = rest.find(part) else {
            return false;
        };
        rest = &rest[at + part.len()..];
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

fn portable_manifest_dir_text(manifest_path: &Path) -> Option<String> {
    let dir = manifest_path.parent().unwrap_or_else(|| Path::new(""));
    Some(dir.to_str()?.replace('\\', "/"))
}

/// True when a declared Cargo path is absolute in either the POSIX or
/// Windows drive-prefixed sense. Absolute paths are valid Cargo inputs, but
/// they cannot be represented as repo-relative identities without observing
/// the host filesystem root.
pub(super) fn is_absolute_declared_path(declared: &str) -> bool {
    // A `/`-rooted path is absolute on every host: Cargo on Windows still
    // resolves `/opt/shared` against the current drive root. Remaining
    // forms (drive and UNC roots) follow host path semantics, so on Unix
    // `a:b` and `..\\shared` stay relative.
    if declared.starts_with('/') {
        return true;
    }
    Path::new(declared).is_absolute()
}

/// Classify a lexically resolved repo-relative path target. A path
/// dependency resolves only when its target is an existing directory
/// that contains a Cargo.toml manifest; a bare directory cannot satisfy
/// the dependency, so it must not count as `Resolved` and become a
/// manifest node downstream (#3613 review).
fn classify_repo_relative_target(root: &Path, resolved: &str) -> PathDependencyResolution {
    let target = root.join(resolved);
    if target.is_dir() && target.join("Cargo.toml").is_file() {
        PathDependencyResolution::Resolved
    } else {
        PathDependencyResolution::TargetMissing
    }
}

/// Lexically resolve a `/`-separated declared path against a `/`-separated
/// repo-relative directory, without touching the filesystem. The caller
/// passes the host resolution form: on Windows hosts `\` has been normalized
/// to `/`; on Unix hosts `\` is a legal filename character and is preserved.
/// Returns the normalized identity; `escaped` is true when resolution climbs
/// above the scan root, in which case the identity keeps its leading `..`
/// segments.
pub(super) fn resolve_repo_relative(base: &str, declared: &str) -> (String, bool) {
    let mut components: Vec<&str> = base
        .split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect();
    for component in declared.split('/') {
        match component {
            "" | "." => {}
            ".." => match components.last() {
                Some(&"..") | None => components.push(".."),
                Some(_) => {
                    components.pop();
                }
            },
            _ => components.push(component),
        }
    }
    let escaped = components.first() == Some(&"..");
    (components.join("/"), escaped)
}

/// Find the path authority for a `workspace = true` dependency: the nearest
/// ancestor directory (inclusive) with a `[workspace]` manifest, then the
/// dependency's entry in its `[workspace.dependencies]`. Returns the
/// workspace root directory text and the entry's declared path (`None` =
/// version-only registry entry, not a path edge). `None` overall means no
/// authority was found at all.
pub(super) fn workspace_dependency_path(
    workspace_tables: &WorkspaceDependencyTables,
    manifest_dir: &str,
    dependency_name: &str,
) -> Option<(String, Option<String>)> {
    let mut ancestor = manifest_dir.to_string();
    loop {
        if let Some((_, entries)) = workspace_tables.iter().find(|(dir, _)| dir == &ancestor) {
            return entries
                .iter()
                .find(|(name, _)| name == dependency_name)
                .map(|(_, path)| (ancestor.clone(), path.clone()));
        }
        if ancestor.is_empty() {
            return None;
        }
        ancestor = ancestor
            .rsplit_once('/')
            .map(|(head, _)| head.to_string())
            .unwrap_or_default();
    }
}

/// Capture the V1 path-dependency edge denominator (see
/// [`PathDependencyEdge`]) from one parsed manifest. Returns the number of
/// `workspace = true` inheritance declarations seen, so the caller can flag
/// an unfollowed `[package] workspace = "..."` redirect when inheritance was
/// actually in play.
fn collect_path_dependency_edges(
    root: &Path,
    manifest_text: &str,
    manifest_dir: &str,
    value: &toml::Value,
    workspace_tables: &WorkspaceDependencyTables,
    edges: &mut Vec<PathDependencyEdge>,
) -> usize {
    let mut workspace_inherited_count = 0usize;
    let mut collect_section = |section: PathDependencySection,
                               target: Option<&str>,
                               table: &toml::Table| {
        for (dependency_name, dep_value) in table {
            let Some(dep_table) = dep_value.as_table() else {
                // String form (`dep = "1.0"`): registry declaration, never a
                // path edge.
                continue;
            };
            let workspace_inherited =
                dep_table.get("workspace").and_then(toml::Value::as_bool) == Some(true);
            if let Some(declared) = dep_table.get("path").and_then(toml::Value::as_str) {
                // The recorded declaration keeps the manifest's raw spelling;
                // only resolution applies host path semantics (`\` is a
                // separator on Windows hosts, a filename character on Unix).
                let declared = declared.to_string();
                #[cfg(windows)]
                let resolution_form = declared.replace('\\', "/");
                #[cfg(not(windows))]
                let resolution_form = declared.clone();
                if workspace_inherited {
                    // `path` + `workspace = true` is Cargo-invalid: Cargo
                    // rejects it when the name is absent from
                    // `[workspace.dependencies]` and otherwise the workspace
                    // entry wins, so honoring the member's explicit `path`
                    // would misstate Cargo semantics. Retain the declaration
                    // as a typed invalid edge instead of a resolved one.
                    edges.push(PathDependencyEdge {
                        from_manifest: manifest_text.to_string(),
                        section,
                        target: target.map(str::to_string),
                        dependency_name: dependency_name.clone(),
                        declared_path: Some(declared),
                        resolved_path: None,
                        resolution: PathDependencyResolution::InvalidDeclaration,
                        source: PathDependencySource::Package,
                    });
                    continue;
                }
                let (resolved_path, resolution) = if is_absolute_declared_path(&resolution_form) {
                    (None, PathDependencyResolution::UnsupportedAbsolutePath)
                } else {
                    let (resolved, escaped) = resolve_repo_relative(manifest_dir, &resolution_form);
                    let resolution = if escaped {
                        PathDependencyResolution::ResolvedOutsideWorkspace
                    } else {
                        classify_repo_relative_target(root, &resolved)
                    };
                    (Some(resolved), resolution)
                };
                edges.push(PathDependencyEdge {
                    from_manifest: manifest_text.to_string(),
                    section,
                    target: target.map(str::to_string),
                    dependency_name: dependency_name.clone(),
                    declared_path: Some(declared),
                    resolved_path,
                    resolution,
                    source: PathDependencySource::Package,
                });
            } else if workspace_inherited {
                workspace_inherited_count += 1;
                match workspace_dependency_path(workspace_tables, manifest_dir, dependency_name) {
                    Some((root_dir, Some(declared))) => {
                        #[cfg(windows)]
                        let resolution_form = declared.replace('\\', "/");
                        #[cfg(not(windows))]
                        let resolution_form = declared.clone();
                        let (resolved_path, resolution) =
                            if is_absolute_declared_path(&resolution_form) {
                                (None, PathDependencyResolution::UnsupportedAbsolutePath)
                            } else {
                                let (resolved, escaped) =
                                    resolve_repo_relative(&root_dir, &resolution_form);
                                let resolution = if escaped {
                                    PathDependencyResolution::ResolvedOutsideWorkspace
                                } else {
                                    classify_repo_relative_target(root, &resolved)
                                };
                                (Some(resolved), resolution)
                            };
                        edges.push(PathDependencyEdge {
                            from_manifest: manifest_text.to_string(),
                            section,
                            target: target.map(str::to_string),
                            dependency_name: dependency_name.clone(),
                            declared_path: Some(declared),
                            resolved_path,
                            resolution,
                            source: PathDependencySource::WorkspaceInherited,
                        });
                    }
                    Some((_, None)) => {
                        // Version-only workspace entry: a registry
                        // inheritance, not a path edge.
                    }
                    None => edges.push(PathDependencyEdge {
                        from_manifest: manifest_text.to_string(),
                        section,
                        target: target.map(str::to_string),
                        dependency_name: dependency_name.clone(),
                        declared_path: None,
                        resolved_path: None,
                        resolution: PathDependencyResolution::UnresolvedWorkspaceInheritance,
                        source: PathDependencySource::WorkspaceInherited,
                    }),
                }
            }
            // Table-form entries with neither `path` nor `workspace = true`
            // are registry/git declarations, not path edges.
        }
    };

    for (section_name, section) in [
        ("dependencies", PathDependencySection::Dependencies),
        ("dev-dependencies", PathDependencySection::DevDependencies),
        (
            "build-dependencies",
            PathDependencySection::BuildDependencies,
        ),
    ] {
        if let Some(table) = value.get(section_name).and_then(toml::Value::as_table) {
            collect_section(section, None, table);
        }
    }
    if let Some(targets) = value.get("target").and_then(toml::Value::as_table) {
        for (spec, target_value) in targets {
            let Some(target_value) = target_value.as_table() else {
                continue;
            };
            for (section_name, table) in target_value {
                if let (Some(section), Some(table)) = (
                    PathDependencySection::from_table_name(section_name),
                    table.as_table(),
                ) {
                    collect_section(section, Some(spec), table);
                }
            }
        }
    }
    workspace_inherited_count
}

fn canonical_toml_value(value: &toml::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".to_string())
}
