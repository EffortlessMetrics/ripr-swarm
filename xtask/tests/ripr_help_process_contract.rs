//! Native controls for the bounded help-proof adapter, separate from guide counts.

#[path = "common/ripr_help.rs"]
pub mod ripr_help;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const FIXTURE_ROLE: &str = "RIPR_HELP_PROCESS_FIXTURE_ROLE";
const FIXTURE_ROOT: &str = "RIPR_HELP_PROCESS_FIXTURE_ROOT";
const BUDGET: Duration = Duration::from_secs(8);

fn fixture_command(root: &Path, role: &str) -> Result<Command, String> {
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .args([
            "--exact",
            "timeout_reaps_primary_and_terminates_descendant",
            "--nocapture",
        ])
        .env(FIXTURE_ROLE, role)
        .env(FIXTURE_ROOT, root)
        .current_dir(root);
    Ok(command)
}

fn fixture(role: &str) -> Result<(), String> {
    let root = PathBuf::from(std::env::var_os(FIXTURE_ROOT).ok_or("fixture root missing")?);
    let mut marker = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(format!("{role}.pid")))
        .map_err(|e| e.to_string())?;
    writeln!(marker, "{}", std::process::id()).map_err(|e| e.to_string())?;
    // Raw child spawn deliberately inherits the adapter's group/Job Object.
    // The fixture's safety exit bounds a broken-containment negative control.
    let mut descendant = if role == "primary" {
        Some(
            fixture_command(&root, "descendant")?
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?,
        )
    } else if role == "descendant" || role == "single" {
        None
    } else {
        return Err(format!("unknown fixture role: {role}"));
    };
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(25) {
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Some(child) = &mut descendant {
        child.kill().map_err(|e| e.to_string())?;
        child.wait().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Absent,
    #[cfg(unix)]
    Zombie,
    Live,
}

fn state(pid: u32) -> Result<State, String> {
    #[cfg(unix)]
    let mut command = Command::new("ps");
    #[cfg(unix)]
    command.args(["-o", "stat=", "-p", &pid.to_string()]);
    #[cfg(windows)]
    let mut command = Command::new("tasklist");
    #[cfg(windows)]
    command.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
    let output = ripr_help::capture_with_cleanup(command, Duration::from_secs(2), true, |_, _| {})?;
    let text = std::str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        if output.status.code() == Some(1) && text.trim().is_empty() && output.stderr.is_empty() {
            return Ok(State::Absent);
        }
        if !output.status.success() || text.trim().is_empty() {
            return Err(format!("ps failed for owned PID {pid}: {output:?}"));
        }
        if text.trim().starts_with('Z') {
            Ok(State::Zombie)
        } else {
            Ok(State::Live)
        }
    }
    #[cfg(windows)]
    {
        if !output.status.success() {
            return Err(format!("tasklist failed for owned PID {pid}: {output:?}"));
        }
        if text.lines().any(|line| {
            line.split(',')
                .nth(1)
                .is_some_and(|value| value.trim_matches('"') == pid.to_string())
        }) {
            Ok(State::Live)
        } else {
            Ok(State::Absent)
        }
    }
}

