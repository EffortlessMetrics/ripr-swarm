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
    let headers = removed_owner_headers(changed);
    // Removed headers an added header replaced: the owner they open lives on.
    let re_headed: BTreeSet<usize> = counterparts
        .iter()
        .filter(|(added_line, _)| {
            changed
                .added_lines
                .iter()
                .any(|added| added.line == **added_line && opens_an_owner(&added.text))
        })
        .filter_map(|(_, removed)| headers.get(&removed.line).copied())
        .collect();
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    for finding in findings {
        *ids.entry(finding.probe.id.0.clone()).or_default() += 1;
    }
    let mut out = Vec::new();
    for removed in &changed.removed_lines {
        if paired.contains(&removed.line)
            || headers.contains_key(&removed.line)
            || !is_removed_behavior_line(&removed.text)
        {
            continue;
        }
        let Some(owner) = deleted_line_owner(changed, removed, &headers, &re_headed, owners) else {
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
/// bracket, not an import or re-export, and not a line that only closes a
/// block (`} else {`). Owner headers are excluded by the caller: a removed
/// `function` or method header deletes or re-shapes an owner, which this
/// record does not describe.
fn is_removed_behavior_line(text: &str) -> bool {
    let trimmed = text.trim_start();
    !should_ignore_typescript_changed_line(text)
        && !trimmed.starts_with('}')
        && !trimmed.starts_with("import ")
        && !trimmed.starts_with("export {")
        && !trimmed.starts_with("export *")
}

/// Every removed line that is part of an owner header, keyed to the header's
/// first old line. A header can span lines: a parameter list Prettier wraps
/// (`save(` / `id: string,` / `) {`) or an Allman brace on its own line.
/// The lines of an open parenthesis join until it closes, unless a line
/// opens a block first, as a callback passed to a call does.
fn removed_owner_headers(changed: &ChangedFile) -> BTreeMap<usize, usize> {
    let mut by_old_line: Vec<&ChangedLine> = changed.removed_lines.iter().collect();
    by_old_line.sort_by_key(|line| line.line);
    let mut headers = BTreeMap::new();
    let mut group: Vec<&ChangedLine> = Vec::new();
    let mut depth: isize = 0;
    for line in by_old_line {
        let code = code_only(&line.text);
        let continues = group.last().is_some_and(|last| {
            last.line + 1 == line.line
                && ((depth > 0 && !code_only(&last.text).trim_end().ends_with('{'))
                    || code.trim() == "{")
        });
        if !continues {
            close_header_group(&group, &mut headers);
            group.clear();
            depth = 0;
        }
        depth += paren_delta(&code);
        group.push(line);
    }
    close_header_group(&group, &mut headers);
    headers
}

fn close_header_group(group: &[&ChangedLine], headers: &mut BTreeMap<usize, usize>) {
    let Some(first) = group.first() else {
        return;
    };
    let joined = group
        .iter()
        .map(|line| code_only(&line.text).trim().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    if opens_an_owner(&joined) {
        for line in group {
            headers.insert(line.line, first.line);
        }
    }
}

/// Where the walk back from a deleted line ended.
enum Enclosing {
    /// A kept line: the enclosing owner is live and spans the line.
    Live,
    /// A removed header no added header replaced: the owner went with it.
    Lost,
    /// A removed header an added header replaced: the owner lives on and
    /// starts where the replacement does.
    ReHeaded,
}

/// The live owner that held the deleted line. The diff parser records a
/// removed line at the new-side line that follows it, and a body's closing
/// brace survives a deleted statement, so the narrowest owner whose header
/// sits above that line and whose span reaches it held the deleted line. A
/// re-headed owner's replacement header sits at that line itself. A line
/// whose own function or method was deleted with it belonged to an owner
/// that no longer exists, and resolves to none.
fn deleted_line_owner<'a>(
    changed: &ChangedFile,
    removed: &ChangedLine,
    headers: &BTreeMap<usize, usize>,
    re_headed: &BTreeSet<usize>,
    owners: &'a [TypeScriptOwner],
) -> Option<&'a TypeScriptOwner> {
    let header_may_sit_at_line = match enclosing(changed, removed, headers, re_headed) {
        Enclosing::Lost => return None,
        Enclosing::Live => false,
        Enclosing::ReHeaded => true,
    };
    let at = removed.new_side_line;
    let file = normalized_path(&changed.path);
    owners
        .iter()
        .filter(|owner| normalized_path(&owner.file) == file)
        .filter(|owner| {
            (owner.start_line < at || (header_may_sit_at_line && owner.start_line == at))
                && at <= owner.end_line
        })
        .min_by_key(|owner| owner.end_line - owner.start_line)
}

/// Whether the function, method or class that enclosed `removed` was
/// deleted with it. The walk goes back through the removed lines in
/// old-file order while they stay contiguous there, counting braces: the
/// first line that opens a block around the deleted line is its enclosing
/// header. An `if`, loop or callback header leads on outward; an owner
/// header ends the walk. A kept line breaks contiguity, and its owner is
/// live; so is a removed header an added header pairs with, as when only
/// its types changed.
fn enclosing(
    changed: &ChangedFile,
    removed: &ChangedLine,
    headers: &BTreeMap<usize, usize>,
    re_headed: &BTreeSet<usize>,
) -> Enclosing {
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
            return Enclosing::Live;
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
        if let Some(header) = headers.get(&previous.line) {
            return if re_headed.contains(header) {
                Enclosing::ReHeaded
            } else {
                Enclosing::Lost
            };
        }
        depth = 0;
    }
}

