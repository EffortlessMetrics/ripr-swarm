//! Persistent, content-keyed per-file TypeScript fact store (#10725).
//!
//! Issue #10725: `check` re-parses every untouched workspace file on every
//! run. The Rust adapter already persists per-file facts keyed by content
//! (`RepoFileFactCache` in `analysis/seam_cache.rs`); this module is the
//! TypeScript equivalent. One envelope per workspace file holds the facts
//! the single-parse extraction derived — owners, export facts, phase-2
//! facts, and the test extraction — so a warm run serves them from disk
//! instead of re-parsing.
//!
//! Reuse is keyed on CONTENT, never on a stat tuple (#2175/#3848 contract
//! mirrored for the preview adapter): the filename binds the schema, the
//! analyzer identity, the normalized path, and a hash of the source bytes.
//! A same-length rewrite with a restored mtime still misses. The workspace
//! root hash and the alias-map digest stay envelope-only, exactly like
//! `RepoFileFactCacheKey`.
//!
//! Integrity is the seam_cache contract: an unkeyed semantic digest over
//! the serialized envelope; a tampered or truncated entry fails it and is
//! corrupt-ignored, never served. Parse-error facts are persisted too — the
//! envelope is content-keyed, so the cached default facts are as honest as
//! the in-memory cache's.

use super::extraction_cache::{FileFacts, TestExtraction};
use super::paths::normalized_path;
use super::tsconfig::TsAliasMap;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::analysis::seam_cache::{self, CacheLayer, CacheLoad};

#[cfg(test)]
pub(crate) use crate::analysis::language::adapter::render_fact_cache_line;

/// On-disk schema version of a `typescript-file-facts` envelope. Any change
/// to a serialized fact spelling (field rename, type change, new label) is
/// a schema change: bump this so stale entries read as misses.
pub(crate) const TYPESCRIPT_FILE_FACT_SCHEMA_VERSION: &str = "6";

/// Approximate per-run ceiling on newly persisted fact bytes. Parsing the
/// file already happened by store time, so exhausting the budget only skips
/// the write — correctness-neutral, like the in-memory cache bounds.
const MAX_STORE_BYTES_PER_RUN: usize = 256 * 1024 * 1024;

/// Digest domain binding the envelope's identity and served fields. Distinct
/// from every seam_cache domain so a cross-layer copy can never validate.
const ENVELOPE_DIGEST_DOMAIN: &str = "ripr-cache:TypeScriptFileFactEnvelope:v1";

/// Identity of one persisted file fact set. The filename binds schema,
/// analyzer, path, and content hash; the workspace root hash and alias-map
/// digest are envelope-only admission checks, mirroring
/// `RepoFileFactCacheKey`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptFileFactKey {
    schema_version: String,
    analyzer_version: String,
    file_path: String,
    content_hash: String,
    workspace_root_hash: String,
    alias_digest: (u64, u64),
}

impl TypeScriptFileFactKey {
    pub(crate) fn new(
        relative: &Path,
        source: &str,
        workspace_root: &Path,
        alias_map: Option<&TsAliasMap>,
    ) -> Self {
        let alias_digest = alias_map.map_or((0, 0), TsAliasMap::cache_digest);
        Self::with_alias_digest(relative, source, workspace_root, alias_digest)
    }

    pub(crate) fn with_alias_digest(
        relative: &Path,
        source: &str,
        workspace_root: &Path,
        alias_digest: (u64, u64),
    ) -> Self {
        Self {
            schema_version: TYPESCRIPT_FILE_FACT_SCHEMA_VERSION.to_string(),
            analyzer_version: crate::build_identity::cache_identity().to_string(),
            file_path: normalized_path(relative),
            content_hash: seam_cache::hash_str(source),
            workspace_root_hash: seam_cache::hash_str(&workspace_root.to_string_lossy()),
            alias_digest,
        }
    }

    /// Entry filename: FNV-1a 64 over NUL-joined admission fields, stable
    /// across Rust releases (unlike `DefaultHasher`). The root hash and
    /// alias digest deliberately stay out — one file's entry does not move
    /// when another workspace's cache shares neither.
    fn filename(&self) -> String {
        Self::filename_for(&self.file_path, &self.content_hash)
    }

