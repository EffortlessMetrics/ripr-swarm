use std::collections::BTreeMap;
use std::path::Path;

use super::identity::{
    digest_file, git_rev_parse, identities_compatible, package_qualification_receipt_path,
    rehearsal_receipt_path, semantic_identity_digest, supply_chain_report_path,
    supply_chain_report_status,
};
use super::model::{
    IdentityInputs, NON_CLAIMS, ReadinessDimension, ReadinessDimensionId, ReadinessDimensionState,
    ReadinessEvidenceRecord, ReadinessSnapshot, SNAPSHOT_KIND, SNAPSHOT_SCHEMA_VERSION,
    not_proven_dimension,
};
use super::rehearsal::{component_pass, load_rehearsal_receipt};

pub(crate) struct BuildInput<'a> {
    pub(crate) root: &'a Path,
    pub(crate) sha: &'a str,
    pub(crate) rehearsal_receipt: Option<&'a Path>,
}

pub(crate) fn build_snapshot(input: &BuildInput<'_>) -> Result<ReadinessSnapshot, String> {
    let source_sha = git_rev_parse(input.sha)?;
    let tree_clean = super::identity::tree_clean_for_subject(&source_sha)?;
    let identity_inputs =
        super::identity::collect_identity_inputs_for_commit(input.root, &source_sha)?;
    let mut dimensions = default_dimensions(&source_sha, tree_clean);

    let rehearsal_path = rehearsal_receipt_path(input.root, input.rehearsal_receipt);
    if rehearsal_path.is_file() {
        let receipt = load_rehearsal_receipt(&rehearsal_path)?;
        apply_rehearsal_adapters(
            &mut dimensions,
            &source_sha,
            tree_clean,
            &identity_inputs,
            &receipt,
            &rehearsal_path,
        )?;
    }

    apply_supply_chain_adapter(
        input.root,
        &mut dimensions,
        &source_sha,
        tree_clean,
        &identity_inputs,
    )?;

    let mut snapshot = ReadinessSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
        kind: SNAPSHOT_KIND.to_string(),
        source_sha: source_sha.clone(),
        tree_clean,
        generated_at: "telemetry-only".to_string(),
        release_authority: false,
        candidate_frozen: false,
        ship_verdict: None,
        identity_inputs,
        dimensions,
        blocking_unknowns: Vec::new(),
        published_channels: Vec::new(),
        non_claims: NON_CLAIMS.iter().map(|s| (*s).to_string()).collect(),
        semantic_identity_digest: String::new(),
    };
    snapshot.semantic_identity_digest = semantic_identity_digest(&snapshot);
    Ok(snapshot)
}

fn default_dimensions(source_sha: &str, tree_clean: bool) -> BTreeMap<String, ReadinessDimension> {
    let mut map = BTreeMap::new();
    for id in ReadinessDimensionId::all() {
        map.insert(
            id.as_str().to_string(),
            not_proven_dimension(
                source_sha,
                tree_clean,
                *id,
                "no current receipt consumed for this dimension",
                "run the owning producer or refresh command for this dimension",
            ),
        );
    }
    map
}

