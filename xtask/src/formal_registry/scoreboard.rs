use super::*;
use formal::{Criticality, Phase, ProcessState, Status};

#[derive(Debug, Serialize)]
pub struct LastResult {
    pub run_id: String,
    pub registry_generation: String,
    pub claim_generation: u64,
    pub source_digest: String,
    pub provider: String,
    pub provider_version: String,
    pub declared_status: Status,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct AssuranceRow {
    pub claim_id: String,
    pub claim_generation: u64,
    pub semantic_family: CriticalFamily,
    pub criticality: Criticality,
    pub phase: Phase,
    pub provider: String,
    pub status: Status,
    pub provenance: String,
    pub source_digest: String,
    pub source_file_count: usize,
    pub required_covers: usize,
    pub native_cover_mappings: usize,
    pub satisfied_covers: Option<u64>,
    pub negative_control_state: &'static str,
    pub negative_controls: Vec<String>,
    pub negative_control_observations: Vec<ControlObservation>,
    pub alternative_evidence_owner: Option<String>,
    pub owner_spec: String,
    pub owner_issue: u64,
    pub reasons: Vec<String>,
    pub non_claims: Vec<String>,
    pub last_result: Option<LastResult>,
}

#[derive(Debug, Serialize)]
pub struct Scoreboard {
    pub schema: &'static str,
    pub registry_generation: String,
    pub execution_authority: &'static str,
    pub producer_authentication: &'static str,
    pub registered_claims: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_semantic_family: BTreeMap<String, usize>,
    pub by_criticality: BTreeMap<String, usize>,
    pub by_phase: BTreeMap<String, usize>,
    pub claims: Vec<AssuranceRow>,
    pub claims_without_current_native_result: Vec<String>,
    pub claims_without_executed_negative_control: Vec<String>,
    pub claims_without_required_cover_evidence: Vec<String>,
    pub claims_without_native_cover_mapping: Vec<String>,
    pub controls_without_native_assertion_mapping: Vec<String>,
    pub changed_critical_owners_without_current_applicable_claim: Vec<String>,
    pub change_facts: &'static str,
    pub unregistered_native_observations: usize,
    pub proof_authority_transition: Transition,
    pub limitations: Vec<&'static str>,
}

fn label(status: &Status) -> &'static str {
    match status {
        Status::Established => "ESTABLISHED",
        Status::Failed => "FAILED",
        Status::NotEstablished => "NOT_ESTABLISHED",
        Status::Stale => "STALE",
        Status::Unsupported => "UNSUPPORTED",
        Status::NotProven => "NOT_PROVEN",
    }
}

