use serde_json::{Value, json};

use super::{Report, Status, evaluate};

const MANIFEST: &str = r#"
schema_version = "0.1"
non_claim = "no publication claim"
control_issue = 9000

[[release]]
version = "0.12.0"
milestone = "0.12.0 candidate"
goal_issue = 100
claim_blockers = [101]
proof_blockers = [102]
companions = []
conditional_issues = [104]

[[release]]
version = "0.13.0"
milestone = "0.13.0 candidate"
goal_issue = 200
claim_blockers = [300]
proof_blockers = []
companions = []
conditional_issues = []

[[parent]]
issue = 300
leaves = [101]
counted_in = "0.13.0"
justification = "distinct final acceptance"

[[parent]]
issue = 301
leaves = [102]
counted_in = "none"
justification = "umbrella outside milestone progress"

[[rolling]]
issue = 400
justification = "rolling control work"
"#;

fn fixture() -> Value {
    let description = "Candidate qualification in `ripr-swarm`; completion does not authorize or establish source publication.";
    json!({
        "schema_version": 1,
        "repository": "EffortlessMetrics/ripr-swarm",
        "complete": true,
        "milestones": [
            {"number": 12, "title": "0.12.0 candidate", "description": description},
            {"number": 13, "title": "0.13.0 candidate", "description": description},
            {"number": 17, "title": "Maintenance", "description": "Maintenance backlog"},
        ],
        "issues": [
            {"number": 100, "closed": false, "milestone": 12, "labels": ["target/0.12.0"]},
            {"number": 101, "closed": false, "milestone": 12, "labels": ["implementation", "target/0.12.0"]},
            {"number": 102, "closed": true, "milestone": 12, "labels": ["target/0.12.0"]},
            {"number": 104, "closed": false, "milestone": null, "labels": ["target/conditional"]},
            {"number": 200, "closed": true, "milestone": 13, "labels": ["target/0.13.0"]},
            {"number": 300, "closed": true, "milestone": 13, "labels": ["target/0.13.0"]},
            {"number": 301, "closed": false, "milestone": null, "labels": []},
            {"number": 400, "closed": false, "milestone": 17, "labels": ["target/rolling"]},
            {"number": 500, "closed": false, "milestone": null, "labels": []},
        ],
        "pull_requests": [
            {"number": 501, "closed": false, "milestone": 12, "closes": [101, 102]},
            {"number": 502, "closed": true, "milestone": null, "closes": [101]},
        ]
    })
}

fn compare(fixture: &Value) -> Report {
    evaluate(MANIFEST, &fixture.to_string())
}

