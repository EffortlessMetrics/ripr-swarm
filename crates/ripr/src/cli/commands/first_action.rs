//! CLI adapter for `ripr first-action` (#4854 extraction).

use crate::cli::commands_options::*;
use crate::cli::commands_timestamps::generated_at_unix_ms;
use crate::cli::help;
use crate::cli::suggest::unknown_argument;
use crate::output;
use std::path::PathBuf;

use super::{emit_report_warnings, read_optional_text_for_report, write_text_file};
use super::{non_empty_path_arg, non_empty_string_arg};

pub(in crate::cli) fn first_action(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_first_action_help();
        return Ok(());
    }

    let options = parse_first_action_options(args)?;
    let pr_guidance_path = options
        .pr_guidance
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let assistant_proof_path = options
        .assistant_proof
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let gap_ledger_path = options
        .gap_ledger
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let ledger_path = options
        .ledger
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let baseline_delta_path = options
        .baseline_delta
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let receipt_path = options
        .receipt
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let gate_decision_path = options
        .gate_decision
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let coverage_frontier_path = options
        .coverage_frontier
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let editor_context_path = options
        .editor_context
        .as_ref()
        .map(|path| output::first_useful_action::display_path(path));
    let input = output::first_useful_action::FirstUsefulActionInput {
        root: options.root,
        generated_at: first_action_generated_at()?,
        pr_guidance_path,
        assistant_proof_path,
        gap_ledger_path,
        ledger_path,
        baseline_delta_path,
        receipt_path,
        gate_decision_path,
        coverage_frontier_path,
        editor_context_path,
        pr_guidance_json: options
            .pr_guidance
            .as_ref()
            .map(|path| read_optional_text_for_report("PR guidance", path)),
        assistant_proof_json: options
            .assistant_proof
            .as_ref()
            .map(|path| read_optional_text_for_report("assistant proof", path)),
        gap_ledger_json: options
            .gap_ledger
            .as_ref()
            .map(|path| read_optional_text_for_report("gap decision ledger", path)),
        ledger_json: options
            .ledger
            .as_ref()
            .map(|path| read_optional_text_for_report("PR evidence ledger", path)),
        baseline_delta_json: options
            .baseline_delta
            .as_ref()
            .map(|path| read_optional_text_for_report("baseline debt delta", path)),
        receipt_json: options
            .receipt
            .as_ref()
            .map(|path| read_optional_text_for_report("receipt", path)),
        gate_decision_json: options
            .gate_decision
            .as_ref()
            .map(|path| read_optional_text_for_report("gate decision", path)),
        coverage_frontier_json: options
            .coverage_frontier
            .as_ref()
            .map(|path| read_optional_text_for_report("coverage/grip frontier", path)),
        editor_context_json: options
            .editor_context
            .as_ref()
            .map(|path| read_optional_text_for_report("editor context", path)),
    };
    let report = output::first_useful_action::build_first_useful_action_report(input);
    let rendered_json = output::first_useful_action::render_first_useful_action_json(&report)?;
    let rendered_md = output::first_useful_action::render_first_useful_action_markdown(&report);
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    // Missing-input warnings ride stderr like every other advisory surface
    // (#9856): a run that routed from nothing must not look like a healthy
    // run in the terminal or in `2>&1` CI logs.
    emit_report_warnings(output::first_useful_action::first_useful_action_warnings(
        &report,
    ));
    Ok(())
}

