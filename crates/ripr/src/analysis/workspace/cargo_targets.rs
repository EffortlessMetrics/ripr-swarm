//! Cargo target enumeration for source-role confirmation (#3283).
//!
//! ripr never ran Cargo target discovery: `[[test]]` and `[[bench]]`
//! entries were invisible, so an explicit `path = "src/contract_test.rs"`
//! target could not confirm evidence role outside the default `tests/`
//! and `benches/` layouts. This module reads the package manifests that
//! own the analyzed files and extracts exactly the target declarations
//! the source-role model consumes.
//!
//! Bounded scope, stated so the gate does not overclaim:
//!
//! - Only explicit `path = ...` entries on `[[test]]` and `[[bench]]`
//!   targets are extracted. Autodiscovered targets live under `tests/`
//!   and `benches/` by construction, which the layout rules already
//!   classify.
//! - `autotests = false` / `autobenches = false` only suppress
//!   *autodiscovery*; explicit targets remain valid, so the flags are
//!   read but do not change classification.
//! - Malformed manifests never fail analysis: an unreadable or invalid
//!   `Cargo.toml` yields no declared targets for that package, and the
//!   layout rules keep applying (fail-closed toward production, which is
//!   the conservative direction — over-excluding production files is the
//!   named risk of #3283).
//! - Paths are workspace-relative-resolved against the manifest's
//!   directory, then normalized so Windows and POSIX compare equal.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::source_role::{SourceRoleContext, cargo_discoverable_under};

/// Declared explicit target paths for one workspace, keyed by nothing —
/// a flat set is all the role model needs.
#[derive(Clone, Debug, Default)]
pub(crate) struct DeclaredCargoTargets {
    pub(crate) tests: BTreeSet<PathBuf>,
    pub(crate) benches: BTreeSet<PathBuf>,
}

/// Read `path = ...` entries from the `[[test]]` and `[[bench]]` arrays
/// of one manifest. `manifest_dir` is the directory containing the
/// `Cargo.toml`; declared paths resolve relative to it.
pub(crate) fn declared_targets_from_manifest(
    manifest_text: &str,
    manifest_dir: &Path,
) -> DeclaredCargoTargets {
    let mut targets = DeclaredCargoTargets::default();
    let Ok(value) = toml::from_str::<toml::Value>(manifest_text) else {
        return targets;
    };
    collect_explicit_paths(value.get("test"), manifest_dir, &mut targets.tests);
    collect_explicit_paths(value.get("bench"), manifest_dir, &mut targets.benches);
    targets
}

fn collect_explicit_paths(
    target_entries: Option<&toml::Value>,
    manifest_dir: &Path,
    out: &mut BTreeSet<PathBuf>,
) {
    let Some(entries) = target_entries.and_then(|value| value.as_array()) else {
        return;
    };
    for entry in entries {
        let Some(path) = entry.get("path").and_then(|value| value.as_str()) else {
            continue;
        };
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        out.insert(normalize(&manifest_dir.join(path)));
    }
}

/// One declared Cargo test target with its effective `harness` flag
/// (#3608): the parsed Cargo target metadata the harness-registry
/// validation consumes. `harness` carries Cargo's default (`true`) when
/// the manifest omits the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclaredCargoTestTarget {
    pub(crate) path: PathBuf,
    pub(crate) harness: bool,
}

/// Enumerate every `[[test]]` target of one manifest with its effective
/// `harness` flag (#3608). Two entry shapes contribute:
///
/// - an explicit `path = ...` entry contributes exactly its resolved path;
/// - a name-only entry (no `path`) contributes its default path
///   `tests/<name>.rs` (review HAla: only the file layout; the directory
///   layout `tests/<name>/main.rs` is a separate autodiscovered target
///   governed by the autodiscovery rules), so a registration on the
///   conventional layout still matches the declaration that governs it.
///
/// The flag is the entry's `harness` key when present, Cargo's `true`
/// default otherwise. Entries with neither a name nor a path contribute
/// nothing (fail closed). Malformed manifests yield no targets.
/// Text-taking test form of the parsed-value core; the verdict path
/// parses each manifest once and calls
/// [`declared_test_targets_with_harness_from_value`] directly.
#[cfg(test)]
pub(crate) fn declared_test_targets_with_harness_from_manifest(
    manifest_text: &str,
    manifest_dir: &Path,
) -> Vec<DeclaredCargoTestTarget> {
    match toml::from_str::<toml::Value>(manifest_text) {
        Ok(value) => declared_test_targets_with_harness_from_value(&value, manifest_dir, false),
        Err(_) => Vec::new(),
    }
}

/// The parsed-manifest core of [`declared_test_targets_with_harness_from_manifest`].
/// The verdict path parses each manifest exactly once and reuses the value
/// here (#3608 review). `explicit_only` restricts the result to explicit
/// `path = ...` entries: the ancestor declaration walk matches only those
/// (a name-only entry resolves to autodiscovery shapes under its own
/// manifest and must not claim targets governed by a deeper manifest),
/// while nearest-manifest matching includes both shapes.
fn declared_test_targets_with_harness_from_value(
    value: &toml::Value,
    manifest_dir: &Path,
    explicit_only: bool,
) -> Vec<DeclaredCargoTestTarget> {
    let Some(entries) = value.get("test").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    for entry in entries {
        let harness = entry
            .get("harness")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        if let Some(path) = entry.get("path").and_then(|value| value.as_str()) {
            let path = path.trim();
            if path.is_empty() {
                continue;
            }
            targets.push(DeclaredCargoTestTarget {
                path: lexical(&normalize(&manifest_dir.join(path))),
                harness,
            });
            continue;
        }
        if explicit_only {
            continue;
        }
        let Some(name) = entry.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        // A name-only entry defaults to exactly `tests/<name>.rs` (review
        // HAla): the directory shape `tests/<name>/main.rs` is a separate
        // autodiscovered target whose premise comes from the autodiscovery
        // rules (package presence, edition, `autotests`), not from this
        // entry's `harness` flag.
        targets.push(DeclaredCargoTestTarget {
            path: lexical(&normalize(&manifest_dir.join(format!("tests/{name}.rs")))),
            harness,
        });
    }
    targets
}

/// Cargo's test-autodiscovery default for one parsed manifest (#3608
/// review). An explicit `package.autotests` flag wins. Otherwise the
/// default is `false` only for Cargo's backward-compatibility rule —
/// edition 2015 (explicit or omitted, Cargo's own default) combined with
/// at least one manually declared `[[test]]` target — and `true` in every
/// other combination. `inherited_workspace_edition` carries the
/// `[workspace.package]` edition of the analysis-root manifest: a member
/// declaring `edition.workspace = true` inherits its effective edition
/// from there, and an unresolvable root conservatively keeps the 2015
/// default.
fn test_autodiscovery_default(
    value: &toml::Value,
    inherited_workspace_edition: Option<&str>,
) -> bool {
    let package = value.get("package");
    if let Some(flag) = package
        .and_then(|package| package.get("autotests"))
        .and_then(|value| value.as_bool())
    {
        return flag;
    }
    let raw_edition = package.and_then(|package| package.get("edition"));
    // `edition.workspace = true` parses either as a `{ workspace = true }`
    // table (TOML dotted key) or — defensively — as a bare boolean.
    let edition_is_inherited = match raw_edition {
        Some(toml::Value::Table(table)) => {
            table.get("workspace").and_then(|value| value.as_bool()) == Some(true)
        }
        Some(toml::Value::Boolean(true)) => true,
        _ => false,
    };
    let edition = if edition_is_inherited {
        inherited_workspace_edition.unwrap_or("2015")
    } else {
        raw_edition
            .and_then(|value| value.as_str())
            .unwrap_or("2015")
    };
    let manual_test_target = value
        .get("test")
        .and_then(|value| value.as_array())
        .is_some_and(|entries| !entries.is_empty());
    !(edition == "2015" && manual_test_target)
}

