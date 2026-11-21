//! Pure helpers shared by portable help tests and repository guide audits.

#[path = "doc_invocations.rs"]
mod invocation_parser;

use invocation_parser::doc_invocations;

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
            // `check --help` is a one-screen summary; the docs guard needs the
            // exhaustive option reference behind `--all`.
            if path == ["check"] {
                args.push("--all");
            }
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
        for (line, invocation) in doc_invocations(doc) {
            let Some((command, help)) = command_help(&invocation.words, cache, run_ripr)? else {
                continue;
            };
            for flag in &invocation.flags {
                if !help_lists_flag(&help, flag) {
                    drift.push(format!(
                        "{name}:{line} `ripr {command} {flag}` is not an option of `ripr {command} --help`"
                    ));
                }
            }
        }
    }
    Ok(drift)
}
