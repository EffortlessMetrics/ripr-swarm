//! Negative controls. A packet that cannot fail proves nothing, so each control
//! must reach a failing state or the aggregate is not green.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::corpus::{Case, Expect, Route};
use super::exec::{self, Binary};
use super::model::{Control, ControlKind, Family, Role, Row, RowResult};

/// Row states that show the removed repair mattered: the unrepaired path
/// aborts, hangs, or returns a clean result with no limitation.
fn is_removal_signal(result: RowResult) -> bool {
    matches!(
        result,
        RowResult::ProcessAbort
            | RowResult::NonterminalDeadline
            | RowResult::MissingOrFalseCleanLimitation
    )
}

fn ran(id: &str, kind: ControlKind, family: Option<Family>, expected: &str) -> Control {
    Control {
        id: id.to_string(),
        kind,
        family,
        expected: expected.to_string(),
        observed: None,
        discriminated: false,
        detail: String::new(),
    }
}

/// Run the whole corpus a second time against the same binary and require the
/// semantic row content to agree.
pub(crate) fn repeat_control(
    binary: &Binary,
    work_root: &Path,
    cases: &[Case],
    first: &[Row],
    enabled: bool,
    deadline: Duration,
) -> Vec<Control> {
    let mut control = ran(
        "repeat-identical-semantic-outcomes",
        ControlKind::Repeat,
        None,
        "second pass equals first pass row for row",
    );
    if !enabled {
        control.detail = "skipped by --no-repeat".to_string();
        return vec![control];
    }
    let mut differing = Vec::new();
    for (case, before) in cases.iter().zip(first) {
        let (after, _) = exec::run_case(binary, work_root, case, deadline);
        if semantic_text(before) != semantic_text(&after) {
            differing.push(case.id.clone());
        }
    }
    let _ = fs::remove_dir_all(work_root);
    control.discriminated = differing.is_empty();
    control.observed = Some(if differing.is_empty() {
        "identical".to_string()
    } else {
        format!("differs: {}", differing.join(", "))
    });
    control.detail = if differing.is_empty() {
        "every row's semantic content repeated".to_string()
    } else {
        "rows changed between passes of the unchanged binary".to_string()
    };
    vec![control]
}

fn semantic_text(row: &Row) -> String {
    serde_json::to_string(row).unwrap_or_default()
}

#[cfg(unix)]
pub(crate) fn instrument_controls(work_root: &Path, cases: &[Case]) -> Vec<Control> {
    // A typed-limitation check row is the strictest classifier target: it
    // needs a limitation, an incomplete outcome and a changed-file finding.
    let Some(case) = cases.iter().find(|case| {
        case.route == Route::Check && matches!(case.expect, Expect::TypedLimitation { .. })
    }) else {
        return vec![ran(
            "instrument-classifier",
            ControlKind::Instrument,
            None,
            "stub binaries classify as failures",
        )];
    };
    let stubs = work_root.join("stubs");
    if let Err(error) = fs::create_dir_all(&stubs) {
        let mut control = ran(
            "instrument-classifier",
            ControlKind::Instrument,
            None,
            "stub binaries classify as failures",
        );
        control.detail = format!("could not create {}: {error}", stubs.display());
        return vec![control];
    }
    let hang_pid = stubs.join("hang.pid");
    let hang_script = format!(
        "#!/bin/sh\nsleep 600 &\necho $! > '{}'\nwait\n",
        hang_pid.display()
    );
    let wrong_subject = format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", wrong_subject_json(case));
    let cases_to_run: [(&str, &str, RowResult); 6] = [
        (
            "instrument-abort",
            "#!/bin/sh\nkill -ABRT $$\n",
            RowResult::ProcessAbort,
        ),
        (
            "instrument-hang",
            &hang_script,
            RowResult::NonterminalDeadline,
        ),
        (
            "instrument-false-clean",
            "#!/bin/sh\necho '{\"summary\":{\"findings\":0},\"findings\":[]}'\n",
            RowResult::MissingOrFalseCleanLimitation,
        ),
        (
            "instrument-wrong-subject",
            &wrong_subject,
            RowResult::WrongSubject,
        ),
        (
            "instrument-not-json",
            "#!/bin/sh\necho 'ripr: ok'\n",
            RowResult::SemanticFailure,
        ),
        (
            "instrument-launch-failure",
            "#!/bin/sh\nexit 0\n",
            RowResult::InstrumentFailure,
        ),
    ];
    let mut out = Vec::new();
    for (id, script, expected) in cases_to_run {
        let mut control = ran(
            id,
            ControlKind::Instrument,
            Some(case.family),
            expected.as_str(),
        );
        let path = stubs.join(id);
        if id == "instrument-launch-failure" {
            // No file at all: the launch itself must be an instrument failure.
        } else if let Err(error) = write_executable(&path, script) {
            control.detail = error;
            out.push(control);
            continue;
        }
        let deadline = Duration::from_secs(2);
        let (row, _) = exec::run_case(
            &Binary { path: path.clone() },
            &work_root.join(id),
            case,
            deadline,
        );
        control.observed = Some(row.result.as_str().to_string());
        control.discriminated = row.result == expected && !row.result.is_pass();
        control.detail = row.detail.clone();
        if id == "instrument-hang" {
            let reaped = child_gone(&hang_pid);
            control.discriminated = control.discriminated && reaped;
            control.detail = format!(
                "deadline kill left the stub's child {}",
                if reaped {
                    "terminated"
                } else {
                    "running (orphan)"
                }
            );
        }
        out.push(control);
    }
    let _ = fs::remove_dir_all(&stubs);
    out
}

