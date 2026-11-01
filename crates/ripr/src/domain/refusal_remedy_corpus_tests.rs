//! Corpus proof for [`RefusalRemedy`] (#11790): every message-shaped string
//! literal in the shipped source yields a remedy grounded in its own prose or
//! an explicit `none`, never an invented one.
//!
//! The corpus is the source tree itself, so a refusal added later is covered
//! without a list to maintain. The refusal-prose guard
//! (`tests/refusal_next_step.rs`) keeps every refusal naming a next step; this
//! test proves the structured projection of that prose is faithful.

use super::*;
use std::path::{Path, PathBuf};

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("read entry in {}: {error}", dir.display()))?
            .path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            if name != "tests" && name != "testing" {
                source_files(&path, out)?;
            }
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Plain `"..."` literals of `source`, escapes kept as written.
fn string_literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = source.chars();
    let mut current: Option<String> = None;
    while let Some(ch) = chars.next() {
        match &mut current {
            Some(text) => match ch {
                '\\' => match chars.next() {
                    // A line-continuation joins the lines, dropping the
                    // indentation, exactly as the compiler does.
                    Some('\n') => {
                        let rest = chars.as_str();
                        let trimmed = rest.trim_start();
                        chars = trimmed.chars();
                    }
                    Some(next) => {
                        text.push(ch);
                        text.push(next);
                    }
                    None => text.push(ch),
                },
                '"' => out.extend(current.take()),
                _ => text.push(ch),
            },
            None if ch == '"' => current = Some(String::new()),
            None => {}
        }
    }
    out
}

fn corpus() -> Result<Vec<String>, String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    source_files(&src, &mut files)?;
    files.sort();
    let mut messages = Vec::new();
    for path in files {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let body = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(source.as_str());
        messages.extend(
            string_literals(body)
                .into_iter()
                .filter(|literal| literal.len() >= 20 && literal.contains(' ')),
        );
    }
    Ok(messages)
}

/// Independent oracle: a backticked `ripr <word>` span the prose uses as an
/// instruction or fallback, i.e. not merely described (`X is a `ripr check`
/// finding ID`). A `none` result must have no such span.
fn stated_ripr_command_span(prose: &str) -> bool {
    let mut offset = 0;
    for (index, part) in prose.split('`').enumerate() {
        let at = offset;
        offset += part.len() + 1;
        if index % 2 == 0 {
            continue;
        }
        let is_command = part.trim().strip_prefix("ripr ").is_some_and(|rest| {
            rest.chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '-')
        });
        if !is_command {
            continue;
        }
        let before = &prose[..at.saturating_sub(1)];
        let sentence = before
            .rsplit([';'])
            .next()
            .unwrap_or(before)
            .to_ascii_lowercase();
        let described = [" is a ", " is an ", " is the "]
            .iter()
            .any(|copula| sentence.contains(copula))
            && !sentence.contains("run ");
        if !described {
            return true;
        }
    }
    false
}

/// Independent recall oracle: `--flag` tokens not inside a backticked
/// `ripr ...` command span.
fn flags_outside_command_spans(prose: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (index, part) in prose.split('`').enumerate() {
        let in_tick = index % 2 == 1;
        if in_tick && part.trim().starts_with("ripr ") {
            continue;
        }
        for token in part.split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')') {
            let flag = token.trim_end_matches(['.', ';', ':', '`']);
            let bare = flag.strip_prefix("--");
            if bare.is_some_and(|rest| {
                rest.starts_with(|c: char| c.is_ascii_lowercase())
                    && rest
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            }) && !out.contains(&flag.to_string())
            {
                out.push(flag.to_string());
            }
        }
    }
    out
}

