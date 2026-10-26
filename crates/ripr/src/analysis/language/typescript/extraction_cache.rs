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
//! the LSP. Test extraction is NOT routed through the combined facts
//! because `promote_committed_snapshots` reads `__snapshots__` files, so the
//! analysis of a test file is not a pure function of its content. The
//! extraction itself (`extract_tests_with_expansion`) is: it reads only
//! `(path, source)`. [`test_extraction_cached`] caches that pure step in a
//! separate cache keyed by path and content, and the caller runs snapshot
//! promotion on a clone of the cached tests every run, so a changed or new
//! snapshot file is still seen (#10017).

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

/// A 128-bit fingerprint of every input the facts depend on: workspace
/// root, alias-map digest, path, and source bytes. Moved unchanged from the
/// deleted export-facts cache (#10063); now the fingerprint of the combined
/// facts cache (#10071). `DefaultHasher::new` is keyed deterministically
/// within a process, which is all the cache needs.
///
/// One pass over the source bytes: the first lane hashes every input; the
/// second lane hashes the finished first lane plus the identity metadata
/// instead of re-reading the source (#10087 follow-up). The pair keeps the
/// (u64, u64) shape, both lanes stay deterministic per process, and a
/// process-lifetime cache capped at a few thousand entries sits far below
/// the birthday bound of even a single 64-bit lane.
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
    let first = first.finish();
    let mut second = std::collections::hash_map::DefaultHasher::new();
    second.write_u8(0x5C);
    second.write_u64(first);
    second.write_u64(source.len() as u64);
    second.write_u64(alias_digest.1);
    second.write_u64(alias_digest.0);
    second.write(path.as_bytes());
    second.write_u8(0x00);
    workspace_root.hash(&mut second);
    (first, second.finish())
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

/// The result of `extract_tests_with_expansion` for one test file.
pub(crate) type TestExtraction = (
    Vec<super::types::TypeScriptTest>,
    Option<super::types::TypeScriptConditionalCreditGap>,
    Option<super::suite_factory::SuiteFactoryExpansion>,
);

struct TestExtractionCache {
    entries: HashMap<CacheKey, Rc<TestExtraction>>,
    approx_bytes: usize,
    max_entries: usize,
    max_approx_bytes: usize,
}

/// Approximate heap bytes of a `Vec<String>`: the handles plus their text.
fn strings_bytes(strings: &[String]) -> usize {
    strings
        .iter()
        .map(|text| text.len().saturating_add(std::mem::size_of::<String>()))
        .sum()
}

/// Approximate retained bytes of one extracted test, without the file-wide
/// lists its tests share; `shared_bytes` counts each of those once.
fn test_bytes(test: &super::types::TypeScriptTest) -> usize {
    std::mem::size_of::<super::types::TypeScriptTest>()
        .saturating_add(test.name.len())
        .saturating_add(test.local_name.len())
        .saturating_add(test.body_text.len())
        .saturating_add(test.reach_body_text.as_ref().map_or(0, String::len))
        .saturating_add(test.file.as_os_str().len())
        .saturating_add(strings_bytes(&test.describe_names))
        .saturating_add(strings_bytes(&test.setup_hook_bodies))
        .saturating_add(test.assertions.len().saturating_mul(
            std::mem::size_of::<super::types::TypeScriptAssertion>().saturating_add(96),
        ))
}

