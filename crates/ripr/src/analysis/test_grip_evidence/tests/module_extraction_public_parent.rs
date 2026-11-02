//! #4681: public parent entrypoint delegates to a private helper; tests call the parent.

use super::*;
use crate::analysis::facts::build_index;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
enum HelperPlacement {
    ParentInline,
    InlineChild,
    FileChild,
}

const HELPER_PLACEMENTS: [HelperPlacement; 3] = [
    HelperPlacement::FileChild,
    HelperPlacement::InlineChild,
    HelperPlacement::ParentInline,
];

fn public_parent_fixture(
    placement: HelperPlacement,
    helper_value: i32,
    wrong_sibling: bool,
    test_calls_wrapper: bool,
) -> Result<(AuthorityFixtureRoot, RustIndex, RepoSeam), String> {
    let root = claim_index_fixture_root(4681)?;
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"public-parent-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;

    let helper_body = format!("fn compute_value() -> i32 {{ {helper_value} }}\n");
    let (lib_extra, helper_call, owner_file, expected_owner) = match placement {
        HelperPlacement::ParentInline => (
            helper_body.clone(),
            "compute_value()",
            "src/lib.rs",
            "src/lib.rs::compute_value",
        ),
        HelperPlacement::InlineChild => (
            format!("mod child {{\n{helper_body}}}"),
            "child::compute_value()",
            "src/lib.rs",
            "src/lib.rs::child::compute_value",
        ),
        HelperPlacement::FileChild => {
            fs::write(root.join("src/child.rs"), helper_body).map_err(|error| error.to_string())?;
            (
                "mod child;\n".to_string(),
                "child::compute_value()",
                "src/child.rs",
                "src/child.rs::compute_value",
            )
        }
    };

    let sibling = if wrong_sibling {
        "mod rival { pub(super) fn compute_value() -> i32 { 99 } }\n"
    } else {
        ""
    };
    let test_call = if test_calls_wrapper {
        "assert_eq!(super::entry(), 9);"
    } else {
        "assert_eq!(super::rival::compute_value(), 99);"
    };

    let lib = format!(
        "{lib_extra}{sibling}\
pub fn entry() -> i32 {{ {helper_call} }}\n\
#[cfg(test)] mod tests {{\n\
#[test]\nfn observes_public_parent() {{ {test_call} }}\n\
}}\n"
    );
    fs::write(root.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;

    let mut files = vec![PathBuf::from("src/lib.rs")];
    if matches!(placement, HelperPlacement::FileChild) {
        files.push(PathBuf::from("src/child.rs"));
    }
    let index = build_index(&root, &files)?;
    assert_eq!(index.tests().len(), 1, "{placement:?}");
    let seams = inventory_seams_from_index(&[PathBuf::from(owner_file)], &index);
    let selected = seams
        .into_iter()
        .filter(|seam| seam.kind() == SeamKind::ReturnValue && seam.owner() == expected_owner)
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1, "{placement:?}: seams={selected:?}");
    Ok((
        root,
        index,
        selected
            .into_iter()
            .next()
            .ok_or_else(|| format!("missing owner seam for {placement:?}"))?,
    ))
}

#[test]
fn module_extraction_public_parent_inline_and_file_child_keep_helper_witness() -> Result<(), String>
{
    let mut baseline = None;
    for placement in HELPER_PLACEMENTS {
        let (_root, index, seam) = public_parent_fixture(placement, 9, false, true)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert_eq!(
            evidence.related_tests.len(),
            1,
            "{placement:?}: {evidence:?}"
        );
        let related = &evidence.related_tests[0];
        assert_eq!(related.test_name, "observes_public_parent");
        assert_eq!(
            related.relation_reason,
            RelationReason::HelperOwnerCall,
            "{placement:?}: {evidence:?}"
        );
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
        let projection = semantic_projection_public_parent(&seam, &evidence);
        if let Some(expected) = &baseline {
            assert_eq!(&projection, expected, "{placement:?}: pure motion drift");
        } else {
            baseline = Some(projection);
        }
    }
    Ok(())
}

fn semantic_projection_public_parent(
    seam: &RepoSeam,
    evidence: &TestGripEvidence,
) -> serde_json::Value {
    serde_json::json!({
        "owner_name": seam.owner().rsplit("::").next(),
        "kind": seam.kind(),
        "related_tests": evidence.related_tests.iter().map(|test| serde_json::json!({
            "name": test.test_name,
            "reason": test.relation_reason,
        })).collect::<Vec<_>>(),
        "reach": evidence.reach.state,
        "discriminate": evidence.discriminate.state,
        "contradictions": evidence.statically_contradicted_related_tests,
    })
}

#[test]
fn module_extraction_public_parent_same_named_sibling_stays_unrelated() -> Result<(), String> {
    for placement in HELPER_PLACEMENTS {
        let (_root, index, seam) = public_parent_fixture(placement, 9, true, false)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert!(
            evidence
                .related_tests
                .iter()
                .all(|test| test.relation_reason != RelationReason::HelperOwnerCall),
            "{placement:?}: {evidence:?}"
        );
        assert_ne!(evidence.reach.state, StageState::Yes, "{placement:?}");
    }
    Ok(())
}

#[test]
fn module_extraction_public_parent_changed_value_keeps_discriminator() -> Result<(), String> {
    for placement in HELPER_PLACEMENTS {
        let (_baseline_root, baseline_index, baseline_seam) =
            public_parent_fixture(placement, 9, false, true)?;
        let baseline = evidence_for_seam(&baseline_seam, &baseline_index);
        assert_eq!(baseline.discriminate.state, StageState::Yes);
        let (_root, index, seam) = public_parent_fixture(placement, 10, false, true)?;
        let evidence = evidence_for_seam(&seam, &index);
        assert_eq!(evidence.statically_contradicted_related_tests, 1);
        assert_ne!(evidence.discriminate.state, StageState::Yes);
    }
    Ok(())
}

#[test]
fn module_extraction_public_parent_controls() -> Result<(), String> {
    module_extraction_public_parent_inline_and_file_child_keep_helper_witness()?;
    module_extraction_public_parent_same_named_sibling_stays_unrelated()?;
    module_extraction_public_parent_changed_value_keeps_discriminator()?;
    Ok(())
}
