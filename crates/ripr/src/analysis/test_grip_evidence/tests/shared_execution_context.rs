use super::*;

#[test]
fn collection_grip_requires_assertion_trust_in_every_shared_context() -> Result<(), String> {
    for (main, expected) in [
        ("mod shared; fn main() {}", StageState::Yes),
        (
            "macro_rules! assert_eq { ($($t:tt)*) => {} } mod shared; fn main() {}",
            StageState::No,
        ),
    ] {
        let file = PathBuf::from("src/shared.rs");
        let index = index_from_files(&[
            (PathBuf::from("src/lib.rs"), "mod shared;"),
            (PathBuf::from("src/main.rs"), main),
            (
                file.clone(),
                "pub fn record(out: &mut Vec<u32>) { out.push(3); }\n#[cfg(test)] mod tests { use super::*; #[test] fn pin() { let mut out = Vec::new(); record(&mut out); assert_eq!(out, Vec::from([3])); } }",
            ),
        ])?;
        let seams = inventory_seams_from_index(std::slice::from_ref(&file), &index);
        let selected: Vec<_> = seams
            .iter()
            .filter(|seam| {
                matches!(seam.kind(), SeamKind::SideEffect | SeamKind::CallPresence)
                    && seam.expression().trim().trim_end_matches(';') == "out.push(3)"
            })
            .collect();
        assert_eq!(selected.len(), 2, "nonempty intended effect seams");
        let compact = CompactGripContext::new(&index);
        for seam in selected {
            let evidence = evidence_for_seam(seam, &index);
            assert_eq!(evidence.related_tests.len(), 1);
            assert_eq!(
                evidence.related_tests[0].oracle_strength,
                if expected == StageState::Yes {
                    OracleStrength::Strong
                } else {
                    OracleStrength::None
                },
                "refused assertions contribute no selected strength"
            );
            assert_eq!(
                evidence.discriminate.state, expected,
                "{main}: {evidence:?}"
            );
            assert_eq!(
                compact_evidence_for_seam(seam, &compact).discriminate.state,
                expected,
                "compact consumer: {main}"
            );
        }
    }
    Ok(())
}
