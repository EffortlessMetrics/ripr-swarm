//! Owned, disposable execution of one corpus case against the exact binary.
//!
//! Process custody (own process group, tree termination, bounded drains) comes
//! from the shared `run` helpers. This module adds the workspace, the
//! environment boundary, the outer deadline classification and cleanup.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::assess::{self, Assessment, Observed};
use super::corpus::Case;
use super::model::{InputDigest, Observation, Row, RowResult, Subjects};
use crate::blind_journey::sha256_hex;
use crate::run;

#[cfg(unix)]
pub(crate) const CHILD_PATH: &str = "/usr/bin:/bin";
#[cfg(not(unix))]
pub(crate) const CHILD_PATH: &str = "";

/// Placeholder for the owned case root in every retained string.
pub(crate) const ROOT_TOKEN: &str = "<case-root>";

pub(crate) struct Binary {
    pub(crate) path: PathBuf,
}

/// Environment variables the child inherits from the parent are limited to
/// what the corpus does not set itself; `RIPR_*` is removed so ambient
/// configuration cannot change a row.
pub(crate) fn removed_env() -> Vec<String> {
    let mut names: Vec<String> = std::env::vars()
        .map(|(name, _)| name)
        .filter(|name| name.starts_with("RIPR_"))
        .collect();
    names.sort();
    names
}

pub(crate) fn environment_boundary() -> Vec<String> {
    vec![
        format!("child PATH is exactly `{CHILD_PATH}`; the binary is launched by absolute path"),
        "HOME, TMPDIR and XDG_{CACHE,CONFIG,DATA}_HOME point inside the owned case root"
            .to_string(),
        "NO_COLOR=1 and RUST_BACKTRACE=0".to_string(),
        "every RIPR_* variable present in the parent environment is removed from the child"
            .to_string(),
        "other inherited variables are not scrubbed".to_string(),
    ]
}

pub(crate) fn write_case_files(root: &Path, case: &Case) -> Result<Vec<InputDigest>, String> {
    let mut digests = Vec::new();
    for (relative, bytes) in &case.files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        fs::write(&path, bytes)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        digests.push(InputDigest {
            path: relative.clone(),
            bytes: bytes.len(),
            sha256: sha256_hex(bytes),
        });
    }
    digests.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(digests)
}

pub(crate) fn run_case(
    binary: &Binary,
    work_root: &Path,
    case: &Case,
    deadline: Duration,
) -> (Row, Observation) {
    let root = work_root.join(&case.id);
    let mut row = base_row(case, deadline);
    let mut observation = Observation {
        id: case.id.clone(),
        elapsed_ms: 0,
    };

    if root.exists() {
        row.result = RowResult::InstrumentFailure;
        row.detail = "owned case root already existed; refusing to reuse it".to_string();
        row.cleanup = "retained_preexisting_root".to_string();
        return (row, observation);
    }
    match write_case_files(&root, case) {
        Ok(inputs) => row.inputs = inputs,
        Err(error) => {
            row.result = RowResult::InstrumentFailure;
            row.detail = error;
            row.cleanup = cleanup(&root);
            return (row, observation);
        }
    }
    row.subjects.workspace_files = case.files.len().saturating_sub(1);
    row.subjects.unchanged_outside_diff = usize::from(case.hostile.is_some());

    let outcome = launch(binary, &root, case, deadline);
    match outcome {
        Err(error) => {
            row.result = RowResult::InstrumentFailure;
            row.detail = error;
        }
        Ok(launched) => {
            observation.elapsed_ms = launched.duration.as_millis();
            let root_text = root.to_string_lossy().to_string();
            let stdout_text =
                String::from_utf8_lossy(&launched.stdout).replace(&root_text, ROOT_TOKEN);
            let stderr_text =
                String::from_utf8_lossy(&launched.stderr).replace(&root_text, ROOT_TOKEN);
            row.stdout_sha256 = sha256_hex(stdout_text.as_bytes());
            row.stderr_sha256 = sha256_hex(stderr_text.as_bytes());
            row.termination = launched.termination.clone();
            let artifact_text = case.artifact.as_ref().and_then(|relative| {
                fs::read(root.join(relative))
                    .ok()
                    .map(|bytes| String::from_utf8_lossy(&bytes).replace(&root_text, ROOT_TOKEN))
            });
            let assessment = if launched.timed_out {
                Assessment::fail(
                    RowResult::NonterminalDeadline,
                    format!("outer deadline of {}s reached", deadline.as_secs()),
                )
            } else if launched.aborted {
                Assessment::fail(
                    RowResult::ProcessAbort,
                    format!(
                        "process ended without an exit code ({})",
                        launched.termination
                    ),
                )
            } else {
                assess::assess(
                    case,
                    &Observed {
                        exit_code: launched.exit_code,
                        stdout: &stdout_text,
                        stderr: &stderr_text,
                        artifact: artifact_text.as_deref(),
                    },
                )
            };
            row.result = assessment.result;
            row.detail = assessment.detail;
            row.limitation = assessment.limitation;
            row.artifact_sha256 = assessment
                .artifact_projection
                .as_ref()
                .map(|text| sha256_hex(text.as_bytes()));
            row.subjects = Subjects {
                findings_reported: assessment.findings_reported,
                findings_suppressed: assessment.findings_suppressed,
                declined_files: assessment.declined_files,
                ..row.subjects
            };
        }
    }
    row.cleanup = cleanup(&root);
    if row.cleanup != "removed" && row.result.is_pass() {
        row.result = RowResult::InstrumentFailure;
        row.detail = format!("cleanup did not remove the case root ({})", row.cleanup);
    }
    (row, observation)
}

