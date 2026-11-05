use super::*;
use crate::analysis::classify::{
    BooleanComparisonReceipt, OwnerPinSyntax, boolean_comparison_receipt,
};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/rust-verdict-corpus/subjects/authored-mined-wire/src/lib.rs.txt"
));

#[test]
fn ordinary_library_settings_revoke_and_restore_the_receipt() -> Result<(), String> {
    let source = format!("{SOURCE}\nfn main() {{}}\n");
    let path = PathBuf::from("src/lib.rs");
    let mut index = index_from_files(&[(path, source.as_str())])?;
    let root = index
        .workspace_authority
        .as_ref()
        .ok_or("workspace")?
        .root
        .clone();
    let manifest =
        fs::read_to_string(root.join("Cargo.toml")).map_err(|error| error.to_string())?;
    let syntax = OwnerPinSyntax::default();
    let has_receipt = |index: &RustIndex| -> Result<bool, String> {
        let functions = index.functions();
        let owner = functions
            .iter()
            .find(|owner| owner.name == "read_u32")
            .ok_or("owner")?;
        let tests = index.tests();
        let test = tests
            .iter()
            .find(|test| test.name == "u32_reads_what_was_written")
            .ok_or("test")?;
        Ok(boolean_comparison_receipt(owner, test, index, &syntax).is_some())
    };
    assert!(has_receipt(&index)?);
    fs::write(
        root.join("Cargo.toml"),
        format!("{manifest}\n[lib]\nharness=false\n"),
    )
    .map_err(|error| error.to_string())?;
    assert!(
        !has_receipt(&index)?,
        "old indexed manifest cannot reuse warm target credit"
    );
    for (settings, expected) in [
        ("\n[lib]\nharness=false\n", false),
        ("\n[lib]\ntest=false\n", false),
        ("", true),
    ] {
        fs::write(root.join("Cargo.toml"), format!("{manifest}{settings}"))
            .map_err(|error| error.to_string())?;
        index.index.workspace_authority = Some(WorkspaceRootAuthority::from_sources(
            &root,
            index
                .files()
                .iter()
                .map(|(path, facts)| (path, facts.data().source.as_ref())),
        ));
        assert_eq!(has_receipt(&index)?, expected, "{settings}");
    }
    Ok(())
}

#[test]
fn root_default_member_changes_revoke_a_warm_helper_receipt() -> Result<(), String> {
    let source = format!("{SOURCE}\nfn main() {{}}\n");
    let mut index = index_from_files(&[(PathBuf::from("b/src/lib.rs"), source.as_str())])?;
    let root = index
        .workspace_authority
        .as_ref()
        .ok_or("workspace")?
        .root
        .clone();
    for member in ["a", "b"] {
        let directory = root.join(member);
        fs::create_dir_all(directory.join("src")).map_err(|error| error.to_string())?;
        fs::write(
            directory.join("Cargo.toml"),
            format!("[package]\nname='{member}'\nversion='0.1.0'\nedition='2021'\n"),
        )
        .map_err(|error| error.to_string())?;
    }
    fs::write(root.join("a/src/lib.rs"), "").map_err(|error| error.to_string())?;
    let selected = "[workspace]\nmembers=['a','b']\ndefault-members=['b']\nresolver='2'\n";
    let unselected = "[workspace]\nmembers=['a','b']\ndefault-members=['a']\nresolver='2'\n";
    fs::write(root.join("Cargo.toml"), selected).map_err(|error| error.to_string())?;
    let authority = || {
        WorkspaceRootAuthority::from_sources(
            &root,
            [(PathBuf::from("b/src/lib.rs"), source.as_str())]
                .iter()
                .map(|(path, source)| (path, *source)),
        )
    };
    index.index.workspace_authority = Some(authority());
    let original_identity = index
        .workspace_authority
        .as_ref()
        .ok_or("workspace")?
        .workspace_identity
        .clone();
    let syntax = OwnerPinSyntax::default();
    let has_receipt = |index: &RustIndex| -> Result<bool, String> {
        let functions = index.functions();
        let owner = functions
            .iter()
            .find(|owner| owner.name == "read_u32")
            .ok_or("owner")?;
        let tests = index.tests();
        let test = tests
            .iter()
            .find(|test| test.name == "u32_reads_what_was_written")
            .ok_or("test")?;
        Ok(boolean_comparison_receipt(owner, test, index, &syntax).is_some())
    };
    assert!(has_receipt(&index)?);
    fs::write(root.join("Cargo.toml"), unselected).map_err(|error| error.to_string())?;
    assert!(
        !has_receipt(&index)?,
        "root selector change must revoke the warm target"
    );
    index.index.workspace_authority = Some(authority());
    assert_eq!(
        index
            .workspace_authority
            .as_ref()
            .ok_or("workspace")?
            .workspace_identity,
        original_identity
    );
    assert!(
        !has_receipt(&index)?,
        "nearest-manifest identity alone is insufficient"
    );
    fs::write(root.join("Cargo.toml"), selected).map_err(|error| error.to_string())?;
    assert!(
        has_receipt(&index)?,
        "exact selector restoration restores admission"
    );
    Ok(())
}

