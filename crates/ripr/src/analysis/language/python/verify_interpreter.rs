//! Producer-owned interpreter and confidence for the pytest verify route
//! (#13801, follow-up to #13617).
//!
//! The classifier spells every generated pytest command with PATH `python`.
//! Before findings leave the Python producer, this pass decides once, from
//! static facts at the selected root, which interpreter the printed command
//! names and how confident the route is. Every surface (human, JSON, SARIF,
//! the Python repair card, pilot, MCP) then reads the same evidence lines, so
//! no renderer has to look at the filesystem.
//!
//! - A project `.venv/bin/python` or `venv/bin/python` (POSIX only) replaces
//!   `python`, root-relative, because that is the interpreter the project's
//!   pytest lives in.
//! - When that venv statically lacks pytest, or its layout cannot be read
//!   statically, the route confidence drops below `high`.
//! - With no project venv nothing changes: the command keeps PATH `python` at
//!   its existing confidence. A static producer cannot know whether system
//!   Python has pytest, and a probe would make reports host-dependent;
//!   `ripr doctor` checks the PATH interpreter instead.
//!
//! - A test file under a nested Python project (a directory below the root
//!   that holds `pyproject.toml`, `setup.cfg`, `setup.py`, `pytest.ini` or
//!   `tox.ini` and owns the test file, #15302) is verified from that
//!   directory: the command carries the test path and the venv interpreter
//!   relative to it, and `suggested_verify_cwd: <dir>` names the directory
//!   relative to the root. Run from the root, pytest cannot import the
//!   project's own package. A root-level project carries no cwd line.
//!
//! Detection reads directory listings and `pyvenv.cfg` only. It never runs
//! the interpreter or any code from the checkout.

use crate::analysis::python_env::{VenvPytest, project_venv_pytest, project_venv_python};
use crate::domain::{Finding, LanguageId};
use std::collections::BTreeMap;
use std::path::{Component, Path};

const SUGGESTED: &str = "suggested_verify_command: ";
const CONFIDENCE: &str = "suggested_verify_command_confidence: ";
const PER_TEST: &str = "test_verify_command: ";
const PATH_PYTEST: &str = "python -m pytest ";
const TEST_FILE: &str = "suggested_test_file: ";
const TEST_NODE: &str = "suggested_test_node_id: ";
/// Evidence line naming the static pytest fact behind a venv route.
pub(super) const VENV_PYTEST: &str = "suggested_verify_venv_pytest: ";
/// Evidence line naming the root-relative directory a nested project's verify
/// command runs from. Absent when the command runs from the root.
pub(crate) const VERIFY_CWD: &str = "suggested_verify_cwd: ";
/// Files that mark a directory as a Python project (pytest rootdir) root.
const PROJECT_MARKERS: [&str; 5] = [
    "pyproject.toml",
    "setup.cfg",
    "setup.py",
    "pytest.ini",
    "tox.ini",
];

/// The nearest directory strictly below `root` that holds a Python project
/// marker and contains `test_file`, as a root-relative POSIX path. `None` when
/// the owning project is the root (or no marker exists), so the command stays
/// root-relative exactly as before.
fn nested_project_dir(root: &Path, test_file: &str) -> Option<String> {
    let path = Path::new(test_file);
    let parts: Vec<&str> = path
        .components()
        .map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let dirs = parts.split_last()?.1;
    (1..=dirs.len()).rev().find_map(|depth| {
        let dir = dirs[..depth].join("/");
        let marked = PROJECT_MARKERS
            .iter()
            .any(|marker| root.join(&dir).join(marker).is_file());
        marked.then_some(dir)
    })
}

