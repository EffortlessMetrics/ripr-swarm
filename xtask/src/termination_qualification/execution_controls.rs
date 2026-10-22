//! Execution-boundary controls (#4510). Each one runs a stub through the same
//! launch path as a corpus row and checks that the harness, not the
//! environment, decided what ran: a scrubbed `PATH`, an owned working
//! directory, a stale artifact that cannot be read as fresh output, and a
//! snapshot that a rewrite of the candidate path cannot change.

use std::path::Path;

use super::corpus::{Case, Expect, Route};
use super::model::{Control, ControlKind, RowResult};

fn control(id: &str, expected: &str) -> Control {
    Control {
        id: id.to_string(),
        kind: ControlKind::Execution,
        family: None,
        expected: expected.to_string(),
        observed: None,
        discriminated: false,
        detail: String::new(),
    }
}

#[cfg(not(unix))]
pub(crate) fn execution_controls(_work_root: &Path, _cases: &[Case]) -> Vec<Control> {
    let mut control = control("execution-boundary", "execution boundary holds");
    control.detail = "execution controls need a Unix shell; not run on this host".to_string();
    vec![control]
}

#[cfg(unix)]
pub(crate) fn execution_controls(work_root: &Path, cases: &[Case]) -> Vec<Control> {
    use std::fs;
    use std::time::Duration;

    use super::corpus::Route;
    use super::exec::{self, Binary, CHILD_PATH, ROOT_TOKEN};
    use super::model::RowResult;
    use crate::blind_journey::sha256_hex;

    let deadline = Duration::from_secs(30);
    let stubs = work_root.join("stubs");
    let Some(check) = cases.iter().find(|case| case.route == Route::Check) else {
        return Vec::new();
    };
    let write = |name: &str, script: &str| -> Result<Binary, String> {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(&stubs).map_err(|error| error.to_string())?;
        let path = stubs.join(name);
        fs::write(&path, script).map_err(|error| error.to_string())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        Ok(Binary { path })
    };
    // Compare the digest of what the stub printed with what it should print.
    let stdout_is = |binary: &Binary, name: &str, printed: &str| {
        let (row, _) = exec::run_case(binary, &work_root.join(name), check, deadline);
        (row.stdout_sha256 == sha256_hex(printed.as_bytes()), row)
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
            let (matched, _) = stdout_is(&binary, "path", &format!("{CHILD_PATH}\nnone\n"));
            path_control.observed = Some(
                if matched {
                    "scrubbed"
                } else {
                    "ambient_visible"
                }
                .to_string(),
            );
            path_control.discriminated = matched;
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
            let (matched, _) = stdout_is(&binary, "cwd", &format!("{ROOT_TOKEN}\n"));
            cwd_control.observed =
                Some(if matched { "owned_root" } else { "foreign_cwd" }.to_string());
            cwd_control.discriminated = matched;
        }
        Err(error) => cwd_control.detail = error,
    }
    out.push(cwd_control);

    let mut stale_control = control(
        "execution-stale-artifact",
        "an artifact left in the workspace before launch is not read as the run's output",
    );
    if let Some(pilot) = cases.iter().find(|case| case.artifact.is_some()) {
        let relative = pilot.artifact.clone().unwrap_or_default();
        let stale = format!(
            "{{\"run_status\":\"complete\",\"seams\":[{{\"seam_id\":\"stale\",\"file\":\"{}\",\"grip_class\":\"x\"}}]}}",
            pilot.changed
        );
        let mut seeded = pilot.clone();
        seeded.files.push((relative, stale.into_bytes()));
        match write(
            "stale-probe",
            "#!/bin/sh\necho '{\"status\":\"complete\"}'\n",
        ) {
            Ok(binary) => {
                let (row, _) = exec::run_case(&binary, &work_root.join("stale"), &seeded, deadline);
                stale_control.observed = Some(row.result.as_str().to_string());
                stale_control.discriminated = row.result == RowResult::SemanticFailure;
                stale_control.detail = row.detail;
            }
            Err(error) => stale_control.detail = error,
        }
    } else {
        stale_control.detail = "no artifact-reading route in the selected corpus".to_string();
    }
    out.push(stale_control);

    let mut snapshot_control = control(
        "execution-snapshot-isolation",
        "rewriting the candidate path after admission does not change what runs",
    );
    let candidate = work_root.join("moved-candidate");
    let moved = write("moved-v1", "#!/bin/sh\necho v1\n").and_then(|first| {
        fs::copy(&first.path, &candidate).map_err(|error| error.to_string())?;
        let admitted = fs::read(&candidate).map_err(|error| error.to_string())?;
        let snapshot =
            super::command::snapshot_binary(&candidate, work_root, &sha256_hex(&admitted))?;
        fs::write(&candidate, "#!/bin/sh\necho v2\n").map_err(|error| error.to_string())?;
        Ok(Binary { path: snapshot })
    });
    match moved {
        Ok(binary) => {
            let (matched, _) = stdout_is(&binary, "moved", "v1\n");
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
        Err(error) => snapshot_control.detail = error,
    }
    out.push(snapshot_control);

    let mut cleanup_control = control(
        "execution-cleanup-failure",
        "a row whose workspace cannot be removed does not pass",
    );
    if let Some(supported) = cases
        .iter()
        .find(|case| case.route == Route::Check && matches!(case.expect, Expect::Supported))
    {
        let json = format!(
            "{{\"findings\":[{{\"canonical_gap\":{{\"file\":\"{}\"}}}}],\"analysis_outcome\":{{\"analysis_complete\":true}}}}",
            supported.changed
        );
        match write(
            "clean-probe",
            &format!("#!/bin/sh\nprintf '%s\\n' '{json}'\n"),
        ) {
            Ok(binary) => {
                let (baseline, _) =
                    exec::run_case(&binary, &work_root.join("clean-ok"), supported, deadline);
                // The remover fails the way a permission error would; a root
                // user ignores directory mode bits, so it is injected.
                let denied = |root: &Path| {
                    exec::cleanup_with(root, &|_| {
                        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
                    })
                };
                let (failed, _) = exec::run_case_with(
                    &binary,
                    &work_root.join("clean-denied"),
                    supported,
                    deadline,
                    &denied,
                );
                cleanup_control.observed = Some(format!(
                    "baseline={}, denied={} ({})",
                    baseline.result.as_str(),
                    failed.result.as_str(),
                    failed.cleanup
                ));
                cleanup_control.discriminated = baseline.result.is_pass()
                    && failed.result == RowResult::InstrumentFailure
                    && failed.cleanup.starts_with("retained");
            }
            Err(error) => cleanup_control.detail = error,
        }
    } else {
        cleanup_control.detail = "no supported check row in the selected corpus".to_string();
    }
    out.push(cleanup_control);
    out
}
