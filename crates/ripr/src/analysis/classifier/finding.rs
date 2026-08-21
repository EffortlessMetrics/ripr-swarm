use super::evidence::ClassifiedProbeEvidence;
use crate::analysis::classify::{
    ProbeContext, ensure_unknown_stop_reason, exact_error_variant, missing_evidence,
    recommended_next_step, stop_reasons,
};
use crate::domain::*;

pub(in crate::analysis) fn build_finding(
    context: &ProbeContext<'_>,
    class: ExposureClass,
    evidence: ClassifiedProbeEvidence,
) -> Finding {
    let missing = missing_evidence(
        context.probe,
        &class,
        &evidence.infect,
        &evidence.observe,
        &evidence.discriminate,
        &evidence.activation,
    );
    let test_summaries = context.related_test_summaries();
    let mut stop_reasons = stop_reasons(context.probe, context.owner_fn, &test_summaries);
    ensure_unknown_stop_reason(&class, &mut stop_reasons);
    let exact_oracle_covers_direct_sink = evidence.discriminate.state == StageState::Yes
        && matches!(
            context.probe.family,
            ProbeFamily::ReturnValue | ProbeFamily::ErrorPath | ProbeFamily::FieldConstruction
        )
        && exact_oracle_aligns_with_sink(context.probe, &evidence);
    let recommended_next_step =
        if class == ExposureClass::WeaklyExposed && exact_oracle_covers_direct_sink {
            None
        } else {
            recommended_next_step(context.probe, &class, context.owner_assertion_shaped)
        };
    let confidence = evidence.confidence(&class);
    let invalid_propagation_witness = evidence.propagation_witness().is_some_and(|diagnostic| {
        diagnostic.is_invalid() || !diagnostic.witness().digest_matches()
    });
    let mut evidence_lines = evidence.evidence;
    if class == ExposureClass::WeaklyExposed && exact_oracle_covers_direct_sink {
        evidence_lines.push(
            "static limitation: exact oracle established; no assertion repair is indicated"
                .to_string(),
        );
    }
    if invalid_propagation_witness {
        evidence_lines
            .push("propagation witness digest invalid; diagnostic witness withheld".to_string());
    }
    // RIPR-SPEC-0133: disclose the oracle reframe so JSON/human projections
    // show why the guidance is phrased for an assertion helper.
    if context.owner_assertion_shaped {
        evidence_lines.push(format!(
            "owner_shape: assertion_shaped ({})",
            crate::domain::ASSERTION_SHAPED_OWNER_REASON
        ));
    }

    Finding {
        id: context.probe.id.0.clone(),
        canonical_gap: None,
        probe: context.probe.clone(),
        class,
        ripr: evidence.ripr,
        confidence,
        evidence: evidence_lines,
        missing,
        flow_sinks: evidence.flow_sinks,
        activation: evidence.activation,
        stop_reasons,
        related_tests: evidence.related_tests,
        recommended_next_step,
        // Language metadata is populated by the per-language adapter
        // (e.g. `analysis::language::RustAdapter::analyze_diff`) after
        // classification. The classifier itself stays language-neutral.
        language: None,
        language_status: None,
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: None,
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        // Source currentness is resolved by the producer that observed the diff
        // evidence; this constructor has none, so the disposition stays the
        // explicit unknown (#3280).
        source_currentness: crate::domain::SourceCurrentness::UnresolvedSubject,
    }
}

fn exact_oracle_aligns_with_sink(probe: &Probe, evidence: &ClassifiedProbeEvidence) -> bool {
    let Some(sink) = evidence.flow_sinks.iter().find(|sink| {
        matches!(
            sink.kind,
            FlowSinkKind::ReturnValue | FlowSinkKind::ErrorVariant | FlowSinkKind::StructField
        )
    }) else {
        return false;
    };
    evidence.related_tests.iter().any(|related| {
        let Some(oracle) = related.oracle.as_deref() else {
            return false;
        };
        match probe.family {
            ProbeFamily::ErrorPath => exact_error_variant(&probe.expression)
                .is_some_and(|variant| oracle.contains(&variant)),
            ProbeFamily::ReturnValue | ProbeFamily::FieldConstruction => {
                semantic_tokens(&sink.text)
                    .iter()
                    .any(|token| contains_identifier(oracle, token))
            }
            _ => false,
        }
    })
}

fn semantic_tokens(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .filter(|token| {
            token
                .chars()
                .any(|character| character.is_ascii_alphabetic())
        })
        .map(str::to_string)
        .collect()
}

fn contains_identifier(text: &str, token: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|candidate| candidate == token)
}
