#[test]
fn compact_windows_round_trip_payload_fallback_and_exact_requests() -> Result<(), String> {
    let dir = isolated_dir("compact-windows-parity");
    let cache = RepoSeamFactCache::at_dir(dir.clone());
    let key = empty_key();
    let seams: Vec<_> = (0..5)
        .map(|i| classified_with_pad(&format!("window-{i}")))
        .collect();
    let fallback = vec![PathBuf::from("src/fallback.rs")];
    let mut requests = Vec::new();
    let mut source = seams.clone().into_iter();
    let status =
        cache.store_compact_classified_windows(&key, seams.len(), &fallback, 2, |count| {
            requests.push(count);
            Ok(Some(source.by_ref().take(count).collect()))
        })?;
    assert!(status.label.starts_with("sharded_ok"));
    assert_eq!(requests, [2, 2, 1]);
    let CacheLoad::Hit((actual, limit, actual_fallback)) =
        cache.load_classified_seams_with_fallback(&key)
    else {
        return Err("windowed compact cache should be warm".into());
    };
    assert_eq!(
        serde_json::to_value(actual).map_err(|e| e.to_string())?,
        serde_json::to_value(seams).map_err(|e| e.to_string())?
    );
    assert!(limit.is_none());
    assert_eq!(actual_fallback, fallback);
    ignore_remove_dir_all(&dir);
    Ok(())
}

#[test]
fn compact_windows_failed_or_declined_replacements_preserve_prior_entries() -> Result<(), String> {
    for prior_limit in [1, 8] {
        let dir = isolated_dir("compact-windows-preserve");
        let cache = RepoSeamFactCache::at_dir(dir.clone());
        let key = empty_key();
        let prior = vec![
            classified_with_pad("prior-a"),
            classified_with_pad("prior-b"),
        ];
        cache.store_classified_seams_with_record_and_byte_limits(
            &key,
            &prior,
            None,
            prior_limit,
            1_000_000,
        )?;
        let generations = listed_generation_dirs(&cache, &key)?;
        for failure in 0..4 {
            let mut calls = 0;
            let result = publish_compact_windows(
                &cache,
                &key,
                3,
                &[],
                1,
                if failure == 3 { 10_000 } else { 1_000_000 },
                |_| {
                    calls += 1;
                    if calls == 2 {
                        return match failure {
                            0 => Err("producer failure".into()),
                            1 => Ok(None),
                            2 => Ok(Some(Vec::new())),
                            _ => Ok(Some(vec![classified_with_pad(&"x".repeat(20_000))])),
                        };
                    }
                    Ok(Some(vec![classified_with_pad("new")]))
                },
            );
            match failure {
                0 | 2 => {
                    let error = result.err().ok_or("invalid compact producer succeeded")?;
                    assert!(
                        !error.is_empty(),
                        "producer refusal should explain its cause"
                    );
                }
                1 | 3 => assert!(result?.label.starts_with("skipped_")),
                _ => return Err("unknown negative compact-window case".into()),
            }
            round_trip(&cache, &key, &prior)?;
            assert_eq!(listed_generation_dirs(&cache, &key)?, generations);
        }
        ignore_remove_dir_all(&dir);
    }
    Ok(())
}

#[test]
fn compact_windows_native_failure_hooks_preserve_prior_entries() -> Result<(), String> {
    for prior_limit in [1, 8] {
        let dir = isolated_dir("compact-windows-hooks");
        let cache = RepoSeamFactCache::at_dir(dir.clone());
        let key = empty_key();
        let prior = vec![
            classified_with_pad("prior-a"),
            classified_with_pad("prior-b"),
        ];
        cache.store_classified_seams_with_record_and_byte_limits(
            &key,
            &prior,
            None,
            prior_limit,
            1_000_000,
        )?;
        let generations = listed_generation_dirs(&cache, &key)?;
        for hook in 0..3 {
            match hook {
                0 => fail_after_writing_shards(1),
                1 => fail_after_writing_shards(3),
                _ => fail_after_parking_single_entry(),
            }
            let error = publish_compact_windows(&cache, &key, 3, &[], 1, 1_000_000, |_| {
                Ok(Some(vec![classified_with_pad("next")]))
            })
            .err()
            .ok_or("injected compact publication failure was ignored")?;
            assert!(error.starts_with("injected"), "wrong refusal: {error}");
            round_trip(&cache, &key, &prior)?;
            assert_eq!(listed_generation_dirs(&cache, &key)?, generations);
        }
        ignore_remove_dir_all(&dir);
    }
    Ok(())
}

#[test]
fn compact_windows_cancellation_in_last_producer_preserves_prior() -> Result<(), String> {
    use crate::analysis::cancellation::{AnalysisAbortKind, AnalysisCancellationToken, with_token};
    let dir = isolated_dir("compact-windows-cancel");
    let cache = RepoSeamFactCache::at_dir(dir.clone());
    let key = empty_key();
    let prior = vec![classified_with_pad("prior")];
    cache.store_classified_seams_with_record_and_byte_limits(&key, &prior, None, 8, 1_000_000)?;
    let generations = listed_generation_dirs(&cache, &key)?;
    let token = AnalysisCancellationToken::new();
    let mut calls = 0;
    let result = with_token(&token, || {
        publish_compact_windows(&cache, &key, 2, &[], 1, 1_000_000, |_| {
            calls += 1;
            if calls == 2 {
                token.cancel(AnalysisAbortKind::Cancelled);
            }
            Ok(Some(vec![classified_with_pad("next")]))
        })
    });
    assert!(
        result
            .err()
            .is_some_and(|err| err.contains("analysis cancelled"))
    );
    assert_eq!(calls, 2);
    round_trip(&cache, &key, &prior)?;
    assert_eq!(listed_generation_dirs(&cache, &key)?, generations);
    ignore_remove_dir_all(&dir);
    Ok(())
}

#[test]
fn compact_windows_empty_result_uses_no_producer_or_shards() -> Result<(), String> {
    let dir = isolated_dir("compact-windows-empty");
    let cache = RepoSeamFactCache::at_dir(dir.clone());
    let key = empty_key();
    let fallback = vec![PathBuf::from("src/empty-fallback.rs")];
    let calls = std::cell::Cell::new(0);
    let status = cache.store_compact_classified_windows(&key, 0, &fallback, 2, |_| {
        calls.set(calls.get() + 1);
        Err("empty producer called".into())
    })?;
    assert_eq!(calls.get(), 0);
    assert_eq!(status.label, "ok");
    assert!(listed_generation_dirs(&cache, &key)?.is_empty());
    assert!(
        matches!(cache.load_classified_seams_with_fallback(&key), CacheLoad::Hit((seams, None, paths)) if seams.is_empty() && paths == fallback)
    );
    let error = publish_compact_windows(&cache, &key, 0, &[], 0, 1_000_000, |_| {
        calls.set(calls.get() + 1);
        Err("invalid producer called".into())
    })
    .err()
    .ok_or("zero compact window size must refuse")?;
    assert!(
        error.contains("compact cache window limit must be positive"),
        "wrong refusal: {error}"
    );
    assert_eq!(calls.get(), 0);
    ignore_remove_dir_all(&dir);
    Ok(())
}
