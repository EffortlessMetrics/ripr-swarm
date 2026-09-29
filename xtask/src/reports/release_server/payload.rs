mod identity;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::policy::distribution::{DistributionContract, load_distribution_contract};
use crate::run::run_output_owned;

use self::identity::{
    FinalNativePayloadIdentity, NativeRuntimeEvidence, PAYLOAD_SCHEMA_VERSION,
    PayloadBuildEnvironment, PayloadFileIdentity, PayloadToolchainIdentity,
    RUNTIME_EVIDENCE_SOURCE, RUNTIME_EVIDENCE_STATE, payload_aggregate_sha256,
    payload_identity_json, payload_identity_markdown,
};
use super::{release_server_readme, sha256_file};

const PAYLOAD_IDENTITY_JSON: &str = "payload-identity.json";
const PAYLOAD_IDENTITY_MARKDOWN: &str = "payload-identity.md";

#[derive(Clone, Debug)]
pub(crate) struct FinalNativePayloadPaths {
    pub(crate) payload_dir: PathBuf,
    pub(crate) identity_json: PathBuf,
    pub(crate) identity_markdown: PathBuf,
}

pub(crate) fn stage_final_native_payload(
    version: &str,
    target: &str,
    executable: &str,
) -> Result<FinalNativePayloadPaths, String> {
    validate_single_path_component("version", version)?;
    validate_single_path_component("target", target)?;
    validate_single_path_component("executable", executable)?;

    let contract = load_distribution_contract()?;
    validate_requested_payload(&contract, version, target, executable)?;

    let root = payload_root(version, target);
    if root.exists() {
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to remove {}: {err}", root.display()))?;
    }
    let payload_dir = root.join("payload");
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
        release_server_readme(version),
    )
    .map_err(|err| {
        format!(
            "failed to write {}: {err}",
            payload_dir.join("README-server.txt").display()
        )
    })?;

    let paths = FinalNativePayloadPaths {
        identity_json: root.join(PAYLOAD_IDENTITY_JSON),
        identity_markdown: root.join(PAYLOAD_IDENTITY_MARKDOWN),
        payload_dir,
    };
    let identity =
        build_payload_identity(&paths.payload_dir, &contract, version, target, executable)?;
    write_payload_identity(&paths, &identity)?;
    Ok(paths)
}

pub(crate) fn verify_final_native_payload(
    paths: &FinalNativePayloadPaths,
    version: &str,
    target: &str,
    executable: &str,
) -> Result<FinalNativePayloadIdentity, String> {
    validate_single_path_component("version", version)?;
    validate_single_path_component("target", target)?;
    validate_single_path_component("executable", executable)?;

    let expected_root = payload_root(version, target);
    let expected_paths = FinalNativePayloadPaths {
        payload_dir: expected_root.join("payload"),
        identity_json: expected_root.join(PAYLOAD_IDENTITY_JSON),
        identity_markdown: expected_root.join(PAYLOAD_IDENTITY_MARKDOWN),
    };
    if paths.payload_dir != expected_paths.payload_dir
        || paths.identity_json != expected_paths.identity_json
        || paths.identity_markdown != expected_paths.identity_markdown
    {
        return Err(format!(
            "final native payload paths do not match the deterministic target/version staging boundary `{}`",
            expected_root.display()
        ));
    }

    let identity_text = fs::read_to_string(&paths.identity_json)
        .map_err(|err| format!("failed to read {}: {err}", paths.identity_json.display()))?;
    let identity: FinalNativePayloadIdentity = serde_json::from_str(&identity_text)
        .map_err(|err| {
            format!(
                "{}: invalid payload identity: {err}",
                paths.identity_json.display()
            )
        })?;

    let canonical_identity_text = payload_identity_json(&identity)?;
    if identity_text != canonical_identity_text {
        return Err(format!(
            "{} is not the canonical payload identity rendering",
            paths.identity_json.display()
        ));
    }

    let contract = load_distribution_contract()?;
    validate_requested_payload(&contract, version, target, executable)?;
    let expected =
        build_payload_identity(&paths.payload_dir, &contract, version, target, executable)?;
    if identity != expected {
        return Err(format!(
            "{} is stale or does not match the staged final native payload",
            paths.identity_json.display()
        ));
    }

    let markdown = fs::read_to_string(&paths.identity_markdown).map_err(|err| {
        format!(
            "failed to read {}: {err}",
            paths.identity_markdown.display()
        )
    })?;
    let expected_markdown = payload_identity_markdown(&identity);
    if markdown != expected_markdown {
        return Err(format!(
            "{} is stale or does not match the machine-readable payload identity",
            paths.identity_markdown.display()
        ));
    }

    Ok(identity)
}

