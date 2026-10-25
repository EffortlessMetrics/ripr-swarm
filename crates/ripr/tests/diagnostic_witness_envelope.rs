//! RIPR-SPEC-0069: compatibility of the complete public context packet.

use std::{collections::BTreeMap, path::Path};

use ripr::config::RiprConfig;
use ripr::domain::{
    ActivationEvidence, Confidence, DeltaKind, DiagnosticWitness, ExposureClass, Finding,
    MissingDiscriminatorFact, Probe, ProbeFamily, ProbeId, RevealEvidence, RiprEvidence,
    SourceCurrentness, SourceLocation, StageEvidence, StageState,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// ContextPacket deliberately has no production Deserialize implementation.
// This read-only external consumer parses the entire rendered envelope, using
// the real DiagnosticWitness type for its nested contract. It does not add a
// second product parser or a promise to forward unknown witness properties.
#[derive(Debug, Deserialize, Serialize)]
struct PacketReader {
    version: String,
    tool: String,
    classification: String,
    related_tests_total: usize,
    probe: Value,
    witness: Option<DiagnosticWitness>,
    #[serde(flatten)]
    other: BTreeMap<String, Value>,
}

fn finding() -> Finding {
    let stage = || StageEvidence::new(StageState::Yes, Confidence::Medium, "fixture evidence");
    Finding {
        id: "probe:src_lib_rs:9:predicate".into(),
        canonical_gap: None,
        probe: Probe {
            id: ProbeId("probe:src_lib_rs:9:predicate".into()),
            location: SourceLocation::new("src/lib.rs", 9, 1),
            owner: None,
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: Some("amount > threshold".into()),
            after: Some("amount >= threshold".into()),
            expression: "amount >= threshold".into(),
            expected_sinks: vec!["return_value".into()],
            required_oracles: Vec::new(),
        },
        class: ExposureClass::WeaklyExposed,
        ripr: RiprEvidence {
            reach: stage(),
            infect: stage(),
            propagate: stage(),
            reveal: RevealEvidence {
                observe: stage(),
                discriminate: stage(),
            },
        },
        confidence: 0.5,
        evidence: Vec::new(),
        missing: Vec::new(),
        flow_sinks: Vec::new(),
        activation: ActivationEvidence {
            observed_values: Vec::new(),
            missing_discriminators: vec![MissingDiscriminatorFact {
                value: "amount == threshold".into(),
                reason: "the equality boundary is not distinguished".into(),
                flow_sink: None,
            }],
        },
        stop_reasons: Vec::new(),
        related_tests_matched_total: None,
        related_tests: Vec::new(),
        recommended_next_step: None,
        language: None,
        language_status: None,
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: None,
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness: SourceCurrentness::UnresolvedSubject,
    }
}

fn envelope() -> Result<Value, serde_json::Error> {
    let rendered = ripr::output::json::render_context_packet(
        &finding(),
        5,
        &RiprConfig::default(),
        Path::new("."),
    );
    serde_json::from_str(&rendered)
}

fn read_packet(value: Value) -> Result<PacketReader, Box<dyn std::error::Error>> {
    // Cross the string boundary with the whole document, including its header.
    let packet: PacketReader = serde_json::from_str(&serde_json::to_string(&value)?)?;
    assert_eq!(packet.version, "1.1");
    assert_eq!(packet.tool, "ripr");
    assert_eq!(packet.classification, "weakly_exposed");
    assert_eq!(packet.related_tests_total, 0);
    assert_eq!(packet.probe["id"], "probe:src_lib_rs:9:predicate");
    Ok(packet)
}

#[test]
fn given_public_packet_when_decoded_then_producer_limitations_and_wire_are_retained()
-> Result<(), Box<dyn std::error::Error>> {
    let value = envelope()?;
    let expected = DiagnosticWitness::from_finding(&finding()).ok_or("fixture witness missing")?;
    assert!(
        !expected.limitations.is_empty(),
        "must reach the real producer"
    );
    let packet = read_packet(value.clone())?;
    assert_eq!(packet.witness, Some(expected));
    assert!(value["witness"].get("suggested_assertion").is_none());
    let reparsed: Value = serde_json::from_str(&serde_json::to_string(&value)?)?;
    assert_eq!(reparsed, value);
    Ok(())
}

#[test]
fn given_omitted_or_empty_packet_limitations_then_full_consumer_defaults_to_empty()
-> Result<(), Box<dyn std::error::Error>> {
    let mut omitted = envelope()?;
    let removed = omitted["witness"]
        .as_object_mut()
        .ok_or("fixture witness object missing")?
        .remove("limitations")
        .ok_or("producer limitations missing")?;
    assert!(removed.as_array().is_some_and(|rows| !rows.is_empty()));
    let mut explicit_empty = omitted.clone();
    explicit_empty["witness"]["limitations"] = json!([]);
    let absent = read_packet(omitted)?
        .witness
        .ok_or("omitted case witness missing")?;
    let empty = read_packet(explicit_empty)?
        .witness
        .ok_or("empty case witness missing")?;
    assert!(absent.limitations.is_empty());
    assert_eq!(absent, empty);
    Ok(())
}

#[test]
fn given_null_or_malformed_packet_limitations_then_full_consumer_refuses()
-> Result<(), Box<dyn std::error::Error>> {
    for invalid in [Value::Null, json!(false), json!(""), json!({}), json!([{}])] {
        let mut value = envelope()?;
        value["witness"]["limitations"] = invalid;
        // The envelope remains syntactically valid JSON; its nested typed
        // witness contract, rather than transport parsing, must reject it.
        let wire = serde_json::to_string(&value)?;
        assert!(serde_json::from_str::<Value>(&wire).is_ok());
        assert!(serde_json::from_str::<PacketReader>(&wire).is_err());
    }
    Ok(())
}

#[test]
fn given_unknown_optional_packet_properties_then_known_facts_and_opaque_wire_survive()
-> Result<(), Box<dyn std::error::Error>> {
    let base = envelope()?;
    let expected = read_packet(base.clone())?.witness;
    let mut future = base;
    future["future_envelope"] = json!({"revision": 2, "hint": null});
    future["witness"]["future_optional_fact"] = json!({"text": "boundary \"λ\"", "count": 0});
    future["witness"]["confidence"]["future_basis"] = json!(["not interpreted"]);
    future["witness"]["limitations"][0]["future_recovery"] = json!({"route": "inspect"});
    let wire = serde_json::to_string(&future)?;
    let opaque: Value = serde_json::from_str(&wire)?;
    assert_eq!(
        opaque, future,
        "the complete JSON document must retain extensions"
    );
    let packet = read_packet(opaque)?;
    assert_eq!(
        packet.witness, expected,
        "unknown properties must not fabricate known facts"
    );
    // A typed read/write relay retains the envelope's opaque fields, but the
    // nested DiagnosticWitness has no unknown-property store. Pin this limit
    // so an input-compatibility fix is not mistaken for lossless forwarding.
    let typed_relay = serde_json::to_value(&packet)?;
    assert_eq!(typed_relay["future_envelope"], future["future_envelope"]);
    assert!(typed_relay["witness"].get("future_optional_fact").is_none());
    assert!(
        typed_relay["witness"]["confidence"]
            .get("future_basis")
            .is_none()
    );
    assert!(
        typed_relay["witness"]["limitations"][0]
            .get("future_recovery")
            .is_none()
    );
    Ok(())
}

#[test]
fn given_future_limitation_kind_then_full_consumer_preserves_its_kind_and_detail()
-> Result<(), Box<dyn std::error::Error>> {
    let mut value = envelope()?;
    value["witness"]["limitations"] = json!([
        {"kind": "future_producer_limit", "detail": "unknown is retained, not guessed"},
        {"kind": "future_producer_limit", "detail": "a second distinct row"}
    ]);
    let witness = read_packet(value)?
        .witness
        .ok_or("future case witness missing")?;
    assert_eq!(witness.limitations.len(), 2);
    assert_eq!(witness.limitations[0].kind, "future_producer_limit");
    assert_eq!(
        witness.limitations[0].detail,
        "unknown is retained, not guessed"
    );
    assert_eq!(witness.limitations[1].detail, "a second distinct row");
    Ok(())
}

#[test]
fn given_exposed_finding_without_gap_then_public_packet_emits_explicit_null_witness()
-> Result<(), Box<dyn std::error::Error>> {
    let mut finding = finding();
    finding.class = ExposureClass::Exposed;
    finding.activation.missing_discriminators.clear();
    let rendered = ripr::output::json::render_context_packet(
        &finding,
        5,
        &RiprConfig::default(),
        Path::new("."),
    );
    let value: Value = serde_json::from_str(&rendered)?;
    assert_eq!(value.get("witness"), Some(&Value::Null));
    assert!(
        serde_json::from_str::<PacketReader>(&rendered)?
            .witness
            .is_none()
    );
    Ok(())
}

#[test]
fn given_public_packet_when_rendered_then_complete_wire_is_recorded()
-> Result<(), serde_json::Error> {
    // A disposable removal run compares this full production output with the
    // repaired candidate's output, including whitespace and field omission.
    let rendered = ripr::output::json::render_context_packet(
        &finding(),
        5,
        &RiprConfig::default(),
        Path::new("."),
    );
    assert_eq!(serde_json::from_str::<Value>(&rendered)?, envelope()?);
    println!("{rendered}");
    Ok(())
}
