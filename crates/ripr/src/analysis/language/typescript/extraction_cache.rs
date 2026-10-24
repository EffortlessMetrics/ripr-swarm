//! Process-lifetime cache for TypeScript owner extraction (#10044).
//!
//! `extract_owners` is a pure function of `(relative path, source bytes)`:
//! one oxc parse plus per-statement owner walks, with no filesystem or
//! cross-file reads (owners.rs). The LSP runs an analysis on every
//! `did_save`, and before this cache each run re-parsed every workspace
//! source file, so `publishDiagnostics` latency scaled with workspace size
//! instead of diff size (measured: 8.3 s for an unchanged re-save on an
//! 8.8 MB tree).
//!
//! The cache key carries the normalized relative path, the source length,
//! and a 128-bit content fingerprint, so a stale entry can never be served
//! after an edit: any byte change misses and re-extracts. Only successful
//! extractions of files that passed the capped-read gate are ever cached
//! (the call site only reaches the cache for sources already in the
//! `source_cache`), and the per-file caps are deterministic functions of
//! the same bytes, so caching cannot bypass a read limit.
//!
//! Entries are bounded by count and approximate retained bytes; exceeding
//! either bound clears the whole cache, which is correctness-neutral (it
//! only costs re-extraction).
//!
//! Unlike [`DirectoryModuleCacheScope`] this cache is deliberately NOT
//! scoped per run: cross-run reuse on one thread is the entire point for
//! the LSP. Test extraction is NOT routed through here because
//! `promote_committed_snapshots` reads `__snapshots__` files — test
//! extraction is not a pure function of the test file's content.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hasher;
use std::path::Path;

use super::owners::extract_owners;
use super::paths::normalized_path;
use super::types::TypeScriptOwner;

/// Entry-count bound for the process-lifetime cache. Clearing on overflow
/// is correctness-neutral, so the bound only trades re-extraction work.
const MAX_ENTRIES: usize = 2048;

/// Approximate retained-bytes bound (sources plus one owner vec per entry).
const MAX_APPROX_BYTES: usize = 256 * 1024 * 1024;

/// Identity of one extraction: where the source lives, how long it is, and
/// a fingerprint of its bytes. Any edit changes the fingerprint and/or the
/// length, so a lookup can never serve owners for stale content.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    path: String,
    source_len: usize,
    fingerprint: (u64, u64),
}

/// Two independent 64-bit hashes over different domain separators, composed
/// into a 128-bit fingerprint. `DefaultHasher::new` is keyed deterministically
/// within a process, which is all the cache needs.
fn source_fingerprint(path: &str, source: &str) -> (u64, u64) {
    let mut first = std::collections::hash_map::DefaultHasher::new();
    first.write_u8(0xA7);
    first.write(path.as_bytes());
    first.write_u8(0x00);
    first.write(source.as_bytes());
    let mut second = std::collections::hash_map::DefaultHasher::new();
    second.write_u8(0x5C);
    second.write(source.as_bytes());
    second.write_u8(0x00);
    second.write(path.as_bytes());
    (first.finish(), second.finish())
}

struct OwnerExtractionCache {
    entries: HashMap<CacheKey, Vec<TypeScriptOwner>>,
    approx_bytes: usize,
    max_entries: usize,
    max_approx_bytes: usize,
}

impl OwnerExtractionCache {
    /// Bounds a fresh cache; `Default` installs the production constants.
    fn with_bounds(max_entries: usize, max_approx_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            approx_bytes: 0,
            max_entries,
            max_approx_bytes,
        }
    }

    fn insert(&mut self, key: CacheKey, owners: &[TypeScriptOwner]) {
        let entry_bytes = key
            .source_len
            .saturating_add(key.path.len())
            .saturating_add(owners.len().saturating_mul(std::mem::size_of::<TypeScriptOwner>()));
        if self.entries.len() >= self.max_entries
            || self.approx_bytes.saturating_add(entry_bytes) > self.max_approx_bytes
        {
            // Correctness-neutral: drop everything and start over.
            self.entries.clear();
            self.approx_bytes = 0;
        }
        // Replacing an existing key double-counts its bytes; the bound is
        // approximate by contract, so the drift is acceptable.
        self.approx_bytes = self.approx_bytes.saturating_add(entry_bytes);
        self.entries.insert(key, owners.to_vec());
    }
}

