//! Renderer working-directory projection for checked-in expectations.
//!
//! Issue #3872 anchors funnel redirect targets at the resolved `--root`, so
//! rendered commands embed the renderer machine directory. Checked-in
//! expectations pin the anchored shape with a `<cwd>/` placeholder for that
//! prefix — never a real machine directory. The spelling and the projection
//! rule live here so every golden, corpus, and fixture projection agrees.
//! (This module — not a path-included production file — is the home for the
//! helpers: `xtask` compiles `agent::loop_commands` into its own crate via
//! `#[path]`, where cross-module test callers do not exist.)

use crate::agent::loop_commands::display_path;
use std::path::PathBuf;

/// The machine prefix that anchored redirect targets embed: the renderer
/// working directory with stable separators and a trailing slash.
///
/// An unreadable working directory degrades to a prefix that matches
/// nothing, so the projection becomes a no-op and the comparison fails
/// loudly instead of passing on unprojected machine paths.
#[cfg(test)]
pub(crate) fn renderer_cwd_prefix() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    format!("{}/", display_path(&cwd))
}

/// Replace every renderer-working-directory prefix inside `value` with
/// `<cwd>/`, recursively. The anchor sits mid-string inside rendered
/// commands, so this projects occurrences, not just a leading prefix.
#[cfg(test)]
pub(crate) fn project_renderer_cwd(value: &mut serde_json::Value) {
    let prefix = renderer_cwd_prefix();
    project_cwd_prefix(value, &prefix);
}

#[cfg(test)]
fn project_cwd_prefix(value: &mut serde_json::Value, prefix: &str) {
    match value {
        serde_json::Value::String(text) if text.contains(prefix) => {
            *text = text.replace(prefix, "<cwd>/");
        }
        serde_json::Value::String(_) => {}
        serde_json::Value::Array(items) => items
            .iter_mut()
            .for_each(|item| project_cwd_prefix(item, prefix)),
        serde_json::Value::Object(map) => map
            .values_mut()
            .for_each(|item| project_cwd_prefix(item, prefix)),
        _ => {}
    }
}
