//! Arg-parsing and dispatch for `ripr gate evaluate` and `ripr gate check`.
//!
//! `gate check` is a composition, not a second policy: it runs the existing
//! `review-comments` producer and hands its guidance document to the same
//! `gate evaluate` path (RIPR-SPEC-0435). This is the CLI adapter layer only. Gate evaluation and report rendering
//! live in `crate::output::gate`. This module owns argv parsing, output
//! destination selection, and exit mapping for the gate command family.

use crate::cli::CommandError;
use crate::cli::commands_options::GateOptions;
use crate::cli::help;
use crate::cli::parse::{expect_value, reconcile_root};
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::output;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use super::{non_empty_path_arg, non_empty_string_arg, write_text_file};
use crate::output::gate::{GATE_STATUS_BLOCKED, GATE_STATUS_CONFIG_ERROR};

/// Map a failing gate-evaluate status onto the process exit contract.
/// The producer tokens live on `output::gate`; `blocked` is a completed
/// blocking decision (exit 3) and every other failing status, including
/// `config_error` and any unknown token, is could-not-complete (exit 2).
/// Callers only pass statuses `gate_decision_should_fail` already accepted.
pub(crate) fn gate_evaluate_exit_error(status: &str, message: String) -> CommandError {
    if status == GATE_STATUS_BLOCKED {
        CommandError::Decision(message)
    } else {
        CommandError::Failure(message)
    }
}

pub(in crate::cli) fn gate(args: &[String]) -> Result<(), CommandError> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_gate_help();
        return Ok(());
    }
    // The sole subcommand is implied when the first token is a flag rather
    // than a subcommand name (#7740):
    // `ripr gate --pr-guidance ...` behaves like `ripr gate evaluate --pr-guidance ...`.
    let (subcommand, rest) = match args.split_first() {
        Some((first, _)) if first.starts_with('-') => ("evaluate", args),
        Some((subcommand, rest)) => (subcommand.as_str(), rest),
        None => {
            help::print_gate_help();
            return Ok(());
        }
    };
    match subcommand {
        "evaluate" => gate_evaluate(rest),
        "check" => gate_check(rest),
        _ => Err(CommandError::from(unknown_value(
            "gate subcommand",
            subcommand,
            GATE_SUBCOMMANDS,
        ))),
    }
}

const GATE_SUBCOMMANDS: &[&str] = &["evaluate", "check"];

fn gate_evaluate(args: &[String]) -> Result<(), CommandError> {
    let options = parse_gate_options(args)?;
    evaluate_and_write(&options)
}

/// Evaluate a parsed gate request, write both reports, and map the decision
/// onto the exit contract. Shared by `gate evaluate` and `gate check` so the
/// two commands cannot drift into separate decision or exit paths.
fn evaluate_and_write(options: &GateOptions) -> Result<(), CommandError> {
    crate::atomic_file::ensure_report_output_contained(&options.input.root)?;
    // Surface the missing-input requirement directly instead of burying it
    // inside the gate-decision.json config_errors array (#2589 review).
    // Only `--pr-guidance` and `--gap-ledger` feed gate candidates.
    // `--baseline` is a comparison input, not a candidate source (#7654);
    // `--repo-exposure` is advisory context the producer never reads for
    // candidates, so a repo-exposure-only run would always end as a
    // config_error after writing reports (#7741). Refuse both up front and
    // name the inputs that actually feed candidates.
    if options.input.pr_guidance.is_none() && options.input.gap_ledger.is_none() {
        return Err(CommandError::from(
            missing_candidate_input_message(
                options.input.baseline.is_some(),
                options.input.repo_exposure.is_some(),
            )
            .to_string(),
        ));
    }
    let report = output::gate::build_gate_decision_report(&options.input)?;
    // Mode disclosure is only meaningful after a completed evaluation (#7447).
    // A config_error report is a written fail-closed artifact, not a mode
    // decision: the run never applied visible-only to findings.
    if let Some(warning) = default_visible_only_warning_for_completed_report(
        options.mode_explicit,
        !std::io::stderr().is_terminal(),
        output::gate::gate_decision_status(&report),
    ) {
        eprintln!("{warning}");
    }
    let rendered_json = output::gate::render_gate_decision_json(&report)?;
    let rendered_md = output::gate::render_gate_decision_markdown(&report);
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    if output::gate::gate_decision_should_fail(&report) {
        let detail = output::gate::gate_decision_inline_detail(&report);
        let message = format!(
            "ripr gate decision is {}{}; see {} for the full report",
            output::gate::gate_decision_status(&report),
            detail,
            options.out.display()
        );
        // A config_error means the evaluation could not complete (exit 2);
        // only a completed evaluation reaching its blocking decision is a
        // Decision (exit 3).
        return Err(gate_evaluate_exit_error(
            output::gate::gate_decision_status(&report),
            message,
        ));
    }
    Ok(())
}

fn parse_gate_options(args: &[String]) -> Result<GateOptions, String> {
    let mut root = PathBuf::from(".");
    let mut root_explicit = false;
    let mut repo_exposure = None;
    let mut pr_guidance = None;
    let mut gap_ledger = None;
    let mut sarif_policy = None;
    let mut labels_json = None;
    let mut labels = Vec::new();
    let mut agent_verify = None;
    let mut agent_receipt = None;
    let mut recommendation_calibration = None;
    let mut mutation_calibration = None;
    let mut baseline = None;
    let mut exception_policy = None;
    let mut acknowledgements = None;
    let mut mode = output::gate::GateMode::VisibleOnly;
    let mut mode_explicit = false;
    let mut acknowledgement_labels = Vec::new();
    let mut out = PathBuf::from(output::gate::DEFAULT_GATE_OUT);
    let mut out_md = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = non_empty_string_arg(args, i, "--root", "gate")?;
                let reconciled = reconcile_root(&root, root_explicit, &value, "gate")?;
                root = reconciled;
                root_explicit = true;
            }
            "--repo-exposure" => {
                i += 1;
                repo_exposure = Some(non_empty_path_arg(args, i, "--repo-exposure", "gate")?);
            }
            "--pr-guidance" => {
                i += 1;
                pr_guidance = Some(non_empty_path_arg(args, i, "--pr-guidance", "gate")?);
            }
            "--gap-ledger" => {
                i += 1;
                gap_ledger = Some(non_empty_path_arg(args, i, "--gap-ledger", "gate")?);
            }
            "--sarif-policy" => {
                i += 1;
                sarif_policy = Some(non_empty_path_arg(args, i, "--sarif-policy", "gate")?);
            }
            "--labels-json" => {
                i += 1;
                labels_json = Some(non_empty_path_arg(args, i, "--labels-json", "gate")?);
            }
            "--label" => {
                i += 1;
                labels.push(non_empty_string_arg(args, i, "--label", "gate")?);
            }
            "--agent-verify" => {
                i += 1;
                agent_verify = Some(non_empty_path_arg(args, i, "--agent-verify", "gate")?);
            }
            "--agent-receipt" => {
                i += 1;
                agent_receipt = Some(non_empty_path_arg(args, i, "--agent-receipt", "gate")?);
            }
            "--recommendation-calibration" => {
                i += 1;
                recommendation_calibration = Some(non_empty_path_arg(
                    args,
                    i,
                    "--recommendation-calibration",
                    "gate",
                )?);
            }
            "--mutation-calibration" => {
                i += 1;
                mutation_calibration = Some(non_empty_path_arg(
                    args,
                    i,
                    "--mutation-calibration",
                    "gate",
                )?);
            }
            "--baseline" => {
                i += 1;
                baseline = Some(non_empty_path_arg(args, i, "--baseline", "gate")?);
            }
            "--exception-policy" => {
                i += 1;
                exception_policy = Some(non_empty_path_arg(args, i, "--exception-policy", "gate")?);
            }
            "--acknowledgements" => {
                i += 1;
                acknowledgements = Some(non_empty_path_arg(args, i, "--acknowledgements", "gate")?);
            }
            "--mode" => {
                i += 1;
                mode_explicit = true;
                mode = output::gate::GateMode::parse(expect_value(args, i, "--mode")?)?;
            }
            "--acknowledgement-label" => {
                i += 1;
                acknowledgement_labels.push(non_empty_string_arg(
                    args,
                    i,
                    "--acknowledgement-label",
                    "gate",
                )?);
            }
            "--out" => {
                i += 1;
                out = non_empty_path_arg(args, i, "--out", "gate")?;
            }
            "--out-md" => {
                i += 1;
                out_md = Some(non_empty_path_arg(args, i, "--out-md", "gate")?);
            }
            other => return Err(unknown_argument("gate", other)),
        }
        i += 1;
    }

    let out_md = out_md.unwrap_or_else(|| output::gate::markdown_path_for(&out));
    // Destinations follow the invoking cwd, independently of --root. Resolve
    // relative/absolute spellings without traversing the filesystem, then use
    // the shared native lexical comparison (no symlink/case-folding guarantee).
    let absolute_out =
        std::path::absolute(&out).map_err(|err| format!("gate could not resolve --out: {err}; pass a path whose directory exists, such as `--out target/ripr/gate.json`"))?;
    let absolute_out_md = std::path::absolute(&out_md)
        .map_err(|err| format!("gate could not resolve --out-md: {err}; pass a path whose directory exists, such as `--out-md target/ripr/gate.md`"))?;
    if output::path::same_output_leaf(&absolute_out, &absolute_out_md) {
        return Err(
            "gate --out and --out-md must not be the same path; choose distinct JSON and Markdown destinations"
                .to_string(),
        );
    }
    require_existing_gate_root(&root)?;
    Ok(GateOptions {
        input: output::gate::GateEvaluateInput {
            root,
            repo_exposure,
            pr_guidance,
            gap_ledger,
            sarif_policy,
            labels_json,
            labels,
            agent_verify,
            agent_receipt,
            recommendation_calibration,
            mutation_calibration,
            baseline,
            mode,
            acknowledgement_labels,
            exception_policy,
            acknowledgements,
        },
        out,
        out_md,
        mode_explicit,
    })
}

