//! Native Kani capture and custody under #14472/#12470. This extends the
//! existing process owner and report contract; it grants no execution authority.
use crate::{formal_evidence as formal, kani_lane as lane, run};
use formal::native::{
    Artifact, Execution, Invocation, NativeInventoryArtifact, ObservationStatus, Snapshot, Subject,
};
use serde::Serialize;

#[path = "kani_evidence/capture.rs"]
mod capture;
#[path = "kani_evidence/configuration.rs"]
mod configuration;
#[path = "kani_evidence/inventory.rs"]
mod inventory;
#[path = "kani_evidence/publication.rs"]
mod publication;
#[path = "kani_evidence/registration.rs"]
mod registration;
#[cfg(test)]
#[path = "kani_evidence/tests.rs"]
mod tests;
use capture::Custody;
use configuration::{configuration, installed_toolchain};
use formal::native::{native_tool_header, validate_cargo_metadata};
use formal::native::{sha256 as hash, source_is_current as source_current};
use formal::native_inventory::{
    compare_inventories, native_inventory, observe, observe_invocation,
};
use inventory::declared;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SMALL_STREAM: usize = 1024 * 1024;
// The measured repository Git listing exceeds 1 MiB; other small streams retain
// their existing bounds.
const GIT_INVENTORY_STREAM: usize = 4 * 1024 * 1024;
const NATIVE_STREAM: usize = 16 * 1024 * 1024;
const TOTAL_ARTIFACT_BYTES: u64 = 96 * 1024 * 1024;
const FILE_BYTES: u64 = 8 * 1024 * 1024;
const SOURCE_BYTES: u64 = 256 * 1024 * 1024;
const PACKAGES: [(&str, &str); 2] = [
    ("ripr", "crates/ripr/src"),
    ("ripr-contract", "crates/ripr-contract/src"),
];

fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(value).map_err(|e| e.to_string())
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    formal::native::read_file_bounded(path, limit)
}

pub(crate) fn invalidate_current_attempt() -> Result<(), String> {
    let root = std::env::current_dir()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    publication::invalidate_aliases(&root)
}

pub(crate) fn record_argument_refusal(error: &str) -> Result<(), String> {
    let root = std::env::current_dir()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    let args = lane::KaniArgs::default();
    let packages: Vec<_> = PACKAGES
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    let mut custody = Custody::create(&root, &packages)?;
    publication::initial_checkpoint(
        &mut custody,
        &args,
        &packages,
        &format!("invalid native-capture arguments; no verifier was run: {error}"),
    )
}

