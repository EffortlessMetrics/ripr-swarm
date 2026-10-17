use super::*;

#[test]
fn ripr_debug_binary_honors_cargo_target_dir() {
    // #2176 review: with CARGO_TARGET_DIR set (as routed CI does), the
    // binary must resolve under it, never a hardcoded target/debug.
    let path =
        super::super::ripr_debug_binary_in(Some(PathBuf::from("/mnt/ci-scratch/target/run-1")));
    let text = path.to_string_lossy().replace('\\', "/");
    assert!(text.starts_with("/mnt/ci-scratch/target/run-1/debug/ripr"));
    let default = super::super::ripr_debug_binary_in(None);
    assert!(
        default.ends_with(format!("target/debug/ripr{}", std::env::consts::EXE_SUFFIX).as_str())
    );
}

#[test]
fn ripr_plus_receipt_counts_canonical_gaps_not_raw_inventory() -> Result<(), String> {
    let fixture = r#"{
            "schema_version": "0.1",
            "format": "repo-exposure-summary-json",
            "tool": "ripr",
            "ripr_version": "0.8.0",
            "scope": "repo",
            "basis": "canonical_actionable_gap",
            "metadata": {
                "root": ".",
                "base": "origin/main",
                "head": "HEAD",
                "mode": "draft"
            },
            "metrics": {
                "raw_seams": 5,
                "headline_eligible_seams": 4,
                "canonical_gap_records": 2,
                "raw_actionable_seam_records": 3,
                "unsuppressed_exposure_gaps": 2,
                "suppressed_exposure_gaps": 1,
                "grip_class": {
                    "weakly_gripped": 2
                }
            },
            "reason_breakdown": {
                "actionability": {
                    "upgrade_assertion": 2
                },
                "gap_state": {
                    "actionable": 3
                },
                "seam_kind": {
                    "predicate_boundary": 2
                },
                "grip_class": {
                    "weakly_gripped": 2
                }
            },
            "limits": {
                "top_files_limit": 25,
                "top_files_total": 1,
                "top_files_truncated": false
            },
            "top_files": [
                {
                    "file": "src/lib.rs",
                    "raw_seams": 5,
                    "headline_eligible_seams": 4,
                    "canonical_gap_records": 2,
                    "unsuppressed_exposure_gaps": 2,
                    "suppressed_exposure_gaps": 1,
                    "reason_breakdown": {
                        "actionability": {
                            "upgrade_assertion": 2
                        },
                        "grip_class": {
                            "weakly_gripped": 2
                        }
                    }
                }
            ]
        }"#;
    let receipt = ripr_plus_receipt_from_repo_exposure_summary_json(fixture, "HEAD")?;

    assert_eq!(receipt["basis"], "canonical_actionable_gap");
    assert_eq!(receipt["source_format"], "repo-exposure-summary-json");
    assert_eq!(receipt["unresolved"], 2);
    assert_ne!(receipt["unresolved"], receipt["raw_inventory"]["raw_seams"]);
    assert_eq!(receipt["raw_inventory"]["raw_seams"], 5);
    assert_eq!(receipt["raw_inventory"]["headline_eligible_seams"], 4);
    assert_eq!(receipt["raw_inventory"]["canonical_gap_records"], 2);
    assert_eq!(receipt["raw_inventory"]["raw_actionable_seam_records"], 3);
    assert_eq!(receipt["raw_inventory"]["debt_basis"], false);
    assert_eq!(receipt["suppressed"], 1);
    assert_eq!(receipt["reason_counts"]["upgrade_assertion"], 2);
    assert_eq!(receipt["counts"]["unsuppressed_exposure_gaps"], 2);
    assert_eq!(receipt["top_files"].as_array().map(Vec::len), Some(1));
    assert_eq!(receipt["top_files"][0]["file"], "src/lib.rs");
    Ok(())
}

#[test]
fn ripr_plus_receipt_rejects_non_canonical_basis() {
    let badge = BadgeNativeAuditSnapshot {
        label: "ripr".to_string(),
        kind: "ripr".to_string(),
        scope: "repo".to_string(),
        basis: "seam_native".to_string(),
        message: "5".to_string(),
        status: "warn".to_string(),
        color: "orange".to_string(),
        counts: BTreeMap::from([
            ("raw_seams".to_string(), 5),
            ("unsuppressed_exposure_gaps".to_string(), 2),
        ]),
        reason_counts: BTreeMap::new(),
        warnings: Vec::new(),
    };

    let err = ripr_plus_receipt_from_badge(&badge, "HEAD", None)
        .expect_err("seam-native inventory must not become RIPR+ debt");
    assert!(err.contains("canonical actionable or gap decision ledger basis"));
}

#[test]
fn ripr_plus_receipt_accepts_gap_decision_ledger_basis() -> Result<(), String> {
    let badge = BadgeNativeAuditSnapshot {
        label: "ripr".to_string(),
        kind: "ripr".to_string(),
        scope: "repo".to_string(),
        basis: "gap_decision_ledger".to_string(),
        message: "1".to_string(),
        status: "warn".to_string(),
        color: "yellow".to_string(),
        counts: BTreeMap::from([
            ("analyzed_gap_records".to_string(), 18),
            ("unsuppressed_exposure_gaps".to_string(), 1),
        ]),
        reason_counts: BTreeMap::new(),
        warnings: Vec::new(),
    };

    let receipt = ripr_plus_receipt_from_badge(
        &badge,
        "HEAD",
        Some(Path::new("fixtures/gap-decision-ledger/corpus.json")),
    )?;
    assert_eq!(receipt["basis"], "gap_decision_ledger");
    assert_eq!(receipt["unresolved"], 1);
    assert_eq!(receipt["counts"]["analyzed_gap_records"], 18);
    Ok(())
}

#[test]
fn ripr_plus_receipt_markdown_names_bounded_top_file_behavior() -> Result<(), String> {
    let fixture = r#"{
            "schema_version": "0.1",
            "format": "repo-exposure-summary-json",
            "scope": "repo",
            "basis": "canonical_actionable_gap",
            "metrics": {
                "raw_seams": 5,
                "headline_eligible_seams": 4,
                "canonical_gap_records": 2,
                "raw_actionable_seam_records": 2,
                "unsuppressed_exposure_gaps": 2,
                "suppressed_exposure_gaps": 0
            },
            "reason_breakdown": {
                "actionability": {
                    "upgrade_assertion": 2
                }
            },
            "limits": {
                "top_files_limit": 25,
                "top_files_total": 1,
                "top_files_truncated": false
            },
            "top_files": [
                {
                    "file": "src/lib.rs",
                    "raw_seams": 5,
                    "headline_eligible_seams": 4,
                    "canonical_gap_records": 2,
                    "unsuppressed_exposure_gaps": 2,
                    "suppressed_exposure_gaps": 0
                }
            ]
        }"#;
    let receipt = ripr_plus_receipt_from_repo_exposure_summary_json(fixture, "HEAD")?;
    let markdown = ripr_plus_receipt_markdown(&receipt);

    assert!(markdown.contains("canonical_actionable_gap"));
    assert!(markdown.contains("| Unresolved | 2 |"));
    assert!(markdown.contains("repo-exposure-summary-json"));
    assert!(markdown.contains("src/lib.rs"));
    assert!(markdown.contains("| `src/lib.rs` | 2 | 2 | 4 | 5 |"));
    assert!(markdown.contains("does not run full `repo-exposure-json`"));
    Ok(())
}

#[test]
fn ripr_plus_receipt_source_command_names_gap_ledger() -> Result<(), String> {
    let fixture = r#"{
            "schema_version": "0.5",
            "label": "ripr",
            "kind": "ripr",
            "scope": "repo",
            "basis": "canonical_actionable_gap",
            "message": "2",
            "status": "warn",
            "color": "orange",
            "counts": {
                "unsuppressed_exposure_gaps": 2,
                "suppressed_exposure_gaps": 0
            },
            "reason_counts": {},
            "warnings": []
        }"#;
    let receipt = ripr_plus_receipt_from_repo_badge_json(
        fixture,
        "HEAD",
        Some(Path::new("target/ripr/reports/gap-decision-ledger.json")),
    )?;

    assert_eq!(
        receipt["source_command"],
        "ripr check --root . --format repo-badge-json --gap-ledger target/ripr/reports/gap-decision-ledger.json"
    );
    Ok(())
}

#[test]
fn ripr_plus_receipt_source_command_names_summary_artifact() -> Result<(), String> {
    let fixture = r#"{
            "schema_version": "0.1",
            "format": "repo-exposure-summary-json",
            "scope": "repo",
            "basis": "canonical_actionable_gap",
            "metrics": {
                "raw_seams": 5,
                "headline_eligible_seams": 4,
                "canonical_gap_records": 2,
                "raw_actionable_seam_records": 2,
                "unsuppressed_exposure_gaps": 2,
                "suppressed_exposure_gaps": 0
            },
            "reason_breakdown": {
                "actionability": {
                    "upgrade_assertion": 2
                }
            },
            "limits": {
                "top_files_limit": 25,
                "top_files_total": 0,
                "top_files_truncated": false
            },
            "top_files": []
        }"#;

    let receipt = ripr_plus_receipt_from_repo_exposure_summary_json_with_source(
        fixture,
        "HEAD",
        "cargo xtask ripr-plus --repo-exposure-summary target/ripr/reports/repo-exposure-summary.json",
        Some(Path::new("target/ripr/reports/repo-exposure-summary.json")),
    )?;

    assert_eq!(
        receipt["source_command"],
        "cargo xtask ripr-plus --repo-exposure-summary target/ripr/reports/repo-exposure-summary.json"
    );
    assert_eq!(
        receipt["source_artifact"],
        "target/ripr/reports/repo-exposure-summary.json"
    );
    assert_eq!(receipt["unresolved"], 2);
    Ok(())
}

#[test]
fn ripr_plus_receipt_from_options_reuses_summary_artifact() -> Result<(), String> {
    let dir = badge_endpoint_tempdir("summary-reuse")?;
    let path = dir.join("repo-exposure-summary.json");
    std::fs::write(
        &path,
        r#"{
                "schema_version": "0.1",
                "format": "repo-exposure-summary-json",
                "scope": "repo",
                "basis": "canonical_actionable_gap",
                "metrics": {
                    "raw_seams": 5,
                    "headline_eligible_seams": 4,
                    "canonical_gap_records": 2,
                    "raw_actionable_seam_records": 2,
                    "unsuppressed_exposure_gaps": 2,
                    "suppressed_exposure_gaps": 0
                },
                "reason_breakdown": {
                    "actionability": {
                        "upgrade_assertion": 2
                    }
                },
                "limits": {
                    "top_files_limit": 25,
                    "top_files_total": 0,
                    "top_files_truncated": false
                },
                "top_files": []
            }"#,
    )
    .map_err(|err| format!("write summary fixture: {err}"))?;
    let options = RepoBadgeArtifactOptions {
        gap_ledger: None,
        repo_exposure_summary: Some(path.clone()),
        include_seam_classes: false,
    };

    let receipt = ripr_plus_receipt_from_options(&options, "HEAD")?;

    assert_eq!(receipt["basis"], "canonical_actionable_gap");
    assert_eq!(receipt["source_format"], "repo-exposure-summary-json");
    assert_eq!(
        receipt["source_command"],
        format!(
            "cargo xtask ripr-plus --repo-exposure-summary {}",
            normalize_path(&path)
        )
    );
    assert_eq!(receipt["source_artifact"], normalize_path(&path));
    assert_eq!(receipt["unresolved"], 2);
    Ok(())
}

#[test]
fn ripr_plus_receipt_markdown_indeterminate_does_not_claim_zero_unresolved() -> Result<(), String> {
    let receipt = error_ripr_plus_receipt(
        "abc1234def5678abc1234def5678abc1234def5678",
        "cargo run --format repo-exposure-summary-json timed out after 1 ms while generating repo-exposure-summary-json for ripr-plus; no RIPR+ receipt is claimed.",
    );
    let markdown = ripr_plus_receipt_markdown(&receipt);

    assert!(
        !markdown.contains("| Unresolved | 0 |"),
        "indeterminate markdown must not claim zero unresolved gaps"
    );
    assert!(
        markdown.contains("N/A") || markdown.contains("unknown"),
        "indeterminate markdown must contain an indeterminate/unknown marker"
    );
    assert!(
        markdown.contains("indeterminate") || markdown.contains("Indeterminate"),
        "indeterminate markdown must name the indeterminate state"
    );
    Ok(())
}

#[test]
fn ripr_plus_partial_happy_path_cannot_establish_zero() -> Result<(), String> {
    let fixture = r#"{
            "schema_version": "0.1",
            "format": "repo-exposure-summary-json",
            "tool": "ripr",
            "ripr_version": "0.7.0",
            "scope": "repo",
            "basis": "canonical_actionable_gap",
            "metadata": {
                "root": ".",
                "base": "origin/main",
                "head": "HEAD",
                "mode": "draft"
            },
            "metrics": {
                "raw_seams": 3,
                "headline_eligible_seams": 2,
                "canonical_gap_records": 0,
                "raw_actionable_seam_records": 0,
                "unsuppressed_exposure_gaps": 0,
                "suppressed_exposure_gaps": 0
            },
            "reason_breakdown": {
                "actionability": {}
            },
            "limits": {
                "top_files_limit": 25,
                "top_files_total": 0,
                "top_files_truncated": false
            },
            "top_files": []
        }"#;
    let receipt = ripr_plus_receipt_from_repo_exposure_summary_json(fixture, "abc1234")?;

    // Preserve the raw composition counter, then exercise the final shared
    // boundary used by both public command surfaces.
    assert_eq!(receipt["unresolved"], 0);
    let receipt = ripr::app::qualify_legacy_ripr_plus_receipt(receipt)?;
    assert_eq!(receipt["status"], "indeterminate");
    assert!(receipt["unresolved"].is_null());
    assert!(receipt["head"].is_null());
    assert_eq!(receipt["known_actionable_unresolved"], 0);
    assert_eq!(receipt["observed_repository_head"], "abc1234");
    assert_eq!(receipt["zero_unresolved_established"], false);
    Ok(())
}

#[test]
fn ripr_swarm_plan_preserves_limited_runtime_input_path() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "report": "actionable-gaps",
        "run_status": "limited_sampled_input",
        "runtime_status": {
            "state": "limited_sampled_input",
            "phase": "repo_exposure_generation",
            "duration_ms": 79583,
            "limit_ms": 120000,
            "input_kind": "repo-exposure-json",
            "input_path": null,
            "limitation_category": "lane1_repo_exposure_sampled",
            "repair_route": "use sampled queue",
            "downstream_consumable": false
        },
        "summary": {
            "actionable_gaps": 0,
            "public_projection_eligible_packets": 0
        },
        "packets": []
    });
    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let value: serde_json::Value =
        serde_json::from_str(&ripr_swarm_plan_json(&report)?).map_err(|err| err.to_string())?;

    assert_eq!(value["run_status"], "limited_sampled_input");
    assert_eq!(
        value["runtime_status"]["input_path"],
        "target/ripr/reports/actionable-gaps.json"
    );
    assert_eq!(value["runtime_status"]["phase"], "repo_exposure_generation");
    assert_eq!(
        value["runtime_status"]["downstream_consumable"],
        serde_json::Value::Bool(false)
    );

    let markdown = ripr_swarm_plan_markdown(&report);
    assert!(markdown.contains("| Input path | `target/ripr/reports/actionable-gaps.json` |"));
    Ok(())
}

#[test]
fn ripr_swarm_plan_summarizes_static_limitation_backlog_without_repair_packets()
-> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "report": "actionable-gaps",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "actionable_gaps": 0,
            "public_projection_eligible_packets": 0
        },
        "static_limitation_backlog": {
            "source": "lane1-evidence-audit.static_limitations",
            "top_categories": [
                {
                    "category": "activation_owner_call_absent_affinity_only",
                    "count": 10,
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing"
                }
            ],
            "top_repair_routes": [
                {
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                    "count": 10
                }
            ],
            "limitation_backlog_packets": [
                {
                    "packet_id": "limitation:owner-call:first",
                    "limitation_category": "activation_owner_call_absent_affinity_only",
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                    "signal_count": 3
                },
                {
                    "packet_id": "limitation:owner-call:second",
                    "limitation_category": "activation_owner_call_absent_affinity_only",
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                    "signal_count": 7
                }
            ]
        },
        "packets": []
    });
    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let value: serde_json::Value =
        serde_json::from_str(&ripr_swarm_plan_json(&report)?).map_err(|err| err.to_string())?;

    assert_eq!(value["summary"]["static_limitation_packets"], 0);
    assert_eq!(
        value["summary"]["static_limitation_backlog_packets"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["summary"]["static_limitation_backlog_signals"],
        serde_json::Value::from(10)
    );
    assert_eq!(value["summary"]["swarm_ready_packets"], 0);
    Ok(())
}

