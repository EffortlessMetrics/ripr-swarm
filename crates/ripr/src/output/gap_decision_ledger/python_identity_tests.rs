use super::*;
use crate::analysis::{AnalysisMode, AnalysisOptions};
use crate::app::{CheckOutput, Mode};
use crate::config::OraclePolicy;
use crate::domain::{ExposureClass, LanguageId, Summary};
use serde_json::json;
use std::path::PathBuf;

// Run the real parser, candidate ordering, runnable selector, card and check
// renderer. Only the failure-boundary tests below mutate the resulting JSON.
fn check_output(first_file: &str) -> Result<Value, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("ripr-ledger-8132-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    let result = (|| {
        std::fs::write(root.join("src/pricing.py"), "def calculate_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n").map_err(|error| error.to_string())?;
        for (file, line) in [(first_file, 10), ("tests/test_plain.py", 60)] {
            let source = format!(
                "import unittest\nfrom src.pricing import calculate_discount\n{}class CheckoutTests(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(calculate_discount(150, 100))\n",
                "\n".repeat(line - 4)
            );
            std::fs::write(root.join(file), source).map_err(|error| error.to_string())?;
        }
        let options = AnalysisOptions {
            root: root.clone(),
            base: None,
            diff_file: Some(root.join("change.patch")),
            mode: AnalysisMode::Draft,
            include_unchanged_tests: false,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            perl_producer_failure: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
            resolved_subject_identity: None,
            open_rust_index_paths: Default::default(),
        };
        std::fs::write(root.join("change.patch"), "diff --git a/src/pricing.py b/src/pricing.py\n--- a/src/pricing.py\n+++ b/src/pricing.py\n@@ -1,4 +1,4 @@\n def calculate_discount(amount, threshold):\n-    if amount > threshold:\n+    if amount >= threshold:\n         return amount - 10\n     return amount\n").map_err(|error| error.to_string())?;
        let analyzed = crate::analysis::run_analysis_with_oracle_policy(
            &options,
            &OraclePolicy::default(),
            &[LanguageId::Python],
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(analyzed.findings.len(), 1);
        assert_eq!(analyzed.findings[0].class, ExposureClass::WeaklyExposed);
        let output = CheckOutput {
            schema_version: "0.2".to_string(),
            harness_projections: Vec::new(),
            tool: "ripr".to_string(),
            mode: Mode::Draft,
            root: PathBuf::from("."),
            base: None,
            analysis_outcome: None,
            summary: Summary::default(),
            findings: analyzed.findings,
            preview_language_advisories: Vec::new(),
            language_runs: Vec::new(),
            no_scope_provided: false,
            unanalyzed_working_tree: false,
            untracked_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            partial_scope: None,
            analyzed_revisions: None,
        };
        serde_json::from_str(&crate::output::json::render(&output))
            .map_err(|error| error.to_string())
    })();
    std::fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    result
}

fn projected_record(payload: &Value) -> Result<Value, String> {
    let report = build_gap_decision_ledger_report(GapDecisionLedgerInput {
        root: ".".to_string(),
        generated_at: "test".to_string(),
        source_kind: GapDecisionLedgerSourceKind::CheckOutput,
        records_path: "check.json".to_string(),
        records_json: Ok(payload.to_string()),
    });
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.records.len(), 1);
    let rendered: Value = serde_json::from_str(&render_gap_decision_ledger_json(&report)?)
        .map_err(|error| error.to_string())?;
    Ok(rendered["records"][0].clone())
}

fn expected_record(
    file: &str,
    name: Option<&str>,
    line: Option<u64>,
    command: &str,
) -> Result<Value, String> {
    let mut expected: Value =
        serde_json::from_str(&EXPECTED_RECORD.replace(SECOND_COMMAND, command))
            .map_err(|error| error.to_string())?;
    let route = expected["repair_route"]
        .as_object_mut()
        .ok_or("expected route missing")?;
    route.insert("target_file".to_string(), json!(file));
    match name {
        Some(name) => {
            route.insert("related_test".to_string(), json!(name));
        }
        None => {
            route.remove("related_test");
        }
    }
    match line {
        Some(line) => {
            route.insert("target_line".to_string(), json!(line));
        }
        None => {
            route.remove("target_line");
        }
    }
    expected["verification_commands"] = json!([command]);
    Ok(expected)
}

