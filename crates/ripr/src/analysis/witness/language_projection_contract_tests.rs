use std::collections::BTreeMap;

use super::TargetState;
use super::language_projection::{
    CanonicalFamilyState, LanguageNativeProducerFacts, NativeExtensionEnvelopeV1, ProducerIdentity,
    RelatedTestRowWitness, project_native_facts, stage_unknown, stage_yes,
};
use super::language_projection_fixtures::{
    FULLY_MAPPED, PARTIAL, STALE_PRODUCER, UNMAPPABLE_NATIVE,
};
use super::relations::{RelationStatus, RelationWitness};
use crate::domain::{Confidence, StageEvidence, StageState};

fn load_fixture(name: &str) -> Result<LanguageNativeProducerFacts, String> {
    let text = match name {
        "fully_mapped" => FULLY_MAPPED,
        "partial" => PARTIAL,
        "unmappable_native" => UNMAPPABLE_NATIVE,
        "stale_producer" => STALE_PRODUCER,
        other => return Err(format!("unknown fixture name: {other}")),
    };
    serde_json::from_str(text).map_err(|err| format!("parse fixture {name}: {err}"))
}

fn producer_identity(currentness: &str) -> ProducerIdentity {
    ProducerIdentity {
        producer: "preview_adapter".to_string(),
        input: "diff_only_subject_set".to_string(),
        capability: "static_projection".to_string(),
        currentness: currentness.to_string(),
    }
}

fn relation(status: RelationStatus) -> RelationWitness {
    RelationWitness {
        relation_reason: "direct_owner_call".to_string(),
        relation_confidence: "medium".to_string(),
        oracle_kind: "assertion".to_string(),
        oracle_strength: "strong".to_string(),
        status,
        has_test_target: true,
    }
}

#[test]
fn fixture_fully_mapped_projects_canonical_family_without_strengthening() -> Result<(), String> {
    let facts = load_fixture("fully_mapped")?;
    let projection = project_native_facts(facts);
    assert!(matches!(
        projection.canonical_family,
        CanonicalFamilyState::Mapped { .. }
    ));
    assert!(projection.digest_matches());
    assert_eq!(projection.reach.state, "yes");
    assert_eq!(projection.activation.state, "yes");
    Ok(())
}

#[test]
fn fixture_partial_retains_missing_stage_and_limitations() -> Result<(), String> {
    let facts = load_fixture("partial")?;
    let projection = project_native_facts(facts);
    assert_ne!(projection.observation.state, "yes");
    assert!(
        projection
            .limitations
            .iter()
            .any(|item| item == "missing_observer"),
        "limitations: {:?}",
        projection.limitations
    );
    Ok(())
}

#[test]
fn fixture_unmappable_native_retains_extension_and_typed_limitation() -> Result<(), String> {
    let facts = load_fixture("unmappable_native")?;
    let projection = project_native_facts(facts);
    assert!(matches!(
        projection.canonical_family,
        CanonicalFamilyState::Unmappable { .. }
    ));
    assert!(
        projection
            .limitations
            .iter()
            .any(|item| item == "canonical_family_unmappable")
    );
    assert!(!projection.native_extensions.fields.is_empty());
    Ok(())
}

#[test]
fn fixture_stale_producer_records_currentness_limitation() -> Result<(), String> {
    let facts = load_fixture("stale_producer")?;
    let projection = project_native_facts(facts);
    assert!(
        projection
            .limitations
            .iter()
            .any(|item| item == "stale_input")
    );
    assert_eq!(projection.producer_identity.currentness, "stale_input");
    Ok(())
}