/// The verdict of one registered harness target against the parsed Cargo
/// target metadata of its owning package (#3608).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CargoHarnessVerdict {
    /// The path is a declared `[[test]]` target whose effective
    /// `harness` flag is `false`: the registration's premise holds.
    HarnessDisabled,
    /// The path is a known Cargo test target (explicit entry or package
    /// autodiscovery) whose effective `harness` flag is `true`: the
    /// libtest harness still collects the file, so the `harness = false`
    /// premise of a custom-harness registration does not hold.
    HarnessEnabled,
    /// The parsed manifest declares no Cargo test target for this path:
    /// the target is missing from Cargo metadata.
    NotDeclared,
    /// No premise about the target can be established from metadata: no
    /// manifest resolves for it, a workspace manifest could not be read
    /// or parsed (the declaration map is incomplete), or two manifests
    /// declare the path with conflicting `harness` flags.
    ManifestUnavailable,
}

/// One analysis pass's parsed owning manifests (#3608 review): each
/// manifest is read and parsed at most once per batch, so a batch of
/// registrations in one package costs one manifest parse, not one per
/// registration.
#[derive(Default)]
pub(crate) struct ManifestInventory {
    manifests: BTreeMap<PathBuf, OwnedManifest>,
    workspace_scan: Option<WorkspaceScan>,
}

/// The per-batch workspace manifest inventory (#3608 review): whether
/// every discovered manifest could be read and parsed, and the map of
/// lexically resolved explicit `[[test]] path = ...` declarations to the
/// `harness` flags of every package manifest declaring each path.
#[derive(Default)]
struct WorkspaceScan {
    any_unresolvable: bool,
    declarations: BTreeMap<PathBuf, Vec<bool>>,
}

/// The memoized parse outcome for one manifest directory on the ownership
/// walk.
#[derive(Clone)]
enum OwnedManifest {
    /// No manifest exists at this directory.
    Absent,
    /// The manifest exists but could not be read or parsed.
    Unresolvable,
    Parsed {
        root: PathBuf,
        value: toml::Value,
    },
}

impl ManifestInventory {
    /// The Cargo target metadata verdict for one registered harness
    /// target path. `workspace_root` anchors the ownership resolution;
    /// the registration target is workspace-relative.
    ///
    /// Explicit-target ownership is declaration-driven across the whole
    /// workspace (#3608 review): every parsed package manifest's
    /// `[[test]] path = ...` entries are resolved against their own
    /// manifest directory (lexically, `..` collapsed), and any manifest
    /// declaring the exact normalized target path claims it — so a
    /// sibling package's `../shared/mimic.rs` declaration and a
    /// workspace-root declaration below a nested manifest directory both
    /// resolve. Agreeing declarations are deterministic; conflicting
    /// `harness` flags on one path are ambiguous and fail closed.
    /// Nearest-manifest resolution then governs the autodiscovery
    /// premise alone (package-root `tests/**` shape, package presence,
    /// effective edition, `autotests` flag).
    pub(crate) fn verdict(
        &mut self,
        workspace_root: &Path,
        registration_target: &Path,
    ) -> CargoHarnessVerdict {
        let anchored = lexical(&normalize(&workspace_root.join(registration_target)));
        self.ensure_workspace_scan(workspace_root);
        let scan = match self.workspace_scan.as_ref() {
            Some(scan) => scan,
            None => return CargoHarnessVerdict::ManifestUnavailable,
        };
        if scan.any_unresolvable {
            // The declaration map is incomplete: a manifest that could not
            // be read or parsed may declare this target, so no ownership
            // or autodiscovery premise is provable.
            return CargoHarnessVerdict::ManifestUnavailable;
        }
        if let Some(flags) = scan.declarations.get(&anchored) {
            let harness = flags[0];
            if flags.iter().all(|flag| *flag == harness) {
                return if harness {
                    CargoHarnessVerdict::HarnessEnabled
                } else {
                    CargoHarnessVerdict::HarnessDisabled
                };
            }
            // Two manifests declare the same path with different harness
            // flags; which compilation unit collects the file is not
            // statically decidable here.
            return CargoHarnessVerdict::ManifestUnavailable;
        }
        // Autodiscovery stays a nearest-manifest premise.
        let Some((root, value)) = self.nearest_parsed_manifest(workspace_root, &anchored) else {
            return CargoHarnessVerdict::ManifestUnavailable;
        };
        let inherited_edition = self.workspace_inherited_edition(workspace_root);
        self.verdict_from_parsed(inherited_edition.as_deref(), &anchored, &root, &value)
    }

    /// The first parsed manifest on the anchored target's ancestor chain,
    /// bounded at the workspace root.
    fn nearest_parsed_manifest(
        &mut self,
        workspace_root: &Path,
        anchored: &Path,
    ) -> Option<(PathBuf, toml::Value)> {
        let normalized_root = normalize(workspace_root);
        let mut cursor = anchored.parent();
        while let Some(dir) = cursor {
            let normalized_dir = normalize(dir);
            if !normalized_dir.starts_with(&normalized_root) {
                break;
            }
            if let OwnedManifest::Parsed { root, value } = self.manifest_at(dir) {
                return Some((root, value));
            }
            if normalized_dir == normalized_root {
                break;
            }
            cursor = dir.parent();
        }
        None
    }

    /// Enumerate the workspace's manifests once per batch (the same scan
    /// the cache key and #3616 manifest attribution use) and build the
    /// explicit-declaration map: lexically resolved declared path to the
    /// `harness` flags of every package manifest declaring it. A manifest
    /// without `[package]` contributes nothing (review FhIA).
    fn ensure_workspace_scan(&mut self, workspace_root: &Path) {
        if self.workspace_scan.is_some() {
            return;
        }
        let mut scan = WorkspaceScan::default();
        let normalized_root = normalize(workspace_root);
        // Membership (review HAkg): the analysis-root manifest defines the
        // workspace — its own package when it has `[package]`, plus the
        // declared `[workspace.members]` (globs matched lexically) minus
        // `[workspace.exclude]`, plus package manifests reached through
        // members' regular path dependencies. Manifests outside this
        // member set are not part of the analyzed workspace: their
        // declarations never enter the map and their malformed state is
        // not this workspace's premise. An absent root manifest defines
        // no members at all.
        let mut queue: Vec<PathBuf> = Vec::new();
        let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
        let enqueue = |dir: PathBuf, queue: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>| {
            if !dir.starts_with(&normalized_root) {
                return;
            }
            if seen.insert(dir.clone()) {
                queue.push(dir);
            }
        };
        match self.manifest_at(workspace_root) {
            OwnedManifest::Parsed { value, .. } => {
                if value.get("package").is_some() {
                    enqueue(normalize(workspace_root), &mut queue, &mut seen);
                }
                if let Some(workspace) = value.get("workspace") {
                    let excluded = collect_string_array(workspace.get("exclude"));
                    let mut candidates: Vec<String> = Vec::new();
                    for pattern in collect_string_array(workspace.get("members")) {
                        candidates.extend(expand_member_pattern(workspace_root, &pattern));
                    }
                    candidates.retain(|candidate| {
                        !excluded.iter().any(|excluded_pattern| {
                            workspace_member_glob_matches(excluded_pattern, candidate)
                        })
                    });
                    for candidate in candidates {
                        let dir = normalize(&workspace_root.join(&candidate));
                        enqueue(dir, &mut queue, &mut seen);
                    }
                }
            }
            OwnedManifest::Unresolvable => scan.any_unresolvable = true,
            OwnedManifest::Absent => {}
        }
        let mut index = 0usize;
        while index < queue.len() {
            let dir = queue[index].clone();
            index += 1;
            match self.manifest_at(&dir) {
                // A queued member directory without a manifest is a broken
                // workspace (Cargo rejects it too): the declaration map is
                // incomplete and the scan fails closed (review round five,
                // IZc5). Nonmembers never reach this queue.
                OwnedManifest::Absent => scan.any_unresolvable = true,
                OwnedManifest::Unresolvable => scan.any_unresolvable = true,
                OwnedManifest::Parsed { root, value } => {
                    if value.get("package").is_some() {
                        for target in
                            declared_test_targets_with_harness_from_value(&value, &root, true)
                        {
                            scan.declarations
                                .entry(target.path)
                                .or_default()
                                .push(target.harness);
                        }
                    }
                    for dependency_dir in member_path_dependency_dirs(&value, &root) {
                        enqueue(dependency_dir, &mut queue, &mut seen);
                    }
                }
            }
        }
        self.workspace_scan = Some(scan);
    }

