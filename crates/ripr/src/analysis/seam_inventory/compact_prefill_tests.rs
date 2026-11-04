use super::super::tests::{make_tempdir, write_file};
use super::*;
use crate::output::repo_exposure::render_repo_exposure_summary_json;

fn fixture(label: &str) -> Result<PathBuf, String> {
    let root = make_tempdir(label)?;
    write_file(
        &root.join("Cargo.toml"),
        "[package]\nname='window_fixture'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    let source = (0..70)
        .map(|i| format!("pub fn f{i}(x:i32)->bool {{ x >= {i} }}\n"))
        .collect::<String>();
    write_file(&root.join("src/lib.rs"), &source)?;
    write_file(&root.join("src/empty.rs"), "// No seams in this file.\n")?;
    write_file(
        &root.join("tests/calls.rs"),
        "#[test] fn calls_f0() { assert!(window_fixture::f0(1)); }\n",
    )?;
    Ok(root)
}

#[test]
fn compact_windows_full_cold_run_populates_complete_summary_cache() -> Result<(), String> {
    let root = fixture("compact-windows-full")?;
    let config = RiprConfig::default();
    let state = collect_workspace_state(&root, &config)?;
    let (baseline, fallback) =
        inventory_compact_classified_seams_from_state_with_config(&state, &config)?;
    assert!(
        baseline.len() > LIMITS.window * 2,
        "fixture must exercise several windows"
    );
    let cap = PilotInventoryDefault::widen_to(1);
    assert_eq!(
        repo_exposure_seam_limit()?,
        Some((1, SeamLimitSource::Default))
    );
    let full = inventory_classified_seams_report_inner(&root, &config, None)?;
    assert_eq!(full.classified.len(), 1);
    assert_eq!(
        full.limit_info.as_ref().map(|info| info.total),
        Some(baseline.len())
    );
    let key = state.cache_key();
    let cache = RepoSeamFactCache::at_compact_classified(&root);
    match cache.load_classified_seams_with_fallback(&key) {
        CacheLoad::Hit((complete, limit, loaded_fallback)) => {
            assert_eq!(complete.len(), baseline.len());
            assert!(limit.is_none());
            assert_eq!(loaded_fallback, fallback);
            assert_eq!(
                serde_json::to_value(&complete).map_err(|e| e.to_string())?,
                serde_json::to_value(&baseline).map_err(|e| e.to_string())?
            );
        }
        other => {
            return Err(format!(
                "full cold run did not publish complete compact hit: {other:?}"
            ));
        }
    }
    let warm = inventory_compact_classified_seams_at_with_config(&root, &config)?;
    assert_eq!(
        render_repo_exposure_summary_json(&warm, &root, None, "draft"),
        render_repo_exposure_summary_json(&baseline, &root, None, "draft")
    );
    cache.store_compact_classified_seams_with_limit(&key, &[], 1)?;
    assert!(
        inventory_compact_classified_seams_at_with_config(&root, &config)?.is_empty(),
        "valid alternate entry must distinguish reuse from recomputation"
    );
    drop(cap);
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn compact_windows_admission_and_payload_limits_preserve_prior_generation() -> Result<(), String> {
    let root = fixture("compact-windows-limits")?;
    let config = RiprConfig::default();
    let state = collect_workspace_state(&root, &config)?;
    let mut cached = state.build_index(&config)?;
    rust_index::apply_oracle_policy(&mut cached.index, config.oracles());
    let files = production_files_from_state_with_role(&state, &config);
    let total = inventory_seams_from_index(&files, &cached.index).len();
    let fallback = rust_index::lexical_fallback_files(&cached.index);
    let pass = test_grip_evidence::EvidencePass::new(&cached.index);
    let key = state.cache_key();
    let cache = RepoSeamFactCache::at_compact_classified(&root);
    cache.store_compact_classified_seams_with_limit(&key, &[], 1)?;
    let mut session = Session {
        state: &state,
        key: &key,
        files: &files,
        index: &cached.index,
        pass: &pass,
        total,
        fallback: &fallback,
        limits: LIMITS,
        retention: Retention::default(),
    };
    for limits in [
        Limits { files: 0, ..LIMITS },
        Limits {
            source_bytes: 0,
            ..LIMITS
        },
        Limits {
            file_source_bytes: 0,
            ..LIMITS
        },
        Limits {
            file_shapes: 0,
            ..LIMITS
        },
        Limits {
            shapes: 0,
            ..LIMITS
        },
        Limits { tests: 0, ..LIMITS },
        Limits {
            functions: 0,
            ..LIMITS
        },
        Limits {
            name_bytes: 0,
            ..LIMITS
        },
    ] {
        session.limits = limits;
        assert_eq!(session.run()?.label, "skipped_compact_prefill_input_limit");
        match cache.load_classified_seams_with_fallback(&key) {
            CacheLoad::Hit((old, _, _)) => assert!(old.is_empty()),
            other => return Err(format!("admission replaced valid generation: {other:?}")),
        }
    }
    session.limits = Limits {
        retained_bytes: 1,
        ..LIMITS
    };
    assert!(
        session.run().is_err(),
        "one-byte per-file payload cannot fit a real seam"
    );
    match cache.load_classified_seams_with_fallback(&key) {
        CacheLoad::Hit((old, _, _)) => assert!(old.is_empty()),
        other => {
            return Err(format!(
                "payload refusal replaced valid generation: {other:?}"
            ));
        }
    }
    session.limits = LIMITS;
    assert!(session.run()?.label.starts_with("sharded_ok_"));
    assert!(session.retention.file_bytes <= LIMITS.retained_bytes);
    assert!(session.retention.window_bytes <= LIMITS.retained_bytes);
    assert!(session.retention.window_records <= LIMITS.window);
    assert!(session.retention.file_records <= LIMITS.file_shapes);
    let loaded = match cache.load_classified_seams_with_fallback(&key) {
        CacheLoad::Hit((loaded, _, _)) => loaded,
        other => return Err(format!("admitted windows did not publish: {other:?}")),
    };
    assert_eq!(loaded.len(), total);
    eprintln!(
        "compact prefill retained: {} file records / {} encoded bytes; {} window records / {} encoded bytes; total {}",
        session.retention.file_records,
        session.retention.file_bytes,
        session.retention.window_records,
        session.retention.window_bytes,
        total
    );
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn compact_windows_moved_inputs_and_wrong_total_refuse_without_replacing_cache()
-> Result<(), String> {
    let root = fixture("compact-windows-custody")?;
    let config = RiprConfig::default();
    let state = collect_workspace_state(&root, &config)?;
    let cached = state.build_index(&config)?;
    let files = production_files_from_state_with_role(&state, &config);
    let total = inventory_seams_from_index(&files, &cached.index).len();
    let fallback = rust_index::lexical_fallback_files(&cached.index);
    let pass = test_grip_evidence::EvidencePass::new(&cached.index);
    let key = state.cache_key();
    let cache = RepoSeamFactCache::at_compact_classified(&root);
    cache.store_compact_classified_seams_with_limit(&key, &[], 1)?;
    let mut session = Session {
        state: &state,
        key: &key,
        files: &files,
        index: &cached.index,
        pass: &pass,
        total: total - 1,
        fallback: &fallback,
        limits: LIMITS,
        retention: Retention::default(),
    };
    assert!(
        session.run().is_err(),
        "trailing seams must not be published as complete"
    );
    session.total = total + 1;
    assert!(
        session.run().is_err(),
        "exhausted inventory must not be published as complete"
    );
    session.total = total;
    write_file(&root.join("src/empty.rs"), "pub fn added()->i32 { 9 }\n")?;
    assert!(
        session.run().is_err(),
        "moved source must not publish under the old key"
    );
    match cache.load_classified_seams_with_fallback(&key) {
        CacheLoad::Hit((old, _, _)) => assert!(old.is_empty()),
        other => {
            return Err(format!(
                "custody failure replaced prior generation: {other:?}"
            ));
        }
    }
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}