fn parse_first_action_options(args: &[String]) -> Result<FirstActionOptions, String> {
    let mut root = ".".to_string();
    let mut pr_guidance = None;
    let mut assistant_proof = None;
    let mut gap_ledger = None;
    let mut ledger = None;
    let mut baseline_delta = None;
    let mut receipt = None;
    let mut gate_decision = None;
    let mut coverage_frontier = None;
    let mut editor_context = None;
    let mut out = PathBuf::from(output::first_useful_action::DEFAULT_FIRST_USEFUL_ACTION_OUT);
    let mut out_md = PathBuf::from(output::first_useful_action::DEFAULT_FIRST_USEFUL_ACTION_MD_OUT);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                root = non_empty_string_arg(args, i, "--root", "first-action")?;
            }
            "--pr-guidance" => {
                i += 1;
                pr_guidance = Some(non_empty_path_arg(
                    args,
                    i,
                    "--pr-guidance",
                    "first-action",
                )?);
            }
            "--assistant-proof" => {
                i += 1;
                assistant_proof = Some(non_empty_path_arg(
                    args,
                    i,
                    "--assistant-proof",
                    "first-action",
                )?);
            }
            "--gap-ledger" => {
                i += 1;
                gap_ledger = Some(non_empty_path_arg(args, i, "--gap-ledger", "first-action")?);
            }
            "--ledger" => {
                i += 1;
                ledger = Some(non_empty_path_arg(args, i, "--ledger", "first-action")?);
            }
            "--baseline-delta" => {
                i += 1;
                baseline_delta = Some(non_empty_path_arg(
                    args,
                    i,
                    "--baseline-delta",
                    "first-action",
                )?);
            }
            "--receipt" => {
                i += 1;
                receipt = Some(non_empty_path_arg(args, i, "--receipt", "first-action")?);
            }
            "--gate-decision" => {
                i += 1;
                gate_decision = Some(non_empty_path_arg(
                    args,
                    i,
                    "--gate-decision",
                    "first-action",
                )?);
            }
            "--coverage-frontier" => {
                i += 1;
                coverage_frontier = Some(non_empty_path_arg(
                    args,
                    i,
                    "--coverage-frontier",
                    "first-action",
                )?);
            }
            "--editor-context" => {
                i += 1;
                editor_context = Some(non_empty_path_arg(
                    args,
                    i,
                    "--editor-context",
                    "first-action",
                )?);
            }
            "--out" => {
                i += 1;
                out = non_empty_path_arg(args, i, "--out", "first-action")?;
            }
            "--out-md" => {
                i += 1;
                out_md = non_empty_path_arg(args, i, "--out-md", "first-action")?;
            }
            other => return Err(unknown_argument("first-action", other)),
        }
        i += 1;
    }

    if pr_guidance.is_none()
        && assistant_proof.is_none()
        && gap_ledger.is_none()
        && ledger.is_none()
        && baseline_delta.is_none()
        && receipt.is_none()
        && gate_decision.is_none()
        && coverage_frontier.is_none()
        && editor_context.is_none()
    {
        return Err("first-action requires at least one explicit artifact input".to_string());
    }

    Ok(FirstActionOptions {
        root,
        pr_guidance,
        assistant_proof,
        gap_ledger,
        ledger,
        baseline_delta,
        receipt,
        gate_decision,
        coverage_frontier,
        editor_context,
        out,
        out_md,
    })
}

fn first_action_generated_at() -> Result<String, String> {
    generated_at_unix_ms()
}

#[cfg(test)]
mod tests {
    use super::super::tests::{args, unique_command_test_dir};
    use super::first_action;
    use crate::output;
    use std::path::PathBuf;

    #[test]
    fn first_action_parses_option_surface() {
        assert_eq!(
            super::parse_first_action_options(&args(&[
                "--root",
                ".",
                "--pr-guidance",
                "target/ripr/review/comments.json",
                "--assistant-proof",
                "target/ripr/reports/test-oracle-assistant-proof.json",
                "--gap-ledger",
                "target/ripr/reports/gap-decision-ledger.json",
                "--ledger",
                "target/ripr/reports/pr-evidence-ledger.json",
                "--baseline-delta",
                "target/ripr/reports/baseline-debt-delta.json",
                "--receipt",
                "target/ripr/reports/agent-receipt.json",
                "--gate-decision",
                "target/ripr/reports/gate-decision.json",
                "--coverage-frontier",
                "target/ripr/reports/coverage-grip-frontier.json",
                "--editor-context",
                "target/ripr/workflow/evidence-context.json",
                "--out",
                "target/ripr/reports/first-useful-action.json",
                "--out-md",
                "target/ripr/reports/first-useful-action.md",
            ])),
            Ok(super::FirstActionOptions {
                root: ".".to_string(),
                pr_guidance: Some(PathBuf::from("target/ripr/review/comments.json")),
                assistant_proof: Some(PathBuf::from(
                    "target/ripr/reports/test-oracle-assistant-proof.json",
                )),
                gap_ledger: Some(PathBuf::from(
                    "target/ripr/reports/gap-decision-ledger.json"
                )),
                ledger: Some(PathBuf::from("target/ripr/reports/pr-evidence-ledger.json")),
                baseline_delta: Some(PathBuf::from(
                    "target/ripr/reports/baseline-debt-delta.json",
                )),
                receipt: Some(PathBuf::from("target/ripr/reports/agent-receipt.json")),
                gate_decision: Some(PathBuf::from("target/ripr/reports/gate-decision.json")),
                coverage_frontier: Some(PathBuf::from(
                    "target/ripr/reports/coverage-grip-frontier.json",
                )),
                editor_context: Some(PathBuf::from("target/ripr/workflow/evidence-context.json")),
                out: PathBuf::from("target/ripr/reports/first-useful-action.json"),
                out_md: PathBuf::from("target/ripr/reports/first-useful-action.md"),
            })
        );
    }

