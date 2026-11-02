use super::source_utils::normalized_path;
use super::{LanguageAdapter, PythonAdapter, PythonOwner};
use crate::config::{is_detectable_generated_python_path, is_python_excluded_dir_everywhere};
use std::{
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

pub(super) fn owner_for_changed_line<'a>(
    file: &Path,
    line: usize,
    owners: &'a [PythonOwner],
) -> Option<&'a PythonOwner> {
    let changed_file = normalized_path(file);
    // Allocation-free per owner (#10021): this scan runs once per changed
    // line, and `normalized_path` allocated a String for every workspace
    // owner on every line. The component-wise Path compare covers the
    // common same-spelling case; the normalizing compare stays the
    // fallback for divergent spellings (`\` vs `/`, a leading `./`).
    owners
        .iter()
        .filter(|owner| {
            owner.file.as_path() == file || normalized_path(&owner.file) == changed_file
        })
        .filter(|owner| {
            (line >= owner.start_line && line <= owner.end_line)
                || owner
                    .decorator_lines
                    .is_some_and(|(first, last)| line >= first && line <= last)
        })
        .min_by_key(|owner| (owner.span_width(), owner.specificity_rank()))
}

/// The workspace walk with the `[languages.python] handwritten_files`
/// declarations (#7386): directories are still pruned at the excluded-name
/// boundary, except an ancestor of a declared hand-written file, which stays
/// reachable so the declared file is read and indexed.
#[cfg(test)]
pub(super) fn collect_workspace_python_files_with_handwritten(
    root: &Path,
    handwritten: &[String],
    excluded_file_patterns: &[String],
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    visit_workspace_with_handwritten(
        root,
        root,
        handwritten,
        excluded_file_patterns,
        &mut |file| out.push(file),
    );
    out.sort();
    out
}

/// Streams each admitted file to `sink` in directory order, so a caller
/// that bounds what it keeps never holds the whole walk (#10594).
pub(super) fn visit_workspace_with_handwritten(
    root: &Path,
    dir: &Path,
    handwritten: &[String],
    excluded_file_patterns: &[String],
    sink: &mut dyn FnMut(PathBuf),
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        // Diff discovery prunes the config-owned excluded-directory
        // authority, including `vendor` (#3672): vendored Python is not
        // project or production source, so it never enters the diff-mode
        // working set. A declared hand-written file keeps its ancestor
        // directories reachable instead (#7386).
        if is_python_excluded_dir_everywhere(name)
            && !crate::config::declared_python_handwritten_under(handwritten, &relative)
        {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            visit_workspace_with_handwritten(
                root,
                &path,
                handwritten,
                excluded_file_patterns,
                sink,
            );
        } else if file_type.is_file() {
            let adapter = PythonAdapter;
            if adapter.accepts_path(&path)
                && !is_detectable_generated_python_path(&path)
                && !crate::config::path_matches_preview_excluded_patterns(
                    excluded_file_patterns,
                    &relative,
                )
                && (!crate::config::is_detectable_excluded_python_path(&relative)
                    || crate::config::is_declared_handwritten_python_path(handwritten, &relative))
            {
                sink(relative);
            }
        }
    }
}

pub(super) fn line_is_in_ranges(line: usize, ranges: &[RangeInclusive<usize>]) -> bool {
    ranges.iter().any(|range| range.contains(&line))
}
