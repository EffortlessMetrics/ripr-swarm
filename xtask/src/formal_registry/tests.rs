use super::*;
use formal::{Covers, ProcessState, Row, Status, Verdict};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn registered() -> AuditResult<Registry> {
    formal::parse_json(include_bytes!("../../../.ripr/formal-claims.json"))
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl Fixture {
    fn write(&self, name: &str, bytes: &[u8]) -> AuditResult<()> {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().ok_or("parent missing")?).map_err(|e| e.to_string())?;
        fs::write(path, bytes).map_err(|e| e.to_string())
    }
    fn registry(&self, registry: &Registry) -> AuditResult<PreparedRegistry> {
        self.write(REGISTRY_PATH, &json_bytes(registry)?)?;
        prepare(&self.0, Path::new(REGISTRY_PATH))
    }
    fn new() -> AuditResult<Self> {
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent)
            .map_err(|e| format!("create temporary parent {}: {e}", parent.display()))?;
        let path = parent.join(format!(
            "ripr-registry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)
            .map_err(|e| format!("create temporary fixture {}: {e}", path.display()))?;
        let fixture = Self(path);
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("repo missing")?;
        let registry = registered()?;
        let mut paths = BTreeSet::new();
        for entry in &registry.claims {
            paths.extend(entry.claim.source_closure.iter().cloned());
            paths.extend(entry.authority_paths.iter().cloned());
            paths.insert(entry.claim.owner_spec.clone());
        }
        for path in paths {
            fixture.write(
                &path,
                &fs::read(repo.join(&path))
                    .map_err(|e| format!("read fixture source {path}: {e}"))?,
            )?;
        }
        // Test-only closed source images. These metadata checks do not execute
        // or claim compilation of the fixture's placeholder Cargo/tool files.
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            "xtask/Cargo.toml",
            "xtask/src/formal_evidence.rs",
            "xtask/src/formal_registry.rs",
            "xtask/src/kani_lane.rs",
            "xtask/src/kani_evidence.rs",
            "xtask/src/formal_evidence/native.rs",
            "xtask/src/formal_registry/mod.rs",
            "xtask/src/kani_evidence/mod.rs",
            "crates/ripr/Cargo.toml",
            "crates/ripr-contract/Cargo.toml",
            "crates/ripr/src/process_owner.rs",
            "crates/ripr-contract/src/analysis/cancellation.rs",
            "tools/repo-policy/src/lib.rs",
        ] {
            fixture.write(path, b"test-only metadata fixture\n")?;
        }
        Ok(fixture)
    }
}

fn declared_success(prepared: &PreparedRegistry) -> AuditResult<Run> {
    let binding = prepared
        .claims
        .iter()
        .find(|p| p.claim.claim_id == "ripr.currentness.candidate-action")
        .ok_or("claim missing")?;
    let mut run: Run = formal::parse_json(include_bytes!(
        "../../../docs/formal-verification/examples/native-proof-run.example.json"
    ))?;
    run.claim_registry_generation = prepared.registry_generation.clone();
    run.provider.solver_version = Some("6.11.0".to_string());
    run.source_closure_digest = Some(binding.source_digest.clone());
    run.inventory.expected_claim_ids = vec![binding.claim.claim_id.clone()];
    run.inventory.discovered_subjects = binding.claim.subjects.clone();
    run.inventory.complete = true;
    run.inventory.rejection_reasons.clear();
    run.process.state = ProcessState::Completed;
    run.process.exit_code = Some(0);
    run.process.raw_output_digest = Some(native::sha256(
        b"synthetic supplied document; not execution",
    ));
    run.results = vec![Row {
        claim_id: binding.claim.claim_id.clone(),
        claim_generation: binding.claim.claim_generation,
        subject: binding.claim.subjects[0].clone(),
        source_digest: binding.source_digest.clone(),
        status: Status::Established,
        native_verdict: Verdict::Verified,
        covers: Covers {
            required: binding.required_cover_count,
            satisfied: binding.required_cover_count,
            unreachable: 0,
        },
        assumptions_digest: Some(binding.assumptions_digest.clone()),
        bounds_digest: binding.bounds_digest.clone(),
        duration_ms: Some(1),
        peak_memory_bytes: None,
        raw_block_digest: Some(native::sha256(b"synthetic block")),
        current: true,
        reasons: vec![
            "Synthetic consistent input for a refusal control; no native execution.".to_string(),
        ],
        non_claims: vec!["Not actual provider evidence.".to_string()],
    }];
    Ok(run)
}

