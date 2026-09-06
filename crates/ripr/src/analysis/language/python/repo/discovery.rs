//! Bounded repo working-set discovery for the Python adapter (#3554 PR A).
//!
//! Issue #2109 contract: repo-scoped analysis must never read a workspace
//! without a working-set bound. The Python repo walk prunes the adapter's
//! excluded directory subtrees at directory granularity (no descent, so a
//! large `.venv` costs nothing), counts every adapter-routed Python file it
//! sees, classifies each with the shared role authority, and selects at most
//! the working-set limit of eligible files in deterministic (sorted) order.
//!
//! The cap source and the operator recovery route are retained as typed
//! fields so every surface can disclose the same remediation.

use super::super::{LanguageAdapter, PYTHON_WORKSPACE_EXCLUDED_DIRS, PythonAdapter};
use super::roles::{PythonFileRole, classify_python_file_role, role_is_excluded_from_analysis};
use std::path::{Path, PathBuf};

/// Default repo working-set limit for the Python adapter.
///
/// Same bound family as the Rust repo guard (`REPO_INDEX_FILE_LIMIT`,
/// #2109): repo-scoped analysis over a working set at this scale is the
/// protected envelope; larger workspaces must raise the limit explicitly.
pub(in crate::analysis::language::python) const PYTHON_REPO_FILE_LIMIT: usize = 800;

/// Shared repo working-set override (see #2109). One operator knob governs
/// the repo working-set bound across language producers.
pub(in crate::analysis::language::python) const PYTHON_REPO_FILE_LIMIT_ENV: &str =
    "RIPR_MAX_REPO_INDEX_FILES";

/// Where the effective working-set limit came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis::language::python) enum RepoWorkingSetCapSource {
    /// The producer-owned default ([`PYTHON_REPO_FILE_LIMIT`]).
    Default,
    /// An operator raised or lowered it via
    /// [`PYTHON_REPO_FILE_LIMIT_ENV`].
    EnvOverride,
}

/// The effective repo working-set limit and its source (#2109).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis::language::python) struct RepoWorkingSetLimit {
    pub(in crate::analysis::language::python) limit: usize,
    pub(in crate::analysis::language::python) source: RepoWorkingSetCapSource,
}

/// Operator recovery routes for a capped working set, retained as typed
/// values so every surface renders the same remediation (#2109).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis::language::python) enum CapRecoveryRoute {
    /// Narrow the scope to changed behavior with a diff-based run.
    DiffBasedRun,
    /// Raise the working-set limit via the shared env override.
    RaiseWorkingSetLimit,
}

impl CapRecoveryRoute {
    pub(in crate::analysis::language::python) fn describe(self) -> &'static str {
        match self {
            Self::DiffBasedRun => "narrow the scope with a diff-based run (--base/--diff)",
            Self::RaiseWorkingSetLimit => "raise the limit via RIPR_MAX_REPO_INDEX_FILES=<number>",
        }
    }
}

/// Recovery routes in operator-facing priority order.
pub(in crate::analysis::language::python) const CAP_RECOVERY_ROUTES: &[CapRecoveryRoute] = &[
    CapRecoveryRoute::DiffBasedRun,
    CapRecoveryRoute::RaiseWorkingSetLimit,
];

/// Resolve the effective working-set limit from a shared-env read.
///
/// Mirrors the Rust adapter's positive-integer env contract (#2109): absent
/// means the default; zero, non-integer, or non-UTF-8 values fail with a
/// named error instead of being silently ignored.
pub(in crate::analysis::language::python) fn resolve_repo_working_set_limit(
    env: Result<String, std::env::VarError>,
) -> Result<RepoWorkingSetLimit, String> {
    match env {
        Err(std::env::VarError::NotPresent) => Ok(RepoWorkingSetLimit {
            limit: PYTHON_REPO_FILE_LIMIT,
            source: RepoWorkingSetCapSource::Default,
        }),
        Ok(raw) => {
            let parsed = raw.trim().parse::<usize>().map_err(|err| {
                format!("{PYTHON_REPO_FILE_LIMIT_ENV} must be a positive integer: {err}")
            })?;
            if parsed == 0 {
                return Err(format!(
                    "{PYTHON_REPO_FILE_LIMIT_ENV} must be a positive integer"
                ));
            }
            Ok(RepoWorkingSetLimit {
                limit: parsed,
                source: RepoWorkingSetCapSource::EnvOverride,
            })
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("{PYTHON_REPO_FILE_LIMIT_ENV} must be valid UTF-8"))
        }
    }
}

