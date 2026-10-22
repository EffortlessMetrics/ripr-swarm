//! Diff-mode outcomes and mixed-language aggregation (#6468).

use super::diff_aggregate::DiffCompleteness;
use super::*;

fn finding_in(language: Option<LanguageId>) -> Result<Finding, String> {
    let mut finding = crate::mcp::gaps::test_finding()?;
    finding.language = language;
    Ok(finding)
}

fn diff_result(findings: Vec<Finding>, changed_files: usize) -> LanguageDiffResult {
    LanguageDiffResult {
        findings,
        changed_files,
        ..LanguageDiffResult::default()
    }
}

fn diff_ok(
    language: LanguageId,
    findings: Vec<Finding>,
    kept: &[&str],
) -> LanguageAnalysisOutcomeV1 {
    LanguageAnalysisOutcomeV1::from_diff_result(
        language,
        &diff_result(findings, 1),
        &kept.iter().map(|k| k.to_string()).collect::<Vec<_>>(),
        true,
        "full",
    )
}

fn aggregate(children: Vec<LanguageAnalysisOutcomeV1>) -> DiffLanguageAggregate {
    let mut aggregate = DiffLanguageAggregate::new("full".to_string());
    for child in children {
        aggregate.record(child);
    }
    aggregate.finish()
}

fn rust_complete() -> Result<LanguageAnalysisOutcomeV1, String> {
    Ok(diff_ok(
        LanguageId::Rust,
        vec![finding_in(Some(LanguageId::Rust))?],
        &[],
    ))
}

#[test]
fn repo_digest_spelling_is_unchanged() {
    let repo = LanguageRepoResult {
        partial_reason: Some("b".to_string()),
        ..LanguageRepoResult::default()
    };
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::Python,
        &repo,
        Some("a".to_string()),
    );
    let expected = format!("{:x}", Sha256::digest(b"v1|python|repo|partial|l:a|l:b"));
    assert_eq!(outcome.semantic_digest, expected);
}

#[test]
fn rust_complete_with_python_partial_keeps_rust_and_is_incomplete() -> Result<(), String> {
    let python = diff_ok(LanguageId::Python, Vec::new(), &["parse_budget_exceeded"]);
    assert_eq!(python.status, LanguageOutcomeStatus::Partial);
    let aggregate = aggregate(vec![rust_complete()?, python]);
    assert_eq!(aggregate.validate(), Ok(()));
    assert_eq!(aggregate.completeness(), DiffCompleteness::Incomplete);
    assert_eq!(aggregate.blockers(), vec![("python", "partial")]);
    let rust = aggregate.outcomes.get("rust").ok_or("rust child missing")?;
    assert_eq!(rust.status, LanguageOutcomeStatus::Complete);
    assert_eq!(rust.finding_count(), 1);
    Ok(())
}

#[test]
fn rust_complete_with_perl_invalid_names_perl_and_projects_invalid() -> Result<(), String> {
    let perl = LanguageAnalysisOutcomeV1::from_diff_err(
        LanguageId::Perl,
        "ingestion: fingerprint mismatch".to_string(),
        LanguageRunStatus::Invalid,
        "full",
    );
    assert_eq!(
        perl.compat_run(),
        Some(LanguageRun {
            language: "perl".to_string(),
            status: LanguageRunStatus::Invalid,
            reason: Some("ingestion: fingerprint mismatch".to_string()),
        })
    );
    let aggregate = aggregate(vec![rust_complete()?, perl]);
    assert_eq!(aggregate.blockers(), vec![("perl", "invalid")]);
    assert_eq!(aggregate.completeness(), DiffCompleteness::Incomplete);
    assert_eq!(
        aggregate.outcomes.get("rust").map(|c| c.finding_count()),
        Some(1)
    );
    Ok(())
}

#[test]
fn rust_with_unsupported_go_cannot_be_complete() -> Result<(), String> {
    let only_rust = aggregate(vec![rust_complete()?]);
    assert_eq!(only_rust.completeness(), DiffCompleteness::Complete);
    let with_go = aggregate(vec![
        rust_complete()?,
        LanguageAnalysisOutcomeV1::unsupported("go", 2, "full"),
    ]);
    assert_eq!(with_go.blockers(), vec![("go", "unsupported")]);
    assert_eq!(with_go.completeness(), DiffCompleteness::Incomplete);
    assert_eq!(with_go.validate(), Ok(()));
    Ok(())
}

