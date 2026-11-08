//! Adapter-to-shared-witness projection contract (#6405 / #5532 PR A).
//!
//! Defines `LanguageWitnessProjectionV1`, the pure boundary by which
//! language-native producer facts populate shared witness semantics without
//! granting new evidence or losing native detail. Rust shared-witness bytes
//! remain owned by the existing #4790 adapters; this module does not replace
//! them.

#![expect(
    dead_code,
    reason = "policy:6405: projection contract lands before preview adapter consumption (#6424/#6426)"
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::BehaviorEvidenceWitnessV1;
use super::adapters::{AdapterInput, from_finding_with_input};
use super::relations::{RelationStatus, RelationWitness};
use super::{
    DiscriminatorIdentity, OwnerIdentity, StageWitness, TargetState, UnresolvedEdge, sort_unique,
};
use crate::domain::{Finding, StageEvidence, StageState};

pub(in crate::analysis) const LANGUAGE_WITNESS_PROJECTION_SCHEMA_VERSION: u16 = 1;
pub(in crate::analysis) const NATIVE_EXTENSION_ENVELOPE_VERSION: u16 = 1;

/// Declared producer, input, capability, and currentness identities for one
/// projection invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct ProducerIdentity {
    pub(in crate::analysis) producer: String,
    pub(in crate::analysis) input: String,
    pub(in crate::analysis) capability: String,
    pub(in crate::analysis) currentness: String,
}

/// Canonical behavior family or typed unmappable state. Mapping consumes the
/// existing seam/oracle crosswalk vocabulary; it does not recreate mapping
/// tables or grant stage strength.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(in crate::analysis) enum CanonicalFamilyState {
    Mapped { canonical_family: String },
    Unmappable { reason: String },
}

/// Namespaced, versioned native extension facts. Field keys participate in the
/// semantic digest without colliding across languages.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct NativeExtensionEnvelopeV1 {
    pub(in crate::analysis) namespace: String,
    pub(in crate::analysis) schema_version: u16,
    #[serde(default)]
    pub(in crate::analysis) fields: BTreeMap<String, String>,
}

impl NativeExtensionEnvelopeV1 {
    pub(in crate::analysis::witness) fn namespaced_field_key(
        language: &str,
        field_name: &str,
    ) -> String {
        format!("{language}:native_extension:{field_name}")
    }

    pub(in crate::analysis::witness) fn normalize(&mut self) {
        // Native extension values stay verbatim; only the map ordering is canonical.
    }
}

/// One related-test row assessment projected from producer facts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(in crate::analysis) struct RelatedTestRowWitness {
    pub(in crate::analysis) test_name: String,
    pub(in crate::analysis) row_assessment: String,
    pub(in crate::analysis) oracle_kind: String,
    pub(in crate::analysis) oracle_strength: String,
    pub(in crate::analysis) status: RelationStatus,
}

/// Language-native producer facts for pure projection. Adapters populate this
/// record; projection cannot read renderer or classifier outputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct LanguageNativeProducerFacts {
    pub(in crate::analysis) language: String,
    pub(in crate::analysis) native_behavior_family: String,
    pub(in crate::analysis) owner_identity: String,
    pub(in crate::analysis) owner_source: String,
    pub(in crate::analysis) candidate_relations: Vec<RelationWitness>,
    pub(in crate::analysis) established_relations: Vec<RelationWitness>,
    pub(in crate::analysis) reach: StageEvidence,
    pub(in crate::analysis) activation: StageEvidence,
    pub(in crate::analysis) propagation: StageEvidence,
    pub(in crate::analysis) observation: StageEvidence,
    pub(in crate::analysis) discrimination: StageEvidence,
    pub(in crate::analysis) required_discriminator: String,
    pub(in crate::analysis) required_discriminator_source: String,
    #[serde(default)]
    pub(in crate::analysis) observed_discriminators: Vec<String>,
    #[serde(default)]
    pub(in crate::analysis) missing_discriminators: Vec<String>,
    #[serde(default)]
    pub(in crate::analysis) related_test_rows: Vec<RelatedTestRowWitness>,
    pub(in crate::analysis) selected_target: TargetState,
    #[serde(default)]
    pub(in crate::analysis) limitations: Vec<String>,
    #[serde(default)]
    pub(in crate::analysis) non_claims: Vec<String>,
    #[serde(default)]
    pub(in crate::analysis) native_extensions: NativeExtensionEnvelopeV1,
    pub(in crate::analysis) producer_identity: ProducerIdentity,
}