#[test]
fn every_message_yields_a_grounded_remedy_or_an_explicit_none() -> Result<(), String> {
    let messages = corpus()?;
    let mut commands = 0;
    let mut tools = 0;
    let mut flags = 0;
    let mut nones = 0;
    let mut failures = Vec::new();
    for message in &messages {
        let remedy = RefusalRemedy::from_prose(message);
        if let Some(command) = &remedy.command {
            if !message.contains(command.as_str()) {
                failures.push(format!("command not in prose: {command} <- {message}"));
            }
        }
        if let Some(tool) = &remedy.tool {
            if !message.contains(tool.as_str()) {
                failures.push(format!("tool not in prose: {tool} <- {message}"));
            }
        }
        for flag in &remedy.flags {
            if !message.contains(flag.as_str()) {
                failures.push(format!("flag not in prose: {flag} <- {message}"));
            }
        }
        match remedy.kind {
            RemedyKind::Command => {
                commands += 1;
                if remedy.command.is_none() {
                    failures.push(format!("command kind without command: {message}"));
                }
            }
            RemedyKind::McpTool => {
                tools += 1;
                if remedy.tool.is_none() || remedy.command.is_some() {
                    failures.push(format!("mcp_tool kind malformed: {message}"));
                }
            }
            RemedyKind::Flags => {
                flags += 1;
                if remedy.flags.is_empty() || remedy.command.is_some() || remedy.tool.is_some() {
                    failures.push(format!("flags kind malformed: {message}"));
                }
            }
            RemedyKind::None => {
                nones += 1;
                // Fail closed in the other direction too: a stated ripr
                // command must never be dropped to `none`.
                if stated_ripr_command_span(message) {
                    failures.push(format!("stated command dropped to none: {message}"));
                }
                if remedy.command.is_some()
                    || remedy.tool.is_some()
                    || !remedy.flags.is_empty()
                    || !remedy.alternatives.is_empty()
                    || !remedy.fill_in.is_empty()
                    || remedy.conditional
                    || remedy.prose_has_more
                {
                    failures.push(format!("none carries stray fields: {message}"));
                }
            }
        }
        // Recall: when a step is stated, every `--flag` the message names
        // outside a command span must be reported, and the remedy must say
        // the prose has more.
        // Help and usage blobs are not refusal messages; skip them.
        if message.len() <= 700 && matches!(remedy.kind, RemedyKind::Command | RemedyKind::McpTool)
        {
            // The list is bounded (16); beyond that the first 16 must be present.
            for flag in flags_outside_command_spans(message)
                .into_iter()
                .take(MAX_LISTED)
            {
                if !remedy.flags.contains(&flag) {
                    failures.push(format!("flag {flag} missed by remedy: {message}"));
                }
                if !remedy.prose_has_more {
                    failures.push(format!("prose_has_more unset for {flag}: {message}"));
                }
            }
        }
        if remedy.kind != RemedyKind::Command && !remedy.fill_in.is_empty() {
            failures.push(format!("fill_in without a command: {message}"));
        }
    }
    // Instrument check: the corpus is real and exercises every outcome.
    if messages.len() < 1000 || commands < 50 || flags < 5 || nones < 100 {
        failures.push(format!(
            "corpus too thin: {} literals, {commands} command, {tools} mcp_tool, {flags} flags, {nones} none",
            messages.len()
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        failures.truncate(20);
        Err(failures.join("\n"))
    }
}

#[test]
fn a_reason_only_message_is_never_given_a_guessed_remedy() {
    // Negative control: a refusal that states no next step must stay `none`
    // even though it names ripr, a path and a flag-shaped word.
    let remedy = RefusalRemedy::from_prose(
        "ripr could not read `src/lib.rs`: the flag --json was ignored by the internal state",
    );
    assert_eq!(remedy, RefusalRemedy::none());
}

#[test]
fn the_oracle_tells_an_instructed_command_from_a_contextual_mention() {
    // Negative control: a described mention is not a stated command, an
    // instructed one is.
    assert!(!stated_ripr_command_span(
        "`probe:x` is a `ripr check` finding ID"
    ));
    assert!(stated_ripr_command_span(
        "`probe:x` is a finding ID; run `ripr check`"
    ));
    assert!(stated_ripr_command_span(
        "pass the id; if stale, run `ripr doctor`"
    ));
}

#[test]
fn the_recall_oracle_sees_flags_outside_the_span() {
    assert_eq!(
        flags_outside_command_spans("run `ripr x --in-span`, repeating `--root` and --other"),
        vec!["--root".to_string(), "--other".to_string()]
    );
}

#[test]
fn a_dropped_command_would_be_caught_by_the_corpus_oracle() {
    // The oracle itself: a message with a stated command must not be `none`.
    let prose = "stale; run `ripr doctor --root .` first";
    assert!(stated_ripr_command_span(prose));
    assert_ne!(RefusalRemedy::from_prose(prose).kind, RemedyKind::None);
}
