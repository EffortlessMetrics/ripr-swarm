use super::*;

#[test]
fn a_fragment_with_no_heading_names_the_link_and_the_target() -> Result<(), String> {
    let target = Path::new("docs/CI.md");
    let slugs = heading_slugs("# Copyable ripr advisory workflow\n");
    let resolving = MarkdownLink {
        line: 118,
        target: "CI.md#copyable-ripr-advisory-workflow".to_string(),
    };
    let broken = MarkdownLink {
        line: 118,
        target: "CI.md#this-anchor-does-not-exist".to_string(),
    };

    let mut failures = Vec::new();
    if let Some(violation) = missing_anchor_violation(
        "docs/QUICKSTART.md",
        &resolving,
        target,
        "copyable-ripr-advisory-workflow",
        &slugs,
    ) {
        failures.push(format!("a resolving anchor was reported: {violation}"));
    }
    match missing_anchor_violation(
        "docs/QUICKSTART.md",
        &broken,
        target,
        "this-anchor-does-not-exist",
        &slugs,
    ) {
        None => failures.push("a fragment with no heading was reported as a pass".to_string()),
        Some(violation) => {
            for expected in [
                "docs/QUICKSTART.md:118",
                "CI.md#this-anchor-does-not-exist",
                "docs/CI.md",
            ] {
                if !violation.contains(expected) {
                    failures.push(format!("`{violation}` does not name `{expected}`"));
                }
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn actionable_gap_outcomes_command_rejects_unsafe_argument_shapes() -> Result<(), String> {
    for (args, expected) in [
        (
            vec!["--actionable-gaps"],
            "missing value for `--actionable-gaps`",
        ),
        (
            vec!["--agent-receipt"],
            "missing value for `--agent-receipt`",
        ),
        (
            vec!["--targeted-test-outcome"],
            "missing value for `--targeted-test-outcome`",
        ),
        (vec!["--help"], "usage: cargo xtask actionable-gap-outcomes"),
        (
            vec!["--unknown"],
            "unknown actionable-gap-outcomes option `--unknown`",
        ),
        (
            vec!["positional"],
            "unexpected positional argument `positional`",
        ),
    ] {
        let owned_args = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        let err = parse_actionable_gap_outcomes_args(&owned_args)
            .expect_err("invalid actionable-gap-outcomes args should be rejected");
        assert!(
            err.contains(expected),
            "expected {expected:?} in error for {args:?}, got {err:?}"
        );
    }

    with_temp_cwd("actionable-gap-outcomes-missing-input", |_| {
        let err = actionable_gap_outcomes_report_impl(&[])
            .expect_err("missing actionable-gaps artifact should fail closed");
        assert!(
            err.contains(
                "actionable-gap-outcomes requires `target/ripr/reports/actionable-gaps.json`"
            ),
            "missing artifact error should name regeneration target, got {err:?}"
        );
        Ok(())
    })
}

#[test]
fn actionable_gap_outcomes_command_writes_markdown_and_json() -> Result<(), String> {
    with_temp_cwd("actionable-gap-outcomes", |_| {
        fs::create_dir_all("target/ripr/reports")
            .map_err(|err| format!("failed to create reports dir: {err}"))?;
        write(
            Path::new("target/ripr/reports/actionable-gaps.json"),
            r#"{
  "schema_version": "0.1",
  "tool": "ripr",
  "report": "actionable-gaps",
  "packets": [
    {
      "canonical_gap_id": "gap:seam-a",
      "evidence_class": "predicate_boundary",
      "repair_kind": "add_boundary_assertion",
      "source_file": "src/pricing.rs",
      "primary_anchor": {"file": "src/pricing.rs", "line": 42},
      "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
      "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-a"
    }
  ]
}"#,
        );
        write(
            Path::new("target/ripr/reports/agent-receipt.json"),
            r#"{
  "schema_version": "0.3",
  "seam": {
    "seam_id": "seam-a",
    "file": "src/pricing.rs",
    "line": 42,
    "before": "weakly_gripped",
    "after": "strongly_gripped",
    "change": "improved",
    "evidence_delta": ["missing discriminator no longer reported: threshold equality"]
  },
  "provenance": {"seam_id": "seam-a", "movement": "improved"}
}"#,
        );

        actionable_gap_outcomes_report_impl(&[])?;
        let json = fs::read_to_string("target/ripr/reports/actionable-gap-outcomes.json")
            .map_err(|err| format!("failed to read outcomes JSON: {err}"))?;
        let markdown = fs::read_to_string("target/ripr/reports/actionable-gap-outcomes.md")
            .map_err(|err| format!("failed to read outcomes Markdown: {err}"))?;
        assert!(json.contains("\"report\": \"actionable-gap-outcomes\""));
        assert!(json.contains("\"evidence_improved\": 1"));
        assert!(json.contains("\"movement_front\""));
        assert!(markdown.contains("# Actionable Gap Outcomes"));
        assert!(markdown.contains("## Movement Since Prior Refresh"));
        assert!(markdown.contains("gap:seam-a"));
        Ok(())
    })
}

#[test]
fn actionable_gap_outcomes_fixture_case_reports_expected_shape_drift() -> Result<(), String> {
    let base_packet = serde_json::json!({
        "canonical_gap_id": "gap:expected-shape",
        "evidence_class": "predicate_boundary",
        "repair_kind": "add_boundary_assertion",
        "source_file": "src/lib.rs",
        "primary_anchor": {"file": "src/lib.rs", "line": 1},
        "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
        "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id expected-shape --json"
    });

    for (case, expected) in [
        (
            serde_json::json!({
                "id": "empty-packets",
                "description": "Empty packet list.",
                "must_not_claim": ["Do not accept an empty packet list."],
                "actionable_gaps": {"packets": []}
            }),
            "actionable_gaps.packets must not be empty",
        ),
        (
            serde_json::json!({
                "id": "missing-expected",
                "description": "Missing expected object.",
                "must_not_claim": ["Do not accept a missing expected object."],
                "actionable_gaps": {"packets": [base_packet.clone()]}
            }),
            "is missing expected object",
        ),
        (
            serde_json::json!({
                "id": "empty-expected-outcomes",
                "description": "Empty expected outcomes.",
                "must_not_claim": ["Do not accept empty expected outcomes."],
                "actionable_gaps": {"packets": [base_packet.clone()]},
                "expected": {"summary": {"packets_total": 1}, "outcomes": []}
            }),
            "expected.outcomes must not be empty",
        ),
    ] {
        let case_id = case.get("id").and_then(Value::as_str).unwrap_or_default();
        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_case(
            &case,
            case_id,
            &mut violations,
        )?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(expected)),
            "expected fixture violation containing {expected:?}, got {violations:?}"
        );
    }

    let case = serde_json::json!({
        "id": "bad-expected-shape",
        "description": "Malformed expected shape.",
        "must_not_claim": ["Do not accept malformed expected outcome fields."],
        "actionable_gaps": {"packets": [base_packet]},
        "agent_receipt": null,
        "targeted_test_outcome": null,
        "expected": {
            "summary": {"packets_total": "one", "outcomes_total": 2},
            "outcomes": [
                {},
                {"canonical_gap_id": "gap:missing"},
                {"canonical_gap_id": "gap:expected-shape"}
            ]
        }
    });
    let mut violations = Vec::new();
    super::super::validate_actionable_gap_outcomes_fixture_case(
        &case,
        "bad-expected-shape",
        &mut violations,
    )?;
    for expected in [
        "expected outcome is missing canonical_gap_id",
        "did not produce outcome gap:missing",
        "expected outcome gap:expected-shape is missing outcome_state",
        "expected outcome gap:expected-shape is missing receipt_state",
        "expected.summary.packets_total must be numeric",
        "summary.outcomes_total must be 2, got 1",
    ] {
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(expected)),
            "expected malformed expected-shape violation containing {expected:?}, got {violations:?}"
        );
    }

    Ok(())
}

#[test]
fn actionable_gap_outcomes_fixture_corpus_matches_expected_states() -> Result<(), String> {
    with_repo_cwd(|| {
        let mut violations = Vec::new();
        validate_actionable_gap_outcomes_fixture_corpus(&mut violations)?;
        if violations.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "actionable gap outcomes corpus violations:\n- {}",
                violations.join("\n- ")
            ))
        }
    })
}

#[test]
fn actionable_gap_outcomes_fixture_corpus_reports_contract_drift() -> Result<(), String> {
    with_temp_cwd("actionable-gap-outcomes-corpus-drift", |_| {
        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_corpus_at(
            Path::new("missing-corpus.json"),
            &mut violations,
        )?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("actionable gap outcomes corpus is missing")),
            "missing corpus path should be reported: {violations:?}"
        );

        write(
            Path::new("bad-corpus.json"),
            r#"{
  "schema_version": "bad",
  "kind": "wrong",
  "spec": "RIPR-SPEC-0000",
  "related_spec": "RIPR-SPEC-0000",
  "cases": [
    {
      "id": "bad-outcome",
      "description": "Bad outcome case.",
      "must_not_claim": ["Do not trust this malformed case."],
      "actionable_gaps": {
        "packets": [
          {
            "canonical_gap_id": "gap:bad-outcome",
            "evidence_class": "predicate_boundary",
            "repair_kind": "add_boundary_assertion",
            "source_file": "src/lib.rs",
            "primary_anchor": {"file": "src/lib.rs", "line": 1},
            "verify_command": "ripr agent verify --json",
            "receipt_command_or_path": "ripr agent receipt --seam-id bad-outcome"
          }
        ]
      },
      "agent_receipt": null,
      "targeted_test_outcome": null,
      "expected": {
        "summary": {"packets_total": 2},
            "outcomes": [
          {
            "canonical_gap_id": "gap:bad-outcome",
            "outcome_state": "resolved",
            "receipt_state": "receipt_found",
            "movement_source": "agent_receipt"
          }
        ]
      }
    }
  ]
}"#,
        );

        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_corpus_at(
            Path::new("bad-corpus.json"),
            &mut violations,
        )?;
        for expected in [
            "kind must be actionable_gap_outcomes_corpus",
            "schema_version must be 0.1",
            "spec must be RIPR-SPEC-0031",
            "related_spec must be RIPR-SPEC-0057",
            "outcome_state must be resolved, got not_attempted",
            "receipt_state must be receipt_found, got receipt_not_applicable",
            "summary.packets_total must be 2, got 1",
            "corpus is missing case not_attempted_packet",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.contains(expected)),
                "expected violation containing {expected:?}, got {violations:?}"
            );
        }

        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_case(
            &serde_json::json!({"id": "missing-fields"}),
            "missing-fields",
            &mut violations,
        )?;
        for expected in [
            "missing description",
            "must_not_claim must be a non-empty string array",
            "is missing actionable_gaps object",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.contains(expected)),
                "expected fixture-case violation containing {expected:?}, got {violations:?}"
            );
        }

        let targeted_shape_case = serde_json::json!({
            "id": "bad-targeted-shape",
            "description": "Bad targeted-test outcome shape.",
            "must_not_claim": ["Do not infer movement from malformed targeted-test outcome."],
            "actionable_gaps": {
                "packets": [
                    {
                        "canonical_gap_id": "gap:bad-targeted-shape",
                        "evidence_class": "predicate_boundary",
                        "repair_kind": "add_boundary_assertion",
                        "source_file": "src/lib.rs",
                        "primary_anchor": {"file": "src/lib.rs", "line": 1},
                        "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                        "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id bad-targeted-shape --json"
                    }
                ]
            },
            "agent_receipt": {"receipts": []},
            "targeted_test_outcome": {
                "schema_version": "0.1",
                "moved": [
                    {
                        "seam_id": "bad-targeted-shape",
                        "seam_kind": "predicate_boundary",
                        "file": "src/lib.rs",
                        "line": 1
                    }
                ],
                "unchanged": [],
                "regressed": [],
                "removed": [
                    {
                        "seam_id": "bad-targeted-shape-removed",
                        "seam_kind": "predicate_boundary",
                        "file": "src/lib.rs",
                        "line": 2,
                        "before": "weakly_gripped"
                    }
                ],
                "new": [
                    {
                        "seam_id": "bad-targeted-shape-new",
                        "seam_kind": "predicate_boundary",
                        "file": "src/lib.rs",
                        "line": 3,
                        "after": "ungripped"
                    }
                ]
            },
            "expected": {
                "summary": {"packets_total": 1},
                "outcomes": [
                    {
                        "canonical_gap_id": "gap:bad-targeted-shape",
                        "outcome_state": "evidence_improved",
                        "receipt_state": "receipt_missing",
                        "movement_source": "targeted_test_outcome",
                        "movement_direction": "improved",
                        "before": "weakly_gripped",
                        "after": "strongly_gripped"
                    }
                ]
            }
        });
        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_case(
            &targeted_shape_case,
            "bad-targeted-shape",
            &mut violations,
        )?;
        for expected in [
            "targeted_test_outcome.moved item must include before",
            "targeted_test_outcome.moved item must include after",
            "targeted_test_outcome.moved item must include direction",
            "targeted_test_outcome.removed item must include grip_class",
            "targeted_test_outcome.removed item must use one-sided grip_class instead of before",
            "targeted_test_outcome.new item must include grip_class",
            "targeted_test_outcome.new item must use one-sided grip_class instead of after",
            "outcome_state must be evidence_improved, got attempted_no_receipt",
            "movement_direction must be \"improved\", got \"move\"",
            "before must be \"weakly_gripped\", got null",
            "after must be \"strongly_gripped\", got null",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.contains(expected)),
                "expected targeted-shape violation containing {expected:?}, got {violations:?}"
            );
        }

        Ok(())
    })
}

#[test]
fn actionable_gap_outcomes_fixture_corpus_reports_manifest_edge_states() -> Result<(), String> {
    with_temp_cwd("actionable-gap-outcomes-corpus-edge-states", |_| {
        write(Path::new("bad-json.json"), "{");
        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_corpus_at(
            Path::new("bad-json.json"),
            &mut violations,
        )?;
        assert!(
            !violations.is_empty(),
            "invalid corpus JSON should be reported"
        );

        write(
            Path::new("no-cases.json"),
            r#"{
  "schema_version": "0.1",
  "kind": "actionable_gap_outcomes_corpus",
  "spec": "RIPR-SPEC-0031",
  "related_spec": "RIPR-SPEC-0057"
}"#,
        );
        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_corpus_at(
            Path::new("no-cases.json"),
            &mut violations,
        )?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("is missing cases array")),
            "missing cases array should be reported: {violations:?}"
        );

        write(
            Path::new("duplicate-case.json"),
            r#"{
  "schema_version": "0.1",
  "kind": "actionable_gap_outcomes_corpus",
  "spec": "RIPR-SPEC-0031",
  "related_spec": "RIPR-SPEC-0057",
  "cases": [
    {
      "id": "not_attempted_packet",
      "description": "Duplicate case A.",
      "must_not_claim": ["Do not treat duplicate cases as valid."],
      "actionable_gaps": {
        "packets": [
          {
            "canonical_gap_id": "gap:duplicate-a",
            "evidence_class": "predicate_boundary",
            "repair_kind": "add_boundary_assertion",
            "source_file": "src/lib.rs",
            "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
            "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id duplicate-a"
          }
        ]
      },
      "expected": {
        "summary": {"packets_total": 1},
        "outcomes": [
          {"canonical_gap_id": "gap:duplicate-a", "outcome_state": "not_attempted", "receipt_state": "receipt_not_applicable"}
        ]
      }
    },
    {
      "id": "not_attempted_packet",
      "description": "Duplicate case B.",
      "must_not_claim": ["Do not treat duplicate cases as valid."],
      "actionable_gaps": {
        "packets": [
          {
            "canonical_gap_id": "gap:duplicate-b",
            "evidence_class": "predicate_boundary",
            "repair_kind": "add_boundary_assertion",
            "source_file": "src/lib.rs",
            "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
            "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id duplicate-b"
          }
        ]
      },
      "expected": {
        "summary": {"packets_total": 1},
        "outcomes": [
          {"canonical_gap_id": "gap:duplicate-b", "outcome_state": "receipt_present", "receipt_state": "receipt_found"}
        ]
      }
    }
  ]
}"#,
        );
        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_corpus_at(
            Path::new("duplicate-case.json"),
            &mut violations,
        )?;
        for expected in [
            "case not_attempted_packet is duplicated",
            "case not_attempted_packet must have state not_attempted/receipt_not_applicable",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.contains(expected)),
                "expected duplicate-case violation containing {expected:?}, got {violations:?}"
            );
        }

        Ok::<(), String>(())
    })
}

#[test]
fn actionable_gap_outcomes_fixture_corpus_reports_spec_wrapper_drift() -> Result<(), String> {
    with_temp_cwd("actionable-gap-outcomes-wrapper-drift", |_| {
        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_corpus(&mut violations)?;
        for expected in [
            "fixture corpus is missing fixtures/actionable-gap-outcomes-corpus/SPEC.md",
            "fixture corpus is missing fixtures/actionable-gap-outcomes-corpus/corpus.json",
            "corpus is missing fixtures/actionable-gap-outcomes-corpus/corpus.json",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.contains(expected)),
                "expected wrapper violation containing {expected:?}, got {violations:?}"
            );
        }

        write(
            Path::new("fixtures/actionable-gap-outcomes-corpus/SPEC.md"),
            "# Bad spec\n\n## Given\n",
        );
        let mut violations = Vec::new();
        super::super::validate_actionable_gap_outcomes_fixture_corpus(&mut violations)?;
        for expected in [
            "is missing `Spec: RIPR-SPEC-0031`",
            "is missing `Related: RIPR-SPEC-0057`",
            "is missing `## When`",
            "is missing `## Then`",
            "is missing `## Must Not`",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.contains(expected)),
                "expected spec violation containing {expected:?}, got {violations:?}"
            );
        }

        Ok(())
    })
}

#[test]
fn actionable_gap_outcomes_fixture_rejects_incomplete_receipt_command() -> Result<(), String> {
    let case = serde_json::json!({
        "id": "incomplete_receipt_command",
        "description": "outcome fixture packet with an incomplete receipt command",
        "actionable_gaps": {
            "schema_version": "0.1",
            "tool": "ripr",
            "report": "actionable-gaps",
            "packets": [
                {
                    "canonical_gap_id": "gap:incomplete-receipt-command",
                    "evidence_class": "predicate_boundary",
                    "repair_kind": "add_boundary_assertion",
                    "source_file": "src/lib.rs",
                    "primary_anchor": {
                        "file": "src/lib.rs",
                        "line": 12
                    },
                    "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                    "receipt_command_or_path": "ripr agent receipt --root . --json"
                }
            ]
        },
        "agent_receipt": null,
        "targeted_test_outcome": null,
        "expected": {
            "summary": {
                "packets_total": 1,
                "outcomes_total": 1,
                "not_attempted": 1,
                "receipt_present": 0,
                "evidence_improved": 0,
                "receipts_present": 0
            },
            "outcomes": [
                {
                    "canonical_gap_id": "gap:incomplete-receipt-command",
                    "outcome_state": "not_attempted",
                    "receipt_state": "receipt_not_applicable",
                    "movement_source": null
                }
            ]
        },
        "must_not_claim": [
            "Do not accept malformed receipt commands."
        ]
    });
    let mut violations = Vec::new();
    validate_actionable_gap_outcomes_fixture_case(
        &case,
        "incomplete_receipt_command",
        &mut violations,
    )?;

    violations
        .iter()
        .any(|violation| violation.contains("unsupported receipt command"))
        .then_some(())
        .ok_or_else(|| format!("missing unsupported receipt command violation: {violations:?}"))
}

#[test]
fn actionable_gap_outcomes_join_receipt_arrays_anchor_and_unknown_movement() -> Result<(), String> {
    let packets = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gaps",
        "packets": [
            {
                "canonical_gap_id": "gap:anchor-only",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "source_file": "src/anchor.rs",
                "primary_anchor": {"file": "src/anchor.rs", "line": 7},
                "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id anchor-only"
            },
            {
                "canonical_gap_id": "gap:changed-target",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "source_file": "src/target.rs",
                "primary_anchor": {"file": "src/target.rs", "line": 11},
                "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id changed-target"
            }
        ]
    });
    let receipt = serde_json::json!([
        {
            "schema_version": "0.3",
            "seam": {
                "file": "src/anchor.rs",
                "line": 7,
                "before": "weakly_gripped",
                "after": "weakly_gripped"
            },
            "summary": {"next_action": {"kind": "changed"}}
        }
    ]);
    let targeted = serde_json::json!({
        "schema_version": "0.1",
        "moved": [
            {
                "seam_id": "changed-target",
                "seam_kind": "predicate_boundary",
                "file": "src/target.rs",
                "line": 11,
                "before": "weakly_gripped",
                "after": "weakly_gripped",
                "direction": "changed",
                "evidence_delta": []
            }
        ],
        "unchanged": [],
        "regressed": [],
        "removed": [],
        "new": []
    });

    let report = actionable_gap_outcomes_report_from_values(
        &packets,
        Some(&receipt),
        Some(&targeted),
        "target/ripr/reports/actionable-gaps.json".to_string(),
        Some("target/ripr/reports/agent-receipt.json".to_string()),
        Some("target/ripr/reports/targeted-test-outcome.json".to_string()),
    )?;
    let json = actionable_gap_outcomes_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["summary"]["receipt_present"], 1);
    assert_eq!(value["summary"]["attempted_no_receipt"], 1);
    assert_eq!(value["summary"]["unknown"], 0);
    assert_eq!(value["outcomes"][0]["canonical_gap_id"], "gap:anchor-only");
    assert_eq!(value["outcomes"][0]["receipt_state"], "receipt_found");
    assert_eq!(value["outcomes"][0]["outcome_state"], "receipt_present");
    assert_eq!(value["outcomes"][0]["movement_direction"], "changed");
    assert_eq!(
        value["outcomes"][1]["canonical_gap_id"],
        "gap:changed-target"
    );
    assert_eq!(value["outcomes"][1]["receipt_state"], "receipt_missing");
    assert_eq!(
        value["outcomes"][1]["outcome_state"],
        "attempted_no_receipt"
    );
    assert_eq!(
        value["outcomes"][1]["reason"],
        "Matched targeted-test outcome `moved` bucket."
    );
    Ok(())
}

#[test]
fn actionable_gap_outcomes_join_receipts_and_targeted_movement() -> Result<(), String> {
    let packets = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gaps",
        "packets": [
            {
                "canonical_gap_id": "gap:seam-a",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "source_file": "src/pricing.rs",
                "primary_anchor": {"file": "src/pricing.rs", "line": 42},
                "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-a"
            },
            {
                "canonical_gap_id": "gap:seam-b",
                "evidence_class": "error_path",
                "repair_kind": "add_exact_error_variant",
                "source_file": "src/parser.rs",
                "primary_anchor": {"file": "src/parser.rs", "line": 12},
                "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-b"
            },
            {
                "canonical_gap_id": "gap:seam-c",
                "evidence_class": "side_effect",
                "repair_kind": "add_side_effect_observer",
                "source_file": "src/audit.rs",
                "primary_anchor": {"file": "src/audit.rs", "line": 9},
                "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-c"
            },
            {
                "canonical_gap_id": "gap:seam-d",
                "evidence_class": "config_or_policy_constant",
                "repair_kind": "add_output_observer",
                "source_file": "src/config.rs",
                "primary_anchor": {"file": "src/config.rs", "line": 4},
                "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-d"
            },
            {
                "canonical_gap_id": "gap:seam-e",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "source_file": "src/new_gap.rs",
                "primary_anchor": {"file": "src/new_gap.rs", "line": 20},
                "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                "receipt_command_or_path": "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-e"
            }
        ]
    });
    let receipt = serde_json::json!({
        "schema_version": "0.3",
        "verify_result": "pass",
        "seam": {
            "seam_id": "seam-a",
            "file": "src/pricing.rs",
            "line": 42,
            "before": "weakly_gripped",
            "after": "strongly_gripped",
            "change": "improved",
            "evidence_delta": ["missing discriminator no longer reported: threshold equality"]
        },
        "provenance": {
            "seam_id": "seam-a",
            "movement": "improved",
            "generated_at": "unix_ms:2"
        }
    });
    let targeted = serde_json::json!({
        "schema_version": "0.1",
        "generated_at": "unix_ms:3",
        "verify_result": "pass",
        "moved": [],
        "unchanged": [
            {
                "seam_id": "seam-b",
                "seam_kind": "error_path",
                "file": "src/parser.rs",
                "line": 12,
                "before": "weakly_gripped",
                "after": "weakly_gripped",
                "direction": "unchanged",
                "evidence_delta": []
            }
        ],
        "regressed": [],
        "removed": [
            {
                "seam_id": "seam-c",
                "seam_kind": "side_effect",
                "file": "src/audit.rs",
                "line": 9,
                "grip_class": "weakly_gripped"
            }
        ],
        "new": [
            {
                "seam_id": "seam-e",
                "seam_kind": "predicate_boundary",
                "file": "src/new_gap.rs",
                "line": 20,
                "grip_class": "ungripped"
            }
        ]
    });

    let report = actionable_gap_outcomes_report_from_values(
        &packets,
        Some(&receipt),
        Some(&targeted),
        "target/ripr/reports/actionable-gaps.json".to_string(),
        Some("target/ripr/reports/agent-receipt.json".to_string()),
        Some("target/ripr/reports/targeted-test-outcome.json".to_string()),
    )?;
    let states = report
        .outcomes
        .iter()
        .map(|outcome| {
            (
                outcome.canonical_gap_id.as_str(),
                outcome.outcome_state.as_str(),
                outcome.receipt_state.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        vec![
            ("gap:seam-a", "evidence_improved", RECEIPT_MOVEMENT_IMPROVED,),
            ("gap:seam-b", "attempted_no_receipt", RECEIPT_MISSING),
            ("gap:seam-c", "attempted_no_receipt", RECEIPT_MISSING),
            ("gap:seam-d", "not_attempted", RECEIPT_MISSING),
            ("gap:seam-e", "attempted_no_receipt", RECEIPT_MISSING),
        ]
    );

    let json = actionable_gap_outcomes_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["report"], "actionable-gap-outcomes");
    assert_eq!(
        value["outcomes"][0]["receipt_command"],
        "ripr agent receipt --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-a"
    );
    assert_eq!(value["outcomes"][0]["verify_result"], "pass");
    assert_eq!(value["outcomes"][1]["verify_result"], "pass");
    assert_eq!(
        value["outcomes"][3]["verify_result"],
        serde_json::Value::Null
    );
    assert_eq!(value["summary"]["evidence_improved"], 1);
    assert_eq!(value["summary"]["evidence_unchanged"], 0);
    assert_eq!(value["summary"]["resolved"], 0);
    assert_eq!(value["summary"]["not_attempted"], 1);
    assert_eq!(value["summary"]["attempted_no_receipt"], 3);
    assert_eq!(value["summary"]["receipts_present"], 1);
    assert_eq!(value["movement_front"]["current_actionable_count"], 5);
    assert_eq!(
        value["movement_front"]["receipt_linked_actionable_delta"],
        0
    );
    assert_eq!(value["movement_front"]["resolved"], 0);
    assert_eq!(value["movement_front"]["improved"], 1);
    assert_eq!(value["movement_front"]["unchanged_after_attempt"], 0);
    assert_eq!(value["movement_front"]["missing_receipts"], 4);
    assert_eq!(
        value["movement_front"]["top_blocked_reason"],
        "missing_receipts"
    );
    assert_eq!(
        value["outcomes"][0]["reason"],
        "Matched agent receipt artifact."
    );
    assert_eq!(value["outcomes"][0]["timestamp"], "unix_ms:2");
    assert_eq!(
        value["outcomes"][0]["attempt_instance"],
        "timestamp:unix_ms:2"
    );
    assert_eq!(value["outcomes"][1]["timestamp"], "unix_ms:3");
    assert_eq!(
        value["outcomes"][1]["attempt_instance"],
        "timestamp:unix_ms:3"
    );
    assert_eq!(
        value["outcomes"][3]["attempt_instance"],
        serde_json::Value::Null
    );
    let markdown = actionable_gap_outcomes_markdown(&report);
    assert!(markdown.contains("# Actionable Gap Outcomes"));
    assert!(markdown.contains("## Movement Since Prior Refresh"));
    assert!(markdown.contains("| Current actionable count | 5 |"));
    assert!(markdown.contains("| Receipt-linked actionable delta | 0 |"));
    assert!(markdown.contains("| Unchanged after attempt | 0 |"));
    assert!(markdown.contains("| Missing receipts | 4 |"));
    assert!(markdown.contains("| Top blocked reason | missing_receipts |"));
    assert!(markdown.contains("gap:seam-c"));
    assert!(markdown.contains("attempted_no_receipt"));
    Ok(())
}

#[test]
fn actionable_gap_outcomes_render_empty_packet_set_as_no_action() -> Result<(), String> {
    let packets = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gaps",
        "packets": []
    });
    let report = actionable_gap_outcomes_report_from_values(
        &packets,
        None,
        None,
        "target/ripr/reports/actionable-gaps.json".to_string(),
        None,
        None,
    )?;
    let json = actionable_gap_outcomes_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["packets_total"], 0);
    assert_eq!(value["summary"]["outcomes_total"], 0);

    let markdown = actionable_gap_outcomes_markdown(&report);
    assert!(markdown.contains("No actionable-gap packets were present."));
    Ok(())
}

