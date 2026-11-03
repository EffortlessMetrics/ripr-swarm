//! Kani bounded-model-checking harnesses for the character policy behind
//! `terminal_text` (`needs_terminal_escape`). Compiled only under `cargo kani`.
//!
//! `terminal_safe`, `single_line` and `json_terminal_safe` are per-character
//! maps over `needs_terminal_escape` that build `String`s with `format!`.
//! CBMC does not terminate on those allocation/formatting paths even for a
//! one-character input (`str::count` and `fmt` unwinding, >9 minutes), so the
//! proofs below target the character-level policy and the escape alphabet,
//! which together carry the "no raw control character survives" and
//! idempotence claims. Whole-`String` behavior stays covered by unit tests.

use crate::agent::loop_commands::needs_terminal_escape;

/// Property: for EVERY `char` (no bound), the policy escapes each control
/// character except `\n` and `\t`, and the bidi overrides/isolates and
/// invisible tag characters. Control characters can never pass through raw.
#[kani::proof]
fn needs_terminal_escape_covers_controls_and_bidi() {
    let ch: char = kani::any();
    if ch.is_control() && ch != '\n' && ch != '\t' {
        assert!(needs_terminal_escape(ch));
    }
    if matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}') {
        assert!(needs_terminal_escape(ch));
    }
    if matches!(ch, '\u{e0000}'..='\u{e007f}' | '\u{200b}' | '\u{feff}') {
        assert!(needs_terminal_escape(ch));
    }
    // Printable ASCII and the newline/tab layout characters stay literal.
    if matches!(ch, ' '..='~' | '\n' | '\t') {
        assert!(!needs_terminal_escape(ch));
    }
    kani::cover!(ch == '\u{1b}' && needs_terminal_escape(ch));
    kani::cover!(ch == 'a' && !needs_terminal_escape(ch));
}

/// Property (idempotence core): every character of the escape spelling
/// `\u{<hex>}` (backslash, `u`, braces, hex digits) is itself left literal by
/// the policy, so re-escaping already-escaped text is a no-op.
#[kani::proof]
fn escape_spelling_alphabet_is_never_escaped() {
    let d: u32 = kani::any();
    kani::assume(d < 16);
    let digit = char::from_digit(d, 16);
    assert!(digit.is_some());
    if let Some(c) = digit {
        assert!(!needs_terminal_escape(c));
        assert!(!c.is_control());
    }
    for c in ['\\', 'u', '{', '}'] {
        assert!(!needs_terminal_escape(c));
        assert!(!c.is_control());
    }
    kani::cover!(d == 15);
}

/// Property: every char the policy flags encodes to one or two UTF-16 units,
/// so the two-slot buffer in `json_terminal_safe` (surrogate pairs for astral
/// chars) cannot panic, and a pair is produced exactly for astral chars (tag
/// block, variation-selector supplement).
#[kani::proof]
fn json_escape_utf16_buffer_is_sufficient() {
    let ch: char = kani::any();
    if needs_terminal_escape(ch) {
        let mut units = [0u16; 2];
        let n = ch.encode_utf16(&mut units).len();
        assert!(n == 1 || n == 2);
        assert!((n == 2) == (ch as u32 > 0xFFFF));
        kani::cover!(n == 2);
        kani::cover!(n == 1);
    }
}
