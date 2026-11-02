use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::model::IdentityInputs;

pub(crate) fn git_rev_parse(rev: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("{rev}^{{commit}}")])
        .output()
        .map_err(|err| format!("failed to run git rev-parse: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse {rev} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn git_worktree_clean() -> Result<bool, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map_err(|err| format!("failed to run git status: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

pub(crate) fn digest_file(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn collect_identity_inputs(root: &Path) -> Result<IdentityInputs, String> {
    let cargo_lock = root.join("Cargo.lock");
    let package_lock = root.join("editors/vscode/package-lock.json");
    let rust_toolchain = root.join("rust-toolchain.toml");
    let ripr_cargo = root.join("crates/ripr/Cargo.toml");
    let vscode_package = root.join("editors/vscode/package.json");

    Ok(IdentityInputs {
        cargo_lock_digest: digest_optional(&cargo_lock)?,
        package_lock_digest: digest_optional(&package_lock)?,
        rust_toolchain_digest: digest_optional(&rust_toolchain)?,
        crate_version: read_toml_version(&ripr_cargo)?,
        extension_version: read_json_version(&vscode_package)?,
    })
}

fn digest_optional(path: &Path) -> Result<Option<String>, String> {
    if path.is_file() {
        digest_file(path).map(Some)
    } else {
        Ok(None)
    }
}

fn read_toml_version(path: &Path) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "version.workspace = true" || trimmed.contains("version.workspace = true") {
            return workspace_package_version(path);
        }
        if trimmed.starts_with("version = ") {
            let value = trimmed
                .trim_start_matches("version = ")
                .trim()
                .trim_matches('"');
            if value.contains("workspace = true") {
                return workspace_package_version(path);
            }
            if !value.is_empty() {
                return Ok(Some(value.to_string()));
            }
        }
    }
    Ok(None)
}

fn workspace_package_version(crate_manifest: &Path) -> Result<Option<String>, String> {
    let workspace_manifest = crate_manifest
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.join("Cargo.toml"));
    let Some(workspace_manifest) = workspace_manifest else {
        return Ok(None);
    };
    if !workspace_manifest.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&workspace_manifest)
        .map_err(|err| format!("failed to read {}: {err}", workspace_manifest.display()))?;
    let mut in_workspace_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "[workspace.package]" {
            in_workspace_package = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_workspace_package = false;
        }
        if in_workspace_package && trimmed.starts_with("version = ") {
            let value = trimmed
                .trim_start_matches("version = ")
                .trim()
                .trim_matches('"');
            if !value.is_empty() {
                return Ok(Some(value.to_string()));
            }
        }
    }
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = false;
        }
        if in_package && trimmed.starts_with("version = ") {
            let value = trimmed
                .trim_start_matches("version = ")
                .trim()
                .trim_matches('"');
            if !value.is_empty() {
                return Ok(Some(value.to_string()));
            }
        }
    }
    Ok(None)
}

fn read_json_version(path: &Path) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    Ok(value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string))
}

pub(crate) fn rehearsal_receipt_path(root: &Path, override_path: Option<&Path>) -> PathBuf {
    override_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("target/ripr/reports/readiness-rehearsal.json"))
}

pub(crate) fn supply_chain_report_path(root: &Path) -> PathBuf {
    root.join("target/ripr/reports/supply-chain.md")
}

pub(crate) fn package_qualification_receipt_path(root: &Path) -> PathBuf {
    root.join("target/ripr/reports/package-qualification-receipt.json")
}

pub(crate) fn identities_compatible(
    snapshot: &IdentityInputs,
    rehearsal: &super::model::RehearsalIdentity,
) -> bool {
    snapshot.cargo_lock_digest.as_deref() == Some(rehearsal.cargo_lock_digest.as_str())
        && snapshot.rust_toolchain_digest.as_deref()
            == Some(rehearsal.rust_toolchain_digest.as_str())
        && snapshot.crate_version.as_deref() == Some(rehearsal.crate_version.as_str())
        && package_lock_matches(snapshot, rehearsal)
}

fn package_lock_matches(
    snapshot: &IdentityInputs,
    rehearsal: &super::model::RehearsalIdentity,
) -> bool {
    match (
        snapshot.package_lock_digest.as_deref(),
        rehearsal.package_lock_digest.as_deref(),
    ) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}

pub(crate) fn semantic_identity_digest(snapshot: &super::model::ReadinessSnapshot) -> String {
    let mut clone = snapshot.clone();
    clone.generated_at = String::new();
    clone.semantic_identity_digest = String::new();
    let json = serde_json::to_string(&clone).unwrap_or_default();
    format!("sha256:{}", sha256_hex(json.as_bytes()))
}