    fn filename_for(file_path: &str, content_hash: &str) -> String {
        let parts = [
            TYPESCRIPT_FILE_FACT_SCHEMA_VERSION,
            crate::build_identity::cache_identity(),
            file_path,
            content_hash,
        ];
        let mut buf = String::new();
        for (idx, part) in parts.iter().enumerate() {
            if idx > 0 {
                buf.push('\0');
            }
            buf.push_str(part);
        }
        format!("{:016x}.json", seam_cache::fnv1a_64(buf.as_bytes()))
    }
}

/// Facts served from a persisted envelope, rehydrated into the
/// process-lifetime caches by the adapter.
pub(crate) struct PersistedFileFacts {
    pub(crate) file_facts: FileFacts,
    pub(crate) test_extraction: Option<TestExtraction>,
    /// The file's partial-extraction verdict, when one was recorded.
    pub(crate) partial_extraction: Option<super::types::PersistedPartialExtraction>,
}

/// On-disk shape. The key fields are embedded so a read can verify identity
/// even though the filename already encodes a hash of the same fields.
#[derive(serde::Serialize, serde::Deserialize)]
struct TypeScriptFileFactEnvelope {
    #[serde(default, skip_serializing)]
    payload_digest: Option<String>,
    typescript_file_fact_schema_version: String,
    analyzer_version: String,
    file_path: String,
    content_hash: String,
    workspace_root_hash: String,
    alias_digest: (u64, u64),
    file_facts: FileFacts,
    /// `None` for production files (extraction never ran for them) and for
    /// parse-error files, whose tests were never extracted.
    #[serde(default)]
    test_extraction: Option<TestExtraction>,
    /// Whether the extractor dropped a registration shape in this test file,
    /// a pure function of the same bytes. `None` when it was not recorded
    /// (production files, parse errors, a failed check).
    #[serde(default)]
    partial_extraction: Option<super::types::PersistedPartialExtraction>,
}

impl TypeScriptFileFactEnvelope {
    fn new(
        key: &TypeScriptFileFactKey,
        file_facts: FileFacts,
        test_extraction: Option<TestExtraction>,
        partial_extraction: Option<super::types::PersistedPartialExtraction>,
    ) -> Self {
        Self {
            payload_digest: None,
            typescript_file_fact_schema_version: key.schema_version.clone(),
            analyzer_version: key.analyzer_version.clone(),
            file_path: key.file_path.clone(),
            content_hash: key.content_hash.clone(),
            workspace_root_hash: key.workspace_root_hash.clone(),
            alias_digest: key.alias_digest,
            file_facts,
            test_extraction,
            partial_extraction,
        }
    }

    fn matches_key(&self, key: &TypeScriptFileFactKey) -> bool {
        self.typescript_file_fact_schema_version == key.schema_version
            && self.analyzer_version == key.analyzer_version
            && self.file_path == key.file_path
            && self.content_hash == key.content_hash
            && self.workspace_root_hash == key.workspace_root_hash
            && self.alias_digest == key.alias_digest
    }

