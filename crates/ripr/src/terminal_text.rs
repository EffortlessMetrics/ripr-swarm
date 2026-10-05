//! Terminal-safe rendering of repository text.
//!
//! The character policy (`needs_terminal_escape`) lives in
//! `agent/loop_commands.rs`, which `xtask` includes by path; this module is the
//! library-only escape built on it, importable from any layer (reports, stderr
//! notices in analysis, workflow encoders).

use crate::agent::loop_commands::needs_terminal_escape;

/// Make a finished human report safe to print to a terminal. Repository text
/// (assertion source, test names, observed values) reaches the report verbatim,
/// so a hostile repository could otherwise carry ESC/CSI/OSC sequences (clear
/// the screen, retitle the window), BEL, a bare CR that overwrites a line, or a
/// bidi override that reorders what the reader sees. Every control character
/// except `\n` and `\t`, and the bidi/invisible formatting characters, renders
/// as `\u{XX}`. Machine formats (JSON, SARIF) keep the raw value, escaped by
/// their own encoders.
pub(crate) fn terminal_safe(text: String) -> String {
    if !text.chars().any(needs_terminal_escape) {
        return text;
    }
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if needs_terminal_escape(ch) {
            out.push_str(&format!("\\u{{{:02x}}}", ch as u32));
        } else {
            out.push(ch);
        }
    }
    out
}
