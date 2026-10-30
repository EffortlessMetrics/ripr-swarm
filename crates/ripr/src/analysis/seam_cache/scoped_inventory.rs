//! Persisted diff-scoped classified inventory (`repo-scoped-classified-seams`, #9899).
//!
//! The MCP refresh classifies only the seams that can bind a changed finding.
//! That classification is a pure function of the workspace cache key and the
//! caller's predicate identity, and the process-wide reuse in
//! `seam_inventory::bound_inventory_reuse` already serves it for an identical
//! pair. This layer extends the same contract across processes: a new
//! `ripr mcp --stdio` server on an unchanged workspace reads the classified
//! result back instead of rebuilding the evidence context and re-evaluating
//! every bindable seam (about 35 s of a 60 s refresh on this repository).
//!
//! An entry is admitted only when every field of the full
//! [`RepoSeamCacheKey`] and the exact reuse-identity bytes equal the request's
//! and the semantic payload digest verifies. The file name is a hash of both,
//! but the name only locates the entry; the embedded key decides admission, so
//! a name collision is a miss, never a wrong hit. An unknown Cargo
//! configuration identity, an untrusted (Git-tracked) cache root, an unreadable
//! or undecodable entry, and a store failure are all misses that recompute.
//! Rendered output is never cached: the entry holds classified facts only.

use super::{
    CacheLayer, CacheLoad, CachedSeamLimitInfo, ChecksummedBody, ClassifiedSeam, RepoSeamCacheKey,
    cache_base_untrusted_reason, cache_dir_not_a_directory_reason, cache_layer_dir, corrupt_entry,
    fnv1a_64, related_test_table, semantic_body_digest,
};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// On-disk shape version. Bump when [`ScopedInventoryEnvelope`] or
/// [`PersistedScopedInventory`] changes shape; the key's own schema and
/// analyzer identity still gate classification semantics.
pub(crate) const SCOPED_INVENTORY_CACHE_SCHEMA_VERSION: &str = "0.1";

const SCOPED_INVENTORY_DIGEST_DOMAIN: &str = "ripr-cache:ScopedInventoryEnvelope:v1";

/// Entries kept per cache directory. Every distinct workspace state writes
/// one entry, so the newest few are kept and the rest are removed after a
/// successful store; `ripr cache clear` and `cache gc` still own the layer.
const RETAINED_ENTRIES: usize = 4;

/// Largest entry file this layer reads or writes, in bytes. A measured
/// 287-seam entry on this repository is about 40 MB compact; the ceiling is
/// several times that so a large refresh still persists, while a planted or
/// runaway file is neither read into memory nor written. Over the ceiling is
/// a miss on load and a skipped store.
pub(crate) const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

/// The repo-wide seam-limit fact as the scoped inventory carries it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum PersistedSeamLimit {
    /// The run was not asked for the fact (`repo_seam_limit == None`).
    NotComputed,
    /// The limit fact: `None` means the inventory is complete.
    Computed(Option<CachedSeamLimitInfo>),
}

/// The serializable part of a scoped classified inventory: everything except
/// the per-run cache statistics, the workspace key (carried by the envelope),
/// and the absent-changed-files fact (recomputed on every call).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct PersistedScopedInventory {
    #[serde(with = "related_test_table")]
    pub(crate) classified: Vec<ClassifiedSeam>,
    pub(crate) classified_seams_considered: usize,
    pub(crate) total_rust_files: usize,
    pub(crate) total_production_files: usize,
    pub(crate) scoped_production_files: Vec<PathBuf>,
    pub(crate) changed_production_files: Vec<PathBuf>,
    pub(crate) parser_declined_files: Vec<PathBuf>,
    pub(crate) immediate_caller_files: Vec<PathBuf>,
    pub(crate) unevaluated_seams: usize,
    pub(crate) repo_seam_limit: PersistedSeamLimit,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ScopedInventoryEnvelope {
    #[serde(default, skip_serializing)]
    payload_digest: Option<String>,
    scoped_inventory_cache_schema_version: String,
    key: RepoSeamCacheKey,
    /// Lowercase hex of the exact reuse-identity bytes.
    reuse_identity_hex: String,
    inventory: PersistedScopedInventory,
}

