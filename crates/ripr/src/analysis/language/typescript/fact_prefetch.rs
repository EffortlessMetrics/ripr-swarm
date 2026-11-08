//! Parallel lookahead for the per-file TypeScript fact loop (#13832).
//!
//! The workspace fact loop in `analyze_diff_with_read_limits_and_handwritten`
//! derives every file's facts serially: one guarded parse per file, plus the
//! test extraction for test files. On a cold run over a large monorepo
//! (vscode: about 15k files) that serial derivation is most of the wall time.
//!
//! Both derivations are pure functions of their inputs and already run on a
//! fresh large-stack parse worker ([`super::parse::parse_on_worker`]), so no
//! caller thread state reaches them. This module derives a window of upcoming
//! files on a few threads at once; the loop then seeds each result into the
//! process-lifetime cache immediately before its own unchanged lookup, which
//! serves the seeded value. The loop body, its order, its cache admissions
//! and every output stay exactly as in the serial run.
//!
//! A file is only prefetched when the loop would derive it: not already in
//! memory, and no persisted envelope that could serve it. A derivation that
//! fails (worker refusal or panic) is dropped, so the loop recomputes it
//! serially and discloses exactly as before.

use super::extraction_cache::{self, FileFacts, TestExtraction};
use super::tsconfig::TsAliasMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Upper bound on lookahead threads. Each runs one parse worker at a time.
const MAX_WORKERS: usize = 8;
/// Files derived per window and worker. Small enough that a window's results
/// are short-lived, large enough to amortize the thread start per window.
const FILES_PER_WORKER: usize = 16;

/// One file the loop will derive.
pub(super) struct PrefetchRequest<'a> {
    pub(super) relative: &'a Path,
    pub(super) source: &'a str,
    /// Also derive the test extraction (a test file whose extraction is not
    /// already in memory).
    pub(super) tests: bool,
}

/// Results of one window, consumed by the loop as it reaches each file.
#[derive(Default)]
pub(super) struct Prefetched {
    facts: HashMap<PathBuf, FileFacts>,
    tests: HashMap<PathBuf, TestExtraction>,
}

impl Prefetched {
    /// Seed `relative`'s prefetched facts into the process-lifetime cache so
    /// the loop's next `file_facts_cached` lookup serves them.
    pub(super) fn seed_facts(
        &mut self,
        relative: &Path,
        source: &str,
        workspace_root: &Path,
        alias_map: Option<&TsAliasMap>,
    ) {
        if let Some(facts) = self.facts.remove(relative) {
            extraction_cache::seed_file_facts(
                relative,
                source,
                workspace_root,
                alias_map,
                std::rc::Rc::new(facts),
            );
        }
    }

    /// Seed `relative`'s prefetched test extraction so the loop's next
    /// `test_extraction_cached_disclosing` lookup serves it.
    pub(super) fn seed_tests(&mut self, relative: &Path, source: &str) {
        if let Some(extraction) = self.tests.remove(relative) {
            extraction_cache::seed_test_extraction(relative, source, std::rc::Rc::new(extraction));
        }
    }
}

/// Lookahead threads for this process: none on a single-CPU host, where the
/// serial loop is already the best schedule.
pub(super) fn workers() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(MAX_WORKERS)
}

/// Files per window for `workers` lookahead threads.
pub(super) fn window(workers: usize) -> usize {
    workers.saturating_mul(FILES_PER_WORKER).max(1)
}

/// Derive `requests` on up to `workers` threads. A thread that cannot be
/// spawned leaves its share to the others; if none can, nothing is
/// prefetched and the loop derives every file serially.
pub(super) fn prefetch(
    requests: &[PrefetchRequest<'_>],
    workspace_root: &Path,
    alias_map: Option<&TsAliasMap>,
    workers: usize,
) -> Prefetched {
    let workers = workers.min(requests.len());
    if workers < 2 {
        return Prefetched::default();
    }
    let next = AtomicUsize::new(0);
    let results: Mutex<Prefetched> = Mutex::new(Prefetched::default());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            // A refused spawn only narrows the pool: claimed work is never
            // lost, because claims happen inside the running threads.
            let _spawned = std::thread::Builder::new()
                .name("ripr-ts-prefetch".to_string())
                .spawn_scoped(scope, || {
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(request) = requests.get(index) else {
                            break;
                        };
                        // A panic drops only this file's lookahead; the
                        // loop then derives it serially, exactly as before.
                        let _derived =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                derive(request, workspace_root, alias_map, &results);
                            }));
                    }
                });
        }
    });
    results.into_inner().unwrap_or_default()
}

fn derive(
    request: &PrefetchRequest<'_>,
    workspace_root: &Path,
    alias_map: Option<&TsAliasMap>,
    results: &Mutex<Prefetched>,
) {
    let Ok(facts) = extraction_cache::compute_file_facts(
        request.relative,
        request.source,
        workspace_root,
        alias_map,
    ) else {
        return;
    };
    // The loop extracts tests only from a cleanly parsed test file.
    let tests = (request.tests && facts.parse_error.is_none())
        .then(|| {
            super::tests_extract::try_extract_tests_with_expansion(request.relative, request.source)
                .ok()
        })
        .flatten();
    if let Ok(mut results) = results.lock() {
        let relative = request.relative.to_path_buf();
        if let Some(tests) = tests {
            results.tests.insert(relative.clone(), tests);
        }
        results.facts.insert(relative, facts);
    }
}
