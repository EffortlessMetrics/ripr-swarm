//! Required execution-boundary controls (#4510). Stubs run through the same
//! launch path as successor corpus rows.

#[cfg(unix)]
use std::fs;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use super::admission::snapshot_admitted_binary;
#[cfg(unix)]
use super::launch::{self, CHILD_PATH, ROOT_TOKEN};
use super::model::CandidateProofControl;
#[cfg(unix)]
use super::model::HarnessExecutionState;
#[cfg(unix)]
use super::process::{classify_observation, observe_process};
#[cfg(unix)]
use crate::blind_journey::sha256_hex;

fn control(id: &str, expected: &str) -> CandidateProofControl {
    CandidateProofControl {
        id: id.to_string(),
        expected: expected.to_string(),
        observed: None,
        discriminated: false,
        detail: String::new(),
    }
}

#[cfg(not(unix))]
pub(crate) fn execution_boundary_controls(_work_root: &Path) -> Vec<CandidateProofControl> {
    let mut item = control("execution-boundary", "execution boundary holds");
    item.detail = "execution controls need a Unix shell; not run on this host".to_string();
    vec![item]
}

#[cfg(unix)]
pub(crate) fn execution_boundary_controls(work_root: &Path) -> Vec<CandidateProofControl> {
    use std::os::unix::fs::PermissionsExt;

    let deadline = Duration::from_secs(30);
    let stubs = work_root.join("stubs");
    let write = |name: &str, script: &str| -> Result<PathBuf, String> {
        fs::create_dir_all(&stubs).map_err(|error| error.to_string())?;
        let path = stubs.join(name);
        fs::write(&path, script).map_err(|error| error.to_string())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        Ok(path)
    };
    let run_probe =
        |binary: &Path, case_id: &str| -> Result<(HarnessExecutionState, String), String> {
            let root = launch::OwnedFixtureRoot::new(work_root, case_id)?;
            let observation = observe_process(
                binary,
                &[],
                root.path(),
                deadline,
                &format!("harness control probe {case_id}"),
            )?;
            let (state, _detail) = classify_observation(&observation, deadline);
            let stdout = root.tokenize(&String::from_utf8_lossy(&observation.stdout));
            Ok((state, stdout))
        };

    let mut out = Vec::new();

    let mut path_control = control(
        "execution-scrubbed-path",
        "the child sees PATH exactly as the harness set it and finds no ambient `ripr`",
    );
    match write(
        "path-probe",
        "#!/bin/sh\necho \"$PATH\"\nif command -v ripr >/dev/null 2>&1; then echo ambient; else echo none; fi\n",
    ) {
        Ok(binary) => {
            if let Ok((_, stdout)) = run_probe(&binary, "path") {
                let matched = stdout == format!("{CHILD_PATH}\nnone\n");
                path_control.observed = Some(
                    if matched {
                        "scrubbed"
                    } else {
                        "ambient_visible"
                    }
                    .to_string(),
                );
                path_control.discriminated = matched;
                if !matched {
                    path_control.detail = stdout;
                }
            }
        }
        Err(error) => path_control.detail = error,
    }
    out.push(path_control);

    let mut cwd_control = control(
        "execution-owned-cwd",
        "the child runs in the owned case root, not the caller's directory",
    );
    match write("cwd-probe", "#!/bin/sh\npwd -P\n") {
        Ok(binary) => {
            if let Ok((_, stdout)) = run_probe(&binary, "cwd") {
                let matched = stdout.trim_end() == ROOT_TOKEN;
                cwd_control.observed =
                    Some(if matched { "owned_root" } else { "foreign_cwd" }.to_string());
                cwd_control.discriminated = matched;
                if !matched {
                    cwd_control.detail = stdout;
                }
            }
        }
        Err(error) => cwd_control.detail = error,
    }
    out.push(cwd_control);

    let mut decoy_control = control(
        "execution-decoy-repository",
        "a repository above the owned case root is not discovered by the child",
    );
    let decoy = work_root.join("decoy-above");
    let planted = ["objects", "refs"]
        .iter()
        .try_for_each(|dir| fs::create_dir_all(decoy.join(".git").join(dir)))
        .and_then(|()| fs::write(decoy.join(".git").join("HEAD"), "ref: refs/heads/main\n"))
        .map_err(|error| error.to_string());
    match planted.and_then(|()| {
        write(
            "decoy-probe",
            "#!/bin/sh\ncommand -v git >/dev/null 2>&1 || { echo no-git; exit 0; }\ngit rev-parse --show-toplevel 2>/dev/null || echo none\n",
        )
    }) {
        Ok(binary) => {
            if let Ok((_, stdout)) = run_probe(&binary, "decoy-above/probe") {
                let matched = stdout.trim_end() == "none";
                decoy_control.observed = Some(
                    if matched {
                        "decoy_not_found"
                    } else {
                        "decoy_or_git_missing"
                    }
                    .to_string(),
                );
                decoy_control.discriminated = matched;
                if !matched {
                    decoy_control.detail = stdout;
                }
            }
        }
        Err(error) => decoy_control.detail = error,
    }
    out.push(decoy_control);

    let mut snapshot_control = control(
        "execution-snapshot-isolation",
        "rewriting the candidate path after admission does not change what runs",
    );
    let candidate = work_root.join("moved-candidate");
    let moved = write("moved-v1", "#!/bin/sh\necho v1\n").and_then(|first| {
        fs::copy(&first, &candidate).map_err(|error| error.to_string())?;
        let admitted = fs::read(&candidate).map_err(|error| error.to_string())?;
        let snapshot = snapshot_admitted_binary(&candidate, work_root, &sha256_hex(&admitted))?;
        fs::write(&candidate, "#!/bin/sh\necho v2\n").map_err(|error| error.to_string())?;
        Ok(snapshot)
    });
    match moved {
        Ok(binary) => {
            if let Ok((_, stdout)) = run_probe(&binary, "moved") {
                let matched = stdout.trim_end() == "v1";
                snapshot_control.observed = Some(
                    if matched {
                        "ran_admitted_bytes"
                    } else {
                        "ran_rewritten_bytes"
                    }
                    .to_string(),
                );
                snapshot_control.discriminated = matched;
            }
        }
        Err(error) => snapshot_control.detail = error,
    }
    out.push(snapshot_control);

    out
}
