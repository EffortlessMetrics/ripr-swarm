use super::*;

#[test]
fn evidence_promotion_honesty_rejects_vacuous_non_promotion_charter() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-non-vacuity");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py-empty");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    write_evidence_promotion_check_json(
        &py_fixture,
        serde_json::json!({
            "summary": {"findings": 0},
            "findings": []
        }),
    )?;
    for fixture in [&ts_fixture, &rust_fixture] {
        write_evidence_promotion_check(fixture, "weakly_exposed")?;
    }
    for fixture in [&rust_control_fixture, &ts_control_fixture] {
        write_evidence_promotion_check(fixture, "exposed")?;
    }

    let guarded_non_promotion = |id: &str, language: &str, source_fixture: &Path| {
        serde_json::json!({
            "id": id,
            "language": language,
            "tier": "pure",
            "source_fixture": source_fixture,
            "assertions": [
                {"type": "must_not_report_clean"},
                {"type": "must_not_promote"}
            ]
        })
    };
    let promoted_control = |id: &str, language: &str, source_fixture: &Path| {
        serde_json::json!({
            "id": id,
            "language": language,
            "tier": "pure",
            "source_fixture": source_fixture,
            "assertions": [{"type": "must_promote"}]
        })
    };
    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_vacuous_non_promotion",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "assertions": [{"type": "must_not_promote"}]
            },
            guarded_non_promotion("ts_guarded", "typescript", &ts_fixture),
            guarded_non_promotion("rust_guarded", "rust", &rust_fixture),
            promoted_control("rust_control", "rust", &rust_control_fixture),
            promoted_control("ts_control", "typescript", &ts_control_fixture)
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");
    assert!(
        report.contains("py_vacuous_non_promotion")
            && report.contains("`must_not_promote` requires `must_not_report_clean`")
            && report.contains("cannot pass vacuously"),
        "expected the manifest-level non-vacuity violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_missing_unknown_and_impure_tiers() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-tier-contract");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_missing_tier",
                "language": "python",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_unknown_tier",
                "language": "typescript",
                "tier": "external-ish",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_pure_with_external_claim",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_fixture,
                "external_repo": "https://github.com/dtolnay/semver",
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(
        report.contains("py_missing_tier") && report.contains("`tier` is required"),
        "expected missing tier violation, got {violations:?}"
    );
    assert!(
        report.contains("ts_unknown_tier") && report.contains("unknown tier `external-ish`"),
        "expected unknown tier violation, got {violations:?}"
    );
    assert!(
        report.contains("rust_pure_with_external_claim")
            && report.contains("tier `pure` must not")
            && report.contains("external_repo"),
        "expected pure-tier external metadata violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_incomplete_pinned_external_tier() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-pinned-external-tier");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_external_incomplete",
                "language": "rust",
                "tier": "pinned_external",
                "source_fixture": rust_fixture,
                "external_commit": "main",
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(
        report.contains("rust_external_incomplete")
            && report.contains("tier `pinned_external`")
            && report.contains("external_repo")
            && report.contains("external_command")
            && report.contains("external_commit")
            && report.contains("external_patch")
            && report.contains("runtime_budget_seconds")
            && report.contains("artifact_budget_bytes"),
        "expected pinned-external metadata violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_accepts_complete_pinned_external_tier() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-pinned-external-complete");
    let corpus = root.join("corpus.json");
    let patch_path = root.join("patches/semver-boundary.diff");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    write(&patch_path, "diff --git a/src/lib.rs b/src/lib.rs\n");
    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_non_promoted",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_external_complete",
                "language": "rust",
                "tier": "pinned_external",
                "source_fixture": rust_fixture,
                "external_repo": "https://github.com/dtolnay/semver",
                "external_command": "ripr check --root {checkout} --diff {external_patch} --mode fast --json",
                "external_commit": "0123456789abcdef0123456789abcdef01234567",
                "external_patch": patch_path,
                "runtime_budget_seconds": 120,
                "artifact_budget_bytes": 10485760,
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_accepts_typed_assertion_vocabulary() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-typed-assertions");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_report = root.join("reports/rust-rich.json");
    let packet_report = root.join("reports/rust-packet.json");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    write_evidence_promotion_check(&py_fixture, "weakly_exposed")?;
    write_evidence_promotion_check(&ts_fixture, "weakly_exposed")?;
    write_evidence_promotion_check(&rust_control_fixture, "exposed")?;
    write_evidence_promotion_check(&ts_control_fixture, "exposed")?;
    write(
            &rust_report,
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": "0.2",
                "tool": "ripr",
                "mode": "fast",
                "root": ".",
                "base": "HEAD",
                "analysis_scope": {"completeness": "complete"},
                "summary": {"findings": 1},
                "findings": [
                    {
                        "id": "probe:src_lib.rs:predicate:typed",
                        "classification": "no_static_path",
                        "oracle_kind": "unknown",
                        "oracle_strength": "unknown",
                        "probe": {"file": "src/lib.rs"},
                        "static_limit_kind": "rust_transitive_reach_unresolved",
                        "static_limitation": {
                            "kind": "rust_transitive_reach_unresolved",
                            "last_established_edge": "test `typed_case` (tests/typed.rs:1) -> entry `entry`",
                            "first_unresolved_edge": "entry `entry` -> owner `changed` through a transitive Rust helper path",
                            "analyzer_route": "analysis/rust-public-api-transitive-reach",
                            "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                        },
                        "verify_command": "cargo test typed_case",
                        "evidence": [
                            "For example, the test `typed_case` (tests/typed.rs:1) calls `entry`, an entry point that may lead here.",
                            "limitation_last_established_edge: test `typed_case` (tests/typed.rs:1) -> entry `entry`",
                            "limitation_first_unresolved_edge: entry `entry` -> owner `changed` through a transitive Rust helper path",
                            "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                            "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                        ]
                    }
                ]
            }))
            .map_err(|err| err.to_string())?,
        );
    write(
        &packet_report,
        &serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "0.2",
            "tool": "ripr",
            "mode": "fast",
            "root": ".",
            "base": "HEAD",
            "analysis_scope": {"completeness": "complete"},
            "summary": {"findings": 1},
            "findings": [
                {
                    "id": "gap:typed-packet",
                    "classification": "reachable_unrevealed",
                    "repair_packet_ready": true,
                    "verify_command": "cargo test typed_packet",
                    "receipt_command": "ripr receipt write typed-packet",
                    "repair_packet": {
                        "allowed_edit_surface": ["tests/typed_packet.rs"],
                        "assertion_shape": "assert_eq!(actual, expected)",
                        "authority_boundary": "test",
                        "canonical_gap_id": "gap:typed-packet",
                        "file": "src/lib.rs",
                        "forbidden_files": ["src/lib.rs"],
                        "gap_id": "gap:typed-packet",
                        "language": "rust",
                        "language_status": "stable",
                        "line": 12,
                        "must_not_change": ["Do not edit production code."],
                        "receipt_command": "ripr receipt write typed-packet",
                        "repair_kind": "AddBoundaryAssertion",
                        "target_test": "tests/typed_packet.rs::typed_packet",
                        "verify_command": "cargo test typed_packet"
                    },
                    "preview_actionability": {
                        "raw_evidence_refs": [
                            {
                                "file": "src/lib.rs",
                                "kind": "rust_probe",
                                "line": 12,
                                "source_id": "gap:typed-packet"
                            }
                        ]
                    }
                }
            ]
        }))
        .map_err(|err| err.to_string())?,
    );

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "assertions": [
                    {"type": "must_not_report_clean"},
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "weakly_exposed"}
                ]
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "assertions": [
                    {"type": "must_not_report_clean"},
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "weakly_exposed"}
                ]
            },
            {
                "id": "rust_typed_vocabulary",
                "language": "rust",
                "tier": "pure",
                "source_report": rust_report,
                "assertions": [
                    {"type": "must_not_report_clean"},
                    {"type": "must_disclose_scope"},
                    {"type": "must_see_changed_file", "path": "src/lib.rs"},
                    {
                        "type": "must_emit_limitation",
                        "expected_limit_kind": "rust_transitive_reach_unresolved"
                    },
                    {"type": "must_have_verify_command"},
                    {"type": "must_not_emit_repair_packet"},
                    {"type": "must_disclose_witness"},
                    {"type": "must_disclose_limitation_detail"},
                    {
                        "type": "expected_limitation_detail",
                        "last_established_edge": "test `typed_case` (tests/typed.rs:1) -> entry `entry`",
                        "first_unresolved_edge": "entry `entry` -> owner `changed` through a transitive Rust helper path",
                        "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                    },
                    {
                        "type": "expected_limitation_route",
                        "route": "analysis/rust-public-api-transitive-reach"
                    },
                    {"type": "must_not_claim_no_tests_found"},
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "no_static_path"},
                    {"type": "expected_class", "class": "no_static_path"},
                    {"type": "expected_oracle", "kind": "unknown", "strength": "unknown"},
                    {"type": "expected_completeness", "completeness": "complete"}
                ]
            },
            {
                "id": "rust_packet_commands",
                "language": "rust",
                "tier": "pure",
                "source_report": packet_report,
                "assertions": [
                    {"type": "must_have_verify_command"},
                    {"type": "must_have_receipt_command"},
                    {"type": "must_emit_repair_packet"},
                    {"type": "must_disclose_repair_packet_detail"},
                    {
                        "type": "expected_repair_packet_detail",
                        "canonical_gap_id": "gap:typed-packet",
                        "source_file": "src/lib.rs",
                        "source_line": 12,
                        "target_test": "tests/typed_packet.rs::typed_packet",
                        "assertion_shape": "assert_eq!(actual, expected)",
                        "authority_boundary": "test",
                        "repair_kind": "AddBoundaryAssertion",
                        "verify_command": "cargo test typed_packet",
                        "receipt_command": "ripr receipt write typed-packet",
                        "allowed_edit_surface": ["tests/typed_packet.rs"],
                        "forbidden_files": ["src/lib.rs"]
                    },
                    {"type": "must_not_have_contradictory_packet_messaging"}
                ]
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "assertions": [{"type": "must_promote"}]
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "assertions": [{"type": "must_promote"}]
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    assert!(
        violations.is_empty(),
        "unexpected typed assertion violations: {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_duplicate_keys_in_case_object() -> Result<(), String> {
    // issue #2277: a hand-spliced case object with duplicate keys parses under
    // serde_json's last-wins rule and silently drops the earlier case's pin —
    // the gate must fail closed instead of blessing the loss.
    let root = temp_dir("evidence-promotion-honesty-duplicate-keys");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "cases": [
    {
      "id": "ts_hoc_wrapped_owner",
      "language": "typescript",
      "tier": "pure",
      "source_fixture": "fixtures/typescript_adversarial_hoc_wrapped_owner",
      "vector": "higher_order_wrapper_obscures_owner_identity",
      "id": "ts_mocked_owner_module_unrelated_assertion",
      "language": "typescript",
      "tier": "pure",
      "source_fixture": "fixtures/typescript_adversarial_owner_module_mock",
      "vector": "mocked_owner_module_strong_oracle_observes_unrelated_sink",
      "assertions": [{ "type": "must_not_promote" }]
    }
  ]
}
"#,
    );

    let mut violations = Vec::new();
    let result =
        super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations);

    let err = match result {
        Err(err) => err,
        Ok(()) => {
            return Err(format!(
                "duplicate-key corpus must fail closed, got Ok with violations {violations:?}"
            ));
        }
    };
    assert!(
        err.contains("duplicate key `id`"),
        "expected duplicate-key parse failure, got {err}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_unknown_assertion_type() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-unknown-assertion");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "assertions": [
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "weakly_exposed"}
                ]
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "assertions": [
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "weakly_exposed"}
                ]
            },
            {
                "id": "rust_bad_assertion",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_fixture,
                "assertions": [
                    {"type": "must_not_promote"},
                    {"type": "must_guess"}
                ]
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "assertions": [{"type": "must_promote"}]
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "assertions": [{"type": "must_promote"}]
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");
    assert!(report.contains("rust_bad_assertion"), "{report}");
    assert!(
        report.contains("unknown assertion type `must_guess`"),
        "{report}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_semantic_assertions_reject_projection_drift() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotHaveVerifyCommand,
        super::super::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand,
        super::super::EvidencePromotionSemanticAssertion::MustEmitRepairPacket,
        super::super::EvidencePromotionSemanticAssertion::MustNotEmitLimitation,
        super::super::EvidencePromotionSemanticAssertion::ExpectedCompleteness {
            completeness: "limited".to_string(),
        },
        super::super::EvidencePromotionSemanticAssertion::ExpectedChangedRustFiles { count: 2 },
    ];
    let check_json = serde_json::json!({
        "analysis_scope": {"completeness": "complete"},
        "summary": {"changed_rust_files": 1, "findings": 1},
        "findings": [
            {
                "id": "projection-drift",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "verify_command": "cargo test",
                "receipt_command": "ripr receipt write projection-drift"
            }
        ]
    });

    let report = super::super::evidence_promotion_semantic_violations(
        "projection_drift",
        Some("fixtures/projection_drift"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("must_not_have_verify_command"), "{report}");
    assert!(report.contains("must_not_have_receipt_command"), "{report}");
    assert!(report.contains("must_emit_repair_packet"), "{report}");
    assert!(report.contains("must_not_emit_limitation"), "{report}");
    assert!(report.contains("expected_completeness"), "{report}");
    assert!(report.contains("expected_changed_rust_files"), "{report}");
    assert!(
        report.contains("summary.changed_rust_files `2`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_expected_changed_rust_files() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedChangedRustFiles { count: 0 },
    ];
    let check_json = serde_json::json!({
        "summary": {"changed_rust_files": 0, "findings": 0},
        "findings": []
    });

    let violations = super::super::evidence_promotion_semantic_violations(
        "changed_rust_files",
        Some("fixtures/changed_rust_files"),
        &assertions,
        &check_json,
        None,
        false,
    );
    assert!(
        violations.is_empty(),
        "expected changed Rust file count should pass: {violations:?}"
    );

    let missing_count = serde_json::json!({
        "summary": {"findings": 0},
        "findings": []
    });
    let report = super::super::evidence_promotion_semantic_violations(
        "missing_changed_rust_files",
        Some("fixtures/changed_rust_files"),
        &assertions,
        &missing_count,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_changed_rust_files"), "{report}");
    assert!(report.contains("<missing>"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_pin_zero_findings() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotPromote,
        super::super::EvidencePromotionSemanticAssertion::ExpectedFindingCount { count: 0 },
    ];
    let clean = serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "mode": "fast",
        "root": "fixtures/no_behavior/input",
        "base": "origin/main",
        "summary": {"findings": 0},
        "findings": []
    });
    let violations = super::super::evidence_promotion_semantic_violations(
        "zero_findings",
        Some("fixtures/no_behavior"),
        &assertions,
        &clean,
        None,
        false,
    );
    assert!(
        violations.is_empty(),
        "an exact zero-finding no-behavior case should be non-vacuous: {violations:?}"
    );

    let regressed = serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "mode": "fast",
        "root": "fixtures/no_behavior/input",
        "base": "origin/main",
        "summary": {"findings": 1},
        "findings": [{"id": "false-exposed", "classification": "exposed"}]
    });
    let report = super::super::evidence_promotion_semantic_violations(
        "zero_findings",
        Some("fixtures/no_behavior"),
        &assertions,
        &regressed,
        None,
        false,
    )
    .join("\n");
    assert!(report.contains("expected_finding_count"), "{report}");
    assert!(report.contains("promoted to exposed"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_verify_command_projection() {
    let assertions = vec![super::super::EvidencePromotionSemanticAssertion::MustHaveVerifyCommand];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "verify-human-missing",
                "classification": "weakly_exposed",
                "verify_command": "jest tests/discount.test.ts"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

TypeScript repair packet (advisory)
  status: packet-ready
";

    let report = super::super::evidence_promotion_semantic_violations(
        "verify_human_missing",
        Some("fixtures/verify_human_missing"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_have_verify_command"), "{report}");
    assert!(
        report.contains("missing verify_command `jest tests/discount.test.ts`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_invented_verify_command() {
    let assertions =
        vec![super::super::EvidencePromotionSemanticAssertion::MustNotHaveVerifyCommand];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "verify-human-invented",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Static limitation
  rust_transitive_reach_unresolved
  verify: cargo test transitive_path
";

    let report = super::super::evidence_promotion_semantic_violations(
        "verify_human_invented",
        Some("fixtures/verify_human_invented"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_not_have_verify_command"), "{report}");
    assert!(report.contains("cargo test transitive_path"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_invented_receipt_command() {
    let assertions =
        vec![super::super::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "receipt-human-invented",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Static limitation
  rust_transitive_reach_unresolved
  receipt: ripr receipt write receipt-human-invented
";

    let report = super::super::evidence_promotion_semantic_violations(
        "receipt_human_invented",
        Some("fixtures/receipt_human_invented"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_not_have_receipt_command"), "{report}");
    assert!(
        report.contains("ripr receipt write receipt-human-invented"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_unavailable_human_receipt_status() {
    let assertions =
        vec![super::super::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "receipt-human-unavailable",
                "classification": "weakly_exposed"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Python repair card (preview/advisory)
  receipt: unavailable_until_python_gap_ledger
";

    let report = super::super::evidence_promotion_semantic_violations(
        "receipt_human_unavailable",
        Some("fixtures/receipt_human_unavailable"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(report.is_empty(), "{report:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_oracle_drift() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedOracle {
            kind: "smoke_only".to_string(),
            strength: "smoke".to_string(),
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:oracle-drift",
                "classification": "weakly_exposed",
                "oracle_kind": "exact_value",
                "oracle_strength": "strong"
            }
        ]
    });

    let report = super::super::evidence_promotion_semantic_violations(
        "oracle_drift",
        Some("fixtures/typescript_oracle_drift"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_oracle"), "{report}");
    assert!(report.contains("oracle_kind `exact_value`"), "{report}");
    assert!(report.contains("oracle_strength `strong`"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_oracle_projection() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedOracle {
            kind: "exact_value".to_string(),
            strength: "strong".to_string(),
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:oracle-human-drift",
                "classification": "exposed",
                "oracle_kind": "exact_value",
                "oracle_strength": "strong"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Evidence
  - related test tests/score.test.ts:3 reaches score
";

    let report = super::super::evidence_promotion_semantic_violations(
        "oracle_human_drift",
        Some("fixtures/typescript_oracle_human_drift"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("expected_oracle"), "{report}");
    assert!(report.contains("fixture human output"), "{report}");
    assert!(
        report.contains("oracle projection `exact_value/strong`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_human_oracle_projection() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedOracle {
            kind: "exact_value".to_string(),
            strength: "strong".to_string(),
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:oracle-human-projection",
                "classification": "exposed",
                "oracle_kind": "exact_value",
                "oracle_strength": "strong"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

TypeScript preview
  oracle: exact_value (strong)
";

    let report = super::super::evidence_promotion_semantic_violations(
        "oracle_human_projection",
        Some("fixtures/typescript_oracle_human_projection"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(report.is_empty(), "{report:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_human_oracle_golden() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedOracle {
            kind: "exact_value".to_string(),
            strength: "strong".to_string(),
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:oracle-missing-human",
                "classification": "exposed",
                "oracle_kind": "exact_value",
                "oracle_strength": "strong"
            }
        ]
    });

    let report = super::super::evidence_promotion_semantic_violations(
        "oracle_missing_human",
        Some("fixtures/typescript_oracle_missing_human"),
        &assertions,
        &check_json,
        None,
        true,
    )
    .join("\n");

    assert!(report.contains("expected_oracle"), "{report}");
    assert!(report.contains("expected/human-full.txt"), "{report}");
}

#[test]
fn evidence_promotion_human_oracle_line_matches_normalized_projection() {
    assert!(super::super::evidence_promotion_human_oracle_line_matches(
        "  oracle: exact_value (strong)",
        "exact_value",
        "strong"
    ));
    assert!(super::super::evidence_promotion_human_oracle_line_matches(
        "current test evidence: oracle_strength : strong; oracle_kind = exact_value.",
        "exact_value",
        "strong"
    ));
    assert!(super::super::evidence_promotion_human_oracle_line_matches(
        "current test evidence: oracle_kind=exact_value, oracle_strength=strong",
        "exact_value",
        "strong"
    ));
    assert!(!super::super::evidence_promotion_human_oracle_line_matches(
        "Strongest extracted oracle kind: `exact_value` (rank 5)",
        "exact_value",
        "strong"
    ));
}

#[test]
fn evidence_promotion_human_oracle_line_matches_real_rust_evidence() {
    assert!(super::super::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 observes_score uses weak relational check oracle: assert!(matches!(value, _));",
        "relational_check",
        "weak"
    ));
    assert!(super::super::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 observes_score uses strong exact value oracle: assert!(matches!(value, 2));",
        "exact_value",
        "strong"
    ));
    assert!(super::super::evidence_promotion_human_oracle_line_matches(
        "  - related test tests/a uses helpers/score.rs:8 observes_score uses weak relational check oracle: assert!(matches!(value, _));",
        "relational_check",
        "weak"
    ));
    assert!(super::super::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 r#match uses weak relational check oracle: assert!(matches!(value, _));",
        "relational_check",
        "weak"
    ));
}

#[test]
fn evidence_promotion_human_oracle_line_rejects_diagnostic_overrides() {
    assert!(!super::super::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 observes_score uses strong exact value oracle: assert!(false, \"oracle_kind=relational_check oracle_strength=weak\");",
        "relational_check",
        "weak"
    ));
    assert!(!super::super::evidence_promotion_human_oracle_line_matches(
        "message: uses weak relational check oracle: assert!(matches!(value, _));",
        "relational_check",
        "weak"
    ));
    assert!(!super::super::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 observes_score uses weak relational check oracle: assert!(matches!(value, _));",
        "exact_value",
        "strong"
    ));
    assert!(!super::super::evidence_promotion_human_oracle_line_matches(
        "  - related test tests/a uses helpers/score.rs:8 observes_score uses strong exact value oracle: assert!(false, \"src/lib.rs:8 observes_score uses weak relational check oracle: forged\");",
        "relational_check",
        "weak"
    ));
}

#[test]
fn evidence_promotion_semantic_assertions_retain_related_test_identity() -> Result<(), String> {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedRelatedTest {
            name: "observes_score".to_string(),
            file: "src/lib.rs".to_string(),
            line: 8,
            kind: "relational_check".to_string(),
            strength: "weak".to_string(),
            relation_reason: None,
            miss: None,
        },
    ];
    let original: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/wildcard_oracle_wildcard_original/expected/check.json"
    ))
    .map_err(|err| format!("invalid canonical wildcard golden: {err}"))?;
    let human =
        include_str!("../../../fixtures/wildcard_oracle_wildcard_original/expected/human-full.txt");
    let inspect = |json: &serde_json::Value| {
        super::super::evidence_promotion_semantic_violations(
            "related_test_identity",
            Some("fixtures/wildcard_oracle_wildcard_original"),
            &assertions,
            json,
            Some(human),
            true,
        )
    };
    assert!(inspect(&original).is_empty());
    for (field, replacement) in [
        ("name", serde_json::json!("unrelated_test")),
        ("file", serde_json::json!("src/unrelated.rs")),
        ("line", serde_json::json!(9)),
        ("oracle_kind", serde_json::json!("exact_value")),
        ("oracle_strength", serde_json::json!("strong")),
    ] {
        let mut changed = original.clone();
        changed["findings"][0]["related_tests"][0][field] = replacement;
        let violations = inspect(&changed);
        assert!(
            violations
                .iter()
                .any(|v| v.contains("expected_related_test")),
            "{field}: {violations:?}"
        );
    }
    for replacement in [serde_json::json!([]), serde_json::Value::Null] {
        let mut changed = original.clone();
        changed["findings"][0]["related_tests"] = replacement;
        assert!(!inspect(&changed).is_empty());
    }
    let mut missing = original.clone();
    missing["findings"][0]
        .as_object_mut()
        .ok_or("canonical wildcard finding must be an object")?
        .remove("related_tests");
    assert!(!inspect(&missing).is_empty());
    missing["findings"] = serde_json::json!([]);
    assert!(!inspect(&missing).is_empty());
    Ok(())
}

#[test]
fn evidence_promotion_related_test_relation_pin_is_optional_and_exact() -> Result<(), String> {
    let assertion_with_relation = |relation_reason: Option<&str>| {
        vec![
            super::super::EvidencePromotionSemanticAssertion::ExpectedRelatedTest {
                name: "observes_score".to_string(),
                file: "src/lib.rs".to_string(),
                line: 8,
                kind: "relational_check".to_string(),
                strength: "weak".to_string(),
                relation_reason: relation_reason.map(str::to_string),
                miss: None,
            },
        ]
    };
    let original: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/wildcard_oracle_wildcard_original/expected/check.json"
    ))
    .map_err(|err| format!("invalid canonical wildcard golden: {err}"))?;
    let human =
        include_str!("../../../fixtures/wildcard_oracle_wildcard_original/expected/human-full.txt");
    let inspect = |assertions: &[_], json: &serde_json::Value| {
        super::super::evidence_promotion_semantic_violations(
            "related_test_relation",
            Some("fixtures/wildcard_oracle_wildcard_original"),
            assertions,
            json,
            Some(human),
            true,
        )
    };
    // The canonical golden relates via `direct_owner_call`.
    assert!(inspect(&assertion_with_relation(None), &original).is_empty());
    assert!(
        inspect(
            &assertion_with_relation(Some("direct_owner_call")),
            &original
        )
        .is_empty()
    );
    assert!(
        !inspect(
            &assertion_with_relation(Some("weak_token_substring")),
            &original
        )
        .is_empty()
    );
    // A re-blessed relation flips the pin.
    let mut weak = original.clone();
    weak["findings"][0]["related_tests"][0]["relation_reason"] =
        serde_json::json!("weak_token_substring");
    assert!(
        inspect(
            &assertion_with_relation(Some("weak_token_substring")),
            &weak
        )
        .is_empty()
    );
    assert!(!inspect(&assertion_with_relation(Some("direct_owner_call")), &weak).is_empty());
    // Absent pins stay backward compatible.
    assert!(inspect(&assertion_with_relation(None), &weak).is_empty());
    Ok(())
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_class_projection() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedClass {
            class: "weakly_exposed".to_string(),
        },
    ];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedClass {
            class: "weakly_exposed".to_string(),
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:class-missing-human",
                "classification": "weakly_exposed"
            }
        ]
    });

    let report = super::super::evidence_promotion_semantic_violations(
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
    assert!(super::super::evidence_promotion_human_class_line_matches(
        "  weakly_exposed (warning, confidence 0.40)",
        "weakly_exposed"
    ));
    assert!(super::super::evidence_promotion_human_class_line_matches(
        "  exposed (info, confidence 0.60)",
        "exposed"
    ));
    assert!(!super::super::evidence_promotion_human_class_line_matches(
        "  weakly_exposed (warning, confidence 0.40)",
        "exposed"
    ));
}

#[test]
fn evidence_promotion_semantic_assertions_reject_expected_oracle_without_findings() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::ExpectedOracle {
            kind: "exact_value".to_string(),
            strength: "strong".to_string(),
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 0},
        "findings": []
    });

    let report = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::MustHaveReceiptCommand,
        super::super::EvidencePromotionSemanticAssertion::MustEmitRepairPacket,
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

    let report = super::super::evidence_promotion_semantic_violations(
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
        vec![super::super::EvidencePromotionSemanticAssertion::MustDiscloseRepairPacketDetail];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::ExpectedRepairPacketDetail {
            detail: super::super::ExpectedRepairPacketDetail {
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging,
    ];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::MaximumClass {
            class: "weakly_exposed".to_string(),
        },
    ];
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

    let scoped = super::super::evidence_promotion_semantic_violations_scoped(
        "scoped_predicate_control",
        Some("fixtures/split_test_boundary_oracle"),
        &assertions,
        &check_json,
        None,
        false,
        Some("predicate"),
    );
    assert!(scoped.is_empty(), "{scoped:?}");

    let promoted_family = super::super::evidence_promotion_semantic_violations_scoped(
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

    let unscoped = super::super::evidence_promotion_semantic_violations(
        "unscoped_fixture_wide",
        Some("fixtures/split_test_boundary_oracle"),
        &assertions,
        &check_json,
        None,
        false,
    );
    assert_eq!(unscoped.len(), 1, "{unscoped:?}");

    let empty_match = super::super::evidence_promotion_semantic_violations_scoped(
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
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging,
    ];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging,
    ];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging,
    ];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
        vec![super::super::EvidencePromotionSemanticAssertion::MustDiscloseRepairPacketDetail];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::ExpectedRepairPacketDetail {
            detail: super::super::ExpectedRepairPacketDetail {
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

    let report = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::MustDiscloseWitness,
        super::super::EvidencePromotionSemanticAssertion::MustNotClaimNoTestsFound,
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions = vec![super::super::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions = vec![super::super::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions = vec![super::super::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::MustDiscloseWitness,
        super::super::EvidencePromotionSemanticAssertion::MustNotClaimNoTestsFound,
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions =
        vec![super::super::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
    let assertions =
        vec![super::super::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail];
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

    let report = super::super::evidence_promotion_semantic_violations(
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
            super::super::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail,
            super::super::EvidencePromotionSemanticAssertion::ExpectedLimitationDetail {
                last_established_edge:
                    "test `integration_path` (tests/it.rs:4) -> entry `outer`".to_string(),
                first_unresolved_edge:
                    "entry `outer` -> owner `inner` through a transitive Rust helper path"
                        .to_string(),
                non_claim:
                    "named limitation only; ripr cannot confirm or deny that this path observes the change"
                        .to_string(),
            },
            super::super::EvidencePromotionSemanticAssertion::ExpectedLimitationRoute {
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

    let violations = super::super::evidence_promotion_semantic_violations(
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
            super::super::EvidencePromotionSemanticAssertion::ExpectedLimitationDetail {
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

    let report = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::ExpectedLimitationRoute {
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

    let report = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::MustNotReportClean,
        super::super::EvidencePromotionSemanticAssertion::MustDiscloseNoScope,
    ];
    let no_scope_violations = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::MustNotReportClean,
        super::super::EvidencePromotionSemanticAssertion::MustDiscloseUnanalyzedWorkingTree,
    ];
    let dirty_violations = super::super::evidence_promotion_semantic_violations(
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
        super::super::EvidencePromotionSemanticAssertion::MustDiscloseScope,
        super::super::EvidencePromotionSemanticAssertion::MustNotDiscloseUnanalyzedWorkingTree,
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

    let scoped_violations = super::super::evidence_promotion_semantic_violations(
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

    let report = super::super::evidence_promotion_semantic_violations(
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

#[test]
fn evidence_promotion_semantic_assertions_reject_false_no_scope_disclosure() {
    let assertions = vec![
        super::super::EvidencePromotionSemanticAssertion::MustDiscloseScope,
        super::super::EvidencePromotionSemanticAssertion::MustNotDiscloseNoScope,
    ];
    let scoped_report = serde_json::json!({
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

    let scoped_violations = super::super::evidence_promotion_semantic_violations(
        "scoped_report",
        Some("fixtures/scope_worktree_analyzed_dirty"),
        &assertions,
        &scoped_report,
        None,
        false,
    );
    assert!(
        scoped_violations.is_empty(),
        "explicit scope report without no-scope disclosure should pass: {scoped_violations:?}"
    );

    let legacy_no_scope = serde_json::json!({
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
        "no_scope_provided": true
    });
    let legacy_report = super::super::evidence_promotion_semantic_violations(
        "legacy_no_scope",
        Some("fixtures/scope_worktree_analyzed_dirty"),
        &assertions,
        &legacy_no_scope,
        None,
        false,
    )
    .join("\n");
    assert!(
        legacy_report.contains("must_not_disclose_no_scope"),
        "{legacy_report}"
    );

    let structured_no_scope = serde_json::json!({
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
        "scope_disclosures": [
            {"scope_status": "no_scope_provided", "category": "no_scope_disclosure"}
        ]
    });
    let structured_report = super::super::evidence_promotion_semantic_violations(
        "structured_no_scope",
        Some("fixtures/scope_worktree_analyzed_dirty"),
        &assertions,
        &structured_no_scope,
        None,
        false,
    )
    .join("\n");
    assert!(
        structured_report.contains("must_not_disclose_no_scope"),
        "{structured_report}"
    );
    assert!(
        structured_report.contains("explicit analysis scope"),
        "{structured_report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_bare_empty_false_clean() {
    let assertions = vec![super::super::EvidencePromotionSemanticAssertion::MustNotReportClean];
    let bare_empty = serde_json::json!({
        "summary": {"findings": 0},
        "findings": []
    });

    let report = super::super::evidence_promotion_semantic_violations(
        "bare_empty",
        Some("fixtures/scope_honesty_bare_empty"),
        &assertions,
        &bare_empty,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("must_not_report_clean"), "{report}");
    assert!(report.contains("no non-clean signal"), "{report}");
}

#[test]
fn evidence_promotion_pinned_external_semantics_accept_semver_limitation_shape() {
    let case = semver_external_case_for_test();
    let check_json = semver_limitation_check_json();

    let violations =
        super::super::evidence_promotion_external_semantic_violations(&case, &check_json);
    assert!(
        violations.is_empty(),
        "expected semver external limitation shape to pass, got {violations:?}"
    );
}

#[test]
fn evidence_promotion_pinned_external_without_diff_still_requires_base() {
    let mut case = semver_external_case_for_test();
    case.external_command = "ripr check --root {checkout} --mode fast --json".to_string();
    case.external_patch = PathBuf::new();
    let check_json = semver_limitation_check_json();

    let report = super::super::evidence_promotion_external_semantic_violations(&case, &check_json)
        .join("\n");
    assert!(
        report.contains("must_disclose_scope") && report.contains("base"),
        "a pinned-external launch without --diff provenance must still require base, got {report}"
    );
}

/// Dispatch-level coverage for the harness-projection surface (#3636):
/// `must_emit_limitation` must accept the kind from
/// `test_harnesses[].limitations[].code` through the same semantic
/// dispatch the corpus gate uses, not only through the helper predicate.
#[test]
fn evidence_promotion_semantics_accept_harness_limitation_kind() {
    let case = harness_limitation_case(vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotReportClean,
        super::super::EvidencePromotionSemanticAssertion::MustEmitLimitation {
            expected_limit_kind: "registration_unreachable".to_string(),
        },
    ]);
    let violations = super::super::evidence_promotion_external_semantic_violations(
        &case,
        &harness_check_json(Some("registration_unreachable")),
    );
    assert!(
        violations.is_empty(),
        "expected the harness projection kind to satisfy must_emit_limitation, got {violations:?}"
    );
}

#[test]
fn evidence_promotion_semantics_reject_missing_harness_limitation_kind() {
    let case = harness_limitation_case(vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotReportClean,
        super::super::EvidencePromotionSemanticAssertion::MustEmitLimitation {
            expected_limit_kind: "registration_unreachable".to_string(),
        },
    ]);
    let violations = super::super::evidence_promotion_external_semantic_violations(
        &case,
        &harness_check_json(None),
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("expected limitation kind")),
        "expected the stripped limitations to fail must_emit_limitation, got {violations:?}"
    );
}

#[test]
fn evidence_promotion_must_not_emit_limitation_forbids_harness_projection() {
    let case = harness_limitation_case(vec![
        super::super::EvidencePromotionSemanticAssertion::MustNotEmitLimitation,
    ]);
    let violations = super::super::evidence_promotion_external_semantic_violations(
        &case,
        &harness_check_json(Some("registration_unreachable")),
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("test_harnesses limitations")),
        "expected the harness projection to fail must_not_emit_limitation, got {violations:?}"
    );
}

#[test]
fn evidence_promotion_pinned_external_semantics_reject_false_clean_and_packet() {
    let case = semver_external_case_for_test();
    let check_json = serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "mode": "fast",
        "root": "target/ripr/evidence-promotion-honesty/checkouts/semver",
        "summary": {"findings": 0},
        "findings": [
            {
                "id": "probe:src_eval.rs:predicate:87a89f28",
                "classification": "exposed",
                "probe": {"file": "target/ripr/evidence-promotion-honesty/checkouts/semver/src/eval.rs"},
                "preview_actionability": {"repair_packet_ready": true},
                "evidence": []
            }
        ]
    });

    let report = super::super::evidence_promotion_external_semantic_violations(&case, &check_json)
        .join("\n");
    assert!(report.contains("expected limitation kind"), "{report}");
    assert!(report.contains("repair_packet_ready=true"), "{report}");
    assert!(report.contains("must_disclose_witness"), "{report}");
    assert!(report.contains("promoted to exposed"), "{report}");
}

#[test]
fn evidence_promotion_corpus_summary_report_writes_pure_and_not_run_external_cases()
-> Result<(), String> {
    with_temp_cwd("evidence-promotion-corpus-summary", |root| {
        let corpus = root.join(super::super::EVIDENCE_PROMOTION_HONESTY_CORPUS);
        let py_fixture = root.join("fixtures/py");
        let ts_fixture = root.join("fixtures/ts");
        let rust_fixture = root.join("fixtures/rust");
        let rust_control_fixture = root.join("fixtures/rust-control");
        let ts_control_fixture = root.join("fixtures/ts-control");
        let patch_path =
            root.join("fixtures/evidence-promotion-honesty-corpus/patches/semver-boundary.diff");

        write(&patch_path, "diff --git a/src/lib.rs b/src/lib.rs\n");
        for (fixture, classification) in [
            (&py_fixture, "weakly_exposed"),
            (&ts_fixture, "weakly_exposed"),
            (&rust_fixture, "no_static_path"),
            (&rust_control_fixture, "exposed"),
            (&ts_control_fixture, "exposed"),
        ] {
            write_evidence_promotion_check(fixture, classification)?;
        }

        let corpus_json = serde_json::json!({
            "cases": [
                {
                    "id": "py_non_promoted",
                    "language": "python",
                    "tier": "pure",
                    "source_fixture": py_fixture,
                    "must_remain_non_promoted": true
                },
                {
                    "id": "ts_non_promoted",
                    "language": "typescript",
                    "tier": "pure",
                    "source_fixture": ts_fixture,
                    "must_remain_non_promoted": true
                },
                {
                    "id": "rust_non_promoted",
                    "language": "rust",
                    "tier": "pure",
                    "source_fixture": rust_fixture,
                    "must_remain_non_promoted": true,
                    "expected_max_class": "no_static_path"
                },
                {
                    "id": "rust_semver_external",
                    "language": "rust",
                    "tier": "pinned_external",
                    "external_repo": "https://github.com/dtolnay/semver",
                    "external_command": "ripr check --root {checkout} --diff {external_patch} --mode fast --json",
                    "external_commit": "0123456789abcdef0123456789abcdef01234567",
                    "external_patch": "fixtures/evidence-promotion-honesty-corpus/patches/semver-boundary.diff",
                    "runtime_budget_seconds": 120,
                    "artifact_budget_bytes": 10485760,
                    "must_not_report_clean": true
                },
                {
                    "id": "rust_control",
                    "language": "rust",
                    "tier": "pure",
                    "source_fixture": rust_control_fixture,
                    "expected_promoted": true
                },
                {
                    "id": "ts_control",
                    "language": "typescript",
                    "tier": "pure",
                    "source_fixture": ts_control_fixture,
                    "expected_promoted": true
                }
            ]
        });
        write(
            &corpus,
            &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
        );

        super::super::check_evidence_promotion_honesty(&[])?;
        let summary_path = root.join("target/ripr/reports/corpus-summary.json");
        let summary_text = fs::read_to_string(&summary_path)
            .map_err(|err| format!("read {}: {err}", summary_path.display()))?;
        let summary: Value = serde_json::from_str(&summary_text)
            .map_err(|err| format!("parse corpus summary: {err}"))?;

        assert_eq!(
            summary.get("kind").and_then(Value::as_str),
            Some("corpus_summary")
        );
        assert_eq!(summary.get("status").and_then(Value::as_str), Some("pass"));
        assert_eq!(summary.get("cases_failed").and_then(Value::as_u64), Some(0));
        assert_eq!(
            summary.get("cases_not_run").and_then(Value::as_u64),
            Some(1)
        );
        let cases = summary
            .get("cases")
            .and_then(Value::as_array)
            .ok_or_else(|| "corpus summary cases array missing".to_string())?;
        let external = cases
            .iter()
            .find(|case| case.get("id").and_then(Value::as_str) == Some("rust_semver_external"))
            .ok_or_else(|| "external case summary missing".to_string())?;
        assert_eq!(
            external.get("status").and_then(Value::as_str),
            Some("not_run")
        );
        assert_eq!(
            external.get("result_kind").and_then(Value::as_str),
            Some("not_run")
        );
        let external_case = external
            .get("external_case")
            .and_then(Value::as_object)
            .ok_or_else(|| "external case launch metadata missing".to_string())?;
        assert_eq!(
            external_case.get("repo").and_then(Value::as_str),
            Some("https://github.com/dtolnay/semver")
        );
        assert_eq!(
            external_case.get("commit").and_then(Value::as_str),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
        assert_eq!(
            external_case.get("patch").and_then(Value::as_str),
            Some("fixtures/evidence-promotion-honesty-corpus/patches/semver-boundary.diff")
        );
        assert_eq!(
            external_case.get("command").and_then(Value::as_str),
            Some("ripr check --root {checkout} --diff {external_patch} --mode fast --json")
        );
        assert_eq!(
            external_case
                .get("runtime_budget_seconds")
                .and_then(Value::as_u64),
            Some(120)
        );
        assert_eq!(
            external_case
                .get("artifact_budget_bytes")
                .and_then(Value::as_u64),
            Some(10_485_760)
        );
        assert!(
            cases.iter().any(|case| {
                case.get("id").and_then(Value::as_str) == Some("rust_non_promoted")
                    && case.get("status").and_then(Value::as_str) == Some("pass")
                    && case.get("result_kind").and_then(Value::as_str) == Some("pass")
            }),
            "expected passing pure rust case in {summary_text}"
        );

        let summary_md = fs::read_to_string(root.join("target/ripr/reports/corpus-summary.md"))
            .map_err(|err| format!("read corpus summary markdown: {err}"))?;
        assert!(summary_md.contains("Status: `pass`"));
        assert!(summary_md.contains("`not_run`"));
        assert!(summary_md.contains("## Pinned External Launches"));
        assert!(summary_md.contains("https://github.com/dtolnay/semver"));
        assert!(summary_md.contains("0123456789abcdef0123456789abcdef01234567"));
        assert!(summary_md.contains("10485760"));
        Ok(())
    })
}

#[test]
fn evidence_promotion_pinned_external_report_projects_launch_metadata() -> Result<(), String> {
    with_temp_cwd("evidence-promotion-pinned-external-report", |root| {
        let run = super::super::EvidencePromotionExternalRun {
            id: "rust_semver_external".to_string(),
            status: "pass".to_string(),
            result_kind: "pass".to_string(),
            runtime_ms: 321,
            artifact_bytes: 654,
            external_case: Some(super::super::EvidencePromotionExternalLaunch {
                repo: "https://github.com/dtolnay/semver".to_string(),
                commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
                patch: "fixtures/evidence-promotion-honesty-corpus/patches/semver-boundary.diff"
                    .to_string(),
                command: "ripr check --root {checkout} --diff {external_patch} --mode fast --json"
                    .to_string(),
                runtime_budget_seconds: 120,
                artifact_budget_bytes: 10_485_760,
            }),
            checkout: "target/ripr/evidence-promotion-honesty/checkouts/rust_semver_external"
                .to_string(),
            violations: Vec::new(),
        };

        super::super::write_evidence_promotion_external_report(&[run], &[])?;

        let report_path = root
            .join("target/ripr/reports")
            .join(super::super::EVIDENCE_PROMOTION_EXTERNAL_JSON);
        let report_text = fs::read_to_string(&report_path)
            .map_err(|err| format!("read {}: {err}", report_path.display()))?;
        let report: Value = serde_json::from_str(&report_text)
            .map_err(|err| format!("parse pinned external report: {err}"))?;
        let launch = report
            .get("runs")
            .and_then(Value::as_array)
            .and_then(|runs| runs.first())
            .and_then(|run| run.get("external_case"))
            .and_then(Value::as_object)
            .ok_or_else(|| "pinned external launch metadata missing".to_string())?;
        assert_eq!(
            launch.get("repo").and_then(Value::as_str),
            Some("https://github.com/dtolnay/semver")
        );
        assert_eq!(
            launch.get("commit").and_then(Value::as_str),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
        assert_eq!(
            launch.get("runtime_budget_seconds").and_then(Value::as_u64),
            Some(120)
        );
        assert_eq!(
            launch.get("artifact_budget_bytes").and_then(Value::as_u64),
            Some(10_485_760)
        );

        let markdown_path = root
            .join("target/ripr/reports")
            .join(super::super::EVIDENCE_PROMOTION_EXTERNAL_MD);
        let markdown = fs::read_to_string(&markdown_path)
            .map_err(|err| format!("read {}: {err}", markdown_path.display()))?;
        assert!(markdown.contains("## Launch Points"));
        assert!(markdown.contains("https://github.com/dtolnay/semver"));
        assert!(markdown.contains("0123456789abcdef0123456789abcdef01234567"));
        assert!(markdown.contains("10485760"));
        Ok(())
    })
}

#[test]
fn evidence_promotion_corpus_summary_reports_pinned_external_setup_failure() -> Result<(), String> {
    with_temp_cwd("evidence-promotion-corpus-summary-setup-failure", |root| {
        let corpus = root.join(super::super::EVIDENCE_PROMOTION_HONESTY_CORPUS);
        let py_fixture = root.join("fixtures/py");
        let ts_fixture = root.join("fixtures/ts");
        let rust_fixture = root.join("fixtures/rust");
        let rust_control_fixture = root.join("fixtures/rust-control");
        let ts_control_fixture = root.join("fixtures/ts-control");
        let patch_path =
            root.join("fixtures/evidence-promotion-honesty-corpus/patches/semver-boundary.diff");

        write(&patch_path, "diff --git a/src/lib.rs b/src/lib.rs\n");
        for (fixture, classification) in [
            (&py_fixture, "weakly_exposed"),
            (&ts_fixture, "weakly_exposed"),
            (&rust_fixture, "no_static_path"),
            (&rust_control_fixture, "exposed"),
            (&ts_control_fixture, "exposed"),
        ] {
            write_evidence_promotion_check(fixture, classification)?;
        }

        let corpus_json = serde_json::json!({
            "cases": [
                {
                    "id": "py_non_promoted",
                    "language": "python",
                    "tier": "pure",
                    "source_fixture": py_fixture,
                    "must_remain_non_promoted": true
                },
                {
                    "id": "ts_non_promoted",
                    "language": "typescript",
                    "tier": "pure",
                    "source_fixture": ts_fixture,
                    "must_remain_non_promoted": true
                },
                {
                    "id": "rust_non_promoted",
                    "language": "rust",
                    "tier": "pure",
                    "source_fixture": rust_fixture,
                    "must_remain_non_promoted": true,
                    "expected_max_class": "no_static_path"
                },
                {
                    "id": "rust_bad_external",
                    "language": "rust",
                    "tier": "pinned_external",
                    "external_command": "ripr check --root {checkout} --diff {external_patch} --mode fast --json",
                    "external_commit": "0123456789abcdef0123456789abcdef01234567",
                    "external_patch": "fixtures/evidence-promotion-honesty-corpus/patches/semver-boundary.diff",
                    "runtime_budget_seconds": 120,
                    "artifact_budget_bytes": 10485760,
                    "must_not_report_clean": true
                },
                {
                    "id": "rust_control",
                    "language": "rust",
                    "tier": "pure",
                    "source_fixture": rust_control_fixture,
                    "expected_promoted": true
                },
                {
                    "id": "ts_control",
                    "language": "typescript",
                    "tier": "pure",
                    "source_fixture": ts_control_fixture,
                    "expected_promoted": true
                }
            ]
        });
        write(
            &corpus,
            &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
        );

        let err =
            super::super::check_evidence_promotion_honesty(&["--pinned-external".to_string()])
                .expect_err("malformed pinned_external case should fail");
        assert!(
            err.contains("missing required string `external_repo`"),
            "{err}"
        );

        let summary_path = root.join("target/ripr/reports/corpus-summary.json");
        let summary_text = fs::read_to_string(&summary_path)
            .map_err(|err| format!("read {}: {err}", summary_path.display()))?;
        let summary: Value = serde_json::from_str(&summary_text)
            .map_err(|err| format!("parse corpus summary: {err}"))?;
        assert_eq!(summary.get("status").and_then(Value::as_str), Some("fail"));
        let cases = summary
            .get("cases")
            .and_then(Value::as_array)
            .ok_or_else(|| "corpus summary cases array missing".to_string())?;
        assert!(
            cases.iter().any(|case| {
                case.get("id").and_then(Value::as_str) == Some("rust_bad_external")
                    && case.get("status").and_then(Value::as_str) == Some("fail")
                    && case.get("result_kind").and_then(Value::as_str) == Some("setup_failure")
            }),
            "expected selected pinned_external case setup failure in {summary_text}"
        );
        assert!(
            cases.iter().any(|case| {
                case.get("id").and_then(Value::as_str) == Some("__runner__")
                    && case.get("result_kind").and_then(Value::as_str) == Some("setup_failure")
            }),
            "expected runner setup failure in {summary_text}"
        );
        Ok(())
    })
}

#[test]
fn evidence_promotion_corpus_summary_classifies_failure_kinds() {
    assert_eq!(
        super::super::evidence_promotion_pure_failure_kind(&[
            r#"case `pure_clean`: `must_not_report_clean` failed after a re-bless"#.to_string()
        ]),
        "golden_drift"
    );
    assert_eq!(
        super::super::evidence_promotion_pure_failure_kind(&[
            r#"case `pure_promoted`: finding has classification `exposed`"#.to_string()
        ]),
        "unexpected_promotion"
    );
    assert_eq!(
        super::super::evidence_promotion_pure_failure_kind(&[
            r#"case `pure_setup`: source_fixture `fixtures/missing` does not exist"#.to_string()
        ]),
        "setup_failure"
    );
    assert_eq!(
        super::super::evidence_promotion_external_failure_kind(&[
            "clone failed: could not resolve host".to_string()
        ]),
        "network_unavailable"
    );
    assert_eq!(
        super::super::evidence_promotion_external_failure_kind(&[
            "runtime budget exceeded by timeout after 120000ms".to_string()
        ]),
        "runtime_budget_exceeded"
    );
    assert_eq!(
        super::super::evidence_promotion_external_failure_kind(&[
            "artifact bytes 11 exceeded budget 10".to_string()
        ]),
        "artifact_budget_exceeded"
    );
    assert_eq!(
        super::super::evidence_promotion_external_failure_kind(&[
            "expected static_limit_kind `rust_transitive_reach_unresolved` was not emitted"
                .to_string()
        ]),
        "unexpected_limitation"
    );
    assert_eq!(
        super::super::evidence_promotion_external_failure_kind(&[
            "finding `probe` promoted to exposed".to_string()
        ]),
        "unexpected_promotion"
    );
    assert_eq!(
        super::super::evidence_promotion_external_failure_kind(&[
            "must_not_report_clean requires findings".to_string()
        ]),
        "semantic_failure"
    );
}

#[test]
fn evidence_promotion_honesty_rejects_clean_report_for_clean_guard_case() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-clean-guard");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_clean_fixture = root.join("fixtures/rust-clean");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, summary_findings, findings) in [
        (
            &py_fixture,
            1,
            serde_json::json!([{"id":"py","classification":"weakly_exposed"}]),
        ),
        (
            &ts_fixture,
            1,
            serde_json::json!([{"id":"ts","classification":"weakly_exposed"}]),
        ),
        (&rust_clean_fixture, 0, serde_json::json!([])),
        (
            &rust_control_fixture,
            1,
            serde_json::json!([{"id":"rust-control","classification":"exposed"}]),
        ),
        (
            &ts_control_fixture,
            1,
            serde_json::json!([{"id":"ts-control","classification":"exposed"}]),
        ),
    ] {
        let check_json = serde_json::json!({
            "summary": {"findings": summary_findings},
            "findings": findings
        });
        write(
            &fixture.join("expected/check.json"),
            &serde_json::to_string_pretty(&check_json).map_err(|err| err.to_string())?,
        );
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_clean_guard",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_clean_fixture,
                "must_not_report_clean": true,
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;

    assert!(
        violations.iter().any(|violation| {
            violation.contains("rust_clean_guard")
                && violation.contains("must_not_report_clean")
                && violation.contains("summary.findings=0")
        }),
        "expected clean-guard violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_missing_scope_for_scope_guard_case() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-scope-guard");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_scope_fixture = root.join("fixtures/rust-scope");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_scope_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        let check_json = serde_json::json!({
            "summary": {"findings": 1},
            "findings": [{"id": classification, "classification": classification}]
        });
        write(
            &fixture.join("expected/check.json"),
            &serde_json::to_string_pretty(&check_json).map_err(|err| err.to_string())?,
        );
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_scope_guard",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_scope_fixture,
                "must_disclose_scope": true,
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(
        report.contains("rust_scope_guard") && report.contains("must_disclose_scope"),
        "expected scope-guard violation, got {violations:?}"
    );
    assert!(report.contains("schema_version"));
    assert!(report.contains("mode"));
    assert!(report.contains("root"));
    assert!(report.contains("base"));
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_packet_ready_limitation_case() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-no-packet-guard");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_packet_fixture = root.join("fixtures/rust-packet");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, findings) in [
        (
            &py_fixture,
            serde_json::json!([{"id":"py","classification":"weakly_exposed"}]),
        ),
        (
            &ts_fixture,
            serde_json::json!([{"id":"ts","classification":"weakly_exposed"}]),
        ),
        (
            &rust_packet_fixture,
            serde_json::json!([{
                "id": "rust-packet",
                "classification": "no_static_path",
                "preview_actionability": {
                    "repair_packet_ready": true
                }
            }]),
        ),
        (
            &rust_control_fixture,
            serde_json::json!([{"id":"rust-control","classification":"exposed"}]),
        ),
        (
            &ts_control_fixture,
            serde_json::json!([{"id":"ts-control","classification":"exposed"}]),
        ),
    ] {
        let check_json = serde_json::json!({
            "summary": {"findings": 1},
            "findings": findings
        });
        write(
            &fixture.join("expected/check.json"),
            &serde_json::to_string_pretty(&check_json).map_err(|err| err.to_string())?,
        );
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_packet_guard",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_packet_fixture,
                "must_not_emit_repair_packet": true,
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(
        report.contains("rust_packet_guard")
            && report.contains("must_not_emit_repair_packet")
            && report.contains("findings[0].preview_actionability.repair_packet_ready"),
        "expected packet-ready guard violation, got {violations:?}"
    );
    Ok(())
}

// RIPR-SPEC-0108 (#1729): prove the honesty gate actually ENFORCES the Perl
// advisory corpus entry rather than letting it sit inert. The real charter
// case `perl_preview_card_advisory_no_repair_packet` is a tier-`pure`,
// `source_report` case whose typed assertions include `must_not_promote`
// and `expected_class: weakly_exposed`; corpus iteration is language-
// agnostic, so those assertions are evaluated on every default
// `check-evidence-promotion-honesty` run. This pins that enforcement in
// both directions: an honest advisory Perl report stays clean, and a
// dishonest re-bless of the advisory finding to `exposed` is caught.
#[test]
fn evidence_promotion_honesty_catches_dishonest_perl_advisory_rebless() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-perl-advisory");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");
    let perl_report = root.join("reports/perl-preview-advisory.json");

    // Honest {python, typescript, rust} non-promoted + rust/ts control
    // cases so the corpus satisfies the language-parity requirements and
    // only the Perl case can move the needle.
    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "weakly_exposed"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    // Mirror the real Perl charter case shape: a byte-pinned advisory-only
    // `source_report` with a single `weakly_exposed` finding.
    let perl_report_json = |classification: &str| {
        serde_json::json!({
            "schema_version": "0.2",
            "tool": "ripr",
            "mode": "draft",
            "root": ".",
            "base": "origin/main",
            "summary": {"changed_perl_files": 1, "probes": 1, "findings": 1},
            "findings": [{
                "id": "perl:preview:discount-threshold",
                "language": "perl",
                "classification": classification,
                "perl_preview_card": {
                    "language_status": "preview",
                    "authority_boundary": "preview_advisory_only",
                    "repair_packet_ready": false
                }
            }]
        })
    };

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_non_promoted",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "perl_advisory_dishonest_rebless",
                "language": "perl",
                "tier": "pure",
                "source_report": perl_report,
                "assertions": [
                    {"type": "must_not_report_clean"},
                    {"type": "must_not_promote"},
                    {"type": "expected_class", "class": "weakly_exposed"}
                ]
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    // Green witness: an honest advisory Perl report yields no violation, so
    // the caught case below is a real regression signal, not always-on.
    write(
        &perl_report,
        &serde_json::to_string_pretty(&perl_report_json("weakly_exposed"))
            .map_err(|err| err.to_string())?,
    );
    let mut honest_violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut honest_violations)?;
    assert!(
        honest_violations.is_empty(),
        "honest Perl advisory corpus must be clean, got {honest_violations:?}"
    );

    // Caught: a dishonest re-bless of the Perl advisory finding to `exposed`
    // trips the `must_not_promote` assertion the corpus entry carries.
    write(
        &perl_report,
        &serde_json::to_string_pretty(&perl_report_json("exposed"))
            .map_err(|err| err.to_string())?,
    );
    let mut dishonest_violations = Vec::new();
    super::super::validate_evidence_promotion_honesty_corpus_at(
        &corpus,
        &mut dishonest_violations,
    )?;
    assert!(
        dishonest_violations.iter().any(|violation| {
            violation.contains("perl_advisory_dishonest_rebless")
                && violation.contains("must_not_promote")
                && violation.contains("promoted to exposed")
        }),
        "expected the dishonest Perl advisory re-bless to be caught, got {dishonest_violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_pass_report_names_clean_guard() -> Result<(), String> {
    with_temp_cwd("evidence-promotion-honesty-clean-guard-report", |root| {
        let corpus = root.join(super::super::EVIDENCE_PROMOTION_HONESTY_CORPUS);
        let py_fixture = root.join("fixtures/py");
        let ts_fixture = root.join("fixtures/ts");
        let rust_guard_fixture = root.join("fixtures/rust-guard");
        let rust_control_fixture = root.join("fixtures/rust-control");
        let ts_control_fixture = root.join("fixtures/ts-control");

        for (fixture, classification) in [
            (&py_fixture, "weakly_exposed"),
            (&ts_fixture, "weakly_exposed"),
            (&rust_guard_fixture, "no_static_path"),
            (&rust_control_fixture, "exposed"),
            (&ts_control_fixture, "exposed"),
        ] {
            let check_json = serde_json::json!({
                "summary": {"findings": 1},
                "findings": [{"id": classification, "classification": classification}]
            });
            write(
                &fixture.join("expected/check.json"),
                &serde_json::to_string_pretty(&check_json).map_err(|err| err.to_string())?,
            );
        }

        let corpus_json = serde_json::json!({
            "cases": [
                {
                    "id": "py_non_promoted",
                    "language": "python",
                    "tier": "pure",
                    "source_fixture": py_fixture,
                    "must_remain_non_promoted": true
                },
                {
                    "id": "ts_non_promoted",
                    "language": "typescript",
                    "tier": "pure",
                    "source_fixture": ts_fixture,
                    "must_remain_non_promoted": true
                },
                {
                    "id": "rust_clean_guard",
                    "language": "rust",
                    "tier": "pure",
                    "source_fixture": rust_guard_fixture,
                    "must_not_report_clean": true,
                    "must_remain_non_promoted": true,
                    "expected_max_class": "no_static_path"
                },
                {
                    "id": "rust_control",
                    "language": "rust",
                    "tier": "pure",
                    "source_fixture": rust_control_fixture,
                    "expected_promoted": true
                },
                {
                    "id": "ts_control",
                    "language": "typescript",
                    "tier": "pure",
                    "source_fixture": ts_control_fixture,
                    "expected_promoted": true
                }
            ]
        });
        write(
            &corpus,
            &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
        );

        super::super::check_evidence_promotion_honesty(&[])?;
        let report =
            fs::read_to_string(root.join("target/ripr/reports/evidence-promotion-honesty.md"))
                .map_err(|err| format!("read check-evidence-promotion-honesty report: {err}"))?;

        assert!(
            report.contains("no clean-guard case lost its findings"),
            "pass report should name the clean-guard invariant, got {report}"
        );
        assert!(
            report.contains("scope-guard cases kept report scope headers"),
            "pass report should name the scope-guard invariant, got {report}"
        );
        Ok(())
    })
}

#[test]
fn evidence_record_contract_fixture_corpus_is_valid() -> Result<(), String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest must have workspace parent".to_string())?;
    let corpus =
        repo_root.join("fixtures/boundary_gap/expected/evidence-record-contract/corpus.json");
    let mut violations = Vec::new();
    super::super::validate_evidence_record_contract_fixture_corpus_at(&corpus, &mut violations)?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn evidence_record_contract_fixture_guard_reports_missing_fields() -> Result<(), String> {
    let root = temp_dir("evidence-record-contract-invalid");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "kind": "wrong",
  "schema_version": "0.2",
  "spec": "RIPR-SPEC-9999",
  "cases": [
    {
      "id": "predicate_boundary_missing_equality",
      "source": "fixture",
      "record": {
        "schema_version": "0.2",
        "seam_id": "bad",
        "canonical_gap_id": 12,
        "owner": "owner",
        "seam_kind": "predicate_boundary",
        "grip_class": "weakly_gripped",
        "headline_eligible": "yes",
        "evidence_path": {
          "reach": {"state": "yes"},
          "activate": {},
          "propagate": {},
          "observe": {},
          "discriminate": {}
        },
        "observed_values": [],
        "missing_discriminators": [],
        "related_tests": [],
        "recommendation": {},
        "actionability": {"class": "unsupported", "signals": {}},
        "calibration": {},
        "static_limitations": []
      }
    }
  ]
}
"#,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_record_contract_fixture_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("kind must be evidence_record_contract_corpus"));
    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("spec must be RIPR-SPEC-0021"));
    assert!(report.contains("record schema_version must be 0.1"));
    assert!(report.contains("canonical_gap_id must be string or null"));
    assert!(report.contains("canonical_gap_group_size must be number or null"));
    assert!(report.contains("canonical_gap_reason must be string or null"));
    assert!(report.contains("headline_eligible must be boolean"));
    assert!(report.contains("location must be an object"));
    assert!(report.contains("is missing string field confidence"));
    assert!(report.contains("recommendation"));
    assert!(report.contains("actionability.class is unsupported"));
    assert!(report.contains("calibration"));
    assert!(report.contains("missing case exact_error_variant_gap"));
    Ok(())
}

#[test]
fn evidence_record_contract_fixture_guard_requires_oracle_semantics() -> Result<(), String> {
    let root = temp_dir("evidence-record-contract-oracle-semantics-invalid");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "kind": "evidence_record_contract_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0021",
  "cases": [
    {
      "id": "predicate_boundary_missing_equality",
      "description": "invalid oracle semantics shape",
      "source": "fixture",
      "record": {
        "schema_version": "0.1",
        "seam_id": "contract-predicate-boundary",
        "canonical_gap_id": null,
        "canonical_gap_group_size": null,
        "canonical_gap_reason": null,
        "owner": "src/pricing.rs::discounted_total",
        "location": {"file": "src/pricing.rs", "line": 88},
        "seam_kind": "predicate_boundary",
        "grip_class": "weakly_gripped",
        "headline_eligible": true,
        "evidence_path": {
          "reach": {"state": "yes", "confidence": "medium", "summary": "related tests call the owner"},
          "activate": {"state": "yes", "confidence": "medium", "summary": "boundary values are visible"},
          "propagate": {"state": "yes", "confidence": "low", "summary": "predicate flows to return value"},
          "observe": {"state": "yes", "confidence": "medium", "summary": "related tests assert returned totals"},
          "discriminate": {"state": "weak", "confidence": "medium", "summary": "boundary is not asserted"}
        },
        "observed_values": [],
        "missing_discriminators": [],
        "related_tests_total": 1,
        "related_tests": [
          {"name": "below_threshold_has_no_discount", "file": "tests/pricing.rs", "line": 10, "oracle_kind": "exact_value", "oracle_strength": "strong", "evidence_summary": "exact value assertion", "relation_reason": "direct_owner_call", "relation_confidence": "high"}
        ],
        "recommendation": {
          "action": "write_targeted_test",
          "reason": "extend the nearest related test",
          "recommended_test": null,
          "nearest_test_to_imitate": {"name": "below_threshold_has_no_discount", "file": "tests/pricing.rs", "line": 10, "oracle_kind": "exact_value", "oracle_strength": "strong", "evidence_summary": "exact value assertion", "oracle_semantics": {"observes": "exact value", "missing": "none", "upgrade_suggestion": false}, "relation_reason": "direct_owner_call", "relation_confidence": "high"},
          "candidate_values": [],
          "assertion_shape": null,
          "verify_command": null
        },
        "actionability": {
          "class": "actionable_related_test_extension",
          "reason": "extend related test",
          "has_concrete_guidance": true,
          "signals": {"missing_discriminator": false, "candidate_value": false, "assertion_shape": false, "related_test": true, "recommended_test_target": false, "verification_command": false}
        },
        "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
        "static_limitations": []
      }
    }
  ]
}
"#,
    );

    let mut violations = Vec::new();
    super::super::validate_evidence_record_contract_fixture_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("related_tests[0].oracle_semantics must be an object"));
    assert!(report.contains(
            "recommendation.nearest_test_to_imitate.oracle_semantics.upgrade_suggestion must be string or null"
        ));
    Ok(())
}

#[test]
fn evidence_quality_benchmark_corpus_is_valid() -> Result<(), String> {
    let corpus = evidence_quality_benchmark_corpus_path()?;
    let mut violations = Vec::new();
    super::super::validate_evidence_quality_benchmark_fixture_corpus_at(&corpus, &mut violations)?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn evidence_quality_benchmark_requires_all_case_kinds() -> Result<(), String> {
    let mut corpus = evidence_quality_benchmark_corpus_value()?;
    corpus["required_case_kinds"] = serde_json::json!(["positive"]);

    let report = evidence_quality_benchmark_violations(&corpus).join("\n");

    assert!(report.contains("required_case_kinds is missing negative_guard"));
    assert!(report.contains("required_case_kinds is missing calibration"));
    Ok(())
}

#[test]
fn evidence_quality_benchmark_reports_contract_drift() {
    let corpus = serde_json::json!({
        "kind": "wrong",
        "schema_version": "0.2",
        "spec": "RIPR-SPEC-9999",
        "evidence_classes": ["duplicate_canonical_gap"],
        "required_case_kinds": ["positive"],
        "cases": [
            {
                "id": "bad",
                "case_kind": "surprise",
                "evidence_class": "unknown",
                "maturity_scope": "global",
                "must_not_claim": [],
                "expected_repo_exposure": {},
                "expected_audit_signal": null
            },
            {
                "id": "bad",
                "case_kind": "metamorphic_line_movement",
                "evidence_class": "match_arm_discriminator_split",
                "maturity_scope": "fixture_backed",
                "description": "bad line movement",
                "fixture_reference": "fixture",
                "expected_claim": "claim",
                "must_not_claim": ["guard"],
                "repair_route": "analysis/canonical-gap-overcount-regression",
                "expected_repo_exposure": {
                    "static_gap_created": true,
                    "evidence_record": {}
                },
                "expected_audit_signal": {},
                "metamorphic": {
                    "before": {
                        "canonical_gap_id": "gap:a",
                        "line": 10
                    },
                    "after": {
                        "canonical_gap_id": "gap:b",
                        "line": 10
                    }
                }
            }
        ]
    });

    let report = evidence_quality_benchmark_violations(&corpus).join("\n");

    assert!(report.contains("kind must be lane1_evidence_quality_benchmark_corpus"));
    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("spec must be RIPR-SPEC-0035"));
    assert!(report.contains("evidence_classes is missing runtime_only_signal"));
    assert!(report.contains("required_case_kinds is missing calibration"));
    assert!(report.contains("is missing capability_scope object"));
    assert!(report.contains("is missing calibration_scope object"));
    assert!(report.contains("is missing audit_expectations object"));
    assert!(report.contains("cases collection has duplicate id bad"));
    assert!(report.contains("unsupported evidence_class unknown"));
    assert!(report.contains("unsupported case_kind surprise"));
    assert!(report.contains("unsupported maturity_scope global"));
    assert!(report.contains("bad is missing string field description"));
    assert!(report.contains("bad is missing string field fixture_reference"));
    assert!(report.contains("bad is missing string field expected_claim"));
    assert!(report.contains("bad is missing string field repair_route"));
    assert!(report.contains("expected_repo_exposure.evidence_record must be an object"));
    assert!(report.contains("is missing expected_audit_signal object"));
    assert!(report.contains("must preserve canonical_gap_id"));
    assert!(report.contains("must move line numbers"));
    assert!(report.contains("must include a runtime-only nonstatic guard"));
}

#[test]
fn evidence_quality_benchmark_reports_missing_must_not_claims() -> Result<(), String> {
    let mut corpus = evidence_quality_benchmark_corpus_value()?;
    let case = evidence_quality_benchmark_case_mut(&mut corpus, "broad_vs_exact_error_oracle")?;
    case["must_not_claim"] = serde_json::json!([]);

    let report = evidence_quality_benchmark_violations(&corpus).join("\n");

    assert!(report.contains(
            "Lane 1 evidence-quality case broad_vs_exact_error_oracle must_not_claim must be a non-empty string array"
        ));
    Ok(())
}

#[test]
fn evidence_quality_benchmark_requires_verify_command_for_actionable_cases() -> Result<(), String> {
    let mut corpus = evidence_quality_benchmark_corpus_value()?;
    let case = evidence_quality_benchmark_case_mut(
        &mut corpus,
        "config_policy_rendered_label_unobserved",
    )?;
    case["expected_repo_exposure"]["evidence_record"]["verify_command"] =
        serde_json::json!("verify_command_unknown");

    let report = evidence_quality_benchmark_violations(&corpus).join("\n");

    assert!(report.contains(
            "actionable case config_policy_rendered_label_unobserved must include a concrete expected_repo_exposure.evidence_record.verify_command"
        ));
    Ok(())
}

#[test]
fn evidence_quality_benchmark_requires_static_limitation_category_at_case_level()
-> Result<(), String> {
    let mut corpus = evidence_quality_benchmark_corpus_value()?;
    let case = evidence_quality_benchmark_case_mut(
        &mut corpus,
        "presentation_text_constant_visibility_unknown",
    )?;
    case["static_limitation_category"] = serde_json::Value::Null;
    case["expected_repo_exposure"]["evidence_record"]["static_limitation"]["category"] =
        serde_json::json!("presentation_text_visibility_unknown");

    let report = evidence_quality_benchmark_violations(&corpus).join("\n");

    assert!(report.contains(
            "static-limitation case presentation_text_constant_visibility_unknown is missing static_limitation_category"
        ));
    assert!(report.contains(
            "static-limitation case presentation_text_constant_visibility_unknown must keep static_limitation_category at the case level"
        ));
    Ok(())
}

#[test]
fn evidence_quality_benchmark_keeps_runtime_only_signal_nonstatic() -> Result<(), String> {
    let mut corpus = evidence_quality_benchmark_corpus_value()?;
    let case = evidence_quality_benchmark_case_mut(&mut corpus, "runtime_only_signal_nonstatic")?;
    case["expected_repo_exposure"]["static_gap_created"] = serde_json::json!(true);
    case["calibration"]["runtime_signal"] = serde_json::json!(false);

    let report = evidence_quality_benchmark_violations(&corpus).join("\n");

    assert!(report.contains(
            "runtime-only case runtime_only_signal_nonstatic must keep static_gap_created=false and evidence_record=null"
        ));
    assert!(report.contains(
        "runtime-only case runtime_only_signal_nonstatic must set calibration.runtime_signal=true"
    ));
    Ok(())
}

#[test]
fn evidence_quality_benchmark_keeps_ambiguous_runtime_join_ambiguous() -> Result<(), String> {
    let mut corpus = evidence_quality_benchmark_corpus_value()?;
    let case =
        evidence_quality_benchmark_case_mut(&mut corpus, "ambiguous_runtime_join_stays_ambiguous")?;
    case["calibration"]["join_status"] = serde_json::json!("matched");

    let report = evidence_quality_benchmark_violations(&corpus).join("\n");

    assert!(report.contains(
            "ambiguous join case ambiguous_runtime_join_stays_ambiguous must set calibration.join_status=ambiguous"
        ));
    Ok(())
}

#[test]
fn evidence_quality_benchmark_pins_line_movement_identity() -> Result<(), String> {
    let mut corpus = evidence_quality_benchmark_corpus_value()?;
    let case =
        evidence_quality_benchmark_case_mut(&mut corpus, "match_arm_discriminator_line_movement")?;
    case["metamorphic"]["after"]["canonical_gap_id"] = serde_json::json!("gap:changed");

    let report = evidence_quality_benchmark_violations(&corpus).join("\n");

    assert!(report.contains(
        "line-movement case match_arm_discriminator_line_movement must preserve canonical_gap_id"
    ));
    Ok(())
}

#[test]
fn evidence_quality_benchmark_is_manifest_only_fixture_dir() -> Result<(), String> {
    assert!(super::super::is_manifest_only_fixture_dir(Path::new(
        "fixtures/evidence-quality-benchmark"
    )));
    assert!(super::super::is_manifest_only_fixture_dir(Path::new(
        "fixtures/gap-decision-ledger"
    )));
    assert!(super::super::is_manifest_only_fixture_dir(Path::new(
        "fixtures/typescript-preview-repair-loop"
    )));
    assert!(super::super::is_manifest_only_fixture_dir(Path::new(
        "fixtures/perl_lsp_facts_exporter"
    )));
    assert!(!super::super::is_manifest_only_fixture_dir(Path::new(
        "fixtures/boundary_gap"
    )));
    let violations = super::super::fixture_contract_violations(Path::new(
        "fixtures/evidence-quality-benchmark",
    ))?;
    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn evidence_health_build_timeout_writes_named_limitation_reports() -> Result<(), String> {
    with_temp_cwd("evidence-health-build-timeout", |_root| {
        let timeout = Duration::from_millis(7);
        let args = evidence_health_args();
        let stale_json = Path::new("target/ripr/reports/evidence-health.json");
        let stale_md = Path::new("target/ripr/reports/evidence-health.md");
        write(stale_json, "stale json");
        write(stale_md, "stale markdown");

        write_evidence_health_report_with_runners(
            Path::new("ripr"),
            &args,
            timeout,
            |_timeout| {
                Ok(TimedOutput {
                    status: None,
                    stdout: "partial build stdout".to_string(),
                    stderr: "Compiling ripr\n".to_string(),
                    duration: Duration::from_millis(8),
                    timed_out: true,
                })
            },
            |_binary, _args, _timeout| {
                Err("evidence-health generation should not run after build timeout".to_string())
            },
        )?;

        let json_text = fs::read_to_string(stale_json).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["schema_version"], "0.2");
        assert_eq!(value["status"], "warn");
        assert_eq!(
            value["run_limitations"][0]["category"],
            "evidence_health_timeout"
        );
        assert_eq!(
            value["run_limitations"][0]["phase"],
            "evidence_health_build"
        );
        assert_eq!(
            value["run_limitations"][0]["command"],
            "cargo build -p ripr"
        );
        assert_eq!(
            value["inputs"]["generation"]["command"],
            "cargo build -p ripr"
        );
        assert_eq!(
            value["inputs"]["generation"]["phase"],
            "evidence_health_build"
        );
        assert_eq!(
            value["inputs"]["generation"]["status"],
            serde_json::Value::from("timeout")
        );
        assert_eq!(
            value["inputs"]["generation"]["stderr_excerpt"],
            "Compiling ripr"
        );
        assert!(!json_text.contains("stale json"));

        let markdown = fs::read_to_string(stale_md).map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("evidence_health_timeout"));
        assert!(markdown.contains("evidence_health_build"));
        assert!(markdown.contains("cargo build -p ripr"));
        assert!(!markdown.contains("stale markdown"));
        Ok(())
    })
}

#[test]
fn evidence_health_build_runner_error_writes_named_limitation_reports() -> Result<(), String> {
    with_temp_cwd("evidence-health-build-runner-error", |_root| {
        let timeout = Duration::from_millis(7);
        let args = evidence_health_args();
        let stale_json = Path::new("target/ripr/reports/evidence-health.json");
        let stale_md = Path::new("target/ripr/reports/evidence-health.md");
        write(stale_json, "stale json");
        write(stale_md, "stale markdown");

        write_evidence_health_report_with_runners(
            Path::new("ripr"),
            &args,
            timeout,
            |_timeout| Err("failed to spawn cargo build".to_string()),
            |_binary, _args, _timeout| {
                Err(
                    "evidence-health generation should not run after build runner error"
                        .to_string(),
                )
            },
        )?;

        let json_text = fs::read_to_string(stale_json).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["schema_version"], "0.2");
        assert_eq!(value["status"], "warn");
        assert_eq!(
            value["run_limitations"][0]["category"],
            "evidence_health_runner_error"
        );
        assert_eq!(
            value["run_limitations"][0]["phase"],
            "evidence_health_build"
        );
        assert_eq!(
            value["inputs"]["generation"]["status"],
            serde_json::Value::from("runner_error")
        );
        assert!(
            value["inputs"]["generation"]["failure_reason"]
                .as_str()
                .unwrap_or_default()
                .contains("failed to spawn cargo build")
        );
        assert!(!json_text.contains("stale json"));

        let markdown = fs::read_to_string(stale_md).map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("evidence_health_runner_error"));
        assert!(markdown.contains("failed to spawn cargo build"));
        assert!(!markdown.contains("stale markdown"));
        Ok(())
    })
}

#[test]
fn evidence_health_timeout_writes_named_limitation_reports() -> Result<(), String> {
    with_temp_cwd("evidence-health-timeout", |_root| {
        let timeout = Duration::from_millis(7);
        let args = evidence_health_args();
        let stale_json = Path::new("target/ripr/reports/evidence-health.json");
        let stale_md = Path::new("target/ripr/reports/evidence-health.md");
        write(stale_json, "stale json");
        write(stale_md, "stale markdown");

        write_evidence_health_report_with_runner(
            Path::new("ripr"),
            &args,
            timeout,
            |_binary, _args, _timeout| {
                Ok(TimedOutput {
                        status: None,
                        stdout: "partial stdout".to_string(),
                        stderr: concat!(
                            "ripr_repo_exposure_latency phase=inventory_seams status=ok duration_ms=3\n",
                            "ripr_repo_exposure_latency phase=evidence_for_seams_progress status=processed_500_of_39021 duration_ms=7\n",
                        )
                        .to_string(),
                        duration: Duration::from_millis(8),
                        timed_out: true,
                    })
            },
        )?;

        let json_text = fs::read_to_string(stale_json).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["schema_version"], "0.2");
        assert_eq!(value["status"], "warn");
        assert_eq!(
            value["run_limitations"][0]["category"],
            "evidence_health_timeout"
        );
        assert_eq!(
            value["inputs"]["generation"]["phase"],
            "evidence_health_generation"
        );
        assert_eq!(
            value["inputs"]["generation"]["status"],
            serde_json::Value::from("timeout")
        );
        assert_eq!(
            value["run_limitations"][0]["stderr_excerpt"],
            concat!(
                "ripr_repo_exposure_latency phase=inventory_seams status=ok duration_ms=3\n",
                "ripr_repo_exposure_latency phase=evidence_for_seams_progress status=processed_500_of_39021 duration_ms=7"
            )
        );
        assert_eq!(
            value["inputs"]["generation"]["latency_trace_events_total"],
            2
        );
        assert_eq!(
            value["inputs"]["generation"]["latency_trace_tail"][1]["phase"],
            "evidence_for_seams_progress"
        );
        assert_eq!(
            value["inputs"]["generation"]["latency_trace_tail"][1]["status"],
            "processed_500_of_39021"
        );
        assert_eq!(value["run_limitations"][0]["latency_trace_events_total"], 2);
        assert_eq!(
            value["run_limitations"][0]["latency_trace_tail"][0]["phase"],
            "inventory_seams"
        );
        assert_eq!(
            value["top_static_limitations"][0]["repair_route"],
            "inspect evidence-health runtime, increase RIPR_EVIDENCE_HEALTH_TIMEOUT_MS for slower machines, or add a narrower fixture-backed analyzer path"
        );
        assert!(!json_text.contains("stale json"));

        let markdown = fs::read_to_string(stale_md).map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("evidence_health_timeout"));
        assert!(markdown.contains("RIPR_EVIDENCE_HEALTH_TIMEOUT_MS"));
        assert!(markdown.contains("Repo-exposure Latency Trace Tail"));
        assert!(markdown.contains("processed_500_of_39021"));
        assert!(!markdown.contains("stale markdown"));
        Ok(())
    })
}

#[test]
fn evidence_health_generation_runner_error_writes_named_limitation_reports() -> Result<(), String> {
    with_temp_cwd("evidence-health-generation-runner-error", |_root| {
        let timeout = Duration::from_millis(7);
        let args = evidence_health_args();
        let stale_json = Path::new("target/ripr/reports/evidence-health.json");
        let stale_md = Path::new("target/ripr/reports/evidence-health.md");
        write(stale_json, "stale json");
        write(stale_md, "stale markdown");

        write_evidence_health_report_with_runner(
            Path::new("ripr"),
            &args,
            timeout,
            |_binary, _args, _timeout| {
                Err("failed to read evidence-health child stdout".to_string())
            },
        )?;

        let json_text = fs::read_to_string(stale_json).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["schema_version"], "0.2");
        assert_eq!(value["status"], "warn");
        assert_eq!(
            value["run_limitations"][0]["category"],
            "evidence_health_runner_error"
        );
        assert_eq!(
            value["run_limitations"][0]["phase"],
            "evidence_health_generation"
        );
        assert_eq!(
            value["inputs"]["generation"]["status"],
            serde_json::Value::from("runner_error")
        );
        assert!(
            value["inputs"]["generation"]["failure_reason"]
                .as_str()
                .unwrap_or_default()
                .contains("failed to read evidence-health child stdout")
        );
        assert!(!json_text.contains("stale json"));

        let markdown = fs::read_to_string(stale_md).map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("evidence_health_runner_error"));
        assert!(markdown.contains("failed to read evidence-health child stdout"));
        assert!(!markdown.contains("stale markdown"));
        Ok(())
    })
}

#[test]
fn evidence_health_default_timeout_is_bounded_for_live_repo_pathologies() {
    assert_eq!(super::super::EVIDENCE_HEALTH_DEFAULT_TIMEOUT_MS, 240_000);
}

#[test]
fn evidence_health_child_envs_enable_latency_trace() {
    assert_eq!(
        super::super::evidence_health_child_envs(),
        [(super::super::REPO_EXPOSURE_LATENCY_TRACE_ENV, "1")]
    );
}

#[test]
fn evidence_health_output_excerpt_is_bounded() -> Result<(), String> {
    let text = (0..12)
        .map(|line| format!("line-{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        super::super::evidence_health_output_excerpt(&text).as_deref(),
        Some("line-4\nline-5\nline-6\nline-7\nline-8\nline-9\nline-10\nline-11")
    );

    let long_line = "x".repeat(super::super::EVIDENCE_HEALTH_OUTPUT_EXCERPT_CHARS + 8);
    let excerpt = super::super::evidence_health_output_excerpt(&long_line)
        .ok_or_else(|| "non-empty long line should produce excerpt".to_string())?;
    assert!(excerpt.starts_with("[truncated]\n"));
    assert!(excerpt.chars().count() <= super::super::EVIDENCE_HEALTH_OUTPUT_EXCERPT_CHARS + 12);
    Ok(())
}

#[test]
fn evidence_health_success_accepts_complete_report_artifacts() -> Result<(), String> {
    with_temp_cwd("evidence-health-success-complete", |_root| {
        let timeout = Duration::from_secs(30);
        let args = evidence_health_args();
        write_evidence_health_report_with_runner(
            Path::new("ripr"),
            &args,
            timeout,
            |_binary, _args, _timeout| {
                write(
                    Path::new("target/ripr/reports/evidence-health.json"),
                    &complete_evidence_health_json_fixture(),
                );
                write(
                    Path::new("target/ripr/reports/evidence-health.md"),
                    "# RIPR evidence health report\n\n| Field | Value |\n| --- | --- |\n| Status | advisory |\n",
                );
                Ok(TimedOutput {
                    status: Some(success_exit_status()),
                    stdout: "ok".to_string(),
                    stderr: String::new(),
                    duration: Duration::from_secs(1),
                    timed_out: false,
                })
            },
        )?;

        let json_text = fs::read_to_string("target/ripr/reports/evidence-health.json")
            .map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["status"], "advisory");
        assert!(value["metrics"]["grip_class_counts"].is_object());
        assert!(value["evidence_quality"]["movement_availability"].is_object());
        assert!(value["top_static_limitations"].is_array());
        let markdown = fs::read_to_string("target/ripr/reports/evidence-health.md")
            .map_err(|err| err.to_string())?;
        assert!(markdown.contains("| Status | advisory |"));
        Ok(())
    })
}

#[test]
fn evidence_health_success_with_status_only_stub_writes_named_limitation_reports()
-> Result<(), String> {
    with_temp_cwd("evidence-health-success-stub", |_root| {
        let timeout = Duration::from_secs(30);
        let args = evidence_health_args();
        write_evidence_health_report_with_runner(
            Path::new("ripr"),
            &args,
            timeout,
            |_binary, _args, _timeout| {
                write(
                    Path::new("target/ripr/reports/evidence-health.json"),
                    r#"{"schema_version":"0.2","status":"advisory"}"#,
                );
                write(
                    Path::new("target/ripr/reports/evidence-health.md"),
                    "# RIPR evidence health report\n\nStatus: advisory\n",
                );
                Ok(TimedOutput {
                    status: Some(success_exit_status()),
                    stdout: "ok".to_string(),
                    stderr: String::new(),
                    duration: Duration::from_secs(1),
                    timed_out: false,
                })
            },
        )?;

        let json_text = fs::read_to_string("target/ripr/reports/evidence-health.json")
            .map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["status"], "warn");
        assert_eq!(
            value["inputs"]["generation"]["status"],
            serde_json::Value::from("pass_incomplete")
        );
        assert!(
            value["run_limitations"][0]["failure_reason"]
                .as_str()
                .unwrap_or_default()
                .contains("is missing `inputs`")
        );
        let markdown = fs::read_to_string("target/ripr/reports/evidence-health.md")
            .map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("evidence_health_incomplete"));
        Ok(())
    })
}

#[test]
fn evidence_health_success_without_artifacts_writes_named_limitation_reports() -> Result<(), String>
{
    with_temp_cwd("evidence-health-success-missing", |_root| {
        let timeout = Duration::from_secs(30);
        let args = evidence_health_args();
        let stale_json = Path::new("target/ripr/reports/evidence-health.json");
        let stale_md = Path::new("target/ripr/reports/evidence-health.md");
        write(
            stale_json,
            r#"{"schema_version":"0.2","status":"advisory","marker":"stale"}"#,
        );
        write(stale_md, "Status: advisory\nstale markdown");

        write_evidence_health_report_with_runner(
            Path::new("ripr"),
            &args,
            timeout,
            |_binary, _args, _timeout| {
                Ok(TimedOutput {
                    status: Some(success_exit_status()),
                    stdout: "completed without files".to_string(),
                    stderr: String::new(),
                    duration: Duration::from_secs(2),
                    timed_out: false,
                })
            },
        )?;

        let json_text = fs::read_to_string(stale_json).map_err(|err| err.to_string())?;
        assert!(!json_text.contains("marker"));
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["status"], "warn");
        assert_eq!(
            value["run_limitations"][0]["category"],
            "evidence_health_incomplete"
        );
        assert_eq!(
            value["inputs"]["generation"]["status"],
            serde_json::Value::from("pass_incomplete")
        );
        assert!(
            value["inputs"]["generation"]["failure_reason"]
                .as_str()
                .unwrap_or_default()
                .contains("failed to read evidence-health JSON artifact")
        );

        let markdown = fs::read_to_string(stale_md).map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("evidence_health_incomplete"));
        assert!(markdown.contains("Failure reason"));
        assert!(!markdown.contains("stale markdown"));
        Ok(())
    })
}

#[test]
fn evidence_health_report_artifact_completion_validator_names_bad_shapes() -> Result<(), String> {
    with_temp_cwd("evidence-health-artifact-completion", |_root| {
        let json_path = Path::new("target/ripr/reports/evidence-health.json");
        let md_path = Path::new("target/ripr/reports/evidence-health.md");
        write(md_path, "Status: advisory\n");

        let cases = [
            ("not json", "failed to parse evidence-health JSON artifact"),
            (
                r#"{"schema_version":"0.1","status":"advisory"}"#,
                "did not use schema_version 0.2",
            ),
            (
                r#"{"schema_version":"0.2","status":"warn"}"#,
                "did not report advisory status",
            ),
            (
                r#"{"schema_version":"0.2","status":"advisory"}"#,
                "is missing `inputs`",
            ),
            (
                r#"{"schema_version":"0.2","status":"advisory","inputs":{},"metrics":{},"evidence_quality":{},"calibration":{},"top_static_limitations":[]}"#,
                "is missing `inputs.root`",
            ),
        ];
        for (json, expected) in cases {
            write(json_path, json);
            let err = super::super::evidence_health_report_artifacts_are_complete()
                .expect_err("bad JSON artifact shape should be rejected");
            assert!(err.contains(expected), "expected {expected:?} in {err:?}");
        }

        write(json_path, &complete_evidence_health_json_fixture());
        if let Ok(()) = fs::remove_file(md_path) {}
        let err = super::super::evidence_health_report_artifacts_are_complete()
            .expect_err("missing Markdown artifact should be rejected");
        assert!(err.contains("failed to read evidence-health Markdown artifact"));

        write(md_path, "");
        let err = super::super::evidence_health_report_artifacts_are_complete()
            .expect_err("empty Markdown artifact should be rejected");
        assert!(err.contains("was empty"));

        write(md_path, "Status: warn\n");
        let err = super::super::evidence_health_report_artifacts_are_complete()
            .expect_err("warning Markdown artifact should be rejected as incomplete success");
        assert!(err.contains("did not report advisory status"));

        write(json_path, &complete_evidence_health_json_fixture());
        write(md_path, "Status: advisory\n");
        super::super::evidence_health_report_artifacts_are_complete()?;
        Ok(())
    })
}

#[test]
fn evidence_health_incomplete_exit_writes_named_limitation_reports() -> Result<(), String> {
    with_temp_cwd("evidence-health-incomplete", |_root| {
        let timeout = Duration::from_mins(30);
        let args = evidence_health_args();
        let stale_json = Path::new("target/ripr/reports/evidence-health.json");
        let stale_md = Path::new("target/ripr/reports/evidence-health.md");
        write(stale_json, "stale json");
        write(stale_md, "stale markdown");

        write_evidence_health_report_with_runner(
            Path::new("ripr"),
            &args,
            timeout,
            |_binary, _args, _timeout| {
                Ok(TimedOutput {
                    status: None,
                    stdout: "partial stdout".to_string(),
                    stderr: "phase=inventory\nphase=evidence_health\n".to_string(),
                    duration: Duration::from_secs(377),
                    timed_out: false,
                })
            },
        )?;

        let json_text = fs::read_to_string(stale_json).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["schema_version"], "0.2");
        assert_eq!(value["status"], "warn");
        assert_eq!(
            value["run_limitations"][0]["category"],
            "evidence_health_incomplete"
        );
        assert_eq!(
            value["evidence_quality"]["static_limitation_category_counts"]["evidence_health_incomplete"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            value["inputs"]["generation"]["status"],
            serde_json::Value::from("fail")
        );
        assert_eq!(
            value["inputs"]["generation"]["phase"],
            "evidence_health_generation"
        );
        assert_eq!(
            value["inputs"]["generation"]["stdout_excerpt"],
            "partial stdout"
        );
        assert!(!json_text.contains("stale json"));

        let markdown = fs::read_to_string(stale_md).map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("evidence_health_incomplete"));
        assert!(markdown.contains("stdout/stderr"));
        assert!(!markdown.contains("stale markdown"));
        Ok(())
    })
}

#[test]
fn evidence_health_nonzero_exit_writes_named_limitation_reports() -> Result<(), String> {
    with_temp_cwd("evidence-health-nonzero", |_root| {
        let timeout = Duration::from_secs(90);
        let args = evidence_health_args();
        let stale_json = Path::new("target/ripr/reports/evidence-health.json");
        let stale_md = Path::new("target/ripr/reports/evidence-health.md");
        write(stale_json, "stale json");
        write(stale_md, "stale markdown");

        write_evidence_health_report_with_runner(
            Path::new("ripr"),
            &args,
            timeout,
            |_binary, _args, _timeout| {
                Ok(TimedOutput {
                    status: Some(failure_exit_status()),
                    stdout: "partial stdout".to_string(),
                    stderr: "error: evidence health failed\n".to_string(),
                    duration: Duration::from_secs(12),
                    timed_out: false,
                })
            },
        )?;

        let json_text = fs::read_to_string(stale_json).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["schema_version"], "0.2");
        assert_eq!(value["status"], "warn");
        assert_eq!(
            value["run_limitations"][0]["category"],
            "evidence_health_incomplete"
        );
        assert_eq!(
            value["run_limitations"][0]["exit_code"].as_i64(),
            failure_exit_status().code().map(i64::from)
        );
        assert_eq!(
            value["inputs"]["generation"]["exit_code"].as_i64(),
            failure_exit_status().code().map(i64::from)
        );
        assert_eq!(
            value["inputs"]["generation"]["status"],
            serde_json::Value::from("fail")
        );
        assert_eq!(
            value["inputs"]["generation"]["phase"],
            "evidence_health_generation"
        );
        assert_eq!(
            value["inputs"]["generation"]["stderr_excerpt"],
            "error: evidence health failed"
        );
        assert_eq!(
            value["run_limitations"][0]["stderr_excerpt"],
            "error: evidence health failed"
        );
        assert!(!json_text.contains("stale json"));

        let markdown = fs::read_to_string(stale_md).map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("evidence_health_incomplete"));
        assert!(markdown.contains("error: evidence health failed"));
        assert!(!markdown.contains("stale markdown"));
        Ok(())
    })
}

