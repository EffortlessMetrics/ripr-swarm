use std::collections::BTreeSet;

use super::model::*;

impl CrossLanguageEdgeRegistryV1 {
    pub fn validate(&self) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
        if self.schema_version != RIPR_CROSS_LANGUAGE_EDGE_REGISTRY_SCHEMA_VERSION {
            return Err(error(
                CrossLanguageEdgeRegistryErrorCodeV1::UnsupportedSchema,
                "unsupported cross-language edge registry schema",
            ));
        }
        require_non_empty("proof_generation", &self.proof_generation)?;
        require_non_empty("currentness_generation", &self.currentness_generation)?;
        if self.supported_boundary_families.is_empty() {
            return Err(error(
                CrossLanguageEdgeRegistryErrorCodeV1::MissingField,
                "supported_boundary_families is empty",
            ));
        }
        let supported: BTreeSet<_> = self.supported_boundary_families.iter().copied().collect();
        if supported.len() != self.supported_boundary_families.len() {
            return Err(error(
                CrossLanguageEdgeRegistryErrorCodeV1::DuplicateEdgeId,
                "supported_boundary_families contains duplicates",
            ));
        }
        let mut producer_ids = BTreeSet::new();
        for producer in &self.registered_producers {
            if !producer_ids.insert(producer.producer_id.clone()) {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::DuplicateEdgeId,
                    "registered_producers contains duplicate producer_id values",
                ));
            }
            validate_producer_registration(producer, &supported)?;
        }
        let mut edge_ids = BTreeSet::new();
        for edge in &self.edges {
            if !edge_ids.insert(edge.edge_id.clone()) {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::DuplicateEdgeId,
                    "registry contains duplicate edge_id values",
                ));
            }
            validate_edge(edge, &supported, &self.registered_producers)?;
        }
        if let Some(digest) = &self.semantic_digest {
            require_sha256("semantic_digest", digest)?;
            let expected = super::digest::cross_language_edge_registry_semantic_digest(self)?;
            if digest != &expected {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::IdentityMismatch,
                    "semantic_digest does not match canonical registry content",
                ));
            }
        }
        Ok(())
    }
}

impl CrossLanguageSubjectV1 {
    pub fn validate(&self) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
        require_non_empty("language", &self.language)?;
        require_non_empty("subject_id", &self.subject_id)?;
        require_sha256("subject_body_digest", &self.subject_body_digest)?;
        require_optional_non_empty("display_name", self.display_name.as_deref())?;
        self.project.validate()
    }

    pub fn portable_identity_key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            length_prefixed_segment(&self.language),
            length_prefixed_segment(&self.subject_id),
            identity_sha256_component(&self.subject_body_digest),
            self.project.portable_key()
        )
    }
}

impl CrossLanguageProjectIdentityV1 {
    pub fn validate(&self) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
        require_non_empty("project_id", &self.project_id)?;
        require_optional_non_empty("package_id", self.package_id.as_deref())?;
        require_sha256("config_digest", &self.config_digest)
    }

    pub fn portable_key(&self) -> String {
        format!(
            "{}|{}|{}",
            length_prefixed_segment(&self.project_id),
            length_prefixed_segment(self.package_id.as_deref().unwrap_or("")),
            identity_sha256_component(&self.config_digest)
        )
    }
}

impl CrossLanguageGeneratedArtifactV1 {
    pub fn validate(&self) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
        require_non_empty("artifact_id", &self.artifact_id)?;
        require_non_empty("generator_id", &self.generator_id)?;
        require_sha256("input_digest", &self.input_digest)?;
        require_sha256("output_digest", &self.output_digest)
    }
}

/// Returns `true` only when a registered edge explicitly connects the two
/// subjects. Same display name or subject_id across languages is not enough.
pub fn subjects_related_by_registered_edge(
    left: &CrossLanguageSubjectV1,
    right: &CrossLanguageSubjectV1,
    registry: &CrossLanguageEdgeRegistryV1,
) -> bool {
    registry.edges.iter().any(|edge| {
        edge.disposition == EdgeDispositionV1::PositiveRegistered
            && edge_matches_subjects(edge, left, right)
    })
}

fn edge_matches_subjects(
    edge: &CrossLanguageEdgeV1,
    left: &CrossLanguageSubjectV1,
    right: &CrossLanguageSubjectV1,
) -> bool {
    let (source, target) = (&edge.source, &edge.target);
    (subject_same(source, left) && subject_same(target, right))
        || (subject_same(source, right) && subject_same(target, left))
}

fn subject_same(left: &CrossLanguageSubjectV1, right: &CrossLanguageSubjectV1) -> bool {
    left.language == right.language
        && left.subject_id == right.subject_id
        && canonical_sha256_digest(&left.subject_body_digest)
            == canonical_sha256_digest(&right.subject_body_digest)
        && project_same(&left.project, &right.project)
}

fn project_same(
    left: &CrossLanguageProjectIdentityV1,
    right: &CrossLanguageProjectIdentityV1,
) -> bool {
    left.project_id == right.project_id
        && left.package_id == right.package_id
        && canonical_sha256_digest(&left.config_digest)
            == canonical_sha256_digest(&right.config_digest)
}

