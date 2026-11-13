//! Passive admission of one declared production mutation between two native
//! positives. All verifier parsing and retained-byte replay stay with `native`.
use super::*;
use formal::{Cleanup, ProcessState, Status, Verdict, require};
use std::path::{Component, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedRun {
    /// Both locators are relative to the pair manifest's directory.
    pub run: native::Artifact,
    pub artifacts: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeControlPair {
    pub schema: String,
    pub claim_id: String,
    pub claim_generation: u64,
    pub registry_generation: String,
    pub control_id: String,
    pub positive: RetainedRun,
    pub mutant: RetainedRun,
    pub restored: RetainedRun,
    pub original_source: native::Artifact,
    pub mutant_source: native::Artifact,
}

#[derive(Debug, Serialize)]
pub struct ControlObservation {
    pub claim_id: String,
    pub claim_generation: u64,
    pub control_id: String,
    pub state: &'static str,
    pub provenance: &'static str,
    pub pair_digest: String,
    pub positive_run_id: String,
    pub mutant_run_id: String,
    pub restored_run_id: String,
    pub production_file: String,
    pub original_digest: String,
    pub mutant_digest: String,
    pub failed_assertions: Vec<String>,
    pub satisfied_covers: u64,
    pub non_claims: Vec<&'static str>,
}

struct CheckedRun {
    run: Run,
    directory: PathBuf,
    sources: SourceManifest,
}

fn contained_directory(root: &Path, relative: &str) -> AuditResult<PathBuf> {
    native::relative_path(relative)?;
    let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let mut path = root.clone();
    for component in Path::new(relative).components() {
        require(
            matches!(component, Component::Normal(_)),
            "pair artifact directory must be a normal relative path",
        )?;
        path.push(component);
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        require(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "pair artifact directory contains a symlink or non-directory",
        )?;
    }
    require(
        std::fs::canonicalize(&path)
            .map_err(|e| e.to_string())?
            .starts_with(&root),
        "pair artifact directory escaped its parent",
    )?;
    Ok(path)
}

fn load_run(root: &Path, reference: &RetainedRun) -> AuditResult<CheckedRun> {
    let bytes = native::verify_artifact(root, &reference.run, 32 * 1_048_576)?;
    let run: Run = formal::parse_json(&bytes)?;
    let directory = contained_directory(root, &reference.artifacts)?;
    let sources = native::verify_execution_artifacts(&run, &directory)?;
    Ok(CheckedRun {
        run,
        directory,
        sources,
    })
}

fn execution(run: &CheckedRun) -> AuditResult<&native::Execution> {
    run.run
        .execution
        .as_ref()
        .ok_or_else(|| "pair run has no native execution".into())
}

fn observed<'a>(
    run: &'a CheckedRun,
    binding: &PreparedClaim,
) -> AuditResult<&'a native::UnregisteredResult> {
    let subject = binding
        .subject
        .as_ref()
        .ok_or("paired claim has no native subject")?;
    let execution = execution(run)?;
    require(
        execution.source_current
            && execution.configuration_admitted
            && run.run.inventory.complete
            && execution.selected_subjects == [subject.clone()]
            && run.run.unregistered_results.len() == 1
            && run.run.unregistered_results[0].subject == *subject
            && run.run.process.cleanup_state == Cleanup::Complete
            && !run.run.process.timed_out,
        "paired run lacks one complete current subject, admitted configuration or clean process custody",
    )?;
    let result = &run.run.unregistered_results[0];
    let covers = match_native_covers(binding, &result.cover_properties)?;
    require(
        covers.satisfied == covers.required && covers.unreachable == 0,
        "paired run lacks the same complete reachable domain and semantic covers",
    )?;
    Ok(result)
}

