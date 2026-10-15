use super::*;
use crate::analysis::{AnalysisMode, AnalysisOptions};
use crate::app::{CheckOutput, Mode};
use crate::config::OraclePolicy;
use crate::domain::{ExposureClass, LanguageId, Summary};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// Run the real parser, candidate ordering, runnable selector, card and check
// renderer. Only the failure-boundary tests below mutate the resulting JSON.
enum Layout {
    TwoFile { first: &'static str },
    CappedMixed,
}

struct TempRoot(PathBuf);

impl TempRoot {
    fn create() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
        let root = std::env::temp_dir().join(format!(
            "ripr-ledger-8132-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn check_output(layout: Layout) -> Result<Value, String> {
    let root = TempRoot::create()?;
    std::fs::write(
        root.0.join("src/pricing.py"),
        "def calculate_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
    )
    .map_err(|error| error.to_string())?;
    match layout {
        Layout::TwoFile { first } => {
            for (file, line) in [(first, 10), ("tests/test_plain.py", 60)] {
                let source = format!(
                    "import unittest\nfrom src.pricing import calculate_discount\n{}class CheckoutTests(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(calculate_discount(150, 100))\n",
                    "\n".repeat(line - 4)
                );
                std::fs::write(root.0.join(file), source).map_err(|error| error.to_string())?;
            }
        }
        Layout::CappedMixed => {
            let mut source =
                String::from("import unittest\nfrom src.pricing import calculate_discount\n");
            for index in 0..7 {
                source.push_str(&format!(
                    "class Early{index}(unittest.TestCase):\n    def test_early{index}(self):\n        self.assertTrue(calculate_discount(150, 100))\n"
                ));
            }
            source.push_str(
                "class CheckoutTests(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(calculate_discount(150, 100))\nclass TestGood:\n    def test_same(self):\n        calculate_discount(150, 100)\n",
            );
            std::fs::write(root.0.join("tests/test_mixed.v2.py"), source)
                .map_err(|error| error.to_string())?;
        }
    }
    std::fs::write(
        root.0.join("change.patch"),
        "diff --git a/src/pricing.py b/src/pricing.py\n--- a/src/pricing.py\n+++ b/src/pricing.py\n@@ -1,4 +1,4 @@\n def calculate_discount(amount, threshold):\n-    if amount > threshold:\n+    if amount >= threshold:\n         return amount - 10\n     return amount\n",
    )
    .map_err(|error| error.to_string())?;
    let options = AnalysisOptions {
        root: root.0.clone(),
        base: None,
        diff_file: Some(root.0.join("change.patch")),
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
    let analyzed = crate::analysis::run_analysis_with_oracle_policy(
        &options,
        &OraclePolicy::default(),
        &[LanguageId::Python],
    )
    .map_err(|error| error.to_string())?;
    if analyzed.findings.len() != 1 {
        return Err(format!(
            "expected one finding, got {}",
            analyzed.findings.len()
        ));
    }
    if analyzed.findings[0].class != ExposureClass::WeaklyExposed {
        return Err(format!(
            "expected weakly_exposed, got {:?}",
            analyzed.findings[0].class
        ));
    }
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
    serde_json::from_str(&crate::output::json::render(&output)).map_err(|error| error.to_string())
}

fn projected_record(payload: &Value) -> Result<Value, String> {
    let report = build_gap_decision_ledger_report(GapDecisionLedgerInput {
        root: ".".to_string(),
        generated_at: "test".to_string(),
        source_kind: GapDecisionLedgerSourceKind::CheckOutput,
        records_path: "check.json".to_string(),
        records_json: Ok(payload.to_string()),
    });
    if !report.warnings.is_empty() {
        return Err(format!("unexpected warnings: {:?}", report.warnings));
    }
    if report.records.len() != 1 {
        return Err(format!("expected one record, got {}", report.records.len()));
    }
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
const MIXED_COMMAND: &str = "python -m pytest tests/test_mixed.v2.py::TestGood::test_same";

#[test]
fn python_selected_second_line_follows_runnable_card_identity() -> Result<(), String> {
    let payload = check_output(Layout::TwoFile {
        first: "tests/test_checkout.v2.py",
    })?;
    let finding = &payload["findings"][0];
    if finding["related_tests"][0]["file"] != "tests/test_checkout.v2.py" {
        return Err(format!(
            "first related file: {}",
            finding["related_tests"][0]["file"]
        ));
    }
    if finding["related_tests"][0]["line"] != 10 {
        return Err(format!(
            "first related line: {}",
            finding["related_tests"][0]["line"]
        ));
    }
    if finding["related_tests"][1]["file"] != "tests/test_plain.py" {
        return Err(format!(
            "second related file: {}",
            finding["related_tests"][1]["file"]
        ));
    }
    if finding["related_tests"][1]["line"] != 60 {
        return Err(format!(
            "second related line: {}",
            finding["related_tests"][1]["line"]
        ));
    }
    if finding["python_repair_card"]["suggested_location"]["test_file"] != "tests/test_plain.py" {
        return Err(format!(
            "selected file: {}",
            finding["python_repair_card"]["suggested_location"]["test_file"]
        ));
    }
    if finding["python_repair_card"]["verify"]["command"] != SECOND_COMMAND {
        return Err(format!(
            "selected command: {}",
            finding["python_repair_card"]["verify"]["command"]
        ));
    }
    let has_dotted_verify = finding["evidence"]
        .as_array()
        .ok_or("missing evidence")?
        .iter()
        .filter_map(Value::as_str)
        .any(|entry| {
            entry.starts_with("test_verify_command:") && entry.contains("test_checkout.v2")
        });
    if has_dotted_verify {
        return Err("dotted unittest path must not become a verify command".to_string());
    }
    let record = projected_record(&payload)?;
    if record["repair_route"]["target_line"] != 60 {
        return Err(format!(
            "selected-second line must follow the runnable card, got {}",
            record["repair_route"]["target_line"]
        ));
    }
    if record
        != expected_record(
            "tests/test_plain.py",
            Some("test_same"),
            Some(60),
            SECOND_COMMAND,
        )?
    {
        return Err(format!("complete record mismatch: {record}"));
    }
    Ok(())
}

#[test]
fn python_selected_first_keeps_its_line_with_same_name_in_other_file() -> Result<(), String> {
    let payload = check_output(Layout::TwoFile {
        first: "tests/test_checkout.py",
    })?;
    let related = payload["findings"][0]["related_tests"]
        .as_array()
        .ok_or("missing related rows")?;
    if related.len() != 2 {
        return Err(format!("expected two related rows, got {}", related.len()));
    }
    let command = "python -m unittest tests.test_checkout.CheckoutTests.test_same";
    if payload["findings"][0]["python_repair_card"]["verify"]["command"] != command {
        return Err(format!(
            "selected command: {}",
            payload["findings"][0]["python_repair_card"]["verify"]["command"]
        ));
    }
    if projected_record(&payload)?
        != expected_record(
            "tests/test_checkout.py",
            Some("test_same"),
            Some(10),
            command,
        )?
    {
        return Err("selected-first complete record mismatch".to_string());
    }
    Ok(())
}

#[test]
fn python_truncated_inventory_does_not_bind_visible_namesake() -> Result<(), String> {
    let payload = check_output(Layout::CappedMixed)?;
    let finding = &payload["findings"][0];
    let total = finding["related_tests_total"]
        .as_u64()
        .ok_or("missing related_tests_total")?;
    let rows = finding["related_tests"]
        .as_array()
        .ok_or("missing related_tests")?;
    if total != 9 {
        return Err(format!("expected related_tests_total 9, got {total}"));
    }
    if rows.len() != 8 {
        return Err(format!(
            "expected 8 rendered related rows, got {}",
            rows.len()
        ));
    }
    if finding["python_repair_card"]["suggested_location"]["test_file"] != "tests/test_mixed.v2.py"
    {
        return Err(format!(
            "selected file: {}",
            finding["python_repair_card"]["suggested_location"]["test_file"]
        ));
    }
    if finding["python_repair_card"]["verify"]["command"] != MIXED_COMMAND {
        return Err(format!(
            "selected command: {}",
            finding["python_repair_card"]["verify"]["command"]
        ));
    }
    let evidence_line = finding["evidence"]
        .as_array()
        .ok_or("missing evidence")?
        .iter()
        .filter_map(Value::as_str)
        .find_map(|entry| entry.strip_prefix("suggested_test_line: "))
        .ok_or("missing suggested_test_line evidence")?
        .parse::<u64>()
        .map_err(|error| error.to_string())?;
    if evidence_line != 28 {
        return Err(format!(
            "capped fixture must select pytest line 28, got {evidence_line}"
        ));
    }
    let visible_namesake = rows
        .iter()
        .find(|row| row["file"] == "tests/test_mixed.v2.py" && row["name"] == "test_same")
        .ok_or("visible inventory must retain the unittest namesake")?;
    let visible_line = visible_namesake["line"]
        .as_u64()
        .ok_or("visible namesake missing line")?;
    if visible_line == evidence_line {
        return Err("capped fixture must hide the selected pytest line".to_string());
    }
    if rows.iter().any(|row| row["line"] == evidence_line) {
        return Err("selected pytest line must be outside the rendered cap".to_string());
    }
    let record = projected_record(&payload)?;
    if record["repair_route"]["target_line"] != evidence_line {
        return Err(format!(
            "truncated inventory must keep the selected line {evidence_line}, got {}",
            record["repair_route"]["target_line"]
        ));
    }
    let expected: Value =
        serde_json::from_str(EXPECTED_MIXED_RECORD).map_err(|error| error.to_string())?;
    if record != expected {
        return Err(format!("capped complete record mismatch: {record}"));
    }
    Ok(())
}

#[test]
fn python_selected_identity_missing_unmatched_or_ambiguous_withholds_line() -> Result<(), String> {
    let original = check_output(Layout::TwoFile {
        first: "tests/test_checkout.v2.py",
    })?;
    for case in [
        "absent-name",
        "unmatched-name",
        "unmatched-file",
        "absent-rows",
        "duplicate-identity",
        "missing-line",
        "zero-line",
        "blank-name",
        "truncated-without-evidence",
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
                finding["related_tests_total"] = json!(0);
            }
            "duplicate-identity" => {
                let mut duplicate = finding["related_tests"][1].clone();
                duplicate["line"] = json!(90);
                finding["related_tests"]
                    .as_array_mut()
                    .ok_or("missing rows")?
                    .push(duplicate);
                finding["related_tests_total"] = json!(3);
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
            "truncated-without-evidence" => {
                finding["related_tests_total"] = json!(9);
                if let Some(evidence) = finding["evidence"].as_array_mut() {
                    evidence.retain(|entry| {
                        entry
                            .as_str()
                            .is_none_or(|value| !value.starts_with("suggested_test_line:"))
                    });
                }
            }
            _ => return Err("unknown case".to_string()),
        }
        let record = projected_record(&payload)?;
        if record != expected_record(file, name, None, SECOND_COMMAND)? {
            return Err(format!("{case} complete record mismatch: {record}"));
        }
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

const EXPECTED_MIXED_RECORD: &str = r#"{
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
  "receipt_command": "ripr receipt write --gap 'gap:python:src/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold' --verify-command 'python -m pytest tests/test_mixed.v2.py::TestGood::test_same' --status not_run --out target/ripr/receipts/gap-python-src-pricing.py-calculate_discount-predicate_boundary-predicate-amount-threshold.json",
  "regeneration_commands": [
    "ripr check --root . --json > target/ripr/reports/after-check.json"
  ],
  "repair_route": {
    "assertion_shape": "Assert the owner result or effect at `amount == threshold` first. Optional pytest shape: @pytest.mark.parametrize(\"amount, expected\", [(threshold - 1, ...), (threshold, ...), (threshold + 1, ...)]); fill expected values from domain behavior only.",
    "changed_behavior": "predicate_boundary changed at src/pricing.py:2: `if amount >= threshold:`",
    "missing_discriminator": "amount == threshold",
    "related_test": "test_same",
    "route_kind": "StrengthenExistingTest",
    "stop_conditions": [
      "Stop if imports, fixtures, or test setup cannot call the changed owner.",
      "Stop if the expected value for the missing discriminator is ambiguous.",
      "Stop if adding the test appears to require a production-code edit.",
      "Stop before adding parametrized below/above rows if their expected values are not clear; keep only the equality-boundary assertion."
    ],
    "target_file": "tests/test_mixed.v2.py",
    "target_line": 28
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
    "python -m pytest tests/test_mixed.v2.py::TestGood::test_same"
  ]
}"#;