/// Default guidance path for `gate check`: the same path `review-comments`
/// writes by default, so the gate report names the document CI already
/// archives.
const DEFAULT_GATE_CHECK_GUIDANCE: &str = "target/ripr/review/comments.json";

/// A parsed `gate check` request: the producer argv for `review-comments` and
/// the fully validated `gate evaluate` options that consume its output.
#[derive(Debug)]
struct GateCheckPlan {
    review_comments_args: Vec<String>,
    guidance_out: PathBuf,
    gate: GateOptions,
}

/// `ripr gate check`: run `review-comments`, then `gate evaluate` over the
/// guidance it wrote, in one process with one exit status.
///
/// Fail-closed contract: a stale guidance document from an earlier run is
/// removed first, so a producer that fails before writing leaves no document
/// and the gate's existing `config_error` path (exit 2) decides. A producer
/// that writes an incomplete or refused document is likewise a
/// `config_error`. No decision is derived here; `evaluate_and_write` owns it.
fn gate_check(args: &[String]) -> Result<(), CommandError> {
    gate_check_with_producer(args, super::review_comments)
}

/// `gate check` over an injectable producer, so tests can prove the
/// fail-closed contract for a producer that writes guidance and then fails.
fn gate_check_with_producer(
    args: &[String],
    produce: impl FnOnce(&[String]) -> Result<(), String>,
) -> Result<(), CommandError> {
    let plan = parse_gate_check_plan(args)?;
    // Refuse escaping output paths (for example a committed `target` symlink)
    // before deleting anything, not only before the producer writes.
    crate::atomic_file::ensure_report_output_contained(&plan.gate.input.root)?;
    crate::atomic_file::ensure_relative_output_contained(&plan.guidance_out)?;
    ensure_guidance_parent_contained(&plan.guidance_out)?;
    remove_stale_guidance(&plan.guidance_out)?;
    if let Err(error) = produce(&plan.review_comments_args) {
        // The producer can fail after it wrote a complete-looking document
        // (for example its deadline or receipt write fails afterwards). An
        // errored run never feeds the gate: drop whatever it left so the
        // evaluator sees a missing input and decides config_error.
        remove_stale_guidance(&plan.guidance_out)?;
        eprintln!(
            "ripr gate check: review-comments could not complete: {error}; the gate fails closed with config_error (exit 2)"
        );
    }
    evaluate_and_write(&plan.gate)
}

/// Refuse destinations that would overwrite each other. review-comments
/// writes the guidance JSON and its `.md` companion (the same leaf when the
/// guidance path itself ends in `.md`), and the gate then writes its two
/// reports; any shared leaf would lose the guidance or feed the gate a
/// Markdown file. Compared lexically on absolute spellings, like
/// `parse_gate_options` compares `--out` and `--out-md`.
fn refuse_overlapping_outputs(guidance_out: &Path, gate: &GateOptions) -> Result<(), String> {
    let absolute = |path: &Path, flag: &str| {
        std::path::absolute(path).map_err(|err| {
            format!(
                "gate check could not resolve {flag}: {err}; pass a path whose directory exists"
            )
        })
    };
    let guidance = absolute(guidance_out, "--guidance-out")?;
    let guidance_md = absolute(
        &super::review_comments::review_comments_markdown_path(guidance_out),
        "--guidance-out",
    )?;
    if output::path::same_output_leaf(&guidance, &guidance_md) {
        return Err(
            "gate check --guidance-out must be a JSON path such as target/ripr/review/comments.json; review-comments writes its Markdown companion next to it with a .md extension"
                .to_string(),
        );
    }
    let out = absolute(&gate.out, "--out")?;
    let out_md = absolute(&gate.out_md, "--out-md")?;
    for produced in [&guidance, &guidance_md] {
        for report in [&out, &out_md] {
            if output::path::same_output_leaf(produced, report) {
                return Err(format!(
                    "gate check --guidance-out and the gate report path {} name the same file; choose distinct guidance and report destinations",
                    output::path::human_path(report)
                ));
            }
        }
    }
    Ok(())
}

/// A relative guidance path is resolved against the invoking directory. Its
/// existing parent must resolve inside that directory, so a committed
/// symlink on the way (for example `out -> /elsewhere`) cannot make gate
/// check delete files outside the workspace. Absolute paths are the caller's
/// explicit choice, as for the shared report guards.
fn ensure_guidance_parent_contained(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        return Ok(());
    }
    let cwd = std::env::current_dir()
        .map_err(|err| format!("gate check cannot resolve the current directory: {err}"))?;
    let Some(parent) = cwd.join(path).parent().map(Path::to_path_buf) else {
        return Ok(());
    };
    // Walk up to the nearest existing ancestor; missing directories are
    // created later by the producer under that ancestor.
    let mut existing = parent.as_path();
    while !existing.exists() {
        match existing.parent() {
            Some(next) => existing = next,
            None => return Ok(()),
        }
    }
    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|err| format!("gate check cannot resolve the current directory: {err}"))?;
    let canonical_parent = existing.canonicalize().map_err(|err| {
        format!(
            "gate check cannot resolve the --guidance-out directory {}: {err}",
            existing.display()
        )
    })?;
    if canonical_parent.starts_with(&canonical_cwd) {
        Ok(())
    } else {
        Err(format!(
            "refusing gate check --guidance-out {}: its directory resolves outside the current directory ({}); pass a path inside the workspace or an explicit absolute path",
            output::path::human_path(path),
            canonical_parent.display()
        ))
    }
}

/// Remove the guidance JSON and its Markdown companion, if present.
fn remove_stale_guidance(path: &Path) -> Result<(), String> {
    for target in [
        path.to_path_buf(),
        super::review_comments::review_comments_markdown_path(path),
    ] {
        match std::fs::remove_file(&target) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "gate check could not remove the previous guidance document {}: {err}; delete it or pass --guidance-out naming a writable path, then rerun",
                    output::path::human_path(&target)
                ));
            }
        }
    }
    Ok(())
}

