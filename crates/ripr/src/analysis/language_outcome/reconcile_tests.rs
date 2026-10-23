//! Hostile hand-built inputs for the cross-layer reconcile (#6473). Each
//! invariant has a test that builds a contradicting input and sees it
//! rejected, because the pipeline call sites are debug-only.

use super::*;
use crate::analysis::language_outcome::{DiffCompleteness, LanguageAggregate};
use crate::analysis_outcome::{
    AnalysisIdentity, AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcome,
    AnalysisOutcomeCounts, AnalysisOutcomeKind, AnalysisRecovery, AnalysisRecoveryKind,
    AnalysisStage,
};
use std::collections::BTreeSet;

fn finding_in(language: LanguageId) -> Result<Finding, String> {
    let mut finding = crate::mcp::gaps::test_finding()?;
    finding.language = Some(language);
    Ok(finding)
}

fn repo_child(
    language: LanguageId,
    findings: Vec<Finding>,
    production_files: usize,
) -> LanguageAnalysisOutcomeV1 {
    let result = LanguageRepoResult {
        findings,
        production_files,
        ..LanguageRepoResult::default()
    };
    LanguageAnalysisOutcomeV1::from_repo_result(language, &result, None)
}

fn repo_aggregate(children: Vec<LanguageAnalysisOutcomeV1>) -> LanguageAggregate {
    let mut aggregate = LanguageAggregate::new_repo();
    for child in children {
        aggregate.record(child);
    }
    aggregate.finish()
}

fn diff_children(
    language: LanguageId,
    findings: Vec<Finding>,
    touched: &[&str],
) -> Vec<LanguageAnalysisOutcomeV1> {
    let result = LanguageDiffResult {
        findings,
        changed_files: 1,
        ..LanguageDiffResult::default()
    };
    let touched: BTreeSet<String> = touched.iter().map(|label| label.to_string()).collect();
    LanguageAnalysisOutcomeV1::from_diff_result(language, &result, &[], &touched, "full")
}

fn diff_aggregate(children: Vec<LanguageAnalysisOutcomeV1>) -> LanguageAggregate {
    let mut aggregate = LanguageAggregate::new("full".to_string());
    for child in children {
        aggregate.record(child);
    }
    aggregate.finish()
}

fn limitation(kind: AnalysisLimitationKind) -> Result<AnalysisLimitation, String> {
    Ok(AnalysisLimitation::new(
        kind,
        AnalysisStage::LanguageAdapter,
        AnalysisRecovery::new(AnalysisRecoveryKind::Retry, "Re-run the analysis.")?,
    ))
}

fn outcome(
    kind: AnalysisOutcomeKind,
    finding_count: u64,
    limitations: Vec<AnalysisLimitation>,
) -> Result<AnalysisOutcome, String> {
    AnalysisOutcome::new(
        kind,
        AnalysisIdentity::default(),
        AnalysisOutcomeCounts {
            changed_file_count: 1,
            changed_line_count: 1,
            candidate_line_count: 1,
            probe_count: finding_count,
            finding_count,
        },
        limitations,
    )
}

fn rejected(result: Result<(), String>, needle: &str) -> Result<(), String> {
    let error = result.err().ok_or("contradiction was accepted")?;
    assert!(error.contains(needle), "{error}");
    Ok(())
}

#[test]
fn clean_nonempty_and_valid_empty_repos_reconcile() -> Result<(), String> {
    let findings = vec![finding_in(LanguageId::Rust)?];
    let clean = repo_aggregate(vec![
        repo_child(LanguageId::Rust, findings.clone(), 3),
        repo_child(LanguageId::Python, Vec::new(), 0),
    ]);
    assert_eq!(
        reconcile(
            &clean,
            &[LanguageId::Rust, LanguageId::Python],
            &findings,
            None
        ),
        Ok(())
    );
    // A valid empty project: nothing discovered is nothing in scope, never
    // complete-zero, and it still reconciles.
    let empty = repo_aggregate(vec![repo_child(LanguageId::Rust, Vec::new(), 0)]);
    assert_eq!(empty.completeness(), DiffCompleteness::Incomplete);
    assert_eq!(reconcile(&empty, &[LanguageId::Rust], &[], None), Ok(()));
    Ok(())
}

