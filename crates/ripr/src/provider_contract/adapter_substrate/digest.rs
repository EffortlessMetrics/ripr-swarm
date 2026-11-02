use sha2::{Digest, Sha256};

use super::model::*;

/// Canonical portable semantic digest for a qualification receipt.
///
/// Volatile fields such as `semantic_digest` and checkout-local path hints in
/// limitation messages prefixed with `locator:` are excluded. Layer order is
/// normalized by `layer_id`.
pub fn adapter_substrate_qualification_semantic_digest(
    qualification: &AdapterSubstrateQualificationV1,
) -> Result<String, AdapterSubstrateQualificationErrorV1> {
    let mut layers = qualification.layers.clone();
    layers.sort_by(|left, right| left.layer_id.cmp(&right.layer_id));
    let mut dependencies = qualification.dependencies.clone();
    dependencies.sort_by(|left, right| left.dependency_id.cmp(&right.dependency_id));
    let mut limitations = qualification.limitations.clone();
    for limitation in &mut limitations {
        if let Some((head, _)) = limitation.message.split_once("|locator:") {
            limitation.message = head.to_string();
        }
    }
    limitations.sort_by(|left, right| left.limitation_id.cmp(&right.limitation_id));
    let mut safety_refs = qualification.safety_refs.clone();
    safety_refs.sort_by(|left, right| left.receipt_id.cmp(&right.receipt_id));
    let canonical = AdapterSubstrateQualificationV1 {
        semantic_digest: None,
        layers,
        dependencies,
        limitations,
        safety_refs,
        ..qualification.clone()
    };
    let encoded =
        serde_json::to_vec(&canonical).map_err(|error| AdapterSubstrateQualificationErrorV1 {
            code: AdapterSubstrateQualificationErrorCodeV1::ContradictoryLayers,
            message: format!("qualification semantic serialization failed: {error}"),
        })?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}
