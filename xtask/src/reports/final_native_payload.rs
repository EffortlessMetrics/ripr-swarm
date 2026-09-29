use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::policy::distribution::load_distribution_contract;
use crate::run::run_output;

use super::release_server::{hex_lower, release_server_readme, sha256_file};

const PAYLOAD_SCHEMA_VERSION: &str = "1.0";
const RUNTIME_EVIDENCE_STATE: &str = "unqualified";
const RUNTIME_EVIDENCE_SOURCE: &str = "issue:#4489";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FinalNativePayloadIdentity {
    pub(crate) schema_version: String,
    pub(crate) product: String,
    pub(crate) native_version: String,
    pub(crate) target: String,
    pub(crate) executable: String,
    pub(crate) candidate_commit: String,
    pub(crate) candidate_tree: String,
    pub(crate) cargo_lock_sha256: String,
    pub(crate) default_features: bool,
    pub(crate) cargo_features: Vec<String>,
    pub(crate) toolchain: ToolchainIdentity,
    pub(crate) build_environment: BuildEnvironmentIdentity,
    pub(crate) files: Vec<PayloadFileIdentity>,
    pub(crate) payload_sha256: String,
    pub(crate) native_runtime_evidence: NativeRuntimeEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ToolchainIdentity {
    pub(crate) rustc_verbose_version: String,
    pub(crate) cargo_verbose_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuildEnvironmentIdentity {
    pub(crate) provider: String,
    pub(crate) host_os: String,
    pub(crate) host_arch: String,
    pub(crate) image_os: Option<String>,
    pub(crate) image_version: Option<String>,
    pub(crate) profile: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PayloadFileIdentity {
    pub(crate) relative_path: String,
    pub(crate) role: PayloadFileRole,
    pub(crate) size: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PayloadFileRole {
    Executable,
    LicenseApache,
    LicenseMit,
    Readme,
}

impl PayloadFileRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Executable => "executable",
            Self::LicenseApache => "license_apache",
            Self::LicenseMit => "license_mit",
            Self::Readme => "readme",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeRuntimeEvidence {
    pub(crate) state: String,
    pub(crate) source: String,
}

#[derive(Debug)]
pub(crate) struct StagedNativePayload {
    pub(crate) root: PathBuf,
    pub(crate) payload_dir: PathBuf,
    pub(crate) identity_path: PathBuf,
    pub(crate) markdown_path: PathBuf,
    pub(crate) identity: FinalNativePayloadIdentity,
}

#[derive(Clone, Debug)]
struct PayloadContractView {
    product: String,
    native_version: String,
    target: String,
    executable: String,
    default_features: bool,
    cargo_features: Vec<String>,
}

#[derive(Clone, Debug)]
struct PortableBuildInputs {
    candidate_commit: String,
    candidate_tree: String,
    cargo_lock_sha256: String,
    toolchain: ToolchainIdentity,
    build_environment: BuildEnvironmentIdentity,
}

pub(crate) fn stage_final_native_payload(
    version: &str,
    target: &str,
    executable: &str,
) -> Result<StagedNativePayload, String> {
    let repository_root = Path::new(".");
    let contract = load_distribution_contract()?;
    let target_contract = contract
        .target
        .iter()
        .find(|entry| entry.rust_target == target)
        .ok_or_else(|| format!("distribution contract has no target `{target}`"))?;
    if target_contract.executable != executable {
        return Err(format!(
            "distribution target `{target}` requires executable `{}`, got `{executable}`",
            target_contract.executable
        ));
    }
    if target_contract.archive != "zip" && target_contract.archive != "tar.gz" {
        return Err(format!(
            "distribution target `{target}` has unsupported archive `{}`",
            target_contract.archive
        ));
    }

    let workspace_version = workspace_version(repository_root.join("Cargo.toml"))?;
    if version != workspace_version {
        return Err(format!(
            "payload version `{version}` does not match workspace version `{workspace_version}`"
        ));
    }

    let mut cargo_features = contract.product.features.clone();
    cargo_features.sort();
    cargo_features.dedup();
    let contract_view = PayloadContractView {
        product: contract.product.name,
        native_version: workspace_version,
        target: target.to_string(),
        executable: executable.to_string(),
        default_features: contract.product.default_features,
        cargo_features,
    };
    let inputs = collect_portable_build_inputs(repository_root)?;
    let built_executable = repository_root
        .join("target")
        .join(target)
        .join("release")
        .join(executable);
    stage_final_native_payload_at(repository_root, &built_executable, &contract_view, &inputs)
}

pub(crate) fn verify_archive_against_payload(
    asset_path: &Path,
    archive: &str,
    staged: &StagedNativePayload,
) -> Result<(), String> {
    let readback_dir = staged.root.join("archive-readback");
    if readback_dir.exists() {
        fs::remove_dir_all(&readback_dir)
            .map_err(|err| format!("failed to remove {}: {err}", readback_dir.display()))?;
    }
    fs::create_dir_all(&readback_dir)
        .map_err(|err| format!("failed to create {}: {err}", readback_dir.display()))?;

    let extraction_result = match archive {
        "zip" => extract_zip(asset_path, &readback_dir),
        "tar.gz" => extract_tar_gz(asset_path, &readback_dir),
        other => Err(format!(
            "unsupported release server archive format `{other}`"
        )),
    };
    if let Err(error) = extraction_result {
        return match fs::remove_dir_all(&readback_dir) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(format!(
                "{error}; failed to remove {} after extraction failure: {cleanup_error}",
                readback_dir.display()
            )),
        };
    }

    let verification_result = verify_payload_directory(&readback_dir, &staged.identity);
    let cleanup_result = fs::remove_dir_all(&readback_dir)
        .map_err(|err| format!("failed to remove {}: {err}", readback_dir.display()));
    verification_result?;
    cleanup_result
}

fn stage_final_native_payload_at(
    repository_root: &Path,
    built_executable: &Path,
    contract: &PayloadContractView,
    inputs: &PortableBuildInputs,
) -> Result<StagedNativePayload, String> {
    validate_contract_view(contract)?;
    validate_build_inputs(inputs)?;
    if !built_executable.is_file() {
        return Err(format!(
            "built payload executable is missing: {}",
            built_executable.display()
        ));
    }

    let root = repository_root
        .join("target")
        .join("ripr")
        .join("distribution")
        .join(format!("v{}", contract.native_version))
        .join(&contract.target);
    if root.exists() {
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
    }
    let payload_dir = root.join("payload");
    fs::create_dir_all(&payload_dir)
        .map_err(|err| format!("failed to create {}: {err}", payload_dir.display()))?;

    copy_payload_file(
        built_executable,
        &payload_dir.join(&contract.executable),
        "executable",
    )?;
    copy_payload_file(
        &repository_root.join("LICENSE-APACHE"),
        &payload_dir.join("LICENSE-APACHE"),
        "Apache license",
    )?;
    copy_payload_file(
        &repository_root.join("LICENSE-MIT"),
        &payload_dir.join("LICENSE-MIT"),
        "MIT license",
    )?;
    let readme_path = payload_dir.join("README-server.txt");
    fs::write(&readme_path, release_server_readme(&contract.native_version))
        .map_err(|err| format!("failed to write {}: {err}", readme_path.display()))?;

    let files = payload_files(&payload_dir, &contract.executable)?;
    let payload_sha256 = aggregate_payload_sha256(&files);
    let identity = FinalNativePayloadIdentity {
        schema_version: PAYLOAD_SCHEMA_VERSION.to_string(),
        product: contract.product.clone(),
        native_version: contract.native_version.clone(),
        target: contract.target.clone(),
        executable: contract.executable.clone(),
        candidate_commit: inputs.candidate_commit.clone(),
        candidate_tree: inputs.candidate_tree.clone(),
        cargo_lock_sha256: inputs.cargo_lock_sha256.clone(),
        default_features: contract.default_features,
        cargo_features: contract.cargo_features.clone(),
        toolchain: inputs.toolchain.clone(),
        build_environment: inputs.build_environment.clone(),
        files,
        payload_sha256,
        native_runtime_evidence: NativeRuntimeEvidence {
            state: RUNTIME_EVIDENCE_STATE.to_string(),
            source: RUNTIME_EVIDENCE_SOURCE.to_string(),
        },
    };
    validate_identity(&identity, contract, inputs)?;
    verify_payload_directory(&payload_dir, &identity)?;

    let identity_path = root.join("payload-identity.json");
    let markdown_path = root.join("payload-identity.md");
    let identity_text = serde_json::to_string_pretty(&identity)
        .map_err(|err| format!("failed to render final native payload identity: {err}"))?;
    fs::write(&identity_path, format!("{identity_text}\n"))
        .map_err(|err| format!("failed to write {}: {err}", identity_path.display()))?;
    fs::write(&markdown_path, render_payload_markdown(&identity))
        .map_err(|err| format!("failed to write {}: {err}", markdown_path.display()))?;

    let readback = read_payload_identity(&identity_path)?;
    if readback != identity {
        return Err(format!(
            "{} did not round-trip to the staged payload identity",
            identity_path.display()
        ));
    }
    validate_identity(&readback, contract, inputs)?;
    verify_payload_directory(&payload_dir, &readback)?;

    Ok(StagedNativePayload {
        root,
        payload_dir,
        identity_path,
        markdown_path,
        identity,
    })
}

fn collect_portable_build_inputs(repository_root: &Path) -> Result<PortableBuildInputs, String> {
    let tracked_changes = run_output("git", &["status", "--porcelain=v1", "--untracked-files=no"])?;
    if !tracked_changes.trim().is_empty() {
        return Err("final native payload requires a clean tracked source tree".to_string());
    }
    let candidate_commit = normalized_command_output("git", &["rev-parse", "HEAD"], "commit")?;
    let candidate_tree = normalized_command_output(
        "git",
        &["rev-parse", "HEAD^{tree}"],
        "source tree",
    )?;
    let cargo_lock_sha256 = sha256_file(&repository_root.join("Cargo.lock"))?;
    let rustc_verbose_version =
        normalized_command_output("rustc", &["-vV"], "rustc toolchain")?;
    let cargo_verbose_version =
        normalized_command_output("cargo", &["-vV"], "cargo toolchain")?;
    let inputs = PortableBuildInputs {
        candidate_commit,
        candidate_tree,
        cargo_lock_sha256,
        toolchain: ToolchainIdentity {
            rustc_verbose_version,
            cargo_verbose_version,
        },
        build_environment: collect_build_environment()?,
    };
    validate_build_inputs(&inputs)?;
    Ok(inputs)
}

fn collect_build_environment() -> Result<BuildEnvironmentIdentity, String> {
    let github_actions = optional_env("GITHUB_ACTIONS")?;
    let provider = if github_actions.as_deref() == Some("true") {
        "github-actions"
    } else {
        "local"
    };
    let host_os = optional_env("RUNNER_OS")?.unwrap_or_else(|| std::env::consts::OS.to_string());
    let host_arch =
        optional_env("RUNNER_ARCH")?.unwrap_or_else(|| std::env::consts::ARCH.to_string());
    let build_environment = BuildEnvironmentIdentity {
        provider: provider.to_string(),
        host_os,
        host_arch,
        image_os: optional_env("ImageOS")?,
        image_version: optional_env("ImageVersion")?,
        profile: "release".to_string(),
    };
    validate_build_environment(&build_environment)?;
    Ok(build_environment)
}

fn optional_env(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) => {
            let normalized = value.trim().to_string();
            if normalized.is_empty() {
                Ok(None)
            } else {
                validate_portable_scalar(name, &normalized)?;
                Ok(Some(normalized))
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("{name} must be valid UTF-8 text"))
        }
    }
}

fn normalized_command_output(
    program: &str,
    args: &[&str],
    label: &str,
) -> Result<String, String> {
    let output = run_output(program, args)?;
    let normalized = output.replace("\r\n", "\n").trim().to_string();
    if normalized.is_empty() {
        Err(format!("{label} identity was empty"))
    } else {
        Ok(normalized)
    }
}

fn workspace_version(path: PathBuf) -> Result<String, String> {
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let value: toml::Value = toml::from_str(&text)
        .map_err(|err| format!("{} is invalid TOML: {err}", path.display()))?;
    value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{} is missing [workspace.package].version", path.display()))
}

fn copy_payload_file(source: &Path, destination: &Path, label: &str) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("{label} is missing: {}", source.display()));
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

