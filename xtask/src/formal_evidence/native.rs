//! Captured native execution is separate from registered proposition assurance.
use super::{AuditResult, Run, digest, require, strings};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path};

mod profile;
pub use profile::{
    ConfigurationSnapshot, InstalledToolchain, validate_cargo_metadata, validate_configuration,
    validate_installed_toolchain,
};
pub use profile::{DeclaredKaniProfile, aggregate_process, native_tool_header};
mod replay;
pub use replay::{RetainedReplay, verify_execution_artifacts};

/// Sorted repository-relative file commitments. This is a captured input
/// manifest, not a claim of production-body equivalence or producer identity.
pub type SourceManifest = BTreeMap<String, String>;

/// Bounded regular-file bytes; closure callers separately enforce repository
/// containment and reject symlinks along every source path component.
pub fn read_file_bounded(path: &Path, limit: u64) -> AuditResult<Vec<u8>> {
    require(
        fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .is_file(),
        "input is not a regular file",
    )?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(bytes.len() as u64 <= limit, "input exceeds its byte budget")?;
    Ok(bytes)
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub fn canonical_manifest_bytes(manifest: &SourceManifest) -> AuditResult<Vec<u8>> {
    serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())
}

pub fn parse_source_manifest(bytes: &[u8]) -> AuditResult<SourceManifest> {
    let manifest: SourceManifest = super::parse_json(bytes)?;
    for (path, value) in &manifest {
        relative_path(path)?;
        digest(value, "source file digest")?;
    }
    Ok(manifest)
}

pub fn source_is_current(before: &Snapshot, after: &Snapshot) -> bool {
    before.error.is_none()
        && after.error.is_none()
        && before.revision.is_some()
        && before.tree.is_some()
        && before.dirty.is_some()
        && before.revision == after.revision
        && before.tree == after.tree
        && before.dirty == after.dirty
        && before.manifest.is_some()
        && before.manifest.as_ref().map(|a| &a.sha256) == after.manifest.as_ref().map(|a| &a.sha256)
        && before.file_count == after.file_count
}

/// Verify locally supplied retained bytes against their recorded commitment.
/// Symlinks at any component are refused. This does not authenticate who wrote
/// the record, prove that a verifier ran, or attest a current source checkout.
pub fn verify_artifact(directory: &Path, value: &Artifact, limit: u64) -> AuditResult<Vec<u8>> {
    artifact(value)?;
    require(value.bytes <= limit, "artifact exceeds its read budget")?;
    let root = fs::canonicalize(directory).map_err(|e| e.to_string())?;
    let mut path = root.clone();
    for component in Path::new(&value.path).components() {
        let Component::Normal(part) = component else {
            return Err("artifact path has a non-relative component".to_string());
        };
        path.push(part);
        require(
            !fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink(),
            "raw artifact path contains a symlink",
        )?;
    }
    require(
        fs::canonicalize(&path)
            .map_err(|e| e.to_string())?
            .starts_with(&root)
            && fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .is_file(),
        "raw artifact escaped its run directory or is not regular",
    )?;
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|e| e.to_string())?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(
        value.bytes == bytes.len() as u64 && value.sha256 == sha256(&bytes),
        "raw artifact commitment mismatch",
    )?;
    Ok(bytes)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    #[serde(deserialize_with = "super::required_nullable")]
    pub revision: Option<String>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub tree: Option<String>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub dirty: Option<bool>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub manifest: Option<Artifact>,
    pub file_count: usize,
    #[serde(deserialize_with = "super::required_nullable")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    pub package: String,
    pub file: String,
    pub name: String,
    #[serde(deserialize_with = "super::required_nullable")]
    pub qualified_name: Option<String>,
    pub source_digest: String,
}

