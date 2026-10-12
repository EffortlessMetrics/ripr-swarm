//! Cargo-owned install artifact observations, carried by the existing handle.
use super::qualified_command_with_environment;
use crate::reports::release::candidate_harness::input::revalidate_retained_file;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) struct InstallToolchain {
    pub(super) rustc_command: String,
    pub(super) rustc_verbose_version: String,
    pub(super) cargo_verbose_version: String,
    environment: Vec<(String, String)>,
}

impl InstallToolchain {
    pub(super) fn observe(root: &Path) -> Result<Self, String> {
        let path = std::env::var("PATH")
            .map_err(|error| format!("observe install compiler PATH: {error}"))?;
        let rustc_command = resolve_rustc_entry(root, &path)?;
        let environment = vec![
            ("PATH".to_string(), path),
            ("RUSTC".to_string(), rustc_command.clone()),
            ("CARGO_BUILD_RUSTC".to_string(), rustc_command.clone()),
            ("RUSTC_WRAPPER".to_string(), String::new()),
            ("RUSTC_WORKSPACE_WRAPPER".to_string(), String::new()),
            ("CARGO_BUILD_RUSTC_WRAPPER".to_string(), String::new()),
            (
                "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER".to_string(),
                String::new(),
            ),
        ];
        let values = environment
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let rustc_verbose_version = tool_version(root, Path::new(&rustc_command), &values)?;
        let cargo_verbose_version = tool_version(root, Path::new("cargo"), &values)?;
        Ok(Self {
            rustc_command,
            rustc_verbose_version,
            cargo_verbose_version,
            environment,
        })
    }

    pub(super) fn environment(&self) -> Vec<(&str, &str)> {
        self.environment
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect()
    }

    pub(super) fn revalidate(&self, root: &Path) -> Result<(), String> {
        let environment = self.environment();
        if tool_version(root, Path::new(&self.rustc_command), &environment)?
            != self.rustc_verbose_version
            || tool_version(root, Path::new("cargo"), &environment)? != self.cargo_verbose_version
        {
            return Err("install toolchain changed during compilation".to_string());
        }
        Ok(())
    }

    pub(super) fn configure(
        &self,
        config: &mut toml::map::Map<String, toml::Value>,
    ) -> Result<(), String> {
        let mut build = toml::map::Map::new();
        for (name, value) in [
            ("rustc", self.rustc_command.as_str()),
            ("rustc-wrapper", ""),
            ("rustc-workspace-wrapper", ""),
        ] {
            build.insert(name.to_string(), toml::Value::String(value.to_string()));
        }
        config.insert("build".to_string(), toml::Value::Table(build));
        let Some(toml::Value::Table(environment)) = config.get_mut("env") else {
            return Err("owned install Cargo environment is missing".to_string());
        };
        for (name, value) in &self.environment {
            let mut setting = toml::map::Map::new();
            setting.insert("value".to_string(), toml::Value::String(value.clone()));
            setting.insert("force".to_string(), toml::Value::Boolean(true));
            setting.insert("relative".to_string(), toml::Value::Boolean(false));
            environment.insert(name.clone(), toml::Value::Table(setting));
        }
        Ok(())
    }
}

fn resolve_rustc_entry(root: &Path, path: &str) -> Result<String, String> {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let name = format!("rustc{}", std::env::consts::EXE_SUFFIX);
    for directory in std::env::split_paths(path) {
        if !directory.is_absolute()
            && matches!(
                directory.components().next(),
                Some(std::path::Component::Prefix(_))
            )
        {
            return Err(
                "install compiler PATH has an unsupported drive-relative entry".to_string(),
            );
        }
        let directory = if directory.is_absolute() {
            directory
        } else {
            root.join(directory)
        };
        let entry = directory.join(&name);
        let Ok(metadata) = std::fs::metadata(&entry) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        #[cfg(unix)]
        if metadata.permissions().mode() & 0o111 == 0 {
            continue;
        }
        // Resolve the parent only: rustup's rustc proxy must keep its basename.
        let entry = directory
            .canonicalize()
            .map_err(|error| format!("resolve install compiler directory: {error}"))?
            .join(&name);
        return entry
            .into_os_string()
            .into_string()
            .map_err(|path| format!("selected install compiler path is not UTF-8: {path:?}"));
    }
    Err("install compiler PATH contains no executable rustc entry".to_string())
}

