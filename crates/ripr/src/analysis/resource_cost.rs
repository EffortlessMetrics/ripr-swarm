//! Process resource-cost observability (#5213).
//!
//! The opt-in latency trace family ([`LATENCY_TRACE_ENV`]) reports wall-clock
//! time only, so `ripr` could not falsify its own efficiency claims: "fast",
//! "bounded", "minimal working set" had no CPU or memory evidence behind
//! them. This module adds the missing observation and nothing else. It
//! changes no cost, no classification, no output document, and no exit code.
//!
//! ## What is observed
//!
//! One [`ResourceCostObservation`] of **this** `ripr` process, taken once
//! after the command has finished so the peak covers rendering and
//! serialization instead of stopping at the analysis boundary. It carries:
//!
//! * CPU time attributed to this process, split user/system, with the raw
//!   host value, the unit the host counted it in, and the divisor used to
//!   reach milliseconds.
//! * Peak resident set size of this process.
//! * The host identity (`host_os`, `host_arch`) and the observer scope, so a
//!   reader can tell these numbers belong to the analyzer and not to the
//!   harness that launched it.
//!
//! ## Honesty contract
//!
//! Every number is either observed or explicitly unavailable with a named
//! reason ([`ResourceMeasurement`], [`CpuCost`]). The two states are distinct
//! serialized shapes, so an unavailable observation cannot parse, render, or
//! aggregate as `0`, and nothing is silently omitted. A receipt that cannot be
//! serialized at all still reports every field as unavailable rather than
//! disappearing or emitting a placeholder number.
//!
//! ## Platform support, and why
//!
//! * **Linux** - observed. CPU time from `/proc/self/stat` `utime`+`stime`
//!   counted in `USER_HZ` clock ticks; peak resident set from
//!   `/proc/self/status` `VmHWM`. Both are plain file reads: no `unsafe`, no
//!   dependency, no raw FFI.
//! * **Windows** - explicitly unavailable. The `winsafe` 0.0.29 surface this
//!   crate already depends on exposes no safe per-process counter.
//!   `GetProcessTimes`, `GetProcessMemoryInfo`, and
//!   `NtQueryInformationProcess` are all unwrapped (`kernel/ffi.rs` declares
//!   the first two as raw `unsafe` externs), and the wrappers that *are*
//!   safe - `GetPerformanceInfo` under `psapi`, `GlobalMemoryStatusEx` under
//!   `kernel` - are machine-wide aggregates that cannot answer a per-process
//!   question. Reporting a machine aggregate as this process's cost would be a
//!   false claim, so the receipt names
//!   [`ResourceUnavailableReason::MachineWideOrUnsafeOnly`] instead. Closing
//!   this gap needs either a `winsafe` release that wraps those calls or an
//!   explicitly authorized FFI surface; both are outside this slice.
//! * **Every other target** - explicitly unavailable as
//!   [`ResourceUnavailableReason::PlatformNotSupported`].
//!
//! ## Cost of the observation
//!
//! One bounded file read per metric on Linux, once per run, and one short JSON
//! line on stderr. With [`LATENCY_TRACE_ENV`] unset nothing is read and
//! nothing is printed, so default stdout JSON and default stderr are
//! unchanged.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The opt-in stderr trace switch this module owns. Presence enables
/// tracing; the value is never read, so `RIPR_REPO_EXPOSURE_LATENCY_TRACE=0`
/// still enables it - the same contract the phase trace already had.
pub(crate) const LATENCY_TRACE_ENV: &str = "RIPR_REPO_EXPOSURE_LATENCY_TRACE";

/// Prefix of the single-line resource-cost receipt on stderr.
pub(crate) const RESOURCE_COST_RECEIPT_PREFIX: &str = "ripr_resource_cost_receipt ";

/// Schema version of the emitted resource-cost receipt.
pub(crate) const RESOURCE_COST_SCHEMA_VERSION: &str = "0.1";

/// Attribution string on every receipt. The numbers are read from inside the
/// analyzed `ripr` process; no harness measures them on this process's behalf.
pub(crate) const RESOURCE_COST_OBSERVER_SCOPE: &str = "ripr_process_self";

