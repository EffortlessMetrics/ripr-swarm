use super::render_helpers::push_path_field;
use crate::output::json::escape as json_escape;
use crate::output::markdown::{PowershellForm, powershell_form};
use crate::output::path::display_path;
use crate::output::pilot::commands::{PilotCommands, suggested_retry_timeout_ms};
use crate::output::pilot::{PILOT_SUMMARY_SCHEMA_VERSION, PilotSummaryContext};

/// Human timeout reason. JSON keeps `timeout_ms` as the spent budget
/// (schema `0.3`); the prose names the default and the automatic
/// extension separately so a user who typed nothing is not told they
/// asked for 270s (#7427).
fn analysis_timeout_reason(context: PilotSummaryContext<'_>) -> String {
    let spent_ms = context.timeout_ms;
    let extension_ms = context.timeout_extension_ms;
    if extension_ms > 0 {
        let requested_ms = spent_ms.saturating_sub(extension_ms);
        format!(
            "analysis timed out after {spent_ms} ms total: {requested_ms} default + {extension_ms} automatic extension; pass --timeout-ms to set a hard budget"
        )
    } else {
        format!("analysis timed out after {spent_ms} ms")
    }
}

/// Extra next-step copy when the printed retry is not a plain doubling
/// of the spent budget: the interactive cap bound it, or the spent
/// budget already sat at or above that cap (#5443, #7427).
fn retry_bound_note(spent_ms: u64) -> Option<&'static str> {
    let retry_ms = suggested_retry_timeout_ms(spent_ms);
    (retry_ms < spent_ms.saturating_mul(2)).then_some(
        "The printed retry is an interactive next step and is never smaller than the time already spent. If that is still not enough, warm the fact cache with a prior `ripr check` or pass a larger --timeout-ms.",
    )
}

pub(crate) fn render_pilot_timeout_summary_json(context: PilotSummaryContext<'_>) -> String {
    let commands = PilotCommands::new(context);

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "  \"schema_version\": \"{}\",\n",
        PILOT_SUMMARY_SCHEMA_VERSION
    ));
    out.push_str("  \"tool\": \"ripr\",\n");
    out.push_str("  \"scope\": \"repo\",\n");
    out.push_str("  \"status\": \"partial\",\n");
    out.push_str("  \"reason\": \"timeout\",\n");
    out.push_str(&format!("  \"timeout_ms\": {},\n", context.timeout_ms));
    out.push_str("  \"completed_phases\": [],\n");
    out.push_str(&format!(
        "  \"root\": \"{}\",\n",
        json_escape(&display_path(context.root))
    ));
    out.push_str(&format!("  \"mode\": \"{}\",\n", context.mode.as_str()));
    out.push_str("  \"config\": {");
    match context.config_path {
        Some(path) => out.push_str(&format!(
            "\"state\": \"loaded\", \"path\": \"{}\"",
            json_escape(&display_path(path))
        )),
        None => out.push_str("\"state\": \"missing\", \"path\": null"),
    }
    out.push_str("},\n");

    // `outputs` stays the packet path map (intended locations) so the
    // timeout JSON shape remains schema `0.3`. `outputs_written` is the
    // authority for what this run produced; the timeout command removes
    // the unwritten siblings so those paths do not still hold a prior run
    // (#7425).
    out.push_str("  \"outputs\": {\n");
    push_path_field(
        &mut out,
        "repo_exposure_json",
        &context.artifacts.repo_exposure_json,
        true,
    );
    push_path_field(
        &mut out,
        "repo_exposure_md",
        &context.artifacts.repo_exposure_md,
        true,
    );
    push_path_field(
        &mut out,
        "agent_seam_packets_json",
        &context.artifacts.agent_seam_packets_json,
        true,
    );
    push_path_field(
        &mut out,
        "pilot_summary_json",
        &context.artifacts.pilot_summary_json,
        true,
    );
    push_path_field(
        &mut out,
        "pilot_summary_md",
        &context.artifacts.pilot_summary_md,
        false,
    );
    out.push_str("  },\n");

    out.push_str("  \"outputs_written\": [\n");
    out.push_str("    \"pilot_summary_json\",\n");
    out.push_str("    \"pilot_summary_md\"\n");
    out.push_str("  ],\n");
    out.push_str(&format!("  \"max_seams\": {},\n", context.max_seams));
    out.push_str("  \"actionable_seams_total\": null,\n");
    out.push_str("  \"withheld_static_limitations_total\": null,\n");
    out.push_str("  \"top_actionable_seams\": [],\n");
    out.push_str("  \"next\": {\n");
    out.push_str(&format!(
        "    \"retry_command\": \"{}\"\n",
        json_escape(&commands.retry)
    ));
    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

