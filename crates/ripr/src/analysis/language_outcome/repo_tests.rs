//! Repo-mode outcomes and the shared language aggregate (#6470).

use super::*;
use crate::analysis::language_outcome::{DiffCompleteness, LanguageAggregate};

fn finding_in(language: LanguageId) -> Result<Finding, String> {
    let mut finding = crate::mcp::gaps::test_finding()?;
    finding.language = Some(language);
    Ok(finding)
}

fn repo_ok(language: LanguageId, findings: Vec<Finding>) -> LanguageAnalysisOutcomeV1 {
    let result = LanguageRepoResult {
        findings,
        production_files: 3,
        ..LanguageRepoResult::default()
    };
    LanguageAnalysisOutcomeV1::from_repo_result(language, &result, None)
}

fn aggregate(children: Vec<LanguageAnalysisOutcomeV1>) -> LanguageAggregate {
    let mut aggregate = LanguageAggregate::new_repo();
    for child in children {
        aggregate.record(child);
    }
    aggregate.finish()
}

#[test]
fn no_discovered_production_file_is_no_current_scope_not_complete_zero() {
    let empty = LanguageRepoResult::default();
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(LanguageId::Python, &empty, None);
    assert_eq!(outcome.status, LanguageOutcomeStatus::NoCurrentScope);
    assert!(outcome.compat_run().is_none());
    // Nothing in scope evidences nothing: the aggregate is not complete.
    assert_eq!(
        aggregate(vec![outcome]).completeness(),
        DiffCompleteness::Incomplete
    );
}

#[test]
fn discovered_production_files_without_findings_stay_complete_zero() {
    let outcome = repo_ok(LanguageId::Python, Vec::new());
    assert_eq!(outcome.status, LanguageOutcomeStatus::CompleteZero);
    assert_eq!(
        aggregate(vec![outcome]).completeness(),
        DiffCompleteness::Complete
    );
}

#[test]
fn partial_reason_wins_over_the_zero_discovery_rule() {
    let partial = LanguageRepoResult {
        partial_reason: Some("capped".to_string()),
        ..LanguageRepoResult::default()
    };
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(LanguageId::Python, &partial, None);
    assert_eq!(outcome.status, LanguageOutcomeStatus::Partial);
    assert!(outcome.compat_run().is_some());
}

#[test]
fn zero_finding_run_over_a_non_establishing_cell_reports_the_cell_limit() {
    let outcome = repo_ok(LanguageId::Perl, Vec::new());
    assert_eq!(outcome.status, LanguageOutcomeStatus::Unsupported);
    assert_eq!(outcome.limitations.len(), 1);
    assert!(outcome.limitations[0].contains("lang-perl"));
    assert!(outcome.compat_run().is_none());
    assert!(outcome.contradiction().is_none());
}

#[test]
fn non_establishing_cell_with_findings_keeps_status_but_blocks_the_aggregate() -> Result<(), String>
{
    let perl = repo_ok(LanguageId::Perl, vec![finding_in(LanguageId::Perl)?]);
    // The child and its public projection are exactly as before.
    assert_eq!(perl.status, LanguageOutcomeStatus::Complete);
    assert!(perl.compat_run().is_none());

    let rust = repo_ok(LanguageId::Rust, vec![finding_in(LanguageId::Rust)?]);
    let sealed = aggregate(vec![rust, perl]);
    assert_eq!(sealed.completeness(), DiffCompleteness::Incomplete);
    let blockers = sealed.blockers();
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].0, "perl");
    assert!(blockers[0].1.contains("lang-perl"), "{blockers:?}");

    // An aggregate that claims complete over it is rejected.
    let mut forged = sealed.clone();
    forged.claimed = DiffCompleteness::Complete;
    assert!(forged.validate().is_err());
    assert_eq!(sealed.validate(), Ok(()));
    Ok(())
}

#[test]
fn establishing_cells_do_not_block() -> Result<(), String> {
    let sealed = aggregate(vec![
        repo_ok(LanguageId::Rust, vec![finding_in(LanguageId::Rust)?]),
        repo_ok(LanguageId::Python, Vec::new()),
    ]);
    assert!(sealed.blockers().is_empty(), "{:?}", sealed.blockers());
    assert_eq!(sealed.completeness(), DiffCompleteness::Complete);
    assert_eq!(sealed.validate(), Ok(()));
    Ok(())
}

#[test]
fn a_failed_language_keeps_sibling_evidence() -> Result<(), String> {
    let rust = repo_ok(LanguageId::Rust, vec![finding_in(LanguageId::Rust)?]);
    let failed = LanguageAnalysisOutcomeV1::from_repo_err(
        LanguageId::Python,
        "parse failure".to_string(),
        LanguageRunStatus::Failed,
    );
    let compat = failed.compat_run();
    assert_eq!(
        compat,
        Some(LanguageRun {
            language: "python".to_string(),
            status: LanguageRunStatus::Failed,
            reason: Some("parse failure".to_string()),
        })
    );
    let sealed = aggregate(vec![failed, rust.clone()]);
    assert_eq!(sealed.outcomes.get("rust"), Some(&rust));
    assert_eq!(sealed.blockers(), vec![("python", "failed")]);
    assert_eq!(sealed.completeness(), DiffCompleteness::Incomplete);
    Ok(())
}

#[test]
fn language_order_does_not_change_the_aggregate_identity() -> Result<(), String> {
    let children = || -> Result<Vec<LanguageAnalysisOutcomeV1>, String> {
        Ok(vec![
            repo_ok(LanguageId::Rust, vec![finding_in(LanguageId::Rust)?]),
            repo_ok(LanguageId::Python, Vec::new()),
            LanguageAnalysisOutcomeV1::repo_not_selected("perl", 2, "not selected"),
        ])
    };
    let forward = aggregate(children()?);
    let mut reversed = children()?;
    reversed.reverse();
    assert_eq!(forward.semantic_digest, aggregate(reversed).semantic_digest);
    Ok(())
}

#[test]
fn a_diff_child_cannot_sit_in_a_repo_aggregate() {
    let mut sealed = LanguageAggregate::new_repo();
    sealed.record(LanguageAnalysisOutcomeV1::unsupported("go", 1, "full"));
    assert!(sealed.finish().validate().is_err());
}
