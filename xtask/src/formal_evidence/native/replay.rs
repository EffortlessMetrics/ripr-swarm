//! One retained-byte replay owner used by live collection and the scoreboard.
use super::*;
use crate::formal_evidence::{
    self as formal,
    native_inventory::{compare_inventories, native_inventory, observe, observe_invocation},
    parse_json,
};
use std::cell::{Cell, RefCell};
const MAX_BYTES: u64 = 96 * 1024 * 1024;
fn json<T: serde::Serialize>(value: &T) -> AuditResult<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|error| error.to_string())
}

pub fn verify_execution_artifacts(report: &Run, directory: &Path) -> AuditResult<SourceManifest> {
    validate(report)?;
    let execution = report.execution.as_ref().ok_or("no captured execution")?;
    let retained = RefCell::new(BTreeMap::<String, (String, u64)>::new());
    let total = Cell::new(0u64);
    let read = |artifact: &Artifact| -> AuditResult<Vec<u8>> {
        if let Some(previous) = retained.borrow().get(&artifact.path) {
            require(
                previous == &(artifact.sha256.clone(), artifact.bytes),
                "one artifact path has conflicting commitments",
            )?;
        } else {
            let next = total
                .get()
                .checked_add(artifact.bytes)
                .ok_or("artifact byte count overflow")?;
            require(
                next <= MAX_BYTES,
                "retained artifacts exceed the run replay budget",
            )?;
            total.set(next);
        }
        retained.borrow_mut().insert(
            artifact.path.clone(),
            (artifact.sha256.clone(), artifact.bytes),
        );
        verify_artifact(directory, artifact, MAX_BYTES)
    };
    let mut manifests = Vec::new();
    for (phase, snapshot) in [
        ("before", &execution.source_before),
        ("after", &execution.source_after),
    ] {
        let manifest = if let Some(artifact) = &snapshot.manifest {
            let manifest = parse_source_manifest(&read(artifact)?)?;
            require(
                manifest.len() == snapshot.file_count,
                "source manifest count contradicts snapshot",
            )?;
            manifest
        } else {
            SourceManifest::new()
        };
        verify_snapshot(execution, phase, snapshot, &read)?;
        manifests.push(manifest);
    }
    let before = manifests.first().ok_or("before manifest absent")?;
    require(
        report.revision == execution.source_before.revision
            && report.tree == execution.source_before.tree
            && report.source_closure_digest
                == execution
                    .source_before
                    .manifest
                    .as_ref()
                    .map(|a| a.sha256.clone())
            && report.worktree_digest == report.source_closure_digest,
        "run source identity disagrees with its retained snapshot",
    )?;
    let configuration = read(&execution.configuration_before)?;
    let before_config: ConfigurationSnapshot = parse_json(&configuration)?;
    let after_config: ConfigurationSnapshot = parse_json(&read(&execution.configuration_after)?)?;
    require(
        sha256(&configuration) == execution.configuration_digest,
        "configuration commitment disagrees with artifact",
    )?;
    if execution.configuration_admitted && execution.source_before.error.is_none() {
        validate_configuration(&before_config, before)?;
        if execution.source_current {
            validate_configuration(&after_config, &manifests[1])?;
        }
    }
    for value in before_config
        .inputs
        .values()
        .chain(after_config.inputs.values())
    {
        digest(value, "configuration input commitment")?;
    }
    if let Some(artifact) = &execution.cargo_metadata {
        validate_cargo_metadata(
            &read(artifact)?,
            Path::new(&execution.repository_directory),
            before,
        )?;
        require(
            execution
                .invocations
                .iter()
                .filter(|i| i.phase == "metadata")
                .count()
                == 1
                && execution.invocations.iter().any(|i| {
                    i.phase == "metadata" && i.succeeded() && i.stdout.as_ref() == Some(artifact)
                }),
            "resolved Cargo metadata is not the successful captured metadata invocation",
        )?;
    }
    let installed = execution
        .installed_toolchain
        .as_ref()
        .map(|artifact| -> AuditResult<InstalledToolchain> {
            let value = parse_json(&read(artifact)?)?;
            validate_installed_toolchain(&value)?;
            Ok(value)
        })
        .transpose()?;
    let after_installed = execution
        .installed_toolchain_after
        .as_ref()
        .map(|artifact| -> AuditResult<InstalledToolchain> {
            let value = parse_json(&read(artifact)?)?;
            validate_installed_toolchain(&value)?;
            Ok(value)
        })
        .transpose()?;
    if let Some(installed) = &installed {
        require(
            report.toolchain.language == installed.rustc
                && report.toolchain.target.as_ref() == Some(&installed.target),
            "reported verifier compiler differs from observed bundle",
        )?;
    }
    if execution.source_current {
        require(
            installed == after_installed,
            "Kani bundle changed during capture",
        )?;
    }
    let expected_index = json(&execution.invocations)?;
    let raw_index = Artifact {
        path: "raw-index.json".to_string(),
        sha256: sha256(&expected_index),
        bytes: expected_index.len() as u64,
    };
    read(&raw_index)?;
    require(
        json(&report.process)?
            == json(&aggregate_process(
                &execution.invocations,
                &raw_index.sha256,
            ))?,
        "aggregate process, cleanup or raw-index commitment contradicts captured invocations",
    )?;
    let commands = execution
        .invocations
        .iter()
        .map(|i| (&i.program, &i.arguments))
        .collect::<Vec<_>>();
    require(
        report.runner.command_digest == sha256(&json(&commands)?),
        "command commitment disagrees with retained arguments",
    )?;
    if execution
        .invocations
        .iter()
        .any(|i| i.phase == "verification")
    {
        let runner: SourceManifest = formal::runner_source_inputs()
            .iter()
            .map(|(path, _)| {
                before
                    .get(*path)
                    .cloned()
                    .map(|value| ((*path).to_string(), value))
                    .ok_or_else(|| format!("collector source commitment absent: {path}"))
            })
            .collect::<AuditResult<_>>()?;
        require(
            report.runner.generation == sha256(&canonical_manifest_bytes(&runner)?),
            "collector generation is not bound to the captured compiled-source closure",
        )?;
        require(
            installed.is_some()
                && execution.cargo_metadata.is_some()
                && execution.configuration_admitted,
            "verification started without admitted bundle/configuration/metadata evidence",
        )?;
    }
    let mut inventory = Vec::new();
    let mut packages = BTreeSet::new();
    for list in &execution.native_inventory_artifacts {
        require(
            packages.insert(&list.package),
            "duplicate package inventory artifact",
        )?;
        let bytes = read(&list.artifact)?;
        match native_inventory(
            &bytes,
            &list.package,
            Path::new(&execution.repository_directory),
            before,
        ) {
            Ok(subjects) => inventory.extend(subjects),
            Err(_)
                if !report.inventory.complete
                    && !execution.lane_success
                    && !execution.inventory_rejections.is_empty() => {}
            Err(error) => return Err(error),
        }
    }
    inventory.sort();
    require(
        inventory == execution.native_subjects,
        "native inventory does not reproduce from retained bytes",
    )?;
    let selected: Vec<_> = inventory
        .iter()
        .filter(|subject| {
            execution.harness_filter.as_ref().is_none_or(|wanted| {
                subject
                    .qualified_name
                    .as_ref()
                    .is_some_and(|name| name.contains(wanted))
            })
        })
        .cloned()
        .collect();
    require(
        selected == execution.selected_subjects,
        "native selection does not reproduce from inventory and filter",
    )?;
    for subject in &execution.declared_subjects {
        require(
            before.get(&subject.file) == Some(&subject.source_digest),
            "declared subject is not bound to captured file",
        )?;
    }
    for path in &execution.explicit_inputs {
        relative_path(path)?;
        require(
            before.contains_key(path),
            "explicit input is absent from captured source closure",
        )?;
    }
    let mut package_union = BTreeSet::new();
    for (package, paths) in &execution.package_inputs {
        require(
            execution.selected_packages.contains(package),
            "source-input scope names an unselected package",
        )?;
        strings(paths, "package input paths", false)?;
        package_union.extend(paths.iter().cloned());
    }
    require(
        package_union == execution.explicit_inputs.iter().cloned().collect(),
        "package input scopes do not reproduce their captured union",
    )?;
    if report.inventory.complete {
        require(
            compare_inventories(&execution.declared_subjects, &inventory).is_empty(),
            "declared/native denominators disagree",
        )?;
        require(
            packages.into_iter().cloned().collect::<BTreeSet<_>>()
                == execution.selected_packages.iter().cloned().collect(),
            "complete inventory lacks a selected package",
        )?;
    }
    let mut replayed = Vec::new();
    let mut observed = Vec::new();
    let mut verification_packages = BTreeSet::new();
    for invocation in &execution.invocations {
        verify_command(report, invocation)?;
        let stdout = invocation.stdout.as_ref().map(&read).transpose()?;
        let stderr = invocation.stderr.as_ref().map(&read).transpose()?;
        if invocation.phase == "preflight"
            && invocation.succeeded()
            && report.provider.version != "unavailable"
        {
            native_tool_header(
                std::str::from_utf8(stdout.as_deref().ok_or("version stdout absent")?)
                    .map_err(|e| e.to_string())?,
            )?;
            require(
                report.provider.version == "0.68.0"
                    && report.provider.solver_version.as_deref() == Some("6.11.0"),
                "provider identity contradicts its version observation",
            )?;
        }
        if invocation.phase == "verification" {
            let package = invocation
                .package
                .as_ref()
                .ok_or("verification package absent")?;
            require(
                verification_packages.insert(package),
                "duplicate package verification invocation",
            )?;
            let subjects: Vec<_> = selected
                .iter()
                .filter(|s| &s.package == package)
                .cloned()
                .collect();
            let (rows, seen, _) =
                observe_invocation(&subjects, invocation, stdout.as_deref(), stderr.as_deref());
            replayed.extend(rows);
            observed.extend(seen);
        }
    }
    for subject in &selected {
        if !replayed.iter().any(|row| row.subject == *subject) {
            replayed.extend(observe(std::slice::from_ref(subject), "").0);
        }
    }
    replayed.sort_by_key(|row| row.subject.id());
    observed.sort();
    observed.dedup();
    require(
        replayed == report.unregistered_results,
        "native result interpretation does not reproduce from retained bytes",
    )?;
    require(
        observed == execution.observed_subjects,
        "observed native identities do not reproduce",
    )?;
    verify_registered(report, directory, before, &read)?;
    Ok(before.clone())
}