/// Native stdout and registration custody are inspected through the shared
/// native owner. This is a local retained-byte observation, never a signature,
/// remote attestation, required-check result, or governance decision.
pub fn scoreboard(
    current: &PreparedRegistry,
    previous: Option<&PreparedRegistry>,
    observation: Option<(&Run, Option<&Path>)>,
    changed_owners: &[String],
) -> AuditResult<Scoreboard> {
    let transition = compare_predecessor(current, previous)?;
    let mut retained = None;
    if let Some((run, artifacts)) = observation {
        if let (Some(directory), Some(registration)) = (
            artifacts,
            run.execution.as_ref().and_then(|e| e.registration.as_ref()),
        ) {
            let captured = native::verify_execution_artifacts(run, directory)?;
            let bytes = native::verify_artifact(directory, registration, 32 * 1_048_576)?;
            let prepared: PreparedRegistry = formal::parse_json(&bytes)?;
            validate_binding(&prepared, run, &captured)?;
            retained = Some(prepared);
        } else {
            // Reuse the document auditor; consistent documents alone receive no
            // native assurance. Wrong generations/provider may be refused here.
            formal::audit(
                &current
                    .claims
                    .iter()
                    .map(|p| p.claim.clone())
                    .collect::<Vec<_>>(),
                Some(run),
            )?;
        }
    }
    let mut rows = Vec::new();
    for prepared in &current.claims {
        let claim = &prepared.claim;
        let mut row = AssuranceRow {
            claim_id: claim.claim_id.clone(),
            claim_generation: claim.claim_generation,
            semantic_family: prepared.semantic_family.clone(),
            criticality: claim.criticality.clone(),
            phase: claim.phase.clone(),
            provider: claim.provider.clone(),
            status: match claim.provider.as_str() {
                "manual" => Status::NotProven,
                "kani" => Status::NotEstablished,
                _ => Status::Unsupported,
            },
            provenance: "not_observed".to_string(),
            source_digest: prepared.source_digest.clone(),
            source_file_count: prepared.source_manifest.len(),
            required_covers: claim.required_covers.len(),
            native_cover_mappings: prepared
                .registration
                .cover_requirements
                .iter()
                .filter(|cover| cover.native.is_some())
                .count(),
            satisfied_covers: None,
            negative_control_state: "retained_no_admitted_execution",
            negative_controls: claim.negative_controls.clone(),
            negative_control_observations: Vec::new(),
            alternative_evidence_owner: prepared.registration.alternative_evidence_owner.clone(),
            owner_spec: claim.owner_spec.clone(),
            owner_issue: claim.owner_issue,
            reasons: vec!["No current registered native observation.".to_string()],
            non_claims: claim.non_claims.clone(),
            last_result: None,
        };
        if let Some((run, _)) = observation
            && let Some(result) = run.results.iter().find(|r| r.claim_id == claim.claim_id)
        {
            row.last_result = Some(LastResult {
                run_id: run.run_id.clone(),
                registry_generation: run.claim_registry_generation.clone(),
                claim_generation: result.claim_generation,
                source_digest: result.source_digest.clone(),
                provider: run.provider.name.clone(),
                provider_version: run.provider.version.clone(),
                declared_status: result.status.clone(),
                duration_ms: result.duration_ms,
            });
            let mismatched = result.claim_generation != claim.claim_generation
                || run.claim_registry_generation != current.registry_generation
                || result.source_digest != prepared.source_digest
                || result.bounds_digest != prepared.bounds_digest
                || result
                    .assumptions_digest
                    .as_ref()
                    .is_some_and(|digest| digest != &prepared.assumptions_digest)
                || prepared.provider_profile.as_ref().is_some_and(|profile| {
                    run.provider.version != profile.version
                        || run
                            .provider
                            .solver_version
                            .as_ref()
                            .is_some_and(|version| version != &profile.solver_version)
                        || run
                            .toolchain
                            .target
                            .as_ref()
                            .is_some_and(|target| target != &profile.target)
                });
            if mismatched {
                row.status = Status::Stale;
                row.provenance = "supplied_identity_mismatch".to_string();
                row.reasons = vec!["Supplied claim/source/provider/configuration identity differs from the current registration.".to_string()];
            } else if let Some(captured_registry) = &retained {
                row.provenance = "local_retained_bytes_checked_not_authenticated".to_string();
                let captured = captured_registry
                    .claims
                    .iter()
                    .find(|p| p.claim.claim_id == claim.claim_id);
                let applicable = run.claim_registry_generation == current.registry_generation
                    && captured.is_some_and(|p| {
                        p.source_digest == prepared.source_digest
                            && p.authority_manifest == prepared.authority_manifest
                            && p.claim.claim_generation == claim.claim_generation
                            && p.provider_profile == prepared.provider_profile
                    });
                if !applicable {
                    row.status = Status::Stale;
                    row.reasons = vec!["Native run belongs to a different registry/claim/source/authority/provider configuration.".to_string()];
                } else if result.status == Status::Established
                    && !(run.process.state == ProcessState::Completed
                        && run.process.exit_code == Some(0)
                        && !run.process.timed_out
                        && run.inventory.complete)
                {
                    row.status = Status::NotEstablished;
                    row.reasons =
                        vec!["Favorable row lacks a complete successful native run.".to_string()];
                } else {
                    row.status = result.status.clone();
                    row.satisfied_covers = Some(result.covers.satisfied);
                    row.reasons = result.reasons.clone();
                }
            } else {
                row.provenance = "supplied_documents_not_authenticated".to_string();
                row.reasons = vec!["Supplied success/failure declarations have no verified registration and raw-byte custody.".to_string()];
            }
        }
        rows.push(row);
    }
    let mut counts: BTreeMap<String, usize> = [
        "ESTABLISHED",
        "FAILED",
        "NOT_ESTABLISHED",
        "STALE",
        "UNSUPPORTED",
        "NOT_PROVEN",
    ]
    .into_iter()
    .map(|s| (s.to_string(), 0))
    .collect();
    for row in &rows {
        *counts.entry(label(&row.status).to_string()).or_default() += 1;
    }
    let mut families = BTreeMap::new();
    let mut criticalities = BTreeMap::new();
    let mut phases = BTreeMap::new();
    for row in &rows {
        for (map, value) in [
            (&mut families, serde_json::to_value(&row.semantic_family)),
            (&mut criticalities, serde_json::to_value(&row.criticality)),
            (&mut phases, serde_json::to_value(&row.phase)),
        ] {
            let value = value.map_err(|e| e.to_string())?;
            let word = value
                .as_str()
                .ok_or("group vocabulary must be a string")?
                .to_string();
            *map.entry(word).or_insert(0) += 1;
        }
    }
    let current_claim = |id: &str| {
        rows.iter()
            .any(|r| r.claim_id == id && matches!(r.status, Status::Established | Status::Failed))
    };
    let mut uncovered = BTreeSet::new();
    for owner in changed_owners {
        native::relative_path(owner)?;
        if !current.claims.iter().any(|claim| {
            claim.source_manifest.contains_key(owner)
                && claim.claim.criticality == Criticality::Critical
                && current_claim(&claim.claim.claim_id)
        }) {
            uncovered.insert(owner.clone());
        }
    }
    Ok(Scoreboard {
        schema: "ripr.formal-assurance-scoreboard.v1",
        registry_generation: current.registry_generation.clone(),
        execution_authority: "not_granted",
        producer_authentication: "not_established",
        registered_claims: rows.len(),
        by_status: counts,
        by_semantic_family: families,
        by_criticality: criticalities,
        by_phase: phases,
        claims_without_current_native_result: rows
            .iter()
            .filter(|r| !matches!(r.status, Status::Established | Status::Failed))
            .map(|r| r.claim_id.clone())
            .collect(),
        claims_without_executed_negative_control: rows.iter().map(|r| r.claim_id.clone()).collect(),
        claims_without_required_cover_evidence: rows
            .iter()
            .filter(|r| r.satisfied_covers != Some(r.required_covers as u64))
            .map(|r| r.claim_id.clone())
            .collect(),
        claims_without_native_cover_mapping: rows
            .iter()
            .filter(|r| r.provider == "kani" && r.native_cover_mappings != r.required_covers)
            .map(|r| r.claim_id.clone())
            .collect(),
        controls_without_native_assertion_mapping: current
            .claims
            .iter()
            .filter(|claim| claim.claim.provider == "kani")
            .flat_map(|claim| &claim.registration.negative_controls)
            .filter(|control| control.expected_failures.is_empty())
            .map(|control| control.id.clone())
            .collect(),
        changed_critical_owners_without_current_applicable_claim: uncovered.into_iter().collect(),
        change_facts: "supplied_paths_not_authenticated_or_complete",
        unregistered_native_observations: observation
            .map_or(0, |(r, _)| r.unregistered_results.len()),
        proof_authority_transition: transition,
        claims: rows,
        limitations: vec![
            "The source census remains separate; unregistered harnesses do not increase registered assurance.",
            "Retained byte equality and local native replay do not authenticate the producer or grant CI authority.",
            "Only explicitly supplied positive/mutant/restored pairs can admit a declared negative control; other retained native runs remain outside this evidence join.",
            "A paired observation discriminates one exact production mutation under the registered domain and profile; it is not an authenticated mutation score or specification-completeness claim.",
            "No aggregate formal-coverage percentage is defined; source closure is conservative, not a minimal dependency proof.",
        ],
    })
}

