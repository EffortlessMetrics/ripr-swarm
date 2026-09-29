use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::policy::distribution::{DistributionContract, TargetContract, load_distribution_contract};
use crate::run::capture_process_output;

use super::{hex_lower, release_server_readme, sha256_file};

const PAYLOAD_SCHEMA_VERSION: &str = "1.0";
const PAYLOAD_IDENTITY_JSON: &str = "payload-identity.json";
const PAYLOAD_IDENTITY_MARKDOWN: &str = "payload-identity.md";
const DISTRIBUTION_ROOT: &str = "target/ripr/distribution";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FinalNativePayloadIdentity {
    pub(crate) schema_version: String,
    pub(crate) product: String,
    pub(crate) native_version: String,
    pub(crate) rust_target: String,
    pub(crate) executable_relative_path: String,
    pub(crate) candidate_sha: String,
    pub(crate) candidate_tree: String,
    pub(crate) cargo_lock_sha256: String,
    pub(crate) cargo_features: Vec<String>,
    pub(crate) toolchain: PayloadToolchainIdentity,
    pub(crate) build_environment: PayloadBuildEnvironmentIdentity,
    pub(crate) files: Vec<PayloadFileIdentity>,
    pub(crate) payload_sha256: String,
    pub(crate) native_runtime_evidence: NativeRuntimeEvidence,
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
    pub(crate) runner_os: String,
    pub(crate) runner_arch: String,
    pub(crate) image: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PayloadFileIdentity {
    pub(crate) relative_path: String,
    pub(crate) role: String,
    pub(crate) size: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeRuntimeEvidence {
    pub(crate) state: String,
    pub(crate) qualification_issue: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PayloadIdentityContext {
    product: String,
    native_version: String,
    rust_target: String,
    executable_relative_path: String,
    candidate_sha: String,
    candidate_tree: String,
    cargo_lock_sha256: String,
    cargo_features: Vec<String>,
    toolchain: PayloadToolchainIdentity,
    build_environment: PayloadBuildEnvironmentIdentity,
    native_runtime_evidence: NativeRuntimeEvidence,
}

#[derive(Clone, Debug)]
pub(crate) struct StagedFinalNativePayload {
    pub(crate) root: PathBuf,
    pub(crate) payload_dir: PathBuf,
    pub(crate) identity_json: PathBuf,
    pub(crate) identity_markdown: PathBuf,
    pub(crate) identity: FinalNativePayloadIdentity,
}

pub(crate) fn stage_final_native_payload(
    version: &str,
    rust_target: &str,
    executable: &str,
    archive: &str,
) -> Result<StagedFinalNativePayload, String> {
    validate_path_segment("version", version)?;
    validate_path_segment("target", rust_target)?;
    validate_payload_relative_path(executable)?;

    let contract = load_distribution_contract()?;
    let target_contract = target_contract(&contract, rust_target)?;
    if target_contract.executable != executable {
        return Err(format!(
            "distribution target `{rust_target}` requires executable `{}`, got `{executable}`",
            target_contract.executable
        ));
    }
    if target_contract.archive != archive {
        return Err(format!(
            "distribution target `{rust_target}` requires archive `{}`, got `{archive}`",
            target_contract.archive
        ));
    }

    let workspace_version = workspace_version(Path::new("Cargo.toml"))?;
    if version != workspace_version {
        return Err(format!(
            "payload version `{version}` does not match workspace version `{workspace_version}`"
        ));
    }

    let context = payload_context(&contract, target_contract, version, executable)?;
    let root = Path::new(DISTRIBUTION_ROOT)
        .join(format!("v{version}"))
        .join(rust_target);
    let payload_dir = root.join("payload");
    if root.exists() {
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
    }
    fs::create_dir_all(&payload_dir)
        .map_err(|err| format!("failed to create {}: {err}", payload_dir.display()))?;

    let built_executable = Path::new("target")
        .join(rust_target)
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
        release_server_readme(version),
    )
    .map_err(|err| {
        format!(
            "failed to write {}: {err}",
            payload_dir.join("README-server.txt").display()
        )
    })?;

    let identity = identity_for_payload(&payload_dir, &context)?;
    let identity_json = root.join(PAYLOAD_IDENTITY_JSON);
    let identity_markdown = root.join(PAYLOAD_IDENTITY_MARKDOWN);
    write_identity_files(&identity_json, &identity_markdown, &identity)?;
    let verified = verify_final_native_payload(&root, Some(&identity))?;
    if verified != identity {
        return Err("verified final native payload identity changed after staging".to_string());
    }

    Ok(StagedFinalNativePayload {
        root,
        payload_dir,
        identity_json,
        identity_markdown,
        identity,
    })
}

pub(crate) fn verify_final_native_payload(
    root: &Path,
    expected: Option<&FinalNativePayloadIdentity>,
) -> Result<FinalNativePayloadIdentity, String> {
    let payload_dir = root.join("payload");
    let identity_json_path = root.join(PAYLOAD_IDENTITY_JSON);
    let identity_markdown_path = root.join(PAYLOAD_IDENTITY_MARKDOWN);
    let identity_text = fs::read_to_string(&identity_json_path)
        .map_err(|err| format!("failed to read {}: {err}", identity_json_path.display()))?;
    let identity: FinalNativePayloadIdentity = serde_json::from_str(&identity_text).map_err(|err| {
        format!(
            "failed to parse {} as final native payload identity: {err}",
            identity_json_path.display()
        )
    })?;
    validate_identity_semantics(&identity)?;

    let canonical_json = render_identity_json(&identity)?;
    if identity_text != canonical_json {
        return Err(format!(
            "{} is not the canonical final native payload identity encoding",
            identity_json_path.display()
        ));
    }
    if let Some(expected) = expected
        && &identity != expected
    {
        return Err("staged final native payload identity does not match the selected build inputs".to_string());
    }

    let actual_files = payload_file_identities(&payload_dir, &identity.executable_relative_path)?;
    if actual_files != identity.files {
        return Err(format!(
            "final native payload file inventory or digest mismatch under {}",
            payload_dir.display()
        ));
    }
    let actual_payload_sha256 = payload_aggregate_digest(&actual_files);
    if actual_payload_sha256 != identity.payload_sha256 {
        return Err(format!(
            "final native payload aggregate digest mismatch: expected {}, got {}",
            identity.payload_sha256, actual_payload_sha256
        ));
    }

    let markdown = fs::read_to_string(&identity_markdown_path).map_err(|err| {
        format!(
            "failed to read {}: {err}",
            identity_markdown_path.display()
        )
    })?;
    let canonical_markdown = render_identity_markdown(&identity);
    if markdown != canonical_markdown {
        return Err(format!(
            "{} does not match the machine-readable final native payload identity",
            identity_markdown_path.display()
        ));
    }

    Ok(identity)
}

fn payload_context(
    contract: &DistributionContract,
    target: &TargetContract,
    version: &str,
    executable: &str,
) -> Result<PayloadIdentityContext, String> {
    let candidate_sha = git_output(&["rev-parse", "HEAD"])?;
    validate_lower_hex("candidate SHA", &candidate_sha, 40)?;
    if let Ok(requested_sha) = env::var("CANDIDATE_SHA") {
        let requested_sha = requested_sha.trim().to_ascii_lowercase();
        if requested_sha != candidate_sha {
            return Err(format!(
                "candidate SHA mismatch: requested `{requested_sha}`, checkout is `{candidate_sha}`"
            ));
        }
    }

    let candidate_tree = git_output(&["rev-parse", "HEAD^{tree}"])?;
    validate_lower_hex("candidate tree", &candidate_tree, 40)?;
    if let Ok(requested_tree) = env::var("CANDIDATE_TREE") {
        let requested_tree = requested_tree.trim().to_ascii_lowercase();
        if requested_tree != candidate_tree {
            return Err(format!(
                "candidate tree mismatch: requested `{requested_tree}`, checkout is `{candidate_tree}`"
            ));
        }
    }

    let mut cargo_features = contract.product.features.clone();
    cargo_features.sort();
    cargo_features.dedup();

    Ok(PayloadIdentityContext {
        product: contract.product.name.clone(),
        native_version: version.to_string(),
        rust_target: target.rust_target.clone(),
        executable_relative_path: executable.to_string(),
        candidate_sha,
        candidate_tree,
        cargo_lock_sha256: sha256_file(Path::new("Cargo.lock"))?,
        cargo_features,
        toolchain: PayloadToolchainIdentity {
            rustc_verbose_version: command_output("rustc", &["-vV"])?,
            cargo_verbose_version: command_output("cargo", &["-vV"])?,
        },
        build_environment: PayloadBuildEnvironmentIdentity {
            runner_os: env_value("RUNNER_OS", env::consts::OS),
            runner_arch: env_value("RUNNER_ARCH", env::consts::ARCH),
            image: build_image_identity(),
        },
        native_runtime_evidence: NativeRuntimeEvidence {
            state: target.compatibility_state.clone(),
            qualification_issue: target.qualification_issue,
        },
    })
}

fn target_contract<'a>(
    contract: &'a DistributionContract,
    rust_target: &str,
) -> Result<&'a TargetContract, String> {
    contract
        .target
        .iter()
        .find(|target| target.rust_target == rust_target)
        .ok_or_else(|| format!("distribution target `{rust_target}` is not registered"))
}

