use sha2::{Digest, Sha256};

use super::model::*;

/// Canonical portable semantic digest for a qualification receipt.
///
/// Volatile fields such as `semantic_digest` and checkout-local path hints in
/// limitation messages prefixed with `locator:` are excluded. Layer order is
/// normalized by `layer_id` and nested list fields are sorted for portability.
pub fn adapter_substrate_qualification_semantic_digest(
    qualification: &AdapterSubstrateQualificationV1,
) -> Result<String, AdapterSubstrateQualificationErrorV1> {
    let mut layers = qualification.layers.clone();
    for layer in &mut layers {
        layer.dependency_ids.sort();
    }
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
    let mut analysis_modes = qualification.analysis_modes.clone();
    analysis_modes.sort();
    let mut non_claims = qualification.non_claims.clone();
    non_claims.sort_by(|left, right| left.claim.cmp(&right.claim));
    let mut supersession_history = qualification.supersession_history.clone();
    supersession_history.sort_by(|left, right| {
        left.superseded_qualification_id
            .cmp(&right.superseded_qualification_id)
    });
    let mut packaging = qualification.packaging.clone();
    packaging.platform_limits.sort();
    let canonical = AdapterSubstrateQualificationV1 {
        semantic_digest: None,
        layers,
        dependencies,
        limitations,
        safety_refs,
        analysis_modes,
        non_claims,
        supersession_history,
        packaging,
        ..qualification.clone()
    };
    let encoded =
        serde_json::to_vec(&canonical).map_err(|error| AdapterSubstrateQualificationErrorV1 {
            code: AdapterSubstrateQualificationErrorCodeV1::ContradictoryLayers,
            message: format!("qualification semantic serialization failed: {error}"),
        })?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}
