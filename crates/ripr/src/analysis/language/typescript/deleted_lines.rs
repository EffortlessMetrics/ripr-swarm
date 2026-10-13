//! Removed-only TypeScript probes (#7689, RIPR-SPEC-0156).
//!
//! A deleted behavior line that no added line pairs with left no record at
//! all: a pure deletion, or a code line whose comment pairs by content with a
//! re-added comment, read as "no behavioral candidates". The Rust and Python
//! adapters keep such a line as a removed-only probe whose
//! `source_currentness` says it is base-side evidence; this module does the
//! same for TypeScript and JavaScript. A base-side record is never a
//! candidate edit target, so every counted, gated, annotated and diagnostic
//! surface skips it (#3281).

use super::paths::normalized_path;
use super::probe_shape::should_ignore_typescript_changed_line;
use super::types::TypeScriptOwner;
use super::{ChangedFile, Finding};
use crate::analysis::diff::ChangedLine;
use crate::domain::SourceCurrentness;
use std::collections::{BTreeMap, BTreeSet};

/// One base-side finding per removed behavior line that no added line pairs
/// with. `counterparts` is the file's one pairing
/// (`ChangedFile::aligned_counterparts`); `findings` holds the findings so
/// far so a repeated probe id gets an ordinal suffix. `classify` classifies
/// a line at a resolved owner.
pub(super) fn deleted_line_findings(
    changed: &ChangedFile,
    counterparts: &BTreeMap<usize, &ChangedLine>,
    owners: &[TypeScriptOwner],
    findings: &[Finding],
    classify: impl Fn(usize, &str, &TypeScriptOwner) -> Option<Finding>,
) -> Vec<Finding> {
    let paired: BTreeSet<usize> = counterparts.values().map(|removed| removed.line).collect();
    // Removed headers an added header replaced: the owner they open lives on.
    let re_headed: BTreeSet<usize> = counterparts
        .iter()
        .filter(|(added_line, _)| {
            changed
                .added_lines
                .iter()
                .any(|added| added.line == **added_line && opens_an_owner(&added.text))
        })
        .map(|(_, removed)| removed.line)
        .collect();
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    for finding in findings {
        *ids.entry(finding.probe.id.0.clone()).or_default() += 1;
    }
    let mut out = Vec::new();
    for removed in &changed.removed_lines {
        if paired.contains(&removed.line) || !is_removed_behavior_line(&removed.text) {
            continue;
        }
        let Some(owner) = deleted_line_owner(changed, removed, &re_headed, owners) else {
            continue;
        };
        let Some(mut finding) = classify(removed.new_side_line, &removed.text, owner) else {
            continue;
        };
        let expression = removed.text.trim();
        // Same-text evidence is the only movement evidence; without it the
        // record stays `base_deleted` (RIPR-SPEC-0156).
        finding.source_currentness = if changed
            .added_lines
            .iter()
            .any(|added| added.text.trim() == expression)
        {
            SourceCurrentness::MovedOrRenamed
        } else {
            SourceCurrentness::BaseDeleted
        };
        finding.probe.before = Some(removed.text.clone());
        finding.probe.after = None;
        let count = ids.entry(finding.probe.id.0.clone()).or_default();
        *count += 1;
        if *count > 1 {
            finding.probe.id.0 = format!("{}.{}", finding.probe.id.0, count);
            finding.id = finding.probe.id.0.clone();
        }
        out.push(finding);
    }
    out
}

/// A removed line that ran behavior: not a comment, blank line or lone
/// bracket, not an import or re-export, not a line that only closes a block
/// (`} else {`), and not an owner's own header: a removed `function` or
/// method header deletes or re-shapes an owner, which this record does not
/// describe.
fn is_removed_behavior_line(text: &str) -> bool {
    let trimmed = text.trim_start();
    !should_ignore_typescript_changed_line(text)
        && !trimmed.starts_with('}')
        && !trimmed.starts_with("import ")
        && !trimmed.starts_with("export {")
        && !trimmed.starts_with("export *")
        && !opens_an_owner(text)
}

