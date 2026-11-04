//! Sequential compact publication: only the manifest and one evidence window
//! remain live. A generation becomes visible only after every window succeeds.

use super::*;

pub(in crate::analysis::seam_cache) fn publish_compact_windows(
    cache: &RepoSeamFactCache,
    key: &RepoSeamCacheKey,
    total_seams: usize,
    lexical_fallback_files: &[PathBuf],
    window_limit: usize,
    byte_ceiling: usize,
    mut producer: impl FnMut(usize) -> Result<Option<Vec<ClassifiedSeam>>, String>,
) -> Result<CacheStoreStatus, String> {
    key.require_reusable()?;
    if window_limit == 0 {
        return Err("compact cache window limit must be positive".to_string());
    }
    if byte_ceiling == 0 {
        return Err("classified seam cache encoded shard ceiling must be positive".to_string());
    }
    // An empty complete result uses the ordinary empty envelope, never an empty
    // shard or a synthetic producer call. This also checks metadata byte bounds.
    if total_seams == 0 {
        return publish_classified_generation(
            cache,
            key,
            &[],
            None,
            lexical_fallback_files,
            window_limit,
            byte_ceiling,
        );
    }
    crate::analysis::cancellation::checkpoint()?;
    let _publication_lock = match acquire_publication_lock(cache, key)? {
        PublicationLock::Held(lock) => Some(lock),
        PublicationLock::Unsupported => None,
        PublicationLock::Busy => {
            return Ok(CacheStoreStatus {
                label: PUBLICATION_BUSY_LABEL.to_string(),
                advisory: None,
            });
        }
    };
    let shard_count = total_seams.div_ceil(window_limit);
    let started_at = std::time::SystemTime::now();
    let sharded_dir = cache.sharded_entry_dir(key);
    std::fs::create_dir_all(&sharded_dir)
        .map_err(|err| format!("create sharded cache dir failed: {err}"))?;
    let previous = read_previous_manifest(cache, key);
    let publication_id = publication_id();
    let generation_dir = sharded_dir.join(format!("g{publication_id}"));
    std::fs::create_dir_all(&generation_dir)
        .map_err(|err| format!("create sharded cache generation dir failed: {err}"))?;
    let mut unpublished = UnpublishedGeneration::new(generation_dir);
    // Avoid reserving based on an unchecked caller total before any evidence is
    // produced. References grow with shard count, evidence does not.
    let mut shard_refs = Vec::new();
    let mut remaining = total_seams;
    for index in 0..shard_count {
        crate::analysis::cancellation::checkpoint()?;
        #[cfg(test)]
        if fail_after_shards_value() == Some(index) {
            clear_fail_after_shards();
            return Err(
                "injected sharded cache publication failure before remaining shards".into(),
            );
        }
        let requested = remaining.min(window_limit);
        let Some(window) = producer(requested)? else {
            return Ok(CacheStoreStatus {
                label: "skipped_compact_window_unavailable".to_string(),
                advisory: None,
            });
        };
        if window.len() != requested {
            return Err(format!(
                "compact cache producer returned {} seams for window {index}, expected {requested}",
                window.len()
            ));
        }
        crate::analysis::cancellation::checkpoint()?;
        let envelope = borrowed_shard_envelope(key, index, shard_count, &window);
        if !checksummed_pretty_fits(&envelope, byte_ceiling)? {
            return Ok(CacheStoreStatus {
                label: format!("skipped_oversized_compact_window_{index}_ceiling_{byte_ceiling}"),
                advisory: Some(format!(
                    "compact cache window {index} exceeds the {byte_ceiling}-byte encoded shard ceiling"
                )),
            });
        }
        shard_refs.push(write_shard(
            key,
            &window,
            &(0..requested),
            index,
            shard_count,
            &sharded_dir,
            &publication_id,
        )?);
        remaining -= requested;
        // `window` is dropped here, before the next producer invocation.
    }
    crate::analysis::cancellation::checkpoint()?;
    #[cfg(test)]
    if fail_after_shards_value() == Some(shard_count) {
        clear_fail_after_shards();
        return Err("injected sharded cache publication failure before manifest".into());
    }
    let mut parked = ParkedSingleEntry::park(
        cache.entry_path(key),
        cache.sharded_manifest_path(key),
        &publication_id,
    )?;
    #[cfg(test)]
    if take_fail_after_park() {
        return Err(
            "injected sharded cache publication failure after parking the single entry".into(),
        );
    }
    crate::analysis::cancellation::checkpoint()?;
    let manifest = ShardedCacheManifest::new(
        key.clone(),
        total_seams,
        shard_count,
        shard_refs,
        None,
        lexical_fallback_files.to_vec(),
    );
    let digest = manifest.expected_digest()?;
    stream_checksummed_cache_file(
        cache.sharded_manifest_path(key),
        "sharded cache manifest",
        &manifest,
        digest,
    )?;
    if let Some(parked) = parked.as_mut() {
        parked.commit();
    }
    unpublished.retain();
    #[cfg(test)]
    run_after_commit_hook();
    remove_single_entry_older_than(&cache.entry_path(key), started_at);
    if let Some(previous) = previous {
        remove_replaced_generation_files(&sharded_dir, &previous, &publication_id);
    }
    sweep_orphan_generations(
        &sharded_dir,
        std::time::SystemTime::now(),
        ORPHAN_GENERATION_GRACE,
    );
    Ok(CacheStoreStatus {
        label: format!("sharded_ok_seams_{total_seams}_shards_{shard_count}_limit_{window_limit}"),
        advisory: None,
    })
}