#[test]
fn initial_registry_binds_six_claims_and_all_five_families_to_real_source() -> AuditResult<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("repo missing")?;
    let prepared = prepare(repo, Path::new(REGISTRY_PATH))?;
    assert_eq!(prepared.claims.len(), 6);
    assert_eq!(
        prepared
            .claims
            .iter()
            .map(|p| &p.semantic_family)
            .collect::<BTreeSet<_>>()
            .len(),
        5
    );
    let currentness = prepared
        .claims
        .iter()
        .find(|p| p.claim.claim_id == "ripr.currentness.candidate-action")
        .ok_or("currentness missing")?;
    assert_eq!(currentness.required_cover_count, 6);
    assert!(
        currentness
            .source_manifest
            .contains_key("crates/ripr-contract/src/domain/probe.rs")
    );
    assert!(
        currentness
            .authority_manifest
            .contains_key("crates/ripr-contract/src/domain/proof_domains.rs")
    );
    assert!(
        currentness
            .source_manifest
            .contains_key("crates/ripr/src/process_owner.rs")
    );
    assert!(
        currentness
            .source_manifest
            .contains_key("crates/ripr-contract/src/analysis/cancellation.rs")
    );
    Ok(())
}

#[test]
fn preparation_refuses_undeclared_unwind_and_transforming_harness_attributes() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let registry = registered()?;
    fixture.registry(&registry)?;
    let file = "crates/ripr-contract/src/domain/probe_kani.rs";
    let original = fs::read_to_string(fixture.0.join(file)).map_err(|e| e.to_string())?;
    let anchor = "#[kani::proof]\nfn currentness_action_and_base_side_are_disjoint";
    for attribute in [
        "#[kani::unwind(1)]",
        "#[kani::should_panic]",
        "#[kani::stub(SourceCurrentness::permits_candidate_action, replacement)]",
    ] {
        fixture.write(
            file,
            original
                .replacen(anchor, &format!("{attribute}\n{anchor}"), 1)
                .as_bytes(),
        )?;
        assert!(fixture.registry(&registry).is_err(), "{attribute}");
    }
    fixture.write(file, original.as_bytes())?;
    fixture.registry(&registry)?;
    Ok(())
}

