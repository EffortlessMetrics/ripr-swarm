//! One shared mixed-language completeness view (#6475, parent #5546).
//!
//! A read-only projection of the sealed `LanguageAggregate`. Surfaces (human,
//! JSON, gate inputs, editor and agent status) must say the same thing about
//! each language, so the per-language row, how that row is disclosed, and the
//! public `LanguageRun` compatibility projection are derived here once, from
//! the children alone. The view never strengthens a child, changes an
//! identity, status, count or limitation, or decides gate policy; renderers
//! may budget detail but cannot change what a row says.
//!
//! This first slice adds the owner and the agreement checks. Moving each
//! surface onto the view directly is tracked on #6475.

use super::{DiffCompleteness, LanguageAggregate, LanguageOutcomeStatus, OutcomeMode};
use crate::analysis::LanguageRun;

/// Where a row must be visible on the existing public surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LanguageDisclosure {
    /// Evidenced or nothing in scope: no public row is expected.
    Silent,
    /// A non-success `language_runs[]` row with this status string.
    PublicRun(&'static str),
    /// A top-level typed `AnalysisOutcome` limitation, not a `language_runs[]`
    /// row (changed source with no ripr adapter).
    TypedLimitation,
    /// Recorded in the aggregate and the preview advisory only (#6468
    /// aggregate-only disposition); no run row or typed limitation exists.
    AdvisoryOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguageCompletenessRow {
    pub(crate) label: String,
    pub(crate) status: LanguageOutcomeStatus,
    pub(crate) findings: usize,
    pub(crate) limitations: Vec<String>,
    pub(crate) disclosure: LanguageDisclosure,
}

impl LanguageDisclosure {
    /// How a child with this status is disclosed in this mode. A diff-mode
    /// `partial` comes from an adapter that returned `Ok` with kept
    /// limitations: the diff pipeline reports it through the typed
    /// `AnalysisOutcome` limitations and no `language_runs[]` row, while repo
    /// mode records a `partial` run row. (A diff-mode `Err` is never
    /// `partial`; it maps to `failed`.)
    pub(crate) const fn of(mode: OutcomeMode, status: LanguageOutcomeStatus) -> Self {
        match status {
            LanguageOutcomeStatus::Complete
            | LanguageOutcomeStatus::CompleteZero
            | LanguageOutcomeStatus::NoCurrentScope => Self::Silent,
            LanguageOutcomeStatus::Partial => match mode {
                OutcomeMode::Diff => Self::TypedLimitation,
                OutcomeMode::Repo => Self::PublicRun("partial"),
            },
            LanguageOutcomeStatus::Unavailable => Self::PublicRun("unavailable"),
            LanguageOutcomeStatus::Invalid => Self::PublicRun("invalid"),
            LanguageOutcomeStatus::Failed => Self::PublicRun("failed"),
            LanguageOutcomeStatus::Unsupported => Self::TypedLimitation,
            LanguageOutcomeStatus::NotSelected => Self::AdvisoryOnly,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguageCompletenessView {
    pub(crate) mode: OutcomeMode,
    pub(crate) completeness: DiffCompleteness,
    /// One row per canonical language label, in label order.
    pub(crate) rows: Vec<LanguageCompletenessRow>,
    pub(crate) total_findings: usize,
    /// The public `language_runs` compatibility projection of every row, in
    /// label order. Owned here so a surface cannot invent its own.
    pub(crate) public_runs: Vec<LanguageRun>,
}

/// One language row of the `check --json` `language_completeness` member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguageCompletenessProjectionRow {
    pub(crate) language: String,
    pub(crate) status: &'static str,
    pub(crate) findings: usize,
}

/// The additive `check --json` projection of [`LanguageCompletenessView`]
/// (#11272). Derived from the view alone: a consumer can tell a language that
/// ran to `complete_zero` from one that was skipped, not selected or never
/// dispatched (no row), without inferring from absent `language_runs` rows.
/// It restates canonical status strings and counts; it never decides gate
/// policy and cannot strengthen a row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguageCompletenessProjection {
    /// Analysis mode the aggregate binds: `diff` or `repo`.
    pub(crate) mode: &'static str,
    /// Run scope that produced the aggregate: `diff`, `worktree` or `repo`.
    pub(crate) scope: &'static str,
    /// Aggregate completeness: `complete` or `incomplete`.
    pub(crate) completeness: &'static str,
    pub(crate) total_findings: usize,
    /// One row per canonical language label, in label order.
    pub(crate) languages: Vec<LanguageCompletenessProjectionRow>,
}

impl LanguageCompletenessView {
    /// Projects the view for the `check --json` envelope; `scope` is the run
    /// scope label the caller executed (`diff`, `worktree` or `repo`).
    pub(crate) fn project(&self, scope: &'static str) -> LanguageCompletenessProjection {
        LanguageCompletenessProjection {
            mode: self.mode.as_str(),
            scope,
            completeness: self.completeness.as_str(),
            total_findings: self.total_findings,
            languages: self
                .rows
                .iter()
                .map(|row| LanguageCompletenessProjectionRow {
                    language: row.label.clone(),
                    status: row.status.as_str(),
                    findings: row.findings,
                })
                .collect(),
        }
    }

    pub(crate) fn from_aggregate(aggregate: &LanguageAggregate) -> Self {
        let rows: Vec<LanguageCompletenessRow> = aggregate
            .outcomes
            .values()
            .map(|child| LanguageCompletenessRow {
                label: child.language.clone(),
                status: child.status,
                findings: child.finding_count(),
                limitations: child.limitations.clone(),
                disclosure: LanguageDisclosure::of(aggregate.mode, child.status),
            })
            .collect();
        Self {
            mode: aggregate.mode,
            completeness: aggregate.completeness(),
            total_findings: rows.iter().map(|row| row.findings).sum(),
            public_runs: aggregate
                .outcomes
                .values()
                .filter(|child| {
                    matches!(
                        LanguageDisclosure::of(aggregate.mode, child.status),
                        LanguageDisclosure::PublicRun(_)
                    )
                })
                .filter_map(|child| child.compat_run())
                .collect(),
            rows,
        }
    }

    /// Rows a surface must not drop, as `(label, disclosure)`.
    pub(crate) fn disclosed(&self) -> Vec<(&str, LanguageDisclosure)> {
        self.rows
            .iter()
            .map(|row| (row.label.as_str(), row.disclosure))
            .filter(|(_, disclosure)| *disclosure != LanguageDisclosure::Silent)
            .collect()
    }

    /// Checks the public `language_runs` against the view: a row that must be
    /// a public run is present with its status (a degradation is never
    /// hidden), and no run names a language the aggregate does not hold. An
    /// out-of-scope optional adapter failure may keep a public run over a
    /// `no_current_scope` row, so extra runs of known labels are allowed.
    pub(crate) fn reconcile_public_runs(&self, runs: &[LanguageRun]) -> Result<(), String> {
        for (label, disclosure) in self.disclosed() {
            if let LanguageDisclosure::PublicRun(status) = disclosure
                && !runs
                    .iter()
                    .any(|run| run.language == label && run.status.as_str() == status)
            {
                return Err(format!(
                    "language view: {label} is {status} but has no public language_runs row"
                ));
            }
        }
        if let Some(run) = runs
            .iter()
            .find(|run| !self.rows.iter().any(|row| row.label == run.language))
        {
            return Err(format!(
                "language view: public language_runs row {} has no aggregate child",
                run.language
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;

    fn row(label: &str, status: LanguageOutcomeStatus, findings: usize) -> LanguageCompletenessRow {
        LanguageCompletenessRow {
            label: label.to_string(),
            status,
            findings,
            limitations: Vec::new(),
            disclosure: LanguageDisclosure::of(OutcomeMode::Diff, status),
        }
    }

    fn view(rows: Vec<LanguageCompletenessRow>) -> LanguageCompletenessView {
        LanguageCompletenessView {
            mode: OutcomeMode::Diff,
            completeness: DiffCompleteness::Complete,
            total_findings: rows.iter().map(|row| row.findings).sum(),
            rows,
            public_runs: Vec::new(),
        }
    }

    #[test]
    fn projection_restates_every_row_with_canonical_status_and_count() {
        let projection = view(vec![
            row("python", LanguageOutcomeStatus::NoCurrentScope, 0),
            row("rust", LanguageOutcomeStatus::CompleteZero, 0),
            row("typescript", LanguageOutcomeStatus::Complete, 2),
        ])
        .project("worktree");
        assert_eq!(projection.mode, "diff");
        assert_eq!(projection.scope, "worktree");
        assert_eq!(projection.completeness, "complete");
        assert_eq!(projection.total_findings, 2);
        let rows: Vec<(&str, &str, usize)> = projection
            .languages
            .iter()
            .map(|row| (row.language.as_str(), row.status, row.findings))
            .collect();
        assert_eq!(
            rows,
            [
                ("python", "no_current_scope", 0),
                ("rust", "complete_zero", 0),
                ("typescript", "complete", 2),
            ]
        );
    }

    /// Every canonical status spelling (and each mode, scope and completeness
    /// label) must be admitted by the published schema, and the schema must not
    /// admit a spelling the producer cannot emit.
    #[test]
    fn schema_enums_equal_the_canonical_producer_spellings() -> Result<(), String> {
        use LanguageOutcomeStatus as S;
        // Exhaustive on purpose: a new variant fails to compile here until it
        // is listed, then fails the comparison until the schema admits it.
        let all = |status: S| match status {
            S::Complete
            | S::CompleteZero
            | S::Partial
            | S::Unavailable
            | S::Invalid
            | S::Failed
            | S::Unsupported
            | S::NotSelected
            | S::NoCurrentScope => status,
        };
        let statuses: std::collections::BTreeSet<&str> = [
            S::Complete,
            S::CompleteZero,
            S::Partial,
            S::Unavailable,
            S::Invalid,
            S::Failed,
            S::Unsupported,
            S::NotSelected,
            S::NoCurrentScope,
        ]
        .into_iter()
        .map(|status| all(status).as_str())
        .collect();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/ripr/check.schema.json");
        let text = std::fs::read_to_string(&path).map_err(|err| format!("{path:?}: {err}"))?;
        let schema: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let enum_of = |pointer: &str| -> Result<std::collections::BTreeSet<String>, String> {
            schema
                .pointer(pointer)
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("schema has no enum at {pointer}"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| format!("non-string enum member at {pointer}"))
                })
                .collect()
        };
        let base = "/$defs/language_completeness/properties";
        let row = "/$defs/language_completeness_row/properties/status/enum";
        assert_eq!(
            enum_of(row)?,
            statuses.iter().map(|s| s.to_string()).collect(),
            "schema status enum must equal LanguageOutcomeStatus::as_str()"
        );
        let labels = |values: &[&str]| -> std::collections::BTreeSet<String> {
            values.iter().map(|v| v.to_string()).collect()
        };
        assert_eq!(
            enum_of(&format!("{base}/mode/enum"))?,
            labels(&[OutcomeMode::Diff.as_str(), OutcomeMode::Repo.as_str()])
        );
        assert_eq!(
            enum_of(&format!("{base}/completeness/enum"))?,
            labels(&[
                DiffCompleteness::Complete.as_str(),
                DiffCompleteness::Incomplete.as_str()
            ])
        );
        // The scope enum is pinned beside `AnalysisProgressScope::as_str`.
        Ok(())
    }

    #[test]
    fn projection_keeps_silent_rows_that_the_public_run_projection_drops() {
        let view = view(vec![row("rust", LanguageOutcomeStatus::CompleteZero, 0)]);
        assert!(view.public_runs.is_empty());
        assert_eq!(view.project("diff").languages.len(), 1);
    }
}
