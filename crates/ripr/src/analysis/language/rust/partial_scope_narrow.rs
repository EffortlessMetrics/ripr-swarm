//! #15484: the route to a complete run when no budget raise can select the
//! whole diff.

use super::PartialDiffScope;

/// The sentence every CLI partial-scope surface adds after its widen
/// instruction when the whole diff cannot be selected in one run.
pub(crate) const NARROW_DIFF_ROUTE: &str = "The whole diff exceeds the current analysis-cost \
    limits, so each raise selects only one more part of it: to analyze every changed file, \
    narrow the diff with a PATH argument (`ripr check <path>...`) or a nearer `--base`";

impl PartialDiffScope {
    /// [`NARROW_DIFF_ROUTE`] when a file was left out and no budget pair
    /// selects the whole diff ([`Self::whole_diff_budgets`] is `None`).
    /// Following the next-file minimums then adds one file per run and never
    /// completes, so the widen advice alone would loop. `None` when the
    /// raise can complete the run, including when every enabled file was
    /// already selected. The route is CLI-shaped, so the editor surface does
    /// not use it.
    pub(crate) fn narrow_diff_route(&self) -> Option<&'static str> {
        (self.whole_diff_budgets.is_none() && self.next_file_changed_lines.is_some())
            .then_some(NARROW_DIFF_ROUTE)
    }
}

#[cfg(test)]
mod tests {
    use super::NARROW_DIFF_ROUTE;
    use crate::analysis::{PartialDiffScope, PartialDiffStopReason};

    fn scope(whole: Option<(usize, usize)>, next: Option<usize>) -> PartialDiffScope {
        PartialDiffScope {
            run_status: PartialDiffScope::RUN_STATUS.to_string(),
            diff_identity: "sha256:abc".to_string(),
            file_budget: 50,
            line_budget: 1_000,
            budget_disclosures: Vec::new(),
            selected_files: vec!["src/a.rs".to_string()],
            unselected_files: vec!["src/b.rs".to_string()],
            selected_changed_lines: 900,
            uninspected_files_lower_bound: 1,
            uninspected_changed_lines_lower_bound: 2_500,
            stop_reason: PartialDiffStopReason::LineBudget,
            next_file_changed_lines: next,
            whole_diff_budgets: whole,
            over_limit_files: Vec::new(),
            line_budget_limits: Vec::new(),
            partition_identity: "c".repeat(64),
        }
    }

    #[test]
    fn a_diff_no_budget_can_select_whole_is_told_to_narrow() {
        assert_eq!(
            scope(None, Some(115)).narrow_diff_route(),
            Some(NARROW_DIFF_ROUTE)
        );
        let disclosure = scope(None, Some(115)).continuation_disclosure();
        assert!(disclosure.contains("nearer `--base`"), "{disclosure}");
    }

    #[test]
    fn a_diff_a_raise_can_complete_keeps_only_the_raise() {
        // The whole diff fits: the raise names the budget that selects it.
        assert_eq!(scope(Some((2, 1_500)), Some(115)).narrow_diff_route(), None);
        // Every enabled file was selected: the line raise completes the run.
        assert_eq!(scope(None, None).narrow_diff_route(), None);
        let disclosure = scope(Some((2, 1_500)), Some(115)).continuation_disclosure();
        assert!(!disclosure.contains("nearer `--base`"), "{disclosure}");
    }
}
