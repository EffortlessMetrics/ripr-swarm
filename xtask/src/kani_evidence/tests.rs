use super::configuration::{reject_cargo_configuration, reject_kani_metadata, reject_replacements};
use super::*;
fn source() -> BTreeMap<String, String> {
    BTreeMap::from([("crates/ripr/src/a.rs".to_string(), hash(b"source"))])
}
fn listing() -> serde_json::Value {
    serde_json::json!({
        "kani-version":"0.68.0","file-version":"0.1","standard-harnesses":{"crates/ripr/src/a.rs":["a::one"]},
        "contract-harnesses":{},"contracts":[],"totals":{"standard-harnesses":1,"contract-harnesses":0,"functions-under-contract":0}})
}
fn subjects() -> Result<Vec<Subject>, String> {
    native_inventory(&json(&listing())?, "ripr", Path::new("/repo"), &source())
}
fn require_refusal<T>(result: Result<T, String>, context: &str) -> Result<(), String> {
    match result {
        Err(reason) => {
            assert!(!reason.is_empty(), "{context} must explain its refusal");
            Ok(())
        }
        Ok(_) => Err(format!("{context} was unexpectedly admitted")),
    }
}
#[test]
fn native_inventory_uses_qualified_file_identity_and_refuses_duplicate_or_missing_scope()
-> Result<(), String> {
    let native = subjects()?;
    assert_eq!(native[0].qualified_name.as_deref(), Some("a::one"));
    for mut value in [listing(), listing(), listing(), listing()] {
        value["totals"]["standard-harnesses"] = serde_json::json!(2);
        require_refusal(
            native_inventory(&json(&value)?, "ripr", Path::new("/repo"), &source()),
            "contradictory native inventory total",
        )?;
    }
    let mut value = listing();
    value["standard-harnesses"]["crates/ripr/src/a.rs"] = serde_json::json!(["a::one", "a::one"]);
    value["totals"]["standard-harnesses"] = serde_json::json!(2);
    require_refusal(
        native_inventory(&json(&value)?, "ripr", Path::new("/repo"), &source()),
        "duplicate native harness identity",
    )?;
    let mut value = listing();
    value["file-version"] = serde_json::json!("0.2");
    require_refusal(
        native_inventory(&json(&value)?, "ripr", Path::new("/repo"), &source()),
        "unsupported native inventory format",
    )?;
    require_refusal(
        native_inventory(
            br#"{"kani-version":"0.68.0","kani-version":"0.68.0"}"#,
            "ripr",
            Path::new("/repo"),
            &source(),
        ),
        "duplicate native inventory JSON key",
    )?;
    Ok(())
}
#[test]
fn declared_and_native_denominators_are_symmetric() -> Result<(), String> {
    let native = subjects()?;
    let mut declared = native.clone();
    declared[0].qualified_name = None;
    assert!(compare_inventories(&declared, &native).is_empty());
    assert!(!compare_inventories(&[], &native).is_empty());
    assert!(!compare_inventories(&declared, &[]).is_empty());
    declared.push(declared[0].clone());
    assert!(!compare_inventories(&declared, &native).is_empty());
    Ok(())
}
#[test]
fn exact_native_result_identity_cannot_borrow_a_neighbor_or_hide_an_extra() -> Result<(), String> {
    let subjects = subjects()?;
    let good = "Checking harness a::one...\r\nCheck 1: a::one.cover.1\r\n - Status: SATISFIED\r\n ** 1 of 1 cover properties satisfied\r\nVERIFICATION:- SUCCESSFUL\r\nVerification Time: 0.1s\r\n";
    let (rows, observed, reasons) = observe(&subjects, good);
    assert!(reasons.is_empty());
    assert_eq!(observed, [subjects[0].id()]);
    assert_eq!(rows[0].status, ObservationStatus::Verified);
    assert_eq!(
        rows[0].raw_block_digest,
        Some(hash(
            good.split_once('\n').ok_or("header missing")?.1.as_bytes()
        ))
    );
    let (rows, _, reasons) = observe(&subjects, &good.replace("a::one", "neighbor::one"));
    assert_eq!(rows[0].status, ObservationStatus::NotEstablished);
    assert!(!reasons.is_empty());
    let (rows, _, reasons) = observe(&subjects, &format!("{good}{good}"));
    assert_eq!(rows[0].status, ObservationStatus::NotEstablished);
    assert!(!reasons.is_empty());
    let (_, _, reasons) = observe(
        &subjects,
        &format!("{good}Checking harness extra::hidden...\nVERIFICATION:- SUCCESSFUL\n"),
    );
    assert!(!reasons.is_empty());
    Ok(())
}
#[test]
fn weakening_verifier_configuration_is_not_admitted_even_when_hashable() -> Result<(), String> {
    for text in [
        "[workspace.metadata.kani.flags]\ndefault-checks=false",
        "[package.metadata.kani.flags]\ncbmc-args=['--no-bounds-check']",
        "[kani.unstable]\nstubbing=true",
    ] {
        let value = toml::from_str(text).map_err(|e| e.to_string())?;
        require_refusal(reject_kani_metadata(&value), text)?;
    }
    reject_kani_metadata(&toml::from_str("[workspace]\nmembers=[]").map_err(|e| e.to_string())?)?;
    for text in [
        "[build]\nrustflags=['--cfg','replacement']",
        "[build]\nrustc-wrapper='fake'",
        "[target.x86_64-unknown-linux-gnu]\nrustflags=['-C','overflow-checks=no']",
        "[alias]\nkani='run'",
        "[env]\nRUSTFLAGS='--cfg replacement'",
    ] {
        require_refusal(
            reject_cargo_configuration(&toml::from_str(text).map_err(|e| e.to_string())?),
            text,
        )?;
    }
    reject_cargo_configuration(
        &toml::from_str("[build]\njobs=1\n[env]\nTMP='target'").map_err(|e| e.to_string())?,
    )?;
    Ok(())
}
#[test]
fn real_stub_and_verification_only_bodies_refuse_while_fixture_strings_do_not() -> Result<(), String>
{
    for text in [
        "#[kani::stub(actual, fake)] fn proof() {}",
        "#[cfg(kani)] fn production() { }",
        "#[cfg_attr(kani,kani::stub(actual,fake))] fn proof() {}",
    ] {
        require_refusal(reject_replacements(text), text)?;
    }
    reject_replacements("const FIXTURE:&str=r#\"#[kani::stub(actual,fake)] fn f() {}\"#;")?;
    reject_replacements("#[cfg(kani)] mod kani_proofs { #[kani::proof] fn p() {} }")?;
    Ok(())
}
#[test]
fn source_currentness_requires_equal_observed_manifests_and_revisions() -> Result<(), String> {
    let snapshot = Snapshot {
        revision: Some("1".repeat(40)),
        tree: Some("2".repeat(40)),
        dirty: Some(true),
        manifest: Some(Artifact {
            path: "source.json".to_string(),
            sha256: hash(b"input"),
            bytes: 5,
        }),
        file_count: 1,
        error: None,
    };
    assert!(source_current(&snapshot, &snapshot));
    let mut changed = snapshot.clone();
    changed.manifest.as_mut().ok_or("manifest absent")?.sha256 = hash(b"changed");
    assert!(!source_current(&snapshot, &changed));
    let mut changed = snapshot.clone();
    changed.revision = Some("3".repeat(40));
    assert!(!source_current(&snapshot, &changed));
    let mut changed = snapshot.clone();
    changed.error = Some("missing source".to_string());
    assert!(!source_current(&snapshot, &changed));
    Ok(())
}
#[test]
fn artifact_commitment_refuses_modified_or_missing_bytes() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("ripr-custody-test-{}", std::process::id()));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let file = root.join("native.stdout");
    fs::write(&file, b"native bytes").map_err(|e| e.to_string())?;
    let artifact = Artifact {
        path: "native.stdout".to_string(),
        sha256: hash(b"native bytes"),
        bytes: 12,
    };
    assert_eq!(read_artifact(&root, &artifact)?, b"native bytes");
    fs::write(&file, b"changed bytes").map_err(|e| e.to_string())?;
    require_refusal(read_artifact(&root, &artifact), "modified native bytes")?;
    fs::remove_file(file).map_err(|e| e.to_string())?;
    require_refusal(read_artifact(&root, &artifact), "missing native bytes")?;
    fs::remove_dir(root).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn pinned_native_inventory_fixture_qualifies_the_supported_format() -> Result<(), String> {
    let bytes = include_bytes!("../../tests/fixtures/kani-native-inventory/kani-list.json");
    assert_eq!(
        hash(bytes),
        "sha256:d023ab5a23a66e93b649312ef2f229af1659d04abd522330c26d587dc5fb7224"
    );
    let value: serde_json::Value = formal::parse_json(bytes)?;
    let keys = value["standard-harnesses"]
        .as_object()
        .ok_or("native map absent")?;
    let source = keys
        .keys()
        .map(|key| {
            (
                key.clone(),
                hash(b"format-control-only-not-source-provenance"),
            )
        })
        .collect();
    let subjects = native_inventory(bytes, "ripr-contract", Path::new("/fixture"), &source)?;
    assert_eq!(subjects.len(), 21);
    assert!(
        subjects
            .iter()
            .all(|s| s.package == "ripr-contract" && s.qualified_name.is_some())
    );
    Ok(())
}