fn validate_requested_payload(
    contract: &DistributionContract,
    version: &str,
    target: &str,
    executable: &str,
) -> Result<(), String> {
    if contract.product.name != "ripr" || contract.product.cargo_package != "ripr" {
        return Err("distribution contract product identity is not `ripr`".to_string());
    }
    if !contract.product.default_features {
        return Err(
            "distribution contract must select the default release feature set".to_string(),
        );
    }

    let workspace_version = workspace_version()?;
    if workspace_version != version {
        return Err(format!(
            "final native payload version `{version}` does not match workspace version `{workspace_version}`"
        ));
    }

    let Some(target_contract) = contract.target.iter().find(|row| row.rust_target == target) else {
        return Err(format!(
            "target `{target}` is not present in policy/distribution.toml"
        ));
    };
    if target_contract.executable != executable {
        return Err(format!(
            "target `{target}` requires executable `{}`, got `{executable}`",
            target_contract.executable
        ));
    }
    Ok(())
}

fn workspace_version() -> Result<String, String> {
    let path = Path::new("Cargo.toml");
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|err| format!("{}: invalid TOML: {err}", path.display()))?;
    value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "{}: missing [workspace.package].version string",
                path.display()
            )
        })
}

fn payload_root(version: &str, target: &str) -> PathBuf {
    Path::new("target")
        .join("ripr")
        .join("distribution")
        .join(format!("v{version}"))
        .join(target)
}

fn build_payload_identity(
    payload_dir: &Path,
    contract: &DistributionContract,
    version: &str,
    target: &str,
    executable: &str,
) -> Result<FinalNativePayloadIdentity, String> {
    let expected_roles = expected_payload_roles(executable);
    let payload_files = payload_file_identities(payload_dir, &expected_roles)?;
    let payload_sha256 = payload_aggregate_sha256(&payload_files);
    let candidate_sha = command_identity("git", &["rev-parse", "HEAD"])?;
    validate_lower_hex("candidate commit SHA", &candidate_sha, 40)?;
    if let Some(expected_candidate) = optional_env("CANDIDATE_SHA")? {
        let expected_candidate = expected_candidate.trim().to_ascii_lowercase();
        if candidate_sha != expected_candidate {
            return Err(format!(
                "checked-out candidate SHA `{candidate_sha}` does not match CANDIDATE_SHA `{expected_candidate}`"
            ));
        }
    }
    let candidate_tree = command_identity("git", &["rev-parse", "HEAD^{tree}"])?;
    validate_lower_hex("candidate tree", &candidate_tree, 40)?;

    let cargo_lock_sha256 = sha256_file(Path::new("Cargo.lock"))?;
    validate_lower_hex("Cargo.lock SHA-256", &cargo_lock_sha256, 64)?;

    let mut cargo_features = contract.product.features.clone();
    cargo_features.sort();
    cargo_features.dedup();

    Ok(FinalNativePayloadIdentity {
        schema_version: PAYLOAD_SCHEMA_VERSION,
        product: contract.product.name.clone(),
        native_version: version.to_string(),
        target: target.to_string(),
        executable: executable.to_string(),
        candidate_sha,
        candidate_tree,
        cargo_lock_sha256,
        cargo_default_features: contract.product.default_features,
        cargo_features,
        toolchain: PayloadToolchainIdentity {
            rustc_verbose_version: command_identity("rustc", &["-vV"])?,
            cargo_verbose_version: command_identity("cargo", &["-vV"])?,
        },
        build_environment: payload_build_environment()?,
        payload_files,
        payload_sha256,
        native_runtime_evidence: NativeRuntimeEvidence {
            state: RUNTIME_EVIDENCE_STATE.to_string(),
            source: RUNTIME_EVIDENCE_SOURCE.to_string(),
        },
    })
}

fn expected_payload_roles(executable: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("LICENSE-APACHE".to_string(), "license".to_string()),
        ("LICENSE-MIT".to_string(), "license".to_string()),
        ("README-server.txt".to_string(), "readme".to_string()),
        (executable.to_string(), "executable".to_string()),
    ])
}

