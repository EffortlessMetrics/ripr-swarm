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
    /// one replacement block. The diff parser gives every removed line of a
    /// block the new-side coordinate where the block's added run starts, so
    /// the k-th added line pairs with the k-th removed line when both runs
    /// have the same length. Unequal runs pair nothing: a guessed pairing
    /// could compare an added line against the wrong old line.
    pub(crate) fn replaced_line_counterpart(
        &self,
        added_new_side_line: usize,
    ) -> Option<&ChangedLine> {
        let run_start = self.added_run_start(added_new_side_line);
        let mut run_len = 0usize;
        while self
            .added_lines
            .iter()
            .any(|line| line.new_side_line == run_start + run_len)
        {
            run_len += 1;
        }
        let removed = self
            .removed_lines
            .iter()
            .filter(|line| line.new_side_line == run_start)
            .collect::<Vec<_>>();
        if removed.len() != run_len {
            return None;
        }
        removed
            .get(added_new_side_line.checked_sub(run_start)?)
            .copied()
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
}
