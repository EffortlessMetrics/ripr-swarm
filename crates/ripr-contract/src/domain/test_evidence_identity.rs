//! Canonical missing-discriminator identity for portable test evidence.
//!
//! Discriminator values are Rust-shaped expressions. Formatting whitespace
//! may move without changing the behavior, while whitespace inside literals is
//! behavior. Keep that distinction local to the producer instead of asking
//! renderers or cross-tool consumers to reconstruct it.

/// Normalize a transport path for portable summaries: forward slashes and no
/// leading `./` prefix. Relative `../` segments are preserved.
pub fn normalize_transport_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized.trim_start_matches("./").to_string()
}

/// Normalize, order, and deduplicate producer-owned discriminator identities.
pub(super) fn canonicalize_missing_discriminators(values: &[String]) -> Vec<String> {
    let mut canonical = values
        .iter()
        .map(|value| normalize_discriminator_text(value))
        .collect::<Vec<_>>();
    canonical.retain(|value| !value.is_empty());
    canonical.sort();
    canonical.dedup();
    canonical
}

/// Encode arbitrary discriminator text without delimiter collisions.
///
/// Each sorted item is byte-length-prefixed. Values containing `;`, `:`, or
/// the fingerprint's own marker remain uniquely decodable.
pub(super) fn encode_missing_discriminators(values: &[String]) -> String {
    let mut encoded = String::new();
    for value in values {
        encoded.push_str(&value.len().to_string());
        encoded.push(':');
        encoded.push_str(value);
    }
    encoded
}

/// Normalize formatting whitespace without rewriting Rust literal contents.
///
/// Whitespace between tokens is formatting; whitespace inside normal, byte,
/// C, raw, or character literals is behavior. Lifetimes are not treated as
/// character literals unless the following token has a character-literal
/// shape.
fn normalize_discriminator_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    let mut pending_space = false;

    while let Some(character) = chars.next() {
        if character.is_whitespace() {
            pending_space = true;
            continue;
        }

        if pending_space && !normalized.is_empty() {
            normalized.push(' ');
        }
        pending_space = false;

        match character {
            '"' => {
                normalized.push(character);
                copy_quoted_literal(&mut chars, &mut normalized, character);
            }
            '\'' if starts_character_literal(chars.clone()) => {
                normalized.push(character);
                copy_quoted_literal(&mut chars, &mut normalized, character);
            }
            'r' => {
                normalized.push(character);
                if let Some(hash_count) = raw_string_hash_count(chars.clone()) {
                    copy_raw_string(&mut chars, &mut normalized, hash_count);
                }
            }
            _ => normalized.push(character),
        }
    }

    normalized
}

fn copy_quoted_literal(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    output: &mut String,
    delimiter: char,
) {
    let mut escaped = false;
    for character in chars.by_ref() {
        output.push(character);
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == delimiter {
            break;
        }
    }
}

fn starts_character_literal(mut chars: std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let Some(first) = chars.next() else {
        return false;
    };

    match first {
        '\\' => {
            let Some(escape) = chars.next() else {
                return false;
            };
            match escape {
                'x' => {
                    let Some(first_hex) = chars.next() else {
                        return false;
                    };
                    let Some(second_hex) = chars.next() else {
                        return false;
                    };
                    first_hex.is_ascii_hexdigit()
                        && second_hex.is_ascii_hexdigit()
                        && chars.next() == Some('\'')
                }
                'u' => {
                    if chars.next() != Some('{') {
                        return false;
                    }
                    let mut digits = 0usize;
                    loop {
                        let Some(character) = chars.next() else {
                            return false;
                        };
                        if character == '}' {
                            break;
                        }
                        if !character.is_ascii_hexdigit() && character != '_' {
                            return false;
                        }
                        if character != '_' {
                            digits += 1;
                        }
                    }
                    digits > 0 && chars.next() == Some('\'')
                }
                '\n' | '\r' => false,
                _ => chars.next() == Some('\''),
            }
        }
        '\n' | '\r' | '\'' => false,
        _ => chars.next() == Some('\''),
    }
}

fn raw_string_hash_count(mut chars: std::iter::Peekable<std::str::Chars<'_>>) -> Option<usize> {
    let mut hash_count = 0usize;
    while chars.peek() == Some(&'#') {
        let _ = chars.next();
        hash_count += 1;
    }
    (chars.next() == Some('"')).then_some(hash_count)
}

