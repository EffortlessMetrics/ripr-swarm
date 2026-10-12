use super::ChangedFile;

/// Reconstruct the old source side using the parsed diff's exact coordinates.
/// Refuse mismatched additions or unavailable old positions instead of borrowing
/// candidate-side syntax to classify a removed subject.
pub(crate) fn reconstruct_old_source(new_source: &str, changed: &ChangedFile) -> Option<String> {
    let mut lines = new_source.lines().map(str::to_string).collect::<Vec<_>>();
    let mut added = changed.added_lines.iter().collect::<Vec<_>>();
    added.sort_by_key(|line| std::cmp::Reverse(line.line));
    for line in added {
        let index = line.line.checked_sub(1)?;
        if lines.get(index)? != &line.text {
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
        lines.insert(index, line.text.clone());
    }
    Some(lines.join("\n"))
}
