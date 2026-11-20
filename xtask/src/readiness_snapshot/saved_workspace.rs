use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::Deserialize;

use super::identity::digest_bytes;
use super::model::{
    ReadinessDimension, ReadinessDimensionId, ReadinessDimensionState, ReadinessEvidenceRecord,
    not_proven_dimension,
};

const ARTIFACT: &str = "target/ripr/reports/lsp-performance.json";
const COMMAND: &str = "cargo xtask lsp-performance-report";
const SCHEMA: &str = crate::reports::LSP_PERFORMANCE_SCHEMA;
const FRESHNESS: &str = "the producer-reported source SHA must match the snapshot commit";
const STEPS: [&str; 11] = [
    "cold_start",
    "unchanged_save",
    "unchanged_refresh",
    "production_edit",
    "related_test_edit",
    "unrelated_test_edit",
    "rename",
    "config_change",
    "cancellation",
    "corrupt_cache",
    "explicit_full_refresh",
];

#[derive(Deserialize)]
struct SavedEditReceipt {
    schema_version: String,
    tool: String,
    report: String,
    identity: SavedEditIdentity,
    steps: Vec<SavedEditStep>,
    claim_boundary: String,
}

#[derive(Deserialize)]
struct SavedEditIdentity {
    source_sha: String,
    binary_path: String,
    binary_digest: String,
    host_class: String,
    cache_reset_procedure: String,
    sample_count: u32,
}

#[derive(Deserialize)]
struct SavedEditStep {
    step: String,
}

pub(super) fn project(root: &Path, source_sha: &str, tree_clean: bool) -> ReadinessDimension {
    let id = ReadinessDimensionId::SavedWorkspaceLifecycle;
    let mut dimension = not_proven_dimension(
        source_sha,
        tree_clean,
        id,
        "no saved-edit sequence receipt found",
        COMMAND,
    );
    dimension.owners.push("#1578".to_string());

    let bytes = match fs::read(root.join(ARTIFACT)) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return dimension,
        Err(_) => {
            dimension.limitations =
                vec!["saved-edit sequence receipt could not be read".to_string()];
            return dimension;
        }
    };

    let mut evidence = ReadinessEvidenceRecord {
        producer: "lsp-performance-report".to_string(),
        schema_version: "unrecognized".to_string(),
        command: COMMAND.to_string(),
        artifact: ARTIFACT.to_string(),
        artifact_digest: Some(digest_bytes(&bytes)),
        freshness_policy: FRESHNESS.to_string(),
        stale_reason: None,
    };
    let receipt = serde_json::from_slice::<SavedEditReceipt>(&bytes)
        .ok()
        .filter(|receipt| {
            receipt.schema_version == SCHEMA
                && receipt.tool == "ripr"
                && receipt.report == "lsp-performance"
        });
    if let Some(receipt) = receipt {
        evidence.schema_version = SCHEMA.to_string();
        apply_receipt(&mut dimension, &mut evidence, &receipt);
    } else {
        dimension.limitations =
            vec!["malformed or unsupported saved-edit sequence receipt".to_string()];
    }
    dimension.evidence.push(evidence);
    dimension
}

fn apply_receipt(
    dimension: &mut ReadinessDimension,
    evidence: &mut ReadinessEvidenceRecord,
    receipt: &SavedEditReceipt,
) {
    let identity = &receipt.identity;
    if identity.source_sha.len() != dimension.source_sha.len()
        || !identity
            .source_sha
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        dimension.limitations = vec!["saved-edit receipt lacks a full source SHA".to_string()];
        return;
    }
    if identity.source_sha != dimension.source_sha {
        let reason = format!(
            "saved-edit receipt source SHA {} differs from the snapshot commit",
            identity.source_sha
        );
        dimension.state = ReadinessDimensionState::Stale;
        evidence.stale_reason = Some(reason.clone());
        dimension.limitations = vec![reason];
        return;
    }

    let steps: BTreeSet<_> = receipt
        .steps
        .iter()
        .map(|step| step.step.as_str())
        .collect();
    let covered = STEPS.iter().filter(|step| steps.contains(**step)).count();
    let denominator = format!(
        "{} sample(s), {} steps covered out of {} expected ({} rows)",
        identity.sample_count,
        covered,
        STEPS.len(),
        receipt.steps.len(),
    );
    let metadata = [
        &identity.binary_path,
        &identity.binary_digest,
        &identity.host_class,
        &identity.cache_reset_procedure,
        &receipt.claim_boundary,
    ];
    if identity.sample_count == 0
        || covered != STEPS.len()
        || receipt.steps.len() != STEPS.len()
        || metadata.iter().any(|value| value.trim().is_empty())
    {
        dimension.limitations = vec![format!(
            "saved-edit sequence receipt has incomplete fixture evidence ({denominator})"
        )];
        return;
    }

    dimension.state = ReadinessDimensionState::Limited;
    dimension.limitations = vec![
        format!("producer reports a saved-edit fixture sequence ({denominator})"),
        "the receipt records a Git SHA but does not attest producer checkout cleanliness or test-binary custody".to_string(),
    ];
    dimension.non_claims =
        vec!["no installed-client lifecycle or latency qualification".to_string()];
    dimension.next_proof = vec![
        "retain an exact-source installed-client lifecycle receipt with checkout and binary identity".to_string(),
    ];
}
