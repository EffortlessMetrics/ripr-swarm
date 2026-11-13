//! Reconstruct supplied retained bytes without authenticating who produced them.
//! A successful replay cannot construct a live process observation capability.
use super::*;
use crate::formal_evidence::{
    self as formal,
    native_inventory::{compare_inventories, native_inventory, observe, observe_invocation},
    parse_json,
};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
const MAX_BYTES: u64 = 96 * 1024 * 1024;
const MAX_INVOCATIONS: usize = 12;

/// One immutable byte observation per path/commitment within this replay.
/// This cache is not atomic filesystem custody or an observed process result.
struct ArtifactCache<'a> {
    directory: &'a Path,
    limit: u64,
    total: Cell<u64>,
    entries: RefCell<BTreeMap<String, CachedArtifact>>,
}

struct CachedArtifact {
    commitment: Artifact,
    bytes: Arc<[u8]>,
}

impl<'a> ArtifactCache<'a> {
    fn new(directory: &'a Path, limit: u64) -> Self {
        Self {
            directory,
            limit,
            total: Cell::new(0),
            entries: RefCell::new(BTreeMap::new()),
        }
    }

    fn read(&self, artifact: &Artifact) -> AuditResult<Arc<[u8]>> {
        if let Some(previous) = self.entries.borrow().get(&artifact.path) {
            require(
                &previous.commitment == artifact,
                "one artifact path has conflicting commitments",
            )?;
            return Ok(Arc::clone(&previous.bytes));
        }
        let next = self
            .total
            .get()
            .checked_add(artifact.bytes)
            .ok_or("artifact byte count overflow")?;
        require(
            next <= self.limit,
            "retained artifacts exceed the run replay budget",
        )?;
        let bytes: Arc<[u8]> = verify_artifact(self.directory, artifact, self.limit)?.into();
        self.total.set(next);
        self.entries.borrow_mut().insert(
            artifact.path.clone(),
            CachedArtifact {
                commitment: artifact.clone(),
                bytes: Arc::clone(&bytes),
            },
        );
        Ok(bytes)
    }
}
fn json<T: serde::Serialize>(value: &T) -> AuditResult<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|error| error.to_string())
}

/// A consistency result constructed only by retained-byte replay, never a live
/// execution capability or an authenticated source/lifecycle admission.
#[derive(Clone, Debug)]
pub struct RetainedReplay {
    sources: SourceManifest,
    observations: Vec<UnregisteredResult>,
    registration: Option<Arc<[u8]>>,
}

impl RetainedReplay {
    pub fn sources(&self) -> &SourceManifest {
        &self.sources
    }
    pub fn observations(&self) -> &[UnregisteredResult] {
        &self.observations
    }
    pub fn registration(&self) -> Option<&[u8]> {
        self.registration.as_deref()
    }
}

pub fn verify_execution_artifacts(
    report: &Run,
    directory: &Path,
    profile: &DeclaredKaniProfile,
) -> AuditResult<RetainedReplay> {
    profile.validate()?;
    require(
        profile == &DeclaredKaniProfile::rust_only(),
        "retained replay requires the distinct Rust-only profile",
    )?;
    require(
        report.results.is_empty() && report.inventory.expected_claim_ids.is_empty(),
        "passive native replay accepts unregistered observations, not supplied assurance rows",
    )?;
    let execution = report.execution.as_ref().ok_or("no captured execution")?;
    require(
        execution.invocations.len() <= MAX_INVOCATIONS,
        "retained replay exceeds twelve invocation records",
    )?;
    require(
        execution.native_inventory_artifacts.len() <= 1,
        "retained replay exceeds one package inventory artifact",
    )?;
    formal::audit(&[], Some(report))?;
    validate(report)?;
    let [package] = execution.selected_packages.as_slice() else {
        return Err("retained profile requires one selected package".to_string());
    };
    let harness = execution
        .harness_filter
        .as_deref()
        .ok_or("retained profile requires one exact harness")?;
    require(
        !harness.is_empty()
            && execution.configuration_policy == profile.configuration_policy
            && execution.default_unwind.map(u64::from) == Some(profile.default_unwind),
        "retained invocation profile differs from declaration",
    )?;
    require(
        report.provider.version == profile.kani_version
            && report.provider.solver_version.as_ref() == Some(&profile.cbmc_version)
            && report.toolchain.target.as_ref() == Some(&profile.target),
        "retained provider/target differs from declaration",
    )?;
    let retained = ArtifactCache::new(directory, MAX_BYTES);
    let read = |artifact: &Artifact| retained.read(artifact);
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
            package,
            profile,
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
        // The supplied runner generation is not authenticated by these bytes.
        // Executable/source correspondence and protected lifecycle stay external.
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
                    .is_some_and(|name| name == wanted)
            })
        })
        .cloned()
        .collect();
    require(
        selected.len() == 1,
        "exact retained harness is absent or ambiguous",
    )?;
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
            compare_inventories(&execution.declared_subjects, &selected).is_empty(),
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
        verify_command(report, invocation, profile)?;
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
    let registration = execution.registration.as_ref().map(&read).transpose()?;
    Ok(RetainedReplay {
        sources: before.clone(),
        observations: replayed,
        registration,
    })
}

fn verify_snapshot(
    execution: &Execution,
    phase: &str,
    snapshot: &Snapshot,
    read: &impl Fn(&Artifact) -> AuditResult<Arc<[u8]>>,
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

fn verify_command(
    report: &Run,
    invocation: &Invocation,
    profile: &DeclaredKaniProfile,
) -> AuditResult<()> {
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
            profile.metadata_arguments(
                execution
                    .selected_packages
                    .first()
                    .ok_or("metadata package absent")?,
                root,
            )
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
            profile.inventory_arguments(package, root)
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
            profile.verification_arguments(
                package,
                execution
                    .harness_filter
                    .as_deref()
                    .ok_or("exact harness absent")?,
                deadline.per_subject_seconds,
            )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    #[test]
    fn artifact_cache_reads_once_and_refuses_conflicts_and_total_budget_excess() -> AuditResult<()>
    {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct Temporary(std::path::PathBuf);
        impl Drop for Temporary {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let directory = parent.join(format!(
            "native-replay-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let directory = Temporary(directory);
        fs::write(directory.0.join("first"), b"abc").map_err(|error| error.to_string())?;
        let artifact = Artifact {
            path: "first".to_string(),
            sha256: sha256(b"abc"),
            bytes: 3,
        };
        let cache = ArtifactCache::new(&directory.0, 4);
        let first = cache.read(&artifact)?;
        fs::write(directory.0.join("first"), b"def").map_err(|error| error.to_string())?;
        let repeated = cache.read(&artifact)?;
        assert!(Arc::ptr_eq(&first, &repeated));
        assert_eq!(repeated.as_ref(), b"abc");
        assert_eq!(cache.total.get(), 3);
        let conflicting = Artifact {
            sha256: sha256(b"def"),
            ..artifact
        };
        let error = cache
            .read(&conflicting)
            .err()
            .ok_or("expected commitment conflict")?;
        assert!(error.contains("conflicting commitments"), "{error}");
        let second = Artifact {
            path: "second".to_string(),
            sha256: sha256(b"ab"),
            bytes: 2,
        };
        let error = cache
            .read(&second)
            .err()
            .ok_or("expected total byte refusal")?;
        assert!(error.contains("replay budget"), "{error}");
        Ok(())
    }
}