/// Rewrite the Python findings of one analysis root: choose each finding's
/// project directory, then the interpreter and confidence from that
/// directory's static facts.
///
/// The cwd and the interpreter always come from the same directory: a finding
/// whose commands cannot be moved into its nested project keeps the root
/// decision and the root-relative route. A `test_verify_command:` line for a
/// test outside the nested project stays root-relative with PATH `python`,
/// never this project's venv. When the nested project has no venv but the root
/// has one, PATH `python` may not hold pytest, so the route drops to `medium`.
pub(super) fn apply_at(root: &Path, findings: &mut [Finding]) {
    let mut decisions: BTreeMap<Option<String>, PytestVerifyInterpreter> = BTreeMap::new();
    let root_decision = PytestVerifyInterpreter::at(root);
    for finding in findings
        .iter_mut()
        .filter(|finding| finding.language == Some(LanguageId::Python))
    {
        let nested = finding
            .evidence
            .iter()
            .find_map(|line| line.strip_prefix(TEST_FILE))
            .and_then(|file| nested_project_dir(root, file))
            .and_then(|dir| {
                let foreign = relativize_to_project(&dir, &mut finding.evidence)?;
                Some((dir, foreign))
            });
        let Some((dir, foreign)) = nested else {
            root_decision.rewrite_evidence(&mut finding.evidence, &[]);
            continue;
        };
        let decision = decisions
            .entry(Some(dir.clone()))
            .or_insert_with(|| PytestVerifyInterpreter::at(&root.join(&dir)));
        decision.rewrite_evidence(&mut finding.evidence, &foreign);
        if decision.venv_python.is_none() && root_decision.venv_python.is_some() {
            cap_confidence(&mut finding.evidence, "medium");
        }
    }
}

/// Lower the route confidence to `cap` unless it is already at or below it.
fn cap_confidence(evidence: &mut [String], cap: &'static str) {
    let rank = |value: &str| match value {
        "high" => 2,
        "medium" => 1,
        _ => 0,
    };
    for line in evidence.iter_mut() {
        if let Some(current) = line.strip_prefix(CONFIDENCE)
            && rank(current) > rank(cap)
        {
            *line = format!("{CONFIDENCE}{cap}");
        }
    }
}

/// `selector` (a shell-quoted `path::node` token) with the leading `dir/`
/// removed from its path, or `None` when the path is not under `dir`.
fn strip_dir_from_selector(selector: &str, dir: &str) -> Option<String> {
    for quote in ["", "'", "\""] {
        let prefix = format!("{quote}{dir}/");
        if let Some(rest) = selector.strip_prefix(&prefix) {
            return Some(format!("{quote}{rest}"));
        }
    }
    None
}

/// The route decision for one analysis root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PytestVerifyInterpreter {
    /// Root-relative venv interpreter; `None` keeps PATH `python`.
    venv_python: Option<&'static str>,
    pytest: VenvPytest,
}

impl PytestVerifyInterpreter {
    fn at(root: &Path) -> Self {
        Self::from_facts(project_venv_python(root), project_venv_pytest(root))
    }

    fn from_facts(venv_python: Option<&'static str>, pytest: VenvPytest) -> Self {
        if venv_python.is_none() {
            return Self {
                venv_python: None,
                pytest: VenvPytest::NoVenv,
            };
        }
        Self {
            venv_python,
            pytest,
        }
    }

