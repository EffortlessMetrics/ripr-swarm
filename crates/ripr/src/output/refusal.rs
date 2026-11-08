//! The shared refusal block (#11584).
//!
//! One owner for the machine-readable "this run was refused" marker that every
//! JSON refusal document carries:
//!
//! ```json
//! { "status": "refused",
//!   "error": { "code": "...", "message": "...", "next": { "purpose": "...", "command": "..." } | null } }
//! ```
//!
//! `check` refusal documents (scope guards and typed failures) carry the block
//! beside their existing fields; a completed run carries neither key, so a
//! consumer can branch on `status` without reading the exit code. The
//! standalone `cli_refusal` document (argv usage errors and selector misses
//! under an explicit JSON request, #6834 decision) is this block plus a
//! schema version and tool identity, nothing else: no argv echo and no partial
//! analysis.
//!
//! The block never composes a command. `next` is only a runnable `ripr ...`
//! step the refusal prose already states, copied verbatim by
//! [`RefusalRemedy`], or a fixed help pointer the usage-error owner supplies.

use crate::domain::RefusalRemedy;
use crate::output::json::render_pretty_with_newline;
use serde_json::{Map, Value, json};

/// Stable `status` spelling of a refused run.
pub(crate) const REFUSED_STATUS: &str = "refused";

/// Purpose of a step lifted from refusal prose.
pub(crate) const PURPOSE_STATED_STEP: &str = "follow_stated_step";
/// Purpose of a help pointer after an unknown or misplaced argument or value.
pub(crate) const PURPOSE_LIST_ARGUMENTS: &str = "list_valid_arguments";
/// Purpose of a help pointer after an unknown command.
pub(crate) const PURPOSE_LIST_COMMANDS: &str = "list_valid_commands";

/// The one next step a refusal offers: a runnable command and why to run it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RefusalNext {
    /// Fixed vocabulary naming what the command is for.
    pub(crate) purpose: &'static str,
    /// A command the consumer can run as written.
    pub(crate) command: String,
}

/// The shared refusal block's content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RefusalBlock {
    pub(crate) code: String,
    pub(crate) message: String,
    /// The offending argv token or selector, when naming it helps. Only the
    /// standalone `cli_refusal` document carries one.
    pub(crate) token: Option<String>,
    pub(crate) next: Option<RefusalNext>,
}

impl RefusalBlock {
    /// Block for a refusal whose next step (if any) is whatever its prose
    /// states.
    pub(crate) fn from_prose(code: &str, message: &str) -> Self {
        Self::from_remedy(code, message, &RefusalRemedy::from_prose(message))
    }

    /// Block reusing a remedy a renderer already derived from the same prose.
    pub(crate) fn from_remedy(code: &str, message: &str, remedy: &RefusalRemedy) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            token: None,
            next: next_from_remedy(remedy),
        }
    }

    fn error_value(&self) -> Value {
        let mut error = Map::new();
        error.insert("code".to_string(), json!(self.code));
        error.insert("message".to_string(), json!(self.message));
        if let Some(token) = &self.token {
            error.insert("token".to_string(), json!(token));
        }
        let next = match &self.next {
            Some(next) => json!({ "purpose": next.purpose, "command": next.command }),
            None => Value::Null,
        };
        error.insert("next".to_string(), next);
        Value::Object(error)
    }

    /// Add `status` and `error` beside the keys a refusal document already
    /// carries. A non-object value is left untouched.
    pub(crate) fn insert_into(&self, document: &mut Value) {
        if let Value::Object(map) = document {
            map.insert("status".to_string(), json!(REFUSED_STATUS));
            map.insert("error".to_string(), self.error_value());
        }
    }

    /// The standalone `cli_refusal` document for a failure with no analysis
    /// document to carry the block.
    pub(crate) fn render_cli_refusal(&self) -> Result<String, String> {
        let mut document = json!({
            "schema_version": crate::app::CLI_REFUSAL_SCHEMA_VERSION,
            "tool": "ripr",
        });
        self.insert_into(&mut document);
        render_pretty_with_newline(&document, "cli refusal")
    }
}

/// A next step only when the remedy is a complete, unconditional command.
///
/// Fail-closed: a fill-in placeholder, a conditional fallback, or a command
/// that needs flags or environment the prose names elsewhere is not offered as
/// runnable. The richer `remedy` object stays beside it for the consumer that
/// wants the detail.
fn next_from_remedy(remedy: &RefusalRemedy) -> Option<RefusalNext> {
    if remedy.kind.as_str() != "command"
        || remedy.conditional
        || remedy.prose_has_more
        || !remedy.fill_in.is_empty()
    {
        return None;
    }
    remedy.command.as_ref().map(|command| RefusalNext {
        purpose: PURPOSE_STATED_STEP,
        command: command.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_carries_status_and_error_beside_existing_keys() {
        let mut document = json!({ "schema_version": "0.2", "findings": [] });
        RefusalBlock::from_prose("base_unresolvable", "base is gone; run `ripr check --help`")
            .insert_into(&mut document);
        assert_eq!(document["schema_version"], "0.2");
        assert_eq!(document["status"], "refused");
        assert_eq!(document["error"]["code"], "base_unresolvable");
        assert_eq!(document["error"]["next"]["command"], "ripr check --help");
        assert_eq!(document["error"]["next"]["purpose"], PURPOSE_STATED_STEP);
        assert!(document["error"].get("token").is_none());
    }

    #[test]
    fn prose_without_a_complete_command_yields_null_next() {
        for message in [
            "nothing to run here",
            "run `ripr check --base <ref>` after choosing a ref",
        ] {
            let block = RefusalBlock::from_prose("x", message);
            assert_eq!(block.next, None, "{message}");
            let mut document = json!({});
            block.insert_into(&mut document);
            assert_eq!(document["error"]["next"], Value::Null, "{message}");
        }
    }

    #[test]
    fn cli_refusal_document_is_minimal_and_names_the_token_only_when_given() -> Result<(), String> {
        let mut block = RefusalBlock {
            code: "unknown_argument".to_string(),
            message: "unknown check argument \"--bogus\".".to_string(),
            token: Some("--bogus".to_string()),
            next: Some(RefusalNext {
                purpose: PURPOSE_LIST_ARGUMENTS,
                command: "ripr check --help --all".to_string(),
            }),
        };
        let value: Value = serde_json::from_str(&block.render_cli_refusal()?)
            .map_err(|error| error.to_string())?;
        let mut keys: Vec<&str> = value
            .as_object()
            .ok_or("document must be an object")?
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["error", "schema_version", "status", "tool"]);
        assert_eq!(value["schema_version"], "0.1");
        assert_eq!(value["error"]["token"], "--bogus");
        block.token = None;
        let value: Value = serde_json::from_str(&block.render_cli_refusal()?)
            .map_err(|error| error.to_string())?;
        assert!(value["error"].get("token").is_none());
        Ok(())
    }
}
