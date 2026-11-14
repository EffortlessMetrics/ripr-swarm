//! Closed input domains for the #11912 decision proofs and ordinary tests.
//!
//! Each declaration generates its array and its wildcard-free compiler check
//! from the SAME variants. Removing a row or adding an enum variant therefore
//! fails compilation. Duplicate rows are rejected too. No discriminant casts,
//! fabricated enum values, unsafe code, or separately maintained variant count.

use super::executed_control::{ObservedOutcome, ResultState};
use super::{ExposureClass, FeedbackJudgment, FeedbackReason, GapState, OracleStrength};
use super::{ProbeFamily, SourceCurrentness};

macro_rules! exhaustive_domain {
    ($name:ident: $ty:ty, $cover:ident; $($variant:path),+ $(,)?) => {
        pub(super) const $name: &[$ty] = &[$($variant),+];
        const _: fn($ty) = |value| {
            #[deny(unreachable_patterns)]
            match value { $($variant => (),)+ }
        };
        #[cfg(kani)]
        pub(super) fn $cover(value: &$ty) {
            $(kani::cover!(*value == $variant);)+
        }
    };
}

exhaustive_domain!(GAP_STATES: GapState, cover_gap_states;
    GapState::Actionable, GapState::AlreadyObserved, GapState::Resolved, GapState::Unknown);
exhaustive_domain!(ORACLES: OracleStrength, cover_oracles;
    OracleStrength::Strong, OracleStrength::Medium, OracleStrength::Weak,
    OracleStrength::Smoke, OracleStrength::None, OracleStrength::Unknown);
exhaustive_domain!(RESULT_STATES: ResultState, cover_result_states;
    ResultState::Passed, ResultState::Failed, ResultState::NotRun,
    ResultState::NotProven, ResultState::Substituted, ResultState::InstrumentFailure);
exhaustive_domain!(OUTCOMES: ObservedOutcome, cover_outcomes;
    ObservedOutcome::FailedBeforePassedAfter, ObservedOutcome::RejectedWrongImplementation,
    ObservedOutcome::CommandSucceededWithoutExercisingSubject,
    ObservedOutcome::OrdinaryPositiveTestsPassed, ObservedOutcome::ReviewArgumentOnly,
    ObservedOutcome::StructuralDiscriminationOnly, ObservedOutcome::InstrumentUnavailable,
    ObservedOutcome::NotExecuted);
exhaustive_domain!(CURRENTNESS: SourceCurrentness, cover_currentness;
    SourceCurrentness::CandidateCurrent, SourceCurrentness::BaseDeleted,
    SourceCurrentness::MovedOrRenamed, SourceCurrentness::UnresolvedSubject);
exhaustive_domain!(FAMILIES: ProbeFamily, cover_families;
    ProbeFamily::Predicate, ProbeFamily::ReturnValue, ProbeFamily::ErrorPath,
    ProbeFamily::CallDeletion, ProbeFamily::FieldConstruction, ProbeFamily::SideEffect,
    ProbeFamily::MatchArm, ProbeFamily::StaticUnknown);
exhaustive_domain!(CLASSES: ExposureClass, cover_classes;
    ExposureClass::Exposed, ExposureClass::WeaklyExposed, ExposureClass::ReachableUnrevealed,
    ExposureClass::NoStaticPath, ExposureClass::InfectionUnknown,
    ExposureClass::PropagationUnknown, ExposureClass::StaticUnknown);
exhaustive_domain!(JUDGMENTS: FeedbackJudgment, cover_judgments;
    FeedbackJudgment::Useful, FeedbackJudgment::Incorrect, FeedbackJudgment::Unclear,
    FeedbackJudgment::Expensive, FeedbackJudgment::IntentionalNoAction);
exhaustive_domain!(REASONS: FeedbackReason, cover_reasons;
    FeedbackReason::UsefulActionable, FeedbackReason::UsefulLimitation,
    FeedbackReason::CorrectTarget, FeedbackReason::CorrectDiscriminator,
    FeedbackReason::ClearExplanation, FeedbackReason::WrongTarget,
    FeedbackReason::WrongDiscriminator, FeedbackReason::FalseActionable,
    FeedbackReason::DuplicateItem, FeedbackReason::AlreadyCovered,
    FeedbackReason::IncorrectCausalAttribution, FeedbackReason::StaleResult,
    FeedbackReason::MissingContext, FeedbackReason::AmbiguousTarget,
    FeedbackReason::UnclearExplanation, FeedbackReason::VerifyCommandFailed,
    FeedbackReason::ReceiptCommandFailed, FeedbackReason::RouteTooBroad,
    FeedbackReason::TooSlow, FeedbackReason::TooNoisy, FeedbackReason::UnsupportedWorkflow,
    FeedbackReason::IntentionalDefensiveCode, FeedbackReason::IntentionalLowGrip,
    FeedbackReason::ManualReviewPreferred, FeedbackReason::Other);

