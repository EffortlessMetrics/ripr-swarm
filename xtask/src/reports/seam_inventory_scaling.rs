//! `cargo xtask seam-inventory-scaling-benchmark` — a bounded, receipt-backed
//! scaling benchmark for the seam-inventory construction cost behind
//! #1887 / #4996 / #4997.
//!
//! The command generates deterministic synthetic Rust workspaces at several
//! file counts under `target/`, then times cold `ripr check` inventory runs
//! (`repo-seams-json` plus `repo-exposure-json`) at each size. The receipt
//! records the per-size cost curve so a cap-during-construction fix can show
//! a slope change on a later run. Like the other benchmark receipts, claims
//! are limited to the recorded revision and runner class.

use crate::run::{capture_output_with_timeout, run, run_output, run_output_owned};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: &str = "ripr-seam-inventory-scaling-benchmark-v1";
const DEFAULT_SIZES: &str = "200,800,2000";
const DEFAULT_SAMPLES: usize = 2;
const DEFAULT_TIMEOUT_MS: u64 = 300_000;
const MAX_SIZE: usize = 12_000;
const CACHE_ENV: &str = "RIPR_CACHE_DIR";
const SEAMS_FORMAT: &str = "repo-seams-json";
const EXPOSURE_FORMAT: &str = "repo-exposure-json";

pub(crate) fn seam_inventory_scaling_benchmark(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_options(args)?;
    run("cargo", &["build", "-p", "ripr"])?;
    let binary = crate::ripr_debug_binary();

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let scratch =
        PathBuf::from("target/ripr/reports").join(format!("seam-inventory-scaling-{stamp}"));
    fs::create_dir_all(&scratch)
        .map_err(|err| format!("create benchmark scratch {}: {err}", scratch.display()))?;
    let cache_dir = scratch.join("cache");
    let envs = [(CACHE_ENV, cache_dir.to_string_lossy().into_owned())];
    let timeout = Duration::from_millis(options.timeout_ms);

    let mut sizes = Vec::new();
    for files in &options.sizes {
        let workspace = generate_workspace(&scratch, *files)?;
        let seams = run_format_series(
            &binary,
            &workspace,
            SEAMS_FORMAT,
            &envs,
            timeout,
            options.samples,
            parse_seams_count,
        )?;
        let exposure = run_format_series(
            &binary,
            &workspace,
            EXPOSURE_FORMAT,
            &envs,
            timeout,
            options.samples,
            parse_exposure_count,
        )?;
        sizes.push(SizeReport {
            files: *files,
            seams,
            exposure,
        });
        if !options.keep_workspaces {
            fs::remove_dir_all(&workspace).map_err(|err| {
                format!("remove generated workspace {}: {err}", workspace.display())
            })?;
        }
    }
    fs::remove_dir_all(&cache_dir).ok();

    let report = build_report(&options, &sizes);
    let json_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("serialize seam inventory scaling benchmark: {err}"))?;
    crate::write_report(
        "seam-inventory-scaling-benchmark.json",
        &format!("{json_text}\n"),
    )?;
    crate::write_report(
        "seam-inventory-scaling-benchmark.md",
        &benchmark_markdown(&report),
    )?;
    println!("Wrote target/ripr/reports/seam-inventory-scaling-benchmark.json");
    println!("Wrote target/ripr/reports/seam-inventory-scaling-benchmark.md");
    Ok(())
}

const USAGE: &str = "usage: cargo xtask seam-inventory-scaling-benchmark [--sizes <n,n,n>] [--samples <n>] [--timeout-ms <n>] [--keep-workspaces]";

