//! Replay splitting for printed `ripr` commands (#6762). The host-aware
//! splitter lives in `common/printed_words.rs`, shared with the
//! boundary-gap harness; this module adds the redirect split and the tests.

#[path = "../common/printed_words.rs"]
mod printed_words;

pub(crate) use printed_words::{posix_words, printed_words, windows_console_words};

/// Split a printed `ripr … > path` command after host word splitting, so a
/// quoted root that contains ` > ` stays one argument (#6942). `split_once(" > ")`
/// would cut inside that quoted path.
pub(crate) fn printed_words_with_redirect(command: &str) -> Result<(Vec<String>, String), String> {
    split_redirect(command, printed_words(command)?)
}

fn posix_words_with_redirect(command: &str) -> Result<(Vec<String>, String), String> {
    split_redirect(command, posix_words(command)?)
}

fn split_redirect(command: &str, words: Vec<String>) -> Result<(Vec<String>, String), String> {
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

/// The Windows console form `shell_arg` prints on Windows (#7729, #8152)
/// decodes with the native argv rules on every host, so this pins the
/// Windows replay splitter on the Linux lane too.
#[test]
fn windows_console_words_decode_the_printed_console_form() -> Result<(), String> {
    for (printed, expected) in [
        (
            r#"ripr continue --root "/srv/after test not run" --attempt repair-attempt-1"#,
            vec![
                "ripr",
                "continue",
                "--root",
                "/srv/after test not run",
                "--attempt",
                "repair-attempt-1",
            ],
        ),
        (
            r#"ripr doctor --root "/srv/selected répo it's""#,
            vec!["ripr", "doctor", "--root", "/srv/selected répo it's"],
        ),
        // Interior `"` is doubled; a backslash run is literal unless a quote
        // follows, and a trailing run is doubled before the closing quote.
        (r#"x "a""b""#, vec!["x", r#"a"b"#]),
        (r#"x "srv\a b\c""#, vec!["x", r"srv\a b\c"]),
        (r#"x "srv\repo dir\\""#, vec!["x", r"srv\repo dir\"]),
        (r#"x srv\plain\path"#, vec!["x", r"srv\plain\path"]),
        (r#"x "" y"#, vec!["x", "", "y"]),
        (
            r#"ripr check --root "/srv/r x" > "/srv/r x/out.json""#,
            vec![
                "ripr",
                "check",
                "--root",
                "/srv/r x",
                ">",
                "/srv/r x/out.json",
            ],
        ),
        // `%` and `!` are literal outside cmd.exe (the console form's
        // documented limit, #9659).
        (
            r#"ripr check --root "/srv/100% done!/repo""#,
            vec!["ripr", "check", "--root", "/srv/100% done!/repo"],
        ),
        // `$` keeps the single-quote form on Windows, read as Git Bash does.
        (
            r"ripr doctor --root 'srv\x\it'\''s $'",
            vec!["ripr", "doctor", "--root", r"srv\x\it's $"],
        ),
        // A recovery hint's `<ref>` placeholder stays one word, as
        // `posix_words` reads it (#9272 run 39819783344).
        (
            "ripr first-pr --base <ref> --head HEAD",
            vec!["ripr", "first-pr", "--base", "<ref>", "--head", "HEAD"],
        ),
    ] {
        assert_eq!(windows_console_words(printed)?, expected, "{printed}");
    }
    for rejected in [
        r#"ripr check --root "a$b""#,
        r#"ripr check --root 'a'"$(printf '\033')"'b'"#,
        r#"ripr check --root "open"#,
        "ripr check --root a & b",
    ] {
        if let Ok(words) = windows_console_words(rejected) {
            return Err(format!(
                "`{rejected}` split into {words:?} instead of failing"
            ));
        }
    }
    Ok(())
}

/// `printed_words` dispatches on the host form: the POSIX form on Unix and
/// the console form on Windows, so a replay decodes what this host printed.
#[test]
fn printed_words_follow_the_host_quoting_form() -> Result<(), String> {
    let printed = if cfg!(windows) {
        r#"ripr explain --root "/srv/checkouts/my repo/it's""#
    } else {
        r"ripr explain --root '/srv/checkouts/my repo/it'\''s'"
    };
    assert_eq!(
        printed_words(printed)?,
        ["ripr", "explain", "--root", "/srv/checkouts/my repo/it's"]
    );
    Ok(())
}
