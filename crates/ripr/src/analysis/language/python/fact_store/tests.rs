//! Pins for the persistent per-file Python fact store (#10914): a served
//! envelope restores exactly the facts a fresh extraction produces (less the
//! per-statement list diff mode never reads), reuse is keyed on content, and
//! a damaged entry is never served.

use super::super::{
    AnalysisOptions, ChangedFile, LanguageAdapter, LanguageDiffResult, OraclePolicy, PythonAdapter,
    extract_source_facts,
};
#[cfg(unix)]
use super::super::{FactLookup, stored_or_extracted_facts};
use super::*;
use crate::analysis::diff::ChangedLine;

const PRODUCTION: &str = "import math\nfrom os import path\n\n\ndef area(radius):\n    return math.pi * radius * radius\n\n\nclass Box:\n    def volume(self, side):\n        return side ** 3\n";

/// Two tests whose imports and rebinding facts differ (one imports inside
/// its body), so the interned tables hold more than one value.
const TESTS: &str = "from src.geometry import area\nimport src.geometry as geometry\n\n\ndef test_area():\n    assert area(1) == 3.141592653589793\n\n\ndef test_area_patched(monkeypatch):\n    import math\n    monkeypatch.setattr(geometry, \"area\", lambda radius: 0)\n    assert math.pi > 3\n";

fn unique_tempdir(label: &str) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("clock: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-fact-store-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root)
        .map_err(|err| format!("create_dir_all({}): {err}", root.display()))?;
    Ok(root)
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create_dir_all({}): {err}", parent.display()))?;
    }
    std::fs::write(path, contents).map_err(|err| format!("write({}): {err}", path.display()))
}

#[test]
fn stored_facts_restore_every_interned_field() -> Result<(), String> {
    for (file, source) in [
        ("src/geometry.py", PRODUCTION),
        ("tests/test_geometry.py", TESTS),
    ] {
        let facts = extract_source_facts(Path::new(file), source);
        assert!(
            !facts.facts.is_empty(),
            "{file}: the fixture yields statement facts"
        );
        let stored = StoredFacts::from_facts(&facts);
        let bytes = serde_json::to_vec(&stored).map_err(|err| err.to_string())?;
        let decoded: StoredFacts = serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
        assert_eq!(decoded.into_facts(), Some(served_view(facts)), "{file}");
    }
    let tests = extract_source_facts(Path::new("tests/test_geometry.py"), TESTS);
    assert_eq!(tests.tests.len(), 2);
    let stored = StoredFacts::from_facts(&tests);
    assert_eq!(stored.test_imports.indexes.len(), 2);
    assert_eq!(
        stored.test_imports.values.len(),
        2,
        "a body import gives the second test its own imports"
    );
    assert_eq!(
        stored.test_rebindings.values.len(),
        2,
        "a fixture parameter and a body import give the second test its own rebinding facts"
    );
    let production = StoredFacts::from_facts(&extract_source_facts(
        Path::new("src/geometry.py"),
        PRODUCTION,
    ));
    assert!(production.owner_imports.indexes.len() > 1);
    assert_eq!(
        production.owner_imports.values.len(),
        1,
        "owners of one file share one import list"
    );
    Ok(())
}

#[test]
fn a_table_that_does_not_cover_its_items_is_refused() {
    let facts = extract_source_facts(Path::new("src/geometry.py"), PRODUCTION);
    let mut stored = StoredFacts::from_facts(&facts);
    stored.owner_imports.indexes.pop();
    assert_eq!(stored.clone().into_facts(), None);
    let mut stored = StoredFacts::from_facts(&facts);
    if let Some(index) = stored.owner_imports.indexes.first_mut() {
        *index = 7;
    }
    assert_eq!(stored.into_facts(), None);
}