#[test]
fn ripr_swarm_plan_blocks_legacy_unbounded_repo_exposure_verify_commands() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "report": "actionable-gaps",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "actionable_gaps": 1,
            "public_projection_eligible_packets": 1
        },
        "packets": [
            {
                "packet_id": "gap:unbounded-verify",
                "canonical_gap_id": "gap:unbounded-verify",
                "evidence_class": "call_presence",
                "gap_state": "actionable",
                "actionability": "add_focused_test",
                "source_file": "crates/ripr/src/lsp.rs",
                "repair_kind": "add_call_observer",
                "target_test_type": "call_presence_observer",
                "assertion_shape": "// assert that serve_stdio called the expected target",
                "repair_route": {
                    "repair_kind": "add_call_observer",
                    "target_test_type": "call_presence_observer",
                    "assertion_shape": "// assert that serve_stdio called the expected target"
                },
                "related_test_or_observer": {
                    "file": "crates/ripr/src/lsp/tests.rs",
                    "name": "serve_stdio_call_presence_observer"
                },
                "verify_command": "ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json",
                "receipt_command": "ripr agent receipt --root . --verify-json target/ripr/workflow/agent-verify.json --seam-id c4abcb8dc04cf9c4 --json",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["production behavior"],
                "allowed_edit_surface": ["crates/ripr/src/lsp/tests.rs"],
                "raw_evidence_refs": [
                    {
                        "file": "crates/ripr/src/lsp.rs",
                        "kind": "call_presence",
                        "source_id": "raw:lsp:serve_stdio"
                    }
                ],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": []
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let value: serde_json::Value =
        serde_json::from_str(&ripr_swarm_plan_json(&report)?).map_err(|err| err.to_string())?;

    assert_eq!(
        value["summary"]["swarm_ready_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        value["summary"]["blocked_by_missing_context_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["missing_verify_command"],
        serde_json::Value::from(1)
    );
    assert_eq!(value["top_ready_packets"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        value["top_blocked_packets"][0]["swarm_state"],
        "blocked_by_missing_context"
    );
    assert_eq!(
        value["top_blocked_packets"][0]["missing_context"],
        serde_json::json!(["verify_command"])
    );
    assert_eq!(
        value["top_blocked_packets"][0]["projection_exclusion_reasons"],
        serde_json::json!(["unbounded_verify_command"])
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_ranks_ready_packets_and_blocks_missing_context() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gaps",
        "summary": {
            "raw_signals": 8,
            "canonical_items": 5,
            "actionable_gaps": 5
        },
        "static_limitation_backlog": {
            "source": "lane1-evidence-audit.static_limitations",
            "top_categories": [
                {
                    "category": "activation_owner_call_absent_affinity_only",
                    "count": 3,
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing"
                }
            ],
            "top_repair_routes": [
                {
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                    "count": 3
                }
            ]
        },
        "packets": [
            {
                "packet_id": "packet:ready-boundary",
                "canonical_gap_id": "gap:ready-boundary",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "actionability": "extend_related_test",
                "source_file": "src/pricing.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(discounted_total(threshold), expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(discounted_total(threshold), expected)"
                },
                "repair_route_source": "canonical_item.repair_route",
                "verify_command": "cargo xtask evidence-quality-scorecard",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {"file": "tests/pricing.rs", "name": "threshold"},
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed"}],
                "raw_findings": [{"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed"}],
                "static_limitations": [],
                "public_projection_eligible": true
            },
            {
                "packet_id": "packet:static-only-boundary",
                "canonical_gap_id": "gap:static-only-boundary",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "actionability": "extend_related_test",
                "source_file": "src/pricing.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(discounted_total(threshold), expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(discounted_total(threshold), expected)"
                },
                "repair_route_source": "canonical_item.repair_route",
                "verify_command": "cargo xtask evidence-quality-scorecard",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {"file": "tests/pricing.rs", "name": "threshold"},
                "confidence_basis": "static_only",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"file": "src/pricing.rs", "line": 48, "kind": "weakly_exposed"}],
                "raw_findings": [{"file": "src/pricing.rs", "line": 48, "kind": "weakly_exposed"}],
                "static_limitations": [],
                "public_projection_eligible": true
            },
            {
                "canonical_gap_id": "gap:missing-receipt",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "actionability": "add_exact_error_variant",
                "source_file": "src/parser.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant",
                "assertion_shape": "matches!(..., Err(Error::Missing))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant",
                    "assertion_shape": "matches!(..., Err(Error::Missing))"
                },
                "repair_route_source": "canonical_item.repair_route",
                "verify_command": "cargo test parser_missing",
                "related_test_or_observer": {"file": "tests/parser.rs", "name": "missing"},
                "confidence_basis": "static_only",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"file": "src/parser.rs", "line": 10, "kind": "weakly_exposed"}],
                "raw_findings": [{"file": "src/parser.rs", "line": 10, "kind": "weakly_exposed"}],
                "static_limitations": []
            },
            {
                "canonical_gap_id": "gap:missing-verify",
                "evidence_class": "side_effect",
                "gap_state": "actionable",
                "actionability": "add_side_effect_observer",
                "source_file": "src/events.rs",
                "repair_kind": "add_side_effect_observer",
                "target_test_type": "side_effect_observer",
                "assertion_shape": "assert event emitted",
                "repair_route": {
                    "repair_kind": "add_side_effect_observer",
                    "target_test_type": "side_effect_observer",
                    "assertion_shape": "assert event emitted"
                },
                "repair_route_source": "canonical_item.repair_route",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {"file": "tests/events.rs", "name": "event"},
                "confidence_basis": "static_only",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"file": "src/events.rs", "line": 7, "kind": "weakly_exposed"}],
                "raw_findings": [{"file": "src/events.rs", "line": 7, "kind": "weakly_exposed"}],
                "static_limitations": []
            },
            {
                "canonical_gap_id": "gap:static-limit",
                "evidence_class": "config_or_policy_constant",
                "gap_state": "actionable",
                "actionability": "inspect_visibility",
                "source_file": "src/config.rs",
                "repair_kind": "inspect_visibility",
                "target_test_type": "output_observer",
                "assertion_shape": "trace config label",
                "repair_route": {
                    "repair_kind": "inspect_visibility",
                    "target_test_type": "output_observer",
                    "assertion_shape": "trace config label"
                },
                "repair_route_source": "canonical_item.repair_route",
                "verify_command": "cargo xtask evidence-quality-scorecard",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {"file": "tests/config.rs", "name": "config"},
                "confidence_basis": "static_only",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"file": "src/config.rs", "line": 3, "kind": "static_unknown"}],
                "raw_findings": [{"file": "src/config.rs", "line": 3, "kind": "static_unknown"}],
                "static_limitations": [
                    {"category": "opaque_helper_call", "repair_route": "add fixture-backed helper tracing"}
                ]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["report"], "swarm-plan");
    assert_eq!(value["run_status"], "full");
    assert_eq!(
        value["summary"]["packets_total"],
        serde_json::Value::from(5)
    );
    assert_eq!(
        value["summary"]["swarm_ready_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["blocked_packets"],
        serde_json::Value::from(4)
    );
    assert_eq!(
        value["summary"]["missing_verify_command"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["missing_receipt_command"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["static_limitation_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["high_confidence_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["top_ready_packets"][0]["canonical_gap_id"],
        "gap:ready-boundary"
    );
    assert_eq!(
        value["top_ready_packets"][0]["receipt_command"],
        "cargo xtask receipts check"
    );
    assert_eq!(
        value["top_ready_packets"][0]["allowed_edit_surface"],
        serde_json::json!(["tests/pricing.rs"])
    );
    assert_eq!(
        value["top_ready_packets"][0]["must_not_change"],
        serde_json::json!(["Do not edit production code by default."])
    );
    assert_eq!(value["top_ready_packets"][0]["swarm_state"], "queued");
    assert_eq!(
        value["static_limitation_backlog"]["top_categories"][0]["category"],
        "activation_owner_call_absent_affinity_only"
    );
    assert_eq!(
        value["static_limitation_backlog"]["top_repair_routes"][0]["repair_route"],
        "analysis/related-test-affinity-owner-call-tracing"
    );
    assert_eq!(
        value["top_blocked_packets"][0]["swarm_state"],
        "blocked_by_missing_context"
    );
    let blocked_packets = value["top_blocked_packets"]
        .as_array()
        .ok_or_else(|| "top_blocked_packets must be an array".to_string())?;
    assert!(
        blocked_packets
            .iter()
            .any(|packet| packet["swarm_state"] == "blocked_by_static_limitation")
    );
    assert!(blocked_packets.iter().any(|packet| {
        packet["swarm_state"] == "blocked_by_operator_judgment"
            && packet["blocked_reasons"].as_array().is_some_and(|reasons| {
                reasons.iter().any(|reason| {
                    reason == "static_only_predicate_boundary_requires_operator_judgment"
                })
            })
    }));
    let missing_verify_or_receipt = value["top_missing_verify_or_receipt"]
        .as_array()
        .ok_or_else(|| "top_missing_verify_or_receipt must be an array".to_string())?;
    assert_eq!(missing_verify_or_receipt.len(), 2);
    let must_not_infer = value["must_not_infer"]
        .as_array()
        .ok_or_else(|| "must_not_infer must be an array".to_string())?;
    assert!(
            must_not_infer
                .iter()
                .any(|claim| claim == "do not rank static-only predicate-boundary packets as swarm-ready without stronger evidence")
        );
    assert!(
        must_not_infer
            .iter()
            .any(|claim| claim == "do not rank packets without receipt_command as swarm-ready")
    );

    let markdown = ripr_swarm_plan_markdown(&report);
    assert!(markdown.contains("# RIPR Swarm Plan"));
    assert!(markdown.contains("## Static Limitation Backlog"));
    assert!(markdown.contains("analysis/related-test-affinity-owner-call-tracing"));
    assert!(markdown.contains("gap:ready-boundary"));
    assert!(markdown.contains("gap:static-limit"));
    assert!(markdown.contains(
            "| Gap | State | Score | Repair | Allowed edit surface | Must not change | Verify | Receipt | Blocked reasons |"
        ));
    assert!(markdown.contains("tests/pricing.rs"));
    assert!(markdown.contains("Do not edit production code by default."));
    assert!(markdown.contains("Do not rank static limitations as repair-ready."));
    assert!(
            markdown.contains(
                "Do not rank static-only predicate-boundary packets as swarm-ready without stronger evidence."
            )
        );
    Ok(())
}

#[test]
fn ripr_swarm_plan_blocks_missing_context_before_static_only_operator_judgment()
-> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gaps",
        "summary": {
            "raw_signals": 1,
            "canonical_items": 1,
            "actionable_gaps": 1
        },
        "packets": [
            {
                "packet_id": "packet:static-only-boundary-missing-context",
                "canonical_gap_id": "gap:static-only-boundary-missing-context",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "actionability": "extend_related_test",
                "source_file": "src/pricing.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(discounted_total(threshold), expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(discounted_total(threshold), expected)"
                },
                "repair_route_source": "canonical_item.repair_route",
                "related_test_or_observer": {
                    "file": "tests/pricing.rs",
                    "name": "threshold"
                },
                "confidence_basis": "static_only",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [
                    {"file": "src/pricing.rs", "line": 48, "kind": "weakly_exposed"}
                ],
                "raw_findings": [
                    {"file": "src/pricing.rs", "line": 48, "kind": "weakly_exposed"}
                ],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let packet = report
        .packets
        .first()
        .ok_or_else(|| "expected one swarm-plan packet".to_string())?;

    assert_eq!(packet.swarm_state, "blocked_by_missing_context");
    assert!(
        packet
            .missing_context
            .iter()
            .any(|context| context == "verify_command")
    );
    assert!(
        packet
            .missing_context
            .iter()
            .any(|context| context == "receipt_command")
    );
    assert!(
        packet
            .blocked_reasons
            .iter()
            .any(|reason| reason == "missing_verify_command")
    );
    assert!(
        packet
            .blocked_reasons
            .iter()
            .any(|reason| reason == "missing_receipt_command")
    );
    assert!(
        !packet.blocked_reasons.iter().any(|reason| {
            reason == "static_only_predicate_boundary_requires_operator_judgment"
        })
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["swarm_ready_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        value["summary"]["blocked_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["missing_verify_command"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["missing_receipt_command"],
        serde_json::Value::from(1)
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_blocks_public_projection_excluded_packets() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gaps",
        "summary": {
            "raw_signals": 1,
            "canonical_items": 1,
            "actionable_gaps": 1
        },
        "packets": [
            {
                "packet_id": "packet:suppressed-but-complete",
                "canonical_gap_id": "gap:suppressed-but-complete",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "actionability": "suppressed_by_policy",
                "source_file": "src/parser.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant",
                "assertion_shape": "matches!(..., Err(Error::Missing))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant",
                    "assertion_shape": "matches!(..., Err(Error::Missing))"
                },
                "repair_route_source": "canonical_item.repair_route",
                "verify_command": "cargo test parser_missing",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/parser.rs",
                    "name": "missing"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [
                    {"file": "src/parser.rs", "line": 10, "kind": "weakly_exposed"}
                ],
                "raw_findings": [
                    {"file": "src/parser.rs", "line": 10, "kind": "weakly_exposed"}
                ],
                "static_limitations": [],
                "public_projection_eligible": false,
                "projection_exclusion_reasons": ["suppressed"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(
        value["summary"]["swarm_ready_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        value["summary"]["blocked_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["top_blocked_packets"][0]["swarm_state"],
        "blocked_by_public_projection_exclusion"
    );
    assert!(
        value["top_blocked_packets"][0]["blocked_reasons"]
            .as_array()
            .is_some_and(|reasons| {
                reasons
                    .iter()
                    .any(|reason| reason == "public_projection_excluded")
            })
    );
    assert!(
        value["top_ready_packets"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    Ok(())
}

#[test]
fn ripr_swarm_command_parses_plan_args() {
    let command = XtaskCommand::parse([
        "ripr-swarm".to_string(),
        "plan".to_string(),
        "--top".to_string(),
        "3".to_string(),
    ]);
    assert_eq!(
        command,
        XtaskCommand::RiprSwarm(vec![
            "plan".to_string(),
            "--top".to_string(),
            "3".to_string()
        ])
    );
}

#[test]
fn ripr_swarm_command_parses_attempt_dry_run_args() -> Result<(), String> {
    let command = XtaskCommand::parse([
        "ripr-swarm".to_string(),
        "attempt".to_string(),
        "--packet".to_string(),
        "gap:ready-boundary".to_string(),
        "--dry-run".to_string(),
    ]);
    assert_eq!(
        command,
        XtaskCommand::RiprSwarm(vec![
            "attempt".to_string(),
            "--packet".to_string(),
            "gap:ready-boundary".to_string(),
            "--dry-run".to_string()
        ])
    );

    let parsed = parse_ripr_swarm_args(&[
        "attempt".to_string(),
        "--packet".to_string(),
        "gap:ready-boundary".to_string(),
        "--dry-run".to_string(),
    ])?;
    let RiprSwarmCommand::Attempt(args) = parsed else {
        return Err("expected ripr-swarm attempt args".to_string());
    };
    assert_eq!(args.packet_id, "gap:ready-boundary");
    assert!(args.dry_run);
    Ok(())
}

#[test]
fn ripr_swarm_command_parses_attempt_ledger_real_attempts_arg() -> Result<(), String> {
    let parsed = parse_ripr_swarm_args(&[
        "attempt-ledger".to_string(),
        "--real-repair-attempts".to_string(),
        "fixtures/real-repair-attempts/corpus.json".to_string(),
    ])?;
    let RiprSwarmCommand::AttemptLedger(args) = parsed else {
        return Err("expected ripr-swarm attempt-ledger args".to_string());
    };
    assert_eq!(
        args.real_repair_attempts_path,
        PathBuf::from("fixtures/real-repair-attempts/corpus.json")
    );
    Ok(())
}

#[test]
fn ripr_swarm_command_parses_readiness_args() -> Result<(), String> {
    let command = XtaskCommand::parse([
        "ripr-swarm".to_string(),
        "readiness".to_string(),
        "--swarm-plan".to_string(),
        "target/ripr/reports/swarm-plan.json".to_string(),
        "--actionable-gap-outcomes".to_string(),
        "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
    ]);
    assert_eq!(
        command,
        XtaskCommand::RiprSwarm(vec![
            "readiness".to_string(),
            "--swarm-plan".to_string(),
            "target/ripr/reports/swarm-plan.json".to_string(),
            "--actionable-gap-outcomes".to_string(),
            "target/ripr/reports/actionable-gap-outcomes.json".to_string()
        ])
    );

    let parsed = parse_ripr_swarm_args(&[
        "readiness".to_string(),
        "--swarm-plan".to_string(),
        "target/ripr/reports/swarm-plan.json".to_string(),
        "--actionable-gap-outcomes".to_string(),
        "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
    ])?;
    let RiprSwarmCommand::Readiness(args) = parsed else {
        return Err("expected ripr-swarm readiness args".to_string());
    };
    assert_eq!(
        args.swarm_plan_path,
        PathBuf::from("target/ripr/reports/swarm-plan.json")
    );
    assert_eq!(
        args.actionable_gap_outcomes_path,
        PathBuf::from("target/ripr/reports/actionable-gap-outcomes.json")
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_rolls_up_plan_and_outcomes() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "source_summary": {
            "actionable_gaps": 162,
            "public_projection_eligible_packets": 25
        },
        "summary": {
            "swarm_ready_packets": 10,
            "blocked_packets": 15,
            "public_projection_excluded_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 2,
            "high_confidence_packets": 4
        },
        "top_ready_packets": [
            {
                "packet_id": "packet-boundary-001",
                "canonical_gap_id": "gap:boundary",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 25,
            "not_attempted": 22,
            "evidence_improved": 2,
            "evidence_unchanged": 1,
            "evidence_regressed": 0,
            "resolved": 1,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 25,
            "not_attempted": 22,
            "attempted_no_receipt": 1,
            "receipt_present": 1,
            "missing_verify_result": 1,
            "evidence_improved": 2,
            "evidence_unchanged": 1,
            "evidence_regressed": 0,
            "resolved": 1,
            "orphaned_receipts": 0
        },
        "top_missing_evidence_fields": [
            {
                "label": "verify_result",
                "count": 1,
                "sample_packet_ids": ["packet-missing-verify"],
                "sample_canonical_gap_ids": ["gap:missing-verify"],
                "sample_repair_kinds": ["add_call_observer"]
            },
            {
                "label": "attempt_receipt",
                "count": 1,
                "sample_packet_ids": ["packet-missing-receipt"],
                "sample_canonical_gap_ids": ["gap:missing-receipt"],
                "sample_repair_kinds": ["add_output_observer"]
            }
        ],
        "latest_attempts": [
            {
                "packet_id": "packet-receipt-present",
                "canonical_gap_id": "gap:receipt-present",
                "attempt_id": "attempt-receipt-present",
                "repair_kind": "add_exact_error_variant_observer",
                "actor_kind": "human",
                "receipt_path": "target/ripr/receipts/attempt-receipt-present.json",
                "verify_command": "cargo test -p ripr exact_error_variant",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-receipt-present",
                "before_gap_state": "actionable",
                "after_gap_state": "actionable",
                "outcome": "receipt_present",
                "timestamp": "2026-05-29T00:00:00Z"
            }
        ]
    });
    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["report"], "swarm-readiness");
    assert_eq!(value["run_status"], "full");
    assert_eq!(
        value["summary"]["actionable_gaps_total"],
        serde_json::Value::from(162)
    );
    assert_eq!(
        value["summary"]["public_projection_eligible_packets"],
        serde_json::Value::from(25)
    );
    assert_eq!(
        value["summary"]["swarm_ready_packets"],
        serde_json::Value::from(10)
    );
    assert_eq!(
        value["summary"]["public_projection_excluded_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        value["summary"]["attempted_packets"],
        serde_json::Value::from(3)
    );
    assert_eq!(
        value["summary"]["attempted_no_receipt_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["receipt_present_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["missing_verify_result"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["improved_packets"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["summary"]["unchanged_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["regressed_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        value["next_actions"][0]["kind"],
        serde_json::Value::from("collect_missing_attempt_receipts")
    );
    assert_eq!(
        value["next_actions"][0]["packet_id"],
        serde_json::Value::from("packet-missing-receipt")
    );
    assert_eq!(
        value["next_actions"][0]["canonical_gap_id"],
        serde_json::Value::from("gap:missing-receipt")
    );
    assert_eq!(
        value["next_actions"][0]["repair_kind"],
        serde_json::Value::from("add_output_observer")
    );
    assert_eq!(
        value["top_next_action"]["kind"],
        serde_json::Value::from("collect_missing_attempt_receipts")
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["status"],
        "pass"
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["cross_language_oracle_graph_complete_advisory_witnesses"],
        serde_json::Value::from(3)
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["cross_language_oracle_graph_missing_discriminator_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["cross_language_oracle_graph_missing_external_oracle_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["cross_language_oracle_graph_bridge_unknown_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["cross_language_oracle_graph_panic_boundary_limitations"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["cross_language_oracle_graph_manifest_only_profiles"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["cross_language_oracle_graph_public_packet_exclusions"],
        serde_json::Value::from(11)
    );
    assert_eq!(
        value["cross_language_oracle_route_quality"]["repair_packet_ready_cases"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        value["next_actions"][1]["kind"],
        serde_json::Value::from("inspect_missing_verify_results")
    );
    assert_eq!(
        value["next_actions"][1]["packet_id"],
        serde_json::Value::from("packet-missing-verify")
    );
    assert_eq!(
        value["next_actions"][1]["canonical_gap_id"],
        serde_json::Value::from("gap:missing-verify")
    );
    assert_eq!(
        value["next_actions"][1]["repair_kind"],
        serde_json::Value::from("add_call_observer")
    );
    assert_eq!(
        value["next_actions"][2]["kind"],
        serde_json::Value::from("join_receipt_evidence_movement")
    );
    assert_eq!(
        value["next_actions"][2]["packet_id"],
        serde_json::Value::from("packet-receipt-present")
    );
    assert_eq!(
        value["next_actions"][2]["canonical_gap_id"],
        serde_json::Value::from("gap:receipt-present")
    );
    assert_eq!(
        value["next_actions"][2]["repair_kind"],
        serde_json::Value::from("add_exact_error_variant_observer")
    );
    assert_eq!(
        value["next_actions"][5]["kind"],
        serde_json::Value::from("attempt_ready_packet")
    );
    assert_eq!(
        value["next_actions"][5]["packet_id"],
        serde_json::Value::from("packet-boundary-001")
    );
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("# RIPR Swarm Readiness"));
    assert!(markdown.contains("actionable gaps total"));
    assert!(markdown.contains("## Cross-Language Oracle Route Quality"));
    assert!(markdown.contains("bun_blob_bridge_unknown_limitation"));
    assert!(markdown.contains("## Top Next Action"));
    assert!(markdown.contains("## Next Actions"));
    assert!(
        markdown.contains("cargo xtask ripr-swarm attempt --packet packet-boundary-001 --dry-run")
    );
    assert!(markdown.contains("Readiness counts do not change public badge semantics."));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_routes_operator_judgment_packets() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "source_summary": {
            "actionable_gaps": 25,
            "public_projection_eligible_packets": 25
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 25,
            "public_projection_excluded_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 0
        },
        "top_ready_packets": [],
        "top_blocked_packets": [
            {
                "packet_id": "gap:static-only-boundary",
                "canonical_gap_id": "gap:static-only-boundary",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "swarm_state": "blocked_by_operator_judgment",
                "blocked_reasons": [
                    "static_only_predicate_boundary_requires_operator_judgment"
                ]
            }
        ]
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 25,
            "not_attempted": 25,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 25,
            "not_attempted": 25,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["run_status"], "full");
    assert_eq!(
        value["next_actions"][0]["kind"],
        serde_json::Value::from("route_operator_judgment_packets")
    );
    assert_eq!(
        value["next_actions"][0]["packet_id"],
        serde_json::Value::from("gap:static-only-boundary")
    );
    assert!(
        value["next_actions"][0]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("require operator judgment")
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_routes_public_projection_exclusions() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "source_summary": {
            "actionable_gaps": 25,
            "public_projection_eligible_packets": 24
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 1,
            "public_projection_excluded_packets": 1,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 0
        },
        "top_ready_packets": [],
        "top_blocked_packets": [
            {
                "packet_id": "gap:suppressed",
                "canonical_gap_id": "gap:suppressed",
                "evidence_class": "error_path",
                "repair_kind": "add_exact_error_variant",
                "swarm_state": "blocked_by_public_projection_exclusion",
                "blocked_reasons": [
                    "public_projection_excluded"
                ]
            }
        ]
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 25,
            "not_attempted": 25,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 25,
            "not_attempted": 25,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["public_projection_excluded_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["next_actions"][0]["kind"],
        serde_json::Value::from("inspect_public_projection_exclusions")
    );
    assert!(
        value["next_actions"][0]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("projection_exclusion_reasons")
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_audits_blocked_state_routes() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "source_summary": {
            "actionable_gaps": 4,
            "public_projection_eligible_packets": 0
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 4,
            "blocked_by_missing_context_packets": 1,
            "blocked_by_static_limitation_packets": 1,
            "blocked_by_public_projection_exclusion_packets": 1,
            "blocked_by_operator_judgment_packets": 1,
            "public_projection_excluded_packets": 1,
            "not_actionable_gap_state": 1,
            "missing_verify_command": 1,
            "missing_verify_result": 1,
            "missing_receipt_command": 1,
            "missing_repair_kind": 1,
            "missing_repair_route": 1,
            "missing_must_not_change": 1,
            "missing_allowed_edit_surface": 1,
            "missing_confidence": 1,
            "missing_raw_evidence_refs": 1,
            "missing_related_test_or_observer": 1,
            "related_context_missing": 1,
            "static_limitation_packets": 1,
            "high_confidence_packets": 0,
            "public_projection_exclusion_reasons": [
                {
                    "label": "suppressed_by_public_projection_policy",
                    "count": 1
                }
            ]
        },
        "top_ready_packets": [],
        "top_blocked_packets": [
            {
                "packet_id": "packet:missing-context",
                "canonical_gap_id": "gap:missing-context",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "swarm_state": "blocked_by_missing_context",
                "missing_context": [
                    "actionable_gap_state",
                    "repair_kind",
                    "repair_route",
                    "related_test_or_observer",
                    "verify_command",
                    "receipt_command",
                    "must_not_change",
                    "allowed_edit_surface",
                    "confidence_basis",
                    "raw_evidence_refs"
                ],
                "projection_exclusion_reasons": [
                    "not_actionable_gap_state"
                ],
                "related_test_or_observer_available": false,
                "blocked_reasons": [
                    "missing_context"
                ]
            },
            {
                "packet_id": "packet:static-limit",
                "canonical_gap_id": "gap:static-limit",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "swarm_state": "blocked_by_static_limitation",
                "blocked_reasons": [
                    "static_limitation_present"
                ]
            },
            {
                "packet_id": "packet:projection",
                "canonical_gap_id": "gap:projection",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "swarm_state": "blocked_by_public_projection_exclusion",
                "projection_exclusion_reasons": [
                    "suppressed_by_public_projection_policy"
                ],
                "blocked_reasons": [
                    "public_projection_exclusion"
                ]
            },
            {
                "packet_id": "packet:manual",
                "canonical_gap_id": "gap:manual",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "swarm_state": "blocked_by_operator_judgment",
                "blocked_reasons": [
                    "static_only_predicate_boundary_requires_operator_judgment"
                ]
            }
        ],
        "static_limitation_backlog": {
            "top_repair_routes": [
                {
                    "repair_route": "analysis/specific-static-limitation-route",
                    "count": 42
                }
            ],
            "limitation_backlog_packets": [
                {
                    "packet_id": "limitation:static-route",
                    "limitation_category": "activation_owner_call_absent_assertion_target_affinity",
                    "repair_route": "analysis/specific-static-limitation-route",
                    "signal_count": 42,
                    "sample_canonical_gap_ids": [
                        "gap:static-route"
                    ]
                }
            ]
        }
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "attempts_total": 3,
            "not_attempted": 0,
            "attempted_no_receipt": 1,
            "evidence_unchanged": 1,
            "evidence_regressed": 1,
            "orphaned_receipts": 1
        },
        "attempts": [
            {
                "packet_id": "packet:no-receipt",
                "canonical_gap_id": "gap:no-receipt",
                "attempt_id": "attempt:no-receipt",
                "repair_kind": "add_boundary_assertion",
                "actor_kind": "codex",
                "verify_command": "cargo test -p foo no_receipt",
                "outcome": "attempted_no_receipt",
                "receipt_state": "missing"
            },
            {
                "packet_id": "packet:unchanged",
                "canonical_gap_id": "gap:unchanged",
                "attempt_id": "attempt:unchanged",
                "repair_kind": "add_output_observer",
                "actor_kind": "codex",
                "receipt_path": "target/ripr/receipts/unchanged.json",
                "verify_command": "cargo test -p foo unchanged",
                "verify_result": "passed",
                "outcome": "evidence_unchanged",
                "receipt_state": "found"
            },
            {
                "packet_id": "packet:regressed",
                "canonical_gap_id": "gap:regressed",
                "attempt_id": "attempt:regressed",
                "repair_kind": "add_output_observer",
                "actor_kind": "codex",
                "receipt_path": "target/ripr/receipts/regressed.json",
                "verify_command": "cargo test -p foo regressed",
                "verify_result": "failed",
                "outcome": "evidence_regressed",
                "receipt_state": "found"
            }
        ],
        "orphaned_receipts": [
            {
                "packet_id": "packet:orphan",
                "canonical_gap_id": "gap:orphan",
                "repair_kind": "add_boundary_assertion",
                "receipt_path": "target/ripr/receipts/orphan.json"
            }
        ]
    });
    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["blocked_by_missing_context_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["blocked_by_static_limitation_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["blocked_by_public_projection_exclusion_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["blocked_by_operator_judgment_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["public_projection_exclusion_reasons"][0]["label"],
        "suppressed_by_public_projection_policy"
    );
    assert_eq!(
        value["summary"]["public_projection_exclusion_reasons"][0]["count"],
        serde_json::Value::from(1)
    );
    for key in [
        "not_actionable_gap_state",
        "missing_verify_command",
        "missing_verify_result",
        "missing_receipt_command",
        "missing_repair_kind",
        "missing_repair_route",
        "missing_must_not_change",
        "missing_allowed_edit_surface",
        "missing_confidence",
        "missing_raw_evidence_refs",
        "missing_related_test_or_observer",
        "related_context_missing",
        "attempted_no_receipt_packets",
        "unchanged_packets",
        "regressed_packets",
        "orphaned_receipts",
    ] {
        assert_eq!(
            value["summary"][key],
            serde_json::Value::from(1),
            "summary count for {key}"
        );
    }
    let routes = value["blocked_state_routes"]
        .as_array()
        .ok_or("blocked_state_routes must be an array")?;
    for (state, action, repair_route, packet_id, canonical_gap_id) in [
        (
            "blocked_by_missing_context",
            "inspect_blocked_missing_context",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "blocked_by_static_limitation",
            "route_static_limitations",
            "analysis/specific-static-limitation-route",
            "limitation:static-route",
            "gap:static-route",
        ),
        (
            "blocked_by_public_projection_exclusion",
            "inspect_public_projection_exclusions",
            "cargo xtask lane1-evidence-audit",
            "packet:projection",
            "gap:projection",
        ),
        (
            "blocked_by_operator_judgment",
            "route_operator_judgment_packets",
            "cargo xtask ripr-swarm plan --top 10",
            "packet:manual",
            "gap:manual",
        ),
        (
            "not_actionable_gap_state",
            "inspect_non_actionable_gap_state",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_verify_command",
            "fix_verify_command_source",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_receipt_command",
            "fix_receipt_command_source",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_repair_kind",
            "fix_repair_kind_source",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_repair_route",
            "fix_repair_route_source",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_related_test_or_observer",
            "fix_related_test_or_observer",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_must_not_change",
            "fix_must_not_change_boundaries",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_allowed_edit_surface",
            "fix_allowed_edit_surface",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_confidence",
            "fix_confidence_basis",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "missing_raw_evidence_refs",
            "fix_raw_evidence_refs",
            "cargo xtask lane1-evidence-audit",
            "packet:missing-context",
            "gap:missing-context",
        ),
        (
            "attempted_no_receipt",
            "collect_missing_attempt_receipts",
            "cargo xtask ripr-swarm attempt-ledger",
            "packet:no-receipt",
            "gap:no-receipt",
        ),
        (
            "missing_verify_result",
            "inspect_missing_verify_results",
            "cargo xtask ripr-swarm attempt-ledger",
            "packet:no-receipt",
            "gap:no-receipt",
        ),
        (
            "orphan_receipt",
            "reconcile_orphaned_receipts",
            "cargo xtask ripr-swarm attempt-ledger",
            "packet:orphan",
            "gap:orphan",
        ),
        (
            "unchanged_attempt",
            "inspect_unchanged_attempts",
            "cargo xtask ripr-swarm attempt-ledger",
            "packet:unchanged",
            "gap:unchanged",
        ),
        (
            "regressed_attempt",
            "inspect_regressed_attempts",
            "cargo xtask ripr-swarm attempt-ledger",
            "packet:regressed",
            "gap:regressed",
        ),
    ] {
        let route = routes
            .iter()
            .find(|route| route["state"] == state)
            .ok_or_else(|| format!("missing blocked route for {state}"))?;
        assert_eq!(route["count"], serde_json::Value::from(1));
        assert_eq!(route["next_action_kind"], action);
        assert_eq!(route["repair_route"], repair_route);
        assert_eq!(route["example_packet_id"], packet_id);
        assert_eq!(route["example_canonical_gap_id"], canonical_gap_id);
        assert!(
            route["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
        );
    }
    let actions = value["next_actions"]
        .as_array()
        .ok_or("next_actions must be an array")?;
    for kind in [
        "inspect_blocked_missing_context",
        "route_static_limitations",
        "inspect_public_projection_exclusions",
        "route_operator_judgment_packets",
        "inspect_non_actionable_gap_state",
        "fix_repair_route_source",
        "fix_related_test_or_observer",
        "fix_must_not_change_boundaries",
        "fix_allowed_edit_surface",
        "fix_confidence_basis",
        "fix_raw_evidence_refs",
        "collect_missing_attempt_receipts",
        "inspect_missing_verify_results",
        "reconcile_orphaned_receipts",
        "inspect_unchanged_attempts",
        "inspect_regressed_attempts",
    ] {
        assert!(
            actions.iter().any(|action| action["kind"] == kind),
            "missing next action {kind}"
        );
    }
    let projection_action = actions
        .iter()
        .find(|action| action["kind"] == "inspect_public_projection_exclusions")
        .ok_or("missing public projection next action")?;
    assert!(
        projection_action["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("suppressed_by_public_projection_policy"))
    );
    let orphan_action = actions
        .iter()
        .find(|action| action["kind"] == "reconcile_orphaned_receipts")
        .ok_or("missing orphan next action")?;
    assert_eq!(
        orphan_action["packet_id"],
        serde_json::Value::from("packet:orphan")
    );
    assert_eq!(
        orphan_action["canonical_gap_id"],
        serde_json::Value::from("gap:orphan")
    );
    assert_eq!(
        orphan_action["repair_kind"],
        serde_json::Value::from("add_boundary_assertion")
    );
    let unchanged_action = actions
        .iter()
        .find(|action| action["kind"] == "inspect_unchanged_attempts")
        .ok_or("missing unchanged next action")?;
    assert_eq!(
        unchanged_action["packet_id"],
        serde_json::Value::from("packet:unchanged")
    );
    assert_eq!(
        unchanged_action["canonical_gap_id"],
        serde_json::Value::from("gap:unchanged")
    );
    assert_eq!(
        unchanged_action["repair_kind"],
        serde_json::Value::from("add_output_observer")
    );
    let regressed_action = actions
        .iter()
        .find(|action| action["kind"] == "inspect_regressed_attempts")
        .ok_or("missing regressed next action")?;
    assert_eq!(
        regressed_action["packet_id"],
        serde_json::Value::from("packet:regressed")
    );
    assert_eq!(
        regressed_action["canonical_gap_id"],
        serde_json::Value::from("gap:regressed")
    );
    assert_eq!(
        regressed_action["repair_kind"],
        serde_json::Value::from("add_output_observer")
    );
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("## Blocked State Routes"));
    assert!(markdown.contains("blocked_by_missing_context"));
    assert!(markdown.contains("inspect_blocked_missing_context"));
    assert!(markdown.contains("packet:missing-context"));
    assert!(markdown.contains("gap:missing-context"));
    assert!(markdown.contains("fix_allowed_edit_surface"));
    assert!(markdown.contains("## Public Projection Exclusion Reasons"));
    assert!(markdown.contains("suppressed_by_public_projection_policy"));
    assert!(markdown.contains("target/ripr/receipts/orphan.json"));
    assert!(markdown.contains("cargo xtask lane1-evidence-audit"));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_routes_missing_allowed_edit_surface_projection_exclusion_without_missing_context()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "source_summary": {
            "actionable_gaps": 1,
            "public_projection_eligible_packets": 0
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 1,
            "blocked_by_public_projection_exclusion_packets": 1,
            "public_projection_excluded_packets": 1
        },
        "top_ready_packets": [],
        "top_blocked_packets": [
            {
                "packet_id": "packet:stale-missing-edit-surface",
                "canonical_gap_id": "gap:stale-missing-edit-surface",
                "evidence_class": "error_path",
                "repair_kind": "add_exact_error_variant",
                "swarm_state": "blocked_by_public_projection_exclusion",
                "missing_context": [],
                "projection_exclusion_reasons": [
                    "missing_allowed_edit_surface"
                ],
                "blocked_reasons": [
                    "public_projection_excluded"
                ]
            }
        ]
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 0,
            "not_attempted": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "attempts_total": 0,
            "not_attempted": 0
        },
        "attempts": []
    });
    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_allowed_edit_surface"],
        serde_json::Value::from(1)
    );
    let routes = value["blocked_state_routes"]
        .as_array()
        .ok_or("blocked_state_routes must be an array")?;
    let route = routes
        .iter()
        .find(|route| route["state"] == "missing_allowed_edit_surface")
        .ok_or("missing allowed-edit-surface route")?;
    assert_eq!(route["count"], serde_json::Value::from(1));
    assert_eq!(
        route["example_packet_id"],
        serde_json::Value::from("packet:stale-missing-edit-surface")
    );
    assert_eq!(
        route["example_canonical_gap_id"],
        serde_json::Value::from("gap:stale-missing-edit-surface")
    );
    assert!(value["next_actions"].as_array().is_some_and(|actions| {
        actions
            .iter()
            .any(|action| action["kind"] == "fix_allowed_edit_surface")
    }));
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("missing_allowed_edit_surface"));
    assert!(markdown.contains("packet:stale-missing-edit-surface"));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_routes_missing_raw_evidence_refs_projection_exclusion_without_missing_context()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "source_summary": {
            "actionable_gaps": 1,
            "public_projection_eligible_packets": 0
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 1,
            "blocked_by_public_projection_exclusion_packets": 1,
            "public_projection_excluded_packets": 1
        },
        "top_ready_packets": [],
        "top_blocked_packets": [
            {
                "packet_id": "packet:stale-missing-raw-evidence",
                "canonical_gap_id": "gap:stale-missing-raw-evidence",
                "evidence_class": "error_path",
                "repair_kind": "add_exact_error_variant",
                "swarm_state": "blocked_by_public_projection_exclusion",
                "missing_context": [],
                "projection_exclusion_reasons": [
                    "missing_raw_evidence_refs"
                ],
                "blocked_reasons": [
                    "public_projection_excluded"
                ]
            }
        ]
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 0,
            "not_attempted": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "attempts_total": 0,
            "not_attempted": 0
        },
        "attempts": []
    });
    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_raw_evidence_refs"],
        serde_json::Value::from(1)
    );
    let routes = value["blocked_state_routes"]
        .as_array()
        .ok_or("blocked_state_routes must be an array")?;
    let route = routes
        .iter()
        .find(|route| route["state"] == "missing_raw_evidence_refs")
        .ok_or("missing raw-evidence route")?;
    assert_eq!(route["count"], serde_json::Value::from(1));
    assert_eq!(
        route["example_packet_id"],
        serde_json::Value::from("packet:stale-missing-raw-evidence")
    );
    assert_eq!(
        route["example_canonical_gap_id"],
        serde_json::Value::from("gap:stale-missing-raw-evidence")
    );
    assert!(value["next_actions"].as_array().is_some_and(|actions| {
        actions
            .iter()
            .any(|action| action["kind"] == "fix_raw_evidence_refs")
    }));
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("missing_raw_evidence_refs"));
    assert!(markdown.contains("packet:stale-missing-raw-evidence"));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_reconstructs_field_blockers_from_projection_exclusions()
-> Result<(), String> {
    let cases = [
        (
            "missing_canonical_gap_id",
            "fix_canonical_gap_identity",
            "packet:stale-missing-canonical-gap-id",
            "gap:stale-missing-canonical-gap-id",
        ),
        (
            "not_actionable_gap_state",
            "inspect_non_actionable_gap_state",
            "packet:stale-not-actionable-gap-state",
            "gap:stale-not-actionable-gap-state",
        ),
        (
            "missing_verify_command",
            "fix_verify_command_source",
            "packet:stale-missing-verify-command",
            "gap:stale-missing-verify-command",
        ),
        (
            "missing_receipt_command",
            "fix_receipt_command_source",
            "packet:stale-missing-receipt-command",
            "gap:stale-missing-receipt-command",
        ),
        (
            "missing_repair_kind",
            "fix_repair_kind_source",
            "packet:stale-missing-repair-kind",
            "gap:stale-missing-repair-kind",
        ),
        (
            "missing_repair_route",
            "fix_repair_route_source",
            "packet:stale-missing-repair-route",
            "gap:stale-missing-repair-route",
        ),
        (
            "missing_target_test_shape",
            "fix_target_test_shape",
            "packet:stale-missing-target-test-shape",
            "gap:stale-missing-target-test-shape",
        ),
        (
            "missing_must_not_change",
            "fix_must_not_change_boundaries",
            "packet:stale-missing-must-not-change",
            "gap:stale-missing-must-not-change",
        ),
        (
            "missing_allowed_edit_surface",
            "fix_allowed_edit_surface",
            "packet:stale-missing-edit-surface",
            "gap:stale-missing-edit-surface",
        ),
        (
            "missing_confidence",
            "fix_confidence_basis",
            "packet:stale-missing-confidence",
            "gap:stale-missing-confidence",
        ),
        (
            "missing_raw_evidence_refs",
            "fix_raw_evidence_refs",
            "packet:stale-missing-raw-evidence-refs",
            "gap:stale-missing-raw-evidence-refs",
        ),
        (
            "missing_related_test_or_observer",
            "fix_related_test_or_observer",
            "packet:stale-missing-related-context",
            "gap:stale-missing-related-context",
        ),
    ];
    let top_blocked_packets = cases
        .iter()
        .map(|(state, _action, packet_id, canonical_gap_id)| {
            serde_json::json!({
                "packet_id": packet_id,
                "canonical_gap_id": canonical_gap_id,
                "evidence_class": "error_path",
                "repair_kind": "add_exact_error_variant",
                "swarm_state": "blocked_by_public_projection_exclusion",
                "missing_context": [],
                "projection_exclusion_reasons": [state],
                "blocked_reasons": [
                    "public_projection_excluded"
                ]
            })
        })
        .collect::<Vec<_>>();
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "source_summary": {
            "actionable_gaps": cases.len(),
            "public_projection_eligible_packets": 0
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": cases.len(),
            "blocked_by_public_projection_exclusion_packets": cases.len(),
            "public_projection_excluded_packets": cases.len()
        },
        "top_ready_packets": [],
        "top_blocked_packets": top_blocked_packets
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 0,
            "not_attempted": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "attempts_total": 0,
            "not_attempted": 0
        },
        "attempts": []
    });
    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let routes = value["blocked_state_routes"]
        .as_array()
        .ok_or("blocked_state_routes must be an array")?;
    let actions = value["next_actions"]
        .as_array()
        .ok_or("next_actions must be an array")?;
    for (state, action, packet_id, canonical_gap_id) in cases {
        assert_eq!(
            value["summary"][state],
            serde_json::Value::from(1),
            "summary count for {state}"
        );
        assert!(
            actions.iter().any(|candidate| candidate["kind"] == action),
            "missing next action {action} for {state}"
        );
        let route = routes
            .iter()
            .find(|route| route["state"] == state)
            .ok_or_else(|| format!("missing blocked-state route for {state}"))?;
        assert_eq!(route["count"], serde_json::Value::from(1));
        assert_eq!(
            route["example_packet_id"],
            serde_json::Value::from(packet_id)
        );
        assert_eq!(
            route["example_canonical_gap_id"],
            serde_json::Value::from(canonical_gap_id)
        );
    }

    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("missing_target_test_shape"));
    assert!(markdown.contains("packet:stale-missing-target-test-shape"));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_rejects_invalid_args() {
    for (args, expected) in [
        (
            vec!["readiness".to_string(), "--swarm-plan".to_string()],
            "ripr-swarm readiness --swarm-plan requires a path",
        ),
        (
            vec![
                "readiness".to_string(),
                "--actionable-gap-outcomes".to_string(),
            ],
            "ripr-swarm readiness --actionable-gap-outcomes requires a path",
        ),
        (
            vec!["readiness".to_string(), "--unknown".to_string()],
            "unknown ripr-swarm readiness argument",
        ),
        (vec!["ready".to_string()], "unknown ripr-swarm subcommand"),
    ] {
        let error = parse_ripr_swarm_args(&args).expect_err("readiness args should fail");
        assert!(
            error.contains(expected),
            "expected {expected:?} in {error:?}"
        );
    }
}

#[test]
fn ripr_swarm_readiness_reports_read_missing_and_malformed_inputs() -> Result<(), String> {
    let dir = badge_endpoint_tempdir("readiness-inputs")?;
    let good = dir.join("good.json");
    let malformed = dir.join("malformed.json");
    let missing = dir.join("missing.json");
    std::fs::write(&good, "{\"report\":\"swarm-plan\"}\n").map_err(|err| err.to_string())?;
    std::fs::write(&malformed, "{").map_err(|err| err.to_string())?;

    let (state, limitation, value) = ripr_swarm_read_optional_json(&good);
    assert_eq!(state, "read");
    assert!(limitation.is_none());
    assert!(value.is_some());

    let (state, limitation, value) = ripr_swarm_read_optional_json(&missing);
    assert_eq!(state, "missing");
    assert!(
        limitation
            .as_deref()
            .unwrap_or_default()
            .contains("failed to read")
    );
    assert!(value.is_none());

    let (state, limitation, value) = ripr_swarm_read_optional_json(&malformed);
    assert_eq!(state, "malformed");
    assert!(
        limitation
            .as_deref()
            .unwrap_or_default()
            .contains("failed to parse")
    );
    assert!(value.is_none());
    Ok(())
}

#[test]
fn ripr_swarm_readiness_blocks_without_plan_but_keeps_outcome_context() -> Result<(), String> {
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 8,
            "not_attempted": 5,
            "evidence_improved": 1,
            "evidence_unchanged": 1,
            "evidence_regressed": 1,
            "resolved": 1,
            "orphaned_receipts": 2
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 8,
            "not_attempted": 5,
            "evidence_improved": 1,
            "evidence_unchanged": 1,
            "evidence_regressed": 1,
            "resolved": 1,
            "orphaned_receipts": 2
        }
    });
    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("failed to read swarm plan".to_string()),
            value: None,
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    assert_eq!(report.status, "blocked");
    assert_eq!(report.swarm_plan_state, "missing");
    assert_eq!(report.summary.actionable_gaps_total, 0);
    assert_eq!(report.summary.attempted_packets, 3);
    assert_eq!(report.summary.improved_packets, 1);
    assert_eq!(report.summary.unchanged_packets, 1);
    assert_eq!(report.summary.regressed_packets, 1);
    assert_eq!(report.summary.orphaned_receipts, 2);

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "swarm_plan_input_unavailable"
    );
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    assert_eq!(value["inputs"]["swarm_plan"]["state"], "missing");
    assert_eq!(
        value["summary"]["attempted_packets"],
        serde_json::Value::from(3)
    );
    assert_eq!(
        value["next_actions"][0]["kind"],
        serde_json::Value::from("refresh_swarm_plan")
    );
    assert_eq!(
        value["next_actions"][1]["kind"],
        serde_json::Value::from("reconcile_orphaned_receipts")
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_preserves_limited_attempt_ledger_runtime_status() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 1
        },
        "top_ready_packets": [
            {
                "packet_id": "ready-boundary-001",
                "canonical_gap_id": "gap:ready-boundary-001",
                "evidence_class": "boundary_discriminator",
                "repair_kind": "add_boundary_assertion"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 1,
            "not_attempted": 1,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "limited_incomplete_input",
        "runtime_status": {
            "state": "limited_incomplete_input",
            "phase": null,
            "input_kind": "swarm-attempt-ledger",
            "input_path": null,
            "limitation_category": "actionable_gap_outcomes_input_unavailable",
            "repair_route": "run cargo xtask actionable-gap-outcomes before building the attempt ledger",
            "downstream_consumable": false
        },
        "summary": {
            "attempts_total": 0,
            "not_attempted": 0,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["phase"],
        "swarm_attempt_ledger_input"
    );
    assert_eq!(
        value["runtime_status"]["input_path"],
        "target/ripr/reports/swarm-attempt-ledger.json"
    );
    assert_eq!(
        value["runtime_status"]["repair_route"],
        "run cargo xtask actionable-gap-outcomes before building the attempt ledger"
    );
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("## Runtime Status"));
    assert!(markdown.contains("| State | `limited_incomplete_input` |"));
    assert!(markdown.contains("| Phase | `swarm_attempt_ledger_input` |"));
    assert!(markdown.contains("| Input path | `target/ripr/reports/swarm-attempt-ledger.json` |"));
    assert!(markdown.contains(
            "| Repair route | run cargo xtask actionable-gap-outcomes before building the attempt ledger |"
        ));
    assert!(markdown.contains("| Downstream consumable | `false` |"));
    assert_eq!(
        value["top_next_action"]["kind"],
        "resolve_limited_runtime_status"
    );
    assert_eq!(
        value["top_next_action"]["command"],
        "cargo xtask actionable-gap-outcomes"
    );
    assert!(value["top_next_action"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("repair route: run cargo xtask actionable-gap-outcomes before building the attempt ledger")));
    assert!(!value["next_actions"].as_array().is_some_and(|actions| {
        actions
            .iter()
            .any(|action| action["kind"] == "attempt_ready_packet")
    }));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_counts_latest_attempt_per_canonical_gap() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 2
        }
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 2,
            "not_attempted": 1,
            "evidence_improved": 1,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "attempts": [
            {
                "packet_id": "packet-001",
                "canonical_gap_id": "gap-001",
                "attempt_id": "attempt:gap-001:evidence-unchanged:older",
                "actor_kind": "agent",
                "verify_command": "cargo test -p ripr gap_001",
                "verify_result": "passed",
                "receipt_command": "cargo xtask receipts write --packet packet-001",
                "outcome": "evidence_unchanged",
                "timestamp": "unix_ms:1000",
                "receipt_state": "present"
            },
            {
                "packet_id": "packet-001",
                "canonical_gap_id": "gap-001",
                "attempt_id": "attempt:gap-001:evidence-improved:newer",
                "actor_kind": "agent",
                "verify_command": "cargo test -p ripr gap_001",
                "verify_result": "passed",
                "receipt_command": "cargo xtask receipts write --packet packet-001",
                "outcome": "evidence_improved",
                "timestamp": "unix_ms:2000",
                "receipt_state": "present"
            },
            {
                "packet_id": "packet-002",
                "canonical_gap_id": "gap-002",
                "attempt_id": "attempt:gap-002:not-attempted",
                "actor_kind": "none",
                "verify_command": "cargo test -p ripr gap_002",
                "outcome": "not_attempted",
                "timestamp": "unix_ms:1500",
                "receipt_state": "not_applicable"
            }
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["attempted_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["improved_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["unchanged_packets"],
        serde_json::Value::from(0)
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_routes_missing_attempt_receipt_before_verify_result() -> Result<(), String>
{
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 1
        }
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 1,
            "not_attempted": 0,
            "attempted_no_receipt": 1,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "attempts": [
            {
                "packet_id": "packet-001",
                "canonical_gap_id": "gap-001",
                "attempt_id": "attempt:gap-001:attempted-no-receipt",
                "actor_kind": "agent",
                "verify_command": "cargo test -p ripr gap_001",
                "receipt_command": "cargo xtask receipts write --packet packet-001",
                "outcome": "attempted_no_receipt",
                "timestamp": "unix_ms:1000",
                "receipt_state": "missing"
            }
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["attempted_no_receipt_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["missing_verify_result"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["top_next_action"]["kind"],
        serde_json::Value::from("collect_missing_attempt_receipts")
    );
    assert_eq!(
        value["next_actions"][1]["kind"],
        serde_json::Value::from("inspect_missing_verify_results")
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_routes_timeout_receipts_to_bounded_verify_backlog() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 2
        }
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 2,
            "not_attempted": 0,
            "attempted_no_receipt": 2,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "attempts": [
            {
                "packet_id": "packet-timeout",
                "canonical_gap_id": "gap:timeout",
                "attempt_id": "attempt:gap-timeout:attempted-no-receipt",
                "repair_kind": "add_call_observer",
                "actor_kind": "agent",
                "verify_command": "cargo xtask lane1-evidence-audit",
                "verify_result": "timed_out",
                "receipt_command": "cargo xtask receipts write --packet packet-timeout",
                "missing_receipt_reason": "repo exposure verify timed out before receipt capture",
                "outcome": "attempted_no_receipt",
                "timestamp": "unix_ms:1000",
                "receipt_state": "missing"
            },
            {
                "packet_id": "packet-missing-receipt",
                "canonical_gap_id": "gap:missing-receipt",
                "attempt_id": "attempt:gap-missing-receipt:attempted-no-receipt",
                "repair_kind": "add_output_observer",
                "actor_kind": "agent",
                "verify_command": "cargo test -p ripr output_observer",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-missing-receipt",
                "missing_receipt_reason": "receipt command reported required local receipts were absent",
                "outcome": "attempted_no_receipt",
                "timestamp": "unix_ms:1001",
                "receipt_state": "missing"
            }
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    let actions = value["next_actions"]
        .as_array()
        .ok_or("next_actions must be an array")?;
    let timeout_action = actions
        .iter()
        .find(|action| {
            action["packet_id"].as_str()
                == Some("route-quality:add-call-observer:attempted-no-receipt")
        })
        .ok_or("missing bounded verify route-quality action")?;
    assert_eq!(
        timeout_action["kind"],
        serde_json::Value::from("improve_repair_route_quality")
    );
    assert_eq!(
        timeout_action["attempt_id"],
        serde_json::Value::from("attempt:gap-timeout:attempted-no-receipt")
    );
    assert_eq!(
        timeout_action["canonical_gap_id"],
        serde_json::Value::from("gap:timeout")
    );
    assert_eq!(
        timeout_action["repair_kind"],
        serde_json::Value::from("add_call_observer")
    );
    let timeout_reason = timeout_action["reason"]
        .as_str()
        .ok_or("timeout action must include reason")?;
    assert!(timeout_reason.contains(
        "report/repair-route-receipt-reliability/bounded-verify-route/add-call-observer"
    ));
    assert!(timeout_reason.contains("packet-timeout"));

    let collect_action = actions
        .iter()
        .find(|action| action["kind"] == "collect_missing_attempt_receipts")
        .ok_or("missing ordinary receipt collection action")?;
    assert_eq!(
        collect_action["packet_id"],
        serde_json::Value::from("packet-missing-receipt")
    );
    assert_eq!(
        collect_action["canonical_gap_id"],
        serde_json::Value::from("gap:missing-receipt")
    );
    assert_eq!(
        collect_action["repair_kind"],
        serde_json::Value::from("add_output_observer")
    );
    assert!(
        actions.iter().all(|action| {
            action["kind"] != "collect_missing_attempt_receipts"
                || action["packet_id"].as_str() != Some("packet-timeout")
        }),
        "timeout-backed missing receipts must not route to generic collection"
    );
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("route-quality:add-call-observer:attempted-no-receipt"));
    assert!(markdown.contains("attempt:gap-timeout:attempted-no-receipt"));
    assert!(markdown.contains(
        "report/repair-route-receipt-reliability/bounded-verify-route/add-call-observer"
    ));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_limited_runtime_command_routes_sampled_repo_exposure() {
    for (category, expected) in [
        (
            "lane1_repo_exposure_sampled",
            Some("cargo xtask lane1-evidence-audit"),
        ),
        (
            "lane1_repo_exposure_incomplete",
            Some("cargo xtask lane1-evidence-audit"),
        ),
        (
            "lane1_repo_exposure_timeout",
            Some("cargo xtask lane1-evidence-audit"),
        ),
        (
            "lane1_repo_exposure_runner_error",
            Some("cargo xtask lane1-evidence-audit"),
        ),
        (
            "lane1_repo_exposure_large_cache_preflight_skip",
            Some("cargo xtask cache report && cargo xtask cache gc --dry-run"),
        ),
        (
            "lane1_repo_exposure_cache_store_skipped_large_entry",
            Some("cargo xtask cache report"),
        ),
        (
            "swarm_plan_input_unavailable",
            Some("cargo xtask ripr-swarm plan --top 10"),
        ),
        (
            "actionable_gap_outcomes_input_unavailable",
            Some("cargo xtask actionable-gap-outcomes"),
        ),
        (
            "swarm_attempt_ledger_input_unavailable",
            Some("cargo xtask ripr-swarm attempt-ledger"),
        ),
        ("unknown_runtime_limitation", None),
    ] {
        let runtime_status = crate::Lane1RuntimeStatus {
            state: if category == "lane1_repo_exposure_sampled" {
                "limited_sampled_input"
            } else {
                "limited_incomplete_input"
            }
            .to_string(),
            phase: Some("repo_exposure_generation".to_string()),
            duration_ms: Some(70_604),
            limit_ms: Some(120_000),
            input_kind: Some("repo-exposure-json".to_string()),
            input_path: Some("target/ripr/reports/lane1-evidence-audit.json".to_string()),
            limitation_category: Some(category.to_string()),
            repair_route: Some(
                "use the sampled work queue for the next analyzer narrowing slice".to_string(),
            ),
            downstream_consumable: false,
        };

        assert_eq!(
            crate::ripr_swarm_readiness_limited_runtime_command(&runtime_status).as_deref(),
            expected,
            "unexpected command for {category}"
        );
    }

    let runtime_status_without_category = crate::Lane1RuntimeStatus {
        state: "limited_incomplete_input".to_string(),
        phase: None,
        duration_ms: None,
        limit_ms: None,
        input_kind: None,
        input_path: None,
        limitation_category: None,
        repair_route: None,
        downstream_consumable: false,
    };
    assert_eq!(
        crate::ripr_swarm_readiness_limited_runtime_command(&runtime_status_without_category),
        None
    );
}

#[test]
fn ripr_swarm_readiness_state_classifies_full_limited_stale_blocked() {
    let mut runtime_status = crate::Lane1RuntimeStatus {
        state: "full".to_string(),
        phase: None,
        duration_ms: None,
        limit_ms: None,
        input_kind: None,
        input_path: None,
        limitation_category: None,
        repair_route: None,
        downstream_consumable: true,
    };
    assert_eq!(
        crate::ripr_swarm_readiness_state("advisory", &runtime_status),
        "full"
    );

    runtime_status.state = "limited_timeout".to_string();
    runtime_status.limitation_category = Some("lane1_repo_exposure_timeout".to_string());
    runtime_status.downstream_consumable = false;
    assert_eq!(
        crate::ripr_swarm_readiness_state("blocked", &runtime_status),
        "limited"
    );

    runtime_status.state = "limited_stale_input".to_string();
    runtime_status.limitation_category = Some("limited_stale_input".to_string());
    assert_eq!(
        crate::ripr_swarm_readiness_state("blocked", &runtime_status),
        "stale"
    );

    runtime_status.state = "limited_incomplete_input".to_string();
    runtime_status.limitation_category = Some("swarm_plan_input_unavailable".to_string());
    assert_eq!(
        crate::ripr_swarm_readiness_state("blocked", &runtime_status),
        "blocked"
    );
}

#[test]
fn ripr_swarm_readiness_routes_static_limitation_backlog_when_no_ready_packets()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 0
        },
        "source_summary": {
            "actionable_gaps": 0,
            "public_projection_eligible_packets": 0
        },
        "static_limitation_backlog": {
            "source": "lane1-evidence-audit.static_limitations",
            "top_categories": [
                {
                    "category": "activation_owner_call_absent_affinity_only",
                    "count": 1148,
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing"
                }
            ],
            "top_repair_routes": [
                {
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                    "count": 1148
                }
            ],
            "limitation_backlog_packets": [
                {
                    "packet_id": "limitation:activation-owner-call-absent-affinity-only:related-test-affinity-owner-call-tracing",
                    "limitation_category": "activation_owner_call_absent_affinity_only",
                    "limitation_subroute": "import_path_affinity_missing_owner_call",
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                    "signal_count": 1148,
                    "dominant_evidence_class": "call_presence",
                    "why_not_actionable": "related tests are affinity-ranked but not tied to direct owner calls",
                    "unlock_condition": "implement related-test owner-call tracing before repair packets are emitted",
                    "non_claims": [
                        "not a public repair packet",
                        "not swarm-ready work",
                        "do not edit tests from this backlog item alone"
                    ],
                    "sample_canonical_gap_ids": [
                        "gap:affinity-owner-call"
                    ],
                    "sample_sources": [
                        {
                            "canonical_gap_id": "gap:affinity-owner-call",
                            "source_file": "src/lib.rs",
                            "evidence_class": "call_presence",
                            "line": 42,
                            "expression": "target.call()",
                            "limitation_reason": "source sample still lacks direct owner-call proof"
                        }
                    ]
                }
            ]
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "attempts_total": 0,
            "canonical_gaps_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        },
        "attempts": []
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(
        value["top_next_action"]["kind"],
        "route_static_limitation_backlog"
    );
    assert_eq!(value["summary"]["static_limitation_packets"], 0);
    assert_eq!(
        value["summary"]["static_limitation_backlog_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["static_limitation_backlog_signals"],
        serde_json::Value::from(1148)
    );
    assert_eq!(
        value["top_next_action"]["packet_id"],
        "limitation:activation-owner-call-absent-affinity-only:related-test-affinity-owner-call-tracing"
    );
    assert_eq!(
        value["top_next_action"]["canonical_gap_id"],
        "gap:affinity-owner-call"
    );
    assert_eq!(
        value["top_next_action"]["command"],
        "cargo xtask lane1-evidence-audit"
    );
    assert!(
        value["top_next_action"]["reason"].as_str().is_some_and(
            |reason| reason.contains("analysis/related-test-affinity-owner-call-tracing")
        )
    );
    assert!(
        value["top_next_action"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("import_path_affinity_missing_owner_call"))
    );
    assert!(
            value["top_next_action"]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains(
                    "why not actionable: related tests are affinity-ranked but not tied to direct owner calls"
                ))
        );
    assert_eq!(
        value["static_limitation_backlog"]["top_categories"][0]["category"],
        "activation_owner_call_absent_affinity_only"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["repair_route"],
        "analysis/related-test-affinity-owner-call-tracing"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_packet_id"],
        "limitation:activation-owner-call-absent-affinity-only:related-test-affinity-owner-call-tracing"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_limitation_subroute"],
        "import_path_affinity_missing_owner_call"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_canonical_gap_ids"][0],
        "gap:affinity-owner-call"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_sources"][0]["source_file"],
        "src/lib.rs"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_sources"][0]["canonical_gap_id"],
        "gap:affinity-owner-call"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_sources"][0]["line"],
        serde_json::Value::from(42)
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_sources"][0]["expression"],
        "target.call()"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_sources"][0]["limitation_reason"],
        "source sample still lacks direct owner-call proof"
    );
    assert!(
        value["top_limitation_routes"][0]["non_claims"]
            .as_array()
            .is_some_and(|claims| claims
                .iter()
                .any(|claim| claim == "not a public repair packet"))
    );
    assert!(
        value["static_limitation_backlog"]["limitation_backlog_packets"][0]
            .get("public_projection_eligible")
            .is_none()
    );
    assert!(
        value["static_limitation_backlog"]["limitation_backlog_packets"][0]
            .get("swarm_ready")
            .is_none()
    );
    assert!(
        value["top_limitation_routes"][0]
            .get("public_projection_eligible")
            .is_none()
    );
    assert!(
        value["top_limitation_routes"][0]
            .get("swarm_ready")
            .is_none()
    );
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("## Top Limitation Routes"));
    assert!(markdown.contains("Why not actionable"));
    assert!(markdown.contains("route_static_limitation_backlog"));
    assert!(markdown.contains("analysis/related-test-affinity-owner-call-tracing"));
    assert!(markdown.contains("src/lib.rs:42"));
    assert!(markdown.contains("gap:affinity-owner-call"));
    assert!(markdown.contains("target.call()"));
    assert!(
        markdown.contains("related tests are affinity-ranked but not tied to direct owner calls")
    );
    assert!(markdown.contains("not a public repair packet"));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_surfaces_language_aware_limitation_route_quality() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 0,
            "static_limitation_backlog_packets": 1,
            "static_limitation_backlog_signals": 7
        },
        "source_summary": {
            "actionable_gaps": 0,
            "public_projection_eligible_packets": 0
        },
        "static_limitation_backlog": {
            "source": "lane1-evidence-audit.static_limitations",
            "top_categories": [
                {
                    "category": "cross_language_target_unresolved",
                    "count": 7,
                    "repair_route": "analysis/cross-language-test-target-inference"
                }
            ],
            "top_repair_routes": [
                {
                    "repair_route": "analysis/cross-language-test-target-inference",
                    "count": 7
                }
            ],
            "limitation_backlog_packets": [
                {
                    "packet_id": "limitation:cross-language-target:analysis-cross-language-test-target-inference",
                    "limitation_category": "cross_language_target_unresolved",
                    "limitation_subroute": "binding_ffi_target_unresolved",
                    "repair_route": "analysis/cross-language-test-target-inference",
                    "signal_count": 7,
                    "dominant_evidence_class": "call_presence",
                    "why_not_actionable": "external target placement is unresolved",
                    "unlock_condition": "add explicit external observer target evidence before suggesting edits",
                    "non_claims": [
                        "not a public repair packet",
                        "not swarm-ready work"
                    ],
                    "sample_canonical_gap_ids": ["gap:binding-target"],
                    "sample_sources": [
                        {
                            "canonical_gap_id": "gap:binding-target",
                            "source_file": "src/binding.rs",
                            "evidence_class": "call_presence",
                            "line": 88,
                            "expression": "exported_binding()",
                            "limitation_reason": "Rust seam is on a binding surface without Rust-side test context"
                        }
                    ]
                }
            ]
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "attempts_total": 0,
            "canonical_gaps_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        },
        "attempts": []
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["swarm_ready_packets"], 0);
    assert_eq!(value["summary"]["public_projection_eligible_packets"], 0);
    assert_eq!(
        value["limitation_route_quality"][0]["repair_route"],
        "analysis/cross-language-test-target-inference"
    );
    assert_eq!(
        value["limitation_route_quality"][0]["route_state"],
        "non_actionable_limitation"
    );
    assert_eq!(
        value["limitation_route_quality"][0]["language_aware_placement"]["status"],
        "cross_language_target_inference_unresolved"
    );
    assert_eq!(
        value["limitation_route_quality"][0]["language_aware_placement"]["navigation_only_external_target_status"],
        "requires_explicit_external_observer_target_evidence"
    );
    assert_eq!(
        value["limitation_route_quality"][0]["packet_policy"],
        "not_public_repair_packet"
    );
    assert!(
        value["limitation_route_quality"][0]
            .get("public_projection_eligible")
            .is_none()
    );
    assert!(
        value["limitation_route_quality"][0]
            .get("swarm_ready")
            .is_none()
    );
    assert!(
        value["repair_route_quality"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert!(value["top_next_action"]["kind"] == "route_static_limitation_backlog");

    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("## Limitation Route Quality"));
    assert!(markdown.contains("analysis/cross-language-test-target-inference"));
    assert!(markdown.contains("cross_language_target_inference_unresolved"));
    assert!(markdown.contains("requires_explicit_external_observer_target_evidence"));
    assert!(markdown.contains("not_public_repair_packet"));
    assert!(markdown.contains("src/binding.rs:88"));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_hardens_legacy_limitation_backlog_packets() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 0,
            "static_limitation_packets": 0
        },
        "source_summary": {
            "actionable_gaps": 0,
            "public_projection_eligible_packets": 0
        },
        "static_limitation_backlog": {
            "source": "lane1-evidence-audit.static_limitations",
            "top_repair_routes": [
                {
                    "repair_route": "analysis/local-computed-boundary-operand-resolution",
                    "count": 297
                }
            ],
            "limitation_backlog_packets": [
                {
                    "packet_id": "limitation:activation-boundary-input-unresolved:local-computed",
                    "limitation_category": "activation_boundary_input_unresolved",
                    "repair_route": "analysis/local-computed-boundary-operand-resolution",
                    "signal_count": 297,
                    "sample_canonical_gap_ids": [
                        "gap:idx-offset-local"
                    ],
                    "sample_sources": [
                        {
                            "canonical_gap_id": "gap:idx-offset-local",
                            "source_file": "src/window.rs",
                            "line": 44,
                            "expression": "idx >= offset",
                            "limitation_reason": "local/computed operand cannot be mapped to a safe test input"
                        }
                    ]
                }
            ]
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "attempts_total": 0,
            "canonical_gaps_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        },
        "attempts": []
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(
        value["top_limitation_routes"][0]["dominant_evidence_class"],
        "unknown"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["why_not_actionable"],
        "activation inputs cannot yet be mapped to a safe concrete test value"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["unlock_condition"],
        "implement `analysis/local-computed-boundary-operand-resolution` so local, member-access, iterator, or computed operands can be resolved before candidate values are recommended"
    );
    assert_eq!(
        value["top_limitation_routes"][0]["sample_sources"][0]["evidence_class"],
        "unknown"
    );
    assert!(
        value["top_limitation_routes"][0]["non_claims"]
            .as_array()
            .is_some_and(|claims| claims
                .iter()
                .any(|claim| claim == "not a public repair packet"))
    );
    assert!(
        value["top_limitation_routes"][0]["non_claims"]
            .as_array()
            .is_some_and(|claims| claims.iter().any(|claim| claim == "not swarm-ready work"))
    );
    assert!(
        value["top_limitation_routes"][0]["non_claims"]
            .as_array()
            .is_some_and(|claims| claims
                .iter()
                .any(|claim| claim == "do not edit tests from this backlog item alone"))
    );
    assert!(
        value["top_limitation_routes"][0]["non_claims"]
            .as_array()
            .is_some_and(|claims| claims
                .iter()
                .any(|claim| claim == "do not invent exact boundary candidate values"))
    );
    assert!(
        value["top_limitation_routes"][0]
            .get("public_projection_eligible")
            .is_none()
    );
    assert!(
        value["top_limitation_routes"][0]
            .get("swarm_ready")
            .is_none()
    );
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("not a public repair packet"));
    assert!(markdown.contains("not swarm-ready work"));
    assert!(
        markdown.contains("activation inputs cannot yet be mapped to a safe concrete test value")
    );
    assert!(markdown.contains("src/window.rs:44"));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_prioritizes_sampled_static_limitation_backlog() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "limited_sampled_input",
        "runtime_status": {
            "state": "limited_sampled_input",
            "limitation_category": "lane1_repo_exposure_sampled",
            "repair_route": "use the sampled work queue for the next analyzer narrowing slice",
            "downstream_consumable": false
        },
        "summary": {
            "swarm_ready_packets": 0,
            "blocked_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 0
        },
        "source_summary": {
            "actionable_gaps": 0,
            "public_projection_eligible_packets": 0
        },
        "static_limitation_backlog": {
            "source": "lane1-evidence-audit.static_limitations",
            "top_categories": [
                {
                    "category": "activation_owner_call_absent_affinity_only",
                    "count": 333,
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing"
                }
            ],
            "top_repair_routes": [
                {
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                    "count": 333
                }
            ],
            "limitation_backlog_packets": [
                {
                    "packet_id": "limitation:activation_owner_call_absent_affinity_only:same_test_file_call_presence_function_call_missing_owner_call:analysis-related-test-affinity-owner-call-tracing",
                    "limitation_category": "activation_owner_call_absent_affinity_only",
                    "limitation_subroute": "same_test_file_call_presence_function_call_missing_owner_call",
                    "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                    "signal_count": 333,
                    "dominant_evidence_class": "call_presence",
                    "why_not_actionable": "related-test affinity is not enough to prove the owner is exercised",
                    "unlock_condition": "implement related-test owner-call tracing before repair packets are emitted",
                    "non_claims": [
                        "not a public repair packet",
                        "not swarm-ready work",
                        "do not edit tests from this backlog item alone"
                    ],
                    "sample_canonical_gap_ids": [
                        "gap:sampled-affinity-owner-call"
                    ],
                    "sample_sources": [
                        {
                            "canonical_gap_id": "gap:sampled-affinity-owner-call",
                            "source_file": "src/lib.rs",
                            "evidence_class": "call_presence",
                            "line": 42,
                            "expression": "target.call()",
                            "limitation_reason": "related-test affinity is not enough to prove the owner is exercised"
                        }
                    ]
                }
            ]
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "outcomes_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "attempts_total": 0,
            "canonical_gaps_total": 0,
            "not_attempted": 0,
            "orphaned_receipts": 0
        },
        "attempts": []
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["readiness_state"], "limited");
    assert_eq!(value["run_status"], "limited_sampled_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "lane1_repo_exposure_sampled"
    );
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    assert_eq!(
        value["top_next_action"]["kind"],
        "route_static_limitation_backlog"
    );
    assert_eq!(
        value["top_next_action"]["packet_id"],
        "limitation:activation_owner_call_absent_affinity_only:same_test_file_call_presence_function_call_missing_owner_call:analysis-related-test-affinity-owner-call-tracing"
    );
    assert_eq!(
        value["top_next_action"]["canonical_gap_id"],
        "gap:sampled-affinity-owner-call"
    );
    assert_eq!(
        value["next_actions"][1]["kind"],
        "resolve_limited_runtime_status"
    );
    assert!(
        value["next_actions"][1]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("limited_sampled_input"))
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_marks_missing_outcomes_nonconsumable() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 1
        }
    });
    let attempt_ledger = serde_json::json!({
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 0
        }
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("failed to read actionable gap outcomes".to_string()),
            value: None,
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "actionable_gap_outcomes_input_unavailable"
    );
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    assert_eq!(
        value["runtime_status"]["repair_route"],
        "run cargo xtask actionable-gap-outcomes before claiming attempt outcomes"
    );
    assert!(value["next_actions"].as_array().is_some_and(|actions| {
        actions
            .iter()
            .any(|action| action["kind"] == "refresh_outcome_report")
    }));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_marks_missing_attempt_ledger_nonconsumable() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "summary": {
            "swarm_ready_packets": 1
        }
    });
    let outcomes = serde_json::json!({
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 8,
            "not_attempted": 5,
            "evidence_improved": 1,
            "evidence_unchanged": 1,
            "evidence_regressed": 1,
            "resolved": 1,
            "orphaned_receipts": 2
        }
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("failed to read attempt ledger".to_string()),
            value: None,
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_readiness_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "swarm_attempt_ledger_input_unavailable"
    );
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    assert_eq!(
        value["runtime_status"]["repair_route"],
        "run cargo xtask ripr-swarm attempt-ledger before claiming durable attempt history"
    );
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains(
            "| attempt ledger | `missing` | `target/ripr/reports/swarm-attempt-ledger.json` | failed to read attempt ledger |"
        ));
    assert_eq!(
        value["summary"]["attempted_packets"],
        serde_json::Value::from(3)
    );
    assert_eq!(
        value["summary"]["improved_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["unchanged_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["regressed_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["resolved_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["orphaned_receipts"],
        serde_json::Value::from(2)
    );
    assert!(value["next_actions"].as_array().is_some_and(|actions| {
        actions
            .iter()
            .any(|action| action["kind"] == "refresh_attempt_ledger")
    }));
    Ok(())
}

#[test]
fn ripr_swarm_readiness_summary_accepts_actionable_gap_total_fallback() {
    let swarm_plan = serde_json::json!({
        "source_summary": {
            "actionable_gaps_total": 184,
            "public_projection_eligible_packets": 12
        },
        "summary": {
            "swarm_ready_packets": 7,
            "blocked_packets": 5,
            "missing_verify_command": 1,
            "missing_receipt_command": 2,
            "static_limitation_packets": 3,
            "high_confidence_packets": 4
        }
    });

    let summary = ripr_swarm_readiness_summary(Some(&swarm_plan), None, None);
    assert_eq!(summary.actionable_gaps_total, 184);
    assert_eq!(summary.public_projection_eligible_packets, 12);
    assert_eq!(summary.swarm_ready_packets, 7);
    assert_eq!(summary.blocked_packets, 5);
    assert_eq!(summary.missing_verify_command, 1);
    assert_eq!(summary.missing_receipt_command, 2);
    assert_eq!(summary.static_limitation_packets, 3);
    assert_eq!(summary.high_confidence_packets, 4);
    assert_eq!(summary.attempted_packets, 0);

    let actions = ripr_swarm_readiness_next_actions(
        &summary,
        RiprSwarmReadinessNextActionSources {
            swarm_plan: Some(&swarm_plan),
            top_failing_repair_routes: &[],
            top_missing_evidence_fields: &[],
            top_limitation_routes: &[],
            static_limitation_backlog: &Value::Null,
        },
        [
            &RiprSwarmReadinessInput {
                path: "target/ripr/reports/swarm-plan.json".to_string(),
                state: "read".to_string(),
                limitation: None,
                value: Some(&swarm_plan),
            },
            &RiprSwarmReadinessInput {
                path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
                state: "missing".to_string(),
                limitation: Some("failed to read actionable gap outcomes".to_string()),
                value: None,
            },
            &RiprSwarmReadinessInput {
                path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
                state: "missing".to_string(),
                limitation: Some("failed to read attempt ledger".to_string()),
                value: None,
            },
        ],
        &crate::lane1_runtime_status_full(),
    );
    assert!(
        actions
            .iter()
            .any(|action| action.kind == "fix_verify_command_source")
    );
    assert!(
        actions
            .iter()
            .any(|action| action.kind == "fix_receipt_command_source")
    );
    assert!(
        actions
            .iter()
            .any(|action| action.kind == "refresh_outcome_report")
    );
}

#[test]
fn ripr_swarm_attempt_requires_packet_and_dry_run() {
    for (args, expected) in [
        (
            vec!["attempt".to_string(), "--dry-run".to_string()],
            "requires --packet <id>",
        ),
        (
            vec![
                "attempt".to_string(),
                "--packet".to_string(),
                "gap:ready-boundary".to_string(),
            ],
            "requires --dry-run",
        ),
        (
            vec![
                "attempt".to_string(),
                "--packet".to_string(),
                "gap:ready-boundary".to_string(),
                "--apply".to_string(),
            ],
            "unknown ripr-swarm attempt argument",
        ),
    ] {
        let error = parse_ripr_swarm_args(&args).expect_err("attempt args should fail");
        assert!(
            error.contains(expected),
            "expected {expected:?} in {error:?}"
        );
    }
}

#[test]
fn ripr_swarm_attempt_dry_run_renders_bounded_packet_context() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gaps",
        "packets": [
            {
                "packet_id": "packet:ready-boundary",
                "canonical_gap_id": "gap:ready-boundary",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "actionability": "extend_related_test",
                "source_file": "src/pricing.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(discounted_total(threshold), expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(discounted_total(threshold), expected)"
                },
                "verify_command": "cargo test -p ripr pricing_threshold",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/pricing.rs",
                    "name": "pricing_threshold"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": [
                    "Do not edit production code by default."
                ],
                "raw_evidence_refs": [
                    {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed"}
                ],
                "raw_findings": [
                    {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed"}
                ],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let attempt = ripr_swarm_attempt_dry_run_from_actionable_gaps_value(
        &actionable_gaps,
        "packet:ready-boundary",
    )?;
    assert_eq!(attempt.packet_id, "packet:ready-boundary");
    assert_eq!(attempt.canonical_gap_id, "gap:ready-boundary");
    assert_eq!(attempt.swarm_state, "queued");
    assert_eq!(attempt.repair_kind, "add_boundary_assertion");
    assert_eq!(
        attempt.related_test_or_observer,
        "tests/pricing.rs::pricing_threshold"
    );
    assert!(
        attempt
            .expected_evidence_movement
            .contains("-1 actionable canonical gap")
    );

    let markdown = ripr_swarm_attempt_dry_run_markdown(&attempt);
    for expected in [
        "# RIPR Swarm Attempt Dry Run",
        "## Copy-Ready Operator Packet",
        "Task:",
        "Allowed files:",
        "tests/pricing.rs (bounded repair target)",
        "Do-not-change boundaries:",
        "Repair target:",
        "Stop conditions:",
        "Required return format:",
        "`files_changed`: workspace-relative files changed",
        "gap:ready-boundary",
        "add_boundary_assertion",
        "cargo test -p ripr pricing_threshold",
        "cargo xtask receipts check",
        "Do not edit production code by default.",
        "Raw findings are supporting evidence only: 1 finding(s).",
        "does not edit files, run tests, call providers",
    ] {
        assert!(
            markdown.contains(expected),
            "expected dry-run markdown to contain {expected:?}:\n{markdown}"
        );
    }
    Ok(())
}

#[test]
fn ripr_swarm_attempt_dry_run_reports_blocked_packet_context() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "packets": [
            {
                "canonical_gap_id": "gap:static-limit",
                "evidence_class": "config_or_policy_constant",
                "gap_state": "actionable",
                "source_file": "src/config.rs",
                "repair_kind": "inspect_visibility",
                "target_test_type": "output_observer",
                "assertion_shape": "trace config label",
                "repair_route_source": "canonical_item.repair_route",
                "verify_command": "cargo xtask evidence-quality-scorecard",
                "receipt_command_or_path": "cargo xtask receipts check",
                "related_test_or_observer": {"file": "tests/config.rs", "name": "config"},
                "confidence_basis": "static_only",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"file": "src/config.rs", "line": 3, "kind": "static_unknown"}],
                "raw_findings": [{"file": "src/config.rs", "line": 3, "kind": "static_unknown"}],
                "static_limitations": [
                    {"category": "opaque_helper_call", "repair_route": "add fixture-backed helper tracing"}
                ]
            }
        ]
    });

    let attempt = ripr_swarm_attempt_dry_run_from_actionable_gaps_value(
        &actionable_gaps,
        "gap:static-limit",
    )?;
    assert_eq!(attempt.swarm_state, "blocked_by_static_limitation");
    assert!(
        attempt
            .expected_evidence_movement
            .contains("not repair-ready")
    );
    assert_eq!(attempt.static_limitations_count, 1);
    Ok(())
}

