//! #10839: property authority through the real index and seam consumers.
use super::*;
use crate::analysis::seams::{SeamGripClass, SeamKind};
use crate::analysis::{seam_classification, seam_inventory, test_grip_evidence};
use crate::domain::StageState;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = include_str!("../../../tests/fixtures/proptest_singleton/input/src/lib.rs");
const PROPERTY: &str =
    include_str!("../../../tests/fixtures/proptest_singleton/input/tests/boundary.rs");
const MANIFEST: &str = include_str!("../../../tests/fixtures/proptest_singleton/input/Cargo.toml");
const LOCK: &str = include_str!("../../../tests/fixtures/proptest_singleton/input/Cargo.lock");
const CHECKSUM: &str = "4b45fcc2344c680f5025fe57779faef368840d0bd1f42f216291f0dc4ace4744";
const ORDINARY: &str =
    "#[test]\nfn ordinary_witness() { assert_eq!(lib_proptest::ordinary_total(2), 3); }\n";
const PROPERTY_PATH: &str = "tests/boundary.rs";

/// Admission needs a verified proptest identity, whose offline
/// `cargo metadata` probe refuses when the fixture lockfile's crates are
/// absent from the local Cargo cache. That is an environment precondition,
/// not product behavior: on a cold-cache host every admitted-property
/// assertion would fail with a misleading count. Skip loudly there; warm
/// hosts (including CI, which builds the workspace first) run the full
/// assertions unchanged (#11176).
fn proptest_identity_available(root: &Path) -> bool {
    if crate::analysis::workspace::verified_proptest_identity(root).is_some() {
        return true;
    }
    eprintln!(
        "skipping registered-property assertions: verified proptest identity \
         unavailable for {} (offline cargo metadata needs the fixture \
         lockfile's crates in the local Cargo cache; see #11176)",
        root.display()
    );
    false
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-proptest-index-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        for directory in ["src", "tests", ".cargo"] {
            std::fs::create_dir(fixture.0.join(directory)).map_err(|error| error.to_string())?;
        }
        fixture.write("Cargo.toml", MANIFEST)?;
        fixture.write("Cargo.lock", LOCK)?;
        fixture.write(
            "src/lib.rs",
            &format!("{SOURCE}\npub fn ordinary_total(value: i32) -> i32 {{ value + 1 }}\n"),
        )?;
        fixture.write("tests/ordinary.rs", ORDINARY)?;
        fixture.write(PROPERTY_PATH, PROPERTY)?;
        Ok(fixture)
    }

    fn write(&self, path: &str, source: &str) -> Result<(), String> {
        std::fs::write(self.0.join(path), source).map_err(|error| error.to_string())
    }

    fn loaded(&self) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
        ["src/lib.rs", "tests/ordinary.rs", PROPERTY_PATH]
            .into_iter()
            .map(|path| {
                std::fs::read(self.0.join(path))
                    .map(|bytes| (PathBuf::from(path), bytes))
                    .map_err(|error| error.to_string())
            })
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retained failed property index fixture {}",
                self.0.display()
            );
        } else if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!(
                "property index fixture cleanup {}: {error}",
                self.0.display()
            );
        }
    }
}

fn predicate(index: &RustIndex) -> Result<seam_classification::ClassifiedSeam, String> {
    let seams = seam_inventory::inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], index);
    let evidence = test_grip_evidence::evidence_for_seams(&seams, index);
    let mut owned = seam_classification::classify_seams(&seams, &evidence)
        .into_iter()
        .filter(|seam| {
            seam.seam.owner() == "src/lib.rs::discounted_total"
                && seam.seam.kind() == SeamKind::PredicateBoundary
        })
        .collect::<Vec<_>>();
    assert_eq!(
        owned.len(),
        1,
        "the real production predicate must be inventoried"
    );
    owned
        .pop()
        .ok_or_else(|| "missing classified property predicate".to_string())
}

fn ordinary(index: &RustIndex) -> Result<TestFact, String> {
    let tests = index
        .tests()
        .iter()
        .filter(|test| test.name == "ordinary_witness")
        .cloned()
        .collect::<Vec<_>>();
    let [test] = tests.as_slice() else {
        return Err("expected one independent ordinary witness".into());
    };
    assert_eq!(test.file, Path::new("tests/ordinary.rs"));
    assert_eq!(test.assertions.len(), 1);
    Ok(test.clone())
}

