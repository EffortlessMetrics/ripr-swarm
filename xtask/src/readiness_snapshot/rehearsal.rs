use std::fs;
use std::path::Path;

use super::model::{
    REHEARSAL_PLAN_SCHEMA_VERSION, REHEARSAL_RECEIPT_KIND, REHEARSAL_RECEIPT_SCHEMA_VERSION,
    RehearsalReceipt,
};

pub(crate) fn parse_rehearsal_receipt(text: &str) -> Result<RehearsalReceipt, String> {
    let receipt: RehearsalReceipt = serde_json::from_str(text)
        .map_err(|err| format!("rehearsal receipt JSON invalid: {err}"))?;
    validate_rehearsal_receipt(&receipt)?;
    Ok(receipt)
}

pub(crate) fn load_rehearsal_receipt(path: &Path) -> Result<RehearsalReceipt, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read rehearsal receipt {}: {err}", path.display()))?;
    parse_rehearsal_receipt(&text)
}

fn validate_rehearsal_receipt(receipt: &RehearsalReceipt) -> Result<(), String> {
    match receipt.schema_version.as_str() {
        REHEARSAL_RECEIPT_SCHEMA_VERSION if receipt.plan.is_none() => {}
        REHEARSAL_PLAN_SCHEMA_VERSION => validate_plan(receipt)?,
        _ => return Err("unsupported rehearsal receipt schema_version or metadata".to_string()),
    }
    if receipt.kind != REHEARSAL_RECEIPT_KIND {
        return Err(format!(
            "rehearsal receipt kind must be {REHEARSAL_RECEIPT_KIND}, got {}",
            receipt.kind
        ));
    }
    if receipt.source_sha.len() != 40 || !receipt.source_sha.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("rehearsal receipt source_sha must be a 40-character git commit".to_string());
    }
    if !receipt.tree_clean {
        return Err(
            "rehearsal receipt requires tree_clean=true for snapshot consumption".to_string(),
        );
    }
    if receipt.identity.cargo_lock_digest.is_empty()
        || receipt.identity.rust_toolchain_digest.is_empty()
        || receipt.identity.crate_version.is_empty()
    {
        return Err(
            "rehearsal receipt identity must bind cargo lock, rust toolchain and crate version"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_plan(receipt: &RehearsalReceipt) -> Result<(), String> {
    let plan = receipt
        .plan
        .as_ref()
        .ok_or("schema 1.1 requires plan metadata")?;
    if plan.profile != "plan_only"
        || plan.source_tree.len() != 40
        || !plan.source_tree.chars().all(|c| c.is_ascii_hexdigit())
        || plan.host_os.is_empty()
        || plan.host_arch.is_empty()
        || plan.reads.is_empty()
        || plan.tools.is_empty()
        || plan.prohibited_operations.is_empty()
        || plan
            .reads
            .iter()
            .chain(&plan.tools)
            .chain(&plan.prohibited_operations)
            .any(String::is_empty)
    {
        return Err("invalid schema 1.1 plan identity, profile or boundary".to_string());
    }
    if !receipt.published_channels.is_empty()
        || !plan.tags_created.is_empty()
        || !plan.writes.is_empty()
        || !plan.cache_roots.is_empty()
        || plan.versions_changed
        || plan.candidate_frozen
        || plan.release_cut
        || plan.release_authority
        || plan.signing_performed
        || plan.credentials_accessed
        || plan.deployments_performed
    {
        return Err("schema 1.1 plan must record no mutation or publication".to_string());
    }
    let components = [
        receipt.crate_package.as_ref(),
        receipt.installed_binary.as_ref(),
        receipt.vsix.as_ref(),
        receipt.dependency_policy.as_ref(),
        receipt.sbom.as_ref(),
        receipt.provenance_plan.as_ref(),
    ];
    if components
        .into_iter()
        .flatten()
        .chain(receipt.server_artifacts.iter())
        .chain(receipt.install_smokes.iter())
        .any(|c| c.status != "not_run" || c.sha256.is_some())
    {
        return Err(
            "schema 1.1 plan components must be not_run without artifact digests".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn component_pass(component: &super::model::RehearsalComponent) -> bool {
    component.status == "pass"
}
