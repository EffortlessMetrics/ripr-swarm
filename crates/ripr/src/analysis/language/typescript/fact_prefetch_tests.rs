//! Parallel fact lookahead (#13832): a prefetched derivation is the value the
//! serial loop would have computed, and seeding it is what the loop's own
//! lookup then serves.

use super::extraction_cache::{
    compute_file_facts, file_facts_in_memory, reset_caches_for_test, test_extraction_cached,
    test_extraction_in_memory,
};
use super::fact_prefetch::{PrefetchRequest, prefetch, window, workers};
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
    let mut prefetched = prefetch(&requests, root, None, 2);
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
    let mut prefetched = prefetch(&requests, root, None, 1);
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
