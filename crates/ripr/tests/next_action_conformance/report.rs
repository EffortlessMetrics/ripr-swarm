//! Deterministic report DTO for the #6307 first-hour next-action conformance harness.

use serde::{Deserialize, Serialize};

/// Closed result vocabulary (#6307). Only the first five are success for a
/// deliberately selected positive/limitation row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowResult {
    AdvancedExpectedState,
    TerminalStateConfirmed,
    HonestLimitation,
    SelectionRequired,
    PrerequisiteRequired,
    WrongSubject,
    WrongRootOrDiff,
    StaleAction,
    CommandRefused,
    UnavailableRendering,
    MissingProducerArtifact,
    ManualPlumbingRequired,
    NonterminalOrCycle,
    InstrumentFailure,
    NotRun,
}

impl RowResult {
    pub fn is_acceptable_positive(self) -> bool {
        matches!(
            self,
            RowResult::AdvancedExpectedState
                | RowResult::TerminalStateConfirmed
                | RowResult::HonestLimitation
                | RowResult::SelectionRequired
                | RowResult::PrerequisiteRequired
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RowReport {
    pub row_id: String,
    pub action_family: String,
    pub producer_surface: String,
    pub selected: bool,
    pub executed: bool,
    pub result: RowResult,
    pub reason: String,
    pub expected_transition: String,
    pub observed_transition: String,
    pub action_identity: String,
    pub fixture_root: String,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConformanceReport {
    pub schema_version: String,
    pub candidate_binary: String,
    pub selected_count: u32,
    pub executed_count: u32,
    pub acceptable_positive_count: u32,
    pub rows: Vec<RowReport>,
    pub limitations: Vec<String>,
}

impl ConformanceReport {
    pub fn new(candidate_binary: String) -> Self {
        Self {
            schema_version: "next_action_conformance.v1".to_string(),
            candidate_binary,
            selected_count: 0,
            executed_count: 0,
            acceptable_positive_count: 0,
            rows: Vec::new(),
            limitations: vec![
                "Slice 1: CLI-only projection parity; LSP/MCP/editor rows remain follow-ups."
                    .to_string(),
                "Slice 1: formal/property journey extension rows from #10745 are not in denominator yet."
                    .to_string(),
            ],
        }
    }

    pub fn push_row(&mut self, row: RowReport) {
        if row.selected {
            self.selected_count += 1;
        }
        if row.executed {
            self.executed_count += 1;
        }
        if row.selected && row.result.is_acceptable_positive() {
            self.acceptable_positive_count += 1;
        }
        self.rows.push(row);
    }

    pub fn aggregate_pass(&self) -> Result<(), String> {
        if self.selected_count == 0 || self.executed_count == 0 {
            return Err(format!(
                "zero selected ({}) or zero executed ({}) cannot aggregate to pass",
                self.selected_count, self.executed_count
            ));
        }
        for row in &self.rows {
            if !row.selected {
                continue;
            }
            if !row.executed {
                return Err(format!(
                    "selected row {} must be executed before aggregation (result {:?})",
                    row.row_id, row.result
                ));
            }
            if row.row_id.starts_with("control_") {
                if row.result.is_acceptable_positive()
                    || row.result == RowResult::WrongRootOrDiff
                    || row.result == RowResult::WrongSubject
                    || row.result == RowResult::CommandRefused
                {
                    continue;
                }
                return Err(format!(
                    "control row {} expected distinct refusal vocabulary, got {:?}: {}",
                    row.row_id, row.result, row.reason
                ));
            }
            if !row.result.is_acceptable_positive() {
                return Err(format!(
                    "positive row {} failed: {:?} — {}",
                    row.row_id, row.result, row.reason
                ));
            }
        }
        Ok(())
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# Next-action conformance report\n\n");
        out.push_str(&format!("- schema: `{}`\n", self.schema_version));
        out.push_str(&format!("- binary: `{}`\n", self.candidate_binary));
        out.push_str(&format!(
            "- selected / executed / acceptable-positive: {} / {} / {}\n\n",
            self.selected_count, self.executed_count, self.acceptable_positive_count
        ));
        for row in &self.rows {
            out.push_str(&format!(
                "## {} ({}) — {:?}\n\n{}\n\n",
                row.row_id, row.action_family, row.result, row.reason
            ));
        }
        if !self.limitations.is_empty() {
            out.push_str("## Non-claims\n\n");
            for item in &self.limitations {
                out.push_str(&format!("- {item}\n"));
            }
        }
        out
    }
}

#[cfg(test)]
mod aggregate_tests {
    use super::*;

    fn sample_row(selected: bool, executed: bool) -> RowReport {
        RowReport {
            row_id: "row".to_string(),
            action_family: "family".to_string(),
            producer_surface: "surface".to_string(),
            selected,
            executed,
            result: RowResult::HonestLimitation,
            reason: "reason".to_string(),
            expected_transition: "expected".to_string(),
            observed_transition: "observed".to_string(),
            action_identity: "id".to_string(),
            fixture_root: "/tmp/fixture".to_string(),
            limitations: Vec::new(),
        }
    }

    #[test]
    fn aggregate_rejects_selected_but_unexecuted_rows() {
        let mut report = ConformanceReport::new("ripr-test".to_string());
        report.push_row(sample_row(true, true));
        report.push_row({
            let mut row = sample_row(true, false);
            row.row_id = "skipped".to_string();
            row
        });
        let aggregate = report.aggregate_pass();
        assert!(
            matches!(
                &aggregate,
                Err(err) if err.contains("skipped") && err.contains("executed")
            ),
            "{aggregate:?}"
        );
    }
}
