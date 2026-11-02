//! Non-executing Kani import (#10689): native outcomes, byte matching and
//! provenance stay independent, without static promotion or receipt issuance.

mod native;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::external_verification::{
    KANI_BINDING_SCHEMA, KANI_REPORT_SCHEMA, KaniInvocationContext, KaniSelectedHarness,
    KaniSourceFile, SourceFileRole,
};
use crate::domain::external_verification_native::NativeRun;
use crate::provider_contract::{RiprRepositorySnapshotV1, RiprSourceViewV1};

pub(crate) const MAX_NATIVE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_BINDING_BYTES: usize = 2 * 1024 * 1024;
const MAX_SOURCE_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_SOURCE_FILES: usize = 4096;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct KaniBinding {
    schema_version: String,
    repository: RiprRepositorySnapshotV1,
    run_id: String,
    artifact_sha256: String,
    source_files: Vec<KaniSourceFile>,
    context: KaniInvocationContext,
    selected_harnesses: Vec<KaniSelectedHarness>,
}

/// Original native fields stay beside, and separate from, typed observations.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct NativeDocument {
    #[serde(flatten)]
    pub(crate) run: NativeRun,
    pub(crate) document: serde_json::Value,
}

/// Matching listed files neither authenticates the producer nor establishes the
/// completeness of its dependency inventory or the freshness of its invocation.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct KaniReport {
    pub(crate) schema_version: String,
    pub(crate) adapter_version: String,
    pub(crate) adapter_profile: String,
    pub(crate) provenance: &'static str,
    pub(crate) source_binding_state: &'static str,
    pub(crate) checked_root: String,
    #[serde(skip)]
    pub(crate) checked_source_paths: Vec<PathBuf>,
    pub(crate) artifact_sha256: String,
    pub(crate) binding_sha256: String,
    binding: KaniBinding,
    pub(crate) native: NativeDocument,
    pub(crate) limitations: Vec<String>,
}

pub(crate) fn import_kani_report(
    root: &Path,
    native_json: &str,
    binding_json: &str,
) -> Result<KaniReport, String> {
    if native_json.len() > MAX_NATIVE_BYTES || binding_json.len() > MAX_BINDING_BYTES {
        return Err(
            "Kani import exceeds the 16 MiB native or 2 MiB binding limit; see `ripr help`".into(),
        );
    }
    let binding: KaniBinding = serde_json::from_str(binding_json)
        .map_err(|err| format!("invalid Kani binding JSON: {err}; see `ripr help`"))?;
    if binding.schema_version != KANI_BINDING_SCHEMA {
        return Err(format!(
            "unsupported Kani binding schema; expected {KANI_BINDING_SCHEMA}; see `ripr help`"
        ));
    }
    binding
        .repository
        .validate()
        .map_err(|err| format!("invalid Kani repository binding: {err}; see `ripr help`"))?;
    if binding.repository.source_view != RiprSourceViewV1::CapturedSourceSet {
        return Err("Kani import currently requires a captured_source_set binding; Git trees, index and worktree identity resolution are not supported; see `ripr help`".into());
    }
    require_text("run_id", &binding.run_id)?;
    require_digest("artifact_sha256", &binding.artifact_sha256)?;
    if sha256(native_json.as_bytes()) != binding.artifact_sha256 {
        return Err(
            "Kani artifact digest mismatch; supply the exact export named by the binding".into(),
        );
    }
    let native = native::parse_native(native_json)?;
    validate_context(&binding, &native.run)?;
    let root = root
        .canonicalize()
        .map_err(|err| format!("resolve Kani source root {}: {err}", root.display()))?;
    if !root.is_dir() {
        return Err("Kani source root must be a directory; see `ripr help`".into());
    }
    let checked_source_paths = validate_sources(&root, &binding, &native.run)?;
    Ok(KaniReport {
        schema_version: KANI_REPORT_SCHEMA.into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        adapter_profile: "kani-export-0.1.0@744b3e84d6eb68fd5456826997568666791bd355".into(),
        provenance: "external_unattested",
        source_binding_state: "listed_files_match",
        checked_root: root.to_str().ok_or("Kani source root is not UTF-8; see `ripr help`")?.to_owned(),
        checked_source_paths,
        artifact_sha256: binding.artifact_sha256.clone(),
        binding_sha256: sha256(binding_json.as_bytes()),
        binding,
        native,
        limitations: vec![
            "External, unattested observation: digests do not authenticate the verifier or establish that it consumed these files or this invocation context.".into(),
            "Only listed files were checked, individually at import time; repository identity, complete dependencies, atomic snapshot capture and run freshness are not established.".into(),
            "Full qualification of this development adapter profile remains outstanding.".into(),
            "Successful check counts are available at harness scope, but the export does not provide a complete inventory of their property identities or a functional-versus-safety classification.".into(),
            "Native success does not establish non-vacuity, specification adequacy, separately discharged stub contracts, or correctness outside the recorded checks, assumptions and bounds.".into(),
            "This import does not execute Kani, change static exposure, create a repair receipt, or authorize a merge.".into(),
        ],
    })
}

