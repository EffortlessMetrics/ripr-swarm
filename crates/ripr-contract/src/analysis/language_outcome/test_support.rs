//! Private owned inputs for contract-native outcome controls; not adapter carriers.
use super::*;
#[derive(Default)]
pub(super) struct LanguageRepoResult {
    pub findings: Vec<Finding>,
    pub production_files: usize,
    pub skipped_files: usize,
    pub partial_reason: Option<String>,
    pub file_accounting: RepoFileAccounting,
}
impl<'a> From<&'a LanguageRepoResult> for RepoOutcomeInput<'a> {
    fn from(r: &'a LanguageRepoResult) -> Self {
        Self {
            findings: &r.findings,
            production_files: r.production_files,
            skipped_files: r.skipped_files,
            partial_reason: r.partial_reason.as_deref(),
            file_accounting: r.file_accounting,
        }
    }
}
#[derive(Default)]
pub(super) struct LanguageDiffResult {
    pub findings: Vec<Finding>,
    pub skipped_files: usize,
    // Existing helper records this input; outcome decisions never consume it.
    #[allow(dead_code)]
    pub changed_files: usize,
}
impl<'a> From<&'a LanguageDiffResult> for DiffOutcomeInput<'a> {
    fn from(r: &'a LanguageDiffResult) -> Self {
        Self {
            findings: &r.findings,
            skipped_files: r.skipped_files,
            partial_run_status: None,
        }
    }
}
pub(crate) fn test_finding() -> Result<Finding, String> {
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, Probe, ProbeFamily, ProbeId, RelatedTest,
        RiprEvidence, SourceLocation, StageEvidence, StageState,
    };
    let stage = || StageEvidence::new(StageState::Unknown, Confidence::Unknown, "test");
    Ok(Finding {
        analysis_scope: crate::domain::AnalysisScope::Diff,
        confidence_basis: Vec::new(),
        id: "finding:test:1".to_string(),
        canonical_gap: Some(crate::domain::FindingCanonicalGap {
            id: "gap:test:1".to_string(),
            language: "rust".to_string(),
            file: "src/lib.rs".to_string(),
            owner: "checkout".to_string(),
            behavior_kind: "predicate".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "total == 10000".to_string(),
        }),
        probe: Probe {
            id: ProbeId("probe:test:1".to_string()),
            location: SourceLocation::new("src/lib.rs", 12, 5),
            owner: Some(crate::domain::SymbolId("symbol:checkout".to_string())),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Value,
            before: Some("total > 10000".to_string()),
            after: Some("total >= 10000".to_string()),
            expression: "total >= 10000".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        },
        class: crate::domain::ExposureClass::WeaklyExposed,
        ripr: RiprEvidence {
            reach: stage(),
            infect: stage(),
            propagate: stage(),
            reveal: crate::domain::RevealEvidence {
                observe: stage(),
                discriminate: stage(),
            },
        },
        confidence: 0.5,
        evidence: vec!["the changed comparison reaches the checkout return".to_string()],
        missing: vec!["No strong discriminator was detected".to_string()],
        flow_sinks: Vec::new(),
        activation: ActivationEvidence::default(),
        stop_reasons: Vec::new(),
        related_tests_matched_total: Some(1),
        related_tests: vec![RelatedTest {
            name: "checkout_totals".to_string(),
            file: std::path::PathBuf::from("tests/checkout.rs"),
            line: 40,
            oracle: Some("assert_eq!(total, 10001)".to_string()),
            oracle_kind: crate::domain::OracleKind::ExactValue,
            oracle_strength: crate::domain::OracleStrength::Strong,
            relation_reason: Some(crate::domain::RelationReason::DirectOwnerCall),
            relation_confidence: Some(crate::domain::RelationConfidence::High),
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }],
        recommended_next_step: Some("add a boundary assertion for 10000".to_string()),
        language: Some(crate::domain::LanguageId::Rust),
        language_status: None,
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: Some("total".to_string()),
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
    })
}