const SECOND_COMMAND: &str = "python -m unittest tests.test_plain.CheckoutTests.test_same";

#[test]
fn python_selected_second_line_follows_runnable_card_identity() -> Result<(), String> {
    let payload = check_output("tests/test_checkout.v2.py")?;
    let finding = &payload["findings"][0];
    assert_eq!(
        finding["related_tests"][0]["file"],
        "tests/test_checkout.v2.py"
    );
    assert_eq!(finding["related_tests"][0]["line"], 10);
    assert_eq!(finding["related_tests"][1]["file"], "tests/test_plain.py");
    assert_eq!(finding["related_tests"][1]["line"], 60);
    assert_eq!(
        finding["python_repair_card"]["suggested_location"]["test_file"],
        "tests/test_plain.py"
    );
    assert_eq!(
        finding["python_repair_card"]["verify"]["command"],
        SECOND_COMMAND
    );
    assert!(
        !finding["evidence"]
            .as_array()
            .ok_or("missing evidence")?
            .iter()
            .filter_map(Value::as_str)
            .any(|entry| entry.starts_with("test_verify_command:")
                && entry.contains("test_checkout.v2"))
    );
    let record = projected_record(&payload)?;
    assert_eq!(record["repair_route"]["target_line"], 60);
    assert_eq!(
        record,
        expected_record(
            "tests/test_plain.py",
            Some("test_same"),
            Some(60),
            SECOND_COMMAND
        )?
    );
    Ok(())
}

#[test]
fn python_selected_first_keeps_its_line_with_same_name_in_other_file() -> Result<(), String> {
    let payload = check_output("tests/test_checkout.py")?;
    assert_eq!(
        payload["findings"][0]["related_tests"]
            .as_array()
            .ok_or("missing related rows")?
            .len(),
        2
    );
    let command = "python -m unittest tests.test_checkout.CheckoutTests.test_same";
    assert_eq!(
        payload["findings"][0]["python_repair_card"]["verify"]["command"],
        command
    );
    assert_eq!(
        projected_record(&payload)?,
        expected_record(
            "tests/test_checkout.py",
            Some("test_same"),
            Some(10),
            command
        )?
    );
    Ok(())
}

#[test]
fn python_selected_identity_missing_unmatched_or_ambiguous_withholds_line() -> Result<(), String> {
    let original = check_output("tests/test_checkout.v2.py")?;
    for case in [
        "absent-name",
        "unmatched-name",
        "unmatched-file",
        "absent-rows",
        "duplicate-identity",
        "missing-line",
        "zero-line",
        "blank-name",
    ] {
        let mut payload = original.clone();
        let finding = &mut payload["findings"][0];
        let mut file = "tests/test_plain.py";
        let mut name = Some("test_same");
        match case {
            "absent-name" => {
                finding["python_repair_card"]["suggested_location"]
                    .as_object_mut()
                    .ok_or("missing location")?
                    .remove("test_name");
                name = None;
            }
            "unmatched-name" => {
                name = Some("test_other");
                finding["python_repair_card"]["suggested_location"]["test_name"] =
                    json!("test_other");
            }
            "unmatched-file" => {
                file = "tests/test_absent.py";
                finding["python_repair_card"]["suggested_location"]["test_file"] = json!(file);
            }
            "absent-rows" => {
                finding["related_tests"] = json!([]);
            }
            "duplicate-identity" => {
                let mut duplicate = finding["related_tests"][1].clone();
                duplicate["line"] = json!(90);
                finding["related_tests"]
                    .as_array_mut()
                    .ok_or("missing rows")?
                    .push(duplicate);
            }
            "missing-line" => {
                finding["related_tests"][1]
                    .as_object_mut()
                    .ok_or("missing row")?
                    .remove("line");
            }
            "zero-line" => {
                finding["related_tests"][1]["line"] = json!(0);
            }
            "blank-name" => {
                name = Some("");
                finding["python_repair_card"]["suggested_location"]["test_name"] = json!("");
                finding["related_tests"][1]["name"] = json!("");
            }
            _ => return Err("unknown case".to_string()),
        }
        assert_eq!(
            projected_record(&payload)?,
            expected_record(file, name, None, SECOND_COMMAND)?,
            "{case}"
        );
    }
    Ok(())
}

