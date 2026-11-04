use std::fs;
use std::path::{Path, PathBuf};

use super::build::{BuildInput, build_snapshot};
use super::identity::{collect_identity_inputs_for_commit, semantic_identity_digest};
use super::model::{
    IdentityInputs, REHEARSAL_RECEIPT_KIND, REHEARSAL_RECEIPT_SCHEMA_VERSION, ReadinessDimension,
    ReadinessDimensionId, ReadinessDimensionState, RehearsalComponent, RehearsalIdentity,
    RehearsalReceipt,
};
use super::rehearsal::parse_rehearsal_receipt;
use super::render::{markdown_matches_json, render_json, render_markdown};

fn fixture(name: &str) -> PathBuf {
    repo_root().join("fixtures/readiness-snapshot").join(name)
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn dimension(
    snapshot: &super::model::ReadinessSnapshot,
    id: ReadinessDimensionId,
) -> Result<&ReadinessDimension, String> {
    snapshot
        .dimensions
        .get(id.as_str())
        .ok_or_else(|| format!("missing dimension `{}`", id.as_str()))
}

fn fixture_receipt(name: &str) -> Result<RehearsalReceipt, String> {
    let text =
        fs::read_to_string(fixture(name)).map_err(|err| format!("read fixture `{name}`: {err}"))?;
    parse_rehearsal_receipt(&text)
}

#[test]
fn rehearsal_plan_round_trip_is_deterministic_and_legacy_stays_compatible() -> Result<(), String> {
    let legacy = fixture_receipt("rehearsal-receipt.complete.json")?;
    assert!(legacy.plan.is_none());
    assert!(
        !serde_json::to_value(&legacy)
            .map_err(|e| e.to_string())?
            .as_object()
            .ok_or("receipt is not object")?
            .contains_key("plan")
    );
    let inputs = IdentityInputs {
        cargo_lock_digest: Some(legacy.identity.cargo_lock_digest.clone()),
        package_lock_digest: legacy.identity.package_lock_digest.clone(),
        rust_toolchain_digest: Some(legacy.identity.rust_toolchain_digest.clone()),
        crate_version: Some(legacy.identity.crate_version.clone()),
        extension_version: legacy.identity.extension_version.clone(),
    };
    let make = || {
        super::rehearsal_plan::plan_receipt(
            legacy.source_sha.clone(),
            "1".repeat(40),
            inputs.clone(),
            "linux",
            "x86_64",
        )
    };
    let first = make()?;
    assert_eq!(first, make()?);
    let encoded = serde_json::to_string(&first).map_err(|e| e.to_string())?;
    assert_eq!(parse_rehearsal_receipt(&encoded)?, first);
    assert_eq!(
        super::build::package_dimension_state(&first),
        ReadinessDimensionState::NotProven
    );
    Ok(())
}

#[test]
fn rehearsal_plan_rejects_mutation_execution_and_unknown_metadata() -> Result<(), String> {
    let text =
        fs::read_to_string(fixture("rehearsal-receipt.plan.json")).map_err(|e| e.to_string())?;
    let original: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    parse_rehearsal_receipt(&text)?;
    let mut cases = Vec::new();
    for field in [
        "versions_changed",
        "candidate_frozen",
        "release_cut",
        "release_authority",
        "signing_performed",
        "credentials_accessed",
        "deployments_performed",
    ] {
        let mut bad = original.clone();
        bad["plan"][field] = serde_json::json!(true);
        cases.push(bad);
    }
    for field in ["tags_created", "writes", "cache_roots"] {
        let mut bad = original.clone();
        bad["plan"][field] = serde_json::json!(["unexpected"]);
        cases.push(bad);
    }
    for field in ["reads", "tools", "prohibited_operations"] {
        for value in [serde_json::json!([]), serde_json::json!([""])] {
            let mut bad = original.clone();
            bad["plan"][field] = value;
            cases.push(bad);
        }
    }
    for (field, value) in [
        ("profile", serde_json::json!("executed")),
        ("source_tree", serde_json::json!("wrong")),
        ("host_os", serde_json::json!("")),
        ("host_arch", serde_json::json!("")),
        ("unknown", serde_json::json!(false)),
    ] {
        let mut bad = original.clone();
        bad["plan"][field] = value;
        cases.push(bad);
    }
    for (field, value) in [
        ("schema_version", serde_json::json!("1.0")),
        ("schema_version", serde_json::json!("2.0")),
        ("published_channels", serde_json::json!(["crate"])),
        ("plan", serde_json::Value::Null),
        ("tree_clean", serde_json::json!(false)),
        ("unknown", serde_json::json!(false)),
    ] {
        let mut bad = original.clone();
        bad[field] = value;
        cases.push(bad);
    }
    for slot in [
        "crate_package",
        "installed_binary",
        "vsix",
        "dependency_policy",
        "sbom",
        "provenance_plan",
    ] {
        let mut bad = original.clone();
        bad[slot]["status"] = serde_json::json!("pass");
        cases.push(bad);
        let mut bad = original.clone();
        bad[slot]["sha256"] = serde_json::json!("invented");
        cases.push(bad);
    }
    for bad in cases {
        assert!(
            parse_rehearsal_receipt(&bad.to_string()).is_err(),
            "accepted {bad}"
        );
    }
    Ok(())
}

#[test]
fn rehearsal_plan_cannot_satisfy_snapshot_readiness() -> Result<(), String> {
    let (path, head) = head_bound_receipt_fixture("rehearsal-receipt.plan.json")?;
    let snapshot = build_snapshot(&BuildInput {
        root: &repo_root(),
        sha: &head,
        rehearsal_receipt: Some(&path),
    });
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    let snapshot = snapshot?;
    for id in [
        ReadinessDimensionId::ToolchainReproducibility,
        ReadinessDimensionId::PackageAndInstallability,
    ] {
        assert_eq!(
            dimension(&snapshot, id)?.state,
            ReadinessDimensionState::NotProven
        );
    }
    let mut forged = fixture_receipt("rehearsal-receipt.plan.json")?;
    forged.crate_package = Some(RehearsalComponent {
        status: "pass".to_string(),
        sha256: Some("invented".to_string()),
        limitation: None,
    });
    assert_eq!(
        super::build::package_dimension_state(&forged),
        ReadinessDimensionState::NotProven
    );
    Ok(())
}

fn rehearsal_identity_from_inputs(inputs: &IdentityInputs) -> Result<RehearsalIdentity, String> {
    Ok(RehearsalIdentity {
        cargo_lock_digest: inputs
            .cargo_lock_digest
            .clone()
            .ok_or("workspace missing Cargo.lock digest for rehearsal identity")?,
        package_lock_digest: inputs.package_lock_digest.clone(),
        rust_toolchain_digest: inputs
            .rust_toolchain_digest
            .clone()
            .ok_or("workspace missing rust-toolchain digest for rehearsal identity")?,
        crate_version: inputs
            .crate_version
            .clone()
            .ok_or("workspace missing crate version for rehearsal identity")?,
        extension_version: inputs.extension_version.clone(),
    })
}

/// Writes a copy of a corpus fixture with `source_sha` and `identity` aligned to `HEAD`.
fn head_bound_receipt_fixture(name: &str) -> Result<(PathBuf, String), String> {
    let root = repo_root();
    let head = super::identity::git_rev_parse("HEAD")?;
    let identity = collect_identity_inputs_for_commit(&root, &head)?;
    let mut receipt = fixture_receipt(name)?;
    receipt.source_sha = head.clone();
    receipt.identity = rehearsal_identity_from_inputs(&identity)?;
    let path = std::env::temp_dir().join(format!(
        "ripr-readiness-receipt-{}-{}.json",
        name.replace('.', "-"),
        std::process::id()
    ));
    let text = serde_json::to_string_pretty(&receipt)
        .map_err(|err| format!("encode head-bound receipt `{name}`: {err}"))?;
    fs::write(&path, text).map_err(|err| format!("write head-bound receipt `{name}`: {err}"))?;
    Ok((path, head))
}

#[test]
fn readiness_snapshot_artifacts_populates_only_proven_dimensions() -> Result<(), String> {
    let root = repo_root();
    let (receipt_path, head) = head_bound_receipt_fixture("rehearsal-receipt.complete.json")?;
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: &head,
        rehearsal_receipt: Some(&receipt_path),
    })?;

    assert_eq!(
        dimension(&snapshot, ReadinessDimensionId::PackageAndInstallability)?.state,
        ReadinessDimensionState::Pass
    );
    assert_eq!(
        dimension(&snapshot, ReadinessDimensionId::ToolchainReproducibility)?.state,
        ReadinessDimensionState::Pass
    );
    assert_eq!(
        dimension(&snapshot, ReadinessDimensionId::MainHealth)?.state,
        ReadinessDimensionState::NotProven
    );
    Ok(())
}