/// Whether the opt-in trace family is enabled for this process.
pub(crate) fn latency_trace_enabled() -> bool {
    std::env::var_os(LATENCY_TRACE_ENV).is_some()
}

/// Emit one wall-clock phase line. The single owner of that line shape for
/// every analysis phase, so a new phase cannot invent a parallel spelling.
pub(crate) fn trace_latency_phase(phase: &str, status: &str, duration: Duration) {
    if latency_trace_enabled() {
        eprintln!("{}", latency_trace_line(phase, status, duration));
    }
}

/// The wall-clock phase line, named by callers that only render it. Crate
/// visible so a consumer's test can assert the exact wire shape without
/// reading stderr.
pub(crate) fn latency_trace_line(phase: &str, status: &str, duration: Duration) -> String {
    format!(
        "ripr_repo_exposure_latency phase={phase} status={status} duration_ms={}",
        duration.as_millis()
    )
}

/// Why a resource number is not a measurement.
///
/// Each variant names a capability this crate deliberately does not fake, so a
/// reader can tell an absent observation from a zero one and can tell which
/// missing capability would unblock it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResourceUnavailableReason {
    /// The host platform has no safe, dependency-free per-process source in
    /// this crate's `unsafe_code = "forbid"` posture.
    PlatformNotSupported,
    /// The host offers the capability only through a machine-wide aggregate,
    /// or through an `unsafe` per-process call this crate's policy excludes.
    /// Both answer a different question, so neither may stand in for this
    /// process's own cost.
    MachineWideOrUnsafeOnly,
    /// The documented source file could not be read at all.
    SourceUnreadable,
    /// The documented source file was read but the expected field was absent.
    SourceFieldMissing,
    /// The expected field was present but did not parse as its documented unit.
    SourceValueMalformed,
}

/// The named reason a host offers no safe per-process resource source.
///
/// Total and platform-independent so the mapping is the same code the
/// observation path uses, and so a test can pin every platform's wording on
/// any host.
pub(crate) fn unavailable_reason_for(os: &str) -> ResourceUnavailableReason {
    match os.as_bytes() {
        // `windows` is the only target whose safe wrappers are machine-wide
        // aggregates; naming it keeps that finding distinct from "this crate
        // has no source for that platform at all".
        b"windows" => ResourceUnavailableReason::MachineWideOrUnsafeOnly,
        _ => ResourceUnavailableReason::PlatformNotSupported,
    }
}

/// The unit a host counts per-process CPU time in, and the documented number
/// of those units per second.
///
/// The rate is a documented ABI constant of the named source, not a
/// measurement of this process, and it travels in the receipt so a reader can
/// recompute the millisecond fields from the raw source fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CpuSourceUnit {
    /// Linux `/proc/self/stat` `utime` and `stime`, counted in `USER_HZ`
    /// clock ticks. `USER_HZ` is fixed at 100 by the Linux userspace ABI
    /// (`getconf CLK_TCK` reports 100 on Linux), so 100 source intervals per
    /// millisecond.
    LinuxUserHzClockTicks,
    /// Windows `GetProcessTimes`, counted in 100-nanosecond intervals, so
    /// 10 000 000 intervals per second.
    ///
    /// Named so the unit contract is total and testable on any host. The
    /// Windows read is not implemented (see the module docs), so this unit
    /// never reaches a Windows receipt.
    WindowsHundredNanoseconds,
}

impl CpuSourceUnit {
    /// Source intervals per second for this unit.
    fn source_unit_per_second(self) -> u64 {
        match self {
            Self::LinuxUserHzClockTicks => 100,
            Self::WindowsHundredNanoseconds => 10_000_000,
        }
    }
}

/// The unit this host counts per-process CPU time in, or `None` on a host with
/// no known unit.
fn cpu_source_unit_for(os: &str) -> Option<CpuSourceUnit> {
    match os.as_bytes() {
        b"linux" => Some(CpuSourceUnit::LinuxUserHzClockTicks),
        b"windows" => Some(CpuSourceUnit::WindowsHundredNanoseconds),
        _ => None,
    }
}