fn assert_state(index: &RustIndex, admitted: bool, label: &str) -> Result<Value, String> {
    for path in ["src/lib.rs", "tests/ordinary.rs", PROPERTY_PATH] {
        let context = index
            .execution_context(Path::new(path))
            .ok_or_else(|| format!("{label}: missing execution context for {path}"))?;
        assert_eq!(
            context.roots,
            std::collections::BTreeSet::from([PathBuf::from(path)]),
            "{label}: wrong execution roots for {path}"
        );
        assert!(
            context.edges.is_empty(),
            "{label}: unexpected ancestry for {path}"
        );
    }
    ordinary(index)?;
    let tests = index
        .tests()
        .iter()
        .filter(|test| test.file == Path::new(PROPERTY_PATH))
        .cloned()
        .collect::<Vec<_>>();
    let file = index
        .files()
        .get(Path::new(PROPERTY_PATH))
        .ok_or("missing property file")?;
    let seam = predicate(index)?;
    if admitted {
        let [test] = tests.as_slice() else {
            return Err(format!(
                "{label}: admitted property must have exactly one TestFact"
            ));
        };
        assert_eq!(test.name, "exact_boundary");
        assert_eq!(test.start_line, 3);
        assert_eq!(test.assertions.len(), 1);
        assert_eq!(test.assertions[0].line, 4);
        assert!(file.unresolved_property_macros.is_empty());
        assert_eq!(index.proptest_receipts.len(), 1);
        let receipt = index
            .proptest_receipts
            .get(&(PathBuf::from(PROPERTY_PATH), 3, "exact_boundary".into()))
            .ok_or("missing exact property receipt")?;
        assert_eq!(&receipt.test, test);
        assert_eq!(receipt.line_offset, 1);
        assert!(
            receipt
                .source
                .contains("assert_eq!(lib_proptest::discounted_total(100i32, 100), 90)")
        );
        assert_eq!(receipt.name_scopes.scopes.len(), 2);
        assert!(receipt.name_scopes.scopes[0].fn_body);
        assert!(!receipt.name_scopes.scopes[1].fn_body);
        assert_eq!(
            seam.class,
            SeamGripClass::StronglyGripped,
            "{label}: {seam:?}"
        );
        assert_eq!(seam.evidence.related_tests.len(), 1);
        assert_eq!(seam.evidence.related_tests[0].test_name, "exact_boundary");
        assert!(
            seam.evidence
                .observed_values
                .iter()
                .any(|value| value.value == "100"),
            "{label}: {seam:?}"
        );
        for stage in [
            &seam.evidence.reach,
            &seam.evidence.activate,
            &seam.evidence.propagate,
            &seam.evidence.observe,
            &seam.evidence.discriminate,
        ] {
            assert_eq!(stage.state, StageState::Yes, "{label}: {seam:?}");
        }
    } else {
        assert!(
            tests.is_empty(),
            "{label}: refused property retained a generated test"
        );
        assert!(index.proptest_receipts.is_empty());
        assert_eq!(file.unresolved_property_macros.len(), 1);
        assert_ne!(
            seam.class,
            SeamGripClass::StronglyGripped,
            "{label}: {seam:?}"
        );
        assert!(seam.evidence.related_tests.is_empty(), "{label}: {seam:?}");
        assert!(
            seam.evidence.observed_values.is_empty(),
            "{label}: {seam:?}"
        );
        for stage in [
            &seam.evidence.reach,
            &seam.evidence.activate,
            &seam.evidence.observe,
            &seam.evidence.discriminate,
        ] {
            assert_ne!(stage.state, StageState::Yes, "{label}: {seam:?}");
        }
    }
    serde_json::to_value(seam).map_err(|error| error.to_string())
}

fn assert_same(
    left: &RustIndex,
    right: &RustIndex,
    admitted: bool,
    label: &str,
) -> Result<(), String> {
    // Entire facts and receipt scopes are compared, not just test names or the
    // final class: stale authority can otherwise hide behind an unchanged label.
    assert_eq!(
        left.tests().iter().collect::<Vec<_>>(),
        right.tests().iter().collect::<Vec<_>>(),
        "{label}: TestFacts"
    );
    assert_eq!(
        left.proptest_receipts.len(),
        right.proptest_receipts.len(),
        "{label}: receipt count"
    );
    for (key, receipt) in &left.proptest_receipts {
        let other = right
            .proptest_receipts
            .get(key)
            .ok_or_else(|| format!("{label}: receipt identity mismatch"))?;
        assert_eq!(receipt.test, other.test, "{label}: receipt TestFact");
        assert_eq!(receipt.source, other.source, "{label}: receipt source");
        assert_eq!(
            receipt.line_offset, other.line_offset,
            "{label}: receipt offset"
        );
        assert_eq!(
            receipt.name_scopes, other.name_scopes,
            "{label}: receipt scopes"
        );
    }
    assert_eq!(
        assert_state(left, admitted, &format!("{label}: reference"))?,
        assert_state(right, admitted, &format!("{label}: candidate"))?,
        "{label}: classified predicate"
    );
    Ok(())
}

