use std::path::{Component, Path, PathBuf};

pub(super) fn parse_new_path_marker(raw: &str) -> Option<PathBuf> {
    let marker = raw.strip_prefix("+++ ")?;
    let path = parse_diff_path_token(marker)?;
    if path == "/dev/null" {
        return None;
    }
    let path = path.strip_prefix("b/").unwrap_or(&path);
    confine_to_relative_path(path)
}

pub(super) fn is_dev_null_new_path_marker(raw: &str) -> bool {
    raw.strip_prefix("+++ ")
        .and_then(parse_diff_path_token)
        .is_some_and(|path| path == "/dev/null")
}

pub(super) fn parse_git_old_path(raw: &str) -> Option<PathBuf> {
    let marker = raw.strip_prefix("diff --git ")?;
    let path = if marker.starts_with('"') {
        parse_diff_path_token(marker)?
    } else {
        let old_path = marker.strip_prefix("a/")?.split_once(" b/")?.0;
        parse_diff_path_token(&format!("a/{old_path}"))?
    };
    let path = path.strip_prefix("a/").unwrap_or(&path);
    confine_to_relative_path(path)
}

pub(super) fn parse_rename_from_path(raw: &str) -> Option<PathBuf> {
    parse_rename_path_marker(raw, "rename from ")
}

pub(super) fn parse_rename_to_path(raw: &str) -> Option<PathBuf> {
    parse_rename_path_marker(raw, "rename to ")
}

fn parse_rename_path_marker(raw: &str, prefix: &str) -> Option<PathBuf> {
    let path = raw.strip_prefix(prefix).and_then(parse_diff_path_token)?;
    confine_to_relative_path(&path)
}

/// Whether `raw` is syntactically a `+++ <path>` new-path marker, regardless
/// of whether the path survives confinement. Boundary detection must treat a
/// confinement-rejected (or `/dev/null`) marker as a file-section boundary:
/// otherwise, in a plain diff with no `diff --git` separators, the rejected
/// marker line and its hunk are consumed as payload of the previous file,
/// mis-attributing attacker-controlled lines to an in-workspace path
/// (#2099 review).
pub(super) fn is_new_path_marker(raw: &str) -> bool {
    let Some(marker) = raw.strip_prefix("+++ ") else {
        return false;
    };
    let trimmed = marker.trim_end_matches('\r');
    let quoted = trimmed.starts_with('"');
    match parse_diff_path_token(marker) {
        // An unquoted path containing whitespace is implausible as a diff
        // path (git C-quotes such paths): without this gate, hunk payload
        // lines like `--- token` / `+++ token with spaces` could be misread
        // as a file-section boundary. Mirrors the plausibility contract of
        // parse_old_path_marker. Quoted paths may legitimately contain
        // spaces.
        Some(path) => quoted || is_plausible_unquoted_diff_path(&path),
        None => false,
    }
}

/// Lexically confine a parsed diff path to the workspace: keep only `Normal`
/// components and reject the whole path when it contains a parent-directory,
/// root, or prefix component. A crafted diff such as
/// `+++ b/../../../etc/passwd` would otherwise reach `root.join(path)`
/// unconfined and produce a `SourceLocation` that escapes the workspace
/// (#2099). Rejection returns `None`, which the parser treats like
/// `/dev/null`: the file is never registered, so no probe, output record, or
/// snapshot lookup can reference the escaping path.
fn confine_to_relative_path(path: &str) -> Option<PathBuf> {
    let mut confined = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => confined.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if confined.as_os_str().is_empty() {
        None
    } else {
        Some(confined)
    }
}

/// Detect whether `raw` is a syntactically valid `--- <path>` old-path marker.
///
/// Returns `true` for any plausible old-path marker, including paths that
/// would be rejected by `confine_to_relative_path`. This function is a
/// **boundary detector** for the parser — it must recognize `---` lines as
/// file-section boundaries even when the path escapes the workspace, so the
/// parser can clear the current file and prevent payload mis-attribution
/// (#2099).
///
/// The old path is **not** confined here because it is currently discarded
/// (the parser does not build a `SourceLocation` from it). Any future feature
/// that consumes the old path (rename-aware probe, copy detection) MUST call
/// [`confine_to_relative_path`] on the extracted path before joining it to
/// the workspace root. Use [`parse_old_path_for_confinement`] for that
/// purpose (#2402).
pub(super) fn parse_old_path_marker(raw: &str) -> bool {
    let Some(marker) = raw.strip_prefix("--- ") else {
        return false;
    };
    let Some(path) = parse_diff_path_token(marker) else {
        return false;
    };
    if path == "/dev/null" {
        return true;
    }
    let path = path.strip_prefix("a/").unwrap_or(&path);
    is_plausible_unquoted_diff_path(path)
}