fn payload_file_identities(
    payload_dir: &Path,
    expected_roles: &BTreeMap<String, String>,
) -> Result<Vec<PayloadFileIdentity>, String> {
    let mut actual_paths = BTreeMap::new();
    let entries = fs::read_dir(payload_dir)
        .map_err(|err| format!("failed to read {}: {err}", payload_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read entry under {}: {err}",
                payload_dir.display()
            )
        })?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "final native payload must be flat and contain only files; found `{}`",
                path.display()
            ));
        }
        let relative_path = entry
            .file_name()
            .into_string()
            .map_err(|_| format!("non-UTF-8 payload file name under {}", payload_dir.display()))?;
        validate_payload_relative_path(&relative_path)?;
        if actual_paths.insert(relative_path.clone(), path).is_some() {
            return Err(format!(
                "duplicate final native payload path `{relative_path}`"
            ));
        }
    }

    let actual_names: Vec<&String> = actual_paths.keys().collect();
    let expected_names: Vec<&String> = expected_roles.keys().collect();
    if actual_names != expected_names {
        return Err(format!(
            "final native payload inventory mismatch: expected {:?}, got {:?}",
            expected_names, actual_names
        ));
    }

    let mut identities = Vec::with_capacity(expected_roles.len());
    for (relative_path, role) in expected_roles {
        let path = actual_paths.get(relative_path).ok_or_else(|| {
            format!(
                "final native payload file `{relative_path}` disappeared during identity capture"
            )
        })?;
        let metadata = fs::metadata(path)
            .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
        identities.push(PayloadFileIdentity {
            relative_path: relative_path.clone(),
            role: role.clone(),
            size: metadata.len(),
            sha256: sha256_file(path)?,
        });
    }
    Ok(identities)
}

fn payload_build_environment() -> Result<PayloadBuildEnvironment, String> {
    let github_actions = optional_env("GITHUB_ACTIONS")?
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    Ok(PayloadBuildEnvironment {
        provider: if github_actions {
            "github-actions".to_string()
        } else {
            "local".to_string()
        },
        runner_os: optional_env("RUNNER_OS")?,
        runner_arch: optional_env("RUNNER_ARCH")?,
        image_os: optional_env("ImageOS")?,
        image_version: optional_env("ImageVersion")?,
    })
}

fn command_identity(program: &str, args: &[&str]) -> Result<String, String> {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let output = run_output_owned(program, &owned_args)?;
    let normalized = output.trim().replace("\r\n", "\n");
    if normalized.is_empty() {
        Err(format!(
            "{program} {} produced an empty identity",
            args.join(" ")
        ))
    } else {
        Ok(normalized)
    }
}

fn optional_env(name: &str) -> Result<Option<String>, String> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(format!(
            "{name} must be valid UTF-8 when recording payload identity"
        )),
    }
}

fn write_payload_identity(
    paths: &FinalNativePayloadPaths,
    identity: &FinalNativePayloadIdentity,
) -> Result<(), String> {
    fs::write(&paths.identity_json, payload_identity_json(identity)?)
        .map_err(|err| format!("failed to write {}: {err}", paths.identity_json.display()))?;
    fs::write(
        &paths.identity_markdown,
        payload_identity_markdown(identity),
    )
    .map_err(|err| {
        format!(
            "failed to write {}: {err}",
            paths.identity_markdown.display()
        )
    })
}

fn validate_single_path_component(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.contains('/') || value.contains('\\') {
        return Err(format!(
            "{label} must be one non-empty portable path component, got `{value}`"
        ));
    }
    let mut components = Path::new(value).components();
    let first = components.next();
    if !matches!(first, Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "{label} must be one normal portable path component, got `{value}`"
        ));
    }
    Ok(())
}

fn validate_payload_relative_path(value: &str) -> Result<(), String> {
    validate_single_path_component("payload relative path", value)
}

fn validate_lower_hex(label: &str, value: &str, expected_len: usize) -> Result<(), String> {
    if value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(format!(
            "{label} must be {expected_len} lowercase hexadecimal characters, got `{value}`"
        ))
    }
}

fn copy_payload_file(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::metadata(source)
        .map_err(|err| format!("failed to stat {}: {err}", source.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "final native payload source is not a file: {}",
            source.display()
        ));
    }
    let _copied = fs::copy(source, destination).map_err(|err| {
        format!(
            "failed to copy {} to {}: {err}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}
