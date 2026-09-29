use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::policy::distribution::{DistributionContract, TargetContract, load_distribution_contract};
use crate::run::run_output_owned;

use super::{hex_lower, normalize_release_version, release_server_readme, sha256_file};

const PAYLOAD_SCHEMA_VERSION: u32 = 1;
const PAYLOAD_KIND: &str = "ripr_final_native_payload";
const PAYLOAD_ROOT: &str = "target/ripr/distribution";
const IDENTITY_JSON: &str = "payload-identity.json";
const IDENTITY_MARKDOWN: &str = "payload-identity.md";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FinalNativePayloadIdentity {
    pub(crate) schema_version: u32,
    pub(crate) kind: String,
    pub(crate) product: String,
    pub(crate) version: String,
    pub(crate) target: String,
    pub(crate) executable: String,
    pub(crate) source: PayloadSourceIdentity,
    pub(crate) cargo_lock_sha256: String,
    pub(crate) selected_features: Vec<String>,
    pub(crate) toolchain: PayloadToolchainIdentity,
    pub(crate) build_environment: PayloadBuildEnvironmentIdentity,
    pub(crate) files: Vec<PayloadFileIdentity>,
    pub(crate) payload_aggregate_sha256: String,
    pub(crate) native_runtime_evidence: NativeRuntimeEvidenceIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PayloadSourceIdentity {
    pub(crate) commit_sha: String,
    pub(crate) tree_sha: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PayloadToolchainIdentity {
    pub(crate) rustc_verbose_version: String,
    pub(crate) cargo_verbose_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PayloadBuildEnvironmentIdentity {
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) ci_provider: String,
    pub(crate) runner_image: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PayloadFileRole {
    Executable,
    License,
    Notice,
    NativeLibrary,
}

impl PayloadFileRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Executable => "executable",
            Self::License => "license",
            Self::Notice => "notice",
            Self::NativeLibrary => "native_library",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PayloadFileIdentity {
    pub(crate) path: String,
    pub(crate) role: PayloadFileRole,
    pub(crate) size_bytes: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeRuntimeEvidenceIdentity {
    pub(crate) state: String,
    pub(crate) source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PayloadBuildContext {
    product: String,
    version: String,
    target: String,
    executable: String,
    source: PayloadSourceIdentity,
    cargo_lock_sha256: String,
    selected_features: Vec<String>,
    toolchain: PayloadToolchainIdentity,
    build_environment: PayloadBuildEnvironmentIdentity,
    native_runtime_evidence: NativeRuntimeEvidenceIdentity,
}

#[derive(Clone, Debug)]
pub(crate) struct StagedFinalNativePayload {
    pub(crate) payload_dir: PathBuf,
    pub(crate) identity_json: PathBuf,
    pub(crate) identity_markdown: PathBuf,
    pub(crate) identity: FinalNativePayloadIdentity,
}

pub(crate) fn stage_final_native_payload(
    version: &str,
    target: &str,
    executable: &str,
    archive: &str,
) -> Result<StagedFinalNativePayload, String> {
    let version = normalize_release_version(version);
    let contract = load_distribution_contract()?;
    let target_contract = select_target(&contract, target, executable, Some(archive))?;
    let context = current_payload_context(&contract, target_contract, &version, executable)?;
    let root = payload_root(&version, target)?;
    let payload_dir = root.join("payload");
    let identity_json = root.join(IDENTITY_JSON);
    let identity_markdown = root.join(IDENTITY_MARKDOWN);

    if root.exists() {
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
    }
    fs::create_dir_all(&payload_dir)
        .map_err(|err| format!("failed to create {}: {err}", payload_dir.display()))?;

    let built_executable = Path::new("target")
        .join(target)
        .join("release")
        .join(executable);
    copy_payload_file(&built_executable, &payload_dir.join(executable))?;
    copy_payload_file(Path::new("LICENSE-MIT"), &payload_dir.join("LICENSE-MIT"))?;
    copy_payload_file(
        Path::new("LICENSE-APACHE"),
        &payload_dir.join("LICENSE-APACHE"),
    )?;
    fs::write(
        payload_dir.join("README-server.txt"),
        release_server_readme(&version),
    )
    .map_err(|err| {
        format!(
            "failed to write {}: {err}",
            payload_dir.join("README-server.txt").display()
        )
    })?;

    let files = payload_file_identities(&payload_dir, executable)?;
    let identity = identity_from_context(&context, files);
    write_identity_json(&identity_json, &identity)?;
    fs::write(
        &identity_markdown,
        render_payload_identity_markdown(&identity),
    )
    .map_err(|err| format!("failed to write {}: {err}", identity_markdown.display()))?;

    let verified = verify_final_native_payload(&payload_dir, &identity_json)?;
    if verified != identity {
        return Err("final native payload changed while its identity was written".to_string());
    }

    Ok(StagedFinalNativePayload {
        payload_dir,
        identity_json,
        identity_markdown,
        identity,
    })
}

pub(crate) fn verify_final_native_payload(
    payload_dir: &Path,
    identity_path: &Path,
) -> Result<FinalNativePayloadIdentity, String> {
    let identity = read_identity_json(identity_path)?;
    validate_identity_shape(&identity)?;

    let contract = load_distribution_contract()?;
    let target_contract = select_target(
        &contract,
        &identity.target,
        &identity.executable,
        None,
    )?;
    let context = current_payload_context(
        &contract,
        target_contract,
        &identity.version,
        &identity.executable,
    )?;
    validate_identity_context(&identity, &context)?;
    verify_payload_files(payload_dir, &identity)?;
    Ok(identity)
}

pub(crate) fn payload_identity_sha256(staged: &StagedFinalNativePayload) -> Result<String, String> {
    sha256_file(&staged.identity_json)
}

fn payload_root(version: &str, target: &str) -> Result<PathBuf, String> {
    validate_component("version", version)?;
    validate_component("target", target)?;
    Ok(Path::new(PAYLOAD_ROOT)
        .join(format!("v{version}"))
        .join(target))
}

fn select_target<'a>(
    contract: &'a DistributionContract,
    target: &str,
    executable: &str,
    archive: Option<&str>,
) -> Result<&'a TargetContract, String> {
    let target_contract = contract
        .target
        .iter()
        .find(|candidate| candidate.rust_target == target)
        .ok_or_else(|| format!("target `{target}` is absent from policy/distribution.toml"))?;
    if target_contract.executable != executable {
        return Err(format!(
            "target `{target}` requires executable `{}`, got `{executable}`",
            target_contract.executable
        ));
    }
    if let Some(archive) = archive
        && target_contract.archive != archive
    {
        return Err(format!(
            "target `{target}` requires archive `{}`, got `{archive}`",
            target_contract.archive
        ));
    }
    Ok(target_contract)
}

fn current_payload_context(
    contract: &DistributionContract,
    target: &TargetContract,
    version: &str,
    executable: &str,
) -> Result<PayloadBuildContext, String> {
    let workspace_version = workspace_version()?;
    if version != workspace_version {
        return Err(format!(
            "payload version `{version}` does not match workspace version `{workspace_version}`"
        ));
    }
    ensure_clean_tracked_source()?;

    let mut selected_features = contract.product.features.clone();
    selected_features.sort();
    selected_features.dedup();
    Ok(PayloadBuildContext {
        product: contract.product.name.clone(),
        version: version.to_string(),
        target: target.rust_target.clone(),
        executable: executable.to_string(),
        source: PayloadSourceIdentity {
            commit_sha: git_rev_parse("HEAD")?,
            tree_sha: git_rev_parse("HEAD^{tree}")?,
        },
        cargo_lock_sha256: sha256_file(Path::new("Cargo.lock"))?,
        selected_features,
        toolchain: PayloadToolchainIdentity {
            rustc_verbose_version: checked_output("rustc", &["-vV"], "rustc identity")?,
            cargo_verbose_version: checked_output("cargo", &["-vV"], "cargo identity")?,
        },
        build_environment: current_build_environment(),
        native_runtime_evidence: NativeRuntimeEvidenceIdentity {
            state: target.compatibility_state.clone(),
            source: format!("EffortlessMetrics/ripr-swarm#{}", target.qualification_issue),
        },
    })
}

fn workspace_version() -> Result<String, String> {
    let path = Path::new("Cargo.toml");
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let value: toml::Value = toml::from_str(&text)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Cargo.toml: missing [workspace.package].version".to_string())
}

fn ensure_clean_tracked_source() -> Result<(), String> {
    let status = checked_output(
        "git",
        &["status", "--porcelain=v1", "--untracked-files=no"],
        "tracked source status",
    )?;
    if status.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "tracked source is dirty; final native payload identity requires committed bytes: {status}"
        ))
    }
}

