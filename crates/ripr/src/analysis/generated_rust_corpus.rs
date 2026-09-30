//! Analyzable Rust corpus for repo seam inventory.
//!
//! `ripr check` already drops generated Rust through
//! [`is_generated_rust_file_with_patterns`]. Seam inventory used to walk the
//! raw `discover_rust_files` set, so the same files became seams in
//! repo-exposure. This module is the single owner for the analyzable file
//! set, the skipped generated paths, and the fingerprint of that same set.

use super::language::is_generated_rust_file_with_patterns;
use super::seam_cache::corpus_fingerprint;
use super::workspace;
use crate::config::RiprConfig;
use std::path::{Path, PathBuf};

/// Discovered Rust files after the generated-source predicate `ripr check`
/// uses, plus the skipped paths and the fingerprint of the analyzable set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnalyzableRustCorpus {
    pub(crate) analyzable: Vec<PathBuf>,
    pub(crate) skipped_generated: Vec<PathBuf>,
    pub(crate) fingerprint: Option<String>,
}

/// Stat-only size of the analyzable corpus: the file count and total source
/// bytes an index build over the workspace would read. Nothing is read or
/// materialized, so a caller can refuse an over-ceiling payload before the
/// corpus is loaded (#4388).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CorpusPayloadSize {
    pub(crate) file_count: usize,
    pub(crate) total_bytes: u64,
}

/// Measure the analyzable corpus without loading it: same discovery the
/// inventory uses, then metadata-only byte totals. A file whose metadata
/// cannot be read fails closed — the later index build could not read it
/// either.
pub(crate) fn analyzable_corpus_payload_size(
    root: &Path,
    config: &RiprConfig,
) -> Result<CorpusPayloadSize, String> {
    let corpus = discover_analyzable_rust_corpus(root, config)?;
    let mut total_bytes = 0u64;
    for path in &corpus.analyzable {
        let metadata = std::fs::metadata(root.join(path))
            .map_err(|err| format!("stat {} failed: {err}", path.display()))?;
        total_bytes = total_bytes.saturating_add(metadata.len());
    }
    Ok(CorpusPayloadSize {
        file_count: corpus.analyzable.len(),
        total_bytes,
    })
}

/// Walk `root` and keep the generated-Rust files `ripr check` skips out of
/// the inventory corpus. The fingerprint covers only analyzable files so a
/// generated-file edit does not bust the seam cache.
pub(crate) fn discover_analyzable_rust_corpus(
    root: &Path,
    config: &RiprConfig,
) -> Result<AnalyzableRustCorpus, String> {
    let discovered = workspace::discover_rust_files(root)?;
    Ok(partition_analyzable_rust_corpus(root, discovered, config))
}

pub(crate) fn partition_analyzable_rust_corpus(
    root: &Path,
    discovered: Vec<PathBuf>,
    config: &RiprConfig,
) -> AnalyzableRustCorpus {
    let patterns = config.languages().generated_file_patterns();
    let mut analyzable = Vec::new();
    let mut skipped_generated = Vec::new();
    for path in discovered {
        if is_generated_rust_file_with_patterns(&path, patterns) {
            skipped_generated.push(path);
        } else {
            analyzable.push(path);
        }
    }
    let fingerprint = corpus_fingerprint(root, &analyzable);
    AnalyzableRustCorpus {
        analyzable,
        skipped_generated,
        fingerprint,
    }
}

#[cfg(test)]
mod tests {
    use super::partition_analyzable_rust_corpus;
    use crate::config::RiprConfig;
    use std::path::{Path, PathBuf};

    fn paths(values: &[&str]) -> Vec<PathBuf> {
        values.iter().map(PathBuf::from).collect()
    }

    fn portable(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn partition_drops_conventional_generated_rust_and_keeps_near_misses() {
        let corpus = partition_analyzable_rust_corpus(
            Path::new("/tmp/unused-root"),
            paths(&[
                "src/lib.rs",
                "src/bindings.rs",
                "src/schema.rs",
                "src/bind.rs",
                "src/ffi.rs",
                "src/gen/model.rs",
                "src/out_of_band.rs",
            ]),
            &RiprConfig::default(),
        );
        let analyzable: Vec<String> = corpus.analyzable.iter().map(|p| portable(p)).collect();
        let skipped: Vec<String> = corpus
            .skipped_generated
            .iter()
            .map(|p| portable(p))
            .collect();
        assert_eq!(
            analyzable,
            vec![
                "src/lib.rs".to_string(),
                "src/bind.rs".to_string(),
                "src/ffi.rs".to_string(),
                "src/out_of_band.rs".to_string()
            ]
        );
        assert_eq!(
            skipped,
            vec![
                "src/bindings.rs".to_string(),
                "src/schema.rs".to_string(),
                "src/gen/model.rs".to_string()
            ]
        );
    }

    #[test]
    fn partition_applies_configured_generated_file_patterns() -> Result<(), String> {
        let config = crate::config::tests_only_parse(
            "[languages.rust]\ngenerated_file_patterns = [\"src/ffi.rs\"]\n",
        )
        .map_err(|error| format!("fixture config parses: {error}"))?;
        let corpus = partition_analyzable_rust_corpus(
            Path::new("/tmp/unused-root"),
            paths(&["src/lib.rs", "src/ffi.rs"]),
            &config,
        );
        assert_eq!(
            corpus
                .analyzable
                .iter()
                .map(|p| portable(p))
                .collect::<Vec<_>>(),
            vec!["src/lib.rs".to_string()]
        );
        assert_eq!(
            corpus
                .skipped_generated
                .iter()
                .map(|p| portable(p))
                .collect::<Vec<_>>(),
            vec!["src/ffi.rs".to_string()]
        );
        Ok(())
    }
}
