//! The post-edit completion verdict for a check run (#13516).
//!
//! One owner for whether a [`CheckOutput`] establishes static completion:
//! renderers (the Agent `verify=` record) only format this typed result. It is
//! a pure projection of producer-owned facts: the closed analysis outcome, the
//! full summary counts (never a displayed or triaged subset) and the
//! working-tree, partial-scope and no-scope disclosures. It adds no classifier
//! decision of its own, and it is static evidence only.

use super::CheckOutput;
use crate::analysis_outcome::AnalysisOutcomeKind;

/// Why a check run does not establish completion, in record order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VerificationRefusal {
    /// The run carries no producer analysis outcome.
    OutcomeMissing,
    /// The outcome is complete but had no behavior in scope.
    NoBehaviorInScope,
    /// The outcome is partial, unsupported or failed.
    AnalysisIncomplete,
    /// Full summary findings exceed full summary exposed findings.
    GapsRemain,
    /// A committed-history run left uncommitted source/test edits unanalyzed.
    UnanalyzedWorkingTree,
    /// Untracked routed source/test files were excluded from the diff.
    UntrackedSourceExcluded,
    /// Gitignored routed source/test files were excluded from the diff.
    IgnoredSourceExcluded,
    /// The run analyzed a bounded partition of the diff.
    PartialScope,
    /// The caller provided no analysis scope.
    NoScope,
}

impl VerificationRefusal {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::OutcomeMissing => "outcome_missing",
            Self::NoBehaviorInScope => "no_behavior_in_scope",
            Self::AnalysisIncomplete => "analysis_incomplete",
            Self::GapsRemain => "gaps_remain",
            Self::UnanalyzedWorkingTree => "unanalyzed_working_tree",
            Self::UntrackedSourceExcluded => "untracked_source_excluded",
            Self::IgnoredSourceExcluded => "ignored_source_excluded",
            Self::PartialScope => "partial_scope",
            Self::NoScope => "no_scope",
        }
    }
}

/// The typed completion verdict and the facts it was decided from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckVerification {
    pub(crate) outcome: Option<AnalysisOutcomeKind>,
    pub(crate) findings: usize,
    pub(crate) exposed: usize,
    /// Every unmet condition, in [`VerificationRefusal`] order. Empty means
    /// complete.
    pub(crate) refusals: Vec<VerificationRefusal>,
}

impl CheckVerification {
    pub(crate) fn is_complete(&self) -> bool {
        self.refusals.is_empty()
    }
}

