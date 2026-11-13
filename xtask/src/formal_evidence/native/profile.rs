//! Pinned native tool identity and process-result laws for shared interpretation.
//! These pure checks inspect supplied observations and grant no execution authority.

use super::{digest, require};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const KANI_VERSION: &str = "0.68.0";
const CBMC_VERSION: &str = "6.11.0";

/// An explicit provider configuration request, never an observation that the
/// named compiler, solver, target, or flags actually ran.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredKaniProfile {
    pub kani_version: String,
    pub cbmc_version: String,
    pub target: String,
    pub configuration_policy: String,
    pub default_unwind: u64,
    pub lib: bool,
    pub default_features: bool,
    pub features: Vec<String>,
    pub all_features: bool,
}

impl DeclaredKaniProfile {
    pub fn for_policy(policy: &str) -> Result<Self, String> {
        match policy {
            "kani-0.68-defaults-v1" => Ok(Self::pinned()),
            "kani-0.68-rust-only-v1" => Ok(Self::rust_only()),
            _ => Err("unsupported native configuration policy".to_string()),
        }
    }

    pub fn pinned() -> Self {
        Self {
            kani_version: KANI_VERSION.to_string(),
            cbmc_version: CBMC_VERSION.to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            configuration_policy: "kani-0.68-defaults-v1".to_string(),
            default_unwind: 32,
            lib: true,
            default_features: true,
            features: Vec::new(),
            all_features: false,
        }
    }

