use super::*;

fn member(pr_number: u64, digit: char) -> Member {
    Member {
        pr_number,
        head: digit.to_string().repeat(40),
    }
}

fn claim(
    id: &str,
    criticality: Criticality,
    route: ProviderRoute,
    source: &str,
    authority: &str,
    owner: &str,
    subject: &str,
) -> ClaimIndexEntry {
    ClaimIndexEntry {
        claim_id: id.to_string(),
        claim_generation: 1,
        criticality,
        phase: Phase::AdmittedMergeGroup,
        provider: if route == ProviderRoute::Native {
            "kani".to_string()
        } else {
            "manual".to_string()
        },
        route,
        owners: vec![owner.to_string()],
        source_closure: vec![source.to_string()],
        authority_paths: vec![authority.to_string()],
        subjects: if route == ProviderRoute::Native {
            vec![subject.to_string()]
        } else {
            Vec::new()
        },
        limitations: Vec::new(),
    }
}

fn fixture() -> SelectionInput {
    SelectionInput {
        schema: "ripr.formal-claim-selection-input.v1".to_string(),
        repository: "EffortlessMetrics/ripr-swarm".to_string(),
        claim_index: ClaimIndex {
            generation: "registry-1".to_string(),
            global_authority_paths: vec!["xtask/src/kani_lane.rs".to_string()],
            claims: vec![
                claim(
                    "ripr.actionability",
                    Criticality::Critical,
                    ProviderRoute::Native,
                    "crates/ripr-contract/src/domain/evidence_state.rs",
                    "crates/ripr/src/kani_smoke.rs",
                    "ripr_contract::domain::EvidenceState::is_actionable",
                    "ripr::kani_smoke::evidence_state_actionable_only_for_actionable",
                ),
                claim(
                    "ripr.repair-finish",
                    Criticality::Critical,
                    ProviderRoute::Unsupported,
                    "crates/ripr/src/domain/repair.rs",
                    "formal/repair-attempt/RepairAttempt.tla",
                    "ripr::domain::repair::finish",
                    "unused",
                ),
                claim(
                    "ripr.advisory-helper",
                    Criticality::Advisory,
                    ProviderRoute::Manual,
                    "crates/ripr/src/advisory.rs",
                    "docs/advisory.md",
                    "ripr::advisory",
                    "unused",
                ),
            ],
        },
        change_set: ChangeSet {
            base: "1".repeat(40),
            group: "2".repeat(40),
            members: vec![member(10, '3'), member(11, '4')],
            expected_registry_generation: "registry-1".to_string(),
            controller_generation: "controller-1".to_string(),
            producer_generation: "producer-1".to_string(),
            proof_profile: "kani-critical-v1".to_string(),
            classification_complete: true,
            changes: vec![ChangedPath {
                path: "docs/README.md".to_string(),
                impact: ImpactClass::NonSemantic,
                semantic_subjects: Vec::new(),
            }],
            limitations: Vec::new(),
        },
    }
}

fn selected(plan: &Plan) -> Vec<&str> {
    plan.claims
        .iter()
        .filter(|claim| claim.disposition == Disposition::Selected)
        .map(|claim| claim.claim_id.as_str())
        .collect()
}

#[test]
fn direct_owner_and_source_closure_change_selects_exact_claim() -> PlanResult<()> {
    let mut input = fixture();
    input.change_set.changes = vec![ChangedPath {
        path: "crates/ripr-contract/src/domain/evidence_state.rs".to_string(),
        impact: ImpactClass::FormalRelevant,
        semantic_subjects: vec!["ripr_contract::domain::EvidenceState::is_actionable".to_string()],
    }];
    let result = plan(&input, "sha256:test".to_string())?;
    assert_eq!(selected(&result), ["ripr.actionability"]);
    assert!(!result.fallback_all_critical);
    assert_eq!(
        result.expected_native_subjects,
        ["ripr::kani_smoke::evidence_state_actionable_only_for_actionable".to_string()]
    );
    Ok(())
}

#[test]
fn harness_or_model_only_change_selects_its_claim() -> PlanResult<()> {
    let mut input = fixture();
    input.change_set.changes = vec![ChangedPath {
        path: "formal/repair-attempt/RepairAttempt.tla".to_string(),
        impact: ImpactClass::FormalRelevant,
        semantic_subjects: Vec::new(),
    }];
    let result = plan(&input, "sha256:test".to_string())?;
    assert_eq!(selected(&result), ["ripr.repair-finish"]);
    assert!(result.expected_native_subjects.is_empty());
    let repair = result
        .claims
        .iter()
        .find(|claim| claim.claim_id == "ripr.repair-finish")
        .ok_or("missing repair claim")?;
    assert_eq!(repair.route, ProviderRoute::Unsupported);
    Ok(())
}