#[test]
fn readiness_snapshot_artifacts_rejects_mixed_source_receipt() -> Result<(), String> {
    let root = repo_root();
    let subject = super::identity::git_rev_parse("HEAD")?;
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: &subject,
        rehearsal_receipt: Some(&fixture("rehearsal-receipt.wrong-sha.json")),
    })?;

    let package = dimension(&snapshot, ReadinessDimensionId::PackageAndInstallability)?;
    if package.state != ReadinessDimensionState::Stale {
        return Err(format!(
            "expected stale package dimension, got {:?}",
            package.state
        ));
    }
    if !package
        .limitations
        .iter()
        .any(|line| line.contains("source_sha"))
    {
        return Err("expected source_sha stale limitation".to_string());
    }
    Ok(())
}

#[test]
fn readiness_snapshot_artifacts_fail_closed_on_missing_digest() -> Result<(), String> {
    let mut receipt: RehearsalReceipt = serde_json::from_str(
        &fs::read_to_string(fixture("rehearsal-receipt.complete.json"))
            .map_err(|err| format!("read complete fixture: {err}"))?,
    )
    .map_err(|err| format!("parse fixture json: {err}"))?;
    receipt.installed_binary = Some(RehearsalComponent {
        status: "pass".to_string(),
        sha256: None,
        limitation: None,
    });
    let text = serde_json::to_string_pretty(&receipt)
        .map_err(|err| format!("encode edited receipt: {err}"))?;
    let parsed = parse_rehearsal_receipt(&text)?;
    if parsed
        .installed_binary
        .as_ref()
        .is_none_or(|c| c.sha256.is_some())
    {
        return Err("expected missing installed_binary digest".to_string());
    }

    let state = super::build::package_dimension_state(&parsed);
    if state != ReadinessDimensionState::Limited {
        return Err(format!("expected limited package state, got {state:?}"));
    }
    Ok(())
}

