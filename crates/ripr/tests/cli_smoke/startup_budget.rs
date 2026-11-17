//! Startup cost guard (#12906, cheap deterministic controls).
//!
//! Output size already has per-change byte budgets (`agent_output_budget`,
//! `findings_byte_budget`) and run time has the nightly size-scaling and
//! warm-ab lanes. Nothing watched what `ripr` costs before it does any work: a
//! static initializer, an eager table build or a workspace read added ahead of
//! command dispatch taxes every invocation and passes every functional test.
//!
//! The commands below do no analysis, so their cost is the cost of starting.
//! The numbers come from the process's own resource-cost receipt (#5213): CPU
//! time and peak resident memory. Neither grows when the runner is busy, which
//! wall time does, so the ceilings can sit close enough to catch a regression
//! without flaking. Wall time is checked only as the best of several runs
//! against a ceiling hundreds of times the measured value.
//!
//! Measured 2026-11-11 on Linux x86_64, dev profile, 100 runs per command, run
//! from this repository's root: `--version` and `--help` peak at 23.0 to
//! 25.3 MiB, `check --help` and `capabilities` at 31.4 to 36.4 MiB; CPU time
//! was below one 10 ms clock tick in every run; wall time was 5 to 14 ms.
//!
//! A deliberate startup cost raises a ceiling in the same change and says why.
//! To re-measure, run this module with `--nocapture`; every command prints its
//! `startup-budget` line.
use super::{run_command_with_env, unique_external_workspace, workspace_root};
use std::fs;
use std::time::{Duration, Instant};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const TRACE_ENV: &str = "RIPR_REPO_EXPOSURE_LATENCY_TRACE";
const RECEIPT_PREFIX: &str = "ripr_resource_cost_receipt ";
/// An inherited harness token would put `token=<value>` ahead of the receipt
/// body. An empty value is not a valid token, so the child keeps the plain
/// receipt shape whatever the parent environment carries.
const NO_RECEIPT_TOKEN: (&str, &str) = ("RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN", "");
const MIB: u64 = 1024 * 1024;

/// Runs per command. Peak memory is judged on the worst run, CPU time on the
/// sum (a single short run usually lands between clock ticks), wall time on
/// the best run.
const SAMPLES: usize = 5;

/// CPU time allowed across all [`SAMPLES`] runs of one command. Measured: 0.
const CPU_MS_CEILING: u64 = 100;

/// A peak below this is not a plausible process image; it means the receipt
/// is broken, and a broken instrument must not pass as a small number.
const PEAK_FLOOR_BYTES: u64 = MIB;

/// Best-of-[`SAMPLES`] wall time. Measured: 5 ms. This catches only a gross
/// regression (a child process, a network wait, a repository walk); the CPU
/// and memory ceilings carry the finer signal.
const WALL_BEST_CEILING: Duration = Duration::from_secs(2);