fn git_rev_parse(revision: &str) -> Result<String, String> {
    let value = checked_output("git", &["rev-parse", revision], "git source identity")?;
    if is_lower_hex(&value, 40) {
        Ok(value)
    } else {
        Err(format!(
            "git rev-parse {revision} returned invalid object identity `{value}`"
        ))
    }
}

fn checked_output(program: &str, args: &[&str], label: &str) -> Result<String, String> {
    let owned = args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>();
    let output = run_output_owned(program, &owned)?;
    let output = output.replace("\r\n", "\n").trim().to_string();
    if output.is_empty() && program != "git" {
        Err(format!("{label} produced empty output"))
    } else {
        Ok(output)
    }
}

fn current_build_environment() -> PayloadBuildEnvironmentIdentity {
    let os = nonempty_env("RUNNER_OS").unwrap_or_else(|| std::env::consts::OS.to_string());
    let arch = nonempty_env("RUNNER_ARCH").unwrap_or_else(|| std::env::consts::ARCH.to_string());
    let ci_provider = if std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
        "github-actions".to_string()
    } else {
        "local".to_string()
    };
    let runner_image = match (nonempty_env("ImageOS"), nonempty_env("ImageVersion")) {
        (Some(os), Some(version)) => Some(format!("{os}@{version}")),
        (Some(os), None) => Some(os),
        (None, Some(version)) => Some(version),
        (None, None) => None,
    };
    PayloadBuildEnvironmentIdentity {
        os,
        arch,
        ci_provider,
        runner_image,
    }
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn identity_from_context(
    context: &PayloadBuildContext,
    files: Vec<PayloadFileIdentity>,
) -> FinalNativePayloadIdentity {
    let payload_aggregate_sha256 = aggregate_payload_digest(&files);
    FinalNativePayloadIdentity {
        schema_version: PAYLOAD_SCHEMA_VERSION,
        kind: PAYLOAD_KIND.to_string(),
        product: context.product.clone(),
        version: context.version.clone(),
        target: context.target.clone(),
        executable: context.executable.clone(),
        source: context.source.clone(),
        cargo_lock_sha256: context.cargo_lock_sha256.clone(),
        selected_features: context.selected_features.clone(),
        toolchain: context.toolchain.clone(),
        build_environment: context.build_environment.clone(),
        files,
        payload_aggregate_sha256,
        native_runtime_evidence: context.native_runtime_evidence.clone(),
    }
}

