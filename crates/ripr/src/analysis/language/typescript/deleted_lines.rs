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
/// (`ChangedFile::aligned_counterparts`); `new_source` is the candidate file
/// when it was read; `findings` holds the findings so far so a repeated
/// probe id gets an ordinal suffix. `classify` classifies a line at a
/// resolved owner. The work is linear in the diff: each removed run is read
/// once, in old-file order.
pub(super) fn deleted_line_findings(
    changed: &ChangedFile,
    counterparts: &BTreeMap<usize, &ChangedLine>,
    new_source: Option<&str>,
    owners: &[TypeScriptOwner],
    findings: &[Finding],
    classify: impl Fn(usize, &str, &TypeScriptOwner) -> Option<Finding>,
) -> Vec<Finding> {
    let paired: BTreeSet<usize> = counterparts.values().map(|removed| removed.line).collect();
    let runs = removed_runs(changed, new_source);
    let headers = removed_owner_headers(&runs);
    let added_text: BTreeMap<usize, &str> = changed
        .added_lines
        .iter()
        .map(|added| (added.line, added.text.as_str()))
        .collect();
    // Removed headers an added header of the same name replaced: the owner
    // they open lives on. A header replaced by a differently named one is a
    // different owner, and its deleted body is not the new owner's. Added
    // headers use the same wrap grouping as removed ones, so a Prettier
    // split (`save(` / `id: string,` / `) {`) still matches (#7705).
    let added_headers = added_owner_headers(changed);
    let re_headed: BTreeSet<usize> = counterparts
        .iter()
        .filter_map(|(added_line, removed)| {
            let group = headers.group_of.get(&removed.line)?;
            let removed_header = headers.text.get(group).map(String::as_str)?;
            let added = if let Some(added_group) = added_headers.group_of.get(added_line) {
                added_headers.text.get(added_group)?.clone()
            } else {
                code_only(added_text.get(added_line)?)
            };
            same_live_owner(&added, removed_header).then_some(*group)
        })
        .collect();
    let enclosures = enclosures(&runs, &headers, &re_headed);
    let added_texts: BTreeSet<&str> = changed
        .added_lines
        .iter()
        .map(|added| added.text.trim())
        .collect();
    let code_by_line: BTreeMap<usize, &str> = runs
        .iter()
        .flatten()
        .map(|removed| (removed.line.line, removed.code.as_str()))
        .collect();
    let file = normalized_path(&changed.path);
    let file_owners: Vec<&TypeScriptOwner> = owners
        .iter()
        .filter(|owner| normalized_path(&owner.file) == file)
        .collect();
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    for finding in findings {
        *ids.entry(finding.probe.id.0.clone()).or_default() += 1;
    }
    let mut out = Vec::new();
    for removed in &changed.removed_lines {
        let code = code_by_line.get(&removed.line).copied().unwrap_or_default();
        if paired.contains(&removed.line)
            || headers.group_of.contains_key(&removed.line)
            || !is_removed_behavior_line(code)
        {
            continue;
        }
        let enclosing = enclosures
            .get(&removed.line)
            .copied()
            .unwrap_or(Enclosing::Live);
        let Some(owner) = deleted_line_owner(removed, enclosing, &file_owners) else {
            continue;
        };
        let Some(mut finding) = classify(removed.new_side_line, &removed.text, owner) else {
            continue;
        };
        // Same-text evidence is the only movement evidence; without it the
        // record stays `base_deleted` (RIPR-SPEC-0156).
        finding.source_currentness = if added_texts.contains(removed.text.trim()) {
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

/// A removed line's code: the line with comments and string contents
/// removed, read with the block-comment state the lines before it left.
struct RemovedCode<'a> {
    line: &'a ChangedLine,
    code: String,
}

/// The removed lines as contiguous old-side runs, in old-file order. A run
/// starts inside a block comment when the candidate file has one open where
/// the run was removed: a line deleted from a comment's body is not code.
fn removed_runs<'a>(
    changed: &'a ChangedFile,
    new_source: Option<&str>,
) -> Vec<Vec<RemovedCode<'a>>> {
    let comment_open = new_source
        .map(comment_open_at_line_start)
        .unwrap_or_default();
    let mut by_old_line: Vec<&ChangedLine> = changed.removed_lines.iter().collect();
    by_old_line.sort_by_key(|line| line.line);
    let mut runs: Vec<Vec<RemovedCode<'a>>> = Vec::new();
    let mut in_comment = false;
    for line in by_old_line {
        let continues = runs
            .last()
            .and_then(|run| run.last())
            .is_some_and(|last| last.line.line + 1 == line.line);
        if !continues {
            in_comment = line
                .new_side_line
                .checked_sub(1)
                .and_then(|index| comment_open.get(index))
                .copied()
                .unwrap_or(false);
            runs.push(Vec::new());
        }
        let code = code_only_from(&line.text, &mut in_comment);
        if let Some(run) = runs.last_mut() {
            run.push(RemovedCode { line, code });
        }
    }
    runs
}