fn parse_gate_check_plan(args: &[String]) -> Result<GateCheckPlan, String> {
    let mut review_args: Vec<String> = Vec::new();
    let mut gate_args: Vec<String> = Vec::new();
    let mut root_explicit = false;
    let mut base = None;
    let mut head = None;
    let mut mode = None;
    let mut guidance_out = PathBuf::from(DEFAULT_GATE_CHECK_GUIDANCE);

    let mut i = 0usize;
    while i < args.len() {
        let flag = args[i].as_str();
        match flag {
            "--base" => {
                i += 1;
                base = Some(non_empty_string_arg(args, i, flag, "gate check")?);
            }
            "--head" => {
                i += 1;
                head = Some(non_empty_string_arg(args, i, flag, "gate check")?);
            }
            "--timeout-ms" => {
                i += 1;
                let value = non_empty_string_arg(args, i, flag, "gate check")?;
                // Validate here so a bad budget is a usage error before any
                // file is removed, not a config_error after.
                crate::cli::commands_numeric::parse_positive_u64(&value, flag)?;
                review_args.extend([flag.to_string(), value]);
            }
            "--guidance-out" => {
                i += 1;
                guidance_out = non_empty_path_arg(args, i, flag, "gate check")?;
            }
            "--mode" => {
                i += 1;
                let value = expect_value(args, i, flag)?;
                // Validate here so a bad mode fails before analysis starts.
                output::gate::GateMode::parse(value)?;
                mode = Some(value.to_string());
            }
            "--root" => {
                i += 1;
                let value = non_empty_string_arg(args, i, flag, "gate check")?;
                root_explicit = true;
                review_args.extend([flag.to_string(), value.clone()]);
                gate_args.extend([flag.to_string(), value]);
            }
            "--label"
            | "--labels-json"
            | "--acknowledgement-label"
            | "--exception-policy"
            | "--acknowledgements"
            | "--baseline"
            | "--out"
            | "--out-md" => {
                i += 1;
                let value = non_empty_string_arg(args, i, flag, "gate check")?;
                gate_args.extend([flag.to_string(), value]);
            }
            other => return Err(unknown_argument("gate check", other)),
        }
        i += 1;
    }

    let base = base.ok_or_else(|| {
        "gate check requires --base <rev>; pass the pull request's base, for example `ripr gate check --base origin/main`".to_string()
    })?;
    let head = head.unwrap_or_else(|| "HEAD".to_string());
    review_args.extend([
        "--base".to_string(),
        base,
        "--head".to_string(),
        head,
        "--out".to_string(),
        guidance_out.display().to_string(),
    ]);

    // `review-comments --out` follows the invoking directory while gate
    // inputs resolve against `--root`; hand the gate an absolute path when a
    // root is named so both sides name the same file.
    let gate_guidance = if root_explicit || guidance_out.is_absolute() {
        std::path::absolute(&guidance_out).map_err(|err| {
            format!("gate check could not resolve --guidance-out: {err}; pass a path whose directory exists")
        })?
    } else {
        guidance_out.clone()
    };
    gate_args.extend([
        "--pr-guidance".to_string(),
        gate_guidance.display().to_string(),
        "--mode".to_string(),
        mode.unwrap_or_else(|| output::gate::GateMode::Acknowledgeable.as_str().to_string()),
    ]);
    let gate = parse_gate_options(&gate_args)?;
    refuse_overlapping_outputs(&guidance_out, &gate)?;
    Ok(GateCheckPlan {
        review_comments_args: review_args,
        guidance_out,
        gate,
    })
}

fn missing_candidate_input_message(
    baseline_present: bool,
    repo_exposure_present: bool,
) -> &'static str {
    // The named requirement mirrors the producer's candidate-feed check
    // (`output::gate`: only `--pr-guidance` and `--gap-ledger` feed
    // candidates). `--repo-exposure` is advisory context, never a candidate
    // source, so it is acknowledged like `--baseline` instead of satisfying
    // the requirement (#7654, #7741).
    match (baseline_present, repo_exposure_present) {
        (true, true) => {
            "gate evaluate received --baseline and --repo-exposure but neither is a candidate source; gate evaluate requires at least one of --pr-guidance <path> or --gap-ledger <path>; see `ripr gate --help` for all options"
        }
        (true, false) => {
            "gate evaluate received --baseline but it is not a candidate source; gate evaluate requires at least one of --pr-guidance <path> or --gap-ledger <path>; see `ripr gate --help` for all options"
        }
        (false, true) => {
            "gate evaluate received --repo-exposure but it is not a candidate source; gate evaluate requires at least one of --pr-guidance <path> or --gap-ledger <path>; see `ripr gate --help` for all options"
        }
        (false, false) => {
            "gate evaluate requires at least one of --pr-guidance <path> or --gap-ledger <path>; see `ripr gate --help` for all options"
        }
    }
}

fn require_existing_gate_root(root: &Path) -> Result<(), String> {
    if root.is_dir() {
        Ok(())
    } else {
        Err(format!(
            "gate evaluate --root {} is not a directory; pass --root naming an existing repository directory",
            root.display()
        ))
    }
}

const DEFAULT_VISIBLE_ONLY_WARNING: &str = "ripr: gate evaluate is using default mode visible-only; it records advisory evidence and never blocks on findings, but exception-policy violations still can. Pass --mode acknowledgeable, --mode baseline-check, or --mode calibrated-gate to opt into blocking policy.";

fn default_visible_only_warning(
    mode_explicit: bool,
    non_interactive: bool,
) -> Option<&'static str> {
    if non_interactive && !mode_explicit {
        Some(DEFAULT_VISIBLE_ONLY_WARNING)
    } else {
        None
    }
}

