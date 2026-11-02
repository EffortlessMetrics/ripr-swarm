//! `cargo xtask kani`: honest runner for the Kani bounded-model-checking lane
//! (RIPR-SPEC-0371).
//!
//! The lane runs `cargo kani -p ripr` once (Kani compiles the whole library, so
//! one invocation per harness would multiply a minutes-long build) and reads
//! one result block per discovered `#[kani::proof]` harness:
//!
//! - `VERIFIED` (within the harness's documented bounds), or
//! - `FAILED` (counterexample, or a `cover!` that is unreachable and so makes
//!   the proof vacuous), or
//! - `NOT-ESTABLISHED` (tool missing, timed out, no verdict in the output).
//!
//! Absence never passes: a missing tool, zero harnesses (unless the operator
//! passes `--allow-empty`) and every `NOT-ESTABLISHED` harness exit nonzero.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::run;

const HARNESS_ROOT: &str = "crates/ripr/src";
const REPORT_DIR: &str = "target/ripr/reports";
const REPORT_FILE: &str = "kani.md";
/// Whole-run wall-clock deadline; a wedged solver is `NOT-ESTABLISHED`.
const DEFAULT_TIMEOUT_SECS: u64 = 3600;
/// Per-harness deadline handed to Kani, so one non-terminating harness is
/// `NOT-ESTABLISHED` for itself and the rest of the lane still reports. The
/// slowest verified harness runs about four minutes; this leaves headroom
/// without letting one wedge consume the whole-run deadline.
const DEFAULT_HARNESS_TIMEOUT_SECS: u64 = 900;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KaniArgs {
    pub(crate) allow_empty: bool,
    pub(crate) harness: Option<String>,
    pub(crate) default_unwind: Option<u32>,
    pub(crate) timeout_secs: u64,
    pub(crate) harness_timeout_secs: u64,
}

pub(crate) fn parse_args(args: &[String]) -> Result<KaniArgs, String> {
    let mut parsed = KaniArgs {
        allow_empty: false,
        harness: None,
        default_unwind: None,
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        harness_timeout_secs: DEFAULT_HARNESS_TIMEOUT_SECS,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--allow-empty" => parsed.allow_empty = true,
            "--harness" => {
                parsed.harness = Some(next_value(&mut iter, "--harness")?);
            }
            "--default-unwind" => {
                let value = next_value(&mut iter, "--default-unwind")?;
                parsed.default_unwind = Some(parse_positive(&value, "--default-unwind")?);
            }
            "--timeout-secs" => {
                let value = next_value(&mut iter, "--timeout-secs")?;
                parsed.timeout_secs = u64::from(parse_positive(&value, "--timeout-secs")?);
            }
            "--harness-timeout-secs" => {
                let value = next_value(&mut iter, "--harness-timeout-secs")?;
                parsed.harness_timeout_secs =
                    u64::from(parse_positive(&value, "--harness-timeout-secs")?);
            }
            other => {
                return Err(format!(
                    "unknown kani argument `{other}`; usage: cargo xtask kani [--allow-empty] [--harness <name>] [--default-unwind <n>] [--timeout-secs <n>] [--harness-timeout-secs <n>]"
                ));
            }
        }
    }
    Ok(parsed)
}