/// The line with string literal contents and any line comment removed, so
/// brackets and arrows inside them are not read as code. Quotes stay.
fn code_only(text: &str) -> String {
    let mut code = String::with_capacity(text.len());
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match quote {
            Some(open) => {
                if ch == '\\' {
                    chars.next();
                } else if ch == open {
                    quote = None;
                    code.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' | '`' => {
                    quote = Some(ch);
                    code.push(ch);
                }
                '/' if chars.peek() == Some(&'/') => break,
                _ => code.push(ch),
            },
        }
    }
    code
}

/// `{` and `}` outside string literals and line comments.
fn brace_counts(text: &str) -> (usize, usize) {
    let code = code_only(text);
    (code.matches('{').count(), code.matches('}').count())
}

/// Opened minus closed parentheses in code.
fn paren_delta(code: &str) -> isize {
    code.chars().fold(0, |depth, ch| match ch {
        '(' => depth + 1,
        ')' => depth - 1,
        _ => depth,
    })
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

/// Length of the identifier that opens `text`.
fn identifier_len(text: &str) -> usize {
    text.find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '#'))
        .unwrap_or(text.len())
}

/// Whether a line opens a function, method, class or named arrow function:
/// the shapes the owner extractor indexes. A callback passed to a call
/// (`items.forEach((item) => {`, `setState({`), an assignment whose value
/// only contains a callback (`total = items.reduce((s, i) => s + i, 0);`)
/// or an `if` block is not an owner.
pub(super) fn opens_an_owner(text: &str) -> bool {
    let code = code_only(text);
    let mut rest = code.trim();
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
    if binds_a_function(rest) {
        return true;
    }
    // A method header: a name, its parameters (which may hold a function
    // type) and a body brace outside them.
    let rest = rest.strip_prefix('*').unwrap_or(rest).trim_start();
    let name_end = identifier_len(rest);
    let name = &rest[..name_end];
    let after_name = rest[name_end..].trim_start();
    name_end > 0
        && !CONTROL_KEYWORDS.contains(&name)
        && (after_name.starts_with('(') || after_name.starts_with('<'))
        && rest.trim_end().ends_with('{')
        && paren_delta(rest) == 0
}

/// `name = (…) =>`, `const name = async x =>`, `name: (…) => {`,
/// `const name: Type = (…) =>` or `name: function (…) {`: a binding whose
/// value is itself a function, not one whose value only contains one.
fn binds_a_function(rest: &str) -> bool {
    let binding = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| rest.strip_prefix(keyword))
        .unwrap_or(rest);
    let name_end = identifier_len(binding);
    if name_end == 0 {
        return false;
    }
    let after_name = binding[name_end..].trim_start();
    let after_name = after_name
        .strip_prefix(['?', '!'])
        .unwrap_or(after_name)
        .trim_start();
    if let Some(value) = after_name.strip_prefix('=') {
        return !value.starts_with(['=', '>']) && starts_a_function(value);
    }
    let Some(value) = after_name.strip_prefix(':') else {
        return false;
    };
    // A typed binding's value follows its first plain `=`; with none, the
    // text after `:` is a property's value.
    match plain_assignment(value) {
        Some(at) => starts_a_function(&value[at + 1..]),
        None => starts_a_function(value),
    }
}

/// An arrow function or a `function` expression.
fn starts_a_function(text: &str) -> bool {
    let text = text.trim_start();
    let text = text
        .strip_prefix("async ")
        .map(str::trim_start)
        .unwrap_or(text);
    let function_keyword = text
        .strip_prefix("function")
        .is_some_and(|after| after.starts_with([' ', '*', '(']));
    function_keyword || starts_an_arrow(text)
}

/// The first `=` outside brackets that is not part of `=>`, `==`, `<=`,
/// `>=` or `!=`: a parameter default (`(opts = {}) =>`) is not the binding's
/// value.
fn plain_assignment(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth: usize = 0;
    (0..bytes.len()).find(|&at| {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        depth == 0
            && bytes[at] == b'='
            && !matches!(bytes.get(at + 1), Some(b'=' | b'>'))
            && !(at > 0 && matches!(bytes[at - 1], b'=' | b'!' | b'<' | b'>'))
    })
}

/// Whether `text` begins with an arrow function: an optional `async`, an
/// optional type parameter list, then a parenthesised parameter list (with
/// an optional return type) or a lone parameter, then `=>`.
fn starts_an_arrow(text: &str) -> bool {
    let text = text.trim_start();
    let text = text
        .strip_prefix("async")
        .filter(|after| after.starts_with([' ', '(']))
        .unwrap_or(text)
        .trim_start();
    let text = match text.strip_prefix('<') {
        Some(after) => match closing_angle(after) {
            Some(close) => after[close + 1..].trim_start(),
            None => return false,
        },
        None => text,
    };
    if text.starts_with('(') {
        let mut depth: usize = 0;
        for (at, ch) in text.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let after = text[at + 1..].trim_start();
                        return after.starts_with("=>")
                            || (after.starts_with(':') && after.contains("=>"));
                    }
                }
                _ => {}
            }
        }
        return false;
    }
    let name_end = identifier_len(text);
    name_end > 0 && text[name_end..].trim_start().starts_with("=>")
}

/// The index of the `>` that closes a type parameter list whose `<` came
/// just before `text`, counting nested `<…>` and passing over `=>`.
fn closing_angle(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth: usize = 1;
    for (at, &byte) in bytes.iter().enumerate() {
        match byte {
            b'<' => depth += 1,
            b'>' if at > 0 && bytes[at - 1] == b'=' => {}
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}
