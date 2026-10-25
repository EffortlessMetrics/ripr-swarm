//! `cargo xtask size-scaling --binary <path>` (#10362).
//!
//! Runs `ripr check` against generated files of one size, then twice that,
//! then four times that, and fails when the cost of a file grows faster than
//! its size. A single large file that costs a minute is a hostile-input
//! failure even when the output is correct (#10017), and a quadratic term is
//! invisible at the sizes real repositories use.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::run as shared_run;

mod cells;
mod judge;

#[cfg(test)]
mod tests;

use cells::{Cell, cells};
use judge::{Limits, Outcome, Sample, Verdict, judge};

const DEFAULT_OUT: &str = "target/ripr/reports";
const DEFAULT_WORK: &str = "target/ripr/size-scaling";
const REPORT_JSON: &str = "size-scaling.json";
const REPORT_MD: &str = "size-scaling.md";
const SCHEMA: &str = "ripr-size-scaling-v1";
const OUTPUT_BYTES: usize = 16 * 1024 * 1024;
/// A cell that reaches this deadline is stopped and recorded as a failure.
const DEADLINE: Duration = Duration::from_mins(3);

pub(crate) struct Options {
    pub(crate) binary: PathBuf,
    pub(crate) out: PathBuf,
    pub(crate) work: PathBuf,
    /// Size of the smallest step; the others are 2x and 4x this.
    pub(crate) base_kib: usize,
    pub(crate) languages: Vec<String>,
    pub(crate) limits: Limits,
}

#[derive(Serialize)]
pub(crate) struct Row {
    cell: String,
    language: String,
    role: String,
    shape: String,
    samples: Vec<RowSample>,
    growth: Option<f64>,
    allowed_growth: Option<f64>,
    verdict: String,
    detail: String,
}

#[derive(Serialize)]
pub(crate) struct RowSample {
    bytes: usize,
    millis: u128,
    outcome: String,
}

#[derive(Serialize)]
pub(crate) struct Packet {
    schema: &'static str,
    binary: String,
    binary_sha256: String,
    host: String,
    base_kib: usize,
    max_doubling_ratio: f64,
    noise_floor_ms: u128,
    quiet_ms: u128,
    ceiling_secs: u64,
    rows: Vec<Row>,
    passed: bool,
    non_claims: Vec<&'static str>,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let packet = build_packet(&options)?;
    fs::create_dir_all(&options.out)
        .map_err(|error| format!("failed to create {}: {error}", options.out.display()))?;
    let json_path = options.out.join(REPORT_JSON);
    let md_path = options.out.join(REPORT_MD);
    let json = serde_json::to_string_pretty(&packet)
        .map_err(|error| format!("failed to render {REPORT_JSON}: {error}"))?;
    fs::write(&json_path, json + "\n")
        .map_err(|error| format!("failed to write {}: {error}", json_path.display()))?;
    fs::write(&md_path, render_markdown(&packet))
        .map_err(|error| format!("failed to write {}: {error}", md_path.display()))?;
    println!(
        "size-scaling: {} ({} cells); wrote {} and {}",
        if packet.passed { "passed" } else { "failed" },
        packet.rows.len(),
        json_path.display(),
        md_path.display()
    );
    if packet.passed {
        return Ok(());
    }
    let failed: Vec<&str> = packet
        .rows
        .iter()
        .filter(|row| row.verdict != "pass")
        .map(|row| row.cell.as_str())
        .collect();
    Err(format!("size-scaling failed: {}", failed.join(", ")))
}

pub(crate) fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut binary = None;
    let mut options = Options {
        binary: PathBuf::new(),
        out: PathBuf::from(DEFAULT_OUT),
        work: PathBuf::from(DEFAULT_WORK),
        base_kib: 512,
        languages: Vec::new(),
        limits: Limits::default(),
    };
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match flag.as_str() {
            "--binary" => binary = Some(PathBuf::from(value("--binary")?)),
            "--out" => options.out = PathBuf::from(value("--out")?),
            "--work" => options.work = PathBuf::from(value("--work")?),
            "--base-kib" => options.base_kib = number(&value("--base-kib")?, "--base-kib")?,
            "--languages" => {
                options.languages = value("--languages")?
                    .split(',')
                    .map(str::to_string)
                    .collect();
            }
            "--max-doubling-ratio" => {
                options.limits.max_doubling_ratio = value("--max-doubling-ratio")?
                    .parse()
                    .map_err(|_error| "--max-doubling-ratio must be a number".to_string())?;
            }
            "--ceiling-secs" => {
                options.limits.ceiling_ms =
                    u128::from(number::<u64>(&value("--ceiling-secs")?, "--ceiling-secs")?) * 1000;
            }
            other => return Err(format!("unknown size-scaling argument `{other}`")),
        }
    }
    options.binary = binary.ok_or_else(|| "size-scaling requires --binary <path>".to_string())?;
    if options.base_kib == 0 {
        return Err("--base-kib must be at least 1".to_string());
    }
    if !options.limits.max_doubling_ratio.is_finite() {
        return Err("--max-doubling-ratio must be a finite number".to_string());
    }
    if options.limits.max_doubling_ratio < 2.0 {
        return Err(
            "--max-doubling-ratio below 2.0 would fail a perfectly linear cost".to_string(),
        );
    }
    for language in &options.languages {
        if !cells::LANGUAGES.contains(&language.as_str()) {
            return Err(format!(
                "unknown language `{language}`; expected one of {}",
                cells::LANGUAGES.join(", ")
            ));
        }
    }
    Ok(options)
}

