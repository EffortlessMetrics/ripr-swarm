/// One documented `ripr ...` invocation: the leading lowercase words (the
/// candidate command path) and every `--flag` it passes.
#[derive(Debug, PartialEq)]
pub(super) struct DocInvocation {
    pub(super) words: Vec<String>,
    pub(super) flags: Vec<String>,
}

/// Extract `ripr ...` invocations that pass at least one flag. A command
/// ends at a code-span, pipe, or shell separator; a trailing `\` joins the
/// next line so multi-line Bash examples are read whole.
pub(super) fn doc_invocations(doc: &str) -> Vec<(usize, DocInvocation)> {
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
                found.push((index + 1, DocInvocation { words, flags }));
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