#[test]
fn native_extension_field_ordering_does_not_change_semantic_digest() {
    let mut first_fields = BTreeMap::new();
    first_fields.insert("perl.packet.slot".to_string(), "three".to_string());
    first_fields.insert("perl.packet.kind".to_string(), "mention_only".to_string());

    let mut second_fields = BTreeMap::new();
    second_fields.insert("perl.packet.kind".to_string(), "mention_only".to_string());
    second_fields.insert("perl.packet.slot".to_string(), "three".to_string());

    let base = LanguageNativeProducerFacts {
        language: "perl".to_string(),
        native_behavior_family: "perl.packet.mention_only".to_string(),
        owner_identity: "MyModule::handler".to_string(),
        owner_source: "packet_owner".to_string(),
        candidate_relations: vec![relation(RelationStatus::Candidate)],
        established_relations: Vec::new(),
        reach: stage_yes(),
        activation: stage_yes(),
        propagation: stage_yes(),
        observation: stage_unknown(),
        discrimination: stage_unknown(),
        required_discriminator: "slot == three".to_string(),
        required_discriminator_source: "packet_fact".to_string(),
        observed_discriminators: Vec::new(),
        missing_discriminators: vec!["slot == three".to_string()],
        related_test_rows: Vec::new(),
        selected_target: TargetState::TypedAbsence {
            reason: "no_selected_target".to_string(),
        },
        limitations: Vec::new(),
        non_claims: Vec::new(),
        native_extensions: NativeExtensionEnvelopeV1 {
            namespace: "perl.preview.v1".to_string(),
            schema_version: 1,
            fields: first_fields,
        },
        producer_identity: producer_identity("current"),
    };

    let mut reordered = base.clone();
    reordered.native_extensions.fields = second_fields;

    let left = project_native_facts(base);
    let right = project_native_facts(reordered);
    assert_eq!(left.semantic_digest, right.semantic_digest);
}

#[test]
fn same_native_owner_token_does_not_collide_across_languages() {
    let token = "pricing::discounted_total";
    let mut python = base_facts("python", "predicate_boundary", token);
    let mut perl = base_facts("perl", "predicate_boundary", token);
    python.language = "python".to_string();
    perl.language = "perl".to_string();

    let python_projection = project_native_facts(python);
    let perl_projection = project_native_facts(perl);
    assert_ne!(
        python_projection.portable_owner_identity(),
        perl_projection.portable_owner_identity()
    );
    assert_ne!(
        python_projection.semantic_digest,
        perl_projection.semantic_digest
    );
}

#[test]
fn removing_activation_fact_weakens_only_activation_stage() {
    let full = base_facts("python", "predicate_boundary", "module::compute");
    let mut weakened = full.clone();
    weakened.activation = StageEvidence::new(StageState::Unknown, Confidence::Unknown, "producer");

    let full_projection = project_native_facts(full);
    let weakened_projection = project_native_facts(weakened);

    assert_eq!(full_projection.reach.state, weakened_projection.reach.state);
    assert_eq!(
        full_projection.propagation.state,
        weakened_projection.propagation.state
    );
    assert_eq!(
        full_projection.observation.state,
        weakened_projection.observation.state
    );
    assert_eq!(full_projection.reach.state, "yes");
    assert_eq!(full_projection.activation.state, "yes");
    assert_ne!(
        full_projection.activation.state,
        weakened_projection.activation.state
    );
}

#[test]
fn removing_observer_fact_weakens_only_observation_stage() {
    let full = base_facts("typescript", "return_value", "src/util.ts::sum");
    let mut weakened = full.clone();
    weakened.observation = StageEvidence::new(StageState::Unknown, Confidence::Unknown, "producer");

    let full_projection = project_native_facts(full);
    let weakened_projection = project_native_facts(weakened);
    assert_eq!(
        full_projection.activation.state,
        weakened_projection.activation.state
    );
    assert_ne!(
        full_projection.observation.state,
        weakened_projection.observation.state
    );
}

#[test]
fn rust_probe_family_consumes_existing_canonical_vocabulary() {
    use super::language_projection::{
        CanonicalFamilyState, canonical_family_for_native, canonical_family_from_probe_family,
    };
    use crate::domain::ProbeFamily;

    assert!(matches!(
        canonical_family_from_probe_family(ProbeFamily::Predicate),
        CanonicalFamilyState::Mapped {
            canonical_family
        } if canonical_family == "predicate_boundary"
    ));
    assert!(matches!(
        canonical_family_for_native("call_deletion"),
        CanonicalFamilyState::Unmappable { .. }
    ));
}

