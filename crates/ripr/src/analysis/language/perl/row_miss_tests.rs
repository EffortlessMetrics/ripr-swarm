//! Focused controls for Perl v2 row-local miss mapping (#5562).

use super::tests::{
    EXACT_RETURN_PACKET, SECOND_TEST, THRESHOLD_TEST, edited_packet, findings_from_packet,
    first_finding_misses, only_finding, packet_array, rows_by_name, set_first, with_second_row,
};
use super::*;
use crate::app::PERL_FACT_PACKET_V2_SCHEMA;
use crate::domain::{ExposureClass, RelatedTestMiss};

const PRIMARY_RELATION: &str = "relation:change:discount-return:test:threshold";
const WRAPPER_RELATION: &str = "relation:change:discount-return:test:wrapper";

fn init_v2_arrays(packet: &mut serde_json::Value) -> Result<(), String> {
    for key in [
        "predicate_participations",
        "oracle_relevances",
        "activation_framework",
        "relation_reach",
        "oracle_scope_coverage",
    ] {
        if packet.get(key).is_none() {
            packet[key] = serde_json::json!([]);
        }
    }
    Ok(())
}

fn v2_packet(text: &str) -> Result<String, String> {
    edited_packet(text, |packet| {
        packet["schema_version"] = PERL_FACT_PACKET_V2_SCHEMA.into();
        init_v2_arrays(packet)
    })
}

fn relation_fields(
    packet: &serde_json::Value,
    relation_id: &str,
) -> Result<(String, String, Option<String>), String> {
    let relations = packet
        .get("relations")
        .and_then(serde_json::Value::as_array)
        .ok_or("relations")?;
    let relation = relations
        .iter()
        .find(|row| row.get("relation_id").and_then(serde_json::Value::as_str) == Some(relation_id))
        .ok_or_else(|| format!("missing relation `{relation_id}`"))?;
    let change_id = relation
        .get("change_id")
        .and_then(serde_json::Value::as_str)
        .ok_or("change_id")?
        .to_string();
    let test_id = relation
        .get("test_id")
        .and_then(serde_json::Value::as_str)
        .ok_or("test_id")?
        .to_string();
    let oracle_id = relation
        .get("oracle_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    Ok((change_id, test_id, oracle_id))
}

fn add_participation(
    packet: &mut serde_json::Value,
    relation_id: &str,
    participation: &str,
    support: Option<&str>,
) -> Result<(), String> {
    init_v2_arrays(packet)?;
    let (change_id, test_id, oracle_id) = relation_fields(packet, relation_id)?;
    let row = serde_json::json!({
        "participation_id": format!("participation:{relation_id}"),
        "change_id": change_id,
        "relation_id": relation_id,
        "test_id": test_id,
        "oracle_id": oracle_id,
        "participation": participation,
        "support_value": support
    });
    packet_array(packet, "predicate_participations")?.push(row);
    Ok(())
}

fn add_relevance(
    packet: &mut serde_json::Value,
    relation_id: &str,
    state: &str,
    discriminator: Option<&str>,
) -> Result<(), String> {
    init_v2_arrays(packet)?;
    let (change_id, test_id, oracle_id) = relation_fields(packet, relation_id)?;
    let mut row = serde_json::json!({
        "relevance_id": format!("relevance:{relation_id}"),
        "change_id": change_id,
        "relation_id": relation_id,
        "test_id": test_id,
        "oracle_id": oracle_id,
        "state": state
    });
    if let Some(value) = discriminator {
        row["required_discriminator"] = value.into();
    }
    packet_array(packet, "oracle_relevances")?.push(row);
    Ok(())
}

fn add_activation(
    packet: &mut serde_json::Value,
    relation_id: &str,
    activation: &str,
    execution: &str,
) -> Result<(), String> {
    init_v2_arrays(packet)?;
    packet_array(packet, "activation_framework")?.push(serde_json::json!({
        "fact_id": format!("activation:{relation_id}"),
        "relation_id": relation_id,
        "activation": activation,
        "framework_execution": execution,
        "framework_oracle_mode": "normal"
    }));
    Ok(())
}

fn add_scope(
    packet: &mut serde_json::Value,
    complete_empty: bool,
    unresolved: bool,
) -> Result<(), String> {
    init_v2_arrays(packet)?;
    let test_id = packet
        .pointer("/relations/0/test_id")
        .and_then(serde_json::Value::as_str)
        .ok_or("test")?
        .to_string();
    packet_array(packet, "oracle_scope_coverage")?.push(serde_json::json!({
        "scope_id": format!("scope:{test_id}"),
        "test_id": test_id,
        "extraction_complete": true,
        "recognized_assertion_count": if complete_empty { 0 } else { 1 },
        "unresolved_admission": unresolved
    }));
    Ok(())
}

fn add_reach(packet: &mut serde_json::Value, relation_id: &str, state: &str) -> Result<(), String> {
    init_v2_arrays(packet)?;
    packet_array(packet, "relation_reach")?.push(serde_json::json!({
        "reach_id": format!("reach:{relation_id}"),
        "relation_id": relation_id,
        "state": state
    }));
    Ok(())
}

#[test]
fn v2_same_outcome_participation_is_missing_input() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_participation(
            packet,
            PRIMARY_RELATION,
            "same_outcome",
            Some("$amount == 100"),
        )
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::MissingInput)
    );
    assert_eq!(
        finding.related_tests[0]
            .miss_support
            .as_ref()
            .map(|support| support.value.as_str()),
        Some("$amount == 100")
    );
    Ok(())
}