fn payload_files(
    payload_dir: &Path,
    executable: &str,
) -> Result<Vec<PayloadFileIdentity>, String> {
    let expected = expected_payload_roles(executable);
    let mut files = Vec::new();
    for entry in fs::read_dir(payload_dir)
        .map_err(|err| format!("failed to read {}: {err}", payload_dir.display()))?
    {
        let entry = entry
            .map_err(|err| format!("failed to read entry under {}: {err}", payload_dir.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "final native payload must remain flat; found non-file `{}`",
                path.display()
            ));
        }
        let relative_path = entry
            .file_name()
            .to_str()
            .ok_or_else(|| format!("payload file name is not UTF-8: {}", path.display()))?
            .to_string();
        validate_relative_payload_path(&relative_path)?;
        let role = expected.get(&relative_path).copied().ok_or_else(|| {
            format!("final native payload contains unexpected file `{relative_path}`")
        })?;
        files.push(PayloadFileIdentity {
            relative_path,
            role,
            size: metadata.len(),
            sha256: sha256_file(&path)?,
        });
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let actual = files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<BTreeSet<_>>();
    let expected_names = expected.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if actual != expected_names {
        return Err(format!(
            "final native payload inventory mismatch; expected {expected_names:?}, got {actual:?}"
        ));
    }
    Ok(files)
}

