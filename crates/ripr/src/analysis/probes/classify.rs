use super::super::rust_index::{ProbeShapeFact, ProbeShapeKind, RustIndex};
use super::family::family_for_probe_shape;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FileData;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter, parse_clean_source_file};
use crate::domain::ProbeFamily;
use ra_ap_syntax::{AstNode, ast};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::path::Path;

mod declarations;

/// Changed lines handed to `declarations::type_bound_lines` between two
/// cancellation checkpoints. A whole-file change (an unsaved buffer) puts
/// thousands of lines in one call, and the verdict work grows faster than
/// the line count, so one call could outlast a `shutdown` by many seconds
/// (#11439). Each verdict depends only on its own line, so the chunking does
/// not change any result.
const TYPE_BOUND_CHUNK_LINES: usize = 256;

fn cancellable_type_bound_lines(
    source: &str,
    lines: &[crate::analysis::diff::ChangedLine],
) -> Vec<bool> {
    verdicts_in_cancellable_chunks(lines, TYPE_BOUND_CHUNK_LINES, |chunk| {
        declarations::type_bound_lines(source, chunk)
    })
}

/// Runs `verdicts` over `lines` in chunks, checking the run's cancellation
/// token before each chunk. After a cancel the remaining lines get `false`
/// without being examined, so the result always has one entry per line. The
/// token is sticky and every caller reaches an unconditional checkpoint
/// before publishing, so these filler verdicts are discarded with the
/// cancelled run and never become a finding.
fn verdicts_in_cancellable_chunks(
    lines: &[crate::analysis::diff::ChangedLine],
    chunk_lines: usize,
    mut verdicts: impl FnMut(&[crate::analysis::diff::ChangedLine]) -> Vec<bool>,
) -> Vec<bool> {
    let mut out = Vec::with_capacity(lines.len());
    for chunk in lines.chunks(chunk_lines.max(1)) {
        if crate::analysis::cancellation::checkpoint().is_err() {
            break;
        }
        out.extend(verdicts(chunk));
    }
    out.resize(lines.len(), false);
    out
}

pub(super) fn added_type_bound_lines(
    index: &RustIndex,
    changed: &crate::analysis::diff::ChangedFile,
) -> Vec<bool> {
    file_facts(index, &changed.path)
        .filter(|facts| !facts.used_lexical_fallback)
        .map(|facts| cancellable_type_bound_lines(&facts.source, &changed.added_lines))
        .unwrap_or_else(|| vec![false; changed.added_lines.len()])
}

/// Added lines that touch a type bound anywhere on the line (see
/// [`declarations::header_bound_lines`]); fails closed to `true` per line when
/// no clean syntax is available.
pub(super) fn added_header_bound_lines(
    index: &RustIndex,
    changed: &crate::analysis::diff::ChangedFile,
) -> Vec<bool> {
    file_facts(index, &changed.path)
        .filter(|facts| !facts.used_lexical_fallback)
        .map(|facts| declarations::header_bound_lines(&facts.source, &changed.added_lines))
        .unwrap_or_else(|| vec![true; changed.added_lines.len()])
}

/// Removed declarations must be admitted using reconstructed old-side syntax,
/// never a line coincidence in the candidate index. A refused reconstruction
/// or parse leaves the existing lexical/unknown evidence intact.
pub(super) struct RemovedTypeBounds {
    pub lines: Vec<bool>,
    /// Removed struct or enum-variant field declarations (#9183), admitted on
    /// the reconstructed old syntax. Only set when `old_index` is available.
    pub field_declarations: Vec<bool>,
    pub old_index: Option<RustIndex>,
}

/// The changed file's old source, rebuilt from the indexed new source and
/// the diff. Rebuilding copies the whole file, so a caller builds it once
/// per file and shares it (#7637). `None` when nothing was removed or the
/// file is not indexed.
pub(super) fn old_source(
    index: &RustIndex,
    changed: &crate::analysis::diff::ChangedFile,
) -> Option<String> {
    if changed.removed_lines.is_empty() {
        return None;
    }
    file_facts(index, &changed.path)
        .and_then(|facts| crate::analysis::diff::reconstruct_old_source(&facts.source, changed))
}

pub(super) fn removed_type_bounds(
    root: &Path,
    registrations: &[crate::config::TestHarnessRegistration],
    old_source: Option<&str>,
    changed: &crate::analysis::diff::ChangedFile,
) -> RemovedTypeBounds {
    let unavailable = || RemovedTypeBounds {
        lines: vec![false; changed.removed_lines.len()],
        field_declarations: vec![false; changed.removed_lines.len()],
        old_index: None,
    };
    if changed.removed_lines.is_empty() {
        return unavailable();
    }
    let Some(source) = old_source else {
        return unavailable();
    };
    let lines = cancellable_type_bound_lines(source, &changed.removed_lines);
    let field_declarations = declarations::field_declaration_lines(source, &changed.removed_lines);
    if !lines
        .iter()
        .chain(&field_declarations)
        .any(|admitted| *admitted)
    {
        return unavailable();
    }
    let Ok(facts) = RaRustSyntaxAdapter.summarize_file(&changed.path, source) else {
        return unavailable();
    };
    let mut old_index = RustIndex::default();
    old_index.insert_file(changed.path.clone(), facts, true);
    if crate::analysis::facts::normalize_reconstructed_source_roles(
        &mut old_index,
        root,
        registrations,
    )
    .is_err()
    {
        return unavailable();
    }
    RemovedTypeBounds {
        lines,
        field_declarations,
        old_index: Some(old_index),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParserProbeShape<'a> {
    pub(crate) family: ProbeFamily,
    pub(crate) start_line: usize,
    pub(crate) start_byte: usize,
    pub(crate) text: &'a str,
    pub(crate) standalone_call: bool,
    pub(crate) unsafe_boundary: bool,
}

pub(crate) fn parser_probe_shapes_for_changed_line<'a>(
    index: &'a RustIndex,
    file: &Path,
    line: usize,
    changed_text: &str,
) -> Vec<ParserProbeShape<'a>> {
    parser_probe_shapes_for_changed_line_against(index, file, line, changed_text, None)
}

/// Like [`parser_probe_shapes_for_changed_line`], with the removed text the
/// diff pairs with this line. When several same-family shapes match the line
/// equally well (`Id { counter: 0x00ab_cdef, version: 0x1 }`), a shape whose
/// text the removed line already holds as a whole token run is unchanged and
/// loses to one it does not hold, so the probe names the edited field rather
/// than its neighbour (#6731). The match category (exact, containing,
/// contained) still decides first, so an exact shape is never traded for an
/// enclosing one.
pub(crate) fn parser_probe_shapes_for_changed_line_against<'a>(
    index: &'a RustIndex,
    file: &Path,
    line: usize,
    changed_text: &str,
    removed_text: Option<&str>,
) -> Vec<ParserProbeShape<'a>> {
    let Some(facts) = file_facts(index, file) else {
        return Vec::new();
    };
    // One window scan of the changed line, keyed by needle length, answers
    // `changed.contains(shape)` for every overlapping shape. Per-shape
    // `str::contains` is quadratic on one long generated line (#8647).
    let ranker = ChangedLineRank::build(
        changed_text,
        facts.probe_shapes.iter().filter_map(|shape| {
            (shape.start_line() <= line && line <= shape.end_line()).then_some(shape.text.as_str())
        }),
    );
    // The unchanged test counts token runs of every overlapping shape in the
    // removed and the changed line. Counting per shape re-masks and re-scans
    // the whole line each time, which is quadratic on one long generated line
    // (#9895), so both lines are scanned once per distinct needle length.
    let unchanged_counts = removed_text.map(|removed| {
        let needles = facts
            .probe_shapes
            .iter()
            .filter(|shape| shape.start_line() <= line && line <= shape.end_line())
            .map(|shape| {
                (
                    shape.text.trim(),
                    family_for_probe_shape(shape.kind) == ProbeFamily::FieldConstruction,
                )
            })
            .collect::<Vec<_>>();
        (
            TokenRunCounts::build(removed, &needles),
            TokenRunCounts::build(changed_text, &needles),
        )
    });
    let selection_key = |family: &ProbeFamily, shape_text: &str| {
        ranker
            .as_ref()
            .and_then(|ranker| ranker.rank(shape_text))
            .map(|(category, distance)| {
                // Unchanged only when the old line holds every occurrence the new
                // line has: with two literals on one line, an edited field whose
                // new text matches the *other* literal's old field is new.
                let needle = shape_text.trim();
                let unchanged = unchanged_counts.as_ref().is_some_and(|(removed, changed)| {
                    // Only a record field is bounded by `,`/`}`; other families
                    // (calls, predicates) sit inside larger expressions by design.
                    let whole_unit = *family == ProbeFamily::FieldConstruction;
                    let before = removed.get(needle, whole_unit);
                    before > 0 && before >= changed.get(needle, whole_unit)
                });
                (category, unchanged, distance)
            })
    };
    let mut selected = Vec::<(ParserProbeShape<'a>, Option<(u8, bool, usize)>)>::new();
    // #14192: parsed at most once per changed line, and only when a
    // multi-line returned-value shape spans it.
    let mut parsed: Option<Option<ra_ap_syntax::Parse<ast::SourceFile>>> = None;
    for shape in &facts.probe_shapes {
        if shape.start_line() > line || line > shape.end_line() {
            continue;
        }
        let family = family_for_probe_shape(shape.kind);
        if matches!(family, ProbeFamily::ReturnValue | ProbeFamily::ErrorPath)
            && shape.start_line() < line
            && line < shape.end_line()
            && parsed
                .get_or_insert_with(|| last_file_parse(&facts.source))
                .as_ref()
                .is_some_and(|parse| line_is_nested_statement(&facts.source, parse, shape, line))
        {
            continue;
        }
        let unsafe_boundary = shape.kind == ProbeShapeKind::UnsafeBoundary;
        if unsafe_boundary && !unsafe_boundary_owns_changed_line(facts, shape, line) {
            continue;
        }
        let candidate_key = selection_key(&family, shape.text.as_str());
        if !unsafe_boundary && candidate_key.is_none() {
            continue;
        }
        let candidate = ParserProbeShape {
            family,
            // Unsafe boundaries match by parser-owned containment rather than
            // source text. Project them at the changed line so diff synthesis
            // emits one explicit canonical probe while `start_byte` keeps the
            // stable boundary identity used for deduplication.
            start_line: if unsafe_boundary {
                line
            } else {
                shape.start_line()
            },
            start_byte: shape.start_byte(),
            text: shape.text.as_str(),
            // Resolved for the winners only: it scans the line prefix, which
            // is linear in the line per shape (#9895).
            standalone_call: true,
            unsafe_boundary,
        };
        if let Some(position) = selected
            .iter()
            .position(|(current, _)| current.family == candidate.family)
        {
            if candidate.unsafe_boundary && selected[position].0.unsafe_boundary {
                if candidate.start_byte > selected[position].0.start_byte {
                    selected[position] = (candidate, candidate_key);
                }
                continue;
            }
            let current_key = selected[position].1;
            if candidate_key < current_key
                || (candidate_key == current_key && candidate.text < selected[position].0.text)
            {
                selected[position] = (candidate, candidate_key);
            }
        } else {
            selected.push((candidate, candidate_key));
        }
    }
    if let Some(removed) = removed_text {
        retarget_inline_branch_value_edit(&facts.source, line, removed, &mut selected, &mut parsed);
    }
    for (shape, _) in &mut selected {
        shape.standalone_call = parser_call_shape_is_standalone(facts, shape);
    }
    if selected.iter().all(|(shape, _)| shape.unsafe_boundary)
        && let Some(declaration) = declarations::declaration_shape(facts, line, changed_text)
    {
        // Keep the enclosing unsafe obligation as well as the exact declaration.
        // They share a family, but their source identities are not interchangeable.
        selected.push((declaration, None));
    }
    selected.sort_by(|(left, _), (right, _)| {
        left.family
            .as_str()
            .cmp(right.family.as_str())
            .then(left.start_byte.cmp(&right.start_byte))
            .then(left.text.cmp(right.text))
    });
    selected.into_iter().map(|(shape, _)| shape).collect()
}