fn projected_rows(run: &formal::Run) -> Vec<lane::HarnessResult> {
    let mut rows: Vec<_> = run
        .unregistered_results
        .iter()
        .map(|row| lane::HarnessResult {
            harness: lane::Harness {
                file: row.subject.file.clone(),
                name: row.subject.name.clone(),
            },
            status: match row.status {
                ObservationStatus::Verified => lane::HarnessStatus::Verified,
                ObservationStatus::Failed => lane::HarnessStatus::Failed(row.detail.clone()),
                ObservationStatus::NotEstablished => {
                    lane::HarnessStatus::NotEstablished(row.detail.clone())
                }
            },
            time: row.kani_time.clone(),
        })
        .collect();
    if let Some(execution) = &run.execution {
        for declared in &execution.declared_subjects {
            if execution
                .harness_filter
                .as_ref()
                .is_some_and(|wanted| !declared.name.contains(wanted))
            {
                continue;
            }
            if !execution.native_subjects.iter().any(|s| {
                s.package == declared.package && s.file == declared.file && s.name == declared.name
            }) {
                rows.push(lane::HarnessResult {
                    harness: lane::Harness {
                        file: declared.file.clone(),
                        name: declared.name.clone(),
                    },
                    status: lane::HarnessStatus::NotEstablished(
                        "compiled native identity was not established".to_string(),
                    ),
                    time: "n/a".to_string(),
                });
            }
        }
    }
    rows.sort_by(|a, b| {
        (&a.harness.file, &a.harness.name).cmp(&(&b.harness.file, &b.harness.name))
    });
    rows
}
/// Called by the existing lane after argument parsing. The owned #12581 probe
/// remains the sole preflight decision; it supplies its observed capture here.
pub(crate) fn run_lane(
    args: lane::KaniArgs,
    probe: impl FnOnce(Duration) -> lane::ProbeObservation,
) -> Result<(), String> {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(args.timeout_secs);
    let root = std::env::current_dir()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    let packages: Vec<String> = PACKAGES
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| args.package.as_deref().is_none_or(|wanted| wanted == *name))
        .map(str::to_string)
        .collect();
    publication::invalidate_aliases(&root)?;
    let mut custody = Custody::create(&root, &packages)?;
    publication::initial_checkpoint(
        &mut custody,
        &args,
        &packages,
        "native capture is pending; no verifier result is established",
    )?;
    custody.deadline = Some(deadline);
    let mut reasons = Vec::new();
    let prepared = match registration::prepare(&root, &args, &packages) {
        Ok(prepared) => prepared,
        Err(error) => {
            reasons.push(format!(
                "registration was not admitted before execution: {error}"
            ));
            None
        }
    };
    let registration_artifact = if let Some(prepared) = &prepared {
        custody.required_inputs.push(prepared.registry_path.clone());
        Some(custody.retain("prepared-registration.json", &json(prepared)?)?)
    } else {
        None
    };
    let (before, sources) = custody.snapshot("before");
    let (configuration_digest, mut configuration_rejections, configuration_bytes) =
        configuration(&root, &sources, &packages);
    if before.error.is_some() {
        configuration_rejections.push(
            "source capture is incomplete; verification configuration is not established"
                .to_string(),
        );
    }
    let configuration_before = custody.retain("before-configuration.json", &configuration_bytes)?;
    let runner_matches = compiled_runner_matches(&sources);
    let declared = declared(&root, &sources);
    if !runner_matches {
        reasons.push("running collector source differs from captured checkout".to_string());
    }
    if let Some(error) = &before.error {
        reasons.push(format!("source capture: {error}"));
    }
    let declared = match declared {
        Ok(value) => value,
        Err(error) => {
            reasons.push(error);
            Vec::new()
        }
    };
    let declared: Vec<_> = declared
        .into_iter()
        .filter(|subject| packages.contains(&subject.package))
        .collect();
    let preflight_budget = args.timeout_secs.clamp(1, 10);
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(preflight_budget));
    let mut preflight = Invocation {
        phase: "preflight".to_string(),
        package: None,
        program: "cargo".to_string(),
        arguments: vec!["kani".to_string(), "--version".to_string()],
        working_directory: root.display().to_string(),
        timeout_ms: remaining.as_millis().min(u128::from(u64::MAX)).max(1) as u64,
        stream_limit_bytes: SMALL_STREAM,
        duration_ms: None,
        exit_code: None,
        native_status: None,
        timed_out: false,
        capture_complete: false,
        stdout: None,
        stderr: None,
        error: None,
    };
    let mut version = "unavailable".to_string();
    let probe = if remaining.as_millis() == 0 {
        lane::ProbeObservation {
            output: None,
            failure: Some("whole-run deadline exhausted before version preflight".to_string()),
        }
    } else {
        probe(remaining)
    };
    if let Some(error) = probe.failure {
        preflight.error = Some(error.clone());
        reasons.push(error);
    }
    if let Some(output) = probe.output {
        preflight.duration_ms = Some(output.duration.as_millis().min(u128::from(u64::MAX)) as u64);
        preflight.exit_code = output
            .status
            .as_ref()
            .and_then(std::process::ExitStatus::code);
        preflight.native_status = output.status.as_ref().map(ToString::to_string);
        preflight.timed_out = output.timed_out;
        preflight.stdout = Some(custody.retain("preflight.stdout", &output.stdout)?);
        preflight.stderr = Some(custody.retain("preflight.stderr", &output.stderr)?);
        preflight.capture_complete = output.status.is_some();
        if preflight.error.is_none() {
            let supported = std::str::from_utf8(&output.stdout)
                .is_ok_and(|stdout| native_tool_header(stdout).is_ok());
            if supported {
                version = "0.68.0".to_string();
            } else {
                reasons
                    .push("unsupported or unrecognized Kani version; expected 0.68.0".to_string());
            }
        }
    }
    let available = version == "0.68.0" && preflight.succeeded();
    custody.invocations.push(preflight);
    let mut native = Vec::new();
    let mut selected = Vec::new();
    let mut rows = Vec::new();
    let mut observed = Vec::new();
    let mut inventory_rejections = Vec::new();
    let mut native_inventory_artifacts = Vec::new();
    let mut cargo_metadata = None;
    let mut toolchain = None;
    let mut toolchain_artifact = None;
    let mut solver_version = available.then(|| "6.11.0".to_string());
    if available && reasons.is_empty() && configuration_rejections.is_empty() {
        match installed_toolchain() {
            Ok(installed) => {
                toolchain_artifact =
                    Some(custody.retain("installed-toolchain.json", &json(&installed)?)?);
                toolchain = Some(installed);
            }
            Err(error) => reasons.push(error),
        }
        let command = vec![
            "metadata".to_string(),
            "--locked".to_string(),
            "--format-version".to_string(),
            "1".to_string(),
        ];
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            reasons.push("whole-run deadline exhausted before Cargo metadata".to_string());
        } else {
            match custody.capture(
                "metadata",
                None,
                "cargo",
                &command,
                &root,
                (remaining.min(Duration::from_secs(30)), NATIVE_STREAM),
            ) {
                Ok((stdout, _)) => {
                    if custody
                        .invocations
                        .last()
                        .is_some_and(Invocation::succeeded)
                    {
                        match validate_cargo_metadata(stdout.as_bytes(), &root, &sources) {
                            Ok(()) => {
                                cargo_metadata =
                                    custody.invocations.last().and_then(|i| i.stdout.clone())
                            }
                            Err(error) => reasons.push(error),
                        }
                    } else {
                        reasons.push("Cargo metadata did not complete successfully".to_string());
                    }
                }
                Err(error) => reasons.push(error),
            }
        }
    }
    if available && reasons.is_empty() && configuration_rejections.is_empty() {
        for package in packages.iter().map(String::as_str) {
            let cwd = custody.directory.join(format!("list-{package}"));
            fs::create_dir(&cwd).map_err(|e| e.to_string())?;
            let command = formal::native::inventory_arguments(package, &root);
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                inventory_rejections
                    .push("whole-run deadline exhausted before inventory".to_string());
                break;
            }
            match custody.capture(
                "inventory",
                Some(package),
                "cargo",
                &command,
                &cwd,
                (remaining, SMALL_STREAM),
            ) {
                Err(error) => inventory_rejections.push(format!("{package}: {error}")),
                Ok((stdout, _)) => {
                    if let Err(error) = native_tool_header(&stdout) {
                        inventory_rejections.push(format!("{package}: {error}"));
                    } else {
                        solver_version = Some("6.11.0".to_string());
                    }
                    if !custody
                        .invocations
                        .last()
                        .is_some_and(Invocation::succeeded)
                    {
                        inventory_rejections.push(format!(
                            "{package}: native inventory process did not succeed"
                        ));
                        continue;
                    }
                    match read_bounded(&cwd.join("kani-list.json"), SMALL_STREAM as u64).and_then(
                        |bytes| {
                            let artifact =
                                custody.retain(&format!("{package}-native-list.json"), &bytes)?;
                            native_inventory_artifacts.push(NativeInventoryArtifact {
                                package: package.to_string(),
                                artifact,
                            });
                            native_inventory(&bytes, package, &root, &sources)
                        },
                    ) {
                        Ok(subjects) => native.extend(subjects),
                        Err(error) => inventory_rejections.push(format!("{package}: {error}")),
                    }
                }
            }
        }
        inventory_rejections.extend(compare_inventories(&declared, &native));
        selected = native
            .iter()
            .filter(|s| {
                args.harness.as_ref().is_none_or(|wanted| {
                    s.qualified_name
                        .as_ref()
                        .is_some_and(|name| name.contains(wanted))
                })
            })
            .cloned()
            .collect();
        if let Some(prepared) = &prepared {
            for id in &args.registered_claims {
                let binding = prepared
                    .claims
                    .iter()
                    .find(|p| &p.claim.claim_id == id)
                    .ok_or("prepared selected claim disappeared")?;
                if !binding
                    .subject
                    .as_ref()
                    .is_some_and(|s| selected.contains(s))
                {
                    inventory_rejections.push(format!(
                        "registered subject was not selected with its exact captured bytes: {id}"
                    ));
                }
            }
        }
        if inventory_rejections.is_empty() {
            for package in packages.iter().map(String::as_str) {
                let subjects: Vec<_> = selected
                    .iter()
                    .filter(|s| s.package == package)
                    .cloned()
                    .collect();
                if subjects.is_empty() {
                    continue;
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    reasons.push(format!(
                        "{package}: whole-run deadline exhausted before verification"
                    ));
                    let (missing, _, missing_reasons) = observe(&subjects, "");
                    rows.extend(missing);
                    inventory_rejections.extend(missing_reasons);
                    continue;
                }
                let mut command = lane::kani_command(&args, package);
                // Native version output names Kani's verifier compiler and CBMC;
                // the product toolchain pin alone is not the verifier compiler.
                command.push("--verbose".to_string());
                let captured = custody.capture(
                    "verification",
                    Some(package),
                    "cargo",
                    &command,
                    &root,
                    (remaining, NATIVE_STREAM),
                );
                if let Err(error) = captured {
                    reasons.push(format!("{package}: {error}"));
                }
                let invocation = custody
                    .invocations
                    .last()
                    .ok_or("native invocation record missing")?;
                let stdout = invocation
                    .stdout
                    .as_ref()
                    .map(|a| read_artifact(&custody.directory, a))
                    .transpose()?;
                let stderr = invocation
                    .stderr
                    .as_ref()
                    .map(|a| read_artifact(&custody.directory, a))
                    .transpose()?;
                let (results, seen, missing) =
                    observe_invocation(&subjects, invocation, stdout.as_deref(), stderr.as_deref());
                rows.extend(results);
                observed.extend(seen);
                inventory_rejections.extend(missing);
                if !invocation.succeeded() {
                    reasons.push(format!(
                        "{package}: native verification did not complete successfully"
                    ));
                }
            }
        }
    }
    for subject in &selected {
        if !rows.iter().any(|row| row.subject == *subject) {
            let (missing, _, _) = observe(std::slice::from_ref(subject), "");
            rows.extend(missing);
        }
    }
    native.sort();
    selected.sort();
    rows.sort_by_key(|r| r.subject.id());
    observed.sort();
    observed.dedup();
    if selected.is_empty() {
        inventory_rejections.push("zero selected native harnesses".to_string());
    }
    let (after, after_files) = custody.snapshot("after");
    let (after_config, after_config_rejections, after_configuration_bytes) =
        configuration(&root, &after_files, &packages);
    let configuration_after =
        custody.retain("after-configuration.json", &after_configuration_bytes)?;
    let after_toolchain = toolchain.as_ref().and_then(|_| installed_toolchain().ok());
    let toolchain_current = toolchain == after_toolchain;
    let after_toolchain_artifact = after_toolchain
        .as_ref()
        .map(|value| custody.retain("after-installed-toolchain.json", &json(value)?))
        .transpose()?;
    let current = toolchain_current
        && source_current(&before, &after)
        && after_config == configuration_digest
        && after_config_rejections == configuration_rejections;
    if !current {
        reasons.push("source/configuration identity changed or could not be captured".to_string());
    }
    reasons.extend(configuration_rejections.iter().cloned());
    reasons.extend(inventory_rejections.iter().cloned());
    reasons.sort();
    reasons.dedup();
    let raw_bytes = json(&custody.invocations)?;
    let raw_index = custody.retain("raw-index.json", &raw_bytes)?;
    let inventory_complete = inventory_rejections.is_empty() && !selected.is_empty();
    let mut lane_success = current
        && reasons.is_empty()
        && inventory_complete
        && available
        && rows.iter().all(|r| r.status == ObservationStatus::Verified)
        && custody.invocations.iter().all(Invocation::succeeded);
    let process = formal::native::aggregate_process(&custody.invocations, &raw_index.sha256);
    let command_digest = hash(&json(
        &custody
            .invocations
            .iter()
            .map(|i| (&i.program, &i.arguments))
            .collect::<Vec<_>>(),
    )?);
    let execution=Execution { producer:"ripr-kani-custody-v1".to_string(),producer_entrypoint:option_env!("CARGO_BIN_NAME").unwrap_or("formal-runner-test").to_string(),producer_arguments:std::env::args().collect(),artifact_directory:custody.relative.clone(),
        source_before:before.clone(),source_after:after,source_current:current,
        repository_directory:root.display().to_string(),native_inventory_artifacts,
        configuration_policy:"kani-0.68-defaults-v1".to_string(),configuration_digest,configuration_before,configuration_after,cargo_metadata,installed_toolchain:toolchain_artifact,installed_toolchain_after:after_toolchain_artifact,registration:registration_artifact,explicit_inputs:custody.explicit_inputs.iter().cloned().collect(),package_inputs:custody.package_inputs.iter().map(|(package, paths)| (package.clone(), paths.iter().cloned().collect())).collect(),configuration_admitted:configuration_rejections.is_empty(),configuration_rejections,
        selected_packages:packages,harness_filter:args.harness.clone(),default_unwind:args.default_unwind,declared_subjects:declared,selected_subjects:selected.clone(),native_subjects:native,observed_subjects:observed,
        invocations:std::mem::take(&mut custody.invocations),inventory_rejections,rejection_reasons:reasons.clone(),allow_empty:args.allow_empty,lane_success,
        github_run_id:std::env::var("GITHUB_RUN_ID").ok(),github_run_attempt:std::env::var("GITHUB_RUN_ATTEMPT").ok(),github_job:std::env::var("GITHUB_JOB").ok(),
        non_claims:vec!["Unregistered harness observations do not establish registered semantic claims or specification adequacy.".to_string(),
            "Digests bind retained bytes, not producer authenticity. Dependency/compiler/model correctness remains trusted.".to_string(),
            "Before/after equality detects observed drift; it is not an immutable filesystem snapshot or protection against change-and-restore.".to_string(),
            "Preflight, verification and inventory streams retain exact bytes when bounded capture completes; absent or partial capture cannot establish assurance.".to_string()] };
    let mut report = formal::Run {
        schema: "ripr.native-proof-run.v1".to_string(),
        run_id: custody.relative.clone(),
        repository: "EffortlessMetrics/ripr-swarm".to_string(),
        revision: before.revision,
        tree: before.tree,
        worktree_digest: before.manifest.as_ref().map(|a| a.sha256.clone()),
        source_closure_digest: before.manifest.map(|a| a.sha256),
        claim_registry_generation: prepared.as_ref().map_or_else(
            || "unregistered-no-assurance".to_string(),
            |p| p.registry_generation.clone(),
        ),
        provider: formal::Provider {
            family: formal::Family::BoundedModelCheck,
            name: "kani".to_string(),
            version,
            solver_version,
        },
        toolchain: formal::Toolchain {
            language: toolchain
                .as_ref()
                .map_or_else(|| "unavailable".to_string(), |t| t.rustc.clone()),
            target: toolchain.as_ref().map(|t| t.target.clone()),
        },
        runner: formal::Runner {
            generation: hash(&formal::native::canonical_manifest_bytes(
                &compiled_runner_manifest(),
            )?),
            command_digest,
            deadline_policy: formal::Deadlines {
                preflight_seconds: preflight_budget,
                whole_run_seconds: args.timeout_secs,
                per_subject_seconds: args.harness_timeout_secs,
            },
        },
        inventory: formal::Inventory {
            expected_claim_ids: prepared
                .as_ref()
                .map_or_else(Vec::new, |_| args.registered_claims.clone()),
            discovered_subjects: selected.iter().map(Subject::id).collect(),
            complete: inventory_complete,
            rejection_reasons: if inventory_complete {
                Vec::new()
            } else {
                reasons.clone()
            },
        },
        process,
        results: Vec::new(),
        unregistered_results: rows,
        execution: Some(execution),
    };
    if let Some(prepared) = &prepared {
        report.results = formal::native::registered_rows(prepared, &report, &sources)?;
        if report
            .results
            .iter()
            .any(|r| r.status != formal::Status::Established)
        {
            lane_success = false;
            if let Some(execution) = report.execution.as_mut() {
                execution.lane_success = false;
                execution.rejection_reasons.push(
                    "one or more selected registered claims were not established".to_string(),
                );
            }
        }
    }
    let claims = registration::claims(prepared.as_ref());
    formal::audit(&claims, Some(&report))?;
    let bytes = json(&report)?;
    let reread: formal::Run = formal::parse_json(&bytes)?;
    formal::audit(&claims, Some(&reread))?;
    let note = format!(
        "Native execution: {}. Registered rows: {} established of {} selected, with local unauthenticated custody.\n\nEvidence: `{}`.\n\n{}",
        if lane_success {
            "complete"
        } else {
            "NOT-ESTABLISHED"
        },
        report
            .results
            .iter()
            .filter(|r| r.status == formal::Status::Established)
            .count(),
        report.results.len(),
        custody.relative,
        reasons
            .iter()
            .map(|r| markdown_text(r))
            .collect::<Vec<_>>()
            .join("; ")
    );
    verify_retained(&reread, &custody.directory)?;
    let markdown = lane::render_report(&projected_rows(&reread), &note);
    custody.retain("kani.md", markdown.as_bytes())?;
    custody.retain("kani.json", &bytes)?;
    let reports = root.join("target/ripr/reports");
    publish(&reports.join("kani.md"), markdown.as_bytes())?;
    // JSON is the commit marker and authority. It is published after the same
    // DTO's Markdown, and links the unique immutable raw directory.
    publish(&reports.join("kani.json"), &bytes)?;
    println!("{markdown}");
    let admitted_empty = args.allow_empty
        && selected.is_empty()
        && available
        && current
        && reasons == ["zero selected native harnesses"]
        && report.execution.as_ref().is_some_and(|e| {
            e.configuration_admitted && e.inventory_rejections == ["zero selected native harnesses"]
        });
    if lane_success || admitted_empty {
        Ok(())
    } else {
        Err(
            "NOT-ESTABLISHED: native Kani execution or inventory is incomplete; see kani.json"
                .to_string(),
        )
    }
}
fn publish(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let stage = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&stage, path)
        .map_err(|e| format!("atomic report publication {}: {e}", path.display()))
}

