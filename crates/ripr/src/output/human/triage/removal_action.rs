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
    "every finding left is on removed code, which is never an edit target";

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

/// How many removed lines the `Removed code` block lists before it points
/// at `--format human-full` for the rest (the Hidden list is bounded too).
const REMOVED_ROW_LIMIT: usize = 3;

/// #15617: the lines under `Safe next action` for a removal-only run. Each
/// row names one removed line, the code that stood there, and what the
/// static evidence on that old line shows, so the default view answers
/// "what was removed, and do tests appear to check it" without a rerun.
/// `None` for any mix `removal_only_safe_action_line` also declines.
///
/// The rows restate each finding's class in removal words. They add no
/// verdict, select nothing for repair and claim no runtime outcome.
pub(super) fn removal_only_rows(root: &std::path::Path, omitted: &[&Finding]) -> Option<String> {
    removal_only_safe_action_line(omitted)?;
    let mut rows: Vec<String> = Vec::new();
    for finding in omitted {
        let code = super::super::sections::one_line(&finding.probe.expression);
        let subject = if code.is_empty() {
            String::new()
        } else {
            format!(" `{}`", code.replace('`', "'"))
        };
        let row = format!(
            "    - {}:{}{} (kind: {}): {}\n",
            crate::analysis::finding_location_text(root, &finding.probe.location.file),
            finding.probe.location.line,
            subject,
            // The same line can carry two findings; the kind tells them apart.
            super::super::evidence_lines::plain_probe_family(&finding.probe.family),
            removed_class_words(&finding.class)
        );
        if !rows.contains(&row) {
            rows.push(row);
        }
    }
    let mut out = String::from(
        "  Removed code (static evidence on the old lines, not a test result; each location is where the code stood in the current file):\n",
    );
    for row in rows.iter().take(REMOVED_ROW_LIMIT) {
        out.push_str(row);
    }
    if rows.len() > REMOVED_ROW_LIMIT {
        out.push_str(&format!(
            "    - {} more: rerun with --format human-full\n",
            rows.len() - REMOVED_ROW_LIMIT
        ));
    }
    Some(out)
}

/// What a class says about a removed line. The class was computed for the
/// old code against the current tests, so it reads as whether a test
/// appears to check the behavior that is now gone.
fn removed_class_words(class: &ExposureClass) -> &'static str {
    match class {
        ExposureClass::Exposed => "a related test appears to check this behavior",
        ExposureClass::WeaklyExposed => "tests reach it but check it only loosely",
        ExposureClass::ReachableUnrevealed => "tests reach it but none appears to check it",
        ExposureClass::NoStaticPath => "no test path was found",
        ExposureClass::InfectionUnknown
        | ExposureClass::PropagationUnknown
        | ExposureClass::StaticUnknown => {
            "ripr could not tell whether a test checks it (analysis limited, not a verdict)"
        }
    }
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
    use super::{removal_only_rows, removal_only_safe_action_line};
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

    fn removed_at(line: usize, code: &str, class: ExposureClass) -> Finding {
        let mut finding = removed(class);
        finding.probe.location.line = line;
        finding.probe.expression = code.to_string();
        finding
    }

    #[test]
    fn removed_rows_name_the_code_and_what_tests_show() {
        let guard = removed_at(2, "if percent < 0:", ExposureClass::WeaklyExposed);
        let raise = removed_at(
            3,
            "raise ValueError(\"negative\")",
            ExposureClass::ReachableUnrevealed,
        );
        let unclear = removed_at(9, "", ExposureClass::StaticUnknown);
        let rows = removal_only_rows(std::path::Path::new("."), &[&guard, &raise, &unclear]);
        assert_eq!(
            rows.as_deref(),
            Some(
                "  Removed code (static evidence on the old lines, not a test result; each location is where the code stood in the current file):\n    - src/lib.rs:2 `if percent < 0:` (kind: unclear): tests reach it but check it only loosely\n    - src/lib.rs:3 `raise ValueError(\"negative\")` (kind: unclear): tests reach it but none appears to check it\n    - src/lib.rs:9 (kind: unclear): ripr could not tell whether a test checks it (analysis limited, not a verdict)\n"
            )
        );
    }

    #[test]
    fn removed_rows_are_bounded_and_drop_repeats() {
        let findings: Vec<Finding> = (1..=5)
            .map(|line| removed_at(line, "total = 0", ExposureClass::NoStaticPath))
            .collect();
        let mut refs: Vec<&Finding> = findings.iter().collect();
        // The same line, code and class twice is one row.
        refs.extend(findings.first());
        let rows = removal_only_rows(std::path::Path::new("."), &refs).unwrap_or_default();
        assert_eq!(
            rows.matches("`total = 0` (kind: unclear): no test path was found")
                .count(),
            3
        );
        assert!(
            rows.ends_with("    - 2 more: rerun with --format human-full\n"),
            "{rows}"
        );
    }

    #[test]
    fn rows_are_withheld_from_every_mix_that_is_not_removal_only() {
        let weak = removed(ExposureClass::WeaklyExposed);
        let mut moved = removed(ExposureClass::NoStaticPath);
        moved.source_currentness = SourceCurrentness::MovedOrRenamed;
        let mut unresolved = removed(ExposureClass::Exposed);
        unresolved.source_currentness = SourceCurrentness::UnresolvedSubject;
        let root = std::path::Path::new(".");
        assert_eq!(removal_only_rows(root, &[&weak, &moved]), None);
        assert_eq!(removal_only_rows(root, &[&unresolved]), None);
        assert_eq!(removal_only_rows(root, &[]), None);
    }
}