    /// Unsigned typed body binds identity and all serialized served fields in
    /// fixed order; the digest is a corruption check, not authentication
    /// (seam_cache contract, #7780).
    fn expected_digest(&self) -> Result<String, String> {
        // Shared lists digest in their wire form, so the digest binds the
        // same bytes the encoder writes.
        let _wire = super::types::SharedWireScope::open();
        seam_cache::semantic_body_digest(ENVELOPE_DIGEST_DOMAIN, self)
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

/// Names the entry file in a corrupt-load reason so a burst of ignored-entry
/// warnings is attributable (#4383, same convention as seam_cache).
fn corrupt_entry<T>(path: &Path, reason: impl std::fmt::Display) -> CacheLoad<T> {
    CacheLoad::CorruptIgnored {
        reason: format!("{}: {reason}", path.display()),
    }
}

/// Content-keyed persistent store for one workspace's per-file TypeScript
/// facts. Constructed once per analysis run; the probes and the known-path
/// snapshot evaluate at most once per instance.
pub(crate) struct TypeScriptFileFactStore {
    /// The cache base whose directory-ness the #4918 probe checks — exactly
    /// two components above `dir`, as registered by `CacheLayer`.
    base: PathBuf,
    dir: PathBuf,
    unusable_base_reason: OnceLock<Option<String>>,
    /// Header-only snapshot of the normalized paths that had a schema- and
    /// analyzer-matching entry when the run began. A miss against a
    /// snapshotted path means the file's content changed on disk — the
    /// content-invalidation signal the `-v` line and the stats disclose.
    known_paths: OnceLock<BTreeSet<String>>,
    remaining_store_bytes: AtomicUsize,
}

impl TypeScriptFileFactStore {
    pub(crate) fn at(workspace_root: &Path) -> Self {
        Self::at_base(&seam_cache::cache_base_dir(workspace_root))
    }

    /// Base-anchored constructor for cache maintenance (mirrors seam_cache's
    /// `at_base` family): `base` is the already-resolved cache base.
    pub(crate) fn at_base(base: &Path) -> Self {
        let dir = seam_cache::cache_layer_versioned_dir(
            base,
            CacheLayer::TypeScriptFileFacts,
            TYPESCRIPT_FILE_FACT_SCHEMA_VERSION,
        );
        // `{base}/typescript-file-facts/{version}`: the base is two
        // components up, derived lexically as seam_cache's `at` does.
        let base = dir
            .ancestors()
            .nth(2)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.clone());
        Self {
            base,
            dir,
            unusable_base_reason: OnceLock::new(),
            known_paths: OnceLock::new(),
            remaining_store_bytes: AtomicUsize::new(MAX_STORE_BYTES_PER_RUN),
        }
    }

    /// The entry directory this cache writes into, with whether storing
    /// there also creates subdirectories. Entry files are written directly
    /// in `dir` (mirrors `RepoFileFactCache::entry_dir`). Production
    /// (seam_cache `production_entry_dirs`, doctor's writability probe) and
    /// the constructor pin both call this — never a hand-written copy.
    pub(crate) fn entry_dir(&self) -> (PathBuf, bool) {
        (self.dir.clone(), false)
    }

    /// Borrow of the versioned entry directory (`entry_dir` without the flag),
    /// for the layer-inventory pin in seam_cache's tests.
    #[cfg(test)]
    pub(crate) fn dir_path(&self) -> &Path {
        &self.dir
    }

    /// Entry file for `(relative, source)` — the test seam that locates an
    /// envelope without knowing the key type. The filename depends only on
    /// the normalized path and content hash, so no root or alias inputs are
    /// needed.
    #[cfg(test)]
    pub(crate) fn entry_path(&self, relative: &Path, source: &str) -> PathBuf {
        self.dir.join(TypeScriptFileFactKey::filename_for(
            &normalized_path(relative),
            &seam_cache::hash_str(source),
        ))
    }

    /// `Some(reason)` when the cache base exists but is not usable (not a
    /// directory, or holds Git-tracked entries ripr did not write). At most
    /// once per instance; a missing base stays `None` — the ordinary cold
    /// cache whose lookups read as silent misses.
    pub(crate) fn unusable_base_reason(&self) -> Option<&str> {
        self.unusable_base_reason
            .get_or_init(|| {
                seam_cache::cache_dir_not_a_directory_reason(&self.base)
                    .or_else(|| seam_cache::cache_base_untrusted_reason(&self.base))
            })
            .as_deref()
    }

    /// Facts for `key`, admitted only when the envelope's embedded identity
    /// matches and its integrity digest validates. Corrupt entries are
    /// corrupt-ignored, never served; an absent entry is a plain miss.
    pub(crate) fn load(&self, key: &TypeScriptFileFactKey) -> CacheLoad<PersistedFileFacts> {
        if let Some(reason) = self.unusable_base_reason() {
            return CacheLoad::CorruptIgnored {
                reason: reason.to_owned(),
            };
        }
        let path = self.dir.join(key.filename());
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return CacheLoad::Miss,
            // A path component that is a file is a missing entry, not a
            // corrupt one (#9633, audit F-C4-4): nothing was read, and the
            // store that follows reports the same condition once.
            Err(err) if err.kind() == std::io::ErrorKind::NotADirectory => {
                return CacheLoad::Miss;
            }
            Err(err) => return corrupt_entry(&path, format!("read failed: {err}")),
        };
        let decoded = {
            let _wire = super::types::SharedWireScope::open();
            serde_json::from_slice::<TypeScriptFileFactEnvelope>(&bytes)
        };
        match decoded {
            Ok(envelope) => {
                if !envelope.matches_key(key) {
                    return CacheLoad::Miss;
                }
                if let Err(reason) = envelope.validate_integrity() {
                    return corrupt_entry(&path, reason);
                }
                let mut test_extraction = envelope.test_extraction;
                if let Some((tests, _, _)) = test_extraction.as_mut() {
                    super::types::reshare_file_lists(tests);
                }
                CacheLoad::Hit(PersistedFileFacts {
                    file_facts: envelope.file_facts,
                    test_extraction,
                    partial_extraction: envelope.partial_extraction,
                })
            }
            Err(reason) => corrupt_entry(&path, format!("decode failed: {reason}")),
        }
    }

