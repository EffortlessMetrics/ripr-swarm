//! Execution-boundary controls (#4510) via the shared exact-candidate harness.

use std::path::Path;
use std::time::Duration;

use super::corpus::{Case, Expect, Route};
use super::exec::{self, Binary};
use super::model::{Control, ControlKind, RowResult};
use crate::exact_candidate_harness::execution_boundary_controls;

fn map_control(control: crate::exact_candidate_harness::model::CandidateProofControl) -> Control {
    Control {
        id: control.id,
        kind: ControlKind::Execution,
        family: None,
        expected: control.expected,
        observed: control.observed,
        discriminated: control.discriminated,
        detail: control.detail,
    }
}

pub(crate) fn execution_controls(work_root: &Path, cases: &[Case]) -> Vec<Control> {
    let mut out: Vec<Control> = execution_boundary_controls(work_root)
        .into_iter()
        .map(map_control)
        .collect();

    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let deadline = Duration::from_secs(30);
        let stubs = work_root.join("stubs");
        let write = |name: &str, script: &str| -> Result<Binary, String> {
            fs::create_dir_all(&stubs).map_err(|error| error.to_string())?;
            let path = stubs.join(name);
            fs::write(&path, script).map_err(|error| error.to_string())?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .map_err(|error| error.to_string())?;
            Ok(Binary { path })
        };

        let mut stale_control = Control {
            id: "execution-stale-artifact".to_string(),
            kind: ControlKind::Execution,
            family: None,
            expected:
                "an artifact left in the workspace before launch is not read as the run's output"
                    .to_string(),
            observed: None,
            discriminated: false,
            detail: String::new(),
        };
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
                    let (row, _) =
                        exec::run_case(&binary, &work_root.join("stale"), &seeded, deadline);
                    stale_control.observed = Some(row.result.as_str().to_string());
                    stale_control.discriminated = row.result == RowResult::SemanticFailure;
                    stale_control.detail = row.detail;
                }
                Err(error) => stale_control.detail = error,
            }
        } else {
            stale_control.detail = "no artifact-reading route in the selected corpus".to_string();
        }
        if let Some(index) = out
            .iter()
            .position(|control| control.id == "execution-snapshot-isolation")
        {
            out.insert(index, stale_control);
        } else {
            out.push(stale_control);
        }

        let mut cleanup_control = Control {
            id: "execution-cleanup-failure".to_string(),
            kind: ControlKind::Execution,
            family: None,
            expected: "a row whose workspace cannot be removed does not pass".to_string(),
            observed: None,
            discriminated: false,
            detail: String::new(),
        };
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
        // Replace harness cleanup control with the corpus-aware version above.
        if let Some(index) = out
            .iter()
            .position(|control| control.id == "execution-cleanup-failure")
        {
            out[index] = cleanup_control;
        } else {
            out.push(cleanup_control);
        }
    }

    out
}
