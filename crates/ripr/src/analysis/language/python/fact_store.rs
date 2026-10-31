//! Persistent, content-keyed per-file Python fact store (#10914).
//!
//! `check` in a Python workspace extracted source facts from every workspace
//! file on every run, so a warm run paid a cold run's parse cost (pandas:
//! 1,545 files, ~1.6 s of a ~5 s run on 4 cores). This module is the Python
//! twin of the TypeScript store (RIPR-SPEC-0363, `typescript/fact_store.rs`):
//! one envelope per workspace file holds the facts `extract_source_facts`
//! derived from it, so a warm run serves untouched files from disk.
//!
//! Reuse is keyed on CONTENT, never on a stat tuple: the filename binds the
//! schema, the analyzer identity, the normalized path and a hash of the
//! source bytes, and the envelope repeats them with the workspace-root hash.
//! Extraction reads nothing but `(path, source)`, so equal keys mean equal
//! facts.
//!
//! The diff-mode pass never reads the per-statement `facts` list, so the
//! store leaves it out (it was most of a naive envelope). The imports every
//! owner of a file shares, and the imports and rebinding facts tests of one
//! file repeat, are stored once per distinct value. Debug builds re-extract
//! every served file and assert the served facts equal the fresh ones.
//!
//! Integrity is the seam_cache contract: an unkeyed semantic digest over the
//! serialized envelope; a tampered or truncated entry fails it and is
//! corrupt-ignored, never served.

use super::module_constants::PythonTestRebinding;
use super::module_value::ForeignWrites;
use super::source_facts::PythonSourceFacts;
use super::{PythonImport, normalized_path};
use crate::analysis::seam_cache::{self, CacheLayer, CacheLoad};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// On-disk schema version of a `python-file-facts` envelope. Any change to a
/// serialized fact spelling (field rename, type change, new label) is a
/// schema change: bump this so stale entries read as misses.
/// `11`: each file records `<module>.<attr>` writes so a test-module
/// constant another file rebinds is not credited (#11424). `10` entries
/// lack that fact.
/// `12`: each assertion records the outer callee of an independent nested
/// call equality for per-finding exact reading (#9814). `11` entries lack it.
pub(crate) const PYTHON_FILE_FACT_SCHEMA_VERSION: &str = "12";

/// Approximate per-run ceiling on newly persisted fact bytes. The parse
/// already happened by store time, so exhausting the budget only skips the
/// write.
const MAX_STORE_BYTES_PER_RUN: usize = 256 * 1024 * 1024;

/// Digest domain binding the envelope's identity and served fields. Distinct
/// from every seam_cache domain so a cross-layer copy can never validate.
const ENVELOPE_DIGEST_DOMAIN: &str = "ripr-cache:PythonFileFactEnvelope:v1";

/// Identity of one persisted file fact set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonFileFactKey {
    schema_version: String,
    analyzer_version: String,
    file_path: String,
    content_hash: String,
    workspace_root_hash: String,
}

impl PythonFileFactKey {
    pub(super) fn new(relative: &Path, source: &str, workspace_root: &Path) -> Self {
        Self {
            schema_version: PYTHON_FILE_FACT_SCHEMA_VERSION.to_string(),
            analyzer_version: crate::build_identity::cache_identity().to_string(),
            file_path: normalized_path(relative),
            content_hash: seam_cache::hash_str(source),
            workspace_root_hash: seam_cache::hash_str(&workspace_root.to_string_lossy()),
        }
    }

    /// Entry filename: FNV-1a 64 over NUL-joined admission fields, stable
    /// across Rust releases. The root hash stays out, as in the TypeScript
    /// store.
    fn filename(&self) -> String {
        Self::filename_for(&self.file_path, &self.content_hash)
    }

    fn filename_for(file_path: &str, content_hash: &str) -> String {
        let parts = [
            PYTHON_FILE_FACT_SCHEMA_VERSION,
            crate::build_identity::cache_identity(),
            file_path,
            content_hash,
        ];
        format!(
            "{:016x}.json",
            seam_cache::fnv1a_64(parts.join("\0").as_bytes())
        )
    }
}

