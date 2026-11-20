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
use std::collections::BTreeSet;
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
    /// Check-finding ids more than one seam of the whole diff-scoped
    /// inventory can be named by. `None` means the inventory was not
    /// supplied, so no id can be shown unique and id entries fail closed.
    shared_ids: Option<BTreeSet<String>>,
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
            shared_ids: None,
        }
    }

    /// Supplies the ids shared by several seams of the full diff-scoped
    /// inventory (not only the selected or capped subset).
    pub(crate) fn with_inventory_shared_ids(mut self, shared_ids: BTreeSet<String>) -> Self {
        self.shared_ids = Some(shared_ids);
        self
    }

    pub(crate) fn resolve(
        &self,
        root: &Path,
        seams: &[&ClassifiedSeam],
    ) -> ResolvedSeamSuppression {
        let mut resolved = ResolvedSeamSuppression::default();
        if let Some(policy) = &self.policy {
            // A `static_class` narrowing names an exposure class; a review
            // seam carries a grip class from a different vocabulary, so the
            // entry can never be shown to apply. Say so; never apply it
            // unnarrowed.
            let (narrowed, applicable): (Vec<&SuppressionEntry>, Vec<&SuppressionEntry>) = policy
                .entries
                .iter()
                .partition(|entry| entry.static_class.is_some());
            for entry in narrowed {
                resolved.warnings.push(format!(
                    "exposure_gap suppression `{}` uses static_class, which does not apply to review-comments seams; entry not applied",
                    entry_label(entry)
                ));
            }
            let applicable: Vec<SuppressionEntry> = applicable.into_iter().cloned().collect();
            let (candidates, ambiguous) = CheckSuppressionCandidate::for_seams(
                root,
                seams,
                &applicable,
                self.shared_ids.as_ref(),
            );
            resolved.warnings.extend(ambiguous);
            let (matched, warnings) =
                apply_check_suppressions(&candidates, &applicable, &self.today);
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
            let (candidates, ambiguous) = CheckSuppressionCandidate::for_seams(
                root,
                seams,
                &entries,
                self.shared_ids.as_ref(),
            );
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
    /// decided: one per suppressed finding id, bound to that identity alone.
    /// The check run's `suppressed_by` selector is deliberately not widened to
    /// its owner function: that would drop any `static_class` narrowing the
    /// original entry carried and suppress seams the run left blocking.
    fn check_output_entries(&self) -> Vec<SuppressionEntry> {
        self.check_output
            .iter()
            .map(|item| synthetic_entry(&item.finding_id))
            .collect()
    }
}

fn entry_label(entry: &SuppressionEntry) -> String {
    match (&entry.finding_id, &entry.path, &entry.function) {
        (Some(id), _, _) => id.clone(),
        (None, Some(path), Some(function)) => function_selector_label(path, function),
        (None, Some(path), None) => path.clone(),
        (None, None, _) => "unselected".to_string(),
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

fn synthetic_entry(finding_id: &str) -> SuppressionEntry {
    SuppressionEntry {
        kind: SuppressionKind::ExposureGap,
        finding_id: Some(finding_id.to_string()),
        test: None,
        path: None,
        function: None,
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
    };
    use crate::analysis::ClassifiedSeam;
    use crate::analysis::canonical_gap::seam_nameable_ids;
    use crate::analysis::seams::{
        ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind,
    };
    use crate::analysis::test_grip_evidence::TestGripEvidence;
    use crate::domain::{Confidence, StageEvidence, StageState};
    use crate::output::suppressions::{CheckSuppressionCandidate, SuppressionEntry};
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::path::Path;

    fn seam_at(owner: &str, line: usize) -> ClassifiedSeam {
        let seam = RepoSeam::new(
            "src/retry.rs",
            owner,
            SeamKind::PredicateBoundary,
            line * 10,
            line,
            "attempt >= limit",
            RequiredDiscriminator::BoundaryValue {
                description: "attempt == limit".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let stage = || StageEvidence::new(StageState::Yes, Confidence::Medium, "test stage");
        ClassifiedSeam {
            evidence: TestGripEvidence {
                seam_id: seam.id().clone(),
                related_tests: Vec::new(),
                reach: stage(),
                activate: stage(),
                propagate: stage(),
                observe: stage(),
                discriminate: stage(),
                observed_values: Vec::new(),
                missing_discriminators: Vec::new(),
                new_test_target: None,
                statically_contradicted_related_tests: 0,
            },
            seam,
            class: SeamGripClass::WeaklyGripped,
        }
    }

    fn id_entry(id: &str) -> SuppressionEntry {
        let mut entry = super::synthetic_entry(id);
        entry.owner = "team".to_string();
        entry
    }

    fn aliases(candidates: &[CheckSuppressionCandidate]) -> Vec<usize> {
        candidates
            .iter()
            .map(|candidate| candidate.finding_aliases.len())
            .collect()
    }

    const OWNER: &str = "src/retry.rs::retry_allowed";

    #[test]
    fn a_unique_bare_id_binds_to_exactly_one_seam() {
        let only = seam_at(OWNER, 3);
        let other = seam_at("src/retry.rs::backoff_ms", 9);
        let id = seam_nameable_ids(&only)[0].clone();
        let entries = vec![id_entry(&id)];
        let (candidates, warnings) = CheckSuppressionCandidate::for_seams(
            Path::new("."),
            &[&only, &other],
            &entries,
            Some(&BTreeSet::new()),
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(aliases(&candidates), vec![1, 0]);
        assert_eq!(candidates[0].finding_aliases, vec![id]);
    }

    #[test]
    fn an_id_naming_two_selected_seams_warns_and_suppresses_nothing() {
        let first = seam_at(OWNER, 3);
        let second = seam_at(OWNER, 12);
        let id = seam_nameable_ids(&first)[0].clone();
        assert_eq!(id, seam_nameable_ids(&second)[0]);
        let entries = vec![id_entry(&id)];
        let (candidates, warnings) = CheckSuppressionCandidate::for_seams(
            Path::new("."),
            &[&first, &second],
            &entries,
            Some(&BTreeSet::from([id.clone()])),
        );
        assert_eq!(aliases(&candidates), vec![0, 0]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("names 2 review seams"), "{warnings:?}");
    }

    #[test]
    fn an_unselected_sibling_makes_the_id_ambiguous() {
        // Only the second seam is selected; the first sits outside the cap but
        // carries the same bare id, so the entry must not reach the second.
        let first = seam_at(OWNER, 3);
        let second = seam_at(OWNER, 12);
        let id = seam_nameable_ids(&first)[0].clone();
        let entries = vec![id_entry(&id)];
        let (candidates, warnings) = CheckSuppressionCandidate::for_seams(
            Path::new("."),
            &[&second],
            &entries,
            Some(&BTreeSet::from([id.clone()])),
        );
        assert_eq!(aliases(&candidates), vec![0]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("another seam in the diff-scoped inventory"),
            "{warnings:?}"
        );
    }

    #[test]
    fn an_id_is_refused_when_the_full_inventory_is_unavailable() {
        let only = seam_at(OWNER, 3);
        let id = seam_nameable_ids(&only)[0].clone();
        let entries = vec![id_entry(&id)];
        let (candidates, warnings) =
            CheckSuppressionCandidate::for_seams(Path::new("."), &[&only], &entries, None);
        assert_eq!(aliases(&candidates), vec![0]);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("inventory is unavailable")),
            "{warnings:?}"
        );
    }

    #[test]
    fn an_ordinal_id_is_refused_with_a_warning() {
        let only = seam_at(OWNER, 3);
        let id = format!("{}.2", seam_nameable_ids(&only)[0]);
        let entries = vec![id_entry(&id)];
        let (candidates, warnings) = CheckSuppressionCandidate::for_seams(
            Path::new("."),
            &[&only],
            &entries,
            Some(&BTreeSet::new()),
        );
        assert_eq!(aliases(&candidates), vec![0]);
        assert!(
            warnings.iter().any(|w| w.contains("collision ordinal")),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_static_class_entry_is_reported_not_silently_skipped() {
        let only = seam_at(OWNER, 3);
        let mut entry = super::synthetic_entry("unused");
        entry.finding_id = None;
        entry.path = Some("src/retry.rs".to_string());
        entry.function = Some("retry_allowed".to_string());
        entry.static_class = Some("weakly_exposed".to_string());
        entry.owner = "team".to_string();
        let suppression = ReviewCommentsSuppression::with_today(
            Some("policy.toml".to_string()),
            vec![entry],
            Vec::new(),
            "2026-01-01".to_string(),
        );
        let resolved = suppression.resolve(Path::new("."), &[&only]);
        assert!(resolved.by_seam.is_empty());
        assert_eq!(resolved.warnings.len(), 1, "{:?}", resolved.warnings);
        assert!(
            resolved.warnings[0].contains(
                "static_class, which does not apply to review-comments seams; entry not applied"
            ),
            "{:?}",
            resolved.warnings
        );
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
    fn a_check_output_function_label_is_not_widened_to_the_owner() {
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
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].finding_id.as_deref(), Some("probe:a"));
        assert!(entries[0].path.is_none() && entries[0].function.is_none());
    }

    #[test]
    fn check_output_suppresses_only_the_named_finding_in_a_two_seam_function() {
        let kept = seam_at(OWNER, 3);
        let mut suppressed = seam_at(OWNER, 12);
        // Distinct expression so each finding has its own id.
        suppressed.seam = RepoSeam::new(
            "src/retry.rs",
            OWNER,
            SeamKind::PredicateBoundary,
            120,
            12,
            "attempt > limit",
            RequiredDiscriminator::BoundaryValue {
                description: "attempt == limit".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        suppressed.evidence.seam_id = suppressed.seam.id().clone();
        let id = seam_nameable_ids(&suppressed)[0].clone();
        assert_ne!(id, seam_nameable_ids(&kept)[0]);
        let suppression = ReviewCommentsSuppression::with_today(
            None,
            Vec::new(),
            vec![CheckOutputSuppressedFinding {
                finding_id: id,
                selector: "src/retry.rs::retry_allowed".to_string(),
            }],
            "2026-01-01".to_string(),
        )
        .with_inventory_shared_ids(BTreeSet::new());
        let resolved = suppression.resolve(Path::new("."), &[&kept, &suppressed]);
        assert_eq!(
            resolved.by_seam.keys().cloned().collect::<Vec<_>>(),
            vec![suppressed.seam.id().as_str().to_string()],
            "{:?}",
            resolved.warnings
        );
    }
}