/// Approximate retained bytes of the file-wide lists (imports, mocks,
/// helpers, scope bindings) the tests of one file share. Each distinct list is
/// counted once however many tests hold it.
fn shared_bytes(tests: &[super::types::TypeScriptTest]) -> usize {
    use super::types::{Shared, TypeScriptImport, TypeScriptScopeBinding};
    use std::collections::HashSet;

    // Elements own further strings; a flat 96 bytes each keeps the estimate
    // on the safe (large) side without walking their fields.
    fn elements<T: Clone>(list: &Shared<T>, seen: &mut HashSet<usize>, each: usize) -> usize {
        if list.is_empty() || !seen.insert(list.identity() as usize) {
            return 0;
        }
        list.len().saturating_mul(each.saturating_add(96))
    }
    let mut seen = HashSet::new();
    let mut total = 0usize;
    for test in tests {
        for list in [
            &test.mocks_in_file,
            &test.unstable_member_roots_in_file,
            &test.identity_helpers_in_file,
            &test.written_locals_in_file,
        ] {
            total = total.saturating_add(elements(list, &mut seen, std::mem::size_of::<String>()));
        }
        for list in [
            &test.default_member_only_locals_in_file,
            &test.mock_factories_in_file,
            &test.mock_spread_sources_in_file,
        ] {
            total =
                total.saturating_add(elements(list, &mut seen, 2 * std::mem::size_of::<String>()));
        }
        total = total.saturating_add(elements(
            &test.imports_in_file,
            &mut seen,
            std::mem::size_of::<TypeScriptImport>(),
        ));
        total = total.saturating_add(elements(
            &test.scope_bindings,
            &mut seen,
            std::mem::size_of::<TypeScriptScopeBinding>(),
        ));
    }
    total
}

impl TestExtractionCache {
    fn with_bounds(max_entries: usize, max_approx_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            approx_bytes: 0,
            max_entries,
            max_approx_bytes,
        }
    }

    fn entry_bytes(key: &CacheKey, extraction: &TestExtraction) -> usize {
        let tests_bytes: usize = extraction
            .0
            .iter()
            .map(test_bytes)
            .sum::<usize>()
            .saturating_add(shared_bytes(&extraction.0));
        let expansion_bytes = extraction
            .2
            .as_ref()
            .map_or(0, |expansion| expansion.source.len());
        key.source_len
            .saturating_add(key.path.len())
            .saturating_add(tests_bytes)
            .saturating_add(expansion_bytes)
    }

    /// Stores `extraction` unless it alone exceeds the byte bound; an entry
    /// that cannot fit is served uncached rather than retained past the bound.
    fn insert(&mut self, key: CacheKey, extraction: Rc<TestExtraction>) {
        let entry_bytes = Self::entry_bytes(&key, &extraction);
        if entry_bytes > self.max_approx_bytes {
            return;
        }
        if self.entries.len() >= self.max_entries
            || self.approx_bytes.saturating_add(entry_bytes) > self.max_approx_bytes
        {
            // Correctness-neutral: drop everything and start over.
            self.entries.clear();
            self.approx_bytes = 0;
        }
        self.approx_bytes = self.approx_bytes.saturating_add(entry_bytes);
        self.entries.insert(key, extraction);
    }
}

thread_local! {
    static TEST_EXTRACTION_CACHE: RefCell<Option<TestExtractionCache>> =
        const { RefCell::new(None) };
}

/// `extract_tests_with_expansion(relative, source)`, served from a
/// process-lifetime cache keyed by normalized path, source length and a
/// 128-bit content fingerprint (#10017). The extraction reads only its
/// arguments, so a byte change misses and re-extracts; callers must still
/// run filesystem-dependent steps (committed-snapshot promotion) on a clone
/// of the returned tests. A reentrant or poisoned cell degrades to an
/// uncached extraction. Test extraction costs about 4 s/MB in a debug build,
/// which an unchanged test file used to pay again on every LSP save.
pub(crate) fn test_extraction_cached(relative: &Path, source: &str) -> Rc<TestExtraction> {
    test_extraction_cached_with(relative, source, || {
        super::tests_extract::try_extract_tests_with_expansion(relative, source)
    })
}