#[test]
fn invalid_vocabulary_duplicate_ids_and_missing_critical_fields_are_refused() -> AuditResult<()> {
    let registry = registered()?;
    let mut duplicate = registry.clone();
    duplicate.claims.push(duplicate.claims[0].clone());
    assert!(validate_registry(&duplicate).is_err());
    for field in ["family", "criticality", "phase"] {
        let mut value = serde_json::to_value(&registry).map_err(|e| e.to_string())?;
        value["claims"][0]["claim"][field] = "unregistered-vocabulary".into();
        assert!(
            formal::parse_json::<Registry>(&json_bytes(&value)?).is_err(),
            "{field}"
        );
    }
    for field in [
        "proposition",
        "domain",
        "observable",
        "result_satisfaction",
        "transition_policy",
        "owner_spec",
    ] {
        let mut value = serde_json::to_value(&registry).map_err(|e| e.to_string())?;
        value["claims"][0]["claim"][field] = "".into();
        let invalid: Registry = formal::parse_json(&json_bytes(&value)?)?;
        assert!(validate_registry(&invalid).is_err(), "{field}");
    }
    for field in [
        "owners",
        "source_closure",
        "non_claims",
        "trust_dependencies",
        "subjects",
        "required_covers",
        "negative_controls",
        "currentness_inputs",
    ] {
        let mut value = serde_json::to_value(&registry).map_err(|e| e.to_string())?;
        value["claims"][0]["claim"][field] = serde_json::json!([]);
        let invalid: Registry = formal::parse_json(&json_bytes(&value)?)?;
        assert!(validate_registry(&invalid).is_err(), "{field}");
    }
    let mut weakened = registry.clone();
    weakened.claims[0].negative_controls[0].file = weakened.claims[0]
        .subject
        .as_ref()
        .ok_or("subject missing")?
        .file
        .clone();
    assert!(
        validate_registry(&weakened).is_err(),
        "a proof edit is not a production mutation"
    );
    let mut wrong_bound = registry.clone();
    wrong_bound.claims[0].claim.bounds.insert(
        "default_unwind".into(),
        formal::BoundValue::Number(1u64.into()),
    );
    assert!(
        validate_registry(&wrong_bound).is_err(),
        "declared default unwind must equal the profile"
    );
    let mut invalid = registry;
    invalid.claims[0].claim.provider = "shell".to_string();
    assert!(validate_registry(&invalid).is_err());
    invalid.claims[0].claim.provider = "kani".to_string();
    invalid.claims[0].claim.claim_generation = 0;
    assert!(validate_registry(&invalid).is_err());
    invalid.claims[0].claim.claim_generation = 1;
    invalid.claims[0].claim.claim_id = "not a stable id".to_string();
    assert!(validate_registry(&invalid).is_err());
    Ok(())
}

#[test]
fn passive_input_rejects_commands_unknown_keys_and_duplicate_keys() -> AuditResult<()> {
    let mut value = serde_json::to_value(registered()?).map_err(|e| e.to_string())?;
    value["claims"][0]["command"] = "touch never-executed".into();
    assert!(formal::parse_json::<Registry>(&json_bytes(&value)?).is_err());
    assert!(formal::parse_json::<Registry>(br#"{"schema":"x","schema":"y"}"#).is_err());
    Ok(())
}

#[test]
fn registry_ordering_does_not_change_scoreboard_or_selection_projection() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let mut registry = registered()?;
    let first = fixture.registry(&registry)?;
    registry.claims.reverse();
    for entry in &mut registry.claims {
        entry.claim.source_closure.reverse();
        entry.claim.subjects.reverse();
        entry.claim.required_covers.reverse();
        entry.claim.currentness_inputs.reverse();
        entry.authority_paths.reverse();
        entry.cover_requirements.reverse();
    }
    let second = fixture.registry(&registry)?;
    assert_eq!(first.registry_generation, second.registry_generation);
    assert_ne!(first.registry_digest, second.registry_digest);
    assert_eq!(
        json_bytes(&scoreboard(&first, None, None, &[])?)?,
        json_bytes(&scoreboard(&second, None, None, &[])?)?
    );
    assert_eq!(claim_index(&first)?, claim_index(&second)?);
    Ok(())
}

#[test]
fn missing_proofs_manual_obligations_and_changed_owners_stay_visible() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let prepared = fixture.registry(&registered()?)?;
    let board = scoreboard(&prepared, None, None, &["unmapped/critical.rs".to_string()])?;
    assert_eq!(board.registered_claims, 6);
    assert_eq!(board.by_status.get("NOT_ESTABLISHED"), Some(&5));
    assert_eq!(board.by_status.get("NOT_PROVEN"), Some(&1));
    assert_eq!(board.claims_without_executed_negative_control.len(), 6);
    assert_eq!(board.claims_without_required_cover_evidence.len(), 6);
    assert!(
        board
            .controls_without_native_assertion_mapping
            .iter()
            .all(|id| !id.starts_with("ripr.repair.readiness-producer-conformance"))
    );
    assert!(
        board
            .controls_without_native_assertion_mapping
            .contains(&"ripr.gate.command-error-exit.wrong-implementation".to_string())
    );
    assert_eq!(
        board.changed_critical_owners_without_current_applicable_claim,
        ["unmapped/critical.rs"]
    );
    assert_eq!(
        board.proof_authority_transition.state,
        transition::TransitionState::NotAssessed
    );
    let markdown = render_markdown(&board);
    for row in &board.claims {
        assert!(markdown.contains(&row.claim_id));
        assert!(markdown.contains(&row.source_digest));
    }
    assert!(markdown.contains("not_established"));
    let projection = claim_index(&prepared)?;
    assert_eq!(projection["generation"], prepared.registry_generation);
    assert!(
        projection["global_authority_paths"]
            .as_array()
            .ok_or("authority paths missing")?
            .contains(&serde_json::Value::String(
                "xtask/src/formal_registry/prepare.rs".to_string()
            ))
    );
    assert_eq!(
        projection["claims"]
            .as_array()
            .ok_or("index missing")?
            .len(),
        6
    );
    Ok(())
}