fn number<T: std::str::FromStr>(text: &str, name: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_error| format!("{name} must be a whole number"))
}

pub(crate) fn build_packet(options: &Options) -> Result<Packet, String> {
    if !cfg!(unix) {
        return Err("size-scaling runs on a Unix host only".to_string());
    }
    let binary = fs::canonicalize(&options.binary)
        .map_err(|error| format!("failed to resolve {}: {error}", options.binary.display()))?;
    fs::create_dir_all(&options.work)
        .map_err(|error| format!("failed to create {}: {error}", options.work.display()))?;
    let run_root = fs::canonicalize(&options.work)
        .map_err(|error| format!("failed to resolve {}: {error}", options.work.display()))?
        .join(std::process::id().to_string());
    if run_root.exists() {
        return Err(format!(
            "owned work root {} already exists; refusing to reuse it",
            run_root.display()
        ));
    }
    fs::create_dir_all(&run_root)
        .map_err(|error| format!("failed to create {}: {error}", run_root.display()))?;
    // Measure a private copy so a rebuild during the run cannot change the
    // binary under test, and record exactly which bytes were measured.
    let snapshot = run_root.join("ripr-under-test");
    fs::copy(&binary, &snapshot)
        .map_err(|error| format!("failed to copy {}: {error}", binary.display()))?;
    let bytes = fs::read(&snapshot)
        .map_err(|error| format!("failed to read {}: {error}", snapshot.display()))?;
    let binary_sha256 = crate::blind_journey::sha256_hex(&bytes);
    drop(bytes);
    let mut rows = Vec::new();
    for cell in cells(&options.languages) {
        let row = measure(&snapshot, &run_root, &cell, options);
        eprintln!(
            "size-scaling: {} {} ({})",
            row.cell, row.verdict, row.detail
        );
        rows.push(row);
    }
    let cleaned = fs::remove_dir_all(&run_root);
    let leftover = cleaned.is_err() && run_root.exists();
    let mut passed = !rows.is_empty() && rows.iter().all(|row| row.verdict == "pass");
    if leftover {
        passed = false;
        eprintln!("size-scaling: could not remove {}", run_root.display());
    }
    Ok(Packet {
        schema: SCHEMA,
        binary: binary.to_string_lossy().to_string(),
        binary_sha256,
        host: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        base_kib: options.base_kib,
        max_doubling_ratio: options.limits.max_doubling_ratio,
        noise_floor_ms: options.limits.noise_floor_ms,
        quiet_ms: options.limits.quiet_ms,
        ceiling_secs: u64::try_from(options.limits.ceiling_ms / 1000).unwrap_or(u64::MAX),
        rows,
        passed,
        non_claims: non_claims(),
    })
}

fn measure(binary: &Path, run_root: &Path, cell: &Cell, options: &Options) -> Row {
    let mut samples = Vec::new();
    let mut instrument_error = None;
    for step in 0..3 {
        let bytes = options.base_kib * 1024 * (1 << step);
        match run_step(binary, run_root, cell, bytes, step) {
            Ok(sample) => samples.push(sample),
            Err(error) => {
                instrument_error = Some(error);
                break;
            }
        }
    }
    let (verdict, detail, growth, allowed) = match instrument_error {
        Some(error) => (Verdict::Fail, error, None, None),
        None => {
            let judged = judge(&samples, &options.limits);
            (judged.verdict, judged.detail, judged.growth, judged.allowed)
        }
    };
    Row {
        cell: cell.id(),
        language: cell.language.to_string(),
        role: cell.role.to_string(),
        shape: cell.shape.to_string(),
        samples: samples
            .iter()
            .map(|sample| RowSample {
                bytes: sample.bytes,
                millis: sample.millis,
                outcome: sample.outcome.as_str().to_string(),
            })
            .collect(),
        growth,
        allowed_growth: allowed,
        verdict: verdict.as_str().to_string(),
        detail,
    }
}

