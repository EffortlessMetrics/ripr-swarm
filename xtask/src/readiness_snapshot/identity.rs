use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::model::IdentityInputs;

// No probe may execute an inherited fsmonitor, fetch objects or use replace refs.
pub(super) fn git_output(args: &[&str]) -> Result<std::process::Output, String> {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        let key = name.to_string_lossy().to_ascii_uppercase();
        if key.starts_with("GIT_TRACE")
            || key.starts_with("GIT_REDIRECT_")
            || key.starts_with("GIT_TEST_")
        {
            command.env_remove(name);
        }
    }
    command
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.splitIndex=false",
            "-c",
            "protocol.allow=never",
        ])
        // Explicit zero targets also override trace2 targets from Git configuration.
        .env("GIT_TRACE", "0")
        .env("GIT_TRACE2", "0")
        .env("GIT_TRACE2_EVENT", "0")
        .env("GIT_TRACE2_PERF", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "")
        .args(args)
        .output()
        .map_err(|err| format!("failed to run read-only git probe: {err}"))
}

pub(crate) fn git_rev_parse(rev: &str) -> Result<String, String> {
    git_object(rev, "commit")
}

pub(crate) fn git_source_tree(rev: &str) -> Result<String, String> {
    git_object(rev, "tree")
}

fn git_object(rev: &str, object_kind: &str) -> Result<String, String> {
    if rev.starts_with(':') {
        return Err("source revision cannot use index expressions".to_string());
    }
    let output = git_output(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{rev}^{{{object_kind}}}"),
    ])?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse {rev} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
pub(crate) fn git_worktree_clean() -> Result<bool, String> {
    let commit = git_rev_parse("HEAD")?;
    git_worktree_clean_for_commit(&commit)
}

fn git_worktree_clean_for_commit(commit: &str) -> Result<bool, String> {
    // Status can invoke clean/process filters. Reject their configuration before
    // asking it to inspect files, including globally configured but unused filters.
    let filters = git_output(&[
        "config",
        "--name-only",
        "--get-regexp",
        r"^filter\..*\.(clean|process)$",
    ])?;
    if filters.status.success() {
        return Err(
            "source inspection refuses configured clean/process filter helpers".to_string(),
        );
    }
    if filters.status.code() != Some(1) || !filters.stdout.is_empty() {
        return Err("could not establish absence of clean/process filter helpers".to_string());
    }
    let root = git_toplevel()?;
    let root_text = root.to_str().ok_or("source root is not UTF-8")?;
    reject_shared_indexes()?;
    // No pathspec: inherited literal/glob modes cannot narrow this inventory.
    let index = git_output(&["-C", root_text, "ls-files", "--stage", "-v", "-z"])?;
    if !index.status.success() {
        return Err("could not inspect source index flags and modes".to_string());
    }
    if index.stdout.split(|byte| *byte == 0).any(|entry| {
        !entry.is_empty()
            && (entry.first() != Some(&b'H')
                || !matches!(entry.get(2..8), Some(b"100644" | b"100755")))
    }) {
        return Err(
            "source index masks changes or contains an unsupported gitlink/mode".to_string(),
        );
    }
    if !super::source_tree::tracked_bytes_match(&root, commit)? {
        return Ok(false);
    }
    let output = git_output(&[
        "status",
        "--porcelain",
        "--untracked-files=all",
        "--ignore-submodules=none",
    ])?;
    if !output.status.success() {
        return Err(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().is_empty()
        && git_rev_parse("HEAD")? == commit)
}

pub(crate) fn git_toplevel() -> Result<PathBuf, String> {
    git_path(&["rev-parse", "--show-toplevel"])
}

fn git_path(args: &[&str]) -> Result<PathBuf, String> {
    let output = git_output(args)?;
    if !output.status.success() {
        return Err(format!(
            "read-only Git path query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let root =
        std::str::from_utf8(&output.stdout).map_err(|_| "source root is not UTF-8".to_string())?;
    let root = root
        .strip_suffix('\n')
        .ok_or("git root output has no line terminator")?;
    #[cfg(windows)]
    let root = root.strip_suffix('\r').unwrap_or(root);
    if root.is_empty() {
        return Err("git root output is empty".to_string());
    }
    Ok(PathBuf::from(root))
}

fn reject_shared_indexes() -> Result<(), String> {
    // Reading a split index can freshen its shared file even with optional
    // locks disabled. Inspect directory names before any index-reading command.
    let git_dir = git_path(&["rev-parse", "--absolute-git-dir"])?;
    let common = git_path(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let index = git_path(&["rev-parse", "--path-format=absolute", "--git-path", "index"])?;
    let index_parent = index
        .parent()
        .ok_or("source index has no parent")?
        .to_path_buf();
    let directories = [git_dir, common, index_parent]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    for directory in directories {
        for entry in fs::read_dir(directory)
            .map_err(|e| format!("cannot inspect Git index directory: {e}"))?
        {
            let entry = entry.map_err(|e| format!("cannot inspect Git index entry: {e}"))?;
            if entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("sharedindex.")
            {
                return Err(
                    "source inspection refuses shared split indexes that can change on read"
                        .to_string(),
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn tree_clean_for_subject(source_sha: &str) -> Result<bool, String> {
    let head = git_rev_parse("HEAD")?;
    if head != source_sha {
        return Ok(false);
    }
    git_worktree_clean_for_commit(source_sha)
}

pub(crate) fn collect_identity_inputs_for_commit(
    _root: &Path,
    commit: &str,
) -> Result<IdentityInputs, String> {
    // Even current-HEAD identities come from immutable objects, never live disk.
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
    let output = git_output(&["show", &spec])?;
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
