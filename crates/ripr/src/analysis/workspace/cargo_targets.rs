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

use super::source_role::SourceRoleContext;

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