fn apply_rehearsal_adapters(
    dimensions: &mut BTreeMap<String, ReadinessDimension>,
    source_sha: &str,
    tree_clean: bool,
    identity_inputs: &IdentityInputs,
    receipt: &super::model::RehearsalReceipt,
    path: &Path,
) -> Result<(), String> {
    if receipt.source_sha != source_sha {
        set_dimension(
            dimensions,
            ReadinessDimensionId::PackageAndInstallability,
            ReadinessDimensionState::Stale,
            source_sha,
            tree_clean,
            vec!["rehearsal receipt source_sha does not match snapshot subject".to_string()],
            vec!["cargo xtask readiness-rehearsal --sha HEAD --no-publish".to_string()],
        );
        set_dimension(
            dimensions,
            ReadinessDimensionId::ToolchainReproducibility,
            ReadinessDimensionState::Stale,
            source_sha,
            tree_clean,
            vec!["rehearsal receipt source_sha does not match snapshot subject".to_string()],
            vec!["refresh the #1703 rehearsal receipt on the requested SHA".to_string()],
        );
        return Ok(());
    }

    if !identities_compatible(identity_inputs, &receipt.identity) {
        set_dimension(
            dimensions,
            ReadinessDimensionId::ToolchainReproducibility,
            ReadinessDimensionState::Stale,
            source_sha,
            tree_clean,
            vec![
                "lockfile, toolchain, or package manifest identity changed since rehearsal"
                    .to_string(),
            ],
            vec!["cargo xtask readiness-rehearsal --sha HEAD --no-publish".to_string()],
        );
        set_dimension(
            dimensions,
            ReadinessDimensionId::PackageAndInstallability,
            ReadinessDimensionState::Stale,
            source_sha,
            tree_clean,
            vec!["rehearsal identity no longer matches current source inputs".to_string()],
            vec!["cargo xtask readiness-rehearsal --sha HEAD --no-publish".to_string()],
        );
        return Ok(());
    }

    let artifact_digest = digest_file(path).ok();
    let evidence = vec![ReadinessEvidenceRecord {
        producer: "readiness_no_publish_rehearsal".to_string(),
        schema_version: super::model::REHEARSAL_RECEIPT_SCHEMA_VERSION.to_string(),
        command: "cargo xtask readiness-rehearsal --sha <SHA> --no-publish".to_string(),
        artifact: path.display().to_string(),
        artifact_digest,
        freshness_policy:
            "stale when source SHA, lockfiles, toolchains, manifests, or extension inputs change"
                .to_string(),
        stale_reason: None,
    }];

    dimensions.insert(
        ReadinessDimensionId::ToolchainReproducibility
            .as_str()
            .to_string(),
        ReadinessDimension {
            state: ReadinessDimensionState::Pass,
            source_sha: source_sha.to_string(),
            tree_clean,
            evidence: evidence.clone(),
            limitations: receipt.limitations.clone(),
            non_claims: receipt.non_claims.clone(),
            owners: vec!["#4640".to_string()],
            next_proof: vec!["repeat rehearsal after lockfile or toolchain movement".to_string()],
        },
    );

    let package_state = package_dimension_state(receipt);
    dimensions.insert(
        ReadinessDimensionId::PackageAndInstallability
            .as_str()
            .to_string(),
        ReadinessDimension {
            state: package_state,
            source_sha: source_sha.to_string(),
            tree_clean,
            evidence,
            limitations: rehearsal_limitations(receipt),
            non_claims: receipt.non_claims.clone(),
            owners: vec!["#4640".to_string(), "#1703".to_string()],
            next_proof: vec!["cargo xtask readiness-rehearsal --sha HEAD --no-publish".to_string()],
        },
    );

    if receipt.dependency_policy.is_some()
        || receipt.sbom.is_some()
        || receipt.provenance_plan.is_some()
    {
        merge_supply_chain_from_rehearsal(dimensions, source_sha, tree_clean, receipt, path);
    }
    Ok(())
}

pub(crate) fn package_dimension_state(
    receipt: &super::model::RehearsalReceipt,
) -> ReadinessDimensionState {
    if receipt
        .install_smokes
        .iter()
        .any(|component| component.status == "failed")
    {
        return ReadinessDimensionState::Limited;
    }
    let slots = [
        receipt.crate_package.as_ref(),
        receipt.installed_binary.as_ref(),
        receipt.vsix.as_ref(),
    ];
    let missing_slots = slots.iter().filter(|slot| slot.is_none()).count();
    let mut any_pass = false;
    let mut any_failed = false;
    let mut any_non_pass = false;
    for component in slots.into_iter().flatten() {
        if component_pass(component) {
            if component.sha256.as_deref().unwrap_or("").is_empty() {
                any_failed = true;
            } else {
                any_pass = true;
            }
        } else if component.status == "failed" {
            any_failed = true;
        } else {
            any_non_pass = true;
        }
    }
    if any_failed {
        ReadinessDimensionState::Limited
    } else if missing_slots > 0 {
        if any_pass {
            ReadinessDimensionState::Limited
        } else {
            ReadinessDimensionState::NotProven
        }
    } else if any_pass && !any_non_pass {
        ReadinessDimensionState::Pass
    } else if any_pass {
        ReadinessDimensionState::Limited
    } else {
        ReadinessDimensionState::NotProven
    }
}

