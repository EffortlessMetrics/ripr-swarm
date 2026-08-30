use super::super::AnalysisMode;
use super::classify::package_root;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Depth bound for the module-context closure. Real parent chains nest a
/// handful of levels; anything past this bound stops pulling parents and
/// fails closed (their children keep their standalone roles).
const MAX_MODULE_CONTEXT_DEPTH: usize = 32;

pub fn select_rust_files_for_mode(
    all_files: &[PathBuf],
    changed_rust_files: &[PathBuf],
    mode: AnalysisMode,
    include_unchanged_tests: bool,
) -> Vec<PathBuf> {
    let changed_existing = changed_existing_files(all_files, changed_rust_files);
    if matches!(mode, AnalysisMode::Instant) || !include_unchanged_tests {
        return with_module_context_files(all_files, changed_existing);
    }

    if matches!(mode, AnalysisMode::Deep | AnalysisMode::Ready) {
        return sorted_unique(all_files.iter().cloned());
    }

    let package_roots = changed_rust_files
        .iter()
        .filter_map(|path| package_root(path))
        .collect::<Vec<_>>();
    if package_roots.is_empty() {
        return with_module_context_files(all_files, changed_existing);
    }

    let package_files = all_files.iter().filter(|file| {
        package_root(file)
            .as_ref()
            .is_some_and(|root| package_roots.iter().any(|changed| changed == root))
    });
    with_module_context_files(
        all_files,
        sorted_unique(package_files.cloned().chain(changed_existing)),
    )
}

/// Extends a narrowed selection with the default-layout module parents of
/// the selected files (#3533): role composition derives a child file's
/// context from the declaration in its parent, so a changed-files-only index
/// that omits an unchanged parent leaves test-only helpers as production
/// subjects. Candidates are plain members of the workspace list — no disk
/// access, deterministic, depth-bounded — and files already selected are
/// never duplicated. `#[path]` redirections from files outside the default
/// layouts stay invisible in narrowed modes (fail closed: no composed role).
fn with_module_context_files(all_files: &[PathBuf], selected: Vec<PathBuf>) -> Vec<PathBuf> {
    if selected.len() >= all_files.len() {
        return selected;
    }
    let known: BTreeSet<&PathBuf> = all_files.iter().collect();
    let mut result: BTreeSet<PathBuf> = selected.into_iter().collect();
    let mut frontier: Vec<PathBuf> = result.iter().cloned().collect();
    for _ in 0..MAX_MODULE_CONTEXT_DEPTH {
        if frontier.is_empty() {
            break;
        }
        let mut next = Vec::new();
        for file in &frontier {
            for candidate in module_parent_candidates(file) {
                if result.contains(&candidate) || !known.contains(&candidate) {
                    continue;
                }
                result.insert(candidate.clone());
                next.push(candidate);
            }
        }
        frontier = next;
    }
    result.into_iter().collect()
}

/// The default-layout files that can declare `mod <name>;` resolving to
/// `file` (#3533). Default resolution looks for `<module-dir>/<name>.rs` and
/// `<module-dir>/<name>/mod.rs` under the declaring file's module directory,
/// so the candidate declaring files are the module-directory files of
/// `file`'s own directory plus the sibling stem file:
/// `src/foo/tests.rs` is declared by `src/foo/mod.rs`, `src/foo/lib.rs`,
/// `src/foo/main.rs`, or `src/foo.rs`; a `mod.rs` child
/// (`src/foo/tests/mod.rs`) is declared from the parent directory instead.
fn module_parent_candidates(file: &Path) -> Vec<PathBuf> {
    let file_name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let Some(directory) = file
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
    else {
        return Vec::new();
    };
    // Candidates are built by string join, not `Path::join`, so the
    // constructed identities keep the workspace list's forward-slash form
    // and compare equal to its members on every platform.
    let join =
        |directory: &Path, name: &str| PathBuf::from(format!("{}/{}", directory.display(), name));
    let declaring_directory = if file_name == "mod.rs" {
        match directory
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            Some(parent) => parent.to_path_buf(),
            None => return Vec::new(),
        }
    } else {
        directory.to_path_buf()
    };
    let mut candidates = vec![
        join(&declaring_directory, "mod.rs"),
        join(&declaring_directory, "lib.rs"),
        join(&declaring_directory, "main.rs"),
    ];
    if let Some(stem) = declaring_directory.file_stem()
        && let Some(parent) = declaring_directory
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
    {
        candidates.push(join(parent, &format!("{}.rs", stem.to_string_lossy())));
    }
    candidates
}

fn changed_existing_files(all_files: &[PathBuf], changed_rust_files: &[PathBuf]) -> Vec<PathBuf> {
    sorted_unique(
        changed_rust_files
            .iter()
            .filter(|changed| all_files.iter().any(|file| file == *changed))
            .cloned(),
    )
}