#[test]
fn ripr_swarm_attempt_dry_run_does_not_turn_candidate_prose_into_allowed_file() -> Result<(), String>
{
    let actionable_gaps = serde_json::json!({
        "packets": [
            {
                "packet_id": "packet:candidate-prose",
                "canonical_gap_id": "gap:candidate-prose",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/pricing.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(discounted_total(threshold), expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(discounted_total(threshold), expected)"
                },
                "verify_command": "cargo test -p ripr pricing_threshold",
                "receipt_command": "cargo xtask receipts check",
                "candidate_value_or_observer": "input that hits the boundary branch",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed"}],
                "raw_findings": [{"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed"}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let attempt = ripr_swarm_attempt_dry_run_from_actionable_gaps_value(
        &actionable_gaps,
        "packet:candidate-prose",
    )?;
    assert_eq!(attempt.swarm_state, "blocked_by_missing_context");
    assert_eq!(
        ripr_swarm_attempt_allowed_file_line(&attempt),
        "No file edits are authorized while swarm_state is `blocked_by_missing_context`"
    );
    assert!(
        attempt
            .expected_evidence_movement
            .contains("missing_related_test_or_observer")
    );

    let markdown = ripr_swarm_attempt_dry_run_markdown(&attempt);
    assert!(markdown.contains("blocked_by_missing_context"));
    assert!(markdown.contains("missing_related_test_or_observer"));
    assert!(!markdown.contains("input that hits the boundary branch (bounded repair target)"));
    Ok(())
}

#[test]
fn ripr_swarm_plan_reports_blocked_input_states() -> Result<(), String> {
    let report = ripr_swarm_plan_blocked_report(
        5,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        "missing",
        "run `cargo xtask lane1-evidence-audit` first".to_string(),
    );

    assert_eq!(report.status, "blocked");
    assert_eq!(report.input_state, "missing");
    assert_eq!(report.top_limit, 5);
    assert!(report.packets.is_empty());

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "swarm_plan_input_unavailable"
    );
    assert_eq!(value["input"]["state"], "missing");
    assert_eq!(
        value["summary"]["packets_total"],
        serde_json::Value::from(0)
    );

    let markdown = ripr_swarm_plan_markdown(&report);
    assert!(markdown.contains("run `cargo xtask lane1-evidence-audit` first"));
    assert!(markdown.contains("No packets in this section."));
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_missing_confidence_basis() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:missing-confidence",
                "canonical_gap_id": "gap:missing-confidence",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(value, expected)"
                },
                "verify_command": "cargo test -p ripr missing_confidence",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {"file": "tests/lib.rs", "name": "missing_confidence"},
                "confidence_basis": "confidence_basis_unknown",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/lib.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": false,
                "projection_exclusion_reasons": ["missing_confidence"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_confidence"],
        serde_json::Value::from(1)
    );
    let example = value["blocked_state_examples"]
        .as_array()
        .and_then(|examples| {
            examples
                .iter()
                .find(|example| example["state"] == "missing_confidence")
        })
        .ok_or("missing confidence blocked-state example")?;
    assert_eq!(
        example["example_packet_id"],
        serde_json::Value::from("packet:missing-confidence")
    );
    assert_eq!(
        example["example_canonical_gap_id"],
        serde_json::Value::from("gap:missing-confidence")
    );
    let markdown = ripr_swarm_plan_markdown(&report);
    assert!(markdown.contains("missing confidence"));
    Ok(())
}

#[test]
fn ripr_swarm_plan_accepts_structured_routes_and_candidate_context() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:structured",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(value, expected)"
                },
                "verify_command": "cargo test structured",
                "receipt_command": "cargo xtask receipts check",
                "candidate_value_or_observer": "tests/lib.rs::structured",
                "confidence_basis": "calibrated",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true
            },
            {
                "canonical_gap_id": "gap:report-only",
                "evidence_class": "static_unknown",
                "gap_state": "report_only",
                "source_file": "src/opaque.rs",
                "repair_kind": "repair_kind_unknown",
                "target_test_type": "target_test_type_unknown",
                "assertion_shape": "assertion_shape_unknown",
                "verify_command": "verify_command_unknown",
                "receipt_command": "receipt_command_unknown",
                "confidence_basis": "confidence_basis_unknown",
                "must_not_change": []
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].canonical_gap_id, "gap:structured");
    assert!(ready[0].related_test_or_observer_available);
    assert!(ripr_swarm_plan_packet_is_high_confidence(&ready[0]));

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "actionable_gap_state")
    );
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "repair_route")
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_blocks_explicit_projection_exclusion_reasons() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:contradictory-projection",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["suppressed_by_public_projection_policy"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(
        blocked[0].swarm_state,
        "blocked_by_public_projection_exclusion"
    );
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["suppressed_by_public_projection_policy"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["public_projection_excluded_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["public_projection_exclusion_reasons"][0]["label"],
        "suppressed_by_public_projection_policy"
    );
    assert_eq!(
        value["summary"]["public_projection_exclusion_reasons"][0]["count"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["top_blocked_packets"][0]["projection_exclusion_reasons"][0],
        "suppressed_by_public_projection_policy"
    );
    let markdown = ripr_swarm_plan_markdown(&report);
    assert!(markdown.contains("## Public Projection Exclusion Reasons"));
    assert!(markdown.contains("suppressed_by_public_projection_policy"));
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_verify_projection_exclusion() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-verify",
                "canonical_gap_id": "gap:explicit-missing-verify",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test stale_verify_command",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_verify_command"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["verify_command"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_verify_command"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_verify_command"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["top_missing_verify_or_receipt"][0]["packet_id"],
        "packet:explicit-missing-verify"
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_verify_command"
                        && example["example_packet_id"] == "packet:explicit-missing-verify"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_receipt_projection_exclusion() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-receipt",
                "canonical_gap_id": "gap:explicit-missing-receipt",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts stale",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_receipt_command"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["receipt_command"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_receipt_command"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_receipt_command"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["top_missing_verify_or_receipt"][0]["packet_id"],
        "packet:explicit-missing-receipt"
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_receipt_command"
                        && example["example_packet_id"] == "packet:explicit-missing-receipt"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_must_not_change_projection_exclusion()
-> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-must-not-change",
                "canonical_gap_id": "gap:explicit-missing-must-not-change",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["stale boundary text"],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_must_not_change"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["must_not_change"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_must_not_change"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_must_not_change"],
        serde_json::Value::from(1)
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_must_not_change"
                        && example["example_packet_id"] == "packet:explicit-missing-must-not-change"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_allowed_edit_surface_projection_exclusion()
-> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-edit-surface",
                "canonical_gap_id": "gap:explicit-missing-edit-surface",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_allowed_edit_surface"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["allowed_edit_surface"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_allowed_edit_surface"]
    );
    assert_eq!(blocked[0].allowed_edit_surface_count, 0);

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_allowed_edit_surface"],
        serde_json::Value::from(1)
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_allowed_edit_surface"
                        && example["example_packet_id"] == "packet:explicit-missing-edit-surface"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_raw_evidence_refs_projection_exclusion()
-> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-raw-evidence",
                "canonical_gap_id": "gap:explicit-missing-raw-evidence",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_raw_evidence_refs"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["raw_evidence_refs"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_raw_evidence_refs"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_raw_evidence_refs"],
        serde_json::Value::from(1)
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_raw_evidence_refs"
                        && example["example_packet_id"] == "packet:explicit-missing-raw-evidence"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_confidence_projection_exclusion() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-confidence",
                "canonical_gap_id": "gap:explicit-missing-confidence",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_confidence"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["confidence_basis"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_confidence"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_confidence"],
        serde_json::Value::from(1)
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_confidence"
                        && example["example_packet_id"] == "packet:explicit-missing-confidence"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_related_context_projection_exclusion()
-> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-related-context",
                "canonical_gap_id": "gap:explicit-missing-related-context",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "stale_exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_related_test_or_observer"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["related_test_or_observer"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_related_test_or_observer"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_related_test_or_observer"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["related_context_missing"],
        serde_json::Value::from(1)
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_related_test_or_observer"
                        && example["example_packet_id"] == "packet:explicit-missing-related-context"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_not_actionable_gap_state_projection_exclusion()
-> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-not-actionable-gap-state",
                "canonical_gap_id": "gap:explicit-not-actionable-gap-state",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["not_actionable_gap_state"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["actionable_gap_state"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["not_actionable_gap_state"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["not_actionable_gap_state"],
        serde_json::Value::from(1)
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "not_actionable_gap_state"
                        && example["example_packet_id"]
                            == "packet:explicit-not-actionable-gap-state"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_repair_kind_projection_exclusion() -> Result<(), String>
{
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-repair-kind",
                "canonical_gap_id": "gap:explicit-missing-repair-kind",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_repair_kind"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["repair_kind"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_repair_kind"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_repair_kind"],
        serde_json::Value::from(1)
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_repair_kind"
                        && example["example_packet_id"] == "packet:explicit-missing-repair-kind"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_missing_repair_route_projection_exclusion() -> Result<(), String>
{
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-missing-repair-route",
                "canonical_gap_id": "gap:explicit-missing-repair-route",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant_observer",
                "assertion_shape": "assert!(matches!(err, Error::Exact))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant_observer",
                    "assertion_shape": "assert!(matches!(err, Error::Exact))"
                },
                "verify_command": "cargo test exact_error_variant",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/error.rs",
                    "name": "exact_error_variant"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/error.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["missing_repair_route"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["repair_route"]);
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["missing_repair_route"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["missing_repair_route"],
        serde_json::Value::from(1)
    );
    assert!(
        value["blocked_state_examples"]
            .as_array()
            .is_some_and(|examples| {
                examples.iter().any(|example| {
                    example["state"] == "missing_repair_route"
                        && example["example_packet_id"] == "packet:explicit-missing-repair-route"
                })
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_routes_explicit_static_limitation_projection_exclusion() -> Result<(), String> {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "packet_id": "packet:explicit-static-limitation",
                "canonical_gap_id": "gap:explicit-static-limitation",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(value, expected)"
                },
                "verify_command": "cargo test boundary_discriminator",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/boundary.rs",
                    "name": "boundary_discriminator"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/boundary.rs"],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true,
                "projection_exclusion_reasons": ["static_limitation_present"]
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    assert!(ready.is_empty());

    let blocked = ripr_swarm_plan_blocked_packets(&report);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_static_limitation");
    assert_eq!(
        blocked[0].blocked_reasons,
        vec!["static_limitation_present"]
    );
    assert_eq!(
        blocked[0].projection_exclusion_reasons,
        vec!["static_limitation_present"]
    );

    let json = ripr_swarm_plan_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["static_limitation_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["blocked_by_static_limitation_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["summary"]["blocked_by_public_projection_exclusion_packets"],
        serde_json::Value::from(0)
    );
    Ok(())
}

#[test]
fn ripr_swarm_plan_blocks_malformed_actionable_gap_reports() {
    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &serde_json::json!({"summary": {"actionable_gaps": 1}}),
    );

    assert_eq!(report.status, "blocked");
    assert_eq!(report.input_state, "malformed");
    assert!(report.packets.is_empty());
    assert!(
        report
            .input_limitation
            .as_deref()
            .is_some_and(|limitation| limitation.contains("`packets` array"))
    );
}

#[test]
fn ripr_swarm_plan_blocks_missing_target_test_shape() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:no-target-test-shape",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(value, expected)"
                },
                "target_test_shape": "target_test_shape_unknown",
                "verify_command": "cargo test no_target_test_shape",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/lib.rs",
                    "name": "no_target_test_shape"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/lib.rs"],
                "raw_evidence_refs": [
                    {"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}
                ],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    let blocked = ripr_swarm_plan_blocked_packets(&report);
    let summary = crate::ripr_swarm_plan_summary_json(&report);
    let examples = crate::ripr_swarm_plan_blocked_state_examples_json(&report);

    assert!(ready.is_empty());
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["target_test_shape"]);
    assert_eq!(
        summary["missing_target_test_shape"],
        serde_json::Value::from(1)
    );
    assert!(examples.iter().any(|example| {
        example["state"] == "missing_target_test_shape"
            && example["example_missing_context"] == serde_json::json!(["target_test_shape"])
    }));
}

#[test]
fn ripr_swarm_plan_blocks_missing_related_context() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:no-related-context",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(value, expected)"
                },
                "verify_command": "cargo test no_related_context",
                "receipt_command": "cargo xtask receipts check",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&report);

    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "related_test_or_observer")
    );
}

#[test]
fn ripr_swarm_plan_blocks_candidate_prose_without_typed_target() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:candidate-prose",
                "evidence_class": "call_presence",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_call_observer",
                "target_test_type": "call_presence_observer",
                "assertion_shape": "// assert that helper called the expected target",
                "repair_route": {
                    "repair_kind": "add_call_observer",
                    "target_test_type": "call_presence_observer",
                    "assertion_shape": "// assert that helper called the expected target"
                },
                "verify_command": "cargo test helper_call",
                "receipt_command": "cargo xtask receipts check",
                "candidate_value_or_observer": "input that reaches call helper()",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"kind": "ungripped", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "ungripped", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&report);

    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "related_test_or_observer")
    );
}

