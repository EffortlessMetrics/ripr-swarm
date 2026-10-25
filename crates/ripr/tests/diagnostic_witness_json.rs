//! RIPR-SPEC-0069: retain the typed witness across its sparse JSON form.

use proptest::prelude::*;
use ripr::domain::{
    DiagnosticConfidence, DiagnosticFixSite, DiagnosticSourceLocation, DiagnosticWitness,
    DiagnosticWitnessLimitation, MissingDiscriminatorFact,
};
use serde_json::{Value, json};

fn witness() -> DiagnosticWitness {
    DiagnosticWitness {
        kind: "static_discriminator_gap".into(),
        probe_family: "predicate".into(),
        changed_expression: "amount >= threshold".into(),
        before: None,
        after: None,
        expected_sink: None,
        missing_discriminators: vec![MissingDiscriminatorFact {
            value: "amount == threshold".into(),
            reason: "the equality boundary is not distinguished".into(),
            flow_sink: None,
        }],
        fix_site: None,
        suggested_assertion: None,
        explain_command: "ripr explain --root . probe:boundary".into(),
        confidence: DiagnosticConfidence {
            value: None,
            basis: "static_only".into(),
        },
        limitations: Vec::new(),
    }
}

fn limitation() -> DiagnosticWitnessLimitation {
    DiagnosticWitnessLimitation {
        kind: "suggested_assertion_unavailable".into(),
        detail: "No producer-owned assertion is available.".into(),
    }
}

#[test]
fn given_empty_limitations_when_roundtripped_then_omitted_array_is_restored()
-> Result<(), serde_json::Error> {
    let original = witness();
    let encoded = serde_json::to_value(&original)?;
    assert!(encoded.get("limitations").is_none());
    let decoded: DiagnosticWitness = serde_json::from_value(encoded)?;
    assert_eq!(decoded, original);
    Ok(())
}

#[test]
fn given_populated_limitations_when_roundtripped_then_every_row_is_retained()
-> Result<(), serde_json::Error> {
    let mut original = witness();
    original.limitations = vec![
        limitation(),
        DiagnosticWitnessLimitation {
            kind: "oracle_source_location_unavailable".into(),
            detail: "No exact assertion location is available.".into(),
        },
    ];
    let encoded = serde_json::to_value(&original)?;
    assert_eq!(encoded["limitations"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        serde_json::from_value::<DiagnosticWitness>(encoded)?,
        original
    );
    Ok(())
}

#[test]
fn given_absent_or_null_optional_facts_then_both_decode_to_none() -> Result<(), serde_json::Error> {
    let mut original = witness();
    original.limitations.push(limitation());
    let absent = serde_json::to_value(&original)?;
    let mut explicit_null = absent.clone();
    for field in [
        "before",
        "after",
        "expected_sink",
        "fix_site",
        "suggested_assertion",
    ] {
        assert!(absent.get(field).is_none(), "{field} should be omitted");
        explicit_null[field] = Value::Null;
    }
    assert!(absent["confidence"].get("value").is_none());
    explicit_null["confidence"]["value"] = Value::Null;
    assert_eq!(
        serde_json::from_value::<DiagnosticWitness>(absent.clone())?,
        original
    );
    let decoded: DiagnosticWitness = serde_json::from_value(explicit_null)?;
    assert_eq!(decoded, original);
    // None has one canonical wire form: omission, including after a null input.
    assert_eq!(serde_json::to_value(decoded)?, absent);
    Ok(())
}

#[test]
fn given_null_or_malformed_limitations_then_deserialization_still_refuses()
-> Result<(), serde_json::Error> {
    let baseline = serde_json::to_value(witness())?;
    for invalid in [Value::Null, json!(false), json!(""), json!({}), json!([{}])] {
        let mut value = baseline.clone();
        value["limitations"] = invalid;
        assert!(serde_json::from_value::<DiagnosticWitness>(value).is_err());
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        rng_seed: proptest::test_runner::RngSeed::Fixed(69_2026),
        ..ProptestConfig::default()
    })]

    #[test]
    fn given_optional_witness_facts_when_roundtripped_then_presence_and_values_survive(
        fields in 0u16..512,
        text in proptest::collection::vec(any::<char>(), 0..32)
            .prop_map(|characters| characters.into_iter().collect::<String>()),
    ) {
        let present = |bit: u32| fields & (1u16 << bit) != 0;
        let mut original = witness();
        original.before = present(0).then(|| text.clone());
        original.after = present(1).then(|| text.clone());
        original.expected_sink = present(2).then(|| text.clone());
        original.suggested_assertion = present(3).then(|| text.clone());
        original.confidence.value = present(4).then_some(0.0);
        original.fix_site = present(5).then(|| DiagnosticFixSite {
            file: "tests/pricing.rs".into(),
            line: 42,
            test_name: "equality_boundary".into(),
            current_oracle: present(6).then(|| text.clone()),
            oracle_kind: "exact_equality".into(),
            oracle_strength: "strong".into(),
            oracle_location: present(7).then(|| DiagnosticSourceLocation {
                file: "tests/pricing.rs".into(),
                line: 44,
            }),
        });
        if present(8) {
            original.limitations.push(limitation());
        }
        let encoded = serde_json::to_value(&original)?;
        prop_assert_eq!(encoded.get("before").is_some(), original.before.is_some());
        prop_assert_eq!(encoded.get("limitations").is_some(), !original.limitations.is_empty());
        let decoded: DiagnosticWitness = serde_json::from_value(encoded)?;
        prop_assert_eq!(decoded, original);
    }
}