#[test]
fn unsupported_alternative_provider_remains_a_registered_obligation() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let mut registry = registered()?;
    let entry = registry
        .claims
        .iter_mut()
        .find(|e| e.claim.provider == "manual")
        .ok_or("manual missing")?;
    entry.claim.provider = "tlc".to_string();
    entry.claim.family = formal::Family::StateMachineModel;
    let prepared = fixture.registry(&registry)?;
    let board = scoreboard(&prepared, None, None, &[])?;
    assert_eq!(board.by_status.get("UNSUPPORTED"), Some(&1));
    assert!(
        board
            .claims
            .iter()
            .any(|r| r.status == Status::Unsupported && r.alternative_evidence_owner.is_some())
    );
    Ok(())
}

#[test]
fn consistent_success_document_never_authenticates_execution() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let prepared = fixture.registry(&registered()?)?;
    let run = declared_success(&prepared)?;
    let mut invalid = serde_json::to_value(&run).map_err(|e| e.to_string())?;
    invalid["results"][0]["status"] = "UNKNOWN_STATUS".into();
    assert!(formal::parse_json::<Run>(&json_bytes(&invalid)?).is_err());
    assert_eq!(
        formal::audit(
            &prepared
                .claims
                .iter()
                .map(|p| p.claim.clone())
                .collect::<Vec<_>>(),
            Some(&run)
        )?
        .declared_established_rows,
        1
    );
    let board = scoreboard(&prepared, None, Some((&run, None)), &[])?;
    assert_eq!(board.by_status.get("ESTABLISHED"), Some(&0));
    let row = board
        .claims
        .iter()
        .find(|r| r.claim_id == "ripr.currentness.candidate-action")
        .ok_or("row missing")?;
    assert_eq!(row.status, Status::NotEstablished);
    assert_eq!(
        row.last_result
            .as_ref()
            .ok_or("last result missing")?
            .declared_status,
        Status::Established
    );
    assert_eq!(board.producer_authentication, "not_established");
    Ok(())
}

#[test]
fn wrong_generation_source_provider_and_bounds_are_stale_or_refused() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let prepared = fixture.registry(&registered()?)?;
    for axis in [
        "registry",
        "claim",
        "source",
        "version",
        "bounds",
        "assumptions",
        "provider",
    ] {
        let mut run = declared_success(&prepared)?;
        match axis {
            "registry" => run.claim_registry_generation = native::sha256(b"other registry"),
            "claim" => {
                run.results[0].claim_generation += 1;
                run.results[0].status = Status::Stale;
                run.results[0].current = false;
            }
            "source" => run.results[0].source_digest = native::sha256(b"other source"),
            "version" => run.provider.version = "other-version".to_string(),
            "bounds" => run.results[0].bounds_digest = native::sha256(b"weaker bounds"),
            "assumptions" => {
                run.results[0].assumptions_digest = Some(native::sha256(b"narrow domain"))
            }
            _ => run.provider.name = "not-kani".to_string(),
        }
        match scoreboard(&prepared, None, Some((&run, None)), &[]) {
            Ok(board) => assert!(
                board
                    .claims
                    .iter()
                    .any(|r| r.claim_id == "ripr.currentness.candidate-action"
                        && r.status == Status::Stale),
                "{axis}"
            ),
            Err(error) => assert!(
                matches!(axis, "provider" | "assumptions"),
                "unexpected refusal for {axis}: {error}"
            ),
        }
    }
    Ok(())
}

