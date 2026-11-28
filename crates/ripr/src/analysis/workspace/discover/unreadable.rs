//! Guard for unreadable directories that hide a changed file.

use super::UnreadableDir;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Refuses the run when an unreadable directory is an ancestor of (or equal
/// to) a changed path in `keep`: the changed file's evidence is hidden, so the
/// strict read error stands instead of a disclosed skip. Unchanged unreadable
/// directories stay tolerated.
pub(crate) fn refuse_unreadable_dir_over_changed_path(
    dirs: &[UnreadableDir],
    keep: &BTreeSet<PathBuf>,
) -> Result<(), String> {
    for (dir, message) in dirs {
        for changed in keep {
            if changed.starts_with(dir) {
                return Err(message.clone());
            }
        }
    }
    Ok(())
}