/// #4234: an agent receipt's `verification.status: "verification_not_run"`
/// says no command ran. The audit must count that as a missing verify result
/// and fall back to the targeted-test outcome, not report "verification_not_run"
/// as the attempt's verify result.
#[test]
fn actionable_gap_outcomes_treat_receipt_verification_not_run_as_missing() -> Result<(), String> {
    let packets = serde_json::json!({
        "packets": [{
            "canonical_gap_id": "gap:seam-a",
            "evidence_class": "predicate_boundary",
            "repair_kind": "add_boundary_assertion",
            "source_file": "src/pricing.rs",
            "verify_command": "ripr agent verify --root . --json"
        }]
    });
    let receipt = serde_json::json!({
        "schema_version": "0.5",
        "seam": {"seam_id": "seam-a", "file": "src/pricing.rs", "line": 42,
                 "before": "weakly_gripped", "after": "strongly_gripped", "change": "improved"},
        "provenance": {"seam_id": "seam-a", "movement": "improved", "generated_at": "unix_ms:2"},
        "verification": {"status": "verification_not_run", "commands_run": [],
                         "non_claims": ["static_only_assurance"]}
    });
    let report = actionable_gap_outcomes_report_from_values(
        &packets,
        Some(&receipt),
        None,
        "target/ripr/reports/actionable-gaps.json".to_string(),
        Some("target/ripr/reports/agent-receipt.json".to_string()),
        None,
    )?;
    let json = actionable_gap_outcomes_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["outcomes"][0]["receipt_state"],
        RECEIPT_MOVEMENT_IMPROVED
    );
    assert_eq!(
        value["outcomes"][0]["verify_result"],
        serde_json::Value::Null
    );
    assert_eq!(
        crate::actionable_gap_outcomes_missing_verify_result_count(&value),
        1
    );

    // Alternate control: a real status is still the verify result, so the
    // filter drops only the not-run sentinel.
    let mut ran = receipt.clone();
    ran["verification"]["status"] = serde_json::json!("passed");
    let report = actionable_gap_outcomes_report_from_values(
        &packets,
        Some(&ran),
        None,
        "target/ripr/reports/actionable-gaps.json".to_string(),
        Some("target/ripr/reports/agent-receipt.json".to_string()),
        None,
    )?;
    let value: serde_json::Value = serde_json::from_str(&actionable_gap_outcomes_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["outcomes"][0]["verify_result"], "passed");
    assert_eq!(
        crate::actionable_gap_outcomes_missing_verify_result_count(&value),
        0
    );
    Ok(())
}

#[test]
fn active_yaml_lines_strips_comments_and_blanks() {
    let input = "\
# commented out
key: value
  # indented comment

active: true
";
    let lines = active_yaml_lines(input);
    assert_eq!(lines, vec!["key: value", "active: true"]);
}

#[test]
fn agent_seam_packet_command_args_use_requested_root_and_format() {
    let args =
        repo_seam_inventory_command_args_for_root("agent-seam-packets-json", "fixtures/demo");
    assert_eq!(
        args,
        vec![
            "run",
            "-p",
            "ripr",
            "--quiet",
            "--",
            "check",
            "--root",
            "fixtures/demo",
            "--format",
            "agent-seam-packets-json",
        ]
    );
    assert!(known_xtask_command("agent-seam-packets"));
}

#[test]
fn allow_attribute_budget_fails_stale_shrunk_and_orphaned_rows() {
    let guarded = guarded_allow_attribute_lints();
    let mut allowlist = BTreeMap::new();
    allowlist.insert(("a.rs".to_string(), "allow(dead_code)".to_string()), 3);
    allowlist.insert(("b.rs".to_string(), "allow(dead_code)".to_string()), 1);
    allowlist.insert(("c.rs".to_string(), "expect(dead_code)".to_string()), 2);
    allowlist.insert(
        (
            "d.rs".to_string(),
            "allow(clippy::module_name_repetitions)".to_string(),
        ),
        2,
    );
    let mut counts = BTreeMap::new();
    counts.insert(
        ("a.rs".to_string(), "allow(dead_code)".to_string()),
        vec![10, 20],
    );
    counts.insert(
        ("c.rs".to_string(), "expect(dead_code)".to_string()),
        vec![4, 8],
    );

    let violations = allow_attribute_budget_violations(&allowlist, &counts, &guarded);
    let stale = |path: &str| {
        violations
            .iter()
            .any(|violation| violation.contains(path) && violation.contains("count is stale"))
    };
    assert!(stale("a.rs"), "shrunk row should fail: {violations:?}");
    assert!(
        violations.iter().any(|violation| {
            violation.contains("a.rs") && violation.contains("found 2, allowed 3")
        }),
        "shrunk row should name both counts: {violations:?}"
    );
    assert!(stale("b.rs"), "orphaned row should fail: {violations:?}");
    assert!(
        violations.iter().any(|violation| {
            violation.contains("b.rs") && violation.contains("found 0, allowed 1")
        }),
        "orphaned row should name a zero count: {violations:?}"
    );
    assert!(
        !violations
            .iter()
            .any(|violation| violation.contains("c.rs")),
        "exact row should pass: {violations:?}"
    );
    assert!(
        violations.iter().any(|violation| {
            violation.contains("d.rs") && violation.contains("unsupported guarded attribute")
        }),
        "unsupported row should still fail: {violations:?}"
    );
    assert!(
        !violations
            .iter()
            .any(|violation| violation.contains("d.rs") && violation.contains("count is stale")),
        "unsupported row is not also a stale-count failure: {violations:?}"
    );
}

#[test]
fn allow_attribute_budget_still_fails_when_source_exceeds_the_row() {
    let guarded = guarded_allow_attribute_lints();
    let mut allowlist = BTreeMap::new();
    allowlist.insert(("a.rs".to_string(), "allow(dead_code)".to_string()), 1);
    let mut counts = BTreeMap::new();
    counts.insert(
        ("a.rs".to_string(), "allow(dead_code)".to_string()),
        vec![3, 9],
    );

    let violations = allow_attribute_budget_violations(&allowlist, &counts, &guarded);
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("a.rs:3,9") && violation.contains("allowed 1")),
        "over-budget source should still fail with its lines: {violations:?}"
    );
    assert!(
        !violations
            .iter()
            .any(|violation| violation.contains("count is stale")),
        "over-budget is not a stale-count failure: {violations:?}"
    );
}

#[test]
fn allow_attribute_detection_counts_lines_after_many_rejected_hashes() {
    let mut text = format!("// 🦀 # # # {}\n", concat!("#", "[cfg(any())]")).repeat(128);
    text.push_str(concat!("#", "[allow(dead_code, warnings, warnings)]"));

    let findings = guarded_allow_attributes_in_text(&text, &guarded_allow_attribute_lints());

    assert_eq!(
        findings,
        vec![
            (129, "allow(dead_code)".to_string()),
            (129, "allow(warnings)".to_string()),
            (129, "allow(warnings)".to_string()),
        ]
    );
}

#[test]
fn allow_attribute_detection_flags_guarded_lint_suppressions() {
    let text = format!(
        "{}\nfn f() {{}}\n{}",
        concat!("#[", "allow", "(clippy::unwrap_used, dead_code)]"),
        concat!("#![", "expect", "(warnings, reason = \"temporary\")]")
    );

    let findings = guarded_allow_attributes_in_text(&text, &guarded_allow_attribute_lints());

    assert_eq!(
        findings,
        vec![
            (1, "allow(clippy::unwrap_used)".to_string()),
            (1, "allow(dead_code)".to_string()),
            (3, concat!("expect", "(warnings)").to_string()),
        ]
    );
}

#[test]
fn allow_attribute_detection_ignores_untracked_lints() {
    let text = format!(
        "{}\nfn f() {{}}\n",
        concat!("#[", "allow", "(clippy::module_name_repetitions)]")
    );

    let findings = guarded_allow_attributes_in_text(&text, &guarded_allow_attribute_lints());

    assert!(findings.is_empty());
}

#[test]
fn allow_attribute_detection_keeps_malformed_and_unguarded_shapes_ignored() {
    for text in [
        "#",
        "#!",
        "#[]",
        concat!("#", "[allow]"),
        concat!("#", "[allow(warnings"),
        concat!("#", "[allows(warnings)]"),
        concat!("#", "[expect = (warnings)]"),
        concat!("#", "[allow(warning)]"),
        concat!("#", "[allow(reason = \"warnings\")]"),
        concat!("#", "[allow(clippy::module_name_repetitions)]"),
    ] {
        assert!(
            guarded_allow_attributes_in_text(text, &guarded_allow_attribute_lints()).is_empty(),
            "unexpected finding in {text:?}",
        );
    }
}

#[test]
fn allow_attribute_detection_preserves_literal_multiline_byte_lines() {
    let text = format!(
        "// 🦀 # noise\n{}\nconst HASH: &str = \"#\";\n{}\n dead_code,\n warnings,\n clippy::module_name_repetitions,\n reason = \"🦀 #\")]\nfn f() {{}}\n{}\n// {}\n",
        concat!("#", "[cfg(any())]"),
        concat!("#", "[allow("),
        concat!("#", "![expect(unused_imports)]"),
        concat!("#", "[allow(unsafe_code)]"),
    );

    let findings = guarded_allow_attributes_in_text(&text, &guarded_allow_attribute_lints());

    // The existing text scanner also recognizes attribute text in comments.
    assert_eq!(
        findings,
        vec![
            (4, "allow(dead_code)".to_string()),
            (4, "allow(warnings)".to_string()),
            (10, "expect(unused_imports)".to_string()),
            (11, "allow(unsafe_code)".to_string()),
        ]
    );
}

#[test]
fn artifact_router_inputs_never_copy_build_output_from_the_checkout() {
    // A declared-absent `target/...` input stays absent even when an earlier
    // run left a file at that path in the checkout.
    let absent = ArtifactRouterInput::at(
        "assistant-proof",
        "target/ripr/reports/test-oracle-assistant-proof.json",
    );
    assert_eq!(absent.copy_source(), None);

    let committed = ArtifactRouterInput::at("ledger", "fixtures/x/ledger.json");
    assert_eq!(
        committed.copy_source(),
        Some(Path::new("fixtures/x/ledger.json"))
    );
    let sourced = ArtifactRouterInput::from(
        "ledger",
        "target/ripr/reports/pr-evidence-ledger.json",
        "fixtures/x/inputs/pr-evidence-ledger.json",
    );
    assert_eq!(
        sourced.copy_source(),
        Some(Path::new("fixtures/x/inputs/pr-evidence-ledger.json"))
    );
}

#[test]
fn artifact_router_path_violation_rejects_paths_outside_the_render_root() {
    assert_eq!(
        artifact_router_path_violation("first-useful-action", "target/ripr/reports/x.json"),
        None
    );

    // Each input is copied to `<render root>/<path>`, so any of these would
    // write outside the scratch directory the case owns.
    for path in ["", "/etc/x.json", "../x.json", "a/../../x.json", "./x.json"] {
        assert!(
            artifact_router_path_violation("first-useful-action", path).is_some(),
            "`{path}` should be rejected as a render-root input path"
        );
    }
}

#[test]
fn assistant_loop_health_fixture_corpus_guard_accepts_complete_contract() -> Result<(), String> {
    let root = temp_dir("assistant-loop-health-valid");
    let base = root.join("assistant-loop-health");
    let report = root.join("assistant-loop-health.json");
    let markdown = root.join("assistant-loop-health.md");
    let proof = root.join("proof.json");
    write_assistant_loop_health_corpus(&base, &report, &markdown, &proof, "advisory", 0);
    write(
        &report,
        r#"{
  "kind": "assistant_loop_health",
  "status": "advisory",
  "summary": {
    "proofs": 1,
    "complete": 1,
    "partial": 0,
    "missing_required_input": 0,
    "missing_optional_input": 0,
    "improved": 1,
    "unchanged": 0,
    "regressed": 0,
    "unknown_movement": 0,
    "warnings": 0,
    "repair_queue": 0
  },
  "repair_queue": [],
  "limits": ["Static RIPR evidence only."]
}
"#,
    );
    write(
        &markdown,
        "# RIPR Assistant Loop Health\n\nStatus: advisory\n",
    );

    let mut violations = Vec::new();
    super::super::validate_assistant_loop_health_fixture_corpus_at(&base, &mut violations)?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn assistant_loop_health_fixture_corpus_guard_reports_contract_drift() -> Result<(), String> {
    let root = temp_dir("assistant-loop-health-invalid");
    let base = root.join("assistant-loop-health");
    let report = root.join("assistant-loop-health.json");
    let markdown = root.join("assistant-loop-health.md");
    let proof = root.join("proof.json");
    write_assistant_loop_health_corpus(&base, &report, &markdown, &proof, "advisory", 1);
    write(
        &report,
        r#"{
  "kind": "wrong_kind",
  "status": "blocked",
  "summary": {
    "proofs": 1,
    "complete": 0,
    "partial": 0,
    "missing_required_input": 0,
    "missing_optional_input": 0,
    "improved": 0,
    "unchanged": 0,
    "regressed": 0,
    "unknown_movement": 0,
    "warnings": 0,
    "repair_queue": 1
  },
  "proofs": [{"seam": {"static_class": "weakly_gripped"}}],
  "repair_queue": [{"reason": "missing kind"}],
  "limits": ["Does not run mutation testing."]
}
"#,
    );
    write(
        &markdown,
        "# RIPR Assistant Loop Health\n\nStatus: blocked\n\nNext repair queue:\n- inspect this\n",
    );

    let mut violations = Vec::new();
    super::super::validate_assistant_loop_health_fixture_corpus_at(&base, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("report kind must be assistant_loop_health"));
    assert!(report.contains("expected status advisory"));
    assert!(report.contains("must use grip_class, not static_class"));
    assert!(report.contains("expected complete=1, got 0"));
    assert!(report.contains("repair_queue entries must include repair_kind"));
    assert!(report.contains("report is missing static evidence limit"));
    assert!(report.contains("Markdown must pin status advisory"));
    assert!(report.contains("Markdown repair queue must include repair_kind"));
    Ok(())
}

#[test]
fn assistant_loop_health_missing_corpus_fails_both_shared_gates() -> Result<(), String> {
    let root = temp_dir("assistant-loop-health-missing");
    let base = root.join("assistant-loop-health");

    let mut fixture_contract_violations = Vec::new();
    super::super::validate_assistant_loop_health_fixture_corpus_at(
        &base,
        &mut fixture_contract_violations,
    )?;
    assert_eq!(
        fixture_contract_violations,
        vec![format!(
            "assistant-loop-health corpus is missing {}",
            super::super::normalize_path(&base)
        )]
    );

    let golden_violations =
        super::super::golden_assistant_loop_health_contract_violations_at(&base)?;
    assert_eq!(golden_violations, fixture_contract_violations);
    Ok(())
}

#[test]
fn bdd_name_helper_accepts_given_when_then_pattern() {
    assert!(is_bdd_test_name(
        "given_invalid_token_when_authenticate_then_returns_revoked_error"
    ));
    assert!(!is_bdd_test_name(
        "given_invalid_token_then_returns_revoked_error_when_authenticate"
    ));
    assert!(!is_bdd_test_name(
        "given__when_authenticate_then_returns_revoked_error"
    ));
    assert!(!is_bdd_test_name(
        "given_invalid_token_when__then_returns_revoked_error"
    ));
    assert!(!is_bdd_test_name("authenticate_rejects_invalid_token"));
}

#[test]
fn bun_ub_calibration_command_writes_markdown_and_json() -> Result<(), String> {
    with_temp_cwd("bun-ub-calibration-report", |_root| {
        let out = PathBuf::from("target/ripr/reports/bun-ub-calibration.json");
        let out_md = PathBuf::from("target/ripr/reports/bun-ub-calibration.md");
        let corpus = repo_root()?.join(super::super::TYPESCRIPT_BUN_UB_CALIBRATION_CORPUS);
        super::super::bun_ub_calibration_impl(&[
            "--corpus".to_string(),
            corpus.to_string_lossy().into_owned(),
            "--out".to_string(),
            out.to_string_lossy().into_owned(),
            "--out-md".to_string(),
            out_md.to_string_lossy().into_owned(),
        ])?;

        let json = fs::read_to_string(&out)
            .map_err(|err| format!("failed to read Bun UB calibration JSON: {err}"))?;
        let value: Value =
            serde_json::from_str(&json).map_err(|err| format!("invalid JSON: {err}"))?;
        assert_eq!(value["report"], "bun-ub-calibration");
        assert_eq!(value["summary"]["cases_total"], serde_json::Value::from(7));
        assert_eq!(
            value["summary"]["repair_packet_ready_cases"],
            serde_json::Value::from(0)
        );

        let markdown = fs::read_to_string(&out_md)
            .map_err(|err| format!("failed to read Bun UB calibration Markdown: {err}"))?;
        assert!(markdown.contains("# Bun UB TypeScript Calibration"));
        assert!(markdown.contains("bridge_unknown"));
        Ok(())
    })
}