    /// The confidence that replaces the classifier's when the selected venv
    /// does not statically establish pytest; `None` keeps it.
    fn confidence_cap(&self) -> Option<&'static str> {
        match self.pytest {
            VenvPytest::NoVenv | VenvPytest::Present(_) => None,
            VenvPytest::Unknown => Some("medium"),
            VenvPytest::Missing => Some("low"),
        }
    }

    fn pytest_fact(&self) -> &'static str {
        match self.pytest {
            VenvPytest::NoVenv => "no_venv",
            VenvPytest::Present(_) => "present",
            VenvPytest::Missing => "missing",
            VenvPytest::Unknown => "unknown",
        }
    }

    /// Rewrite one Python finding's evidence lines, leaving the lines at the
    /// `foreign` indices (tests outside the nested project) alone. Idempotent:
    /// an already-rewritten command no longer starts with PATH `python`.
    fn rewrite_evidence(&self, evidence: &mut Vec<String>, foreign: &[usize]) {
        let Some(venv_python) = self.venv_python else {
            return;
        };
        let mut suggested = false;
        for (index, line) in evidence.iter_mut().enumerate() {
            if foreign.contains(&index) {
                continue;
            }
            for prefix in [SUGGESTED, PER_TEST] {
                let Some(selector) = line
                    .strip_prefix(prefix)
                    .and_then(|command| command.strip_prefix(PATH_PYTEST))
                else {
                    continue;
                };
                *line = format!("{prefix}{venv_python} -m pytest {selector}");
                suggested |= prefix == SUGGESTED;
            }
        }
        if !suggested {
            return;
        }
        if let Some(cap) = self.confidence_cap() {
            for line in evidence.iter_mut() {
                if line.starts_with(CONFIDENCE) {
                    *line = format!("{CONFIDENCE}{cap}");
                }
            }
        }
        if evidence.iter().any(|line| line.starts_with(VENV_PYTEST)) {
            return;
        }
        let at = evidence
            .iter()
            .position(|line| line.starts_with(CONFIDENCE))
            .map_or(evidence.len(), |at| at + 1);
        evidence.insert(at, format!("{VENV_PYTEST}{}", self.pytest_fact()));
    }
}

