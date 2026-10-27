//! Red-pin tests for the persistent per-file TypeScript fact cache (#10725).
//!
//! Issue #10725: `check` re-parses every untouched workspace file on every
//! run, so a one-line diff next to one 3 MB `.ts` file costs ~48 s and a
//! second run is no faster. The Rust adapter already persists per-file facts
//! keyed by content (`RepoFileFactCache` in `analysis/seam_cache.rs`); these
//! tests pin the TypeScript equivalent: per-file facts derived from one parse
//! survive across adapter runs in a fresh-process shape (the process-lifetime
//! in-memory caches are reset between runs), are reused only when the file's
//! content still matches, and are never served after tampering.
//!
//! These tests are fixture-first and FAIL on current code: the
//! `typescript::fact_store` module, the `PreviewFactCacheStats` carrier on
//! `LanguageDiffResult`, and the `reset_caches_for_test` hook do not exist
//! yet. The planned behavior each test pins is documented in its doc comment.
//! Reuse counts must reflect real cache admissions — a hit means the facts
//! were decoded from a content-keyed on-disk envelope that passed its
//! integrity check, never an estimate.

use super::TypeScriptAdapter;
use super::extraction_cache::reset_caches_for_test;
use super::fact_store::{TypeScriptFileFactStore, render_fact_cache_line};
use super::tests::{ts_analysis_options, ts_unique_tempdir, ts_write_file};
use crate::analysis::diff::ChangedLine;
use crate::analysis::language::adapter::PreviewFactCacheStats;
use crate::domain::Finding;
use std::path::{Path, PathBuf};

/// Per-file read caps for the fixture runs: far above the fixture sizes, far
/// below anything that could refuse a fixture file.
const FIXTURE_FILE_READ_LIMIT: u64 = 1_048_576;
const FIXTURE_WORKSPACE_READ_BUDGET: u64 = 67_108_864;

/// A multi-hundred-line production file standing in for the issue's 3 MB
/// `.ts` file: unchanged by the diff, it is the file whose per-run re-parse
/// dominates wall time today. `step` keeps a same-length rewrite available
/// for the mtime-preserving-rewrite test (`acc += 1` -> `acc += 2`).
fn big_source(step: u32) -> String {
    let mut source =
        String::from("export function big(input: number): number {\n  let acc = input;\n");
    for index in 0..150 {
        source.push_str(&format!("  acc += {step}; // line {index}\n"));
    }
    source.push_str("  return acc;\n}\n");
    source
}

const SMALL_SOURCE: &str = "export function greet(name: string): string {\n  const greeting = `Hello, ${name}`;\n  return greeting;\n}\n";

const SMALL_TEST_SOURCE: &str = "import { greet } from \"../src/small\";\n\ntest(\"greets by name\", () => {\n  expect(greet(\"Sam\")).toBe(\"Hello, Sam\");\n});\n";

/// One added line inside `greet`'s body: the smallest honest diff for this
/// fixture, and enough to produce at least one finding.
fn small_changed_file() -> super::ChangedFile {
    super::ChangedFile {
        path: PathBuf::from("src/small.ts"),
        added_lines: vec![ChangedLine {
            line: 3,
            text: "  return greeting;".to_string(),
            new_side_line: 3,
        }],
        removed_lines: Vec::new(),
    }
}

fn run_adapter(
    root: &Path,
    changed: &[super::ChangedFile],
) -> Result<super::LanguageDiffResult, String> {
    reset_caches_for_test();
    let options = ts_analysis_options(root.to_path_buf());
    TypeScriptAdapter::analyze_diff_with_read_limits(
        &options,
        changed,
        FIXTURE_FILE_READ_LIMIT,
        FIXTURE_WORKSPACE_READ_BUDGET,
    )
}

/// Findings identity stable across runs: cached facts must produce the same
/// classified output as freshly parsed facts, not merely the same count.
fn finding_identities(findings: &[Finding]) -> Vec<(String, usize, String)> {
    let mut identities: Vec<(String, usize, String)> = findings
        .iter()
        .map(|finding| {
            (
                finding
                    .probe
                    .location
                    .file
                    .to_string_lossy()
                    .replace('\\', "/"),
                finding.probe.location.line,
                format!("{:?}", finding.class),
            )
        })
        .collect();
    identities.sort();
    identities
}

fn build_fixture(label: &str) -> Result<PathBuf, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/big.ts"), &big_source(1))?;
    ts_write_file(&root.join("src/small.ts"), SMALL_SOURCE)?;
    ts_write_file(&root.join("tests/small.test.ts"), SMALL_TEST_SOURCE)?;
    Ok(root)
}