fn reconstructed(fixture: &Fixture, files: &[(PathBuf, Vec<u8>)]) -> Result<RustIndex, String> {
    let mut raw = parse_loaded_files_with_cache(&fixture.0, files)?;
    let check_raw = |index: &RustIndex| -> Result<(), String> {
        assert!(index.proptest_receipts.is_empty());
        assert!(
            index
                .tests()
                .iter()
                .all(|test| test.file != Path::new(PROPERTY_PATH))
        );
        let property = index
            .files()
            .get(Path::new(PROPERTY_PATH))
            .ok_or("missing raw property")?;
        assert_eq!(property.unresolved_property_macros.len(), 1);
        assert!(property.tests.is_empty());
        ordinary(index)?;
        Ok(())
    };
    check_raw(&raw)?;
    let wire = serde_json::to_value(&raw).map_err(|error| error.to_string())?;
    let decoded: RustIndex = serde_json::from_value(wire).map_err(|error| error.to_string())?;
    check_raw(&decoded)?;
    assert_eq!(
        decoded.owned_file(Path::new(PROPERTY_PATH)),
        raw.owned_file(Path::new(PROPERTY_PATH))
    );
    normalize_reconstructed_source_roles(&mut raw, &fixture.0, &[], None)?;
    // Local normalization intentionally leaves external composition to its
    // caller. Even an admitted receipt cannot supply owner reach without it.
    assert!(raw.execution_context(Path::new(PROPERTY_PATH)).is_none());
    let uncomposed = predicate(&raw)?;
    assert_eq!(
        uncomposed.class,
        SeamGripClass::Ungripped,
        "normalized without composition: {uncomposed:?}"
    );
    assert!(
        uncomposed.evidence.related_tests.is_empty(),
        "normalized without composition: {uncomposed:?}"
    );
    // This fixture has the whole corpus, so use the ordinary composition
    // authority before the downstream seam/evidence/classification consumers.
    compose_fixture_source_roles(&mut raw, &fixture.0);
    raw.finalize()?;
    Ok(raw)
}

#[test]
fn registered_property_authority_matches_reconstructed_and_incremental_indexes()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    if !proptest_identity_available(&fixture.0) {
        return Ok(());
    }
    let files = fixture.loaded()?;
    let paths = files
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let fresh = build_index(&fixture.0, &paths)?;
    assert_state(&fresh, true, "fresh")?;
    let cold =
        build_index_from_loaded_files_with_cache_and_test_harnesses(&fixture.0, &files, &[])?;
    assert_eq!(cold.file_fact_cache.misses, files.len());
    assert_eq!(cold.file_fact_cache.stores, files.len());
    assert_same(&fresh, &cold.index, true, "cold file cache")?;
    let warm =
        build_index_from_loaded_files_with_cache_and_test_harnesses(&fixture.0, &files, &[])?;
    assert_eq!(warm.file_fact_cache.hits, files.len());
    assert_eq!(warm.file_fact_cache.misses, 0);
    assert_same(&fresh, &warm.index, true, "warm file cache")?;
    assert_same(
        &fresh,
        &reconstructed(&fixture, &files)?,
        true,
        "reconstructed",
    )?;
    let core = files
        .iter()
        .filter(|(path, _)| path != Path::new(PROPERTY_PATH))
        .cloned()
        .collect::<Vec<_>>();
    let core = build_index_from_loaded_files_with_cache_and_test_harnesses(&fixture.0, &core, &[])?;
    assert!(core.index.proptest_receipts.is_empty());
    assert_eq!(core.index.tests().len(), 1);
    let extended = reuse_cached_index_with_loaded_files(core, &fixture.0, &files, &[])?;
    assert_same(&fresh, &extended.index, true, "incremental addition")?;
    // Promotion of other index instances must never contaminate cached parse facts.
    assert_same(
        &fresh,
        &reconstructed(&fixture, &files)?,
        true,
        "reconstructed after promotion",
    )?;
    Ok(())
}

