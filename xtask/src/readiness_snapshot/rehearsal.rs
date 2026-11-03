use std::fs;
use std::path::Path;

use super::model::{REHEARSAL_RECEIPT_KIND, REHEARSAL_RECEIPT_SCHEMA_VERSION, RehearsalReceipt};

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
    if receipt.schema_version != REHEARSAL_RECEIPT_SCHEMA_VERSION {
        return Err(format!(
            "rehearsal receipt schema_version must be {REHEARSAL_RECEIPT_SCHEMA_VERSION}, got {}",
            receipt.schema_version
        ));
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

pub(crate) fn component_pass(component: &super::model::RehearsalComponent) -> bool {
    component.status == "pass"
}