type SelectedShape<'a> = (ParserProbeShape<'a>, Option<(u8, bool, usize)>);

/// One line's text with formatting removed but token spelling kept: string
/// and char literals are copied verbatim, a whitespace run becomes one space
/// only where it separates two word characters (`x as u32`), and is dropped
/// elsewhere. Returns `None` for text this scan does not model (raw strings,
/// unterminated literals, comments), so callers fail closed.
pub(super) fn formatting_free_text(text: &str) -> Option<String> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        if c.is_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }
        if pending_space && out.ends_with(word) && word(c) {
            out.push(' ');
        }
        pending_space = false;
        let next = chars.get(i + 1).copied();
        if c == '/' && matches!(next, Some('/' | '*')) {
            return None;
        }
        if c == 'r' && matches!(next, Some('"' | '#')) && !out.ends_with(word) {
            return None;
        }
        let literal_end = match c {
            '"' => Some(literal_close(&chars, i, '"')?),
            // `'x'` / `'\n'` is a char literal; `'a` alone is a lifetime.
            '\'' if chars.get(i + 2) == Some(&'\'') || next == Some('\\') => {
                Some(literal_close(&chars, i, '\'')?)
            }
            _ => None,
        };
        let end = literal_end.unwrap_or(i);
        out.extend(&chars[i..=end]);
        i = end + 1;
    }
    Some(out)
}

/// Index of the unescaped `quote` closing the literal opened at `open`.
fn literal_close(chars: &[char], open: usize, quote: char) -> Option<usize> {
    let mut i = open + 1;
    while let Some(&c) = chars.get(i) {
        match c {
            '\\' => i += 2,
            c if c == quote => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// F1: an edit on an `if`'s condition line that misses the condition and
/// lies inside a branch block (`if price > 100 { 10 } else { 0 }` to
/// `{ 15 }`) did not change the predicate, so the predicate's boundary is not
/// the discriminator for it: a `price == 100` test passes on both sides.
/// When the edit is inside a value the owner returns, the unchanged
/// predicate shape leaves the line and that value is the line's
/// `return_value` shape, as it is when it sits on its own line (#14385).
/// Otherwise another selected shape that spans the edit (`return 15`) stands
/// for it; with none, the line keeps its predicate shape (a non-returned value
/// has no shape of its own to take its place).
fn retarget_inline_branch_value_edit<'a>(
    source: &'a str,
    line: usize,
    removed: &str,
    selected: &mut Vec<SelectedShape<'a>>,
    parsed: &mut Option<Option<ra_ap_syntax::Parse<ast::SourceFile>>>,
) {
    let Some(position) = selected
        .iter()
        .position(|(shape, _)| shape.family == ProbeFamily::Predicate && !shape.unsafe_boundary)
    else {
        return;
    };
    let Some(line_range) = source_line_byte_range(source, line) else {
        return;
    };
    let Some(head) = source.get(line_range.clone()) else {
        return;
    };
    let lead = line_range.start + (head.len() - head.trim_start().len());
    let (head, removed) = (head.trim(), removed.trim());
    // A formatting-only edit (`{ 10 }` to `{ 10}`, `price + 1` to `price +1`)
    // changes no value; the line keeps its prior reading. Lines compare as
    // token text, so whitespace inside a literal (`"a b"` to `"ab"`) stays a
    // real edit; a line the scan cannot tokenize never takes this shortcut.
    if let (Some(head_tokens), Some(removed_tokens)) =
        (formatting_free_text(head), formatting_free_text(removed))
        && head_tokens == removed_tokens
    {
        return;
    }
    let (prefix, suffix) = super::diff::common_prefix_suffix(head, removed);
    let edit = lead + prefix..lead + head.len() - suffix;
    let Some(parse) = parsed
        .get_or_insert_with(|| last_file_parse(source))
        .as_ref()
    else {
        return;
    };
    let Some(branch) = crate::analysis::syntax::ra::inline_branch_edit(
        &parse.tree(),
        source,
        selected[position].0.start_byte,
        edit.clone(),
    ) else {
        return;
    };
    let spans_edit = |shape: &ParserProbeShape<'_>| {
        shape.start_byte <= edit.start && edit.end <= shape.start_byte + shape.text.len()
    };
    let Some((value, true)) = branch.value else {
        // Not a returned value: the predicate leaves only when another shape
        // on the line (`return 15`) spans the edit and stands for it. With
        // none, the line keeps today's reading rather than an unmodeled one.
        if selected
            .iter()
            .enumerate()
            .any(|(at, (shape, _))| at != position && spans_edit(shape))
        {
            selected.remove(position);
        }
        return;
    };
    let Some(value_text) = source.get(value.clone()) else {
        return;
    };
    selected.remove(position);
    // A returned-value shape elsewhere on the line is not this edit's.
    selected.retain(|(shape, _)| shape.family != ProbeFamily::ReturnValue || spans_edit(shape));
    if !selected
        .iter()
        .any(|(shape, _)| shape.family == ProbeFamily::ReturnValue)
    {
        selected.push((
            ParserProbeShape {
                family: ProbeFamily::ReturnValue,
                start_line: line,
                start_byte: value.start,
                text: value_text,
                standalone_call: true,
                unsafe_boundary: false,
            },
            None,
        ));
    }
}

thread_local! {
    /// #14192: the last whole-file parse [`line_is_nested_statement`] read.
    /// A diff visits one file's changed lines in a row, and a large file is
    /// past the shared memo's size cap, so one entry keyed by the full text
    /// saves a parse per changed line.
    static LAST_FILE_PARSE: std::cell::RefCell<Option<(String, Option<ra_ap_syntax::Parse<ast::SourceFile>>)>> =
        const { std::cell::RefCell::new(None) };
}

fn last_file_parse(source: &str) -> Option<ra_ap_syntax::Parse<ast::SourceFile>> {
    LAST_FILE_PARSE.with(|last| {
        let mut last = last.borrow_mut();
        if let Some((text, parse)) = last.as_ref()
            && text == source
        {
            return parse.clone();
        }
        let parse = parse_clean_source_file(source);
        *last = Some((source.to_string(), parse.clone()));
        parse
    })
}

/// #14192: whether the changed `line` lies wholly inside a statement (`let`,
/// expression statement or item) of a block nested in the returned
/// expression `shape` spans: `let doubled = item * 2;` inside
/// `with(|| { .. })`, `Box::pin(async move { .. })` or an `async { .. }` tail.
/// Such a line is a statement of its own block, not part of the returned
/// value, so the outer `return_value` / `error_path` shape does not apply to
/// it; a `return` on it keeps its own narrower shape. A nested block's tail
/// expression, a method-chain link and an argument line are not statements
/// and keep the outer shape. Any shape or line the parse cannot place keeps
/// today's selection.
fn line_is_nested_statement(
    source: &str,
    parse: &ra_ap_syntax::Parse<ast::SourceFile>,
    shape: &ProbeShapeFact,
    line: usize,
) -> bool {
    let Some(line_range) = source_line_byte_range(source, line) else {
        return false;
    };
    let Some(text) = source.get(line_range.clone()) else {
        return false;
    };
    let leading = text.len() - text.trim_start().len();
    let trailing = text.len() - text.trim_end().len();
    if text.trim().is_empty() {
        return false;
    }
    let (Ok(content_start), Ok(content_end)) = (
        u32::try_from(line_range.start + leading),
        u32::try_from(line_range.end - trailing),
    ) else {
        return false;
    };
    let shape_range = ra_ap_syntax::TextRange::new(shape.start_byte.into(), shape.end_byte.into());
    let content = ra_ap_syntax::TextRange::new(content_start.into(), content_end.into());
    if shape_range.is_empty() || !shape_range.contains_range(content) {
        return false;
    }
    let root = parse.tree();
    if shape_range.end() > root.syntax().text_range().end() {
        return false;
    }
    let ra_ap_syntax::NodeOrToken::Node(outer) = root.syntax().covering_element(shape_range) else {
        return false;
    };
    if outer.text_range() != shape_range {
        return false;
    }
    // A trailing `// comment` sits outside the statement's range, so the line
    // content ends at its last non-trivia token (#14917 review).
    let mut content_end = content.end();
    while content_end > content.start() {
        let Some(token) = root
            .syntax()
            .token_at_offset(content_end)
            .left_biased()
            .filter(|token| token.kind().is_trivia())
        else {
            break;
        };
        content_end = token.text_range().start().max(content.start());
    }
    let content = ra_ap_syntax::TextRange::new(content.start(), content_end);
    if content.is_empty() {
        return false;
    }
    let start = match root.syntax().covering_element(content) {
        ra_ap_syntax::NodeOrToken::Node(node) => node,
        ra_ap_syntax::NodeOrToken::Token(token) => match token.parent() {
            Some(parent) => parent,
            None => return false,
        },
    };
    // The innermost statement of a block that holds the whole line, walking up
    // from the line rather than down the whole shape (#14917 review).
    let Some(statement) = start.ancestors().find(|node| {
        ast::Stmt::can_cast(node.kind())
            && node
                .parent()
                .is_some_and(|parent| ast::StmtList::can_cast(parent.kind()))
            && node.text_range().contains_range(content)
    }) else {
        return false;
    };
    if !shape_range.contains_range(statement.text_range()) || statement == outer {
        return false;
    }
    // Only a closure or `async` body between the statement and the shape runs
    // as its own unit; a statement of a plain, `if`, `match` or `loop` block
    // still runs in the owner's own body and feeds its value (#14917 review).
    statement
        .ancestors()
        .skip(1)
        .take_while(|node| outer.text_range().contains_range(node.text_range()))
        .any(|node| {
            ast::ClosureExpr::can_cast(node.kind())
                || ast::BlockExpr::cast(node).is_some_and(|block| block.async_token().is_some())
        })
}

/// A line-only diff cannot identify which bytes changed when code outside an
/// unsafe boundary shares its opening or closing line. Re-resolve the exact
/// AST range and fail closed on those ambiguous edge lines; interior lines and
/// edge lines wholly owned by the boundary remain eligible.
fn unsafe_boundary_owns_changed_line(
    facts: &FileData,
    shape: &ProbeShapeFact,
    line: usize,
) -> bool {
    let Some(boundary) = unsafe_boundary_syntax_range(facts, shape) else {
        return false;
    };
    let Some(line_range) = source_line_byte_range(&facts.source, line) else {
        return false;
    };
    let boundary_start = u32::from(boundary.start()) as usize;
    let boundary_end = u32::from(boundary.end()) as usize;
    if boundary_start >= line_range.end || boundary_end <= line_range.start {
        return false;
    }

    if line == shape.start_line() {
        let prefix_end = boundary_start.max(line_range.start).min(line_range.end);
        let Some(prefix) = facts.source.get(line_range.start..prefix_end) else {
            return false;
        };
        if !boundary_edge_is_empty(prefix) {
            return false;
        }
    }
    if line == shape.end_line() {
        let suffix_start = boundary_end.max(line_range.start).min(line_range.end);
        let Some(suffix) = facts.source.get(suffix_start..line_range.end) else {
            return false;
        };
        if !boundary_edge_is_empty(suffix) {
            return false;
        }
    }
    true
}

fn unsafe_boundary_syntax_range(
    facts: &FileData,
    shape: &ProbeShapeFact,
) -> Option<ra_ap_syntax::TextRange> {
    let parse = parse_clean_source_file(&facts.source)?;
    let root = parse.tree();
    for function in root.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(token) = function.unsafe_token() else {
            continue;
        };
        if u32::from(token.text_range().start()) == shape.start_byte {
            return Some(function.syntax().text_range());
        }
    }
    for block in root.syntax().descendants().filter_map(ast::BlockExpr::cast) {
        let Some(token) = block.unsafe_token() else {
            continue;
        };
        if u32::from(token.text_range().start()) == shape.start_byte {
            return Some(block.syntax().text_range());
        }
    }
    None
}