fn next_value<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<String, String> {
    iter.next()
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive(value: &str, flag: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(number) if number > 0 => Ok(number),
        _ => Err(format!("{flag} requires a positive integer, got `{value}`")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Harness {
    pub(crate) file: String,
    pub(crate) name: String,
}

/// Finds `#[kani::proof]` harnesses in `(path, source)` pairs: the attribute
/// line followed (after any further attributes or doc comments) by `fn name`.
pub(crate) fn discover_harnesses(files: &[(String, String)]) -> Vec<Harness> {
    let mut found = Vec::new();
    for (file, text) in files {
        let mut pending = false;
        let mut in_raw_string = false;
        for line in text.lines() {
            let trimmed = line.trim();
            // Text inside a `r#"..."#` literal is fixture data (for example the
            // test-style fixtures that mention `#[kani::proof]`), not a harness.
            if in_raw_string {
                if trimmed.contains("\"#") {
                    in_raw_string = false;
                }
                continue;
            }
            if let Some(open) = trimmed.find("r#\"")
                && !trimmed[open + 3..].contains("\"#")
            {
                in_raw_string = true;
                pending = false;
                continue;
            }
            if trimmed.starts_with("#[kani::proof") {
                pending = true;
                continue;
            }
            if !pending {
                continue;
            }
            if trimmed.starts_with("#[") || trimmed.starts_with("//") || trimmed.is_empty() {
                continue;
            }
            pending = false;
            if let Some(name) = function_name(trimmed) {
                found.push(Harness {
                    file: file.clone(),
                    name,
                });
            }
        }
    }
    found.sort_by(|left, right| (&left.file, &left.name).cmp(&(&right.file, &right.name)));
    found
}

fn function_name(line: &str) -> Option<String> {
    let rest = line.split("fn ").nth(1)?;
    let name: String = rest
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HarnessStatus {
    Verified,
    Failed(String),
    NotEstablished(String),
}

impl HarnessStatus {
    fn label(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Failed(_) => "FAILED",
            Self::NotEstablished(_) => "NOT-ESTABLISHED",
        }
    }

    fn detail(&self) -> &str {
        match self {
            Self::Verified => "within the harness bounds",
            Self::Failed(detail) | Self::NotEstablished(detail) => detail,
        }
    }
}

/// Classifies one harness result block from Kani's output.
/// Only an explicit success verdict with every `cover!` satisfied is
/// `Verified`; a missing verdict is never a pass.
pub(crate) fn classify_output(output: &str) -> HarnessStatus {
    // Kani prints `VERIFICATION:- FAILED` for a solver timeout too; that is no
    // counterexample, so it is checked first and stays `NOT-ESTABLISHED`.
    if output.contains("CBMC timed out") {
        return HarnessStatus::NotEstablished(
            "CBMC exceeded the per-harness timeout; no verdict".to_string(),
        );
    }
    if output.contains("VERIFICATION:- FAILED") {
        return HarnessStatus::Failed("verification failed (counterexample)".to_string());
    }
    if output.contains("No proof harnesses") {
        return HarnessStatus::NotEstablished(
            "kani found no proof harness with this name".to_string(),
        );
    }
    if output.contains("VERIFICATION:- SUCCESSFUL") {
        if output.contains("Status: UNSATISFIABLE") || cover_shortfall(output) {
            return HarnessStatus::Failed(
                "a cover! property is unreachable, so the proof is vacuous".to_string(),
            );
        }
        return HarnessStatus::Verified;
    }
    HarnessStatus::NotEstablished(
        "no successful verification verdict in the kani output".to_string(),
    )
}

/// True when kani's summary reports fewer satisfied covers than declared
/// (`** 1 of 2 cover properties satisfied`).
fn cover_shortfall(output: &str) -> bool {
    output.lines().any(|line| {
        let Some((head, _)) = line.split_once(" cover properties satisfied") else {
            return false;
        };
        let words: Vec<&str> = head.split_whitespace().collect();
        let Some(split) = words.iter().position(|word| *word == "of") else {
            return false;
        };
        let satisfied = split
            .checked_sub(1)
            .and_then(|index| words.get(index))
            .and_then(|word| word.parse::<u32>().ok());
        let declared = words
            .get(split + 1)
            .and_then(|word| word.parse::<u32>().ok());
        matches!((satisfied, declared), (Some(done), Some(total)) if done < total)
    })
}

pub(crate) struct HarnessResult {
    pub(crate) harness: Harness,
    pub(crate) status: HarnessStatus,
    pub(crate) time: String,
}

/// Whether the lane as a whole is acceptable.
pub(crate) fn lane_verdict(
    results: &[HarnessResult],
    tool_available: bool,
    allow_empty: bool,
) -> Result<(), String> {
    if !tool_available {
        return Err("NOT-ESTABLISHED: cargo-kani is not installed or not runnable".to_string());
    }
    if results.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(
                "NOT-ESTABLISHED: zero kani harnesses found (pass --allow-empty to accept)"
                    .to_string(),
            )
        };
    }
    let failed = results
        .iter()
        .filter(|result| matches!(result.status, HarnessStatus::Failed(_)))
        .count();
    let unestablished = results
        .iter()
        .filter(|result| matches!(result.status, HarnessStatus::NotEstablished(_)))
        .count();
    if failed > 0 || unestablished > 0 {
        return Err(format!(
            "kani lane not clean: {failed} failed, {unestablished} not established of {} harnesses",
            results.len()
        ));
    }
    Ok(())
}

pub(crate) fn render_report(results: &[HarnessResult], note: &str) -> String {
    let mut out = String::from("# Kani lane (RIPR-SPEC-0371)\n\n");
    out.push_str("Bounded model checking of pure cores. A VERIFIED row means no violation within the harness bounds, not a full-program or runtime-mutation claim.\n\n");
    if !note.is_empty() {
        out.push_str(&format!("{note}\n\n"));
    }
    out.push_str(
        "| Harness | File | Status | Detail | Kani time |\n| --- | --- | --- | --- | ---: |\n",
    );
    for result in results {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} |\n",
            result.harness.name,
            result.harness.file,
            result.status.label(),
            result.status.detail(),
            result.time
        ));
    }
    out
}

