//! Passive admission of the existing full Rust inventory's consumed corpus.

use crate::analysis::seam_cache::CACHE_SCHEMA_VERSION;
use crate::analysis::{
    workspace_cache_key_and_production_files_at_with_config, workspace_cache_key_at_with_config,
};
use crate::config::RiprConfig;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// The producer's corpus content-hash spelling: 16 lowercase hex digits.
pub(crate) fn is_corpus_hash_shape(hash: &str) -> bool {
    hash.len() == 16
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// A corpus mismatch is distinct from a named-file mismatch: a newly added
/// test can change a gap while every file in its old repair route is intact.
pub(crate) enum CorpusSubjectCheck {
    Current,
    Stale,
    Unverifiable(&'static str),
}

/// Compare the captured analysis key with the same owner's current key.
/// This reuses its corpus discovery, generated-file exclusions, content hash,
/// and fingerprint cache; it never runs the full seam classifier.
pub(crate) fn check_rust_corpus_subject(
    root: &Path,
    stamp: Option<&Value>,
    config: &RiprConfig,
) -> CorpusSubjectCheck {
    let hash = match rust_corpus_hash(stamp) {
        Ok(hash) => hash,
        Err(reason) => return reason,
    };
    match workspace_cache_key_at_with_config(root, config) {
        Ok(key) if key.files_content_hash == hash => CorpusSubjectCheck::Current,
        Ok(_) => CorpusSubjectCheck::Stale,
        Err(_) => CorpusSubjectCheck::Unverifiable("source_subject_corpus_unreadable"),
    }
}

fn rust_corpus_hash(stamp: Option<&Value>) -> Result<&str, CorpusSubjectCheck> {
    let Some(corpus) = stamp.and_then(|stamp| stamp.get("corpus")) else {
        return Err(CorpusSubjectCheck::Unverifiable(
            "source_subject_corpus_missing",
        ));
    };
    if let Some(reason) = corpus.get("unavailable").and_then(Value::as_str) {
        return Err(CorpusSubjectCheck::Unverifiable(match reason {
            "input_source_subject_corpus_root_mismatch" => "source_subject_corpus_root_mismatch",
            "input_source_subject_corpus_root_missing" => "source_subject_corpus_root_missing",
            _ => "source_subject_corpus_malformed",
        }));
    }
    let (Some(scope), Some(schema), Some(hash)) = (
        corpus.get("scope").and_then(Value::as_str),
        corpus.get("cache_schema_version").and_then(Value::as_str),
        corpus.get("files_content_hash").and_then(Value::as_str),
    ) else {
        return Err(CorpusSubjectCheck::Unverifiable(
            "source_subject_corpus_malformed",
        ));
    };
    if scope != "rust_workspace" {
        return Err(CorpusSubjectCheck::Unverifiable(
            "source_subject_corpus_unsupported_scope",
        ));
    }
    if schema != CACHE_SCHEMA_VERSION {
        return Err(CorpusSubjectCheck::Unverifiable(
            "source_subject_corpus_unsupported_schema",
        ));
    }
    if !is_corpus_hash_shape(hash) {
        return Err(CorpusSubjectCheck::Unverifiable(
            "source_subject_corpus_malformed",
        ));
    }
    Ok(hash)
}

pub(crate) fn production_files_subject(files: &[PathBuf]) -> Option<Value> {
    let paths = files
        .iter()
        .map(|path| {
            let parts = path
                .components()
                .map(|part| match part {
                    Component::Normal(part) => part.to_str(),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            let path = parts.join("/");
            rust_workspace_path_gap(&path).is_none().then_some(path)
        })
        .collect::<Option<BTreeSet<_>>>()?;
    Some(serde_json::json!(paths))
}

/// A present empty list establishes no covered paths. Missing, malformed,
/// duplicate or ambiguous membership never establishes workspace coverage.
pub(crate) fn analyzed_production_files(corpus: &Value) -> Option<BTreeSet<&str>> {
    let files = corpus.get("analyzed_production_files")?.as_array()?;
    let paths = files
        .iter()
        .map(|file| {
            let path = file.as_str()?;
            rust_workspace_path_gap(path).is_none().then_some(path)
        })
        .collect::<Option<BTreeSet<_>>>()?;
    (paths.len() == files.len()).then_some(paths)
}

/// Gate admission uses the same discovery/role owner as the producer. A
/// matching Rust content hash alone cannot establish production membership
/// after a source-role policy or Cargo target declaration changes.
pub(crate) fn rust_workspace_coverage_is_current(
    root: &Path,
    coverage: &Value,
    config: &RiprConfig,
) -> bool {
    let Ok(hash) = rust_corpus_hash(Some(coverage)) else {
        return false;
    };
    let Some(captured) = coverage.get("corpus").and_then(analyzed_production_files) else {
        return false;
    };
    let Ok((key, files)) = workspace_cache_key_and_production_files_at_with_config(root, config)
    else {
        return false;
    };
    let Some(current) = production_files_subject(&files) else {
        return false;
    };
    let Some(current) = current.as_array() else {
        return false;
    };
    key.files_content_hash == hash
        && captured
            == current
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
}

/// Normalized portable Rust path shape, shared by membership admission and
/// baseline consumption. Shape is necessary; only actual membership is coverage.
pub(crate) fn rust_workspace_path_gap(raw: &str) -> Option<&'static str> {
    if raw != raw.trim() {
        return Some("the path has surrounding whitespace");
    }
    if raw.contains('\\') {
        return Some("the path is not a normalized `/`-separated path");
    }
    if raw.starts_with('/')
        || raw
            .split('/')
            .next()
            .is_some_and(|first| first.contains(':'))
    {
        return Some("the path is absolute or carries a drive prefix");
    }
    if raw
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Some("the path is not normalized (empty, `.` or `..` component)");
    }
    if !raw.ends_with(".rs") {
        return Some("the run covered Rust sources only");
    }
    if crate::analysis::path_enters_ignored_dir(raw) {
        return Some("the producer's workspace walk skips that directory");
    }
    None
}
