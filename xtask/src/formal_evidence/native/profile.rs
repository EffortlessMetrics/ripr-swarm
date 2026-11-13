//! Pinned native profile facts, shared by live capture and passive replay.
use super::{digest, require};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Aggregate observed verification processes without borrowing individual
/// harness verdicts. The live producer and retained replay use this same law.
pub fn aggregate_process(
    invocations: &[super::Invocation],
    raw_index_digest: &str,
) -> crate::formal_evidence::Process {
    use crate::formal_evidence::{Cleanup, Process, ProcessState};
    let verifications: Vec<_> = invocations
        .iter()
        .filter(|call| call.phase == "verification")
        .collect();
    let started = !verifications.is_empty();
    let timed_out = verifications.iter().any(|call| call.timed_out);
    let all_exited = started
        && verifications
            .iter()
            .all(|call| call.capture_complete && call.exit_code.is_some());
    let exit_code = all_exited.then(|| {
        verifications
            .iter()
            .filter_map(|call| call.exit_code)
            .find(|code| *code != 0)
            .unwrap_or(0)
    });
    Process {
        state: if timed_out {
            ProcessState::TimedOut
        } else if !started {
            ProcessState::NotStarted
        } else if exit_code == Some(0) {
            ProcessState::Completed
        } else {
            ProcessState::Failed
        },
        exit_code,
        timed_out,
        raw_output_digest: started.then(|| raw_index_digest.to_string()),
        cleanup_state: if !started {
            Cleanup::NotApplicable
        } else if invocations.iter().all(|call| call.capture_complete) {
            Cleanup::Complete
        } else {
            Cleanup::NotObserved
        },
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstalledToolchain {
    pub installation: String,
    pub rustc: String,
    pub rustup_toolchain: String,
    pub target: String,
    pub binaries: BTreeMap<String, String>,
}

pub fn native_tool_header(stdout: &str) -> Result<(), String> {
    let kani: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("Kani Rust Verifier "))
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    let cbmc: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("CBMC "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|version| *version != "version")
        .collect();
    if kani != ["0.68.0"] || cbmc.is_empty() || cbmc.iter().any(|version| *version != "6.11.0") {
        return Err("native tool headers do not match pinned Kani0.68.0/CBMC6.11.0".to_string());
    }
    Ok(())
}

pub fn validate_cargo_metadata(
    bytes: &[u8],
    root: &Path,
    sources: &BTreeMap<String, String>,
) -> Result<(), String> {
    let value: serde_json::Value = super::super::parse_json(bytes)?;
    let object = value.as_object().ok_or("Cargo metadata is not an object")?;
    if object.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err("unsupported Cargo metadata version".to_string());
    }
    let workspace = object
        .get("workspace_root")
        .and_then(serde_json::Value::as_str)
        .ok_or("Cargo workspace root absent")?;
    if Path::new(workspace) != root {
        return Err("Cargo resolved another workspace".to_string());
    }
    let packages = object
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or("Cargo package metadata absent")?;
    if object.get("resolve").is_none_or(serde_json::Value::is_null) {
        return Err("Cargo resolved feature/dependency graph absent".to_string());
    }
    let mut local = BTreeSet::new();
    if object
        .get("workspace_metadata")
        .is_some_and(|metadata| metadata.get("kani").is_some())
    {
        return Err("workspace Kani metadata is outside the admitted profile".to_string());
    }
    for package in packages {
        if package.get("source").is_none_or(serde_json::Value::is_null) {
            if package
                .get("metadata")
                .is_some_and(|metadata| metadata.get("kani").is_some())
            {
                return Err("package Kani metadata is outside the admitted profile".to_string());
            }
            let path = Path::new(
                package
                    .get("manifest_path")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("Cargo package manifest absent")?,
            );
            let relative = path
                .strip_prefix(root)
                .map_err(|error| {
                    format!("Cargo path package escapes captured repository: {error}")
                })?
                .to_str()
                .ok_or("Cargo package path is not UTF-8")?
                .replace('\\', "/");
            if !sources.contains_key(&relative) {
                return Err("Cargo path package was not captured".to_string());
            }
            local.insert(
                package
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("Cargo package ID absent")?,
            );
        }
    }
    let members = object
        .get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .ok_or("Cargo workspace members absent")?;
    if members.is_empty()
        || members
            .iter()
            .any(|member| member.as_str().is_none_or(|id| !local.contains(id)))
    {
        return Err("Cargo workspace member is outside captured package graph".to_string());
    }
    Ok(())
}

