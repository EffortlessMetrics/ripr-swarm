//! Passive admission of the existing full Rust inventory's consumed corpus.

use crate::analysis::seam_cache::CACHE_SCHEMA_VERSION;
use crate::analysis::workspace_cache_key_at_with_config;
use crate::config::RiprConfig;
use serde_json::Value;
use std::path::Path;

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
    let Some(corpus) = stamp.and_then(|stamp| stamp.get("corpus")) else {
        return CorpusSubjectCheck::Unverifiable("source_subject_corpus_missing");
    };
    if let Some(reason) = corpus.get("unavailable").and_then(Value::as_str) {
        return CorpusSubjectCheck::Unverifiable(match reason {
            "input_source_subject_corpus_root_mismatch" => "source_subject_corpus_root_mismatch",
            "input_source_subject_corpus_root_missing" => "source_subject_corpus_root_missing",
            _ => "source_subject_corpus_malformed",
        });
    }
    let (Some(scope), Some(schema), Some(hash)) = (
        corpus.get("scope").and_then(Value::as_str),
        corpus.get("cache_schema_version").and_then(Value::as_str),
        corpus.get("files_content_hash").and_then(Value::as_str),
    ) else {
        return CorpusSubjectCheck::Unverifiable("source_subject_corpus_malformed");
    };
    if scope != "rust_workspace" {
        return CorpusSubjectCheck::Unverifiable("source_subject_corpus_unsupported_scope");
    }
    if schema != CACHE_SCHEMA_VERSION {
        return CorpusSubjectCheck::Unverifiable("source_subject_corpus_unsupported_schema");
    }
    if !is_corpus_hash_shape(hash) {
        return CorpusSubjectCheck::Unverifiable("source_subject_corpus_malformed");
    }
    match workspace_cache_key_at_with_config(root, config) {
        Ok(key) if key.files_content_hash == hash => CorpusSubjectCheck::Current,
        Ok(_) => CorpusSubjectCheck::Stale,
        Err(_) => CorpusSubjectCheck::Unverifiable("source_subject_corpus_unreadable"),
    }
}