/// Move the PATH pytest commands of `evidence` into the project `dir` and name
/// it. Returns the indices of `test_verify_command:` lines left root-relative
/// because their test lies outside `dir`, or `None` (evidence untouched) when
/// the suggested command cannot be moved, so the caller keeps the root route.
/// A finding already carrying a cwd was moved before and returns `Some`.
fn relativize_to_project(dir: &str, evidence: &mut Vec<String>) -> Option<Vec<usize>> {
    if evidence.iter().any(|line| line.starts_with(VERIFY_CWD)) {
        // Already moved: a per-test line still spelled with PATH `python`
        // either names a test outside `dir` or there is no venv to apply.
        return Some(
            evidence
                .iter()
                .enumerate()
                .filter(|(_, line)| {
                    line.strip_prefix(PER_TEST)
                        .is_some_and(|command| command.starts_with(PATH_PYTEST))
                })
                .map(|(index, _)| index)
                .collect(),
        );
    }
    let value = |prefix: &str| {
        evidence
            .iter()
            .find_map(|line| line.strip_prefix(prefix))
            .map(str::to_string)
    };
    let suggested = value(SUGGESTED).filter(|c| c.starts_with(PATH_PYTEST))?;
    let relative_suggested = {
        let exact = value(TEST_FILE)
            .zip(value(TEST_NODE))
            .and_then(|(file, node)| {
                let name = node.strip_prefix(&file)?.strip_prefix("::")?.to_string();
                let rel = file.strip_prefix(dir)?.strip_prefix('/')?.to_string();
                crate::domain::python_pytest_verify_display(&rel, &name, cfg!(windows))
            });
        match exact {
            Some(command) => command,
            None => {
                let selector = suggested.strip_prefix(PATH_PYTEST).unwrap_or_default();
                match strip_dir_from_selector(selector, dir) {
                    Some(rest) => format!("{PATH_PYTEST}{rest}"),
                    None => return None,
                }
            }
        }
    };
    let at = evidence
        .iter()
        .position(|line| line.starts_with(CONFIDENCE))
        .map_or(evidence.len(), |at| at + 1);
    evidence.insert(at, format!("{VERIFY_CWD}{dir}"));
    let mut foreign = Vec::new();
    for (index, line) in evidence.iter_mut().enumerate() {
        if let Some(command) = line.strip_prefix(SUGGESTED) {
            if command == suggested {
                *line = format!("{SUGGESTED}{relative_suggested}");
            }
        } else if let Some(selector) = line
            .strip_prefix(PER_TEST)
            .and_then(|command| command.strip_prefix(PATH_PYTEST))
        {
            match strip_dir_from_selector(selector, dir) {
                Some(rest) => *line = format!("{PER_TEST}{PATH_PYTEST}{rest}"),
                None => foreign.push(index),
            }
        }
    }
    Some(foreign)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A per-test temp root removed on drop.
    struct TempRoot(std::path::PathBuf);
    impl TempRoot {
        fn new(tag: &str) -> std::io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "ripr-verify-interpreter-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path)?;
            Ok(Self(path))
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn evidence(lines: &[&str]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    const ROUTE: [&str; 4] = [
        "suggested_verify_command: python -m pytest tests/test_a.py::test_total",
        "suggested_verify_command_confidence: high",
        "suggested_test_location_reason: strengthen existing weak pytest relation",
        "test_verify_command: python -m pytest tests/test_a.py::test_total (test_total)",
    ];

    fn applied(venv: Option<&'static str>, pytest: VenvPytest) -> Vec<String> {
        let mut lines = evidence(&ROUTE);
        let decision = PytestVerifyInterpreter::from_facts(venv, pytest);
        decision.rewrite_evidence(&mut lines, &[]);
        // Idempotent: a second pass changes nothing.
        let once = lines.clone();
        decision.rewrite_evidence(&mut lines, &[]);
        assert_eq!(lines, once);
        once
    }

    #[test]
    fn venv_with_pytest_names_the_venv_interpreter_and_keeps_high() {
        let evidence = applied(
            Some(".venv/bin/python"),
            VenvPytest::Present(Some("9.2.0".to_string())),
        );
        assert_eq!(
            evidence,
            [
                "suggested_verify_command: .venv/bin/python -m pytest tests/test_a.py::test_total",
                "suggested_verify_command_confidence: high",
                "suggested_verify_venv_pytest: present",
                "suggested_test_location_reason: strengthen existing weak pytest relation",
                "test_verify_command: .venv/bin/python -m pytest tests/test_a.py::test_total (test_total)",
            ]
        );
    }

    #[test]
    fn venv_without_pytest_is_low_and_unreadable_venv_is_medium() {
        let missing = applied(Some("venv/bin/python"), VenvPytest::Missing);
        assert_eq!(
            missing[0],
            "suggested_verify_command: venv/bin/python -m pytest tests/test_a.py::test_total"
        );
        assert_eq!(missing[1], "suggested_verify_command_confidence: low");
        assert_eq!(missing[2], "suggested_verify_venv_pytest: missing");
        let unknown = applied(Some(".venv/bin/python"), VenvPytest::Unknown);
        assert_eq!(unknown[1], "suggested_verify_command_confidence: medium");
        assert_eq!(unknown[2], "suggested_verify_venv_pytest: unknown");
    }

    #[test]
    fn no_venv_keeps_path_python_and_confidence_byte_for_byte() {
        assert_eq!(applied(None, VenvPytest::NoVenv), ROUTE);
        // A stale `pytest` fact without an interpreter cannot rewrite.
        assert_eq!(applied(None, VenvPytest::Missing), ROUTE);
    }

    #[test]
    fn unittest_and_findings_without_a_pytest_route_are_untouched() {
        let decision =
            PytestVerifyInterpreter::from_facts(Some(".venv/bin/python"), VenvPytest::Missing);
        for lines in [
            &[
                "suggested_verify_command: python -m unittest tests.test_a.TestA.test_total",
                "suggested_verify_command_confidence: high",
            ][..],
            &["suggested_verify_command_confidence: high", "owner: total"][..],
            &[][..],
        ] {
            let mut rewritten = evidence(lines);
            decision.rewrite_evidence(&mut rewritten, &[]);
            assert_eq!(rewritten, lines);
        }
        // Per-test commands follow the venv without capping a confidence
        // that no suggested route carries.
        let mut per_test =
            evidence(&["test_verify_command: python -m pytest t.py::test_a (test_a)"]);
        decision.rewrite_evidence(&mut per_test, &[]);
        assert_eq!(
            per_test,
            ["test_verify_command: .venv/bin/python -m pytest t.py::test_a (test_a)"]
        );
    }

    /// #15302: a related test outside the nested project keeps its
    /// root-relative PATH command; only in-project lines take the project's
    /// venv, and a second pass changes nothing.
    #[test]
    fn nested_move_leaves_foreign_related_tests_root_relative_without_the_venv() {
        let mut lines = evidence(&[
            "suggested_verify_command: python -m pytest py/tests/test_a.py::test_total",
            "suggested_verify_command_confidence: high",
            "test_verify_command: python -m pytest py/tests/test_a.py::test_total (test_total)",
            "test_verify_command: python -m pytest other/tests/test_b.py::test_b (test_b)",
        ]);
        let foreign = relativize_to_project("py", &mut lines);
        assert_eq!(foreign, Some(vec![4]));
        let decision = PytestVerifyInterpreter::from_facts(
            Some(".venv/bin/python"),
            VenvPytest::Present(Some("9.2.0".to_string())),
        );
        decision.rewrite_evidence(&mut lines, &foreign.unwrap_or_default());
        let expected = [
            "suggested_verify_command: .venv/bin/python -m pytest tests/test_a.py::test_total",
            "suggested_verify_command_confidence: high",
            "suggested_verify_venv_pytest: present",
            "suggested_verify_cwd: py",
            "test_verify_command: .venv/bin/python -m pytest tests/test_a.py::test_total (test_total)",
            "test_verify_command: python -m pytest other/tests/test_b.py::test_b (test_b)",
        ];
        assert_eq!(lines, expected);
        let again = relativize_to_project("py", &mut lines);
        decision.rewrite_evidence(&mut lines, &again.unwrap_or_default());
        assert_eq!(lines, expected);
    }

    /// #15302: a suggested command that cannot move into the nested project
    /// leaves the evidence untouched, so the caller keeps the root route.
    #[test]
    fn nested_move_that_cannot_relativize_leaves_the_root_route() {
        let original = evidence(&[
            "suggested_verify_command: python -m pytest other/tests/test_b.py::test_b",
            "suggested_verify_command_confidence: high",
        ]);
        let mut lines = original.clone();
        assert_eq!(relativize_to_project("py", &mut lines), None);
        assert_eq!(lines, original);
        let mut unittest = evidence(&[
            "suggested_verify_command: python -m unittest py.tests.test_a",
            "suggested_verify_command_confidence: high",
        ]);
        let before = unittest.clone();
        assert_eq!(relativize_to_project("py", &mut unittest), None);
        assert_eq!(unittest, before);
    }

    #[test]
    fn cap_confidence_only_lowers() {
        for (current, capped) in [("high", "medium"), ("medium", "medium"), ("low", "low")] {
            let mut lines = vec![format!("{CONFIDENCE}{current}")];
            cap_confidence(&mut lines, "medium");
            assert_eq!(lines, [format!("{CONFIDENCE}{capped}")]);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn root_with_dot_venv_selects_its_interpreter_from_static_facts() -> std::io::Result<()> {
        let dir = TempRoot::new("posix")?;
        assert_eq!(
            PytestVerifyInterpreter::at(dir.path()),
            PytestVerifyInterpreter::from_facts(None, VenvPytest::NoVenv)
        );
        let site = dir.path().join(".venv/lib/python3.13/site-packages");
        std::fs::create_dir_all(site.join("pytest-9.2.0.dist-info"))?;
        std::fs::create_dir_all(dir.path().join(".venv/bin"))?;
        std::fs::write(dir.path().join(".venv/bin/python"), "")?;
        assert_eq!(
            PytestVerifyInterpreter::at(dir.path()),
            PytestVerifyInterpreter::from_facts(
                Some(".venv/bin/python"),
                VenvPytest::Present(Some("9.2.0".to_string()))
            )
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn windows_keeps_path_python_even_with_a_venv_layout() -> std::io::Result<()> {
        let dir = TempRoot::new("windows")?;
        std::fs::create_dir_all(dir.path().join(".venv/bin"))?;
        std::fs::write(dir.path().join(".venv/bin/python"), "")?;
        let mut lines = evidence(&ROUTE);
        PytestVerifyInterpreter::at(dir.path()).rewrite_evidence(&mut lines, &[]);
        assert_eq!(lines, ROUTE);
        Ok(())
    }
}
