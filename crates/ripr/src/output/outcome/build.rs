use super::parse::oracle_strength_rank;
use super::*;

pub(super) fn build_targeted_test_outcome_report(
    before: &[StaticSeamRecord],
    after: &[StaticSeamRecord],
    before_path: String,
    after_path: String,
) -> Result<TargetedTestOutcomeReport, String> {
    let before_by_id = targeted_outcome_seams_by_id(before, "before")?;
    let after_by_id = targeted_outcome_seams_by_id(after, "after")?;
    let mut moved = Vec::new();
    let mut unchanged = Vec::new();
    let mut regressed = Vec::new();
    let mut removed = Vec::new();

    for (seam_id, before_seam) in &before_by_id {
        match after_by_id.get(seam_id) {
            Some(after_seam) => {
                let movement = targeted_test_outcome_movement(before_seam, after_seam);
                if movement.before == movement.after {
                    unchanged.push(movement);
                } else if targeted_outcome_grip_rank(&movement.after)
                    < targeted_outcome_grip_rank(&movement.before)
                {
                    regressed.push(movement);
                } else {
                    moved.push(movement);
                }
            }
            None => removed.push(targeted_test_outcome_seam(before_seam)),
        }
    }

    let mut new = Vec::new();
    for (seam_id, after_seam) in &after_by_id {
        if !before_by_id.contains_key(seam_id) {
            new.push(targeted_test_outcome_seam(after_seam));
        }
    }

    Ok(TargetedTestOutcomeReport {
        before_path,
        after_path,
        before_counts: targeted_outcome_class_counts(before),
        after_counts: targeted_outcome_class_counts(after),
        moved,
        unchanged,
        regressed,
        new,
        removed,
    })
}

fn targeted_outcome_seams_by_id(
    seams: &[StaticSeamRecord],
    label: &str,
) -> Result<BTreeMap<String, StaticSeamRecord>, String> {
    let mut out = BTreeMap::new();
    for seam in seams {
        if out.insert(seam.seam_id.clone(), seam.clone()).is_some() {
            return Err(format!(
                "{label} static snapshot JSON contains duplicate seam_id `{}`",
                seam.seam_id
            ));
        }
    }
    Ok(out)
}

fn targeted_outcome_class_counts(seams: &[StaticSeamRecord]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    counts.insert("seams_total".to_string(), seams.len());
    for class in SEAM_GRIP_CLASS_ORDER {
        counts.insert((*class).to_string(), 0);
    }
    for seam in seams {
        *counts.entry(seam.seam_grip_class.clone()).or_insert(0) += 1;
    }
    counts
}

pub(super) fn targeted_test_outcome_movement(
    before: &StaticSeamRecord,
    after: &StaticSeamRecord,
) -> TargetedTestOutcomeMovement {
    let before_rank = targeted_outcome_grip_rank(&before.seam_grip_class);
    let after_rank = targeted_outcome_grip_rank(&after.seam_grip_class);
    let direction = if before.seam_grip_class == after.seam_grip_class {
        "unchanged"
    } else if after_rank > before_rank {
        "improved"
    } else if after_rank < before_rank {
        "regressed"
    } else {
        "changed"
    };
    let gap_movement = targeted_outcome_gap_movement(
        before.seam_grip_class.as_str(),
        after.seam_grip_class.as_str(),
        direction,
    );
    let evidence_source = movement_evidence_source(before, after);
    let reach_delta = stage_delta(before, after, "reach");
    let activate_delta = stage_delta(before, after, "activate");
    let propagate_delta = stage_delta(before, after, "propagate");
    let observe_delta = stage_delta(before, after, "observe");
    let discriminate_delta = stage_delta(before, after, "discriminate");
    let observed_values_added =
        string_values_added(&before.observed_values, &after.observed_values);
    let observed_values_removed =
        string_values_removed(&before.observed_values, &after.observed_values);
    let missing_discriminators_resolved = string_values_removed(
        &before.missing_discriminators,
        &after.missing_discriminators,
    );
    let missing_discriminators_reopened = string_values_added(
        &before.missing_discriminators,
        &after.missing_discriminators,
    );
    let oracle_strength_delta = oracle_strength_delta(before, after);
    let related_test_delta = related_test_delta(before, after);
    let delta_inputs = TargetedOutcomeEvidenceDelta {
        stage_deltas: [
            &reach_delta,
            &activate_delta,
            &propagate_delta,
            &observe_delta,
            &discriminate_delta,
        ],
        observed_values_added: &observed_values_added,
        observed_values_removed: &observed_values_removed,
        missing_discriminators_resolved: &missing_discriminators_resolved,
        missing_discriminators_reopened: &missing_discriminators_reopened,
        oracle_strength_delta: oracle_strength_delta.as_deref(),
        related_test_delta,
    };
    let evidence_delta = targeted_outcome_evidence_delta(before, after, &delta_inputs);
    let no_movement_reason = no_movement_reason(direction, &evidence_delta, &evidence_source);
    TargetedTestOutcomeMovement {
        seam_id: before.seam_id.clone(),
        seam_kind: before.seam_kind.clone(),
        file: before.file.clone(),
        line: before.line,
        before: before.seam_grip_class.clone(),
        after: after.seam_grip_class.clone(),
        direction: direction.to_string(),
        gap_movement: gap_movement.to_string(),
        evidence_delta,
        evidence_source,
        reach_delta,
        activate_delta,
        propagate_delta,
        observe_delta,
        discriminate_delta,
        observed_values_added,
        observed_values_removed,
        missing_discriminators_resolved,
        missing_discriminators_reopened,
        oracle_strength_delta,
        related_test_delta,
        no_movement_reason,
    }
}