fn run_step(
    binary: &Path,
    run_root: &Path,
    cell: &Cell,
    bytes: usize,
    step: usize,
) -> Result<Sample, String> {
    let root = run_root.join(format!("{}-{step}", cell.id()));
    for (relative, content) in cell.files(bytes) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        fs::write(&path, content)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    }
    let home = root.join(".home");
    let home_text = home.to_string_lossy().to_string();
    fs::create_dir_all(&home)
        .map_err(|error| format!("failed to create {}: {error}", home.display()))?;
    let ceiling = run_root.to_string_lossy().to_string();
    let envs: Vec<(&str, &str)> = vec![
        ("HOME", &home_text),
        ("TMPDIR", &home_text),
        ("GIT_CEILING_DIRECTORIES", &ceiling),
        ("NO_COLOR", "1"),
        ("RUST_BACKTRACE", "0"),
    ];
    let removed = removed_env();
    let removed_refs: Vec<&str> = removed.iter().map(String::as_str).collect();
    let args: Vec<String> = [
        "check",
        "--root",
        ".",
        "--diff",
        cells::DIFF,
        "--format",
        "json",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    let timed = shared_run::capture_bytes_in_dir_with_env_budget(
        binary,
        &args,
        &root,
        (&envs, &removed_refs),
        shared_run::ByteCaptureBudget {
            timeout: DEADLINE,
            stdout_bytes: OUTPUT_BYTES,
            stderr_bytes: OUTPUT_BYTES,
        },
        &format!("size-scaling cell {} at {bytes} bytes", cell.id()),
    )?;
    let _ = fs::remove_dir_all(&root);
    let outcome = if timed.timed_out {
        Outcome::Deadline
    } else {
        match timed.status.and_then(|status| status.code()) {
            Some(0 | 1) => match analyzed(&timed.stdout, cell) {
                Ok(()) => Outcome::Completed,
                Err(reason) => {
                    eprintln!("size-scaling: {} at {bytes} bytes: {reason}", cell.id());
                    Outcome::NotAnalyzed
                }
            },
            Some(code) => Outcome::Exit(code),
            None => Outcome::Signalled,
        }
    };
    Ok(Sample {
        bytes,
        millis: timed.duration.as_millis(),
        outcome,
    })
}

/// A fast exit proves nothing if the analyzer never read the file. The report
/// must carry a finding for the changed file and must not name the large file
/// in a limitation (a skipped or refused file is cheap, not scaled).
fn analyzed(stdout: &[u8], cell: &Cell) -> Result<(), String> {
    let report: serde_json::Value = serde_json::from_slice(stdout)
        .map_err(|error| format!("stdout is not a JSON report: {error}"))?;
    let changed = cell.changed_path();
    let has_finding = report
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|findings| {
            findings.iter().any(|finding| {
                ["/canonical_gap/file", "/probe/file"]
                    .iter()
                    .any(|pointer| {
                        finding
                            .pointer(pointer)
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|file| file.trim_start_matches("./") == changed)
                    })
            })
        });
    if !has_finding {
        return Err(format!("no finding for {changed}; nothing was analyzed"));
    }
    let big = cell.big_path();
    let limited = ["/analysis_outcome/outcome/limitations", "/limitations"]
        .iter()
        .filter_map(|pointer| report.pointer(pointer))
        .any(|limitations| limitations.to_string().contains(big));
    if limited {
        return Err(format!(
            "a limitation names {big}; the file was not analyzed"
        ));
    }
    Ok(())
}

/// Ambient `RIPR_*` configuration (including the file-read cap) must not
/// change a measurement.
fn removed_env() -> Vec<String> {
    let mut names: Vec<String> = std::env::vars()
        .map(|(name, _)| name)
        .filter(|name| name.starts_with("RIPR_"))
        .collect();
    names.sort();
    names
}

fn non_claims() -> Vec<&'static str> {
    vec![
        "Wall time only. Peak memory is not measured; a cost that is quadratic in memory but not in time passes.",
        "One run per size. A noisy host can move one ratio; the gate allows slack and passes cells whose largest step is under the quiet limit.",
        "Measures the binary it is given. A debug binary passes slower absolute numbers through the same ratio check; only the ceiling is build-dependent.",
        "Perl is not covered: ripr needs an external producer for it (#10364).",
        "Does not change product behavior or qualify a release.",
    ]
}

fn render_markdown(packet: &Packet) -> String {
    let mut text = format!(
        "# Size scaling\n\nSchema `{}`; {}; steps {}, {} and {} KiB; at most {:.2}x per doubling (growth measured from at least {} ms; cells under {} ms pass; ceiling {} s).\n\nResult: **{}**\n\n| Cell | Samples (bytes: ms) | Growth | Allowed | Verdict |\n|---|---|---|---|---|\n",
        packet.schema,
        packet.host,
        packet.base_kib,
        packet.base_kib * 2,
        packet.base_kib * 4,
        packet.max_doubling_ratio,
        packet.noise_floor_ms,
        packet.quiet_ms,
        packet.ceiling_secs,
        if packet.passed { "passed" } else { "failed" },
    );
    for row in &packet.rows {
        let samples: Vec<String> = row
            .samples
            .iter()
            .map(|sample| format!("{}: {}", sample.bytes, sample.millis))
            .collect();
        let number = |value: Option<f64>| value.map_or("-".to_string(), |v| format!("{v:.2}x"));
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} {} |\n",
            row.cell,
            samples.join("; "),
            number(row.growth),
            number(row.allowed_growth),
            row.verdict,
            row.detail,
        ));
    }
    text.push_str("\n## Non-claims\n\n");
    for claim in &packet.non_claims {
        text.push_str(&format!("- {claim}\n"));
    }
    text
}
