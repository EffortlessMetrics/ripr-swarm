//! Shared fallback cleanup for the real capture controls, outside the standalone
//! probe driver's source module so that driver gains no unused test-only API.

use super::test_session::custody::{member_from_stat_read, pid, read_member, send_kill};
use rustix::process::{PidfdFlags, pidfd_open};
use std::time::{Duration, Instant};

/// A fixture may retain a descendant's stat before its primary exits. Cleanup
/// opens a process handle, revalidates that recorded start/session identity,
/// then signals only the handle. It never revives a reaped numeric group.
pub(crate) fn finish_recorded_member(stat: &str) -> Result<(), String> {
    let value = stat
        .split_whitespace()
        .next()
        .ok_or("recorded fixture stat has no pid")?
        .parse::<u32>()
        .map_err(|error| format!("invalid recorded fixture pid: {error}"))?;
    let recorded = member_from_stat_read(value, Ok(stat.to_string()))?
        .ok_or("recorded fixture stat disappeared")?;
    let handle = match pidfd_open(pid(value)?, PidfdFlags::empty()) {
        Ok(handle) => handle,
        Err(rustix::io::Errno::SRCH) => return Ok(()),
        Err(error) => return Err(format!("could not open recorded fixture handle: {error}")),
    };
    if let Some(current) = read_member(value)? {
        if current.start != recorded.start || current.session != recorded.session {
            return Err("recorded fixture identity changed; no signal was sent".into());
        }
        send_kill(&handle)?;
    }
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        match read_member(value)? {
            None => return Ok(()),
            Some(current)
                if current.start != recorded.start
                    || current.session != recorded.session
                    || current.state == "Z"
                    || current.state == "X" =>
            {
                return Ok(());
            }
            Some(_) => {}
        }
        if Instant::now() >= end {
            return Err("recorded fixture did not settle after handle-based cleanup".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
