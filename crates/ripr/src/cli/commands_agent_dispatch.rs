use crate::cli::agent::AgentCommand;
use crate::cli::help;

pub(super) fn print_help(command: &AgentCommand) -> bool {
    match command {
        AgentCommand::Help => help::print_agent_help(),
        AgentCommand::StartHelp => help::print_agent_start_help(),
        AgentCommand::BriefHelp => help::print_agent_brief_help(),
        AgentCommand::PacketHelp => help::print_agent_packet_help(),
        AgentCommand::VerifyHelp => help::print_agent_verify_help(),
        AgentCommand::ReceiptHelp => help::print_agent_receipt_help(),
        AgentCommand::StatusHelp => help::print_agent_status_help(),
        AgentCommand::ReviewSummaryHelp => help::print_agent_review_summary_help(),
        _ => return false,
    }
    true
}
