use super::{
    ActionCommandSpecs, ActionInputs, FirstUsefulActionInput, FirstUsefulActionReport,
    ParsedSources, acknowledged_report, actionable_report, baseline_only_report,
    gap_record_report, missing_assistant_proof_report, no_actionable_report, read_error_report,
    receipt_report, stale_report, suppressed_report, waived_report,
};
use crate::domain::CommandSpec;
use crate::output::gap_decision_ledger::GapRecord;
use serde_json::Value;

pub(super) fn select_report(
    input: &FirstUsefulActionInput,
    parsed: &ParsedSources,
    inputs: &ActionInputs,
    generated_at: &str,
) -> FirstUsefulActionReport {
    if let Some(report) = stale_report(input, parsed, inputs, generated_at) {
        report
    } else if let Some(report) = read_error_report(input, parsed, inputs, generated_at) {
        report
    } else if let Some(report) = receipt_report(input, parsed, inputs, generated_at) {
        report
    } else if let Some(report) = suppressed_report(input, parsed, inputs, generated_at) {
        report
    } else if let Some(report) = acknowledged_report(input, parsed, inputs, generated_at) {
        report
    } else if let Some(report) = waived_report(input, parsed, inputs, generated_at) {
        report
    } else if let Some(report) = gap_record_report(input, parsed, inputs, generated_at) {
        with_gap_verify_command_spec(report, parsed)
    } else if let Some(report) = missing_assistant_proof_report(input, parsed, inputs, generated_at)
    {
        report
    } else if let Some(report) = actionable_report(input, parsed, inputs, generated_at) {
        report
    } else if let Some(report) = baseline_only_report(input, parsed, inputs, generated_at) {
        report
    } else {
        no_actionable_report(input, parsed, inputs, generated_at)
    }
}

/// Preserve producer-owned typed command authority when first-action selects a
/// gap record.
///
/// The report's display command remains compatibility text for humans. Machine
/// authority is projected only from the selected record's validated
/// `command_specs.verify` collection, and only when exactly one typed route
/// matches the display route already selected by `gap_record_report`.
fn with_gap_verify_command_spec(
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
    let gap_ledger = gap_ledger
        .ok_or_else(|| "selected gap report has no gap ledger input".to_string())?;
    let records = gap_ledger
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| "gap ledger is missing records[]".to_string())?;
    let mut matching_records = records.iter().filter(|record| {
        record.get("gap_id").and_then(Value::as_str) == Some(gap_id)
    });
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
    use crate::agent::command_specs::{
        agent_receipt_command_spec, agent_verify_command_spec,
    };
    use serde_json::json;

    #[test]
    fn gap_verify_spec_preserves_the_producer_owned_route() {
        let spec = agent_verify_command_spec(
            ".",
            "target/ripr/workflow/before.json",
            "target/ripr/workflow/after.json",
            None,
        );
        let display = spec.display.clone();
        let ledger = json!({
            "records": [{
                "gap_id": "gap-1",
                "verification_commands": [display.clone()],
                "command_specs": { "verify": [spec.clone()] }
            }]
        });

        assert_eq!(
            producer_gap_verify_spec(Some(&ledger), "gap-1", &display),
            Ok(Some(spec))
        );
    }

    #[test]
    fn gap_verify_spec_does_not_synthesize_authority_for_legacy_display_text() {
        let display = "ripr agent verify --root . --before before.json --after after.json --json";
        let ledger = json!({
            "records": [{
                "gap_id": "gap-1",
                "verification_commands": [display]
            }]
        });

        assert_eq!(
            producer_gap_verify_spec(Some(&ledger), "gap-1", display),
            Ok(None)
        );
    }

    #[test]
    fn gap_verify_spec_rejects_typed_and_display_route_drift() {
        let displayed = agent_verify_command_spec(
            ".",
            "target/ripr/workflow/before.json",
            "target/ripr/workflow/after.json",
            None,
        );
        let typed = agent_verify_command_spec(
            ".",
            "target/ripr/workflow/other-before.json",
            "target/ripr/workflow/after.json",
            None,
        );
        let ledger = json!({
            "records": [{
                "gap_id": "gap-1",
                "verification_commands": [displayed.display.clone()],
                "command_specs": { "verify": [typed] }
            }]
        });
        let result = producer_gap_verify_spec(Some(&ledger), "gap-1", &displayed.display);

        assert!(matches!(
            result,
            Err(reason) if reason.contains("do not match the selected display route")
        ));
    }

    #[test]
    fn gap_verify_spec_reuses_role_validation_from_the_gap_record_boundary() {
        let displayed = agent_verify_command_spec(
            ".",
            "target/ripr/workflow/before.json",
            "target/ripr/workflow/after.json",
            None,
        );
        let receipt = agent_receipt_command_spec(
            ".",
            "target/ripr/workflow/verify.json",
            "seam-1",
            None,
        );
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
