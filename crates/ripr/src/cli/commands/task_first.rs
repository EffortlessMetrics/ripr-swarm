//! Task-first repair façade (#6305): `ripr repair`, `ripr continue`, and
//! `ripr status`.
//!
//! This is the CLI adapter layer only. Selection decisions live in
//! `crate::app::task_first`, the before/after/status execution in the shared
//! agent-route services, and rendering in the shared status/card/next-action
//! renderers. This module owns argument parsing, outcome dispatch, and the
//! three help bodies. `ripr agent repair` and `ripr agent status` stay
//! available as the advanced spellings of the same services.

use std::path::PathBuf;

use crate::app::task_first::{
    ContinueSelection, RepairStartDecision, RepairSubject, resolve_repair_start,
    resolve_repair_subject, select_continue_attempt,
};
use crate::cli::CommandError;
use crate::cli::agent::{
    AgentCardOptions, AgentRepairOptions, AgentRepairPhase, AgentStatusOptions,
};
use crate::cli::commands_context::ensure_command_root;
use crate::cli::parse::expect_value;
use crate::cli::suggest::unknown_argument;

pub(in crate::cli) const REPAIR_HELP: &str = r#"Start a repair attempt for one gap, or select the gap to repair.

Usage: ripr repair [<item>] [--root PATH]

Options:
  --root PATH      Workspace root. Defaults to current directory.

Without an item, repair starts only when exactly one repair-eligible seam
is visible under the selected root. Several eligible seams print a bounded
selection list with a retry command instead of selecting one implicitly;
no eligible seam prints the honest setup or limitation action. With an
item — a seam ID, or a canonical gap ID naming exactly one seam — repair
starts the before phase for that exact subject.

A successful start prints the before-phase summary and the compact repair
card for the seam. If the card cannot render after the start published,
repair refuses with exit 3 and empty stdout, naming the retained attempt
and its recovery instead of printing a partial success. Repair never edits
source files and never runs project verification; make the focused test
edit, then run `ripr continue`.

This is the ordinary route into the repair transaction. The advanced
spelling `ripr agent repair --seam-id ID --phase before` runs the same
before phase with explicit phase vocabulary."#;

pub(in crate::cli) const CONTINUE_HELP: &str = r#"Continue the current repair attempt through its after phase.

Usage: ripr continue [--attempt ID] [--root PATH]

Options:
  --attempt ID     Select one repair attempt by ID.
  --root PATH      Workspace root. Defaults to current directory.

Without an ID, continue runs only when exactly one current attempt is
eligible under the selected root. Several current attempts print a bounded
selection list with a retry command instead of selecting one implicitly;
no current attempt reports that honestly. An explicitly selected attempt
whose status confirms a compliant receipt reports its already-complete
status instead of running again; one that ended without a receipt (stale,
failed, or incomparable), and one whose finished state no compliant
receipt confirms (unissued, unavailable, or compatibility-only), reports
its state and refuses with exit 3 instead of claiming completion.

Continue runs the accepted after path: currentness admission, the edit
cage, verification composition, and the receipt. A stale head, drifted
analysis input, or refused verification ends in the canonical typed
refusal with its restart or recovery route.

This is the ordinary route through the repair transaction. The advanced
spelling `ripr agent repair --attempt ID --phase after` runs the same
after phase with explicit phase vocabulary."#;

pub(in crate::cli) const STATUS_HELP: &str = r#"Report the current repair state and the one next action.

Usage: ripr status [--attempt ID] [--root PATH] [--json]

Options:
  --attempt ID     Select exactly one repair attempt by ID and report its
                   typed state, currentness posture, and one exact next or
                   recovery action. Without it, status lists every attempt in
                   the store and selects a next command only when that choice
                   is unambiguous.
  --root PATH      Workspace root. Defaults to current directory.
  --json           Emit the machine-readable status report. Human Markdown is the default.

Status projects the repair-attempt state with its canonical next action.
Human and JSON forms derive from one semantic state. The command is
read-only: it never finishes, restarts, rewrites, or deletes an attempt.

