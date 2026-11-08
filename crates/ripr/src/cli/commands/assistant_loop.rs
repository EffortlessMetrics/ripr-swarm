//! CLI adapter for `ripr assistant-loop` (#4854 extraction).

use crate::cli::commands_options::*;
use crate::cli::commands_timestamps::generated_at_unix_ms;
use crate::cli::help;
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::output;
use std::path::PathBuf;

use super::{non_empty_path_arg, non_empty_string_arg};
use super::{read_optional_text_for_report, write_text_file};

pub(in crate::cli) fn assistant_loop(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_assistant_loop_help();
        return Ok(());
    }
    let Some((subcommand, rest)) = args.split_first() else {
        help::print_assistant_loop_help();
        return Ok(());
    };
    match subcommand.as_str() {
        "proof" => assistant_loop_proof(rest),
        "health" => assistant_loop_health(rest),
        _ => Err(unknown_value(
            "assistant-loop subcommand",
            subcommand,
            &["proof", "health"],
        )),
    }
}

fn assistant_loop_proof(args: &[String]) -> Result<(), String> {
    let options = parse_assistant_loop_proof_options(args)?;
    let pr_guidance_path = options
        .pr_guidance
        .as_ref()
        .map(|path| output::test_oracle_assistant_proof::display_path(path));
    let agent_packet_path = options
        .agent_packet
        .as_ref()
        .map(|path| output::test_oracle_assistant_proof::display_path(path));
    let before_path = options
        .before
        .as_ref()
        .map(|path| output::test_oracle_assistant_proof::display_path(path));
    let after_path = options
        .after
        .as_ref()
        .map(|path| output::test_oracle_assistant_proof::display_path(path));
    let receipt_path = options
        .receipt
        .as_ref()
        .map(|path| output::test_oracle_assistant_proof::display_path(path));
    let ledger_path = options
        .ledger
        .as_ref()
        .map(|path| output::test_oracle_assistant_proof::display_path(path));
    let coverage_frontier_path = options
        .coverage_frontier
        .as_ref()
        .map(|path| output::test_oracle_assistant_proof::display_path(path));
    let gate_decision_path = options
        .gate_decision
        .as_ref()
        .map(|path| output::test_oracle_assistant_proof::display_path(path));
    let input = output::test_oracle_assistant_proof::TestOracleAssistantProofInput {
        root: options.root,
        pr_guidance_path,
        agent_packet_path,
        before_path,
        after_path,
        receipt_path,
        ledger_path,
        coverage_frontier_path,
        gate_decision_path,
        pr_guidance_json: options
            .pr_guidance
            .as_ref()
            .map(|path| read_optional_text_for_report("PR guidance", path)),
        agent_packet_json: options
            .agent_packet
            .as_ref()
            .map(|path| read_optional_text_for_report("agent packet", path)),
        before_json: options
            .before
            .as_ref()
            .map(|path| read_optional_text_for_report("before evidence", path)),
        after_json: options
            .after
            .as_ref()
            .map(|path| read_optional_text_for_report("after evidence", path)),
        receipt_json: options
            .receipt
            .as_ref()
            .map(|path| read_optional_text_for_report("receipt", path)),
        ledger_json: options
            .ledger
            .as_ref()
            .map(|path| read_optional_text_for_report("PR evidence ledger", path)),
        coverage_frontier_json: options
            .coverage_frontier
            .as_ref()
            .map(|path| read_optional_text_for_report("coverage/grip frontier", path)),
        gate_decision_json: options
            .gate_decision
            .as_ref()
            .map(|path| read_optional_text_for_report("gate decision", path)),
    };
    let report =
        output::test_oracle_assistant_proof::build_test_oracle_assistant_proof_report(input);
    let rendered_json =
        output::test_oracle_assistant_proof::render_test_oracle_assistant_proof_json(&report)?;
    let rendered_md =
        output::test_oracle_assistant_proof::render_test_oracle_assistant_proof_markdown(&report);
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    Ok(())
}

fn assistant_loop_health(args: &[String]) -> Result<(), String> {
    let options = parse_assistant_loop_health_options(args)?;
    let proofs = options
        .proofs
        .iter()
        .map(|path| {
            let source_artifact = output::assistant_loop_health::display_path(path);
            let proof_json = read_optional_text_for_report("assistant proof", path);
            output::assistant_loop_health::AssistantLoopHealthProofInput {
                source_artifact,
                proof_json,
            }
        })
        .collect::<Vec<_>>();
    let report = output::assistant_loop_health::build_assistant_loop_health_report(
        output::assistant_loop_health::AssistantLoopHealthInput {
            root: options.root,
            generated_at: assistant_loop_health_generated_at()?,
            proofs,
        },
    );
    let rendered_json = output::assistant_loop_health::render_assistant_loop_health_json(&report)?;
    let rendered_md = output::assistant_loop_health::render_assistant_loop_health_markdown(&report);
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    println!(
        "Proofs: {}",
        output::assistant_loop_health::assistant_loop_health_proof_count(&report)
    );
    Ok(())
}