fn source_line_byte_range(source: &str, line: usize) -> Option<Range<usize>> {
    if line == 0 {
        return None;
    }
    let mut start = 0usize;
    for _ in 1..line {
        let next = source.get(start..)?.find('\n')?;
        start = start.saturating_add(next).saturating_add(1);
    }
    let end = source
        .get(start..)?
        .find('\n')
        .map(|offset| start.saturating_add(offset))
        .unwrap_or(source.len());
    Some(start..end)
}

fn boundary_edge_is_empty(text: &str) -> bool {
    text.chars()
        .all(|character| character.is_whitespace() || character == ';')
}

fn parser_call_shape_is_standalone(facts: &FileData, shape: &ParserProbeShape<'_>) -> bool {
    if shape.family != ProbeFamily::CallDeletion {
        return true;
    }
    let end = shape.start_byte.saturating_add(shape.text.len());
    let Some(before) = facts.source.get(..shape.start_byte) else {
        return false;
    };
    let prefix = before.rsplit('\n').next().unwrap_or(before).trim();
    if !prefix.is_empty() {
        return false;
    }
    let Some(after) = facts.source.get(end..) else {
        return false;
    };
    let suffix = after.split('\n').next().unwrap_or(after).trim_start();
    suffix.starts_with(';') || suffix.starts_with("?;")
}

pub(crate) fn parser_expression_for_probe<'a>(
    index: &'a RustIndex,
    file: &Path,
    line: usize,
    family: &ProbeFamily,
    changed_text: &str,
) -> Option<&'a str> {
    parser_probe_shapes_for_changed_line(index, file, line, changed_text)
        .into_iter()
        .find(|shape| &shape.family == family)
        .map(|shape| shape.text)
}

fn file_facts<'a>(index: &'a RustIndex, file: &Path) -> Option<&'a FileData> {
    index
        .files()
        .get(file)
        .or_else(|| {
            index
                .files()
                .iter()
                .filter(|(indexed_path, _)| file.ends_with(indexed_path))
                .max_by_key(|(indexed_path, _)| indexed_path.as_os_str().len())
                .map(|(_, facts)| facts)
        })
        .map(|facts| facts.data())
}

/// Whether the raw `needle` match at `start` counts as a token run: it starts
/// on an unmasked code byte with no identifier or digit character on either
/// side, so `b: 2` is not found inside `b: 20` and a removed line's trailing
/// `// version: 0x2` comment does not make the edited `version: 0x2` field
/// read as unchanged. `masked` keeps the byte length of `haystack`, so a match
/// position indexes both.
fn is_token_run(haystack: &str, masked: &[u8], start: usize, needle: &str) -> bool {
    let starts_in_code = masked
        .get(start)
        .is_some_and(|byte| !byte.is_ascii_whitespace());
    if !starts_in_code {
        return false;
    }
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let before = haystack[..start].chars().next_back();
    let after = haystack[start + needle.len()..].chars().next();
    let open_start =
        needle.chars().next().is_none_or(|ch| !is_word(ch)) || before.is_none_or(|ch| !is_word(ch));
    let open_end = needle.chars().next_back().is_none_or(|ch| !is_word(ch))
        || after.is_none_or(|ch| !is_word(ch));
    open_start && open_end
}

/// How often `needle` occurs in `haystack` as a code token run that no
/// neighbouring operand syntax extends (#14722): `total > limit` is not
/// found in `subtotal > limit`, `total > limit.max`, `total > limit[0]`,
/// `total > limit + 1` or `x.total > limit`, but is found in
/// `let over = total > limit;`. A shape the diff producer cannot cut is
/// unchanged only when the old line holds it at least this often.
pub(super) fn operand_bounded_token_run_count(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let masked = mask_comments_and_strings(haystack);
    haystack
        .match_indices(needle)
        .filter(|(start, _)| {
            // Neighbours are read on the masked line, so a comment beside
            // the comparison (`a > b // note`) is not read as an operator.
            let next = masked
                .get(start + needle.len()..)
                .unwrap_or_default()
                .trim_start();
            let previous = masked.get(..*start).unwrap_or_default().trim_end();
            is_token_run(haystack, masked.as_bytes(), *start, needle)
                && !extends_rightward(next)
                && !extends_leftward(previous)
        })
        .count()
}

/// Whether `next` (the text after a match, leading space trimmed) continues
/// the right operand: a member, call, index, `?`, an arithmetic or bit
/// operator (`|` and `&` but not `||`/`&&`, which end the operand), a shift,
/// a macro `!`, a path, or an `as` cast.
fn extends_rightward(next: &str) -> bool {
    let doubled = |op: &str| next.starts_with(op);
    if next.starts_with([
        '.', '(', '[', '?', '+', '-', '*', '/', '%', ':', '^', '<', '>', '!',
    ]) {
        return true;
    }
    if (next.starts_with('|') && !doubled("||")) || (next.starts_with('&') && !doubled("&&")) {
        return true;
    }
    next.strip_prefix("as")
        .is_some_and(|rest| !rest.starts_with(|ch: char| ch.is_alphanumeric() || ch == '_'))
}

/// Whether `previous` (the text before a match, trailing space trimmed)
/// continues the left operand: a member or path, an arithmetic or bit
/// operator (`|` and `&` but not `||`/`&&`), a unary `!`, or a shift.
/// A struct field `:`, a match arrow `=>`, a return arrow `->`, a range
/// `..` and closure parameters `|x|` end the operand instead.
fn extends_leftward(previous: &str) -> bool {
    if previous.ends_with("..") {
        return false;
    }
    if previous.ends_with(['.', '+', '-', '*', '/', '%', '!', '^']) {
        return true;
    }
    let single = |op: char, doubled: &str| previous.ends_with(op) && !previous.ends_with(doubled);
    (single('|', "||") && !closes_closure_parameters(previous))
        || single('&', "&&")
        || previous.ends_with("::")
        || previous.ends_with(">>")
        || previous.ends_with("<<")
}

/// Whether `previous` ends with a closure parameter list (`|x|`,
/// `move |a, b: u8|`, `|x: &Vec<u8>|`) rather than a bit-or operand: the
/// opening `|` follows an expression start (`(`, `,`, `=`, `=>`, `{`, `;`,
/// `[`, the line start, `move`, `return` or another closure's parameters),
/// not a left operand as in `m | n | a > b`. A binary `|` always has a left
/// operand, so the parameter text itself needs no check.
fn closes_closure_parameters(previous: &str) -> bool {
    let Some(head) = previous.strip_suffix('|') else {
        return false;
    };
    let Some(open) = head.rfind('|') else {
        return false;
    };
    let before = head.get(..open).unwrap_or_default().trim_end();
    let ends_with_word = |word: &str| {
        before
            .strip_suffix(word)
            .is_some_and(|rest| !rest.ends_with(|ch: char| ch.is_alphanumeric() || ch == '_'))
    };
    before.is_empty()
        || before.ends_with(['(', ',', '=', '{', ';', '['])
        || before.ends_with("=>")
        || ends_with_word("move")
        || ends_with_word("return")
        || closes_closure_parameters(before)
}

/// A record field must be a whole syntactic unit, not the head or tail of a
/// longer expression: `flag: foo` inside the old `flag: foo && bar` is not the
/// unchanged field. Rejecting a match only makes a shape read as changed,
/// which falls back to the distance tie-break.
fn is_whole_unit(haystack: &str, start: usize, needle: &str) -> bool {
    let next = haystack[start + needle.len()..].trim_start().chars().next();
    let previous = haystack[..start].trim_end().chars().next_back();
    let closes = next.is_none_or(|ch| matches!(ch, ',' | '}' | ')' | ']' | ';' | '{'));
    let opens = previous.is_none_or(|ch| !"&|+-*/%<>=!^.".contains(ch));
    closes && opens
}

/// How often `needle` occurs in `haystack` as code (see [`is_token_run`]).
/// One needle at a time; [`TokenRunCounts`] answers many needles at once and
/// is checked against this in the tests.
#[cfg(test)]
fn token_run_count(haystack: &str, needle: &str, whole_unit: bool) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let masked = mask_comments_and_strings(haystack);
    let masked = masked.as_bytes();
    haystack
        .match_indices(needle)
        .filter(|(start, _)| {
            is_token_run(haystack, masked, *start, needle)
                && (!whole_unit || is_whole_unit(haystack, *start, needle))
        })
        .count()
}

/// Needles longer than this are searched per distinct value instead of hashed
/// window by window, which would cost `haystack * needle_len`.
const LONG_NEEDLE_BYTES: usize = 64;

#[derive(Default)]
struct TokenRunTally {
    /// End of the last raw match, which `match_indices` never overlaps.
    consumed: usize,
    plain: usize,
    whole: usize,
}

impl TokenRunTally {
    fn record(&mut self, haystack: &str, masked: &[u8], start: usize, needle: &str, whole: bool) {
        if is_token_run(haystack, masked, start, needle) {
            self.plain += 1;
            if whole && is_whole_unit(haystack, start, needle) {
                self.whole += 1;
            }
        }
    }
}

/// Token-run counts of many needles in one haystack, equal to calling the
/// per-needle count for each, in work bounded by
/// `haystack.len() * 64` per short length, plus one search per distinct long
/// needle, after masking once. The
/// per-needle count re-masks and re-scans the whole haystack per needle, which
/// is quadratic on one long generated line (#9895).
struct TokenRunCounts<'a> {
    tallies: HashMap<&'a str, TokenRunTally>,
}

impl<'a> TokenRunCounts<'a> {
    /// `needles` pairs each needle with whether any caller reads its
    /// whole-unit count.
    fn build(haystack: &str, needles: &[(&'a str, bool)]) -> Self {
        let mut by_len = BTreeMap::<usize, HashMap<&'a str, (bool, TokenRunTally)>>::new();
        for &(needle, whole_unit) in needles {
            if needle.is_empty() || needle.len() > haystack.len() {
                continue;
            }
            let entry = by_len
                .entry(needle.len())
                .or_default()
                .entry(needle)
                .or_default();
            entry.0 |= whole_unit;
        }
        let mut tallies = HashMap::new();
        if by_len.is_empty() {
            return Self { tallies };
        }
        let masked = mask_comments_and_strings(haystack);
        let masked = masked.as_bytes();
        for (needle_len, mut group) in by_len {
            if needle_len > LONG_NEEDLE_BYTES {
                // A long needle: hashing a `needle_len` window at every position
                // is quadratic, so search each distinct long value with the
                // former `match_indices` (work per distinct long value).
                for (&needle, (whole_unit, tally)) in &mut group {
                    #[cfg(test)]
                    TOKEN_RUN_WINDOW_BYTES
                        .with(|bytes| bytes.set(bytes.get().saturating_add(haystack.len())));
                    for (start, _) in haystack.match_indices(needle) {
                        tally.record(haystack, masked, start, needle, *whole_unit);
                    }
                }
            } else {
                #[cfg(test)]
                TOKEN_RUN_WINDOW_BYTES.with(|bytes| {
                    bytes.set(
                        bytes
                            .get()
                            .saturating_add(haystack.len().saturating_mul(needle_len)),
                    );
                });
                for (start, _) in haystack.char_indices() {
                    let end = start.saturating_add(needle_len);
                    let Some(window) = haystack.get(start..end) else {
                        if end > haystack.len() {
                            break;
                        }
                        continue;
                    };
                    let Some((whole_unit, tally)) = group.get_mut(window) else {
                        continue;
                    };
                    if start < tally.consumed {
                        continue;
                    }
                    tally.consumed = end;
                    tally.record(haystack, masked, start, window, *whole_unit);
                }
            }
            tallies.extend(
                group
                    .into_iter()
                    .map(|(needle, (_, tally))| (needle, tally)),
            );
        }
        Self { tallies }
    }

    fn get(&self, needle: &str, whole_unit: bool) -> usize {
        self.tallies.get(needle).map_or(
            0,
            |tally| {
                if whole_unit { tally.whole } else { tally.plain }
            },
        )
    }
}

/// Trim and rank rules of the former per-shape `str::contains` helper.
/// Category 0 is exact, 1 is shape-contains-changed, 2 is changed-contains-shape.
struct ChangedLineRank<'a> {
    trimmed: &'a str,
    contained: HashSet<&'a str>,
}

impl<'a> ChangedLineRank<'a> {
    fn build(changed_text: &'a str, shape_texts: impl Iterator<Item = &'a str>) -> Option<Self> {
        let trimmed = changed_text
            .trim()
            .trim_end_matches(';')
            .trim_end_matches(',');
        if trimmed.is_empty() {
            return None;
        }
        let mut by_len = BTreeMap::<usize, HashSet<&'a str>>::new();
        for shape_text in shape_texts {
            let shape = shape_text.trim();
            if shape.len() >= trimmed.len() {
                continue;
            }
            by_len.entry(shape.len()).or_default().insert(shape);
        }
        Some(Self {
            contained: needles_in_haystack(trimmed, &by_len),
            trimmed,
        })
    }