/// Versioned adapter-to-shared-witness projection contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct LanguageWitnessProjectionV1 {
    pub(in crate::analysis) schema_version: u16,
    pub(in crate::analysis) language: String,
    pub(in crate::analysis) native_behavior_family: String,
    pub(in crate::analysis) canonical_family: CanonicalFamilyState,
    pub(in crate::analysis) owner: OwnerIdentity,
    pub(in crate::analysis) candidate_relations: Vec<RelationWitness>,
    pub(in crate::analysis) established_relations: Vec<RelationWitness>,
    pub(in crate::analysis) reach: StageWitness,
    pub(in crate::analysis) activation: StageWitness,
    pub(in crate::analysis) propagation: StageWitness,
    pub(in crate::analysis) observation: StageWitness,
    pub(in crate::analysis) discrimination: StageWitness,
    pub(in crate::analysis) required_discriminator: DiscriminatorIdentity,
    #[serde(default)]
    pub(in crate::analysis) observed_discriminators: Vec<String>,
    #[serde(default)]
    pub(in crate::analysis) missing_discriminators: Vec<String>,
    #[serde(default)]
    pub(in crate::analysis) related_test_rows: Vec<RelatedTestRowWitness>,
    pub(in crate::analysis) selected_target: TargetState,
    pub(in crate::analysis) earliest_unresolved_edge: Option<UnresolvedEdge>,
    #[serde(default)]
    pub(in crate::analysis) limitations: Vec<String>,
    #[serde(default)]
    pub(in crate::analysis) non_claims: Vec<String>,
    pub(in crate::analysis) native_extensions: NativeExtensionEnvelopeV1,
    pub(in crate::analysis) producer_identity: ProducerIdentity,
    pub(in crate::analysis) semantic_digest: String,
}

impl LanguageWitnessProjectionV1 {
    pub(in crate::analysis::witness) fn finalize(mut self) -> Self {
        self.normalize_collections();
        self.earliest_unresolved_edge = earliest_unresolved_edge(&self);
        self.semantic_digest = self.compute_semantic_digest();
        self
    }

    pub(in crate::analysis::witness) fn digest_matches(&self) -> bool {
        !self.semantic_digest.is_empty() && self.semantic_digest == self.compute_semantic_digest()
    }

    pub(in crate::analysis::witness) fn portable_owner_identity(&self) -> String {
        format!("{}:{}", self.language, self.owner.identity)
    }

    fn normalize_collections(&mut self) {
        sort_unique(&mut self.limitations);
        sort_unique(&mut self.non_claims);
        sort_unique(&mut self.observed_discriminators);
        sort_unique(&mut self.missing_discriminators);
        self.candidate_relations.sort();
        self.established_relations.sort();
        self.related_test_rows.sort();
        self.reach.normalize();
        self.activation.normalize();
        self.propagation.normalize();
        self.observation.normalize();
        self.discrimination.normalize();
        self.native_extensions.normalize();
        if let TargetState::SelectedExisting { identities } = &mut self.selected_target {
            sort_unique(identities);
        }
    }

