use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};

use super::{
    ChangedFile, DIFF_FILE_LIMIT_ENV, MAX_DIFF_PATH_COMPONENTS, ParsedDiff, is_new_path_marker,
    parse_old_path_marker, parser_state, path_marker_exceeds_depth_bound,
};

/// How many scoped-out paths the parse keeps by name. The pipeline's
/// disclosure lists only the first few, so the tally stays constant-size even
/// when a vendored tree puts tens of thousands of sections out of scope.
const OUT_OF_SCOPE_SAMPLE: usize = 3;

/// Sections the operator's scope removed during the parse (#9331). Counted but
/// never retained, so they neither trip the file-count bound nor hold memory.
#[derive(Default)]
struct OutOfScopeTally {
    count: usize,
    sample: Vec<PathBuf>,
    last: Option<PathBuf>,
}

impl OutOfScopeTally {
    fn record(&mut self, path: &Path) {
        // A rename's metadata and its `+++` header register the same path
        // back to back; sections are contiguous, so comparing with the last
        // path dedupes without keeping a set that grows with the diff.
        if self.last.as_deref() == Some(path) {
            return;
        }
        self.last = Some(path.to_path_buf());
        self.count = self.count.saturating_add(1);
        if self.sample.len() < OUT_OF_SCOPE_SAMPLE {
            self.sample.push(path.to_path_buf());
        }
    }
}

pub(super) fn parse_unbounded(input: &str) -> ParsedDiff {
    match parse_lines(
        input.lines(),
        |_| true,
        |_| Ok::<(), Infallible>(()),
        |_| Ok::<(), Infallible>(()),
    ) {
        Ok(parsed) => parsed,
        Err(never) => match never {},
    }
}

pub(super) fn parse_bounded_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    limit: usize,
) -> Result<ParsedDiff, String> {
    parse_bounded_lines_scoped(lines, limit, |_| true)
}

/// `keep` is the operator's `--path`/`--exclude` scope: a section it rejects
/// is dropped as soon as its path registers, before the file-count bound
/// sees it, and is tallied on the result so the caller can still disclose it.
pub(super) fn parse_bounded_lines_scoped<'a>(
    lines: impl Iterator<Item = &'a str>,
    limit: usize,
    keep: impl Fn(&Path) -> bool,
) -> Result<ParsedDiff, String> {
    parse_lines(
        lines,
        keep,
        |count| {
            if count <= limit {
                return Ok(());
            }
            Err(format!(
                "diff_scope_oversized: at least {count} changed files exceed the \
                 {DIFF_FILE_LIMIT_ENV} limit ({limit}); parsing stopped before the \
                 remaining file bodies and analysis was not run. Repair route: reduce \
                 the diff scope, split the extraction PR, run a narrower diff, or \
                 raise the limit via {DIFF_FILE_LIMIT_ENV}=<number>."
            ))
        },
        |raw| {
            if !path_marker_exceeds_depth_bound(raw) {
                return Ok(());
            }
            Err(format!(
                "diff_path_too_deep: a diff path header exceeds the \
                 {MAX_DIFF_PATH_COMPONENTS}-component path depth bound; parsing \
                 stopped before the header was resolved and analysis was not run. \
                 Repair route: regenerate the diff from a real checkout, whose \
                 paths are far shallower than this bound."
            ))
        },
    )
}