/// Observed bundle facts remain local custody, never a signed distribution claim.
pub fn validate_installed_toolchain(value: &InstalledToolchain) -> Result<(), String> {
    require(
        Path::new(&value.installation).is_absolute(),
        "Kani installation must be absolute",
    )?;
    require(
        value.target == "x86_64-unknown-linux-gnu"
            && value.rustup_toolchain == "nightly-2026-08-21-x86_64-unknown-linux-gnu"
            && value.rustc == "rustc 1.100.0-nightly (8925ea358 2026-08-20)",
        "unsupported Kani bundled compiler metadata",
    )?;
    require(
        value
            .binaries
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == BTreeSet::from(["kani-compiler", "kani-driver", "cbmc"]),
        "Kani bundle binary commitments are incomplete",
    )?;
    for value in value.binaries.values() {
        digest(value, "Kani bundle binary")?;
    }
    Ok(())
}

/// Relevant configuration structure is retained without copying HTTP tokens,
/// proxy credentials or unrelated transport values into uploaded evidence.
/// Full-file digests commit the bytes observed by the producer. These local
/// observations do not authenticate their provenance or interpret all Cargo.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationSnapshot {
    pub inputs: BTreeMap<String, String>,
    pub cargo_config_keys: BTreeMap<String, Vec<Vec<String>>>,
}

pub fn validate_cargo_configuration_keys(keys: &[Vec<String>]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for key in keys {
        require(
            !key.is_empty() && key.iter().all(|k| !k.is_empty()) && seen.insert(key),
            "malformed or duplicate Cargo configuration key",
        )?;
        let root = key[0].as_str();
        require(
            matches!(
                root,
                "alias"
                    | "env"
                    | "build"
                    | "net"
                    | "http"
                    | "registry"
                    | "registries"
                    | "source"
                    | "term"
            ),
            "unknown or proof-affecting Cargo configuration is outside the supported profile",
        )?;
        if let Some(setting) = key.get(1).map(String::as_str) {
            require(
                !(root == "alias" && setting == "kani"),
                "a Cargo kani alias changes the executor",
            )?;
            require(
                root != "env" || matches!(setting, "TMP" | "TEMP" | "TMPDIR"),
                "custom Cargo environment is not admitted",
            )?;
            require(
                root != "build" || matches!(setting, "jobs" | "incremental" | "target-dir"),
                "custom build target/compiler/flags are not admitted",
            )?;
        }
    }
    Ok(())
}

pub fn validate_configuration(
    value: &ConfigurationSnapshot,
    sources: &super::SourceManifest,
) -> Result<(), String> {
    for (path, expected) in sources {
        require(
            value.inputs.get(path) == Some(expected),
            "configuration omitted or substituted a captured source input",
        )?;
    }
    for (name, commitment) in &value.inputs {
        digest(commitment, "configuration input")?;
        if let Some(path) = name.strip_prefix("cargo-config:") {
            require(
                Path::new(path).is_absolute(),
                "Cargo configuration locator is not absolute",
            )?;
            let keys = value
                .cargo_config_keys
                .get(name)
                .ok_or("Cargo configuration structural observation is absent")?;
            validate_cargo_configuration_keys(keys)?;
        } else if let Some(name) = name.strip_prefix("locator:") {
            require(
                matches!(
                    name,
                    "KANI_HOME"
                        | "CARGO_HOME"
                        | "RUSTUP_HOME"
                        | "RUSTUP_TOOLCHAIN"
                        | "PATH"
                        | "LD_LIBRARY_PATH"
                        | "DYLD_LIBRARY_PATH"
                ),
                "unknown configuration locator",
            )?;
        } else {
            require(
                sources.contains_key(name),
                "unsupported verification environment or unrecognized configuration input",
            )?;
        }
    }
    require(
        value
            .cargo_config_keys
            .keys()
            .all(|key| key.starts_with("cargo-config:") && value.inputs.contains_key(key)),
        "Cargo configuration structure lacks its byte commitment",
    )
}

/// One argv constructor for both maintained entrypoints and offline admission.
pub fn verification_arguments(
    package: &str,
    harness: Option<&str>,
    unwind: Option<u32>,
    per_subject_seconds: u64,
) -> Vec<String> {
    let mut command = vec!["kani".to_string(), "-p".to_string(), package.to_string()];
    if let Some(harness) = harness {
        command.extend(["--harness".to_string(), harness.to_string()]);
    }
    if let Some(unwind) = unwind {
        command.extend(["--default-unwind".to_string(), unwind.to_string()]);
    }
    command.extend([
        "-Z".to_string(),
        "unstable-options".to_string(),
        "--harness-timeout".to_string(),
        format!("{per_subject_seconds}s"),
    ]);
    command
}

pub fn inventory_arguments(package: &str, root: &Path) -> Vec<String> {
    vec![
        "kani".to_string(),
        "-p".to_string(),
        package.to_string(),
        "--manifest-path".to_string(),
        root.join("Cargo.toml").display().to_string(),
        "list".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ]
}