This is the ordinary route to repair state. The advanced spelling
`ripr agent status` reports the same state with explicit agent vocabulary."#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RepairFacadeOptions {
    pub(super) root: PathBuf,
    pub(super) item: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ContinueFacadeOptions {
    pub(super) root: PathBuf,
    pub(super) attempt_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StatusFacadeOptions {
    pub(super) root: PathBuf,
    pub(super) attempt_id: Option<String>,
    pub(super) json: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RepairFacadeCommand {
    Help,
    Repair(RepairFacadeOptions),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ContinueFacadeCommand {
    Help,
    Continue(ContinueFacadeOptions),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StatusFacadeCommand {
    Help,
    Status(StatusFacadeOptions),
}

pub(super) fn parse_repair_args(args: &[String]) -> Result<RepairFacadeCommand, String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(RepairFacadeCommand::Help);
    }
    let mut root = PathBuf::from(".");
    let mut item: Option<String> = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = PathBuf::from(expect_value(args, index, "--root")?);
            }
            other if other.starts_with('-') => {
                return Err(unknown_argument("repair", other));
            }
            other => {
                if other.trim().is_empty() {
                    return Err("repair requires a non-empty item".to_string());
                }
                if item.is_some() {
                    return Err(format!(
                        "repair accepts one item at most; already selected `{}`",
                        item.as_deref().unwrap_or_default()
                    ));
                }
                item = Some(other.to_string());
            }
        }
        index += 1;
    }
    Ok(RepairFacadeCommand::Repair(RepairFacadeOptions {
        root,
        item,
    }))
}

pub(super) fn parse_continue_args(args: &[String]) -> Result<ContinueFacadeCommand, String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(ContinueFacadeCommand::Help);
    }
    let mut root = PathBuf::from(".");
    let mut attempt_id: Option<String> = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = PathBuf::from(expect_value(args, index, "--root")?);
            }
            "--attempt" => {
                index += 1;
                let value = expect_value(args, index, "--attempt")?;
                if value.trim().is_empty() {
                    return Err("continue --attempt requires a non-empty ID".to_string());
                }
                attempt_id = Some(value.to_string());
            }
            other => return Err(unknown_argument("continue", other)),
        }
        index += 1;
    }
    Ok(ContinueFacadeCommand::Continue(ContinueFacadeOptions {
        root,
        attempt_id,
    }))
}

pub(super) fn parse_status_args(args: &[String]) -> Result<StatusFacadeCommand, String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(StatusFacadeCommand::Help);
    }
    let mut root = PathBuf::from(".");
    let mut attempt_id: Option<String> = None;
    let mut json = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = PathBuf::from(expect_value(args, index, "--root")?);
            }
            "--attempt" => {
                index += 1;
                let value = expect_value(args, index, "--attempt")?;
                if value.trim().is_empty() {
                    return Err("status --attempt requires a non-empty ID".to_string());
                }
                attempt_id = Some(value.to_string());
            }
            "--json" => json = true,
            other => return Err(unknown_argument("status", other)),
        }
        index += 1;
    }
    Ok(StatusFacadeCommand::Status(StatusFacadeOptions {
        root,
        attempt_id,
        json,
    }))
}

pub(in crate::cli) fn repair(args: &[String]) -> Result<(), CommandError> {
    match parse_repair_args(args).map_err(CommandError::from)? {
        RepairFacadeCommand::Help => {
            println!("{REPAIR_HELP}");
            Ok(())
        }
        RepairFacadeCommand::Repair(options) => run_facade_repair(options),
    }
}

pub(in crate::cli) fn continue_repair(args: &[String]) -> Result<(), CommandError> {
    match parse_continue_args(args).map_err(CommandError::from)? {
        ContinueFacadeCommand::Help => {
            println!("{CONTINUE_HELP}");
            Ok(())
        }
        ContinueFacadeCommand::Continue(options) => run_facade_continue(options),
    }
}

pub(in crate::cli) fn status(args: &[String]) -> Result<(), CommandError> {
    match parse_status_args(args).map_err(CommandError::from)? {
        StatusFacadeCommand::Help => {
            println!("{STATUS_HELP}");
            Ok(())
        }
        StatusFacadeCommand::Status(options) => run_facade_status(options),
    }
}

