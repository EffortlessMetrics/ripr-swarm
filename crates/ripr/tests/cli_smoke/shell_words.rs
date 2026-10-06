//! Bash-word splitting for replaying printed `ripr` commands (#6762).
//!
//! Producers quote values with `shell_arg` (POSIX single quotes, `'\''` for
//! an embedded quote). Splitting a printed command on whitespace breaks any
//! checkout path that needs quoting, such as one with a space, and keeps the
//! quotes as literal bytes. Replays split the way the shell would instead.

/// Split `command` into the argv a POSIX shell would pass for the subset
/// `shell_arg` prints: bare words, single quotes and backslash escapes
/// outside them. Double quotes (the `ansi_c_quote` form for control or bidi
/// characters), unterminated quotes and a trailing backslash are errors,
/// not silently repaired tokens.
pub(crate) fn posix_words(command: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut chars = command.chars().peekable();
    loop {
        while chars.peek().is_some_and(|ch| ch.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut word = String::new();
        while let Some(ch) = chars.peek().copied() {
            if ch.is_whitespace() {
                break;
            }
            chars.next();
            match ch {
                '\'' => loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => word.push(inner),
                        None => return Err(format!("unterminated quote in `{command}`")),
                    }
                },
                '\\' => word.push(
                    chars
                        .next()
                        .ok_or_else(|| format!("trailing backslash in `{command}`"))?,
                ),
                '"' => {
                    return Err(format!(
                        "double-quoted form is not replayable here: `{command}`"
                    ));
                }
                other => word.push(other),
            }
        }
        words.push(word);
    }
    Ok(words)
}

#[test]
fn posix_words_keep_quoted_checkout_paths_whole() -> Result<(), String> {
    let words =
        posix_words("ripr explain --root '/home/dev/my repo/it'\\''s' --worktree probe:a:1:b")?;
    assert_eq!(
        words,
        [
            "ripr",
            "explain",
            "--root",
            "/home/dev/my repo/it's",
            "--worktree",
            "probe:a:1:b"
        ]
    );
    assert!(posix_words("ripr check --root 'open").is_err());
    assert!(posix_words("ripr check --root 'a'\"$(printf '\\033')\"'b'").is_err());
    Ok(())
}
