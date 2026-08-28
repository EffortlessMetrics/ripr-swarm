use super::super::{ActionCommandSpecs, FirstUsefulActionReport, ParsedSources, gap_records};
use crate::domain::CommandSpec;
use crate::output::gap_decision_ledger::GapRecord;
use serde_json::Value;

/// Preserve producer-owned typed command authority when first-action selects a
/// gap record.
///
/// The report's display command remains compatibility text for humans. Machine
/// authority is projected only from the selected record's validated
/// `command_specs.verify` collection, and only when exactly one typed route
/// matches the display route already selected by `gap_record_report`.
pub(super) fn with_gap_verify_command_spec(
    mut report: FirstUsefulActionReport,
    parsed: &ParsedSources,
) -> FirstUsefulActionReport {
    let Some(gap_id) = report
        .selected
        .as_ref()
        .and_then(|selected| selected.gap_id.as_deref())
        .map(str::to_string)
    else {
        return report;
    };
    let Some(display) = report.commands.verify.clone() else {
        return report;
    };

    match producer_gap_verify_spec(parsed.gap_ledger.as_ref(), &gap_id, &display) {
        Ok(Some(spec)) => match report.commands.command_specs.as_mut() {
            Some(command_specs) => command_specs.verify = Some(spec),
            None => {
                report.commands.command_specs = Some(ActionCommandSpecs {
                    verify: Some(spec),
                    receipt: None,
                });
            }
        },
        Ok(None) => {}
        Err(reason) => report.warnings.push(format!(
            "typed gap verification route unavailable for {gap_id}: {reason}"
        )),
    }

    report
}

