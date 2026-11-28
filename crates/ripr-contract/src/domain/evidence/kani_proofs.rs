//! Kani bounded-model-checking proofs for oracle strength ordering.
//!
//! Compiled only under `cfg(kani)`; normal builds never see this module.
//! Run with `cargo kani -p ripr --harness <name>`.

use super::OracleStrength;

/// Symbolic `OracleStrength` covering every variant (6 values).
fn any_strength() -> OracleStrength {
    let tag: u8 = kani::any();
    kani::assume(tag < 6);
    match tag {
        0 => OracleStrength::Strong,
        1 => OracleStrength::Medium,
        2 => OracleStrength::Weak,
        3 => OracleStrength::Smoke,
        4 => OracleStrength::None,
        _ => OracleStrength::Unknown,
    }
}

/// `rank` is injective: two strengths share a rank only when they are equal,
/// so every strength has a distinct place in the ordering.
#[kani::proof]
fn oracle_strength_rank_is_injective() {
    let a = any_strength();
    let b = any_strength();
    kani::cover!(a != b, "distinct strengths compared");
    assert!((a.rank() == b.rank()) == (a == b));
}

/// The ordering is the documented chain
/// Strong > Medium > Weak > Smoke > Unknown > None, and `Unknown` never
/// outranks a recognised smoke-or-better oracle.
#[kani::proof]
fn oracle_strength_rank_is_the_documented_chain() {
    assert!(OracleStrength::Strong.rank() > OracleStrength::Medium.rank());
    assert!(OracleStrength::Medium.rank() > OracleStrength::Weak.rank());
    assert!(OracleStrength::Weak.rank() > OracleStrength::Smoke.rank());
    assert!(OracleStrength::Smoke.rank() > OracleStrength::Unknown.rank());
    assert!(OracleStrength::Unknown.rank() > OracleStrength::None.rank());
    let s = any_strength();
    kani::cover!(s == OracleStrength::Unknown);
    if s == OracleStrength::Unknown {
        assert!(s.rank() < OracleStrength::Smoke.rank());
    }
}

/// The wire word identifies the strength: equal words imply equal strengths.
#[kani::proof]
#[kani::unwind(24)]
fn oracle_strength_wire_word_identifies_strength() {
    let a = any_strength();
    let b = any_strength();
    kani::cover!(a == b);
    kani::cover!(a != b);
    if a.as_str() == b.as_str() {
        assert!(a == b);
    }
}
