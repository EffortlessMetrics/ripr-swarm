//! Property tests for the TypeScript preview actionability mapper (#13948).
//!
//! These properties characterize current behavior, not a permanent contract.
//! Today `typescript_actionability_for` never promotes a finding: every branch
//! returns a non-actionable `EvidenceState`. RIPR-SPEC-0087 (proposed, PR 7 of
//! the RIPR-SPEC-0085 campaign) plans to flip a complete-contract TypeScript
//! finding to actionable through this mapper. When 0087 lands, the "never
//! actionable" assertion and `expected_refusal` must be revised to admit that
//! branch, keeping every refusal below fail-closed. The example tests and the
//! exhaustive two-value sweep in `actionability.rs` pin one static-limit kind
//! and one discriminator; these properties range over every `ExposureClass`,
//! every `StaticLimitKind`, arbitrary repair-route text, any discriminator
//! count and shape, and both values of each boolean input.
//!
//! Refusal conditions, in the mapper's own precedence order:
//!
//! 1. `private_member_unknown` -> `unknown` / `private_member_reach_unknown`;
//! 2. a static limit -> `static_limitation` / the limit kind's word;
//! 3. `exposed` -> `already_observed` (no repair packet for a strong oracle);
//! 4. `no_static_path` -> `unknown` / `missing_context`;
//! 5. no owner-call evidence -> `unknown` / `ambiguous_related_test`;
//! 6. no missing discriminator -> `unknown` / `missing_target_shape`;
//! 7. otherwise -> `unknown` / `incomplete_repair_packet`.
//!
//! `expected_refusal` mirrors the mapper's branch order. It is a
//! characterization pin that catches a changed branch, state or category, not
//! an independent oracle of what the order should be.
//!
//! Monotonicity: since no input yields an actionable state today, "removing evidence
//! never turns non-actionable into actionable" holds trivially. The stronger,
//! true invariant asserted here is that an established refusal (conditions 1
//! and 2) is stable under evidence removal: dropping owner-call evidence or
//! missing discriminators never changes the refusal category or state.
//!
//! The default case count is small so the required test run stays cheap. The
//! mapper is a pure function, so each case costs microseconds. Run longer
//! locally with `PROPTEST_CASES=2000 cargo test -p ripr --lib
//! actionability_proptest_tests`.

use super::actionability::{TypeScriptActionability, typescript_actionability_for};
use super::*;
use crate::domain::{DiscriminatorShape, EvidenceState};
use proptest::prelude::*;
use proptest::test_runner::Config;

const DEFAULT_CASES: u32 = 64;

fn config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CASES),
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..Config::default()
    }
}

/// Every `StaticLimitKind`. `ripr_contract`'s own `ALL` list is
/// `#[cfg(test)]` in that crate and not visible here, so this copy is kept in
/// step by the exhaustive match in `static_limit_kind_ordinal`: a new variant
/// fails to compile there, and a variant added to the match but not to this
/// array fails `static_limit_kind_list_is_complete_and_ordered`.
const STATIC_LIMIT_KINDS: [StaticLimitKind; 35] = [
    StaticLimitKind::DynamicDispatch,
    StaticLimitKind::Metaprogramming,
    StaticLimitKind::MissingImportGraph,
    StaticLimitKind::DecoratorIndirection,
    StaticLimitKind::MockedModule,
    StaticLimitKind::OpaqueCustomAssertionHelper,
    StaticLimitKind::PropertyBasedTest,
    StaticLimitKind::UnresolvedPytestFixture,
    StaticLimitKind::UnsupportedSyntax,
    StaticLimitKind::CrossLanguageOracleVisibilityUnresolved,
    StaticLimitKind::RustTransitiveReachUnresolved,
    StaticLimitKind::RustIntegrationPublicApiPathUnresolved,
    StaticLimitKind::RustMacroReachUnresolved,
    StaticLimitKind::RustMacroWrappedTestCallUnresolved,
    StaticLimitKind::RustMacroWrappedAssertionUnresolved,
    StaticLimitKind::RustValuePropagationUnresolved,
    StaticLimitKind::RustFieldMatchSelectorUnresolved,
    StaticLimitKind::RustSubprocessBinaryReachUnresolved,
    StaticLimitKind::WrapperErrorBindingUnresolved,
    StaticLimitKind::PythonTransitiveReachUnresolved,
    StaticLimitKind::RustAssertionContextUnresolved,
    StaticLimitKind::RustLogCaptureUnresolved,
    StaticLimitKind::RustPanicOutcomeUnresolved,
    StaticLimitKind::RustDebugAssertionProfileUnresolved,
    StaticLimitKind::RustCallFactBudget,
    StaticLimitKind::RustFrameworkOracleUnresolved,
    StaticLimitKind::RustPinnedShapeUnresolved,
    StaticLimitKind::RustImplicitCallUnresolved,
    StaticLimitKind::RustIndirectOwnerCallUnresolved,
    StaticLimitKind::RustCallerLoopSelectorUnresolved,
    StaticLimitKind::ProofHarnessNotEvaluated,
    StaticLimitKind::RustInherentReceiverUnestablished,
    StaticLimitKind::TypescriptWorkspaceDiscoveryTruncated,
    StaticLimitKind::TypescriptWorkspaceReadLimited,
    StaticLimitKind::RustFutureNeverPolled,
];