    fn manifest_at(&mut self, dir: &Path) -> OwnedManifest {
        if !dir.join("Cargo.toml").is_file() {
            return OwnedManifest::Absent;
        }
        if let Some(cached) = self.manifests.get(dir) {
            return cached.clone();
        }
        let parsed = match std::fs::read_to_string(dir.join("Cargo.toml"))
            .ok()
            .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
        {
            Some(value) => OwnedManifest::Parsed {
                root: dir.to_path_buf(),
                value,
            },
            None => OwnedManifest::Unresolvable,
        };
        self.manifests.insert(dir.to_path_buf(), parsed.clone());
        parsed
    }

    /// The `[workspace.package]` edition of the analysis-root manifest,
    /// when one exists (#3608 review): member manifests declaring
    /// `edition.workspace = true` inherit their effective edition from
    /// there, and the edition-2015 autodiscovery rule consumes the
    /// effective edition. Bounded to the analysis root's own manifest —
    /// an analysis root below a larger workspace cannot see past its
    /// root, and an absent or unresolvable root keeps the conservative
    /// 2015 default downstream.
    fn workspace_inherited_edition(&mut self, workspace_root: &Path) -> Option<String> {
        match self.manifest_at(workspace_root) {
            OwnedManifest::Parsed { value, .. } => value
                .get("workspace")
                .and_then(|workspace| workspace.get("package"))
                .and_then(|package| package.get("edition"))
                .and_then(|edition| edition.as_str())
                .map(str::to_string),
            _ => None,
        }
    }

    fn verdict_from_parsed(
        &self,
        inherited_workspace_edition: Option<&str>,
        anchored: &Path,
        root: &Path,
        value: &toml::Value,
    ) -> CargoHarnessVerdict {
        // Cargo rejects target tables in virtual manifests (review FhIA):
        // a TOML-valid `[[test]]` in a manifest without `[package]`
        // declares nothing.
        if value.get("package").is_none() {
            return CargoHarnessVerdict::NotDeclared;
        }
        let declared = declared_test_targets_with_harness_from_value(value, root, false);
        for target in &declared {
            if target.path == *anchored {
                return if target.harness {
                    CargoHarnessVerdict::HarnessEnabled
                } else {
                    CargoHarnessVerdict::HarnessDisabled
                };
            }
        }
        // No explicit entry matched. Cargo still knows the target through
        // package autodiscovery when the path has the conventional test
        // shape directly at the package root, autodiscovery is enabled,
        // and the manifest is a package manifest (autodiscovery is a
        // package behavior; a virtual workspace root declares nothing).
        // The index-0 guard keeps nested `src/tests/case.rs` module files
        // out: the shared layout predicate classifies any `tests`
        // component for source-role purposes, but Cargo only ever
        // autodiscovers `tests/**` at the package root.
        let relative_is_root_test_target = anchored
            .strip_prefix(normalize(root))
            .map(|relative| {
                let components = relative.components().collect::<Vec<_>>();
                components
                    .first()
                    .is_some_and(|component| component.as_os_str().to_string_lossy() == "tests")
                    && cargo_discoverable_under(&components, "tests")
            })
            .unwrap_or(false);
        if test_autodiscovery_default(value, inherited_workspace_edition)
            && relative_is_root_test_target
        {
            return CargoHarnessVerdict::HarnessEnabled;
        }
        CargoHarnessVerdict::NotDeclared
    }
}

/// Resolve the Cargo target metadata verdict for one registered harness
/// target path (#3608). Single-target form: constructs a throwaway
/// [`ManifestInventory`]. Batch callers (the registry, the role-grant
/// filter) reuse one inventory so a registration set in one package
/// parses its manifest once. Test-only: production consumers go through
/// [`ManifestInventory`].
#[cfg(test)]
pub(crate) fn cargo_test_target_harness_verdict(
    workspace_root: &Path,
    registration_target: &Path,
) -> CargoHarnessVerdict {
    ManifestInventory::default().verdict(workspace_root, registration_target)
}