fn payload_file_identities(
    payload_dir: &Path,
    executable: &str,
) -> Result<Vec<PayloadFileIdentity>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(payload_dir)
        .map_err(|err| format!("failed to read {}: {err}", payload_dir.display()))?
    {
        let path = entry
            .map_err(|err| format!("failed to read entry under {}: {err}", payload_dir.display()))?
            .path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "final native payload must be a flat set of regular files; found `{}`",
                path.display()
            ));
        }
        let relative = path
            .strip_prefix(payload_dir)
            .map_err(|err| format!("failed to relativize {}: {err}", path.display()))?;
        let relative = portable_flat_path(relative)?;
        files.push(PayloadFileIdentity {
            role: role_for_path(&relative, executable)?,
            path: relative,
            size_bytes: metadata.len(),
            sha256: sha256_file(&path)?,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    if files.is_empty() {
        return Err(format!(
            "final native payload directory is empty: {}",
            payload_dir.display()
        ));
    }
    Ok(files)
}

fn verify_payload_files(
    payload_dir: &Path,
    identity: &FinalNativePayloadIdentity,
) -> Result<(), String> {
    let actual = payload_file_identities(payload_dir, &identity.executable)?;
    let expected_paths = identity
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    let actual_paths = actual
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    if actual_paths != expected_paths {
        return Err(format!(
            "payload inventory mismatch: expected {expected_paths:?}, got {actual_paths:?}"
        ));
    }
    for (expected, actual) in identity.files.iter().zip(&actual) {
        if expected != actual {
            return Err(format!(
                "payload file identity mismatch for `{}`: expected {expected:?}, got {actual:?}",
                expected.path
            ));
        }
    }
    let aggregate = aggregate_payload_digest(&actual);
    if aggregate != identity.payload_aggregate_sha256 {
        return Err(format!(
            "payload aggregate SHA-256 mismatch: expected {}, got {aggregate}",
            identity.payload_aggregate_sha256
        ));
    }
    Ok(())
}

fn validate_identity_shape(identity: &FinalNativePayloadIdentity) -> Result<(), String> {
    let mut violations = Vec::new();
    if identity.schema_version != PAYLOAD_SCHEMA_VERSION {
        violations.push(format!(
            "schema_version must be {PAYLOAD_SCHEMA_VERSION}, got {}",
            identity.schema_version
        ));
    }
    if identity.kind != PAYLOAD_KIND {
        violations.push(format!("kind must be `{PAYLOAD_KIND}`, got `{}`", identity.kind));
    }
    if identity.product != "ripr" {
        violations.push(format!("product must be `ripr`, got `{}`", identity.product));
    }
    if let Err(error) = validate_component("version", &identity.version) {
        violations.push(error);
    }
    if let Err(error) = validate_component("target", &identity.target) {
        violations.push(error);
    }
    if let Err(error) = validate_component("executable", &identity.executable) {
        violations.push(error);
    }
    if !is_lower_hex(&identity.source.commit_sha, 40) {
        violations.push("source.commit_sha must be 40 lowercase hexadecimal characters".to_string());
    }
    if !is_lower_hex(&identity.source.tree_sha, 40) {
        violations.push("source.tree_sha must be 40 lowercase hexadecimal characters".to_string());
    }
    if !is_lower_hex(&identity.cargo_lock_sha256, 64) {
        violations.push("cargo_lock_sha256 must be 64 lowercase hexadecimal characters".to_string());
    }
    if !is_lower_hex(&identity.payload_aggregate_sha256, 64) {
        violations.push(
            "payload_aggregate_sha256 must be 64 lowercase hexadecimal characters".to_string(),
        );
    }
    if identity.selected_features.is_empty() {
        violations.push("selected_features must not be empty".to_string());
    }
    let mut sorted_features = identity.selected_features.clone();
    sorted_features.sort();
    sorted_features.dedup();
    if sorted_features != identity.selected_features {
        violations.push("selected_features must be sorted and unique".to_string());
    }
    if identity.files.is_empty() {
        violations.push("files must not be empty".to_string());
    }
    let mut previous = None;
    let mut executable_count = 0;
    for file in &identity.files {
        if let Err(error) = validate_component("payload file path", &file.path) {
            violations.push(error);
        }
        if previous.is_some_and(|previous: &str| previous >= file.path.as_str()) {
            violations.push("files must be strictly sorted by unique path".to_string());
        }
        previous = Some(file.path.as_str());
        if !is_lower_hex(&file.sha256, 64) {
            violations.push(format!(
                "payload file `{}` SHA-256 must be 64 lowercase hexadecimal characters",
                file.path
            ));
        }
        if file.role == PayloadFileRole::Executable {
            executable_count += 1;
            if file.path != identity.executable {
                violations.push(format!(
                    "executable role path `{}` does not match executable `{}`",
                    file.path, identity.executable
                ));
            }
        }
    }
    if executable_count != 1 {
        violations.push(format!(
            "files must contain exactly one executable role, found {executable_count}"
        ));
    }
    for required in [
        identity.executable.as_str(),
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "README-server.txt",
    ] {
        if !identity.files.iter().any(|file| file.path == required) {
            violations.push(format!("files are missing required payload entry `{required}`"));
        }
    }
    if identity.toolchain.rustc_verbose_version.trim().is_empty() {
        violations.push("toolchain.rustc_verbose_version must not be empty".to_string());
    }
    if identity.toolchain.cargo_verbose_version.trim().is_empty() {
        violations.push("toolchain.cargo_verbose_version must not be empty".to_string());
    }
    if identity.build_environment.os.trim().is_empty()
        || identity.build_environment.arch.trim().is_empty()
        || identity.build_environment.ci_provider.trim().is_empty()
    {
        violations.push("build_environment os, arch and ci_provider must not be empty".to_string());
    }
    if identity.native_runtime_evidence.state.trim().is_empty()
        || identity.native_runtime_evidence.source.trim().is_empty()
    {
        violations.push("native_runtime_evidence state and source must not be empty".to_string());
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "invalid final native payload identity:\n- {}",
            violations.join("\n- ")
        ))
    }
}

