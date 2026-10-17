use super::*;

#[test]
fn pr_review_front_panel_fixture_corpus_guard_accepts_complete_contract() -> Result<(), String> {
    let root = temp_dir("pr-review-front-panel-valid");
    let base = root.join("pr-review-front-panel");
    let report = root.join("pr-review-front-panel.json");
    let markdown = root.join("pr-review-front-panel.md");
    write_pr_review_front_panel_corpus(&base, &report, &markdown, "advisory", 0);
    write(
        &report,
        r#"{
  "kind": "pr_review_front_panel",
  "status": "advisory",
  "summary": {
    "top_issue_state": "actionable",
    "policy_state": "new_policy_eligible",
    "placement": "changed_line",
    "movement_state": "unknown",
    "coverage_grip_state": "not_available",
    "new_policy_eligible": 1,
    "baseline_resolved": 0,
    "blocking_candidates": 0,
    "warnings": 0
  },
  "artifacts": [
    {"group": "start_here"}
  ],
  "limits": ["Static RIPR evidence only."]
}
"#,
    );
    write(&markdown, "# RIPR PR Review\n\nStatus: advisory\n");

    let mut violations = Vec::new();
    super::super::validate_pr_review_front_panel_fixture_corpus_at(&base, &mut violations)?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn pr_review_front_panel_fixture_corpus_guard_reports_contract_drift() -> Result<(), String> {
    let root = temp_dir("pr-review-front-panel-invalid");
    let base = root.join("pr-review-front-panel");
    let report = root.join("pr-review-front-panel.json");
    let markdown = root.join("pr-review-front-panel.md");
    write_pr_review_front_panel_corpus(&base, &report, &markdown, "blocked", 1);
    write(
        &report,
        r#"{
  "kind": "wrong_kind",
  "status": "advisory",
  "summary": {
    "top_issue_state": "missing",
    "policy_state": "none",
    "placement": "not_available",
    "movement_state": "not_available",
    "coverage_grip_state": "not_available",
    "new_policy_eligible": 0,
    "baseline_resolved": 0,
    "blocking_candidates": 0,
    "warnings": 0
  },
  "artifacts": [
    {"group": "unknown"}
  ],
  "limits": ["Does not run mutation testing."]
}
"#,
    );
    write(&markdown, "# Wrong\n\nStatus: advisory\n");

    let mut violations = Vec::new();
    super::super::validate_pr_review_front_panel_fixture_corpus_at(&base, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("report kind must be pr_review_front_panel"));
    assert!(report.contains("expected status blocked"));
    assert!(report.contains("expected top_issue_state=actionable"));
    assert!(report.contains("expected policy_state=new_policy_eligible"));
    assert!(report.contains("expected placement=changed_line"));
    assert!(report.contains("expected movement_state=unknown"));
    assert!(report.contains("expected new_policy_eligible=1, got 0"));
    assert!(report.contains("expected blocking_candidates=1, got 0"));
    assert!(report.contains("artifacts must use known groups"));
    assert!(report.contains("report is missing static evidence limit"));
    assert!(report.contains("Markdown must use the PR review heading"));
    assert!(report.contains("Markdown must pin status blocked"));
    assert!(report.contains("blocked Markdown must name gate authority"));
    Ok(())
}

#[test]
fn pr_review_front_panel_fixture_corpus_guard_rejects_stale_case_removal() -> Result<(), String> {
    let root = temp_dir("pr-review-front-panel-stale-case-removal");
    let base = root.join("pr-review-front-panel");
    let report = root.join("pr-review-front-panel.json");
    let markdown = root.join("pr-review-front-panel.md");
    write_pr_review_front_panel_corpus(&base, &report, &markdown, "advisory", 0);
    write(
        &report,
        r#"{
  "kind": "pr_review_front_panel",
  "status": "advisory",
  "summary": {
    "top_issue_state": "actionable",
    "policy_state": "new_policy_eligible",
    "placement": "changed_line",
    "movement_state": "unknown",
    "coverage_grip_state": "not_available",
    "new_policy_eligible": 1,
    "baseline_resolved": 0,
    "blocking_candidates": 0,
    "warnings": 0
  },
  "artifacts": [{"group": "start_here"}],
  "limits": ["Static RIPR evidence only."]
}
"#,
    );
    write(&markdown, "# RIPR PR Review\n\nStatus: advisory\n");

    let corpus_path = base.join("corpus.json");
    let corpus_text = fs::read_to_string(&corpus_path)
        .map_err(|err| format!("read temporary front-panel corpus failed: {err}"))?;
    let mut corpus: serde_json::Value = serde_json::from_str(&corpus_text)
        .map_err(|err| format!("parse temporary front-panel corpus failed: {err}"))?;
    corpus["cases"]
        .as_array_mut()
        .ok_or_else(|| "temporary front-panel corpus cases missing".to_string())?
        .retain(|case| {
            case.get("id").and_then(serde_json::Value::as_str)
                != Some("stale_same_path_receipt_fails_closed")
        });
    write(
        &corpus_path,
        &serde_json::to_string_pretty(&corpus)
            .map_err(|err| format!("render temporary front-panel corpus failed: {err}"))?,
    );

    let mut violations = Vec::new();
    super::super::validate_pr_review_front_panel_fixture_corpus_at(&base, &mut violations)?;
    assert_contains_error(
        &violations,
        "corpus is missing required case stale_same_path_receipt_fails_closed",
    );
    Ok(())
}