#[test]
fn bun_ub_calibration_report_summarizes_calibrated_states() -> Result<(), String> {
    with_repo_cwd(|| {
        let report = super::super::bun_ub_calibration_report_value(Path::new(
            super::super::TYPESCRIPT_BUN_UB_CALIBRATION_CORPUS,
        ));
        assert_eq!(report["schema_version"], "0.1");
        assert_eq!(report["report"], "bun-ub-calibration");
        assert_eq!(report["status"], "pass");
        assert_eq!(report["summary"]["cases_total"], serde_json::Value::from(7));
        assert_eq!(
            report["summary"]["ts_discriminated_cases"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["ts_missing_resizable_cases"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["ts_missing_shared_cases"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["ts_missing_shared_and_resizable_cases"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["ts_missing_external_oracle_cases"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["ts_mention_not_observer_cases"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["bridge_unknown_cases"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["repair_packet_ready_cases"],
            serde_json::Value::from(0)
        );

        let rows = report["rows"]
            .as_array()
            .ok_or_else(|| "Bun UB calibration rows must be an array".to_string())?;
        let known_good = rows
            .iter()
            .find(|row| row["case_id"] == "bun_blob_shared_and_resizable_present")
            .ok_or_else(|| "missing known-good Bun Blob row".to_string())?;
        assert_eq!(known_good["observed_state"], "ts_discriminated");
        assert_eq!(known_good["status"], "pass");
        assert_eq!(known_good["repair_packet_ready"], false);

        let missing_resizable = rows
            .iter()
            .find(|row| row["case_id"] == "bun_blob_resizable_missing")
            .ok_or_else(|| "missing stripped-resizable row".to_string())?;
        assert_eq!(missing_resizable["observed_state"], "ts_missing_resizable");
        assert!(
            missing_resizable["missing_discriminators"]
                .as_array()
                .is_some_and(|missing| missing.iter().any(|item| item == "resizable_array_buffer"))
        );
        assert!(
            missing_resizable["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| {
                    legs.iter()
                        .any(|item| item == "boundary_discriminator:resizable_array_buffer")
                })
        );
        assert_eq!(
            missing_resizable["suggested_test_file"],
            super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE
        );

        let mention_only = rows
            .iter()
            .find(|row| row["case_id"] == "bun_blob_max_byte_length_mention_not_observer")
            .ok_or_else(|| "missing mention-only row".to_string())?;
        assert_eq!(mention_only["observed_state"], "ts_mention_not_observer");
        assert!(
            mention_only["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| {
                    legs.iter()
                        .any(|item| item == "external_blob_or_stable_byte_observer")
                })
        );

        let missing_external_oracle = rows
            .iter()
            .find(|row| row["case_id"] == "bun_blob_partial_observer_missing_external_oracle")
            .ok_or_else(|| "missing partial-observer row".to_string())?;
        assert_eq!(
            missing_external_oracle["observed_state"],
            "ts_missing_external_oracle"
        );
        assert_eq!(
            missing_external_oracle["suggested_test_file"],
            "not_applicable"
        );
        assert!(
            missing_external_oracle["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs
                    .iter()
                    .any(|item| item == "external_oracle:stable_byte_copy"))
        );

        let bridge_unknown = rows
            .iter()
            .find(|row| row["case_id"] == "bun_blob_bridge_unknown_without_hint")
            .ok_or_else(|| "missing bridge-unknown row".to_string())?;
        assert_eq!(bridge_unknown["observed_state"], "bridge_unknown");
        assert!(
            bridge_unknown["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs.iter().any(|item| item == "binding_or_ffi_edge"))
        );

        let markdown = super::super::bun_ub_calibration_report_markdown(&report);
        assert!(markdown.contains("# Bun UB TypeScript Calibration"));
        assert!(markdown.contains("Bun UB TypeScript calibration is preview/advisory only"));
        assert!(markdown.contains("bun_blob_resizable_missing"));
        assert!(markdown.contains("Missing external oracle cases"));
        assert!(markdown.contains("binding_or_ffi_edge"));
        assert!(markdown.contains("Repair-packet-ready cases"));
        Ok(())
    })
}

#[test]
fn bun_ub_preview_summary_accepts_custom_paths_and_rejects_bad_args() -> Result<(), String> {
    let args = super::super::parse_bun_ub_preview_summary_args(&[
        "--calibration-corpus".to_string(),
        "calibration.json".to_string(),
        "--graph-corpus".to_string(),
        "graph.json".to_string(),
        "--dogfood-corpus".to_string(),
        "dogfood.json".to_string(),
        "--out".to_string(),
        "summary.json".to_string(),
        "--out-md".to_string(),
        "summary.md".to_string(),
    ])?;
    assert_eq!(args.calibration_corpus, PathBuf::from("calibration.json"));
    assert_eq!(args.graph_corpus, PathBuf::from("graph.json"));
    assert_eq!(args.dogfood_corpus, PathBuf::from("dogfood.json"));
    assert_eq!(args.out, PathBuf::from("summary.json"));
    assert_eq!(args.out_md, PathBuf::from("summary.md"));

    let missing_value =
        super::super::parse_bun_ub_preview_summary_args(&["--out".to_string()]).unwrap_err();
    assert!(missing_value.contains("missing value for `--out`"));
    assert!(missing_value.contains("usage: cargo xtask bun-ub-preview-summary"));

    let unknown =
        super::super::parse_bun_ub_preview_summary_args(&["--unknown".to_string()]).unwrap_err();
    assert!(unknown.contains("unknown bun-ub-preview-summary argument `--unknown`"));

    let help =
        super::super::parse_bun_ub_preview_summary_args(&["--help".to_string()]).unwrap_err();
    assert!(help.contains("usage: cargo xtask bun-ub-preview-summary"));
    Ok(())
}

#[test]
fn bun_ub_preview_summary_command_writes_markdown_and_json() -> Result<(), String> {
    with_temp_cwd("bun-ub-preview-summary-report", |_root| {
        let out = PathBuf::from("target/ripr/reports/bun-ub-preview-summary.json");
        let out_md = PathBuf::from("target/ripr/reports/bun-ub-preview-summary.md");
        super::super::bun_ub_preview_summary_impl(&[])?;

        let json = fs::read_to_string(&out)
            .map_err(|err| format!("failed to read Bun UB preview summary JSON: {err}"))?;
        let value: Value =
            serde_json::from_str(&json).map_err(|err| format!("invalid JSON: {err}"))?;
        assert_eq!(value["report"], "bun-ub-preview-summary");
        assert_eq!(value["authority"], "preview_advisory_only");
        assert_eq!(
            value["summary"]["repair_packet_ready_cases"],
            serde_json::Value::from(0)
        );

        let markdown = fs::read_to_string(&out_md)
            .map_err(|err| format!("failed to read Bun UB preview summary Markdown: {err}"))?;
        assert!(markdown.contains("# Bun UB Preview Summary"));
        assert!(markdown.contains("authority = preview_advisory_only"));
        assert!(markdown.contains("repair_packet_ready: false"));
        Ok(())
    })
}

#[test]
fn bun_ub_preview_summary_fails_closed_on_public_repair_claims() {
    let report = serde_json::json!({
        "status": "fail",
        "authority": "preview_advisory_only",
        "authority_boundary": "preview_advisory_only",
        "repair_packet_ready": false,
        "source_paths": {},
        "summary": {
            "calibration_cases_total": 0,
            "route_quality_cases_total": 0,
            "dogfood_receipts_total": 0,
            "route_state_counts": {"future_state": 2},
            "dogfood_state_counts": {"future_state": 1},
            "named_static_limitations": [],
            "public_packet_exclusions": 0,
            "repair_packet_ready_cases": 1
        },
        "calibrated_routes": [],
        "dogfood_receipts": [],
        "non_claims": [],
        "errors": ["repair_packet_ready_cases must remain 0, got 1"]
    });
    let markdown = super::super::bun_ub_preview_summary_markdown(&report);
    assert!(markdown.contains("Status: `fail`"));
    assert!(markdown.contains("| `future_state` | 2 | 1 |"));
    assert!(markdown.contains("| none | 0 | none | none |"));
    assert!(markdown.contains("| none |  |  |  |  |  |  |"));
    assert!(markdown.contains("- none"));
    assert_eq!(
        report["errors"][0],
        "repair_packet_ready_cases must remain 0, got 1"
    );
}

#[test]
fn bun_ub_preview_summary_summarizes_existing_cross_language_state() -> Result<(), String> {
    with_repo_cwd(|| {
        let report = super::super::bun_ub_preview_summary_report_value(
            &super::super::BunUbPreviewSummaryArgs {
                calibration_corpus: PathBuf::from(
                    super::super::TYPESCRIPT_BUN_UB_CALIBRATION_CORPUS,
                ),
                graph_corpus: super::super::cross_language_oracle_graph_corpus_path(),
                dogfood_corpus: PathBuf::from(super::super::BUN_UB_CROSS_LANGUAGE_DOGFOOD_CORPUS),
                out: PathBuf::from("target/ripr/reports/bun-ub-preview-summary.json"),
                out_md: PathBuf::from("target/ripr/reports/bun-ub-preview-summary.md"),
            },
        );
        assert_eq!(report["schema_version"], "0.1");
        assert_eq!(report["report"], "bun-ub-preview-summary");
        assert_eq!(report["status"], "pass");
        assert_eq!(report["authority"], "preview_advisory_only");
        assert_eq!(report["authority_boundary"], "preview_advisory_only");
        assert_eq!(report["repair_packet_ready"], false);
        assert_eq!(
            report["summary"]["route_state_counts"]["rust_ungripped_ts_discriminated"],
            serde_json::Value::from(3)
        );
        assert_eq!(
            report["summary"]["route_state_counts"]["rust_ungripped_ts_missing_discriminator"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["route_state_counts"]["bridge_unknown"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["route_state_counts"]["ts_mention_not_observer"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["route_state_counts"]["named_static_limitation"],
            serde_json::Value::from(2)
        );
        assert_eq!(
            report["summary"]["repair_packet_ready_cases"],
            serde_json::Value::from(0)
        );

        let limitations = report["summary"]["named_static_limitations"]
            .as_array()
            .ok_or_else(|| "named_static_limitations must be an array".to_string())?;
        let categories = limitations
            .iter()
            .filter_map(|row| row["category"].as_str())
            .collect::<BTreeSet<_>>();
        for required in [
            "cross_language_oracle_visibility_unresolved",
            "cross_language_target_unresolved",
            "cross_language_panic_boundary_visibility_unresolved",
            "cross_language_profile_manifest_only",
        ] {
            assert!(
                categories.contains(required),
                "Bun UB preview summary should include limitation category {required}"
            );
        }

        let routes = report["calibrated_routes"]
            .as_array()
            .ok_or_else(|| "calibrated_routes must be an array".to_string())?;
        for required in [
            "bun_markdown_resizable_array_buffer_configured_bridge_advisory",
            "bun_ffi_negative_offset_panic_boundary_limitation",
            "bun_node_fs_scalar_write_manifest_only_profile",
            "bun_write_helper_gated_manifest_only_profile",
        ] {
            assert!(
                routes.iter().any(|row| row["route_label"] == required),
                "Bun UB preview summary should include route {required}"
            );
        }
        let missing_resizable = routes
            .iter()
            .find(|row| row["route_label"] == "bun_blob_missing_resizable_oracle_limitation")
            .ok_or_else(|| "missing resizable route should be summarized".to_string())?;
        assert_eq!(
            missing_resizable["suggested_test_file"],
            super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE
        );
        assert_eq!(missing_resizable["repair_packet_ready"], false);

        let markdown = super::super::bun_ub_preview_summary_markdown(&report);
        assert!(markdown.contains("# Bun UB Preview Summary"));
        assert!(markdown.contains("authority = preview_advisory_only"));
        assert!(markdown.contains("repair_packet_ready: false"));
        assert!(markdown.contains("rust_ungripped_ts_discriminated"));
        assert!(markdown.contains("rust_ungripped_ts_missing_discriminator"));
        assert!(markdown.contains("bridge_unknown"));
        assert!(markdown.contains("ts_mention_not_observer"));
        assert!(markdown.contains("named_static_limitation"));
        assert!(markdown.contains("cross_language_oracle_visibility_unresolved"));
        assert!(markdown.contains("cross_language_profile_manifest_only"));
        Ok(())
    })
}

#[test]
fn capabilities_accept_usable_alpha_status_with_fixture() -> Result<(), String> {
    with_temp_cwd("capabilities-usable-alpha", |root| {
        write(
            &root.join("docs/specs/RIPR-SPEC-0028-python.md"),
            "Status: accepted\n",
        );
        write(
            &root.join("fixtures/python-real-repo-evals/SPEC.md"),
            "Python real-repo eval fixture\n",
        );
        let manifest = r#"
[[capability]]
id = "python_repair_routing_loop"
name = "Python repair routing loop"
status = "usable alpha"
spec = "RIPR-SPEC-0028"
evidence = ["fixture-backed repair routing proof"]
fixtures = ["fixtures/python-real-repo-evals"]
next = "dogfood/python-stability-evals-v1"
metric = "language_adapter_python_repair_routing_quality_metrics"
"#;

        let (capabilities, parse_violations) =
            super::super::parse_capabilities_manifest_text("metrics/capabilities.toml", manifest);
        assert!(parse_violations.is_empty(), "{parse_violations:#?}");
        let mut violations = Vec::new();
        super::super::validate_capabilities(&capabilities, &mut violations)?;
        assert!(violations.is_empty(), "{violations:#?}");
        Ok(())
    })
}

#[test]
fn capabilities_require_fixture_for_usable_alpha_status() -> Result<(), String> {
    with_temp_cwd("capabilities-usable-alpha-fixture", |root| {
        write(
            &root.join("docs/specs/RIPR-SPEC-0028-python.md"),
            "Status: accepted\n",
        );
        let manifest = r#"
[[capability]]
id = "python_repair_routing_loop"
name = "Python repair routing loop"
status = "usable alpha"
spec = "RIPR-SPEC-0028"
evidence = ["fixture-backed repair routing proof"]
next = "dogfood/python-stability-evals-v1"
metric = "language_adapter_python_repair_routing_quality_metrics"
"#;

        let (capabilities, parse_violations) =
            super::super::parse_capabilities_manifest_text("metrics/capabilities.toml", manifest);
        assert!(parse_violations.is_empty(), "{parse_violations:#?}");
        let mut violations = Vec::new();
        super::super::validate_capabilities(&capabilities, &mut violations)?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("usable alpha or stable but has no fixture entries")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn capability_helpers_validate_ids_and_escape_json() {
    assert!(is_snake_case_id("agent_context_v2"));
    assert!(!is_snake_case_id("AgentContextV2"));
    assert!(!is_snake_case_id("agent__context"));
    assert_eq!(json_escape("a\"b\\c\n"), "a\\\"b\\\\c\\n");

    let capability = Capability {
        id: Some("agent_context_v2".to_string()),
        name: Some("Agent context v2".to_string()),
        status: Some("planned".to_string()),
        spec: Some("RIPR-SPEC-0003".to_string()),
        evidence: vec!["agent context spec".to_string()],
        fixtures: Vec::new(),
        next: Some("agent-context-v2".to_string()),
        metric: Some("context packets with suggested assertions".to_string()),
        line: 1,
    };
    let json = super::super::capability_metrics_json(&[capability]);
    assert!(json.contains("\"id\": \"agent_context_v2\""));
    assert!(json.contains("\"schema_version\": \"0.1\""));
}

#[test]
fn capability_matrix_accepts_manifest_names() -> Result<(), String> {
    with_temp_cwd("capability-matrix-aligned", |root| {
        write(
            &root.join("docs/CAPABILITY_MATRIX.md"),
            "| Capability | Status | Spec | Current evidence | Next checkpoint | Metric |\n| --- | --- | --- | --- | --- | --- |\n| Static exposure loop | `alpha` | `RIPR-SPEC-0001` | fixture | `next` | metric |\n\n| Unknown stop reasons | `alpha` | `RIPR-SPEC-0001` | fixture | `next` | metric |\n",
        );
        let capabilities = vec![
            Capability {
                name: Some("Static exposure loop".to_string()),
                ..Capability::default()
            },
            Capability {
                name: Some("Unknown stop reasons".to_string()),
                ..Capability::default()
            },
        ];
        let mut violations = Vec::new();
        super::super::validate_capability_matrix(&capabilities, &mut violations)?;
        assert!(violations.is_empty(), "{violations:#?}");
        Ok(())
    })
}

#[test]
fn capability_matrix_rejects_stale_or_duplicate_names() -> Result<(), String> {
    with_temp_cwd("capability-matrix-drift", |root| {
        write(
            &root.join("docs/CAPABILITY_MATRIX.md"),
            "| Capability | Status | Spec | Current evidence | Next checkpoint | Metric |\n| --- | --- | --- | --- | --- | --- |\n| Static exposure loop | `alpha` | `RIPR-SPEC-0001` | fixture | `next` | metric |\n| Static exposure loop | `alpha` | `RIPR-SPEC-0001` | fixture | `next` | metric |\n| Retired capability | `alpha` | `RIPR-SPEC-0001` | fixture | `next` | metric |\n",
        );
        let capabilities = vec![
            Capability {
                name: Some("Static exposure loop".to_string()),
                ..Capability::default()
            },
            Capability {
                name: Some("Unknown stop reasons".to_string()),
                ..Capability::default()
            },
        ];
        let mut violations = Vec::new();
        super::super::validate_capability_matrix(&capabilities, &mut violations)?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("missing capability `Unknown stop reasons`")
            }),
            "{violations:#?}"
        );
        assert!(
            violations.iter().any(|violation| {
                violation.contains("contains capability `Retired capability` absent")
            }),
            "{violations:#?}"
        );
        assert!(
            violations.iter().any(|violation| {
                violation.contains("contains capability `Static exposure loop` 2 times")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn capability_matrix_reports_missing_structure() -> Result<(), String> {
    with_temp_cwd("capability-matrix-missing", |_root| -> Result<(), String> {
        let capabilities = vec![Capability {
            name: Some("Static exposure loop".to_string()),
            ..Capability::default()
        }];
        let mut violations = Vec::new();
        super::super::validate_capability_matrix(&capabilities, &mut violations)?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("docs/CAPABILITY_MATRIX.md is missing")),
            "{violations:#?}"
        );
        Ok(())
    })?;

    with_temp_cwd(
        "capability-matrix-missing-header",
        |root| -> Result<(), String> {
            write(
                &root.join("docs/CAPABILITY_MATRIX.md"),
                "not a capability table\n",
            );
            let capabilities = vec![Capability {
                name: Some("Static exposure loop".to_string()),
                ..Capability::default()
            }];
            let mut violations = Vec::new();
            super::super::validate_capability_matrix(&capabilities, &mut violations)?;
            assert!(
                violations.iter().any(|violation| violation
                    .contains("docs/CAPABILITY_MATRIX.md is missing the capability table header")),
                "{violations:#?}"
            );
            Ok(())
        },
    )?;

    with_temp_cwd(
        "capability-matrix-nameless-row",
        |root| -> Result<(), String> {
            write(
                &root.join("docs/CAPABILITY_MATRIX.md"),
                "| Capability | Status | Spec | Current evidence | Next checkpoint | Metric |\n| --- | --- | --- | --- | --- | --- |\n| | `alpha` | `RIPR-SPEC-0001` | fixture | `next` | metric |\n",
            );
            let mut violations = Vec::new();
            super::super::validate_capability_matrix(&[], &mut violations)?;
            assert!(
            violations
                .iter()
                .any(|violation| violation.contains("contains a capability row without a name")),
            "{violations:#?}"
        );
            Ok(())
        },
    )
}

#[test]
fn capability_next_checkpoint_parser_reads_source_values() {
    let source = "next = \"fixture-laboratory\"\nmetric = \"fixture pass rate\"\nnext = \"agent-context-v2\"\n";

    assert_eq!(
        next_checkpoints_from_capabilities(source),
        Ok(vec![
            "fixture-laboratory".to_string(),
            "agent-context-v2".to_string()
        ])
    );
}

#[test]
fn ci_enforced_xtask_invocations_separates_enforced_from_advisory() {
    let workflow = r#"
jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - run: cargo xtask check-static-language
      - run: cargo policy preflight
      - name: Required Rust gates
        run: |
          cargo fmt --check
          cargo xtask check-network-policy
          cargo xtask goldens check
          cargo xtask ripr-pr --base "$BASE_SHA" --head "$HEAD_SHA"
          cargo xtask test-efficiency-report || true
      - name: Badge refresh
        continue-on-error: true
        run: cargo xtask badges
      - name: Advisory step
        run: cargo xtask check-support-tiers
        continue-on-error: true
  docs:
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - run: cargo xtask check-doc-artifacts
  release:
    runs-on: ubuntu-latest
    steps:
      - name: Package
        run: cargo xtask release-server-archive --version "${{ github.ref_name }}"
      - name: Open PR
        uses: peter-evans/create-pull-request@v8
        with:
          body: |
            ```bash
            cargo xtask badges
            ```
"#;
    let enforced = ci_enforced_xtask_invocations(workflow);
    let has_root = |root: &str| enforced.iter().any(|(command, _)| command == root);
    assert!(enforced.contains(&("check-static-language".to_string(), String::new())));
    assert!(enforced.contains(&("check-agent-skills".to_string(), String::new())));
    assert!(
        super::super::workflow_run_xtask_invocation("cargo policy preflight || true").is_none()
    );
    assert!(super::super::workflow_run_xtask_invocation("# cargo policy preflight").is_none());
    assert!(enforced.contains(&("check-network-policy".to_string(), String::new())));
    assert!(enforced.contains(&("goldens".to_string(), "check".to_string())));
    assert!(enforced.contains(&("ripr-pr".to_string(), String::new())));
    assert!(enforced.contains(&("release-server-archive".to_string(), String::new())));
    assert!(!has_root("test-efficiency-report"));
    assert!(!has_root("badges"));
    assert!(!has_root("check-support-tiers"));
    assert!(!has_root("check-doc-artifacts"));
    assert!(!has_root("fmt"));
}

#[test]
fn ci_full_evidence_gate_runner_accepts_successful_gates() -> Result<(), String> {
    let gates = [CiFullEvidenceGate {
        name: "ok",
        run: ci_full_ok_gate,
    }];

    run_ci_full_evidence_gates(&gates)
}

#[test]
fn ci_full_evidence_gate_runner_names_failing_gate() -> Result<(), String> {
    let gates = [
        CiFullEvidenceGate {
            name: "ok",
            run: ci_full_ok_gate,
        },
        CiFullEvidenceGate {
            name: "bad",
            run: ci_full_err_gate,
        },
    ];

    let error = run_ci_full_evidence_gates(&gates)
        .err()
        .ok_or_else(|| "expected failing gate".to_string())?;

    assert!(error.contains("`bad`"));
    assert!(error.contains("boom"));
    Ok(())
}

#[test]
fn ci_full_evidence_gates_pin_release_review_order() {
    let names = ci_full_evidence_gates()
        .iter()
        .map(|gate| gate.name)
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "fixtures",
            "goldens check",
            "test-oracle-report",
            "dogfood",
            "metrics"
        ]
    );
}

#[test]
fn ci_policy_dispatch_and_catalog_expand_the_existing_policy_owner() -> Result<(), String> {
    assert_eq!(
        XtaskCommand::parse(["ci-policy".to_string()]),
        XtaskCommand::CiPolicy
    );
    assert!(matches!(
        XtaskCommand::parse(["ci-policy".to_string(), "--skip".to_string()]),
        XtaskCommand::Unknown(_)
    ));
    let required =
        "jobs:\n  policy:\n    steps:\n      - name: Policy\n        run: cargo xtask ci-policy\n";
    let mut enforced = super::super::ci_enforced_xtask_invocations(required);
    super::super::expand_precommit_ci_invocations(&mut enforced);
    for gate in super::super::PRECOMMIT_GATE_COMMANDS {
        assert!(
            enforced.contains(&(gate.to_string(), String::new())),
            "missing {gate}"
        );
    }
    for advisory in [
        required.replace("ci-policy", "ci-policy || true"),
        required.replace(
            "- name: Policy",
            "- name: Policy\n        continue-on-error: true",
        ),
    ] {
        let mut enforced = super::super::ci_enforced_xtask_invocations(&advisory);
        super::super::expand_precommit_ci_invocations(&mut enforced);
        assert!(enforced.is_empty(), "advisory expansion: {enforced:?}");
    }
    Ok(())
}

#[test]
fn ci_policy_publication_preserves_local_report_and_local_io_failures() -> Result<(), String> {
    with_temp_cwd("ci-policy-local-report", |root| {
        let reports = root.join("target/ripr/reports");
        std::fs::create_dir_all(&reports).map_err(|e| e.to_string())?;
        let local = reports.join("precommit.md");
        std::fs::write(&local, "local report witness").map_err(|e| e.to_string())?;
        super::super::publish_precommit_policy_report(true)?;
        assert_eq!(
            std::fs::read_to_string(&local).map_err(|e| e.to_string())?,
            "local report witness"
        );
        super::super::publish_precommit_policy_report(false)?;
        assert_eq!(
            std::fs::read_to_string(&local).map_err(|e| e.to_string())?,
            super::super::precommit_report_body()
        );
        std::fs::remove_file(&local).map_err(|e| e.to_string())?;
        std::fs::create_dir(&local).map_err(|e| e.to_string())?;
        super::super::publish_precommit_policy_report(true)?;
        assert!(local.is_dir());
        assert!(super::super::publish_precommit_policy_report(false).is_err());
        Ok(())
    })
}

#[test]
fn cockpit_packet_reports_action_queue_and_boundaries() -> Result<(), String> {
    let steps = vec![
        PrReadyStep {
            id: "command_catalog_check",
            command: "cargo xtask check-command-catalog",
            report: "target/ripr/reports/command-catalog.md",
            required: true,
            status: "pass".to_string(),
            summary: "completed".to_string(),
        },
        PrReadyStep {
            id: "pr_triage",
            command: "cargo xtask pr-triage-report",
            report: "target/ripr/reports/pr-triage.md",
            required: false,
            status: "needs_attention".to_string(),
            summary: "stale duplicate PRs detected".to_string(),
        },
    ];

    assert_eq!(pr_ready_status(&steps), "actionable");
    let json = cockpit_json(&steps);
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["mode"], "advisory");
    assert_eq!(value["status"], "actionable");
    assert!(
        value["action_queue"]
            .as_array()
            .ok_or("action_queue must be an array")?
            .iter()
            .any(|entry| entry
                .as_str()
                .is_some_and(|text| text.contains("stale, duplicate")))
    );
    assert!(
        value["judgment_required"]
            .as_array()
            .ok_or("judgment_required must be an array")?
            .iter()
            .any(|entry| entry == "baseline adoption")
    );

    let markdown = cockpit_markdown(&steps);
    assert!(markdown.contains("# ripr Repo Cockpit"));
    assert!(markdown.contains("Status: actionable"));
    assert!(markdown.contains("## Action Queue"));
    assert!(markdown.contains("## Generated Only"));
    assert!(markdown.contains("## Start-Here Language"));
    assert!(markdown.contains("preview-limited evidence"));
    Ok(())
}

#[test]
fn command_catalog_check_accepts_current_catalog() {
    let commands = known_commands();
    let catalog = command_catalog();

    assert_eq!(
        command_catalog_violations(&commands, &catalog),
        Vec::<String>::new()
    );
}

#[test]
fn command_catalog_check_reports_drift_and_missing_classification() {
    let commands = vec!["shape", "missing-command", "arg-command"];
    let catalog = vec![
        CommandCatalogEntry {
            command: "shape",
            mutability: "mutating",
            writes: "",
            judgment_required: false,
            ci_enforced: false,
            notes: "Writes local files.",
        },
        CommandCatalogEntry {
            command: "removed-command",
            mutability: "external_state_mutating",
            writes: "GitHub",
            judgment_required: false,
            ci_enforced: false,
            notes: "Uploads artifacts.",
        },
        CommandCatalogEntry {
            command: "arg-command",
            mutability: "argument_dependent",
            writes: "target/ripr/reports",
            judgment_required: false,
            ci_enforced: false,
            notes: "May write.",
        },
    ];

    let report = command_catalog_violations(&commands, &catalog).join("\n");

    assert!(report.contains("missing from the command mutability catalog"));
    assert!(report.contains("does not match any known xtask command"));
    assert!(report.contains("command `shape` must document writes"));
    assert!(report.contains("must be judgment-required"));
    assert!(report.contains("must explain when it writes"));
}

#[test]
fn command_catalog_check_reports_order_drift() {
    let commands = vec!["shape", "check-pr"];
    let catalog = vec![
        CommandCatalogEntry {
            command: "check-pr",
            mutability: "non_mutating_check",
            writes: "target/ripr/reports",
            judgment_required: false,
            ci_enforced: false,
            notes: "Review-ready gate.",
        },
        CommandCatalogEntry {
            command: "shape",
            mutability: "mutating",
            writes: "source files",
            judgment_required: false,
            ci_enforced: false,
            notes: "Runs deterministic shaping.",
        },
    ];

    let report = command_catalog_violations(&commands, &catalog).join("\n");

    assert!(report.contains("must follow the xtask help catalog order"));
}

#[test]
fn command_catalog_check_reports_order_drift_for_commands_with_shared_roots() {
    let commands = vec!["goldens check", "goldens bless <name> --reason <reason>"];
    let catalog = vec![
        CommandCatalogEntry {
            command: "goldens bless <name> --reason <reason>",
            mutability: "mutating",
            writes: "fixtures/**/expected/**",
            judgment_required: true,
            ci_enforced: false,
            notes: "Updates golden expected outputs.",
        },
        CommandCatalogEntry {
            command: "goldens check",
            mutability: "non_mutating_check",
            writes: "target/ripr/reports/goldens.md",
            judgment_required: false,
            ci_enforced: false,
            notes: "Checks golden drift.",
        },
    ];

    let report = command_catalog_violations(&commands, &catalog).join("\n");

    assert!(report.contains("must follow the xtask help catalog order"));
}

#[test]
fn command_catalog_ci_drift_reports_both_directions() {
    let catalog = vec![
        CommandCatalogEntry {
            command: "check-static-language",
            mutability: "non_mutating_check",
            writes: "target/ripr/reports",
            judgment_required: false,
            ci_enforced: false,
            notes: "Checks static language policy.",
        },
        CommandCatalogEntry {
            command: "check-supply-chain",
            mutability: "non_mutating_check",
            writes: "target/ripr/reports",
            judgment_required: false,
            ci_enforced: true,
            notes: "Checks supply-chain policy.",
        },
        CommandCatalogEntry {
            command: "goldens check",
            mutability: "non_mutating_check",
            writes: "target/ripr/reports/goldens.md",
            judgment_required: false,
            ci_enforced: false,
            notes: "Checks golden drift.",
        },
        CommandCatalogEntry {
            command: "goldens bless <name> --reason <reason>",
            mutability: "mutating",
            writes: "fixtures/**/expected/**",
            judgment_required: true,
            ci_enforced: false,
            notes: "Updates golden expected outputs.",
        },
    ];
    let enforced = std::collections::BTreeSet::from([
        ("check-static-language".to_string(), String::new()),
        ("goldens".to_string(), "check".to_string()),
    ]);

    let violations = command_catalog_ci_drift_violations(&catalog, &enforced);
    let report = violations.join("\n");
    assert_eq!(violations.len(), 3);
    assert!(report.contains(
            "`check-static-language` is invoked by a CI workflow without advisory shielding but the catalog marks ci_enforced=false"
        ));
    assert!(
        report.contains("`goldens check` is invoked by a CI workflow without advisory shielding")
    );
    assert!(
        report.contains(
            "`check-supply-chain` is marked ci_enforced=true but no CI workflow invokes it"
        )
    );
    assert!(!report.contains("goldens bless"));
}

#[test]
fn command_catalog_ci_enforced_flags_match_repo_workflows() -> Result<(), String> {
    let workflows_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".github/workflows");
    let mut enforced = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(&workflows_dir).map_err(|err| err.to_string())? {
        let path = entry.map_err(|err| err.to_string())?.path();
        let name = path.file_name().map(|name| name.to_string_lossy());
        if !name.is_some_and(|name| name.ends_with(".yml") || name.ends_with(".yaml")) {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
        enforced.extend(ci_enforced_xtask_invocations(&text));
    }
    super::super::expand_precommit_ci_invocations(&mut enforced);
    let catalog = command_catalog();
    assert_eq!(
        command_catalog_ci_drift_violations(&catalog, &enforced),
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn command_catalog_classifies_core_workflow_commands() -> Result<(), String> {
    let catalog = command_catalog();
    let find = |command: &str| {
        catalog
            .iter()
            .find(|entry| entry.command == command)
            .ok_or_else(|| format!("missing command catalog entry `{command}`"))
    };

    assert_eq!(find("shape")?.mutability, "mutating");
    assert_eq!(find("check-pr")?.mutability, "non_mutating_check");
    assert_eq!(find("pr-ready")?.mutability, "report_only");
    assert_eq!(find("cockpit")?.mutability, "external_state_read");
    assert_eq!(
        find("check-command-catalog")?.mutability,
        "non_mutating_check"
    );
    assert_eq!(find("badges")?.mutability, "mutating");
    assert_eq!(find("badges --check")?.mutability, "non_mutating_check");
    assert!(find("goldens bless <name> --reason <reason>")?.judgment_required);
    assert_eq!(
        find("release-upload-assets --version <version>")?.mutability,
        "external_state_mutating"
    );
    assert!(find("release-upload-assets --version <version>")?.judgment_required);
    Ok(())
}

#[test]
fn command_catalog_covers_help_catalog_roots() {
    let catalog = command_catalog();
    let catalog_roots = catalog
        .iter()
        .map(|entry| {
            entry
                .command
                .split_once(' ')
                .map_or(entry.command, |(root, _)| root)
        })
        .collect::<BTreeSet<_>>();

    for command in known_commands() {
        let root = command.split_once(' ').map_or(command, |(root, _)| root);
        assert!(
            catalog_roots.contains(root),
            "command catalog missing help root `{root}`"
        );
    }
}

#[test]
fn command_catalog_has_no_duplicate_entries() {
    let catalog = command_catalog();
    let unique = catalog
        .iter()
        .map(|entry| entry.command)
        .collect::<BTreeSet<_>>();
    assert_eq!(catalog.len(), unique.len());
}

#[test]
fn command_catalog_pins_ci_enforced_classification() -> Result<(), String> {
    let catalog = command_catalog();
    let ci_enforced = |command: &str| {
        catalog
            .iter()
            .find(|entry| entry.command == command)
            .map(|entry| entry.ci_enforced)
            .ok_or_else(|| format!("missing catalog entry for {command}"))
    };
    assert!(ci_enforced("check-static-language")?);
    assert!(ci_enforced("goldens check")?);
    assert!(ci_enforced("check-doc-index")?);
    assert!(!ci_enforced("release-upload-assets --version <version>")?);
    assert!(ci_enforced("release-readiness --version <version>")?);
    // Issue #2258: the routed-rust lanes invoke `cargo xtask precommit` as the
    // shared required gate table, so precommit and every gate it runs are
    // CI-enforced.
    assert!(ci_enforced("precommit")?);
    assert!(ci_enforced("check-architecture")?);
    assert!(ci_enforced("check-doc-artifacts")?);
    assert!(ci_enforced("markdown-links")?);
    assert!(!ci_enforced("goldens bless <name> --reason <reason>")?);
    assert!(!ci_enforced("check-supply-chain")?);
    Ok(())
}

#[test]
fn commands_report_json_and_markdown_are_structured() -> Result<(), String> {
    let catalog = command_catalog();
    let json = commands_report_json(&catalog);
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["mode"], "advisory");
    assert!(
        value["commands"]
            .as_array()
            .ok_or("commands must be an array")?
            .iter()
            .any(|entry| entry["command"] == "check-pr"
                && entry["mutability"] == "non_mutating_check")
    );

    let markdown = commands_report_markdown(&catalog);
    assert!(markdown.contains("# ripr command mutability catalog"));
    assert!(markdown.contains("Status: pass"));
    assert!(markdown.contains("| `check-pr` | `non_mutating_check` |"));
    Ok(())
}

#[test]
fn committed_spec_review_receipts_validate() -> Result<(), String> {
    // Every committed receipt must parse under SpecReviewReceiptV1 and carry
    // no keys outside the enumerated contract (#3466). An empty or absent
    // reviews directory is valid: this slice commits no receipts, and the
    // first real batch belongs to the backlog adjudication follow-up.
    let reviews_dir = slice_test_repo_root().join(".allow/spec-system/reviews");
    if !reviews_dir.exists() {
        return Ok(());
    }
    let mut validated = 0;
    let entries =
        std::fs::read_dir(&reviews_dir).map_err(|err| format!("read reviews directory: {err}"))?;
    for entry in entries {
        let path = entry.map_err(|err| format!("receipt entry: {err}"))?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("read receipt {}: {err}", path.display()))?;
        crate::reports::validate_receipt(&text)
            .map_err(|reason| format!("{}: rejected ({reason})", path.display()))?;
        for line in text.lines() {
            let Some(key) = line.split('=').next() else {
                continue;
            };
            let key = key.trim();
            const RECEIPT_KEYS: [&str; 18] = [
                "schema_version",
                "producer",
                "spec_id",
                "spec_path",
                "content_digest",
                "status_observed",
                "observed_at",
                "reviewed_by",
                "disposition",
                "waived_until",
                "disposition_detail",
                "reasons_inspected",
                "evidence_refs",
                "limitations",
                "receipt_id",
                "predecessor_receipt_id",
                "claim_boundary",
                "non_claims",
            ];
            if !key.is_empty() && !RECEIPT_KEYS.contains(&key) {
                return Err(format!(
                    "{}: receipt carries key `{key}` outside the contract enumeration",
                    path.display()
                ));
            }
        }
        validated += 1;
    }
    let _ = validated;
    Ok(())
}

#[test]
fn compute_badge_endpoint_violations_errors_when_source_missing() -> Result<(), String> {
    let reports = badge_endpoint_tempdir("compute-noreport-reports")?;
    let repo_root = badge_endpoint_tempdir("compute-noreport-root")?;
    // No source files at all.

    let result = super::super::compute_badge_endpoint_violations(&reports, &repo_root);
    let err = result.err().ok_or_else(|| {
        "missing reports directory contents should produce a hard error".to_string()
    })?;
    assert!(
        err.contains("failed to read"),
        "error should describe the read failure: {err}"
    );

    ignore_remove_dir_all(&reports);
    ignore_remove_dir_all(&repo_root);
    Ok(())
}

#[test]
fn compute_badge_endpoint_violations_flags_missing_committed_file() -> Result<(), String> {
    let reports = badge_endpoint_tempdir("compute-missing-reports")?;
    let repo_root = badge_endpoint_tempdir("compute-missing-root")?;
    write_fixture_shields(
        &reports,
        "repo-ripr-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"0\",\"color\":\"brightgreen\"}",
    )?;
    write_fixture_shields(
        &reports,
        "repo-ripr-plus-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"0\",\"color\":\"brightgreen\"}",
    )?;
    // No committed badges/*.json files exist on the temp repo root.

    let violations = super::super::compute_badge_endpoint_violations(&reports, &repo_root)?;
    assert_eq!(
        violations.len(),
        2,
        "both committed files missing must flag both: {violations:?}"
    );
    for violation in &violations {
        assert!(
            violation.contains("missing badge endpoint file"),
            "violation should be the missing-file shape: {violation}"
        );
    }

    ignore_remove_dir_all(&reports);
    ignore_remove_dir_all(&repo_root);
    Ok(())
}

#[test]
fn compute_badge_endpoint_violations_flags_stale_committed_file() -> Result<(), String> {
    let reports = badge_endpoint_tempdir("compute-stale-reports")?;
    let repo_root = badge_endpoint_tempdir("compute-stale-root")?;
    let want = b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"5\",\"color\":\"orange\"}";
    let stale = b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"99\",\"color\":\"orange\"}";
    write_fixture_shields(&reports, "repo-ripr-badge-shields.json", want)?;
    write_fixture_shields(
        &reports,
        "repo-ripr-plus-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"5\",\"color\":\"orange\"}",
    )?;
    std::fs::create_dir_all(repo_root.join("badges"))
        .map_err(|err| format!("mkdir badges: {err}"))?;
    // ripr.json is stale; ripr-plus.json is in sync.
    std::fs::write(repo_root.join("badges/ripr.json"), stale)
        .map_err(|err| format!("write stale ripr.json: {err}"))?;
    std::fs::write(
        repo_root.join("badges/ripr-plus.json"),
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"5\",\"color\":\"orange\"}",
    )
    .map_err(|err| format!("write fresh ripr-plus.json: {err}"))?;

    let violations = super::super::compute_badge_endpoint_violations(&reports, &repo_root)?;
    assert_eq!(violations.len(), 1, "exactly one stale: {violations:?}");
    assert!(
        violations[0].contains("badges/ripr.json is stale"),
        "violation should name the stale file: {}",
        violations[0]
    );

    ignore_remove_dir_all(&reports);
    ignore_remove_dir_all(&repo_root);
    Ok(())
}

#[test]
fn compute_badge_endpoint_violations_returns_empty_when_in_sync() -> Result<(), String> {
    let reports = badge_endpoint_tempdir("compute-sync-reports")?;
    let repo_root = badge_endpoint_tempdir("compute-sync-root")?;
    let ripr_body =
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"3\",\"color\":\"yellow\"}";
    let plus_body =
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"3\",\"color\":\"yellow\"}";
    write_fixture_shields(&reports, "repo-ripr-badge-shields.json", ripr_body)?;
    write_fixture_shields(&reports, "repo-ripr-plus-badge-shields.json", plus_body)?;
    std::fs::create_dir_all(repo_root.join("badges"))
        .map_err(|err| format!("mkdir badges: {err}"))?;
    std::fs::write(repo_root.join("badges/ripr.json"), ripr_body)
        .map_err(|err| format!("write committed ripr.json: {err}"))?;
    std::fs::write(repo_root.join("badges/ripr-plus.json"), plus_body)
        .map_err(|err| format!("write committed ripr-plus.json: {err}"))?;

    let violations = super::super::compute_badge_endpoint_violations(&reports, &repo_root)?;
    assert!(
        violations.is_empty(),
        "in-sync committed files must produce no violations: {violations:?}"
    );

    ignore_remove_dir_all(&reports);
    ignore_remove_dir_all(&repo_root);
    Ok(())
}

#[test]
fn configured_bridge_inventory_accepts_custom_paths_and_rejects_bad_args() -> Result<(), String> {
    let args = super::super::parse_configured_bridge_inventory_args(&[
        "--graph-corpus".to_string(),
        "graph.json".to_string(),
        "--out".to_string(),
        "inventory.json".to_string(),
        "--out-md".to_string(),
        "inventory.md".to_string(),
    ])?;
    assert_eq!(args.graph_corpus, PathBuf::from("graph.json"));
    assert_eq!(args.out, PathBuf::from("inventory.json"));
    assert_eq!(args.out_md, PathBuf::from("inventory.md"));

    let missing_value =
        super::super::parse_configured_bridge_inventory_args(&["--out".to_string()]).unwrap_err();
    assert!(missing_value.contains("missing value for `--out`"));
    assert!(missing_value.contains("usage: cargo xtask configured-bridge-inventory"));

    let unknown = super::super::parse_configured_bridge_inventory_args(&["--unknown".to_string()])
        .unwrap_err();
    assert!(unknown.contains("unknown configured-bridge-inventory argument `--unknown`"));

    let help =
        super::super::parse_configured_bridge_inventory_args(&["--help".to_string()]).unwrap_err();
    assert!(help.contains("usage: cargo xtask configured-bridge-inventory"));
    Ok(())
}

#[test]
fn configured_bridge_inventory_command_writes_markdown_and_json() -> Result<(), String> {
    with_temp_cwd("configured-bridge-inventory-report", |_root| {
        let out = PathBuf::from("target/ripr/reports/configured-bridge-inventory.json");
        let out_md = PathBuf::from("target/ripr/reports/configured-bridge-inventory.md");
        super::super::configured_bridge_inventory_impl(&[])?;

        let json = fs::read_to_string(&out)
            .map_err(|err| format!("failed to read configured bridge inventory JSON: {err}"))?;
        let value: Value =
            serde_json::from_str(&json).map_err(|err| format!("invalid JSON: {err}"))?;
        assert_eq!(value["report"], "configured-bridge-inventory");
        assert_eq!(value["status"], "pass");
        assert_eq!(value["authority"], "preview_advisory_only");
        assert_eq!(
            value["summary"]["repair_packet_ready_cases"],
            serde_json::Value::from(0)
        );

        let markdown = fs::read_to_string(&out_md)
            .map_err(|err| format!("failed to read configured bridge inventory Markdown: {err}"))?;
        assert!(markdown.contains("# Configured Bridge Inventory"));
        assert!(markdown.contains("Future Or Missing Surfaces"));
        assert!(markdown.contains("bun_node_fs_scalar_write"));
        assert!(markdown.contains("repair_packet_ready: false"));
        Ok(())
    })
}

#[test]
fn configured_bridge_inventory_reports_missing_future_surfaces() -> Result<(), String> {
    fn assert_inventory_rows_are_non_repair(rows: &[Value]) {
        for row in rows {
            assert_eq!(row["inventory_action"], "inventory_only");
            assert_eq!(row["repair_packet_ready"], false);
            assert_eq!(row["public_projection_eligible"], false);
            assert_eq!(row["repair_target"], "not_applicable");
            assert_eq!(row["verify_command"], "not_applicable");
            assert_eq!(row["receipt_command"], "not_applicable");
            assert!(
                row["allowed_edit_surface"]
                    .as_array()
                    .is_some_and(Vec::is_empty),
                "inventory row should not expose an edit surface: {row:?}"
            );
        }
    }

    with_repo_cwd(|| {
        let report = super::super::configured_bridge_inventory_report_value(
            &super::super::ConfiguredBridgeInventoryArgs {
                graph_corpus: super::super::cross_language_oracle_graph_corpus_path(),
                out: PathBuf::from("target/ripr/reports/configured-bridge-inventory.json"),
                out_md: PathBuf::from("target/ripr/reports/configured-bridge-inventory.md"),
            },
        );
        assert_eq!(report["schema_version"], "0.1");
        assert_eq!(report["report"], "configured-bridge-inventory");
        assert_eq!(report["status"], "pass");
        assert_eq!(report["authority"], "preview_advisory_only");
        assert_eq!(report["authority_boundary"], "preview_advisory_only");
        assert_eq!(report["repair_packet_ready"], false);
        assert_eq!(
            report["summary"]["configured_bridge_profiles"],
            serde_json::Value::from(3)
        );
        assert_eq!(
            report["summary"]["future_or_missing_surfaces"],
            serde_json::Value::from(2)
        );
        assert_eq!(
            report["summary"]["bridge_unknown_profiles"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            report["summary"]["repair_packet_ready_cases"],
            serde_json::Value::from(0)
        );
        assert_eq!(
            report["summary"]["s3_surfaces_backed_by_corpus"],
            serde_json::Value::Bool(false)
        );

        let configured = report["configured_bridges"]
            .as_array()
            .ok_or_else(|| "configured_bridges must be an array".to_string())?;
        let configured_profiles = configured
            .iter()
            .filter_map(|row| row["profile"].as_str())
            .collect::<BTreeSet<_>>();
        for required in [
            "bun_blob_array_buffer",
            "bun_array_buffer_copy_to_unshared",
            "bun_markdown_resizable_array_buffer",
        ] {
            assert!(
                configured_profiles.contains(required),
                "configured bridge inventory should list {required}"
            );
        }
        assert_inventory_rows_are_non_repair(configured);

        let future = report["future_or_missing_surfaces"]
            .as_array()
            .ok_or_else(|| "future_or_missing_surfaces must be an array".to_string())?;
        let future_profiles = future
            .iter()
            .filter_map(|row| row["profile"].as_str())
            .collect::<BTreeSet<_>>();
        for required in ["bun_node_fs_scalar_write", "bun_write_helper_gated"] {
            assert!(
                future_profiles.contains(required),
                "configured bridge inventory should list future surface {required}"
            );
        }
        assert!(
            !future_profiles.iter().any(|profile| profile.contains("s3")),
            "S3 should not be listed without corpus metadata"
        );
        let bun_write = future
            .iter()
            .find(|row| row["profile"] == "bun_write_helper_gated")
            .ok_or_else(|| "missing Bun.write helper-gated future surface".to_string())?;
        assert_eq!(bun_write["surface_state"], "manifest_only");
        assert!(
            bun_write["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs
                    .iter()
                    .any(|item| item == "helper:bun_write_fixture_helper"))
        );
        assert_eq!(bun_write["inventory_action"], "inventory_only");
        assert_inventory_rows_are_non_repair(future);

        let unknown = report["bridge_unknown"]
            .as_array()
            .ok_or_else(|| "bridge_unknown must be an array".to_string())?;
        assert!(unknown.iter().any(|row| {
            row["profile"] == "bun_blob_array_buffer"
                && row["surface_state"] == "bridge_unknown"
                && row["missing_graph_legs"]
                    .as_array()
                    .is_some_and(|legs| legs.iter().any(|item| item == "binding_or_ffi_edge"))
        }));
        assert_inventory_rows_are_non_repair(unknown);

        let limitations = report["named_static_limitations"]
            .as_array()
            .ok_or_else(|| "named_static_limitations must be an array".to_string())?;
        assert_inventory_rows_are_non_repair(limitations);

        let markdown = super::super::configured_bridge_inventory_markdown(&report);
        assert!(markdown.contains("# Configured Bridge Inventory"));
        assert!(markdown.contains("Blob ArrayBuffer"));
        assert!(markdown.contains("copy_to_unshared"));
        assert!(markdown.contains("MarkdownObject"));
        assert!(markdown.contains("node:fs scalar write sink"));
        assert!(markdown.contains("Bun.write sink"));
        assert!(markdown.contains("S3 surfaces backed by corpus"));
        assert!(markdown.contains("no inferred reachability"));
        assert!(markdown.contains("no public repair packet"));
        assert!(markdown.contains("repair_packet_ready: false"));
        assert!(!markdown.contains("bun_s3_stable_byte_sink"));
        Ok(())
    })
}

#[test]
fn copy_badge_endpoints_from_reports_creates_badges_dir_when_missing() -> Result<(), String> {
    let reports = badge_endpoint_tempdir("copy-mkdir-reports")?;
    let repo_root = badge_endpoint_tempdir("copy-mkdir-root")?;
    write_fixture_shields(
        &reports,
        "repo-ripr-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"0\",\"color\":\"brightgreen\"}",
    )?;
    write_fixture_shields(
        &reports,
        "repo-ripr-plus-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"0\",\"color\":\"brightgreen\"}",
    )?;
    // badges/ subdir does not exist yet; copy_badge_endpoints_from_reports must create it.
    assert!(!repo_root.join("badges").exists());

    super::super::copy_badge_endpoints_from_reports(&reports, &repo_root)?;

    assert!(repo_root.join("badges").is_dir());
    assert!(repo_root.join("badges/ripr.json").exists());
    assert!(repo_root.join("badges/ripr-plus.json").exists());

    ignore_remove_dir_all(&reports);
    ignore_remove_dir_all(&repo_root);
    Ok(())
}

#[test]
fn copy_badge_endpoints_from_reports_errors_when_source_missing() -> Result<(), String> {
    let reports = badge_endpoint_tempdir("copy-err-reports")?;
    let repo_root = badge_endpoint_tempdir("copy-err-root")?;
    // Write only the ripr source; ripr-plus is missing.
    write_fixture_shields(
        &reports,
        "repo-ripr-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"0\",\"color\":\"brightgreen\"}",
    )?;

    let result = super::super::copy_badge_endpoints_from_reports(&reports, &repo_root);
    let err = result.err().ok_or_else(|| {
        "missing source must produce an error from copy_badge_endpoints_from_reports".to_string()
    })?;
    assert!(
        err.contains("repo-ripr-plus-badge-shields.json"),
        "error should name the missing source file: {err}"
    );
    assert!(
        err.contains("cargo xtask repo-badge-artifacts"),
        "error should suggest regenerating the source: {err}"
    );

    ignore_remove_dir_all(&reports);
    ignore_remove_dir_all(&repo_root);
    Ok(())
}

#[test]
fn copy_badge_endpoints_from_reports_writes_both_files() -> Result<(), String> {
    let reports = badge_endpoint_tempdir("copy-reports")?;
    let repo_root = badge_endpoint_tempdir("copy-root")?;
    write_fixture_shields(
        &reports,
        "repo-ripr-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"7\",\"color\":\"yellow\"}",
    )?;
    write_fixture_shields(
        &reports,
        "repo-ripr-plus-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"7\",\"color\":\"yellow\"}",
    )?;

    super::super::copy_badge_endpoints_from_reports(&reports, &repo_root)?;

    let ripr = std::fs::read(repo_root.join("badges/ripr.json"))
        .map_err(|err| format!("read written ripr.json: {err}"))?;
    let ripr_plus = std::fs::read(repo_root.join("badges/ripr-plus.json"))
        .map_err(|err| format!("read written ripr-plus.json: {err}"))?;
    assert_eq!(
        ripr,
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"7\",\"color\":\"yellow\"}"
    );
    assert_eq!(
        ripr_plus,
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"7\",\"color\":\"yellow\"}"
    );

    ignore_remove_dir_all(&reports);
    ignore_remove_dir_all(&repo_root);
    Ok(())
}

#[test]
fn count_policy_allowlist_accepts_unique_path_pattern_row() -> Result<(), String> {
    let policy = "\
# header
fixtures/example.rs|ExampleMarker|2|owners|owned bound
fixtures/example.rs|OtherMarker|1|owners|different pattern
fixtures/other.rs|ExampleMarker|1|owners|different path
";
    let allowed =
        super::super::parse_count_policy_allowlist("policy/process_allowlist.txt", policy)
            .map_err(|err| format!("unique count-policy rows must parse: {err}"))?;
    if allowed.get(&(
        "fixtures/example.rs".to_string(),
        "ExampleMarker".to_string(),
    )) != Some(&2)
    {
        return Err(format!(
            "unique policy row bound was not retained: {allowed:?}"
        ));
    }
    if allowed.len() != 3 {
        return Err(format!(
            "each unique path|pattern key must be kept, got {}",
            allowed.len()
        ));
    }

    let runtime = "\
.github/workflows/example.yml|uses:example/action|2|owned bound
.github/workflows/other.yml|uses:example/action|1|different path
";
    let allowed = super::super::parse_count_allowlist(
        "policy/workflow_action_runtime_allowlist.txt",
        runtime,
    )
    .map_err(|err| format!("unique count-allowlist rows must parse: {err}"))?;
    if allowed.get(&(
        ".github/workflows/example.yml".to_string(),
        "uses:example/action".to_string(),
    )) != Some(&2)
    {
        return Err(format!(
            "unique runtime row bound was not retained: {allowed:?}"
        ));
    }
    if allowed.len() != 2 {
        return Err(format!(
            "each unique path|pattern key must be kept, got {}",
            allowed.len()
        ));
    }
    Ok(())
}

#[test]
fn count_policy_allowlist_rejects_duplicate_path_pattern_row() -> Result<(), String> {
    let policy = "\
# header
fixtures/example.rs|ExampleMarker|1|owners|first bound
fixtures/example.rs|ExampleMarker|9|owners|silently wider bound
";
    let err = super::super::parse_count_policy_allowlist("policy/process_allowlist.txt", policy)
        .err()
        .ok_or_else(|| "duplicate count-policy row must fail".to_string())?;
    if !err.contains("policy/process_allowlist.txt:3") {
        return Err(format!("expected the later line number, got {err}"));
    }
    if !err.contains("path|pattern `fixtures/example.rs|ExampleMarker` is duplicated (first declared near line 2)")
    {
        return Err(format!("expected a first-declaration pointer, got {err}"));
    }

    let runtime = "\
.github/workflows/example.yml|uses:example/action|1|first bound
.github/workflows/example.yml|uses:example/action|4|silently wider bound
";
    let err = super::super::parse_count_allowlist(
        "policy/workflow_action_runtime_allowlist.txt",
        runtime,
    )
    .err()
    .ok_or_else(|| "duplicate count-allowlist row must fail".to_string())?;
    if !err.contains("policy/workflow_action_runtime_allowlist.txt:2") {
        return Err(format!("expected the later line number, got {err}"));
    }
    if !err.contains(
        "path|pattern `.github/workflows/example.yml|uses:example/action` is duplicated (first declared near line 1)",
    ) {
        return Err(format!("expected a first-declaration pointer, got {err}"));
    }
    Ok(())
}

#[test]
fn count_policy_gates_have_no_stale_bounds() -> Result<(), String> {
    // Regression for #2413: the count-based gates now check bidirectionally.
    // A stale max_count (actual < allowed) or an orphaned entry (pattern not
    // found) must produce a violation. This test runs the real gates against
    // the real allowlists to catch drift introduced by refactoring that drops
    // process/network occurrences without updating the allowlist.
    //
    // If this test fails, tighten the stale max_count or remove the orphaned
    // entry in the cited allowlist file.
    use crate::{check_local_context, check_network_policy, check_process_policy};
    with_repo_cwd(|| {
        check_process_policy()?;
        check_network_policy()?;
        check_local_context()?;
        Ok(())
    })
}

#[test]
fn create_zip_archive_writes_flat_package_contents() -> Result<(), String> {
    with_temp_cwd("release-server-archive-zip", |root| {
        let package = root.join("package");
        fs::create_dir_all(&package).map_err(|err| format!("create package dir: {err}"))?;
        write(&package.join("ripr.exe"), "binary");
        write(&package.join("LICENSE-MIT"), "mit");
        write(&package.join("LICENSE-APACHE"), "apache");
        write(&package.join("README-server.txt"), "readme");

        let dist = root.join("dist");
        fs::create_dir_all(&dist).map_err(|err| format!("create dist dir: {err}"))?;
        let asset = dist.join("ripr-server-v1.2.3.zip");
        super::super::create_zip_archive(&package, &asset)?;

        let zip_file = fs::File::open(&asset).map_err(|err| format!("open written zip: {err}"))?;
        let mut archive =
            zip::ZipArchive::new(zip_file).map_err(|err| format!("read written zip: {err}"))?;

        let mut names: Vec<String> = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|err| format!("zip entry {index}: {err}"))?;
            names.push(
                entry
                    .name()
                    .map_err(|err| format!("zip entry {index} name: {err}"))?
                    .into_owned(),
            );
        }
        names.sort();
        assert_eq!(
            names,
            vec![
                "LICENSE-APACHE".to_string(),
                "LICENSE-MIT".to_string(),
                "README-server.txt".to_string(),
                "ripr.exe".to_string(),
            ]
        );

        let mut binary_contents = String::new();
        archive
            .by_name("ripr.exe")
            .map_err(|err| format!("locate ripr.exe entry: {err}"))?
            .read_to_string(&mut binary_contents)
            .map_err(|err| format!("read ripr.exe entry: {err}"))?;
        assert_eq!(binary_contents, "binary");
        Ok(())
    })
}

#[test]
fn critic_accepts_analysis_with_evidence_reports_and_receipts() {
    let changes = vec![
        ChangedPath {
            path: "crates/ripr/src/analysis/classifier.rs".to_string(),
            statuses: BTreeSet::from(["M".to_string()]),
        },
        ChangedPath {
            path: "fixtures/boundary_gap/SPEC.md".to_string(),
            statuses: BTreeSet::from(["M".to_string()]),
        },
    ];
    let reports = ["pr-summary.md", "pr-shape.md", "fixtures.md", "goldens.md"]
        .into_iter()
        .map(|file| ReportIndexEntry {
            file: file.to_string(),
            path: format!("target/ripr/reports/{file}"),
            status: "pass".to_string(),
        })
        .collect::<Vec<_>>();
    let receipts = vec![ReportIndexEntry {
        file: "check-pr.json".to_string(),
        path: "target/ripr/receipts/check-pr.json".to_string(),
        status: "present".to_string(),
    }];

    let findings = critic_findings(&changes, &reports, &receipts);

    assert!(
        !findings
            .iter()
            .any(|finding| finding.id.starts_with("analysis_"))
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.id == "missing_receipts")
    );
}

#[test]
fn critic_flags_analysis_without_fixture_or_golden_reports() {
    let changes = vec![ChangedPath {
        path: "crates/ripr/src/analysis/classifier.rs".to_string(),
        statuses: BTreeSet::from(["M".to_string()]),
    }];
    let reports = vec![ReportIndexEntry {
        file: "pr-summary.md".to_string(),
        path: "target/ripr/reports/pr-summary.md".to_string(),
        status: "pass".to_string(),
    }];
    let findings = critic_findings(&changes, &reports, &[]);

    assert!(
        findings
            .iter()
            .any(|finding| finding.id == "analysis_without_behavior_evidence")
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.id == "analysis_missing_fixture_report")
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.id == "missing_receipts")
    );
}

#[test]
fn critic_flags_golden_changes_without_blessing_reason() {
    let changes = vec![ChangedPath {
        path: "fixtures/boundary_gap/expected/check.json".to_string(),
        statuses: BTreeSet::from(["M".to_string()]),
    }];

    let missing = golden_changes_without_blessing(&changes);

    assert_eq!(missing.len(), 1);
    assert!(missing[0].contains("fixtures/boundary_gap/expected/CHANGELOG.md"));
}

#[test]
fn cross_language_oracle_graph_corpus_cases_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let cases = super::super::cross_language_oracle_graph_cases();
        for required in super::super::CROSS_LANGUAGE_ORACLE_GRAPH_REQUIRED_CASES {
            assert!(
                cases
                    .iter()
                    .any(|case| { case.name == required.0 && case.expected_state == required.1 }),
                "{} cross-language oracle graph case should be checked as {}",
                required.0,
                required.1
            );
        }

        let states = cases
            .iter()
            .map(|case| case.expected_state.as_str())
            .collect::<BTreeSet<_>>();
        for required in [
            "rust_ungripped_ts_discriminated",
            "rust_ungripped_ts_missing_discriminator",
            "rust_ungripped_ts_missing_external_oracle",
            "ts_mention_not_observer",
            "bridge_unknown",
            "cross_language_target_unresolved",
            "public_reachable_panic_boundary_unrevealed",
            "named_static_limitation",
        ] {
            assert!(
                states.contains(required),
                "cross-language oracle graph should include {required}"
            );
        }

        for case in cases {
            let errors = super::super::cross_language_oracle_graph_case_errors(&case);
            assert!(
                errors.is_empty(),
                "{} cross-language oracle graph case should validate: {:?}",
                case.name,
                errors
            );
        }

        Ok(())
    })
}

#[test]
fn cross_language_oracle_graph_corpus_path_is_repo_rooted_for_parallel_cwd_safety()
-> Result<(), String> {
    let path = super::super::cross_language_oracle_graph_corpus_path();
    assert!(
        path.is_absolute(),
        "corpus read path should not depend on process cwd: {}",
        path.display()
    );
    assert!(
        path.ends_with(super::super::CROSS_LANGUAGE_ORACLE_GRAPH_CORPUS),
        "corpus read path should keep the canonical fixture suffix: {}",
        path.display()
    );
    assert!(
        path.exists(),
        "corpus read path should exist: {}",
        path.display()
    );
    Ok(())
}

#[test]
fn cross_language_oracle_graph_rejects_actionability_and_location_holes() {
    let mut case = valid_cross_language_oracle_graph_case();
    case.name = " ".to_string();
    case.source.clear();
    case.language = "javascript".to_string();
    case.language_status = "stable".to_string();
    case.authority_boundary = "gate_ready".to_string();
    case.repair_packet_ready = true;
    case.public_projection_eligible = true;
    case.suggested_test_file = "src/jsc/Blob.rs".to_string();
    case.allowed_edit_surface
        .push("test/js/web/fetch/blob.test.ts".to_string());
    case.verify_command = Some("bun test test/js/web/fetch/blob.test.ts".to_string());
    case.receipt_command = Some("cargo xtask receipts check".to_string());
    case.rust_file = "src/lib.rs".to_string();
    case.rust_line = None;
    case.rust_owner = "Blob::from_js".to_string();
    case.rust_boundary = "array_buffer.shared".to_string();
    case.external_callsite_file = "test/js/web/fetch/blob.test.js".to_string();
    case.external_callsite_line = None;
    case.external_oracle_file = "test/js/web/fetch/blob.test.js".to_string();
    case.external_oracle_line = None;
    case.external_entrypoints.clear();
    case.raw_evidence_refs.clear();
    case.non_claims.clear();
    case.reason.clear();

    let errors = super::super::cross_language_oracle_graph_case_errors(&case);
    for expected in [
        "case id must be present",
        "source must be present",
        "reason must be present",
        "rust line must be present",
        "external_callsite line must be present",
        "external_oracle line must be present",
        "language must be typescript",
        "language_status must be preview",
        "authority_boundary must be preview_advisory_only",
        "repair_packet_ready must remain false",
        "public_projection_eligible must remain false",
        "suggested_test_file must be not_applicable, the configured Bun Blob TypeScript test file for missing-discriminator rows, or the typed node:fs manifest placement",
        "allowed_edit_surface must remain empty",
        "verify_command must be omitted",
        "receipt_command must be omitted",
        "rust_file must identify the Bun Blob Rust seam",
        "rust_owner must pin Blob::from_js_without_defer_gc",
        "rust_boundary must include array_buffer.shared and array_buffer.resizable",
        "external_callsite_file must be a Bun test/js TypeScript test path",
        "external_oracle_file must be a Bun test/js TypeScript test path",
        "external_entrypoints must name at least one external callsite",
        "non_claims must keep cross-language preview denials visible",
        "raw_evidence_refs must include structured graph refs",
    ] {
        assert_contains_error(&errors, expected);
    }
}

#[test]
fn cross_language_oracle_graph_rejects_duplicate_and_required_state_drift() -> Result<(), String> {
    let root = temp_dir("cross-language-oracle-graph-required-cases");
    let mut duplicate = valid_cross_language_oracle_graph_case();
    duplicate.name = "duplicated".to_string();
    let duplicate_case = cross_language_oracle_graph_case_json(&duplicate);
    let duplicate_path = root.join("duplicate.json");
    write(
        &duplicate_path,
        &cross_language_oracle_graph_corpus_json(&[duplicate_case.clone(), duplicate_case]),
    );

    let mut violations = Vec::new();
    super::super::validate_cross_language_oracle_graph_fixture_corpus_at(
        &duplicate_path,
        &mut violations,
    )?;
    assert_contains_error(&violations, "case duplicated is duplicated");
    assert_contains_error(
        &violations,
        "Cross-language oracle graph corpus is missing case bun_blob_complete_ts_discriminated_advisory",
    );
    assert_contains_error(
        &violations,
        "Cross-language oracle graph corpus must include state bridge_unknown",
    );

    let wrong_required_path = root.join("wrong-required.json");
    let mut wrong_required = valid_cross_language_oracle_graph_case();
    wrong_required.name = "bun_blob_complete_ts_discriminated_advisory".to_string();
    wrong_required.expected_state = "bridge_unknown".to_string();
    wrong_required.gap_state = "static_limitation".to_string();
    wrong_required.limitation_category = "cross_language_oracle_visibility_unresolved".to_string();
    wrong_required.repair_route = "analysis/cross-language-oracle-visibility".to_string();
    wrong_required.binding_edge_kind = "unresolved".to_string();
    wrong_required.binding_edge_confidence = "unknown".to_string();
    wrong_required
        .missing_graph_legs
        .push("binding_or_ffi_edge".to_string());
    wrong_required
        .raw_evidence_refs
        .retain(|raw_ref| raw_ref.leg != "binding_edge");
    write(
        &wrong_required_path,
        &cross_language_oracle_graph_corpus_json(&[cross_language_oracle_graph_case_json(
            &wrong_required,
        )]),
    );

    let mut violations = Vec::new();
    super::super::validate_cross_language_oracle_graph_fixture_corpus_at(
        &wrong_required_path,
        &mut violations,
    )?;
    assert_contains_error(
        &violations,
        "case bun_blob_complete_ts_discriminated_advisory must have expected_state rust_ungripped_ts_discriminated, got bridge_unknown",
    );

    Ok(())
}

#[test]
fn cross_language_oracle_graph_rejects_invalid_state_shapes() {
    let mut advisory = valid_cross_language_oracle_graph_case();
    advisory.shared_array_buffer = false;
    advisory.max_byte_length_mention_only = true;
    advisory.binding_edge_confidence = "unknown".to_string();
    advisory
        .missing_discriminators
        .push("shared_array_buffer".to_string());
    advisory
        .missing_graph_legs
        .push("external_discriminator:shared_array_buffer".to_string());
    advisory.unlock_condition = "needs more evidence".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&advisory);
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_discriminated requires either the Blob shared/resizable stable-byte facts or the Bun markdown resizable strong-oracle profile facts",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_discriminated must not be mention-only evidence",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_discriminated requires configured bridge confidence",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_discriminated must not name missing discriminators",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_discriminated must not name missing graph legs",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_discriminated must use unlock_condition not_applicable",
    );

    let mut missing = valid_cross_language_oracle_graph_case();
    missing.name = "bun_blob_missing_resizable_oracle_limitation".to_string();
    missing.expected_state = "rust_ungripped_ts_missing_discriminator".to_string();
    missing.gap_state = "static_limitation".to_string();
    missing.limitation_category = "cross_language_oracle_visibility_unresolved".to_string();
    missing.repair_route = "analysis/cross-language-oracle-visibility".to_string();
    missing.missing_discriminators.clear();
    missing.missing_graph_legs.clear();
    missing.unlock_condition = "Name missing discriminator".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&missing);
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_missing_discriminator must name missing discriminators",
    );
    assert_contains_error(
        &errors,
        "bun_blob_missing_resizable case must name resizable_array_buffer",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_missing_discriminator must name the missing boundary discriminator leg",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_missing_discriminator must suggest the configured Bun Blob TypeScript test file",
    );

    let mut missing_external_oracle = valid_cross_language_oracle_graph_case();
    missing_external_oracle.expected_state =
        "rust_ungripped_ts_missing_external_oracle".to_string();
    missing_external_oracle.gap_state = "static_limitation".to_string();
    missing_external_oracle.limitation_category =
        "cross_language_oracle_visibility_unresolved".to_string();
    missing_external_oracle.repair_route = "analysis/cross-language-oracle-visibility".to_string();
    missing_external_oracle.missing_discriminators = vec!["resizable_array_buffer".to_string()];
    missing_external_oracle.missing_graph_legs.clear();
    missing_external_oracle.view_backed_blob_input = true;
    missing_external_oracle.stable_byte_copy_oracle = true;
    missing_external_oracle.unlock_condition = "Name external oracle".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&missing_external_oracle);
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_missing_external_oracle requires a missing Blob input or stable-byte oracle",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_missing_external_oracle must not name missing boundary discriminators",
    );
    assert_contains_error(
        &errors,
        "rust_ungripped_ts_missing_external_oracle must name the missing external observer graph leg",
    );

    let mut mention = valid_cross_language_oracle_graph_case();
    mention.expected_state = "ts_mention_not_observer".to_string();
    mention.gap_state = "static_limitation".to_string();
    mention.limitation_category = "cross_language_oracle_visibility_unresolved".to_string();
    mention.repair_route = "analysis/cross-language-oracle-visibility".to_string();
    mention.max_byte_length_mention_only = false;
    mention.view_backed_blob_input = true;
    mention.stable_byte_copy_oracle = true;
    mention.missing_graph_legs.clear();
    mention.unlock_condition = "Name stable observer".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&mention);
    assert_contains_error(
        &errors,
        "ts_mention_not_observer must record max_byte_length_mention_only=true",
    );
    assert_contains_error(
        &errors,
        "ts_mention_not_observer must not count Blob input or stable-byte oracle facts",
    );
    assert_contains_error(
        &errors,
        "ts_mention_not_observer must name the missing external Blob callsite graph leg",
    );

    let mut bridge = valid_cross_language_oracle_graph_case();
    bridge.expected_state = "bridge_unknown".to_string();
    bridge.gap_state = "static_limitation".to_string();
    bridge.limitation_category = "cross_language_oracle_visibility_unresolved".to_string();
    bridge.repair_route = "analysis/cross-language-oracle-visibility".to_string();
    bridge.binding_edge_kind = "configured_bridge".to_string();
    bridge.binding_edge_confidence = "configured_hint".to_string();
    bridge.missing_graph_legs.clear();
    bridge.unlock_condition = "Name bridge".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&bridge);
    assert_contains_error(
        &errors,
        "rows with missing binding_or_ffi_edge must not claim a binding_edge raw ref",
    );
    assert_contains_error(
        &errors,
        "bridge_unknown requires unresolved binding edge and unknown confidence",
    );
    assert_contains_error(
        &errors,
        "bridge_unknown must name binding_or_ffi_edge as missing",
    );

    let mut target = valid_cross_language_oracle_graph_case();
    target.expected_state = "cross_language_target_unresolved".to_string();
    target.gap_state = "static_limitation".to_string();
    target.limitation_category = "cross_language_target_unresolved".to_string();
    target.repair_route = "analysis/cross-language-test-target-inference".to_string();
    target.missing_graph_legs.clear();
    target.unlock_condition = "Name target".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&target);
    assert_contains_error(
        &errors,
        "cross_language_target_unresolved must name safe_external_observer_target as missing",
    );

    let mut panic_boundary = valid_cross_language_oracle_graph_case();
    panic_boundary.profile = "bun_ffi_negative_offset_panic_boundary".to_string();
    panic_boundary.rust_file = "src/bun.js/bindings/FFIObject.rs".to_string();
    panic_boundary.rust_line = Some(277);
    panic_boundary.rust_owner = "FFIObject::read".to_string();
    panic_boundary.rust_boundary = "usize::try_from(to_int32()).expect(\"int cast\")".to_string();
    panic_boundary.binding_edge_kind = "ffi_binding".to_string();
    panic_boundary.binding_edge_confidence = "heuristic".to_string();
    panic_boundary.external_callsite_file = "unresolved:typescript-test-surface".to_string();
    panic_boundary.external_callsite_line = None;
    panic_boundary.external_entrypoints = vec!["read.u8(ptr, -1)".to_string()];
    panic_boundary.external_oracle_file = "unresolved:negative-offset-panic-oracle".to_string();
    panic_boundary.external_oracle_line = None;
    panic_boundary.expected_state = "public_reachable_panic_boundary_unrevealed".to_string();
    panic_boundary.gap_state = "static_limitation".to_string();
    panic_boundary.limitation_category =
        "cross_language_panic_boundary_visibility_unresolved".to_string();
    panic_boundary.repair_route = "analysis/cross-language-panic-boundary-visibility".to_string();
    panic_boundary.suggested_test_file = "test/js/bun/ffi/ffi.test.ts".to_string();
    panic_boundary.missing_discriminators.clear();
    panic_boundary.missing_graph_legs.clear();
    panic_boundary.external_oracle_kind = "wrong_oracle".to_string();
    panic_boundary.oracle_strength = "weak".to_string();
    panic_boundary.unlock_condition = "Name panic oracle".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&panic_boundary);
    assert_contains_error(
        &errors,
        "public_reachable_panic_boundary_unrevealed must name negative_offset as the missing discriminator",
    );
    assert_contains_error(
        &errors,
        "public_reachable_panic_boundary_unrevealed must name the missing negative-offset panic oracle leg",
    );
    assert_contains_error(
        &errors,
        "public_reachable_panic_boundary_unrevealed must name safe_external_observer_target as missing",
    );
    assert_contains_error(
        &errors,
        "public_reachable_panic_boundary_unrevealed must not suggest a test file",
    );
    assert_contains_error(
        &errors,
        "public_reachable_panic_boundary_unrevealed must use negative_offset_panic_oracle_unresolved oracle kind",
    );
    assert_contains_error(
        &errors,
        "public_reachable_panic_boundary_unrevealed must use missing_boundary oracle strength",
    );
}

