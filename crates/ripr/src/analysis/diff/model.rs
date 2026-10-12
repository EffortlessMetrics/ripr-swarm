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
        if self.removed_lines.is_empty() {
            return pairs;
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
        while let Some(run_start) = lines.next() {
            let mut run_end = run_start;
            while lines.next_if(|&next| next == run_end + 1).is_some() {
                run_end += 1;
            }
            let Some(removed) = removed_at.get(&run_start) else {
                continue;
            };
            let interrupted = removed_at
                .range(run_start + 1..run_end + 1)
                .next()
                .is_some();
            if interrupted || removed.len() != run_end - run_start + 1 {
                continue;
            }
            for (offset, line) in removed.iter().enumerate() {
                pairs.insert(run_start + offset, *line);
            }
        }
        pairs
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
}
