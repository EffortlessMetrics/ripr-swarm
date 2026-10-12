//! Cargo-owned install artifact observations, carried by the existing handle.
use super::qualified_command;
use crate::reports::release::candidate_harness::input::revalidate_retained_file;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Eq, PartialEq)]
pub(super) struct InstallToolchain {
    pub(super) rustc_verbose_version: String,
    pub(super) cargo_verbose_version: String,
}

impl InstallToolchain {
    pub(super) fn observe(root: &Path) -> Result<Self, String> {
        Ok(Self {
            rustc_verbose_version: tool_version(root, "rustc")?,
            cargo_verbose_version: tool_version(root, "cargo")?,
        })
    }
}

fn tool_version(root: &Path, tool: &str) -> Result<String, String> {
    let output = qualified_command(
        Path::new(tool),
        &["-vV".to_string()],
        root,
        Duration::from_secs(30),
        "observe install toolchain",
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