#[test]
fn global_authority_change_falls_back_to_all_critical() -> PlanResult<()> {
    let mut input = fixture();
    input.change_set.changes = vec![ChangedPath {
        path: "xtask/src/kani_lane.rs".to_string(),
        impact: ImpactClass::FormalRelevant,
        semantic_subjects: Vec::new(),
    }];
    let result = plan(&input, "sha256:test".to_string())?;
    assert!(result.fallback_all_critical);
    assert_eq!(
        selected(&result),
        ["ripr.actionability", "ripr.repair-finish"]
    );
    assert_eq!(result.selected_claim_count, result.critical_claim_count);
    Ok(())
}

#[test]
fn unknown_or_incomplete_classification_falls_back_to_all_critical() -> PlanResult<()> {
    for (classification_complete, impact) in [
        (false, ImpactClass::FormalIrrelevant),
        (true, ImpactClass::Unknown),
    ] {
        let mut input = fixture();
        input.change_set.classification_complete = classification_complete;
        input.change_set.changes = vec![ChangedPath {
            path: "crates/ripr/src/unknown.rs".to_string(),
            impact,
            semantic_subjects: Vec::new(),
        }];
        let result = plan(&input, "sha256:test".to_string())?;
        assert!(result.fallback_all_critical);
        assert_eq!(result.selected_claim_count, 2);
    }
    Ok(())
}

#[test]
fn complete_nonsemantic_changes_can_select_none() -> PlanResult<()> {
    let input = fixture();
    let result = plan(&input, "sha256:test".to_string())?;
    assert!(!result.fallback_all_critical);
    assert!(selected(&result).is_empty());
    assert!(result.zero_selection_basis.is_some());
    Ok(())
}

#[test]
fn formal_relevant_unmapped_change_is_rejected_or_widened() -> PlanResult<()> {
    let mut input = fixture();
    input.change_set.changes = vec![ChangedPath {
        path: "crates/ripr/src/new_formal_owner.rs".to_string(),
        impact: ImpactClass::FormalRelevant,
        semantic_subjects: Vec::new(),
    }];
    let result = plan(&input, "sha256:test".to_string())?;
    assert!(result.fallback_all_critical);
    assert!(
        result
            .fallback_reasons
            .iter()
            .any(|reason| reason.contains("unmapped_formal_relevant_change"))
    );
    Ok(())
}

#[test]
fn manual_and_unsupported_routes_remain_visible_in_fallback() -> PlanResult<()> {
    let mut input = fixture();
    input
        .claim_index
        .claims
        .get_mut(1)
        .ok_or("missing repair claim")?
        .route = ProviderRoute::Manual;
    input.change_set.classification_complete = false;
    let result = plan(&input, "sha256:test".to_string())?;
    assert_eq!(selected(&result).len(), 2);
    let manual = result
        .claims
        .iter()
        .find(|claim| claim.claim_id == "ripr.repair-finish")
        .ok_or("missing manual claim")?;
    assert_eq!(manual.route, ProviderRoute::Manual);
    assert!(
        result
            .expected_native_subjects
            .iter()
            .all(|subject| !subject.contains("repair"))
    );
    Ok(())
}

#[test]
fn empty_change_facts_fall_back_to_all_critical() -> PlanResult<()> {
    let mut input = fixture();
    input.change_set.changes.clear();
    let result = plan(&input, "sha256:test".to_string())?;
    assert!(result.fallback_all_critical);
    assert!(
        result
            .fallback_reasons
            .iter()
            .any(|reason| reason == "no_change_facts_supplied")
    );
    assert_eq!(result.selected_claim_count, result.critical_claim_count);
    Ok(())
}

