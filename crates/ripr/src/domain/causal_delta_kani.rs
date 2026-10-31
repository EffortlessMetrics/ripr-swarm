//! Kani bounded-model-checking harnesses for `compare_fixture_delta`.
//!
//! Identity strings are fixed literals ("o"/"b"/"d"): the property under proof
//! is the decision table over `GapState` x `OracleStrength` x availability,
//! not string handling. Run: `cargo kani -p ripr --harness <name>`.

use super::*;

fn any_gap_state() -> GapState {
    match kani::any::<u8>() % 4 {
        0 => GapState::Actionable,
        1 => GapState::AlreadyObserved,
        2 => GapState::Resolved,
        _ => GapState::Unknown,
    }
}

fn any_oracle() -> OracleStrength {
    match kani::any::<u8>() % 6 {
        0 => OracleStrength::Strong,
        1 => OracleStrength::Medium,
        2 => OracleStrength::Weak,
        3 => OracleStrength::Smoke,
        4 => OracleStrength::None,
        _ => OracleStrength::Unknown,
    }
}

fn any_state(discriminator: &str) -> CanonicalEvidenceState {
    CanonicalEvidenceState {
        canonical_owner: "o".to_string(),
        behavior_identity: "b".to_string(),
        discriminator_identity: discriminator.to_string(),
        gap_state: any_gap_state(),
        oracle_strength: any_oracle(),
    }
}

fn any_opt_state(discriminator: &str) -> Option<CanonicalEvidenceState> {
    if kani::any() {
        Some(any_state(discriminator))
    } else {
        None
    }
}

/// An unavailable base or head snapshot is always `comparison_unknown` with
/// unknown confidence, whatever states the other side carries (never promotes
/// a head-only item to `introduced_by_change`).
#[kani::proof]
#[kani::unwind(5)]
fn unavailable_snapshot_is_always_unknown() {
    let base_available: bool = kani::any();
    let head_available: bool = kani::any();
    kani::assume(!base_available || !head_available);
    let delta = compare_fixture_delta(
        "g",
        base_available,
        head_available,
        any_opt_state("d"),
        any_opt_state("d"),
    );
    assert_eq!(delta.delta_attribution, DeltaAttribution::ComparisonUnknown);
    assert_eq!(delta.comparison_confidence, ComparisonConfidence::Unknown);
    assert!(!delta.attribution_basis.is_empty());
    kani::cover!(!base_available && !head_available);
    kani::cover!(!base_available && head_available);
    kani::cover!(base_available && !head_available);
}

/// With both snapshots available and the same identity, the attribution
/// follows the documented decision table: Resolved only through a resolving
/// gap-state transition or a strict oracle-rank increase (never when the
/// transition is a reintroduction); Weakened only on a strict rank decrease;
/// ChangedSurfaceExisting only when nothing moved.
#[kani::proof]
#[kani::unwind(5)]
fn same_identity_decision_table() {
    let base = any_state("d");
    let head = any_state("d");
    let delta = compare_fixture_delta("g", true, true, Some(base.clone()), Some(head.clone()));
    let (br, hr) = (base.oracle_strength.rank(), head.oracle_strength.rank());
    let base_res = matches!(
        base.gap_state,
        GapState::Resolved | GapState::AlreadyObserved
    );
    let head_res = matches!(
        head.gap_state,
        GapState::Resolved | GapState::AlreadyObserved
    );
    match delta.delta_attribution {
        DeltaAttribution::ReintroducedByChange => {
            assert!(base_res && head.gap_state == GapState::Actionable);
        }
        DeltaAttribution::ResolvedByChange => {
            assert!((head_res && !base_res) || hr > br);
            assert!(!(base_res && head.gap_state == GapState::Actionable));
        }
        DeltaAttribution::WeakenedByChange => {
            assert!(hr < br);
            assert!(!(head_res && !base_res));
        }
        DeltaAttribution::ChangedSurfaceExisting => {
            assert!(hr == br);
            assert!(base.gap_state == head.gap_state || (base_res && head_res));
        }
        DeltaAttribution::ComparisonUnknown => {
            assert_eq!(delta.comparison_confidence, ComparisonConfidence::Low);
            assert!(hr == br);
        }
        _ => {
            assert!(false, "unreachable attribution");
        }
    }
    kani::cover!(delta.delta_attribution == DeltaAttribution::ReintroducedByChange);
    kani::cover!(delta.delta_attribution == DeltaAttribution::WeakenedByChange);
    kani::cover!(delta.delta_attribution == DeltaAttribution::ResolvedByChange);
    kani::cover!(delta.delta_attribution == DeltaAttribution::ChangedSurfaceExisting);
    kani::cover!(delta.delta_attribution == DeltaAttribution::ComparisonUnknown);
}

/// A differing identity is never attributed causally, even if the lines would
/// be adjacent.
#[kani::proof]
#[kani::unwind(5)]
fn identity_mismatch_is_unknown() {
    let base = any_state("d");
    let head = any_state("other");
    let delta = compare_fixture_delta("g", true, true, Some(base), Some(head));
    assert_eq!(delta.delta_attribution, DeltaAttribution::ComparisonUnknown);
    assert_eq!(delta.comparison_confidence, ComparisonConfidence::Unknown);
    kani::cover!(delta.head_state.is_some());
}

/// Totality plus the global invariant over every availability/presence
/// combination: `comparison_unknown` is never `FixtureBacked`, and
/// introduced-by-change requires complete snapshots with an absent base.
#[kani::proof]
#[kani::unwind(5)]
fn compare_is_total_and_unknown_is_never_fixture_backed() {
    let base_available: bool = kani::any();
    let head_available: bool = kani::any();
    let delta = compare_fixture_delta(
        "g",
        base_available,
        head_available,
        any_opt_state("d"),
        any_opt_state("d"),
    );
    if delta.delta_attribution == DeltaAttribution::ComparisonUnknown {
        assert!(delta.comparison_confidence != ComparisonConfidence::FixtureBacked);
    }
    if delta.delta_attribution == DeltaAttribution::IntroducedByChange {
        assert!(base_available && head_available);
        assert!(delta.base_state.is_none() && delta.head_state.is_some());
    }
    kani::cover!(delta.delta_attribution == DeltaAttribution::IntroducedByChange);
    kani::cover!(delta.delta_attribution == DeltaAttribution::ResolvedByChange);
}

/// `GapState::from(&str)` round-trips every variant's wire label, and an
/// unrecognised label is `Unknown` (opaque input is never coerced to an
/// actionable or resolved state).
#[kani::proof]
#[kani::unwind(25)]
fn gap_state_parse_roundtrip_and_unknown_fallback() {
    let state = any_gap_state();
    assert_eq!(GapState::from(state.as_str()), state);
    assert_eq!(GapState::from(""), GapState::Unknown);
    assert_eq!(GapState::from("Actionable"), GapState::Unknown);
    assert_eq!(GapState::from("resolved "), GapState::Unknown);
    assert_eq!(GapState::from("other"), GapState::Unknown);
    kani::cover!(state == GapState::Actionable);
}