fn positive(
    current: &PreparedRegistry,
    binding: &PreparedClaim,
    checked: &CheckedRun,
) -> AuditResult<()> {
    let run = &checked.run;
    let execution = execution(checked)?;
    let artifact = execution
        .registration
        .as_ref()
        .ok_or("positive lacks retained registration")?;
    let prepared: PreparedRegistry = formal::parse_json(&native::verify_artifact(
        &checked.directory,
        artifact,
        32 * 1_048_576,
    )?)?;
    validate_binding(&prepared, run, &checked.sources)?;
    let captured = prepared
        .claims
        .iter()
        .find(|p| p.claim.claim_id == binding.claim.claim_id)
        .ok_or("paired claim absent from retained registration")?;
    require(
        prepared.registry_generation == current.registry_generation
            && prepared.registry_digest == current.registry_digest
            && json_bytes(captured)? == json_bytes(binding)?,
        "paired positive belongs to a stale registry, source, generation, bounds or assumptions",
    )?;
    let observation = observed(checked, binding)?;
    require(
        run.inventory.expected_claim_ids == [binding.claim.claim_id.clone()]
            && run.results.len() == 1
            && run.results[0].status == Status::Established
            && run.results[0].native_verdict == Verdict::Verified
            && run.results[0].current
            && observation.status == native::ObservationStatus::Verified
            && observation.native_verdict == Verdict::Verified
            && run.process.state == ProcessState::Completed
            && run.process.exit_code == Some(0)
            && execution.lane_success,
        "paired positive is not one established registered native proof",
    )
}

/// The only tolerated source difference is exactly one occurrence of the
/// registered production mutation. No harness, domain, config or helper edit.
fn exact_mutation(
    control: &NegativeControl,
    original: &[u8],
    mutated: &[u8],
    before: &SourceManifest,
    after: &SourceManifest,
) -> AuditResult<()> {
    let source = std::str::from_utf8(original).map_err(|e| e.to_string())?;
    require(
        source.matches(&control.before).count() == 1
            && source
                .replacen(&control.before, &control.after, 1)
                .as_bytes()
                == mutated,
        "retained production bytes are not exactly the one declared mutation",
    )?;
    let original_digest = native::sha256(original);
    let mutant_digest = native::sha256(mutated);
    require(
        original_digest != mutant_digest
            && before.get(&control.file) == Some(&original_digest)
            && after.get(&control.file) == Some(&mutant_digest),
        "retained production bytes differ from the native source commitments",
    )?;
    let mut expected = before.clone();
    expected.insert(control.file.clone(), mutant_digest);
    require(
        expected == *after,
        "mutant changed, omitted or added an input beyond the one declared production file",
    )
}

fn configuration(checked: &CheckedRun) -> AuditResult<native::ConfigurationSnapshot> {
    formal::parse_json(&native::verify_artifact(
        &checked.directory,
        &execution(checked)?.configuration_before,
        8 * 1_048_576,
    )?)
}

fn same_configuration(
    control: &NegativeControl,
    before: &native::ConfigurationSnapshot,
    after: &native::ConfigurationSnapshot,
    original_digest: &str,
    mutant_digest: &str,
) -> AuditResult<()> {
    let mut left = before.inputs.clone();
    let mut right = after.inputs.clone();
    require(
        left.remove(&control.file).as_deref() == Some(original_digest)
            && right.remove(&control.file).as_deref() == Some(mutant_digest)
            && left == right
            && before.cargo_config_keys == after.cargo_config_keys,
        "paired configuration differs beyond the governed mutant source input",
    )
}

