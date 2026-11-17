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
//! `choices` carries the commands a selection refusal's owner already composed
//! from typed seam or attempt ids for its stderr listing (#14980).

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

/// One command a selection refusal offers, as its owner printed it on stderr.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RefusalChoice {
    /// What picking this choice selects, in plain words.
    pub(crate) label: String,
    /// The command that makes this choice, runnable as written in a POSIX
    /// shell.
    pub(crate) command: String,
    /// The PowerShell spelling, only when PowerShell needs a different line
    /// and an honest translation exists (#10978).
    pub(crate) powershell_command: Option<String>,
}

impl RefusalChoice {
    pub(crate) fn new(label: String, command: String) -> Self {
        let powershell_command = match crate::output::markdown::powershell_form(&command) {
            crate::output::markdown::PowershellForm::Translated(line) => Some(line),
            _ => None,
        };
        Self {
            label,
            command,
            powershell_command,
        }
    }
}

/// The listed choices of a selection refusal and how many exist in all; a
/// bounded listing has fewer `items` than `total`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RefusalChoices {
    pub(crate) total: usize,
    pub(crate) items: Vec<RefusalChoice>,
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
    /// What the caller can do about a typed repair/continue refusal
    /// (`retryable`, `terminal`, `needs_selection`, `not_started`, #14931).
    /// Only the standalone `cli_refusal` document carries one.
    pub(crate) disposition: Option<&'static str>,
    /// The commands a selection refusal offers (#14980). Only the standalone
    /// `cli_refusal` document carries them.
    pub(crate) choices: Option<RefusalChoices>,
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
            disposition: None,
            choices: None,
        }
    }

    fn error_value(&self) -> Value {
        let mut error = Map::new();
        error.insert("code".to_string(), json!(self.code));
        error.insert("message".to_string(), json!(self.message));
        if let Some(token) = &self.token {
            error.insert("token".to_string(), json!(token));
        }
        if let Some(disposition) = self.disposition {
            error.insert("disposition".to_string(), json!(disposition));
        }
        if let Some(choices) = &self.choices {
            let items: Vec<Value> = choices
                .items
                .iter()
                .map(|choice| {
                    let mut item = Map::new();
                    item.insert("label".to_string(), json!(choice.label));
                    item.insert("command".to_string(), json!(choice.command));
                    if let Some(form) = &choice.powershell_command {
                        item.insert("powershell_command".to_string(), json!(form));
                    }
                    Value::Object(item)
                })
                .collect();
            error.insert(
                "choices".to_string(),
                json!({ "total": choices.total, "items": items }),
            );
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
    runnable_command(remedy).map(|command| RefusalNext {
        purpose: PURPOSE_STATED_STEP,
        command: command.to_string(),
    })
}

/// The remedy's command when it is safe to run as-is (see
/// [`next_from_remedy`]); the one owner every refusal renderer asks.
pub(crate) fn runnable_command(remedy: &RefusalRemedy) -> Option<&str> {
    if remedy.kind.as_str() != "command"
        || remedy.conditional
        || remedy.prose_has_more
        || !remedy.fill_in.is_empty()
    {
        return None;
    }
    remedy.command.as_deref()
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
            disposition: None,
            choices: None,
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
        assert!(value["error"].get("choices").is_none());
        Ok(())
    }

    /// #14980: a selection refusal's choices render with their full count,
    /// and a PowerShell line only where PowerShell needs a different one.
    #[test]
    fn choices_render_total_items_and_only_a_differing_powershell_line() -> Result<(), String> {
        let block = RefusalBlock {
            code: "several_eligible_seams".to_string(),
            message: "no repair started; run one of the commands above".to_string(),
            token: None,
            next: None,
            disposition: Some("needs_selection"),
            choices: Some(RefusalChoices {
                total: 3,
                items: vec![
                    RefusalChoice::new("plain".to_string(), "ripr repair a --root .".to_string()),
                    RefusalChoice::new(
                        "apostrophe root".to_string(),
                        "ripr repair b --root '/tmp/it'\\''s'".to_string(),
                    ),
                ],
            }),
        };
        let value: Value = serde_json::from_str(&block.render_cli_refusal()?)
            .map_err(|error| error.to_string())?;
        let choices = &value["error"]["choices"];
        assert_eq!(choices["total"], 3);
        assert_eq!(choices["items"][0]["label"], "plain");
        assert_eq!(choices["items"][0]["command"], "ripr repair a --root .");
        assert!(choices["items"][0].get("powershell_command").is_none());
        let translated = choices["items"][1]["powershell_command"]
            .as_str()
            .ok_or("an apostrophe root needs a PowerShell line")?;
        assert_ne!(
            translated,
            choices["items"][1]["command"].as_str().unwrap_or_default()
        );
        assert_eq!(value["error"]["next"], Value::Null);
        Ok(())
    }
}