fn run_facade_repair(options: RepairFacadeOptions) -> Result<(), CommandError> {
    ensure_command_root(&options.root, "repair")?;
    let seam_id = match options.item.as_deref() {
        Some(item) => {
            match resolve_repair_subject(&options.root, item).map_err(CommandError::from)? {
                RepairSubject::Seam(seam_id) => seam_id,
                RepairSubject::Decision(message) => return Err(CommandError::Decision(message)),
            }
        }
        None => match resolve_repair_start(&options.root).map_err(CommandError::from)? {
            RepairStartDecision::Start { seam_id } => seam_id,
            RepairStartDecision::NoEligible {
                total_seams,
                reasons,
            } => {
                // A deliberate named outcome, not a failure: the honest
                // setup/limitation message on stderr, stdout empty.
                return Err(CommandError::Decision(no_eligible_seam_message(
                    &options.root,
                    total_seams,
                    &reasons,
                )));
            }
            RepairStartDecision::Action(action) => {
                // A deliberate named outcome, not a failure: the canonical
                // block on stderr, stdout empty, decision exit code.
                eprint!(
                    "{}",
                    crate::output::next_action::render_next_action_human_at_root(
                        &action,
                        &options.root
                    )
                );
                if action.action_class() == crate::domain::NextActionClass::ChooseItem {
                    print_retry_line(&format!(
                        "ripr repair <seam-id> --root {}",
                        crate::agent::loop_commands::shell_arg(
                            &crate::agent::loop_commands::bound_root(
                                &options.root.to_string_lossy()
                            )
                        )
                    ));
                }
                return Err(CommandError::Decision(
                    "repair selected no seam; rerun with an explicit item".to_string(),
                ));
            }
        },
    };
    // The start's stdout is held until the handoff card renders too:
    // `cmd:repair` declares `EXIT_TYPED_REFUSAL_EMPTY_STDOUT`, so a card
    // failure after publication must leave stdout empty — a refusal
    // preceded by a success document would hand an orchestrator
    // success-shaped output on a retry that already started (#7032).
    let (attempt_id, before_stdout) = crate::cli::drive_before_phase_deferring_stdout(
        facade_before_options(&options.root, &seam_id),
    )?;
    // The compact handoff renders through the same card service the
    // advanced route serves. The attempt is already published here, so a
    // card failure must name the started attempt and its recovery
    // instead of implying no attempt exists.
    let card_stdout = super::agent_card::agent_card_stdout(AgentCardOptions {
        root: options.root.clone(),
        seam_id: seam_id.clone(),
        json: false,
    })
    .map_err(|error| {
        CommandError::Decision(crate::app::task_first::card_after_publish_message(
            &attempt_id,
            &seam_id,
            &crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root(
                &options.root.to_string_lossy(),
            )),
            error.message(),
        ))
    })?;
    print!("{before_stdout}");
    print!("{card_stdout}");
    Ok(())
}

fn facade_before_options(root: &std::path::Path, seam_id: &str) -> AgentRepairOptions {
    AgentRepairOptions {
        root: root.to_path_buf(),
        seam_id: Some(seam_id.to_string()),
        attempt_id: None,
        phase: AgentRepairPhase::Before,
        python_repair_trust: None,
        edit_authorization: crate::app::python_repair_binding::EditAuthorization {
            authorized: false,
            authority: None,
        },
        verify_authorization: crate::app::python_repair_verification::VerifyAuthorization {
            authorized: false,
            authority: None,
        },
        verify_rollback: false,
        store: None,
        json: false,
    }
}