    /// True when a schema- and analyzer-matching entry for `normalized_file`
    /// existed when the run began. The snapshot is header-only and taken
    /// once; entries written during this run are deliberately not observed.
    pub(crate) fn knows_file(&self, normalized_file: &str) -> bool {
        self.known_paths_snapshot().contains(normalized_file)
    }

    fn known_paths_snapshot(&self) -> &BTreeSet<String> {
        self.known_paths.get_or_init(|| {
            let mut paths = BTreeSet::new();
            let identity = crate::build_identity::cache_identity();
            let Ok(entries) = std::fs::read_dir(&self.dir) else {
                return paths;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Some(header) = read_entry_header(&path) else {
                    continue;
                };
                if header.schema_version == TYPESCRIPT_FILE_FACT_SCHEMA_VERSION
                    && header.analyzer_version == identity
                {
                    paths.insert(header.file_path);
                }
            }
            paths
        })
    }

    /// Persist facts for `key` as one content-keyed envelope.
    ///
    /// `Ok(true)` when an envelope was written; `Ok(false)` when the per-run
    /// byte budget was already exhausted (the parse is sunk either way, so
    /// skipping the write is correctness-neutral); `Err` on an IO or encode
    /// failure. The budget is charged the encoded envelope's length, which
    /// can be many times the source's.
    pub(crate) fn store(
        &self,
        key: &TypeScriptFileFactKey,
        file_facts: &FileFacts,
        test_extraction: Option<&TestExtraction>,
        partial_extraction: Option<&super::types::PersistedPartialExtraction>,
    ) -> Result<bool, String> {
        if let Some(reason) = self.unusable_base_reason() {
            return Err(reason.to_owned());
        }
        let envelope = TypeScriptFileFactEnvelope::new(
            key,
            file_facts.clone(),
            test_extraction.cloned(),
            partial_extraction.cloned(),
        );
        let digest = envelope.expected_digest()?;
        let bytes = {
            // Compact, with shared lists written once: a file with thousands of
            // tests or owners would otherwise store one copy of its file-wide
            // lists for each (VS Code: a 267 MB envelope for one test file).
            let _wire = super::types::SharedWireScope::open();
            seam_cache::encode_integrity_body_compact(&envelope, digest)?
        };
        let approx_bytes = bytes.len();
        let mut remaining = self.remaining_store_bytes.load(Ordering::Relaxed);
        loop {
            if remaining < approx_bytes {
                return Ok(false);
            }
            match self.remaining_store_bytes.compare_exchange_weak(
                remaining,
                remaining - approx_bytes,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => remaining = current,
            }
        }
        std::fs::create_dir_all(&self.dir)
            .map_err(|err| format!("create typescript file fact cache dir failed: {err}"))?;
        crate::atomic_file::write_cache(
            &self.dir.join(key.filename()),
            &bytes,
            "typescript file fact cache",
        )?;
        Ok(true)
    }
}