#[test]
fn native_extension_field_keys_are_language_namespaced() {
    use super::language_projection::NativeExtensionEnvelopeV1;
    assert_eq!(
        NativeExtensionEnvelopeV1::namespaced_field_key("perl", "packet.kind"),
        "perl:native_extension:packet.kind"
    );
}

#[test]
fn stage_source_identity_corruption_invalidates_digest() {
    let facts = base_facts("python", "predicate_boundary", "module::compute");
    let mut projection = project_native_facts(facts);
    assert!(projection.digest_matches());
    projection
        .reach
        .source_identities
        .push("direct_owner_call".to_string());
    assert!(!projection.digest_matches());
}

#[test]
fn related_test_row_fields_do_not_collide_on_colons() {
    let mut left = base_facts("python", "predicate_boundary", "module::compute");
    left.related_test_rows = vec![RelatedTestRowWitness {
        test_name: "module::test:case".to_string(),
        row_assessment: "missing".to_string(),
        oracle_kind: "assertion".to_string(),
        oracle_strength: "strong".to_string(),
        status: RelationStatus::Candidate,
    }];
    let mut right = base_facts("python", "predicate_boundary", "module::compute");
    right.related_test_rows = vec![RelatedTestRowWitness {
        test_name: "module::test".to_string(),
        row_assessment: "case:missing".to_string(),
        oracle_kind: "assertion".to_string(),
        oracle_strength: "strong".to_string(),
        status: RelationStatus::Candidate,
    }];
    assert_ne!(
        project_native_facts(left).semantic_digest,
        project_native_facts(right).semantic_digest
    );
}

#[test]
fn related_test_rename_without_evidence_change_keeps_digest() {
    let mut first = base_facts("python", "predicate_boundary", "module::compute");
    first.related_test_rows[0].test_name = "test_discount_boundary".to_string();
    let mut second = first.clone();
    second.related_test_rows[0].test_name = "test_discount_edge".to_string();
    assert_eq!(
        project_native_facts(first).semantic_digest,
        project_native_facts(second).semantic_digest
    );
}

#[test]
fn native_extension_values_stay_verbatim_in_digest() {
    let mut with_semicolon = base_facts("perl", "perl.packet.mention_only", "MyApp::run");
    with_semicolon
        .native_extensions
        .fields
        .insert("perl.packet.token".to_string(), "$x;".to_string());
    let mut without_semicolon = with_semicolon.clone();
    without_semicolon
        .native_extensions
        .fields
        .insert("perl.packet.token".to_string(), "$x".to_string());
    assert_ne!(
        project_native_facts(with_semicolon).semantic_digest,
        project_native_facts(without_semicolon).semantic_digest
    );
}

fn base_facts(language: &str, family: &str, owner: &str) -> LanguageNativeProducerFacts {
    LanguageNativeProducerFacts {
        language: language.to_string(),
        native_behavior_family: family.to_string(),
        owner_identity: owner.to_string(),
        owner_source: "native_owner".to_string(),
        candidate_relations: vec![relation(RelationStatus::Candidate)],
        established_relations: Vec::new(),
        reach: stage_yes(),
        activation: stage_yes(),
        propagation: stage_yes(),
        observation: stage_yes(),
        discrimination: stage_yes(),
        required_discriminator: "boundary".to_string(),
        required_discriminator_source: "native".to_string(),
        observed_discriminators: vec!["boundary".to_string()],
        missing_discriminators: Vec::new(),
        related_test_rows: vec![RelatedTestRowWitness {
            test_name: "test_boundary".to_string(),
            row_assessment: "aligned_observer_missing".to_string(),
            oracle_kind: "assertion".to_string(),
            oracle_strength: "strong".to_string(),
            status: RelationStatus::Candidate,
        }],
        selected_target: TargetState::TypedAbsence {
            reason: "no_selected_target".to_string(),
        },
        limitations: Vec::new(),
        non_claims: Vec::new(),
        native_extensions: NativeExtensionEnvelopeV1 {
            namespace: format!("{language}.preview.v1"),
            schema_version: 1,
            fields: BTreeMap::new(),
        },
        producer_identity: producer_identity("current"),
    }
}