#[test]
fn cross_language_oracle_graph_rejects_malformed_corpus_files() -> Result<(), String> {
    let root = temp_dir("cross-language-oracle-graph-malformed");
    let missing_path = root.join("missing.json");
    let mut violations = Vec::new();
    super::super::validate_cross_language_oracle_graph_fixture_corpus_at(
        &missing_path,
        &mut violations,
    )?;
    assert_contains_error(&violations, "Cross-language oracle graph corpus is missing");

    for (name, body, reason) in [
        (
            "bad-schema",
            r#"{"schema_version":"0.2","kind":"cross_language_oracle_graph_corpus","spec":"RIPR-SPEC-0062","cases":[]}"#,
            "schema_version must be 0.1",
        ),
        (
            "bad-kind",
            r#"{"schema_version":"0.1","kind":"other","spec":"RIPR-SPEC-0062","cases":[]}"#,
            "kind must be cross_language_oracle_graph_corpus",
        ),
        (
            "bad-spec",
            r#"{"schema_version":"0.1","kind":"cross_language_oracle_graph_corpus","spec":"RIPR-SPEC-9999","cases":[]}"#,
            "spec must be RIPR-SPEC-0062",
        ),
        (
            "missing-cases",
            r#"{"schema_version":"0.1","kind":"cross_language_oracle_graph_corpus","spec":"RIPR-SPEC-0062"}"#,
            "missing cases array",
        ),
    ] {
        let path = root.join(format!("{name}.json"));
        write(&path, body);
        let cases = super::super::cross_language_oracle_graph_cases_at(&path);
        assert_contains_error(std::slice::from_ref(&cases[0].reason), reason);

        let mut violations = Vec::new();
        super::super::validate_cross_language_oracle_graph_fixture_corpus_at(
            &path,
            &mut violations,
        )?;
        assert_contains_error(
            &violations,
            "Cross-language oracle graph case corpus: source must be present",
        );
    }

    Ok(())
}

