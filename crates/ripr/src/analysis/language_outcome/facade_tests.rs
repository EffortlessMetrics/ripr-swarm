//! Real carrier projection parity; policy assertions live in contract native tests.
use super::*;
#[test]
fn repo_carrier_projection_preserves_every_consumed_field() -> Result<(), String> {
    let r = LanguageRepoResult {
        findings: vec![crate::mcp::gaps::test_finding()?],
        file_accounting: crate::analysis::language::RepoFileAccounting {
            discovered: Some(8),
            selected: Some(3),
            skipped: Some(2),
            omitted: Some(3),
            first_cap_fired: None,
        },
        production_files: 3,
        skipped_files: 2,
        partial_reason: Some("cap".into()),
        ..Default::default()
    };
    let p = RepoOutcomeInput::from(&r);
    assert!(std::ptr::eq(p.findings, r.findings.as_slice()));
    assert_eq!(
        (
            p.production_files,
            p.skipped_files,
            p.partial_reason,
            p.file_accounting
        ),
        (
            r.production_files,
            r.skipped_files,
            r.partial_reason.as_deref(),
            r.file_accounting
        )
    );
    assert_eq!(
        LanguageAnalysisOutcomeV1::from_repo_result(LanguageId::Python, &r, None),
        LanguageAnalysisOutcomeV1::from_repo_result(LanguageId::Python, p, None)
    );
    Ok(())
}
#[test]
fn diff_carrier_projection_preserves_every_consumed_field() -> Result<(), String> {
    let r = LanguageDiffResult {
        findings: vec![crate::mcp::gaps::test_finding()?],
        partial_scope: Some(PartialDiffScope {
            run_status: "limited_partial_scope".into(),
            diff_identity: "source".into(),
            file_budget: 1,
            line_budget: 1,
            budget_disclosures: vec![],
            selected_files: vec!["src/lib.rs".into()],
            unselected_files: vec!["src/other.rs".into()],
            selected_changed_lines: 1,
            uninspected_files_lower_bound: 1,
            uninspected_changed_lines_lower_bound: 1,
            stop_reason: crate::analysis::language::PartialDiffStopReason::FileBudget,
            next_file_changed_lines: Some(1),
            whole_diff_budgets: Some((2, 2)),
            partition_identity: "partition".into(),
        }),
        skipped_files: 2,
        ..Default::default()
    };
    let p = DiffOutcomeInput::from(&r);
    assert!(std::ptr::eq(p.findings, r.findings.as_slice()));
    assert_eq!(
        (p.skipped_files, p.partial_run_status),
        (
            r.skipped_files,
            r.partial_scope.as_ref().map(|s| s.run_status.as_str())
        )
    );
    let touched = std::collections::BTreeSet::from(["rust".to_string()]);
    assert_eq!(
        LanguageAnalysisOutcomeV1::from_diff_result(LanguageId::Rust, &r, &[], &touched, "full"),
        LanguageAnalysisOutcomeV1::from_diff_result(LanguageId::Rust, p, &[], &touched, "full")
    );
    Ok(())
}
