//! Fixture contract checks for the packaged repair parity manifest (#4648 / #2929).

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

const MANIFEST: &str = "fixtures/packaged_repair_parity/manifest.json";
const PARENT_MANIFEST: &str = "fixtures/blind_journey_installed_rust/manifest.json";
const SCHEMA: &str = "packaged_repair_parity_fixture.v1";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(relative: &str) -> Result<Value, String> {
    let path = workspace_root().join(relative);
    let text =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn git_identity(value: &Value) -> Option<&str> {
    value.as_str().filter(|identity| {
        identity.len() == 40
            && identity
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

#[test]
fn manifest_binds_parent_fixture_identities() -> Result<(), String> {
    let manifest = read_json(MANIFEST)?;
    assert_eq!(
        manifest.get("schema_version").and_then(Value::as_str),
        Some(SCHEMA)
    );
    assert_eq!(
        manifest
            .pointer("/parent_fixture/manifest")
            .and_then(Value::as_str),
        Some(PARENT_MANIFEST)
    );
    let base = manifest
        .pointer("/repository/snapshots/base/commit")
        .ok_or_else(|| "missing base commit".to_string())?;
    let head = manifest
        .pointer("/repository/snapshots/head/commit")
        .ok_or_else(|| "missing head commit".to_string())?;
    let base_id = git_identity(base).ok_or_else(|| "base commit malformed".to_string())?;
    let head_id = git_identity(head).ok_or_else(|| "head commit malformed".to_string())?;

    let parent = read_json(PARENT_MANIFEST)?;
    let parent_base = parent
        .pointer("/repository/snapshots/base/commit")
        .and_then(Value::as_str)
        .ok_or_else(|| "parent base missing".to_string())?;
    let parent_head = parent
        .pointer("/repository/snapshots/head/commit")
        .and_then(Value::as_str)
        .ok_or_else(|| "parent head missing".to_string())?;
    if base_id != parent_base {
        return Err(format!("base commit drift: {base_id} != {parent_base}"));
    }
    if head_id != parent_head {
        return Err(format!("head commit drift: {head_id} != {parent_head}"));
    }
    Ok(())
}

#[test]
fn manifest_declares_cross_channel_contract() -> Result<(), String> {
    let manifest = read_json(MANIFEST)?;
    let axes = manifest
        .get("semantic_comparison_axes")
        .and_then(Value::as_array)
        .ok_or_else(|| "semantic_comparison_axes must be an array".to_string())?;
    if axes.len() < 5 {
        return Err("semantic_comparison_axes too small".to_string());
    }
    for channel in ["installed_cli", "packaged_vsix"] {
        let Some(_) = manifest.pointer(&format!("/channels/{channel}")) else {
            return Err(format!("missing channel {channel}"));
        };
    }
    let gap = manifest
        .pointer("/canonical_transaction/canonical_gap_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "canonical gap missing".to_string())?;
    if gap.is_empty() {
        return Err("canonical gap empty".to_string());
    }
    assert_eq!(
        manifest.get("dogfood_denominator").and_then(Value::as_str),
        Some("excluded")
    );
    Ok(())
}

#[test]
fn repository_snapshot_files_exist_on_disk() -> Result<(), String> {
    let root = workspace_root();
    let repo = root.join("fixtures/blind_journey_installed_rust/repository");
    for sub in ["base", "head"] {
        let dir = repo.join(sub);
        if !dir.is_dir() {
            return Err(format!("missing repository snapshot {}", dir.display()));
        }
    }
    Ok(())
}