#[test]
fn pr_inline_comment_publisher_fixture_corpus_guard_accepts_complete_contract() -> Result<(), String>
{
    let root = temp_dir("pr-inline-comment-publisher-valid");
    let base = root.join("pr-inline-comment-publisher");
    let report = root.join("comment-publish-plan.json");
    let markdown = root.join("comment-publish-plan.md");
    write_pr_inline_comment_publisher_corpus(&base, &report, &markdown, "plan", 1, 0, 0);
    write(
        &report,
        r#"{
  "schema_version": "0.1",
  "kind": "pr_inline_comment_publish_plan",
  "status": "advisory",
  "mode": "plan",
  "limits": {
    "comments_default": "off"
  },
  "summary": {
    "publishable": 1,
    "skipped": 0,
    "blocked": 0,
    "safe_to_publish": false
  },
  "operations": [
    {
      "operation": "create",
      "source_collection": "comments",
      "dedupe_key": "ripr:test:src/lib.rs:1",
      "placement": {
        "path": "src/lib.rs",
        "line": 1,
        "side": "RIGHT"
      }
    }
  ],
  "skipped": [],
  "blocked": [],
  "limits_note": "Advisory inline-comment publish plan only."
}
"#,
    );
    write(
        &markdown,
        "# RIPR Inline Comment Publish Plan\n\nMode: plan\nStatus: advisory\n\nPlanned operations:\n- create src/lib.rs:1\n\nLimits:\n- Advisory inline-comment publish plan only.\n",
    );

    let mut violations = Vec::new();
    super::super::validate_pr_inline_comment_publisher_fixture_corpus_at(&base, &mut violations)?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn pr_inline_comment_publisher_fixture_corpus_guard_reports_contract_drift() -> Result<(), String> {
    let root = temp_dir("pr-inline-comment-publisher-invalid");
    let base = root.join("pr-inline-comment-publisher");
    let report = root.join("comment-publish-plan.json");
    let markdown = root.join("comment-publish-plan.md");
    write_pr_inline_comment_publisher_corpus(&base, &report, &markdown, "inline", 1, 1, 1);
    write(
        &report,
        r#"{
  "schema_version": "0.2",
  "kind": "wrong_kind",
  "status": "pass",
  "mode": "off",
  "limits": {
    "comments_default": "inline"
  },
  "summary": {
    "publishable": 0,
    "skipped": 0,
    "blocked": 0,
    "safe_to_publish": true
  },
  "operations": [
    {
      "operation": "surprise",
      "source_collection": "summary_only"
    }
  ],
  "skipped": [
    {"skip_reason": "surprise"}
  ],
  "blocked": [
    {"blocked_reason": "surprise"}
  ],
  "limits_note": "wrong"
}
"#,
    );
    write(&markdown, "# Wrong\n\nMode: off\nStatus: pass\n");

    let mut violations = Vec::new();
    super::super::validate_pr_inline_comment_publisher_fixture_corpus_at(&base, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("report kind must be pr_inline_comment_publish_plan"));
    assert!(report.contains("expected status advisory"));
    assert!(report.contains("expected mode inline"));
    assert!(report.contains("limits.comments_default must be off"));
    assert!(report.contains("expected publishable=1, got 0"));
    assert!(report.contains("expected skipped=1, got 0"));
    assert!(report.contains("expected blocked=1, got 0"));
    assert!(report.contains("expected safe_to_publish=false, got true"));
    assert!(report.contains("operations must use known operation vocabulary"));
    assert!(report.contains("operations must not publish summary_only guidance"));
    assert!(report.contains("is missing expected operation create"));
    assert!(report.contains("skipped entries must use known skip_reason vocabulary"));
    assert!(report.contains("blocked entries must use known blocked_reason vocabulary"));
    assert!(report.contains("report is missing advisory publish-plan limit"));
    assert!(report.contains("Markdown must use the inline comment publish-plan heading"));
    assert!(report.contains("Markdown must pin mode inline"));
    assert!(report.contains("Markdown must pin status advisory"));
    assert!(report.contains("Markdown must list planned operations"));
    assert!(report.contains("Markdown must list skipped guidance"));
    assert!(report.contains("Markdown must list blocked operations"));
    assert!(report.contains("Markdown must name advisory publish-plan limit"));
    Ok(())
}

#[test]
fn pr_summary_lists_top_level_plans_as_docs_evidence() {
    let changes = vec![ChangedPath {
        path: "plans/campaign-27/lane3-editor-preview-routing.md".to_string(),
        statuses: BTreeSet::from(["??".to_string()]),
    }];

    let body = pr_summary_body(&changes);

    assert!(body.contains("- `plans/campaign-27/lane3-editor-preview-routing.md (??)`"));
    assert!(body.contains("Evidence/support delta:"));
    assert!(body.contains("Docs:"));
}

#[test]
fn pr_summary_lists_changelog_fragments_as_docs_evidence() {
    let changes = vec![ChangedPath {
        path: "changelog.d/6000-cli-fix.md".to_string(),
        statuses: BTreeSet::from(["A".to_string()]),
    }];

    let body = pr_summary_body(&changes);

    assert!(body.contains("- `changelog.d/6000-cli-fix.md (A)`"));
    assert!(body.contains("Evidence/support delta:"));
    assert!(body.contains("Docs:"));
}

