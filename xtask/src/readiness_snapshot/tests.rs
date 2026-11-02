use std::fs;
use std::path::{Path, PathBuf};

use super::build::{BuildInput, build_snapshot};
use super::identity::{collect_identity_inputs_for_commit, semantic_identity_digest};
use super::model::{
    REHEARSAL_RECEIPT_KIND, REHEARSAL_RECEIPT_SCHEMA_VERSION, ReadinessDimension,
    ReadinessDimensionId, ReadinessDimensionState, RehearsalComponent, RehearsalReceipt,
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

#[test]
fn readiness_snapshot_artifacts_populates_only_proven_dimensions() -> Result<(), String> {
    let root = repo_root();
    let receipt_text = fs::read_to_string(fixture("rehearsal-receipt.complete.json"))
        .map_err(|err| format!("read complete fixture: {err}"))?;
    let receipt = parse_rehearsal_receipt(&receipt_text)?;
    let identity = collect_identity_inputs_for_commit(&root, &receipt.source_sha)?;
    if !super::identity::identities_compatible(&identity, &receipt.identity) {
        return Err(
            "fixture identity no longer matches workspace; refresh fixtures/readiness-snapshot"
                .to_string(),
        );
    }

    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: "HEAD",
        rehearsal_receipt: Some(&fixture("rehearsal-receipt.complete.json")),
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
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: "HEAD",
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
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: "HEAD",
        rehearsal_receipt: Some(&fixture("rehearsal-receipt.supply-chain.json")),
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
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: "HEAD",
        rehearsal_receipt: Some(&fixture("rehearsal-receipt.complete.json")),
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
    let text = fs::read_to_string(&path).map_err(|err| format!("read fixture: {err}"))?;
    let receipt = parse_rehearsal_receipt(&text)?;
    if receipt.kind != REHEARSAL_RECEIPT_KIND {
        return Err(format!("unexpected receipt kind `{}`", receipt.kind));
    }
    if receipt.schema_version != REHEARSAL_RECEIPT_SCHEMA_VERSION {
        return Err(format!(
            "unexpected receipt schema `{}`",
            receipt.schema_version
        ));
    }
    let identity = collect_identity_inputs_for_commit(&root, &receipt.source_sha)?;
    if !super::identity::identities_compatible(&identity, &receipt.identity) {
        eprintln!(
            "skipping identity-bound snapshot assertion: refresh fixtures/readiness-snapshot/rehearsal-receipt.complete.json"
        );
        return Ok(());
    }
    let snapshot = build_snapshot(&BuildInput {
        root: &root,
        sha: "HEAD",
        rehearsal_receipt: Some(&path),
    })?;
    if dimension(&snapshot, ReadinessDimensionId::PackageAndInstallability)?.state
        != ReadinessDimensionState::Pass
    {
        return Err("expected package dimension pass for identity-matched fixture".to_string());
    }
    Ok(())
}
