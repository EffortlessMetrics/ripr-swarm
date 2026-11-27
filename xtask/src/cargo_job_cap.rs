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
pub(crate) const LOW_MEMORY_BYTES: u64 = 24 * 1024 * 1024 * 1024;
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
#[cfg(any(not(target_os = "windows"), test))]
fn meminfo_total_bytes(text: &str) -> Option<u64> {
    let line = text
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?;
    let mut words = line.split_whitespace();
    let kibibytes = words.next()?.parse::<u64>().ok()?;
    (words.next() == Some("kB")).then(|| kibibytes.saturating_mul(1024))
}

/// A cgroup memory limit file's value in bytes. `max` means no limit.
#[cfg(any(not(target_os = "windows"), test))]
fn cgroup_limit_bytes(text: &str) -> Option<u64> {
    text.trim().parse::<u64>().ok()
}

/// The memory-limit files that bound this process, from `/proc/self/cgroup`
/// text: its own cgroup and every ancestor up to the root, for cgroup v2
/// (`0::<path>`) and for a v1 hierarchy carrying the `memory` controller. A
/// nested child limit below an unlimited root is therefore seen. Without a
/// readable membership, the two root files.
#[cfg(any(not(target_os = "windows"), test))]
fn cgroup_limit_files(membership: Option<&str>) -> Vec<String> {
    let Some(text) = membership else {
        return vec![
            "/sys/fs/cgroup/memory.max".to_string(),
            "/sys/fs/cgroup/memory/memory.limit_in_bytes".to_string(),
        ];
    };
    let mut files = Vec::new();
    for line in text.lines() {
        let mut fields = line.splitn(3, ':');
        let (Some(id), Some(controllers), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (base, file) = if id == "0" && controllers.is_empty() {
            ("/sys/fs/cgroup", "memory.max")
        } else if controllers.split(',').any(|name| name == "memory") {
            ("/sys/fs/cgroup/memory", "memory.limit_in_bytes")
        } else {
            continue;
        };
        let mut ancestor = path.trim_end_matches('/');
        loop {
            files.push(format!("{base}{ancestor}/{file}"));
            match ancestor.rfind('/') {
                Some(cut) => ancestor = &ancestor[..cut],
                None => break,
            }
        }
    }
    files
}

/// The smallest of `MemTotal` and every readable cgroup limit on this
/// process's membership path, read through `read`.
#[cfg(any(not(target_os = "windows"), test))]
fn smallest_memory_bound(read: &dyn Fn(&str) -> Option<String>) -> Option<u64> {
    let membership = read("/proc/self/cgroup");
    let limits = cgroup_limit_files(membership.as_deref())
        .into_iter()
        .filter_map(|file| read(&file).and_then(|text| cgroup_limit_bytes(&text)));
    read("/proc/meminfo")
        .and_then(|text| meminfo_total_bytes(&text))
        .into_iter()
        .chain(limits)
        .min()
}

/// Timeout for a single Windows host-memory probe subprocess.
#[cfg(target_os = "windows")]
const WINDOWS_MEMORY_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowsMemoryUnit {
    Bytes,
    Kibibytes,
}

/// Parse a single positive integer memory reading from a Windows memory query.
#[cfg(any(target_os = "windows", test))]
fn parse_windows_memory(text: &str, unit: WindowsMemoryUnit) -> Option<u64> {
    let value = text.trim().parse::<u64>().ok()?;
    if value == 0 {
        return None;
    }
    match unit {
        WindowsMemoryUnit::Bytes => Some(value),
        WindowsMemoryUnit::Kibibytes => value.checked_mul(1024),
    }
}

/// Query Windows host memory without `unsafe` FFI by trying bounded PowerShell
/// probes in order:
/// 1. `pwsh` `.NET` `GC.GetGCMemoryInfo().TotalAvailableMemoryBytes` (bytes;
///    respects Windows Job Object memory limits as well as physical RAM),
/// 2. Windows PowerShell 5.1 `Microsoft.VisualBasic.Devices.ComputerInfo.TotalPhysicalMemory`
///    (bytes; uses `GlobalMemoryStatusEx` without WMI/CIM),
/// 3. Windows PowerShell 5.1 `Win32_OperatingSystem.TotalVisibleMemorySize` (KiB).
#[cfg(any(target_os = "windows", test))]
fn windows_memory_bytes_with(
    run_probe: &dyn Fn(&str, &[&str]) -> Option<String>,
    windows_powershell_path: &str,
) -> Option<u64> {
    let probes: [(&str, &[&str], WindowsMemoryUnit); 3] = [
        (
            "pwsh",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[GC]::GetGCMemoryInfo().TotalAvailableMemoryBytes",
            ],
            WindowsMemoryUnit::Bytes,
        ),
        (
            windows_powershell_path,
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Add-Type -AssemblyName Microsoft.VisualBasic; ([Microsoft.VisualBasic.Devices.ComputerInfo]::new()).TotalPhysicalMemory",
            ],
            WindowsMemoryUnit::Bytes,
        ),
        (
            windows_powershell_path,
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance -ClassName Win32_OperatingSystem).TotalVisibleMemorySize",
            ],
            WindowsMemoryUnit::Kibibytes,
        ),
    ];
    for (program, args, unit) in probes {
        if let Some(bytes) =
            run_probe(program, args).and_then(|output| parse_windows_memory(&output, unit))
        {
            return Some(bytes);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_powershell_executable() -> String {
    if let Some(root) = std::env::var_os("SystemRoot") {
        let candidate = std::path::PathBuf::from(root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    "powershell.exe".to_string()
}

#[cfg(target_os = "windows")]
fn run_windows_memory_probe(program: &str, args: &[&str]) -> Option<String> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        std::path::Path::new(program),
        &args,
        &cwd,
        &[],
        &[],
        WINDOWS_MEMORY_PROBE_TIMEOUT,
        "windows host memory probe",
    )
    .ok()?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(target_os = "windows")]
fn detect_host_memory_bytes() -> Option<u64> {
    let powershell = windows_powershell_executable();
    windows_memory_bytes_with(&run_windows_memory_probe, &powershell)
}

#[cfg(not(target_os = "windows"))]
fn detect_host_memory_bytes() -> Option<u64> {
    smallest_memory_bound(&|path| std::fs::read_to_string(path).ok())
}

/// The smallest memory bound this process can see on Linux (`/proc/meminfo`
/// and cgroup limits) or Windows (`.NET` / PowerShell memory query), or
/// `None` on other platforms or when nothing is readable.
pub(crate) fn host_memory_bytes() -> Option<u64> {
    static MEMORY_BYTES: OnceLock<Option<u64>> = OnceLock::new();
    *MEMORY_BYTES.get_or_init(detect_host_memory_bytes)
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
    use super::{
        LOW_MEMORY_BYTES, WindowsMemoryUnit, capped_jobs, cgroup_limit_bytes, meminfo_total_bytes,
        parse_windows_memory, smallest_memory_bound, windows_memory_bytes_with,
    };
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn bound(files: &[(&str, String)]) -> Option<u64> {
        let files = files
            .iter()
            .map(|(path, text)| (path.to_string(), text.clone()))
            .collect::<BTreeMap<_, _>>();
        smallest_memory_bound(&|path| files.get(path).cloned())
    }

    /// #14417 review: the operative limit can sit on a nested cgroup below an
    /// unlimited root, or on an ancestor of this process's own cgroup. Both
    /// are found for cgroup v2 and v1; a sibling's limit is not.
    #[test]
    fn nested_and_ancestor_cgroup_limits_bound_memory() {
        let meminfo = (
            "/proc/meminfo",
            format!("MemTotal: {} kB\n", 64 * GIB / 1024),
        );
        let fifteen = (15 * GIB).to_string();
        let v2 = ("/proc/self/cgroup", "0::/outer/job\n".to_string());
        assert_eq!(
            bound(&[
                meminfo.clone(),
                v2.clone(),
                ("/sys/fs/cgroup/memory.max", "max\n".to_string()),
                ("/sys/fs/cgroup/outer/job/memory.max", fifteen.clone()),
            ]),
            Some(15 * GIB),
            "v2 child limited, root unlimited"
        );
        assert_eq!(
            bound(&[
                meminfo.clone(),
                v2.clone(),
                ("/sys/fs/cgroup/outer/memory.max", fifteen.clone()),
                ("/sys/fs/cgroup/outer/job/memory.max", "max\n".to_string()),
            ]),
            Some(15 * GIB),
            "v2 parent limited"
        );
        let v1 = (
            "/proc/self/cgroup",
            "9:pids:/a\n4:memory:/a/b\n0::/\n".to_string(),
        );
        assert_eq!(
            bound(&[
                meminfo.clone(),
                v1.clone(),
                (
                    "/sys/fs/cgroup/memory/memory.limit_in_bytes",
                    "9223372036854771712\n".to_string()
                ),
                (
                    "/sys/fs/cgroup/memory/a/b/memory.limit_in_bytes",
                    fifteen.clone()
                ),
            ]),
            Some(15 * GIB),
            "v1 child limited, root unlimited"
        );
        assert_eq!(
            bound(&[
                meminfo.clone(),
                v1.clone(),
                (
                    "/sys/fs/cgroup/memory/a/memory.limit_in_bytes",
                    fifteen.clone()
                ),
            ]),
            Some(15 * GIB),
            "v1 parent limited"
        );
        assert_eq!(
            bound(&[
                meminfo.clone(),
                v2.clone(),
                ("/sys/fs/cgroup/other/memory.max", fifteen.clone()),
            ]),
            Some(64 * GIB),
            "a sibling cgroup's limit does not apply"
        );
        assert_eq!(
            bound(&[meminfo.clone(), ("/sys/fs/cgroup/memory.max", fifteen),]),
            Some(15 * GIB),
            "no readable membership: the root file still counts"
        );
        assert_eq!(bound(&[]), None, "nothing readable");
    }

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

    #[test]
    fn windows_memory_parsers_accept_only_positive_integers() {
        assert_eq!(
            parse_windows_memory("272315080704\r\n", WindowsMemoryUnit::Bytes),
            Some(272_315_080_704)
        );
        assert_eq!(
            parse_windows_memory("17179869184\n", WindowsMemoryUnit::Bytes),
            Some(16 * GIB)
        );
        assert_eq!(
            parse_windows_memory("16777216\r\n", WindowsMemoryUnit::Kibibytes),
            Some(16 * GIB)
        );

        for bad in [
            "",
            "   \r\n",
            "0\r\n",
            "-1",
            "16.5",
            "16 GB",
            "warn\n17179869184\n",
        ] {
            assert_eq!(
                parse_windows_memory(bad, WindowsMemoryUnit::Bytes),
                None,
                "bytes should reject {bad:?}"
            );
            assert_eq!(
                parse_windows_memory(bad, WindowsMemoryUnit::Kibibytes),
                None,
                "kibibytes should reject {bad:?}"
            );
        }
        assert_eq!(
            parse_windows_memory(&u64::MAX.to_string(), WindowsMemoryUnit::Kibibytes),
            None,
            "kibibytes overflow must reject instead of saturating"
        );
    }

    #[test]
    fn windows_memory_probe_falls_back_through_powershell_and_caps_low_memory_hosts() {
        const PS51: &str = "powershell.exe";

        let calls = RefCell::new(Vec::new());
        let low = windows_memory_bytes_with(
            &|program, args| {
                calls.borrow_mut().push((
                    program.to_string(),
                    args.last().copied().unwrap_or_default().to_string(),
                ));
                (program == "pwsh").then(|| format!("{}\r\n", 16 * GIB))
            },
            PS51,
        );
        assert_eq!(low, Some(16 * GIB));
        assert_eq!(calls.borrow().len(), 1, "primary pwsh stops fallback chain");
        assert_eq!(capped_jobs(false, low, Some(4)), Some("1"));
        assert_eq!(capped_jobs(true, low, Some(4)), None);

        let high = windows_memory_bytes_with(
            &|program, _| (program == "pwsh").then(|| format!("{}\r\n", 256 * GIB)),
            PS51,
        );
        assert_eq!(high, Some(256 * GIB));
        assert_eq!(capped_jobs(false, high, Some(32)), None);

        let computer_info = windows_memory_bytes_with(
            &|program, args| {
                if program == PS51 && args.last().is_some_and(|cmd| cmd.contains("ComputerInfo")) {
                    Some(format!("{}\r\n", 16 * GIB))
                } else {
                    None
                }
            },
            PS51,
        );
        assert_eq!(computer_info, Some(16 * GIB));
        assert_eq!(capped_jobs(false, computer_info, Some(4)), Some("1"));

        let cim_fallback = windows_memory_bytes_with(
            &|program, args| {
                if program == "pwsh" {
                    Some("not-a-number\r\n".to_string())
                } else if program == PS51
                    && args
                        .last()
                        .is_some_and(|cmd| cmd.contains("Win32_OperatingSystem"))
                {
                    Some(format!("{}\r\n", 16 * GIB / 1024))
                } else {
                    None
                }
            },
            PS51,
        );
        assert_eq!(cim_fallback, Some(16 * GIB));

        let unreadable = windows_memory_bytes_with(&|_, _| None, PS51);
        assert_eq!(unreadable, None);
        assert_eq!(capped_jobs(false, unreadable, Some(4)), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_windows_host_memory_bytes_detects_physical_memory() -> Result<(), String> {
        let first = super::host_memory_bytes();
        let second = super::host_memory_bytes();
        assert_eq!(first, second, "cached host memory must be stable");
        let bytes = first.ok_or_else(|| {
            "native Windows host memory probe should return a positive byte count".to_string()
        })?;
        assert!(
            bytes >= 512 * 1024 * 1024,
            "detected Windows memory {bytes} bytes is implausibly small"
        );
        Ok(())
    }
}
