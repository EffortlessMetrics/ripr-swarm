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

/// Where a card's gap really is, when GitHub could only place the card on a
/// different changed line (#14303). `owner_function_changed_line` and
/// `same_file_changed_line` cards sit on the nearest changed line of the
/// seam's owner, so a reader at that line would otherwise read a missing
/// check for an expression that is not there. Returns the gap's own line and
/// expression from a `comments.json` card; `None` for an exact placement, an
/// unknown mode, or a card that does not name a different line.
pub(crate) fn displaced_seam_location(card: &Value) -> Option<(u64, &str)> {
    let placement = card.get("placement")?;
    let mode = placement.get("mode").and_then(Value::as_str)?;
    if !matches!(
        mode,
        "owner_function_changed_line" | "same_file_changed_line"
    ) {
        return None;
    }
    let placed_line = placement.get("line").and_then(Value::as_u64)?;
    let seam_line = card
        .pointer("/source_location/line")
        .and_then(Value::as_u64)
        .or_else(|| card.pointer("/seam/line").and_then(Value::as_u64))?;
    if seam_line == placed_line || seam_line == 0 {
        return None;
    }
    let expression = card
        .pointer("/seam/expression")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    Some((seam_line, expression))
}

/// The workflow-annotation prefix naming a displaced card's gap line
/// (#14303), for both annotation renderers. The expression is user source;
/// it is workflow-command data only, which the caller's `escape_data`
/// encodes, and it is capped so one long expression cannot crowd out the
/// reason.
pub(crate) fn displaced_seam_prefix(card: &Value) -> Option<String> {
    const EXPRESSION_CHARS: usize = 120;
    let (line, expression) = displaced_seam_location(card)?;
    let expression = if expression.chars().count() > EXPRESSION_CHARS {
        let kept: String = expression.chars().take(EXPRESSION_CHARS).collect();
        format!("{kept}…")
    } else {
        expression.to_string()
    };
    Some(if expression.is_empty() {
        format!("Gap at line {line} of this function: ")
    } else {
        format!("Gap at line {line} (`{expression}`) of this function: ")
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
