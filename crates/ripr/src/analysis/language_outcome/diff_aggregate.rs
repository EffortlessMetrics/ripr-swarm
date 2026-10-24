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
    OutcomeMode,
};
use crate::domain::LanguageId;
use crate::domain::language_capability::{
    AnalysisMode, CAPABILITY_CELLS, CapabilityFamily, FRAMEWORK_ANY, state_establishes_capability,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
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

/// The shared mixed-language aggregate shape (#6468 diff, #6470 repo).
pub(crate) type DiffLanguageAggregate = LanguageAggregate;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguageAggregate {
    pub(crate) mode: OutcomeMode,
    /// Diff scope label every child must have run over; `None` in repo mode.
    pub(crate) scope: Option<String>,
    /// Children keyed by canonical language label (`javascript` is its own
    /// key, never folded into `typescript`).
    pub(crate) outcomes: BTreeMap<String, LanguageAnalysisOutcomeV1>,
    /// Labels recorded more than once where one row would let the aggregate
    /// claim completeness and another blocks it. The kept child is still the
    /// deterministic blocker-first one; this only makes the disagreement
    /// visible to `validate` (#6473). Two blocking rows (a configured adapter
    /// that errors beside its own not-selected advisory) are not a conflict.
    /// Not part of the digest.
    pub(crate) conflicts: BTreeSet<String>,
    /// The completeness this aggregate asserts; `validate` rejects a claim the
    /// children do not support.
    pub(crate) claimed: DiffCompleteness,
    pub(crate) semantic_digest: String,
}

impl LanguageAggregate {
    pub(crate) fn new(scope: String) -> Self {
        Self::with_scope(OutcomeMode::Diff, Some(scope))
    }

    /// A repo-mode aggregate: no diff scope, children bound to repo mode.
    pub(crate) fn new_repo() -> Self {
        Self::with_scope(OutcomeMode::Repo, None)
    }

    fn with_scope(mode: OutcomeMode, scope: Option<String>) -> Self {
        let mut aggregate = Self {
            mode,
            scope,
            outcomes: BTreeMap::new(),
            conflicts: BTreeSet::new(),
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
        if let Some(existing) = self.outcomes.get(&outcome.language)
            && existing.status.is_complete_for_aggregate()
                != outcome.status.is_complete_for_aggregate()
        {
            self.conflicts.insert(outcome.language.clone());
        }
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
    ///
    /// In repo mode a child that claims `complete`/`complete_zero` over a
    /// language whose exact repo `static_facts` capability cell does not
    /// establish capability is also a blocker, named with the cell's limit
    /// text. The child keeps its own status, so the public compatibility
    /// projection is unchanged (#6470).
    pub(crate) fn blockers(&self) -> Vec<(&str, &'static str)> {
        self.outcomes
            .iter()
            .filter_map(|(label, outcome)| {
                if !outcome.status.is_complete_for_aggregate() {
                    Some((label.as_str(), outcome.status.as_str()))
                } else {
                    self.capability_blocker(outcome)
                        .map(|limit| (label.as_str(), limit))
                }
            })
            .collect()
    }

    /// The limit text of a non-establishing repo capability cell under an
    /// evidenced (`complete`/`complete_zero`) child, else `None`. A language
    /// with no cell is `not_proven`.
    fn capability_blocker(&self, outcome: &LanguageAnalysisOutcomeV1) -> Option<&'static str> {
        if self.mode != OutcomeMode::Repo
            || !matches!(
                outcome.status,
                LanguageOutcomeStatus::Complete | LanguageOutcomeStatus::CompleteZero
            )
        {
            return None;
        }
        let language = LanguageId::from_wire(&outcome.language)?;
        repo_static_facts_limit(language)
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
            if outcome.mode != self.mode {
                return Err(format!(
                    "diff language aggregate: {label} ran in {:?} mode, aggregate in {:?}",
                    outcome.mode, self.mode
                ));
            }
            if self.mode == OutcomeMode::Repo {
                // Complete-zero needs positive discovery proof (#6473): a
                // zero-finding result with no discovered production file is
                // nothing in scope, and a missing count is not a proof.
                let production = outcome.resources.production_files;
                if outcome.status == LanguageOutcomeStatus::CompleteZero
                    && production.is_none_or(|files| files == 0)
                {
                    return Err(format!(
                        "diff language aggregate: {label} claims complete_zero without discovery proof (production_files {production:?})"
                    ));
                }
                if outcome.status == LanguageOutcomeStatus::NoCurrentScope && production != Some(0)
                {
                    return Err(format!(
                        "diff language aggregate: {label} claims no_current_scope but production_files is {production:?}"
                    ));
                }
            }
            if outcome.scope != self.scope {
                return Err(format!(
                    "diff language aggregate: {label} ran over scope {:?}, aggregate over {:?}",
                    outcome.scope, self.scope
                ));
            }
        }
        // The TypeScript adapter serves both ids, so its two loop runs may
        // legitimately disagree on the family labels (a bridge profile's
        // Rust-line findings stay with the loop language's child). Any other
        // label recorded twice with different content is a conflict (#6473).
        if let Some(label) = self
            .conflicts
            .iter()
            .find(|label| !matches!(label.as_str(), "typescript" | "javascript"))
        {
            return Err(format!(
                "diff language aggregate: {label} recorded twice with conflicting outcomes"
            ));
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

    /// Rejects an aggregate that omits a selected language: every language
    /// the run selected must have recorded a child, so an empty or short run
    /// vector cannot hide it (#6473).
    pub(crate) fn validate_selected(&self, selected: &[LanguageId]) -> Result<(), String> {
        for language in selected {
            if !self.outcomes.contains_key(language.as_str()) {
                return Err(format!(
                    "diff language aggregate: selected language {} has no child",
                    language.as_str()
                ));
            }
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
        let mut canonical = match &self.scope {
            Some(scope) => format!(
                "v{}|diff-aggregate|s:{}|{}",
                LANGUAGE_ANALYSIS_OUTCOME_SCHEMA_VERSION,
                scope,
                self.claimed.as_str()
            ),
            None => format!(
                "v{}|repo-aggregate|{}",
                LANGUAGE_ANALYSIS_OUTCOME_SCHEMA_VERSION,
                self.claimed.as_str()
            ),
        };
        for digest in children {
            let _ = write!(canonical, "|c:{digest}");
        }
        format!("{:x}", Sha256::digest(canonical.as_bytes()))
    }
}

/// `None` when the language's exact repo `static_facts` cell establishes
/// capability; otherwise the cell's limit text, or the state spelling when the
/// cell names no limit. A missing cell is `not_proven`.
pub(crate) fn repo_static_facts_limit(language: LanguageId) -> Option<&'static str> {
    let cell = CAPABILITY_CELLS.iter().find(|cell| {
        cell.language == language
            && cell.framework == FRAMEWORK_ANY
            && cell.mode == AnalysisMode::Repo
            && cell.family == CapabilityFamily::StaticFacts
    });
    match cell {
        Some(cell) if state_establishes_capability(cell.state) => None,
        Some(cell) => Some(cell.limit.unwrap_or_else(|| cell.state.as_str())),
        None => Some("not_proven"),
    }
}