fn rehearsal_limitations(receipt: &super::model::RehearsalReceipt) -> Vec<String> {
    let mut out = receipt.limitations.clone();
    if receipt
        .crate_package
        .as_ref()
        .is_some_and(|c| !component_pass(c))
    {
        out.push("crate package rehearsal did not pass".to_string());
    }
    if receipt
        .installed_binary
        .as_ref()
        .is_some_and(|c| !component_pass(c))
    {
        out.push("installed binary rehearsal did not pass".to_string());
    }
    if receipt.vsix.as_ref().is_some_and(|c| !component_pass(c)) {
        out.push("VSIX rehearsal did not pass".to_string());
    }
    out
}

fn merge_supply_chain_from_rehearsal(
    dimensions: &mut BTreeMap<String, ReadinessDimension>,
    source_sha: &str,
    tree_clean: bool,
    receipt: &super::model::RehearsalReceipt,
    path: &Path,
) {
    let mut evidence = Vec::new();
    if let Some(component) = &receipt.dependency_policy {
        evidence.push(record_for_component(
            path,
            "dependency_policy",
            component,
            "cargo xtask check-dependencies",
        ));
    }
    if let Some(component) = &receipt.sbom {
        evidence.push(record_for_component(
            path,
            "sbom",
            component,
            "local SBOM producer when configured",
        ));
    }
    if let Some(component) = &receipt.provenance_plan {
        evidence.push(record_for_component(
            path,
            "provenance_plan",
            component,
            "local provenance plan producer when configured",
        ));
    }
    if evidence.is_empty() {
        return;
    }
    let all_present = receipt.dependency_policy.is_some()
        && receipt.sbom.is_some()
        && receipt.provenance_plan.is_some();
    let all_pass = evidence.iter().all(|record| record.stale_reason.is_none());
    let state = if !all_present {
        ReadinessDimensionState::Limited
    } else if all_pass {
        ReadinessDimensionState::Pass
    } else {
        ReadinessDimensionState::Limited
    };
    dimensions.insert(
        ReadinessDimensionId::SupplyChainArtifacts
            .as_str()
            .to_string(),
        ReadinessDimension {
            state,
            source_sha: source_sha.to_string(),
            tree_clean,
            evidence,
            limitations: receipt.limitations.clone(),
            non_claims: receipt.non_claims.clone(),
            owners: vec!["#4640".to_string()],
            next_proof: vec!["retain distinct dependency, SBOM and provenance records".to_string()],
        },
    );
}

fn record_for_component(
    receipt_path: &Path,
    name: &str,
    component: &super::model::RehearsalComponent,
    command: &str,
) -> ReadinessEvidenceRecord {
    ReadinessEvidenceRecord {
        producer: name.to_string(),
        schema_version: super::model::REHEARSAL_RECEIPT_SCHEMA_VERSION.to_string(),
        command: command.to_string(),
        artifact: format!("{}@{}", receipt_path.display(), name),
        artifact_digest: component
            .sha256
            .as_ref()
            .map(|d| d.strip_prefix("sha256:").unwrap_or(d.as_str()))
            .map(|d| format!("sha256:{d}")),
        freshness_policy: "stale when lockfiles or advisory databases change".to_string(),
        stale_reason: if component_pass(component) {
            None
        } else {
            Some("component_not_pass".to_string())
        },
    }
}