/// Optional paired controls use the same native replay and registered positive
/// as the main scoreboard. The projector still executes no verifier or command.
pub fn scoreboard_with_control_pairs(
    current: &PreparedRegistry,
    previous: Option<&PreparedRegistry>,
    observation: Option<(&Run, Option<&Path>)>,
    changed_owners: &[String],
    pairs: &[std::path::PathBuf],
) -> AuditResult<Scoreboard> {
    formal::require(
        pairs.len() <= 16,
        "at most sixteen retained control pairs are admitted per projection",
    )?;
    let mut board = scoreboard(current, previous, observation, changed_owners)?;
    if pairs.is_empty() {
        return Ok(board);
    }
    let (run, Some(_)) =
        observation.ok_or("control pairs require a positive run and its retained artifacts")?
    else {
        return Err("control pairs require the positive run's retained artifacts".into());
    };
    let mut seen = BTreeSet::new();
    for path in pairs {
        let admitted = validate_control_pair(current, run, path)?;
        formal::require(
            seen.insert(admitted.control_id.clone()),
            "duplicate supplied control pair",
        )?;
        let row = board
            .claims
            .iter_mut()
            .find(|row| row.claim_id == admitted.claim_id)
            .ok_or("paired claim missing from scoreboard")?;
        formal::require(
            row.status == Status::Established,
            "paired control cannot attach to a noncurrent or unestablished positive row",
        )?;
        row.negative_control_observations.push(admitted);
    }
    for row in &mut board.claims {
        row.negative_control_observations
            .sort_by(|a, b| a.control_id.cmp(&b.control_id));
        if !row.negative_control_observations.is_empty() {
            row.negative_control_state =
                if row.negative_control_observations.len() == row.negative_controls.len() {
                    "local_native_discrimination_observed"
                } else {
                    "partial_local_native_discrimination_observed"
                };
        }
    }
    board.claims_without_executed_negative_control = board
        .claims
        .iter()
        .filter(|row| row.negative_control_observations.len() != row.negative_controls.len())
        .map(|row| row.claim_id.clone())
        .collect();
    Ok(board)
}

