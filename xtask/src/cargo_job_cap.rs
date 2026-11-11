//! Build-job cap for local `precommit` on memory-constrained hosts (#13705).
//!
//! `ripr` and `xtask` are each one large compilation unit. On a 4-core,
//! 15 GB host, whole-workspace Clippy checks the `ripr` library test (about
//! 9.8 GB) and, beside it, an `xtask` unit. At Cargo's default job count that
//! run was stopped by the memory limit; at two jobs one run passed (12.8 GB
//! peak) and the next was stopped (13.6 GB). Only one job keeps the large
//! units apart.
//!
//! Local `precommit` therefore sets `CARGO_BUILD_JOBS=1` for the Clippy and
//! enumeration children it starts when the host has less memory than
//! [`LOW_MEMORY_BYTES`] and more than one CPU. An explicit `CARGO_BUILD_JOBS`
//! always wins, a host whose memory cannot be read is left alone, and hosted
//! `ci-policy` never uses this.

use std::sync::OnceLock;

/// Below this much memory the cap applies. The uncapped whole-workspace
/// Clippy run needs more than 15 GB; 24 GiB leaves headroom above that.
const LOW_MEMORY_BYTES: u64 = 24 * 1024 * 1024 * 1024;
const CAPPED_JOBS: &str = "1";
const JOBS_VARIABLE: &str = "CARGO_BUILD_JOBS";

/// The job count to impose, or `None` to leave Cargo's own choice.
fn capped_jobs(
    explicit_jobs: bool,
    memory_bytes: Option<u64>,
    cpus: Option<usize>,
) -> Option<&'static str> {
    let constrained = memory_bytes.is_some_and(|bytes| bytes < LOW_MEMORY_BYTES);
    let parallel = cpus.is_some_and(|count| count > 1);
    (!explicit_jobs && constrained && parallel).then_some(CAPPED_JOBS)
}

/// `MemTotal` from `/proc/meminfo` text, in bytes.
fn meminfo_total_bytes(text: &str) -> Option<u64> {
    let line = text
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?;
    let mut words = line.split_whitespace();
    let kibibytes = words.next()?.parse::<u64>().ok()?;
    (words.next() == Some("kB")).then(|| kibibytes.saturating_mul(1024))
}

/// A cgroup memory limit file's value in bytes. `max` means no limit.
fn cgroup_limit_bytes(text: &str) -> Option<u64> {
    text.trim().parse::<u64>().ok()
}

/// The smallest memory bound this process can see, or `None` off Linux or
/// when nothing is readable.
fn host_memory_bytes() -> Option<u64> {
    let read = |path: &str| std::fs::read_to_string(path).ok();
    [
        read("/proc/meminfo").and_then(|text| meminfo_total_bytes(&text)),
        read("/sys/fs/cgroup/memory.max").and_then(|text| cgroup_limit_bytes(&text)),
        read("/sys/fs/cgroup/memory/memory.limit_in_bytes")
            .and_then(|text| cgroup_limit_bytes(&text)),
    ]
    .into_iter()
    .flatten()
    .min()
}

/// Environment for the Cargo children of a local `precommit`: empty, or the
/// job cap. Decided once per process and announced on stderr when it applies.
pub(crate) fn local_precommit_envs() -> &'static [(&'static str, &'static str)] {
    static ENVS: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    ENVS.get_or_init(|| {
        let memory = host_memory_bytes();
        let cpus = std::thread::available_parallelism()
            .ok()
            .map(std::num::NonZeroUsize::get);
        let explicit = std::env::var_os(JOBS_VARIABLE).is_some();
        match capped_jobs(explicit, memory, cpus) {
            Some(jobs) => {
                eprintln!(
                    "precommit: host memory is below 24 GiB; running cargo with {JOBS_VARIABLE}={jobs} (set {JOBS_VARIABLE} to override)"
                );
                vec![(JOBS_VARIABLE, jobs)]
            }
            None => Vec::new(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{LOW_MEMORY_BYTES, capped_jobs, cgroup_limit_bytes, meminfo_total_bytes};

    /// #13705: the cap applies only to a readable, constrained, parallel host
    /// with no explicit job count.
    #[test]
    fn job_cap_applies_only_to_a_constrained_parallel_host_without_an_explicit_count() {
        let small = Some(16 * 1024 * 1024 * 1024);
        assert_eq!(capped_jobs(false, small, Some(4)), Some("1"));
        assert_eq!(
            capped_jobs(false, Some(LOW_MEMORY_BYTES - 1), Some(2)),
            Some("1")
        );

        assert_eq!(
            capped_jobs(true, small, Some(4)),
            None,
            "explicit count wins"
        );
        assert_eq!(
            capped_jobs(false, Some(LOW_MEMORY_BYTES), Some(4)),
            None,
            "boundary"
        );
        assert_eq!(capped_jobs(false, None, Some(4)), None, "unreadable memory");
        assert_eq!(capped_jobs(false, small, Some(1)), None, "already one job");
        assert_eq!(
            capped_jobs(false, small, None),
            None,
            "unreadable CPU count"
        );
    }

    #[test]
    fn memory_sources_parse_only_their_exact_shapes() {
        let meminfo = "MemFree:  1 kB\nMemTotal:       16472084 kB\nMemAvailable: 2 kB\n";
        assert_eq!(meminfo_total_bytes(meminfo), Some(16_472_084 * 1024));
        assert_eq!(meminfo_total_bytes("MemTotal: 12 MB\n"), None);
        assert_eq!(meminfo_total_bytes("MemTotal: many kB\n"), None);
        assert_eq!(meminfo_total_bytes("MemFree: 1 kB\n"), None);

        assert_eq!(cgroup_limit_bytes("16106127360\n"), Some(16_106_127_360));
        assert_eq!(cgroup_limit_bytes("max\n"), None);
        assert_eq!(cgroup_limit_bytes(""), None);
    }
}
