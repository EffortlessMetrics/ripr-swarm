use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::gate::evaluate;
use super::model::{
    ExpectedFixtureIdentity, GateInput, GateVerdict, ParityVerdict, QualificationSectionState,
    RECEIPT_KIND, RepairAttemptSemanticSnapshot,
};
use super::parse::parse_receipt;
use super::render::{render_gate_json, render_gate_markdown, render_receipt_json};

const BASE: &str = "01821f774cb2cc2bc745b7506a3941312456fb4e";
const HEAD: &str = "18a45fa5cd718f85dc60fcb27495b24dbd9715e7";
const GAP: &str = "seam-tier-boundary-equality";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn skeleton_receipt_text() -> Result<String, String> {
    let path =
        workspace_root().join("fixtures/packaged_repair_parity/expected/skeleton-receipt.json");
    fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn semantic_snapshot() -> RepairAttemptSemanticSnapshot {
    RepairAttemptSemanticSnapshot {
        canonical_gap_id: GAP.to_string(),
        allowed_edit_surface: vec!["tests/tier_boundary.rs".to_string()],
        must_not_change: vec![
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
            "target/notes.rs".to_string(),
        ],
        status_class: "awaiting_edit".to_string(),
        movement_state: "improved".to_string(),
        receipt_status: "issued".to_string(),
        verify_status: "not_run".to_string(),
        currentness: "finished_current".to_string(),
        next_action_kind: "repair_attempt_after".to_string(),
        limitations: vec![],
        non_claims: vec![],
    }
}

fn expected_fixture_identity() -> ExpectedFixtureIdentity {
    ExpectedFixtureIdentity {
        base_commit: Some(BASE.to_string()),
        head_commit: Some(HEAD.to_string()),
        canonical_gap_id: Some(GAP.to_string()),
        allowed_edit_surface: Some(vec!["tests/tier_boundary.rs".to_string()]),
        must_not_change: Some(vec![
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
            "target/notes.rs".to_string(),
        ]),
    }
}

fn evaluate_skeleton(require_full: bool) -> Result<super::model::PackagedRepairParityGate, String> {
    let receipt = parse_receipt(&skeleton_receipt_text()?)?;
    Ok(evaluate(&GateInput {
        receipt: &receipt,
        expected_fixture: &expected_fixture_identity(),
        require_full_qualification: require_full,
    }))
}

#[test]
fn packaged_repair_receipt_skeleton_parses_and_round_trips() -> Result<(), String> {
    let receipt = parse_receipt(&skeleton_receipt_text()?)?;
    let rendered = render_receipt_json(&receipt)?;
    let again = parse_receipt(&rendered)?;
    assert_eq!(receipt, again);
    assert_eq!(receipt.kind, RECEIPT_KIND);
    Ok(())
}

#[test]
fn packaged_repair_receipt_contract_gate_passes_without_full_qualification() -> Result<(), String> {
    let gate = evaluate_skeleton(false)?;
    assert_eq!(gate.verdict, GateVerdict::Passed);
    assert_eq!(
        gate.aggregate_section_state,
        QualificationSectionState::MissingEvidence
    );
    assert_eq!(gate.parity_verdict, ParityVerdict::MissingEvidence);
    Ok(())
}

#[test]
fn packaged_repair_receipt_json_and_markdown_share_gate_dto() -> Result<(), String> {
    let gate = evaluate_skeleton(false)?;
    let json = render_gate_json(&gate)?;
    let markdown = render_gate_markdown(&gate);
    for needle in [BASE, HEAD, GAP, "missing_evidence", "passed"] {
        if !json.contains(needle) {
            return Err(format!("JSON missing {needle}: {json}"));
        }
        if !markdown.contains(needle) {
            return Err(format!("Markdown missing {needle}: {markdown}"));
        }
    }
    let parsed: super::model::PackagedRepairParityGate = serde_json::from_str(&json)
        .map_err(|error| format!("gate JSON must parse as the same DTO: {error}"))?;
    assert_eq!(parsed, gate);
    Ok(())
}

#[test]
fn packaged_repair_receipt_qualified_without_steps_fails_closed() -> Result<(), String> {
    let mut value: Value =
        serde_json::from_str(&skeleton_receipt_text()?).map_err(|error| error.to_string())?;
    value["channels"]["installed_cli"]["section_state"] = json!("qualified");
    value["required_sections"][0]["state"] = json!("qualified");
    let receipt = parse_receipt(&value.to_string())?;
    let gate = evaluate(&GateInput {
        receipt: &receipt,
        expected_fixture: &ExpectedFixtureIdentity::default(),
        require_full_qualification: false,
    });
    assert_eq!(gate.verdict, GateVerdict::Failed);
    assert!(
        gate.failures
            .iter()
            .any(|failure| failure.contains("executed channel steps"))
    );
    Ok(())
}

#[test]
fn packaged_repair_receipt_semantic_mismatch_fails_parity() -> Result<(), String> {
    let mut value: Value =
        serde_json::from_str(&skeleton_receipt_text()?).map_err(|error| error.to_string())?;
    let cli = semantic_snapshot();
    let mut vsix = semantic_snapshot();
    vsix.movement_state = "unchanged".to_string();
    value["channels"]["installed_cli"]["semantic"] =
        serde_json::to_value(&cli).map_err(|e| e.to_string())?;
    value["channels"]["packaged_vsix"]["semantic"] =
        serde_json::to_value(&vsix).map_err(|e| e.to_string())?;
    value["channels"]["installed_cli"]["section_state"] = json!("qualified");
    value["channels"]["packaged_vsix"]["section_state"] = json!("qualified");
    value["channels"]["installed_cli"]["executed_steps"] = json!(["journey"]);
    value["channels"]["packaged_vsix"]["executed_steps"] = json!(["journey"]);
    value["parity"]["verdict"] = json!("qualified");
    let receipt = parse_receipt(&value.to_string())?;
    let gate = evaluate(&GateInput {
        receipt: &receipt,
        expected_fixture: &ExpectedFixtureIdentity::default(),
        require_full_qualification: false,
    });
    assert_eq!(gate.verdict, GateVerdict::Failed);
    assert!(
        gate.failures
            .iter()
            .any(|failure| failure.contains("movement_state"))
    );
    Ok(())
}

#[test]
fn packaged_repair_receipt_full_qualification_requires_qualified_sections() -> Result<(), String> {
    let gate = evaluate_skeleton(true)?;
    assert_eq!(gate.verdict, GateVerdict::Failed);
    assert!(
        gate.failures
            .iter()
            .any(|failure| failure.contains("aggregate section state"))
    );
    Ok(())
}

#[test]
fn packaged_repair_receipt_unknown_field_rejected() -> Result<(), String> {
    let mut value: Value =
        serde_json::from_str(&skeleton_receipt_text()?).map_err(|error| error.to_string())?;
    value
        .as_object_mut()
        .ok_or_else(|| "object".to_string())?
        .insert("token".to_string(), json!("secret"));
    match parse_receipt(&value.to_string()) {
        Ok(_) => Err("unknown field must fail closed".to_string()),
        Err(error) if error.contains("typed contract") => Ok(()),
        Err(error) => Err(format!("unexpected error: {error}")),
    }
}

#[test]
fn packaged_repair_receipt_stale_fixture_head_fails() -> Result<(), String> {
    let receipt = parse_receipt(&skeleton_receipt_text()?)?;
    let gate = evaluate(&GateInput {
        receipt: &receipt,
        expected_fixture: &ExpectedFixtureIdentity {
            base_commit: Some(BASE.to_string()),
            head_commit: Some("ffffffffffffffffffffffffffffffffffffffff".to_string()),
            canonical_gap_id: Some(GAP.to_string()),
            allowed_edit_surface: None,
            must_not_change: None,
        },
        require_full_qualification: false,
    });
    assert_eq!(gate.verdict, GateVerdict::Failed);
    Ok(())
}

#[test]
fn packaged_repair_receipt_missing_required_section_fails() -> Result<(), String> {
    let mut value: Value =
        serde_json::from_str(&skeleton_receipt_text()?).map_err(|error| error.to_string())?;
    let sections = value
        .get_mut("required_sections")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "sections".to_string())?;
    sections.pop();
    let receipt = parse_receipt(&value.to_string())?;
    let gate = evaluate(&GateInput {
        receipt: &receipt,
        expected_fixture: &expected_fixture_identity(),
        require_full_qualification: false,
    });
    assert_eq!(gate.verdict, GateVerdict::Failed);
    assert!(
        gate.failures
            .iter()
            .any(|failure| failure.contains("missing required section"))
    );
    Ok(())
}

#[test]
fn packaged_repair_receipt_wrong_semantic_gap_fails_against_fixture() -> Result<(), String> {
    let mut value: Value =
        serde_json::from_str(&skeleton_receipt_text()?).map_err(|error| error.to_string())?;
    let mut semantic = semantic_snapshot();
    semantic.canonical_gap_id = "other-gap".to_string();
    value["channels"]["installed_cli"]["semantic"] =
        serde_json::to_value(&semantic).map_err(|error| error.to_string())?;
    let receipt = parse_receipt(&value.to_string())?;
    let gate = evaluate(&GateInput {
        receipt: &receipt,
        expected_fixture: &expected_fixture_identity(),
        require_full_qualification: false,
    });
    assert_eq!(gate.verdict, GateVerdict::Failed);
    Ok(())
}