#[test]
fn ripr_swarm_plan_blocks_missing_raw_evidence_refs() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:no-raw-evidence",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(value, expected)"
                },
                "verify_command": "cargo test no_raw_evidence",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": "tests/lib.rs::no_raw_evidence",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [],
                "raw_findings": [],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&report);

    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "raw_evidence_refs")
    );
}

#[test]
fn ripr_swarm_plan_blocks_missing_raw_evidence_refs_even_with_raw_findings() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:raw-findings-only",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(value, expected)"
                },
                "verify_command": "cargo test raw_findings_only",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": {
                    "file": "tests/lib.rs",
                    "name": "raw_findings_only"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "allowed_edit_surface": ["tests/lib.rs"],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);
    let blocked = ripr_swarm_plan_blocked_packets(&report);
    let summary = crate::ripr_swarm_plan_summary_json(&report);

    assert!(ready.is_empty());
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["raw_evidence_refs"]);
    assert_eq!(blocked[0].raw_findings_count, 1);
    assert_eq!(
        summary["missing_raw_evidence_refs"],
        serde_json::Value::from(1)
    );
}

#[test]
fn ripr_swarm_plan_blocks_receipt_path_without_receipt_command() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:receipt-path-only",
                "evidence_class": "error_path",
                "gap_state": "actionable",
                "source_file": "src/auth.rs",
                "repair_kind": "add_exact_error_variant",
                "target_test_type": "error_variant",
                "assertion_shape": "matches!(auth(input), Err(AuthError::RevokedToken))",
                "repair_route": {
                    "repair_kind": "add_exact_error_variant",
                    "target_test_type": "error_variant",
                    "assertion_shape": "matches!(auth(input), Err(AuthError::RevokedToken))"
                },
                "verify_command": "cargo test -p ripr revoked_token",
                "receipt_command_or_path": "target/ripr/receipts/receipt-path-only.json",
                "related_test_or_observer": {
                    "file": "tests/auth.rs",
                    "name": "revoked_token"
                },
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production auth code by default."],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/auth.rs", "line": 55}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&report);
    let ready = ripr_swarm_plan_ready_packets(&report);

    assert!(ready.is_empty());
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "receipt_command")
    );
    assert!(
        blocked[0]
            .blocked_reasons
            .iter()
            .any(|reason| reason == "missing_receipt_command")
    );
}