/// For each candidate line, whether a block comment is open where it starts.
fn comment_open_at_line_start(source: &str) -> Vec<bool> {
    let mut in_comment = false;
    source
        .lines()
        .map(|line| {
            let open = in_comment;
            code_only_from(line, &mut in_comment);
            open
        })
        .collect()
}

/// A removed line that ran behavior, judged on its code: not a comment,
/// blank line or lone bracket, not an import, re-export or type-only
/// declaration, and not a line that only closes a block (`} else {`). Owner
/// headers are excluded by the caller: a removed `function` or method header
/// deletes or re-shapes an owner, which this record does not describe.
fn is_removed_behavior_line(code: &str) -> bool {
    let trimmed = code.trim_start();
    !should_ignore_typescript_changed_line(code)
        && !trimmed.starts_with('}')
        && !trimmed.starts_with("import ")
        && !trimmed.starts_with("export {")
        && !trimmed.starts_with("export *")
        && !is_type_only_declaration(trimmed)
}

/// `type Id = string;`, `interface Shape {`, `declare const x: number;`:
/// erased at compile time, so deleting one changes no behavior.
fn is_type_only_declaration(trimmed: &str) -> bool {
    let rest = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    // `declare` and `interface` lead a declaration only when a name follows,
    // not in `declare = true;`.
    let names_follow = |keyword: &str| {
        rest.strip_prefix(keyword)
            .is_some_and(|after| identifier_len(after.trim_start()) > 0)
    };
    names_follow("declare ")
        || names_follow("interface ")
        || rest.starts_with("type {")
        || rest.starts_with("type *")
        || rest.strip_prefix("type ").is_some_and(|after| {
            let name_end = identifier_len(after);
            let after_name = after[name_end..].trim_start();
            name_end > 0 && (after_name.starts_with('=') || after_name.starts_with('<'))
        })
}

/// Removed owner headers, grouped across the lines a header spans.
#[derive(Default)]
struct RemovedHeaders {
    /// Each header line's group, named by the group's first old line.
    group_of: BTreeMap<usize, usize>,
    /// Each group's joined code.
    text: BTreeMap<usize, String>,
}

