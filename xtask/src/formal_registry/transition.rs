use super::*;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransitionState {
    NotAssessed,
    Preserved,
    ReviewRequired,
}

#[derive(Clone, Debug, Serialize)]
pub struct TransitionRow {
    pub claim_id: String,
    pub state: String,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Transition {
    pub state: TransitionState,
    pub predecessor_registry_generation: Option<String>,
    pub claims: Vec<TransitionRow>,
    pub authority: &'static str,
}

fn replace_exact(value: &mut serde_json::Value, from: &str, to: &str, unordered: bool) {
    match value {
        serde_json::Value::String(text) if text == from => *text = to.to_string(),
        serde_json::Value::Array(values) => {
            for item in values.iter_mut() {
                replace_exact(item, from, to, true);
            }
            // Set-valued fields can reorder after a file move. The assumptions
            // array is an exact ordered commitment and must remain untouched.
            if unordered {
                values.sort_by_key(serde_json::Value::to_string);
            }
        }
        serde_json::Value::Object(values) => {
            for (key, item) in values.iter_mut() {
                replace_exact(item, from, to, key != "assumptions");
            }
        }
        _ => {}
    }
}

fn relocated(
    old: &PreparedClaim,
    new: &PreparedClaim,
    record: &CompatibleRelocation,
) -> AuditResult<bool> {
    if old.claim.claim_id != record.claim_id
        || old.claim.subjects != [record.from_subject.clone()]
        || new.claim.subjects != [record.to_subject.clone()]
    {
        return Ok(false);
    }
    if old.authority_manifest.get(&record.from_file) != Some(&record.source_digest)
        || new.authority_manifest.get(&record.to_file) != Some(&record.source_digest)
    {
        return Ok(false);
    }
    let mut old_authority = old.authority_manifest.clone();
    old_authority.remove(&record.from_file);
    old_authority.insert(record.to_file.clone(), record.source_digest.clone());
    if old_authority != new.authority_manifest {
        return Ok(false);
    }
    let mut before = serde_json::to_value(&old.registration).map_err(|e| e.to_string())?;
    let mut after = serde_json::to_value(&new.registration).map_err(|e| e.to_string())?;
    replace_exact(&mut before, &record.from_file, &record.to_file, true);
    replace_exact(&mut before, &record.from_subject, &record.to_subject, true);
    replace_exact(&mut after, "", "", true);
    Ok(before == after)
}

/// Reject same-generation weakening and silent deletion. A generation increase
/// records a review obligation; it does not accept reduced proof authority.
pub fn compare_predecessor(
    current: &PreparedRegistry,
    predecessor: Option<&PreparedRegistry>,
) -> AuditResult<Transition> {
    validate_prepared(current)?;
    let Some(previous) = predecessor else {
        return Ok(Transition { state: TransitionState::NotAssessed, predecessor_registry_generation: None,
            claims: current.claims.iter().map(|claim| TransitionRow {
                claim_id: claim.claim.claim_id.clone(), state: "NOT_ASSESSED".to_string(),
                reasons: vec!["No predecessor registry/source snapshot supplied; replacement and retirement are not assessed.".to_string()],
            }).collect(), authority: "not_granted" });
    };
    validate_prepared(previous)?;
    formal::require(
        current.registry.generation >= previous.registry.generation,
        "registry generation regressed",
    )?;
    if current.registry_generation != previous.registry_generation {
        formal::require(
            current.registry.generation > previous.registry.generation,
            "changed registry needs a new registry generation",
        )?;
    }
    let old: BTreeMap<_, _> = previous
        .claims
        .iter()
        .map(|c| (&c.claim.claim_id, c))
        .collect();
    let new: BTreeMap<_, _> = current
        .claims
        .iter()
        .map(|c| (&c.claim.claim_id, c))
        .collect();
    for id in old.keys() {
        formal::require(
            new.contains_key(id),
            &format!("claim {id} disappeared; deletion is not accepted retirement"),
        )?;
    }
    let mut review_required = false;
    let mut rows = Vec::new();
    for claim in &current.claims {
        let id = &claim.claim.claim_id;
        let (state, reason) = if let Some(before) = old.get(id) {
            formal::require(
                claim.claim.claim_generation >= before.claim.claim_generation,
                "claim generation regressed",
            )?;
            if claim.claim.claim_generation > before.claim.claim_generation {
                review_required = true;
                (
                    "REVIEW_REQUIRED",
                    "Claim generation advanced; base protection and semantic movement require owner review.",
                )
            } else {
                let metadata_equal =
                    json_bytes(&before.registration)? == json_bytes(&claim.registration)?;
                if metadata_equal && before.authority_manifest == claim.authority_manifest {
                    if before.source_manifest == claim.source_manifest {
                        (
                            "PRESERVED",
                            "Registered proposition and captured source/authority bytes are unchanged.",
                        )
                    } else {
                        review_required = true;
                        (
                            "SOURCE_CHANGED",
                            "Production source changed; previous native evidence is not current for this source.",
                        )
                    }
                } else {
                    let record = current
                        .registry
                        .compatible_relocations
                        .iter()
                        .find(|r| &r.claim_id == id);
                    let compatible = record
                        .map(|r| relocated(before, claim, r))
                        .transpose()?
                        .unwrap_or(false);
                    formal::require(
                        compatible,
                        &format!(
                            "{id}: decision-bearing metadata or proof-authority bytes changed without a new claim generation"
                        ),
                    )?;
                    review_required = true;
                    (
                        "EXACT_RELOCATION",
                        "Exact unchanged authority bytes moved through a recorded relocation; integration still needs review.",
                    )
                }
            }
        } else {
            review_required = true;
            (
                "ADDED",
                "New proposition needs current proof and independent scope review.",
            )
        };
        rows.push(TransitionRow {
            claim_id: id.clone(),
            state: state.to_string(),
            reasons: vec![reason.to_string()],
        });
    }
    Ok(Transition {
        state: if review_required {
            TransitionState::ReviewRequired
        } else {
            TransitionState::Preserved
        },
        predecessor_registry_generation: Some(previous.registry_generation.clone()),
        claims: rows,
        authority: "not_granted",
    })
}
