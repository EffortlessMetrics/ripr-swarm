//! Read-back helpers for conditional-return assertion refusals (#6541, #7974).
//!
//! The classifier writes the typed reason on the `assertion not credited:` evidence
//! line; renderers and gap withholding must not fall back to standard-macro wording
//! when that line names a guard instead.

use super::ASSERTION_NOT_CREDITED_PREFIX;

/// Why the related assertion was not credited, when the classifier recorded one.
pub fn assertion_not_credited_reason(evidence: &[String]) -> Option<&str> {
    assertion_not_credited_entry(evidence).and_then(|entry| {
        entry
            .rsplit_once(": ")
            .map(|(_, reason)| reason.trim())
            .filter(|reason| !reason.is_empty())
    })
}

/// Full `assertion not credited:` evidence line when it names the given test.
pub fn assertion_not_credited_entry_for_test<'a>(
    evidence: &'a [String],
    test_name: &str,
) -> Option<&'a str> {
    let needle = format!(" in {test_name} at ");
    evidence.iter().find_map(|line| {
        let body = line.strip_prefix(ASSERTION_NOT_CREDITED_PREFIX)?;
        body.contains(&needle).then_some(line.as_str())
    })
}

fn assertion_not_credited_entry(evidence: &[String]) -> Option<&str> {
    evidence
        .iter()
        .find_map(|entry| entry.strip_prefix(ASSERTION_NOT_CREDITED_PREFIX))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
}

