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

/// True when stable path text carries a `%XX` marker for a non-UTF-8 byte
/// (any uppercase `%XX` other than the literal-percent `%25`). A property
/// cannot carry it: GitHub's runner decodes only `%25 %0D %0A %3A %2C`, so
/// `file=a_%FF.rs` would name a different file than the one on disk (#13961).
pub(crate) fn stable_path_has_non_utf8_marker(stable: &str) -> bool {
    let bytes = stable.as_bytes();
    bytes.windows(3).any(|w| {
        w[0] == b'%'
            && (w[1].is_ascii_digit() || (b'A'..=b'F').contains(&w[1]))
            && (w[2].is_ascii_digit() || (b'A'..=b'F').contains(&w[2]))
            && &w[1..] != b"25"
    })
}

/// Message prefix for an annotation whose path holds non-UTF-8 bytes. The
/// path keeps its `%XX` notation, which keeps distinct names distinct.
pub(crate) fn unplaced_non_utf8_location_prefix(path: &str, line: &str) -> String {
    let location = if line.is_empty() || line == "0" {
        path.to_string()
    } else {
        format!("{path}:{line}")
    };
    format!(
        "Location (file name is not valid UTF-8, so not placed): {}. ",
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
        stable_path_has_non_utf8_marker, unplaced_location_prefix,
    };

    #[test]
    fn only_characters_a_property_cannot_carry_make_a_path_unplaceable() {
        assert!(path_is_unplaceable("a\u{1b}b"));
        assert!(path_is_unplaceable("a\u{202e}b"));
        assert!(path_is_unplaceable("a\u{7}b"));
        // CR and LF encode as %0D/%0A, which GitHub decodes faithfully.
        assert!(!path_is_unplaceable("a\r\nb"));
        assert!(!path_is_unplaceable("src/a,b:c%dé.rs"));
        assert!(stable_path_has_non_utf8_marker("a_%FF.rs"));
        assert!(stable_path_has_non_utf8_marker("a%25_%C3.rs"));
        assert!(!stable_path_has_non_utf8_marker("a%25FF.rs"));
        assert!(!stable_path_has_non_utf8_marker("src/a%25.rs"));
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

    // --- Property-based tests (#13950) ---
    //
    // `decode_*` model the consumer: GitHub's runner. The encoders in
    // actions/toolkit `packages/core/src/command.ts` define the wire format
    // (`escapeData`: `%`->`%25`, `\r`->`%0D`, `\n`->`%0A`; `escapeProperty`
    // additionally `:`->`%3A`, `,`->`%2C`), and actions/runner
    // `ActionCommand.cs` (`UnescapeData`/`UnescapeProperty`) reverses them
    // with sequential whole-string replaces, `%25` last. Nothing else is
    // decoded, so the models below decode exactly those tokens in that order.
    //
    // ripr escapes MORE than toolkit: after the wire escape, `terminal_safe`
    // spells every other `needs_terminal_escape` character (controls except
    // `\n`/`\t`, bidi and invisible formatting) as literal `\u{XX}` text.
    // `visible` models that, so the round trip is `decode(escape(s)) ==
    // visible(s)`, not `== s`.
    //
    // Case budget follows the repository convention: a small fixed default for
    // the required test run, raised by `PROPTEST_CASES` (fuzz.yml uses 400).
    // These are pure string functions, so the default is generous.

    use crate::agent::loop_commands::needs_terminal_escape;
    use proptest::prelude::*;
    use proptest::test_runner::Config;

    const DEFAULT_CASES: u32 = 256;

    fn config() -> Config {
        Config {
            cases: std::env::var("PROPTEST_CASES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_CASES),
            failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
            ..Config::default()
        }
    }

    /// actions/runner `UnescapeData`: `%0D`, `%0A`, then `%25`.
    fn decode_data(value: &str) -> String {
        value
            .replace("%0D", "\r")
            .replace("%0A", "\n")
            .replace("%25", "%")
    }

    /// actions/runner `UnescapeProperty`: `%0D`, `%0A`, `%3A`, `%2C`, then
    /// `%25`.
    fn decode_property(value: &str) -> String {
        value
            .replace("%0D", "\r")
            .replace("%0A", "\n")
            .replace("%3A", ":")
            .replace("%2C", ",")
            .replace("%25", "%")
    }

    /// What a reader sees after GitHub decodes ripr's output: CR and LF
    /// survive (they travel as `%0D`/`%0A`), every other character the
    /// terminal policy flags is spelled `\u{XX}`, the rest is unchanged.
    fn visible(value: &str) -> String {
        let mut out = String::new();
        for c in value.chars() {
            if c != '\r' && c != '\n' && needs_terminal_escape(c) {
                out.push_str(&format!("\\u{{{:02x}}}", c as u32));
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Hostile text: wire metacharacters, already-escaped-looking tokens,
    /// controls, bidi, and arbitrary chars, so every branch is hit often.
    fn hostile_text() -> impl Strategy<Value = String> {
        let fragment = prop_oneof![
            prop::sample::select(vec![
                "%",
                "%25",
                "%2525",
                "%0A",
                "%0D",
                "%0a",
                "%3A",
                "%2C",
                "%250A",
                "%%",
                "\r",
                "\n",
                "\r\n",
                ",",
                ":",
                "::",
                "=",
                "\t",
                "\u{0}",
                "\u{7}",
                "\u{1b}",
                "\u{7f}",
                "\u{85}",
                "\u{202e}",
                "\u{2066}",
                "\u{2028}",
                "\u{feff}",
                "\u{e0041}",
                "\\u{1b}",
                "é",
                "::warning file=x::",
                "/",
            ])
            .prop_map(str::to_string),
            any::<char>().prop_map(String::from),
            "[a-z]{1,4}",
        ];
        prop::collection::vec(fragment, 0..24).prop_map(|parts| parts.concat())
    }

    /// True when every `%` in `value` starts one of `tokens`.
    fn every_percent_starts_a_token(value: &str, tokens: &[&str]) -> bool {
        value
            .match_indices('%')
            .all(|(at, _)| tokens.iter().any(|token| value[at..].starts_with(token)))
    }

    fn has_raw_line_or_terminal_char(value: &str) -> bool {
        value
            .chars()
            .any(|c| c == '\r' || c == '\n' || needs_terminal_escape(c))
    }

    proptest! {
        #![proptest_config(config())]

        #[test]
        fn proptest_data_round_trips_through_github_decode(text in hostile_text()) {
            let escaped = escape_data(&text);
            prop_assert_eq!(decode_data(&escaped), visible(&text));
            prop_assert!(!has_raw_line_or_terminal_char(&escaped), "raw control in {escaped:?}");
            prop_assert!(every_percent_starts_a_token(&escaped, &["%25", "%0D", "%0A"]));
        }

        #[test]
        fn proptest_property_round_trips_through_github_decode(text in hostile_text()) {
            let escaped = escape_property(&text);
            prop_assert_eq!(decode_property(&escaped), visible(&text));
            prop_assert!(!has_raw_line_or_terminal_char(&escaped), "raw control in {escaped:?}");
            prop_assert!(!escaped.contains([',', ':']), "raw separator in {escaped:?}");
            prop_assert!(every_percent_starts_a_token(
                &escaped,
                &["%25", "%0D", "%0A", "%3A", "%2C"]
            ));
        }

        /// Pre-encoded input is stable path text (`stable_path_text`): every
        /// literal `%` is already `%25`. Its contract is to add the wire
        /// escape without touching any `%`, so stable text escapes to the same
        /// bytes as the raw text through `escape_property` (no `%2525` for a
        /// literal `%`), and GitHub decodes it back to the real name.
        #[test]
        fn proptest_pre_encoded_property_never_re_encodes_percent(text in hostile_text()) {
            // On Windows stable text turns `\` into `/`; keep the raw name
            // backslash-free so the comparison holds on every host.
            let raw = text.replace('\\', "");
            let stable = crate::analysis::stable_path_text(std::path::Path::new(&raw));
            let escaped = escape_property_pre_encoded(&stable);
            prop_assert_eq!(&escaped, &escape_property(&raw));
            prop_assert_eq!(decode_property(&escaped), visible(&raw));
            prop_assert!(!escaped.contains([',', ':']), "raw separator in {escaped:?}");
            // Every `%` of the input survives verbatim; only the wire escape
            // adds `%`s, one per CR, LF, comma, or colon.
            let added = stable.chars().filter(|c| matches!(c, '\r' | '\n' | ',' | ':')).count();
            prop_assert_eq!(
                escaped.matches('%').count(),
                stable.matches('%').count() + added
            );
        }

        /// A path is unplaceable exactly when a `file=` property cannot carry
        /// it faithfully: GitHub's decode of the escaped property differs from
        /// the path.
        #[test]
        fn proptest_unplaceable_iff_property_cannot_carry_the_path(text in hostile_text()) {
            let carried = decode_property(&escape_property(&text)) == text;
            prop_assert_eq!(path_is_unplaceable(&text), !carried);
        }
    }
}
