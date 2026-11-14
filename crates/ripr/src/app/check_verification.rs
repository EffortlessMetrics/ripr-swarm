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
    /// Full summary findings exceed full summary exposed findings. Named for
    /// what it counts, not "gaps": a limited finding (`static_unknown`, ...)
    /// is not a test-side gap in the agent header, and the record must not
    /// contradict a `gaps=0` header (#13825).
    UnexposedFindingsRemain,
    /// A committed-history run left uncommitted source/test edits unanalyzed.
    UnanalyzedWorkingTree,
    /// Untracked source/test files in a language enabled for this run were
    /// excluded from the diff. Gitignored files never refuse: the repository
    /// excludes them on purpose and the human/GitHub/JSON notes keep naming
    /// them.
    UntrackedSourceExcluded,
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
            Self::UnexposedFindingsRemain => "unexposed_findings_remain",
            Self::UnanalyzedWorkingTree => "unanalyzed_working_tree",
            Self::UntrackedSourceExcluded => "untracked_source_excluded",
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
    /// scope, equal full findings/exposed counts, and no unanalyzed working
    /// tree, untracked enabled-language source, partial or no-scope
    /// condition. Anything else refuses with every reason named; empty
    /// findings alone never establish completion. Gitignored source is
    /// disclosed by the notes but never refuses.
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
            refusals.push(VerificationRefusal::UnexposedFindingsRemain);
        }
        if self.unanalyzed_working_tree {
            refusals.push(VerificationRefusal::UnanalyzedWorkingTree);
        }
        if self.untracked_enabled_language_source_present() {
            refusals.push(VerificationRefusal::UntrackedSourceExcluded);
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

    /// Whether an untracked routed path belongs to a language this run
    /// enabled. An agent's new file stays untracked until `git add -N`, so it
    /// is exactly the edit a working-tree diff would silently miss; a stray
    /// untracked file in a disabled language would not be analyzed even if
    /// staged, so it does not block completion. The enabled set is the one
    /// the producer recorded for this run
    /// (`AnalysisOutcome::check_findings_inputs.languages`); when it is
    /// unknown, every untracked routed path refuses (fail closed).
    fn untracked_enabled_language_source_present(&self) -> bool {
        let enabled = self
            .analysis_outcome
            .as_ref()
            .and_then(|outcome| outcome.check_findings_inputs.as_ref())
            .map(|inputs| inputs.languages.as_slice());
        self.untracked_working_tree_source_paths
            .iter()
            .any(|path| match enabled {
                None => true,
                Some(enabled) => crate::analysis::language::route(std::path::Path::new(path))
                    .is_some_and(|language| enabled.contains(&language)),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::VerificationRefusal::{
        AnalysisIncomplete, NoBehaviorInScope, NoScope, OutcomeMissing, UnanalyzedWorkingTree,
        UnexposedFindingsRemain, UntrackedSourceExcluded,
    };
    use crate::analysis_outcome::{
        AnalysisIdentity, AnalysisOutcome, AnalysisOutcomeCounts, AnalysisOutcomeKind,
        CheckFindingsInputs,
    };
    use crate::app::CheckOutput;
    use crate::app::tests::check_output_with;
    use crate::domain::LanguageId;

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

    fn with_enabled_languages(
        mut output: CheckOutput,
        languages: Vec<LanguageId>,
    ) -> Result<CheckOutput, String> {
        let outcome = output
            .analysis_outcome
            .as_mut()
            .ok_or("fixture must carry an analysis outcome")?;
        outcome.check_findings_inputs = Some(CheckFindingsInputs {
            config_identity: String::new(),
            include_unchanged_tests: false,
            languages,
            perl_facts_used: false,
        });
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

        let mut stale = complete_no_findings()?;
        stale.unanalyzed_working_tree = true;
        assert_eq!(stale.verification().refusals, vec![UnanalyzedWorkingTree]);

        let mut gaps = complete_no_findings()?;
        gaps.summary.findings = 2;
        gaps.summary.exposed = 1;
        assert_eq!(gaps.verification().refusals, vec![UnexposedFindingsRemain]);
        Ok(())
    }

    #[test]
    fn gitignored_source_is_disclosed_but_never_refuses() -> Result<(), String> {
        // The repository excludes these on purpose (`node_modules/`, `.venv/`,
        // generated output, nested worktrees); they are almost never the
        // agent's edit, and the human/GitHub/JSON notes keep naming them.
        let mut ignored = with_enabled_languages(
            complete_no_findings()?,
            vec![LanguageId::Rust, LanguageId::TypeScript, LanguageId::Python],
        )?;
        ignored.ignored_working_tree_source_paths = vec![
            "src/gen.rs".to_string(),
            "node_modules/pkg/index.js".to_string(),
            ".venv/lib/site.py".to_string(),
        ];
        let verdict = ignored.verification();
        assert!(verdict.is_complete(), "{verdict:?}");

        // Without a recorded enabled set, gitignored source still never refuses.
        let mut unknown = complete_no_findings()?;
        unknown.ignored_working_tree_source_paths = vec!["src/gen.rs".to_string()];
        let verdict = unknown.verification();
        assert!(verdict.is_complete(), "{verdict:?}");
        Ok(())
    }

    #[test]
    fn untracked_source_refuses_only_in_a_language_enabled_for_the_run() -> Result<(), String> {
        let rust_only = || with_enabled_languages(complete_no_findings()?, vec![LanguageId::Rust]);

        // An untracked file in an enabled language is the agent's unstaged
        // new file: completion refuses.
        let mut enabled = rust_only()?;
        enabled.untracked_working_tree_source_paths =
            vec!["scratch.py".to_string(), "src/new.rs".to_string()];
        assert_eq!(
            enabled.verification().refusals,
            vec![UntrackedSourceExcluded]
        );

        // Untracked files only in disabled languages would not be analyzed
        // even if staged: they do not block completion.
        let mut disabled = rust_only()?;
        disabled.untracked_working_tree_source_paths = vec![
            "scratch.py".to_string(),
            "tools/build.mjs".to_string(),
            "script.pl".to_string(),
        ];
        let verdict = disabled.verification();
        assert!(verdict.is_complete(), "{verdict:?}");

        // The same Python file refuses once Python is enabled.
        let mut python = with_enabled_languages(
            complete_no_findings()?,
            vec![LanguageId::Rust, LanguageId::Python],
        )?;
        python.untracked_working_tree_source_paths = vec!["scratch.py".to_string()];
        assert_eq!(
            python.verification().refusals,
            vec![UntrackedSourceExcluded]
        );

        // Without a recorded enabled set the verdict fails closed.
        let mut unknown = complete_no_findings()?;
        unknown.untracked_working_tree_source_paths = vec!["scratch.py".to_string()];
        assert_eq!(
            unknown.verification().refusals,
            vec![UntrackedSourceExcluded]
        );
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
            vec![UnanalyzedWorkingTree, UntrackedSourceExcluded, NoScope]
        );
        Ok(())
    }
}