#[test]
fn complete_zero_without_discovery_proof_is_rejected() -> Result<(), String> {
    for production in [None, Some(0)] {
        let mut lying = repo_child(LanguageId::Rust, Vec::new(), 3);
        assert_eq!(lying.status, LanguageOutcomeStatus::CompleteZero);
        lying.resources.production_files = production;
        let forged = repo_aggregate(vec![lying]);
        rejected(forged.validate(), "without discovery proof")?;
        rejected(
            reconcile(&forged, &[LanguageId::Rust], &[], None),
            "without discovery proof",
        )?;
    }
    // The converse: no_current_scope over discovered files is not nothing.
    let mut hiding = repo_child(LanguageId::Rust, Vec::new(), 0);
    assert_eq!(hiding.status, LanguageOutcomeStatus::NoCurrentScope);
    hiding.resources.production_files = Some(4);
    rejected(repo_aggregate(vec![hiding]).validate(), "no_current_scope")
}

#[test]
fn findings_cannot_coexist_with_complete_zero() -> Result<(), String> {
    let mut lying = repo_child(LanguageId::Rust, Vec::new(), 3);
    lying.findings_by_language.insert("rust".to_string(), 1);
    let forged = repo_aggregate(vec![lying]);
    rejected(
        reconcile(
            &forged,
            &[LanguageId::Rust],
            &[finding_in(LanguageId::Rust)?],
            None,
        ),
        "complete_zero carries 1 finding",
    )
}

#[test]
fn a_partial_child_blocks_stronger_aggregate_completeness() -> Result<(), String> {
    let findings = vec![finding_in(LanguageId::Rust)?];
    let partial = LanguageRepoResult {
        partial_reason: Some("capped".to_string()),
        production_files: 3,
        ..LanguageRepoResult::default()
    };
    let aggregate = repo_aggregate(vec![
        repo_child(LanguageId::Rust, findings.clone(), 3),
        LanguageAnalysisOutcomeV1::from_repo_result(LanguageId::Python, &partial, None),
    ]);
    assert_eq!(aggregate.completeness(), DiffCompleteness::Incomplete);
    let mut forged = aggregate.clone();
    forged.claimed = DiffCompleteness::Complete;
    rejected(forged.validate(), "python=partial")
}

#[test]
fn a_complete_outcome_kind_cannot_sit_over_a_hard_blocker() -> Result<(), String> {
    let findings = vec![finding_in(LanguageId::Rust)?];
    let blocked = LanguageAnalysisOutcomeV1::from_diff_err(
        LanguageId::Python,
        "boom".to_string(),
        LanguageRunStatus::Failed,
        "full",
    );
    let aggregate = diff_aggregate(vec![
        diff_children(LanguageId::Rust, findings.clone(), &["rust"]).remove(0),
        blocked,
    ]);
    let selected = [LanguageId::Rust, LanguageId::Python];
    let lying = outcome(AnalysisOutcomeKind::CompleteWithFindings, 1, Vec::new())?;
    rejected(
        reconcile(&aggregate, &selected, &findings, Some(&lying)),
        "python is failed",
    )?;
    // With the typed limitation the same aggregate reconciles.
    let honest = outcome(
        AnalysisOutcomeKind::PartialWithLimitations,
        1,
        vec![limitation(AnalysisLimitationKind::ProducerFailure)?],
    )?;
    assert_eq!(
        reconcile(&aggregate, &selected, &findings, Some(&honest)),
        Ok(())
    );
    Ok(())
}