fn run_facade_continue(options: ContinueFacadeOptions) -> Result<(), CommandError> {
    ensure_command_root(&options.root, "continue")?;
    match select_continue_attempt(&options.root, options.attempt_id.as_deref())
        .map_err(CommandError::from)?
    {
        ContinueSelection::Proceed { attempt_id } => super::agent::run_agent_repair_with_identity(
            AgentRepairOptions {
                root: options.root,
                seam_id: None,
                attempt_id: Some(attempt_id.as_str().to_string()),
                phase: AgentRepairPhase::After,
                python_repair_trust: None,
                edit_authorization: crate::app::python_repair_binding::EditAuthorization {
                    authorized: false,
                    authority: None,
                },
                verify_authorization: crate::app::python_repair_verification::VerifyAuthorization {
                    authorized: false,
                    authority: None,
                },
                verify_rollback: false,
                store: None,
                json: false,
            },
            None,
        ),
        ContinueSelection::AlreadyComplete { report } => {
            print!(
                "{}",
                crate::app::agent_status::render_agent_attempt_status_markdown(&report)
            );
            eprintln!(
                "ripr: repair attempt `{}` is already complete ({}); no new attempt was created",
                report.attempt.attempt_id, report.attempt.status_class
            );
            Ok(())
        }
        ContinueSelection::EndedUnsuccessfully { report } => {
            // The facts print like status but on stderr: this arm is a
            // typed refusal, and `cmd:continue` declares
            // `EXIT_TYPED_REFUSAL_EMPTY_STDOUT`, so stdout stays empty
            // while stderr carries both the state and the refusal.
            // A stale, failed, or incomparable attempt ended without a
            // receipt, so exit 3 and no completion claim.
            eprint!(
                "{}",
                crate::app::agent_status::render_agent_attempt_status_markdown(&report)
            );
            let root = crate::agent::loop_commands::shell_arg(
                &crate::agent::loop_commands::bound_root(&options.root.to_string_lossy()),
            );
            Err(CommandError::Decision(format!(
                "ripr: repair attempt `{}` ended {} without a receipt; no completion is claimed. Inspect: `ripr status --attempt {} --root {root}`; start fresh: `ripr repair --root {root}`",
                report.attempt.attempt_id, report.attempt.status_class, report.attempt.attempt_id
            )))
        }
        ContinueSelection::EndedUnconfirmed { report } => {
            // The after phase committed, but the status authority's
            // fail-closed projection cannot confirm a compliant receipt
            // (unissued, unbound, unavailable, or compatibility-only).
            // Like the unsuccessful arm, the facts print on stderr while
            // `cmd:continue` keeps stdout empty
            // (`EXIT_TYPED_REFUSAL_EMPTY_STDOUT`), exit 3, no completion
            // claim: a `ready_to_finish` manifest alone is not one.
            eprint!(
                "{}",
                crate::app::agent_status::render_agent_attempt_status_markdown(&report)
            );
            let root = crate::agent::loop_commands::shell_arg(
                &crate::agent::loop_commands::bound_root(&options.root.to_string_lossy()),
            );
            Err(CommandError::Decision(format!(
                "ripr: repair attempt `{}` ended `{}` without confirming completion; no completion is claimed. Inspect: `ripr status --attempt {} --root {root}`; start fresh when appropriate: `ripr repair --root {root}`",
                report.attempt.attempt_id, report.attempt.status_class, report.attempt.attempt_id
            )))
        }
        ContinueSelection::NoneAvailable {
            prepared,
            terminal,
            trust_bound,
        } => {
            let root = crate::agent::loop_commands::shell_arg(
                &crate::agent::loop_commands::bound_root(&options.root.to_string_lossy()),
            );
            Err(CommandError::Decision(format!(
                "no current attempt to continue under {root}: {}. Next: `ripr status --root {root}`; start with `ripr repair --root {root}`",
                none_available_detail(prepared, terminal, trust_bound, &root)
            )))
        }
        ContinueSelection::Ambiguous { candidates } => {
            let root = crate::agent::loop_commands::shell_arg(
                &crate::agent::loop_commands::bound_root(&options.root.to_string_lossy()),
            );
            const MAX_LISTED: usize = 8;
            let mut rendered = format!(
                "ripr: {} current attempts under {root}; select one explicitly:\n",
                candidates.len()
            );
            for id in candidates.iter().take(MAX_LISTED) {
                rendered.push_str(&format!("  {id}\n"));
            }
            if candidates.len() > MAX_LISTED {
                rendered.push_str(&format!("  (and {} more)\n", candidates.len() - MAX_LISTED));
            }
            eprint!("{rendered}");
            print_retry_line(&format!(
                "ripr continue --attempt <attempt-id> --root {root}"
            ));
            Err(CommandError::Decision(
                "continue selected no attempt; rerun with --attempt".to_string(),
            ))
        }
    }
}

fn no_eligible_seam_message(
    root: &std::path::Path,
    total_seams: usize,
    reasons: &[String],
) -> String {
    let root = crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root(
        &root.to_string_lossy(),
    ));
    if total_seams == 0 {
        return format!(
            "no seams visible under {root}; analyze the workspace before starting a repair. Next: `ripr pilot --root {root}`"
        );
    }
    let noun = if total_seams == 1 { "seam" } else { "seams" };
    let mut message = format!("{total_seams} visible {noun} under {root}, none repair-eligible");
    if !reasons.is_empty() {
        message.push_str(&format!(" ({})", reasons.join("; ")));
    }
    message.push_str(&format!(". Next: `ripr pilot --root {root}`"));
    message
}

