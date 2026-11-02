//! Cache payload tamper/recovery corpus contract (#4515, #4507).
//!
//! The exact-candidate runner and evidence packet emission remain follow-up
//! work under the shared #4510 harness. This module owns the manifest schema,
//! offline validation, and fixture-contract registration only.

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub(crate) const CORPUS_SCHEMA: &str = "cache_payload_tamper_recovery_corpus.v1";
pub(crate) const CORPUS_RELATIVE: &str = "fixtures/cache-payload-tamper-recovery/corpus.json";

pub(crate) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub(crate) fn corpus_path() -> PathBuf {
    workspace_root().join(CORPUS_RELATIVE)
}

pub(crate) const REQUIRED_FAMILY_IDS: [&str; 6] = [
    "repository_file_facts",
    "full_classified_seam_records",
    "compact_classified_seam_records",
    "sharded_classified_records_and_shard_set_metadata",
    "file_fact_inventory_admission_metadata",
    "corpus_fingerprint_mapping_envelope",
];

pub(crate) const REQUIRED_TAMPER_OPERATION_IDS: [&str; 11] = [
    "rename_related_test",
    "rename_owner_or_canonical_subject",
    "alter_classification_or_evidence_field",
    "remove_integrity_field",
    "malformed_digest",
    "stale_or_wrong_digest",
    "corrupt_one_shard_member",
    "corrupt_shard_set_or_inventory_metadata",
    "foreign_or_legacy_schema_identity_control",
    "truncated_json_decode_control",
    "re_sealed_unkeyed_checksum_boundary",
];

pub(crate) const REQUIRED_CONTROL_IDS: [&str; 9] = [
    "disable_checksum_validation",
    "validate_envelope_key_only",
    "corrupt_one_shard_while_siblings_valid",
    "remove_recovery_rewrite",
    "unchanged_current_entries_remain_hits",
    "re_sealed_changed_payload_nonauthentication",
    "nonzero_produced_and_consumed_rows",
    "candidate_or_schema_movement_stales_packet",
    "cancellation_during_recompute_store",
];

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FamilyDisposition {
    IncludedAndExercised,
    NotPersistedOrNotApplicable,
    ExplicitlyExcludedWithNonClaim,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExactCandidateExecution {
    NotRun,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FamilyRow {
    pub(crate) id: String,
    pub(crate) disposition: FamilyDisposition,
    #[serde(default)]
    pub(crate) library_producer_test: Option<String>,
    #[serde(default)]
    pub(crate) library_consumer_test: Option<String>,
    pub(crate) exact_candidate_execution: ExactCandidateExecution,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TamperOperationRow {
    pub(crate) id: String,
    pub(crate) library_coverage: String,
    pub(crate) exact_candidate_execution: ExactCandidateExecution,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ControlRow {
    pub(crate) id: String,
    pub(crate) exact_candidate_execution: ExactCandidateExecution,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Corpus {
    pub(crate) schema_version: String,
    pub(crate) owner_issues: Vec<u32>,
    pub(crate) non_claim: String,
    pub(crate) families: Vec<FamilyRow>,
    pub(crate) tamper_operations: Vec<TamperOperationRow>,
    pub(crate) required_controls: Vec<ControlRow>,
}

pub(crate) fn load_corpus(body: &str) -> Result<Corpus, String> {
    serde_json::from_str(body).map_err(|error| format!("corpus JSON: {error}"))
}

pub(crate) fn load_committed_corpus() -> Result<Corpus, String> {
    let path = corpus_path();
    let body = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    load_corpus(&body)
}

pub(crate) fn validate_corpus(corpus: &Corpus) -> Result<(), String> {
    if corpus.schema_version != CORPUS_SCHEMA {
        return Err(format!(
            "schema_version must be {CORPUS_SCHEMA}, got {}",
            corpus.schema_version
        ));
    }
    if !corpus.owner_issues.contains(&4515) {
        return Err("owner_issues must include 4515".to_string());
    }
    if corpus.non_claim.trim().is_empty() {
        return Err("non_claim must be nonempty".to_string());
    }
    validate_unique_ids(corpus.families.iter().map(|row| row.id.as_str()), "family")?;
    for required in REQUIRED_FAMILY_IDS {
        let Some(row) = corpus.families.iter().find(|row| row.id == required) else {
            return Err(format!("missing required family `{required}`"));
        };
        match row.disposition {
            FamilyDisposition::IncludedAndExercised => {
                for label in ["library_producer_test", "library_consumer_test"] {
                    let anchor = match label {
                        "library_producer_test" => row.library_producer_test.as_deref(),
                        _ => row.library_consumer_test.as_deref(),
                    };
                    let Some(anchor) = anchor else {
                        return Err(format!(
                            "family `{required}` must name {label} when included_and_exercised"
                        ));
                    };
                    validate_test_anchor(anchor)?;
                }
            }
            FamilyDisposition::NotPersistedOrNotApplicable
            | FamilyDisposition::ExplicitlyExcludedWithNonClaim => {}
        }
        if row.exact_candidate_execution != ExactCandidateExecution::NotRun {
            return Err(format!(
                "family `{required}` exact_candidate_execution must remain not_run in this corpus slice"
            ));
        }
    }
    validate_unique_ids(
        corpus.tamper_operations.iter().map(|row| row.id.as_str()),
        "tamper operation",
    )?;
    for required in REQUIRED_TAMPER_OPERATION_IDS {
        let Some(row) = corpus
            .tamper_operations
            .iter()
            .find(|row| row.id == required)
        else {
            return Err(format!("missing required tamper operation `{required}`"));
        };
        if row.exact_candidate_execution != ExactCandidateExecution::NotRun {
            return Err(format!(
                "tamper operation `{required}` exact_candidate_execution must remain not_run in this corpus slice"
            ));
        }
        if row.library_coverage.trim().is_empty() {
            return Err(format!(
                "tamper operation `{required}` must name library_coverage"
            ));
        }
    }
    validate_unique_ids(
        corpus.required_controls.iter().map(|row| row.id.as_str()),
        "required control",
    )?;
    for required in REQUIRED_CONTROL_IDS {
        let Some(row) = corpus
            .required_controls
            .iter()
            .find(|row| row.id == required)
        else {
            return Err(format!("missing required control `{required}`"));
        };
        if row.exact_candidate_execution != ExactCandidateExecution::NotRun {
            return Err(format!(
                "required control `{required}` exact_candidate_execution must remain not_run in this corpus slice"
            ));
        }
    }
    Ok(())
}

fn validate_unique_ids<'a>(ids: impl Iterator<Item = &'a str>, label: &str) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(format!("duplicate {label} id `{id}`"));
        }
    }
    Ok(())
}

fn validate_test_anchor(anchor: &str) -> Result<(), String> {
    let Some((path, symbol)) = anchor.split_once("::") else {
        return Err(format!(
            "test anchor `{anchor}` must use path::module::...::symbol form"
        ));
    };
    if symbol.trim().is_empty() {
        return Err(format!("test anchor `{anchor}` must name a symbol"));
    }
    if !workspace_root().join(path).exists() {
        return Err(format!(
            "test anchor `{anchor}` source path `{path}` is missing from the workspace"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