fn validate_context(binding: &KaniBinding, native: &NativeRun) -> Result<(), String> {
    require_text("context.target", &binding.context.target)?;
    require_text("context.toolchain", &binding.context.toolchain)?;
    require_digest(
        "context.environment_sha256",
        &binding.context.environment_sha256,
    )?;
    if binding.context.target != native.target {
        return Err("Kani target differs from the bound invocation context".into());
    }
    if binding.context.invocation.is_empty() {
        return Err(
            "Kani binding must record descriptive invocation tokens; see `ripr help`".into(),
        );
    }
    for token in &binding.context.invocation {
        require_text("context.invocation token", token)?;
    }
    let mut features = BTreeSet::new();
    for feature in &binding.context.cargo_features {
        require_text("context.cargo_features entry", feature)?;
        if !features.insert(feature) {
            return Err("duplicate Kani Cargo feature binding".into());
        }
    }
    let mut selected = BTreeSet::new();
    for harness in &binding.selected_harnesses {
        require_text("selected_harnesses.crate_name", &harness.crate_name)?;
        require_text("selected_harnesses.name", &harness.name)?;
        if !selected.insert(harness) {
            return Err("duplicate selected Kani harness binding".into());
        }
    }
    if selected.len() as u64 != native.matched_count {
        return Err("Kani selected harness count differs from the binding".into());
    }
    for harness in &native.harnesses {
        let key = KaniSelectedHarness {
            crate_name: harness.crate_name.clone(),
            name: harness.name.clone(),
        };
        if !selected.contains(&key) {
            return Err(format!(
                "Kani returned an unbound harness {:?}::{:?}",
                harness.crate_name, harness.name
            ));
        }
    }
    Ok(())
}

fn validate_sources(
    root: &Path,
    binding: &KaniBinding,
    native: &NativeRun,
) -> Result<Vec<PathBuf>, String> {
    if binding.source_files.is_empty() || binding.source_files.len() > MAX_SOURCE_FILES {
        return Err(
            "Kani binding must list between 1 and 4096 source files; see `ripr help`".into(),
        );
    }
    let mut files = BTreeMap::new();
    for file in &binding.source_files {
        validate_source_path(&file.path)?;
        require_digest("source_files.sha256", &file.sha256)?;
        if files.insert(file.path.as_str(), file).is_some() {
            return Err(format!("duplicate Kani source path {}", file.path));
        }
        if file.roles.is_empty()
            || file
                .roles
                .iter()
                .enumerate()
                .any(|(index, role)| file.roles.iter().skip(index + 1).any(|other| role == other))
        {
            return Err(format!(
                "Kani source {} needs nonempty unique roles",
                file.path
            ));
        }
    }
    for role in [SourceFileRole::Implementation, SourceFileRole::Harness] {
        if !files.values().any(|file| file.roles.contains(&role)) {
            return Err(
                "Kani binding must identify implementation and harness source files; see `ripr help`".into(),
            );
        }
    }
    let manifest = files
        .values()
        .map(|file| format!("{}\t{}\n", file.path, file.sha256))
        .collect::<String>();
    if sha256(manifest.as_bytes()) != binding.repository.source_digest {
        return Err("Kani source inventory digest differs from the captured snapshot".into());
    }
    for harness in &native.harnesses {
        validate_source_path(&harness.file)?;
        let file = files.get(harness.file.as_str()).ok_or_else(|| {
            format!(
                "Kani harness source {} is absent from the binding",
                harness.file
            )
        })?;
        if !file.roles.contains(&SourceFileRole::Harness) {
            return Err(format!(
                "Kani source {} lacks the harness role",
                harness.file
            ));
        }
    }
    let mut total_bytes = 0_u64;
    let mut checked_paths = BTreeSet::new();
    for file in files.values() {
        let path = regular_source_path(root, &file.path)?;
        let bytes = crate::bounded_input::read_with_limit(&path, MAX_SOURCE_FILE_BYTES)
            .map_err(|err| format!("read Kani source {}: {err}", file.path))?;
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        if total_bytes > MAX_SOURCE_BYTES {
            return Err(
                "Kani source inventory exceeds the 256 MiB total limit; see `ripr help`".into(),
            );
        }
        if sha256(&bytes) != file.sha256 {
            return Err(format!(
                "Kani source digest mismatch for {}; preserve the historical export and recapture matching source evidence",
                file.path
            ));
        }
        if !checked_paths.insert(path) {
            return Err("Kani source inventory contains aliases of the same path".into());
        }
    }
    Ok(checked_paths.into_iter().collect())
}

fn validate_source_path(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.chars().any(char::is_control)
        || value.contains(['\\', ':'])
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!(
            "Kani source path {value:?} must be portable, relative and free of traversal; see `ripr help`"
        ));
    }
    Ok(())
}

fn regular_source_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|err| format!("inspect Kani source {}: {err}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Kani source symlinks are unsupported: {}; see `ripr help`",
                path.display()
            ));
        }
    }
    let resolved = path
        .canonicalize()
        .map_err(|err| format!("resolve Kani source {}: {err}", path.display()))?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err(format!(
            "Kani source must be a regular file under root: {relative}; see `ripr help`"
        ));
    }
    Ok(resolved)
}

fn require_text(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        Err(format!(
            "Kani {field} must be nonempty text without control characters; see `ripr help`"
        ))
    } else {
        Ok(())
    }
}

fn require_digest(field: &str, value: &str) -> Result<(), String> {
    if value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }) {
        Ok(())
    } else {
        Err(format!(
            "Kani {field} must be sha256:<64 lowercase hexadecimal characters>; see `ripr help`"
        ))
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use tests::{test_inputs, tests_root};