fn default_visible_only_warning_for_completed_report(
    mode_explicit: bool,
    non_interactive: bool,
    status: &str,
) -> Option<&'static str> {
    if status == GATE_STATUS_CONFIG_ERROR {
        None
    } else {
        default_visible_only_warning(mode_explicit, non_interactive)
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::tests::{args, unique_command_test_dir, unique_repo_relative_test_dir};
    use super::*;
    use crate::output::gate::{GATE_STATUS_BLOCKED, GATE_STATUS_CONFIG_ERROR};
    use std::path::Path;

    #[test]
    fn gate_evaluate_exit_mapping_matches_the_typed_contract() {
        assert_eq!(
            gate_evaluate_exit_error(GATE_STATUS_CONFIG_ERROR, "config".to_string()).exit_code(),
            crate::cli::EXIT_COULD_NOT_COMPLETE
        );
        assert_eq!(
            gate_evaluate_exit_error(GATE_STATUS_BLOCKED, "blocked".to_string()).exit_code(),
            crate::cli::EXIT_DECISION_OR_REFUSAL
        );
        assert_eq!(
            gate_evaluate_exit_error(GATE_STATUS_CONFIG_ERROR, "config".to_string()),
            CommandError::Failure("config".to_string())
        );
        assert_eq!(
            gate_evaluate_exit_error(GATE_STATUS_BLOCKED, "blocked".to_string()),
            CommandError::Decision("blocked".to_string())
        );
        // Unknown failing statuses are could-not-complete, never a decision.
        assert_eq!(
            gate_evaluate_exit_error("future_unknown_status", "unknown".to_string()),
            CommandError::Failure("unknown".to_string())
        );
        assert_eq!(
            gate_evaluate_exit_error("future_unknown_status", "unknown".to_string()).exit_code(),
            crate::cli::EXIT_COULD_NOT_COMPLETE
        );
    }

    fn assert_gate_root_refusal(
        result: Result<(), CommandError>,
        root: &str,
        out: &Path,
    ) -> Result<(), CommandError> {
        let Err(error) = result else {
            return Err(CommandError::Failure(
                "expected a missing-root refusal, got Ok".to_string(),
            ));
        };
        if error.exit_code() != crate::cli::EXIT_COULD_NOT_COMPLETE {
            return Err(CommandError::Failure(format!(
                "expected exit 2, got {}: {error:?}",
                error.exit_code()
            )));
        }
        let CommandError::Failure(message) = &error else {
            return Err(CommandError::Failure(format!(
                "expected Failure, got {error:?}"
            )));
        };
        if !message.contains(root) {
            return Err(CommandError::Failure(format!(
                "refusal must name the root {root:?}: {message}"
            )));
        }
        if !message.contains("is not a directory") {
            return Err(CommandError::Failure(format!(
                "refusal must say the root is not a directory: {message}"
            )));
        }
        if message.contains("required PR guidance") || message.contains("not found") {
            return Err(CommandError::Failure(format!(
                "refusal must not look like a missing input file: {message}"
            )));
        }
        if out.exists() {
            return Err(CommandError::Failure(format!(
                "missing root must not write {}",
                out.display()
            )));
        }
        Ok(())
    }

    #[test]
    fn gate_parses_full_option_surface() -> Result<(), String> {
        let root = unique_command_test_dir("gate-parse-root");
        std::fs::create_dir_all(&root).map_err(|err| format!("create parse root: {err}"))?;
        let root_arg = root.display().to_string();
        let options = parse_gate_options(&args(&[
            "--root",
            &root_arg,
            "--repo-exposure",
            "target/ripr/reports/repo-exposure.json",
            "--pr-guidance",
            "target/ripr/review/comments.json",
            "--gap-ledger",
            "target/ripr/reports/gap-decision-ledger.json",
            "--sarif-policy",
            "target/ripr/reports/sarif-policy.json",
            "--labels-json",
            "target/ci/labels.json",
            "--label",
            "ripr-waive",
            "--agent-verify",
            "target/ripr/workflow/agent-verify.json",
            "--agent-receipt",
            "target/ripr/reports/agent-receipt.json",
            "--recommendation-calibration",
            "target/ripr/reports/recommendation-calibration.json",
            "--mutation-calibration",
            "target/ripr/reports/mutation-calibration.json",
            "--baseline",
            "target/ripr/reports/gate-baseline.json",
            "--mode",
            "calibrated-gate",
            "--acknowledgement-label",
            "custom-waive",
            "--out",
            "target/ripr/reports/gate-decision.json",
        ]));

        assert_eq!(
            options,
            Ok(GateOptions {
                input: output::gate::GateEvaluateInput {
                    root: root.clone(),
                    repo_exposure: Some(PathBuf::from("target/ripr/reports/repo-exposure.json")),
                    pr_guidance: Some(PathBuf::from("target/ripr/review/comments.json")),
                    gap_ledger: Some(PathBuf::from(
                        "target/ripr/reports/gap-decision-ledger.json"
                    )),
                    sarif_policy: Some(PathBuf::from("target/ripr/reports/sarif-policy.json")),
                    labels_json: Some(PathBuf::from("target/ci/labels.json")),
                    labels: vec!["ripr-waive".to_string()],
                    agent_verify: Some(PathBuf::from("target/ripr/workflow/agent-verify.json")),
                    agent_receipt: Some(PathBuf::from("target/ripr/reports/agent-receipt.json")),
                    recommendation_calibration: Some(PathBuf::from(
                        "target/ripr/reports/recommendation-calibration.json"
                    )),
                    mutation_calibration: Some(PathBuf::from(
                        "target/ripr/reports/mutation-calibration.json"
                    )),
                    baseline: Some(PathBuf::from("target/ripr/reports/gate-baseline.json")),
                    mode: output::gate::GateMode::CalibratedGate,
                    acknowledgement_labels: vec!["custom-waive".to_string()],
                    exception_policy: None,
                    acknowledgements: None,
                },
                out: PathBuf::from("target/ripr/reports/gate-decision.json"),
                out_md: PathBuf::from("target/ripr/reports/gate-decision.md"),
                mode_explicit: true,
            })
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove parse root: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_rejects_bad_surface_and_unknown_args() {
        assert_eq!(gate(&args(&[])), Ok(()));
        assert_eq!(
            gate(&args(&["evaluate"])),
            Err(CommandError::Failure(
                "gate evaluate requires at least one of --pr-guidance <path> or --gap-ledger <path>; see `ripr gate --help` for all options"
                    .to_string()
            ))
        );
        assert_eq!(
            gate(&args(&["inspect"])),
            Err(CommandError::Failure(
                "unknown gate subcommand \"inspect\". Accepted: evaluate, check.".to_string()
            ))
        );
        assert_eq!(
            gate(&args(&["foo"])),
            Err(CommandError::Failure(
                "unknown gate subcommand \"foo\". Accepted: evaluate, check.".to_string()
            ))
        );
        assert_eq!(
            parse_gate_options(&args(&["--mode", "strict"])),
            Err("unknown gate mode \"strict\". Accepted: visible-only, acknowledgeable, baseline-check, calibrated-gate.".to_string())
        );
        assert_eq!(
            parse_gate_options(&args(&["--out", ""])),
            Err("gate --out requires a non-empty value".to_string())
        );
        assert_eq!(
            parse_gate_options(&args(&["--bad"])),
            Err("unknown gate argument \"--bad\". Run `ripr gate --help`.".to_string())
        );
        assert_eq!(
            parse_gate_options(&args(&[])),
            Ok(GateOptions {
                input: output::gate::GateEvaluateInput {
                    root: PathBuf::from("."),
                    repo_exposure: None,
                    pr_guidance: None,
                    gap_ledger: None,
                    sarif_policy: None,
                    labels_json: None,
                    labels: Vec::new(),
                    agent_verify: None,
                    agent_receipt: None,
                    recommendation_calibration: None,
                    mutation_calibration: None,
                    baseline: None,
                    mode: output::gate::GateMode::VisibleOnly,
                    acknowledgement_labels: Vec::new(),
                    exception_policy: None,
                    acknowledgements: None,
                },
                out: PathBuf::from(output::gate::DEFAULT_GATE_OUT),
                out_md: PathBuf::from("target/ripr/reports/gate-decision.md"),
                mode_explicit: false,
            })
        );
    }

    #[test]
    fn unknown_gate_subcommand_suggests_evaluate_for_evalute() {
        assert_eq!(
            gate(&args(&["evalute"])),
            Err(CommandError::Failure(
                "unknown gate subcommand \"evalute\". Did you mean `evaluate`? Accepted: evaluate, check."
                    .to_string()
            ))
        );
    }

    #[test]
    fn gate_leading_flags_dispatch_to_evaluate() -> Result<(), CommandError> {
        // `ripr gate --pr-guidance <path> --mode visible-only` must imply
        // evaluate (#7740), not treat `--pr-guidance` as a subcommand. The
        // missing-guidance config_error path is enough to bind dispatch without
        // depending on a successful producer.
        let dir = unique_command_test_dir("gate-leading-flags");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gate dir: {err}"))?;
        let missing = dir.join("missing-guidance.json");
        let out = dir.join("gate-decision.json");
        let root = dir.display().to_string();
        let missing_arg = missing.display().to_string();
        let out_arg = out.display().to_string();
        let flag_argv = args(&[
            "--pr-guidance",
            &missing_arg,
            "--mode",
            "visible-only",
            "--root",
            &root,
            "--out",
            &out_arg,
        ]);
        let mut evaluate_argv = vec!["evaluate".to_string()];
        evaluate_argv.extend(flag_argv.iter().cloned());
        let flag_result = gate(&flag_argv);
        let evaluate_result = gate(&evaluate_argv);
        assert_ne!(
            flag_result,
            Err(CommandError::Failure(
                "unknown gate subcommand \"--pr-guidance\". Accepted: evaluate, check.".to_string()
            )),
            "leading --pr-guidance must not be a subcommand"
        );
        assert_eq!(
            flag_result, evaluate_result,
            "leading flags must dispatch like an explicit evaluate subcommand"
        );
        let Err(error) = flag_result else {
            return Err(CommandError::Failure(
                "expected a config_error failure, got Ok".to_string(),
            ));
        };
        assert!(
            matches!(&error, CommandError::Failure(message) if message.contains(GATE_STATUS_CONFIG_ERROR)),
            "config_error must carry the Failure variant: {error:?}"
        );
        assert_eq!(error.exit_code(), crate::cli::EXIT_COULD_NOT_COMPLETE);
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove gate dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_evaluate_missing_input_acknowledges_received_baseline() {
        // `--baseline` is parsed, then rejected as a non-candidate source.
        // The error must name that flag instead of listing only the three
        // candidate inputs as if nothing was passed (#7654). Missing-input
        // returns before writing reports, so these cases must not delete
        // shared default paths.
        for argv in [
            args(&[
                "evaluate",
                "--baseline",
                "target/ripr/reports/gate-baseline.json",
            ]),
            args(&[
                "evaluate",
                "--baseline",
                "target/ripr/reports/gate-baseline.json",
                "--mode",
                "baseline-check",
            ]),
        ] {
            let result = gate(&argv);
            assert_eq!(
                result,
                Err(CommandError::Failure(
                    "gate evaluate received --baseline but it is not a candidate source; gate evaluate requires at least one of --pr-guidance <path> or --gap-ledger <path>; see `ripr gate --help` for all options"
                        .to_string()
                )),
                "argv={argv:?}"
            );
        }
    }

    #[test]
    fn gate_evaluate_missing_input_without_baseline_does_not_claim_baseline_was_received() {
        // Discriminator: a message that always mentions a received
        // `--baseline` would fail this `--mode baseline-check` case (#7654).
        let result = gate(&args(&["evaluate", "--mode", "baseline-check"]));
        assert_eq!(
            result,
            Err(CommandError::Failure(
                "gate evaluate requires at least one of --pr-guidance <path> or --gap-ledger <path>; see `ripr gate --help` for all options"
                    .to_string()
            ))
        );
        if let Err(CommandError::Failure(message)) = &result {
            assert!(
                !message.contains("received --baseline"),
                "absent --baseline must not be acknowledged as received: {message}"
            );
        }
    }

    #[test]
    fn gate_evaluate_refuses_repo_exposure_only_up_front() -> Result<(), CommandError> {
        // `--repo-exposure` is advisory context, never a candidate source: the
        // producer only feeds candidates from `--pr-guidance`/`--gap-ledger`,
        // so a repo-exposure-only run always ended as a config_error after
        // writing reports. The argv layer must refuse it up front, name the
        // received flags (including `--baseline` in the combined case), name
        // the inputs that actually feed candidates, and write no reports
        // (#7741).
        let dir = unique_command_test_dir("gate-repo-exposure-only");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gate dir: {err}"))?;
        let exposure = dir.join("repo-exposure.json");
        std::fs::write(&exposure, b"{\"seams\":[]}")
            .map_err(|err| format!("write repo exposure: {err}"))?;
        let out = dir.join("gate-decision.json");
        let out_md = dir.join("gate-decision.md");
        for argv in [
            args(&[
                "evaluate",
                "--root",
                &dir.display().to_string(),
                "--repo-exposure",
                &exposure.display().to_string(),
                "--out",
                &out.display().to_string(),
                "--out-md",
                &out_md.display().to_string(),
            ]),
            args(&[
                "evaluate",
                "--root",
                &dir.display().to_string(),
                "--repo-exposure",
                &exposure.display().to_string(),
                "--baseline",
                "target/ripr/reports/gate-baseline.json",
                "--out",
                &out.display().to_string(),
                "--out-md",
                &out_md.display().to_string(),
            ]),
        ] {
            // Flag-specific received-clause oracle: the combined case must
            // keep naming `--baseline`, so a wording regression that reuses
            // the repo-exposure-only clause there fails this assertion.
            let expected_receipt = if argv.iter().any(|arg| arg == "--baseline") {
                "received --baseline and --repo-exposure but neither is a candidate source"
            } else {
                "received --repo-exposure but it is not a candidate source"
            };
            let result = gate(&argv);
            let Err(CommandError::Failure(message)) = result else {
                return Err(CommandError::Failure(format!(
                    "expected a repo-exposure-only refusal, got {result:?} for argv={argv:?}"
                )));
            };
            assert!(
                message.contains("--repo-exposure") && message.contains("candidate source"),
                "refusal must name --repo-exposure as a non-candidate source: {message}"
            );
            assert!(
                message.contains(expected_receipt),
                "refusal must state the received flags precisely: {message}"
            );
            assert!(
                message.contains("--pr-guidance") && message.contains("--gap-ledger"),
                "refusal must name the inputs that feed candidates: {message}"
            );
            assert!(
                !message.contains(GATE_STATUS_CONFIG_ERROR),
                "up-front refusal must not look like a producer config_error: {message}"
            );
            assert_eq!(
                CommandError::Failure(message.clone()).exit_code(),
                crate::cli::EXIT_COULD_NOT_COMPLETE
            );
            for report in [&out, &out_md] {
                assert!(
                    !report.exists(),
                    "up-front refusal must not write {}",
                    report.display()
                );
            }
        }
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove gate dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_evaluate_pr_guidance_with_repo_exposure_still_evaluates() -> Result<(), CommandError> {
        // Positive control for the #7741 guard: `--repo-exposure` alongside a
        // candidate-feeding input must flow to evaluation, not the up-front
        // refusal. Advisory visible-only decision, reports written.
        let fixture = crate::testing::fixture_workspace::hermetic_gate_fixture_root()?;
        let dir = unique_command_test_dir("gate-guidance-plus-exposure");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gate dir: {err}"))?;
        let exposure = dir.join("repo-exposure.json");
        std::fs::write(&exposure, b"{\"seams\":[]}")
            .map_err(|err| format!("write repo exposure: {err}"))?;
        let out = dir.join("gate-decision.json");
        let out_md = dir.join("gate-decision.md");
        gate(&args(&[
            "evaluate",
            "--root",
            &fixture.path().display().to_string(),
            "--pr-guidance",
            "fixtures/boundary_gap/expected/pr-guidance/exact-line/comments.json",
            "--repo-exposure",
            &exposure.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json_text =
            std::fs::read_to_string(&out).map_err(|err| format!("read gate json: {err}"))?;
        assert!(json_text.contains("\"status\": \"advisory\""));
        assert!(out_md.exists(), "evaluation must write the markdown report");
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove gate dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn default_visible_only_warning_says_findings_never_block_but_exception_policy_still_can() {
        assert!(
            DEFAULT_VISIBLE_ONLY_WARNING.contains("never blocks on findings"),
            "visible-only warning must qualify that findings do not block: {DEFAULT_VISIBLE_ONLY_WARNING}"
        );
        assert!(
            DEFAULT_VISIBLE_ONLY_WARNING.contains("exception-policy violations still can"),
            "visible-only warning must disclose that exception-policy can still block: {DEFAULT_VISIBLE_ONLY_WARNING}"
        );
        assert!(
            !DEFAULT_VISIBLE_ONLY_WARNING.contains("and never blocks."),
            "unqualified 'never blocks' contradicts exception-policy blocking in visible-only: {DEFAULT_VISIBLE_ONLY_WARNING}"
        );
    }

    #[test]
    fn default_visible_only_warning_requires_omitted_mode_and_non_interactive_use() {
        assert_eq!(
            default_visible_only_warning(false, true),
            Some(DEFAULT_VISIBLE_ONLY_WARNING)
        );
        assert_eq!(default_visible_only_warning(false, false), None);
        assert_eq!(default_visible_only_warning(true, true), None);
        assert_eq!(
            default_visible_only_warning_for_completed_report(
                false,
                true,
                GATE_STATUS_CONFIG_ERROR
            ),
            None
        );
        assert_eq!(
            default_visible_only_warning_for_completed_report(false, true, "advisory"),
            Some(DEFAULT_VISIBLE_ONLY_WARNING)
        );
        assert_eq!(
            default_visible_only_warning_for_completed_report(false, true, GATE_STATUS_BLOCKED),
            Some(DEFAULT_VISIBLE_ONLY_WARNING)
        );
        assert_eq!(
            parse_gate_options(&args(&["--label", "--mode"])).map(|options| options.mode_explicit),
            Ok(false)
        );
        assert_eq!(
            parse_gate_options(&args(&["--pr-guidance", "--mode"]))
                .map(|options| options.mode_explicit),
            Ok(false)
        );
    }

    #[test]
    fn gate_command_writes_visible_only_reports() -> Result<(), CommandError> {
        let fixture = crate::testing::fixture_workspace::hermetic_gate_fixture_root()?;
        let dir = unique_command_test_dir("gate-visible");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gate dir: {err}"))?;
        let out = dir.join("gate-decision.json");
        let out_md = dir.join("gate-decision.md");
        gate(&args(&[
            "evaluate",
            "--root",
            &fixture.path().display().to_string(),
            "--pr-guidance",
            "fixtures/boundary_gap/expected/pr-guidance/exact-line/comments.json",
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json_text =
            std::fs::read_to_string(&out).map_err(|err| format!("read gate json: {err}"))?;
        let md_text =
            std::fs::read_to_string(&out_md).map_err(|err| format!("read gate md: {err}"))?;
        assert!(json_text.contains("\"status\": \"advisory\""));
        assert!(json_text.contains("\"mode\": \"visible-only\""));
        assert!(md_text.contains("# RIPR Gate Decision"));
        assert!(md_text.contains("Decision: advisory"));
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove gate dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_command_writes_blocked_report_before_error() -> Result<(), CommandError> {
        let fixture = crate::testing::fixture_workspace::hermetic_gate_fixture_root()?;
        let dir = unique_command_test_dir("gate-blocked");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gate dir: {err}"))?;
        let out = dir.join("gate-decision.json");
        let result = gate(&args(&[
            "evaluate",
            "--root",
            &fixture.path().display().to_string(),
            "--pr-guidance",
            "fixtures/boundary_gap/expected/pr-guidance/exact-line/comments.json",
            "--mode",
            "acknowledgeable",
            "--out",
            &out.display().to_string(),
        ]));

        // A blocked gate decision is a successful evaluation reaching its
        // blocking decision: it maps to the decision exit code 3, not the
        // could-not-complete code 2.
        let Err(decision) = result else {
            return Err(CommandError::Failure(
                "expected a blocked gate decision error, got Ok".to_string(),
            ));
        };
        assert!(
            matches!(&decision, CommandError::Decision(message) if message.contains("blocked")),
            "blocked decision must carry the Decision variant: {decision:?}"
        );
        assert_eq!(decision.exit_code(), 3);
        let json_text =
            std::fs::read_to_string(&out).map_err(|err| format!("read gate json: {err}"))?;
        assert!(json_text.contains(&format!("\"status\": \"{GATE_STATUS_BLOCKED}\"")));
        assert!(json_text.contains("\"decision\": \"blocking\""));
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove gate dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_command_maps_producer_config_error_to_failure() -> Result<(), CommandError> {
        let dir = unique_command_test_dir("gate-config-error");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gate dir: {err}"))?;
        let out = dir.join("gate-decision.json");
        let missing = dir.join("missing-guidance.json");
        let result = gate(&args(&[
            "evaluate",
            "--root",
            &dir.display().to_string(),
            "--pr-guidance",
            &missing.display().to_string(),
            "--out",
            &out.display().to_string(),
        ]));

        let Err(error) = result else {
            return Err(CommandError::Failure(
                "expected a config_error failure, got Ok".to_string(),
            ));
        };
        assert!(
            matches!(&error, CommandError::Failure(message) if message.contains(GATE_STATUS_CONFIG_ERROR)),
            "config_error must carry the Failure variant: {error:?}"
        );
        assert_eq!(error.exit_code(), crate::cli::EXIT_COULD_NOT_COMPLETE);
        let json_text =
            std::fs::read_to_string(&out).map_err(|err| format!("read gate json: {err}"))?;
        assert!(json_text.contains(&format!("\"status\": \"{GATE_STATUS_CONFIG_ERROR}\"")));
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove gate dir: {err}"))?;
        Ok(())
    }

    const CLEAN_PR_GUIDANCE_JSON: &str = r#"{
  "schema_version": "0.1",
  "tool": "ripr",
  "status": "advisory",
  "summary": {"unchanged_tests": true},
  "comments": [],
  "summary_only": [],
  "suppressed": []
}"#;

    #[test]
    fn gate_evaluate_refuses_missing_root() -> Result<(), CommandError> {
        let missing = unique_command_test_dir("gate-missing-root");
        if missing.exists() {
            return Err(CommandError::Failure(format!(
                "precondition: {} must not exist",
                missing.display()
            )));
        }
        let scratch = unique_command_test_dir("gate-missing-root-scratch");
        std::fs::create_dir_all(&scratch)
            .map_err(|err| format!("create missing-root scratch: {err}"))?;
        let guidance = scratch.join("guidance.json");
        std::fs::write(&guidance, CLEAN_PR_GUIDANCE_JSON)
            .map_err(|err| format!("write absolute guidance: {err}"))?;
        let out = scratch.join("gate-decision.json");
        let root = missing.display().to_string();
        let result = gate(&args(&[
            "evaluate",
            "--root",
            &root,
            "--pr-guidance",
            &guidance.display().to_string(),
            "--out",
            &out.display().to_string(),
        ]));
        assert_gate_root_refusal(result, &root, &out)?;
        std::fs::remove_dir_all(&scratch)
            .map_err(|err| format!("remove missing-root scratch: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_evaluate_refuses_file_root() -> Result<(), CommandError> {
        let dir = unique_command_test_dir("gate-file-root");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create file-root dir: {err}"))?;
        let file = dir.join("not-a-directory");
        std::fs::write(&file, b"not a directory")
            .map_err(|err| format!("write file root: {err}"))?;
        let guidance = dir.join("guidance.json");
        std::fs::write(&guidance, CLEAN_PR_GUIDANCE_JSON)
            .map_err(|err| format!("write absolute guidance: {err}"))?;
        let out = dir.join("gate-decision.json");
        let root = file.display().to_string();
        let result = gate(&args(&[
            "evaluate",
            "--root",
            &root,
            "--pr-guidance",
            &guidance.display().to_string(),
            "--out",
            &out.display().to_string(),
        ]));
        assert_gate_root_refusal(result, &root, &out)?;
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove file-root dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_evaluate_refuses_relative_missing_root() -> Result<(), CommandError> {
        let relative = unique_repo_relative_test_dir("gate-rel-missing");
        if relative.exists() {
            return Err(CommandError::Failure(format!(
                "precondition: {} must not exist",
                relative.display()
            )));
        }
        if relative.is_absolute() {
            return Err(CommandError::Failure(format!(
                "precondition: {} must stay relative",
                relative.display()
            )));
        }
        let out = unique_command_test_dir("gate-rel-missing-out").join("gate-decision.json");
        let root = relative.display().to_string();
        let result = gate(&args(&[
            "evaluate",
            "--root",
            &root,
            "--pr-guidance",
            "guidance.json",
            "--out",
            &out.display().to_string(),
        ]));
        assert_gate_root_refusal(result, &root, &out)
    }

    #[cfg(unix)]
    #[test]
    fn gate_evaluate_refuses_a_target_ripr_review_link() -> Result<(), String> {
        let base = unique_command_test_dir("gate-review-link");
        let root = base.join("root");
        let external = base.join("external");
        std::fs::create_dir_all(root.join("target/ripr")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&external).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&external, root.join("target/ripr/review"))
            .map_err(|e| e.to_string())?;
        let result = gate(&args(&["evaluate", "--root", &root.display().to_string()]));
        let leaked = std::fs::read_dir(&external)
            .map_err(|e| e.to_string())?
            .count();
        let _ = std::fs::remove_dir_all(&base);
        let message = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            message.contains("refusing to write ripr output"),
            "{message}"
        );
        assert_eq!(leaked, 0);
        Ok(())
    }

    // ── gate check (RIPR-SPEC-0435) ──────────────────────────────────────────

    #[test]
    fn gate_check_plan_defaults_to_acknowledgeable_over_fresh_guidance() -> Result<(), String> {
        let plan = parse_gate_check_plan(&args(&[
            "--base",
            "origin/main",
            "--label",
            "ripr-waive",
            "--labels-json",
            "target/ci/labels.json",
        ]))?;
        assert_eq!(
            plan.review_comments_args,
            args(&[
                "--base",
                "origin/main",
                "--head",
                "HEAD",
                "--out",
                DEFAULT_GATE_CHECK_GUIDANCE,
            ])
        );
        assert_eq!(
            plan.guidance_out,
            PathBuf::from(DEFAULT_GATE_CHECK_GUIDANCE)
        );
        assert_eq!(
            plan.gate.input.pr_guidance,
            Some(PathBuf::from(DEFAULT_GATE_CHECK_GUIDANCE))
        );
        assert_eq!(
            plan.gate.input.mode,
            output::gate::GateMode::Acknowledgeable
        );
        assert!(plan.gate.mode_explicit, "gate check always names its mode");
        assert_eq!(plan.gate.input.labels, vec!["ripr-waive".to_string()]);
        assert_eq!(
            plan.gate.input.labels_json,
            Some(PathBuf::from("target/ci/labels.json"))
        );
        assert_eq!(plan.gate.input.gap_ledger, None);
        assert_eq!(plan.gate.out, PathBuf::from(output::gate::DEFAULT_GATE_OUT));
        Ok(())
    }

    #[test]
    fn gate_check_plan_routes_producer_and_gate_flags_to_their_owner() -> Result<(), String> {
        let root = unique_command_test_dir("gate-check-plan-root");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        let root_arg = root.display().to_string();
        let plan = parse_gate_check_plan(&args(&[
            "--root",
            &root_arg,
            "--base",
            "abc123",
            "--head",
            "def456",
            "--timeout-ms",
            "5000",
            "--guidance-out",
            "target/gc/comments.json",
            "--mode",
            "baseline-check",
            "--baseline",
            ".ripr/gate-baseline.json",
            "--acknowledgement-label",
            "ripr-accept",
            "--out",
            "target/gc/gate.json",
        ]))?;
        assert_eq!(
            plan.review_comments_args,
            args(&[
                "--root",
                &root_arg,
                "--timeout-ms",
                "5000",
                "--base",
                "abc123",
                "--head",
                "def456",
                "--out",
                "target/gc/comments.json",
            ])
        );
        assert_eq!(plan.gate.input.root, root);
        assert_eq!(plan.gate.input.mode, output::gate::GateMode::BaselineCheck);
        assert_eq!(
            plan.gate.input.baseline,
            Some(PathBuf::from(".ripr/gate-baseline.json"))
        );
        assert_eq!(
            plan.gate.input.acknowledgement_labels,
            vec!["ripr-accept".to_string()]
        );
        // With a named root the gate gets the absolute guidance path, because
        // review-comments writes relative to the invoking directory.
        let expected = std::path::absolute("target/gc/comments.json")
            .map_err(|err| format!("absolute: {err}"))?;
        assert_eq!(plan.gate.input.pr_guidance, Some(expected));
        assert_eq!(plan.gate.out_md, PathBuf::from("target/gc/gate.md"));
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_check_refuses_bad_surface_before_analysis() {
        let missing_base = parse_gate_check_plan(&args(&["--head", "HEAD"]));
        assert!(
            matches!(&missing_base, Err(message) if message.contains("gate check requires --base <rev>")),
            "{missing_base:?}"
        );
        // Candidate inputs belong to gate check itself; a caller cannot point
        // it at a different (possibly stale) guidance document.
        for flag in [
            "--pr-guidance",
            "--gap-ledger",
            "--repo-exposure",
            "--format",
        ] {
            let result = parse_gate_check_plan(&args(&["--base", "main", flag, "x"]));
            assert!(
                matches!(&result, Err(message) if message.contains(flag)),
                "{flag} must be refused: {result:?}"
            );
        }
        let bad_mode = parse_gate_check_plan(&args(&["--base", "main", "--mode", "strict"]));
        assert!(bad_mode.is_err(), "unknown mode must refuse: {bad_mode:?}");
        let empty_base = parse_gate_check_plan(&args(&["--base", " "]));
        assert!(
            empty_base.is_err(),
            "blank base must refuse: {empty_base:?}"
        );
        for budget in ["0", "abc"] {
            let result = parse_gate_check_plan(&args(&["--base", "main", "--timeout-ms", budget]));
            assert!(
                matches!(&result, Err(message) if message.contains("--timeout-ms")),
                "--timeout-ms {budget} must be a usage error before analysis: {result:?}"
            );
        }
        let unknown = gate(&args(&["chekc"]));
        assert!(
            matches!(&unknown, Err(CommandError::Failure(message)) if message.contains("check")),
            "unknown subcommand must suggest check: {unknown:?}"
        );
    }

    #[test]
    fn gate_check_producer_failure_removes_stale_guidance_and_fails_closed()
    -> Result<(), CommandError> {
        // A base that resolves nowhere: review-comments cannot load a diff,
        // whether or not the temp directory sits inside a git checkout. A clean guidance document from an earlier run sits at
        // the guidance path. Control: evaluating that stale document alone
        // passes, so a gate check that reused it would wrongly pass.
        let dir = unique_command_test_dir("gate-check-stale");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        let guidance = dir.join("comments.json");
        let out = dir.join("gate-decision.json");
        std::fs::write(&guidance, CLEAN_PR_GUIDANCE_JSON)
            .map_err(|err| format!("write stale guidance: {err}"))?;
        let dir_arg = dir.display().to_string();
        let guidance_arg = guidance.display().to_string();
        let out_arg = out.display().to_string();
        gate(&args(&[
            "evaluate",
            "--root",
            &dir_arg,
            "--pr-guidance",
            &guidance_arg,
            "--mode",
            "acknowledgeable",
            "--out",
            &out_arg,
        ]))?;

        let result = gate(&args(&[
            "check",
            "--root",
            &dir_arg,
            "--base",
            "ripr-gate-check-no-such-base",
            "--guidance-out",
            &guidance_arg,
            "--out",
            &out_arg,
        ]));
        let Err(error) = result else {
            return Err(CommandError::Failure(
                "gate check must fail closed when review-comments cannot run".to_string(),
            ));
        };
        assert_eq!(error.exit_code(), crate::cli::EXIT_COULD_NOT_COMPLETE);
        assert!(
            matches!(&error, CommandError::Failure(message) if message.contains(GATE_STATUS_CONFIG_ERROR)),
            "{error:?}"
        );
        assert!(
            !guidance.exists(),
            "the stale guidance document must not survive a failed producer run"
        );
        let json_text =
            std::fs::read_to_string(&out).map_err(|err| format!("read gate json: {err}"))?;
        assert!(json_text.contains(&format!("\"status\": \"{GATE_STATUS_CONFIG_ERROR}\"")));
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_check_refuses_overlapping_guidance_and_report_paths() {
        let markdown = parse_gate_check_plan(&args(&[
            "--base",
            "main",
            "--guidance-out",
            "target/gc/guidance.md",
        ]));
        assert!(
            matches!(&markdown, Err(message) if message.contains("must be a JSON path")),
            "a .md guidance path collides with its Markdown companion: {markdown:?}"
        );
        for (guidance, out) in [
            ("target/gc/x.json", "target/gc/x.json"),
            ("target/gc/x.json", "./target/gc/x.json"),
            ("target/gc/x.json", "target/gc/other/../x.json"),
        ] {
            let result = parse_gate_check_plan(&args(&[
                "--base",
                "main",
                "--guidance-out",
                guidance,
                "--out",
                out,
            ]));
            assert!(
                matches!(&result, Err(message) if message.contains("name the same file")),
                "--guidance-out {guidance} with --out {out} must refuse: {result:?}"
            );
        }
        let md_collision = parse_gate_check_plan(&args(&[
            "--base",
            "main",
            "--guidance-out",
            "target/gc/x.json",
            "--out",
            "target/gc/gate.json",
            "--out-md",
            "target/gc/x.md",
        ]));
        assert!(
            matches!(&md_collision, Err(message) if message.contains("name the same file")),
            "{md_collision:?}"
        );
        let distinct = parse_gate_check_plan(&args(&[
            "--base",
            "main",
            "--guidance-out",
            "target/gc/comments.json",
            "--out",
            "target/gc/gate.json",
        ]));
        assert!(distinct.is_ok(), "{distinct:?}");
    }

    #[cfg(unix)]
    #[test]
    fn gate_check_refuses_a_guidance_directory_that_escapes_through_a_symlink() -> Result<(), String>
    {
        // Relative guidance paths resolve against the current directory. A
        // repository-relative link to a directory outside must be refused
        // before anything is deleted.
        let outside = unique_command_test_dir("gate-check-outside");
        std::fs::create_dir_all(&outside).map_err(|err| format!("create outside: {err}"))?;
        let victim = outside.join("comments.json");
        std::fs::write(&victim, "{}").map_err(|err| format!("write victim: {err}"))?;
        // Under `target/` but not `target/ripr/`, so the shared report guard
        // that other tests run concurrently never enumerates this link.
        let link = PathBuf::from("target").join(
            unique_command_test_dir("gate-check-escape-link")
                .file_name()
                .map(std::ffi::OsStr::to_os_string)
                .unwrap_or_default(),
        );
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("create parent: {err}"))?;
        }
        std::os::unix::fs::symlink(&outside, &link).map_err(|err| format!("symlink: {err}"))?;
        let guidance = link.join("comments.json");
        let result = ensure_guidance_parent_contained(&guidance);
        let inside =
            ensure_guidance_parent_contained(Path::new("target/gate-check-new-dir/x.json"));
        std::fs::remove_file(&link).map_err(|err| format!("remove link: {err}"))?;
        assert!(
            matches!(&result, Err(message) if message.contains("resolves outside")),
            "{result:?}"
        );
        assert!(victim.exists(), "the outside file must not be touched");
        assert_eq!(inside, Ok(()));
        std::fs::remove_dir_all(&outside).map_err(|err| format!("remove outside: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_check_producer_that_writes_then_fails_still_fails_closed() -> Result<(), CommandError> {
        // review-comments can write a complete-looking document and only then
        // fail (deadline, Markdown or receipt write). That document passes on
        // its own; gate check must not evaluate it.
        let dir = unique_command_test_dir("gate-check-late-failure");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        let guidance = dir.join("comments.json");
        let guidance_md = dir.join("comments.md");
        let out = dir.join("gate-decision.json");
        let dir_arg = dir.display().to_string();
        let guidance_arg = guidance.display().to_string();
        let out_arg = out.display().to_string();
        let argv = args(&[
            "--root",
            &dir_arg,
            "--base",
            "main",
            "--guidance-out",
            &guidance_arg,
            "--out",
            &out_arg,
        ]);

        let mut produced_args = Vec::new();
        let result = gate_check_with_producer(&argv, |producer_args| {
            produced_args = producer_args.to_vec();
            std::fs::write(&guidance, CLEAN_PR_GUIDANCE_JSON)
                .map_err(|err| format!("write guidance: {err}"))?;
            std::fs::write(&guidance_md, "# guidance\n")
                .map_err(|err| format!("write guidance md: {err}"))?;
            Err("review-comments exceeded --timeout-ms during artifact_io".to_string())
        });
        assert!(
            produced_args
                .windows(2)
                .any(|pair| pair[0] == "--out" && pair[1] == guidance_arg),
            "producer must write the guidance path: {produced_args:?}"
        );
        let Err(error) = result else {
            return Err(CommandError::Failure(
                "a producer error must never let gate check pass".to_string(),
            ));
        };
        assert_eq!(error.exit_code(), crate::cli::EXIT_COULD_NOT_COMPLETE);
        assert!(!guidance.exists() && !guidance_md.exists());

        // Control: the same producer returning Ok lets the gate evaluate its
        // document, so the failure above comes from the producer error alone.
        let ok = gate_check_with_producer(&argv, |_| {
            std::fs::write(&guidance, CLEAN_PR_GUIDANCE_JSON)
                .map_err(|err| format!("write guidance: {err}"))
        });
        assert_eq!(ok, Ok(()));
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn gate_check_blocks_a_weak_boundary_test_and_passes_waived_or_fixed() -> Result<(), String> {
        use crate::testing::fixture_git::{fixture_git_ok, remove_fixture_tree};
        let root = unique_command_test_dir("gate-check-e2e");
        std::fs::create_dir_all(root.join("src")).map_err(|err| format!("create src: {err}"))?;
        fixture_git_ok(&root, &["init", "-q", "--initial-branch=main"])?;
        for (key, value) in [
            ("user.name", "Gate Check"),
            ("user.email", "gate-check@example.com"),
            ("commit.gpgsign", "false"),
        ] {
            fixture_git_ok(&root, &["config", "--local", key, value])?;
        }
        let write = |path: &str, text: &str| {
            std::fs::write(root.join(path), text).map_err(|err| format!("write {path}: {err}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"gatecheck\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        write(
            "src/lib.rs",
            "pub fn is_large(n: u32) -> bool {\n    n > 10\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn large() {\n        assert!(is_large(50));\n    }\n}\n",
        )?;
        fixture_git_ok(&root, &["add", "."])?;
        fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
        write(
            "src/lib.rs",
            "pub fn is_large(n: u32) -> bool {\n    n >= 10\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn large() {\n        assert!(is_large(50));\n    }\n}\n",
        )?;
        fixture_git_ok(&root, &["commit", "-q", "-am", "weak"])?;

        let root_arg = root.display().to_string();
        let guidance_arg = root.join("out/comments.json").display().to_string();
        let out_arg = root.join("out/gate-decision.json").display().to_string();
        std::fs::create_dir_all(root.join("out")).map_err(|err| format!("create out: {err}"))?;
        let run = |extra: &[&str]| {
            let mut argv = vec![
                "check",
                "--root",
                &root_arg,
                "--base",
                "HEAD~1",
                "--guidance-out",
                &guidance_arg,
                "--out",
                &out_arg,
            ];
            argv.extend_from_slice(extra);
            gate(&args(&argv))
        };

        let weak = run(&[]);
        assert!(
            matches!(&weak, Err(CommandError::Decision(message)) if message.contains(GATE_STATUS_BLOCKED)),
            "a weak boundary test must block with exit 3: {weak:?}"
        );
        let waived = run(&["--label", "ripr-waive"]);
        assert_eq!(waived, Ok(()), "the waiver label must acknowledge the gap");
        let advisory = run(&["--mode", "visible-only"]);
        assert_eq!(advisory, Ok(()), "visible-only never blocks on findings");

        write(
            "src/lib.rs",
            "pub fn is_large(n: u32) -> bool {\n    n >= 10\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn large() {\n        assert!(is_large(50));\n        assert!(is_large(10));\n        assert!(!is_large(9));\n    }\n}\n",
        )?;
        fixture_git_ok(&root, &["commit", "-q", "-am", "boundary"])?;
        let fixed = gate(&args(&[
            "check",
            "--root",
            &root_arg,
            "--base",
            "HEAD~2",
            "--guidance-out",
            &guidance_arg,
            "--out",
            &out_arg,
        ]));
        assert_eq!(fixed, Ok(()), "a boundary-discriminating test must pass");
        remove_fixture_tree(&root)
    }
}