impl ScopedInventoryEnvelope {
    fn expected_digest(&self) -> Result<String, String> {
        semantic_body_digest(SCOPED_INVENTORY_DIGEST_DOMAIN, self)
    }

    fn validate_integrity(&self) -> Result<(), String> {
        let expected = self.expected_digest()?;
        if self.payload_digest.as_deref() == Some(expected.as_str()) {
            Ok(())
        } else {
            Err("missing or mismatching semantic payload digest".to_owned())
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Crate-private cache I/O surface for persisted scoped inventories. Holds
/// only the directory; cheap to construct per call.
pub(crate) struct RepoScopedInventoryCache {
    pub(super) dir: PathBuf,
    base: PathBuf,
    unusable_base_reason: OnceLock<Option<String>>,
}

impl RepoScopedInventoryCache {
    pub(crate) fn at(workspace_root: &Path) -> Self {
        let dir = cache_layer_dir(workspace_root, CacheLayer::ScopedClassifiedSeams)
            .join(SCOPED_INVENTORY_CACHE_SCHEMA_VERSION);
        // `{base}/{layer}/{version}`: the cache base is two components up.
        let base = dir
            .ancestors()
            .nth(2)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.clone());
        Self {
            dir,
            base,
            unusable_base_reason: OnceLock::new(),
        }
    }

    /// Construct at an already resolved cache `base` (doctor's probe list).
    pub(super) fn at_base(base: &Path) -> Self {
        Self {
            dir: base
                .join(CacheLayer::ScopedClassifiedSeams.name())
                .join(SCOPED_INVENTORY_CACHE_SCHEMA_VERSION),
            base: base.to_path_buf(),
            unusable_base_reason: OnceLock::new(),
        }
    }

    /// The entry directory this cache writes into, with whether storing
    /// there also creates subdirectories (it does not).
    pub(crate) fn entry_dir(&self) -> (PathBuf, bool) {
        (self.dir.clone(), false)
    }

    #[cfg(test)]
    pub(crate) fn at_dir(dir: PathBuf) -> Self {
        Self {
            base: dir.clone(),
            dir,
            unusable_base_reason: OnceLock::new(),
        }
    }

    fn unusable_base_reason(&self) -> Option<&str> {
        self.unusable_base_reason
            .get_or_init(|| {
                cache_dir_not_a_directory_reason(&self.base)
                    .or_else(|| cache_base_untrusted_reason(&self.base))
            })
            .as_deref()
    }

    fn entry_path(&self, key: &RepoSeamCacheKey, identity: &[u8]) -> PathBuf {
        let mut name = key.filename().into_bytes();
        name.push(0);
        name.extend_from_slice(identity);
        self.dir.join(format!("{:016x}.json", fnv1a_64(&name)))
    }

    /// Load the entry for exactly this key and identity. Every failure mode
    /// other than a fully verified, exactly matching entry is a miss.
    pub(crate) fn load(
        &self,
        key: &RepoSeamCacheKey,
        identity: &[u8],
    ) -> CacheLoad<PersistedScopedInventory> {
        self.load_within(key, identity, MAX_ENTRY_BYTES)
    }

    fn load_within(
        &self,
        key: &RepoSeamCacheKey,
        identity: &[u8],
        max_bytes: u64,
    ) -> CacheLoad<PersistedScopedInventory> {
        if !key.is_reusable() {
            return CacheLoad::Miss;
        }
        if let Some(reason) = self.unusable_base_reason() {
            return CacheLoad::CorruptIgnored {
                reason: reason.to_owned(),
            };
        }
        let path = self.entry_path(key, identity);
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.len() > max_bytes => {
                return corrupt_entry(
                    &path,
                    format!(
                        "entry is {} bytes, over the {max_bytes} byte ceiling",
                        metadata.len()
                    ),
                );
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return CacheLoad::Miss,
            Err(err) => return corrupt_entry(&path, format!("stat failed: {err}")),
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return CacheLoad::Miss,
            Err(err) => return corrupt_entry(&path, format!("read failed: {err}")),
        };
        let envelope: ScopedInventoryEnvelope = match serde_json::from_slice(&bytes) {
            Ok(envelope) => envelope,
            Err(err) => return corrupt_entry(&path, format!("decode failed: {err}")),
        };
        if envelope.scoped_inventory_cache_schema_version != SCOPED_INVENTORY_CACHE_SCHEMA_VERSION
            || envelope.key != *key
            || envelope.reuse_identity_hex != hex(identity)
        {
            return CacheLoad::Miss;
        }
        if let Err(reason) = envelope.validate_integrity() {
            return corrupt_entry(&path, reason);
        }
        CacheLoad::Hit(envelope.inventory)
    }

    /// Persist `inventory` for exactly this key and identity. Best effort:
    /// the caller already holds the computed result.
    pub(crate) fn store(
        &self,
        key: &RepoSeamCacheKey,
        identity: &[u8],
        inventory: PersistedScopedInventory,
    ) -> Result<(), String> {
        self.store_within(key, identity, inventory, MAX_ENTRY_BYTES)
    }

    fn store_within(
        &self,
        key: &RepoSeamCacheKey,
        identity: &[u8],
        inventory: PersistedScopedInventory,
        max_bytes: u64,
    ) -> Result<(), String> {
        key.require_reusable()?;
        if let Some(reason) = self.unusable_base_reason() {
            return Err(reason.to_owned());
        }
        std::fs::create_dir_all(&self.dir)
            .map_err(|err| format!("create scoped inventory cache dir failed: {err}"))?;
        let envelope = ScopedInventoryEnvelope {
            payload_digest: None,
            scoped_inventory_cache_schema_version: SCOPED_INVENTORY_CACHE_SCHEMA_VERSION.to_owned(),
            key: key.clone(),
            reuse_identity_hex: hex(identity),
            inventory,
        };
        let digest = envelope.expected_digest()?;
        // Compact JSON: the classified evidence is large and this layer is
        // read back only by ripr, so pretty-printing would only cost bytes.
        let bytes = serde_json::to_vec(&ChecksummedBody {
            body: &envelope,
            payload_digest: digest,
        })
        .map_err(|err| format!("encode scoped inventory failed: {err}"))?;
        if bytes.len() as u64 > max_bytes {
            return Err(format!(
                "scoped inventory entry is {} bytes, over the {max_bytes} byte ceiling",
                bytes.len()
            ));
        }
        let path = self.entry_path(key, identity);
        crate::atomic_file::write_cache(&path, &bytes, "scoped inventory cache")?;
        self.prune(&path);
        Ok(())
    }

    /// Keep the newest [`RETAINED_ENTRIES`] entries, never the one just written.
    fn prune(&self, just_written: &Path) {
        let Ok(read_dir) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut entries = read_dir
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata.is_file().then_some(())?;
                Some((metadata.modified().ok()?, entry.path()))
            })
            .collect::<Vec<_>>();
        // The entry just written is always kept; the newest `RETAINED - 1`
        // others join it. Ties on modification time break by file name, so
        // the kept set is deterministic and never miscounted.
        entries.retain(|(_, path)| path != just_written);
        if entries.len() < RETAINED_ENTRIES {
            return;
        }
        entries.sort();
        let excess = entries.len() - (RETAINED_ENTRIES - 1);
        for (_, path) in entries.into_iter().take(excess) {
            // Best effort: a stranded old entry costs only disk space.
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::CARGO_INPUT_IDENTITY_UNAVAILABLE;
    use super::*;

    fn key() -> RepoSeamCacheKey {
        RepoSeamCacheKey {
            schema_version: "s".to_owned(),
            analyzer_version: "a".to_owned(),
            workspace_root_hash: "r".to_owned(),
            files_content_hash: "f".to_owned(),
            cfg_features_hash: "c".to_owned(),
            config_hash: "g".to_owned(),
            test_intent_hash: "t".to_owned(),
            suppressions_hash: "p".to_owned(),
            workspace_manifests_hash: "m".to_owned(),
            lockfile_hash: "l".to_owned(),
            toolchain_hash: "z".to_owned(),
            seam_limit_key: "limit_1".to_owned(),
        }
    }

    fn inventory() -> PersistedScopedInventory {
        PersistedScopedInventory {
            classified: Vec::new(),
            classified_seams_considered: 3,
            total_rust_files: 9,
            total_production_files: 4,
            scoped_production_files: vec![PathBuf::from("src/a.rs")],
            changed_production_files: vec![PathBuf::from("src/a.rs")],
            parser_declined_files: Vec::new(),
            immediate_caller_files: Vec::new(),
            unevaluated_seams: 2,
            repo_seam_limit: PersistedSeamLimit::Computed(None),
        }
    }

    fn scratch(label: &str) -> Result<PathBuf, String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-scoped-cache-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).map_err(|error| format!("create scratch: {error}"))?;
        Ok(dir)
    }