/// The facts as stored: the per-statement list is dropped, and the fields
/// owners and tests repeat are interned into per-file tables.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct StoredFacts {
    facts: PythonSourceFacts,
    owner_imports: Interned<Vec<PythonImport>>,
    test_imports: Interned<Vec<PythonImport>>,
    test_rebindings: Interned<PythonTestRebinding>,
    /// The module-level lists every rebinding of a file repeats (#10914:
    /// pandas test files hold hundreds of distinct per-test rebindings).
    rebinding_modules: Interned<RebindingModuleLists>,
    /// Owner fields the workspace pass fills after the store is read (the
    /// foreign writes) or that every function of a module repeats (its module
    /// objects).
    owner_foreign_writes: Interned<ForeignWrites>,
    owner_module_objects: Interned<Vec<String>>,
}

/// [`PythonTestRebinding`]'s module-level lists, as one interned value.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct RebindingModuleLists {
    module_names: Arc<Vec<String>>,
    module_bound: Arc<Vec<String>>,
    attributes: Arc<Vec<String>>,
}

/// Distinct values and, per item, the index of its value.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Interned<T> {
    values: Vec<T>,
    indexes: Vec<usize>,
}

impl<T: PartialEq> Interned<T> {
    fn intern(items: impl Iterator<Item = T>) -> Self {
        let mut interned = Self {
            values: Vec::new(),
            indexes: Vec::new(),
        };
        for item in items {
            let index = match interned.values.iter().rposition(|value| *value == item) {
                Some(index) => index,
                None => {
                    interned.values.push(item);
                    interned.values.len() - 1
                }
            };
            interned.indexes.push(index);
        }
        interned
    }
}

impl<T: Clone> Interned<T> {
    /// The value of each item in order, or `None` when the table does not
    /// cover `count` items.
    fn expand(&self, count: usize) -> Option<Vec<T>> {
        if self.indexes.len() != count {
            return None;
        }
        self.indexes
            .iter()
            .map(|index| self.values.get(*index).cloned())
            .collect()
    }
}

impl StoredFacts {
    fn from_facts(facts: &PythonSourceFacts) -> Self {
        let mut facts = facts.clone();
        facts.facts.clear();
        let owner_imports = Interned::intern(
            facts
                .owners
                .iter_mut()
                .map(|owner| std::mem::take(&mut owner.imports)),
        );
        let test_imports = Interned::intern(
            facts
                .tests
                .iter_mut()
                .map(|test| std::mem::take(&mut test.imports)),
        );
        let rebinding_modules = Interned::intern(facts.tests.iter_mut().map(|test| {
            let rebinding = &mut test.constant_rebinding;
            RebindingModuleLists {
                module_names: std::mem::take(&mut rebinding.module_names),
                module_bound: std::mem::take(&mut rebinding.module_bound),
                attributes: std::mem::take(&mut rebinding.attributes),
            }
        }));
        let test_rebindings = Interned::intern(
            facts
                .tests
                .iter_mut()
                .map(|test| std::mem::take(&mut test.constant_rebinding)),
        );
        let owner_foreign_writes = Interned::intern(
            facts
                .owners
                .iter_mut()
                .map(|owner| std::mem::take(&mut owner.foreign_writes)),
        );
        let owner_module_objects = Interned::intern(
            facts
                .owners
                .iter_mut()
                .map(|owner| std::mem::take(&mut owner.class_facts.module_objects)),
        );
        Self {
            facts,
            owner_imports,
            test_imports,
            test_rebindings,
            rebinding_modules,
            owner_foreign_writes,
            owner_module_objects,
        }
    }