#[test]
fn unsupported_changed_source_needs_a_top_level_limitation() -> Result<(), String> {
    let findings = vec![finding_in(LanguageId::Rust)?];
    let aggregate = diff_aggregate(vec![
        diff_children(LanguageId::Rust, findings.clone(), &["rust"]).remove(0),
        LanguageAnalysisOutcomeV1::unsupported("go", 1, "full"),
    ]);
    let selected = [LanguageId::Rust];
    // Partial-with-another-limitation is not the unsupported disclosure.
    let wrong = outcome(
        AnalysisOutcomeKind::PartialWithLimitations,
        1,
        vec![limitation(AnalysisLimitationKind::ProducerFailure)?],
    )?;
    rejected(
        reconcile(&aggregate, &selected, &findings, Some(&wrong)),
        "no top-level limitation",
    )?;
    let honest = outcome(
        AnalysisOutcomeKind::PartialWithLimitations,
        1,
        vec![limitation(
            AnalysisLimitationKind::LanguageScopeUnsupported,
        )?],
    )?;
    assert_eq!(
        reconcile(&aggregate, &selected, &findings, Some(&honest)),
        Ok(())
    );
    Ok(())
}

#[test]
fn aggregate_counts_must_equal_the_union_of_children_and_findings() -> Result<(), String> {
    let one = vec![finding_in(LanguageId::Rust)?];
    let aggregate = repo_aggregate(vec![repo_child(LanguageId::Rust, one.clone(), 3)]);
    // Retained findings dropped or invented.
    rejected(
        reconcile(&aggregate, &[LanguageId::Rust], &[], None),
        "children carry 1 finding",
    )?;
    let two = vec![finding_in(LanguageId::Rust)?, finding_in(LanguageId::Rust)?];
    rejected(
        reconcile(&aggregate, &[LanguageId::Rust], &two, None),
        "retained findings are 2",
    )?;
    // The top-level outcome count disagrees with the same findings.
    let diff = diff_aggregate(diff_children(LanguageId::Rust, one.clone(), &["rust"]));
    let stale_count = outcome(AnalysisOutcomeKind::CompleteWithFindings, 2, Vec::new())?;
    rejected(
        reconcile(&diff, &[LanguageId::Rust], &one, Some(&stale_count)),
        "outcome counts 2",
    )?;
    let agreeing = outcome(AnalysisOutcomeKind::CompleteWithFindings, 1, Vec::new())?;
    assert_eq!(
        reconcile(&diff, &[LanguageId::Rust], &one, Some(&agreeing)),
        Ok(())
    );
    Ok(())
}

#[test]
fn an_omitted_selected_language_cannot_hide_behind_an_empty_run_vector() -> Result<(), String> {
    let findings = vec![finding_in(LanguageId::Rust)?];
    let rust_only = repo_aggregate(vec![repo_child(LanguageId::Rust, findings.clone(), 3)]);
    assert_eq!(rust_only.validate(), Ok(()));
    rejected(
        reconcile(
            &rust_only,
            &[LanguageId::Rust, LanguageId::Python],
            &findings,
            None,
        ),
        "selected language python has no child",
    )?;
    let diff = diff_aggregate(diff_children(LanguageId::Rust, findings.clone(), &["rust"]));
    rejected(
        reconcile(
            &diff,
            &[LanguageId::Rust, LanguageId::Perl],
            &findings,
            None,
        ),
        "selected language perl has no child",
    )
}

#[test]
fn conflicting_duplicate_children_fail_closed() -> Result<(), String> {
    let good = repo_child(LanguageId::Python, Vec::new(), 3);
    let bad = LanguageAnalysisOutcomeV1::from_repo_err(
        LanguageId::Python,
        "boom".to_string(),
        LanguageRunStatus::Failed,
    );
    for order in [vec![good.clone(), bad.clone()], vec![bad, good.clone()]] {
        let aggregate = repo_aggregate(order);
        // The blocker is kept, and the disagreement is now visible.
        assert_eq!(aggregate.completeness(), DiffCompleteness::Incomplete);
        assert!(aggregate.conflicts.contains("python"));
        rejected(aggregate.validate(), "python recorded twice")?;
    }
    // Two blocking rows (an adapter that errors beside its own not-selected
    // disposition) are the pipeline's real shape and not a conflict.
    let two_blockers = repo_aggregate(vec![
        LanguageAnalysisOutcomeV1::from_repo_err(
            LanguageId::Perl,
            "no packet".to_string(),
            LanguageRunStatus::Unavailable,
        ),
        LanguageAnalysisOutcomeV1::repo_not_selected("perl", 1, "adapter not compiled"),
    ]);
    assert!(two_blockers.conflicts.is_empty());
    assert_eq!(two_blockers.validate(), Ok(()));
    // The identical row twice is not a conflict.
    let twice = repo_aggregate(vec![good.clone(), good]);
    assert!(twice.conflicts.is_empty());
    assert_eq!(twice.validate(), Ok(()));
    Ok(())
}

