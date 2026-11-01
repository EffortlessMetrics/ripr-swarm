//! `cargo xtask warm-ab --baseline <bin> --candidate <bin> --tree <dir> --diff <file>`
//! (#10639).
//!
//! Times a warm `ripr check` of one pinned diff of this repository with two
//! binaries on the same runner in the same job, so runner class and load
//! cancel, and fails when the candidate costs more than the allowed multiple
//! of the baseline. #10489 doubled the warm check of this repository's own
//! diff and nothing failed: the size-scaling cells are far smaller than
//! `xtask/src/tests.rs`, and a stored number is not comparable across runner
//! classes.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::run as shared_run;

mod judge;
mod nightly;

#[cfg(test)]
mod tests;

use judge::{Limits, Verdict, evidence_failure, judge};

const DEFAULT_OUT: &str = "target/ripr/reports";
const DEFAULT_WORK: &str = "target/ripr/warm-ab";
const SCHEMA: &str = "ripr-warm-ab-v1";
const OUTPUT_BYTES: usize = 64 * 1024 * 1024;
/// A single check that reaches this is stopped and recorded as a failure.
const DEADLINE: Duration = Duration::from_mins(10);

pub(crate) struct Options {
    baseline: PathBuf,
    candidate: PathBuf,
    tree: PathBuf,
    diff: PathBuf,
    pin: String,
    runs: usize,
    out: PathBuf,
    work: PathBuf,
    limits: Limits,
    /// Present for the nightly orchestration: builds and pins are made here.
    nightly: Option<nightly::Plan>,
}

#[derive(Serialize)]
struct Side {
    binary_sha256: String,
    cold_ms: u128,
    warm_ms: Vec<u128>,
    findings: usize,
    findings_sha256: String,
}

#[derive(Serialize)]
struct Packet {
    schema: &'static str,
    pin: String,
    host: String,
    runs: usize,
    max_ratio: f64,
    noise_floor_ms: u128,
    ceiling_secs: u64,
    baseline: Side,
    candidate: Side,
    baseline_median_ms: Option<u128>,
    candidate_median_ms: Option<u128>,
    ratio: Option<f64>,
    findings_identical: bool,
    verdict: String,
    detail: String,
    passed: bool,
    non_claims: Vec<&'static str>,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    // Fail fast on the wrong host before the nightly path builds two
    // release binaries; `build_packet` keeps the same check for direct use.
    if !cfg!(unix) {
        return Err("warm-ab runs on a Unix host only".to_string());
    }
    let options = parse_args(args)?;
    match &options.nightly {
        Some(plan) => nightly::run(&options, plan),
        None => report(&options),
    }
}

fn report(options: &Options) -> Result<(), String> {
    let packet = build_packet(options)?;
    fs::create_dir_all(&options.out)
        .map_err(|error| format!("failed to create {}: {error}", options.out.display()))?;
    let json_path = options.out.join(format!("warm-ab-{}.json", options.pin));
    let md_path = options.out.join(format!("warm-ab-{}.md", options.pin));
    let json = serde_json::to_string_pretty(&packet)
        .map_err(|error| format!("failed to render the warm-ab report: {error}"))?;
    fs::write(&json_path, json + "\n")
        .map_err(|error| format!("failed to write {}: {error}", json_path.display()))?;
    fs::write(&md_path, render_markdown(&packet))
        .map_err(|error| format!("failed to write {}: {error}", md_path.display()))?;
    println!(
        "warm-ab {}: {} ({}); wrote {} and {}",
        packet.pin,
        packet.verdict,
        packet.detail,
        json_path.display(),
        md_path.display()
    );
    if packet.passed {
        Ok(())
    } else {
        Err(format!("warm-ab {} failed: {}", packet.pin, packet.detail))
    }
}

