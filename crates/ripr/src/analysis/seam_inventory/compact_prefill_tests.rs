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
    const CHILD: &str = "RIPR_COMPACT_PREFILL_CHILD_ACK";
    let Some(acknowledgement) = std::env::var_os(CHILD) else {
        let child_root = make_tempdir("compact-windows-child")?;
        let acknowledgement = child_root.join("complete.txt");
        let log_path = child_root.join("child.log");
        let log = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&log_path)
            .map_err(|error| error.to_string())?;
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let module = module_path!();
        let module = module.split_once("::").map_or(module, |(_, rest)| rest);
        let mut command = std::process::Command::new(executable);
        command
            .args([
                "--exact",
                &format!(
                    "{module}::compact_windows_full_cold_run_populates_complete_summary_cache"
                ),
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &acknowledgement)
            .env(REPO_EXPOSURE_SEAM_LIMIT_ENV, "1")
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone().map_err(|error| error.to_string())?)
            .stderr(log);
        let mut child = crate::process_owner::OwnedProcess::spawn_isolated(command)
            .map_err(|error| error.to_string())?;
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let cleanup = child.terminate_tree();
                    return Err(format!(
                        "compact child poll failed: {error}; cleanup {cleanup:?}; log {}",
                        log_path.display()
                    ));
                }
            }
            if started.elapsed() >= Duration::from_mins(2)
                || std::fs::metadata(&log_path)
                    .map_err(|error| error.to_string())?
                    .len()
                    > 262_144
            {
                child.terminate_tree()?;
                return Err(format!(
                    "compact child exceeded its time/output limit; log {}",
                    log_path.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let output = crate::bounded_input::read_to_string_with_limit(&log_path, 262_144)
            .map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("compact child failed: {status}\n{output}"));
        }
        assert_eq!(
            crate::bounded_input::read_to_string_with_limit(&acknowledgement, 1024)
                .map_err(|error| error.to_string())?,
            "complete compact prefill assertions executed"
        );
        eprintln!("{output}");
        std::fs::remove_dir_all(child_root).map_err(|error| error.to_string())?;
        return Ok(());
    };
    let acknowledgement = PathBuf::from(acknowledgement);
    let root = fixture("compact-windows-full")?;
    let config = RiprConfig::default();
    let state = collect_workspace_state(&root, &config)?;
    let (baseline, fallback) =
        inventory_compact_classified_seams_from_state_with_config(&state, &config)?;
    assert!(
        baseline.len() > LIMITS.window * 2,
        "fixture must exercise several windows"
    );
    assert_eq!(
        repo_exposure_seam_limit()?,
        Some((1, SeamLimitSource::Configured))
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
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    std::fs::write(
        acknowledgement,
        "complete compact prefill assertions executed",
    )
    .map_err(|error| error.to_string())?;
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
        inputs_changed: std::cell::Cell::new(false),
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
        inputs_changed: std::cell::Cell::new(false),
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
    assert!(session.inputs_changed.get());
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

#[test]
fn compact_windows_verification_bounds_cumulative_moved_source() -> Result<(), String> {
    let root = fixture("compact-windows-reread-budget")?;
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
    let source_bytes = cached
        .index
        .files()
        .values()
        .map(|facts| facts.source.len().saturating_add(3))
        .sum();
    let mut session = Session {
        state: &state,
        key: &key,
        files: &files,
        index: &cached.index,
        pass: &pass,
        total,
        fallback: &fallback,
        limits: Limits {
            source_bytes,
            ..LIMITS
        },
        retention: Retention::default(),
        inputs_changed: std::cell::Cell::new(false),
    };
    assert!(session.admitted()?);
    write_file(
        &root.join("src/empty.rs"),
        &format!("// No seams in this file.\n{}", " ".repeat(64)),
    )?;
    let error = session
        .run()
        .err()
        .ok_or("aggregate source growth must refuse")?;
    assert!(
        error.contains("grew beyond its admitted byte limit"),
        "wrong refusal: {error}"
    );
    assert!(session.inputs_changed.get());
    session.limits = LIMITS;
    write_file(
        &root.join("src/empty.rs"),
        &" ".repeat(LIMITS.file_source_bytes + 1),
    )?;
    let error = session
        .run()
        .err()
        .ok_or("one oversized moved file must refuse")?;
    assert!(
        error.contains("grew beyond its admitted byte limit"),
        "wrong refusal: {error}"
    );
    assert!(session.inputs_changed.get());
    match cache.load_classified_seams_with_fallback(&key) {
        CacheLoad::Hit((old, _, _)) => assert!(old.is_empty()),
        other => return Err(format!("bounded reread refusal replaced cache: {other:?}")),
    }
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn compact_windows_unchanged_bom_at_byte_boundary_declines_or_fits_without_drift()
-> Result<(), String> {
    let root = make_tempdir("compact-windows-bom-budget")?;
    write_file(
        &root.join("Cargo.toml"),
        "[package]\nname='bom_window'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    write_file(
        &root.join("src/lib.rs"),
        "\u{feff}pub fn answer()->i32 { 42 }\n",
    )?;
    let config = RiprConfig::default();
    let state = collect_workspace_state(&root, &config)?;
    let cached = state.build_index(&config)?;
    let files = production_files_from_state_with_role(&state, &config);
    let total = inventory_seams_from_index(&files, &cached.index).len();
    assert!(total > 0);
    let decoded = cached
        .index
        .files()
        .get(Path::new("src/lib.rs"))
        .ok_or("BOM fixture missing from index")?
        .source
        .len();
    let raw = std::fs::metadata(root.join("src/lib.rs"))
        .map_err(|e| e.to_string())?
        .len() as usize;
    assert_eq!(
        raw,
        decoded + 3,
        "fixture must exercise actual BOM stripping"
    );
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
        inputs_changed: std::cell::Cell::new(false),
    };
    for limits in [
        Limits {
            file_source_bytes: decoded,
            source_bytes: raw,
            ..LIMITS
        },
        Limits {
            file_source_bytes: raw,
            source_bytes: decoded,
            ..LIMITS
        },
    ] {
        session.limits = limits;
        assert_eq!(session.run()?.label, "skipped_compact_prefill_input_limit");
        assert!(
            !session.inputs_changed.get(),
            "unchanged BOM bytes are not input drift"
        );
    }
    session.limits = Limits {
        file_source_bytes: raw,
        source_bytes: raw,
        ..LIMITS
    };
    assert!(session.run()?.label.starts_with("sharded_ok_"));
    assert!(!session.inputs_changed.get());
    match cache.load_classified_seams_with_fallback(&key) {
        CacheLoad::Hit((complete, _, _)) => assert_eq!(complete.len(), total),
        other => {
            return Err(format!(
                "raw-byte-admitted BOM prefill was not published: {other:?}"
            ));
        }
    }
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}
