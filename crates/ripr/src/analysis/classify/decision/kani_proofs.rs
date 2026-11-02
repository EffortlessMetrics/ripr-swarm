//! Kani proofs for the exposure-class decision (`classify`).
//!
//! Compiled only under `cfg(kani)`. Stage evidence is fully symbolic in
//! `state`; `confidence` is irrelevant to `classify` and fixed, and the
//! free-text `summary` is empty (classify never reads it). Probe fields other
//! than `family` are fixed placeholders (classify reads only `family`).

use super::{classify, ensure_unknown_stop_reason};
use crate::domain::*;
use std::path::PathBuf;

fn any_state() -> StageState {
    let tag: u8 = kani::any();
    kani::assume(tag < 6);
    match tag {
        0 => StageState::Yes,
        1 => StageState::Weak,
        2 => StageState::No,
        3 => StageState::Unknown,
        4 => StageState::Opaque,
        _ => StageState::NotApplicable,
    }
}

fn stage() -> StageEvidence {
    stage_with(any_state())
}

fn stage_with(state: StageState) -> StageEvidence {
    StageEvidence {
        state,
        confidence: Confidence::Unknown,
        summary: String::new(),
    }
}

fn probe(static_unknown: bool) -> Probe {
    Probe {
        id: ProbeId(String::new()),
        location: SourceLocation {
            file: PathBuf::new(),
            line: 0,
            column: 0,
        },
        owner: None,
        family: if static_unknown {
            ProbeFamily::StaticUnknown
        } else {
            ProbeFamily::Predicate
        },
        delta: DeltaKind::Unknown,
        before: None,
        after: None,
        expression: String::new(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    }
}

fn is_unknown_like(s: &StageState) -> bool {
    matches!(s, StageState::Unknown | StageState::Opaque)
}

/// Totality: every combination of stage states and probe family maps to one
/// of the seven vocabulary classes (a documented wire word, a member of
/// `ExposureClass::ALL`), with no panic and no eighth outcome.
#[kani::proof]
#[kani::unwind(24)]
fn classify_is_total_over_the_seven_class_vocabulary() {
    let (r, i, p, o, d) = (stage(), stage(), stage(), stage(), stage());
    let su: bool = kani::any();
    let class = classify(&r, &i, &p, &o, &d, &probe(su));
    kani::cover!(class == ExposureClass::Exposed);
    kani::cover!(class == ExposureClass::WeaklyExposed);
    kani::cover!(class == ExposureClass::ReachableUnrevealed);
    kani::cover!(class == ExposureClass::NoStaticPath);
    kani::cover!(class == ExposureClass::InfectionUnknown);
    kani::cover!(class == ExposureClass::PropagationUnknown);
    kani::cover!(class == ExposureClass::StaticUnknown);
    let word = class.as_str();
    assert!(
        word == "exposed"
            || word == "weakly_exposed"
            || word == "reachable_unrevealed"
            || word == "no_static_path"
            || word == "infection_unknown"
            || word == "propagation_unknown"
            || word == "static_unknown"
    );
    assert!(ExposureClass::ALL.contains(&class));
}

/// Unknowns never promote: if reach is not Yes, or infect/propagate is
/// unknown/opaque/no, the class is never `exposed`. A `static_unknown` probe
/// yields exactly `static_unknown` whatever the stage evidence says.
#[kani::proof]
#[kani::unwind(2)]
fn unknown_or_missing_stage_never_promotes_to_exposed() {
    let (r, i, p, o, d) = (stage(), stage(), stage(), stage(), stage());
    let su: bool = kani::any();
    let weak_link = r.state != StageState::Yes
        || is_unknown_like(&i.state)
        || is_unknown_like(&p.state)
        || i.state == StageState::No
        || p.state == StageState::No;
    kani::assume(weak_link || su);
    kani::cover!(is_unknown_like(&i.state) && !su, "unknown infect");
    kani::cover!(
        is_unknown_like(&p.state) && !su && r.state != StageState::No,
        "unknown propagate"
    );
    kani::cover!(su, "static unknown probe");
    let class = classify(&r, &i, &p, &o, &d, &probe(su));
    assert!(class != ExposureClass::Exposed);
    if su {
        assert!(class == ExposureClass::StaticUnknown);
    }
}

/// Precise characterisation of `exposed`: produced exactly when the probe is
/// not static_unknown, reach/infect/propagate/discriminate are all Yes and
/// observe is not `No`.
#[kani::proof]
#[kani::unwind(2)]
fn exposed_iff_all_gating_stages_yes() {
    let (r, i, p, o, d) = (stage(), stage(), stage(), stage(), stage());
    let su: bool = kani::any();
    let class = classify(&r, &i, &p, &o, &d, &probe(su));
    let expect = !su
        && r.state == StageState::Yes
        && i.state == StageState::Yes
        && p.state == StageState::Yes
        && d.state == StageState::Yes
        && o.state != StageState::No;
    kani::cover!(expect, "exposed path");
    kani::cover!(
        !expect && class == ExposureClass::WeaklyExposed,
        "weak path"
    );
    assert!((class == ExposureClass::Exposed) == expect);
}

/// Unknown/opaque oracle stages: with reach/infect/propagate Yes and
/// discriminate not Yes, an unknown/opaque observe or discriminate stage
/// cannot yield `exposed`.
#[kani::proof]
#[kani::unwind(2)]
fn unresolved_discriminate_never_exposed() {
    let (o, d) = (stage(), stage());
    kani::assume(is_unknown_like(&o.state) || is_unknown_like(&d.state));
    kani::assume(d.state != StageState::Yes);
    let yes = || stage_with(StageState::Yes);
    let class = classify(&yes(), &yes(), &yes(), &o, &d, &probe(false));
    kani::cover!(is_unknown_like(&d.state));
    kani::cover!(is_unknown_like(&o.state));
    assert!(class != ExposureClass::Exposed);
}

/// Monotonicity along the oracle dimensions: with reach/infect/propagate at
/// Yes the classes form the chain ReachableUnrevealed < WeaklyExposed <
/// Exposed. Improving observe (from No) or improving discriminate to Yes
/// never lowers the class, and results stay inside that chain.
#[kani::proof]
#[kani::unwind(2)]
fn stronger_oracle_evidence_never_lowers_the_class() {
    fn rank(c: &ExposureClass) -> u8 {
        match c {
            ExposureClass::ReachableUnrevealed => 0,
            ExposureClass::WeaklyExposed => 1,
            ExposureClass::Exposed => 2,
            // Unreachable when reach/infect/propagate are Yes.
            _ => 255,
        }
    }
    let yes = || stage_with(StageState::Yes);
    let (o1, d1) = (stage(), stage());
    let (o2, d2) = (stage(), stage());
    kani::assume(o1.state == o2.state || o1.state == StageState::No);
    kani::assume(d1.state == d2.state || d2.state == StageState::Yes);
    let p0 = probe(false);
    let c1 = classify(&yes(), &yes(), &yes(), &o1, &d1, &p0);
    let c2 = classify(&yes(), &yes(), &yes(), &o2, &d2, &p0);
    kani::cover!(rank(&c2) > rank(&c1), "strict improvement");
    kani::cover!(
        rank(&c1) == 0 && o2.state != StageState::No,
        "unrevealed repaired"
    );
    assert!(rank(&c1) != 255 && rank(&c2) != 255);
    assert!(rank(&c2) >= rank(&c1));
}

/// Cross-owner consistency: a class requires a stop reason exactly when a
/// stop reason exists for it, and `ensure_unknown_stop_reason` supplies one
/// for every unknown class `classify` can emit (and none otherwise).
#[kani::proof]
#[kani::unwind(3)]
fn unknown_classes_always_receive_a_stop_reason() {
    let (r, i, p, o, d) = (stage(), stage(), stage(), stage(), stage());
    let su: bool = kani::any();
    let class = classify(&r, &i, &p, &o, &d, &probe(su));
    assert!(class.requires_stop_reason() == StopReason::for_unknown_class(&class).is_some());
    let mut reasons: Vec<StopReason> = Vec::new();
    ensure_unknown_stop_reason(&class, &mut reasons);
    kani::cover!(class.requires_stop_reason(), "unknown class");
    kani::cover!(!class.requires_stop_reason(), "known class");
    assert!(reasons.is_empty() != class.requires_stop_reason());
}