/// Cold run: every workspace file is parsed once, and the derived per-file
/// facts (owners, export facts, phase-2 facts, test extraction) are persisted
/// under the workspace cache base, one content-keyed envelope per file.
#[test]
fn cold_run_persists_per_file_facts_for_every_workspace_file() -> Result<(), String> {
    let root = build_fixture("fact-store-cold")?;
    let store = TypeScriptFileFactStore::at(&root);

    let result = run_adapter(&root, &[small_changed_file()]);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;

    assert!(
        !result.findings.is_empty(),
        "the fixture diff must produce at least one finding"
    );
    assert_eq!(
        result.preview_fact_cache.hits, 0,
        "a cold run serves no cached facts"
    );
    assert_eq!(
        result.preview_fact_cache.misses, 3,
        "every workspace file is a miss on a cold run, got {:?}",
        result.preview_fact_cache
    );
    assert_eq!(
        result.preview_fact_cache.stores, 3,
        "every parsed file's facts are persisted, got {:?}",
        result.preview_fact_cache
    );

    let (entry_dir, _) = store.entry_dir();
    let persisted = std::fs::read_dir(&entry_dir)
        .map_err(|err| format!("read fact-store entry dir {}: {err}", entry_dir.display()))?
        .filter_map(|entry| entry.ok())
        .count();
    assert_eq!(
        persisted,
        3,
        "one on-disk envelope per workspace file, got {persisted} entries in {}",
        entry_dir.display()
    );
    Ok(())
}

/// Warm rerun with the process-lifetime in-memory caches reset (a fresh
/// `ripr check` process has no memory of the previous run): unchanged files
/// are served from the persisted envelopes — no re-parse — and the classified
/// findings are identical to the cold run.
#[test]
fn warm_rerun_reuses_persisted_facts_and_reproduces_identical_findings() -> Result<(), String> {
    let root = build_fixture("fact-store-warm")?;

    let cold = run_adapter(&root, &[small_changed_file()])?;
    let cold_identities = finding_identities(&cold.findings);

    let warm = run_adapter(&root, &[small_changed_file()]);
    let _ = std::fs::remove_dir_all(&root);
    let warm = warm?;

    assert_eq!(
        warm.preview_fact_cache.hits, 3,
        "a warm rerun serves every unchanged file from the persisted store, got {:?}",
        warm.preview_fact_cache
    );
    assert_eq!(
        warm.preview_fact_cache.misses, 0,
        "a warm rerun re-parses nothing, got {:?}",
        warm.preview_fact_cache
    );
    assert_eq!(
        warm.preview_fact_cache.stores, 0,
        "a warm rerun stores nothing, got {:?}",
        warm.preview_fact_cache
    );
    assert_eq!(
        finding_identities(&warm.findings),
        cold_identities,
        "cached facts must classify identically to freshly parsed facts"
    );
    Ok(())
}

/// A content edit invalidates exactly the edited file: the other files still
/// reuse persisted facts; the edited file is re-parsed, re-stored, and named
/// in `invalidated_files`.
#[test]
fn edited_file_is_recomputed_while_unchanged_files_reuse() -> Result<(), String> {
    let root = build_fixture("fact-store-edit")?;
    run_adapter(&root, &[small_changed_file()])?;

    let edited = "export function greet(name: string): string {\n  const greeting = `Hello, ${name}`;\n  const loud = greeting.toUpperCase();\n  return loud;\n}\n";
    ts_write_file(&root.join("src/small.ts"), edited)?;
    let changed = super::ChangedFile {
        path: PathBuf::from("src/small.ts"),
        added_lines: vec![ChangedLine {
            line: 3,
            text: "  const loud = greeting.toUpperCase();".to_string(),
            new_side_line: 3,
        }],
        removed_lines: Vec::new(),
    };

    let result = run_adapter(&root, &[changed]);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;

    assert_eq!(
        result.preview_fact_cache.hits, 2,
        "the two untouched files reuse persisted facts, got {:?}",
        result.preview_fact_cache
    );
    assert_eq!(
        result.preview_fact_cache.misses, 1,
        "exactly the edited file re-parses, got {:?}",
        result.preview_fact_cache
    );
    assert_eq!(
        result.preview_fact_cache.stores, 1,
        "exactly the edited file is re-stored, got {:?}",
        result.preview_fact_cache
    );
    assert_eq!(
        result
            .preview_fact_cache
            .invalidated_files
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>(),
        vec!["src/small.ts".to_string()],
        "the content-invalidation signal names the edited file"
    );
    Ok(())
}

