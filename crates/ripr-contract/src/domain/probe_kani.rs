//! Kani harnesses for probe vocabulary: source-currentness can never be
//! silently upgraded, stop reasons exist exactly for unknown classes, and
//! probe-family wire tokens round-trip. Run: `cargo kani -p ripr --harness <name>`.

use super::*;

const CURRENTNESS: [SourceCurrentness; 4] = [
    SourceCurrentness::CandidateCurrent,
    SourceCurrentness::BaseDeleted,
    SourceCurrentness::MovedOrRenamed,
    SourceCurrentness::UnresolvedSubject,
];

const FAMILIES: [ProbeFamily; 8] = [
    ProbeFamily::Predicate,
    ProbeFamily::ReturnValue,
    ProbeFamily::ErrorPath,
    ProbeFamily::CallDeletion,
    ProbeFamily::FieldConstruction,
    ProbeFamily::SideEffect,
    ProbeFamily::MatchArm,
    ProbeFamily::StaticUnknown,
];

// Wildcard-free matches: a new variant fails to compile under `cfg(kani)` until
// the symbolic domains above are extended.
fn currentness_domain_is_exhaustive(value: SourceCurrentness) {
    match value {
        SourceCurrentness::CandidateCurrent
        | SourceCurrentness::BaseDeleted
        | SourceCurrentness::MovedOrRenamed
        | SourceCurrentness::UnresolvedSubject => {}
    }
}

fn family_domain_is_exhaustive(family: &ProbeFamily) {
    match family {
        ProbeFamily::Predicate
        | ProbeFamily::ReturnValue
        | ProbeFamily::ErrorPath
        | ProbeFamily::CallDeletion
        | ProbeFamily::FieldConstruction
        | ProbeFamily::SideEffect
        | ProbeFamily::MatchArm
        | ProbeFamily::StaticUnknown => {}
    }
}

fn any_currentness() -> SourceCurrentness {
    let index: usize = kani::any();
    kani::assume(index < CURRENTNESS.len());
    let value = CURRENTNESS[index];
    currentness_domain_is_exhaustive(value);
    value
}

fn any_family() -> ProbeFamily {
    let index: usize = kani::any();
    kani::assume(index < FAMILIES.len());
    let value = FAMILIES[index].clone();
    family_domain_is_exhaustive(&value);
    value
}

/// Candidate action requires established candidate currentness; base-side and
/// the explicit unknown never permit it, and the predicates are disjoint.
#[kani::proof]
fn currentness_action_and_base_side_are_disjoint() {
    let value = any_currentness();
    assert_eq!(
        value.permits_candidate_action(),
        value == SourceCurrentness::CandidateCurrent
    );
    assert!(!(value.permits_candidate_action() && value.is_base_side()));
    if value == SourceCurrentness::UnresolvedSubject {
        assert!(!value.permits_candidate_action() && !value.is_base_side());
    }
    kani::cover!(value.is_base_side());
    kani::cover!(value.permits_candidate_action());
}

/// The delta-only resolver never fabricates base-side or unresolved-to-current
/// promotion: an `after` expression yields `CandidateCurrent`; without one the
/// result is the explicit unknown (never action-permitting).
#[kani::proof]
#[kani::unwind(2)]
fn from_probe_delta_never_upgrades_without_after() {
    let before = if kani::any() { Some("x") } else { None };
    let after = if kani::any() { Some("y") } else { None };
    let value = SourceCurrentness::from_probe_delta(before, after);
    if after.is_some() {
        assert_eq!(value, SourceCurrentness::CandidateCurrent);
    } else {
        assert_eq!(value, SourceCurrentness::UnresolvedSubject);
        assert!(!value.permits_candidate_action());
    }
    kani::cover!(before.is_some() && after.is_none());
    kani::cover!(before.is_none() && after.is_some());
}

/// A stop reason is derived exactly for the three unknown classes; every
/// known class yields none (no invented stop reason on a determined class).
#[kani::proof]
fn stop_reason_for_unknown_class_is_exactly_the_unknown_classes() {
    let index: usize = kani::any();
    kani::assume(index < ExposureClass::ALL.len());
    let class = ExposureClass::ALL[index].clone();
    let reason = StopReason::for_unknown_class(&class);
    let is_unknown = matches!(
        class,
        ExposureClass::InfectionUnknown
            | ExposureClass::PropagationUnknown
            | ExposureClass::StaticUnknown
    );
    assert_eq!(reason.is_some(), is_unknown);
    kani::cover!(reason.is_some());
    kani::cover!(reason.is_none());
}

/// `from_wire(as_str(f)) == Some(f)` for every family; stub routes are offered
/// for every family except `static_unknown`.
#[kani::proof]
#[kani::unwind(20)]
fn probe_family_wire_roundtrip() {
    let family = any_family();
    assert_eq!(
        ProbeFamily::from_wire(family.as_str()),
        Some(family.clone())
    );
    assert_eq!(
        family.offers_stub_route(),
        family != ProbeFamily::StaticUnknown
    );
    kani::cover!(family == ProbeFamily::StaticUnknown);
}
