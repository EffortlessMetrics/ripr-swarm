use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

mod transition_controls;

#[cfg(unix)]
mod path_controls;

fn refusal<T>(outcome: RegistryResult<T>) -> RegistryResult<String> {
    match outcome {
        Ok(_) => Err("expected input refusal".to_string()),
        Err(error) => Ok(error),
    }
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> RegistryResult<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "formal-claim-registry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in [
            "policy/formal-claims.d",
            "docs/formal-verification",
            "docs/specs",
            "src",
        ] {
            fs::create_dir_all(root.join(directory)).map_err(|error| error.to_string())?;
        }
        for file in [
            "docs/formal-verification/CLAIMS.md",
            "docs/specs/RIPR-SPEC-0371-kani-bounded-model-checking-lane.md",
            "src/a.rs",
            "src/example.rs",
            "src/z.rs",
        ] {
            fs::write(root.join(file), "synthetic fixture\n").map_err(|error| error.to_string())?;
        }
        Ok(Self(root))
    }

    fn claim(&self, claim_id: &str, proposition: &str) -> RegistryResult<ClaimDocument> {
        load::parse_unique(
            serde_json::json!({
                "schema": "ripr.formal-claim.v1",
                "claim_id": claim_id,
                "claim_generation": 1,
                "family": "bounded_model_check",
                "provider": "kani",
                "criticality": "critical",
                "phase": "admitted_merge_group",
                "proposition": proposition,
                "non_claims": ["synthetic test claim only"],
                "owners": ["example::owner"],
                "source_closure": ["src/example.rs"],
                "domain": "two synthetic states",
                "assumptions": [],
                "trust_dependencies": ["synthetic provider"],
                "observable": "boolean result",
                "bounds": {"states": 2},
                "subjects": ["example::proof"],
                "required_covers": ["true", "false"],
                "negative_controls": ["invert the predicate"],
                "result_satisfaction": "one current verified row",
                "currentness_inputs": ["source and configuration"],
                "transition_policy": "base_and_head",
                "owner_spec": "RIPR-SPEC-0371",
                "owner_issue": 14473
            })
            .to_string()
            .as_bytes(),
        )
    }

    fn write_registry(
        &self,
        claim: &ClaimDocument,
        registry_generation: u64,
    ) -> RegistryResult<PathBuf> {
        let document = format!("policy/formal-claims.d/{}.json", claim.claim_id);
        fs::write(
            self.0.join(&document),
            serde_json::to_vec_pretty(claim).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let manifest = serde_json::json!({
            "schema": "ripr.formal-claim-registry.v1",
            "repository": "example/repository",
            "registry_generation": registry_generation,
            "global_authority_paths": ["docs/formal-verification/CLAIMS.md"],
            "claims": [{
                "document": document,
                "claim_id": claim.claim_id.clone(),
                "claim_generation": claim.claim_generation,
                "route": "native",
                "authority_paths": ["docs/specs/RIPR-SPEC-0371-kani-bounded-model-checking-lane.md"],
                "expected_decision_digest": decision_digest(
                    claim,
                    &document,
                    ProviderRoute::Native,
                    &["docs/specs/RIPR-SPEC-0371-kani-bounded-model-checking-lane.md".to_string()],
                )?
            }],
            "retirements": []
        });
        let path = self.0.join("policy/formal-claims.json");
        fs::write(
            &path,
            serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        Ok(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn registry_emits_deterministic_index_and_honest_scoreboard() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let claim = fixture.claim("example.actionability", "Only ready is actionable.")?;
    let path = fixture.write_registry(&claim, 1)?;
    let loaded = load::load_registry(&path)?;
    let first = project::render_index(&loaded, OutputFormat::Json)?;
    let second = project::render_index(&load::load_registry(&path)?, OutputFormat::Json)?;
    assert_eq!(first, second);
    assert!(first.contains("\"execution_authority\": \"not_granted\""));
    let scoreboard: serde_json::Value =
        serde_json::from_str(&project::render_scoreboard(&loaded, OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
    assert_eq!(scoreboard["summary"]["registered_claims"], 1);
    assert_eq!(scoreboard["summary"]["not_established"], 1);
    assert_eq!(scoreboard["claims"][0]["status"], "NOT_ESTABLISHED");
    assert_eq!(scoreboard["native_assurance"], "not_established");
    Ok(())
}

#[test]
fn registry_membership_is_complete_and_exact() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let claim = fixture.claim("example.actionability", "Only ready is actionable.")?;
    let path = fixture.write_registry(&claim, 1)?;
    fs::write(
        fixture.0.join("policy/formal-claims.d/unlisted.json"),
        b"{}",
    )
    .map_err(|error| error.to_string())?;
    assert!(refusal(load::load_registry(&path))?.contains("registry membership differs"));
    fs::remove_file(fixture.0.join("policy/formal-claims.d/unlisted.json"))
        .map_err(|error| error.to_string())?;
    fs::remove_file(fixture.0.join("src/example.rs")).map_err(|error| error.to_string())?;
    assert!(refusal(load::load_registry(&path))?.contains("claim source closure"));
    fs::write(fixture.0.join("src/example.rs"), "synthetic fixture\n")
        .map_err(|error| error.to_string())?;
    fs::remove_file(
        fixture
            .0
            .join("policy/formal-claims.d/example.actionability.json"),
    )
    .map_err(|error| error.to_string())?;
    assert!(refusal(load::load_registry(&path))?.contains("registry membership differs"));
    Ok(())
}

#[test]
fn claim_document_stability_rejects_changed_bytes() -> RegistryResult<()> {
    let document = "policy/formal-claims.d/example.json";
    let before = br#"{"claim_generation":1}"#;
    let after = br#"{"claim_generation":2}"#;
    assert_eq!(
        refusal(load::require_stable_document(document, before, after))?,
        "claim document policy/formal-claims.d/example.json changed while semantic admission was in progress"
    );
    load::require_stable_document(document, before, before)?;
    Ok(())
}

#[test]
fn decision_digest_mismatch_and_duplicate_manifest_keys_reject() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let claim = fixture.claim("example.actionability", "Only ready is actionable.")?;
    let path = fixture.write_registry(&claim, 1)?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    value["claims"][0]["expected_decision_digest"] =
        serde_json::Value::String(format!("sha256:{}", "0".repeat(64)));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    assert!(refusal(load::load_registry(&path))?.contains("decision digest mismatch"));

    let duplicate = br#"{
      "schema":"ripr.formal-claim-registry.v1",
      "schema":"ripr.formal-claim-registry.v1",
      "repository":"example/repository",
      "registry_generation":1,
      "global_authority_paths":["docs/a.md"],
      "claims":[],
      "retirements":[]
    }"#;
    assert!(
        refusal(load::parse_unique::<Manifest>(duplicate))?.contains("duplicate JSON key: schema")
    );
    Ok(())
}

#[test]
fn decision_digest_covers_route_authority_and_document_locator() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let claim = fixture.claim("example.actionability", "Only ready is actionable.")?;
    let authority = ["docs/a.md".to_string()];
    let baseline = decision_digest(
        &claim,
        "policy/formal-claims.d/example.actionability.json",
        ProviderRoute::Native,
        &authority,
    )?;
    assert_ne!(
        baseline,
        decision_digest(
            &claim,
            "policy/formal-claims.d/example.actionability.json",
            ProviderRoute::Manual,
            &authority,
        )?
    );
    assert_ne!(
        baseline,
        decision_digest(
            &claim,
            "policy/formal-claims.d/example.actionability.json",
            ProviderRoute::Native,
            &["docs/b.md".to_string()],
        )?
    );
    assert_ne!(
        baseline,
        decision_digest(
            &claim,
            "policy/formal-claims.d/renamed.json",
            ProviderRoute::Native,
            &authority,
        )?
    );
    Ok(())
}

#[test]
fn transition_requires_generation_bumps_for_decision_changes() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base_claim = base_fixture.claim("example.actionability", "Old proposition.")?;
    let base_path = base_fixture.write_registry(&base_claim, 1)?;
    let base = load::load_registry(&base_path)?;

    let head_fixture = Fixture::new()?;
    let changed = head_fixture.claim("example.actionability", "New proposition.")?;
    let head_path = head_fixture.write_registry(&changed, 2)?;
    let head = load::load_registry(&head_path)?;
    assert!(
        refusal(transition::validate_transition(&base, &head))?
            .contains("changed decision must increase claim_generation")
    );

    let mut changed = changed;
    changed.claim_generation = 2;
    let head_path = head_fixture.write_registry(&changed, 2)?;
    let head = load::load_registry(&head_path)?;
    let report: serde_json::Value = serde_json::from_str(&transition::render_transition(
        &base,
        &head,
        OutputFormat::Json,
    )?)
    .map_err(|error| error.to_string())?;
    assert_eq!(
        report["changed_claims"],
        serde_json::json!(["example.actionability"])
    );
    Ok(())
}