/// Crate-root paths declared by one manifest, relative to `manifest_dir`
/// (#3533 review): `path = ...` on `[lib]`, `[[bin]]`, `[[test]]`,
/// `[[bench]]`, and `[[example]]` targets. Rust resolves out-of-line
/// `mod name;` in these files relative to the file's containing directory
/// regardless of its filename, so module composition needs this identity.
/// Entries whose declared path escapes the package directory are dropped
/// rather than trusted. Malformed manifests yield no declared roots.
pub(crate) fn declared_crate_root_paths_from_manifest(
    manifest_text: &str,
    manifest_dir: &Path,
) -> Vec<PathBuf> {
    let Ok(value) = toml::from_str::<toml::Value>(manifest_text) else {
        return Vec::new();
    };
    let normalized_dir = normalize(manifest_dir);
    let mut normalized = BTreeSet::new();
    for key in ["lib", "bin", "test", "bench", "example"] {
        let entries = match value.get(key) {
            // `[lib]` is a single table; the targets are arrays of tables.
            Some(toml::Value::Array(entries)) => entries.iter().collect::<Vec<_>>(),
            Some(table @ toml::Value::Table(_)) => vec![table],
            _ => continue,
        };
        for entry in entries {
            let Some(path) = entry.get("path").and_then(|path| path.as_str()) else {
                continue;
            };
            let path = path.trim();
            if path.is_empty() {
                continue;
            }
            normalized.insert(normalize(&manifest_dir.join(path)));
        }
    }
    normalized
        .into_iter()
        .filter_map(|target| {
            target
                .strip_prefix(&normalized_dir)
                .ok()
                .map(|relative| relative.to_path_buf())
        })
        .filter(|relative| {
            // ParentDir (or absolute/prefix) components escape the package
            // directory: drop the target rather than trust it.
            relative.components().all(|component| {
                matches!(
                    component,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            })
        })
        .collect()
}

/// Build the source-role context for a set of analyzed files: read each
/// distinct owning package manifest once. `files` are workspace-relative
/// discovery identities; `workspace_root` anchors the package-root walk
/// (a relative path alone has no anchor, and reading a bare
/// `Cargo.toml` would consult the process working directory instead of
/// the analyzed workspace). Declared target paths are normalized back to
/// workspace-relative so they compare against discovery identities.
pub(crate) fn context_for_files<'a, I>(workspace_root: &Path, files: I) -> SourceRoleContext
where
    I: IntoIterator<Item = &'a Path>,
{
    let mut manifests: BTreeMap<PathBuf, DeclaredCargoTargets> = BTreeMap::new();
    let mut context = SourceRoleContext::empty();
    for file in files {
        let anchored = workspace_root.join(file);
        let Some(root) = package_root_of(&anchored) else {
            continue;
        };
        if !manifests.contains_key(&root) {
            let targets = std::fs::read_to_string(root.join("Cargo.toml"))
                .map(|text| declared_targets_from_manifest(&text, &root))
                .unwrap_or_default();
            manifests.insert(root.clone(), targets);
        }
        if let Some(targets) = manifests.get(&root) {
            context
                .declared_test_targets
                .extend(strip_root(workspace_root, &targets.tests));
            context
                .declared_bench_targets
                .extend(strip_root(workspace_root, &targets.benches));
        }
    }
    context
}

/// Normalize absolute declared-target paths back to workspace-relative
/// identities (forward-slashed), dropping anything that escapes the
/// workspace root rather than trusting it.
fn strip_root(workspace_root: &Path, targets: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    targets
        .iter()
        .filter_map(|target| {
            let normalized = normalize(target);
            normalized
                .strip_prefix(normalize(workspace_root))
                .ok()
                .map(normalize)
        })
        .collect()
}

/// The package root owning `file`: the nearest ancestor directory that
/// starts a Cargo source layout (`src/`, `tests/`, `benches/`,
/// `examples/` — mirroring `workspace::classify::package_root`).
fn package_root_of(file: &Path) -> Option<PathBuf> {
    let mut parent = file.parent()?;
    loop {
        let name = parent
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if matches!(name.as_str(), "src" | "tests" | "benches" | "examples") {
            return parent.parent().map(Path::to_path_buf);
        }
        parent = parent.parent()?;
    }
}

fn normalize(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

/// Collect the string entries of one optional TOML array value (the
/// `[workspace.members]` / `[workspace.exclude]` shape).
fn collect_string_array(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Expand one `[workspace.members]` glob pattern into the
/// workspace-relative directories it matches (review HAkg). Lexical
/// approximation documented in place of full Cargo glob semantics:
/// `**` matches any number of path components, `*` matches any characters
/// within one component, `?` matches one character; no symlink,
/// canonicalization, or case-folding behavior. Only directories that
/// exist on disk are returned; whether a match actually carries a
/// manifest is decided by the caller.
fn expand_member_pattern(workspace_root: &Path, pattern: &str) -> Vec<String> {
    let normalized = pattern.trim().replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    let components: Vec<&str> = match trimmed {
        "" | "." => Vec::new(),
        _ => trimmed.split('/').collect(),
    };
    let mut matched = Vec::new();
    expand_member_pattern_walk(workspace_root, &components, &mut matched);
    let normalized_root = normalize(workspace_root);
    matched
        .into_iter()
        .filter_map(|dir| {
            normalize(&dir)
                .strip_prefix(&normalized_root)
                .ok()
                .map(|relative| relative.to_string_lossy().to_string())
        })
        .collect()
}

fn expand_member_pattern_walk(base: &Path, components: &[&str], matched: &mut Vec<PathBuf>) {
    match components.split_first() {
        None => matched.push(base.to_path_buf()),
        Some((&"**", rest)) => {
            // `**` spans zero or more directories at this position (review
            // round five, IZb_): match the remainder here, then descend
            // into each child KEEPING the `**` component so arbitrarily
            // deep levels still match. Recursion is bounded by directory
            // depth.
            expand_member_pattern_walk(base, rest, matched);
            let Ok(entries) = std::fs::read_dir(base) else {
                return;
            };
            for entry in entries.flatten() {
                if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                    expand_member_pattern_walk(&entry.path(), components, matched);
                }
            }
        }
        Some((component, rest)) => {
            let Ok(entries) = std::fs::read_dir(base) else {
                return;
            };
            for entry in entries.flatten() {
                if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if workspace_member_glob_matches(component, &name) {
                    expand_member_pattern_walk(&entry.path(), rest, matched);
                }
            }
        }
    }
}

/// Whether one path component matches one glob component: `*` matches any
/// characters, `?` exactly one character, everything else is literal.
fn workspace_member_glob_matches(pattern: &str, name: &str) -> bool {
    match pattern.chars().next() {
        None => name.is_empty(),
        Some('*') => {
            let rest = &pattern[1..];
            (0..=name.chars().count()).any(|skip| {
                let tail: String = name.chars().skip(skip).collect();
                workspace_member_glob_matches(rest, &tail)
            })
        }
        Some('?') => {
            let mut name_chars = name.chars();
            match name_chars.next() {
                None => false,
                Some(_) => workspace_member_glob_matches(&pattern[1..], name_chars.as_str()),
            }
        }
        Some(first) => {
            let mut name_chars = name.chars();
            match name_chars.next() {
                Some(candidate) if candidate == first => {
                    workspace_member_glob_matches(&pattern[first.len_utf8()..], name_chars.as_str())
                }
                _ => false,
            }
        }
    }
}

/// The lexically resolved directories of the parsed manifest's path
/// dependencies across every dependency section Cargo folds into
/// workspace membership — `[dependencies]`, `[dev-dependencies]`,
/// `[build-dependencies]`, and their `[target.*]`-specific forms
/// (verified against `cargo metadata`: a member's dev- or build-path
/// dependency becomes a workspace member). Workspace-inherited
/// (`{ workspace = true }`) dependencies are not resolved in this
/// bounded model and contribute nothing.
fn member_path_dependency_dirs(value: &toml::Value, manifest_dir: &Path) -> Vec<PathBuf> {
    let mut dependency_tables = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = value.get(section).and_then(|value| value.as_table()) {
            dependency_tables.push(dependencies);
        }
    }
    if let Some(targets) = value.get("target").and_then(|value| value.as_table()) {
        for (_, target_table) in targets {
            for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(dependencies) =
                    target_table.get(section).and_then(|value| value.as_table())
                {
                    dependency_tables.push(dependencies);
                }
            }
        }
    }
    dependency_tables
        .iter()
        .flat_map(|dependencies| dependencies.iter())
        .filter_map(|(_, entry)| entry.get("path").and_then(|path| path.as_str()))
        .filter(|path| !path.trim().is_empty())
        .map(|path| lexical(&normalize(&manifest_dir.join(path.trim()))))
        .collect()
}

