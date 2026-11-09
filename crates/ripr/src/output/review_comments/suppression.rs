//! Policy suppression for diff-scoped review comments (#13895).
//!
//! `ripr check --suppression-policy` marks findings suppressed. Review-comments
//! reaches the gate through seams, so the same decision has to reach seams:
//! this module only joins seams to the shared matcher in
//! [`crate::output::suppressions`] (the single owner of entry parsing,
//! expiry, `finding_id`, `path` glob and `path` + `function` selection). It
//! never decides what a selector means.
//!
//! Suppressed seams move to the document's existing `suppressed` array, which
//! `gate evaluate` already reads as non-blocking. Nothing is hidden: the seam
//! id, location and the matching policy entry stay visible.

use crate::analysis::ClassifiedSeam;
use crate::output::suppressions::{
    CheckSuppressionCandidate, SuppressionEntry, SuppressionKind, apply_check_suppressions,
    current_iso_date, exposure_gap_suppression_entries, function_selector_label,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// The `reason` a policy-suppressed seam carries in the `suppressed` array.
pub(crate) const SUPPRESSED_BY_POLICY_REASON: &str = "suppressed_by_policy";

/// One finding a `ripr check --json` document marks `suppressed: true`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckOutputSuppressedFinding {
    pub(crate) finding_id: String,
    /// The check document's `suppressed_by` selector label.
    pub(crate) selector: String,
}

/// Suppression inputs for one review-comments run: an explicit policy file
/// and/or the suppressions a `--check-output` document already recorded. When
/// both are present the union applies; the policy names the entry first.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReviewCommentsSuppression {
    policy: Option<LoadedPolicy>,
    check_output: Vec<CheckOutputSuppressedFinding>,
    today: String,
}

#[derive(Clone, Debug)]
struct LoadedPolicy {
    path: String,
    entries: Vec<SuppressionEntry>,
}

/// Outcome of resolving suppression against the selected seams.
#[derive(Debug, Default)]
pub(crate) struct ResolvedSeamSuppression {
    /// Seam id to the human message naming the matching entry.
    pub(crate) by_seam: BTreeMap<String, String>,
    /// Expired and ambiguous-selector warnings (never "unmatched" noise: a
    /// repository-wide policy rarely matches every diff).
    pub(crate) warnings: Vec<String>,
}

impl ReviewCommentsSuppression {
    pub(crate) fn new(
        policy_path: Option<String>,
        policy_entries: Vec<SuppressionEntry>,
        check_output: Vec<CheckOutputSuppressedFinding>,
    ) -> Self {
        Self::with_today(
            policy_path,
            policy_entries,
            check_output,
            current_iso_date(),
        )
    }

    pub(crate) fn with_today(
        policy_path: Option<String>,
        policy_entries: Vec<SuppressionEntry>,
        check_output: Vec<CheckOutputSuppressedFinding>,
        today: String,
    ) -> Self {
        Self {
            policy: policy_path.map(|path| LoadedPolicy {
                path,
                entries: exposure_gap_suppression_entries(&policy_entries),
            }),
            check_output,
            today,
        }
    }

    pub(crate) fn resolve(
        &self,
        root: &Path,
        seams: &[&ClassifiedSeam],
    ) -> ResolvedSeamSuppression {
        let mut resolved = ResolvedSeamSuppression::default();
        if let Some(policy) = &self.policy {
            let (candidates, ambiguous) =
                CheckSuppressionCandidate::for_seams(root, seams, &policy.entries);
            resolved.warnings.extend(ambiguous);
            let (matched, warnings) =
                apply_check_suppressions(&candidates, &policy.entries, &self.today);
            resolved
                .warnings
                .extend(warnings.into_iter().filter(|w| w.starts_with("expired ")));
            for (seam_id, selector) in matched {
                resolved.by_seam.insert(
                    seam_id,
                    format!(
                        "Suppressed by suppression policy `{}` entry `{selector}`.",
                        policy.path
                    ),
                );
            }
        }
        if !self.check_output.is_empty() {
            let entries = self.check_output_entries();
            let (candidates, ambiguous) =
                CheckSuppressionCandidate::for_seams(root, seams, &entries);
            resolved.warnings.extend(ambiguous);
            let (matched, _) = apply_check_suppressions(&candidates, &entries, &self.today);
            for (seam_id, selector) in matched {
                resolved.by_seam.entry(seam_id).or_insert_with(|| {
                    format!(
                        "Suppressed in the --check-output document (finding `{selector}` was marked suppressed by the check run's policy)."
                    )
                });
            }
        }
        resolved.warnings.sort();
        resolved.warnings.dedup();
        resolved
    }