    /// The facts with every interned field restored, or `None` when a table
    /// does not match the owner or test count.
    fn into_facts(self) -> Option<PythonSourceFacts> {
        let Self {
            mut facts,
            owner_imports,
            test_imports,
            test_rebindings,
            rebinding_modules,
            owner_foreign_writes,
            owner_module_objects,
        } = self;
        let owner_imports = owner_imports.expand(facts.owners.len())?;
        let owner_foreign_writes = owner_foreign_writes.expand(facts.owners.len())?;
        let owner_module_objects = owner_module_objects.expand(facts.owners.len())?;
        let test_imports = test_imports.expand(facts.tests.len())?;
        let test_rebindings = test_rebindings.expand(facts.tests.len())?;
        let rebinding_modules = rebinding_modules.expand(facts.tests.len())?;
        for (((owner, imports), foreign_writes), module_objects) in facts
            .owners
            .iter_mut()
            .zip(owner_imports)
            .zip(owner_foreign_writes)
            .zip(owner_module_objects)
        {
            owner.imports = imports;
            owner.foreign_writes = foreign_writes;
            owner.class_facts.module_objects = module_objects;
        }
        for (((test, imports), mut rebinding), lists) in facts
            .tests
            .iter_mut()
            .zip(test_imports)
            .zip(test_rebindings)
            .zip(rebinding_modules)
        {
            let RebindingModuleLists {
                module_names,
                module_bound,
                attributes,
            } = lists;
            rebinding.module_names = module_names;
            rebinding.module_bound = module_bound;
            rebinding.attributes = attributes;
            test.imports = imports;
            test.constant_rebinding = rebinding;
        }
        Some(facts)
    }
}

/// On-disk shape. The key fields are embedded so a read can verify identity
/// even though the filename already encodes a hash of the same fields.
#[derive(serde::Serialize, serde::Deserialize)]
struct PythonFileFactEnvelope {
    #[serde(default, skip_serializing)]
    payload_digest: Option<String>,
    python_file_fact_schema_version: String,
    analyzer_version: String,
    file_path: String,
    content_hash: String,
    workspace_root_hash: String,
    stored: StoredFacts,
}

impl PythonFileFactEnvelope {
    fn new(key: &PythonFileFactKey, stored: StoredFacts) -> Self {
        Self {
            payload_digest: None,
            python_file_fact_schema_version: key.schema_version.clone(),
            analyzer_version: key.analyzer_version.clone(),
            file_path: key.file_path.clone(),
            content_hash: key.content_hash.clone(),
            workspace_root_hash: key.workspace_root_hash.clone(),
            stored,
        }
    }

    fn matches_key(&self, key: &PythonFileFactKey) -> bool {
        self.python_file_fact_schema_version == key.schema_version
            && self.analyzer_version == key.analyzer_version
            && self.file_path == key.file_path
            && self.content_hash == key.content_hash
            && self.workspace_root_hash == key.workspace_root_hash
    }

    /// Unsigned typed body binding identity and every served field in fixed
    /// order; a corruption check, not authentication (seam_cache, #7780).
    fn expected_digest(&self) -> Result<String, String> {
        seam_cache::semantic_body_digest(ENVELOPE_DIGEST_DOMAIN, self)
    }

    fn validate_integrity(&self) -> Result<(), String> {
        let expected = self.expected_digest()?;
        if self.payload_digest.as_deref() == Some(expected.as_str()) {
            Ok(())
        } else {
            Err("missing or mismatching semantic payload digest; delete the cache entry and rerun `ripr check` to rebuild it".to_owned())
        }
    }
}

/// Names the entry file in a corrupt-load reason (#4383).
fn corrupt_entry<T>(path: &Path, reason: impl std::fmt::Display) -> CacheLoad<T> {
    CacheLoad::CorruptIgnored {
        reason: format!("{}: {reason}", path.display()),
    }
}

/// Content-keyed persistent store for one workspace's per-file Python facts.
/// Constructed once per analysis run; shared by the parallel fact workers.
pub(crate) struct PythonFileFactStore {
    /// The cache base whose directory-ness the #4918 probe checks: exactly
    /// two components above `dir`, as registered by `CacheLayer`.
    base: PathBuf,
    dir: PathBuf,
    unusable_base_reason: OnceLock<Option<String>>,
    remaining_store_bytes: AtomicUsize,
}

impl PythonFileFactStore {
    pub(crate) fn at(workspace_root: &Path) -> Self {
        Self::at_base(&seam_cache::cache_base_dir(workspace_root))
    }

