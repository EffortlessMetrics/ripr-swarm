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

impl LanguageCompletenessView {
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