    /// Synthetic exposure-gap entries standing for what the check run already
    /// decided: the suppressed finding's own id, plus its owner function when
    /// the check run's `suppressed_by` label is a `path::function` selector
    /// (so every seam of that function is covered, as the policy would).
    fn check_output_entries(&self) -> Vec<SuppressionEntry> {
        let mut entries = Vec::new();
        for item in &self.check_output {
            entries.push(synthetic_entry(Some(&item.finding_id), None, None));
            if let Some((path, function)) = function_label_parts(&item.selector) {
                entries.push(synthetic_entry(None, Some(path), Some(function)));
            }
        }
        entries
    }
}

/// Suppressed findings of a `ripr check --json` document (already validated
/// as a consumable producer envelope by the caller).
pub(crate) fn check_output_suppressed_findings(
    document: &Value,
) -> Vec<CheckOutputSuppressedFinding> {
    document
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| finding.get("suppressed").and_then(Value::as_bool) == Some(true))
        .filter_map(|finding| {
            let finding_id = finding.get("id")?.as_str()?.to_string();
            let selector = finding
                .get("suppressed_by")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some(CheckOutputSuppressedFinding {
                finding_id,
                selector,
            })
        })
        .collect()
}

/// `src/retry.rs::retry_allowed` is a `path` + `function` selector label (see
/// [`function_selector_label`]); a finding id (`probe:...`) or path glob is
/// not. The path part must be a plain file path.
fn function_label_parts(selector: &str) -> Option<(&str, &str)> {
    let (path, function) = selector.split_once("::")?;
    let plain_path = !path.is_empty()
        && !path.contains(['*', '?', '[', ':'])
        && function_selector_label(path, function) == selector;
    (plain_path && !function.trim().is_empty()).then_some((path, function))
}

fn synthetic_entry(
    finding_id: Option<&str>,
    path: Option<&str>,
    function: Option<&str>,
) -> SuppressionEntry {
    SuppressionEntry {
        kind: SuppressionKind::ExposureGap,
        finding_id: finding_id.map(str::to_string),
        test: None,
        path: path.map(str::to_string),
        function: function.map(str::to_string),
        reason: "recorded by the check run".to_string(),
        owner: "check-output".to_string(),
        expires: None,
        scope: None,
        created_at: None,
        last_seen: None,
        review_by: None,
        expected_visibility: None,
        static_class: None,
        language: None,
        language_status: None,
        block_line: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CheckOutputSuppressedFinding, ReviewCommentsSuppression, check_output_suppressed_findings,
        function_label_parts,
    };
    use serde_json::json;

    #[test]
    fn function_labels_are_told_apart_from_ids_and_globs() {
        assert_eq!(
            function_label_parts("src/retry.rs::retry_allowed"),
            Some(("src/retry.rs", "retry_allowed"))
        );
        assert_eq!(
            function_label_parts("src/retry.rs::impl Retry::allowed"),
            Some(("src/retry.rs", "impl Retry::allowed"))
        );
        for not_a_function in [
            "probe:src_retry.rs:predicate:d7b0ead2",
            "repo-probe:src_retry.rs:predicate:d7b0ead2",
            "src/**",
            "src/*.rs::f",
            "src/retry.rs::",
            "",
        ] {
            assert_eq!(
                function_label_parts(not_a_function),
                None,
                "{not_a_function}"
            );
        }
    }

    #[test]
    fn only_findings_marked_suppressed_are_read_from_check_output() {
        let document = json!({"findings": [
            {"id": "probe:a", "suppressed": true, "suppressed_by": "src/a.rs::f"},
            {"id": "probe:b"},
            {"id": "probe:c", "suppressed": false},
            {"suppressed": true, "suppressed_by": "src/**"},
        ]});
        assert_eq!(
            check_output_suppressed_findings(&document),
            vec![CheckOutputSuppressedFinding {
                finding_id: "probe:a".to_string(),
                selector: "src/a.rs::f".to_string(),
            }]
        );
        assert!(check_output_suppressed_findings(&json!({})).is_empty());
    }

    #[test]
    fn a_check_output_function_label_becomes_an_owner_entry() {
        let suppression = ReviewCommentsSuppression::with_today(
            None,
            Vec::new(),
            vec![CheckOutputSuppressedFinding {
                finding_id: "probe:a".to_string(),
                selector: "src/a.rs::f".to_string(),
            }],
            "2026-01-01".to_string(),
        );
        let entries = suppression.check_output_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].finding_id.as_deref(), Some("probe:a"));
        assert_eq!(entries[1].path.as_deref(), Some("src/a.rs"));
        assert_eq!(entries[1].function.as_deref(), Some("f"));
    }
}