#[test]
fn evidence_class_work_queue_prefers_dominant_live_repair_signal() -> Result<(), String> {
    let rows = vec![
        super::super::Lane1EvidenceAuditAlignmentClassCoverage {
            evidence_class: "predicate_boundary".to_string(),
            raw_findings: 20,
            canonical_items: 10,
            aligned_raw_findings: 20,
            unaligned_raw_findings: 0,
            actionable_items: 3,
            already_observed_items: 0,
            internal_no_action_items: 0,
            static_limitation_items: 0,
            static_limitation_categories: BTreeMap::new(),
            static_limitation_repair_routes: BTreeMap::new(),
            unknown_items: 0,
        },
        super::super::Lane1EvidenceAuditAlignmentClassCoverage {
            evidence_class: "call_presence".to_string(),
            raw_findings: 30,
            canonical_items: 25,
            aligned_raw_findings: 30,
            unaligned_raw_findings: 0,
            actionable_items: 0,
            already_observed_items: 0,
            internal_no_action_items: 0,
            static_limitation_items: 8,
            static_limitation_categories: BTreeMap::from([(
                "activation_owner_call_unresolved".to_string(),
                8,
            )]),
            static_limitation_repair_routes: BTreeMap::from([(
                "analysis/related-test-ranking-audit-fixes".to_string(),
                8,
            )]),
            unknown_items: 0,
        },
    ];

    let queue = super::super::audit_evidence_class_work_queue(&rows);
    let top = queue
        .first()
        .ok_or_else(|| "work queue should contain scored rows".to_string())?;
    assert_eq!(top.evidence_class, "call_presence");
    assert_eq!(top.dominant_signal, "static_limitations");
    assert_eq!(
        top.dominant_static_limitation_category.as_deref(),
        Some("activation_owner_call_unresolved")
    );
    assert_eq!(top.dominant_static_limitation_category_count, 8);
    assert_eq!(
        top.dominant_static_limitation_repair_route.as_deref(),
        Some("analysis/related-test-ranking-audit-fixes")
    );
    assert_eq!(top.next_repair, "analysis/related-test-ranking-audit-fixes");
    assert!(
        queue
            .iter()
            .any(|row| row.evidence_class == "predicate_boundary"
                && row.dominant_signal == "actionable_canonical_gaps"),
        "actionable classes should remain visible even when static limitations dominate"
    );
    Ok(())
}