/// CPU time attributed to this process, or the named reason it could not be.
///
/// User and kernel time come from one host source, so they are observed
/// together or not at all: a receipt that carried one without the other would
/// present a partial observation as a complete one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum CpuCost {
    /// Measured on this host, with the raw value and the divisor kept so the
    /// millisecond fields are auditable rather than trusted.
    Observed {
        source_unit: CpuSourceUnit,
        source_unit_per_second: u64,
        user_source: u64,
        system_source: u64,
        user_ms: u64,
        system_ms: u64,
    },
    /// No safe per-process CPU source was available here.
    Unavailable { reason: ResourceUnavailableReason },
}

impl CpuCost {
    fn from_source(unit: CpuSourceUnit, user_source: u64, system_source: u64) -> Self {
        let source_unit_per_second = unit.source_unit_per_second();
        Self::Observed {
            source_unit: unit,
            source_unit_per_second,
            user_source,
            system_source,
            // Truncating the remainder keeps a sub-millisecond observation
            // visible as zero rather than rounding a cost up past what was
            // actually used.
            user_ms: source_to_ms(user_source, source_unit_per_second),
            system_ms: source_to_ms(system_source, source_unit_per_second),
        }
    }
}

fn source_to_ms(source: u64, source_unit_per_second: u64) -> u64 {
    u128::from(source)
        .saturating_mul(1000)
        .checked_div(u128::from(source_unit_per_second.max(1)))
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or(u64::MAX)
}

/// One resource number: measured, or explicitly unavailable.
///
/// The two states are separate serialized shapes, so an unavailable
/// observation cannot round-trip, render, or aggregate as `0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum ResourceMeasurement {
    /// Measured on this process by the host's documented safe source.
    Observed { value: u64 },
    /// Not measured here, with the reason named.
    Unavailable { reason: ResourceUnavailableReason },
}

/// Read this host's per-process CPU source text, or name why there is none.
///
/// Compiled on every platform so the shared parse and conversion path below is
/// always the same code, and so a test on any host covers it.
fn cpu_source_text() -> Result<String, ResourceUnavailableReason> {
    #[cfg(target_os = "linux")]
    {
        return std::fs::read_to_string("/proc/self/stat")
            .map_err(|_unreadable| ResourceUnavailableReason::SourceUnreadable);
    }
    #[cfg(not(target_os = "linux"))]
    {
        // This host's documented per-process CPU source needs either an FFI
        // surface or a machine-wide aggregate, so there is no text to parse.
        Err(unavailable_reason_for(std::env::consts::OS))
    }
}

/// Read this host's per-process peak-resident source text, or name why there
/// is none.
fn peak_resident_source_text() -> Result<String, ResourceUnavailableReason> {
    #[cfg(target_os = "linux")]
    {
        return std::fs::read_to_string("/proc/self/status")
            .map_err(|_unreadable| ResourceUnavailableReason::SourceUnreadable);
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(unavailable_reason_for(std::env::consts::OS))
    }
}

/// Field 14 (`utime`) and field 15 (`stime`), 1-indexed within the whole
/// `/proc/self/stat` line.
///
/// `comm` (field 2) is parenthesized and may itself contain spaces and
/// parentheses, so the remaining fields are counted from the last `)`. After
/// it, `state` is field 3, which puts `utime` at offset 11 and `stime` at
/// offset 12 - 13 fields in total.
const LINUX_STAT_CPU_FIELD_COUNT: usize = 13;

const LINUX_STAT_USER_TICK_OFFSET: usize = LINUX_STAT_CPU_FIELD_COUNT - 2;

/// Parse `utime` and `stime` clock ticks out of a `/proc/self/stat` body.
///
/// A pure function over the file text, so the parse path is covered by tests
/// on any host, not only on the host that produces the file.
fn parse_cpu_source_ticks(text: &str) -> Result<(u64, u64), ResourceUnavailableReason> {
    let Some(closing_paren) = text.rfind(')') else {
        return Err(ResourceUnavailableReason::SourceValueMalformed);
    };
    let after_comm = text
        .get(closing_paren.saturating_add(1)..)
        .unwrap_or_default();
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    if fields.len() < LINUX_STAT_CPU_FIELD_COUNT {
        return Err(ResourceUnavailableReason::SourceFieldMissing);
    }
    match (
        fields[LINUX_STAT_USER_TICK_OFFSET].parse::<u64>(),
        fields[LINUX_STAT_USER_TICK_OFFSET + 1].parse::<u64>(),
    ) {
        (Ok(user), Ok(system)) => Ok((user, system)),
        _ => Err(ResourceUnavailableReason::SourceValueMalformed),
    }
}