#[test]
fn v2_distinguishes_participation_prevents_missing_input() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_participation(packet, PRIMARY_RELATION, "distinguishes", None)
    })?)?;
    let finding = only_finding(&packet)?;
    assert_ne!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::MissingInput)
    );
    Ok(())
}

#[test]
fn v2_impossible_activation_never_overloads_miss() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_participation(
            packet,
            PRIMARY_RELATION,
            "same_outcome",
            Some("$amount == 100"),
        )?;
        add_activation(packet, PRIMARY_RELATION, "impossible", "executes")
    })?)?;
    assert_eq!(
        first_finding_misses(&packet)?,
        (ExposureClass::WeaklyExposed, vec![None])
    );
    Ok(())
}

#[test]
fn v2_affirmative_other_sink_is_assertion_not_observing() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_relevance(packet, PRIMARY_RELATION, "affirmative_other_sink", None)
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::AssertionNotObserving)
    );
    Ok(())
}

#[test]
fn v2_candidate_relevance_is_observation_unconfirmed() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_relevance(packet, PRIMARY_RELATION, "candidate_sink", None)
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

#[test]
fn v2_broad_exact_surface_is_missing_exact_assertion() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_relevance(
            packet,
            PRIMARY_RELATION,
            "exact_sink_broad_assertion",
            Some("return 42"),
        )
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::MissingExactAssertion)
    );
    assert_eq!(
        finding.related_tests[0]
            .miss_support
            .as_ref()
            .map(|support| support.value.as_str()),
        Some("return 42")
    );
    Ok(())
}

#[test]
fn v2_complete_empty_scope_is_no_assertion() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        set_first(packet, "relations", "oracle_id", serde_json::Value::Null)?;
        add_scope(packet, true, false)
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::NoAssertion)
    );
    Ok(())
}

#[test]
fn v2_unresolved_admission_fabricates_no_miss() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_scope(packet, true, true)
    })?)?;
    assert_eq!(
        first_finding_misses(&packet)?,
        (ExposureClass::WeaklyExposed, vec![None])
    );
    Ok(())
}