fn workspace_version(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let value: toml::Value = toml::from_str(&text)
        .map_err(|err| format!("{}: invalid Cargo manifest: {err}", path.display()))?;
    value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{}: missing [workspace.package].version", path.display()))
}

fn command_output(program: &str, args: &[&str]) -> Result<String, String> {
    let args = args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>();
    let bytes = capture_process_output(program, &args, &[]).map_err(|error| error.message)?;
    let text = String::from_utf8(bytes)
        .map_err(|err| format!("{program} output was not UTF-8: {err}"))?;
    let normalized = text.replace("\r\n", "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(format!("{program} {} produced empty output", args.join(" ")));
    }
    Ok(trimmed.to_string())
}

fn git_output(args: &[&str]) -> Result<String, String> {
    command_output("git", args).map(|value| value.to_ascii_lowercase())
}

fn env_value(name: &str, fallback: &str) -> String {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn build_image_identity() -> String {
    let image_os = env::var("ImageOS")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let image_version = env::var("ImageVersion")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    match (image_os, image_version) {
        (Some(image_os), Some(image_version)) => format!("{image_os}@{image_version}"),
        (Some(image_os), None) => image_os,
        (None, Some(image_version)) => format!("github-actions@{image_version}"),
        (None, None) => "local-or-unspecified".to_string(),
    }
}

fn copy_payload_file(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|err| format!("failed to inspect {}: {err}", source.display()))?;
    if !metadata.file_type().is_file() {
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

fn identity_for_payload(
    payload_dir: &Path,
    context: &PayloadIdentityContext,
) -> Result<FinalNativePayloadIdentity, String> {
    let files = payload_file_identities(payload_dir, &context.executable_relative_path)?;
    if !files.iter().any(|file| {
        file.relative_path == context.executable_relative_path && file.role == "executable"
    }) {
        return Err(format!(
            "final native payload does not contain executable `{}`",
            context.executable_relative_path
        ));
    }
    let payload_sha256 = payload_aggregate_digest(&files);
    let identity = FinalNativePayloadIdentity {
        schema_version: PAYLOAD_SCHEMA_VERSION.to_string(),
        product: context.product.clone(),
        native_version: context.native_version.clone(),
        rust_target: context.rust_target.clone(),
        executable_relative_path: context.executable_relative_path.clone(),
        candidate_sha: context.candidate_sha.clone(),
        candidate_tree: context.candidate_tree.clone(),
        cargo_lock_sha256: context.cargo_lock_sha256.clone(),
        cargo_features: context.cargo_features.clone(),
        toolchain: context.toolchain.clone(),
        build_environment: context.build_environment.clone(),
        files,
        payload_sha256,
        native_runtime_evidence: context.native_runtime_evidence.clone(),
    };
    validate_identity_semantics(&identity)?;
    Ok(identity)
}

fn payload_file_identities(
    payload_dir: &Path,
    executable_relative_path: &str,
) -> Result<Vec<PayloadFileIdentity>, String> {
    let mut paths = Vec::new();
    collect_payload_files(payload_dir, payload_dir, &mut paths)?;
    paths.sort_by(|left, right| left.0.cmp(&right.0));
    paths
        .into_iter()
        .map(|(relative_path, absolute_path)| {
            let metadata = fs::metadata(&absolute_path).map_err(|err| {
                format!("failed to inspect {}: {err}", absolute_path.display())
            })?;
            Ok(PayloadFileIdentity {
                role: payload_role(&relative_path, executable_relative_path),
                relative_path,
                size: metadata.len(),
                sha256: sha256_file(&absolute_path)?,
            })
        })
        .collect()
}

fn collect_payload_files(
    root: &Path,
    directory: &Path,
    paths: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|err| format!("failed to read {}: {err}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to read entry under {}: {err}", directory.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "final native payload cannot contain symlink `{}`",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_payload_files(root, &path, paths)?;
        } else if metadata.is_file() {
            let relative = path.strip_prefix(root).map_err(|err| {
                format!(
                    "failed to make {} relative to {}: {err}",
                    path.display(),
                    root.display()
                )
            })?;
            let relative = portable_relative_path(relative)?;
            paths.push((relative, path));
        } else {
            return Err(format!(
                "final native payload contains unsupported filesystem entry `{}`",
                path.display()
            ));
        }
    }
    Ok(())
}

fn portable_relative_path(path: &Path) -> Result<String, String> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().ok_or_else(|| {
                    format!("final native payload path is not UTF-8: {}", path.display())
                })?;
                if value.is_empty() || value == "." || value == ".." || value.contains('\0') {
                    return Err(format!(
                        "final native payload path is not portable: {}",
                        path.display()
                    ));
                }
                components.push(value);
            }
            _ => {
                return Err(format!(
                    "final native payload path is not relative and portable: {}",
                    path.display()
                ));
            }
        }
    }
    if components.is_empty() {
        return Err("final native payload path cannot be empty".to_string());
    }
    Ok(components.join("/"))
}