/// [`test_extraction_cached`] with the extractor injected, so a test can
/// force the worker-failure path that cannot be provoked from outside.
fn test_extraction_cached_with(
    relative: &Path,
    source: &str,
    extract: impl FnOnce() -> Result<TestExtraction, String>,
) -> Rc<TestExtraction> {
    // Lossless path identity, not `normalized_path`: the extracted tests store
    // `relative` as their `file`, and a Unix filename may contain a literal
    // backslash that normalization would fold into a directory separator.
    let path = relative.to_string_lossy().into_owned();
    let key = CacheKey {
        fingerprint: facts_fingerprint(Path::new(""), (0, 0), &path, source),
        source_len: source.len(),
        path,
    };
    let cached = TEST_EXTRACTION_CACHE.with(|cache| {
        cache.try_borrow_mut().ok().and_then(|cache| {
            cache
                .as_ref()
                .and_then(|cache| cache.entries.get(&key).map(Rc::clone))
        })
    });
    if let Some(extraction) = cached {
        return extraction;
    }
    // A worker failure is environmental, not a function of the bytes: serve
    // the empty result this call but never store it.
    let extraction = match extract() {
        Ok(extraction) => Rc::new(extraction),
        Err(_) => return Rc::new((Vec::new(), None, None)),
    };
    TEST_EXTRACTION_CACHE.with(|cache| {
        if let Ok(mut cache) = cache.try_borrow_mut() {
            let cache = cache.get_or_insert_with(|| {
                TestExtractionCache::with_bounds(MAX_ENTRIES, MAX_APPROX_BYTES)
            });
            cache.insert(key, Rc::clone(&extraction));
        }
    });
    extraction
}