/// One no-analysis command and its peak resident memory ceiling in MiB, about
/// 1.75 to 1.9 times the largest measured peak.
const BUDGETS: &[(&[&str], u64)] = &[
    (&["--version"], 48),
    (&["--help"], 48),
    (&["check", "--help"], 64),
    (&["capabilities"], 64),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cost {
    cpu_ms: u64,
    peak_bytes: u64,
}

/// The cost stated by the single receipt line in `stderr`. Fails when the line
/// is missing or repeated, or when either number is reported unavailable: a
/// measurement that was not taken is never read as zero.
fn cost_from_stderr(stderr: &str) -> Result<Cost, String> {
    let mut receipts = stderr
        .lines()
        .filter_map(|line| line.strip_prefix(RECEIPT_PREFIX));
    let body = receipts
        .next()
        .ok_or_else(|| format!("no resource-cost receipt on stderr:\n{stderr}"))?;
    if receipts.next().is_some() {
        return Err(format!("more than one resource-cost receipt:\n{stderr}"));
    }
    let receipt: serde_json::Value =
        serde_json::from_str(body).map_err(|error| format!("receipt is not JSON: {error}"))?;
    let observed = |field: &str| -> Result<&serde_json::Value, String> {
        let value = &receipt[field];
        if value["state"] == "observed" {
            Ok(value)
        } else {
            Err(format!("receipt `{field}` was not observed: {body}"))
        }
    };
    let number = |value: &serde_json::Value, key: &str| -> Result<u64, String> {
        value[key]
            .as_u64()
            .ok_or_else(|| format!("receipt has no unsigned `{key}`: {body}"))
    };
    let cpu = observed("cpu")?;
    let peak = observed("peak_resident_bytes")?;
    Ok(Cost {
        cpu_ms: number(cpu, "user_ms")?
            .checked_add(number(cpu, "system_ms")?)
            .ok_or("CPU time overflows")?,
        peak_bytes: number(peak, "value")?,
    })
}

/// The verdict for one command's samples. The only place a ceiling is applied.
fn judge(label: &str, samples: &[Cost], peak_ceiling_mib: u64) -> Result<(), String> {
    let worst_peak = samples
        .iter()
        .map(|cost| cost.peak_bytes)
        .max()
        .ok_or_else(|| format!("{label}: no startup measurement was taken"))?;
    let least_peak = samples.iter().map(|cost| cost.peak_bytes).min();
    if least_peak.is_some_and(|peak| peak < PEAK_FLOOR_BYTES) {
        return Err(format!(
            "{label}: a peak of {least_peak:?} bytes is below the {PEAK_FLOOR_BYTES}-byte floor; \
             the resource-cost receipt is not measuring this process"
        ));
    }
    if worst_peak > peak_ceiling_mib * MIB {
        return Err(format!(
            "{label}: startup peak resident memory grew to {worst_peak} bytes, over its \
             {peak_ceiling_mib} MiB budget"
        ));
    }
    let cpu_ms: u64 = samples.iter().map(|cost| cost.cpu_ms).sum();
    if cpu_ms > CPU_MS_CEILING {
        return Err(format!(
            "{label}: startup CPU time grew to {cpu_ms} ms across {} runs, over its \
             {CPU_MS_CEILING} ms budget",
            samples.len()
        ));
    }
    Ok(())
}

// The receipt is observed on Linux and Windows only, and the ceilings were
// measured on Linux; another host needs its own measurement before it is
// judged against them.
#[cfg(target_os = "linux")]
#[test]
fn startup_commands_stay_inside_their_cpu_and_memory_budgets() -> TestResult {
    // An instrumented coverage build maps its counters at load, which is not
    // the cost a user pays. Say so instead of judging it.
    if std::env::var_os("LLVM_PROFILE_FILE").is_some() {
        eprintln!("startup-budget: not judged, this is a coverage-instrumented run");
        return Ok(());
    }
    // This repository is the working directory on purpose: a startup path
    // that begins to read the workspace then pays for a large real one.
    let root = workspace_root();
    for (args, peak_ceiling_mib) in BUDGETS {
        let label = args.join(" ");
        let mut samples = Vec::with_capacity(SAMPLES);
        let mut best_wall = Duration::MAX;
        for _ in 0..SAMPLES {
            let started = Instant::now();
            let output = run_command_with_env(
                env!("CARGO_BIN_EXE_ripr"),
                &root,
                args,
                &[(TRACE_ENV, "1"), NO_RECEIPT_TOKEN],
            )?;
            best_wall = best_wall.min(started.elapsed());
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "ripr {label}: {stderr}");
            samples.push(cost_from_stderr(&stderr)?);
        }
        let worst_peak = samples.iter().map(|cost| cost.peak_bytes).max();
        let cpu_ms: u64 = samples.iter().map(|cost| cost.cpu_ms).sum();
        eprintln!(
            "startup-budget {label}: peak {worst_peak:?} bytes (ceiling {peak_ceiling_mib} MiB), \
             cpu {cpu_ms} ms over {SAMPLES} runs (ceiling {CPU_MS_CEILING} ms), \
             best wall {best_wall:?} (ceiling {WALL_BEST_CEILING:?})"
        );
        judge(&label, &samples, *peak_ceiling_mib)?;
        assert!(
            best_wall <= WALL_BEST_CEILING,
            "ripr {label}: the fastest of {SAMPLES} starts took {best_wall:?}, over \
             {WALL_BEST_CEILING:?}"
        );
    }
    Ok(())
}

fn receipt(cpu: &str, peak: &str) -> String {
    format!("noise\n{RECEIPT_PREFIX}{{\"cpu\":{cpu},\"peak_resident_bytes\":{peak}}}\n")
}

const OBSERVED_CPU: &str = r#"{"state":"observed","user_ms":10,"system_ms":20}"#;
const OBSERVED_PEAK: &str = r#"{"state":"observed","value":26214400}"#;
const UNAVAILABLE: &str = r#"{"state":"unavailable","reason":"platform_not_supported"}"#;

