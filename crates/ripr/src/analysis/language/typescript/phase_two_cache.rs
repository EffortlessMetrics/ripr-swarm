//! Process-lifetime cache for TypeScript phase-2 parse facts (#10051).
//!
//! Phase 2 used to re-parse each changed file three separate times after
//! phase 1 already parsed it twice (owner extraction + re-export facts):
//! `detect_owner_extraction_gap`, `changed_enum_member_lines`, and
//! `ambient_declaration_lines` each ran their own full oxc parse (~2.0 s
//! total on a 5 MB saved file — see #10044's measured phase breakdown).
//!
//! All three consumers read pure functions of `(relative path, source
//! bytes)`: no filesystem or cross-file access, per-diff `changed_lines`
//! filtering deferred to call time. This cache stores the one combined
//! [`PhaseTwoFacts`] extraction per content, keyed by the same
//! normalized-path + length + 128-bit fingerprint identity as the
//! owner-extraction cache (#10044), so a stale entry can never be served
//! after an edit: any byte change misses and re-extracts.
//!
//! Entries are bounded by count and approximate retained bytes; exceeding
//! either bound clears the whole cache, which is correctness-neutral (it
//! only costs re-extraction). Deliberately NOT scoped per run: cross-run
//! reuse on one thread is the entire point for the LSP, which runs an
//! analysis on every `did_save`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use super::extraction_cache::{CacheKey, source_fingerprint};
use super::owners::{PhaseTwoFacts, extract_phase_two_facts};

/// Entry-count bound for the process-lifetime cache. Clearing on overflow
/// is correctness-neutral, so the bound only trades re-extraction work.
const MAX_ENTRIES: usize = 2048;

/// Approximate retained-bytes bound (fact vectors plus identities).
const MAX_APPROX_BYTES: usize = 256 * 1024 * 1024;

struct PhaseTwoFactsCache {
    entries: HashMap<CacheKey, PhaseTwoFacts>,
    approx_bytes: usize,
    max_entries: usize,
    max_approx_bytes: usize,
}

impl PhaseTwoFactsCache {
    /// Bounds a fresh cache; `Default` installs the production constants.
    fn with_bounds(max_entries: usize, max_approx_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            approx_bytes: 0,
            max_entries,
            max_approx_bytes,
        }
    }

    fn entry_bytes(key: &CacheKey, facts: &PhaseTwoFacts) -> usize {
        let fact_bytes = facts
            .gap_shapes
            .len()
            .saturating_mul(std::mem::size_of::<super::owners::GapShape>())
            .saturating_add(
                facts
                    .enum_members
                    .len()
                    .saturating_mul(std::mem::size_of::<super::owners::EnumMember>()),
            )
            .saturating_add(
                facts
                    .ambient_ranges
                    .len()
                    .saturating_mul(std::mem::size_of::<(usize, usize)>()),
            )
            .saturating_add(
                facts
                    .enum_members
                    .iter()
                    .map(|member| member.identity.len())
                    .sum::<usize>(),
            );
        key.source_len
            .saturating_add(key.path.len())
            .saturating_add(fact_bytes)
    }

    fn insert(&mut self, key: CacheKey, facts: PhaseTwoFacts) {
        let entry_bytes = Self::entry_bytes(&key, &facts);
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
        self.entries.insert(key, facts);
    }
}

impl Default for PhaseTwoFactsCache {
    fn default() -> Self {
        Self::with_bounds(MAX_ENTRIES, MAX_APPROX_BYTES)
    }
}

thread_local! {
    static PHASE_TWO_FACTS_CACHE: RefCell<Option<PhaseTwoFactsCache>> =
        const { RefCell::new(None) };
}

