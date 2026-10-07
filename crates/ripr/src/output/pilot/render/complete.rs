use super::render_helpers::{
    NO_REPAIR_TARGET_FOCUSED_TEST, no_repair_target_hand_step, push_markdown_recommendation,
    push_path_field, push_top_seam_json, yes_no,
};
use super::why_line;
use crate::agent::loop_commands::shell_path;
use crate::analysis::ClassifiedSeam;
use crate::output::agent_seam_packets::{
    suggested_assertion_for_classified_seam, targeted_test_brief_outline_for_classified_seam,
};
use crate::output::json::escape as json_escape;
use crate::output::markdown::{PowershellForm, powershell_form};
use crate::output::path::{display_path, display_path_text};
use crate::output::pilot::commands::{
    PilotCommands, python_card_first_pr_command, repair_start_command,
};
use crate::output::pilot::ranking::{
    actionable_in_change, actionable_in_owner, actionable_total, top_actionable_seams,
    withheld_static_limitations,
};
use crate::output::pilot::{
    ChangeSeams, PILOT_SUMMARY_SCHEMA_VERSION, PilotCurrentChange, PilotLanguageRoute,
    PilotLanguageRoutes, PilotPythonFirstUse, PilotSummaryContext, RUST_EXCLUDED_GUIDANCE,
};
use crate::output::python_repair_card::PythonRepairCard;

const PYTHON_PREVIEW_SUPPORTED_FEATURES: &[&str] = &[
    "project_detection",
    "diff_owner_mapping",
    "pytest_oracle_facts",
    "unittest_oracle_facts",
    "repair_cards",
];

const PYTHON_PREVIEW_DEFERRED_FEATURES: &[&str] = &[
    "outcome_receipts",
    "runtime_mutation_execution",
    "gate_authority",
    "generated_tests",
];

pub(crate) fn render_pilot_summary_json(
    classified: &[ClassifiedSeam],
    context: PilotSummaryContext<'_>,
) -> String {
    let actionable_total = actionable_total(classified);
    let top = top_actionable_seams(classified, context.max_seams, context.current_change);
    let commands = PilotCommands::new(context);

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "  \"schema_version\": \"{}\",\n",
        PILOT_SUMMARY_SCHEMA_VERSION
    ));
    out.push_str("  \"tool\": \"ripr\",\n");
    out.push_str("  \"scope\": \"repo\",\n");
    out.push_str("  \"status\": \"complete\",\n");
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

    out.push_str(&format!("  \"max_seams\": {},\n", context.max_seams));
    out.push_str(&format!("  \"timeout_ms\": {},\n", context.timeout_ms));
    out.push_str("  \"outputs_written\": [\n");
    out.push_str("    \"repo_exposure_json\",\n");
    out.push_str("    \"repo_exposure_md\",\n");
    out.push_str("    \"agent_seam_packets_json\",\n");
    out.push_str("    \"pilot_summary_json\",\n");
    out.push_str("    \"pilot_summary_md\"\n");
    out.push_str("  ],\n");
    out.push_str(&format!(
        "  \"actionable_seams_total\": {},\n",
        actionable_total
    ));
    out.push_str(&format!(
        "  \"withheld_static_limitations_total\": {},\n",
        withheld_static_limitations(classified)
    ));
    out.push_str("  \"top_actionable_seams\": [");
    for (idx, entry) in top.iter().enumerate() {
        if idx == 0 {
            out.push('\n');
        }
        push_top_seam_json(&mut out, entry);
        if idx + 1 != top.len() {
            out.push_str(",\n");
        } else {
            out.push('\n');
        }
    }
    if !top.is_empty() {
        out.push_str("  ");
    }
    out.push_str("],\n");
    push_python_first_use_json(&mut out, context.python_first_use);
    push_language_routes_json(&mut out, context.language_routes);
    push_current_change_json(
        &mut out,
        classified,
        top.first().copied(),
        context.current_change,
    );
    out.push_str("  \"next\": {\n");
    out.push_str(&format!(
        "    \"inspect_packet\": \"{}\",\n",
        json_escape(&display_path(&context.artifacts.agent_seam_packets_json))
    ));
    // An unanalyzed-only workspace has no seam to snapshot or measure, so
    // offering the follow-up commands would send the reader into a loop.
    // A Rust exclusion likewise emptied the ranking (#5205), and so did
    // withholding every seam as a static limitation with no Python card to
    // offer instead (#5497): there is no gap to snapshot or measure.
    let withheld_only = top.is_empty()
        && withheld_static_limitations(classified) > 0
        && python_top_repair_card(context.python_first_use).is_none();
    if unanalyzed_only(context).is_some() || rust_excluded(context).is_some() || withheld_only {
        out.push_str("    \"after_snapshot_command\": null,\n");
        out.push_str("    \"outcome_command\": null,\n");
    } else {
        out.push_str(&format!(
            "    \"after_snapshot_command\": \"{}\",\n",
            json_escape(&commands.after_snapshot)
        ));
        out.push_str(&format!(
            "    \"outcome_command\": \"{}\",\n",
            json_escape(&commands.outcome)
        ));
    }
    match top
        .first()
        .and_then(|entry| repair_start_command(context.root, entry))
    {
        Some(command) => out.push_str(&format!(
            "    \"repair_command\": \"{}\"\n",
            json_escape(&command)
        )),
        None => out.push_str("    \"repair_command\": null\n"),
    }
    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