    fn rank(&self, shape_text: &str) -> Option<(u8, usize)> {
        let shape = shape_text.trim();
        if shape == self.trimmed {
            Some((0, 0))
        } else if shape.len() > self.trimmed.len() {
            shape
                .contains(self.trimmed)
                .then_some((1, shape.len() - self.trimmed.len()))
        } else if shape.len() < self.trimmed.len()
            && (shape.is_empty() || self.contained.contains(shape))
        {
            Some((2, self.trimmed.len() - shape.len()))
        } else {
            None
        }
    }
}

/// Answer `haystack.contains(needle)` for every needle, in work bounded by
/// `haystack.len() * 64` per short length, plus one search per distinct long
/// needle, rather than `needles * haystack`.
fn needles_in_haystack<'a>(
    haystack: &'a str,
    by_len: &BTreeMap<usize, HashSet<&'a str>>,
) -> HashSet<&'a str> {
    let mut found = HashSet::new();
    let hay_len = haystack.len();
    for (&needle_len, needles) in by_len {
        if needle_len == 0 || needle_len > hay_len || needles.is_empty() {
            continue;
        }
        if needle_len > LONG_NEEDLE_BYTES {
            // Long needles are searched per distinct value; hashing a long
            // window at every position is quadratic.
            for &needle in needles {
                #[cfg(test)]
                RANK_WINDOW_BYTES.with(|bytes| bytes.set(bytes.get().saturating_add(hay_len)));
                if haystack.contains(needle) {
                    found.insert(needle);
                }
            }
            continue;
        }
        #[cfg(test)]
        RANK_WINDOW_BYTES.with(|bytes| {
            bytes.set(
                bytes
                    .get()
                    .saturating_add(hay_len.saturating_mul(needle_len)),
            );
        });
        let last = hay_len - needle_len;
        let mut start = 0usize;
        let mut remaining = needles.clone();
        while start <= last && !remaining.is_empty() {
            if haystack.is_char_boundary(start)
                && haystack.is_char_boundary(start + needle_len)
                && let Some(hit) = remaining.take(&haystack[start..start + needle_len])
            {
                found.insert(hit);
            }
            start = match haystack.get(start..).and_then(|rest| rest.chars().next()) {
                Some(ch) => start.saturating_add(ch.len_utf8()),
                None => break,
            };
        }
    }
    found
}