const LINUX_STATUS_PEAK_RSS_FIELD: &str = "VmHWM:";
const LINUX_STATUS_VALUE_UNIT: &str = "kB";
const BYTES_PER_KIBIBYTE: u64 = 1024;

/// Parse peak resident set bytes out of a `/proc/self/status` body.
///
/// `VmHWM` is the kernel's own high-water mark for this process's resident
/// set, reported in kibibytes. A pure function over the file text, so the
/// parse path is covered by tests on any host.
fn parse_peak_resident_source_bytes(text: &str) -> Result<u64, ResourceUnavailableReason> {
    let Some(line) = text
        .lines()
        .find(|line| line.starts_with(LINUX_STATUS_PEAK_RSS_FIELD))
    else {
        return Err(ResourceUnavailableReason::SourceFieldMissing);
    };
    let rest = line
        .get(LINUX_STATUS_PEAK_RSS_FIELD.len()..)
        .unwrap_or_default();
    let mut parts = rest.split_whitespace();
    let (Some(amount), Some(unit), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(ResourceUnavailableReason::SourceValueMalformed);
    };
    if unit != LINUX_STATUS_VALUE_UNIT {
        return Err(ResourceUnavailableReason::SourceValueMalformed);
    }
    let kibibytes = amount
        .parse::<u64>()
        .map_err(|_not_a_count| ResourceUnavailableReason::SourceValueMalformed)?;
    kibibytes
        .checked_mul(BYTES_PER_KIBIBYTE)
        .ok_or(ResourceUnavailableReason::SourceValueMalformed)
}

/// CPU time of this process, or the named reason it could not be read.
pub(crate) fn observe_cpu_cost() -> CpuCost {
    let unit = match cpu_source_unit_for(std::env::consts::OS) {
        Some(unit) => unit,
        None => {
            return CpuCost::Unavailable {
                reason: unavailable_reason_for(std::env::consts::OS),
            };
        }
    };
    match cpu_source_text().and_then(|text| parse_cpu_source_ticks(&text)) {
        Ok((user_source, system_source)) => CpuCost::from_source(unit, user_source, system_source),
        Err(reason) => CpuCost::Unavailable { reason },
    }
}

/// Peak resident set size of this process, or the named reason it could not be
/// read.
pub(crate) fn observe_peak_resident_bytes() -> ResourceMeasurement {
    match peak_resident_source_text().and_then(|text| parse_peak_resident_source_bytes(&text)) {
        Ok(value) => ResourceMeasurement::Observed { value },
        Err(reason) => ResourceMeasurement::Unavailable { reason },
    }
}

/// One resource-cost observation of this process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceCostObservation {
    pub(crate) schema_version: String,
    /// Always [`RESOURCE_COST_OBSERVER_SCOPE`]. Carried as data so a consumer
    /// that never sees the documentation still knows whose cost these are.
    pub(crate) observer: String,
    pub(crate) observer_pid: u32,
    pub(crate) host_os: String,
    pub(crate) host_arch: String,
    pub(crate) cpu: CpuCost,
    pub(crate) peak_resident_bytes: ResourceMeasurement,
}

impl ResourceCostObservation {
    /// Observe this process now.
    pub(crate) fn observe() -> Self {
        Self {
            schema_version: RESOURCE_COST_SCHEMA_VERSION.to_string(),
            observer: RESOURCE_COST_OBSERVER_SCOPE.to_string(),
            observer_pid: std::process::id(),
            host_os: std::env::consts::OS.to_string(),
            host_arch: std::env::consts::ARCH.to_string(),
            cpu: observe_cpu_cost(),
            peak_resident_bytes: observe_peak_resident_bytes(),
        }
    }

    /// Serialize the receipt body: the text after [`RESOURCE_COST_RECEIPT_PREFIX`].
    fn to_receipt_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|error| error.to_string())
    }
}