fn assert_findings(report: &Report, status: Status, expected: &[(&str, &str)]) {
    assert_eq!(report.status, status, "{:?}", report.findings);
    assert_eq!(
        report
            .findings
            .iter()
            .map(|finding| (finding.code, finding.detail.as_str()))
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn complete_fixture_has_nonzero_subjects_and_clean_parity() -> Result<(), serde_json::Error> {
    let report = compare(&fixture());
    assert_findings(&report, Status::InSync, &[]);
    let json = serde_json::to_value(&report)?;
    assert_eq!(
        json["observed"],
        json!({"milestones": 3, "issues": 9, "pull_requests": 2})
    );
    assert_eq!(json["evidence"], "local_fixture");
    assert_eq!(json["status"], "in_sync");
    assert!(
        report
            .markdown()
            .contains("Observed: 3 milestones, 9 issues, 2 pull requests.")
    );
    Ok(())
}

#[test]
fn comparable_drift_pins_each_role_and_both_renderings() -> Result<(), serde_json::Error> {
    let mut snapshot = fixture();
    snapshot["issues"][0]["closed"] = json!(true);
    snapshot["issues"][1]["milestone"] = json!(13);
    snapshot["issues"][1]["labels"] = json!(["target/0.12"]);
    snapshot["issues"][3]["milestone"] = json!(12);
    snapshot["issues"][5]["milestone"] = json!(12);
    snapshot["issues"][6]["milestone"] = json!(13);
    snapshot["issues"][7]["milestone"] = json!(13);
    snapshot["issues"][8]["milestone"] = json!(12);
    snapshot["pull_requests"][0]["milestone"] = json!(null);

    let report = compare(&snapshot);
    assert_findings(
        &report,
        Status::Drift,
        &[
            (
                "closed_goal_with_open_work",
                "closed goal #100 has open committed issue #101",
            ),
            (
                "conditional_issue_in_candidate",
                "issue #104 is in candidate milestone #12",
            ),
            (
                "issue_milestone",
                "issue #101 requires milestone #12; observed #13",
            ),
            (
                "issue_milestone",
                "issue #300 requires milestone #13; observed #12",
            ),
            (
                "pull_request_milestone",
                "open pull request #501 closes issue #101 and requires milestone #12; observed none",
            ),
            (
                "pull_request_milestone",
                "open pull request #501 closes issue #102 and requires milestone #12; observed none",
            ),
            (
                "rolling_issue_in_candidate",
                "issue #400 is in candidate milestone #13",
            ),
            (
                "target_label_missing",
                "issue #101 requires label \"target/0.12.0\"",
            ),
            (
                "uncounted_parent_in_candidate",
                "issue #301 is in candidate milestone #13",
            ),
            (
                "unmanifested_issue_in_candidate",
                "issue #500 is in candidate milestone #12",
            ),
        ],
    );
    let json = serde_json::to_string_pretty(&report)?;
    let markdown = report.markdown();
    for finding in &report.findings {
        assert!(markdown.contains(&format!(
            "    {}: {}",
            finding.code,
            finding.detail.escape_default()
        )));
    }
    assert!(markdown.contains("Status: `drift`"));
    assert!(markdown.contains("Findings: 10"));

    for key in ["milestones", "issues", "pull_requests"] {
        if let Some(rows) = snapshot[key].as_array_mut() {
            rows.reverse();
        }
    }
    snapshot["pull_requests"][1]["closes"] = json!([102, 101]);
    let reordered = compare(&snapshot);
    assert_eq!(json, serde_json::to_string_pretty(&reordered)?);
    assert_eq!(markdown, reordered.markdown());
    Ok(())
}

#[test]
fn exact_titles_and_unique_milestone_numbers_define_candidates() {
    let mut snapshot = fixture();
    snapshot["milestones"][0]["title"] = json!("0.12.0 candidate renamed");
    snapshot["milestones"][1]["description"] = json!("Source publication approved.");
    let mut duplicate = snapshot["milestones"][1].clone();
    duplicate["number"] = json!(29);
    duplicate["description"] = snapshot["milestones"][0]["description"].clone();
    if let Some(rows) = snapshot["milestones"].as_array_mut() {
        rows.push(duplicate);
    }
    let report = compare(&snapshot);
    assert_findings(
        &report,
        Status::Drift,
        &[
            (
                "milestone_duplicated",
                "candidate \"0.13.0 candidate\" matches milestones [13, 29]",
            ),
            (
                "milestone_missing",
                "candidate \"0.12.0 candidate\" is absent",
            ),
            (
                "milestone_nonclaim",
                "milestone #13 (0.13.0 candidate) must have description \"Candidate qualification in `ripr-swarm`; completion does not authorize or establish source publication.\"",
            ),
        ],
    );
}

#[test]
fn incomplete_or_invalid_evidence_cannot_report_parity() {
    let mut snapshot = fixture();
    snapshot["complete"] = json!(false);
    assert_findings(
        &compare(&snapshot),
        Status::NotComparable,
        &[(
            "fixture_incomplete",
            "snapshot declares incomplete coverage",
        )],
    );

    let mut snapshot = fixture();
    snapshot["repository"] = json!("https://github.com/OtherOwner/ripr-swarm");
    assert_findings(
        &compare(&snapshot),
        Status::NotComparable,
        &[(
            "fixture_repository",
            "expected EffortlessMetrics/ripr-swarm, found \"https://github.com/OtherOwner/ripr-swarm\"",
        )],
    );

    let mut snapshot = fixture();
    snapshot["issues"][1]["number"] = json!(100);
    assert_findings(
        &compare(&snapshot),
        Status::NotComparable,
        &[("duplicate_identity", "issue #100 appears more than once")],
    );

    let mut snapshot = fixture();
    if let Some(issues) = snapshot["issues"].as_array_mut() {
        issues.remove(1);
    }
    assert_findings(
        &compare(&snapshot),
        Status::NotComparable,
        &[(
            "issue_unavailable",
            "manifest issue #101 is absent from the snapshot",
        )],
    );

    let mut snapshot = fixture();
    snapshot["issues"][1]["milestone"] = json!(99);
    assert_findings(
        &compare(&snapshot),
        Status::NotComparable,
        &[(
            "unknown_milestone_reference",
            "issue #101 references milestone #99, absent from the snapshot",
        )],
    );

    let mut snapshot = fixture();
    if let Some(issue) = snapshot["issues"][3].as_object_mut() {
        issue.remove("milestone");
    }
    let report = compare(&snapshot);
    assert_eq!(report.status, Status::NotComparable);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "invalid_fixture"
                && finding.detail.contains("missing field `milestone`"))
    );

    let report = evaluate("", &fixture().to_string());
    assert_eq!(report.status, Status::NotComparable);
    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.code == "invalid_manifest")
    );
    assert!(!report.findings.is_empty());
}