#[test]
fn evidence_quality_scorecard_renders_required_json_sections() -> Result<(), String> {
    let audit = lane1_scorecard_sample_audit_value()?;
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["report"], "evidence-quality-scorecard");
    assert!(value.get("inputs").is_some());
    assert!(value.get("headline").is_some());
    assert!(value.get("summary").is_some());
    assert!(value.get("maturity_by_class").is_some());
    assert!(value.get("canonical_gap_groups").is_some());
    assert!(value.get("duplicate_looking_groups").is_some());
    assert!(value.get("static_limitation_categories").is_some());
    assert!(
        value["static_limitation_categories"]
            .get("by_category")
            .is_some()
    );
    assert!(
        value["static_limitation_categories"]
            .get("repair_routes")
            .is_some()
    );
    assert!(value.get("missing_discriminator_classes").is_some());
    assert!(value.get("related_test_confidence").is_some());
    assert!(value.get("oracle_semantics_distribution").is_some());
    assert!(value.get("movement_availability").is_some());
    assert!(value.get("calibration_coverage").is_some());
    assert!(value.get("actionable_gap_top_lists").is_some());
    assert!(
        value
            .get("actionable_gap_packet_public_projection")
            .is_some()
    );
    assert!(value.get("evidence_class_work_queue").is_some());
    assert!(
        value
            .get("language_aware_placement_route_quality")
            .is_some()
    );
    assert!(value.get("cross_language_oracle_route_quality").is_some());
    assert!(value.get("recommended_repairs").is_some());
    assert!(value.get("recent_audit_deltas").is_some());
    assert!(value.get("unknowns").is_some());
    assert_eq!(
        value["inputs"]["evidence_health"]["status"],
        serde_json::Value::from("missing")
    );
    assert_eq!(
        value["headline"]["primary_metric"],
        "finding_alignment_actionable_unresolved_canonical_gaps"
    );
    assert_eq!(
        value["headline"]["counting_model"],
        "actionable_canonical_gaps"
    );
    Ok(())
}

