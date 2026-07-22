use std::path::{Component, Path, PathBuf};

pub(super) fn parse_new_path_marker(raw: &str) -> Option<PathBuf> {
    let marker = raw.strip_prefix("+++ ")?;
    let path = parse_diff_path_token(marker)?;
    if path == "/dev/null" {
        return None;
    }
    let path = path.strip_prefix("b/").unwrap_or(&path);
    let parsed = PathBuf::from(path);
    if is_path_safe(&parsed) {
        Some(parsed)
    } else {
        None
    }
}

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

fn is_plausible_unquoted_diff_path(path: &str) -> bool {
    !path.is_empty() && !path.chars().any(char::is_whitespace)
}

/// Reject paths that traverse outside the workspace root or are absolute.
/// A git diff path like `b/../../../etc/passwd` would otherwise be joined
/// to the workspace root at the consumer site (`root.join(changed.path)`),
/// producing a path outside root. This guard stops the traversal at the
/// parser boundary so every downstream consumer inherits the protection.
/// See #2099.
fn is_path_safe(path: &Path) -> bool {
    !path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
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
    let mut path = String::new();
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(path),
            '\\' => path.push(parse_c_escape(&mut chars)),
            _ => path.push(ch),
        }
    }

    None
}

fn parse_c_escape<I>(chars: &mut std::iter::Peekable<I>) -> char
where
    I: Iterator<Item = char>,
{
    let Some(ch) = chars.next() else {
        return '\\';
    };

    match ch {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        '\\' => '\\',
        '"' => '"',
        '0'..='7' => parse_octal_escape(ch, chars),
        _ => ch,
    }
}

fn parse_octal_escape<I>(first: char, chars: &mut std::iter::Peekable<I>) -> char
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

    char::from_u32(value).unwrap_or('\u{FFFD}')
}

#[cfg(test)]
mod tests {
    use super::parse_new_path_marker;
    use std::path::PathBuf;

    #[test]
    fn parse_new_path_marker_accepts_normal_relative_path() -> Result<(), String> {
        let path = parse_new_path_marker("+++ b/src/lib.rs")
            .ok_or_else(|| "expected Some for normal relative path".to_string())?;
        assert_eq!(path, PathBuf::from("src/lib.rs"));
        Ok(())
    }

    #[test]
    fn parse_new_path_marker_rejects_parent_dir_traversal() {
        // `+++ b/../../../etc/passwd` must be rejected — the path would
        // escape the workspace root when joined at the consumer site.
        assert_eq!(parse_new_path_marker("+++ b/../../../etc/passwd"), None);
    }

    #[test]
    fn parse_new_path_marker_rejects_single_parent_dir() {
        assert_eq!(parse_new_path_marker("+++ ../secret"), None);
    }

    #[test]
    fn parse_new_path_marker_rejects_absolute_path() {
        assert_eq!(parse_new_path_marker("+++ /etc/passwd"), None);
    }

    #[test]
    fn parse_new_path_marker_accepts_nested_subdirectory() -> Result<(), String> {
        let path = parse_new_path_marker("+++ b/src/nested/deep/file.rs")
            .ok_or_else(|| "expected Some for nested path".to_string())?;
        assert_eq!(path, PathBuf::from("src/nested/deep/file.rs"));
        Ok(())
    }
}
