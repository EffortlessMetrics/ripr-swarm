//! Self-integrity gate for committed repo-exposure reports (#12602).
//!
//! Repo-seam honesty reports carry `artifact.content_sha256` under the
//! `raw_json_placeholder_v1` rule documented in `docs/OUTPUT_SCHEMA.md`. The
//! staleness guard compares judged seam fields only; this module verifies that
//! the declared digest still matches the exact committed bytes.

use super::normalize_path;
use crate::dogfood::repo_rooted_fixture_path;
use crate::reports::release_server::hex_lower;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const REPORTS_DIR: &str = "fixtures/evidence-promotion-honesty-corpus/reports";
const RAW_JSON_PLACEHOLDER_V1: &str = "raw_json_placeholder_v1";
const CONTENT_PLACEHOLDER: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// Locate the value span of the one `content_sha256` key in a raw artifact
/// document. Mirrors `reports/release_negative.rs` (#2824).
fn content_commitment_value_span(text: &str) -> Result<std::ops::Range<usize>, String> {
    let key = "\"content_sha256\"";
    if text.matches(key).count() != 1 {
        return Err(format!("expected exactly one {key} key"));
    }
    let Some(key_start) = text.find(key) else {
        return Err(format!("expected the {key} key"));
    };
    let value_search_start = key_start + key.len();
    let Some(value_offset) = text[value_search_start..].find('"') else {
        return Err(format!("{key} must have a string value"));
    };
    let value_start = value_search_start + value_offset + 1;
    let Some(end_offset) = text[value_start..].find('"') else {
        return Err(format!("{key} value must be terminated"));
    };
    Ok(value_start..value_start + end_offset)
}

/// Recompute the `raw_json_placeholder_v1` digest over exact file bytes.
pub(super) fn recompute_raw_json_placeholder_v1(raw: &str) -> Result<String, String> {
    let mut text = raw.to_string();
    let span = content_commitment_value_span(&text)?;
    text.replace_range(span, CONTENT_PLACEHOLDER);
    // Only the governed commitment value is replaced; unrelated occurrences
    // of the placeholder-shaped text remain part of the bytes being hashed.
    Ok(format!(
        "sha256:{}",
        hex_lower(&Sha256::digest(text.as_bytes()))
    ))
}

fn declared_commitment(value: &Value) -> Option<&str> {
    value
        .pointer("/artifact/content_sha256")
        .and_then(Value::as_str)
}

fn canonicalization(value: &Value) -> Option<&str> {
    value
        .pointer("/artifact/canonicalization")
        .and_then(Value::as_str)
}

fn verify_one_report(path: &Path, text: &str, value: &Value) -> Option<String> {
    let path_label = normalize_path(path);
    if value.pointer("/artifact/canonicalization").is_some() && declared_commitment(value).is_none()
    {
        return Some(format!(
            "{path_label}: repo-exposure artifact envelope is missing artifact.content_sha256"
        ));
    }
    let declared = declared_commitment(value)?;
    let Some(canonicalization) = canonicalization(value) else {
        return Some(format!(
            "{path_label}: artifact.content_sha256 is present but artifact.canonicalization is missing (status: unknown)"
        ));
    };
    if canonicalization != RAW_JSON_PLACEHOLDER_V1 {
        return Some(format!(
            "{path_label}: artifact canonicalization `{canonicalization}` is not recognised (status: unknown)"
        ));
    }
    let recomputed = match recompute_raw_json_placeholder_v1(text) {
        Ok(digest) => digest,
        Err(error) => {
            return Some(format!(
                "{path_label}: could not recompute raw_json_placeholder_v1 digest: {error}"
            ));
        }
    };
    if recomputed == declared {
        return None;
    }
    Some(format!(
        "{path_label}: artifact.content_sha256 is stale (declared `{declared}`, recomputed `{recomputed}`). \
         Recommit with raw_json_placeholder_v1 over the exact file bytes, or regenerate the capture with \
         `cargo run -p ripr -- check --root <report_subject> --mode draft --format repo-exposure-json` \
         and replace the committed report (never edit producer metadata by hand)"
    ))
}

