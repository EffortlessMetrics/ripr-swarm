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

#[test]
fn repository_provider_failure_uses_the_same_source_bound_diagnosis() -> Result<(), String> {
    use crate::analysis::seam_classification::{ClassifiedSeam, classify_seam};
    use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
    use crate::analysis::test_grip_evidence::{self, CompactGripContext};
    use crate::domain::StageState;
    const PROD: &str = "pub fn discounted_total(amount: i32, threshold: i32) -> i32 { if amount >= threshold { amount - 10 } else { amount } }\n";
    const PROPERTY: &str = "proptest::proptest! { #[test] fn exact_boundary(amount in proptest::strategy::Just(100i32)) { proptest::prop_assert_eq!(lib_proptest::discounted_total(amount, 100), 90); } }\n";
    for ordinary in [false, true] {
        let production = if ordinary {
            format!(
                "{PROD}#[test] fn independent_ordinary() {{ assert_eq!(discounted_total(100, 100), 90); }}\n"
            )
        } else {
            PROD.to_string()
        };
        let files = vec![
            (PathBuf::from("src/lib.rs"), production.as_bytes().to_vec()),
            (PathBuf::from(FILE), PROPERTY.as_bytes().to_vec()),
        ];
        let mut index = crate::analysis::facts::build_analysis_index_from_loaded_files(
            Path::new("unused-provider-unit-root"),
            &files,
            &[],
        )?
        .index;
        let witness = index
            .files()
            .get(Path::new(FILE))
            .ok_or("property file")?
            .data()
            .unresolved_property_macros
            .first()
            .ok_or("property witness")?
            .clone();
        index.proptest_metadata_failures.insert(
            (PathBuf::from(FILE), witness.line, witness.name.clone()),
            ProptestMetadataFailure {
                failure: MetadataProbeFailure::UnsuccessfulExit(Some(101)),
                source: Arc::from(PROPERTY),
            },
        );
        let seam = RepoSeam::new(
            "src/lib.rs",
            "src/lib.rs::discounted_total",
            SeamKind::PredicateBoundary,
            70,
            1,
            "amount >= threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount == threshold".into(),
            },
            ExpectedSink::ReturnValue,
        );
        let context = CompactGripContext::new(&index);
        for evidence in [
            test_grip_evidence::evidence_for_seam(&seam, &index),
            test_grip_evidence::compact_evidence_for_seam(&seam, &context),
        ] {
            if ordinary {
                assert_eq!(
                    evidence.reach.state,
                    StageState::Yes,
                    "independent ordinary test remains authoritative"
                );
                assert!(
                    !evidence
                        .reach
                        .summary
                        .contains("offline provider verification")
                );
                continue;
            }
            assert_eq!(evidence.reach.state, StageState::Opaque);
            assert!(
                evidence
                    .reach
                    .summary
                    .contains("rust_macro_reach_unresolved")
            );
            assert!(evidence.reach.summary.contains("status 101"));
            assert!(evidence.reach.summary.contains(
                "cargo metadata --offline --locked --format-version 1 --manifest-path Cargo.toml"
            ));
            assert!(
                evidence.related_tests.is_empty()
                    && evidence.observed_values.is_empty()
                    && evidence.missing_discriminators.is_empty()
            );
            assert!(evidence.new_test_target.is_none());
            for stage in [
                &evidence.activate,
                &evidence.propagate,
                &evidence.observe,
                &evidence.discriminate,
            ] {
                assert_eq!(stage.state, StageState::Unknown);
            }
            let entry = ClassifiedSeam {
                seam: seam.clone(),
                class: classify_seam(&seam, &evidence),
                evidence,
            };
            let record = crate::output::evidence_record::evidence_record_for(&entry, None);
            assert_eq!(record.actionability.class, "static_limitation");
            assert!(!record.actionability.has_concrete_guidance);
            assert!(
                record
                    .static_limitations
                    .iter()
                    .any(|limit| limit.reason.contains("same analysis root and environment"))
            );
        }
        // The authority is the exact invocation's current source, not a stale failure.
        index
            .files
            .get_mut(Path::new(FILE))
            .ok_or("fixture")?
            .data
            .source = Arc::from("// changed generation\n");
        let context = CompactGripContext::new(&index);
        let evidence = test_grip_evidence::compact_evidence_for_seam(&seam, &context);
        assert!(
            !evidence
                .reach
                .summary
                .contains("offline provider verification")
        );
    }
    Ok(())
}