fn none_available_detail(
    prepared: usize,
    terminal: usize,
    trust_bound: usize,
    root_display: &str,
) -> String {
    let mut detail = match (prepared, terminal, trust_bound) {
        (0, 0, 0) => "no attempts recorded".to_string(),
        // A trust-bound-only store is not empty: the awaiting rows are
        // named by the authorization route below, so "no attempts
        // recorded" would contradict it (#7032).
        (0, 0, _) => String::new(),
        (prepared, 0, 0) => format!("{prepared} prepared, none awaiting"),
        (0, terminal, 0) => format!("{terminal} terminal, none awaiting"),
        (prepared, terminal, 0) => {
            format!("{prepared} prepared and {terminal} terminal, none awaiting")
        }
        // With trust-bound rows also awaiting, "none awaiting" would
        // contradict the same route; name only the unbound rows here.
        (prepared, 0, _) => format!("{prepared} prepared"),
        (0, terminal, _) => format!("{terminal} terminal"),
        (prepared, terminal, _) => {
            format!("{prepared} prepared and {terminal} terminal")
        }
    };
    // A trust-bound attempt can never continue on this route: the
    // façade passes no edit authorization. Name the exact advanced
    // spelling instead of leaving a dead end.
    if trust_bound > 0 {
        let noun = if trust_bound == 1 {
            "1 awaiting attempt requires".to_string()
        } else {
            format!("{trust_bound} awaiting attempts require")
        };
        let lead = if detail.is_empty() { "" } else { "; " };
        detail.push_str(&format!(
            "{lead}{noun} edit authorization, which `ripr continue` cannot supply: run `ripr agent repair --attempt <id> --phase after --edit-authorized --edit-authority <identity> --root {root_display}` (IDs in `ripr status --root {root_display}`)"
        ));
    }
    detail
}

fn run_facade_status(options: StatusFacadeOptions) -> Result<(), CommandError> {
    super::agent::run_agent_status(AgentStatusOptions {
        out_dir: None,
        root: options.root,
        json: options.json,
        store: None,
        attempt_id: options.attempt_id,
    })
    .map_err(CommandError::from)
}

