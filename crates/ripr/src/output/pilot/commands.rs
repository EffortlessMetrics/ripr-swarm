use crate::agent::loop_commands::{self, display_path};
use crate::analysis::ClassifiedSeam;
use crate::analysis::repair_route::repair_packet_eligibility;
use crate::domain::{
    CanonicalNextActionV1, NextActionCurrentness, NextActionDiffSource, NextActionInput,
    NextActionPilotDelegation, NextActionProducer, NextActionStop, current_command_platform,
    select_canonical_next_action,
};
use crate::output::agent_seam_packets::{
    recommended_inline_test_module, recommended_test_for, recommended_test_is_repair_edit_target,
};
use crate::output::pilot::PilotSummaryContext;
use std::path::Path;

/// The documented start of the repair transaction for `entry` (#3906), or
/// `None` when the fail-closed repair-packet flip does not hold. The flip, not
/// route readiness alone, is the authority here: a wrong actionable repair
/// signal is worse than falling back to the snapshot comparison.
///
/// `agent repair` also refuses an edit target it cannot route, so the target
/// must pass the shared `recommended_test_is_repair_edit_target`: a test
/// surface, or the seam's own Rust file confined to its one governed inline
/// `#[cfg(test)]` module (#5210). Any other target gets no repair line:
/// offering one would send the user into a refusal.
///
/// Built here rather than in `agent::loop_commands`, whose file xtask includes
/// into its own tree: a template only pilot calls reads as dead code there.
fn offered_repair_start_route(root: &Path, entry: &ClassifiedSeam) -> Option<String> {
    if !repair_packet_eligibility(entry).eligible() {
        return None;
    }
    if !recommended_test_is_repair_edit_target(entry) {
        return None;
    }
    // `root_path_display` before `bound_root`, like the CLI recovery route:
    // `display_path` would turn a literal backslash in a Unix path into a
    // separator, so the pasted command would target a different directory
    // (#4287, #8992).
    Some(format!(
        "ripr agent repair --root {} --seam-id {} --phase before",
        loop_commands::shell_arg(&loop_commands::bound_root(
            &loop_commands::root_path_display(root)
        )),
        loop_commands::shell_arg(entry.seam.id().as_str()),
    ))
}

/// Project pilot's canonical action for one top seam. The delegated
/// check/repair transaction is bound only when the fail-closed flip offers a
/// repair route; otherwise pilot declares no delegation and the selector
/// answers with a typed limitation, never an invented route. Pilot reads no
/// pinned head, so the bound source is the fixed no-base committed mode and
/// the render consumes only the decision's stop.
pub(super) fn canonical_next_action_for_pilot(
    root: &Path,
    entry: &ClassifiedSeam,
) -> Result<CanonicalNextActionV1, String> {
    let root_display = display_path(root);
    let seam_id = entry.seam.id().as_str().to_string();
    let delegation =
        offered_repair_start_route(root, entry).map(|route| NextActionPilotDelegation {
            transaction_ref: seam_id.clone(),
            route,
        });
    let input = NextActionInput {
        producer: NextActionProducer::PilotDelegation,
        root: root_display,
        diff_source: NextActionDiffSource::Committed {
            base: None,
            head: None,
        },
        item_id: seam_id,
        check_item: None,
        card_item: None,
        item_candidates: Vec::new(),
        attempts: Vec::new(),
        currentness: NextActionCurrentness {
            head_expected: None,
            head_observed: None,
            config_expected: None,
            config_observed: None,
        },
        offered_command: None,
        route_admitted: true,
        route_refusal: None,
        missing_input: None,
        platform: current_command_platform(),
        limitation: None,
        limitation_route: None,
        detail_route: "ripr pilot".to_string(),
        transition_from: "pilot_top_seam".to_string(),
        transition_to: None,
        restart_route: "ripr pilot".to_string(),
        check_case: None,
        doctor_recovery: None,
        pilot_delegation: delegation,
        alternatives: Vec::new(),
        limitations: Vec::new(),
    };
    select_canonical_next_action(&input)
}

/// The repair-start line family pilot renders for `entry`. The canonical
/// decision selects it: a pilot-delegated stop renders its route, any other
/// outcome renders no repair line. A projection that cannot be built falls
/// back to the shared offered-route mapping the projection itself consumes,
/// so the render stays total and the two cannot disagree.
pub(super) fn repair_start_command(root: &Path, entry: &ClassifiedSeam) -> Option<String> {
    match canonical_next_action_for_pilot(root, entry) {
        Ok(action) => match action.stop() {
            Some(NextActionStop::PilotDelegated { route, .. }) => Some(route.clone()),
            _ => None,
        },
        Err(_) => offered_repair_start_route(root, entry),
    }
}

