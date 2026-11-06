//! First installed Rust consumer: public entrypoint observations only (#4516).
//! This deliberately does not emit a language acceptance row or infer repair,
//! project verification, static movement, or receipt status from diagnostics.

use super::{CandidateExecution, archive::InstalledCandidate};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

const INPUTS: [&str; 3] = ["Cargo.toml", "src/lib.rs", "tests/tier_boundary.rs"];

/// The caller owns an exclusive, empty parent within its qualification evidence
/// root. Neither a supplied executable nor Legacy execution can enter this API.
pub(crate) fn observe(candidate: &InstalledCandidate, parent: &Path) -> Result<Value, String> {
    candidate.revalidate()?;
    let selected = parent.join("Rust selected é");
    let foreign = parent.join("foreign launch");
    // Refuse reused roots instead of accepting planted diagnostic artifacts.
    std::fs::create_dir(parent)
        .map_err(|error| format!("exclusive Rust entrypoint evidence root: {error}"))?;
    let inputs = materialize(&selected, |path| candidate.rust_journey_file(path))?;
    std::fs::create_dir(&foreign)
        .map_err(|error| format!("create Rust foreign launch root: {error}"))?;
    let selected = selected
        .canonicalize()
        .map_err(|error| format!("resolve Rust selected root: {error}"))?;
    let selected_arg = selected
        .to_str()
        .ok_or_else(|| "Rust selected root is not UTF-8".to_string())?;
    let execution = CandidateExecution::Qualified(candidate);
    let mut commands = Vec::new();
    for (index, argv) in [
        vec!["doctor".to_string(), "--help".to_string()],
        vec![
            "doctor".to_string(),
            "--root".to_string(),
            selected_arg.to_string(),
            "--json".to_string(),
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let result = execution.run(&argv, &foreign, "installed Rust public entrypoint")?;
        // Retain ordinary diagnostic failures as observations. A failed doctor
        // must never acquire a positive repair/qualification interpretation.
        let record = json!({"argv": argv, "cwd": foreign, "exit": result.status,
            "success": result.success, "stdout": result.stdout, "stderr": result.stderr});
        std::fs::write(
            parent.join(format!("command-{index}.json")),
            serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("retain installed Rust entrypoint observation: {error}"))?;
        if index == 0 {
            if !result.success {
                return Err(
                    "installed Rust doctor help failed; raw observation retained".to_string(),
                );
            }
        } else {
            validate_doctor(&result.stdout, result.status, selected_arg)?;
        }
        commands.push(record);
    }
    candidate.revalidate()?;
    Ok(json!({
        "schema_version": "installed_rust_entrypoint_observation.v1",
        "scope": "preparation_only",
        "fixture_snapshot": "head-no-ignore",
        "fixture_git_history": "not_materialized",
        "selection": "not_run",
        "edit_cage": "not_run",
        "selected_root": selected,
        "source_inputs": inputs,
        "commands": commands,
        "candidate_custody": candidate.custody_json(),
        "repair_journey": "not_run",
        "project_verification": "not_run",
        "static_movement": "not_run",
        "receipt": "not_run",
        "installed_acceptance": "not_established"
    }))
}

fn validate_doctor(stdout: &str, exit: Option<i32>, selected: &str) -> Result<(), String> {
    let report: Value = serde_json::from_str(stdout).map_err(|error| {
        format!("installed Rust doctor JSON: {error}; raw observation retained")
    })?;
    if report.get("tool").and_then(Value::as_str) != Some("ripr")
        || report.get("root").and_then(Value::as_str) != Some(selected)
        || !report
            .get("checks")
            .and_then(Value::as_array)
            .is_some_and(|checks| !checks.is_empty())
        || !matches!(
            (exit, report.get("status").and_then(Value::as_str)),
            (Some(0), Some("pass")) | (Some(3), Some("fail"))
        )
    {
        return Err("installed Rust doctor did not report nonempty selected-root diagnostics with a consistent exit/status; raw observation retained".to_string());
    }
    Ok(())
}

fn materialize(
    root: &Path,
    mut read: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<Value>, String> {
    // Read every admitted input before writing any fixture bytes. Missing or
    // unsupported input cannot leave a seemingly complete selected repository.
    let inputs = INPUTS
        .into_iter()
        .map(|path| Ok((path, read(path)?)))
        .collect::<Result<Vec<_>, String>>()?;
    std::fs::create_dir(root).map_err(|error| format!("exclusive Rust selected root: {error}"))?;
    for directory in ["src", "tests"] {
        std::fs::create_dir(root.join(directory))
            .map_err(|error| format!("create Rust fixture directory: {error}"))?;
    }
    let mut records = Vec::new();
    for (path, bytes) in inputs {
        std::fs::write(root.join(path), &bytes)
            .map_err(|error| format!("write admitted Rust fixture {path}: {error}"))?;
        records.push(json!({"path": path, "sha256": format!("{:x}", Sha256::digest(&bytes))}));
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Result<Self, String> {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos();
            let parent = std::env::temp_dir();
            std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
            let root = parent.join(format!("rust-entrypoint-{}-{stamp}", std::process::id()));
            std::fs::create_dir(&root).map_err(|error| error.to_string())?;
            Ok(Self(root))
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn doctor(root: &str) -> Value {
        json!({"tool": "ripr", "root": root, "status": "pass", "checks": [{"name": "root", "status": "pass"}]})
    }

    #[test]
    fn wrong_root_diagnostics_are_refused() -> Result<(), String> {
        if validate_doctor(&doctor("/decoy").to_string(), Some(0), "/selected").is_ok() {
            return Err("foreign root doctor was accepted".to_string());
        }
        validate_doctor(&doctor("/selected").to_string(), Some(0), "/selected")
    }

    #[test]
    fn empty_checks_and_inconsistent_status_are_refused() -> Result<(), String> {
        let mut empty = doctor("/selected");
        empty["checks"] = json!([]);
        for (report, exit) in [
            (empty, Some(0)),
            (doctor("/selected"), Some(3)),
            (doctor("/selected"), None),
        ] {
            if validate_doctor(&report.to_string(), exit, "/selected").is_ok() {
                return Err("empty or inconsistent doctor observation was accepted".to_string());
            }
        }
        Ok(())
    }

    #[test]
    fn diagnostic_findings_remain_a_completed_diagnosis() -> Result<(), String> {
        let mut findings = doctor("/selected");
        findings["status"] = json!("fail");
        validate_doctor(&findings.to_string(), Some(3), "/selected")
    }

    #[test]
    fn retained_inputs_materialize_without_ignore_or_artifacts() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let root = scratch.0.join("selected é");
        let records = materialize(&root, |path| Ok(path.as_bytes().to_vec()))?;
        if records.len() != 3 || root.join(".gitignore").exists() || root.join("target").exists() {
            return Err("Rust preparation invented inputs or artifacts".to_string());
        }
        for path in INPUTS {
            if std::fs::read(root.join(path)).map_err(|error| error.to_string())? != path.as_bytes()
            {
                return Err(format!("Rust input bytes changed: {path}"));
            }
        }
        if materialize(&root, |_| Ok(b"replacement".to_vec())).is_ok()
            || std::fs::read(root.join("Cargo.toml")).map_err(|error| error.to_string())?
                != b"Cargo.toml"
        {
            return Err("occupied Rust root was reused or overwritten".to_string());
        }
        Ok(())
    }

    #[test]
    fn missing_input_creates_no_selected_root() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let root = scratch.0.join("missing-input");
        let result = materialize(&root, |_| Err("missing committed input".to_string()));
        match result {
            Err(error) if error == "missing committed input" && !root.exists() => Ok(()),
            other => Err(format!(
                "missing input did not fail before creation: {other:?}"
            )),
        }
    }
}