#[test]
fn pr_actionable_front_panel_leads_with_typed_repair_delta() {
    let changes = vec![ChangedPath {
        path: "src/pricing.rs".to_string(),
        statuses: BTreeSet::from(["M".to_string()]),
    }];
    let actionable = PrActionableInput::Read(serde_json::json!({
        "report": "actionable-gaps",
        "summary": {
            "actionable_gaps": 4,
            "packets_emitted": 2,
            "public_projection_eligible_packets": 2,
            "static_limitations": 3
        },
        "packets": [
            {
                "canonical_gap_id": "gap:abc",
                "gap_state": "actionable",
                "public_projection_eligible": true,
                "source_file": "src/pricing.rs",
                "changed_behavior": "amount >= threshold",
                "repair_kind": "add_boundary_assertion",
                "missing_discriminators": [
                    {"value": "amount == threshold"}
                ],
                "recommended_repair": "Add exact assertion for amount == threshold.",
                "verify_command": "ripr agent verify --root . --json",
                "receipt_command_or_path": "ripr agent receipt --root . --json",
                "static_limitations": [],
                "related_test_or_observer": {
                    "file": "tests/pricing.rs",
                    "line": 42,
                    "name": "discount_boundary"
                }
            },
            {
                "canonical_gap_id": "gap:def",
                "gap_state": "actionable",
                "public_projection_eligible": true,
                "source_file": "src/other.rs",
                "repair_kind": "add_output_proof",
                "static_limitations": ["observer_unknown"]
            }
        ]
    }));
    let outcomes = PrActionableInput::Read(serde_json::json!({
        "summary": {
            "resolved": 1,
            "receipts_present": 2,
            "receipts_missing_after_input": 1,
            "orphaned_receipts": 0,
            "evidence_improved": 1,
            "evidence_unchanged": 1
        }
    }));
    let front_panel = PrActionableInput::Read(serde_json::json!({
        "summary": {
            "new_policy_eligible": 2
        }
    }));

    let body =
        pr_actionable_delta_front_panel_from_inputs(&changes, &actionable, &outcomes, &front_panel);

    assert!(body.starts_with("## Actionable Repair Front Panel"));
    assert!(body.contains("- repo actionable gaps: `4`"));
    assert!(body.contains("- PR-local actionable gaps: `1`"));
    assert!(body.contains("- new actionable gaps: `2`"));
    assert!(body.contains("- resolved actionable gaps: `1`"));
    assert!(body.contains("`receipts_present=2`"));
    assert!(body.contains(
        "- blocked/static-limited gaps: `3` static limitation(s), `1` static-limited packet(s)"
    ));
    assert!(body.contains("- canonical gap: `gap:abc`"));
    assert!(body.contains("- Changed behavior: amount >= threshold"));
    assert!(body.contains("- Missing discriminator: amount == threshold"));
    assert!(body.contains("- Focused proof intent: Add exact assertion for amount == threshold."));
    assert!(body.contains("- related test or observer: `tests/pricing.rs:42` `discount_boundary`"));
    assert!(body.contains("- verify: `ripr agent verify --root . --json`"));
    assert!(body.contains("Does not run mutation testing"));
}

#[test]
fn pr_actionable_front_panel_highlights_top_python_repair_card() {
    let changes = vec![ChangedPath {
        path: "app/pricing.py".to_string(),
        statuses: BTreeSet::from(["M".to_string()]),
    }];
    let actionable = PrActionableInput::Read(serde_json::json!({
        "report": "actionable-gaps",
        "summary": {
            "actionable_gaps": 1,
            "packets_emitted": 1,
            "public_projection_eligible_packets": 1,
            "static_limitations": 0
        },
        "packets": [
            {
                "canonical_gap_id": "gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold",
                "gap_state": "actionable",
                "public_projection_eligible": true,
                "language": "python",
                "language_status": "preview",
                "authority_boundary": "Python repair cards are preview advisory evidence.",
                "source_file": "app/pricing.py",
                "changed_owner": "calculate_discount",
                "changed_behavior": "if amount >= threshold:",
                "repair_kind": "add_boundary_assertion",
                "missing_discriminator": "amount == threshold",
                "current_test_evidence": "tests/test_pricing.py reaches calculate_discount but only asserts broad success.",
                "recommended_test_shape": "pytest exact boundary assertion",
                "suggested_assertion": "assert calculate_discount(amount=threshold, threshold=threshold) == expected_discount",
                "suggested_test_file": "tests/test_pricing.py",
                "suggested_test_name": "test_calculate_discount_threshold_boundary",
                "allowed_files": ["tests/test_pricing.py"],
                "forbidden_files": ["app/pricing.py"],
                "verify_command": "pytest tests/test_pricing.py::test_calculate_discount_threshold_boundary",
                "receipt_command": "ripr outcome --before target/ripr/reports/before-check.json --after target/ripr/reports/after-check.json --out target/ripr/receipts/python-pricing-boundary.json",
                "stop_if": [
                    "import cannot be resolved",
                    "expected value is ambiguous",
                    "production code edit appears necessary"
                ]
            }
        ]
    }));

    let body = pr_actionable_delta_front_panel_from_inputs(
        &changes,
        &actionable,
        &PrActionableInput::Missing,
        &PrActionableInput::Missing,
    );

    assert!(body.contains("Top Python repair card:"));
    assert!(body.contains("- language: `python` (`preview`)"));
    assert!(body.contains("Python repair cards are preview advisory evidence."));
    assert!(body.contains(
            "- canonical gap: `gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold`"
        ));
    assert!(body.contains("- Changed owner: `calculate_discount`"));
    assert!(body.contains("- Missing discriminator: amount == threshold"));
    assert!(body.contains("- test shape: pytest exact boundary assertion"));
    assert!(body.contains(
            "- Suggested test target: `test_calculate_discount_threshold_boundary` in `tests/test_pricing.py`"
        ));
    assert!(body.contains(
        "- Verify: `pytest tests/test_pricing.py::test_calculate_discount_threshold_boundary`"
    ));
    assert!(body.contains("- Receipt: `ripr outcome --before"));
    assert!(body.contains("  - production code edit appears necessary"));
    assert!(body.contains("Does not run mutation testing"));
}

