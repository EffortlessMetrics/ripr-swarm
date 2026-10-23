//! RIPR-SPEC-0005: numeric generations are wire namespaces, not sufficient
//! semantic identities. Model two repair builds and a feature-only sibling at
//! the SAME generation. These are synthesized predecessors, not historical
//! analyzer captures. Existing generation-transition tests cover old schemas.

use super::*;
use crate::analysis::seams::SeamGripClass;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter as _};

#[test]
fn declared_language_features_require_cache_identity_coverage() -> Result<(), String> {
    let manifest: toml::Value =
        toml::from_str(include_str!("../../../Cargo.toml")).map_err(|error| error.to_string())?;
    let features = manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .ok_or("missing crate feature declarations")?;
    let languages: BTreeSet<&str> = features
        .keys()
        .map(String::as_str)
        .filter(|name| name.starts_with("lang-"))
        .collect();
    assert_eq!(
        languages,
        BTreeSet::from(["lang-rust", "lang-typescript", "lang-python", "lang-perl"]),
        "extend build_identity::cache_identity and its feature-set discriminator when adding a language"
    );
    Ok(())
}

fn other_semantic_builds() -> [String; 3] {
    let current = crate::build_identity::cache_identity();
    let sibling = if cfg!(feature = "lang-perl") {
        current.replace("perl=1", "perl=0")
    } else {
        current.replace("perl=0", "perl=1")
    };
    assert_ne!(
        sibling, current,
        "live identity must bind compiled features"
    );
    [
        format!(
            "{}+1111111111111111111111111111111111111111",
            env!("CARGO_PKG_VERSION")
        ),
        format!(
            "{}+2222222222222222222222222222222222222222",
            env!("CARGO_PKG_VERSION")
        ),
        sibling,
    ]
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Result<Self, String> {
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
        let path = super::tests::isolated_dir(label);
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        super::ignore_remove_dir_all(&self.0);
    }
}

#[test]
fn same_generation_file_facts_refuse_other_builds_even_at_current_path() -> Result<(), String> {
    let scratch = Scratch::new("same-generation-file-facts")?;
    let cache = RepoFileFactCache::at_base(&scratch.0);
    let file = Path::new("src/lib.rs");
    let source = "pub fn owner(x: i32) -> i32 { x + 1 }\n";
    let current = RaRustSyntaxAdapter.summarize_file(file, source)?;
    assert_eq!(current.functions.len(), 1, "nonempty parser stimulus");
    let mut previous = current.clone();
    previous.functions.clear();
    let key = RepoFileFactCacheKey::new(file, source.as_bytes());
    let mut previous_keys = Vec::new();
    for analyzer_version in other_semantic_builds() {
        let previous_key = key.with_test_analyzer_identity(analyzer_version);
        assert_eq!(previous_key.schema_version, key.schema_version);
        assert_ne!(previous_key.filename(), key.filename());
        cache.store_file_facts(&previous_key, &previous)?;
        assert!(
            matches!(cache.load_file_facts(&previous_key), CacheLoad::Hit(ref facts) if facts == &previous)
        );
        // Challenge envelope admission independently of key-derived filenames.
        std::fs::copy(cache.entry_path(&previous_key), cache.entry_path(&key))
            .map_err(|error| error.to_string())?;
        assert!(matches!(cache.load_file_facts(&key), CacheLoad::Miss));
        assert!(!cache.known_file_paths().contains(file));
        previous_keys.push(previous_key);
    }
    cache.store_file_facts(&key, &current)?;
    assert!(matches!(cache.load_file_facts(&key), CacheLoad::Hit(ref facts) if facts == &current));
    assert!(cache.known_file_paths().contains(file));
    for previous_key in previous_keys {
        assert!(
            matches!(cache.load_file_facts(&previous_key), CacheLoad::Hit(ref facts) if facts == &previous)
        );
    }
    Ok(())
}