/// Every removed line that is part of an owner header. A header can span
/// lines: a parameter list Prettier wraps (`save(` / `id: string,` /
/// `) {`), a wrapped return type (`): Promise<` / `Result` / `> {`), a
/// destructured first line (`save({` / `id,` / `}) {`), or an Allman brace
/// on its own line. Open parentheses and type-argument lists join until
/// they close; a `{` inside still-open parens is a parameter object, not
/// the body, unless the previous line opened a callback with `=> {`
/// (#7705).
fn removed_owner_headers(runs: &[Vec<RemovedCode<'_>>]) -> RemovedHeaders {
    owner_headers_from_code_runs(
        runs.iter()
            .map(|run| run.iter().map(|line| (line.line.line, line.code.as_str()))),
    )
}

/// Added owner headers grouped the same way as removed ones, so a wrapped
/// replacement still counts as the same live owner (#7705).
fn added_owner_headers(changed: &ChangedFile) -> RemovedHeaders {
    let mut by_line: Vec<&ChangedLine> = changed.added_lines.iter().collect();
    by_line.sort_by_key(|line| line.line);
    let mut runs: Vec<Vec<(usize, String)>> = Vec::new();
    for line in by_line {
        let continues = runs
            .last()
            .and_then(|run| run.last())
            .is_some_and(|(last_line, _)| *last_line + 1 == line.line);
        if !continues {
            runs.push(Vec::new());
        }
        if let Some(run) = runs.last_mut() {
            run.push((line.line, code_only(&line.text)));
        }
    }
    owner_headers_from_code_runs(
        runs.iter()
            .map(|run| run.iter().map(|(line, code)| (*line, code.as_str()))),
    )
}

fn owner_headers_from_code_runs<'a>(
    runs: impl IntoIterator<Item = impl IntoIterator<Item = (usize, &'a str)>>,
) -> RemovedHeaders {
    let mut headers = RemovedHeaders::default();
    for run in runs {
        let mut group: Vec<(usize, &'a str)> = Vec::new();
        let mut paren_depth: isize = 0;
        let mut angle_depth: isize = 0;
        for (line, code) in run {
            let last_opened_callback = group
                .last()
                .is_some_and(|(_, last)| last.trim_end().ends_with("=> {"));
            let continues = !group.is_empty()
                && !last_opened_callback
                && (paren_depth > 0 || angle_depth > 0 || code.trim() == "{");
            if !continues {
                close_header_group(&group, &mut headers);
                group.clear();
                paren_depth = 0;
                angle_depth = 0;
            }
            paren_depth += paren_delta(code);
            angle_depth += type_angle_delta(code);
            if angle_depth < 0 {
                angle_depth = 0;
            }
            group.push((line, code));
        }
        close_header_group(&group, &mut headers);
    }
    headers
}

fn close_header_group(group: &[(usize, &str)], headers: &mut RemovedHeaders) {
    let Some((first, _)) = group.first() else {
        return;
    };
    let joined = group
        .iter()
        .map(|(_, code)| code.trim())
        .collect::<Vec<_>>()
        .join(" ");
    if opens_an_owner(&joined) {
        for (line, _) in group {
            headers.group_of.insert(*line, *first);
        }
        headers.text.insert(*first, joined);
    }
}

/// What enclosed a deleted line in the old file.
#[derive(Clone, Copy)]
enum Enclosing {
    /// A kept line: the enclosing owner is live and spans the line.
    Live,
    /// A removed header no added header replaced: the owner went with it.
    Lost,
    /// A removed header an added header of the same name replaced: the
    /// owner lives on and starts where the replacement does.
    ReHeaded,
    /// The body of a removed type-only declaration (`interface Shape {`):
    /// erased at compile time, so no behavior.
    TypeBody,
}

/// A block a removed line opened.
#[derive(Clone, Copy)]
enum Block {
    /// An owner header's body, named by the header's group.
    Owner(usize),
    /// A type-only declaration's body.
    TypeOnly,
    /// Any other block: `if`, loop, callback, object literal.
    Plain,
}

/// What enclosed each removed line. Each run is read once in old-file order
/// with a stack of the blocks it opened: the innermost open owner header
/// encloses a line, and an `if`, loop or callback block leads on outward. A
/// line no removed owner header encloses sits in a kept block, whose owner
/// is live.
fn enclosures(
    runs: &[Vec<RemovedCode<'_>>],
    headers: &RemovedHeaders,
    re_headed: &BTreeSet<usize>,
) -> BTreeMap<usize, Enclosing> {
    let mut enclosures = BTreeMap::new();
    for run in runs {
        let mut open_blocks: Vec<Block> = Vec::new();
        for line in run {
            let innermost_owner = open_blocks.iter().rev().find_map(|block| match block {
                Block::Owner(group) => Some(*group),
                Block::TypeOnly | Block::Plain => None,
            });
            let enclosing = if open_blocks
                .iter()
                .any(|block| matches!(block, Block::TypeOnly))
            {
                Enclosing::TypeBody
            } else {
                match innermost_owner {
                    Some(group) if re_headed.contains(&group) => Enclosing::ReHeaded,
                    Some(_) => Enclosing::Lost,
                    None => Enclosing::Live,
                }
            };
            enclosures.insert(line.line.line, enclosing);
            let block = match headers.group_of.get(&line.line.line) {
                Some(group) => Block::Owner(*group),
                None if is_type_only_declaration(line.code.trim_start()) => Block::TypeOnly,
                None => Block::Plain,
            };
            for ch in line.code.chars() {
                match ch {
                    '{' => open_blocks.push(block),
                    '}' => {
                        open_blocks.pop();
                    }
                    _ => {}
                }
            }
        }
    }
    enclosures
}

/// The live owner that held the deleted line. The diff parser records a
/// removed line at the new-side line that follows it, and a body's closing
/// brace survives a deleted statement, so the narrowest owner whose header
/// sits above that line and whose span reaches it held the deleted line. A
/// re-headed owner's replacement header sits at that line itself. A line
/// whose own function or method was deleted with it belonged to an owner
/// that no longer exists, and resolves to none.
fn deleted_line_owner<'a>(
    removed: &ChangedLine,
    enclosing: Enclosing,
    file_owners: &[&'a TypeScriptOwner],
) -> Option<&'a TypeScriptOwner> {
    let header_may_sit_at_line = match enclosing {
        Enclosing::Lost | Enclosing::TypeBody => return None,
        Enclosing::Live => false,
        Enclosing::ReHeaded => true,
    };
    let at = removed.new_side_line;
    file_owners
        .iter()
        .copied()
        .filter(|owner| {
            (owner.start_line < at || (header_may_sit_at_line && owner.start_line == at))
                && at <= owner.end_line
        })
        .min_by_key(|owner| owner.end_line - owner.start_line)
}

/// The line with comments and string literal contents removed, so
/// brackets and arrows inside them are not read as code. Quotes stay.
fn code_only(text: &str) -> String {
    code_only_from(text, &mut false)
}

/// `code_only` for a line read inside a run of lines: `in_comment` says
/// whether a block comment is open where the line starts, and is left saying
/// whether one is open where it ends.
fn code_only_from(text: &str, in_comment: &mut bool) -> String {
    let mut code = String::with_capacity(text.len());
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if *in_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                *in_comment = false;
                code.push(' ');
            }
            continue;
        }
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
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    *in_comment = true;
                }
                _ => code.push(ch),
            },
        }
    }
    code
}

