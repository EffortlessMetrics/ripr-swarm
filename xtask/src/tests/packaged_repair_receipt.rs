//! Validates the #4648/#4650 packaged repair channel receipt fixture contract.

use std::path::{Path, PathBuf};

use serde_json::Value;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

#[test]
fn packaged_repair_receipt_fixture_manifest_lists_closed_qualification_states() -> Result<(), String>
{
    let root = workspace_root().join("fixtures/packaged-repair-parity");
    let manifest = read_json(&root.join("manifest.json"))?;
    if manifest.get("schema_version").and_then(Value::as_str)
        != Some("packaged_repair_channel_receipt.v0")
    {
        return Err(format!("unexpected manifest schema: {manifest}"));
    }
    let states = manifest
        .get("qualification_states")
        .and_then(Value::as_array)
        .ok_or("manifest missing qualification_states")?;
    for required in [
        "qualified",
        "semantic_failure",
        "missing_evidence",
        "stale_or_wrong_head",
        "infrastructure_tempfail",
        "incomparable",
    ] {
        if !states.iter().any(|state| state.as_str() == Some(required)) {
            return Err(format!("manifest missing qualification state `{required}`"));
        }
    }
    let schema = read_json(&root.join("channel-receipt.schema.json"))?;
    if schema.get("title").and_then(Value::as_str) != Some("packaged_repair_channel_receipt.v0") {
        return Err(format!("channel receipt schema title mismatch: {schema}"));
    }
    Ok(())
}

#[test]
fn packaged_repair_receipt_schema_forbids_vsix_parity_claims() -> Result<(), String> {
    let schema = read_json(
        &workspace_root().join("fixtures/packaged-repair-parity/channel-receipt.schema.json"),
    )?;
    let vsix = schema
        .pointer("/properties/vsix_parity_claimed/const")
        .and_then(Value::as_bool);
    if vsix != Some(false) {
        return Err(format!(
            "channel receipt schema must pin vsix_parity_claimed to false: {schema}"
        ));
    }
    Ok(())
}