#[test]
fn cross_language_oracle_graph_validates_bun_ffi_negative_offset_profile() {
    let mut case = valid_cross_language_oracle_graph_case();
    case.name = "bun_ffi_negative_offset_panic_boundary_limitation".to_string();
    case.source = "#950-ffi-negative-offset-public-reachable-panic-boundary".to_string();
    case.profile = "bun_ffi_negative_offset_panic_boundary".to_string();
    case.rust_file = "src/bun.js/bindings/FFIObject.rs".to_string();
    case.rust_line = Some(277);
    case.rust_owner = "FFIObject::read".to_string();
    case.rust_boundary = "usize::try_from(to_int32()).expect(\"int cast\")".to_string();
    case.binding_edge_kind = "ffi_binding".to_string();
    case.binding_edge_confidence = "heuristic".to_string();
    case.external_callsite_file = "unresolved:typescript-test-surface".to_string();
    case.external_callsite_line = None;
    case.external_entrypoints = vec!["read.u8(ptr, -1)".to_string()];
    case.shared_array_buffer = false;
    case.resizable_array_buffer = false;
    case.view_backed_blob_input = false;
    case.stable_byte_copy_oracle = false;
    case.external_oracle_file = "unresolved:negative-offset-panic-oracle".to_string();
    case.external_oracle_line = None;
    case.external_oracle_kind = "negative_offset_panic_oracle_unresolved".to_string();
    case.oracle_strength = "missing_boundary".to_string();
    case.expected_state = "public_reachable_panic_boundary_unrevealed".to_string();
    case.gap_state = "static_limitation".to_string();
    case.limitation_category = "cross_language_panic_boundary_visibility_unresolved".to_string();
    case.repair_route = "analysis/cross-language-panic-boundary-visibility".to_string();
    case.missing_discriminators = vec!["negative_offset".to_string()];
    case.missing_graph_legs = vec![
        "external_oracle:negative_offset_panic_boundary".to_string(),
        "safe_external_observer_target".to_string(),
    ];
    case.unlock_condition =
        "Trace and resolve the negative-offset panic oracle before actionability".to_string();
    case.raw_evidence_refs = vec![
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "rust_seam".to_string(),
            file: "src/bun.js/bindings/FFIObject.rs".to_string(),
            line: Some(277),
            kind: "rust_panic_boundary".to_string(),
            source_id: "#950".to_string(),
            sample: "FFIObject.rs negative-offset conversion panic boundary".to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "boundary_discriminator".to_string(),
            file: "issue:#950".to_string(),
            line: Some(1),
            kind: "negative_offset_discriminator_missing".to_string(),
            source_id: "#950".to_string(),
            sample: "read.u8(ptr, -1) is the boundary discriminator shape".to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "binding_edge".to_string(),
            file: "src/bun.js/bindings/FFIObject.rs".to_string(),
            line: Some(277),
            kind: "ffi_binding".to_string(),
            source_id: "#950".to_string(),
            sample: "Bun FFI read reaches the Rust owner through an FFI binding".to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "external_callsite".to_string(),
            file: "issue:#950".to_string(),
            line: Some(1),
            kind: "typescript_ffi_callsite".to_string(),
            source_id: "#950".to_string(),
            sample: "read.u8(ptr, -1) is the external callsite sample".to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "external_oracle".to_string(),
            file: "issue:#950".to_string(),
            line: Some(1),
            kind: "typescript_assertion_missing".to_string(),
            source_id: "#950".to_string(),
            sample: "no external assertion is proven for the panic boundary".to_string(),
        },
    ];

    let errors = super::super::cross_language_oracle_graph_case_errors(&case);
    assert!(
        errors.is_empty(),
        "Bun FFI negative-offset panic-boundary profile should validate: {errors:?}"
    );

    let mut wrong_shape = case.clone();
    wrong_shape.rust_file = "src/jsc/Blob.rs".to_string();
    wrong_shape.rust_owner = "Blob::from_js_without_defer_gc".to_string();
    wrong_shape.rust_boundary = "array_buffer.shared || array_buffer.resizable".to_string();
    wrong_shape.binding_edge_kind = "configured_bridge".to_string();
    wrong_shape.external_entrypoints = vec!["read.u32(ptr, 0)".to_string()];
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_shape);
    assert_contains_error(
        &errors,
        "rust_file must identify the Bun FFIObject Rust seam",
    );
    assert_contains_error(
        &errors,
        "rust_owner must pin FFIObject::read for the Bun FFI negative-offset route",
    );
    assert_contains_error(
        &errors,
        "rust_boundary must include usize::try_from and expect(\"int cast\")",
    );
    assert_contains_error(
        &errors,
        "bun_ffi_negative_offset_panic_boundary requires FFI binding edge evidence",
    );
    assert_contains_error(
        &errors,
        "bun_ffi_negative_offset_panic_boundary must name read.u8 as an external entrypoint",
    );

    let mut wrong_state = case;
    wrong_state.expected_state = "rust_ungripped_ts_discriminated".to_string();
    wrong_state.gap_state = "already_observed".to_string();
    wrong_state.limitation_category = "not_applicable".to_string();
    wrong_state.repair_route = "manual-review/cross-language-advisory-witness".to_string();
    wrong_state.missing_discriminators.clear();
    wrong_state.missing_graph_legs.clear();
    wrong_state.unlock_condition = "not_applicable".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_state);
    assert_contains_error(
        &errors,
        "bun_ffi_negative_offset_panic_boundary profile currently only supports public_reachable_panic_boundary_unrevealed",
    );
}

#[test]
fn cross_language_oracle_graph_validates_copy_to_unshared_profile() {
    let mut case = valid_cross_language_oracle_graph_case();
    case.name = "bun_array_buffer_copy_to_unshared_configured_bridge_advisory".to_string();
    case.source = "#910-copy-to-unshared".to_string();
    case.profile = "bun_array_buffer_copy_to_unshared".to_string();
    case.rust_file = "src/jsc/array_buffer.rs".to_string();
    case.rust_line = Some(341);
    case.rust_owner = "copy_to_unshared".to_string();
    case.rust_boundary = "SharedArrayBuffer and resizable ArrayBuffer copy semantics".to_string();

    let errors = super::super::cross_language_oracle_graph_case_errors(&case);
    assert!(
        errors.is_empty(),
        "copy_to_unshared configured bridge profile should validate: {errors:?}"
    );

    let mut bridge_unknown = case.clone();
    bridge_unknown.name = "bun_array_buffer_copy_to_unshared_bridge_unknown_limitation".to_string();
    bridge_unknown.expected_state = "bridge_unknown".to_string();
    bridge_unknown.gap_state = "static_limitation".to_string();
    bridge_unknown.limitation_category = "cross_language_oracle_visibility_unresolved".to_string();
    bridge_unknown.repair_route = "analysis/cross-language-oracle-visibility".to_string();
    bridge_unknown.binding_edge_kind = "unresolved".to_string();
    bridge_unknown.binding_edge_confidence = "unknown".to_string();
    bridge_unknown.missing_graph_legs = vec!["binding_or_ffi_edge:copy_to_unshared".to_string()];
    bridge_unknown.unlock_condition = "Name the copy_to_unshared bridge".to_string();
    bridge_unknown
        .raw_evidence_refs
        .retain(|raw_ref| raw_ref.leg != "binding_edge");
    let errors = super::super::cross_language_oracle_graph_case_errors(&bridge_unknown);
    assert!(
        errors.is_empty(),
        "copy_to_unshared bridge-unknown profile should still validate without a binding edge: {errors:?}"
    );

    let mut unproven_credit = case.clone();
    unproven_credit.binding_edge_kind = "unresolved".to_string();
    unproven_credit.binding_edge_confidence = "unknown".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&unproven_credit);
    assert_contains_error(
        &errors,
        "bun_array_buffer_copy_to_unshared configured route requires configured or generated binding edge evidence",
    );

    let mut wrong_shape = case.clone();
    wrong_shape.rust_file = "src/jsc/Blob.rs".to_string();
    wrong_shape.rust_owner = "Blob::from_js_without_defer_gc".to_string();
    wrong_shape.rust_boundary = "array_buffer.shared || array_buffer.resizable".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_shape);
    assert_contains_error(
        &errors,
        "rust_file must identify the Bun array_buffer Rust seam",
    );
    assert_contains_error(
        &errors,
        "rust_owner must pin copy_to_unshared for the Bun array_buffer route",
    );
    assert_contains_error(
        &errors,
        "rust_boundary must name SharedArrayBuffer and resizable ArrayBuffer copy semantics",
    );

    let mut unknown_profile = case;
    unknown_profile.profile = "unknown_profile".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&unknown_profile);
    assert_contains_error(
        &errors,
        "profile must be bun_blob_array_buffer, bun_array_buffer_copy_to_unshared, bun_markdown_resizable_array_buffer, bun_ffi_negative_offset_panic_boundary, bun_node_fs_scalar_write, or bun_write_helper_gated",
    );
}

#[test]
fn cross_language_oracle_route_quality_summarizes_corpus_cases() -> Result<(), String> {
    with_repo_cwd(|| {
        let value = super::super::cross_language_oracle_route_quality_report_value();
        assert_eq!(value["status"], "pass");
        assert_eq!(value["cases_total"], serde_json::Value::from(11));
        assert_eq!(
            value["cross_language_oracle_graph_complete_advisory_witnesses"],
            serde_json::Value::from(3)
        );
        assert_eq!(
            value["cross_language_oracle_graph_missing_discriminator_limitations"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            value["cross_language_oracle_graph_missing_external_oracle_limitations"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            value["cross_language_oracle_graph_bridge_unknown_limitations"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            value["cross_language_oracle_graph_mention_only_limitations"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            value["cross_language_oracle_graph_panic_boundary_limitations"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            value["cross_language_oracle_graph_manifest_only_profiles"],
            serde_json::Value::from(2)
        );
        assert_eq!(
            value["cross_language_oracle_graph_public_packet_exclusions"],
            serde_json::Value::from(11)
        );
        assert_eq!(
            value["repair_packet_ready_cases"],
            serde_json::Value::from(0)
        );

        let rows = value["rows"]
            .as_array()
            .ok_or_else(|| "route-quality rows must be an array".to_string())?;
        let missing_resizable = rows
            .iter()
            .find(|row| row["case_id"] == "bun_blob_missing_resizable_oracle_limitation")
            .ok_or_else(|| "missing resizable route-quality row".to_string())?;
        assert_eq!(
            missing_resizable["observed_state"],
            "rust_ungripped_ts_missing_discriminator"
        );
        assert!(
            missing_resizable["missing_discriminators"]
                .as_array()
                .is_some_and(|missing| {
                    missing.iter().any(|item| item == "resizable_array_buffer")
                })
        );
        assert_eq!(
            missing_resizable["suggested_test_file"],
            super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE
        );
        let missing_external_oracle = rows
            .iter()
            .find(|row| row["case_id"] == "bun_blob_missing_external_oracle_limitation")
            .ok_or_else(|| "missing external oracle route-quality row".to_string())?;
        assert_eq!(
            missing_external_oracle["observed_state"],
            "rust_ungripped_ts_missing_external_oracle"
        );
        assert!(
            missing_external_oracle["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs
                    .iter()
                    .any(|item| item == "external_oracle:stable_byte_copy"))
        );
        let bridge_unknown = rows
            .iter()
            .find(|row| row["case_id"] == "bun_blob_bridge_unknown_limitation")
            .ok_or_else(|| "missing bridge_unknown route-quality row".to_string())?;
        assert!(
            bridge_unknown["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs.iter().any(|item| item == "binding_or_ffi_edge"))
        );
        let copy_to_unshared = rows
            .iter()
            .find(|row| {
                row["case_id"] == "bun_array_buffer_copy_to_unshared_configured_bridge_advisory"
            })
            .ok_or_else(|| "missing copy_to_unshared configured bridge row".to_string())?;
        assert_eq!(
            copy_to_unshared["profile"],
            "bun_array_buffer_copy_to_unshared"
        );
        assert_eq!(
            copy_to_unshared["observed_state"],
            "rust_ungripped_ts_discriminated"
        );
        assert_eq!(
            copy_to_unshared["binding_edge_confidence"],
            "configured_hint"
        );
        assert!(
            copy_to_unshared["missing_graph_legs"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
        assert_eq!(copy_to_unshared["suggested_test_file"], "not_applicable");
        assert_eq!(copy_to_unshared["repair_packet_ready"], false);
        assert_eq!(copy_to_unshared["public_projection_eligible"], false);
        assert_eq!(
            copy_to_unshared["repair_route"],
            "manual-review/cross-language-advisory-witness"
        );
        let markdown_resizable = rows
            .iter()
            .find(|row| {
                row["case_id"] == "bun_markdown_resizable_array_buffer_configured_bridge_advisory"
            })
            .ok_or_else(|| "missing Bun markdown configured bridge row".to_string())?;
        assert_eq!(
            markdown_resizable["profile"],
            "bun_markdown_resizable_array_buffer"
        );
        assert_eq!(
            markdown_resizable["observed_state"],
            "rust_ungripped_ts_discriminated"
        );
        assert_eq!(
            markdown_resizable["binding_edge_confidence"],
            "configured_hint"
        );
        assert!(
            markdown_resizable["missing_graph_legs"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
        assert_eq!(markdown_resizable["suggested_test_file"], "not_applicable");
        assert_eq!(markdown_resizable["repair_packet_ready"], false);
        assert_eq!(markdown_resizable["public_projection_eligible"], false);
        assert_eq!(
            markdown_resizable["repair_route"],
            "manual-review/cross-language-advisory-witness"
        );
        let panic_boundary = rows
            .iter()
            .find(|row| row["case_id"] == "bun_ffi_negative_offset_panic_boundary_limitation")
            .ok_or_else(|| "missing Bun FFI panic-boundary limitation row".to_string())?;
        assert_eq!(
            panic_boundary["profile"],
            "bun_ffi_negative_offset_panic_boundary"
        );
        assert_eq!(
            panic_boundary["observed_state"],
            "public_reachable_panic_boundary_unrevealed"
        );
        assert_eq!(
            panic_boundary["limitation_category"],
            "cross_language_panic_boundary_visibility_unresolved"
        );
        assert_eq!(
            panic_boundary["repair_route"],
            "analysis/cross-language-panic-boundary-visibility"
        );
        assert_eq!(panic_boundary["suggested_test_file"], "not_applicable");
        assert_eq!(panic_boundary["repair_packet_ready"], false);
        assert_eq!(panic_boundary["public_projection_eligible"], false);
        assert!(
            panic_boundary["missing_discriminators"]
                .as_array()
                .is_some_and(|missing| missing.iter().any(|item| item == "negative_offset"))
        );
        assert!(
            panic_boundary["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs
                    .iter()
                    .any(|item| item == "external_oracle:negative_offset_panic_boundary"))
        );
        let node_fs = rows
            .iter()
            .find(|row| row["case_id"] == "bun_node_fs_scalar_write_manifest_only_profile")
            .ok_or_else(|| "missing node:fs scalar write manifest-only row".to_string())?;
        assert_eq!(node_fs["profile"], "bun_node_fs_scalar_write");
        assert_eq!(node_fs["profile_status"], "manifest_only");
        assert_eq!(node_fs["observed_state"], "named_static_limitation");
        assert_eq!(node_fs["proof_mode"], "observable_red_green");
        assert_eq!(node_fs["suggested_test_file"], "test/js/node/fs/fs.test.ts");
        assert_eq!(node_fs["repair_packet_ready"], false);
        assert_eq!(node_fs["public_projection_eligible"], false);
        assert!(
            node_fs["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs
                    .iter()
                    .any(|item| item == "binding_or_ffi_edge:node_fs_scalar_write"))
        );
        assert!(
            node_fs["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs
                    .iter()
                    .any(|item| item == "external_oracle:stable_byte_scalar_write"))
        );
        let bun_write = rows
            .iter()
            .find(|row| row["case_id"] == "bun_write_helper_gated_manifest_only_profile")
            .ok_or_else(|| "missing Bun.write helper-gated manifest-only row".to_string())?;
        assert_eq!(bun_write["profile"], "bun_write_helper_gated");
        assert_eq!(bun_write["profile_status"], "manifest_only");
        assert_eq!(bun_write["observed_state"], "named_static_limitation");
        assert_eq!(bun_write["proof_mode"], "helper_gated");
        assert_eq!(bun_write["suggested_test_file"], "not_applicable");
        assert_eq!(bun_write["repair_packet_ready"], false);
        assert_eq!(bun_write["public_projection_eligible"], false);
        assert!(
            bun_write["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs
                    .iter()
                    .any(|item| item == "helper:bun_write_fixture_helper"))
        );
        assert!(
            bun_write["missing_graph_legs"]
                .as_array()
                .is_some_and(|legs| legs
                    .iter()
                    .any(|item| item == "binding_or_ffi_edge:bun_write_sink"))
        );

        let mut markdown = String::new();
        super::super::cross_language_oracle_route_quality_push_markdown(&mut markdown, &value);
        assert!(markdown.contains("## Cross-Language Oracle Route Quality"));
        assert!(markdown.contains("Complete advisory witnesses"));
        assert!(markdown.contains("bun_blob_missing_resizable_oracle_limitation"));
        assert!(
            markdown.contains("bun_markdown_resizable_array_buffer_configured_bridge_advisory")
        );
        assert!(markdown.contains("Panic boundary limitations"));
        assert!(markdown.contains("bun_ffi_negative_offset_panic_boundary_limitation"));
        assert!(markdown.contains("Manifest-only profiles"));
        assert!(markdown.contains("bun_node_fs_scalar_write_manifest_only_profile"));
        assert!(markdown.contains("bun_write_helper_gated_manifest_only_profile"));
        assert!(markdown.contains("Missing external oracle limitations"));
        assert!(markdown.contains("binding_or_ffi_edge"));
        assert!(markdown.contains("preview/advisory route-quality evidence only"));
        Ok(())
    })
}

#[test]
fn cross_language_profile_intake_requires_helper_gated_rows() {
    let mut case = valid_cross_language_oracle_graph_case();
    case.name = "bun_write_helper_gated_manifest_only_profile".to_string();
    case.source = "profile-intake-bun-write-helper-gated".to_string();
    case.profile = "bun_write_helper_gated".to_string();
    case.profile_status = "manifest_only".to_string();
    case.rust_file = "unresolved:bun-write-stable-byte-rust-seam".to_string();
    case.rust_line = None;
    case.rust_owner = "Bun.write stable-byte sink".to_string();
    case.rust_boundary =
        "JS-owned bytes must not cross Bun.write native sinks without a helper".to_string();
    case.binding_edge_kind = "manifest_only".to_string();
    case.binding_edge_confidence = "manifest_only".to_string();
    case.external_callsite_file = "test/js/bun/write.test.ts".to_string();
    case.external_callsite_line = Some(1);
    case.external_entrypoints = vec![
        "Bun.write".to_string(),
        "Bun.write stable-byte sink".to_string(),
    ];
    case.shared_array_buffer = false;
    case.resizable_array_buffer = false;
    case.view_backed_blob_input = false;
    case.stable_byte_copy_oracle = false;
    case.max_byte_length_mention_only = false;
    case.external_oracle_file = "test/js/bun/write.test.ts".to_string();
    case.external_oracle_line = Some(1);
    case.external_oracle_kind = "stable_byte_write_oracle_helper_gated".to_string();
    case.oracle_strength = "helper_gated".to_string();
    case.expected_state = "named_static_limitation".to_string();
    case.gap_state = "static_limitation".to_string();
    case.limitation_category = "cross_language_profile_manifest_only".to_string();
    case.repair_route = "analysis/cross-language-profile-intake".to_string();
    case.suggested_test_file = "not_applicable".to_string();
    case.missing_graph_legs = vec![
        "binding_or_ffi_edge:bun_write_sink".to_string(),
        "helper:bun_write_fixture_helper".to_string(),
        "external_oracle:stable_byte_write".to_string(),
    ];
    case.unlock_condition =
            "Land or identify bun_write_fixture_helper and audit the Bun.write bridge/oracle before analyzer projection."
                .to_string();
    case.proof_mode = "helper_gated".to_string();
    case.raw_evidence_refs = vec![
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "rust_seam".to_string(),
            file: "issue:bun-write-helper-gated-profile".to_string(),
            line: Some(1),
            kind: "rust_seam_unresolved".to_string(),
            source_id: "profile-intake-bun-write-helper-gated".to_string(),
            sample:
                "Bun.write stable-byte Rust sink remains unresolved in the manifest-only profile"
                    .to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "boundary_discriminator".to_string(),
            file: "issue:bun-write-helper-gated-profile".to_string(),
            line: Some(1),
            kind: "stable_byte_write_discriminator".to_string(),
            source_id: "profile-intake-bun-write-helper-gated".to_string(),
            sample: "Bun.write is the helper-gated stable-byte boundary shape under intake"
                .to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "external_callsite".to_string(),
            file: "test/js/bun/write.test.ts".to_string(),
            line: Some(1),
            kind: "typescript_bun_write_callsite".to_string(),
            source_id: "profile-intake-bun-write-helper-gated".to_string(),
            sample: "Bun.write callsites are expected to be witnessed in write.test.ts".to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "external_oracle".to_string(),
            file: "test/js/bun/write.test.ts".to_string(),
            line: Some(1),
            kind: "typescript_assertion_helper_gated".to_string(),
            source_id: "profile-intake-bun-write-helper-gated".to_string(),
            sample: "stable-byte write assertion shape is blocked on bun_write_fixture_helper"
                .to_string(),
        },
    ];

    let errors = super::super::cross_language_oracle_graph_case_errors(&case);
    assert!(
        errors.is_empty(),
        "Bun.write helper-gated profile should validate: {errors:?}"
    );

    let mut wrong_seam = case.clone();
    wrong_seam.rust_file = "src/bun.js/bindings/BunWrite.rs".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_seam);
    assert_contains_error(
        &errors,
        "rust_file must keep the Bun.write stable-byte Rust seam unresolved",
    );

    let mut wrong_owner = case.clone();
    wrong_owner.rust_owner = "Bun.write resolved sink".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_owner);
    assert_contains_error(
        &errors,
        "rust_owner must pin the manifest-only Bun.write stable-byte sink",
    );

    let mut wrong_boundary = case.clone();
    wrong_boundary.rust_boundary = "native write helper".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_boundary);
    assert_contains_error(
        &errors,
        "rust_boundary must describe JS-owned bytes crossing the Bun.write boundary",
    );

    let mut active = case.clone();
    active.expected_state = "rust_ungripped_ts_discriminated".to_string();
    active.profile_status = "active".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&active);
    assert_contains_error(
        &errors,
        "bun_write_helper_gated profile is manifest_only and must stay a named_static_limitation",
    );
    assert_contains_error(&errors, "bun_write_helper_gated must be manifest_only");

    let mut wrong_proof = case.clone();
    wrong_proof.proof_mode = "observable_red_green".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_proof);
    assert_contains_error(
        &errors,
        "bun_write_helper_gated must record proof_mode helper_gated",
    );

    let mut bridge_credit = case.clone();
    bridge_credit.binding_edge_kind = "configured_bridge".to_string();
    bridge_credit.binding_edge_confidence = "configured_hint".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&bridge_credit);
    assert_contains_error(
        &errors,
        "bun_write_helper_gated must keep bridge evidence manifest_only",
    );

    let mut missing_entrypoint = case.clone();
    missing_entrypoint.external_entrypoints = vec!["writeFile".to_string()];
    let errors = super::super::cross_language_oracle_graph_case_errors(&missing_entrypoint);
    assert_contains_error(
        &errors,
        "bun_write_helper_gated must name Bun.write as an external entrypoint",
    );

    let mut wrong_witness_path = case.clone();
    wrong_witness_path.external_callsite_file = "test/js/node/fs/fs.test.ts".to_string();
    wrong_witness_path.external_oracle_file = "test/js/node/fs/fs.test.ts".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_witness_path);
    assert_contains_error(
        &errors,
        "bun_write_helper_gated must record test/js/bun/write.test.ts as the manifest witness path",
    );

    let mut wrong_oracle = case.clone();
    wrong_oracle.external_oracle_kind = "stable_byte_write_oracle".to_string();
    wrong_oracle.oracle_strength = "manifest_only".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_oracle);
    assert_contains_error(
        &errors,
        "bun_write_helper_gated must keep write-oracle evidence helper_gated",
    );

    let mut missing_helper = case.clone();
    missing_helper
        .missing_graph_legs
        .retain(|leg| leg != "helper:bun_write_fixture_helper");
    let errors = super::super::cross_language_oracle_graph_case_errors(&missing_helper);
    assert_contains_error(
        &errors,
        "bun_write_helper_gated must name helper:bun_write_fixture_helper as missing",
    );

    let mut suggested = case.clone();
    suggested.suggested_test_file = "test/js/bun/write.test.ts".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&suggested);
    assert_contains_error(
        &errors,
        "bun_write_helper_gated must not suggest placement while helper-gated",
    );

    let mut binding_credit = case;
    binding_credit
        .raw_evidence_refs
        .push(super::super::CrossLanguageOracleGraphRawRef {
            leg: "binding_edge".to_string(),
            file: "src/bun.js/bindings/BunWrite.rs".to_string(),
            line: Some(1),
            kind: "configured_bridge".to_string(),
            source_id: "profile-intake-bun-write-helper-gated".to_string(),
            sample: "helper-gated Bun.write intake must not claim a binding edge".to_string(),
        });
    let errors = super::super::cross_language_oracle_graph_case_errors(&binding_credit);
    assert_contains_error(
        &errors,
        "rows with missing binding_or_ffi_edge must not claim a binding_edge raw ref",
    );
}