fn static_limit_kind_ordinal(kind: StaticLimitKind) -> usize {
    match kind {
        StaticLimitKind::DynamicDispatch => 0,
        StaticLimitKind::Metaprogramming => 1,
        StaticLimitKind::MissingImportGraph => 2,
        StaticLimitKind::DecoratorIndirection => 3,
        StaticLimitKind::MockedModule => 4,
        StaticLimitKind::OpaqueCustomAssertionHelper => 5,
        StaticLimitKind::PropertyBasedTest => 6,
        StaticLimitKind::UnresolvedPytestFixture => 7,
        StaticLimitKind::UnsupportedSyntax => 8,
        StaticLimitKind::CrossLanguageOracleVisibilityUnresolved => 9,
        StaticLimitKind::RustTransitiveReachUnresolved => 10,
        StaticLimitKind::RustIntegrationPublicApiPathUnresolved => 11,
        StaticLimitKind::RustMacroReachUnresolved => 12,
        StaticLimitKind::RustMacroWrappedTestCallUnresolved => 13,
        StaticLimitKind::RustMacroWrappedAssertionUnresolved => 14,
        StaticLimitKind::RustValuePropagationUnresolved => 15,
        StaticLimitKind::RustFieldMatchSelectorUnresolved => 16,
        StaticLimitKind::RustSubprocessBinaryReachUnresolved => 17,
        StaticLimitKind::WrapperErrorBindingUnresolved => 18,
        StaticLimitKind::PythonTransitiveReachUnresolved => 19,
        StaticLimitKind::RustAssertionContextUnresolved => 20,
        StaticLimitKind::RustLogCaptureUnresolved => 21,
        StaticLimitKind::RustPanicOutcomeUnresolved => 22,
        StaticLimitKind::RustDebugAssertionProfileUnresolved => 23,
        StaticLimitKind::RustCallFactBudget => 24,
        StaticLimitKind::RustFrameworkOracleUnresolved => 25,
        StaticLimitKind::RustPinnedShapeUnresolved => 26,
        StaticLimitKind::RustImplicitCallUnresolved => 27,
        StaticLimitKind::RustIndirectOwnerCallUnresolved => 28,
        StaticLimitKind::RustCallerLoopSelectorUnresolved => 29,
        StaticLimitKind::ProofHarnessNotEvaluated => 30,
        StaticLimitKind::RustInherentReceiverUnestablished => 31,
        StaticLimitKind::TypescriptWorkspaceDiscoveryTruncated => 32,
        StaticLimitKind::TypescriptWorkspaceReadLimited => 33,
        StaticLimitKind::RustFutureNeverPolled => 34,
    }
}

/// The largest ordinal `static_limit_kind_ordinal` assigns. Keep it equal to
/// the last arm; the test below checks the array reaches it.
const MAX_STATIC_LIMIT_KIND_ORDINAL: usize = 34;

#[test]
fn static_limit_kind_list_is_complete_and_ordered() {
    for (index, kind) in STATIC_LIMIT_KINDS.iter().enumerate() {
        assert_eq!(static_limit_kind_ordinal(*kind), index, "{kind:?}");
    }
    // Every ordinal is distinct and in 0..len by the loop above; the length
    // must also cover the largest ordinal, so a kind given a new ordinal in
    // the match but missing from the array fails here.
    assert_eq!(STATIC_LIMIT_KINDS.len(), MAX_STATIC_LIMIT_KIND_ORDINAL + 1);
    assert_eq!(
        static_limit_kind_ordinal(StaticLimitKind::RustFutureNeverPolled),
        MAX_STATIC_LIMIT_KIND_ORDINAL
    );
}

#[derive(Clone, Debug)]
struct Input {
    class: ExposureClass,
    static_limit: Option<TypeScriptStaticLimit>,
    has_owner_call_evidence: bool,
    missing_discriminators: Vec<MissingDiscriminatorFact>,
    observed_evidence_label: &'static str,
    private_member_unknown: bool,
}

impl Input {
    fn map(&self) -> TypeScriptActionability {
        typescript_actionability_for(
            &self.class,
            self.static_limit.as_ref(),
            self.has_owner_call_evidence,
            &self.missing_discriminators,
            self.observed_evidence_label,
            self.private_member_unknown,
        )
    }
}

