//! #15335: the `Safe next action` line for a run whose only findings are
//! removed lines. Removed code is never an edit target, so nothing is
//! selected for repair. The line still says what the static evidence on the
//! removed lines shows, instead of reading as "nothing to act on".

use crate::domain::{ExposureClass, Finding, SourceCurrentness};

/// The `State:` words for a run where nothing is selected and every omitted
/// finding is a removed line. "no gap selected for repair" read as "nothing
/// to look at"; this names why nothing is selected. The state id
/// (`no_actionable_gap`) is unchanged.
pub(super) const REMOVAL_ONLY_STATE: &str =
    "the diff only removes code, and removed code is never an edit target";

/// True when the omitted set is non-empty and every finding in it is a
/// removed line (`base_deleted`). Moved or renamed code, unresolved
/// currentness, candidate findings and an empty set are not removal-only.
pub(super) fn is_removal_only(omitted: &[&Finding]) -> bool {
    !omitted.is_empty()
        && omitted
            .iter()
            .all(|finding| finding.source_currentness == SourceCurrentness::BaseDeleted)
}

/// The line when every omitted finding is a removed line (`base_deleted`);
/// `None` for any other mix (moved or renamed code, unresolved currentness,
/// candidate findings, or an empty set), which keeps its earlier wording.
pub(super) fn removal_only_safe_action_line(omitted: &[&Finding]) -> Option<String> {
    if !is_removal_only(omitted) {
        return None;
    }
    Some(format!(
        "  Safe next action: the diff removes code, and removed code is never an edit target, so nothing is selected for repair; static evidence on the removed code: {}. Rerun with --format human-full for the evidence on each removed line\n",
        removed_class_counts(omitted)
    ))
}

/// The class counts in the Summary line's plain words, in its order, with
/// zero counts left out.
fn removed_class_counts(omitted: &[&Finding]) -> String {
    let count = |matches: fn(&ExposureClass) -> bool| {
        omitted
            .iter()
            .filter(|finding| matches(&finding.class))
            .count()
    };
    let buckets = [
        (
            count(|class| *class == ExposureClass::Exposed),
            "appear noticed by tests",
        ),
        (
            count(|class| *class == ExposureClass::WeaklyExposed),
            "partly noticed",
        ),
        (
            count(|class| *class == ExposureClass::ReachableUnrevealed),
            "reached but not noticed",
        ),
        (
            count(|class| *class == ExposureClass::NoStaticPath),
            "no test path",
        ),
        (
            count(|class| {
                matches!(
                    class,
                    ExposureClass::InfectionUnknown
                        | ExposureClass::PropagationUnknown
                        | ExposureClass::StaticUnknown
                )
            }),
            "unclear",
        ),
    ];
    buckets
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, words)| format!("{n} {words}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::removal_only_safe_action_line;
    use crate::domain::{ExposureClass, Finding, SourceCurrentness};

    fn removed(class: ExposureClass) -> Finding {
        let mut finding = super::super::tests::test_finding("removed");
        finding.class = class;
        finding.source_currentness = SourceCurrentness::BaseDeleted;
        finding
    }

    #[test]
    fn removed_lines_name_their_static_evidence() {
        let weak = removed(ExposureClass::WeaklyExposed);
        let unclear = removed(ExposureClass::PropagationUnknown);
        let no_path = removed(ExposureClass::NoStaticPath);
        let line = removal_only_safe_action_line(&[&weak, &unclear, &no_path, &unclear]);
        assert_eq!(
            line.as_deref(),
            Some(
                "  Safe next action: the diff removes code, and removed code is never an edit target, so nothing is selected for repair; static evidence on the removed code: 1 partly noticed, 1 no test path, 2 unclear. Rerun with --format human-full for the evidence on each removed line\n"
            )
        );
    }

    #[test]
    fn a_moved_line_keeps_the_earlier_wording() {
        let weak = removed(ExposureClass::WeaklyExposed);
        let mut moved = removed(ExposureClass::NoStaticPath);
        moved.source_currentness = SourceCurrentness::MovedOrRenamed;
        assert_eq!(removal_only_safe_action_line(&[&weak, &moved]), None);
    }

    #[test]
    fn unresolved_or_empty_sets_keep_the_earlier_wording() {
        let mut unresolved: Finding = removed(ExposureClass::Exposed);
        unresolved.source_currentness = SourceCurrentness::UnresolvedSubject;
        assert_eq!(removal_only_safe_action_line(&[&unresolved]), None);
        assert_eq!(removal_only_safe_action_line(&[]), None);
    }
}
