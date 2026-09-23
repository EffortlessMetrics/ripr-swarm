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

use crate::agent::loop_commands::{display_path, shell_arg};
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
        serde_json::Value::String(text) => {
            *text = project_text_with_prefix(text, prefix);
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .for_each(|item| project_cwd_prefix(item, prefix)),
        serde_json::Value::Object(map) => map
            .values_mut()
            .for_each(|item| project_cwd_prefix(item, prefix)),
        _ => {}
    }
}

/// Project one rendered string: the bare machine prefix maps to `<cwd>/`,
/// and so does the `shell_arg`-quoted Bash redirect form. Redirect targets
/// render through `shell_arg`, so a checkout path carrying a space (or `+`,
/// `@`, …) reaches expectations quoted — `> '<prefix>tail'` — while the
/// pinned goldens carry the unquoted `<cwd>/tail` shape. Only the `> ` redirect
/// position unquotes: the PowerShell-safe form intentionally quotes its
/// `WriteAllText('…')` path argument on every checkout, and its goldens pin
/// that quoted shape. Artifact tails never contain a quote, so the first
/// closing quote after the anchored prefix ends the token; a token with no
/// closing quote keeps the bare-prefix projection only, matching previous
/// behavior.
#[cfg(test)]
pub(crate) fn project_cwd_text(text: &str) -> String {
    project_text_with_prefix(text, &renderer_cwd_prefix())
}

#[cfg(test)]
fn project_text_with_prefix(text: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // Mirror `shell_arg`'s single-quote escaping so a prefix that itself
    // carries a quote still opens the needle identically.
    let quoted_open = format!("> '{}", prefix.replace('\'', r"'\''"));
    while let Some(at) = rest.find(quoted_open.as_str()) {
        let after = &rest[at + quoted_open.len()..];
        let Some(end) = after.find('\'') else {
            break;
        };
        out.push_str(&rest[..at]);
        out.push_str("> <cwd>/");
        out.push_str(&after[..end]);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out.replace(prefix, "<cwd>/")
}

#[cfg(test)]
mod tests {
    use super::project_text_with_prefix;
    use crate::agent::loop_commands::shell_arg;

    #[test]
    fn quoted_anchored_target_projects_to_unquoted_placeholder() {
        let prefix = "C:/Users/John Doe/repo/";
        let rendered = format!("> {}", shell_arg(&format!("{prefix}target/ripr/out.json")));
        assert_eq!(
            rendered, "> 'C:/Users/John Doe/repo/target/ripr/out.json'",
            "the hostile checkout path must reach the test quoted"
        );
        assert_eq!(
            project_text_with_prefix(&rendered, prefix),
            "> <cwd>/target/ripr/out.json"
        );
    }

    #[test]
    fn bare_prefix_and_quote_free_checkout_keep_prior_shape() {
        let prefix = "C:/work/repo/";
        assert_eq!(
            project_text_with_prefix(&format!("> {prefix}target/out.json"), prefix),
            "> <cwd>/target/out.json"
        );
        assert_eq!(
            project_text_with_prefix("no anchor here", prefix),
            "no anchor here"
        );
        // A quoted token with no closing quote keeps the bare projection.
        assert_eq!(
            project_text_with_prefix(&format!("> '{prefix}target/out.json"), prefix),
            "> '<cwd>/target/out.json"
        );
    }

    #[test]
    fn powershell_write_all_text_keeps_its_quoted_shape() {
        // The PowerShell-safe form quotes its path argument on every
        // checkout; its goldens pin that shape, so the projection must not
        // strip those quotes — only the Bash `> ` redirect unquotes.
        let prefix = "C:/Users/John Doe/repo/";
        let rendered = format!("[System.IO.File]::WriteAllText('{prefix}target/out.json', $x)");
        assert_eq!(
            project_text_with_prefix(&rendered, prefix),
            "[System.IO.File]::WriteAllText('<cwd>/target/out.json', $x)"
        );
    }
}
