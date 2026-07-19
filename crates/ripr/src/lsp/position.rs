//! Centralized analyzer-span → LSP Range/Position conversion.
//!
//! Every surface that constructs an LSP [`Range`] or [`Position`] from
//! analyzer line/column/expression data must route through this module so
//! that position-encoding decisions (UTF-16 vs UTF-8 vs UTF-32) are made in
//! exactly one place. See #1626 / #1748.
//!
//! When PR B (#1749) lands, the encoding is selected from the negotiated
//! `general.positionEncodings` client capability and plumbed through the
//! width functions. The default is UTF-16 (the mandatory LSP fallback).

use tower_lsp_server::ls_types::{Position, PositionEncodingKind, Range};

/// The negotiated position encoding for this session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PositionEncoding {
    /// UTF-16 code units (the LSP default). Each character contributes
    /// `char::len_utf16()` (1 for BMP, 2 for astral).
    Utf16,
    /// UTF-8 bytes. Each character contributes `char::len_utf8()`.
    Utf8,
    /// UTF-32 code points. Each character contributes 1.
    Utf32,
}

impl PositionEncoding {
    /// Negotiate the encoding from the client's advertised capabilities.
    /// Prefers UTF-16 (the mandatory baseline), then UTF-8, then UTF-32.
    /// If the client sends nothing, defaults to UTF-16.
    pub(crate) fn negotiate(client_encodings: Option<&[PositionEncodingKind]>) -> Self {
        let encodings = client_encodings.unwrap_or(&[]);
        // Check if the client explicitly supports UTF-16. If omitted,
        // UTF-16 is assumed (mandatory per LSP spec).
        let supports_utf16 = encodings.is_empty()
            || encodings
                .iter()
                .any(|e| e.as_str() == PositionEncodingKind::UTF16.as_str());
        if supports_utf16 {
            return PositionEncoding::Utf16;
        }
        // Client doesn't list UTF-16 — pick the first supported.
        if encodings
            .iter()
            .any(|e| e.as_str() == PositionEncodingKind::UTF8.as_str())
        {
            return PositionEncoding::Utf8;
        }
        if encodings
            .iter()
            .any(|e| e.as_str() == PositionEncodingKind::UTF32.as_str())
        {
            return PositionEncoding::Utf32;
        }
        // Fallback: UTF-16 (mandatory).
        PositionEncoding::Utf16
    }

    /// The server-side encoding kind to advertise in `ServerCapabilities`.
    #[allow(clippy::wrong_self_convention, reason = "to_capability is a type conversion, not a mutator")]
    pub(crate) fn to_capability(&self) -> PositionEncodingKind {
        match self {
            PositionEncoding::Utf16 => PositionEncodingKind::UTF16,
            PositionEncoding::Utf8 => PositionEncodingKind::UTF8,
            PositionEncoding::Utf32 => PositionEncodingKind::UTF32,
        }
    }

    /// Compute the character width of `text` under this encoding.
    pub(crate) fn character_width(&self, text: &str) -> u32 {
        match self {
            PositionEncoding::Utf16 => text
                .chars()
                .map(|c| c.len_utf16() as u32)
                .sum::<u32>()
                .max(1),
            PositionEncoding::Utf8 => text
                .chars()
                .map(|c| c.len_utf8() as u32)
                .sum::<u32>()
                .max(1),
            PositionEncoding::Utf32 => text.chars().count() as u32,
        }
    }
}

/// The maximum character width used for full-line diagnostic spans.
/// Diagnostics that cover a whole line (seam/gap diagnostics without a
/// specific expression) span from character 0 to this width.
pub(crate) const MAX_LINE_SPAN_WIDTH: u32 = 120;

/// Compute the UTF-16 code-unit width of a text string (the default encoding).
///
/// This is the LSP default character offset: each character contributes
/// `char::len_utf16()` code units (1 for BMP, 2 for astral planes).
/// Returns at least 1 so a non-empty expression always has a visible span.
pub(crate) fn utf16_character_width(text: &str) -> u32 {
    PositionEncoding::Utf16.character_width(text)
}

/// Build a [`Range`] covering an expression span on a single line.
///
/// `line` is 0-based (LSP convention). `column` is 1-based from the
/// analyzer and is converted to 0-based here. The span width is the UTF-16
/// width of `expression`, capped at [`MAX_LINE_SPAN_WIDTH`].
pub(crate) fn expression_span_range(line: u32, column: u32, expression: &str) -> Range {
    let start_character = column.saturating_sub(1);
    let width = utf16_character_width(expression).min(MAX_LINE_SPAN_WIDTH);
    Range {
        start: Position {
            line,
            character: start_character,
        },
        end: Position {
            line,
            character: start_character.saturating_add(width),
        },
    }
}