/// Read the effective working-set limit from the process environment.
pub(in crate::analysis::language::python) fn repo_working_set_limit()
-> Result<RepoWorkingSetLimit, String> {
    resolve_repo_working_set_limit(std::env::var(PYTHON_REPO_FILE_LIMIT_ENV))
}

/// Typed working-set counts for one bounded discovery pass (#2109, #3554).
///
/// Identities a reviewer or renderer can check:
///
/// - `discovered == selected + skipped`
/// - `skipped == excluded_by_role + capped`
/// - `analyzed_candidates <= selected` (the production-role subset)
///
/// `failed` counts selected files the evidence producer could not read or
/// parse. Discovery itself performs no source reads, so it is always 0 here;
/// the producer (PR B) owns populating it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::analysis::language::python) struct DiscoveryCounts {
    /// Adapter-routed Python files seen by the pruned walk, including
    /// generated and role-excluded files.
    pub(in crate::analysis::language::python) discovered: usize,
    /// Eligible files taken into the working set under the cap.
    pub(in crate::analysis::language::python) selected: usize,
    /// Selected production-role files: the analysis subjects.
    pub(in crate::analysis::language::python) analyzed_candidates: usize,
    /// Discovered files intentionally not selected:
    /// `excluded_by_role + capped`.
    pub(in crate::analysis::language::python) skipped: usize,
    /// Discovered files excluded by their typed role (generated,
    /// environment/cache/vendor).
    pub(in crate::analysis::language::python) excluded_by_role: usize,
    /// Eligible files beyond the cap: counted, not analyzed.
    pub(in crate::analysis::language::python) capped: usize,
    /// Selected files the evidence producer could not read or parse; zero
    /// until that producer lands (PR B).
    pub(in crate::analysis::language::python) failed: usize,
    /// Directory subtrees the walk could not read (permission or I/O
    /// errors). Their contents were never discovered, so a non-zero count
    /// means the discovered set is incomplete and a full-denominator claim
    /// is not earned (#3666 review).
    pub(in crate::analysis::language::python) unreadable_subtrees: usize,
}

/// Result of one bounded discovery pass: the selected working set with
/// roles, plus the typed counts.
pub(in crate::analysis::language::python) struct RepoDiscovery {
    /// Selected `(path, role)` pairs in deterministic sorted-path order.
    pub(in crate::analysis::language::python) selected: Vec<(PathBuf, PythonFileRole)>,
    pub(in crate::analysis::language::python) counts: DiscoveryCounts,
}

/// Discover and select the bounded Python repo working set under `root`.
///
/// Deterministic: the walk output is sorted, role classification is pure,
/// and selection is the sorted eligible prefix of length
/// `min(eligible, limit)`.
pub(in crate::analysis::language::python) fn discover_repo_working_set(
    root: &Path,
    limit: usize,
) -> RepoDiscovery {
    let mut discovered: Vec<PathBuf> = Vec::new();
    let mut unreadable_subtrees: usize = 0;
    visit_repo_workspace(root, root, &mut discovered, &mut unreadable_subtrees);
    discovered.sort();

    let discovered_count = discovered.len();
    // Production files are the analysis subjects and get cap priority
    // (#3666 review): a role-blind sorted prefix could fill the cap with
    // evidence files and hide production source behind `capped`, which
    // would misreport the workspace as having no Python production
    // source at all.
    let mut production: Vec<PathBuf> = Vec::new();
    let mut evidence: Vec<(PathBuf, PythonFileRole)> = Vec::new();
    let mut excluded_by_role = 0usize;
    for path in &discovered {
        let role = classify_python_file_role(path);
        if role_is_excluded_from_analysis(role) {
            excluded_by_role += 1;
        } else if role == PythonFileRole::Production {
            production.push(path.clone());
        } else {
            evidence.push((path.clone(), role));
        }
    }

    let capped;
    let mut selected_production = production.clone();
    let mut selected_evidence: Vec<(PathBuf, PythonFileRole)> = Vec::new();
    if production.len() >= limit {
        // The cap excludes production subjects directly.
        selected_production.truncate(limit);
        capped = production.len() - limit + evidence.len();
    } else {
        let evidence_budget = limit - production.len();
        capped = evidence.len().saturating_sub(evidence_budget);
        selected_evidence = evidence.into_iter().take(evidence_budget).collect();
    }

    let selected_count = selected_production.len() + selected_evidence.len();
    let analyzed_candidates = selected_production.len();

    let mut selected: Vec<(PathBuf, PythonFileRole)> = selected_production
        .into_iter()
        .map(|path| (path, PythonFileRole::Production))
        .collect();
    selected.extend(selected_evidence);
    selected.sort_by(|(left, _), (right, _)| left.cmp(right));

    RepoDiscovery {
        selected,
        counts: DiscoveryCounts {
            discovered: discovered_count,
            selected: selected_count,
            analyzed_candidates,
            skipped: excluded_by_role + capped,
            excluded_by_role,
            capped,
            failed: 0,
            unreadable_subtrees,
        },
    }
}

