//! A diagnostic is bound to an invocation generation, never a cached fact.
use super::*;
use crate::analysis::facts::index::{FileData, IndexedFileFacts};
use std::path::{Path, PathBuf};

const FILE: &str = "tests/boundary.rs";
const NAME: &str = "proptest::proptest";
const SOURCE: &str = "pub fn placeholder() {}\n";

fn seeded_index() -> RustIndex {
    let mut index = RustIndex::default();
    index.files.insert(
        PathBuf::from(FILE),
        IndexedFileFacts {
            data: FileData {
                path: PathBuf::from(FILE),
                source: Arc::from(SOURCE),
                ..FileData::default()
            },
            ..IndexedFileFacts::default()
        },
    );
    index.proptest_metadata_failures.insert(
        (PathBuf::from(FILE), 3, NAME.to_string()),
        ProptestMetadataFailure {
            failure: MetadataProbeFailure::UnsuccessfulExit(Some(101)),
            source: Arc::from(SOURCE),
        },
    );
    index
}

#[test]
fn metadata_failure_requires_exact_invocation_and_current_source() -> Result<(), String> {
    let mut index = seeded_index();
    assert_eq!(
        index.proptest_metadata_failure(Path::new(FILE), 3, NAME),
        Some(MetadataProbeFailure::UnsuccessfulExit(Some(101)))
    );
    for (file, line, name) in [
        ("tests/other.rs", 3, NAME),
        (FILE, 4, NAME),
        (FILE, 3, "quickcheck::quickcheck"),
    ] {
        assert_eq!(
            index.proptest_metadata_failure(Path::new(file), line, name),
            None
        );
    }
    index
        .files
        .get_mut(Path::new(FILE))
        .ok_or("missing fixture file")?
        .data
        .source = Arc::from("pub fn changed() {}\n");
    assert_eq!(
        index.proptest_metadata_failure(Path::new(FILE), 3, NAME),
        None
    );
    Ok(())
}

#[test]
fn metadata_failure_is_not_serialized_as_index_or_parser_authority() -> Result<(), String> {
    let mut index = seeded_index();
    let with_failure = serde_json::to_value(&index).map_err(|error| error.to_string())?;
    index.proptest_metadata_failures.clear();
    let without_failure = serde_json::to_value(&index).map_err(|error| error.to_string())?;
    assert_eq!(with_failure, without_failure);
    assert!(
        index.tests().is_empty(),
        "diagnosis must not manufacture a test"
    );
    Ok(())
}

#[test]
fn removal_and_registration_rebuild_retire_previous_failure() -> Result<(), String> {
    let mut index = seeded_index();
    assert!(index.remove_file(Path::new(FILE)));
    assert!(index.proptest_metadata_failures.is_empty());
    assert_eq!(
        index.proptest_metadata_failure(Path::new(FILE), 3, NAME),
        None
    );

    // With no supported source in this generation, registration performs no
    // provider probe and must still clear the old context unconditionally.
    let mut index = seeded_index();
    register_proptest_tests(&mut index, Path::new("unused-root"))?;
    assert!(index.proptest_metadata_failures.is_empty());
    assert!(index.tests().is_empty());
    Ok(())
}
