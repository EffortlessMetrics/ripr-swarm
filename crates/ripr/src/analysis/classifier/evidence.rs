use crate::analysis::classify::{
    ProbeContext, PropagationWitnessV1, activation_evidence, classify, confidence_score,
    current_path_witness, infection_evidence, local_flow_sinks, propagation_evidence,
    reach_evidence, reveal_evidence_with_expression,
};
use crate::domain::*;

pub(in crate::analysis) struct ClassifiedProbeEvidence {
    pub(in crate::analysis) ripr: RiprEvidence,
    pub(in crate::analysis) evidence: Vec<String>,
    pub(in crate::analysis) flow_sinks: Vec<FlowSinkFact>,
    /// Retained diagnostic witness outcome for the PR-A migration slice.  It
    /// is not projected into `Finding` or used to strengthen a stage yet.
    pub(in crate::analysis) propagation_witness: Option<PropagationWitnessDiagnostic>,
    pub(in crate::analysis) activation: ActivationEvidence,
    pub(in crate::analysis) related_tests: Vec<RelatedTest>,
    pub(in crate::analysis) reach: StageEvidence,
    pub(in crate::analysis) infect: StageEvidence,
    pub(in crate::analysis) propagate: StageEvidence,
    pub(in crate::analysis) observe: StageEvidence,
    pub(in crate::analysis) discriminate: StageEvidence,
}

impl ClassifiedProbeEvidence {
    pub(in crate::analysis) fn gather(context: &ProbeContext<'_>, reveal_expression: &str) -> Self {
        let test_summaries = context.related_test_summaries();
        let reach = reach_evidence(&test_summaries, context.owner_fn);
        let flow_sinks = local_flow_sinks(context.probe, context.owner_fn);
        let propagation_witness = current_path_witness(context.probe, &flow_sinks)
            .map(PropagationWitnessDiagnostic::from_witness);
        let activation = activation_evidence(
            context.probe,
            context.owner_fn,
            &test_summaries,
            &flow_sinks,
            context.helper_chain.as_ref(),
            context.index,
            context.workspace_complete,
        );
        let infect = infection_evidence(context.probe, &test_summaries, &activation);
        let propagate = propagation_evidence(context.probe, &flow_sinks);
        let (observe, discriminate, related_tests) = reveal_evidence_with_expression(
            context.probe,
            reveal_expression,
            &context.related_tests,
        );

        let ripr = RiprEvidence {
            reach: reach.clone(),
            infect: infect.clone(),
            propagate: propagate.clone(),
            reveal: RevealEvidence {
                observe: observe.clone(),
                discriminate: discriminate.clone(),
            },
        };
        let evidence = evidence_summaries([&reach, &infect, &propagate, &observe, &discriminate]);

        Self {
            ripr,
            evidence,
            flow_sinks,
            propagation_witness,
            activation,
            related_tests,
            reach,
            infect,
            propagate,
            observe,
            discriminate,
        }
    }

    pub(in crate::analysis) fn classify(&self, probe: &Probe) -> ExposureClass {
        classify(
            &self.reach,
            &self.infect,
            &self.propagate,
            &self.observe,
            &self.discriminate,
            probe,
        )
    }

    pub(in crate::analysis) fn confidence(&self, class: &ExposureClass) -> f32 {
        confidence_score(
            &self.reach,
            &self.infect,
            &self.propagate,
            &self.observe,
            &self.discriminate,
            class,
        )
    }

    pub(in crate::analysis) fn propagation_witness(&self) -> Option<&PropagationWitnessDiagnostic> {
        self.propagation_witness.as_ref()
    }
}

pub(in crate::analysis) enum PropagationWitnessDiagnostic {
    Valid(PropagationWitnessV1),
    InvalidDigest(PropagationWitnessV1),
}

impl PropagationWitnessDiagnostic {
    fn from_witness(witness: PropagationWitnessV1) -> Self {
        if witness.digest_matches() {
            Self::Valid(witness)
        } else {
            Self::InvalidDigest(witness)
        }
    }

    pub(in crate::analysis) fn witness(&self) -> &PropagationWitnessV1 {
        match self {
            Self::Valid(witness) | Self::InvalidDigest(witness) => witness,
        }
    }

    pub(in crate::analysis) fn is_invalid(&self) -> bool {
        matches!(self, Self::InvalidDigest(_))
    }
}

