//! Immutable source and controller custody for qualification-only producers.
use super::{QualificationInput, safe_artifact_path};
use crate::policy::{CandidateAuthoritySnapshot, capture_candidate_authority};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Actual source custody, constructed only from the validated controller and
/// the selected checkout's Git objects. Caller-supplied identity strings cannot
/// construct this handle.
pub(crate) struct AdmittedSource {
    authority: CandidateAuthoritySnapshot,
    root: PathBuf,
    version: String,
    package_prefix: String,
    package_name: String,
    blobs: BTreeMap<String, Vec<u8>>,
}

impl AdmittedSource {
    pub(crate) fn admit(input: &QualificationInput, version: &str) -> Result<Self, String> {
        let artifact = input
            .artifact()
            .to_str()
            .ok_or_else(|| "candidate artifact is not UTF-8".to_string())?;
        let authority = capture_candidate_authority(input.controller_root(), version, artifact)?;
        let root = input
            .source_root()
            .canonicalize()
            .map_err(|error| format!("resolve candidate source root: {error}"))?;
        if root.to_str().is_none() {
            return Err("qualification source root must be UTF-8".to_string());
        }
        verify_source_identity(&root, &authority)?;
        let blobs = committed_blobs(&root, authority.candidate_sha()?)?;
        let workspace_bytes = blobs
            .get("Cargo.toml")
            .ok_or_else(|| "candidate source lacks committed root Cargo.toml".to_string())?;
        let workspace: toml::Value = std::str::from_utf8(workspace_bytes)
            .map_err(|error| format!("source manifest UTF-8: {error}"))?
            .parse()
            .map_err(|error| format!("source manifest TOML: {error}"))?;
        let package_prefix = if workspace.get("package").is_some() {
            ""
        } else {
            "crates/ripr/"
        };
        let manifest_path = format!("{package_prefix}Cargo.toml");
        let manifest_bytes = blobs
            .get(&manifest_path)
            .ok_or_else(|| "candidate source lacks committed package Cargo.toml".to_string())?;
        let manifest: toml::Value = std::str::from_utf8(manifest_bytes)
            .map_err(|error| format!("package manifest UTF-8: {error}"))?
            .parse()
            .map_err(|error| format!("package manifest TOML: {error}"))?;
        let package = manifest
            .get("package")
            .ok_or_else(|| "candidate manifest lacks package".to_string())?;
        let package_name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| "candidate package name missing".to_string())?
            .to_string();
        let actual_version = package
            .get("version")
            .and_then(toml::Value::as_str)
            .or_else(|| {
                package
                    .get("version")
                    .and_then(|value| value.get("workspace"))
                    .and_then(toml::Value::as_bool)
                    .filter(|value| *value)
                    .and_then(|_| workspace.get("workspace"))
                    .and_then(|value| value.get("package"))
                    .and_then(|value| value.get("version"))
                    .and_then(toml::Value::as_str)
            });
        if actual_version != Some(version) {
            return Err(
                "candidate source package version differs from requested release".to_string(),
            );
        }
        if !blobs.contains_key("Cargo.lock") {
            return Err("candidate source lacks committed Cargo.lock".to_string());
        }
        let admitted = Self {
            authority,
            root,
            version: version.to_string(),
            package_prefix: package_prefix.to_string(),
            package_name,
            blobs,
        };
        admitted.revalidate()?;
        Ok(admitted)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn version(&self) -> &str {
        &self.version
    }
    pub(super) fn package_prefix(&self) -> &str {
        &self.package_prefix
    }
    pub(crate) fn package_name(&self) -> &str {
        &self.package_name
    }
    pub(crate) fn package_root(&self) -> PathBuf {
        self.root.join(&self.package_prefix)
    }
    pub(crate) fn revalidate(&self) -> Result<(), String> {
        self.authority.revalidate()?;
        verify_source_identity(&self.root, &self.authority)?;
        // Git's clean status does not detect skip-worktree/filter substitutions.
        // This first slice supports raw-byte checkouts only; it does not guess
        // Cargo normalization for source inputs or accept ignored file bytes.
        for (path, expected) in &self.blobs {
            let actual = std::fs::read(self.root.join(path))
                .map_err(|error| format!("read selected source blob {path}: {error}"))?;
            if actual != *expected {
                return Err(format!(
                    "selected checkout bytes differ from committed blob: {path}"
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn committed_file(&self, path: &str) -> Option<&[u8]> {
        self.blobs.get(path).map(Vec::as_slice)
    }
    pub(crate) fn source_sha(&self) -> Result<&str, String> {
        self.authority.candidate_sha()
    }
    pub(crate) fn custody_json(&self) -> serde_json::Value {
        serde_json::json!({"controller": self.authority.custody_json(),
            "source_root": self.root, "package_name": self.package_name,
            "package_version": self.version, "package_prefix": self.package_prefix,
            "source_checkout_contract": "ordinary tracked files match raw committed blobs; transformed/sparse inputs refuse"})
    }
}

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let args = args
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        Path::new("git"),
        &args,
        root,
        &[],
        &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
        Duration::from_secs(30),
        "candidate source Git identity",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err("candidate source Git identity command failed or timed out".to_string());
    }
    Ok(output.stdout)
}

fn verify_source_identity(
    root: &Path,
    authority: &CandidateAuthoritySnapshot,
) -> Result<(), String> {
    let sha = authority.candidate_sha()?;
    let tree = authority.candidate_tree()?;
    let git_ref = authority.candidate_ref()?;
    for (revision, expected) in [
        ("HEAD".to_string(), sha),
        (format!("{sha}^{{tree}}"), tree),
        (format!("{git_ref}^{{commit}}"), sha),
    ] {
        let bytes = git_bytes(root, &["rev-parse", "--verify", &revision])?;
        let actual = std::str::from_utf8(&bytes)
            .map_err(|error| format!("Git identity UTF-8: {error}"))?
            .trim();
        if actual != expected {
            return Err(format!("candidate source identity changed at {revision}"));
        }
    }
    if !git_bytes(root, &["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty() {
        return Err("candidate source checkout is not clean".to_string());
    }
    Ok(())
}

fn committed_blobs(root: &Path, sha: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let tree = git_bytes(root, &["ls-tree", "-rz", "--full-tree", sha])?;
    let mut entries = Vec::new();
    let mut input = Vec::new();
    for entry in tree
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let separator = entry
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| "malformed Git tree entry".to_string())?;
        let header = std::str::from_utf8(
            entry
                .get(..separator)
                .ok_or_else(|| "Git tree header boundary".to_string())?,
        )
        .map_err(|error| format!("Git tree header UTF-8: {error}"))?;
        let mut fields = header.split_whitespace();
        let mode = fields
            .next()
            .ok_or_else(|| "Git tree mode missing".to_string())?;
        let kind = fields
            .next()
            .ok_or_else(|| "Git tree kind missing".to_string())?;
        let oid = fields
            .next()
            .ok_or_else(|| "Git tree OID missing".to_string())?;
        if !matches!(mode, "100644" | "100755") || kind != "blob" {
            // Only ordinary blobs can attribute an ordinary package entry. A
            // packaged symlink/submodule is rejected during archive validation.
            continue;
        }
        let path = std::str::from_utf8(
            entry
                .get(separator.saturating_add(1)..)
                .ok_or_else(|| "Git tree path boundary".to_string())?,
        )
        .map_err(|error| format!("Git tree path UTF-8: {error}"))?
        .to_string();
        if !safe_artifact_path(Path::new(&path)) {
            return Err("unsupported Git source path".to_string());
        }
        entries.push((path, oid.to_string()));
        input.extend_from_slice(oid.as_bytes());
        input.push(b'\n');
    }
    let output = crate::run::capture_bytes_in_dir_with_input_timeout(
        Path::new("git"),
        &["cat-file".to_string(), "--batch".to_string()],
        root,
        &input,
        &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
        Duration::from_secs(30),
        "candidate committed source blobs",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err("candidate committed blob capture failed or timed out".to_string());
    }
    let mut remaining = output.stdout.as_slice();
    let mut blobs = BTreeMap::new();
    for (path, oid) in entries {
        let end = remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| "Git batch header incomplete".to_string())?;
        let header = std::str::from_utf8(
            remaining
                .get(..end)
                .ok_or_else(|| "Git batch header boundary".to_string())?,
        )
        .map_err(|error| format!("Git batch header UTF-8: {error}"))?;
        let mut fields = header.split_whitespace();
        if fields.next() != Some(oid.as_str()) || fields.next() != Some("blob") {
            return Err("Git batch object identity/type mismatch".to_string());
        }
        let size = fields
            .next()
            .ok_or_else(|| "Git batch size missing".to_string())?
            .parse::<usize>()
            .map_err(|error| format!("Git batch size: {error}"))?;
        let body = remaining
            .get(end.saturating_add(1)..)
            .ok_or_else(|| "Git batch body boundary".to_string())?;
        let bytes = body
            .get(..size)
            .ok_or_else(|| "Git batch body incomplete".to_string())?;
        if body.get(size) != Some(&b'\n') {
            return Err("Git batch terminator missing".to_string());
        }
        if blobs.insert(path, bytes.to_vec()).is_some() {
            return Err("duplicate committed source path".to_string());
        }
        remaining = body
            .get(size.saturating_add(1)..)
            .ok_or_else(|| "Git batch tail boundary".to_string())?;
    }
    if !remaining.is_empty() {
        return Err("unexpected Git batch tail".to_string());
    }
    Ok(blobs)
}
