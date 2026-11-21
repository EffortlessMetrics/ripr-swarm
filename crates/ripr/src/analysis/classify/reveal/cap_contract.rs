//! Independent, closed contract for the self-computed oracle cap (#12109).
//!
//! Ordinary tests and Kani use the same constructor declaration. Its exact
//! expected-value match has no wildcard and rejects duplicate patterns, so a
//! missing row or a new `OracleStrength` variant is a compile error in both.
//! Expected values are literal variants, never derived from production `rank`
//! or `self_computed_cap`. Detection of self-computation is a separate claim.

use crate::domain::OracleStrength;

macro_rules! exact_cap_contract {
    ($($input:path => $capped:path),+ $(,)?) => {
        pub(super) const STRENGTHS: &[OracleStrength] = &[$($input),+];

        #[deny(unreachable_patterns)]
        pub(super) fn expected_cap(
            strength: &OracleStrength,
            self_computed: bool,
        ) -> OracleStrength {
            match (strength, self_computed) {
                $(($input, false) => $input, ($input, true) => $capped,)+
            }
        }

        #[cfg(kani)]
        pub(super) fn cover_inputs(strength: &OracleStrength, self_computed: bool) {
            $(
                kani::cover!(*strength == $input && !self_computed);
                kani::cover!(*strength == $input && self_computed);
            )+
        }
    };
}

exact_cap_contract! {
    OracleStrength::Strong => OracleStrength::Weak,
    OracleStrength::Medium => OracleStrength::Weak,
    OracleStrength::Weak => OracleStrength::Weak,
    OracleStrength::Smoke => OracleStrength::Smoke,
    OracleStrength::None => OracleStrength::None,
    OracleStrength::Unknown => OracleStrength::Unknown,
}

#[cfg(kani)]
pub(super) fn any_strength() -> OracleStrength {
    let index: usize = kani::any();
    // Every constructor in the compiler-checked declaration is admitted.
    kani::assume(index < STRENGTHS.len());
    STRENGTHS[index].clone()
}

#[cfg(test)]
mod tests {
    use super::super::self_computed_cap;
    use super::*;

    #[test]
    fn self_computed_cap_matches_every_literal_case() {
        for self_computed in [false, true] {
            for strength in STRENGTHS {
                assert_eq!(
                    self_computed_cap(strength.clone(), self_computed),
                    expected_cap(strength, self_computed),
                    "strength={strength:?}, self_computed={self_computed}",
                );
            }
        }
    }

    /// This wrong implementation is admitted by the original upper-bound and
    /// monotonicity properties. The exact contract must distinguish it.
    #[test]
    fn self_computed_none_mutant_passes_rank_laws_but_fails_exact_contract() {
        let wrong_cap = |strength: OracleStrength, self_computed| {
            if self_computed {
                OracleStrength::None
            } else {
                strength
            }
        };
        for self_computed in [false, true] {
            for strength in STRENGTHS {
                let capped = wrong_cap(strength.clone(), self_computed);
                assert!(capped.rank() <= strength.rank());
                if self_computed {
                    assert!(capped.rank() <= OracleStrength::Weak.rank());
                } else {
                    assert_eq!(capped.rank(), strength.rank());
                }
                for other in STRENGTHS {
                    if strength.rank() <= other.rank() {
                        assert!(capped.rank() <= wrong_cap(other.clone(), self_computed).rank());
                    }
                }
            }
        }

        assert_eq!(
            expected_cap(&OracleStrength::Strong, true),
            OracleStrength::Weak
        );
        assert_ne!(
            wrong_cap(OracleStrength::Strong, true),
            expected_cap(&OracleStrength::Strong, true),
        );
    }
}
