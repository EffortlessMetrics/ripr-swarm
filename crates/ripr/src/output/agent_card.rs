//! Human repair-card presentation shared by CLI, usability measurement, and
//! other inward callers (#5193). CLI adapters own argv and dispatch only.

use crate::analysis;
use crate::app::task_first::CardStartHold;
use crate::domain::{
    CardCurrentnessGoal, EditCageGoal, FixInstructionState, FocusedExecutionGoal,
    MutationConfirmationGoal, RepairCardDoneWhen, RepairCardSnapshotCurrentness, RepairCardTarget,
    RepairCardV1, StaticMovementGoal,
};

fn wire_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// `paste_root` binds the canonical next-action block to the selected
/// repository for copy/paste (#3999); measurement callers with no checkout
/// pass `None` and keep the portable render.
pub(crate) fn agent_card_prose_lines(
    card: &RepairCardV1,
    packet_command: &str,
    paste_root: Option<&std::path::Path>,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("Repair card {}", card.repair_card_id));
    // The `repair_card.v1` schema identity is a machine contract: `--json`
    // carries it as `schema_version`. A newcomer cannot act on it, so the
    // human summary leaves it out (#9244).
    let mut subject = format!("  seam: {}", card.subject.seam_id);
    if let Some(finding_id) = &card.subject.finding_id {
        subject.push_str(&format!(" (finding {finding_id})"));
    }
    lines.push(subject);
    lines.push(format!(
        "  snapshot: {} @ {} ({})",
        card.snapshot.workspace_identity,
        card.snapshot.repository_head,
        snapshot_currentness_words(card.snapshot.currentness)
    ));
    lines.push(format!(
        "  instruction: {} (fix site: {}, suggested assertion: {})",
        instruction_state_words(card.instruction.state),
        yes_no(card.instruction.has_fix_site),
        yes_no(card.instruction.has_suggested_assertion)
    ));
    // #10644: this readiness line renders for a human reader, so translate
    // the internal evidence codes; the card's wire data keeps them stable.
    let missing = if card.readiness.missing_evidence.is_empty() {
        "none".to_string()
    } else {
        card.readiness
            .missing_evidence
            .iter()
            .map(|evidence| analysis::repair_route::humanize_evidence(evidence))
            .collect::<Vec<_>>()
            .join("; ")
    };
    lines.push(format!(
        "  readiness: repair_ready={} (missing evidence: {missing})",
        card.readiness.repair_ready
    ));
    lines.push(format!("  changed behavior: {}", card.changed_behavior));
    lines.push(format!(
        "  blocker: {}",
        card.exact_blocker.as_deref().unwrap_or("-")
    ));
    if let Some(goal) = &card.assertion_goal {
        lines.push(format!(
            "  assertion goal: {}: {}",
            wire_name(goal),
            card.assertion_goal_detail.as_deref().unwrap_or("-")
        ));
    }
    if let Some(target) = &card.selected_target {
        let rendered = match target {
            RepairCardTarget::Existing {
                symbol_id,
                file,
                line,
                test_kind,
                relation,
                workspace_identity: _,
            } => format!(
                "  selected target: existing {symbol_id} {file}:{line} ({} test, relation {relation})",
                wire_name(test_kind)
            ),
            RepairCardTarget::Proposed {
                file,
                owner,
                proposal_kind,
            } => format!(
                "  selected target: proposed {owner} in {file} ({} test)",
                wire_name(proposal_kind)
            ),
        };
        lines.push(rendered);
    }
    // The next-action section renders the canonical decision (#6304), so the
    // human prose and the wire DTO share one authority. Cards minted before
    // the canonical field keep the legacy reference-only rendering.
    match &card.canonical_next_action {
        Some(action) => {
            let block = match paste_root {
                Some(root) => {
                    crate::output::next_action::render_next_action_human_at_root(action, root)
                }
                None => crate::output::next_action::render_next_action_human(action),
            };
            for line in block.lines() {
                lines.push(line.to_string());
            }
        }
        None => match &card.next_action {
            Some(action) => {
                lines.push(format!("  next action: {} ({})", action.display, action.command_id));
            }
            None => lines.push(format!(
                "  next action: none (instruction {} with repair_ready={} exposes no bounded route; inspect the detail references below)",
                wire_name(&card.instruction.state),
                card.readiness.repair_ready
            )),
        },
    }
    if !card.allowed_files.is_empty() || !card.forbidden_files.is_empty() {
        lines.push(format!(
            "  edit cage: allowed=[{}] forbidden=[{}]",
            card.allowed_files.join(", "),
            card.forbidden_files.join(", ")
        ));
    }
    lines.push(done_when_line(&card.done_when));
    for stop in &card.stop_conditions {
        lines.push(format!(
            "  stop: {}",
            crate::output::human::plain_repair_card_user_wording(stop)
        ));
    }
    // Group only references with identical destination, state and reason.
    // Each family remains named, and a stale or missing reference cannot be
    // merged into a current one merely because its route is the same.
    let same_detail = |a: &crate::domain::RepairCardDetailRef,
                       b: &crate::domain::RepairCardDetailRef| {
        a.state == b.state && a.route == b.route && a.unavailable_reason == b.unavailable_reason
    };
    for (index, reference) in card.detail_references.iter().enumerate() {
        if card.detail_references[..index]
            .iter()
            .any(|earlier| same_detail(earlier, reference))
        {
            continue;
        }
        let families = card.detail_references[index..]
            .iter()
            .filter(|other| same_detail(other, reference))
            .map(|other| wire_name(&other.family))
            .collect::<Vec<_>>()
            .join(", ");
        let state = wire_name(&reference.state);
        match &reference.route {
            Some(route) => lines.push(format!("  detail [{families}] ({state}): {route}")),
            None => lines.push(format!(
                "  detail [{families}] ({state}): unavailable ({})",
                reference
                    .unavailable_reason
                    .as_deref()
                    .unwrap_or("no reason recorded")
            )),
        }
    }
    // The canonical next-action block above already prints a `limitations:`
    // line; repeating its entries a few lines later reads as a second,
    // different set of caveats (#9033), so the closing line carries only the
    // card entries the block did not already say.
    let already_shown: &[String] = card
        .canonical_next_action
        .as_ref()
        .map_or(&[], |action| action.limitations());
    let card_only: Vec<&str> = card
        .limitations
        .iter()
        .filter(|limitation| !already_shown.contains(limitation))
        .map(String::as_str)
        .collect();
    if !card_only.is_empty() {
        lines.push(format!("  limitations: {}", card_only.join("; ")));
    }
    if let Some(attempt) = &card.attempt {
        lines.push(format!(
            "  attempt: {} ({})",
            attempt.attempt_id, attempt.state
        ));
    }
    // The closing packet command is the caller's: the CLI passes one bound
    // to the selected root so it runs when pasted from any directory (#3999).
    // The card's own typed next action and detail routes stay portable (#4666
    // portability contract) and are printed verbatim above.
    lines.push(format!("  full packet: {packet_command}"));
    // A path with an apostrophe needs doubled quotes in PowerShell, so the
    // bash line alone would split there. Same translator as every other
    // plain-text command surface.
    if let Some(form) = crate::output::markdown::powershell_text_variant(packet_command) {
        lines.push(format!("  (PowerShell) {form}"));
    }
    lines
}