#[test]
fn pr_actionable_front_panel_fails_closed_without_artifacts() {
    let body = pr_actionable_delta_front_panel_from_inputs(
        &[],
        &PrActionableInput::Missing,
        &PrActionableInput::Missing,
        &PrActionableInput::Missing,
    );

    assert!(body.contains("- actionable gaps: `missing`"));
    assert!(body.contains("- repo actionable gaps: `unknown`"));
    assert!(body.contains("run `cargo xtask lane1-evidence-audit`"));
    assert!(body.contains("- new actionable gaps: `unavailable`"));
    assert!(body.contains("Static RIPR evidence only."));
    assert!(!body.contains("Gate passed"));
}

#[test]
fn pr_shape_warnings_flag_analysis_without_evidence() {
    let changes = vec![ChangedPath {
        path: "crates/ripr/src/analysis/classifier.rs".to_string(),
        statuses: BTreeSet::from(["M".to_string()]),
    }];
    let warnings = pr_shape_warnings(&changes);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("Analysis code changed"))
    );

    let with_evidence = vec![
        ChangedPath {
            path: "crates/ripr/src/analysis/classifier.rs".to_string(),
            statuses: BTreeSet::from(["M".to_string()]),
        },
        ChangedPath {
            path: "docs/specs/RIPR-SPEC-0001-static-exposure-loop.md".to_string(),
            statuses: BTreeSet::from(["M".to_string()]),
        },
    ];
    assert!(pr_shape_warnings(&with_evidence).is_empty());
}

#[test]
fn pr_triage_findings_flag_queue_risks() {
    let mut first = triage_pr(
        1,
        "devex: split workflow renderer",
        &["xtask/src/main.rs", "docs/PR_AUTOMATION.md"],
    );
    first.merge_state_status = "BEHIND".to_string();

    let second = triage_pr(
        2,
        "devex: split workflow renderer",
        &["xtask/src/main.rs", "docs/PR_AUTOMATION.md"],
    );

    let mut stale = triage_pr(3, "docs: old draft", &["docs/ROADMAP.md"]);
    stale.is_draft = true;
    stale.created_at = "2026-05-01T00:00:00Z".to_string();

    let mut policy = triage_pr(
        4,
        "ci(policy): generated workflow",
        &[".github/workflows/ci.yml"],
    );
    policy.body = "Validation\n- not run: cargo xtask check-pr\n".to_string();
    policy.checks = vec![PrTriageCheck {
        name: "rust".to_string(),
        status: "COMPLETED".to_string(),
        conclusion: "FAILURE".to_string(),
    }];

    let today = days_from_civil(2026, 5, 14);
    let prs = vec![first, second, stale, policy];
    let findings = pr_triage_findings(&prs, today);
    let categories = findings
        .iter()
        .map(|finding| finding.category.as_str())
        .collect::<BTreeSet<_>>();
    assert!(categories.contains("same title family"));
    assert!(categories.contains("same changed file set"));
    assert!(categories.contains("stale draft"));
    assert!(categories.contains("behind main"));
    assert!(categories.contains("incomplete validation"));
    assert!(categories.contains("policy-sensitive surface"));

    let dispositions = pr_triage_queue_dispositions(&prs, &findings)
        .into_iter()
        .map(|item| (item.pr_number, item.disposition))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        dispositions.get(&1).map(String::as_str),
        Some("needs_owner_decision")
    );
    assert_eq!(
        dispositions.get(&2).map(String::as_str),
        Some("needs_owner_decision")
    );
    assert_eq!(
        dispositions.get(&3).map(String::as_str),
        Some("needs_owner_decision")
    );
    assert_eq!(
        dispositions.get(&4).map(String::as_str),
        Some("do_not_touch_wrong_lane")
    );
}

