use super::digest::cross_language_edge_registry_semantic_digest;
use super::model::*;

/// Deserialize, validate, and bind portable registry identity for downstream
/// currentness and base/head movement analysis.
pub fn load_cross_language_edge_registry_from_json(
    text: &str,
) -> Result<LoadedCrossLanguageEdgeRegistryV1, CrossLanguageEdgeRegistryErrorV1> {
    let registry: CrossLanguageEdgeRegistryV1 =
        serde_json::from_str(text).map_err(|error| CrossLanguageEdgeRegistryErrorV1 {
            code: CrossLanguageEdgeRegistryErrorCodeV1::MalformedIdentity,
            message: format!("registry JSON parse failed: {error}"),
        })?;
    registry.validate()?;
    let semantic_digest = cross_language_edge_registry_semantic_digest(&registry)?;
    Ok(LoadedCrossLanguageEdgeRegistryV1 {
        registry,
        semantic_digest,
    })
}

impl LoadedCrossLanguageEdgeRegistryV1 {
    pub fn producer_registration(
        &self,
        producer_id: &str,
    ) -> Option<&CrossLanguageEdgeProducerRegistrationV1> {
        self.registry
            .registered_producers
            .iter()
            .find(|producer| producer.producer_id == producer_id)
    }
}
