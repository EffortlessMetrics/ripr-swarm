use std::collections::BTreeSet;

use super::digest::adapter_substrate_qualification_semantic_digest;
use super::model::*;

impl AdapterSubstrateQualificationV1 {
    pub fn validate(&self) -> Result<(), AdapterSubstrateQualificationErrorV1> {
        if self.schema_version != RIPR_ADAPTER_SUBSTRATE_QUALIFICATION_SCHEMA_VERSION {
            return Err(error(
                AdapterSubstrateQualificationErrorCodeV1::UnsupportedSchema,
                "unsupported adapter substrate qualification schema",
            ));
        }
        require_non_empty("qualification_id", &self.qualification_id)?;
        require_non_empty("controller", &self.controller)?;
        require_non_empty("language", &self.language)?;
        require_non_empty("platform", &self.platform)?;
        require_non_empty("producer_owner", &self.producer_owner)?;
        require_non_empty("source_input_identity", &self.source_input_identity)?;
        require_non_empty(
            "project_configuration_identity",
            &self.project_configuration_identity,
        )?;
        require_non_empty("platform_identity", &self.platform_identity)?;
        require_non_empty("currentness_generation", &self.currentness_generation)?;
        if self.analysis_modes.is_empty() {
            return Err(error(
                AdapterSubstrateQualificationErrorCodeV1::MissingField,
                "analysis_modes is empty",
            ));
        }
        if self.language.contains(',') {
            return Err(error(
                AdapterSubstrateQualificationErrorCodeV1::ImpossibleClaimCeiling,
                "one qualification cannot promote multiple languages",
            ));
        }
        validate_authority_ref(&self.adr_ref, "adr_ref")?;
        if self.adr_ref.currentness == AuthorityCurrentnessV1::Stale {
            return Err(error(
                AdapterSubstrateQualificationErrorCodeV1::StaleAuthorityReference,
                "adr_ref is stale",
            ));
        }
        for safety_ref in &self.safety_refs {
            validate_safety_ref(safety_ref)?;
        }
        validate_input_behavior(&self.input_behavior)?;
        validate_packaging(self)?;
        validate_dependencies(&self.dependencies)?;
        validate_layers(self)?;
        validate_substrate_class_laws(self)?;
        validate_claim_ceiling(self)?;
        if let Some(digest) = &self.semantic_digest {
            require_sha256("semantic_digest", digest)?;
            let expected = adapter_substrate_qualification_semantic_digest(self)?;
            if digest != &expected {
                return Err(error(
                    AdapterSubstrateQualificationErrorCodeV1::IdentityMismatch,
                    "semantic_digest does not match canonical qualification content",
                ));
            }
        }
        Ok(())
    }
}

/// Returns whether two qualifications may share one implementation family without
/// collapsing distinct language capability cells.
pub fn substrate_classes_compatible(
    left: &AdapterSubstrateQualificationV1,
    right: &AdapterSubstrateQualificationV1,
) -> bool {
    left.shared_adapter_family_id.is_some()
        && left.shared_adapter_family_id == right.shared_adapter_family_id
        && left.language != right.language
}

