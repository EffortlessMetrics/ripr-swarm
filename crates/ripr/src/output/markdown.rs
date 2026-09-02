/// Shell disclosure shown before generated command fences (#2628).
///
/// The command strings are bash source (`agent::loop_commands::shell_arg`), so
/// each fenced `bash` block is paired with a PowerShell translation derived by
/// [`powershell_command`]. Naming both shells — and the cmd.exe boundary — in
/// the prose keeps the packet honest on Windows; the wording mirrors the landed
/// `agent_workflow` disclosure so every generated-command surface states the
/// same contract. Shared here — beside the translation it describes — so the
/// fenced command surfaces do not fork one disclosure per module.
pub(crate) const COMMAND_SHELL_DISCLOSURE: &str = "Each command includes Bash and PowerShell variants. The Bash form uses POSIX single-quote quoting and `>` redirection; the PowerShell form uses PowerShell's doubled-quote equivalent and UTF-8 `Out-File` redirection. cmd.exe is not supported. On Windows, use either Git Bash or PowerShell. WSL bash is not a drop-in substitute: paths keep their Windows drive-letter prefix, which WSL resolves as a relative path.\n\n";

pub(crate) fn render_string_section(out: &mut String, title: &str, values: &[String]) {
    out.push_str(&format!("\n## {title}\n\n"));
    if values.is_empty() {
        out.push_str("- none\n");
    } else {
        for value in values {
            out.push_str(&format!("- {}\n", markdown_text(value)));
        }
    }
}

pub(crate) fn markdown_text(value: &str) -> String {
    value.replace('\\', "\\\\")
}

/// Translate a bash-rendered advisory command into its PowerShell form.
///
/// The bash string stays authoritative (`agent::loop_commands::shell_arg`
/// renders it); this derives a copy-pasteable PowerShell equivalent so a
/// Windows reader is not left with bash source that cmd.exe reads as literal
/// quotes and PowerShell rejects at the `'\''` escape (#2628). Exactly two
/// translations are applied:
///
/// - PowerShell single quotes are also literal inside, but its doubling idiom
///   differs: bash closes, escapes, and reopens (`'\''`) where PowerShell
///   doubles in place (`''`), so every occurrence is rewritten.
/// - bash `>` redirection becomes a .NET write with BOM-free UTF-8, so Windows
///   PowerShell 5.1 does not produce a UTF-16 or BOM-prefixed file that JSON
///   consumers would reject. The target is rendered as a PowerShell string
///   literal — method-call arguments parse in expression mode, where a bare
///   path is a parse error (PR #3617 review) — and the redirect is detected
///   only outside single-quoted regions, so a quoted `>` inside an argument
///   cannot hijack it.
///
/// cmd.exe has no translation: it has no quoting form that keeps an argv token
/// literal, so a generated command is deliberately not offered for it. This
/// lives beside the markdown render helpers because every generated-command
/// surface renders both shell variants from this one implementation.
///
/// Disclosed limitation: the tests pin these forms as strings; no pwsh runtime
/// oracle that executes them is assumed on every host.
pub(crate) fn powershell_command(command: &str) -> String {
    let command = command.replace("'\\''", "''");
    let mut chars = command.char_indices().peekable();
    let mut in_single_quote = false;
    let mut redirect = None;
    while let Some((index, ch)) = chars.next() {
        if ch == '\'' {
            if in_single_quote && chars.peek().is_some_and(|(_, next)| *next == '\'') {
                chars.next();
                continue;
            }
            in_single_quote = !in_single_quote;
        } else if !in_single_quote
            && ch == '>'
            && command[..index].ends_with(' ')
            && command[index + ch.len_utf8()..].starts_with(' ')
        {
            redirect = Some(index);
            break;
        }
    }
    if let Some(index) = redirect {
        let invocation = command[..index].trim_end();
        let output = powershell_literal(command[index + 1..].trim());
        return format!(
            "[System.IO.File]::WriteAllText({output}, (({invocation}) | Out-String), [System.Text.UTF8Encoding]::new($false))"
        );
    }
    command
}

