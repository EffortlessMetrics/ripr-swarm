//! Deterministic mixed-language diff aggregation (#6468, parent #5546).
//!
//! The aggregate holds one `LanguageAnalysisOutcomeV1` per canonical language
//! label and derives whole-diff completeness from the children alone, so no
//! child is ever strengthened and a sibling's evidence is never dropped.
//! Adapter execution order cannot change it: children live in a `BTreeMap` and
//! the digest hashes the sorted child digests.
//!
//! It is an internal record. It does not feed `AnalysisOutcome.kind`, the
//! public `language_runs` projection, or any gate.

use super::{
    LANGUAGE_ANALYSIS_OUTCOME_SCHEMA_VERSION, LanguageAnalysisOutcomeV1, LanguageOutcomeStatus,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Whole-diff completeness. `Incomplete` is the fail-closed default: it also
/// covers an aggregate with no child at all, which evidenced nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffCompleteness {
    Complete,
    Incomplete,
}

impl DiffCompleteness {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffLanguageAggregate {
    /// Diff scope label every child must have run over.
    pub(crate) scope: String,
    /// Children keyed by canonical language label (`javascript` is its own
    /// key, never folded into `typescript`).
    pub(crate) outcomes: BTreeMap<String, LanguageAnalysisOutcomeV1>,
    /// The completeness this aggregate asserts; `validate` rejects a claim the
    /// children do not support.
    pub(crate) claimed: DiffCompleteness,
    pub(crate) semantic_digest: String,
}

impl DiffLanguageAggregate {
    pub(crate) fn new(scope: String) -> Self {
        let mut aggregate = Self {
            scope,
            outcomes: BTreeMap::new(),
            claimed: DiffCompleteness::Incomplete,
            semantic_digest: String::new(),
        };
        aggregate.semantic_digest = aggregate.compute_digest();
        aggregate
    }

    /// Records one child. A repeated label keeps the child that blocks
    /// completeness, then the greater digest, so the result does not depend on
    /// the order the duplicates arrived in.
    pub(crate) fn record(&mut self, outcome: LanguageAnalysisOutcomeV1) {
        let rank = |candidate: &LanguageAnalysisOutcomeV1| {
            (
                !candidate.status.is_complete_for_aggregate(),
                candidate.semantic_digest.clone(),
            )
        };
        match self.outcomes.get(&outcome.language) {
            Some(existing) if rank(existing) >= rank(&outcome) => {}
            _ => {
                self.outcomes.insert(outcome.language.clone(), outcome);
            }
        }
    }

    /// Seals the aggregate: asserts the completeness derived from the children
    /// and binds the digest.
    pub(crate) fn finish(mut self) -> Self {
        self.claimed = self.completeness();
        self.semantic_digest = self.compute_digest();
        self
    }

    /// Languages whose status keeps the aggregate from claiming whole-diff
    /// completeness, as `(label, status)` in label order.
    pub(crate) fn blockers(&self) -> Vec<(&str, &'static str)> {
        self.outcomes
            .iter()
            .filter(|(_, outcome)| !outcome.status.is_complete_for_aggregate())
            .map(|(label, outcome)| (label.as_str(), outcome.status.as_str()))
            .collect()
    }

    /// Pure completeness: complete only when at least one child is evidenced
    /// (`complete` or `complete_zero`) and every child is complete, evidenced
    /// `complete_zero`, or had nothing in scope. Nothing-in-scope children
    /// alone evidence nothing. Unsupported, not-selected, unavailable,
    /// invalid, failed and partial children all keep it incomplete.
    pub(crate) fn completeness(&self) -> DiffCompleteness {
        let evidenced = self.outcomes.values().any(|outcome| {
            matches!(
                outcome.status,
                LanguageOutcomeStatus::Complete | LanguageOutcomeStatus::CompleteZero
            )
        });
        if !evidenced || !self.blockers().is_empty() {
            DiffCompleteness::Incomplete
        } else {
            DiffCompleteness::Complete
        }
    }

    /// Rejects a self-contradictory aggregate: a child that contradicts its
    /// own status, a child filed under another label or scope, a completeness
    /// claim the children do not support, or a stale digest.
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (label, outcome) in &self.outcomes {
            if label != &outcome.language {
                return Err(format!(
                    "diff language aggregate: child {} filed under {label}",
                    outcome.language
                ));
            }
            if let Some(contradiction) = outcome.contradiction() {
                return Err(format!("diff language aggregate: {contradiction}"));
            }
            if outcome.scope.as_deref() != Some(self.scope.as_str()) {
                return Err(format!(
                    "diff language aggregate: {label} ran over scope {:?}, aggregate over {}",
                    outcome.scope, self.scope
                ));
            }
        }
        if self.claimed == DiffCompleteness::Complete
            && self.completeness() != DiffCompleteness::Complete
        {
            let blockers = self
                .blockers()
                .iter()
                .map(|(label, status)| format!("{label}={status}"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "diff language aggregate claims complete but has non-complete children: {blockers}"
            ));
        }
        if self.semantic_digest != self.compute_digest() {
            return Err("diff language aggregate: digest does not match its children".to_string());
        }
        Ok(())
    }

    /// Digest over the bound scope, claimed completeness and the sorted child
    /// digests. Each child digest already binds its own label, status,
    /// counts, limitations and scope, so adapter order cannot matter.
    fn compute_digest(&self) -> String {
        let mut children: Vec<&str> = self
            .outcomes
            .values()
            .map(|outcome| outcome.semantic_digest.as_str())
            .collect();
        children.sort_unstable();
        let mut canonical = format!(
            "v{}|diff-aggregate|s:{}|{}",
            LANGUAGE_ANALYSIS_OUTCOME_SCHEMA_VERSION,
            self.scope,
            self.claimed.as_str()
        );
        for digest in children {
            let _ = write!(canonical, "|c:{digest}");
        }
        format!("{:x}", Sha256::digest(canonical.as_bytes()))
    }
}
