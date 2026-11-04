use super::model::{
    CLAIM_BOUNDARY, ChannelResults, PackagedRepairParityReceipt, RECEIPT_KIND,
    RECEIPT_SCHEMA_VERSION, STANDING_NON_CLAIMS,
};

pub(crate) fn parse_receipt(text: &str) -> Result<PackagedRepairParityReceipt, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("receipt is not JSON: {error}"))?;
    let receipt: PackagedRepairParityReceipt = serde_json::from_value(value)
        .map_err(|error| format!("receipt does not match the typed contract: {error}"))?;
    validate_receipt(&receipt)?;
    Ok(receipt)
}

fn validate_receipt(receipt: &PackagedRepairParityReceipt) -> Result<(), String> {
    if receipt.schema_version != RECEIPT_SCHEMA_VERSION {
        return Err(format!(
            "schema_version must be {RECEIPT_SCHEMA_VERSION}, got {}",
            receipt.schema_version
        ));
    }
    if receipt.kind != RECEIPT_KIND {
        return Err(format!("kind must be {RECEIPT_KIND}, got {}", receipt.kind));
    }
    if receipt.claim_boundary != CLAIM_BOUNDARY {
        return Err(
            "claim_boundary must use the standing packaged-repair-parity boundary".to_string(),
        );
    }
    for standing in STANDING_NON_CLAIMS {
        if !receipt.non_claims.iter().any(|claim| claim == standing) {
            return Err(format!(
                "non_claims must include the standing non-claim `{standing}`"
            ));
        }
    }
    validate_channel_pair(&receipt.channels)?;
    if receipt.required_sections.is_empty() {
        return Err("required_sections must be nonempty".to_string());
    }
    Ok(())
}

fn validate_channel_pair(channels: &ChannelResults) -> Result<(), String> {
    if channels.installed_cli.channel != "installed_cli" {
        return Err("installed_cli.channel must be installed_cli".to_string());
    }
    if channels.packaged_vsix.channel != "packaged_vsix" {
        return Err("packaged_vsix.channel must be packaged_vsix".to_string());
    }
    Ok(())
}
