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
fn member_pattern_selects(pattern: &str, relative: &str) -> Option<bool> {
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
fn segment_matches(pattern: &str, segment: &str) -> bool {
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
fn is_absolute_declared_path(declared: &str) -> bool {
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
fn resolve_repo_relative(base: &str, declared: &str) -> (String, bool) {
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
fn workspace_dependency_path(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn ignore_remove_dir_all(path: &Path) {
        if let Ok(()) = std::fs::remove_dir_all(path) {}
    }

    /// Tiny non-crypto unique-ish suffix for tempdir naming. Avoids
    /// depending on `tempfile` and avoids tests racing each other when
    /// run with `--test-threads`.
    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{}-{:x}", std::process::id(), nanos)
    }

    fn isolated_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ripr-path-deps-{label}-{}", uuid_like()))
    }

    fn write_manifest(root: &Path, relative: &str, text: &str) -> Result<(), String> {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create {}: {err}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|err| format!("write {}: {err}", path.display()))
    }

    #[test]
    fn graph_provenance_reports_local_package_and_feature_facts_without_network()
    -> Result<(), String> {
        let root = isolated_dir("graph-provenance");
        ignore_remove_dir_all(&root);
        std::fs::create_dir_all(root.join("crates/app"))
            .map_err(|err| format!("create workspace: {err}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )
        .map_err(|err| format!("write root manifest: {err}"))?;
        std::fs::write(
            root.join("crates/app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[features]\ndefault = []\nfast = []\n",
        )
        .map_err(|err| format!("write member manifest: {err}"))?;

        let first = workspace_graph_provenance(&root);
        assert_eq!(first.package_graph_status, "complete");
        assert_eq!(first.feature_graph_status, "complete");
        assert!(first.package_graph_hash.is_some());
        assert!(first.feature_graph_hash.is_some());
        assert_eq!(first.external_dependency_graph_status, "unavailable");
        assert!(
            first
                .external_dependency_graph_detail
                .contains("no network")
        );

        std::fs::write(
            root.join("crates/app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[features]\ndefault = []\nslow = []\n",
        )
        .map_err(|err| format!("change feature manifest: {err}"))?;
        let second = workspace_graph_provenance(&root);
        assert_ne!(first.feature_graph_hash, second.feature_graph_hash);
        assert_eq!(first.package_graph_hash, second.package_graph_hash);

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn graph_provenance_names_unavailable_or_malformed_manifests() -> Result<(), String> {
        let root = isolated_dir("graph-provenance-limited");
        ignore_remove_dir_all(&root);
        std::fs::create_dir_all(&root).map_err(|err| format!("create workspace: {err}"))?;
        let missing = workspace_graph_provenance(&root);
        assert_eq!(missing.package_graph_status, "unavailable");
        assert_eq!(missing.feature_graph_status, "unavailable");

        std::fs::write(root.join("Cargo.toml"), "[package\nname = \"broken\"\n")
            .map_err(|err| format!("write malformed manifest: {err}"))?;
        let malformed = workspace_graph_provenance(&root);
        assert_eq!(malformed.package_graph_status, "unavailable");
        assert_eq!(malformed.feature_graph_status, "limited");
        assert!(malformed.package_graph_detail.is_some());
        assert!(malformed.feature_graph_detail.is_some());

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn resolve_repo_relative_normalizes_lexically() {
        assert_eq!(
            resolve_repo_relative("crates/app", "../shared"),
            ("crates/shared".to_string(), false)
        );
        assert_eq!(
            resolve_repo_relative("crates/app", "../../../outside"),
            ("../outside".to_string(), true)
        );
        assert_eq!(
            resolve_repo_relative("", "a/./b/../c"),
            ("a/c".to_string(), false)
        );
        assert_eq!(resolve_repo_relative("a", ".."), (String::new(), false));
        assert_eq!(
            resolve_repo_relative("", "../../deep"),
            ("../../deep".to_string(), true)
        );
    }

    #[test]
    fn path_dependency_edges_capture_package_dev_and_build_sections() -> Result<(), String> {
        let root = isolated_dir("path-dep-sections");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )?;
        write_manifest(
            &root,
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nshared = { path = \"../shared\" }\n\n\
             [dev-dependencies]\ndevhelper = { path = \"../shared\" }\n\n\
             [build-dependencies]\nbuildtool = { path = \"../shared\" }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 3, "expected one edge per section: {edges:?}");
        // Sorted: Dependencies < DevDependencies < BuildDependencies.
        assert_eq!(edges[0].section, PathDependencySection::Dependencies);
        assert_eq!(edges[0].dependency_name, "shared");
        assert_eq!(edges[1].section, PathDependencySection::DevDependencies);
        assert_eq!(edges[1].dependency_name, "devhelper");
        assert_eq!(edges[2].section, PathDependencySection::BuildDependencies);
        assert_eq!(edges[2].dependency_name, "buildtool");
        for edge in edges {
            assert_eq!(edge.from_manifest, "crates/app/Cargo.toml");
            assert_eq!(edge.target, None);
            assert_eq!(edge.declared_path.as_deref(), Some("../shared"));
            assert_eq!(edge.resolved_path.as_deref(), Some("crates/shared"));
            assert_eq!(edge.resolution, PathDependencyResolution::Resolved);
            assert_eq!(edge.source, PathDependencySource::Package);
        }
        assert!(provenance.path_dependency_limitations.is_empty());

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_resolve_workspace_inherited_dependencies() -> Result<(), String> {
        let root = isolated_dir("path-dep-workspace-inherit");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n\n\
             [workspace.dependencies]\nshared = { path = \"crates/shared\" }\nregistry = \"1.0\"\n",
        )?;
        write_manifest(
            &root,
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nshared = { workspace = true }\n\
             unknown = { workspace = true }\nregistry = { workspace = true }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(
            edges.len(),
            2,
            "workspace-inherited path dep plus one unresolved inheritance: {edges:?}"
        );
        // Sorted by dependency_name: "shared" < "unknown".
        let inherited = &edges[0];
        assert_eq!(inherited.dependency_name, "shared");
        assert_eq!(inherited.source, PathDependencySource::WorkspaceInherited);
        // Workspace dependency paths resolve relative to the workspace root.
        assert_eq!(inherited.declared_path.as_deref(), Some("crates/shared"));
        assert_eq!(inherited.resolved_path.as_deref(), Some("crates/shared"));
        assert_eq!(inherited.resolution, PathDependencyResolution::Resolved);

        let unresolved = &edges[1];
        assert_eq!(unresolved.dependency_name, "unknown");
        assert_eq!(
            unresolved.resolution,
            PathDependencyResolution::UnresolvedWorkspaceInheritance
        );
        assert_eq!(unresolved.declared_path, None);
        assert_eq!(unresolved.resolved_path, None);

        // `registry` inherits a version-only workspace entry: a registry
        // inheritance, not a path edge.
        assert!(edges.iter().all(|edge| edge.dependency_name != "registry"));

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn workspace_dependency_path_stops_at_nearest_workspace_authority() {
        let tables = vec![
            (
                String::new(),
                vec![(String::from("shared"), Some(String::from("shared")))],
            ),
            (String::from("crates/inner"), Vec::new()),
        ];
        assert_eq!(
            workspace_dependency_path(&tables, "crates/inner/member", "shared"),
            None,
            "an inner workspace without the entry must not inherit from an outer workspace"
        );
    }

    #[test]
    fn path_dependency_edges_capture_target_specific_sections() -> Result<(), String> {
        let root = isolated_dir("path-dep-target-specific");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )?;
        write_manifest(
            &root,
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [target.'cfg(windows)'.dependencies]\nwindep = { path = \"../shared\" }\n\n\
             [target.x86_64-unknown-linux-gnu.build-dependencies]\nlinuxbuild = { path = \"../shared\" }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 2, "one edge per target table: {edges:?}");
        // Sorted by target text: "cfg(windows)" < "x86_64-...".
        assert_eq!(edges[0].target.as_deref(), Some("cfg(windows)"));
        assert_eq!(edges[0].section, PathDependencySection::Dependencies);
        assert_eq!(edges[0].dependency_name, "windep");
        assert_eq!(edges[1].target.as_deref(), Some("x86_64-unknown-linux-gnu"));
        assert_eq!(edges[1].section, PathDependencySection::BuildDependencies);
        assert_eq!(edges[1].dependency_name, "linuxbuild");
        for edge in edges {
            assert_eq!(edge.resolved_path.as_deref(), Some("crates/shared"));
            assert_eq!(edge.resolution, PathDependencyResolution::Resolved);
        }

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_keep_governed_forms_distinct_and_deterministic() -> Result<(), String>
    {
        let root = isolated_dir("path-dep-governed-forms");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n\n\
             [workspace.dependencies]\nshared = { path = \"crates/shared\" }\n",
        )?;
        write_manifest(
            &root,
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nshared = { path = \"../shared\" }\n\n\
             [dev-dependencies]\nshared = { workspace = true }\n",
        )?;

        let first = workspace_graph_provenance(&root);
        let second = workspace_graph_provenance(&root);
        assert_eq!(
            first.path_dependency_edges, second.path_dependency_edges,
            "edge capture must be deterministic across runs"
        );
        let edges = &first.path_dependency_edges;
        assert_eq!(
            edges.len(),
            2,
            "the same dep under two governed forms is two distinct edges, not a dedup: {edges:?}"
        );
        let mut sorted = edges.clone();
        sorted.sort();
        assert_eq!(*edges, sorted, "edges must be emitted fully sorted");
        // The direct package declaration and the workspace-inherited form
        // stay distinct records with their own sources.
        assert_eq!(edges[0].section, PathDependencySection::Dependencies);
        assert_eq!(edges[0].source, PathDependencySource::Package);
        assert_eq!(edges[1].section, PathDependencySection::DevDependencies);
        assert_eq!(edges[1].source, PathDependencySource::WorkspaceInherited);

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_normalize_separators_spaces_and_non_ascii() -> Result<(), String> {
        let root = isolated_dir("path-dep-portable-identity");
        ignore_remove_dir_all(&root);
        write_manifest(&root, "Cargo.toml", "[workspace]\nmembers = []\n")?;
        write_manifest(
            &root,
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )?;
        // Directory with spaces and non-ASCII; one dep spells its path with a
        // Windows-style separator (TOML `\\` decodes to a single `\`), which
        // the portable identity normalizes to `/`.
        write_manifest(
            &root,
            "crates/mý äpp/Cargo.toml",
            "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nplain = { path = \"../shared\" }\n\
             winstyle = { path = \"..\\\\shared\" }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 2, "{edges:?}");
        for edge in edges {
            assert_eq!(edge.from_manifest, "crates/mý äpp/Cargo.toml");
        }
        // Cargo uses host path semantics: a backslash is a separator on
        // Windows but a legal filename character on Unix.
        #[cfg(windows)]
        for edge in edges {
            assert_eq!(edge.resolved_path.as_deref(), Some("crates/shared"));
            assert_eq!(edge.resolution, PathDependencyResolution::Resolved);
        }
        #[cfg(not(windows))]
        {
            assert_eq!(edges[0].resolved_path.as_deref(), Some("crates/shared"));
            assert_eq!(edges[0].resolution, PathDependencyResolution::Resolved);
            assert_eq!(
                edges[1].resolved_path.as_deref(),
                Some("crates/mý äpp/..\\shared")
            );
            assert_eq!(edges[1].resolution, PathDependencyResolution::TargetMissing);
        }
        // Sorted by dependency_name: "plain" < "winstyle".
        assert_eq!(edges[0].declared_path.as_deref(), Some("../shared"));
        assert_eq!(edges[1].declared_path.as_deref(), Some("..\\shared"));

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edge_to_manifest_less_directory_is_target_missing() -> Result<(), String> {
        // A bare directory cannot satisfy a Cargo path dependency: the
        // producer classifies it TargetMissing so it never becomes a
        // manifest node downstream (#3613 review).
        let root = isolated_dir("path-dep-manifest-less-target");
        ignore_remove_dir_all(&root);
        write_manifest(&root, "Cargo.toml", "[workspace]\nmembers = []\n")?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nempty = { path = \"../empty-dep\" }\n",
        )?;
        std::fs::create_dir_all(root.join("crates/empty-dep")).map_err(|err| err.to_string())?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 1, "{edges:?}");
        assert_eq!(
            edges[0].resolution,
            PathDependencyResolution::TargetMissing,
            "a directory without Cargo.toml must not count as resolved: {edges:?}"
        );

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn path_dependency_helpers_preserve_unix_colons_and_backslashes() {
        assert!(!is_absolute_declared_path("a:b"));
        assert!(!is_absolute_declared_path(r"..\shared"));
        assert_eq!(
            resolve_repo_relative("crates/app", r"..\shared"),
            ("crates/app/..\\shared".to_string(), false)
        );
    }

    #[test]
    fn path_dependency_edges_keep_absolute_paths_explicit() -> Result<(), String> {
        let root = isolated_dir("path-dep-absolute");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )?;
        let windows_path = format!("{}:/shared", 'C');
        let manifest = format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nposix = {{ path = \"/opt/shared\" }}\n             windows = {{ path = \"{windows_path}\" }}\n",
        );
        write_manifest(&root, "crates/app/Cargo.toml", &manifest)?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 2, "{edges:?}");
        assert_eq!(edges[0].dependency_name, "posix");
        assert_eq!(edges[1].dependency_name, "windows");
        assert_eq!(
            edges[0].resolution,
            PathDependencyResolution::UnsupportedAbsolutePath,
            "POSIX absolute paths are absolute on every host"
        );
        assert_eq!(edges[0].resolved_path, None);
        assert_eq!(edges[0].declared_path.as_deref(), Some("/opt/shared"));
        #[cfg(windows)]
        {
            assert_eq!(
                edges[1].resolution,
                PathDependencyResolution::UnsupportedAbsolutePath,
                "drive-rooted paths are absolute on Windows"
            );
            assert_eq!(edges[1].resolved_path, None);
        }
        #[cfg(unix)]
        {
            assert_eq!(
                edges[1].resolution,
                PathDependencyResolution::TargetMissing,
                "a drive-prefixed path is relative on Unix"
            );
            let unix_resolved = format!("crates/app/{windows_path}");
            assert_eq!(
                edges[1].resolved_path.as_deref(),
                Some(unix_resolved.as_str())
            );
        }
        assert_eq!(
            edges[1].declared_path.as_deref(),
            Some(windows_path.as_str())
        );

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_mark_outside_workspace_and_missing_targets() -> Result<(), String> {
        let root = isolated_dir("path-dep-resolution-status");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nghost = { path = \"../ghost\" }\noutside = { path = \"../../../outside\" }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 2, "{edges:?}");
        // Sorted by dependency_name: "ghost" < "outside".
        assert_eq!(edges[0].dependency_name, "ghost");
        assert_eq!(edges[0].resolved_path.as_deref(), Some("crates/ghost"));
        assert_eq!(edges[0].resolution, PathDependencyResolution::TargetMissing);
        assert_eq!(edges[1].dependency_name, "outside");
        assert_eq!(edges[1].resolved_path.as_deref(), Some("../outside"));
        assert_eq!(
            edges[1].resolution,
            PathDependencyResolution::ResolvedOutsideWorkspace
        );

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_preserve_cycles() -> Result<(), String> {
        let root = isolated_dir("path-dep-cycle");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n",
        )?;
        write_manifest(
            &root,
            "crates/a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nb = { path = \"../b\" }\n",
        )?;
        write_manifest(
            &root,
            "crates/b/Cargo.toml",
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\na = { path = \"../a\" }\n",
        )?;

        let edges = workspace_graph_provenance(&root).path_dependency_edges;
        assert_eq!(edges.len(), 2, "{edges:?}");
        assert_eq!(edges[0].from_manifest, "crates/a/Cargo.toml");
        assert_eq!(edges[0].dependency_name, "b");
        assert_eq!(edges[1].from_manifest, "crates/b/Cargo.toml");
        assert_eq!(edges[1].dependency_name, "a");
        assert!(edges.iter().all(|edge| {
            edge.resolution == PathDependencyResolution::Resolved && edge.resolved_path.is_some()
        }));

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn path_dependency_edges_skip_non_utf8_manifest_with_explicit_limitation() -> Result<(), String>
    {
        use std::os::unix::ffi::OsStrExt;
        let root = isolated_dir("path-dep-non-utf8");
        ignore_remove_dir_all(&root);
        write_manifest(&root, "Cargo.toml", "[workspace]\nmembers = []\n")?;
        let bad_dir = root.join(std::ffi::OsStr::from_bytes(b"bad\xffdir"));
        std::fs::create_dir_all(&bad_dir).map_err(|err| format!("create bad dir: {err}"))?;
        // The `[workspace]` table also exercises the authority-extraction
        // boundary: a non-UTF-8 workspace root is not an inheritance
        // authority either.
        std::fs::write(
            bad_dir.join("Cargo.toml"),
            "[workspace]\nmembers = []\n\n\
             [package]\nname = \"bad\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nshared = { path = \"../shared\" }\n",
        )
        .map_err(|err| format!("write bad manifest: {err}"))?;

        let provenance = workspace_graph_provenance(&root);
        assert!(
            provenance.path_dependency_edges.is_empty(),
            "a non-UTF-8 manifest path must not contribute lossy edges: {:?}",
            provenance.path_dependency_edges
        );
        assert_eq!(
            provenance.path_dependency_limitations.len(),
            1,
            "the capture gap must be an explicit limitation: {:?}",
            provenance.path_dependency_limitations
        );
        let limitation = &provenance.path_dependency_limitations[0];
        assert_eq!(
            limitation.kind,
            PathDependencyLimitationKind::NonUtf8ManifestPath
        );
        assert!(limitation.manifest.is_empty());
        assert!(limitation.detail.contains("non-UTF-8"));

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn package_graph_hash_excludes_path_dependency_edges() -> Result<(), String> {
        let root = isolated_dir("path-dep-hash-exclusion");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )?;
        for member in ["a", "b"] {
            write_manifest(
                &root,
                &format!("crates/{member}/Cargo.toml"),
                &format!("[package]\nname = \"{member}\"\nversion = \"0.1.0\"\n"),
            )?;
        }
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nshared = { path = \"../a\" }\n",
        )?;

        let first = workspace_graph_provenance(&root);
        assert_eq!(first.path_dependency_edges.len(), 1);
        assert_eq!(
            first.path_dependency_edges[0].resolved_path.as_deref(),
            Some("crates/a")
        );

        // Changing only the path value (same dep name) must change the edges
        // but not the package graph hash: the field is additive and excluded.
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nshared = { path = \"../b\" }\n",
        )?;
        let second = workspace_graph_provenance(&root);
        assert_eq!(
            first.package_graph_hash, second.package_graph_hash,
            "path-dependency edges must not feed package_graph_hash"
        );
        assert_ne!(first.path_dependency_edges, second.path_dependency_edges);

        // A target-specific path dep is likewise excluded from hash inputs.
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nshared = { path = \"../b\" }\n\n\
             [target.'cfg(unix)'.dependencies]\nextra = { path = \"../a\" }\n",
        )?;
        let third = workspace_graph_provenance(&root);
        assert_eq!(second.package_graph_hash, third.package_graph_hash);
        assert_eq!(third.path_dependency_edges.len(), 2);

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn unparsed_manifest_outside_every_member_pattern_is_not_a_limitation() -> Result<(), String> {
        let root = isolated_dir("path-dep-unparsed-outside");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(&root, "tests/fixture/in/Cargo.toml", "[package\nname = ")?;

        let provenance = workspace_graph_provenance(&root);
        assert!(
            provenance.path_dependency_limitations.is_empty(),
            "a fixture manifest outside every member pattern is outside the graph: {:?}",
            provenance.path_dependency_limitations
        );

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn unparsed_manifest_that_may_be_in_the_graph_stays_a_limitation() -> Result<(), String> {
        let root = isolated_dir("path-dep-unparsed-inside");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )?;
        // Selected by `crates/*`.
        write_manifest(&root, "crates/broken/Cargo.toml", "[package\nname = ")?;
        // Reached by a path dependency of a parsed member.
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nvendored = { path = \"../../vendor/dep\" }\n",
        )?;
        write_manifest(&root, "vendor/dep/Cargo.toml", "[package\nname = ")?;

        let provenance = workspace_graph_provenance(&root);
        let manifests: Vec<&str> = provenance
            .path_dependency_limitations
            .iter()
            .map(|limitation| limitation.manifest.as_str())
            .collect();
        assert_eq!(
            manifests,
            vec!["crates/broken/Cargo.toml", "vendor/dep/Cargo.toml"]
        );

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn unparsed_manifest_stays_a_limitation_when_any_boundary_is_unproven() -> Result<(), String> {
        let root = isolated_dir("path-dep-unparsed-unproven");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\n\n\
             [patch.crates-io]\npatched = { path = \"vendor/patched\" }\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )?;
        // Nested workspace that selects `m` although the root does not.
        write_manifest(
            &root,
            "tools/ws/Cargo.toml",
            "[workspace]\nmembers = [\"m\"]\n",
        )?;
        write_manifest(&root, "tools/ws/m/Cargo.toml", "[package\nname = ")?;
        // `[patch]` target.
        write_manifest(&root, "vendor/patched/Cargo.toml", "[package\nname = ")?;
        // Unparsed manifest that may itself be a workspace root.
        write_manifest(&root, "sub/Cargo.toml", "[workspace\nmembers = ")?;
        write_manifest(
            &root,
            "sub/inner/Cargo.toml",
            "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n",
        )?;
        let provenance = workspace_graph_provenance(&root);
        let manifests: Vec<&str> = provenance
            .path_dependency_limitations
            .iter()
            .map(|limitation| limitation.manifest.as_str())
            .collect();
        assert_eq!(
            manifests,
            vec![
                "sub/Cargo.toml",
                "tools/ws/m/Cargo.toml",
                "vendor/patched/Cargo.toml"
            ]
        );

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unparsed_manifest_reached_through_a_symlink_or_backslash_spelling_stays_a_limitation()
    -> Result<(), String> {
        let root = isolated_dir("path-dep-unparsed-spellings");
        ignore_remove_dir_all(&root);
        write_manifest(&root, "Cargo.toml", "[workspace]\nmembers = [\"app\"]\n")?;
        write_manifest(
            &root,
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nlinked = { path = \"../linked\" }\nodd = { path = \"../odd\\\\dir\" }\n",
        )?;
        write_manifest(&root, "vendor/bad/Cargo.toml", "[package\nname = ")?;
        write_manifest(&root, "odd/dir/Cargo.toml", "[package\nname = ")?;
        std::os::unix::fs::symlink(root.join("vendor/bad"), root.join("linked"))
            .map_err(|err| err.to_string())?;

        let provenance = workspace_graph_provenance(&root);
        let manifests: Vec<&str> = provenance
            .path_dependency_limitations
            .iter()
            .map(|limitation| limitation.manifest.as_str())
            .collect();
        assert_eq!(
            manifests,
            vec!["odd/dir/Cargo.toml", "vendor/bad/Cargo.toml"]
        );

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn unparsed_manifest_without_any_ancestor_workspace_stays_a_limitation() -> Result<(), String> {
        let root = isolated_dir("path-dep-unparsed-no-workspace");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(&root, "other/Cargo.toml", "[package\nname = ")?;

        let provenance = workspace_graph_provenance(&root);
        assert_eq!(provenance.path_dependency_limitations.len(), 1);

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn unparsed_manifest_is_kept_when_member_syntax_is_not_modeled() {
        for pattern in ["crates/**", "crates/[ab]*", "crates/?", "../x", "/abs"] {
            assert_eq!(member_pattern_selects(pattern, "tests/fixture"), None);
        }
        assert_eq!(
            member_pattern_selects("crates/*", "tests/fixture"),
            Some(false)
        );
        assert_eq!(member_pattern_selects("crates/*", "crates/app"), Some(true));
        assert_eq!(
            member_pattern_selects("crates/a*p", "crates/app"),
            Some(true)
        );
        assert_eq!(
            member_pattern_selects("crates/*", "crates/a/b"),
            Some(false)
        );
        assert!(!segment_matches("a*b*c", "abxbd"));
        assert!(segment_matches("a*b*c", "axbyc"));
    }

    #[test]
    fn path_dependency_edges_ignore_registry_string_and_table_forms() -> Result<(), String> {
        let root = isolated_dir("path-dep-registry-forms");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nstringdep = \"1.0\"\ntabledep = { version = \"1.0\", features = [\"x\"] }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        assert!(
            provenance.path_dependency_edges.is_empty(),
            "registry declarations are outside the path-edge denominator: {:?}",
            provenance.path_dependency_edges
        );
        assert!(provenance.path_dependency_limitations.is_empty());

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_workspace_inherited_boundary_statuses() -> Result<(), String> {
        let root = isolated_dir("path-dep-inherited-boundaries");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n\n\
             [workspace.dependencies]\n\
             abs = { path = \"/opt/shared\" }\n\
             outside = { path = \"../outside-ws\" }\n\
             ghost = { path = \"missing/dir\" }\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\n\
             abs = { workspace = true }\n\
             outside = { workspace = true }\n\
             ghost = { workspace = true }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 3, "{edges:?}");
        // Sorted by dependency_name: "abs" < "ghost" < "outside".
        assert_eq!(edges[0].dependency_name, "abs");
        assert_eq!(edges[0].declared_path.as_deref(), Some("/opt/shared"));
        assert_eq!(edges[0].resolved_path, None);
        assert_eq!(
            edges[0].resolution,
            PathDependencyResolution::UnsupportedAbsolutePath
        );
        assert_eq!(edges[1].dependency_name, "ghost");
        assert_eq!(edges[1].resolved_path.as_deref(), Some("missing/dir"));
        assert_eq!(edges[1].resolution, PathDependencyResolution::TargetMissing);
        assert_eq!(edges[2].dependency_name, "outside");
        assert_eq!(edges[2].resolved_path.as_deref(), Some("../outside-ws"));
        assert_eq!(
            edges[2].resolution,
            PathDependencyResolution::ResolvedOutsideWorkspace
        );
        for edge in edges {
            assert_eq!(edge.source, PathDependencySource::WorkspaceInherited);
        }
        assert!(provenance.path_dependency_limitations.is_empty());

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_workspace_true_without_workspace_authority() -> Result<(), String> {
        let root = isolated_dir("path-dep-no-authority");
        ignore_remove_dir_all(&root);
        // No `[workspace]` table anywhere: the ancestor walk must terminate at
        // the scan root instead of fabricating an authority.
        write_manifest(
            &root,
            "Cargo.toml",
            "[package]\nname = \"solo\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\ndep = { workspace = true }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 1, "{edges:?}");
        assert_eq!(edges[0].dependency_name, "dep");
        assert_eq!(
            edges[0].resolution,
            PathDependencyResolution::UnresolvedWorkspaceInheritance
        );
        assert_eq!(edges[0].declared_path, None);
        assert_eq!(edges[0].resolved_path, None);
        assert!(provenance.path_dependency_limitations.is_empty());

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_treat_path_plus_workspace_as_invalid() -> Result<(), String> {
        let root = isolated_dir("path-dep-invalid-declaration");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n\n\
             [workspace.dependencies]\nboth = { path = \"crates/shared\" }\n",
        )?;
        write_manifest(
            &root,
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )?;
        // Cargo rejects `path` + `workspace = true` when the name is absent
        // from `[workspace.dependencies]` and ignores the member's own `path`
        // otherwise, so the explicit `path` must never win. `both` has a
        // workspace entry; `other` does not — both are invalid declarations.
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\n\
             both = { path = \"../shared\", workspace = true }\n\
             other = { path = \"../shared\", workspace = true }\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 2, "{edges:?}");
        for edge in edges {
            assert_eq!(
                edge.resolution,
                PathDependencyResolution::InvalidDeclaration
            );
            assert_eq!(edge.resolved_path, None);
            assert_eq!(edge.declared_path.as_deref(), Some("../shared"));
            assert_eq!(edge.source, PathDependencySource::Package);
        }
        assert_eq!(edges[0].dependency_name, "both");
        assert_eq!(edges[1].dependency_name, "other");
        assert!(provenance.path_dependency_limitations.is_empty());

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_flag_unfollowed_workspace_redirect() -> Result<(), String> {
        let root = isolated_dir("path-dep-workspace-redirect");
        ignore_remove_dir_all(&root);
        // The nearest ancestor workspace root has no `[workspace.dependencies]`
        // entry for `shared`; the redirect target does. Not following the
        // redirect must be a typed limitation, not a silent miss.
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\", \"crates/plain\"]\n",
        )?;
        write_manifest(
            &root,
            "other-root/Cargo.toml",
            "[workspace]\nmembers = []\n\n\
             [workspace.dependencies]\nshared = { path = \"other-shared\" }\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nworkspace = \"../../other-root\"\n\n\
             [dependencies]\nshared = { workspace = true }\n",
        )?;
        // A redirect without `workspace = true` dependencies is irrelevant to
        // the path-edge inventory and must not produce a limitation.
        write_manifest(
            &root,
            "crates/plain/Cargo.toml",
            "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nworkspace = \"../..\"\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(edges.len(), 1, "{edges:?}");
        assert_eq!(edges[0].from_manifest, "crates/app/Cargo.toml");
        assert_eq!(edges[0].dependency_name, "shared");
        assert_eq!(
            edges[0].resolution,
            PathDependencyResolution::UnresolvedWorkspaceInheritance
        );

        let limitations = &provenance.path_dependency_limitations;
        assert_eq!(limitations.len(), 1, "{limitations:?}");
        assert_eq!(limitations[0].manifest, "crates/app/Cargo.toml");
        assert_eq!(
            limitations[0].kind,
            PathDependencyLimitationKind::UnfollowedWorkspaceRedirect
        );
        assert!(limitations[0].detail.contains("../../other-root"));

        ignore_remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn path_dependency_edges_cover_target_dev_dependencies_and_skip_malformed_target_tables()
    -> Result<(), String> {
        let root = isolated_dir("path-dep-target-boundaries");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\"]\n",
        )?;
        write_manifest(
            &root,
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [target.'cfg(unix)'.dev-dependencies]\ndevt = { path = \"../shared\" }\n\n\
             [target.'cfg(bogus)'.not-a-section]\nghost = { path = \"../shared\" }\n\n\
             [target.'cfg(plain)']\ndependencies = \"not-a-table\"\n\n\
             [target]\n'cfg(z)' = \"plain\"\n",
        )?;

        let provenance = workspace_graph_provenance(&root);
        let edges = &provenance.path_dependency_edges;
        assert_eq!(
            edges.len(),
            1,
            "only the governed target dev-dependency is an edge: {edges:?}"
        );
        assert_eq!(edges[0].dependency_name, "devt");
        assert_eq!(edges[0].section, PathDependencySection::DevDependencies);
        assert_eq!(edges[0].target.as_deref(), Some("cfg(unix)"));
        assert_eq!(edges[0].resolved_path.as_deref(), Some("crates/shared"));
        assert_eq!(edges[0].resolution, PathDependencyResolution::Resolved);
        assert!(provenance.path_dependency_limitations.is_empty());

        ignore_remove_dir_all(&root);
        Ok(())
    }

    /// Differential proof against Cargo itself (#3037): the resolver's typed
    /// edge set for a synthetic workspace must agree with `cargo metadata`
    /// on every supported path-dependency form. Skips (does not fail) when no
    /// `cargo` binary is available.
    #[test]
    fn path_dependency_edges_match_cargo_metadata() -> Result<(), String> {
        let cargo_available = std::process::Command::new("cargo")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !cargo_available {
            // Fail closed: an unobserved differential must not report as a
            // pass with zero compared subjects. Environments that genuinely
            // lack a cargo binary opt out explicitly.
            if std::env::var_os("RIPR_ALLOW_MISSING_CARGO").is_none() {
                return Err(
                    "cargo metadata differential not_run: no cargo binary available; set \
                     RIPR_ALLOW_MISSING_CARGO=1 to accept an unobserved differential"
                        .to_string(),
                );
            }
            eprintln!("skipping cargo metadata differential: cargo binary unavailable");
            return Ok(());
        }

        let root = isolated_dir("path-dep-cargo-differential");
        ignore_remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/app\", \"crates/shared\"]\n\n\
             [workspace.dependencies]\nwsdep = { path = \"crates/shared\" }\n",
        )?;
        write_manifest(
            &root,
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(
            &root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\ndirect = { path = \"../shared\" }\nwsdep = { workspace = true }\n\n\
             [dev-dependencies]\ndevd = { path = \"../shared\" }\n\n\
             [build-dependencies]\nbuildd = { path = \"../shared\" }\n\n\
             [target.'cfg(unix)'.dependencies]\ntgt = { path = \"../shared\" }\n",
        )?;
        // `cargo metadata` requires each package to declare a target.
        for member in ["crates/app", "crates/shared"] {
            let src = root.join(member).join("src");
            std::fs::create_dir_all(&src)
                .map_err(|err| format!("create {}: {err}", src.display()))?;
            std::fs::write(src.join("lib.rs"), "").map_err(|err| format!("write lib.rs: {err}"))?;
        }

        let output = std::process::Command::new("cargo")
            .args([
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--offline",
            ])
            .arg("--manifest-path")
            .arg(root.join("Cargo.toml"))
            .output()
            .map_err(|err| format!("run cargo metadata: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|err| format!("parse cargo metadata: {err}"))?;

        // Cargo's view: every `path` dependency of package `app`, keyed by the
        // manifest key (`rename` or name), with kind and target scope.
        let mut expected: BTreeSet<(String, String, Option<String>)> = BTreeSet::new();
        let mut expected_paths: Vec<(String, String)> = Vec::new();
        let packages = metadata
            .get("packages")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "cargo metadata has no packages array".to_string())?;
        let app = packages
            .iter()
            .find(|package| package.get("name").and_then(serde_json::Value::as_str) == Some("app"))
            .ok_or_else(|| "cargo metadata has no app package".to_string())?;
        let dependencies = app
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "app package has no dependencies array".to_string())?;
        for dep in dependencies {
            let Some(path) = dep.get("path").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let name = dep
                .get("rename")
                .and_then(serde_json::Value::as_str)
                .or_else(|| dep.get("name").and_then(serde_json::Value::as_str))
                .ok_or_else(|| "dependency without a name".to_string())?;
            let kind = dep
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("normal")
                .to_string();
            let target = dep
                .get("target")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            expected.insert((name.to_string(), kind, target));
            expected_paths.push((name.to_string(), path.replace('\\', "/")));
        }
        assert_eq!(
            expected.len(),
            5,
            "the fixture must exercise all five supported forms: {expected:?}"
        );

        // The resolver's view for the same manifest.
        let provenance = workspace_graph_provenance(&root);
        let actual: BTreeSet<(String, String, Option<String>)> = provenance
            .path_dependency_edges
            .iter()
            .filter(|edge| edge.from_manifest == "crates/app/Cargo.toml")
            .map(|edge| {
                let kind = match edge.section {
                    PathDependencySection::Dependencies => "normal",
                    PathDependencySection::DevDependencies => "dev",
                    PathDependencySection::BuildDependencies => "build",
                };
                (
                    edge.dependency_name.clone(),
                    kind.to_string(),
                    edge.target.clone(),
                )
            })
            .collect();
        assert_eq!(
            actual, expected,
            "resolver edge set must agree with cargo metadata (name, kind, target)"
        );

        // Every edge Cargo reports must also agree on the resolved directory
        // identity and carry no capture limitation.
        for edge in &provenance.path_dependency_edges {
            assert_eq!(
                edge.resolution,
                PathDependencyResolution::Resolved,
                "{edge:?}"
            );
            let resolved = edge
                .resolved_path
                .as_deref()
                .ok_or_else(|| format!("edge without resolved path: {edge:?}"))?;
            let (_, cargo_path) = expected_paths
                .iter()
                .find(|(name, _)| name == &edge.dependency_name)
                .ok_or_else(|| format!("no cargo path for {}", edge.dependency_name))?;
            assert!(
                cargo_path == resolved || cargo_path.ends_with(&format!("/{resolved}")),
                "cargo path {cargo_path} must end with resolver identity {resolved}"
            );
        }
        assert!(provenance.path_dependency_limitations.is_empty());

        ignore_remove_dir_all(&root);
        Ok(())
    }
}
