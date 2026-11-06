//! A private Linux session for a cooperative reexecuted test. Nested timed
//! commands may create process groups, but remain in this session. Keep the
//! primary waitable until cleanup so its SID cannot be recycled beneath us.

use super::{CaptureDeadline, OwnedProcess, WaitOutcome, poll_interval, proc_peak_rss_bytes};
use rustix::process::{
    Pid, PidfdFlags, Signal, WaitId, WaitIdOptions, getsid, pidfd_open, pidfd_send_signal, setsid,
    waitid,
};
use std::fs;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum CleanupFault {
    BeforePrimaryKill,
    AfterPrimaryStop,
}

thread_local! {
    // Module is test-only; injection is local to the exercising parent thread.
    static CLEANUP_FAULT: std::cell::Cell<Option<CleanupFault>> = const { std::cell::Cell::new(None) };
}

fn injected_failure(point: CleanupFault) -> Result<(), String> {
    CLEANUP_FAULT.with(|fault| {
        if fault.get() == Some(point) {
            fault.set(None);
            Err("injected isolated-session cleanup failure".to_string())
        } else {
            Ok(())
        }
    })
}

pub(crate) fn enter_test_session() -> Result<(), String> {
    setsid()
        .map(|_| ())
        .map_err(|error| format!("could not create isolated test session: {error}"))
}

pub(super) struct TestSession {
    primary: u32,
    active: bool,
}

impl TestSession {
    pub(super) fn new(primary: u32) -> Self {
        Self {
            primary,
            active: true,
        }
    }