/// The default human card: a short brief a first-time reader can act on.
/// Every line is projected from a typed card field; nothing is re-derived and
/// the JSON card is untouched. The ledger rows (producer, subject, non-claim,
/// detail routes, hashes) live in [`agent_card_prose_lines`] under
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
    let hold_with_next = hold.filter(|hold| hold.next.is_some());
    let started = card.attempt.is_some() && card.readiness.repair_ready && !refused;
    let ready = card.readiness.repair_ready && !refused;
    if let Some(hold) = hold_with_next {
        lines.push(format!("  Not ready to repair yet: {}.", hold.reason));
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
        _ if ready && !started && !own_route => {
            let command = start_command();
            lines.push(format!("  Next: {command}"));
            lines.extend(powershell_pair(&command));
        }
        // A started attempt with no stop of its own: the next step is the
        // attempt's, not "no ready command" (#11381).
        Some(action)
            if started
                && !own_route
                && action.stop().is_none_or(|stop| {
                    matches!(stop, crate::domain::NextActionStop::Unsupported { .. })
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
        lines.push(format!("  Repair in progress: {}", attempt.attempt_id));
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

/// Plain-language spelling of the snapshot currentness for the human summary.
/// The wire value stays in `--json`; the match is exhaustive so a new variant
/// must choose its wording.
fn snapshot_currentness_words(value: RepairCardSnapshotCurrentness) -> &'static str {
    match value {
        RepairCardSnapshotCurrentness::Current => "committed HEAD",
        RepairCardSnapshotCurrentness::AcceptedDirtyDraft => "your uncommitted working tree",
    }
}

fn instruction_state_words(value: FixInstructionState) -> &'static str {
    match value {
        FixInstructionState::FixSiteReady => "fix site found, no exact edit ready",
        FixInstructionState::StaticLimitation => "static limitation, no bounded repair route",
        FixInstructionState::Stale => "stale, refresh the analysis first",
        FixInstructionState::InspectOnly => "inspect only, nothing to fix",
        FixInstructionState::Unavailable => "no witness available",
    }
}

/// One sentence-fragment list per done-when axis, in plain words. The five
/// axes stay separate (#3166); the wire spellings remain in `--json`.
fn done_when_line(done: &RepairCardDoneWhen) -> String {
    let static_movement = match done.static_movement {
        StaticMovementGoal::ClosedBySelectedRoute => "the selected route closes the gap",
        StaticMovementGoal::ImprovedAccordingToSelectedRoute => {
            "the selected route improves the gap"
        }
    };
    let focused = match done.focused_test_execution {
        FocusedExecutionGoal::VerifiedPass => "the focused test passes",
        FocusedExecutionGoal::ExplicitlyNotRun => "ripr does not run the focused test",
    };
    let cage = match done.edit_cage {
        EditCageGoal::Compliant => "edits stay inside the edit cage",
    };
    let mutation = match done.mutation_confirmation {
        MutationConfirmationGoal::NotRequested => "mutation testing is not requested",
        MutationConfirmationGoal::OptionalSeparateConfirmation => {
            "mutation confirmation is optional and separate"
        }
    };
    let currentness = match done.currentness {
        CardCurrentnessGoal::Current => "the snapshot is current",
        CardCurrentnessGoal::AcceptedDirtyDraft => "the uncommitted draft snapshot is accepted",
    };
    format!("  done when: {static_movement}; {focused}; {cage}; {mutation}; {currentness}")
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
