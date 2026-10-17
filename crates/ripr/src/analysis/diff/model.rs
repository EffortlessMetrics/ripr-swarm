use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub added_lines: Vec<ChangedLine>,
    pub removed_lines: Vec<ChangedLine>,
}

/// A line that was added or removed in a diff hunk.
///
/// For **added** lines, `line` is the new-file line number and `new_side_line`
/// is always equal to `line`.
///
/// For **removed** lines, `line` is the OLD-file line number (the coordinate
/// recorded by the parser's old-side counter). `new_side_line` is the
/// corresponding position in the NEW file — i.e. the parser's `new_line`
/// counter at the moment the removed line was encountered. When `line ==
/// new_side_line` the two coordinate systems agree (no accumulated net delta
/// above this hunk). When they diverge, using `line` to index into the new
/// file would point at the wrong location.
///
/// Always use `new_side_line` to form a `SourceLocation` that targets the
/// new file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedLine {
    /// Old-side line number for removed lines; new-side line number for added
    /// lines. Use `new_side_line` for any location that must point into the
    /// new (on-disk) file.
    pub line: usize,
    pub text: String,
    /// New-file line number. For added lines this equals `line`. For removed
    /// lines this is the new-side position at the moment the parser saw the
    /// `-` prefix — it may differ from `line` when an earlier hunk shifted the
    /// coordinate systems.
    pub new_side_line: usize,
}

impl ChangedFile {
    /// The removed line this added line replaced, paired by position inside
    /// one replacement block. See [`Self::replacement_counterparts`]; a
    /// caller visiting every added line of a file should build that map once
    /// instead.
    pub(crate) fn replaced_line_counterpart(
        &self,
        added_new_side_line: usize,
    ) -> Option<&ChangedLine> {
        if self.removed_lines.is_empty() {
            return None;
        }
        self.replacement_counterparts()
            .get(&added_new_side_line)
            .copied()
    }

    /// Every added line's positional counterpart, keyed by the added line's
    /// new-side line. The diff parser gives every removed line of a block the
    /// new-side coordinate where the block's added run starts, so the k-th
    /// added line pairs with the k-th removed line when both runs have the
    /// same length. Unequal runs pair nothing, and neither does a run that a
    /// removed line interrupts (`-a -b +x -c +y`): a guessed pairing could
    /// compare an added line against the wrong old line.
    ///
    /// Built in one pass over the file's lines, so a large added block costs
    /// O(n log n), not a scan per line.
    pub(crate) fn replacement_counterparts(&self) -> BTreeMap<usize, &ChangedLine> {
        let mut pairs = BTreeMap::new();
        for run in self.replacement_runs() {
            if run.interrupted || run.removed.len() != run.len() {
                continue;
            }
            for (offset, line) in run.removed.iter().enumerate() {
                pairs.insert(run.start + offset, *line);
            }
        }
        pairs
    }

    /// Every added line's counterpart for a caller that compares an added
    /// line with the old line it replaced, keyed by the added line's
    /// new-side line. An even block pairs by position, as
    /// [`Self::replacement_counterparts`] does. An uneven block pairs by
    /// content (see [`align_by_content`]), so an inserted or deleted line
    /// beside an edit cannot shift the pairing. When content pairs the
    /// run's first added line with nothing, it keeps the same-coordinate
    /// match with the block's first removed line unless another removed
    /// line matches it better: that is the line the coordinate match always
    /// gave it, so a tie never loses a guard that fired before. A run that a
    /// removed line interrupts (`-a -b +x -c +y`) keeps the same-coordinate
    /// match, the first removed line recorded at each added line's own
    /// coordinate.
    pub(crate) fn aligned_counterparts(&self) -> BTreeMap<usize, &ChangedLine> {
        let mut pairs = BTreeMap::new();
        let added_at: BTreeMap<usize, &ChangedLine> = self
            .added_lines
            .iter()
            .map(|line| (line.new_side_line, line))
            .collect();
        for run in self.replacement_runs() {
            if run.interrupted {
                for (new_side_line, removed) in run.removed_by_coordinate {
                    if let Some(first) = removed.first() {
                        pairs.insert(new_side_line, *first);
                    }
                }
            } else if run.removed.len() == run.len() {
                for (offset, line) in run.removed.iter().enumerate() {
                    pairs.insert(run.start + offset, *line);
                }
            } else {
                let added: Vec<&ChangedLine> = (run.start..=run.end)
                    .filter_map(|new_side_line| added_at.get(&new_side_line).copied())
                    .collect();
                for (removed_index, added_index) in align_by_content(&run.removed, &added) {
                    pairs.insert(added[added_index].new_side_line, run.removed[removed_index]);
                }
                if let (Some(first_added), Some(first_removed)) =
                    (added.first(), run.removed.first())
                    && !pairs.contains_key(&first_added.new_side_line)
                    && !pairs
                        .values()
                        .any(|paired| std::ptr::eq(*paired, *first_removed))
                {
                    // Each line is tokenized once here, so a very long added
                    // line beside many removed lines costs one scan of each.
                    let first_key = line_key(first_added);
                    let coordinate_score = key_similarity(&line_key(first_removed), &first_key);
                    let better_elsewhere = run.removed.iter().skip(1).any(|other| {
                        key_similarity(&line_key(other), &first_key) > coordinate_score
                    });
                    if !better_elsewhere {
                        pairs.insert(first_added.new_side_line, *first_removed);
                    }
                }
            }
        }
        pairs
    }

