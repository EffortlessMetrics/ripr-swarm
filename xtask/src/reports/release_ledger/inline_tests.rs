//! Inline test edits for the direct-main ledger (#2768): a path under
//! `crates/ripr/src` whose every changed hunk lies inside a `#[cfg(test)]` item
//! is test evidence, not product. Path classes alone cannot see this, so capture
//! reads each such commit's zero-context diff and the `#[cfg(test)]` item spans
//! of both file revisions. Anything it cannot place inside a span stays product.

use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::capture::{GIT_TIMEOUT, git_bytes};
use super::rule::{PathClass, classify_path};

/// Inclusive 1-based line range.
type Span = (u64, u64);

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Hunk {
    pub(super) old_start: u64,
    pub(super) old_count: u64,
    pub(super) new_start: u64,
    pub(super) new_count: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct FileDiff {
    pub(super) path: String,
    /// Added or deleted whole files are never inline test edits.
    pub(super) whole_file: bool,
    pub(super) hunks: Vec<Hunk>,
}

/// A product-class `.rs` file that could hold inline tests.
pub(super) fn is_candidate(path: &str) -> bool {
    path.ends_with(".rs") && classify_path(path) == PathClass::Product
}

/// Source with comments and string/char literal contents replaced by spaces,
/// newlines kept, so brace matching and attribute search ignore them.
fn blank_non_code(source: &str) -> Vec<char> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = chars.clone();
    let blank = |out: &mut Vec<char>, from: usize, to: usize| {
        for slot in out.iter_mut().take(to).skip(from) {
            if *slot != '\n' {
                *slot = ' ';
            }
        }
    };
    let mut index = 0;
    while index < chars.len() {
        let here = chars[index];
        let next = chars.get(index + 1).copied();
        if here == '/' && next == Some('/') {
            let end = chars[index..]
                .iter()
                .position(|c| *c == '\n')
                .map_or(chars.len(), |offset| index + offset);
            blank(&mut out, index, end);
            index = end;
        } else if here == '/' && next == Some('*') {
            let mut depth = 1;
            let mut end = index + 2;
            while end < chars.len() && depth > 0 {
                if chars[end] == '/' && chars.get(end + 1) == Some(&'*') {
                    depth += 1;
                    end += 2;
                } else if chars[end] == '*' && chars.get(end + 1) == Some(&'/') {
                    depth -= 1;
                    end += 2;
                } else {
                    end += 1;
                }
            }
            blank(&mut out, index, end);
            index = end;
        } else if let Some(end) = raw_string_end(&chars, index) {
            blank(&mut out, index, end);
            index = end;
        } else if here == '"' {
            let mut end = index + 1;
            while end < chars.len() && chars[end] != '"' {
                end += if chars[end] == '\\' { 2 } else { 1 };
            }
            let end = (end + 1).min(chars.len());
            blank(&mut out, index, end);
            index = end;
        } else if here == '\'' {
            // A char literal closes within a few characters; a lifetime does not.
            let end = if next == Some('\\') {
                chars[index + 2..]
                    .iter()
                    .take(10)
                    .position(|c| *c == '\'')
                    .map(|offset| index + 2 + offset + 1)
            } else if chars.get(index + 2) == Some(&'\'') {
                Some(index + 3)
            } else {
                None
            };
            if let Some(end) = end {
                blank(&mut out, index, end);
                index = end;
            } else {
                index += 1;
            }
        } else {
            index += 1;
        }
    }
    out
}

/// End (exclusive) of a raw string starting at `index` (`r"`, `r#"`, `br#"`).
fn raw_string_end(chars: &[char], index: usize) -> Option<usize> {
    let mut cursor = index;
    if chars.get(cursor) == Some(&'b') {
        cursor += 1;
    }
    if chars.get(cursor) != Some(&'r') {
        return None;
    }
    // Not a raw string when `r` ends an identifier.
    if index > 0 && (chars[index - 1].is_alphanumeric() || chars[index - 1] == '_') {
        return None;
    }
    cursor += 1;
    let hashes = chars[cursor..].iter().take_while(|c| **c == '#').count();
    cursor += hashes;
    if chars.get(cursor) != Some(&'"') {
        return None;
    }
    cursor += 1;
    let closing: Vec<char> = std::iter::once('"')
        .chain(std::iter::repeat_n('#', hashes))
        .collect();
    while cursor + closing.len() <= chars.len() {
        if chars[cursor..cursor + closing.len()] == closing[..] {
            return Some(cursor + closing.len());
        }
        cursor += 1;
    }
    Some(chars.len())
}