#[test]
fn ripr_swarm_plan_blocks_malformed_raw_evidence_refs() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:malformed-raw-evidence",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "assertion_shape": "assert_eq!(value, expected)"
                },
                "verify_command": "cargo test malformed_raw_evidence",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": "tests/lib.rs::malformed_raw_evidence",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{}],
                "raw_findings": ["placeholder"],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&report);

    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "raw_evidence_refs")
    );
}

#[test]
fn ripr_swarm_plan_blocks_incomplete_structured_repair_routes() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:incomplete-route",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {"repair_kind": "add_boundary_assertion"},
                "verify_command": "cargo test incomplete_route",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": "tests/lib.rs::incomplete_route",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&report);

    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "repair_route")
    );
}

#[test]
fn ripr_swarm_plan_blocks_inconsistent_structured_repair_routes() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:inconsistent-route",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_output_observer",
                    "target_test_type": "output_observer",
                    "assertion_shape": "assert!(output.contains(expected))"
                },
                "verify_command": "cargo test inconsistent_route",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": "tests/lib.rs::inconsistent_route",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&report);
    let ready = ripr_swarm_plan_ready_packets(&report);

    assert!(ready.is_empty());
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "repair_route_consistency")
    );
    assert!(
        blocked[0]
            .blocked_reasons
            .iter()
            .any(|reason| reason == "missing_repair_route_consistency")
    );
}

#[test]
fn ripr_swarm_plan_blocks_unserialized_repair_route_hints() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:route-source-only",
                "evidence_class": "call_presence",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_call_observer",
                "target_test_type": "call_presence_observer",
                "assertion_shape": "// assert that helper called the expected target",
                "repair_route_source": "canonical_item.repair_route",
                "verify_command": "cargo test helper_call",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": "tests/lib.rs::helper_call",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"kind": "ungripped", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "ungripped", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&report);

    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert!(
        blocked[0]
            .missing_context
            .iter()
            .any(|field| field == "repair_route")
    );
}

#[test]
fn ripr_swarm_plan_accepts_structured_routes_with_suggested_assertion() {
    let actionable_gaps = serde_json::json!({
        "summary": {"actionable_gaps": 1},
        "packets": [
            {
                "canonical_gap_id": "gap:suggested-assertion",
                "evidence_class": "predicate_boundary",
                "gap_state": "actionable",
                "source_file": "src/lib.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)",
                "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_discriminator",
                    "suggested_assertion": "assert_eq!(value, expected)"
                },
                "verify_command": "cargo test suggested_assertion",
                "receipt_command": "cargo xtask receipts check",
                "related_test_or_observer": "tests/lib.rs::suggested_assertion",
                "confidence_basis": "fixture_backed",
                "must_not_change": ["Do not edit production code by default."],
                "raw_evidence_refs": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "raw_findings": [{"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}],
                "static_limitations": [],
                "public_projection_eligible": true
            }
        ]
    });

    let report = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &actionable_gaps,
    );
    let ready = ripr_swarm_plan_ready_packets(&report);

    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].canonical_gap_id, "gap:suggested-assertion");
    let markdown = ripr_swarm_plan_markdown(&report);
    assert!(markdown.contains("## Runtime Status"));
    assert!(markdown.contains("| State | `full` |"));
    assert!(markdown.contains("| Downstream consumable | `true` |"));
}