fn evidence_summaries<'e>(stages: impl IntoIterator<Item = &'e StageEvidence>) -> Vec<String> {
    let mut summaries = stages
        .into_iter()
        .filter_map(|stage| (!stage.summary.is_empty()).then_some(stage.summary.clone()))
        .collect::<Vec<_>>();
    summaries.sort();
    summaries.dedup();
    summaries
}

#[cfg(test)]
mod tests {
    use super::evidence_summaries;
    use super::{ClassifiedProbeEvidence, ProbeContext, PropagationWitnessDiagnostic};
    use crate::analysis::classifier::finding::build_finding;
    use crate::analysis::facts::{FunctionSummary, ReturnFact, RustIndex};
    use crate::domain::{
        Confidence, DeltaKind, Probe, ProbeFamily, ProbeId, SourceLocation, StageEvidence,
        StageState, SymbolId,
    };
    use std::path::PathBuf;

    #[test]
    fn evidence_summaries_drop_empty_and_deduplicate_in_sorted_order() {
        let stages = [
            StageEvidence::new(StageState::Yes, Confidence::High, "z evidence"),
            StageEvidence::new(StageState::No, Confidence::Low, ""),
            StageEvidence::new(StageState::Weak, Confidence::Medium, "a evidence"),
            StageEvidence::new(StageState::Yes, Confidence::High, "z evidence"),
        ];

        assert_eq!(
            evidence_summaries(stages.iter()),
            vec!["a evidence".to_string(), "z evidence".to_string()]
        );
    }

    #[test]
    fn gather_retains_digest_valid_propagation_witness_for_internal_consumer() -> Result<(), String>
    {
        let probe = Probe {
            id: ProbeId("probe:fixture:1".to_string()),
            location: SourceLocation::new("src/lib.rs", 10, 2),
            owner: Some(SymbolId("owner:calculate".to_string())),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("amount".to_string()),
            after: Some("amount + 1".to_string()),
            expression: "amount".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let owner = FunctionSummary {
            id: SymbolId("owner:calculate".to_string()),
            name: "calculate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 20,
            body: "fn calculate(amount: i32) -> Result<i32, Error> { Ok(amount) }".to_string(),
            calls: Vec::new(),
            returns: vec![ReturnFact {
                line: 14,
                text: "Ok(amount)".to_string(),
            }],
            literals: Vec::new(),
            is_test: false,
            attrs: Vec::new(),
        };
        let index = RustIndex::default();
        let context = ProbeContext::new(&probe, Some(&owner), Vec::new(), false, &index, true);
        let evidence = ClassifiedProbeEvidence::gather(&context, "amount");
        let Some(diagnostic) = evidence.propagation_witness() else {
            return Err("gather discarded the producer witness".to_string());
        };
        if diagnostic.is_invalid() || !diagnostic.witness().digest_matches() {
            return Err("gather retained a stale witness digest".to_string());
        }
        Ok(())
    }

    #[test]
    fn corrupt_digest_is_retained_as_rejected_diagnostic_outcome() -> Result<(), String> {
        let probe = Probe {
            id: ProbeId("probe:fixture:corrupt".to_string()),
            location: SourceLocation::new("src/lib.rs", 10, 2),
            owner: Some(SymbolId("owner:calculate".to_string())),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("amount".to_string()),
            after: Some("amount + 1".to_string()),
            expression: "amount".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let owner = FunctionSummary {
            id: SymbolId("owner:calculate".to_string()),
            name: "calculate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 20,
            body: "fn calculate(amount: i32) -> Result<i32, Error> { Ok(amount) }".to_string(),
            calls: Vec::new(),
            returns: vec![ReturnFact {
                line: 14,
                text: "Ok(amount)".to_string(),
            }],
            literals: Vec::new(),
            is_test: false,
            attrs: Vec::new(),
        };
        let index = RustIndex::default();
        let context = ProbeContext::new(&probe, Some(&owner), Vec::new(), false, &index, true);
        let mut evidence = ClassifiedProbeEvidence::gather(&context, "amount");
        let Some(PropagationWitnessDiagnostic::Valid(witness)) =
            evidence.propagation_witness.as_mut()
        else {
            return Err("expected a valid witness before corruption".to_string());
        };
        witness.semantic_digest = "sha256:corrupt".to_string();
        let finding = build_finding(
            &context,
            crate::domain::ExposureClass::PropagationUnknown,
            evidence,
        );
        if !finding
            .evidence
            .iter()
            .any(|line| line == "propagation witness digest invalid; diagnostic witness withheld")
        {
            return Err("corrupt witness diagnostic was not emitted".to_string());
        }
        Ok(())
    }
}