fn verify_snapshot(
    execution: &Execution,
    phase: &str,
    snapshot: &Snapshot,
    read: &impl Fn(&Artifact) -> AuditResult<Vec<u8>>,
) -> AuditResult<()> {
    let calls: Vec<_> = execution
        .invocations
        .iter()
        .filter(|i| i.phase == phase)
        .collect();
    if snapshot.error.is_none() {
        require(
            calls.len() == 4,
            "complete source snapshot lacks four Git observations",
        )?;
    }
    for (index, wanted) in [snapshot.revision.as_ref(), snapshot.tree.as_ref()]
        .into_iter()
        .enumerate()
    {
        if let Some(wanted) = wanted {
            let call = calls
                .get(index)
                .ok_or("Git source identity observation absent")?;
            require(
                call.succeeded(),
                "source identity borrowed a failed Git invocation",
            )?;
            let bytes = read(call.stdout.as_ref().ok_or("Git source stdout absent")?)?;
            require(
                std::str::from_utf8(&bytes)
                    .map_err(|e| e.to_string())?
                    .trim()
                    == wanted,
                "snapshot identity does not reproduce from Git stdout",
            )?;
        }
    }
    if let Some(dirty) = snapshot.dirty {
        let call = calls.get(2).ok_or("Git status observation absent")?;
        require(
            call.succeeded(),
            "source status borrowed a failed Git invocation",
        )?;
        let bytes = read(call.stdout.as_ref().ok_or("Git status stdout absent")?)?;
        require(
            dirty != bytes.is_empty(),
            "snapshot dirty flag does not reproduce from Git stdout",
        )?;
    }
    if snapshot.manifest.is_some() {
        require(
            calls.len() == 4 && calls.iter().all(|i| i.succeeded()),
            "source manifest lacks completed Git inventory",
        )?;
    }
    Ok(())
}