#[test]
fn cross_language_profile_intake_requires_manifest_rows() {
    let mut case = valid_cross_language_oracle_graph_case();
    case.name = "bun_node_fs_scalar_write_manifest_only_profile".to_string();
    case.source = "profile-intake-node-fs-scalar-write".to_string();
    case.profile = "bun_node_fs_scalar_write".to_string();
    case.profile_status = "manifest_only".to_string();
    case.rust_file = "unresolved:node-fs-scalar-write-rust-seam".to_string();
    case.rust_line = None;
    case.rust_owner = "node:fs scalar write sink".to_string();
    case.rust_boundary =
        "JS-owned bytes must be copied before native write scalar sinks".to_string();
    case.binding_edge_kind = "manifest_only".to_string();
    case.binding_edge_confidence = "manifest_only".to_string();
    case.external_callsite_file = "test/js/node/fs/fs.test.ts".to_string();
    case.external_callsite_line = Some(1);
    case.external_entrypoints = vec![
        "node:fs scalar write".to_string(),
        "fs.writeFileSync".to_string(),
        "fs.writeSync".to_string(),
    ];
    case.shared_array_buffer = false;
    case.resizable_array_buffer = false;
    case.view_backed_blob_input = false;
    case.stable_byte_copy_oracle = false;
    case.max_byte_length_mention_only = false;
    case.external_oracle_file = "test/js/node/fs/fs.test.ts".to_string();
    case.external_oracle_line = Some(1);
    case.external_oracle_kind = "stable_byte_scalar_write_oracle_manifest_only".to_string();
    case.oracle_strength = "manifest_only".to_string();
    case.expected_state = "named_static_limitation".to_string();
    case.gap_state = "static_limitation".to_string();
    case.limitation_category = "cross_language_profile_manifest_only".to_string();
    case.repair_route = "analysis/cross-language-profile-intake".to_string();
    case.suggested_test_file = "test/js/node/fs/fs.test.ts".to_string();
    case.missing_graph_legs = vec![
        "binding_or_ffi_edge:node_fs_scalar_write".to_string(),
        "external_oracle:stable_byte_scalar_write".to_string(),
    ];
    case.unlock_condition =
            "Audit the node:fs scalar write Rust sink and bridge/oracle evidence before analyzer projection."
                .to_string();
    case.proof_mode = "observable_red_green".to_string();
    case.raw_evidence_refs = vec![
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "rust_seam".to_string(),
            file: "issue:node-fs-scalar-write-profile".to_string(),
            line: Some(1),
            kind: "rust_seam_unresolved".to_string(),
            source_id: "profile-intake-node-fs-scalar-write".to_string(),
            sample:
                "node:fs scalar write Rust sink remains unresolved in the manifest-only profile"
                    .to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "boundary_discriminator".to_string(),
            file: "issue:node-fs-scalar-write-profile".to_string(),
            line: Some(1),
            kind: "stable_byte_scalar_write_discriminator".to_string(),
            source_id: "profile-intake-node-fs-scalar-write".to_string(),
            sample: "scalar write is the stable-byte boundary shape under intake".to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "external_callsite".to_string(),
            file: "test/js/node/fs/fs.test.ts".to_string(),
            line: Some(1),
            kind: "typescript_node_fs_callsite".to_string(),
            source_id: "profile-intake-node-fs-scalar-write".to_string(),
            sample: "node:fs scalar writes are expected to be witnessed in fs.test.ts".to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "external_oracle".to_string(),
            file: "test/js/node/fs/fs.test.ts".to_string(),
            line: Some(1),
            kind: "typescript_assertion_manifest_only".to_string(),
            source_id: "profile-intake-node-fs-scalar-write".to_string(),
            sample: "stable-byte scalar write assertion shape is manifest-only".to_string(),
        },
        super::super::CrossLanguageOracleGraphRawRef {
            leg: "placement".to_string(),
            file: "test/js/node/fs/fs.test.ts".to_string(),
            line: Some(1),
            kind: "typed_test_placement".to_string(),
            source_id: "profile-intake-node-fs-scalar-write".to_string(),
            sample: "typed node:fs placement is pinned without actionability".to_string(),
        },
    ];
    let errors = super::super::cross_language_oracle_graph_case_errors(&case);
    assert!(
        errors.is_empty(),
        "node:fs manifest-only profile should validate: {errors:?}"
    );

    let mut active = case.clone();
    active.profile_status = "active".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&active);
    assert_contains_error(
        &errors,
        "named_static_limitation profile intake rows must be manifest_only",
    );
    assert_contains_error(&errors, "bun_node_fs_scalar_write must be manifest_only");

    let mut no_placement_ref = case.clone();
    no_placement_ref
        .raw_evidence_refs
        .retain(|raw_ref| raw_ref.leg != "placement");
    let errors = super::super::cross_language_oracle_graph_case_errors(&no_placement_ref);
    assert_contains_error(
        &errors,
        "suggested_test_file must be not_applicable, the configured Bun Blob TypeScript test file for missing-discriminator rows, or the typed node:fs manifest placement",
    );

    let mut binding_credit = case.clone();
    binding_credit
        .raw_evidence_refs
        .push(super::super::CrossLanguageOracleGraphRawRef {
            leg: "binding_edge".to_string(),
            file: "src/node/fs.rs".to_string(),
            line: Some(1),
            kind: "configured_bridge".to_string(),
            source_id: "profile-intake-node-fs-scalar-write".to_string(),
            sample: "manifest-only node:fs intake must not claim a binding edge".to_string(),
        });
    let errors = super::super::cross_language_oracle_graph_case_errors(&binding_credit);
    assert_contains_error(
        &errors,
        "rows with missing binding_or_ffi_edge must not claim a binding_edge raw ref",
    );

    let mut wrong_proof = case;
    wrong_proof.proof_mode = "mutation_plus_miri".to_string();
    let errors = super::super::cross_language_oracle_graph_case_errors(&wrong_proof);
    assert_contains_error(
        &errors,
        "bun_node_fs_scalar_write must record proof_mode observable_red_green",
    );
}

// ============================================================================
// Test efficiency and declared intent tests
// ============================================================================

#[test]
fn declared_intent_structure_holds_intent_and_metadata() {
    let intent = DeclaredIntent {
        intent: TestIntentKind::Smoke,
        owner: "docs".to_string(),
        reason: "example".to_string(),
        source: "docs/example.md".to_string(),
    };
    assert_eq!(intent.intent, TestIntentKind::Smoke);
    assert_eq!(intent.owner, "docs");
    assert_eq!(intent.reason, "example");
}

#[test]
fn defaults_first_example_corpus_index_names_required_operator_artifacts() -> Result<(), String> {
    with_repo_cwd(|| {
        let text = fs::read_to_string("fixtures/EXAMPLE_CORPUS.md")
            .map_err(|err| format!("failed to read example corpus index: {err}"))?;
        for required_text in [
            "Boundary gap",
            "Missing equality boundary",
            "Weak oracle",
            "Exact error variant",
            "Opaque fixture/builder",
            "Optional calibration",
            "targeted-test-outcome.json",
            "mutation-calibration.json",
            "lsp-code-actions.json",
            "editor-agent-loop/agent-packet.json",
            "editor-agent-loop/agent-brief.json",
            "editor-agent-loop/agent-verify.json",
            "editor-agent-loop/agent-receipt.json",
            "editor-agent-loop/operator-cockpit.json",
        ] {
            assert!(
                text.contains(required_text),
                "example corpus index should mention {required_text}"
            );
        }
        for required_path in [
            "fixtures/boundary_gap/expected/lsp-diagnostics.json",
            "fixtures/boundary_gap/expected/lsp-code-actions.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/agent-packet.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/agent-brief.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/agent-verify.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/agent-receipt.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/operator-cockpit.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/operator-cockpit.md",
            "fixtures/boundary_gap/calibration/before-targeted-test.repo-exposure.json",
            "fixtures/boundary_gap/calibration/after-targeted-test.repo-exposure.json",
            "fixtures/boundary_gap/calibration/targeted-test-outcome.json",
            "fixtures/boundary_gap/calibration/targeted-test-outcome.md",
            "fixtures/boundary_gap/calibration/runtime-mutants.json",
            "fixtures/boundary_gap/calibration/mutation-calibration.json",
            "fixtures/boundary_gap/calibration/mutation-calibration.md",
            "fixtures/opaque_fixture_builder/expected/check.json",
            "fixtures/opaque_fixture_builder/expected/human.txt",
        ] {
            assert!(
                Path::new(required_path).exists(),
                "example corpus artifact should exist: {required_path}"
            );
        }
        Ok(())
    })
}

#[test]
fn dependency_policy_detects_package_manager_surfaces() {
    assert!(is_dependency_surface_candidate("Cargo.toml"));
    assert!(is_dependency_surface_candidate("xtask/Cargo.toml"));
    assert!(is_dependency_surface_candidate(
        "editors/vscode/package.json"
    ));
    assert!(is_dependency_surface_candidate(
        "fixtures/example/input/Cargo.toml"
    ));
    assert!(is_dependency_surface_candidate(
        "tools/example/requirements.txt"
    ));
    assert!(!is_dependency_surface_candidate("docs/DEPENDENCIES.md"));
}

#[test]
fn deprecated_traceability_entry_may_omit_runtime_metrics() -> Result<(), String> {
    with_temp_cwd("traceability-deprecated-without-metrics", |root| {
        let spec = root.join("docs/specs/RIPR-SPEC-9998-deprecated.md");
        write(
            &spec,
            "# RIPR-SPEC-9998: Deprecated\n\nStatus: deprecated\n",
        );

        let behavior = super::super::TraceBehavior {
            line: 1,
            id: Some("RIPR-SPEC-9998".to_string()),
            name: Some("deprecated documentation contract".to_string()),
            spec: Some(spec.to_string_lossy().replace('\\', "/")),
            outputs: vec![spec.to_string_lossy().replace('\\', "/")],
            ..Default::default()
        };
        let mut behavior_ids = BTreeSet::new();
        let mut violations = Vec::new();
        let mut advisories = Vec::new();

        super::super::validate_trace_behavior(
            &behavior,
            &mut behavior_ids,
            &mut violations,
            &mut advisories,
        )?;

        if !violations.is_empty() {
            return Err(format!(
                "deprecated documentation entry should not require a runtime metric: {violations:?}"
            ));
        }
        Ok(())
    })
}

#[test]
fn diff_badge_artifact_command_args_still_use_git_diff_input() -> Result<(), String> {
    // Companion to the repo regression: the diff-scope contract is that
    // the args DO consult `git diff origin/main...HEAD` (captured to
    // target/ripr/badge-input.diff before render). Pinning this here
    // catches any accidental drift to a unified scope path.
    for format in [
        "badge-json",
        "badge-shields",
        "badge-plus-json",
        "badge-plus-shields",
    ] {
        let args = badge_artifact_command_args(format);
        if !args.iter().any(|arg| arg == "--diff") {
            return Err(format!(
                "diff-scope command args must contain `--diff` for {format:?}: {args:?}"
            ));
        }
        if !args.iter().any(|arg| arg.contains("badge-input.diff")) {
            return Err(format!(
                "diff-scope command args must reference badge-input.diff for {format:?}: {args:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn duplicate_grouping_assigns_deterministic_group_ids() {
    let build = || {
        vec![
            duplicate_entry(
                "a",
                10,
                "strong_discriminator",
                "exact assertion",
                "strong",
                &["pricing::a"],
                &[("function_argument", "1")],
            ),
            duplicate_entry(
                "b",
                20,
                "strong_discriminator",
                "exact assertion",
                "strong",
                &["pricing::a"],
                &[("function_argument", "1")],
            ),
            duplicate_entry(
                "c",
                30,
                "strong_discriminator",
                "exact assertion",
                "strong",
                &["pricing::b"],
                &[("function_argument", "1")],
            ),
            duplicate_entry(
                "d",
                40,
                "strong_discriminator",
                "exact assertion",
                "strong",
                &["pricing::b"],
                &[("function_argument", "1")],
            ),
        ]
    };

    let mut first = build();
    let groups_first = apply_duplicate_discriminator_groups(&mut first);
    let mut second = build();
    let groups_second = apply_duplicate_discriminator_groups(&mut second);

    assert_eq!(groups_first.len(), 2);
    assert_eq!(groups_first[0].id, "duplicate_group_1");
    assert_eq!(groups_first[1].id, "duplicate_group_2");
    for (a, b) in groups_first.iter().zip(groups_second.iter()) {
        assert_eq!(a.id, b.id);
        let names_a: Vec<_> = a.members.iter().map(|m| &m.name).collect();
        let names_b: Vec<_> = b.members.iter().map(|m| &m.name).collect();
        assert_eq!(names_a, names_b);
    }
}

#[test]
fn duplicate_grouping_does_not_group_different_activation_values() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "b",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "2")],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty());
}

#[test]
fn duplicate_grouping_does_not_group_different_oracle_kind() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "b",
            30,
            "useful_but_broad",
            "broad predicate",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty());
}

#[test]
fn duplicate_grouping_does_not_group_different_oracle_strength() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "b",
            30,
            "useful_but_broad",
            "exact assertion",
            "weak",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty());
}

#[test]
fn duplicate_grouping_does_not_group_different_owners() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::a"],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "b",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::b"],
            &[("function_argument", "1")],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty());
    for entry in &entries {
        assert_eq!(entry.class, "strong_discriminator");
        assert!(entry.duplicate_group_id.is_none());
    }
}

#[test]
fn duplicate_grouping_does_not_group_swapped_function_and_assertion_values() {
    let mut entries = vec![
        duplicate_entry(
            "score_two_equals_three",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::score"],
            &[("function_argument", "2"), ("assertion_argument", "3")],
        ),
        duplicate_entry(
            "score_three_equals_two",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::score"],
            &[("function_argument", "3"), ("assertion_argument", "2")],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(
        groups.is_empty(),
        "swapped activation/expected values must not group: same raw value set, different roles"
    );
}

#[test]
fn duplicate_grouping_does_not_group_swapped_function_argument_order() {
    let mut entries = vec![
        duplicate_entry(
            "f_one_two",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::f"],
            &[("function_argument", "1"), ("function_argument", "2")],
        ),
        duplicate_entry(
            "f_two_one",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::f"],
            &[("function_argument", "2"), ("function_argument", "1")],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty(), "argument order must matter");
}

#[test]
fn duplicate_grouping_emits_per_test_id_linked_to_top_level_groups() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "b",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
    ];
    let groups = apply_duplicate_discriminator_groups(&mut entries);
    let json = test_efficiency_report_json(&entries, &groups, &TestIntentReportSummary::default());

    assert!(json.contains("\"duplicate_group_id\": \"duplicate_group_1\""));
    assert!(json.contains("\"duplicate_groups\": ["));
    assert!(json.contains("\"id\": \"duplicate_group_1\""));
    assert!(json.contains("\"duplicative\": 2"));
    assert!(json.contains("\"duplicate_activation_and_oracle_shape\": 2"));
}

#[test]
fn duplicate_grouping_excludes_entries_with_no_activation_literals() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[],
        ),
        duplicate_entry(
            "b",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(
        groups.is_empty(),
        "tests with no activation literals must not group; signature would be empty"
    );
}

#[test]
fn duplicate_grouping_excludes_opaque_entries() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "opaque",
            "exact assertion",
            "strong",
            &[],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "b",
            30,
            "opaque",
            "exact assertion",
            "strong",
            &[],
            &[("function_argument", "1")],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty());
    for entry in &entries {
        assert_eq!(entry.class, "opaque");
        assert!(entry.duplicate_group_id.is_none());
    }
}

#[test]
fn duplicate_grouping_groups_structurally_identical_eligible_tests() {
    let mut entries = vec![
        duplicate_entry(
            "discount_above_threshold",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::discounted_total"],
            &[("function_argument", "100"), ("assertion_argument", "90")],
        ),
        duplicate_entry(
            "vip_discount_above_threshold",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::discounted_total"],
            &[("function_argument", "100"), ("assertion_argument", "90")],
        ),
    ];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, "duplicate_group_1");
    assert_eq!(groups[0].members.len(), 2);
    for entry in &entries {
        assert_eq!(entry.class, "duplicative");
        assert!(
            entry
                .reasons
                .contains(&"duplicate_activation_and_oracle_shape".to_string())
        );
        assert_eq!(
            entry.duplicate_group_id.as_deref(),
            Some("duplicate_group_1")
        );
    }
}

#[test]
fn duplicate_grouping_ignores_single_candidate() {
    let mut entries = vec![duplicate_entry(
        "a",
        10,
        "strong_discriminator",
        "exact assertion",
        "strong",
        &["pricing::p"],
        &[("function_argument", "1")],
    )];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty());
    assert_eq!(entries[0].class, "strong_discriminator");
    assert!(entries[0].duplicate_group_id.is_none());
}

#[test]
fn duplicate_grouping_keeps_likely_vacuous_priority() {
    let mut a = duplicate_entry(
        "a",
        10,
        "likely_vacuous",
        "exact assertion",
        "strong",
        &["pricing::p"],
        &[("function_argument", "1")],
    );
    a.reasons.push("no_assertion_detected".to_string());
    let mut b = duplicate_entry(
        "b",
        30,
        "likely_vacuous",
        "exact assertion",
        "strong",
        &["pricing::p"],
        &[("function_argument", "1")],
    );
    b.reasons.push("no_assertion_detected".to_string());
    let mut entries = vec![a, b];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty());
    for entry in &entries {
        assert_eq!(entry.class, "likely_vacuous");
    }
}

#[test]
fn duplicate_grouping_keeps_possibly_circular_priority() {
    let mut a = duplicate_entry(
        "a",
        10,
        "possibly_circular",
        "exact assertion",
        "strong",
        &["pricing::p"],
        &[("function_argument", "1")],
    );
    a.reasons
        .push("expected_value_computed_from_detected_owner_path".to_string());
    let mut b = duplicate_entry(
        "b",
        30,
        "possibly_circular",
        "exact assertion",
        "strong",
        &["pricing::p"],
        &[("function_argument", "1")],
    );
    b.reasons
        .push("expected_value_computed_from_detected_owner_path".to_string());
    let mut entries = vec![a, b];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert!(groups.is_empty());
    for entry in &entries {
        assert_eq!(entry.class, "possibly_circular");
    }
}

#[test]
fn duplicate_grouping_markdown_renders_groups_without_deletion_language() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "b",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
    ];
    let groups = apply_duplicate_discriminator_groups(&mut entries);
    let markdown =
        test_efficiency_report_markdown(&entries, &groups, &TestIntentReportSummary::default());

    assert!(markdown.contains("## Duplicate Discriminator Groups"));
    assert!(markdown.contains("duplicate_group_1"));
    assert!(markdown.contains("Duplicate discriminator groups: 1"));
    assert!(
        !markdown.to_ascii_lowercase().contains("delete"),
        "report must not recommend deleting tests"
    );
}

#[test]
fn duplicate_grouping_promotes_smoke_duplicates_and_keeps_smoke_reason() {
    let mut a = duplicate_entry(
        "a",
        10,
        "smoke_only",
        "smoke only",
        "smoke",
        &["pricing::p"],
        &[("function_argument", "1")],
    );
    a.reasons.push("smoke_oracle_only".to_string());
    let mut b = duplicate_entry(
        "b",
        30,
        "smoke_only",
        "smoke only",
        "smoke",
        &["pricing::p"],
        &[("function_argument", "1")],
    );
    b.reasons.push("smoke_oracle_only".to_string());
    let mut entries = vec![a, b];

    let groups = apply_duplicate_discriminator_groups(&mut entries);

    assert_eq!(groups.len(), 1);
    for entry in &entries {
        assert_eq!(entry.class, "duplicative");
        assert!(
            entry.reasons.contains(&"smoke_oracle_only".to_string()),
            "smoke duplicate must retain smoke_oracle_only reason"
        );
        assert!(
            entry
                .reasons
                .contains(&"duplicate_activation_and_oracle_shape".to_string()),
            "smoke duplicate must add duplicate reason"
        );
    }
}

#[test]
fn editor_adoption_assurance_fixture_corpus_validator_accepts_complete_matrix() -> Result<(), String>
{
    with_temp_cwd("editor-adoption-assurance-fixtures", |root| {
        write_editor_adoption_assurance_corpus(root);

        let mut violations = Vec::new();
        super::super::validate_editor_adoption_assurance_fixture_corpus(&mut violations)?;
        assert_eq!(violations, Vec::<String>::new());

        let fixture_dirs = super::super::lsp_cockpit_fixture_dirs()?;
        let fixture_names = fixture_dirs
            .iter()
            .map(|(fixture, _)| fixture.as_str())
            .collect::<BTreeSet<_>>();
        assert!(fixture_names.contains("editor_adoption_assurance/first_pr_packet_ready"));
        assert!(fixture_names.contains("editor_adoption_assurance/server_version_mismatch"));

        let (ready_name, ready_path) = fixture_dirs
            .iter()
            .find(|(fixture, _)| {
                fixture.as_str() == "editor_adoption_assurance/first_pr_packet_ready"
            })
            .ok_or_else(|| "first-pr ready fixture must exist".to_string())?;
        let ready = super::super::lsp_cockpit_fixture_report(ready_name, ready_path).and_then(
            |report| report.ok_or_else(|| "first-pr ready report must exist".to_string()),
        )?;
        assert_eq!(ready.diagnostic_count, 1);
        assert!(
            ready
                .action_commands
                .contains(&"ripr.copyFirstPrRepairPacket".to_string())
        );
        Ok(())
    })
}

#[test]
fn editor_adoption_assurance_fixture_corpus_validator_reports_unsafe_states() -> Result<(), String>
{
    with_temp_cwd("editor-adoption-assurance-violations", |root| {
        let mut violations = Vec::new();
        super::super::validate_editor_adoption_assurance_fixture_corpus(&mut violations)?;
        assert!(violations.iter().any(|violation| {
            violation.contains("editor adoption assurance fixture corpus is missing")
        }));

        write_editor_adoption_assurance_corpus(root);
        write_editor_adoption_status(root, "server_missing", "server_missing", "active");
        write_editor_adoption_actions(
            root,
            "first_pr_packet_mismatch",
            &[
                "First PR: open packet",
                "Refresh Analysis - Saved Workspace Check",
            ],
        );
        write_editor_adoption_actions(
            root,
            "first_pr_packet_ready",
            &[
                "First PR: open packet",
                "Refresh Analysis - Saved Workspace Check",
            ],
        );
        write_editor_adoption_first_pr(root, "first_pr_packet_ready", "missing");
        write_editor_adoption_receipt(root, "stale_receipt", "receipt_missing", true, true);

        let mut violations = Vec::new();
        super::super::validate_editor_adoption_assurance_fixture_corpus(&mut violations)?;
        assert!(violations.iter().any(|violation| {
            violation.contains("editor adoption assurance case server_missing must fail closed")
        }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor adoption assurance case first_pr_packet_mismatch must suppress repair/proof actions",
                )
            }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor adoption assurance case first_pr_packet_ready must include ripr.copyFirstPrRepairPacket",
                )
            }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor adoption assurance case first_pr_packet_ready first-pr-status packet_state must be top_repairable_gap",
                )
            }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor adoption assurance case stale_receipt receipt-status receipt_state must be receipt_stale",
                )
            }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor adoption assurance case stale_receipt receipt-status must deny runtime_adequacy_claim",
                )
            }));
        Ok(())
    })
}

#[test]
fn editor_first_run_usability_fixture_corpus_validator_accepts_complete_matrix()
-> Result<(), String> {
    with_temp_cwd("editor-first-run-usability-fixtures", |root| {
        write_editor_first_run_usability_corpus(root);

        let mut violations = Vec::new();
        super::super::validate_editor_first_run_usability_fixture_corpus(&mut violations)?;
        assert_eq!(violations, Vec::<String>::new());
        Ok(())
    })
}

#[test]
fn editor_first_run_usability_fixture_corpus_validator_reports_unsafe_states() -> Result<(), String>
{
    with_temp_cwd("editor-first-run-usability-violations", |root| {
        let mut violations = Vec::new();
        super::super::validate_editor_first_run_usability_fixture_corpus(&mut violations)?;
        assert!(violations.iter().any(|violation| {
            violation.contains("editor first-run usability fixture corpus is missing")
        }));

        write_editor_first_run_usability_corpus(root);
        write_editor_first_run_status(root, "server_missing", "active", None);
        write_editor_first_run_actions(root, "setup_ok", &[]);
        write_editor_first_run_actions(root, "receipt_found", &[]);
        write_editor_first_run_actions(root, "artifact_missing", &["Copy first repair packet"]);
        write(
            &root.join(
                "fixtures/editor_first_run_usability/artifact_stale/expected/receipt-status.json",
            ),
            &serde_json::json!({
                "schema_version": "0.1",
                "receipt_state": "receipt_missing",
                "runtime_adequacy_claim": true,
                "gate_eligibility_claim": true
            })
            .to_string(),
        );

        let mut violations = Vec::new();
        super::super::validate_editor_first_run_usability_fixture_corpus(&mut violations)?;
        assert!(violations.iter().any(|violation| {
            violation.contains("editor first-run usability case server_missing must fail closed")
        }));
        assert!(violations.iter().any(|violation| {
            violation.contains(
                "editor first-run usability case setup_ok must include Copy first repair packet",
            )
        }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor first-run usability case receipt_found must include Copy first repair packet",
                )
            }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor first-run usability case artifact_missing must not expose first repair packet",
                )
            }));
        assert!(violations.iter().any(|violation| {
            violation
                .contains("editor first-run usability artifact_stale must record receipt_stale")
        }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor first-run usability case artifact_stale receipt-status must deny runtime_adequacy_claim",
                )
            }));
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor first-run usability case artifact_stale receipt-status must deny gate_eligibility_claim",
                )
            }));
        Ok(())
    })
}

#[test]
fn editor_gap_cockpit_fixture_case_guard_accepts_actionable_contract() -> Result<(), String> {
    let root = temp_dir("editor-gap-cockpit-valid");
    write_editor_gap_case_expected(
        &root,
        "rust_actionable",
        r#"{
  "diagnostics": [
    {
      "data": {
        "language": "rust",
        "language_status": "stable",
        "canonical_gap_id": "gap:rust:pricing"
      }
    }
  ]
}
"#,
        r#"{
  "actions": [
    {"title": "Inspect gap: copy repair packet", "command": "ripr.copyAgentPacketCommand", "arguments": [{"command": "ripr agent packet"}]},
    {"title": "Write targeted test: open best related test", "command": "ripr.openRelatedTest", "arguments": [{"uri": "file:///repo/tests/pricing.rs"}]},
    {"title": "Verify after test: copy verify command", "command": "ripr.copyAgentVerifyCommand", "arguments": [{"command": "ripr agent verify"}]},
    {"title": "Review result: copy receipt command", "command": "ripr.copyAgentReceiptCommand", "arguments": [{"command": "ripr agent receipt"}]},
    {"title": "Refresh Analysis - Saved Workspace Check", "command": "ripr.refresh"}
  ]
}
"#,
        "# Hover\n\n## Evidence boundary\n\n## Gap state\n\n## Limits\n",
    );

    let mut violations = Vec::new();
    super::super::validate_editor_gap_cockpit_fixture_case(
        &root,
        "rust_actionable",
        &mut violations,
    )?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

/// #4001 control 12: a checked-in editor projection must name the selected
/// workspace; a portable `--root .` command is a violation.
#[test]
fn editor_gap_cockpit_fixture_case_guard_rejects_portable_root_commands() -> Result<(), String> {
    let root = temp_dir("editor-gap-cockpit-portable-root");
    write_editor_gap_case_expected(
        &root,
        "rust_actionable",
        r#"{
  "diagnostics": [
    {
      "data": {
        "language": "rust",
        "language_status": "stable",
        "canonical_gap_id": "gap:rust:pricing",
        "receipt_command": "ripr agent receipt --root . --json"
      }
    }
  ]
}
"#,
        r#"{
  "actions": [
    {"title": "Inspect gap: copy repair packet", "command": "ripr.copyAgentPacketCommand", "arguments": [{"command": "ripr agent packet"}]},
    {"title": "Write targeted test: open best related test", "command": "ripr.openRelatedTest", "arguments": [{"uri": "file:///repo/tests/pricing.rs"}]},
    {"title": "Verify after test: copy verify command", "command": "ripr.copyAgentVerifyCommand", "arguments": [{"command": "ripr agent verify --root <root> --json"}]},
    {"title": "Review result: copy receipt command", "command": "ripr.copyAgentReceiptCommand", "arguments": [{"command": "ripr agent receipt --root <root> --json"}]},
    {"title": "Refresh Analysis - Saved Workspace Check", "command": "ripr.refresh"}
  ]
}
"#,
        "# Hover\n\n## Evidence boundary\n\n## Gap state\n\n- verify: `ripr agent verify --root ./sub --json`\n\n## Limits\n",
    );

    let mut violations = Vec::new();
    super::super::validate_editor_gap_cockpit_fixture_case(
        &root,
        "rust_actionable",
        &mut violations,
    )?;

    // Only the diagnostics file carries `--root .`; `<root>` and `./sub` pass.
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("lsp-diagnostics.json") && violations[0].contains("--root ."),
        "{violations:?}"
    );
    Ok(())
}