fn apply_supply_chain_adapter(
    root: &Path,
    dimensions: &mut BTreeMap<String, ReadinessDimension>,
    source_sha: &str,
    tree_clean: bool,
    identity_inputs: &IdentityInputs,
) -> Result<(), String> {
    let supply_chain = supply_chain_report_path(root);
    let package_receipt = package_qualification_receipt_path(root);
    let mut evidence = Vec::new();

    if supply_chain.is_file() {
        let stale_reason = supply_chain_report_status(&supply_chain)
            .ok()
            .flatten()
            .and_then(|status| {
                if status.eq_ignore_ascii_case("pass") {
                    None
                } else {
                    Some(format!("report_status_{status}"))
                }
            });
        evidence.push(ReadinessEvidenceRecord {
            producer: "cargo_deny_supply_chain".to_string(),
            schema_version: "advisory".to_string(),
            command: "cargo xtask check-supply-chain".to_string(),
            artifact: supply_chain.display().to_string(),
            artifact_digest: digest_file(&supply_chain).ok(),
            freshness_policy: "stale when Cargo.lock or deny.toml changes".to_string(),
            stale_reason,
        });
    }

    if package_receipt.is_file() {
        evidence.push(ReadinessEvidenceRecord {
            producer: "package_qualification_receipt".to_string(),
            schema_version: super::model::REHEARSAL_RECEIPT_SCHEMA_VERSION.to_string(),
            command: "cargo xtask package-qualification-gate --receipt <path>".to_string(),
            artifact: package_receipt.display().to_string(),
            artifact_digest: digest_file(&package_receipt).ok(),
            freshness_policy: "stale when source tree or package identity changes".to_string(),
            stale_reason: None,
        });
    }

    if evidence.is_empty() {
        return Ok(());
    }

    let existing = dimensions
        .get(ReadinessDimensionId::SupplyChainArtifacts.as_str())
        .cloned();
    let mut merged = existing.unwrap_or_else(|| {
        not_proven_dimension(
            source_sha,
            tree_clean,
            ReadinessDimensionId::SupplyChainArtifacts,
            "no supply-chain evidence yet",
            "cargo xtask check-supply-chain",
        )
    });
    let preserved_pass = merged.state == ReadinessDimensionState::Pass;
    merged.evidence.extend(evidence);
    if !preserved_pass {
        merged.state = ReadinessDimensionState::Limited;
    }
    merged.limitations.push(
        "supply-chain dimension keeps dependency, package-qualification and SBOM records separate"
            .to_string(),
    );
    merged.next_proof = vec![
        "cargo xtask check-supply-chain".to_string(),
        "consume versioned #1703 rehearsal SBOM rows when present".to_string(),
    ];
    merged.owners = vec!["#4640".to_string()];
    if identity_inputs.cargo_lock_digest.is_none() {
        merged.state = ReadinessDimensionState::NotProven;
        merged
            .limitations
            .push("Cargo.lock missing; dependency freshness unknown".to_string());
    }
    dimensions.insert(
        ReadinessDimensionId::SupplyChainArtifacts
            .as_str()
            .to_string(),
        merged,
    );
    Ok(())
}

fn set_dimension(
    dimensions: &mut BTreeMap<String, ReadinessDimension>,
    id: ReadinessDimensionId,
    state: ReadinessDimensionState,
    source_sha: &str,
    tree_clean: bool,
    limitations: Vec<String>,
    next_proof: Vec<String>,
) {
    dimensions.insert(
        id.as_str().to_string(),
        ReadinessDimension {
            state,
            source_sha: source_sha.to_string(),
            tree_clean,
            evidence: Vec::new(),
            limitations,
            non_claims: Vec::new(),
            owners: vec![id.default_owner().to_string()],
            next_proof,
        },
    );
}