#[test]
fn same_asserted_helper_receipt_owns_repository_return_evidence() -> Result<(), String> {
    let path = PathBuf::from("src/lib.rs");
    let index = index_from_files(&[(path.clone(), SOURCE)])?;
    let seams = inventory_seams_from_index(&[path], &index);
    let seam = seams
        .iter()
        .find(|seam| {
            seam.kind() == SeamKind::ReturnValue
                && seam.expression().starts_with("u32::from_be_bytes")
        })
        .ok_or("exact u32 return seam")?;
    let evidence = evidence_for_seam(seam, &index);
    assert_eq!(evidence.activate.state, StageState::Yes, "{evidence:?}");
    assert_eq!(evidence.propagate.state, StageState::Yes, "{evidence:?}");
    assert_eq!(evidence.discriminate.state, StageState::Yes, "{evidence:?}");
    assert_eq!(
        crate::analysis::seam_classification::classify_seam(seam, &evidence),
        SeamGripClass::StronglyGripped,
        "{evidence:?}"
    );
    assert!(
        evidence
            .propagate
            .summary
            .contains("asserted_u32_helper_comparison"),
        "{evidence:?}"
    );
    let inputs: Vec<_> = evidence
        .observed_values
        .iter()
        .filter(|fact| fact.value.starts_with("bytes = "))
        .collect();
    assert_eq!(inputs.len(), 4, "{inputs:?}");
    let compact_context = CompactGripContext::new(&index);
    let compact = compact_evidence_for_seam(seam, &compact_context);
    assert_eq!(compact.reach.state, evidence.reach.state);
    assert_eq!(compact.activate.state, evidence.activate.state);
    assert_eq!(compact.propagate.state, evidence.propagate.state);
    assert_eq!(compact.discriminate.state, evidence.discriminate.state);
    assert!(
        compact.observed_values.is_empty(),
        "compact output deliberately omits input payloads"
    );
    Ok(())
}