#[derive(Clone, Debug)]
struct Options {
    sizes: Vec<usize>,
    samples: usize,
    timeout_ms: u64,
    keep_workspaces: bool,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut sizes = None;
    let mut samples = DEFAULT_SAMPLES;
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    let mut keep_workspaces = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--sizes" => {
                index += 1;
                sizes = Some(parse_sizes(required_arg(args, index, "--sizes")?)?);
            }
            "--samples" => {
                index += 1;
                samples = required_arg(args, index, "--samples")?
                    .parse()
                    .map_err(|err| {
                        format!(
                            "seam-inventory-scaling-benchmark --samples must be positive: {err}"
                        )
                    })?;
                if samples == 0 {
                    return Err(
                        "seam-inventory-scaling-benchmark --samples must be positive".to_string(),
                    );
                }
            }
            "--timeout-ms" => {
                index += 1;
                timeout_ms = required_arg(args, index, "--timeout-ms")?
                    .parse()
                    .map_err(|err| {
                        format!(
                            "seam-inventory-scaling-benchmark --timeout-ms must be positive: {err}"
                        )
                    })?;
                if timeout_ms == 0 {
                    return Err(
                        "seam-inventory-scaling-benchmark --timeout-ms must be positive"
                            .to_string(),
                    );
                }
            }
            "--keep-workspaces" => {
                keep_workspaces = true;
            }
            other => {
                return Err(format!(
                    "unknown seam-inventory-scaling-benchmark argument `{other}`; {USAGE}"
                ));
            }
        }
        index += 1;
    }
    Ok(Options {
        sizes: sizes.unwrap_or_else(|| {
            DEFAULT_SIZES
                .split(',')
                .filter_map(|size| size.parse().ok())
                .collect()
        }),
        samples,
        timeout_ms,
        keep_workspaces,
    })
}

fn parse_sizes(value: &str) -> Result<Vec<usize>, String> {
    let mut sizes = Vec::new();
    for part in value.split(',') {
        let size: usize = part.trim().parse().map_err(|err| {
            format!("seam-inventory-scaling-benchmark --sizes must be a comma list of file counts: {err}")
        })?;
        if size == 0 || size > MAX_SIZE {
            return Err(format!(
                "seam-inventory-scaling-benchmark sizes must be within 1..={MAX_SIZE}, got {size}"
            ));
        }
        sizes.push(size);
    }
    if sizes.is_empty() {
        return Err(format!(
            "seam-inventory-scaling-benchmark needs at least one --sizes entry; {USAGE}"
        ));
    }
    sizes.sort_unstable();
    sizes.dedup();
    Ok(sizes)
}

fn required_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    let Some(value) = args.get(index) else {
        return Err(format!("missing value for {flag}; {USAGE}"));
    };
    if value.trim().is_empty() {
        return Err(format!("{flag} requires a non-empty value; {USAGE}"));
    }
    Ok(value)
}

/// Generate a deterministic synthetic workspace: `files` production modules
/// with three small fns each, no tests directory, fixed template content.
fn generate_workspace(scratch: &Path, files: usize) -> Result<PathBuf, String> {
    let root = scratch.join(format!("ws-{files}"));
    let src = root.join("src");
    fs::create_dir_all(&src)
        .map_err(|err| format!("create generated workspace {}: {err}", root.display()))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"scaleprobe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|err| format!("write generated Cargo.toml: {err}"))?;
    let mut lib = String::new();
    for index in 0..files {
        lib.push_str(&format!("mod m{index};\n"));
        let body = format!(
            "pub fn f{index}_a(x: u32) -> u32 {{ if x > {index} {{ x * 2 }} else {{ x + 1 }} }}\n\
             pub fn f{index}_b(s: &str) -> usize {{ s.len() + {index} }}\n\
             pub fn f{index}_c(flag: bool) -> u32 {{ if flag {{ {index} }} else {{ 0 }} }}\n"
        );
        fs::write(src.join(format!("m{index}.rs")), body)
            .map_err(|err| format!("write generated module m{index}: {err}"))?;
    }
    fs::write(src.join("lib.rs"), lib).map_err(|err| format!("write generated lib.rs: {err}"))?;
    Ok(root)
}

#[derive(Clone, Debug)]
struct Sample {
    status: String,
    duration_ms: u128,
    seam_count: Option<u64>,
    stdout_bytes: usize,
    detail: Option<String>,
}

struct SizeReport {
    files: usize,
    seams: Vec<Sample>,
    exposure: Vec<Sample>,
}

