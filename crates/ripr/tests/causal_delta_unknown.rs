//! Public comparator regressions for unknown input evidence (#12714).
//! These ordinary Rust tests do not execute the separate Kani harnesses.

use ripr::domain::{
    AttributionBasis, CanonicalEvidenceState, ComparisonConfidence, DeltaAttribution, GapState,
    OracleStrength, compare_fixture_delta,
};

fn state(gap: GapState, oracle: OracleStrength) -> CanonicalEvidenceState {
    CanonicalEvidenceState::new("owner", "behavior", "discriminator", gap, oracle)
}

#[test]
fn unknown_gap_inputs_remain_non_causal() {
    let gaps = [
        GapState::Actionable,
        GapState::AlreadyObserved,
        GapState::Resolved,
        GapState::Unknown,
    ];
    let oracles = [
        OracleStrength::Strong,
        OracleStrength::Medium,
        OracleStrength::Weak,
        OracleStrength::Smoke,
        OracleStrength::None,
        OracleStrength::Unknown,
    ];
    let mut inputs = vec![None];
    for gap in gaps {
        for oracle in &oracles {
            inputs.push(Some(state(gap, oracle.clone())));
        }
    }
    assert_eq!(inputs.len(), 25);
    let mut checked = 0;
    let mut base_only = 0;
    let mut head_only = 0;
    for base in &inputs {
        for head in &inputs {
            let unknown = base
                .as_ref()
                .is_some_and(|value| value.gap_state == GapState::Unknown)
                || head
                    .as_ref()
                    .is_some_and(|value| value.gap_state == GapState::Unknown);
            if !unknown {
                continue;
            }
            for base_available in [false, true] {
                for head_available in [false, true] {
                    checked += 1;
                    base_only += usize::from(head.is_none());
                    head_only += usize::from(base.is_none());
                    let delta = compare_fixture_delta(
                        "gap:unknown-input",
                        base_available,
                        head_available,
                        base.clone(),
                        head.clone(),
                    );
                    assert_eq!(delta.delta_attribution, DeltaAttribution::ComparisonUnknown);
                    assert_eq!(delta.comparison_confidence, ComparisonConfidence::Unknown);
                    assert_eq!(delta.canonical_gap_id, "gap:unknown-input");
                    assert_eq!(delta.base_state.as_ref(), base.as_ref());
                    assert_eq!(delta.head_state.as_ref(), head.as_ref());
                    assert!(!delta.attribution_basis.is_empty());
                    for unsupported in [
                        AttributionBasis::OracleStrengthIncreased,
                        AttributionBasis::OracleStrengthDecreased,
                        AttributionBasis::UnchangedEvidence,
                        AttributionBasis::BaselineReceipt,
                    ] {
                        assert!(!delta.attribution_basis.contains(&unsupported));
                    }
                }
            }
        }
    }
    // 25^2 - 19^2 option pairs contain unknown evidence, across four
    // availability combinations. Fixed counts reject an empty/partial loop.
    assert_eq!(checked, 1_056);
    assert_eq!(base_only, 24);
    assert_eq!(head_only, 24);
}

#[test]
fn known_gap_inputs_keep_supported_transitions() {
    let cases = [
        (
            None,
            Some(state(GapState::Actionable, OracleStrength::Weak)),
            DeltaAttribution::IntroducedByChange,
        ),
        (
            Some(state(GapState::Actionable, OracleStrength::Weak)),
            None,
            DeltaAttribution::ResolvedByChange,
        ),
        (
            Some(state(GapState::Actionable, OracleStrength::Strong)),
            Some(state(GapState::Actionable, OracleStrength::Weak)),
            DeltaAttribution::WeakenedByChange,
        ),
        (
            Some(state(GapState::Resolved, OracleStrength::Strong)),
            Some(state(GapState::Actionable, OracleStrength::Weak)),
            DeltaAttribution::ReintroducedByChange,
        ),
        (
            Some(state(GapState::Actionable, OracleStrength::Strong)),
            Some(state(GapState::Resolved, OracleStrength::Weak)),
            DeltaAttribution::ResolvedByChange,
        ),
        (
            Some(state(GapState::Actionable, OracleStrength::Weak)),
            Some(state(GapState::Actionable, OracleStrength::Strong)),
            DeltaAttribution::ResolvedByChange,
        ),
        (
            Some(state(GapState::Resolved, OracleStrength::Weak)),
            Some(state(GapState::AlreadyObserved, OracleStrength::Weak)),
            DeltaAttribution::ChangedSurfaceExisting,
        ),
        (
            Some(state(GapState::Actionable, OracleStrength::Unknown)),
            Some(state(GapState::Resolved, OracleStrength::Unknown)),
            DeltaAttribution::ResolvedByChange,
        ),
    ];
    assert_eq!(cases.len(), 8);
    for (base, head, expected) in cases {
        let delta = compare_fixture_delta("gap:known", true, true, base.clone(), head.clone());
        assert_eq!(delta.delta_attribution, expected);
        assert_eq!(
            delta.comparison_confidence,
            ComparisonConfidence::FixtureBacked
        );
        assert_eq!(delta.base_state, base);
        assert_eq!(delta.head_state, head);
    }
}

#[test]
fn unknown_gap_retains_truthful_bases_and_json() -> Result<(), serde_json::Error> {
    let base = state(GapState::from("unsupported"), OracleStrength::Weak);
    let mut head = state(GapState::Unknown, OracleStrength::Strong);
    let delta = compare_fixture_delta(
        "gap:unknown",
        true,
        true,
        Some(base.clone()),
        Some(head.clone()),
    );
    assert_eq!(
        delta.attribution_basis,
        vec![
            AttributionBasis::SameCanonicalOwner,
            AttributionBasis::SameBehaviorIdentity,
            AttributionBasis::SameDiscriminator,
        ]
    );
    let value = serde_json::to_value(&delta)?;
    assert_eq!(value["delta_attribution"], "comparison_unknown");
    assert_eq!(value["comparison_confidence"], "unknown");
    assert_eq!(value["base_state"]["gap_state"], "unknown");
    assert_eq!(value["head_state"]["gap_state"], "unknown");

    head.canonical_owner = "other-owner".to_string();
    let mismatch = compare_fixture_delta("gap:unknown", true, true, Some(base.clone()), Some(head));
    assert_eq!(
        mismatch.delta_attribution,
        DeltaAttribution::ComparisonUnknown
    );
    assert_eq!(
        mismatch.attribution_basis,
        vec![AttributionBasis::IdentityAmbiguous]
    );

    let unavailable = compare_fixture_delta("gap:unknown", true, false, Some(base.clone()), None);
    assert_eq!(
        unavailable.attribution_basis,
        vec![AttributionBasis::HeadSnapshotUnavailable]
    );
    let removed = compare_fixture_delta("gap:unknown", true, true, Some(base), None);
    assert_eq!(
        removed.delta_attribution,
        DeltaAttribution::ComparisonUnknown
    );
    assert_eq!(
        removed.attribution_basis,
        vec![AttributionBasis::HeadItemAbsent]
    );
    Ok(())
}
