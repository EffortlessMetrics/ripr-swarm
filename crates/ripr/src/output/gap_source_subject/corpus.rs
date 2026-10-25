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

/// Non-Rust analysis inputs that shape findings without changing any Rust
/// file: the build-feature environment, the classified configuration, the
/// test-intent file and the suppressions file. They are the producer cache
/// key's own hashes, so a gate recomputes them with the same helper.
const ANALYSIS_INPUT_KEYS: [&str; 8] = [
    "cfg_features_hash",
    "config_hash",
    "test_intent_hash",
    "suppressions_hash",
    "workspace_manifests_hash",
    "lockfile_hash",
    "toolchain_hash",
    "analyzer_version",
];

pub(crate) fn analysis_inputs_subject(
    key: &crate::analysis::seam_cache::RepoSeamCacheKey,
) -> Value {
    serde_json::json!({
        "cfg_features_hash": key.cfg_features_hash,
        "config_hash": key.config_hash,
        "test_intent_hash": key.test_intent_hash,
        "suppressions_hash": key.suppressions_hash,
        "workspace_manifests_hash": key.workspace_manifests_hash,
        "lockfile_hash": key.lockfile_hash,
        "toolchain_hash": key.toolchain_hash,
        "analyzer_version": key.analyzer_version,
    })
}

/// The disclosed input identity, `None` unless every component is a
/// non-empty string (missing or partial identity never establishes coverage).
pub(crate) fn analysis_inputs(corpus: &Value) -> Option<[&str; 8]> {
    let inputs = corpus.get("analysis_inputs")?.as_object()?;
    if inputs.len() != ANALYSIS_INPUT_KEYS.len() {
        return None;
    }
    let mut out = [""; 8];
    for (slot, name) in out.iter_mut().zip(ANALYSIS_INPUT_KEYS) {
        let value = inputs.get(name)?.as_str()?;
        if value.is_empty() {
            return None;
        }
        *slot = value;
    }
    Some(out)
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
/// matching Rust content hash alone cannot establish production membership or
/// the non-Rust analysis inputs. Returns the classes that differ from the
/// disclosure (`content`, `membership`, or an input name such as `config`,
/// `test_intent`, `suppressions`, `cfg_features`, `workspace_manifests`,
/// `lockfile`, `toolchain`, `analyzer_version`); `unverifiable` when the
/// disclosure is malformed or the current workspace cannot be read. Empty
/// means current. The hashes are the cache key's 64-bit FNV digests:
/// non-adversarial change detection, not authentication.
pub(crate) fn rust_workspace_coverage_mismatches(
    root: &Path,
    coverage: &Value,
    config: &RiprConfig,
) -> Vec<&'static str> {
    let unverifiable = || vec!["unverifiable"];
    let Ok(hash) = rust_corpus_hash(Some(coverage)) else {
        return unverifiable();
    };
    let Some(captured) = coverage.get("corpus").and_then(analyzed_production_files) else {
        return unverifiable();
    };
    let Some(inputs) = coverage.get("corpus").and_then(analysis_inputs) else {
        return unverifiable();
    };
    let Ok((key, files)) = workspace_cache_key_and_production_files_at_with_config(root, config)
    else {
        return unverifiable();
    };
    let Some(current) = production_files_subject(&files) else {
        return unverifiable();
    };
    let current_wrapper = serde_json::json!({"analysis_inputs": analysis_inputs_subject(&key)});
    let Some(current_inputs) = analysis_inputs(&current_wrapper) else {
        return unverifiable();
    };
    let mut mismatched = Vec::new();
    if key.files_content_hash != hash {
        mismatched.push("content");
    }
    let current_members = current.as_array().map(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>()
    });
    if current_members.as_ref() != Some(&captured) {
        mismatched.push("membership");
    }
    for ((name, captured), current) in ANALYSIS_INPUT_KEYS.iter().zip(inputs).zip(current_inputs) {
        if captured != current {
            mismatched.push(name.strip_suffix("_hash").unwrap_or(name));
        }
    }
    mismatched
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_membership_is_sorted_deduplicated_and_fails_closed() {
        let paths = |items: &[&str]| items.iter().map(PathBuf::from).collect::<Vec<_>>();
        let subject =
            production_files_subject(&paths(&["src/z.rs", "crates/a/src/lib.rs", "src/z.rs"]));
        assert_eq!(
            subject,
            Some(serde_json::json!(["crates/a/src/lib.rs", "src/z.rs"]))
        );
        // One unrepresentable path withholds the whole list, never a partial one.
        for bad in [
            "/abs/x.rs",
            "../x.rs",
            "src/x.py",
            "target/gen.rs",
            "fixtures/c/src/lib.rs",
        ] {
            assert!(
                production_files_subject(&paths(&["src/ok.rs", bad])).is_none(),
                "{bad}"
            );
        }
        let corpus = serde_json::json!({"analyzed_production_files": []});
        assert_eq!(analyzed_production_files(&corpus), Some(BTreeSet::new()));
    }
}