fn validate_producer_registration(
    producer: &CrossLanguageEdgeProducerRegistrationV1,
    supported: &BTreeSet<BoundaryFamilyV1>,
) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    require_non_empty("producer_id", &producer.producer_id)?;
    require_non_empty("producer_version", &producer.producer_version)?;
    if producer.supported_families.is_empty() {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::MissingField,
            "registered producer supported_families is empty",
        ));
    }
    for family in &producer.supported_families {
        if !supported.contains(family) {
            return Err(error(
                CrossLanguageEdgeRegistryErrorCodeV1::UnsupportedDisposition,
                "producer references a boundary family not listed on the registry",
            ));
        }
    }
    Ok(())
}

fn validate_edge(
    edge: &CrossLanguageEdgeV1,
    supported: &BTreeSet<BoundaryFamilyV1>,
    producers: &[CrossLanguageEdgeProducerRegistrationV1],
) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    require_non_empty("edge_id", &edge.edge_id)?;
    edge.source.validate()?;
    edge.target.validate()?;
    if subject_same(&edge.source, &edge.target) {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::ContradictoryDirection,
            "source and target subjects must differ",
        ));
    }
    if edge.disposition == EdgeDispositionV1::PositiveRegistered
        && edge.source.language == edge.target.language
    {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::UnsupportedDisposition,
            "positive registered edges require distinct source and target languages",
        ));
    }
    if !supported.contains(&edge.boundary_family) {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::UnsupportedDisposition,
            "edge boundary_family is not supported on this registry",
        ));
    }
    validate_direction(edge)?;
    validate_family_strength(edge)?;
    validate_strength_ceiling(edge)?;
    validate_disposition(edge)?;
    for artifact in &edge.generated_artifacts {
        artifact.validate()?;
    }
    for obligation in &edge.obligations {
        require_non_empty("obligation_id", &obligation.obligation_id)?;
        require_non_empty("description", &obligation.description)?;
    }
    for limitation in &edge.limitations {
        require_non_empty("limitation_id", &limitation.limitation_id)?;
        require_non_empty("category", &limitation.category)?;
        require_non_empty("message", &limitation.message)?;
    }
    for non_claim in &edge.non_claims {
        require_non_empty("claim", &non_claim.claim)?;
        require_non_empty("reason", &non_claim.reason)?;
    }
    match edge.disposition {
        EdgeDispositionV1::PositiveRegistered => {
            require_registered_producer(edge, producers)?;
            if edge.producer_basis != EdgeProducerBasisV1::RegisteredProducerFact {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::AuthorityViolation,
                    "positive registered edges require registered_producer_fact basis",
                ));
            }
            if edge.effective_strength == EdgeStrengthV1::Unsupported {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::IncompatibleFamilyStrength,
                    "positive registered edges cannot remain unsupported",
                ));
            }
            if edge.first_unresolved_leg.is_some() {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::CompletenessConflict,
                    "positive registered edges cannot carry first_unresolved_leg",
                ));
            }
            if edge.currentness != EdgeCurrentnessV1::Current {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::CompletenessConflict,
                    "positive registered edges require current currentness",
                ));
            }
        }
        EdgeDispositionV1::UnsupportedFamily => {
            if edge.producer_basis == EdgeProducerBasisV1::RegisteredProducerFact {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::AuthorityViolation,
                    "unsupported family edges cannot claim registered producer facts",
                ));
            }
        }
        EdgeDispositionV1::UnresolvedLeg => {
            if edge
                .first_unresolved_leg
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::MissingField,
                    "unresolved-leg edges require first_unresolved_leg",
                ));
            }
        }
        EdgeDispositionV1::Stale => {
            if edge.currentness != EdgeCurrentnessV1::Stale {
                return Err(error(
                    CrossLanguageEdgeRegistryErrorCodeV1::StaleCurrentness,
                    "stale disposition requires stale currentness",
                ));
            }
        }
    }
    Ok(())
}

fn validate_direction(edge: &CrossLanguageEdgeV1) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    if edge.direction == CrossLanguageEdgeDirectionV1::Bidirectional
        && edge.boundary_family == BoundaryFamilyV1::SubprocessCli
    {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::ContradictoryDirection,
            "subprocess_cli edges cannot be bidirectional call propagation",
        ));
    }
    Ok(())
}

fn validate_family_strength(
    edge: &CrossLanguageEdgeV1,
) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    let ceiling = family_strength_ceiling(edge.boundary_family);
    if edge.declared_strength > ceiling || edge.effective_strength > ceiling {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::IncompatibleFamilyStrength,
            "edge strength exceeds boundary-family ceiling",
        ));
    }
    if edge.propagation_ceiling > ceiling {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::IncompatibleFamilyStrength,
            "propagation ceiling exceeds boundary-family ceiling",
        ));
    }
    if matches!(
        edge.boundary_family,
        BoundaryFamilyV1::SchemaProtocol
            | BoundaryFamilyV1::SharedResource
            | BoundaryFamilyV1::SerializationContract
    ) && edge.effective_strength > EdgeStrengthV1::VerificationObligation
    {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::IncompatibleFamilyStrength,
            "schema/resource/serialization families cannot claim in-process propagation",
        ));
    }
    if edge.boundary_family == BoundaryFamilyV1::UnknownDynamicBoundary
        && edge.disposition == EdgeDispositionV1::PositiveRegistered
    {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::UnsupportedDisposition,
            "unknown_dynamic_boundary cannot be positively registered",
        ));
    }
    Ok(())
}

