//! The shared machine-readable refusal block (#11584).
//!
//! One shape answers "why did this not produce a result, and what do I run
//! next" for every command that refuses under a JSON request:
//!
//! ```json
//! { "status": "refused",
//!   "error": { "code": "...", "message": "...", "next": { "purpose": "...", "command": "..." } } }
//! ```
//!
//! `check` refusal documents carry the block beside their existing
//! `analysis_scope` / `run_limitations` fields, so a consumer branches on
//! `status` instead of inferring refusal from an empty `findings` list.
//! Commands with no result document (usage errors, selector misses, the
//! task-first `repair` / `continue` facade) emit the standalone
//! [`render_cli_refusal`] document. The code comes from the typed owner of
//! each refusal (`CoreError::refusal_notice`, the usage-message builders in
//! `cli::suggest`); nothing here inspects message text.

use crate::output::json::render_pretty_with_newline;
use serde_json::{Value, json};

/// Schema version of the standalone refusal document.
pub(crate) const CLI_REFUSAL_SCHEMA_VERSION: &str = "0.1";

/// `status` value that discriminates a refusal from a result.
pub(crate) const REFUSED_STATUS: &str = "refused";

/// The runnable command that answers a `check` repair route, or `None` when
/// the route has no single honest command. Closed over the routes
/// `CoreError::refusal_notice` and the scope guards produce; a route added
/// there must decide here (the table test enumerates them).
pub(crate) fn next_command_for_route(route: &str) -> Option<&'static str> {
    match route {
        "analysis/base-resolution" => Some("git fetch origin"),
        "analysis/diff-scope-budget" | "analysis/repo-scope-budget" => {
            Some("ripr check --help --all")
        }
        "analysis/repository-root"
        | "analysis/config-load"
        | "analysis/suppression-policy"
        | "analysis/check-artifact"
        | "analysis/gap-ledger"
        | "analysis/diff-input"
        | "analysis/git-timeout"
        | "analysis/timeout"
        | "analysis/failure" => Some("ripr doctor"),
        "analysis/seam-inventory" | "analysis/seam-policy" | "analysis/seam-limitation" => {
            Some("ripr pilot")
        }
        "analysis/selector" => Some("ripr check --json"),
        _ => None,
    }
}

/// The `error` object: a closed `code`, the human `message` (non-authority,
/// verbatim from stderr), and `next` (the one command to run, or `null`).
pub(crate) fn error_block(code: &str, message: &str, next_command: Option<&str>) -> Value {
    json!({
        "code": code,
        "message": message,
        "next": next_command.map(|command| json!({
            "purpose": "recover",
            "command": command,
        })),
    })
}

/// The inputs of one standalone refusal document.
pub(crate) struct CliRefusal<'a> {
    /// The command the user ran, as its first argv word (`check`, `repair`).
    pub(crate) command: Option<&'a str>,
    /// The process exit code the refusal maps to (2 or 3).
    pub(crate) exit_code: i32,
    pub(crate) code: &'a str,
    pub(crate) message: &'a str,
    pub(crate) next_command: Option<&'a str>,
}

/// Render the standalone refusal document (schema `cli-refusal`).
pub(crate) fn render_cli_refusal(refusal: &CliRefusal<'_>) -> Result<String, String> {
    let value = json!({
        "schema_version": CLI_REFUSAL_SCHEMA_VERSION,
        "kind": "cli_refusal",
        "tool": "ripr",
        "status": REFUSED_STATUS,
        "command": refusal.command,
        "exit_code": refusal.exit_code,
        "error": error_block(refusal.code, refusal.message, refusal.next_command),
    });
    render_pretty_with_newline(&value, "cli refusal")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_document_carries_discriminator_code_and_next_command() -> Result<(), String> {
        let rendered = render_cli_refusal(&CliRefusal {
            command: Some("check"),
            exit_code: 2,
            code: "unknown_argument",
            message: "unknown check argument \"--bogus\".",
            next_command: Some("ripr check --help --all"),
        })?;
        let value: Value = serde_json::from_str(&rendered).map_err(|err| err.to_string())?;
        for (pointer, expected) in [
            ("/status", "refused"),
            ("/kind", "cli_refusal"),
            ("/schema_version", CLI_REFUSAL_SCHEMA_VERSION),
            ("/command", "check"),
            ("/error/code", "unknown_argument"),
            ("/error/next/command", "ripr check --help --all"),
        ] {
            let actual = value.pointer(pointer).and_then(Value::as_str);
            if actual != Some(expected) {
                return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
            }
        }
        if value.pointer("/exit_code") != Some(&json!(2)) {
            return Err("exit_code must be 2".to_string());
        }
        Ok(())
    }

    #[test]
    fn missing_next_command_is_an_explicit_null() {
        let block = error_block("x", "y", None);
        assert_eq!(block.pointer("/next"), Some(&Value::Null));
    }

    #[test]
    fn every_check_repair_route_names_a_next_command() {
        for route in [
            "analysis/base-resolution",
            "analysis/repository-root",
            "analysis/config-load",
            "analysis/suppression-policy",
            "analysis/check-artifact",
            "analysis/gap-ledger",
            "analysis/seam-inventory",
            "analysis/seam-policy",
            "analysis/seam-limitation",
            "analysis/selector",
            "analysis/timeout",
            "analysis/diff-input",
            "analysis/git-timeout",
            "analysis/failure",
            "analysis/diff-scope-budget",
            "analysis/repo-scope-budget",
        ] {
            assert!(next_command_for_route(route).is_some(), "{route}");
        }
        assert_eq!(next_command_for_route("analysis/invented"), None);
    }
}