/// Extract and confine the old path from a `--- <path>` marker, symmetrically
/// with [`parse_new_path_marker`]. Returns `None` when the path escapes the
/// workspace (traversal, absolute, prefix). Future features that consume the
/// old path should use this instead of raw string extraction (#2402).
#[allow(
    dead_code,
    reason = "reserved for future rename/copy-detection features that consume the old path (#2402)"
)]
pub(super) fn parse_old_path_for_confinement(raw: &str) -> Option<PathBuf> {
    let marker = raw.strip_prefix("--- ")?;
    let path = parse_diff_path_token(marker)?;
    if path == "/dev/null" {
        return None;
    }
    let path = path.strip_prefix("a/").unwrap_or(&path);
    confine_to_relative_path(path)
}

fn is_plausible_unquoted_diff_path(path: &str) -> bool {
    !path.is_empty() && !path.chars().any(char::is_whitespace)
}

fn parse_diff_path_token(raw: &str) -> Option<String> {
    let raw = raw.trim_end_matches('\r');
    if let Some(quoted) = raw.strip_prefix('"') {
        return parse_c_quoted_path(quoted);
    }

    let token = raw.split_once('\t').map_or(raw, |(path, _metadata)| path);
    Some(token.trim_end().to_string()).filter(|path| !path.is_empty())
}

fn parse_c_quoted_path(raw: &str) -> Option<String> {
    // Decode at the byte level: git's octal escapes carry raw bytes, so
    // mapping each escaped byte to one Unicode scalar would turn a valid
    // UTF-8 name like `caf\303\251.rs` into `cafÃ©.rs` and lose workspace
    // identity (#3601 review). Valid UTF-8 sequences reconstruct their
    // true name; invalid bytes stay visible as octal residue, keeping
    // distinct invalid names distinct instead of collapsing onto one
    // replacement character.
    let mut bytes = Vec::new();
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(decode_path_bytes(bytes)),
            '\\' => parse_c_escape(&mut chars, &mut bytes),
            _ => {
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
        }
    }

    None
}

/// Reconstruct a path string from decoded bytes: valid UTF-8 passes
/// through untouched; invalid bytes render as literal octal escapes
/// (`\377`) — distinct per byte and never the U+FFFD replacement
/// character, so distinct invalid names cannot merge downstream.
fn decode_path_bytes(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            let bytes = err.into_bytes();
            let mut out = String::with_capacity(bytes.len());
            let mut rest: &[u8] = &bytes;
            while !rest.is_empty() {
                match std::str::from_utf8(rest) {
                    Ok(valid) => {
                        out.push_str(valid);
                        break;
                    }
                    Err(error) => {
                        let valid_up_to = error.valid_up_to();
                        if let Ok(valid) = std::str::from_utf8(&rest[..valid_up_to]) {
                            out.push_str(valid);
                        }
                        let invalid_len = error.error_len().unwrap_or(rest.len() - valid_up_to);
                        for byte in &rest[valid_up_to..valid_up_to + invalid_len] {
                            push_octal_residue(&mut out, *byte);
                        }
                        rest = &rest[valid_up_to + invalid_len..];
                    }
                }
            }
            out
        }
    }
}

fn push_octal_residue(out: &mut String, byte: u8) {
    out.push('\\');
    out.push((b'0' + byte / 64) as char);
    out.push((b'0' + (byte / 8) % 8) as char);
    out.push((b'0' + byte % 8) as char);
}

fn parse_c_escape<I>(chars: &mut std::iter::Peekable<I>, bytes: &mut Vec<u8>)
where
    I: Iterator<Item = char>,
{
    let Some(ch) = chars.next() else {
        bytes.push(b'\\');
        return;
    };

    match ch {
        'n' => bytes.push(b'\n'),
        'r' => bytes.push(b'\r'),
        't' => bytes.push(b'\t'),
        '\\' => bytes.push(b'\\'),
        '"' => bytes.push(b'"'),
        '0'..='7' => bytes.push(parse_octal_escape(ch, chars)),
        other => {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
        }
    }
}

