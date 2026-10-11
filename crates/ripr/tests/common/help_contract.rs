//! Pure helpers shared by portable help tests and repository guide audits.

pub fn assert_contains(surface: &str, text: &str, needle: &str) -> Result<(), String> {
    if text.contains(needle) {
        return Ok(());
    }
    Err(format!("{surface} lost the canonical route `{needle}`"))
}

/// Collapse all whitespace runs so a needle survives line reflows and help
/// column realignment; the vocabulary and token order, not the wrapping, are
/// the contract.
pub fn normalized(doc: &str) -> String {
    doc.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One documented `ripr ...` invocation: the leading lowercase words (the
/// candidate command path) and every `--flag` it passes.
#[derive(Debug, PartialEq)]
struct DocInvocation {
    line: usize,
    words: Vec<String>,
    flags: Vec<String>,
}

/// Extract `ripr ...` invocations that pass at least one flag. A command
/// ends at a code-span, pipe, or shell separator; a trailing `\` joins the
/// next line so multi-line Bash examples are read whole.
fn doc_invocations(doc: &str) -> Vec<DocInvocation> {
    let lf = doc.replace("\r\n", "\n");
    let lines: Vec<&str> = lf.lines().collect();
    let mut found = Vec::new();
    for (index, raw) in lines.iter().enumerate() {
        let mut text = raw.to_string();
        let mut next = index + 1;
        while text.trim_end().ends_with('\\') && next < lines.len() {
            text = format!("{} {}", text.trim_end().trim_end_matches('\\'), lines[next]);
            next += 1;
        }
        let mut rest = text.as_str();
        while let Some(at) = rest.find("ripr ") {
            let boundary = rest[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || "-_/.".contains(c)));
            let tail = &rest[at + "ripr ".len()..];
            rest = tail;
            if !boundary {
                continue;
            }
            let command = command_text(tail);
            let tokens: Vec<&str> = command.split_whitespace().collect();
            let words = tokens
                .iter()
                .take(2)
                .take_while(|token| {
                    token.starts_with(|c: char| c.is_ascii_lowercase())
                        && token
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                })
                .map(|token| (*token).to_string())
                .collect::<Vec<_>>();
            let flags = tokens
                .iter()
                .filter_map(|token| token.strip_prefix("--"))
                .map(|flag| flag.split('=').next().unwrap_or(flag))
                .map(|flag| flag.trim_end_matches(['.', ',', ':', ']']))
                .filter(|flag| {
                    flag.starts_with(|c: char| c.is_ascii_lowercase())
                        && flag
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                })
                .map(|flag| format!("--{flag}"))
                .collect::<Vec<_>>();
            if !words.is_empty() && !flags.is_empty() {
                found.push(DocInvocation {
                    line: index + 1,
                    words,
                    flags,
                });
            }
        }
    }
    found
}

/// The command text after `ripr `: it ends at a code-span, pipe, or shell
/// separator outside quotes. Each quoted value becomes one `VALUE` token, so
/// a flag after `--diff "a b.patch"` is still checked; an unterminated quote
/// ends the command.
fn command_text(tail: &str) -> String {
    let mut text = String::new();
    let mut chars = tail.chars();
    while let Some(c) = chars.next() {
        match c {
            '`' | '|' | ';' | '&' | ')' | '#' => break,
            '"' | '\'' => {
                if !chars.by_ref().any(|close| close == c) {
                    break;
                }
                text.push_str("VALUE");
            }
            _ => text.push(c),
        }
    }
    text
}

fn help_lists_flag(help: &str, flag: &str) -> bool {
    help.match_indices(flag).any(|(at, _)| {
        help[at + flag.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '-'))
    })
}

/// Resolve the help screen for the longest documented command path that the
/// CLI accepts. Prose such as "ripr check with --json" falls back to
/// `ripr check`; a first word that is not a command is not an invocation.
fn command_help(
    words: &[String],
    cache: &mut std::collections::BTreeMap<Vec<String>, Option<String>>,
    run_ripr: &impl Fn(&[&str]) -> Result<std::process::Output, String>,
) -> Result<Option<(String, String)>, String> {
    for len in (1..=words.len()).rev() {
        let path = words[..len].to_vec();
        if !cache.contains_key(&path) {
            let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
            args.push("--help");
            let output = run_ripr(&args)?;
            let help = output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).into_owned());
            cache.insert(path.clone(), help);
        }
        if let Some(Some(help)) = cache.get(&path) {
            return Ok(Some((path.join(" "), help.clone())));
        }
    }
    Ok(None)
}

pub fn undocumented_doc_flags(
    docs: &[(&str, &str)],
    cache: &mut std::collections::BTreeMap<Vec<String>, Option<String>>,
    run_ripr: &impl Fn(&[&str]) -> Result<std::process::Output, String>,
) -> Result<Vec<String>, String> {
    let mut drift = Vec::new();
    for (name, doc) in docs {
        for invocation in doc_invocations(doc) {
            let Some((command, help)) = command_help(&invocation.words, cache, run_ripr)? else {
                continue;
            };
            for flag in &invocation.flags {
                if !help_lists_flag(&help, flag) {
                    drift.push(format!(
                        "{name}:{} `ripr {command} {flag}` is not an option of `ripr {command} --help`",
                        invocation.line
                    ));
                }
            }
        }
    }
    Ok(drift)
}
