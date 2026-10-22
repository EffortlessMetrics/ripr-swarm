//! Reader for the frozen-candidate manifest (#1609). The DTO and its admission
//! validator are `LiveHeadSnapshot` in the release candidate harness; this is
//! the one field the termination harness consumes: `candidate.sha`, the full
//! 40-character commit the candidate was frozen at. A test admits a fixture
//! through the real validator and requires this reader to agree. Nothing else
//! in the manifest is read or trusted here.

use std::fs;
use std::path::Path;

use serde_json::Value;

pub(super) fn candidate_sha(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    parse_candidate_sha(&text).map_err(|error| format!("{}: {error}", path.display()))
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