#[test]
fn registered_property_reuse_revokes_configuration_and_provider_authority() -> Result<(), String> {
    let fixture = Fixture::new()?;
    if !proptest_identity_available(&fixture.0) {
        return Ok(());
    }
    let files = fixture.loaded()?;
    let paths = files
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let original = build_index(&fixture.0, &paths)?;
    let original_predicate = assert_state(&original, true, "original fresh")?;
    let ordinary_before = ordinary(&original)?;
    let mut cached =
        build_index_from_loaded_files_with_cache_and_test_harnesses(&fixture.0, &files, &[])?;
    let source_hash = cached.files_content_hash.clone();
    for premise in ["configuration", "provider"] {
        if premise == "configuration" {
            fixture.write(".cargo/config.toml", "[env]\nPROPTEST_CASES='0'\n")?;
        } else {
            let altered = LOCK.replace(CHECKSUM, &"0".repeat(64));
            assert_ne!(altered, LOCK);
            fixture.write("Cargo.lock", &altered)?;
        }
        assert_eq!(fixture.loaded()?, files, "{premise} changed Rust bytes");
        let refused = build_index(&fixture.0, &paths)?;
        assert_state(&refused, false, &format!("{premise}: refused fresh"))?;
        assert_same(
            &refused,
            &reconstructed(&fixture, &files)?,
            false,
            &format!("{premise}: refused reconstructed"),
        )?;
        let warm =
            build_index_from_loaded_files_with_cache_and_test_harnesses(&fixture.0, &files, &[])?;
        assert_eq!(warm.file_fact_cache.hits, files.len());
        assert_same(
            &refused,
            &warm.index,
            false,
            &format!("{premise}: refused warm cache"),
        )?;
        cached = reuse_cached_index_with_loaded_files(cached, &fixture.0, &files, &[])?;
        assert_eq!(cached.files_content_hash, source_hash);
        assert_same(
            &refused,
            &cached.index,
            false,
            &format!("{premise}: refused reuse"),
        )?;
        assert_eq!(ordinary(&cached.index)?, ordinary_before);
        if premise == "configuration" {
            std::fs::remove_file(fixture.0.join(".cargo/config.toml"))
                .map_err(|error| error.to_string())?;
        } else {
            fixture.write("Cargo.lock", LOCK)?;
        }
        assert_eq!(fixture.loaded()?, files);
        cached = reuse_cached_index_with_loaded_files(cached, &fixture.0, &files, &[])?;
        assert_eq!(cached.files_content_hash, source_hash);
        assert_same(
            &original,
            &cached.index,
            true,
            &format!("{premise}: restored reuse"),
        )?;
        assert_eq!(
            assert_state(&cached.index, true, &format!("{premise}: restored reuse"))?,
            original_predicate
        );
        let restored = build_index(&fixture.0, &paths)?;
        assert_same(
            &original,
            &restored,
            true,
            &format!("{premise}: restored fresh"),
        )?;
        assert_same(
            &original,
            &reconstructed(&fixture, &files)?,
            true,
            &format!("{premise}: restored reconstructed"),
        )?;
        assert_eq!(ordinary(&restored)?, ordinary_before);
    }
    Ok(())
}