/// Cache integrity: an envelope that fails its integrity check (a tampered
/// on-disk entry) is never served. The affected file is re-parsed
/// (corrupt-ignored, counted, disclosed) and the run's findings match the
/// cold run's.
#[test]
fn tampered_cache_entry_is_recomputed_not_served() -> Result<(), String> {
    let root = build_fixture("fact-store-tamper")?;
    let cold = run_adapter(&root, &[small_changed_file()])?;
    let cold_identities = finding_identities(&cold.findings);

    let store = TypeScriptFileFactStore::at(&root);
    let entry = store.entry_path(Path::new("src/big.ts"), &big_source(1));
    let mut bytes = std::fs::read(&entry)
        .map_err(|err| format!("read tampered-entry target {}: {err}", entry.display()))?;
    let flip = bytes.len() / 2;
    bytes[flip] = bytes[flip].wrapping_add(1);
    std::fs::write(&entry, &bytes)
        .map_err(|err| format!("tamper entry {}: {err}", entry.display()))?;

    let result = run_adapter(&root, &[small_changed_file()]);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;

    assert_eq!(
        result.preview_fact_cache.corrupt_ignored, 1,
        "exactly the tampered envelope is corrupt-ignored, got {:?}",
        result.preview_fact_cache
    );
    assert_eq!(
        result.preview_fact_cache.hits, 2,
        "the two untampered files still reuse persisted facts, got {:?}",
        result.preview_fact_cache
    );
    assert_eq!(
        result.preview_fact_cache.misses, 1,
        "the tampered file's facts are recomputed, got {:?}",
        result.preview_fact_cache
    );
    assert_eq!(
        finding_identities(&result.findings),
        cold_identities,
        "recomputed facts after tampering must classify identically to the cold run"
    );
    Ok(())
}

/// #2175/#3848 contract, mirrored for the preview adapter: reuse is keyed on
/// CONTENT, not on a stat tuple. A same-length rewrite whose mtime is
/// restored (what `rsync -a` / `cp --preserve=timestamps` produces) must miss
/// and recompute — a stat-keyed cache would serve the stale entry here.
#[test]
fn mtime_preserving_same_length_rewrite_is_recomputed() -> Result<(), String> {
    let root = build_fixture("fact-store-mtime")?;
    run_adapter(&root, &[small_changed_file()])?;

    let big_path = root.join("src/big.ts");
    let original_mtime = std::fs::metadata(&big_path)
        .and_then(|metadata| metadata.modified())
        .map_err(|err| format!("stat fixture big.ts: {err}"))?;
    let rewritten = big_source(2);
    assert_eq!(
        rewritten.len(),
        big_source(1).len(),
        "the rewrite must be same-length or the test does not discriminate"
    );
    std::fs::write(&big_path, &rewritten).map_err(|err| format!("rewrite big.ts: {err}"))?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&big_path)
        .map_err(|err| format!("open big.ts to restore mtime: {err}"))?;
    file.set_modified(original_mtime)
        .map_err(|err| format!("restore big.ts mtime: {err}"))?;
    drop(file);

    let result = run_adapter(&root, &[small_changed_file()]);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;

    assert_eq!(
        result.preview_fact_cache.misses, 1,
        "the rewritten file must miss despite the preserved mtime, got {:?}",
        result.preview_fact_cache
    );
    assert_eq!(
        result.preview_fact_cache.hits, 2,
        "the untouched files still reuse persisted facts, got {:?}",
        result.preview_fact_cache
    );
    assert!(
        result
            .preview_fact_cache
            .invalidated_files
            .iter()
            .any(|path| path.to_string_lossy().replace('\\', "/") == "src/big.ts"),
        "the rewritten file is named in the content-invalidation signal"
    );
    Ok(())
}

/// The `-v` reuse line discloses the real counters and stays silent when the
/// adapter did no cache work — a zero-work run must not borrow numbers it did
/// not produce.
#[test]
fn fact_cache_line_discloses_real_counts_and_stays_silent_on_zero_work() {
    let line = render_fact_cache_line(
        "typescript",
        &PreviewFactCacheStats {
            hits: 3,
            misses: 0,
            corrupt_ignored: 0,
            stores: 0,
            store_errors: 0,
            invalidated_files: Default::default(),
        },
    );
    assert_eq!(
        line.as_deref(),
        Some("typescript per-file facts: 3 reused, 0 recomputed, 0 stored")
    );

    let corrupted = render_fact_cache_line(
        "typescript",
        &PreviewFactCacheStats {
            hits: 2,
            misses: 1,
            corrupt_ignored: 1,
            stores: 1,
            store_errors: 0,
            invalidated_files: Default::default(),
        },
    );
    assert_eq!(
        corrupted.as_deref(),
        Some(
            "typescript per-file facts: 2 reused, 1 recomputed, 1 stored; 1 corrupt entry ignored"
        )
    );

    assert_eq!(
        render_fact_cache_line("typescript", &PreviewFactCacheStats::zero_work()),
        None,
        "a run that did no cache work stays silent instead of borrowing counts"
    );
}