/// Pruned workspace walk.
///
/// Mirrors the adapter's `visit_workspace` pattern (no descent into
/// excluded-directory subtrees) but retains generated files so the role
/// authority can exclude them with counts.
fn visit_repo_workspace(
    root: &Path,
    dir: &Path,
    out: &mut Vec<PathBuf>,
    unreadable_subtrees: &mut usize,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // The subtree's contents were never discovered: count it so the
        // run cannot claim a full denominator over an incomplete scan
        // (#3666 review).
        *unreadable_subtrees += 1;
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if PYTHON_WORKSPACE_EXCLUDED_DIRS.contains(&name) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            visit_repo_workspace(root, &path, out, unreadable_subtrees);
        } else if file_type.is_file() {
            let adapter = PythonAdapter;
            if adapter.accepts_path(&path) {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                out.push(relative);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, contents: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("create parent: {err}"))?;
        }
        std::fs::write(path, contents).map_err(|err| format!("write {}: {err}", path.display()))
    }

    fn unique_test_root(label: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "ripr-py-repo-discovery-{label}-{}-{stamp}",
            std::process::id()
        ))
    }

    fn selected_paths(discovery: &RepoDiscovery) -> Vec<String> {
        discovery
            .selected
            .iter()
            .map(|(path, _)| path.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn resolve_limit_defaults_when_env_absent() -> Result<(), String> {
        let resolved = resolve_repo_working_set_limit(Err(std::env::VarError::NotPresent))?;
        assert_eq!(resolved.limit, PYTHON_REPO_FILE_LIMIT);
        assert_eq!(resolved.source, RepoWorkingSetCapSource::Default);
        Ok(())
    }

    #[test]
    fn resolve_limit_accepts_positive_override() -> Result<(), String> {
        let resolved = resolve_repo_working_set_limit(Ok(" 120 ".to_string()))?;
        assert_eq!(resolved.limit, 120);
        assert_eq!(resolved.source, RepoWorkingSetCapSource::EnvOverride);
        Ok(())
    }

    #[test]
    fn resolve_limit_rejects_zero_garbage_and_non_unicode() -> Result<(), String> {
        let zero = resolve_repo_working_set_limit(Ok("0".to_string()))
            .err()
            .ok_or("a zero limit must be rejected")?;
        assert!(zero.contains(PYTHON_REPO_FILE_LIMIT_ENV), "{zero}");

        let garbage = resolve_repo_working_set_limit(Ok("many".to_string()))
            .err()
            .ok_or("a non-integer limit must be rejected")?;
        assert!(garbage.contains(PYTHON_REPO_FILE_LIMIT_ENV), "{garbage}");

        let non_unicode =
            resolve_repo_working_set_limit(Err(std::env::VarError::NotUnicode("x".into())))
                .err()
                .ok_or("a non-UTF-8 limit must be rejected")?;
        assert!(
            non_unicode.contains(PYTHON_REPO_FILE_LIMIT_ENV),
            "{non_unicode}"
        );
        Ok(())
    }

    #[test]
    fn discovery_selects_nothing_from_an_empty_workspace() -> Result<(), String> {
        let root = unique_test_root("empty");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        let discovery = discover_repo_working_set(&root, 800);
        assert!(discovery.selected.is_empty());
        assert_eq!(discovery.counts, DiscoveryCounts::default());
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn discovery_enforces_the_working_set_cap_with_counts() -> Result<(), String> {
        let root = unique_test_root("capped");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        for name in ["alpha.py", "beta.py", "gamma.py"] {
            write_file(&root.join(name), "def value():\n    return 1\n")?;
        }
        let discovery = discover_repo_working_set(&root, 2);
        assert_eq!(discovery.counts.discovered, 3);
        assert_eq!(discovery.counts.selected, 2);
        assert_eq!(discovery.counts.analyzed_candidates, 2);
        assert_eq!(discovery.counts.capped, 1);
        assert_eq!(discovery.counts.skipped, 1);
        assert_eq!(discovery.counts.excluded_by_role, 0);
        assert_eq!(selected_paths(&discovery).len(), 2);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn discovery_keeps_deterministic_sorted_order() -> Result<(), String> {
        let root = unique_test_root("order");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        for name in ["zeta.py", "src/alpha.py", "beta.py"] {
            write_file(&root.join(name), "VALUE = 1\n")?;
        }
        let first = discover_repo_working_set(&root, 800);
        let second = discover_repo_working_set(&root, 800);
        assert_eq!(
            selected_paths(&first),
            vec!["beta.py", "src/alpha.py", "zeta.py"]
        );
        assert_eq!(first.selected, second.selected);
        assert_eq!(first.counts, second.counts);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn discovery_counts_generated_and_environment_exclusions_by_role() -> Result<(), String> {
        let root = unique_test_root("excluded");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        write_file(&root.join("app.py"), "VALUE = 1\n")?;
        write_file(&root.join("gen_pb2.py"), "# generated\n")?;
        write_file(&root.join("vendor/dep.py"), "VALUE = 2\n")?;
        // Excluded-directory subtrees are pruned, not walked: the .venv file
        // contributes nothing to any count.
        write_file(&root.join(".venv/lib/hidden.py"), "VALUE = 3\n")?;
        let discovery = discover_repo_working_set(&root, 800);
        assert_eq!(discovery.counts.discovered, 3);
        assert_eq!(discovery.counts.excluded_by_role, 2);
        assert_eq!(discovery.counts.selected, 1);
        assert_eq!(discovery.counts.skipped, 2);
        assert_eq!(selected_paths(&discovery), vec!["app.py"]);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn discovery_ignores_non_python_files_in_a_mixed_workspace() -> Result<(), String> {
        let root = unique_test_root("mixed");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        write_file(&root.join("app.py"), "VALUE = 1\n")?;
        write_file(&root.join("main.rs"), "fn main() {}\n")?;
        write_file(&root.join("README.md"), "# mixed\n")?;
        let discovery = discover_repo_working_set(&root, 800);
        assert_eq!(discovery.counts.discovered, 1);
        assert_eq!(selected_paths(&discovery), vec!["app.py"]);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn discovery_counts_reconcile_across_roles() -> Result<(), String> {
        let root = unique_test_root("reconcile");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        write_file(&root.join("app.py"), "VALUE = 1\n")?;
        write_file(
            &root.join("tests/test_app.py"),
            "def test_it():\n    pass\n",
        )?;
        write_file(&root.join("conftest.py"), "import pytest\n")?;
        write_file(&root.join("gen_pb2.py"), "# generated\n")?;
        write_file(&root.join("vendor/dep.py"), "VALUE = 2\n")?;
        let discovery = discover_repo_working_set(&root, 3);
        let counts = discovery.counts;
        assert_eq!(counts.discovered, 5);
        assert_eq!(counts.excluded_by_role, 2);
        assert_eq!(counts.selected, 3);
        assert_eq!(counts.capped, 0);
        assert_eq!(counts.skipped, counts.excluded_by_role + counts.capped);
        assert_eq!(counts.discovered, counts.selected + counts.skipped);
        assert_eq!(counts.analyzed_candidates, 1);
        let roles: Vec<PythonFileRole> = discovery.selected.iter().map(|(_, role)| *role).collect();
        assert!(roles.contains(&PythonFileRole::Production));
        assert!(roles.contains(&PythonFileRole::PhysicalTest));
        assert!(roles.contains(&PythonFileRole::InlineHelper));
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }
}