#[test]
fn editor_gap_cockpit_fixture_case_guard_reports_projection_drift() -> Result<(), String> {
    let root = temp_dir("editor-gap-cockpit-invalid-preview");
    let expected = root
        .join("typescript_preview_static_limit")
        .join("expected");
    write(
        &expected.join("gap-projection.json"),
        r#"{
  "schema_version": "0.2",
  "case": "wrong_case"
}
"#,
    );
    write(
        &expected.join("lsp-diagnostics.json"),
        r#"{
  "diagnostics": [
    {"data": {"language": "rust", "language_status": "stable"}},
    {"data": {"language": "typescript", "language_status": "preview", "canonical_gap_id": "gap:ts:second"}}
  ]
}
"#,
    );
    write(
        &expected.join("lsp-code-actions.json"),
        r#"{"actions": []}"#,
    );
    write(
        &expected.join("lsp-hover.md"),
        "## Evidence boundary\n\n## Gap state\n\n## Limits\n\nSuggested action: inspect\n\nStatic limit: missing import graph\n",
    );
    write(&expected.join("vscode-status.json"), r#"{"status":"warn"}"#);

    let mut violations = Vec::new();
    super::super::validate_editor_gap_cockpit_fixture_case(
        &root,
        "typescript_preview_static_limit",
        &mut violations,
    )?;
    let report = violations.join("\n");

    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("case must be typescript_preview_static_limit"));
    assert!(report.contains("must project exactly one diagnostic"));
    assert!(report.contains("diagnostic language must be typescript"));
    assert!(report.contains("diagnostic language_status must be preview"));
    assert!(report.contains("diagnostic must carry canonical_gap_id"));
    assert!(report.contains("is missing action `Inspect gap: copy static-limit note`"));
    assert!(report.contains("hover must show static limits before action language"));
    Ok(())
}

#[test]
fn editor_gap_cockpit_fixture_case_guard_requires_fail_closed_projection() -> Result<(), String> {
    let root = temp_dir("editor-gap-cockpit-disabled");
    write_editor_gap_case_expected(
        &root,
        "disabled_language",
        r#"{
  "diagnostics": [
    {"data": {"language": "python", "language_status": "preview", "canonical_gap_id": "gap:py"}}
  ]
}
"#,
        r#"{"actions": []}"#,
        "# Hover\n\n## Evidence boundary\n\n## Gap state\n\n## Limits\n",
    );

    let mut violations = Vec::new();
    super::super::validate_editor_gap_cockpit_fixture_case(
        &root,
        "disabled_language",
        &mut violations,
    )?;
    let report = violations.join("\n");

    assert!(report.contains("must not project diagnostics"));
    assert!(report.contains("must fail closed to refresh-only actions"));
    Ok(())
}

#[test]
fn editor_gap_cockpit_fixture_corpus_validator_accepts_complete_matrix() -> Result<(), String> {
    with_temp_cwd("editor-gap-cockpit-fixtures", |root| {
        write_editor_gap_cockpit_corpus(root, true);

        let mut violations = Vec::new();
        super::super::validate_editor_gap_cockpit_fixture_corpus(&mut violations)?;
        assert_eq!(violations, Vec::<String>::new());

        let fixture_dirs = super::super::lsp_cockpit_fixture_dirs()?;
        let fixture_names = fixture_dirs
            .iter()
            .map(|(fixture, _)| fixture.as_str())
            .collect::<BTreeSet<_>>();
        assert!(fixture_names.contains("editor_gap_cockpit/rust_actionable"));
        assert!(fixture_names.contains("editor_gap_cockpit/typescript_preview_static_limit"));
        assert!(fixture_names.contains("editor_gap_cockpit/python_preview_static_limit"));
        assert!(fixture_names.contains("editor_gap_cockpit/disabled_language"));
        assert!(fixture_names.contains("editor_gap_cockpit/wrong_root"));
        assert!(fixture_names.contains("editor_gap_cockpit/stale_artifact"));
        assert!(fixture_names.contains("editor_gap_cockpit/no_actionable_gap"));
        let (preview_name, preview_path) = fixture_dirs
            .iter()
            .find(|(fixture, _)| {
                fixture.as_str() == "editor_gap_cockpit/typescript_preview_static_limit"
            })
            .ok_or_else(|| "typescript preview fixture must exist".to_string())?;
        let preview = super::super::lsp_cockpit_fixture_report(preview_name, preview_path)
            .and_then(|report| {
                report.ok_or_else(|| "typescript preview fixture report must exist".to_string())
            })?;
        assert_eq!(preview.diagnostic_count, 1);
        assert!(preview.context.refresh_available);
        Ok(())
    })
}

#[test]
fn editor_gap_cockpit_fixture_corpus_validator_reports_missing_and_misordered_cases()
-> Result<(), String> {
    with_temp_cwd("editor-gap-cockpit-violations", |root| {
        let mut violations = Vec::new();
        super::super::validate_editor_gap_cockpit_fixture_corpus(&mut violations)?;
        assert!(violations.iter().any(|violation| {
            violation.contains("editor gap cockpit fixture corpus is missing")
        }));

        write_editor_gap_cockpit_corpus(root, false);
        let mut violations = Vec::new();
        super::super::validate_editor_gap_cockpit_fixture_corpus(&mut violations)?;
        assert!(violations.iter().any(|violation| {
                violation.contains(
                    "editor gap cockpit case typescript_preview_static_limit hover must show static limits before action language",
                )
            }));
        Ok(())
    })
}

#[test]
fn editor_lsp_workflow_fixture_pins_saved_workspace_loop() -> Result<(), String> {
    let report = with_repo_cwd(build_lsp_cockpit_report)?;
    let Some(editor_fixture) = report
        .fixtures
        .iter()
        .find(|fixture| fixture.fixture == "editor_lsp_workflow")
    else {
        return Err("expected editor_lsp_workflow LSP cockpit fixture".to_string());
    };

    assert_eq!(editor_fixture.diagnostic_count, 1);
    assert_eq!(editor_fixture.seam_diagnostic_count, 1);
    assert!(editor_fixture.context.seam_packet_available);
    assert!(editor_fixture.context.targeted_test_brief_available);
    assert!(editor_fixture.context.agent_repair_command_available);
    assert!(editor_fixture.context.agent_packet_command_available);
    assert!(editor_fixture.context.agent_brief_command_available);
    assert!(editor_fixture.context.after_snapshot_command_available);
    assert!(editor_fixture.context.agent_verify_command_available);
    assert!(editor_fixture.context.agent_receipt_command_available);
    assert!(editor_fixture.context.assertion_available);
    assert!(editor_fixture.context.related_test_available);
    assert!(editor_fixture.context.refresh_available);

    let expected = repo_root()?.join("fixtures/editor_lsp_workflow/expected");
    let hover = fs::read_to_string(expected.join("lsp-hover.md"))
        .map_err(|err| format!("failed to read editor hover fixture: {err}"))?;
    for needle in [
        "## Evidence Path",
        "## Missing discriminator",
        "## Suggested test shape",
        "## Handoff, verify, and receipt commands",
        "## Status projection",
        "## Limits",
    ] {
        if !hover.contains(needle) {
            return Err(format!("editor hover fixture missing `{needle}`"));
        }
    }

    let vscode_status = read_lsp_cockpit_json_value(&expected.join("vscode-status.json"))?;
    assert_eq!(vscode_status["schema_version"], "0.1");
    assert_eq!(vscode_status["fixture"], "editor_lsp_workflow");
    let states = vscode_status["states"]
        .as_array()
        .ok_or_else(|| "vscode-status fixture should include states".to_string())?;
    for state in [
        "valid_first_useful_action",
        "wrong_root_report",
        "stale_saved_workspace",
    ] {
        if !states.iter().any(|entry| entry["name"] == state) {
            return Err(format!("vscode-status fixture missing `{state}`"));
        }
    }

    let first_action_status =
        read_lsp_cockpit_json_value(&expected.join("first-useful-action-status.json"))?;
    assert_eq!(first_action_status["schema_version"], "0.1");
    assert_eq!(first_action_status["fixture"], "editor_lsp_workflow");
    assert_eq!(
        first_action_status["accepted"]["seam_id"],
        "67fc764ba37d77bd"
    );
    assert_eq!(
        first_action_status["stale_behavior"]["report_remains_visible"],
        true
    );
    Ok(())
}

#[test]
fn editors_vscode_license_carries_both_complete_canonical_license_texts() -> Result<(), String> {
    with_repo_cwd(|| {
        let root = super::super::repo_root()?;
        let notices = super::super::load_repository_license_notices(&root)?;
        super::super::verify_source_vscode_license(&root.join("editors/vscode/LICENSE"), &notices)
    })
}

#[test]
fn error_ripr_plus_receipt_timeout_has_indeterminate_status_and_null_unresolved()
-> Result<(), String> {
    let receipt = error_ripr_plus_receipt(
        "abc1234def5678abc1234def5678abc1234def5678",
        "cargo run --format repo-exposure-summary-json timed out after 1 ms while generating repo-exposure-summary-json for ripr-plus; no RIPR+ receipt is claimed.",
    );

    assert_eq!(
        receipt["status"], "indeterminate",
        "timeout error must produce indeterminate status"
    );
    assert_eq!(
        receipt["machine_readable_cause"], "evaluation_timeout",
        "timeout error string must map to evaluation_timeout"
    );
    assert!(
        receipt
            .get("unresolved")
            .is_some_and(serde_json::Value::is_null),
        "unresolved must be null on an indeterminate receipt"
    );
    let warnings = receipt
        .get("warnings")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    assert!(
        warnings > 0,
        "warnings must be non-empty on an error receipt"
    );
    assert!(
        receipt["head"].is_null(),
        "error receipt cannot qualify a candidate"
    );
    assert_eq!(
        receipt["observed_repository_head"],
        "abc1234def5678abc1234def5678abc1234def5678"
    );
    assert_eq!(receipt["candidate_binding"], "not_established");
    assert_eq!(receipt["zero_unresolved_established"], false);
    assert_eq!(receipt["schema_version"], "0.1");
    Ok(())
}

#[test]
fn extract_json_object_usize_map_orders_keys_lexicographically() -> Result<(), String> {
    let json = r#"{"counts": {"zeta": 1, "alpha": 2, "mu": 3}}"#;
    let map = extract_json_object_usize_map(json, "\"counts\":");
    let keys: Vec<String> = map.keys().cloned().collect();
    let expected = vec!["alpha".to_string(), "mu".to_string(), "zeta".to_string()];
    if keys == expected {
        Ok(())
    } else {
        Err(format!(
            "expected lexicographic order {expected:?}, got {keys:?}"
        ))
    }
}

#[test]
fn extract_json_object_usize_map_reads_flat_object() -> Result<(), String> {
    let json = r#"{"counts": {"a": 1, "b": 2, "c": 3}}"#;
    let map = extract_json_object_usize_map(json, "\"counts\":");
    let expected: std::collections::BTreeMap<String, usize> = [("a", 1), ("b", 2), ("c", 3)]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    if map == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {map:?}"))
    }
}

#[test]
fn extract_json_object_usize_map_returns_empty_for_missing_key() -> Result<(), String> {
    let json = r#"{"label": "ripr"}"#;
    let map = extract_json_object_usize_map(json, "\"counts\":");
    if map.is_empty() {
        Ok(())
    } else {
        Err(format!("expected empty map, got {map:?}"))
    }
}

#[test]
fn extract_json_object_usize_map_skips_non_numeric_values() -> Result<(), String> {
    let json = r#"{"counts": {"a": 1, "b": "two", "c": 3}}"#;
    let map = extract_json_object_usize_map(json, "\"counts\":");
    if map.contains_key("b") {
        return Err(format!(
            "non-numeric value 'two' should be skipped: {map:?}"
        ));
    }
    if map.get("a") != Some(&1) || map.get("c") != Some(&3) {
        return Err(format!("expected a=1 and c=3, got {map:?}"));
    }
    Ok(())
}

#[test]
fn extract_json_string_returns_none_for_missing_key() -> Result<(), String> {
    let json = r#"{"label": "ripr"}"#;
    match extract_json_string(json, "\"color\":") {
        None => Ok(()),
        other => Err(format!("expected None, got {other:?}")),
    }
}

#[test]
fn extract_json_string_returns_none_for_unterminated_value() -> Result<(), String> {
    let json = r#"{"label": "ripr"#;
    match extract_json_string(json, "\"label\":") {
        None => Ok(()),
        other => Err(format!(
            "expected None for unterminated value, got {other:?}"
        )),
    }
}

#[test]
fn extract_json_string_returns_value_for_present_key() -> Result<(), String> {
    let json = r#"{"label": "ripr", "color": "brightgreen"}"#;
    match extract_json_string(json, "\"color\":") {
        Some(value) if value == "brightgreen" => Ok(()),
        other => Err(format!("expected Some(\"brightgreen\"), got {other:?}")),
    }
}

#[test]
fn extract_json_warnings_returns_all_entries() -> Result<(), String> {
    let json = r#"{"warnings": ["first", "second", "third"]}"#;
    let warnings = extract_json_warnings(json);
    let expected = vec![
        "first".to_string(),
        "second".to_string(),
        "third".to_string(),
    ];
    if warnings == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {warnings:?}"))
    }
}

#[test]
fn extract_json_warnings_returns_empty_for_empty_array() -> Result<(), String> {
    let json = r#"{"warnings": []}"#;
    let warnings = extract_json_warnings(json);
    if warnings.is_empty() {
        Ok(())
    } else {
        Err(format!("expected empty Vec, got {warnings:?}"))
    }
}

#[test]
fn extract_json_warnings_returns_empty_for_missing_key() -> Result<(), String> {
    let json = r#"{"label": "ripr"}"#;
    let warnings = extract_json_warnings(json);
    if warnings.is_empty() {
        Ok(())
    } else {
        Err(format!("expected empty Vec, got {warnings:?}"))
    }
}

#[test]
fn extract_json_warnings_unescapes_backslash_pairs() -> Result<(), String> {
    // The substring extractor consumes the backslash-escape and keeps the
    // following character — so `\"` becomes a literal quote inside the
    // captured warning, and `\\` becomes a single backslash.
    let json = r#"{"warnings": ["with \"quote\"", "with \\path"]}"#;
    let warnings = extract_json_warnings(json);
    let expected = vec!["with \"quote\"".to_string(), "with \\path".to_string()];
    if warnings == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {warnings:?}"))
    }
}

