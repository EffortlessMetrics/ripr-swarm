//! Existing retained-fixture path, byte-count and digest checks.

use std::fs;
use std::path::{Component, Path};

use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn local_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub(super) fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

pub(super) fn verify_file(root: &Path, entry: &Value) -> Result<(), String> {
    let path = entry["path"]
        .as_str()
        .filter(|path| local_path(path))
        .ok_or_else(|| "upstream evidence needs a contained relative file path".to_string())?;
    let bytes = fs::read(root.join(path)).map_err(|error| format!("{path}: {error}"))?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if entry["sha256"].as_str() != Some(actual.as_str()) {
        return Err(format!("upstream evidence digest mismatch: {path}"));
    }
    if entry["bytes"].as_u64() != Some(bytes.len() as u64) {
        return Err(format!("upstream evidence size mismatch: {path}"));
    }
    Ok(())
}
