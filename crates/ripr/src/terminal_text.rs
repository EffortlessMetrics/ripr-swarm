//! Which characters must not reach a terminal raw.
//!
//! One owner for the policy, dependency-free so the `xtask` copy of
//! `agent/loop_commands.rs` can include it by path. Report escaping
//! (`output::human::terminal_safe`) and shell quoting
//! (`agent::loop_commands::shell_arg`) both decide from this predicate.

/// Every control character except `\n` and `\t`, plus the bidi formatting
/// characters.
pub(crate) fn needs_terminal_escape(ch: char) -> bool {
    match ch {
        '\n' | '\t' => false,
        c if c.is_control() => true,
        // Arabic letter mark, LRM/RLM, embeddings/overrides (LRE..RLO), and
        // isolates (LRI..PDI): they reorder text without any visible glyph.
        '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {
            true
        }
        _ => false,
    }
}