impl Default for OwnerExtractionCache {
    fn default() -> Self {
        Self::with_bounds(MAX_ENTRIES, MAX_APPROX_BYTES)
    }
}

thread_local! {
    static OWNER_EXTRACTION_CACHE: RefCell<Option<OwnerExtractionCache>> =
        const { RefCell::new(None) };
}

/// Owners for `(relative, source)`, served from the process-lifetime cache
/// when this thread extracted the same content before. A miss extracts and
/// stores; reentrancy or a poisoned cell degrades to plain extraction.
pub(crate) fn extract_owners_cached(relative: &Path, source: &str) -> Vec<TypeScriptOwner> {
    let path = normalized_path(relative);
    let key = CacheKey {
        fingerprint: source_fingerprint(&path, source),
        source_len: source.len(),
        path,
    };
    let cached = OWNER_EXTRACTION_CACHE.with(|cache| {
        cache
            .try_borrow_mut()
            .ok()
            .and_then(|mut cache| cache.as_mut().and_then(|cache| cache.entries.get(&key).cloned()))
    });
    if let Some(owners) = cached {
        return owners;
    }
    let owners = extract_owners(relative, source);
    OWNER_EXTRACTION_CACHE.with(|cache| {
        if let Ok(mut cache) = cache.try_borrow_mut() {
            let cache = cache.get_or_insert_with(OwnerExtractionCache::default);
            cache.insert(key, &owners);
        }
    });
    owners
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_for(path: &str, source: &str) -> CacheKey {
        CacheKey {
            path: path.to_string(),
            source_len: source.len(),
            fingerprint: source_fingerprint(path, source),
        }
    }

    #[test]
    fn identical_content_hits_the_cache() {
        let source = "export function alpha(): number {\n    return 1;\n}\n";
        let first = extract_owners_cached(Path::new("src/a.ts"), source);
        assert!(!first.is_empty(), "fixture must yield owners");
        let second = extract_owners_cached(Path::new("src/a.ts"), source);
        assert_eq!(first, second);
    }

    #[test]
    fn edited_source_misses_and_reextracts() {
        let one = "export function alpha(): number {\n    return 1;\n}\n";
        let two = "export function alpha(): number {\n    return 1;\n}\nexport function beta(): number {\n    return 2;\n}\n";
        let before = extract_owners_cached(Path::new("src/a.ts"), one);
        let after = extract_owners_cached(Path::new("src/a.ts"), two);
        assert!(
            after.len() > before.len(),
            "an edit must re-extract, not serve stale owners"
        );
    }

    #[test]
    fn same_content_under_different_paths_are_distinct_entries() {
        let source = "export function alpha(): number {\n    return 1;\n}\n";
        let _ = extract_owners_cached(Path::new("src/a.ts"), source);
        let _ = extract_owners_cached(Path::new("src/b.ts"), source);
        OWNER_EXTRACTION_CACHE.with(|cache| {
            let cache = cache.borrow();
            let Some(cache) = cache.as_ref() else {
                return;
            };
            assert_eq!(
                cache.entries.len(),
                2,
                "different paths must not share an owner entry"
            );
        });
    }

    #[test]
    fn entry_overflow_clears_and_starts_over() {
        let mut cache = OwnerExtractionCache::with_bounds(2, usize::MAX);
        let source = "export function alpha(): number {\n    return 1;\n}\n";
        for index in 0..3 {
            let key = key_for(&format!("src/f{index}.ts"), source);
            cache.insert(key, &[]);
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
        let mut cache = OwnerExtractionCache::with_bounds(usize::MAX, 8);
        let key = key_for("src/a.ts", "export function alpha(): number {\n    return 1;\n}\n");
        cache.insert(key.clone(), &[]);
        assert_eq!(cache.entries.len(), 1);
        cache.insert(key_for("src/b.ts", "export function beta(): number {\n    return 2;\n}\n"), &[]);
        assert_eq!(
            cache.entries.len(),
            1,
            "exceeding the byte bound clears, then keeps the newest"
        );
        assert!(cache.entries.contains_key(&key_for("src/b.ts", "export function beta(): number {\n    return 2;\n}\n")));
    }
}
