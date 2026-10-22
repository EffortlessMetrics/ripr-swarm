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

/// Split `command` in the form `shell_arg` prints on Windows (#7729).
///
/// The console form follows the native Windows argv rules (the Microsoft C
/// runtime rules Rust's `std::env::args` follows), which is what `ripr.exe`
/// receives when the line is pasted into cmd.exe: a double quote toggles
/// quoting, `""` inside quotes is one literal quote, and a backslash run is
/// literal unless a quote follows it (2n backslashes then `"` are n
/// backslashes and a toggle, 2n+1 are n backslashes and a literal quote).
///
/// A value carrying `$` or backtick is printed in the single-quote form
/// instead (Git Bash and PowerShell expand both inside double quotes), so a
/// token that opens a single quote is decoded with the POSIX rules from
/// there on, as Git Bash reads it.
///
/// Bytes neither form prints are errors, so a producer regression fails
/// here instead of decoding to a different argument: `$` or backtick outside
/// a single-quoted run, a double quote inside a single-quote token (the
/// control-character `"$(printf …)"` form is bash source), cmd.exe operators
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
        // Set once the token opens a single quote: the rest is POSIX source.
        let mut posix = false;
        while let Some(ch) = chars.next() {
            match ch {
                ' ' | '\t' if !quoted => break,
                '\'' if !quoted => {
                    posix = true;
                    loop {
                        match chars.next() {
                            Some('\'') => break,
                            Some(inner) => word.push(inner),
                            None => return Err(format!("unterminated quote in `{command}`")),
                        }
                    }
                }
                '\\' if posix => word.push(
                    chars
                        .next()
                        .ok_or_else(|| format!("trailing backslash in `{command}`"))?,
                ),
                '"' if posix => {
                    return Err(format!(
                        "double quote inside a single-quote token is bash source: `{command}`"
                    ));
                }
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
                '$' | '`' => {
                    return Err(format!(
                        "`{ch}` outside single quotes expands in Git Bash and PowerShell: `{command}`"
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