fn parse_assistant_loop_proof_options(
    args: &[String],
) -> Result<AssistantLoopProofOptions, String> {
    let mut root = ".".to_string();
    let mut pr_guidance = None;
    let mut agent_packet = None;
    let mut before = None;
    let mut after = None;
    let mut receipt = None;
    let mut ledger = None;
    let mut coverage_frontier = None;
    let mut gate_decision = None;
    let mut out =
        PathBuf::from(output::test_oracle_assistant_proof::DEFAULT_TEST_ORACLE_ASSISTANT_PROOF_OUT);
    let mut out_md = PathBuf::from(
        output::test_oracle_assistant_proof::DEFAULT_TEST_ORACLE_ASSISTANT_PROOF_MD_OUT,
    );

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                root = non_empty_string_arg(args, i, "--root", "assistant-loop proof")?;
            }
            "--pr-guidance" => {
                i += 1;
                pr_guidance = Some(non_empty_path_arg(
                    args,
                    i,
                    "--pr-guidance",
                    "assistant-loop proof",
                )?);
            }
            "--agent-packet" => {
                i += 1;
                agent_packet = Some(non_empty_path_arg(
                    args,
                    i,
                    "--agent-packet",
                    "assistant-loop proof",
                )?);
            }
            "--before" => {
                i += 1;
                before = Some(non_empty_path_arg(
                    args,
                    i,
                    "--before",
                    "assistant-loop proof",
                )?);
            }
            "--after" => {
                i += 1;
                after = Some(non_empty_path_arg(
                    args,
                    i,
                    "--after",
                    "assistant-loop proof",
                )?);
            }
            "--receipt" => {
                i += 1;
                receipt = Some(non_empty_path_arg(
                    args,
                    i,
                    "--receipt",
                    "assistant-loop proof",
                )?);
            }
            "--ledger" => {
                i += 1;
                ledger = Some(non_empty_path_arg(
                    args,
                    i,
                    "--ledger",
                    "assistant-loop proof",
                )?);
            }
            "--coverage-frontier" => {
                i += 1;
                coverage_frontier = Some(non_empty_path_arg(
                    args,
                    i,
                    "--coverage-frontier",
                    "assistant-loop proof",
                )?);
            }
            "--gate-decision" => {
                i += 1;
                gate_decision = Some(non_empty_path_arg(
                    args,
                    i,
                    "--gate-decision",
                    "assistant-loop proof",
                )?);
            }
            "--out" => {
                i += 1;
                out = non_empty_path_arg(args, i, "--out", "assistant-loop proof")?;
            }
            "--out-md" => {
                i += 1;
                out_md = non_empty_path_arg(args, i, "--out-md", "assistant-loop proof")?;
            }
            other => return Err(unknown_argument("assistant-loop proof", other)),
        }
        i += 1;
    }

    if pr_guidance.is_none()
        && agent_packet.is_none()
        && before.is_none()
        && after.is_none()
        && receipt.is_none()
        && ledger.is_none()
    {
        return Err(
            "assistant-loop proof requires at least one explicit artifact input".to_string(),
        );
    }

    Ok(AssistantLoopProofOptions {
        root,
        pr_guidance,
        agent_packet,
        before,
        after,
        receipt,
        ledger,
        coverage_frontier,
        gate_decision,
        out,
        out_md,
    })
}

fn parse_assistant_loop_health_options(
    args: &[String],
) -> Result<AssistantLoopHealthOptions, String> {
    let mut root = ".".to_string();
    let mut proofs = Vec::new();
    let mut out = PathBuf::from(output::assistant_loop_health::DEFAULT_ASSISTANT_LOOP_HEALTH_OUT);
    let mut out_md =
        PathBuf::from(output::assistant_loop_health::DEFAULT_ASSISTANT_LOOP_HEALTH_MD_OUT);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                root = non_empty_string_arg(args, i, "--root", "assistant-loop health")?;
            }
            "--proof" => {
                i += 1;
                proofs.push(non_empty_path_arg(
                    args,
                    i,
                    "--proof",
                    "assistant-loop health",
                )?);
            }
            "--out" => {
                i += 1;
                out = non_empty_path_arg(args, i, "--out", "assistant-loop health")?;
            }
            "--out-md" => {
                i += 1;
                out_md = non_empty_path_arg(args, i, "--out-md", "assistant-loop health")?;
            }
            other => return Err(unknown_argument("assistant-loop health", other)),
        }
        i += 1;
    }

    if proofs.is_empty() {
        return Err("assistant-loop health requires at least one --proof path".to_string());
    }

    Ok(AssistantLoopHealthOptions {
        root,
        proofs,
        out,
        out_md,
    })
}

fn assistant_loop_health_generated_at() -> Result<String, String> {
    generated_at_unix_ms()
}

#[cfg(test)]
mod tests {
    use super::super::tests::args;
    use super::assistant_loop;

    #[test]
    fn assistant_loop_dispatch_wiring_help_exits_cleanly() {
        assert_eq!(assistant_loop(&args(&["--help"])), Ok(()));
        assert_eq!(assistant_loop(&args(&[])), Ok(()));
    }
}