fn expected_payload_roles(executable: &str) -> std::collections::BTreeMap<String, PayloadFileRole> {
    std::collections::BTreeMap::from([
        (executable.to_string(), PayloadFileRole::Executable),
        (
            "LICENSE-APACHE".to_string(),
            PayloadFileRole::LicenseApache,
        ),
        ("LICENSE-MIT".to_string(), PayloadFileRole::LicenseMit),
        ("README-server.txt".to_string(), PayloadFileRole::Readme),
    ])
}

fn aggregate_payload_sha256(files: &[PayloadFileIdentity]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ripr-final-native-payload-v1\0");
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update(b"\0");
        hasher.update(file.role.as_str().as_bytes());
        hasher.update(b"\0");
        hasher.update(file.size.to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(file.sha256.as_bytes());
        hasher.update(b"\0");
    }
    hex_lower(&hasher.finalize())
}

fn verify_payload_directory(
    payload_dir: &Path,
    identity: &FinalNativePayloadIdentity,
) -> Result<(), String> {
    let actual_files = payload_files(payload_dir, &identity.executable)?;
    if actual_files != identity.files {
        return Err(format!(
            "payload files under {} do not match the recorded identity",
            payload_dir.display()
        ));
    }
    let aggregate = aggregate_payload_sha256(&actual_files);
    if aggregate != identity.payload_sha256 {
        return Err(format!(
            "payload aggregate digest mismatch: expected {}, got {aggregate}",
            identity.payload_sha256
        ));
    }
    Ok(())
}

