//! Current-source identity for closing-only `?` diff lines (#1715).
//! This supplies no activation, flow, assertion or exposure authority.

use crate::analysis::diff::ChangedLine;
use crate::analysis::rust_index::{RustIndex, find_file_facts, find_owner_function};
use crate::analysis::syntax::{parse_clean_source_file, ra::LineIndex};
use crate::domain::SymbolId;
use ra_ap_syntax::{AstNode, ast};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) struct PropagationSubject {
    pub expression: String,
    pub line: usize,
    pub column: usize,
    pub start_byte: usize,
    pub owner: SymbolId,
}

/// Parse once per changed file, only when a whole closing line asks for a
/// propagation subject. Mixed sibling code, stale text, parse fallback and
/// nested return targets retain their existing evidence.
pub(super) fn closing_propagation_subjects(
    index: &RustIndex,
    file: &Path,
    changed: &[ChangedLine],
) -> BTreeMap<usize, PropagationSubject> {
    current_subjects(index, file, changed).unwrap_or_default()
}

fn current_subjects(
    index: &RustIndex,
    file: &Path,
    changed: &[ChangedLine],
) -> Option<BTreeMap<usize, PropagationSubject>> {
    let wanted: BTreeMap<_, _> = changed
        .iter()
        .filter_map(|line| {
            let text = line.text.trim();
            (text.bytes().filter(|byte| *byte == b'?').count() == 1
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_whitespace() || b")]}?;".contains(&byte)))
            .then_some((line.new_side_line, text))
        })
        .collect();
    if wanted.is_empty() {
        return None;
    }
    let facts = find_file_facts(index, file)?;
    if facts.used_lexical_fallback {
        return None;
    }
    let source = facts.source.as_ref();
    let parse = parse_clean_source_file(source)?;
    let line_index = LineIndex::new(source);
    let mut offset = 0;
    let lines: Vec<_> = source
        .split_inclusive('\n')
        .map(|text| {
            let start = offset;
            offset += text.len();
            (start, text)
        })
        .collect();
    let mut selected = BTreeMap::<usize, Option<PropagationSubject>>::new();
    for expression in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::TryExpr::cast)
    {
        let question = expression.question_mark_token()?;
        let line = line_index.line(question.text_range().start());
        let Some(claimed) = wanted.get(&line) else {
            continue;
        };
        let (line_start, current) = lines.get(line.checked_sub(1)?)?;
        if current.trim() != *claimed {
            continue;
        }
        let range = expression.syntax().text_range();
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        // The expression owns every non-whitespace byte on the closing line,
        // except its statement terminator. Its opening is on an earlier line.
        if start >= *line_start
            || source.get(*line_start..end)?.trim() != claimed.trim_end_matches(';').trim_end()
        {
            continue;
        }
        let Some(owner) = find_owner_function(index, file, line) else {
            continue;
        };
        let mut direct_owner = false;
        for ancestor in expression.syntax().ancestors().skip(1) {
            if let Some(function) = ast::Fn::cast(ancestor.clone()) {
                let start = usize::from(function.fn_token()?.text_range().start());
                let end = usize::from(function.syntax().text_range().end());
                direct_owner = source.get(start..end) == Some(owner.body.as_str())
                    && !function
                        .syntax()
                        .ancestors()
                        .skip(1)
                        .any(|node| ast::Fn::can_cast(node.kind()));
                break;
            }
            if ast::ClosureExpr::can_cast(ancestor.kind())
                || ast::BlockExpr::cast(ancestor).is_some_and(|block| {
                    block.async_token().is_some()
                        || block.const_token().is_some()
                        || block.try_block_modifier().is_some()
                        || block.gen_token().is_some()
                })
            {
                break;
            }
        }
        if !direct_owner {
            continue;
        }
        let first_line = line_index.line(range.start());
        let (opening, _) = lines.get(first_line.checked_sub(1)?)?;
        let subject = PropagationSubject {
            expression: source.get(start..end)?.to_string(),
            line: first_line,
            column: source.get(*opening..start)?.chars().count() + 1,
            start_byte: start,
            owner: owner.id.clone(),
        };
        // Ambiguity never selects the first candidate by convenience.
        selected
            .entry(line)
            .and_modify(|slot| *slot = None)
            .or_insert(Some(subject));
    }
    Some(
        selected
            .into_iter()
            .filter_map(|(line, subject)| subject.map(|subject| (line, subject)))
            .collect(),
    )
}
