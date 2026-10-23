//! Render the diff-scoped check findings as a producer-identified artifact.
//!
//! `check-findings-json` is the unbounded `check --format json` document with
//! the same top-level `artifact` identity envelope `repo-exposure-json` binds
//! (ADR 0019 shared projection, ADR 0020 amendment). It exists so a
//! gap-keyed `agent verify` can reject a fabricated, edited, foreign or
//! incomplete findings snapshot instead of trusting hand-authored JSON.
//!
//! The document is never findings-bounded: a prefix of the findings would let
//! a "closed" gap read as absent only because it was cut.

use crate::agent::artifact::{
    CONTENT_SHA256_PLACEHOLDER, RepoExposureArtifactContext, Sha256Writer,
    check_findings_artifact_metadata,
};
use crate::app::CheckOutput;
use crate::config::RiprConfig;

/// Render the check findings bound to their subject. The `content_sha256`
/// commitment covers the exact emitted bytes with the digest field replaced
/// by the fixed placeholder: the first pass hashes the placeholder document,
/// the second re-renders with the digest.
pub(crate) fn render_check_findings_json_with_context(
    output: &CheckOutput,
    config: &RiprConfig,
    context: &RepoExposureArtifactContext,
) -> Result<String, String> {
    use std::io::Write as _;

    let body = crate::output::render::render_check_json_unbounded(output, config);
    let placeholder = check_findings_artifact_metadata(context, CONTENT_SHA256_PLACEHOLDER)?;
    let hashed = splice_artifact_member(&body, &placeholder)?;
    let mut hasher = Sha256Writer::new();
    hasher
        .write_all(hashed.as_bytes())
        .map_err(|err| format!("hash check findings JSON failed: {err}"))?;
    let content_sha256 = hasher.finish();
    let mut metadata = placeholder;
    metadata["content_sha256"] = serde_json::Value::String(content_sha256);
    splice_artifact_member(&body, &metadata)
}

/// Insert `"artifact": {...}` as the first member after the document's
/// opening brace, leaving every other byte of the check document unchanged.
fn splice_artifact_member(body: &str, artifact: &serde_json::Value) -> Result<String, String> {
    let rest = body
        .strip_prefix("{\n")
        .ok_or_else(|| "check findings JSON did not start with an object".to_string())?;
    Ok(format!("{{\n  \"artifact\": {artifact},\n{rest}"))
}