/// Parse a receipt body back into an observation.
///
/// The producer never reads its own receipt, so this exists to pin the wire
/// contract: the same derive pair that writes the line must read it back
/// unchanged, which is what makes the round trip a real discriminator rather
/// than a self-confirming serialize check. The xtask consumer parses the same
/// wire shape with its own independent mirror type, so a drift on either side
/// of the contract is caught rather than silently accepted.
#[cfg(test)]
fn parse_resource_cost_receipt(body: &str) -> Result<ResourceCostObservation, String> {
    let observation: ResourceCostObservation = serde_json::from_str(body)
        .map_err(|error| format!("parse resource-cost receipt: {error}"))?;
    if observation.schema_version != RESOURCE_COST_SCHEMA_VERSION {
        return Err(format!(
            "unsupported resource-cost schema_version `{}`",
            observation.schema_version
        ));
    }
    Ok(observation)
}

/// Emit this run's resource cost on stderr when the trace family is enabled.
///
/// With [`LATENCY_TRACE_ENV`] unset this reads nothing and prints nothing, so
/// stdout JSON and default stderr stay byte-identical.
pub(crate) fn emit_run_resource_cost() {
    if !latency_trace_enabled() {
        return;
    }
    let observation = ResourceCostObservation::observe();
    match observation.to_receipt_json() {
        Ok(body) => eprintln!("{RESOURCE_COST_RECEIPT_PREFIX}{body}"),
        // A receipt that cannot serialize must still say so. Reporting every
        // field unavailable keeps the receipt parseable and keeps a lost
        // payload from reading as a completed observation.
        Err(_) => eprintln!(
            "{RESOURCE_COST_RECEIPT_PREFIX}{}",
            unserializable_receipt_json()
        ),
    }
}

