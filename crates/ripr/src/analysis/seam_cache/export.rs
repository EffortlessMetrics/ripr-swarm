//! Current-input cache export. Never maintains or deletes the live cache.
use super::{
    CacheLoad, RepoFileFactCache, RepoFileFactCacheKey, RepoSeamCacheKey, RepoSeamFactCache,
    cache_base_dir, codec, resolve_sharded_cache_file,
};
use crate::analysis::seam_inventory::current_cache_keys_at;
use crate::config::load_for_root;
use std::fs::{self, Metadata, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize)]
pub(crate) struct CacheExportReport {
    pub(crate) schema_version: &'static str,
    pub(crate) cache_dir: PathBuf,
    pub(crate) classified_entries: usize,
    pub(crate) file_fact_entries: usize,
    pub(crate) missing_file_facts: usize,
    pub(crate) copied_files: usize,
    /// Encoded regular-file lengths, not allocated filesystem bytes.
    pub(crate) logical_file_bytes: u64,
}

/// Refuse aliases before reading or writing. This is a cooperative-filesystem
/// guard, not protection against a hostile concurrent directory replacement.
fn refuse_links(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if linked(&metadata) => {
                return Err(format!(
                    "cache export refuses linked path {}",
                    ancestor.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("inspect {}: {error}", ancestor.display())),
        }
    }
    Ok(())
}