pub(crate) fn render_pilot_summary_md(
    classified: &[ClassifiedSeam],
    context: PilotSummaryContext<'_>,
) -> String {
    let actionable_total = actionable_total(classified);
    let top = top_actionable_seams(classified, context.max_seams, context.current_change);
    let commands = PilotCommands::new(context);

    let mut out = String::new();
    out.push_str("# RIPR Pilot Summary\n\n");
    out.push_str("## What Was Inspected\n\n");
    out.push_str("- Status: `complete`\n");
    out.push_str(&format!("- Root: `{}`\n", display_path(context.root)));
    out.push_str(&format!("- Mode: `{}`\n", context.mode.as_str()));
    out.push_str(&format!("- Timeout: {} ms\n", context.timeout_ms));
    match context.config_path {
        Some(path) => out.push_str(&format!("- Config: loaded `{}`\n", display_path(path))),
        None => out.push_str("- Config: missing; using built-in defaults\n"),
    }
    if let Some(scope) = scope_line(context, true) {
        out.push_str(&format!("- Scope: {scope}\n"));
    }
    // #6602: a seam limit cut the classified list before ranking, so every
    // Rust seam count below covers only the seams that were kept, and the
    // actionable count is a lower bound.
    if let Some(limit) = context.seam_limit {
        out.push_str(&format!(
            "- Seam limit reached: ranked {} of {} seams; Rust seam counts below cover those only\n",
            limit.analyzed, limit.total
        ));
        out.push_str(&format!(
            "- Actionable seams: at least {}, showing up to {}\n",
            actionable_total, context.max_seams
        ));
    } else {
        out.push_str(&format!(
            "- Actionable seams: {} total, showing up to {}\n",
            actionable_total, context.max_seams
        ));
    }
    let withheld = withheld_static_limitations(classified);
    if withheld > 0 {
        out.push_str(&format!(
            "- Withheld: {} ({WITHHELD_REASON}; listed in `{}`)\n",
            withheld_count_label(withheld, context.seam_limit),
            display_path(&context.artifacts.repo_exposure_md)
        ));
    }
    if top.is_empty()
        && let Some(reason) = unranked_change_reason(context, true)
    {
        out.push_str(&format!("- Current change: {reason}\n"));
    }
    out.push('\n');

    let python_top = python_top_repair_card(context.python_first_use);
    if top.is_empty() {
        out.push_str("## Top Recommendation\n\n");
        if let Some(card) = python_top {
            push_python_repair_card_md(&mut out, card);
        } else if let Some(excluded) = rust_excluded(context) {
            out.push_str(&format!(
                "Pilot ranks Rust seams, but Rust is not enabled in `ripr.toml [languages]` ({} not analyzed). This is not a clean result; see Excluded From Pilot's Rust Seam Scan.\n\n",
                file_count_label(excluded)
            ));
        } else if required_routes(context).is_some() {
            out.push_str(
                "Pilot ranks Rust seams and found none in this repository. This is not a clean result for the languages listed under Languages Outside The Rust Seam Scan.\n\n",
            );
        } else if let Some(unanalyzed) = unanalyzed_only(context) {
            out.push_str(&format!(
                "None: {UNANALYZED_ONLY_VERDICT} Found: {}.\n\n",
                unanalyzed_label(unanalyzed)
            ));
        } else if withheld > 0 {
            out.push_str(&format!(
                "None ranked: {} {WITHHELD_ONLY_VERDICT} Inspect them in `{}`.\n\n",
                withheld_count_label(withheld, context.seam_limit),
                display_path(&context.artifacts.repo_exposure_md)
            ));
        } else {
            out.push_str("No actionable seam was ranked by the default pilot policy.\n\n");
        }
    } else {
        out.push_str("## Top Recommendation\n\n");
        if let Some(label) = current_change_label(context, top[0]) {
            out.push_str(&format!("- Current change: {}\n", label.markdown()));
        }
        push_markdown_recommendation(&mut out, top[0]);
        out.push('\n');

        // A ranked seam is a gap worth reading, not a repair offer. When none
        // of them can start `ripr agent repair`, the heading must not call
        // them actionable (#4216 row 3).
        if top
            .iter()
            .any(|entry| repair_start_command(context.root, entry).is_some())
        {
            out.push_str("## Ranked Actionable Seams\n\n");
        } else {
            out.push_str("## Ranked Seams\n\n");
            out.push_str(
                "None of these seams can start a repair attempt (`ripr agent repair`); they are ranked for inspection by hand. Repair scope: `ripr agent repair --help`.\n\n",
            );
        }
        for (idx, entry) in top.iter().enumerate() {
            let in_change = context
                .current_change
                .is_some_and(|change| change.touches(entry));
            out.push_str(&format!(
                "{}. `{}` {} (`{}`) {}:{} `{}`{}\n",
                idx + 1,
                entry.seam.id().as_str(),
                entry.class.plain_label(),
                entry.class.as_str(),
                display_path(entry.seam.file()),
                entry.seam.display_line(),
                entry.seam.kind().as_str(),
                if in_change {
                    " (in your current change)"
                } else {
                    ""
                }
            ));
            out.push_str(&format!("   - Owner: `{}`\n", entry.seam.owner()));
            // Ranking spreads the list across owners (#5770); say once, on
            // the owner's first pick, what else it stands for, so the owner's
            // other seams stay visible.
            let same_owner = |shown: &&ClassifiedSeam| -> bool {
                shown.seam.file() == entry.seam.file() && shown.seam.owner() == entry.seam.owner()
            };
            let first_of_owner = top.iter().position(same_owner) == Some(idx);
            let unlisted = actionable_in_owner(classified, entry)
                .saturating_sub(top.iter().filter(|shown| same_owner(shown)).count());
            if first_of_owner && unlisted > 0 {
                out.push_str(&format!(
                    "   - Also in this function: {}{} more actionable {} not listed here\n",
                    if context.seam_limit.is_some() {
                        "at least "
                    } else {
                        ""
                    },
                    unlisted,
                    if unlisted == 1 { "seam" } else { "seams" }
                ));
            }
            out.push_str(&format!("   - Why: {}\n", why_line(entry)));
            out.push_str(&format!(
                "   - Related test present: {}\n",
                yes_no(!entry.evidence.related_tests.is_empty())
            ));
            out.push_str(&format!(
                "   - Suggested assertion present: {}\n",
                yes_no(suggested_assertion_for_classified_seam(entry).is_some())
            ));
            out.push('\n');
        }
    }

    if let Some(first_use) = context.python_first_use {
        push_python_first_use_md(&mut out, first_use);
    }
    if let Some(excluded) = rust_excluded(context) {
        push_rust_exclusion_md(&mut out, excluded);
    }
    if let Some(routes) = required_routes(context) {
        push_language_routes_md(&mut out, routes);
    }

    out.push_str("## Outputs\n\n");
    out.push_str(&format!(
        "- Repo exposure JSON: `{}`\n",
        display_path(&context.artifacts.repo_exposure_json)
    ));
    out.push_str(&format!(
        "- Repo exposure Markdown: `{}`\n",
        display_path(&context.artifacts.repo_exposure_md)
    ));
    out.push_str(&format!(
        "- Agent seam packets: `{}`\n",
        display_path(&context.artifacts.agent_seam_packets_json)
    ));
    out.push_str(&format!(
        "- Pilot summary JSON: `{}`\n\n",
        display_path(&context.artifacts.pilot_summary_json)
    ));

    out.push_str("## Next Commands\n\n");
    let repair = top
        .first()
        .and_then(|entry| repair_start_command(context.root, entry));
    // One ordinary route (#3906): when the top seam can be repaired, the
    // repair transaction replaces the manual before/after snapshot pair.
    let routes = required_routes(context);
    let python_card = python_top_repair_card(context.python_first_use).filter(|_| top.is_empty());
    let python_first_pr = python_card_first_pr_command(context.root);
    let next_commands: Vec<&String> = match (repair.as_ref(), routes) {
        (Some(command), _) => {
            out.push_str(
                "Start the repair transaction for the top seam, add one focused test (test files only), then run the `--attempt ... --phase after` command it prints:\n\n",
            );
            vec![command]
        }
        // #3906: with no Rust seams, the repo-exposure snapshot pair would
        // only report that no seams moved. Route to the diff-first check that
        // analyzes the languages pilot did not rank.
        // rc rehearsal (py-pricing): a Python repair card is the top
        // recommendation, so the next commands follow its route; `ripr check`
        // would only lead back to pilot.
        (None, _) if python_card.is_some() => match python_card {
            Some(card) => {
                let edit = format!(
                    "{} `{}` in `{}` (test files only)",
                    capitalized(repair_action_label(&card.repair_action)),
                    card.suggested_test_name,
                    card.suggested_test_file
                );
                match card.receipt_command.as_ref() {
                    Some(receipt) => {
                        out.push_str(&format!(
                            "{edit}, then run the card's verify command and its receipt command:\n\n"
                        ));
                        vec![&card.verify_command, receipt]
                    }
                    None => {
                        out.push_str(&format!(
                            "Run `ripr first-pr` before the test edit: it names this gap's receipt command (run any regeneration command it prints first). Then {} `{}` in `{}` (test files only), run the card's verify command, and run that receipt command:\n\n",
                            repair_action_label(&card.repair_action),
                            card.suggested_test_name,
                            card.suggested_test_file
                        ));
                        vec![&python_first_pr, &card.verify_command]
                    }
                }
            }
            None => Vec::new(),
        },
        // #5205: see the terminal renderer: check honors the same config,
        // so only the config edit unblocks Rust ranking.
        (None, _) if rust_excluded(context).is_some() => {
            out.push_str("To rank Rust seams:\n\n");
            out.push_str(&format!("- {}\n", capitalized(RUST_EXCLUDED_GUIDANCE)));
            return out;
        }
        (None, Some(routes)) => {
            let commands = PilotLanguageRoutes::commands(routes);
            if commands.is_empty() {
                out.push_str(NO_LANGUAGE_ROUTE_COMMAND);
                out.push('\n');
                return out;
            }
            out.push_str(
                "Analyze the changed code in the languages outside the Rust seam scan with the diff-first check:\n\n",
            );
            commands
        }
        (None, None) if unanalyzed_only(context).is_some() => {
            out.push_str(NO_ANALYZED_LANGUAGE_COMMAND);
            out.push('\n');
            return out;
        }
        // #5497: with every seam withheld there is no gap to test, so the
        // snapshot pair would only measure an edit nobody was asked to make.
        (None, None) if top.is_empty() && withheld > 0 => {
            out.push_str(&withheld_only_next(context.seam_limit));
            out.push('\n');
            return out;
        }
        (None, None) => {
            match top.first() {
                Some(entry)
                    if targeted_test_brief_outline_for_classified_seam(entry)
                        .is_not_applicable() =>
                {
                    out.push_str(&format!(
                        "No repair attempt is available for the top seam. Next, {}, then rerun repo exposure and compare the snapshots:\n\n",
                        no_repair_target_hand_step(entry)
                    ));
                }
                _ => out.push_str(
                    "After adding one focused test, rerun repo exposure and compare the snapshots:\n\n",
                ),
            }
            vec![&commands.after_snapshot, &commands.outcome]
        }
    };
    out.push_str(super::COMMAND_SHELL_DISCLOSURE);
    out.push_str("```bash\n");
    for command in &next_commands {
        out.push_str(command);
        out.push('\n');
    }
    out.push_str("```\n");
    let mut unavailable: Vec<&String> = Vec::new();
    let mut translations: Vec<String> = Vec::new();
    let mut any_translated = false;
    for command in next_commands {
        match powershell_form(command) {
            PowershellForm::Translated(line) => {
                any_translated = true;
                translations.push(line);
            }
            PowershellForm::SameAsBash => translations.push(command.clone()),
            PowershellForm::Unavailable => unavailable.push(command),
        }
    }
    // Only fence translations that exist; a compound command under-emits to a
    // disclosure naming the bash form instead of an invalid translation. The
    // fence is the whole sequence, so it carries unchanged lines too, and is
    // omitted when every line runs unchanged in PowerShell.
    if any_translated {
        out.push_str("\n```powershell\n");
        for line in &translations {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("\n```\n");
    }
    for command in unavailable {
        out.push_str(&format!(
            "{}: `{command}`\n",
            crate::output::markdown::POWERSHELL_UNAVAILABLE_DISCLOSURE
        ));
    }
    out
}

pub(crate) fn render_pilot_terminal(
    classified: &[ClassifiedSeam],
    context: PilotSummaryContext<'_>,
) -> String {
    let top = top_actionable_seams(classified, 1, context.current_change);
    let commands = PilotCommands::new(context);

    let mut out = String::new();
    out.push_str("RIPR pilot complete.\n\n");
    out.push_str("Inspected:\n");
    out.push_str(&format!("  root: {}\n", display_path(context.root)));
    out.push_str(&format!("  mode: {}\n", context.mode.as_str()));
    match context.config_path {
        Some(path) => out.push_str(&format!("  config: loaded {}\n", display_path(path))),
        None => out.push_str("  config: missing, using built-in defaults\n"),
    }
    out.push_str(&format!("  timeout: {} ms\n", context.timeout_ms));
    if let Some(scope) = scope_line(context, false) {
        out.push_str(&format!("  scope: {scope}\n"));
    }
    // #5497: the terminal is where most users read the empty ranking, so it
    // states the seam limit too; a gap past the cut was never classified.
    if let Some(limit) = context.seam_limit {
        out.push_str(&format!(
            "  seam limit: ranked {} of {} seams\n",
            limit.analyzed, limit.total
        ));
    }
    let withheld = withheld_static_limitations(classified);
    if withheld > 0 {
        out.push_str(&format!(
            "  withheld: {} ({WITHHELD_REASON})\n",
            withheld_count_label(withheld, context.seam_limit)
        ));
    }
    if top.is_empty()
        && let Some(reason) = unranked_change_reason(context, false)
    {
        out.push_str(&format!("  current change: {reason}\n"));
    }
    out.push('\n');

    let no_repair_target = if let Some(entry) = top.first() {
        let outline = targeted_test_brief_outline_for_classified_seam(entry);
        out.push_str("Top recommendation:\n");
        if let Some(label) = current_change_label(context, entry) {
            out.push_str(&format!("  current change: {}\n", label.terminal()));
        }
        // The id leads the line, as it does in the Markdown sibling
        // (`render_helpers::push_markdown_recommendation`). Until this was
        // added, the terminal was the only one of the three pilot renderers
        // that dropped it, so a user who ran `ripr pilot` and read the screen
        // had no way to reach `ripr agent repair --seam-id <id>` — the step the
        // README names next — without opening a written artifact.
        out.push_str(&format!(
            "  inspected seam: {} {}:{} {} in {} ({})\n",
            entry.seam.id().as_str(),
            display_path(entry.seam.file()),
            entry.seam.display_line(),
            entry.seam.kind().as_str(),
            entry.seam.owner(),
            entry.class.human_label()
        ));
        out.push_str(&format!("  why it matters: {}\n", why_line(entry)));
        if outline.is_not_applicable() {
            out.push_str(&format!(
                "  focused test: {NO_REPAIR_TARGET_FOCUSED_TEST}\n"
            ));
        } else {
            out.push_str(&format!(
                "  focused test: add {} in {}\n",
                outline.suggested_name,
                display_path_text(&outline.suggested_file)
            ));
        }
        if let Some(value) = outline.candidate_value.as_ref() {
            out.push_str(&format!("  candidate value: {value}\n"));
        }
        out.push_str(&format!("  assertion: {}\n", outline.assertion_shape));
        // Only a seam that passes the fail-closed repair-packet flip gets the
        // paste-ready command. Route readiness alone is weaker: a ready seam
        // can still be ineligible, and offering a repair transaction there
        // would promise a target `agent repair` refuses. The closing block
        // uses the same builder, so the two lines cannot disagree (#3906).
        if let Some(command) = repair_start_command(context.root, entry) {
            out.push_str(&format!("  repair this seam: {command}\n"));
            if let Some(form) = crate::output::markdown::powershell_text_variant(&command) {
                out.push_str(&format!("  (PowerShell) {form}\n"));
            }
        } else if !outline.is_not_applicable() {
            // The focused test above is a suggestion, not a repair offer. Say
            // so on the screen, so the README's "run the `ripr agent repair`
            // command pilot prints" is not left waiting for a command that
            // will never appear.
            out.push_str(&format!("  repair this seam: {NO_REPAIR_START_LINE}\n"));
        }
        out.push('\n');
        outline.is_not_applicable()
    } else if let Some(card) = python_top_repair_card(context.python_first_use) {
        out.push_str("Top recommendation:\n");
        push_python_repair_card_terminal(&mut out, card);
        out.push('\n');
        false
    } else if let Some(excluded) = rust_excluded(context) {
        // #5205: the exclusion explains the empty ranking, so it outranks
        // the routed-languages pointer; those sections still render below.
        out.push_str("Top recommendation:\n");
        out.push_str(&format!(
            "  none: pilot ranks Rust seams, but Rust is not enabled in ripr.toml [languages] ({} not analyzed); see the excluded scope below\n\n",
            file_count_label(excluded)
        ));
        false
    } else if required_routes(context).is_some() {
        out.push_str("Top recommendation:\n");
        out.push_str(
            "  none: pilot ranks Rust seams and found none here; see the languages below\n\n",
        );
        false
    } else if let Some(unanalyzed) = unanalyzed_only(context) {
        out.push_str("Top recommendation:\n");
        out.push_str(&format!(
            "  none: {UNANALYZED_ONLY_VERDICT}\n  found: {}\n\n",
            unanalyzed_label(unanalyzed)
        ));
        false
    } else if withheld > 0 {
        out.push_str("Top recommendation:\n");
        out.push_str(&format!(
            "  none ranked: {} {WITHHELD_ONLY_VERDICT}\n  inspect: {}\n\n",
            withheld_count_label(withheld, context.seam_limit),
            display_path(&context.artifacts.repo_exposure_md)
        ));
        false
    } else {
        out.push_str("Top recommendation:\n");
        out.push_str("  none ranked by the default pilot policy\n\n");
        false
    };

    if let Some(first_use) = context.python_first_use {
        push_python_first_use_terminal(&mut out, first_use);
    }
    if let Some(excluded) = rust_excluded(context) {
        push_rust_exclusion_terminal(&mut out, excluded);
    }
    let routes = required_routes(context);
    if let Some(routes) = routes {
        push_language_routes_terminal(&mut out, routes);
    }

    out.push_str("Detailed brief:\n");
    out.push_str(&format!(
        "  {}\n",
        display_path(&context.artifacts.pilot_summary_md)
    ));
    out.push_str("Structured packet:\n");
    out.push_str(&format!(
        "  {}\n\n",
        display_path(&context.artifacts.agent_seam_packets_json)
    ));
    if let Some(command) = top
        .first()
        .and_then(|entry| repair_start_command(context.root, entry))
    {
        out.push_str("Next, in order:\n");
        out.push_str(&format!("  1. {command}\n"));
        if let Some(form) = crate::output::markdown::powershell_text_variant(&command) {
            out.push_str(&format!("     (PowerShell) {form}\n"));
        }
        out.push_str("  2. add the focused test named above (test files only)\n");
        out.push_str("  3. run the `--attempt ... --phase after` command that step 1 prints\n");
        out.push_str(
            "  (do not redirect these commands' output into the checkout, for example `> packet.json`: the edit cage counts that file as an edit; use target/ripr/ or a directory outside the repository)\n",
        );
        return out;
    }
    // rc rehearsal (py-pricing): with no Rust seam but a Python repair card,
    // the card is the top recommendation, so the closing block names the
    // card's route, not `ripr check`, which only leads back here.
    if top.is_empty()
        && let Some(card) = python_top_repair_card(context.python_first_use)
    {
        let edit = format!(
            "{} {} in {} (test files only): {}",
            repair_action_label(&card.repair_action),
            card.suggested_test_name,
            card.suggested_test_file,
            card.suggested_assertion
        );
        out.push_str("Next, in order:\n");
        if let Some(receipt) = card.receipt_command.as_deref() {
            out.push_str(&format!("  1. {edit}\n"));
            out.push_str(&format!("  2. {}\n", card.verify_command));
            out.push_str(&format!("  3. {receipt}\n"));
        } else {
            out.push_str(&format!(
                "  1. {} (names this gap's receipt command; run any regeneration command it prints first)\n",
                python_card_first_pr_command(context.root)
            ));
            out.push_str(&format!("  2. {edit}\n"));
            out.push_str(&format!("  3. {}\n", card.verify_command));
            out.push_str("  4. run the receipt command step 1 printed\n");
        }
        return out;
    }
    // #5205: Rust files exist but Rust is disabled, so no seam could rank.
    // Routing to `ripr check` would loop (check honors the same config);
    // the only unblock is the config edit.
    if rust_excluded(context).is_some() {
        out.push_str("Next, to rank Rust seams:\n");
        out.push_str(&format!("  {}\n", capitalized(RUST_EXCLUDED_GUIDANCE)));
        return out;
    }
    if let Some(routes) = routes {
        let commands = PilotLanguageRoutes::commands(routes);
        if commands.is_empty() {
            out.push_str(NO_LANGUAGE_ROUTE_COMMAND);
            out.push('\n');
        } else {
            out.push_str("Next, analyze the changed code in these languages:\n");
            for command in commands {
                out.push_str(&format!("  {command}\n"));
            }
        }
        return out;
    }
    if unanalyzed_only(context).is_some() {
        out.push_str(NO_ANALYZED_LANGUAGE_COMMAND);
        out.push('\n');
        return out;
    }
    if top.is_empty() && withheld > 0 {
        out.push_str(&withheld_only_next(context.seam_limit));
        out.push('\n');
        return out;
    }
    if let Some(entry) = top.first().filter(|_| no_repair_target) {
        out.push_str(&format!(
            "Next, by hand: {}, then compare against this run:\n",
            no_repair_target_hand_step(entry)
        ));
    } else if !top.is_empty() {
        out.push_str(
            "Next, by hand: add the focused test named above, then compare against this run:\n",
        );
    } else {
        out.push_str("Run after adding the focused test:\n");
    }
    out.push_str(&format!("  {}\n", commands.after_snapshot));
    out.push_str(&format!("  {}\n", commands.outcome));
    out
}

/// Terminal note for a top seam whose focused test is named but that fails the
/// repair-packet eligibility flip, so no `ripr agent repair` command is printed.
const NO_REPAIR_START_LINE: &str = "not available for this seam (static evidence does not admit a repair target); add the focused test by hand";

/// What pilot's ranking covers: change-first when there is a current change,
/// else the whole repository (with the reason when the change could not be
/// loaded). `None` when change data was not collected.
fn scope_line(context: PilotSummaryContext<'_>, code: bool) -> Option<String> {
    let change = context.current_change?;
    Some(if change.is_changed() {
        match change.base() {
            Some(base) if code => {
                format!("change-first (Rust seams on lines changed since `{base}` rank first)")
            }
            Some(base) => {
                format!("change-first (Rust seams on lines changed since {base} rank first)")
            }
            None => "change-first (Rust seams on changed lines rank first)".to_string(),
        }
    } else if let Some(reason) = change.unavailable_reason() {
        format!("whole repository (current change unavailable: {reason})")
    } else {
        "whole repository".to_string()
    })
}

/// Whether the top recommendation is part of the current change.
/// `None` when there is no current change or it could not be
/// loaded: pilot's ranking is then repo-wide, as it always was.
enum CurrentChangeLabel {
    PartOfChange {
        base: Option<String>,
    },
    Elsewhere {
        base: Option<String>,
        check: String,
        seams: ChangeSeams,
        diff_only: Option<DiffOnlyNote>,
    },
}

/// The first changed file only diff analysis covers, its shape, and how
/// many such files the change has (#6944).
struct DiffOnlyNote {
    file: String,
    source: crate::analysis::DiffOnlySource,
    count: usize,
}

impl DiffOnlyNote {
    fn of(change: &crate::output::pilot::PilotCurrentChange) -> Option<Self> {
        let files = change.diff_only_files();
        let (file, source) = files.first()?;
        Some(Self {
            file: file.clone(),
            source: *source,
            count: files.len(),
        })
    }

    /// "the change is in `build.rs`, a Cargo build script, which pilot's
    /// repo-wide ranking leaves out"
    fn reason(&self, code: bool) -> String {
        use crate::analysis::DiffOnlySource;
        let kind = match self.source {
            DiffOnlySource::BuildScript => "a Cargo build script",
            DiffOnlySource::RepoAutomation => "repository automation",
            DiffOnlySource::DeclaredOutsideSrc => "a crate source declared outside src",
        };
        let file = if code {
            format!("`{}`", self.file)
        } else {
            self.file.clone()
        };
        match self.count {
            1 => format!(
                "the change is in {file}, {kind}, which pilot's repo-wide ranking leaves out"
            ),
            count => format!(
                "the change includes {count} files pilot's repo-wide ranking leaves out, such as {file} ({kind})"
            ),
        }
    }
}

fn current_change_label(
    context: PilotSummaryContext<'_>,
    top: &ClassifiedSeam,
) -> Option<CurrentChangeLabel> {
    let change = context
        .current_change
        .filter(|change| change.is_changed())?;
    let base = change.base().map(str::to_string);
    Some(if change.touches(top) {
        CurrentChangeLabel::PartOfChange { base }
    } else {
        CurrentChangeLabel::Elsewhere {
            base,
            seams: change.seams().unwrap_or_default(),
            diff_only: DiffOnlyNote::of(change),
            check: format!(
                "ripr check --root {}{}",
                shell_path(&crate::agent::loop_commands::bound_root_path(context.root)),
                change.check_selector()
            ),
        }
    })
}

/// With nothing ranked, why the current change's seams are not ranked
/// either, when it has any or a seam limit may hide them (#5309). The
/// pilot budget can drop those seams before the empty-ranking text counts
/// what was withheld, so this reads the counts taken before the cut.
fn unranked_change_reason(context: PilotSummaryContext<'_>, code: bool) -> Option<String> {
    let change = context.current_change?;
    let seams = change.seams()?;
    let diff_only = DiffOnlyNote::of(change);
    if seams.touched == 0 && seams.unanalyzed.is_none() && diff_only.is_none() {
        return None;
    }
    let base = change.base().map(str::to_string);
    Some(CurrentChangeLabel::why_elsewhere(
        seams,
        diff_only.as_ref(),
        base.as_ref(),
        code,
    ))
}

impl CurrentChangeLabel {
    fn changed_line(base: Option<&String>, code: bool) -> String {
        match base {
            Some(base) if code => format!("a line changed since `{base}`"),
            Some(base) => format!("a line changed since {base}"),
            None => "a line your current change touches".to_string(),
        }
    }

    /// Where a count of seams sits: "a line changed since X" for one,
    /// "lines changed since X" for several.
    fn changed_lines(base: Option<&String>, code: bool, count: usize) -> String {
        if count == 1 {
            return Self::changed_line(base, code);
        }
        match base {
            Some(base) if code => format!("lines changed since `{base}`"),
            Some(base) => format!("lines changed since {base}"),
            None => "lines your current change touches".to_string(),
        }
    }

    /// Why no seam on the change ranks (#5309). A change whose seams pilot
    /// withholds, or whose seams a seam limit left unanalyzed, must not read
    /// as a change with no seams.
    fn why_elsewhere(
        seams: ChangeSeams,
        diff_only: Option<&DiffOnlyNote>,
        base: Option<&String>,
        code: bool,
    ) -> String {
        let ChangeSeams {
            touched,
            withheld,
            unanalyzed,
        } = seams;
        let lines = |count| Self::changed_lines(base, code, count);
        // Seams past the inventory limit were never classified, so a reason
        // drawn from the analyzed seams alone must say it may not be all.
        let unseen = unanalyzed.map(|(analyzed, total)| {
            format!(
                "the seam limit left {} of {total} seams unanalyzed, so the change may have seams pilot did not see",
                total.saturating_sub(analyzed)
            )
        });
        let gripped = touched.saturating_sub(withheld);
        let mut reason = if withheld > 0 {
            let which = match (withheld, touched) {
                (1, 1) => format!("the analyzed seam on {}", lines(1)),
                (w, t) if w == t => format!("the {t} analyzed seams on {}", lines(t)),
                (w, t) => format!("{w} of the {t} analyzed seams on {}", lines(t)),
            };
            let what = if withheld == 1 {
                "its static evidence is unknown or opaque, so it is a static limitation, not a gap"
            } else {
                "their static evidence is unknown or opaque, so they are static limitations, not gaps"
            };
            let rest = match gripped {
                0 => String::new(),
                1 => "; the other is already gripped, intentional or suppressed".to_string(),
                _ => "; the others are already gripped, intentional or suppressed".to_string(),
            };
            format!("Pilot withholds {which}: {what}{rest}")
        } else if touched > 0 {
            match touched {
                1 => format!(
                    "The analyzed seam on {} has no gap to rank: it is already gripped, intentional or suppressed",
                    lines(1)
                ),
                t => format!(
                    "The {t} analyzed seams on {} have no gap to rank: they are already gripped, intentional or suppressed",
                    lines(t)
                ),
            }
        } else if let Some(note) = diff_only {
            // #6944: `ripr check` analyzes these files, so "no seam" alone
            // would contradict it; say why pilot did not look.
            format!(
                "No seam pilot analyzed is on {}: {}",
                lines(1),
                note.reason(code)
            )
        } else if unseen.is_some() {
            format!("No analyzed seam is on {}", lines(1))
        } else {
            return format!("No seam pilot analyzed is on {}.", lines(1));
        };
        if let Some(unseen) = unseen {
            reason.push_str(", but ");
            reason.push_str(&unseen);
        }
        reason.push('.');
        reason
    }

    fn terminal(&self) -> String {
        match self {
            Self::PartOfChange { base } => format!(
                "part of it (this seam is on {})",
                Self::changed_line(base.as_ref(), false)
            ),
            Self::Elsewhere {
                base,
                check,
                seams,
                diff_only,
            } => format!(
                "not part of it. {} This recommendation is elsewhere in the repo. For the change itself, run: {check}",
                Self::why_elsewhere(*seams, diff_only.as_ref(), base.as_ref(), false)
            ),
        }
    }

    fn markdown(&self) -> String {
        match self {
            Self::PartOfChange { base } => format!(
                "part of it (this seam is on {})",
                Self::changed_line(base.as_ref(), true)
            ),
            Self::Elsewhere {
                base,
                check,
                seams,
                diff_only,
            } => format!(
                "not part of it. {} This recommendation is elsewhere in the repo. For the change itself, run `{check}`.",
                Self::why_elsewhere(*seams, diff_only.as_ref(), base.as_ref(), true)
            ),
        }
    }
}

fn push_current_change_json(
    out: &mut String,
    classified: &[ClassifiedSeam],
    top: Option<&ClassifiedSeam>,
    change: Option<&PilotCurrentChange>,
) {
    out.push_str("  \"current_change\": ");
    let Some(change) = change else {
        out.push_str("null,\n");
        return;
    };
    out.push_str("{\n");
    json_string_field(out, 4, "state", change.state(), true);
    json_optional_string_field(out, 4, "base", change.base(), true);
    json_optional_string_field(out, 4, "reason", change.unavailable_reason(), true);
    if change.is_changed() {
        out.push_str(&format!(
            "    \"actionable_seams_in_change\": {},\n",
            actionable_in_change(classified, change)
        ));
        out.push_str(&format!(
            "    \"withheld_seams_in_change\": {},\n",
            change.seams().unwrap_or_default().withheld
        ));
    } else {
        out.push_str("    \"actionable_seams_in_change\": null,\n");
        out.push_str("    \"withheld_seams_in_change\": null,\n");
    }
    match top.filter(|_| change.is_changed()) {
        Some(entry) => out.push_str(&format!(
            "    \"top_recommendation_in_change\": {}\n",
            change.touches(entry)
        )),
        None => out.push_str("    \"top_recommendation_in_change\": null\n"),
    }
    out.push_str("  },\n");
}

/// Closing line when every language pilot did not rank is unavailable in
/// this binary, so no runnable command exists.
const NO_LANGUAGE_ROUTE_COMMAND: &str =
    "No follow-up command applies: this ripr binary cannot analyze the languages listed above.";

/// Why an empty ranking is a non-claim when pilot found only source in
/// languages no ripr adapter reads.
const UNANALYZED_ONLY_VERDICT: &str = "this repository's source is in languages ripr does not analyze, so the empty ranking is not a clean result. ripr analyzes Rust, plus TypeScript/JavaScript and Python as previews.";

/// Why pilot withheld seams from its ranking (#5497): their class is
/// `opaque` or an `*_unknown` class, so ripr's static evidence could not
/// establish whether a test discriminates them.
const WITHHELD_REASON: &str =
    "static evidence is unknown or opaque, so they are static limitations, not gaps";

/// Why an empty ranking with withheld seams is not a clean result.
const WITHHELD_ONLY_VERDICT: &str = "were withheld because their static evidence is unknown or opaque. ripr cannot tell whether a test discriminates them, so this is not a clean result.";

/// Closing line for [`WITHHELD_ONLY_VERDICT`]: no gap to test, no snapshot
/// pair to compare.
const WITHHELD_ONLY_NEXT: &str = "No gap to test: each withheld seam's evidence in the repo exposure report names the stage ripr could not resolve.";

/// [`WITHHELD_ONLY_NEXT`], unless a seam limit cut seams pilot never
/// classified: those may hold gaps, so "no gap to test" would claim an
/// absence the run did not establish, and raising the limit is the step.
fn withheld_only_next(seam_limit: Option<&crate::analysis::SeamLimitInfo>) -> String {
    match seam_limit {
        Some(limit) => format!(
            "No gap ranked among the {} seams pilot analyzed, but the seam limit left {} of {} seams unanalyzed and they may hold gaps: raise or remove RIPR_PILOT_SEAM_BUDGET and RIPR_REPO_EXPOSURE_SEAM_LIMIT, then rerun pilot. Each withheld seam's evidence in the repo exposure report names the stage ripr could not resolve.",
            limit.analyzed,
            limit.total.saturating_sub(limit.analyzed),
            limit.total
        ),
        None => WITHHELD_ONLY_NEXT.to_string(),
    }
}

fn seam_count_label(count: usize) -> String {
    format!("{count} {}", if count == 1 { "seam" } else { "seams" })
}

/// Withheld seams counted over the kept seams only are a lower bound once a
/// seam limit cut the list (#6602).
fn withheld_count_label(
    count: usize,
    seam_limit: Option<&crate::analysis::SeamLimitInfo>,
) -> String {
    match seam_limit {
        Some(_) => format!("at least {}", seam_count_label(count)),
        None => seam_count_label(count),
    }
}

/// Closing line for [`UNANALYZED_ONLY_VERDICT`]: no ripr command applies.
const NO_ANALYZED_LANGUAGE_COMMAND: &str =
    "No follow-up command applies: review changes in these languages with their own tests.";

fn unanalyzed_only<'a>(context: PilotSummaryContext<'a>) -> Option<&'a [(&'static str, usize)]> {
    context
        .language_routes
        .and_then(PilotLanguageRoutes::unanalyzed_only)
}