fn discriminator() -> impl Strategy<Value = MissingDiscriminatorFact> {
    (
        "[a-z ]{0,12}",
        "[a-z ]{0,12}",
        prop::sample::select(vec![
            DiscriminatorShape::Boundary,
            DiscriminatorShape::GuardDomain,
            DiscriminatorShape::Other,
        ]),
    )
        .prop_map(|(value, reason, shape)| MissingDiscriminatorFact {
            value,
            reason,
            flow_sink: None,
            shape,
            drafted_boundary: None,
        })
}

fn static_limit() -> impl Strategy<Value = TypeScriptStaticLimit> {
    (
        prop::sample::select(STATIC_LIMIT_KINDS.to_vec()),
        prop_oneof![
            Just(String::new()),
            "[ -~]{0,24}",
            "Repair route: [ -~]{0,24}\\.",
        ],
    )
        .prop_map(|(kind, repair_route)| TypeScriptStaticLimit {
            kind,
            evidence: Vec::new(),
            missing: String::new(),
            repair_route,
        })
}

fn input() -> impl Strategy<Value = Input> {
    (
        prop::sample::select(ExposureClass::ALL.to_vec()),
        prop::option::of(static_limit()),
        any::<bool>(),
        prop::collection::vec(discriminator(), 0..4),
        prop::sample::select(vec!["related test", "", "observer"]),
        any::<bool>(),
    )
        .prop_map(
            |(
                class,
                static_limit,
                has_owner_call_evidence,
                missing_discriminators,
                observed_evidence_label,
                private_member_unknown,
            )| Input {
                class,
                static_limit,
                has_owner_call_evidence,
                missing_discriminators,
                observed_evidence_label,
                private_member_unknown,
            },
        )
}

/// The refusal category the mapper's precedence order assigns. This mirrors
/// the branch order of `typescript_actionability_for` (a characterization pin,
/// not an independent oracle) and must be revised when RIPR-SPEC-0087 adds an
/// actionable branch.
fn expected_refusal(input: &Input) -> (EvidenceState, &'static str) {
    if input.private_member_unknown {
        return (EvidenceState::Unknown, "private_member_reach_unknown");
    }
    if let Some(limit) = &input.static_limit {
        return (EvidenceState::StaticLimitation, limit.kind.as_str());
    }
    match input.class {
        ExposureClass::Exposed => (EvidenceState::AlreadyObserved, "strong_oracle_observed"),
        ExposureClass::NoStaticPath => (EvidenceState::Unknown, "missing_context"),
        _ if !input.has_owner_call_evidence => (EvidenceState::Unknown, "ambiguous_related_test"),
        _ if input.missing_discriminators.is_empty() => {
            (EvidenceState::Unknown, "missing_target_shape")
        }
        _ => (EvidenceState::Unknown, "incomplete_repair_packet"),
    }
}

proptest! {
    #![proptest_config(config())]

    /// Current behavior: no input yields an actionable state, and every
    /// refusal condition maps to its own non-actionable state and category in
    /// precedence order. Revise when RIPR-SPEC-0087 lands.
    #[test]
    fn typescript_actionability_never_actionable_and_refusals_hold(input in input()) {
        let actionability = input.map();
        let state = EvidenceState::parse(actionability.gap_state);
        prop_assert!(state.is_some(), "gap_state `{}` is outside EvidenceState", actionability.gap_state);
        prop_assert!(
            !EvidenceState::is_actionable_word(actionability.gap_state),
            "gap_state `{}` must never be actionable for {input:?}",
            actionability.gap_state
        );
        let (expected_state, expected_category) = expected_refusal(&input);
        prop_assert_eq!(state, Some(expected_state));
        prop_assert_eq!(actionability.category, expected_category);
        // An `unknown` refusal must name what is missing; it never presents
        // itself as a complete, repair-ready record.
        if expected_state == EvidenceState::Unknown {
            prop_assert!(!actionability.missing_fields.is_empty());
        }
        prop_assert!(!actionability.why_not_actionable.is_empty());
    }

    /// Removing supporting evidence (owner-call evidence, missing
    /// discriminators) never yields an actionable state and never moves an
    /// established private-member or static-limit refusal.
    #[test]
    fn typescript_actionability_refusals_survive_evidence_removal(
        input in input(),
        drop_owner_call in any::<bool>(),
        keep_discriminators in 0usize..4,
    ) {
        let before = input.map();
        let mut reduced = input.clone();
        if drop_owner_call {
            reduced.has_owner_call_evidence = false;
        }
        reduced.missing_discriminators.truncate(keep_discriminators);
        let after = reduced.map();
        prop_assert!(!EvidenceState::is_actionable_word(after.gap_state));
        if input.private_member_unknown || input.static_limit.is_some() {
            prop_assert_eq!(after.gap_state, before.gap_state);
            prop_assert_eq!(after.category, before.category);
        }
    }
}
