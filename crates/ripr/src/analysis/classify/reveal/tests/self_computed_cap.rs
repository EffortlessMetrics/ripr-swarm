//! RIPR-SPEC-0035: production caller and detector controls, separate from the
//! scalar proof in RIPR-SPEC-0371.

use super::super::cap_contract::{STRENGTHS, expected_cap};
use super::*;

#[test]
fn self_computed_reveal_uses_exact_cap_for_credited_and_uncredited_rows() {
    let probe = owned_probe(ProbeFamily::ReturnValue, "subtotal * RATE", "tax");
    for (text, self_computed) in [
        ("assert_eq!(tax(RATE), tax(RATE));", true),
        ("assert_eq!(tax(RATE), 24);", false),
    ] {
        // The test supplies assertion text, not a self-computed flag, to the
        // production reveal path. Pin the separate detector's reading too.
        assert_eq!(
            expected_computed_through_owner(text, "tax", true, &|_, _| false),
            self_computed,
        );
        for reason in [
            RelationReason::DirectOwnerCall,
            RelationReason::WeakTokenSubstring,
        ] {
            for strength in STRENGTHS {
                // ExactValue family normalization replaces None/Unknown
                // before the cap. The scalar contract separately checks
                // their preservation; this fixture covers the four retained
                // ExactValue strengths reaching both production cap calls.
                if matches!(strength, OracleStrength::None | OracleStrength::Unknown) {
                    continue;
                }
                let assertion = oracle(text, OracleKind::ExactValue, strength.clone());
                assert_eq!(
                    probe_relative_oracle_strength(&probe.family, &assertion),
                    *strength,
                    "fixture normalization must preserve the intended cap input",
                );
                let test = test_with_body_assertions("cap_input", text, vec![assertion]);
                let (observe, discriminate, related) = reveal_evidence(&probe, &[(&test, reason)]);
                assert_eq!(related.len(), 1, "the intended assertion must be retained");
                let row = &related[0];
                assert_eq!(row.name, "cap_input");
                assert_eq!(row.oracle.as_deref(), Some(text));
                assert_eq!(row.oracle_strength, expected_cap(strength, self_computed));
                if reason == RelationReason::WeakTokenSubstring {
                    assert_eq!(row.miss, Some(RelatedTestMiss::NoCallPath));
                    assert_ne!(observe.state, StageState::Yes);
                    assert_ne!(discriminate.state, StageState::Yes);
                } else if self_computed {
                    // The broad observation stage records a nearby matching
                    // assertion. Confirmation of the changed sink belongs to
                    // discrimination and must stay withheld here.
                    assert_eq!(observe.state, StageState::Yes);
                    assert_ne!(discriminate.state, StageState::Yes);
                } else if *strength == OracleStrength::Strong {
                    assert_eq!(observe.state, StageState::Yes);
                    assert_eq!(discriminate.state, StageState::Yes);
                }
            }
        }
    }
}

#[test]
fn self_computed_detection_does_not_confuse_text_or_independent_values_with_owner_calls() {
    for (text, expected) in [
        ("assert_eq!(tax(100), tax(100));", true),
        ("assert_eq!(crate::tax(100), tax(100));", true),
        ("assert_eq!(tax(100), 8);", false),
        ("assert_eq!(tax(100), other(100));", false),
        ("assert_eq!(tax(100), 8, \"tax(100)\");", false),
        ("assert_eq!(tax(100) * 2, tax(100) + 8);", false),
    ] {
        assert_eq!(
            expected_computed_through_owner(text, "tax", true, &|_, _| false),
            expected,
            "{text}",
        );
    }
}