fn tool_version(root: &Path, tool: &Path, environment: &[(&str, &str)]) -> Result<String, String> {
    let output = qualified_command_with_environment(
        tool,
        &["-vV".to_string()],
        root,
        Duration::from_secs(30),
        "observe install toolchain",
        environment,
    )?;
    let value = String::from_utf8(output.stdout)
        .map_err(|error| format!("install toolchain UTF-8: {error}"))?;
    if value.trim().is_empty() {
        return Err("install toolchain observation is empty".to_string());
    }
    Ok(value.trim().to_string())
}

pub(super) struct InstallProducer {
    pub(super) artifact: serde_json::Value,
    pub(super) toolchain: InstallToolchain,
    compiled_path: PathBuf,
}

impl InstallProducer {
    pub(super) fn from_output(
        output: &[u8],
        extracted: &Path,
        target: &Path,
        package: &str,
        toolchain: InstallToolchain,
        installed_bytes: &[u8],
    ) -> Result<Self, String> {
        let manifest = extracted
            .join("Cargo.toml")
            .canonicalize()
            .map_err(|error| format!("resolve installed manifest: {error}"))?;
        let target = target
            .canonicalize()
            .map_err(|error| format!("resolve install compiler target: {error}"))?;
        let messages = output
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
            .map(serde_json::from_slice::<serde_json::Value>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Cargo install artifact protocol: {error}"))?;
        if !messages.last().is_some_and(|message| {
            message["reason"] == "build-finished" && message["success"] == true
        }) {
            return Err("Cargo install lacks terminal successful build-finished".to_string());
        }
        let selected = messages
            .into_iter()
            .filter(|message| {
                message["reason"] == "compiler-artifact"
                    && message["target"]["kind"] == serde_json::json!(["bin"])
                    && message["target"]["name"] == package
                    && message["profile"]["test"] == false
                    && message["executable"].is_string()
            })
            .collect::<Vec<_>>();
        let [artifact] = selected.as_slice() else {
            return Err(format!(
                "Cargo install requires one selected executable artifact; got {}",
                selected.len()
            ));
        };
        let actual_manifest = artifact["manifest_path"]
            .as_str()
            .ok_or_else(|| "selected install artifact lacks manifest".to_string())?;
        if Path::new(actual_manifest).canonicalize().ok().as_ref() != Some(&manifest)
            || artifact["package_id"].as_str().is_none_or(str::is_empty)
            || !artifact["features"]
                .as_array()
                .is_some_and(|features| features.iter().all(serde_json::Value::is_string))
        {
            return Err(
                "selected install artifact has foreign or incomplete package provenance"
                    .to_string(),
            );
        }
        let executable = artifact["executable"]
            .as_str()
            .ok_or_else(|| "selected install artifact lacks executable".to_string())?;
        let compiled_path = Path::new(executable)
            .canonicalize()
            .map_err(|error| format!("resolve selected compiler executable: {error}"))?;
        if !compiled_path.starts_with(&target) {
            return Err(
                "selected compiler executable escapes the owned install target".to_string(),
            );
        }
        let producer = Self {
            artifact: artifact.clone(),
            toolchain,
            compiled_path,
        };
        producer.revalidate(installed_bytes)?;
        Ok(producer)
    }

    pub(super) fn revalidate(&self, installed_bytes: &[u8]) -> Result<(), String> {
        revalidate_retained_file(
            &self.compiled_path,
            installed_bytes,
            "selected compiler executable differs from installed bytes",
        )
    }
}
