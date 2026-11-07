//! Additive controls for the extracted owner; facade tests stay in place.
use super::*;
use crate::domain::{
    ActivationEvidence, Confidence, DeltaKind, ExposureClass, Probe, ProbeFamily, ProbeId,
    RevealEvidence, RiprEvidence, SourceCurrentness, SourceLocation, StageEvidence, StageState,
};
use std::collections::BTreeSet;

fn finding(language: Option<LanguageId>) -> Finding {
    let stage = || StageEvidence::new(StageState::Unknown, Confidence::Unknown, "fixture");
    Finding {
        id: "finding:outcome".to_string(),
        canonical_gap: None,
        probe: Probe {
            id: ProbeId("probe:outcome".to_string()),
            location: SourceLocation::new("src/lib.rs", 1, 1),
            owner: None,
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: None,
            after: None,
            expression: "value > 0".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        },
        class: ExposureClass::WeaklyExposed,
        ripr: RiprEvidence {
            reach: stage(),
            infect: stage(),
            propagate: stage(),
            reveal: RevealEvidence {
                observe: stage(),
                discriminate: stage(),
            },
        },
        confidence: 0.5,
        confidence_basis: Vec::new(),
        evidence: Vec::new(),
        missing: Vec::new(),
        flow_sinks: Vec::new(),
        activation: ActivationEvidence::default(),
        stop_reasons: Vec::new(),
        related_tests_matched_total: None,
        related_tests: Vec::new(),
        recommended_next_step: None,
        language,
        language_status: None,
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: None,
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness: SourceCurrentness::CandidateCurrent,
        analysis_scope: crate::domain::AnalysisScope::Diff,
    }
}

fn repo_child(
    language: LanguageId,
    findings: &[Finding],
    production_files: usize,
) -> LanguageAnalysisOutcomeV1 {
    LanguageAnalysisOutcomeV1::from_repo_result(
        language,
        RepoOutcomeInput {
            findings,
            production_files,
            skipped_files: 0,
            partial_reason: None,
            file_accounting: RepoFileAccounting::default(),
        },
        None,
    )
}

fn diff_children(
    language: LanguageId,
    findings: &[Finding],
    touched: &[&str],
) -> Vec<LanguageAnalysisOutcomeV1> {
    LanguageAnalysisOutcomeV1::from_diff_result(
        language,
        DiffOutcomeInput {
            findings,
            skipped_files: 0,
            partial_run_status: None,
        },
        &[],
        &touched.iter().map(|label| label.to_string()).collect(),
        "full",
    )
}

#[test]
fn disclosure_table_and_json_preserve_mode_and_status() -> Result<(), String> {
    use LanguageDisclosure::{AdvisoryOnly, PublicRun, Silent, TypedLimitation};
    use LanguageOutcomeStatus::*;
    for (status, repo, diff) in [
        (Complete, Silent, Silent),
        (CompleteZero, Silent, Silent),
        (NoCurrentScope, Silent, Silent),
        (Partial, PublicRun("partial"), TypedLimitation),
        (
            Unavailable,
            PublicRun("unavailable"),
            PublicRun("unavailable"),
        ),
        (Invalid, PublicRun("invalid"), PublicRun("invalid")),
        (Failed, PublicRun("failed"), PublicRun("failed")),
        (Unsupported, TypedLimitation, TypedLimitation),
        (NotSelected, AdvisoryOnly, AdvisoryOnly),
    ] {
        assert_eq!(LanguageDisclosure::of(OutcomeMode::Repo, status), repo);
        assert_eq!(LanguageDisclosure::of(OutcomeMode::Diff, status), diff);
    }
    let partial = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::Python,
        RepoOutcomeInput {
            findings: &[],
            production_files: 2,
            skipped_files: 0,
            partial_reason: Some("producer cap"),
            file_accounting: RepoFileAccounting::default(),
        },
        None,
    );
    let mut aggregate = LanguageAggregate::new_repo();
    aggregate.record(partial);
    let view = LanguageCompletenessView::from_aggregate(&aggregate.finish());
    assert_eq!(
        view.to_json_value(),
        serde_json::json!({
            "schema_version": "language_completeness.v1", "mode": "repo",
            "completeness": "incomplete", "total_findings": 0,
            "languages": [{ "language": "python", "status": "partial", "findings": 0,
                "limitations": ["producer cap"], "disclosure": "language_run" }]
        })
    );
    assert_eq!(
        view.public_runs,
        vec![LanguageRun {
            language: "python".to_string(),
            status: LanguageRunStatus::Partial,
            reason: Some("producer cap".to_string()),
        }]
    );
    assert!(view.reconcile_public_runs(&[]).is_err());
    view.reconcile_public_runs(&view.public_runs)?;
    Ok(())
}