pub(crate) fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut baseline = None;
    let mut candidate = None;
    let mut tree = None;
    let mut diff = None;
    let mut options = Options {
        baseline: PathBuf::new(),
        candidate: PathBuf::new(),
        tree: PathBuf::new(),
        diff: PathBuf::new(),
        pin: "pin".to_string(),
        runs: 3,
        out: PathBuf::from(DEFAULT_OUT),
        work: PathBuf::from(DEFAULT_WORK),
        limits: Limits::default(),
        nightly: None,
    };
    let mut baseline_ref = None;
    let mut candidate_ref = None;
    let mut pins = None;
    let mut external_pins = None;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match flag.as_str() {
            "--baseline" => baseline = Some(PathBuf::from(value("--baseline")?)),
            "--candidate" => candidate = Some(PathBuf::from(value("--candidate")?)),
            "--tree" => tree = Some(PathBuf::from(value("--tree")?)),
            "--diff" => diff = Some(PathBuf::from(value("--diff")?)),
            "--pin" => options.pin = value("--pin")?,
            "--baseline-ref" => baseline_ref = Some(value("--baseline-ref")?),
            "--candidate-ref" => candidate_ref = Some(value("--candidate-ref")?),
            "--pins" => pins = Some(value("--pins")?),
            "--external-pins" => external_pins = Some(value("--external-pins")?),
            "--out" => options.out = PathBuf::from(value("--out")?),
            "--work" => options.work = PathBuf::from(value("--work")?),
            "--runs" => {
                options.runs = value("--runs")?
                    .parse()
                    .map_err(|_error| "--runs must be a whole number".to_string())?;
            }
            "--max-ratio" => {
                options.limits.max_ratio = value("--max-ratio")?
                    .parse()
                    .map_err(|_error| "--max-ratio must be a number".to_string())?;
            }
            "--ceiling-secs" => {
                let secs: u64 = value("--ceiling-secs")?
                    .parse()
                    .map_err(|_error| "--ceiling-secs must be a whole number".to_string())?;
                options.limits.ceiling_ms = u128::from(secs) * 1000;
            }
            other => return Err(format!("unknown warm-ab argument `{other}`")),
        }
    }
    if pins.is_some() || external_pins.is_some() {
        if baseline.is_some() || candidate.is_some() || tree.is_some() || diff.is_some() {
            return Err(
                "--pins builds its own binaries and trees; do not combine it with --baseline, --candidate, --tree or --diff"
                    .to_string(),
            );
        }
        options.nightly = Some(nightly::Plan::new(
            pins.as_deref(),
            external_pins.as_deref(),
            baseline_ref,
            candidate_ref,
        )?);
    } else {
        if baseline_ref.is_some() || candidate_ref.is_some() {
            return Err("--baseline-ref and --candidate-ref need --pins".to_string());
        }
        options.baseline = baseline.ok_or("warm-ab requires --baseline <binary>")?;
        options.candidate = candidate.ok_or("warm-ab requires --candidate <binary>")?;
        options.tree = tree.ok_or("warm-ab requires --tree <checkout>")?;
        options.diff = diff.ok_or("warm-ab requires --diff <file>")?;
    }
    if options.runs == 0 {
        return Err("--runs must be at least 1".to_string());
    }
    if !options.limits.max_ratio.is_finite() || options.limits.max_ratio < 1.0 {
        return Err("--max-ratio must be a finite number of at least 1.0".to_string());
    }
    let id_ok = !options.pin.is_empty()
        && options
            .pin
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !id_ok {
        return Err("--pin may hold only letters, digits, `-` and `_`".to_string());
    }
    Ok(options)
}

fn build_packet(options: &Options) -> Result<Packet, String> {
    if !cfg!(unix) {
        return Err("warm-ab runs on a Unix host only".to_string());
    }
    let resolve = |path: &Path| {
        fs::canonicalize(path)
            .map_err(|error| format!("failed to resolve {}: {error}", path.display()))
    };
    let tree = resolve(&options.tree)?;
    let diff = resolve(&options.diff)?;
    fs::create_dir_all(&options.work)
        .map_err(|error| format!("failed to create {}: {error}", options.work.display()))?;
    let run_root = resolve(&options.work)?.join(format!("{}-{}", options.pin, std::process::id()));
    if run_root.exists() {
        return Err(format!(
            "owned work root {} already exists; refusing to reuse it",
            run_root.display()
        ));
    }
    fs::create_dir_all(&run_root)
        .map_err(|error| format!("failed to create {}: {error}", run_root.display()))?;
    let result = measure_pair(options, &tree, &diff, &run_root);
    let _ = fs::remove_dir_all(&run_root);
    result
}

struct Prepared {
    binary: PathBuf,
    sha256: String,
    cache: PathBuf,
}