#[test]
fn ripr_swarm_plan_packet_corpus_matches_expected_states() -> Result<(), String> {
    with_repo_cwd(|| {
        let mut violations = Vec::new();
        validate_swarm_plan_packet_fixture_corpus(&mut violations)?;
        if violations.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "swarm plan packet corpus violations:\n- {}",
                violations.join("\n- ")
            ))
        }
    })
}

#[test]
fn ripr_swarm_plan_packet_corpus_rejects_unsupported_receipt_commands() {
    let case = serde_json::json!({
        "id": "unsupported_receipt_emit",
        "description": "ready-looking packet with an unsupported receipt command",
        "must_not_claim": ["Do not create receipts from the swarm plan."],
        "packet": {
            "canonical_gap_id": "gap:unsupported-receipt",
            "evidence_class": "predicate_boundary",
            "gap_state": "actionable",
            "source_file": "src/lib.rs",
            "repair_kind": "add_boundary_assertion",
            "target_test_type": "boundary_discriminator",
            "assertion_shape": "assert_eq!(value, expected)",
            "repair_route": {
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)"
            },
            "verify_command": "cargo test unsupported_receipt",
            "receipt_command": "cargo xtask receipts emit --packet gap:unsupported-receipt",
            "related_test_or_observer": "tests/lib.rs::unsupported_receipt",
            "confidence_basis": "fixture_backed",
            "must_not_change": ["Do not edit production code by default."],
            "raw_findings": [
                {"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}
            ],
            "static_limitations": [],
            "public_projection_eligible": true
        },
        "expected": {
            "swarm_state": "queued",
            "swarm_ready": true,
            "high_confidence": true,
            "missing_context": [],
            "blocked_reasons": [],
            "summary": {
                "swarm_ready_packets": 1,
                "blocked_packets": 0
            }
        }
    });
    let mut violations = Vec::new();
    validate_swarm_plan_packet_fixture_case(&case, "unsupported_receipt_emit", &mut violations);

    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("unsupported receipt_command"))
    );
}

#[test]
fn ripr_swarm_plan_packet_corpus_rejects_incomplete_agent_receipt_commands() {
    let case = serde_json::json!({
        "id": "incomplete_agent_receipt",
        "description": "ready-looking packet with an incomplete agent receipt command",
        "must_not_claim": ["Do not accept malformed receipt commands."],
        "packet": {
            "canonical_gap_id": "gap:incomplete-agent-receipt",
            "evidence_class": "predicate_boundary",
            "gap_state": "actionable",
            "source_file": "src/lib.rs",
            "repair_kind": "add_boundary_assertion",
            "target_test_type": "boundary_discriminator",
            "assertion_shape": "assert_eq!(value, expected)",
            "repair_route": {
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(value, expected)"
            },
            "verify_command": "cargo test incomplete_agent_receipt",
            "receipt_command": "ripr agent receipt --root . --json",
            "related_test_or_observer": "tests/lib.rs::incomplete_agent_receipt",
            "confidence_basis": "fixture_backed",
            "must_not_change": ["Do not edit production code by default."],
            "raw_findings": [
                {"kind": "weakly_exposed", "file": "src/lib.rs", "line": 12}
            ],
            "static_limitations": [],
            "public_projection_eligible": true
        },
        "expected": {
            "swarm_state": "queued",
            "swarm_ready": true,
            "high_confidence": true,
            "missing_context": [],
            "blocked_reasons": [],
            "summary": {
                "swarm_ready_packets": 1,
                "blocked_packets": 0
            }
        }
    });
    let mut violations = Vec::new();
    validate_swarm_plan_packet_fixture_case(&case, "incomplete_agent_receipt", &mut violations);

    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("unsupported receipt_command"))
    );
}