#[test]
fn javascript_only_changes_stay_javascript() -> Result<(), String> {
    let javascript = diff_ok(
        LanguageId::JavaScript,
        vec![finding_in(Some(LanguageId::JavaScript))?],
        &[],
    );
    let aggregate = aggregate(vec![javascript]);
    assert_eq!(
        aggregate.outcomes.keys().collect::<Vec<_>>(),
        vec!["javascript"]
    );
    let child = aggregate
        .outcomes
        .get("javascript")
        .ok_or("javascript child missing")?;
    assert_eq!(child.findings_by_language.get("javascript"), Some(&1));
    assert_eq!(child.findings_by_language.get("typescript"), None);
    // A TypeScript-adapter run that carries JavaScript findings keeps them
    // counted as JavaScript under the adapter's own label.
    let ts_run = diff_ok(
        LanguageId::TypeScript,
        vec![finding_in(Some(LanguageId::JavaScript))?],
        &[],
    );
    assert_eq!(ts_run.language, "typescript");
    assert_eq!(ts_run.findings_by_language.get("javascript"), Some(&1));
    Ok(())
}

#[test]
fn unavailable_not_selected_unsupported_no_scope_and_complete_zero_stay_distinct() {
    let unavailable = LanguageAnalysisOutcomeV1::from_diff_err(
        LanguageId::Python,
        "no packet".to_string(),
        LanguageRunStatus::Unavailable,
        "full",
    );
    let not_selected = LanguageAnalysisOutcomeV1::not_selected("python", 1, "disabled", "full");
    let unsupported = LanguageAnalysisOutcomeV1::unsupported("python", 1, "full");
    let no_scope = LanguageAnalysisOutcomeV1::from_diff_result(
        LanguageId::Python,
        &diff_result(Vec::new(), 0),
        &[],
        false,
        "full",
    );
    let complete_zero = LanguageAnalysisOutcomeV1::from_diff_result(
        LanguageId::Python,
        &diff_result(Vec::new(), 1),
        &[],
        true,
        "full",
    );
    let all = [
        &unavailable,
        &not_selected,
        &unsupported,
        &no_scope,
        &complete_zero,
    ];
    assert_eq!(
        all.map(|outcome| outcome.status),
        [
            LanguageOutcomeStatus::Unavailable,
            LanguageOutcomeStatus::NotSelected,
            LanguageOutcomeStatus::Unsupported,
            LanguageOutcomeStatus::NoCurrentScope,
            LanguageOutcomeStatus::CompleteZero,
        ]
    );
    let digests = all.map(|outcome| outcome.semantic_digest.clone());
    let unique = digests.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), digests.len());
    // Only an adapter that could not run projects into `language_runs`.
    assert!(unavailable.compat_run().is_some());
    assert!(not_selected.compat_run().is_none());
    assert!(unsupported.compat_run().is_none());
    assert!(no_scope.compat_run().is_none());
    assert!(complete_zero.compat_run().is_none());
    // Nothing in scope does not block completeness; the others do.
    assert!(no_scope.status.is_complete_for_aggregate());
    assert!(!not_selected.status.is_complete_for_aggregate());
    assert!(!unsupported.status.is_complete_for_aggregate());
    assert!(!unavailable.status.is_complete_for_aggregate());
}

#[test]
fn adapter_order_permutations_give_identical_aggregates() -> Result<(), String> {
    let children = || -> Result<Vec<LanguageAnalysisOutcomeV1>, String> {
        Ok(vec![
            rust_complete()?,
            diff_ok(LanguageId::Python, Vec::new(), &["capped"]),
            LanguageAnalysisOutcomeV1::unsupported("go", 1, "full"),
            LanguageAnalysisOutcomeV1::from_diff_err(
                LanguageId::Perl,
                "ingestion: x".to_string(),
                LanguageRunStatus::Invalid,
                "full",
            ),
        ])
    };
    let forward = aggregate(children()?);
    let mut reversed = children()?;
    reversed.reverse();
    let reversed = aggregate(reversed);
    let mut rotated = children()?;
    rotated.rotate_left(2);
    let rotated = aggregate(rotated);
    assert_eq!(forward, reversed);
    assert_eq!(forward.semantic_digest, reversed.semantic_digest);
    assert_eq!(forward.semantic_digest, rotated.semantic_digest);
    // Removing a child changes the digest, so the digest is not constant.
    let mut fewer = children()?;
    fewer.pop();
    assert_ne!(forward.semantic_digest, aggregate(fewer).semantic_digest);
    Ok(())
}

