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
    // Count the envelopes before the fixture (and the cache under it) is
    // removed.
    let persisted = store
        .entry_dir()
        .0
        .read_dir()
        .map(|entries| entries.filter_map(|entry| entry.ok()).count());
    let entry_dir = store.entry_dir().0;
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

    let persisted = persisted
        .map_err(|err| format!("read fact-store entry dir {}: {err}", entry_dir.display()))?;
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
            unusable_base: None,
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
            unusable_base: None,
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

/// A relative re-export resolves its target on disk, which can change while
/// the barrel's bytes do not, so the barrel is recomputed every run; a renamed
/// own export resolves to the file itself and is persisted like any file.
#[test]
fn a_barrel_that_re_exports_through_the_filesystem_is_never_persisted() -> Result<(), String> {
    let root = build_fixture("fact-store-barrel")?;
    ts_write_file(
        &root.join("src/index.ts"),
        "export * from \"./small\";\nexport { greet as hello } from \"./small\";\n",
    )?;
    ts_write_file(
        &root.join("src/own.ts"),
        "function inner(): number {\n  return 1;\n}\nexport { inner as outer };\n",
    )?;

    let cold = run_adapter(&root, &[small_changed_file()]);
    let warm = run_adapter(&root, &[small_changed_file()]);
    let _ = std::fs::remove_dir_all(&root);
    let (cold, warm) = (cold?, warm?);

    assert_eq!(
        (
            cold.preview_fact_cache.misses,
            cold.preview_fact_cache.stores
        ),
        (5, 4),
        "every file is parsed cold, and all but the barrel are persisted: {:?}",
        cold.preview_fact_cache
    );
    assert_eq!(
        (warm.preview_fact_cache.hits, warm.preview_fact_cache.misses),
        (4, 1),
        "the barrel is the one file recomputed warm: {:?}",
        warm.preview_fact_cache
    );
    assert_eq!(
        finding_identities(&cold.findings),
        finding_identities(&warm.findings)
    );
    Ok(())
}

/// `export { a as b } from './'` resolves `./` through the directory, which
/// today lands on this very file; a later `package.json` can move it without
/// touching the barrel, so the barrel is never persisted even though its
/// recorded target equals its own module (#10725 review).
#[test]
fn a_re_export_that_resolves_back_to_its_own_file_is_never_persisted() -> Result<(), String> {
    let root = build_fixture("fact-store-self-directory")?;
    ts_write_file(
        &root.join("src/index.ts"),
        "export function foo(): number {\n  return 1;\n}\nexport { foo as bar } from \"./\";\n",
    )?;
    let fresh_root = build_fixture("fact-store-self-directory-fresh")?;

    let cold = run_adapter(&root, &[small_changed_file()]);
    // Redirect the directory without editing the barrel.
    let redirect = ts_write_file(&root.join("src/package.json"), "{\"main\": \"other.ts\"}\n")
        .and_then(|()| {
            ts_write_file(
                &root.join("src/other.ts"),
                "export function foo(): number {\n  return 2;\n}\n",
            )
        });
    let warm = redirect.and_then(|()| run_adapter(&root, &[small_changed_file()]));
    let fresh = std::fs::read_dir(root.join("src"))
        .map_err(|error| error.to_string())
        .and_then(|entries| {
            for entry in entries {
                let path = entry.map_err(|error| error.to_string())?.path();
                let Some(name) = path.file_name() else {
                    continue;
                };
                let source = std::fs::read(&path).map_err(|error| error.to_string())?;
                std::fs::write(fresh_root.join("src").join(name), source)
                    .map_err(|error| error.to_string())?;
            }
            run_adapter(&fresh_root, &[small_changed_file()])
        });
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&fresh_root);
    let (cold, warm, fresh) = (cold?, warm?, fresh?);

    assert_eq!(
        (
            cold.preview_fact_cache.misses,
            cold.preview_fact_cache.stores
        ),
        (4, 3),
        "the self-resolving barrel is parsed but not persisted: {:?}",
        cold.preview_fact_cache
    );
    assert_eq!(
        warm.preview_fact_cache.hits, 3,
        "only the three plain files are reused: {:?}",
        warm.preview_fact_cache
    );
    assert_eq!(
        finding_identities(&warm.findings),
        finding_identities(&fresh.findings),
        "a warm run after the redirect matches a cold run on the same tree"
    );
    Ok(())
}

