use super::*;

fn load_claim(
    fixture: &Fixture,
    claim_id: &str,
    proposition: &str,
    claim_generation: u64,
    registry_generation: u64,
) -> RegistryResult<LoadedRegistry> {
    let mut claim = fixture.claim(claim_id, proposition)?;
    claim.claim_generation = claim_generation;
    load::load_registry(&fixture.write_registry(&claim, registry_generation)?)
}

fn set_retirements(fixture: &Fixture, retirements: serde_json::Value) -> RegistryResult<()> {
    let path = fixture.0.join("policy/formal-claims.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    manifest["retirements"] = retirements;
    fs::write(
        path,
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn retirement(claim_id: &str, replacement: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "claim_id": claim_id,
        "final_generation": 1,
        "replacement_claim_id": replacement,
        "reason": "synthetic explicit retirement"
    })
}

fn reload(fixture: &Fixture) -> RegistryResult<LoadedRegistry> {
    load::load_registry(&fixture.0.join("policy/formal-claims.json"))
}

fn transition_error(base: &LoadedRegistry, head: &LoadedRegistry) -> RegistryResult<String> {
    refusal(transition::render_transition(
        base,
        head,
        OutputFormat::Json,
    ))
}

#[test]
fn unchanged_registry_rejects_numeric_generation_movement() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base = load_claim(&base_fixture, "example.active", "Same decision.", 1, 2)?;
    let head_fixture = Fixture::new()?;
    for generation in [1, 3] {
        let head = load_claim(
            &head_fixture,
            "example.active",
            "Same decision.",
            1,
            generation,
        )?;
        assert_eq!(
            transition_error(&base, &head)?,
            "unchanged registry cannot change registry_generation"
        );
    }
    let head = load_claim(&head_fixture, "example.active", "Same decision.", 1, 2)?;
    transition::validate_transition(&base, &head)?;
    Ok(())
}

#[test]
fn changed_registry_requires_an_increased_generation() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base = load_claim(&base_fixture, "example.active", "Old decision.", 1, 2)?;
    let head_fixture = Fixture::new()?;
    for generation in [1, 2] {
        let head = load_claim(
            &head_fixture,
            "example.active",
            "New decision.",
            2,
            generation,
        )?;
        assert_eq!(
            transition_error(&base, &head)?,
            "changed registry must increase registry_generation"
        );
    }
    let head = load_claim(&head_fixture, "example.active", "New decision.", 2, 3)?;
    transition::validate_transition(&base, &head)?;
    Ok(())
}

#[test]
fn unchanged_decision_cannot_change_its_claim_generation() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base = load_claim(&base_fixture, "example.active", "Same decision.", 1, 1)?;
    let head_fixture = Fixture::new()?;
    let head = load_claim(&head_fixture, "example.active", "Same decision.", 2, 2)?;
    assert_eq!(
        transition_error(&base, &head)?,
        "example.active: unchanged decision cannot change claim_generation"
    );
    Ok(())
}

#[test]
fn newly_added_claim_starts_at_generation_one() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base = load_claim(&base_fixture, "example.old", "Old decision.", 1, 1)?;
    let head_fixture = Fixture::new()?;
    load_claim(&head_fixture, "example.new", "New decision.", 2, 2)?;
    set_retirements(
        &head_fixture,
        serde_json::json!([retirement("example.old", None)]),
    )?;
    assert_eq!(
        transition_error(&base, &reload(&head_fixture)?)?,
        "example.new: newly introduced claim must start at generation 1"
    );
    load_claim(&head_fixture, "example.new", "New decision.", 1, 2)?;
    set_retirements(
        &head_fixture,
        serde_json::json!([retirement("example.old", None)]),
    )?;
    transition::validate_transition(&base, &reload(&head_fixture)?)?;
    Ok(())
}

