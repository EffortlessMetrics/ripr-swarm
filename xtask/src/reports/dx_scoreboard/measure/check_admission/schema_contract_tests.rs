//! Structural schema conformance and DX sample admission are separate contracts.
//! These are document examples, not claims about CLI-produced outcome states.
use super::{
    CheckAdmission, CheckSubject, FINDINGS_BOUND_STATUS, admit_check_document,
    permissive_json_exit_zero,
};
use crate::verification_contracts::validate_value_against_schema;
use serde_json::{Value, json};
use std::path::Path;

fn read_contract(path: &str) -> Result<Value, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path);
    let bytes =
        std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn complete_document() -> Result<Value, String> {
    read_contract("tests/fixtures/verification/ripr/check-complete.valid.json")
}

fn subject() -> CheckSubject {
    CheckSubject {
        root: "fixtures/weak_error_oracle/input".to_string(),
        mode: "fast".to_string(),
        base: "origin/main".to_string(),
        head: None,
    }
}

fn assert_structural(document: &Value) -> Result<(), String> {
    let schema = read_contract("schemas/ripr/check.schema.json")?;
    let mut violations = Vec::new();
    validate_value_against_schema(
        document,
        &schema,
        &schema,
        "check admission example".to_string(),
        &mut violations,
    );
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!("structural example rejected: {violations:?}"))
    }
}

fn mark_rendering_prefix(document: &mut Value) {
    document["run_limitations"] = json!([{
        "category": FINDINGS_BOUND_STATUS,
        "run_status": FINDINGS_BOUND_STATUS,
        "basis": "check_findings_byte_budget",
        "downstream_consumable": false,
        "message": "The contract example renders one of two findings.",
        "repair_route": "output/check-findings-budget"
    }]);
}

#[test]
fn schema_positive_total_mismatch_requires_scoreboard_admission() -> Result<(), String> {
    let mut valid = complete_document()?;
    let rows = valid["findings"]
        .as_array_mut()
        .ok_or("complete fixture findings must be an array")?;
    assert_eq!(rows.len(), 2, "intended complete fixture must be nonempty");
    rows.truncate(1);
    valid["summary"]["findings"] = json!(1);
    valid["summary"]["weakly_exposed"] = json!(1);
    valid["analysis_outcome"]["outcome"]["counts"]["finding_count"] = json!(1);
    assert_structural(&valid)?;
    assert!(matches!(
        admit_check_document(&valid, &subject()),
        CheckAdmission::Complete {
            findings: 1,
            rendering_truncated: false,
            ..
        }
    ));

    // Literal counterexample: positive 1 -> 2 remains structurally valid.
    let mut mismatch = valid.clone();
    mismatch["summary"]["findings"] = json!(2);
    assert_eq!(
        mismatch["analysis_outcome"]["outcome"]["counts"]["finding_count"],
        1
    );
    for rendering_prefix in [false, true] {
        let mut example = mismatch.clone();
        if rendering_prefix {
            mark_rendering_prefix(&mut example);
        }
        assert_structural(&example)?;
        let stdout = serde_json::to_string(&example).map_err(|error| error.to_string())?;
        assert!(
            permissive_json_exit_zero(false, true, &stdout),
            "the earlier exit-zero JSON predicate accepts this counterexample"
        );
        assert_eq!(
            admit_check_document(&example, &subject()),
            CheckAdmission::Rejected {
                reason: "declared finding counts disagree".to_string()
            },
            "the rendering-prefix exception must not admit disagreement"
        );
    }
    Ok(())
}

#[test]
fn schema_prefix_and_legacy_documents_keep_distinct_admission() -> Result<(), String> {
    let complete = complete_document()?;
    assert_structural(&complete)?;
    assert!(matches!(
        admit_check_document(&complete, &subject()),
        CheckAdmission::Complete {
            findings: 2,
            rendering_truncated: false,
            ..
        }
    ));
    let mut prefix = complete.clone();
    prefix["findings"]
        .as_array_mut()
        .ok_or("complete fixture findings must be an array")?
        .truncate(1);
    mark_rendering_prefix(&mut prefix);
    assert_structural(&prefix)?;
    assert!(matches!(
        admit_check_document(&prefix, &subject()),
        CheckAdmission::Complete {
            findings: 2,
            rendering_truncated: true,
            ..
        }
    ));

    let mut unmarked = prefix;
    unmarked
        .as_object_mut()
        .ok_or("prefix must be an object")?
        .remove("run_limitations");
    assert_structural(&unmarked)?;
    assert_eq!(
        admit_check_document(&unmarked, &subject()),
        CheckAdmission::Rejected {
            reason: "findings count does not match the findings array".to_string()
        }
    );

    let mut legacy = complete;
    legacy
        .as_object_mut()
        .ok_or("legacy example must be an object")?
        .remove("analysis_outcome");
    assert_structural(&legacy)?;
    assert_eq!(
        admit_check_document(&legacy, &subject()),
        CheckAdmission::Rejected {
            reason: "missing analysis_outcome".to_string()
        },
        "legacy schema compatibility does not establish complete DX work"
    );
    Ok(())
}

#[test]
fn schema_validity_does_not_override_scoreboard_subject() -> Result<(), String> {
    let document = complete_document()?;
    assert_structural(&document)?;
    let expected = subject();
    assert!(admit_check_document(&document, &expected).is_complete());
    let mut different = expected;
    different.root = "fixtures/different-subject/input".to_string();
    assert!(matches!(
        admit_check_document(&document, &different),
        CheckAdmission::Rejected { reason } if reason.contains("producer root")
    ));
    Ok(())
}