fn unanalyzed_label(unanalyzed: &[(&'static str, usize)]) -> String {
    unanalyzed
        .iter()
        .map(|(language, count)| format!("{language} ({})", file_count_label(*count)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Routes the human output must show: present only when pilot's Rust seam
/// scan produced no seams, so output for Rust seams stays unchanged.
fn required_routes<'a>(context: PilotSummaryContext<'a>) -> Option<&'a [PilotLanguageRoute]> {
    context
        .language_routes
        .and_then(PilotLanguageRoutes::required)
}

/// Rust files excluded by `[languages] enabled`, when the exclusion emptied
/// the ranking (#5205).
fn rust_excluded(context: PilotSummaryContext<'_>) -> Option<usize> {
    context
        .language_routes
        .and_then(PilotLanguageRoutes::rust_exclusion)
}

fn route_status_label(route: &PilotLanguageRoute) -> &'static str {
    match (route.available, route.enabled) {
        (false, _) => "not available in this build",
        (true, true) => "preview, diff-first",
        (true, false) => "preview, diff-first; not enabled in ripr.toml [languages]",
    }
}

fn file_count_label(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

fn push_language_routes_terminal(out: &mut String, routes: &[PilotLanguageRoute]) {
    out.push_str("Languages outside pilot's Rust seam scan:\n");
    for route in routes {
        out.push_str(&format!(
            "  {}: {} ({})\n",
            route.language.as_str(),
            file_count_label(route.file_count),
            route_status_label(route)
        ));
        // One label for every language: the guidance category id stays in
        // `pilot-summary.json` and the Markdown guidance line, not the label.
        if let Some(command) = route.command.as_deref() {
            out.push_str(&format!("    route: {command}\n"));
        } else if let Some(guidance) = route.guidance.as_deref() {
            out.push_str(&format!("    {guidance}\n"));
        }
    }
    out.push('\n');
}

fn push_rust_exclusion_terminal(out: &mut String, file_count: usize) {
    out.push_str("Excluded from pilot's Rust seam scan:\n");
    out.push_str(&format!(
        "  rust: {} (not enabled in ripr.toml [languages])\n",
        file_count_label(file_count)
    ));
    out.push_str(&format!("    {RUST_EXCLUDED_GUIDANCE}\n\n"));
}

fn push_rust_exclusion_md(out: &mut String, file_count: usize) {
    out.push_str("## Excluded From Pilot's Rust Seam Scan\n\n");
    out.push_str(&format!(
        "- `rust`: {} (not enabled in `ripr.toml [languages]`)\n  - {RUST_EXCLUDED_GUIDANCE}\n\n",
        file_count_label(file_count)
    ));
}

fn push_language_routes_md(out: &mut String, routes: &[PilotLanguageRoute]) {
    out.push_str("## Languages Outside The Rust Seam Scan\n\n");
    for route in routes {
        out.push_str(&format!(
            "- `{}`: {} ({})\n",
            route.language.as_str(),
            file_count_label(route.file_count),
            route_status_label(route)
        ));
        if let Some(command) = route.command.as_deref() {
            out.push_str(&format!("  - Route: `{command}`\n"));
        }
        match (route.guidance_category, route.guidance.as_deref()) {
            (Some(category), Some(guidance)) => {
                out.push_str(&format!("  - `{category}`: {guidance}\n"));
            }
            (None, Some(guidance)) => out.push_str(&format!("  - {guidance}\n")),
            _ => {}
        }
    }
    out.push('\n');
}

fn push_language_routes_json(out: &mut String, routes: Option<&PilotLanguageRoutes>) {
    out.push_str("  \"language_routes\": ");
    let Some(routes) = routes else {
        out.push_str("null,\n");
        return;
    };
    out.push_str("{\n");
    json_string_field(out, 4, "state", routes.state.as_str(), true);
    out.push_str("    \"routes\": [");
    for (idx, route) in routes.routes.iter().enumerate() {
        out.push_str(if idx == 0 { "\n" } else { ",\n" });
        out.push_str("      {\n");
        json_string_field(out, 8, "language", route.language.as_str(), true);
        out.push_str(&format!("        \"file_count\": {},\n", route.file_count));
        json_string_field(out, 8, "language_status", route.language_status(), true);
        out.push_str(&format!("        \"enabled\": {},\n", route.enabled));
        json_string_field(out, 8, "route", route.route(), true);
        json_optional_string_field(out, 8, "command", route.command.as_deref(), true);
        json_optional_string_field(out, 8, "guidance_category", route.guidance_category, true);
        json_optional_string_field(out, 8, "guidance", route.guidance.as_deref(), false);
        out.push_str("      }");
    }
    if !routes.routes.is_empty() {
        out.push_str("\n    ");
    }
    out.push(']');
    // Omitted when empty, so pilot JSON for supported repositories is
    // unchanged.
    if !routes.unanalyzed.is_empty() {
        out.push_str(",\n    \"unanalyzed_languages\": [");
        for (idx, (language, count)) in routes.unanalyzed.iter().enumerate() {
            out.push_str(if idx == 0 { "\n" } else { ",\n" });
            out.push_str(&format!(
                "      {{ \"language\": \"{}\", \"file_count\": {count} }}",
                json_escape(language)
            ));
        }
        out.push_str("\n    ]");
    }
    // Omitted when Rust is enabled (or no Rust files exist), so pilot JSON
    // for supported repositories is unchanged (#5205).
    if let Some(file_count) = routes.rust_exclusion() {
        out.push_str(",\n    \"rust_excluded_from_scope\": {\n");
        json_string_field(out, 6, "language", "rust", true);
        out.push_str(&format!("      \"file_count\": {file_count},\n"));
        out.push_str("      \"enabled\": false,\n");
        json_string_field(out, 6, "guidance", RUST_EXCLUDED_GUIDANCE, false);
        out.push_str("    }");
    }
    out.push('\n');
    out.push_str("  },\n");
}

fn python_top_repair_card(first_use: Option<&PilotPythonFirstUse>) -> Option<&PythonRepairCard> {
    first_use.and_then(|first_use| first_use.top_repair_card.as_ref())
}

fn push_python_first_use_json(out: &mut String, first_use: Option<&PilotPythonFirstUse>) {
    out.push_str("  \"python_first_use\": ");
    let Some(first_use) = first_use else {
        out.push_str("null,\n");
        return;
    };

    out.push_str("{\n");
    json_string_field(out, 4, "status", first_use.status.as_str(), true);
    json_string_field(out, 4, "language", "python", true);
    json_string_field(out, 4, "language_status", "preview", true);
    json_string_field(out, 4, "authority_boundary", "preview_advisory_only", true);
    out.push_str(&format!(
        "    \"findings_total\": {},\n",
        first_use.findings_total
    ));
    out.push_str(&format!(
        "    \"repair_cards_total\": {},\n",
        first_use.repair_cards_total
    ));
    out.push_str(&format!(
        "    \"limitation_count\": {},\n",
        first_use.limitation_count
    ));
    json_optional_string_field(
        out,
        4,
        "analysis_error",
        first_use.analysis_error.as_deref(),
        true,
    );
    json_string_array_field(
        out,
        4,
        "supported_features",
        PYTHON_PREVIEW_SUPPORTED_FEATURES,
        true,
    );
    json_string_array_field(
        out,
        4,
        "deferred_features",
        PYTHON_PREVIEW_DEFERRED_FEATURES,
        true,
    );
    out.push_str("    \"top_repair_card\": ");
    if let Some(card) = first_use.top_repair_card.as_ref() {
        push_python_repair_card_json(out, card, 4);
        out.push('\n');
    } else {
        out.push_str("null\n");
    }
    out.push_str("  },\n");
}

fn push_python_repair_card_json(out: &mut String, card: &PythonRepairCard, indent: usize) {
    let sp = " ".repeat(indent);
    out.push_str("{\n");
    json_string_field(out, indent + 2, "card_version", &card.card_version, true);
    json_string_field(out, indent + 2, "source", &card.source, true);
    json_string_field(
        out,
        indent + 2,
        "canonical_gap_id",
        &card.canonical_gap_id,
        true,
    );
    json_string_field(out, indent + 2, "language", &card.language, true);
    json_string_field(
        out,
        indent + 2,
        "language_status",
        &card.language_status,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "authority_boundary",
        &card.authority_boundary,
        true,
    );
    json_string_field(out, indent + 2, "repair_action", &card.repair_action, true);
    json_string_field(out, indent + 2, "changed_owner", &card.changed_owner, true);
    json_string_field(
        out,
        indent + 2,
        "changed_behavior",
        &card.changed_behavior,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "current_test_evidence",
        &card.current_test_evidence,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "missing_discriminator",
        &card.missing_discriminator,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "recommended_test_shape",
        &card.recommended_test_shape,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "suggested_assertion",
        &card.suggested_assertion,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "suggested_test_file",
        &card.suggested_test_file,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "suggested_test_name",
        &card.suggested_test_name,
        true,
    );
    json_optional_string_field(
        out,
        indent + 2,
        "suggested_test_node_id",
        card.suggested_test_node_id.as_deref(),
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "verify_command",
        &card.verify_command,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "verify_command_confidence",
        &card.verify_command_confidence,
        true,
    );
    json_optional_string_field(
        out,
        indent + 2,
        "receipt_command",
        card.receipt_command.as_deref(),
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "receipt_status",
        &card.receipt_status,
        true,
    );
    json_string_field(
        out,
        indent + 2,
        "receipt_guidance",
        &card.receipt_guidance,
        true,
    );
    json_string_array_field_refs(
        out,
        indent + 2,
        "stop_conditions",
        &card.stop_conditions,
        true,
    );
    json_string_array_field_refs(out, indent + 2, "limits", &card.limits, false);
    out.push_str(&format!("{sp}}}"));
}

fn json_string_field(out: &mut String, indent: usize, name: &str, value: &str, trailing: bool) {
    out.push_str(&format!(
        "{}\"{}\": \"{}\"{}\n",
        " ".repeat(indent),
        name,
        json_escape(value),
        if trailing { "," } else { "" }
    ));
}

fn json_optional_string_field(
    out: &mut String,
    indent: usize,
    name: &str,
    value: Option<&str>,
    trailing: bool,
) {
    let sp = " ".repeat(indent);
    match value {
        Some(value) => out.push_str(&format!(
            "{sp}\"{name}\": \"{}\"{}\n",
            json_escape(value),
            if trailing { "," } else { "" }
        )),
        None => out.push_str(&format!(
            "{sp}\"{name}\": null{}\n",
            if trailing { "," } else { "" }
        )),
    }
}

fn json_string_array_field(
    out: &mut String,
    indent: usize,
    name: &str,
    values: &[&str],
    trailing: bool,
) {
    let owned = values
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    json_string_array_field_refs(out, indent, name, &owned, trailing);
}

fn json_string_array_field_refs(
    out: &mut String,
    indent: usize,
    name: &str,
    values: &[String],
    trailing: bool,
) {
    let sp = " ".repeat(indent);
    out.push_str(&format!("{sp}\"{name}\": ["));
    for (idx, value) in values.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("\"{}\"", json_escape(value)));
    }
    out.push_str(&format!("]{}\n", if trailing { "," } else { "" }));
}

fn push_python_first_use_md(out: &mut String, first_use: &PilotPythonFirstUse) {
    out.push_str("## Python Preview First Use\n\n");
    out.push_str(&format!("- Status: `{}`\n", first_use.status.as_str()));
    out.push_str("- Language: `python` (`preview`)\n");
    out.push_str("- Boundary: `preview_advisory_only`\n");
    out.push_str(&format!(
        "- Python findings: `{}`\n",
        first_use.findings_total
    ));
    out.push_str(&format!(
        "- Repair cards: `{}`\n",
        first_use.repair_cards_total
    ));
    out.push_str(&format!(
        "- Limitations: `{}`\n",
        first_use.limitation_count
    ));
    if let Some(error) = first_use.analysis_error.as_deref() {
        out.push_str(&format!("- Analysis note: `{}`\n", error));
    }
    if let Some(card) = first_use.top_repair_card.as_ref() {
        out.push('\n');
        push_python_repair_card_md(out, card);
    } else {
        out.push_str("\nNo Python repair card was selected for this run.\n\n");
    }
}

fn push_python_repair_card_md(out: &mut String, card: &PythonRepairCard) {
    out.push_str("- Top Python repairable gap:\n");
    out.push_str(&format!("  - Gap: `{}`\n", card.canonical_gap_id));
    out.push_str(&format!("  - Repair action: `{}`\n", card.repair_action));
    out.push_str(&format!("  - Changed owner: `{}`\n", card.changed_owner));
    out.push_str(&format!(
        "  - Changed behavior: {}\n",
        card.changed_behavior
    ));
    out.push_str(&format!(
        "  - Current test evidence: {}\n",
        card.current_test_evidence
    ));
    out.push_str(&format!(
        "  - Missing discriminator: `{}`\n",
        card.missing_discriminator
    ));
    out.push_str(&format!(
        "  - Recommended test shape: {}\n",
        card.recommended_test_shape
    ));
    out.push_str(&format!(
        "  - Suggested assertion: {}\n",
        card.suggested_assertion
    ));
    out.push_str(&format!(
        "  - Suggested test target: `{}` in `{}`\n",
        card.suggested_test_name, card.suggested_test_file
    ));
    out.push_str(&format!("  - Verify: `{}`\n", card.verify_command));
    if let Some(command) = card.receipt_command.as_deref() {
        out.push_str(&format!("  - Receipt: `{command}`\n"));
    } else {
        out.push_str(&format!("  - Receipt status: `{}`\n", card.receipt_status));
    }
    out.push_str(&format!(
        "  - Receipt guidance: {}\n",
        card.receipt_guidance
    ));
    out.push('\n');
}

fn push_python_first_use_terminal(out: &mut String, first_use: &PilotPythonFirstUse) {
    out.push_str("Python preview:\n");
    out.push_str(&format!("  status: {}\n", first_use.status.as_str()));
    out.push_str("  language: python (preview)\n");
    out.push_str(&format!("  findings: {}\n", first_use.findings_total));
    out.push_str(&format!(
        "  repair cards: {}\n",
        first_use.repair_cards_total
    ));
    out.push_str(&format!("  limitations: {}\n", first_use.limitation_count));
    if let Some(error) = first_use.analysis_error.as_deref() {
        out.push_str(&format!("  analysis note: {error}\n"));
    }
    if first_use.top_repair_card.is_none() {
        out.push_str("  top repair card: none\n");
    }
    out.push('\n');
}

fn push_python_repair_card_terminal(out: &mut String, card: &PythonRepairCard) {
    out.push_str("  language: python (preview)\n");
    out.push_str(&format!("  gap: {}\n", card.canonical_gap_id));
    out.push_str(&format!("  repair action: {}\n", card.repair_action));
    out.push_str(&format!("  changed owner: {}\n", card.changed_owner));
    out.push_str(&format!("  changed behavior: {}\n", card.changed_behavior));
    out.push_str(&format!(
        "  current test evidence: {}\n",
        card.current_test_evidence
    ));
    out.push_str(&format!(
        // #4381: the label comes from the shared gap-vocabulary authority.
        "  {}: {}\n",
        crate::output::gap_vocabulary::MISSING_DISCRIMINATOR_LABEL,
        card.missing_discriminator
    ));
    out.push_str(&format!(
        "  recommended repair: {} {} in {}\n",
        repair_action_label(&card.repair_action),
        card.suggested_test_name,
        card.suggested_test_file
    ));
    out.push_str(&format!("  test shape: {}\n", card.recommended_test_shape));
    out.push_str(&format!("  assertion: {}\n", card.suggested_assertion));
    out.push_str(&format!("  verify: {}\n", card.verify_command));
    if let Some(command) = card.receipt_command.as_deref() {
        out.push_str(&format!("  receipt: {command}\n"));
    } else {
        out.push_str(&format!("  receipt status: {}\n", card.receipt_status));
    }
    out.push_str(&format!("  receipt guidance: {}\n", card.receipt_guidance));
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn repair_action_label(action: &str) -> &'static str {
    match action {
        "strengthen_existing_test" => "strengthen",
        _ => "add or strengthen",
    }
}
