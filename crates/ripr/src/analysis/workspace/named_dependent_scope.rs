//! Named dependent-scope admission helpers (#6919).
//!
//! The full diff index disables crate-root macro routing when any indexed file
//! uses lexical fallback or composes modules through an unreadable edge. The
//! name-admitted path withholds files until admission, so these signals must be
//! recovered from withheld bytes and from the same lexical superset the full
//! index uses for `include!` / `#[path]` / `#[cfg_attr]` composition.

use std::path::Path;

use crate::analysis::syntax::parse_clean_source_file;

/// Whether `bytes` would land in the main index with `used_lexical_fallback`
/// if summarized now. Invalid UTF-8 and parser rejection match [`crate::analysis::facts::build`]'s
/// adapter summarize path without extracting file facts.
pub(crate) fn withheld_source_disables_crate_root_routing(_path: &Path, bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return true;
    };
    parse_clean_source_file(text).is_none()
}

/// Lexical superset: `include !`, `# [ path ..]`, comments in the attribute
/// head, non-ASCII / vertical-tab gap whitespace, and any `path` / `cfg_attr`
/// attribute (including `#[cfg_attr(.., path = "..")]` with no static target).
pub(crate) fn composes_modules(bytes: &[u8]) -> bool {
    let after_space = |at: usize| skip_gap_whitespace(bytes, at);
    let word_at = |at: usize, word: &[u8]| {
        bytes.get(at..at + word.len()) == Some(word)
            && bytes
                .get(at + word.len())
                .is_none_or(|next| !is_identifier_byte(*next))
    };
    (0..bytes.len()).any(|at| {
        let starts_word = at == 0 || !is_identifier_byte(bytes[at - 1]);
        if starts_word && word_at(at, b"include") {
            return bytes.get(after_space(at + b"include".len())) == Some(&b'!');
        }
        if bytes[at] != b'#' {
            return false;
        }
        let mut next = after_space(at + 1);
        if bytes.get(next) == Some(&b'!') {
            next = after_space(next + 1);
        }
        next = skip_attribute_head_comments(bytes, next);
        if bytes.get(next) != Some(&b'[') {
            return false;
        }
        next = skip_attribute_head_comments(bytes, after_space(next + 1));
        word_at(next, b"path") || word_at(next, b"cfg_attr")
    })
}

fn skip_attribute_head_comments(bytes: &[u8], mut at: usize) -> usize {
    loop {
        at = skip_gap_whitespace(bytes, at);
        if bytes.get(at..at + 2) == Some(b"//") {
            at += 2;
            while bytes.get(at).is_some_and(|byte| *byte != b'\n') {
                at += 1;
            }
            continue;
        }
        if bytes.get(at..at + 2) == Some(b"/*") {
            at += 2;
            let mut depth = 1usize;
            while depth > 0 {
                match bytes.get(at..at + 2) {
                    Some(b"/*") => {
                        depth += 1;
                        at += 2;
                    }
                    Some(b"*/") => {
                        depth -= 1;
                        at += 2;
                    }
                    Some(_) => at += 1,
                    None => return bytes.len(),
                }
            }
            continue;
        }
        break;
    }
    at
}

fn skip_gap_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == 0x0B || byte.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let limit = std::cmp::min(at + 4, bytes.len());
        let subslice = &bytes[at..limit];
        let valid_str = match std::str::from_utf8(subslice) {
            Ok(text) => text,
            Err(err) => {
                let valid_len = err.valid_up_to();
                if valid_len == 0 {
                    break;
                }
                match std::str::from_utf8(&subslice[..valid_len]) {
                    Ok(text) => text,
                    Err(_) => break,
                }
            }
        };
        let Some(ch) = valid_str.chars().next() else {
            break;
        };
        if ch.is_whitespace() && !ch.is_ascii() {
            at += ch.len_utf8();
            continue;
        }
        break;
    }
    at
}

fn is_identifier_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_modules_reads_spaced_include_and_bracket_attributes() {
        assert!(composes_modules(b"include !(\"x.rs\");"));
        assert!(composes_modules(b"#[path = \"a.rs\"] mod a;"));
        assert!(composes_modules(b"#[cfg_attr(unix, path = \"u.rs\")] mod check;"));
        assert!(composes_modules(b"# [ path = \"a.rs\" ] mod a;"));
        assert!(!composes_modules(b"#[derive(Debug)] struct S;"));
    }

    #[test]
    fn composes_modules_skips_comments_and_unicode_gap_whitespace() {
        assert!(composes_modules(
            b"#[/*c*/ cfg_attr(unix, path = \"u.rs\")] mod platform;"
        ));
        assert!(composes_modules(b"# /*c*/ [path = \"a.rs\"] mod a;"));
        assert!(composes_modules("#[\u{00a0}path = \"a.rs\"] mod a;".as_bytes()));
        assert!(composes_modules(b"#[\x0Bpath = \"a.rs\"] mod a;"));
    }

    #[test]
    fn withheld_unparsable_source_disables_crate_root_routing() {
        let path = Path::new("e/src/broken.rs");
        assert!(withheld_source_disables_crate_root_routing(
            path,
            b"fn oops( {"
        ));
        assert!(!withheld_source_disables_crate_root_routing(
            path,
            b"fn ok() {}"
        ));
        assert!(withheld_source_disables_crate_root_routing(
            path,
            b"\xff not utf8"
        ));
    }
}
