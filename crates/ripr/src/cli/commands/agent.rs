use crate::analysis;
use crate::app;
use crate::app::agent_brief::{
    AgentBriefPolicy, AgentBriefResolvedWorkingSet, select_agent_brief_seams,
};
use crate::cli::agent::{
    AgentBriefOptions, AgentCommand, AgentPacketOptions, AgentReceiptOptions,
    AgentReviewSummaryOptions, AgentStartOptions, AgentStatusOptions, AgentVerifyOptions,
    parse_agent_args,
};
use crate::cli::commands::agent_dispatch;
use crate::cli::commands::io::write_text_file;
use crate::cli::commands_agent_support::{
    build_agent_receipt_provenance, read_agent_verify_snapshot, resolve_agent_brief_working_set,
    validate_agent_receipt_verify_path, validate_agent_verify_snapshot_path,
};
use crate::cli::commands_context::{ensure_command_root, load_root_input_and_config};
use crate::config::load_for_root;
use crate::output;
use std::path::{Path, PathBuf};

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let command = parse_agent_args(args)?;
    if let Some(result) = agent_dispatch::run_agent_help_command(&command) {
        return result;
    }

    match command {
        AgentCommand::Start(options) => run_agent_start(options),
        AgentCommand::Brief(options) => run_agent_brief(options),
        AgentCommand::Packet(options) => run_agent_packet(options),
        AgentCommand::Verify(options) => run_agent_verify(options),
        AgentCommand::Receipt(options) => run_agent_receipt(options),
        AgentCommand::Status(options) => run_agent_status(options),
        AgentCommand::ReviewSummary(options) => run_agent_review_summary(options),
        help_command @ (AgentCommand::Help
        | AgentCommand::StartHelp
        | AgentCommand::BriefHelp
        | AgentCommand::PacketHelp
        | AgentCommand::VerifyHelp
        | AgentCommand::ReceiptHelp
        | AgentCommand::StatusHelp
        | AgentCommand::ReviewSummaryHelp) => agent_dispatch::run_agent_help_command(&help_command)
            .unwrap_or_else(|| Err("agent help command was not dispatched".to_string())),
    }
}

fn run_agent_start(options: AgentStartOptions) -> Result<(), String> {
    ensure_command_root(&options.root, "agent start")?;
    let (input, config) = load_root_input_and_config(&options.root)?;

    let working_set = AgentBriefResolvedWorkingSet::seam_id(options.seam_id.clone());
    let classified = analysis::inventory_classified_seams_at_with_config(&input.root, &config)?;
    let selection = select_agent_brief_seams(
        &classified,
        &working_set,
        1,
        AgentBriefPolicy::from_config(&config),
    );
    if selection.top_seams.is_empty() {
        return Err(format!(
            "agent start seam_id {} was not found or is hidden by config",
            options.seam_id
        ));
    }

    let out_dir = resolve_agent_start_out_dir(&input.root, &options.out_dir);
    std::fs::create_dir_all(&out_dir)
        .map_err(|err| format!("create {} failed: {err}", out_dir.display()))?;

    let agent_brief_json = output::agent_brief::render_agent_brief_json(
        &input.root,
        &input.mode,
        &config,
        &working_set,
        &selection,
    )?;
    let agent_brief_path = out_dir.join("agent-brief.json");
    write_text_file(&agent_brief_path, &agent_brief_json)?;

    let manifest = app::agent_workflow::build_agent_workflow_manifest(
        &input.root,
        &options.root,
        &input.mode,
        &options.out_dir,
        &options.seam_id,
        &agent_brief_json,
    )?;
    let workflow_json = output::agent_workflow::render_agent_workflow_json(&manifest)?;
    let commands_md = output::agent_workflow::render_agent_workflow_commands_md(&manifest);
    let workflow_path = out_dir.join("workflow.json");
    let commands_path = out_dir.join("commands.md");
    write_text_file(&workflow_path, &workflow_json)?;
    write_text_file(&commands_path, &commands_md)?;

    println!("Wrote {}", workflow_path.display());
    println!("Wrote {}", commands_path.display());
    println!("Wrote {}", agent_brief_path.display());
    if let Some(next) = manifest.missing_inputs.first() {
        println!("Next: {}", next.command);
    }
    Ok(())
}

fn run_agent_brief(options: AgentBriefOptions) -> Result<(), String> {
    ensure_command_root(&options.root, "agent brief")?;
    let (input, config) = load_root_input_and_config(&options.root)?;

    let working_set = resolve_agent_brief_working_set(&input.root, &options.working_set)?;
    let classified = analysis::inventory_classified_seams_at_with_config(&input.root, &config)?;
    let selection = select_agent_brief_seams(
        &classified,
        &working_set,
        options.max_seams,
        AgentBriefPolicy::from_config(&config),
    );
    let rendered = output::agent_brief::render_agent_brief_json(
        &input.root,
        &input.mode,
        &config,
        &working_set,
        &selection,
    )?;
    println!("{rendered}");
    Ok(())
}

