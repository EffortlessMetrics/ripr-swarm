//! Exit notification only. `OwnedProcess` remains the authority for wait/reap
//! and tree cleanup; this observer never signals or consumes child status.

use std::time::Duration;

pub(super) struct CompletionWait {
    #[cfg(target_os = "linux")]
    pidfd: Option<rustix::fd::OwnedFd>,
}

impl CompletionWait {
    pub(super) fn new(pid: u32) -> Self {
        #[cfg(target_os = "linux")]
        {
            // The caller still owns the unreaped child. A pidfd opened here
            // cannot accidentally observe a later reuse of its numeric PID.
            // Older kernels and restricted hosts may refuse pidfd_open; those
            // hosts retain the bounded polling fallback, without a wait thread.
            Self {
                pidfd: i32::try_from(pid)
                    .ok()
                    .and_then(rustix::process::Pid::from_raw)
                    .and_then(|pid| {
                        rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()).ok()
                    }),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            Self {}
        }
    }

    /// Sleep until exit notification or the caller's next deadline/sample
    /// check. `true` means notification, never successful reap or tree cleanup.
    pub(super) fn wait(&self, budget: Duration) -> std::io::Result<bool> {
        #[cfg(target_os = "linux")]
        if let Some(pidfd) = &self.pidfd {
            use rustix::event::{PollFd, PollFlags, Timespec, poll};
            let timeout = Timespec::try_from(budget).map_err(std::io::Error::other)?;
            let mut fds = [PollFd::new(pidfd, PollFlags::IN)];
            match poll(&mut fds, Some(&timeout)) {
                Ok(_) => {
                    let events = fds[0].revents();
                    if events.intersects(PollFlags::ERR | PollFlags::NVAL) {
                        return Err(std::io::Error::other("invalid child completion descriptor"));
                    }
                    return Ok(events.intersects(PollFlags::IN | PollFlags::HUP));
                }
                // Return control for an authoritative exit/deadline check;
                // restarting a full wait here would extend the caller's budget.
                Err(rustix::io::Errno::INTR) => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
        Self::polling_wait(budget);
        Ok(false)
    }

    fn polling_wait(budget: Duration) {
        std::thread::sleep(Self::polling_budget(budget));
    }

    fn polling_budget(budget: Duration) -> Duration {
        budget.min(Duration::from_millis(5))
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn use_polling_for_test(&mut self) {
        self.pidfd = None;
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn has_native_notification(&self) -> bool {
        self.pidfd.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_preserves_short_deadlines_and_caps_observation_delay() {
        assert_eq!(
            CompletionWait::polling_budget(Duration::ZERO),
            Duration::ZERO
        );
        assert_eq!(
            CompletionWait::polling_budget(Duration::from_micros(250)),
            Duration::from_micros(250)
        );
        assert_eq!(
            CompletionWait::polling_budget(Duration::from_secs(30)),
            Duration::from_millis(5)
        );
    }
}