fn copy_raw_string(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    output: &mut String,
    hash_count: usize,
) {
    for _ in 0..hash_count {
        if let Some(hash) = chars.next() {
            output.push(hash);
        }
    }
    if let Some(opening_quote) = chars.next() {
        output.push(opening_quote);
    }

    while let Some(character) = chars.next() {
        output.push(character);
        if character != '"' || !raw_string_closes(chars.clone(), hash_count) {
            continue;
        }
        for _ in 0..hash_count {
            if let Some(hash) = chars.next() {
                output.push(hash);
            }
        }
        break;
    }
}

fn raw_string_closes(
    mut chars: std::iter::Peekable<std::str::Chars<'_>>,
    hash_count: usize,
) -> bool {
    (0..hash_count).all(|_| chars.next() == Some('#'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_whitespace_is_canonicalized_and_deduplicated() {
        let canonical = canonicalize_missing_discriminators(&[
            " amount   == threshold ".to_string(),
            "amount == threshold".to_string(),
            "   ".to_string(),
        ]);

        assert_eq!(canonical, vec!["amount == threshold".to_string()]);
    }

    #[test]
    fn normal_string_literal_whitespace_remains_significant() {
        let canonical = canonicalize_missing_discriminators(&[
            r#"actual == "a b""#.to_string(),
            r#"actual == "a  b""#.to_string(),
        ]);

        assert_eq!(canonical.len(), 2);
        assert_ne!(canonical[0], canonical[1]);
    }

    #[test]
    fn raw_and_character_literal_whitespace_remains_significant() {
        assert_ne!(
            normalize_discriminator_text(r##"actual == r#"a b"#"##),
            normalize_discriminator_text(r##"actual == r#"a  b"#"##)
        );
        assert_eq!(
            normalize_discriminator_text("actual   ==   ' '"),
            "actual == ' '"
        );
    }

    #[test]
    fn lifetime_quote_does_not_capture_following_formatting_whitespace() {
        assert_eq!(
            normalize_discriminator_text("value::< 'a   >()"),
            "value::< 'a >()"
        );
    }

    #[test]
    fn length_prefix_encoding_distinguishes_delimiter_partitions() {
        let left = encode_missing_discriminators(&["a;b".to_string(), "c".to_string()]);
        let right = encode_missing_discriminators(&["a".to_string(), "b;c".to_string()]);

        assert_ne!(left, right);
    }

    /// Each row is `(input, normalized)`.
    fn assert_normalized(cases: &[(&str, &str)]) {
        for (input, expected) in cases {
            assert_eq!(
                normalize_discriminator_text(input),
                *expected,
                "input {input:?}"
            );
        }
    }

    #[test]
    fn formatting_whitespace_collapses_to_one_space_and_is_trimmed() {
        assert_normalized(&[
            ("", ""),
            (" \t\n ", ""),
            ("amount", "amount"),
            ("  amount\t==\n\n threshold  ", "amount == threshold"),
            ("f( a ,\n    b )", "f( a , b )"),
            // An `r` that opens no raw string is an ordinary token.
            ("err   ==  r", "err == r"),
            ("r   ==   1", "r == 1"),
            ("r#type   ==   x", "r#type == x"),
        ]);
    }

    #[test]
    fn quoted_literals_are_copied_verbatim_up_to_their_closing_quote() {
        assert_normalized(&[
            (r#"x   ==   "a  b""#, r#"x == "a  b""#),
            // An escaped quote does not close the literal.
            (r#""a \"  b"   ==   x"#, r#""a \"  b" == x"#),
            // An escaped backslash does not escape the quote after it.
            (r#""a\\"   +   "b  c""#, r#""a\\" + "b  c""#),
            (r#"b"a  b"   ==   c"x  y""#, r#"b"a  b" == c"x  y""#),
            // An unterminated literal keeps the rest as written.
            (r#"x   ==   "a  b "#, r#"x == "a  b "#),
        ]);
    }

    #[test]
    fn raw_strings_close_only_on_a_quote_followed_by_all_their_hashes() {
        assert_normalized(&[
            (r#"r"a  b"   ==   x"#, r#"r"a  b" == x"#),
            // A backslash is not an escape in a raw string.
            (r#"r"a\"   +   "b  c""#, r#"r"a\" + "b  c""#),
            (r###"r#"a"  b"#   ==   c"###, r###"r#"a"  b"# == c"###),
            (
                r###"r##"a "#  b"##   ==   y"###,
                r###"r##"a "#  b"## == y"###,
            ),
            (r###"br#"a  b"#   ==   x"###, r###"br#"a  b"# == x"###),
            // Unterminated: the rest stays as written.
            (r###"x   ==   r#"a  b"  "###, r###"x == r#"a  b"  "###),
        ]);
    }

    #[test]
    fn a_character_literal_keeps_the_character_it_holds() {
        assert_normalized(&[
            ("c   ==   ' '", "c == ' '"),
            ("c   ==   '\t'   ||   d", "c == '\t' || d"),
            // A double quote inside a character literal opens no string.
            (r#"a == '"'   &&   b == '"'"#, r#"a == '"' && b == '"'"#),
            (
                r#"a == '\"'   &&   b == "x  y""#,
                r#"a == '\"' && b == "x  y""#,
            ),
            (r"a == '\''   &&   b", r"a == '\'' && b"),
        ]);
    }

    #[test]
    fn a_quote_that_opens_no_character_literal_does_not_protect_whitespace() {
        assert_normalized(&[
            // Lifetimes and labels.
            ("&'a   str   ==   &'b   str", "&'a str == &'b str"),
            (
                "'outer:   loop   {   break   'outer   }",
                "'outer: loop { break 'outer }",
            ),
            // More than one character before the next quote.
            ("'ab   c   '", "'ab c '"),
            // A line break cannot be the character.
            ("'\n'   a", "' ' a"),
            ("'\r'   a", "' ' a"),
            // Escapes that are not closed by a quote.
            (r"'\n   a   '", r"'\n a '"),
            (r"'\x41   a   '", r"'\x41 a '"),
            (r"'\u{41}   a   '", r"'\u{41} a '"),
            // A backslash before a line break is not an escape.
            ("'\\\n'   a", "'\\ ' a"),
            ("'\\\r'   a", "'\\ ' a"),
            // Input that ends inside the would-be literal.
            ("x   '", "x '"),
            (r"x   '\", r"x '\"),
            (r"x   '\x4", r"x '\x4"),
            (r"x   '\u{4", r"x '\u{4"),
        ]);
    }

    #[test]
    fn hex_and_unicode_escapes_close_at_their_own_quote() {
        // The string that follows each escape keeps its double space only when
        // the escape was recognised as one whole character literal; otherwise
        // the escape's closing quote would open a literal of its own.
        assert_normalized(&[
            (r#"'\x41'"'  ""#, r#"'\x41'"'  ""#),
            (r#"'\u{41}'"'  ""#, r#"'\u{41}'"'  ""#),
            (r#"'\u{4_1}'"'  ""#, r#"'\u{4_1}'"'  ""#),
        ]);
    }

    #[test]
    fn a_unicode_escape_without_digits_is_not_a_character_literal() {
        assert_normalized(&[
            (r#"'\u{}'"'  ""#, r#"'\u{}'"' ""#),
            (r#"'\u{_}'"'  ""#, r#"'\u{_}'"' ""#),
        ]);
    }

    #[test]
    fn the_encoding_prefixes_each_value_with_its_byte_length() {
        assert_eq!(encode_missing_discriminators(&[]), "");
        assert_eq!(
            encode_missing_discriminators(&["ab".to_string(), "c".to_string()]),
            "2:ab1:c"
        );
        assert_eq!(
            encode_missing_discriminators(&["\u{e9}".to_string()]),
            "2:\u{e9}"
        );
    }

    #[test]
    fn canonical_discriminators_are_sorted_and_literal_differences_survive() {
        let canonical = canonicalize_missing_discriminators(&[
            "z  ==  1".to_string(),
            r#"a == "x  y""#.to_string(),
            "z == 1".to_string(),
            r#"a   ==   "x y""#.to_string(),
        ]);
        assert_eq!(
            canonical,
            vec![
                r#"a == "x  y""#.to_string(),
                r#"a == "x y""#.to_string(),
                "z == 1".to_string(),
            ]
        );
    }

    #[test]
    fn transport_paths_use_forward_slashes_and_drop_leading_current_dir() {
        assert_eq!(normalize_transport_path(r".\src\lib.rs"), "src/lib.rs");
        assert_eq!(normalize_transport_path("././tests/a.rs"), "tests/a.rs");
        assert_eq!(normalize_transport_path("../shared/a.rs"), "../shared/a.rs");
        assert_eq!(normalize_transport_path("src/./lib.rs"), "src/./lib.rs");
    }
}

#[cfg(kani)]
mod kani_proofs;
