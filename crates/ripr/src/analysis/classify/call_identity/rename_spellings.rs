//! #11738: the per-file text prefilters of the bare-call binding checks,
//! read once per test file instead of once per (owner, test).
//!
//! [`bare_name_renames_another_function`](super::bare_name_renames_another_function)
//! and
//! [`bare_name_bound_only_by_std_globs`](super::bare_name_bound_only_by_std_globs)
//! each start with a whole-file text scan of the test's file. The relation
//! asks them for every candidate test of every owner, so a file holding
//! thousands of tests was rescanned once per test: quadratic in the file's
//! size (8,000 tests in one file spent half of a 14 s run there).
//!
//! The answers depend on the file's source and the owner's name alone.
//! [`FileSpellings::read`] walks the source once and keeps every word for
//! which [`source_spells_rename_to`](super::source_spells_rename_to) answers
//! yes, plus the standard-glob spelling. The per-test question is then a
//! set lookup. The memo holds the exact source it read, so a lookup against
//! a different source never reuses it.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::{STD_ROOTS, head_word, is_ident_char, source_spells_rename_to};

/// What one file's source spells for the two bare-call prefilters.
pub(super) struct FileSpellings {
    /// Every identifier word with an occurrence where
    /// [`source_spells_rename_to`] would answer yes: `as <word>`,
    /// `as r#<word>`, or the word straight after a comment.
    renamed: HashSet<String>,
    /// Whether the source spells a `*` and a `std::`/`core::`/`alloc::`
    /// path, the text every standard-library glob import contains.
    std_glob: bool,
}

#[cfg(test)]
thread_local! {
    /// Test-only: how many sources [`FileSpellings::read`] walked on this
    /// thread, so a test can pin one walk per file.
    pub(super) static FILE_SPELLING_READS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

impl FileSpellings {
    /// One walk over `source`.
    pub(super) fn read(source: &str) -> Self {
        #[cfg(test)]
        FILE_SPELLING_READS.with(|reads| reads.set(reads.get() + 1));
        let lines = LineComments::new(source);
        let mut renamed = HashSet::new();
        let mut chars = source.char_indices().peekable();
        let mut previous_is_ident = false;
        while let Some((start, ch)) = chars.next() {
            if !is_ident_char(ch) {
                previous_is_ident = false;
                continue;
            }
            if previous_is_ident {
                continue;
            }
            previous_is_ident = true;
            // `start` opens a maximal identifier run: the only places a
            // `match_indices` hit of a whole identifier passes both of
            // `source_spells_rename_to`'s boundary checks.
            let mut end = start + ch.len_utf8();
            while let Some(&(at, next)) = chars.peek() {
                if !is_ident_char(next) {
                    break;
                }
                end = at + next.len_utf8();
                chars.next();
            }
            let word = &source[start..end];
            if !renamed.contains(word) && spelled_as_rename_at(source, start, &lines) {
                renamed.insert(word.to_string());
            }
        }
        let std_glob = source.contains('*')
            && STD_ROOTS
                .iter()
                .any(|root| source.contains(&format!("{root}::")));
        Self { renamed, std_glob }
    }

    /// [`source_spells_rename_to`]`(source, name)` for the `source` this
    /// was read from. A name that is not one non-empty identifier word is
    /// answered by the original scan.
    pub(super) fn spells_rename_to(&self, source: &str, name: &str) -> bool {
        if name.is_empty() || !name.chars().all(is_ident_char) {
            return source_spells_rename_to(source, name);
        }
        self.renamed.contains(name)
    }

    /// Whether the source may hold a standard-library glob import.
    pub(super) fn spells_std_glob(&self) -> bool {
        self.std_glob
    }
}

/// `source_spells_rename_to`'s per-hit test for the identifier run opening
/// at `start`, with its "a `//` earlier on the line" read through `lines`
/// instead of rescanning the line.
fn spelled_as_rename_at(source: &str, start: usize, lines: &LineComments) -> bool {
    let head = &source[..start];
    let head = head.strip_suffix("r#").unwrap_or(head);
    if head.chars().next_back().is_some_and(is_ident_char) {
        return false;
    }
    let before = head.trim_end();
    let after_comment = before.ends_with("*/") || lines.line_has_slashes_before(before.len());
    after_comment || (before.len() < head.len() && head_word(before) == "as")
}

/// For each line of a source, where its first `//` starts.
struct LineComments {
    /// Byte offsets of every `\n`, ascending.
    newlines: Vec<usize>,
    /// Per line (`newlines.len() + 1` of them), the offset of its first
    /// `//`, if any.
    first_slashes: Vec<Option<usize>>,
}

impl LineComments {
    fn new(source: &str) -> Self {
        let newlines: Vec<usize> = source.match_indices('\n').map(|(at, _)| at).collect();
        let mut first_slashes = Vec::with_capacity(newlines.len() + 1);
        let mut line_start = 0;
        for line_end in newlines.iter().copied().chain([source.len()]) {
            first_slashes.push(
                source[line_start..line_end]
                    .find("//")
                    .map(|at| line_start + at),
            );
            line_start = line_end + 1;
        }
        Self {
            newlines,
            first_slashes,
        }
    }

    /// Whether `source[..end]`'s last line (`rsplit('\n').next()`) contains
    /// `//`: the line's first `//` ends by `end`.
    fn line_has_slashes_before(&self, end: usize) -> bool {
        let line = self.newlines.partition_point(|&at| at < end);
        self.first_slashes
            .get(line)
            .copied()
            .flatten()
            .is_some_and(|at| at + 2 <= end)
    }
}

/// Per test file, the spellings read from the exact source it was read
/// from.
pub(super) type FileSpellingsMemo = Mutex<BTreeMap<PathBuf, (Arc<str>, Arc<FileSpellings>)>>;

/// The memoized spellings of `file`, whose indexed source is `source`. A
/// memo entry read from another source (another `Arc`) is replaced, never
/// reused; a poisoned lock recomputes without caching.
pub(super) fn file_spellings(
    memo: &FileSpellingsMemo,
    file: &Path,
    source: &Arc<str>,
) -> Arc<FileSpellings> {
    if let Ok(cache) = memo.lock()
        && let Some((read_from, spellings)) = cache.get(file)
        && Arc::ptr_eq(read_from, source)
    {
        return Arc::clone(spellings);
    }
    let spellings = Arc::new(FileSpellings::read(source));
    if let Ok(mut cache) = memo.lock() {
        cache.insert(
            file.to_path_buf(),
            (Arc::clone(source), Arc::clone(&spellings)),
        );
    }
    spellings
}

#[cfg(test)]
mod tests;
