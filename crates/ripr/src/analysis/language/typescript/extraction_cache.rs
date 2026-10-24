//! Process-lifetime cache for combined TypeScript per-file facts (#10071).
//!
//! One analysis run used to parse a changed source file up to four times:
//! the phase-1 parse-error check, owner extraction (#10044), per-file export
//! facts for the re-export index (#10063), and the phase-2 parse facts
//! (#10051). `FileFacts` derives all four outputs from ONE oxc parse per
//! `(path, content, workspace_root, alias_map)`, measured on a 5 MB changed
//! file as ~4 parses -> 1 per save.
//!
//! Owner extraction and the phase-2 facts are pure functions of `(relative
//! path, source bytes)`; the export facts resolve import specifiers through
//! the workspace root and the tsconfig alias map. The combined key
//! therefore carries, alongside the normalized path, source length, and
//! content fingerprint, a digest of the workspace root and the full
//! `TsAliasMap` (see [`TsAliasMap::cache_digest`]), exactly like the old
//! export-facts key. Any change to the file set, the tsconfig, or a file's
//! bytes misses and re-parses that file only — unchanged files reuse their
//! facts, so a one-file edit re-parses one file instead of the workspace.
//!
//! The cache key carries the normalized relative path, the source length,
//! and a 128-bit content fingerprint, so a stale entry can never be served
//! after an edit: any byte change misses and re-extracts. Only successful
//! extractions of files that passed the capped-read gate are ever cached
//! (the call site only reaches the cache for sources already in the
//! `source_cache`), and the per-file caps are deterministic functions of
//! the same bytes, so caching cannot bypass a read limit. Files that fail
//! the nesting budget or whose worker cannot run are NOT cached (the caller
//! skips them exactly as before); parse errors cache the default (empty)
//! facts with the parse-error reason, matching each consumer's previous
//! parse-error contract.
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
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::rc::Rc;

use oxc_parser::Parser;

use super::owners::{PhaseTwoFacts, owners_from_program, phase_two_facts_from_program};
use super::parse::parse_on_worker;
use super::paths::normalized_path;
use super::related_tests::ModuleExportFacts;
use super::source_type_for;
use super::tsconfig::TsAliasMap;
use super::types::TypeScriptOwner;

/// Entry-count bound for the process-lifetime cache. Clearing on overflow
/// is correctness-neutral, so the bound only trades re-extraction work.
const MAX_ENTRIES: usize = 2048;

/// Approximate retained-bytes bound (sources plus one facts struct per
/// entry: owner vec, export-facts vectors, and phase-2 fact vectors).
const MAX_APPROX_BYTES: usize = 256 * 1024 * 1024;

/// Identity of one extraction: where the source lives, how long it is, and
/// a fingerprint of its bytes. Any edit changes the fingerprint and/or the
/// length, so a lookup can never serve facts for stale content. The
/// combined facts cache (#10071) keys every per-file computation —
/// owners (#10044), export facts (#10063), and phase-2 facts (#10051) —
/// by one identical content-plus-resolution identity.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    pub(crate) path: String,
    pub(crate) source_len: usize,
    pub(crate) fingerprint: (u64, u64),
}

