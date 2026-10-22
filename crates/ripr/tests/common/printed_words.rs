//! Split a `ripr` command the way the shell it was printed for would (#6762,
//! Windows replay #9272 cluster 1).
//!
//! Producers quote values with `shell_arg`, which follows the host's shell
//! family (#7729): POSIX single quotes on Unix, the Windows console
//! double-quote form on Windows. A replay must decode the form this host
//! printed. Splitting on whitespace breaks any path that needs quoting and
//! keeps the quotes as literal bytes, and decoding the Windows form with
//! POSIX rules rejects or mis-splits a command a Windows user can paste.

/// Split `command` into the argv this host's shell would pass, for the form
/// `shell_arg` prints on this host.
pub(crate) fn printed_words(command: &str) -> Result<Vec<String>, String> {
    if cfg!(windows) {
        windows_console_words(command)
    } else {
        posix_words(command)
    }
}

/// Split `command` into the argv a POSIX shell would pass for the subset
/// `shell_arg` prints on Unix: bare words, single quotes and backslash
/// escapes outside them. Double quotes (the `ansi_c_quote` form for control
/// or bidi characters), unterminated quotes and a trailing backslash are
/// errors, not silently repaired tokens.
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

/// Split `command` with the native Windows argv rules (the Microsoft C
/// runtime rules Rust's `std::env::args` follows), which is what `ripr.exe`
/// receives when the line is pasted into cmd.exe. These are the rules
/// `windows_console_quote` encodes for: a double quote toggles quoting, `""`
/// inside quotes is one literal quote, and a backslash run is literal unless a
/// quote follows it (2n backslashes then `"` are n backslashes and a toggle,
/// 2n+1 are n backslashes and a literal quote).
///
/// Bytes the console form never prints are errors, so a producer regression
/// fails here instead of decoding to a different argument: a single quote
/// outside double quotes (the `$`/backtick and control-character forms are
/// Git Bash or PowerShell source, not cmd.exe argv), cmd.exe operators
/// (`&`, `|`, `<`, `^`) outside quotes, `%` anywhere (cmd.exe expands
/// `%NAME%` even inside double quotes), and an unterminated quote.
pub(crate) fn windows_console_words(command: &str) -> Result<Vec<String>, String> {
    if command.contains('%') {
        return Err(format!(
            "`%` does not survive a cmd.exe paste, even quoted: `{command}`"
        ));
    }
    let mut words = Vec::new();
    let mut chars = command.chars().peekable();
    loop {
        while chars.peek().is_some_and(|ch| matches!(ch, ' ' | '\t')) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut word = String::new();
        let mut quoted = false;
        while let Some(ch) = chars.next() {
            match ch {
                ' ' | '\t' if !quoted => break,
                '\\' => {
                    let mut run = 1;
                    while chars.peek() == Some(&'\\') {
                        chars.next();
                        run += 1;
                    }
                    if chars.peek() == Some(&'"') {
                        word.push_str(&"\\".repeat(run / 2));
                        if run % 2 == 1 {
                            chars.next();
                            word.push('"');
                        }
                    } else {
                        word.push_str(&"\\".repeat(run));
                    }
                }
                '"' if quoted && chars.peek() == Some(&'"') => {
                    chars.next();
                    word.push('"');
                }
                '"' => quoted = !quoted,
                '\'' if !quoted => {
                    return Err(format!(
                        "single-quoted form is not cmd.exe argv: `{command}`"
                    ));
                }
                '&' | '|' | '<' | '^' if !quoted => {
                    return Err(format!("unquoted cmd.exe operator `{ch}` in `{command}`"));
                }
                other => word.push(other),
            }
        }
        if quoted {
            return Err(format!("unterminated quote in `{command}`"));
        }
        words.push(word);
    }
    Ok(words)
}