/// A structurally valid receipt in which every number is unavailable because
/// the receipt itself could not be serialized.
fn unserializable_receipt_json() -> String {
    let unavailable = ResourceUnavailableReason::SourceUnreadable;
    let observation = ResourceCostObservation {
        schema_version: RESOURCE_COST_SCHEMA_VERSION.to_string(),
        observer: RESOURCE_COST_OBSERVER_SCOPE.to_string(),
        observer_pid: std::process::id(),
        host_os: std::env::consts::OS.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        cpu: CpuCost::Unavailable {
            reason: unavailable,
        },
        peak_resident_bytes: ResourceMeasurement::Unavailable {
            reason: unavailable,
        },
    };
    observation.to_receipt_json().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        BYTES_PER_KIBIBYTE, CpuCost, CpuSourceUnit, RESOURCE_COST_OBSERVER_SCOPE,
        RESOURCE_COST_SCHEMA_VERSION, ResourceCostObservation, ResourceMeasurement,
        ResourceUnavailableReason, cpu_source_unit_for, latency_trace_line, observe_cpu_cost,
        observe_peak_resident_bytes, parse_cpu_source_ticks, parse_peak_resident_source_bytes,
        parse_resource_cost_receipt, unavailable_reason_for,
    };
    use std::time::Duration;

    /// The dishonest shapes this wire contract must refuse: an unavailable field
    /// carrying a zero number beside its reason, and a bare number standing in for
    /// a measurement. Both parse as the wrong thing rather than failing loudly, so
    /// they are pinned here explicitly.
    #[test]
    fn receipt_parser_rejects_a_number_standing_in_for_an_observation() -> Result<(), String> {
        let dishonest = serde_json::json!({
            "schema_version": RESOURCE_COST_SCHEMA_VERSION,
            "observer": RESOURCE_COST_OBSERVER_SCOPE,
            "observer_pid": 4242,
            "host_os": "windows",
            "host_arch": "x86_64",
            "cpu": {"state": "unavailable", "reason": "machine_wide_or_unsafe_only", "user_ms": 0},
            "peak_resident_bytes": 0u64,
        })
        .to_string();
        // The real parser must reject the number-only peak field.
        let error = parse_resource_cost_receipt(&dishonest)
            .err()
            .unwrap_or_default();
        assert!(
            error.contains("parse resource-cost receipt"),
            "the producer parser must reject a bare number: {error}"
        );
        // And a receipt that does parse must never compare equal to the honest
        // unavailable observation.
        let honest = unavailable("windows").to_receipt_json()?;
        assert_ne!(honest, dishonest);
        Ok(())
    }

    fn observation(host_os: &str) -> ResourceCostObservation {
        ResourceCostObservation {
            schema_version: RESOURCE_COST_SCHEMA_VERSION.to_string(),
            observer: RESOURCE_COST_OBSERVER_SCOPE.to_string(),
            observer_pid: 4242,
            host_os: host_os.to_string(),
            host_arch: "x86_64".to_string(),
            cpu: CpuCost::from_source(CpuSourceUnit::LinuxUserHzClockTicks, 1234, 56),
            peak_resident_bytes: ResourceMeasurement::Observed {
                value: 2048 * BYTES_PER_KIBIBYTE,
            },
        }
    }

    fn unavailable(host_os: &str) -> ResourceCostObservation {
        let reason = unavailable_reason_for(host_os);
        ResourceCostObservation {
            schema_version: RESOURCE_COST_SCHEMA_VERSION.to_string(),
            observer: RESOURCE_COST_OBSERVER_SCOPE.to_string(),
            observer_pid: 4242,
            host_os: host_os.to_string(),
            host_arch: "x86_64".to_string(),
            cpu: CpuCost::Unavailable { reason },
            peak_resident_bytes: ResourceMeasurement::Unavailable { reason },
        }
    }

    #[test]
    fn latency_trace_line_keeps_the_existing_phase_spelling() {
        assert_eq!(
            latency_trace_line("cache_load", "hit", Duration::from_millis(7)),
            "ripr_repo_exposure_latency phase=cache_load status=hit duration_ms=7"
        );
        assert_eq!(
            latency_trace_line(
                "file_fact_cache",
                "start_files_42_production_7",
                Duration::ZERO
            ),
            "ripr_repo_exposure_latency phase=file_fact_cache \
             status=start_files_42_production_7 duration_ms=0"
        );
    }

    #[test]
    fn observed_and_unavailable_receipts_round_trip_and_stay_distinct() -> Result<(), String> {
        // The raw source values must survive the wire so the millisecond
        // fields stay auditable instead of being trusted.
        let observed = observation("linux");
        let CpuCost::Observed {
            source_unit,
            source_unit_per_second,
            user_source,
            system_source,
            user_ms,
            system_ms,
        } = observed.cpu
        else {
            return Err("observed CPU must serialize as observed".to_string());
        };
        assert_eq!(source_unit, CpuSourceUnit::LinuxUserHzClockTicks);
        assert_eq!(source_unit_per_second, 100);
        assert_eq!((user_source, system_source), (1234, 56));
        assert_eq!((user_ms, system_ms), (12340, 560));

        let observed_body = observed.to_receipt_json()?;
        assert_eq!(parse_resource_cost_receipt(&observed_body)?, observed);
        assert!(
            observed_body.contains(r#""state":"observed""#),
            "{observed_body}"
        );
        assert!(
            observed_body.contains(r#""observer":"ripr_process_self""#),
            "{observed_body}"
        );

        // An unavailable receipt round-trips into the unavailable state, is
        // not equal to a measurement, and carries no zero anywhere.
        let missing = unavailable("windows");
        let missing_body = missing.to_receipt_json()?;
        let round_tripped = parse_resource_cost_receipt(&missing_body)?;
        assert_eq!(round_tripped, missing);
        assert_ne!(round_tripped, observed);
        assert!(matches!(
            round_tripped.peak_resident_bytes,
            ResourceMeasurement::Unavailable { .. }
        ));
        // No numeric measurement field survives in an unavailable receipt:
        // `cpu` and `peak_resident_bytes` are objects naming a reason only.
        let missing_value: serde_json::Value =
            serde_json::from_str(&missing_body).map_err(|error| error.to_string())?;
        for field in ["cpu", "peak_resident_bytes"] {
            let entry = missing_value
                .get(field)
                .ok_or_else(|| format!("{field} must be present: {missing_body}"))?;
            let object = entry
                .as_object()
                .ok_or_else(|| format!("{field} must be an object: {missing_body}"))?;
            assert_eq!(
                object.get("state").and_then(serde_json::Value::as_str),
                Some("unavailable"),
                "{field}: {missing_body}"
            );
            assert!(!object.contains_key("value"), "{field}: {missing_body}");
        }
        assert!(!missing_body.contains(r#""value""#), "{missing_body}");
        assert!(
            missing_body.contains(r#""reason":"machine_wide_or_unsafe_only""#),
            "{missing_body}"
        );
        assert!(
            missing_body.contains(r#""state":"unavailable""#),
            "{missing_body}"
        );

        // A measured zero is still an observation and stays distinct.
        let zeroed = ResourceCostObservation {
            cpu: CpuCost::from_source(CpuSourceUnit::LinuxUserHzClockTicks, 0, 0),
            peak_resident_bytes: ResourceMeasurement::Observed { value: 0 },
            ..observation("linux")
        };
        let zeroed_body = zeroed.to_receipt_json()?;
        assert_eq!(parse_resource_cost_receipt(&zeroed_body)?, zeroed);
        assert_ne!(zeroed, unavailable("linux"));
        assert_ne!(
            parse_resource_cost_receipt(&zeroed_body)?,
            parse_resource_cost_receipt(&missing_body)?
        );
        // The unavailable body has no `"value"` key at all; the zero body does.
        assert!(!missing_body.contains(r#""value""#), "{missing_body}");
        assert!(zeroed_body.contains(r#""value":0"#), "{zeroed_body}");
        Ok(())
    }

    #[test]
    fn receipt_rejects_an_unknown_schema_version() {
        let body = observation("linux")
            .to_receipt_json()
            .unwrap_or_default()
            .replace(RESOURCE_COST_SCHEMA_VERSION, "9.9");
        let error = parse_resource_cost_receipt(&body).err().unwrap_or_default();
        assert!(
            error.contains("unsupported resource-cost schema_version"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn cpu_milliseconds_follow_the_named_source_divisor() -> Result<(), String> {
        let windows =
            CpuCost::from_source(CpuSourceUnit::WindowsHundredNanoseconds, 10_000_000, 5_000);
        let CpuCost::Observed {
            source_unit_per_second,
            user_ms,
            system_ms,
            ..
        } = windows
        else {
            return Err("expected an observed CPU cost".to_string());
        };
        assert_eq!(source_unit_per_second, 10_000_000);
        // 10 000_000 hundred-nanosecond intervals is one second, so 1 000 ms.
        // 5 000 is half a millisecond, which truncation reports as 0 rather
        // than rounding the cost up past what was used.
        assert_eq!((user_ms, system_ms), (1_000, 0));
        Ok(())
    }

    /// A `/proc/self/stat` line with the given `comm` and the given
    /// `utime`/`stime` fields, spelled with the real field count so a change
    /// to the offset arithmetic shows up here rather than on a live host.
    fn stat_line(comm: &str, utime: &str, stime: &str) -> String {
        // Fields 3..13 after `comm`: state plus the ten fields before utime.
        format!("4242 ({comm}) S 1 1 1 0 0 0 0 0 0 0 {utime} {stime} 0 0")
    }

    #[test]
    fn cpu_source_parses_ticks_past_a_spacey_comm_field() -> Result<(), String> {
        // Field 2 is `comm`; a process named `my (weird) proc` proves the
        // count is taken from the last `)` rather than from whitespace.
        assert_eq!(
            parse_cpu_source_ticks(&stat_line("my (weird) proc", "4200", "137")),
            Ok((4200, 137))
        );
        assert_eq!(
            parse_cpu_source_ticks("no parens here"),
            Err(ResourceUnavailableReason::SourceValueMalformed)
        );
        assert_eq!(
            parse_cpu_source_ticks("1 (short) S 1 2"),
            Err(ResourceUnavailableReason::SourceFieldMissing)
        );
        assert_eq!(
            parse_cpu_source_ticks(&stat_line("x", "notanumber", "137")),
            Err(ResourceUnavailableReason::SourceValueMalformed)
        );
        // A negative tick count is not a clock tick count.
        assert_eq!(
            parse_cpu_source_ticks(&stat_line("x", "-1", "137")),
            Err(ResourceUnavailableReason::SourceValueMalformed)
        );
        Ok(())
    }

    #[test]
    fn peak_resident_parses_kibibytes() -> Result<(), String> {
        let status =
            "Name:\tripr\nVmPeak:\t  204800 kB\nVmHWM:\t   40960 kB\nVmRSS:\t   10240 kB\n";
        assert_eq!(
            parse_peak_resident_source_bytes(status),
            Ok(40960 * BYTES_PER_KIBIBYTE)
        );
        // `VmRSS` is the current set, not the peak; reading it here would
        // understate the cost, so only `VmHWM` counts.
        assert_eq!(
            parse_peak_resident_source_bytes("Name:\tripr\nVmRSS:\t1 kB\n"),
            Err(ResourceUnavailableReason::SourceFieldMissing)
        );
        assert_eq!(
            parse_peak_resident_source_bytes("VmHWM:\n"),
            Err(ResourceUnavailableReason::SourceValueMalformed)
        );
        assert_eq!(
            parse_peak_resident_source_bytes("VmHWM:\t40960 mB\n"),
            Err(ResourceUnavailableReason::SourceValueMalformed)
        );
        // A value that cannot be represented in bytes must not wrap to zero.
        assert_eq!(
            parse_peak_resident_source_bytes(&format!("VmHWM:\t{}\tkB\n", u64::MAX)),
            Err(ResourceUnavailableReason::SourceValueMalformed)
        );
        Ok(())
    }

    #[test]
    fn every_host_names_its_unavailable_reason_and_unit() {
        assert_eq!(
            unavailable_reason_for("windows"),
            ResourceUnavailableReason::MachineWideOrUnsafeOnly
        );
        for os in ["linux", "macos", "freebsd"] {
            assert_eq!(
                unavailable_reason_for(os),
                ResourceUnavailableReason::PlatformNotSupported,
                "{os} must name platform_not_supported"
            );
        }
        assert_eq!(
            cpu_source_unit_for("linux"),
            Some(CpuSourceUnit::LinuxUserHzClockTicks)
        );
        assert_eq!(
            cpu_source_unit_for("windows"),
            Some(CpuSourceUnit::WindowsHundredNanoseconds)
        );
        assert_eq!(cpu_source_unit_for("macos"), None);
    }

    /// The live observation must agree with the platform it ran on: a host
    /// that claims an observation must really produce one, and a host with no
    /// safe source must name the reason rather than print a zero.
    #[test]
    fn live_observation_matches_this_host() {
        let observation = ResourceCostObservation::observe();
        assert_eq!(observation.schema_version, RESOURCE_COST_SCHEMA_VERSION);
        assert_eq!(observation.observer, RESOURCE_COST_OBSERVER_SCOPE);
        assert_eq!(observation.observer_pid, std::process::id());
        assert_eq!(observation.host_os, std::env::consts::OS);

        if std::env::consts::OS == "linux" {
            assert!(
                matches!(observation.cpu, CpuCost::Observed { .. }),
                "linux must observe this process's CPU time: {:?}",
                observation.cpu
            );
            assert!(
                matches!(
                    observation.peak_resident_bytes,
                    ResourceMeasurement::Observed { .. }
                ),
                "linux must observe this process's peak resident set: {:?}",
                observation.peak_resident_bytes
            );
            return;
        }
        let expected = unavailable_reason_for(std::env::consts::OS);
        assert_eq!(observation.cpu, CpuCost::Unavailable { reason: expected });
        assert_eq!(
            observation.peak_resident_bytes,
            ResourceMeasurement::Unavailable { reason: expected },
            "{} has no safe per-process source and must say so",
            std::env::consts::OS
        );
    }

    #[test]
    fn observation_entry_points_are_callable_without_the_trace_switch() {
        // The observation is reachable without the trace switch so a test can
        // prove the capability without printing anything; the subprocess proof
        // that tracing off prints nothing lives in
        // `tests/resource_cost_trace.rs`, where the environment is isolated.
        let _ = observe_cpu_cost();
        let _ = observe_peak_resident_bytes();
    }
}
