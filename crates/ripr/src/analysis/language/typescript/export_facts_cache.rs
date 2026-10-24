//! Process-lifetime cache for TypeScript per-file export facts (#10063).
//!
//! `ReExportIndex::build` used to run a full oxc parse of EVERY non-test
//! workspace source on every analysis run, only to collect per-file export
//! facts (named re-exports, star edges, local export names, enum bindings).
//! Measured on the 8.8 MB bigfile-probe fixture this index build cost ~1.4 s
//! per save — the largest reducible chunk after the owner-extraction
//! (#10049) and phase-2 (#10051) caches.
//!
//! Unlike those caches the facts here are NOT a function of one file's
//! bytes alone: `module_export_facts` resolves import specifiers through the
//! workspace root and the tsconfig alias map. The key therefore carries,
//! alongside the normalized path, source length, and content fingerprint, a
//! digest of the workspace root and the full `TsAliasMap` (see
//! [`TsAliasMap::cache_digest`]). Any change to the file set, the tsconfig,
//! or a file's bytes misses and re-parses that file only — unchanged files
//! reuse their facts, so a one-file edit re-parses one file instead of the
//! workspace.
//!
//! Entries are bounded by count and approximate retained bytes; exceeding
//! either bound clears the whole cache, which is correctness-neutral (it
//! only costs re-parsing). Deliberately NOT scoped per run: cross-run reuse
//! on one thread is the entire point for the LSP. Files that fail the
//! nesting budget or whose worker cannot run are NOT cached (the caller
//! skips them exactly as before); parse errors cache the default (empty)
//! facts, matching the previous contract.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use oxc_parser::Parser;

use super::extraction_cache::CacheKey;
use super::parse::parse_on_worker;
use super::paths::normalized_path;
use super::related_tests::ModuleExportFacts;
use super::source_type_for;
use super::tsconfig::TsAliasMap;

/// Entry-count bound for the process-lifetime cache. Clearing on overflow
/// is correctness-neutral, so the bound only trades re-parsing work.
const MAX_ENTRIES: usize = 2048;

/// Approximate retained-bytes bound (facts vectors plus strings).
const MAX_APPROX_BYTES: usize = 256 * 1024 * 1024;

/// Two independent 64-bit hashes over different domain separators, composed
/// into a 128-bit fingerprint of every input the facts depend on: workspace
/// root, alias-map digest, path, and source bytes.
fn facts_fingerprint(
    workspace_root: &Path,
    alias_digest: (u64, u64),
    path: &str,
    source: &str,
) -> (u64, u64) {
    let mut first = std::collections::hash_map::DefaultHasher::new();
    first.write_u8(0xA7);
    workspace_root.hash(&mut first);
    first.write_u64(alias_digest.0);
    first.write_u64(alias_digest.1);
    first.write_u8(0x00);
    first.write(path.as_bytes());
    first.write_u8(0x00);
    first.write(source.as_bytes());
    let mut second = std::collections::hash_map::DefaultHasher::new();
    second.write_u8(0x5C);
    second.write(source.as_bytes());
    second.write_u8(0x00);
    second.write(path.as_bytes());
    second.write_u8(0x00);
    second.write_u64(alias_digest.1);
    second.write_u64(alias_digest.0);
    workspace_root.hash(&mut second);
    (first.finish(), second.finish())
}

struct ExportFactsCache {
    entries: HashMap<CacheKey, ModuleExportFacts>,
    approx_bytes: usize,
    max_entries: usize,
    max_approx_bytes: usize,
}

impl ExportFactsCache {
    /// Bounds a fresh cache; `Default` installs the production constants.
    fn with_bounds(max_entries: usize, max_approx_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            approx_bytes: 0,
            max_entries,
            max_approx_bytes,
        }
    }

    fn entry_bytes(key: &CacheKey, facts: &ModuleExportFacts) -> usize {
        key.source_len
            .saturating_add(key.path.len())
            .saturating_add(facts.approx_bytes())
    }

    fn insert(&mut self, key: CacheKey, facts: &ModuleExportFacts) {
        let entry_bytes = Self::entry_bytes(&key, facts);
        // Replacing a key swaps its retained bytes exactly; capture the old
        // entry's bytes before the bounds check because an overflow clear
        // drops them together with the map.
        let replaced_bytes = self
            .entries
            .get(&key)
            .map(|old| Self::entry_bytes(&key, old));
        if self.entries.len() >= self.max_entries
            || self.approx_bytes.saturating_add(entry_bytes) > self.max_approx_bytes
        {
            // Correctness-neutral: drop everything and start over.
            self.entries.clear();
            self.approx_bytes = 0;
            self.approx_bytes = self.approx_bytes.saturating_add(entry_bytes);
            self.entries.insert(key, facts.clone());
            return;
        }
        self.entries.insert(key, facts.clone());
        self.approx_bytes = self
            .approx_bytes
            .saturating_add(entry_bytes)
            .saturating_sub(replaced_bytes.unwrap_or(0));
    }
}

impl Default for ExportFactsCache {
    fn default() -> Self {
        Self::with_bounds(MAX_ENTRIES, MAX_APPROX_BYTES)
    }
}

thread_local! {
    static EXPORT_FACTS_CACHE: RefCell<Option<ExportFactsCache>> =
        const { RefCell::new(None) };
}