#[test]
fn store_serves_by_content_and_never_a_damaged_entry() -> Result<(), String> {
    let root = unique_tempdir("load")?;
    let store = PythonFileFactStore::at(&root);
    let relative = Path::new("src/geometry.py");
    let key = PythonFileFactKey::new(relative, PRODUCTION, &root);
    let facts = extract_source_facts(relative, PRODUCTION);
    let checks = (|| -> Result<(), String> {
        assert!(matches!(store.load(&key), CacheLoad::Miss));
        assert_eq!(store.store(&key, &facts), Ok(true));
        match store.load(&key) {
            CacheLoad::Hit(served) => assert_eq!(served, served_view(facts.clone())),
            _ => return Err("a stored envelope is served".to_string()),
        }
        // Same path, other content: the key moves, so the entry misses.
        let edited = PRODUCTION.replace("radius * radius", "radius ** 2");
        let edited_key = PythonFileFactKey::new(relative, &edited, &root);
        assert!(matches!(store.load(&edited_key), CacheLoad::Miss));
        // One flipped byte is a decode or digest failure, never a hit.
        let entry = store.entry_path(relative, PRODUCTION);
        let mut bytes = std::fs::read(&entry).map_err(|err| err.to_string())?;
        let flip = bytes.len() / 2;
        bytes[flip] = bytes[flip].wrapping_add(1);
        std::fs::write(&entry, &bytes).map_err(|err| err.to_string())?;
        assert!(matches!(
            store.load(&key),
            CacheLoad::CorruptIgnored { .. } | CacheLoad::Miss
        ));
        assert!(!matches!(store.load(&key), CacheLoad::Hit(_)));
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root);
    checks?;
    cleanup.map_err(|err| err.to_string())
}

fn run_adapter(root: &Path) -> Result<LanguageDiffResult, String> {
    let options = AnalysisOptions {
        root: root.to_path_buf(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let changed = [ChangedFile {
        path: PathBuf::from("src/geometry.py"),
        added_lines: vec![ChangedLine {
            line: 6,
            new_side_line: 6,
            text: "    return math.pi * radius * radius".to_string(),
        }],
        removed_lines: Vec::new(),
    }];
    PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed)
}

#[test]
fn a_warm_run_serves_every_file_and_an_edit_recomputes_only_that_file() -> Result<(), String> {
    let root = unique_tempdir("adapter")?;
    let checks = (|| -> Result<(), String> {
        write_file(&root.join("src/geometry.py"), PRODUCTION)?;
        write_file(&root.join("tests/test_geometry.py"), TESTS)?;
        let cold = run_adapter(&root)?;
        let warm = run_adapter(&root)?;
        assert!(
            !cold.findings.is_empty(),
            "the changed return is classified"
        );
        assert_eq!(
            format!("{:?}", cold.findings),
            format!("{:?}", warm.findings)
        );
        let (cold_stats, warm_stats) = (&cold.preview_fact_cache, &warm.preview_fact_cache);
        assert_eq!(
            (cold_stats.hits, cold_stats.misses, cold_stats.stores),
            (0, 2, 2)
        );
        assert_eq!((warm_stats.hits, warm_stats.misses), (2, 0));
        assert_eq!(warm_stats.corrupt_ignored + warm_stats.store_errors, 0);
        // An edit recomputes and re-stores only the edited file.
        write_file(
            &root.join("tests/test_geometry.py"),
            &TESTS.replace("assert math.pi > 3", "assert math.pi > 2"),
        )?;
        let edited = run_adapter(&root)?.preview_fact_cache;
        assert_eq!(
            (edited.hits, edited.misses, edited.stores),
            (1, 1, 1),
            "only the edited test file misses"
        );
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root);
    checks?;
    cleanup.map_err(|err| err.to_string())
}

#[cfg(unix)]
#[test]
fn a_path_holding_a_backslash_is_neither_read_nor_persisted() -> Result<(), String> {
    let root = unique_tempdir("backslash")?;
    let store = PythonFileFactStore::at(&root);
    let checks: Result<(), String> = {
        let (facts, lookup) = stored_or_extracted_facts(
            Some(&store),
            &root,
            Path::new("src/odd\\name.py"),
            PRODUCTION,
        );
        assert!(matches!(lookup, FactLookup::Skipped));
        assert!(!facts.owners.is_empty(), "the file is still extracted");
        assert!(
            !store.dir_path().exists(),
            "nothing is written for a path the normalized key would fold"
        );
        let (_, lookup) = stored_or_extracted_facts(
            Some(&store),
            &root,
            Path::new("src/geometry.py"),
            PRODUCTION,
        );
        assert!(
            matches!(lookup, FactLookup::Miss { stored: Ok(true) }),
            "a lossless path in the same store is persisted"
        );
        Ok(())
    };
    let cleanup = std::fs::remove_dir_all(&root);
    checks?;
    cleanup.map_err(|err| err.to_string())
}