#[test]
fn original_empty_property_receipts_survive_index_paths_and_revoke_on_unchanged_source()
-> Result<(), String> {
    // Keep the original source bytes shared with the native/CLI acceptance.
    let fixture = Fixture::new()?;
    std::fs::remove_file(fixture.0.join(PROPERTY_PATH)).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(fixture.0.join("perl-test-generators/src"))
        .map_err(|error| error.to_string())?;
    let sources = [
        (
            "src/lib.rs",
            include_str!("../../../tests/fixtures/proptest_empty_string/input/src/lib.rs"),
        ),
        (
            "src/convert.rs",
            include_str!("../../../tests/fixtures/proptest_empty_string/input/src/convert.rs"),
        ),
        (
            "tests/prop_utf16_roundtrip.rs",
            include_str!(
                "../../../tests/fixtures/proptest_empty_string/input/tests/prop_utf16_roundtrip.rs"
            ),
        ),
        (
            "perl-test-generators/src/lib.rs",
            include_str!(
                "../../../tests/fixtures/proptest_empty_string/input/perl-test-generators/src/lib.rs"
            ),
        ),
        (
            "tests/ordinary.rs",
            "#[test]\nfn ordinary_witness() { assert_eq!(perl_position_tracking::ordinary_total(2), 3); }\n",
        ),
    ];
    for (path, source) in sources {
        fixture.write(path, source)?;
    }
    fixture.write(
        "Cargo.toml",
        include_str!("../../../tests/fixtures/proptest_empty_string/input/Cargo.toml"),
    )?;
    fixture.write(
        "Cargo.lock",
        include_str!("../../../tests/fixtures/proptest_empty_string/input/Cargo.lock"),
    )?;
    fixture.write(
        "perl-test-generators/Cargo.toml",
        include_str!(
            "../../../tests/fixtures/proptest_empty_string/input/perl-test-generators/Cargo.toml"
        ),
    )?;
    if !proptest_identity_available(&fixture.0) {
        return Ok(());
    }
    let files = sources
        .iter()
        .map(|(path, source)| (PathBuf::from(path), source.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    let paths = files
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let snapshot = |index: &RustIndex, admitted: bool| -> Result<Value, String> {
        ordinary(index)?;
        let empty = index
            .tests()
            .iter()
            .filter(|test| test.name == "empty_string")
            .collect::<Vec<_>>();
        assert_eq!(empty.len(), usize::from(admitted));
        assert_eq!(index.proptest_receipts.len(), usize::from(admitted));
        assert_eq!(
            index
                .files()
                .get(Path::new("tests/prop_utf16_roundtrip.rs"))
                .ok_or("missing original property file")?
                .unresolved_property_macros
                .len(),
            1,
            "unsupported siblings keep their limitation"
        );
        if let [test] = empty.as_slice() {
            assert_eq!(test.assertions.len(), 2);
            assert_eq!(
                test.assertions
                    .iter()
                    .map(|assertion| (assertion.line, assertion.text.as_str()))
                    .collect::<Vec<_>>(),
                vec![
                    (156, "assert_eq!((line, col), (0, 0));"),
                    (158, "assert_eq!(back, 0);"),
                ],
                "both original source assertions retain their own identity"
            );
            assert_eq!(index.proptest_singleton_value(test), Some(("s", "\"\"")));
            assert!(
                test.body
                    .contains("let (line, col) = offset_to_utf16_line_col(&s, 0);")
            );
            assert!(
                test.body
                    .contains("let back = utf16_line_col_to_offset(&s, line, col);")
            );
            let mut changed = (**test).clone();
            changed.body = format!("{}\n// changed fact", changed.body).into();
            assert!(
                index.proptest_singleton_value(&changed).is_none(),
                "a changed fact cannot borrow singleton authority"
            );
        }
        serde_json::to_value(index.tests().iter().collect::<Vec<_>>())
            .map_err(|error| error.to_string())
    };
    let fresh = build_index(&fixture.0, &paths)?;
    let expected = snapshot(&fresh, true)?;
    let cold =
        build_index_from_loaded_files_with_cache_and_test_harnesses(&fixture.0, &files, &[])?;
    assert_eq!(snapshot(&cold.index, true)?, expected);
    let warm =
        build_index_from_loaded_files_with_cache_and_test_harnesses(&fixture.0, &files, &[])?;
    assert_eq!(warm.file_fact_cache.hits, files.len());
    assert_eq!(snapshot(&warm.index, true)?, expected);
    let raw = parse_loaded_files_with_cache(&fixture.0, &files)?;
    assert!(raw.proptest_receipts.is_empty());
    assert!(raw.tests().iter().all(|test| test.name != "empty_string"));
    let wire = serde_json::to_value(&raw).map_err(|error| error.to_string())?;
    let mut reconstructed: RustIndex =
        serde_json::from_value(wire).map_err(|error| error.to_string())?;
    assert!(reconstructed.proptest_receipts.is_empty());
    normalize_reconstructed_source_roles(&mut reconstructed, &fixture.0, &[], None)?;
    compose_fixture_source_roles(&mut reconstructed, &fixture.0);
    reconstructed.finalize()?;
    assert_eq!(snapshot(&reconstructed, true)?, expected);
    let mut reused = warm;
    let source_hash = reused.files_content_hash.clone();
    fixture.write(".cargo/config.toml", "[env]\nPROPTEST_CASES='0'\n")?;
    reused = reuse_cached_index_with_loaded_files(reused, &fixture.0, &files, &[])?;
    assert_eq!(reused.files_content_hash, source_hash);
    snapshot(&reused.index, false)?;
    std::fs::remove_file(fixture.0.join(".cargo/config.toml"))
        .map_err(|error| error.to_string())?;
    reused = reuse_cached_index_with_loaded_files(reused, &fixture.0, &files, &[])?;
    assert_eq!(reused.files_content_hash, source_hash);
    assert_eq!(snapshot(&reused.index, true)?, expected);
    Ok(())
}