fn validate_substrate_class_laws(
    qualification: &AdapterSubstrateQualificationV1,
) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    if qualification.substrate_class == AdapterSubstrateClassV1::InProcessParser
        && qualification
            .layers
            .iter()
            .any(|layer| layer.layer_class == AdapterSubstrateLayerClassV1::CompilerResolved)
        && qualification.claim_ceiling.ceiling == SemanticClaimCeilingV1::SyntaxFactsOnly
    {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::ContradictoryLayers,
            "in-process parser cannot expose compiler-resolved layers under a syntax-only ceiling",
        ));
    }
    if qualification.substrate_class == AdapterSubstrateClassV1::GeneratedParser
        && !has_generated_parser_packaging_disposition(&qualification.packaging)
    {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::MissingPackagingDisposition,
            "generated parser requires build_script and generated_code disposition",
        ));
    }
    if qualification.substrate_class == AdapterSubstrateClassV1::ExternalHelper
        && qualification
            .required_fallback
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::MissingField,
            "external helper requires required_fallback for syntax-only safe fallback",
        ));
    }
    if qualification.substrate_class == AdapterSubstrateClassV1::PacketProducerConsumer
        && !matches!(
            qualification.claim_ceiling.ceiling,
            SemanticClaimCeilingV1::PacketFactsOnly
                | SemanticClaimCeilingV1::NoLocalSemanticAuthority
        )
    {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::ImpossibleClaimCeiling,
            "packet producer cannot establish local compiler or syntax authority",
        ));
    }
    if qualification.substrate_class == AdapterSubstrateClassV1::NativeLibrary
        && qualification.packaging.platform_limits.is_empty()
    {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::MissingPackagingDisposition,
            "native library requires explicit platform_limits",
        ));
    }
    if qualification.substrate_class == AdapterSubstrateClassV1::Hybrid
        && qualification.limitations.is_empty()
    {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::ImpossibleClaimCeiling,
            "hybrid substrate cannot promote every member without typed limitations",
        ));
    }
    Ok(())
}

fn validate_claim_ceiling(
    qualification: &AdapterSubstrateQualificationV1,
) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    require_non_empty(
        "claim_ceiling.message",
        &qualification.claim_ceiling.message,
    )?;
    for layer in &qualification.layers {
        if layer.layer_class == AdapterSubstrateLayerClassV1::SyntaxOnly
            && matches!(
                layer.claim_ceiling.ceiling,
                SemanticClaimCeilingV1::CompilerFactsWithFallback
                    | SemanticClaimCeilingV1::NativePlatformLimited
            )
        {
            return Err(error(
                AdapterSubstrateQualificationErrorCodeV1::ImpossibleClaimCeiling,
                "syntax-only layer cannot inherit compiler-resolved claim ceilings",
            ));
        }
    }
    Ok(())
}

fn has_generated_parser_packaging_disposition(
    packaging: &SubstratePackagingImplicationsV1,
) -> bool {
    let build_script = packaging.build_script.trim();
    let generated_code = packaging.generated_code.trim();
    !build_script.is_empty()
        && !generated_code.is_empty()
        && !build_script.eq_ignore_ascii_case("none")
        && !generated_code.eq_ignore_ascii_case("none")
}

fn validate_layers(
    qualification: &AdapterSubstrateQualificationV1,
) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    if qualification.layers.is_empty() {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::MissingField,
            "layers is empty",
        ));
    }
    let dependency_ids: BTreeSet<_> = qualification
        .dependencies
        .iter()
        .map(|dependency| dependency.dependency_id.clone())
        .collect();
    let mut layer_ids = BTreeSet::new();
    for layer in &qualification.layers {
        if !layer_ids.insert(layer.layer_id.clone()) {
            return Err(error(
                AdapterSubstrateQualificationErrorCodeV1::DuplicateIdentity,
                "layers contains duplicate layer_id values",
            ));
        }
        require_non_empty("layer_id", &layer.layer_id)?;
        require_non_empty("claim_ceiling.message", &layer.claim_ceiling.message)?;
        for dependency_id in &layer.dependency_ids {
            if !dependency_ids.contains(dependency_id) {
                return Err(error(
                    AdapterSubstrateQualificationErrorCodeV1::AuthorityViolation,
                    "layer references unknown dependency_id",
                ));
            }
        }
    }
    Ok(())
}

fn validate_dependencies(
    dependencies: &[AdapterSubstrateDependencyRefV1],
) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    let mut ids = BTreeSet::new();
    for dependency in dependencies {
        if !ids.insert(dependency.dependency_id.clone()) {
            return Err(error(
                AdapterSubstrateQualificationErrorCodeV1::DuplicateIdentity,
                "dependencies contains duplicate dependency_id values",
            ));
        }
        require_non_empty("dependency_id", &dependency.dependency_id)?;
        require_non_empty("crate_or_tool", &dependency.crate_or_tool)?;
        require_non_empty("exact_version", &dependency.exact_version)?;
        require_non_empty(
            "grammar_or_schema_generation",
            &dependency.grammar_or_schema_generation,
        )?;
    }
    Ok(())
}