fn report_paths() -> Result<Vec<PathBuf>, String> {
    let dir = repo_rooted_fixture_path(REPORTS_DIR);
    if !dir.is_dir() {
        return Err(format!("{REPORTS_DIR} is not a directory"));
    }
    let mut paths = fs::read_dir(&dir)
        .map_err(|error| format!("read {REPORTS_DIR}: {error}"))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

/// Verify every committed report under the honesty corpus reports directory
/// that carries `artifact.content_sha256`.
pub(super) fn check_committed_report_content_commitments(
    violations: &mut Vec<String>,
) -> Result<(), String> {
    for path in report_paths()? {
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", normalize_path(&path)))?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| format!("{} is not JSON: {error}", normalize_path(&path)))?;
        if let Some(violation) = verify_one_report(&path, &text, &value) {
            violations.push(violation);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_report(name: &str, body: &str) -> Result<PathBuf, String> {
        let dir = crate::tests::temp_dir("report-content-commitment");
        let path = dir.join(name);
        fs::write(&path, body).map_err(|error| format!("write temp report: {error}"))?;
        Ok(path)
    }

    #[test]
    fn stale_cache_schema_version_without_recommit_fails() -> Result<(), String> {
        let source = repo_rooted_fixture_path(
            "fixtures/evidence-promotion-honesty-corpus/reports/rust-repo-exposure-identical-inline-arms.json",
        );
        let text =
            fs::read_to_string(&source).map_err(|error| format!("read fixture report: {error}"))?;
        let tampered = text.replace(
            r#""cache_schema_version":"1.138""#,
            r#""cache_schema_version":"1.999""#,
        );
        if tampered == text {
            return Err("expected to tamper cache_schema_version".to_string());
        }
        let path = temp_report("tampered-cache-schema.json", &tampered)?;
        let value: Value = serde_json::from_str(&tampered)
            .map_err(|error| format!("parse tampered report: {error}"))?;
        let violation = verify_one_report(&path, &tampered, &value)
            .ok_or_else(|| "expected stale content_sha256 after hand edit".to_string())?;
        if !violation.contains("content_sha256 is stale") {
            return Err(format!("unexpected violation: {violation}"));
        }
        Ok(())
    }

    #[test]
    fn honest_fixture_report_passes_recompute() -> Result<(), String> {
        let path = repo_rooted_fixture_path(
            "fixtures/evidence-promotion-honesty-corpus/reports/rust-repo-exposure-identical-inline-arms.json",
        );
        let text =
            fs::read_to_string(&path).map_err(|error| format!("read fixture report: {error}"))?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| format!("parse fixture report: {error}"))?;
        if verify_one_report(&path, &text, &value).is_some() {
            return Err("honest fixture report should verify".to_string());
        }
        Ok(())
    }

    #[test]
    fn unknown_canonicalization_is_not_pass() -> Result<(), String> {
        let path = temp_report(
            "unknown-canonicalization.json",
            r#"{"artifact":{"canonicalization":"future_rule_v9","content_sha256":"sha256:abc"}}"#,
        )?;
        let text = fs::read_to_string(&path).map_err(|error| format!("read temp: {error}"))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|error| format!("parse temp: {error}"))?;
        let violation = verify_one_report(&path, &text, &value)
            .ok_or_else(|| "expected unknown canonicalization violation".to_string())?;
        if !violation.contains("not recognised") {
            return Err(format!("unexpected violation: {violation}"));
        }
        Ok(())
    }

    #[test]
    fn missing_content_sha256_on_repo_exposure_envelope_fails() -> Result<(), String> {
        let path = temp_report(
            "missing-commitment.json",
            r#"{"artifact":{"canonicalization":"raw_json_placeholder_v1","kind":"repo_exposure"},"seams":[]}"#,
        )?;
        let text = fs::read_to_string(&path).map_err(|error| format!("read temp: {error}"))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|error| format!("parse temp: {error}"))?;
        let violation = verify_one_report(&path, &text, &value)
            .ok_or_else(|| "expected missing commitment violation".to_string())?;
        if !violation.contains("missing artifact.content_sha256") {
            return Err(format!("unexpected violation: {violation}"));
        }
        Ok(())
    }

    #[test]
    fn unrelated_placeholder_shaped_text_does_not_break_recompute() -> Result<(), String> {
        let template = format!(
            r#"{{"artifact":{{"canonicalization":"raw_json_placeholder_v1","content_sha256":"{CONTENT_PLACEHOLDER}"}},"seams":[{{"expression":"literal {CONTENT_PLACEHOLDER} in seam text"}}]}}"#
        );
        let recomputed = recompute_raw_json_placeholder_v1(&template)?;
        let span = content_commitment_value_span(&template)?;
        let bytes = template.as_bytes();
        let mut file = Vec::new();
        file.write_all(&bytes[..span.start])
            .map_err(|error| format!("write prefix: {error}"))?;
        file.write_all(recomputed.as_bytes())
            .map_err(|error| format!("write digest: {error}"))?;
        file.write_all(&bytes[span.end..])
            .map_err(|error| format!("write suffix: {error}"))?;
        let committed = String::from_utf8(file).map_err(|error| format!("utf8: {error}"))?;
        let path = temp_report("placeholder-in-seam.json", &committed)?;
        let value: Value = serde_json::from_str(&committed)
            .map_err(|error| format!("parse committed report: {error}"))?;
        if verify_one_report(&path, &committed, &value).is_some() {
            return Err(
                "report with unrelated placeholder-shaped seam text should verify".to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn recommit_restores_valid_commitment() -> Result<(), String> {
        let source = repo_rooted_fixture_path(
            "fixtures/evidence-promotion-honesty-corpus/reports/rust-repo-exposure-identical-inline-arms.json",
        );
        let mut text =
            fs::read_to_string(&source).map_err(|error| format!("read fixture report: {error}"))?;
        text = text.replace(
            r#""cache_schema_version":"1.138""#,
            r#""cache_schema_version":"1.999""#,
        );
        let recomputed = recompute_raw_json_placeholder_v1(&text)?;
        let span = content_commitment_value_span(&text)?;
        let bytes = text.as_bytes();
        let mut file = Vec::new();
        file.write_all(&bytes[..span.start])
            .map_err(|error| format!("write prefix: {error}"))?;
        file.write_all(recomputed.as_bytes())
            .map_err(|error| format!("write digest: {error}"))?;
        file.write_all(&bytes[span.end..])
            .map_err(|error| format!("write suffix: {error}"))?;
        let committed = String::from_utf8(file).map_err(|error| format!("utf8: {error}"))?;
        let value: Value = serde_json::from_str(&committed)
            .map_err(|error| format!("parse recommitted report: {error}"))?;
        if verify_one_report(&source, &committed, &value).is_some() {
            return Err("recommitted report should verify".to_string());
        }
        Ok(())
    }
}
