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

    let stubs = work_root.join("stubs");
    let write = |name: &str, script: &str| -> Result<PathBuf, String> {
        fs::create_dir_all(&stubs)
            .map_err(|error| format!("create probe directory {}: {error}", stubs.display()))?;
        let path = stubs.join(name);
        fs::write(&path, script)
            .map_err(|error| format!("write probe {}: {error}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("set probe permissions {}: {error}", path.display()))?;
        Ok(path)
    };

    let mut out = Vec::new();

    let mut path_control = control(
        "execution-scrubbed-path",
        "the child sees PATH exactly as the harness set it and finds no ambient `ripr`",
    );
    probe_control(
        &mut path_control,
        work_root,
        write(
            "path-probe",
            "#!/bin/sh\necho \"$PATH\"\nif command -v ripr >/dev/null 2>&1; then echo ambient; else echo none; fi\n",
        ),
        "path",
        |stdout| stdout == format!("{CHILD_PATH}\nnone\n"),
        ("scrubbed", "ambient_visible"),
    );
    out.push(path_control);

    let mut cwd_control = control(
        "execution-owned-cwd",
        "the child runs in the owned case root, not the caller's directory",
    );
    probe_control(
        &mut cwd_control,
        work_root,
        write("cwd-probe", "#!/bin/sh\npwd -P\n"),
        "cwd",
        |stdout| stdout.trim_end() == ROOT_TOKEN,
        ("owned_root", "foreign_cwd"),
    );
    out.push(cwd_control);

    let mut decoy_control = control(
        "execution-decoy-repository",
        "a repository above the owned case root is not discovered by the child",
    );
    let decoy = work_root.join("decoy-above");
    let planted = ["objects", "refs"]
        .iter()
        .try_for_each(|dir| {
            let path = decoy.join(".git").join(dir);
            fs::create_dir_all(&path)
                .map_err(|error| format!("create decoy directory {}: {error}", path.display()))
        })
        .and_then(|()| {
            let path = decoy.join(".git").join("HEAD");
            fs::write(&path, "ref: refs/heads/main\n")
                .map_err(|error| format!("write decoy HEAD {}: {error}", path.display()))
        });
    probe_control(
        &mut decoy_control,
        work_root,
        planted.and_then(|()| {
            write(
                "decoy-probe",
                "#!/bin/sh\ncommand -v git >/dev/null 2>&1 || { echo no-git; exit 0; }\ngit rev-parse --show-toplevel 2>/dev/null || echo none\n",
            )
        }),
        "decoy-above/probe",
        |stdout| stdout.trim_end() == "none",
        ("decoy_not_found", "decoy_or_git_missing"),
    );
    out.push(decoy_control);

    let mut snapshot_control = control(
        "execution-snapshot-isolation",
        "rewriting the candidate path after admission does not change what runs",
    );
    let candidate = work_root.join("moved-candidate");
    let moved = write("moved-v1", "#!/bin/sh\necho v1\n").and_then(|first| {
        fs::copy(&first, &candidate)
            .map_err(|error| format!("copy candidate to {}: {error}", candidate.display()))?;
        let admitted = fs::read(&candidate)
            .map_err(|error| format!("read candidate {}: {error}", candidate.display()))?;
        let snapshot = snapshot_admitted_binary(&candidate, work_root, &sha256_hex(&admitted))?;
        fs::write(&candidate, "#!/bin/sh\necho v2\n")
            .map_err(|error| format!("rewrite candidate {}: {error}", candidate.display()))?;
        Ok(snapshot)
    });
    probe_control(
        &mut snapshot_control,
        work_root,
        moved,
        "moved",
        |stdout| stdout.trim_end() == "v1",
        ("ran_admitted_bytes", "ran_rewritten_bytes"),
    );
    out.push(snapshot_control);

    out
}

/// The shared consumer of each execution control's existing bounded runner.
/// Setup/capture errors and nonzero or abnormal child exits cannot discriminate.
#[cfg(unix)]
pub(super) fn probe_control(
    item: &mut CandidateProofControl,
    work_root: &Path,
    binary: Result<PathBuf, String>,
    case_id: &str,
    matches: impl FnOnce(&str) -> bool,
    observed: (&str, &str),
) {
    item.discriminated = false;
    item.observed = None;
    item.detail.clear();
    let deadline = Duration::from_secs(30);
    let run = || {
        let binary = binary.map_err(|error| format!("prepare probe: {error}"))?;
        let root = launch::OwnedFixtureRoot::new(work_root, case_id)
            .map_err(|error| format!("prepare owned root: {error}"))?;
        let observation = observe_process(
            &binary,
            &[],
            root.path(),
            deadline,
            &format!("harness control probe {case_id}"),
        )
        .map_err(|error| format!("launch/capture probe {}: {error}", binary.display()))?;
        Ok::<_, String>((root, observation))
    };
    match run() {
        Err(error) => item.detail = diagnostic_excerpt(&error, work_root),
        Ok((root, observation)) => {
            let (state, _) = classify_observation(&observation, deadline);
            let stdout = root.tokenize(&String::from_utf8_lossy(&observation.stdout));
            let clean_exit = state.is_pass() && observation.exit_code == Some(0);
            let matched = matches(&stdout);
            item.discriminated = clean_exit && matched;
            item.observed = Some(
                if !clean_exit {
                    "process_failed"
                } else if matched {
                    observed.0
                } else {
                    observed.1
                }
                .to_string(),
            );
            if !item.discriminated {
                let stderr = root.tokenize(&String::from_utf8_lossy(&observation.stderr));
                item.detail = format!(
                    "state={}; exit_code={:?}; timed_out={}; aborted={}; termination=\"{}\"; stdout=\"{}\"; stderr=\"{}\"",
                    state.as_str(),
                    observation.exit_code,
                    observation.timed_out,
                    observation.aborted,
                    diagnostic_excerpt(&observation.termination, work_root),
                    diagnostic_excerpt(&stdout, work_root),
                    diagnostic_excerpt(&stderr, work_root),
                );
            }
        }
    }
}

/// Bound the rendered bytes, including escape expansion. Never emit terminal
/// control characters. Callers capture only the fixed probes, never environment dumps.
#[cfg(unix)]
fn diagnostic_excerpt(text: &str, work_root: &Path) -> String {
    const LIMIT: usize = 2048;
    const TRUNCATED: &str = "...[truncated]";
    // Scrub before escaping/truncating: downstream consumers cannot match a
    // quoted, Unicode or partially truncated raw work-root afterward.
    let text = text.replace(work_root.to_string_lossy().as_ref(), "<work-root>");
    let mut out = String::new();
    for ch in text.chars() {
        let escaped = ch.escape_default().to_string();
        if out.len() + escaped.len() > LIMIT - TRUNCATED.len() {
            out.push_str(TRUNCATED);
            break;
        }
        out.push_str(&escaped);
    }
    out
}
