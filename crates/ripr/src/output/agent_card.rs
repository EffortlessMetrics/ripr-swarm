//! Plain-language human rendering of the compact repair card (#4667, #9244).
//!
//! Output-layer owner of the default `ripr agent card` summary lines. Every
//! line is projected from a typed `RepairCardV1` field; nothing is re-derived
//! and the JSON card is untouched. Both the CLI adapter and the application
//! layer's repair-card usability evidence consume this renderer, so neither
//! `app` nor `output` depends on the CLI adapter for it (#5193).

use crate::analysis;
use crate::app::task_first::CardStartHold;
use crate::domain::{
    FixInstructionState, FocusedExecutionGoal, RepairCardDoneWhen, RepairCardTarget, RepairCardV1,
    StaticMovementGoal,
};

/// The default human card: a short brief a first-time reader can act on.
/// Every line is projected from a typed card field; nothing is re-derived and
/// the JSON card is untouched. The ledger rows (producer, subject, non-claim,
/// detail routes, hashes) live in `agent_card_prose_lines` under
/// `--verbose`.
pub(crate) fn agent_card_plain_lines(
    card: &RepairCardV1,
    packet_command: &str,
    paste_root: Option<&std::path::Path>,
    hold: Option<&CardStartHold>,
) -> Vec<String> {
    let mut lines = vec!["Repair brief for this gap".to_string()];
    lines.push(format!("  Changed behavior: {}", card.changed_behavior));
    // One verdict: the shared start gate (`hold`) outranks the card's own
    // evidence readiness, so "ready" is only printed when `ripr repair`
    // would also start (#11397, #11381).
    // Any gate refusal means `ripr repair` would not start, so it forces
    // `ready` and `started` off (fail closed). Only the held-file cause,
    // which carries its own next step, also replaces the card's wording and
    // next action; other refusals keep the card's evidence wording and
    // canonical next action, and the gate's reason is only the fallback.
    let refused = hold.is_some();
    let hold_with_next = hold.filter(|hold| card.attempt.is_none() && hold.next.is_some());
    let started = card
        .attempt
        .as_ref()
        .is_some_and(|attempt| attempt.state == "awaiting_edit")
        && card.readiness.repair_ready
        && !refused;
    let ready = card.readiness.repair_ready && !refused;
    if let Some(hold) = hold_with_next {
        lines.push(format!("  Not ready to repair yet: {}.", hold.reason));
    } else if let Some(attempt) = card
        .attempt
        .as_ref()
        .filter(|attempt| attempt.state != "awaiting_edit")
    {
        lines.push(match attempt.state.as_str() {
            "ready_to_finish" => {
                "  The attempt's after phase has finished; inspect its status for the outcome."
                    .to_string()
            }
            "prepared" => {
                "  The attempt was prepared but has not started; inspect its status.".to_string()
            }
            _ => "  The repair attempt has ended; inspect its status for the outcome.".to_string(),
        });
    } else if started {
        lines.push(
            "  Ready to repair. The repair has started and is waiting for your test.".to_string(),
        );
    } else if ready {
        lines.push("  Ready to repair.".to_string());
    } else {
        let missing = card
            .readiness
            .missing_evidence
            .iter()
            .map(|evidence| analysis::repair_route::humanize_evidence(evidence))
            .collect::<Vec<_>>()
            .join("; ");
        // Plain evidence words first; the producer's blocker string can carry
        // internal vocabulary, so it is only the fallback.
        let reason = (!missing.is_empty())
            .then_some(missing)
            .or_else(|| {
                card.exact_blocker
                    .clone()
                    .filter(|blocker| !blocker.is_empty())
            })
            .or_else(|| hold.map(|hold| hold.reason.clone()));
        lines.push(match reason {
            Some(reason) => format!("  Not ready to repair yet: {reason}"),
            None => "  Not ready to repair yet.".to_string(),
        });
    }
    lines.push(format!(
        "  Suggested assertion: {}",
        match (
            &card.assertion_goal_detail,
            card.instruction.has_suggested_assertion
        ) {
            (Some(detail), true) => detail.clone(),
            // A repair that can start or has started has a route, so the
            // "no bounded repair route" wording would contradict it.
            _ if ready || started => {
                "none; ripr has no exact assertion to suggest for this gap.".to_string()
            }
            _ => instruction_plain_words(card.instruction.state).to_string(),
        }
    ));
    match &card.selected_target {
        Some(RepairCardTarget::Existing { file, line, .. }) => {
            lines.push(format!("  Test to extend: {file}:{line}"));
        }
        Some(RepairCardTarget::Proposed { file, .. }) => {
            lines.push(format!("  Add a test in: {file}"));
        }
        None => {}
    }
    let own_route = card
        .canonical_next_action
        .as_ref()
        .is_some_and(|action| action.command().is_some());
    let root_arg = || {
        crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root(
            &paste_root.map_or_else(|| ".".to_string(), |r| r.to_string_lossy().into_owned()),
        ))
    };
    let start_command = || {
        format!(
            "ripr repair {} --root {}",
            crate::agent::loop_commands::shell_arg(&card.subject.seam_id),
            root_arg()
        )
    };
    match &card.canonical_next_action {
        _ if hold_with_next.is_some() => {
            if let Some(next) = hold_with_next.and_then(|hold| hold.next.as_ref()) {
                lines.push(format!("  Next: {next}"));
                if let Some(command) = hold_with_next.and_then(|hold| hold.command.as_deref()) {
                    lines.extend(powershell_pair(command));
                }
            }
        }
        // No runnable command of the card's own, a repair not yet started,
        // and the start gate holds nothing: the one next step is the repair
        // start itself (#11397), the command the pilot printed.
        _ if ready && card.attempt.is_none() && !own_route => {
            let command = start_command();
            lines.push(format!("  Next: {command}"));
            lines.extend(powershell_pair(&command));
        }
        // A started attempt awaiting its test: the next step is the attempt's
        // continue command, not "no ready command" (#11381). The canonical
        // action for such an attempt is a prerequisite stop (the focused test
        // edit), which the generic plain renderer has no command line for.
        Some(action)
            if started
                && !own_route
                && action.stop().is_none_or(|stop| {
                    matches!(stop, crate::domain::NextActionStop::Unsupported { .. })
                        || matches!(stop,
                        crate::domain::NextActionStop::ProvideInput { input, .. }
                            if input.starts_with("focused test edit for attempt"))
                }) =>
        {
            if let Some(attempt) = &card.attempt {
                let command = format!(
                    "ripr continue --root {} --attempt {}",
                    root_arg(),
                    crate::agent::loop_commands::shell_arg(&attempt.attempt_id)
                );
                lines.push(format!("  Next: write the test, then run `{command}`"));
                lines.extend(powershell_pair(&command));
            }
        }
        Some(action) => {
            lines.extend(
                crate::output::next_action::render_next_action_plain_at_root(action, paste_root),
            );
        }
        None => match &card.next_action {
            Some(action) => lines.push(format!("  Next: {}", action.display)),
            None => lines.push(
                "  Next: ripr has no ready command for this gap (see --verbose).".to_string(),
            ),
        },
    }
    if !card.allowed_files.is_empty() {
        lines.push(format!(
            "  Files you may change: {}",
            card.allowed_files.join(", ")
        ));
    }
    if !card.forbidden_files.is_empty() {
        lines.push(format!(
            "  Do not change: {} (ripr rejects the repair if these change)",
            card.forbidden_files.join(", ")
        ));
    }
    lines.push(plain_done_when_line(&card.done_when));
    if let Some(attempt) = &card.attempt {
        lines.push(format!(
            "  Repair attempt: {} ({})",
            attempt.attempt_id, attempt.state
        ));
    }
    let already_shown: &[String] = card
        .canonical_next_action
        .as_ref()
        .map_or(&[], |action| action.limitations());
    let limits: Vec<&str> = already_shown
        .iter()
        .chain(
            card.limitations
                .iter()
                .filter(|l| !already_shown.contains(l)),
        )
        .map(String::as_str)
        .collect();
    if !limits.is_empty() {
        // Keep the brief short: two limits inline, the rest behind --verbose.
        let shown = limits
            .iter()
            .take(2)
            .copied()
            .collect::<Vec<_>>()
            .join("; ");
        let more = limits.len().saturating_sub(2);
        lines.push(if more == 0 {
            format!("  Limits: {shown}")
        } else {
            format!("  Limits: {shown}; and {more} more (see --verbose)")
        });
    }
    // The raw gap id is a copy-paste handle, not a title: it sits below the
    // actionable lines, next to the machine-readable route that takes it.
    lines.push(format!("  Repair ID: {}", card.subject.seam_id));
    lines.push("  This is a suggestion; ripr has not run or verified anything.".to_string());
    lines.push(format!(
        "  Machine-readable details (JSON): {packet_command}"
    ));
    if let Some(form) = crate::output::markdown::powershell_text_variant(packet_command) {
        lines.push(format!("  (PowerShell) {form}"));
    }
    lines.push("  More fields: add --verbose.".to_string());
    lines
}

