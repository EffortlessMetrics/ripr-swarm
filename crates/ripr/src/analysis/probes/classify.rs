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

pub(super) fn added_type_bound_lines(
    index: &RustIndex,
    changed: &crate::analysis::diff::ChangedFile,
) -> Vec<bool> {
    file_facts(index, &changed.path)
        .filter(|facts| !facts.used_lexical_fallback)
        .map(|facts| declarations::type_bound_lines(&facts.source, &changed.added_lines))
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
    let lines = declarations::type_bound_lines(source, &changed.removed_lines);
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
            (shape.start_line <= line && line <= shape.end_line).then_some(shape.text.as_str())
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
            .filter(|shape| shape.start_line <= line && line <= shape.end_line)
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
    for shape in &facts.probe_shapes {
        if shape.start_line > line || line > shape.end_line {
            continue;
        }
        let family = family_for_probe_shape(shape.kind);
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
                shape.start_line
            },
            start_byte: shape.start_byte,
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

    if line == shape.start_line {
        let prefix_end = boundary_start.max(line_range.start).min(line_range.end);
        let Some(prefix) = facts.source.get(line_range.start..prefix_end) else {
            return false;
        };
        if !boundary_edge_is_empty(prefix) {
            return false;
        }
    }
    if line == shape.end_line {
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
        if u32::from(token.text_range().start()) as usize == shape.start_byte {
            return Some(function.syntax().text_range());
        }
    }
    for block in root.syntax().descendants().filter_map(ast::BlockExpr::cast) {
        let Some(token) = block.unsafe_token() else {
            continue;
        };
        if u32::from(token.text_range().start()) as usize == shape.start_byte {
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
    if word_after("impl") && !text.contains("fn ") {
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
                            start_byte: function_start,
                            end_byte: function_start + 18,
                            kind: ProbeShapeKind::UnsafeBoundary,
                            text: "unsafe fn read_raw".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                        ProbeShapeFact {
                            start_line: 2,
                            end_line: 4,
                            start_byte: block_start,
                            end_byte: block_start + 12,
                            kind: ProbeShapeKind::UnsafeBoundary,
                            text: "unsafe block".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                        ProbeShapeFact {
                            start_line: 3,
                            end_line: 3,
                            start_byte: predicate_start,
                            end_byte: predicate_start + 13,
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
                        start_byte: block_start,
                        end_byte: block_start + 12,
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
                        start_byte: block_start,
                        end_byte: block_start + 12,
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
                            start_byte: first,
                            end_byte: first + 6,
                            kind: ProbeShapeKind::CallDeletion,
                            text: "read()".into(),
                            value_consumed: false,
                            source_lines: None,
                        },
                        ProbeShapeFact {
                            start_line: 3,
                            end_line: 3,
                            start_byte: second,
                            end_byte: second + 6,
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
            start_byte,
            end_byte,
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
        let shift = prefix.len();
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
                    start_line: shape.start_line,
                    start_byte: shape.start_byte,
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
}