fn payload_role(relative_path: &str, executable_relative_path: &str) -> String {
    if relative_path == executable_relative_path {
        "executable".to_string()
    } else if matches!(relative_path, "LICENSE-MIT" | "LICENSE-APACHE") {
        "license".to_string()
    } else if relative_path == "README-server.txt" {
        "readme".to_string()
    } else if relative_path.ends_with(".dll")
        || relative_path.ends_with(".so")
        || relative_path.contains(".so.")
        || relative_path.ends_with(".dylib")
    {
        "native_library".to_string()
    } else {
        "auxiliary".to_string()
    }
}

fn payload_aggregate_digest(files: &[PayloadFileIdentity]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        update_len_prefixed(&mut hasher, file.relative_path.as_bytes());
        update_len_prefixed(&mut hasher, file.role.as_bytes());
        hasher.update(file.size.to_be_bytes());
        update_len_prefixed(&mut hasher, file.sha256.as_bytes());
    }
    hex_lower(&hasher.finalize())
}

fn update_len_prefixed(hasher: &mut Sha256, value: &[u8]) {
    let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
    hasher.update(length.to_be_bytes());
    hasher.update(value);
}

fn write_identity_files(
    json_path: &Path,
    markdown_path: &Path,
    identity: &FinalNativePayloadIdentity,
) -> Result<(), String> {
    fs::write(json_path, render_identity_json(identity)?)
        .map_err(|err| format!("failed to write {}: {err}", json_path.display()))?;
    fs::write(markdown_path, render_identity_markdown(identity))
        .map_err(|err| format!("failed to write {}: {err}", markdown_path.display()))?;
    Ok(())
}