/// The TypeScript adapter serves both ids. With a bridge-profile Rust-line
/// finding the two loop runs genuinely produce different `typescript`
/// children, so the family labels are exempt from the conflict rule.
#[test]
fn typescript_family_duplicates_can_differ_and_are_not_a_conflict() -> Result<(), String> {
    let plain = vec![finding_in(LanguageId::TypeScript)?];
    let touched = ["typescript", "javascript"];
    let digest_of = |language: LanguageId, findings: Vec<Finding>, label: &str| {
        diff_children(language, findings, &touched)
            .into_iter()
            .find(|child| child.language == label)
            .map(|child| child.semantic_digest)
    };
    // In-family findings: the duplicate `typescript` child is identical.
    assert_eq!(
        digest_of(LanguageId::TypeScript, plain.clone(), "typescript"),
        digest_of(LanguageId::JavaScript, plain, "typescript")
    );
    // A Rust-line finding stays with the loop language's child: they differ.
    let bridged = vec![finding_in(LanguageId::Rust)?];
    assert_ne!(
        digest_of(LanguageId::TypeScript, bridged.clone(), "typescript"),
        digest_of(LanguageId::JavaScript, bridged.clone(), "typescript")
    );
    let mut both = LanguageAggregate::new("full".to_string());
    for child in diff_children(LanguageId::TypeScript, bridged.clone(), &touched)
        .into_iter()
        .chain(diff_children(LanguageId::JavaScript, bridged, &touched))
    {
        both.record(child);
    }
    let both = both.finish();
    assert_eq!(both.validate(), Ok(()));
    Ok(())
}

#[test]
fn both_typescript_ids_may_retain_more_findings_than_children_but_never_fewer() -> Result<(), String>
{
    let findings = vec![
        finding_in(LanguageId::TypeScript)?,
        finding_in(LanguageId::TypeScript)?,
    ];
    let aggregate = diff_aggregate(diff_children(
        LanguageId::TypeScript,
        findings[..1].to_vec(),
        &["typescript"],
    ));
    let both = [LanguageId::TypeScript, LanguageId::JavaScript];
    // Retained duplicates from the second loop run are tolerated...
    let javascript = diff_children(LanguageId::JavaScript, Vec::new(), &["javascript"]);
    let mut with_js = aggregate.clone();
    for child in javascript {
        with_js.record(child);
    }
    let with_js = with_js.finish();
    assert_eq!(reconcile(&with_js, &both, &findings, None), Ok(()));
    // ...but a retained list shorter than the children is still a contradiction.
    rejected(
        reconcile(&with_js, &both, &[], None),
        "children carry 1 finding",
    )?;
    // And with one id selected the rule is exact.
    rejected(
        reconcile(&aggregate, &[LanguageId::TypeScript], &findings, None),
        "retained findings are 2",
    )
}

#[test]
fn reordered_children_reconcile_identically() -> Result<(), String> {
    let findings = vec![finding_in(LanguageId::Rust)?];
    let children = || {
        vec![
            repo_child(LanguageId::Rust, findings.clone(), 3),
            repo_child(LanguageId::Python, Vec::new(), 0),
            repo_child(LanguageId::Perl, Vec::new(), 2),
        ]
    };
    let forward = repo_aggregate(children());
    let reversed = repo_aggregate(children().into_iter().rev().collect());
    let selected = [LanguageId::Rust, LanguageId::Python, LanguageId::Perl];
    assert_eq!(forward, reversed);
    assert_eq!(
        reconcile(&forward, &selected, &findings, None),
        reconcile(&reversed, &selected, &findings, None)
    );
    Ok(())
}