fn validate_identity_context(
    identity: &FinalNativePayloadIdentity,
    context: &PayloadBuildContext,
) -> Result<(), String> {
    let mut violations = Vec::new();
    compare_field("product", &identity.product, &context.product, &mut violations);
    compare_field("version", &identity.version, &context.version, &mut violations);
    compare_field("target", &identity.target, &context.target, &mut violations);
    compare_field(
        "executable",
        &identity.executable,
        &context.executable,
        &mut violations,
    );
    if identity.source != context.source {
        violations.push(format!(
            "source identity is stale: expected {:?}, got {:?}",
            context.source, identity.source
        ));
    }
    compare_field(
        "cargo_lock_sha256",
        &identity.cargo_lock_sha256,
        &context.cargo_lock_sha256,
        &mut violations,
    );
    if identity.selected_features != context.selected_features {
        violations.push(format!(
            "selected_features are stale: expected {:?}, got {:?}",
            context.selected_features, identity.selected_features
        ));
    }
    if identity.toolchain != context.toolchain {
        violations.push("toolchain identity is stale".to_string());
    }
    if identity.build_environment != context.build_environment {
        violations.push(format!(
            "build_environment identity is stale: expected {:?}, got {:?}",
            context.build_environment, identity.build_environment
        ));
    }
    if identity.native_runtime_evidence != context.native_runtime_evidence {
        violations.push(format!(
            "native_runtime_evidence is stale: expected {:?}, got {:?}",
            context.native_runtime_evidence, identity.native_runtime_evidence
        ));
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "final native payload context mismatch:\n- {}",
            violations.join("\n- ")
        ))
    }
}