    /// A distinct reviewed request profile. This does not claim default-feature
    /// equivalence, Cargo metadata admission, or execution of any native claim.
    pub fn rust_only() -> Self {
        Self {
            configuration_policy: "kani-0.68-rust-only-v1".to_string(),
            default_features: false,
            features: vec!["lang-rust".to_string()],
            ..Self::pinned()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self != &Self::pinned() && self != &Self::rust_only() {
            return Err(
                "declared native profile differs from a pinned ordinary Kani profile".to_string(),
            );
        }
        Ok(())
    }

    fn feature_arguments(&self) -> Vec<String> {
        let mut args = Vec::new();
        if !self.default_features {
            args.push("--no-default-features".to_string());
        }
        if !self.features.is_empty() {
            args.extend(["--features".to_string(), self.features.join(",")]);
        }
        args
    }

    pub fn metadata_arguments(&self, package: &str, root: &Path) -> Vec<String> {
        let mut args = vec![
            "metadata".to_string(),
            "--locked".to_string(),
            "--format-version".to_string(),
            "1".to_string(),
            "--manifest-path".to_string(),
            root.join(format!("crates/{package}/Cargo.toml"))
                .display()
                .to_string(),
            "--filter-platform".to_string(),
            self.target.clone(),
        ];
        args.extend(self.feature_arguments());
        args
    }

    pub fn inventory_arguments(&self, package: &str, root: &Path) -> Vec<String> {
        let mut args = vec![
            "kani".to_string(),
            "-p".to_string(),
            package.to_string(),
            "--lib".to_string(),
            "--manifest-path".to_string(),
            root.join("Cargo.toml").display().to_string(),
        ];
        args.extend(self.feature_arguments());
        args.extend([
            "list".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ]);
        args
    }

    pub fn verification_arguments(
        &self,
        package: &str,
        harness: &str,
        seconds: u64,
    ) -> Vec<String> {
        let mut args = vec![
            "kani".to_string(),
            "-p".to_string(),
            package.to_string(),
            "--lib".to_string(),
        ];
        args.extend(self.feature_arguments());
        args.extend([
            "--harness".to_string(),
            harness.to_string(),
            "--exact".to_string(),
            "--default-unwind".to_string(),
            self.default_unwind.to_string(),
            "--output-format".to_string(),
            "regular".to_string(),
            "--verbose".to_string(),
            "-Z".to_string(),
            "unstable-options".to_string(),
            "--harness-timeout".to_string(),
            format!("{seconds}s"),
        ]);
        args
    }
}

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
            .all(|call| call.capture_complete && call.exit_code.is_some() && call.error.is_none());
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
    if kani != [KANI_VERSION]
        || cbmc.is_empty()
        || cbmc.iter().any(|version| *version != CBMC_VERSION)
    {
        return Err("native tool headers do not match pinned Kani0.68.0/CBMC6.11.0".to_string());
    }
    Ok(())
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

/// Supplied bundle facts pass structure/pin checks, not executable authentication.
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
/// Full-file digests commit the asserted bytes. These supplied records do not
/// authenticate their provenance, prove complete discovery, or interpret all Cargo.
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

/// Validate the selected resolved Cargo graph from supplied retained bytes.
/// Unselected workspace packages do not enlarge or replace that denominator.
/// This checks the Rust-only request profile, not Cargo/compiler provenance.
pub fn validate_cargo_metadata(
    bytes: &[u8],
    root: &Path,
    sources: &super::SourceManifest,
    package: &str,
    profile: &DeclaredKaniProfile,
) -> Result<(), String> {
    require(
        profile == &DeclaredKaniProfile::rust_only(),
        "retained Cargo replay requires the distinct Rust-only profile",
    )?;
    let value: serde_json::Value = super::super::parse_json(bytes)?;
    require(
        value["version"].as_u64() == Some(1),
        "unsupported Cargo metadata version",
    )?;
    require(
        value["workspace_root"]
            .as_str()
            .is_some_and(|path| Path::new(path) == root),
        "Cargo resolved another workspace",
    )?;
    require(
        value["workspace_metadata"].get("kani").is_none(),
        "workspace Kani metadata is outside the admitted profile",
    )?;
    let mut packages = BTreeMap::new();
    for item in value["packages"]
        .as_array()
        .ok_or("Cargo packages absent")?
    {
        let id = item["id"].as_str().ok_or("Cargo package ID absent")?;
        require(
            packages.insert(id, item).is_none(),
            "duplicate Cargo package ID",
        )?;
    }
    let mut nodes = BTreeMap::new();
    for node in value["resolve"]["nodes"]
        .as_array()
        .ok_or("Cargo resolved feature/dependency graph absent")?
    {
        let id = node["id"].as_str().ok_or("Cargo resolved node ID absent")?;
        require(
            nodes.insert(id, node).is_none(),
            "duplicate Cargo resolved node",
        )?;
    }
    let selected: Vec<_> = packages
        .iter()
        .filter(|(_, item)| item["name"].as_str() == Some(package) && item["source"].is_null())
        .collect();
    let [(selected_id, selected_package)] = selected.as_slice() else {
        return Err("selected Cargo package is missing or ambiguous".to_string());
    };
    let expected_manifest = root.join(format!("crates/{package}/Cargo.toml"));
    require(
        selected_package["manifest_path"]
            .as_str()
            .is_some_and(|path| Path::new(path) == expected_manifest),
        "selected Cargo manifest differs from prepared package",
    )?;
    let mut pending = vec![**selected_id];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let item = packages
            .get(id)
            .ok_or("resolved Cargo dependency package absent")?;
        let node = nodes
            .get(id)
            .ok_or("resolved Cargo dependency node absent")?;
        if item["source"].is_null() {
            require(
                item["metadata"].get("kani").is_none(),
                "package Kani metadata is outside the admitted profile",
            )?;
            let path = Path::new(
                item["manifest_path"]
                    .as_str()
                    .ok_or("Cargo manifest absent")?,
            );
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("Cargo local dependency escapes repository: {error}"))?
                .to_str()
                .ok_or("Cargo local manifest is not UTF-8")?
                .replace('\\', "/");
            require(
                sources.contains_key(&relative),
                "Cargo local dependency manifest was not captured",
            )?;
            require(
                matches!(item["name"].as_str(), Some("ripr" | "ripr-contract")),
                "unsupported local package in native proof graph",
            )?;
            let libraries: Vec<_> = item["targets"]
                .as_array()
                .ok_or("Cargo package targets absent")?
                .iter()
                .filter(|target| target["kind"] == serde_json::json!(["lib"]))
                .collect();
            let [library] = libraries.as_slice() else {
                return Err("Cargo local library target absent or ambiguous".to_string());
            };
            let library_source = format!(
                "crates/{}/src/lib.rs",
                item["name"].as_str().ok_or("Cargo package name absent")?
            );
            require(
                library["src_path"]
                    .as_str()
                    .is_some_and(|path| Path::new(path) == root.join(&library_source))
                    && sources.contains_key(&library_source),
                "Cargo local library source differs from prepared package",
            )?;
            let features = node["features"]
                .as_array()
                .ok_or("Cargo resolved features absent")?;
            require(
                features == &[serde_json::Value::String("lang-rust".to_string())],
                "Cargo resolved features differ from the Rust-only profile",
            )?;
        }
        for dependency in node["deps"]
            .as_array()
            .ok_or("Cargo resolved dependencies absent")?
        {
            pending.push(
                dependency["pkg"]
                    .as_str()
                    .ok_or("Cargo resolved dependency ID absent")?,
            );
        }
    }
    Ok(())
}