fn targeted_test_outcome_seam(seam: &StaticSeamRecord) -> TargetedTestOutcomeSeam {
    TargetedTestOutcomeSeam {
        seam_id: seam.seam_id.clone(),
        seam_kind: seam.seam_kind.clone(),
        file: seam.file.clone(),
        line: seam.line,
        grip_class: seam.seam_grip_class.clone(),
    }
}

pub(super) fn targeted_outcome_grip_rank(class: &str) -> u8 {
    match class {
        "strongly_gripped" | "intentional" | "suppressed" => 7,
        "weakly_gripped" => 5,
        "reachable_unrevealed" => 4,
        "activation_unknown"
        | "propagation_unknown"
        | "observation_unknown"
        | "discrimination_unknown" => 3,
        "opaque" => 2,
        "ungripped" => 1,
        _ => 0,
    }
}

fn targeted_outcome_gap_movement(before: &str, after: &str, direction: &str) -> &'static str {
    let before_needs_attention = review_attention_class(before);
    let after_needs_attention = review_attention_class(after);
    match (before_needs_attention, after_needs_attention, direction) {
        (true, false, _) => "closed",
        (false, true, _) => "opened",
        (true, true, "improved") => "improved",
        (true, true, "regressed") => "regressed",
        (_, _, "changed") => "changed",
        _ => "unchanged",
    }
}

fn targeted_outcome_evidence_delta(
    before: &StaticSeamRecord,
    after: &StaticSeamRecord,
    delta: &TargetedOutcomeEvidenceDelta<'_>,
) -> Vec<String> {
    let mut deltas = Vec::new();
    if before.seam_grip_class != after.seam_grip_class {
        deltas.push(format!(
            "grip class moved from {} to {}",
            before.seam_grip_class, after.seam_grip_class
        ));
    }

    for (stage, stage_delta) in EVIDENCE_STAGES.iter().zip(delta.stage_deltas.iter()) {
        if let Some(stage_delta) = stage_delta {
            deltas.push(format!(
                "{} evidence moved from {} to {}",
                stage,
                optional_delta_value(stage_delta.before_state.as_deref()),
                optional_delta_value(stage_delta.after_state.as_deref())
            ));
        }
    }

    for value in delta.missing_discriminators_resolved {
        deltas.push(format!(
            "missing discriminator no longer reported: {}",
            md_escape(value)
        ));
    }
    for value in delta.missing_discriminators_reopened {
        deltas.push(format!(
            "new missing discriminator reported: {}",
            md_escape(value)
        ));
    }

    for value in delta.observed_values_added {
        deltas.push(format!("new observed value: {}", md_escape(value)));
    }
    for value in delta.observed_values_removed {
        deltas.push(format!(
            "previous observed value absent: {}",
            md_escape(value)
        ));
    }

    if let Some(oracle_delta) = delta.oracle_strength_delta {
        if oracle_strength_rank(&after.oracle_strength)
            > oracle_strength_rank(&before.oracle_strength)
        {
            deltas.push(format!("stronger related oracle visible: {oracle_delta}"));
        } else {
            deltas.push(format!("related oracle strength decreased: {oracle_delta}"));
        }
    }
    if before.oracle_kind != after.oracle_kind && before.oracle_strength == after.oracle_strength {
        deltas.push(format!(
            "related oracle kind changed: {} -> {}",
            before.oracle_kind, after.oracle_kind
        ));
    }
    match delta.related_test_delta.cmp(&0) {
        std::cmp::Ordering::Greater => {
            deltas.push(format!(
                "related test count increased by {}",
                delta.related_test_delta
            ));
        }
        std::cmp::Ordering::Less => {
            deltas.push(format!(
                "related test count decreased by {}",
                delta.related_test_delta.abs()
            ));
        }
        std::cmp::Ordering::Equal => {}
    }

    if deltas.is_empty() && before.seam_grip_class != after.seam_grip_class {
        deltas.push("grip class changed without rendered evidence details".to_string());
    }
    deltas
}