#[test]
fn same_generation_metadata_weakening_and_proof_input_narrowing_are_refused() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let mut registry = registered()?;
    let original = fixture.registry(&registry)?;
    registry.generation += 1;
    registry.claims[0].claim.domain = "Only the already successful input".to_string();
    let weaker = fixture.registry(&registry)?;
    assert!(compare_predecessor(&weaker, Some(&original)).is_err());
    registry.claims[0].claim.claim_generation += 1;
    let changed = fixture.registry(&registry)?;
    assert_eq!(
        compare_predecessor(&changed, Some(&original))?.state,
        transition::TransitionState::ReviewRequired
    );

    let mut registry = registered()?;
    let original = fixture.registry(&registry)?;
    let harness = "crates/ripr-contract/src/domain/probe_kani.rs";
    let source = fs::read_to_string(fixture.0.join(harness)).map_err(|e| e.to_string())?;
    assert!(source.contains("kani::assume(index < CURRENTNESS.len());"));
    fixture.write(
        harness,
        source
            .replace(
                "kani::assume(index < CURRENTNESS.len());",
                "kani::assume(index < 1);",
            )
            .as_bytes(),
    )?;
    let narrowed = fixture.registry(&registry)?;
    assert!(compare_predecessor(&narrowed, Some(&original)).is_err());
    let row = registry
        .claims
        .iter_mut()
        .find(|r| r.claim.claim_id == "ripr.currentness.candidate-action")
        .ok_or("claim missing")?;
    row.claim.claim_generation += 1;
    registry.generation += 1;
    let changed = fixture.registry(&registry)?;
    assert_eq!(
        compare_predecessor(&changed, Some(&original))?.state,
        transition::TransitionState::ReviewRequired
    );
    Ok(())
}

#[test]
fn closure_omissions_missing_anchors_and_silent_retirement_are_refused() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let registry = registered()?;
    let original = fixture.registry(&registry)?;
    let mut captured = SourceManifest::from([(
        original.registry_path.clone(),
        original.registry_digest.clone(),
    )]);
    bind_captured_registry(&original, &captured)?;
    captured.insert(
        original.registry_path.clone(),
        native::sha256(b"other registry"),
    );
    assert!(bind_captured_registry(&original, &captured).is_err());
    captured.clear();
    assert!(bind_captured_registry(&original, &captured).is_err());
    let mut omitted = original.clone();
    let removed_path = omitted.claims[0].claim.source_closure[0].clone();
    omitted.claims[0].source_manifest.remove(&removed_path);
    omitted.claims[0].source_digest = native::sha256(&native::canonical_manifest_bytes(
        &omitted.claims[0].source_manifest,
    )?);
    assert!(validate_prepared(&omitted).is_err());
    let original_claim = &original.claims[0];
    let mut omitted_helper = original_claim.source_manifest.clone();
    assert!(
        omitted_helper
            .remove("tools/repo-policy/src/lib.rs")
            .is_some()
    );
    assert_ne!(
        omitted_helper,
        prepare::project_source(
            &original_claim.registration,
            &original_claim.source_manifest,
            &original_claim.explicit_inputs,
        )?
    );
    let mut removed = registry.clone();
    removed.claims.pop();
    removed.generation += 1;
    assert!(compare_predecessor(&fixture.registry(&removed)?, Some(&original)).is_err());
    let mut missing = registry;
    missing.claims[0].cover_requirements[0].anchor = "not a real source cover".to_string();
    assert!(fixture.registry(&missing).is_err());

    fixture.write(
        "xtask/src/registered_input.rs",
        b"const PROOF_DATA: &str = include_str!(\"../../proof-input.txt\");\n",
    )?;
    fixture.write("proof-input.txt", b"full admitted input fixture\n")?;
    let included = fixture.registry(&registered()?)?;
    let claim = &included.claims[0];
    assert!(
        claim
            .explicit_inputs
            .contains(&"proof-input.txt".to_string())
    );
    assert!(claim.source_manifest.contains_key("proof-input.txt"));
    let mut omitted = included.clone();
    omitted.claims[0].source_manifest.remove("proof-input.txt");
    omitted.claims[0].source_digest = native::sha256(&native::canonical_manifest_bytes(
        &omitted.claims[0].source_manifest,
    )?);
    assert!(validate_prepared(&omitted).is_err());
    fixture.write("proof-input.txt", b"narrowed input fixture\n")?;
    let changed = fixture.registry(&registered()?)?;
    assert_eq!(
        compare_predecessor(&changed, Some(&included))?.state,
        transition::TransitionState::ReviewRequired
    );
    Ok(())
}