    /// Base-anchored constructor for cache maintenance.
    pub(crate) fn at_base(base: &Path) -> Self {
        let dir = seam_cache::cache_layer_versioned_dir(
            base,
            CacheLayer::PythonFileFacts,
            PYTHON_FILE_FACT_SCHEMA_VERSION,
        );
        let base = dir
            .ancestors()
            .nth(2)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.clone());
        Self {
            base,
            dir,
            unusable_base_reason: OnceLock::new(),
            remaining_store_bytes: AtomicUsize::new(MAX_STORE_BYTES_PER_RUN),
        }
    }

    /// The entry directory, with whether storing there also creates
    /// subdirectories. Entry files are written directly in `dir`.
    pub(crate) fn entry_dir(&self) -> (PathBuf, bool) {
        (self.dir.clone(), false)
    }

    #[cfg(test)]
    pub(crate) fn dir_path(&self) -> &Path {
        &self.dir
    }

    /// Entry file for `(relative, source)`.
    #[cfg(test)]
    pub(super) fn entry_path(&self, relative: &Path, source: &str) -> PathBuf {
        self.dir.join(PythonFileFactKey::filename_for(
            &normalized_path(relative),
            &seam_cache::hash_str(source),
        ))
    }

    /// `Some(reason)` when the cache base exists but is not usable. At most
    /// once per instance; a missing base stays `None`.
    pub(super) fn unusable_base_reason(&self) -> Option<&str> {
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
    pub(super) fn load(&self, key: &PythonFileFactKey) -> CacheLoad<PythonSourceFacts> {
        if let Some(reason) = self.unusable_base_reason() {
            return CacheLoad::CorruptIgnored {
                reason: reason.to_owned(),
            };
        }
        let path = self.dir.join(key.filename());
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return CacheLoad::Miss,
            Err(err) if err.kind() == std::io::ErrorKind::NotADirectory => {
                return CacheLoad::Miss;
            }
            Err(err) => return corrupt_entry(&path, format!("read failed: {err}")),
        };
        match serde_json::from_slice::<PythonFileFactEnvelope>(&bytes) {
            Ok(envelope) => {
                if !envelope.matches_key(key) {
                    return CacheLoad::Miss;
                }
                if let Err(reason) = envelope.validate_integrity() {
                    return corrupt_entry(&path, reason);
                }
                match envelope.stored.into_facts() {
                    Some(facts) => CacheLoad::Hit(facts),
                    None => corrupt_entry(&path, "interned table does not cover its items"),
                }
            }
            Err(reason) => corrupt_entry(&path, format!("decode failed: {reason}")),
        }
    }

    /// Persist `facts` for `key` as one envelope. `Ok(false)` when the per-run
    /// byte budget is exhausted; `Err` on an IO or encode failure.
    pub(super) fn store(
        &self,
        key: &PythonFileFactKey,
        facts: &PythonSourceFacts,
    ) -> Result<bool, String> {
        if let Some(reason) = self.unusable_base_reason() {
            return Err(reason.to_owned());
        }
        let envelope = PythonFileFactEnvelope::new(key, StoredFacts::from_facts(facts));
        let bytes =
            seam_cache::encode_integrity_body_compact(&envelope, envelope.expected_digest()?)?;
        let approx_bytes = bytes.len();
        if self
            .remaining_store_bytes
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                remaining.checked_sub(approx_bytes)
            })
            .is_err()
        {
            return Ok(false);
        }
        std::fs::create_dir_all(&self.dir)
            .map_err(|err| format!("create python file fact cache dir failed: {err}"))?;
        crate::atomic_file::write_cache(
            &self.dir.join(key.filename()),
            &bytes,
            "python file fact cache",
        )?;
        Ok(true)
    }
}

/// The facts a store hit must equal: a fresh extraction without the
/// per-statement list the store leaves out.
pub(super) fn served_view(mut facts: PythonSourceFacts) -> PythonSourceFacts {
    facts.facts.clear();
    facts
}

#[cfg(test)]
mod budget_tests;
#[cfg(test)]
mod tests;
