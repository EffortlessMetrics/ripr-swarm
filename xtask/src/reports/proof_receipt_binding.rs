//! Conservative compatibility for local advisory preflight evidence.
//! Execution identity stays in the receipt; compatibility never retags a run.

use crate::run::run_output_owned_with_timeout;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreflightBinding {
    pub(super) execution_head_sha: String,
    pub(super) source_tree: String,
    pub(super) base_sha: String,
    pub(super) merge_base_sha: String,
    pub(super) environment_digest: String,
    pub(super) plan_digest: String,
}

impl PreflightBinding {
    pub(super) fn compatible_with(&self, current: &Self) -> bool {
        [self, current].iter().all(|binding| {
            [
                &binding.execution_head_sha,
                &binding.source_tree,
                &binding.base_sha,
                &binding.merge_base_sha,
                &binding.environment_digest,
                &binding.plan_digest,
            ]
            .iter()
            .all(|value| !value.trim().is_empty())
        }) && self.source_tree == current.source_tree
            && self.base_sha == current.base_sha
            && self.merge_base_sha == current.merge_base_sha
            && self.environment_digest == current.environment_digest
            && self.plan_digest == current.plan_digest
    }
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn append(bytes: &mut Vec<u8>, part: &[u8]) {
    bytes.extend_from_slice(&(part.len() as u64).to_le_bytes());
    bytes.extend_from_slice(part);
}

fn observe(program: &str, args: &[String]) -> Result<String, String> {
    run_output_owned_with_timeout(
        program,
        args,
        Duration::from_secs(30),
        "preflight reuse binding",
    )
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let root = root.to_str().ok_or("preflight root is not UTF-8")?;
    let args = ["-C", root]
        .into_iter()
        .chain(args.iter().copied())
        .map(str::to_string)
        .collect::<Vec<_>>();
    observe("git", &args).map(|output| output.trim().to_string())
}

/// Bind only clean, actually selected worktrees. A capture error means no
/// reusable evidence, never an empty fingerprint that can match another error.
pub(super) fn capture_binding(
    root: &Path,
    base_sha: &str,
    head_sha: &str,
    environment_digest: &str,
    plan_digest: &str,
) -> Result<PreflightBinding, String> {
    if git(root, &["rev-parse", "HEAD"])? != head_sha {
        return Err("selected head is not the executing HEAD".to_string());
    }
    if !git(root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
        return Err("executing worktree has source changes".to_string());
    }
    let flags = git(root, &["ls-files", "-v", "-z"])?;
    if flags
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .any(|entry| entry.as_bytes()[0].is_ascii_lowercase() || entry.starts_with('S'))
    {
        return Err("preflight reuse cannot bind hidden index entries".to_string());
    }
    let entries = git(root, &["ls-files", "--stage", "-z"])?;
    if entries
        .split('\0')
        .any(|entry| entry.starts_with("160000 "))
    {
        return Err("preflight reuse does not bind submodule worktrees".to_string());
    }
    let source_tree = git(root, &["rev-parse", &format!("{head_sha}^{{tree}}")])?;
    let merge_base_sha = git(root, &["merge-base", "--all", base_sha, head_sha])?;
    if merge_base_sha.lines().count() != 1 {
        return Err("preflight merge base is ambiguous".to_string());
    }
    Ok(PreflightBinding {
        execution_head_sha: head_sha.to_string(),
        source_tree,
        base_sha: base_sha.to_string(),
        merge_base_sha,
        environment_digest: environment_digest.to_string(),
        plan_digest: plan_digest.to_string(),
    })
}

/// An observed local environment, not authentication of SDK/dependency/service
/// bytes. Hash values rather than publishing environment contents. All variables
/// are included conservatively: incidental changes may refuse otherwise useful
/// reuse, but cannot silently preserve it after a changed variable.
pub(super) fn environment_digest(root: &Path) -> Result<String, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("preflight root: {error}"))?;
    for key in [
        "RUSTC",
        "RUSTDOC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        if std::env::var_os(key).is_some_and(|value| !value.is_empty()) {
            return Err(format!("preflight reuse does not bind {key} overrides"));
        }
    }
    // Without explicit Cargo home, do not guess the canonical platform fallback.
    let home = std::env::var_os("CARGO_HOME")
        .filter(|value| !value.is_empty())
        .ok_or("preflight reuse requires an explicit CARGO_HOME")?;
    let home = root.join(PathBuf::from(home));
    let mut bytes = Vec::new();
    append(&mut bytes, b"local-preflight-environment-v1");
    append(&mut bytes, root.as_os_str().as_encoded_bytes());
    append(&mut bytes, std::env::consts::OS.as_bytes());
    append(&mut bytes, std::env::consts::ARCH.as_bytes());
    append_environment(&mut bytes, std::env::vars_os().collect());
    for (program, args) in [
        (
            "rustc",
            vec!["--version".to_string(), "--verbose".to_string()],
        ),
        ("cargo", vec!["--version".to_string()]),
    ] {
        let version = observe(program, &args)?;
        if version.trim().is_empty() {
            return Err(format!("{program} identity is empty"));
        }
        append(&mut bytes, version.as_bytes());
    }
    let mut configs = BTreeSet::new();
    for directory in root.ancestors() {
        for name in [
            ".cargo/config",
            ".cargo/config.toml",
            "rust-toolchain",
            "rust-toolchain.toml",
        ] {
            configs.insert(directory.join(name));
        }
    }
    configs.extend([home.join("config"), home.join("config.toml")]);
    append_configs(&mut bytes, &configs)?;
    Ok(digest(&bytes))
}

fn append_environment(
    bytes: &mut Vec<u8>,
    mut environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
) {
    environment.sort();
    for (key, value) in environment {
        append(bytes, key.as_encoded_bytes());
        append(bytes, value.as_encoded_bytes());
    }
}

fn append_configs(bytes: &mut Vec<u8>, configs: &BTreeSet<PathBuf>) -> Result<(), String> {
    let mut total = 0usize;
    for path in configs {
        append(bytes, path.as_os_str().as_encoded_bytes());
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                append(bytes, b"absent");
                continue;
            }
            Err(error) => return Err(format!("preflight config metadata: {error}")),
            Ok(_) => {}
        }
        if !std::fs::metadata(path)
            .map_err(|error| format!("preflight config: {error}"))?
            .is_file()
        {
            return Err("preflight config is not a regular file".to_string());
        }
        append(bytes, b"present");
        append(
            bytes,
            path.canonicalize()
                .map_err(|error| error.to_string())?
                .as_os_str()
                .as_encoded_bytes(),
        );
        let mut content = Vec::new();
        std::fs::File::open(path)
            .map_err(|error| error.to_string())?
            .take(1_048_577)
            .read_to_end(&mut content)
            .map_err(|error| error.to_string())?;
        total += content.len();
        if content.len() > 1_048_576 || total > 16_777_216 {
            return Err("preflight configuration exceeds capture budget".to_string());
        }
        append(bytes, &content);
    }
    Ok(())
}

#[cfg(test)]
#[path = "proof_receipt_binding_tests.rs"]
mod tests;