#[test]
fn evidence_quality_scorecard_surfaces_limited_inputs_as_unknowns() -> Result<(), String> {
    let mut audit = lane1_scorecard_sample_audit_value()?;
    let audit_object = audit
        .as_object_mut()
        .ok_or_else(|| "sample audit must be an object".to_string())?;
    audit_object.insert(
        "run_limitations".to_string(),
        serde_json::json!([
            {
                "category": "lane1_repo_exposure_timeout",
                "phase": "repo_exposure_generation",
                "repair_route": "inspect repo-exposure latency trace"
            }
        ]),
    );
    audit_object.insert(
        "static_limitations".to_string(),
        serde_json::json!({
            "by_reason": [],
            "by_stage": {},
            "by_category": {"lane1_repo_exposure_timeout": 1},
            "repair_routes": {"report/lane1-audit-bounded-diagnostics": 1}
        }),
    );
    audit_object
        .get_mut("summary")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "sample audit summary must be an object".to_string())?
        .insert("static_limitations_total".to_string(), Value::from(0));
    let evidence_health = serde_json::json!({
        "schema_version": "0.1",
        "status": "warn",
        "run_limitations": [
            {
                "category": "evidence_health_timeout",
                "phase": "evidence_health_generation",
                "repair_route": "inspect evidence-health runtime"
            }
        ]
    });
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        Some(&evidence_health),
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let kinds = value["unknowns"]
        .as_array()
        .ok_or("unknowns must be an array")?
        .iter()
        .filter_map(|unknown| unknown["kind"].as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(value["run_status"], "limited_timeout");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "lane1_repo_exposure_timeout"
    );
    assert!(kinds.contains("lane1_evidence_audit_limited"));
    assert!(kinds.contains("evidence_health_limited"));
    assert_eq!(value["summary"]["static_limitations_total"], 1);
    assert_eq!(value["headline"]["static_limitations"], 1);
    assert_eq!(
        value["static_limitation_categories"]["by_category"]["lane1_repo_exposure_timeout"],
        1
    );
    Ok(())
}