    /// The file's contiguous added runs that have removed lines recorded at
    /// their start. The diff parser gives every removed line of a block the
    /// new-side coordinate where the block's added run starts.
    fn replacement_runs(&self) -> Vec<ReplacementRun<'_>> {
        let mut runs = Vec::new();
        if self.removed_lines.is_empty() {
            return runs;
        }
        let mut removed_at: BTreeMap<usize, Vec<&ChangedLine>> = BTreeMap::new();
        for line in &self.removed_lines {
            removed_at.entry(line.new_side_line).or_default().push(line);
        }
        let added: BTreeSet<usize> = self
            .added_lines
            .iter()
            .map(|line| line.new_side_line)
            .collect();
        let mut lines = added.iter().copied().peekable();
        while let Some(start) = lines.next() {
            let mut end = start;
            while lines.next_if(|&next| next == end + 1).is_some() {
                end += 1;
            }
            // A run that starts with a pure insertion (`+a -b +c`) still
            // holds a replacement: `b` is recorded at `c`'s coordinate. It
            // counts as interrupted, so each added line keeps the removed
            // line recorded at its own coordinate.
            let removed = removed_at.remove(&start).unwrap_or_default();
            let later: Vec<(usize, Vec<&ChangedLine>)> = removed_at
                .range(start + 1..end + 1)
                .map(|(line, removed)| (*line, removed.clone()))
                .collect();
            if removed.is_empty() && later.is_empty() {
                continue;
            }
            let mut removed_by_coordinate = Vec::new();
            if !removed.is_empty() {
                removed_by_coordinate.push((start, removed.clone()));
            }
            let interrupted = !later.is_empty();
            removed_by_coordinate.extend(later);
            runs.push(ReplacementRun {
                start,
                end,
                removed,
                interrupted,
                removed_by_coordinate,
            });
        }
        runs
    }

    /// The new-side line where the contiguous added run holding this line
    /// starts.
    pub(crate) fn added_run_start(&self, added_new_side_line: usize) -> usize {
        let mut run_start = added_new_side_line;
        while run_start > 0
            && self
                .added_lines
                .iter()
                .any(|line| line.new_side_line == run_start - 1)
        {
            run_start -= 1;
        }
        run_start
    }
}

/// One contiguous added run and the removed lines recorded inside it.
struct ReplacementRun<'a> {
    start: usize,
    end: usize,
    /// Removed lines recorded at `start`, in old-file order.
    removed: Vec<&'a ChangedLine>,
    /// A removed line is recorded past `start`, inside the run.
    interrupted: bool,
    /// Removed lines grouped by the coordinate they were recorded at.
    removed_by_coordinate: Vec<(usize, Vec<&'a ChangedLine>)>,
}

impl ReplacementRun<'_> {
    fn len(&self) -> usize {
        self.end - self.start + 1
    }
}

/// Blocks with more line pairs than this pair nothing by content.
const MAX_ALIGNED_CELLS: usize = 65_536;

/// Lines with more code tokens than this, such as a minified bundle line,
/// never pair by content, which bounds the work per pair of lines.
const MAX_ALIGNED_TOKENS: usize = 256;

/// Similarity, in thousandths, at or above which two lines may pair.
const ALIKE_THRESHOLD: u32 = 500;