/// Phase-2 facts for `(path, source)`, served from the process-lifetime
/// cache when this thread extracted the same content before. A miss
/// extracts and stores; reentrancy or a poisoned cell degrades to a plain
/// uncached extraction.
pub(crate) fn phase_two_facts_cached(relative: &Path, source: &str) -> Option<PhaseTwoFacts> {
    let path = super::paths::normalized_path(relative);
    let key = CacheKey {
        fingerprint: source_fingerprint(&path, source),
        source_len: source.len(),
        path,
    };
    let cached = PHASE_TWO_FACTS_CACHE.with(|cache| {
        cache.try_borrow_mut().ok().and_then(|mut cache| {
            cache
                .as_mut()
                .and_then(|cache| cache.entries.get(&key).cloned())
        })
    });
    if let Some(facts) = cached {
        return Some(facts);
    }
    let facts = extract_phase_two_facts(relative, source)?;
    PHASE_TWO_FACTS_CACHE.with(|cache| {
        if let Ok(mut cache) = cache.try_borrow_mut() {
            let cache = cache.get_or_insert_with(PhaseTwoFactsCache::default);
            cache.insert(key, facts.clone());
        }
    });
    Some(facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENUM_SOURCE: &str = "export enum Plan {\n    Basic = 1,\n    Pro = 2,\n}\n";
    const AMBIENT_SOURCE: &str =
        "declare function setup(): void;\nexport function run(): number {\n    return 1;\n}\n";

    #[test]
    fn identical_content_hits_the_cache() {
        let first = phase_two_facts_cached(Path::new("src/a.ts"), ENUM_SOURCE);
        assert!(first.is_some(), "fixture must parse");
        let Some(first) = first else {
            return;
        };
        assert!(!first.enum_members.is_empty(), "fixture must yield members");
        let second = phase_two_facts_cached(Path::new("src/a.ts"), ENUM_SOURCE);
        assert_eq!(second, Some(first));
    }

    #[test]
    fn edited_source_misses_and_reextracts() {
        let mut edited = String::from(ENUM_SOURCE);
        edited.push_str("export enum Extra {\n    X = 9,\n}\n");
        let before = phase_two_facts_cached(Path::new("src/a.ts"), ENUM_SOURCE);
        let after = phase_two_facts_cached(Path::new("src/a.ts"), &edited);
        assert!(before.is_some() && after.is_some(), "fixtures must parse");
        let (Some(before), Some(after)) = (before, after) else {
            return;
        };
        assert!(
            after.enum_members.len() > before.enum_members.len(),
            "an edit must re-extract, not serve stale facts"
        );
    }

    #[test]
    fn same_content_under_different_paths_are_distinct_entries() {
        let _ = phase_two_facts_cached(Path::new("src/a.ts"), AMBIENT_SOURCE);
        let _ = phase_two_facts_cached(Path::new("src/b.ts"), AMBIENT_SOURCE);
        PHASE_TWO_FACTS_CACHE.with(|cache| {
            let cache = cache.borrow();
            let Some(cache) = cache.as_ref() else {
                return;
            };
            assert_eq!(
                cache.entries.len(),
                2,
                "different paths must not share a facts entry"
            );
        });
    }

    #[test]
    fn entry_overflow_clears_and_starts_over() {
        let mut cache = PhaseTwoFactsCache::with_bounds(2, usize::MAX);
        for index in 0..3 {
            let path = format!("src/f{index}.ts");
            let key = CacheKey {
                path: path.clone(),
                source_len: AMBIENT_SOURCE.len(),
                fingerprint: source_fingerprint(&path, AMBIENT_SOURCE),
            };
            cache.insert(key, PhaseTwoFacts::default());
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
        let mut cache = PhaseTwoFactsCache::with_bounds(usize::MAX, 8);
        let key = |path: &str| CacheKey {
            path: path.to_string(),
            source_len: AMBIENT_SOURCE.len(),
            fingerprint: source_fingerprint(path, AMBIENT_SOURCE),
        };
        cache.insert(key("src/a.ts"), PhaseTwoFacts::default());
        assert_eq!(cache.entries.len(), 1);
        cache.insert(key("src/b.ts"), PhaseTwoFacts::default());
        assert_eq!(
            cache.entries.len(),
            1,
            "exceeding the byte bound clears, then keeps the newest"
        );
        assert!(cache.entries.contains_key(&key("src/b.ts")));
    }
}