fn parse_octal_escape<I>(first: char, chars: &mut std::iter::Peekable<I>) -> u8
where
    I: Iterator<Item = char>,
{
    let mut value = first.to_digit(8).unwrap_or(0);

    for _ in 0..2 {
        let Some(next) = chars.peek().copied() else {
            break;
        };
        let Some(digit) = next.to_digit(8) else {
            break;
        };
        let _ = chars.next();
        value = value.saturating_mul(8).saturating_add(digit);
    }

    // git emits three octal digits per raw byte, so the value fits u8;
    // clamp out-of-range forms instead of widening to a Unicode scalar.
    u8::try_from(value).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_quoted_paths_reconstruct_utf8_names_and_keep_invalid_bytes_distinct() {
        // Valid UTF-8 sequences decode at the byte level, restoring the
        // on-disk name (#3601 review): \303\251 is é, not Ã©.
        assert_eq!(
            parse_diff_path_token("\"caf\\303\\251.rs\""),
            Some("café.rs".to_string())
        );
        // Distinct invalid bytes stay distinct as octal residue, never a
        // replacement character.
        assert_eq!(
            parse_diff_path_token("\"pricing_\\377.rs\""),
            Some("pricing_\\377.rs".to_string())
        );
        assert_eq!(
            parse_diff_path_token("\"pricing_\\376.rs\""),
            Some("pricing_\\376.rs".to_string())
        );
        // Escaped backslash and metacharacters decode as before.
        assert_eq!(
            parse_diff_path_token("\"a\\\\b\\tc.rs\""),
            Some("a\\b\tc.rs".to_string())
        );
        // No closing quote stays malformed.
        assert_eq!(parse_diff_path_token("\"unclosed.rs"), None);
    }

    #[test]
    fn parse_new_path_marker_strips_b_prefix() {
        let path = parse_new_path_marker("+++ b/src/lib.rs");
        assert_eq!(path, Some(PathBuf::from("src/lib.rs")));
    }

    #[test]
    fn parse_new_path_marker_accepts_no_prefix() {
        let path = parse_new_path_marker("+++ src/lib.rs");
        assert_eq!(path, Some(PathBuf::from("src/lib.rs")));
    }

    #[test]
    fn parse_new_path_marker_returns_none_for_dev_null() {
        assert_eq!(parse_new_path_marker("+++ /dev/null"), None);
    }

    #[test]
    fn parse_git_old_path_confines_the_section_identity() {
        assert_eq!(
            parse_git_old_path("diff --git a/src/old.rs b/src/old.rs"),
            Some(PathBuf::from("src/old.rs"))
        );
        assert_eq!(
            parse_git_old_path("diff --git a/../../../etc/passwd b/../../../etc/passwd"),
            None
        );
    }

    #[test]
    fn parse_new_path_marker_rejects_traversal() {
        assert_eq!(parse_new_path_marker("+++ b/../escape.rs"), None);
        assert_eq!(parse_new_path_marker("+++ ../../../etc/passwd"), None);
    }

    #[test]
    fn parse_new_path_marker_strips_curdir() {
        // A path like `./src/lib.rs` normalizes to `src/lib.rs`.
        let path = parse_new_path_marker("+++ b/./src/lib.rs");
        assert_eq!(path, Some(PathBuf::from("src/lib.rs")));
    }

    #[test]
    fn parse_new_path_marker_parses_c_quoted_path() {
        let path = parse_new_path_marker("+++ \"b/src/my file.rs\"");
        assert_eq!(path, Some(PathBuf::from("src/my file.rs")));
    }

    #[test]
    fn parse_new_path_marker_parses_octal_escape() {
        // \040 is the C-quoted form of a space.
        let path = parse_new_path_marker("+++ \"b/src/my\\040file.rs\"");
        assert_eq!(path, Some(PathBuf::from("src/my file.rs")));
    }

    #[test]
    fn is_new_path_marker_recognizes_valid_markers() {
        assert!(is_new_path_marker("+++ b/src/lib.rs"));
        assert!(is_new_path_marker("+++ /dev/null"));
        assert!(is_new_path_marker("+++ \"b/src/file with space.rs\""));
    }

    #[test]
    fn is_new_path_marker_rejects_non_markers() {
        assert!(!is_new_path_marker("--- a/src/lib.rs"));
        assert!(!is_new_path_marker("++ payload with spaces"));
        assert!(!is_new_path_marker("regular line"));
    }

    #[test]
    fn parse_old_path_marker_recognizes_valid_markers() {
        assert!(parse_old_path_marker("--- a/src/lib.rs"));
        assert!(parse_old_path_marker("--- /dev/null"));
    }

    #[test]
    fn parse_old_path_marker_rejects_non_markers() {
        assert!(!parse_old_path_marker("+++ b/src/lib.rs"));
        assert!(!parse_old_path_marker("regular line"));
    }
}