/// The `(PowerShell)` pair line for a printed bash command, from the shared
/// translator every plain-text command surface uses; `None` when PowerShell
/// runs the bash spelling unchanged.
fn powershell_pair(command: &str) -> Option<String> {
    crate::output::markdown::powershell_text_variant(command)
        .map(|form| format!("  (PowerShell) {form}"))
}

/// Plain spelling of what a missing suggestion means; exhaustive so a new
/// instruction state must choose its wording.
fn instruction_plain_words(value: FixInstructionState) -> &'static str {
    match value {
        FixInstructionState::FixSiteReady => {
            "ripr found where the fix goes but has no exact assertion to suggest."
        }
        FixInstructionState::StaticLimitation => "ripr has no bounded repair route for this gap.",
        FixInstructionState::Stale => "this brief is out of date; rerun ripr check first.",
        FixInstructionState::InspectOnly => "inspect only; there is nothing to fix here.",
        FixInstructionState::Unavailable => "none; ripr has no suggested assertion for this gap.",
    }
}

/// The plain done-when line: the same typed axes as [`done_when_line`], minus
/// the internal ones (edit surface, mutation, snapshot), which stay in
/// `--verbose` and JSON.
fn plain_done_when_line(done: &RepairCardDoneWhen) -> String {
    let test = match done.focused_test_execution {
        FocusedExecutionGoal::VerifiedPass => "the new test passes",
        FocusedExecutionGoal::ExplicitlyNotRun => {
            "you have run the new test yourself (ripr does not)"
        }
    };
    let gap = match done.static_movement {
        StaticMovementGoal::ClosedBySelectedRoute => "the gap no longer shows in `ripr check`",
        StaticMovementGoal::ImprovedAccordingToSelectedRoute => {
            "`ripr check` shows the gap improved"
        }
    };
    format!("  Done when: {test} and {gap}.")
}