/// Exact projection consumed by #14542. No selection logic is implemented here.
pub fn claim_index(prepared: &PreparedRegistry) -> AuditResult<serde_json::Value> {
    validate_prepared(prepared)?;
    let mut authority: BTreeSet<String> = [
        prepared.registry_path.clone(),
        "xtask/src/formal_registry.rs".to_string(),
        "xtask/src/formal_registry/prepare.rs".to_string(),
        "xtask/src/formal_registry/transition.rs".to_string(),
        "xtask/src/formal_registry/scoreboard.rs".to_string(),
        "xtask/src/formal_evidence.rs".to_string(),
    ]
    .into_iter()
    .collect();
    for claim in &prepared.claims {
        authority.extend(
            claim
                .source_manifest
                .keys()
                .filter(|path| {
                    path.starts_with("xtask/src/formal_registry/")
                        || path.starts_with("xtask/src/formal_evidence/")
                })
                .cloned(),
        );
    }
    Ok(serde_json::json!({
        "generation": prepared.registry_generation,
        "global_authority_paths": authority,
        "claims": prepared.claims.iter().map(|p| serde_json::json!({
            "claim_id": p.claim.claim_id, "claim_generation": p.claim.claim_generation,
            "criticality": p.claim.criticality, "phase": p.claim.phase, "provider": p.claim.provider,
            "route": match p.claim.provider.as_str() { "kani" => "native", "manual" => "manual", _ => "unsupported" },
            "owners": p.claim.owners, "source_closure": p.source_manifest.keys().collect::<Vec<_>>(),
            "authority_paths": p.registration.authority_paths, "subjects": p.claim.subjects,
            "limitations": p.claim.non_claims,
        })).collect::<Vec<_>>()
    }))
}

fn cell(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
}