impl CheckOutput {
    /// Complete requires a present closed `complete_*` outcome with behavior in
    /// scope, equal full findings/exposed counts, and no unanalyzed,
    /// untracked or gitignored excluded source, partial or no-scope
    /// condition. Anything else refuses with every reason named; empty
    /// findings alone never establish completion.
    pub(crate) fn verification(&self) -> CheckVerification {
        use AnalysisOutcomeKind::{CompleteNoFindings, CompleteWithFindings};
        let outcome = self.analysis_outcome.as_ref().map(|outcome| outcome.kind);
        let mut refusals = Vec::new();
        match outcome {
            None => refusals.push(VerificationRefusal::OutcomeMissing),
            Some(CompleteNoFindings | CompleteWithFindings) => {}
            Some(kind) if kind.is_complete() => {
                refusals.push(VerificationRefusal::NoBehaviorInScope);
            }
            Some(_) => refusals.push(VerificationRefusal::AnalysisIncomplete),
        }
        if self.summary.findings != self.summary.exposed {
            refusals.push(VerificationRefusal::GapsRemain);
        }
        if self.unanalyzed_working_tree {
            refusals.push(VerificationRefusal::UnanalyzedWorkingTree);
        }
        if !self.untracked_working_tree_source_paths.is_empty() {
            refusals.push(VerificationRefusal::UntrackedSourceExcluded);
        }
        if !self.ignored_working_tree_source_paths.is_empty() {
            refusals.push(VerificationRefusal::IgnoredSourceExcluded);
        }
        if self.partial_scope.is_some() {
            refusals.push(VerificationRefusal::PartialScope);
        }
        if self.no_scope_provided {
            refusals.push(VerificationRefusal::NoScope);
        }
        CheckVerification {
            outcome,
            findings: self.summary.findings,
            exposed: self.summary.exposed,
            refusals,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VerificationRefusal::{
        AnalysisIncomplete, GapsRemain, IgnoredSourceExcluded, NoBehaviorInScope, NoScope,
        OutcomeMissing, UnanalyzedWorkingTree, UntrackedSourceExcluded,
    };
    use crate::analysis_outcome::{
        AnalysisIdentity, AnalysisOutcome, AnalysisOutcomeCounts, AnalysisOutcomeKind,
    };
    use crate::app::CheckOutput;
    use crate::app::tests::check_output_with;

    fn complete_no_findings() -> Result<CheckOutput, String> {
        let mut output = check_output_with(Vec::new());
        output.analysis_outcome = Some(AnalysisOutcome::new(
            AnalysisOutcomeKind::CompleteNoFindings,
            AnalysisIdentity::default(),
            AnalysisOutcomeCounts {
                changed_file_count: 1,
                changed_line_count: 1,
                candidate_line_count: 1,
                probe_count: 0,
                finding_count: 0,
            },
            Vec::new(),
        )?);
        Ok(output)
    }

    #[test]
    fn a_clean_complete_run_verifies_and_each_disclosure_refuses_alone() -> Result<(), String> {
        let clean = complete_no_findings()?.verification();
        assert!(clean.is_complete(), "{clean:?}");
        assert_eq!(clean.outcome, Some(AnalysisOutcomeKind::CompleteNoFindings));

        let mut untracked = complete_no_findings()?;
        untracked.untracked_working_tree_source_paths = vec!["src/new.rs".to_string()];
        assert_eq!(
            untracked.verification().refusals,
            vec![UntrackedSourceExcluded]
        );

        let mut ignored = complete_no_findings()?;
        ignored.ignored_working_tree_source_paths = vec!["src/gen.rs".to_string()];
        assert_eq!(ignored.verification().refusals, vec![IgnoredSourceExcluded]);

        let mut stale = complete_no_findings()?;
        stale.unanalyzed_working_tree = true;
        assert_eq!(stale.verification().refusals, vec![UnanalyzedWorkingTree]);

        let mut gaps = complete_no_findings()?;
        gaps.summary.findings = 2;
        gaps.summary.exposed = 1;
        assert_eq!(gaps.verification().refusals, vec![GapsRemain]);
        Ok(())
    }

    #[test]
    fn outcome_kinds_refuse_by_kind_and_every_reason_is_named_in_order() -> Result<(), String> {
        assert_eq!(
            check_output_with(Vec::new()).verification().refusals,
            vec![OutcomeMissing]
        );
        let mut empty = complete_no_findings()?;
        if let Some(outcome) = empty.analysis_outcome.as_mut() {
            outcome.kind = AnalysisOutcomeKind::NoChangedLines;
        }
        assert_eq!(empty.verification().refusals, vec![NoBehaviorInScope]);
        let mut failed = complete_no_findings()?;
        if let Some(outcome) = failed.analysis_outcome.as_mut() {
            outcome.kind = AnalysisOutcomeKind::AnalysisFailed;
        }
        assert_eq!(failed.verification().refusals, vec![AnalysisIncomplete]);

        let mut all = complete_no_findings()?;
        all.unanalyzed_working_tree = true;
        all.untracked_working_tree_source_paths = vec!["src/new.rs".to_string()];
        all.ignored_working_tree_source_paths = vec!["src/gen.rs".to_string()];
        all.no_scope_provided = true;
        let verdict = all.verification();
        assert!(!verdict.is_complete());
        assert_eq!(
            verdict.refusals,
            vec![
                UnanalyzedWorkingTree,
                UntrackedSourceExcluded,
                IgnoredSourceExcluded,
                NoScope
            ]
        );
        Ok(())
    }
}
