use super::super::tests::{make_tempdir, write_file};
use super::*;

fn cache_bytes(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    fn walk(base: &Path, dir: &Path, found: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|err| err.to_string())? {
            let entry = entry.map_err(|err| err.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|err| err.to_string())?;
            if kind.is_dir() {
                walk(base, &path, found)?;
            } else if kind.is_file() {
                found.insert(
                    path.strip_prefix(base)
                        .map_err(|err| err.to_string())?
                        .to_path_buf(),
                    std::fs::read(path).map_err(|err| err.to_string())?,
                );
            } else {
                return Err("unexpected cache fixture entry".to_string());
            }
        }
        Ok(())
    }
    let mut found = BTreeMap::new();
    walk(root, root, &mut found)?;
    Ok(found)
}

#[test]
fn compact_windows_zero_total_with_nonempty_index_preserves_prior_generation() -> Result<(), String>
{
    let root = super::tests::fixture("compact-windows-false-zero")?;
    let config = RiprConfig::default();
    let state = collect_workspace_state(&root, &config)?;
    let mut cached = state.build_index(&config)?;
    rust_index::apply_oracle_policy(&mut cached.index, config.oracles());
    let files = production_files_from_state_with_role(&state, &config);
    assert!(!inventory_seams_from_index(&files, &cached.index).is_empty());
    let fallback = rust_index::lexical_fallback_files(&cached.index);
    let pass = test_grip_evidence::EvidencePass::new(&cached.index);
    let key = state.cache_key();
    let cache = RepoSeamFactCache::at_compact_classified(&root);
    let (baseline, _) = inventory_compact_classified_seams_from_state_with_config(&state, &config)?;
    assert!(
        !baseline.is_empty(),
        "the prior generation must be observably nonempty"
    );
    cache.store_compact_classified_seams_with_limit(&key, &baseline, 1)?;
    let cache_root = crate::analysis::seam_cache::cache_base_dir(&root);
    let before = cache_bytes(&cache_root)?;
    let mut session = Session {
        state: &state,
        key: &key,
        files: &files,
        index: &cached.index,
        pass: &pass,
        total: 0,
        fallback: &fallback,
        limits: LIMITS,
        retention: Retention::default(),
        inputs_changed: std::cell::Cell::new(false),
    };
    let outcome = session.run();
    assert_eq!(
        cache_bytes(&cache_root)?,
        before,
        "a false zero must not replace or otherwise mutate the prior cache generation"
    );
    assert!(
        outcome.is_err(),
        "zero declared seams must refuse a nonempty index: {outcome:?}"
    );
    match cache.load_classified_seams_with_fallback(&key) {
        CacheLoad::Hit((loaded, None, loaded_fallback)) => {
            assert_eq!(
                serde_json::to_value(loaded).map_err(|err| err.to_string())?,
                serde_json::to_value(baseline).map_err(|err| err.to_string())?
            );
            assert_eq!(loaded_fallback, fallback);
        }
        other => return Err(format!("false zero replaced prior generation: {other:?}")),
    }
    std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn compact_windows_genuinely_empty_index_publishes_a_complete_empty_generation()
-> Result<(), String> {
    let root = make_tempdir("compact-windows-real-zero")?;
    write_file(
        &root.join("Cargo.toml"),
        "[package]\nname='empty_window_fixture'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    write_file(
        &root.join("src/lib.rs"),
        "// genuinely empty production inventory\n",
    )?;
    let config = RiprConfig::default();
    let state = collect_workspace_state(&root, &config)?;
    let cached = state.build_index(&config)?;
    let files = production_files_from_state_with_role(&state, &config);
    assert!(inventory_seams_from_index(&files, &cached.index).is_empty());
    let fallback = rust_index::lexical_fallback_files(&cached.index);
    let pass = test_grip_evidence::EvidencePass::new(&cached.index);
    let key = state.cache_key();
    let mut session = Session {
        state: &state,
        key: &key,
        files: &files,
        index: &cached.index,
        pass: &pass,
        total: 0,
        fallback: &fallback,
        limits: LIMITS,
        retention: Retention::default(),
        inputs_changed: std::cell::Cell::new(false),
    };
    session.run()?;
    match RepoSeamFactCache::at_compact_classified(&root).load_classified_seams_with_fallback(&key)
    {
        CacheLoad::Hit((loaded, None, loaded_fallback)) => {
            assert!(loaded.is_empty());
            assert_eq!(loaded_fallback, fallback);
        }
        other => {
            return Err(format!(
                "genuine empty generation did not publish: {other:?}"
            ));
        }
    }
    std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    Ok(())
}