#[test]
fn pr_triage_queue_dispositions_name_actionable_queue_state() {
    let mergeable = triage_pr(1, "devex: ready", &["xtask/src/main.rs"]);
    let mut behind = triage_pr(2, "devex: behind", &["docs/CI.md"]);
    behind.merge_state_status = "BEHIND".to_string();
    let mut pending = triage_pr(3, "devex: pending", &["docs/PR_AUTOMATION.md"]);
    pending.checks = vec![PrTriageCheck {
        name: "rust".to_string(),
        status: "IN_PROGRESS".to_string(),
        conclusion: String::new(),
    }];
    let mut duplicate = triage_pr(4, "devex: duplicate cleanup", &["docs/README.md"]);
    duplicate.labels = vec!["duplicate".to_string()];
    let mut superseded = triage_pr(5, "devex: old approach", &["docs/OLD.md"]);
    superseded.body.push_str("\nSuperseded by #6.\n");
    let mut duplicate_body = triage_pr(6, "devex: refresh", &["docs/REFRESH.md"]);
    duplicate_body.body.push_str("\nDuplicate of #4.\n");

    let prs = vec![
        mergeable,
        behind,
        pending,
        duplicate,
        superseded,
        duplicate_body,
    ];
    let findings = pr_triage_findings(&prs, days_from_civil(2026, 5, 14));
    let dispositions = pr_triage_queue_dispositions(&prs, &findings)
        .into_iter()
        .map(|item| (item.pr_number, item.disposition))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        dispositions.get(&1).map(String::as_str),
        Some("merge_candidate")
    );
    assert_eq!(
        dispositions.get(&2).map(String::as_str),
        Some("needs_integration_review")
    );
    assert_eq!(
        dispositions.get(&3).map(String::as_str),
        Some("needs_fresh_validation")
    );
    assert_eq!(
        dispositions.get(&4).map(String::as_str),
        Some("close_duplicate")
    );
    assert_eq!(dispositions.get(&5).map(String::as_str), Some("superseded"));
    assert_eq!(
        dispositions.get(&6).map(String::as_str),
        Some("close_duplicate")
    );
}

#[test]
fn pr_triage_behind_packets_preserve_observation_without_branch_update() -> Result<(), String> {
    let mut pr = triage_pr(2, "devex: behind", &["docs/CI.md"]);
    pr.merge_state_status = "BEHIND".to_string();
    let prs = std::slice::from_ref(&pr);
    let today = days_from_civil(2026, 5, 14);
    let findings = pr_triage_findings(prs, today);
    let behind = findings
        .iter()
        .filter(|f| f.category == "behind main")
        .collect::<Vec<_>>();
    assert_eq!(behind.len(), 1, "setup must exercise the behind finding");
    let expected = "Inspect actual conflicts, changed prerequisites, combined-tree proof and applicable freshness policy; preserve unaffected proof. Behind alone does not require a branch update.";
    assert_eq!(behind[0].recommended_action, expected);
    let dispositions = pr_triage_queue_dispositions(prs, &findings);
    assert_eq!(dispositions.len(), 1);
    assert_eq!(dispositions[0].disposition, "needs_integration_review");
    assert_eq!(dispositions[0].recommended_action, expected);
    let markdown = pr_triage_markdown(prs, &findings, today);
    assert!(markdown.contains("`needs_integration_review`"));
    assert!(markdown.contains("BEHIND"));
    let (finding_section, remaining_sections) = markdown
        .split_once("\n## Queue Disposition\n\n")
        .ok_or("board must contain the queue section")?;
    assert!(finding_section.contains(&format!("Recommended action:\n\n```text\n{expected}\n```")));
    let queue_section = remaining_sections
        .split_once("\n## Open PRs\n\n")
        .ok_or("board must contain the open PR section")?
        .0;
    let queue_row = queue_section
        .lines()
        .find(|row| row.starts_with("| #2 |"))
        .ok_or("board must contain this PR's disposition row")?;
    assert!(queue_row.ends_with(&format!(" | {expected} |")));
    assert!(!markdown.contains("needs_rebase"));
    let json: Value = serde_json::from_str(&pr_triage_json(prs, &findings, today, "fixture"))
        .map_err(|err| format!("behind board packet should parse: {err}"))?;
    assert_eq!(json["mode"], "advisory");
    assert_eq!(
        json["queue_disposition"][0]["disposition"],
        "needs_integration_review"
    );
    assert_eq!(json["queue_disposition"][0]["recommended_action"], expected);
    let encoded_findings = json["findings"]
        .as_array()
        .ok_or("board findings must be an array")?;
    let encoded_behind = encoded_findings
        .iter()
        .find(|f| f["category"] == "behind main")
        .ok_or("board must retain the behind observation")?;
    assert_eq!(encoded_behind["recommended_action"], expected);
    let recommended_actions = json["recommended_actions"]
        .as_array()
        .ok_or("board recommended actions must be an array")?;
    let behind_action = recommended_actions
        .iter()
        .find(|a| a["category"] == "behind main")
        .ok_or("board must recommend a next step for the behind observation")?;
    assert_eq!(behind_action["action"], expected);
    Ok(())
}

