//! Render the targeted-test before/after outcome receipt.
//!
//! `ripr outcome` compares two previously rendered RIPR static snapshots.
//! Repo-exposure snapshots are matched by seam identity; check-output snapshots
//! can also be matched by canonical gap identity. It does not run analysis or
//! mutation testing; it only reports whether static evidence moved after a
//! focused test change.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) const TARGETED_TEST_OUTCOME_SCHEMA_VERSION: &str = "0.1";
pub(crate) const AGENT_VERIFY_SCHEMA_VERSION: &str = "0.1";

const SEAM_GRIP_CLASS_ORDER: &[&str] = &[
    "strongly_gripped",
    "weakly_gripped",
    "ungripped",
    "reachable_unrevealed",
    "activation_unknown",
    "propagation_unknown",
    "observation_unknown",
    "discrimination_unknown",
    "opaque",
    "intentional",
    "suppressed",
];

const EVIDENCE_STAGES: &[&str] = &["reach", "activate", "propagate", "observe", "discriminate"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StaticSeamRecord {
    seam_id: String,
    seam_kind: String,
    file: String,
    line: usize,
    seam_grip_class: String,
    oracle_kind: String,
    oracle_strength: String,
    observed_values: Vec<String>,
    missing_discriminators: Vec<String>,
    evidence_source: String,
    evidence_path: BTreeMap<String, StaticEvidenceStage>,
    related_tests_total: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StaticEvidenceStage {
    state: String,
    confidence: String,
    summary: String,
}

struct TargetedOutcomeEvidenceDelta<'a> {
    stage_deltas: [&'a Option<TargetedTestOutcomeStageDelta>; 5],
    observed_values_added: &'a [String],
    observed_values_removed: &'a [String],
    missing_discriminators_resolved: &'a [String],
    missing_discriminators_reopened: &'a [String],
    oracle_strength_delta: Option<&'a str>,
    related_test_delta: isize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TargetedTestOutcomeReport {
    before_path: String,
    after_path: String,
    before_counts: BTreeMap<String, usize>,
    after_counts: BTreeMap<String, usize>,
    moved: Vec<TargetedTestOutcomeMovement>,
    unchanged: Vec<TargetedTestOutcomeMovement>,
    regressed: Vec<TargetedTestOutcomeMovement>,
    new: Vec<TargetedTestOutcomeSeam>,
    removed: Vec<TargetedTestOutcomeSeam>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TargetedTestOutcomeMovement {
    seam_id: String,
    seam_kind: String,
    file: String,
    line: usize,
    before: String,
    after: String,
    direction: String,
    gap_movement: String,
    evidence_delta: Vec<String>,
    evidence_source: String,
    reach_delta: Option<TargetedTestOutcomeStageDelta>,
    activate_delta: Option<TargetedTestOutcomeStageDelta>,
    propagate_delta: Option<TargetedTestOutcomeStageDelta>,
    observe_delta: Option<TargetedTestOutcomeStageDelta>,
    discriminate_delta: Option<TargetedTestOutcomeStageDelta>,
    observed_values_added: Vec<String>,
    observed_values_removed: Vec<String>,
    missing_discriminators_resolved: Vec<String>,
    missing_discriminators_reopened: Vec<String>,
    oracle_strength_delta: Option<String>,
    related_test_delta: isize,
    no_movement_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TargetedTestOutcomeStageDelta {
    before_state: Option<String>,
    after_state: Option<String>,
    before_confidence: Option<String>,
    after_confidence: Option<String>,
    before_summary: Option<String>,
    after_summary: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TargetedTestOutcomeSeam {
    seam_id: String,
    seam_kind: String,
    file: String,
    line: usize,
    grip_class: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct TargetedTestOutcomeGapSummary {
    closed: usize,
    opened: usize,
    strengthened: usize,
    weakened: usize,
    unchanged: usize,
    new: usize,
    removed: usize,
    changed: usize,
}
mod build;
mod parse;
mod render_json;
mod render_md;

use build::build_targeted_test_outcome_report;
#[cfg(test)]
use build::targeted_test_outcome_movement;
use parse::parse_repo_exposure_static_seams;

pub(crate) use render_json::{render_agent_verify_json, render_targeted_test_outcome_json};
pub(crate) use render_md::render_targeted_test_outcome_md;

pub(crate) fn targeted_test_outcome_report_from_json(
    before_json: &str,
    after_json: &str,
    before_path: String,
    after_path: String,
) -> Result<TargetedTestOutcomeReport, String> {
    let before = parse_repo_exposure_static_seams(before_json)?;
    let after = parse_repo_exposure_static_seams(after_json)?;
    build_targeted_test_outcome_report(&before, &after, before_path, after_path)
}

pub(crate) fn display_path(path: &Path) -> String {
    normalize_report_path(&path.display().to_string())
}

fn review_attention_class(class: &str) -> bool {
    matches!(
        class,
        "weakly_gripped"
            | "ungripped"
            | "reachable_unrevealed"
            | "activation_unknown"
            | "propagation_unknown"
            | "observation_unknown"
            | "discrimination_unknown"
            | "opaque"
    )
}

fn normalize_report_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    match normalized.strip_prefix("./") {
        Some(stripped) => stripped.to_string(),
        None => normalized,
    }
}

fn md_escape(value: &str) -> String {
    value.replace('`', "\\`").replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targeted_test_outcome_report_buckets_seam_movement() -> Result<(), String> {
        let mut before_moved = targeted_static_seam("seam-moved", "weakly_gripped");
        before_moved.missing_discriminators = vec!["threshold equality".to_string()];
        before_moved.oracle_strength = "weak".to_string();
        let before = vec![
            before_moved,
            targeted_static_seam("seam-regressed", "weakly_gripped"),
            targeted_static_seam("seam-same", "strongly_gripped"),
            targeted_static_seam("seam-removed", "ungripped"),
        ];

        let mut after_moved = targeted_static_seam("seam-moved", "strongly_gripped");
        after_moved.observed_values = vec!["50".to_string(), "100".to_string()];
        after_moved.oracle_strength = "strong".to_string();
        let after = vec![
            after_moved,
            targeted_static_seam("seam-regressed", "ungripped"),
            targeted_static_seam("seam-same", "strongly_gripped"),
            targeted_static_seam("seam-new", "weakly_gripped"),
        ];

        let report = build_targeted_test_outcome_report(
            &before,
            &after,
            "before.json".to_string(),
            "after.json".to_string(),
        )?;
        assert_eq!(report.moved.len(), 1);
        assert_eq!(report.moved[0].seam_id, "seam-moved");
        assert_eq!(report.moved[0].direction, "improved");
        assert!(
            report.moved[0]
                .evidence_delta
                .iter()
                .any(|delta| delta.contains("missing discriminator no longer reported"))
        );
        assert!(
            report.moved[0]
                .evidence_delta
                .iter()
                .any(|delta| delta.contains("stronger related oracle visible"))
        );
        assert_eq!(report.regressed.len(), 1);
        assert_eq!(report.unchanged.len(), 1);
        assert_eq!(report.new.len(), 1);
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.before_counts.get("weakly_gripped"), Some(&2));
        assert_eq!(report.after_counts.get("strongly_gripped"), Some(&2));
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_json_and_markdown_are_structured() -> Result<(), String> {
        let before = vec![
            targeted_static_seam("seam-a", "weakly_gripped"),
            targeted_static_seam("seam-same", "weakly_gripped"),
        ];
        let mut after_same = targeted_static_seam("seam-same", "weakly_gripped");
        after_same.observed_values = vec!["50".to_string(), "100".to_string()];
        let after = vec![
            targeted_static_seam("seam-a", "strongly_gripped"),
            after_same,
        ];
        let report = build_targeted_test_outcome_report(
            &before,
            &after,
            "target/ripr/before.json".to_string(),
            "target/ripr/after.json".to_string(),
        )?;

        let json = render_targeted_test_outcome_json(&report)?;
        let value: Value = serde_json::from_str(&json)
            .map_err(|err| format!("targeted-test outcome JSON should parse: {err}"))?;
        assert_eq!(
            value["schema_version"],
            TARGETED_TEST_OUTCOME_SCHEMA_VERSION
        );
        assert_eq!(value["status"], "advisory");
        assert_eq!(value["summary"]["moved"], 1);
        assert_eq!(value["summary"]["gap_movement"]["closed"], 1);
        assert_eq!(value["summary"]["gap_movement"]["unchanged"], 1);
        assert_eq!(
            value["review_receipt"]["movement_after_verification"][0],
            "1 improved, 0 changed without ranking higher, 0 regressed, 1 unchanged."
        );
        assert_eq!(value["review_receipt"]["gap_movement"]["closed"], 1);
        assert_eq!(
            value["review_receipt"]["movement_after_verification"][1],
            "Gap movement: 1 closed, 0 opened, 0 strengthened, 0 weakened, 1 unchanged, 0 new, 0 removed, 0 changed."
        );
        assert!(
            value["review_receipt"]["focused_proof_added"][0]
                .as_str()
                .is_some_and(|text| text.contains("outside RIPR")
                    && text.contains("new observed value: 100"))
        );
        assert!(
            value["review_receipt"]["reviewer_may_believe"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item
                    .as_str()
                    .is_some_and(|text| text.contains("static claim boundary"))))
        );
        assert!(
            value["review_receipt"]["reviewer_should_not_believe"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item == "Merge approval."))
        );

        let markdown = render_targeted_test_outcome_md(&report);
        assert!(markdown.contains("# ripr targeted-test outcome report"));
        assert!(markdown.contains("| moved | 1 |"));
        assert!(markdown.contains("## Gap Movement"));
        assert!(markdown.contains("| closed | 1 |"));
        assert!(markdown.contains("| unchanged | 1 |"));
        assert!(markdown.contains("## Unchanged"));
        assert!(markdown.contains("seam-same"));
        assert!(markdown.contains("new observed value: 100"));
        assert!(markdown.contains("## Review Receipt"));
        assert!(markdown.contains("### What focused proof changed?"));
        assert!(markdown.contains("### Reviewer may believe"));
        assert!(markdown.contains("test or output proof changed outside RIPR"));
        assert!(markdown.contains("### Reviewer should not believe"));
        assert!(markdown.contains("weakly_gripped -> strongly_gripped"));
        Ok(())
    }

    #[test]
    fn agent_verify_json_maps_outcome_to_agent_status_buckets() -> Result<(), String> {
        let before = vec![
            targeted_static_seam("improved", "weakly_gripped"),
            targeted_static_seam("regressed", "weakly_gripped"),
            targeted_static_seam("unchanged", "weakly_gripped"),
            targeted_static_seam("resolved", "ungripped"),
        ];
        let after = vec![
            targeted_static_seam("improved", "strongly_gripped"),
            targeted_static_seam("regressed", "ungripped"),
            targeted_static_seam("unchanged", "weakly_gripped"),
            targeted_static_seam("new", "weakly_gripped"),
        ];
        let report = build_targeted_test_outcome_report(
            &before,
            &after,
            "before.json".to_string(),
            "after.json".to_string(),
        )?;

        let json = render_agent_verify_json(&report)?;
        let value: Value = serde_json::from_str(&json)
            .map_err(|err| format!("agent verify JSON should parse: {err}"))?;
        assert_eq!(value["schema_version"], AGENT_VERIFY_SCHEMA_VERSION);
        assert_eq!(value["status"], "advisory");
        assert_eq!(value["summary"]["improved"], 1);
        assert_eq!(value["summary"]["regressed"], 1);
        assert_eq!(value["summary"]["unchanged"], 1);
        assert_eq!(value["summary"]["new"], 1);
        assert_eq!(value["summary"]["resolved"], 1);
        assert_eq!(value["summary"]["gap_movement"]["closed"], 1);
        assert_eq!(value["summary"]["gap_movement"]["weakened"], 1);
        assert_eq!(value["summary"]["gap_movement"]["unchanged"], 1);
        assert_eq!(value["summary"]["gap_movement"]["new"], 1);
        assert_eq!(value["summary"]["gap_movement"]["removed"], 1);
        assert_eq!(value["changed_seams"][0]["change"], "improved");
        assert_eq!(value["resolved_gaps"][0]["change"], "resolved");
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_from_repo_exposure_json_parses_static_evidence() -> Result<(), String>
    {
        let before = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "predicate_boundary",
      "file": ".\\src\\pricing.rs",
      "line": 42,
      "grip_class": "weakly_gripped",
      "related_tests": [
        {"oracle_kind": "exact_value", "oracle_strength": "weak"}
      ],
      "observed_values": ["50"],
      "missing_discriminators": [
        {"value": "threshold equality", "reason": "not observed"}
      ]
    }
  ]
}"#;
        let after = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "predicate_boundary",
      "file": "src/pricing.rs",
      "line": 42,
      "grip_class": "strongly_gripped",
      "related_tests": [
        {"oracle_kind": "exact_value", "oracle_strength": "strong"}
      ],
      "observed_values": ["50", "100"],
      "missing_discriminators": []
    }
  ]
}"#;
        let report = targeted_test_outcome_report_from_json(
            before,
            after,
            "before.json".to_string(),
            "after.json".to_string(),
        )?;
        assert_eq!(report.moved.len(), 1);
        assert_eq!(report.moved[0].file, "src/pricing.rs");
        assert!(
            report.moved[0]
                .evidence_delta
                .iter()
                .any(|delta| delta.contains("threshold equality"))
        );
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_from_python_check_json_matches_canonical_gap_ids() -> Result<(), String>
    {
        let before = r#"{
  "schema_version": "0.1",
  "tool": "ripr",
  "findings": [
    {
      "id": "probe:src_discount.py:2:python_preview",
      "canonical_gap_id": "gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold",
      "canonical_gap": {
        "id": "gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold",
        "language": "python",
        "file": "src/discount.py",
        "owner": "apply_discount",
        "behavior_kind": "predicate_boundary"
      },
      "classification": "weakly_exposed",
      "probe": {"family": "predicate", "file": "src/discount.py", "line": 2},
      "ripr": {
        "reach": {"state": "yes", "confidence": "low", "summary": "related test reaches owner"},
        "infect": {"state": "yes", "confidence": "low", "summary": "predicate can alter branch"},
        "propagate": {"state": "weak", "confidence": "low", "summary": "branch can propagate"},
        "observe": {"state": "weak", "confidence": "low", "summary": "smoke assertion only"},
        "discriminate": {"state": "weak", "confidence": "low", "summary": "boundary not asserted"}
      },
      "missing_discriminators": [
        {"value": "amount == threshold", "reason": "not observed"}
      ],
      "related_tests": [
        {"name": "test_apply_discount_smoke", "file": "tests/test_discount.py", "line": 4, "oracle_strength": "unknown", "oracle_kind": "unknown"}
      ],
      "language": "python",
      "language_status": "preview"
    }
  ]
}"#;
        let after = r#"{
  "schema_version": "0.1",
  "tool": "ripr",
  "findings": [
    {
      "id": "probe:src_discount.py:2:python_preview",
      "canonical_gap_id": "gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold",
      "canonical_gap": {
        "id": "gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold",
        "language": "python",
        "file": "src/discount.py",
        "owner": "apply_discount",
        "behavior_kind": "predicate_boundary"
      },
      "classification": "exposed",
      "probe": {"family": "predicate", "file": "src/discount.py", "line": 2},
      "ripr": {
        "reach": {"state": "yes", "confidence": "low", "summary": "related test reaches owner"},
        "infect": {"state": "yes", "confidence": "low", "summary": "predicate can alter branch"},
        "propagate": {"state": "weak", "confidence": "low", "summary": "branch can propagate"},
        "observe": {"state": "yes", "confidence": "low", "summary": "exact assertion"},
        "discriminate": {"state": "yes", "confidence": "low", "summary": "boundary asserted"}
      },
      "missing_discriminators": [],
      "related_tests": [
        {"name": "test_apply_discount_boundary", "file": "tests/test_discount.py", "line": 4, "oracle_strength": "strong", "oracle_kind": "exact_value", "oracle": "assert apply_discount(100, 100) == 90"}
      ],
      "language": "python",
      "language_status": "preview"
    }
  ]
}"#;

        let report = targeted_test_outcome_report_from_json(
            before,
            after,
            "before-check.json".to_string(),
            "after-check.json".to_string(),
        )?;

        assert_eq!(report.moved.len(), 1);
        let movement = &report.moved[0];
        assert_eq!(
            movement.seam_id,
            "gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold"
        );
        assert_eq!(movement.seam_kind, "predicate_boundary");
        assert_eq!(movement.file, "src/discount.py");
        assert_eq!(movement.before, "weakly_gripped");
        assert_eq!(movement.after, "strongly_gripped");
        assert_eq!(movement.direction, "improved");
        assert_eq!(movement.gap_movement, "closed");
        assert_eq!(movement.evidence_source, "check_output_finding");
        assert_eq!(
            movement.missing_discriminators_resolved,
            vec!["amount == threshold (not observed)".to_string()]
        );
        assert_eq!(
            movement.oracle_strength_delta,
            Some("unknown -> strong".to_string())
        );
        assert_eq!(
            movement
                .discriminate_delta
                .as_ref()
                .and_then(|delta| delta.before_state.as_deref()),
            Some("weak")
        );
        assert_eq!(
            movement
                .discriminate_delta
                .as_ref()
                .and_then(|delta| delta.after_state.as_deref()),
            Some("yes")
        );

        let receipt_json = render_targeted_test_outcome_json(&report)?;
        let receipt: Value = serde_json::from_str(&receipt_json)
            .map_err(|err| format!("targeted-test outcome JSON should parse: {err}"))?;
        assert_eq!(receipt["summary"]["gap_movement"]["closed"], 1);
        assert_eq!(receipt["moved"][0]["gap_movement"], "closed");
        assert_eq!(
            receipt["moved"][0]["evidence_source"],
            "check_output_finding"
        );

        let verify_json = render_agent_verify_json(&report)?;
        let verify: Value = serde_json::from_str(&verify_json)
            .map_err(|err| format!("agent verify JSON should parse: {err}"))?;
        assert_eq!(verify["summary"]["gap_movement"]["closed"], 1);
        assert_eq!(verify["changed_seams"][0]["change"], "improved");
        assert_eq!(verify["changed_seams"][0]["gap_movement"], "closed");
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_python_preview_fixture_matches_expected_receipts() -> Result<(), String>
    {
        let weak = include_str!(
            "../../../../fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json"
        );
        let strong = include_str!(
            "../../../../fixtures/first_successful_pr/python-preview-gap/inputs/reports/after-check.json"
        );
        let no_path = include_str!(
            "../../../../fixtures/first_successful_pr/python-preview-gap/inputs/reports/no-path-check.json"
        );
        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: weak,
            after: strong,
            before_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json",
            after_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/after-check.json",
            expected_gap_movement: "closed",
            expected_bucket: "moved",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/closed.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/closed.md"
            ),
        })?;

        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: weak,
            after: weak,
            before_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json",
            after_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json",
            expected_gap_movement: "unchanged",
            expected_bucket: "unchanged",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/unchanged.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/unchanged.md"
            ),
        })?;

        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: strong,
            after: weak,
            before_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/after-check.json",
            after_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json",
            expected_gap_movement: "opened",
            expected_bucket: "regressed",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/opened.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/opened.md"
            ),
        })?;

        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: no_path,
            after: weak,
            before_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/no-path-check.json",
            after_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json",
            expected_gap_movement: "strengthened",
            expected_bucket: "moved",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/strengthened.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/strengthened.md"
            ),
        })?;

        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: weak,
            after: no_path,
            before_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json",
            after_path: "fixtures/first_successful_pr/python-preview-gap/inputs/reports/no-path-check.json",
            expected_gap_movement: "weakened",
            expected_bucket: "regressed",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/weakened.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-preview-gap/expected/outcome/weakened.md"
            ),
        })?;
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_python_return_value_fixture_matches_expected_receipts()
    -> Result<(), String> {
        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: include_str!(
                "../../../../fixtures/first_successful_pr/python-return-gap/inputs/reports/before-check.json"
            ),
            after: include_str!(
                "../../../../fixtures/first_successful_pr/python-return-gap/inputs/reports/after-check.json"
            ),
            before_path: "fixtures/first_successful_pr/python-return-gap/inputs/reports/before-check.json",
            after_path: "fixtures/first_successful_pr/python-return-gap/inputs/reports/after-check.json",
            expected_gap_movement: "closed",
            expected_bucket: "moved",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-return-gap/expected/outcome/closed.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-return-gap/expected/outcome/closed.md"
            ),
        })?;
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_python_exception_fixture_matches_expected_receipts()
    -> Result<(), String> {
        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: include_str!(
                "../../../../fixtures/first_successful_pr/python-exception-gap/inputs/reports/before-check.json"
            ),
            after: include_str!(
                "../../../../fixtures/first_successful_pr/python-exception-gap/inputs/reports/after-check.json"
            ),
            before_path: "fixtures/first_successful_pr/python-exception-gap/inputs/reports/before-check.json",
            after_path: "fixtures/first_successful_pr/python-exception-gap/inputs/reports/after-check.json",
            expected_gap_movement: "closed",
            expected_bucket: "moved",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-exception-gap/expected/outcome/closed.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-exception-gap/expected/outcome/closed.md"
            ),
        })?;
        Ok(())
    }

    struct PythonPreviewOutcomeFixture<'a> {
        before: &'a str,
        after: &'a str,
        before_path: &'a str,
        after_path: &'a str,
        expected_gap_movement: &'a str,
        expected_bucket: &'a str,
        expected_json: &'a str,
        expected_md: &'a str,
    }

    fn assert_python_preview_outcome_fixture(
        fixture: PythonPreviewOutcomeFixture<'_>,
    ) -> Result<(), String> {
        let report = targeted_test_outcome_report_from_json(
            fixture.before,
            fixture.after,
            fixture.before_path.to_string(),
            fixture.after_path.to_string(),
        )?;
        let value: Value = serde_json::from_str(&render_targeted_test_outcome_json(&report)?)
            .map_err(|err| format!("targeted-test outcome JSON should parse: {err}"))?;
        assert_eq!(
            value["summary"]["gap_movement"][fixture.expected_gap_movement],
            1
        );
        assert_eq!(value["summary"][fixture.expected_bucket], 1);
        assert_eq!(
            render_targeted_test_outcome_json(&report)?,
            fixture.expected_json
        );
        assert_eq!(
            render_targeted_test_outcome_md(&report),
            fixture.expected_md
        );
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_python_field_fixture_matches_expected_receipts() -> Result<(), String>
    {
        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: include_str!(
                "../../../../fixtures/first_successful_pr/python-field-gap/inputs/reports/before-check.json"
            ),
            after: include_str!(
                "../../../../fixtures/first_successful_pr/python-field-gap/inputs/reports/after-check.json"
            ),
            before_path: "fixtures/first_successful_pr/python-field-gap/inputs/reports/before-check.json",
            after_path: "fixtures/first_successful_pr/python-field-gap/inputs/reports/after-check.json",
            expected_gap_movement: "closed",
            expected_bucket: "moved",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-field-gap/expected/outcome/closed.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-field-gap/expected/outcome/closed.md"
            ),
        })?;
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_python_output_fixture_matches_expected_receipts() -> Result<(), String>
    {
        assert_python_preview_outcome_fixture(PythonPreviewOutcomeFixture {
            before: include_str!(
                "../../../../fixtures/first_successful_pr/python-output-gap/inputs/reports/before-check.json"
            ),
            after: include_str!(
                "../../../../fixtures/first_successful_pr/python-output-gap/inputs/reports/after-check.json"
            ),
            before_path: "fixtures/first_successful_pr/python-output-gap/inputs/reports/before-check.json",
            after_path: "fixtures/first_successful_pr/python-output-gap/inputs/reports/after-check.json",
            expected_gap_movement: "closed",
            expected_bucket: "moved",
            expected_json: include_str!(
                "../../../../fixtures/first_successful_pr/python-output-gap/expected/outcome/closed.json"
            ),
            expected_md: include_str!(
                "../../../../fixtures/first_successful_pr/python-output-gap/expected/outcome/closed.md"
            ),
        })?;
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_prefers_evidence_record_movement() -> Result<(), String> {
        let before = r#"{
  "schema_version": "0.3",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "legacy_kind",
      "file": "legacy.rs",
      "line": 7,
      "grip_class": "ungripped",
      "related_tests": [],
      "observed_values": ["legacy-only"],
      "missing_discriminators": ["legacy missing"],
      "evidence_record": {
        "schema_version": "0.1",
        "seam_id": "seam-a",
        "location": {"file": ".\\src\\pricing.rs", "line": 42},
        "seam_kind": "predicate_boundary",
        "grip_class": "weakly_gripped",
        "evidence_path": {
          "reach": {"state": "yes", "confidence": "high", "summary": "owner reached"},
          "activate": {"state": "yes", "confidence": "high", "summary": "above boundary covered"},
          "propagate": {"state": "yes", "confidence": "medium", "summary": "return value flows"},
          "observe": {"state": "weak", "confidence": "medium", "summary": "weak assertion"},
          "discriminate": {"state": "missing", "confidence": "high", "summary": "equality not asserted"}
        },
        "observed_values": [{"value": "50", "line": 9, "text": "discounted_total(50)", "context": "function_argument"}],
        "missing_discriminators": [{"value": "threshold equality", "reason": "not observed"}],
        "related_tests_total": 1,
        "related_tests": [
          {"oracle_kind": "exact_value", "oracle_strength": "weak"}
        ]
      }
    }
  ]
}"#;
        let after = r#"{
  "schema_version": "0.3",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "legacy_kind",
      "file": "legacy.rs",
      "line": 7,
      "grip_class": "ungripped",
      "related_tests": [],
      "observed_values": ["legacy-only"],
      "missing_discriminators": ["legacy missing"],
      "evidence_record": {
        "schema_version": "0.1",
        "seam_id": "seam-a",
        "location": {"file": "src/pricing.rs", "line": 42},
        "seam_kind": "predicate_boundary",
        "grip_class": "strongly_gripped",
        "evidence_path": {
          "reach": {"state": "yes", "confidence": "high", "summary": "owner reached"},
          "activate": {"state": "yes", "confidence": "high", "summary": "equality covered"},
          "propagate": {"state": "yes", "confidence": "medium", "summary": "return value flows"},
          "observe": {"state": "yes", "confidence": "high", "summary": "exact assertion"},
          "discriminate": {"state": "yes", "confidence": "high", "summary": "equality asserted"}
        },
        "observed_values": [
          {"value": "50", "line": 9, "text": "discounted_total(50)", "context": "function_argument"},
          {"value": "100", "line": 10, "text": "discounted_total(100)", "context": "function_argument"}
        ],
        "missing_discriminators": [],
        "related_tests_total": 2,
        "related_tests": [{"oracle_kind": "exact_value", "oracle_strength": "strong"}]
      }
    }
  ]
}"#;
        let report = targeted_test_outcome_report_from_json(
            before,
            after,
            "before.json".to_string(),
            "after.json".to_string(),
        )?;

        assert_eq!(report.moved.len(), 1);
        let movement = &report.moved[0];
        assert_eq!(movement.seam_kind, "predicate_boundary");
        assert_eq!(movement.file, "src/pricing.rs");
        assert_eq!(movement.line, 42);
        assert_eq!(movement.before, "weakly_gripped");
        assert_eq!(movement.after, "strongly_gripped");
        assert_eq!(movement.evidence_source, "evidence_record");
        assert_eq!(movement.observed_values_added, vec!["100".to_string()]);
        assert_eq!(
            movement.missing_discriminators_resolved,
            vec!["threshold equality (not observed)".to_string()]
        );
        assert_eq!(
            movement.oracle_strength_delta,
            Some("weak -> strong".to_string())
        );
        assert_eq!(movement.related_test_delta, 1);
        assert_eq!(
            movement
                .discriminate_delta
                .as_ref()
                .and_then(|delta| delta.before_state.as_deref()),
            Some("missing")
        );
        assert_eq!(
            movement
                .discriminate_delta
                .as_ref()
                .and_then(|delta| delta.after_state.as_deref()),
            Some("yes")
        );

        let json = render_targeted_test_outcome_json(&report)?;
        let value: Value = serde_json::from_str(&json)
            .map_err(|err| format!("targeted-test outcome JSON should parse: {err}"))?;
        assert_eq!(value["moved"][0]["evidence_source"], "evidence_record");
        assert_eq!(value["moved"][0]["observed_values_added"][0], "100");
        assert_eq!(
            value["moved"][0]["missing_discriminators_resolved"][0],
            "threshold equality (not observed)"
        );
        assert_eq!(value["moved"][0]["oracle_strength_delta"], "weak -> strong");
        assert_eq!(value["moved"][0]["related_test_delta"], 1);
        assert_eq!(
            value["moved"][0]["discriminate_delta"]["before_state"],
            "missing"
        );
        assert_eq!(
            value["moved"][0]["discriminate_delta"]["after_state"],
            "yes"
        );
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_records_no_movement_reason() {
        let seam = targeted_static_seam("same", "weakly_gripped");
        let movement = targeted_test_outcome_movement(&seam, &seam);
        assert_eq!(movement.direction, "unchanged");
        assert_eq!(
            movement.no_movement_reason.as_deref(),
            Some("grip class and legacy_fields evidence were unchanged")
        );
    }

    #[test]
    fn targeted_test_outcome_rejects_duplicate_seam_ids() {
        let seam = targeted_static_seam("same", "weakly_gripped");
        let result = build_targeted_test_outcome_report(
            &[seam.clone(), seam],
            &[],
            "before.json".to_string(),
            "after.json".to_string(),
        );
        assert!(matches!(result, Err(message) if message.contains("duplicate seam_id `same`")));
    }

    #[test]
    fn targeted_test_outcome_reports_non_class_delta_branches() {
        let mut before = targeted_static_seam("same-rank", "activation_unknown");
        before.missing_discriminators = vec!["new missing later".to_string()];
        before.observed_values = vec!["old".to_string()];
        before.oracle_kind = "exact_value".to_string();
        before.oracle_strength = "strong".to_string();
        let mut after = targeted_static_seam("same-rank", "propagation_unknown");
        after.missing_discriminators = vec!["different missing now".to_string()];
        after.oracle_kind = "error_variant".to_string();
        after.oracle_strength = "weak".to_string();

        let movement = targeted_test_outcome_movement(&before, &after);
        assert_eq!(movement.direction, "changed");
        assert!(
            movement
                .evidence_delta
                .iter()
                .any(|delta| delta.contains("new missing discriminator reported"))
        );
        assert!(
            movement
                .evidence_delta
                .iter()
                .any(|delta| delta.contains("previous observed value absent"))
        );
        assert!(
            movement
                .evidence_delta
                .iter()
                .any(|delta| delta.contains("related oracle strength decreased"))
        );

        let mut before_kind = targeted_static_seam("same-kind-rank", "weakly_gripped");
        before_kind.oracle_kind = "exact_value".to_string();
        before_kind.oracle_strength = "medium".to_string();
        let mut after_kind = before_kind.clone();
        after_kind.oracle_kind = "custom_helper".to_string();
        let kind_movement = targeted_test_outcome_movement(&before_kind, &after_kind);
        assert!(
            kind_movement
                .evidence_delta
                .iter()
                .any(|delta| delta.contains("related oracle kind changed"))
        );
    }

    #[test]
    fn targeted_test_outcome_json_and_markdown_render_new_and_removed() -> Result<(), String> {
        let before = vec![targeted_static_seam("removed", "weakly_gripped")];
        let after = vec![targeted_static_seam("new", "ungripped")];
        let report = build_targeted_test_outcome_report(
            &before,
            &after,
            "before.json".to_string(),
            "after.json".to_string(),
        )?;

        let json = render_targeted_test_outcome_json(&report)?;
        assert!(json.contains(r#""removed""#));
        assert!(json.contains(r#""new""#));
        assert!(json.contains(r#""grip_class": "ungripped""#));

        let markdown = render_targeted_test_outcome_md(&report);
        assert!(markdown.contains("## New"));
        assert!(markdown.contains("`new` src/pricing.rs:42 ungripped"));
        assert!(markdown.contains("## Removed"));
        assert!(markdown.contains("`removed` src/pricing.rs:42 weakly_gripped"));
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_parser_handles_scalar_fallbacks_and_empty_inputs() -> Result<(), String>
    {
        let before = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": 7,
      "kind": "predicate_boundary",
      "file": "./src/pricing.rs",
      "line": "42",
      "grip_class": "weakly_gripped",
      "related_tests": [],
      "observed_values": [50, true],
      "missing_discriminators": [
        "plain missing",
        {"value": "value only", "reason": ""}
      ]
    }
  ]
}"#;
        let after = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": []
}"#;
        let report = targeted_test_outcome_report_from_json(
            before,
            after,
            "before.json".to_string(),
            "after.json".to_string(),
        )?;
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].seam_id, "7");
        assert_eq!(report.removed[0].file, "src/pricing.rs");
        Ok(())
    }

    #[test]
    fn targeted_test_outcome_rejects_missing_required_fields() {
        let result = targeted_test_outcome_report_from_json(
            r#"{"seams":[{"seam_id":"missing-kind"}]}"#,
            r#"{"seams":[]}"#,
            "before.json".to_string(),
            "after.json".to_string(),
        );
        assert!(matches!(result, Err(message) if message.contains("missing string field `kind`")));
    }

    fn targeted_static_seam(id: &str, grip_class: &str) -> StaticSeamRecord {
        StaticSeamRecord {
            seam_id: id.to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: grip_class.to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "unknown".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
            evidence_source: "legacy_fields".to_string(),
            evidence_path: BTreeMap::new(),
            related_tests_total: 0,
        }
    }
}
