use super::*;

fn finding_in(language: Option<LanguageId>) -> Result<Finding, String> {
    let mut finding = crate::mcp::gaps::test_finding()?;
    finding.language = language;
    Ok(finding)
}

fn result(findings: Vec<Finding>, partial_reason: Option<&str>) -> LanguageRepoResult {
    LanguageRepoResult {
        findings,
        production_files: 3,
        skipped_files: 1,
        partial_reason: partial_reason.map(str::to_string),
        ..LanguageRepoResult::default()
    }
}

#[test]
fn clean_run_without_findings_is_complete_zero_and_silent_on_the_compat_surface() {
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::Python,
        &result(Vec::new(), None),
        None,
    );
    assert_eq!(outcome.status, LanguageOutcomeStatus::CompleteZero);
    assert_eq!(outcome.finding_count(), 0);
    assert!(outcome.compat_run().is_none());
}

#[test]
fn clean_run_with_findings_is_complete_and_silent_on_the_compat_surface() -> Result<(), String> {
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::Python,
        &result(vec![finding_in(Some(LanguageId::Python))?], None),
        None,
    );
    assert_eq!(outcome.status, LanguageOutcomeStatus::Complete);
    assert_eq!(outcome.finding_count(), 1);
    assert!(outcome.compat_run().is_none());
    Ok(())
}

#[test]
fn partial_reason_is_never_complete_zero_and_projects_to_the_exact_compat_run() {
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::TypeScript,
        &result(Vec::new(), Some("typescript_repo_capped: 800")),
        None,
    );
    assert_eq!(outcome.status, LanguageOutcomeStatus::Partial);
    assert_eq!(
        outcome.compat_run(),
        Some(LanguageRun {
            language: "typescript".to_string(),
            status: LanguageRunStatus::Partial,
            reason: Some("typescript_repo_capped: 800".to_string()),
        })
    );
    assert_eq!(
        outcome.resources.bound_fired.as_deref(),
        Some("partial_reason")
    );
}

#[test]
fn extra_disclosure_joins_the_adapter_reason_in_order() {
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::Rust,
        &result(Vec::new(), Some("a")),
        Some("b".to_string()),
    );
    assert_eq!(
        outcome.compat_run().and_then(|run| run.reason),
        Some("a | b".to_string())
    );
}

#[test]
fn typescript_family_run_keeps_typescript_and_javascript_counts_separate() -> Result<(), String> {
    let findings = vec![
        finding_in(Some(LanguageId::TypeScript))?,
        finding_in(Some(LanguageId::JavaScript))?,
        finding_in(Some(LanguageId::JavaScript))?,
        finding_in(None)?,
    ];
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::TypeScript,
        &result(findings, None),
        None,
    );
    assert_eq!(outcome.findings_by_language.get("typescript"), Some(&1));
    assert_eq!(outcome.findings_by_language.get("javascript"), Some(&2));
    assert_eq!(outcome.findings_by_language.get("unknown"), Some(&1));
    assert_eq!(outcome.finding_count(), 4);
    Ok(())
}

#[test]
fn resources_record_only_measured_producer_facts() {
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::Python,
        &result(Vec::new(), None),
        None,
    );
    assert_eq!(outcome.resources.files_skipped, Some(1));
    assert_eq!(outcome.resources.production_files, Some(3));
    assert_eq!(outcome.resources.bound_fired, None);
}

#[test]
fn only_partial_leaves_a_reasoned_compat_record() {
    use LanguageOutcomeStatus::*;
    for (status, expected) in [
        (Partial, Some(LanguageRunStatus::Partial)),
        (Complete, None),
        (CompleteZero, None),
    ] {
        let outcome = LanguageAnalysisOutcomeV1::new(
            LanguageId::Perl,
            status,
            BTreeMap::new(),
            LanguageResourceObservations::default(),
            Vec::new(),
        );
        let run = outcome.compat_run();
        assert_eq!(run.as_ref().map(|run| run.status), expected, "{status:?}");
        if let Some(run) = run {
            assert_eq!(run.reason.as_deref(), Some(status.as_str()));
        }
    }
}

#[test]
fn digest_tracks_semantics_but_not_resource_telemetry_or_limitation_order() {
    let outcome = |status, resources, limitations: &[&str]| {
        LanguageAnalysisOutcomeV1::new(
            LanguageId::Python,
            status,
            BTreeMap::new(),
            resources,
            limitations.iter().map(|l| l.to_string()).collect(),
        )
    };
    let base = outcome(
        LanguageOutcomeStatus::Partial,
        LanguageResourceObservations::default(),
        &["a", "b"],
    );
    let telemetry_and_order = outcome(
        LanguageOutcomeStatus::Partial,
        LanguageResourceObservations {
            files_skipped: Some(9),
            production_files: Some(9),
            bound_fired: Some("x".to_string()),
        },
        &["b", "a"],
    );
    assert_eq!(base.semantic_digest, telemetry_and_order.semantic_digest);
    let other_status = outcome(
        LanguageOutcomeStatus::CompleteZero,
        LanguageResourceObservations::default(),
        &["a", "b"],
    );
    assert_ne!(base.semantic_digest, other_status.semantic_digest);
}
