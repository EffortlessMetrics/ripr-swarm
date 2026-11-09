//! The standalone `cli_refusal` document for argv usage errors and selector
//! misses under an explicit JSON request (#11584, #6834 decision).
//!
//! An agent that asked for JSON parses stdout. A usage error used to leave it
//! empty, so the agent scraped stderr prose to learn which flag or value was
//! wrong. The decision revises "usage errors stay prose-only" for runs that
//! explicitly request JSON (`--json` or a JSON `--format`), and only there:
//!
//! - exit code and stderr prose are unchanged;
//! - without a JSON request stdout stays empty exactly as before;
//! - the document is the shared refusal block (`output::refusal`) plus
//!   `schema_version` and `tool`, with the offending token and a `next`
//!   command. No argv echo, no partial analysis.
//!
//! The classification is typed at the source. The shared message builders
//! (`suggest::unknown_argument`, `suggest::unknown_value`, the unknown-command
//! error, the context selector miss) call `note_*` with the code and token
//! they already know; nothing here inspects message text to decide what kind
//! of failure it is. The boundary emits only when the failure that actually
//! escaped the command carries the noted message, so a builder whose result
//! was discarded, or a failure of another kind, can never be mislabeled.

use crate::cli::CommandError;
use crate::output::refusal::{
    PURPOSE_LIST_ARGUMENTS, PURPOSE_LIST_COMMANDS, RefusalBlock, RefusalNext,
};
use std::cell::RefCell;

/// Machine code of an unrecognized flag or stray positional.
pub(crate) const CODE_UNKNOWN_ARGUMENT: &str = "unknown_argument";
/// Machine code of an unrecognized enum-style value (`--format yaml`).
pub(crate) const CODE_UNKNOWN_VALUE: &str = "unknown_value";
/// Machine code of an unrecognized top-level command.
pub(crate) const CODE_UNKNOWN_COMMAND: &str = "unknown_command";
/// Machine code of a `--at`/`--finding` selector that matched no finding.
pub(crate) const CODE_SELECTOR_MISS: &str = "selector_miss";

thread_local! {
    /// The most recent usage-class refusal a builder noted on this thread.
    static NOTED: RefCell<Option<RefusalBlock>> = const { RefCell::new(None) };
}

/// Record a usage-class refusal at the point its message is built.
fn record(block: RefusalBlock) {
    NOTED.with(|noted| *noted.borrow_mut() = Some(block));
}

fn take() -> Option<RefusalBlock> {
    NOTED.with(|noted| noted.borrow_mut().take())
}

/// Forget anything noted by an earlier run on this thread.
pub(super) fn reset() {
    let _ = take();
}

/// Note an unknown flag or positional. `pointer` is the command's help route.
pub(crate) fn note_unknown_argument(token: &str, message: &str, pointer: &str) {
    record(RefusalBlock {
        code: CODE_UNKNOWN_ARGUMENT.to_string(),
        message: message.to_string(),
        token: Some(token.to_string()),
        next: Some(RefusalNext {
            purpose: PURPOSE_LIST_ARGUMENTS,
            command: pointer.to_string(),
        }),
    });
}

/// Note an unknown enum-style value. `pointer` is the help command when the
/// builder knows it; otherwise the boundary fills it from argv.
pub(crate) fn note_unknown_value(token: &str, message: &str, pointer: Option<&str>) {
    record(RefusalBlock {
        code: CODE_UNKNOWN_VALUE.to_string(),
        message: message.to_string(),
        token: Some(token.to_string()),
        next: pointer.map(|command| RefusalNext {
            purpose: PURPOSE_LIST_ARGUMENTS,
            command: command.to_string(),
        }),
    });
}

/// Note an unknown top-level command.
pub(crate) fn note_unknown_command(token: &str, message: &str) {
    record(RefusalBlock {
        code: CODE_UNKNOWN_COMMAND.to_string(),
        message: message.to_string(),
        token: Some(token.to_string()),
        next: Some(RefusalNext {
            purpose: PURPOSE_LIST_COMMANDS,
            command: "ripr --help".to_string(),
        }),
    });
}

/// Note a selector that matched no finding. The miss prose embeds the
/// caller's own selector text, so no `next` is ever lifted from it: the field
/// stays `null` (fail closed, #11584).
pub(crate) fn note_selector_miss(selector: &str, message: &str) {
    record(RefusalBlock {
        code: CODE_SELECTOR_MISS.to_string(),
        message: message.to_string(),
        token: Some(selector.to_string()),
        next: None,
    });
}

