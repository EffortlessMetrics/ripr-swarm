//! A private Linux session for a cooperative reexecuted test. Nested timed
//! commands may create process groups, but remain in this session. Keep the
//! primary waitable until cleanup so its SID cannot be recycled beneath us.

use rustix::process::{
    Pid, PidfdFlags, Signal, WaitId, WaitIdOptions, getsid, pidfd_open, pidfd_send_signal, setsid,
    waitid,
};
use std::fs;
use std::time::{Duration, Instant};

// The parent binds the one shared disappearance decision for its target.
// Custody is reused by the independently bounded integration driver without
// compiling a second instance of this module or the procfs helper.
use super::proc_stat;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CleanupFault {
    BeforePrimaryKill,
    AfterPrimaryStop,
}

thread_local! {
    // Module is test-only; injection is local to the exercising parent thread.
    pub(super) static CLEANUP_FAULT: std::cell::Cell<Option<CleanupFault>> = const { std::cell::Cell::new(None) };
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

pub(crate) struct TestSession {
    pub(super) primary: u32,
    active: bool,
}

impl TestSession {
    pub(crate) fn new(primary: u32) -> Self {
        Self {
            primary,
            active: true,
        }
    }

    /// Stop every live member before the caller reaps the session primary.
    /// On failure Drop remains armed and retries while the SID is still owned.
    pub(crate) fn finish(&mut self) -> Result<(), String> {
        self.cleanup()?;
        self.active = false;
        Ok(())
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

pub(crate) fn pid(value: u32) -> Result<Pid, String> {
    Pid::from_raw(value as i32).ok_or_else(|| "invalid isolated process identity".to_string())
}

pub(crate) fn send_kill(handle: &rustix::fd::OwnedFd) -> Result<(), String> {
    match pidfd_send_signal(handle, Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(format!("could not terminate owned session member: {error}")),
    }
}

pub(crate) struct Member {
    pub(crate) pid: u32,
    pub(crate) session: u32,
    pub(crate) start: String,
    pub(crate) state: String,
}

pub(crate) fn read_member(pid: u32) -> Result<Option<Member>, String> {
    member_from_stat_read(pid, fs::read_to_string(format!("/proc/{pid}/stat")))
}

pub(crate) fn member_from_stat_read(
    pid: u32,
    stat: std::io::Result<String>,
) -> Result<Option<Member>, String> {
    let text = match stat {
        Ok(text) => text,
        Err(error) if proc_stat::proc_stat_error_is_gone(&error) => return Ok(None),
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