#[test]
fn pr_triage_behind_keeps_review_validation_and_policy_routes() {
    for (case, draft, review, status, conclusion, path, expected) in [
        (
            "draft",
            true,
            "APPROVED",
            "COMPLETED",
            "SUCCESS",
            "docs/CI.md",
            "needs_review",
        ),
        (
            "review required",
            false,
            "REVIEW_REQUIRED",
            "COMPLETED",
            "SUCCESS",
            "docs/CI.md",
            "needs_review",
        ),
        (
            "pending",
            false,
            "APPROVED",
            "IN_PROGRESS",
            "",
            "docs/CI.md",
            "needs_fresh_validation",
        ),
        (
            "failed",
            false,
            "APPROVED",
            "COMPLETED",
            "FAILURE",
            "docs/CI.md",
            "needs_fresh_validation",
        ),
        (
            "policy",
            false,
            "APPROVED",
            "COMPLETED",
            "SUCCESS",
            ".github/workflows/ci.yml",
            "do_not_touch_wrong_lane",
        ),
    ] {
        let mut pr = triage_pr(2, "devex: behind", &[path]);
        pr.merge_state_status = "BEHIND".to_string();
        pr.is_draft = draft;
        pr.review_decision = review.to_string();
        pr.checks = vec![gh_pr_status_check("rust", status, conclusion)];
        let prs = std::slice::from_ref(&pr);
        let findings = pr_triage_findings(prs, days_from_civil(2026, 5, 14));
        let dispositions = pr_triage_queue_dispositions(prs, &findings);
        assert_eq!(dispositions.len(), 1);
        assert_eq!(dispositions[0].disposition, expected, "{case}");
    }
}

#[test]
fn pr_triage_helpers_classify_validation_and_sensitive_paths() {
    assert_eq!(
        pr_title_family("WIP devex: split workflow renderer v2"),
        "devex split workflow renderer"
    );
    assert_eq!(
        pr_body_validation_warning("Summary\n\nValidation\n- cargo xtask check-pr\n"),
        None
    );
    assert!(
        pr_body_validation_warning("Summary only")
            .is_some_and(|warning| warning.contains("no validation"))
    );
    assert_eq!(
        pr_sensitive_file_reason(".github/workflows/ci.yml"),
        Some("workflow behavior can alter generated CI or branch checks")
    );
    assert_eq!(
        pr_checks_summary(&[PrTriageCheck {
            name: "rust".to_string(),
            status: "IN_PROGRESS".to_string(),
            conclusion: String::new(),
        }]),
        "1 pending"
    );
    assert_eq!(
        pr_checks_summary(&[PrTriageCheck {
            name: "CodeRabbit".to_string(),
            status: "SUCCESS".to_string(),
            conclusion: "SUCCESS".to_string(),
        }]),
        "1 passed"
    );
}

#[test]
fn pr_triage_markdown_lists_findings_and_open_prs() {
    let pr = triage_pr(9, "devex: report", &["xtask/src/main.rs"]);
    let finding = PrTriageFinding {
        category: "same title family".to_string(),
        severity: "warn".to_string(),
        message: "#9 shares a title family".to_string(),
        prs: vec![9],
        details: vec!["#9 devex: report".to_string()],
        recommended_action: "Choose a canonical PR.".to_string(),
    };
    let markdown = pr_triage_markdown(&[pr], &[finding], days_from_civil(2026, 5, 14));
    assert!(markdown.contains("# ripr PR triage report"));
    assert!(markdown.contains("Status: warn"));
    assert!(markdown.contains("## Queue Disposition"));
    assert!(markdown.contains("needs_owner_decision"));
    assert!(markdown.contains("## Open PRs"));
    assert!(markdown.contains("cargo xtask pr-triage-report"));
}

#[test]
fn pr_triage_json_lists_prs_findings_and_actions() -> Result<(), String> {
    let pr = triage_pr(9, "devex: report", &["xtask/src/main.rs"]);
    let finding = PrTriageFinding {
        category: "same title family".to_string(),
        severity: "warn".to_string(),
        message: "#9 shares a title family".to_string(),
        prs: vec![9],
        details: vec!["#9 devex: report".to_string()],
        recommended_action: "Choose a canonical PR.".to_string(),
    };
    let json = pr_triage_json(&[pr], &[finding], days_from_civil(2026, 5, 14), "unix_ms:1");
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["mode"], "advisory");
    assert_eq!(value["generated_at"], "unix_ms:1");
    assert_eq!(value["open_prs"][0]["number"], 9);
    assert_eq!(
        value["queue_disposition"][0]["disposition"],
        "needs_owner_decision"
    );
    assert_eq!(value["findings"][0]["category"], "same title family");
    assert_eq!(
        value["recommended_actions"][0]["action"],
        "Choose a canonical PR."
    );
    Ok(())
}

#[test]
fn pr_ready_packet_marks_required_failure_as_fail() -> Result<(), String> {
    let steps = vec![
        PrReadyStep {
            id: "worktree_doctor",
            command: "cargo xtask worktree doctor",
            report: "target/ripr/reports/worktree-doctor.md",
            required: true,
            status: "fail".to_string(),
            summary: "branch is behind origin/main".to_string(),
        },
        PrReadyStep {
            id: "receipts_check",
            command: "cargo xtask receipts check",
            report: "target/ripr/reports/receipts.md",
            required: false,
            status: "needs_attention".to_string(),
            summary: "missing receipt".to_string(),
        },
    ];

    assert_eq!(pr_ready_status(&steps), "fail");
    let json = pr_ready_json(&steps);
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["mode"], "advisory");
    assert_eq!(value["status"], "fail");
    assert_eq!(value["steps"][0]["id"], "worktree_doctor");
    assert_eq!(value["steps"][0]["required"], true);
    assert!(
        value["judgment_required"]
            .as_array()
            .ok_or("judgment_required must be an array")?
            .iter()
            .any(|entry| entry == "golden blessing")
    );

    let markdown = pr_ready_markdown(&steps);
    assert!(markdown.contains("# ripr PR Ready"));
    assert!(markdown.contains("Status: fail"));
    assert!(markdown.contains("## Stop / Judgment Required"));
    assert!(markdown.contains("## Start-Here Language"));
    assert!(
        markdown.contains("missing artifact / stale evidence / wrong root / malformed artifact")
    );
    assert!(markdown.contains("verify command / receipt command / receipt path"));
    Ok(())
}