/// Line spans of every `#[cfg(test)]` item: from the attribute line to the
/// item's closing brace or semicolon. An item that does not close is skipped, so
/// a lexing miss can only keep a path product, never make one evidence.
pub(super) fn test_spans(source: &str) -> Vec<Span> {
    let code = blank_non_code(source);
    let mut line_of = Vec::with_capacity(code.len());
    let mut line = 1u64;
    for ch in &code {
        line_of.push(line);
        if *ch == '\n' {
            line += 1;
        }
    }
    let mut spans = Vec::new();
    let mut index = 0;
    while index < code.len() {
        if code[index] == '#' && code.get(index + 1) == Some(&'[') {
            let Some(close) = bracket_end(&code, index + 1) else {
                break;
            };
            let inner: String = code[index + 2..close]
                .iter()
                .filter(|c| !c.is_whitespace())
                .collect();
            if inner == "cfg(test)" {
                let mut cursor = close + 1;
                // Further attributes belong to the same item.
                loop {
                    while code.get(cursor).is_some_and(|c| c.is_whitespace()) {
                        cursor += 1;
                    }
                    if code.get(cursor) == Some(&'#') && code.get(cursor + 1) == Some(&'[') {
                        match bracket_end(&code, cursor + 1) {
                            Some(end) => cursor = end + 1,
                            None => return spans,
                        }
                    } else {
                        break;
                    }
                }
                if let Some(end) = item_end(&code, cursor) {
                    // Hunks are whole lines, so a span that shares a boundary
                    // line with other code cannot prove an edit there is a test edit.
                    if alone_on_line(&code, index, end) {
                        spans.push((line_of[index], line_of[end]));
                    }
                    index = end + 1;
                    continue;
                }
            }
            index = close + 1;
        } else {
            index += 1;
        }
    }
    spans
}

/// Whether only whitespace (or blanked comments) precedes `start` on its line and
/// follows `end` on its line.
fn alone_on_line(code: &[char], start: usize, end: usize) -> bool {
    let before = code[..start]
        .iter()
        .rev()
        .take_while(|c| **c != '\n')
        .all(|c| c.is_whitespace());
    let after = code[end + 1..]
        .iter()
        .take_while(|c| **c != '\n')
        .all(|c| c.is_whitespace());
    before && after
}

/// Index of the `]` closing the `[` at `open`.
fn bracket_end(code: &[char], open: usize) -> Option<usize> {
    let mut depth = 0i64;
    for (offset, ch) in code[open..].iter().enumerate() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Index of the last character of the item starting at `start`.
fn item_end(code: &[char], start: usize) -> Option<usize> {
    let mut nesting = 0i64;
    let mut index = start;
    while index < code.len() {
        match code[index] {
            '(' | '[' => nesting += 1,
            ')' | ']' => nesting -= 1,
            ';' if nesting == 0 => return Some(index),
            '{' if nesting == 0 => {
                let mut depth = 0i64;
                for (offset, ch) in code[index..].iter().enumerate() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(index + offset);
                            }
                        }
                        _ => {}
                    }
                }
                return None;
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Parses `git diff -U0 --no-renames` output. A path git quotes is skipped, so
/// it can never be reported as an inline test edit.
pub(super) fn parse_diff(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut old_path: Option<String> = None;
    let mut current: Option<usize> = None;
    for line in text.lines() {
        if line.starts_with("diff --git ") {
            old_path = None;
            current = None;
        } else if let Some(rest) = line.strip_prefix("--- ") {
            old_path = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            let old = old_path.take().unwrap_or_default();
            let whole_file = old == "/dev/null" || rest == "/dev/null";
            let path = if rest == "/dev/null" {
                old.strip_prefix("a/")
            } else {
                rest.strip_prefix("b/")
            };
            current = path.filter(|path| !path.starts_with('"')).map(|path| {
                files.push(FileDiff {
                    path: path.to_string(),
                    whole_file,
                    hunks: Vec::new(),
                });
                files.len() - 1
            });
        } else if let (Some(rest), Some(index)) = (line.strip_prefix("@@ "), current)
            && let Some(hunk) = parse_hunk(rest)
        {
            files[index].hunks.push(hunk);
        }
    }
    files
}

fn parse_hunk(header: &str) -> Option<Hunk> {
    let mut parts = header.split(' ');
    let (old_start, old_count) = parse_range(parts.next()?.strip_prefix('-')?)?;
    let (new_start, new_count) = parse_range(parts.next()?.strip_prefix('+')?)?;
    Some(Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
    })
}