fn render_identity_json(identity: &FinalNativePayloadIdentity) -> Result<String, String> {
    serde_json::to_string_pretty(identity)
        .map(|text| format!("{text}\n"))
        .map_err(|err| format!("failed to render final native payload identity: {err}"))
}

fn render_identity_markdown(identity: &FinalNativePayloadIdentity) -> String {
    let mut output = String::new();
    output.push_str("# Final native payload identity\n\n");
    output.push_str(&format!("- product: `{}`\n", identity.product));
    output.push_str(&format!("- native version: `{}`\n", identity.native_version));
    output.push_str(&format!("- Rust target: `{}`\n", identity.rust_target));
    output.push_str(&format!(
        "- executable: `{}`\n",
        identity.executable_relative_path
    ));
    output.push_str(&format!("- candidate SHA: `{}`\n", identity.candidate_sha));
    output.push_str(&format!("- candidate tree: `{}`\n", identity.candidate_tree));
    output.push_str(&format!(
        "- Cargo.lock SHA-256: `{}`\n",
        identity.cargo_lock_sha256
    ));
    output.push_str(&format!(
        "- Cargo features: `{}`\n",
        identity.cargo_features.join(",")
    ));
    output.push_str(&format!(
        "- runner: `{}/{}` (`{}`)\n",
        identity.build_environment.runner_os,
        identity.build_environment.runner_arch,
        identity.build_environment.image
    ));
    output.push_str(&format!(
        "- native runtime evidence: `{}` (issue #{})\n",
        identity.native_runtime_evidence.state,
        identity.native_runtime_evidence.qualification_issue
    ));
    output.push_str(&format!(
        "- payload SHA-256: `{}`\n\n",
        identity.payload_sha256
    ));
    output.push_str("| File | Role | Bytes | SHA-256 |\n");
    output.push_str("| --- | --- | ---: | --- |\n");
    for file in &identity.files {
        output.push_str(&format!(
            "| `{}` | `{}` | {} | `{}` |\n",
            file.relative_path, file.role, file.size, file.sha256
        ));
    }
    output
}

