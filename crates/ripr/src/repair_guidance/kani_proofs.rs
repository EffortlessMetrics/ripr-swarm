//! Kani bounded-model-checking proofs for the repair-guidance contract's
//! closed state/reason/recovery tables. Compiled only under `cargo kani`;
//! normal builds never see this file.
//!
//! The view validators (`AssertionGuidanceView::validate`,
//! `DiscriminatorGuidanceView::validate`, `RepairGuidance::from_views`) were
//! also attempted and are NOT covered: their `format!("{:?}")` error paths
//! and `String` trimming exhaust CBMC memory (the assertion-view harness ran
//! 30 minutes and then ran out of memory), so they remain guarded by the
//! unit tests only. This file proves the pure data tables the validators
//! consult.

use super::*;

fn any_reason() -> GuidanceReason {
    let index: u8 = kani::any();
    kani::assume(index < 8);
    match index {
        0 => GuidanceReason::ProducerFactAbsent,
        1 => GuidanceReason::NoBehavioralDiscriminatorDerived,
        2 => GuidanceReason::ObserverNotStaticallyVisible,
        3 => GuidanceReason::RouteIsInspectionOnly,
        4 => GuidanceReason::RouteIsVerificationOnly,
        5 => GuidanceReason::StaticLimitationBlocksDerivation,
        6 => GuidanceReason::SnapshotStale,
        _ => GuidanceReason::CrossLanguageOracleUnresolved,
    }
}

fn any_recovery() -> GuidanceRecovery {
    let index: u8 = kani::any();
    kani::assume(index < 6);
    match index {
        0 => GuidanceRecovery::InspectFixSite,
        1 => GuidanceRecovery::RunExplain,
        2 => GuidanceRecovery::AddObserverThenAssert,
        3 => GuidanceRecovery::RefreshAnalysis,
        4 => GuidanceRecovery::ReviewExternalOracle,
        _ => GuidanceRecovery::NoRecoveryAvailable,
    }
}

fn any_assertion_state() -> AssertionState {
    let index: u8 = kani::any();
    kani::assume(index < 6);
    match index {
        0 => AssertionState::Concrete,
        1 => AssertionState::RequiresObserverSetup,
        2 => AssertionState::FixSiteOnly,
        3 => AssertionState::VerificationOnly,
        4 => AssertionState::Unresolved,
        _ => AssertionState::Stale,
    }
}

fn any_discriminator_state() -> DiscriminatorState {
    let index: u8 = kani::any();
    kani::assume(index < 5);
    match index {
        0 => DiscriminatorState::Present,
        1 => DiscriminatorState::NotProduced,
        2 => DiscriminatorState::NotApplicable,
        3 => DiscriminatorState::StaticLimitation,
        _ => DiscriminatorState::Stale,
    }
}

/// The closed reason/recovery tables keep staleness exclusive, for every
/// state and every reason/recovery: the stale-snapshot reason and the refresh
/// recovery are admitted by a stale state and by no other state; a concrete
/// (assertion) or present (discriminator) state admits no reason and no
/// recovery at all; and a stale state admits nothing but the stale-snapshot
/// reason and the refresh / no-recovery routes. A stale input can therefore
/// never be relabeled with a different explanation or routed to a fix site.
/// These tables are pure data, so this proof is exhaustive and cheap.
#[kani::proof]
#[kani::unwind(12)]
fn reason_and_recovery_tables_keep_staleness_exclusive() {
    let reason = any_reason();
    let recovery = any_recovery();
    let assertion = any_assertion_state();
    let discriminator = any_discriminator_state();

    let assertion_stale = assertion == AssertionState::Stale;
    let discriminator_stale = discriminator == DiscriminatorState::Stale;

    // The stale reason is admitted exactly by stale states.
    assert_eq!(
        allowed_assertion_reasons(assertion).contains(&GuidanceReason::SnapshotStale),
        assertion_stale
    );
    assert_eq!(
        allowed_discriminator_reasons(discriminator).contains(&GuidanceReason::SnapshotStale),
        discriminator_stale
    );
    // A stale state admits no other reason.
    if assertion_stale && allowed_assertion_reasons(assertion).contains(&reason) {
        assert!(reason == GuidanceReason::SnapshotStale);
    }
    if discriminator_stale && allowed_discriminator_reasons(discriminator).contains(&reason) {
        assert!(reason == GuidanceReason::SnapshotStale);
    }
    // Recoveries of a stale state are refresh-only; refresh is stale-only.
    if assertion_stale && allowed_assertion_recoveries(assertion).contains(&recovery) {
        assert!(
            recovery == GuidanceRecovery::RefreshAnalysis
                || recovery == GuidanceRecovery::NoRecoveryAvailable
        );
    }
    if discriminator_stale && allowed_discriminator_recoveries(discriminator).contains(&recovery) {
        assert!(
            recovery == GuidanceRecovery::RefreshAnalysis
                || recovery == GuidanceRecovery::NoRecoveryAvailable
        );
    }
    if allowed_assertion_recoveries(assertion).contains(&GuidanceRecovery::RefreshAnalysis) {
        assert!(assertion_stale);
    }
    if allowed_discriminator_recoveries(discriminator).contains(&GuidanceRecovery::RefreshAnalysis)
    {
        assert!(discriminator_stale);
    }
    // Concrete / present guidance carries neither reason nor recovery.
    if assertion == AssertionState::Concrete {
        assert!(allowed_assertion_reasons(assertion).is_empty());
        assert!(allowed_assertion_recoveries(assertion).is_empty());
    }
    if discriminator == DiscriminatorState::Present {
        assert!(allowed_discriminator_reasons(discriminator).is_empty());
        assert!(allowed_discriminator_recoveries(discriminator).is_empty());
    }
    kani::cover!(assertion_stale && allowed_assertion_reasons(assertion).contains(&reason));
    kani::cover!(!assertion_stale && allowed_assertion_reasons(assertion).contains(&reason));
    kani::cover!(
        discriminator_stale && allowed_discriminator_recoveries(discriminator).contains(&recovery)
    );
}