fn compare_field(field: &str, actual: &str, expected: &str, violations: &mut Vec<String>) {
    if actual != expected {
        violations.push(format!(
            "{field} is stale: expected `{expected}`, got `{actual}`"
        ));
    }
}

fn aggregate_payload_digest(files: &[PayloadFileIdentity]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hash_field(&mut hasher, file.path.as_bytes());
        hash_field(&mut hasher, file.role.as_str().as_bytes());
        hash_field(&mut hasher, file.size_bytes.to_string().as_bytes());
        hash_field(&mut hasher, file.sha256.as_bytes());
    }
    hex_lower(&hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(value.len().to_le_bytes());
    hasher.update(value);
}

fn role_for_path(path: &str, executable: &str) -> Result<PayloadFileRole, String> {
    if path == executable {
        Ok(PayloadFileRole::Executable)
    } else if matches!(path, "LICENSE-MIT" | "LICENSE-APACHE") {
        Ok(PayloadFileRole::License)
    } else if path == "README-server.txt" {
        Ok(PayloadFileRole::Notice)
    } else if path.ends_with(".dll")
        || path.ends_with(".dylib")
        || path.ends_with(".so")
        || path.contains(".so.")
    {
        Ok(PayloadFileRole::NativeLibrary)
    } else {
        Err(format!(
            "unclassified final native payload file `{path}`; add an explicit portable role before packaging it"
        ))
    }
}

fn copy_payload_file(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|err| format!("failed to stat {}: {err}", source.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "final native payload source must be a regular file: {}",
            source.display()
        ));
    }
    fs::copy(source, destination).map_err(|err| {
        format!(
            "failed to copy {} to {}: {err}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn write_identity_json(
    path: &Path,
    identity: &FinalNativePayloadIdentity,
) -> Result<(), String> {
    let rendered = serde_json::to_string_pretty(identity)
        .map_err(|err| format!("failed to render final native payload identity: {err}"))?;
    fs::write(path, format!("{rendered}\n"))
        .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn read_identity_json(path: &Path) -> Result<FinalNativePayloadIdentity, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))
}

