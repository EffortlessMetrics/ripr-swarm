//! Case-sensitive discriminator admission and repair eligibility controls.

use super::{
    MissingDiscriminatorFact, RelationReason, RepairPacketIneligibility, RequiredDiscriminator,
    SeamGripClass, boundary_seam, classified_with, discriminator_fact_matches,
    is_safe_for_repair_packet, repair_packet_eligibility, repair_packet_queue_visible,
    rust_related_test,
};

#[test]
fn boundary_matching_preserves_operand_case() {
    let required = RequiredDiscriminator::BoundaryValue {
        description: "amount >= LIMIT".to_string(),
    };

    for fact in [
        "amount >= LIMIT",
        "amount == LIMIT",
        "LIMIT (equality boundary)",
        "LIMIT (boundary value)",
    ] {
        assert!(discriminator_fact_matches(&required, fact), "{fact}");
    }
    for fact in [
        "Amount >= LIMIT",
        "amount >= limit",
        "Amount == LIMIT",
        "amount == limit",
        "limit (equality boundary)",
    ] {
        assert!(!discriminator_fact_matches(&required, fact), "{fact}");
    }
}

#[test]
fn exact_discriminator_matching_preserves_case_and_literal_contents() {
    let cases = [
        (
            RequiredDiscriminator::ErrorVariant {
                variant: "Error::HTTP".to_string(),
            },
            "Error::HTTP",
            "Error::Http",
        ),
        (
            RequiredDiscriminator::ReturnValue {
                description: r#""Ready""#.to_string(),
            },
            r#""Ready""#,
            r#""ready""#,
        ),
        (
            RequiredDiscriminator::FieldValue {
                field: r#"status: "OK""#.to_string(),
            },
            r#"status: "OK""#,
            r#"status: "ok""#,
        ),
        (
            RequiredDiscriminator::MatchArmTaken {
                arm: "Mode::Read => true".to_string(),
            },
            "Mode::Read => true",
            "Mode::READ => true",
        ),
    ];
    for (required, exact, different) in cases {
        assert!(discriminator_fact_matches(&required, exact), "{exact}");
        assert!(discriminator_fact_matches(
            &required,
            &format!("  {exact}  ")
        ));
        assert!(
            !discriminator_fact_matches(&required, different),
            "{different}"
        );
    }

    let required = RequiredDiscriminator::ReturnValue {
        description: r#""Ready Now""#.to_string(),
    };
    assert!(discriminator_fact_matches(&required, r#""Ready Now""#));
    assert!(!discriminator_fact_matches(&required, r#""Ready  Now""#));
}

#[test]
fn case_mismatched_discriminator_cannot_authorize_a_repair_packet() {
    let mut entry = classified_with(
        boundary_seam(),
        SeamGripClass::WeaklyGripped,
        vec![rust_related_test(RelationReason::DirectOwnerCall)],
    );
    let original = entry.evidence.missing_discriminators.clone();
    assert!(repair_packet_eligibility(&entry).eligible());

    entry.evidence.missing_discriminators = vec![MissingDiscriminatorFact {
        value: "DISCOUNT_THRESHOLD (equality boundary)".to_string(),
        reason: "case-distinct boundary identity".to_string(),
        flow_sink: None,
    }];
    let eligibility = repair_packet_eligibility(&entry);
    assert_eq!(
        eligibility.ineligibility,
        Some(RepairPacketIneligibility::RouteNotReady)
    );
    assert!(!eligibility.eligible());
    assert!(!eligibility.readiness.is_repair_ready());
    assert!(eligibility.readiness.test_target.is_some());
    assert!(!is_safe_for_repair_packet(&entry));
    assert!(repair_packet_queue_visible(&entry));

    entry.evidence.missing_discriminators = original;
    assert!(repair_packet_eligibility(&entry).eligible());
}

/// #10622: the human check advisory recognises the boundary-value item through
/// the producer's own format. Build the real not-ready readiness and assert the
/// recogniser accepts what the producer emitted, so format drift fails here.
#[test]
fn boundary_value_missing_evidence_is_recognised_from_the_real_producer() {
    let mut entry = classified_with(
        boundary_seam(),
        SeamGripClass::WeaklyGripped,
        vec![rust_related_test(RelationReason::DirectOwnerCall)],
    );
    entry.evidence.missing_discriminators = Vec::new();
    let missing = repair_packet_eligibility(&entry).readiness.missing_evidence;
    assert!(
        missing
            .iter()
            .any(|item| super::super::is_boundary_value_discriminator_evidence(item)),
        "producer missing list must be recognised: {missing:?}"
    );
    assert!(!super::super::is_boundary_value_discriminator_evidence(
        "safe test target"
    ));
    assert!(!super::super::is_boundary_value_discriminator_evidence(
        &super::super::producer_discriminator_evidence("return_value")
    ));
}