fn collect_sources(root: &Path, files: &mut Vec<(String, String)>) -> Result<(), String> {
    let entries =
        fs::read_dir(root).map_err(|err| format!("failed to read {}: {err}", root.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|err| format!("failed to read entry in {}: {err}", root.display()))?
            .path();
        if path.is_dir() {
            collect_sources(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            if text.contains("#[kani::proof") {
                files.push((path.to_string_lossy().replace('\\', "/"), text));
            }
        }
    }
    Ok(())
}

fn tool_available() -> bool {
    run::capture_output("cargo", &["kani", "--version"], "cargo kani --version")
        .map(|captured| captured.status.success())
        .unwrap_or(false)
}

/// Splits Kani's output into `(qualified harness name, block text)` pairs at
/// each `Checking harness <name>...` header.
pub(crate) fn split_harness_blocks(output: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Checking harness ") {
            let name = rest.trim_end().trim_end_matches("...").to_string();
            blocks.push((name, String::new()));
        } else if let Some((_, text)) = blocks.last_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    blocks
}

fn verification_time(block: &str) -> String {
    block
        .lines()
        .find_map(|line| line.trim().strip_prefix("Verification Time:"))
        .map_or_else(|| "n/a".to_string(), |time| time.trim().to_string())
}

fn matches_harness(qualified: &str, harness: &Harness) -> bool {
    qualified == harness.name || qualified.ends_with(&format!("::{}", harness.name))
}

/// Joins discovered harnesses to Kani's result blocks. A harness with no block
/// is `NOT-ESTABLISHED`, never assumed verified.
pub(crate) fn results_from_output(harnesses: &[Harness], output: &str) -> Vec<HarnessResult> {
    let blocks = split_harness_blocks(output);
    harnesses
        .iter()
        .map(|harness| {
            let block = blocks
                .iter()
                .find(|(name, _)| matches_harness(name, harness));
            let (status, time) = match block {
                Some((_, text)) => (classify_output(text), verification_time(text)),
                None => (
                    HarnessStatus::NotEstablished(
                        "kani printed no result block for this harness".to_string(),
                    ),
                    "n/a".to_string(),
                ),
            };
            HarnessResult {
                harness: harness.clone(),
                status,
                time,
            }
        })
        .collect()
}

fn all_not_established(harnesses: &[Harness], reason: &str) -> Vec<HarnessResult> {
    harnesses
        .iter()
        .map(|harness| HarnessResult {
            harness: harness.clone(),
            status: HarnessStatus::NotEstablished(reason.to_string()),
            time: "n/a".to_string(),
        })
        .collect()
}

/// The exact `cargo` arguments for the lane, kept separate so a test can pin
/// what Kani actually receives.
fn kani_command(args: &KaniArgs) -> Vec<String> {
    let mut command = vec!["kani".to_string(), "-p".to_string(), "ripr".to_string()];
    if let Some(wanted) = &args.harness {
        command.push("--harness".to_string());
        command.push(wanted.clone());
    }
    if let Some(unwind) = args.default_unwind {
        command.push("--default-unwind".to_string());
        command.push(unwind.to_string());
    }
    // `--harness-timeout` is gated behind `-Z unstable-options` in Kani 0.68.
    command.push("-Z".to_string());
    command.push("unstable-options".to_string());
    command.push("--harness-timeout".to_string());
    command.push(format!("{}s", args.harness_timeout_secs));
    command
}