#[test]
fn completeness_requires_evidence_and_rejects_contradictory_counts() -> Result<(), String> {
    assert_eq!(
        LanguageAggregate::new_repo().finish().completeness(),
        DiffCompleteness::Incomplete
    );
    let mut no_scope = LanguageAggregate::new_repo();
    no_scope.record(repo_child(LanguageId::Rust, &[], 0));
    assert_eq!(
        no_scope.finish().completeness(),
        DiffCompleteness::Incomplete
    );
    let complete_zero = repo_child(LanguageId::Rust, &[], 1);
    assert_eq!(complete_zero.status, LanguageOutcomeStatus::CompleteZero);
    let mut valid = LanguageAggregate::new_repo();
    valid.record(complete_zero.clone());
    let valid = valid.finish();
    valid.validate()?;
    assert_eq!(valid.completeness(), DiffCompleteness::Complete);
    for production_files in [None, Some(0)] {
        let mut wrong = complete_zero.clone();
        wrong.resources.production_files = production_files;
        let mut aggregate = LanguageAggregate::new_repo();
        aggregate.record(wrong);
        assert!(aggregate.finish().validate().is_err());
    }
    let mut wrong = complete_zero;
    wrong.findings_by_language.insert("rust".to_string(), 1);
    assert!(wrong.contradiction().is_some());
    let mut aggregate = LanguageAggregate::new_repo();
    aggregate.record(wrong);
    assert!(aggregate.finish().validate().is_err());
    Ok(())
}

#[test]
fn duplicate_order_keeps_one_blocker_and_the_same_digest() -> Result<(), String> {
    let complete = diff_children(LanguageId::Python, &[], &["python"])
        .pop()
        .ok_or("missing positive-scope child")?;
    let failed = LanguageAnalysisOutcomeV1::from_diff_err(
        LanguageId::Python,
        "failure".to_string(),
        LanguageRunStatus::Failed,
        "full",
    );
    let mut expected = None;
    for children in [
        vec![complete.clone(), failed.clone()],
        vec![failed.clone(), complete.clone()],
        vec![complete.clone(), failed.clone(), complete],
    ] {
        let mut aggregate = LanguageAggregate::new("full".to_string());
        for child in children {
            aggregate.record(child);
        }
        let aggregate = aggregate.finish();
        assert_eq!(aggregate.outcomes.len(), 1);
        assert_eq!(aggregate.blockers(), vec![("python", "failed")]);
        assert_eq!(aggregate.conflicts, BTreeSet::from(["python".to_string()]));
        assert!(aggregate.validate().is_err());
        if let Some(expected) = &expected {
            assert_eq!(&aggregate.semantic_digest, expected);
        }
        expected = Some(aggregate.semantic_digest);
    }
    Ok(())
}

#[test]
fn nonempty_family_counts_preserve_diff_labels_and_repo_deduplication() -> Result<(), String> {
    let findings = [
        finding(Some(LanguageId::TypeScript)),
        finding(Some(LanguageId::JavaScript)),
        finding(Some(LanguageId::Rust)),
        finding(None),
    ];
    let children = diff_children(
        LanguageId::TypeScript,
        &findings,
        &["typescript", "javascript"],
    );
    assert_eq!(children.len(), 2);
    let mut aggregate = LanguageAggregate::new("full".to_string());
    for child in children {
        aggregate.record(child);
    }
    let aggregate = aggregate.finish();
    let ts = aggregate
        .outcomes
        .get("typescript")
        .ok_or("missing typescript")?;
    let js = aggregate
        .outcomes
        .get("javascript")
        .ok_or("missing javascript")?;
    assert_eq!(
        ts.findings_by_language,
        BTreeMap::from([
            ("typescript".to_string(), 1),
            ("rust".to_string(), 1),
            ("unknown".to_string(), 1),
        ])
    );
    assert_eq!(
        js.findings_by_language,
        BTreeMap::from([("javascript".to_string(), 1)])
    );
    reconcile(&aggregate, &[LanguageId::TypeScript], &findings, None)?;
    let mut repo = LanguageAggregate::new_repo();
    for language in [LanguageId::TypeScript, LanguageId::JavaScript] {
        repo.record(repo_child(language, &findings, 4));
    }
    let repo = repo.finish();
    reconcile(
        &repo,
        &[LanguageId::TypeScript, LanguageId::JavaScript],
        &findings,
        None,
    )?;
    assert!(reconcile(&repo, &[LanguageId::TypeScript], &findings, None).is_err());
    let mut unequal = LanguageAggregate::new_repo();
    unequal.record(repo_child(LanguageId::TypeScript, &findings, 4));
    unequal.record(repo_child(LanguageId::JavaScript, &findings[..1], 4));
    assert!(
        reconcile(
            &unequal.finish(),
            &[LanguageId::TypeScript, LanguageId::JavaScript],
            &findings,
            None
        )
        .is_err()
    );
    Ok(())
}