/// Whether two lines are similar enough to read as one line edited: the
/// same text, or a Dice coefficient of at least one half over their word
/// tokens (identifiers, keywords and numbers). String literals and line
/// comments are left out, so a changed message or a reworded comment does
/// not hide that the code around it is the same.
pub(crate) fn lines_look_alike(old: &str, new: &str) -> bool {
    line_similarity(old, new) >= ALIKE_THRESHOLD
}

fn line_similarity(old: &str, new: &str) -> u32 {
    key_similarity(
        &(old.trim(), code_tokens(old)),
        &(new.trim(), code_tokens(new)),
    )
}

/// Similarity of two lines from their precomputed [`line_key`]s.
fn key_similarity(old: &(&str, Option<Vec<&str>>), new: &(&str, Option<Vec<&str>>)) -> u32 {
    if old.0 == new.0 {
        return 1000;
    }
    match (&old.1, &new.1) {
        (Some(old), Some(new)) => token_similarity(old, new),
        _ => 0,
    }
}

/// Dice coefficient, in thousandths, of two sorted token lists.
fn token_similarity(old: &[&str], new: &[&str]) -> u32 {
    let total = old.len() + new.len();
    if old.is_empty() || new.is_empty() {
        return 0;
    }
    let (mut i, mut j, mut shared) = (0, 0, 0usize);
    while i < old.len() && j < new.len() {
        match old[i].cmp(new[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
        }
    }
    u32::try_from(shared * 2000 / total).unwrap_or(0)
}

/// The sorted word tokens of one line, outside string literals and before
/// any `//` comment, or `None` past [`MAX_ALIGNED_TOKENS`].
fn code_tokens(line: &str) -> Option<Vec<&str>> {
    let mut tokens = Vec::new();
    let mut chars = line.char_indices().peekable();
    let mut token_start = None;
    while let Some((index, ch)) = chars.next() {
        let is_word = ch.is_alphanumeric() || ch == '_' || ch == '$';
        if is_word {
            token_start.get_or_insert(index);
            continue;
        }
        if let Some(start) = token_start.take() {
            tokens.push(&line[start..index]);
            if tokens.len() > MAX_ALIGNED_TOKENS {
                return None;
            }
        }
        if ch == '/' && chars.peek().is_some_and(|(_, next)| *next == '/') {
            break;
        }
        if matches!(ch, '"' | '\'' | '`') {
            let mut escaped = false;
            for (_, inner) in chars.by_ref() {
                if escaped {
                    escaped = false;
                } else if inner == '\\' {
                    escaped = true;
                } else if inner == ch {
                    break;
                }
            }
        }
    }
    if let Some(start) = token_start {
        tokens.push(&line[start..]);
    }
    if tokens.len() > MAX_ALIGNED_TOKENS {
        return None;
    }
    tokens.sort_unstable();
    Some(tokens)
}

/// A line's trimmed text and sorted code tokens, computed once per line.
fn line_key(line: &ChangedLine) -> (&str, Option<Vec<&str>>) {
    (line.text.trim(), code_tokens(&line.text))
}

/// Pairs of (removed index, added index) inside one uneven block. A pair
/// holds two lines that look alike and are each other's single best match
/// among the block's lines; a tie pairs nothing, since choosing would be a
/// guess. Pairs that would cross (a later removed line paired with an
/// earlier added line) are dropped, because a replacement keeps order.
fn align_by_content(removed: &[&ChangedLine], added: &[&ChangedLine]) -> Vec<(usize, usize)> {
    let (rows, cols) = (removed.len(), added.len());
    if rows == 0 || cols == 0 || rows.saturating_mul(cols) > MAX_ALIGNED_CELLS {
        return Vec::new();
    }
    let removed_keys: Vec<_> = removed.iter().map(|line| line_key(line)).collect();
    let added_keys: Vec<_> = added.iter().map(|line| line_key(line)).collect();
    let similarity: Vec<Vec<u32>> = removed_keys
        .iter()
        .map(|old| {
            added_keys
                .iter()
                .map(|new| key_similarity(old, new))
                .collect()
        })
        .collect();
    let single_best = |scores: &mut dyn Iterator<Item = (usize, u32)>| {
        let mut best: Option<(usize, u32)> = None;
        let mut tied = false;
        for (index, score) in scores {
            match best {
                Some((_, top)) if score == top => tied = true,
                Some((_, top)) if score < top => {}
                _ => {
                    best = Some((index, score));
                    tied = false;
                }
            }
        }
        best.filter(|(_, score)| !tied && *score >= ALIKE_THRESHOLD)
            .map(|(index, _)| index)
    };
    let mut pairs = Vec::new();
    for (i, row) in similarity.iter().enumerate() {
        let Some(j) = single_best(&mut row.iter().copied().enumerate()) else {
            continue;
        };
        if single_best(&mut similarity.iter().map(|row| row[j]).enumerate()) == Some(i) {
            pairs.push((i, j));
        }
    }
    let crossing = |&(i, j): &(usize, usize), &(k, l): &(usize, usize)| (i < k) != (j < l);
    pairs
        .iter()
        .filter(|pair| {
            !pairs
                .iter()
                .any(|other| other != *pair && crossing(pair, other))
        })
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_types_are_constructible() {
        let changed_file = ChangedFile {
            path: PathBuf::from("src/lib.rs"),
            added_lines: vec![],
            removed_lines: vec![],
        };
        assert_eq!(changed_file.path, PathBuf::from("src/lib.rs"));
        assert!(changed_file.added_lines.is_empty());
        assert!(changed_file.removed_lines.is_empty());

        let changed_line = ChangedLine {
            line: 42,
            text: "example".to_string(),
            new_side_line: 42,
        };
        assert_eq!(changed_line.line, 42);
        assert_eq!(changed_line.new_side_line, 42);
        assert_eq!(changed_line.text, "example");
    }

    fn line(line: usize, new_side_line: usize, text: &str) -> ChangedLine {
        ChangedLine {
            line,
            text: text.to_string(),
            new_side_line,
        }
    }

    #[test]
    fn even_replacement_block_pairs_lines_by_position() {
        // `-a -b +x +y` at new line 4: both removed lines carry new-side 4.
        let changed = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "x"), line(5, 5, "y")],
            removed_lines: vec![line(4, 4, "a"), line(5, 4, "b")],
        };
        let text = |added| {
            changed
                .replaced_line_counterpart(added)
                .map(|line| line.text.as_str())
        };
        assert_eq!(text(4), Some("a"));
        assert_eq!(text(5), Some("b"));
        assert_eq!(changed.added_run_start(5), 4);
    }

    #[test]
    fn uneven_block_or_pure_insertion_pairs_nothing() {
        let uneven = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "x")],
            removed_lines: vec![line(4, 4, "a"), line(5, 4, "b")],
        };
        assert!(uneven.replaced_line_counterpart(4).is_none());
        let inserted = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "x")],
            removed_lines: Vec::new(),
        };
        assert!(inserted.replaced_line_counterpart(4).is_none());
    }

    #[test]
    fn separate_blocks_pair_within_their_own_run() {
        // `-a +x  context  -b +y`: two one-line blocks.
        let changed = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "x"), line(6, 6, "y")],
            removed_lines: vec![line(4, 4, "a"), line(6, 6, "b")],
        };
        assert_eq!(
            changed
                .replaced_line_counterpart(6)
                .map(|line| line.text.as_str()),
            Some("b")
        );
    }

    #[test]
    fn interleaved_lines_are_one_line_blocks() {
        // `-a +x -b +y`: each removed line carries its own new-side line, so
        // the two-line added run is uneven against either removed line and
        // pairs nothing; callers fall back to the same-coordinate match.
        let changed = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "x"), line(5, 5, "y")],
            removed_lines: vec![line(4, 4, "a"), line(5, 5, "b")],
        };
        assert!(changed.replaced_line_counterpart(4).is_none());
        assert!(changed.replaced_line_counterpart(5).is_none());
    }

    #[test]
    fn a_removed_line_inside_an_added_run_breaks_the_block() {
        // `-a -b +x -c +y`: a and b carry new-side 4, x is 4, c and y are 5.
        // The added run 4..=5 has two lines and two removed lines at its
        // start, but c sits inside it, so y is not b's replacement.
        let changed = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "x"), line(6, 5, "y")],
            removed_lines: vec![line(4, 4, "a"), line(5, 4, "b"), line(6, 5, "c")],
        };
        assert!(changed.replaced_line_counterpart(4).is_none());
        assert!(changed.replaced_line_counterpart(5).is_none());
        assert!(changed.replacement_counterparts().is_empty());
    }

    #[test]
    fn counterpart_map_matches_the_per_line_lookup() {
        // Two blocks separated by context, then an uneven block.
        let changed = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![
                line(4, 4, "x"),
                line(5, 5, "y"),
                line(8, 8, "z"),
                line(12, 12, "p"),
                line(13, 13, "q"),
            ],
            removed_lines: vec![
                line(4, 4, "a"),
                line(5, 4, "b"),
                line(8, 8, "c"),
                line(12, 12, "d"),
            ],
        };
        let map = changed.replacement_counterparts();
        let texts: Vec<(usize, &str)> = map
            .iter()
            .map(|(added, removed)| (*added, removed.text.as_str()))
            .collect();
        assert_eq!(texts, vec![(4, "a"), (5, "b"), (8, "c")]);
        for added in &changed.added_lines {
            assert_eq!(
                changed
                    .replaced_line_counterpart(added.new_side_line)
                    .map(|line| line.text.as_str()),
                map.get(&added.new_side_line).map(|line| line.text.as_str()),
                "line {}",
                added.new_side_line
            );
        }
    }

    #[test]
    fn a_large_block_pairs_in_one_pass() {
        // A new 50,000-line file and a 50,000-line even replacement: a scan
        // per added line would be quadratic or worse here.
        let lines = 50_000;
        let added: Vec<ChangedLine> = (1..=lines).map(|n| line(n, n, "x")).collect();
        let inserted = ChangedFile {
            path: PathBuf::from("src/new.ts"),
            added_lines: added.clone(),
            removed_lines: Vec::new(),
        };
        assert!(inserted.replacement_counterparts().is_empty());
        assert!(inserted.replaced_line_counterpart(lines).is_none());
        let replaced = ChangedFile {
            path: PathBuf::from("src/big.ts"),
            added_lines: added,
            removed_lines: (1..=lines).map(|n| line(n, 1, "old")).collect(),
        };
        let map = replaced.replacement_counterparts();
        assert_eq!(map.len(), lines);
        assert_eq!(map.get(&lines).map(|line| line.line), Some(lines));
    }

    fn aligned_text(changed: &ChangedFile, added: usize) -> Option<&str> {
        changed
            .aligned_counterparts()
            .get(&added)
            .map(|line| line.text.trim())
    }

    #[test]
    fn uneven_block_pairs_lines_by_content() {
        // `-// note -throw old +throw new`: the comment is recorded first,
        // at the added line's coordinate.
        let shorter = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "throw new Error(\"is blank\");")],
            removed_lines: vec![
                line(4, 4, "// refuse an empty label"),
                line(5, 4, "throw new Error(\"must not be blank\");"),
            ],
        };
        assert_eq!(
            aligned_text(&shorter, 4),
            Some("throw new Error(\"must not be blank\");")
        );
        assert!(shorter.replacement_counterparts().is_empty());
        // `-// a -throw old +// b +// c +throw new`: the third added line.
        let longer = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![
                line(4, 4, "// refuse a blank label"),
                line(5, 5, "// whitespace counts"),
                line(6, 6, "throw new Error(\"is blank\");"),
            ],
            removed_lines: vec![
                line(4, 4, "// refuse an empty label"),
                line(5, 4, "throw new Error(\"must not be blank\");"),
            ],
        };
        // Comments carry no code tokens, so content pairs neither comment;
        // the first added line keeps its coordinate match.
        assert_eq!(aligned_text(&longer, 4), Some("// refuse an empty label"));
        assert_eq!(aligned_text(&longer, 5), None);
        assert_eq!(
            aligned_text(&longer, 6),
            Some("throw new Error(\"must not be blank\");")
        );
    }

    #[test]
    fn a_tie_keeps_the_coordinate_match() {
        // Both removed `throw`s match the added one equally: content pairs
        // nothing, and the first added line keeps the first removed line,
        // as the coordinate match always gave it.
        let tied = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "throw new Error(\"c\");")],
            removed_lines: vec![
                line(4, 4, "throw new Error(\"a\");"),
                line(5, 4, "throw new Error(\"b\");"),
            ],
        };
        assert_eq!(tied.aligned_counterparts().len(), 1);
        assert_eq!(aligned_text(&tied, 4), Some("throw new Error(\"a\");"));
    }

    #[test]
    fn a_symmetric_tie_pairs_nothing_by_content() {
        // Two identical removed lines against two equally close added lines
        // (plus one unrelated): every candidate ties, so content pairs
        // nothing and only the first line's coordinate match remains.
        let tied = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![
                line(4, 4, "f(a, 1);"),
                line(5, 5, "f(a, 2);"),
                line(6, 6, "h();"),
            ],
            removed_lines: vec![line(4, 4, "f(a);"), line(5, 4, "f(a);")],
        };
        let map = tied.aligned_counterparts();
        assert_eq!(map.len(), 1, "{map:?}");
        assert_eq!(aligned_text(&tied, 4), Some("f(a);"));
    }

    #[test]
    fn a_run_starting_with_an_insertion_keeps_the_coordinate_match() {
        // `+// note -throw old +throw new`: the removed `throw` is recorded
        // at the added `throw`'s coordinate, inside a run that starts with a
        // pure insertion.
        let changed = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![
                line(4, 4, "// note"),
                line(5, 5, "throw new Error(\"is blank\");"),
            ],
            removed_lines: vec![line(4, 5, "throw new Error(\"must be blank\");")],
        };
        assert_eq!(aligned_text(&changed, 4), None);
        assert_eq!(
            aligned_text(&changed, 5),
            Some("throw new Error(\"must be blank\");")
        );
        assert!(changed.replacement_counterparts().is_empty());
    }

    #[test]
    fn unlike_lines_do_not_pair_by_content() {
        let unlike = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![
                line(4, 4, "log(b, c);"),
                line(5, 5, "return total;"),
                line(6, 6, "y();"),
            ],
            removed_lines: vec![line(4, 4, "const a = 1;"), line(5, 4, "log(b);")],
        };
        assert_eq!(aligned_text(&unlike, 4), Some("log(b);"));
        assert_eq!(aligned_text(&unlike, 5), None);
    }

    #[test]
    fn a_better_match_elsewhere_wins_over_order_and_coordinate() {
        // The first added line matches the second removed line exactly
        // (literals aside) and the first only partly; it pairs with the
        // second, and the coordinate fallback does not override it.
        let changed = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![
                line(4, 4, "if (b) throw new Error(\"B!\");"),
                line(5, 5, "if (b && c) throw new Error(\"B\");"),
                line(6, 6, "x();"),
            ],
            removed_lines: vec![
                line(4, 4, "if (a) throw new Error(\"A\");"),
                line(5, 4, "if (b) throw new Error(\"B\");"),
            ],
        };
        assert_eq!(
            aligned_text(&changed, 4),
            Some("if (b) throw new Error(\"B\");")
        );
        assert_eq!(aligned_text(&changed, 5), None);
        assert_eq!(aligned_text(&changed, 6), None);
    }

    #[test]
    fn crossing_pairs_are_dropped() {
        // `-f(a) -g(b) +g(b, 1) +f(a, 1) +h()`: the best matches cross, so
        // neither pairs; the first added line's coordinate match is refused
        // because `g(b)` matches it better than `f(a)` does.
        let changed = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![
                line(4, 4, "g(b, 1);"),
                line(5, 5, "f(a, 1);"),
                line(6, 6, "h();"),
            ],
            removed_lines: vec![line(4, 4, "f(a);"), line(5, 4, "g(b);")],
        };
        assert!(changed.aligned_counterparts().is_empty());
    }

    #[test]
    fn aligned_pairing_keeps_position_and_coordinate_rules() {
        let even = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "x"), line(5, 5, "y")],
            removed_lines: vec![line(4, 4, "a"), line(5, 4, "b")],
        };
        assert_eq!(aligned_text(&even, 4), Some("a"));
        assert_eq!(aligned_text(&even, 5), Some("b"));
        // `-a -b +x -c +y`: each added line keeps the first removed line at
        // its own coordinate.
        let interleaved = ChangedFile {
            path: PathBuf::from("src/lib.ts"),
            added_lines: vec![line(4, 4, "x"), line(5, 5, "y")],
            removed_lines: vec![line(4, 4, "a"), line(5, 4, "b"), line(6, 5, "c")],
        };
        assert_eq!(aligned_text(&interleaved, 4), Some("a"));
        assert_eq!(aligned_text(&interleaved, 5), Some("c"));
    }

    #[test]
    fn lines_look_alike_ignores_literals_and_comments() {
        assert!(lines_look_alike(
            "throw new Error(\"label must not be blank\");",
            "throw new Error('label is blank'); // reworded"
        ));
        assert!(lines_look_alike(
            "export function oldPrice(a: number): number {",
            "export function newPrice(a: number): number {"
        ));
        assert!(lines_look_alike("}", "}"));
        assert!(!lines_look_alike("}", "};"));
        assert!(!lines_look_alike(
            "// pricing rules",
            "export function helper(a) {"
        ));
        assert!(!lines_look_alike("const a = 1;", "return total;"));
    }

    #[test]
    fn lines_look_alike_accepts_lines_at_the_token_cap() {
        // Each line has 256 tokens, with 255 in common and one changed.
        let shared: String = (0..255).map(|n| format!("a{n}+")).collect();
        let old = format!("{shared}old");
        let new = format!("{shared}fresh");
        assert!(lines_look_alike(&old, &new));
    }

    #[test]
    fn lines_look_alike_rejects_lines_above_the_token_cap() {
        // These distinct 257-token lines would look alike without the token cap.
        let shared: String = (0..256).map(|n| format!("a{n}+")).collect();
        let old = format!("{shared}old");
        let new = format!("{shared}fresh");
        assert!(!lines_look_alike(&old, &new));
    }

    #[test]
    fn content_pairing_stops_at_the_block_size_cap() {
        // `let vN = old;` -> `let vN = fresh;` pairs each line with its own
        // old line (667 against 333 for every other line) below the cap.
        let block = |removed: usize, added: usize| ChangedFile {
            path: PathBuf::from("src/big.ts"),
            added_lines: (0..added)
                .map(|n| line(n + 1, n + 1, &format!("let v{n} = fresh;")))
                .collect(),
            removed_lines: (0..removed)
                .map(|n| line(n + 1, 1, &format!("let v{n} = old;")))
                .collect(),
        };
        let below = block(256, 255);
        let below = below.aligned_counterparts();
        assert_eq!(below.len(), 255, "256 x 255 cells are aligned by content");
        assert_eq!(
            below.get(&200).map(|line| line.text.as_str()),
            Some("let v199 = old;")
        );
        let at_cap = block(257, 255);
        assert_eq!(
            at_cap.aligned_counterparts().len(),
            255,
            "65,535 cells are still aligned"
        );
        let above = block(258, 255);
        assert_eq!(
            above.aligned_counterparts().len(),
            1,
            "65,790 cells exceed the cap; only the coordinate match remains"
        );
    }

    #[test]
    fn a_very_long_line_does_not_pair_by_content() {
        // A minified bundle line rebuilt with a source-map comment added: the
        // lines are over the token cap, so content pairs nothing and the
        // first added line keeps its coordinate match, quickly.
        let bundle: String = (0..50_000).map(|n| format!("a{n}+")).collect();
        let rebuilt: String = (0..50_000).map(|n| format!("b{n}+")).collect();
        let changed = ChangedFile {
            path: PathBuf::from("dist/app.js"),
            added_lines: vec![
                line(1, 1, &rebuilt),
                line(2, 2, "//# sourceMappingURL=app.js.map"),
            ],
            removed_lines: vec![line(1, 1, &bundle)],
        };
        let started = std::time::Instant::now();
        let map = changed.aligned_counterparts();
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(map.len(), 1);
        assert!(!lines_look_alike(&bundle, &rebuilt));
    }

    #[test]
    fn a_long_first_line_beside_many_removed_lines_stays_bounded() {
        // One 1 MB string-literal line against 1,000 removed lines: the
        // block exceeds the cell cap, and the first-line fallback tokenizes
        // each line once.
        let literal = format!("const s = \"{}\";", "x".repeat(1_000_000));
        let changed = ChangedFile {
            path: PathBuf::from("src/big.ts"),
            added_lines: (1..=70)
                .map(|n| line(n, n, if n == 1 { &literal } else { "y();" }))
                .collect(),
            removed_lines: (1..=1_000)
                .map(|n| line(n, 1, &format!("let v{n} = 0;")))
                .collect(),
        };
        let started = std::time::Instant::now();
        let map = changed.aligned_counterparts();
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn code_tokens_handle_unicode_and_escapes() {
        assert!(lines_look_alike(
            "const café = naïve(\"é\\\"\");",
            "const café = naïve('ü');"
        ));
        assert!(!lines_look_alike("const café = 1;", "return naïve;"));
        assert!(lines_look_alike(
            "throw new Error(\"unterminated",
            "throw new Error(`x`);"
        ));
    }
}