#[test]
fn v2_heuristic_relation_keeps_no_miss_on_reachable_unrevealed() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        set_first(
            packet,
            "relations",
            "relation_kind",
            "test_name_match".into(),
        )?;
        set_first(
            packet,
            "relations",
            "reachability_hint",
            "weakly_reachable".into(),
        )?;
        add_reach(packet, PRIMARY_RELATION, "heuristic")
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::NoCallPath)
    );
    Ok(())
}

#[test]
fn v2_mixed_rows_keep_independent_reasons() -> Result<(), String> {
    let base = with_second_row(
        EXACT_RETURN_PACKET,
        "direct_owner_call",
        Some(("smoke_ok", "weak_smoke")),
        Some("oracle:t/app.t:16:ok"),
    )?;
    let packet = v2_packet(&edited_packet(&base, |packet| {
        add_participation(packet, WRAPPER_RELATION, "same_outcome", Some("boundary"))?;
        add_relevance(packet, PRIMARY_RELATION, "candidate_sink", None)
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        rows_by_name(&finding),
        vec![
            (
                THRESHOLD_TEST.to_string(),
                Some(RelatedTestMiss::ObservationUnconfirmed)
            ),
            (SECOND_TEST.to_string(), Some(RelatedTestMiss::MissingInput)),
        ]
    );
    Ok(())
}

#[test]
fn v2_weak_oracle_on_second_direct_row() -> Result<(), String> {
    let base = with_second_row(
        EXACT_RETURN_PACKET,
        "direct_owner_call",
        Some(("smoke_ok", "weak_smoke")),
        Some("oracle:t/app.t:16:ok"),
    )?;
    let packet = v2_packet(&edited_packet(&base, |packet| {
        add_relevance(packet, PRIMARY_RELATION, "candidate_sink", None)
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(
        rows_by_name(&finding),
        vec![
            (
                THRESHOLD_TEST.to_string(),
                Some(RelatedTestMiss::ObservationUnconfirmed)
            ),
            (
                SECOND_TEST.to_string(),
                Some(RelatedTestMiss::WeakAssertion)
            ),
        ]
    );
    Ok(())
}

#[test]
fn v2_exposed_finding_keeps_every_row_miss_none() -> Result<(), String> {
    let aligned = super::tests::with_sinks("$amount * 0.9", Some("$amount * 0.9"));
    let packet = v2_packet(&edited_packet(&aligned, |packet| {
        add_participation(
            packet,
            PRIMARY_RELATION,
            "same_outcome",
            Some("$amount == 100"),
        )
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert!(finding.related_tests.iter().all(|row| row.miss.is_none()));
    Ok(())
}

#[test]
fn v2_miss_annotation_leaves_class_unchanged() -> Result<(), String> {
    let without = edited_packet(EXACT_RETURN_PACKET, |packet| {
        packet["schema_version"] = PERL_FACT_PACKET_V2_SCHEMA.into();
        Ok(())
    })?;
    let with = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_relevance(packet, PRIMARY_RELATION, "candidate_sink", None)
    })?)?;
    let baseline = only_finding(&without)?;
    let annotated = only_finding(&with)?;
    assert_eq!(baseline.class, annotated.class);
    assert_eq!(baseline.id, annotated.id);
    assert_eq!(baseline.canonical_gap, annotated.canonical_gap);
    Ok(())
}

#[test]
fn v2_unrelated_row_facts_preserve_legacy_observation_unconfirmed() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_activation(packet, WRAPPER_RELATION, "possible", "executes")
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

#[test]
fn v2_distinguishes_input_still_allows_assertion_not_observing() -> Result<(), String> {
    let packet = v2_packet(&edited_packet(EXACT_RETURN_PACKET, |packet| {
        add_participation(packet, PRIMARY_RELATION, "distinguishes", None)?;
        add_relevance(packet, PRIMARY_RELATION, "affirmative_other_sink", None)
    })?)?;
    let finding = only_finding(&packet)?;
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::AssertionNotObserving)
    );
    Ok(())
}