    fn compute_semantic_digest(&self) -> String {
        let mut canonical = Vec::new();
        append_field(&mut canonical, "language-witness-projection-v1");
        append_field(&mut canonical, &self.schema_version.to_string());
        append_field(&mut canonical, &self.language);
        append_field(&mut canonical, &self.native_behavior_family);
        append_canonical_family(&mut canonical, &self.canonical_family);
        append_field(&mut canonical, &self.portable_owner_identity());
        append_field(&mut canonical, &self.owner.source);
        append_field(&mut canonical, &self.required_discriminator.identity);
        append_field(&mut canonical, &self.required_discriminator.source);
        append_relations(&mut canonical, "established", &self.established_relations);
        append_relations(&mut canonical, "candidate", &self.candidate_relations);
        append_stage(&mut canonical, "reach", &self.reach);
        append_stage(&mut canonical, "activation", &self.activation);
        append_stage(&mut canonical, "propagation", &self.propagation);
        append_stage(&mut canonical, "observation", &self.observation);
        append_stage(&mut canonical, "discrimination", &self.discrimination);
        append_field(
            &mut canonical,
            &self.observed_discriminators.len().to_string(),
        );
        for value in &self.observed_discriminators {
            append_field(&mut canonical, value);
        }
        append_field(
            &mut canonical,
            &self.missing_discriminators.len().to_string(),
        );
        for value in &self.missing_discriminators {
            append_field(&mut canonical, value);
        }
        append_field(&mut canonical, &self.related_test_rows.len().to_string());
        // Rows are ordered by `test_name` first (derived `Ord`), but the name is
        // deliberately not digested, so a rename could reorder rows. Order the
        // digested fields themselves so the digest is rename-stable.
        let mut digested_rows: Vec<(&str, &str, &str, &str)> = self
            .related_test_rows
            .iter()
            .map(|row| {
                (
                    row.row_assessment.as_str(),
                    row.oracle_kind.as_str(),
                    row.oracle_strength.as_str(),
                    row.status.as_str(),
                )
            })
            .collect();
        digested_rows.sort_unstable();
        for (assessment, oracle_kind, oracle_strength, status) in digested_rows {
            append_field(&mut canonical, "related_test_row");
            append_field(&mut canonical, assessment);
            append_field(&mut canonical, oracle_kind);
            append_field(&mut canonical, oracle_strength);
            append_field(&mut canonical, status);
        }
        match &self.selected_target {
            TargetState::SelectedExisting { identities } => {
                append_field(&mut canonical, "selected_existing");
                append_field(&mut canonical, &identities.len().to_string());
                for identity in identities {
                    append_field(&mut canonical, identity);
                }
            }
            TargetState::TypedAbsence { reason } => {
                append_field(&mut canonical, "typed_absence");
                append_field(&mut canonical, reason);
            }
        }
        append_field(&mut canonical, &self.native_extensions.namespace);
        append_field(
            &mut canonical,
            &self.native_extensions.schema_version.to_string(),
        );
        append_field(
            &mut canonical,
            &self.native_extensions.fields.len().to_string(),
        );
        for (key, value) in &self.native_extensions.fields {
            append_field(&mut canonical, key);
            append_field(&mut canonical, value);
        }
        append_field(&mut canonical, &self.producer_identity.producer);
        append_field(&mut canonical, &self.producer_identity.input);
        append_field(&mut canonical, &self.producer_identity.capability);
        append_field(&mut canonical, &self.producer_identity.currentness);
        append_field(&mut canonical, &self.limitations.len().to_string());
        for limitation in &self.limitations {
            append_field(&mut canonical, limitation);
        }
        append_field(&mut canonical, &self.non_claims.len().to_string());
        for non_claim in &self.non_claims {
            append_field(&mut canonical, non_claim);
        }
        hex_digest(&canonical)
    }
}