fn family_strength_ceiling(family: BoundaryFamilyV1) -> EdgeStrengthV1 {
    match family {
        BoundaryFamilyV1::SubprocessCli => EdgeStrengthV1::ProcessBoundary,
        BoundaryFamilyV1::SchemaProtocol
        | BoundaryFamilyV1::SerializationContract
        | BoundaryFamilyV1::SharedResource => EdgeStrengthV1::VerificationObligation,
        BoundaryFamilyV1::UnknownDynamicBoundary => EdgeStrengthV1::Unsupported,
        _ => EdgeStrengthV1::OracleEstablished,
    }
}

fn validate_strength_ceiling(
    edge: &CrossLanguageEdgeV1,
) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    let weakest = weakest_strength(edge);
    if edge.effective_strength > weakest {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::IncompatibleFamilyStrength,
            "effective_strength exceeds weakest unresolved leg",
        ));
    }
    if edge.propagation_ceiling > weakest {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::IncompatibleFamilyStrength,
            "propagation_ceiling exceeds weakest unresolved leg",
        ));
    }
    Ok(())
}

fn weakest_strength(edge: &CrossLanguageEdgeV1) -> EdgeStrengthV1 {
    let mut weakest = edge.declared_strength.min(edge.propagation_ceiling);
    if edge.first_unresolved_leg.is_some() {
        weakest = weakest.min(EdgeStrengthV1::RegisteredBinding);
    }
    if edge.currentness == EdgeCurrentnessV1::Stale {
        weakest = EdgeStrengthV1::Unsupported;
    }
    if edge.currentness == EdgeCurrentnessV1::PartialFacts {
        weakest = weakest.min(EdgeStrengthV1::VerificationObligation);
    }
    for obligation in &edge.obligations {
        weakest = weakest.min(obligation.required_strength);
    }
    weakest.min(family_strength_ceiling(edge.boundary_family))
}

fn validate_disposition(
    edge: &CrossLanguageEdgeV1,
) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    if edge.disposition == EdgeDispositionV1::PositiveRegistered
        && matches!(edge.exactness, EdgeExactnessV1::Unsupported)
    {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::CompletenessConflict,
            "positive registered edges cannot remain unsupported exactness",
        ));
    }
    Ok(())
}

fn require_registered_producer(
    edge: &CrossLanguageEdgeV1,
    producers: &[CrossLanguageEdgeProducerRegistrationV1],
) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    require_non_empty("producer_id", &edge.producer_id)?;
    let Some(producer) = producers
        .iter()
        .find(|producer| producer.producer_id == edge.producer_id)
    else {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::AuthorityViolation,
            "positive edge references an unregistered producer_id",
        ));
    };
    if !producer.supported_families.contains(&edge.boundary_family) {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::AuthorityViolation,
            "producer does not admit the edge boundary family",
        ));
    }
    Ok(())
}

fn require_sha256(field: &str, value: &str) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    let _ = canonical_sha256_digest(value).map_err(|message| {
        error(
            CrossLanguageEdgeRegistryErrorCodeV1::MalformedIdentity,
            format!("{field}: {message}"),
        )
    })?;
    Ok(())
}

fn identity_sha256_component(value: &str) -> String {
    canonical_sha256_digest(value).unwrap_or_else(|_| value.to_string())
}

fn canonical_sha256_digest(value: &str) -> Result<String, String> {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return Err("must use sha256:<64-hex>".into());
    };
    if digest.len() != 64 || !is_hex(digest) {
        return Err("must use sha256:<64-hex>".into());
    }
    Ok(format!("sha256:{}", digest.to_ascii_lowercase()))
}

fn length_prefixed_segment(value: &str) -> String {
    format!("{}:{}", value.len(), value)
}

fn is_hex(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn require_non_empty(field: &str, value: &str) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    if value.trim().is_empty() {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::MissingField,
            format!("{field} is empty"),
        ));
    }
    Ok(())
}

fn require_optional_non_empty(
    field: &str,
    value: Option<&str>,
) -> Result<(), CrossLanguageEdgeRegistryErrorV1> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(error(
            CrossLanguageEdgeRegistryErrorCodeV1::MissingField,
            format!("{field} is empty"),
        ));
    }
    Ok(())
}

fn error(
    code: CrossLanguageEdgeRegistryErrorCodeV1,
    message: impl Into<String>,
) -> CrossLanguageEdgeRegistryErrorV1 {
    CrossLanguageEdgeRegistryErrorV1 {
        code,
        message: message.into(),
    }
}