fn parse_range(text: &str) -> Option<(u64, u64)> {
    match text.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((text.parse().ok()?, 1)),
    }
}

/// Whether the hunk sits inside one span without touching its attribute or
/// closing line, or is exactly that whole span. Touching a boundary line alone
/// changes whether the item is compiled for tests, which is a product change.
fn within(spans: &[Span], start: u64, count: u64) -> bool {
    let end = start + count - 1;
    spans.iter().any(|(from, to)| {
        let whole = start == *from && end == *to;
        let interior = *from < start && end < *to;
        whole || interior
    })
}

/// True when the file changed and every hunk lies inside a `#[cfg(test)]` item
/// on each side it has lines on.
pub(super) fn is_inline_test_edit(file: &FileDiff, old_spans: &[Span], new_spans: &[Span]) -> bool {
    !file.whole_file
        && !file.hunks.is_empty()
        && file.hunks.iter().all(|hunk| {
            (hunk.old_count == 0 || within(old_spans, hunk.old_start, hunk.old_count))
                && (hunk.new_count == 0 || within(new_spans, hunk.new_start, hunk.new_count))
        })
}

fn spans_at(repo: &Path, rev: &str, path: &str) -> Result<Vec<Span>, String> {
    let bytes = git_bytes(repo, &["show", &format!("{rev}:{path}")], GIT_TIMEOUT)?;
    Ok(test_spans(&String::from_utf8_lossy(&bytes)))
}

fn commit_inline_paths(repo: &Path, parent: &str, sha: &str) -> Result<Vec<String>, String> {
    let bytes = git_bytes(
        repo,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "-U0",
            "--no-renames",
            "--no-ext-diff",
            "--no-color",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            parent,
            sha,
            "--",
            "crates/ripr/src",
            "editors",
            "schemas",
        ],
        GIT_TIMEOUT,
    )?;
    let text = String::from_utf8_lossy(&bytes);
    let mut inline = Vec::new();
    for file in parse_diff(&text) {
        if !is_candidate(&file.path) || file.whole_file || file.hunks.is_empty() {
            continue;
        }
        let old_spans = if file.hunks.iter().any(|hunk| hunk.old_count > 0) {
            spans_at(repo, parent, &file.path)?
        } else {
            Vec::new()
        };
        let new_spans = if file.hunks.iter().any(|hunk| hunk.new_count > 0) {
            spans_at(repo, sha, &file.path)?
        } else {
            Vec::new()
        };
        if is_inline_test_edit(&file, &old_spans, &new_spans) {
            inline.push(file.path);
        }
    }
    inline.sort();
    inline.dedup();
    Ok(inline)
}

/// Candidate paths each commit edits only inside `#[cfg(test)]` items, keyed by
/// commit. Commits with no candidate path, no parent or no such edit are absent.
pub(super) fn inline_test_paths(
    repo: &Path,
    commits: &[(String, Option<String>)],
    touched: &BTreeMap<String, BTreeSet<String>>,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    let work: Vec<(&String, &String)> = commits
        .iter()
        .filter_map(|(sha, parent)| Some((sha, parent.as_ref()?)))
        .filter(|(sha, _)| {
            touched
                .get(*sha)
                .is_some_and(|paths| paths.iter().any(|path| is_candidate(path)))
        })
        .collect();
    let results: Vec<Result<(String, Vec<String>), String>> = work
        .par_iter()
        .map(|(sha, parent)| {
            commit_inline_paths(repo, parent, sha)
                .map(|paths| ((*sha).clone(), paths))
                .map_err(|error| format!("inline test scan failed for {sha}: {error}"))
        })
        .collect();
    let mut found = BTreeMap::new();
    for result in results {
        let (sha, paths) = result?;
        if !paths.is_empty() {
            found.insert(sha, paths);
        }
    }
    Ok(found)
}
