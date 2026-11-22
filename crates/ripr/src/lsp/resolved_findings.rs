//! Findings that became `exposed` since the previous committed snapshot
//! (#13636).
//!
//! When a user adds a test that appears to observe a changed behavior and
//! saves, the gap's diagnostic simply stops being published: the actionable
//! profile never publishes an `exposed` finding. Without a note the editor
//! goes quiet, which reads the same as an analysis failure. This module is
//! the single owner of that transition: at commit time it compares the
//! previous and the new snapshot and names the findings whose gap class
//! moved to `exposed`. Status projects the rows and clients only render
//! them.
//!
//! Conservative by construction:
//! - a row needs the same finding identity in both snapshots of the same
//!   root, candidate-current both times. A finding that is merely absent
//!   (deleted code, a changed diff scope) is never reported as exposed;
//! - the wording is static evidence ("a test appears to observe the changed
//!   behavior"), never a runtime mutation outcome.

use super::diagnostics::is_actionable_gap_class;
use super::state::AnalysisSnapshot;
use crate::domain::{DiagnosticWitness, ExposureClass, Finding};
use serde_json::Value;

/// Upper bound on listed rows; the count stays exact.
pub(super) const MAX_RESOLVED_FINDING_ROWS: usize = 20;

/// One finding whose gap became `exposed` since the previous snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ResolvedFinding {
    /// Workspace-relative source path, `/`-separated.
    pub(super) path: String,
    /// One-based source line of the changed behavior.
    pub(super) line: usize,
    /// The changed expression, as the finding names it.
    pub(super) expression: String,
    /// The gap class the previous snapshot carried, such as
    /// `weakly_exposed`.
    pub(super) previous_class: &'static str,
    /// The witness-selected related test the new snapshot names, when one
    /// exists; never a proximity-only test.
    pub(super) test: Option<String>,
}

/// The findings that were candidate-current actionable gaps in `previous`
/// and are candidate-current `exposed` in `current`, matched by finding id.
/// Empty when the roots differ. Sorted by path and line.
pub(super) fn resolved_since(
    previous: &AnalysisSnapshot,
    current: &AnalysisSnapshot,
) -> Vec<ResolvedFinding> {
    if previous.root != current.root {
        return Vec::new();
    }
    // Indexed once: this runs while the commit holds its locks.
    let current_by_id = current
        .findings
        .iter()
        .map(|finding| (finding.id.as_str(), finding))
        .collect::<std::collections::HashMap<_, _>>();
    let mut rows = previous
        .findings
        .iter()
        .filter(|finding| was_open_gap(finding))
        .filter_map(|before| {
            let after = *current_by_id.get(before.id.as_str())?;
            is_now_exposed(after).then(|| ResolvedFinding {
                path: after
                    .probe
                    .location
                    .file
                    .to_string_lossy()
                    .replace('\\', "/"),
                line: after.probe.location.line,
                expression: after.probe.expression.clone(),
                previous_class: before.class.as_str(),
                test: DiagnosticWitness::selected_related_test(after).map(|test| test.name.clone()),
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        (&left.path, left.line, &left.expression).cmp(&(&right.path, right.line, &right.expression))
    });
    rows.dedup();
    rows
}

fn was_open_gap(finding: &Finding) -> bool {
    finding.is_candidate_actionable() && is_actionable_gap_class(&finding.class)
}

fn is_now_exposed(finding: &Finding) -> bool {
    finding.is_candidate_actionable() && matches!(finding.class, ExposureClass::Exposed)
}

/// The human line for one row, in static vocabulary.
pub(super) fn resolved_row_text(row: &ResolvedFinding) -> String {
    let by = row
        .test
        .as_deref()
        .map(|test| format!(" (`{test}`)"))
        .unwrap_or_default();
    format!(
        "{}:{}: now exposed: a test appears to observe the changed behavior{by}",
        row.path, row.line
    )
}

/// The `resolved_findings` member of the analysis status payload: the exact
/// count, at most [`MAX_RESOLVED_FINDING_ROWS`] rows, and how many were left
/// out. `count` is 0 with no rows when nothing moved.
pub(super) fn resolved_findings_status(rows: &[ResolvedFinding]) -> Value {
    let listed = rows
        .iter()
        .take(MAX_RESOLVED_FINDING_ROWS)
        .map(|row| {
            serde_json::json!({
                "path": row.path,
                "line": row.line,
                "expression": row.expression,
                "previous_class": row.previous_class,
                "class": ExposureClass::Exposed.as_str(),
                "test": row.test,
                "text": resolved_row_text(row),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "count": rows.len(),
        "rows": listed,
        "omitted": rows.len().saturating_sub(MAX_RESOLVED_FINDING_ROWS),
        "evidence": "static",
    })
}
