use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

#[derive(Serialize)]
struct Stage {
    elapsed_ms: u128,
    name: &'static str,
    input_line: Option<usize>,
    io_error_kind: Option<String>,
}

#[derive(Default, Serialize)]
struct Progress {
    stages: Vec<Stage>,
    truncated: bool,
}

/// Minimum nonsemantic stages that distinguish launch, input completion, a
/// correlated reply, stdin release, child exit, and the owned deadline (#4914).
pub(super) const INVESTIGATION_STAGE_CONTRACT_V1: &[&str] = &[
    "child_spawn_returned",
    "input_line_flushed",
    "matching_reply_received",
    "stdin_released",
    "child_exit_observed",
    "owned_deadline_started",
];

pub(super) fn assert_investigation_stage_contract(observation_json: &str) -> Result<(), String> {
    let value: Value = serde_json::from_str(observation_json)
        .map_err(|error| format!("parse MCP observation JSON: {error}"))?;
    if value.get("timed_out").and_then(Value::as_bool) != Some(false) {
        return Err("investigation attempt must complete within the owned deadline".into());
    }
    if value.get("deadline_seconds").and_then(Value::as_i64) != Some(10) {
        return Err("investigation attempt must retain the 10-second contract".into());
    }
    let stage_names = value
        .pointer("/progress/stages")
        .and_then(Value::as_array)
        .ok_or_else(|| "observation JSON missing progress.stages".to_string())?
        .iter()
        .filter_map(|stage| stage.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    for required in INVESTIGATION_STAGE_CONTRACT_V1 {
        if !stage_names.contains(required) {
            return Err(format!(
                "observation missing required investigation stage `{required}`; saw {stage_names:?}"
            ));
        }
    }
    Ok(())
}

pub(super) struct Observation {
    started: Instant,
    executable: PathBuf,
    executable_sha256: String,
    child_pid: u32,
    hold_input_until_exit: bool,
    retention_directory: Option<PathBuf>,
    progress: Mutex<Progress>,
}

impl Observation {
    pub(super) fn executable_custody(path: &Path) -> Result<(PathBuf, String), String> {
        let absolute = path
            .canonicalize()
            .map_err(|error| format!("resolve worktree-built MCP executable: {error}"))?;
        let mut file = std::fs::File::open(&absolute)
            .map_err(|error| format!("open worktree-built MCP executable: {error}"))?;
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|error| format!("hash worktree-built MCP executable: {error}"))?;
            if count == 0 {
                break;
            }
            let bytes = buffer
                .get(..count)
                .ok_or_else(|| "executable reader exceeded its buffer".to_string())?;
            hash.update(bytes);
        }
        Ok((absolute, format!("{:x}", hash.finalize())))
    }

    pub(super) fn new(
        started: Instant,
        executable: PathBuf,
        executable_sha256: String,
        child_pid: u32,
        hold_input_until_exit: bool,
        retention_directory: Option<PathBuf>,
    ) -> Self {
        Self {
            started,
            executable,
            executable_sha256,
            child_pid,
            hold_input_until_exit,
            retention_directory,
            progress: Mutex::new(Progress::default()),
        }
    }

    pub(super) fn record(
        &self,
        name: &'static str,
        input_line: Option<usize>,
        error: Option<std::io::ErrorKind>,
    ) {
        if let Ok(mut progress) = self.progress.lock() {
            if progress.stages.len() < 64 {
                progress.stages.push(Stage {
                    elapsed_ms: self.started.elapsed().as_millis(),
                    name,
                    input_line,
                    io_error_kind: error.map(|kind| format!("{kind:?}")),
                });
            } else {
                progress.truncated = true;
            }
        }
    }

    /// Test custody only: absolute executable identity is never MCP wire data.
    pub(super) fn finish(&self, timed_out: bool, exit_code: Option<i32>) -> Result<String, String> {
        let progress = self
            .progress
            .lock()
            .map_err(|_error| "MCP observation lock unavailable".to_string())?;
        let value = serde_json::json!({
            "executable": self.executable,
            "executable_sha256_before_launch": self.executable_sha256,
            "child_pid": self.child_pid,
            "elapsed_ms": self.started.elapsed().as_millis(),
            "deadline_seconds": 10,
            "timed_out": timed_out,
            "exit_code": exit_code,
            "hold_input_until_exit": self.hold_input_until_exit,
            "progress": &*progress,
            "scope": "Observed test process and IO stages; no startup or protocol failure cause inferred"
        });
        let encoded = serde_json::to_string_pretty(&value)
            .map_err(|error| format!("encode MCP observation: {error}"))?;
        if let Some(directory) = self
            .retention_directory
            .clone()
            .or_else(|| std::env::var_os("RIPR_3088_STDIO_OBSERVATIONS").map(PathBuf::from))
        {
            let path = directory.join(format!("child-{}.json", self.child_pid));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| format!("create MCP process observation: {error}"))?;
            std::io::Write::write_all(&mut file, encoded.as_bytes())
                .map_err(|error| format!("retain MCP process observation: {error}"))?;
        }
        Ok(encoded)
    }
}