#[cfg(test)]
thread_local! {
    static TOKEN_RUN_WINDOW_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
thread_local! {
    static RANK_WINDOW_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How `shape_match_rank` decided before #8647. Kept as the equivalence
/// oracle so a faster scan cannot quietly change category or distance.
#[cfg(test)]
fn legacy_shape_match_rank(shape_text: &str, changed_text: &str) -> Option<(u8, usize)> {
    let changed = changed_text
        .trim()
        .trim_end_matches(';')
        .trim_end_matches(',');
    if changed.is_empty() {
        return None;
    }
    let shape = shape_text.trim();
    if shape == changed {
        Some((0, 0))
    } else if shape.contains(changed) {
        Some((1, shape.len().saturating_sub(changed.len())))
    } else if changed.contains(shape) {
        Some((2, changed.len().saturating_sub(shape.len())))
    } else {
        None
    }
}

pub fn should_ignore_changed_line(text: &str) -> bool {
    text.is_empty()
        || text.starts_with("//")
        || text.starts_with("use ")
        || text.starts_with("pub use ")
        || text.starts_with("mod ")
        || text.starts_with("#")
        || is_visible_module_declaration(text)
        || is_plain_extern_crate(text)
}

/// A private, plain `extern crate name;` only links a crate, the same way a
/// private `use` imports one, so it holds no behavior for a test to notice
/// (#14403). A visibility-qualified form (`pub extern crate foo;`) re-exports
/// the crate under the declaring module's path, like a visible `use`, and the
/// renaming form (`extern crate core as std;`, `extern crate self as name;`)
/// changes what a name means for the rest of the crate; both stay probed, as
/// does any line with trailing code.
fn is_plain_extern_crate(text: &str) -> bool {
    let Some(body) = text.strip_suffix(';') else {
        return false;
    };
    body.strip_prefix("extern")
        .filter(|tail| tail.starts_with(char::is_whitespace))
        .map(str::trim_start)
        .and_then(|tail| tail.strip_prefix("crate"))
        .filter(|tail| tail.starts_with(char::is_whitespace))
        .map(str::trim)
        .is_some_and(|name| {
            let name = name.strip_prefix("r#").unwrap_or(name);
            name != "self"
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// A visibility-qualified out-of-line `mod name;` declaration
/// (`pub mod new;`, `pub(crate) mod x;`) names a module and holds no behavior
/// to check. Visibility-qualified `use` lines are deliberately not covered: a
/// retargeted re-export (`pub(crate) use fast::selected;` to
/// `pub(crate) use slow::selected;`) changes which implementation existing call
/// sites use. Only the `;`-terminated form qualifies, so an inline module body
/// or any trailing code stays probed.
fn is_visible_module_declaration(text: &str) -> bool {
    let rest = super::diff::strip_visibility(text);
    if rest.len() == text.len() {
        return false;
    }
    let Some(body) = rest.strip_suffix(';') else {
        return false;
    };
    if body.contains(['{', '}', ';', '=']) {
        return false;
    }
    body.strip_prefix("mod")
        .filter(|tail| tail.starts_with(char::is_whitespace))
        .map(str::trim)
        .is_some_and(|name| {
            let name = name.strip_prefix("r#").unwrap_or(name);
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// An `impl`, `struct`, `enum`, `union` or `trait` header (`impl Tr for K where
/// Self: Sized {`, `enum Op<Q: Ord> {`) holds no expression:
/// its `Self: Sized` text only looks like a field `name: Type`, so a
/// constructed-field probe on it reports a gap that does not exist (#8389).
/// The header itself stays probed as `static_unknown`, so a changed bound is
/// never silently dropped. A one-line `impl` whose body holds a `fn` is not a
/// header.
pub(crate) fn is_item_header(text: &str) -> bool {
    // A trailing comment (`// fn go`) must not read as a body.
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let text = masked.trim();
    let text = text
        .strip_prefix("unsafe")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map_or(text, str::trim_start);
    let word_after = |word: &str| {
        text.strip_prefix(word).is_some_and(|rest| {
            rest.is_empty() || rest.starts_with('<') || rest.starts_with(char::is_whitespace)
        })
    };
    // A `;` inside brackets (`where [u8; 4]: Copy`) is an array type, not a body.
    let mut depth = 0usize;
    let body_semicolon = text.chars().any(|c| match c {
        '[' | '(' => {
            depth += 1;
            false
        }
        ']' | ')' => {
            depth = depth.saturating_sub(1);
            false
        }
        ';' => depth == 0,
        _ => false,
    });
    if body_semicolon {
        return false;
    }
    // Text after the opening `{` is a one-line body, not a header.
    if text.contains('{') && !text.ends_with('{') {
        return false;
    }
    // A one-line `impl` body holding a `fn` is not a header. A type header's
    // `fn` is a fn-pointer bound (`T: Fn(fn(u8))`) and stays a header. A line
    // opening with `impl` that holds no body brace or `;` is the first line
    // of a rustfmt-wrapped header (`impl<T, const N: usize> Drop`) and also
    // declares only: its generic bounds are not fields or calls.
    // A const-generic block, a macro or an array-length call in the header
    // (`impl Foo<[u8; width()]>`) is an expression that can call code, so it
    // keeps its call probe exactly as for the other item headers below.
    if word_after("impl")
        && !text.contains("fn ")
        && !generics_hold_expression(text.strip_prefix("impl").unwrap_or(text))
    {
        return true;
    }
    // #9170: a `struct`/`enum`/`union`/`trait` header declares generic bounds
    // (`enum Op<Q: Ord + Copy> {`) and runs nothing; the bound only looks
    // like a field `name: Type`. Only a header that opens its body on this
    // line counts, so a one-line `struct A { x: u8 }` stays probed.
    let declared = super::diff::strip_visibility(text);
    let declared = declared
        .strip_prefix("unsafe")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map_or(declared, str::trim_start);
    ["struct", "enum", "union", "trait"].iter().any(|word| {
        declared.strip_prefix(word).is_some_and(|rest| {
            // `union` is a contextual keyword: `union = Bits {` is an
            // expression, so a declaration needs an identifier next.
            rest.starts_with(char::is_whitespace)
                && rest
                    .trim_start()
                    .starts_with(|c: char| c.is_alphabetic() || c == '_')
                && text.ends_with('{')
                && !generics_hold_expression(rest)
        })
    })
}

/// A one-line `type` item, whether a trait's associated type (`type Eq: CmpEq;`,
/// `type Item;`), an impl's binding (`type Eq = __m128i;`) or a type alias
/// (`pub type Map<K> = BTreeMap<K, u8>;`), declares a type and runs nothing
/// (#14429). Its `name: Bound` only looks like a constructed field `name:
/// value`, so it reads as one `static_unknown`, like an item header, and a
/// changed type stays visible. The line must hold the whole item, ending in
/// `;`. A type that holds an expression (a block, a macro, or any array
/// length) keeps its ordinary families.
pub(crate) fn is_type_item_declaration(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    let text = masked.trim();
    let declared = super::diff::strip_visibility(text);
    let declared = declared
        .strip_prefix("default")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map_or(declared, str::trim_start);
    let Some(rest) = declared
        .strip_prefix("type")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map(str::trim_start)
    else {
        return false;
    };
    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let name = rest.get(..name_len).unwrap_or_default();
    let starts_like_identifier = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let Some(body) = rest.strip_suffix(';') else {
        return false;
    };
    // A `;` outside brackets joins a second statement (`type A = u8; f();`);
    // one inside an array type (`[u8; 4]`) is part of the type.
    let mut depth = 0usize;
    let second_statement = body.chars().any(|c| match c {
        '[' | '(' | '<' => {
            depth += 1;
            false
        }
        ']' | ')' | '>' => {
            depth = depth.saturating_sub(1);
            false
        }
        ';' => depth == 0,
        _ => false,
    });
    // An array length is a const expression evaluated in the type
    // (`[T; 4]`), so the item keeps its ordinary families, as the inline
    // bound owner rule requires.
    let has_array_length = array_length_texts(body).next().is_some();
    starts_like_identifier
        && !second_statement
        && !has_array_length
        && !generics_hold_expression(body)
}

/// A const-generic default (`<const N: usize = { capacity() }>`) or an array
/// length in a generic default is an expression that runs at compile time and
/// can call code, so a header carrying one keeps its call probe instead of
/// reading as declaration-only. `rest` ends with the header's own `{`.
fn generics_hold_expression(rest: &str) -> bool {
    let generics = rest.strip_suffix('{').unwrap_or(rest);
    let masked = mask_comments_and_strings(generics);
    // A block expression, or a macro invocation, in the header.
    let bang_macro = masked
        .match_indices('!')
        .any(|(i, _)| masked[i + 1..].trim_start().starts_with(['(', '[', '{']));
    // An array length call: `(` after a `;` (`[u8; cap()]`), not `[u8; 4]`.
    let array_len_call = array_length_texts(&masked).any(|len| len.contains('('));
    masked.contains('{') || bang_macro || array_len_call
}

/// The array-length text of each `[elem; len]` in `text`: the text after the
/// `;` that belongs to that bracket's own frame, so a `;` inside a nested
/// element (`[([u8; 4], fn()); 3]`) is not the outer length's separator.
fn array_length_texts(text: &str) -> impl Iterator<Item = &str> {
    // Each open bracket: (kind, byte index, its own `;` if it is a `[`).
    let mut frames: Vec<(char, usize, Option<usize>)> = Vec::new();
    let mut lengths = Vec::new();
    for (i, c) in text.char_indices() {
        match c {
            '[' | '(' | '{' => frames.push((c, i, None)),
            ';' => {
                if let Some(frame) = frames.last_mut()
                    && frame.0 == '['
                    && frame.2.is_none()
                {
                    frame.2 = Some(i);
                }
            }
            ']' | ')' | '}' => {
                if let Some((kind, _, semi)) = frames.pop()
                    && kind == '['
                    && let Some(semi) = semi
                {
                    lengths.push(&text[semi + 1..i]);
                }
            }
            _ => {}
        }
    }
    lengths.into_iter()
}

/// A line that only opens or closes a block (`}`, `});`, `} else {`,
/// `else {`) carries no expression of its own (#4216 row 5). Opening `(`
/// and `[` are not delimiters here, so a unit `()` or empty `[]` value is
/// never structural; nor is `else if`, a tail value, or a call.
///
/// Structural is not the same as ignorable: inserting `} else {` alone
/// splits a block and changes behavior. The diff producer skips a
/// structural line only when its contiguous changed run also holds a
/// behavioral line that seeds the probe for that change.
pub(crate) fn is_structural_delimiter_line(text: &str) -> bool {
    let rest = text.trim_matches(|ch: char| {
        matches!(ch, '{' | '}' | ')' | ']' | ';' | ',') || ch.is_whitespace()
    });
    rest.is_empty() || rest == "else"
}

#[cfg(test)]
mod tests {
    use super::super::super::rust_index::{FileFacts, ProbeShapeFact, ProbeShapeKind, RustIndex};
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn numbered_lines(count: usize) -> Vec<crate::analysis::diff::ChangedLine> {
        (1..=count)
            .map(|line| crate::analysis::diff::ChangedLine {
                line,
                new_side_line: line,
                text: format!("let x{line} = {line};"),
            })
            .collect()
    }

    /// #11439: the verdict pass must stop examining lines once the run is
    /// cancelled. Counting the lines the verdict closure is handed is
    /// deterministic, unlike a wall-clock bound; the old single call over
    /// every line would hand it all 1000.
    #[test]
    fn type_bound_verdicts_stop_after_cancellation() {
        use crate::analysis::cancellation::{AnalysisAbortKind, AnalysisCancellationToken};
        let lines = numbered_lines(1000);
        let token = AnalysisCancellationToken::new();

        // Control: an active token examines every line, in order, with the
        // same verdicts an unchunked call would give.
        let mut examined = 0usize;
        let all = crate::analysis::cancellation::with_token(&token, || {
            verdicts_in_cancellable_chunks(&lines, 100, |chunk| {
                examined += chunk.len();
                chunk.iter().map(|line| line.line % 2 == 0).collect()
            })
        });
        assert_eq!(examined, 1000);
        assert_eq!(all.len(), 1000);
        assert!(
            all.iter()
                .enumerate()
                .all(|(i, v)| *v == ((i + 1) % 2 == 0))
        );

        // Cancel after the third chunk: three chunks examined, no more, and
        // the result still has one entry per line.
        let mut chunks = 0usize;
        let mut examined = 0usize;
        let cancelled = crate::analysis::cancellation::with_token(&token, || {
            verdicts_in_cancellable_chunks(&lines, 100, |chunk| {
                chunks += 1;
                examined += chunk.len();
                if chunks == 3 {
                    token.cancel(AnalysisAbortKind::Cancelled);
                }
                vec![true; chunk.len()]
            })
        });
        assert_eq!((chunks, examined), (3, 300));
        assert_eq!(cancelled.len(), 1000);
        assert!(cancelled[..300].iter().all(|v| *v));
        assert!(cancelled[300..].iter().all(|v| !*v));

        // The token is sticky: a run that starts cancelled examines nothing.
        let mut examined = 0usize;
        let none = crate::analysis::cancellation::with_token(&token, || {
            verdicts_in_cancellable_chunks(&lines, 100, |chunk| {
                examined += chunk.len();
                vec![true; chunk.len()]
            })
        });
        assert_eq!(examined, 0);
        assert_eq!(none.len(), 1000);
    }

    /// #8389: an `impl` header is not a behavior line, but a one-line `impl`,
    /// a `fn` signature, an expression that merely contains the word and a
    /// function `where` bound stay probed.
    #[test]
    fn impl_headers_are_item_headers_but_behavior_is_not() {
        for header in [
            "impl Tr for K {",
            "impl<T: Debug> Tr for K<T> where Self: Sized {",
            "impl Tr for K where Self: Sized",
            "impl Tr for K where Self: Sized { // fn go",
            "impl Tr for K where Self: Sized { // x;",
            "impl<T: Fn(u8)> Tr for K<T> where Self: Sized {",
            "impl\tTr for K where Self: Sized {",
            "unsafe\timpl Tr for K where [u8; 4]: Copy {",
            "unsafe impl Send for K {",
            "impl<T: Iterator<Item = u32>> Tr for K<T> {",
            // A rustfmt-wrapped header's first line opens no body yet.
            "impl<T: Debug> Tr for K<T>",
            "impl<Item, const N: usize> Drop",
            "unsafe impl<#[cfg_attr(feature = \"x\", may_dangle)] T> Drop",
            // #9170: type declaration headers with generic bounds.
            "enum Operator<P1, P2, Q: Ord + Copy> {",
            "pub(crate) struct Wrapper<T: Clone + Send> {",
            "pub struct Pair<A, B: Default> where A: Copy {",
            "unsafe trait Marker<T: Sized>: Send {",
            "union Bits<T: Copy> {",
            "pub struct Arr<T> where [u8; 4]: Copy {",
            "enum Op<T> where [([u8; 4], fn()); 3]: Copy {",
            "enum É<T: Copy> {",
            "struct Δ<T: Copy> {",
            "struct r#async<T: Copy> {",
            "struct Callback<T: Fn(fn (u8))> {",
            "struct Wrapper<T> where [u8; 4]: Copy, T: Fn() {",
        ] {
            assert!(is_item_header(header), "`{header}`");
            assert_eq!(
                super::super::lexical::classify_changed_line(header),
                vec![ProbeFamily::StaticUnknown],
                "`{header}` must not classify as a field"
            );
        }
        for behavior in [
            "impl K { fn go(&self) -> u32 { 1 } }",
            "fn go() -> impl Iterator<Item = u32> {",
            "implement(x);",
            "let impl_len = 3;",
            "where T: Debug,",
            "impl Tr for K where Self: Sized;",
            "impl_len += 1",
            "impl<T: Debug> Tr for K<T>; let x = 1",
            "impl Foo<[u8; width()]>",
            "impl Foo<{ width() }> for K",
            "impl Foo<m!()> for K",
            "impl<const N: usize> K { Foo { a: 1 } }",
            "impl K { const A: u32 = 1; }",
            "impl K where Self: Sized { const A: u32 = 1; }",
            "impl K where Self: Sized { Foo { a: 1 } }",
            "T: Debug + Clone,",
            // #9170: one-line bodies, constructions and lookalike words.
            "struct A { x: u8 }",
            "enum E { A = 1 }",
            "let enum_len: usize = 3;",
            "structure.field: Foo {",
            "traitor: Foo {",
            "pub fn enum_of<T: Clone>(t: T) -> T {",
            // Contextual keyword `union` and other non-declaration uses.
            "union = Bits {",
            "union |= Bits {",
            "union => Bits {",
            "union : Foo {",
            "union .map(|x| {",
            // Const-generic defaults and macros are expressions that can
            // call code: they keep their call probe.
            "struct Buffer<const N: usize = { capacity() }> {",
            "struct Buffer<T = [u8; cap()]> {",
            "struct Tagged<const N: usize = { len!() }> {",
            "struct S<T = m!()> {",
            "struct S<T = [[u8; cap()]; 2]> {",
            "struct = Foo {",
            "enum : Foo {",
        ] {
            assert!(!is_item_header(behavior), "`{behavior}`");
        }
    }

    #[test]
    fn array_length_texts_follow_bracket_nesting() {
        fn lengths(text: &str) -> Vec<&str> {
            array_length_texts(text).collect()
        }
        assert_eq!(lengths("[u8; 4]"), [" 4"]);
        assert_eq!(lengths("[([u8; 4], fn()); 3]"), [" 4", " 3"]);
        assert_eq!(lengths("[[u8; cap()]; 2]"), [" cap()", " 2"]);
        assert!(lengths("[u8]").is_empty());
        assert!(lengths("f(a; b)").is_empty());
    }

    /// #9033: a visibility-qualified `mod name;` declaration has no behavior;
    /// inline module bodies, trailing code, and every visibility-qualified
    /// `use` (a retargeted re-export changes behavior) stay probed.
    #[test]
    fn visible_module_declarations_are_ignored_but_behavior_is_not() {
        for ignored in [
            "pub mod new;",
            "pub(crate) mod x;",
            "pub(in crate::a) mod y;",
            "pub ( super ) mod r#raw;",
            "mod new;",
        ] {
            assert!(should_ignore_changed_line(ignored), "`{ignored}`");
        }
        for kept in [
            "pub mod new { fn go() {} }",
            "pub mod new { pub fn go() { 1 } }",
            "pub mod new; fn go() {}",
            "pub fn mod_new() {}",
            "pub const MODE: u8 = 1;",
            "pub modx;",
            "pub mod ;",
            "pub(crate) mod",
            "pub( mod x;",
            "pub(crate) static X: u8 = 1;",
            "pub(crate) use a::b;",
            "pub(crate) use fast::selected;",
            "pub(in crate::a) use slow::selected;",
        ] {
            assert!(!is_visible_module_declaration(kept), "`{kept}`");
        }
        // `pub use` keeps its pre-existing ignore; the new helper must not
        // be what claims it.
        assert!(!is_visible_module_declaration("pub use a::b;"));
    }

    /// #14429: a one-line `type` item declares a type; its bound only looks
    /// like a constructed field. Expressions in the type and trailing code
    /// keep their ordinary families.
    #[test]
    fn type_items_read_as_declarations_not_fields() {
        use crate::analysis::probes::lexical::classify_changed_line;
        for declaration in [
            "type Eq: CmpEq;",
            "type Mask: MoveMask;",
            "type Item;",
            "type Eq = __m128i;",
            "pub type Map<K> = BTreeMap<K, u8>;",
            "default type Output = Self;",
            "type Error: std::error::Error + Send + Sync + 'static;",
            "type F = fn(u8) -> u8;",
        ] {
            assert!(is_type_item_declaration(declaration), "`{declaration}`");
            assert_eq!(
                classify_changed_line(declaration),
                vec![ProbeFamily::StaticUnknown],
                "`{declaration}`"
            );
        }
        for kept in [
            "type Buf = [u8; cap()];",
            "pub(crate) type Buf = [u8; 4];",
            "type Packet<T: Clone> = [T; 4];",
            "type A = u8; f();",
            "type Out = { 1 };",
            "type M = mac!(u8);",
            "type Eq: CmpEq",
            "type = u8;",
            "type 1x = u8;",
            "typed: u8,",
            "type_: Kind,",
            "let ty = Type::new();",
        ] {
            assert!(!is_type_item_declaration(kept), "`{kept}`");
        }
    }

    /// #14429 producer level: a changed trait associated type yields no
    /// field-construction probe, while a changed struct-literal field still
    /// does (negative control).
    #[test]
    fn diff_producer_reads_associated_type_as_declaration() -> Result<(), String> {
        use crate::analysis::diff::{ChangedFile, ChangedLine};
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
        let source = "pub trait Vector {\n    type Eq: CmpEq;\n}\npub struct S {\n    pub a: u8,\n}\npub fn make() -> S {\n    S {\n        a: 1,\n    }\n}\n";
        let path = PathBuf::from("src/lib.rs");
        let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(path.clone(), facts)]),
            ..Default::default()
        });
        let families_for = |line: usize| {
            let text = source.lines().nth(line - 1).unwrap_or_default().to_string();
            let changed = ChangedFile {
                path: path.clone(),
                added_lines: vec![ChangedLine {
                    line,
                    new_side_line: line,
                    text,
                }],
                removed_lines: Vec::new(),
            };
            super::super::diff::probes_for_file(std::path::Path::new("."), &changed, &index)
                .into_iter()
                .map(|probe| probe.family)
                .collect::<Vec<_>>()
        };
        let associated = families_for(2);
        assert!(
            !associated.contains(&ProbeFamily::FieldConstruction),
            "`type Eq: CmpEq;` must not read as a constructed field: {associated:?}"
        );
        let field = families_for(9);
        assert!(
            field.contains(&ProbeFamily::FieldConstruction),
            "the struct-literal field `a: 1,` must stay field_construction: {field:?}"
        );
        Ok(())
    }

    /// #14403: a private plain `extern crate name;` links a crate and has no
    /// behavior; a re-export (`pub extern crate x;`), a renaming
    /// `extern crate a as b;` and any trailing code stay probed.
    #[test]
    fn plain_extern_crate_is_ignored_but_a_rename_is_not() {
        for ignored in [
            "extern crate alloc;",
            "extern  crate  std ;",
            "extern crate r#async;",
        ] {
            assert!(should_ignore_changed_line(ignored), "`{ignored}`");
        }
        for kept in [
            "pub extern crate core;",
            "pub(crate) extern crate serde_json;",
            "extern crate core as std;",
            "pub extern crate other as u32;",
            "extern crate self as my_crate;",
            "extern crate self;",
            "extern crate alloc; fn go() {}",
            "extern crate alloc",
            "extern crate;",
            "extern \"C\" fn go();",
            "externcrate alloc;",
            "extern cratex alloc;",
            "extern crate 1alloc;",
        ] {
            assert!(!should_ignore_changed_line(kept), "`{kept}`");
        }
    }

    /// Producer level: an added `extern crate alloc;` yields no probe, while a
    /// renaming `extern crate core as std;` still yields one.
    #[test]
    fn diff_producer_ignores_plain_extern_crate_but_probes_a_rename() -> Result<(), String> {
        use crate::analysis::diff::{ChangedFile, ChangedLine};
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
        let source = "extern crate alloc;\nextern crate core as std;\n";
        let path = PathBuf::from("src/lib.rs");
        let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(path.clone(), facts)]),
            ..Default::default()
        });
        let probes_for = |line: usize| {
            let text = source.lines().nth(line - 1).unwrap_or_default().to_string();
            let changed = ChangedFile {
                path: path.clone(),
                added_lines: vec![ChangedLine {
                    line,
                    new_side_line: line,
                    text,
                }],
                removed_lines: Vec::new(),
            };
            super::super::diff::probes_for_file(std::path::Path::new("."), &changed, &index)
        };
        assert!(
            probes_for(1).is_empty(),
            "`extern crate alloc;` must yield no probe"
        );
        assert!(
            !probes_for(2).is_empty(),
            "a renaming `extern crate core as std;` must still be probed"
        );
        Ok(())
    }

    /// Producer level: an added `pub mod new;` yields no probe, while a
    /// retargeted `pub(crate) use` re-export still yields one.
    #[test]
    fn diff_producer_ignores_visible_mod_but_probes_retargeted_use() -> Result<(), String> {
        use crate::analysis::diff::{ChangedFile, ChangedLine};
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
        let source = "pub mod new;\npub(crate) use slow::selected;\n";
        let path = PathBuf::from("src/lib.rs");
        let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(path.clone(), facts)]),
            ..Default::default()
        });
        let probes_for = |line: usize| {
            let text = source.lines().nth(line - 1).unwrap_or_default().to_string();
            let changed = ChangedFile {
                path: path.clone(),
                added_lines: vec![ChangedLine {
                    line,
                    new_side_line: line,
                    text,
                }],
                removed_lines: Vec::new(),
            };
            super::super::diff::probes_for_file(std::path::Path::new("."), &changed, &index)
        };
        assert!(
            probes_for(1).is_empty(),
            "`pub mod new;` must yield no probe"
        );
        assert!(
            !probes_for(2).is_empty(),
            "a retargeted `pub(crate) use` must still be probed"
        );
        Ok(())
    }

    #[test]
    fn classify_functions_are_callable() {
        assert!(should_ignore_changed_line("// comment"));
        assert!(!should_ignore_changed_line("let x = 5;"));
    }

    /// #4216 row 5: brace-only and else-only lines are structural; every
    /// line that still holds a token of behavior (a tail value, `else if`
    /// condition, call, unit value) is not.
    #[test]
    fn structural_delimiter_lines_are_recognized() {
        for structural in [
            "}", "{", "} else {", "else {", "} else", "});", "},", "};", "]", ")",
        ] {
            assert!(
                is_structural_delimiter_line(structural),
                "`{structural}` is structural"
            );
            assert!(
                !should_ignore_changed_line(structural),
                "`{structural}` is never ignored unconditionally"
            );
        }
        for kept in [
            "amount",
            "} else if amount > 5 {",
            "} else { amount }",
            "}))?;",
            "()",
            "[]",
            "Ok(())",
            "elsewhere",
            "} else_value",
        ] {
            assert!(
                !is_structural_delimiter_line(kept),
                "`{kept}` is not structural"
            );
        }
    }

    #[test]
    fn parser_probe_shapes_use_matching_syntax_shape() {
        let path = PathBuf::from("src/lib.rs");
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    probe_shapes: vec![
                        ProbeShapeFact {
                            start_line: 3,
                            end_line: 3,
                            start_byte: 0,
                            end_byte: 24,
                            kind: ProbeShapeKind::Predicate,
                            text: "if amount >= threshold {".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                        ProbeShapeFact {
                            start_line: 7,
                            end_line: 7,
                            start_byte: 20,
                            end_byte: 43,
                            kind: ProbeShapeKind::ErrorPath,
                            text: "Err(AuthError::Revoked)".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                    ],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let shapes = parser_probe_shapes_for_changed_line(&index, &path, 3, "amount >= threshold;");
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].family, ProbeFamily::Predicate);
    }

    #[test]
    fn parser_probe_shapes_are_empty_without_matching_shape() {
        let path = PathBuf::from("src/lib.rs");
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    probe_shapes: vec![ProbeShapeFact {
                        start_line: 3,
                        end_line: 3,
                        start_byte: 0,
                        end_byte: 24,
                        kind: ProbeShapeKind::Predicate,
                        text: "if amount >= threshold {".into(),
                        value_consumed: false,
                        source_lines: None,
                    }],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let shapes = parser_probe_shapes_for_changed_line(&index, &path, 4, "return total");
        assert!(shapes.is_empty());
    }

    #[test]
    fn unsafe_boundaries_match_by_span_and_prefer_the_innermost_boundary() -> Result<(), String> {
        let path = PathBuf::from("src/lib.rs");
        let source = "pub unsafe fn read_raw(value: i32, limit: i32) -> i32 {\n    unsafe {\n        if value < limit { value } else { limit }\n    }\n}\n";
        let function_start = source
            .find("unsafe fn")
            .ok_or_else(|| "missing unsafe function token".to_string())?;
        let block_start = source
            .rfind("unsafe {")
            .ok_or_else(|| "missing unsafe block token".to_string())?;
        let predicate_start = source
            .find("value < limit")
            .ok_or_else(|| "missing predicate".to_string())?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    source: source.into(),
                    probe_shapes: vec![
                        ProbeShapeFact {
                            start_line: 1,
                            end_line: 5,
                            start_byte: crate::analysis::facts::probe_shape_position(
                                function_start,
                            ),
                            end_byte: crate::analysis::facts::probe_shape_position(
                                function_start + 18,
                            ),
                            kind: ProbeShapeKind::UnsafeBoundary,
                            text: "unsafe fn read_raw".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                        ProbeShapeFact {
                            start_line: 2,
                            end_line: 4,
                            start_byte: crate::analysis::facts::probe_shape_position(block_start),
                            end_byte: crate::analysis::facts::probe_shape_position(
                                block_start + 12,
                            ),
                            kind: ProbeShapeKind::UnsafeBoundary,
                            text: "unsafe block".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                        ProbeShapeFact {
                            start_line: 3,
                            end_line: 3,
                            start_byte: crate::analysis::facts::probe_shape_position(
                                predicate_start,
                            ),
                            end_byte: crate::analysis::facts::probe_shape_position(
                                predicate_start + 13,
                            ),
                            kind: ProbeShapeKind::Predicate,
                            text: "value < limit".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                    ],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let shapes = parser_probe_shapes_for_changed_line(&index, &path, 3, "value < limit");
        assert_eq!(shapes.len(), 2);
        assert!(
            shapes
                .iter()
                .any(|shape| shape.family == ProbeFamily::Predicate)
        );
        let unsafe_shape = shapes
            .iter()
            .find(|shape| shape.family == ProbeFamily::StaticUnknown)
            .ok_or_else(|| "missing unsafe boundary shape".to_string())?;
        assert_eq!(unsafe_shape.start_line, 3);
        assert_eq!(unsafe_shape.start_byte, block_start);
        assert_eq!(unsafe_shape.text, "unsafe block");
        assert!(unsafe_shape.unsafe_boundary);
        Ok(())
    }

    #[test]
    fn unsafe_boundary_rejects_shared_edge_lines_with_outside_code() -> Result<(), String> {
        let path = PathBuf::from("src/lib.rs");
        let source = "pub fn read(ptr: *const u8) -> u8 { let limit = 2; unsafe { ptr.read() } }\n";
        let block_start = source
            .find("unsafe {")
            .ok_or_else(|| "missing unsafe block token".to_string())?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    source: source.into(),
                    probe_shapes: vec![ProbeShapeFact {
                        start_line: 1,
                        end_line: 1,
                        start_byte: crate::analysis::facts::probe_shape_position(block_start),
                        end_byte: crate::analysis::facts::probe_shape_position(block_start + 12),
                        kind: ProbeShapeKind::UnsafeBoundary,
                        text: "unsafe block".into(),
                        value_consumed: false,
                        source_lines: None,
                    }],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let shapes = parser_probe_shapes_for_changed_line(
            &index,
            &path,
            1,
            "pub fn read(ptr: *const u8) -> u8 { let limit = 3; unsafe { ptr.read() } }",
        );
        assert!(shapes.is_empty());
        Ok(())
    }

    #[test]
    fn unsafe_boundary_accepts_an_edge_line_owned_by_the_boundary() -> Result<(), String> {
        let path = PathBuf::from("src/lib.rs");
        let source = "pub fn read(ptr: *const u8) -> u8 {\n    unsafe { ptr.read() }\n}\n";
        let block_start = source
            .find("unsafe {")
            .ok_or_else(|| "missing unsafe block token".to_string())?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    source: source.into(),
                    probe_shapes: vec![ProbeShapeFact {
                        start_line: 2,
                        end_line: 2,
                        start_byte: crate::analysis::facts::probe_shape_position(block_start),
                        end_byte: crate::analysis::facts::probe_shape_position(block_start + 12),
                        kind: ProbeShapeKind::UnsafeBoundary,
                        text: "unsafe block".into(),
                        value_consumed: false,
                        source_lines: None,
                    }],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let shapes =
            parser_probe_shapes_for_changed_line(&index, &path, 2, "unsafe { ptr.add(1).read() }");
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].family, ProbeFamily::StaticUnknown);
        Ok(())
    }

    #[test]
    fn parser_call_shapes_preserve_inventory_and_mark_diff_eligibility() -> Result<(), String> {
        let path = PathBuf::from("src/lib.rs");
        let source = "fn demo() {\n    read()?;\n    let value = read()?;\n}\n";
        let first = source
            .find("read()")
            .ok_or_else(|| "missing standalone call".to_string())?;
        let second = source
            .rfind("read()")
            .ok_or_else(|| "missing initializer call".to_string())?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    source: source.into(),
                    probe_shapes: vec![
                        ProbeShapeFact {
                            start_line: 2,
                            end_line: 2,
                            start_byte: crate::analysis::facts::probe_shape_position(first),
                            end_byte: crate::analysis::facts::probe_shape_position(first + 6),
                            kind: ProbeShapeKind::CallDeletion,
                            text: "read()".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                        ProbeShapeFact {
                            start_line: 3,
                            end_line: 3,
                            start_byte: crate::analysis::facts::probe_shape_position(second),
                            end_byte: crate::analysis::facts::probe_shape_position(second + 6),
                            kind: ProbeShapeKind::CallDeletion,
                            text: "read()".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                    ],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let standalone = parser_probe_shapes_for_changed_line(&index, &path, 2, "read()?;");
        let initializer =
            parser_probe_shapes_for_changed_line(&index, &path, 3, "let value = read()?;");
        if standalone.len() != 1 || !standalone[0].standalone_call {
            return Err(format!("standalone call was not retained: {standalone:?}"));
        }
        if initializer.len() != 1 || initializer[0].standalone_call {
            return Err(format!("initializer call was promoted: {initializer:?}"));
        }
        Ok(())
    }

    #[test]
    fn parser_expression_resolves_absolute_probe_to_most_specific_shape() -> Result<(), String> {
        let path = PathBuf::from("src/gate_watchdog.rs");
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path,
                FileFacts {
                    path: PathBuf::from("src/gate_watchdog.rs"),
                    probe_shapes: vec![
                        ProbeShapeFact {
                            start_line: 10,
                            end_line: 13,
                            start_byte: 100,
                            end_byte: 150,
                            kind: ProbeShapeKind::CallDeletion,
                            text: "watchdog_reason(\n    \"run-missing\",\n    receipt,\n)".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                        ProbeShapeFact {
                            start_line: 10,
                            end_line: 15,
                            start_byte: 90,
                            end_byte: 142,
                            kind: ProbeShapeKind::CallDeletion,
                            text: "with_reason(watchdog_reason(\"run-missing\", receipt))".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                    ],
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        });

        let expression = parser_expression_for_probe(
            &index,
            Path::new("/repo/ub-review/src/gate_watchdog.rs"),
            10,
            &ProbeFamily::CallDeletion,
            "watchdog_reason(",
        )
        .ok_or_else(|| "expected parser expression".to_string())?;
        if expression != "watchdog_reason(\n    \"run-missing\",\n    receipt,\n)" {
            return Err(format!("unexpected parser expression: {expression}"));
        }
        let outer = parser_expression_for_probe(
            &index,
            Path::new("/repo/ub-review/src/gate_watchdog.rs"),
            10,
            &ProbeFamily::CallDeletion,
            "with_reason(watchdog_reason(\"run-missing\", receipt))",
        )
        .ok_or_else(|| "expected outer parser expression".to_string())?;
        if outer != "with_reason(watchdog_reason(\"run-missing\", receipt))" {
            return Err(format!("nested shape replaced exact outer call: {outer}"));
        }
        Ok(())
    }

    fn shape_fact(
        kind: ProbeShapeKind,
        start_byte: usize,
        text: impl Into<crate::analysis::facts::SourceText>,
    ) -> ProbeShapeFact {
        let text = text.into();
        let end_byte = start_byte.saturating_add(text.len());
        ProbeShapeFact {
            start_line: 1,
            end_line: 1,
            start_byte: crate::analysis::facts::probe_shape_position(start_byte),
            end_byte: crate::analysis::facts::probe_shape_position(end_byte),
            kind,
            text,
            value_consumed: false,
            source_lines: None,
        }
    }

    fn long_match_line_and_shapes(arms: usize) -> Result<(String, Vec<ProbeShapeFact>), String> {
        let mut inner = String::from("match x {");
        let mut shapes = Vec::with_capacity(arms.saturating_add(4));
        for index in 0..arms {
            let arm = format!("{index}=>{index}");
            let start_byte = inner.len();
            inner.push_str(&arm);
            inner.push(',');
            shapes.push(shape_fact(ProbeShapeKind::MatchArm, start_byte, arm));
        }
        let tail_start = inner.len();
        inner.push_str("_=>0}");
        shapes.push(shape_fact(ProbeShapeKind::MatchArm, tail_start, "_=>0"));
        let prefix = "Id { a: 1, b: 2, x: ";
        let shift = crate::analysis::facts::probe_shape_position(prefix.len());
        for shape in &mut shapes {
            shape.start_byte = shape.start_byte.saturating_add(shift);
            shape.end_byte = shape.end_byte.saturating_add(shift);
        }
        let line = format!("{prefix}{inner} }}");
        let a_at = line
            .find("a: 1")
            .ok_or_else(|| "fixture lost field a: 1".to_string())?;
        let b_at = line
            .find("b: 2")
            .ok_or_else(|| "fixture lost field b: 2".to_string())?;
        shapes.push(shape_fact(ProbeShapeKind::FieldConstruction, a_at, "a: 1"));
        shapes.push(shape_fact(ProbeShapeKind::FieldConstruction, b_at, "b: 2"));
        shapes.push(shape_fact(ProbeShapeKind::MatchArm, 0, line.clone()));
        Ok((line, shapes))
    }

    fn index_with_shapes(path: PathBuf, source: String, shapes: Vec<ProbeShapeFact>) -> RustIndex {
        RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path,
                    source: source.into(),
                    probe_shapes: shapes,
                    ..FileFacts::default()
                },
            )]),
            ..Default::default()
        })
    }

    #[test]
    fn shape_match_rank_matches_legacy_contains_categories() -> Result<(), String> {
        let changed = "  Id { a: 1, b: 2 }; ";
        let shapes = [
            "Id { a: 1, b: 2 }",
            "a: 1",
            "b: 2",
            "Id { a: 1, b: 2, c: 3 }",
            "nope",
            "",
            "  a: 1  ",
        ];
        let ranker = ChangedLineRank::build(changed, shapes.into_iter())
            .ok_or_else(|| "changed text should rank".to_string())?;
        for shape in shapes {
            if ranker.rank(shape) != legacy_shape_match_rank(shape, changed) {
                return Err(format!("rank drifted for {shape:?}"));
            }
        }
        if legacy_shape_match_rank("x", "   ;").is_some() {
            return Err("empty trimmed changed must not rank".to_string());
        }
        if ChangedLineRank::build("   ;", std::iter::empty()).is_some() {
            return Err("empty trimmed changed must not build a ranker".to_string());
        }
        Ok(())
    }

    #[test]
    fn long_changed_line_ranking_is_equivalent_and_scans_by_length_not_shape_count()
    -> Result<(), String> {
        const ARMS: usize = 2_500;
        let (line, shapes) = long_match_line_and_shapes(ARMS)?;
        let ranker = ChangedLineRank::build(&line, shapes.iter().map(|shape| shape.text.as_str()))
            .ok_or_else(|| "changed text should rank".to_string())?;
        for shape in &shapes {
            if ranker.rank(shape.text.as_str())
                != legacy_shape_match_rank(shape.text.as_str(), &line)
            {
                return Err(format!("rank drifted for {}", shape.text.as_str()));
            }
        }
        RANK_WINDOW_BYTES.with(|bytes| bytes.set(0));
        let path = PathBuf::from("src/lib.rs");
        let index = index_with_shapes(path.clone(), line.clone(), shapes.clone());
        let selected = parser_probe_shapes_for_changed_line_against(
            &index,
            &path,
            1,
            &line,
            Some("Id { a: 1, b: 3, x: old }"),
        );
        let window_bytes = RANK_WINDOW_BYTES.with(std::cell::Cell::get);
        let naive_bytes = shapes.len().saturating_mul(line.len());
        if window_bytes == 0 {
            return Err("the length-class scan must examine the changed line".to_string());
        }
        if window_bytes.saturating_mul(20) >= naive_bytes {
            return Err(format!(
                "window scan {window_bytes} bytes is not bounded vs naive {naive_bytes} for {ARMS} shapes on a {}-byte line",
                line.len()
            ));
        }
        let match_arm = selected
            .iter()
            .find(|shape| shape.family == ProbeFamily::MatchArm)
            .ok_or_else(|| "missing match arm".to_string())?;
        if match_arm.text != line.as_str() {
            return Err(format!(
                "exact whole-line shape must win, got {}",
                match_arm.text
            ));
        }
        let fields: Vec<&str> = selected
            .iter()
            .filter(|shape| shape.family == ProbeFamily::FieldConstruction)
            .map(|shape| shape.text)
            .collect();
        if fields != ["b: 2"] {
            return Err(format!(
                "the #6731 unchanged-field tie-break must still pick the edited field: {selected:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn token_run_counts_equal_the_per_needle_count() {
        let haystacks = [
            "Id { a: 1, b: 2, c: 20 }",
            "aaaa aaa a_aa aa",
            "let x = foo(1); // foo(1)\nlet y = \"foo(1)\"; foo(1)",
            "S { flag: foo && bar, flag: foo, flag: foo }",
            "caf\u{e9} = \u{e9}; \u{e9}\u{e9}",
            "a && b &&& c && d &&&& e",
            "\u{e9}a\u{e9}a \u{e9}\u{e9}\u{e9}\u{e9} \u{4e2d}\u{6587}\u{4e2d}\u{6587}",
            "f(xxxxxxxxxxxxxxxxxxxxxxxx); f(xxxxxxxxxxxxxxxxxxxxxxxx) // f(xxxxxxxxxxxxxxxxxxxxxxxx)",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "x ",
            "",
        ];
        let needles = [
            "a: 1",
            "b: 2",
            "c: 2",
            "aa",
            "a",
            "foo(1)",
            "flag: foo",
            "\u{e9}",
            "x",
            "a_aa",
            "aaa",
            "&&",
            "&&&",
            "&&&&",
            "a ",
            "a_",
            "x ",
            "\u{e9}a",
            "\u{e9}\u{e9}",
            "\u{4e2d}\u{6587}",
            "f(xxxxxxxxxxxxxxxxxxxxxxxx)",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ];
        // The mixed set has more length-2 needles than 2, so it exercises the
        // hashed window scan; each needle alone exercises the memmem scan.
        for needle in needles {
            for haystack in haystacks {
                for whole_unit in [false, true] {
                    assert_eq!(
                        TokenRunCounts::build(haystack, &[(needle, whole_unit)])
                            .get(needle, whole_unit),
                        token_run_count(haystack, needle, whole_unit),
                        "alone {needle:?} (whole_unit={whole_unit}) in {haystack:?}"
                    );
                }
            }
        }
        for haystack in haystacks {
            let counted = needles
                .iter()
                .flat_map(|needle| [(*needle, false), (*needle, true)])
                .collect::<Vec<_>>();
            let counts = TokenRunCounts::build(haystack, &counted);
            for needle in needles {
                for whole_unit in [false, true] {
                    assert_eq!(
                        counts.get(needle, whole_unit),
                        token_run_count(haystack, needle, whole_unit),
                        "{needle:?} (whole_unit={whole_unit}) in {haystack:?}"
                    );
                }
            }
        }
        assert_eq!(TokenRunCounts::build("a a", &[("", true)]).get("", true), 0);
    }

    /// The review repro (#9980): one long call shape on a growing line. Hashing
    /// a window per position made this `haystack * needle_len`.
    #[test]
    fn long_needle_work_is_bounded_by_the_haystack_not_its_length() {
        for n in [4_096usize, 8_192, 16_384, 32_768] {
            let present = format!("f(\"{}\")", "a".repeat(n));
            // Same length as `present`, never in either haystack.
            let absent = format!("f(\"{}\")", "z".repeat(n));
            let changed = format!("{present}; g(\"{}\");", "b".repeat(n));
            let removed = changed.replacen('a', "c", 1);
            for (name, haystack) in [("changed", &changed), ("removed", &removed)] {
                for needles in [
                    vec![present.as_str()],
                    vec![absent.as_str()],
                    vec![present.as_str(), absent.as_str()],
                ] {
                    let requested = needles
                        .iter()
                        .flat_map(|needle| [(*needle, false), (*needle, true)])
                        .collect::<Vec<_>>();
                    TOKEN_RUN_WINDOW_BYTES.with(|bytes| bytes.set(0));
                    let counts = TokenRunCounts::build(haystack, &requested);
                    let work = TOKEN_RUN_WINDOW_BYTES.with(std::cell::Cell::get);
                    let unique = needles.iter().map(|needle| needle.len()).sum::<usize>();
                    assert!(
                        work > 0 && work <= 64 * (haystack.len() + unique),
                        "n={n} {name}: work {work} vs budget {}",
                        64 * (haystack.len() + unique)
                    );
                    for needle in &needles {
                        for whole_unit in [false, true] {
                            assert_eq!(
                                counts.get(needle, whole_unit),
                                token_run_count(haystack, needle, whole_unit),
                                "n={n} {name} whole_unit={whole_unit}"
                            );
                        }
                    }
                }
            }
            // The present needle is a real hit in `changed`, not a vacuous zero.
            assert_eq!(
                TokenRunCounts::build(&changed, &[(present.as_str(), false)]).get(&present, false),
                1
            );
            let by_len = BTreeMap::from([(
                present.len(),
                HashSet::from([present.as_str(), absent.as_str()]),
            )]);
            RANK_WINDOW_BYTES.with(|bytes| bytes.set(0));
            let found = needles_in_haystack(&changed, &by_len);
            let rank_work = RANK_WINDOW_BYTES.with(std::cell::Cell::get);
            assert!(found.contains(present.as_str()) && !found.contains(absent.as_str()));
            assert!(rank_work > 0 && rank_work <= 64 * (changed.len() + 2 * present.len()));
        }
    }

    /// Parser-produced facts for two long calls on one line: the complete
    /// selection equals the former per-shape selector built from
    /// `legacy_shape_match_rank` and `token_run_count` (#6731 tie-break,
    /// standalone call resolution included).
    #[test]
    fn parser_produced_two_long_calls_select_like_the_former_selector() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-9980-{stamp}"));
        std::fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='t'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|err| err.to_string())?;
        let n = 2_048;
        let changed = format!(
            "pub fn f(_: &str) {{}} pub fn g(_: &str) {{}} pub fn apply() {{ f(\"{}\"); g(\"{}\"); }}",
            "a".repeat(n),
            "b".repeat(n)
        );
        // Edit the first byte of the first long argument, not the `a` in `apply`.
        let payload_at = changed
            .find("f(\"a")
            .map(|at| at + "f(\"".len())
            .ok_or("fixture lost the first long argument")?;
        let mut removed = changed.clone();
        removed.replace_range(payload_at..=payload_at, "c");
        if removed == changed
            || removed.replace("(\"c", "(\"a") != changed
            || !removed.contains("pub fn apply")
        {
            return Err("removed line must differ from changed only in the payload".to_string());
        }
        let path = PathBuf::from("src/lib.rs");
        std::fs::write(root.join(&path), &changed).map_err(|err| err.to_string())?;
        let index = crate::analysis::facts::build_index(&root, std::slice::from_ref(&path))?;
        let _ = std::fs::remove_dir_all(&root);
        let facts = file_facts(&index, &path).ok_or("no facts")?;
        for removed_text in [Some(removed.as_str()), Some(changed.as_str()), None] {
            let actual = parser_probe_shapes_for_changed_line_against(
                &index,
                &path,
                1,
                &changed,
                removed_text,
            );
            // Former selector: per shape rank and token-run counts.
            let mut expected = Vec::<(ParserProbeShape<'_>, Option<(u8, bool, usize)>)>::new();
            for shape in &facts.probe_shapes {
                let family = family_for_probe_shape(shape.kind);
                if shape.kind == ProbeShapeKind::UnsafeBoundary {
                    continue;
                }
                let Some((category, distance)) =
                    legacy_shape_match_rank(shape.text.as_str(), &changed)
                else {
                    continue;
                };
                let needle = shape.text.trim();
                let whole = family == ProbeFamily::FieldConstruction;
                let unchanged = removed_text.is_some_and(|old| {
                    let before = token_run_count(old, needle, whole);
                    before > 0 && before >= token_run_count(&changed, needle, whole)
                });
                let key = Some((category, unchanged, distance));
                let candidate = ParserProbeShape {
                    family,
                    start_line: shape.start_line(),
                    start_byte: shape.start_byte(),
                    text: shape.text.as_str(),
                    standalone_call: true,
                    unsafe_boundary: false,
                };
                if let Some(position) = expected
                    .iter()
                    .position(|(current, _)| current.family == candidate.family)
                {
                    if key < expected[position].1
                        || (key == expected[position].1
                            && candidate.text < expected[position].0.text)
                    {
                        expected[position] = (candidate, key);
                    }
                } else {
                    expected.push((candidate, key));
                }
            }
            for (shape, _) in &mut expected {
                shape.standalone_call = parser_call_shape_is_standalone(facts, shape);
            }
            expected.sort_by(|(left, _), (right, _)| {
                left.family
                    .as_str()
                    .cmp(right.family.as_str())
                    .then(left.start_byte.cmp(&right.start_byte))
                    .then(left.text.cmp(right.text))
            });
            let expected = expected
                .into_iter()
                .map(|(shape, _)| shape)
                .collect::<Vec<_>>();
            if actual.is_empty() || !actual.iter().any(|shape| shape.text.len() > n) {
                return Err(format!(
                    "fixture selected no long shape: {} shapes",
                    actual.len()
                ));
            }
            let summarize = |shapes: &[ParserProbeShape<'_>]| {
                shapes
                    .iter()
                    .map(|shape| {
                        (
                            shape.family.as_str(),
                            shape.start_line,
                            shape.start_byte,
                            shape.text.to_string(),
                            shape.standalone_call,
                            shape.unsafe_boundary,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            if summarize(&actual) != summarize(&expected) {
                return Err("selection drifted from the former selector".to_string());
            }
        }
        Ok(())
    }

    /// Fixed-size many-shape control: many distinct same-length needles stay
    /// one window scan, not one scan per needle.
    #[test]
    fn many_same_length_needles_stay_one_window_scan() {
        let haystack = (0..2_000)
            .map(|i| format!("v{i:05}: 1,"))
            .collect::<Vec<_>>()
            .join(" ");
        let owned = (0..2_000)
            .map(|i| format!("v{i:05}: 1"))
            .collect::<Vec<_>>();
        let needles = owned
            .iter()
            .map(|n| (n.as_str(), false))
            .collect::<Vec<_>>();
        TOKEN_RUN_WINDOW_BYTES.with(|bytes| bytes.set(0));
        let counts = TokenRunCounts::build(&haystack, &needles);
        let bytes = TOKEN_RUN_WINDOW_BYTES.with(std::cell::Cell::get);
        for needle in &owned {
            assert_eq!(counts.get(needle, false), 1);
        }
        assert!(
            bytes <= haystack.len() * (owned[0].len() + 1),
            "{bytes} bytes for {} needles",
            owned.len()
        );
    }

    #[test]
    fn long_line_unchanged_test_scans_by_length_not_shape_count() -> Result<(), String> {
        const ARMS: usize = 12_000;
        let (line, shapes) = long_match_line_and_shapes(ARMS)?;
        TOKEN_RUN_WINDOW_BYTES.with(|bytes| bytes.set(0));
        let path = PathBuf::from("src/lib.rs");
        let index = index_with_shapes(path.clone(), line.clone(), shapes.clone());
        // Both the removed and the changed line are long, as in a minified or
        // generated file edited in place (#9895).
        let removed = line.replace("b: 2", "b: 3");
        let selected = parser_probe_shapes_for_changed_line_against(
            &index,
            &path,
            1,
            &line,
            Some(removed.as_str()),
        );
        let window_bytes = TOKEN_RUN_WINDOW_BYTES.with(std::cell::Cell::get);
        let naive_bytes = shapes.len().saturating_mul(line.len());
        if window_bytes == 0 {
            return Err("the unchanged test must scan the lines".to_string());
        }
        if window_bytes.saturating_mul(100) >= naive_bytes {
            return Err(format!(
                "token-run scan {window_bytes} bytes is not bounded vs naive {naive_bytes} for {} shapes on a {}-byte line",
                shapes.len(),
                line.len()
            ));
        }
        let fields: Vec<&str> = selected
            .iter()
            .filter(|shape| shape.family == ProbeFamily::FieldConstruction)
            .map(|shape| shape.text)
            .collect();
        if fields != ["b: 2"] {
            return Err(format!(
                "the edited field must still win the #6731 tie-break: {selected:?}"
            ));
        }
        Ok(())
    }

    /// #14722 review: an operator that continues an operand hides the
    /// comparison, and one that ends it (`&&`, `||`, a field `:`, `=>`,
    /// `->`, a range, closure parameters, a comment) does not.
    #[test]
    fn operand_bounded_count_reads_operand_boundaries() {
        let count = |haystack: &str| operand_bounded_token_run_count(haystack, "a > b");
        for extended in [
            "(a > b as u8, c)",
            "(a > b | m, c)",
            "(a > b & m, c)",
            "(a > b ^ m, c)",
            "(a > b << 1, c)",
            "(a > b >> 1, c)",
            "(a > b!(x), c)",
            "(a > b::C, c)",
            "(m | a > b, c)",
            "(m & a > b, c)",
            "(m >> a > b, c)",
            "(m::a > b, c)",
            "(!a > b, c)",
            "(a > b.c, c)",
            "(a > b(c), c)",
            "(a > b[0], c)",
            "(a > b?, c)",
            "(a > b + 1, c)",
            "(a > b - 1, c)",
            "(a > b * 2, c)",
            "(a > b / 2, c)",
            "(a > b % 2, c)",
            "(a > b < c, c)",
            "(m.a > b, c)",
            "(m + a > b, c)",
            "(m - a > b, c)",
            "(m * a > b, c)",
            "(m / a > b, c)",
            "(m % a > b, c)",
            "(m ^ a > b, c)",
            "(m << a > b, c)",
            "(m | n | a > b, c)",
            "(f(x) | a > b, c)",
            "(m, n | k | a > b)",
            "x = m | n | a > b;",
            "{ m | n | a > b }",
            "(remove |x| a > b, c)",
            "(x || y | a > b, c)",
        ] {
            assert_eq!(count(extended), 0, "{extended}");
        }
        for bounded in [
            "(x && a > b, c)",
            "(x || a > b, c)",
            "(a > b && x, c)",
            "(a > b || x, c)",
            "(S { ok: a > b }, c)",
            "x => a > b,",
            "fn f() -> bool { a > b }",
            "(a > b assert, c)",
            "(v.any(|x| a > b), c)",
            "(move |x, y: u8| a > b, c)",
            "let f = |x| a > b;",
            "|x| a > b",
            "(c, |x| a > b)",
            "{ |x| a > b }",
            "(v.any(|x: &Vec<u8>| a > b), c)",
            "(v.any(|[x, _]: [u8; 2]| a > b), c)",
            "(v.any(|x: &'a T| a > b), c)",
            "x => |x| a > b,",
            "return |x| a > b;",
            "(|x| |y| a > b, c)",
            "(x && a > b, c) // note",
            "x && a > b // a / b",
            "(/* why */ a > b, c)",
            "(..a > b, c)",
        ] {
            assert_eq!(count(bounded), 1, "{bounded}");
        }
    }
}