fn validate_identity(
    identity: &FinalNativePayloadIdentity,
    contract: &PayloadContractView,
    inputs: &PortableBuildInputs,
) -> Result<(), String> {
    if identity.schema_version != PAYLOAD_SCHEMA_VERSION {
        return Err(format!(
            "payload schema version must be `{PAYLOAD_SCHEMA_VERSION}`, got `{}`",
            identity.schema_version
        ));
    }
    if identity.product != contract.product
        || identity.native_version != contract.native_version
        || identity.target != contract.target
        || identity.executable != contract.executable
        || identity.default_features != contract.default_features
        || identity.cargo_features != contract.cargo_features
    {
        return Err("payload identity does not match the distribution contract".to_string());
    }
    if identity.candidate_commit != inputs.candidate_commit
        || identity.candidate_tree != inputs.candidate_tree
        || identity.cargo_lock_sha256 != inputs.cargo_lock_sha256
        || identity.toolchain != inputs.toolchain
        || identity.build_environment != inputs.build_environment
    {
        return Err("payload identity does not match the selected build inputs".to_string());
    }
    if identity.native_runtime_evidence.state != RUNTIME_EVIDENCE_STATE
        || identity.native_runtime_evidence.source != RUNTIME_EVIDENCE_SOURCE
    {
        return Err("payload runtime evidence must remain unqualified under issue #4489".to_string());
    }
    validate_sha256("payload_sha256", &identity.payload_sha256)?;
    let mut previous = None;
    for file in &identity.files {
        validate_relative_payload_path(&file.relative_path)?;
        validate_sha256("payload file sha256", &file.sha256)?;
        if let Some(previous_path) = previous {
            if previous_path >= file.relative_path.as_str() {
                return Err("payload file records must be strictly sorted and unique".to_string());
            }
        }
        previous = Some(file.relative_path.as_str());
    }
    if aggregate_payload_sha256(&identity.files) != identity.payload_sha256 {
        return Err("payload aggregate digest does not match its file records".to_string());
    }
    Ok(())
}