fn render_payload_identity_markdown(identity: &FinalNativePayloadIdentity) -> String {
    let mut output = format!(
        "# Final native payload identity\n\n- schema version: {}\n- kind: `{}`\n- product: `{}`\n- version: `{}`\n- target: `{}`\n- executable: `{}`\n- source commit: `{}`\n- source tree: `{}`\n- Cargo.lock SHA-256: `{}`\n- selected features: `{}`\n- build OS/arch: `{}` / `{}`\n- CI provider: `{}`\n- runner image: `{}`\n- payload aggregate SHA-256: `{}`\n- native runtime evidence: `{}` from `{}`\n\n## Payload files\n\n| Path | Role | Bytes | SHA-256 |\n| --- | --- | ---: | --- |\n",
        identity.schema_version,
        identity.kind,
        identity.product,
        identity.version,
        identity.target,
        identity.executable,
        identity.source.commit_sha,
        identity.source.tree_sha,
        identity.cargo_lock_sha256,
        identity.selected_features.join(", "),
        identity.build_environment.os,
        identity.build_environment.arch,
        identity.build_environment.ci_provider,
        identity
            .build_environment
            .runner_image
            .as_deref()
            .unwrap_or("not recorded"),
        identity.payload_aggregate_sha256,
        identity.native_runtime_evidence.state,
        identity.native_runtime_evidence.source,
    );
    for file in &identity.files {
        output.push_str(&format!(
            "| `{}` | `{}` | {} | `{}` |\n",
            file.path,
            file.role.as_str(),
            file.size_bytes,
            file.sha256
        ));
    }
    output.push_str("\n## Toolchain\n\n### rustc -vV\n\n```text\n");
    output.push_str(&identity.toolchain.rustc_verbose_version);
    output.push_str("\n```\n\n### cargo -vV\n\n```text\n");
    output.push_str(&identity.toolchain.cargo_verbose_version);
    output.push_str("\n```\n");
    output
}

fn validate_component(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value == "." || value == ".." || value.contains(['/', '\\']) {
        return Err(format!(
            "{label} must be one non-empty portable path component, got `{value}`"
        ));
    }
    Ok(())
}

