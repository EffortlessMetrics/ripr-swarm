use super::{
    ActionCommands, DEFAULT_TEST_ORACLE_ASSISTANT_PROOF_MD_OUT,
    DEFAULT_TEST_ORACLE_ASSISTANT_PROOF_OUT, FirstUsefulActionInput, ParsedSources,
    first_guidance_item, loop_commands, string_from_sources,
};

pub(super) fn seam_commands(
    input: &FirstUsefulActionInput,
    parsed: &ParsedSources,
) -> ActionCommands {
    let seam_id = selected_seam_id(parsed);
    let Some(seam_id) = seam_id else {
        return ActionCommands::default();
    };
    ActionCommands {
        context_packet: Some(format!(
            "ripr agent packet --root {} --seam-id {} --json",
            loop_commands::shell_arg(&input.root),
            loop_commands::shell_arg(&seam_id)
        )),
        after_snapshot: Some(loop_commands::check_repo_exposure_command(
            &input.root,
            "draft",
            loop_commands::WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
        )),
        verify: Some(loop_commands::agent_verify_command(
            &input.root,
            loop_commands::WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
            loop_commands::WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
            None,
        )),
        receipt: Some(loop_commands::agent_receipt_command(
            &input.root,
            loop_commands::WORKFLOW_AGENT_VERIFY_ARTIFACT,
            &seam_id,
            None,
        )),
        assistant_proof: None,
        status: None,
    }
}

pub(super) fn receipt_command(
    input: &FirstUsefulActionInput,
    parsed: &ParsedSources,
) -> Option<String> {
    let seam_id = selected_seam_id(parsed)?;
    Some(loop_commands::agent_receipt_command(
        &input.root,
        loop_commands::WORKFLOW_AGENT_VERIFY_ARTIFACT,
        &seam_id,
        None,
    ))
}

pub(super) fn assistant_proof_command() -> String {
    format!(
        "ripr assistant-loop proof --pr-guidance target/ripr/review/comments.json --agent-packet target/ripr/workflow/agent-brief.json --before {} --after {} --receipt {} --ledger target/ripr/reports/pr-evidence-ledger.json --out {} --out-md {}",
        loop_commands::WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
        loop_commands::WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
        loop_commands::WORKFLOW_AGENT_RECEIPT_ARTIFACT,
        DEFAULT_TEST_ORACLE_ASSISTANT_PROOF_OUT,
        DEFAULT_TEST_ORACLE_ASSISTANT_PROOF_MD_OUT
    )
}

pub(super) fn selected_seam_id(parsed: &ParsedSources) -> Option<String> {
    string_from_sources(&[
        (parsed.assistant_proof.as_ref(), &["seam", "seam_id"]),
        (parsed.receipt.as_ref(), &["provenance", "seam_id"]),
        (parsed.receipt.as_ref(), &["seam", "seam_id"]),
        (
            first_guidance_item(parsed.pr_guidance.as_ref()),
            &["seam_id"],
        ),
        (parsed.ledger.as_ref(), &["top_repair_route", "seam_id"]),
    ])
}
