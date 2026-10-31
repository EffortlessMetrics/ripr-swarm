//! Evidence-promotion semantic-assertion tests, moved verbatim from
//! `tests.rs` (#2948, slice C of #2119). Test names are unchanged apart
//! from the module path; `super::` references to the xtask crate root
//! became `crate::`.

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_class_projection() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::ExpectedClass {
        class: "weakly_exposed".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:class-human-drift",
                "classification": "weakly_exposed"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Static exposure
  exposed (info, confidence 0.60)
";

    let report = crate::evidence_promotion_semantic_violations(
        "class_human_drift",
        Some("fixtures/typescript_class_human_drift"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("expected_class"), "{report}");
    assert!(
        report.contains("project class `weakly_exposed`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_human_class_golden() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::ExpectedClass {
        class: "weakly_exposed".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:class-missing-human",
                "classification": "weakly_exposed"
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "class_missing_human",
        Some("fixtures/typescript_class_missing_human"),
        &assertions,
        &check_json,
        None,
        true,
    )
    .join("\n");

    assert!(report.contains("expected_class"), "{report}");
    assert!(report.contains("expected/human-full.txt"), "{report}");
}

#[test]
fn evidence_promotion_human_class_line_matches_exact_class_token() {
    assert!(crate::evidence_promotion_human_class_line_matches(
        "  weakly_exposed (warning, confidence 0.40)",
        "weakly_exposed"
    ));
    assert!(crate::evidence_promotion_human_class_line_matches(
        "  exposed (info, confidence 0.60)",
        "exposed"
    ));
    assert!(!crate::evidence_promotion_human_class_line_matches(
        "  weakly_exposed (warning, confidence 0.40)",
        "exposed"
    ));
}

#[test]
fn evidence_promotion_semantic_assertions_reject_expected_oracle_without_findings() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "exact_value".to_string(),
        strength: "strong".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 0},
        "findings": []
    });

    let report = crate::evidence_promotion_semantic_violations(
        "oracle_empty",
        Some("fixtures/typescript_oracle_empty"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_oracle"), "{report}");
    assert!(report.contains("requires at least one finding"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_receipt_command() {
    let assertions = vec![
        crate::EvidencePromotionSemanticAssertion::MustHaveReceiptCommand,
        crate::EvidencePromotionSemanticAssertion::MustEmitRepairPacket,
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-missing-receipt",
                "classification": "reachable_unrevealed",
                "repair_packet_ready": true,
                "verify_command": "cargo test packet_missing_receipt"
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "packet_missing_receipt",
        Some("fixtures/packet_missing_receipt"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("must_have_receipt_command"), "{report}");
    assert!(
        report.contains("requires a non-empty receipt_command"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_repair_packet_detail() {
    let assertions =
        vec![crate::EvidencePromotionSemanticAssertion::MustDiscloseRepairPacketDetail];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-missing-detail",
                "classification": "weakly_exposed",
                "repair_packet_ready": true,
                "typescript_repair_packet": {
                    "allowed_edit_surface": ["tests/discount.test.ts"],
                    "assertion_shape": "expect(result).toBe(50)",
                    "authority_boundary": "preview_advisory_only",
                    "canonical_gap_id": "gap:typescript:discount",
                    "file": "src/discount.ts",
                    "forbidden_files": ["src/discount.ts"],
                    "gap_id": "probe:discount",
                    "language": "typescript",
                    "language_status": "preview",
                    "line": 2,
                    "must_not_change": ["Do not edit production code."],
                    "receipt_command": "ripr outcome --before baseline --after repair",
                    "repair_kind": "AddBoundaryAssertion",
                    "verify_command": "jest tests/discount.test.ts"
                }
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "packet_missing_detail",
        Some("fixtures/packet_missing_detail"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(
        report.contains("must_disclose_repair_packet_detail"),
        "{report}"
    );
    assert!(
        report.contains("target_test:missing target test"),
        "{report}"
    );
    assert!(
        report.contains("$.raw_evidence_refs:missing raw evidence refs"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_wrong_repair_packet_detail() {
    let assertions = vec![
        crate::EvidencePromotionSemanticAssertion::ExpectedRepairPacketDetail {
            detail: crate::ExpectedRepairPacketDetail {
                canonical_gap_id: "gap:typescript:discount".to_string(),
                source_file: "src/discount.ts".to_string(),
                source_line: 2,
                target_test: "tests/discount.test.ts::discount boundary".to_string(),
                assertion_shape: "expect(result).toBe(50)".to_string(),
                authority_boundary: "preview_advisory_only".to_string(),
                repair_kind: "AddBoundaryAssertion".to_string(),
                verify_command: "jest tests/discount.test.ts".to_string(),
                receipt_command: "ripr outcome --before baseline --after repair".to_string(),
                allowed_edit_surface: vec!["tests/discount.test.ts".to_string()],
                forbidden_files: vec!["src/discount.ts".to_string()],
            },
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-wrong-detail",
                "classification": "weakly_exposed",
                "repair_packet_ready": true,
                "typescript_repair_packet": {
                    "allowed_edit_surface": ["tests/discount.test.ts"],
                    "assertion_shape": "expect(result).toBe(50)",
                    "authority_boundary": "preview_advisory_only",
                    "canonical_gap_id": "gap:typescript:discount",
                    "file": "src/discount.ts",
                    "forbidden_files": ["src/discount.ts"],
                    "gap_id": "probe:discount",
                    "language": "typescript",
                    "language_status": "preview",
                    "line": 2,
                    "must_not_change": ["Do not edit production code."],
                    "receipt_command": "ripr outcome --before baseline --after repair",
                    "repair_kind": "AddBoundaryAssertion",
                    "target_test": "tests/wrong.test.ts::discount boundary",
                    "verify_command": "jest tests/wrong.test.ts"
                }
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "packet_wrong_detail",
        Some("fixtures/packet_wrong_detail"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_repair_packet_detail"), "{report}");
    assert!(
            report.contains(
                "target_test:expected `tests/discount.test.ts::discount boundary` got `tests/wrong.test.ts::discount boundary`"
            ),
            "{report}"
        );
    assert!(
        report.contains(
            "verify_command:expected `jest tests/discount.test.ts` got `jest tests/wrong.test.ts`"
        ),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_contradictory_packet_messaging() {
    let assertions =
        vec![crate::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-contradictory-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": true,
                    "gap_state": "actionable",
                    "actionability_category": "complete_repair_packet"
                },
                "typescript_repair_packet": {
                    "canonical_gap_id": "gap:typescript:discount",
                    "verify_command": "jest tests/discount.test.ts",
                    "receipt_command": "ripr outcome --before baseline --after repair"
                },
                "evidence": [
                    "owner: applyDiscount",
                    "gap_state: advisory",
                    "actionability_category: incomplete_repair_packet",
                    "why_not_actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract",
                    "repair_route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available",
                    "missing_actionability_fields: canonical_gap_id",
                    "evidence_needed_to_promote: canonical gap identity"
                ]
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "packet_contradictory_messaging",
        Some("fixtures/packet_contradictory_messaging"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(
        report.contains("must_not_have_contradictory_packet_messaging"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence[1]:blocked gap_state evidence"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence[3]:blocked why-not-actionable evidence"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_probe_family_selector_scopes_findings_and_refuses_empty_match() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MaximumClass {
        class: "weakly_exposed".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 2},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:fa1d51d0",
                "classification": "weakly_exposed",
                "probe": {"id": "probe:src_lib.rs:predicate:fa1d51d0", "family": "predicate"}
            },
            {
                "id": "probe:src_lib.rs:return_value:c71d52af",
                "classification": "exposed",
                "probe": {"id": "probe:src_lib.rs:return_value:c71d52af", "family": "return_value"}
            }
        ]
    });

    let scoped = crate::evidence_promotion_semantic_violations_scoped(
        "scoped_predicate_control",
        Some("fixtures/split_test_boundary_oracle"),
        &assertions,
        &check_json,
        None,
        false,
        Some("predicate"),
    );
    assert!(scoped.is_empty(), "{scoped:?}");

    let promoted_family = crate::evidence_promotion_semantic_violations_scoped(
        "scoped_wrong_family",
        Some("fixtures/split_test_boundary_oracle"),
        &assertions,
        &check_json,
        None,
        false,
        Some("return_value"),
    );
    assert_eq!(promoted_family.len(), 1, "{promoted_family:?}");
    assert!(
        promoted_family[0].contains("probe:src_lib.rs:return_value:c71d52af"),
        "{promoted_family:?}"
    );

    let unscoped = crate::evidence_promotion_semantic_violations(
        "unscoped_fixture_wide",
        Some("fixtures/split_test_boundary_oracle"),
        &assertions,
        &check_json,
        None,
        false,
    );
    assert_eq!(unscoped.len(), 1, "{unscoped:?}");

    let empty_match = crate::evidence_promotion_semantic_violations_scoped(
        "scoped_empty_match",
        Some("fixtures/split_test_boundary_oracle"),
        &assertions,
        &check_json,
        None,
        false,
        Some("call_deletion"),
    );
    assert_eq!(empty_match.len(), 1, "{empty_match:?}");
    assert!(
        empty_match[0].contains("matched no findings"),
        "{empty_match:?}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_contradictory_packet_messaging() {
    let assertions =
        vec![crate::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-human-contradictory-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": true,
                    "gap_state": "actionable",
                    "actionability_category": "complete_repair_packet"
                },
                "typescript_repair_packet": {
                    "canonical_gap_id": "gap:typescript:discount",
                    "verify_command": "jest tests/discount.test.ts",
                    "receipt_command": "ripr outcome --before baseline --after repair"
                },
                "evidence": [
                    "owner: applyDiscount",
                    "gap_state: actionable",
                    "actionability_category: complete_repair_packet"
                ]
            }
        ]
    });
    let human_text = "\
TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  status: not actionable
  why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
  missing fields: verify_command, receipt_command
";

    let report = crate::evidence_promotion_semantic_violations(
        "packet_human_contradictory_messaging",
        Some("fixtures/packet_human_contradictory_messaging"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(
        report.contains("must_not_have_contradictory_packet_messaging"),
        "{report}"
    );
    assert!(
        report.contains(
            "expected/human-full.txt:status: not actionable:blocked not-actionable status"
        ),
        "{report}"
    );
    assert!(
            report.contains("expected/human-full.txt:missing fields: verify_command, receipt_command:blocked missing-fields line"),
            "{report}"
        );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_human_complete_packet_messaging() {
    let assertions =
        vec![crate::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-human-complete-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": true,
                    "gap_state": "actionable",
                    "actionability_category": "complete_repair_packet"
                },
                "typescript_repair_packet": {
                    "canonical_gap_id": "gap:typescript:discount",
                    "verify_command": "jest tests/discount.test.ts",
                    "receipt_command": "ripr outcome --before baseline --after repair"
                },
                "evidence": [
                    "owner: applyDiscount",
                    "gap_state: actionable",
                    "actionability_category: complete_repair_packet",
                    "why_actionable: complete repair packet"
                ]
            }
        ]
    });
    let human_text = "\
TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  source: applyDiscount at src/discount.ts:2
  related test: tests/discount.test.ts::discount
  oracle: expect(result).toBe(50)
  edit surface: tests/discount.test.ts
  verify: jest tests/discount.test.ts
  receipt: ripr outcome --before baseline --after repair
  why actionable: complete repair packet
  authority: preview_advisory_only
";

    let report = crate::evidence_promotion_semantic_violations(
        "packet_human_complete_messaging",
        Some("fixtures/packet_human_complete_messaging"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(report.is_empty(), "{report:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_accept_human_mixed_packet_and_blocked_messaging() {
    let assertions =
        vec![crate::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging];
    let check_json = serde_json::json!({
        "summary": {"findings": 2},
        "findings": [
            {
                "id": "packet-human-complete-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": true,
                    "gap_state": "actionable",
                    "actionability_category": "complete_repair_packet"
                },
                "typescript_repair_packet": {
                    "canonical_gap_id": "gap:typescript:discount",
                    "verify_command": "jest tests/discount.test.ts",
                    "receipt_command": "ripr outcome --before baseline --after repair"
                },
                "evidence": [
                    "owner: applyDiscount",
                    "gap_state: actionable",
                    "actionability_category: complete_repair_packet",
                    "why_actionable: complete repair packet"
                ]
            },
            {
                "id": "packet-human-blocked-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": false,
                    "gap_state": "advisory",
                    "actionability_category": "incomplete_repair_packet"
                },
                "evidence": [
                    "owner: shippingTotal",
                    "gap_state: advisory",
                    "actionability_category: incomplete_repair_packet",
                    "why_not_actionable: TypeScript preview lacks a complete repair packet contract"
                ]
            }
        ]
    });
    let human_text = "\
TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  source: applyDiscount at src/discount.ts:2
  related test: tests/discount.test.ts::discount
  oracle: expect(result).toBe(50)
  edit surface: tests/discount.test.ts
  verify: jest tests/discount.test.ts
  receipt: ripr outcome --before baseline --after repair
  why actionable: complete repair packet
  authority: preview_advisory_only

TypeScript repair packet (advisory)
  canonical gap: gap:typescript:shipping
  status: not actionable
  why not actionable: TypeScript preview lacks a complete repair packet contract
  missing fields: verify_command, receipt_command
";

    let report = crate::evidence_promotion_semantic_violations(
        "packet_human_mixed_messaging",
        Some("fixtures/packet_human_mixed_messaging"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(report.is_empty(), "{report:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_repair_packet_detail() {
    let assertions =
        vec![crate::EvidencePromotionSemanticAssertion::MustDiscloseRepairPacketDetail];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-human-missing-detail",
                "classification": "weakly_exposed",
                "repair_packet_ready": true,
                "typescript_repair_packet": {
                    "allowed_edit_surface": ["tests/discount.test.ts"],
                    "assertion_shape": "expect(result).toBe(50)",
                    "authority_boundary": "preview_advisory_only",
                    "canonical_gap_id": "gap:typescript:discount",
                    "file": "src/discount.ts",
                    "forbidden_files": ["src/discount.ts"],
                    "gap_id": "probe:discount",
                    "language": "typescript",
                    "language_status": "preview",
                    "line": 2,
                    "must_not_change": ["Do not edit production code."],
                    "receipt_command": "ripr outcome --before baseline --after repair",
                    "repair_kind": "AddBoundaryAssertion",
                    "target_test": "tests/discount.test.ts::discount",
                    "verify_command": "jest tests/discount.test.ts"
                },
                "preview_actionability": {
                    "raw_evidence_refs": [
                        {
                            "file": "src/discount.ts",
                            "kind": "typescript_preview_probe",
                            "line": 2,
                            "source_id": "probe:discount"
                        }
                    ]
                }
            }
        ]
    });
    let human_text = "\
TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  source: applyDiscount at src/discount.ts:2
  related test: tests/discount.test.ts::discount
  oracle: expect(result).toBe(50)
  edit surface: tests/discount.test.ts
  verify: jest tests/discount.test.ts
  must not change:
    - Do not edit production code.
  authority: preview_advisory_only
";

    let report = crate::evidence_promotion_semantic_violations(
        "packet_human_missing_detail",
        Some("fixtures/packet_human_missing_detail"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(
        report.contains("must_disclose_repair_packet_detail"),
        "{report}"
    );
    assert!(
        report.contains("expected/human-full.txt:missing receipt command `receipt:`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_wrong_repair_packet_detail() {
    let assertions = vec![
        crate::EvidencePromotionSemanticAssertion::ExpectedRepairPacketDetail {
            detail: crate::ExpectedRepairPacketDetail {
                canonical_gap_id: "gap:typescript:discount".to_string(),
                source_file: "src/discount.ts".to_string(),
                source_line: 2,
                target_test: "tests/discount.test.ts::discount boundary".to_string(),
                assertion_shape: "expect(result).toBe(50)".to_string(),
                authority_boundary: "preview_advisory_only".to_string(),
                repair_kind: "AddBoundaryAssertion".to_string(),
                verify_command: "jest tests/discount.test.ts".to_string(),
                receipt_command: "ripr outcome --before baseline --after repair".to_string(),
                allowed_edit_surface: vec!["tests/discount.test.ts".to_string()],
                forbidden_files: vec!["src/discount.ts".to_string()],
            },
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-human-wrong-detail",
                "classification": "weakly_exposed",
                "repair_packet_ready": true,
                "typescript_repair_packet": {
                    "allowed_edit_surface": ["tests/discount.test.ts"],
                    "assertion_shape": "expect(result).toBe(50)",
                    "authority_boundary": "preview_advisory_only",
                    "canonical_gap_id": "gap:typescript:discount",
                    "file": "src/discount.ts",
                    "forbidden_files": ["src/discount.ts"],
                    "gap_id": "probe:discount",
                    "language": "typescript",
                    "language_status": "preview",
                    "line": 2,
                    "must_not_change": ["Do not edit production code."],
                    "receipt_command": "ripr outcome --before baseline --after repair",
                    "repair_kind": "AddBoundaryAssertion",
                    "target_test": "tests/discount.test.ts::discount boundary",
                    "verify_command": "jest tests/discount.test.ts"
                }
            }
        ]
    });
    let human_text = "\
Preview actionability
  related test: tests/discount.test.ts::discount boundary
  verify: jest tests/discount.test.ts
  authority: preview_advisory_only

TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  source: applyDiscount at src/discount.ts:2
  related test: tests/wrong.test.ts::discount boundary
  oracle: expect(result).toBe(50)
  edit surface: tests/discount.test.ts
  verify: jest tests/wrong.test.ts
  receipt: ripr outcome --before baseline --after repair
  must not change:
    - Do not edit production code.
  authority: preview_advisory_only
";

    let report = crate::evidence_promotion_semantic_violations(
        "packet_human_wrong_detail",
        Some("fixtures/packet_human_wrong_detail"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("expected_repair_packet_detail"), "{report}");
    assert!(
            report.contains(
                "expected/human-full.txt:missing target test `tests/discount.test.ts::discount boundary`"
            ),
            "{report}"
        );
    assert!(
        report.contains(
            "expected/human-full.txt:missing verify command `jest tests/discount.test.ts`"
        ),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_no_tests_claim_with_witness() {
    let assertions = vec![
        crate::EvidencePromotionSemanticAssertion::MustDiscloseWitness,
        crate::EvidencePromotionSemanticAssertion::MustNotClaimNoTestsFound,
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "ripr": {
                    "infect": {
                        "summary": "No tests were found, so activation/infection cannot be estimated"
                    }
                },
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here.",
                    "No tests were found, so activation/infection cannot be estimated"
                ]
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "witnessed_no_tests_claim",
        Some("fixtures/witnessed_no_tests_claim"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("must_not_claim_no_tests_found"), "{report}");
    assert!(report.contains("$.findings[0].evidence[1]"), "{report}");
    assert!(
        report.contains("$.findings[0].ripr.infect.summary"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_witness_projection() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });
    let human_text = "Static limitation\nNo statically reachable test path was found\n";

    let report = crate::evidence_promotion_semantic_violations(
        "witnessed_human_missing_projection",
        Some("fixtures/witnessed_human_missing_projection"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_disclose_witness"), "{report}");
    assert!(
        report.contains("expected/human-full.txt:missing Where to look"),
        "{report}"
    );
    assert!(
        report.contains(
            "expected/human-full.txt:missing witness `For example, the test `integration_path`"
        ),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_mismatched_witness_projection() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });
    let human_text = concat!(
        "Where to look\n",
        "  For example, the test `stale_path` (tests/it.rs:9) calls `outer`, an entry point that may lead here.\n",
    );

    let report = crate::evidence_promotion_semantic_violations(
        "witnessed_human_mismatched_projection",
        Some("fixtures/witnessed_human_mismatched_projection"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_disclose_witness"), "{report}");
    assert!(
        report.contains(
            "expected/human-full.txt:missing witness `For example, the test `integration_path`"
        ),
        "{report}"
    );
    assert!(!report.contains("missing Where to look"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_human_witness_golden() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "witnessed_missing_human_golden",
        Some("fixtures/witnessed_missing_human_golden"),
        &assertions,
        &check_json,
        None,
        true,
    )
    .join("\n");

    assert!(report.contains("must_disclose_witness"), "{report}");
    assert!(
        report.contains("requires fixture human output at `expected/human-full.txt`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_no_tests_claim_with_witness() {
    let assertions = vec![
        crate::EvidencePromotionSemanticAssertion::MustDiscloseWitness,
        crate::EvidencePromotionSemanticAssertion::MustNotClaimNoTestsFound,
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "ripr": {
                    "infect": {
                        "summary": "No statically reachable test path was found, so activation/infection cannot be estimated"
                    }
                },
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });
    let human_text = concat!(
        "Where to look\n",
        "  For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here.\n",
        "No tests were found, so activation/infection cannot be estimated\n",
    );

    let report = crate::evidence_promotion_semantic_violations(
        "witnessed_human_no_tests_claim",
        Some("fixtures/witnessed_human_no_tests_claim"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_not_claim_no_tests_found"), "{report}");
    assert!(report.contains("expected/human-full.txt:3"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_limitation_detail() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "missing_limitation_detail",
        Some("fixtures/missing_limitation_detail"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(
        report.contains("must_disclose_limitation_detail"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence:missing last established edge"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence:missing first unresolved edge"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence:missing analyzer route"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence:missing non-claim"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_limitation_detail() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "limitation_last_established_edge: test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "limitation_first_unresolved_edge: entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ],
                "static_limitation": {
                    "kind": "rust_transitive_reach_unresolved",
                    "last_established_edge": "test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "first_unresolved_edge": "entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "analyzer_route": "analysis/rust-public-api-transitive-reach",
                    "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                }
            }
        ]
    });
    let human_text = "Static limitation\n  rust_transitive_reach_unresolved\n";

    let report = crate::evidence_promotion_semantic_violations(
        "human_missing_limitation_detail",
        Some("fixtures/human_missing_limitation_detail"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(
        report.contains("must_disclose_limitation_detail"),
        "{report}"
    );
    assert!(
        report.contains("expected/human-full.txt:missing Limitation detail"),
        "{report}"
    );
    assert!(
        report.contains("expected/human-full.txt:missing detail `last established edge:"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_limitation_detail_projection() {
    let assertions = vec![
            crate::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail,
            crate::EvidencePromotionSemanticAssertion::ExpectedLimitationDetail {
                last_established_edge:
                    "test `integration_path` (tests/it.rs:4) -> entry `outer`".to_string(),
                first_unresolved_edge:
                    "entry `outer` -> owner `inner` through a transitive Rust helper path"
                        .to_string(),
                non_claim:
                    "named limitation only; ripr cannot confirm or deny that this path observes the change"
                        .to_string(),
            },
            crate::EvidencePromotionSemanticAssertion::ExpectedLimitationRoute {
                route: "analysis/rust-public-api-transitive-reach".to_string(),
            },
        ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "limitation_last_established_edge: test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "limitation_first_unresolved_edge: entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ],
                "static_limitation": {
                    "kind": "rust_transitive_reach_unresolved",
                    "last_established_edge": "test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "first_unresolved_edge": "entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "analyzer_route": "analysis/rust-public-api-transitive-reach",
                    "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                }
            }
        ]
    });
    let human_text = concat!(
        "Limitation detail\n",
        "  last established edge: test `integration_path` (tests/it.rs:4) -> entry `outer`\n",
        "  first unresolved edge: entry `outer` -> owner `inner` through a transitive Rust helper path\n",
        "  analyzer route: analysis/rust-public-api-transitive-reach\n",
        "  non-claim: named limitation only; ripr cannot confirm or deny that this path observes the change\n",
    );

    let violations = crate::evidence_promotion_semantic_violations(
        "limitation_detail_projection",
        Some("fixtures/limitation_detail_projection"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_wrong_limitation_detail() {
    let assertions = vec![
            crate::EvidencePromotionSemanticAssertion::ExpectedLimitationDetail {
                last_established_edge:
                    "test `integration_path` (tests/it.rs:4) -> entry `outer`".to_string(),
                first_unresolved_edge:
                    "entry `outer` -> owner `inner` through a transitive Rust helper path"
                        .to_string(),
                non_claim:
                    "named limitation only; ripr cannot confirm or deny that this path observes the change"
                        .to_string(),
            },
        ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "limitation_last_established_edge: test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "limitation_first_unresolved_edge: entry `other` -> owner `inner` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ]
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "wrong_limitation_detail",
        Some("fixtures/wrong_limitation_detail"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_limitation_detail"), "{report}");
    assert!(
        report.contains(
            "$.findings[0].evidence:first unresolved edge:entry `other` -> owner `inner`"
        ),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_wrong_limitation_route() {
    let assertions = vec![
        crate::EvidencePromotionSemanticAssertion::ExpectedLimitationRoute {
            route: "analysis/rust-public-api-transitive-reach".to_string(),
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "limitation_last_established_edge: test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "limitation_first_unresolved_edge: entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/generic-static-limitation",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ]
            }
        ]
    });

    let report = crate::evidence_promotion_semantic_violations(
        "wrong_limitation_route",
        Some("fixtures/wrong_limitation_route"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_limitation_route"), "{report}");
    assert!(
        report.contains("$.findings[0].evidence:analysis/generic-static-limitation"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_scope_limited_empty_results() {
    let no_scope = serde_json::json!({
        "summary": {"findings": 0},
        "findings": [],
        "scope_disclosures": [
            {"scope_status": "no_scope_provided", "category": "no_scope_disclosure"}
        ]
    });
    let no_scope_assertions = vec![
        crate::EvidencePromotionSemanticAssertion::MustNotReportClean,
        crate::EvidencePromotionSemanticAssertion::MustDiscloseNoScope,
    ];
    let no_scope_violations = crate::evidence_promotion_semantic_violations(
        "no_scope",
        Some("fixtures/scope_honesty_no_scope_empty"),
        &no_scope_assertions,
        &no_scope,
        None,
        false,
    );
    assert!(
        no_scope_violations.is_empty(),
        "no-scope disclosure should make the empty result non-clean: {no_scope_violations:?}"
    );

    let dirty_worktree = serde_json::json!({
        "summary": {"findings": 0},
        "findings": [],
        "unanalyzed_working_tree": true
    });
    let dirty_assertions = vec![
        crate::EvidencePromotionSemanticAssertion::MustNotReportClean,
        crate::EvidencePromotionSemanticAssertion::MustDiscloseUnanalyzedWorkingTree,
    ];
    let dirty_violations = crate::evidence_promotion_semantic_violations(
        "dirty_worktree",
        Some("fixtures/scope_honesty_unanalyzed_worktree_empty"),
        &dirty_assertions,
        &dirty_worktree,
        None,
        false,
    );
    assert!(
        dirty_violations.is_empty(),
        "unanalyzed_working_tree should make the empty result non-clean: {dirty_violations:?}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_false_unanalyzed_worktree_disclosure() {
    let assertions = vec![
        crate::EvidencePromotionSemanticAssertion::MustDiscloseScope,
        crate::EvidencePromotionSemanticAssertion::MustNotDiscloseUnanalyzedWorkingTree,
    ];
    let scoped_worktree = serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "mode": "draft",
        "root": ".",
        "base": "HEAD",
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:4e6b5f28",
                "classification": "no_static_path",
                "probe": {"file": "src/lib.rs"}
            }
        ]
    });

    let scoped_violations = crate::evidence_promotion_semantic_violations(
        "scoped_worktree",
        Some("fixtures/scope_worktree_analyzed_dirty"),
        &assertions,
        &scoped_worktree,
        None,
        false,
    );
    assert!(
        scoped_violations.is_empty(),
        "scoped worktree report without unanalyzed disclosure should pass: {scoped_violations:?}"
    );

    let false_exclusion = serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "mode": "draft",
        "root": ".",
        "base": "HEAD",
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:4e6b5f28",
                "classification": "no_static_path",
                "probe": {"file": "src/lib.rs"}
            }
        ],
        "unanalyzed_working_tree": true
    });

    let report = crate::evidence_promotion_semantic_violations(
        "false_exclusion",
        Some("fixtures/scope_worktree_analyzed_dirty"),
        &assertions,
        &false_exclusion,
        None,
        false,
    )
    .join("\n");

    assert!(
        report.contains("must_not_disclose_unanalyzed_working_tree"),
        "{report}"
    );
    assert!(
        report.contains("working-tree draft was in scope"),
        "{report}"
    );
}