fn run_agent_packet(options: AgentPacketOptions) -> Result<(), String> {
    ensure_command_root(&options.root, "agent packet")?;

    if let (Some(gap_ledger), Some(gap_id)) = (&options.gap_ledger, &options.gap_id) {
        let rendered = render_agent_packet_from_gap_ledger(gap_ledger, gap_id)?;
        print!("{rendered}");
        return Ok(());
    }

    let seam_id = options.seam_id.as_deref().ok_or_else(|| {
        "agent packet requires --seam-id or --gap-ledger with --gap-id".to_string()
    })?;
    let config = load_for_root(&options.root)?;
    let classified = analysis::inventory_classified_seams_at_with_config(&options.root, &config)?;
    let entry = classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == seam_id)
        .ok_or_else(|| format!("agent packet seam_id {seam_id} was not found"))?;

    let policy = AgentBriefPolicy::from_config(&config);
    if let Some(reason) = policy.omission_reason_for_class(entry.class) {
        return Err(format!("agent packet seam_id {seam_id} {reason}"));
    }

    let rendered = output::agent_seam_packets::render_agent_seam_packet_json(entry);
    print!("{rendered}");
    Ok(())
}

pub(super) fn render_agent_packet_from_gap_ledger(
    gap_ledger: &Path,
    gap_id: &str,
) -> Result<String, String> {
    let contents = std::fs::read_to_string(gap_ledger).map_err(|err| {
        format!(
            "agent packet --gap-ledger {} is invalid: read failed: {err}",
            gap_ledger.display()
        )
    })?;
    let records =
        output::gap_decision_ledger::parse_gap_records_json(&contents).map_err(|err| {
            format!(
                "agent packet --gap-ledger {} is invalid: {err}",
                gap_ledger.display()
            )
        })?;
    let record = records
        .iter()
        .find(|record| record.gap_id == gap_id || record.canonical_gap_id == gap_id)
        .ok_or_else(|| format!("agent packet gap_id {gap_id} was not found"))?;
    output::agent_seam_packets::render_agent_gap_record_packet_json(
        &output::outcome::display_path(gap_ledger),
        record,
    )
    .map_err(|err| format!("agent packet gap_id {gap_id} {err}"))
}

fn run_agent_verify(options: AgentVerifyOptions) -> Result<(), String> {
    let before_path =
        validate_agent_verify_snapshot_path(&options.root, &options.before, "--before")?;
    let after_path = validate_agent_verify_snapshot_path(&options.root, &options.after, "--after")?;
    let before_json = read_agent_verify_snapshot(&before_path, "before")?;
    let after_json = read_agent_verify_snapshot(&after_path, "after")?;
    let report = output::outcome::targeted_test_outcome_report_from_json(
        &before_json,
        &after_json,
        output::outcome::display_path(&options.before),
        output::outcome::display_path(&options.after),
    )?;
    let rendered = output::outcome::render_agent_verify_json(&report)?;
    println!("{rendered}");
    Ok(())
}

fn run_agent_receipt(options: AgentReceiptOptions) -> Result<(), String> {
    ensure_command_root(&options.root, "agent receipt")?;

    let verify_path = validate_agent_receipt_verify_path(&options.root, &options.verify_json)?;
    let verify_json = std::fs::read_to_string(&verify_path).map_err(|err| {
        format!(
            "read agent receipt verify JSON {} failed: {err}",
            output::outcome::display_path(&verify_path)
        )
    })?;
    let input_paths = output::agent_receipt::agent_receipt_input_paths(&verify_json)?;
    let provenance = build_agent_receipt_provenance(
        &options.root,
        &options.verify_json,
        &verify_path,
        &input_paths,
    )?;
    let rendered = output::agent_receipt::render_agent_receipt_json(
        &verify_json,
        output::outcome::display_path(&options.verify_json),
        &options.seam_id,
        options.test_changed.as_deref(),
        &options.commands_run,
        provenance,
    )?;

    match options.out {
        Some(path) => {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("create {} failed: {err}", parent.display()))?;
            }
            std::fs::write(&path, rendered).map_err(|err| {
                format!(
                    "write {} failed: {err}",
                    output::outcome::display_path(&path)
                )
            })
        }
        None => {
            print!("{rendered}");
            Ok(())
        }
    }
}

fn run_agent_status(options: AgentStatusOptions) -> Result<(), String> {
    ensure_command_root(&options.root, "agent status")?;

    let report = app::agent_status::build_agent_status_report(&options.root, &options.root);
    if options.json {
        let rendered = app::agent_status::render_agent_status_json(&report)?;
        print!("{rendered}");
    } else {
        let rendered = app::agent_status::render_agent_status_markdown(&report);
        print!("{rendered}");
    }
    Ok(())
}

fn run_agent_review_summary(options: AgentReviewSummaryOptions) -> Result<(), String> {
    ensure_command_root(&options.root, "agent review-summary")?;

    let report =
        app::agent_review_summary::build_agent_review_summary_report(&options.root, &options.root);
    if options.json {
        let rendered = app::agent_review_summary::render_agent_review_summary_json(&report)?;
        print!("{rendered}");
    } else {
        let rendered = app::agent_review_summary::render_agent_review_summary_markdown(&report);
        print!("{rendered}");
    }
    Ok(())
}

fn resolve_agent_start_out_dir(root: &Path, out_dir: &Path) -> PathBuf {
    if out_dir.is_absolute() {
        out_dir.to_path_buf()
    } else {
        root.join(out_dir)
    }
}
