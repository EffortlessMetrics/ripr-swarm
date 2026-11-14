use crate::domain::*;

/// A generator changes the test stimulus, not an observed product return.
/// Keep the subject visible without inventing reach, an oracle, or a gap.
pub(super) fn finding(probe: &Probe) -> Finding {
    let reason = "QuickCheck test-input generator changes test stimulus; static product exposure is not established";
    let stage = StageEvidence::new(StageState::Unknown, Confidence::Low, reason);
    Finding {
        analysis_scope: AnalysisScope::Diff,
        confidence_basis: Vec::new(),
        id: probe.id.0.clone(),
        canonical_gap: None,
        probe: probe.clone(),
        class: ExposureClass::StaticUnknown,
        ripr: RiprEvidence {
            reach: stage.clone(),
            infect: stage.clone(),
            propagate: stage.clone(),
            reveal: RevealEvidence { observe: stage.clone(), discriminate: stage },
        },
        confidence: 0.0,
        evidence: vec![reason.to_string()],
        missing: vec!["The effect of the changed generator on test stimulus has not been evaluated".to_string()],
        flow_sinks: Vec::new(),
        activation: ActivationEvidence::default(),
        stop_reasons: vec![StopReason::StaticProbeUnknown],
        related_tests_matched_total: None,
        related_tests: Vec::new(),
        recommended_next_step: Some("Review how the generator changes the inputs supplied to properties, including distribution and shrinking. This finding does not establish a missing product test.".to_string()),
        language: None,
        language_status: None,
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: None,
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness: SourceCurrentness::CandidateCurrent,
    }
}