pub(super) fn targeted_test_outcome_gap_summary(
    report: &TargetedTestOutcomeReport,
) -> TargetedTestOutcomeGapSummary {
    let mut summary = TargetedTestOutcomeGapSummary {
        new: report.new.len(),
        removed: report.removed.len(),
        ..TargetedTestOutcomeGapSummary::default()
    };
    for movement in report
        .moved
        .iter()
        .chain(report.unchanged.iter())
        .chain(report.regressed.iter())
    {
        match movement.gap_movement.as_str() {
            "closed" => summary.closed += 1,
            "opened" => summary.opened += 1,
            "improved" => summary.strengthened += 1,
            "regressed" => summary.weakened += 1,
            "changed" => summary.changed += 1,
            "unchanged" => summary.unchanged += 1,
            _ => summary.changed += 1,
        }
    }
    summary
}

fn movement_evidence_source(before: &StaticSeamRecord, after: &StaticSeamRecord) -> String {
    if before.evidence_source == after.evidence_source {
        before.evidence_source.clone()
    } else {
        format!("{} -> {}", before.evidence_source, after.evidence_source)
    }
}

fn stage_delta(
    before: &StaticSeamRecord,
    after: &StaticSeamRecord,
    stage: &str,
) -> Option<TargetedTestOutcomeStageDelta> {
    let before_stage = before.evidence_path.get(stage);
    let after_stage = after.evidence_path.get(stage);
    if before_stage == after_stage {
        return None;
    }
    if before_stage.is_none() && after_stage.is_none() {
        return None;
    }
    Some(TargetedTestOutcomeStageDelta {
        before_state: before_stage.map(|stage| stage.state.clone()),
        after_state: after_stage.map(|stage| stage.state.clone()),
        before_confidence: before_stage.map(|stage| stage.confidence.clone()),
        after_confidence: after_stage.map(|stage| stage.confidence.clone()),
        before_summary: before_stage.map(|stage| stage.summary.clone()),
        after_summary: after_stage.map(|stage| stage.summary.clone()),
    })
}

pub(super) fn stage_delta_json(delta: &TargetedTestOutcomeStageDelta) -> Value {
    serde_json::json!({
        "before_state": delta.before_state.as_deref(),
        "after_state": delta.after_state.as_deref(),
        "before_confidence": delta.before_confidence.as_deref(),
        "after_confidence": delta.after_confidence.as_deref(),
        "before_summary": delta.before_summary.as_deref(),
        "after_summary": delta.after_summary.as_deref(),
    })
}

fn string_values_added(before: &[String], after: &[String]) -> Vec<String> {
    let before_values = before.iter().collect::<BTreeSet<_>>();
    let after_values = after.iter().collect::<BTreeSet<_>>();
    after_values
        .difference(&before_values)
        .map(|value| (*value).clone())
        .collect()
}

fn string_values_removed(before: &[String], after: &[String]) -> Vec<String> {
    let before_values = before.iter().collect::<BTreeSet<_>>();
    let after_values = after.iter().collect::<BTreeSet<_>>();
    before_values
        .difference(&after_values)
        .map(|value| (*value).clone())
        .collect()
}

fn oracle_strength_delta(before: &StaticSeamRecord, after: &StaticSeamRecord) -> Option<String> {
    (before.oracle_strength != after.oracle_strength)
        .then(|| format!("{} -> {}", before.oracle_strength, after.oracle_strength))
}

fn related_test_delta(before: &StaticSeamRecord, after: &StaticSeamRecord) -> isize {
    match (
        isize::try_from(after.related_tests_total),
        isize::try_from(before.related_tests_total),
    ) {
        (Ok(after_total), Ok(before_total)) => after_total - before_total,
        _ => 0,
    }
}

fn no_movement_reason(
    direction: &str,
    evidence_delta: &[String],
    evidence_source: &str,
) -> Option<String> {
    (direction == "unchanged" && evidence_delta.is_empty())
        .then(|| format!("grip class and {evidence_source} evidence were unchanged"))
}

fn optional_delta_value(value: Option<&str>) -> &str {
    match value {
        Some(text) if !text.is_empty() => text,
        _ => "missing",
    }
}
