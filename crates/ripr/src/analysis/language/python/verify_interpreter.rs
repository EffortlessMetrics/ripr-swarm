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
//! Detection reads directory listings and `pyvenv.cfg` only. It never runs
//! the interpreter or any code from the checkout.

use crate::analysis::python_env::{VenvPytest, project_venv_pytest, project_venv_python};
use crate::domain::{Finding, LanguageId};
use std::path::Path;

const SUGGESTED: &str = "suggested_verify_command: ";
const CONFIDENCE: &str = "suggested_verify_command_confidence: ";
const PER_TEST: &str = "test_verify_command: ";
const PATH_PYTEST: &str = "python -m pytest ";
/// Evidence line naming the static pytest fact behind a venv route.
pub(super) const VENV_PYTEST: &str = "suggested_verify_venv_pytest: ";

/// The route decision for one analysis root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PytestVerifyInterpreter {
    /// Root-relative venv interpreter; `None` keeps PATH `python`.
    venv_python: Option<&'static str>,
    pytest: VenvPytest,
}

impl PytestVerifyInterpreter {
    pub(super) fn at(root: &Path) -> Self {
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

    /// Rewrite the PATH pytest commands of Python findings in place.
    pub(super) fn apply(&self, findings: &mut [Finding]) {
        for finding in findings
            .iter_mut()
            .filter(|finding| finding.language == Some(LanguageId::Python))
        {
            self.rewrite_evidence(&mut finding.evidence);
        }
    }

    /// Rewrite one Python finding's evidence lines. Idempotent: an
    /// already-rewritten command no longer starts with PATH `python`.
    fn rewrite_evidence(&self, evidence: &mut Vec<String>) {
        let Some(venv_python) = self.venv_python else {
            return;
        };
        let mut suggested = false;
        for line in evidence.iter_mut() {
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
        decision.rewrite_evidence(&mut lines);
        // Idempotent: a second pass changes nothing.
        let once = lines.clone();
        decision.rewrite_evidence(&mut lines);
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
            decision.rewrite_evidence(&mut rewritten);
            assert_eq!(rewritten, lines);
        }
        // Per-test commands follow the venv without capping a confidence
        // that no suggested route carries.
        let mut per_test =
            evidence(&["test_verify_command: python -m pytest t.py::test_a (test_a)"]);
        decision.rewrite_evidence(&mut per_test);
        assert_eq!(
            per_test,
            ["test_verify_command: .venv/bin/python -m pytest t.py::test_a (test_a)"]
        );
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
        PytestVerifyInterpreter::at(dir.path()).rewrite_evidence(&mut lines);
        assert_eq!(lines, ROUTE);
        Ok(())
    }
}