/// Map a native behavior family through the existing canonical vocabulary.
pub(in crate::analysis::witness) fn canonical_family_for_native(
    native_behavior_family: &str,
) -> CanonicalFamilyState {
    match native_behavior_family {
        "predicate" | "predicate_boundary" => CanonicalFamilyState::Mapped {
            canonical_family: "predicate_boundary".to_string(),
        },
        "return_value" => CanonicalFamilyState::Mapped {
            canonical_family: "return_value".to_string(),
        },
        "error_path" | "error_variant" => CanonicalFamilyState::Mapped {
            canonical_family: "error_variant".to_string(),
        },
        "field_construction" => CanonicalFamilyState::Mapped {
            canonical_family: "field_construction".to_string(),
        },
        "side_effect" => CanonicalFamilyState::Mapped {
            canonical_family: "side_effect".to_string(),
        },
        "match_arm" => CanonicalFamilyState::Mapped {
            canonical_family: "match_arm".to_string(),
        },
        "call_deletion" => CanonicalFamilyState::Unmappable {
            reason: "no_canonical_crosswalk_to_call_presence".to_string(),
        },
        "static_unknown" => CanonicalFamilyState::Unmappable {
            reason: "static_unknown_family".to_string(),
        },
        other if other.starts_with("perl.packet.") => CanonicalFamilyState::Unmappable {
            reason: "native_packet_family_without_shared_crosswalk".to_string(),
        },
        _ => CanonicalFamilyState::Unmappable {
            reason: "unknown_native_behavior_family".to_string(),
        },
    }
}

/// Pure projection from language-native producer facts. Never strengthens
/// producer stage meaning or promotes candidate relations.
pub(in crate::analysis::witness) fn project_native_facts(
    facts: LanguageNativeProducerFacts,
) -> LanguageWitnessProjectionV1 {
    let mut limitations = facts.limitations;
    let mut non_claims = if facts.non_claims.is_empty() {
        vec![
            "projection_does_not_recompute_stage_meaning".to_string(),
            "canonical_mapping_does_not_grant_stage_strength".to_string(),
            "candidate_affinity_is_not_established_relation".to_string(),
        ]
    } else {
        facts.non_claims
    };

    let canonical_family = canonical_family_for_native(&facts.native_behavior_family);
    if matches!(canonical_family, CanonicalFamilyState::Unmappable { .. }) {
        limitations.push("canonical_family_unmappable".to_string());
        non_claims.push("unmappable_native_fact_retained_as_extension_or_limitation".to_string());
    }

    if facts.producer_identity.currentness == "stale_input" {
        limitations.push("stale_input".to_string());
    }
    if facts.producer_identity.currentness == "wrong_input" {
        limitations.push("wrong_input".to_string());
    }

    let owner = OwnerIdentity {
        identity: facts.owner_identity,
        source: facts.owner_source,
    };

    let reach = StageWitness::from_producer("reach", &facts.reach);
    let activation = StageWitness::from_producer("activation", &facts.activation);
    let propagation = StageWitness::from_producer("propagation", &facts.propagation);
    let observation = StageWitness::from_producer("observation", &facts.observation);
    let discrimination = StageWitness::from_producer("discrimination", &facts.discrimination);

    LanguageWitnessProjectionV1 {
        schema_version: LANGUAGE_WITNESS_PROJECTION_SCHEMA_VERSION,
        language: facts.language,
        native_behavior_family: facts.native_behavior_family,
        canonical_family,
        owner,
        candidate_relations: facts.candidate_relations,
        established_relations: facts.established_relations,
        reach,
        activation,
        propagation,
        observation,
        discrimination,
        required_discriminator: DiscriminatorIdentity {
            identity: facts.required_discriminator,
            source: facts.required_discriminator_source,
        },
        observed_discriminators: facts.observed_discriminators,
        missing_discriminators: facts.missing_discriminators,
        related_test_rows: facts.related_test_rows,
        selected_target: facts.selected_target,
        earliest_unresolved_edge: None,
        limitations,
        non_claims,
        native_extensions: facts.native_extensions,
        producer_identity: facts.producer_identity,
        semantic_digest: String::new(),
    }
    .finalize()
}

pub(in crate::analysis::witness) fn stage_yes() -> StageEvidence {
    StageEvidence::new(
        StageState::Yes,
        crate::domain::Confidence::Medium,
        "producer",
    )
}

