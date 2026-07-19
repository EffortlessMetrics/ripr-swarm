//! Typed doctor/preflight report for machine-readable output (`ripr doctor --json`).
//!
//! See #1771 / #1614. The report captures the core checks (root, Cargo.toml,
//! configuration, and tool availability) as typed `DoctorCheck` values and
//! leaves deeper sub-checks (languages, cache, Perl, and test surfaces) for a
//! follow-up projection. The structure here proves the dual human/JSON
//! projection without a massive one-shot refactor.

use serde::Serialize;

/// The top-level doctor status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    /// All top-level checks passed.
    Pass,
    /// One or more top-level checks failed.
    Fail,
}

/// A single typed doctor check (root, Cargo.toml, tool availability).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DoctorCheck {
    /// The check name (e.g. "root_directory", "cargo_toml", "tool_git").
    pub name: String,
    /// The check status.
    pub status: DoctorStatus,
    /// Human-readable evidence (e.g. "Cargo.toml found at /workspace").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// A text-based section from a deeper check (languages, cache, etc.).
/// Typed checks replace these incrementally.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DoctorSection {
    /// The section name (e.g. "detected_languages", "cache_status").
    pub name: String,
    /// The captured text output.
    pub lines: Vec<String>,
}

/// The full doctor report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DoctorReport {
    pub schema_version: &'static str,
    pub tool: &'static str,
    pub root: String,
    pub status: DoctorStatus,
    pub checks: Vec<DoctorCheck>,
    pub sections: Vec<DoctorSection>,
}

impl DoctorReport {
    pub const SCHEMA_VERSION: &'static str = "0.1";

    pub fn new(root: &str) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            tool: "ripr",
            root: root.to_string(),
            status: DoctorStatus::Pass,
            checks: Vec::new(),
            sections: Vec::new(),
        }
    }

    /// Add a typed check and update the overall status.
    pub fn add_check(&mut self, name: &str, status: DoctorStatus, evidence: Option<String>) {
        if status == DoctorStatus::Fail {
            self.status = DoctorStatus::Fail;
        }
        self.checks.push(DoctorCheck {
            name: name.to_string(),
            status,
            evidence,
        });
    }

    /// Add a text-based section.
    #[cfg(test)]
    pub fn add_section(&mut self, name: &str, lines: Vec<String>) {
        self.sections.push(DoctorSection {
            name: name.to_string(),
            lines,
        });
    }

    /// Render the report as human-readable text (mirrors the existing prose output).
    #[cfg(test)]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str("ripr doctor\n");
        out.push_str(&format!("- root: {}\n", self.root));
        for check in &self.checks {
            let icon = if check.status == DoctorStatus::Pass {
                "✓"
            } else {
                "!"
            };
            if let Some(evidence) = &check.evidence {
                out.push_str(&format!("{icon} {evidence}\n"));
            } else {
                out.push_str(&format!("{icon} {}\n", check.name));
            }
        }
        for section in &self.sections {
            for line in &section.lines {
                out.push_str(line);
                out.push('\n');
            }
        }
        match self.status {
            DoctorStatus::Pass => out.push_str("✓ doctor checks passed\n"),
            DoctorStatus::Fail => {
                out.push_str("! doctor checks failed; run `ripr doctor --help` for usage\n")
            }
        }
        out
    }

    /// Render the report as JSON.
    pub fn render_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize doctor report: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_is_pass() {
        let report = DoctorReport::new("/workspace");
        assert_eq!(report.status, DoctorStatus::Pass);
        assert!(report.checks.is_empty());
    }

    #[test]
    fn failed_check_flips_overall_status() {
        let mut report = DoctorReport::new("/workspace");
        report.add_check(
            "root_directory",
            DoctorStatus::Pass,
            Some("root exists".to_string()),
        );
        assert_eq!(report.status, DoctorStatus::Pass);
        report.add_check(
            "cargo_toml",
            DoctorStatus::Fail,
            Some("no Cargo.toml".to_string()),
        );
        assert_eq!(report.status, DoctorStatus::Fail);
    }

    #[test]
    fn render_text_shows_pass_and_fail_checks() {
        let mut report = DoctorReport::new("/workspace");
        report.add_check(
            "root_directory",
            DoctorStatus::Pass,
            Some("root exists".to_string()),
        );
        report.add_check(
            "cargo_toml",
            DoctorStatus::Fail,
            Some("no Cargo.toml".to_string()),
        );
        report.add_section("guidance", vec!["run ripr doctor --help".to_string()]);
        let text = report.render_text();
        assert!(text.contains("✓ root exists"));
        assert!(text.contains("! no Cargo.toml"));
        assert!(text.contains("run ripr doctor --help"));
        assert!(text.contains("! doctor checks failed"));
    }

    #[test]
    fn render_text_names_checks_without_evidence_and_passes() {
        let mut report = DoctorReport::new("/workspace");
        report.add_check("config", DoctorStatus::Pass, None);
        let text = report.render_text();
        assert!(text.contains("✓ config"));
        assert!(text.contains("✓ doctor checks passed"));
    }

    #[test]
    fn render_json_produces_valid_json() -> Result<(), String> {
        let mut report = DoctorReport::new("/workspace");
        report.add_check(
            "root_directory",
            DoctorStatus::Pass,
            Some("root exists".to_string()),
        );
        report.add_section("cache", vec!["cache: target/ripr/cache".to_string()]);
        let json = report.render_json()?;
        let parsed: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("invalid JSON: {e}"))?;
        assert_eq!(parsed["schema_version"], "0.1");
        assert_eq!(parsed["tool"], "ripr");
        assert_eq!(parsed["status"], "pass");
        assert_eq!(parsed["checks"][0]["name"], "root_directory");
        assert_eq!(parsed["checks"][0]["status"], "pass");
        assert_eq!(parsed["sections"][0]["name"], "cache");
        Ok(())
    }

    #[test]
    fn sections_are_optional() -> Result<(), String> {
        let report = DoctorReport::new("/workspace");
        let json = report.render_json()?;
        let parsed: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("invalid JSON: {e}"))?;
        assert!(parsed["sections"].is_array());
        assert_eq!(parsed["sections"].as_array().map(Vec::len), Some(0));
        Ok(())
    }
}