#[test]
fn duplicate_labels_resolve_independently_of_arrival_order() -> Result<(), String> {
    let good = rust_complete()?;
    let bad = LanguageAnalysisOutcomeV1::from_diff_err(
        LanguageId::Rust,
        "boom".to_string(),
        LanguageRunStatus::Failed,
        "full",
    );
    let one = aggregate(vec![good.clone(), bad.clone()]);
    let two = aggregate(vec![bad, good]);
    assert_eq!(one, two);
    assert_eq!(one.completeness(), DiffCompleteness::Incomplete);
    Ok(())
}

#[test]
fn validator_rejects_complete_claim_over_a_non_complete_child() -> Result<(), String> {
    let mut forged = aggregate(vec![
        rust_complete()?,
        LanguageAnalysisOutcomeV1::unsupported("go", 1, "full"),
    ]);
    assert_eq!(forged.claimed, DiffCompleteness::Incomplete);
    assert_eq!(forged.validate(), Ok(()));
    forged.claimed = DiffCompleteness::Complete;
    let error = forged.validate().err().ok_or("forged claim accepted")?;
    assert!(error.contains("go=unsupported"), "{error}");
    Ok(())
}

#[test]
fn validator_rejects_contradictory_children_scope_and_stale_digest() -> Result<(), String> {
    // A complete_zero child that carries findings.
    let mut lying = diff_ok(LanguageId::Python, Vec::new(), &[]);
    lying.findings_by_language.insert("python".to_string(), 1);
    assert!(aggregate(vec![lying]).validate().is_err());
    // An unsupported child that carries findings.
    let mut ghost = LanguageAnalysisOutcomeV1::unsupported("go", 1, "full");
    ghost.findings_by_language.insert("go".to_string(), 1);
    assert!(aggregate(vec![ghost]).validate().is_err());
    // A boundary-complete child that still carries a limitation.
    let mut limited = rust_complete()?;
    limited.limitations.push("capped".to_string());
    assert!(aggregate(vec![limited]).validate().is_err());
    // A child that ran over a different scope than the aggregate.
    let other_scope = LanguageAnalysisOutcomeV1::unsupported("go", 1, "partial:sha256:x");
    assert!(aggregate(vec![other_scope]).validate().is_err());
    // A digest that no longer matches the children.
    let mut stale = aggregate(vec![rust_complete()?]);
    stale.semantic_digest = "0".repeat(64);
    assert!(stale.validate().is_err());
    Ok(())
}

#[test]
fn empty_aggregate_and_scope_binding_fail_closed() -> Result<(), String> {
    let empty = aggregate(Vec::new());
    assert_eq!(empty.completeness(), DiffCompleteness::Incomplete);
    let full = aggregate(vec![rust_complete()?]);
    let mut partial_scope = DiffLanguageAggregate::new("partial:sha256:x".to_string());
    partial_scope.record(LanguageAnalysisOutcomeV1::from_diff_result(
        LanguageId::Rust,
        &diff_result(vec![finding_in(Some(LanguageId::Rust))?], 1),
        &[],
        true,
        "partial:sha256:x",
    ));
    let partial_scope = partial_scope.finish();
    assert_eq!(partial_scope.validate(), Ok(()));
    assert_ne!(full.semantic_digest, partial_scope.semantic_digest);
    Ok(())
}

#[test]
fn err_compat_run_is_the_adapter_reason_verbatim() {
    for status in [
        LanguageRunStatus::Unavailable,
        LanguageRunStatus::Invalid,
        LanguageRunStatus::Failed,
    ] {
        let outcome = LanguageAnalysisOutcomeV1::from_diff_err(
            LanguageId::Perl,
            "a | b".to_string(),
            status,
            "full",
        );
        assert_eq!(
            outcome.compat_run(),
            Some(LanguageRun {
                language: "perl".to_string(),
                status,
                reason: Some("a | b".to_string()),
            })
        );
    }
}

#[test]
fn generated_skips_make_a_diff_run_partial() {
    let mut run = diff_result(Vec::new(), 2);
    run.skipped_files = 1;
    let outcome =
        LanguageAnalysisOutcomeV1::from_diff_result(LanguageId::Rust, &run, &[], true, "full");
    assert_eq!(outcome.status, LanguageOutcomeStatus::Partial);
    assert_eq!(outcome.limitations, vec!["generated_files_skipped: 1"]);
}