fn portable_flat_path(path: &Path) -> Result<String, String> {
    let components = path.components().collect::<Vec<_>>();
    if components.len() != 1 || !matches!(components[0], Component::Normal(_)) {
        return Err(format!(
            "final native payload path must be one portable flat component: {}",
            path.display()
        ));
    }
    let value = path
        .to_str()
        .ok_or_else(|| format!("final native payload path is not UTF-8: {}", path.display()))?;
    validate_component("payload file path", value)?;
    Ok(value.to_string())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_identity_is_deterministic_and_verifies() -> Result<(), String> {
        let root = unique_temp_dir("deterministic")?;
        let payload = root.join("payload");
        write_fixture_payload(&payload)?;
        let context = fixture_context();
        let first = identity_from_context(&context, payload_file_identities(&payload, "ripr")?);
        let second = identity_from_context(&context, payload_file_identities(&payload, "ripr")?);
        assert_eq!(first, second);
        assert_eq!(
            serde_json::to_string_pretty(&first).map_err(|err| err.to_string())?,
            serde_json::to_string_pretty(&second).map_err(|err| err.to_string())?
        );
        validate_identity_shape(&first)?;
        validate_identity_context(&first, &context)?;
        verify_payload_files(&payload, &first)?;
        fs::remove_dir_all(root).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[test]
    fn payload_verifier_rejects_mutation_and_missing_notice() -> Result<(), String> {
        let root = unique_temp_dir("mutations")?;
        let payload = root.join("payload");
        write_fixture_payload(&payload)?;
        let context = fixture_context();
        let identity = identity_from_context(&context, payload_file_identities(&payload, "ripr")?);

        fs::write(payload.join("ripr"), b"changed executable").map_err(|err| err.to_string())?;
        let error = verify_payload_files(&payload, &identity).expect_err("mutation must fail");
        assert!(error.contains("payload file identity mismatch"));

        write_fixture_payload(&payload)?;
        fs::remove_file(payload.join("README-server.txt")).map_err(|err| err.to_string())?;
        let error = verify_payload_files(&payload, &identity).expect_err("missing notice must fail");
        assert!(error.contains("payload inventory mismatch"));
        fs::remove_dir_all(root).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[test]
    fn identity_parser_rejects_unknown_fields() -> Result<(), String> {
        let root = unique_temp_dir("unknown-field")?;
        let payload = root.join("payload");
        write_fixture_payload(&payload)?;
        let identity = identity_from_context(
            &fixture_context(),
            payload_file_identities(&payload, "ripr")?,
        );
        let mut value = serde_json::to_value(identity).map_err(|err| err.to_string())?;
        value
            .as_object_mut()
            .ok_or_else(|| "identity must serialize to an object".to_string())?
            .insert(
                "absolute_build_path".to_string(),
                serde_json::Value::String("/tmp/secret".to_string()),
            );
        assert!(serde_json::from_value::<FinalNativePayloadIdentity>(value).is_err());
        fs::remove_dir_all(root).map_err(|err| err.to_string())?;
        Ok(())
    }

    #[test]
    fn context_validator_rejects_stale_identity_dimensions() -> Result<(), String> {
        let root = unique_temp_dir("stale-context")?;
        let payload = root.join("payload");
        write_fixture_payload(&payload)?;
        let context = fixture_context();
        let identity = identity_from_context(&context, payload_file_identities(&payload, "ripr")?);

        let mut stale = identity.clone();
        stale.version = "9.9.9".to_string();
        assert!(validate_identity_context(&stale, &context).is_err());
        let mut stale = identity.clone();
        stale.target = "other-target".to_string();
        assert!(validate_identity_context(&stale, &context).is_err());
        let mut stale = identity.clone();
        stale.selected_features = vec!["lang-rust".to_string()];
        assert!(validate_identity_context(&stale, &context).is_err());
        let mut stale = identity.clone();
        stale.cargo_lock_sha256 = "f".repeat(64);
        assert!(validate_identity_context(&stale, &context).is_err());
        let mut stale = identity;
        stale.source.commit_sha = "b".repeat(40);
        assert!(validate_identity_context(&stale, &context).is_err());
        fs::remove_dir_all(root).map_err(|err| err.to_string())?;
        Ok(())
    }

    fn fixture_context() -> PayloadBuildContext {
        PayloadBuildContext {
            product: "ripr".to_string(),
            version: "0.11.0".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            executable: "ripr".to_string(),
            source: PayloadSourceIdentity {
                commit_sha: "a".repeat(40),
                tree_sha: "b".repeat(40),
            },
            cargo_lock_sha256: "c".repeat(64),
            selected_features: vec![
                "lang-python".to_string(),
                "lang-rust".to_string(),
                "lang-typescript".to_string(),
            ],
            toolchain: PayloadToolchainIdentity {
                rustc_verbose_version: "rustc 1.95.0\nhost: x86_64-unknown-linux-gnu".to_string(),
                cargo_verbose_version: "cargo 1.95.0\nhost: x86_64-unknown-linux-gnu".to_string(),
            },
            build_environment: PayloadBuildEnvironmentIdentity {
                os: "Linux".to_string(),
                arch: "X64".to_string(),
                ci_provider: "github-actions".to_string(),
                runner_image: Some("ubuntu22@20260901.1".to_string()),
            },
            native_runtime_evidence: NativeRuntimeEvidenceIdentity {
                state: "unqualified".to_string(),
                source: "EffortlessMetrics/ripr-swarm#4489".to_string(),
            },
        }
    }

    fn write_fixture_payload(payload: &Path) -> Result<(), String> {
        fs::create_dir_all(payload).map_err(|err| err.to_string())?;
        fs::write(payload.join("ripr"), b"binary").map_err(|err| err.to_string())?;
        fs::write(payload.join("LICENSE-MIT"), b"MIT").map_err(|err| err.to_string())?;
        fs::write(payload.join("LICENSE-APACHE"), b"Apache")
            .map_err(|err| err.to_string())?;
        fs::write(payload.join("README-server.txt"), b"ripr server 0.11.0\n")
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn unique_temp_dir(label: &str) -> Result<PathBuf, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| format!("clock failed: {err}"))?
            .as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "ripr-final-native-payload-{label}-{}-{nanos}",
            std::process::id()
        )))
    }
}