/// Whether argv explicitly requests JSON: a `--json` token, or `--format`
/// (either spelling) naming a JSON format. SARIF is its own format and is not
/// a JSON request.
pub(super) fn json_requested(args: &[String]) -> bool {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--json" {
            return true;
        }
        let value = match arg.strip_prefix("--format=") {
            Some(value) => Some(value),
            None if arg == "--format" => iter.next().map(String::as_str),
            None => None,
        };
        if value.is_some_and(is_json_format) {
            return true;
        }
    }
    false
}

/// JSON-ness comes from the format owner (`OutputFormat`), not a second
/// hand-kept list. Every other command's `--format` JSON spelling is `json`,
/// which the owner also parses.
fn is_json_format(value: &str) -> bool {
    crate::output::format::OutputFormat::parse_cli_name(value)
        .is_some_and(|format| format.is_json_request())
}

/// The first non-flag argv token, when it names a command with help.
fn argv_command(args: &[String]) -> Option<&str> {
    let first = args
        .iter()
        .skip(1)
        .map(String::as_str)
        .find(|arg| !arg.starts_with('-'))?;
    crate::cli::help::help_text_for(first).map(|_| first)
}

/// Render the `cli_refusal` document for a failed run, when one is owed.
///
/// `None` unless JSON was explicitly requested, the failure is a could-not-
/// complete error, and the noted usage-class refusal is the failure that
/// escaped. Consumes the note either way.
pub(super) fn document_for(args: &[String], error: &CommandError) -> Option<String> {
    let mut block = take()?;
    if !matches!(error, CommandError::Failure(_)) || !json_requested(args) {
        return None;
    }
    if !error.message().contains(&block.message) {
        return None;
    }
    // The document's message is the stderr prose the process printed.
    block.message = error.message().to_string();
    if block.next.is_none()
        && block.code == CODE_UNKNOWN_VALUE
        && let Some(command) = argv_command(args)
    {
        block.next = Some(RefusalNext {
            purpose: PURPOSE_LIST_ARGUMENTS,
            command: crate::cli::suggest::help_pointer(command),
        });
    }
    block.render_cli_refusal().ok()
}

/// Write the document owed for a failed run to stdout. A render or write
/// failure leaves stdout empty: the prose error is still delivered.
pub(super) fn emit_for(args: &[String], error: &CommandError) {
    if let Some(document) = document_for(args, error) {
        let _ = crate::cli::commands_context::write_stdout_refusal_json(document);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn json_request_is_explicit_json_or_a_json_format() {
        for yes in [
            &["ripr", "check", "--json"][..],
            &["ripr", "check", "--format", "json"],
            &["ripr", "check", "--format=json-compact"],
            &["ripr", "check", "--format", "json-terse"],
            &["ripr", "check", "--format", "check-findings-json"],
            &["ripr", "check", "--format", "agent-seam-packets-json"],
            &["ripr", "--json", "chekc"],
        ] {
            assert!(json_requested(&args(yes)), "{yes:?}");
        }
        for no in [
            &["ripr", "check"][..],
            &["ripr", "check", "--format", "human"],
            &["ripr", "check", "--format", "sarif"],
            &["ripr", "check", "--format", "badge-shields"],
            &["ripr", "check", "--format", "yaml-json"],
            &["ripr", "check", "--format"],
            &["ripr", "check", "--formatjson"],
        ] {
            assert!(!json_requested(&args(no)), "{no:?}");
        }
    }

    #[test]
    fn document_requires_the_noted_message_to_be_the_escaped_failure() {
        let message = "unknown command \"chekc\". Run `ripr --help`.";
        reset();
        note_unknown_command("chekc", message);
        let other = CommandError::Failure("some other failure".to_string());
        assert_eq!(document_for(&args(&["ripr", "--json"]), &other), None);

        // No JSON request: nothing is written, and the note is consumed.
        note_unknown_command("chekc", message);
        let same = CommandError::Failure(message.to_string());
        assert_eq!(document_for(&args(&["ripr", "chekc"]), &same), None);

        note_unknown_command("chekc", message);
        let document = document_for(&args(&["ripr", "chekc", "--json"]), &same);
        assert!(document.is_some_and(|text| text.contains("\"unknown_command\"")));
        // The note is consumed: a second failure cannot reuse it.
        assert_eq!(document_for(&args(&["ripr", "--json"]), &same), None);
    }

    #[test]
    fn decision_exits_never_become_usage_documents() {
        reset();
        note_unknown_command("x", "unknown command \"x\".");
        let decision = CommandError::Decision("unknown command \"x\".".to_string());
        assert_eq!(document_for(&args(&["ripr", "--json"]), &decision), None);
    }
}
