//! Controls exercise the same snapshot validation that check-fixture-contracts
//! calls, so disabling Rust reconstruction cannot turn them green.

use super::*;

fn rust_fixture(label: &str) -> Result<(ScratchRepository, Value), String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(INSTALLED_RUST_CONTRACT.fixture_root);
    let root =
        std::env::temp_dir().join(format!("ripr-rust-recipe-{label}-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|error| format!("clear scratch: {error}"))?;
    }
    copy_tree(&source, &root)?;
    let scratch = ScratchRepository(root);
    let manifest = read_json_value(&scratch.0.join("manifest.json"))?;
    Ok((scratch, manifest))
}

#[test]
fn rust_fixture_gate_refuses_a_forged_no_ignore_commit() -> Result<(), String> {
    let (root, mut manifest) = rust_fixture("forged-commit")?;
    manifest["repository"]["snapshots"]["head-no-ignore"]["commit"] =
        serde_json::json!("0".repeat(40));
    let violations =
        installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &manifest);
    assert!(
        violations
            .iter()
            .any(|message| { message.contains("snapshot `head-no-ignore` records commit") }),
        "the public fixture gate must reconstruct the forged commit, got {violations:?}"
    );
    Ok(())
}

#[test]
fn rust_recipe_retains_positive_history_and_corrects_only_the_variant() -> Result<(), String> {
    let (root, manifest) = rust_fixture("canonical")?;
    let rebuilt = installed_journey_rebuilt_ids(&INSTALLED_RUST_CONTRACT, &root.0, &manifest)?;
    let expected = [
        (
            "base",
            "01821f774cb2cc2bc745b7506a3941312456fb4e",
            "8c3a6ff250d34675386567f37ccf05d8a0dd6d6e",
        ),
        (
            "head",
            "18a45fa5cd718f85dc60fcb27495b24dbd9715e7",
            "60374952915a027947d4e1654bd6e945834114c3",
        ),
        (
            "head-no-ignore",
            "7eaa951e85de8ede45441272f06fb99327440416",
            "36e9bee8d075276b344af3389b3ad76510b2f930",
        ),
    ];
    assert_eq!(rebuilt.len(), expected.len());
    for (actual, (snapshot, commit, tree)) in rebuilt.iter().zip(expected) {
        assert_eq!(
            actual,
            &(snapshot.to_string(), commit.to_string(), tree.to_string())
        );
    }
    assert!(
        installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &manifest)
            .is_empty()
    );
    Ok(())
}

#[test]
fn rust_fixture_gate_refuses_the_historical_pin_and_a_forged_tree() -> Result<(), String> {
    let (root, manifest) = rust_fixture("historical")?;
    for (field, value) in [
        (
            "commit",
            "7831d6f6e31fe47fad83d204f8644bd9618f2bb4".to_string(),
        ),
        ("tree", "0".repeat(40)),
    ] {
        let mut tampered = manifest.clone();
        tampered["repository"]["snapshots"]["head-no-ignore"][field] = serde_json::json!(value);
        let violations =
            installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &tampered);
        assert!(
            violations
                .iter()
                .any(|message| message
                    .contains(&format!("snapshot `head-no-ignore` records {field}"))),
            "{violations:?}"
        );
        assert!(
            violations
                .iter()
                .all(|message| !message.contains("snapshot `base`")
                    && !message.contains("snapshot `head`")),
            "{violations:?}"
        );
    }
    Ok(())
}