pub fn render_markdown(board: &Scoreboard) -> String {
    let mut text = format!(
        "# Critical formal assurance scoreboard\n\nRegistry: `{}`. Registered claims: {}.\n\nExecution authority: **{}**. Producer authentication: **{}**.\n\n| Claim | Generation | Family | Status | Covers | Negative control |\n| --- | --- | --- | --- | --- | --- |\n",
        board.registry_generation,
        board.registered_claims,
        board.execution_authority,
        board.producer_authentication
    );
    for row in &board.claims {
        text.push_str(&format!(
            "| {} | {} | {:?} | {} | {}/{} | {} |\n",
            cell(&row.claim_id),
            row.claim_generation,
            row.semantic_family,
            label(&row.status),
            row.satisfied_covers
                .map_or_else(|| "not observed".to_string(), |v| v.to_string()),
            row.required_covers,
            row.negative_control_state
        ));
    }
    for (name, counts) in [
        ("Families", &board.by_semantic_family),
        ("Criticality", &board.by_criticality),
        ("Phases", &board.by_phase),
    ] {
        text.push_str(&format!(
            "\n{name}: {}.\n",
            counts
                .iter()
                .map(|(key, count)| format!("{}={count}", cell(key)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let transition = match board.proof_authority_transition.state {
        super::transition::TransitionState::NotAssessed => "NOT_ASSESSED",
        super::transition::TransitionState::Preserved => "PRESERVED",
        super::transition::TransitionState::ReviewRequired => "REVIEW_REQUIRED",
    };
    text.push_str(&format!(
        "\nProof-authority transition: **{transition}**; authority: **{}**.\n",
        board.proof_authority_transition.authority
    ));
    for row in &board.claims {
        text.push_str(&format!("\n## {}\n\nOwner: `{}` / #{}. Phase: {:?}; criticality: {:?}.\n\nSource: `{}` ({} files). Provenance: {}.\n",
            cell(&row.claim_id), cell(&row.owner_spec), row.owner_issue, row.phase, row.criticality,
            row.source_digest, row.source_file_count, row.provenance));
        if let Some(owner) = &row.alternative_evidence_owner {
            text.push_str(&format!(
                "\nAlternative/manual evidence owner: {}.\n",
                cell(owner)
            ));
        }
        if row.provider == "kani" {
            text.push_str(&format!(
                "\nExact native cover mappings: {}/{}.\n",
                row.native_cover_mappings, row.required_covers
            ));
        }
        if let Some(last) = &row.last_result {
            text.push_str(&format!(
                "\nLast result: `{}`; {} {}; declared {}; duration {:?} ms.\n",
                cell(&last.run_id),
                cell(&last.provider),
                cell(&last.provider_version),
                label(&last.declared_status),
                last.duration_ms
            ));
        }
        for control in &row.negative_control_observations {
            text.push_str(&format!(
                "\nNegative control `{}`: **{}**; {}. Positive `{}`, mutant `{}`, restored `{}`. Exact failed assertions: {}. Covers: {}.\n",
                cell(&control.control_id), control.state, control.provenance,
                cell(&control.positive_run_id), cell(&control.mutant_run_id), cell(&control.restored_run_id),
                control.failed_assertions.iter().map(|id| cell(id)).collect::<Vec<_>>().join(", "),
                control.satisfied_covers
            ));
            for limitation in &control.non_claims {
                text.push_str(&format!("\n- {}\n", cell(limitation)));
            }
        }
        for reason in row.reasons.iter().chain(&row.non_claims) {
            text.push_str(&format!("\n- {}\n", cell(reason)));
        }
    }
    text.push_str("\n## Visible gaps\n");
    for (name, values) in [
        (
            "Without current native result",
            &board.claims_without_current_native_result,
        ),
        (
            "Without admitted negative-control execution",
            &board.claims_without_executed_negative_control,
        ),
        (
            "Without required cover evidence",
            &board.claims_without_required_cover_evidence,
        ),
        (
            "Without qualified native cover mapping",
            &board.claims_without_native_cover_mapping,
        ),
        (
            "Controls without qualified native assertion mapping",
            &board.controls_without_native_assertion_mapping,
        ),
        (
            "Changed critical owners without current applicable claim",
            &board.changed_critical_owners_without_current_applicable_claim,
        ),
    ] {
        text.push_str(&format!(
            "\n{name}: {}.\n",
            if values.is_empty() {
                "none".to_string()
            } else {
                values
                    .iter()
                    .map(|s| cell(s))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
    }
    text.push_str(&format!(
        "\nUnregistered native observations: {} (excluded from registered assurance).\n",
        board.unregistered_native_observations
    ));
    for limitation in &board.limitations {
        text.push_str(&format!("\n- {}\n", cell(limitation)));
    }
    text
}