fn validate_contract_view(contract: &PayloadContractView) -> Result<(), String> {
    validate_portable_scalar("product", &contract.product)?;
    validate_portable_scalar("native_version", &contract.native_version)?;
    validate_portable_scalar("target", &contract.target)?;
    validate_relative_payload_path(&contract.executable)?;
    if contract.cargo_features.is_empty() {
        return Err("payload identity requires at least one Cargo feature".to_string());
    }
    let feature_set = contract
        .cargo_features
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if feature_set.len() != contract.cargo_features.len() {
        return Err("payload Cargo features must be unique".to_string());
    }
    if !contract
        .cargo_features
        .windows(2)
        .all(|pair| pair[0] < pair[1])
    {
        return Err("payload Cargo features must be sorted".to_string());
    }
    for feature in &contract.cargo_features {
        validate_portable_scalar("cargo feature", feature)?;
    }
    Ok(())
}

fn validate_build_inputs(inputs: &PortableBuildInputs) -> Result<(), String> {
    validate_git_oid("candidate_commit", &inputs.candidate_commit)?;
    validate_git_oid("candidate_tree", &inputs.candidate_tree)?;
    validate_sha256("cargo_lock_sha256", &inputs.cargo_lock_sha256)?;
    validate_toolchain_text("rustc_verbose_version", &inputs.toolchain.rustc_verbose_version)?;
    validate_toolchain_text("cargo_verbose_version", &inputs.toolchain.cargo_verbose_version)?;
    validate_build_environment(&inputs.build_environment)
}

fn validate_git_oid(label: &str, value: &str) -> Result<(), String> {
    if value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(format!(
            "{label} must be a lowercase 40-character Git object ID"
        ))
    }
}

fn validate_sha256(label: &str, value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(format!("{label} must be a lowercase SHA-256 digest"))
    }
}

fn validate_toolchain_text(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 4096 || value.contains('\0') || value.contains('\r') {
        Err(format!("{label} is not portable bounded text"))
    } else {
        Ok(())
    }
}

fn validate_build_environment(environment: &BuildEnvironmentIdentity) -> Result<(), String> {
    validate_portable_scalar("build provider", &environment.provider)?;
    validate_portable_scalar("build host OS", &environment.host_os)?;
    validate_portable_scalar("build host architecture", &environment.host_arch)?;
    validate_portable_scalar("build profile", &environment.profile)?;
    if environment.profile != "release" {
        return Err("final native payload must use the release profile".to_string());
    }
    if let Some(image_os) = environment.image_os.as_deref() {
        validate_portable_scalar("build image OS", image_os)?;
    }
    if let Some(image_version) = environment.image_version.as_deref() {
        validate_portable_scalar("build image version", image_version)?;
    }
    Ok(())
}