/// Where the focused test may go for `entry`'s repair, stated after the
/// "add the focused test" step: test files, or only new test functions inside
/// the inline test module the edit cage confines the repair to (#5210).
pub(super) fn repair_edit_scope(entry: &ClassifiedSeam) -> String {
    let recommended = recommended_test_for(entry);
    match recommended_inline_test_module(entry, &recommended.file) {
        Some(module) => format!(
            "(new test functions inside `mod {module}` of {} only; production code and existing tests stay unchanged)",
            recommended.file
        ),
        None => "(test files only)".to_string(),
    }
}

/// `ripr first-pr` for a Python repair card without its own receipt command.
/// First-pr builds the gap ledger and names the gap's receipt command; it must
/// run before the test edit, because once the edit closes the gap first-pr no
/// longer selects it.
pub(super) fn python_card_first_pr_command(root: &Path) -> String {
    // Same root rule as [`offered_repair_start_route`] (#8992): keep the
    // root's literal characters so the pasted command names this checkout.
    format!(
        "ripr first-pr --root {}",
        loop_commands::shell_arg(&loop_commands::bound_root(
            &loop_commands::root_path_display(root)
        ))
    )
}

/// Floor for a printed timeout retry: a 30s default that never
/// auto-extended still suggests two minutes, not 60s.
const PILOT_RETRY_MIN_MS: u64 = 120_000;

/// Interactive cap for a printed timeout retry. Doubling stays under
/// this bound; a spent budget already at or above it is kept (#5443).
const PILOT_RETRY_INTERACTIVE_CAP_MS: u64 = 300_000;

/// Suggested `--timeout-ms` after a timeout.
///
/// Doubles the spent budget, keeps the 120s floor for short runs, and
/// caps at five minutes. The suggestion is never smaller than the time
/// already spent (#5443), so a 270s default-plus-extension run suggests
/// 300s instead of 1080s (#7427).
pub(super) fn suggested_retry_timeout_ms(spent_ms: u64) -> u64 {
    spent_ms
        .saturating_mul(2)
        .min(PILOT_RETRY_INTERACTIVE_CAP_MS)
        .max(spent_ms)
        .max(PILOT_RETRY_MIN_MS)
}

pub(super) struct PilotCommands {
    pub(super) after_snapshot: String,
    pub(super) outcome: String,
    pub(super) retry: String,
}

impl PilotCommands {
    pub(super) fn new(context: PilotSummaryContext<'_>) -> Self {
        // `--out` resolves against the working directory pilot ran in (its
        // default sits under `--root`, #5324), so every pilot path a
        // follow-up command names is bound against that directory once, as
        // is the root (#4000): pasted from any directory, the commands read
        // and write the files this run produced.
        let out_dir = loop_commands::bound_root_path(
            context
                .artifacts
                .pilot_summary_json
                .parent()
                .unwrap_or_else(|| Path::new(".")),
        );
        let after_path = out_dir.join("after.repo-exposure.json");
        // `--root` binds the same way the repair start and first-pr routes do
        // (#8992): the root keeps its literal characters before `bound_root`,
        // so a pasted retry or snapshot command names this checkout.
        let command_root =
            loop_commands::bound_root(&loop_commands::root_path_display(context.root));
        let after_snapshot = loop_commands::check_repo_exposure_command(
            &command_root,
            context.mode.as_str(),
            &display_path(&after_path),
        );
        let outcome = loop_commands::outcome_command(
            &display_path(&loop_commands::bound_root_path(
                &context.artifacts.repo_exposure_json,
            )),
            &display_path(&after_path),
            None,
        );
        let retry_timeout_ms = suggested_retry_timeout_ms(context.timeout_ms);
        let retry = format!(
            "ripr pilot --root {} --out {} --mode {} --max-seams {} --timeout-ms {}",
            loop_commands::shell_arg(&command_root),
            loop_commands::shell_path(&out_dir),
            context.mode.as_str(),
            context.max_seams,
            retry_timeout_ms
        );
        Self {
            after_snapshot,
            outcome,
            retry,
        }
    }
}