#[test]
fn startup_receipt_reader_refuses_missing_repeated_and_unavailable_measurements() -> TestResult {
    let read = cost_from_stderr(&receipt(OBSERVED_CPU, OBSERVED_PEAK))?;
    assert_eq!(
        read,
        Cost {
            cpu_ms: 30,
            peak_bytes: 26_214_400
        }
    );

    let refusals = [
        ("no receipt", "ripr 0.0.0\n".to_string(), "no resource-cost"),
        (
            "two receipts",
            receipt(OBSERVED_CPU, OBSERVED_PEAK).repeat(2),
            "more than one",
        ),
        (
            "unavailable cpu",
            receipt(UNAVAILABLE, OBSERVED_PEAK),
            "`cpu` was not observed",
        ),
        (
            "unavailable peak",
            receipt(OBSERVED_CPU, UNAVAILABLE),
            "`peak_resident_bytes` was not observed",
        ),
        (
            "peak without a value",
            receipt(OBSERVED_CPU, r#"{"state":"observed"}"#),
            "no unsigned `value`",
        ),
        (
            "not JSON",
            format!("{RECEIPT_PREFIX}{{\n"),
            "receipt is not JSON",
        ),
    ];
    for (case, stderr, expected) in refusals {
        match cost_from_stderr(&stderr) {
            Ok(cost) => return Err(format!("{case}: read as {cost:?}").into()),
            Err(reason) => assert!(reason.contains(expected), "{case}: {reason}"),
        }
    }
    Ok(())
}

#[test]
fn startup_judge_fails_each_exceeded_budget_and_an_absent_measurement() -> TestResult {
    let cost = |cpu_ms, peak_mib: u64| Cost {
        cpu_ms,
        peak_bytes: peak_mib * MIB,
    };
    let verdict = |samples: &[Cost]| judge("subject", samples, 48);

    assert_eq!(verdict(&[cost(0, 25), cost(10, 26), cost(0, 24)]), Ok(()));
    // Exactly at each ceiling passes; one step over fails.
    assert_eq!(verdict(&[cost(CPU_MS_CEILING, 48)]), Ok(()));

    let failures: [(&[Cost], &str); 5] = [
        (&[], "no startup measurement"),
        (&[cost(0, 25), cost(0, 49)], "peak resident memory grew"),
        (
            &[Cost {
                cpu_ms: 0,
                peak_bytes: 48 * MIB + 1,
            }],
            "peak resident memory grew",
        ),
        // No single run is over, the sum is: a cost between clock ticks adds up.
        (
            &[
                cost(CPU_MS_CEILING / 2, 25),
                cost(CPU_MS_CEILING / 2 + 10, 25),
            ],
            "CPU time grew",
        ),
        (
            &[
                cost(0, 25),
                Cost {
                    cpu_ms: 0,
                    peak_bytes: PEAK_FLOOR_BYTES - 1,
                },
            ],
            "not measuring this process",
        ),
    ];
    for (samples, expected) in failures {
        match verdict(samples) {
            Ok(()) => return Err(format!("{samples:?} passed, expected `{expected}`").into()),
            Err(reason) => assert!(reason.contains(expected), "{samples:?}: {reason}"),
        }
    }
    Ok(())
}

/// Starting must not depend on the directory it starts in. An unparsable
/// `ripr.toml` and manifest make a workspace read ahead of dispatch visible as
/// a failure or as changed output, on every platform and with no timing. A
/// read whose result and errors are both discarded changes neither and is not
/// seen here; only the Linux cost ceilings above can notice that one.
#[test]
fn startup_commands_do_not_read_the_working_directory() -> TestResult {
    let scratch = unique_external_workspace("startup-budget-cwd")?;
    let _cleanup = super::DeclaredHunkScratch(scratch.clone());
    let empty = scratch.join("empty");
    let hostile = scratch.join("hostile");
    fs::create_dir_all(&empty)?;
    fs::create_dir_all(hostile.join("src"))?;
    fs::write(hostile.join("ripr.toml"), "[languages\nenabled = = \n")?;
    fs::write(hostile.join("Cargo.toml"), "[package\nname = \n")?;
    fs::write(hostile.join("src/lib.rs"), "pub fn (((\n")?;

    // A parent that already traces makes every child print a receipt carrying
    // its own pid; that line is the instrument, not the command's output.
    let without_receipts = |stderr: &[u8]| -> String {
        String::from_utf8_lossy(stderr)
            .lines()
            .filter(|line| !line.starts_with(RECEIPT_PREFIX))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (args, _) in BUDGETS {
        let label = args.join(" ");
        let run = |cwd| run_command_with_env(env!("CARGO_BIN_EXE_ripr"), cwd, args, &[]);
        let clean = run(&empty)?;
        let exposed = run(&hostile)?;
        assert!(
            clean.status.success() && exposed.status.success(),
            "ripr {label}: clean {:?}, hostile directory {:?}: {}",
            clean.status,
            exposed.status,
            String::from_utf8_lossy(&exposed.stderr)
        );
        assert!(!clean.stdout.is_empty(), "ripr {label} printed nothing");
        assert_eq!(
            String::from_utf8_lossy(&exposed.stdout),
            String::from_utf8_lossy(&clean.stdout),
            "ripr {label}: stdout depends on the working directory"
        );
        assert_eq!(
            without_receipts(&exposed.stderr),
            without_receipts(&clean.stderr),
            "ripr {label}: stderr depends on the working directory"
        );
    }
    Ok(())
}
