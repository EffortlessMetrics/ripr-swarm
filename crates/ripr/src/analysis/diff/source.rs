use super::ChangedFile;

/// Reconstruct the old source side using the parsed diff's exact coordinates.
/// Refuse mismatched additions or unavailable old positions instead of borrowing
/// candidate-side syntax to classify a removed subject.
pub(crate) fn reconstruct_old_source(new_source: &str, changed: &ChangedFile) -> Option<String> {
    let lines = reconstruct_old_lines_with_origins(new_source, changed)?;
    Some(
        lines
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// [`reconstruct_old_source`] line by line, each old line tagged with the
/// new-side line number it is unchanged from (`None` for a removed line).
/// A caller maps a new-side span to its old-side counterpart through the
/// tags of its unchanged boundary lines (#14852).
pub(crate) fn reconstruct_old_lines_with_origins(
    new_source: &str,
    changed: &ChangedFile,
) -> Option<Vec<(Option<usize>, String)>> {
    let mut lines = new_source
        .lines()
        .enumerate()
        .map(|(index, text)| (Some(index + 1), text.to_string()))
        .collect::<Vec<_>>();
    let mut added = changed.added_lines.iter().collect::<Vec<_>>();
    added.sort_by_key(|line| std::cmp::Reverse(line.line));
    for line in added {
        let index = line.line.checked_sub(1)?;
        if lines.get(index)?.1 != line.text {
            return None;
        }
        lines.remove(index);
    }
    let mut removed = changed.removed_lines.iter().collect::<Vec<_>>();
    removed.sort_by_key(|line| line.line);
    for line in removed {
        let index = line.line.checked_sub(1)?;
        if index > lines.len() {
            return None;
        }
        lines.insert(index, (None, line.text.clone()));
    }
    Some(lines)
}