fn run_format_series(
    binary: &Path,
    root: &Path,
    format: &str,
    envs: &[(&str, String)],
    timeout: Duration,
    samples: usize,
    parse_count: fn(&Value) -> Option<u64>,
) -> Result<Vec<Sample>, String> {
    let owned: Vec<(&str, &str)> = envs
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let mut out = Vec::new();
    for _ in 0..samples {
        clear_env_cache(envs)?;
        let args = [
            "check".to_string(),
            "--root".to_string(),
            root.display().to_string(),
            "--format".to_string(),
            format.to_string(),
        ];
        let output = capture_output_with_timeout(
            &binary.display().to_string(),
            &args,
            &owned,
            timeout,
            "seam inventory scaling benchmark",
        )?;
        let mut status = if output.timed_out {
            "timeout"
        } else if output.status.is_some_and(|status| status.success()) {
            "pass"
        } else {
            "fail"
        };
        let stdout_bytes = output.stdout.len();
        let (seam_count, detail) = if status == "pass" {
            match serde_json::from_str::<Value>(&output.stdout) {
                Ok(value) => (parse_count(&value), None),
                Err(err) => {
                    status = "invalid_receipt";
                    (None, Some(format!("{format} JSON parse failed: {err}")))
                }
            }
        } else {
            (None, Some(summarize_output(&output.stderr)))
        };
        out.push(Sample {
            status: status.to_string(),
            duration_ms: output.duration.as_millis(),
            seam_count,
            stdout_bytes,
            detail,
        });
    }
    Ok(out)
}

fn parse_seams_count(value: &Value) -> Option<u64> {
    value
        .pointer("/seams")?
        .as_array()
        .map(|seams| seams.len() as u64)
}

fn parse_exposure_count(value: &Value) -> Option<u64> {
    value.pointer("/metrics/seams_total")?.as_u64()
}

fn clear_env_cache(envs: &[(&str, String)]) -> Result<(), String> {
    let cache_dir = envs
        .iter()
        .find(|(name, _)| *name == CACHE_ENV)
        .map(|(_, value)| PathBuf::from(value))
        .ok_or_else(|| format!("benchmark environment is missing {CACHE_ENV}"))?;
    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)
            .map_err(|err| format!("remove benchmark cache {}: {err}", cache_dir.display()))?;
    }
    fs::create_dir_all(&cache_dir)
        .map_err(|err| format!("create benchmark cache {}: {err}", cache_dir.display()))
}

fn percentile(values: &mut [u128], percentile: usize) -> u128 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let rank = ((values.len() * percentile).saturating_add(99) / 100).saturating_sub(1);
    values[rank.min(values.len() - 1)]
}

fn series_p50(samples: &[Sample]) -> u128 {
    let mut values: Vec<u128> = samples.iter().map(|sample| sample.duration_ms).collect();
    percentile(&mut values, 50)
}

fn series_p95(samples: &[Sample]) -> u128 {
    let mut values: Vec<u128> = samples.iter().map(|sample| sample.duration_ms).collect();
    percentile(&mut values, 95)
}

fn all_pass(samples: &[Sample]) -> bool {
    !samples.is_empty() && samples.iter().all(|sample| sample.status == "pass")
}

fn series_json(samples: &[Sample]) -> Value {
    json!({
        "p50_ms": series_p50(samples),
        "p95_ms": series_p95(samples),
        "samples": samples.iter().map(|sample| json!({
            "status": sample.status,
            "duration_ms": sample.duration_ms,
            "seam_count": sample.seam_count,
            "stdout_bytes": sample.stdout_bytes,
            "detail": sample.detail,
        })).collect::<Vec<_>>()
    })
}

/// Coarse ms-per-file slope between the smallest and largest passing size.
/// A construction-cost fix (cap during build) should bend this slope, not
/// just shift one point.
fn slope_ms_per_file(sizes: &[SizeReport], select: fn(&SizeReport) -> &[Sample]) -> Option<f64> {
    let first = sizes.first()?;
    let last = sizes.last()?;
    if last.files <= first.files || !all_pass(select(first)) || !all_pass(select(last)) {
        return None;
    }
    Some(
        (series_p50(select(last)) as f64 - series_p50(select(first)) as f64)
            / (last.files - first.files) as f64,
    )
}