fn same_profile(original: &CheckedRun, other: &CheckedRun) -> AuditResult<()> {
    let left = execution(original)?;
    let right = execution(other)?;
    let tool = |e: &native::Execution| {
        e.installed_toolchain
            .as_ref()
            .map(|a| (a.sha256.clone(), a.bytes))
    };
    require(
        tool(left).is_some() && tool(left) == tool(right),
        "paired provider/compiler/solver executable commitments differ or are absent",
    )?;
    // Working directories and output locations belong to individual runs.
    // Exact effective native argv and declared resource policy are compared
    // separately from the producer's registered/unregistered selection args.
    // Each measured timeout is the remaining whole-run budget, so elapsed
    // preparation legitimately changes it. Shared native replay bounds it by
    // the declared policy; equal remaining milliseconds are not required.
    let profile = |checked: &CheckedRun| -> AuditResult<serde_json::Value> {
        let e = execution(checked)?;
        Ok(serde_json::json!({
            "provider": checked.run.provider, "toolchain": checked.run.toolchain,
            "runner_generation": checked.run.runner.generation,
            "deadlines": checked.run.runner.deadline_policy,
            "repository": checked.run.repository,
            "revision": checked.run.revision, "tree": checked.run.tree,
            "repository_directory": e.repository_directory,
            "producer": e.producer, "entrypoint": e.producer_entrypoint,
            "configuration_policy": e.configuration_policy,
            "default_unwind": e.default_unwind,
            "packages": e.selected_packages, "filter": e.harness_filter,
            "subjects": e.selected_subjects, "declared": e.declared_subjects,
            "native": e.native_subjects,
            "explicit_inputs": e.explicit_inputs, "package_inputs": e.package_inputs,
            "commands": e.invocations.iter().filter(|i| matches!(i.phase.as_str(), "preflight" | "metadata" | "inventory" | "verification"))
                .map(|i| serde_json::json!({"phase":i.phase,"package":i.package,"program":i.program,
                    "arguments":i.arguments,"stream_limit_bytes":i.stream_limit_bytes}))
                .collect::<Vec<_>>()
        }))
    };
    require(
        profile(original)? == profile(other)?,
        "paired effective native profile, subject, bounds or source revision differs",
    )
}

fn assertions(
    control: &NegativeControl,
    properties: &[native::NativeProperty],
    failed: bool,
) -> AuditResult<()> {
    require(
        !control.expected_failures.is_empty(),
        "control has no qualified native assertion mapping",
    )?;
    let expected_names: BTreeSet<_> = control
        .expected_failures
        .iter()
        .map(|e| e.property.as_str())
        .collect();
    let ordinary_assertion_prefix = format!(
        "{}.assertion.",
        control
            .expected_failures
            .first()
            .ok_or("failure mapping absent")?
            .presentation
            .function
    );
    let mut names = BTreeSet::new();
    let mut failures = BTreeSet::new();
    for property in properties {
        require(
            names.insert(property.name.as_str()),
            "paired native property identity is duplicated",
        )?;
        // Kani prunes a failing assertion's continuation. A later, unmapped
        // ordinary assertion can therefore be unreachable in the mutant even
        // though the required input-domain covers remain fully satisfied.
        let pruned_assertion = failed
            && property.status == "UNREACHABLE"
            && property
                .name
                .strip_prefix(&ordinary_assertion_prefix)
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                })
            && property
                .description
                .as_deref()
                .is_some_and(|description| description.starts_with("\"assertion failed: "))
            && !expected_names.contains(property.name.as_str());
        require(
            matches!(
                property.status.as_str(),
                "SUCCESS" | "FAILURE" | "SATISFIED"
            ) || pruned_assertion,
            "paired native property has an indeterminate or unreachable status",
        )?;
        if property.status == "FAILURE" {
            failures.insert(property.name.as_str());
        }
    }
    require(
        if failed {
            failures == expected_names
        } else {
            failures.is_empty()
        },
        "native failures are missing, unrelated or exceed the registered intended assertions",
    )?;
    for expected in &control.expected_failures {
        let property = properties
            .iter()
            .find(|p| p.name == expected.property)
            .ok_or("registered intended assertion absent from native property inventory")?;
        require(
            property.description.as_ref() == Some(&expected.presentation.description)
                && property.location.as_ref() == Some(&expected.presentation.location())
                && property.status == if failed { "FAILURE" } else { "SUCCESS" },
            "native intended assertion identity, source location or verdict differs",
        )?;
    }
    Ok(())
}

