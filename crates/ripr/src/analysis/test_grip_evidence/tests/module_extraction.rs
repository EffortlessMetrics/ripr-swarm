//! Production-index probes for the bounded direct relation in #4680.

use super::*;

#[derive(Clone, Copy, Debug)]
enum Placement {
    Parent,
    InlineChild,
    FileChild,
}

const PLACEMENTS: [Placement; 3] = [
    Placement::Parent,
    Placement::InlineChild,
    Placement::FileChild,
];

fn motion_case(
    placement: Placement,
    value: i32,
    wrong_owner: bool,
) -> Result<(AuthorityFixtureRoot, RustIndex, RepoSeam), String> {
    let root = claim_index_fixture_root(4680)?;
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"module-motion-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    let function = format!("pub(super) fn compute() -> i32 {{ {value} }}\n");
    let (declaration, call, owner_file) = match placement {
        Placement::Parent => (
            format!("fn compute() -> i32 {{ {value} }}\n"),
            "super::compute()",
            "src/lib.rs",
        ),
        Placement::InlineChild => (
            format!("mod child {{\n{function}}}\n"),
            "super::child::compute()",
            "src/lib.rs",
        ),
        Placement::FileChild => {
            fs::write(root.join("src/child.rs"), function).map_err(|error| error.to_string())?;
            (
                "mod child;\n".to_string(),
                "super::child::compute()",
                "src/child.rs",
            )
        }
    };
    let call = if wrong_owner {
        "super::other::compute()"
    } else {
        call
    };
    let expected = if wrong_owner { 99 } else { 9 };
    let source = format!(
        "{declaration}\nmod other {{ pub(super) fn compute() -> i32 {{ 99 }} }}\n\
         #[cfg(test)]\nmod tests {{\n\
         #[test]\nfn exact_value() {{ assert_eq!({call}, {expected}); }}\n}}\n"
    );
    fs::write(root.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let mut files = vec![PathBuf::from("src/lib.rs")];
    if matches!(placement, Placement::FileChild) {
        files.push(PathBuf::from("src/child.rs"));
    }
    // Use the retained disk-backed producer, including path and role composition.
    // The manual index_from_files helper would bypass those production steps.
    let index = build_index(&root, &files)?;
    assert_eq!(
        index.tests().len(),
        1,
        "{placement:?}: fixture test missing"
    );
    assert_eq!(
        index
            .functions()
            .iter()
            .filter(|f| f.name == "compute")
            .count(),
        2,
        "{placement:?}: both owner and same-named sibling must be indexed"
    );
    let seams = inventory_seams_from_index(&[PathBuf::from(owner_file)], &index);
    let selected = seams
        .into_iter()
        .filter(|seam| {
            seam.kind() == SeamKind::ReturnValue
                && seam.owner().ends_with("::compute")
                && !seam.owner().contains("::other::")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        1,
        "{placement:?}: exact owner return seam: {selected:?}"
    );
    let seam = selected.into_iter().next().ok_or("missing owner seam")?;
    Ok((root, index, seam))
}

fn semantic_projection(seam: &RepoSeam, evidence: &TestGripEvidence) -> serde_json::Value {
    // Location-bearing seam/symbol IDs deliberately change with extraction.
    // Pin semantic fields explicitly; never normalize arbitrary output strings.
    serde_json::json!({
        "owner_name": seam.owner().rsplit("::").next(),
        "kind": seam.kind(),
        "expression": seam.expression(),
        "required_discriminator": seam.required_discriminator(),
        "expected_sink": seam.expected_sink(),
        "related_tests": evidence.related_tests.iter().map(|test| serde_json::json!({
            "name": test.test_name,
            "reason": test.relation_reason,
            "confidence": test.relation_confidence,
            "oracle_kind": test.oracle_kind,
            "oracle_strength": test.oracle_strength,
        })).collect::<Vec<_>>(),
        "reach": evidence.reach.state,
        "activate": evidence.activate.state,
        "propagate": evidence.propagate.state,
        "observe": evidence.observe.state,
        "discriminate": evidence.discriminate.state,
        "contradictions": evidence.statically_contradicted_related_tests,
    })
}

#[test]
fn module_extraction_parent_inline_and_file_child_keep_direct_observer() -> Result<(), String> {
    let mut baseline = None;
    for placement in PLACEMENTS {
        // Two independent roots for each form also challenge absolute-root leakage.
        for _ in 0..2 {
            let (_root, index, seam) = motion_case(placement, 9, false)?;
            let evidence = evidence_for_seam(&seam, &index);
            assert_eq!(
                evidence.related_tests.len(),
                1,
                "{placement:?}: {evidence:?}"
            );
            let related = &evidence.related_tests[0];
            assert_eq!(related.test_name, "exact_value");
            assert_eq!(related.relation_reason, RelationReason::DirectOwnerCall);
            assert_eq!(
                evidence.reach.state,
                StageState::Yes,
                "{placement:?}: {evidence:?}"
            );
            assert_eq!(
                evidence.discriminate.state,
                StageState::Yes,
                "{placement:?}: {evidence:?}"
            );
            let projection = semantic_projection(&seam, &evidence);
            if let Some(expected) = &baseline {
                assert_eq!(
                    &projection, expected,
                    "{placement:?}: pure motion changed evidence"
                );
            } else {
                baseline = Some(projection);
            }
        }
    }
    Ok(())
}

#[test]
fn module_extraction_same_named_sibling_cannot_observe_owner() -> Result<(), String> {
    for placement in PLACEMENTS {
        let (_root, index, seam) = motion_case(placement, 9, true)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert!(
            evidence.related_tests.is_empty(),
            "{placement:?}: {evidence:?}"
        );
        assert_ne!(
            evidence.reach.state,
            StageState::Yes,
            "{placement:?}: {evidence:?}"
        );
    }
    Ok(())
}

#[test]
fn module_extraction_changed_value_keeps_semantic_discriminator() -> Result<(), String> {
    for placement in PLACEMENTS {
        let (_root, index, seam) = motion_case(placement, 10, false)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert_eq!(
            evidence.statically_contradicted_related_tests, 1,
            "{placement:?}: {evidence:?}"
        );
        assert_ne!(
            evidence.discriminate.state,
            StageState::Yes,
            "{placement:?}: {evidence:?}"
        );
    }
    Ok(())
}