#[test]
fn same_generation_classified_builds_coexist_without_replaying_each_other() -> Result<(), String> {
    for compact in [false, true] {
        for sharded in [false, true] {
            let scratch = Scratch::new("same-generation-classified")?;
            let (family, schema) = if compact {
                (
                    ClassifiedCacheFamily::CompactClassifiedSeams,
                    COMPACT_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION,
                )
            } else {
                (ClassifiedCacheFamily::SeamFacts, CACHE_SCHEMA_VERSION)
            };
            let cache = RepoSeamFactCache::at_base(&scratch.0, family, schema);
            // The compact directory has its own wire generation, but the
            // production workspace key still carries the full outer schema.
            let key = super::tests::empty_state().cache_key();
            let mut corrected = super::tests::sample_classified();
            corrected.class = SeamGripClass::ReachableUnrevealed;
            let count = if sharded { 2 } else { 1 };
            let corrected = vec![corrected; count];
            let mut previous_rows = Vec::new();
            for (analyzer_version, class) in other_semantic_builds().into_iter().zip([
                SeamGripClass::StronglyGripped,
                SeamGripClass::WeaklyGripped,
                SeamGripClass::Ungripped,
            ]) {
                let previous_key = RepoSeamCacheKey {
                    analyzer_version,
                    ..key.clone()
                };
                assert_eq!(previous_key.schema_version, key.schema_version);
                assert_ne!(previous_key.filename(), key.filename());
                let mut previous = super::tests::sample_classified();
                previous.class = class;
                let previous = vec![previous; count];
                store(&cache, &previous_key, &previous, compact, sharded)?;
                assert_warm(&cache, &previous_key, &previous)?;
                // Seed the complete old entry at the current key's location.
                // Valid old bytes must miss because of identity, not because
                // a file is absent or its payload digest is invalid.
                copy_entry(&cache, &previous_key, &key, sharded)?;
                assert!(matches!(
                    cache.load_classified_seams_with_fallback(&key),
                    CacheLoad::Miss
                ));
                previous_rows.push((previous_key, previous));
            }
            store(&cache, &key, &corrected, compact, sharded)?;
            assert_warm(&cache, &key, &corrected)?;
            for (previous_key, previous) in &previous_rows {
                assert_warm(&cache, previous_key, previous)?;
            }
            if sharded {
                // Hold the current manifest fixed and transplant a prior
                // build's valid shard: the shard has its own admission guard.
                let current_manifest = read_manifest(&cache, &key)?;
                let previous_key = &previous_rows.first().ok_or("missing seeded predecessor")?.0;
                let previous_manifest = read_manifest(&cache, previous_key)?;
                let current_shard = current_manifest
                    .shards
                    .first()
                    .ok_or("missing current shard")?;
                let previous_shard = previous_manifest
                    .shards
                    .first()
                    .ok_or("missing previous shard")?;
                std::fs::copy(
                    cache
                        .sharded_entry_dir(previous_key)
                        .join(&previous_shard.file),
                    cache.sharded_entry_dir(&key).join(&current_shard.file),
                )
                .map_err(|error| error.to_string())?;
                assert!(
                    matches!(cache.load_classified_seams_with_fallback(&key), CacheLoad::CorruptIgnored { reason } if reason.contains("sharded cache key mismatch"))
                );
                store(&cache, &key, &corrected, compact, sharded)?;
                assert_warm(&cache, &key, &corrected)?;
            }
        }
    }
    Ok(())
}

fn store(
    cache: &RepoSeamFactCache,
    key: &RepoSeamCacheKey,
    rows: &[ClassifiedSeam],
    compact: bool,
    sharded: bool,
) -> Result<(), String> {
    let limit = if sharded { 1 } else { 10 };
    if compact {
        cache.store_compact_classified_seams_with_limit(key, rows, limit)?;
    } else {
        cache.store_classified_seams_with_limit(key, rows, None, limit)?;
    }
    assert_eq!(cache.sharded_manifest_path(key).exists(), sharded);
    assert_eq!(cache.entry_path(key).exists(), !sharded);
    Ok(())
}

fn assert_warm(
    cache: &RepoSeamFactCache,
    key: &RepoSeamCacheKey,
    expected: &[ClassifiedSeam],
) -> Result<(), String> {
    let CacheLoad::Hit((actual, _, _)) = cache.load_classified_seams_with_fallback(key) else {
        return Err("expected nonempty warm hit".to_owned());
    };
    assert!(!actual.is_empty());
    assert_eq!(
        serde_json::to_value(actual).map_err(|error| error.to_string())?,
        serde_json::to_value(expected).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn read_manifest(
    cache: &RepoSeamFactCache,
    key: &RepoSeamCacheKey,
) -> Result<ShardedCacheManifest, String> {
    codec::decode_sharded_manifest(
        &std::fs::read(cache.sharded_manifest_path(key)).map_err(|error| error.to_string())?,
    )
}

fn copy_entry(
    cache: &RepoSeamFactCache,
    from: &RepoSeamCacheKey,
    to: &RepoSeamCacheKey,
    sharded: bool,
) -> Result<(), String> {
    if sharded {
        let manifest = read_manifest(cache, from)?;
        manifest.validate_integrity()?;
        assert!(!manifest.shards.is_empty());
        for shard in &manifest.shards {
            let source = cache.sharded_entry_dir(from).join(&shard.file);
            let dest = cache.sharded_entry_dir(to).join(&shard.file);
            std::fs::create_dir_all(dest.parent().ok_or("shard has no parent")?)
                .map_err(|error| error.to_string())?;
            std::fs::copy(source, dest).map_err(|error| error.to_string())?;
        }
        std::fs::copy(
            cache.sharded_manifest_path(from),
            cache.sharded_manifest_path(to),
        )
        .map_err(|error| error.to_string())?;
    } else {
        std::fs::copy(cache.entry_path(from), cache.entry_path(to))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