#[test]
fn comparison_receipt_requires_live_source_owner_and_exact_asserted_identity() -> Result<(), String>
{
    let index = index_from_files(&[(PathBuf::from("src/lib.rs"), SOURCE)])?;
    let functions = index.functions();
    let owner = functions
        .iter()
        .find(|function| function.name == "read_u32")
        .ok_or("owner")?;
    let tests = index.tests();
    let test = tests
        .iter()
        .find(|test| test.name == "u32_reads_what_was_written")
        .ok_or("test")?;
    let syntax = OwnerPinSyntax::default();
    let receipt =
        boolean_comparison_receipt(owner, test, &index, &syntax).ok_or("source receipt")?;
    let assertion = receipt.oracle();
    assert!(receipt.matches_source(
        &owner.file,
        26,
        "u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])"
    ));
    assert!(receipt.admits(test, &assertion));
    let workspace_root = &index
        .workspace_authority
        .as_ref()
        .ok_or("workspace root")?
        .root;
    let expression = "u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])";
    for path in [
        owner.file.clone(),
        PathBuf::from("./src/lib.rs"),
        workspace_root.join(&owner.file),
    ] {
        assert!(receipt.matches_source(&path, 26, expression), "{path:?}");
    }
    for path in [
        PathBuf::from("src/other.rs"),
        PathBuf::from("src/../src/lib.rs"),
        workspace_root.join("../src/lib.rs"),
        workspace_root.join("foreign-root/src/lib.rs"),
    ] {
        assert!(!receipt.matches_source(&path, 26, expression), "{path:?}");
    }
    let serialized = serde_json::to_value(&receipt).map_err(|error| error.to_string())?;
    let roundtrip: BooleanComparisonReceipt =
        serde_json::from_value(serialized.clone()).map_err(|error| error.to_string())?;
    assert!(roundtrip.matches_source(&workspace_root.join(&owner.file), 26, expression));
    let mut old_shape = serialized.clone();
    old_shape
        .as_object_mut()
        .ok_or("receipt object")?
        .remove("workspace_root");
    assert!(serde_json::from_value::<BooleanComparisonReceipt>(old_shape).is_err());
    let mut old_invocation_shape = serialized.clone();
    old_invocation_shape
        .as_object_mut()
        .ok_or("receipt object")?
        .remove("invocation_dir");
    assert!(serde_json::from_value::<BooleanComparisonReceipt>(old_invocation_shape).is_err());
    let mut wrong_invocation = serialized.clone();
    wrong_invocation["invocation_dir"] =
        serde_json::json!(workspace_root.join("foreign-invocation"));
    let wrong_invocation: BooleanComparisonReceipt =
        serde_json::from_value(wrong_invocation).map_err(|error| error.to_string())?;
    assert!(!wrong_invocation.matches_source(&owner.file, 26, expression));
    let mut wrong_root = serialized;
    wrong_root["workspace_root"] = serde_json::json!(workspace_root.join("foreign-root"));
    let wrong_root: BooleanComparisonReceipt =
        serde_json::from_value(wrong_root).map_err(|error| error.to_string())?;
    assert!(!wrong_root.matches_source(&owner.file, 26, expression));
    assert!(
        test.assertions.is_empty(),
        "source receipt must not mutate raw parser facts"
    );
    let mut wrong_assertion = assertion.clone();
    wrong_assertion.line += 1;
    assert!(!receipt.admits(test, &wrong_assertion));
    assert!(
        boolean_comparison_receipt(&owner.clone(), test, &index, &syntax).is_none(),
        "copied owner cannot nominate live source authority"
    );
    let mut forged = serde_json::to_value(&receipt).map_err(|error| error.to_string())?;
    forged["samples"] = serde_json::json!([7]);
    let forged: BooleanComparisonReceipt =
        serde_json::from_value(forged).map_err(|error| error.to_string())?;
    assert!(!forged.matches_source(
        &owner.file,
        26,
        "u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])"
    ));
    Ok(())
}

#[test]
fn captured_workspace_anchor_resolves_prefixed_relative_probe_paths() {
    use crate::analysis::facts::WorkspaceRootAuthority;
    let invocation = std::env::temp_dir();
    let root = invocation.join("ripr-relative-helper-subject");
    let expected = std::path::Path::new("src/lib.rs");
    for path in [
        PathBuf::from("src/lib.rs"),
        PathBuf::from("./src/lib.rs"),
        PathBuf::from("ripr-relative-helper-subject/src/lib.rs"),
        PathBuf::from("./ripr-relative-helper-subject/src/lib.rs"),
        root.join("src/lib.rs"),
    ] {
        assert!(
            WorkspaceRootAuthority::source_path_matches(&root, &invocation, expected, &path),
            "{path:?}"
        );
    }
    for path in [
        PathBuf::from("foreign-subject/src/lib.rs"),
        PathBuf::from("ripr-relative-helper-subject/src/other.rs"),
        PathBuf::from("ripr-relative-helper-subject/../src/lib.rs"),
        invocation.join("foreign-subject/src/lib.rs"),
    ] {
        assert!(
            !WorkspaceRootAuthority::source_path_matches(&root, &invocation, expected, &path),
            "{path:?}"
        );
    }
    assert!(!WorkspaceRootAuthority::source_path_matches(
        &root,
        &invocation.join(".."),
        expected,
        expected
    ));
}
