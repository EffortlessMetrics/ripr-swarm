use super::*;
use crate::analysis::classify::insta_field_snapshot_limitation;
use crate::analysis::seam_classification::classify_seam;
use crate::domain::StaticLimitKind;
use crate::output::evidence_record::static_limitations_for;

const PANIC: StaticLimitKind = StaticLimitKind::RustPanicOutcomeUnresolved;
const FRAMEWORK: StaticLimitKind = StaticLimitKind::RustFrameworkOracleUnresolved;

#[test]
fn ordinary_panic_token_test_name_stays_unmarked_in_full_and_compact() -> Result<(), String> {
    for name in ["ordinary_weak_assertion", PANIC.as_str()] {
        let source = format!(
            "#[test] fn {name}() {{
                let record = make_record(\"ord\", &[1, 2]);
                assert!(record.items > 0);
            }}"
        );
        for (full, compact) in super::framework_snapshot::entries(&source)? {
            for entry in [full, compact] {
                assert_eq!(entry.class, SeamGripClass::WeaklyGripped, "{entry:?}");
                assert!(entry.evidence.reach.summary.contains(name));
                assert!(!entry.evidence.names_static_limit(PANIC));
                assert!(!entry.evidence.names_static_limit(FRAMEWORK));
                for stage in [
                    &entry.evidence.reach,
                    &entry.evidence.activate,
                    &entry.evidence.propagate,
                    &entry.evidence.observe,
                ] {
                    assert_eq!(stage.state, StageState::Yes, "{entry:?}");
                }
                assert_eq!(entry.evidence.discriminate.state, StageState::Weak);
                assert!(
                    static_limitations_for(&entry)
                        .iter()
                        .all(|limit| limit.category != PANIC.as_str())
                );
            }
        }
    }
    Ok(())
}

#[test]
fn full_and_compact_preserve_real_panic_and_weak_assertion_control() -> Result<(), String> {
    let lib = "pub fn div(a: i32, b: i32) -> i32 {
    if b == 1 {
        panic!(\"zero\");
    }
    a / b
}
";
    for (extra, expected) in [
        ("", true),
        ("#[test] fn ok() { assert!(div(4, 2) > 0); }", false),
    ] {
        let tests = format!(
            "use memory_fixture::div;
#[test]
#[should_panic]
fn t() {{ div(1, 0); }}
{extra}
"
        );
        let index = index_from_files(&[
            (PathBuf::from("src/lib.rs"), lib),
            (PathBuf::from("tests/t.rs"), tests.as_str()),
        ])?;
        assert!(index.files().values().all(|file| !file.used_lexical_fallback));
        let context = CompactGripContext::new(&index);
        let seams: Vec<_> = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index)
            .into_iter()
            .filter(|seam| seam.kind() == SeamKind::PredicateBoundary && seam.display_line() == 2)
            .collect();
        assert_eq!(seams.len(), 1, "one intended div predicate");
        let seam = &seams[0];
        for evidence in [
            evidence_for_seam(seam, &index),
            compact_evidence_for_seam(seam, &context),
        ] {
            assert_eq!(evidence.names_static_limit(PANIC), expected, "{evidence:?}");
            let entry = ClassifiedSeam {
                class: classify_seam(seam, &evidence),
                seam: seam.clone(),
                evidence,
            };
            assert_eq!(
                entry.class,
                if expected {
                    SeamGripClass::ReachableUnrevealed
                } else {
                    SeamGripClass::WeaklyGripped
                }
            );
            let named: Vec<_> = static_limitations_for(&entry)
                .into_iter()
                .filter(|limit| limit.category == PANIC.as_str())
                .collect();
            assert_eq!(named.len(), usize::from(expected));
            if expected {
                assert_eq!(named[0].stage, "discriminate");
                assert_eq!(named[0].state, "no");
                assert_eq!(named[0].reason, entry.evidence.discriminate.summary);
                assert!(named[0].reason.starts_with(&format!("{}: ", PANIC.as_str())));
                assert!(named[0].reason.contains("passes only if something panics"));
            }
        }
    }
    Ok(())
}

#[test]
fn framework_uncertainty_preserves_an_existing_real_panic_marker() -> Result<(), String> {
    let source = "pub struct Record { pub items: usize }
pub fn make_record(items: &[u32]) -> Record {
    let record = Record {
        items: items.iter().count(),
    };
    panic!(\"unfinished\");
    record
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[should_panic]
    fn snapshot_panicking_record() {
        let record = make_record(&[1, 2]);
        insta::with_settings!({redactions => vec![(\".other\", \"[other]\")]}, {
            insta::assert_json_snapshot!(record);
        });
    }
}
";
    let index = index_from_files(&[(PathBuf::from("src/lib.rs"), source)])?;
    assert!(index.files().values().all(|file| !file.used_lexical_fallback));
    let owner = index
        .functions()
        .iter()
        .find(|owner| owner.name == "make_record")
        .ok_or("missing record owner")?;
    let test = index
        .tests()
        .iter()
        .find(|test| test.name == "snapshot_panicking_record")
        .ok_or("missing combined panic and snapshot test")?;
    assert!(
        insta_field_snapshot_limitation(test, "items: items.iter().count(),", &owner.name).is_some()
    );
    let seams: Vec<_> = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index)
        .into_iter()
        .filter(|seam| seam.kind() == SeamKind::FieldConstruction && seam.display_line() == 4)
        .collect();
    assert_eq!(seams.len(), 1, "one intended field seam");
    let seam = &seams[0];
    let context = CompactGripContext::new(&index);
    for mut evidence in [
        evidence_for_seam(seam, &index),
        compact_evidence_for_seam(seam, &context),
    ] {
        assert_eq!(
            classify_seam(seam, &evidence),
            SeamGripClass::ReachableUnrevealed
        );
        assert!(evidence.names_static_limit(PANIC), "{evidence:?}");
        assert!(!evidence.names_static_limit(FRAMEWORK));
        assert!(evidence.discriminate.summary.starts_with(&format!("{}: ", PANIC.as_str())));
        assert!(evidence.discriminate.summary.contains("snapshot_panicking_record"));
        let original = serde_json::to_value(&evidence).map_err(|error| error.to_string())?;
        assert!(!super::super::framework_snapshot::withhold(
            seam,
            &mut evidence,
            Some(owner),
            [test]
        ));
        assert_eq!(
            serde_json::to_value(&evidence).map_err(|error| error.to_string())?,
            original
        );
    }
    Ok(())
}
