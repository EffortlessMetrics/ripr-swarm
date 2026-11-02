use super::ReviewPlacement;
use crate::agent::loop_commands::display_path;
use crate::app::agent_brief::{AgentBriefResolvedWorkingSet, AgentBriefSelectedSeam};
use serde_json::{Value, json};
use std::cmp::Ordering;
use std::path::Path;

pub(crate) fn placement_for(
    selected: &AgentBriefSelectedSeam<'_>,
    working_set: &AgentBriefResolvedWorkingSet,
) -> Option<ReviewPlacement> {
    let seam = &selected.seam.seam;
    let seam_file = normalize_path_text(seam.file());
    let production_lines = working_set
        .changed_lines
        .iter()
        .filter(|line| !is_test_like_path(&line.file))
        .collect::<Vec<_>>();

    if production_lines.iter().any(|line| {
        normalize_path_text(&line.file) == seam_file && line.line == seam.display_line()
    }) {
        return Some(ReviewPlacement {
            path: seam_file,
            line: seam.display_line(),
            mode: "exact_seam_line",
        });
    }

    let owner_line = working_set
        .changed_owners
        .iter()
        .filter(|owner| normalize_path_text(&owner.file) == seam_file)
        .filter(|owner| owner.owner == seam.owner())
        .filter(|owner| !is_test_like_path(&owner.file))
        .min_by(|left, right| nearest_line_ordering(left.line, right.line, seam.display_line()));
    if let Some(owner) = owner_line {
        return Some(ReviewPlacement {
            path: seam_file,
            line: owner.line,
            mode: "owner_function_changed_line",
        });
    }

    // `same_file_changed_line` (RIPR-SPEC-0012): a changed line inside the
    // seam owner's span that owner attribution bound to a nested function.
    // Sharing a file with the diff is not enough: a seam whose own line and
    // owner span sit outside every hunk has no safe changed-line placement
    // and stays summary-only, which keeps it out of annotations and gate
    // blocking. Unknown owner spans (no enclosing-owner evidence) fail
    // closed the same way.
    working_set
        .enclosing_owners
        .iter()
        .filter(|owner| normalize_path_text(&owner.file) == seam_file)
        .filter(|owner| owner.owner == seam.owner())
        .filter(|owner| !is_test_like_path(&owner.file))
        .filter(|owner| {
            production_lines
                .iter()
                .any(|line| normalize_path_text(&line.file) == seam_file && line.line == owner.line)
        })
        .min_by(|left, right| nearest_line_ordering(left.line, right.line, seam.display_line()))
        .map(|owner| ReviewPlacement {
            path: seam_file,
            line: owner.line,
            mode: "same_file_changed_line",
        })
}

pub(crate) fn placement_json(placement: &ReviewPlacement) -> Value {
    json!({
        "path": placement.path,
        "line": placement.line,
        "side": "RIGHT",
        "mode": placement.mode,
    })
}

pub(super) fn is_test_like_path(path: &Path) -> bool {
    let text = normalize_path_text(path);
    text.starts_with("tests/")
        || text.contains("/tests/")
        || text.ends_with("_test.rs")
        || text.ends_with("_tests.rs")
}

pub(crate) fn normalize_path_text(path: &Path) -> String {
    display_path(path)
        .trim_start_matches("./")
        .replace('\\', "/")
}

pub(super) fn nearest_line_ordering(left: usize, right: usize, target: usize) -> Ordering {
    let left_distance = left.abs_diff(target);
    let right_distance = right.abs_diff(target);
    left_distance
        .cmp(&right_distance)
        .then_with(|| left.cmp(&right))
}
