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

#[derive(Clone, Copy, Debug)]
enum Control {
    Ordinary,
    CrateVisible,
    PrivateChild,
    ShadowedImport,
    AmbiguousImport,
}

fn motion_case(
    placement: Placement,
    value: i32,
    wrong_owner: bool,
    imported: bool,
) -> Result<(AuthorityFixtureRoot, RustIndex, RepoSeam), String> {
    motion_case_with_control(placement, value, wrong_owner, imported, Control::Ordinary)
}

fn motion_case_with_control(
    placement: Placement,
    value: i32,
    wrong_owner: bool,
    imported: bool,
    control: Control,
) -> Result<(AuthorityFixtureRoot, RustIndex, RepoSeam), String> {
    let root = claim_index_fixture_root(4680)?;
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"module-motion-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    let visibility = match control {
        Control::CrateVisible => "pub(crate) ",
        Control::PrivateChild => "",
        _ => "pub(super) ",
    };
    let function = format!("{visibility}fn compute() -> i32 {{ {value} }}\n");
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
    let import = if matches!(control, Control::AmbiguousImport) {
        format!(
            "use {};\nuse super::other::compute;\n",
            call.trim_end_matches("()")
        )
    } else if imported {
        format!("use {};\n", call.trim_end_matches("()"))
    } else {
        String::new()
    };
    let call = if imported { "compute()" } else { call };
    let shadow = if matches!(control, Control::ShadowedImport) {
        "fn compute() -> i32 { 99 }\n"
    } else {
        ""
    };
    let expected = if wrong_owner || matches!(control, Control::ShadowedImport) {
        99
    } else {
        9
    };
    let source = format!(
        "{declaration}\nmod other {{ pub(super) fn compute() -> i32 {{ 99 }} }}\n\
         #[cfg(test)]\nmod tests {{\n{import}\
         #[test]\nfn exact_value() {{\n{shadow}assert_eq!({call}, {expected});\n}}\n}}\n"
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
        if matches!(control, Control::ShadowedImport) {
            3
        } else {
            2
        },
        "{placement:?}: both owner and same-named sibling must be indexed"
    );
    let seams = inventory_seams_from_index(&[PathBuf::from(owner_file)], &index);
    let expected_owner = match placement {
        Placement::Parent => "src/lib.rs::compute",
        Placement::InlineChild => "src/lib.rs::child::compute",
        Placement::FileChild => "src/child.rs::compute",
    };
    let selected = seams
        .into_iter()
        .filter(|seam| seam.kind() == SeamKind::ReturnValue && seam.owner() == expected_owner)
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
            "target_symbol": test.test_target.as_ref().map(|target| &target.symbol_id().0),
            "target_kind": test.test_target.as_ref().map(TestTargetEvidence::test_kind),
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
        let mut workspace_identity = None;
        // Two independent roots for each form also challenge absolute-root leakage.
        for _ in 0..2 {
            let (_root, index, seam) = motion_case(placement, 9, false, false)?;
            let evidence = evidence_for_seam(&seam, &index);
            assert_eq!(
                evidence.related_tests.len(),
                1,
                "{placement:?}: {evidence:?}"
            );
            let related = &evidence.related_tests[0];
            assert_eq!(related.test_name, "exact_value");
            assert_eq!(related.relation_reason, RelationReason::DirectOwnerCall);
            let target = related
                .test_target
                .as_ref()
                .ok_or("indexed test identity missing")?;
            assert_eq!(target.symbol_id().0, "src/lib.rs::tests::exact_value");
            assert_eq!(target.file(), Path::new("src/lib.rs"));
            assert_eq!(target.test_kind(), TestKind::InlineUnit);
            assert_eq!(target.provenance(), TestTargetProvenance::RustIndexFunction);
            assert_eq!(target.currentness, TestTargetCurrentness::Current);
            // This is a snapshot digest: source motion intentionally changes
            // it, but moving the same bytes to an equivalent root must not.
            if let Some(expected) = &workspace_identity {
                assert_eq!(
                    target.workspace_identity(),
                    expected,
                    "{placement:?}: root-dependent snapshot identity"
                );
            } else {
                workspace_identity = Some(target.workspace_identity().to_string());
            }
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
        let (_root, index, seam) = motion_case(placement, 9, true, false)?;
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
        // The current contradiction authority deliberately leaves qualified
        // assertion operands not evaluable. Use an explicit direct import for
        // this discriminator; the qualified relation is exercised separately.
        // First require the unchanged imported observer to establish evidence.
        let (_baseline_root, baseline_index, baseline_seam) =
            motion_case(placement, 9, false, true)?;
        let baseline = evidence_for_seam(&baseline_seam, &baseline_index);
        assert_eq!(
            baseline.discriminate.state,
            StageState::Yes,
            "{placement:?}: {baseline:?}"
        );
        assert_eq!(baseline.statically_contradicted_related_tests, 0);
        let (_root, index, seam) = motion_case(placement, 10, false, true)?;
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

#[test]
fn module_extraction_imported_sibling_stays_unrelated() -> Result<(), String> {
    for placement in PLACEMENTS {
        let (_root, index, seam) = motion_case(placement, 9, true, true)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert!(
            evidence.related_tests.is_empty(),
            "{placement:?}: {evidence:?}"
        );
    }
    Ok(())
}

#[test]
fn module_extraction_local_function_shadows_imported_owner() -> Result<(), String> {
    for placement in PLACEMENTS {
        let (_root, index, seam) =
            motion_case_with_control(placement, 9, false, true, Control::ShadowedImport)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert_ne!(
            evidence.reach.state,
            StageState::Yes,
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

#[test]
fn module_extraction_private_child_is_not_accessible_to_parent_observer() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        // Intentionally inaccessible Rust: parsing a spelled path must not
        // manufacture a reachable relation which ordinary visibility forbids.
        let (_root, index, seam) =
            motion_case_with_control(placement, 9, false, false, Control::PrivateChild)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert_ne!(
            evidence.reach.state,
            StageState::Yes,
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

#[test]
fn module_extraction_ambiguous_import_is_not_an_established_observer() -> Result<(), String> {
    for placement in PLACEMENTS {
        // Duplicate bindings are deliberately invalid/ambiguous input. A
        // parser-only analyzer must retain uncertainty instead of choosing one.
        let (_root, index, seam) =
            motion_case_with_control(placement, 9, false, true, Control::AmbiguousImport)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert_ne!(
            evidence.reach.state,
            StageState::Yes,
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

#[test]
fn module_extraction_pub_crate_keeps_existing_direct_relation() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        let (_ordinary_root, ordinary_index, ordinary_seam) =
            motion_case(placement, 9, false, false)?;
        let ordinary = evidence_for_seam(&ordinary_seam, &ordinary_index);
        let (_root, index, seam) =
            motion_case_with_control(placement, 9, false, false, Control::CrateVisible)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert_eq!(
            semantic_projection(&seam, &evidence),
            semantic_projection(&ordinary_seam, &ordinary)
        );
    }
    Ok(())
}
