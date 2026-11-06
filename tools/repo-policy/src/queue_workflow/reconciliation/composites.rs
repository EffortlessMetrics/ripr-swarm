//! Necessary structural composite checks, never authenticated execution proof.
use super::{ObservedRow, Subject};
use crate::product_gate_plan::composites::{
    BASE_RECOVERY, CompositeObservation, CompositeResult, CompositeSubject, Event,
};

pub(super) fn bounded(observation: &CompositeObservation) -> bool {
    observation.completed_steps.len() <= 16
        && observation.commands.len() <= 16
        && observation
            .commands
            .iter()
            .all(|command| command.len() <= 4096)
        && observation.base_sha.len() <= 40
        && observation.target_sha.len() <= 40
        && observation
            .matcher_sha
            .as_ref()
            .is_none_or(|sha| sha.len() <= 40)
        && observation.artifacts.len() <= 16
        && observation
            .artifacts
            .iter()
            .all(|binding| binding.subject_sha.len() <= 40)
}

pub(super) fn inspect(subject: &Subject, row: &ObservedRow, findings: &mut Vec<String>) {
    let id = &row.row.id;
    let Some(producer) = row.row.composite else {
        if row.composite.is_some() {
            findings.push(format!("unexpected composite observation: {id}"));
        }
        return;
    };
    let Some(observation) = &row.composite else {
        findings.push(format!("missing composite observation: {id}"));
        return;
    };
    let contract = producer.contract(subject.event);
    if contract.pr_events_only && !subject.event.has_pr_evidence() {
        findings.push(format!("composite event guard excludes execution: {id}"));
    }
    if contract.result_owner() != Some(observation.owner)
        || observation.completed_steps != contract.steps
    {
        findings.push(format!("composite owning step or order differs: {id}"));
    }
    let comparison_sha = if subject.event == Event::MergeGroup {
        &subject.group_sha
    } else {
        &subject.head_sha
    };
    let target_sha = if contract.subject == CompositeSubject::AdmittedComparison {
        comparison_sha
    } else {
        &subject.group_sha
    };
    if observation.base_sha != subject.base_sha || &observation.target_sha != target_sha {
        findings.push(format!("composite subject differs: {id}"));
    }
    let pin_only = observation.result == CompositeResult::MatcherPinOnly;
    if contract.subject == CompositeSubject::FirstParentMatcherAgainstCheckout {
        if observation.matcher_sha.as_ref() != Some(&subject.base_sha)
            || observation.supports_cross_tree != Some(!pin_only)
            || observation
                .recovery
                .is_some_and(|recovery| recovery != BASE_RECOVERY)
        {
            findings.push(format!(
                "base-owned matcher or exact recovery differs: {id}"
            ));
        }
        if pin_only {
            findings.push(format!(
                "cross-tree execution not established (matcher pin only): {id}"
            ));
        }
    } else if pin_only
        || observation.matcher_sha.is_some()
        || observation.supports_cross_tree.is_some()
        || observation.recovery.is_some()
    {
        findings.push(format!("wrong composite outcome or matcher context: {id}"));
    }
    let commands: Vec<_> = observation.commands.iter().map(String::as_str).collect();
    let commands_match = if pin_only {
        commands.is_empty()
    } else {
        commands == contract.commands
    };
    if !commands_match {
        findings.push(format!(
            "composite command order or execution differs: {id}"
        ));
    }
    if observation
        .artifacts
        .iter()
        .map(|binding| binding.edge)
        .collect::<Vec<_>>()
        != contract.artifacts
    {
        findings.push(format!("composite artifact edges differ: {id}"));
    }
    for binding in &observation.artifacts {
        let artifact_subject =
            if binding.edge.producer == crate::product_gate_plan::ProductGateId::PrEvidence {
                comparison_sha
            } else {
                &subject.group_sha
            };
        if binding.run_id != subject.run_id
            || binding.run_attempt != subject.run_attempt
            || &binding.subject_sha != artifact_subject
        {
            findings.push(format!("missing current-run artifact binding: {id}"));
        }
    }
}
