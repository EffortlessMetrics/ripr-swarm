//! Bounded Python repository-mode input and native item authority.
//!
//! See `docs/specs/RIPR-SPEC-0028-python-preview-static-facts.md` and issue #3558.
//!
//! Discovers workspace Python files, assigns roles (production, test, helper,
//! and excluded), enforces working-set limits against `RIPR_MAX_REPO_INDEX_FILES`
//! with fail-closed recovery guidance, and extracts native Python item/symbol
//! identities without manufacturing fake probes or findings.

use super::source_facts::extract_source_facts;
use super::source_utils::normalized_path;
use super::{
    PythonAdapter, PythonOwner, PythonTest, is_detectable_generated_python_file,
    is_python_workspace_excluded_dir,
};
use crate::analysis::language::LanguageAdapter;
use crate::analysis_outcome::{
    AnalysisLimitation, AnalysisLimitationKind, AnalysisRecovery, AnalysisRecoveryKind,
    AnalysisStage,
};
use crate::config::{detect_python_project, is_python_project_excluded_dir};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Environment variable for overriding the repo-mode file index ceiling.
pub(crate) const REPO_INDEX_FILE_LIMIT_ENV: &str = "RIPR_MAX_REPO_INDEX_FILES";

/// Default repo-mode file index ceiling matching the Rust adapter standard.
pub(crate) const DEFAULT_REPO_INDEX_FILE_LIMIT: usize = 800;

/// Role assigned to a Python file discovered in a workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PythonFileRole {
    /// Production implementation file (eligible to seed native owners/symbols).
    Production,
    /// Test file (contains or is named according to test conventions).
    Test,
    /// Test support or helper file (e.g. conftest.py, fixture modules under tests/).
    Helper,
    /// Explicitly excluded or generated file.
    Excluded,
}

/// Discovered and role-classified Python workspace file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PythonWorkspaceFile {
    /// Workspace-relative path.
    pub(crate) relative_path: PathBuf,
    /// Absolute filesystem path.
    pub(crate) absolute_path: PathBuf,
    /// Assigned role.
    pub(crate) role: PythonFileRole,
}

/// A syntax or parse failure encountered while inspecting a Python source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PythonFileParseError {
    pub(crate) relative_path: PathBuf,
    pub(crate) error: String,
}

/// Bounded Python repository input snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PythonRepoInput {
    /// Root workspace directory.
    pub(crate) root: PathBuf,
    /// Whether standard Python project markers were detected.
    pub(crate) project_detected: bool,
    /// All discovered workspace files with their assigned roles.
    pub(crate) files: Vec<PythonWorkspaceFile>,
    /// Production files in the working set.
    pub(crate) production_files: Vec<PathBuf>,
    /// Test files in the working set.
    pub(crate) test_files: Vec<PathBuf>,
    /// Helper files in the working set.
    pub(crate) helper_files: Vec<PathBuf>,
    /// Number of generated or excluded files skipped during discovery.
    pub(crate) skipped_generated_files: usize,
    /// Configured working set limit.
    pub(crate) working_set_limit: usize,
    /// Extracted native Python owners from production files.
    pub(crate) owners: Vec<PythonOwner>,
    /// Extracted native Python tests from test and helper files.
    pub(crate) tests: Vec<PythonTest>,
    /// Files that encountered parse or read errors.
    pub(crate) parse_errors: Vec<PythonFileParseError>,
    /// Docstring line ranges by relative path.
    pub(crate) docstring_ranges_by_file: BTreeMap<PathBuf, Vec<std::ops::RangeInclusive<usize>>>,
    /// Typed analysis limitations.
    pub(crate) limitations: Vec<AnalysisLimitation>,
}

impl PythonRepoInput {
    /// Discovers and bounds Python repo inputs for a given workspace root.
    pub(crate) fn discover(root: &Path) -> Result<Self, String> {
        let limit = repo_index_file_limit_from_env(std::env::var(REPO_INDEX_FILE_LIMIT_ENV))?;
        Self::discover_with_limit(root, limit)
    }

