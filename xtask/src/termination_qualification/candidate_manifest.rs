//! Reader for the frozen-candidate manifest (#1609). The DTO and its admission
//! validator are `LiveHeadSnapshot` in the release candidate harness; this is
//! the one field the termination harness consumes: `candidate.sha`, the full
//! 40-character commit the candidate was frozen at. A test admits a fixture
//! through the real validator and requires this reader to agree. Nothing else
//! in the manifest is read or trusted here.
//!
//! The bytes are bound before the field is read: the caller supplies the
//! accepted raw-byte digest from the reviewed #1609 handoff (the same
//! `ACCEPTED_MANIFEST_SHA256` the transaction runbook checks), and an edited,
//! stale or swapped manifest is refused rather than silently supplying a
//! different commit (#10251).

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::blind_journey::sha256_hex;

pub(super) fn candidate_sha(path: &Path, accepted_sha256: &str) -> Result<String, String> {
    if accepted_sha256.len() != 64 || !accepted_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "--candidate-manifest-sha256 `{accepted_sha256}` is not a 64-character SHA-256"
        ));
    }
    // Read once and hash and parse the same bytes, so the file cannot change
    // between the digest check and the field read.
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let actual = sha256_hex(&bytes);
    if !actual.eq_ignore_ascii_case(accepted_sha256) {
        return Err(format!(
            "{}: manifest bytes hash to {actual}, not the accepted --candidate-manifest-sha256 {}",
            path.display(),
            accepted_sha256.to_ascii_lowercase()
        ));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("{}: manifest is not UTF-8: {error}", path.display()))?;
    parse_candidate_sha(text).map_err(|error| format!("{}: {error}", path.display()))
}

pub(super) fn parse_candidate_sha(text: &str) -> Result<String, String> {
    let root: Value =
        serde_json::from_str(text).map_err(|error| format!("manifest is not JSON: {error}"))?;
    let sha = root
        .pointer("/candidate/sha")
        .and_then(Value::as_str)
        .ok_or("manifest has no candidate.sha")?;
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "candidate.sha `{sha}` is not a full 40-character commit"
        ));
    }
    Ok(sha.to_ascii_lowercase())
}