/// Render one value as a PowerShell single-quoted string literal.
///
/// A value that is already a single-quoted literal passes through: the bash
/// form quotes every argument that needs quoting, and the `'\''` rewrite in
/// [`powershell_command`] has already made such an interior PowerShell-valid.
/// Anything else is wrapped, doubling any embedded `'` — without the wrapping,
/// `WriteAllText(target/ripr/out.json, ...)` would not parse at the copy site.
fn powershell_literal(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_text_escapes_backslashes() {
        assert_eq!(markdown_text("a\\b"), "a\\\\b");
        assert_eq!(markdown_text("no backslash"), "no backslash");
    }

    #[test]
    fn render_string_section_lists_values_or_none() {
        let mut out = String::new();
        render_string_section(&mut out, "Example", &[]);
        assert_eq!(out, "\n## Example\n\n- none\n");

        let mut out = String::new();
        render_string_section(&mut out, "Example", &["a\\b".to_string()]);
        assert_eq!(out, "\n## Example\n\n- a\\\\b\n");
    }

    #[test]
    fn powershell_command_handles_unredirected_quoted_and_unicode_commands() {
        assert_eq!(
            powershell_command("ripr check --root 'a > b'"),
            "ripr check --root 'a > b'"
        );
        assert_eq!(
            powershell_command("ripr check --root 'café' > 'résumé.json'"),
            "[System.IO.File]::WriteAllText('résumé.json', ((ripr check --root 'café') | Out-String), [System.Text.UTF8Encoding]::new($false))"
        );
    }

    /// The PowerShell single-quote round-trip: bash's close-escape-reopen idiom
    /// (`'\''`) is rewritten to PowerShell's doubled quote (`''`), and
    /// PowerShell reads `it''s` back as the original bytes `it's`. An embedded
    /// quote left as `'\''` would be a syntax error at the copy site.
    #[test]
    fn powershell_command_round_trips_embedded_quotes_through_doubling() {
        let bash = "ripr receipt write --gap 'it'\\''s'";
        assert_eq!(powershell_command(bash), "ripr receipt write --gap 'it''s'");
        // A quoted `>` inside an argument must not be mistaken for a redirect.
        assert_eq!(
            powershell_command("ripr receipt write --gap 'gap > file'"),
            "ripr receipt write --gap 'gap > file'"
        );
    }

    /// PowerShell parses method-call arguments in expression mode, where a
    /// bare path like `target/ripr/out.json` is a parse error before anything
    /// runs (PR #3617 review, gemini HIGH + codex P1): the redirect target
    /// must arrive as a quoted literal even when the bash form left it
    /// unquoted. This is the default pilot path shape.
    #[test]
    fn powershell_command_quotes_the_default_unquoted_redirect_target() {
        assert_eq!(
            powershell_command(
                "ripr check --root . --mode draft --format repo-exposure-json > target/ripr/pilot/after.repo-exposure.json"
            ),
            "[System.IO.File]::WriteAllText('target/ripr/pilot/after.repo-exposure.json', ((ripr check --root . --mode draft --format repo-exposure-json) | Out-String), [System.Text.UTF8Encoding]::new($false))"
        );
    }

    /// An embedded quote in an unwrapped target must survive the literal: the
    /// wrapper doubles it. A target the bash form already single-quoted passes
    /// through with its interior `''` doubling intact.
    #[test]
    fn powershell_command_redirect_target_escapes_embedded_quotes() {
        assert_eq!(
            powershell_command("ripr check --root . > it's.json"),
            "[System.IO.File]::WriteAllText('it''s.json', ((ripr check --root .) | Out-String), [System.Text.UTF8Encoding]::new($false))"
        );
        assert_eq!(
            powershell_command("ripr check --root . > 'it'\\''s.json'"),
            "[System.IO.File]::WriteAllText('it''s.json', ((ripr check --root .) | Out-String), [System.Text.UTF8Encoding]::new($false))"
        );
    }
}
