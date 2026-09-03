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
/// - a name-only entry (no `path`) contributes the two autodiscovery
///   shapes Cargo resolves the name to (`tests/<name>.rs` and
///   `tests/<name>/main.rs`), so a registration on the conventional
///   layout still matches the declaration that governs it.
///
/// The flag is the entry's `harness` key when present, Cargo's `true`
/// default otherwise. Entries with neither a name nor a path contribute
/// nothing (fail closed). Malformed manifests yield no targets.
pub(crate) fn declared_test_targets_with_harness_from_manifest(
    manifest_text: &str,
    manifest_dir: &Path,
) -> Vec<DeclaredCargoTestTarget> {
    let Ok(value) = toml::from_str::<toml::Value>(manifest_text) else {
        return Vec::new();
    };
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
                path: normalize(&manifest_dir.join(path)),
                harness,
            });
            continue;
        }
        let Some(name) = entry.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        for candidate in [format!("tests/{name}.rs"), format!("tests/{name}/main.rs")] {
            targets.push(DeclaredCargoTestTarget {
                path: normalize(&manifest_dir.join(candidate)),
                harness,
            });
        }
    }
    targets
}

/// Whether the manifest's `[package]` table disables test autodiscovery
/// via `autotests = false`. Absent key (or absent table) keeps Cargo's
/// enabled default.
fn test_autodiscovery_enabled(manifest_text: &str) -> bool {
    let Ok(value) = toml::from_str::<toml::Value>(manifest_text) else {
        return true;
    };
    value
        .get("package")
        .and_then(|package| package.get("autotests"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
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
    /// The owning package manifest is missing, unreadable, or malformed,
    /// so no premise about the target can be established from metadata.
    ManifestUnavailable,
}

/// Resolve the Cargo target metadata verdict for one registered harness
/// target path (#3608). Reuses this module's package-root walk and
/// manifest read — the same loading `context_for_files` performs — rather
/// than a second discovery walk. The registration target is
/// workspace-relative; it is anchored at `workspace_root` and resolved
/// against the manifest of the nearest owning package.
pub(crate) fn cargo_test_target_harness_verdict(
    workspace_root: &Path,
    registration_target: &Path,
) -> CargoHarnessVerdict {
    let anchored = normalize(&workspace_root.join(registration_target));
    let Some(root) = package_root_of(&anchored) else {
        return CargoHarnessVerdict::NotDeclared;
    };
    let Ok(manifest_text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return CargoHarnessVerdict::ManifestUnavailable;
    };
    let declared = declared_test_targets_with_harness_from_manifest(&manifest_text, &root);
    for target in &declared {
        if target.path == anchored {
            return if target.harness {
                CargoHarnessVerdict::HarnessEnabled
            } else {
                CargoHarnessVerdict::HarnessDisabled
            };
        }
    }
    // No explicit entry matched. Cargo still knows the target through
    // package autodiscovery when the path has the conventional test
    // shape, autotests are not disabled, and the manifest is a package
    // manifest (autodiscovery is a package behavior; a virtual workspace
    // root declares nothing).
    let package_table_present = toml::from_str::<toml::Value>(&manifest_text)
        .ok()
        .is_some_and(|value| value.get("package").is_some());
    let relative_matches = anchored
        .strip_prefix(normalize(&root))
        .map(|relative| {
            cargo_discoverable_under(&relative.components().collect::<Vec<_>>(), "tests")
        })
        .unwrap_or(false);
    if package_table_present && test_autodiscovery_enabled(&manifest_text) && relative_matches {
        return CargoHarnessVerdict::HarnessEnabled;
    }
    CargoHarnessVerdict::NotDeclared
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
                ("/ws/pkg/tests/plain/main.rs".to_string(), false),
                ("/ws/pkg/tests/flagged.rs".to_string(), true),
                ("/ws/pkg/tests/flagged/main.rs".to_string(), true),
                ("/ws/pkg/tests/defaults_on.rs".to_string(), true),
                ("/ws/pkg/tests/defaults_on/main.rs".to_string(), true),
                ("/ws/pkg/tests/explicit_only.rs".to_string(), true),
            ]
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
        std::fs::create_dir_all(dir.join("pkg/src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(dir.join("orphan/src")).map_err(|error| error.to_string())?;
        let verdict = |relative: &str| cargo_test_target_harness_verdict(&dir, Path::new(relative));

        // No manifest at the package root: the premise cannot be established.
        assert_eq!(
            verdict("orphan/src/mimic.rs"),
            CargoHarnessVerdict::ManifestUnavailable
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
        // A path outside any package source layout has no owning manifest.
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
}