#[test]
fn pr_ready_packet_keeps_nonblocking_items_actionable() {
    let steps = vec![
        PrReadyStep {
            id: "worktree_doctor",
            command: "cargo xtask worktree doctor",
            report: "target/ripr/reports/worktree-doctor.md",
            required: true,
            status: "pass".to_string(),
            summary: "completed".to_string(),
        },
        PrReadyStep {
            id: "receipts_check",
            command: "cargo xtask receipts check",
            report: "target/ripr/reports/receipts.md",
            required: false,
            status: "needs_attention".to_string(),
            summary: "missing receipt".to_string(),
        },
    ];

    assert_eq!(pr_ready_status(&steps), "actionable");
    assert_eq!(
        pr_ready_next_action(&steps),
        "review the attention items, then run cargo xtask check-pr for full gate receipts"
    );
}

#[test]
fn pr_ready_report_status_maps_warnings_to_attention() {
    assert_eq!(
        pr_ready_status_from_report_status("warn"),
        Some("needs_attention")
    );
    assert_eq!(
        pr_ready_status_from_report_status("actionable"),
        Some("needs_attention")
    );
    assert_eq!(pr_ready_status_from_report_status("fail"), Some("fail"));
    assert_eq!(pr_ready_status_from_report_status("pass"), None);
}

#[test]
fn pr_ready_status_does_not_fail_required_warning() {
    let steps = vec![PrReadyStep {
        id: "worktree_doctor",
        command: "cargo xtask worktree doctor",
        report: "target/ripr/reports/worktree-doctor.md",
        required: true,
        status: "needs_attention".to_string(),
        summary: "report status: warn; see target/ripr/reports/worktree-doctor.md".to_string(),
    }];

    assert_eq!(pr_ready_status(&steps), "actionable");
}

#[test]
fn pr_change_name_status_bytes_decode_exotic_names_exact() -> Result<(), String> {
    // Real `--name-status -z` grammar (space, non-ASCII UTF-8, scored
    // rename): the old tab-split route without `-z` kept git's C-quoted
    // octal form verbatim, so byte-exactness here discriminates the
    // migration. Rename records attribute the target, matching the old
    // `parts.last()` projection.
    let mut changes = BTreeMap::new();
    add_name_status_bytes(
        &mut changes,
        "M\0sp ace.txt\0A\0uni-é.txt\0R100\0old.txt\0new.txt\0".as_bytes(),
    )?;
    let expected: BTreeMap<String, BTreeSet<String>> = [
        ("sp ace.txt".to_string(), ["M".to_string()].into()),
        ("uni-é.txt".to_string(), ["A".to_string()].into()),
        ("new.txt".to_string(), ["R100".to_string()].into()),
    ]
    .into();
    if changes != expected {
        return Err(format!(
            "exotic name-status inventory mismatch: got {changes:?}, want {expected:?}"
        ));
    }
    Ok(())
}

#[test]
fn pr_change_name_status_bytes_reject_non_utf8() -> Result<(), String> {
    // The old route lossy-decoded through `run_output`, collapsing this
    // record into replacement characters and returning success; the strict
    // route must fail loudly instead.
    let mut changes = BTreeMap::new();
    let err = match add_name_status_bytes(&mut changes, b"M\0ok.txt\0A\0\xffbad\0") {
        Err(err) => err,
        Ok(()) => return Err(format!("non-UTF-8 inventory must fail, got {changes:?}")),
    };
    if !err.contains("not valid UTF-8") {
        return Err(format!("unexpected strict-decode error: {err}"));
    }
    Ok(())
}

#[test]
fn pr_change_name_status_bytes_reject_legacy_line_grammar() -> Result<(), String> {
    // Legacy non-`-z` git output (C-quoted, newline-delimited) fed to the
    // new decoder must fail, not silently mangle: this pins the `-z`
    // requirement at the decode boundary. The old tab-split parser accepted
    // this shape and inventoried the quoted octal form as a path.
    let mut changes = BTreeMap::new();
    let legacy = b"M\t\"uni-\\303\\251.txt\"\n";
    match add_name_status_bytes(&mut changes, legacy) {
        Err(_) => Ok(()),
        Ok(()) => Err(format!(
            "legacy line grammar must fail strict decode, got {changes:?}"
        )),
    }
}

#[test]
fn pr_change_porcelain_bytes_decode_exotic_names_exact() -> Result<(), String> {
    // Real `status --porcelain=v1 -z` grammar (verified against git):
    // `XY␣path\0`, renames as `XY␣new\0old\0`, no quoting. The ` -> ` in
    // the fourth name is literal path bytes: the old `split_once(" -> ")`
    // projection would have inventoried `b.txt` instead.
    let mut changes = BTreeMap::new();
    add_porcelain_bytes(
        &mut changes,
        b"M  sp ace.txt\0R  new name.txt\0old name.txt\0?? uni-\xc3\xa9.txt\0M  a -> b.txt\0M  li\nne.txt\0",
    )?;
    let expected: BTreeMap<String, BTreeSet<String>> = [
        ("sp ace.txt".to_string(), ["M".to_string()].into()),
        ("new name.txt".to_string(), ["R".to_string()].into()),
        ("uni-é.txt".to_string(), ["??".to_string()].into()),
        ("a -> b.txt".to_string(), ["M".to_string()].into()),
        ("li\nne.txt".to_string(), ["M".to_string()].into()),
    ]
    .into();
    if changes != expected {
        return Err(format!(
            "exotic porcelain inventory mismatch: got {changes:?}, want {expected:?}"
        ));
    }
    Ok(())
}

