use super::build::CachedRustIndex;
use super::model::{RustIndex, WorkspaceRootAuthority};
use super::super::syntax::{
    LexicalRustSyntaxAdapter, RaRustSyntaxAdapter, RustSyntaxAdapter,
};
use crate::analysis::cancellation;
use crate::analysis::seam_cache::{
    CacheLoad, FileFactCacheStats, RepoFileFactCache, RepoFileFactCacheKey,
};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Keep cache admission, parse, and insertion temporaries bounded together.
///
/// The previous builder parsed misses in 64-file batches but retained every
/// completed `FileFacts` value until the whole corpus had finished parsing.
/// On a cold large workspace that made the batch size a CPU-concurrency bound,
/// not a memory-lifetime bound. Each batch is now inserted before the next one
/// is admitted, so hit and miss facts from completed batches are dropped.
const PARSE_BATCH_FILES: usize = 64;

pub(super) fn build_index_from_loaded_files_with_cache(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<CachedRustIndex, String> {
    let cache = RepoFileFactCache::at(root);
    build_index_with_file_fact_cache(
        root,
        files,
        &RaRustSyntaxAdapter,
        &LexicalRustSyntaxAdapter,
        &cache,
        || cache.known_file_paths(),
    )
}

fn build_index_with_file_fact_cache(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    adapter: &(dyn RustSyntaxAdapter + Send + Sync),
    fallback: &(dyn RustSyntaxAdapter + Send + Sync),
    cache: &RepoFileFactCache,
    mut load_known_file_paths: impl FnMut() -> HashSet<PathBuf>,
) -> Result<CachedRustIndex, String> {
    enum Pending {
        Ready(super::FileFacts),
        Parse { key: RepoFileFactCacheKey },
    }

    let mut known_cached_file_paths: Option<HashSet<PathBuf>> = None;
    let mut stats = FileFactCacheStats::default();
    let mut first_corrupt_reason: Option<String> = None;
    let mut index = RustIndex::default();

    for batch in files.chunks(PARSE_BATCH_FILES) {
        if let Err(error) = cancellation::checkpoint() {
            return fail_with_corrupt_warning(&stats, first_corrupt_reason.as_deref(), error);
        }

        // Cache lookup remains sequential so hit/miss/invalidation accounting
        // and deterministic input ordering match the existing builder.
        let mut pending = Vec::with_capacity(batch.len());
        for (file, bytes) in batch {
            if let Err(error) = cancellation::checkpoint() {
                return fail_with_corrupt_warning(
                    &stats,
                    first_corrupt_reason.as_deref(),
                    error,
                );
            }
            let key = RepoFileFactCacheKey::new(file, bytes);
            match cache.load_file_facts(&key) {
                CacheLoad::Hit(facts) => {
                    stats.hits += 1;
                    pending.push(Pending::Ready(facts));
                }
                CacheLoad::Miss => {
                    stats.misses += 1;
                    if known_cached_file_paths
                        .get_or_insert_with(&mut load_known_file_paths)
                        .contains(file)
                    {
                        stats.invalidated_files.insert(file.clone());
                    }
                    pending.push(Pending::Parse { key });
                }
                CacheLoad::CorruptIgnored { reason } => {
                    stats.corrupt_ignored += 1;
                    first_corrupt_reason.get_or_insert(reason);
                    pending.push(Pending::Parse { key });
                }
            }
        }

        // Only this batch's misses are live outside the canonical index.
        // Indexed parallel collection preserves the deterministic input map.
        let parse_positions = pending
            .iter()
            .enumerate()
            .filter_map(|(position, entry)| {
                matches!(entry, Pending::Parse { .. }).then_some(position)
            })
            .collect::<Vec<_>>();
        let results = parse_positions
            .par_iter()
            .map(|&position| {
                let (file, bytes) = &batch[position];
                (
                    position,
                    summarize_loaded_file(file, bytes, adapter, fallback),
                )
            })
            .collect::<Vec<_>>();
        let mut parsed = Vec::new();
        parsed.resize_with(batch.len(), || None);
        for (position, result) in results {
            parsed[position] = Some(result);
        }

        // Store and insert in original order. Once this loop finishes, every
        // `Pending` and parsed result owned by the batch is dropped before the
        // next batch is admitted.
        for (position, entry) in pending.into_iter().enumerate() {
            let (file, bytes) = &batch[position];
            if super::rust_source_text(bytes).not_utf8 {
                index.non_utf8_sources.insert(file.clone());
            }
            let summary = match entry {
                Pending::Ready(facts) => facts,
                Pending::Parse { key } => {
                    let facts = match parsed[position].take() {
                        Some(Ok(facts)) => facts,
                        Some(Err(error)) => {
                            return fail_with_corrupt_warning(
                                &stats,
                                first_corrupt_reason.as_deref(),
                                error,
                            );
                        }
                        None => {
                            return fail_with_corrupt_warning(
                                &stats,
                                first_corrupt_reason.as_deref(),
                                format!(
                                    "missing parse result for {}",
                                    root.join(file).display()
                                ),
                            );
                        }
                    };
                    match cache.store_file_facts(&key, &facts) {
                        Ok(()) => stats.stores += 1,
                        Err(error) => stats.record_store_failure(file.clone(), error),
                    }
                    facts
                }
            };
            insert_file_summary(&mut index, file.clone(), summary);
            if let Err(error) = cancellation::checkpoint() {
                return fail_with_corrupt_warning(
                    &stats,
                    first_corrupt_reason.as_deref(),
                    error,
                );
            }
        }
    }

    emit_corrupt_entries_warning(stats.corrupt_ignored, first_corrupt_reason.as_deref());
    super::includes::resolve_repository_local_includes(root, &mut index);
    index.workspace_authority = Some(WorkspaceRootAuthority::from_index(root, &index.files));
    index.package_names = manifest_package_names(root);
    Ok(CachedRustIndex {
        index,
        file_fact_cache: stats,
    })
}

fn fail_with_corrupt_warning<T>(
    stats: &FileFactCacheStats,
    first_reason: Option<&str>,
    error: String,
) -> Result<T, String> {
    emit_corrupt_entries_warning(stats.corrupt_ignored, first_reason);
    Err(error)
}

fn emit_corrupt_entries_warning(count: usize, first_reason: Option<&str>) {
    if let Some(reason) = first_reason {
        if count == 1 {
            eprintln!("ripr: repo file fact cache entry ignored ({reason})");
        } else {
            eprintln!(
                "ripr: {count} repo file fact cache entries ignored; first: ({reason})"
            );
        }
    }
}

fn summarize_loaded_file(
    file: &Path,
    bytes: &[u8],
    adapter: &dyn RustSyntaxAdapter,
    fallback: &dyn RustSyntaxAdapter,
) -> Result<super::FileFacts, String> {
    let source = super::rust_source_text(bytes);
    if source.not_utf8 {
        let mut facts = fallback.summarize_file(file, &source.text)?;
        facts.used_lexical_fallback = true;
        return Ok(facts);
    }
    match adapter.summarize_file(file, &source.text) {
        Ok(facts) => Ok(facts),
        Err(_) => {
            let mut facts = fallback.summarize_file(file, &source.text)?;
            facts.used_lexical_fallback = true;
            Ok(facts)
        }
    }
}

fn insert_file_summary(index: &mut RustIndex, file: PathBuf, summary: super::FileFacts) {
    // Canonical fact de-duplication is a separate representation repair (#5026).
    // Preserve current semantics here while bounding only temporary ownership.
    index.tests.extend(summary.tests.clone());
    index.functions.extend(summary.functions.clone());
    index.files.insert(file, summary);
}

fn manifest_package_names(root: &Path) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return names;
    };
    let Ok(value) = text.parse::<toml::Table>() else {
        return names;
    };
    let mut insert = |name: &str| {
        names.insert(name.to_string());
        names.insert(name.replace('-', "_"));
    };
    if let Some(name) = value
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
    {
        insert(name);
    }
    if let Some(name) = value
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(toml::Value::as_str)
    {
        insert(name);
    }
    names
}