/// Opened minus closed parentheses in code.
fn paren_delta(code: &str) -> isize {
    code.chars().fold(0, |depth, ch| match ch {
        '(' => depth + 1,
        ')' => depth - 1,
        _ => depth,
    })
}

/// Opened minus closed type-argument angles on this line. Comparisons
/// (`a < b`) and arrows (`=>`, `>=`, `<=`) are not type lists: a type
/// opener sits immediately after an identifier, `)`, `]` or `>`. `>=`
/// does not emit a close. A later `> {` may report -1 so it can close
/// `Promise<` from the previous line; the grouping accumulator never
/// lets angle depth go below zero, so `a > b` cannot cancel an open `(`
/// (#7705).
fn type_angle_delta(code: &str) -> isize {
    let bytes = code.as_bytes();
    let mut depth: isize = 0;
    for at in 0..bytes.len() {
        match bytes[at] {
            b'<' if bytes.get(at + 1) != Some(&b'=') && is_type_open_angle(bytes, at) => {
                depth += 1;
            }
            b'>' if bytes.get(at + 1) == Some(&b'=') || (at > 0 && bytes[at - 1] == b'=') => {}
            b'>' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn is_type_open_angle(bytes: &[u8], at: usize) -> bool {
    let Some(prev) = at.checked_sub(1).and_then(|i| bytes.get(i).copied()) else {
        return false;
    };
    prev.is_ascii_alphanumeric() || matches!(prev, b'_' | b'$' | b')' | b']' | b'>')
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
    let trimmed = code.trim();
    // `export default (x) => {` has no binding name; after modifiers it is
    // only an arrow, which `binds_a_function` would refuse (#7705).
    let export_default = trimmed.starts_with("export default");
    let rest = strip_leading_modifiers(trimmed);
    let function_keyword = rest
        .strip_prefix("function")
        .is_some_and(|after| after.starts_with([' ', '*', '(']));
    if function_keyword || rest.starts_with("class ") {
        return true;
    }
    if binds_a_function(rest) {
        return true;
    }
    if export_default && starts_an_arrow(rest) {
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
/// value. Angle brackets count so a type-parameter default (`<T = string>`)
/// is not the binding's value either (#7705).
fn plain_assignment(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth: usize = 0;
    (0..bytes.len()).find(|&at| {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            // `<=` is not a type-parameter list.
            b'<' if bytes.get(at + 1) != Some(&b'=') => depth += 1,
            // `=>` and `>=` are not a list close.
            b'>' if at > 0 && bytes[at - 1] == b'=' => {}
            b'>' => depth = depth.saturating_sub(1),
            _ => {}
        }
        depth == 0
            && bytes[at] == b'='
            && !matches!(bytes.get(at + 1), Some(&b'=' | &b'>'))
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

/// The name an owner header declares: `function name`, `class Name`, a
/// method's name or a binding's name. Anonymous default exports
/// (`export default function (`, `export default class {`,
/// `export default (x) => {`) have none (#7705).
fn owner_name(code: &str) -> Option<String> {
    let rest = strip_leading_modifiers(code.trim());
    let rest = ["function", "class ", "const ", "let ", "var "]
        .iter()
        .find_map(|keyword| rest.strip_prefix(keyword))
        .unwrap_or(rest)
        .trim_start();
    let rest = rest.strip_prefix('*').unwrap_or(rest).trim_start();
    let name_end = identifier_len(rest);
    (name_end > 0).then(|| rest[..name_end].to_string())
}

/// An added header keeps the owner a removed header opened when they name
/// the same function, method or binding, or when both are anonymous
/// (`export default function (` replaced by another nameless default).
fn same_live_owner(added: &str, removed: &str) -> bool {
    if !opens_an_owner(added) {
        return false;
    }
    match (owner_name(added), owner_name(removed)) {
        (Some(added_name), Some(removed_name)) => added_name == removed_name,
        (None, None) => true,
        _ => false,
    }
}

/// Keywords and `get`/`set` accessors that may precede an owner header.
/// `get =` / `set =` is a field named `get`/`set`, not an accessor (#7705).
fn strip_leading_modifiers(text: &str) -> &str {
    let mut rest = text;
    'strip: loop {
        for modifier in MODIFIERS {
            if let Some(after) = rest.strip_prefix(modifier)
                && after.starts_with(char::is_whitespace)
            {
                let trimmed = after.trim_start();
                if matches!(*modifier, "get" | "set") && trimmed.starts_with('=') {
                    return rest;
                }
                rest = trimmed;
                continue 'strip;
            }
        }
        break;
    }
    rest
}
