//! Native controls for the newly lowered immutable input boundary.
use super::*;
use std::collections::BTreeSet;

#[test]
fn repo_input_preserves_unknown_accounting_and_partial_reason() {
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::Python,
        RepoOutcomeInput {
            findings: &[],
            production_files: 2,
            skipped_files: 3,
            partial_reason: Some("producer-owned cap"),
            file_accounting: RepoFileAccounting::default(),
        },
        None,
    );
    assert_eq!(outcome.status, LanguageOutcomeStatus::Partial);
    assert_eq!(outcome.resources.production_files, Some(2));
    assert_eq!(outcome.resources.files_skipped, Some(3));
    assert_eq!(outcome.resources.files, RepoFileAccounting::default());
    assert_eq!(outcome.limitations, ["producer-owned cap"]);
    assert_eq!(
        outcome.compat_run().map(|run| run.status),
        Some(LanguageRunStatus::Partial)
    );
}

#[test]
fn diff_input_preserves_family_labels_partial_scope_and_generated_disclosure() {
    let children = LanguageAnalysisOutcomeV1::from_diff_result(
        LanguageId::TypeScript,
        DiffOutcomeInput {
            findings: &[],
            skipped_files: 2,
            partial_run_status: Some("selected scope"),
        },
        &[],
        &BTreeSet::from(["typescript".to_string(), "javascript".to_string()]),
        "partial:exact-source",
    );
    assert_eq!(children.len(), 2);
    assert_eq!(
        children
            .iter()
            .map(|child| child.language.as_str())
            .collect::<Vec<_>>(),
        ["javascript", "typescript"]
    );
    for child in children {
        assert_eq!(child.status, LanguageOutcomeStatus::Partial);
        assert_eq!(child.scope.as_deref(), Some("partial:exact-source"));
        assert_eq!(child.resources.files_skipped, Some(2));
        assert_eq!(
            child.limitations,
            [
                "limited_partial_scope: selected scope",
                "generated_files_skipped: 2"
            ]
        );
    }
}

#[test]
fn accounting_projection_keeps_unknown_separate_without_changing_identity() {
    let measured = RepoFileAccounting {
        discovered: Some(5),
        selected: Some(2),
        skipped: Some(1),
        omitted: Some(2),
        first_cap_fired: Some(RepoCapFired {
            name: "RIPR_MAX_REPO_INDEX_FILES",
            stage: RepoLimitingStage::Selection,
            limit: 2,
        }),
    };
    let build = |files| {
        LanguageAnalysisOutcomeV1::from_repo_result(
            LanguageId::Python,
            RepoOutcomeInput {
                findings: &[],
                production_files: 2,
                skipped_files: 1,
                partial_reason: Some("partial"),
                file_accounting: files,
            },
            None,
        )
    };
    let known = build(measured);
    let unknown = build(RepoFileAccounting::default());
    assert_eq!(known.resources.files, measured);
    assert_ne!(known.resources.files, unknown.resources.files);
    assert_eq!(known.semantic_digest, unknown.semantic_digest);
}

#[test]
fn reconciliation_projection_keeps_count_blocker_and_unsupported_refusals() -> Result<(), String> {
    let mut aggregate = LanguageAggregate::new("full".to_string());
    aggregate.record(LanguageAnalysisOutcomeV1::unsupported("rust", 1, "full"));
    let aggregate = aggregate.finish();
    let input = |finding_count, complete, has_unsupported_limitation| OutcomeReconciliationInput {
        finding_count,
        kind: if complete { "complete" } else { "partial" },
        complete,
        has_unsupported_limitation,
    };
    let selected = [LanguageId::Rust];
    assert!(reconcile(&aggregate, &selected, &[], Some(input(1, false, true))).is_err());
    assert!(reconcile(&aggregate, &selected, &[], Some(input(0, true, true))).is_err());
    assert!(reconcile(&aggregate, &selected, &[], Some(input(0, false, false))).is_err());
    reconcile(&aggregate, &selected, &[], Some(input(0, false, true)))?;
    Ok(())
}

#[test]
fn borrowed_repo_input_retains_the_established_digest_spelling() {
    let outcome = LanguageAnalysisOutcomeV1::from_repo_result(
        LanguageId::Python,
        RepoOutcomeInput {
            findings: &[],
            production_files: 1,
            skipped_files: 0,
            partial_reason: Some("b"),
            file_accounting: RepoFileAccounting::default(),
        },
        Some("a".to_string()),
    );
    assert_eq!(
        outcome.semantic_digest,
        format!("{:x}", Sha256::digest(b"v1|python|repo|partial|l:a|l:b"))
    );
}
