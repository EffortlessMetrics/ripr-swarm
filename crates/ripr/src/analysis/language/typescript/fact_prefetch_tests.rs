//! Parallel fact lookahead (#13832): a prefetched derivation is the value the
//! serial loop would have computed, and seeding it is what the loop's own
//! lookup then serves.

use super::extraction_cache::{
    compute_file_facts, file_facts_in_memory, reset_caches_for_test, test_extraction_cached,
    test_extraction_in_memory,
};
use super::fact_prefetch::{PrefetchRequest, derive_window, window, workers};
use super::tests_extract::try_extract_tests_with_expansion;
use std::path::Path;

const OWNER: &str = "import { Level } from './level';\n\
export enum Mode { On, Off }\n\
export function price(input: number): number {\n  return input > 10 ? input * 2 : input;\n}\n\
export function label(level: Level): string {\n  return `${Level.High}:${level}`;\n}\n";

const TEST: &str = "import { price } from '../src/price';\n\
describe('price', () => {\n  it('doubles above ten', () => {\n    expect(price(11)).toBe(22);\n  });\n});\n";

const BROKEN: &str = "export function broken( {\n";

fn requests() -> Vec<PrefetchRequest<'static>> {
    vec![
        PrefetchRequest {
            relative: Path::new("src/price.ts"),
            source: OWNER,
            tests: false,
        },
        PrefetchRequest {
            relative: Path::new("test/price.test.ts"),
            source: TEST,
            tests: true,
        },
        PrefetchRequest {
            relative: Path::new("src/broken.ts"),
            source: BROKEN,
            tests: false,
        },
    ]
}

fn facts_json(relative: &Path, source: &str, root: &Path) -> Result<String, String> {
    let facts = compute_file_facts(relative, source, root, None)?;
    serde_json::to_string(&facts).map_err(|err| err.to_string())
}

#[test]
fn prefetched_facts_and_tests_equal_the_serial_derivation() -> Result<(), String> {
    reset_caches_for_test();
    let root = Path::new("/workspace");
    let requests = requests();
    let mut prefetched = derive_window(&requests, root, None, 2);
    for request in &requests {
        assert!(!file_facts_in_memory(
            request.relative,
            request.source,
            root,
            None
        ));
        prefetched.seed_facts(request.relative, request.source, root, None);
        // Seeding fills the cache the loop's `file_facts_cached` reads.
        assert!(file_facts_in_memory(
            request.relative,
            request.source,
            root,
            None
        ));
        let seeded = super::extraction_cache::file_facts_cached(
            request.relative,
            request.source,
            root,
            None,
        )?;
        let seeded = serde_json::to_string(&*seeded).map_err(|err| err.to_string())?;
        assert_eq!(seeded, facts_json(request.relative, request.source, root)?);
    }
    let test = &requests[1];
    prefetched.seed_tests(test.relative, test.source);
    assert!(test_extraction_in_memory(test.relative, test.source));
    let seeded = test_extraction_cached(test.relative, test.source);
    let serial = try_extract_tests_with_expansion(test.relative, test.source)?;
    let json = |extraction: &super::extraction_cache::TestExtraction| {
        serde_json::to_string(extraction).map_err(|err| err.to_string())
    };
    assert_eq!(json(&seeded)?, json(&serial)?);
    assert!(!seeded.0.is_empty(), "fixture must register a test");
    // Only requested test files are extracted.
    prefetched.seed_tests(requests[0].relative, requests[0].source);
    assert!(!test_extraction_in_memory(
        requests[0].relative,
        requests[0].source
    ));
    reset_caches_for_test();
    Ok(())
}

#[test]
fn the_broken_fixture_derives_a_parse_error_like_the_serial_path() -> Result<(), String> {
    let facts = compute_file_facts(Path::new("src/broken.ts"), BROKEN, Path::new("/w"), None)?;
    assert!(facts.parse_error.is_some());
    Ok(())
}