/// Resolve the one producer-owned verify route corresponding to the selected
/// compatibility display command.
///
/// Deserializing the selected record through `GapRecord` reuses the producer
/// boundary's `CommandSpec::validate` and role checks. Legacy string-only
/// records remain readable, but they do not gain synthesized machine authority.
fn producer_gap_verify_spec(
    gap_ledger: Option<&Value>,
    gap_id: &str,
    display: &str,
) -> Result<Option<CommandSpec>, String> {
    let gap_ledger =
        gap_ledger.ok_or_else(|| "selected gap report has no gap ledger input".to_string())?;
    let mut matching_records = gap_records(gap_ledger)
        .into_iter()
        .filter(|record| record.get("gap_id").and_then(Value::as_str) == Some(gap_id));
    let record_value = matching_records
        .next()
        .ok_or_else(|| format!("selected gap record {gap_id} is missing from the gap ledger"))?;
    if matching_records.next().is_some() {
        return Err(format!(
            "selected gap id {gap_id} is ambiguous in the gap ledger"
        ));
    }

    let record: GapRecord = serde_json::from_value(record_value.clone())
        .map_err(|error| format!("selected gap record {gap_id} is invalid: {error}"))?;
    let Some(command_specs) = record.command_specs else {
        return Ok(None);
    };
    if command_specs.verify.is_empty() {
        return Ok(None);
    }

    let mut matching_specs = command_specs
        .verify
        .into_iter()
        .filter(|spec| spec.display.as_str() == display);
    let spec = matching_specs.next().ok_or_else(|| {
        "producer-owned verify specs do not match the selected display route".to_string()
    })?;
    if matching_specs.next().is_some() {
        return Err(
            "multiple producer-owned verify specs match the selected display route".to_string(),
        );
    }

    Ok(Some(spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::command_specs::{agent_receipt_command_spec, agent_verify_command_spec};
    use crate::output::first_useful_action::{
        FirstUsefulActionInput, build_first_useful_action_report,
    };
    use serde_json::json;

    fn verify_spec() -> CommandSpec {
        agent_verify_command_spec(
            ".",
            "target/ripr/workflow/before.json",
            "target/ripr/workflow/after.json",
            None,
        )
    }

    fn actionable_gap_record(display: &str, verify_specs: Option<Vec<CommandSpec>>) -> Value {
        let mut record = json!({
            "gap_id": "gap-1",
            "kind": "MissingBoundaryAssertion",
            "language": "rust",
            "language_status": "stable",
            "scope": "pr_local",
            "evidence_class": "predicate_boundary",
            "gap_state": "actionable",
            "policy_state": "new",
            "repairability": "repairable",
            "repair_route": {
                "route_kind": "AddBoundaryAssertion",
                "target_file": "tests/pricing.rs",
                "assertion_shape": "assert_eq!(discount(100), 90)"
            },
            "verification_commands": [display]
        });
        if let (Some(specs), Value::Object(fields)) = (verify_specs, &mut record) {
            fields.insert("command_specs".to_string(), json!({ "verify": specs }));
        }
        record
    }

    fn build_gap_report(ledger: &Value) -> FirstUsefulActionReport {
        build_first_useful_action_report(FirstUsefulActionInput {
            root: ".".to_string(),
            generated_at: "2026-08-28T00:00:00Z".to_string(),
            pr_guidance_path: None,
            assistant_proof_path: None,
            gap_ledger_path: Some("gap-ledger.json".to_string()),
            ledger_path: None,
            baseline_delta_path: None,
            receipt_path: None,
            gate_decision_path: None,
            coverage_frontier_path: None,
            editor_context_path: None,
            pr_guidance_json: None,
            assistant_proof_json: None,
            gap_ledger_json: Some(Ok(ledger.to_string())),
            ledger_json: None,
            baseline_delta_json: None,
            receipt_json: None,
            gate_decision_json: None,
            coverage_frontier_json: None,
            editor_context_json: None,
        })
    }

    fn parsed_gap_ledger(ledger: &Value) -> ParsedSources {
        ParsedSources {
            gap_ledger: Some(ledger.clone()),
            ..ParsedSources::default()
        }
    }

    #[test]
    fn gap_first_action_preserves_the_producer_owned_typed_verify_route() {
        let spec = verify_spec();
        let display = spec.display.clone();
        let ledger = json!({
            "records": [actionable_gap_record(&display, Some(vec![spec.clone()]))]
        });
        let report = build_gap_report(&ledger);

        assert_eq!(report.status, "actionable");
        assert_eq!(report.commands.verify.as_deref(), Some(display.as_str()));
        assert_eq!(
            report
                .commands
                .command_specs
                .as_ref()
                .and_then(|specs| specs.verify.as_ref()),
            Some(&spec)
        );
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn gap_projection_leaves_reports_without_selected_authority_unchanged() {
        let spec = verify_spec();
        let ledger = json!({
            "records": [actionable_gap_record(&spec.display, Some(vec![spec]))]
        });
        let parsed = parsed_gap_ledger(&ledger);
        let report = build_gap_report(&ledger);

        let mut without_selection = report.clone();
        without_selection.selected = None;
        without_selection.commands.command_specs = None;
        let expected = without_selection.clone();
        assert_eq!(
            with_gap_verify_command_spec(without_selection, &parsed),
            expected
        );

        let mut without_display = report;
        without_display.commands.verify = None;
        without_display.commands.command_specs = None;
        let expected = without_display.clone();
        assert_eq!(
            with_gap_verify_command_spec(without_display, &parsed),
            expected
        );
    }

    #[test]
    fn gap_projection_preserves_an_existing_receipt_spec() {
        let spec = verify_spec();
        let ledger = json!({
            "records": [actionable_gap_record(&spec.display, Some(vec![spec.clone()]))]
        });
        let parsed = parsed_gap_ledger(&ledger);
        let receipt =
            agent_receipt_command_spec(".", "target/ripr/workflow/verify.json", "seam-1", None);
        let mut report = build_gap_report(&ledger);
        report.commands.command_specs = Some(ActionCommandSpecs {
            verify: None,
            receipt: Some(receipt.clone()),
        });

        let projected = with_gap_verify_command_spec(report, &parsed);

        assert_eq!(
            projected.commands.command_specs,
            Some(ActionCommandSpecs {
                verify: Some(spec),
                receipt: Some(receipt),
            })
        );
    }

    #[test]
    fn gap_verify_spec_reuses_the_supported_root_array_shape() {
        let spec = verify_spec();
        let display = spec.display.clone();
        let ledger = json!([{
            "gap_id": "gap-1",
            "verification_commands": [display.clone()],
            "command_specs": { "verify": [spec.clone()] }
        }]);

        assert_eq!(
            producer_gap_verify_spec(Some(&ledger), "gap-1", &display),
            Ok(Some(spec))
        );
    }

    #[test]
    fn gap_first_action_does_not_synthesize_authority_for_legacy_display_text() {
        let spec = verify_spec();
        let ledger = json!({
            "records": [actionable_gap_record(&spec.display, None)]
        });
        let report = build_gap_report(&ledger);

        assert_eq!(report.commands.verify.as_deref(), Some(spec.display.as_str()));
        assert!(report.commands.command_specs.is_none());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn gap_first_action_warns_and_withholds_a_drifted_typed_route() {
        let displayed = verify_spec();
        let typed = agent_verify_command_spec(
            ".",
            "target/ripr/workflow/other-before.json",
            "target/ripr/workflow/after.json",
            None,
        );
        let ledger = json!({
            "records": [actionable_gap_record(
                &displayed.display,
                Some(vec![typed]),
            )]
        });
        let report = build_gap_report(&ledger);

        assert_eq!(
            report.commands.verify.as_deref(),
            Some(displayed.display.as_str())
        );
        assert!(report.commands.command_specs.is_none());
        assert!(report.warnings.iter().any(|warning| {
            warning.contains("producer-owned verify specs do not match the selected display route")
        }));
    }

    #[test]
    fn gap_verify_spec_rejects_an_ambiguous_selected_gap_id() {
        let spec = verify_spec();
        let record = actionable_gap_record(&spec.display, Some(vec![spec.clone()]));
        let ledger = json!({ "records": [record.clone(), record] });
        let result = producer_gap_verify_spec(Some(&ledger), "gap-1", &spec.display);

        assert!(matches!(
            result,
            Err(reason) if reason.contains("selected gap id gap-1 is ambiguous")
        ));
    }

    #[test]
    fn gap_verify_spec_treats_an_empty_typed_collection_as_legacy_only() {
        let spec = verify_spec();
        let ledger = json!({
            "records": [actionable_gap_record(&spec.display, Some(Vec::new()))]
        });

        assert_eq!(
            producer_gap_verify_spec(Some(&ledger), "gap-1", &spec.display),
            Ok(None)
        );
    }

    #[test]
    fn gap_verify_spec_rejects_multiple_matching_typed_routes() {
        let spec = verify_spec();
        let mut alternate = spec.clone();
        alternate.timeout_ms += 1;
        let ledger = json!({
            "records": [actionable_gap_record(
                &spec.display,
                Some(vec![spec.clone(), alternate]),
            )]
        });
        let result = producer_gap_verify_spec(Some(&ledger), "gap-1", &spec.display);

        assert!(matches!(
            result,
            Err(reason) if reason.contains("multiple producer-owned verify specs")
        ));
    }

    #[test]
    fn gap_verify_spec_reuses_role_validation_from_the_gap_record_boundary() {
        let displayed = verify_spec();
        let receipt =
            agent_receipt_command_spec(".", "target/ripr/workflow/verify.json", "seam-1", None);
        let ledger = json!({
            "records": [{
                "gap_id": "gap-1",
                "verification_commands": [displayed.display.clone()],
                "command_specs": { "verify": [receipt] }
            }]
        });
        let result = producer_gap_verify_spec(Some(&ledger), "gap-1", &displayed.display);

        assert!(matches!(
            result,
            Err(reason) if reason.contains("does not match verify collection")
        ));
    }
}