#[test]
fn file_policy_allowlist_accepts_host_scoped_coverage() -> Result<(), String> {
    with_temp_cwd("file-policy-host-coverage", |root| {
        let path = root.join("allowlist.toml");
        write(
            &path,
            r#"[[allow]]
glob = "tools/example.py"
kind = "tool"
owner = "policy"
surface = "repo"
classification = "tooling"
reason = "Host-scoped tests supplement common controls."
covered_by = ["cargo test -p xtask common", "cargo xtask check-file-policy"]
covered_by_unix = ["cargo test -p xtask unix_control"]
covered_by_windows = ["cargo test -p xtask windows_control"]
"#,
        );
        let commands = crate::read_file_policy_test_commands(&path.to_string_lossy())?;
        assert_eq!(commands.len(), 3, "keep common and both platform selectors");
        assert_eq!(commands[0].host, None);
        assert_eq!(commands[1].host, Some(crate::FilePolicyHost::Unix));
        assert_eq!(commands[2].host, Some(crate::FilePolicyHost::Windows));
        assert!(commands[0].command.ends_with(" common"));
        assert!(commands[1].command.ends_with(" unix_control"));
        assert!(commands[2].command.ends_with(" windows_control"));
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_rejects_malformed_host_coverage() -> Result<(), String> {
    with_temp_cwd("file-policy-invalid-host", |root| {
        let path = root.join("allowlist.toml");
        let entry = concat!(
            "[[allow]]\nglob = \"tools/example.py\"\nkind = \"tool\"\n",
            "owner = \"policy\"\nsurface = \"repo\"\nclassification = \"tooling\"\n",
            "reason = \"Explicit applicability\"\ncovered_by = [\"cargo test common\"]\n",
        );
        for invalid in [
            "covered_by_linux = [\"cargo test native\"]",
            "covered_by_Unix = [\"cargo test native\"]",
            "covered_by_unix = \"cargo test native\"",
            "covered_by_unix = [1]",
            "covered_by_unix = []",
            "covered_by_windows = [\" \" ]",
            "covered_by_windows = [\"cargo xtask check-file-policy\"]",
            "covered_by_unix = [\"cargo test native\",,]",
            "covered_by_unix = [\"cargo test native\"]\ncovered_by_unix = []",
            "covered_by_unix = [\"cargo test native\"] garbage",
            "[allow.covered_by_unix]",
            "[allow.covered_by_unknown]",
        ] {
            write(&path, &format!("{entry}{invalid}\n"));
            if crate::read_file_policy_test_commands(&path.to_string_lossy()).is_ok() {
                return Err(format!("malformed applicability was accepted: {invalid}"));
            }
        }
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_toml_comments_preserve_coverage_commands() -> Result<(), String> {
    with_temp_cwd("file-policy-toml-comments", |root| {
        let path = root.join("allowlist.toml");
        let mut failures = Vec::new();
        for (field, host) in [
            ("covered_by", None),
            ("covered_by_unix", Some(crate::FilePolicyHost::Unix)),
            ("covered_by_windows", Some(crate::FilePolicyHost::Windows)),
        ] {
            let plain = r#"["cargo test -p xtask first", "cargo test -p xtask second"]"#;
            let baseline: toml::Value =
                toml::from_str(&file_policy_toml_coverage_fixture(field, plain))
                    .map_err(|error| error.to_string())?;
            for (case, array) in [
                ("plain", plain),
                (
                    "multiline",
                    "[\n  \"cargo test -p xtask first\",\n  \"cargo test -p xtask second\",\n]",
                ),
                (
                    "trailing",
                    "[\"cargo test -p xtask first\", \"cargo test -p xtask second\"] # host note",
                ),
                (
                    "inter_item",
                    "[\n  \"cargo test -p xtask first\",\n  # host note\n  \"cargo test -p xtask second\",\n]",
                ),
                (
                    "item_line",
                    "[\n  \"cargo test -p xtask first\", # first selector\n  \"cargo test -p xtask second\",\n]",
                ),
            ] {
                let source = file_policy_toml_coverage_fixture(field, array);
                let parsed: toml::Value =
                    toml::from_str(&source).map_err(|error| error.to_string())?;
                assert_eq!(
                    parsed, baseline,
                    "{field}/{case} must keep the same TOML value"
                );
                write(&path, &source);
                match crate::read_file_policy_test_commands(&path.to_string_lossy()) {
                    Ok(commands) => {
                        let actual = commands
                            .iter()
                            .map(|command| (command.line, command.command.as_str(), command.host))
                            .collect::<Vec<_>>();
                        let expected = [
                            (1, "cargo test -p xtask first", host),
                            (1, "cargo test -p xtask second", host),
                        ];
                        if actual != expected {
                            failures.push(format!("{field}/{case}: {actual:?}"));
                        }
                    }
                    Err(error) => failures.push(format!("{field}/{case}: {error}")),
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_toml_keeps_governed_field_refusals() -> Result<(), String> {
    with_temp_cwd("file-policy-toml-refusals", |root| {
        let path = root.join("allowlist.toml");
        let valid = file_policy_toml_coverage_fixture("covered_by", "[\"cargo test common\"]");
        let mut cases = Vec::new();
        for field in ["covered_by", "covered_by_unix", "covered_by_windows"] {
            for invalid in [
                "42",
                "[42]",
                "[\"cargo test selected\", 42]",
                "[\"cargo test selected\",,]",
                "{ selector = \"cargo test selected\" }",
            ] {
                cases.push((
                    format!("{field}/{invalid}"),
                    file_policy_toml_coverage_fixture(field, invalid),
                ));
            }
            let declaration = file_policy_toml_coverage_fixture(field, "[\"cargo test selected\"]");
            cases.push((
                format!("duplicate/{field}"),
                format!("{declaration}{field} = []\n"),
            ));
        }
        for invalid in [
            "covered_by_linux = [\"cargo test selected\"]",
            "covered_by_Unix = [\"cargo test selected\"]",
            "covered_by_unix = []",
            "covered_by_windows = []",
            "covered_by_unix = [\" \" ]",
            "covered_by_windows = [\"cargo xtask check-file-policy\"]",
            "mystery = \"unknown governed field\"",
            "[allow.covered_by_unix]",
            "[allow.covered_by_unknown]",
            "[allow.expires]",
            "[allow.retired]",
            "[unknown_table]",
            "[unknown.nested]",
            "[[unknown.nested]]",
        ] {
            cases.push((invalid.to_string(), format!("{valid}{invalid}\n")));
        }
        cases.extend([
            (
                "missing common".to_string(),
                valid.replace("covered_by = [\"cargo test common\"]\n", ""),
            ),
            (
                "root coverage".to_string(),
                format!("covered_by_windows = [\"cargo test selected\"]\n{valid}"),
            ),
            (
                "missing reason".to_string(),
                valid.replace("reason = \"TOML coverage control\"\n", ""),
            ),
            (
                "non-string glob".to_string(),
                valid.replace("glob = \"policy/*.toml\"", "glob = 42"),
            ),
        ]);
        let mut failures = Vec::new();
        for (case, source) in cases {
            write(&path, &source);
            if crate::read_file_policy_test_commands(&path.to_string_lossy()).is_ok() {
                failures.push(case);
            }
        }
        assert!(
            failures.is_empty(),
            "accepted invalid declarations: {}",
            failures.join("\n")
        );
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_toml_keeps_legacy_fields_and_common_empty_array() -> Result<(), String> {
    with_temp_cwd("file-policy-toml-legacy", |root| {
        let path = root.join("allowlist.toml");
        let source = format!(
            "{}generated_by = \"cargo xtask fixtures\"\nexpires = 42\nretired = \"legacy metadata\"\n",
            file_policy_toml_coverage_fixture("covered_by", "[]")
        );
        write(&path, &source);
        let entries = parse_file_policy_allowlist(&path.to_string_lossy())?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].line, 1);
        assert!(entries[0].covered_by.as_ref().is_some_and(Vec::is_empty));
        assert_eq!(
            entries[0].generated_by.as_deref(),
            Some("cargo xtask fixtures")
        );
        assert!(crate::read_file_policy_test_commands(&path.to_string_lossy())?.is_empty());
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_toml_parser_rejects_ungoverned_entries() -> Result<(), String> {
    with_temp_cwd("file-policy-allowlist-missing-field", |root| {
        let path = root.join("allowlist.toml");
        write(
            &path,
            r#"[[allow]]
glob = "scripts/*.sh"
kind = "script"
owner = "release"
surface = "ci"
classification = "tooling"
covered_by = ["cargo xtask check-file-policy"]
"#,
        );

        let path = path.to_string_lossy().to_string();
        let err = match parse_file_policy_allowlist(&path) {
            Ok(_) => return Err("expected missing reason to fail".to_string()),
            Err(err) => err,
        };
        assert!(err.contains("requires `reason`"));
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_toml_parser_requires_governed_fields() -> Result<(), String> {
    with_temp_cwd("file-policy-allowlist", |root| {
        let path = root.join("allowlist.toml");
        write(
            &path,
            r#"schema_version = "1.0"
policy = "non-rust-allowlist"
status = "active"

[[allow]]
glob = "editors/vscode/**/*.ts"
kind = "editor_extension"
owner = "lsp/editor"
surface = "editor"
classification = "production"
reason = "VS Code extensions are authored against the TypeScript VS Code API."
covered_by = ["npm run compile"]
"#,
        );

        let path = path.to_string_lossy().to_string();
        let entries = parse_file_policy_allowlist(&path)?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].glob.as_deref(), Some("editors/vscode/**/*.ts"));
        assert_eq!(entries[0].surface.as_deref(), Some("editor"));
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_toml_preserves_value_admission_for_ignored_metadata() -> Result<(), String>
{
    with_temp_cwd("file-policy-toml-value-admission", |root| {
        let path = root.join("allowlist.toml");
        let entry = file_policy_toml_coverage_fixture("covered_by", "[]");
        let mut failures = Vec::new();
        for location in [
            "expires",
            "retired",
            "root",
            "expires_inline",
            "retired_inline_array",
            "root_inline",
        ] {
            for (value, accepted) in [
                ("9223372036854775808", false),
                ("1e9999", false),
                ("42", true),
                ("9223372036854775807", true),
                ("-9223372036854775808", true),
                ("1e300", true),
            ] {
                let declaration = match location {
                    "expires" => format!("expires = {value}\n"),
                    "retired" => format!("retired = {value}\n"),
                    "root" => format!("metadata = {value}\n"),
                    "expires_inline" => format!("expires = {{ outer = {{ value = {value} }} }}\n"),
                    "retired_inline_array" => {
                        format!("retired = {{ outer = [{{ value = {value} }}] }}\n")
                    }
                    _ => format!("metadata = {{ outer = {{ value = {value} }} }}\n"),
                };
                let root_metadata = location.starts_with("root");
                let source = if root_metadata {
                    format!("{declaration}{entry}")
                } else {
                    format!("{entry}{declaration}")
                };
                // DeTable retains numeric lexemes. The previous Value reader
                // also enforced representable integers and finite exponents,
                // including inside otherwise ignored metadata.
                toml::de::DeTable::parse(&source).map_err(|error| {
                    format!("{location}/{value}: invalid DeTable fixture: {error}")
                })?;
                assert_eq!(
                    toml::from_str::<toml::Value>(&source).is_ok(),
                    accepted,
                    "{location}/{value}: bind this control to the existing Value authority"
                );
                write(&path, &source);
                match parse_file_policy_allowlist(&path.to_string_lossy()) {
                    Ok(entries)
                        if accepted
                            && entries.len() == 1
                            && entries[0].line == if root_metadata { 2 } else { 1 }
                            && entries[0].covered_by.as_ref().is_some_and(Vec::is_empty) => {}
                    Err(error)
                        if !accepted && error.contains("invalid non-Rust allowlist TOML") => {}
                    actual => failures.push(format!("{location}/{value}: {actual:?}")),
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_toml_spans_preserve_entry_order_and_lines() -> Result<(), String> {
    with_temp_cwd("file-policy-toml-spans", |root| {
        let path = root.join("allowlist.toml");
        let source = r#"# [[allow]] is not an entry
schema_version = "1.0"
note = "[[allow]] is still not an entry"

[[allow]]
glob = "policy/*.toml"
kind = "policy"
owner = "xtask"
surface = "policy"
classification = "config"
reason = "An example [[allow]] header is not a declaration"
covered_by_windows = ["""cargo test first
[[allow]]
"""]
covered_by_unix = ["cargo test unix_first"]
covered_by = ["cargo test common_first"]

# [[allow]] must not steal the following entry's attribution
[[allow]] # the actual second entry
glob = "docs/*.md"
kind = "docs"
owner = "xtask"
surface = "docs"
classification = "docs"
reason = "The second governed entry"
covered_by_windows = ["cargo test windows_second"] # keep this selector
covered_by_unix = ["cargo test unix_second"]
covered_by = ["cargo test common_second"]
"#;
        for (case, source, multiline_command) in [
            ("lf", source.to_string(), "cargo test first\n[[allow]]\n"),
            (
                "crlf",
                source.replace('\n', "\r\n"),
                "cargo test first\r\n[[allow]]\r\n",
            ),
        ] {
            // Preserve the selected TOML authority's newline bytes, rather
            // than assuming it normalizes CRLF inside multiline strings.
            let parsed: toml::Value = toml::from_str(&source).map_err(|error| error.to_string())?;
            assert_eq!(parsed["allow"].as_array().map(Vec::len), Some(2));
            assert_eq!(
                parsed["allow"][0]["covered_by_windows"][0].as_str(),
                Some(multiline_command),
                "{case}: the fixture's exact decoded selector is independently checked"
            );
            write(&path, &source);
            let entries = parse_file_policy_allowlist(&path.to_string_lossy())?;
            assert_eq!(
                entries.iter().map(|entry| entry.line).collect::<Vec<_>>(),
                [5, 19],
                "{case}"
            );
            let commands = crate::read_file_policy_test_commands(&path.to_string_lossy())?;
            let actual = commands
                .iter()
                .map(|command| (command.line, command.command.as_str(), command.host))
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                [
                    (5, "cargo test common_first", None),
                    (
                        5,
                        "cargo test unix_first",
                        Some(crate::FilePolicyHost::Unix)
                    ),
                    (5, multiline_command, Some(crate::FilePolicyHost::Windows)),
                    (19, "cargo test common_second", None),
                    (
                        19,
                        "cargo test unix_second",
                        Some(crate::FilePolicyHost::Unix)
                    ),
                    (
                        19,
                        "cargo test windows_second",
                        Some(crate::FilePolicyHost::Windows)
                    ),
                ],
                "{case}: command order and attribution must come from parsed entries"
            );
        }
        Ok(())
    })
}

#[test]
fn file_policy_allowlist_toml_strings_preserve_decoded_selectors() -> Result<(), String> {
    with_temp_cwd("file-policy-toml-strings", |root| {
        let path = root.join("allowlist.toml");
        let mut failures = Vec::new();
        for (field, host) in [
            ("covered_by", None),
            ("covered_by_unix", Some(crate::FilePolicyHost::Unix)),
            ("covered_by_windows", Some(crate::FilePolicyHost::Windows)),
        ] {
            for (case, value, expected) in [
                (
                    "hash",
                    r#""cargo test hash#selector""#,
                    "cargo test hash#selector",
                ),
                (
                    "quote",
                    r#""cargo test quote\"selector""#,
                    "cargo test quote\"selector",
                ),
                (
                    "backslash",
                    r#""cargo test path\\selector""#,
                    "cargo test path\\selector",
                ),
                (
                    "unicode",
                    r#""cargo test unicode\u005fselector""#,
                    "cargo test unicode_selector",
                ),
                (
                    "comma",
                    r#""cargo test comma,selector""#,
                    "cargo test comma,selector",
                ),
                (
                    "literal",
                    "'cargo test literal#selector'",
                    "cargo test literal#selector",
                ),
            ] {
                let source = file_policy_toml_coverage_fixture(field, &format!("[{value}]"));
                let parsed: toml::Value =
                    toml::from_str(&source).map_err(|error| error.to_string())?;
                assert_eq!(
                    parsed["allow"][0][field][0].as_str(),
                    Some(expected),
                    "{field}/{case} fixture must contain the expected decoded string"
                );
                write(&path, &source);
                match crate::read_file_policy_test_commands(&path.to_string_lossy()) {
                    Ok(commands)
                        if commands.len() == 1
                            && commands[0].line == 1
                            && commands[0].host == host
                            && commands[0].command == expected => {}
                    other => failures.push(format!("{field}/{case}: {other:?}")),
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        Ok(())
    })
}

#[test]
fn finding_alignment_dogfood_fixture_corpus_validator_reports_contract_drift() {
    with_temp_cwd("finding-alignment-dogfood-invalid", |_| {
        write(
            Path::new("fixtures/finding-alignment-dogfood/SPEC.md"),
            "# Finding Alignment Dogfood\n",
        );
        write(
            Path::new("fixtures/finding-alignment-dogfood/corpus.json"),
            r#"{
  "schema_version": "0.1",
  "kind": "finding_alignment_dogfood_corpus",
  "cases": [
    {
      "id": "presentation_text_actionable_output_observer",
      "source_pr": "EffortlessMetrics/ripr#966",
      "evidence_class": "presentation_text",
      "raw_findings_total": 2,
      "canonical_items_total": 1,
      "gap_state": "already_observed",
      "actionability": "already_observed",
      "user_outcome": "no_action",
      "repair_kind": "no_action",
      "target_test_type": "none",
      "verify_command": "none",
      "raw_findings_supporting_only": true,
      "recommended_repair": "No action.",
      "must_not_claim": ["Do not infer actionability from raw static class."],
      "reason": "duplicate id with wrong state"
    },
    {
      "id": "presentation_text_actionable_output_observer",
      "source_pr": "bad",
      "evidence_class": "unknown",
      "raw_findings_total": 0,
      "canonical_items_total": 0,
      "gap_state": "mystery",
      "actionability": "unknown",
      "user_outcome": "unknown",
      "repair_kind": "unknown",
      "target_test_type": "unknown",
      "verify_command": "unknown",
      "raw_findings_supporting_only": false,
      "recommended_repair": "",
      "must_not_claim": [],
      "reason": ""
    }
  ]
}"#,
        );

        let mut violations = Vec::new();
        let validation_result =
            super::super::validate_finding_alignment_dogfood_fixture_corpus(&mut violations);
        assert!(
            validation_result.is_ok(),
            "fixture corpus validation should not hard-fail: {validation_result:?}"
        );
        let report = violations.join("\n");

        assert!(report.contains("finding alignment dogfood case presentation_text_actionable_output_observer is duplicated"));
        assert!(report.contains("must have gap_state actionable, got mystery"));
        assert!(report.contains("finding alignment dogfood corpus is missing case presentation_text_already_observed_output"));
        assert!(report.contains("unsupported evidence class for dogfood receipt"));
        assert!(report.contains("canonical_items_total must be non-zero"));
        assert!(report.contains("unsupported gap_state `mystery`"));
    });
}

#[test]
fn finding_alignment_verify_command_missing_recognizes_empty_unknown_and_none() {
    assert!(finding_alignment_verify_command_is_missing(""));
    assert!(finding_alignment_verify_command_is_missing("   "));
    assert!(finding_alignment_verify_command_is_missing("unknown"));
    assert!(finding_alignment_verify_command_is_missing("none"));
    assert!(!finding_alignment_verify_command_is_missing(
        "cargo xtask evidence-quality-scorecard"
    ));
}

#[test]
fn first_line_difference_escapes_backticks_for_snapshot_diffs() {
    assert_eq!(
        first_line_difference("value with `tick`", "value with plain tick"),
        Some(
            "line 1 expected `value with \\`tick\\`` vs actual `value with plain tick`".to_string()
        )
    );
}

#[test]
fn first_line_difference_escapes_control_characters_for_snapshot_diffs() {
    assert_eq!(
        first_line_difference("ok\nline\tone", "ok\nline\rone"),
        Some("line 2 expected `line\\tone` vs actual `line\\rone`".to_string())
    );
}

#[test]
fn first_line_difference_reports_snapshot_context() {
    assert_eq!(
        first_line_difference("a\nb\nc", "a\nx\nc"),
        Some("line 2 expected `b` vs actual `x`".to_string())
    );
    assert_eq!(
        first_line_difference("a\nb", "a\nb\nc"),
        Some("line 3 expected `<missing>` vs actual `c`".to_string())
    );
    assert_eq!(first_line_difference("a\nb", "a\nb"), None);
}

#[test]
fn first_line_difference_truncates_very_long_snapshot_lines() {
    let expected = format!("ok\n{}", "a".repeat(130));
    let actual = format!("ok\n{}", "b".repeat(130));
    let diff = first_line_difference(&expected, &actual);
    assert!(matches!(
        diff.as_deref(),
        Some(message)
            if message.contains("expected `") && message.contains("…` vs actual `")
    ));
}

#[test]
fn first_successful_pr_corpus_is_valid() -> Result<(), String> {
    let corpus = first_successful_pr_corpus_path()?;
    let mut violations = Vec::new();
    super::super::validate_first_successful_pr_fixture_corpus_at(&corpus, &mut violations)?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn first_successful_pr_corpus_rejects_unsafe_demo_story_path() -> Result<(), String> {
    let root = temp_dir("first-successful-pr-unsafe-demo");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "kind": "first_successful_pr_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0051",
  "cases": [
    {
      "id": "boundary-gap",
      "description": "unsafe demo path",
      "input": "boundary-gap/inputs/reports/gap-decision-ledger.json",
      "expected": "boundary-gap/expected/start-here.json",
      "demo_story": "../README.md",
      "expected_status": "actionable",
      "expected_state": "top_gap"
    }
  ]
}"#,
    );
    let mut violations = Vec::new();
    super::super::validate_first_successful_pr_fixture_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("demo_story must be workspace-local"));
    Ok(())
}

#[test]
fn first_successful_pr_corpus_reports_contract_drift() -> Result<(), String> {
    let root = temp_dir("first-successful-pr-invalid");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "kind": "wrong",
  "schema_version": "0.2",
  "spec": "RIPR-SPEC-9999",
  "cases": [
    {
      "id": "boundary-gap",
      "description": "bad case",
      "expected_status": "blocked",
      "expected_state": "wrong_state"
    },
    {
      "id": "boundary-gap",
      "description": "duplicate case",
      "expected_status": "actionable",
      "expected_state": "top_gap"
    },
    {
      "id": "output-contract-gap",
      "description": "wrong status case",
      "expected_status": "blocked",
      "expected_state": "wrong_state"
    }
  ]
}"#,
    );
    let mut violations = Vec::new();
    super::super::validate_first_successful_pr_fixture_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("kind must be first_successful_pr_corpus"));
    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("spec must be RIPR-SPEC-0051"));
    assert!(report.contains("case boundary-gap is duplicated"));
    assert!(report.contains("case output-contract-gap must be actionable/top_gap"));
    assert!(report.contains("is missing case empty-diff"));
    assert!(report.contains("is missing case blocked-ledger"));
    assert!(report.contains("case boundary-gap is missing demo_story"));
    assert!(report.contains("inputs/reports/gap-decision-ledger.json"));
    assert!(report.contains("expected/start-here.json"));
    assert!(report.contains("expected/start-here.md"));
    Ok(())
}

#[test]
fn first_successful_pr_corpus_reports_outcome_receipt_drift() -> Result<(), String> {
    let root = temp_dir("first-successful-pr-outcome-drift");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "kind": "first_successful_pr_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0051",
  "cases": [
    {
      "id": "python-preview-gap",
      "description": "bad outcome receipt case",
      "expected_status": "actionable",
      "expected_state": "top_gap",
      "expected_output_state": "preview_limited",
      "outcome_receipts": [
        {
          "id": "bad",
          "before": "python-preview-gap/inputs/reports/missing-before.json",
          "after": "python-preview-gap/inputs/reports/missing-after.json",
          "expected": "python-preview-gap/expected/outcome/bad.json",
          "expected_markdown": "python-preview-gap/expected/outcome/bad.md",
          "expected_gap_movement": "closed"
        },
        {
          "id": "bad",
          "before": "python-preview-gap/inputs/reports/missing-before.json",
          "after": "python-preview-gap/inputs/reports/missing-after.json",
          "expected": "python-preview-gap/expected/outcome/bad.json",
          "expected_markdown": "python-preview-gap/expected/outcome/bad.md",
          "expected_gap_movement": "mystery"
        }
      ]
    }
  ]
}"#,
    );
    write(
        &root.join("python-preview-gap/expected/outcome/bad.json"),
        r#"{
  "schema_version": "0.1",
  "tool": "ripr",
  "status": "advisory",
  "inputs": {
    "before": "wrong-before.json",
    "after": "wrong-after.json"
  },
  "summary": {
    "gap_movement": {
      "closed": 0
    }
  }
}"#,
    );
    write(
        &root.join("python-preview-gap/expected/outcome/bad.md"),
        "# ripr targeted-test outcome report\n\nStatus: advisory\n\n## Gap Movement\n\n## Review Receipt\n",
    );

    let mut violations = Vec::new();
    super::super::validate_first_successful_pr_fixture_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("outcome receipt bad is duplicated"));
    assert!(report.contains("missing-before.json"));
    assert!(report.contains("unknown expected_gap_movement mystery"));
    assert!(report.contains("before input must be"));
    assert!(report.contains("must report expected gap movement closed"));
    assert!(report.contains("must include review_receipt"));
    assert!(report.contains("Markdown must show `| closed | 1 |`"));
    Ok(())
}

#[test]
fn first_successful_pr_corpus_reports_outcome_receipt_shape_drift() -> Result<(), String> {
    let root = temp_dir("first-successful-pr-outcome-shape-drift");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "kind": "first_successful_pr_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0051",
  "cases": [
    {
      "id": "python-preview-gap",
      "description": "bad outcome receipt case",
      "expected_status": "actionable",
      "expected_state": "top_gap",
      "expected_output_state": "preview_limited",
      "outcome_receipts": {
        "id": "not-an-array"
      }
    }
  ]
}"#,
    );

    let mut violations = Vec::new();
    super::super::validate_first_successful_pr_fixture_corpus_at(&corpus, &mut violations)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("outcome_receipts must be an array")),
        "expected malformed outcome_receipts violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn first_successful_pr_corpus_requires_demo_story_tokens() -> Result<(), String> {
    let root = temp_dir("first-successful-pr-demo-tokens");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "kind": "first_successful_pr_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0051",
  "cases": [
    {
      "id": "boundary-gap",
      "description": "demo token coverage",
      "input": "boundary-gap/inputs/reports/gap-decision-ledger.json",
      "expected": "boundary-gap/expected/start-here.json",
      "demo_story": "boundary-gap/README.md",
      "expected_status": "actionable",
      "expected_state": "top_gap"
    }
  ]
}"#,
    );
    write(
        &root.join("boundary-gap/inputs/reports/gap-decision-ledger.json"),
        r#"{"kind":"gap_decision_ledger"}"#,
    );
    write(
        &root.join("boundary-gap/expected/start-here.json"),
        r#"{
  "schema_version": "0.1",
  "kind": "first_pr_start_here",
  "status": "actionable",
  "posture": "advisory",
  "selected": {
    "state": "top_gap",
    "verify_command": "cargo xtask fixtures boundary_gap"
  }
}"#,
    );
    write(
        &root.join("boundary-gap/expected/start-here.md"),
        "# RIPR First PR Start Here\n\nStatus: advisory\n\n## Start Here\n\n- State: `top_gap`\n\n## Artifacts\n\n## Authority\n\n## Limits\n",
    );
    write(&root.join("boundary-gap/README.md"), "ripr first-pr\n");
    let mut violations = Vec::new();
    super::super::validate_first_successful_pr_fixture_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("is missing `ripr outcome`"));
    assert!(report.contains("is missing `focused external proof`"));
    assert!(report.contains("is missing `reviewer receipt`"));
    assert!(report.contains("is missing `fixture smoke path`"));
    assert!(report.contains("is missing `release demo`"));
    assert!(report.contains("is missing `agent training path`"));
    assert!(report.contains("is missing `No runtime mutation proof`"));
    assert!(report.contains("is missing `No merge approval`"));
    assert!(report.contains("actionable packet must name top actionable gap"));
    assert!(report.contains("actionable packet must name changed behavior"));
    assert!(report.contains("actionable packet must name why this matters"));
    assert!(report.contains("actionable packet must name static_evidence_boundary"));
    assert!(report.contains("actionable Markdown must include `- Top actionable gap:`"));
    assert!(report.contains("actionable Markdown must include `- Why this matters:`"));
    assert!(report.contains("actionable Markdown must include receipt command or path"));
    Ok(())
}

#[test]
fn first_successful_pr_outcome_input_path_is_stable_outside_repo_cwd() -> Result<(), String> {
    let corpus = first_successful_pr_corpus_path()?;
    let root = corpus
        .parent()
        .ok_or_else(|| "first successful PR corpus must have parent".to_string())?;

    with_temp_cwd("first-successful-pr-outcome-input-cwd", |_| {
        let input = super::super::first_successful_pr_outcome_input_path(
            root,
            "python-preview-gap/inputs/reports/after-check.json",
        );
        assert_eq!(
            input,
            "fixtures/first_successful_pr/python-preview-gap/inputs/reports/after-check.json"
        );
    });

    Ok(())
}

#[test]
fn first_successful_pr_outcome_input_path_uses_non_fixture_fallbacks() -> Result<(), String> {
    with_temp_cwd(
        "first-successful-pr-outcome-input-fallback-cwd",
        |root| -> Result<(), String> {
            let direct_root = root.join("direct-case");
            let direct = super::super::first_successful_pr_outcome_input_path(
                &direct_root,
                "inputs/reports/before.json",
            );
            assert_eq!(direct, "direct-case/inputs/reports/before.json");

            let nested = root.join("nested");
            fs::create_dir_all(&nested).map_err(|err| format!("create nested cwd: {err}"))?;
            std::env::set_current_dir(&nested).map_err(|err| format!("enter nested cwd: {err}"))?;
            let sibling_root = root.join("sibling-case");
            let sibling = super::super::first_successful_pr_outcome_input_path(
                &sibling_root,
                "inputs/reports/after.json",
            );
            assert_eq!(sibling, "sibling-case/inputs/reports/after.json");

            let outside_root = std::env::temp_dir().join("ripr-outcome-path-outside");
            let outside =
                super::super::first_successful_pr_outcome_input_path(&outside_root, "outside.json");
            assert!(outside.ends_with("ripr-outcome-path-outside/outside.json"));
            Ok(())
        },
    )?;
    Ok(())
}

#[test]
fn first_successful_pr_outcome_receipt_reports_field_and_render_drift() -> Result<(), String> {
    let root = temp_dir("first-successful-pr-outcome-render-drift");
    let mut violations = Vec::new();

    for receipt in [
        serde_json::json!({"id": "missing-before"}),
        serde_json::json!({
            "id": "missing-after",
            "before": "inputs/before.json"
        }),
        serde_json::json!({
            "id": "missing-expected",
            "before": "inputs/before.json",
            "after": "inputs/after.json"
        }),
        serde_json::json!({
            "id": "missing-markdown",
            "before": "inputs/before.json",
            "after": "inputs/after.json",
            "expected": "expected/outcome.json"
        }),
        serde_json::json!({
            "id": "missing-movement",
            "before": "inputs/before.json",
            "after": "inputs/after.json",
            "expected": "expected/outcome.json",
            "expected_markdown": "expected/outcome.md"
        }),
    ] {
        super::super::validate_first_successful_pr_outcome_receipt(
            &root,
            "python-preview-gap",
            &receipt,
            &mut violations,
        )?;
    }

    write(&root.join("inputs/before.json"), "{}");
    write(&root.join("inputs/after.json"), "{}");
    write(
        &root.join("expected/outcome.json"),
        r#"{
  "schema_version": "0.2",
  "tool": "other",
  "status": "complete",
  "inputs": {
    "before": "wrong-before.json",
    "after": "wrong-after.json"
  },
  "summary": {
    "gap_movement": {
      "closed": 1
    }
  },
  "review_receipt": {}
}"#,
    );
    write(
        &root.join("expected/outcome.md"),
        "not a targeted outcome report\n",
    );
    let rendered_drift = serde_json::json!({
        "id": "rendered-drift",
        "before": "inputs/before.json",
        "after": "inputs/after.json",
        "expected": "expected/outcome.json",
        "expected_markdown": "expected/outcome.md",
        "expected_gap_movement": "closed"
    });
    super::super::validate_first_successful_pr_outcome_receipt(
        &root,
        "python-preview-gap",
        &rendered_drift,
        &mut violations,
    )?;

    let report = violations.join("\n");
    for expected in [
        "is missing before",
        "is missing after",
        "is missing expected",
        "is missing expected_markdown",
        "is missing expected_gap_movement",
        "schema_version must be 0.1",
        "tool must be ripr",
        "status must be advisory",
        "before input must be",
        "after input must be",
        "Markdown is missing `# ripr targeted-test outcome report`",
        "Markdown must show `| closed | 1 |`",
    ] {
        assert!(
            report.contains(expected),
            "expected `{expected}` in outcome receipt drift report:\n{report}"
        );
    }
    Ok(())
}

#[test]
fn fixture_output_normalization_keeps_json_escapes_readable() {
    let json =
        r#"{"file":"fixtures/example/input\\src/lib.rs","oracle":"assert!(value == \"x\")"}"#;

    assert_eq!(
        normalize_fixture_json_output(json),
        r#"{"file":"fixtures/example/input/src/lib.rs","oracle":"assert!(value == \"x\")"}"#
    );
    assert_eq!(
        normalize_fixture_human_output("fixtures\\example\\input\\src/lib.rs"),
        "fixtures/example/input/src/lib.rs\n"
    );
}

/// #3054, the binding this gate rests on: the directory the fixture runner
/// clears must be the directory the `ripr` child actually writes its facts to.
///
/// `ripr` derives its default cache base from the workspace root it analyses, and
/// a fixture run is rooted at `<fixture>/input` — so before this fix the runner
/// cleared the repository cache while every fixture read and wrote its own
/// `input/target/ripr/cache`. Nothing observed the mismatch, which is why the
/// clear could be inert and still look correct.
///
/// This runs the real production path against a real fixture and asserts all
/// three legs: a seeded stale entry is discarded, facts appear under the cleared
/// directory, and nothing lands inside the fixture workspace.
#[test]
fn fixture_run_writes_its_facts_into_the_cache_the_runner_cleared() -> Result<(), String> {
    fn file_count(dir: &Path) -> Result<usize, String> {
        let mut total = 0;
        let entries = fs::read_dir(dir).map_err(|err| format!("read {}: {err}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|err| format!("read entry in {}: {err}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                total += file_count(&path)?;
            } else {
                total += 1;
            }
        }
        Ok(total)
    }

    with_repo_cwd(|| {
        let owned = fixture_cache::Case::new("binding")?;
        let fixture = owned.fixture.clone();
        let workspace_cache = fixture
            .join("input")
            .join("target")
            .join("ripr")
            .join("cache");
        ignore_remove_dir_all(fixture.join("input").join("target"));

        let cache_dir = &owned.cache;
        let stale = cache_dir
            .join("repo-file-facts")
            .join("0.2")
            .join("stale-3054.json");
        let stale_parent = stale
            .parent()
            .ok_or_else(|| "seeded cache entry should have a parent".to_string())?;
        fs::create_dir_all(stale_parent)
            .map_err(|err| format!("create {}: {err}", stale_parent.display()))?;
        fs::write(&stale, b"{\"stale\":true}")
            .map_err(|err| format!("write {}: {err}", stale.display()))?;

        super::super::run_fixture_outputs(&fixture)?;

        assert!(
            !stale.exists(),
            "a fixture run must discard cached facts computed by a previous binary: {}",
            stale.display()
        );
        assert!(
            cache_dir.is_dir(),
            "the fixture run must write its facts under the cleared cache: {}",
            cache_dir.display()
        );
        assert!(
            file_count(cache_dir)? > 0,
            "the cleared cache must be the cache the child populates, but {} is empty",
            cache_dir.display()
        );
        assert!(
            !workspace_cache.exists(),
            "no fixture cache may land inside the fixture workspace: {}",
            workspace_cache.display()
        );
        Ok(())
    })
}

#[test]
fn forbids_active_line_catches_active_forbidden_content() {
    let lines = active_yaml_lines("settings: |");
    assert!(forbids_active_line(&lines, "settings:"));
}

#[test]
fn forbids_active_line_ignores_commented_forbidden_content() {
    let lines = active_yaml_lines("# settings: |");
    assert!(!forbids_active_line(&lines, "settings:"));
}

#[test]
fn front_panel_case_inputs_follow_the_crate_corpus_root_rule() {
    let (root, inputs) = front_panel_case_inputs(&serde_json::json!({
        "inputs": { "first_action": "a.json", "gate_decision": null, "ledger": "b.json" }
    }));
    assert_eq!(root, "fixtures/boundary_gap/input");
    let flags = inputs
        .iter()
        .map(|input| (input.flag.as_str(), input.path.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(flags, [("first-action", "a.json"), ("ledger", "b.json")]);

    let (root, _) = front_panel_case_inputs(&serde_json::json!({
        "inputs": { "ledger": "b.json" }
    }));
    assert_eq!(root, ".");

    let (root, inputs) = front_panel_case_inputs(&serde_json::json!({
        "inputs": { "root": "repo", "assistant_health": "h.json" }
    }));
    assert_eq!(root, "repo");
    assert_eq!(inputs.len(), 1, "`root` is a label, not an input flag");
}

#[test]
fn gap_decision_ledger_corpus_is_valid() -> Result<(), String> {
    let corpus = gap_decision_ledger_corpus_path()?;
    let mut violations = Vec::new();
    super::super::validate_gap_decision_ledger_fixture_corpus_at(&corpus, &mut violations)?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn gap_decision_ledger_gate_candidate_requires_safe_predicate() -> Result<(), String> {
    let mut corpus = gap_decision_ledger_corpus_value()?;
    let case = gap_decision_ledger_case_mut(
        &mut corpus,
        "repairable_boundary_gap_pr_comment_gate_candidate",
    )?;
    let record = case
        .get_mut("expected_gap_record")
        .ok_or_else(|| "case must have expected_gap_record".to_string())?;
    record["language_status"] = serde_json::json!("preview");
    record["safe_gate_predicate"]["policy_target_enabled"] = serde_json::json!(false);
    record["safe_gate_predicate"]["static_unknown_only"] = serde_json::json!(true);

    let report = gap_decision_ledger_violations(&corpus).join("\n");

    assert!(report.contains(
        "PR-comment case repairable_boundary_gap_pr_comment_gate_candidate must be stable Rust"
    ));
    assert!(report.contains(
        "gate candidate repairable_boundary_gap_pr_comment_gate_candidate must be stable Rust"
    ));
    assert!(report.contains("must set safe_gate_predicate.policy_target_enabled=true"));
    assert!(report.contains("must set safe_gate_predicate.static_unknown_only=false"));
    Ok(())
}

#[test]
fn gap_decision_ledger_is_manifest_only_fixture_dir() -> Result<(), String> {
    let violations =
        super::super::fixture_contract_violations(Path::new("fixtures/gap-decision-ledger"))?;
    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn gap_decision_ledger_missing_artifact_requires_regeneration() -> Result<(), String> {
    let mut corpus = gap_decision_ledger_corpus_value()?;
    let case = gap_decision_ledger_case_mut(&mut corpus, "missing_artifact_warning_regeneration")?;
    let record = case
        .get_mut("expected_gap_record")
        .ok_or_else(|| "case must have expected_gap_record".to_string())?;
    record["regeneration_commands"] = serde_json::json!([]);
    record["projection_eligibility"]["gate_candidate"]["eligible"] = serde_json::json!(true);

    let report = gap_decision_ledger_violations(&corpus).join("\n");

    assert!(report.contains("regeneration_commands must be a non-empty string array"));
    assert!(report.contains(
            "missing-artifact case missing_artifact_warning_regeneration must not be PR-comment or gate eligible"
        ));
    Ok(())
}

#[test]
fn gap_decision_ledger_missing_output_contract_requires_output_route() -> Result<(), String> {
    let mut corpus = gap_decision_ledger_corpus_value()?;
    let case = gap_decision_ledger_case_mut(
        &mut corpus,
        "missing_output_contract_routes_to_output_golden",
    )?;
    let record = case
        .get_mut("expected_gap_record")
        .ok_or_else(|| "case must have expected_gap_record".to_string())?;
    record["evidence_class"] = serde_json::json!("return_value");
    record["repair_route"]["route_kind"] = serde_json::json!("AddBoundaryAssertion");

    let report = gap_decision_ledger_violations(&corpus).join("\n");

    assert!(report.contains(
            "MissingOutputContract case missing_output_contract_routes_to_output_golden must use presentation_text evidence"
        ));
    assert!(report.contains(
            "MissingOutputContract case missing_output_contract_routes_to_output_golden must route to output/golden repair"
        ));
    Ok(())
}

#[test]
fn gap_decision_ledger_reports_contract_drift() {
    let corpus = serde_json::json!({
        "kind": "wrong",
        "schema_version": "0.2",
        "spec": "RIPR-SPEC-9999",
        "proposal": "RIPR-PROP-9999",
        "required_gap_kinds": ["MissingBoundaryAssertion"],
        "required_scopes": ["pr_local"],
        "required_policy_states": ["new"],
        "required_repairability": ["repairable"],
        "cases": [
            {
                "id": "bad",
                "description": "bad case",
                "source_artifacts": [],
                "expected_claim": "claim",
                "must_not_claim": [],
                "expected_gap_record": {
                    "gap_id": "gap:bad",
                    "canonical_gap_id": "gap:bad",
                    "kind": "surprise",
                    "language": "rust",
                    "language_status": "stable",
                    "scope": "global",
                    "evidence_class": "unknown",
                    "gap_state": "actionable",
                    "policy_state": "mystery",
                    "repairability": "maybe",
                    "evidence_ids": [],
                    "verification_commands": [],
                    "projection_eligibility": {
                        "ci_summary": {"eligible": "yes", "reason": null}
                    }
                }
            },
            {
                "id": "bad",
                "description": "duplicate case",
                "source_artifacts": ["artifact"],
                "expected_claim": "claim",
                "must_not_claim": ["guard"],
                "expected_gap_record": {
                    "gap_id": "gap:dup",
                    "canonical_gap_id": "gap:dup",
                    "kind": "MissingBoundaryAssertion",
                    "language": "rust",
                    "language_status": "stable",
                    "scope": "pr_local",
                    "evidence_class": "predicate_boundary",
                    "gap_state": "actionable",
                    "policy_state": "new",
                    "repairability": "repairable",
                    "evidence_ids": ["evidence"],
                    "verification_commands": ["cargo xtask check-pr"],
                    "projection_eligibility": {}
                }
            }
        ]
    });

    let report = gap_decision_ledger_violations(&corpus).join("\n");

    assert!(report.contains("kind must be gap_decision_ledger_corpus"));
    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("spec must be RIPR-SPEC-0046"));
    assert!(report.contains("proposal must be RIPR-PROP-0006"));
    assert!(report.contains("required_gap_kinds is missing MissingErrorDiscriminator"));
    assert!(report.contains("required_scopes is missing repo_scoped"));
    assert!(report.contains("required_policy_states is missing baseline_known"));
    assert!(report.contains("required_repairability is missing needs_human_design"));
    assert!(report.contains("case bad is duplicated"));
    assert!(report.contains("source_artifacts must be a non-empty string array"));
    assert!(report.contains("must_not_claim must be a non-empty string array"));
    assert!(report.contains("has unsupported kind surprise"));
    assert!(report.contains("has unsupported scope global"));
    assert!(report.contains("has unsupported policy_state mystery"));
    assert!(report.contains("has unsupported repairability maybe"));
    assert!(report.contains("evidence_ids must be a non-empty string array"));
    assert!(report.contains("verification_commands must be a non-empty string array"));
    assert!(report.contains("projection ci_summary is missing eligible boolean"));
    assert!(report.contains("projection ci_summary is missing reason"));
    assert!(report.contains("is missing projection_eligibility.report_packet"));
    assert!(report.contains("repairable case bad is missing repair_route object"));
    assert!(report.contains("must include a safe gate-candidate case"));
    assert!(report.contains("must include a MissingOutputContract case"));
}