impl Subject {
    pub fn id(&self) -> String {
        format!(
            "{}:{}::{}",
            self.package,
            self.file,
            self.qualified_name.as_deref().unwrap_or(&self.name)
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObservationStatus {
    Verified,
    Failed,
    NotEstablished,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CoverObservation {
    pub total: u64,
    pub satisfied: u64,
    pub undetermined: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeProperty {
    pub name: String,
    pub status: String,
    #[serde(deserialize_with = "super::required_nullable")]
    pub description: Option<String>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub location: Option<String>,
}
pub fn has_counterexample_properties(properties: &[NativeProperty]) -> bool {
    properties.iter().any(|p| p.status == "FAILURE")
        && !properties
            .iter()
            .any(|p| p.status == "FAILURE" && p.name.contains(".unwind."))
}

pub type CoverProperty = NativeProperty;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UnregisteredResult {
    pub subject: Subject,
    pub status: ObservationStatus,
    pub native_verdict: super::Verdict,
    pub detail: String,
    pub kani_time: String,
    #[serde(deserialize_with = "super::required_nullable")]
    pub duration_ms: Option<u64>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub covers: Option<CoverObservation>,
    pub cover_properties: Vec<CoverProperty>,
    pub properties: Vec<NativeProperty>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub raw_block_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub phase: String,
    #[serde(deserialize_with = "super::required_nullable")]
    pub package: Option<String>,
    pub program: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub timeout_ms: u64,
    pub stream_limit_bytes: usize,
    #[serde(deserialize_with = "super::required_nullable")]
    pub duration_ms: Option<u64>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub exit_code: Option<i32>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub native_status: Option<String>,
    pub timed_out: bool,
    pub capture_complete: bool,
    #[serde(deserialize_with = "super::required_nullable")]
    pub stdout: Option<Artifact>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub stderr: Option<Artifact>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub error: Option<String>,
}
impl Invocation {
    pub fn succeeded(&self) -> bool {
        self.capture_complete
            && self.exit_code == Some(0)
            && !self.timed_out
            && self.error.is_none()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeInventoryArtifact {
    pub package: String,
    pub artifact: Artifact,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub producer: String,
    pub producer_entrypoint: String,
    pub producer_arguments: Vec<String>,
    pub artifact_directory: String,
    pub repository_directory: String,
    pub native_inventory_artifacts: Vec<NativeInventoryArtifact>,
    pub source_before: Snapshot,
    pub source_after: Snapshot,
    pub source_current: bool,
    pub configuration_policy: String,
    pub configuration_digest: String,
    pub configuration_before: Artifact,
    pub configuration_after: Artifact,
    #[serde(deserialize_with = "super::required_nullable")]
    pub cargo_metadata: Option<Artifact>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub installed_toolchain: Option<Artifact>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub installed_toolchain_after: Option<Artifact>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub registration: Option<Artifact>,
    pub explicit_inputs: Vec<String>,
    pub package_inputs: BTreeMap<String, Vec<String>>,
    pub configuration_admitted: bool,
    pub configuration_rejections: Vec<String>,
    pub selected_packages: Vec<String>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub harness_filter: Option<String>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub default_unwind: Option<u32>,
    pub declared_subjects: Vec<Subject>,
    pub selected_subjects: Vec<Subject>,
    pub native_subjects: Vec<Subject>,
    pub observed_subjects: Vec<String>,
    pub invocations: Vec<Invocation>,
    pub inventory_rejections: Vec<String>,
    pub rejection_reasons: Vec<String>,
    pub allow_empty: bool,
    pub lane_success: bool,
    #[serde(deserialize_with = "super::required_nullable")]
    pub github_run_id: Option<String>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub github_run_attempt: Option<String>,
    #[serde(deserialize_with = "super::required_nullable")]
    pub github_job: Option<String>,
    pub non_claims: Vec<String>,
}

pub fn relative_path(value: &str) -> AuditResult<()> {
    require(
        !value.is_empty()
            && !value.starts_with('/')
            && !value.contains(['\\', ':', '\0'])
            && value
                .split('/')
                .all(|c| !c.is_empty() && c != "." && c != ".."),
        "artifact path must remain relative to its owned run directory",
    )
}
pub fn artifact(value: &Artifact) -> AuditResult<()> {
    relative_path(&value.path)?;
    digest(&value.sha256, "artifact sha256")
}

/// One document-law owner; it does not claim to have read the artifact bytes.
pub fn validate(run: &Run) -> AuditResult<()> {
    let Some(execution) = &run.execution else {
        return require(
            run.unregistered_results.is_empty(),
            "unregistered observations need an execution receipt",
        );
    };
    relative_path(&execution.artifact_directory)?;
    require(
        run.revision == execution.source_before.revision
            && run.tree == execution.source_before.tree
            && run.source_closure_digest.as_ref()
                == execution.source_before.manifest.as_ref().map(|a| &a.sha256),
        "top-level source identity disagrees with captured source",
    )?;
    require(
        execution.producer == "ripr-kani-custody-v1",
        "unsupported native capture producer",
    )?;
    require(
        matches!(
            execution.producer_entrypoint.as_str(),
            "xtask" | "formal-kani" | "formal-runner-test"
        ) && !execution.producer_arguments.is_empty(),
        "missing native producer entrypoint/arguments",
    )?;
    DeclaredKaniProfile::for_policy(&execution.configuration_policy)?;
    digest(&execution.configuration_digest, "configuration digest")?;
    artifact(&execution.configuration_before)?;
    artifact(&execution.configuration_after)?;
    require(
        execution.configuration_digest == execution.configuration_before.sha256,
        "configuration digest disagrees with its retained before artifact",
    )?;
    for value in [
        &execution.cargo_metadata,
        &execution.installed_toolchain,
        &execution.installed_toolchain_after,
        &execution.registration,
    ]
    .into_iter()
    .flatten()
    {
        artifact(value)?;
    }
    strings(&execution.selected_packages, "selected packages", true)?;
    require(
        execution
            .selected_packages
            .iter()
            .all(|p| matches!(p.as_str(), "ripr" | "ripr-contract")),
        "unsupported selected package",
    )?;
    require(
        execution.configuration_admitted == execution.configuration_rejections.is_empty(),
        "configuration admission contradicts its rejection reasons",
    )?;
    require(
        !execution.non_claims.is_empty(),
        "native execution must name its assurance boundary",
    )?;
    for snapshot in [&execution.source_before, &execution.source_after] {
        if let Some(manifest) = &snapshot.manifest {
            artifact(manifest)?;
        }
        if let Some(revision) = &snapshot.revision {
            super::git_object(revision, "snapshot revision")?;
        }
        if let Some(tree) = &snapshot.tree {
            super::git_object(tree, "snapshot tree")?;
        }
    }
    if execution.source_current {
        require(
            source_is_current(&execution.source_before, &execution.source_after)
                && execution.configuration_before.sha256 == execution.configuration_after.sha256,
            "source/configuration currentness lacks equal captured inputs",
        )?;
        require(
            execution.source_before.error.is_none()
                && execution.source_after.error.is_none()
                && execution.source_before.manifest.is_some()
                && execution.source_after.manifest.is_some()
                && execution.source_before.revision.is_some()
                && execution.source_before.tree.is_some(),
            "current source requires complete before/after capture",
        )?;
        require(
            execution.source_before.revision == execution.source_after.revision
                && execution.source_before.tree == execution.source_after.tree
                && execution.source_before.dirty == execution.source_after.dirty
                && execution.source_before.manifest.as_ref().map(|v| &v.sha256)
                    == execution.source_after.manifest.as_ref().map(|v| &v.sha256),
            "source changed during native capture",
        )?;
    }
    let mut selected = BTreeSet::new();
    for subject in &execution.selected_subjects {
        digest(&subject.source_digest, "selected source digest")?;
        require(
            subject.qualified_name.is_some(),
            "selected subject lacks native qualified identity",
        )?;
        require(
            selected.insert(subject.id()),
            "duplicate selected native subject",
        )?;
        require(
            execution.native_subjects.contains(subject),
            "selected subject absent from native inventory",
        )?;
    }
    let mut native = BTreeSet::new();
    for subject in &execution.native_subjects {
        digest(&subject.source_digest, "native source digest")?;
        relative_path(&subject.file)?;
        require(
            execution.selected_packages.contains(&subject.package)
                && subject
                    .qualified_name
                    .as_ref()
                    .is_some_and(|name| !name.is_empty()),
            "native subject is outside selected package scope or has no identity",
        )?;
        require(
            native.insert(subject.id()),
            "duplicate native inventory subject",
        )?;
    }
    let mut rows = BTreeSet::new();
    let mut complete_packages = BTreeSet::new();
    for invocation in &execution.invocations {
        if invocation.phase == "verification"
            && invocation.capture_complete
            && let Some(package) = &invocation.package
        {
            complete_packages.insert(package);
        }
    }
    for row in &run.unregistered_results {
        let id = row.subject.id();
        require(
            rows.insert(id.clone()),
            "duplicate unregistered observation",
        )?;
        require(
            execution.selected_subjects.contains(&row.subject),
            "observation identity or source differs from selected native inventory",
        )?;
        require(
            match row.status {
                ObservationStatus::Verified => row.native_verdict == super::Verdict::Verified,
                ObservationStatus::Failed => match row.native_verdict {
                    super::Verdict::Counterexample => {
                        has_counterexample_properties(&row.properties)
                    }
                    super::Verdict::UnreachableCover => row
                        .cover_properties
                        .iter()
                        .any(|p| matches!(p.status.as_str(), "UNSATISFIABLE" | "UNREACHABLE")),
                    _ => false,
                },
                ObservationStatus::NotEstablished => true,
            },
            "native observation status contradicts its typed verifier verdict",
        )?;
        if let Some(value) = &row.raw_block_digest {
            digest(value, "native block digest")?;
        }
        if row.status != ObservationStatus::NotEstablished {
            require(
                complete_packages.contains(&row.subject.package) && row.raw_block_digest.is_some(),
                "observed result lacks a captured package invocation",
            )?;
        }
        if row.status == ObservationStatus::Verified {
            let covers = row
                .covers
                .as_ref()
                .ok_or("verified observation has no covers")?;
            require(
                covers.total > 0
                    && covers.satisfied == covers.total
                    && covers.undetermined == 0
                    && row.raw_block_digest.is_some()
                    && u64::try_from(row.cover_properties.len()).ok() == Some(covers.total)
                    && row.cover_properties.iter().all(|p| p.status == "SATISFIED"),
                "verified observation lacks complete positive evidence",
            )?;
        }
        let mut properties = BTreeSet::new();
        for property in &row.properties {
            require(
                !property.name.is_empty() && properties.insert(&property.name),
                "native property identity is missing or duplicated",
            )?;
            require(
                matches!(
                    property.status.as_str(),
                    "SUCCESS"
                        | "FAILURE"
                        | "SATISFIED"
                        | "UNSATISFIABLE"
                        | "UNREACHABLE"
                        | "UNDETERMINED"
                        | "ERROR"
                ),
                "native property status is unsupported",
            )?;
        }
        require(
            row.properties
                .iter()
                .filter(|p| p.name.rsplit('.').nth(1) == Some("cover"))
                .eq(row.cover_properties.iter()),
            "native cover projection differs from its retained property set",
        )?;
        if row.status == ObservationStatus::Verified {
            require(
                row.properties
                    .iter()
                    .all(|p| matches!(p.status.as_str(), "SUCCESS" | "SATISFIED" | "UNREACHABLE")),
                "verified ordinary observation contains a failing or unknown native property",
            )?;
        }
    }
    strings(
        &execution.observed_subjects,
        "observed native subjects",
        false,
    )?;
    for invocation in &execution.invocations {
        require(
            invocation.timeout_ms > 0 && invocation.stream_limit_bytes > 0,
            "native invocation needs explicit bounds",
        )?;
        if let Some(value) = &invocation.stdout {
            artifact(value)?;
            require(
                value.bytes <= invocation.stream_limit_bytes as u64,
                "stdout exceeds its recorded capture budget",
            )?;
        }
        if let Some(value) = &invocation.stderr {
            artifact(value)?;
            require(
                value.bytes <= invocation.stream_limit_bytes as u64,
                "stderr exceeds its recorded capture budget",
            )?;
        }
        if invocation.capture_complete {
            require(
                invocation.stdout.is_some()
                    && invocation.stderr.is_some()
                    && invocation.native_status.is_some()
                    && invocation.duration_ms.is_some(),
                "complete capture lacks raw streams or native process observation",
            )?;
        }
    }
    for inventory in &execution.native_inventory_artifacts {
        artifact(&inventory.artifact)?;
    }
    if run.inventory.complete {
        require(
            !selected.is_empty() && rows == selected && execution.inventory_rejections.is_empty(),
            "complete inventory lacks its exact selected result set",
        )?;
        let observed: BTreeSet<_> = execution.observed_subjects.iter().cloned().collect();
        require(
            observed == selected,
            "complete inventory disagrees with observed native subjects",
        )?;
        require(
            execution
                .native_inventory_artifacts
                .iter()
                .map(|a| a.package.as_str())
                .collect::<BTreeSet<_>>()
                == execution
                    .selected_packages
                    .iter()
                    .map(String::as_str)
                    .collect(),
            "complete inventory lacks each selected package artifact",
        )?;
    }
    if execution.lane_success {
        for phase in ["preflight", "metadata"] {
            require(
                execution
                    .invocations
                    .iter()
                    .filter(|i| i.phase == phase)
                    .count()
                    == 1,
                "successful lane lacks one preflight or metadata invocation",
            )?;
        }
        for package in &execution.selected_packages {
            require(
                execution
                    .invocations
                    .iter()
                    .filter(|i| i.phase == "inventory" && i.package.as_ref() == Some(package))
                    .count()
                    == 1,
                "successful lane lacks one native inventory invocation per selected package",
            )?;
        }
        for package in execution
            .selected_subjects
            .iter()
            .map(|s| &s.package)
            .collect::<BTreeSet<_>>()
        {
            require(
                execution
                    .invocations
                    .iter()
                    .filter(|i| i.phase == "verification" && i.package.as_ref() == Some(package))
                    .count()
                    == 1,
                "successful lane lacks one verification invocation per selected package with subjects",
            )?;
        }
        require(
            execution.source_current
                && execution.configuration_admitted
                && execution.cargo_metadata.is_some()
                && execution.installed_toolchain.is_some()
                && execution.installed_toolchain_after.is_some()
                && run.process.cleanup_state == super::Cleanup::Complete
                && run.process.state == super::ProcessState::Completed
                && run.process.exit_code == Some(0)
                && run.inventory.complete
                && execution.rejection_reasons.is_empty()
                && run
                    .unregistered_results
                    .iter()
                    .all(|r| r.status == ObservationStatus::Verified)
                && execution.invocations.iter().all(Invocation::succeeded),
            "successful lane lacks complete current admitted native evidence",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