fn sorted_unique(files: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut out = files.into_iter().collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::workspace::discover_rust_files;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn files(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    fn temp_dir(name: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("system clock before unix epoch: {err}"))?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ripr-workspace-{name}-{stamp}"));
        fs::create_dir_all(&dir).map_err(|err| format!("create temp dir failed: {err}"))?;
        Ok(dir)
    }

    #[test]
    fn instant_indexes_changed_rust_files_only() {
        let all = files(&["src/lib.rs", "tests/pricing.rs", "crates/other/src/lib.rs"]);
        let selected =
            select_rust_files_for_mode(&all, &files(&["src/lib.rs"]), AnalysisMode::Instant, true);
        assert_eq!(selected, files(&["src/lib.rs"]));
    }

    /// A narrowed selection must bring the unchanged default-layout parents
    /// of changed module children (#3533): composition derives a child's
    /// context from its parent declaration, so a changed-files-only index
    /// would leave test-only helpers as production subjects.
    #[test]
    fn narrowed_selection_brings_out_of_line_module_parents() {
        let all = files(&["src/lib.rs", "src/foo.rs", "src/foo/tests.rs"]);
        let expected_identity = |mut expected: Vec<PathBuf>| {
            expected.sort();
            expected
        };
        let selected = select_rust_files_for_mode(
            &all,
            &files(&["src/foo/tests.rs"]),
            AnalysisMode::Instant,
            true,
        );
        assert_eq!(
            selected,
            expected_identity(files(&["src/foo.rs", "src/foo/tests.rs", "src/lib.rs"])),
            "the child's stem-directory parent and the workspace root arrive with it"
        );
        // include_unchanged_tests=false narrows Deep the same way.
        assert_eq!(
            select_rust_files_for_mode(
                &all,
                &files(&["src/foo/tests.rs"]),
                AnalysisMode::Deep,
                false
            ),
            expected_identity(files(&["src/foo.rs", "src/foo/tests.rs", "src/lib.rs"]))
        );
    }

    /// The common sibling include shape: a changed fragment's compilation
    /// unit in the same directory is pulled in, so the include edge resolves
    /// and the fragment inherits its context.
    #[test]
    fn narrowed_selection_brings_the_include_unit_for_sibling_fragments() {
        let all = files(&["src/lib.rs", "src/fragment.rs"]);
        let selected = select_rust_files_for_mode(
            &all,
            &files(&["src/fragment.rs"]),
            AnalysisMode::Instant,
            true,
        );
        let mut expected = files(&["src/fragment.rs", "src/lib.rs"]);
        expected.sort();
        assert_eq!(selected, expected);
    }

    /// A `mod.rs` child's declaring files live in the parent directory.
    #[test]
    fn narrowed_selection_brings_mod_rs_children_parents() {
        let all = files(&["src/foo.rs", "src/foo/tests/mod.rs"]);
        let selected = select_rust_files_for_mode(
            &all,
            &files(&["src/foo/tests/mod.rs"]),
            AnalysisMode::Instant,
            true,
        );
        let mut expected = files(&["src/foo.rs", "src/foo/tests/mod.rs"]);
        expected.sort();
        assert_eq!(selected, expected);
    }

    /// The closure adds only real candidates: a child whose default-layout
    /// parents do not exist in the workspace pulls nothing extra in.
    #[test]
    fn narrowed_selection_closure_adds_only_existing_candidates() {
        let all = files(&["crates/alpha/src/lib.rs", "crates/alpha/src/foo/tests.rs"]);
        let selected = select_rust_files_for_mode(
            &all,
            &files(&["crates/alpha/src/foo/tests.rs"]),
            AnalysisMode::Instant,
            true,
        );
        assert_eq!(
            selected,
            files(&["crates/alpha/src/foo/tests.rs"]),
            "crates/alpha/src/foo.rs is absent from the workspace list, so nothing is added"
        );
    }

    #[test]
    fn draft_and_fast_index_changed_package_files() {
        let all = files(&[
            "crates/pricing/src/lib.rs",
            "crates/pricing/tests/pricing.rs",
            "crates/risk/src/lib.rs",
            "crates/risk/tests/risk.rs",
        ]);
        let changed = files(&["crates/pricing/src/lib.rs"]);

        for mode in [AnalysisMode::Draft, AnalysisMode::Fast] {
            let selected = select_rust_files_for_mode(&all, &changed, mode, true);
            assert_eq!(
                selected,
                files(&[
                    "crates/pricing/src/lib.rs",
                    "crates/pricing/tests/pricing.rs"
                ])
            );
        }
    }

    #[test]
    fn deep_and_ready_index_entire_workspace() {
        let all = files(&["src/lib.rs", "tests/pricing.rs", "crates/other/src/lib.rs"]);
        let changed = files(&["src/lib.rs"]);

        for mode in [AnalysisMode::Deep, AnalysisMode::Ready] {
            let selected = select_rust_files_for_mode(&all, &changed, mode, true);
            assert_eq!(
                selected,
                files(&["crates/other/src/lib.rs", "src/lib.rs", "tests/pricing.rs"])
            );
        }
    }

    #[test]
    fn operator_mode_tiers_are_pinned_for_defaults_first_adoption() {
        let all = files(&[
            "crates/pricing/src/lib.rs",
            "crates/pricing/tests/pricing.rs",
            "crates/risk/src/lib.rs",
            "crates/risk/tests/risk.rs",
        ]);
        let changed = files(&["crates/pricing/src/lib.rs"]);

        assert_eq!(
            select_rust_files_for_mode(&all, &changed, AnalysisMode::Instant, true),
            files(&["crates/pricing/src/lib.rs"])
        );
        assert_eq!(
            select_rust_files_for_mode(&all, &changed, AnalysisMode::Draft, true),
            files(&[
                "crates/pricing/src/lib.rs",
                "crates/pricing/tests/pricing.rs"
            ])
        );
        assert_eq!(
            select_rust_files_for_mode(&all, &changed, AnalysisMode::Fast, true),
            files(&[
                "crates/pricing/src/lib.rs",
                "crates/pricing/tests/pricing.rs"
            ])
        );
        assert_eq!(
            select_rust_files_for_mode(&all, &changed, AnalysisMode::Deep, true),
            files(&[
                "crates/pricing/src/lib.rs",
                "crates/pricing/tests/pricing.rs",
                "crates/risk/src/lib.rs",
                "crates/risk/tests/risk.rs"
            ])
        );
        assert_eq!(
            select_rust_files_for_mode(&all, &changed, AnalysisMode::Ready, true),
            files(&[
                "crates/pricing/src/lib.rs",
                "crates/pricing/tests/pricing.rs",
                "crates/risk/src/lib.rs",
                "crates/risk/tests/risk.rs"
            ])
        );
    }

    #[test]
    fn source_role_excludes_xtask_automation() {
        assert!(
            !crate::analysis::workspace::classify_with(
                Path::new("xtask/src/main.rs"),
                &crate::analysis::workspace::SourceRoleContext::empty(),
            )
            .seeds_production_findings()
        );
        assert!(
            crate::analysis::workspace::classify_with(
                Path::new("crates/ripr/src/lib.rs"),
                &crate::analysis::workspace::SourceRoleContext::empty(),
            )
            .seeds_production_findings()
        );
    }

    #[test]
    fn repo_discovery_skips_fixture_tree_but_fixture_roots_still_work() -> Result<(), String> {
        let root = temp_dir("fixtures")?;
        fs::create_dir_all(root.join("src"))
            .map_err(|err| format!("create root src failed: {err}"))?;
        fs::create_dir_all(root.join("fixtures/boundary/input/src"))
            .map_err(|err| format!("create fixture src failed: {err}"))?;
        fs::write(root.join("src/lib.rs"), "")
            .map_err(|err| format!("write root src failed: {err}"))?;
        fs::write(root.join("fixtures/boundary/input/src/lib.rs"), "")
            .map_err(|err| format!("write fixture src failed: {err}"))?;

        assert_eq!(discover_rust_files(&root)?, files(&["src/lib.rs"]));
        assert_eq!(
            discover_rust_files(&root.join("fixtures/boundary/input"))?,
            files(&["src/lib.rs"])
        );
        Ok(())
    }

    #[test]
    fn no_unchanged_tests_limits_any_mode_to_changed_files() {
        let all = files(&["src/lib.rs", "tests/pricing.rs"]);
        let selected =
            select_rust_files_for_mode(&all, &files(&["src/lib.rs"]), AnalysisMode::Deep, false);
        assert_eq!(selected, files(&["src/lib.rs"]));
    }

    #[test]
    fn draft_and_fast_selection_is_stable_and_subset_of_workspace() {
        let corpus = [
            "src/lib.rs",
            "src/main.rs",
            "tests/root.rs",
            "examples/root.rs",
            "crates/alpha/src/lib.rs",
            "crates/alpha/tests/alpha.rs",
            "crates/beta/src/lib.rs",
            "crates/beta/tests/beta.rs",
            "crates/gamma/src/lib.rs",
            "crates/gamma/tests/gamma.rs",
            "tools/helper/src/lib.rs",
            "tools/helper/tests/helper.rs",
        ];
        let all = files(&corpus);

        let mut seed = 0x5EED_u64;
        for _case in 0..256 {
            let mut changed = Vec::new();
            for path in &all {
                if next_u64(&mut seed) & 1 == 0 {
                    changed.push(path.clone());
                }
            }

            for mode in [AnalysisMode::Draft, AnalysisMode::Fast] {
                let selected = select_rust_files_for_mode(&all, &changed, mode, true);

                assert!(selected.windows(2).all(|w| w[0] < w[1]));
                assert!(selected.iter().all(|path| all.contains(path)));
                assert!(
                    changed
                        .iter()
                        .filter(|path| all.contains(path))
                        .all(|path| selected.contains(path))
                );
            }
        }
    }

    fn next_u64(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *seed
    }
}
