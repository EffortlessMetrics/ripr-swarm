use super::*;
use crate::analysis::seam_classification::classify_seam;
use crate::domain::StaticLimitKind;

const LIMIT: StaticLimitKind = StaticLimitKind::RustFrameworkOracleUnresolved;

fn fixture_index(test_body: &str) -> Result<FixtureIndex, String> {
    let source = format!(
        "pub struct Record {{
    pub id: String,
    pub items: usize,
}}

pub fn make_record(prefix: &str, items: &[u32]) -> Record {{
    Record {{
        id: format!(\"{{}}-1\", prefix),
        items: items.iter().count(),
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;
    {test_body}
}}
"
    );
    index_from_files(&[(PathBuf::from("src/lib.rs"), source.as_str())])
}

pub(super) fn entries(test_body: &str) -> Result<Vec<(ClassifiedSeam, ClassifiedSeam)>, String> {
    let index = fixture_index(test_body)?;
    assert!(index.files().values().all(|file| !file.used_lexical_fallback));
    let context = CompactGripContext::new(&index);
    let mut entries = Vec::new();
    for seam in inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index) {
        if !seam.owner().ends_with("::make_record")
            || !matches!(seam.kind(), SeamKind::FieldConstruction | SeamKind::ReturnValue)
        {
            continue;
        }
        let full = evidence_for_seam(&seam, &index);
        let compact = compact_evidence_for_seam(&seam, &context);
        entries.push((
            ClassifiedSeam {
                class: classify_seam(&seam, &full),
                seam: seam.clone(),
                evidence: full,
            },
            ClassifiedSeam {
                class: classify_seam(&seam, &compact),
                seam,
                evidence: compact,
            },
        ));
    }
    assert_eq!(entries.len(), 3, "one return and two field seams: {entries:?}");
    Ok(entries)
}

fn assert_limits(test: &str, expected_fields: [bool; 2]) -> Result<(), String> {
    for (full, compact) in entries(test)? {
        assert_eq!(full.class, compact.class, "{}", full.seam.display_line());
        for entry in [full, compact] {
            let expected = match entry.seam.display_line() {
                7 => {
                    assert_eq!(entry.seam.kind(), SeamKind::ReturnValue);
                    false
                }
                8 => expected_fields[0],
                9 => expected_fields[1],
                line => return Err(format!("unexpected fixture seam at line {line}")),
            };
            assert_eq!(entry.evidence.names_static_limit(LIMIT), expected, "{entry:?}");
            if expected {
                assert!(matches!(
                    entry.class,
                    SeamGripClass::WeaklyGripped | SeamGripClass::ReachableUnrevealed
                ));
                assert_eq!(classify_seam(&entry.seam, &entry.evidence), entry.class);
                assert!(entry.evidence.discriminate.summary.contains("snapshot"));
            }
        }
    }
    Ok(())
}