fn run_kani(harnesses: &[Harness], args: &KaniArgs) -> Vec<HarnessResult> {
    let command = kani_command(args);
    let output = match run::capture_output_with_timeout(
        "cargo",
        &command,
        &[],
        Duration::from_secs(args.timeout_secs),
        "cargo kani",
    ) {
        Err(error) => return all_not_established(harnesses, &error),
        Ok(output) => output,
    };
    if output.timed_out {
        let reason = format!("cargo kani timed out after {} seconds", args.timeout_secs);
        return all_not_established(harnesses, &reason);
    }
    let combined = format!("{}\n{}", output.stdout, output.stderr);
    results_from_output(harnesses, &combined)
}

pub(crate) fn run_lane(args: &[String]) -> Result<(), String> {
    let args = parse_args(args)?;
    let mut files = Vec::new();
    collect_sources(Path::new(HARNESS_ROOT), &mut files)?;
    let mut harnesses = discover_harnesses(&files);
    if let Some(wanted) = &args.harness {
        harnesses.retain(|harness| harness.name.contains(wanted.as_str()));
    }
    let available = tool_available();
    let results = if available && !harnesses.is_empty() {
        eprintln!("kani: checking {} harness(es)", harnesses.len());
        run_kani(&harnesses, &args)
    } else {
        Vec::new()
    };
    let note = if !available {
        "NOT-ESTABLISHED: cargo-kani is not installed; no harness was run."
    } else if results.is_empty() {
        "NOT-ESTABLISHED: zero harnesses found."
    } else {
        ""
    };
    let report = render_report(&results, note);
    fs::create_dir_all(REPORT_DIR)
        .map_err(|err| format!("failed to create {REPORT_DIR}: {err}"))?;
    let report_path = Path::new(REPORT_DIR).join(REPORT_FILE);
    fs::write(&report_path, &report)
        .map_err(|err| format!("failed to write {}: {err}", report_path.display()))?;
    println!("{report}");
    lane_verdict(&results, available, args.allow_empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn result(status: HarnessStatus) -> HarnessResult {
        HarnessResult {
            harness: Harness {
                file: "a.rs".to_string(),
                name: "h".to_string(),
            },
            status,
            time: "1s".to_string(),
        }
    }

    #[test]
    fn discovery_finds_named_harnesses_past_attributes_and_docs() {
        let source = "#[kani::proof]\nfn one() {}\n\n#[kani::proof]\n#[kani::unwind(4)]\n/// doc\nfn two() {}\nfn not_a_harness() {}\n";
        let found = discover_harnesses(&[("x.rs".to_string(), source.to_string())]);
        let names: Vec<&str> = found.iter().map(|harness| harness.name.as_str()).collect();
        assert_eq!(names, vec!["one", "two"]);
    }

    #[test]
    fn discovery_ignores_harness_text_inside_raw_string_fixtures() {
        let source = "let f = r#\"\n#[kani::proof]\nfn fixture_only() {}\n\"#;\n\n#[kani::proof]\nfn real() {}\n";
        let found = discover_harnesses(&[("x.rs".to_string(), source.to_string())]);
        let names: Vec<&str> = found.iter().map(|harness| harness.name.as_str()).collect();
        assert_eq!(names, vec!["real"]);
    }

    #[test]
    fn classification_requires_an_explicit_success_verdict() {
        let ok = "** 2 of 2 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n";
        assert_eq!(classify_output(ok), HarnessStatus::Verified);
        assert!(matches!(
            classify_output("VERIFICATION:- FAILED"),
            HarnessStatus::Failed(_)
        ));
        // Absence of a verdict is never a pass, even with a zero exit.
        assert!(matches!(
            classify_output("compiled fine"),
            HarnessStatus::NotEstablished(_)
        ));
        assert!(matches!(
            classify_output("No proof harnesses (functions with #[kani::proof]) were found"),
            HarnessStatus::NotEstablished(_)
        ));
    }

    #[test]
    fn a_solver_timeout_is_not_a_counterexample() {
        // Kani reports a timed-out harness as FAILED; it has no verdict.
        let timed_out = "CBMC failed\nVERIFICATION:- FAILED\nCBMC timed out. You may want to rerun your proof with a larger timeout\n";
        assert!(matches!(
            classify_output(timed_out),
            HarnessStatus::NotEstablished(_)
        ));
        // A real counterexample without the timeout text stays FAILED.
        assert!(matches!(
            classify_output("VERIFICATION:- FAILED\n"),
            HarnessStatus::Failed(_)
        ));
    }

    #[test]
    fn the_per_harness_timeout_reaches_the_kani_command() {
        let parsed = parse_args(&strings(&["--harness-timeout-secs", "60"]));
        assert!(parsed.is_ok());
        let command = parsed.map(|args| kani_command(&args)).unwrap_or_default();
        let position = command.iter().position(|arg| arg == "--harness-timeout");
        assert_eq!(
            position
                .and_then(|index| command.get(index + 1))
                .map(String::as_str),
            Some("60s")
        );
        let gate = command.iter().position(|arg| arg == "unstable-options");
        assert!(gate.is_some() && gate < position);
    }

    #[test]
    fn harness_timeout_flag_parses() {
        let parsed = parse_args(&strings(&["--harness-timeout-secs", "60"]));
        assert_eq!(parsed.map(|args| args.harness_timeout_secs), Ok(60));
        assert!(
            parse_args(&strings(&["--harness-timeout-secs", "0"]))
                .err()
                .is_some()
        );
    }

    #[test]
    fn unreachable_cover_makes_the_proof_vacuous() {
        let vacuous =
            "** 1 of 2 cover properties satisfied (1 unreachable)\nVERIFICATION:- SUCCESSFUL\n";
        assert!(matches!(classify_output(vacuous), HarnessStatus::Failed(_)));
        let unsat = "Status: UNSATISFIABLE\nVERIFICATION:- SUCCESSFUL\n";
        assert!(matches!(classify_output(unsat), HarnessStatus::Failed(_)));
    }

    #[test]
    fn lane_never_passes_by_absence() {
        assert!(lane_verdict(&[], false, true).is_err());
        assert!(lane_verdict(&[], true, false).is_err());
        assert_eq!(lane_verdict(&[], true, true).err(), None);
        assert_eq!(
            lane_verdict(&[result(HarnessStatus::Verified)], true, false).err(),
            None
        );
        let failed = result(HarnessStatus::Failed("x".to_string()));
        assert!(lane_verdict(&[failed], true, true).is_err());
        let skipped = result(HarnessStatus::NotEstablished("x".to_string()));
        assert!(lane_verdict(&[skipped], true, true).is_err());
    }

    #[test]
    fn arguments_parse_and_reject_unknown_flags() {
        let parsed = parse_args(&strings(&["--allow-empty", "--default-unwind", "8"]));
        assert_eq!(
            parsed,
            Ok(KaniArgs {
                allow_empty: true,
                harness: None,
                default_unwind: Some(8),
                timeout_secs: DEFAULT_TIMEOUT_SECS,
                harness_timeout_secs: DEFAULT_HARNESS_TIMEOUT_SECS,
            })
        );
        assert!(parse_args(&strings(&["--bogus"])).err().is_some());
        assert!(
            parse_args(&strings(&["--default-unwind", "0"]))
                .err()
                .is_some()
        );
        assert!(parse_args(&strings(&["--harness"])).err().is_some());
    }

    #[test]
    fn blocks_join_to_harnesses_and_a_missing_block_is_not_established() {
        let output = "Checking harness m::one...\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\nChecking harness m::two...\nVERIFICATION:- FAILED\n";
        let harness = |name: &str| Harness {
            file: "f.rs".to_string(),
            name: name.to_string(),
        };
        let harnesses = [harness("one"), harness("two"), harness("three")];
        let results = results_from_output(&harnesses, output);
        let statuses: Vec<&HarnessStatus> = results.iter().map(|r| &r.status).collect();
        assert!(matches!(statuses.first(), Some(HarnessStatus::Verified)));
        assert!(matches!(statuses.get(1), Some(HarnessStatus::Failed(_))));
        assert!(matches!(
            statuses.get(2),
            Some(HarnessStatus::NotEstablished(_))
        ));
        assert_eq!(results.first().map(|r| r.time.as_str()), Some("0.5s"));
    }

    #[test]
    fn report_names_every_status_word() {
        let results = [
            result(HarnessStatus::Verified),
            result(HarnessStatus::Failed("f".to_string())),
            result(HarnessStatus::NotEstablished("n".to_string())),
        ];
        let report = render_report(&results, "");
        for word in ["VERIFIED", "FAILED", "NOT-ESTABLISHED"] {
            assert!(report.contains(word), "{word} missing from {report}");
        }
    }
}