/// On Unix a filename may hold a literal backslash, which path normalization
/// folds into a separator; such a file would share an envelope with its
/// folded twin, so it is neither read from nor written to the store.
#[cfg(unix)]
#[test]
fn a_test_file_whose_name_holds_a_backslash_is_never_persisted() -> Result<(), String> {
    let root = build_fixture("fact-store-backslash")?;
    ts_write_file(&root.join("tests/a/b.test.ts"), SMALL_TEST_SOURCE)?;
    ts_write_file(&root.join("tests/a\\b.test.ts"), SMALL_TEST_SOURCE)?;

    let cold = run_adapter(&root, &[small_changed_file()]);
    let warm = run_adapter(&root, &[small_changed_file()]);
    let _ = std::fs::remove_dir_all(&root);
    let (cold, warm) = (cold?, warm?);

    assert_eq!(
        cold.preview_fact_cache.stores, 4,
        "the backslash-named twin is not persisted: {:?}",
        cold.preview_fact_cache
    );
    assert_eq!(
        (warm.preview_fact_cache.hits, warm.preview_fact_cache.misses),
        (4, 0),
        "the twin is recomputed without a store lookup: {:?}",
        warm.preview_fact_cache
    );
    assert_eq!(
        finding_identities(&cold.findings),
        finding_identities(&warm.findings)
    );
    Ok(())
}

/// A process that already holds a file's facts in memory (an LSP refresh)
/// neither reads the envelope again nor rewrites it.
#[test]
fn facts_already_in_memory_skip_the_persistent_store() -> Result<(), String> {
    let root = build_fixture("fact-store-in-memory")?;
    let cold = run_adapter(&root, &[small_changed_file()]);
    let options = ts_analysis_options(root.clone());
    let refresh = TypeScriptAdapter::analyze_diff_with_read_limits(
        &options,
        &[small_changed_file()],
        FIXTURE_FILE_READ_LIMIT,
        FIXTURE_WORKSPACE_READ_BUDGET,
    );
    let _ = std::fs::remove_dir_all(&root);
    let (cold, refresh) = (cold?, refresh?);

    assert_eq!(cold.preview_fact_cache.stores, 3);
    assert!(
        !refresh.preview_fact_cache.did_work(),
        "an in-memory refresh does no store work: {:?}",
        refresh.preview_fact_cache
    );
    assert_eq!(
        finding_identities(&cold.findings),
        finding_identities(&refresh.findings)
    );
    Ok(())
}

/// A cache base that exists but is not a directory is reported once, not as
/// one corrupt entry per workspace file, and the run still completes.
#[test]
fn an_unusable_cache_base_is_reported_once() -> Result<(), String> {
    let root = build_fixture("fact-store-unusable-base")?;
    ts_write_file(&root.join("target/ripr/cache"), "not a directory\n")?;
    let result = run_adapter(&root, &[small_changed_file()]);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;

    let stats = &result.preview_fact_cache;
    assert!(stats.unusable_base.is_some(), "{stats:?}");
    assert_eq!(
        (
            stats.hits,
            stats.misses,
            stats.corrupt_ignored,
            stats.stores,
            stats.store_errors
        ),
        (0, 0, 0, 0, 0),
        "nothing is read or stored: {stats:?}"
    );
    let line = render_fact_cache_line("typescript", stats).unwrap_or_default();
    assert_eq!(line.matches("cache base unusable").count(), 1, "{line}");
    assert!(!result.findings.is_empty());
    Ok(())
}