/// Number of entries in the test-extraction cache (tests only).
#[cfg(test)]
fn test_extraction_cache_len() -> usize {
    TEST_EXTRACTION_CACHE.with(|cache| {
        cache
            .borrow()
            .as_ref()
            .map_or(0, |cache| cache.entries.len())
    })
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

    const TEST_FILE: &str = "import { add } from '../src/add';\nit('adds', () => {\n  expect(add(1, 2)).toBe(3);\n});\n";

    #[test]
    fn test_extraction_cache_serves_unchanged_content_and_misses_on_any_edit() {
        let path = Path::new("tests/cache_probe.test.ts");
        let first = test_extraction_cached(path, TEST_FILE);
        assert_eq!(first.0.len(), 1, "the fixture must extract its one test");
        let again = test_extraction_cached(path, TEST_FILE);
        assert!(
            Rc::ptr_eq(&first, &again),
            "unchanged path and bytes must be served from the cache"
        );
        let edited = TEST_FILE.replace("toBe(3)", "toBe(4)");
        let after_edit = test_extraction_cached(path, &edited);
        assert!(
            !Rc::ptr_eq(&first, &after_edit),
            "a changed byte must miss the cache"
        );
        assert!(
            after_edit.0[0].body_text.contains("toBe(4)"),
            "the edited extraction must reflect the new bytes, not the cached ones"
        );
        let other_path = test_extraction_cached(Path::new("tests/other_probe.test.ts"), TEST_FILE);
        assert!(
            !Rc::ptr_eq(&first, &other_path),
            "the same bytes under another path must not share an entry"
        );
        assert!(test_extraction_cache_len() >= 3);
    }

    #[test]
    fn test_extraction_cache_matches_the_uncached_extraction() {
        let path = Path::new("tests/cache_parity.test.ts");
        let cached = test_extraction_cached(path, TEST_FILE);
        let direct = super::super::tests_extract::extract_tests_with_expansion(path, TEST_FILE);
        assert_eq!(cached.0, direct.0);
        assert_eq!(cached.1, direct.1);
        assert_eq!(cached.2.is_some(), direct.2.is_some());
    }

    #[test]
    fn worker_failures_are_environmental_and_budget_refusals_are_not() {
        use super::super::parse::is_worker_failure;
        assert!(is_worker_failure(
            "static limit parse_worker_failed: TypeScript parse worker thread failed; syntax facts unavailable"
        ));
        assert!(is_worker_failure(
            "static limit parse_worker_unavailable: TypeScript parse worker thread could not start: boom"
        ));
        assert!(!is_worker_failure(
            "static limit expression_nesting_budget: estimated expression nesting depth 9999 exceeds the 2000-level parse budget; parse refused to avoid a parser stack overflow"
        ));
    }

    #[test]
    fn over_budget_test_source_caches_its_deterministic_empty_result() {
        let path = Path::new("tests/deep_probe.test.ts");
        let deep = format!(
            "it('d', () => {{ expect({}1{}).toBe(1); }});\n",
            "(".repeat(2_500),
            ")".repeat(2_500)
        );
        let first = test_extraction_cached(path, &deep);
        assert!(first.0.is_empty());
        let second = test_extraction_cached(path, &deep);
        assert!(
            Rc::ptr_eq(&first, &second),
            "the budget refusal is a function of the bytes, so it may be cached"
        );
    }

    #[test]
    fn test_extraction_cache_keeps_distinct_spellings_of_a_path_apart() {
        // On Unix `a\\b.test.ts` is one filename and `a/b.test.ts` another;
        // each must keep its own `file`.
        let slash = test_extraction_cached(Path::new("tests/a/b.test.ts"), TEST_FILE);
        let backslash = test_extraction_cached(Path::new("tests/a\\b.test.ts"), TEST_FILE);
        assert!(!Rc::ptr_eq(&slash, &backslash));
        assert_eq!(slash.0[0].file, Path::new("tests/a/b.test.ts"));
        assert_eq!(backslash.0[0].file, Path::new("tests/a\\b.test.ts"));
    }

    #[test]
    fn test_extraction_cache_counts_a_shared_file_wide_list_once() {
        let many_imports: String = (0..40)
            .map(|n| format!("import {{ f{n} }} from '../src/f{n}';\n"))
            .collect();
        let tests: String = (0..50)
            .map(|n| format!("it('t{n}', () => {{ expect(f0({n})).toBe({n}); }});\n"))
            .collect();
        let source = format!("{many_imports}{tests}");
        let extraction = test_extraction_cached(Path::new("tests/bytes.test.ts"), &source);
        assert_eq!(extraction.0.len(), 50);
        let key = CacheKey {
            path: "tests/bytes.test.ts".to_string(),
            source_len: source.len(),
            fingerprint: (0, 0),
        };
        let bytes = TestExtractionCache::entry_bytes(&key, &extraction);
        let one_test_imports = extraction.0[0].imports_in_file.len()
            * std::mem::size_of::<super::super::types::TypeScriptImport>();
        assert!(one_test_imports > 0);
        assert!(
            bytes >= one_test_imports,
            "the file's imports must be counted: {bytes} < {one_test_imports}"
        );
        assert!(
            bytes < 50 * one_test_imports,
            "the tests share one import list, so it is not charged per test: {bytes}"
        );
    }

    #[test]
    fn test_extraction_cache_skips_an_entry_larger_than_its_bound() {
        let extraction = Rc::new(super::super::tests_extract::extract_tests_with_expansion(
            Path::new("tests/bound.test.ts"),
            TEST_FILE,
        ));
        let key = CacheKey {
            path: "tests/bound.test.ts".to_string(),
            source_len: TEST_FILE.len(),
            fingerprint: (1, 1),
        };
        let mut cache = TestExtractionCache::with_bounds(8, 16);
        cache.insert(key.clone(), Rc::clone(&extraction));
        assert!(
            cache.entries.is_empty(),
            "an oversize entry must not be stored"
        );
        let mut roomy = TestExtractionCache::with_bounds(8, 1 << 30);
        roomy.insert(key, extraction);
        assert_eq!(roomy.entries.len(), 1);
    }

    #[test]
    fn a_worker_failure_is_served_empty_but_never_stored() {
        let path = Path::new("tests/worker_failure.test.ts");
        let before = test_extraction_cache_len();
        let failed = test_extraction_cached_with(path, TEST_FILE, || {
            Err("static limit parse_worker_failed: forced".to_string())
        });
        assert!(
            failed.0.is_empty(),
            "the failing call serves an empty result"
        );
        assert_eq!(
            test_extraction_cache_len(),
            before,
            "the failure must not be stored"
        );
        let recovered = test_extraction_cached(path, TEST_FILE);
        assert_eq!(
            recovered.0.len(),
            1,
            "after the worker recovers, the same bytes must extract their test"
        );
    }
}
