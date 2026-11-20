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
use crate::domain::{ExposureClass, Finding, LanguageStatus, OracleStrength};

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
    /// Set only when the one refusal is
    /// [`VerificationRefusal::UnexposedFindingsRemain`] and every unexposed
    /// finding is an analyzer limit no test edit can close (#14821): the
    /// per-class counts, in [`ExposureClass::ALL`] order, that the
    /// `verify=complete_with_limits` record discloses. `refusals` still names
    /// the refusal, so [`Self::is_complete`] stays strict.
    pub(crate) closable_limits: Option<Vec<(ExposureClass, usize)>>,
}

impl CheckVerification {
    /// Strict completion: every finding reads exposed and nothing refuses.
    pub(crate) fn is_complete(&self) -> bool {
        self.refusals.is_empty()
    }
}

/// Whether `finding` is an analyzer limit a test edit cannot close (#14821).
///
/// Fail-closed on every axis:
///
/// - only the three unknown classes, never `no_static_path` (an uncalled
///   owner is a test-side gap, #14820) and never a `weakly_exposed` /
///   `reachable_unrevealed` finding, even behind a named limit;
/// - only candidate-current, non-preview evidence whose related-test rows
///   were all retained (none packed away past the bounded list);
/// - never when a contributing related test that is not proximity-only
///   asserts only weakly (`weak` / `smoke`): strengthening it is still work
///   the agent can do (#11825);
/// - and only with positive evidence that a test runs the owner and pins
///   behavior exactly: at least one related test whose relation establishes
///   reach ([`crate::domain::RelationReason::establishes_reach`]) and that
///   either is a contributing row of `strong` or `medium` strength here, or
///   is an examined-only row (ripr ran its assertions against this change
///   and matched none) of a test that is a
///   contributing, reaching `strong` / `medium` row on another finding of
///   the same owner function in the run. An examined-only row alone proves
///   nothing: its oracle text is just the test's first assertion, which may
///   be a smoke `is_ok()`. A static limit's reaching test is often
///   examined-only, so the same-owner cross-check is what lets a test that
///   pins that function vouch for it. A limit no test reaches, reached only
///   by tests that assert nothing, or whose assertion ripr refused to credit
///   (`assertion_not_credited`, strength `none`), is not a finished
///   workflow.
fn is_test_unclosable_limit(finding: &Finding, exact_tests: &ExactTests<'_>) -> bool {
    let owner_is_function = finding.probe.owner.is_some();
    let contributing = || {
        finding
            .related_tests
            .iter()
            .filter(|test| test.is_contributing())
    };
    finding.is_candidate_actionable()
        && matches!(
            finding.class,
            ExposureClass::StaticUnknown
                | ExposureClass::InfectionUnknown
                | ExposureClass::PropagationUnknown
        )
        && finding.language_status != Some(LanguageStatus::Preview)
        // Rows packed away by the bounded related-test list could hide a weak
        // assertion the veto below must see (#14821 review).
        && finding.related_tests_total() <= finding.related_tests.len()
        && !contributing().any(|test| {
            !test
                .relation_reason
                .is_some_and(crate::domain::RelationReason::is_proximity_only)
                && matches!(
                    test.oracle_strength,
                    OracleStrength::Weak | OracleStrength::Smoke
                )
        })
        && finding.related_tests.iter().any(|test| {
            reaches(test, owner_is_function)
                && if test.is_contributing() {
                    pins_exactly(test)
                } else {
                    // Row eligibility reads the role, never `miss`
                    // (the role-test control 7): an examined-only row
                    // counts only through a same-owner exact voucher.
                    finding.probe.owner.as_ref().is_some_and(|owner| {
                            exact_tests.contains(&(
                                owner.0.as_str(),
                                test.file.as_path(),
                                test.name.as_str(),
                                test.line,
                            ))
                        })
                }
        })
}

/// (owner, test file, test name, test start line) for each test that is a
/// contributing, reaching `strong` / `medium` row on a non-withheld,
/// candidate-current, non-preview finding of that owner in the run. Keyed by
/// owner so a test that pins one function exactly never vouches for an
/// unasserted call to another, and by the producer's full test identity
/// (name, file, start line) so same-named tests in different inline modules
/// stay distinct (#14821 review).
type ExactTests<'a> = std::collections::BTreeSet<(&'a str, &'a std::path::Path, &'a str, usize)>;