/// Whether `facts` read nothing but the file's own bytes. A relative
/// re-export resolves its target on disk (a directory's `index` or
/// `package.json`, a missing `tsc` output mapped back to source), and that
/// target can change while this file's content does not, so such a file is
/// recomputed every run instead of persisted.
pub(crate) fn export_facts_are_disk_independent(facts: &FileFacts) -> bool {
    // An own export list (`export { a as c }`, no `from`) records a
    // self-hop resolved without the filesystem. Every `export ... from`
    // resolves on disk, even one whose target is this module today, so the
    // flag, not the recorded target, decides.
    !facts.export_facts.re_exports_from_source
}

/// The envelope fields the known-path snapshot needs; serde skips the rest
/// and the truncated-prefix visitor below stops after them.
#[derive(serde::Deserialize)]
struct EntryHeader {
    #[serde(rename = "typescript_file_fact_schema_version")]
    schema_version: String,
    analyzer_version: String,
    file_path: String,
}

/// Bytes read from the front of an entry to find its header; the writer
/// emits the header fields first.
const ENTRY_HEADER_PREFIX_BYTES: u64 = 8 * 1024;

/// Header from an entry's first bytes, so the snapshot never reads or parses
/// a facts body. Falls back to the whole file when the prefix does not hold
/// all three header fields.
fn read_entry_header(path: &Path) -> Option<EntryHeader> {
    use std::io::Read as _;
    let mut prefix = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(ENTRY_HEADER_PREFIX_BYTES)
        .read_to_end(&mut prefix)
        .ok()?;
    if let Some(header) = header_from_prefix(&prefix) {
        return Some(header);
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Parses keys from the start of the envelope object until the three header
/// fields are found. The truncated remainder is never parsed, so the
/// deserializer's own end-of-object error after an early stop is expected.
fn header_from_prefix(prefix: &[u8]) -> Option<EntryHeader> {
    use serde::de::{Deserializer as _, MapAccess, Visitor};
    struct HeaderVisitor<'a>(&'a mut Option<EntryHeader>);
    impl<'de> Visitor<'de> for HeaderVisitor<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a typescript file fact envelope")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            let (mut schema_version, mut analyzer_version, mut file_path) = (None, None, None);
            while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                match key.as_ref() {
                    "typescript_file_fact_schema_version" => {
                        schema_version = Some(map.next_value()?);
                    }
                    "analyzer_version" => analyzer_version = Some(map.next_value()?),
                    "file_path" => file_path = Some(map.next_value()?),
                    _ => {
                        map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                match (schema_version, analyzer_version, file_path) {
                    (Some(schema_version), Some(analyzer_version), Some(file_path)) => {
                        *self.0 = Some(EntryHeader {
                            schema_version,
                            analyzer_version,
                            file_path,
                        });
                        return Ok(());
                    }
                    partial => (schema_version, analyzer_version, file_path) = partial,
                }
            }
            Ok(())
        }
    }
    let mut header = None;
    let mut deserializer = serde_json::Deserializer::from_slice(prefix);
    let _ = deserializer.deserialize_map(HeaderVisitor(&mut header));
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_pins_ast_probe_shape_facts() -> Result<(), String> {
        // #11250 added per-line probe shapes to the persisted facts, so
        // envelopes written before them must read as misses.
        assert_eq!(TYPESCRIPT_FILE_FACT_SCHEMA_VERSION, "6");
        // A stale-schema envelope: the exact bytes the pre-#11250 ("3")
        // binary wrote — same identity, valid integrity digest, older
        // schema version — sitting at the entry path the current key
        // computes, so only the version gate can refuse it.
        let root = super::super::tests::ts_unique_tempdir("ts-fact-schema-stale")?;
        let store = TypeScriptFileFactStore::at(&root);
        let relative = Path::new("src/a.ts");
        let source = "export const a = 1;\n";
        let key = TypeScriptFileFactKey::with_alias_digest(relative, source, &root, (0, 0));
        let facts = FileFacts {
            parse_error: None,
            owners: Vec::new(),
            export_facts: super::super::related_tests::ModuleExportFacts::default(),
            phase_two: super::super::owners::PhaseTwoFacts::default(),
        };
        let mut stale = TypeScriptFileFactEnvelope::new(&key, facts.clone(), None, None);
        stale.typescript_file_fact_schema_version = "3".to_string();
        let digest = stale.expected_digest()?;
        let bytes = seam_cache::encode_integrity_body(&stale, digest)?;
        std::fs::create_dir_all(store.dir_path()).map_err(|err| {
            format!(
                "create stale-schema entry dir {}: {err}",
                store.dir_path().display()
            )
        })?;
        let entry = store.entry_path(relative, source);
        std::fs::write(&entry, &bytes)
            .map_err(|err| format!("write stale-schema entry {}: {err}", entry.display()))?;
        let stale_load = store.load(&key);
        // Positive control: a current-schema envelope at the same path is
        // a hit, so the miss above is the version gate, not a misplaced
        // file.
        let current = TypeScriptFileFactEnvelope::new(&key, facts, None, None);
        let digest = current.expected_digest()?;
        let bytes = seam_cache::encode_integrity_body(&current, digest)?;
        std::fs::write(&entry, &bytes)
            .map_err(|err| format!("write current-schema entry {}: {err}", entry.display()))?;
        let current_load = store.load(&key);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            matches!(stale_load, CacheLoad::Miss),
            "a stale-schema envelope must read as a miss"
        );
        assert!(
            matches!(current_load, CacheLoad::Hit(_)),
            "a current-schema envelope at the same path must be a hit"
        );
        Ok(())
    }

    #[test]
    fn envelope_round_trips_through_codec() -> Result<(), String> {
        let key = TypeScriptFileFactKey::with_alias_digest(
            Path::new("src/a.ts"),
            "export const a = 1;\n",
            Path::new("/workspace"),
            (7, 9),
        );
        let facts = FileFacts {
            parse_error: None,
            owners: Vec::new(),
            export_facts: super::super::related_tests::ModuleExportFacts::default(),
            phase_two: super::super::owners::PhaseTwoFacts::default(),
        };
        let envelope = TypeScriptFileFactEnvelope::new(&key, facts.clone(), None, None);
        let digest = envelope.expected_digest()?;
        let bytes = seam_cache::encode_integrity_body(&envelope, digest)?;
        let decoded: TypeScriptFileFactEnvelope = serde_json::from_slice(&bytes)
            .map_err(|err| format!("decode round-tripped envelope: {err}"))?;
        assert!(decoded.matches_key(&key));
        decoded.validate_integrity()?;
        assert_eq!(decoded.file_facts.parse_error, facts.parse_error);
        Ok(())
    }

    #[test]
    fn tampered_envelope_fails_integrity() -> Result<(), String> {
        let key = TypeScriptFileFactKey::with_alias_digest(
            Path::new("src/a.ts"),
            "export const a = 1;\n",
            Path::new("/workspace"),
            (0, 0),
        );
        let facts = FileFacts {
            parse_error: None,
            owners: Vec::new(),
            export_facts: super::super::related_tests::ModuleExportFacts::default(),
            phase_two: super::super::owners::PhaseTwoFacts::default(),
        };
        let envelope = TypeScriptFileFactEnvelope::new(&key, facts, None, None);
        let digest = envelope.expected_digest()?;
        let mut bytes = seam_cache::encode_integrity_body(&envelope, digest)?;
        let flip = bytes.len() / 2;
        bytes[flip] = bytes[flip].wrapping_add(1);
        // A tampered entry must not be served, whether the flip breaks the
        // JSON outright (a decode refusal) or leaves a digest mismatch.
        if let Ok(decoded) = serde_json::from_slice::<TypeScriptFileFactEnvelope>(&bytes) {
            assert!(!decoded.matches_key(&key) || decoded.validate_integrity().is_err())
        }
        Ok(())
    }
}
