//! Non-publishing design probe for #12506. No normal run calls this code.
use super::*;
use std::io::Write;

/// Bound retained record count and serialized record payload, not process RSS
/// or a wall-clock deadline. Index/context memory and one in-flight record
/// remain separate obligations before this can become a production prefill.
fn complete_compact_with_budget(
    files: &[PathBuf],
    index: &RustIndex,
    max_shapes: usize,
    max_payload_bytes: usize,
) -> Result<Option<Vec<ClassifiedSeam>>, String> {
    let shapes = files.iter().try_fold(0usize, |count, file| {
        count.checked_add(
            index
                .files()
                .get(file)
                .map_or(0, |facts| facts.probe_shapes.len()),
        )
    });
    if shapes.is_none_or(|count| count > max_shapes) {
        return Ok(None);
    }
    let inventory = inventory_seams_from_index_bounded(files, index, Some(max_shapes))?;
    if inventory.seams.len() != inventory.total_unique {
        return Err("prefill must never accept a truncated inventory".into());
    }
    let context = test_grip_evidence::CompactGripContext::try_new(index)?;
    let mut remaining = max_payload_bytes;
    let mut result = Vec::new();
    for seam in inventory.seams {
        cancellation::checkpoint()?;
        let evidence = test_grip_evidence::compact_evidence_for_seam(&seam, &context);
        cancellation::checkpoint()?;
        let class = seam_classification::classify_seam(&seam, &evidence);
        let record = ClassifiedSeam {
            seam,
            evidence,
            class,
        };
        let mut budget = PayloadBudget { remaining };
        match serde_json::to_writer(&mut budget, &record) {
            Ok(()) => remaining = budget.remaining,
            Err(error) if error.is_io() => return Ok(None),
            Err(error) => return Err(format!("measure compact record: {error}")),
        }
        result.push(record);
    }
    cancellation::checkpoint()?;
    Ok(Some(result))
}

struct PayloadBudget {
    remaining: usize,
}

impl Write for PayloadBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other(
                "compact prefill payload budget exhausted",
            ));
        }
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn same_index_prefill_is_complete_or_absent_and_matches_compact_cold() -> Result<(), String> {
    use crate::output::repo_exposure::render_repo_exposure_summary_json;
    let root = tests::make_tempdir("12506-prefill-probe")?;
    tests::write_file(
        &root.join("Cargo.toml"),
        "[package]\nname='prefill_probe'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    tests::write_file(
        &root.join("src/lib.rs"),
        "pub fn score(x:i32)->i32 { x+1 }\npub fn other(x:i32)->bool { x>=3 }\n#[cfg(test)] mod tests { #[test] fn score_positive() { assert!(super::score(2)>0); } }\n",
    )?;
    let config = RiprConfig::default();
    let state = collect_workspace_state(&root, &config)?;
    let mut cached = state.build_index(&config)?;
    rust_index::apply_oracle_policy(&mut cached.index, config.oracles());
    let files = production_files_from_state_with_role(&state, &config);
    let bounded_full = inventory_seams_from_index_bounded(&files, &cached.index, Some(1))?;
    assert_eq!(bounded_full.seams.len(), 1);
    assert!(
        bounded_full.total_unique > 1,
        "fixture must exceed full cap"
    );
    let compact_cache = RepoSeamFactCache::at_compact_classified(&root);
    let key = state.cache_key();
    assert!(matches!(
        compact_cache.load_classified_seams_with_fallback(&key),
        CacheLoad::Miss
    ));
    assert!(complete_compact_with_budget(&files, &cached.index, 1, usize::MAX)?.is_none());
    assert!(complete_compact_with_budget(&files, &cached.index, 100, 1)?.is_none());
    let cancelled = cancellation::AnalysisCancellationToken::new();
    cancelled.cancel(cancellation::AnalysisAbortKind::Cancelled);
    let interrupted = cancellation::with_token(&cancelled, || {
        complete_compact_with_budget(&files, &cached.index, 100, 1024 * 1024)
    });
    assert!(
        interrupted.is_err(),
        "cancellation must not return a candidate"
    );
    assert!(matches!(
        compact_cache.load_classified_seams_with_fallback(&key),
        CacheLoad::Miss
    ));

    let prefilling = complete_compact_with_budget(&files, &cached.index, 100, 1024 * 1024)?
        .ok_or("small fixture unexpectedly exceeded budget")?;
    assert_eq!(prefilling.len(), bounded_full.total_unique);
    let (baseline, fallback) =
        inventory_compact_classified_seams_from_state_with_config(&state, &config)?;
    assert!(!baseline.is_empty());
    let payload_bytes = baseline.iter().try_fold(0usize, |total, record| {
        let bytes = serde_json::to_vec(record).map_err(|error| error.to_string())?;
        total
            .checked_add(bytes.len())
            .ok_or("payload size overflow".to_string())
    })?;
    assert!(complete_compact_with_budget(&files, &cached.index, 100, payload_bytes - 1)?.is_none());
    assert!(complete_compact_with_budget(&files, &cached.index, 100, payload_bytes)?.is_some());
    assert_eq!(
        serde_json::to_value(&prefilling).map_err(|e| e.to_string())?,
        serde_json::to_value(&baseline).map_err(|e| e.to_string())?
    );
    let baseline_summary = render_repo_exposure_summary_json(&baseline, &root, None, "draft");
    assert_eq!(
        render_repo_exposure_summary_json(&prefilling, &root, None, "draft"),
        baseline_summary
    );
    let stored = compact_cache.store_compact_classified_seams_with_limit_and_fallback(
        &key,
        &prefilling,
        &fallback,
        1,
    )?;
    assert_eq!(
        stored.label,
        format!(
            "sharded_ok_seams_{}_shards_{}_limit_1",
            prefilling.len(),
            prefilling.len()
        )
    );
    assert!(stored.advisory.is_none());
    match compact_cache.load_classified_seams_with_fallback(&key) {
        CacheLoad::Hit((loaded, limit, loaded_fallback)) => {
            assert!(limit.is_none(), "compact entry must be complete");
            assert_eq!(loaded_fallback, fallback);
            assert_eq!(
                serde_json::to_value(loaded).map_err(|e| e.to_string())?,
                serde_json::to_value(&prefilling).map_err(|e| e.to_string())?
            );
        }
        other => {
            return Err(format!(
                "prefill must publish an actual compact hit: {other:?}"
            ));
        }
    }
    let warm = inventory_compact_classified_seams_at_with_config(&root, &config)?;
    assert_eq!(warm.len(), bounded_full.total_unique);
    assert_eq!(
        render_repo_exposure_summary_json(&warm, &root, None, "draft"),
        baseline_summary
    );
    // A valid alternate empty payload distinguishes the actual reader's hit
    // from a cold recomputation, which must return the nonempty fixture.
    let alternate = compact_cache.store_compact_classified_seams_with_limit_and_fallback(
        &key,
        &[],
        &fallback,
        1,
    )?;
    assert_eq!(alternate.label, "ok");
    assert!(inventory_compact_classified_seams_at_with_config(&root, &config)?.is_empty());
    eprintln!(
        "12506 probe: full prefix=1, complete compact={}, same-index/cold/warm summary parity; shape and payload exhaustion publish nothing",
        warm.len()
    );
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}