fn reaches(test: &crate::domain::RelatedTest, owner_is_function: bool) -> bool {
    test.relation_reason
        .is_some_and(|reason| reason.establishes_reach(owner_is_function))
}

fn pins_exactly(test: &crate::domain::RelatedTest) -> bool {
    test.is_contributing()
        && matches!(
            test.oracle_strength,
            OracleStrength::Strong | OracleStrength::Medium
        )
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
        let closable_limits = (refusals == [VerificationRefusal::UnexposedFindingsRemain])
            .then(|| self.test_unclosable_limits())
            .flatten();
        CheckVerification {
            outcome,
            findings: self.summary.findings,
            exposed: self.summary.exposed,
            refusals,
            closable_limits,
        }
    }

    /// Per-class counts of the unexposed findings when every one of them is
    /// an analyzer limit no test can close (#14821), else `None`. The counts
    /// must reconcile with the full summary: exposed findings in the list
    /// equal `summary.exposed`, and exposed plus limits equal
    /// `summary.findings`. A withheld (suppressed or opted-out) unexposed
    /// finding never completes, so a policy cannot legitimize a gap.
    fn test_unclosable_limits(&self) -> Option<Vec<(ExposureClass, usize)>> {
        let withheld = self.withheld_finding_ids();
        let exact_tests: ExactTests<'_> = self
            .findings
            .iter()
            .filter(|finding| {
                !withheld.contains(finding.id.as_str())
                    && finding.is_candidate_actionable()
                    && finding.language_status != Some(LanguageStatus::Preview)
            })
            .filter_map(|finding| Some((finding.probe.owner.as_ref()?.0.as_str(), finding)))
            .flat_map(|(owner, finding)| {
                finding
                    .related_tests
                    .iter()
                    .filter(|test| pins_exactly(test) && reaches(test, true))
                    .map(move |test| (owner, test.file.as_path(), test.name.as_str(), test.line))
            })
            .collect();
        let mut exposed = 0usize;
        let mut limits: Vec<(ExposureClass, usize)> = Vec::new();
        for finding in &self.findings {
            let is_withheld = withheld.contains(finding.id.as_str());
            if finding.class == ExposureClass::Exposed {
                if !is_withheld {
                    exposed += 1;
                }
                continue;
            }
            if is_withheld || !is_test_unclosable_limit(finding, &exact_tests) {
                return None;
            }
            match limits.iter_mut().find(|(class, _)| *class == finding.class) {
                Some((_, count)) => *count += 1,
                None => limits.push((finding.class.clone(), 1)),
            }
        }
        let limited: usize = limits.iter().map(|(_, count)| count).sum();
        if limited == 0
            || exposed != self.summary.exposed
            || exposed + limited != self.summary.findings
        {
            return None;
        }
        limits.sort_by_key(|(class, _)| {
            ExposureClass::ALL
                .iter()
                .position(|candidate| candidate == class)
        });
        Some(limits)
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
    use crate::app::tests::{check_output_with, sample_finding};
    use crate::domain::{ExposureClass, Finding, LanguageId, OracleStrength, RelationReason};

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

    /// A complete-with-findings run over `findings`, with the full summary
    /// counts reconciled to the list.
    fn complete_with(findings: Vec<crate::domain::Finding>) -> Result<CheckOutput, String> {
        let mut output = complete_no_findings()?;
        if let Some(outcome) = output.analysis_outcome.as_mut() {
            outcome.kind = AnalysisOutcomeKind::CompleteWithFindings;
        }
        output.summary.findings = findings.len();
        output.summary.exposed = findings
            .iter()
            .filter(|finding| finding.class == ExposureClass::Exposed)
            .count();
        output.findings = findings;
        Ok(output)
    }

    fn finding(id: &str, class: ExposureClass, strength: OracleStrength) -> Finding {
        let mut finding = sample_finding("src/lib.rs", 10);
        finding.id = id.to_string();
        finding.class = class;
        finding.probe.owner = Some(crate::domain::SymbolId("crate::f".to_string()));
        for test in &mut finding.related_tests {
            test.oracle_strength = strength.clone();
            test.relation_reason = Some(RelationReason::DirectOwnerCall);
            test.oracle = Some("assert_eq!(f(1), 2);".to_string());
            test.name = format!("test_{id}");
        }
        finding
    }

    /// The #14821 repro 1 shape: gaps closed, three exposed findings, and
    /// only analyzer limits left, each pinned by a strong owner-bound test.
    fn limits_only() -> Vec<Finding> {
        vec![
            finding("a", ExposureClass::Exposed, OracleStrength::Strong),
            finding("b", ExposureClass::StaticUnknown, OracleStrength::Strong),
            finding("c", ExposureClass::InfectionUnknown, OracleStrength::Strong),
            finding("d", ExposureClass::StaticUnknown, OracleStrength::Strong),
        ]
    }

    /// `limits_only` with finding `b`'s one row examined-only (its
    /// assertion matched nothing in the change) and named `name`.
    fn examined_only(name: &str) -> Vec<Finding> {
        let mut findings = limits_only();
        let row = &mut findings[1].related_tests[0];
        row.role = crate::domain::RelatedTestRole::ExaminedOnly;
        row.oracle_strength = OracleStrength::None;
        row.miss = Some(crate::domain::RelatedTestMiss::AssertionNotObserving);
        row.name = name.to_string();
        findings
    }

    #[test]
    fn analyzer_limits_alone_complete_with_limits_and_stay_disclosed() -> Result<(), String> {
        let verdict = complete_with(limits_only())?.verification();
        assert!(!verdict.is_complete(), "strict completion stays strict");
        assert_eq!(verdict.refusals, vec![UnexposedFindingsRemain]);
        assert_eq!(
            verdict.closable_limits,
            Some(vec![
                (ExposureClass::InfectionUnknown, 1),
                (ExposureClass::StaticUnknown, 2),
            ])
        );

        // The repro 1 static_unknown shape: the only reaching row is
        // examined-only (its assertion matched nothing in this change), but
        // the same test pins another finding of the run exactly.
        assert!(
            complete_with(examined_only("test_c"))?
                .verification()
                .closable_limits
                .is_some()
        );
        // An exposed finding's exact test vouches too.
        assert!(
            complete_with(examined_only("test_a"))?
                .verification()
                .closable_limits
                .is_some()
        );

        // propagation_unknown is a limit like the other two.
        let mut propagation = limits_only();
        propagation[3].class = ExposureClass::PropagationUnknown;
        assert_eq!(
            complete_with(propagation)?.verification().closable_limits,
            Some(vec![
                (ExposureClass::InfectionUnknown, 1),
                (ExposureClass::PropagationUnknown, 1),
                (ExposureClass::StaticUnknown, 1),
            ])
        );

        // A weak test related only by proximity is not bound to the owner.
        let mut nearby = limits_only();
        let mut weak_nearby = nearby[1].related_tests[0].clone();
        weak_nearby.oracle_strength = OracleStrength::Smoke;
        weak_nearby.relation_reason = Some(RelationReason::SameTestFile);
        nearby[1].related_tests.push(weak_nearby);
        assert!(
            complete_with(nearby)?
                .verification()
                .closable_limits
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn any_test_closable_finding_keeps_the_refusal() -> Result<(), String> {
        let refuses = |findings: Vec<Finding>| -> Result<(), String> {
            let verdict = complete_with(findings)?.verification();
            assert_eq!(verdict.refusals, vec![UnexposedFindingsRemain]);
            assert_eq!(verdict.closable_limits, None, "{verdict:?}");
            Ok(())
        };
        // A real gap, an uncalled owner and a weak finding behind a named
        // limit are test-side work.
        for class in [
            ExposureClass::WeaklyExposed,
            ExposureClass::ReachableUnrevealed,
            ExposureClass::NoStaticPath,
        ] {
            let mut findings = limits_only();
            findings.push(finding("gap", class, OracleStrength::Strong));
            refuses(findings)?;
        }
        let mut named_limit = limits_only();
        named_limit[1].class = ExposureClass::WeaklyExposed;
        named_limit[1].static_limit_kind =
            Some(crate::domain::StaticLimitKind::RustTransitiveReachUnresolved);
        refuses(named_limit)?;

        // A smoke-only (#14820) or weak owner-bound related test can still be
        // strengthened, even when one strong test also relates.
        for strength in [OracleStrength::Smoke, OracleStrength::Weak] {
            let mut smoke = limits_only();
            smoke[2].related_tests[0].oracle_strength = strength.clone();
            refuses(smoke)?;
            let mut mixed = limits_only();
            let mut weak = mixed[2].related_tests[0].clone();
            weak.oracle_strength = strength;
            mixed[2].related_tests.push(weak);
            refuses(mixed)?;
            // A weak row with no recorded relation counts as bound.
            let mut unknown_relation = limits_only();
            let mut unrelated = unknown_relation[3].related_tests[0].clone();
            unrelated.oracle_strength = OracleStrength::Weak;
            unrelated.relation_reason = None;
            unknown_relation[3].related_tests.push(unrelated);
            refuses(unknown_relation)?;
        }

        // No test reaches the limit, only a proximity or name relation does,
        // or the reaching test asserts nothing: no positive evidence.
        let mut bare = limits_only();
        bare[1].related_tests.clear();
        refuses(bare)?;
        for reason in [
            RelationReason::SameTestFile,
            RelationReason::OwnerNamedTest,
            RelationReason::AssertionTargetAffinity,
        ] {
            let mut unbound = limits_only();
            unbound[1].related_tests[0].relation_reason = Some(reason);
            refuses(unbound)?;
        }
        let mut no_relation = limits_only();
        no_relation[1].related_tests[0].relation_reason = None;
        refuses(no_relation)?;
        // An examined-only row of a test that pins nothing exactly anywhere
        // in the run: its oracle text is only the test's first assertion,
        // which may be a smoke `assert!(r.is_ok())` (#14821 review).
        let mut examined_alone = limits_only();
        {
            let row = &mut examined_alone[1].related_tests[0];
            row.role = crate::domain::RelatedTestRole::ExaminedOnly;
            row.oracle_strength = OracleStrength::None;
            row.miss = Some(crate::domain::RelatedTestMiss::AssertionNotObserving);
            row.oracle = Some("assert!(r.is_ok());".to_string());
        }
        refuses(examined_alone)?;
        // The vouching test must reach its own finding's owner.
        let mut far = limits_only();
        {
            let row = &mut far[1].related_tests[0];
            row.role = crate::domain::RelatedTestRole::ExaminedOnly;
            row.oracle_strength = OracleStrength::None;
            row.miss = Some(crate::domain::RelatedTestMiss::AssertionNotObserving);
            row.name = "test_c".to_string();
        }
        far[2].related_tests[0].relation_reason = Some(RelationReason::OwnerNamedTest);
        refuses(far)?;
        // A test that pins another function exactly never vouches for an
        // unasserted call to this one (#14821 second review).
        let mut other_owner = examined_only("test_c");
        other_owner[2].probe.owner = Some(crate::domain::SymbolId("crate::g".to_string()));
        refuses(other_owner)?;
        // The vouching test must be the same test: a same-named test at
        // another start line (another inline module) is a different test.
        let mut namesake = examined_only("test_c");
        namesake[1].related_tests[0].line += 100;
        refuses(namesake)?;
        // A base-side or preview finding cannot vouch for a current limit:
        // the exposed finding `a` is the only voucher for `test_a`.
        let mut stale_voucher = examined_only("test_a");
        stale_voucher[0].source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        refuses(stale_voucher)?;
        let mut preview_voucher = examined_only("test_a");
        preview_voucher[0].language_status = Some(crate::domain::LanguageStatus::Preview);
        refuses(preview_voucher)?;
        // Related-test rows packed away past the bounded list might hide a
        // weak assertion: no completion.
        let mut packed = limits_only();
        packed[1].related_tests_matched_total = Some(9);
        refuses(packed)?;
        let mut all_kept = limits_only();
        all_kept[1].related_tests_matched_total = Some(1);
        assert!(
            complete_with(all_kept)?
                .verification()
                .closable_limits
                .is_some()
        );
        // A limit with no owner has nothing to bind a vouching test to.
        let mut ownerless = examined_only("test_c");
        ownerless[1].probe.owner = None;
        refuses(ownerless)?;
        // Contributing rows that pin nothing: an unmatched or refused
        // assertion (`none` strength) is not exact evidence.
        for miss in [
            crate::domain::RelatedTestMiss::AssertionNotObserving,
            crate::domain::RelatedTestMiss::AssertionNotCredited,
        ] {
            let mut unpinned = limits_only();
            unpinned[1].related_tests[0].oracle_strength = OracleStrength::None;
            unpinned[1].related_tests[0].miss = Some(miss);
            refuses(unpinned)?;
        }
        let mut calls_only = limits_only();
        calls_only[1].related_tests[0].oracle_strength = OracleStrength::None;
        calls_only[1].related_tests[0].oracle = Some(String::new());
        refuses(calls_only)?;
        let mut no_assertion = limits_only();
        no_assertion[1].related_tests[0].oracle_strength = OracleStrength::None;
        no_assertion[1].related_tests[0].miss = Some(crate::domain::RelatedTestMiss::NoAssertion);
        refuses(no_assertion)?;
        // An examined-only weak row is outside the decision set: no veto.
        let mut examined_weak = limits_only();
        let mut examined = examined_weak[1].related_tests[0].clone();
        examined.role = crate::domain::RelatedTestRole::ExaminedOnly;
        examined.oracle_strength = OracleStrength::Smoke;
        examined_weak[1].related_tests.push(examined);
        assert!(
            complete_with(examined_weak)?
                .verification()
                .closable_limits
                .is_some()
        );

        // Preview-language and base-side evidence never completes.
        let mut preview = limits_only();
        preview[1].language_status = Some(crate::domain::LanguageStatus::Preview);
        refuses(preview)?;
        let mut stale = limits_only();
        stale[1].source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        refuses(stale)?;

        // A withheld unexposed finding: a policy cannot legitimize a gap.
        let mut withheld = complete_with(limits_only())?;
        withheld.owner_opt_outs = vec![crate::analysis::owner_opt_out::OwnerOptOut {
            finding_id: "b".to_string(),
            owner: "f".to_string(),
            reason: crate::analysis::owner_opt_out::OwnerOptOutReason::MutantsSkip,
            scope: crate::analysis::owner_opt_out::OwnerOptOutScope::Fn,
            scope_label: String::new(),
        }];
        assert_eq!(withheld.verification().closable_limits, None);

        // Counts that do not reconcile with the full summary refuse.
        let mut drift = complete_with(limits_only())?;
        drift.summary.findings += 1;
        assert_eq!(drift.verification().closable_limits, None);
        Ok(())
    }

    #[test]
    fn limits_never_complete_beside_another_refusal() -> Result<(), String> {
        let mut stale = complete_with(limits_only())?;
        stale.unanalyzed_working_tree = true;
        assert_eq!(stale.verification().closable_limits, None);
        let mut partial = complete_with(limits_only())?;
        if let Some(outcome) = partial.analysis_outcome.as_mut() {
            outcome.kind = AnalysisOutcomeKind::AnalysisFailed;
        }
        assert_eq!(partial.verification().closable_limits, None);
        let mut missing = complete_with(limits_only())?;
        missing.analysis_outcome = None;
        assert_eq!(missing.verification().closable_limits, None);
        let mut unscoped = complete_with(limits_only())?;
        unscoped.no_scope_provided = true;
        assert_eq!(unscoped.verification().closable_limits, None);
        // All exposed is strict completion, not completion with limits.
        let exposed = complete_with(vec![finding(
            "a",
            ExposureClass::Exposed,
            OracleStrength::Strong,
        )])?
        .verification();
        assert!(exposed.is_complete());
        assert_eq!(exposed.closable_limits, None);
        Ok(())
    }
}
