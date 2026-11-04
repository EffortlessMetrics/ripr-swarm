use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::model::IdentityInputs;

pub(crate) fn git_rev_parse(rev: &str) -> Result<String, String> {
    git_object(rev, "commit")
}

pub(crate) fn git_source_tree(rev: &str) -> Result<String, String> {
    git_object(rev, "tree")
}

fn git_object(rev: &str, object_kind: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &format!("{rev}^{{{object_kind}}}")])
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
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
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

pub(crate) fn git_toplevel() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|err| format!("failed to run git rev-parse --show-toplevel: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse --show-toplevel failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

pub(crate) fn tree_clean_for_subject(source_sha: &str) -> Result<bool, String> {
    let head = git_rev_parse("HEAD")?;
    if head != source_sha {
        return Ok(false);
    }
    git_worktree_clean()
}

pub(crate) fn collect_identity_inputs_for_commit(
    root: &Path,
    commit: &str,
) -> Result<IdentityInputs, String> {
    let head = git_rev_parse("HEAD")?;
    if head == commit {
        return collect_identity_inputs(root);
    }
    let workspace_toml = git_blob_text(commit, "Cargo.toml")?;
    Ok(IdentityInputs {
        cargo_lock_digest: git_blob_digest_optional(commit, "Cargo.lock")?,
        package_lock_digest: git_blob_digest_optional(commit, "editors/vscode/package-lock.json")?,
        rust_toolchain_digest: git_blob_digest_optional(commit, "rust-toolchain.toml")?,
        crate_version: read_crate_version_from_commit(commit, workspace_toml.as_deref())?,
        extension_version: git_blob_text(commit, "editors/vscode/package.json")
            .ok()
            .flatten()
            .and_then(|text| read_json_version_from_text(&text).ok())
            .flatten(),
    })
}

fn git_blob_text(rev: &str, path: &str) -> Result<Option<String>, String> {
    git_blob_bytes(rev, path).map(|bytes| bytes.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

fn git_blob_bytes(rev: &str, path: &str) -> Result<Option<Vec<u8>>, String> {
    let spec = format!("{rev}:{path}");
    let output = Command::new("git")
        .args(["show", &spec])
        .output()
        .map_err(|err| format!("failed to run git show {spec}: {err}"))?;
    if output.status.success() {
        return Ok(Some(output.stdout));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("does not exist") || stderr.contains("exists on disk, but not in") {
        return Ok(None);
    }
    Err(format!("git show {spec} failed: {stderr}"))
}

fn git_blob_digest_optional(rev: &str, path: &str) -> Result<Option<String>, String> {
    git_blob_bytes(rev, path).map(|opt| opt.map(|bytes| format!("sha256:{}", sha256_hex(&bytes))))
}

fn read_crate_version_from_commit(
    commit: &str,
    workspace_toml: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(ripr_manifest) = git_blob_text(commit, "crates/ripr/Cargo.toml")? else {
        return Ok(None);
    };
    read_toml_version_from_text(&ripr_manifest, workspace_toml)
}

fn read_toml_version_from_text(
    manifest: &str,
    workspace_manifest: Option<&str>,
) -> Result<Option<String>, String> {
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed == "version.workspace = true" || trimmed.contains("version.workspace = true") {
            if let Some(workspace) = workspace_manifest {
                return workspace_package_version_from_text(workspace);
            }
            return Ok(None);
        }
        if trimmed.starts_with("version = ") {
            let value = trimmed
                .trim_start_matches("version = ")
                .trim()
                .trim_matches('"');
            if value.contains("workspace = true") {
                if let Some(workspace) = workspace_manifest {
                    return workspace_package_version_from_text(workspace);
                }
                return Ok(None);
            }
            if !value.is_empty() {
                return Ok(Some(value.to_string()));
            }
        }
    }
    Ok(None)
}

fn workspace_package_version_from_text(text: &str) -> Result<Option<String>, String> {
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
    Ok(None)
}

fn read_json_version_from_text(text: &str) -> Result<Option<String>, String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|err| format!("failed to parse extension package.json: {err}"))?;
    Ok(value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string))
}

pub(crate) fn supply_chain_report_status(path: &Path) -> Result<Option<String>, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Status:") {
            return Ok(Some(rest.trim().to_string()));
        }
    }
    Ok(None)
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
    let workspace_text = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.join("Cargo.toml"))
        .filter(|p| p.is_file())
        .and_then(|p| fs::read_to_string(p).ok());
    read_toml_version_from_text(&text, workspace_text.as_deref())
}

fn read_json_version(path: &Path) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    read_json_version_from_text(&text)
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
        && extension_version_matches(snapshot, rehearsal)
}

fn extension_version_matches(
    snapshot: &IdentityInputs,
    rehearsal: &super::model::RehearsalIdentity,
) -> bool {
    match (
        snapshot.extension_version.as_deref(),
        rehearsal.extension_version.as_deref(),
    ) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
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