    /// Discovers and bounds Python repo inputs with an explicit limit.
    pub(crate) fn discover_with_limit(
        root: &Path,
        working_set_limit: usize,
    ) -> Result<Self, String> {
        let project_detected = detect_python_project(root);

        let mut discovered_files = Vec::new();
        let mut skipped_generated_files = 0;
        visit_python_workspace(
            root,
            root,
            &mut discovered_files,
            &mut skipped_generated_files,
        );
        discovered_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

        let mut production_files = Vec::new();
        let mut test_files = Vec::new();
        let mut helper_files = Vec::new();

        for file in &discovered_files {
            match file.role {
                PythonFileRole::Production => production_files.push(file.relative_path.clone()),
                PythonFileRole::Test => test_files.push(file.relative_path.clone()),
                PythonFileRole::Helper => helper_files.push(file.relative_path.clone()),
                PythonFileRole::Excluded => {}
            }
        }

        let total_analyzable = production_files.len() + test_files.len() + helper_files.len();
        if total_analyzable > working_set_limit {
            return Err(format!(
                "repo_scope_oversized: {total_analyzable} indexed Python files exceed the \
                 {REPO_INDEX_FILE_LIMIT_ENV} limit ({working_set_limit}); analysis was not run to protect \
                 runner memory. Repair route: narrow the scope with a diff-based run (--base/--diff), \
                 or raise the limit via {REPO_INDEX_FILE_LIMIT_ENV}=<number>."
            ));
        }

        let mut owners = Vec::new();
        let mut tests = Vec::new();
        let mut parse_errors = Vec::new();
        let mut docstring_ranges_by_file = BTreeMap::new();
        let mut limitations = Vec::new();

        for file in &discovered_files {
            if file.role == PythonFileRole::Excluded {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&file.absolute_path) else {
                parse_errors.push(PythonFileParseError {
                    relative_path: file.relative_path.clone(),
                    error: "failed to read file".to_string(),
                });
                continue;
            };

            let facts = extract_source_facts(&file.relative_path, &source);
            docstring_ranges_by_file.insert(
                file.relative_path.clone(),
                facts.docstring_line_ranges.clone(),
            );

            // If facts recorded an unsupported syntax limitation, track it
            for lim in &facts.limitations {
                if let Ok(recovery) = AnalysisRecovery::new(
                    AnalysisRecoveryKind::Retry,
                    "Fix the Python syntax error, then re-run the analysis.",
                ) && let Ok(analysis_lim) = AnalysisLimitation::new(
                    AnalysisLimitationKind::LanguageScopeUnsupported,
                    AnalysisStage::LanguageAdapter,
                    recovery,
                )
                .with_path(normalized_path(&file.relative_path))
                .and_then(|l| l.with_affected_items(1))
                .and_then(|l| l.with_detail(lim.evidence.clone()))
                {
                    limitations.push(analysis_lim);
                }
                parse_errors.push(PythonFileParseError {
                    relative_path: file.relative_path.clone(),
                    error: lim.evidence.clone(),
                });
            }

            match file.role {
                PythonFileRole::Production => {
                    // Production files seed native owners only; never tests.
                    owners.extend(facts.owners);
                }
                PythonFileRole::Test | PythonFileRole::Helper => {
                    // Test and helper files seed tests only; never production owners.
                    tests.extend(facts.tests);
                }
                PythonFileRole::Excluded => {}
            }
        }

        Ok(Self {
            root: root.to_path_buf(),
            project_detected,
            files: discovered_files,
            production_files,
            test_files,
            helper_files,
            skipped_generated_files,
            working_set_limit,
            owners,
            tests,
            parse_errors,
            docstring_ranges_by_file,
            limitations,
        })
    }

    /// Checks whether an error string is the named `repo_scope_oversized` guard stop.
    #[cfg(test)]
    pub(crate) fn is_repo_scope_oversized(error: &str) -> bool {
        error.starts_with("repo_scope_oversized:")
    }
}

/// Classifies a workspace-relative Python path into its role.
pub(crate) fn classify_python_file_role(path: &Path) -> PythonFileRole {
    if is_detectable_generated_python_file(path) {
        return PythonFileRole::Excluded;
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if file_name == "conftest.py" {
        return PythonFileRole::Helper;
    }

    let is_test_name = file_name.starts_with("test_") || file_name.ends_with("_test.py");
    let is_in_test_dir = path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "tests" || s == "test"
    });

    if is_test_name {
        return PythonFileRole::Test;
    }

    if is_in_test_dir {
        return PythonFileRole::Helper;
    }

    PythonFileRole::Production
}

fn visit_python_workspace(
    root: &Path,
    dir: &Path,
    out: &mut Vec<PythonWorkspaceFile>,
    skipped_generated_files: &mut usize,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if is_python_workspace_excluded_dir(name) || is_python_project_excluded_dir(name) {
            continue;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            visit_python_workspace(root, &path, out, skipped_generated_files);
        } else if file_type.is_file() {
            let adapter = PythonAdapter;
            if adapter.accepts_path(&path) {
                if is_detectable_generated_python_file(&path) {
                    *skipped_generated_files += 1;
                    continue;
                }
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                let role = classify_python_file_role(&relative);
                out.push(PythonWorkspaceFile {
                    relative_path: relative,
                    absolute_path: path,
                    role,
                });
            }
        }
    }
}