#[test]
fn historical_native_phase_stays_visible_without_native_subjects() -> PlanResult<()> {
    let mut input = fixture();
    input
        .claim_index
        .claims
        .first_mut()
        .ok_or("missing actionability claim")?
        .phase = Phase::MergeReady;
    input.change_set.classification_complete = false;
    let result = plan(&input, "sha256:test".to_string())?;
    let actionability = result
        .claims
        .iter()
        .find(|claim| claim.claim_id == "ripr.actionability")
        .ok_or("missing actionability claim")?;
    assert_eq!(actionability.disposition, Disposition::Selected);
    assert!(
        actionability
            .reasons
            .iter()
            .any(|reason| reason == "native_phase_not_admitted_for_merge_group")
    );
    assert!(result.expected_native_subjects.is_empty());
    Ok(())
}

#[test]
fn index_without_critical_claims_rejects() -> PlanResult<()> {
    let mut input = fixture();
    for claim in &mut input.claim_index.claims {
        claim.criticality = Criticality::Advisory;
    }
    assert!(plan(&input, "sha256:test".to_string()).is_err());
    Ok(())
}

#[test]
fn stale_registry_or_invalid_stack_identity_rejects() -> PlanResult<()> {
    let mut input = fixture();
    input.change_set.expected_registry_generation = "registry-2".to_string();
    assert!(plan(&input, "sha256:test".to_string()).is_err());
    let mut input = fixture();
    let first_head = input
        .change_set
        .members
        .first()
        .ok_or("missing first member")?
        .head
        .clone();
    input
        .change_set
        .members
        .get_mut(1)
        .ok_or("missing second member")?
        .head = first_head;
    assert!(plan(&input, "sha256:test".to_string()).is_err());
    let mut input = fixture();
    input.change_set.group = input.change_set.base.clone();
    assert!(plan(&input, "sha256:test".to_string()).is_err());
    Ok(())
}

#[test]
fn unregistered_path_resemblance_cannot_enter_critical_assurance() -> PlanResult<()> {
    let mut input = fixture();
    input.change_set.changes = vec![ChangedPath {
        path: "fixtures/lookalike/kani_smoke.rs".to_string(),
        impact: ImpactClass::FormalIrrelevant,
        semantic_subjects: Vec::new(),
    }];
    let result = plan(&input, "sha256:test".to_string())?;
    assert!(selected(&result).is_empty());
    Ok(())
}

#[test]
fn input_order_does_not_change_semantic_plan() -> PlanResult<()> {
    let mut left = fixture();
    left.change_set.classification_complete = false;
    let mut right = left.clone();
    right.claim_index.claims.reverse();
    let left_plan = plan(&left, "sha256:same".to_string())?;
    let right_plan = plan(&right, "sha256:same".to_string())?;
    assert_eq!(
        serde_json::to_value(left_plan).map_err(|error| error.to_string())?,
        serde_json::to_value(right_plan).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[test]
fn json_and_markdown_project_the_same_plan() -> PlanResult<()> {
    let mut input = fixture();
    input.change_set.classification_complete = false;
    let result = plan(&input, "sha256:test".to_string())?;
    let json = serde_json::to_string(&result).map_err(|error| error.to_string())?;
    let markdown = render_markdown(&result);
    assert!(json.contains("\"fallback_all_critical\":true"));
    assert!(markdown.contains("All-critical fallback: `true`"));
    for claim in &result.claims {
        assert!(markdown.contains(&claim.claim_id));
    }
    Ok(())
}

#[test]
fn duplicate_keys_unknown_fields_and_missing_values_reject() -> PlanResult<()> {
    let input = fixture();
    let encoded = serde_json::to_string(&input).map_err(|error| error.to_string())?;
    let duplicate = encoded.replacen("{", "{\"repository\":\"shadow\",", 1);
    assert!(parse_input(duplicate.as_bytes()).is_err());
    let unknown = encoded.replacen("{", "{\"execute\":true,", 1);
    assert!(parse_input(unknown.as_bytes()).is_err());
    let mut value = serde_json::to_value(input).map_err(|error| error.to_string())?;
    value
        .get_mut("change_set")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("missing change_set")?
        .remove("classification_complete");
    assert!(parse_input(&serde_json::to_vec(&value).map_err(|error| error.to_string())?).is_err());
    Ok(())
}

#[test]
fn complete_ordered_stack_identity_is_preserved() -> PlanResult<()> {
    let input = fixture();
    let result = plan(&input, "sha256:test".to_string())?;
    assert_eq!(result.members, input.change_set.members);
    assert_eq!(result.base, input.change_set.base);
    assert_eq!(result.group, input.change_set.group);
    assert_eq!(result.execution_authority, "not_granted");
    assert_eq!(result.native_assurance, "not_assessed");
    Ok(())
}