/// Replay three retained runs and compare their exact source/profile/behavior.
/// This grants no execution authority and cannot authenticate local producers.
pub fn validate_control_pair(
    current: &PreparedRegistry,
    supplied_positive: &Run,
    pair_path: &Path,
) -> AuditResult<ControlObservation> {
    validate_prepared(current)?;
    let bytes = read_input(pair_path)?;
    let pair: NativeControlPair = formal::parse_json(&bytes)?;
    require(
        pair.schema == "ripr.native-control-pair.v1",
        "unsupported native control pair schema",
    )?;
    let binding = current
        .claims
        .iter()
        .find(|p| p.claim.claim_id == pair.claim_id)
        .ok_or("paired claim is not registered")?;
    require(
        pair.registry_generation == current.registry_generation
            && pair.claim_generation == binding.claim.claim_generation,
        "stale paired registry or claim generation",
    )?;
    let control = binding
        .registration
        .negative_controls
        .iter()
        .find(|c| c.id == pair.control_id)
        .ok_or("paired mutation is not registered")?;
    let root = pair_path
        .parent()
        .ok_or("control pair has no parent directory")?;
    let original = load_run(root, &pair.positive)?;
    let mutant = load_run(root, &pair.mutant)?;
    let restored = load_run(root, &pair.restored)?;
    require(
        json_bytes(&original.run)? == json_bytes(supplied_positive)?,
        "control pair belongs to a different supplied positive run",
    )?;
    require(
        [
            &original.run.run_id,
            &mutant.run.run_id,
            &restored.run.run_id,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
        .len()
            == 3,
        "positive, mutant and restored native records must be distinct",
    )?;
    positive(current, binding, &original)?;
    positive(current, binding, &restored)?;
    require(
        original.sources == restored.sources,
        "restored proof did not restore the complete original source closure",
    )?;
    let original_bytes = native::verify_artifact(root, &pair.original_source, 8 * 1_048_576)?;
    let mutant_bytes = native::verify_artifact(root, &pair.mutant_source, 8 * 1_048_576)?;
    exact_mutation(
        control,
        &original_bytes,
        &mutant_bytes,
        &original.sources,
        &mutant.sources,
    )?;
    same_profile(&original, &mutant)?;
    same_profile(&original, &restored)?;
    let original_config = configuration(&original)?;
    let mutant_config = configuration(&mutant)?;
    let restored_config = configuration(&restored)?;
    same_configuration(
        control,
        &original_config,
        &mutant_config,
        &pair.original_source.sha256,
        &pair.mutant_source.sha256,
    )?;
    require(
        json_bytes(&original_config)? == json_bytes(&restored_config)?,
        "restored configuration differs from original",
    )?;
    let result = observed(&mutant, binding)?;
    let mutant_execution = execution(&mutant)?;
    require(
        mutant.run.results.is_empty()
            && mutant.run.inventory.expected_claim_ids.is_empty()
            && mutant_execution.registration.is_none()
            && !mutant_execution.lane_success
            && result.status == native::ObservationStatus::Failed
            && result.native_verdict == Verdict::Counterexample
            && matches!(
                mutant.run.process.state,
                ProcessState::Completed | ProcessState::Failed
            )
            && mutant.run.process.exit_code.is_some_and(|code| code != 0)
            && mutant_execution
                .invocations
                .iter()
                .filter(|i| i.phase == "verification")
                .count()
                == 1
            && mutant_execution
                .invocations
                .iter()
                .filter(|i| i.phase == "verification")
                .all(|i| {
                    i.capture_complete
                        && !i.timed_out
                        && i.error.is_none()
                        && i.exit_code.is_some_and(|code| code != 0)
                }),
        "negative control lacks an unregistered native assertion counterexample; abnormal/setup failures do not discriminate",
    )?;
    assertions(control, &result.properties, true)?;
    assertions(control, &observed(&original, binding)?.properties, false)?;
    assertions(control, &observed(&restored, binding)?.properties, false)?;
    Ok(ControlObservation {
        claim_id: pair.claim_id,
        claim_generation: pair.claim_generation,
        control_id: pair.control_id,
        state: "local_native_discrimination_observed",
        provenance: "local_retained_bytes_checked_not_authenticated",
        pair_digest: native::sha256(&bytes),
        positive_run_id: original.run.run_id,
        mutant_run_id: mutant.run.run_id,
        restored_run_id: restored.run.run_id,
        production_file: control.file.clone(),
        original_digest: pair.original_source.sha256,
        mutant_digest: pair.mutant_source.sha256,
        failed_assertions: control
            .expected_failures
            .iter()
            .map(|e| e.property.clone())
            .collect(),
        satisfied_covers: binding.required_cover_count,
        non_claims: vec![
            "Local retained-byte replay does not authenticate execution, timing, authorship or independent custody.",
            "One exact production mutation does not measure mutation coverage or establish specification completeness.",
            "The mutant remains unregistered and cannot replace the original positive source identity.",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control() -> NegativeControl {
        NegativeControl {
            id: "fixture.currentness.wrong".into(),
            file: "src/production.rs".into(),
            before: "matches!(self, Self::CandidateCurrent)".into(),
            after: "!matches!(self, Self::BaseDeleted)".into(),
            expected_failed_claim: "fixture.currentness".into(),
            expected_subject: "fixture:src/proof.rs::proof::currentness".into(),
            effect: "Synthetic metadata controls; no verifier execution is claimed.".into(),
            expected_failures: vec![NativeAssertionExpectation {
                property: "proof::currentness.assertion.1".into(),
                presentation: NativeCoverExpectation {
                    description: "\"assertion failed: currentness is exact\"".into(),
                    file: "src/proof.rs".into(),
                    function: "proof::currentness".into(),
                    line: 10,
                    column: 5,
                },
            }],
        }
    }

    #[test]
    fn control_pair_rejects_extra_mutations_missing_inputs_and_wrong_source_bytes()
    -> AuditResult<()> {
        let control = control();
        let original = b"fn permits(self) { matches!(self, Self::CandidateCurrent) }\n";
        let mutated = b"fn permits(self) { !matches!(self, Self::BaseDeleted) }\n";
        let before = BTreeMap::from([
            (control.file.clone(), native::sha256(original)),
            (
                "src/proof.rs".into(),
                native::sha256(b"unchanged domain and assertion"),
            ),
        ]);
        let mut after = before.clone();
        after.insert(control.file.clone(), native::sha256(mutated));
        exact_mutation(&control, original, mutated, &before, &after)?;
        let extra = b"fn permits(self) { !matches!(self, Self::BaseDeleted) }\n// second edit\n";
        assert!(exact_mutation(&control, original, extra, &before, &after).is_err());
        for poisoned in [
            BTreeMap::from([(control.file.clone(), native::sha256(mutated))]),
            {
                let mut value = after.clone();
                value.insert("src/proof.rs".into(), native::sha256(b"weaker domain"));
                value
            },
            {
                let mut value = after.clone();
                value.insert("src/new.rs".into(), native::sha256(b"injected helper"));
                value
            },
            {
                let mut value = after.clone();
                value.insert(
                    control.file.clone(),
                    native::sha256(b"different production bytes"),
                );
                value
            },
        ] {
            assert!(exact_mutation(&control, original, mutated, &before, &poisoned).is_err());
        }
        let duplicate = [original.as_slice(), original.as_slice()].concat();
        assert!(exact_mutation(&control, &duplicate, mutated, &before, &after).is_err());
        Ok(())
    }

    #[test]
    fn control_pair_configuration_ignores_only_the_one_governed_source_difference()
    -> AuditResult<()> {
        let control = control();
        let original = native::sha256(b"original");
        let mutated = native::sha256(b"mutated");
        let before = native::ConfigurationSnapshot {
            inputs: BTreeMap::from([
                (control.file.clone(), original.clone()),
                ("locator:PATH".into(), native::sha256(b"same tools")),
            ]),
            cargo_config_keys: BTreeMap::new(),
        };
        let mut after = before.clone();
        after.inputs.insert(control.file.clone(), mutated.clone());
        same_configuration(&control, &before, &after, &original, &mutated)?;
        after
            .inputs
            .insert("locator:PATH".into(), native::sha256(b"substituted tools"));
        assert!(same_configuration(&control, &before, &after, &original, &mutated).is_err());
        after.inputs = before.inputs.clone();
        assert!(same_configuration(&control, &before, &after, &original, &mutated).is_err());
        after.inputs.insert(control.file.clone(), mutated.clone());
        after.cargo_config_keys.insert(
            "cargo-config:/fixture/config".into(),
            vec![vec!["alias".into(), "kani".into()]],
        );
        assert!(same_configuration(&control, &before, &after, &original, &mutated).is_err());
        Ok(())
    }

    #[test]
    fn control_pair_requires_exact_native_failure_identity_and_no_unrelated_failure()
    -> AuditResult<()> {
        let control = control();
        let expected = &control.expected_failures[0];
        let mut properties = vec![native::NativeProperty {
            name: expected.property.clone(),
            status: "SUCCESS".into(),
            description: Some(expected.presentation.description.clone()),
            location: Some(expected.presentation.location()),
        }];
        assertions(&control, &properties, false)?;
        assert!(assertions(&control, &properties, true).is_err());
        properties[0].status = "FAILURE".into();
        assertions(&control, &properties, true)?;
        assert!(assertions(&control, &properties, false).is_err());
        let failure = properties[0].clone();
        let mut pruned = native::NativeProperty {
            name: "proof::currentness.assertion.2".into(),
            status: "UNREACHABLE".into(),
            description: Some(
                "\"assertion failed: unmapped assertion after the intended failure\"".into(),
            ),
            location: None,
        };
        assertions(&control, &[failure.clone(), pruned.clone()], true)?;
        assert!(assertions(&control, &[failure.clone(), pruned.clone()], false).is_err());
        for name in [
            "proof::currentness.cover.1",
            "proof::currentness.pointer_dereference.1",
            "proof::helper.assertion.1",
        ] {
            pruned.name = name.into();
            assert!(assertions(&control, &[failure.clone(), pruned.clone()], true).is_err());
        }
        pruned.name = "proof::currentness.assertion.2".into();
        pruned.description = Some("\"index out of bounds\"".into());
        assert!(assertions(&control, &[failure.clone(), pruned], true).is_err());
        for field in ["name", "description", "location", "status", "unreachable"] {
            let mut poisoned = failure.clone();
            match field {
                "name" => poisoned.name = "proof::other.assertion.1".into(),
                "description" => poisoned.description = Some("unrelated assertion".into()),
                "location" => {
                    poisoned.location =
                        Some("src/substitute.rs:10:5 in function proof::currentness".into())
                }
                "unreachable" => poisoned.status = "UNREACHABLE".into(),
                _ => poisoned.status = "UNDETERMINED".into(),
            }
            assert!(assertions(&control, &[poisoned], true).is_err(), "{field}");
        }
        properties.push(native::NativeProperty {
            name: "proof::currentness.pointer_dereference.1".into(),
            status: "FAILURE".into(),
            description: Some("unrelated memory safety failure".into()),
            location: None,
        });
        assert!(assertions(&control, &properties, true).is_err());
        assert!(assertions(&control, &[failure.clone(), failure], true).is_err());
        assert!(assertions(&control, &[], true).is_err());
        let mut unmapped = control;
        unmapped.expected_failures.clear();
        assert!(assertions(&unmapped, &properties, true).is_err());
        Ok(())
    }

    #[test]
    fn registered_failure_mapping_matches_the_measured_native_format() -> AuditResult<()> {
        // A retained format sample tests interpretation and registration only;
        // it is not an admitted execution on this source generation.
        let registry: Registry =
            formal::parse_json(include_bytes!("../../../.ripr/formal-claims.json"))?;
        let registration = registry
            .claims
            .iter()
            .find(|entry| entry.claim.claim_id == "ripr.currentness.candidate-action")
            .ok_or("currentness registration absent")?;
        let control = registration
            .negative_controls
            .first()
            .ok_or("control absent")?;
        let output =
            include_str!("../../tests/fixtures/kani-native-output/currentness-mutant.stdout");
        let mut properties = formal::kani_output::native_properties(output)?;
        assertions(control, &properties, true)?;
        let failure = properties
            .iter_mut()
            .find(|property| property.status == "FAILURE")
            .ok_or("measured failure absent")?;
        failure.description = failure
            .description
            .as_ref()
            .and_then(|description| description.lines().next())
            .map(str::to_owned);
        assert!(
            assertions(control, &properties, true).is_err(),
            "a truncated multiline description cannot match the registered assertion"
        );
        Ok(())
    }
}
