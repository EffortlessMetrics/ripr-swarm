//! Evidence-promotion semantic-assertion and corpus tests, moved verbatim
//! from `tests.rs` (#2948, slice C of #2119). Test names are unchanged apart
//! from the module path; `super::` references to the xtask crate root became
//! `crate::`.

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
    crate::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
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
    crate::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
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
    crate::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
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
    crate::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
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
    crate::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
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
    let result = crate::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations);

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
    crate::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
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
        crate::EvidencePromotionSemanticAssertion::MustNotHaveVerifyCommand,
        crate::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand,
        crate::EvidencePromotionSemanticAssertion::MustEmitRepairPacket,
        crate::EvidencePromotionSemanticAssertion::MustNotEmitLimitation,
        crate::EvidencePromotionSemanticAssertion::ExpectedCompleteness {
            completeness: "limited".to_string(),
        },
        crate::EvidencePromotionSemanticAssertion::ExpectedChangedRustFiles { count: 2 },
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

    let report = crate::evidence_promotion_semantic_violations(
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
    let assertions =
        vec![crate::EvidencePromotionSemanticAssertion::ExpectedChangedRustFiles { count: 0 }];
    let check_json = serde_json::json!({
        "summary": {"changed_rust_files": 0, "findings": 0},
        "findings": []
    });

    let violations = crate::evidence_promotion_semantic_violations(
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
    let report = crate::evidence_promotion_semantic_violations(
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
        crate::EvidencePromotionSemanticAssertion::MustNotPromote,
        crate::EvidencePromotionSemanticAssertion::ExpectedFindingCount { count: 0 },
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
    let violations = crate::evidence_promotion_semantic_violations(
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
    let report = crate::evidence_promotion_semantic_violations(
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
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustHaveVerifyCommand];
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

    let report = crate::evidence_promotion_semantic_violations(
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
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustNotHaveVerifyCommand];
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

    let report = crate::evidence_promotion_semantic_violations(
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
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand];
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

    let report = crate::evidence_promotion_semantic_violations(
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
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand];
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

    let report = crate::evidence_promotion_semantic_violations(
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
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "smoke_only".to_string(),
        strength: "smoke".to_string(),
    }];
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

    let report = crate::evidence_promotion_semantic_violations(
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
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "exact_value".to_string(),
        strength: "strong".to_string(),
    }];
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

    let report = crate::evidence_promotion_semantic_violations(
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
fn evidence_promotion_human_oracle_line_accepts_unclassified_ungraded_wording() {
    let line = "- related test tests/a.rs:3 t uses an unclassified oracle, not graded against this probe: assert_eq!(f(), 1)";
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        line, "unknown", "unknown"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        line,
        "exact_value",
        "unknown"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        line, "unknown", "strong"
    ));
}

#[test]
fn evidence_promotion_human_oracle_line_accepts_deduplicated_smoke_only_wording() {
    let line = "- related test tests/a.ts:3 t uses smoke only oracle: t.truthy(x)";
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        line,
        "smoke_only",
        "smoke"
    ));
    // The stutter is no longer the rendered form, and other pairs keep the
    // full "<strength> <kind>" spelling.
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        "- related test tests/a.ts:3 t uses smoke smoke only oracle: t.truthy(x)",
        "smoke_only",
        "smoke"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        line,
        "smoke_only",
        "weak"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        line,
        "exact_value",
        "smoke"
    ));
}

#[test]
fn evidence_promotion_human_oracle_line_accepts_kind_ungraded_wording() {
    let line = "- related test tests/a.rs:3 t uses exact value oracle, not graded against this probe: assert_eq!(f(), 1)";
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        line,
        "exact_value",
        "unknown"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        line,
        "exact_value",
        "strong"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        line,
        "smoke_only",
        "unknown"
    ));
    let stale = "- related test tests/a.rs:3 t uses unknown exact value oracle: assert_eq!(f(), 1)";
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        stale,
        "exact_value",
        "unknown"
    ));
}

#[test]
fn evidence_promotion_human_oracle_line_accepts_absent_oracle_wording() {
    let line = "- related test tests/a.rs:3 t uses no oracle; misses: has no assertion";
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        line, "unknown", "none"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        "- related test tests/a.rs:3 t uses none unknown oracle; misses: has no assertion",
        "unknown",
        "none"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        "- related test tests/a.rs:3 t uses no oracles here",
        "unknown",
        "none"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        line, "unknown", "weak"
    ));
    // An absent oracle carries no kind, so a concrete expected kind cannot match it.
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        line,
        "exact_value",
        "none"
    ));
}

#[test]
fn evidence_promotion_semantic_assertions_accept_human_oracle_projection() {
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "exact_value".to_string(),
        strength: "strong".to_string(),
    }];
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

    let report = crate::evidence_promotion_semantic_violations(
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
    let assertions = vec![crate::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "exact_value".to_string(),
        strength: "strong".to_string(),
    }];
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

    let report = crate::evidence_promotion_semantic_violations(
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
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        "  oracle: exact_value (strong)",
        "exact_value",
        "strong"
    ));
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        "current test evidence: oracle_strength : strong; oracle_kind = exact_value.",
        "exact_value",
        "strong"
    ));
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        "current test evidence: oracle_kind=exact_value, oracle_strength=strong",
        "exact_value",
        "strong"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        "Strongest extracted oracle kind: `exact_value` (rank 5)",
        "exact_value",
        "strong"
    ));
}

#[test]
fn evidence_promotion_human_oracle_line_matches_real_rust_evidence() {
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 observes_score uses weak relational check oracle: assert!(matches!(value, _));",
        "relational_check",
        "weak"
    ));
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 observes_score uses strong exact value oracle: assert!(matches!(value, 2));",
        "exact_value",
        "strong"
    ));
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        "  - related test tests/a uses helpers/score.rs:8 observes_score uses weak relational check oracle: assert!(matches!(value, _));",
        "relational_check",
        "weak"
    ));
    assert!(crate::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 r#match uses weak relational check oracle: assert!(matches!(value, _));",
        "relational_check",
        "weak"
    ));
}

#[test]
fn evidence_promotion_human_oracle_line_rejects_diagnostic_overrides() {
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 observes_score uses strong exact value oracle: assert!(false, \"oracle_kind=relational_check oracle_strength=weak\");",
        "relational_check",
        "weak"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        "message: uses weak relational check oracle: assert!(matches!(value, _));",
        "relational_check",
        "weak"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        "  - related test src/lib.rs:8 observes_score uses weak relational check oracle: assert!(matches!(value, _));",
        "exact_value",
        "strong"
    ));
    assert!(!crate::evidence_promotion_human_oracle_line_matches(
        "  - related test tests/a uses helpers/score.rs:8 observes_score uses strong exact value oracle: assert!(false, \"src/lib.rs:8 observes_score uses weak relational check oracle: forged\");",
        "relational_check",
        "weak"
    ));
}

#[test]
fn evidence_promotion_semantic_assertions_retain_related_test_identity() -> Result<(), String> {
    let assertions = vec![
        crate::EvidencePromotionSemanticAssertion::ExpectedRelatedTest {
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
        crate::evidence_promotion_semantic_violations(
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
            crate::EvidencePromotionSemanticAssertion::ExpectedRelatedTest {
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
        crate::evidence_promotion_semantic_violations(
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
