use ripr::config::RiprConfig;
use ripr::domain::context_packet::ContextPacket;
use ripr::domain::{
    ActivationEvidence, Confidence, DeltaKind, ExposureClass, Finding, OracleKind, OracleStrength,
    Probe, ProbeFamily, ProbeId, RelatedTest, RevealEvidence, RiprEvidence, SourceLocation,
    StageEvidence, StageState, SymbolId,
};
use ripr::output::json::render_context_packet;
use std::path::{Path, PathBuf};

#[test]
fn context_packet_related_test_totals_distinguish_identical_bounded_rows() -> Result<(), String> {
    let config = RiprConfig::default();
    let mut finding = sample_finding();
    finding.related_tests = (0..8)
        .map(|index| related(&format!("retained_{index}")))
        .collect();
    let retained = finding.related_tests.clone();
    let mut packets = Vec::new();
    for total in [12, 13] {
        finding.related_tests_matched_total = Some(total);
        let rendered = render_context_packet(&finding, 8, &config, Path::new("."));
        let packet: serde_json::Value =
            serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
        assert_eq!(packet["related_tests"].as_array().map(Vec::len), Some(8));
        assert_eq!(packet["related_tests_total"], total);
        assert_eq!(finding.related_tests, retained);
        packets.push(packet);
    }
    assert_eq!(packets[0]["related_tests"], packets[1]["related_tests"]);
    assert_ne!(packets[0], packets[1]);
    // The total must be the only output difference: metadata cannot alter
    // selection, producer witness, or guidance.
    for packet in &mut packets {
        packet
            .as_object_mut()
            .ok_or("packet must be an object")?
            .remove("related_tests_total");
    }
    assert_eq!(packets[0], packets[1]);
    Ok(())
}

#[test]
fn context_packet_related_test_totals_preserve_floors_and_display_windows() -> Result<(), String> {
    let config = RiprConfig::default();
    for (metadata, retained_count, limit, expected_total, expected_shown) in [
        (Some(12), 8, 8, 12, 8),
        (Some(13), 8, 8, 13, 8),
        (Some(1), 1, 1, 1, 1),
        (Some(0), 0, 8, 0, 0),
        (Some(12), 8, 0, 12, 0),
        (Some(12), 8, 3, 12, 3),
        (None, 8, 3, 8, 3),
        (Some(0), 8, 3, 8, 3),
    ] {
        let mut finding = sample_finding();
        finding.related_tests = (0..retained_count)
            .map(|index| related(&format!("retained_{index}")))
            .collect();
        finding.related_tests_matched_total = metadata;
        let original = finding.clone();
        let packet = ContextPacket::from_finding(&finding, limit, Vec::new(), Path::new("."));
        assert_eq!(packet.related_tests_total, expected_total);
        assert_eq!(packet.related_tests.len(), expected_shown);
        assert_eq!(
            packet.related_tests,
            original.related_tests[..expected_shown]
        );
        let rendered: serde_json::Value = serde_json::from_str(&render_context_packet(
            &finding,
            limit,
            &config,
            Path::new("."),
        ))
        .map_err(|error| error.to_string())?;
        assert_eq!(rendered["related_tests_total"], expected_total);
        assert_eq!(
            rendered["related_tests"].as_array().map(Vec::len),
            Some(expected_shown)
        );
        assert_eq!(
            finding, original,
            "projection cannot mutate producer evidence"
        );
    }
    Ok(())
}

#[test]
fn context_packet_related_test_totals_accept_legacy_omitted_and_null_metadata() -> Result<(), String>
{
    let config = RiprConfig::default();
    let mut finding = sample_finding();
    finding.related_tests = (0..8)
        .map(|index| related(&format!("retained_{index}")))
        .collect();
    finding.related_tests_matched_total = Some(8);
    let explicit = render_context_packet(&finding, 3, &config, Path::new("."));
    for metadata in [None, Some(serde_json::Value::Null)] {
        let mut raw = serde_json::to_value(&finding).map_err(|error| error.to_string())?;
        let fields = raw.as_object_mut().ok_or("finding must be an object")?;
        fields.remove("related_tests_matched_total");
        if let Some(metadata) = metadata {
            fields.insert("related_tests_matched_total".to_string(), metadata);
        }
        let legacy: Finding = serde_json::from_value(raw).map_err(|error| error.to_string())?;
        assert_eq!(legacy.related_tests_matched_total, None);
        let packet = ContextPacket::from_finding(&legacy, 3, Vec::new(), Path::new("."));
        assert_eq!(packet.related_tests_total, 8);
        assert_eq!(packet.related_tests.len(), 3);
        assert_eq!(
            render_context_packet(&legacy, 3, &config, Path::new(".")),
            explicit
        );
    }
    Ok(())
}

fn sample_finding() -> Finding {
    Finding {
        id: "probe:src_lib_rs:9:error_path".to_string(),
        canonical_gap: None,
        probe: Probe {
            id: ProbeId("probe:src_lib_rs:9:error_path".to_string()),
            location: SourceLocation::new("src/lib.rs", 9, 1),
            owner: Some(SymbolId("crate::sample".to_string())),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: Some("x > 0".to_string()),
            after: Some("x >= 0".to_string()),
            expression: "x >= 0".to_string(),
            expected_sinks: vec![],
            required_oracles: vec![],
        },
        class: ExposureClass::WeaklyExposed,
        ripr: RiprEvidence {
            reach: stage("Reachable"),
            infect: stage("Possible infection"),
            propagate: stage("Possible propagation"),
            reveal: RevealEvidence {
                observe: stage("Observed"),
                discriminate: stage("Weak discriminator"),
            },
        },
        confidence: 0.5,
        evidence: vec![],
        missing: vec![],
        flow_sinks: vec![],
        activation: ActivationEvidence::default(),
        stop_reasons: vec![],
        related_tests_matched_total: None,
        related_tests: vec![],
        recommended_next_step: None,
        language: None,
        language_status: None,
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: None,
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness: ripr::domain::SourceCurrentness::CandidateCurrent,
    }
}

fn related(name: &str) -> RelatedTest {
    RelatedTest {
        name: name.to_string(),
        file: PathBuf::from("tests/sample.rs"),
        line: 7,
        oracle: Some("assert_eq!(value, 1)".to_string()),
        oracle_kind: OracleKind::ExactValue,
        oracle_strength: OracleStrength::Strong,
        relation_reason: None,
        relation_confidence: None,
        miss: None,
    }
}

fn stage(summary: &str) -> StageEvidence {
    StageEvidence::new(StageState::Yes, Confidence::Medium, summary)
}
