//! RIPR-SPEC-0235 rule 22: method spacing must preserve owner identity (#13150).

use super::boundary_input::{BoundaryInput, boundary_input_for};
use super::tests::{EXACT_RETURN_PACKET, edited_packet, only_finding, packet_array, set_first};
use crate::domain::{ExposureClass, Finding};

const OWNER: &str = "perl:lib/My/App.pm::My::App::discount";
const COMPARISON: &str = "$amount >= 100";
const METHOD_SEPARATORS: [&str; 5] = ["", " ", "\t", "\n", " # receiver stays attached\n"];

#[test]
fn threshold_method_spacing_preserves_wrong_and_unknown_receivers() {
    for receiver in ["Other", "Other::App", "$obj", "$My::App"] {
        for separator in METHOD_SEPARATORS {
            let expression = format!("is({receiver} ->{separator}discount(100), 10)");
            assert_eq!(
                boundary_input_for(COMPARISON, OWNER, [expression.as_str()]),
                BoundaryInput::Unestablished,
                "a different or unknown receiver supplies no owner input: {expression}"
            );
        }
    }
}

#[test]
fn threshold_method_spacing_preserves_the_owner_positive_control() {
    for separator in METHOD_SEPARATORS {
        let expression = format!("is(My::App ->{separator}discount(100), 10)");
        assert_eq!(
            boundary_input_for(COMPARISON, OWNER, [expression.as_str()]),
            BoundaryInput::OnThreshold,
            "spacing does not hide a known owner receiver: {expression}"
        );
    }
    for expression in ["is(discount(100), 10)", "is(My::App::discount(100), 10)"] {
        assert_eq!(
            boundary_input_for(COMPARISON, OWNER, [expression]),
            BoundaryInput::OnThreshold,
            "existing free-call spellings retain their input: {expression}"
        );
    }
}

/// The aligned oracle calls the changed owner away from the threshold. A
/// second exact oracle in the same test is deliberately not linked to an
/// owner, so the production lexical reader must decide whose input it is.
fn finding_with_secondary_oracle(expression: &str) -> Result<Finding, String> {
    let packet = edited_packet(EXACT_RETURN_PACKET, |packet| {
        set_first(
            packet,
            "changes",
            "changed_observable",
            serde_json::json!(COMPARISON),
        )?;
        set_first(
            packet,
            "oracles",
            "observed_sink",
            serde_json::json!(COMPARISON),
        )?;
        set_first(
            packet,
            "oracles",
            "expression",
            serde_json::json!("is(My::App::discount(5), 10, 'far from threshold')"),
        )?;
        let oracles = packet_array(packet, "oracles")?;
        let mut secondary = oracles
            .first()
            .ok_or_else(|| "expected the aligned owner oracle".to_string())?
            .clone();
        secondary["oracle_id"] = serde_json::json!("oracle:t/app.t:9:is");
        secondary["expression"] = serde_json::json!(expression);
        secondary["target_owner_id"] = serde_json::Value::Null;
        secondary["observed_sink"] = serde_json::Value::Null;
        secondary["range"] = serde_json::json!({
            "start_line": 9, "start_column": 1, "end_line": 9, "end_column": 40
        });
        oracles.push(secondary);
        assert_eq!(oracles.len(), 2, "both oracles must reach the mapper");
        Ok(())
    })?;
    only_finding(&packet)
}

#[test]
fn unrelated_spaced_method_oracle_cannot_supply_the_owner_threshold() -> Result<(), String> {
    for receiver in ["Other", "$obj"] {
        for separator in METHOD_SEPARATORS {
            let expression = format!("is({receiver} ->{separator}discount(100), 10)");
            let finding = finding_with_secondary_oracle(&expression)?;
            assert_eq!(finding.class, ExposureClass::StaticUnknown, "{expression}");
            assert!(finding.canonical_gap.is_none(), "{expression}");
            assert!(
                !finding
                    .evidence
                    .iter()
                    .any(|evidence| evidence.starts_with("perl_already_discriminated:")),
                "an unrelated threshold must not become observed-owner evidence: {expression}"
            );
        }
    }
    Ok(())
}

#[test]
fn same_owner_spaced_method_oracle_still_supplies_the_threshold() -> Result<(), String> {
    for separator in METHOD_SEPARATORS {
        let expression = format!("is(My::App ->{separator}discount(100), 10)");
        let finding = finding_with_secondary_oracle(&expression)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{expression}");
        assert!(
            finding
                .evidence
                .iter()
                .any(|evidence| evidence.starts_with("perl_already_discriminated:")),
            "a known owner threshold retains observed-owner evidence: {expression}"
        );
    }
    Ok(())
}

#[test]
fn spaced_receiver_honesty_projection_matches_production() -> Result<(), String> {
    let finding = finding_with_secondary_oracle("is(Other-> discount(100), 10)")?;
    let actual = serde_json::json!({
        "kind": "perl_threshold_receiver_expected_test_projection",
        "summary": { "findings": 1 },
        "findings": [{
            "classification": finding.class,
            "probe": { "family": finding.probe.family }
        }]
    });
    let expected: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/evidence-promotion-honesty-corpus/reports/",
        "perl-threshold-spaced-receiver-expected-projection.json"
    )))
    .map_err(|error| format!("parse expected threshold-receiver projection: {error}"))?;
    assert_eq!(
        actual, expected,
        "the corpus projection must match the mapper"
    );
    Ok(())
}