/// Print one retry template with its PowerShell pair when the translator
/// renders one. The template keeps a placeholder, never a filled
/// candidate: filling one would be the implicit selection the façade
/// refuses to make.
fn print_retry_line(retry: &str) {
    eprintln!("retry: {retry}");
    if let Some(form) = crate::output::markdown::powershell_text_variant(retry) {
        eprintln!("retry (PowerShell): {form}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn repair_parse_defaults_to_cwd_without_item() -> Result<(), String> {
        match parse_repair_args(&args(&[]))? {
            RepairFacadeCommand::Repair(options) => {
                if options.root != Path::new(".") || options.item.is_some() {
                    return Err(format!("unexpected repair defaults: {options:?}"));
                }
                Ok(())
            }
            RepairFacadeCommand::Help => Err("empty args must not print help".to_string()),
        }
    }

    #[test]
    fn repair_parse_accepts_item_and_root_in_any_order() -> Result<(), String> {
        for argv in [
            args(&["seam-1", "--root", "proj"]),
            args(&["--root", "proj", "seam-1"]),
        ] {
            match parse_repair_args(&argv)? {
                RepairFacadeCommand::Repair(options) => {
                    if options.root != Path::new("proj")
                        || options.item.as_deref() != Some("seam-1")
                    {
                        return Err(format!("bad repair parse: {options:?}"));
                    }
                }
                RepairFacadeCommand::Help => {
                    return Err("an item must not print help".to_string());
                }
            }
        }
        Ok(())
    }

    #[test]
    fn repair_parse_fails_closed() -> Result<(), String> {
        for argv in [
            args(&["one", "two"]),
            args(&[""]),
            args(&["   "]),
            args(&["--phase", "before"]),
            args(&["--root"]),
            args(&["--attempt", "id"]),
        ] {
            if parse_repair_args(&argv).is_ok() {
                return Err(format!("repair accepted {argv:?}"));
            }
        }
        for argv in [args(&["--help"]), args(&["-h"])] {
            if parse_repair_args(&argv)? != RepairFacadeCommand::Help {
                return Err(format!("{argv:?} must print help"));
            }
        }
        Ok(())
    }

    #[test]
    fn continue_parse_handles_identity_and_help() -> Result<(), String> {
        match parse_continue_args(&args(&["--attempt", "id-1", "--root", "proj"]))? {
            ContinueFacadeCommand::Continue(options) => {
                if options.root != Path::new("proj")
                    || options.attempt_id.as_deref() != Some("id-1")
                {
                    return Err(format!("bad continue parse: {options:?}"));
                }
            }
            ContinueFacadeCommand::Help => return Err("flags must not print help".to_string()),
        }
        match parse_continue_args(&args(&[]))? {
            ContinueFacadeCommand::Continue(options) => {
                if options.root != Path::new(".") || options.attempt_id.is_some() {
                    return Err(format!("unexpected continue defaults: {options:?}"));
                }
            }
            ContinueFacadeCommand::Help => {
                return Err("empty args must not print help".to_string());
            }
        }
        for argv in [args(&["--help"]), args(&["-h"])] {
            if parse_continue_args(&argv)? != ContinueFacadeCommand::Help {
                return Err(format!("{argv:?} must print help"));
            }
        }
        for argv in [
            args(&["positional"]),
            args(&["--attempt"]),
            args(&["--attempt", "  "]),
            args(&["--json"]),
        ] {
            if parse_continue_args(&argv).is_ok() {
                return Err(format!("continue accepted {argv:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn status_parse_handles_identity_json_and_help() -> Result<(), String> {
        match parse_status_args(&args(&["--attempt", "id-1", "--json"]))? {
            StatusFacadeCommand::Status(options) => {
                if !options.json || options.attempt_id.as_deref() != Some("id-1") {
                    return Err(format!("bad status parse: {options:?}"));
                }
            }
            StatusFacadeCommand::Help => return Err("flags must not print help".to_string()),
        }
        for argv in [args(&["--help"]), args(&["-h"])] {
            if parse_status_args(&argv)? != StatusFacadeCommand::Help {
                return Err(format!("{argv:?} must print help"));
            }
        }
        for argv in [
            args(&["positional"]),
            args(&["--attempt"]),
            args(&["--store", "x"]),
        ] {
            if parse_status_args(&argv).is_ok() {
                return Err(format!("status accepted {argv:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn facade_help_bodies_document_their_exact_surface() -> Result<(), String> {
        for (body, usage, flags) in [
            (
                REPAIR_HELP,
                "Usage: ripr repair [<item>] [--root PATH]",
                &["--root"][..],
            ),
            (
                CONTINUE_HELP,
                "Usage: ripr continue [--attempt ID] [--root PATH]",
                &["--attempt", "--root"][..],
            ),
            (
                STATUS_HELP,
                "Usage: ripr status [--attempt ID] [--root PATH] [--json]",
                &["--attempt", "--root", "--json"][..],
            ),
        ] {
            if !body.contains(usage) {
                return Err(format!("help body lost its usage line: {usage}"));
            }
            for flag in flags {
                if !body.contains(flag) {
                    return Err(format!("help body lost {flag}"));
                }
            }
        }
        Ok(())
    }

    /// Control 12 (catalog half): the façade rows are ordinary public and
    /// canonical; the agent spellings stay supported as advanced routes.
    #[test]
    fn facade_rows_are_the_ordinary_public_repair_route() -> Result<(), String> {
        use crate::cli::command_catalog::{
            CommandClass, CommandRelation, DiscoveryPosture, catalog,
        };
        for path in ["repair", "continue", "status"] {
            let Some(entry) = catalog().iter().find(|entry| entry.path == path) else {
                return Err(format!("catalog lost {path:?}"));
            };
            if entry.class != CommandClass::Public
                || entry.discovery != DiscoveryPosture::OrdinaryPublic
                || entry.relation != CommandRelation::Canonical
            {
                return Err(format!(
                    "{path:?} must be public/ordinary/canonical, got {:?}/{:?}/{:?}",
                    entry.class, entry.discovery, entry.relation
                ));
            }
        }
        for path in ["agent repair", "agent status"] {
            let Some(entry) = catalog().iter().find(|entry| entry.path == path) else {
                return Err(format!("catalog lost {path:?}"));
            };
            if entry.class != CommandClass::Advanced
                || entry.discovery != DiscoveryPosture::Advanced
                || entry.relation != CommandRelation::Canonical
            {
                return Err(format!(
                    "{path:?} must stay supported as an advanced route, got {:?}/{:?}/{:?}",
                    entry.class, entry.discovery, entry.relation
                ));
            }
        }
        Ok(())
    }

    /// Control 12 (routing half): no ordinary-public row may recommend an
    /// internal phase command as the ordinary next step. The `agent` family
    /// index is exempt: it is the advanced surface's own table of contents,
    /// so its internal routes are coherent.
    #[test]
    fn ordinary_rows_never_route_to_the_advanced_repair_spellings() -> Result<(), String> {
        use crate::cli::command_catalog::{DiscoveryPosture, catalog};
        use crate::cli::command_metadata::metadata;
        for row in metadata() {
            let Some(entry) = catalog().iter().find(|entry| entry.id == row.id) else {
                continue;
            };
            if entry.discovery != DiscoveryPosture::OrdinaryPublic || entry.path == "agent" {
                continue;
            }
            let mut routed = vec![row.example];
            routed.extend(row.next_routes.iter().copied());
            for route in routed {
                if route.contains("agent repair") || route.contains("agent status") {
                    return Err(format!(
                        "ordinary row {} routes to the advanced spelling: {route:?}",
                        row.id
                    ));
                }
            }
        }
        Ok(())
    }

    /// Control 12 (workflow half): the repair workflow starts at the façade
    /// and keeps the agent spellings as advanced alternatives.
    #[test]
    fn repair_workflow_starts_at_the_facade() -> Result<(), String> {
        use crate::cli::workflow_catalog::workflow_catalog;
        let Some(row) = workflow_catalog().iter().find(|row| row.id == "repair-gap") else {
            return Err("workflow catalog lost repair-gap".to_string());
        };
        if row.first_command != "cmd:repair" {
            return Err(format!(
                "repair-gap must start at cmd:repair, got {:?}",
                row.first_command
            ));
        }
        for expected in ["cmd:agent.repair", "cmd:agent.status"] {
            if !row.advanced_alternatives.contains(&expected) {
                return Err(format!(
                    "repair-gap lost the advanced alternative {expected}"
                ));
            }
        }
        Ok(())
    }

    /// Review repair (#7032): a trust-bound awaiting attempt names the
    /// exact advanced spelling, since `continue` cannot supply edit
    /// authorization. The unbound rows keep their legacy text, and a
    /// trust-bound store never reads as empty or awaiting-free.
    #[test]
    fn none_available_detail_names_the_trust_bound_route() -> Result<(), String> {
        if none_available_detail(0, 0, 0, "ROOT") != "no attempts recorded" {
            return Err("empty detail must keep its legacy text".to_string());
        }
        if none_available_detail(1, 2, 0, "ROOT") != "1 prepared and 2 terminal, none awaiting" {
            return Err("unbound detail must keep its legacy text".to_string());
        }
        for (prepared, terminal, trust_bound) in [(0, 0, 1), (0, 0, 2), (1, 0, 1), (0, 2, 1)] {
            let detail = none_available_detail(prepared, terminal, trust_bound, "ROOT");
            if detail.contains("no attempts recorded") {
                return Err(format!(
                    "a store with {trust_bound} trust-bound rows is not empty:\n{detail}"
                ));
            }
            if trust_bound > 0 && prepared + terminal > 0 && detail.contains("none awaiting") {
                return Err(format!(
                    "the awaiting trust-bound rows contradict \"none awaiting\":\n{detail}"
                ));
            }
        }
        let detail = none_available_detail(0, 0, 1, "ROOT");
        for expected in [
            "1 awaiting attempt requires edit authorization",
            "ripr agent repair --attempt <id> --phase after --edit-authorized --edit-authority <identity> --root ROOT",
            "ripr status --root ROOT",
        ] {
            if !detail.contains(expected) {
                return Err(format!(
                    "trust-bound detail must carry {expected:?}:\n{detail}"
                ));
            }
        }
        let mixed = none_available_detail(1, 2, 1, "ROOT");
        for expected in ["1 prepared and 2 terminal", "1 awaiting attempt requires"] {
            if !mixed.contains(expected) {
                return Err(format!("mixed detail must carry {expected:?}:\n{mixed}"));
            }
        }
        let plural = none_available_detail(0, 0, 2, "ROOT");
        if !plural.contains("2 awaiting attempts require edit authorization") {
            return Err(format!("trust-bound detail must pluralize:\n{plural}"));
        }
        Ok(())
    }

    /// Owns a temp test root so a mid-test failure cleans up instead of
    /// leaking the directory; mirrors the app-level facade guard.
    struct TempRootGuard {
        root: std::path::PathBuf,
    }

    impl Drop for TempRootGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Review repair (#7032, post-start card refusal): the façade composes
    /// its stdout from `drive_before_phase_deferring_stdout` plus
    /// `agent_card_stdout`, printing only when both succeed. This control
    /// drives that exact composition against a real fixture: a card
    /// refusal after publication surfaces as a typed decision naming the
    /// seam, the held document was never printed, and the attempt stays
    /// inspectable in the store. A card failure cannot be injected from
    /// outside one process, so the composition — not a spawned CLI — is
    /// what is under test here; the refusal message and the store facts
    /// are pinned end to end by the facade integration suite.
    #[test]
    fn facade_holds_stdout_until_the_card_renders() -> Result<(), String> {
        use crate::testing::fixture_git::fixture_git_ok as run_git;

        const FIXTURE_CARGO_TOML: &str = "[package]\nname = \"boundary_gap_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"boundary_gap_fixture\"\npath = \"src/lib.rs\"\n";
        const FIXTURE_LIB: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
        const FIXTURE_WEAK_TEST: &str = "use boundary_gap_fixture::discounted_total;\n\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n";
        // The boundary fixture's stable seam identity.
        const FIXTURE_SEAM: &str = "67fc764ba37d77bd";

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("test clock failed: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-task-first-card-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create src failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create tests failed: {error}"))?;
        std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML)
            .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), FIXTURE_LIB)
            .map_err(|error| format!("write lib.rs failed: {error}"))?;
        std::fs::write(root.join("tests/pricing.rs"), FIXTURE_WEAK_TEST)
            .map_err(|error| format!("write pricing.rs failed: {error}"))?;
        let root = root
            .canonicalize()
            .map_err(|error| format!("canonicalize {} failed: {error}", root.display()))?;
        let _guard = TempRootGuard { root: root.clone() };
        run_git(&root, &["init"])?;
        // Ordinary Rust repair fixtures meet the build-output precondition
        // through the local Git exclude, like the facade integration suite.
        std::fs::write(root.join(".git/info/exclude"), "/target/\n")
            .map_err(|error| format!("write git exclude: {error}"))?;
        run_git(
            &root,
            &["config", "user.email", "ripr-test@example.invalid"],
        )?;
        run_git(&root, &["config", "user.name", "RIPR Test"])?;
        run_git(&root, &["config", "core.autocrlf", "false"])?;
        run_git(&root, &["add", "."])?;
        run_git(&root, &["commit", "--no-gpg-sign", "-m", "fixture"])?;

        // The start holds its document: it comes back as a value, so
        // nothing reaches stdout before the card has rendered.
        let (attempt_id, before_stdout) = crate::cli::drive_before_phase_deferring_stdout(
            facade_before_options(&root, FIXTURE_SEAM),
        )
        .map_err(|error| error.to_string())?;
        if before_stdout.trim().is_empty() {
            return Err("the held before-phase stdout must not be empty".to_string());
        }
        if !before_stdout.contains("Next, after the test edit:") {
            return Err(format!(
                "the held stdout must carry the after-step pointer:\n{before_stdout}"
            ));
        }
        let attempts_in_store = || -> Result<usize, String> {
            Ok(crate::app::repair_attempt::inventory_repair_attempts_from(&root, None)?.len())
        };
        if attempts_in_store()? != 1 {
            return Err("the start must publish exactly one attempt".to_string());
        }

        // Remove the seam from the live tree, so the card's fresh
        // inventory no longer finds it: a refusal after publication, the
        // window the façade must survive with stdout empty.
        std::fs::write(root.join("src/lib.rs"), "// no public behavior yet\n")
            .map_err(|error| format!("rewrite lib.rs failed: {error}"))?;
        match crate::cli::commands::agent_card::agent_card_stdout(AgentCardOptions {
            root: root.clone(),
            seam_id: FIXTURE_SEAM.to_string(),
            json: false,
        }) {
            Ok(_) => return Err("a vanished seam must refuse the card".to_string()),
            Err(error) => {
                if error.exit_code() != crate::cli::EXIT_DECISION_OR_REFUSAL {
                    return Err(format!(
                        "the post-publication card refusal must be a typed decision, got exit {}: {error}",
                        error.exit_code()
                    ));
                }
                if !error.message().contains(FIXTURE_SEAM) {
                    return Err(format!("the card refusal must name the seam:\n{error}"));
                }
            }
        }
        if attempts_in_store()? != 1 {
            return Err("a refused card must retain the started attempt".to_string());
        }
        let named = crate::app::task_first::card_after_publish_message(
            &attempt_id,
            FIXTURE_SEAM,
            "ROOT",
            "card refused",
        );
        if !named.contains(&attempt_id) {
            return Err("the refusal message must name the started attempt".to_string());
        }
        Ok(())
    }
}