/// SPEC-0179's five distinct feedback meanings, pinned per reason. This oracle
/// must not call `judgment`, parse its output, or derive values from its rank.
pub(super) fn expected_judgment(reason: FeedbackReason) -> FeedbackJudgment {
    match reason {
        FeedbackReason::UsefulActionable => FeedbackJudgment::Useful,
        FeedbackReason::UsefulLimitation => FeedbackJudgment::Useful,
        FeedbackReason::CorrectTarget => FeedbackJudgment::Useful,
        FeedbackReason::CorrectDiscriminator => FeedbackJudgment::Useful,
        FeedbackReason::ClearExplanation => FeedbackJudgment::Useful,
        FeedbackReason::WrongTarget => FeedbackJudgment::Incorrect,
        FeedbackReason::WrongDiscriminator => FeedbackJudgment::Incorrect,
        FeedbackReason::FalseActionable => FeedbackJudgment::Incorrect,
        FeedbackReason::DuplicateItem => FeedbackJudgment::Incorrect,
        FeedbackReason::AlreadyCovered => FeedbackJudgment::Incorrect,
        FeedbackReason::IncorrectCausalAttribution => FeedbackJudgment::Incorrect,
        FeedbackReason::StaleResult => FeedbackJudgment::Incorrect,
        FeedbackReason::MissingContext => FeedbackJudgment::Unclear,
        FeedbackReason::AmbiguousTarget => FeedbackJudgment::Unclear,
        FeedbackReason::UnclearExplanation => FeedbackJudgment::Unclear,
        FeedbackReason::VerifyCommandFailed => FeedbackJudgment::Expensive,
        FeedbackReason::ReceiptCommandFailed => FeedbackJudgment::Expensive,
        FeedbackReason::RouteTooBroad => FeedbackJudgment::Expensive,
        FeedbackReason::TooSlow => FeedbackJudgment::Expensive,
        FeedbackReason::TooNoisy => FeedbackJudgment::Expensive,
        FeedbackReason::UnsupportedWorkflow => FeedbackJudgment::Expensive,
        FeedbackReason::IntentionalDefensiveCode => FeedbackJudgment::IntentionalNoAction,
        FeedbackReason::IntentionalLowGrip => FeedbackJudgment::IntentionalNoAction,
        FeedbackReason::ManualReviewPreferred => FeedbackJudgment::IntentionalNoAction,
        FeedbackReason::Other => FeedbackJudgment::Unclear,
    }
}

/// SPEC-0183: these two states CLAIM satisfaction. Packet validation separately
/// requires actual evidence or an admitted substitute; this is not admission.
pub(super) fn expected_satisfaction(state: ResultState) -> bool {
    match state {
        ResultState::Passed => true,
        ResultState::Substituted => true,
        ResultState::Failed => false,
        ResultState::NotRun => false,
        ResultState::NotProven => false,
        ResultState::InstrumentFailure => false,
    }
}

/// SPEC-0183 requires an executed discriminator for an admitted substitute.
/// An unavailable instrument is a distinct gap, not an unexecuted substitute.
pub(super) fn expected_unexecuted(outcome: ObservedOutcome) -> bool {
    match outcome {
        ObservedOutcome::FailedBeforePassedAfter => false,
        ObservedOutcome::RejectedWrongImplementation => false,
        ObservedOutcome::InstrumentUnavailable => false,
        ObservedOutcome::CommandSucceededWithoutExercisingSubject => true,
        ObservedOutcome::OrdinaryPositiveTestsPassed => true,
        ObservedOutcome::ReviewArgumentOnly => true,
        ObservedOutcome::StructuralDiscriminationOnly => true,
        ObservedOutcome::NotExecuted => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_feedback_reason_has_its_contract_judgment_and_note_rule() {
        for &reason in REASONS {
            assert_eq!(reason.judgment(), expected_judgment(reason), "{reason:?}");
            assert_eq!(reason.requires_note(), reason == FeedbackReason::Other);
        }
        assert_eq!(FeedbackReason::all().as_slice(), REASONS);
    }

    #[test]
    fn every_result_state_claims_exactly_its_contract_satisfaction() {
        for &state in RESULT_STATES {
            assert_eq!(
                state.claims_satisfaction(),
                expected_satisfaction(state),
                "{state:?}"
            );
        }
    }

    #[test]
    fn symbolic_domains_are_nonempty_and_unique() {
        fn check<T: PartialEq + std::fmt::Debug>(values: &[T]) {
            assert!(!values.is_empty());
            for (index, value) in values.iter().enumerate() {
                assert!(!values[..index].contains(value), "duplicate {value:?}");
            }
        }
        check(GAP_STATES);
        check(ORACLES);
        check(RESULT_STATES);
        check(OUTCOMES);
        check(CURRENTNESS);
        check(FAMILIES);
        check(CLASSES);
        check(JUDGMENTS);
        check(REASONS);
    }
}