/// Build a [`Range`] covering a full line (character 0 to
/// [`MAX_LINE_SPAN_WIDTH`]). Used for seam/gap diagnostics that don't have
/// a specific expression to highlight.
pub(crate) fn line_span_range(line: u32) -> Range {
    Range {
        start: Position { line, character: 0 },
        end: Position {
            line,
            character: MAX_LINE_SPAN_WIDTH,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- UTF-16 width tests (backward compat with #1748) ---

    #[test]
    fn utf16_character_width_ascii() {
        assert_eq!(utf16_character_width("hello"), 5);
    }

    #[test]
    fn utf16_character_width_empty_returns_one() {
        assert_eq!(utf16_character_width(""), 1);
    }

    #[test]
    fn utf16_character_width_cjk_counts_two() {
        assert_eq!(utf16_character_width("日本語"), 3);
    }

    #[test]
    fn utf16_character_width_astral_plane_counts_four() {
        assert_eq!(utf16_character_width("🎉"), 2);
        assert_eq!(utf16_character_width("🎉🎉"), 4);
    }

    // --- Range construction tests ---

    #[test]
    fn expression_span_range_basic() {
        let range = expression_span_range(5, 10, "foo");
        assert_eq!(range.start.line, 5);
        assert_eq!(range.start.character, 9);
        assert_eq!(range.end.line, 5);
        assert_eq!(range.end.character, 12);
    }

    #[test]
    fn expression_span_range_caps_at_max_width() {
        let long = "x".repeat(200);
        let range = expression_span_range(0, 1, &long);
        assert_eq!(
            range.end.character - range.start.character,
            MAX_LINE_SPAN_WIDTH
        );
    }

    #[test]
    fn line_span_range_covers_zero_to_max() {
        let range = line_span_range(42);
        assert_eq!(range.start.line, 42);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 42);
        assert_eq!(range.end.character, MAX_LINE_SPAN_WIDTH);
    }

    // --- Position encoding negotiation tests (#1749) ---

    #[test]
    fn negotiate_defaults_to_utf16_when_client_sends_nothing() {
        assert_eq!(PositionEncoding::negotiate(None), PositionEncoding::Utf16);
    }

    #[test]
    fn negotiate_defaults_to_utf16_when_list_is_empty() {
        assert_eq!(
            PositionEncoding::negotiate(Some(&[])),
            PositionEncoding::Utf16
        );
    }

    #[test]
    fn negotiate_picks_utf16_when_client_lists_it() {
        let encodings = vec![PositionEncodingKind::UTF16];
        assert_eq!(
            PositionEncoding::negotiate(Some(&encodings)),
            PositionEncoding::Utf16
        );
    }

    #[test]
    fn negotiate_picks_utf8_when_utf16_not_listed() {
        let encodings = vec![PositionEncodingKind::UTF8];
        assert_eq!(
            PositionEncoding::negotiate(Some(&encodings)),
            PositionEncoding::Utf8
        );
    }

    #[test]
    fn negotiate_falls_back_to_utf16_for_unknown_encodings() {
        let encodings = vec![PositionEncodingKind::new("weird-encoding")];
        assert_eq!(
            PositionEncoding::negotiate(Some(&encodings)),
            PositionEncoding::Utf16
        );
    }

    // --- Encoding-specific width tests ---

    #[test]
    fn utf8_width_counts_bytes() {
        assert_eq!(PositionEncoding::Utf8.character_width("hello"), 5);
        assert_eq!(PositionEncoding::Utf8.character_width("日"), 3); // 3 UTF-8 bytes
        assert_eq!(PositionEncoding::Utf8.character_width("🎉"), 4); // 4 UTF-8 bytes
    }

    #[test]
    fn utf32_width_counts_codepoints() {
        assert_eq!(PositionEncoding::Utf32.character_width("hello"), 5);
        assert_eq!(PositionEncoding::Utf32.character_width("日"), 1);
        assert_eq!(PositionEncoding::Utf32.character_width("🎉"), 1);
    }

    #[test]
    fn to_capability_round_trips() {
        assert_eq!(
            PositionEncoding::Utf16.to_capability().as_str(),
            PositionEncodingKind::UTF16.as_str()
        );
        assert_eq!(
            PositionEncoding::Utf8.to_capability().as_str(),
            PositionEncodingKind::UTF8.as_str()
        );
        assert_eq!(
            PositionEncoding::Utf32.to_capability().as_str(),
            PositionEncodingKind::UTF32.as_str()
        );
    }
}