fn verify_command(report: &Run, invocation: &Invocation) -> AuditResult<()> {
    let execution = report.execution.as_ref().ok_or("execution absent")?;
    let root = Path::new(&execution.repository_directory);
    let deadline = &report.runner.deadline_policy;
    let expected = match invocation.phase.as_str() {
        "before" | "after" => {
            let limit = if invocation.phase == "before" {
                deadline.whole_run_seconds.saturating_mul(1000).min(10_000)
            } else {
                10_000
            };
            require(
                invocation.program == "git"
                    && invocation.package.is_none()
                    && invocation.timeout_ms <= limit
                    && Path::new(&invocation.working_directory) == root,
                "source snapshot command or deadline is outside the admitted profile",
            )?;
            let index = execution
                .invocations
                .iter()
                .filter(|i| i.phase == invocation.phase)
                .position(|i| std::ptr::eq(i, invocation))
                .ok_or("source invocation identity absent")?;
            [
                vec!["rev-parse", "HEAD"],
                vec!["rev-parse", "HEAD^{tree}"],
                vec!["status", "--porcelain=v1", "--untracked-files=normal"],
                vec![
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                ],
            ]
            .get(index)
            .ok_or("extra Git source invocation")?
            .iter()
            .map(|s| (*s).to_string())
            .collect()
        }
        "preflight" => {
            require(
                invocation.package.is_none()
                    && invocation.timeout_ms <= deadline.preflight_seconds.saturating_mul(1000)
                    && Path::new(&invocation.working_directory) == root,
                "version probe has an unadmitted deadline or directory",
            )?;
            vec!["kani".to_string(), "--version".to_string()]
        }
        "metadata" => {
            require(
                invocation.package.is_none()
                    && invocation.timeout_ms <= 30_000
                    && Path::new(&invocation.working_directory) == root,
                "Cargo metadata directory or package scope differs",
            )?;
            vec![
                "metadata".to_string(),
                "--locked".to_string(),
                "--format-version".to_string(),
                "1".to_string(),
            ]
        }
        "inventory" => {
            let package = invocation
                .package
                .as_deref()
                .ok_or("inventory package absent")?;
            require(
                execution.selected_packages.iter().any(|p| p == package),
                "unselected package inventory",
            )?;
            require(
                Path::new(&invocation.working_directory)
                    == root
                        .join("target/ripr/reports")
                        .join(&execution.artifact_directory)
                        .join(format!("list-{package}")),
                "native inventory was not requested in its fresh run directory",
            )?;
            inventory_arguments(package, root)
        }
        "verification" => {
            let package = invocation
                .package
                .as_deref()
                .ok_or("verification package absent")?;
            require(
                execution.selected_packages.iter().any(|p| p == package)
                    && Path::new(&invocation.working_directory) == root,
                "unselected package or foreign verification directory",
            )?;
            let mut args = verification_arguments(
                package,
                execution.harness_filter.as_deref(),
                execution.default_unwind,
                deadline.per_subject_seconds,
            );
            args.push("--verbose".to_string());
            args
        }
        _ => return Err("unknown native invocation phase".to_string()),
    };
    if !matches!(invocation.phase.as_str(), "before" | "after") {
        require(
            invocation.program == "cargo"
                && invocation.timeout_ms <= deadline.whole_run_seconds.saturating_mul(1000),
            "native invocation program or timeout is outside the admitted profile",
        )?;
    }
    require(
        invocation.arguments == expected,
        "native invocation argv is outside the admitted profile",
    )
}

