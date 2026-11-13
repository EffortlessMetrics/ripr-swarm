use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct TransitionReport {
    schema: &'static str,
    repository: String,
    base_registry_generation: String,
    head_registry_generation: String,
    unchanged_claims: Vec<String>,
    changed_claims: Vec<String>,
    added_claims: Vec<String>,
    retired_claims: Vec<String>,
    valid: bool,
    native_assurance: &'static str,
    execution_authority: &'static str,
}

fn transition(base: &LoadedRegistry, head: &LoadedRegistry) -> RegistryResult<TransitionReport> {
    require(
        base.manifest.repository == head.manifest.repository,
        "base/head registries name different repositories",
    )?;
    let changed = base.registry_digest != head.registry_digest;
    if changed {
        require(
            head.manifest.registry_generation > base.manifest.registry_generation,
            "changed registry must increase registry_generation",
        )?;
    } else {
        require(
            head.manifest.registry_generation == base.manifest.registry_generation,
            "unchanged registry cannot change registry_generation",
        )?;
    }

    let base_claims: BTreeMap<_, _> = base
        .claims
        .iter()
        .map(|claim| (claim.claim.claim_id.as_str(), claim))
        .collect();
    let head_claims: BTreeMap<_, _> = head
        .claims
        .iter()
        .map(|claim| (claim.claim.claim_id.as_str(), claim))
        .collect();
    let base_retirements: BTreeMap<_, _> = base
        .manifest
        .retirements
        .iter()
        .map(|retirement| (retirement.claim_id.as_str(), retirement))
        .collect();
    let head_retirements: BTreeMap<_, _> = head
        .manifest
        .retirements
        .iter()
        .map(|retirement| (retirement.claim_id.as_str(), retirement))
        .collect();

    for (claim_id, retirement) in &base_retirements {
        require(
            head_retirements.get(claim_id).copied() == Some(*retirement),
            format!("retirement {claim_id} was removed or rewritten"),
        )?;
        require(
            !head_claims.contains_key(claim_id),
            format!("retired claim {claim_id} cannot become active again"),
        )?;
    }

    // A retirement is evidence about authority that existed on the base side.
    // Never admit a newly invented tombstone for a claim that was neither
    // active nor already retired in the base registry.
    for (claim_id, retirement) in &head_retirements {
        if base_retirements.contains_key(claim_id) {
            continue;
        }
        let base_claim = base_claims.get(claim_id).ok_or_else(|| {
            format!("retirement {claim_id} does not correspond to an active base claim")
        })?;
        require(
            !head_claims.contains_key(claim_id),
            format!("retired claim {claim_id} cannot remain active"),
        )?;
        require(
            retirement.final_generation == base_claim.claim.claim_generation,
            format!(
                "{claim_id}: retirement final_generation does not match the active base claim"
            ),
        )?;
    }

    let mut unchanged_claims = Vec::new();
    let mut changed_claims = Vec::new();
    let mut added_claims = Vec::new();
    let mut retired_claims = Vec::new();

    for (claim_id, base_claim) in &base_claims {
        match head_claims.get(claim_id) {
            Some(head_claim) if head_claim.decision_digest == base_claim.decision_digest => {
                require(
                    head_claim.claim.claim_generation == base_claim.claim.claim_generation,
                    format!(
                        "{claim_id}: unchanged decision cannot change claim_generation"
                    ),
                )?;
                unchanged_claims.push((*claim_id).to_string());
            }
            Some(head_claim) => {
                require(
                    head_claim.claim.claim_generation > base_claim.claim.claim_generation,
                    format!(
                        "{claim_id}: changed decision must increase claim_generation"
                    ),
                )?;
                changed_claims.push((*claim_id).to_string());
            }
            None => {
                let retirement = head_retirements.get(claim_id).ok_or_else(|| {
                    format!("{claim_id}: removed active claim lacks a retirement record")
                })?;
                require(
                    retirement.final_generation == base_claim.claim.claim_generation,
                    format!(
                        "{claim_id}: retirement final_generation does not match the removed claim"
                    ),
                )?;
                if let Some(replacement) = &retirement.replacement_claim_id {
                    require(
                        head_claims.contains_key(replacement.as_str()),
                        format!(
                            "{claim_id}: retirement replacement {replacement} is not active"
                        ),
                    )?;
                }
                retired_claims.push((*claim_id).to_string());
            }
        }
    }

    for (claim_id, head_claim) in &head_claims {
        if !base_claims.contains_key(claim_id) {
            require(
                !base_retirements.contains_key(claim_id),
                format!("retired claim {claim_id} cannot be reintroduced"),
            )?;
            require(
                head_claim.claim.claim_generation == 1,
                format!("{claim_id}: newly introduced claim must start at generation 1"),
            )?;
            added_claims.push((*claim_id).to_string());
        }
    }

    unchanged_claims.sort();
    changed_claims.sort();
    added_claims.sort();
    retired_claims.sort();

    Ok(TransitionReport {
        schema: "ripr.formal-claim-registry-transition.v1",
        repository: head.manifest.repository.clone(),
        base_registry_generation: base.generation.clone(),
        head_registry_generation: head.generation.clone(),
        unchanged_claims,
        changed_claims,
        added_claims,
        retired_claims,
        valid: true,
        native_assurance: "not_assessed",
        execution_authority: "not_granted",
    })
}

fn markdown_transition(value: &TransitionReport) -> String {
    format!(
        "# Formal claim registry transition\n\n- Repository: `{}`\n- Base generation: `{}`\n- Head generation: `{}`\n- Unchanged: {}\n- Changed with generation bump: {}\n- Added: {}\n- Retired: {}\n- Native assurance: `{}`\n- Execution authority: `{}`\n",
        value.repository,
        value.base_registry_generation,
        value.head_registry_generation,
        value.unchanged_claims.len(),
        value.changed_claims.len(),
        value.added_claims.len(),
        value.retired_claims.len(),
        value.native_assurance,
        value.execution_authority,
    )
}

#[cfg(test)]
pub(super) fn validate_transition(
    base: &LoadedRegistry,
    head: &LoadedRegistry,
) -> RegistryResult<()> {
    transition(base, head).map(|_| ())
}

pub(super) fn render_transition(
    base: &LoadedRegistry,
    head: &LoadedRegistry,
    format: OutputFormat,
) -> RegistryResult<String> {
    let value = transition(base, head)?;
    render(&value, format, || markdown_transition(&value))
}