fn read_artifact(directory: &Path, artifact: &Artifact) -> Result<Vec<u8>, String> {
    formal::native::verify_artifact(directory, artifact, TOTAL_ARTIFACT_BYTES)
}
fn verify_retained(report: &formal::Run, directory: &Path) -> Result<(), String> {
    formal::native::verify_execution_artifacts(report, directory).map(|_| ())
}

pub(crate) fn replay(path: &Path) -> Result<(), String> {
    let report: formal::Run = formal::parse_json(&read_bounded(path, SMALL_STREAM as u64)?)?;
    let execution = report
        .execution
        .as_ref()
        .ok_or("report contains no captured execution")?;
    let parent = path.parent().ok_or("report parent absent")?;
    let directory = if parent.join("raw-index.json").is_file() {
        parent.to_path_buf()
    } else {
        parent.join(&execution.artifact_directory)
    };
    verify_retained(&report, &directory)?;
    println!(
        "{}",
        lane::render_report(
            &projected_rows(&report),
            "Retained native interpretation reproduced. This is historical replay, not a new verification or current-source assurance claim."
        )
    );
    if execution.lane_success {
        Ok(())
    } else {
        Err("retained run did not establish a complete successful lane".to_string())
    }
}

fn markdown_text(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ")
        .replace('|', "\\|")
        .replace('`', "\\`")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn compiled_runner_manifest() -> formal::native::SourceManifest {
    formal::compiled_runner_manifest()
}
fn compiled_runner_matches(sources: &BTreeMap<String, String>) -> bool {
    compiled_runner_manifest()
        .iter()
        .all(|(path, digest)| sources.get(path) == Some(digest))
}