#[test]
fn a_single_worker_prefetches_nothing_so_the_loop_stays_serial() {
    reset_caches_for_test();
    let root = Path::new("/workspace");
    let requests = requests();
    let mut prefetched = derive_window(&requests, root, None, 1);
    for request in &requests {
        prefetched.seed_facts(request.relative, request.source, root, None);
        assert!(!file_facts_in_memory(
            request.relative,
            request.source,
            root,
            None
        ));
    }
    reset_caches_for_test();
}

#[test]
fn the_window_scales_with_workers_and_never_reaches_zero() {
    assert_eq!(window(0), 1);
    assert_eq!(window(4), 64);
    assert!((1..=8).contains(&workers()));
}

/// Build one fixture workspace: several owners, their tests, and one file
/// with a parse error, so the window holds more than one uncached file.
fn build_workspace(label: &str) -> Result<std::path::PathBuf, String> {
    use super::tests::{ts_unique_tempdir, ts_write_file};
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/price.ts"), OWNER)?;
    ts_write_file(
        &root.join("src/level.ts"),
        "export enum Level { Low, High }\n",
    )?;
    ts_write_file(&root.join("src/broken.ts"), BROKEN)?;
    ts_write_file(&root.join("test/price.test.ts"), TEST)?;
    for index in 0..6 {
        ts_write_file(
            &root.join(format!("src/extra{index}.ts")),
            &format!(
                "export function extra{index}(value: number): number {{\n  return value + {index};\n}}\n"
            ),
        )?;
        ts_write_file(
            &root.join(format!("test/extra{index}.test.ts")),
            &format!(
                "import {{ extra{index} }} from '../src/extra{index}';\n\
                 it('adds', () => {{\n  expect(extra{index}(1)).toBe({});\n}});\n",
                index + 1
            ),
        )?;
    }
    Ok(root)
}

fn analyze_workspace(root: &Path, workers: usize) -> Result<(String, String, usize), String> {
    use super::fact_prefetch::{SEEDED_FOR_TEST, with_workers};
    reset_caches_for_test();
    SEEDED_FOR_TEST.with(|seeded| seeded.set(0));
    let changed = super::ChangedFile {
        path: std::path::PathBuf::from("src/price.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: "  return input > 10 ? input * 2 : input;".to_string(),
            new_side_line: 4,
        }],
        removed_lines: Vec::new(),
    };
    let options = super::tests::ts_analysis_options(root.to_path_buf());
    let result = with_workers(workers, || {
        super::TypeScriptAdapter::analyze_diff_with_read_limits(
            &options,
            &[changed],
            1_048_576,
            67_108_864,
        )
    });
    let seeded = SEEDED_FOR_TEST.with(std::cell::Cell::get);
    reset_caches_for_test();
    let result = result?;
    let root_text = root.to_string_lossy().into_owned();
    let findings = format!("{:?}", result.findings).replace(&root_text, "<root>");
    let stats = format!("{:?}", result.preview_fact_cache).replace(&root_text, "<root>");
    Ok((findings, stats, seeded))
}

/// The whole adapter run is identical with and without the lookahead, and the
/// lookahead really served the loop (a no-op prefetch would seed nothing).
#[test]
fn the_adapter_run_is_identical_with_and_without_the_lookahead() -> Result<(), String> {
    let serial_root = build_workspace("prefetch-serial")?;
    let parallel_root = build_workspace("prefetch-parallel")?;
    let serial = analyze_workspace(&serial_root, 1);
    let parallel = analyze_workspace(&parallel_root, 4);
    let _ = std::fs::remove_dir_all(&serial_root);
    let _ = std::fs::remove_dir_all(&parallel_root);
    let (serial_findings, serial_stats, serial_seeded) = serial?;
    let (parallel_findings, parallel_stats, parallel_seeded) = parallel?;
    assert!(
        serial_findings.contains("price.ts"),
        "fixture must classify the edit"
    );
    assert_eq!(serial_seeded, 0, "one worker never prefetches");
    // Every cleanly or erroneously parsed workspace file was prefetched.
    assert_eq!(parallel_seeded, 16, "the lookahead must serve the loop");
    assert_eq!(parallel_findings, serial_findings);
    assert_eq!(parallel_stats, serial_stats);
    Ok(())
}