fn repo_index_file_limit_from_env(
    value: Result<String, std::env::VarError>,
) -> Result<usize, String> {
    positive_limit_from_env(
        REPO_INDEX_FILE_LIMIT_ENV,
        DEFAULT_REPO_INDEX_FILE_LIMIT,
        value,
    )
}

fn positive_limit_from_env(
    env_name: &str,
    default: usize,
    value: Result<String, std::env::VarError>,
) -> Result<usize, String> {
    match value {
        Ok(raw) => {
            let parsed = raw
                .trim()
                .parse::<usize>()
                .map_err(|err| format!("{env_name} must be a positive integer: {err}"))?;
            if parsed == 0 {
                return Err(format!("{env_name} must be a positive integer"));
            }
            Ok(parsed)
        }
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{env_name} must be valid UTF-8")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_role_classification_covers_prod_test_and_helper() {
        assert_eq!(
            classify_python_file_role(Path::new("src/pkg/calc.py")),
            PythonFileRole::Production
        );
        assert_eq!(
            classify_python_file_role(Path::new("app.py")),
            PythonFileRole::Production
        );
        assert_eq!(
            classify_python_file_role(Path::new("tests/test_calc.py")),
            PythonFileRole::Test
        );
        assert_eq!(
            classify_python_file_role(Path::new("test_calc.py")),
            PythonFileRole::Test
        );
        assert_eq!(
            classify_python_file_role(Path::new("calc_test.py")),
            PythonFileRole::Test
        );
        assert_eq!(
            classify_python_file_role(Path::new("tests/conftest.py")),
            PythonFileRole::Helper
        );
        assert_eq!(
            classify_python_file_role(Path::new("tests/helpers/fixtures.py")),
            PythonFileRole::Helper
        );
        assert_eq!(
            classify_python_file_role(Path::new("src/pkg/schema_pb2.py")),
            PythonFileRole::Excluded
        );
        assert_eq!(
            classify_python_file_role(Path::new("generated_types.py")),
            PythonFileRole::Excluded
        );
    }

    #[test]
    fn repo_input_oversized_guard_detects_limit() {
        let err = "repo_scope_oversized: 801 indexed Python files exceed the RIPR_MAX_REPO_INDEX_FILES limit (800); analysis was not run to protect runner memory. Repair route: narrow the scope with a diff-based run (--base/--diff), or raise the limit via RIPR_MAX_REPO_INDEX_FILES=<number>.";
        assert!(PythonRepoInput::is_repo_scope_oversized(err));
        assert!(!PythonRepoInput::is_repo_scope_oversized(
            "some other error"
        ));
    }

    #[test]
    fn positive_limit_from_env_parses_or_defaults() -> Result<(), String> {
        let default_val =
            positive_limit_from_env("TEST_LIMIT", 50, Err(std::env::VarError::NotPresent))?;
        if default_val != 50 {
            return Err(format!("expected 50, got {default_val}"));
        }
        let parsed_val = positive_limit_from_env("TEST_LIMIT", 50, Ok("120".to_string()))?;
        if parsed_val != 120 {
            return Err(format!("expected 120, got {parsed_val}"));
        }
        if positive_limit_from_env("TEST_LIMIT", 50, Ok("0".to_string())).is_ok() {
            return Err("expected 0 to be an error".to_string());
        }
        if positive_limit_from_env("TEST_LIMIT", 50, Ok("invalid".to_string())).is_ok() {
            return Err("expected invalid to be an error".to_string());
        }
        Ok(())
    }

    fn write_test_file(path: &Path, content: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create_dir_all({}): {err}", parent.display()))?;
        }
        std::fs::write(path, content).map_err(|err| format!("write({}): {err}", path.display()))?;
        Ok(())
    }

    fn unique_test_tempdir(label: &str) -> Result<PathBuf, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| format!("system time: {err}"))?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ripr-py-repo-input-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("create_dir_all({}): {err}", dir.display()))?;
        Ok(dir)
    }

    #[test]
    fn discover_with_limit_flat_layout_and_symbol_identities() -> Result<(), String> {
        let root = unique_test_tempdir("flat")?;
        write_test_file(&root.join("pyproject.toml"), "[project]\nname = \"demo\"\n")?;
        write_test_file(
            &root.join("math.py"),
            "def add(a, b):\n    return a + b\n\nclass Calculator:\n    def mul(self, x, y):\n        return x * y\n",
        )?;
        write_test_file(
            &root.join("test_math.py"),
            "def test_add():\n    assert add(1, 2) == 3\n",
        )?;
        write_test_file(&root.join("conftest.py"), "import pytest\n")?;

        let input = PythonRepoInput::discover_with_limit(&root, 10)?;
        assert!(input.project_detected);
        assert_eq!(input.production_files, vec![PathBuf::from("math.py")]);
        assert_eq!(input.test_files, vec![PathBuf::from("test_math.py")]);
        assert_eq!(input.helper_files, vec![PathBuf::from("conftest.py")]);
        assert_eq!(input.skipped_generated_files, 0);

        // Native owner symbol IDs
        let symbol_ids: Vec<String> = input.owners.iter().map(|o| o.symbol_id().0).collect();
        assert!(
            symbol_ids.contains(&"python:math.py::add".to_string()),
            "expected symbol_id for add: {symbol_ids:?}"
        );
        assert!(
            symbol_ids.contains(&"python:math.py::Calculator.mul".to_string()),
            "expected symbol_id for Calculator.mul: {symbol_ids:?}"
        );

        // Tests should be extracted
        let test_names: Vec<&str> = input.tests.iter().map(|t| t.name.as_str()).collect();
        assert!(test_names.contains(&"test_add"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn discover_with_limit_src_layout_and_exclusions() -> Result<(), String> {
        let root = unique_test_tempdir("src-layout")?;
        write_test_file(
            &root.join("src/my_pkg/core.py"),
            "def run():\n    return True\n",
        )?;
        write_test_file(
            &root.join("tests/test_core.py"),
            "def test_run():\n    assert run()\n",
        )?;
        write_test_file(
            &root.join("tests/helpers/support.py"),
            "def helper():\n    pass\n",
        )?;
        // Excluded directory files
        write_test_file(&root.join(".venv/lib/site.py"), "x = 1\n")?;
        write_test_file(&root.join("__pycache__/core.cpython-311.py"), "x = 1\n")?;
        write_test_file(&root.join("target/debug/build.py"), "x = 1\n")?;
        write_test_file(&root.join(".tox/py311/bin/pytest.py"), "x = 1\n")?;
        // Excluded generated files
        write_test_file(&root.join("src/my_pkg/proto_pb2.py"), "# proto\n")?;
        write_test_file(&root.join("src/my_pkg/generated_api.py"), "# gen\n")?;

        let input = PythonRepoInput::discover_with_limit(&root, 50)?;
        assert_eq!(
            input.production_files,
            vec![PathBuf::from("src/my_pkg/core.py")]
        );
        assert_eq!(input.test_files, vec![PathBuf::from("tests/test_core.py")]);
        assert_eq!(
            input.helper_files,
            vec![PathBuf::from("tests/helpers/support.py")]
        );
        assert_eq!(input.skipped_generated_files, 2);

        // Excluded dirs should not appear
        for file in &input.files {
            let s = file.relative_path.to_string_lossy();
            assert!(!s.contains(".venv"));
            assert!(!s.contains("__pycache__"));
            assert!(!s.contains("target"));
            assert!(!s.contains(".tox"));
        }

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn discover_with_limit_exceeding_guard_fails_closed() -> Result<(), String> {
        let root = unique_test_tempdir("oversized")?;
        write_test_file(&root.join("a.py"), "x = 1\n")?;
        write_test_file(&root.join("b.py"), "y = 2\n")?;
        write_test_file(&root.join("test_a.py"), "def test_a(): pass\n")?;

        // Limit is 2, but we have 3 analyzable files
        let err = match PythonRepoInput::discover_with_limit(&root, 2) {
            Ok(_) => return Err("expected repo_scope_oversized error".to_string()),
            Err(e) => e,
        };
        assert!(PythonRepoInput::is_repo_scope_oversized(&err));
        assert!(
            err.contains("3 indexed Python files exceed the RIPR_MAX_REPO_INDEX_FILES limit (2)")
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn discover_with_limit_records_parse_errors_honestly() -> Result<(), String> {
        let root = unique_test_tempdir("parse-err")?;
        write_test_file(&root.join("valid.py"), "def ok(): pass\n")?;
        write_test_file(&root.join("syntax_error.py"), "def def broken(\n")?;

        let input = PythonRepoInput::discover_with_limit(&root, 10)?;
        assert_eq!(input.production_files.len(), 2);
        assert_eq!(input.parse_errors.len(), 1);
        assert_eq!(
            input.parse_errors[0].relative_path,
            PathBuf::from("syntax_error.py")
        );
        assert!(!input.limitations.is_empty());

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn discover_uses_default_limit() -> Result<(), String> {
        let root = unique_test_tempdir("discover-default")?;
        write_test_file(&root.join("core.py"), "def foo(): pass\n")?;

        let input = PythonRepoInput::discover(&root)?;
        assert_eq!(input.working_set_limit, DEFAULT_REPO_INDEX_FILE_LIMIT);
        assert_eq!(input.production_files, vec![PathBuf::from("core.py")]);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