#[test]
fn pr_change_porcelain_bytes_attribute_worktree_column_rename() -> Result<(), String> {
    // Worktree-column renames (` R`, e.g. intent-to-add via `git add -N`,
    // verified against real git output) carry the paired source exactly
    // like staged ones. Checking only the index column left the source
    // field unconsumed, failing closed on legitimate state — or worse,
    // misreading a source name with a space at byte index 2 as a new
    // entry. Exactly one entry for the target, status trimmed to `R`.
    let mut changes = BTreeMap::new();
    add_porcelain_bytes(&mut changes, b" R new.txt\0old.txt\0")?;
    let expected: BTreeMap<String, BTreeSet<String>> =
        [("new.txt".to_string(), ["R".to_string()].into())].into();
    if changes != expected {
        return Err(format!(
            "worktree rename inventory mismatch: got {changes:?}, want {expected:?}"
        ));
    }
    Ok(())
}

#[test]
fn pr_change_porcelain_bytes_reject_missing_trailing_nul() -> Result<(), String> {
    // Real git always NUL-terminates every record, so a non-empty input
    // without a trailing NUL is truncation, not a final field: fail
    // loudly instead of parsing a truncated path.
    let mut changes = BTreeMap::new();
    match add_porcelain_bytes(&mut changes, b"M  path_long") {
        Err(err) if err.contains("missing trailing NUL") => Ok(()),
        Err(err) => Err(format!("unexpected truncation error: {err}")),
        Ok(()) => Err(format!("truncated input must fail, got {changes:?}")),
    }
}

#[test]
fn pr_change_porcelain_bytes_reject_truncated_rename() -> Result<(), String> {
    // A rename entry missing its paired source must fail, not attribute
    // the change to half a record.
    let mut changes = BTreeMap::new();
    let err = match add_porcelain_bytes(&mut changes, b"R  new.txt\0") {
        Err(err) => err,
        Ok(()) => return Err(format!("truncated rename must fail, got {changes:?}")),
    };
    if !err.contains("missing its paired path") {
        return Err(format!("unexpected strict-decode error: {err}"));
    }
    Ok(())
}

#[test]
fn pr_change_porcelain_bytes_reject_misframed_entries() -> Result<(), String> {
    // Truncated fields must fail loudly instead of inventing entries. Note
    // what is deliberately NOT rejected here: embedded newlines are legal
    // path bytes (pinned by
    // `pr_change_porcelain_bytes_decode_exotic_names_exact`), so a
    // newline-bearing field decodes as one entry — the `-z` framing, not
    // content sniffing, is what separates records.
    let mut changes = BTreeMap::new();
    match add_porcelain_bytes(&mut changes, b"xy") {
        Err(_) => Ok(()),
        Ok(()) => Err(format!(
            "misframed porcelain input must fail, got {changes:?}"
        )),
    }
}

#[test]
fn pr_change_backslash_name_stays_distinct_from_nested_path() -> Result<(), String> {
    // Item-5 identity control (#4006): the literal-backslash filename
    // `a\b.rs` and the nested path `a/b.rs` are distinct tracked paths
    // (real Linux git fixture). Folding separators before map insertion
    // collapsed them to one key, omitting an inventory path while
    // returning success. Both decoders must preserve two entries.
    // Byte literals carry the exact `-z` grammar (Rust string escapes,
    // not shell quoting, keep the backslash literal).
    let mut changes = BTreeMap::new();
    add_name_status_bytes(&mut changes, b"M\0a\\b.rs\0M\0a/b.rs\0")?;
    if changes.len() != 2 || !changes.contains_key("a\\b.rs") || !changes.contains_key("a/b.rs") {
        return Err(format!(
            "name-status backslash/nested collision: got {changes:?}, want two distinct keys"
        ));
    }
    let mut porcelain = BTreeMap::new();
    add_porcelain_bytes(&mut porcelain, b"M  a\\b.rs\0M  a/b.rs\0")?;
    if porcelain.len() != 2
        || !porcelain.contains_key("a\\b.rs")
        || !porcelain.contains_key("a/b.rs")
    {
        return Err(format!(
            "porcelain backslash/nested collision: got {porcelain:?}, want two distinct keys"
        ));
    }
    Ok(())
}

#[test]
fn pr_change_inventory_bytes_accept_empty() -> Result<(), String> {
    // A real zero-change run decodes to an empty inventory on both routes.
    let mut changes = BTreeMap::new();
    add_name_status_bytes(&mut changes, b"")?;
    add_porcelain_bytes(&mut changes, b"")?;
    if !changes.is_empty() {
        return Err(format!("empty inventory must stay empty, got {changes:?}"));
    }
    Ok(())
}