#[test]
fn ripr_swarm_plan_rejects_unsafe_argument_shapes() -> Result<(), String> {
    expect_ripr_swarm_plan_arg_error(&[])?
        .contains("usage: cargo xtask ripr-swarm plan")
        .then_some(())
        .ok_or_else(|| "missing ripr-swarm subcommand returned unexpected error".to_string())?;
    expect_ripr_swarm_plan_arg_error(&["unknown"])?
        .contains("unknown ripr-swarm subcommand")
        .then_some(())
        .ok_or_else(|| "unknown ripr-swarm subcommand returned unexpected error".to_string())?;
    expect_ripr_swarm_plan_arg_error(&["plan", "--top"])?
        .contains("--top requires a positive integer")
        .then_some(())
        .ok_or_else(|| "missing swarm plan top value returned unexpected error".to_string())?;
    expect_ripr_swarm_plan_arg_error(&["plan", "--top", "0"])?
        .contains("must be greater than zero")
        .then_some(())
        .ok_or_else(|| "zero swarm plan top value returned unexpected error".to_string())?;
    expect_ripr_swarm_plan_arg_error(&["plan", "--actionable-gaps"])?
        .contains("--actionable-gaps requires a path")
        .then_some(())
        .ok_or_else(|| "missing actionable-gaps path returned unexpected error".to_string())?;
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_preserves_prior_attempts_and_highlights_latest() -> Result<(), String>
{
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": [
            {
                "packet_id": "packet-boundary-001",
                "canonical_gap_id": "gap:seam-a",
                "verify_command": "cargo test -p ripr seam_a",
                "receipt_command": "cargo xtask receipts write --packet packet-boundary-001"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "inputs": {
            "agent_receipt": "target/ripr/reports/agent-receipt.json"
        },
        "outcomes": [
            {
                "canonical_gap_id": "gap:seam-a",
                "evidence_class": "predicate_boundary",
                "repair_kind": "add_boundary_assertion",
                "source_file": "src/pricing.rs",
                "verify_command": "cargo test -p ripr seam_a",
                "verify_result": "pass",
                "receipt_command_or_path": "cargo xtask receipts write --packet packet-boundary-001",
                "receipt_state": "receipt_movement_improved",
                "outcome_state": "evidence_improved",
                "seam_id": "seam-a",
                "before": "weakly_gripped",
                "after": "strongly_gripped",
                "movement_source": "agent_receipt",
                "movement_direction": "improved",
                "reason": "Matched agent receipt artifact."
            }
        ],
        "orphaned_receipts": [
            {
                "receipt_id": "receipt:old",
                "seam_id": "old",
                "reason": "Receipt artifact did not match any current actionable canonical gap packet."
            }
        ]
    });
    let prior_ledger = serde_json::json!({
        "schema_version": "0.1",
        "report": "swarm-attempt-ledger",
        "attempts": [
            {
                "packet_id": "packet-boundary-001",
                "canonical_gap_id": "gap:seam-a",
                "attempt_id": "attempt:prior-unchanged",
                "actor_kind": "agent",
                "receipt_path": "target/ripr/reports/agent-receipt-old.json",
                "verify_command": "cargo test -p ripr seam_a",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-boundary-001",
                "before_gap_state": "weakly_gripped",
                "after_gap_state": "weakly_gripped",
                "outcome": "evidence_unchanged",
                "timestamp": "unix_ms:1",
                "receipt_state": "receipt_movement_unchanged",
                "movement_source": "agent_receipt",
                "reason": "prior attempt"
            }
        ]
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:2".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&prior_ledger),
        },
    );

    assert_eq!(report.attempts.len(), 2);
    assert_eq!(report.latest_attempts.len(), 1);
    assert_eq!(report.latest_attempts[0].packet_id, "packet-boundary-001");
    assert_eq!(report.latest_attempts[0].outcome, "evidence_improved");
    assert_eq!(
        report.latest_attempts[0].receipt_path.as_deref(),
        Some("target/ripr/reports/agent-receipt.json")
    );
    let json = ripr_swarm_attempt_ledger_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["report"], "swarm-attempt-ledger");
    assert_eq!(value["summary"]["attempts_total"], 2);
    assert_eq!(value["summary"]["evidence_improved"], 1);
    assert_eq!(value["summary"]["missing_verify_result"], 0);
    assert_eq!(value["summary"]["orphaned_receipts"], 1);
    assert_eq!(
        value["latest_attempts"][0]["attempt_id"],
        "attempt:gap-seam-a:evidence-improved:receipt-movement-improved:agent-receipt:seam-a:receipt-path-target-ripr-reports-agent-receipt-json"
    );
    assert_eq!(value["latest_attempts"][0]["verify_result"], "pass");
    let markdown = ripr_swarm_attempt_ledger_markdown(&report);
    assert!(markdown.contains("# RIPR Swarm Attempt Ledger"));
    assert!(markdown.contains("## Runtime Status"));
    assert!(markdown.contains("| State | `full` |"));
    assert!(markdown.contains("| Downstream consumable | `true` |"));
    assert!(markdown.contains("## Latest Attempts By Canonical Gap"));
    assert!(markdown.contains("evidence_improved"));
    assert!(markdown.contains("| Verify result |"));
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_drops_stale_synthetic_not_attempted_rows() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "outcomes": []
    });
    let prior_ledger = serde_json::json!({
        "schema_version": "0.1",
        "report": "swarm-attempt-ledger",
        "attempts": [
            {
                "packet_id": "packet-stale-001",
                "canonical_gap_id": "gap:stale",
                "attempt_id": "attempt:stale:not-attempted",
                "actor_kind": "swarm",
                "verify_command": "verify_command_unknown",
                "outcome": "not_attempted",
                "timestamp": "unix_ms:1",
                "receipt_state": "receipt_not_applicable",
                "reason": "prior synthetic queue placeholder"
            }
        ]
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:2".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&prior_ledger),
        },
    );

    assert!(report.attempts.is_empty());
    assert!(report.latest_attempts.is_empty());
    assert!(report.repair_route_quality.is_empty());
    assert!(report.top_missing_evidence_fields.is_empty());
    let json = ripr_swarm_attempt_ledger_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["attempts_total"], 0);
    assert_eq!(value["summary"]["not_attempted"], 0);
    assert_eq!(
        value["top_missing_evidence_fields"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_synthesizes_current_plan_not_attempted_rows() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": [
            {
                "packet_id": "packet-current-001",
                "canonical_gap_id": "gap:current",
                "evidence_class": "predicate_boundary",
                "source_file": "src/pricing.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(discount(100, 100), 90)",
                "verify_command": "cargo test -p ripr current_gap",
                "receipt_command": "cargo xtask receipts write --packet packet-current-001"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "outcomes": []
    });
    let prior_ledger = serde_json::json!({
        "schema_version": "0.1",
        "report": "swarm-attempt-ledger",
        "attempts": []
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:2".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&prior_ledger),
        },
    );

    assert_eq!(report.attempts.len(), 1);
    assert_eq!(report.latest_attempts.len(), 1);
    let attempt = &report.latest_attempts[0];
    assert_eq!(attempt.packet_id, "packet-current-001");
    assert_eq!(attempt.canonical_gap_id, "gap:current");
    assert_eq!(
        attempt.repair_kind.as_deref(),
        Some("add_boundary_assertion")
    );
    assert_eq!(attempt.verify_command, "cargo test -p ripr current_gap");
    assert_eq!(
        attempt.receipt_command.as_deref(),
        Some("cargo xtask receipts write --packet packet-current-001")
    );
    assert_eq!(attempt.outcome, "not_attempted");
    assert_eq!(attempt.actor_kind, "none");
    assert_eq!(attempt.receipt_state, RECEIPT_NOT_APPLICABLE);
    assert_eq!(
        attempt.reason,
        "current swarm plan queued packet placeholder"
    );
    let json = ripr_swarm_attempt_ledger_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["attempts_total"], 1);
    assert_eq!(value["summary"]["not_attempted"], 1);
    assert_eq!(value["summary"]["missing_verify_result"], 0);
    assert_eq!(
        value["top_missing_evidence_fields"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_dedupes_current_plan_placeholders_by_packet_or_gap()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": [
            {
                "packet_id": "packet-existing",
                "canonical_gap_id": "gap:new-packet-duplicate",
                "repair_kind": "add_boundary_assertion",
                "verify_command": "cargo test -p ripr duplicate_packet",
                "receipt_command": "cargo xtask receipts write --packet packet-existing"
            },
            {
                "packet_id": "packet-new-gap",
                "canonical_gap_id": "gap:existing",
                "repair_kind": "add_boundary_assertion",
                "verify_command": "cargo test -p ripr duplicate_gap",
                "receipt_command": "cargo xtask receipts write --packet packet-new-gap"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "outcomes": []
    });
    let prior_ledger = serde_json::json!({
        "schema_version": "0.1",
        "report": "swarm-attempt-ledger",
        "attempts": [
            {
                "packet_id": "packet-existing",
                "canonical_gap_id": "gap:prior-packet",
                "attempt_id": "attempt:prior-packet:not-attempted",
                "repair_kind": "add_boundary_assertion",
                "actor_kind": "none",
                "verify_command": "cargo test -p ripr prior_packet",
                "receipt_command": "cargo xtask receipts write --packet packet-existing",
                "outcome": "not_attempted",
                "timestamp": "unix_ms:1",
                "receipt_state": "receipt_not_applicable",
                "reason": "prior current queue placeholder"
            },
            {
                "packet_id": "packet-prior-gap",
                "canonical_gap_id": "gap:existing",
                "attempt_id": "attempt:prior-gap:not-attempted",
                "repair_kind": "add_boundary_assertion",
                "actor_kind": "none",
                "verify_command": "cargo test -p ripr prior_gap",
                "receipt_command": "cargo xtask receipts write --packet packet-prior-gap",
                "outcome": "not_attempted",
                "timestamp": "unix_ms:1",
                "receipt_state": "receipt_not_applicable",
                "reason": "prior current queue placeholder"
            }
        ]
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:2".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&prior_ledger),
        },
    );

    assert_eq!(report.attempts.len(), 2);
    assert_eq!(report.latest_attempts.len(), 2);
    assert!(
        !report
            .latest_attempts
            .iter()
            .any(|attempt| attempt.canonical_gap_id == "gap:new-packet-duplicate")
    );
    assert!(
        !report
            .latest_attempts
            .iter()
            .any(|attempt| attempt.packet_id == "packet-new-gap")
    );
    assert!(report.top_missing_evidence_fields.is_empty());
    let json = ripr_swarm_attempt_ledger_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["attempts_total"], 2);
    assert_eq!(value["summary"]["not_attempted"], 2);
    assert_eq!(
        value["top_missing_evidence_fields"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_synthesizes_fallback_placeholder_fields() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": [
            {
                "canonical_gap_id": "gap:fallback",
                "repair_kind": "add_boundary_assertion",
                "receipt_command": "cargo xtask receipts write --packet gap-fallback"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "outcomes": []
    });
    let prior_ledger = serde_json::json!({
        "schema_version": "0.1",
        "report": "swarm-attempt-ledger",
        "attempts": []
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:2".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&prior_ledger),
        },
    );

    assert_eq!(report.latest_attempts.len(), 1);
    let attempt = &report.latest_attempts[0];
    assert_eq!(attempt.packet_id, "gap:fallback");
    assert_eq!(attempt.canonical_gap_id, "gap:fallback");
    assert_eq!(attempt.verify_command, "verify_command_unknown");
    assert_eq!(
        attempt.reason,
        "current swarm plan queued packet placeholder missing verify_command"
    );
    assert_eq!(report.top_missing_evidence_fields.len(), 1);
    assert_eq!(
        report.top_missing_evidence_fields[0].label,
        "verify_command"
    );
    assert_eq!(report.top_missing_evidence_fields[0].count, 1);
    let json = ripr_swarm_attempt_ledger_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["attempts_total"], 1);
    assert_eq!(value["summary"]["not_attempted"], 1);
    assert_eq!(value["summary"]["missing_verify_result"], 0);
    assert_eq!(
        value["top_missing_evidence_fields"][0]["label"],
        "verify_command"
    );
    assert_eq!(
        value["latest_attempts"][0]["verify_command"],
        "verify_command_unknown"
    );
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_preserves_current_plan_not_attempted_rows() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": [
            {
                "packet_id": "packet-current-001",
                "canonical_gap_id": "gap:current",
                "repair_kind": "add_boundary_assertion",
                "verify_command": "cargo test -p ripr current_gap",
                "receipt_command": "cargo xtask receipts write --packet packet-current-001"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "outcomes": []
    });
    let prior_ledger = serde_json::json!({
        "schema_version": "0.1",
        "report": "swarm-attempt-ledger",
        "attempts": [
            {
                "packet_id": "packet-current-001",
                "canonical_gap_id": "gap:current",
                "attempt_id": "attempt:current:not-attempted",
                "repair_kind": "add_boundary_assertion",
                "actor_kind": "swarm",
                "verify_command": "cargo test -p ripr current_gap",
                "receipt_command": "cargo xtask receipts write --packet packet-current-001",
                "outcome": "not_attempted",
                "timestamp": "unix_ms:1",
                "receipt_state": "receipt_not_applicable",
                "reason": "current queue placeholder"
            }
        ]
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:2".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&prior_ledger),
        },
    );

    assert_eq!(report.attempts.len(), 1);
    assert_eq!(report.latest_attempts.len(), 1);
    assert_eq!(report.latest_attempts[0].packet_id, "packet-current-001");
    assert_eq!(report.latest_attempts[0].outcome, "not_attempted");
    let json = ripr_swarm_attempt_ledger_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["attempts_total"], 1);
    assert_eq!(value["summary"]["not_attempted"], 1);
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_preserves_repeated_same_state_attempts() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": [
            {
                "packet_id": "packet-boundary-001",
                "canonical_gap_id": "gap:seam-a",
                "evidence_class": "predicate_boundary",
                "source_file": "src/pricing.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(discounted_total(threshold), expected)",
                "verify_command": "cargo test -p ripr seam_a",
                "receipt_command": "cargo xtask receipts write --packet packet-boundary-001"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "inputs": {
            "agent_receipt": "target/ripr/reports/agent-receipt-new.json"
        },
        "outcomes": [
            {
                "canonical_gap_id": "gap:seam-a",
                "receipt_state": "receipt_movement_unchanged",
                "outcome_state": "evidence_unchanged",
                "seam_id": "seam-a",
                "before": "weakly_gripped",
                "after": "weakly_gripped",
                "movement_source": "agent_receipt",
                "timestamp": "unix_ms:2",
                "attempt_instance": "receipt:agent-new"
            }
        ]
    });
    let prior_ledger = serde_json::json!({
        "schema_version": "0.1",
        "report": "swarm-attempt-ledger",
        "attempts": [
            {
                "packet_id": "packet-boundary-001",
                "canonical_gap_id": "gap:seam-a",
                "attempt_id": "attempt:gap-seam-a:evidence-unchanged:receipt-movement-unchanged:agent-receipt:seam-a",
                "actor_kind": "agent",
                "receipt_path": "target/ripr/reports/agent-receipt-old.json",
                "verify_command": "cargo test -p ripr seam_a",
                "receipt_command": "cargo xtask receipts write --packet packet-boundary-001",
                "before_gap_state": "weakly_gripped",
                "after_gap_state": "weakly_gripped",
                "outcome": "evidence_unchanged",
                "timestamp": "unix_ms:1",
                "receipt_state": "receipt_movement_unchanged",
                "movement_source": "agent_receipt",
                "reason": "prior unchanged attempt"
            }
        ]
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:3".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&prior_ledger),
        },
    );

    assert_eq!(
        report.attempts.len(),
        2,
        "same-state repeated attempts with distinct timestamps must not collapse"
    );
    assert_eq!(report.latest_attempts.len(), 1);
    assert_eq!(report.latest_attempts[0].outcome, "evidence_unchanged");
    assert_eq!(
        report.latest_attempts[0].attempt_id,
        "attempt:gap-seam-a:evidence-unchanged:receipt-movement-unchanged:agent-receipt:seam-a:receipt-agent-new"
    );
    assert!(
            report.attempts.iter().any(|attempt| attempt.attempt_id.as_str()
                == "attempt:gap-seam-a:evidence-unchanged:receipt-movement-unchanged:agent-receipt:seam-a"),
            "prior same-state attempt remains visible"
        );
    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_attempt_ledger_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["attempts_total"], 2);
    assert_eq!(value["summary"]["evidence_unchanged"], 1);
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_imports_real_repair_attempts() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "outcomes": [
            {
                "canonical_gap_id": "gap:missing-receipt",
                "packet_id": "gap:missing-receipt",
                "outcome_state": "not_attempted",
                "receipt_state": "receipt_not_applicable",
                "repair_kind": "add_call_observer",
                "verify_command": "cargo test -p ripr missing_receipt"
            }
        ]
    });
    let real_attempts = serde_json::json!({
        "schema_version": "0.1",
        "kind": "real_repair_attempts_corpus",
        "cases": [
            {
                "id": "route-improved",
                "canonical_gap_id": "gap:route-improved",
                "packet_id": "limitation:route-improved",
                "language": "typescript",
                "repair_kind": "sharpen_static_limitation_route",
                "target_test_or_observer_shape": "fixture-backed analyzer route split",
                "verify_command": "cargo test -p ripr route_improved",
                "verify_result": "pass",
                "receipt_command": "cargo xtask dogfood",
                "receipt_path": "fixtures/real-repair-attempts/corpus.json",
                "receipt_state": "receipt_movement_improved",
                "actor_kind": "codex",
                "before_gap_state": "static_limitation",
                "after_gap_state": "static_limitation",
                "outcome": "evidence_improved",
                "reason": "real analyzer backlog route improved"
            },
            {
                "id": "missing-receipt",
                "canonical_gap_id": "gap:missing-receipt",
                "packet_id": "gap:missing-receipt",
                "language": "javascript",
                "repair_kind": "add_call_observer",
                "target_test_or_observer_shape": "bounded call observer",
                "verify_command": "cargo test -p ripr missing_receipt",
                "verify_result": "not_run",
                "receipt_command": "cargo xtask receipts write",
                "receipt_path": null,
                "receipt_state": "receipt_missing",
                "actor_kind": "codex",
                "before_gap_state": "actionable",
                "after_gap_state": "actionable",
                "outcome": "attempted_no_receipt",
                "missing_receipt_reason": "receipt command was not run after verify timed out",
                "reason": "attempt stopped before receipt"
            }
        ]
    });

    let report = ripr_swarm_attempt_ledger_from_values_with_real_repair_attempts(
        "unix_ms:99".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("no prior ledger".to_string()),
            value: None,
        },
        RiprSwarmReadinessInput {
            path: "fixtures/real-repair-attempts/corpus.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&real_attempts),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_attempt_ledger_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["attempts_total"], 3);
    assert_eq!(value["summary"]["evidence_improved"], 1);
    assert_eq!(value["summary"]["attempted_no_receipt"], 1);
    assert_eq!(value["summary"]["not_attempted"], 0);
    assert_eq!(
        value["inputs"]["real_repair_attempts"]["state"],
        serde_json::Value::from("read")
    );
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind"],
        serde_json::Value::from("add_call_observer")
    );
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_attempted_no_receipt"],
        serde_json::Value::from(1)
    );
    let language_rows = value["language_repair_route_quality"]
        .as_array()
        .ok_or_else(|| "expected language repair-route quality rows".to_string())?;
    assert!(language_rows.iter().any(|row| {
        row["language"] == "javascript"
            && row["repair_kind"] == "add_call_observer"
            && row["repair_kind_attempted_no_receipt"] == 1
    }));
    assert!(language_rows.iter().any(|row| {
        row["language"] == "typescript"
            && row["repair_kind"] == "sharpen_static_limitation_route"
            && row["repair_kind_improved"] == 1
    }));
    let missing_receipt_attempt = value["attempts"]
        .as_array()
        .and_then(|attempts| {
            attempts
                .iter()
                .find(|attempt| attempt["outcome"] == "attempted_no_receipt")
        })
        .ok_or_else(|| "expected attempted_no_receipt attempt".to_string())?;
    assert_eq!(
        missing_receipt_attempt["missing_receipt_reason"],
        serde_json::Value::from("receipt command was not run after verify timed out")
    );
    assert_eq!(
        missing_receipt_attempt["language"],
        serde_json::Value::from("javascript")
    );
    let markdown = ripr_swarm_attempt_ledger_markdown(&report);
    assert!(markdown.contains("| real repair attempts | `read` |"));
    assert!(markdown.contains("## Repair Route Quality By Language"));
    assert!(markdown.contains("Missing receipt reason"));
    assert!(markdown.contains("receipt command was not run after verify timed out"));
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_real_corpus_supersedes_add_call_observer_timeout() -> Result<(), String>
{
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "outcomes": []
    });
    let corpus_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(REAL_REPAIR_ATTEMPTS_CORPUS);
    let real_attempts = read_json_value(&corpus_path)?;

    let report = ripr_swarm_attempt_ledger_from_values_with_real_repair_attempts(
        "unix_ms:100".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("no prior ledger".to_string()),
            value: None,
        },
        RiprSwarmReadinessInput {
            path: REAL_REPAIR_ATTEMPTS_CORPUS.to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&real_attempts),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_attempt_ledger_json(&report)?)
        .map_err(|err| err.to_string())?;
    let latest = value["latest_attempts"]
        .as_array()
        .and_then(|attempts| {
            attempts
                .iter()
                .find(|attempt| attempt["canonical_gap_id"] == "gap:54970c87c8f8ca9d")
        })
        .ok_or_else(|| "expected latest add_call_observer attempt".to_string())?;
    assert_eq!(latest["outcome"], "evidence_improved");
    assert_eq!(
        latest["verify_command"],
        "cargo test -p ripr serve_stdio_call_presence_observer"
    );
    assert_eq!(
        latest["receipt_path"],
        serde_json::Value::from(REAL_REPAIR_ATTEMPTS_CORPUS)
    );

    let route_quality = value["repair_route_quality"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["repair_kind"] == "add_call_observer")
        })
        .ok_or_else(|| "expected add_call_observer route-quality row".to_string())?;
    assert_eq!(
        route_quality["repair_kind_attempted_no_receipt"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        route_quality["repair_kind_improved"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        route_quality["repair_kind_failure_count"],
        serde_json::Value::from(0)
    );
    assert!(
        value["top_missing_evidence_fields"]
            .as_array()
            .is_none_or(|rows| {
                !rows
                    .iter()
                    .flat_map(|row| {
                        row["sample_canonical_gap_ids"]
                            .as_array()
                            .into_iter()
                            .flatten()
                    })
                    .any(|gap| gap == "gap:54970c87c8f8ca9d")
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_real_corpus_supersedes_add_output_observer_missing_receipt()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "outcomes": []
    });
    let corpus_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(REAL_REPAIR_ATTEMPTS_CORPUS);
    let real_attempts = read_json_value(&corpus_path)?;

    let report = ripr_swarm_attempt_ledger_from_values_with_real_repair_attempts(
        "unix_ms:100".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("no prior ledger".to_string()),
            value: None,
        },
        RiprSwarmReadinessInput {
            path: REAL_REPAIR_ATTEMPTS_CORPUS.to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&real_attempts),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_attempt_ledger_json(&report)?)
        .map_err(|err| err.to_string())?;
    let latest = value["latest_attempts"]
        .as_array()
        .and_then(|attempts| {
            attempts
                .iter()
                .find(|attempt| attempt["canonical_gap_id"] == "gap:report-config-label")
        })
        .ok_or_else(|| "expected latest add_output_observer attempt".to_string())?;
    assert_eq!(latest["outcome"], "evidence_improved");
    assert_eq!(latest["verify_command"], "cargo xtask goldens check");
    assert_eq!(latest["receipt_command"], "cargo xtask dogfood");
    assert_eq!(
        latest["receipt_path"],
        serde_json::Value::from(REAL_REPAIR_ATTEMPTS_CORPUS)
    );

    let route_quality = value["repair_route_quality"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["repair_kind"] == "add_output_observer")
        })
        .ok_or_else(|| "expected add_output_observer route-quality row".to_string())?;
    assert_eq!(
        route_quality["repair_kind_attempted_no_receipt"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        route_quality["repair_kind_improved"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        route_quality["repair_kind_failure_count"],
        serde_json::Value::from(0)
    );
    assert!(
        value["top_missing_evidence_fields"]
            .as_array()
            .is_none_or(|rows| {
                !rows
                    .iter()
                    .flat_map(|row| {
                        row["sample_canonical_gap_ids"]
                            .as_array()
                            .into_iter()
                            .flatten()
                    })
                    .any(|gap| gap == "gap:report-config-label")
            })
    );
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_real_corpus_supersedes_unchanged_attempts() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "outcomes": []
    });
    let corpus_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(REAL_REPAIR_ATTEMPTS_CORPUS);
    let real_attempts = read_json_value(&corpus_path)?;

    let report = ripr_swarm_attempt_ledger_from_values_with_real_repair_attempts(
        "unix_ms:100".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("no prior ledger".to_string()),
            value: None,
        },
        RiprSwarmReadinessInput {
            path: REAL_REPAIR_ATTEMPTS_CORPUS.to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&real_attempts),
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_attempt_ledger_json(&report)?)
        .map_err(|err| err.to_string())?;
    let latest = value["latest_attempts"]
        .as_array()
        .and_then(|attempts| {
            attempts
                .iter()
                .find(|attempt| attempt["canonical_gap_id"] == "gap:unchanged")
        })
        .ok_or_else(|| "expected latest add_exact_error_variant attempt".to_string())?;
    assert_eq!(latest["outcome"], "evidence_improved");
    assert_eq!(latest["repair_kind"], "add_exact_error_variant");
    assert_eq!(
        latest["verify_command"],
        "cargo test -p ripr exact_error_variant --lib"
    );
    assert_eq!(latest["receipt_command"], "cargo xtask dogfood");
    assert_eq!(
        latest["receipt_path"],
        serde_json::Value::from(REAL_REPAIR_ATTEMPTS_CORPUS)
    );

    let route_quality = value["repair_route_quality"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["repair_kind"] == "add_exact_error_variant")
        })
        .ok_or_else(|| "expected add_exact_error_variant route-quality row".to_string())?;
    assert_eq!(
        route_quality["repair_kind_unchanged"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        route_quality["repair_kind_improved"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        route_quality["repair_kind_failure_count"],
        serde_json::Value::from(0)
    );
    let weak_oracle_expected_unchanged = value["attempts"]
        .as_array()
        .and_then(|attempts| {
            attempts.iter().find(|attempt| {
                attempt["canonical_gap_id"] == "gap:typescript-preview-weak-oracle-downgrade"
                    && attempt["outcome"] == "evidence_unchanged"
            })
        })
        .ok_or_else(|| "expected durable weak-oracle expected-unchanged attempt".to_string())?;
    assert_eq!(
        weak_oracle_expected_unchanged["route_quality_expectation"],
        "expected_unchanged_negative_capability"
    );
    assert_eq!(
        value["summary"]["expected_unchanged"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        value["attempt_history_summary"]["evidence_unchanged"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["attempt_history_summary"]["attempted_no_receipt"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["attempt_history_summary"]["expected_unchanged"],
        serde_json::Value::from(1)
    );
    assert!(
        !value["top_failing_repair_routes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["repair_kind"] == "preserve_preview_weak_oracle_boundary")
    );
    assert!(
        !value["repair_route_quality_backlog"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["repair_kind"] == "add_exact_error_variant")
    );

    let weak_oracle_latest = value["latest_attempts"]
        .as_array()
        .and_then(|attempts| {
            attempts.iter().find(|attempt| {
                attempt["canonical_gap_id"] == "gap:typescript-preview-weak-oracle-downgrade"
            })
        })
        .ok_or_else(|| "expected latest TypeScript weak-oracle attempt".to_string())?;
    assert_eq!(weak_oracle_latest["outcome"], "evidence_improved");
    assert_eq!(
        weak_oracle_latest["repair_kind"],
        "preserve_preview_weak_oracle_boundary"
    );
    assert_eq!(
        weak_oracle_latest["verify_command"],
        "cargo test -p ripr typescript_preview_weak_oracle --lib"
    );
    assert_eq!(
        weak_oracle_latest["receipt_command"],
        "cargo xtask fixtures typescript_jest_vitest_assertion_facts"
    );
    assert_eq!(
        weak_oracle_latest["receipt_path"],
        "fixtures/typescript_jest_vitest_assertion_facts/expected/check.json"
    );

    let weak_oracle_route_quality = value["repair_route_quality"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["repair_kind"] == "preserve_preview_weak_oracle_boundary")
        })
        .ok_or_else(|| {
            "expected preserve_preview_weak_oracle_boundary route-quality row".to_string()
        })?;
    assert_eq!(
        weak_oracle_route_quality["repair_kind_unchanged"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        weak_oracle_route_quality["repair_kind_expected_unchanged"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        weak_oracle_route_quality["repair_kind_improved"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        weak_oracle_route_quality["repair_kind_failure_count"],
        serde_json::Value::from(0)
    );
    assert!(
        !value["repair_route_quality_backlog"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["repair_kind"] == "preserve_preview_weak_oracle_boundary")
    );
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_preserves_limited_outcome_runtime_status() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "run_status": "full",
        "runtime_status": {
            "state": "full",
            "downstream_consumable": true
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "run_status": "limited_timeout",
        "runtime_status": {
            "state": "limited_timeout",
            "phase": null,
            "duration_ms": 30000,
            "limit_ms": 30000,
            "input_kind": "actionable-gap-outcomes",
            "input_path": null,
            "limitation_category": "lane1_repo_exposure_timeout",
            "repair_route": "rerun cargo xtask actionable-gap-outcomes with a larger budget",
            "downstream_consumable": false
        },
        "outcomes": []
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:12".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("no prior ledger".to_string()),
            value: None,
        },
    );

    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_attempt_ledger_json(&report)?)
        .map_err(|err| err.to_string())?;
    assert_eq!(value["run_status"], "limited_timeout");
    assert_eq!(
        value["runtime_status"]["phase"],
        "actionable_gap_outcomes_input"
    );
    assert_eq!(
        value["runtime_status"]["input_path"],
        "target/ripr/reports/actionable-gap-outcomes.json"
    );
    assert_eq!(
        value["runtime_status"]["repair_route"],
        "rerun cargo xtask actionable-gap-outcomes with a larger budget"
    );
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_orders_unix_ms_timestamps_numerically() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "outcomes": []
    });
    let prior_ledger = serde_json::json!({
        "schema_version": "0.1",
        "report": "swarm-attempt-ledger",
        "attempts": [
            {
                "packet_id": "packet-boundary-001",
                "canonical_gap_id": "gap:seam-a",
                "attempt_id": "attempt:older-improved",
                "actor_kind": "agent",
                "verify_command": "cargo test -p ripr seam_a",
                "receipt_command": "cargo xtask receipts write --packet packet-boundary-001",
                "outcome": "evidence_improved",
                "timestamp": "unix_ms:9",
                "receipt_state": "receipt_movement_improved"
            },
            {
                "packet_id": "packet-boundary-001",
                "canonical_gap_id": "gap:seam-a",
                "attempt_id": "attempt:newer-regressed",
                "actor_kind": "agent",
                "verify_command": "cargo test -p ripr seam_a",
                "receipt_command": "cargo xtask receipts write --packet packet-boundary-001",
                "outcome": "evidence_regressed",
                "timestamp": "unix_ms:10",
                "receipt_state": "receipt_movement_regressed"
            }
        ]
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:11".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&prior_ledger),
        },
    );

    assert_eq!(report.latest_attempts.len(), 1);
    assert_eq!(
        report.latest_attempts[0].attempt_id, "attempt:newer-regressed",
        "unix_ms timestamps must sort numerically so unix_ms:10 outranks unix_ms:9"
    );
    assert_eq!(report.latest_attempts[0].outcome, "evidence_regressed");
    let json = ripr_swarm_attempt_ledger_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["summary"]["evidence_regressed"],
        serde_json::Value::from(1)
    );
    Ok(())
}

#[test]
fn ripr_swarm_attempt_ledger_summarizes_repair_route_quality() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "top_ready_packets": [
            {
                "packet_id": "packet-boundary-001",
                "canonical_gap_id": "gap:boundary",
                "evidence_class": "predicate_boundary",
                "source_file": "src/pricing.rs",
                "repair_kind": "add_boundary_assertion",
                "target_test_type": "boundary_discriminator",
                "assertion_shape": "assert_eq!(discounted_total(threshold), expected)",
                "verify_command": "cargo test -p ripr boundary",
                "receipt_command": "cargo xtask receipts write --packet packet-boundary-001"
            },
            {
                "packet_id": "packet-output-001",
                "canonical_gap_id": "gap:output-a",
                "evidence_class": "call_presence",
                "source_file": "src/output.rs",
                "repair_kind": "add_output_observer",
                "target_test_type": "output_observer",
                "assertion_shape": "assert!(rendered.contains(label))",
                "verify_command": "cargo test -p ripr output_a",
                "receipt_command": "cargo xtask receipts write --packet packet-output-001"
            },
            {
                "packet_id": "packet-output-002",
                "canonical_gap_id": "gap:output-b",
                "evidence_class": "call_presence",
                "source_file": "src/output.rs",
                "repair_kind": "add_output_observer",
                "target_test_type": "output_observer",
                "assertion_shape": "assert!(rendered.contains(label))",
                "verify_command": "cargo test -p ripr output_b",
                "receipt_command": "cargo xtask receipts write --packet packet-output-002"
            },
            {
                "packet_id": "packet-call-001",
                "canonical_gap_id": "gap:call-a",
                "evidence_class": "call_presence",
                "source_file": "src/call.rs",
                "repair_kind": "add_call_observer",
                "target_test_type": "call_observer",
                "assertion_shape": "assert!(calls.contains(\"emit\"))",
                "verify_command": "cargo test -p ripr call_a",
                "receipt_command": "cargo xtask receipts write --packet packet-call-001"
            }
        ]
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "inputs": {
            "agent_receipt": "target/ripr/reports/agent-receipt.json"
        },
        "outcomes": [
            {
                "canonical_gap_id": "gap:boundary",
                "receipt_state": "receipt_movement_improved",
                "outcome_state": "evidence_improved",
                "seam_id": "boundary",
                "before": "weakly_gripped",
                "after": "strongly_gripped",
                "movement_source": "agent_receipt"
            },
            {
                "canonical_gap_id": "gap:output-a",
                "receipt_state": "receipt_movement_unchanged",
                "outcome_state": "evidence_unchanged",
                "seam_id": "output-a",
                "before": "weakly_gripped",
                "after": "weakly_gripped",
                "movement_source": "agent_receipt"
            },
            {
                "canonical_gap_id": "gap:output-b",
                "receipt_state": "receipt_movement_regressed",
                "outcome_state": "evidence_regressed",
                "seam_id": "output-b",
                "before": "weakly_gripped",
                "after": "reachable_unrevealed",
                "movement_source": "agent_receipt"
            },
            {
                "canonical_gap_id": "gap:unknown-route",
                "receipt_state": "receipt_missing",
                "outcome_state": "attempted_no_receipt",
                "seam_id": "unknown-route"
            },
            {
                "canonical_gap_id": "gap:call-a",
                "receipt_state": "receipt_missing",
                "outcome_state": "attempted_no_receipt",
                "seam_id": "call-a",
                "verify_result": "timed_out",
                "missing_receipt_reason": "repo exposure verify timed out before receipt capture"
            }
        ]
    });

    let report = ripr_swarm_attempt_ledger_from_values(
        "unix_ms:3".to_string(),
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "missing".to_string(),
            limitation: Some("no prior ledger".to_string()),
            value: None,
        },
    );

    assert_eq!(report.repair_route_quality.len(), 4);
    assert_eq!(
        report.repair_route_quality[0].repair_kind,
        "add_output_observer"
    );
    assert_eq!(report.repair_route_quality[0].attempted, 2);
    assert_eq!(report.repair_route_quality[0].unchanged, 1);
    assert_eq!(report.repair_route_quality[0].regressed, 1);
    assert!(
        report
            .top_missing_evidence_fields
            .iter()
            .any(|row| row.label == "repair_kind" && row.count == 1)
    );

    let json = ripr_swarm_attempt_ledger_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind"],
        "add_output_observer"
    );
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_attempted"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_missing_verify_result"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_failure_count"],
        serde_json::Value::from(4)
    );
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_dominant_failure_reason"],
        "missing_verify_result"
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["repair_kind"],
        "add_output_observer"
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["repair_kind_missing_verify_result"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["repair_kind_dominant_failure_reason"],
        "missing_verify_result"
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["sample_packet_ids"][0],
        serde_json::Value::from("packet-output-001")
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["sample_canonical_gap_ids"][0],
        serde_json::Value::from("gap:output-a")
    );
    let call_route = value["repair_route_quality"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["repair_kind"] == "add_call_observer")
        })
        .ok_or("missing add_call_observer route-quality row")?;
    assert_eq!(
        call_route["sample_missing_receipt_reasons"][0],
        serde_json::Value::from("repo exposure verify timed out before receipt capture")
    );
    let call_backlog = value["repair_route_quality_backlog"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["repair_kind"] == "add_call_observer")
        })
        .ok_or("missing add_call_observer route-quality backlog row")?;
    assert_eq!(
        call_backlog["sample_missing_receipt_reasons"][0],
        serde_json::Value::from("repo exposure verify timed out before receipt capture")
    );
    assert_eq!(
        call_backlog["improvement_route"],
        "report/repair-route-receipt-reliability/bounded-verify-route/add-call-observer"
    );
    assert_eq!(
        call_backlog["why_action_required"],
        "`add_call_observer` attempts timed out before receipt capture; repair guidance needs a bounded verify route before outcomes can be trusted"
    );
    assert_eq!(
        call_backlog["unlock_condition"],
        "replace broad `add_call_observer` verify or receipt routes with a bounded command that can complete and emit a receipt inside the runtime budget"
    );
    assert_eq!(
        value["repair_route_quality_backlog"][0]["packet_id"],
        "route-quality:add-output-observer:missing-verify-result"
    );
    assert_eq!(
        value["repair_route_quality_backlog"][0]["improvement_route"],
        "report/repair-route-verify-result-capture/add-output-observer"
    );
    assert_eq!(
        value["repair_route_quality_backlog"][0]["non_claims"][0],
        "not a public repair packet"
    );
    assert!(
        value["repair_route_quality_backlog"][0]["non_claims"]
            .as_array()
            .is_some_and(|claims| claims.iter().any(|claim| claim
                == "do not promote or downgrade actionability from route-quality evidence alone"))
    );
    assert_eq!(value["summary"]["missing_verify_result"], 4);
    assert!(
        value["top_missing_evidence_fields"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["label"] == "repair_kind"))
    );
    assert!(
        value["top_missing_evidence_fields"]
            .as_array()
            .is_some_and(|rows| rows
                .iter()
                .any(|row| row["label"] == "verify_result" && row["count"] == 4))
    );
    assert!(
        value["top_missing_evidence_fields"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| {
                row["label"] == "verify_result"
                    && row["sample_packet_ids"]
                        .as_array()
                        .is_some_and(|samples| !samples.is_empty())
                    && row["sample_canonical_gap_ids"]
                        .as_array()
                        .is_some_and(|samples| !samples.is_empty())
                    && row["sample_repair_kinds"]
                        .as_array()
                        .is_some_and(|samples| !samples.is_empty())
            }))
    );
    assert_eq!(
        value["attempts"][0]["target_test_type"],
        "boundary_discriminator"
    );
    let markdown = ripr_swarm_attempt_ledger_markdown(&report);
    assert!(markdown.contains("## Repair Route Quality"));
    assert!(markdown.contains("## Repair Route Quality Backlog"));
    assert!(markdown.contains("route-quality:add-output-observer:missing-verify-result"));
    assert!(markdown.contains("add_output_observer"));
    assert!(markdown.contains("Failure count | Dominant failure"));
    assert!(markdown.contains("Missing receipt reasons"));
    assert!(markdown.contains("Why action required | Unlock condition"));
    assert!(markdown.contains("missing_verify_result"));
    assert!(markdown.contains("repo exposure verify timed out before receipt capture"));
    assert!(markdown.contains(
        "report/repair-route-receipt-reliability/bounded-verify-route/add-call-observer"
    ));
    assert!(markdown.contains(
            "`add_call_observer` attempts timed out before receipt capture; repair guidance needs a bounded verify route before outcomes can be trusted"
        ));
    assert!(markdown.contains(
            "replace broad `add_call_observer` verify or receipt routes with a bounded command that can complete and emit a receipt inside the runtime budget"
        ));
    assert!(markdown.contains(
            "`add_output_observer` attempts lack typed verify results; route quality cannot be trusted until receipts preserve pass/fail/not-run evidence"
        ));
    assert!(markdown.contains(
            "preserve typed verify_result evidence for `add_output_observer` attempts in receipts or targeted-test outcomes"
        ));
    Ok(())
}

#[test]
fn ripr_swarm_repair_route_quality_samples_attempts_and_classifies_failures() {
    fn attempt(outcome: &str, verify_result: Option<&str>) -> RiprSwarmAttemptLedgerEntry {
        RiprSwarmAttemptLedgerEntry {
            packet_id: format!("packet-{outcome}"),
            canonical_gap_id: format!("gap:{outcome}"),
            attempt_id: format!("attempt:{outcome}"),
            language: None,
            evidence_class: Some("output_observer".to_string()),
            source_file: Some("crates/ripr/src/lib.rs".to_string()),
            repair_kind: Some("add_output_observer".to_string()),
            target_test_type: Some("output_observer".to_string()),
            assertion_shape: Some("assert_eq".to_string()),
            actor_kind: "agent".to_string(),
            receipt_path: None,
            verify_command: "cargo test -p ripr output_observer".to_string(),
            verify_result: verify_result.map(str::to_string),
            receipt_command: Some("cargo xtask receipts write --packet packet".to_string()),
            missing_receipt_reason: None,
            before_gap_state: Some("actionable".to_string()),
            after_gap_state: Some("actionable".to_string()),
            outcome: outcome.to_string(),
            timestamp: Some("unix_ms:1".to_string()),
            receipt_state: "receipt_present".to_string(),
            movement_source: Some("agent_receipt".to_string()),
            route_quality_expectation: None,
            reason: "fixture attempt".to_string(),
        }
    }

    assert!(ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("receipt_present", None)
    ));
    assert!(ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("attempted_no_receipt", Some("not_run"))
    ));
    assert!(ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("evidence_unchanged", Some("pass"))
    ));
    let mut expected_unchanged = attempt("evidence_unchanged", Some("pass"));
    expected_unchanged.route_quality_expectation =
        Some("expected_unchanged_negative_capability".to_string());
    assert!(!ripr_swarm_repair_route_quality_attempt_is_failure(
        &expected_unchanged
    ));
    let expected_unchanged_rows =
        crate::ripr_swarm_attempt_ledger_repair_route_quality(&[expected_unchanged]);
    assert_eq!(expected_unchanged_rows.len(), 1);
    assert_eq!(expected_unchanged_rows[0].unchanged, 1);
    assert_eq!(expected_unchanged_rows[0].expected_unchanged, 1);
    assert_eq!(
        crate::ripr_swarm_repair_route_quality_failure_count(&expected_unchanged_rows[0]),
        0
    );
    assert_eq!(
        crate::ripr_swarm_repair_route_quality_success_rate(&expected_unchanged_rows[0]),
        serde_json::Value::from(1.0)
    );
    assert!(ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("evidence_regressed", Some("fail"))
    ));
    assert!(ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("unknown", Some("pass"))
    ));
    assert!(ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("future_new_outcome", Some("pass"))
    ));
    assert!(!ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("not_attempted", None)
    ));
    assert!(!ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("evidence_improved", Some("pass"))
    ));
    assert!(!ripr_swarm_repair_route_quality_attempt_is_failure(
        &attempt("resolved", Some("pass"))
    ));
    let rows = crate::ripr_swarm_attempt_ledger_repair_route_quality(&[
        attempt("not_attempted", None),
        attempt("evidence_improved", Some("pass")),
        attempt("resolved", Some("pass")),
    ]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].attempted, 2);
    assert_eq!(
        rows[0].sample_packet_ids,
        vec![
            "packet-evidence_improved".to_string(),
            "packet-resolved".to_string()
        ]
    );
    assert_eq!(
        rows[0].sample_attempt_ids,
        vec![
            "attempt:evidence_improved".to_string(),
            "attempt:resolved".to_string()
        ]
    );
    assert_eq!(
        rows[0].sample_canonical_gap_ids,
        vec![
            "gap:evidence_improved".to_string(),
            "gap:resolved".to_string()
        ]
    );
}