    #[test]
    fn exact_key_and_identity_round_trip() -> Result<(), String> {
        let dir = scratch("round-trip")?;
        let cache = RepoScopedInventoryCache::at_dir(dir.clone());
        cache.store(&key(), b"identity", inventory())?;
        let loaded = cache.load(&key(), b"identity");
        let _ = std::fs::remove_dir_all(&dir);
        match loaded {
            CacheLoad::Hit(loaded) => {
                assert_eq!(loaded.classified_seams_considered, 3);
                assert_eq!(loaded.unevaluated_seams, 2);
                assert_eq!(
                    loaded.scoped_production_files,
                    vec![PathBuf::from("src/a.rs")]
                );
                Ok(())
            }
            other => Err(format!("expected a hit, got {other:?}")),
        }
    }

    /// Each key field and the identity independently gate admission, even
    /// when a valid entry is planted at the file name the request would read.
    #[test]
    fn any_differing_key_field_or_identity_misses_even_at_the_requested_name() -> Result<(), String>
    {
        let dir = scratch("field-miss")?;
        let cache = RepoScopedInventoryCache::at_dir(dir.clone());
        cache.store(&key(), b"identity", inventory())?;
        let stored = std::fs::read(cache.entry_path(&key(), b"identity"))
            .map_err(|error| format!("read stored: {error}"))?;
        let x = || "x".to_owned();
        let variants: Vec<(&str, RepoSeamCacheKey)> = vec![
            (
                "schema",
                RepoSeamCacheKey {
                    schema_version: x(),
                    ..key()
                },
            ),
            (
                "analyzer",
                RepoSeamCacheKey {
                    analyzer_version: x(),
                    ..key()
                },
            ),
            (
                "root",
                RepoSeamCacheKey {
                    workspace_root_hash: x(),
                    ..key()
                },
            ),
            (
                "files",
                RepoSeamCacheKey {
                    files_content_hash: x(),
                    ..key()
                },
            ),
            (
                "cfg",
                RepoSeamCacheKey {
                    cfg_features_hash: x(),
                    ..key()
                },
            ),
            (
                "config",
                RepoSeamCacheKey {
                    config_hash: x(),
                    ..key()
                },
            ),
            (
                "intent",
                RepoSeamCacheKey {
                    test_intent_hash: x(),
                    ..key()
                },
            ),
            (
                "suppressions",
                RepoSeamCacheKey {
                    suppressions_hash: x(),
                    ..key()
                },
            ),
            (
                "manifests",
                RepoSeamCacheKey {
                    workspace_manifests_hash: x(),
                    ..key()
                },
            ),
            (
                "lockfile",
                RepoSeamCacheKey {
                    lockfile_hash: x(),
                    ..key()
                },
            ),
            (
                "toolchain",
                RepoSeamCacheKey {
                    toolchain_hash: x(),
                    ..key()
                },
            ),
            (
                "limit",
                RepoSeamCacheKey {
                    seam_limit_key: "limit_2".to_owned(),
                    ..key()
                },
            ),
        ];
        let mut outcomes = Vec::new();
        for (name, other) in &variants {
            // Plant the valid entry at the name the other key would read.
            std::fs::write(cache.entry_path(other, b"identity"), &stored)
                .map_err(|error| format!("plant: {error}"))?;
            outcomes.push((
                *name,
                matches!(cache.load(other, b"identity"), CacheLoad::Miss),
            ));
        }
        std::fs::write(cache.entry_path(&key(), b"other-identity"), &stored)
            .map_err(|error| format!("plant: {error}"))?;
        outcomes.push((
            "identity",
            matches!(cache.load(&key(), b"other-identity"), CacheLoad::Miss),
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let admitted: Vec<&str> = outcomes
            .iter()
            .filter(|(_, missed)| !missed)
            .map(|(name, _)| *name)
            .collect();
        assert!(
            admitted.is_empty(),
            "admitted despite a differing {admitted:?}"
        );
        assert_eq!(outcomes.len(), variants.len() + 1, "every field was probed");
        Ok(())
    }

    #[test]
    fn unknown_cargo_configuration_is_never_stored_or_served() -> Result<(), String> {
        let dir = scratch("unreusable")?;
        let cache = RepoScopedInventoryCache::at_dir(dir.clone());
        let unreusable = RepoSeamCacheKey {
            workspace_manifests_hash: format!("x{CARGO_INPUT_IDENTITY_UNAVAILABLE}"),
            ..key()
        };
        let stored = cache.store(&unreusable, b"identity", inventory());
        let loaded = cache.load(&unreusable, b"identity");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(stored.is_err(), "an unreusable key must not publish");
        assert!(matches!(loaded, CacheLoad::Miss));
        Ok(())
    }

    #[test]
    fn only_the_newest_entries_are_retained() -> Result<(), String> {
        let dir = scratch("prune")?;
        let cache = RepoScopedInventoryCache::at_dir(dir.clone());
        let total = RETAINED_ENTRIES + 4;
        for index in 0..total {
            let state = RepoSeamCacheKey {
                files_content_hash: format!("state-{index}"),
                ..key()
            };
            cache.store(&state, b"identity", inventory())?;
            // Distinct modification times so "newest" is well defined.
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let kept = std::fs::read_dir(&dir)
            .map_err(|error| format!("read dir: {error}"))?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .count();
        let last = RepoSeamCacheKey {
            files_content_hash: format!("state-{}", total - 1),
            ..key()
        };
        let newest_kept = matches!(cache.load(&last, b"identity"), CacheLoad::Hit(_));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(kept, RETAINED_ENTRIES);
        assert!(newest_kept, "the newest entry must survive pruning");
        Ok(())
    }

    #[test]
    fn entries_over_the_ceiling_are_neither_written_nor_read() -> Result<(), String> {
        let dir = scratch("ceiling")?;
        let cache = RepoScopedInventoryCache::at_dir(dir.clone());
        cache.store(&key(), b"identity", inventory())?;
        let size = std::fs::metadata(cache.entry_path(&key(), b"identity"))
            .map_err(|error| format!("stat: {error}"))?
            .len();
        // Read side: the same entry is a hit at its size and refused one byte under.
        let at_size = matches!(
            cache.load_within(&key(), b"identity", size),
            CacheLoad::Hit(_)
        );
        let under = cache.load_within(&key(), b"identity", size - 1);
        let refused =
            matches!(&under, CacheLoad::CorruptIgnored { reason } if reason.contains("ceiling"));
        // Write side: a ceiling below the encoded size stores nothing.
        let other = RepoSeamCacheKey {
            files_content_hash: "other".to_owned(),
            ..key()
        };
        let skipped = cache.store_within(&other, b"identity", inventory(), size - 1);
        let written = cache.entry_path(&other, b"identity").exists();
        let allowed = cache
            .store_within(&other, b"identity", inventory(), size + 1024)
            .is_ok();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(at_size, "an entry at the ceiling must be served");
        assert!(
            refused,
            "an entry over the ceiling must not be read: {under:?}"
        );
        assert!(
            skipped.is_err() && !written,
            "an over-ceiling store must write nothing"
        );
        assert!(allowed, "an under-ceiling store must succeed");
        Ok(())
    }

    #[test]
    fn tied_modification_times_never_drop_the_entry_just_written() -> Result<(), String> {
        let dir = scratch("prune-tie")?;
        let cache = RepoScopedInventoryCache::at_dir(dir.clone());
        let tied = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let mut paths = Vec::new();
        for index in 0..(RETAINED_ENTRIES + 3) {
            let path = dir.join(format!("{index:016x}.json"));
            let file = std::fs::File::create(&path).map_err(|error| format!("create: {error}"))?;
            file.set_modified(tied)
                .map_err(|error| format!("set mtime: {error}"))?;
            paths.push(path);
        }
        // The just-written entry sorts first by name, the worst tie case.
        let written = paths[0].clone();
        cache.prune(&written);
        let remaining: Vec<bool> = paths.iter().map(|path| path.exists()).collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(remaining[0], "the entry just written must survive a tie");
        assert_eq!(
            remaining.iter().filter(|kept| **kept).count(),
            RETAINED_ENTRIES,
            "exactly the retained count must remain: {remaining:?}"
        );
        Ok(())
    }
}