fn validate_identity_semantics(identity: &FinalNativePayloadIdentity) -> Result<(), String> {
    if identity.schema_version != PAYLOAD_SCHEMA_VERSION {
        return Err(format!(
            "unsupported final native payload schema `{}`",
            identity.schema_version
        ));
    }
    if identity.product != "ripr" {
        return Err(format!(
            "final native payload product must be `ripr`, got `{}`",
            identity.product
        ));
    }
    validate_path_segment("native version", &identity.native_version)?;
    validate_path_segment("Rust target", &identity.rust_target)?;
    validate_payload_relative_path(&identity.executable_relative_path)?;
    validate_lower_hex("candidate SHA", &identity.candidate_sha, 40)?;
    validate_lower_hex("candidate tree", &identity.candidate_tree, 40)?;
    validate_lower_hex("Cargo.lock SHA-256", &identity.cargo_lock_sha256, 64)?;
    validate_lower_hex("payload SHA-256", &identity.payload_sha256, 64)?;
    if identity.cargo_features.is_empty() {
        return Err("final native payload Cargo feature set cannot be empty".to_string());
    }
    let mut sorted_features = identity.cargo_features.clone();
    sorted_features.sort();
    sorted_features.dedup();
    if sorted_features != identity.cargo_features {
        return Err("final native payload Cargo features must be sorted and unique".to_string());
    }
    if identity.toolchain.rustc_verbose_version.trim().is_empty()
        || identity.toolchain.cargo_verbose_version.trim().is_empty()
    {
        return Err("final native payload toolchain identity cannot be empty".to_string());
    }
    if identity.build_environment.runner_os.trim().is_empty()
        || identity.build_environment.runner_arch.trim().is_empty()
        || identity.build_environment.image.trim().is_empty()
    {
        return Err("final native payload build environment identity cannot be empty".to_string());
    }
    if identity.native_runtime_evidence.state.trim().is_empty()
        || identity.native_runtime_evidence.qualification_issue == 0
    {
        return Err("final native payload native-runtime evidence owner is invalid".to_string());
    }
    if identity.files.is_empty() {
        return Err("final native payload file inventory cannot be empty".to_string());
    }
    let mut previous = None;
    let mut executable_count = 0_usize;
    for file in &identity.files {
        validate_payload_relative_path(&file.relative_path)?;
        validate_lower_hex("payload file SHA-256", &file.sha256, 64)?;
        if file.role.trim().is_empty() {
            return Err(format!(
                "final native payload file `{}` has an empty role",
                file.relative_path
            ));
        }
        if let Some(previous) = previous
            && previous >= file.relative_path.as_str()
        {
            return Err("final native payload files must be strictly path-sorted".to_string());
        }
        if file.relative_path == identity.executable_relative_path && file.role == "executable" {
            executable_count += 1;
        }
        previous = Some(file.relative_path.as_str());
    }
    if executable_count != 1 {
        return Err(format!(
            "final native payload must contain exactly one selected executable record, got {executable_count}"
        ));
    }
    Ok(())
}