fn build_report(options: &Options, sizes: &[SizeReport]) -> Value {
    let complete = sizes
        .iter()
        .all(|size| all_pass(&size.seams) && all_pass(&size.exposure));
    json!({
        "schema_version": SCHEMA_VERSION,
        "tool": "ripr",
        "report": "seam-inventory-scaling-benchmark",
        "status": if complete { "pass" } else { "inconclusive" },
        "revision": git_revision(),
        "runner_class": runner_class(),
        "analyzer_version": analyzer_version(&crate::ripr_debug_binary()),
        "cache_policy": "isolated RIPR_CACHE_DIR cleared before every sample (cold inventory)",
        "generator": "deterministic synthetic workspace: N production modules, 3 small fns each, no tests directory",
        "sizes": options.sizes,
        "samples": options.samples,
        "timeout_ms": options.timeout_ms,
        "commands": {
            "seams": "ripr check --root <ws> --format repo-seams-json",
            "exposure": "ripr check --root <ws> --format repo-exposure-json"
        },
        "sizes_report": sizes.iter().map(|size| json!({
            "files": size.files,
            "seams": series_json(&size.seams),
            "exposure": series_json(&size.exposure),
        })).collect::<Vec<_>>(),
        "comparison": {
            "seams_slope_ms_per_file": slope_ms_per_file(sizes, |size| &size.seams),
            "exposure_slope_ms_per_file": slope_ms_per_file(sizes, |size| &size.exposure),
        },
        "claim_boundary": "Static cold-inventory wall time on synthetic workspaces for the recorded revision and runner class only; not peak memory, not a gate, not universal latency. Compare runs only on the same runner class."
    })
}

fn benchmark_markdown(report: &Value) -> String {
    let mut rows = String::new();
    if let Some(sizes) = report["sizes_report"].as_array() {
        for size in sizes {
            rows.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                size["files"],
                size["seams"]["p50_ms"],
                size["seams"]["p95_ms"],
                size["exposure"]["p50_ms"],
                size["exposure"]["p95_ms"],
            ));
        }
    }
    format!(
        "# Seam Inventory Scaling Benchmark\n\nStatus: `{}`\n\nRevision: `{}`\nRunner: `{}`\nAnalyzer: `{}`\n\n| Files | Seams p50 (ms) | Seams p95 (ms) | Exposure p50 (ms) | Exposure p95 (ms) |\n| ---: | ---: | ---: | ---: | ---: |\n{}\nSeams slope: {} ms/file; exposure slope: {} ms/file.\n\nClaim boundary: {}\n",
        report["status"].as_str().unwrap_or("unknown"),
        report["revision"].as_str().unwrap_or("unavailable"),
        report["runner_class"].as_str().unwrap_or("unknown"),
        report["analyzer_version"].as_str().unwrap_or("unknown"),
        rows,
        report["comparison"]["seams_slope_ms_per_file"],
        report["comparison"]["exposure_slope_ms_per_file"],
        report["claim_boundary"].as_str().unwrap_or("unknown"),
    )
}

fn summarize_output(stderr: &str) -> String {
    const LIMIT: usize = 500;
    let trimmed = stderr.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    format!("{}…", trimmed.chars().take(LIMIT).collect::<String>())
}

fn git_revision() -> String {
    run_output("git", &["rev-parse", "HEAD"])
        .ok()
        .map(|output| output.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unavailable".to_string())
}

fn runner_class() -> String {
    std::env::var("RUNNER_NAME")
        .or_else(|_| std::env::var("GITHUB_RUNNER_NAME"))
        .unwrap_or_else(|_| format!("local-{}-{}", std::env::consts::OS, std::env::consts::ARCH))
}

fn analyzer_version(binary: &Path) -> String {
    run_output_owned(&binary.display().to_string(), &["--version".to_string()])
        .ok()
        .map(|output| output.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unavailable".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_options_defaults_to_three_sizes() -> Result<(), String> {
        let options = parse_options(&[])?;
        assert_eq!(options.sizes, vec![200, 800, 2000]);
        assert_eq!(options.samples, DEFAULT_SAMPLES);
        assert!(!options.keep_workspaces);
        Ok(())
    }

    #[test]
    fn parse_sizes_rejects_zero_and_oversize() {
        assert!(parse_sizes("0").is_err());
        assert!(parse_sizes("12001").is_err());
        assert!(parse_sizes("").is_err());
        assert_eq!(parse_sizes("800,200,800").expect("dedup"), vec![200, 800]);
    }

    #[test]
    fn empty_series_has_no_slope() {
        assert_eq!(slope_ms_per_file(&[], |size| &size.seams), None);
    }
}