fn validate_portable_scalar(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|character| character.is_control() || character == '\0')
    {
        Err(format!(
            "{label} must be non-empty bounded portable text"
        ))
    } else {
        Ok(())
    }
}

fn validate_relative_payload_path(path: &str) -> Result<(), String> {
    let path_value = Path::new(path);
    if path.is_empty() || path_value.is_absolute() {
        return Err(format!("payload path must be relative: `{path}`"));
    }
    let mut components = path_value.components();
    let first = components.next();
    if !matches!(first, Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "payload path must be one portable flat file name: `{path}`"
        ));
    }
    if path.contains('\\') || path.contains('/') {
        return Err(format!(
            "payload path must not contain directory separators: `{path}`"
        ));
    }
    Ok(())
}

fn read_payload_identity(path: &Path) -> Result<FinalNativePayloadIdentity, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|err| format!("{} is not a valid payload identity: {err}", path.display()))
}

fn render_payload_markdown(identity: &FinalNativePayloadIdentity) -> String {
    let mut output = String::new();
    output.push_str("# Final native payload identity\n\n");
    output.push_str(&format!("- schema: `{}`\n", identity.schema_version));
    output.push_str(&format!("- product: `{}`\n", identity.product));
    output.push_str(&format!(
        "- native version: `{}`\n",
        identity.native_version
    ));
    output.push_str(&format!("- target: `{}`\n", identity.target));
    output.push_str(&format!("- executable: `{}`\n", identity.executable));
    output.push_str(&format!(
        "- candidate commit: `{}`\n",
        identity.candidate_commit
    ));
    output.push_str(&format!(
        "- candidate tree: `{}`\n",
        identity.candidate_tree
    ));
    output.push_str(&format!(
        "- Cargo.lock SHA-256: `{}`\n",
        identity.cargo_lock_sha256
    ));
    output.push_str(&format!(
        "- payload SHA-256: `{}`\n",
        identity.payload_sha256
    ));
    output.push_str(&format!(
        "- native runtime evidence: `{}` (`{}`)\n",
        identity.native_runtime_evidence.state, identity.native_runtime_evidence.source
    ));
    output.push_str(&format!(
        "- build environment: `{}/{}` on `{}`\n",
        identity.build_environment.host_os,
        identity.build_environment.host_arch,
        identity.build_environment.provider
    ));
    if let Some(image_os) = identity.build_environment.image_os.as_deref() {
        output.push_str(&format!("- image OS: `{image_os}`\n"));
    }
    if let Some(image_version) = identity.build_environment.image_version.as_deref() {
        output.push_str(&format!("- image version: `{image_version}`\n"));
    }
    output.push_str("\n## Cargo features\n\n");
    for feature in &identity.cargo_features {
        output.push_str(&format!("- `{feature}`\n"));
    }
    output.push_str("\n## Payload files\n\n");
    output.push_str("| Relative path | Role | Bytes | SHA-256 |\n");
    output.push_str("| --- | --- | ---: | --- |\n");
    for file in &identity.files {
        output.push_str(&format!(
            "| `{}` | `{}` | {} | `{}` |\n",
            file.relative_path,
            file.role.as_str(),
            file.size,
            file.sha256
        ));
    }
    output.push_str("\n## Toolchain\n\n### rustc\n\n```text\n");
    output.push_str(&identity.toolchain.rustc_verbose_version);
    output.push_str("\n```\n\n### cargo\n\n```text\n");
    output.push_str(&identity.toolchain.cargo_verbose_version);
    output.push_str("\n```\n");
    output
}