/// Measure private copies so a rebuild during the run cannot change the bytes
/// under test, and give each side its own cache so neither warms the other.
fn prepare(source: &Path, name: &str, run_root: &Path) -> Result<Prepared, String> {
    let source = fs::canonicalize(source)
        .map_err(|error| format!("failed to resolve {}: {error}", source.display()))?;
    let binary = run_root.join(format!("ripr-{name}"));
    fs::copy(&source, &binary)
        .map_err(|error| format!("failed to copy {}: {error}", source.display()))?;
    let bytes = fs::read(&binary)
        .map_err(|error| format!("failed to read {}: {error}", binary.display()))?;
    let cache = run_root.join(format!("cache-{name}"));
    fs::create_dir_all(&cache)
        .map_err(|error| format!("failed to create {}: {error}", cache.display()))?;
    Ok(Prepared {
        binary,
        sha256: crate::blind_journey::sha256_hex(&bytes),
        cache,
    })
}

/// A report without a `findings` array is malformed evidence, not a
/// zero-finding result: reject it rather than counting it as zero.
fn findings_count(report: &serde_json::Value, label: &str) -> Result<usize, String> {
    report
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .ok_or_else(|| format!("{label} report has no findings array"))
}

struct Timed {
    millis: u128,
    findings: usize,
    stdout_sha256: String,
}