#[cfg(windows)]
fn linked(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn linked(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn read_entry(path: &Path) -> Result<Option<Vec<u8>>, String> {
    refuse_links(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return Err(format!(
                "cache export requires regular entry {}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("inspect {}: {error}", path.display())),
    }
    fs::read(path)
        .map(Some)
        .map_err(|error| format!("read {}: {error}", path.display()))
}

fn relocated(path: &Path, source: &Path, output: &Path) -> Result<PathBuf, String> {
    let relative = path
        .strip_prefix(source)
        .map_err(|error| format!("cache entry outside source: {error}"))?;
    Ok(output.join(relative))
}

fn copy_bytes(
    path: &Path,
    bytes: &[u8],
    source: &Path,
    report: &mut CacheExportReport,
) -> Result<(), String> {
    let destination = relocated(path, source, &report.cache_dir)?;
    let parent = destination.parent().ok_or("cache entry has no parent")?;
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write {}: {error}", destination.display()))?;
    report.copied_files += 1;
    report.logical_file_bytes = report
        .logical_file_bytes
        .checked_add(u64::try_from(bytes.len()).map_err(|error| error.to_string())?)
        .ok_or("export byte count overflow")?;
    Ok(())
}

/// Copy exactly the representation the ordinary loader would prefer. Validate
/// the copied bytes, never a second read of the live source. This preserves
/// immutable-generation references even if a publisher replaces the manifest.
fn copy_classified(
    cache: &RepoSeamFactCache,
    key: &RepoSeamCacheKey,
    source: &Path,
    report: &mut CacheExportReport,
) -> Result<bool, String> {
    if let Some(bytes) = read_entry(&cache.entry_path(key))? {
        copy_bytes(&cache.entry_path(key), &bytes, source, report)?;
    } else {
        let path = cache.sharded_manifest_path(key);
        let Some(bytes) = read_entry(&path)? else {
            return Ok(false);
        };
        let manifest = codec::decode_sharded_manifest(&bytes)?;
        if !manifest.matches_key(key) {
            return Err("export manifest key mismatch".to_owned());
        }
        manifest.validate_integrity()?;
        for shard in &manifest.shards {
            let path = resolve_sharded_cache_file(&cache.sharded_entry_dir(key), &shard.file)?;
            let bytes = read_entry(&path)?
                .ok_or_else(|| format!("missing export shard {}", path.display()))?;
            copy_bytes(&path, &bytes, source, report)?;
        }
        copy_bytes(&path, &bytes, source, report)?;
    }
    let copied = RepoSeamFactCache {
        dir: relocated(&cache.dir, source, &report.cache_dir)?,
        sharded_dir: relocated(&cache.sharded_dir, source, &report.cache_dir)?,
    };
    match copied.load_classified_seams_with_fallback(key) {
        CacheLoad::Hit(_) => {
            report.classified_entries += 1;
            Ok(true)
        }
        CacheLoad::Miss => Err("copied classified entry does not match current key".to_owned()),
        CacheLoad::CorruptIgnored { reason } => {
            Err(format!("invalid copied classified entry: {reason}"))
        }
    }
}

fn export_selected(
    caches: &[RepoSeamFactCache],
    facts: &RepoFileFactCache,
    source: &Path,
    output: &Path,
    key: &RepoSeamCacheKey,
    files: &[RepoFileFactCacheKey],
) -> Result<CacheExportReport, String> {
    let mut report = CacheExportReport {
        schema_version: "0.1",
        cache_dir: output.to_path_buf(),
        classified_entries: 0,
        file_fact_entries: 0,
        missing_file_facts: 0,
        copied_files: 0,
        logical_file_bytes: 0,
    };
    for cache in caches {
        copy_classified(cache, key, source, &mut report)?;
    }
    if report.classified_entries == 0 {
        return Err("no valid classified cache for current inputs; run repo exposure or repo badge analysis first".to_owned());
    }
    let copied = RepoFileFactCache {
        dir: relocated(&facts.dir, source, output)?,
        base: output.to_path_buf(),
        unusable_base_reason: std::sync::OnceLock::new(),
    };
    for key in files {
        let path = facts.entry_path(key);
        let Some(bytes) = read_entry(&path)? else {
            report.missing_file_facts += 1;
            continue;
        };
        copy_bytes(&path, &bytes, source, &mut report)?;
        match copied.load_file_facts(key) {
            CacheLoad::Hit(_) => report.file_fact_entries += 1,
            CacheLoad::Miss => return Err("copied file fact does not match current key".to_owned()),
            CacheLoad::CorruptIgnored { reason } => {
                return Err(format!("invalid copied file fact: {reason}"));
            }
        }
    }
    Ok(report)
}

pub(crate) fn export_current(root: &Path, output: &Path) -> Result<CacheExportReport, String> {
    let metadata = fs::metadata(root).map_err(|error| format!("inspect export root: {error}"))?;
    if !metadata.is_dir() {
        return Err("cache export root must be a directory".to_owned());
    }
    // WorkspaceKeyContext hashes the supplied root spelling. Preserve it;
    // canonical paths below serve only filesystem confinement and relocation.
    let source = cache_base_dir(root);
    refuse_links(&source)?;
    let source =
        fs::canonicalize(&source).map_err(|error| format!("resolve source cache: {error}"))?;
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(output)
    };
    refuse_links(&output)?;
    let parent = output.parent().ok_or("export output has no parent")?;
    let parent =
        fs::canonicalize(parent).map_err(|error| format!("resolve export parent: {error}"))?;
    let name = output
        .file_name()
        .ok_or("export output needs a new directory name")?;
    let output = parent.join(name);
    if output.starts_with(&source) || source.starts_with(&output) {
        return Err("export output must not overlap source cache".to_owned());
    }
    let config = load_for_root(root)?;
    let (key, files) = current_cache_keys_at(root, &config)?;
    fs::create_dir(&output)
        .map_err(|error| format!("create exclusive export {}: {error}", output.display()))?;
    let result = (|| {
        let raw_base = cache_base_dir(root);
        let caches = [
            RepoSeamFactCache::at(root),
            RepoSeamFactCache::at_compact_classified(root),
        ]
        .into_iter()
        .map(|cache| {
            Ok(RepoSeamFactCache {
                dir: relocated(&cache.dir, &raw_base, &source)?,
                sharded_dir: relocated(&cache.sharded_dir, &raw_base, &source)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
        let facts = RepoFileFactCache::at(root);
        let facts = RepoFileFactCache {
            dir: relocated(&facts.dir, &raw_base, &source)?,
            base: source.clone(),
            unusable_base_reason: std::sync::OnceLock::new(),
        };
        let report = export_selected(&caches, &facts, &source, &output, &key, &files)?;
        let config = load_for_root(root)?;
        if current_cache_keys_at(root, &config)? != (key, files) {
            return Err(
                "workspace inputs changed during export; retry with a fresh output".to_owned(),
            );
        }
        Ok(report)
    })();
    result.map_err(|error| {
        format!(
            "{error}; incomplete owned export retained at {}",
            output.display()
        )
    })
}
#[cfg(test)]
mod tests {
    use super::super::FileFacts;
    use super::super::tests::{empty_state, isolated_dir, sample_classified};
    use super::*;
    use std::collections::BTreeMap;

    fn inventory(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
        fn walk(
            root: &Path,
            dir: &Path,
            files: &mut BTreeMap<PathBuf, Vec<u8>>,
        ) -> Result<(), String> {
            for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                let path = entry.path();
                if entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_dir()
                {
                    walk(root, &path, files)?;
                } else {
                    files.insert(
                        path.strip_prefix(root)
                            .map_err(|error| error.to_string())?
                            .to_path_buf(),
                        fs::read(&path).map_err(|error| error.to_string())?,
                    );
                }
            }
            Ok(())
        }
        let mut files = BTreeMap::new();
        walk(root, root, &mut files)?;
        Ok(files)
    }

    struct Fixture {
        root: PathBuf,
        source: PathBuf,
        output: PathBuf,
        caches: Vec<RepoSeamFactCache>,
        facts: RepoFileFactCache,
        key: RepoSeamCacheKey,
        file: RepoFileFactCacheKey,
    }
    impl Fixture {
        fn new(label: &str) -> Result<Self, String> {
            let root = isolated_dir(label);
            let source = root.join("source");
            let output = root.join("export");
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
            let caches = vec![
                RepoSeamFactCache::at_dir(source.join("full")),
                RepoSeamFactCache::at_dir(source.join("compact")),
            ];
            let facts = RepoFileFactCache::at_dir(source.join("facts"));
            let key = empty_state().cache_key();
            let file = RepoFileFactCacheKey::new(Path::new("src/foo.rs"), b"current");
            for cache in &caches {
                cache.store_classified_seams_with_limit(
                    &key,
                    &[sample_classified(), sample_classified()],
                    None,
                    1,
                )?;
            }
            facts.store_file_facts(
                &file,
                &FileFacts {
                    path: PathBuf::from("src/foo.rs"),
                    ..FileFacts::default()
                },
            )?;
            Ok(Self {
                root,
                source,
                output,
                caches,
                facts,
                key,
                file,
            })
        }
        fn export(&self) -> Result<CacheExportReport, String> {
            export_selected(
                &self.caches,
                &self.facts,
                &self.source,
                &self.output,
                &self.key,
                std::slice::from_ref(&self.file),
            )
        }
    }

    #[test]
    fn snapshot_keeps_current_manifest_references_and_file_facts_without_history()
    -> Result<(), String> {
        let fixture = Fixture::new("export-references")?;
        let mut historical = fixture.key.clone();
        historical.files_content_hash = "historical".to_owned();
        for cache in &fixture.caches {
            cache.store_classified_seams_with_limit(
                &historical,
                &[sample_classified(), sample_classified()],
                None,
                1,
            )?;
            fs::write(
                cache
                    .sharded_entry_dir(&fixture.key)
                    .join("unreferenced.json"),
                b"unreferenced",
            )
            .map_err(|error| error.to_string())?;
        }
        let old_file = RepoFileFactCacheKey::new(Path::new("src/foo.rs"), b"historical");
        fixture
            .facts
            .store_file_facts(&old_file, &FileFacts::default())?;
        let before = inventory(&fixture.source)?;
        let report = fixture.export()?;
        assert_eq!(
            (
                report.classified_entries,
                report.file_fact_entries,
                report.missing_file_facts,
                report.copied_files
            ),
            (2, 1, 0, 7)
        );
        assert_eq!(
            inventory(&fixture.source)?,
            before,
            "export must not alter any source bytes or paths"
        );
        let exported = inventory(&fixture.output)?;
        assert!(
            exported
                .keys()
                .all(|path| path.file_name() != Some(std::ffi::OsStr::new("unreferenced.json")))
        );
        for cache in &fixture.caches {
            let copied = RepoSeamFactCache {
                dir: relocated(&cache.dir, &fixture.source, &fixture.output)?,
                sharded_dir: relocated(&cache.sharded_dir, &fixture.source, &fixture.output)?,
            };
            assert!(
                matches!(copied.load_classified_seams_with_fallback(&fixture.key), CacheLoad::Hit((seams, _, _)) if seams.len() == 2)
            );
            assert!(matches!(
                copied.load_classified_seams_with_fallback(&historical),
                CacheLoad::Miss
            ));
        }
        let copied = RepoFileFactCache::at_dir(fixture.output.join("facts"));
        assert!(
            matches!(copied.load_file_facts(&fixture.file), CacheLoad::Hit(facts) if facts.path == Path::new("src/foo.rs"))
        );
        assert!(matches!(copied.load_file_facts(&old_file), CacheLoad::Miss));
        fs::remove_dir_all(fixture.root).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn snapshot_refuses_missing_corrupt_wrong_key_and_escaping_shards() -> Result<(), String> {
        for case in ["missing", "corrupt", "digest", "wrong-key", "escape"] {
            let fixture = Fixture::new(case)?;
            let cache = &fixture.caches[0];
            let manifest_path = cache.sharded_manifest_path(&fixture.key);
            let mut manifest = codec::decode_sharded_manifest(
                &fs::read(&manifest_path).map_err(|error| error.to_string())?,
            )?;
            let shard_path = resolve_sharded_cache_file(
                &cache.sharded_entry_dir(&fixture.key),
                &manifest.shards[0].file,
            )?;
            match case {
                "missing" => fs::remove_file(&shard_path).map_err(|error| error.to_string())?,
                "corrupt" => fs::write(&shard_path, b"{}").map_err(|error| error.to_string())?,
                "digest" => {
                    let bytes = fs::read(&manifest_path).map_err(|error| error.to_string())?;
                    let mut value: serde_json::Value =
                        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                    value["payload_digest"] = serde_json::json!("sha256:invalid");
                    fs::write(
                        &manifest_path,
                        serde_json::to_vec(&value).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                }
                "wrong-key" => {
                    let mut key = fixture.key.clone();
                    key.files_content_hash = "another-input".to_owned();
                    cache.store_classified_seams_with_limit(
                        &key,
                        &[sample_classified(), sample_classified()],
                        None,
                        1,
                    )?;
                    let other = codec::decode_sharded_manifest(
                        &fs::read(cache.sharded_manifest_path(&key))
                            .map_err(|error| error.to_string())?,
                    )?;
                    let bytes = fs::read(resolve_sharded_cache_file(
                        &cache.sharded_entry_dir(&key),
                        &other.shards[0].file,
                    )?)
                    .map_err(|error| error.to_string())?;
                    fs::write(&shard_path, bytes).map_err(|error| error.to_string())?;
                }
                "escape" => {
                    manifest.shards[0].file = "../escape.json".to_owned();
                    fs::write(&manifest_path, codec::encode_sharded_manifest(&manifest)?)
                        .map_err(|error| error.to_string())?;
                }
                _ => return Err("unknown fixture case".to_owned()),
            }
            let before = inventory(&fixture.source)?;
            assert!(
                fixture.export().is_err(),
                "{case} must refuse a successful snapshot"
            );
            assert_eq!(inventory(&fixture.source)?, before);
            fs::remove_dir_all(fixture.root).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    #[test]
    fn snapshot_counts_missing_optional_facts_and_refuses_corrupt_present_facts()
    -> Result<(), String> {
        let fixture = Fixture::new("export-file-fact")?;
        fs::remove_file(fixture.facts.entry_path(&fixture.file))
            .map_err(|error| error.to_string())?;
        let report = fixture.export()?;
        assert_eq!(
            (report.file_fact_entries, report.missing_file_facts),
            (0, 1)
        );
        fs::remove_dir_all(fixture.output.clone()).map_err(|error| error.to_string())?;
        fs::create_dir(&fixture.output).map_err(|error| error.to_string())?;
        fs::write(fixture.facts.entry_path(&fixture.file), b"{}")
            .map_err(|error| error.to_string())?;
        let before = inventory(&fixture.source)?;
        if fixture.export().is_ok() {
            return Err("corrupt present fact unexpectedly exported".to_owned());
        }
        assert_eq!(inventory(&fixture.source)?, before);
        fs::remove_dir_all(fixture.root).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn normalized_source_layout_preserves_relative_and_parent_component_spellings()
    -> Result<(), String> {
        let resolved = Path::new("/resolved/cache");
        for base in [
            Path::new("relative/cache"),
            Path::new("/original/sub/../cache"),
        ] {
            let dir = base
                .join("repo-file-facts")
                .join(super::super::FILE_FACT_CACHE_SCHEMA_VERSION);
            assert_eq!(
                relocated(&dir, base, resolved)?,
                resolved
                    .join("repo-file-facts")
                    .join(super::super::FILE_FACT_CACHE_SCHEMA_VERSION)
            );
        }
        Ok(())
    }
}