/// Both entry points use the same grammar and accepted-path map. The admission
/// check runs at the two registration sites, before reading another input line.
/// The unbounded caller has an infallible policy rather than a fallback that
/// could accidentally turn a refused parse into an empty successful result.
fn parse_lines<'a, E>(
    lines: impl Iterator<Item = &'a str>,
    keep: impl Fn(&Path) -> bool,
    mut admit_file_count: impl FnMut(usize) -> Result<(), E>,
    mut admit_line: impl FnMut(&str) -> Result<(), E>,
) -> Result<ParsedDiff, E> {
    let mut files: BTreeMap<PathBuf, ChangedFile> = BTreeMap::new();
    let mut state = parser_state::ParserState::default();
    let mut out_of_scope = OutOfScopeTally::default();
    // One-line lookahead preserves plain ---/+++ section boundaries without
    // collecting the whole input. The input &str itself is already in memory;
    // this file-count policy does not bound its bytes or a single large hunk.
    let mut lines = lines.peekable();
    while let Some(raw) = lines.next() {
        if state.combined_quarantine()
            && parse_old_path_marker(raw)
            && lines.peek().is_some_and(|next| is_new_path_marker(next))
        {
            // Quarantined parent columns remain inert until this grammar
            // recognizes an unprefixed plain file boundary.
            state.close_combined_quarantine();
        }

        if state.in_hunk()
            && state.can_end_at_plain_boundary()
            && parse_old_path_marker(raw)
            && lines.peek().is_some_and(|next| is_new_path_marker(next))
        {
            state.close_hunk();
        }

        // Decide whether ---/+++ is hunk data before path admission. A Git
        // boundary can close even an unfinished hunk and resolves its old
        // path immediately, so it must still be admitted before that work.
        if (!state.in_hunk() && !state.combined_quarantine()) || raw.starts_with("diff --git ") {
            admit_line(raw)?;
        }
        if state.handle_diff_boundary(raw) {
            continue;
        }

        if state.handle_binary_files_sentinel(raw) {
            state.record_binary_deletion(raw);
            continue;
        }

        state.note_symlink_header(raw);

        if state.handle_submodule_mode(raw) {
            continue;
        }

        if state.handle_submodule_index(raw) {
            continue;
        }

        if state.handle_rename_metadata(raw, &mut files) {
            drop_if_out_of_scope(&mut state, &mut files, &keep, &mut out_of_scope);
            admit_file_count(files.len())?;
            continue;
        }

        if state.register_path_marker(raw, &mut files) {
            drop_if_out_of_scope(&mut state, &mut files, &keep, &mut out_of_scope);
            admit_file_count(files.len())?;
            continue;
        }

        if state.handle_hunk_header(raw) {
            continue;
        }

        state.consume_hunk_line(raw, &mut files);
    }

    // End of stream closes a hunk that was still open: lines it promised but
    // never delivered are malformed. #4375: end of stream also closes the
    // final open file section, so a section truncated at EOF counts exactly
    // like one closed by a later boundary.
    state.close_hunk();
    state.close_file_section_accounting();
    Ok(ParsedDiff {
        changed_files: files.into_values().collect(),
        deleted_file_count: state.deleted_file_count(),
        submodule_file_count: state.submodule_file_count(),
        renamed_file_count: state.renamed_file_count(),
        pure_rename_file_count: state.pure_rename_file_count(),
        pure_rename_paths: state.pure_rename_paths(),
        truncated_file_sections: state.truncated_file_sections(),
        raw_line1_bom_paths: state.raw_line1_bom_paths(),
        limitations: state.limitations(),
        out_of_scope_file_count: out_of_scope.count,
        out_of_scope_sample: out_of_scope.sample,
    })
}

/// Remove the section that just registered when the scope rejects its path.
/// Its hunk lines are then consumed without a file to record into, the same
/// inert handling the parser already gives a path it never registered.
fn drop_if_out_of_scope(
    state: &mut parser_state::ParserState,
    files: &mut BTreeMap<PathBuf, ChangedFile>,
    keep: &impl Fn(&Path) -> bool,
    out_of_scope: &mut OutOfScopeTally,
) {
    let Some(path) = state.current_path().cloned() else {
        return;
    };
    if keep(&path) {
        return;
    }
    files.remove(&path);
    out_of_scope.record(&path);
    state.disown_section();
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod path_admission_tests;

#[cfg(test)]
mod scope_tests;