fn extract_zip(asset_path: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(asset_path)
        .map_err(|err| format!("failed to open {}: {err}", asset_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| format!("failed to read {} as zip: {err}", asset_path.display()))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("failed to read zip entry {index}: {err}"))?;
        if entry.is_dir() {
            return Err(format!(
                "release server zip must remain flat; found directory `{}`",
                entry.name()
            ));
        }
        let relative_path = entry.name().to_string();
        validate_relative_payload_path(&relative_path)?;
        let output_path = destination.join(&relative_path);
        let mut output = fs::File::create(&output_path)
            .map_err(|err| format!("failed to create {}: {err}", output_path.display()))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|err| format!("failed to extract {}: {err}", output_path.display()))?;
        output
            .flush()
            .map_err(|err| format!("failed to flush {}: {err}", output_path.display()))?;
    }
    Ok(())
}

fn extract_tar_gz(asset_path: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(asset_path)
        .map_err(|err| format!("failed to open {}: {err}", asset_path.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(destination)
        .map_err(|err| format!("failed to extract {}: {err}", asset_path.display()))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn staged_identity_round_trips_and_detects_payload_mutation() -> Result<(), String> {
        let root = test_root("round-trip")?;
        write_test_repository(&root)?;
        let built = root.join("built-ripr");
        fs::write(&built, b"native-ripr")
            .map_err(|err| format!("failed to write {}: {err}", built.display()))?;
        let contract = test_contract();
        let inputs = test_inputs();
        let staged = stage_final_native_payload_at(&root, &built, &contract, &inputs)?;
        verify_payload_directory(&staged.payload_dir, &staged.identity)?;
        fs::write(staged.payload_dir.join("ripr"), b"mutated-ripr")
            .map_err(|err| format!("failed to mutate test payload: {err}"))?;
        let error = verify_payload_directory(&staged.payload_dir, &staged.identity)
            .err()
            .ok_or_else(|| "mutated payload unexpectedly verified".to_string())?;
        if !error.contains("do not match") {
            return Err(format!("unexpected mutation error: {error}"));
        }
        cleanup(&root)
    }

    #[test]
    fn strict_identity_rejects_unknown_fields() -> Result<(), String> {
        let identity = test_identity();
        let mut value = serde_json::to_value(identity)
            .map_err(|err| format!("failed to render test identity: {err}"))?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| "test identity did not render as an object".to_string())?;
        object.insert("unexpected".to_string(), serde_json::json!(true));
        let text = serde_json::to_string(&value)
            .map_err(|err| format!("failed to render mutated identity: {err}"))?;
        if serde_json::from_str::<FinalNativePayloadIdentity>(&text).is_ok() {
            return Err("identity with unknown field unexpectedly parsed".to_string());
        }
        Ok(())
    }

    #[test]
    fn aggregate_digest_changes_with_role_size_path_or_bytes() -> Result<(), String> {
        let base = PayloadFileIdentity {
            relative_path: "ripr".to_string(),
            role: PayloadFileRole::Executable,
            size: 4,
            sha256: "11".repeat(32),
        };
        let base_digest = aggregate_payload_sha256(std::slice::from_ref(&base));
        let variants = [
            PayloadFileIdentity {
                relative_path: "other".to_string(),
                ..base.clone()
            },
            PayloadFileIdentity {
                role: PayloadFileRole::Readme,
                ..base.clone()
            },
            PayloadFileIdentity {
                size: 5,
                ..base.clone()
            },
            PayloadFileIdentity {
                sha256: "22".repeat(32),
                ..base.clone()
            },
        ];
        for variant in variants {
            if aggregate_payload_sha256(&[variant]) == base_digest {
                return Err("payload aggregate digest ignored an identity dimension".to_string());
            }
        }
        Ok(())
    }

    #[test]
    fn payload_paths_reject_absolute_parent_and_nested_forms() -> Result<(), String> {
        for path in ["", "/tmp/ripr", "../ripr", "bin/ripr", "bin\\ripr", "."] {
            if validate_relative_payload_path(path).is_ok() {
                return Err(format!(
                    "invalid payload path unexpectedly accepted: {path:?}"
                ));
            }
        }
        validate_relative_payload_path("ripr")
    }

    #[test]
    fn identity_validation_rejects_stale_source_and_lock_inputs() -> Result<(), String> {
        let identity = test_identity();
        let contract = test_contract();
        let mut stale = test_inputs();
        stale.candidate_tree = "c".repeat(40);
        if validate_identity(&identity, &contract, &stale).is_ok() {
            return Err("stale source tree unexpectedly verified".to_string());
        }
        stale = test_inputs();
        stale.cargo_lock_sha256 = "33".repeat(32);
        if validate_identity(&identity, &contract, &stale).is_ok() {
            return Err("stale Cargo.lock identity unexpectedly verified".to_string());
        }
        Ok(())
    }

    fn test_root(label: &str) -> Result<PathBuf, String> {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-final-native-payload-{label}-{}-{id}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root)
                .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
        }
        fs::create_dir_all(&root)
            .map_err(|err| format!("failed to create {}: {err}", root.display()))?;
        Ok(root)
    }

    fn write_test_repository(root: &Path) -> Result<(), String> {
        fs::write(root.join("LICENSE-APACHE"), b"apache")
            .map_err(|err| format!("failed to write test Apache license: {err}"))?;
        fs::write(root.join("LICENSE-MIT"), b"mit")
            .map_err(|err| format!("failed to write test MIT license: {err}"))?;
        fs::write(root.join("Cargo.lock"), b"lock")
            .map_err(|err| format!("failed to write test lockfile: {err}"))
    }

    fn test_contract() -> PayloadContractView {
        PayloadContractView {
            product: "ripr".to_string(),
            native_version: "0.11.0".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            executable: "ripr".to_string(),
            default_features: true,
            cargo_features: vec![
                "lang-python".to_string(),
                "lang-rust".to_string(),
                "lang-typescript".to_string(),
            ],
        }
    }

    fn test_inputs() -> PortableBuildInputs {
        PortableBuildInputs {
            candidate_commit: "a".repeat(40),
            candidate_tree: "b".repeat(40),
            cargo_lock_sha256: "11".repeat(32),
            toolchain: ToolchainIdentity {
                rustc_verbose_version: "rustc 1.95.0\nhost: x86_64-unknown-linux-gnu"
                    .to_string(),
                cargo_verbose_version: "cargo 1.95.0".to_string(),
            },
            build_environment: BuildEnvironmentIdentity {
                provider: "github-actions".to_string(),
                host_os: "Linux".to_string(),
                host_arch: "X64".to_string(),
                image_os: Some("ubuntu22".to_string()),
                image_version: Some("20260922.1".to_string()),
                profile: "release".to_string(),
            },
        }
    }

    fn test_identity() -> FinalNativePayloadIdentity {
        let contract = test_contract();
        let inputs = test_inputs();
        let files = vec![PayloadFileIdentity {
            relative_path: "ripr".to_string(),
            role: PayloadFileRole::Executable,
            size: 4,
            sha256: "11".repeat(32),
        }];
        FinalNativePayloadIdentity {
            schema_version: PAYLOAD_SCHEMA_VERSION.to_string(),
            product: contract.product,
            native_version: contract.native_version,
            target: contract.target,
            executable: contract.executable,
            candidate_commit: inputs.candidate_commit,
            candidate_tree: inputs.candidate_tree,
            cargo_lock_sha256: inputs.cargo_lock_sha256,
            default_features: contract.default_features,
            cargo_features: contract.cargo_features,
            toolchain: inputs.toolchain,
            build_environment: inputs.build_environment,
            payload_sha256: aggregate_payload_sha256(&files),
            files,
            native_runtime_evidence: NativeRuntimeEvidence {
                state: RUNTIME_EVIDENCE_STATE.to_string(),
                source: RUNTIME_EVIDENCE_SOURCE.to_string(),
            },
        }
    }

    fn cleanup(root: &Path) -> Result<(), String> {
        fs::remove_dir_all(root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))
    }
}