#[test]
fn full_and_compact_field_snapshots_reuse_the_framework_reader() -> Result<(), String> {
    for body in [
        "insta::assert_json_snapshot!(record);",
        "let mut settings = insta::Settings::clone_current();
         settings.add_redaction(\".id\", \"[id]\");
         let _guard = settings.bind_to_scope();
         insta::assert_json_snapshot!(record);",
        "insta::with_settings!({redactions => vec![(\".id\", \"[id]\")]}, {
             insta::assert_json_snapshot!(record);
         });",
        "let settings = insta::Settings::clone_current();
         let _guard = settings.bind_to_scope();
         insta::assert_json_snapshot!(record, @\"inline\");",
        "const SELECTOR: &str = \".id\";
         insta::assert_json_snapshot!(record, {SELECTOR => \"[id]\"});",
    ] {
        assert_limits(
            &format!(
                "#[test] fn snapshot() {{
                    let record = make_record(\"ord\", &[1, 2]);
                    {body}
                }}"
            ),
            [true, true],
        )?;
    }
    Ok(())
}

#[test]
fn a_literal_redacted_field_remains_a_gap_while_its_sibling_is_unresolved() -> Result<(), String> {
    assert_limits(
        "#[test] fn snapshot() {
            let record = make_record(\"ord\", &[1, 2]);
            insta::assert_json_snapshot!(record, {\".id\" => \"[id]\"});
        }",
        [false, true],
    )
}

#[test]
fn unrelated_or_shadowed_snapshot_values_do_not_name_the_limit() -> Result<(), String> {
    for body in [
        "let other = Record { id: String::new(), items: 0 };
         insta::assert_json_snapshot!(other);",
        "let record = Record { id: String::new(), items: 0 };
         insta::assert_json_snapshot!(record);",
        "{ let record = Record { id: String::new(), items: 0 };
           insta::assert_json_snapshot!(record); }",
        "insta::assert_json_snapshot!(record, @\"inline\");",
        "assert!(record.items > 0);",
    ] {
        assert_limits(
            &format!(
                "#[test] fn snapshot() {{
                    let record = make_record(\"ord\", &[1, 2]);
                    {body}
                }}"
            ),
            [false, false],
        )?;
    }
    Ok(())
}

#[test]
fn noncode_configuration_words_keep_the_literal_field_gap() -> Result<(), String> {
    assert_limits(
        "#[test] fn snapshot() {
            let record = make_record(\"ord\", &[1, 2]);
            // Settings add_redaction with_settings
            let _words = \"Settings add_redaction with_settings\";
            insta::assert_json_snapshot!(record, {\".id\" => \"[id]\"});
        }",
        [false, true],
    )
}

#[test]
fn an_ordinary_test_named_like_the_limit_remains_unmarked() -> Result<(), String> {
    let test = "#[test] fn rust_framework_oracle_unresolved() {
        let record = make_record(\"ord\", &[1, 2]);
        assert!(record.items > 0);
    }";
    assert_limits(test, [false, false])?;
    for (full, compact) in entries(test)? {
        for entry in [full, compact] {
            assert_eq!(entry.class, SeamGripClass::WeaklyGripped, "{entry:?}");
            assert!(entry.evidence.reach.summary.contains(LIMIT.as_str()));
            assert!(!entry.evidence.names_static_limit(LIMIT));
        }
    }
    Ok(())
}

#[test]
fn an_independent_exact_field_assertion_keeps_strong_grip() -> Result<(), String> {
    let test = "#[test] fn exact() {
        let record = make_record(\"ord\", &[1, 2]);
        let settings = insta::Settings::clone_current();
        let _guard = settings.bind_to_scope();
        assert_eq!(record.items, 2);
        insta::assert_json_snapshot!(record);
    }";
    assert_limits(test, [true, false])?;
    let (full, compact) = entries(test)?
        .into_iter()
        .find(|(entry, _)| entry.seam.display_line() == 9)
        .ok_or("missing items field seam")?;
    for entry in [full, compact] {
        assert_eq!(entry.class, SeamGripClass::StronglyGripped, "{entry:?}");
        assert_eq!(entry.evidence.discriminate.state, StageState::Yes);
    }
    Ok(())
}

#[test]
fn compact_limit_reads_a_snapshot_witness_past_the_related_test_cap() -> Result<(), String> {
    let mut tests = String::new();
    for n in 0..COMPACT_RELATED_TEST_LIMIT + 1 {
        tests.push_str(&format!(
            "#[test] fn a_{n:02}() {{
                 let record = make_record(\"ord\", &[1, 2]);
                 assert!(record.items > 0);
             }}\n"
        ));
    }
    tests.push_str(
        "#[test] fn z_snapshot() {
             let record = make_record(\"ord\", &[1, 2]);
             insta::with_settings!({redactions => vec![(\".id\", \"[id]\")]}, {
                 insta::assert_json_snapshot!(record);
             });
         }",
    );
    // The nested macro supplies no stronger extracted oracle, so the
    // earlier weak asserting tests rank ahead of the snapshot witness.
    let index = fixture_index(&tests)?;
    let context = CompactGripContext::new(&index);
    let fields: Vec<_> = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index)
        .into_iter()
        .filter(|seam| seam.kind() == SeamKind::FieldConstruction)
        .collect();
    assert_eq!(fields.len(), 2);
    for seam in fields {
        let ranked = find_related_tests_ranked(&seam, &context);
        let position = ranked
            .iter()
            .position(|(test, reason)| {
                test.test.name == "z_snapshot" && *reason == RelationReason::DirectOwnerCall
            })
            .ok_or("missing directly related snapshot witness")?;
        assert!(position >= COMPACT_RELATED_TEST_LIMIT, "position {position}");
        let full = evidence_for_seam(&seam, &index);
        let compact = compact_evidence_for_seam(&seam, &context);
        assert!(full.names_static_limit(LIMIT), "{full:?}");
        assert!(compact.names_static_limit(LIMIT), "{compact:?}");
        assert!(compact.related_tests.is_empty());
        assert!(compact.discriminate.summary.contains("z_snapshot"));
    }
    Ok(())
}