fn verify_registered(
    report: &Run,
    _directory: &Path,
    captured: &SourceManifest,
    read: &impl Fn(&Artifact) -> AuditResult<Vec<u8>>,
) -> AuditResult<()> {
    let execution = report.execution.as_ref().ok_or("execution absent")?;
    if let Some(artifact) = &execution.registration {
        let bytes = read(artifact)?;
        #[cfg(feature = "formal-assurance")]
        {
            let prepared: crate::formal_registry::PreparedRegistry = parse_json(&bytes)?;
            let rows = registered_rows(&prepared, report, captured)?;
            require(
                json(&rows)? == json(&report.results)?,
                "registered results do not reproduce from native bytes and the prior registration",
            )?;
            formal::audit(
                &prepared
                    .claims
                    .iter()
                    .map(|c| c.claim.clone())
                    .collect::<Vec<_>>(),
                Some(report),
            )?;
        }
        #[cfg(not(feature = "formal-assurance"))]
        {
            let _ = (bytes, captured);
            return Err(
                "registered replay requires the maintained formal-assurance feature".to_string(),
            );
        }
    } else {
        require(
            report.results.is_empty()
                && report.inventory.expected_claim_ids.is_empty()
                && report.claim_registry_generation == "unregistered-no-assurance",
            "registered result lacks prior captured registration",
        )?;
        formal::audit(&[], Some(report))?;
    }
    Ok(())
}