fn validate_packaging(
    qualification: &AdapterSubstrateQualificationV1,
) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    let packaging = &qualification.packaging;
    require_non_empty("license_spdx", &packaging.license_spdx)?;
    require_non_empty("msrv", &packaging.msrv)?;
    require_non_empty("unsafe_surface", &packaging.unsafe_surface)?;
    require_non_empty("build_script", &packaging.build_script)?;
    require_non_empty("generated_code", &packaging.generated_code)?;
    require_non_empty("native_runtime", &packaging.native_runtime)?;
    require_non_empty("compile_size_class", &packaging.compile_size_class)?;
    require_non_empty("binary_size_class", &packaging.binary_size_class)?;
    require_non_empty("distribution_notes", &packaging.distribution_notes)?;
    if qualification.substrate_class == AdapterSubstrateClassV1::InProcessParser
        && packaging.license_spdx.eq_ignore_ascii_case("unknown")
    {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::MissingPackagingDisposition,
            "license disposition cannot remain unknown for a qualified substrate",
        ));
    }
    Ok(())
}

fn validate_input_behavior(
    profile: &SubstrateInputBehaviorProfileV1,
) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    require_non_empty("exact_span", &profile.exact_span)?;
    require_non_empty("malformed_partial_input", &profile.malformed_partial_input)?;
    require_non_empty("source_map", &profile.source_map)?;
    require_non_empty("generated_code", &profile.generated_code)?;
    require_non_empty("unavailable_tool", &profile.unavailable_tool)?;
    if profile
        .unavailable_tool
        .eq_ignore_ascii_case("clean_result")
    {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::AuthorityViolation,
            "missing tool/runtime state cannot become a clean result",
        ));
    }
    Ok(())
}

fn validate_authority_ref(
    authority: &AdapterSubstrateAuthorityRefV1,
    field: &str,
) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    require_non_empty(&format!("{field}.ref_id"), &authority.ref_id)?;
    require_non_empty(
        &format!("{field}.ref_generation"),
        &authority.ref_generation,
    )?;
    Ok(())
}

fn validate_safety_ref(
    safety_ref: &AdapterSubstrateSafetyRefV1,
) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    require_non_empty("receipt_id", &safety_ref.receipt_id)?;
    require_non_empty("harness_generation", &safety_ref.harness_generation)?;
    if safety_ref.currentness == AuthorityCurrentnessV1::Stale {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::StaleAuthorityReference,
            "safety receipt reference is stale",
        ));
    }
    Ok(())
}

fn require_sha256(field: &str, value: &str) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    let _ = canonical_sha256_digest(value).map_err(|message| {
        error(
            AdapterSubstrateQualificationErrorCodeV1::IdentityMismatch,
            format!("{field}: {message}"),
        )
    })?;
    Ok(())
}

fn canonical_sha256_digest(value: &str) -> Result<String, String> {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return Err("use sha256: followed by 64 hexadecimal digits".into());
    };
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("use sha256: followed by 64 hexadecimal digits".into());
    }
    Ok(format!("sha256:{}", digest.to_ascii_lowercase()))
}

fn require_non_empty(field: &str, value: &str) -> Result<(), AdapterSubstrateQualificationErrorV1> {
    if value.trim().is_empty() {
        return Err(error(
            AdapterSubstrateQualificationErrorCodeV1::MissingField,
            format!("{field} is empty"),
        ));
    }
    Ok(())
}

fn error(
    code: AdapterSubstrateQualificationErrorCodeV1,
    message: impl Into<String>,
) -> AdapterSubstrateQualificationErrorV1 {
    AdapterSubstrateQualificationErrorV1 {
        code,
        message: message.into(),
    }
}
