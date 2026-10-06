//! RIPR-SPEC-0240: unknown, not a gap.
//!
//! A `reachable_unrevealed` finding says the tests that reach a change do
//! not check it. ripr may make that claim only when the failing step was
//! established from evidence it could read. When every related `assert_eq!`
//! was refused because of a limit in ripr's own reading (an unparsed or
//! unplaced file, an unidentified test, a feature `cfg`, a binding that only
//! may rebind the macro), the finding becomes `static_unknown` with a named
//! `static_limit_kind`, so no renderer, gate, or repair route treats an
//! analyzer blind spot as a missing test.
//!
//! A refusal for a shape that can keep the assertion from running (a branch,
//! a closure, an opaque macro, a gated module, a real rebinding) keeps the
//! gap: runtime controls in `tests/owner_pin_execution.rs` prove those gaps.
//!
//! This is the single owner of that decision for Rust findings. It runs after
//! every other limit post-pass and never upgrades a class: it only withholds a
//! gap, which keeps the actionability flip fail-closed.

use super::decision::confidence_score;
use super::reveal::ASSERTION_CONTEXT_UNESTABLISHED;
use crate::domain::*;

/// Classifier-to-post-pass marker: every refused related `assert_eq!` was
/// refused for an analyzer limit. The post-pass removes it from the finding.
pub(in crate::analysis) const REFUSALS_ARE_ANALYZER_LIMITS: &str =
    "gap_admission: every refused related assertion rests on an analyzer limit";

const LIMIT: StaticLimitKind = StaticLimitKind::RustAssertionContextUnresolved;
const FIRST_UNRESOLVED_EDGE: &str =
    "whether the refused related assertions run as the standard `assert_eq!`";
const JUDGED_TEST_LIMIT: usize = 3;

/// Withhold a Rust gap whose evidence rests on an analyzer limit. Returns
/// `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_unsupported_gap(finding: &mut Finding) -> bool {
    let before = finding.evidence.len();
    finding
        .evidence
        .retain(|line| line != REFUSALS_ARE_ANALYZER_LIMITS);
    let marked = finding.evidence.len() != before;
    if !(marked && gap_rests_on_refusal(finding)) {
        return false;
    }
    let withheld_class = finding.class.as_str();
    finding.class = ExposureClass::StaticUnknown;
    // The classifier scored this finding as a gap, and a gap class earns a
    // bonus for a confident negative claim. Rescore for the class it now
    // has; `min` keeps any lower score an earlier limit pass assigned.
    let ripr = &finding.ripr;
    finding.confidence = finding.confidence.min(confidence_score(
        &ripr.reach,
        &ripr.infect,
        &ripr.propagate,
        &ripr.reveal.observe,
        &ripr.reveal.discriminate,
        &finding.class,
    ));
    finding.static_limit_kind = Some(LIMIT);
    if !finding
        .stop_reasons
        .contains(&StopReason::GapEvidenceUnresolved)
    {
        finding.stop_reasons.push(StopReason::GapEvidenceUnresolved);
    }
    let judged = judged_tests(finding);
    finding.evidence.push(format!(
        "gap_withheld: {withheld_class} is not claimed; {FIRST_UNRESOLVED_EDGE} ({})",
        LIMIT.as_str()
    ));
    finding.evidence.extend([
        format!(
            "{LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX}{}",
            last_established_edge(finding, &judged)
        ),
        format!("{LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX}{FIRST_UNRESOLVED_EDGE}"),
        format!("{LIMITATION_ANALYZER_ROUTE_PREFIX}analysis/classify/gap-admission"),
        format!(
            "{LIMITATION_NON_CLAIM_PREFIX}no missing test, no weak oracle, and no test adequacy is established"
        ),
    ]);
    // The absence lines ("no assertion observes ...") and the missing
    // discriminators are the unestablished claim itself. Keeping them would
    // feed repair placement and agent packets a gap this policy withheld.
    finding.missing = vec![LIMIT.describe().to_string()];
    finding.activation.missing_discriminators.clear();
    finding.recommended_next_step = Some(next_step(&judged));
    true
}

fn gap_rests_on_refusal(finding: &Finding) -> bool {
    // A producer that already named a limit owns the finding's explanation.
    finding.static_limit_kind.is_none()
        && finding.class == ExposureClass::ReachableUnrevealed
        && finding.ripr.reveal.observe.state == StageState::No
        && finding.ripr.reveal.observe.summary == ASSERTION_CONTEXT_UNESTABLISHED
}

/// Refusal scope is computed per finding, so every related test is a
/// candidate source of the refused assertions. The rows are already ranked
/// strongest relation first.
fn judged_tests(finding: &Finding) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for test in &finding.related_tests {
        let label = format!(
            "`{}` ({}:{})",
            test.name,
            test.file.display().to_string().replace('\\', "/"),
            test.line
        );
        if !names.contains(&label) {
            names.push(label);
        }
        if names.len() == JUDGED_TEST_LIMIT {
            break;
        }
    }
    names
}

fn last_established_edge(finding: &Finding, judged: &[String]) -> String {
    if judged.is_empty() {
        format!(
            "reach {}: {}",
            finding.ripr.reach.state.as_str(),
            finding.ripr.reach.summary
        )
    } else {
        format!("related tests {}", judged.join(", "))
    }
}

