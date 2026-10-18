use super::super::rust_index::{ProbeShapeFact, ProbeShapeKind, RustIndex};
use super::family::family_for_probe_shape;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FileData;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter, parse_clean_source_file};
use crate::domain::ProbeFamily;
use ra_ap_syntax::{AstNode, ast};
use std::collections::{BTreeMap, HashSet};
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

/// Removed declarations must be admitted using reconstructed old-side syntax,
/// never a line coincidence in the candidate index. A refused reconstruction
/// or parse leaves the existing lexical/unknown evidence intact.
pub(super) struct RemovedTypeBounds {
    pub lines: Vec<bool>,
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
        old_index: None,
    };
    if changed.removed_lines.is_empty() {
        return unavailable();
    }
    let Some(source) = old_source else {
        return unavailable();
    };
    let lines = declarations::type_bound_lines(source, &changed.removed_lines);
    if !lines.iter().any(|bound| *bound) {
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
    let selection_key = |family: &ProbeFamily, shape_text: &str| {
        ranker
            .as_ref()
            .and_then(|ranker| ranker.rank(shape_text))
            .map(|(category, distance)| {
                // Unchanged only when the old line holds every occurrence the new
                // line has: with two literals on one line, an edited field whose
                // new text matches the *other* literal's old field is new.
                let needle = shape_text.trim();
                let unchanged = removed_text.is_some_and(|removed| {
                    // Only a record field is bounded by `,`/`}`; other families
                    // (calls, predicates) sit inside larger expressions by design.
                    let whole_unit = *family == ProbeFamily::FieldConstruction;
                    let before = token_run_count(removed, needle, whole_unit);
                    before > 0 && before >= token_run_count(changed_text, needle, whole_unit)
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
            standalone_call: parser_call_shape_is_standalone(facts, shape),
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

fn parser_call_shape_is_standalone(facts: &FileData, shape: &ProbeShapeFact) -> bool {
    if family_for_probe_shape(shape.kind) != ProbeFamily::CallDeletion {
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

/// How often `needle` occurs in `haystack` as code, without an identifier
/// or digit character on either side, so `b: 2` is not found inside `b: 20`
/// and a removed line's trailing `// version: 0x2` comment does not make the
/// edited `version: 0x2` field read as unchanged.
fn token_run_count(haystack: &str, needle: &str, whole_unit: bool) -> usize {
    if needle.is_empty() {
        return 0;
    }
    // The mask keeps byte length, so a match position indexes both strings;
    // an occurrence counts only when it starts on an unmasked code byte.
    let masked = mask_comments_and_strings(haystack);
    let masked = masked.as_bytes();
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    haystack
        .match_indices(needle)
        .filter(|(start, _)| {
            let start = *start;
            let starts_in_code = masked
                .get(start)
                .is_some_and(|byte| !byte.is_ascii_whitespace());
            if !starts_in_code {
                return false;
            }
            let before = haystack[..start].chars().next_back();
            let after = haystack[start + needle.len()..].chars().next();
            let open_start = needle.chars().next().is_none_or(|ch| !is_word(ch))
                || before.is_none_or(|ch| !is_word(ch));
            let open_end = needle.chars().next_back().is_none_or(|ch| !is_word(ch))
                || after.is_none_or(|ch| !is_word(ch));
            // A record field must be a whole syntactic unit, not the head or
            // tail of a longer expression: `flag: foo` inside the old
            // `flag: foo && bar` is not the unchanged field. Rejecting a
            // match only makes a shape read as changed, which falls back to
            // the distance tie-break.
            let next = haystack[start + needle.len()..].trim_start().chars().next();
            let previous = haystack[..start].trim_end().chars().next_back();
            let closes = next.is_none_or(|ch| matches!(ch, ',' | '}' | ')' | ']' | ';' | '{'));
            let opens = previous.is_none_or(|ch| !"&|+-*/%<>=!^.".contains(ch));
            open_start && open_end && (!whole_unit || (closes && opens))
        })
        .count()
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

/// Answer `haystack.contains(needle)` for every needle, in work linear in
/// `haystack.len() * distinct_needle_lengths` rather than `needles * haystack`.
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
        #[cfg(test)]
        RANK_WINDOW_BYTES.with(|bytes| bytes.set(bytes.get().saturating_add(hay_len)));
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
}