/// Export facts for `(path, source)` under `(workspace_root, alias_map)`,
/// served from the process-lifetime cache when this thread extracted the
/// same content under the same resolution context before. A miss parses on
/// the guarded large-stack worker (#4101) and stores; worker failure
/// degrades to `None` (uncached, skipped by the caller exactly as before).
pub(crate) fn export_facts_cached(
    relative: &Path,
    source: &str,
    workspace_root: &Path,
    alias_map: Option<&TsAliasMap>,
) -> Option<ModuleExportFacts> {
    let path = normalized_path(relative);
    let alias_digest = alias_map.map_or((0, 0), TsAliasMap::cache_digest);
    let key = CacheKey {
        fingerprint: facts_fingerprint(workspace_root, alias_digest, &path, source),
        source_len: source.len(),
        path,
    };
    let cached = EXPORT_FACTS_CACHE.with(|cache| {
        cache.try_borrow_mut().ok().and_then(|mut cache| {
            cache
                .as_mut()
                .and_then(|cache| cache.entries.get(&key).cloned())
        })
    });
    if let Some(facts) = cached {
        return Some(facts);
    }
    // Parse on the guarded large-stack worker (#4101): the recursive
    // oxc parser must not run on the caller's stack, and an over-budget
    // source is skipped exactly like a parse error.
    let worker_root = workspace_root.to_path_buf();
    let worker_alias = alias_map.cloned();
    let facts = parse_on_worker(relative, source, move |relative, source, allocator| {
        let ret = Parser::new(allocator, source, source_type_for(relative)).parse();
        if !ret.errors.is_empty() {
            return ModuleExportFacts::default();
        }
        super::related_tests::module_export_facts(
            relative,
            &ret.program.body,
            source,
            worker_alias.as_ref(),
            worker_root.as_path(),
        )
    })
    .ok()?;
    EXPORT_FACTS_CACHE.with(|cache| {
        if let Ok(mut cache) = cache.try_borrow_mut() {
            let cache = cache.get_or_insert_with(ExportFactsCache::default);
            cache.insert(key, &facts);
        }
    });
    Some(facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BARREL_SOURCE: &str = "export { helper } from './helper';\nexport * from './other';\n";

    #[test]
    fn identical_content_hits_the_cache() {
        let root = Path::new(".");
        let first = export_facts_cached(Path::new("src/barrel.ts"), BARREL_SOURCE, root, None);
        let Some(first) = first else {
            return;
        };
        assert!(!first.entries.is_empty(), "fixture must yield entries");
        let second = export_facts_cached(Path::new("src/barrel.ts"), BARREL_SOURCE, root, None);
        assert_eq!(second, Some(first));
    }

    #[test]
    fn edited_source_misses_and_reextracts() {
        let root = Path::new(".");
        let edited = format!("{BARREL_SOURCE}export const extra = 1;\n");
        let before = export_facts_cached(Path::new("src/barrel.ts"), BARREL_SOURCE, root, None);
        let after = export_facts_cached(Path::new("src/barrel.ts"), &edited, root, None);
        assert!(before.is_some() && after.is_some());
        let (Some(before), Some(after)) = (before, after) else {
            return;
        };
        assert_ne!(
            before.local_exports, after.local_exports,
            "an edit must re-extract, not serve stale facts"
        );
    }

    #[test]
    fn same_content_under_different_roots_are_distinct_entries() {
        let source = BARREL_SOURCE;
        let _ = export_facts_cached(Path::new("src/barrel.ts"), source, Path::new("a"), None);
        let _ = export_facts_cached(Path::new("src/barrel.ts"), source, Path::new("b"), None);
        EXPORT_FACTS_CACHE.with(|cache| {
            let cache = cache.borrow();
            let Some(cache) = cache.as_ref() else {
                return;
            };
            assert_eq!(
                cache.entries.len(),
                2,
                "different workspace roots must not share a facts entry"
            );
        });
    }

    #[test]
    fn entry_overflow_clears_and_starts_over() {
        let mut cache = ExportFactsCache::with_bounds(2, usize::MAX);
        for index in 0..3 {
            let key = CacheKey {
                path: format!("src/f{index}.ts"),
                source_len: BARREL_SOURCE.len(),
                fingerprint: facts_fingerprint(
                    Path::new("."),
                    (0, 0),
                    &format!("src/f{index}.ts"),
                    BARREL_SOURCE,
                ),
            };
            cache.insert(key, &ModuleExportFacts::default());
        }
        assert_eq!(
            cache.entries.len(),
            1,
            "inserting past the entry bound clears, then keeps the newest"
        );
        assert!(cache.approx_bytes > 0);
    }

    #[test]
    fn byte_overflow_clears_and_starts_over() {
        let mut cache = ExportFactsCache::with_bounds(usize::MAX, 8);
        let key = |path: &str| CacheKey {
            path: path.to_string(),
            source_len: BARREL_SOURCE.len(),
            fingerprint: facts_fingerprint(Path::new("."), (0, 0), path, BARREL_SOURCE),
        };
        cache.insert(key("src/a.ts"), &ModuleExportFacts::default());
        assert_eq!(cache.entries.len(), 1);
        cache.insert(key("src/b.ts"), &ModuleExportFacts::default());
        assert_eq!(
            cache.entries.len(),
            1,
            "exceeding the byte bound clears, then keeps the newest"
        );
        assert!(cache.entries.contains_key(&key("src/b.ts")));
    }

    #[test]
    fn fingerprint_separates_alias_and_root_and_content() {
        let source = BARREL_SOURCE;
        let base = facts_fingerprint(Path::new("."), (0, 0), "src/a.ts", source);
        assert_ne!(
            base,
            facts_fingerprint(Path::new("."), (1, 0), "src/a.ts", source),
            "alias-map digest must be part of the key"
        );
        assert_ne!(
            base,
            facts_fingerprint(Path::new("other"), (0, 0), "src/a.ts", source),
            "workspace root must be part of the key"
        );
        assert_ne!(
            base,
            facts_fingerprint(Path::new("."), (0, 0), "src/a.ts", "export const x = 1;\n"),
            "content must be part of the key"
        );
    }
}