fn check_once(
    side: &Prepared,
    tree: &Path,
    diff: &Path,
    home: &Path,
    label: &str,
) -> Result<Timed, String> {
    let home_text = home.to_string_lossy().to_string();
    let cache = side.cache.to_string_lossy().to_string();
    let envs: Vec<(&str, &str)> = vec![
        ("HOME", &home_text),
        ("TMPDIR", &home_text),
        ("RIPR_CACHE_DIR", &cache),
        ("NO_COLOR", "1"),
        ("RUST_BACKTRACE", "0"),
    ];
    // Ambient `RIPR_*` configuration must not change a measurement.
    let removed: Vec<String> = std::env::vars()
        .map(|(name, _)| name)
        .filter(|name| name.starts_with("RIPR_") && name != "RIPR_CACHE_DIR")
        .collect();
    let removed_refs: Vec<&str> = removed.iter().map(String::as_str).collect();
    let args: Vec<String> = [
        "check",
        "--root",
        ".",
        "--diff",
        &diff.to_string_lossy(),
        "--format",
        "json",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    let timed = shared_run::capture_bytes_in_dir_with_env_budget(
        &side.binary,
        &args,
        tree,
        (&envs, &removed_refs),
        shared_run::ByteCaptureBudget {
            timeout: DEADLINE,
            stdout_bytes: OUTPUT_BYTES,
            stderr_bytes: OUTPUT_BYTES,
        },
        label,
    )?;
    if timed.timed_out {
        return Err(format!(
            "{label} reached the {} s deadline",
            DEADLINE.as_secs()
        ));
    }
    match timed.status.and_then(|status| status.code()) {
        Some(0 | 1) => {}
        Some(code) => return Err(format!("{label} exited {code}")),
        None => return Err(format!("{label} ended on a signal")),
    }
    let report: serde_json::Value = serde_json::from_slice(&timed.stdout)
        .map_err(|error| format!("{label} stdout is not a JSON report: {error}"))?;
    let findings = findings_count(&report, label)?;
    Ok(Timed {
        millis: timed.duration.as_millis(),
        findings,
        stdout_sha256: crate::blind_journey::sha256_hex(&timed.stdout),
    })
}

fn measure_pair(
    options: &Options,
    tree: &Path,
    diff: &Path,
    run_root: &Path,
) -> Result<Packet, String> {
    let home = run_root.join("home");
    fs::create_dir_all(&home)
        .map_err(|error| format!("failed to create {}: {error}", home.display()))?;
    let baseline = prepare(&options.baseline, "baseline", run_root)?;
    let candidate = prepare(&options.candidate, "candidate", run_root)?;
    // A cold run fills each side's cache; it is recorded, not judged.
    let base_cold = check_once(&baseline, tree, diff, &home, "baseline cold check")?;
    let cand_cold = check_once(&candidate, tree, diff, &home, "candidate cold check")?;
    let mut base_warm = Vec::new();
    let mut cand_warm = Vec::new();
    let mut base_counts = vec![base_cold.findings];
    let mut cand_counts = vec![cand_cold.findings];
    let mut base_last = None;
    let mut cand_last = None;
    // Alternate sides so a drifting runner touches both alike.
    for run in 0..options.runs {
        let base = check_once(&baseline, tree, diff, &home, "baseline warm check")?;
        let cand = check_once(&candidate, tree, diff, &home, "candidate warm check")?;
        eprintln!(
            "warm-ab {}: run {}: baseline {} ms, candidate {} ms",
            options.pin,
            run + 1,
            base.millis,
            cand.millis
        );
        base_warm.push(base.millis);
        cand_warm.push(cand.millis);
        base_counts.push(base.findings);
        cand_counts.push(cand.findings);
        base_last = Some(base);
        cand_last = Some(cand);
    }
    let (Some(base_last), Some(cand_last)) = (base_last, cand_last) else {
        return Err("no warm run was taken".to_string());
    };
    let judged = judge(&base_warm, &cand_warm, &options.limits);
    // Positive evidence is required independently on each side in every
    // measured check; a one-sided empty report measured nothing.
    let (verdict, detail) = match evidence_failure(&base_counts, &cand_counts) {
        Some(failure) => (Verdict::Fail, failure),
        None => (judged.verdict, judged.detail),
    };
    let identical = base_last.stdout_sha256 == cand_last.stdout_sha256;
    Ok(Packet {
        schema: SCHEMA,
        pin: options.pin.clone(),
        host: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        runs: options.runs,
        max_ratio: options.limits.max_ratio,
        noise_floor_ms: options.limits.noise_floor_ms,
        ceiling_secs: u64::try_from(options.limits.ceiling_ms / 1000).unwrap_or(u64::MAX),
        baseline: Side {
            binary_sha256: baseline.sha256,
            cold_ms: base_cold.millis,
            warm_ms: base_warm,
            findings: base_last.findings,
            findings_sha256: base_last.stdout_sha256,
        },
        candidate: Side {
            binary_sha256: candidate.sha256,
            cold_ms: cand_cold.millis,
            warm_ms: cand_warm,
            findings: cand_last.findings,
            findings_sha256: cand_last.stdout_sha256,
        },
        baseline_median_ms: judged.baseline_ms,
        candidate_median_ms: judged.candidate_ms,
        ratio: judged.ratio,
        findings_identical: identical,
        verdict: verdict.as_str().to_string(),
        detail,
        passed: verdict == Verdict::Pass,
        non_claims: vec![
            "Wall time only; peak memory is not measured.",
            "Medians of a few interleaved runs on one runner; a regression under the ratio limit or the noise floor passes.",
            "One pinned diff per report. A cost that only shows on other shapes of change is not seen.",
            "findings_identical compares whole report bytes. A difference is reported, not failed: baseline and candidate are different builds and may legitimately differ.",
            "Does not change product behavior or qualify a release.",
        ],
    })
}

fn render_markdown(packet: &Packet) -> String {
    let times = |side: &Side| {
        side.warm_ms
            .iter()
            .map(u128::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = format!(
        "# Warm-check A/B\n\nSchema `{}`; pin `{}`; {}; {} warm runs per side; at most {:.2}x (noise floor {} ms, ceiling {} s).\n\nResult: **{}**: {}\n\n| Side | Binary sha256 | Cold ms | Warm ms | Findings |\n|---|---|---|---|---|\n",
        packet.schema,
        packet.pin,
        packet.host,
        packet.runs,
        packet.max_ratio,
        packet.noise_floor_ms,
        packet.ceiling_secs,
        packet.verdict,
        packet.detail,
    );
    for (name, side) in [
        ("baseline", &packet.baseline),
        ("candidate", &packet.candidate),
    ] {
        text.push_str(&format!(
            "| {name} | `{}` | {} | {} | {} |\n",
            side.binary_sha256,
            side.cold_ms,
            times(side),
            side.findings
        ));
    }
    text.push_str(&format!(
        "\nFindings report bytes identical across sides: **{}**.\n\n## Non-claims\n\n",
        packet.findings_identical
    ));
    for claim in &packet.non_claims {
        text.push_str(&format!("- {claim}\n"));
    }
    text
}
