//! Kani proofs for the self-computed oracle strength cap.
//! Compiled only under `cfg(kani)`.

use super::self_computed_cap;
use crate::domain::OracleStrength;

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
