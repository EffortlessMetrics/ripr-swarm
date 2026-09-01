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
///   consumers would reject. The redirect is detected only outside single-quoted
///   regions, so a quoted `>` inside an argument cannot hijack it.
///
/// cmd.exe has no translation: it has no quoting form that keeps an argv token
/// literal, so a generated command is deliberately not offered for it. This
/// lives beside the markdown render helpers because every generated-command
/// surface renders both shell variants from this one implementation.
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
        let output = command[index + 1..].trim();
        return format!(
            "[System.IO.File]::WriteAllText({output}, (({invocation}) | Out-String), [System.Text.UTF8Encoding]::new($false))"
        );
    }
    command
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
}