#[test]
fn transition_requires_explicit_retirement_for_removal() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base_claim = base_fixture.claim("example.actionability", "Old proposition.")?;
    let base_path = base_fixture.write_registry(&base_claim, 1)?;
    let base = load::load_registry(&base_path)?;

    let head_fixture = Fixture::new()?;
    let replacement = head_fixture.claim("example.replacement", "Replacement proposition.")?;
    let head_path = head_fixture.write_registry(&replacement, 2)?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&head_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    manifest["retirements"] = serde_json::json!([{
        "claim_id": "example.actionability",
        "final_generation": 1,
        "replacement_claim_id": "example.replacement",
        "reason": "replaced by an explicitly registered proposition"
    }]);
    fs::write(
        &head_path,
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let head = load::load_registry(&head_path)?;
    let report: serde_json::Value = serde_json::from_str(&transition::render_transition(
        &base,
        &head,
        OutputFormat::Json,
    )?)
    .map_err(|error| error.to_string())?;
    assert_eq!(
        report["retired_claims"],
        serde_json::json!(["example.actionability"])
    );
    assert_eq!(
        report["added_claims"],
        serde_json::json!(["example.replacement"])
    );
    Ok(())
}

#[test]
fn transition_rejects_retirement_without_base_authority() -> RegistryResult<()> {
    let base_fixture = Fixture::new()?;
    let base_claim = base_fixture.claim("example.actionability", "Stable proposition.")?;
    let base_path = base_fixture.write_registry(&base_claim, 1)?;
    let base = load::load_registry(&base_path)?;

    let head_fixture = Fixture::new()?;
    let head_claim = head_fixture.claim("example.actionability", "Stable proposition.")?;
    let head_path = head_fixture.write_registry(&head_claim, 2)?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&head_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    manifest["retirements"] = serde_json::json!([{
        "claim_id": "example.ghost",
        "final_generation": 1,
        "replacement_claim_id": "example.actionability",
        "reason": "this claim never existed in the base registry"
    }]);
    fs::write(
        &head_path,
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let head = load::load_registry(&head_path)?;
    assert_eq!(
        refusal(transition::validate_transition(&base, &head))?,
        "retirement example.ghost does not correspond to an active base claim"
    );
    Ok(())
}

#[test]
fn order_does_not_change_registry_generation() -> RegistryResult<()> {
    let fixture = Fixture::new()?;
    let mut claim = fixture.claim("example.actionability", "Only ready is actionable.")?;
    claim.owners = vec!["z::owner".to_string(), "a::owner".to_string()];
    claim.source_closure = vec!["src/z.rs".to_string(), "src/a.rs".to_string()];
    let path = fixture.write_registry(&claim, 1)?;
    let first = load::load_registry(&path)?.generation;

    claim.owners.reverse();
    claim.source_closure.reverse();
    let path = fixture.write_registry(&claim, 1)?;
    let second = load::load_registry(&path)?.generation;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn initial_claims_include_their_production_domain_definitions() -> RegistryResult<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no repository parent")?;
    let registry = load::load_registry(&root.join("policy/formal-claims.json"))?;
    for (claim_id, domain) in [
        (
            "ripr.gate.top-level-status-fails-closed.v1",
            "crates/ripr/src/output/gate/model.rs",
        ),
        (
            "ripr.repair.route-requires-ready-current-instruction.v1",
            "crates/ripr/src/domain/fix_instruction.rs",
        ),
    ] {
        let claim = registry
            .claims
            .iter()
            .find(|claim| claim.claim.claim_id == claim_id)
            .ok_or_else(|| format!("missing initial claim {claim_id}"))?;
        assert!(
            claim.claim.source_closure.iter().any(|path| path == domain),
            "{claim_id} must track its production domain in {domain}"
        );
    }
    Ok(())
}