/// Lexically resolve one normalized path (#3608 review): collapse CurDir
/// components and ParentDir/preceding-segment pairs without touching the
/// filesystem, so a declared path like `generated/../qa/mimic.rs` (or a
/// `../shared/x.rs` sibling declaration) compares equal to its
/// registration spelling. A leading ParentDir chain — the path escaping
/// above its base — is kept as spelled, so outside-root declarations
/// resolve consistently without being silently clamped into the root.
fn lexical(path: &Path) -> PathBuf {
    let mut resolved: Vec<std::path::Component> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => match resolved.last() {
                Some(std::path::Component::Normal(_)) => {
                    resolved.pop();
                }
                // Nothing to pop (base root or a leading `..` chain): keep
                // the escape as spelled; only a prefix/root below it is
                // dropped, since `..` cannot rise above a filesystem root.
                Some(std::path::Component::Prefix(_) | std::path::Component::RootDir) => {}
                _ => resolved.push(component),
            },
            other => resolved.push(other),
        }
    }
    resolved.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Crate-root identity extraction (#3533 review): `[lib]` tables and
    /// `[[bin]]`/`[[test]]`/`[[bench]]`/`[[example]]` arrays contribute
    /// package-relative paths; entries without `path` and escaping paths
    /// contribute nothing.
    #[test]
    fn crate_root_paths_cover_lib_table_and_target_arrays() {
        let manifest = "[package]\nname='x'\nversion='0.1.0'\n\
            \n[lib]\npath='source/root.rs'\n\
            \n[[bin]]\nname='tool'\npath='src/tool.rs'\n\
            \n[[bin]]\nname='autodiscovered'\n\
            \n[[test]]\npath='../escaped.rs'\n\
            \n[[bench]]\npath='benches/perf.rs'\n";
        let roots = declared_crate_root_paths_from_manifest(manifest, Path::new("/ws/pkg-a"));
        assert_eq!(
            roots,
            vec![
                PathBuf::from("benches/perf.rs"),
                PathBuf::from("source/root.rs"),
                PathBuf::from("src/tool.rs"),
            ]
        );
        assert!(
            declared_crate_root_paths_from_manifest("not [ valid toml", Path::new("/ws/pkg-a"))
                .is_empty()
        );
    }

    #[test]
    fn explicit_test_and_bench_paths_are_extracted() {
        let manifest = "[package]\nname='x'\nversion='0.1.0'\n\
            \n[[test]]\nname='contract'\npath='src/contract_test.rs'\n\
            \n[[bench]]\nname='perf'\npath='src/perf.rs'\n\
            \n[[test]]\nname='autodiscovered'\n";
        let targets = declared_targets_from_manifest(manifest, Path::new("/ws/pkg-a"));
        assert!(
            targets
                .tests
                .contains(&PathBuf::from("/ws/pkg-a/src/contract_test.rs"))
        );
        assert!(
            targets
                .benches
                .contains(&PathBuf::from("/ws/pkg-a/src/perf.rs"))
        );
        // Entries without an explicit path contribute nothing: their
        // files live under tests/ by autodiscovery, which layout covers.
        assert_eq!(targets.tests.len(), 1);
        assert_eq!(targets.benches.len(), 1);
    }

    #[test]
    fn malformed_manifests_yield_no_targets() {
        let targets = declared_targets_from_manifest("not [ valid toml", Path::new("/ws"));
        assert!(targets.tests.is_empty());
        assert!(targets.benches.is_empty());
    }

    /// #3608: the harness-flag extraction names every `[[test]]` target and
    /// carries the effective flag (absent key = Cargo's `true` default).
    #[test]
    fn test_targets_with_harness_cover_explicit_name_resolved_and_defaults() {
        let manifest = "[package]\nname='x'\nversion='0.1.0'\n\
        \n[[test]]\nname='contract'\npath='src/contract_test.rs'\nharness=false\n\
        \n[[test]]\nname='plain'\nharness=false\n\
        \n[[test]]\nname='flagged'\nharness=true\n\
        \n[[test]]\nname='defaults_on'\n\
        \n[[test]]\npath='tests/explicit_only.rs'\n";
        let targets =
            declared_test_targets_with_harness_from_manifest(manifest, Path::new("/ws/pkg"));
        let rendered = targets
            .iter()
            .map(|target| {
                (
                    target.path.to_string_lossy().replace('\\', "/"),
                    target.harness,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rendered,
            vec![
                ("/ws/pkg/src/contract_test.rs".to_string(), false),
                ("/ws/pkg/tests/plain.rs".to_string(), false),
                ("/ws/pkg/tests/flagged.rs".to_string(), true),
                ("/ws/pkg/tests/defaults_on.rs".to_string(), true),
                ("/ws/pkg/tests/explicit_only.rs".to_string(), true),
            ],
            "name-only entries default to exactly tests/<name>.rs (review HAla)"
        );
        assert!(
            declared_test_targets_with_harness_from_manifest("not [ valid toml", Path::new("/ws"))
                .is_empty()
        );
    }

    /// #3608: the verdict discriminates declared-harness-false targets from
    /// harness-enabled targets (explicit or autodiscovered), undeclared
    /// paths, and unreadable manifests.
    #[test]
    fn harness_verdict_discriminates_declared_enabled_and_missing_targets() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-verdict-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let write_manifest = |text: &str| -> Result<(), String> {
            std::fs::write(dir.join("pkg/Cargo.toml"), text).map_err(|error| error.to_string())
        };
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        // The analyzed root declares a real workspace whose member is pkg;
        // only member manifests feed the declaration map (review HAkg).
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = ['pkg']\n")
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("pkg/src")).map_err(|error| error.to_string())?;
        // The declared member carries a valid manifest from the start: a
        // declared member without one is a broken workspace (review round
        // five, IZc5).
        write_manifest("[package]\nname='p'\nversion='0.1.0'\nedition='2024'\n")?;
        std::fs::create_dir_all(dir.join("orphan/src")).map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));

        // orphan is not a member and declares nothing: the nearest
        // manifest (the virtual root) declares no target for the path.
        assert_eq!(
            verdict("orphan/src/mimic.rs"),
            CargoHarnessVerdict::NotDeclared
        );

        // Manifest A: autotests disabled, one explicit harness = false target
        // with a custom path, one explicit harness = true name target.
        write_manifest(
            "[package]\nname='p'\nversion='0.1.0'\nautotests=false\n\n\
         [[test]]\nname='custom'\npath='src/contract_test.rs'\nharness=false\n\n\
         [[test]]\nname='enabled'\nharness=true\n",
        )?;
        // Explicit [[test]] with harness = false and a custom path: the
        // custom-harness premise holds.
        assert_eq!(
            verdict("pkg/src/contract_test.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        // Name-resolved autodiscovery shape with an explicit harness = true
        // entry: the target is known and its harness stays enabled.
        assert_eq!(
            verdict("pkg/tests/enabled.rs"),
            CargoHarnessVerdict::HarnessEnabled
        );
        // Conventional tests/ layout without any entry while autotests =
        // false: Cargo does not discover the target.
        assert_eq!(
            verdict("pkg/tests/undiscovered.rs"),
            CargoHarnessVerdict::NotDeclared
        );
        // A typo'd or swapped path matches nothing.
        assert_eq!(
            verdict("pkg/src/contract_tset.rs"),
            CargoHarnessVerdict::NotDeclared
        );
        // loose.rs matches no declaration and no autodiscovery shape at
        // its nearest manifest (the virtual workspace root).
        assert_eq!(verdict("loose.rs"), CargoHarnessVerdict::NotDeclared);

        // Manifest B: plain package, no [[test]] entries — autodiscovery on.
        write_manifest("[package]\nname='p'\nversion='0.1.0'\n")?;
        assert_eq!(
            verdict("pkg/tests/discovered.rs"),
            CargoHarnessVerdict::HarnessEnabled
        );
        // Without the explicit entry the custom-path target is no longer declared.
        assert_eq!(
            verdict("pkg/src/contract_test.rs"),
            CargoHarnessVerdict::NotDeclared
        );

        // Manifest C: an explicit harness = false declaration on the
        // conventional layout (name-only entry) confirms the premise.
        write_manifest(
            "[package]\nname='p'\nversion='0.1.0'\n\n\
         [[test]]\nname='discovered'\nharness=false\n",
        )?;
        assert_eq!(
            verdict("pkg/tests/discovered.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608: a virtual workspace manifest (no `[package]` table) declares no
    /// autodiscovered targets.
    #[test]
    fn virtual_manifest_root_declares_no_autodiscovered_targets() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-virtual-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("tests")).map_err(|error| error.to_string())?;
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = []\n")
            .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("tests/it.rs")),
            CargoHarnessVerdict::NotDeclared
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review (FhIA): Cargo rejects target tables in virtual
    /// manifests, so a TOML-valid `[[test]]` entry in a manifest without
    /// `[package]` declares nothing — the verdict stays NotDeclared even
    /// though the entry would otherwise match.
    #[test]
    fn virtual_manifest_declares_no_targets_even_with_a_test_table() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-virtual-decl-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("tests")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = []\n\n[[test]]\nname = 'mimic'\nharness = false\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("tests/mimic.rs")),
            CargoHarnessVerdict::NotDeclared
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review (Fe25): declaration-driven ownership — a
    /// workspace-root package's explicit `[[test]] path = ...` entry claims
    /// its target even when the path sits below a directory containing
    /// another (undeclaring) Cargo.toml, while nearest-manifest resolution
    /// still governs autodiscovery credit.
    #[test]
    fn workspace_root_declaration_claims_a_target_below_a_nested_manifest() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-root-decl-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("below/nested/manifest/dir"))
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("below/nested/manifest/tests"))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname='ws'\nedition='2024'\n\n[workspace]\nmembers = ['below/nested/manifest']\n\n\
             [[test]]\nname='mimic'\npath='below/nested/manifest/dir/mimic.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        // The nested manifest directory declares nothing for the target.
        std::fs::write(
            dir.join("below/nested/manifest/Cargo.toml"),
            "[package]\nname='nested'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));

        // The root declaration wins for the explicit target.
        assert_eq!(
            verdict("below/nested/manifest/dir/mimic.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        // Nearest-manifest resolution still governs autodiscovery credit:
        // the nested package's own conventional tests/ file is discovered.
        assert_eq!(
            verdict("below/nested/manifest/tests/other.rs"),
            CargoHarnessVerdict::HarnessEnabled
        );

        // Round 4 (Gajt): two manifests declaring the same path with
        // conflicting `harness` flags is ambiguous ownership — which
        // compilation unit collects the file is not statically decidable —
        // so the verdict fails closed instead of picking a winner.
        std::fs::write(
            dir.join("below/nested/manifest/Cargo.toml"),
            "[package]\nname='nested'\nedition='2024'\n\n\
             [[test]]\nname='mimic'\npath='dir/mimic.rs'\nharness=true\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("below/nested/manifest/dir/mimic.rs"),
            CargoHarnessVerdict::ManifestUnavailable,
            "conflicting declarations fail closed"
        );

        // Agreeing declarations remain deterministic.
        std::fs::write(
            dir.join("below/nested/manifest/Cargo.toml"),
            "[package]\nname='nested'\nedition='2024'\n\n\
             [[test]]\nname='mimic'\npath='dir/mimic.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("below/nested/manifest/dir/mimic.rs"),
            CargoHarnessVerdict::HarnessDisabled,
            "agreeing declarations keep the deterministic verdict"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round four (Gajt): Cargo permits an explicit
    /// `[[test]]` path to resolve outside the declaring package's
    /// directory, so a sibling package's `../shared/mimic.rs`
    /// harness = false declaration claims the shared target even though
    /// the declaring manifest is not an ancestor of it; the
    /// nearest-manifest autodiscovery fallback is unchanged for
    /// undeclared paths.
    #[test]
    fn shared_target_declared_from_a_sibling_package_resolves() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-shared-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("crates/a")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("shared")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]
name='ws'
edition='2024'

[workspace]
members = ['crates/a', 'crates/b']
",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("crates/a/Cargo.toml"),
            "[package]
name='a'
edition='2024'

             [[test]]
name='mimic'
path='../../shared/mimic.rs'
harness=false
",
        )
        .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));
        // The sibling package's declaration claims the shared target.
        assert_eq!(
            verdict("shared/mimic.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        // Undeclared sibling paths keep the ordinary fallback.
        assert_eq!(verdict("shared/other.rs"), CargoHarnessVerdict::NotDeclared);

        // Ambiguity: a second package declaring the same shared path with
        // a conflicting harness flag fails closed.
        std::fs::create_dir_all(dir.join("crates/b")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("crates/b/Cargo.toml"),
            "[package]
name='b'
edition='2024'

             [[test]]
name='mimic'
path='../../shared/mimic.rs'
harness=true
",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("shared/mimic.rs"),
            CargoHarnessVerdict::ManifestUnavailable,
            "conflicting declarations of one shared path are ambiguous"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round four (GalA): declared paths and registration
    /// targets compare on the lexically resolved identity — ParentDir and
    /// CurDir segments collapse without touching the filesystem, and a
    /// leading escape chain stays as spelled so outside-root declarations
    /// resolve consistently.
    #[test]
    fn parent_segments_lexically_resolve_on_both_sides() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-lexical-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("pkg")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("shared")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]
name='ws'
edition='2024'

[workspace]
members = ['pkg']
",
        )
        .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));

        // In-package `..` declaration: `generated/../qa/mimic.rs` resolves
        // to `pkg/qa/mimic.rs` (the generated/ directory need not exist).
        std::fs::write(
            dir.join("pkg/Cargo.toml"),
            "[package]
name='p'
edition='2024'

             [[test]]
name='mimic'
path='generated/../qa/mimic.rs'
harness=false
",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("pkg/qa/mimic.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        // The un-collapsed spelling of the same target matches too.
        assert_eq!(
            verdict("pkg/generated/../qa/mimic.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );

        // The reviewer's shape: a sibling `../shared/x.rs` declaration
        // matches a target that spells the same location with a `..`
        // segment.
        std::fs::write(
            dir.join("pkg/Cargo.toml"),
            "[package]
name='p'
edition='2024'

             [[test]]
name='shared'
path='../shared/x.rs'
harness=false
",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("shared/../shared/x.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        assert_eq!(verdict("shared/x.rs"), CargoHarnessVerdict::HarnessDisabled);

        // An escape above the workspace root stays outside: the
        // declaration resolves outside and never claims an in-workspace
        // target spelled as if it were inside.
        std::fs::write(
            dir.join("pkg/Cargo.toml"),
            "[package]
name='p'
edition='2024'

             [[test]]
name='escape'
path='../../outside/x.rs'
harness=false
",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("outside/x.rs"),
            CargoHarnessVerdict::NotDeclared,
            "a root-escaping declaration does not clamp onto an in-workspace path"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review (Fe4d): a member declaring `edition.workspace = true`
    /// inherits the workspace root's `[workspace.package]` edition before
    /// the edition-2015 autodiscovery rule applies — workspace edition
    /// 2024 keeps a second conventional tests/*.rs autodiscovered instead
    /// of degrading to NotDeclared.
    #[test]
    fn workspace_inherited_edition_governs_autodiscovery() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-ws-edition-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("member/tests")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = ['member']\n\n[workspace.package]\nedition = '2024'\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("member/Cargo.toml"),
            "[package]\nname='m'\nversion='0.1.0'\nedition.workspace=true\n\n\
             [[test]]\nname='declared'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));
        assert_eq!(
            verdict("member/tests/declared.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        assert_eq!(
            verdict("member/tests/other.rs"),
            CargoHarnessVerdict::HarnessEnabled,
            "the member inherits edition 2024, so autodiscovery stays on"
        );

        // Without the workspace edition the member's inherited edition is
        // unknown and conservatively keeps the 2015 default.
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = ['member']\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("member/tests/other.rs"),
            CargoHarnessVerdict::NotDeclared,
            "no inherited edition resolves to the conservative 2015 default"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    fn parse_manifest(text: &str) -> Result<toml::Value, String> {
        toml::from_str::<toml::Value>(text).map_err(|error| error.to_string())
    }

    /// #3608 review: Cargo's test-autodiscovery default is `false` only
    /// for the backward-compatibility combination — edition 2015 (explicit
    /// or omitted) with at least one manually declared `[[test]]` target —
    /// and an explicit `package.autotests` flag always wins.
    #[test]
    fn edition_2015_with_manual_test_target_disables_test_autodiscovery() -> Result<(), String> {
        let enabled = |manifest: &str, inherited: Option<&str>| -> Result<bool, String> {
            Ok(test_autodiscovery_default(
                &parse_manifest(manifest)?,
                inherited,
            ))
        };
        // Explicit flags always win.
        assert!(
            !enabled(
                "[package]\nname='p'\nedition='2015'\nautotests=false\n\n[[test]]\nname='a'\n",
                None
            )?,
            "an explicit autotests = false wins over every default"
        );
        assert!(
            enabled(
                "[package]\nname='p'\nedition='2015'\nautotests=true\n\n[[test]]\nname='a'\n",
                None
            )?,
            "an explicit autotests = true wins over the 2015 backward-compatibility default"
        );
        // Backward-compatibility combination: edition 2015 + manual target.
        assert!(
            !enabled(
                "[package]\nname='p'\nedition='2015'\n\n[[test]]\nname='a'\n",
                None
            )?,
            "edition 2015 with a manual [[test]] disables autodiscovery"
        );
        assert!(
            !enabled("[package]\nname='p'\n\n[[test]]\nname='a'\n", None)?,
            "an omitted edition defaults to 2015, so a manual [[test]] disables autodiscovery"
        );
        // Every other combination keeps autodiscovery enabled.
        assert!(
            enabled("[package]\nname='p'\nedition='2015'\n", None)?,
            "edition 2015 without a manual [[test]] keeps autodiscovery"
        );
        assert!(
            enabled(
                "[package]\nname='p'\nedition='2021'\n\n[[test]]\nname='a'\n",
                None
            )?,
            "edition 2021 with a manual [[test]] keeps autodiscovery"
        );
        assert!(
            enabled(
                "[package]\nname='p'\nedition='2024'\n\n[[test]]\nname='a'\n",
                None
            )?,
            "edition 2024 with a manual [[test]] keeps autodiscovery"
        );
        // Workspace inheritance (review Fe4d): `edition.workspace = true`
        // resolves to the inherited effective edition before the rule
        // applies.
        assert!(
            enabled(
                "[package]\nname='p'\nedition.workspace=true\n\n[[test]]\nname='a'\n",
                Some("2024")
            )?,
            "a member inheriting workspace edition 2024 keeps autodiscovery"
        );
        assert!(
            !enabled(
                "[package]\nname='p'\nedition.workspace=true\n\n[[test]]\nname='a'\n",
                Some("2015")
            )?,
            "a member inheriting workspace edition 2015 disables autodiscovery"
        );
        assert!(
            !enabled(
                "[package]\nname='p'\nedition.workspace=true\n\n[[test]]\nname='a'\n",
                None
            )?,
            "an unresolvable workspace root keeps the conservative 2015 default"
        );
        Ok(())
    }

    /// #3608 review: verdict-level pin of the same rule — under edition
    /// 2015 with a manual `[[test]]` entry, the declared entry still
    /// matches (explicit declarations are independent of autodiscovery)
    /// while a sibling conventional-layout file is no longer discovered.
    #[test]
    fn edition_2015_manual_declaration_keeps_explicit_match_and_drops_discovery()
    -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-edition-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("pkg/tests")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("pkg/Cargo.toml"),
            "[package]\nname='p'\nedition='2015'\n\n[[test]]\nname='declared'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));
        assert_eq!(
            verdict("pkg/tests/declared.rs"),
            CargoHarnessVerdict::HarnessDisabled,
            "the explicit declaration matches regardless of the autodiscovery default"
        );
        assert_eq!(
            verdict("pkg/tests/other.rs"),
            CargoHarnessVerdict::NotDeclared,
            "edition 2015 with a manual [[test]] disables autodiscovery of siblings"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review: a readable but malformed owning manifest cannot
    /// establish any premise about its targets — the verdict is
    /// ManifestUnavailable, never a target-typo NotDeclared.
    #[test]
    fn malformed_readable_manifest_is_manifest_unavailable() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-malformed-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("pkg/src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("pkg/tests")).map_err(|error| error.to_string())?;
        std::fs::write(dir.join("pkg/Cargo.toml"), "not [ valid toml")
            .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));
        assert_eq!(
            verdict("pkg/src/mimic.rs"),
            CargoHarnessVerdict::ManifestUnavailable
        );
        assert_eq!(
            verdict("pkg/tests/mimic.rs"),
            CargoHarnessVerdict::ManifestUnavailable,
            "even the autodiscovery premise cannot be established from a malformed manifest"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review: target ownership follows manifest presence, not
    /// source-layout components. A `[[test]] path = "qa/mimic.rs"`
    /// harness = false target outside the conventional directories
    /// resolves against the nearest (deepest) owning manifest, and a
    /// nested workspace's package manifest owns before the workspace root.
    #[test]
    fn nonconventional_directory_target_resolves_to_the_nearest_manifest() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-nonconventional-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("crates/a/qa")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("qa_root")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname='ws'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(dir.join("qa_root/mimic.rs"), "fn trials() {}\n")
            .map_err(|error| error.to_string())?;
        let write_pkg_manifest = |text: &str| -> Result<(), String> {
            std::fs::write(dir.join("crates/a/Cargo.toml"), text).map_err(|error| error.to_string())
        };
        write_pkg_manifest(
            "[package]\nname='a'\nedition='2024'\n\n\
             [[test]]\nname='mimic'\npath='qa/mimic.rs'\nharness=false\n",
        )?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));

        // Longest ownership: the nested package manifest declares the
        // nonconventional target, so the premise holds.
        assert_eq!(
            verdict("crates/a/qa/mimic.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        // The root package's own nonconventional path matches nothing and
        // is not an autodiscovery shape either.
        assert_eq!(
            verdict("qa_root/mimic.rs"),
            CargoHarnessVerdict::NotDeclared
        );

        // Dropping the nested declaration: the nearest manifest (crates/a)
        // no longer declares the target, and the workspace root cannot own
        // it across the nested package boundary.
        write_pkg_manifest("[package]\nname='a'\nedition='2024'\n")?;
        assert_eq!(
            verdict("crates/a/qa/mimic.rs"),
            CargoHarnessVerdict::NotDeclared
        );

        // A top-level nonconventional target resolves against the root
        // manifest when no deeper manifest exists.
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname='ws'\nedition='2024'\n\n[workspace]\nmembers = ['crates/a']\n\n\
             [[test]]\nname='mimic'\npath='qa_root/mimic.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("qa_root/mimic.rs"),
            CargoHarnessVerdict::HarnessDisabled
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review: Cargo never autodiscovers tests below `src/tests/`;
    /// the package-root guard keeps nested module files out of the
    /// autodiscovery premise without changing the shared layout
    /// predicate's source-role behavior.
    #[test]
    fn nested_src_tests_module_file_is_not_an_autodiscovered_target() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-nested-tests-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("pkg/src/tests")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("pkg/tests")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("pkg/Cargo.toml"),
            "[package]\nname='p'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));
        assert_eq!(
            verdict("pkg/src/tests/case.rs"),
            CargoHarnessVerdict::NotDeclared,
            "a module file below src/tests/ is not a package-root test target"
        );
        assert_eq!(
            verdict("pkg/tests/case.rs"),
            CargoHarnessVerdict::HarnessEnabled,
            "the package-root tests/ shape stays autodiscovered"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn autodiscovery_flags_do_not_drop_explicit_targets() {
        let manifest = "[package]\nname='x'\nautotests=false\nautobenches=false\n\
            \n[[test]]\npath='tests/contract.rs'\n[[bench]]\npath='benches/perf.rs'\n";
        let targets = declared_targets_from_manifest(manifest, Path::new("/ws"));
        assert_eq!(targets.tests.len(), 1);
        assert_eq!(targets.benches.len(), 1);
    }

    #[test]
    fn package_root_resolution_covers_source_layouts() {
        assert_eq!(
            package_root_of(Path::new("/ws/pkg-a/src/lib.rs")),
            Some(PathBuf::from("/ws/pkg-a"))
        );
        assert_eq!(
            package_root_of(Path::new("/ws/pkg-a/tests/it.rs")),
            Some(PathBuf::from("/ws/pkg-a"))
        );
        assert_eq!(
            package_root_of(Path::new("/ws/pkg-a/benches/perf.rs")),
            Some(PathBuf::from("/ws/pkg-a"))
        );
        // A file with no Cargo source-layout ancestor has no package root.
        assert_eq!(package_root_of(Path::new("/ws/loose.rs")), None);
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn context_for_files_collects_declared_targets_from_disk() -> Result<(), String> {
        // #3283 discriminating test for the aggregation itself: a real
        // manifest on disk contributes its declared targets, workspace
        // relative, while an unrelated package contributes nothing.
        let dir = std::env::temp_dir().join(format!(
            "ripr-targets-ctx-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("pkg-a/src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(dir.join("pkg-b/src")).map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("pkg-a/Cargo.toml"),
            "[package]\nname='a'\nversion='0.1.0'\n\n[[test]]\npath='src/contract_test.rs'\n[[bench]]\npath='src/perf.rs'\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("pkg-b/Cargo.toml"),
            "[package]\nname='b'\nversion='0.1.0'\n",
        )
        .map_err(|error| error.to_string())?;
        let files = [dir.join("pkg-a/src/lib.rs"), dir.join("pkg-b/src/lib.rs")]
            .iter()
            .map(|path| {
                path.strip_prefix(&dir)
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|_| path.clone())
            })
            .collect::<Vec<_>>();
        let context = context_for_files(
            &dir,
            files.iter().map(|path| path.as_path()).collect::<Vec<_>>(),
        );
        assert!(
            context
                .declared_test_targets
                .contains(&PathBuf::from("pkg-a/src/contract_test.rs"))
        );
        assert!(
            context
                .declared_bench_targets
                .contains(&PathBuf::from("pkg-a/src/perf.rs"))
        );
        assert!(context.production_like_targets.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round five (HAla): a name-only `[[test]]` entry
    /// defaults to exactly `tests/<name>.rs`; the directory layout
    /// `tests/<name>/main.rs` stays governed by the autodiscovery rules
    /// and does not inherit the entry's `harness` flag.
    #[test]
    fn name_only_entry_credits_only_the_file_layout() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-name-only-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("pkg/tests/suite")).map_err(|error| error.to_string())?;
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = ['pkg']\n")
            .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("pkg/Cargo.toml"),
            "[package]\nname='p'\nedition='2024'\n\n[[test]]\nname='suite'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));
        assert_eq!(
            verdict("pkg/tests/suite.rs"),
            CargoHarnessVerdict::HarnessDisabled,
            "the name-only entry defaults to the file layout"
        );
        assert_eq!(
            verdict("pkg/tests/suite/main.rs"),
            CargoHarnessVerdict::HarnessEnabled,
            "the directory layout is a separate autodiscovered target: it does not inherit the flag"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round five (HAkg): membership gating. A malformed
    /// manifest that is NOT a workspace member is skipped — it neither
    /// rejects a valid member registration nor conflicts with it — while
    /// a malformed MEMBER manifest leaves the declaration map incomplete
    /// and fails closed.
    #[test]
    fn membership_gates_the_declaration_map() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-membership-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("member")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("stray")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = ['member']\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("member/Cargo.toml"),
            "[package]\nname='m'\nedition='2024'\n\n[[test]]\nname='mimic'\npath='target.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        // A standalone nested manifest outside the member set is
        // malformed: it is not a member, so it is ignored entirely.
        std::fs::write(dir.join("stray/Cargo.toml"), "not [ valid toml")
            .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));
        assert_eq!(
            verdict("member/target.rs"),
            CargoHarnessVerdict::HarnessDisabled,
            "a malformed nonmember manifest neither rejects nor conflicts"
        );

        // Even a nonmember declaring the same path with a conflicting
        // flag cannot create ambiguity: it is not part of the workspace.
        std::fs::write(
            dir.join("stray/Cargo.toml"),
            "[package]\nname='s'\nedition='2024'\n\n[[test]]\nname='mimic'\npath='../member/target.rs'\nharness=true\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("member/target.rs"),
            CargoHarnessVerdict::HarnessDisabled,
            "nonmember declarations cannot conflict with member targets"
        );

        // A malformed MEMBER manifest leaves the premise unprovable.
        std::fs::write(dir.join("member/Cargo.toml"), "not [ valid toml")
            .map_err(|error| error.to_string())?;
        assert_eq!(
            verdict("member/target.rs"),
            CargoHarnessVerdict::ManifestUnavailable,
            "a malformed member manifest fails closed"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round five (HAkg): regular path dependencies of
    /// members join the member set, so a dependency package's shared
    /// harness declaration is honored.
    #[test]
    fn path_dependency_members_join_the_declaration_map() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-pathdep-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("crates/a")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("crates/b")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("shared")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = ['crates/a']\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("crates/a/Cargo.toml"),
            "[package]\nname='a'\nedition='2024'\n\n[dependencies]\nb = { path = '../b' }\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("crates/b/Cargo.toml"),
            "[package]\nname='b'\nedition='2024'\n\n[[test]]\nname='mimic'\npath='../../shared/mimic.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("shared/mimic.rs")),
            CargoHarnessVerdict::HarnessDisabled,
            "the path-dependency member's declaration is honored"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round five (HAkg): `[workspace.exclude]` removes glob
    /// matches from the member set, and glob expansion (`crates/*`)
    /// honors declared members.
    #[test]
    fn glob_members_honor_excludes() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-glob-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("crates/kept")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("crates/dropped")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("shared")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = ['crates/*']\nexclude = ['crates/dropped']\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("crates/kept/Cargo.toml"),
            "[package]\nname='kept'\nedition='2024'\n\n[[test]]\nname='mimic'\npath='../../shared/mimic.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("shared/mimic.rs")),
            CargoHarnessVerdict::HarnessDisabled,
            "the glob-kept member's declaration is honored"
        );
        // The excluded package declares the same path with a conflicting
        // flag: excluded from the member set, it cannot create ambiguity.
        std::fs::write(
            dir.join("crates/dropped/Cargo.toml"),
            "[package]\nname='dropped'\nedition='2024'\n\n[[test]]\nname='mimic'\npath='../../shared/mimic.rs'\nharness=true\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("shared/mimic.rs")),
            CargoHarnessVerdict::HarnessDisabled,
            "the excluded member's conflicting declaration is ignored"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round five (IZb_): a mid-pattern `**` spans
    /// arbitrarily deep directories — a harness-declaring package several
    /// directories below a `crates/**/pkg` member pattern resolves.
    #[test]
    fn recursive_member_glob_reaches_deep_packages() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-deep-glob-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("crates/x/y/pkg")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("shared")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = ['crates/**/pkg']\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("crates/x/y/pkg/Cargo.toml"),
            "[package]\nname='deep'\nedition='2024'\n\n[[test]]\nname='mimic'\npath='../../../../shared/mimic.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("shared/mimic.rs")),
            CargoHarnessVerdict::HarnessDisabled,
            "the deep package's declaration is honored through the recursive glob"
        );
        // A trailing `**` reaches every depth — and, matching Cargo, a
        // glob match landing on a manifest-less directory is a broken
        // workspace that fails closed (Cargo errors on such members
        // identically). Every matched directory here carries a manifest,
        // so the declaration resolves.
        std::fs::write(
            dir.join("crates/x/Cargo.toml"),
            "[package]\nname='x'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("crates/x/y/Cargo.toml"),
            "[package]\nname='y'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("crates/x/y/nested"))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("crates/x/y/nested/Cargo.toml"),
            "[package]\nname='nested'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = ['crates/x/**']\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("shared/mimic.rs")),
            CargoHarnessVerdict::HarnessDisabled
        );
        // One manifest-less glob match (an in-package src/ directory) is
        // a broken workspace: the verdict fails closed, mirroring Cargo's
        // rejection of such members.
        std::fs::create_dir_all(dir.join("crates/x/y/nested/src"))
            .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("shared/mimic.rs")),
            CargoHarnessVerdict::ManifestUnavailable,
            "a manifest-less glob match fails closed like Cargo's own rejection"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round five (IZc5): a declared member whose manifest
    /// does not exist is a broken workspace — the declaration map is
    /// incomplete and every custom-harness verdict fails closed, even
    /// when another member declares the registered target.
    #[test]
    fn missing_member_manifest_fails_closed() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-absent-member-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("ghost")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("real")).map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = ['ghost', 'real']\n",
        )
        .map_err(|error| error.to_string())?;
        // ghost/ is deliberately left without a Cargo.toml.
        std::fs::write(
            dir.join("real/Cargo.toml"),
            "[package]\nname='real'\nedition='2024'\n\n[[test]]\nname='mimic'\npath='target.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            cargo_test_target_harness_verdict(&dir, Path::new("real/target.rs")),
            CargoHarnessVerdict::ManifestUnavailable,
            "the missing member manifest leaves the workspace incomplete"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// #3608 review round five (IZdo): dev- and build-dependency path
    /// packages join the member set exactly as `cargo metadata` reports,
    /// so their harness declarations are honored even when that
    /// dependency is the only route to the package.
    #[test]
    fn dev_and_build_path_dependencies_join_the_member_set() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-harness-devbuild-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("main")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("devdep")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("builddep")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("shared")).map_err(|error| error.to_string())?;
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = ['main']\n")
            .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("main/Cargo.toml"),
            "[package]\nname='main'\nedition='2024'\n\n[dev-dependencies]\ndevdep = { path = '../devdep' }\n\n[build-dependencies]\nbuilddep = { path = '../builddep' }\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("devdep/Cargo.toml"),
            "[package]\nname='devdep'\nedition='2024'\n\n[[test]]\nname='dev_mimic'\npath='../shared/dev.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dir.join("builddep/Cargo.toml"),
            "[package]\nname='builddep'\nedition='2024'\n\n[[test]]\nname='build_mimic'\npath='../shared/build.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));
        assert_eq!(
            verdict("shared/dev.rs"),
            CargoHarnessVerdict::HarnessDisabled,
            "the dev-dependency member's declaration is honored"
        );
        assert_eq!(
            verdict("shared/build.rs"),
            CargoHarnessVerdict::HarnessDisabled,
            "the build-dependency member's declaration is honored"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