pub(crate) fn render_pilot_timeout_summary_md(context: PilotSummaryContext<'_>) -> String {
    let commands = PilotCommands::new(context);

    let mut out = String::new();
    out.push_str("# RIPR Pilot Summary\n\n");
    out.push_str("## Scope\n\n");
    out.push_str("- Status: `partial`\n");
    out.push_str(&format!("- Reason: {}\n", analysis_timeout_reason(context)));
    out.push_str(&format!("- Root: `{}`\n", display_path(context.root)));
    out.push_str(&format!("- Mode: `{}`\n", context.mode.as_str()));
    match context.config_path {
        Some(path) => out.push_str(&format!("- Config: loaded `{}`\n\n", display_path(path))),
        None => out.push_str("- Config: missing; using built-in defaults\n\n"),
    }

    out.push_str("## Outputs\n\n");
    out.push_str("Analysis did not finish within the pilot budget, so repo exposure and agent seam packet files were not written.\n");
    out.push_str("This directory does not keep leftover `repo-exposure.json`, `repo-exposure.md`, or `agent-seam-packets.json` files from an earlier run.\n\n");
    out.push_str(&format!(
        "- Pilot summary JSON: `{}`\n",
        display_path(&context.artifacts.pilot_summary_json)
    ));
    out.push_str(&format!(
        "- Pilot summary Markdown: `{}`\n\n",
        display_path(&context.artifacts.pilot_summary_md)
    ));

    out.push_str("## Next Command\n\n");
    out.push_str("Rerun with a larger explicit budget:\n\n");
    out.push_str(super::COMMAND_SHELL_DISCLOSURE);
    out.push_str("```bash\n");
    out.push_str(&commands.retry);
    out.push_str("\n```\n");
    match powershell_form(&commands.retry) {
        PowershellForm::Translated(line) => {
            out.push_str("\n```powershell\n");
            out.push_str(&line);
            out.push_str("\n```\n");
        }
        PowershellForm::SameAsBash => {}
        PowershellForm::Unavailable => out.push_str(&format!(
            "{}: `{}`\n",
            crate::output::markdown::POWERSHELL_UNAVAILABLE_DISCLOSURE,
            commands.retry
        )),
    }
    if let Some(note) = retry_bound_note(context.timeout_ms) {
        out.push('\n');
        out.push_str(note);
        out.push('\n');
    }
    out
}

pub(crate) fn render_pilot_timeout_terminal(context: PilotSummaryContext<'_>) -> String {
    let commands = PilotCommands::new(context);

    let mut out = String::new();
    out.push_str("RIPR pilot partial.\n\n");
    out.push_str("Reason:\n");
    out.push_str(&format!("  {}\n\n", analysis_timeout_reason(context)));
    out.push_str("Config:\n");
    match context.config_path {
        Some(path) => out.push_str(&format!("  loaded: {}\n", display_path(path))),
        None => out.push_str("  missing: using built-in defaults\n"),
    }
    out.push('\n');
    out.push_str("Written:\n");
    out.push_str(&format!(
        "  {}\n",
        display_path(&context.artifacts.pilot_summary_json)
    ));
    out.push_str(&format!(
        "  {}\n\n",
        display_path(&context.artifacts.pilot_summary_md)
    ));
    out.push_str("This packet directory does not keep leftover repo-exposure or seam-packet files from an earlier run.\n\n");
    out.push_str("Next:\n");
    out.push_str(&format!("  {}\n", commands.retry));
    if let Some(note) = retry_bound_note(context.timeout_ms) {
        out.push('\n');
        out.push_str(note);
        out.push('\n');
    }
    out
}
