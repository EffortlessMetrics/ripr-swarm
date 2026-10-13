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

/// Split a printed `ripr … > path` command after POSIX word splitting, so a
/// quoted root that contains ` > ` stays one argument (#6942). `split_once(" > ")`
/// would cut inside that quoted path.
pub(crate) fn posix_words_with_redirect(command: &str) -> Result<(Vec<String>, String), String> {
    let words = posix_words(command)?;
    let Some(at) = words.iter().position(|word| word == ">") else {
        return Err(format!("no redirect in `{command}`"));
    };
    if at == 0 {
        return Err(format!("redirect with no command in `{command}`"));
    }
    let redirect = words
        .get(at + 1)
        .ok_or_else(|| format!("redirect with no path in `{command}`"))?;
    if words.len() != at + 2 {
        return Err(format!(
            "after command redirect is not one word: `{command}`"
        ));
    }
    Ok((words[..at].to_vec(), redirect.clone()))
}

#[test]
fn posix_words_keep_quoted_checkout_paths_whole() -> Result<(), String> {
    let words = posix_words(
        "ripr explain --root '/srv/checkouts/my repo/it'\\''s' --worktree probe:a:1:b",
    )?;
    assert_eq!(
        words,
        [
            "ripr",
            "explain",
            "--root",
            "/srv/checkouts/my repo/it's",
            "--worktree",
            "probe:a:1:b"
        ]
    );
    for rejected in [
        "ripr check --root 'open",
        "ripr check --root 'a'\"$(printf '\\033')\"'b'",
    ] {
        if let Ok(words) = posix_words(rejected) {
            return Err(format!(
                "`{rejected}` split into {words:?} instead of failing"
            ));
        }
    }
    Ok(())
}

#[test]
fn posix_words_with_redirect_keep_quoted_gt_inside_the_root() -> Result<(), String> {
    let command = "ripr check --root '/tmp/foo > bar/repo' --mode draft > '/tmp/foo > bar/repo/after.repo-exposure.json'";
    let (words, redirect) = posix_words_with_redirect(command)?;
    assert_eq!(
        words,
        [
            "ripr",
            "check",
            "--root",
            "/tmp/foo > bar/repo",
            "--mode",
            "draft"
        ]
    );
    assert_eq!(redirect, "/tmp/foo > bar/repo/after.repo-exposure.json");
    // The review-era `split_once(" > ")` cut inside the quoted root.
    let (check_part, _redirect) = command
        .split_once(" > ")
        .ok_or("fixture command must contain a redirect")?;
    if check_part.contains("/tmp/foo > bar/repo") {
        return Err(format!(
            "split_once(\" > \") unexpectedly kept the quoted root whole: {check_part}"
        ));
    }
    Ok(())
}
