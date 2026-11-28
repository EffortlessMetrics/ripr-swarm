//! Kani proofs for the self-computed oracle strength cap.
//! Compiled only under `cfg(kani)`. These scalar proofs call production code;
//! they do not prove the source detector or the surrounding reveal pipeline.
//! The cap and constructor selection have no loops or recursion.

use super::cap_contract::{any_strength, cover_inputs, expected_cap};
use super::self_computed_cap;
use crate::domain::OracleStrength;

/// Every strength/Boolean pair has its exact independently specified result.
/// In particular, dropping every self-computed strength to `None` is wrong even
/// though it satisfies the two rank properties below. Every input is covered.
#[kani::proof]
fn self_computed_cap_matches_exact_mapping() {
    let strength = any_strength();
    let self_computed: bool = kani::any();
    cover_inputs(&strength, self_computed);
    let expected = expected_cap(&strength, self_computed);
    assert!(self_computed_cap(strength, self_computed) == expected);
}

/// The cap never raises a strength; a self-computed assertion ends at most
/// `weak` (or keeps an already lower strength); otherwise it is untouched.
#[kani::proof]
fn self_computed_cap_never_raises_and_bounds_at_weak() {
    let s = any_strength();
    let sc: bool = kani::any();
    let before = s.rank();
    let weak = OracleStrength::Weak.rank();
    let capped = self_computed_cap(s, sc);
    kani::cover!(sc && before > weak, "cap engages");
    kani::cover!(sc && before <= weak, "cap leaves lower strength");
    kani::cover!(!sc, "not self computed");
    assert!(capped.rank() <= before);
    if sc {
        assert!(capped.rank() <= weak);
    } else {
        assert!(capped.rank() == before);
    }
}

/// The cap is monotone: a stronger oracle never ends up weaker after capping.
#[kani::proof]
fn self_computed_cap_is_monotone() {
    let a = any_strength();
    let b = any_strength();
    let sc: bool = kani::any();
    kani::assume(a.rank() <= b.rank());
    kani::cover!(a.rank() < b.rank(), "strictly ordered pair");
    let ca = self_computed_cap(a, sc);
    let cb = self_computed_cap(b, sc);
    assert!(ca.rank() <= cb.rank());
}