fn next_step(judged: &[String]) -> String {
    let tests = if judged.is_empty() {
        "the tests that call this code".to_string()
    } else {
        judged.join(", ")
    };
    format!(
        "ripr could not read whether the assertions in {tests} run as the standard `assert_eq!`, so it does not report a gap. Check that one of them compares the changed value; add a test only if none does."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    fn related(name: &str) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            line: 40,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::None,
            relation_reason: Some(RelationReason::DirectOwnerCall),
            relation_confidence: Some(RelationConfidence::High),
            miss: None,
        }
    }

    /// The serde `format_u8` shape: a feature-gated test whose loop
    /// `assert_eq!` was refused, marked by the classifier.
    fn marked() -> Finding {
        let yes = || stage(StageState::Yes, "yes");
        let mut activation = ActivationEvidence::default();
        activation
            .missing_discriminators
            .push(MissingDiscriminatorFact {
                value: "n == 100".to_string(),
                reason: "predicate boundary equality is not asserted".to_string(),
                flow_sink: None,
            });
        Finding {
            id: "probe:gap-admission".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:gap-admission".to_string()),
                location: SourceLocation::new("src/lib.rs", 10, 1),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "n > 99".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::ReachableUnrevealed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes, "Related tests appear to reach owner"),
                infect: yes(),
                propagate: yes(),
                reveal: RevealEvidence {
                    observe: stage(StageState::No, ASSERTION_CONTEXT_UNESTABLISHED),
                    discriminate: stage(StageState::No, "unestablished"),
                },
            },
            confidence: 0.79,
            evidence: vec![
                "assertion not credited: `assert_eq!` in test_format_u8 at src/lib.rs:41: the test carries `#[cfg(feature = \"std\")]`".to_string(),
                REFUSALS_ARE_ANALYZER_LIMITS.to_string(),
            ],
            missing: vec!["No relevant oracle was detected".to_string()],
            flow_sinks: Vec::new(),
            activation,
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: vec![related("test_format_u8")],
            recommended_next_step: None,
            language: Some(LanguageId::Rust),
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

    #[test]
    fn analyzer_limit_refusals_withhold_the_gap_and_name_the_test() {
        let mut finding = marked();
        assert!(withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert_eq!(finding.static_limit_kind, Some(LIMIT));
        assert_eq!(
            finding.stop_reasons,
            vec![StopReason::GapEvidenceUnresolved]
        );
        assert_eq!(finding.missing, vec![LIMIT.describe().to_string()]);
        assert!(finding.activation.missing_discriminators.is_empty());
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line == REFUSALS_ARE_ANALYZER_LIMITS),
            "the marker is consumed"
        );
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with("gap_withheld: reachable_unrevealed")),
            "{:?}",
            finding.evidence
        );
        assert_eq!(
            finding.recommended_next_step.as_deref(),
            Some(
                "ripr could not read whether the assertions in `test_format_u8` (src/lib.rs:40) run as the standard `assert_eq!`, so it does not report a gap. Check that one of them compares the changed value; add a test only if none does."
            )
        );
    }

    #[test]
    fn withholding_drops_the_gap_confidence_bonus_and_never_raises_a_score() {
        // reach/infect/propagate yes (0.2 each), observe/discriminate no
        // (0.02 each) score 0.64; reachable_unrevealed adds 0.15.
        let mut scored_as_gap = marked();
        assert!(withhold_unsupported_gap(&mut scored_as_gap));
        assert!((scored_as_gap.confidence - 0.64).abs() < 1e-6);

        let mut already_lower = marked();
        already_lower.confidence = 0.5;
        assert!(withhold_unsupported_gap(&mut already_lower));
        assert!((already_lower.confidence - 0.5).abs() < 1e-6);
    }

    #[test]
    fn an_unmarked_refusal_keeps_the_gap() {
        // A local-shape refusal (`if` branch, uncalled closure) leaves no
        // marker: the runtime controls prove those gaps.
        let mut finding = marked();
        finding
            .evidence
            .retain(|line| line != REFUSALS_ARE_ANALYZER_LIMITS);
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
        assert_eq!(finding.static_limit_kind, None);
    }

    #[test]
    fn a_marker_without_the_refused_observe_keeps_the_gap_and_is_consumed() {
        let mut finding = marked();
        finding.ripr.reveal.observe = stage(
            StageState::No,
            "Related tests were found, but no assertion appears to observe the changed value, error, field, or effect",
        );
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line == REFUSALS_ARE_ANALYZER_LIMITS)
        );
    }

    #[test]
    fn an_existing_named_limit_and_non_gap_classes_are_untouched() {
        let mut named = marked();
        named.static_limit_kind = Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
        assert!(!withhold_unsupported_gap(&mut named));
        assert_eq!(named.class, ExposureClass::ReachableUnrevealed);

        for class in [
            ExposureClass::Exposed,
            ExposureClass::WeaklyExposed,
            ExposureClass::NoStaticPath,
            ExposureClass::InfectionUnknown,
            ExposureClass::PropagationUnknown,
            ExposureClass::StaticUnknown,
        ] {
            let mut finding = marked();
            finding.class = class.clone();
            assert!(!withhold_unsupported_gap(&mut finding));
            assert_eq!(finding.class, class);
            assert_eq!(finding.static_limit_kind, None);
        }
    }

    #[test]
    fn withholding_twice_adds_one_stop_reason() {
        let mut finding = marked();
        assert!(withhold_unsupported_gap(&mut finding));
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(
            finding.stop_reasons,
            vec![StopReason::GapEvidenceUnresolved]
        );
    }
}
