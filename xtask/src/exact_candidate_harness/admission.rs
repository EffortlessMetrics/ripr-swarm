//! Candidate admission and owned binary snapshot (#4510).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::launch::CHILD_PATH;
use super::model::BinaryIdentity;
use crate::blind_journey::sha256_hex;
use crate::run as shared_run;

#[derive(Clone, Debug, Default)]
pub(crate) struct AdmitOptions {
    pub(crate) expected_sha256: Option<String>,
    pub(crate) expected_version: Option<String>,
    pub(crate) expected_commit: Option<String>,
}

pub(crate) fn admit_installed_binary(
    binary: &Path,
    options: &AdmitOptions,
) -> Result<BinaryIdentity, String> {
    let path = fs::canonicalize(binary)
        .map_err(|error| format!("binary {} is not readable: {error}", binary.display()))?;
    if !path.is_file() {
        return Err(format!("binary {} is not a file", path.display()));
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let sha256 = sha256_hex(&bytes);
    if let Some(expected) = &options.expected_sha256
        && !expected.eq_ignore_ascii_case(&sha256)
    {
        return Err(format!(
            "binary sha256 {sha256} does not match --expected-sha256 {expected}"
        ));
    }
    let version_args = vec!["--version".to_string()];
    let version = shared_run::capture_bytes_in_dir_with_timeout(
        &path,
        &version_args,
        path.parent().unwrap_or_else(|| Path::new(".")),
        &[("PATH", CHILD_PATH)],
        &[],
        Duration::from_secs(30),
        "exact-candidate binary --version",
    )?;
    if version.timed_out || !version.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "`{} --version` did not exit cleanly; this is not a runnable candidate binary",
            path.display()
        ));
    }
    let reported_version = String::from_utf8_lossy(&version.stdout).trim().to_string();
    if let Some(expected) = &options.expected_commit {
        let reported = version_commit(&reported_version);
        let matches = reported
            .as_deref()
            .is_some_and(|commit| commit.eq_ignore_ascii_case(expected));
        if !matches {
            return Err(format!(
                "binary reports commit {} but expected commit is {expected}",
                reported.unwrap_or_else(|| "none".to_string())
            ));
        }
    }
    if let Some(expected) = &options.expected_version
        && !reported_version.contains(expected.as_str())
    {
        return Err(format!(
            "binary reports `{reported_version}`, expected a version containing `{expected}`"
        ));
    }
    Ok(BinaryIdentity {
        path: path.to_string_lossy().to_string(),
        bytes: bytes.len() as u64,
        sha256,
        reported_version,
        path_resolution: format!(
            "launched by absolute path; child PATH is `{CHILD_PATH}`, so no ambient `ripr` can resolve"
        ),
    })
}

/// Commit embedded in a `ripr --version` line: `ripr 0.11.0 (<sha>)`.
pub(crate) fn version_commit(reported: &str) -> Option<String> {
    let (_, tail) = reported.rsplit_once('(')?;
    let sha = tail.strip_suffix(')')?.trim();
    if sha.is_empty() || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(sha.to_ascii_lowercase())
}

pub(crate) fn snapshot_admitted_binary(
    source: &Path,
    run_root: &Path,
    expected_sha256: &str,
) -> Result<PathBuf, String> {
    let dir = run_root.join("binary");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    let target = dir.join("ripr-under-test");
    fs::copy(source, &target).map_err(|error| {
        format!(
            "failed to snapshot {} to {}: {error}",
            source.display(),
            target.display()
        )
    })?;
    let sha = hash_file(&target)?;
    if sha != expected_sha256 {
        return Err(format!(
            "binary changed between admission and snapshot (admitted {expected_sha256}, copied {sha})"
        ));
    }
    Ok(target)
}

pub(crate) fn hash_file(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(sha256_hex(&bytes))
}