fn validate_path_segment(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
    {
        return Err(format!("{label} is not a portable path segment: `{value}`"));
    }
    Ok(())
}

fn validate_payload_relative_path(value: &str) -> Result<(), String> {
    if value.is_empty() || value.contains('\\') || value.contains('\0') {
        return Err(format!(
            "final native payload relative path is not portable: `{value}`"
        ));
    }
    portable_relative_path(Path::new(value)).map(|_| ())
}

fn validate_lower_hex(label: &str, value: &str, length: usize) -> Result<(), String> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{label} must be {length} lowercase hexadecimal characters, got `{value}`"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn test_root(name: &str) -> Result<PathBuf, String> {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!(
            "ripr-final-payload-{name}-{}-{id}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root)
                .map_err(|err| format!("failed to reset {}: {err}", root.display()))?;
        }
        fs::create_dir_all(root.join("payload"))
            .map_err(|err| format!("failed to create {}: {err}", root.display()))?;
        Ok(root)
    }

    fn test_context() -> PayloadIdentityContext {
        PayloadIdentityContext {
            product: "ripr".to_string(),
            native_version: "0.11.0".to_string(),
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            executable_relative_path: "ripr".to_string(),
            candidate_sha: "1".repeat(40),
            candidate_tree: "2".repeat(40),
            cargo_lock_sha256: "3".repeat(64),
            cargo_features: vec![
                "lang-python".to_string(),
                "lang-rust".to_string(),
                "lang-typescript".to_string(),
            ],
            toolchain: PayloadToolchainIdentity {
                rustc_verbose_version: "rustc 1.95.0\nhost: test".to_string(),
                cargo_verbose_version: "cargo 1.95.0".to_string(),
            },
            build_environment: PayloadBuildEnvironmentIdentity {
                runner_os: "Linux".to_string(),
                runner_arch: "X64".to_string(),
                image: "ubuntu22@test".to_string(),
            },
            native_runtime_evidence: NativeRuntimeEvidence {
                state: "unqualified".to_string(),
                qualification_issue: 4489,
            },
        }
    }

    fn write_test_payload(root: &Path) -> Result<FinalNativePayloadIdentity, String> {
        let payload = root.join("payload");
        fs::write(payload.join("README-server.txt"), "ripr server 0.11.0\n")
            .map_err(|err| err.to_string())?;
        fs::write(payload.join("LICENSE-MIT"), "MIT\n").map_err(|err| err.to_string())?;
        fs::write(payload.join("ripr"), b"native-binary\n").map_err(|err| err.to_string())?;
        fs::write(payload.join("LICENSE-APACHE"), "Apache\n")
            .map_err(|err| err.to_string())?;
        let identity = identity_for_payload(&payload, &test_context())?;
        write_identity_files(
            &root.join(PAYLOAD_IDENTITY_JSON),
            &root.join(PAYLOAD_IDENTITY_MARKDOWN),
            &identity,
        )?;
        Ok(identity)
    }

    #[test]
    fn payload_identity_is_deterministic_and_verifiable() -> Result<(), String> {
        let root = test_root("round-trip")?;
        let first = write_test_payload(&root)?;
        let second = identity_for_payload(&root.join("payload"), &test_context())?;
        assert_eq!(first, second);
        assert_eq!(verify_final_native_payload(&root, Some(&first))?, first);
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
        Ok(())
    }

    #[test]
    fn payload_verifier_rejects_executable_mutation() -> Result<(), String> {
        let root = test_root("mutated-executable")?;
        let identity = write_test_payload(&root)?;
        fs::write(root.join("payload/ripr"), b"changed-native-binary\n")
            .map_err(|err| err.to_string())?;
        let error = verify_final_native_payload(&root, Some(&identity))
            .err()
            .ok_or_else(|| "mutated executable unexpectedly verified".to_string())?;
        assert!(error.contains("inventory or digest mismatch"));
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
        Ok(())
    }

    #[test]
    fn payload_verifier_rejects_missing_notice() -> Result<(), String> {
        let root = test_root("missing-notice")?;
        let identity = write_test_payload(&root)?;
        fs::remove_file(root.join("payload/LICENSE-MIT")).map_err(|err| err.to_string())?;
        let error = verify_final_native_payload(&root, Some(&identity))
            .err()
            .ok_or_else(|| "payload with missing notice unexpectedly verified".to_string())?;
        assert!(error.contains("inventory or digest mismatch"));
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
        Ok(())
    }

    #[test]
    fn payload_verifier_rejects_unknown_identity_fields() -> Result<(), String> {
        let root = test_root("unknown-field")?;
        let identity = write_test_payload(&root)?;
        let json_path = root.join(PAYLOAD_IDENTITY_JSON);
        let text = fs::read_to_string(&json_path).map_err(|err| err.to_string())?;
        let mutated = text.replacen('{', "{\n  \"unexpected\": true,", 1);
        fs::write(&json_path, mutated).map_err(|err| err.to_string())?;
        let error = verify_final_native_payload(&root, Some(&identity))
            .err()
            .ok_or_else(|| "identity with unknown field unexpectedly verified".to_string())?;
        assert!(error.contains("unknown field"));
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
        Ok(())
    }

    #[test]
    fn payload_verifier_rejects_selected_input_mismatch() -> Result<(), String> {
        let root = test_root("input-mismatch")?;
        let identity = write_test_payload(&root)?;
        let mut wrong = identity.clone();
        wrong.candidate_tree = "4".repeat(40);
        let error = verify_final_native_payload(&root, Some(&wrong))
            .err()
            .ok_or_else(|| "stale selected inputs unexpectedly verified".to_string())?;
        assert!(error.contains("selected build inputs"));
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
        Ok(())
    }

    #[test]
    fn identity_semantics_reject_path_escape() {
        let mut identity = FinalNativePayloadIdentity {
            schema_version: PAYLOAD_SCHEMA_VERSION.to_string(),
            product: "ripr".to_string(),
            native_version: "0.11.0".to_string(),
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            executable_relative_path: "../ripr".to_string(),
            candidate_sha: "1".repeat(40),
            candidate_tree: "2".repeat(40),
            cargo_lock_sha256: "3".repeat(64),
            cargo_features: vec!["lang-rust".to_string()],
            toolchain: PayloadToolchainIdentity {
                rustc_verbose_version: "rustc test".to_string(),
                cargo_verbose_version: "cargo test".to_string(),
            },
            build_environment: PayloadBuildEnvironmentIdentity {
                runner_os: "Linux".to_string(),
                runner_arch: "X64".to_string(),
                image: "test".to_string(),
            },
            files: vec![PayloadFileIdentity {
                relative_path: "../ripr".to_string(),
                role: "executable".to_string(),
                size: 1,
                sha256: "4".repeat(64),
            }],
            payload_sha256: "5".repeat(64),
            native_runtime_evidence: NativeRuntimeEvidence {
                state: "unqualified".to_string(),
                qualification_issue: 4489,
            },
        };
        let error = validate_identity_semantics(&identity)
            .err()
            .unwrap_or_else(|| "path escape unexpectedly accepted".to_string());
        assert!(error.contains("relative and portable"));

        identity.executable_relative_path = "ripr".to_string();
        identity.files[0].relative_path = "ripr".to_string();
        assert!(validate_identity_semantics(&identity).is_ok());
    }
}