fn base_row(case: &Case, deadline: Duration) -> Row {
    Row {
        id: case.id.clone(),
        family: case.family,
        role: case.role,
        shape: case.shape.to_string(),
        expected: case.expect.describe(),
        result: RowResult::NotRun,
        detail: String::new(),
        argv: case.args.clone(),
        cwd: ROOT_TOKEN.to_string(),
        deadline_seconds: deadline.as_secs(),
        termination: "not_launched".to_string(),
        limitation: None,
        inputs: Vec::new(),
        subjects: Subjects {
            workspace_files: 0,
            unchanged_outside_diff: 0,
            findings_reported: 0,
            findings_suppressed: 0,
            declined_files: 0,
        },
        stdout_sha256: String::new(),
        stderr_sha256: String::new(),
        artifact_sha256: None,
        cleanup: "not_created".to_string(),
    }
}

struct Launched {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    duration: Duration,
    timed_out: bool,
    aborted: bool,
    termination: String,
    exit_code: Option<i32>,
}

fn launch(
    binary: &Binary,
    root: &Path,
    case: &Case,
    deadline: Duration,
) -> Result<Launched, String> {
    let home = root.join(".termination-home");
    let tmp = home.join("tmp");
    fs::create_dir_all(&tmp)
        .map_err(|error| format!("failed to create {}: {error}", tmp.display()))?;
    let home_text = home.to_string_lossy().to_string();
    let tmp_text = tmp.to_string_lossy().to_string();
    let cache = home.join("cache").to_string_lossy().to_string();
    let config = home.join("config").to_string_lossy().to_string();
    let data = home.join("data").to_string_lossy().to_string();
    let envs: Vec<(&str, &str)> = vec![
        ("PATH", CHILD_PATH),
        ("HOME", &home_text),
        ("TMPDIR", &tmp_text),
        ("XDG_CACHE_HOME", &cache),
        ("XDG_CONFIG_HOME", &config),
        ("XDG_DATA_HOME", &data),
        ("NO_COLOR", "1"),
        ("RUST_BACKTRACE", "0"),
    ];
    let removed = removed_env();
    let removed_refs: Vec<&str> = removed.iter().map(String::as_str).collect();
    let timed = run::capture_bytes_in_dir_with_timeout(
        &binary.path,
        &case.args,
        root,
        &envs,
        &removed_refs,
        deadline,
        &format!("termination case {} via {}", case.id, binary.path.display()),
    )?;
    let (aborted, termination) = match timed.status {
        Some(status) if timed.timed_out => (false, format!("deadline_kill ({status})")),
        Some(status) => match status.code() {
            Some(code) if is_abort_code(code) => (true, format!("exit_code {code} (abort)")),
            Some(code) => (false, format!("exit_code {code}")),
            None => (true, signal_description(&status)),
        },
        None => (false, "unknown".to_string()),
    };
    Ok(Launched {
        stdout: timed.stdout,
        stderr: timed.stderr,
        duration: timed.duration,
        timed_out: timed.timed_out,
        aborted,
        termination,
        exit_code: timed.status.and_then(|status| status.code()),
    })
}

/// Windows stack overflow (`STATUS_STACK_OVERFLOW`) and fast-fail abort codes.
fn is_abort_code(code: i32) -> bool {
    let unsigned = code.cast_unsigned();
    unsigned == 0xC000_00FD || unsigned == 0xC000_0409 || unsigned == 0xC000_0005
}

#[cfg(unix)]
fn signal_description(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.signal() {
        Some(signal) => format!("signal {signal}"),
        None => "no exit code".to_string(),
    }
}

#[cfg(not(unix))]
fn signal_description(_status: &std::process::ExitStatus) -> String {
    "no exit code".to_string()
}

pub(crate) fn cleanup(root: &Path) -> String {
    match fs::remove_dir_all(root) {
        Ok(()) if !root.exists() => "removed".to_string(),
        Ok(()) => "retained_after_remove".to_string(),
        Err(error) if !root.exists() && error.kind() == std::io::ErrorKind::NotFound => {
            "removed".to_string()
        }
        Err(error) => format!("retained ({error})"),
    }
}