#[test]
fn removed_claim_requires_its_explicit_retirement() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base = load_claim(&base_fixture, "example.old", "Old decision.", 1, 1)?;
    let head_fixture = Fixture::new()?;
    let head = load_claim(&head_fixture, "example.new", "New decision.", 1, 2)?;
    assert_eq!(
        transition_error(&base, &head)?,
        "example.old: removed active claim lacks a retirement record"
    );
    Ok(())
}

#[test]
fn retirement_must_name_the_exact_final_generation() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base = load_claim(&base_fixture, "example.old", "Old decision.", 2, 1)?;
    let head_fixture = Fixture::new()?;
    load_claim(&head_fixture, "example.new", "New decision.", 1, 2)?;
    let mut record = retirement("example.old", Some("example.new"));
    set_retirements(&head_fixture, serde_json::json!([record.clone()]))?;
    assert_eq!(
        transition_error(&base, &reload(&head_fixture)?)?,
        "example.old: retirement final_generation does not match the active base claim"
    );
    record["final_generation"] = serde_json::json!(2);
    set_retirements(&head_fixture, serde_json::json!([record]))?;
    transition::validate_transition(&base, &reload(&head_fixture)?)?;
    Ok(())
}

#[test]
fn accepted_retirement_cannot_be_removed_rewritten_or_reactivated() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    load_claim(&base_fixture, "example.active", "Active decision.", 1, 1)?;
    let record = retirement("example.retired", None);
    set_retirements(&base_fixture, serde_json::json!([record.clone()]))?;
    let base = reload(&base_fixture)?;
    let head_fixture = Fixture::new()?;
    let head = load_claim(&head_fixture, "example.active", "Active decision.", 1, 2)?;
    assert_eq!(
        transition_error(&base, &head)?,
        "retirement example.retired was removed or rewritten"
    );

    let mut rewritten = record;
    rewritten["reason"] = serde_json::json!("rewritten historical reason");
    set_retirements(&head_fixture, serde_json::json!([rewritten]))?;
    assert_eq!(
        transition_error(&base, &reload(&head_fixture)?)?,
        "retirement example.retired was removed or rewritten"
    );

    let reactivated_fixture = Fixture::new()?;
    let reactivated = load_claim(
        &reactivated_fixture,
        "example.retired",
        "Reactivated decision.",
        1,
        2,
    )?;
    assert_eq!(
        transition_error(&base, &reactivated)?,
        "retirement example.retired was removed or rewritten"
    );
    Ok(())
}

#[test]
fn retirement_replacement_must_be_active_and_cannot_be_itself() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    load_claim(&fixture, "example.active", "Active decision.", 1, 1)?;
    for replacement in ["example.absent", "example.retired"] {
        set_retirements(
            &fixture,
            serde_json::json!([retirement("example.retired", Some(replacement))]),
        )?;
        assert_eq!(
            refusal(reload(&fixture))?,
            "example.retired: replacement must name another active claim"
        );
    }
    set_retirements(
        &fixture,
        serde_json::json!([retirement("example.retired", Some("example.active"))]),
    )?;
    reload(&fixture)?;
    Ok(())
}

#[test]
fn active_and_retired_memberships_are_disjoint() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    load_claim(&fixture, "example.active", "Active decision.", 1, 1)?;
    set_retirements(
        &fixture,
        serde_json::json!([retirement("example.active", None)]),
    )?;
    assert_eq!(
        refusal(reload(&fixture))?,
        "example.active is both active and retired"
    );
    Ok(())
}

#[test]
fn historical_replacement_must_remain_active() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    load_claim(&base_fixture, "example.second", "Second decision.", 1, 1)?;
    let first = retirement("example.first", Some("example.second"));
    set_retirements(&base_fixture, serde_json::json!([first.clone()]))?;
    reload(&base_fixture)?;

    let head_fixture = Fixture::new()?;
    load_claim(&head_fixture, "example.third", "Third decision.", 1, 2)?;
    set_retirements(
        &head_fixture,
        serde_json::json!([first, retirement("example.second", Some("example.third"))]),
    )?;
    assert_eq!(
        refusal(reload(&head_fixture))?,
        "example.first: replacement must name another active claim"
    );
    Ok(())
}
