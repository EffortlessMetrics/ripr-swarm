//! Run-owned initial checkpoint. A stopped or refused attempt cannot leave the
//! previous invocation's successful canonical alias as its current result.
use super::*;

pub(super) fn invalidate_aliases(root: &Path) -> Result<(), String> {
    for name in ["kani.json", "kani.md"] {
        match fs::remove_file(root.join("target/ripr/reports").join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("invalidate previous Kani alias: {error}")),
        }
    }
    Ok(())
}

pub(super) fn initial_checkpoint(
    custody: &mut Custody,
    args: &lane::KaniArgs,
    packages: &[String],
    reason: &str,
) -> Result<(), String> {
    let directory = custody.directory.join("initial");
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let mut retain = |name: &str, bytes: &[u8]| -> Result<Artifact, String> {
        let artifact = custody.retain(&format!("initial/{name}"), bytes)?;
        Ok(Artifact {
            path: name.to_string(),
            ..artifact
        })
    };
    let configuration = formal::native::ConfigurationSnapshot {
        inputs: BTreeMap::new(),
        cargo_config_keys: BTreeMap::new(),
    };
    let config = retain("configuration.json", &json(&configuration)?)?;
    retain("raw-index.json", b"[]")?;
    let reason = reason.to_string();
    let snapshot = Snapshot {
        revision: None,
        tree: None,
        dirty: None,
        manifest: None,
        file_count: 0,
        error: Some(reason.clone()),
    };
    let artifact_directory = format!("{}/initial", custody.relative);
    let report = formal::Run {
        schema: "ripr.native-proof-run.v1".to_string(), run_id: artifact_directory.clone(),
        repository: "EffortlessMetrics/ripr-swarm".to_string(), revision: None, tree: None,
        worktree_digest: None, source_closure_digest: None, claim_registry_generation: "unregistered-no-assurance".to_string(),
        provider: formal::Provider { family: formal::Family::BoundedModelCheck, name: "kani".to_string(), version: "unavailable".to_string(), solver_version: None },
        toolchain: formal::Toolchain { language: "unavailable".to_string(), target: None },
        runner: formal::Runner { generation: hash(&formal::native::canonical_manifest_bytes(&compiled_runner_manifest())?),
            command_digest: hash(b"[]"), deadline_policy: formal::Deadlines { preflight_seconds: args.timeout_secs.clamp(1,10), whole_run_seconds: args.timeout_secs, per_subject_seconds: args.harness_timeout_secs } },
        inventory: formal::Inventory { expected_claim_ids: Vec::new(), discovered_subjects: Vec::new(), complete: false, rejection_reasons: vec![reason.clone()] },
        process: formal::Process { state: formal::ProcessState::NotStarted, exit_code: None, timed_out: false,
            raw_output_digest: None, cleanup_state: formal::Cleanup::NotApplicable }, results: Vec::new(), unregistered_results: Vec::new(),
        execution: Some(Execution { producer: "ripr-kani-custody-v1".to_string(), producer_entrypoint: option_env!("CARGO_BIN_NAME").unwrap_or("formal-runner-test").to_string(), producer_arguments: std::env::args().collect(),
            artifact_directory, repository_directory: custody.root.display().to_string(), native_inventory_artifacts: Vec::new(),
            source_before: snapshot.clone(), source_after: snapshot, source_current: false, configuration_policy: "kani-0.68-defaults-v1".to_string(),
            configuration_digest: config.sha256.clone(), configuration_before: config.clone(), configuration_after: config,
            cargo_metadata: None, installed_toolchain: None, installed_toolchain_after: None, registration: None, explicit_inputs: Vec::new(), package_inputs: BTreeMap::new(),
            configuration_admitted: false, configuration_rejections: vec![reason.clone()], selected_packages: packages.to_vec(), harness_filter: args.harness.clone(), default_unwind: args.default_unwind,
            declared_subjects: Vec::new(), selected_subjects: Vec::new(), native_subjects: Vec::new(), observed_subjects: Vec::new(), invocations: Vec::new(),
            inventory_rejections: vec![reason.clone()], rejection_reasons: vec![reason.clone()], allow_empty: args.allow_empty, lane_success: false,
            github_run_id: std::env::var("GITHUB_RUN_ID").ok(), github_run_attempt: std::env::var("GITHUB_RUN_ATTEMPT").ok(), github_job: std::env::var("GITHUB_JOB").ok(),
            non_claims: vec![reason.clone(), "This initial checkpoint contains no source, native inventory or verification observation.".to_string()] }),
    };
    formal::audit(&[], Some(&report))?;
    verify_retained(&report, &directory)?;
    let bytes = json(&report)?;
    let markdown = lane::render_report(
        &[],
        &format!(
            "NOT-ESTABLISHED: {reason}. Evidence: `{}`.",
            custody.relative
        ),
    );
    custody.retain("initial/kani.md", markdown.as_bytes())?;
    custody.retain("initial/kani.json", &bytes)?;
    let reports = custody.root.join("target/ripr/reports");
    publish(&reports.join("kani.md"), markdown.as_bytes())?;
    publish(&reports.join("kani.json"), &bytes)
}
