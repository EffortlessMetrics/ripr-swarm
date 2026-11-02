//! Periodic parent-process liveness probe for the stdio LSP sidecar (#12575).
//!
//! In-band session ends (`exit`, stdin EOF) already fire [`SessionEnd`]. When
//! the launching client dies without closing stdio, nothing on the wire
//! signals teardown. On Unix, the kernel reparents the server and
//! [`parent_id`](std::os::unix::process::parent_id) changes; detecting that
//! drift runs the same session-end hook and exits the process.

use std::time::Duration;

use super::session_end::SessionEnd;

const DEFAULT_PROBE_MS: u64 = 250;

fn probe_interval() -> Duration {
    std::env::var("RIPR_LSP_PARENT_DEATH_PROBE_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_millis(DEFAULT_PROBE_MS))
}

/// Pure decision for the periodic probe (initial parent vs current lookup).
pub(super) fn parent_death_detected(initial: u32, current: Option<u32>) -> bool {
    match current {
        Some(current) if current == initial => false,
        Some(_) => true,
        None => false,
    }
}

#[cfg(unix)]
fn parent_process_id() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

#[cfg(not(unix))]
fn parent_process_id() -> Option<u32> {
    // Windows: std and the reviewed safe Windows deps do not expose parent-PID
    // lookup without raw ProcessBasicInformation; out of scope until a safe
    // API is available (#12575).
    None
}

pub(super) fn spawn_parent_death_watchdog(session_end: SessionEnd) {
    let Some(initial) = parent_process_id() else {
        return;
    };
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(probe_interval());
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if parent_death_detected(initial, parent_process_id()) {
                session_end.signal();
                std::process::exit(0);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::parent_death_detected;

    #[test]
    fn unchanged_parent_continues() {
        assert!(!parent_death_detected(42, Some(42)));
    }

    #[test]
    fn reparented_parent_stops() {
        assert!(parent_death_detected(42, Some(1)));
    }

    #[test]
    fn missing_lookup_is_not_a_stop() {
        assert!(!parent_death_detected(42, None));
    }
}