/// Each test serializes its own copy of the file-wide lists; a decoded file
/// shares one list across its tests again, as extraction built it.
#[test]
fn a_decoded_test_file_shares_its_file_wide_lists_again() -> Result<(), String> {
    let root = build_fixture("fact-store-reshare")?;
    let test_file = Path::new("tests/two.test.ts");
    let source = "import { greet } from \"../src/small\";\n\ntest(\"one\", () => {\n  expect(greet(\"A\")).toBe(\"Hello, A\");\n});\n\ntest(\"two\", () => {\n  expect(greet(\"B\")).toBe(\"Hello, B\");\n});\n";
    ts_write_file(&root.join(test_file), source)?;
    let cold = run_adapter(&root, &[small_changed_file()]);
    let store = TypeScriptFileFactStore::at(&root);
    let loaded = store.load(&super::fact_store::TypeScriptFileFactKey::new(
        test_file, source, &root, None,
    ));
    let _ = std::fs::remove_dir_all(&root);
    cold?;

    let crate::analysis::seam_cache::CacheLoad::Hit(persisted) = loaded else {
        return Err("the test file's envelope loads".to_string());
    };
    let (tests, _, _) = persisted
        .test_extraction
        .ok_or("the envelope holds the extraction")?;
    let [first, second] = tests.as_slice() else {
        return Err(format!("two tests, got {}", tests.len()));
    };
    assert!(!first.imports_in_file.is_empty());
    assert!(first.imports_in_file.same_list(&second.imports_in_file));
    assert!(first.mocks_in_file.same_list(&second.mocks_in_file));
    Ok(())
}

/// A test file's source with `count` tests that all see the same imports and
/// the same top-level fixture bindings.
fn many_tests_source(count: usize) -> String {
    let mut source = String::new();
    for index in 0..count {
        source.push_str(&format!(
            "import {{ helper{index} }} from \"../src/helpers{index}\";\n"
        ));
    }
    source.push_str("import { greet } from \"../src/small\";\n\n");
    for index in 0..count {
        source.push_str(&format!(
            "test(\"case {index}\", () => {{\n  expect(greet(\"A{index}\")).toBe(\"Hello, A{index}\");\n}});\n\n"
        ));
    }
    source
}

/// A production source with `count` exported functions under the same imports.
fn many_owners_source(count: usize) -> String {
    let mut source = String::new();
    for index in 0..count {
        source.push_str(&format!(
            "import {{ helper{index} }} from \"./helpers{index}\";\n"
        ));
    }
    source.push('\n');
    for index in 0..count {
        source.push_str(&format!(
            "export function owner{index}(value: number): number {{\n  return helper0(value) + {index};\n}}\n\n"
        ));
    }
    source
}

/// Size of the envelope a cold run persists for `relative`.
fn envelope_len(root: &Path, relative: &Path, source: &str) -> Result<u64, String> {
    let store = TypeScriptFileFactStore::at(root);
    std::fs::metadata(store.entry_path(relative, source))
        .map(|meta| meta.len())
        .map_err(|err| format!("the envelope for {} exists: {err}", relative.display()))
}

