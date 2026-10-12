//! GitHub workflow-command escaping authority (#4065).
//!
//! GitHub's command parser decodes two different escape sets: message
//! *data* (after `::`) decodes percent, CR, and LF only, while *property*
//! values (`file=`, `title=`) additionally decode comma and colon. Encoding
//! comma/colon in data leaves visible `%2C`/`%3A` scars; leaving them raw
//! in properties splits values into metadata. Every workflow-command
//! emitter (raw-check renderer, binary annotations adapter, and any future
//! consumer) must route through these three functions — never a local
//! `escape_cmd` fork — so the wire contract has exactly one owner.
//!
//! Percent handling differs by input kind, and the distinction is load
//! bearing: [`escape_property`] takes raw text and encodes `%` itself,
//! while [`escape_property_pre_encoded`] takes stable path text (which
//! already encodes every literal `%` as `%25` and every non-UTF-8 byte as
//! `%XX`, keeping distinct names distinct) and must NOT encode `%` again —
//! double-encoding yields `%2525`. Unicode passes through raw in all three:
//! the escape map has no UTF-8 branch.

use crate::agent::loop_commands::needs_terminal_escape;
use crate::output::human::terminal_safe;

/// True when a repository path holds a control or bidi character that no
/// workflow-command property can carry faithfully. GitHub decodes only
/// `%25 %0D %0A %3A %2C` in property values, so `%1B` (or any other escape)
/// stays literal text and the annotation would name a file that does not
/// exist. Such paths omit `file=`/`line=` and name the escaped location in
/// the message instead (#6309). `\r` and `\n` stay placeable: they encode as
/// `%0D`/`%0A`.
pub(crate) fn path_is_unplaceable(path: &str) -> bool {
    path.chars().any(|c| c != '\r' && needs_terminal_escape(c))
}

/// Message prefix naming the location of an annotation that could not be
/// placed on its file. `line` is the display text of the line number; an empty
/// or `0` line names the file alone. The path is shown as given: stable path
/// text from the check renderer keeps its `%XX` notation, which is what keeps
/// distinct names distinct; raw `comments.json` paths show as written.
pub(crate) fn unplaced_location_prefix(path: &str, line: &str) -> String {
    let location = if line.is_empty() || line == "0" {
        path.to_string()
    } else {
        format!("{path}:{line}")
    };
    format!(
        "Location (file name has control characters, so not placed): {}. ",
        terminal_safe(location)
    )
}

/// Escape workflow-command *data* (the message after `::`).
///
/// Control and bidi characters other than the ones GitHub decodes print as
/// `\u{XX}` (see `human::terminal_safe`), so a hostile path or message cannot
/// drive the terminal or a log viewer.
///
/// Single pass: each input char is visited once, so the `%` introduced by
/// an insertion is never rescanned — literal `%0A` sequences survive one
/// decode as `%250A` instead of becoming a newline. Never blanket-decode
/// the input to "repair" this.
pub(crate) fn escape_data(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '%' => escaped.push_str("%25"),
            '\r' => escaped.push_str("%0D"),
            '\n' => escaped.push_str("%0A"),
            _ => escaped.push(c),
        }
    }
    terminal_safe(escaped)
}

/// Escape a workflow-command *property* value from raw text.
///
/// Use for values that did not pass through stable path encoding
/// (annotation titles, comments.json placement paths).
pub(crate) fn escape_property(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '%' => escaped.push_str("%25"),
            '\r' => escaped.push_str("%0D"),
            '\n' => escaped.push_str("%0A"),
            ',' => escaped.push_str("%2C"),
            ':' => escaped.push_str("%3A"),
            _ => escaped.push(c),
        }
    }
    terminal_safe(escaped)
}

/// Escape a workflow-command *property* value from stable path text.
///
/// Use only for values produced by stable path normalization, where `%`
/// is already encoded. Encoding `%` again double-encodes (`%2525`).
pub(crate) fn escape_property_pre_encoded(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\r' => escaped.push_str("%0D"),
            '\n' => escaped.push_str("%0A"),
            ',' => escaped.push_str("%2C"),
            ':' => escaped.push_str("%3A"),
            _ => escaped.push(c),
        }
    }
    terminal_safe(escaped)
}

#[cfg(test)]
mod tests {
    use super::{
        escape_data, escape_property, escape_property_pre_encoded, path_is_unplaceable,
        unplaced_location_prefix,
    };

    #[test]
    fn only_characters_a_property_cannot_carry_make_a_path_unplaceable() {
        assert!(path_is_unplaceable("a\u{1b}b"));
        assert!(path_is_unplaceable("a\u{202e}b"));
        assert!(path_is_unplaceable("a\u{7}b"));
        // CR and LF encode as %0D/%0A, which GitHub decodes faithfully.
        assert!(!path_is_unplaceable("a\r\nb"));
        assert!(!path_is_unplaceable("src/a,b:c%dé.rs"));
    }

    #[test]
    fn unplaced_prefix_names_the_escaped_location_with_or_without_a_line() {
        assert_eq!(
            unplaced_location_prefix("a\u{1b}b.rs", "7"),
            "Location (file name has control characters, so not placed): a\\u{1b}b.rs:7. "
        );
        for no_line in ["", "0"] {
            assert_eq!(
                unplaced_location_prefix("a\u{1b}b.rs", no_line),
                "Location (file name has control characters, so not placed): a\\u{1b}b.rs. "
            );
        }
    }

    #[test]
    fn control_and_bidi_characters_print_as_escapes_in_every_form() {
        let hostile = "a\u{1b}[2J\u{7}b\u{202e}c\nd";
        assert_eq!(escape_data(hostile), "a\\u{1b}[2J\\u{07}b\\u{202e}c%0Ad");
        assert_eq!(
            escape_property(hostile),
            "a\\u{1b}[2J\\u{07}b\\u{202e}c%0Ad"
        );
        assert_eq!(
            escape_property_pre_encoded(hostile),
            "a\\u{1b}[2J\\u{07}b\\u{202e}c%0Ad"
        );
    }

    #[test]
    fn data_keeps_comma_colon_literal() {
        assert_eq!(
            escape_data("Result::Err from assert_eq!(actual, expected) at 100%"),
            "Result::Err from assert_eq!(actual, expected) at 100%25"
        );
    }

    #[test]
    fn data_escapes_cr_lf_and_percent_first() {
        assert_eq!(escape_data("a\rb\nc%d"), "a%0Db%0Ac%25d");
        assert_eq!(escape_data("literal %0A token"), "literal %250A token");
    }

    #[test]
    fn property_escapes_all_five_from_raw_text() {
        assert_eq!(
            escape_property("src/a,b:c%d\r\né.rs"),
            "src/a%2Cb%3Ac%25d%0D%0Aé.rs"
        );
    }

    #[test]
    fn pre_encoded_property_skips_percent_only() {
        assert_eq!(
            escape_property_pre_encoded("src/a%25b,c:d\r\nx.rs"),
            "src/a%25b%2Cc%3Ad%0D%0Ax.rs"
        );
    }

    #[test]
    fn unicode_passes_through_everywhere() {
        assert_eq!(escape_data("naïve ünïcode"), "naïve ünïcode");
        assert_eq!(escape_property("src/dé.rs"), "src/dé.rs");
        assert_eq!(escape_property_pre_encoded("src/dé.rs"), "src/dé.rs");
    }
}