/// A limitation for the right file with no finding for the changed file.
#[cfg(unix)]
fn wrong_subject_json(case: &Case) -> String {
    let token = match &case.expect {
        Expect::TypedLimitation { token, .. } => *token,
        _ => "parse_budget",
    };
    let class = match &case.expect {
        Expect::TypedLimitation {
            language_class: Some(class),
            ..
        } => *class,
        _ => "language_scope_unsupported",
    };
    let path = case.hostile.as_deref().unwrap_or("src/deep.py");
    format!(
        "{{\"findings\":[],\"analysis_outcome\":{{\"analysis_complete\":false,\"outcome\":{{\"limitations\":[{{\"kind\":\"{class}\",\"path\":\"{path}\",\"bounded_detail\":\"{token}: stub\"}}]}}}}}}"
    )
}

#[cfg(not(unix))]
pub(crate) fn instrument_controls(_work_root: &Path, _cases: &[Case]) -> Vec<Control> {
    let mut control = ran(
        "instrument-classifier",
        ControlKind::Instrument,
        None,
        "stub binaries classify as failures",
    );
    control.detail = "instrument controls need a Unix shell; not run on this host".to_string();
    vec![control]
}

#[cfg(unix)]
fn write_executable(path: &Path, script: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, script)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("failed to stat {}: {error}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("failed to chmod {}: {error}", path.display()))
}

/// True when the pid recorded by the stub no longer names a live process.
/// Linux `/proc` is the only host that can answer; elsewhere this reports
/// gone only when no pid was recorded, which fails the control.
#[cfg(unix)]
fn child_gone(pid_file: &Path) -> bool {
    let Ok(text) = fs::read_to_string(pid_file) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    let stat = PathBuf::from(format!("/proc/{pid}/stat"));
    match fs::read_to_string(&stat) {
        Ok(text) => text
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next().map(str::to_string))
            .is_some_and(|state| state == "Z" || state == "X"),
        Err(_) => !PathBuf::from(format!("/proc/{pid}")).exists(),
    }
}

/// Run each family's discriminator rows against a supplied pre-repair binary.
/// At least one row must abort, hang, or return a false-clean result.
pub(crate) fn removal_controls(
    work_root: &Path,
    cases: &[Case],
    supplied: &[(Family, PathBuf)],
    candidate_sha256: &str,
    deadline: Duration,
) -> Vec<Control> {
    let mut out = Vec::new();
    for family in Family::ALL {
        if !cases.iter().any(|case| case.family == family) {
            continue;
        }
        let id = format!("removal-{}", family.as_str());
        let mut control = ran(
            &id,
            ControlKind::Removal,
            Some(family),
            "pre-repair binary aborts, hangs, or returns a false clean result on a discriminator row",
        );
        let Some((_, path)) = supplied
            .iter()
            .find(|(supplied_family, _)| *supplied_family == family)
        else {
            control.detail = format!(
                "no --removal-control {}=<pre-repair binary> supplied",
                family.as_str()
            );
            out.push(control);
            continue;
        };
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                control.detail = format!("could not read {}: {error}", path.display());
                out.push(control);
                continue;
            }
        };
        let sha = crate::blind_journey::sha256_hex(&bytes);
        if sha == candidate_sha256 {
            control.observed = Some("same_binary_as_candidate".to_string());
            control.detail = "the removal control is byte-identical to the candidate".to_string();
            out.push(control);
            continue;
        }
        let binary = Binary { path: path.clone() };
        let mut observed = Vec::new();
        let mut signal = false;
        for case in cases
            .iter()
            .filter(|case| case.family == family && case.role == Role::Discriminator)
        {
            let (row, _) = exec::run_case(&binary, &work_root.join(&id), case, deadline);
            signal |= is_removal_signal(row.result);
            observed.push(format!("{}={}", case.id, row.result.as_str()));
        }
        control.discriminated = signal;
        control.observed = Some(observed.join(", "));
        control.detail = format!("control sha256 {sha}");
        out.push(control);
    }
    out
}