// Complete rendered record: route identity is varied explicitly by each case.
const EXPECTED_RECORD: &str = r#"{
  "anchor": {
    "dedupe_fingerprint": "gap:python:src/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold",
    "file": "src/pricing.py",
    "line": 2,
    "owner": "calculate_discount"
  },
  "authority_boundary": "preview_advisory_only",
  "canonical_gap_id": "gap:python:src/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold",
  "evidence_class": "predicate_boundary",
  "evidence_ids": [
    "probe:src_pricing.py:python_preview:b6c16437",
    "gap:python:src/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold"
  ],
  "gap_id": "gap:pr:gap:python:src/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold",
  "gap_state": "actionable",
  "kind": "MissingBoundaryAssertion",
  "language": "python",
  "language_status": "preview",
  "policy_state": "new",
  "projection_eligibility": {
    "agent_packet": {
      "eligible": true,
      "reason": "python_no_strong_oracle_with_bounded_repair_route"
    },
    "ci_summary": {
      "eligible": true,
      "reason": "pr_local_gap_record"
    },
    "gate_candidate": {
      "eligible": false,
      "reason": "policy_target_not_supplied"
    },
    "lsp_diagnostic": {
      "eligible": true,
      "reason": "local_file_scope"
    },
    "pr_comment": {
      "eligible": true,
      "reason": "stable_anchor_and_repair_route"
    },
    "report_packet": {
      "eligible": true,
      "reason": "all_gap_records_are_reportable"
    },
    "ripr_plus_count": {
      "eligible": false,
      "reason": "pr_local_not_repo_scoped"
    },
    "ripr_zero_count": {
      "eligible": false,
      "reason": "pr_local_not_repo_scoped"
    }
  },
  "receipt_command": "ripr receipt write --gap 'gap:python:src/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold' --verify-command 'python -m unittest tests.test_plain.CheckoutTests.test_same' --status not_run --out target/ripr/receipts/gap-python-src-pricing.py-calculate_discount-predicate_boundary-predicate-amount-threshold.json",
  "regeneration_commands": [
    "ripr check --root . --json > target/ripr/reports/after-check.json"
  ],
  "repair_route": {
    "assertion_shape": "Assert the owner result or effect at the boundary `amount == threshold`.",
    "changed_behavior": "predicate_boundary changed at src/pricing.py:2: `if amount >= threshold:`",
    "missing_discriminator": "amount == threshold",
    "related_test": "test_same",
    "route_kind": "StrengthenExistingTest",
    "stop_conditions": [
      "Stop if imports, fixtures, or test setup cannot call the changed owner.",
      "Stop if the expected value for the missing discriminator is ambiguous.",
      "Stop if adding the test appears to require a production-code edit."
    ],
    "target_file": "tests/test_plain.py",
    "target_line": 60
  },
  "repairability": "repairable",
  "scope": "pr_local",
  "source_currentness": "candidate_current",
  "static_limit_detail": "Python repair cards are preview advisory evidence.",
  "static_limit_kind": "python_preview",
  "static_limits": [
    {
      "detail": "Syntax-first Python preview evidence only.",
      "kind": "python_preview_limit"
    },
    {
      "detail": "No source edits, generated tests, mutation execution, provider calls, or gate authority.",
      "kind": "python_preview_limit"
    },
    {
      "detail": "Verify success alone is not a gap-closure receipt.",
      "kind": "python_preview_limit"
    }
  ],
  "verification_commands": [
    "python -m unittest tests.test_plain.CheckoutTests.test_same"
  ]
}"#;