pub(in crate::analysis::witness) fn stage_unknown() -> StageEvidence {
    StageEvidence::new(
        StageState::Unknown,
        crate::domain::Confidence::Unknown,
        "producer",
    )
}

/// Rust shared-witness bytes remain owned by the existing #4790 adapter.
pub(in crate::analysis::witness) fn shared_witness_from_rust_finding(
    finding: &Finding,
    input: AdapterInput,
) -> BehaviorEvidenceWitnessV1 {
    from_finding_with_input(finding, input)
}

pub(in crate::analysis::witness) fn canonical_family_from_probe_family(
    family: crate::domain::ProbeFamily,
) -> CanonicalFamilyState {
    canonical_family_for_native(probe_family_token(family))
}

fn probe_family_token(family: crate::domain::ProbeFamily) -> &'static str {
    match family {
        crate::domain::ProbeFamily::Predicate => "predicate_boundary",
        crate::domain::ProbeFamily::ReturnValue => "return_value",
        crate::domain::ProbeFamily::ErrorPath => "error_variant",
        crate::domain::ProbeFamily::FieldConstruction => "field_construction",
        crate::domain::ProbeFamily::SideEffect => "side_effect",
        crate::domain::ProbeFamily::MatchArm => "match_arm",
        crate::domain::ProbeFamily::CallDeletion => "call_deletion",
        crate::domain::ProbeFamily::StaticUnknown => "static_unknown",
    }
}

fn earliest_unresolved_edge(witness: &LanguageWitnessProjectionV1) -> Option<UnresolvedEdge> {
    let stages = [
        ("reach", &witness.reach),
        ("activation", &witness.activation),
        ("propagation", &witness.propagation),
        ("observation", &witness.observation),
        ("discrimination", &witness.discrimination),
    ];
    stages.into_iter().find_map(|(name, stage)| {
        if stage.state == "yes" {
            None
        } else {
            Some(UnresolvedEdge {
                stage: name.to_string(),
                detail: stage
                    .first_unresolved_edge
                    .clone()
                    .unwrap_or_else(|| format!("unresolved:{name}:{}", stage.state)),
            })
        }
    })
}

fn append_canonical_family(canonical: &mut Vec<u8>, family: &CanonicalFamilyState) {
    match family {
        CanonicalFamilyState::Mapped { canonical_family } => {
            append_field(canonical, "mapped");
            append_field(canonical, canonical_family);
        }
        CanonicalFamilyState::Unmappable { reason } => {
            append_field(canonical, "unmappable");
            append_field(canonical, reason);
        }
    }
}

fn append_relations(canonical: &mut Vec<u8>, label: &str, relations: &[RelationWitness]) {
    append_field(canonical, label);
    append_field(canonical, &relations.len().to_string());
    for relation in relations {
        append_field(canonical, &relation.semantic_identity());
    }
}

fn append_stage(canonical: &mut Vec<u8>, label: &str, stage: &StageWitness) {
    append_field(canonical, label);
    append_field(canonical, &stage.state);
    append_field(canonical, &stage.confidence);
    append_field(
        canonical,
        stage.first_unresolved_edge.as_deref().unwrap_or(""),
    );
    append_field(canonical, &stage.source_identities.len().to_string());
    for identity in &stage.source_identities {
        append_field(canonical, identity);
    }
    append_field(canonical, &stage.established_facts.len().to_string());
    for fact in &stage.established_facts {
        append_field(canonical, fact);
    }
    append_field(canonical, &stage.candidate_facts.len().to_string());
    for fact in &stage.candidate_facts {
        append_field(canonical, fact);
    }
}

fn append_field(canonical: &mut Vec<u8>, value: &str) {
    canonical.extend_from_slice(&(value.len() as u64).to_le_bytes());
    canonical.extend_from_slice(value.as_bytes());
}

fn hex_digest(canonical: &[u8]) -> String {
    let digest = Sha256::digest(canonical);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("sha256:{hex}")
}
