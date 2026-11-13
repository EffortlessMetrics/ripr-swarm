use super::*;
use crate::formal_registry::native_bindings::tests::Fixture;
use serde_json::json;
use std::path::PathBuf;

struct Supplied {
    fixture: Fixture,
    prepared: PreparedNativeInputs,
    run: formal::Run,
    directory: PathBuf,
}

fn artifact(directory: &Path, path: &str, bytes: &[u8]) -> RegistryResult<native::Artifact> {
    let file = directory.join(path);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&file, bytes).map_err(|error| error.to_string())?;
    Ok(native::Artifact {
        path: path.to_string(),
        sha256: native::sha256(bytes),
        bytes: bytes.len() as u64,
    })
}

fn json_artifact<T: Serialize>(
    directory: &Path,
    path: &str,
    value: &T,
) -> RegistryResult<native::Artifact> {
    artifact(
        directory,
        path,
        &serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
}

fn invocation(
    directory: &Path,
    root: &Path,
    phase: &str,
    number: usize,
    arguments: Vec<String>,
    stdout: &[u8],
) -> RegistryResult<native::Invocation> {
    let is_git = matches!(phase, "before" | "after");
    let cwd = if phase == "inventory" {
        root.join("target/ripr/reports/kani-evidence/replay/list-ripr-contract")
    } else {
        root.to_path_buf()
    };
    Ok(native::Invocation {
        phase: phase.to_string(),
        package: matches!(phase, "inventory" | "verification").then(|| "ripr-contract".to_string()),
        program: if is_git { "git" } else { "cargo" }.to_string(),
        arguments,
        working_directory: cwd.display().to_string(),
        timeout_ms: 1000,
        stream_limit_bytes: 1_048_576,
        duration_ms: Some(1),
        exit_code: Some(0),
        native_status: Some("exit status: 0".to_string()),
        timed_out: false,
        capture_complete: true,
        stdout: Some(artifact(
            directory,
            &format!("raw/{number}-stdout"),
            stdout,
        )?),
        stderr: Some(artifact(directory, &format!("raw/{number}-stderr"), b"")?),
        error: None,
    })
}

impl Supplied {
    fn new() -> RegistryResult<Self> {
        let fixture = Fixture::new()?;
        let registry = fixture.view()?;
        let mut bindings = fixture.bindings(&registry)?;
        let profile = native::DeclaredKaniProfile::rust_only();
        bindings.bindings[0].profile = profile.clone();
        let prepared = registry.prepare_native(Some(&bindings))?;
        let directory = fixture.0.join("supplied-evidence");
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let subject = prepared.claims[0].subjects[0].native.clone();
        let sources = &prepared.claims[0].source_manifest;
        let source_artifact = json_artifact(&directory, "source.json", sources)?;
        let registration = artifact(
            &directory,
            "preparation.json",
            prepared.to_json()?.as_bytes(),
        )?;
        let config = json_artifact(
            &directory,
            "configuration.json",
            &native::ConfigurationSnapshot {
                inputs: sources.clone(),
                cargo_config_keys: BTreeMap::new(),
            },
        )?;
        let installed = json_artifact(
            &directory,
            "installed.json",
            &native::InstalledToolchain {
                installation: "/nonexistent-forged-kani".to_string(),
                rustc: "rustc 1.100.0-nightly (8925ea358 2026-08-20)".to_string(),
                rustup_toolchain: "nightly-2026-08-21-x86_64-unknown-linux-gnu".to_string(),
                target: profile.target.clone(),
                binaries: ["kani-compiler", "kani-driver", "cbmc"]
                    .into_iter()
                    .map(|name| (name.to_string(), native::sha256(name.as_bytes())))
                    .collect(),
            },
        )?;
        let inventory_bytes = serde_json::to_vec_pretty(&json!({
            "kani-version":"0.68.0","file-version":"0.1", "standard-harnesses":{subject.file.clone():[subject.qualified_name.clone()]},
            "contract-harnesses":{},"contracts":[],"totals":{"standard-harnesses":1,"contract-harnesses":0,"functions-under-contract":0}
        })).map_err(|error| error.to_string())?;
        let inventory = artifact(&directory, "list.json", &inventory_bytes)?;
        let metadata = json!({"version":1,"workspace_root":fixture.0,"workspace_metadata":{},
            "packages":[{"id":"selected","name":"ripr-contract","source":null,"manifest_path":fixture.0.join("crates/ripr-contract/Cargo.toml"),"metadata":{},"targets":[{"kind":["lib"],"src_path":fixture.0.join("crates/ripr-contract/src/lib.rs")}]}],
            "workspace_members":["selected"],"resolve":{"root":"selected","nodes":[{"id":"selected","features":["lang-rust"],"deps":[]}]}});
        let metadata_bytes =
            serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?;
        let mut invocations = Vec::new();
        for phase in ["before", "after"] {
            for (arguments, stdout) in [
                (vec!["rev-parse", "HEAD"], format!("{}\n", "1".repeat(40))),
                (
                    vec!["rev-parse", "HEAD^{tree}"],
                    format!("{}\n", "2".repeat(40)),
                ),
                (
                    vec!["status", "--porcelain=v1", "--untracked-files=normal"],
                    String::new(),
                ),
                (
                    vec![
                        "ls-files",
                        "-z",
                        "--cached",
                        "--others",
                        "--exclude-standard",
                    ],
                    "Cargo.toml\0".to_string(),
                ),
            ] {
                invocations.push(invocation(
                    &directory,
                    &fixture.0,
                    phase,
                    invocations.len(),
                    arguments.into_iter().map(str::to_string).collect(),
                    stdout.as_bytes(),
                )?);
            }
        }
        invocations.push(invocation(
            &directory,
            &fixture.0,
            "preflight",
            8,
            vec!["kani".to_string(), "--version".to_string()],
            b"Kani Rust Verifier 0.68.0 (cargo plugin)\nCBMC 6.11.0\n",
        )?);
        invocations.push(invocation(
            &directory,
            &fixture.0,
            "metadata",
            9,
            profile.metadata_arguments("ripr-contract", &fixture.0),
            &metadata_bytes,
        )?);
        let metadata_artifact = invocations[9].stdout.clone();
        invocations.push(invocation(
            &directory,
            &fixture.0,
            "inventory",
            10,
            profile.inventory_arguments("ripr-contract", &fixture.0),
            b"wrote kani-list.json\n",
        )?);
        let mut stdout =
            "Kani Rust Verifier 0.68.0\nCBMC 6.11.0\nChecking harness proof_a::proof_a...\n"
                .to_string();
        // This intentionally fabricates success for cover!(false). It is a
        // fully consistent supplied transcript, never an actual native proof.
        for (number, description, line) in [(1, "true", 3), (2, "false", 4)] {
            stdout.push_str(&format!("Check {number}: proof_a::proof_a.cover.{number}\n - Status: SATISFIED\n - Description: \"{description}\"\n - Location: {}:{line}:5 in function proof_a::proof_a\n\n", subject.file));
        }
        stdout.push_str("VERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n** 0 of 0 failed\n** 2 of 2 cover properties satisfied\n");
        invocations.push(invocation(
            &directory,
            &fixture.0,
            "verification",
            11,
            profile.verification_arguments("ripr-contract", "proof_a::proof_a", 30),
            stdout.as_bytes(),
        )?);
        let (observations, observed, reasons) = formal::native_inventory::observe_invocation(
            &[subject.clone()],
            &invocations[11],
            Some(stdout.as_bytes()),
            Some(b""),
        );
        require(
            reasons.is_empty(),
            format!("synthetic native setup is invalid: {reasons:?}"),
        )?;
        require(
            observations.len() == 1
                && observations[0].status == native::ObservationStatus::Verified,
            "synthetic setup did not exercise successful retained interpretation",
        )?;
        let snapshot = native::Snapshot {
            revision: Some("1".repeat(40)),
            tree: Some("2".repeat(40)),
            dirty: Some(false),
            manifest: Some(source_artifact.clone()),
            file_count: sources.len(),
            error: None,
        };
        let mut run = formal::Run {
            schema: "ripr.native-proof-run.v1".to_string(),
            run_id: "self-consistent-forged-run".to_string(),
            repository: "example/repository".to_string(),
            revision: snapshot.revision.clone(),
            tree: snapshot.tree.clone(),
            worktree_digest: Some(source_artifact.sha256.clone()),
            source_closure_digest: Some(source_artifact.sha256.clone()),
            claim_registry_generation: registry.generation().to_string(),
            provider: formal::Provider {
                family: formal::Family::BoundedModelCheck,
                name: "kani".to_string(),
                version: profile.kani_version.clone(),
                solver_version: Some(profile.cbmc_version.clone()),
            },
            toolchain: formal::Toolchain {
                language: "rustc 1.100.0-nightly (8925ea358 2026-08-20)".to_string(),
                target: Some(profile.target.clone()),
            },
            runner: formal::Runner {
                generation: native::sha256(b"forged executable identity"),
                command_digest: native::sha256(b"pending"),
                deadline_policy: formal::Deadlines {
                    preflight_seconds: 10,
                    whole_run_seconds: 60,
                    per_subject_seconds: 30,
                },
            },
            inventory: formal::Inventory {
                expected_claim_ids: Vec::new(),
                discovered_subjects: vec![subject.id()],
                complete: true,
                rejection_reasons: Vec::new(),
            },
            process: native::aggregate_process(&invocations, &native::sha256(b"pending")),
            results: Vec::new(),
            unregistered_results: observations,
            execution: Some(native::Execution {
                producer: "ripr-kani-custody-v1".to_string(),
                producer_entrypoint: "formal-runner-test".to_string(),
                producer_arguments: vec!["formal-runner-test".to_string()],
                artifact_directory: "kani-evidence/replay".to_string(),
                repository_directory: fixture.0.display().to_string(),
                native_inventory_artifacts: vec![native::NativeInventoryArtifact {
                    package: "ripr-contract".to_string(),
                    artifact: inventory,
                }],
                source_before: snapshot.clone(),
                source_after: snapshot,
                source_current: true,
                configuration_policy: profile.configuration_policy.clone(),
                configuration_digest: config.sha256.clone(),
                configuration_before: config.clone(),
                configuration_after: config,
                cargo_metadata: metadata_artifact,
                installed_toolchain: Some(installed.clone()),
                installed_toolchain_after: Some(installed),
                registration: Some(registration),
                explicit_inputs: prepared.claims[0].explicit_inputs.clone(),
                package_inputs: BTreeMap::from([(
                    "ripr-contract".to_string(),
                    prepared.claims[0].explicit_inputs.clone(),
                )]),
                configuration_admitted: true,
                configuration_rejections: Vec::new(),
                selected_packages: vec!["ripr-contract".to_string()],
                harness_filter: Some("proof_a::proof_a".to_string()),
                default_unwind: Some(32),
                declared_subjects: vec![subject.clone()],
                selected_subjects: vec![subject.clone()],
                native_subjects: vec![subject],
                observed_subjects: observed,
                invocations,
                inventory_rejections: Vec::new(),
                rejection_reasons: Vec::new(),
                allow_empty: false,
                lane_success: true,
                github_run_id: Some("supplied-run".to_string()),
                github_run_attempt: Some("1".to_string()),
                github_job: Some("supplied-job".to_string()),
                non_claims: vec![
                    "Fabricated self-consistent bytes; no process was executed".to_string(),
                ],
            }),
        };
        refresh_index(&directory, &mut run)?;
        Ok(Self {
            fixture,
            prepared,
            run,
            directory,
        })
    }

    fn replay(&self) -> RegistryResult<serde_json::Value> {
        formal::parse_json(
            self.prepared
                .replay_native(&self.run, &self.directory)?
                .as_bytes(),
        )
    }
}

fn refresh_index(directory: &Path, run: &mut formal::Run) -> RegistryResult<()> {
    let execution = run.execution.as_ref().ok_or("fixture execution absent")?;
    let index = json_artifact(directory, "raw-index.json", &execution.invocations)?;
    run.process = native::aggregate_process(&execution.invocations, &index.sha256);
    let commands: Vec<_> = execution
        .invocations
        .iter()
        .map(|call| (&call.program, &call.arguments))
        .collect();
    run.runner.command_digest =
        native::sha256(&serde_json::to_vec_pretty(&commands).map_err(|error| error.to_string())?);
    Ok(())
}

fn refuses<T>(outcome: RegistryResult<T>, fragment: &str) -> RegistryResult<()> {
    let error = outcome.err().ok_or("expected retained replay refusal")?;
    assert!(
        !fragment.is_empty() && error.contains(fragment),
        "unexpected refusal: {error}"
    );
    Ok(())
}

#[test]
fn successful_forged_and_rehashed_replay_cannot_mint_native_origin_or_assurance()
-> RegistryResult<()> {
    let mut supplied = Supplied::new()?;
    for changed in [false, true] {
        if changed {
            let execution = supplied
                .run
                .execution
                .as_mut()
                .ok_or("fixture execution absent")?;
            let call = &mut execution.invocations[11];
            let retained = call.stdout.as_ref().ok_or("fixture stdout absent")?;
            let mut bytes = fs::read(supplied.directory.join(&retained.path))
                .map_err(|error| error.to_string())?;
            bytes.extend_from_slice(b"\n");
            call.stdout = Some(artifact(&supplied.directory, &retained.path, &bytes)?);
            let (rows, seen, reasons) = formal::native_inventory::observe_invocation(
                &execution.selected_subjects,
                call,
                Some(&bytes),
                Some(b""),
            );
            require(
                reasons.is_empty(),
                "rehashed synthetic observation was malformed",
            )?;
            supplied.run.unregistered_results = rows;
            execution.observed_subjects = seen;
            refresh_index(&supplied.directory, &mut supplied.run)?;
        }
        let report = supplied.replay()?;
        println!("SUPPLIED_REPLAY_CONSISTENCY={report}");
        assert_eq!(report["retained_consistency"], "consistent");
        assert_eq!(
            report["claims"][0]["observations"][0]["native_status"],
            "VERIFIED"
        );
        assert_eq!(
            report["claims"][0]["observations"][0]["covers"]
                .as_array()
                .ok_or("cover rows absent")?
                .len(),
            2
        );
        assert_eq!(
            report["claims"]
                .as_array()
                .ok_or("canonical rows absent")?
                .len(),
            2
        );
        assert_eq!(
            report["canonical_scoreboard"]["summary"]["not_established"],
            2
        );
        assert_eq!(report["canonical_scoreboard"]["summary"]["established"], 0);
        assert_eq!(report["native_origin"], "not_established");
        assert_eq!(report["native_assurance"], "not_assessed");
        assert_eq!(report["execution_authority"], "not_granted");
        assert!(
            report["claims"][1]["unestablished_obligations"]
                .as_array()
                .ok_or("missing obligations absent")?
                .iter()
                .any(|value| value
                    .as_str()
                    .is_some_and(|reason| reason.contains("native binding absent")))
        );
    }
    Ok(())
}

#[test]
fn retained_artifact_corruption_and_argv_substitution_are_refused() -> RegistryResult<()> {
    let mut supplied = Supplied::new()?;
    supplied.replay()?;
    let call = &mut supplied
        .run
        .execution
        .as_mut()
        .ok_or("fixture execution absent")?
        .invocations[11];
    call.arguments.push("--disable-overflow-checks".to_string());
    refresh_index(&supplied.directory, &mut supplied.run)?;
    refuses(supplied.replay(), "native invocation argv is outside")?;
    let supplied = Supplied::new()?;
    fs::write(supplied.directory.join("raw/11-stdout"), b"tampered")
        .map_err(|error| error.to_string())?;
    refuses(supplied.replay(), "raw artifact commitment mismatch")?;
    Ok(())
}

#[test]
fn recomputed_cargo_feature_metadata_cannot_borrow_the_rust_only_profile() -> RegistryResult<()> {
    for (change, diagnostic) in [
        ("features", "Cargo resolved features differ"),
        ("library", "Cargo local library source differs"),
    ] {
        let mut supplied = Supplied::new()?;
        let execution = supplied
            .run
            .execution
            .as_mut()
            .ok_or("fixture execution absent")?;
        let previous = execution
            .cargo_metadata
            .as_ref()
            .ok_or("fixture metadata absent")?;
        let mut value: serde_json::Value = formal::parse_json(
            &fs::read(supplied.directory.join(&previous.path))
                .map_err(|error| error.to_string())?,
        )?;
        if change == "features" {
            value["resolve"]["nodes"][0]["features"] = json!(["default", "lang-rust"]);
        } else {
            value["packages"][0]["targets"][0]["src_path"] =
                json!(supplied.fixture.0.join("other.rs"));
        }
        let changed = json_artifact(&supplied.directory, &previous.path, &value)?;
        execution.cargo_metadata = Some(changed.clone());
        execution.invocations[9].stdout = Some(changed);
        refresh_index(&supplied.directory, &mut supplied.run)?;
        refuses(supplied.replay(), diagnostic)?;
    }
    Ok(())
}

#[test]
fn retained_registration_cannot_replace_the_loader_constructed_preparation() -> RegistryResult<()> {
    let mut supplied = Supplied::new()?;
    let mut value: serde_json::Value = formal::parse_json(supplied.prepared.to_json()?.as_bytes())?;
    value["claims"][0]["decision_digest"] =
        json!(native::sha256(b"substituted canonical decision"));
    let replacement = json_artifact(&supplied.directory, "preparation.json", &value)?;
    supplied
        .run
        .execution
        .as_mut()
        .ok_or("fixture execution absent")?
        .registration = Some(replacement);
    refuses(supplied.replay(), "retained preparation differs")?;
    Ok(())
}

#[test]
fn fully_rehashed_source_omission_still_refuses_the_canonical_closure() -> RegistryResult<()> {
    let mut supplied = Supplied::new()?;
    let execution = supplied
        .run
        .execution
        .as_mut()
        .ok_or("fixture execution absent")?;
    let mut sources = supplied.prepared.claims[0].source_manifest.clone();
    require(
        sources.remove("Cargo.lock").is_some(),
        "fixture lockfile absent",
    )?;
    let source = json_artifact(&supplied.directory, "source.json", &sources)?;
    for snapshot in [&mut execution.source_before, &mut execution.source_after] {
        snapshot.manifest = Some(source.clone());
        snapshot.file_count = sources.len();
    }
    supplied.run.source_closure_digest = Some(source.sha256.clone());
    supplied.run.worktree_digest = Some(source.sha256);
    let config = json_artifact(
        &supplied.directory,
        "configuration.json",
        &native::ConfigurationSnapshot {
            inputs: sources,
            cargo_config_keys: BTreeMap::new(),
        },
    )?;
    execution.configuration_digest = config.sha256.clone();
    execution.configuration_before = config.clone();
    execution.configuration_after = config;
    refuses(
        supplied.replay(),
        "retained source closure is omitted, substituted or differs",
    )?;
    Ok(())
}

#[test]
fn rehashed_native_cover_text_cannot_satisfy_a_different_canonical_obligation() -> RegistryResult<()>
{
    let mut supplied = Supplied::new()?;
    let execution = supplied
        .run
        .execution
        .as_mut()
        .ok_or("fixture execution absent")?;
    let call = &mut execution.invocations[11];
    let previous = call.stdout.as_ref().ok_or("fixture stdout absent")?;
    let text = fs::read_to_string(supplied.directory.join(&previous.path))
        .map_err(|error| error.to_string())?;
    require(
        text.contains("Description: \"true\""),
        "fixture source cover text absent",
    )?;
    let changed = text.replace(
        "Description: \"true\"",
        "Description: \"another predicate\"",
    );
    call.stdout = Some(artifact(
        &supplied.directory,
        &previous.path,
        changed.as_bytes(),
    )?);
    let (rows, seen, reasons) = formal::native_inventory::observe_invocation(
        &execution.selected_subjects,
        call,
        Some(changed.as_bytes()),
        Some(b""),
    );
    require(
        reasons.is_empty()
            && rows
                .first()
                .is_some_and(|row| row.status == native::ObservationStatus::Verified),
        "cover substitution failed before canonical binding",
    )?;
    supplied.run.unregistered_results = rows;
    execution.observed_subjects = seen;
    refresh_index(&supplied.directory, &mut supplied.run)?;
    refuses(
        supplied.replay(),
        "canonical native cover presentation differs",
    )?;
    Ok(())
}

#[test]
fn exact_native_selection_cannot_borrow_a_substring_or_foreign_generation() -> RegistryResult<()> {
    let mut supplied = Supplied::new()?;
    supplied.run.claim_registry_generation.push('x');
    refuses(
        supplied.replay(),
        "retained run registry generation mismatch",
    )?;
    let mut supplied = Supplied::new()?;
    supplied
        .run
        .execution
        .as_mut()
        .ok_or("fixture execution absent")?
        .harness_filter = Some("proof_a".to_string());
    refuses(
        supplied.replay(),
        "exact retained harness is absent or ambiguous",
    )?;
    Ok(())
}

#[test]
fn borrowed_prepared_views_keep_the_original_canonical_keys_and_source_inputs() -> RegistryResult<()>
{
    let supplied = Supplied::new()?;
    let view = supplied
        .prepared
        .claims()
        .find(|claim| claim.id() == "example.a")
        .ok_or("prepared claim absent")?;
    assert_eq!(view.generation(), 1);
    assert_eq!(
        view.decision_digest(),
        supplied.prepared.claims[0].decision_digest
    );
    assert_eq!(view.binding_state(), "prepared");
    assert_eq!(
        view.source_manifest(),
        &supplied.prepared.claims[0].source_manifest
    );
    assert_eq!(
        view.explicit_inputs(),
        supplied.prepared.claims[0].explicit_inputs
    );
    assert_eq!(
        view.profile(),
        Some(&native::DeclaredKaniProfile::rust_only())
    );
    assert_eq!(
        view.subjects().next().ok_or("prepared subject absent")?.0,
        "proof_a"
    );
    assert!(supplied.fixture.0.is_dir());
    Ok(())
}

#[test]
fn public_replay_command_preserves_the_canonical_denominator_and_refuses_repeated_calls()
-> RegistryResult<()> {
    let mut supplied = Supplied::new()?;
    let mut bindings = supplied.fixture.bindings(supplied.prepared.registry())?;
    bindings.bindings[0].profile = native::DeclaredKaniProfile::rust_only();
    json_artifact(&supplied.directory, "bindings.json", &bindings)?;
    json_artifact(&supplied.directory, "run.json", &supplied.run)?;
    let output = crate::formal_registry::execute(&[
        "replay-native".to_string(),
        "--root".to_string(),
        supplied.fixture.0.display().to_string(),
        "--bindings".to_string(),
        supplied
            .directory
            .join("bindings.json")
            .display()
            .to_string(),
        "--run".to_string(),
        supplied.directory.join("run.json").display().to_string(),
        "--artifacts".to_string(),
        supplied.directory.display().to_string(),
    ])?;
    let value: serde_json::Value = formal::parse_json(output.as_bytes())?;
    assert_eq!(
        value["canonical_scoreboard"]["summary"]["registered_claims"],
        2
    );
    assert_eq!(value["canonical_scoreboard"]["summary"]["established"], 0);
    assert_eq!(value["native_origin"], "not_established");
    let execution = supplied
        .run
        .execution
        .as_mut()
        .ok_or("fixture execution absent")?;
    execution.invocations.push(execution.invocations[8].clone());
    refresh_index(&supplied.directory, &mut supplied.run)?;
    refuses(supplied.replay(), "exceeds twelve invocation records")?;
    Ok(())
}