#[test]
fn rust_fixture_gate_refuses_missing_variant_metadata() -> Result<(), String> {
    let (root, manifest) = rust_fixture("missing-metadata")?;
    for field in [
        "author_name",
        "author_email",
        "author_date",
        "committer_name",
        "committer_email",
        "committer_date",
        "message",
    ] {
        let mut tampered = manifest.clone();
        tampered["repository"]["variant_construction_recipe"][0]["commit"]
            .as_object_mut()
            .ok_or("commit metadata missing")?
            .remove(field);
        let violations =
            installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &tampered);
        assert!(
            violations.iter().any(
                |message| message.contains("invalid explicit variant recipe")
                    && message.contains(field)
            ),
            "missing {field} must refuse: {violations:?}"
        );
    }
    let mut tampered = manifest;
    tampered["repository"]
        .as_object_mut()
        .ok_or("repository missing")?
        .remove("variant_construction_recipe");
    let violations =
        installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &tampered);
    assert!(
        violations
            .iter()
            .any(|message| message.contains("must state the variant")),
        "a missing complete recipe must refuse: {violations:?}"
    );
    Ok(())
}

#[test]
fn rust_fixture_gate_refuses_missing_positive_history_metadata() -> Result<(), String> {
    let (root, manifest) = rust_fixture("missing-positive-metadata")?;
    for (index, omitted) in [
        (0, "git config user.name 'ripr fixture'"),
        (0, "git config user.email 'fixture@example.invalid'"),
        (2, "GIT_COMMITTER_DATE=2026-10-01T00:00:00Z"),
        (4, "GIT_COMMITTER_DATE=2026-10-01T00:01:00Z"),
    ] {
        let mut tampered = manifest.clone();
        let step = tampered["repository"]["construction_recipe"][index]
            .as_str()
            .ok_or("recipe step missing")?
            .replace(omitted, "");
        tampered["repository"]["construction_recipe"][index] = serde_json::json!(step);
        let violations =
            installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &tampered);
        assert!(
            !violations.is_empty(),
            "omitting {omitted} must not use an implicit default"
        );
    }
    Ok(())
}

#[test]
fn rust_fixture_gate_uses_every_declared_commit_field() -> Result<(), String> {
    let (root, manifest) = rust_fixture("changed-metadata")?;
    for (field, value) in [
        ("author_name", "changed author"),
        ("author_email", "changed-author@example.invalid"),
        ("author_date", "2026-10-01T00:03:00Z"),
        ("committer_name", "changed committer"),
        ("committer_email", "changed-committer@example.invalid"),
        ("committer_date", "2026-10-01T00:04:00Z"),
        ("message", "changed message"),
    ] {
        let mut tampered = manifest.clone();
        tampered["repository"]["variant_construction_recipe"][0]["commit"][field] =
            serde_json::json!(value);
        let violations =
            installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &tampered);
        assert!(
            violations
                .iter()
                .any(|message| message.contains("snapshot `head-no-ignore` records commit")),
            "changing {field} must change the commit: {violations:?}"
        );
        assert!(
            violations
                .iter()
                .all(|message| !message.contains("records tree")),
            "metadata must preserve the tree: {violations:?}"
        );
    }
    Ok(())
}

#[test]
fn rust_fixture_gate_requires_exact_variant_inventory_and_base_parent() -> Result<(), String> {
    let (root, manifest) = rust_fixture("variant-inventory")?;
    for variant in [
        serde_json::json!([]),
        serde_json::json!([
            manifest["repository"]["variant_construction_recipe"][0].clone(),
            manifest["repository"]["variant_construction_recipe"][0].clone()
        ]),
    ] {
        let mut tampered = manifest.clone();
        tampered["repository"]["variant_construction_recipe"] = variant;
        let violations =
            installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &tampered);
        assert!(
            !violations.is_empty(),
            "a missing or repeated variant must refuse"
        );
    }
    let mut tampered = manifest;
    tampered["repository"]["variant_construction_recipe"][0]["parent"] = serde_json::json!("head");
    let violations =
        installed_journey_snapshot_violations(&INSTALLED_RUST_CONTRACT, &root.0, &tampered);
    assert!(
        violations
            .iter()
            .any(|message| message.contains("parent `base`")),
        "the documented base parent is required: {violations:?}"
    );
    Ok(())
}