#[test]
fn evidence_quality_scorecard_does_not_treat_cache_store_skip_as_limited_audit()
-> Result<(), String> {
    let mut audit = lane1_scorecard_sample_audit_value()?;
    audit
        .as_object_mut()
        .ok_or_else(|| "sample audit must be an object".to_string())?
        .insert(
            "run_limitations".to_string(),
            serde_json::json!([
                {
                    "category": "lane1_repo_exposure_cache_store_skipped_large_entry",
                    "phase": "repo_exposure_cache_store",
                    "repair_route": "add bounded full-cache serialization"
                }
            ]),
        );
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let kinds = value["unknowns"]
        .as_array()
        .ok_or("unknowns must be an array")?
        .iter()
        .filter_map(|unknown| unknown["kind"].as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(value["run_status"], "limited_large_cache_skip");
    assert_eq!(value["runtime_status"]["downstream_consumable"], true);
    assert!(!kinds.contains("lane1_evidence_audit_limited"));
    assert!(kinds.contains("evidence_health_unavailable"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_treats_large_cache_preflight_as_limited() -> Result<(), String> {
    let mut audit = lane1_scorecard_sample_audit_value()?;
    audit
        .as_object_mut()
        .ok_or_else(|| "sample audit must be an object".to_string())?
        .insert(
            "run_limitations".to_string(),
            serde_json::json!([
                {
                    "category": "lane1_repo_exposure_large_cache_preflight_skip",
                    "phase": "repo_seam_facts_cache",
                    "input": "target/ripr/cache",
                    "repair_route": "cargo xtask cache report && cargo xtask cache gc --dry-run",
                    "downstream_consumable": false
                }
            ]),
        );
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let kinds = value["unknowns"]
        .as_array()
        .ok_or("unknowns must be an array")?
        .iter()
        .filter_map(|unknown| unknown["kind"].as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(value["run_status"], "limited_large_cache_skip");
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    assert!(kinds.contains("lane1_evidence_audit_limited"));
    assert!(kinds.contains("evidence_health_unavailable"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_treats_incomplete_audit_as_limited() -> Result<(), String> {
    let mut audit = lane1_scorecard_sample_audit_value()?;
    audit
        .as_object_mut()
        .ok_or_else(|| "sample audit must be an object".to_string())?
        .insert(
            "run_limitations".to_string(),
            serde_json::json!([
                {
                    "category": "lane1_repo_exposure_incomplete",
                    "phase": "repo_exposure_generation",
                    "repair_route": "inspect repo-exposure generation"
                }
            ]),
        );
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let kinds = value["unknowns"]
        .as_array()
        .ok_or("unknowns must be an array")?
        .iter()
        .filter_map(|unknown| unknown["kind"].as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "lane1_repo_exposure_incomplete"
    );
    assert!(kinds.contains("lane1_evidence_audit_limited"));
    assert!(kinds.contains("evidence_health_unavailable"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_names_audit_regeneration_failure() -> Result<(), String> {
    let audit =
        evidence_quality_scorecard_audit_regeneration_failure_audit("repo exposure failed")?;
    assert_eq!(
        audit["run_limitations"][0]["category"],
        EVIDENCE_QUALITY_SCORECARD_AUDIT_REGENERATION_FAILED
    );
    assert_eq!(
        audit["static_limitations"]["by_category"]
            [EVIDENCE_QUALITY_SCORECARD_AUDIT_REGENERATION_FAILED],
        serde_json::Value::from(1)
    );
    assert_eq!(
        audit["summary"]["static_limitations_total"],
        serde_json::Value::from(1)
    );

    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let kinds = value["unknowns"]
        .as_array()
        .ok_or("unknowns must be an array")?
        .iter()
        .filter_map(|unknown| unknown["kind"].as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        EVIDENCE_QUALITY_SCORECARD_AUDIT_REGENERATION_FAILED
    );
    assert!(kinds.contains("lane1_evidence_audit_limited"));
    assert!(kinds.contains(EVIDENCE_QUALITY_SCORECARD_AUDIT_REGENERATION_FAILED));
    assert_eq!(
        value["headline"]["primary_metric"],
        "finding_alignment_actionable_unresolved_canonical_gaps"
    );
    assert_eq!(
        value["headline"]["primary_count"],
        serde_json::Value::from(0)
    );
    Ok(())
}

#[test]
fn evidence_quality_scorecard_headline_prefers_actionable_canonical_gaps() -> Result<(), String> {
    let mut audit = scorecard_minimal_audit_value(0, 0, 0, 0, 0);
    audit
        .as_object_mut()
        .ok_or_else(|| "sample audit must be an object".to_string())?
        .insert(
            "finding_alignment".to_string(),
            serde_json::json!({
                "summary": {
                    "raw_signals": 7,
                    "canonical_items": 3,
                    "aligned_raw_findings": 7,
                    "unaligned_raw_findings": 0,
                    "duplicate_groups_total": 2,
                    "actionable_gaps": 2,
                    "already_observed": 1,
                    "internal_no_action": 0,
                    "static_limitations": 0,
                    "unknown": 0,
                    "calibrated_supported": 0,
                    "uncalibrated": 3
                }
            }),
        );
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["headline"]["primary_count"], 2);
    assert_eq!(value["headline"]["raw_signals"], 7);
    assert_eq!(value["headline"]["canonical_items"], 3);
    assert_eq!(value["headline"]["already_observed"], 1);
    assert_eq!(value["headline"]["raw_to_canonical_ratio"], 7.0 / 3.0);
    assert_eq!(
        value["summary"]["finding_alignment_actionable_unresolved_canonical_gaps"],
        2
    );
    Ok(())
}

#[test]
fn evidence_quality_scorecard_defaults_static_limitation_taxonomy_sections() -> Result<(), String> {
    let mut audit = scorecard_minimal_audit_value(0, 0, 0, 0, 0);
    audit
        .as_object_mut()
        .ok_or_else(|| "sample audit must be an object".to_string())?
        .remove("static_limitations");
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    let static_limitations = &value["static_limitation_categories"];
    assert!(static_limitations["by_reason"].is_array());
    assert!(static_limitations["by_stage"].is_object());
    assert!(static_limitations["by_category"].is_object());
    assert!(static_limitations["repair_routes"].is_object());
    Ok(())
}

#[test]
fn evidence_quality_scorecard_markdown_names_required_sections() -> Result<(), String> {
    let audit = lane1_scorecard_sample_audit_value()?;
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let markdown = evidence_quality_scorecard_markdown(&report);

    for needle in [
        "Lane 1 evidence quality scorecard",
        "Actionable canonical gaps",
        "Actionable Canonical Gap Top Lists",
        "Actionable gap class",
        "Repair kind",
        "Actionable Gap Packet Public Projection Readiness",
        "Projection exclusion reason",
        "Language-Aware Placement Route Quality",
        "Cross-Language Oracle Route Quality",
        "Evidence Class Work Queue",
        "Dominant signal",
        "Maturity By Class",
        "Top Evidence-Quality Risks",
        "Recommended Lane 1 Repairs",
        "Duplicate-Looking And Canonical Group Signals",
        "Static Limitations And Missing Discriminators",
        "Static limitation category",
        "Related-Test And Oracle Distributions",
        "Movement And Calibration Coverage",
        "Recent Deltas",
        "Unknowns And Unavailable Inputs",
    ] {
        assert!(
            markdown.contains(needle),
            "scorecard Markdown missing {needle}"
        );
    }
    assert!(markdown.contains("## Runtime Status"));
    assert!(markdown.contains("| State | `full` |"));
    assert!(markdown.contains("| Downstream consumable | `true` |"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_carries_actionable_gap_top_lists_from_audit() -> Result<(), String> {
    let audit = lane1_scorecard_sample_audit_value()?;
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(
        value["actionable_gap_top_lists"]["top_actionable_gap_classes"][0]["label"],
        "predicate_boundary"
    );
    assert_eq!(
        value["actionable_gap_top_lists"]["top_repair_kinds"][0]["label"],
        "add_boundary_assertion"
    );
    assert!(
        value["actionable_gap_top_lists"]["top_static_limitation_reasons"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "scorecard should carry only actionable static-limit reasons from the audit"
    );

    let markdown = evidence_quality_scorecard_markdown(&report);
    assert!(markdown.contains("Actionable Canonical Gap Top Lists"));
    assert!(markdown.contains("add_boundary_assertion"));
    assert!(markdown.contains("No static limitation reason counts were reported."));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_carries_evidence_class_work_queue_from_audit() -> Result<(), String> {
    let audit = lane1_scorecard_sample_audit_value()?;
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(
        value["evidence_class_work_queue"][0]["evidence_class"],
        "predicate_boundary"
    );
    assert_eq!(
        value["evidence_class_work_queue"][0]["dominant_signal"],
        "actionable_canonical_gaps"
    );
    assert_eq!(
        value["evidence_class_work_queue"][0]["next_repair"],
        "dogfood/actionable-gap-repair-loop"
    );
    let call_presence = value["evidence_class_work_queue"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["evidence_class"] == "call_presence")
        })
        .ok_or_else(|| "scorecard should carry call_presence work queue row".to_string())?;
    assert_eq!(
        call_presence["dominant_static_limitation_category"],
        "opaque_helper_call"
    );
    assert_eq!(
        call_presence["dominant_static_limitation_category_count"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        call_presence["dominant_static_limitation_repair_route"],
        "analysis/oracle-semantics-audit-fixes"
    );
    assert_eq!(
        call_presence["next_repair"],
        "analysis/oracle-semantics-audit-fixes"
    );

    let markdown = evidence_quality_scorecard_markdown(&report);
    assert!(markdown.contains("Evidence Class Work Queue"));
    assert!(markdown.contains("actionable_canonical_gaps"));
    assert!(markdown.contains("opaque_helper_call"));
    assert!(markdown.contains("analysis/oracle-semantics-audit-fixes"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_recommended_repairs_follow_evidence_class_work_queue()
-> Result<(), String> {
    let audit = lane1_scorecard_sample_audit_value()?;
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;

    let first = report
        .recommended_repairs
        .first()
        .ok_or_else(|| "expected work-queue recommended repair".to_string())?;
    assert_eq!(first.slice, "dogfood/actionable-gap-repair-loop");
    assert_eq!(first.evidence_class, "predicate_boundary");
    assert_eq!(first.risk_kind, "actionable_canonical_gaps");
    assert_eq!(first.signal_count, 2);
    assert!(
        first
            .why
            .contains("Audit work queue ranks `predicate_boundary`"),
        "work-queue repair should explain the class-specific live signal: {first:?}"
    );

    let call_presence = report
        .recommended_repairs
        .iter()
        .find(|repair| repair.evidence_class == "call_presence")
        .ok_or_else(|| "expected call_presence work-queue repair".to_string())?;
    assert_eq!(call_presence.slice, "analysis/oracle-semantics-audit-fixes");
    assert_eq!(
        call_presence.risk_kind,
        "static_limitations:opaque_helper_call"
    );
    assert_eq!(call_presence.signal_count, 1);
    assert!(call_presence.why.contains("opaque_helper_call"));

    let markdown = evidence_quality_scorecard_markdown(&report);
    assert!(markdown.contains("Audit work queue ranks `predicate_boundary`"));
    assert!(markdown.contains("Audit work queue ranks `call_presence`"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_carries_actionable_packet_projection_readiness() -> Result<(), String>
{
    let mut audit = scorecard_minimal_audit_value(0, 0, 0, 0, 0);
    audit
        .as_object_mut()
        .ok_or_else(|| "audit should be an object".to_string())?
        .insert(
            "finding_alignment".to_string(),
            serde_json::json!({
                "summary": {
                    "raw_signals": 9,
                    "canonical_items": 4,
                    "actionable_gaps": 3
                },
                "coverage": {
                    "static_unknown_without_named_limitation": 0,
                    "canonical_items_without_repair_route": 0,
                    "canonical_items_without_verify_command": 0
                },
                "actionable_gap_packet_public_projection": {
                    "scope": "emitted_actionable_gap_packets",
                    "public_projection_eligible_packets": 2,
                    "public_projection_excluded_packets": 1,
                    "projection_exclusion_reasons": [
                        {"label": "missing_receipt_command", "count": 1}
                    ]
                }
            }),
        );

    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;

    assert_eq!(
        report
            .summary
            .finding_alignment_actionable_gap_packet_public_projection_eligible_packets,
        2
    );
    assert_eq!(
        report
            .summary
            .finding_alignment_actionable_gap_packet_public_projection_excluded_packets,
        1
    );
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["finding_alignment_actionable_gap_packet_public_projection_eligible_packets"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["summary"]["finding_alignment_actionable_gap_packet_public_projection_excluded_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["actionable_gap_packet_public_projection"]["projection_exclusion_reasons"][0]["label"],
        "missing_receipt_command"
    );

    let markdown = evidence_quality_scorecard_markdown(&report);
    assert!(markdown.contains("Actionable Gap Packet Public Projection Readiness"));
    assert!(markdown.contains("Public projection eligible packets"));
    assert!(markdown.contains("missing_receipt_command"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_summarizes_language_aware_placement_route_quality()
-> Result<(), String> {
    let mut audit = scorecard_minimal_audit_value(0, 7, 0, 0, 7);
    audit
        .as_object_mut()
        .ok_or_else(|| "audit should be an object".to_string())?
        .insert(
            "static_limitations".to_string(),
            serde_json::json!({
                "by_reason": [],
                "by_stage": {},
                "by_category": {
                    "cross_language_target_unresolved": 7,
                    "cross_language_oracle_visibility_unresolved": 2
                },
                "repair_routes": {
                    "analysis/cross-language-test-target-inference": 7,
                    "analysis/cross-language-oracle-visibility": 2
                }
            }),
        );
    audit
        .as_object_mut()
        .ok_or_else(|| "audit should be an object".to_string())?
        .insert(
            "finding_alignment".to_string(),
            serde_json::json!({
                "summary": {
                    "raw_signals": 7,
                    "canonical_items": 7,
                    "actionable_gaps": 0,
                    "static_limitations": 7
                },
                "coverage": {
                    "static_unknown_without_named_limitation": 0,
                    "canonical_items_without_repair_route": 0,
                    "canonical_items_without_verify_command": 0
                },
                "actionable_gap_packet_public_projection": {
                    "scope": "emitted_actionable_gap_packets",
                    "public_projection_eligible_packets": 0,
                    "public_projection_excluded_packets": 1,
                    "projection_exclusion_reasons": [
                        {"label": "cross_language_target_unresolved", "count": 1}
                    ]
                },
                "actionable_gap_packets": [
                    {
                        "canonical_gap_id": "gap:nav-only",
                        "public_projection_eligible": false,
                        "projection_exclusion_reasons": [
                            "cross_language_target_unresolved"
                        ],
                        "navigation_only_target": {
                            "file": "test/js/web/fetch/blob.test.ts",
                            "line": 41,
                            "language": "typescript",
                            "repair_packet_ready": false
                        }
                    }
                ]
            }),
        );

    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let placement = &value["language_aware_placement_route_quality"];

    assert_eq!(placement["status"], "navigation_only_context_visible");
    assert_eq!(
        placement["repair_route"],
        "analysis/cross-language-test-target-inference"
    );
    assert_eq!(
        placement["cross_language_target_unresolved_signals"],
        serde_json::Value::from(7)
    );
    assert_eq!(
        placement["cross_language_test_target_inference_route_signals"],
        serde_json::Value::from(7)
    );
    assert_eq!(
        placement["cross_language_oracle_visibility_unresolved_signals"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        placement["navigation_only_external_target_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        placement["navigation_only_external_target_public_promotions"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        placement["navigation_only_external_target_status"],
        "explicit_navigation_context_only"
    );
    assert!(placement["non_claims"].as_array().is_some_and(|claims| {
        claims
            .iter()
            .any(|claim| claim == "not a public repair packet")
    }));
    assert_eq!(
        value["summary"]["finding_alignment_actionable_unresolved_canonical_gaps"],
        serde_json::Value::from(0)
    );

    let markdown = evidence_quality_scorecard_markdown(&report);
    assert!(markdown.contains("## Language-Aware Placement Route Quality"));
    assert!(markdown.contains("analysis/cross-language-test-target-inference"));
    assert!(markdown.contains("Navigation-only external target packets"));
    assert!(markdown.contains("navigation_only_external_target_status"));
    assert!(markdown.contains("not runtime calibration or public repair packets"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_summarizes_cross_language_oracle_route_quality() -> Result<(), String>
{
    let audit = scorecard_minimal_audit_value(0, 0, 0, 0, 0);
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let route_quality = &value["cross_language_oracle_route_quality"];

    assert_eq!(route_quality["status"], "pass");
    assert_eq!(
        route_quality["cross_language_oracle_graph_complete_advisory_witnesses"],
        serde_json::Value::from(3)
    );
    assert_eq!(
        route_quality["cross_language_oracle_graph_missing_discriminator_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        route_quality["cross_language_oracle_graph_missing_external_oracle_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        route_quality["cross_language_oracle_graph_bridge_unknown_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        route_quality["cross_language_oracle_graph_mention_only_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        route_quality["cross_language_oracle_graph_panic_boundary_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        route_quality["cross_language_oracle_graph_manifest_only_profiles"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        route_quality["cross_language_oracle_graph_public_packet_exclusions"],
        serde_json::Value::from(11)
    );
    assert_eq!(
        route_quality["repair_packet_ready_cases"],
        serde_json::Value::from(0)
    );
    assert!(
        route_quality["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["case_id"] == "bun_blob_bridge_unknown_limitation"
                    && row["missing_graph_legs"]
                        .as_array()
                        .is_some_and(|legs| legs.iter().any(|leg| leg == "binding_or_ffi_edge"))
            }))
    );
    assert!(
        route_quality["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["case_id"] == "bun_array_buffer_copy_to_unshared_configured_bridge_advisory"
                    && row["observed_state"] == "rust_ungripped_ts_discriminated"
                    && row["binding_edge_confidence"] == "configured_hint"
                    && row["missing_graph_legs"]
                        .as_array()
                        .is_some_and(Vec::is_empty)
                    && row["repair_packet_ready"] == false
                    && row["public_projection_eligible"] == false
            }))
    );
    assert!(
        route_quality["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["case_id"] == "bun_write_helper_gated_manifest_only_profile"
                    && row["profile"] == "bun_write_helper_gated"
                    && row["profile_status"] == "manifest_only"
                    && row["observed_state"] == "named_static_limitation"
                    && row["proof_mode"] == "helper_gated"
                    && row["suggested_test_file"] == "not_applicable"
                    && row["repair_packet_ready"] == false
                    && row["public_projection_eligible"] == false
            }))
    );
    assert!(
        route_quality["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["case_id"] == "bun_markdown_resizable_array_buffer_configured_bridge_advisory"
                    && row["profile"] == "bun_markdown_resizable_array_buffer"
                    && row["observed_state"] == "rust_ungripped_ts_discriminated"
                    && row["binding_edge_confidence"] == "configured_hint"
                    && row["missing_graph_legs"]
                        .as_array()
                        .is_some_and(Vec::is_empty)
                    && row["repair_packet_ready"] == false
                    && row["public_projection_eligible"] == false
            }))
    );
    assert!(
        route_quality["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["case_id"] == "bun_ffi_negative_offset_panic_boundary_limitation"
                    && row["observed_state"] == "public_reachable_panic_boundary_unrevealed"
                    && row["limitation_category"]
                        == "cross_language_panic_boundary_visibility_unresolved"
                    && row["repair_packet_ready"] == false
                    && row["public_projection_eligible"] == false
            }))
    );
    assert!(
        route_quality["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["case_id"] == "bun_node_fs_scalar_write_manifest_only_profile"
                    && row["profile"] == "bun_node_fs_scalar_write"
                    && row["profile_status"] == "manifest_only"
                    && row["observed_state"] == "named_static_limitation"
                    && row["proof_mode"] == "observable_red_green"
                    && row["suggested_test_file"] == "test/js/node/fs/fs.test.ts"
                    && row["repair_packet_ready"] == false
                    && row["public_projection_eligible"] == false
            }))
    );

    let markdown = evidence_quality_scorecard_markdown(&report);
    assert!(markdown.contains("## Cross-Language Oracle Route Quality"));
    assert!(markdown.contains("Missing discriminator limitations"));
    assert!(markdown.contains("Bridge unknown limitations"));
    assert!(markdown.contains("Panic boundary limitations"));
    assert!(markdown.contains("Manifest-only profiles"));
    assert!(markdown.contains("Repair-packet-ready cases"));
    assert!(markdown.contains("not full cross-language proof"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_classifies_maturity_by_proof_scope() -> Result<(), String> {
    let audit = lane1_scorecard_sample_audit_value()?;
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;

    assert_eq!(
        scorecard_status_for(&report, "canonical_gap_identity")?,
        "fixture_backed"
    );
    assert_eq!(
        scorecard_status_for(&report, "related_test_ranking")?,
        "static_only"
    );
    assert_eq!(
        scorecard_status_for(&report, "runtime_calibration")?,
        "imported_runtime_calibrated"
    );

    let uncalibrated = scorecard_minimal_audit_value(0, 0, 0, 0, 0);
    let uncalibrated_report = evidence_quality_scorecard_from_values(
        "unix_ms:2".to_string(),
        scorecard_inputs_for_test(false),
        &uncalibrated,
        None,
        None,
    )?;
    assert_eq!(
        scorecard_status_for(&uncalibrated_report, "runtime_calibration")?,
        "uncalibrated"
    );
    Ok(())
}

#[test]
fn evidence_quality_scorecard_orders_repairs_by_risk_not_count() -> Result<(), String> {
    let audit = scorecard_minimal_audit_value(1, 99, 0, 0, 0);
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    let first = report
        .recommended_repairs
        .first()
        .ok_or_else(|| "expected at least one recommended repair".to_string())?;
    assert_eq!(first.slice, "analysis/related-test-ranking-audit-fixes");
    assert_eq!(first.signal_count, 1);
    Ok(())
}

#[test]
fn evidence_quality_scorecard_reports_recent_deltas_when_present() -> Result<(), String> {
    let audit = scorecard_minimal_audit_value(1, 3, 2, 0, 5);
    let previous = serde_json::json!({
        "summary": {
            "duplicate_looking_groups_total": 8,
            "missing_discriminators_total": 7,
            "static_limitations_total": 5,
            "low_or_opaque_top_related_tests": 4,
            "uncalibrated_records": 9,
            "calibrated_records": 1
        }
    });
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(true),
        &audit,
        None,
        Some(&previous),
    )?;

    assert!(report.recent_audit_deltas.available);
    let duplicate_delta = report
        .recent_audit_deltas
        .deltas
        .iter()
        .find(|delta| delta.metric == "duplicate_looking_groups_total")
        .ok_or_else(|| "duplicate delta missing".to_string())?;
    assert_eq!(duplicate_delta.before, 8);
    assert_eq!(duplicate_delta.after, 2);
    assert_eq!(duplicate_delta.direction, "improved");
    Ok(())
}

#[test]
fn evidence_quality_scorecard_reports_finding_alignment_presentation_text_counts()
-> Result<(), String> {
    let mut audit = scorecard_minimal_audit_value(0, 0, 0, 0, 0);
    audit
        .as_object_mut()
        .ok_or_else(|| "audit should be an object".to_string())?
        .insert(
            "finding_alignment".to_string(),
            serde_json::json!({
                "summary": {
                    "raw_signals": 6,
                    "canonical_items": 3,
                    "aligned_raw_findings": 5,
                    "unaligned_raw_findings": 1,
                    "duplicate_groups_total": 2,
                    "actionable_gaps": 1,
                    "already_observed": 1,
                    "internal_no_action": 1,
                    "static_limitations": 1,
                    "unknown": 0,
                    "presentation_text_total": 3,
                    "presentation_text_user_visible": 2,
                    "presentation_text_observed": 1,
                    "presentation_text_unobserved": 1,
                    "presentation_text_internal_only": 1,
                    "presentation_text_visibility_unknown": 1,
                    "presentation_text_observer_unknown": 1,
                    "presentation_text_duplicate_groups": 2,
                    "presentation_text_actionable_snapshot": 1,
                    "presentation_text_no_action": 2,
                    "presentation_text_static_limitations": 1
                },
                "coverage": {
                    "static_unknown_without_named_limitation": 0,
                    "canonical_items_without_repair_route": 1,
                    "canonical_items_without_verify_command": 2
                }
            }),
        );

    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;
    assert_eq!(report.summary.finding_alignment_raw_signals_total, 6);
    assert_eq!(report.summary.finding_alignment_canonical_items_total, 3);
    assert_eq!(report.summary.finding_alignment_duplicate_groups_total, 2);
    assert_eq!(report.summary.finding_alignment_actionable_items_total, 1);
    assert_eq!(report.summary.finding_alignment_internal_no_action_total, 1);
    assert_eq!(
        report
            .summary
            .finding_alignment_canonical_items_without_repair_route,
        1
    );
    assert_eq!(
        report
            .summary
            .finding_alignment_canonical_items_without_verify_command,
        2
    );
    assert_eq!(
        report
            .summary
            .finding_alignment_static_unknown_without_named_limitation,
        0
    );
    assert_eq!(report.summary.presentation_text_user_visible, 2);
    assert_eq!(report.summary.presentation_text_observed, 1);
    assert_eq!(report.summary.presentation_text_no_action, 2);
    assert_eq!(
        finding_alignment_raw_to_canonical_ratio(&report.summary),
        Some(2.0)
    );
    assert!(report.recommended_repairs.iter().any(|repair| repair.slice
        == "analysis/predicate-boundary-repair-routes"
        && repair.signal_count == 1));

    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["finding_alignment_raw_to_canonical_ratio"],
        serde_json::Value::from(2.0)
    );
    assert_eq!(
        value["summary"]["presentation_text_static_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["finding_alignment_canonical_items_without_repair_route"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["finding_alignment_canonical_items_without_verify_command"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["summary"]["finding_alignment_static_unknown_without_named_limitation"],
        serde_json::Value::from(0)
    );

    let markdown = evidence_quality_scorecard_markdown(&report);
    assert!(markdown.contains("Finding Alignment And Presentation Text"));
    assert!(markdown.contains("Raw-to-canonical ratio"));
    assert!(markdown.contains("Static unknown without named limitation"));
    assert!(markdown.contains("Canonical items without repair route"));
    assert!(markdown.contains("Canonical items without verify command"));
    Ok(())
}

#[test]
fn evidence_quality_scorecard_uses_audit_canonical_item_alignment_summary() -> Result<(), String> {
    let audit = lane1_scorecard_sample_audit_value()?;
    let report = evidence_quality_scorecard_from_values(
        "unix_ms:1".to_string(),
        scorecard_inputs_for_test(false),
        &audit,
        None,
        None,
    )?;

    assert_eq!(report.summary.finding_alignment_raw_signals_total, 3);
    assert_eq!(report.summary.finding_alignment_canonical_items_total, 3);
    assert_eq!(report.summary.finding_alignment_actionable_items_total, 2);
    assert_eq!(report.summary.finding_alignment_static_limitation_total, 1);
    assert_eq!(
        report.summary.finding_alignment_calibrated_supported_total,
        1
    );
    assert_eq!(report.summary.finding_alignment_uncalibrated_total, 2);
    assert!(
        !report
            .unknowns
            .iter()
            .any(|unknown| unknown.kind == "finding_alignment_unavailable")
    );
    let json = evidence_quality_scorecard_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let rows = value["calibration_coverage"]["by_evidence_class"]
        .as_array()
        .ok_or_else(|| "scorecard calibration by-class rows should be an array".to_string())?;
    let predicate = rows
        .iter()
        .find(|row| row["evidence_class"] == "predicate_boundary")
        .ok_or_else(|| "missing predicate_boundary by-class row".to_string())?;
    assert_eq!(predicate["static_only"], serde_json::Value::from(2));
    assert_eq!(predicate["uncalibrated"], serde_json::Value::from(2));
    let call = rows
        .iter()
        .find(|row| row["evidence_class"] == "call_presence")
        .ok_or_else(|| "missing call_presence by-class row".to_string())?;
    assert_eq!(call["calibrated_supported"], serde_json::Value::from(1));
    let markdown = evidence_quality_scorecard_markdown(&report);
    assert!(markdown.contains("Calibrated supported"));
    assert!(markdown.contains("predicate_boundary"));
    Ok(())
}

#[test]
fn evidence_quality_trend_reports_no_history_explicitly() -> Result<(), String> {
    let current = scorecard_minimal_audit_value(0, 0, 0, 2, 3);
    let report = evidence_quality_trend_from_values(
        "unix_ms:1".to_string(),
        trend_inputs_for_test(false),
        &current,
        None,
    )?;
    let json = evidence_quality_trend_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["report"], "evidence-quality-trend");
    assert_eq!(value["summary"]["status"], "unknown");
    assert_eq!(
        value["summary"]["no_history"],
        serde_json::Value::from(true)
    );
    assert_eq!(
        value["movement_front"]["current_actionable_count"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["movement_front"]["actionable_delta_since_prior_refresh"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["movement_front"]["top_blocked_reason"],
        "trend_history_unavailable"
    );
    assert!(value["unknowns"].as_array().is_some_and(|unknowns| {
        unknowns
            .iter()
            .any(|unknown| unknown["kind"] == "trend_history_unavailable")
    }));
    let markdown = evidence_quality_trend_markdown(&report);
    assert!(markdown.contains("Lane 1 evidence quality trend"));
    assert!(markdown.contains("## Movement Since Prior Refresh"));
    assert!(markdown.contains("| Current actionable count | unavailable |"));
    assert!(markdown.contains("| Delta since prior refresh | unavailable |"));
    assert!(markdown.contains("| Improved | unavailable |"));
    assert!(markdown.contains("| Missing receipts | unavailable |"));
    assert!(markdown.contains("Metric Trends"));
    assert!(markdown.contains("Unknowns"));
    Ok(())
}

#[test]
fn evidence_quality_trend_marks_limited_current_scorecard_unknown() -> Result<(), String> {
    let mut current = scorecard_minimal_audit_value(0, 1, 0, 5, 2);
    let mut previous = scorecard_minimal_audit_value(4, 8, 5, 2, 6);
    push_scorecard_unknown(&mut current, "lane1_evidence_audit_limited")?;
    push_scorecard_unknown(&mut current, "evidence_health_limited")?;
    push_scorecard_unknown(
        &mut current,
        "evidence_quality_scorecard_audit_regeneration_failed",
    )?;
    set_static_category_count(&mut current, "opaque_helper_call", 1)?;
    set_static_category_count(&mut previous, "opaque_helper_call", 8)?;

    let report = evidence_quality_trend_from_values(
        "unix_ms:1".to_string(),
        trend_inputs_for_test(true),
        &current,
        Some(&previous),
    )?;

    assert_eq!(report.summary.status, "unknown");
    assert_eq!(report.summary.compared_metrics, 0);
    assert_eq!(report.summary.improved_metrics, 0);
    assert_eq!(report.summary.regressed_metrics, 0);
    assert_eq!(report.summary.unchanged_metrics, 0);
    assert_eq!(report.summary.unknown_metrics, report.metric_trends.len());
    assert!(
        report
            .metric_trends
            .iter()
            .all(|trend| trend.direction == "unknown" && trend.delta.is_none())
    );
    assert!(
        report
            .static_limitation_category_trends
            .iter()
            .all(|trend| trend.direction == "unknown" && trend.delta.is_none())
    );
    let current_limited = report
        .unknowns
        .iter()
        .find(|unknown| unknown.kind == "current_scorecard_limited")
        .ok_or_else(|| "missing current_scorecard_limited trend unknown".to_string())?;
    assert!(
        current_limited
            .summary
            .contains("lane1_evidence_audit_limited")
    );
    assert!(current_limited.summary.contains("evidence_health_limited"));
    assert!(
        current_limited
            .summary
            .contains("evidence_quality_scorecard_audit_regeneration_failed")
    );

    let json = evidence_quality_trend_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "current_scorecard_limited"
    );
    assert_eq!(value["summary"]["status"], "unknown");
    assert_eq!(
        value["summary"]["compared_metrics"],
        serde_json::Value::from(0)
    );
    assert!(value["unknowns"].as_array().is_some_and(|unknowns| {
        unknowns
            .iter()
            .any(|unknown| unknown["kind"] == "current_scorecard_limited")
    }));

    let markdown = evidence_quality_trend_markdown(&report);
    assert!(markdown.contains("## Runtime Status"));
    assert!(markdown.contains("| State | `limited_incomplete_input` |"));
    assert!(markdown.contains("| Limitation category | `current_scorecard_limited` |"));
    assert!(markdown.contains("| Downstream consumable | `false` |"));
    assert!(markdown.contains("current_scorecard_limited"));
    assert!(markdown.contains("cannot claim improvement or regression"));
    Ok(())
}

#[test]
fn evidence_quality_trend_missing_previous_writes_limited_report() -> Result<(), String> {
    with_temp_cwd("trend-missing-previous", |_root| {
        let current = scorecard_minimal_audit_value(0, 3, 1, 5, 4);
        write(
            Path::new("target/ripr/reports/evidence-quality-scorecard.json"),
            &serde_json::to_string(&current).map_err(|err| err.to_string())?,
        );
        let args = vec![
            "--previous".to_string(),
            "target/ripr/reports/missing-previous-scorecard.json".to_string(),
        ];

        evidence_quality_trend_report_impl(&args)?;

        let json = fs::read_to_string("target/ripr/reports/evidence-quality-trend.json")
            .map_err(|err| format!("read trend json: {err}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(|err| err.to_string())?;
        assert_eq!(value["run_status"], "limited_incomplete_input");
        assert_eq!(
            value["runtime_status"]["limitation_category"],
            EVIDENCE_QUALITY_TREND_PREVIOUS_ARTIFACT_UNAVAILABLE
        );
        assert_eq!(value["summary"]["status"], "unknown");
        assert_eq!(value["summary"]["no_history"], true);
        assert_eq!(value["inputs"]["previous_artifact"]["status"], "missing");
        assert!(
            value["metric_trends"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(value["unknowns"].as_array().is_some_and(|unknowns| {
            unknowns.iter().any(|unknown| {
                unknown["kind"] == EVIDENCE_QUALITY_TREND_PREVIOUS_ARTIFACT_UNAVAILABLE
            })
        }));
        let markdown = fs::read_to_string("target/ripr/reports/evidence-quality-trend.md")
            .map_err(|err| format!("read trend markdown: {err}"))?;
        assert!(markdown.contains(EVIDENCE_QUALITY_TREND_PREVIOUS_ARTIFACT_UNAVAILABLE));
        assert!(markdown.contains("No movement or badge-readiness delta claim"));
        Ok(())
    })
}

#[test]
fn evidence_quality_trend_malformed_previous_writes_limited_report() -> Result<(), String> {
    with_temp_cwd("trend-malformed-previous", |_root| {
        let current = scorecard_minimal_audit_value(0, 3, 1, 5, 4);
        write(
            Path::new("target/ripr/reports/evidence-quality-scorecard.json"),
            &serde_json::to_string(&current).map_err(|err| err.to_string())?,
        );
        write(
            Path::new("target/ripr/reports/previous-scorecard.json"),
            "{not-json",
        );
        let args = vec![
            "--previous".to_string(),
            "target/ripr/reports/previous-scorecard.json".to_string(),
        ];

        evidence_quality_trend_report_impl(&args)?;

        let json = fs::read_to_string("target/ripr/reports/evidence-quality-trend.json")
            .map_err(|err| format!("read trend json: {err}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(|err| err.to_string())?;
        assert_eq!(value["run_status"], "limited_incomplete_input");
        assert_eq!(value["summary"]["status"], "unknown");
        assert_eq!(value["summary"]["no_history"], true);
        assert_eq!(value["inputs"]["previous_artifact"]["status"], "malformed");
        assert!(
            value["metric_trends"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(value["unknowns"].as_array().is_some_and(|unknowns| {
            unknowns.iter().any(|unknown| {
                unknown["kind"] == EVIDENCE_QUALITY_TREND_PREVIOUS_ARTIFACT_UNAVAILABLE
            })
        }));
        let markdown = fs::read_to_string("target/ripr/reports/evidence-quality-trend.md")
            .map_err(|err| format!("read trend markdown: {err}"))?;
        assert!(markdown.contains(EVIDENCE_QUALITY_TREND_PREVIOUS_ARTIFACT_UNAVAILABLE));
        assert!(markdown.contains("No movement or badge-readiness delta claim"));
        Ok(())
    })
}

#[test]
fn evidence_quality_trend_distinguishes_improvement_regression_and_unchanged() -> Result<(), String>
{
    let current = scorecard_minimal_audit_value(0, 3, 1, 5, 4);
    let previous = scorecard_minimal_audit_value(2, 3, 3, 3, 6);
    let report = evidence_quality_trend_from_values(
        "unix_ms:1".to_string(),
        trend_inputs_for_test(true),
        &current,
        Some(&previous),
    )?;

    assert_eq!(report.summary.status, "improvement");
    assert_eq!(
        trend_direction_for(&report, "duplicate_looking_groups_total")?,
        "improvement"
    );
    assert_eq!(
        trend_direction_for(&report, "calibrated_records")?,
        "improvement"
    );
    assert_eq!(
        trend_direction_for(&report, "static_limitations_total")?,
        "unchanged"
    );

    let regressing_current = scorecard_minimal_audit_value(0, 5, 7, 3, 8);
    let regressing_report = evidence_quality_trend_from_values(
        "unix_ms:2".to_string(),
        trend_inputs_for_test(true),
        &regressing_current,
        Some(&previous),
    )?;
    assert_eq!(
        trend_direction_for(&regressing_report, "duplicate_looking_groups_total")?,
        "regression"
    );
    Ok(())
}

#[test]
fn evidence_quality_trend_reports_static_limitation_category_deltas() -> Result<(), String> {
    let mut current = scorecard_minimal_audit_value(0, 2, 0, 1, 1);
    let mut previous = scorecard_minimal_audit_value(0, 5, 0, 1, 1);
    set_static_category_count(&mut current, "opaque_helper_call", 2)?;
    set_static_category_count(&mut previous, "opaque_helper_call", 5)?;

    let report = evidence_quality_trend_from_values(
        "unix_ms:1".to_string(),
        trend_inputs_for_test(true),
        &current,
        Some(&previous),
    )?;
    let row = report
        .static_limitation_category_trends
        .iter()
        .find(|row| row.label == "opaque_helper_call")
        .ok_or_else(|| "missing opaque_helper_call category trend".to_string())?;
    assert_eq!(row.before, Some(5));
    assert_eq!(row.after, Some(2));
    assert_eq!(row.direction, "improvement");
    Ok(())
}

#[test]
fn evidence_quality_trend_reports_runtime_confidence_static_only_classes() -> Result<(), String> {
    let mut current = scorecard_minimal_audit_value(0, 2, 0, 1, 1);
    let mut previous = scorecard_minimal_audit_value(0, 5, 0, 1, 1);
    set_runtime_confidence_class_counts(&mut current, "call_presence", 7, 0, 7)?;
    set_runtime_confidence_class_counts(&mut current, "predicate_boundary", 3, 2, 1)?;
    set_runtime_confidence_class_counts(&mut previous, "call_presence", 12, 0, 12)?;
    set_runtime_confidence_class_counts(&mut previous, "predicate_boundary", 4, 1, 3)?;

    let report = evidence_quality_trend_from_values(
        "unix_ms:1".to_string(),
        trend_inputs_for_test(true),
        &current,
        Some(&previous),
    )?;

    let first = report
        .runtime_confidence_static_only_class_trends
        .first()
        .ok_or_else(|| "missing runtime confidence static-only class trend".to_string())?;
    assert_eq!(
        first.metric,
        "runtime_confidence_static_only_class:call_presence"
    );
    assert_eq!(first.label, "call_presence");
    assert_eq!(first.before, Some(12));
    assert_eq!(first.after, Some(7));
    assert_eq!(first.direction, "improvement");
    let predicate = report
        .runtime_confidence_static_only_class_trends
        .iter()
        .find(|row| row.label == "predicate_boundary")
        .ok_or_else(|| "missing predicate_boundary runtime trend".to_string())?;
    assert_eq!(predicate.before, Some(3));
    assert_eq!(predicate.after, Some(1));

    let json = evidence_quality_trend_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let rows = value["runtime_confidence_static_only_class_trends"]
        .as_array()
        .ok_or_else(|| {
            "runtime_confidence_static_only_class_trends should be an array".to_string()
        })?;
    assert!(
        rows.iter()
            .any(|row| row["metric"] == "runtime_confidence_static_only_class:call_presence")
    );

    let markdown = evidence_quality_trend_markdown(&report);
    assert!(markdown.contains("Runtime Confidence Static-Only Class Trends"));
    assert!(markdown.contains("call_presence"));
    Ok(())
}

#[test]
fn evidence_quality_trend_reports_finding_alignment_presentation_text_deltas() -> Result<(), String>
{
    let mut current = scorecard_minimal_audit_value(0, 0, 0, 0, 0);
    let mut previous = scorecard_minimal_audit_value(0, 0, 0, 0, 0);
    set_summary_count(&mut current, "finding_alignment_duplicate_groups_total", 1)?;
    set_summary_count(&mut previous, "finding_alignment_duplicate_groups_total", 3)?;
    set_summary_count(
        &mut current,
        "finding_alignment_actionable_unresolved_canonical_gaps",
        2,
    )?;
    set_summary_count(
        &mut previous,
        "finding_alignment_actionable_unresolved_canonical_gaps",
        5,
    )?;
    set_summary_count(&mut current, "presentation_text_observed", 4)?;
    set_summary_count(&mut previous, "presentation_text_observed", 2)?;
    set_summary_count(&mut current, "presentation_text_visibility_unknown", 1)?;
    set_summary_count(&mut previous, "presentation_text_visibility_unknown", 5)?;
    set_summary_count(
        &mut current,
        "finding_alignment_canonical_items_without_repair_route",
        1,
    )?;
    set_summary_count(
        &mut previous,
        "finding_alignment_canonical_items_without_repair_route",
        4,
    )?;
    set_summary_count(
        &mut current,
        "finding_alignment_canonical_items_without_verify_command",
        2,
    )?;
    set_summary_count(
        &mut previous,
        "finding_alignment_canonical_items_without_verify_command",
        2,
    )?;
    set_summary_count(
        &mut current,
        "finding_alignment_static_unknown_without_named_limitation",
        3,
    )?;
    set_summary_count(
        &mut previous,
        "finding_alignment_static_unknown_without_named_limitation",
        1,
    )?;
    set_summary_count(
        &mut current,
        "finding_alignment_actionable_gap_packet_public_projection_eligible_packets",
        4,
    )?;
    set_summary_count(
        &mut previous,
        "finding_alignment_actionable_gap_packet_public_projection_eligible_packets",
        1,
    )?;
    set_summary_count(
        &mut current,
        "finding_alignment_actionable_gap_packet_public_projection_excluded_packets",
        0,
    )?;
    set_summary_count(
        &mut previous,
        "finding_alignment_actionable_gap_packet_public_projection_excluded_packets",
        3,
    )?;

    let report = evidence_quality_trend_from_values(
        "unix_ms:1".to_string(),
        trend_inputs_for_test(true),
        &current,
        Some(&previous),
    )?;

    let first_metric = report
        .metric_trends
        .first()
        .ok_or_else(|| "expected metric trends".to_string())?;
    assert_eq!(
        first_metric.metric,
        "finding_alignment_actionable_unresolved_canonical_gaps"
    );
    assert_eq!(first_metric.label, "Actionable canonical gaps");
    assert_eq!(first_metric.direction, "improvement");
    assert_eq!(
        trend_direction_for(&report, "finding_alignment_duplicate_groups_total")?,
        "improvement"
    );
    assert_eq!(
        trend_direction_for(&report, "presentation_text_observed")?,
        "improvement"
    );
    assert_eq!(
        trend_direction_for(&report, "presentation_text_visibility_unknown")?,
        "improvement"
    );
    assert_eq!(
        trend_direction_for(
            &report,
            "finding_alignment_canonical_items_without_repair_route"
        )?,
        "improvement"
    );
    assert_eq!(
        trend_direction_for(
            &report,
            "finding_alignment_canonical_items_without_verify_command"
        )?,
        "unchanged"
    );
    assert_eq!(
        trend_direction_for(
            &report,
            "finding_alignment_static_unknown_without_named_limitation"
        )?,
        "regression"
    );
    assert_eq!(
        trend_direction_for(
            &report,
            "finding_alignment_actionable_gap_packet_public_projection_eligible_packets"
        )?,
        "improvement"
    );
    assert_eq!(
        trend_direction_for(
            &report,
            "finding_alignment_actionable_gap_packet_public_projection_excluded_packets"
        )?,
        "improvement"
    );

    let json = evidence_quality_trend_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["movement_front"]["current_actionable_count"], 2);
    assert_eq!(
        value["movement_front"]["actionable_delta_since_prior_refresh"],
        -3
    );
    assert_eq!(
        value["movement_front"]["top_blocked_reason"],
        "metric_regression:finding_alignment_static_unknown_without_named_limitation"
    );
    let metric_trends = value["metric_trends"]
        .as_array()
        .ok_or_else(|| "metric_trends should be an array".to_string())?;
    assert_eq!(
        metric_trends
            .first()
            .and_then(|trend| trend["metric"].as_str()),
        Some("finding_alignment_actionable_unresolved_canonical_gaps")
    );
    assert!(
        metric_trends.iter().any(
            |trend| trend["metric"] == "finding_alignment_actionable_unresolved_canonical_gaps"
        )
    );
    assert!(
        metric_trends.iter().any(
            |trend| trend["metric"] == "finding_alignment_canonical_items_without_repair_route"
        )
    );
    assert!(
        metric_trends
            .iter()
            .any(|trend| trend["metric"]
                == "finding_alignment_canonical_items_without_verify_command")
    );
    assert!(metric_trends.iter().any(
        |trend| trend["metric"] == "finding_alignment_static_unknown_without_named_limitation"
    ));
    assert!(metric_trends.iter().any(|trend| trend["metric"]
        == "finding_alignment_actionable_gap_packet_public_projection_eligible_packets"));
    assert!(metric_trends.iter().any(|trend| trend["metric"]
        == "finding_alignment_actionable_gap_packet_public_projection_excluded_packets"));

    let markdown = evidence_quality_trend_markdown(&report);
    assert!(markdown.contains("## Movement Since Prior Refresh"));
    assert!(markdown.contains("| Current actionable count | 2 |"));
    assert!(markdown.contains("| Delta since prior refresh | -3 |"));
    assert!(markdown.contains("| Orphaned receipts | unavailable |"));
    assert!(markdown.contains("Actionable canonical gaps"));
    assert!(markdown.contains("Finding-alignment canonical items without repair route"));
    assert!(markdown.contains("Finding-alignment canonical items without verify command"));
    assert!(markdown.contains("Finding-alignment static unknown without named limitation"));
    assert!(markdown.contains("Actionable gap public-projection eligible packets"));
    assert!(markdown.contains("Actionable gap public-projection excluded packets"));
    Ok(())
}
