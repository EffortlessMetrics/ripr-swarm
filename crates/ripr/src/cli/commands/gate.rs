//! Arg-parsing and dispatch for `ripr gate evaluate`.
//!
//! This is the CLI adapter layer only. Gate evaluation and report rendering
//! live in `crate::output::gate`. This module owns argv parsing, output
//! destination selection, and exit mapping for the gate command family.

use crate::cli::CommandError;
use crate::cli::commands_options::GateOptions;
use crate::cli::help;
use crate::cli::parse::expect_value;
use crate::cli::suggest::unknown_argument;
use crate::output;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use super::{non_empty_path_arg, non_empty_string_arg, write_text_file};
use crate::output::gate::GATE_STATUS_CONFIG_ERROR;

/// Map a failing gate-evaluate status onto the process exit contract.
/// The producer tokens live on `output::gate`; `config_error` is
/// could-not-complete and `blocked` is a completed blocking decision.
/// Callers only pass statuses `gate_decision_should_fail` already accepted.
pub(crate) fn gate_evaluate_exit_error(status: &str, message: String) -> CommandError {
    if status == GATE_STATUS_CONFIG_ERROR {
        CommandError::Failure(message)
    } else {
        CommandError::Decision(message)
    }
}

pub(in crate::cli) fn gate(args: &[String]) -> Result<(), CommandError> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_gate_help();
        return Ok(());
    }
    // The sole subcommand is implied when the command runs bare, or when the
    // first token is a flag rather than a subcommand name (#2013, #7740):
    // `ripr gate --pr-guidance ...` behaves like `ripr gate evaluate --pr-guidance ...`.
    let (subcommand, rest) = match args.split_first() {
        Some((first, _)) if first.starts_with('-') => ("evaluate", args),
        Some((subcommand, rest)) => (subcommand.as_str(), rest),
        None => ("evaluate", &[][..]),
    };
    if subcommand != "evaluate" {
        return Err(CommandError::from(format!(
            "unknown gate subcommand {subcommand:?}; expected `evaluate`"
        )));
    }

    let options = parse_gate_options(rest)?;
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
    println!("Wrote {}", options.out.display());
    println!("Wrote {}", options.out_md.display());
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
                root = non_empty_path_arg(args, i, "--root", "gate")?;
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
        },
        out,
        out_md,
        mode_explicit,
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
        // Bare `ripr gate` is the `evaluate` alias (#2013): it dispatches
        // instead of erroring on a missing subcommand. The full path writes
        // the default report files, so they are cleaned on both sides.
        for residue in [
            "target/ripr/reports/gate-decision.json",
            "target/ripr/reports/gate-decision.md",
        ] {
            let _ = std::fs::remove_file(residue);
        }
        let bare_result = gate(&args(&[]));
        for residue in [
            "target/ripr/reports/gate-decision.json",
            "target/ripr/reports/gate-decision.md",
        ] {
            let _ = std::fs::remove_file(residue);
        }
        assert_eq!(
            bare_result,
            Err(CommandError::Failure(
                "gate evaluate requires at least one of --pr-guidance <path> or --gap-ledger <path>; see `ripr gate --help` for all options"
                    .to_string()
            ))
        );
        assert_eq!(
            gate(&args(&["inspect"])),
            Err(CommandError::Failure(
                "unknown gate subcommand \"inspect\"; expected `evaluate`".to_string()
            ))
        );
        assert_eq!(
            gate(&args(&["foo"])),
            Err(CommandError::Failure(
                "unknown gate subcommand \"foo\"; expected `evaluate`".to_string()
            ))
        );
        assert_eq!(
            parse_gate_options(&args(&["--mode", "strict"])),
            Err("unknown gate mode `strict`; expected `visible-only`, `acknowledgeable`, `baseline-check`, or `calibrated-gate`".to_string())
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
                },
                out: PathBuf::from(output::gate::DEFAULT_GATE_OUT),
                out_md: PathBuf::from("target/ripr/reports/gate-decision.md"),
                mode_explicit: false,
            })
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
                "unknown gate subcommand \"--pr-guidance\"; expected `evaluate`".to_string()
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
            &crate::testing::fixture_workspace::hermetic_gate_fixture_root()?
                .display()
                .to_string(),
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
        let dir = unique_command_test_dir("gate-visible");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gate dir: {err}"))?;
        let out = dir.join("gate-decision.json");
        let out_md = dir.join("gate-decision.md");
        gate(&args(&[
            "evaluate",
            "--root",
            &crate::testing::fixture_workspace::hermetic_gate_fixture_root()?
                .display()
                .to_string(),
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
        let dir = unique_command_test_dir("gate-blocked");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gate dir: {err}"))?;
        let out = dir.join("gate-decision.json");
        let result = gate(&args(&[
            "evaluate",
            "--root",
            &crate::testing::fixture_workspace::hermetic_gate_fixture_root()?
                .display()
                .to_string(),
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
}