/// Digest `Why …` hint from a guard-shaped refusal (#6541).
///
/// Only when the guard explains the finding class; never override `no_static_path`
/// and similar exposure explanations with an unrelated test's guard refusal.
pub fn guard_refusal_digest_classification_hint(
    finding: &crate::domain::Finding,
) -> Option<String> {
    use crate::domain::{ASSERTION_CONTEXT_UNESTABLISHED, ExposureClass, StageState};
    let reason = assertion_not_credited_reason(&finding.evidence)?;
    guard_refusal_kind(reason)?;
    let applies = match finding.class {
        ExposureClass::StaticUnknown => true,
        ExposureClass::ReachableUnrevealed => {
            finding.ripr.reveal.observe.summary == ASSERTION_CONTEXT_UNESTABLISHED
                && finding.ripr.reach.state == StageState::Yes
                && !finding
                    .ripr
                    .reach
                    .summary
                    .starts_with(crate::domain::CALLEE_ONLY_REACH_PREFIX)
        }
        _ => false,
    };
    if !applies {
        return None;
    }
    guard_refusal_classification_hint(reason)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardRefusalKind {
    /// `AssertionRefusal::ValueGuard` (RIPR-SPEC-0240 trap-tally control).
    ValueGuard,
    /// `AssertionContextRefusal::GuardedReturn` (trap-steps and certain gaps).
    GuardedReturn,
}

pub fn guard_refusal_kind(reason: &str) -> Option<GuardRefusalKind> {
    if reason.contains("spares the value the assertion expects") {
        Some(GuardRefusalKind::ValueGuard)
    } else if reason.contains("earlier `if") && reason.contains("can skip") {
        Some(GuardRefusalKind::GuardedReturn)
    } else {
        None
    }
}

/// Human `Why …` hint when the not-credited note names a conditional return.
pub fn guard_refusal_classification_hint(reason: &str) -> Option<String> {
    Some(match guard_refusal_kind(reason)? {
        GuardRefusalKind::ValueGuard => "a related test asserts here, but whether the assertion runs depends on a conditional return and a value ripr does not evaluate".into(),
        GuardRefusalKind::GuardedReturn => "a related test asserts here, but a conditional return before the assertion may skip it on the executed path".into(),
    })
}

/// First unresolved edge for `withhold_unsupported_gap` when refusals are guard-shaped.
pub fn guard_refusal_limitation_edge(reason: &str) -> Option<String> {
    Some(match guard_refusal_kind(reason)? {
        GuardRefusalKind::ValueGuard => "whether an earlier conditional return skips the assertion depends on a value ripr does not evaluate".into(),
        GuardRefusalKind::GuardedReturn => "whether an earlier conditional return skips the assertion on the executed path".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_guard_reason_is_recognized() {
        let reason = "an earlier `if weight(4) > 1000 { return; }` spares the value the assertion expects, so whether the assertion runs depends on the value of `weight(4)`, which ripr does not evaluate";
        assert_eq!(
            guard_refusal_kind(reason),
            Some(GuardRefusalKind::ValueGuard)
        );
        assert!(
            guard_refusal_classification_hint(reason)
                .is_some_and(|hint| !hint.contains("standard"))
        );
    }

    #[test]
    fn guarded_return_reason_is_recognized() {
        let reason = "the assertion is inside a block that an earlier `if weight(4) < 1000 { return; }` can skip";
        assert_eq!(
            guard_refusal_kind(reason),
            Some(GuardRefusalKind::GuardedReturn)
        );
        assert!(
            guard_refusal_classification_hint(reason)
                .is_some_and(|hint| !hint.contains("standard"))
        );
    }

    #[test]
    fn not_credited_entry_matches_test_name_in_location() {
        let evidence = vec![format!(
            "{ASSERTION_NOT_CREDITED_PREFIX}`assert_eq!` in weighs_two at src/lib.rs:12: guard reason"
        )];
        assert!(assertion_not_credited_entry_for_test(&evidence, "weighs_two").is_some());
        assert!(assertion_not_credited_entry_for_test(&evidence, "other_test").is_none());
    }

    use crate::domain::{
        ASSERTION_CONTEXT_UNESTABLISHED, ActivationEvidence, AnalysisScope,
        CALLEE_ONLY_REACH_PREFIX, Confidence, DeltaKind, ExposureClass, Finding, Probe,
        ProbeFamily, ProbeId, RevealEvidence, RiprEvidence, SourceCurrentness, SourceLocation,
        StageEvidence, StageState,
    };

    const VALUE_GUARD: &str = "an earlier `if weight(4) > 1000 { return; }` spares the value the assertion expects, so whether the assertion runs depends on the value of `weight(4)`, which ripr does not evaluate";
    const GUARDED_RETURN: &str = "the assertion is inside a block that an earlier `if weight(4) < 1000 { return; }` can skip";
    const VALUE_GUARD_HINT: &str = "a related test asserts here, but whether the assertion runs depends on a conditional return and a value ripr does not evaluate";
    const GUARDED_RETURN_HINT: &str = "a related test asserts here, but a conditional return before the assertion may skip it on the executed path";

    fn not_credited(test: &str, reason: &str) -> String {
        format!("{ASSERTION_NOT_CREDITED_PREFIX}`assert_eq!` in {test} at src/lib.rs:12: {reason}")
    }

    #[test]
    fn the_reason_is_the_text_after_the_location_on_the_first_not_credited_line() {
        let evidence = vec![
            "reach: weighs_two calls weight".to_string(),
            not_credited("weighs_two", GUARDED_RETURN),
            not_credited("weighs_three", VALUE_GUARD),
        ];
        assert_eq!(
            assertion_not_credited_reason(&evidence),
            Some(GUARDED_RETURN)
        );
        // Surrounding whitespace is not part of the reason.
        let padded = vec![format!(
            "{ASSERTION_NOT_CREDITED_PREFIX}`assert!` in t at src/lib.rs:3:   macro is shadowed  "
        )];
        assert_eq!(
            assertion_not_credited_reason(&padded),
            Some("macro is shadowed")
        );
    }

    #[test]
    fn evidence_without_a_recorded_reason_yields_none() {
        let no_line = vec!["reach: weighs_two calls weight".to_string()];
        assert_eq!(assertion_not_credited_reason(&no_line), None);
        assert_eq!(assertion_not_credited_reason(&[]), None);

        // The prefix alone, or a line whose reason is blank, records nothing.
        let bare = vec![ASSERTION_NOT_CREDITED_PREFIX.to_string()];
        assert_eq!(assertion_not_credited_reason(&bare), None);
        let blank_reason = vec![format!(
            "{ASSERTION_NOT_CREDITED_PREFIX}`assert!` in t at src/lib.rs:3:  "
        )];
        assert_eq!(assertion_not_credited_reason(&blank_reason), None);
        // A line with no `: ` separator has no reason part.
        let unseparated = vec![format!("{ASSERTION_NOT_CREDITED_PREFIX}no separator here")];
        assert_eq!(assertion_not_credited_reason(&unseparated), None);
    }

    #[test]
    fn the_entry_for_a_test_is_the_whole_line_that_names_it() {
        let two = not_credited("weighs_two", GUARDED_RETURN);
        let three = not_credited("weighs_three", VALUE_GUARD);
        let evidence = vec![
            "note:  in weighs_three at nowhere".to_string(),
            two.clone(),
            three.clone(),
        ];
        // A line without the prefix never matches, even when it names the test.
        assert_eq!(
            assertion_not_credited_entry_for_test(&evidence, "weighs_three"),
            Some(three.as_str())
        );
        assert_eq!(
            assertion_not_credited_entry_for_test(&evidence, "weighs_two"),
            Some(two.as_str())
        );
        // A name that is only a prefix of the recorded one is another test.
        assert_eq!(
            assertion_not_credited_entry_for_test(&evidence, "weighs"),
            None
        );
    }

    #[test]
    fn a_reason_needs_both_halves_of_the_guarded_return_wording() {
        assert_eq!(
            guard_refusal_kind("an earlier `if x { return; }` precedes it"),
            None
        );
        assert_eq!(guard_refusal_kind("a later statement can skip it"), None);
        assert_eq!(
            guard_refusal_kind("the assertion macro is shadowed by a local definition"),
            None
        );
        // The value guard wins when a reason carries both wordings.
        assert_eq!(
            guard_refusal_kind(
                "an earlier `if x { return; }` can skip it and spares the value the assertion expects"
            ),
            Some(GuardRefusalKind::ValueGuard)
        );
    }

    #[test]
    fn each_guard_kind_has_its_own_hint_and_limitation_edge() {
        assert_eq!(
            guard_refusal_classification_hint(VALUE_GUARD).as_deref(),
            Some(VALUE_GUARD_HINT)
        );
        assert_eq!(
            guard_refusal_classification_hint(GUARDED_RETURN).as_deref(),
            Some(GUARDED_RETURN_HINT)
        );
        assert_eq!(
            guard_refusal_limitation_edge(VALUE_GUARD).as_deref(),
            Some(
                "whether an earlier conditional return skips the assertion depends on a value ripr does not evaluate"
            )
        );
        assert_eq!(
            guard_refusal_limitation_edge(GUARDED_RETURN).as_deref(),
            Some("whether an earlier conditional return skips the assertion on the executed path")
        );
        // A refusal that names no guard gets neither: the caller keeps its
        // own wording instead of a guard explanation that does not apply.
        let other = "the assertion macro is shadowed by a local definition";
        assert_eq!(guard_refusal_classification_hint(other), None);
        assert_eq!(guard_refusal_limitation_edge(other), None);
    }

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    /// A `reachable_unrevealed` finding whose related assertion was refused
    /// for `reason`: the shape the digest hint applies to.
    fn guarded_finding(reason: &str) -> Finding {
        Finding {
            analysis_scope: AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: "finding:guard".to_string(),
            canonical_gap: None,
            probe: Probe {
                before_condition: None,
                id: ProbeId("probe:weight".to_string()),
                location: SourceLocation::new("src/lib.rs", 4, 5),
                owner: None,
                family: ProbeFamily::ReturnValue,
                delta: DeltaKind::Value,
                before: None,
                after: None,
                expression: "n * 2".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::ReachableUnrevealed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes, "Related test weighs_two calls weight"),
                infect: stage(StageState::Yes, "input reaches the change"),
                propagate: stage(StageState::Yes, "value is returned"),
                reveal: RevealEvidence {
                    observe: stage(StageState::No, ASSERTION_CONTEXT_UNESTABLISHED),
                    discriminate: stage(StageState::No, "no discriminator"),
                },
            },
            confidence: 0.5,
            evidence: vec![not_credited("weighs_two", reason)],
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: Vec::new(),
            recommended_next_step: None,
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

    #[test]
    fn the_digest_hint_names_the_guard_when_the_guard_explains_the_class() {
        assert_eq!(
            guard_refusal_digest_classification_hint(&guarded_finding(GUARDED_RETURN)).as_deref(),
            Some(GUARDED_RETURN_HINT)
        );
        assert_eq!(
            guard_refusal_digest_classification_hint(&guarded_finding(VALUE_GUARD)).as_deref(),
            Some(VALUE_GUARD_HINT)
        );

        // `static_unknown` takes the hint whatever its stage summaries say.
        let mut unknown = guarded_finding(VALUE_GUARD);
        unknown.class = ExposureClass::StaticUnknown;
        unknown.ripr.reveal.observe.summary = "opaque".to_string();
        unknown.ripr.reach.state = StageState::Unknown;
        assert_eq!(
            guard_refusal_digest_classification_hint(&unknown).as_deref(),
            Some(VALUE_GUARD_HINT)
        );
    }

    #[test]
    fn the_digest_hint_is_withheld_when_the_guard_does_not_explain_the_class() {
        // No recorded reason, or a reason that names no guard.
        let mut silent = guarded_finding(GUARDED_RETURN);
        silent.evidence.clear();
        assert_eq!(guard_refusal_digest_classification_hint(&silent), None);
        let shadowed = guarded_finding("the assertion macro is shadowed by a local definition");
        assert_eq!(guard_refusal_digest_classification_hint(&shadowed), None);

        // Another class keeps its own explanation.
        for class in [
            ExposureClass::NoStaticPath,
            ExposureClass::WeaklyExposed,
            ExposureClass::Exposed,
            ExposureClass::InfectionUnknown,
            ExposureClass::PropagationUnknown,
        ] {
            let mut other = guarded_finding(GUARDED_RETURN);
            other.class = class.clone();
            assert_eq!(
                guard_refusal_digest_classification_hint(&other),
                None,
                "{class:?}"
            );
        }

        // `reachable_unrevealed` for a reason other than an unestablished
        // assertion context.
        let mut no_assertion = guarded_finding(GUARDED_RETURN);
        no_assertion.ripr.reveal.observe.summary = "No oracle found".to_string();
        assert_eq!(
            guard_refusal_digest_classification_hint(&no_assertion),
            None
        );

        // The related tests do not reach the changed owner.
        let mut unreached = guarded_finding(GUARDED_RETURN);
        unreached.ripr.reach.state = StageState::Weak;
        assert_eq!(guard_refusal_digest_classification_hint(&unreached), None);

        // They only call the wrapper seam's converted callee.
        let mut callee_only = guarded_finding(GUARDED_RETURN);
        callee_only.ripr.reach.summary = format!("{CALLEE_ONLY_REACH_PREFIX}: weighs_two");
        assert_eq!(guard_refusal_digest_classification_hint(&callee_only), None);
    }
}