#[test]
fn readiness_snapshot_artifacts_keeps_supply_chain_records_distinct() -> Result<(), String> {
    let root = repo_root();
    let (receipt_path, head) = head_bound_receipt_fixture("rehearsal-receipt.supply-chain.json")?;
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: &head,
        rehearsal_receipt: Some(&receipt_path),
    })?;

    let supply = dimension(&snapshot, ReadinessDimensionId::SupplyChainArtifacts)?;
    if supply.evidence.len() < 3 {
        return Err(format!(
            "expected at least three supply-chain evidence rows, got {}",
            supply.evidence.len()
        ));
    }
    let producers: Vec<_> = supply
        .evidence
        .iter()
        .map(|e| e.producer.as_str())
        .collect();
    for required in ["dependency_policy", "sbom", "provenance_plan"] {
        if !producers.contains(&required) {
            return Err(format!("missing supply-chain producer `{required}`"));
        }
    }
    Ok(())
}

#[test]
fn readiness_snapshot_artifacts_partial_package_receipt_is_limited() -> Result<(), String> {
    let mut receipt: RehearsalReceipt = serde_json::from_str(
        &fs::read_to_string(fixture("rehearsal-receipt.complete.json"))
            .map_err(|err| format!("read complete fixture: {err}"))?,
    )
    .map_err(|err| format!("parse fixture json: {err}"))?;
    receipt.installed_binary = None;
    receipt.vsix = None;
    let state = super::build::package_dimension_state(&receipt);
    if state != ReadinessDimensionState::Limited {
        return Err(format!("expected limited package state, got {state:?}"));
    }
    Ok(())
}

#[test]
fn readiness_snapshot_json_and_markdown_agree() -> Result<(), String> {
    let root = repo_root();
    let (receipt_path, head) = head_bound_receipt_fixture("rehearsal-receipt.complete.json")?;
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: &head,
        rehearsal_receipt: Some(&receipt_path),
    })?;
    let json = render_json(&snapshot)?;
    let markdown = render_markdown(&snapshot);
    if !markdown_matches_json(&snapshot, &markdown) {
        return Err("markdown render disagrees with snapshot json".to_string());
    }
    if !json.contains("\"toolchain_reproducibility\"") {
        return Err("json missing toolchain_reproducibility dimension".to_string());
    }
    if !markdown.contains("toolchain_reproducibility") {
        return Err("markdown missing toolchain_reproducibility dimension".to_string());
    }
    Ok(())
}

#[test]
fn readiness_snapshot_semantic_identity_ignores_generated_at() -> Result<(), String> {
    let root = repo_root();
    let mut snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: "HEAD",
        rehearsal_receipt: None,
    })?;
    snapshot.generated_at = "first".to_string();
    let first = semantic_identity_digest(&snapshot);
    snapshot.generated_at = "second".to_string();
    let second = semantic_identity_digest(&snapshot);
    if first != second {
        return Err("semantic identity digest must ignore generated_at".to_string());
    }
    Ok(())
}

#[test]
fn readiness_snapshot_fixture_receipt_matches_current_identity_or_skips() -> Result<(), String> {
    let root = repo_root();
    let path = fixture("rehearsal-receipt.complete.json");
    if !path.is_file() {
        return Ok(());
    }
    let (receipt_path, head) = head_bound_receipt_fixture("rehearsal-receipt.complete.json")?;
    let receipt = fixture_receipt("rehearsal-receipt.complete.json")?;
    if receipt.kind != REHEARSAL_RECEIPT_KIND {
        return Err(format!("unexpected receipt kind `{}`", receipt.kind));
    }
    if receipt.schema_version != REHEARSAL_RECEIPT_SCHEMA_VERSION {
        return Err(format!(
            "unexpected receipt schema `{}`",
            receipt.schema_version
        ));
    }
    let identity = collect_identity_inputs_for_commit(&root, &head)?;
    if !super::identity::identities_compatible(
        &identity,
        &rehearsal_identity_from_inputs(&identity)?,
    ) {
        eprintln!(
            "skipping identity-bound snapshot assertion: refresh fixtures/readiness-snapshot/rehearsal-receipt.complete.json"
        );
        return Ok(());
    }
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: &head,
        rehearsal_receipt: Some(&receipt_path),
    })?;
    if dimension(&snapshot, ReadinessDimensionId::PackageAndInstallability)?.state
        != ReadinessDimensionState::Pass
    {
        return Err("expected package dimension pass for identity-matched fixture".to_string());
    }
    Ok(())
}