#[test]
fn assumption_commitment_is_compact_utf8_json_including_empty_domain_assumptions() -> AuditResult<()>
{
    assert_eq!(formal::assumptions_digest(&[])?, native::sha256(b"[]"));
    assert_eq!(
        formal::assumptions_digest(&["alpha".to_string(), "β".to_string()])?,
        native::sha256("[\"alpha\",\"β\"]".as_bytes())
    );
    assert_ne!(
        formal::assumptions_digest(&["first".to_string(), "second".to_string()])?,
        formal::assumptions_digest(&["second".to_string(), "first".to_string()])?
    );
    Ok(())
}

#[test]
fn native_cover_mapping_refuses_equal_counts_from_different_properties() -> AuditResult<()> {
    let fixture = Fixture::new()?;
    let prepared = fixture.registry(&registered()?)?;
    let mut binding = prepared
        .claims
        .iter()
        .find(|claim| claim.claim.claim_id == "ripr.currentness.candidate-action")
        .ok_or("claim missing")?
        .clone();
    // Synthetic matching-law fixtures, not native format or execution evidence.
    let mut properties = Vec::new();
    for (index, cover) in binding
        .registration
        .cover_requirements
        .iter_mut()
        .enumerate()
    {
        let expected = NativeCoverExpectation {
            description: format!("\"synthetic {}\"", cover.id),
            file: cover.file.clone(),
            function: "synthetic::cover_function".to_string(),
            line: 1,
            column: 1,
        };
        properties.push(native::CoverProperty {
            name: format!("synthetic.cover.{}", index + 1),
            status: "SATISFIED".to_string(),
            description: Some(expected.description.clone()),
            location: Some(expected.location()),
        });
        cover.native = Some(expected);
    }
    let covers = match_native_covers(&binding, &properties)?;
    assert_eq!(
        (covers.required, covers.satisfied, covers.unreachable),
        (6, 6, 0)
    );
    for axis in ["description", "location", "name", "unknown-status"] {
        let mut wrong = properties.clone();
        match axis {
            "description" => {
                wrong[0].description = Some("a different satisfied condition".to_string())
            }
            "location" => {
                wrong[0].location =
                    Some("other/file.rs:1:1 in function synthetic::cover_function".to_string())
            }
            "name" => wrong[0].name = wrong[1].name.clone(),
            _ => wrong[0].status = "UNDETERMINED".to_string(),
        }
        assert!(match_native_covers(&binding, &wrong).is_err(), "{axis}");
    }
    let mut unreachable = properties.clone();
    unreachable[0].status = "UNSATISFIABLE".to_string();
    let covers = match_native_covers(&binding, &unreachable)?;
    assert_eq!((covers.satisfied, covers.unreachable), (5, 1));
    let mut extra = properties.clone();
    extra.push(properties[0].clone());
    assert!(match_native_covers(&binding, &extra).is_err());
    assert!(match_native_covers(&binding, &properties[..5]).is_err());
    binding.registration.cover_requirements[0].native = None;
    assert!(match_native_covers(&binding, &properties).is_err());
    binding.registration.cover_requirements[0].native =
        binding.registration.cover_requirements[1].native.clone();
    assert!(match_native_covers(&binding, &properties).is_err());
    Ok(())
}