    pub(super) fn wait(
        &mut self,
        child: &mut OwnedProcess,
        started: Instant,
        deadline: &mut CaptureDeadline<'_>,
        sample_rss: bool,
        context: &str,
    ) -> Result<WaitOutcome, String> {
        let primary = pid(self.primary)?;
        let mut peak_rss_bytes = None;
        loop {
            if sample_rss && let Some(bytes) = proc_peak_rss_bytes(self.primary) {
                peak_rss_bytes = Some(peak_rss_bytes.map_or(bytes, |seen: u64| seen.max(bytes)));
            }
            // NOWAIT is essential: reaping before the last session scan could
            // release the SID and permit an unrelated new session to reuse it.
            let exited = waitid(
                WaitId::Pid(primary),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .map_err(|error| format!("{context}: could not observe isolated primary: {error}"))?
            .is_some();
            let timed_out = !exited && deadline.expired(started);
            if exited || timed_out {
                // A cleanup error may precede stopping the primary. Never
                // block in wait on that path, and keep Drop's retry armed.
                self.cleanup()
                    .map_err(|error| format!("{context}: {error}"))?;
                // Successful cleanup observed the primary waitable and left
                // no live session members. Disarm before reaping releases SID.
                self.active = false;
                return Ok(WaitOutcome {
                    status: child.wait().map_err(|error| {
                        format!("{context}: failed to reap isolated primary: {error}")
                    })?,
                    duration: started.elapsed(),
                    timed_out,
                    peak_rss_bytes,
                });
            }
            std::thread::sleep(poll_interval(sample_rss));
        }
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let primary = pid(self.primary)?;
        injected_failure(CleanupFault::BeforePrimaryKill)?;
        let handle = pidfd_open(primary, PidfdFlags::empty())
            .map_err(|error| format!("could not open isolated primary handle: {error}"))?;
        send_kill(&handle)?;
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if waitid(
                WaitId::Pid(primary),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .map_err(|error| format!("could not observe stopped isolated primary: {error}"))?
            .is_some()
            {
                break;
            }
            if Instant::now() >= end {
                return Err("isolated primary did not stop during cleanup".to_string());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // A timeout during test startup may precede setsid(). In that case
        // kill only our primary; its inherited session belongs to the caller.
        if getsid(Some(primary))
            .map_err(|error| format!("could not verify isolated session identity: {error}"))?
            != primary
        {
            return Ok(());
        }
        injected_failure(CleanupFault::AfterPrimaryStop)?;
        loop {
            let members = live_members(self.primary)?;
            if members.is_empty() {
                return Ok(());
            }
            for member in members {
                // Open the PID handle before revalidating membership. Signals
                // then address that process, never a recycled numeric PID.
                let member_pid = pid(member.pid)?;
                let handle = match pidfd_open(member_pid, PidfdFlags::empty()) {
                    Ok(handle) => handle,
                    Err(rustix::io::Errno::SRCH) => continue,
                    Err(error) => return Err(format!("could not own session member: {error}")),
                };
                match read_member(member.pid)? {
                    Some(current)
                        if current.session == self.primary && current.start == member.start =>
                    {
                        send_kill(&handle)?;
                    }
                    _ => {}
                }
            }
            if Instant::now() >= end {
                return Err(
                    "could not confirm isolated session is empty during cleanup".to_string()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for TestSession {
    fn drop(&mut self) {
        if self.active {
            // Declared after OwnedProcess, so this executes before its Drop
            // can reap/release the primary. The explicit path reports errors.
            let _ = self.cleanup();
        }
    }
}

fn pid(value: u32) -> Result<Pid, String> {
    Pid::from_raw(value as i32).ok_or_else(|| "invalid isolated process identity".to_string())
}

fn send_kill(handle: &rustix::fd::OwnedFd) -> Result<(), String> {
    match pidfd_send_signal(handle, Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(format!("could not terminate owned session member: {error}")),
    }
}

struct Member {
    pid: u32,
    session: u32,
    start: String,
    state: String,
}

fn read_member(pid: u32) -> Result<Option<Member>, String> {
    let text = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not inspect session member {pid}: {error}")),
    };
    let (_, fields) = text.rsplit_once(')').ok_or("malformed process stat")?;
    let fields: Vec<_> = fields.split_whitespace().collect();
    let session = fields
        .get(3)
        .ok_or("missing session identity")?
        .parse::<u32>()
        .map_err(|error| format!("invalid session identity: {error}"))?;
    Ok(Some(Member {
        pid,
        session,
        state: fields.first().ok_or("missing process state")?.to_string(),
        start: fields
            .get(19)
            .ok_or("missing process start identity")?
            .to_string(),
    }))
}

fn live_members(session: u32) -> Result<Vec<Member>, String> {
    let mut members = Vec::new();
    for entry in fs::read_dir("/proc").map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if let Some(member) = read_member(pid)?
            && member.session == session
            && member.state != "Z"
            && member.state != "X"
        {
            members.push(member);
        }
    }
    Ok(members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const ROLE: &str = "RIPR_XTASK_SESSION_TEST_ROLE";
    const MARKER: &str = "RIPR_XTASK_SESSION_TEST_MARKER";
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn marker() -> Result<PathBuf, String> {
        for _ in 0..32 {
            let root = std::env::temp_dir().join(format!(
                "ripr-session-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => return Ok(root.join("child")),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("could not claim session regression fixture".to_string())
    }

    fn wait_marker(path: &Path) -> Result<u32, String> {
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(text) = fs::read_to_string(path)
                && let Ok(value) = text.parse()
            {
                return Ok(value);
            }
            if Instant::now() >= end {
                return Err("nested child did not acknowledge startup".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn child(role: &str) -> Result<(), String> {
        if role == "startup" {
            std::thread::sleep(Duration::from_secs(30));
            return Ok(());
        }
        enter_test_session()?;
        let path = PathBuf::from(std::env::var_os(MARKER).ok_or("missing marker path")?);
        let script = if role == "linger" {
            "set -m; sleep 30 >/dev/null 2>&1 & printf '%s' $! > \"$1\""
        } else if role == "normal" {
            "printf '%s' $$ > \"$1\"; sleep 0.1"
        } else {
            "trap '' TERM; printf '%s' $$ > \"$1\"; exec sleep 30"
        };
        let program = if role == "linger" { "bash" } else { "sh" };
        let args = vec![
            "-c".to_string(),
            script.to_string(),
            "session-test".to_string(),
            path.display().to_string(),
        ];
        let worker = std::thread::spawn(move || {
            crate::run::capture_output_with_timeout(
                program,
                &args,
                &[],
                Duration::from_secs(20),
                "nested session regression",
            )
        });
        let nested = wait_marker(&path)?;
        let member =
            read_member(nested)?.ok_or("nested child disappeared before acknowledgement")?;
        let primary = std::process::id();
        if member.session != primary
            || rustix::process::getpgid(Some(pid(nested)?))
                .map_err(|e| e.to_string())?
                .as_raw_nonzero()
                .get()
                .cast_unsigned()
                != nested
        {
            return Err("nested child did not enter an independent group in our session".into());
        }
        if role == "timeout" {
            let handle =
                pidfd_open(pid(nested)?, PidfdFlags::empty()).map_err(|e| e.to_string())?;
            pidfd_send_signal(&handle, Signal::TERM).map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(20));
            if read_member(nested)?.is_none_or(|m| m.start != member.start || m.state == "Z") {
                return Err("TERM-resistance control was not established".into());
            }
        }
        fs::write(
            path.with_extension("ack"),
            format!("{} {} {}", nested, primary, member.start),
        )
        .map_err(|e| e.to_string())?;
        let output = worker.join().map_err(|panic_payload| {
            let message = panic_payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| panic_payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            format!("nested capture panicked: {message}")
        })??;
        if !output.status.is_some_and(|status| status.success()) || output.timed_out {
            return Err(format!(
                "nested capture failed: status={:?} timeout={} stderr={}",
                output.status, output.timed_out, output.stderr
            ));
        }
        Ok(())
    }

    fn exercise(test: &str, role: &str, fault: Option<CleanupFault>) -> Result<(), String> {
        if let Some(role) = std::env::var_os(ROLE) {
            return child(role.to_str().ok_or("invalid session test role")?);
        }
        let test = test.split_once("::").ok_or("missing crate prefix")?.1;
        let path = marker()?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let args = vec![
            test.to_string(),
            "--exact".to_string(),
            "--nocapture".to_string(),
        ];
        CLEANUP_FAULT.with(|value| value.set(fault));
        let started = Instant::now();
        let output = crate::run::capture_isolated_test(
            &exe.to_string_lossy(),
            &args,
            Some(Path::new(env!("CARGO_MANIFEST_DIR"))),
            &[(ROLE, role), (MARKER, &path.to_string_lossy())],
            Duration::from_secs(3),
            "session regression",
        );
        CLEANUP_FAULT.with(|value| value.set(None));
        if fault.is_some() {
            let error = output
                .err()
                .ok_or("cleanup failure was silently accepted")?;
            if !error.contains("injected isolated-session cleanup failure") {
                return Err(error);
            }
            if started.elapsed() > Duration::from_secs(8) {
                return Err("cleanup failure blocked return".into());
            }
        } else {
            let output = output?.output;
            if role == "timeout" || role == "startup" {
                if !output.timed_out {
                    return Err(format!(
                        "expected timeout: status={:?} stderr={}",
                        output.status, output.stderr
                    ));
                }
            } else if output.timed_out || !output.status.is_some_and(|s| s.success()) {
                return Err(format!(
                    "normal completion was not preserved: status={:?} timeout={} stderr={}",
                    output.status, output.timed_out, output.stderr
                ));
            }
        }
        if role != "startup" {
            let ack = fs::read_to_string(path.with_extension("ack"))
                .map_err(|e| format!("missing acknowledgement: {e}"))?;
            let fields: Vec<_> = ack.split_whitespace().collect();
            let nested = fields
                .first()
                .ok_or("missing acknowledged PID")?
                .parse::<u32>()
                .map_err(|e| e.to_string())?;
            if let Some(member) = read_member(nested)?
                && member.state != "Z"
                && member.state != "X"
                && Some(member.start.as_str()) == fields.get(2).copied()
            {
                return Err("owned nested process remained active after capture return".into());
            }
        }
        fs::remove_dir_all(path.parent().ok_or("missing fixture root")?)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    macro_rules! case {
        ($name:ident, $role:expr, $fault:expr) => {
            #[test]
            fn $name() -> Result<(), String> {
                exercise(
                    concat!(module_path!(), "::", stringify!($name)),
                    $role,
                    $fault,
                )
            }
        };
    }
    case!(
        isolated_session_timeout_cleans_independent_group,
        "timeout",
        None
    );
    case!(isolated_session_preserves_normal_completion, "normal", None);
    case!(
        isolated_session_normal_exit_cleans_lingering_group,
        "linger",
        None
    );
    case!(
        isolated_session_startup_timeout_preserves_inherited_session,
        "startup",
        None
    );
    case!(
        isolated_session_primary_cleanup_failure_returns_bounded,
        "timeout",
        Some(CleanupFault::BeforePrimaryKill)
    );
    case!(
        isolated_session_member_cleanup_failure_returns_bounded,
        "timeout",
        Some(CleanupFault::AfterPrimaryStop)
    );

    #[test]
    fn isolated_session_preserves_unrelated_group() -> Result<(), String> {
        if let Some(role) = std::env::var_os(ROLE) {
            return child(role.to_str().ok_or("invalid session test role")?);
        }
        let outside = marker()?;
        let args = vec![
            "-c".to_string(),
            "printf '%s' $$ > \"$1\"; sleep 5".to_string(),
            "unrelated-control".to_string(),
            outside.display().to_string(),
        ];
        let worker = std::thread::spawn(move || {
            crate::run::capture_output_measured(
                "sh",
                &args,
                Some(Path::new(env!("CARGO_MANIFEST_DIR"))),
                &[],
                Duration::from_secs(10),
                "unrelated group control",
            )
            .map(|measured| measured.output)
        });
        let observed: Result<(), String> = (|| {
            let outside_pid = wait_marker(&outside)?;
            let before = read_member(outside_pid)?.ok_or("unrelated child absent")?;
            exercise(
                concat!(
                    module_path!(),
                    "::isolated_session_preserves_unrelated_group"
                ),
                "timeout",
                None,
            )?;
            let after = read_member(outside_pid)?.ok_or("unrelated child was terminated")?;
            if after.start != before.start || after.state == "Z" {
                return Err("session cleanup affected an unrelated group".into());
            }
            Ok(())
        })();
        let completion = worker.join().map_err(|panic_payload| {
            let message = panic_payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| panic_payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            format!("unrelated control panicked: {message}")
        })??;
        observed?;
        if completion.timed_out || !completion.status.is_some_and(|status| status.success()) {
            return Err("unrelated command did not complete normally".into());
        }
        fs::remove_dir_all(outside.parent().ok_or("missing outsider fixture")?)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