#[test]
fn ripr_swarm_attempt_ledger_projects_historical_route_quality() -> Result<(), String> {
    fn attempt(
        packet_id: &str,
        outcome: &str,
        receipt_state: &str,
        verify_result: &str,
        timestamp: &str,
        missing_receipt_reason: Option<&str>,
    ) -> RiprSwarmAttemptLedgerEntry {
        RiprSwarmAttemptLedgerEntry {
            packet_id: packet_id.to_string(),
            canonical_gap_id: "gap:call-a".to_string(),
            attempt_id: format!("attempt:{packet_id}:{outcome}"),
            language: None,
            evidence_class: Some("call_presence".to_string()),
            source_file: Some("crates/ripr/src/lsp/tests.rs".to_string()),
            repair_kind: Some("add_call_observer".to_string()),
            target_test_type: Some("call_observer".to_string()),
            assertion_shape: Some("serve_stdio observer".to_string()),
            actor_kind: "codex".to_string(),
            receipt_path: None,
            verify_command: "cargo test -p ripr serve_stdio_call_presence_observer".to_string(),
            verify_result: Some(verify_result.to_string()),
            receipt_command: Some("cargo xtask dogfood".to_string()),
            missing_receipt_reason: missing_receipt_reason.map(str::to_string),
            before_gap_state: Some("actionable".to_string()),
            after_gap_state: Some("actionable".to_string()),
            outcome: outcome.to_string(),
            timestamp: Some(timestamp.to_string()),
            receipt_state: receipt_state.to_string(),
            movement_source: Some("real_repair_attempts".to_string()),
            route_quality_expectation: None,
            reason: "fixture attempt".to_string(),
        }
    }

    let attempts = vec![
        attempt(
            "packet-call-broad-receipt",
            "attempted_no_receipt",
            "receipt_missing",
            "not_run",
            "unix_ms:1",
            Some("repo exposure verify timed out before receipt capture"),
        ),
        attempt(
            "packet-call-bounded-receipt",
            "evidence_improved",
            "receipt_movement_improved",
            "pass",
            "unix_ms:2",
            None,
        ),
    ];
    let latest_attempts = ripr_swarm_attempt_ledger_latest_attempts(&attempts);
    let repair_route_quality = ripr_swarm_attempt_ledger_repair_route_quality(&latest_attempts);
    let historical_repair_route_quality = ripr_swarm_attempt_ledger_repair_route_quality(&attempts);

    assert_eq!(latest_attempts.len(), 1);
    assert_eq!(repair_route_quality[0].attempted, 1);
    assert_eq!(repair_route_quality[0].improved, 1);
    assert_eq!(
        ripr_swarm_repair_route_quality_failure_count(&repair_route_quality[0]),
        0
    );
    assert_eq!(historical_repair_route_quality[0].attempted, 2);
    assert_eq!(historical_repair_route_quality[0].improved, 1);
    assert_eq!(historical_repair_route_quality[0].attempted_no_receipt, 1);
    assert_eq!(
        ripr_swarm_repair_route_quality_failure_count(&historical_repair_route_quality[0]),
        1
    );
    assert_eq!(
        historical_repair_route_quality[0].sample_packet_ids,
        vec![
            "packet-call-broad-receipt".to_string(),
            "packet-call-bounded-receipt".to_string()
        ]
    );
    assert_eq!(
        historical_repair_route_quality[0].sample_attempt_ids,
        vec![
            "attempt:packet-call-broad-receipt:attempted_no_receipt".to_string(),
            "attempt:packet-call-bounded-receipt:evidence_improved".to_string()
        ]
    );

    let report = RiprSwarmAttemptLedgerReport {
        status: "advisory".to_string(),
        runtime_status: lane1_runtime_status_full(),
        generated_at: "unix_ms:3".to_string(),
        swarm_plan_path: "target/ripr/reports/swarm-plan.json".to_string(),
        swarm_plan_state: "read".to_string(),
        swarm_plan_limitation: None,
        actionable_gap_outcomes_path: "target/ripr/reports/actionable-gap-outcomes.json"
            .to_string(),
        actionable_gap_outcomes_state: "read".to_string(),
        actionable_gap_outcomes_limitation: None,
        prior_ledger_path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
        prior_ledger_state: "read".to_string(),
        prior_ledger_limitation: None,
        real_repair_attempts_path: "fixtures/real-repair-attempts/corpus.json".to_string(),
        real_repair_attempts_state: "read".to_string(),
        real_repair_attempts_limitation: None,
        attempts,
        latest_attempts,
        repair_route_quality,
        language_repair_route_quality: Vec::new(),
        historical_repair_route_quality,
        historical_language_repair_route_quality: Vec::new(),
        top_missing_evidence_fields: Vec::new(),
        orphaned_receipts: Vec::new(),
    };
    let value: serde_json::Value = serde_json::from_str(&ripr_swarm_attempt_ledger_json(&report)?)
        .map_err(|err| err.to_string())?;

    assert_eq!(value["top_failing_repair_routes"], serde_json::json!([]));
    assert_eq!(
        value["top_historical_failing_repair_routes"][0]["repair_kind"],
        "add_call_observer"
    );
    assert_eq!(
        value["top_historical_failing_repair_routes"][0]["sample_packet_ids"][0],
        "packet-call-broad-receipt"
    );
    assert_eq!(
        value["top_historical_failing_repair_routes"][0]["sample_attempt_ids"][0],
        "attempt:packet-call-broad-receipt:attempted_no_receipt"
    );
    let markdown = ripr_swarm_attempt_ledger_markdown(&report);
    assert!(markdown.contains("## Historical Repair Route Quality"));
    assert!(markdown.contains("## Historical Repair Route Quality Backlog"));
    assert!(markdown.contains("packet-call-broad-receipt"));
    assert!(markdown.contains("attempt:packet-call-broad-receipt:attempted_no_receipt"));
    assert!(markdown.contains("current routing still comes from latest attempts"));

    Ok(())
}

#[test]
fn ripr_swarm_readiness_consumes_attempt_ledger_counts() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "summary": {
            "swarm_ready_packets": 2,
            "blocked_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 2
        },
        "source_summary": {
            "actionable_gaps": 2,
            "public_projection_eligible_packets": 2
        },
        "static_limitation_backlog": {
            "source": "lane1-evidence-audit.static_limitations",
            "top_categories": [
                {
                    "category": "activation_value_unresolved",
                    "count": 7,
                    "repair_route": "analysis/value-resolution-audit-fixes"
                }
            ],
            "top_repair_routes": [
                {
                    "repair_route": "analysis/value-resolution-audit-fixes",
                    "count": 7
                }
            ]
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 2,
            "not_attempted": 2,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 4,
            "canonical_gaps_total": 3,
            "not_attempted": 1,
            "attempted_no_receipt": 0,
            "receipt_present": 0,
            "evidence_improved": 2,
            "evidence_unchanged": 1,
            "evidence_regressed": 0,
            "resolved": 0,
            "unknown": 0,
            "orphaned_receipts": 1
        },
        "attempt_history_summary": {
            "attempts_total": 6,
            "durable_attempts_total": 5,
            "canonical_gaps_total": 4,
            "not_attempted": 1,
            "attempted_no_receipt": 1,
            "receipt_present": 0,
            "missing_verify_result": 0,
            "evidence_improved": 2,
            "evidence_unchanged": 2,
            "expected_unchanged": 1,
            "evidence_regressed": 0,
            "resolved": 0,
            "unknown": 0
        },
        "repair_route_quality": [
            {
                "repair_kind": "add_output_observer",
                "repair_kind_attempted": 2,
                "repair_kind_improved": 0,
                "repair_kind_unchanged": 1,
                "repair_kind_regressed": 1,
                "repair_kind_resolved": 0,
                "repair_kind_attempted_no_receipt": 0,
                "repair_kind_receipt_present": 0,
                "repair_kind_unknown": 0,
                "repair_kind_success_rate": 0.0,
                "sample_packet_ids": ["packet-output-001"],
                "sample_attempt_ids": ["attempt-output-001"],
                "sample_canonical_gap_ids": ["gap:output-a"]
            }
        ],
        "top_missing_evidence_fields": [
            {"label": "receipt_command", "count": 1}
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    assert_eq!(report.summary.attempted_packets, 3);
    assert_eq!(report.summary.improved_packets, 2);
    assert_eq!(report.summary.unchanged_packets, 1);
    assert_eq!(report.summary.orphaned_receipts, 1);
    assert_eq!(report.attempt_history_summary.evidence_unchanged, 2);
    assert_eq!(report.attempt_history_summary.attempted_no_receipt, 1);
    assert_eq!(report.attempt_history_summary.expected_unchanged, 1);
    assert_eq!(
        report.repair_route_quality[0].repair_kind,
        "add_output_observer"
    );
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.kind == "improve_repair_route_quality"
                && action.repair_kind.as_deref() == Some("add_output_observer")
                && action.packet_id.as_deref()
                    == Some("route-quality:add-output-observer:regressed")
                && action.attempt_id.as_deref() == Some("attempt-output-001")
                && action.canonical_gap_id.as_deref() == Some("gap:output-a"))
    );
    assert!(report.next_actions.iter().any(|action| {
        action.kind == "inspect_unchanged_attempts"
            && action.command.as_deref() == Some("cargo xtask ripr-swarm attempt-ledger")
    }));
    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["inputs"]["attempt_ledger"]["path"],
        "target/ripr/reports/swarm-attempt-ledger.json"
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["repair_kind"],
        "add_output_observer"
    );
    assert_eq!(
        value["attempt_history_summary"]["evidence_unchanged"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["attempt_history_summary"]["attempted_no_receipt"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["attempt_history_summary"]["expected_unchanged"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["static_limitation_backlog"]["top_categories"][0]["category"],
        "activation_value_unresolved"
    );
    assert_eq!(
        value["static_limitation_backlog"]["top_repair_routes"][0]["repair_route"],
        "analysis/value-resolution-audit-fixes"
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["repair_kind_failure_count"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["repair_kind_dominant_failure_reason"],
        "regressed"
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["sample_packet_ids"][0],
        serde_json::Value::from("packet-output-001")
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["sample_attempt_ids"][0],
        serde_json::Value::from("attempt-output-001")
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["sample_canonical_gap_ids"][0],
        serde_json::Value::from("gap:output-a")
    );
    assert_eq!(
        value["repair_route_quality_backlog"][0]["packet_id"],
        "route-quality:add-output-observer:regressed"
    );
    assert_eq!(
        value["repair_route_quality_backlog"][0]["improvement_route"],
        "analysis/repair-route-regression-review/add-output-observer"
    );
    assert_eq!(
        value["repair_route_quality_backlog"][0]["sample_packet_ids"][0],
        "packet-output-001"
    );
    assert_eq!(
        value["repair_route_quality_backlog"][0]["sample_attempt_ids"][0],
        "attempt-output-001"
    );
    assert!(
        value["next_actions"]
            .as_array()
            .is_some_and(|actions| actions.iter().any(|action| {
                action["kind"] == "improve_repair_route_quality"
                    && action["packet_id"] == "route-quality:add-output-observer:regressed"
                    && action["attempt_id"] == "attempt-output-001"
                    && action["canonical_gap_id"] == "gap:output-a"
                    && action["command"] == "cargo xtask ripr-swarm readiness"
                    && action["reason"].as_str().is_some_and(|reason| {
                        reason.contains("dominant reason `regressed`")
                            && reason.contains(
                                "analysis/repair-route-regression-review/add-output-observer",
                            )
                            && reason.contains("sample failed packet `packet-output-001`")
                            && reason.contains("attempt `attempt-output-001`")
                    })
            }))
    );
    let markdown = ripr_swarm_readiness_markdown(&report);
    assert!(markdown.contains("## Attempt History Summary"));
    assert!(markdown.contains("| evidence unchanged | 2 |"));
    assert!(markdown.contains("## Static Limitation Backlog"));
    assert!(markdown.contains("analysis/value-resolution-audit-fixes"));
    assert!(markdown.contains("## Repair Route Quality Backlog"));
    assert!(markdown.contains("route-quality:add-output-observer:regressed"));
    assert!(markdown.contains("Failure count | Dominant failure"));
    assert!(markdown.contains("regressed"));
    assert_eq!(
        value["top_missing_evidence_fields"][0]["label"],
        "receipt_command"
    );
    assert_eq!(
        value["top_missing_evidence_fields"][0]["sample_packet_ids"],
        serde_json::Value::Array(Vec::new())
    );
    assert_eq!(
        value["top_missing_evidence_fields"][0]["sample_canonical_gap_ids"],
        serde_json::Value::Array(Vec::new())
    );
    assert_eq!(
        value["top_next_action"]["kind"],
        "reconcile_orphaned_receipts"
    );
    assert_eq!(value["top_next_action"], value["next_actions"][0]);
    Ok(())
}

#[test]
fn ripr_swarm_readiness_recomputes_route_quality_from_latest_attempts_when_rows_absent()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "summary": {
            "swarm_ready_packets": 1,
            "blocked_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 1
        },
        "source_summary": {
            "actionable_gaps": 1,
            "public_projection_eligible_packets": 1
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 1,
            "not_attempted": 0,
            "evidence_improved": 1,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 2,
            "canonical_gaps_total": 1,
            "not_attempted": 0,
            "attempted_no_receipt": 0,
            "receipt_present": 0,
            "evidence_improved": 1,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "unknown": 0,
            "orphaned_receipts": 0
        },
        "attempts": [
            {
                "packet_id": "packet-output-001",
                "canonical_gap_id": "gap:output-observer",
                "attempt_id": "attempt:older-unchanged",
                "actor_kind": "agent",
                "language": "typescript",
                "repair_kind": "add_output_observer",
                "verify_command": "cargo test -p ripr output_observer",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-output-001",
                "outcome": "evidence_unchanged",
                "timestamp": "unix_ms:9"
            },
            {
                "packet_id": "packet-output-001",
                "canonical_gap_id": "gap:output-observer",
                "attempt_id": "attempt:newer-improved",
                "actor_kind": "agent",
                "language": "typescript",
                "repair_kind": "add_output_observer",
                "verify_command": "cargo test -p ripr output_observer",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-output-001",
                "outcome": "evidence_improved",
                "timestamp": "unix_ms:10"
            }
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    assert_eq!(report.repair_route_quality.len(), 1);
    assert_eq!(
        report.repair_route_quality[0].repair_kind,
        "add_output_observer"
    );
    assert_eq!(report.repair_route_quality[0].improved, 1);
    assert_eq!(report.repair_route_quality[0].unchanged, 0);
    assert!(report.top_failing_repair_routes.is_empty());
    assert!(!report.next_actions.iter().any(|action| {
        action.kind == "improve_repair_route_quality"
            && action.repair_kind.as_deref() == Some("add_output_observer")
    }));
    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_improved"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_unchanged"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        value["language_repair_route_quality"][0]["language"],
        serde_json::Value::from("typescript")
    );
    assert_eq!(
        value["language_repair_route_quality"][0]["repair_kind"],
        serde_json::Value::from("add_output_observer")
    );
    assert_eq!(
        value["language_repair_route_quality"][0]["repair_kind_improved"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["top_failing_repair_routes"],
        serde_json::Value::Array(Vec::new())
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_routes_missing_verify_result_quality() -> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "summary": {
            "swarm_ready_packets": 1,
            "blocked_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 1
        },
        "source_summary": {
            "actionable_gaps": 1,
            "public_projection_eligible_packets": 1
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 2,
            "not_attempted": 0,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 2,
            "canonical_gaps_total": 2,
            "not_attempted": 0,
            "attempted_no_receipt": 0,
            "receipt_present": 2,
            "missing_verify_result": 2,
            "evidence_improved": 0,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "unknown": 0,
            "orphaned_receipts": 0
        },
        "repair_route_quality": [
            {
                "repair_kind": "add_exact_error_variant_observer",
                "repair_kind_attempted": 2,
                "repair_kind_improved": 0,
                "repair_kind_unchanged": 0,
                "repair_kind_regressed": 0,
                "repair_kind_resolved": 0,
                "repair_kind_attempted_no_receipt": 0,
                "repair_kind_receipt_present": 2,
                "repair_kind_missing_verify_result": 2,
                "repair_kind_unknown": 0,
                "repair_kind_success_rate": 0.0
            }
        ],
        "top_missing_evidence_fields": [
            {"label": "verify_result", "count": 2}
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["top_failing_repair_routes"][0]["repair_kind"],
        "add_exact_error_variant_observer"
    );
    assert_eq!(
        value["top_failing_repair_routes"][0]["repair_kind_dominant_failure_reason"],
        "missing_verify_result"
    );
    assert_eq!(
        value["top_next_action"]["kind"],
        "inspect_missing_verify_results"
    );
    assert!(
        value["next_actions"]
            .as_array()
            .is_some_and(|actions| actions.iter().any(|action| {
                action["kind"] == "improve_repair_route_quality"
                    && action["reason"].as_str().is_some_and(|reason| {
                        reason.contains("dominant reason `missing_verify_result`")
                    })
            }))
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_recomputes_route_quality_from_attempts_when_rows_are_stale()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "summary": {
            "swarm_ready_packets": 1,
            "blocked_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 1
        },
        "source_summary": {
            "actionable_gaps": 1,
            "public_projection_eligible_packets": 1
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 1,
            "not_attempted": 0,
            "evidence_improved": 1,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 1,
            "canonical_gaps_total": 1,
            "not_attempted": 0,
            "attempted_no_receipt": 0,
            "receipt_present": 0,
            "evidence_improved": 1,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "unknown": 0,
            "orphaned_receipts": 0
        },
        "repair_route_quality": [
            {
                "repair_kind": "add_output_observer",
                "repair_kind_attempted": 1,
                "repair_kind_improved": 0,
                "repair_kind_unchanged": 1,
                "repair_kind_regressed": 0,
                "repair_kind_resolved": 0,
                "repair_kind_attempted_no_receipt": 0,
                "repair_kind_receipt_present": 0,
                "repair_kind_unknown": 0,
                "repair_kind_success_rate": 0.0
            }
        ],
        "attempts": [
            {
                "packet_id": "packet-output-001",
                "canonical_gap_id": "gap:output-observer",
                "attempt_id": "attempt:newer-improved",
                "actor_kind": "agent",
                "repair_kind": "add_output_observer",
                "verify_command": "cargo test -p ripr output_observer",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-output-001",
                "outcome": "evidence_improved",
                "timestamp": "unix_ms:10"
            }
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    assert_eq!(report.repair_route_quality.len(), 1);
    assert_eq!(report.repair_route_quality[0].improved, 1);
    assert_eq!(report.repair_route_quality[0].unchanged, 0);
    assert!(report.top_failing_repair_routes.is_empty());
    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_improved"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["repair_route_quality"][0]["repair_kind_unchanged"],
        serde_json::Value::from(0)
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_recomputes_summary_from_attempts_when_summary_is_stale()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "summary": {
            "swarm_ready_packets": 1,
            "blocked_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 1
        },
        "source_summary": {
            "actionable_gaps": 1,
            "public_projection_eligible_packets": 1
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 1,
            "not_attempted": 0,
            "evidence_improved": 1,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 1,
            "canonical_gaps_total": 1,
            "not_attempted": 0,
            "attempted_no_receipt": 0,
            "receipt_present": 0,
            "evidence_improved": 0,
            "evidence_unchanged": 1,
            "evidence_regressed": 0,
            "resolved": 0,
            "unknown": 0,
            "orphaned_receipts": 0
        },
        "attempts": [
            {
                "packet_id": "packet-output-001",
                "canonical_gap_id": "gap:output-observer",
                "attempt_id": "attempt:older-unchanged",
                "actor_kind": "agent",
                "repair_kind": "add_output_observer",
                "verify_command": "cargo test -p ripr output_observer",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-output-001",
                "outcome": "evidence_unchanged",
                "timestamp": "unix_ms:1"
            },
            {
                "packet_id": "packet-output-001",
                "canonical_gap_id": "gap:output-observer",
                "attempt_id": "attempt:newer-improved",
                "actor_kind": "agent",
                "repair_kind": "add_output_observer",
                "verify_command": "cargo test -p ripr output_observer",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-output-001",
                "outcome": "evidence_improved",
                "timestamp": "unix_ms:10"
            }
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    assert_eq!(report.summary.attempted_packets, 1);
    assert_eq!(report.summary.improved_packets, 1);
    assert_eq!(report.summary.unchanged_packets, 0);
    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["summary"]["improved_packets"], 1);
    assert_eq!(value["summary"]["unchanged_packets"], 0);
    assert_eq!(
        value["attempt_history_summary"]["attempts_total"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["attempt_history_summary"]["evidence_improved"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["attempt_history_summary"]["evidence_unchanged"],
        serde_json::Value::from(1)
    );
    Ok(())
}

#[test]
fn ripr_swarm_readiness_recomputes_missing_evidence_from_attempts_when_rows_are_stale()
-> Result<(), String> {
    let swarm_plan = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-plan",
        "summary": {
            "swarm_ready_packets": 1,
            "blocked_packets": 0,
            "missing_verify_command": 0,
            "missing_receipt_command": 0,
            "static_limitation_packets": 0,
            "high_confidence_packets": 1
        },
        "source_summary": {
            "actionable_gaps": 1,
            "public_projection_eligible_packets": 1
        },
        "top_ready_packets": []
    });
    let outcomes = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "actionable-gap-outcomes",
        "summary": {
            "outcomes_total": 1,
            "not_attempted": 0,
            "evidence_improved": 1,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        }
    });
    let attempt_ledger = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-attempt-ledger",
        "summary": {
            "attempts_total": 1,
            "canonical_gaps_total": 1,
            "not_attempted": 0,
            "evidence_improved": 1,
            "evidence_unchanged": 0,
            "evidence_regressed": 0,
            "resolved": 0,
            "orphaned_receipts": 0
        },
        "top_missing_evidence_fields": [
            {
                "label": "receipt_command",
                "count": 1
            }
        ],
        "attempts": [
            {
                "packet_id": "packet-output-001",
                "canonical_gap_id": "gap:output-observer",
                "attempt_id": "attempt:newer-improved",
                "actor_kind": "agent",
                "repair_kind": "add_output_observer",
                "verify_command": "cargo test -p ripr output_observer",
                "verify_result": "pass",
                "receipt_command": "cargo xtask receipts write --packet packet-output-001",
                "outcome": "evidence_improved",
                "timestamp": "unix_ms:10"
            }
        ]
    });

    let report = ripr_swarm_readiness_from_values(
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-plan.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&swarm_plan),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/actionable-gap-outcomes.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&outcomes),
        },
        RiprSwarmReadinessInput {
            path: "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
            state: "read".to_string(),
            limitation: None,
            value: Some(&attempt_ledger),
        },
    );

    assert!(report.top_missing_evidence_fields.is_empty());
    let json = ripr_swarm_readiness_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["top_missing_evidence_fields"],
        serde_json::Value::Array(Vec::new())
    );
    Ok(())
}