#[test]
fn exact_relocation_needs_retained_equal_bytes_and_cannot_hide_an_assertion_edit() -> AuditResult<()>
{
    let fixture = Fixture::new()?;
    let mut registry = registered()?;
    registry
        .claims
        .iter_mut()
        .find(|e| e.claim.claim_id == "ripr.currentness.candidate-action")
        .ok_or("claim missing")?
        .claim
        .assumptions
        .push("Second ordered trust premise.".to_string());
    let original = fixture.registry(&registry)?;
    let id = "ripr.currentness.candidate-action";
    let old = original
        .claims
        .iter()
        .find(|p| p.claim.claim_id == id)
        .ok_or("old claim missing")?;
    let old_file = "crates/ripr-contract/src/domain/probe_kani.rs";
    let new_file = "crates/ripr-contract/src/domain/probe_kani_moved.rs";
    let old_subject = &old.claim.subjects[0];
    let new_subject = old_subject.replace(old_file, new_file);
    fs::rename(fixture.0.join(old_file), fixture.0.join(new_file)).map_err(|e| e.to_string())?;
    let caller = "crates/ripr-contract/src/domain/probe.rs";
    let caller_source = fs::read_to_string(fixture.0.join(caller)).map_err(|e| e.to_string())?;
    fixture.write(
        caller,
        caller_source
            .replace(
                "#[path = \"probe_kani.rs\"]",
                "#[path = \"probe_kani_moved.rs\"]",
            )
            .as_bytes(),
    )?;
    let entry = registry
        .claims
        .iter_mut()
        .find(|e| e.claim.claim_id == id)
        .ok_or("claim missing")?;
    let bytes = json_bytes(entry)?;
    let text = String::from_utf8(bytes)
        .map_err(|e| e.to_string())?
        .replace(old_file, new_file);
    *entry = formal::parse_json(text.as_bytes())?;
    registry.generation += 1;
    registry.compatible_relocations.push(CompatibleRelocation {
        claim_id: id.to_string(),
        from_file: old_file.to_string(),
        to_file: new_file.to_string(),
        from_subject: old_subject.clone(),
        to_subject: new_subject,
        source_digest: old
            .authority_manifest
            .get(old_file)
            .ok_or("authority missing")?
            .clone(),
    });
    let moved = fixture.registry(&registry)?;
    assert!(
        compare_predecessor(&moved, Some(&original))?
            .claims
            .iter()
            .any(|r| r.claim_id == id && r.state == "EXACT_RELOCATION")
    );
    let mut reordered = registry.clone();
    reordered
        .claims
        .iter_mut()
        .find(|e| e.claim.claim_id == id)
        .ok_or("claim missing")?
        .claim
        .assumptions
        .reverse();
    assert!(compare_predecessor(&fixture.registry(&reordered)?, Some(&original)).is_err());
    let text = fs::read_to_string(fixture.0.join(new_file)).map_err(|e| e.to_string())?;
    fixture.write(
        new_file,
        text.replace("assert_eq!(", "assert_ne!(").as_bytes(),
    )?;
    assert!(compare_predecessor(&fixture.registry(&registry)?, Some(&original)).is_err());
    Ok(())
}