/// Two independent 64-bit hashes over different domain separators, composed
/// into a 128-bit fingerprint of every input the facts depend on: workspace
/// root, alias-map digest, path, and source bytes. Moved unchanged from the
/// deleted export-facts cache (#10063); now the fingerprint of the combined
/// facts cache (#10071). `DefaultHasher::new` is keyed deterministically
/// within a process, which is all the cache needs.
pub(crate) fn facts_fingerprint(
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

/// All per-file facts derived from one oxc parse (#10071): parse status,
/// owners, export facts, and phase-2 facts. One parse per (path, content,
/// root, alias-map) replaces the four separate parses per save.
#[derive(Debug, Clone)]
pub(crate) struct FileFacts {
    /// `None` when the source parsed cleanly; otherwise the same
    /// `"N parser error(s): <first message>"` string `parse_error_reason`
    /// produces. Parse-error files cache default facts (empty owners,
    /// default export facts, default phase-2 facts) — matching each
    /// consumer's previous parse-error contract.
    pub(crate) parse_error: Option<String>,
    pub(crate) owners: Vec<TypeScriptOwner>,
    pub(crate) export_facts: super::related_tests::ModuleExportFacts,
    pub(crate) phase_two: super::owners::PhaseTwoFacts,
}

struct FileFactsCache {
    entries: HashMap<CacheKey, Rc<FileFacts>>,
    approx_bytes: usize,
    max_entries: usize,
    max_approx_bytes: usize,
}

impl FileFactsCache {
    /// Bounds a fresh cache; `Default` installs the production constants.
    fn with_bounds(max_entries: usize, max_approx_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            approx_bytes: 0,
            max_entries,
            max_approx_bytes,
        }
    }

    fn entry_bytes(key: &CacheKey, facts: &FileFacts) -> usize {
        // Phase-2 portion mirrors the deleted phase-two cache (#10051):
        // gap-shape/enum-member/ambient-range sizes plus enum identity
        // string lengths.
        let phase_two_bytes = facts
            .phase_two
            .gap_shapes
            .len()
            .saturating_mul(std::mem::size_of::<super::owners::GapShape>())
            .saturating_add(
                facts
                    .phase_two
                    .enum_members
                    .len()
                    .saturating_mul(std::mem::size_of::<super::owners::EnumMember>()),
            )
            .saturating_add(
                facts
                    .phase_two
                    .ambient_ranges
                    .len()
                    .saturating_mul(std::mem::size_of::<(usize, usize)>()),
            )
            .saturating_add(
                facts
                    .phase_two
                    .enum_members
                    .iter()
                    .map(|member| member.identity.len())
                    .sum::<usize>(),
            );
        key.source_len
            .saturating_add(key.path.len())
            .saturating_add(
                facts
                    .owners
                    .len()
                    .saturating_mul(std::mem::size_of::<TypeScriptOwner>()),
            )
            .saturating_add(facts.export_facts.approx_bytes())
            .saturating_add(phase_two_bytes)
    }

    fn insert(&mut self, key: CacheKey, facts: Rc<FileFacts>) {
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

impl Default for FileFactsCache {
    fn default() -> Self {
        Self::with_bounds(MAX_ENTRIES, MAX_APPROX_BYTES)
    }
}

thread_local! {
    static FILE_FACTS_CACHE: RefCell<Option<FileFactsCache>> =
        const { RefCell::new(None) };
}

/// Every per-file fact for `(path, source)` under `(workspace_root,
/// alias_map)`, served from the process-lifetime cache when this thread
/// derived the same facts before. A miss runs ONE parse on the guarded
/// large-stack worker (#4101) and stores the combined facts; a worker
/// refusal or failure is NOT cached and returns `Err` so the caller skips
/// the file exactly as before (#10071, replacing the #10044/#10051/#10063
/// caches). Reentrancy or a poisoned cell degrades to an uncached parse.
///
/// The result is `Rc`-wrapped: the cache is thread-local and single-owner
/// per lookup, so a hit is a cheap pointer clone instead of deep-cloning
/// the owners/export/phase-2 vectors for every workspace file on every
/// save (Gemini review on #10078).
pub(crate) fn file_facts_cached(
    relative: &Path,
    source: &str,
    workspace_root: &Path,
    alias_map: Option<&TsAliasMap>,
) -> Result<Rc<FileFacts>, String> {
    let path = normalized_path(relative);
    let alias_digest = alias_map.map_or((0, 0), TsAliasMap::cache_digest);
    let key = CacheKey {
        fingerprint: facts_fingerprint(workspace_root, alias_digest, &path, source),
        source_len: source.len(),
        path,
    };
    let cached = FILE_FACTS_CACHE.with(|cache| {
        cache.try_borrow_mut().ok().and_then(|mut cache| {
            cache
                .as_mut()
                .and_then(|cache| cache.entries.get(&key).map(Rc::clone))
        })
    });
    if let Some(facts) = cached {
        return Ok(facts);
    }
    // Parse on the guarded large-stack worker (#4101): the recursive
    // oxc parser must not run on the caller's stack, and an over-budget
    // source is refused exactly like a parse error. The single closure
    // fills every fact from this one parse.
    let worker_root = workspace_root.to_path_buf();
    let worker_alias = alias_map.cloned();
    let facts = Rc::new(parse_on_worker(
        relative,
        source,
        move |relative, source, allocator| {
            let ret = Parser::new(allocator, source, source_type_for(relative)).parse();
            let parse_error = if ret.errors.is_empty() {
                None
            } else {
                // Same disclosure string `parse_error_reason` produces, so the
                // parse-limit output is byte-identical to the previous
                // dedicated parse-error pass.
                Some(format!(
                    "{} parser error(s): {}",
                    ret.errors.len(),
                    ret.errors[0]
                ))
            };
            let mut facts = FileFacts {
                parse_error,
                owners: Vec::new(),
                export_facts: ModuleExportFacts::default(),
                phase_two: PhaseTwoFacts::default(),
            };
            if facts.parse_error.is_some() {
                // Parse-error files cache default facts, matching each
                // consumer's previous parse-error contract.
                return facts;
            }
            facts.owners = owners_from_program(&ret.program, relative, source);
            facts.export_facts = super::related_tests::module_export_facts(
                relative,
                &ret.program.body,
                source,
                worker_alias.as_ref(),
                worker_root.as_path(),
            );
            facts.phase_two = phase_two_facts_from_program(&ret.program, source);
            facts
        },
    )?);
    FILE_FACTS_CACHE.with(|cache| {
        if let Ok(mut cache) = cache.try_borrow_mut() {
            let cache = cache.get_or_insert_with(FileFactsCache::default);
            cache.insert(key, Rc::clone(&facts));
        }
    });
    Ok(facts)
}

/// Export facts for `(path, source)` under `(workspace_root, alias_map)`,
/// served from the combined process-lifetime facts cache (#10071, replacing
/// the #10063 export-facts cache). Worker refusal or failure degrades to
/// `None` (uncached, skipped by the caller exactly as before).
pub(crate) fn export_facts_cached(
    relative: &Path,
    source: &str,
    workspace_root: &Path,
    alias_map: Option<&TsAliasMap>,
) -> Option<ModuleExportFacts> {
    file_facts_cached(relative, source, workspace_root, alias_map)
        .map(|facts| facts.export_facts.clone())
        .ok()
}

/// Phase-2 facts for `(path, source)` under `(workspace_root, alias_map)`,
/// served from the combined process-lifetime facts cache (#10071, replacing
/// the #10051 phase-two cache). Worker refusal or failure degrades to
/// `None` (uncached, degrading every consumer to "no facts" exactly as
/// before).
pub(crate) fn phase_two_facts_cached(
    relative: &Path,
    source: &str,
    workspace_root: &Path,
    alias_map: Option<&TsAliasMap>,
) -> Option<PhaseTwoFacts> {
    file_facts_cached(relative, source, workspace_root, alias_map)
        .map(|facts| facts.phase_two.clone())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BARREL_SOURCE: &str = "export { helper } from './helper';\nexport * from './other';\n";
    const ENUM_SOURCE: &str = "export enum Plan {\n    Basic = 1,\n    Pro = 2,\n}\n";
    // One owner (the function) plus one runtime enum: covers every combined
    // fact in a single parse (#10071).
    const FUNCTION_AND_ENUM_SOURCE: &str = "export function alpha(): number {\n    return 1;\n}\nexport enum Plan {\n    Basic = 1,\n    Pro = 2,\n}\n";

    fn default_facts() -> FileFacts {
        FileFacts {
            parse_error: None,
            owners: Vec::new(),
            export_facts: ModuleExportFacts::default(),
            phase_two: PhaseTwoFacts::default(),
        }
    }

    fn key_for(path: &str, source: &str) -> CacheKey {
        CacheKey {
            path: path.to_string(),
            source_len: source.len(),
            fingerprint: facts_fingerprint(Path::new("."), (0, 0), path, source),
        }
    }

    #[test]
    fn identical_content_hits_the_cache() {
        let root = Path::new(".");
        let first = file_facts_cached(Path::new("src/a.ts"), FUNCTION_AND_ENUM_SOURCE, root, None);
        assert!(first.is_ok(), "fixture must parse");
        let Ok(first) = first else {
            return;
        };
        assert!(!first.owners.is_empty(), "fixture must yield owners");
        assert!(
            !first.phase_two.enum_members.is_empty(),
            "fixture must yield enum members"
        );
        let second = file_facts_cached(Path::new("src/a.ts"), FUNCTION_AND_ENUM_SOURCE, root, None);
        assert!(second.is_ok());
        let Ok(second) = second else {
            return;
        };
        assert_eq!(first.owners, second.owners);
        assert_eq!(first.export_facts, second.export_facts);
        assert_eq!(first.phase_two, second.phase_two);
        assert_eq!(first.parse_error, second.parse_error);
    }

    #[test]
    fn edited_source_misses_and_reextracts() {
        let root = Path::new(".");
        let edited = format!(
            "{FUNCTION_AND_ENUM_SOURCE}export function beta(): number {{\n    return 2;\n}}\n"
        );
        let before = file_facts_cached(Path::new("src/a.ts"), FUNCTION_AND_ENUM_SOURCE, root, None);
        let after = file_facts_cached(Path::new("src/a.ts"), &edited, root, None);
        assert!(before.is_ok() && after.is_ok(), "fixtures must parse");
        let (Ok(before), Ok(after)) = (before, after) else {
            return;
        };
        assert!(
            after.owners.len() > before.owners.len(),
            "an edit must re-extract, not serve stale owners"
        );
        assert!(
            after.phase_two.enum_members.len() >= before.phase_two.enum_members.len(),
            "an edit must re-extract, not serve stale facts"
        );
    }

    #[test]
    fn same_content_under_different_paths_are_distinct_entries() {
        let root = Path::new(".");
        let _ = file_facts_cached(Path::new("src/a.ts"), ENUM_SOURCE, root, None);
        let _ = file_facts_cached(Path::new("src/b.ts"), ENUM_SOURCE, root, None);
        FILE_FACTS_CACHE.with(|cache| {
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
    fn same_content_under_different_roots_are_distinct_entries() {
        let _ = file_facts_cached(
            Path::new("src/barrel.ts"),
            BARREL_SOURCE,
            Path::new("a"),
            None,
        );
        let _ = file_facts_cached(
            Path::new("src/barrel.ts"),
            BARREL_SOURCE,
            Path::new("b"),
            None,
        );
        FILE_FACTS_CACHE.with(|cache| {
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
    fn parse_error_content_caches_reason_and_default_facts() {
        let root = Path::new(".");
        let source = "const x = ;\n";
        let first = file_facts_cached(Path::new("src/broken.ts"), source, root, None);
        assert!(first.is_ok(), "parse errors still cache facts");
        let Ok(first) = first else {
            return;
        };
        assert!(
            first.parse_error.is_some(),
            "parse-error content must carry the reason"
        );
        let Some(reason) = first.parse_error.as_ref() else {
            return;
        };
        assert!(
            reason.contains("parser error(s):"),
            "reason mirrors parse_error_reason: {reason}"
        );
        assert!(first.owners.is_empty(), "default owners on parse error");
        assert_eq!(
            first.export_facts,
            ModuleExportFacts::default(),
            "default export facts on parse error"
        );
        assert_eq!(
            first.phase_two,
            PhaseTwoFacts::default(),
            "default phase-2 facts on parse error"
        );
        let second = file_facts_cached(Path::new("src/broken.ts"), source, root, None);
        assert!(second.is_ok());
        let Ok(second) = second else {
            return;
        };
        assert_eq!(second.parse_error, first.parse_error);
    }

    #[test]
    fn over_budget_source_is_not_cached_and_still_errs() {
        // Balanced at budget+1: the oxc worker would parse this cleanly, so
        // any `Err` can only come from the pre-parse nesting budget (#4101).
        let source = format!(
            "const over = {}v{};",
            "(".repeat(super::super::parse::PARSE_NESTING_BUDGET + 1),
            ")".repeat(super::super::parse::PARSE_NESTING_BUDGET + 1)
        );
        let root = Path::new(".");
        let first = file_facts_cached(Path::new("src/over.ts"), &source, root, None);
        let Err(reason) = first else {
            return;
        };
        assert!(
            reason.contains("expression_nesting_budget"),
            "typed budget refusal expected, got: {reason}"
        );
        let second = file_facts_cached(Path::new("src/over.ts"), &source, root, None);
        assert!(
            second.is_err(),
            "a refused source must not be served from the cache"
        );
        // The refusal left no entry behind.
        let path = normalized_path(Path::new("src/over.ts"));
        let key = CacheKey {
            fingerprint: facts_fingerprint(root, (0, 0), &path, &source),
            source_len: source.len(),
            path,
        };
        FILE_FACTS_CACHE.with(|cache| {
            let cache = cache.borrow();
            if let Some(cache) = cache.as_ref() {
                assert!(
                    !cache.entries.contains_key(&key),
                    "refused source must not be cached"
                );
            }
        });
    }

    #[test]
    fn owners_match_plain_extract_owners_for_clean_content() {
        let root = Path::new(".");
        let facts = file_facts_cached(Path::new("src/a.ts"), FUNCTION_AND_ENUM_SOURCE, root, None);
        assert!(facts.is_ok(), "fixture must parse");
        let Ok(facts) = facts else {
            return;
        };
        let expected =
            super::super::owners::extract_owners(Path::new("src/a.ts"), FUNCTION_AND_ENUM_SOURCE);
        assert!(!expected.is_empty(), "fixture must yield owners");
        assert_eq!(
            facts.owners, expected,
            "one-parse owners must equal the dedicated extraction (#10044)"
        );
    }

    #[test]
    fn export_facts_match_previous_barrel_contract() {
        let root = Path::new(".");
        let facts = file_facts_cached(Path::new("src/barrel.ts"), BARREL_SOURCE, root, None);
        assert!(facts.is_ok(), "fixture must parse");
        let Ok(facts) = facts else {
            return;
        };
        // Named re-export edge (`helper` -> `./helper`) and the star edge
        // (`./other`) must both survive the combined extraction (#10063).
        assert!(
            !facts.export_facts.entries.is_empty(),
            "named re-export must be recorded"
        );
        assert!(
            facts
                .export_facts
                .entries
                .iter()
                .any(|((_, name), (_, _))| name == "helper"),
            "named re-export of `helper` expected: {:?}",
            facts.export_facts.entries
        );
        assert!(
            facts
                .export_facts
                .star_edges
                .iter()
                .any(|(_, target)| target.contains("other")),
            "star edge to `./other` expected: {:?}",
            facts.export_facts.star_edges
        );
    }

    #[test]
    fn phase_two_enum_fixture_yields_enum_members() {
        let root = Path::new(".");
        let facts = file_facts_cached(Path::new("src/a.ts"), ENUM_SOURCE, root, None);
        assert!(facts.is_ok(), "fixture must parse");
        let Ok(facts) = facts else {
            return;
        };
        let identities: Vec<&str> = facts
            .phase_two
            .enum_members
            .iter()
            .map(|member| member.identity.as_str())
            .collect();
        assert!(
            identities.contains(&"Plan::Basic") && identities.contains(&"Plan::Pro"),
            "enum members expected, got: {identities:?}"
        );
    }

    #[test]
    fn entry_overflow_clears_and_starts_over() {
        let mut cache = FileFactsCache::with_bounds(2, usize::MAX);
        for index in 0..3 {
            let key = key_for(&format!("src/f{index}.ts"), ENUM_SOURCE);
            cache.insert(key, Rc::new(default_facts()));
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
        let mut cache = FileFactsCache::with_bounds(usize::MAX, 8);
        cache.insert(key_for("src/a.ts", ENUM_SOURCE), Rc::new(default_facts()));
        assert_eq!(cache.entries.len(), 1);
        cache.insert(key_for("src/b.ts", ENUM_SOURCE), Rc::new(default_facts()));
        assert_eq!(
            cache.entries.len(),
            1,
            "exceeding the byte bound clears, then keeps the newest"
        );
        assert!(
            cache
                .entries
                .contains_key(&key_for("src/b.ts", ENUM_SOURCE))
        );
    }

    #[test]
    fn fingerprint_separates_path_and_content() {
        let base = facts_fingerprint(Path::new("."), (0, 0), "src/a.ts", ENUM_SOURCE);
        assert_eq!(
            base,
            facts_fingerprint(Path::new("."), (0, 0), "src/a.ts", ENUM_SOURCE)
        );
        assert_ne!(
            base,
            facts_fingerprint(Path::new("."), (0, 0), "src/b.ts", ENUM_SOURCE),
            "different paths must not share a fingerprint"
        );
        assert_ne!(
            base,
            facts_fingerprint(Path::new("."), (0, 0), "src/a.ts", BARREL_SOURCE),
            "different content must not share a fingerprint"
        );
    }

    #[test]
    fn fingerprint_separates_alias_and_root_and_content() {
        let base = facts_fingerprint(Path::new("."), (0, 0), "src/a.ts", BARREL_SOURCE);
        assert_ne!(
            base,
            facts_fingerprint(Path::new("."), (1, 0), "src/a.ts", BARREL_SOURCE),
            "alias-map digest must be part of the key"
        );
        assert_ne!(
            base,
            facts_fingerprint(Path::new("other"), (0, 0), "src/a.ts", BARREL_SOURCE),
            "workspace root must be part of the key"
        );
        assert_ne!(
            base,
            facts_fingerprint(Path::new("."), (0, 0), "src/a.ts", "export const x = 1;\n"),
            "content must be part of the key"
        );
    }
}
