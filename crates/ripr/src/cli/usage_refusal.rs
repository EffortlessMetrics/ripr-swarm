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
/// Machine code of a value flag given no value, or an empty one (#11979).
pub(crate) const CODE_MISSING_VALUE: &str = "missing_value";
/// Machine code of a required flag the invocation left out (#14076).
pub(crate) const CODE_MISSING_REQUIRED: &str = "missing_required";
/// Machine code of two flags the invocation gave that cannot be used
/// together (#14925).
pub(crate) const CODE_CONFLICTING_FLAGS: &str = "conflicting_flags";
/// Machine code of a value that does not have the shape its flag takes,
/// such as a word where a whole number belongs (#11979).
pub(crate) const CODE_INVALID_VALUE: &str = "invalid_value";

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

/// Note a value flag that arrived without a value (or with an empty one).
/// The token is the flag; the boundary fills the help route from argv.
pub(crate) fn note_missing_value(flag: &str, message: &str) {
    record(RefusalBlock {
        code: CODE_MISSING_VALUE.to_string(),
        message: message.to_string(),
        token: Some(flag.to_string()),
        next: None,
    });
}

/// The refusal for a value flag given an empty value, typed like a missing
/// one. Command parsers keep their own wording; only the code travels.
pub(crate) fn empty_value(flag: &str, message: &str) -> String {
    note_missing_value(flag, message);
    message.to_string()
}

/// The refusal for a required flag the invocation left out. The token is
/// the flag to add; command parsers keep their own wording.
pub(crate) fn missing_required(flag: &str, message: &str) -> String {
    record(RefusalBlock {
        code: CODE_MISSING_REQUIRED.to_string(),
        message: message.to_string(),
        token: Some(flag.to_string()),
        next: None,
    });
    message.to_string()
}

/// The refusal for two given flags that cannot be used together. The token
/// is one of them: dropping it clears this conflict. Command parsers keep
/// their own wording.
pub(crate) fn conflicting_flags(flag: &str, message: &str) -> String {
    record(RefusalBlock {
        code: CODE_CONFLICTING_FLAGS.to_string(),
        message: message.to_string(),
        token: Some(flag.to_string()),
        next: None,
    });
    message.to_string()
}

/// Note a value its flag cannot take. The token is the value as typed; an
/// empty value has no token to name. The boundary fills the help route.
pub(crate) fn note_invalid_value(value: &str, message: &str) {
    record(RefusalBlock {
        code: CODE_INVALID_VALUE.to_string(),
        message: message.to_string(),
        token: (!value.is_empty()).then(|| value.to_string()),
        next: None,
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

/// The command path argv names, when it has help: the two leading command
/// words (`agent status`) when that path has its own help, else the first.
fn argv_command(args: &[String]) -> Option<String> {
    // Dispatch strips the global `-v`/`--verbose` anywhere in argv, so the
    // command path is read the same way (`ripr agent -v status`).
    let mut words = args
        .iter()
        .skip(1)
        .map(String::as_str)
        .filter(|arg| *arg != "-v" && *arg != "--verbose")
        .skip_while(|arg| arg.starts_with('-'))
        .take_while(|arg| !arg.starts_with('-'));
    let first = words.next()?;
    if let Some(second) = words.next() {
        let path = format!("{first} {second}");
        if crate::cli::help::help_text_for(&path).is_some() {
            return Some(path);
        }
    }
    crate::cli::help::help_text_for(first).map(|_| first.to_string())
}

/// Codes whose builder does not know the command path, so the boundary
/// fills the help route from argv.
fn fills_help_route(code: &str) -> bool {
    matches!(
        code,
        CODE_UNKNOWN_VALUE
            | CODE_MISSING_VALUE
            | CODE_INVALID_VALUE
            | CODE_MISSING_REQUIRED
            | CODE_CONFLICTING_FLAGS
    )
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
        && fills_help_route(&block.code)
        && let Some(command) = argv_command(args)
    {
        block.next = Some(RefusalNext {
            purpose: PURPOSE_LIST_ARGUMENTS,
            command: crate::cli::suggest::help_pointer(&command),
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
    fn value_refusals_name_the_two_word_help_route_from_argv() -> Result<(), String> {
        let message = "missing value for --attempt; pass a value right after it";
        reset();
        note_missing_value("--attempt", message);
        let failure = CommandError::Failure(message.to_string());
        let document = document_for(
            &args(&["ripr", "agent", "status", "--json", "--attempt"]),
            &failure,
        )
        .ok_or("a JSON-requested missing value must write a document")?;
        let value: serde_json::Value =
            serde_json::from_str(&document).map_err(|err| err.to_string())?;
        assert_eq!(value["error"]["code"], CODE_MISSING_VALUE);
        assert_eq!(value["error"]["token"], "--attempt");
        assert_eq!(
            value["error"]["next"]["command"],
            "ripr agent status --help"
        );

        // A second word without its own help falls back to the command.
        note_missing_value("--attempt", message);
        let document = document_for(&args(&["ripr", "status", "nosuch", "--json"]), &failure)
            .ok_or("the fallback still writes a document")?;
        assert!(document.contains("\"ripr status --help\""), "{document}");

        // The global verbosity flag between command words is not a word.
        for verbose in ["-v", "--verbose"] {
            note_missing_value("--attempt", message);
            let document = document_for(
                &args(&["ripr", "agent", verbose, "status", "--json", "--attempt"]),
                &failure,
            )
            .ok_or("a verbose run still writes a document")?;
            assert!(
                document.contains("\"ripr agent status --help\""),
                "{verbose}: {document}"
            );
        }
        Ok(())
    }

    #[test]
    fn an_empty_invalid_value_carries_no_token() -> Result<(), String> {
        let message =
            "--top requires a positive integer; pass a whole number of 1 or more, got \"\"";
        reset();
        note_invalid_value("", message);
        let failure = CommandError::Failure(message.to_string());
        let document = document_for(&args(&["ripr", "swarm", "queue", "--json"]), &failure)
            .ok_or("a JSON-requested invalid value must write a document")?;
        let value: serde_json::Value =
            serde_json::from_str(&document).map_err(|err| err.to_string())?;
        assert_eq!(value["error"]["code"], CODE_INVALID_VALUE);
        assert!(value["error"].get("token").is_none(), "{value}");
        Ok(())
    }

    #[test]
    fn decision_exits_never_become_usage_documents() {
        reset();
        note_unknown_command("x", "unknown command \"x\".");
        let decision = CommandError::Decision("unknown command \"x\".".to_string());
        assert_eq!(document_for(&args(&["ripr", "--json"]), &decision), None);
    }
}