fn marker(root: &Path, role: &str) -> Option<u32> {
    std::fs::read_to_string(root.join(format!("{role}.pid")))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn observe(root: &Path, roles: &[&str]) -> Result<Vec<u32>, String> {
    let start = Instant::now();
    loop {
        let pids: Option<Vec<u32>> = roles.iter().map(|role| marker(root, role)).collect();
        if let Some(pids) = pids {
            if pids
                .iter()
                .any(|pid| *pid == 0 || *pid == std::process::id())
            {
                return Err(format!("invalid owned fixture PIDs: {pids:?}"));
            }
            for pid in &pids {
                if state(*pid)? != State::Live {
                    return Err(format!("fixture PID {pid} was not live before timeout"));
                }
            }
            return Ok(pids);
        }
        if start.elapsed() >= Duration::from_secs(5) {
            return Err("fixture readiness deadline exceeded".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Emergency<'a> {
    root: &'a Path,
    roles: &'a [&'a str],
}
impl Drop for Emergency<'_> {
    fn drop(&mut self) {
        for role in self.roles {
            if let Some(pid) = marker(self.root, role)
                && pid != 0
                && pid != std::process::id()
                && matches!(state(pid), Ok(State::Live))
            {
                #[cfg(unix)]
                let mut command = Command::new("kill");
                #[cfg(unix)]
                command.args(["-KILL", "--", &pid.to_string()]);
                #[cfg(windows)]
                let mut command = Command::new("taskkill");
                #[cfg(windows)]
                command.args(["/PID", &pid.to_string(), "/F"]);
                if let Err(error) = ripr_help::capture_with_cleanup(
                    command,
                    Duration::from_secs(2),
                    true,
                    |_, _| {},
                ) {
                    eprintln!("emergency cleanup of owned fixture {pid} failed: {error}");
                }
            }
        }
    }
}

fn timeout_control(
    descendant: bool,
    group_error: bool,
    reap_error: bool,
) -> Result<String, String> {
    let scratch = ripr_help::Scratch::new("help-process-control")?;
    let roles: &[&str] = if descendant {
        &["primary", "descendant"]
    } else {
        &["single"]
    };
    let _emergency = Emergency {
        root: &scratch.0,
        roles,
    };
    let command = fixture_command(&scratch.0, roles[0])?;
    std::thread::scope(|scope| {
        let observer = scope.spawn(|| observe(&scratch.0, roles));
        let start = Instant::now();
        let mut real_cleanup = None;
        let result = ripr_help::capture_with_cleanup(command, BUDGET, true, |group, reap| {
            real_cleanup = Some((group.clone(), reap.clone()));
            if group_error {
                *group = Err("injected group cleanup failure".into());
            }
            if reap_error {
                *reap = Err("injected reap failure".into());
            }
        });
        // Join on every outcome, including setup/error and unexpected success.
        let observed = observer.join().map_err(|panic_payload| {
            let detail = panic_payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| panic_payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            format!("PID observer panicked: {detail}")
        })?;
        let pids = observed?;
        if start.elapsed() >= Duration::from_secs(18) {
            return Err("timeout control exceeded its return bound".into());
        }
        if real_cleanup != Some((Ok(()), Ok(()))) {
            return Err(format!(
                "real cleanup attempts did not both succeed: {real_cleanup:?}"
            ));
        }
        for (index, pid) in pids.iter().enumerate() {
            let actual = state(*pid)?;
            if actual == State::Live || (index == 0 && actual != State::Absent) {
                return Err(format!(
                    "owned PID {pid} persisted after cleanup: {actual:?}"
                ));
            }
            eprintln!("owned fixture PID {pid}: live before timeout, {actual:?} after cleanup");
        }
        result
            .err()
            .ok_or_else(|| "fixture completed instead of reaching timeout".into())
    })
}

#[test]
fn timeout_reaps_primary_and_terminates_descendant() -> Result<(), String> {
    if let Ok(role) = std::env::var(FIXTURE_ROLE) {
        return fixture(&role);
    }
    let error = timeout_control(true, false, false)?;
    if !error.contains("help proof command exceeded 8s") || !error.contains("Ok(None)") {
        return Err(format!("primary timeout observation missing: {error}"));
    }
    Ok(())
}

#[test]
fn timeout_retains_primary_and_every_cleanup_error() -> Result<(), String> {
    let mut failures = Vec::new();
    for (group, reap) in [(true, false), (false, true), (true, true)] {
        let error = timeout_control(false, group, reap)?;
        if !error.contains("help proof command exceeded 8s")
            || !error.contains("Ok(None)")
            || (group && !error.contains("injected group cleanup failure"))
            || (reap && !error.contains("injected reap failure"))
        {
            failures.push(format!("group={group}, reap={reap}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