/// An envelope that wrote each test's copy of the file-wide lists and each
/// owner's copy of the file's imports grew with the square of the file (a
/// 267 MB envelope for one VS Code test file). Doubling the entries must
/// about double the envelope, and the decoded file still shares one list.
#[test]
fn a_persisted_envelope_grows_linearly_with_the_entries_of_one_file() -> Result<(), String> {
    let root = build_fixture("fact-store-linear")?;
    let tests_small = Path::new("tests/few.test.ts");
    let tests_large = Path::new("tests/many.test.ts");
    let owners_small = Path::new("src/few_owners.ts");
    let owners_large = Path::new("src/many_owners.ts");
    let (few_tests, many_tests) = (many_tests_source(60), many_tests_source(120));
    let (few_owners, many_owners) = (many_owners_source(60), many_owners_source(120));
    ts_write_file(&root.join(tests_small), &few_tests)?;
    ts_write_file(&root.join(tests_large), &many_tests)?;
    ts_write_file(&root.join(owners_small), &few_owners)?;
    ts_write_file(&root.join(owners_large), &many_owners)?;
    let cold = run_adapter(&root, &[small_changed_file()]);
    let lengths = (
        envelope_len(&root, tests_small, &few_tests),
        envelope_len(&root, tests_large, &many_tests),
        envelope_len(&root, owners_small, &few_owners),
        envelope_len(&root, owners_large, &many_owners),
    );
    let store = TypeScriptFileFactStore::at(&root);
    let loaded_owners = store.load(&super::fact_store::TypeScriptFileFactKey::new(
        owners_large,
        &many_owners,
        &root,
        None,
    ));
    let loaded_tests = store.load(&super::fact_store::TypeScriptFileFactKey::new(
        tests_large,
        &many_tests,
        &root,
        None,
    ));
    let _ = std::fs::remove_dir_all(&root);
    cold?;

    let (few_t, many_t, few_o, many_o) = (lengths.0?, lengths.1?, lengths.2?, lengths.3?);
    // Linear growth doubles the size; a quadratic envelope quadruples it.
    assert!(
        many_t * 10 < few_t * 25,
        "test envelope grew {few_t} -> {many_t} bytes for twice the tests"
    );
    assert!(
        many_o * 10 < few_o * 25,
        "owner envelope grew {few_o} -> {many_o} bytes for twice the owners"
    );

    let crate::analysis::seam_cache::CacheLoad::Hit(persisted) = loaded_owners else {
        return Err("the owner file's envelope loads".to_string());
    };
    let owners = &persisted.file_facts.owners;
    let [first, .., last] = owners.as_slice() else {
        return Err(format!("many owners, got {}", owners.len()));
    };
    assert!(!first.imports.is_empty());
    assert!(first.imports.same_list(&last.imports));
    let crate::analysis::seam_cache::CacheLoad::Hit(persisted) = loaded_tests else {
        return Err("the test file's envelope loads".to_string());
    };
    let (tests, _, _) = persisted
        .test_extraction
        .ok_or("the envelope holds the extraction")?;
    let [first, .., last] = tests.as_slice() else {
        return Err(format!("many tests, got {}", tests.len()));
    };
    assert!(first.imports_in_file.same_list(&last.imports_in_file));
    Ok(())
}

/// A persisted envelope is untrusted input: a shared-list reference that
/// names no earlier list, or one read outside a wire scope, must fail the
/// decode (so the store recomputes) rather than yield an empty or wrong list.
#[test]
fn a_malformed_shared_list_reference_fails_to_decode() {
    use super::types::{Shared, SharedWireScope};

    // `assert!(x.is_err())` trips clippy::assertions_on_result_states; the
    // helper keeps the intent without an `unwrap_err` on an opaque payload.
    fn fails<T>(result: Result<T, serde_json::Error>) -> bool {
        result.is_err()
    }

    let decode = |json: &str| {
        let _scope = SharedWireScope::open();
        serde_json::from_str::<Vec<Shared<u32>>>(json)
    };

    // Control: the first sight defines the list, later sights reference it.
    let ok = decode(r#"[{"shared":1,"items":[7,8]},{"shared":1}]"#);
    assert!(ok.is_ok_and(|lists| lists.len() == 2 && lists[0].same_list(&lists[1])));

    // A forward or unknown reference.
    assert!(fails(decode(r#"[{"shared":1}]"#)));
    assert!(fails(decode(r#"[{"shared":2},{"shared":2,"items":[1]}]"#)));
    // An unexpected field, and a map without the `shared` key.
    assert!(fails(decode(r#"[{"shared":1,"other":[1]}]"#)));
    assert!(fails(decode(r#"[{"items":[1]}]"#)));
    // The wire form is refused outside a scope.
    assert!(fails(serde_json::from_str::<Vec<Shared<u32>>>(
        r#"[{"shared":1,"items":[1]}]"#
    )));
}