/// The live owner that held the deleted line. The diff parser records a
/// removed line at the new-side line that follows it, and a body's closing
/// brace survives a deleted statement, so the narrowest owner whose header
/// sits above that line and whose span reaches it held the deleted line. A
/// line whose own function or method was deleted with it belonged to an
/// owner that no longer exists, and resolves to none.
fn deleted_line_owner<'a>(
    changed: &ChangedFile,
    removed: &ChangedLine,
    re_headed: &BTreeSet<usize>,
    owners: &'a [TypeScriptOwner],
) -> Option<&'a TypeScriptOwner> {
    if lost_its_owner(changed, removed, re_headed) {
        return None;
    }
    let at = removed.new_side_line;
    let file = normalized_path(&changed.path);
    owners
        .iter()
        .filter(|owner| normalized_path(&owner.file) == file)
        .filter(|owner| owner.start_line < at && at <= owner.end_line)
        .min_by_key(|owner| owner.end_line - owner.start_line)
}

/// Whether the function, method or class that enclosed `removed` was
/// deleted with it. The walk goes back through the removed lines in
/// old-file order while they stay contiguous there, counting braces: the
/// first line that opens a block around the deleted line is its enclosing
/// header. An `if` or loop header leads on outward; an owner header ends the
/// walk. A kept line breaks contiguity, and its owner is live; so is a
/// removed header an added header pairs with, as when only its types changed.
fn lost_its_owner(
    changed: &ChangedFile,
    removed: &ChangedLine,
    re_headed: &BTreeSet<usize>,
) -> bool {
    let by_old_line: BTreeMap<usize, &ChangedLine> = changed
        .removed_lines
        .iter()
        .map(|line| (line.line, line))
        .collect();
    let mut depth: usize = 0;
    let mut old_line = removed.line;
    loop {
        let Some(previous) = old_line
            .checked_sub(1)
            .and_then(|line| by_old_line.get(&line))
        else {
            return false;
        };
        old_line = previous.line;
        let (opens, closes) = brace_counts(&previous.text);
        // Walking backward, a close reopens a sibling block and an open
        // leaves the current one.
        depth += closes;
        if opens <= depth {
            depth -= opens;
            continue;
        }
        if opens_an_owner(&previous.text) {
            return !re_headed.contains(&previous.line);
        }
        depth = 0;
    }
}

/// `{` and `}` outside string literals and line comments.
fn brace_counts(text: &str) -> (usize, usize) {
    let mut opens = 0;
    let mut closes = 0;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match quote {
            Some(open) => {
                if ch == '\\' {
                    chars.next();
                } else if ch == open {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' | '`' => quote = Some(ch),
                '/' if chars.peek() == Some(&'/') => break,
                '{' => opens += 1,
                '}' => closes += 1,
                _ => {}
            },
        }
    }
    (opens, closes)
}

/// Keywords that open a block with a parenthesised head but no owner.
const CONTROL_KEYWORDS: &[&str] = &[
    "if", "for", "while", "switch", "catch", "with", "return", "await", "new", "typeof", "super",
    "this", "throw", "yield", "void", "delete",
];

/// Modifiers that may precede an owner header.
const MODIFIERS: &[&str] = &[
    "export",
    "default",
    "async",
    "static",
    "public",
    "private",
    "protected",
    "readonly",
    "abstract",
    "override",
    "declare",
    "get",
    "set",
];

/// Whether a line opens a function, method, class or named arrow function:
/// the shapes the owner extractor indexes. A callback passed to a call
/// (`items.forEach((item) => {`) or an `if` block is not an owner.
fn opens_an_owner(text: &str) -> bool {
    let mut rest = text.trim();
    'strip: loop {
        for modifier in MODIFIERS {
            if let Some(after) = rest.strip_prefix(modifier)
                && after.starts_with(char::is_whitespace)
            {
                rest = after.trim_start();
                continue 'strip;
            }
        }
        break;
    }
    let function_keyword = rest
        .strip_prefix("function")
        .is_some_and(|after| after.starts_with([' ', '*', '(']));
    if function_keyword || rest.starts_with("class ") {
        return true;
    }
    if let Some(arrow) = rest.find("=>") {
        let head = &rest[..arrow];
        let binding = ["const ", "let ", "var "]
            .iter()
            .find_map(|keyword| head.strip_prefix(keyword))
            .unwrap_or(head);
        let name_end = binding
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '#'))
            .unwrap_or(binding.len());
        let after_name = binding[name_end..].trim_start();
        return name_end > 0 && (after_name.starts_with(':') || after_name.starts_with('='));
    }
    // A method header: a name, its parameters and a body brace.
    let name_end = rest
        .find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '#'))
        .unwrap_or(rest.len());
    let name = &rest[..name_end];
    let after_name = rest[name_end..].trim_start();
    name_end > 0
        && !CONTROL_KEYWORDS.contains(&name)
        && (after_name.starts_with('(') || after_name.starts_with('<'))
        && rest.trim_end().ends_with('{')
}
