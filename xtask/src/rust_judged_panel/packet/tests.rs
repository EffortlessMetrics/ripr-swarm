use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{PortableCurrent, read_strict_json_bytes, replace_file};

#[cfg(windows)]
use super::{remove_replacement_backup, replacement_backup_path};

fn scratch(name: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("clock before epoch: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-packet-{name}-{nonce}"));
    fs::create_dir_all(&root).map_err(|error| format!("create scratch root: {error}"))?;
    Ok(root)
}

#[test]
fn rust_judged_panel_packet_replaces_existing_current_on_second_publication() -> Result<(), String>
{
    let root = scratch("replace")?;
    let current = root.join("current.json");
    let first = root.join("first.tmp");
    let second = root.join("second.tmp");
    fs::write(&first, b"generation-a\n").map_err(|error| error.to_string())?;
    replace_file(&first, &current)?;
    fs::write(&second, b"generation-b\n").map_err(|error| error.to_string())?;
    replace_file(&second, &current)?;
    let actual = fs::read(&current).map_err(|error| error.to_string())?;
    let cleanup = fs::remove_dir_all(&root);
    if cleanup.is_err() || actual != b"generation-b\n" || first.exists() || second.exists() {
        return Err("second publication did not replace the prior current atomically".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_failed_current_replacement_preserves_previous_pointer()
-> Result<(), String> {
    let root = scratch("failed-replace")?;
    let current = root.join("current.json");
    let missing = root.join("missing.tmp");
    fs::write(&current, b"prior-generation\n").map_err(|error| error.to_string())?;
    if replace_file(&missing, &current).is_ok() {
        return Err("missing replacement unexpectedly succeeded".to_string());
    }
    let actual = fs::read(&current).map_err(|error| error.to_string())?;
    fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    if actual == b"prior-generation\n" {
        Ok(())
    } else {
        Err("failed replacement destroyed the previous current pointer".to_string())
    }
}

#[cfg(windows)]
#[test]
fn rust_judged_panel_packet_reconciles_stale_backup_before_publication() -> Result<(), String> {
    let root = scratch("stale-backup")?;
    let current = root.join("current.json");
    let backup = replacement_backup_path(&current);
    let next = root.join("next.tmp");
    fs::write(&current, b"current-generation\n").map_err(|error| error.to_string())?;
    fs::write(&backup, b"stale-generation\n").map_err(|error| error.to_string())?;
    fs::write(&next, b"next-generation\n").map_err(|error| error.to_string())?;
    replace_file(&next, &current)?;
    let actual = fs::read(&current).map_err(|error| error.to_string())?;
    let cleanup = fs::remove_dir_all(&root);
    if cleanup.is_err() || actual != b"next-generation\n" || backup.exists() {
        return Err("stale backup was not reconciled after publication".to_string());
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn rust_judged_panel_packet_recovers_interrupted_staging_before_publication() -> Result<(), String>
{
    let root = scratch("recover-backup")?;
    let current = root.join("current.json");
    let backup = replacement_backup_path(&current);
    let next = root.join("next.tmp");
    fs::write(&backup, b"prior-generation\n").map_err(|error| error.to_string())?;
    fs::write(&next, b"next-generation\n").map_err(|error| error.to_string())?;
    replace_file(&next, &current)?;
    let actual = fs::read(&current).map_err(|error| error.to_string())?;
    let cleanup = fs::remove_dir_all(&root);
    if cleanup.is_err() || actual != b"next-generation\n" || backup.exists() {
        return Err("interrupted staging was not recovered before publication".to_string());
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn rust_judged_panel_packet_reports_backup_cleanup_failure() -> Result<(), String> {
    let root = scratch("cleanup-failure")?;
    let current = root.join("current.json");
    let backup = replacement_backup_path(&current);
    fs::write(&current, b"prior-generation\n").map_err(|error| error.to_string())?;
    fs::create_dir(&backup).map_err(|error| error.to_string())?;
    let result = remove_replacement_backup(&current, &backup);
    fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    if result.is_ok() {
        return Err("backup cleanup failure was not reported after publication".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_strict_readback_rejects_duplicate_and_unknown_keys()
-> Result<(), String> {
    let valid = br#"{"schema_version":"0.1","kind":"rust_judged_panel_portable_current","generation_id":"generation","index_path":"metrics/rust-judged-behavior-panel/portable/generations/generation/packet-index.json","index_sha256":"sha256:index"}"#;
    let duplicate = br#"{"schema_version":"0.1","kind":"rust_judged_panel_portable_current","kind":"duplicate","generation_id":"generation","index_path":"metrics/rust-judged-behavior-panel/portable/generations/generation/packet-index.json","index_sha256":"sha256:index"}"#;
    let unknown = br#"{"schema_version":"0.1","kind":"rust_judged_panel_portable_current","generation_id":"generation","index_path":"metrics/rust-judged-behavior-panel/portable/generations/generation/packet-index.json","index_sha256":"sha256:index","unexpected":true}"#;
    let parsed: PortableCurrent = read_strict_json_bytes(valid, "valid current")?;
    if parsed.generation_id != "generation"
        || read_strict_json_bytes::<PortableCurrent>(duplicate, "duplicate current").is_ok()
        || read_strict_json_bytes::<PortableCurrent>(unknown, "unknown current").is_ok()
    {
        return Err("strict current readback accepted invalid JSON".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_retained_validator_reaches_committed_generation() -> Result<(), String>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask has no workspace parent".to_string())?;
    let manifest = crate::rust_judged_panel::load_and_validate_at(
        root,
        Path::new("metrics/rust-judged-behavior-panel/manifest.json"),
    )?;
    super::validate_at(root, &manifest)
}