    #[test]
    fn first_action_requires_input_and_rejects_unknown_args() {
        assert_eq!(
            super::parse_first_action_options(&args(&[])),
            Err("first-action requires at least one explicit artifact input".to_string())
        );
        assert_eq!(
            super::parse_first_action_options(&args(&["--pr-guidance", ""])),
            Err("first-action --pr-guidance requires a non-empty value".to_string())
        );
        assert_eq!(
            super::parse_first_action_options(&args(&["--bad"])),
            Err(
                "unknown first-action argument \"--bad\". Run `ripr first-action --help`."
                    .to_string()
            )
        );
    }

    #[test]
    fn first_action_cli_writes_gap_record_report() -> Result<(), String> {
        let dir = unique_command_test_dir("first-action-gap-record");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("create first-action gap-record dir: {err}"))?;
        let gap_ledger = dir.join("gap-decision-ledger.json");
        let out = dir.join("first-useful-action.json");
        let out_md = dir.join("first-useful-action.md");
        std::fs::write(
            &gap_ledger,
            r#"{
  "kind": "gap_decision_ledger",
  "records": [
    {
      "gap_id": "gap:pr:pricing:threshold-boundary",
      "source_currentness": "candidate_current",
      "canonical_gap_id": "gap:rust:pricing:discount:threshold-boundary",
      "kind": "MissingBoundaryAssertion",
      "language": "rust",
      "language_status": "stable",
      "scope": "pr_local",
      "evidence_class": "predicate_boundary",
      "gap_state": "actionable",
      "policy_state": "new",
      "repairability": "repairable",
      "anchor": {
        "file": "src/pricing.rs",
        "line": 42,
        "dedupe_fingerprint": "gap:rust:pricing:discount:threshold-boundary"
      },
      "repair_route": {
        "route_kind": "AddBoundaryAssertion",
        "target_file": "tests/pricing.rs",
        "assertion_shape": "assert_eq!(discount(100, 100), 90)"
      },
      "verification_commands": [
        "cargo xtask fixtures boundary_gap"
      ]
    }
  ]
}"#,
        )
        .map_err(|err| format!("write gap decision ledger: {err}"))?;

        super::first_action(&args(&[
            "--gap-ledger",
            &gap_ledger.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json = std::fs::read_to_string(&out)
            .map_err(|err| format!("read first-action JSON: {err}"))?;
        assert!(json.contains(r#""source": "gap_ledger""#));
        assert!(json.contains(r#""repair_route": "AddBoundaryAssertion""#));
        let markdown = std::fs::read_to_string(&out_md)
            .map_err(|err| format!("read first-action Markdown: {err}"))?;
        assert!(markdown.contains("Repair MissingBoundaryAssertion via AddBoundaryAssertion"));

        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("remove first-action gap-record dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn first_action_missing_input_completes_with_warnings() -> Result<(), String> {
        // A missing required input must keep the documented exit-0 contract
        // (docs/EXIT_CODES.md: the command completed its typed answer) while
        // the warning that was previously report-only becomes visible: the
        // report carries it and the CLI echoes the same text to stderr
        // (#9856).
        let dir = unique_command_test_dir("first-action-missing-input");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("create first-action missing-input dir: {err}"))?;
        let missing = dir.join("missing-pr-guidance.json");
        let out = dir.join("first-useful-action.json");
        let out_md = dir.join("first-useful-action.md");
        super::first_action(&args(&[
            "--pr-guidance",
            &missing.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))
        .map_err(|err| format!("missing-input first-action must complete (exit 0): {err}"))?;
        let json = std::fs::read_to_string(&out)
            .map_err(|err| format!("read first-action JSON: {err}"))?;
        let value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|err| format!("first-action JSON must parse: {err}"))?;
        let warnings = value
            .get("warnings")
            .and_then(serde_json::Value::as_array)
            .ok_or("first-action JSON must carry a warnings array")?;
        assert!(!warnings.is_empty(), "warnings must not be empty: {json}");
        let missing_display = output::first_useful_action::display_path(&missing);
        assert!(
            warnings.iter().any(|warning| warning
                .as_str()
                .is_some_and(|text| text.contains(&missing_display))),
            "warnings must name the missing input {missing_display}: {json}"
        );
        // The stderr surface renders the same warnings: non-empty and naming
        // the missing input.
        let texts: Vec<String> = warnings
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect();
        let rendered = super::super::render_report_warnings(&texts);
        assert!(!rendered.is_empty(), "stderr warnings must be non-empty");
        assert!(rendered.contains(&missing_display), "{rendered}");
        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("remove first-action missing-input dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn first_action_dispatch_wiring_requires_input() {
        assert_eq!(
            first_action(&args(&[])),
            Err("first-action requires at least one explicit artifact input".to_string())
        );
    }
}
